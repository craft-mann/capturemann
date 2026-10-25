//! The edited export (EXP-1): honours the edit list (trim, cuts, speed, background, padding,
//! corner radius, shadow) by composing every output frame on the GPU.
//!
//! ```text
//! screen.mp4 ─ ffmpeg (seek, setpts, fps, nv12) ─┐  one decoder process per kept piece,
//! camera.mp4 ─ ffmpeg (… crop, bubble size) ─────┤  read by a pump thread per track
//!                                                ▼
//!                  cm_render::Compositor (background, padding, radius, bubble) ── RGBA
//!                                                ▼ pipe:0
//! mic / system ──────────────► ffmpeg (hardware encoder, audio cut/stretch/mix, mux) ─► .mp4
//! ```
//!
//! Decode and encode stay in FFmpeg child processes (the same LGPL rules as the quick export, see
//! the crate docs); only the picture is made in this process. The piece/segment and audio-graph
//! math is in [`crate::edit_plan`] and unit tested without FFmpeg or a GPU.
//!
//! The quick export hands a recording to this path when its edit list changes the picture
//! (`is_trivial` is false) or when [`ExportPath::Compositor`] forces it.

use std::fs;
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{sync_channel, Receiver};
use std::time::Instant;

use cm_core::{
    BackgroundKind, CmError, EditContext, EditList, MediaTime, PauseGap, Timeline, TICKS_PER_SECOND,
};
use cm_render::compose::{
    plan_blend, Background, CameraLayer, Compositor, Frame, PixelFormat, Scene,
};
use serde_json::{json, Value};

use crate::captions::{caption_layer_at, CaptionCue};
use crate::dsp_audio::{decode_pcm, f32le_bytes, DspAudio};
use crate::edit_plan::{
    audio_graph, frame_count, secs_of, segment_filters, video_segments, with_pauses_cut, AudioKind,
    AudioProcessor, AudioSource, MixInput, MixedPcm, PcmTrack, Segment,
};
use crate::edit_view::{CursorMedia, EditView, ViewParams};
use crate::graph::{
    output_size, rate, secs, video_codec_at, AUDIO_BPS_MONO, AUDIO_BPS_STEREO, AUDIO_RATE,
};
use crate::layout_plan::{geom_of, LayoutPlanner, Placement};
use crate::preset::{self, fit_rect, Kind, Target};
use crate::quick::{
    io_err, master_size, output_name_for, remove_replaced, sweep_stale, Cleanup, ExportError,
    QuickOutcome, QuickRequest, EXPORTS_DIR,
};
use crate::run::{run_ffmpeg, run_ffmpeg_fed, Feeder, RunError, BROKEN_PIPE};
use crate::spec::QuickSpec;
use crate::tools::{
    encoder_candidates, is_encoder_init_failure, probe_caps, probe_file, Encoder, Tools,
};

/// Which renderer an export uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExportPath {
    /// The compositor when the edit list changes the picture, else the FFmpeg graph.
    #[default]
    Auto,
    /// Always the FFmpeg filter graph (ignores the edit list).
    Ffmpeg,
    /// Always the compositor (tests).
    Compositor,
}

/// Direction of a gradient background, degrees clockwise from "up" (top left to bottom right).
const GRADIENT_ANGLE: f32 = 135.0;
/// Widest camera picture decoded for layout ranges (camera full, side by side).
const WIDE_CAMERA_MAX_W: u32 = 1920;
/// Frames a pump may run ahead of the compositor.
const PREFETCH: usize = 4;

/// What the manifest says about the edit list, resolved against the recording.
pub struct Resolved {
    pub list: EditList,
    pub gaps: Vec<PauseGap>,
    /// The recorder's original entry (no `version`), or a list that only restates the defaults.
    pub trivial: bool,
}

/// Reads `manifest.edits[0]` (migrating the recorder's original entry) and decides whether the
/// FFmpeg graph can render it unchanged.
pub fn resolve_edits(manifest: &Value, spec: &QuickSpec) -> Resolved {
    let gaps: Vec<PauseGap> = spec
        .gaps
        .iter()
        .map(|&(from, to)| PauseGap {
            from: MediaTime::from_ticks(from),
            to: MediaTime::from_ticks(to),
        })
        .collect();
    let end = [
        Some(spec.screen.end),
        spec.camera.as_ref().map(|c| c.end),
        spec.mic.as_ref().map(|a| a.end),
        spec.system.as_ref().map(|a| a.end),
    ]
    .into_iter()
    .flatten()
    .max()
    .unwrap_or(0);
    let cx = EditContext {
        end: MediaTime::from_ticks(end),
        live_at: manifest
            .get("live_at")
            .and_then(Value::as_i64)
            .map(MediaTime::from_ticks),
        gaps: &gaps,
    };
    let entry = manifest.get("edits").and_then(|e| e.get(0));
    let full = EditList::full(&cx);
    let list = entry.map_or_else(|| full.clone(), |e| EditList::from_json(e, &cx));
    let versioned = entry.is_some_and(|e| e.get("version").is_some());
    let trivial = !spec.screen.cursor_separate
        && (!versioned
            || (list.start == full.start
                && list.end == full.end
                && list.cuts.is_empty()
                && list.speeds.is_empty()
                && (list.base_speed - 1.0).abs() < 1e-9
                && list.background.kind == BackgroundKind::None
                && list.background.padding_pct == 0.0
                && list.background.radius_px == 0
                && !list.background.shadow
                && list.camera_segments().is_empty()
                && list.view.is_default()));
    Resolved {
        list,
        gaps,
        trivial,
    }
}

// ---- background ---------------------------------------------------------------------------------

enum BgOwned {
    None,
    Color([u8; 3]),
    Gradient([u8; 3], [u8; 3]),
    Image(image::RgbaImage),
}

impl BgOwned {
    fn as_scene(&self) -> Background<'_> {
        match self {
            BgOwned::None => Background::None,
            BgOwned::Color(c) => Background::Color(*c),
            BgOwned::Gradient(a, b) => Background::Gradient {
                from: *a,
                to: *b,
                angle_deg: GRADIENT_ANGLE,
            },
            BgOwned::Image(img) => {
                Background::Image(Frame::rgba(img.width(), img.height(), img.as_raw()))
            }
        }
    }
}

fn parse_hex(v: &Value) -> Result<[u8; 3], CmError> {
    let s = v.as_str().unwrap_or("").trim_start_matches('#');
    let bad = || CmError::invalid_input(format!("the background color `{v}` is not #rrggbb"));
    if s.len() != 6 {
        return Err(bad());
    }
    let n = u32::from_str_radix(s, 16).map_err(|_| bad())?;
    Ok([(n >> 16) as u8, (n >> 8) as u8, n as u8])
}

fn load_background(list: &EditList, bundle: &Path) -> Result<BgOwned, CmError> {
    let b = &list.background;
    Ok(match b.kind {
        BackgroundKind::None => BgOwned::None,
        BackgroundKind::Color => BgOwned::Color(parse_hex(&b.value)?),
        BackgroundKind::Gradient => {
            let stops =
                b.value.as_array().filter(|a| a.len() == 2).ok_or_else(|| {
                    CmError::invalid_input("a gradient background needs two colors")
                })?;
            BgOwned::Gradient(parse_hex(&stops[0])?, parse_hex(&stops[1])?)
        }
        BackgroundKind::Image => {
            let name = b
                .value
                .as_str()
                .ok_or_else(|| CmError::invalid_input("an image background needs a file path"))?;
            let given = PathBuf::from(name);
            let path = if given.is_absolute() {
                given
            } else {
                bundle.join(given)
            };
            let img = image::open(&path).map_err(|e| {
                CmError::invalid_input(format!(
                    "the background image `{}` cannot be read (PNG or JPEG): {e}",
                    path.display()
                ))
            })?;
            BgOwned::Image(img.to_rgba8())
        }
    })
}

// ---- decoder pumps ----------------------------------------------------------------------------

type PumpMsg = Result<Vec<u8>, CmError>;

/// What a pump reads: one track file turned into `size` NV12 frames, segment by segment.
struct PumpJob {
    ffmpeg: PathBuf,
    file: PathBuf,
    segments: Vec<Segment>,
    fps: f64,
    size: (u32, u32),
    /// Filters between `fps` and `scale` (the camera's crop and mirror-free aspect).
    pre: Vec<String>,
}

fn decoder_args(job: &PumpJob, seg: &Segment) -> Vec<String> {
    let filters = segment_filters(seg, job.fps, job.size, &job.pre, "nv12");
    let mut a: Vec<String> = ["-hide_banner", "-nostdin", "-loglevel", "error"]
        .map(String::from)
        .to_vec();
    if seg.seek_s > 0.0005 {
        a.extend(["-ss".into(), secs(seg.seek_s)]);
    }
    a.extend(["-i".into(), job.file.to_string_lossy().into_owned()]);
    a.extend(["-an".into(), "-sn".into(), "-vf".into(), filters]);
    a.extend(["-frames:v".into(), seg.frames.to_string()]);
    a.extend(["-f".into(), "rawvideo".into(), "-".into()]);
    a
}

fn spawn_pump(job: PumpJob) -> Receiver<PumpMsg> {
    let (tx, rx) = sync_channel::<PumpMsg>(PREFETCH);
    std::thread::spawn(move || {
        let frame_bytes = (job.size.0 as usize) * (job.size.1 as usize) * 3 / 2;
        let mut last: Option<Vec<u8>> = None;
        for seg in &job.segments {
            let mut child = match Command::new(&job.ffmpeg)
                .args(decoder_args(&job, seg))
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
            {
                Ok(c) => c,
                Err(e) => {
                    let _ = tx.send(Err(crate::tools::spawn_error(&job.ffmpeg, &e)));
                    return;
                }
            };
            let Some(mut out) = child.stdout.take() else {
                return;
            };
            let mut ended = false;
            for _ in 0..seg.frames {
                let mut buf = vec![0u8; frame_bytes];
                let got = if ended {
                    false
                } else {
                    match out.read_exact(&mut buf) {
                        Ok(()) => true,
                        Err(e) if e.kind() == ErrorKind::UnexpectedEof => false,
                        Err(e) => {
                            let _ = tx.send(Err(CmError::internal(format!(
                                "reading decoded frames of `{}`: {e}",
                                job.file.display()
                            ))));
                            let _ = child.kill();
                            return;
                        }
                    }
                };
                if !got {
                    // The file ended before the segment did: hold the last picture.
                    ended = true;
                    match &last {
                        Some(l) => buf.copy_from_slice(l),
                        None => {
                            let _ = tx.send(Err(CmError::internal(format!(
                                "FFmpeg could not decode `{}` at {:.2} s",
                                job.file.display(),
                                seg.seek_s
                            ))));
                            let _ = child.kill();
                            return;
                        }
                    }
                } else {
                    last = Some(buf.clone());
                }
                if tx.send(Ok(buf)).is_err() {
                    let _ = child.kill();
                    return;
                }
            }
            let _ = child.kill();
            let _ = child.wait();
        }
    });
    rx
}

fn recv(rx: &Receiver<PumpMsg>, what: &str) -> Result<Vec<u8>, CmError> {
    rx.recv()
        .map_err(|_| CmError::internal(format!("the {what} decoder stopped early")))?
}

// ---- the compositor loop -----------------------------------------------------------------------

/// The camera feed of the compositor loop: decoded frames of this size (the bubble's, or the wide
/// uncropped picture when layout ranges need more than the bubble).
struct CameraFeed {
    rx: Receiver<PumpMsg>,
    size: (u32, u32),
}

struct ComposeJob {
    frames: u64,
    fps: f64,
    out: (u32, u32),
    screen_size: (u32, u32),
    screen: Receiver<PumpMsg>,
    camera: Option<CameraFeed>,
    /// Which layout applies at which session time (CAM-3).
    planner: LayoutPlanner,
    timeline: Timeline,
    bg: BgOwned,
    padding_pct: f32,
    radius_px: f32,
    shadow: bool,
    /// EDT-4 and EDT-5: the crop of zooms and reframes, and the telemetry cursor.
    view: std::sync::Arc<EditView>,
    /// Burned-in captions on the output timeline (AI-1); empty without them.
    cues: std::sync::Arc<Vec<CaptionCue>>,
}

fn write_frame(stdin: &mut impl Write, data: &[u8]) -> Result<(), CmError> {
    stdin.write_all(data).map_err(|e| {
        if e.kind() == ErrorKind::BrokenPipe {
            CmError::internal(BROKEN_PIPE)
        } else {
            CmError::internal(format!("writing a frame to FFmpeg: {e}"))
        }
    })
}

fn compose_loop(job: ComposeJob, mut stdin: std::process::ChildStdin) -> Result<(), CmError> {
    let mut gpu = Compositor::new()?;
    // Session time of the last frame that had one, for a frame at the very end of the timeline.
    let mut session = MediaTime::ZERO;
    for k in 0..job.frames {
        let screen = recv(&job.screen, "screen")?;
        let cam = match &job.camera {
            Some(c) => Some((recv(&c.rx, "camera")?, c.size)),
            None => None,
        };
        let out_t =
            MediaTime::from_ticks((k as f64 / job.fps * TICKS_PER_SECOND as f64).round() as i64);
        if let Some(sample) = job.timeline.at_output(out_t) {
            session = sample.source;
        }
        let fl = job.planner.at(session);
        let view = job.view.at(session);
        let mut scene = Scene::new(
            job.out,
            Frame {
                width: job.screen_size.0,
                height: job.screen_size.1,
                format: PixelFormat::Nv12,
                data: &screen,
            },
        );
        scene.background = job.bg.as_scene();
        scene.padding_pct = job.padding_pct;
        scene.radius_px = job.radius_px;
        scene.shadow = job.shadow;
        view.apply(&mut scene);
        scene.captions = caption_layer_at(&job.cues, out_t, job.out);
        if let Some((data, (w, h))) = &cam {
            scene.camera = Some(CameraLayer {
                frame: Frame {
                    width: *w,
                    height: *h,
                    format: PixelFormat::Nv12,
                    data,
                },
                mirror: fl.mirror,
            });
            scene.layout = fl.layout;
        }
        // CAM-3.AC3: part way through a smooth layout change the tiles are between the two layouts.
        let img = match (&cam, fl.from) {
            (Some(_), Some((from, f))) => {
                let plan = plan_blend(&scene, from, f)?;
                gpu.render_with_plan(&scene, plan)?
            }
            _ => gpu.render(&scene)?,
        };
        write_frame(&mut stdin, img.as_raw())?;
    }
    Ok(())
}

// ---- the export -----------------------------------------------------------------------------------

fn even_floor(x: u32) -> u32 {
    (x - x % 2).max(2)
}

/// The refusal for a clip over a preset's length limit (GIF: 30 s).
fn too_long(max: f64, duration_s: f64) -> CmError {
    CmError::invalid_input(format!(
        "Use MP4 or WebM for clips longer than {max} s (this one is {duration_s:.1} s)"
    ))
}

/// Checks a preset request against a recording without running FFmpeg: the preset exists, its
/// options are allowed, it does not enlarge the recording where it must not (EXP-2.AC3) and a
/// length limit holds (EXP-3). Returns what the export would use. `export_video` calls this
/// before it queues a job, so a bad request fails at once instead of in the background.
pub fn check_request(
    manifest: &Value,
    req: &crate::quick::PresetRequest<'_>,
) -> Result<Target, CmError> {
    let spec = QuickSpec::from_manifest(manifest)?;
    let target = preset::resolve(
        req.presets,
        req.id,
        spec.screen.size,
        spec.screen.fps,
        &req.options,
    )?;
    if let Some(max) = target.max_seconds {
        let resolved = resolve_edits(manifest, &spec);
        let list = with_pauses_cut(&resolved.list, &resolved.gaps);
        let d = secs_of(list.timeline().duration());
        if d > max + 0.5 {
            return Err(too_long(max, d));
        }
    }
    Ok(target)
}

/// Runs the edited export. See the module docs.
#[allow(clippy::too_many_arguments)]
pub fn export_edited(
    tools: &Tools,
    req: &QuickRequest<'_>,
    mut spec: QuickSpec,
    resolved: Resolved,
    target: Option<&Target>,
    audio: &dyn AudioProcessor,
    progress: &mut dyn FnMut(f64),
    cancel: &dyn Fn() -> bool,
) -> Result<QuickOutcome, ExportError> {
    let t0 = Instant::now();
    let bundle = req.bundle_dir;
    let caps = probe_caps(tools)?;
    let kind = target.map_or(Kind::Mp4, |t| t.kind);
    check_sidecar(&caps, kind)?;
    // GIF and WebM are made by software encoders FFmpeg offers (NFR-12: BSD or LGPL code).
    let candidates = if kind == Kind::Mp4 {
        encoder_candidates(&caps, req.prefer_encoder, false)?
    } else {
        Vec::new()
    };

    // The master's real size, when ffprobe is there (the manifest's otherwise).
    let screen_path = bundle.join(&spec.screen.file);
    let master = master_size(tools, &screen_path, &spec);
    let screen_size = (even_floor(master.0), even_floor(master.1));
    // A preset names the frame and fps; without one the output is the master's, capped.
    let out_size = target
        .and_then(|t| t.frame)
        .unwrap_or_else(|| output_size(master));
    let fps = target.and_then(|t| t.fps).unwrap_or(spec.screen.fps);
    let preset_id = target.map_or(crate::quick::PRESET_QUICK, |t| t.id.as_str());
    let loudness = req
        .loudness_lufs
        .or_else(|| target.and_then(|t| t.loudness_lufs));

    // Pause gaps are not exported: they join the user's cuts.
    let list = with_pauses_cut(&resolved.list, &resolved.gaps);
    let tl = list.timeline();
    if tl.duration() < cm_core::MIN_OUTPUT {
        return Err(CmError::invalid_input("Nothing left to export").into());
    }
    // Burned-in captions: the lines on this very timeline (cuts removed, speed applied).
    let cues = std::sync::Arc::new(match &req.captions {
        Some(source) => source(&tl)?,
        None => Vec::new(),
    });
    let frames = frame_count(tl.duration(), fps);
    let duration_s = frames as f64 / fps;
    if let Some(t) = target {
        if let Some(max) = t.max_seconds.filter(|m| duration_s > *m + 0.5 / fps) {
            return Err(too_long(max, duration_s).into());
        }
        if let Some(warn) = t.warn_seconds.filter(|w| duration_s > *w) {
            spec.notes.push(format!(
                "the clip is {duration_s:.1} s, over {warn} s: the file will be large"
            ));
        }
        spec.notes.extend(t.notes.iter().cloned());
    }
    let screen_start = MediaTime::from_ticks(spec.screen.start);

    // Optional tracks that are not on disk are left out (noted), the export still succeeds.
    let exists = |f: &str| bundle.join(f).is_file();
    let camera_present = spec.camera.as_ref().is_some_and(|c| exists(&c.file));
    if spec.camera.is_some() && !camera_present {
        spec.notes
            .push("the camera file is missing, exported without the bubble".to_owned());
    }
    let mic = spec.mic.clone().filter(|m| exists(&m.file));
    let system = spec.system.clone().filter(|s| exists(&s.file));
    let (mic, system) = if kind == Kind::Gif {
        (None, None)
    } else {
        (mic, system)
    };
    if kind != Kind::Gif && spec.mic.is_some() && mic.is_none() {
        spec.notes.push("the microphone file is missing".to_owned());
    }
    if kind != Kind::Gif && spec.system.is_some() && system.is_none() {
        spec.notes
            .push("the system audio file is missing".to_owned());
    }

    let bg = load_background(&list, bundle)?;
    // EDT-4 and EDT-5: zooms, the reframe window and the telemetry cursor. The cursor file is
    // read only when something uses it.
    let media = (spec.screen.cursor_separate || !list.view.is_default())
        .then(|| CursorMedia::load(bundle))
        .flatten();
    let view = std::sync::Arc::new(EditView::new(&ViewParams {
        list: &list,
        media,
        screen_size,
        source_size: spec.screen.source_size,
        display_scale: spec.screen.scale,
        cursor_separate: spec.screen.cursor_separate,
        out_size,
    }));
    // A reframed picture fills the frame: no padding, no rounded screen, no shadow, and the
    // bubble sits on the whole frame.
    let reframing = view.reframing();
    let cursor_drawn = view.drawing_cursor();
    let camera_plan = match (spec.visible_camera(), camera_present) {
        (Some((cam, layout)), true) => Some((cam.clone(), layout.clone())),
        _ => None,
    };
    // On a preset frame of another size the bubble sits on the picture, not on the whole frame,
    // and out of the platform's UI zones (vertical-1080).
    let preset_frame = target.is_some_and(|t| t.frame.is_some());
    let placement = if preset_frame {
        Placement::OnPicture {
            pic: if reframing {
                [0, 0, out_size.0, out_size.1]
            } else {
                fit_rect(out_size, list.background.padding_pct, screen_size)
            },
            safe: target.and_then(|t| t.safe),
        }
    } else {
        Placement::Whole
    };
    let bubble = camera_plan
        .as_ref()
        .map(|(_, l)| (geom_of(l, out_size, placement), l.clone()));
    // CAM-3: layout ranges choose, per frame, the bubble, camera full, side by side or no camera.
    let planner = LayoutPlanner::new(
        &list,
        camera_plan.as_ref().map(|(_, l)| l.clone()),
        out_size,
        placement,
    );
    let flexible_camera = planner.is_flexible();
    // The wide camera feed: the picture uncropped at its own aspect, as big as the frame needs.
    let wide_camera = camera_plan.as_ref().map(|(c, _)| {
        let native = tools
            .ffprobe
            .as_ref()
            .and_then(|_| probe_file(tools, &bundle.join(&c.file)).ok())
            .and_then(|p| Some((p.width?, p.height?)))
            .unwrap_or((1280, 720));
        let w = even_floor(native.0.min(out_size.0.max(2)).min(WIDE_CAMERA_MAX_W));
        let h = even_floor(
            (f64::from(w) * f64::from(native.1) / f64::from(native.0.max(1))).round() as u32,
        );
        (w, h)
    });

    // Audio: inputs 1.. follow the piped picture.
    let mut audio_inputs: Vec<PathBuf> = Vec::new();
    let mut sources: Vec<AudioSource> = Vec::new();
    for (kind, t) in [(AudioKind::Mic, &mic), (AudioKind::System, &system)] {
        if let Some(t) = t {
            audio_inputs.push(bundle.join(&t.file));
            sources.push(AudioSource {
                kind,
                input: audio_inputs.len(),
                start: MediaTime::from_ticks(t.start),
            });
        }
    }
    let channels = [mic.as_ref(), system.as_ref()]
        .into_iter()
        .flatten()
        .map(|a| a.channels)
        .max();
    let layout_name = if channels.unwrap_or(1) >= 2 {
        "stereo"
    } else {
        "mono"
    };
    // The PCM mix (AUD-4 gains, cuts, speed with the pitch kept, loudness) when the processor
    // offers one; its FFmpeg filter chains otherwise.
    let mixed: Option<MixedPcm> = if sources.is_empty() {
        None
    } else {
        let mut pcm: Vec<PcmTrack> = Vec::new();
        for (kind, t) in [(AudioKind::Mic, &mic), (AudioKind::System, &system)] {
            if let Some(t) = t {
                if cancel() {
                    return Err(ExportError::Cancelled);
                }
                let ch = t.channels.clamp(1, 2) as u16;
                pcm.push(PcmTrack {
                    kind,
                    samples: decode_pcm(&tools.ffmpeg, &bundle.join(&t.file), ch)?,
                    channels: ch,
                    start: MediaTime::from_ticks(t.start),
                });
            }
        }
        let input = MixInput {
            tracks: &pcm,
            gaps: &resolved.gaps,
            timeline: &tl,
            audio: &list.audio,
            out_channels: channels.unwrap_or(1).clamp(1, 2) as u16,
            loudness_lufs: loudness,
        };
        match audio.mix(&input) {
            Some(r) => Some(r?),
            None => None,
        }
    };
    let audio_graph_str = if mixed.is_some() {
        None
    } else {
        audio_graph(
            &tl,
            &resolved.gaps,
            &sources,
            layout_name,
            caps.amix_normalize(),
            audio,
        )
    };

    // Segments for each picture track.
    let screen_segments = video_segments(&tl, &resolved.gaps, screen_start, fps, frames);
    let camera_segments = camera_plan.as_ref().map(|(c, _)| {
        video_segments(
            &tl,
            &resolved.gaps,
            MediaTime::from_ticks(c.start),
            fps,
            frames,
        )
    });

    let exports = bundle.join(EXPORTS_DIR);
    fs::create_dir_all(&exports).map_err(|e| io_err("create", &exports, &e))?;
    sweep_stale(&exports);
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let name = output_name_for(&spec.title, preset_id, kind.extension());
    let final_path = exports.join(&name);
    let part = exports.join(format!("{name}.{}-{nonce}.part", std::process::id()));
    let mut cleanup = Cleanup {
        paths: vec![part.clone()],
        dirs: Vec::new(),
    };
    if cancel() {
        return Err(ExportError::Cancelled);
    }

    // The mixed PCM goes to a scratch file next to the `.part`, read as raw f32 by FFmpeg.
    let mixed_input: Option<(PathBuf, u32)> = match &mixed {
        Some(m) => {
            let p = exports.join(format!("{name}.{}-{nonce}.mix.part", std::process::id()));
            fs::write(&p, f32le_bytes(&m.samples)).map_err(|e| io_err("write", &p, &e))?;
            cleanup.paths.push(p.clone());
            Some((p, u32::from(m.channels)))
        }
        None => None,
    };
    // A GIF is made in two steps: the compositor and a lossless FFV1 file first, then the palette
    // and the GIF from it, so a "fit under N MB" retry does not compose the frames again.
    let stage1: Option<PathBuf> = (kind == Kind::Gif)
        .then(|| exports.join(format!("{name}.{}-{nonce}.mid.part", std::process::id())));
    if let Some(p) = &stage1 {
        cleanup.paths.push(p.clone());
    }
    let encode_to: &Path = stage1.as_deref().unwrap_or(&part);
    // Share of the progress bar the picture step takes.
    let first_share = if kind == Kind::Gif { 0.8 } else { 0.98 };

    let audio_bps = if channels.unwrap_or(1) >= 2 {
        if kind == Kind::WebM {
            OPUS_BPS_STEREO
        } else {
            AUDIO_BPS_STEREO
        }
    } else if kind == Kind::WebM {
        OPUS_BPS_MONO
    } else {
        AUDIO_BPS_MONO
    };
    let has_audio = channels.is_some() && (audio_graph_str.is_some() || mixed.is_some());
    // WebM "fit under N MB": a bitrate from the limit, tightened and retried when the file is over.
    let max_bytes = target.and_then(|t| t.max_bytes);
    let mut webm_bps: Option<u64> = (kind == Kind::WebM)
        .then(|| {
            max_bytes.map(|m| {
                preset::fit_bitrate(m, duration_s, if has_audio { audio_bps.into() } else { 0 })
            })
        })
        .flatten();
    let mut size_retries = 0;

    let mut tried = 0;
    let mut encoder: Option<Encoder> = candidates.first().copied();
    let mut fallback_note: Option<String> = None;
    let mut bitrate: Option<u64> = None;
    loop {
        let fmt = match (kind, encoder) {
            (Kind::Mp4, Some(enc)) => {
                let codec = video_codec_at(enc, out_size, fps, target.and_then(|t| t.video_bps));
                bitrate = codec.bitrate;
                OutFormat::h264(enc, &codec.args)
            }
            (Kind::WebM, _) => OutFormat::vp9(webm_bps, fps, audio_bps),
            _ => OutFormat::lossless_stage(),
        };
        let args = encode_args(
            &fmt,
            &caps,
            out_size,
            fps,
            // The mix replaces the recorded files as FFmpeg inputs.
            if mixed_input.is_some() {
                &[]
            } else {
                &audio_inputs
            },
            audio_graph_str.as_deref(),
            mixed_input.as_ref().map(|(p, c)| (p.as_path(), *c)),
            channels,
            audio_bps,
            duration_s,
            encode_to,
        );
        let feeder: Feeder = {
            let screen = spawn_pump(PumpJob {
                ffmpeg: tools.ffmpeg.clone(),
                file: screen_path.clone(),
                segments: screen_segments.clone(),
                fps,
                size: screen_size,
                pre: Vec::new(),
            });
            let camera = match (&camera_plan, &camera_segments, &bubble, wide_camera) {
                (Some((c, _)), Some(segs), Some((g, l)), Some(wide)) => {
                    // Without layout ranges the feed is the cropped bubble picture; with them it
                    // is the whole camera picture and the compositor crops per tile.
                    let (size, pre) = if flexible_camera {
                        (wide, Vec::new())
                    } else {
                        let aspect = 1.0 / l.shape.aspect_h_over_w();
                        let a = secs((aspect * 10_000.0).round() / 10_000.0);
                        (
                            (g.w, g.h),
                            vec![format!("crop=w='min(iw,ih*{a})':h='min(ih,iw/{a})'")],
                        )
                    };
                    let rx = spawn_pump(PumpJob {
                        ffmpeg: tools.ffmpeg.clone(),
                        file: bundle.join(&c.file),
                        segments: segs.clone(),
                        fps,
                        size,
                        pre,
                    });
                    Some(CameraFeed { rx, size })
                }
                _ => None,
            };
            let job = ComposeJob {
                frames,
                fps,
                planner: planner.clone(),
                timeline: tl.clone(),
                out: out_size,
                screen_size,
                screen,
                camera,
                bg: match &bg {
                    BgOwned::None => BgOwned::None,
                    BgOwned::Color(c) => BgOwned::Color(*c),
                    BgOwned::Gradient(a, b) => BgOwned::Gradient(*a, *b),
                    BgOwned::Image(i) => BgOwned::Image(i.clone()),
                },
                padding_pct: if reframing {
                    0.0
                } else {
                    list.background.padding_pct as f32
                },
                radius_px: if reframing {
                    0.0
                } else {
                    list.background.radius_px as f32
                },
                shadow: list.background.shadow && !reframing,
                view: view.clone(),
                cues: cues.clone(),
            };
            Box::new(move |stdin| compose_loop(job, stdin))
        };
        let mut on_progress =
            |p: crate::run::Progress| progress((p.fraction * first_share).min(first_share));
        match run_ffmpeg_fed(
            &tools.ffmpeg,
            &args,
            duration_s,
            &mut on_progress,
            cancel,
            Some(feeder),
        ) {
            Ok(()) => {
                // WebM over the size limit: a lower bitrate from what the file came out at.
                if let (Kind::WebM, Some(max), Some(bps)) = (kind, max_bytes, webm_bps) {
                    let got = fs::metadata(encode_to).map_or(0, |m| m.len());
                    if got > max && size_retries < MAX_SIZE_RETRIES {
                        size_retries += 1;
                        let next = (bps as f64 * (max as f64 / got as f64) * 0.92)
                            .max(MIN_VIDEO_BPS as f64) as u64;
                        if next < bps {
                            webm_bps = Some(next);
                            let _ = fs::remove_file(encode_to);
                            progress(0.0);
                            continue;
                        }
                    }
                }
                break;
            }
            Err(RunError::Cancelled) => return Err(ExportError::Cancelled),
            Err(RunError::Failed(e)) => {
                let next = candidates.get(tried + 1).copied();
                let retry = encoder.is_some_and(|enc| {
                    req.prefer_encoder != Some(enc)
                        && e.code() != cm_core::ErrorCode::DiskFull
                        && is_encoder_init_failure(&e.to_string(), enc)
                });
                if let (true, Some(next), Some(enc)) = (retry, next, encoder) {
                    let note = format!(
                        "{} failed, used {} instead: {}",
                        enc.name(),
                        next.name(),
                        e.to_string().lines().next().unwrap_or("")
                    );
                    fallback_note = Some(match fallback_note {
                        Some(prev) => format!("{prev}; {note}"),
                        None => note,
                    });
                    let _ = fs::remove_file(encode_to);
                    tried += 1;
                    encoder = Some(next);
                    progress(0.0);
                    continue;
                }
                return Err(ExportError::Failed(e));
            }
        }
    }
    if cancel() {
        return Err(ExportError::Cancelled);
    }

    // GIF: the palette and the looping GIF from the lossless picture, stepping the width and the
    // frame rate down until the "fit under N MB" limit is met.
    let mut gif: Option<GifOut> = None;
    if let (Some(mid), Some(t)) = (&stage1, target) {
        let mut width = out_size.0;
        let mut gif_fps = fps;
        let mut ladder: Vec<u32> = t
            .width_ladder
            .iter()
            .copied()
            .filter(|w| *w < width)
            .collect();
        let mut fps_ladder: Vec<f64> = GIF_FPS_STEPS
            .iter()
            .copied()
            .filter(|f| *f < gif_fps - 1e-9)
            .collect();
        loop {
            let height = scaled_height(out_size, width);
            let args = gif_args(mid, &part, out_size, (width, height), fps, gif_fps);
            let share = (first_share, 0.98);
            let mut on_progress = |p: crate::run::Progress| {
                progress(share.0 + (share.1 - share.0) * p.fraction.clamp(0.0, 1.0));
            };
            match run_ffmpeg(&tools.ffmpeg, &args, duration_s, &mut on_progress, cancel) {
                Ok(()) => {}
                Err(RunError::Cancelled) => return Err(ExportError::Cancelled),
                Err(RunError::Failed(e)) => return Err(ExportError::Failed(e)),
            }
            let got = fs::metadata(&part)
                .map_err(|e| io_err("read", &part, &e))?
                .len();
            let over = max_bytes.filter(|m| got > *m);
            let Some(max) = over else {
                gif = Some(GifOut {
                    width,
                    height,
                    fps: gif_fps,
                });
                break;
            };
            if !ladder.is_empty() {
                width = ladder.remove(0);
            } else if !fps_ladder.is_empty() {
                gif_fps = fps_ladder.remove(0);
            } else {
                return Err(CmError::invalid_input(format!(
                    "The smallest GIF of this clip is {:.1} MB, over the {:.1} MB limit; \
                     use WebM or a shorter clip",
                    got as f64 / 1e6,
                    max as f64 / 1e6
                ))
                .into());
            }
            let _ = fs::remove_file(&part);
        }
        let _ = fs::remove_file(mid);
    }

    let size = fs::metadata(&part)
        .map_err(|e| io_err("read", &part, &e))?
        .len();
    if size == 0 {
        return Err(CmError::internal("FFmpeg produced an empty file").into());
    }
    if let (Kind::WebM, Some(max)) = (kind, max_bytes) {
        if size > max {
            spec.notes.push(format!(
                "the file is {:.1} MB, over the {:.1} MB limit after {} attempts",
                size as f64 / 1e6,
                max as f64 / 1e6,
                size_retries + 1
            ));
        }
    }
    fs::rename(&part, &final_path).map_err(|e| io_err("write", &final_path, &e))?;
    let replaced = remove_replaced(bundle, req.manifest, preset_id, &final_path);
    // `part` is the finished file now (renamed): forget only that one. The mixed-audio and GIF
    // scratch files stay on the list so they are deleted here too (about 23 MB per minute of audio).
    cleanup.paths.retain(|p| *p != part);
    drop(cleanup);

    let measured = tools
        .ffprobe
        .as_ref()
        .and_then(|_| probe_file(tools, &final_path).ok())
        .and_then(|p| p.duration_s)
        .unwrap_or(duration_s);
    let (codec_name, encoder_name, lgpl_safe) = match (kind, encoder) {
        (Kind::Mp4, Some(enc)) => ("h264", enc.name(), enc.lgpl_safe()),
        (Kind::WebM, _) => ("vp9", "libvpx-vp9", true),
        _ => ("gif", "gif", true),
    };
    let mut video = json!({
        "codec": codec_name, "encoder": encoder_name, "lgpl_safe": lgpl_safe,
    });
    match kind {
        Kind::Mp4 => {
            let enc = encoder.unwrap_or(Encoder::Libx264);
            video["profile"] = json!("high");
            video["bitrate_bps"] = json!(bitrate);
            video["gop"] = json!(video_codec_at(enc, out_size, fps, None).gop);
        }
        Kind::WebM => {
            video["bitrate_bps"] = json!(webm_bps);
            video["crf"] = json!(if webm_bps.is_some() {
                None
            } else {
                Some(WEBM_CRF)
            });
        }
        Kind::Gif => {
            if let Some(g) = &gif {
                video["width"] = json!(g.width);
                video["height"] = json!(g.height);
                video["fps"] = json!(g.fps);
                video["frames"] = json!((duration_s * g.fps).round() as u64);
                video["loop"] = json!(0);
                video["palette"] = json!("palettegen stats_mode=diff, paletteuse bayer 5");
            }
        }
    }
    let mut settings = json!({
        "renderer": "compositor",
        "preset": preset_id,
        "format": kind.extension(),
        "size": [out_size.0, out_size.1],
        "fps": fps,
        "video": video,
        "audio": channels.filter(|_| has_audio).map(|ch| json!({
            "codec": if kind == Kind::WebM { "opus" } else { "aac" },
            "rate": AUDIO_RATE, "channels": ch,
            "bitrate_bps": audio_bps,
            "mic": mic.is_some(), "system": system.is_some(),
            "mixer": if mixed.is_some() { "cm-audio" } else { "ffmpeg" },
            "mic_gain_db": list.audio.mic_gain_db, "system_gain_db": list.audio.system_gain_db,
            "mic_mute": list.audio.mic_mute, "system_mute": list.audio.system_mute,
            "loudness": mixed.as_ref().map(|m| json!({
                "target_lufs": loudness,
                "integrated_lufs": m.integrated_lufs,
                "true_peak_dbtp": m.true_peak_dbtp,
                "gain_db": m.normalise_gain_db,
            })),
        })),
        "edits": {
            "in_s": secs_of(list.start), "out_s": secs_of(list.end),
            "cuts": list.cuts.len(), "speed_spans": list.speeds.len(),
            "background": list.background.kind.as_str(),
            "padding_pct": list.background.padding_pct,
            "radius_px": list.background.radius_px,
            "shadow": list.background.shadow,
            "zooms": list.view.zooms.len(),
            "reframed": reframing,
            "cursor_drawn": cursor_drawn,
        },
        "camera": bubble.as_ref().map(|(g, l)| json!({
            "layout": l.to_json(),
            "bubble_px": [g.x, g.y, g.w, g.h],
        })),
        "notes": spec.notes,
        "ffmpeg": caps.version_line,
    });
    if let Some(t) = target {
        settings["target"] = json!({
            "label": t.label,
            "frame": t.frame.map(|f| [f.0, f.1]),
            "fps": t.fps,
            "video_bps": t.video_bps,
            "loudness_lufs": t.loudness_lufs,
            "max_bytes": t.max_bytes,
            "reframe": match t.reframe {
                preset::Reframe::Fit => json!("fit"),
                preset::Reframe::Crop(r) => json!({ "crop": r }),
            },
            "safe_area": t.safe.map(|s| json!({ "bottom": s.bottom, "right": s.right })),
        });
    }
    if let (Some(o), Some(n)) = (settings.as_object_mut(), fallback_note) {
        o.insert("encoder_fallback".into(), json!(n));
    }
    progress(1.0);
    Ok(QuickOutcome {
        preset: preset_id.to_owned(),
        file: format!("{EXPORTS_DIR}/{name}"),
        path: final_path,
        duration_ms: (measured * 1000.0).round() as i64,
        size_bytes: size,
        encoder: encoder_name.to_owned(),
        lgpl_safe,
        replaced,
        settings,
        elapsed_ms: u64::try_from(t0.elapsed().as_millis()).unwrap_or(u64::MAX),
    })
}

/// Opus bitrates (EXP-3): speech-grade, smaller than AAC's.
const OPUS_BPS_STEREO: u32 = 96_000;
const OPUS_BPS_MONO: u32 = 64_000;
/// VP9 quality when no size limit is set (lower is better; 31 is Google's 1080p recommendation).
const WEBM_CRF: u32 = 32;
/// The lowest video bitrate a "fit under" retry goes to.
const MIN_VIDEO_BPS: u64 = 20_000;
/// How often a WebM over its size limit is re-encoded at a lower bitrate.
const MAX_SIZE_RETRIES: u32 = 3;
/// GIF frame rates a "fit under" retry steps down through.
const GIF_FPS_STEPS: [f64; 3] = [24.0, 15.0, 10.0];

/// What the GIF step produced.
struct GifOut {
    width: u32,
    height: u32,
    fps: f64,
}

/// Height of a frame `width` wide with the aspect of `frame`.
fn scaled_height(frame: (u32, u32), width: u32) -> u32 {
    let h = (f64::from(width) * f64::from(frame.1) / f64::from(frame.0.max(1))).round() as u32;
    h.max(2)
}

/// The sidecar must offer the encoders the format needs, or the export says what to install.
fn check_sidecar(caps: &crate::tools::Caps, kind: Kind) -> Result<(), CmError> {
    let missing: Vec<&str> = match kind {
        Kind::Mp4 => Vec::new(),
        Kind::Gif => [("gif", caps.gif), ("ffv1", caps.ffv1)]
            .into_iter()
            .filter(|(_, ok)| !ok)
            .map(|(n, _)| n)
            .collect(),
        Kind::WebM => [("libvpx-vp9", caps.libvpx_vp9), ("libopus", caps.libopus)]
            .into_iter()
            .filter(|(_, ok)| !ok)
            .map(|(n, _)| n)
            .collect(),
    };
    if missing.is_empty() {
        return Ok(());
    }
    Err(CmError::platform_rejected(
        format!(
            "This FFmpeg has no {m} encoder, which {k} export needs. Use an FFmpeg built with {m} \
             (for example `brew install ffmpeg`), or export MP4.",
            m = missing.join(" and "),
            k = kind.extension().to_uppercase(),
        ),
        None,
    ))
}

/// The container, codecs and rate control of one encode.
#[derive(Debug, Clone)]
struct OutFormat {
    video: Vec<String>,
    pix_fmt: &'static str,
    tag_avc1: bool,
    audio: Option<AudioFmt>,
    container: &'static str,
    faststart: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AudioFmt {
    Aac,
    Opus,
}

impl OutFormat {
    /// H.264 in MP4 with AAC, fast start.
    fn h264(encoder: Encoder, codec: &[String]) -> OutFormat {
        OutFormat {
            video: codec.to_vec(),
            pix_fmt: encoder.pix_fmt(),
            tag_avc1: true,
            audio: Some(AudioFmt::Aac),
            container: "mp4",
            faststart: true,
        }
    }

    /// VP9 in WebM with Opus (EXP-3). With `bps` the rate is constrained to it (the size limit);
    /// without, constant quality.
    fn vp9(bps: Option<u64>, fps: f64, _audio_bps: u32) -> OutFormat {
        let gop = ((fps * 4.0).round() as u32).max(1).to_string();
        let mut video: Vec<String> = ["-c:v", "libvpx-vp9", "-row-mt", "1", "-deadline", "good"]
            .map(String::from)
            .to_vec();
        video.extend(["-cpu-used".into(), "4".into(), "-g".into(), gop]);
        match bps {
            Some(b) => video.extend([
                "-b:v".into(),
                b.to_string(),
                "-maxrate".into(),
                (b + b / 4).to_string(),
                "-bufsize".into(),
                (b * 2).to_string(),
            ]),
            None => video.extend([
                "-b:v".into(),
                "0".into(),
                "-crf".into(),
                WEBM_CRF.to_string(),
            ]),
        }
        OutFormat {
            video,
            pix_fmt: "yuv420p",
            tag_avc1: false,
            audio: Some(AudioFmt::Opus),
            container: "webm",
            faststart: false,
        }
    }

    /// Lossless FFV1 in Matroska: the first step of a GIF (no audio).
    fn lossless_stage() -> OutFormat {
        OutFormat {
            video: ["-c:v", "ffv1", "-level", "3", "-g", "1"]
                .map(String::from)
                .to_vec(),
            pix_fmt: "bgr0",
            tag_avc1: false,
            audio: None,
            container: "matroska",
            faststart: false,
        }
    }
}

/// The GIF step: `from_fps` frames per second in `mid`, `fps` out, `size` px wide (`frame` is the
/// size `mid` has). The palette comes from the whole clip (`stats_mode=diff` weights what
/// moves), applied with a Bayer dither and rectangle diffing; the GIF loops forever.
fn gif_args(
    mid: &Path,
    out: &Path,
    frame: (u32, u32),
    size: (u32, u32),
    from_fps: f64,
    fps: f64,
) -> Vec<String> {
    let mut chain: Vec<String> = Vec::new();
    if (fps - from_fps).abs() > 1e-9 {
        chain.push(format!("fps={}", rate(fps)));
    }
    if size != frame {
        chain.push(format!("scale={}:{}:flags=lanczos", size.0, size.1));
    }
    chain.push("split[a][b]".into());
    let graph = format!(
        "[0:v]{};[a]palettegen=stats_mode=diff[p];[b][p]paletteuse=dither=bayer:bayer_scale=5:diff_mode=rectangle[outv]",
        chain.join(",")
    );
    let mut a: Vec<String> = [
        "-hide_banner",
        "-nostdin",
        "-y",
        "-loglevel",
        "warning",
        "-nostats",
        "-progress",
        "pipe:1",
        "-i",
    ]
    .map(String::from)
    .to_vec();
    a.push(mid.to_string_lossy().into_owned());
    a.extend([
        "-filter_complex".into(),
        graph,
        "-map".into(),
        "[outv]".into(),
    ]);
    a.extend(["-loop", "0", "-f", "gif"].map(String::from));
    a.push(out.to_string_lossy().into_owned());
    a
}

/// The encoding command: composited RGBA frames on `pipe:0`, the audio files after them.
#[allow(clippy::too_many_arguments)]
fn encode_args(
    fmt: &OutFormat,
    caps: &crate::tools::Caps,
    out_size: (u32, u32),
    fps: f64,
    audio_inputs: &[PathBuf],
    audio_graph: Option<&str>,
    mixed: Option<(&Path, u32)>,
    channels: Option<u32>,
    audio_bps: u32,
    duration_s: f64,
    out: &Path,
) -> Vec<String> {
    let mut args: Vec<String> = [
        "-hide_banner",
        "-nostdin",
        "-y",
        "-loglevel",
        "warning",
        "-nostats",
        "-progress",
        "pipe:1",
        "-f",
        "rawvideo",
        "-pix_fmt",
        "rgba",
    ]
    .map(String::from)
    .to_vec();
    args.extend([
        "-s".into(),
        format!("{}x{}", out_size.0, out_size.1),
        "-framerate".into(),
        rate(fps),
        "-i".into(),
        "pipe:0".into(),
    ]);
    // A format without audio (the GIF's first step) takes no audio input at all.
    let (audio_inputs, audio_graph, mixed) = if fmt.audio.is_some() {
        (audio_inputs, audio_graph, mixed)
    } else {
        (&[][..], None, None)
    };
    for a in audio_inputs {
        args.extend(["-i".into(), a.to_string_lossy().into_owned()]);
    }
    if let Some((path, ch)) = mixed {
        // The finished mix: raw f32 on the output timeline, input 1.
        args.extend([
            "-f".into(),
            "f32le".into(),
            "-ar".into(),
            AUDIO_RATE.to_string(),
            "-ac".into(),
            ch.to_string(),
            "-i".into(),
            path.to_string_lossy().into_owned(),
        ]);
    }
    let has_audio = audio_graph.is_some() || mixed.is_some();
    // RGBA to YUV with the BT.709 matrix and limited range, and say so in the stream: FFmpeg's
    // default is BT.601, and players take an untagged HD stream as BT.709, which shifts every
    // colour (found by the M4 preview/export parity test).
    let yuv = fmt.pix_fmt.starts_with("yuv");
    let convert = if yuv {
        format!(
            "scale=out_color_matrix=bt709:out_range=tv,format={}",
            fmt.pix_fmt
        )
    } else {
        format!("format={}", fmt.pix_fmt)
    };
    let mut graph = vec![format!("[0:v]{convert}[outv]")];
    if let Some(a) = audio_graph {
        graph.push(a.to_owned());
    }
    args.extend(["-filter_complex".into(), graph.join(";")]);
    args.extend(["-map".into(), "[outv]".into()]);
    if audio_graph.is_some() {
        args.extend(["-map".into(), "[outa]".into()]);
    } else if mixed.is_some() {
        args.extend(["-map".into(), "1:a".into()]);
    }
    args.extend(["-map_metadata".into(), "-1".into()]);
    args.extend(fmt.video.iter().cloned());
    args.extend(["-pix_fmt".into(), fmt.pix_fmt.into()]);
    if yuv {
        args.extend(
            [
                "-colorspace",
                "bt709",
                "-color_primaries",
                "bt709",
                "-color_trc",
                "bt709",
                "-color_range",
                "tv",
            ]
            .map(String::from),
        );
    }
    if fmt.tag_avc1 {
        args.extend(["-tag:v".into(), "avc1".into()]);
    }
    if caps.fps_mode() {
        args.extend(["-fps_mode".into(), "cfr".into()]);
    }
    if let (Some(ch), true, Some(codec)) = (channels, has_audio, fmt.audio) {
        let name = match codec {
            AudioFmt::Aac => "aac",
            AudioFmt::Opus => "libopus",
        };
        args.extend(
            [
                "-c:a",
                name,
                "-b:a",
                &audio_bps.to_string(),
                "-ar",
                &AUDIO_RATE.to_string(),
                "-ac",
                &ch.to_string(),
            ]
            .map(String::from),
        );
    }
    if fmt.faststart {
        args.extend(["-movflags".into(), "+faststart".into()]);
    }
    args.extend(["-f", fmt.container, "-t", &secs(duration_s)].map(String::from));
    args.push(out.to_string_lossy().into_owned());
    args
}

/// Convenience for [`crate::quick::export_quick`]: the default audio processor.
pub fn default_audio() -> DspAudio {
    DspAudio
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn manifest(edit: Value) -> Value {
        json!({
            "title": "T", "live_at": 36000,
            "tracks": {"screen": {"file": "screen.mp4", "start": 24000, "end": 7_224_000,
                                   "fps": 30, "size": [1920, 1080]}},
            "edits": [edit],
        })
    }

    /// The recorder's original entry has no `version`: the FFmpeg graph renders it.
    #[test]
    fn an_untouched_recording_is_trivial() {
        let m = manifest(json!({"in": 0, "out": 7_200_000, "speed": 1.0, "layout": {}}));
        let spec = QuickSpec::from_manifest(&m).unwrap();
        assert!(resolve_edits(&m, &spec).trivial);
    }

    /// A cut, a speed span or a background makes the compositor necessary.
    #[test]
    fn edits_that_change_the_picture_are_not_trivial() {
        let base = json!({"version": 2, "in": 36000, "out": 7_224_000, "speed": 1.0});
        let m = manifest(base.clone());
        let spec = QuickSpec::from_manifest(&m).unwrap();
        assert!(resolve_edits(&m, &spec).trivial, "a restated default list");
        for patch in [
            json!({"cuts": [[600_000, 1_200_000]]}),
            json!({"speeds": [{"from": 36000, "to": 600_000, "factor": 2.0, "keep_audio": false}]}),
            json!({"background": {"kind": "color", "value": "#112233", "padding_pct": 8.0,
                                   "radius_px": 12, "shadow": true}}),
            json!({"in": 240_000}),
        ] {
            let mut e = base.clone();
            e.as_object_mut()
                .unwrap()
                .extend(patch.as_object().unwrap().clone());
            let m = manifest(e);
            let spec = QuickSpec::from_manifest(&m).unwrap();
            assert!(!resolve_edits(&m, &spec).trivial, "{patch}");
        }
    }

    #[test]
    fn colors_parse_and_bad_ones_are_refused() {
        assert_eq!(parse_hex(&json!("#112233")).unwrap(), [0x11, 0x22, 0x33]);
        assert!(parse_hex(&json!("112")).is_err());
        assert!(parse_hex(&json!(5)).is_err());
    }

    /// The encode command reads the picture from the pipe and the audio files after it.
    #[test]
    fn encode_args_pipe_the_picture_in_and_map_the_graph() {
        let caps = crate::tools::caps_from_output("ffmpeg version 7.1", "V....D h264_videotoolbox");
        let codec = video_codec_at(Encoder::VideoToolbox, (1920, 1080), 30.0, None);
        let a = encode_args(
            &OutFormat::h264(Encoder::VideoToolbox, &codec.args),
            &caps,
            (1920, 1080),
            30.0,
            &[PathBuf::from("/b/mic.wav")],
            Some("[1:a]anull[outa]"),
            None,
            Some(1),
            AUDIO_BPS_MONO,
            10.0,
            Path::new("/b/o.part"),
        );
        let j = a.join(" ");
        assert!(j.contains("-f rawvideo -pix_fmt rgba -s 1920x1080 -framerate 30 -i pipe:0"));
        assert!(j.contains("-i /b/mic.wav"));
        assert!(j.contains(
            "[0:v]scale=out_color_matrix=bt709:out_range=tv,format=yuv420p[outv];[1:a]anull[outa]"
        ));
        // The stream says so too, or players read the untagged HD video as BT.709 anyway.
        assert!(
            j.contains("-colorspace bt709 -color_primaries bt709 -color_trc bt709 -color_range tv"),
            "{j}"
        );
        assert!(j.contains("-map [outv] -map [outa]"));
        assert!(j.contains("-c:v h264_videotoolbox"));
        assert!(j.ends_with("-t 10 /b/o.part"), "{j}");
    }

    /// AUD-4: the finished mix is a raw f32 input on the output timeline and is mapped as audio.
    #[test]
    fn encode_args_read_the_mixed_pcm_as_raw_f32() {
        let caps = crate::tools::caps_from_output("ffmpeg version 7.1", "V....D h264_videotoolbox");
        let codec = video_codec_at(Encoder::VideoToolbox, (1920, 1080), 30.0, None);
        let a = encode_args(
            &OutFormat::h264(Encoder::VideoToolbox, &codec.args),
            &caps,
            (1920, 1080),
            30.0,
            &[],
            None,
            Some((Path::new("/b/o.mix.part"), 2)),
            Some(2),
            AUDIO_BPS_STEREO,
            10.0,
            Path::new("/b/o.part"),
        );
        let j = a.join(" ");
        assert!(
            j.contains("-f f32le -ar 48000 -ac 2 -i /b/o.mix.part"),
            "{j}"
        );
        assert!(j.contains("-map [outv] -map 1:a"), "{j}");
        assert!(j.contains("-c:a aac"), "{j}");
    }

    /// The GIF's lossless first step stays RGB: no YUV matrix and no colour tags.
    #[test]
    fn encode_args_leave_rgb_stages_untagged() {
        let caps = crate::tools::caps_from_output("ffmpeg version 7.1", "V....D h264_videotoolbox");
        let a = encode_args(
            &OutFormat::lossless_stage(),
            &caps,
            (800, 450),
            15.0,
            &[],
            None,
            None,
            None,
            AUDIO_BPS_STEREO,
            10.0,
            Path::new("/b/o.mkv"),
        );
        let j = a.join(" ");
        assert!(j.contains("[0:v]format=bgr0[outv]"), "{j}");
        assert!(
            !j.contains("-colorspace") && !j.contains("out_color_matrix"),
            "{j}"
        );
    }
}
