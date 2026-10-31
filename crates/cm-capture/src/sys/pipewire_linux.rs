//! The Linux screen-cast backend: the xdg-desktop-portal ScreenCast session (ashpd) and a PipeWire
//! video stream, adapted from `spikes/spike-portal-capture` (measured in
//! `verification/L0-portal-screencast-2026-10-31-claude.md`). The OS-independent half is
//! `record::pipewire`. On other systems [`PortalPipeWire`] exists but refuses to open, so callers
//! compile everywhere.

#[cfg(target_os = "linux")]
pub use imp::PortalPipeWire;

#[cfg(not(target_os = "linux"))]
#[derive(Debug, Default)]
pub struct PortalPipeWire;

#[cfg(not(target_os = "linux"))]
impl PortalPipeWire {
    pub fn new() -> Self {
        PortalPipeWire
    }
}

/// What the desktop portal says about screen casting (for `list_devices` and the diagnostics).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PortalStatus {
    /// The ScreenCast interface answered.
    pub available: bool,
    /// It offers whole monitors (the only source type recording uses today).
    pub monitor: bool,
    /// It offers windows.
    pub window: bool,
    /// Why it is not available, or a note.
    pub detail: Option<String>,
}

/// Asks the portal which source types it offers (a D-Bus property read; never opens a session or
/// a dialog). Gives up after 3 s so a stuck portal cannot hang a device listing.
pub fn portal_status() -> PortalStatus {
    imp_status()
}

#[cfg(not(target_os = "linux"))]
fn imp_status() -> PortalStatus {
    PortalStatus {
        available: false,
        monitor: false,
        window: false,
        detail: Some("The desktop portal exists on Linux only.".into()),
    }
}

#[cfg(target_os = "linux")]
fn imp_status() -> PortalStatus {
    imp::status()
}

#[cfg(not(target_os = "linux"))]
use {
    crate::record::pipewire::{PwBackend, StreamInfo},
    crate::record::source::EventSink,
    cm_core::CmError,
    cm_encode::CpuFrame,
};

#[cfg(not(target_os = "linux"))]
impl PwBackend for PortalPipeWire {
    fn open(
        &mut self,
        _restore_token: Option<&str>,
        _show_cursor: bool,
        _sink: EventSink<CpuFrame>,
    ) -> Result<StreamInfo, CmError> {
        Err(CmError::not_implemented(
            "The PipeWire screen source exists on Linux only.",
        ))
    }

    fn close(&mut self) {}
}

#[cfg(target_os = "linux")]
mod imp {
    use std::os::fd::OwnedFd;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::{channel, RecvTimeoutError};
    use std::sync::Arc;
    use std::thread::JoinHandle;
    use std::time::Duration;

    use ashpd::desktop::screencast::{CursorMode, Screencast, SelectSourcesOptions, SourceType};
    use ashpd::desktop::PersistMode;
    use cm_core::{CmError, HostClock, SystemHostClock};
    use cm_encode::{CpuFrame, PixelFormat};
    use pipewire as pw;
    use pw::spa;
    use pw::spa::buffer::meta::MetaHeader;

    /// How long `open` waits for the portal (the first run shows a consent dialog).
    const OPEN_TIMEOUT: Duration = Duration::from_secs(180);

    use crate::record::pipewire::{
        frame_from_buffer, portal_declined, PtsMapper, PwBackend, RawBuffer, StreamInfo,
    };
    use crate::record::source::{EventSink, SourceEvent, StopReason};

    /// Portal + PipeWire. One instance per recording.
    #[derive(Default)]
    pub struct PortalPipeWire {
        stop: Arc<AtomicBool>,
        thread: Option<JoinHandle<()>>,
    }

    impl PortalPipeWire {
        pub fn new() -> Self {
            Self::default()
        }
    }

    pub fn status() -> super::PortalStatus {
        let (tx, rx) = channel();
        let spawned = std::thread::Builder::new()
            .name("cm-portal-probe".into())
            .spawn(move || {
                let r = futures_lite::future::block_on(async {
                    let proxy = Screencast::new().await?;
                    proxy.available_source_types().await
                });
                let _ = tx.send(r.map_err(|e| e.to_string()));
            });
        let failed = |detail: String| super::PortalStatus {
            available: false,
            monitor: false,
            window: false,
            detail: Some(detail),
        };
        if spawned.is_err() {
            return failed("cannot start the portal probe".into());
        }
        match rx.recv_timeout(Duration::from_secs(3)) {
            Ok(Ok(types)) => super::PortalStatus {
                available: true,
                monitor: types.contains(SourceType::Monitor),
                window: types.contains(SourceType::Window),
                detail: None,
            },
            Ok(Err(e)) => failed(format!(
                "The desktop portal has no ScreenCast service (install xdg-desktop-portal and your desktop's portal backend): {e}"
            )),
            Err(_) => failed("The desktop portal did not answer within 3 s.".into()),
        }
    }

    struct Opened {
        node: u32,
        fd: OwnedFd,
        size: (u32, u32),
        token: Option<String>,
    }

    fn map_portal_error(what: &str, e: impl std::fmt::Display) -> CmError {
        let text = e.to_string();
        if text.to_lowercase().contains("cancel") {
            portal_declined("Screen sharing was cancelled in the desktop's dialog.")
        } else {
            CmError::platform_rejected(
                format!("The desktop portal refused screen capture ({what}): {text}"),
                Some("portal_screencast".into()),
            )
        }
    }

    async fn open_portal(
        restore_token: Option<&str>,
        show_cursor: bool,
    ) -> Result<(Opened, Screencast, ashpd::desktop::Session<Screencast>), CmError> {
        let proxy = Screencast::new()
            .await
            .map_err(|e| map_portal_error("connect", e))?;
        let session = proxy
            .create_session(Default::default())
            .await
            .map_err(|e| map_portal_error("create session", e))?;
        let modes = proxy
            .available_cursor_modes()
            .await
            .map_err(|e| map_portal_error("cursor modes", e))?;
        let mode = if show_cursor && modes.contains(CursorMode::Embedded) {
            CursorMode::Embedded
        } else {
            CursorMode::Hidden
        };
        proxy
            .select_sources(
                &session,
                SelectSourcesOptions::default()
                    .set_cursor_mode(mode)
                    .set_sources(ashpd::enumflags2::BitFlags::from_flag(SourceType::Monitor))
                    .set_multiple(false)
                    .set_restore_token(restore_token)
                    .set_persist_mode(PersistMode::ExplicitlyRevoked),
            )
            .await
            .map_err(|e| map_portal_error("select sources", e))?;
        let started = proxy
            .start(&session, None, Default::default())
            .await
            .map_err(|e| map_portal_error("start", e))?
            .response()
            .map_err(|e| map_portal_error("start", e))?;
        let stream = started
            .streams()
            .first()
            .ok_or_else(|| CmError::internal("the portal returned no stream"))?
            .to_owned();
        let token = started.restore_token().map(str::to_owned);
        let fd = proxy
            .open_pipe_wire_remote(&session, Default::default())
            .await
            .map_err(|e| map_portal_error("open remote", e))?;
        let (w, h) = stream.size().unwrap_or((0, 0));
        Ok((
            Opened {
                node: stream.pipe_wire_node_id(),
                fd,
                size: (w.max(0) as u32, h.max(0) as u32),
                token,
            },
            proxy,
            session,
        ))
    }

    fn pixel_format(f: spa::param::video::VideoFormat) -> Option<PixelFormat> {
        use spa::param::video::VideoFormat as V;
        if f == V::BGRx {
            Some(PixelFormat::Bgrx)
        } else if f == V::BGRA {
            Some(PixelFormat::Bgra)
        } else if f == V::RGBx {
            Some(PixelFormat::Rgbx)
        } else if f == V::RGBA {
            Some(PixelFormat::Rgba)
        } else if f == V::NV12 {
            Some(PixelFormat::Nv12)
        } else {
            None
        }
    }

    fn ser(v: spa::pod::Value) -> Result<Vec<u8>, CmError> {
        spa::pod::serialize::PodSerializer::serialize(std::io::Cursor::new(Vec::new()), &v)
            .map(|(c, _)| c.into_inner())
            .map_err(|e| CmError::internal(format!("pod: {e:?}")))
    }

    fn format_pod() -> Result<Vec<u8>, CmError> {
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
        ser(spa::pod::Value::Object(fmt))
    }

    /// Buffers in CPU memory, plus the header meta that carries the presentation timestamp (a
    /// consumer has to ask for it: L0, "What had to be done").
    fn param_pods() -> Result<Vec<Vec<u8>>, CmError> {
        use spa::pod::{Object, Property, PropertyFlags, Value};
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
        let prop = |key: u32, value: Value| Property {
            key,
            flags: PropertyFlags::empty(),
            value,
        };
        let buffers = Object {
            type_: spa::sys::SPA_TYPE_OBJECT_ParamBuffers,
            id: spa::sys::SPA_PARAM_Buffers,
            properties: vec![
                prop(spa::sys::SPA_PARAM_BUFFERS_buffers, range(8, 2, 16)),
                prop(spa::sys::SPA_PARAM_BUFFERS_blocks, Value::Int(1)),
                prop(
                    spa::sys::SPA_PARAM_BUFFERS_dataType,
                    Value::Choice(spa::pod::ChoiceValue::Int(spa::utils::Choice(
                        spa::utils::ChoiceFlags::empty(),
                        spa::utils::ChoiceEnum::Flags {
                            default: (1 << spa::sys::SPA_DATA_MemPtr)
                                | (1 << spa::sys::SPA_DATA_MemFd),
                            flags: vec![
                                (1 << spa::sys::SPA_DATA_MemPtr) | (1 << spa::sys::SPA_DATA_MemFd),
                            ],
                        },
                    ))),
                ),
            ],
        };
        let header = Object {
            type_: spa::sys::SPA_TYPE_OBJECT_ParamMeta,
            id: spa::sys::SPA_PARAM_Meta,
            properties: vec![
                prop(
                    spa::sys::SPA_PARAM_META_type,
                    Value::Id(spa::utils::Id(spa::sys::SPA_META_Header)),
                ),
                prop(
                    spa::sys::SPA_PARAM_META_size,
                    Value::Int(
                        i32::try_from(std::mem::size_of::<spa::sys::spa_meta_header>())
                            .unwrap_or(32),
                    ),
                ),
            ],
        };
        Ok(vec![
            ser(Value::Object(buffers))?,
            ser(Value::Object(header))?,
        ])
    }

    type SharedSink = Arc<std::sync::Mutex<EventSink<CpuFrame>>>;

    fn emit(sink: &SharedSink, ev: SourceEvent<CpuFrame>) {
        if let Ok(mut g) = sink.lock() {
            (*g)(ev);
        }
    }

    struct Fmt {
        info: spa::param::video::VideoInfoRaw,
        mapper: PtsMapper,
    }

    /// Runs the PipeWire loop until `stop` is set or the stream fails.
    fn stream_loop(
        node: u32,
        fd: OwnedFd,
        sink: SharedSink,
        stop: Arc<AtomicBool>,
    ) -> Result<(), CmError> {
        let pw_err =
            |what: &str, e: &dyn std::fmt::Display| CmError::internal(format!("{what}: {e}"));
        pw::init();
        let mainloop =
            pw::main_loop::MainLoopRc::new(None).map_err(|e| pw_err("PipeWire loop", &e))?;
        let context = pw::context::ContextRc::new(&mainloop, None)
            .map_err(|e| pw_err("PipeWire context", &e))?;
        let core = context
            .connect_fd_rc(fd, None)
            .map_err(|e| pw_err("PipeWire connect", &e))?;
        let stream = pw::stream::StreamBox::new(
            &core,
            "capturemann-screen",
            pw::properties::properties! {
                *pw::keys::MEDIA_TYPE => "Video",
                *pw::keys::MEDIA_CATEGORY => "Capture",
                *pw::keys::MEDIA_ROLE => "Screen",
            },
        )
        .map_err(|e| pw_err("PipeWire stream", &e))?;
        let params = param_pods()?;
        let clock = SystemHostClock;
        let ml_state = mainloop.clone();
        let ml_proc = mainloop.clone();
        let stop_proc = Arc::clone(&stop);
        let sink_state = Arc::new(std::sync::Mutex::new(None::<String>));
        let sink_state2 = Arc::clone(&sink_state);
        let _listener = stream
            .add_local_listener_with_user_data(Fmt {
                info: Default::default(),
                mapper: PtsMapper::default(),
            })
            .state_changed(move |_, _, _old, new| {
                if let pw::stream::StreamState::Error(m) = new {
                    if let Ok(mut g) = sink_state2.lock() {
                        *g = Some(m);
                    }
                    ml_state.quit();
                }
            })
            .param_changed(move |stream, fmt, id, param| {
                let Some(param) = param else { return };
                if id != spa::param::ParamType::Format.as_raw() {
                    return;
                }
                if fmt.info.parse(param).is_ok() {
                    let mut pods: Vec<&spa::pod::Pod> = params
                        .iter()
                        .filter_map(|m| spa::pod::Pod::from_bytes(m))
                        .collect();
                    if let Err(e) = stream.update_params(&mut pods) {
                        log::warn!("PipeWire update_params failed: {e}");
                    }
                }
            })
            .process(move |stream, fmt| {
                if stop_proc.load(Ordering::Relaxed) {
                    ml_proc.quit();
                    return;
                }
                let arrival = clock.now_ns();
                let Some(mut buffer) = stream.dequeue_buffer() else {
                    return;
                };
                let pts = buffer.find_meta::<MetaHeader>().map(|h| h.pts());
                let Some(d) = buffer.datas_mut().first_mut() else {
                    return;
                };
                let (size, stride) = (d.chunk().size() as usize, d.chunk().stride());
                let Some(data) = d.data() else { return };
                let Some(format) = pixel_format(fmt.info.format()) else {
                    return;
                };
                let data = &data[..size.min(data.len())];
                let raw = RawBuffer {
                    width: fmt.info.size().width,
                    height: fmt.info.size().height,
                    stride: usize::try_from(stride).unwrap_or(0),
                    format,
                    data,
                    pts_ns: pts,
                    arrival_ns: arrival,
                };
                match frame_from_buffer(&raw, &mut fmt.mapper) {
                    Ok(Some((host_ns, frame))) => {
                        emit(&sink, SourceEvent::Frame { host_ns, frame })
                    }
                    Ok(None) => {}
                    Err(e) => log::warn!("dropped a screen buffer: {e}"),
                }
            })
            .register()
            .map_err(|e| pw_err("PipeWire listener", &e))?;
        let fmt = format_pod()?;
        let mut pods: Vec<&spa::pod::Pod> =
            vec![spa::pod::Pod::from_bytes(&fmt).ok_or_else(|| CmError::internal("format pod"))?];
        stream
            .connect(
                spa::utils::Direction::Input,
                Some(node),
                pw::stream::StreamFlags::AUTOCONNECT | pw::stream::StreamFlags::MAP_BUFFERS,
                &mut pods,
            )
            .map_err(|e| pw_err("PipeWire stream connect", &e))?;
        // The stop flag is also checked here: a still screen sends no buffers to `process`.
        let (ml_t, stop_t) = (mainloop.clone(), Arc::clone(&stop));
        let timer = mainloop.loop_().add_timer(move |_| {
            if stop_t.load(Ordering::Relaxed) {
                ml_t.quit();
            }
        });
        timer.update_timer(
            Some(Duration::from_millis(50)),
            Some(Duration::from_millis(50)),
        );
        mainloop.run();
        drop(_listener);
        let failure = sink_state.lock().ok().and_then(|mut g| g.take());
        match failure {
            Some(m) => Err(CmError::internal(m)),
            None => Ok(()),
        }
    }

    impl PwBackend for PortalPipeWire {
        fn open(
            &mut self,
            restore_token: Option<&str>,
            show_cursor: bool,
            sink: EventSink<CpuFrame>,
        ) -> Result<StreamInfo, CmError> {
            if self.thread.is_some() {
                return Err(CmError::internal("the PipeWire source is already open"));
            }
            self.stop.store(false, Ordering::Relaxed);
            let stop = Arc::clone(&self.stop);
            let token = restore_token.map(str::to_owned);
            let (tx, rx) = channel::<Result<StreamInfo, CmError>>();
            let sink: SharedSink = Arc::new(std::sync::Mutex::new(sink));
            let thread = std::thread::Builder::new()
                .name("cm-pw-screen".into())
                .spawn(move || {
                    let opened =
                        futures_lite::future::block_on(open_portal(token.as_deref(), show_cursor));
                    let (o, _proxy, session) = match opened {
                        Ok(v) => v,
                        Err(e) => {
                            let _ = tx.send(Err(e));
                            return;
                        }
                    };
                    let _ = tx.send(Ok(StreamInfo {
                        width: o.size.0,
                        height: o.size.1,
                        restore_token: o.token.clone(),
                    }));
                    let result = stream_loop(o.node, o.fd, Arc::clone(&sink), Arc::clone(&stop));
                    // A failure nobody asked for ends the recording's screen track.
                    if let Err(e) = &result {
                        if !stop.load(Ordering::Relaxed) {
                            emit(
                                &sink,
                                SourceEvent::Stopped(StopReason::Error(e.to_string())),
                            );
                        }
                    }
                    let _ = futures_lite::future::block_on(session.close());
                    if let Err(e) = result {
                        log::warn!("PipeWire screen stream ended: {e}");
                    }
                })
                .map_err(|e| CmError::internal(format!("cannot start the capture thread: {e}")))?;
            self.thread = Some(thread);
            match rx.recv_timeout(OPEN_TIMEOUT) {
                Ok(Ok(info)) => Ok(info),
                Ok(Err(e)) => {
                    self.close();
                    Err(e)
                }
                Err(RecvTimeoutError::Timeout) => {
                    self.close();
                    Err(CmError::internal(
                        "The desktop did not answer the screen-share request in time.",
                    ))
                }
                Err(RecvTimeoutError::Disconnected) => {
                    self.close();
                    Err(CmError::internal("the capture thread ended early"))
                }
            }
        }

        fn close(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            if let Some(t) = self.thread.take() {
                let _ = t.join();
            }
        }
    }

    impl Drop for PortalPipeWire {
        fn drop(&mut self) {
            self.close();
        }
    }
}
