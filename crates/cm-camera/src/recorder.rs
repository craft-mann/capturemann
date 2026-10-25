//! `CameraRecorder`: the camera [`TrackRecorder`].
//!
//! Frames come from a [`CameraSource`], are retimed by [`Retimer`] (capture PTS on the session
//! clock, see `retime.rs`) and appended to a [`FragmentWriter`] created lazily from the first
//! frame's real size (so a camera in a non-standard mode such as 1280x960 is recorded native).
//!
//! # Pause convention (REC-4)
//!
//! The file has continuous timestamps: frames are written at file time, session time minus the
//! pause gaps before them (see `retime.rs`), exactly like the screen and audio tracks. The track
//! facts keep both views: `TrackInfo.start`/`end` are session time, `details.file_start`/
//! `file_end` are file time, and `details.pauses` is the gap list in session time (the same
//! list the session stores in `manifest.gaps`), which maps file time back to session time.

use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard};

use cm_core::{
    CmError, MediaTime, SessionClock, TrackContext, TrackInfo, TrackKind, TrackRecorder, TrackStats,
};
use serde_json::{json, Value};

use crate::latency::{LatencyStats, BASELINE_FRAMES};
use crate::retime::{Retimed, Retimer};
use crate::types::{
    CameraConfig, CameraEvent, CameraFrame, CameraSource, FrameSink, OpenRequest, SourceRole,
};
use crate::writer::{
    AppendOutcome, FragmentWriter, WriterFactory, WriterReport, WriterSpec, FRAGMENT_SECONDS,
};

/// File name inside the bundle directory.
pub const CAMERA_FILE: &str = "camera.mp4";

/// Things the session/UI should react to while recording (maps to the `camera.disconnected` /
/// `camera.connected` agent events and the toast in CAM-1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecorderEvent {
    /// The camera went away. The file stays valid, the track has a gap, recording continues.
    DeviceLost { at: MediaTime },
    /// Frames are flowing again after a loss.
    DeviceRestored { at: MediaTime },
    /// The encoder/writer failed; the track stopped growing but is finalized at `stop`.
    WriterFailed { at: MediaTime, message: String },
    /// The OS withdrew camera access mid-recording. Like a lost device the file stays valid and
    /// the track has a gap; the owner decides whether the recording goes on (PRM-1).
    PermissionRevoked { at: MediaTime },
}

struct Warning {
    code: &'static str,
    at: MediaTime,
    message: String,
}

#[derive(Default)]
struct Counters {
    before_origin: u64,
    non_monotonic: u64,
    paused_skipped: u64,
    missed_slots: u64,
}

struct Inner {
    clock: SessionClock,
    retimer: Retimer,
    path: PathBuf,
    config: CameraConfig,
    factory: Arc<dyn WriterFactory>,
    writer: Option<Box<dyn FragmentWriter>>,
    writer_failed: bool,
    spec: Option<WriterSpec>,
    stats: TrackStats,
    counters: Counters,
    latency: LatencyStats,
    /// File time of the first / end of the last written frame.
    first_pts: Option<MediaTime>,
    last_end: MediaTime,
    /// Session time of the first / end of the last written frame (`TrackInfo.start` / `end`).
    first_session: Option<MediaTime>,
    last_session_end: MediaTime,
    warnings: Vec<Warning>,
    lost_at: Option<MediaTime>,
    accepting: bool,
    tx: Sender<RecorderEvent>,
}

struct Core {
    inner: Mutex<Inner>,
}

impl Core {
    fn lock(&self) -> MutexGuard<'_, Inner> {
        // A panic in a callback must not wedge `stop`; the data is counters and a writer handle.
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }
}

impl Inner {
    fn emit(&self, e: RecorderEvent) {
        let _ = self.tx.send(e);
    }

    fn note_restored(&mut self, at: MediaTime) {
        if self.lost_at.take().is_some() {
            self.warnings.push(Warning {
                code: "camera_restored",
                at,
                message: "camera frames resumed after a loss; the track has a gap".into(),
            });
            self.emit(RecorderEvent::DeviceRestored { at });
        }
    }

    fn fail_writer(&mut self, at: MediaTime, message: String) {
        if !self.writer_failed {
            self.writer_failed = true;
            self.warnings.push(Warning {
                code: "camera_writer_failed",
                at,
                message: message.clone(),
            });
            self.emit(RecorderEvent::WriterFailed { at, message });
        }
    }

    fn frame(&mut self, f: &CameraFrame) {
        if !self.accepting {
            return;
        }
        self.latency.push(f.delivered_host_ns, f.capture_host_ns);
        match self.retimer.map(f.capture_host_ns) {
            Retimed::Paused => self.counters.paused_skipped += 1,
            Retimed::BeforeOrigin => {
                self.stats.delivered += 1;
                self.counters.before_origin += 1;
            }
            Retimed::NotMonotonic => {
                self.stats.delivered += 1;
                self.stats.dropped += 1;
                self.counters.non_monotonic += 1;
            }
            Retimed::Write {
                pts,
                session_pts,
                duration,
                missed_slots,
            } => {
                self.stats.delivered += 1;
                self.write(f, pts, session_pts, duration, missed_slots);
            }
        }
    }

    fn write(
        &mut self,
        f: &CameraFrame,
        pts: MediaTime,
        session_pts: MediaTime,
        duration: MediaTime,
        missed: u32,
    ) {
        if self.writer_failed {
            self.stats.dropped += 1;
            return;
        }
        if self.writer.is_none() {
            let spec = WriterSpec::for_source((f.width, f.height), self.config.fps);
            match self.factory.create(&self.path, &spec) {
                Ok(w) => {
                    self.writer = Some(w);
                    self.spec = Some(spec);
                }
                Err(e) => {
                    self.fail_writer(pts, e.to_string());
                    self.stats.dropped += 1;
                    return;
                }
            }
        }
        self.note_restored(pts);
        let Some(writer) = self.writer.as_mut() else {
            return;
        };
        match writer.append(f, pts, duration) {
            AppendOutcome::Written => {
                self.stats.written += 1;
                self.first_pts.get_or_insert(pts);
                self.last_end = pts + duration;
                self.first_session.get_or_insert(session_pts);
                self.last_session_end = session_pts + duration;
                self.counters.missed_slots += u64::from(missed);
                let interval_ms = duration.as_secs_f64() * 1000.0;
                let lag = (f.delivered_host_ns as f64 - f.capture_host_ns as f64) / 1e6;
                let late_by_lag = self
                    .latency
                    .baseline_ms()
                    .is_some_and(|b| lag - b > interval_ms);
                self.stats.late += u64::from(missed) + u64::from(late_by_lag);
            }
            AppendOutcome::NotReady => self.stats.dropped += 1,
            AppendOutcome::Failed(m) => {
                self.stats.dropped += 1;
                self.fail_writer(pts, m);
            }
        }
    }

    fn event(&mut self, e: CameraEvent) {
        if !self.accepting {
            return;
        }
        let at = self.clock.now();
        match e {
            CameraEvent::PermissionRevoked => {
                if self.lost_at.is_none() {
                    self.lost_at = Some(at);
                    self.warnings.push(Warning {
                        code: "camera_permission_revoked",
                        at,
                        message: "camera access was revoked".to_string(),
                    });
                    if let Some(w) = self.writer.as_mut() {
                        w.flush();
                    }
                }
                self.emit(RecorderEvent::PermissionRevoked { at });
            }
            CameraEvent::Disconnected | CameraEvent::Error(_) => {
                if self.lost_at.is_some() {
                    return;
                }
                let message = match e {
                    CameraEvent::Error(m) => format!("camera error: {m}"),
                    _ => "camera disconnected".to_string(),
                };
                self.lost_at = Some(at);
                self.warnings.push(Warning {
                    code: "camera_disconnected",
                    at,
                    message,
                });
                // Get everything captured so far onto disk; the writer stays open for a possible
                // reconnect (the gap is just missing time on the track).
                if let Some(w) = self.writer.as_mut() {
                    w.flush();
                }
                self.emit(RecorderEvent::DeviceLost { at });
            }
            // Restoration is reported when the first frame after the loss is written.
            CameraEvent::Reconnected => {}
            CameraEvent::FramesDropped(n) => {
                self.stats.delivered += u64::from(n);
                self.stats.dropped += u64::from(n);
            }
        }
    }
}

struct Sink(Arc<Core>);

impl FrameSink for Sink {
    fn on_frame(&self, frame: &CameraFrame) {
        self.0.lock().frame(frame);
    }
    fn on_event(&self, event: CameraEvent) {
        self.0.lock().event(event);
    }
}

/// Records the camera into `camera.mp4`.
pub struct CameraRecorder {
    config: CameraConfig,
    source: Box<dyn CameraSource>,
    factory: Arc<dyn WriterFactory>,
    core: Option<Arc<Core>>,
    events_rx: Option<Receiver<RecorderEvent>>,
    device: Option<(String, String)>,
    stopped: bool,
}

impl CameraRecorder {
    /// A recorder over any source and writer (tests use the fakes).
    pub fn new(
        config: CameraConfig,
        source: Box<dyn CameraSource>,
        factory: Arc<dyn WriterFactory>,
    ) -> Self {
        CameraRecorder {
            config,
            source,
            factory,
            core: None,
            events_rx: None,
            device: None,
            stopped: false,
        }
    }

    /// The platform recorder: the shared AVCaptureSession and the AVAssetWriter CMAF writer.
    pub fn system(config: CameraConfig) -> Self {
        Self::new(
            config,
            crate::platform::system_source(),
            crate::platform::system_writer_factory(),
        )
    }

    /// Takes the event receiver (call after `start`; `None` the second time).
    pub fn take_events(&mut self) -> Option<Receiver<RecorderEvent>> {
        self.events_rx.take()
    }

    pub fn config(&self) -> &CameraConfig {
        &self.config
    }
}

impl TrackRecorder for CameraRecorder {
    fn kind(&self) -> TrackKind {
        TrackKind::Camera
    }

    fn start(&mut self, ctx: TrackContext) -> Result<(), CmError> {
        if self.core.is_some() {
            return Err(CmError::invalid_input("camera recorder already started"));
        }
        if !(1..=60).contains(&self.config.fps) {
            return Err(CmError::invalid_field("fps", "must be between 1 and 60"));
        }
        let (tx, rx) = channel();
        let core = Arc::new(Core {
            inner: Mutex::new(Inner {
                retimer: Retimer::new(
                    ctx.clock.clone(),
                    self.config.fps,
                    self.config.latency_compensation_ms,
                ),
                clock: ctx.clock,
                path: ctx.dir.join(CAMERA_FILE),
                config: self.config.clone(),
                factory: self.factory.clone(),
                writer: None,
                writer_failed: false,
                spec: None,
                stats: TrackStats::default(),
                counters: Counters::default(),
                latency: LatencyStats::default(),
                first_pts: None,
                last_end: MediaTime::ZERO,
                first_session: None,
                last_session_end: MediaTime::ZERO,
                warnings: Vec::new(),
                lost_at: None,
                accepting: true,
                tx,
            }),
        });
        let req = OpenRequest {
            device_id: self.config.device_id.clone(),
            resolution: self.config.resolution,
            fps: self.config.fps,
            role: SourceRole::Record,
            first_frame_timeout: self.config.first_frame_timeout(),
        };
        let info = self.source.open(&req, Arc::new(Sink(core.clone())))?;
        self.device = Some((info.device_id, info.device_name));
        self.core = Some(core);
        self.events_rx = Some(rx);
        Ok(())
    }

    fn pause(&mut self, at: MediaTime) -> Result<(), CmError> {
        let core = self.running()?;
        core.lock().retimer.pause(at);
        Ok(())
    }

    fn resume(&mut self, at: MediaTime) -> Result<(), CmError> {
        let core = self.running()?;
        core.lock().retimer.resume(at);
        Ok(())
    }

    fn stop(&mut self, at: MediaTime) -> Result<TrackInfo, CmError> {
        let core = self
            .core
            .clone()
            .ok_or_else(|| CmError::invalid_input("camera recorder was not started"))?;
        if self.stopped {
            return Err(CmError::invalid_input("camera recorder already stopped"));
        }
        self.stopped = true;
        self.source.close();
        let mut g = core.lock();
        g.accepting = false;
        let pauses = g.retimer.finish(at);
        let report = match g.writer.take() {
            Some(mut w) => w.finish()?,
            None => {
                return Err(CmError::internal(match g.warnings.last() {
                    Some(w) => format!("camera track has no frames: {}", w.message),
                    None => "camera track has no frames".to_string(),
                }))
            }
        };
        let (id, name) = self.device.clone().unwrap_or_default();
        let details = build_details(&g, &report, &id, &name, &pauses);
        Ok(TrackInfo {
            kind: TrackKind::Camera,
            file: CAMERA_FILE.into(),
            // Session time, like every track (`details.file_start`/`file_end` are file time).
            start: g.first_session.unwrap_or(MediaTime::ZERO),
            end: g.last_session_end,
            details,
            stats: g.stats.clone(),
        })
    }

    fn stats(&self) -> TrackStats {
        self.core
            .as_ref()
            .map(|c| c.lock().stats.clone())
            .unwrap_or_default()
    }
}

impl CameraRecorder {
    fn running(&self) -> Result<&Arc<Core>, CmError> {
        self.core
            .as_ref()
            .ok_or_else(|| CmError::invalid_input("camera recorder was not started"))
    }
}

fn build_details(
    g: &Inner,
    report: &WriterReport,
    id: &str,
    name: &str,
    pauses: &[cm_core::PauseGap],
) -> Value {
    let size = g.spec.map(|s| [s.width, s.height]).unwrap_or([0, 0]);
    let span = (g.last_end - g.first_pts.unwrap_or(MediaTime::ZERO)).as_secs_f64();
    let measured_fps = if span > 0.0 {
        g.stats.written as f64 / span
    } else {
        0.0
    };
    let frag_secs: Vec<f64> = report
        .fragments
        .iter()
        .map(|f| f.duration.as_secs_f64())
        .collect();
    json!({
        "device": { "id": id, "name": name },
        "size": size,
        "fps": g.config.fps,
        "measured_fps": (measured_fps * 100.0).round() / 100.0,
        "codec": "h264",
        "bitrate_bps": g.spec.map(|s| s.bitrate_bps),
        "b_frames": false,
        "resolution_setting": g.config.resolution.as_str(),
        // Display flag only: the stored pixels are never flipped (CAM-2 AC3).
        "mirror": g.config.mirror,
        "timestamp_source": "capture_pts_host_clock",
        "latency_compensation_ms": g.config.latency_compensation_ms,
        "delivery_latency_ms": g.latency.summary(),
        "fragment_seconds": FRAGMENT_SECONDS,
        "fragments": {
            "count": report.fragments.len(),
            "min_s": frag_secs.iter().copied().reduce(f64::min),
            "max_s": frag_secs.iter().copied().reduce(f64::max),
            "bytes": report.bytes,
        },
        // Gaps in session time. The file has none: file time = session time minus the gaps.
        "pauses": pauses,
        "timeline": "file_time_gaps_removed",
        "file_start": g.first_pts.map(MediaTime::ticks),
        "file_end": g.last_end.ticks(),
        "frames": {
            "before_origin": g.counters.before_origin,
            "non_monotonic": g.counters.non_monotonic,
            "paused_skipped": g.counters.paused_skipped,
            "missed_slots": g.counters.missed_slots,
            "baseline_warmup": BASELINE_FRAMES,
        },
        "warnings": g.warnings.iter().map(|w| json!({
            "code": w.code, "at": w.at, "message": w.message,
        })).collect::<Vec<_>>(),
    })
}
