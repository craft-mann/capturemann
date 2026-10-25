//! A scriptable [`SourceFactory`] for tests: tracks that write a few bytes, record what was done to
//! them and can be told to fail, halt or warn. No display, no devices.
//!
//! ```ignore
//! let sources = FakeSources::new();
//! let daemon = builder.recording_sources(sources.clone()).build();
//! // ... start a recording through the daemon ...
//! sources.handle(TrackKind::Screen).unwrap().halt("screen_halted", "window closed");
//! ```

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use cm_audio::{LevelHandle, LevelMeter};
use cm_core::{CmError, MediaTime, Permission, TrackContext, TrackInfo, TrackKind, TrackStats};
use serde_json::{json, Value};

use crate::recording::{
    BuildContext, DeviceNames, NoteLevel, Preflight, PreflightWarning, SensorRequest, SessionTrack,
    SourceFactory, TrackNote,
};

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

#[derive(Default)]
struct TrackState {
    calls: Mutex<Vec<String>>,
    notes: Mutex<Vec<TrackNote>>,
    fail_stop: AtomicBool,
    /// `stop` fails like an encoder that never got a frame (R3-5).
    no_frames: AtomicBool,
    /// `stop` takes this many milliseconds (a slow encoder finish).
    stop_delay_ms: std::sync::atomic::AtomicU64,
    panic_in_poll: AtomicBool,
    meter: Mutex<Option<LevelMeter>>,
    level: Mutex<Option<LevelHandle>>,
}

/// The test's hold on one fake track.
#[derive(Clone)]
pub struct FakeTrackHandle {
    kind: TrackKind,
    state: Arc<TrackState>,
}

impl FakeTrackHandle {
    pub fn kind(&self) -> TrackKind {
        self.kind
    }

    /// `start`, `pause`, `resume`, `stop`, in the order they were called.
    pub fn calls(&self) -> Vec<String> {
        lock(&self.state.calls).clone()
    }

    /// The track reports it cannot continue (window closed, disk full): the engine stops.
    pub fn halt(&self, code: &str, message: &str) {
        lock(&self.state.notes).push(TrackNote::new(NoteLevel::StopRequested, code, message));
    }

    /// The track reports a warning (device lost, ...).
    pub fn warn(&self, code: &str, message: &str) {
        lock(&self.state.notes).push(TrackNote::new(NoteLevel::Warning, code, message));
    }

    /// The track itself notices that the OS removed its permission (a ScreenCaptureKit
    /// "user declined TCCs" stop, an AVCapture not-authorized error): it raises the typed
    /// `permission_revoked` note, a stop request for the screen and a warning otherwise.
    pub fn revoke_permission(&self, p: Permission) {
        lock(&self.state.notes).push(TrackNote::permission_revoked(p));
    }

    /// `stop` fails (the bundle keeps its lock for recovery).
    pub fn fail_stop(&self, fail: bool) {
        self.state.fail_stop.store(fail, Ordering::SeqCst);
    }

    /// `stop` fails with the "no frames were recorded" error (the user stopped before the first
    /// frame); the session deletes the empty bundle (R3-5).
    pub fn stop_before_first_frame(&self, on: bool) {
        self.state.no_frames.store(on, Ordering::SeqCst);
    }

    /// `stop` takes `ms` milliseconds (a hardware encoder that is slow to finish, R3-10).
    pub fn slow_stop(&self, ms: u64) {
        self.state.stop_delay_ms.store(ms, Ordering::SeqCst);
    }

    /// `poll` (called from the session's tick) panics.
    pub fn panic_in_poll(&self, panic: bool) {
        self.state.panic_in_poll.store(panic, Ordering::SeqCst);
    }

    /// Drives the live meter of an audio track to `amplitude` (0..=1 linear).
    pub fn set_level(&self, amplitude: f32) {
        if let Some(m) = lock(&self.state.meter).as_mut() {
            m.push(&vec![amplitude; 4_800]);
        }
    }
}

/// What a fake screen track reports at the end of a run (the 60 fps self-test reads it).
#[derive(Debug, Clone, Copy, PartialEq)]
struct ScreenQuality {
    late_pct: f64,
    dropped: u64,
    /// Capture stall time the fake screen reports (R3-7).
    stall_ms: u64,
}

struct FakeTrack {
    quality: ScreenQuality,
    kind: TrackKind,
    start_error: Option<CmError>,
    state: Arc<TrackState>,
    dir: Option<std::path::PathBuf>,
    started: bool,
}

impl FakeTrack {
    fn file(&self) -> &'static str {
        match self.kind {
            TrackKind::Screen => "screen.mp4",
            TrackKind::Camera => "camera.mp4",
            TrackKind::Mic => "mic.wav",
            TrackKind::System => "system.wav",
            TrackKind::Cursor => "cursor.jsonl",
        }
    }

    fn log(&self, what: &str) {
        lock(&self.state.calls).push(what.to_owned());
    }
}

impl SessionTrack for FakeTrack {
    fn kind(&self) -> TrackKind {
        self.kind
    }

    fn required(&self) -> bool {
        self.kind == TrackKind::Screen
    }

    fn start(&mut self, ctx: TrackContext) -> Result<(), CmError> {
        self.log("start");
        if let Some(e) = self.start_error.take() {
            return Err(e);
        }
        std::fs::write(ctx.dir.join(self.file()), b"fake media")
            .map_err(|e| CmError::internal(format!("fake track cannot write: {e}")))?;
        self.dir = Some(ctx.dir);
        self.started = true;
        Ok(())
    }

    fn pause(&mut self, _at: MediaTime) -> Result<(), CmError> {
        self.log("pause");
        Ok(())
    }

    fn resume(&mut self, _at: MediaTime) -> Result<(), CmError> {
        self.log("resume");
        Ok(())
    }

    fn stop(&mut self, at: MediaTime) -> Result<TrackInfo, CmError> {
        self.log("stop");
        let delay = self.state.stop_delay_ms.load(Ordering::SeqCst);
        if delay > 0 {
            std::thread::sleep(std::time::Duration::from_millis(delay));
        }
        if self.state.no_frames.load(Ordering::SeqCst) {
            return Err(cm_encode::no_frames_error());
        }
        if self.state.fail_stop.load(Ordering::SeqCst) {
            return Err(CmError::internal("fake track failed to stop"));
        }
        Ok(TrackInfo {
            kind: self.kind,
            file: self.file().to_owned(),
            start: MediaTime::ZERO,
            end: at,
            details: if self.kind == TrackKind::Screen {
                json!({ "fake": true, "late_pct": self.quality.late_pct,
                        "stalls": u64::from(self.quality.stall_ms > 0),
                        "stall_ms": self.quality.stall_ms })
            } else {
                json!({ "fake": true })
            },
            stats: if self.kind == TrackKind::Screen {
                // 600 slots: what a 10 s run at 60 fps writes.
                TrackStats {
                    delivered: 600,
                    written: 600,
                    dropped: self.quality.dropped,
                    late: (self.quality.late_pct * 6.0).round() as u64,
                }
            } else {
                TrackStats {
                    delivered: 10,
                    written: 10,
                    dropped: 0,
                    late: 0,
                }
            },
        })
    }

    fn stats(&self) -> TrackStats {
        TrackStats {
            delivered: 10,
            written: 10,
            ..TrackStats::default()
        }
    }

    fn poll(&mut self) -> Vec<TrackNote> {
        if self.state.panic_in_poll.load(Ordering::SeqCst) {
            panic!("injected panic in a fake track");
        }
        std::mem::take(&mut *lock(&self.state.notes))
    }

    fn level(&self) -> Option<LevelHandle> {
        lock(&self.state.level).clone()
    }

    fn live(&self) -> Value {
        if self.kind == TrackKind::Screen {
            json!({ "late_pct": self.quality.late_pct, "recent_late_pct": self.quality.late_pct,
                    "fragments": 0,
                    "disk_left_minutes": null, "repeated": 0, "halted": null })
        } else {
            Value::Null
        }
    }

    fn bitrate_bps(&self) -> u64 {
        1_000_000
    }
}

#[derive(Default)]
struct FactoryState {
    missing: Mutex<Vec<Permission>>,
    /// What the OS poll ([`SourceFactory::revoked_permissions`]) reports as taken away.
    revoked: Mutex<Vec<Permission>>,
    cursor_warning: AtomicBool,
    fail_start: Mutex<HashSet<TrackKind>>,
    builds: AtomicUsize,
    tracks: Mutex<Vec<FakeTrackHandle>>,
    panic_on_tick: AtomicBool,
    quality: Mutex<Option<ScreenQuality>>,
    last_plan: Mutex<Option<cm_capture::record::CapturePlan>>,
}

/// The fake [`SourceFactory`]. Cheap to clone; clones share state.
#[derive(Clone, Default)]
pub struct FakeSources {
    state: Arc<FactoryState>,
}

impl FakeSources {
    pub fn new() -> Self {
        Self::default()
    }

    /// The preflight reports this permission as missing (screen, microphone or camera).
    pub fn with_missing_permission(self, p: Permission) -> Self {
        lock(&self.state.missing).push(p);
        self
    }

    /// From now on the permission poll reports `p` as removed by the OS (PRM-1), like a user
    /// switching it off in System Settings during a recording.
    pub fn revoke_permission(&self, p: Permission) {
        let mut r = lock(&self.state.revoked);
        if !r.contains(&p) {
            r.push(p);
        }
    }

    /// Grants every permission again.
    pub fn grant_all(&self) {
        lock(&self.state.missing).clear();
        lock(&self.state.revoked).clear();
    }

    /// The preflight warns that clicks cannot be recorded (Input Monitoring).
    pub fn with_cursor_warning(self) -> Self {
        self.state.cursor_warning.store(true, Ordering::SeqCst);
        self
    }

    /// `start` of this track fails (`device_busy`).
    pub fn with_failing_start(self, kind: TrackKind) -> Self {
        lock(&self.state.fail_start).insert(kind);
        self
    }

    /// Screen tracks built from now on report this much lateness and this many dropped frames
    /// when they stop (drives the 60 fps self-test verdict: more than 1% late fails).
    pub fn with_screen_quality(self, late_pct: f64, dropped: u64) -> Self {
        self.set_screen_quality(late_pct, dropped);
        self
    }

    /// Same as [`FakeSources::with_screen_quality`] on a shared factory.
    pub fn set_screen_quality(&self, late_pct: f64, dropped: u64) {
        let mut q = lock(&self.state.quality);
        let stall_ms = q.map_or(0, |q| q.stall_ms);
        *q = Some(ScreenQuality {
            late_pct,
            dropped,
            stall_ms,
        });
    }

    /// Screen tracks built from now on report this much capture stall (R3-7): more than 500 ms
    /// fails the 60 fps self-test even with 0% late frames.
    pub fn with_screen_stall_ms(self, stall_ms: u64) -> Self {
        let mut q = lock(&self.state.quality);
        let (late_pct, dropped) = q.map_or((0.0, 0), |q| (q.late_pct, q.dropped));
        *q = Some(ScreenQuality {
            late_pct,
            dropped,
            stall_ms,
        });
        drop(q);
        self
    }

    /// The factory's `tick_hook` panics (reaches the engine's driver task).
    pub fn set_panic_on_tick(&self, panic: bool) {
        self.state.panic_on_tick.store(panic, Ordering::SeqCst);
    }

    /// How many times tracks were built.
    pub fn builds(&self) -> usize {
        self.state.builds.load(Ordering::SeqCst)
    }

    /// The screen plan of the latest build (what the real screen track would capture).
    pub fn last_plan(&self) -> Option<cm_capture::record::CapturePlan> {
        lock(&self.state.last_plan).clone()
    }

    /// The track of `kind` from the latest build.
    pub fn handle(&self, kind: TrackKind) -> Option<FakeTrackHandle> {
        lock(&self.state.tracks)
            .iter()
            .rev()
            .find(|h| h.kind == kind)
            .cloned()
    }
}

impl SourceFactory for FakeSources {
    fn preflight(&self, req: &SensorRequest) -> Preflight {
        let mut out = Preflight::default();
        for p in lock(&self.state.missing).iter() {
            let needed = match p {
                Permission::Screen => true,
                Permission::Microphone => req.mic,
                Permission::Camera => req.camera,
                _ => false,
            };
            if needed {
                out.errors.push(CmError::permission_missing(
                    *p,
                    format!("{} access is off (fake)", p.as_str()),
                ));
            }
        }
        if req.cursor && self.state.cursor_warning.load(Ordering::SeqCst) {
            out.warnings.push(PreflightWarning {
                track: Some("cursor".to_owned()),
                code: "input_monitoring_missing".to_owned(),
                message: "Cursor clicks will not be recorded (fake)".to_owned(),
            });
        }
        out
    }

    fn revoked_permissions(&self, running: &[TrackKind]) -> Vec<Permission> {
        lock(&self.state.revoked)
            .iter()
            .filter(|p| {
                running.contains(match p {
                    Permission::Screen => &TrackKind::Screen,
                    Permission::Microphone => &TrackKind::Mic,
                    Permission::Camera => &TrackKind::Camera,
                    _ => return false,
                })
            })
            .copied()
            .collect()
    }

    fn device_names(&self, req: &SensorRequest) -> DeviceNames {
        DeviceNames {
            mic: req.mic.then(|| "Fake Microphone".to_owned()),
            camera: req.camera.then(|| "Fake Camera".to_owned()),
        }
    }

    fn build(&self, cx: &BuildContext<'_>) -> Result<Vec<Box<dyn SessionTrack>>, CmError> {
        self.state.builds.fetch_add(1, Ordering::SeqCst);
        *lock(&self.state.last_plan) = Some(cx.plan.clone());
        let p = cx.params;
        let mut kinds = vec![TrackKind::Screen];
        if p.cursor {
            kinds.push(TrackKind::Cursor);
        }
        if p.mic.is_some() {
            kinds.push(TrackKind::Mic);
        }
        if p.system_audio {
            kinds.push(TrackKind::System);
        }
        if p.camera.is_some() {
            kinds.push(TrackKind::Camera);
        }
        let failing = lock(&self.state.fail_start).clone();
        let quality = (*lock(&self.state.quality)).unwrap_or(ScreenQuality {
            late_pct: 0.0,
            dropped: 0,
            stall_ms: 0,
        });
        let mut handles = lock(&self.state.tracks);
        handles.clear();
        let mut out: Vec<Box<dyn SessionTrack>> = Vec::new();
        for kind in kinds {
            let state = Arc::new(TrackState::default());
            if matches!(kind, TrackKind::Mic | TrackKind::System) {
                let (meter, handle) = LevelMeter::new(48_000, 1, 30.0);
                *lock(&state.meter) = Some(meter);
                *lock(&state.level) = Some(handle);
            }
            handles.push(FakeTrackHandle {
                kind,
                state: state.clone(),
            });
            out.push(Box::new(FakeTrack {
                quality,
                kind,
                start_error: failing
                    .contains(&kind)
                    .then(|| CmError::device_busy(format!("the {} is busy (fake)", kind.as_str()))),
                state,
                dir: None,
                started: false,
            }));
        }
        Ok(out)
    }

    fn devices(&self) -> Value {
        json!({
            "cameras": [{ "id": "fake-cam", "name": "Fake Camera", "kind": "built_in",
                          "is_default": true, "max_size": [1920, 1080], "max_fps": 30.0,
                          "modes": [], "in_use_elsewhere": false }],
            "cameras_error": null,
            "camera_permission": "authorized",
            "mics": [{ "id": "fake-mic", "name": "Fake Microphone", "default": true,
                       "channels": 1, "sample_rate": 48000, "max_channels": 1,
                       "sample_rate_ranges": [] }],
            "microphone_permission": "authorized",
            "microphone_permission_message": null,
            "system_audio": { "supported": true, "backend": "fake", "reason": null,
                              "permission": "authorized", "excludes_own_audio": true },
            "screen_recording_permission": "authorized",
            "cursor": { "clicks": "available", "missing_permission": null },
        })
    }

    fn tick_hook(&self) {
        if self.state.panic_on_tick.load(Ordering::SeqCst) {
            panic!("injected panic in the recording driver");
        }
    }
}
