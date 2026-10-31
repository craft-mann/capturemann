//! L0 spike (Linux, platform plan 0.4): can the xdg-desktop-portal ScreenCast portal, PipeWire and the
//! `ashpd` and `pipewire` crates deliver screen frames the way the recording pipeline needs them?
//!
//! ```text
//! cargo run --release -p spike-portal-capture -- [--secs 10] [--token-file token.txt] [--no-cursor-meta]
//! ```
//!
//! It opens a monitor ScreenCast session (cursor mode "metadata"), connects a PipeWire stream, counts
//! frames for `--secs` and prints one JSON line: frame count and rate, the gaps between frames, the
//! time to the first frame, the negotiated format, the PipeWire presentation timestamps (header
//! meta) against the local monotonic clock, and how many frames carried cursor metadata. With
//! `--token-file` the portal's restore token is saved and used next time, so the consent dialog
//! appears only once. Nothing is written but that file and the JSON line; no frame is stored.

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("this spike runs on Linux only");
}

#[cfg(target_os = "linux")]
fn main() -> anyhow::Result<()> {
    linux::run()
}

#[cfg(target_os = "linux")]
mod linux {
    use std::cell::RefCell;
    use std::os::fd::OwnedFd;
    use std::rc::Rc;
    use std::time::{Duration, Instant};

    use anyhow::{Context, Result};
    use ashpd::desktop::screencast::{CursorMode, Screencast, SelectSourcesOptions, SourceType};
    use ashpd::desktop::PersistMode;
    use pipewire as pw;
    use pw::spa;
    use pw::spa::buffer::meta::{MetaCursor, MetaHeader};

    #[derive(Default)]
    struct Stats {
        started: Option<Instant>,
        first_frame: Option<Duration>,
        last_arrival: Option<Instant>,
        gaps_ms: Vec<f64>,
        frames: u64,
        empty: u64,
        pts: Vec<(i64, f64)>,
        cursor_frames: u64,
        cursor_present: u64,
        header_present: u64,
        cursor_positions: Vec<(i32, i32)>,
        cursor_bitmaps: u64,
        format: String,
        size: (u32, u32),
        fps_nominal: (u32, u32),
        bytes: u64,
    }

    struct Args {
        secs: u64,
        token_file: Option<String>,
        cursor_meta: bool,
        node: Option<u32>,
    }

    fn args() -> Args {
        let v: Vec<String> = std::env::args().skip(1).collect();
        let get = |f: &str| {
            v.iter()
                .position(|a| a == f)
                .and_then(|i| v.get(i + 1).cloned())
        };
        Args {
            secs: get("--secs").and_then(|s| s.parse().ok()).unwrap_or(10),
            token_file: get("--token-file"),
            cursor_meta: !v.iter().any(|a| a == "--no-cursor-meta"),
            node: get("--node").and_then(|s| s.parse().ok()),
        }
    }

    async fn open_portal(a: &Args) -> Result<(u32, OwnedFd, (i32, i32), Option<String>, String)> {
        let proxy = Screencast::new().await?;
        let version = proxy.version();
        let modes = proxy.available_cursor_modes().await?;
        let sources = proxy.available_source_types().await?;
        let session = proxy.create_session(Default::default()).await?;
        let token = a
            .token_file
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty());
        let mode = if a.cursor_meta && modes.contains(CursorMode::Metadata) {
            CursorMode::Metadata
        } else {
            CursorMode::Embedded
        };
        proxy
            .select_sources(
                &session,
                SelectSourcesOptions::default()
                    .set_cursor_mode(mode)
                    .set_sources(ashpd::enumflags2::BitFlags::from_flag(SourceType::Monitor))
                    .set_multiple(false)
                    .set_restore_token(token.as_deref())
                    .set_persist_mode(PersistMode::ExplicitlyRevoked),
            )
            .await?;
        let started = proxy
            .start(&session, None, Default::default())
            .await?
            .response()?;
        let stream = started
            .streams()
            .first()
            .context("no stream selected")?
            .to_owned();
        let new_token = started.restore_token().map(str::to_owned);
        let fd = proxy
            .open_pipe_wire_remote(&session, Default::default())
            .await?;
        let info = format!(
            "portal ScreenCast v{version}; cursor modes {modes:?}; source types {sources:?}; \
             cursor mode used {mode:?}; restore token {}",
            if token.is_some() {
                "reused"
            } else {
                "none yet"
            }
        );
        Ok((
            stream.pipe_wire_node_id(),
            fd,
            stream.size().unwrap_or((0, 0)),
            new_token,
            info,
        ))
    }

    fn build_params() -> Result<Vec<Vec<u8>>> {
        use spa::pod::Pod;
        use spa::pod::Value;
        let fmt = spa::pod::object!(
            spa::utils::SpaTypes::ObjectParamFormat,
            spa::param::ParamType::EnumFormat,
            spa::pod::property!(
                spa::param::format::FormatProperties::MediaType,
                Id,
                spa::param::format::MediaType::Video
            ),
            spa::pod::property!(
                spa::param::format::FormatProperties::MediaSubtype,
                Id,
                spa::param::format::MediaSubtype::Raw
            ),
            spa::pod::property!(
                spa::param::format::FormatProperties::VideoFormat,
                Choice,
                Enum,
                Id,
                spa::param::video::VideoFormat::BGRx,
                spa::param::video::VideoFormat::BGRx,
                spa::param::video::VideoFormat::BGRA,
                spa::param::video::VideoFormat::RGBx,
                spa::param::video::VideoFormat::RGBA,
                spa::param::video::VideoFormat::NV12,
            ),
            spa::pod::property!(
                spa::param::format::FormatProperties::VideoSize,
                Choice,
                Range,
                Rectangle,
                spa::utils::Rectangle {
                    width: 1280,
                    height: 720
                },
                spa::utils::Rectangle {
                    width: 1,
                    height: 1
                },
                spa::utils::Rectangle {
                    width: 8192,
                    height: 8192
                }
            ),
            spa::pod::property!(
                spa::param::format::FormatProperties::VideoFramerate,
                Choice,
                Range,
                Fraction,
                spa::utils::Fraction { num: 60, denom: 1 },
                spa::utils::Fraction { num: 0, denom: 1 },
                spa::utils::Fraction { num: 240, denom: 1 }
            ),
        );
        let out = vec![ser(Value::Object(fmt))?];
        let _ = Pod::from_bytes(&out[0]).context("format pod")?;
        Ok(out)
    }

    fn ser(v: spa::pod::Value) -> Result<Vec<u8>> {
        Ok(
            spa::pod::serialize::PodSerializer::serialize(std::io::Cursor::new(Vec::new()), &v)
                .map_err(|e| anyhow::anyhow!("pod: {e:?}"))?
                .0
                .into_inner(),
        )
    }

    /// The buffer metadata a consumer asks for once the format is settled: the presentation
    /// timestamp and, when wanted, the cursor (position, hotspot, a bitmap up to 64x64).
    fn build_metas(cursor_meta: bool) -> Result<Vec<Vec<u8>>> {
        use spa::pod::{Object, Property, PropertyFlags, Value};
        let mut out = Vec::new();
        // Buffers: sent together with the metas so the stream allocates buffers that carry them.
        let range = |d: i32, lo: i32, hi: i32| {
            Value::Choice(spa::pod::ChoiceValue::Int(spa::utils::Choice(
                spa::utils::ChoiceFlags::empty(),
                spa::utils::ChoiceEnum::Range {
                    default: d,
                    min: lo,
                    max: hi,
                },
            )))
        };
        let buffers = Object {
            type_: spa::sys::SPA_TYPE_OBJECT_ParamBuffers,
            id: spa::sys::SPA_PARAM_Buffers,
            properties: vec![
                Property {
                    key: spa::sys::SPA_PARAM_BUFFERS_buffers,
                    flags: PropertyFlags::empty(),
                    value: range(8, 2, 16),
                },
                Property {
                    key: spa::sys::SPA_PARAM_BUFFERS_blocks,
                    flags: PropertyFlags::empty(),
                    value: Value::Int(1),
                },
                Property {
                    key: spa::sys::SPA_PARAM_BUFFERS_dataType,
                    flags: PropertyFlags::empty(),
                    value: Value::Choice(spa::pod::ChoiceValue::Int(spa::utils::Choice(
                        spa::utils::ChoiceFlags::empty(),
                        spa::utils::ChoiceEnum::Flags {
                            default: (1 << spa::sys::SPA_DATA_MemPtr)
                                | (1 << spa::sys::SPA_DATA_MemFd),
                            flags: vec![
                                (1 << spa::sys::SPA_DATA_MemPtr) | (1 << spa::sys::SPA_DATA_MemFd),
                            ],
                        },
                    ))),
                },
            ],
        };
        out.push(ser(Value::Object(buffers))?);
        // The header meta carries the presentation timestamp; a consumer has to ask for it.
        let header = Object {
            type_: spa::sys::SPA_TYPE_OBJECT_ParamMeta,
            id: spa::sys::SPA_PARAM_Meta,
            properties: vec![
                Property {
                    key: spa::sys::SPA_PARAM_META_type,
                    flags: PropertyFlags::empty(),
                    value: Value::Id(spa::utils::Id(spa::sys::SPA_META_Header)),
                },
                Property {
                    key: spa::sys::SPA_PARAM_META_size,
                    flags: PropertyFlags::empty(),
                    value: Value::Int(i32::try_from(
                        std::mem::size_of::<spa::sys::spa_meta_header>(),
                    )?),
                },
            ],
        };
        out.push(ser(Value::Object(header))?);
        if cursor_meta {
            // Ask for the cursor as buffer metadata: position, hotspot and a bitmap of up to 64x64 BGRA.
            let base = std::mem::size_of::<spa::sys::spa_meta_cursor>()
                + std::mem::size_of::<spa::sys::spa_meta_bitmap>();
            // Like other screen-cast consumers: 64x64 wanted, from 1x1 up to 1024x1024.
            let (size, size_min, size_max) = (base + 64 * 64 * 4, base + 4, base + 1024 * 1024 * 4);
            let meta = Object {
                type_: spa::sys::SPA_TYPE_OBJECT_ParamMeta,
                id: spa::sys::SPA_PARAM_Meta,
                properties: vec![
                    Property {
                        key: spa::sys::SPA_PARAM_META_type,
                        flags: PropertyFlags::empty(),
                        value: Value::Id(spa::utils::Id(spa::sys::SPA_META_Cursor)),
                    },
                    Property {
                        key: spa::sys::SPA_PARAM_META_size,
                        flags: PropertyFlags::empty(),
                        value: range(
                            i32::try_from(size)?,
                            i32::try_from(size_min)?,
                            i32::try_from(size_max)?,
                        ),
                    },
                ],
            };
            out.push(ser(Value::Object(meta))?);
        }
        Ok(out)
    }

    fn stream_frames(node: u32, fd: Option<OwnedFd>, a: &Args) -> Result<Stats> {
        pw::init();
        let mainloop = pw::main_loop::MainLoopRc::new(None)?;
        let context = pw::context::ContextRc::new(&mainloop, None)?;
        let core = match fd {
            Some(fd) => context.connect_fd_rc(fd, None)?,
            None => context.connect_rc(None)?,
        };
        let stream = pw::stream::StreamBox::new(
            &core,
            "capturemann-spike",
            pw::properties::properties! {
                *pw::keys::MEDIA_TYPE => "Video",
                *pw::keys::MEDIA_CATEGORY => "Capture",
                *pw::keys::MEDIA_ROLE => "Screen",
            },
        )?;
        let stats = Rc::new(RefCell::new(Stats {
            started: Some(Instant::now()),
            ..Stats::default()
        }));
        let deadline = Duration::from_secs(a.secs);
        let (s_fmt, s_proc, ml) = (stats.clone(), stats.clone(), mainloop.clone());
        let metas = build_metas(a.cursor_meta)?;
        let _listener = stream
            .add_local_listener_with_user_data(spa::param::video::VideoInfoRaw::default())
            .param_changed(move |stream, info, id, param| {
                let Some(param) = param else { return };
                if id != spa::param::ParamType::Format.as_raw() {
                    return;
                }
                if info.parse(param).is_ok() {
                    let mut s = s_fmt.borrow_mut();
                    s.format = format!("{:?}", info.format());
                    s.size = (info.size().width, info.size().height);
                    s.fps_nominal = (info.framerate().num, info.framerate().denom);
                    drop(s);
                    let mut pods: Vec<&spa::pod::Pod> = metas
                        .iter()
                        .filter_map(|m| spa::pod::Pod::from_bytes(m))
                        .collect();
                    if let Err(e) = stream.update_params(&mut pods) {
                        eprintln!("update_params failed: {e}");
                    }
                }
            })
            .process(move |stream, _| {
                let now = Instant::now();
                let mut s = s_proc.borrow_mut();
                let started = s.started.unwrap_or(now);
                let Some(mut buffer) = stream.dequeue_buffer() else {
                    return;
                };
                let (size, has_chunk) = {
                    let datas = buffer.datas_mut();
                    match datas.first_mut() {
                        Some(d) => (d.chunk().size(), true),
                        None => (0, false),
                    }
                };
                if !has_chunk || size == 0 {
                    // A buffer with no pixels: the compositor sends one for a cursor-only update.
                    s.empty += 1;
                }
                if let Some(h) = buffer.find_meta::<MetaHeader>() {
                    s.header_present += 1;
                    let t = now.duration_since(started).as_secs_f64() * 1000.0;
                    if s.pts.len() < 4096 {
                        s.pts.push((h.pts(), t));
                    }
                }
                if let Some(c) = buffer.find_meta::<MetaCursor>() {
                    s.cursor_present += 1;
                    if c.is_valid() {
                        s.cursor_frames += 1;
                        if s.cursor_positions.len() < 64 {
                            let p = c.position();
                            s.cursor_positions.push((p.x, p.y));
                        }
                        if c.bitmap().is_some() {
                            s.cursor_bitmaps += 1;
                        }
                    }
                }
                if size > 0 {
                    s.frames += 1;
                    s.bytes += u64::from(size);
                    if s.first_frame.is_none() {
                        s.first_frame = Some(now.duration_since(started));
                    }
                    if let Some(prev) = s.last_arrival {
                        s.gaps_ms
                            .push(now.duration_since(prev).as_secs_f64() * 1000.0);
                    }
                    s.last_arrival = Some(now);
                }
                if now.duration_since(started) >= deadline {
                    ml.quit();
                }
            })
            .register()?;
        let params = build_params()?;
        let mut pods: Vec<&spa::pod::Pod> = params
            .iter()
            .map(|p| spa::pod::Pod::from_bytes(p).context("pod bytes"))
            .collect::<Result<_>>()?;
        stream.connect(
            spa::utils::Direction::Input,
            Some(node),
            pw::stream::StreamFlags::AUTOCONNECT | pw::stream::StreamFlags::MAP_BUFFERS,
            &mut pods,
        )?;
        // Also stop when nothing arrives at all (a static screen sends no frames in damage mode).
        let ml2 = mainloop.clone();
        let timer = mainloop.loop_().add_timer(move |_| ml2.quit());
        timer.update_timer(Some(Duration::from_secs(a.secs + 5)), None);
        stats.borrow_mut().started = Some(Instant::now());
        mainloop.run();
        drop(_listener);
        let out = Rc::try_unwrap(stats).map_err(|_| anyhow::anyhow!("stats still shared"))?;
        Ok(out.into_inner())
    }

    fn percentile(v: &mut [f64], p: f64) -> f64 {
        if v.is_empty() {
            return 0.0;
        }
        v.sort_by(|a, b| a.total_cmp(b));
        v[(((v.len() - 1) as f64) * p).round() as usize]
    }

    pub fn run() -> Result<()> {
        let a = args();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let (node, fd, portal_size, token, info) = match a.node {
            // A node of the default PipeWire instance, for checking a compositor without the portal.
            Some(n) => (n, None, (0, 0), None, "direct node, no portal".to_owned()),
            None => {
                let (n, fd, size, token, info) = rt.block_on(open_portal(&a))?;
                (n, Some(fd), size, token, info)
            }
        };
        if let (Some(t), Some(p)) = (&token, &a.token_file) {
            std::fs::write(p, t)?;
        }
        let mut s = stream_frames(node, fd, &a)?;
        let secs = s.gaps_ms.iter().sum::<f64>().max(1.0) / 1000.0;
        let mut gaps = std::mem::take(&mut s.gaps_ms);
        let (p50, p95, p99, max) = (
            percentile(&mut gaps, 0.5),
            percentile(&mut gaps, 0.95),
            percentile(&mut gaps, 0.99),
            gaps.last().copied().unwrap_or(0.0),
        );
        // PTS (nanoseconds on PipeWire's clock) against local arrival time: a constant offset means the
        // timestamps are usable for sync; the spread of (pts delta - arrival delta) is the jitter.
        let spread = if s.pts.len() > 2 {
            let (p0, t0) = s.pts[0];
            let mut d: Vec<f64> = s
                .pts
                .iter()
                .map(|(p, t)| (*p - p0) as f64 / 1e6 - (t - t0))
                .collect();
            percentile(&mut d, 0.99) - percentile(&mut d, 0.01)
        } else {
            0.0
        };
        println!(
            "{}",
            serde_like(&[
                ("portal", format!("\"{info}\"")),
                ("node", node.to_string()),
                (
                    "portal_size",
                    format!("[{},{}]", portal_size.0, portal_size.1)
                ),
                ("format", format!("\"{}\"", s.format)),
                ("size", format!("[{},{}]", s.size.0, s.size.1)),
                (
                    "nominal_fps",
                    format!("\"{}/{}\"", s.fps_nominal.0, s.fps_nominal.1)
                ),
                ("frames", s.frames.to_string()),
                ("empty_buffers", s.empty.to_string()),
                ("seconds", format!("{secs:.2}")),
                ("fps", format!("{:.1}", s.frames as f64 / secs)),
                (
                    "first_frame_ms",
                    format!(
                        "{:.0}",
                        s.first_frame.map_or(-1.0, |d| d.as_secs_f64() * 1000.0)
                    )
                ),
                (
                    "gap_ms_p50_p95_p99_max",
                    format!("[{p50:.1},{p95:.1},{p99:.1},{max:.1}]")
                ),
                ("pts_vs_arrival_spread_ms", format!("{spread:.1}")),
                ("mb_received", format!("{:.1}", s.bytes as f64 / 1e6)),
                ("header_meta_frames", s.header_present.to_string()),
                ("cursor_meta_present", s.cursor_present.to_string()),
                ("cursor_meta_valid", s.cursor_frames.to_string()),
                ("cursor_bitmaps", s.cursor_bitmaps.to_string()),
                (
                    "cursor_positions",
                    format!(
                        "{:?}",
                        &s.cursor_positions[..s.cursor_positions.len().min(8)]
                    )
                ),
            ])
        );
        Ok(())
    }

    fn serde_like(fields: &[(&str, String)]) -> String {
        let body: Vec<String> = fields.iter().map(|(k, v)| format!("\"{k}\":{v}")).collect();
        format!("{{{}}}", body.join(","))
    }
}
