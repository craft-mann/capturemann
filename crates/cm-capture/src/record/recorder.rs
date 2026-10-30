//! `ScreenRecorder`: the screen track as a [`cm_core::TrackRecorder`].
//!
//! Generic over the OS source ([`ScreenSource`]) and the encoder ([`VideoEncoder`]), so the same
//! code runs on ScreenCaptureKit + VideoToolbox, on a fake source + synthetic encoder in CI, and
//! later on Windows.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use cm_core::{
    CmError, MediaTime, SessionClock, TrackContext, TrackInfo, TrackKind, TrackRecorder,
    TrackStats, TICKS_PER_SECOND,
};
use cm_encode::{is_hardware_encoder_unavailable, smaller_plans, EncoderConfig, VideoEncoder};
use serde::Serialize;
use serde_json::json;

use crate::disk::{assess_disk, DiskStatus, FreeSpace, SystemFreeSpace};

use super::pipeline::{FrameOutcome, HaltReason, Pipeline, PipelineStats};
use super::plan::CapturePlan;
use super::source::{ScreenSource, SourceEvent, StopReason};

/// Name of the screen track file inside the bundle (specs/01-technical-design.md, Data model).
pub const SCREEN_FILE: &str = "screen.mp4";

/// Health and safety notifications from the recorder (`recording.health`, REC-3, REC-8).
#[derive(Debug, Clone, PartialEq)]
pub enum RecorderEvent {
    /// Less than 10 minutes of space at the current bitrate (REC-8.AC4).
    DiskLow { minutes_left: f64 },
    /// At or below 1 GB free: the recorder stopped writing; the file is valid. The session
    /// should stop all tracks and tell the user (REC-8.AC3).
    DiskFull { free_bytes: u64 },
    /// More than 2% late frames over the last 30 s (REC-3.AC4).
    Degraded { late_pct: f64 },
    /// Back below the threshold.
    Healthy,
    /// The source delivered nothing for `silence_ms` while recording (R3-7): a capture stall.
    /// Reported once per stall.
    Stalled { silence_ms: u64 },
    /// The source ended by itself or the encoder failed; what was captured is kept.
    Halted(HaltReason),
}

type Listener = Arc<dyn Fn(RecorderEvent) + Send + Sync>;

/// Tunables; the defaults are what the daemon uses.
#[derive(Clone)]
pub struct RecorderOptions {
    pub file_name: String,
    /// Run the background heartbeat (constant frame rate through static screens, disk and health
    /// checks). Tests turn it off and call `tick` themselves.
    pub heartbeat: bool,
    pub free_space: Arc<dyn FreeSpace>,
    /// How often the disk and health checks run (NFR-7; 5 s per REC-3).
    pub check_every: Duration,
    pub listener: Option<Listener>,
}

impl Default for RecorderOptions {
    fn default() -> Self {
        RecorderOptions {
            file_name: SCREEN_FILE.into(),
            heartbeat: true,
            free_space: Arc::new(SystemFreeSpace),
            check_every: Duration::from_secs(5),
            listener: None,
        }
    }
}

/// Start/stop latency measurements.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct Latency {
    /// `start()` call until the stream was running.
    pub start_call_ms: f64,
    /// `start()` call until the first frame reached the file.
    pub first_frame_ms: Option<f64>,
    /// `stop()` call until the file was finished and closed (REC-5.AC2: within 2 s).
    pub stop_ms: Option<f64>,
}

/// Live status for `get_recording_status`.
#[derive(Debug, Clone, PartialEq)]
pub struct ScreenStatus {
    pub stats: PipelineStats,
    pub late_pct: f64,
    /// Late percentage over the last 30 s.
    pub recent_late_pct: f64,
    pub fragments: usize,
    pub halted: Option<HaltReason>,
    pub disk_left_minutes: Option<f64>,
}

struct Shared<E: VideoEncoder> {
    pipeline: Mutex<Pipeline<E>>,
    clock: SessionClock,
    started: Instant,
    first_frame_ms: Mutex<Option<f64>>,
    listener: Option<Listener>,
    degraded: AtomicBool,
    low_sent: AtomicBool,
    halt_sent: AtomicBool,
    dir: PathBuf,
    bitrate_bps: u64,
    free_space: Arc<dyn FreeSpace>,
    disk_minutes: Mutex<Option<f64>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl<E: VideoEncoder> Shared<E> {
    fn emit(&self, ev: RecorderEvent) {
        if let Some(l) = &self.listener {
            l(ev);
        }
    }

    fn report_halt(&self) {
        let reason = lock(&self.pipeline).halted().cloned();
        if let Some(r) = reason {
            if !self.halt_sent.swap(true, Ordering::SeqCst) {
                if let HaltReason::DiskFull = r {
                    let free = self.free_space.free_bytes(&self.dir).unwrap_or(0);
                    self.emit(RecorderEvent::DiskFull { free_bytes: free });
                }
                self.emit(RecorderEvent::Halted(r));
            }
        }
    }

    fn on_event(&self, ev: SourceEvent<E::Frame>) {
        match ev {
            SourceEvent::Frame { host_ns, frame } => {
                let pts = self.clock.from_host_ns(host_ns);
                let now = self.clock.now();
                let outcome = lock(&self.pipeline).on_frame(pts, now, frame);
                if outcome == FrameOutcome::FirstWritten {
                    *lock(&self.first_frame_ms) = Some(self.started.elapsed().as_secs_f64() * 1e3);
                }
                self.report_halt();
            }
            SourceEvent::Idle { host_ns } => {
                let pts = self.clock.from_host_ns(host_ns);
                lock(&self.pipeline).on_idle(pts);
            }
            SourceEvent::Stopped(reason) => {
                let halt = match reason {
                    StopReason::TargetClosed(m) | StopReason::Error(m) => {
                        HaltReason::SourceStopped(m)
                    }
                    StopReason::PermissionRevoked(m) => HaltReason::PermissionRevoked(m),
                };
                lock(&self.pipeline).halt(halt);
                self.report_halt();
            }
        }
    }

    /// Heartbeat (every call) plus the periodic disk and health checks (`checks`).
    fn tick(&self, checks: bool) {
        let now = self.clock.now();
        let stalled = {
            let mut p = lock(&self.pipeline);
            p.heartbeat(now);
            p.watchdog(now)
        };
        if let Some(silence) = stalled {
            self.emit(RecorderEvent::Stalled {
                silence_ms: u64::try_from(silence.ticks() / (TICKS_PER_SECOND / 1000)).unwrap_or(0),
            });
        }
        self.report_halt();
        if !checks {
            return;
        }
        if let Some(free) = self.free_space.free_bytes(&self.dir) {
            match assess_disk(free, self.bitrate_bps) {
                DiskStatus::Full { .. } => {
                    lock(&self.pipeline).halt(HaltReason::DiskFull);
                    self.report_halt();
                }
                DiskStatus::Low { minutes_left } => {
                    *lock(&self.disk_minutes) = Some(minutes_left);
                    if !self.low_sent.swap(true, Ordering::SeqCst) {
                        self.emit(RecorderEvent::DiskLow { minutes_left });
                    }
                }
                DiskStatus::Ok { minutes_left } => {
                    *lock(&self.disk_minutes) = Some(minutes_left);
                    self.low_sent.store(false, Ordering::SeqCst);
                }
            }
        }
        let recent = lock(&self.pipeline).recent_late_pct(now);
        let degraded = recent > 2.0;
        if degraded != self.degraded.swap(degraded, Ordering::SeqCst) {
            self.emit(if degraded {
                RecorderEvent::Degraded { late_pct: recent }
            } else {
                RecorderEvent::Healthy
            });
        }
    }
}

struct Running<E: VideoEncoder> {
    shared: Arc<Shared<E>>,
    stop_flag: Arc<AtomicBool>,
    heartbeat: Option<JoinHandle<()>>,
}

type EncoderFactory<E> = Box<dyn FnMut(&EncoderConfig) -> Result<E, CmError> + Send>;

pub struct ScreenRecorder<S: ScreenSource, E: VideoEncoder<Frame = S::Frame>> {
    plan: CapturePlan,
    source: S,
    make_encoder: EncoderFactory<E>,
    opts: RecorderOptions,
    running: Option<Running<E>>,
    latency: Latency,
    last_stats: TrackStats,
    /// The size asked for, when the encoder only accepted a smaller one.
    downscaled_from: Option<(u32, u32)>,
}

impl<S: ScreenSource, E: VideoEncoder<Frame = S::Frame>> ScreenRecorder<S, E> {
    pub fn new(
        plan: CapturePlan,
        source: S,
        make_encoder: impl FnMut(&EncoderConfig) -> Result<E, CmError> + Send + 'static,
    ) -> Self {
        ScreenRecorder {
            plan,
            source,
            make_encoder: Box::new(make_encoder),
            opts: RecorderOptions::default(),
            running: None,
            latency: Latency::default(),
            last_stats: TrackStats::default(),
            downscaled_from: None,
        }
    }

    pub fn with_options(mut self, opts: RecorderOptions) -> Self {
        self.opts = opts;
        self
    }

    pub fn plan(&self) -> &CapturePlan {
        &self.plan
    }

    /// Start and stop latencies measured so far (REC-5.AC1, AC2).
    pub fn latency(&self) -> Latency {
        let mut l = self.latency;
        if let Some(r) = &self.running {
            l.first_frame_ms = *lock(&r.shared.first_frame_ms);
        }
        l
    }

    /// Live counters and health, `None` before `start`.
    pub fn status(&self) -> Option<ScreenStatus> {
        let r = self.running.as_ref()?;
        let now = r.shared.clock.now();
        let p = lock(&r.shared.pipeline);
        Some(ScreenStatus {
            stats: p.stats().clone(),
            late_pct: p.late_pct(),
            recent_late_pct: p.recent_late_pct(now),
            fragments: p.fragments_written(),
            halted: p.halted().cloned(),
            disk_left_minutes: *lock(&r.shared.disk_minutes),
        })
    }

    /// Why the recorder stopped writing on its own, if it did.
    pub fn halted(&self) -> Option<HaltReason> {
        let r = self.running.as_ref()?;
        let h = lock(&r.shared.pipeline).halted().cloned();
        h
    }

    /// Runs one heartbeat now (tests; the background thread does this when enabled).
    pub fn tick(&self, with_checks: bool) {
        if let Some(r) = &self.running {
            r.shared.tick(with_checks);
        }
    }
}

impl<S: ScreenSource, E: VideoEncoder<Frame = S::Frame>> TrackRecorder for ScreenRecorder<S, E> {
    fn kind(&self) -> TrackKind {
        TrackKind::Screen
    }

    fn start(&mut self, ctx: TrackContext) -> Result<(), CmError> {
        if self.running.is_some() {
            return Err(CmError::busy("A recording is already running"));
        }
        let call = Instant::now();
        // Permission first: nothing is created and no prompt appears when it is missing.
        self.source.check_permission()?;
        let bitrate = self.plan.video.bitrate_bps;
        if let Some(free) = self.opts.free_space.free_bytes(&ctx.dir) {
            if let DiskStatus::Full { free_bytes } = assess_disk(free, bitrate) {
                return Err(CmError::disk_full(format!(
                    "Only {} MB free; recording needs more than 1 GB free to start",
                    free_bytes / 1_000_000
                )));
            }
        }
        let path = ctx.dir.join(&self.opts.file_name);
        // A Mac whose encoder rejects the asked-for size at every setting (an Intel MacBook Pro
        // with a scaled display, GitHub issue 16) records at a smaller one rather than not at
        // all. The capture is configured after this, so it delivers frames of the size that worked.
        let asked = self.plan.video;
        let mut tries = std::iter::once(asked).chain(smaller_plans(&asked));
        let enc = loop {
            let Some(candidate) = tries.next() else {
                return Err(cm_encode::hardware_encoder_unavailable());
            };
            match (self.make_encoder)(&EncoderConfig {
                path: path.clone(),
                plan: candidate,
            }) {
                Ok(enc) => {
                    if candidate != asked {
                        log::warn!(
                            "the encoder rejected {}x{}; recording at {}x{} instead",
                            asked.width,
                            asked.height,
                            candidate.width,
                            candidate.height
                        );
                        self.plan.video = candidate;
                        self.downscaled_from = Some((asked.width, asked.height));
                    }
                    break enc;
                }
                Err(e) if is_hardware_encoder_unavailable(&e) => {
                    log::warn!(
                        "no encoder setup worked at {}x{}",
                        candidate.width,
                        candidate.height
                    );
                    continue;
                }
                Err(e) => return Err(e),
            }
        };
        let bitrate = self.plan.video.bitrate_bps;
        let shared = Arc::new(Shared {
            pipeline: Mutex::new(Pipeline::new(enc, self.plan.video.fps)),
            clock: ctx.clock.clone(),
            started: call,
            first_frame_ms: Mutex::new(None),
            listener: self.opts.listener.clone(),
            degraded: AtomicBool::new(false),
            low_sent: AtomicBool::new(false),
            halt_sent: AtomicBool::new(false),
            dir: ctx.dir.clone(),
            bitrate_bps: bitrate,
            free_space: Arc::clone(&self.opts.free_space),
            disk_minutes: Mutex::new(None),
        });
        let sink_shared = Arc::clone(&shared);
        if let Err(e) = self
            .source
            .start(&self.plan, Box::new(move |ev| sink_shared.on_event(ev)))
        {
            self.source.stop();
            let _ = lock(&shared.pipeline).finish(ctx.clock.now());
            let _ = std::fs::remove_file(&path);
            return Err(e);
        }
        let stop_flag = Arc::new(AtomicBool::new(false));
        let heartbeat = self.opts.heartbeat.then(|| {
            let (sh, flag, every) = (
                Arc::clone(&shared),
                Arc::clone(&stop_flag),
                self.opts.check_every,
            );
            let beat = Duration::from_micros(
                (1_000_000 / u64::from(self.plan.video.fps.max(1)) / 2).max(4000),
            );
            std::thread::Builder::new()
                .name("cm-screen-heartbeat".into())
                .spawn(move || {
                    let mut last_check = Instant::now();
                    while !flag.load(Ordering::SeqCst) {
                        std::thread::sleep(beat);
                        let checks = last_check.elapsed() >= every;
                        if checks {
                            last_check = Instant::now();
                        }
                        sh.tick(checks);
                    }
                })
                .ok()
        });
        self.latency = Latency {
            start_call_ms: call.elapsed().as_secs_f64() * 1e3,
            ..Latency::default()
        };
        self.running = Some(Running {
            shared,
            stop_flag,
            heartbeat: heartbeat.flatten(),
        });
        Ok(())
    }

    fn pause(&mut self, at: MediaTime) -> Result<(), CmError> {
        let r = self
            .running
            .as_ref()
            .ok_or_else(|| CmError::invalid_input("The screen track is not recording"))?;
        let res = lock(&r.shared.pipeline).pause(at);
        res
    }

    fn resume(&mut self, at: MediaTime) -> Result<(), CmError> {
        let r = self
            .running
            .as_ref()
            .ok_or_else(|| CmError::invalid_input("The screen track is not recording"))?;
        let res = lock(&r.shared.pipeline).resume(at);
        res
    }

    fn stop(&mut self, at: MediaTime) -> Result<TrackInfo, CmError> {
        let call = Instant::now();
        let run = self
            .running
            .take()
            .ok_or_else(|| CmError::invalid_input("The screen track is not recording"))?;
        self.source.stop();
        run.stop_flag.store(true, Ordering::SeqCst);
        if let Some(h) = run.heartbeat {
            let _ = h.join();
        }
        let shared = run.shared;
        let mut p = lock(&shared.pipeline);
        let finished = p.finish(at);
        self.last_stats = p.stats().track_stats();
        let summary = finished?;
        let start = p.first_frame_session_time().unwrap_or(at);
        let end = at.max(start);
        let span = p.file_span();
        let stats = p.stats().clone();
        let gaps = p.gaps();
        // R3-5: a disk that failed after the last frame was appended shows only here, in the
        // writer's summary; the file is kept up to its last fragment either way.
        let halted = p.halted().cloned().or_else(|| {
            summary.halt.as_ref().map(|h| match h.reason {
                "disk_full" | "disk_slow" => HaltReason::DiskFull,
                _ => HaltReason::EncoderFailed(h.message.clone()),
            })
        });
        let late_pct = p.late_pct();
        // A Mac that rejected the asked-for settings may have recorded with simpler ones (another
        // codec): the manifest says what the file really holds.
        let used_plan = p.encoder().used_plan();
        drop(p);
        self.latency.first_frame_ms = *lock(&shared.first_frame_ms);
        self.latency.stop_ms = Some(call.elapsed().as_secs_f64() * 1e3);

        let plan = &self.plan;
        let fragment_secs: Vec<f64> = summary
            .fragments
            .iter()
            .map(|f| f.duration.ticks() as f64 / TICKS_PER_SECOND as f64)
            .collect();
        let used = used_plan.unwrap_or(plan.video);
        let details = json!({
            "size": [used.width, used.height],
            "downscaled_from": self.downscaled_from,
            "fps": plan.video.fps,
            "codec": used.codec.as_str(),
            "bitrate_bps": used.bitrate_bps,
            "quality": plan.video.quality,
            "source_size": [plan.source_size.width, plan.source_size.height],
            "scale": plan.scale.get(),
            "target": target_json(plan),
            "cursor": plan.show_cursor,
            "file_start": span.map(|s| s.0.ticks()),
            "file_end": span.map(|s| s.1.ticks()),
            "gaps": gaps,
            "fragments": summary.fragments.len(),
            "fragment_secs": fragment_secs,
            "bytes": summary.bytes,
            "repeated": stats.repeated,
            "coalesced": stats.coalesced,
            "idle": stats.idle,
            "paused_drops": stats.paused_drops,
            "missed_slots": stats.missed_slots,
            "stalls": stats.stalls,
            "stall_ms": stats.stall_ms,
            "late_pct": late_pct,
            "latency_ms": self.latency,
            "stopped_early": halted,
            "writer_halt": summary.halt.as_ref().map(|h| json!({
                "reason": h.reason, "message": h.message,
            })),
        });
        Ok(TrackInfo {
            kind: TrackKind::Screen,
            file: self.opts.file_name.clone(),
            start,
            end,
            details,
            stats: self.last_stats.clone(),
        })
    }

    fn stats(&self) -> TrackStats {
        match &self.running {
            Some(r) => lock(&r.shared.pipeline).stats().track_stats(),
            None => self.last_stats.clone(),
        }
    }
}

fn target_json(plan: &CapturePlan) -> serde_json::Value {
    use crate::CaptureTarget;
    match plan.target {
        CaptureTarget::Display { display_id } => {
            json!({"type": "display", "display_id": display_id})
        }
        CaptureTarget::Window { window_id } => json!({"type": "window", "window_id": window_id}),
        CaptureTarget::Region { display_id, .. } => json!({
            "type": "region",
            "display_id": display_id,
            "rect": plan.crop.map(|c| [i64::from(c.origin.x), i64::from(c.origin.y), i64::from(c.size.width), i64::from(c.size.height)]),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::disk::FakeFreeSpace;
    use crate::record::fake::{FakeScreenHandle, FakeScreenSource};
    use crate::record::plan::{plan_capture, ScreenSettings};
    use crate::{CaptureTarget, DisplayInfo, Targets};
    use cm_core::{
        ErrorCode, ManualHostClock, PhysicalPoint, PhysicalRect, PhysicalSize, ScaleFactor,
    };
    use cm_encode::synthetic::{SyntheticEncoder, SyntheticFrame};
    use cm_encode::{scan_file, VideoSettings};

    type Rec = ScreenRecorder<FakeScreenSource, SyntheticEncoder>;

    fn targets() -> Targets {
        Targets {
            displays: vec![DisplayInfo {
                id: 1,
                name: "d".into(),
                bounds: PhysicalRect {
                    origin: PhysicalPoint { x: 0, y: 0 },
                    size: PhysicalSize {
                        width: 1920,
                        height: 1080,
                    },
                },
                scale: ScaleFactor::new(1.0).unwrap(),
                is_primary: true,
                is_builtin: false,
            }],
            windows: vec![],
        }
    }

    struct Rig {
        rec: Rec,
        handle: FakeScreenHandle,
        host: Arc<ManualHostClock>,
        dir: tempfile::TempDir,
        ctx: TrackContext,
        events: Arc<Mutex<Vec<RecorderEvent>>>,
        free: Arc<FakeFreeSpace>,
    }

    const ORIGIN_NS: u64 = 5_000_000_000;

    fn rig(fps: u32) -> Rig {
        let mut s = ScreenSettings::new(CaptureTarget::Display { display_id: 1 });
        s.video = VideoSettings {
            fps,
            ..Default::default()
        };
        let plan = plan_capture(&s, &targets()).unwrap();
        let (src, handle) = FakeScreenSource::new();
        let events = Arc::new(Mutex::new(Vec::new()));
        let ev2 = Arc::clone(&events);
        let free = Arc::new(FakeFreeSpace::new(500_000_000_000));
        let rec = ScreenRecorder::new(plan, src, SyntheticEncoder::create).with_options(
            RecorderOptions {
                heartbeat: false,
                free_space: free.clone(),
                listener: Some(Arc::new(move |e| ev2.lock().unwrap().push(e))),
                ..Default::default()
            },
        );
        let host = Arc::new(ManualHostClock::new(ORIGIN_NS));
        let dir = tempfile::tempdir().unwrap();
        let ctx = TrackContext {
            dir: dir.path().to_owned(),
            clock: SessionClock::with_host(host.clone()),
        };
        Rig {
            rec,
            handle,
            host,
            dir,
            ctx,
            events,
            free,
        }
    }

    impl Rig {
        /// Delivers one frame stamped `ms` after the origin, 8 ms later on the host clock.
        fn frame_at(&self, ms: u64, id: u64) {
            self.host.set_ns(ORIGIN_NS + ms * 1_000_000 + 8_000_000);
            self.handle.frame(ORIGIN_NS + ms * 1_000_000, id);
        }
        fn advance_to(&self, ms: u64) {
            self.host.set_ns(ORIGIN_NS + ms * 1_000_000);
        }
    }

    fn mt(ms: i64) -> MediaTime {
        MediaTime::from_millis(ms)
    }

    #[test]
    fn issue16_an_encoder_that_rejects_the_size_gets_a_smaller_one() {
        let r = rig(30);
        let plan = r.rec.plan().clone();
        let asked = plan.video.width;
        let tries = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&tries);
        let (src, _h) = FakeScreenSource::new();
        let mut rec: Rec = ScreenRecorder::new(plan, src, move |cfg: &EncoderConfig| {
            seen.lock().unwrap().push(cfg.plan.width);
            if cfg.plan.width >= asked {
                Err(cm_encode::hardware_encoder_unavailable())
            } else {
                SyntheticEncoder::create(cfg)
            }
        });
        rec.start(r.ctx.clone()).unwrap();
        let used = rec.plan().video;
        assert!(used.width < asked, "{} < {asked}", used.width);
        assert!(used.bitrate_bps > 0);
        let tries = tries.lock().unwrap().clone();
        assert_eq!(tries, vec![asked, used.width]);
        let info = rec.stop(mt(1000)).unwrap();
        assert_eq!(info.details["downscaled_from"][0], serde_json::json!(asked));
    }

    #[test]
    fn issue16_when_every_size_is_rejected_the_error_is_the_encoder_one() {
        let r = rig(30);
        let plan = r.rec.plan().clone();
        let (src, _h) = FakeScreenSource::new();
        let mut rec: Rec = ScreenRecorder::new(plan, src, |_: &EncoderConfig| {
            Err::<SyntheticEncoder, _>(cm_encode::hardware_encoder_unavailable())
        });
        let e = rec.start(r.ctx.clone()).unwrap_err();
        assert!(cm_encode::is_hardware_encoder_unavailable(&e));
    }

    #[test]
    fn rec1_missing_permission_fails_before_anything_starts() {
        let mut r = rig(30);
        let (src, _h) = FakeScreenSource::without_permission();
        let plan = r.rec.plan().clone();
        let mut rec: Rec = ScreenRecorder::new(plan, src, SyntheticEncoder::create);
        let e = rec.start(r.ctx.clone()).unwrap_err();
        assert_eq!(e.code(), ErrorCode::PermissionMissing);
        assert_eq!(e.data().unwrap()["permission"], serde_json::json!("screen"));
        // Nothing was created in the bundle directory.
        assert_eq!(std::fs::read_dir(r.dir.path()).unwrap().count(), 0);
        let _ = &mut r;
    }

    #[test]
    fn rec1_ac1_ten_seconds_at_30fps_is_300_frames_and_10_seconds() {
        let mut r = rig(30);
        r.rec.start(r.ctx.clone()).unwrap();
        for i in 0..300u64 {
            r.frame_at(i * 1000 / 30, i);
        }
        r.advance_to(10_000);
        let info = r.rec.stop(mt(10_000)).unwrap();
        assert_eq!(info.kind, TrackKind::Screen);
        assert_eq!(info.file, "screen.mp4");
        assert_eq!(info.stats.written, 300);
        assert_eq!(info.stats.late, 0);
        let scan = scan_file(&r.dir.path().join("screen.mp4")).unwrap();
        assert!(scan.is_clean());
        assert_eq!(scan.frame_count(), 300);
        assert!((scan.duration().unwrap().as_secs_f64() - 10.0).abs() < 1.0 / 30.0 + 1e-6);
        // 2 s fragments (REC-8).
        assert_eq!(scan.fragments.len(), 5);
        assert_eq!(info.details["fragments"], 5);
        assert_eq!(info.details["size"], serde_json::json!([1920, 1080]));
        assert_eq!(info.details["codec"], "h264");
    }

    #[test]
    fn rec4_pause_and_resume_write_one_file_with_continuous_time() {
        let mut r = rig(30);
        r.rec.start(r.ctx.clone()).unwrap();
        for i in 0..300u64 {
            r.frame_at(i * 1000 / 30, i);
        }
        r.rec.pause(mt(10_000)).unwrap();
        // The stream keeps running while paused; those frames must not be written.
        for i in 0..50u64 {
            r.frame_at(10_000 + i * 400, 900 + i);
        }
        r.rec.resume(mt(30_000)).unwrap();
        for i in 0..300u64 {
            r.frame_at(30_000 + i * 1000 / 30, 2000 + i);
        }
        r.advance_to(40_000);
        let info = r.rec.stop(mt(40_000)).unwrap();
        let scan = scan_file(&r.dir.path().join("screen.mp4")).unwrap();
        // AC1: 20 s +- 1 frame.
        assert!((scan.duration().unwrap().as_secs_f64() - 20.0).abs() <= 1.0 / 30.0 + 1e-6);
        assert_eq!(scan.frame_count(), 600);
        // The gap is reported for the session so other tracks can line up.
        assert_eq!(info.details["gaps"][0]["from"], 10_000 * 120);
        assert_eq!(info.details["gaps"][0]["to"], 30_000 * 120);
        assert_eq!(info.details["paused_drops"], 50);
        // Session time of the track spans the whole 40 s.
        assert_eq!(info.end, mt(40_000));
    }

    /// R3-7: a source that reports idle samples and then goes silent for over a second raises one
    /// `Stalled` event and counts the stall; a static screen with idle samples does not.
    #[test]
    fn r3_7_a_silent_capture_is_a_stall_event_not_a_static_screen() {
        let mut r = rig(30);
        r.rec.start(r.ctx.clone()).unwrap();
        for i in 0..30u64 {
            r.frame_at(i * 1000 / 30, i);
        }
        // Static for 3 s, with idle samples: alive.
        for i in 1..=12u64 {
            r.host.set_ns(ORIGIN_NS + (1000 + i * 250) * 1_000_000);
            r.handle.idle(ORIGIN_NS + (1000 + i * 250) * 1_000_000);
            r.rec.tick(false);
        }
        assert!(!r
            .events
            .lock()
            .unwrap()
            .iter()
            .any(|e| matches!(e, RecorderEvent::Stalled { .. })));
        // Then nothing at all for 2.5 s (display asleep, SCK stuck).
        r.advance_to(4000 + 1200);
        r.rec.tick(false);
        r.advance_to(4000 + 2500);
        r.rec.tick(false);
        r.rec.tick(false);
        let stalls: Vec<_> = r
            .events
            .lock()
            .unwrap()
            .iter()
            .filter(|e| matches!(e, RecorderEvent::Stalled { .. }))
            .cloned()
            .collect();
        assert_eq!(stalls.len(), 1, "reported once: {stalls:?}");
        let info = r.rec.stop(mt(6500)).unwrap();
        assert_eq!(info.details["stalls"], 1);
        assert!(info.details["stall_ms"].as_u64().unwrap() >= 2400);
    }

    /// R3-5: stopping before the first frame is the "no frames" error (the owner deletes the
    /// empty bundle), not a generic failure; the hardware encoder does the same.
    #[test]
    fn r3_5_stopping_before_the_first_frame_is_the_no_frames_error() {
        let mut s = ScreenSettings::new(CaptureTarget::Display { display_id: 1 });
        s.video = VideoSettings {
            fps: 30,
            ..Default::default()
        };
        let plan = plan_capture(&s, &targets()).unwrap();
        let (src, _h) = FakeScreenSource::new();
        let mut rec: Rec = ScreenRecorder::new(plan, src, |cfg| {
            let mut e = SyntheticEncoder::create(cfg)?;
            e.fail_empty = true;
            Ok(e)
        });
        let dir = tempfile::tempdir().unwrap();
        let ctx = TrackContext {
            dir: dir.path().to_owned(),
            clock: SessionClock::with_host(Arc::new(ManualHostClock::new(ORIGIN_NS))),
        };
        rec.start(ctx).unwrap();
        let e = rec.stop(mt(500)).unwrap_err();
        assert!(cm_encode::is_no_frames(&e), "{e}");
    }

    #[test]
    fn start_twice_is_busy() {
        let mut r = rig(30);
        r.rec.start(r.ctx.clone()).unwrap();
        let e = r.rec.start(r.ctx.clone()).unwrap_err();
        assert_eq!(e.code(), ErrorCode::Busy);
        assert!(e.to_string().contains("already running"));
    }

    #[test]
    fn prm1_revoked_screen_permission_halts_typed_and_keeps_a_clean_file() {
        let mut r = rig(30);
        r.rec.start(r.ctx.clone()).unwrap();
        for i in 0..60u64 {
            r.frame_at(i * 1000 / 30, i);
        }
        r.handle.revoke_permission();
        r.frame_at(2100, 999);
        assert!(matches!(
            r.rec.halted(),
            Some(HaltReason::PermissionRevoked(_))
        ));
        let info = r.rec.stop(mt(2000)).unwrap();
        assert_eq!(info.stats.written, 60);
        assert_eq!(
            info.details["stopped_early"]["reason"],
            "permission_revoked"
        );
        assert!(r
            .events
            .lock()
            .unwrap()
            .iter()
            .any(|e| matches!(e, RecorderEvent::Halted(HaltReason::PermissionRevoked(_)))));
        assert!(scan_file(&r.dir.path().join("screen.mp4"))
            .unwrap()
            .is_clean());
    }

    #[test]
    fn rec1_edge_closed_window_keeps_what_was_captured() {
        let mut r = rig(30);
        r.rec.start(r.ctx.clone()).unwrap();
        for i in 0..90u64 {
            r.frame_at(i * 1000 / 30, i);
        }
        r.handle.close_target("The window was closed");
        // Frames after the close are ignored.
        r.frame_at(3100, 999);
        assert!(matches!(r.rec.halted(), Some(HaltReason::SourceStopped(_))));
        let info = r.rec.stop(mt(3000)).unwrap();
        assert_eq!(info.stats.written, 90);
        assert_eq!(info.details["stopped_early"]["reason"], "source_stopped");
        assert!(r
            .events
            .lock()
            .unwrap()
            .iter()
            .any(|e| matches!(e, RecorderEvent::Halted(HaltReason::SourceStopped(_)))));
        assert!(scan_file(&r.dir.path().join("screen.mp4"))
            .unwrap()
            .is_clean());
    }

    #[test]
    fn rec8_ac3_disk_full_stops_writing_and_the_file_is_valid() {
        let mut r = rig(30);
        r.rec.start(r.ctx.clone()).unwrap();
        for i in 0..60u64 {
            r.frame_at(i * 1000 / 30, i);
        }
        r.free.set(900_000_000);
        r.advance_to(2000);
        r.rec.tick(true);
        assert_eq!(r.rec.halted(), Some(HaltReason::DiskFull));
        for i in 60..90u64 {
            r.frame_at(i * 1000 / 30, i);
        }
        let info = r.rec.stop(mt(3000)).unwrap();
        assert_eq!(info.stats.written, 60);
        assert!(r
            .events
            .lock()
            .unwrap()
            .iter()
            .any(|e| matches!(e, RecorderEvent::DiskFull { .. })));
        assert!(scan_file(&r.dir.path().join("screen.mp4"))
            .unwrap()
            .is_clean());
    }

    #[test]
    fn rec8_ac4_low_space_warns_once_and_keeps_recording() {
        let mut r = rig(30);
        // 1920x1080 H.264 30 fps = 16 Mbps = 120 MB/min; 1.1 GB free is 9.2 minutes (> 1 GB stop
        // threshold, < 10 minutes warning threshold).
        r.free.set(1_100_000_000);
        r.rec.start(r.ctx.clone()).unwrap();
        r.frame_at(0, 0);
        r.rec.tick(true);
        r.rec.tick(true);
        let low: Vec<_> = r
            .events
            .lock()
            .unwrap()
            .iter()
            .filter(|e| matches!(e, RecorderEvent::DiskLow { .. }))
            .cloned()
            .collect();
        assert_eq!(low.len(), 1);
        assert!(r.rec.halted().is_none());
        assert!(r.rec.status().unwrap().disk_left_minutes.unwrap() < 10.0);
    }

    #[test]
    fn starting_with_under_1gb_free_is_disk_full() {
        let mut r = rig(30);
        r.free.set(500_000_000);
        let e = r.rec.start(r.ctx.clone()).unwrap_err();
        assert_eq!(e.code(), ErrorCode::DiskFull);
    }

    #[test]
    fn rec3_ac4_degraded_health_is_reported_then_clears() {
        let mut r = rig(60);
        r.rec.start(r.ctx.clone()).unwrap();
        // 10 s of clean delivery.
        for i in 0..600u64 {
            r.frame_at(i * 1000 / 60, i);
        }
        r.rec.tick(true);
        assert!(r.events.lock().unwrap().is_empty());
        // 5 s where every 10th frame is delayed 60 ms: >2% late.
        for i in 600..900u64 {
            let ms = i * 1000 / 60;
            let delay = if i % 10 == 0 { 60 } else { 8 };
            r.host
                .set_ns(ORIGIN_NS + ms * 1_000_000 + delay * 1_000_000);
            r.handle.frame(ORIGIN_NS + ms * 1_000_000, i);
        }
        r.rec.tick(true);
        assert!(r
            .events
            .lock()
            .unwrap()
            .iter()
            .any(|e| matches!(e, RecorderEvent::Degraded { .. })));
        let st = r.rec.status().unwrap();
        assert!(st.recent_late_pct > 2.0, "{st:?}");
    }

    #[test]
    fn latency_is_measured() {
        let mut r = rig(30);
        r.rec.start(r.ctx.clone()).unwrap();
        r.frame_at(0, 0);
        let l = r.rec.latency();
        assert!(l.start_call_ms >= 0.0);
        assert!(l.first_frame_ms.is_some());
        let _ = r.rec.stop(mt(100)).unwrap();
        assert!(r.rec.latency().stop_ms.is_some());
    }

    #[test]
    fn stop_without_start_is_an_error_and_start_failure_leaves_no_file() {
        let mut r = rig(30);
        assert!(r.rec.stop(mt(0)).is_err());
        let plan = r.rec.plan().clone();
        let (src, _h) = FakeScreenSource::new();
        let mut bad: Rec = ScreenRecorder::new(plan, src, |_| {
            Err::<SyntheticEncoder, _>(cm_encode::hardware_encoder_unavailable())
        });
        assert!(bad.start(r.ctx.clone()).is_err());
        assert_eq!(std::fs::read_dir(r.dir.path()).unwrap().count(), 0);
        let _ = SyntheticFrame(0);
    }

    #[test]
    fn exclusions_and_cursor_reach_the_source() {
        let mut s = ScreenSettings::new(CaptureTarget::Display { display_id: 1 });
        s.show_cursor = false;
        s.exclusions.window_ids = vec![42];
        s.exclusions.pids = vec![std::process::id()];
        let plan = plan_capture(&s, &targets()).unwrap();
        let (src, handle) = FakeScreenSource::new();
        let mut rec: Rec = ScreenRecorder::new(plan, src, SyntheticEncoder::create);
        let dir = tempfile::tempdir().unwrap();
        let host = Arc::new(ManualHostClock::new(0));
        rec.start(TrackContext {
            dir: dir.path().to_owned(),
            clock: SessionClock::with_host(host),
        })
        .unwrap();
        let p = handle.plan().unwrap();
        assert!(!p.show_cursor);
        assert_eq!(p.exclusions.window_ids, vec![42]);
        assert_eq!(p.exclusions.pids, vec![std::process::id()]);
    }
}
