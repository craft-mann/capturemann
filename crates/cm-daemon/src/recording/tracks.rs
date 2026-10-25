//! The tracks a [`super::RecordingSession`] drives.
//!
//! [`SessionTrack`] wraps one `cm_core::TrackRecorder` with the things the session needs on top
//! of it and the recorder trait does not carry: whether the track is required, a poll for
//! recorder events (device loss, disk warnings, a halted stream), a live meter, the facts for
//! `session.json` and a bitrate estimate for the disk guard. One wrapper per recorder crate, all
//! constructed with the recorder you would use for real (or its fake in tests).

use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex, MutexGuard};

use cm_audio::{AudioEvent, LevelHandle, MicRecorder, SystemAudioRecorder};
use cm_camera::{CameraRecorder, RecorderEvent as CameraEvent};
use cm_capture::record::{
    CapturePlan, HaltReason, RecorderEvent as ScreenEvent, RecorderOptions, ScreenRecorder,
    ScreenSource,
};
use cm_core::{
    CmError, MediaTime, Permission, TrackContext, TrackInfo, TrackKind, TrackRecorder, TrackStats,
};
use cm_cursor::CursorRecorder;
use cm_encode::{EncoderConfig, VideoEncoder};
use serde_json::{json, Value};

/// How serious a note from a track is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteLevel {
    /// For the log and the HUD only.
    Info,
    /// Recorded in the manifest and shown to the user; the recording continues.
    Warning,
    /// The track can no longer record (disk full, screen window closed): the owner should stop
    /// the session. What was captured is kept.
    StopRequested,
}

/// Something a track reports while it runs.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackNote {
    pub level: NoteLevel,
    /// Stable machine-readable code (`disk_low`, `camera_lost`, `screen_halted`, ...).
    pub code: String,
    pub message: String,
    /// Session time, when the recorder knows it.
    pub at: Option<MediaTime>,
    /// The OS permission behind the note (`permission_revoked`), so clients can say which one.
    pub permission: Option<Permission>,
}

impl TrackNote {
    pub fn new(level: NoteLevel, code: &str, message: impl Into<String>) -> Self {
        TrackNote {
            level,
            code: code.to_owned(),
            message: message.into(),
            at: None,
            permission: None,
        }
    }

    pub fn permission(mut self, p: Permission) -> Self {
        self.permission = Some(p);
        self
    }

    /// The note a track raises when the OS withdrew `p` while it ran (PRM-1): the screen track
    /// cannot go on and asks the session to stop (`StopRequested`); the optional camera and
    /// microphone tracks only warn, the screen recording continues.
    pub fn permission_revoked(p: Permission) -> Self {
        let (level, what) = match p {
            Permission::Screen => (NoteLevel::StopRequested, "Screen Recording"),
            Permission::Camera => (NoteLevel::Warning, "Camera"),
            Permission::Microphone => (NoteLevel::Warning, "Microphone"),
            Permission::Accessibility => (NoteLevel::Warning, "Accessibility"),
            Permission::InputMonitoring => (NoteLevel::Warning, "Input Monitoring"),
        };
        let tail = match level {
            NoteLevel::StopRequested => "the recording is stopped and what was recorded is kept",
            _ => "the recording continues without this track from here on",
        };
        TrackNote::new(
            level,
            "permission_revoked",
            format!("{what} permission was removed; {tail}"),
        )
        .permission(p)
    }

    pub fn at(mut self, at: MediaTime) -> Self {
        self.at = Some(at);
        self
    }
}

/// One track of a recording session.
pub trait SessionTrack: Send {
    fn kind(&self) -> TrackKind;

    /// A failing required track aborts the recording; an optional one is recorded as a warning.
    fn required(&self) -> bool {
        false
    }

    fn start(&mut self, ctx: TrackContext) -> Result<(), CmError>;
    fn pause(&mut self, at: MediaTime) -> Result<(), CmError>;
    fn resume(&mut self, at: MediaTime) -> Result<(), CmError>;
    fn stop(&mut self, at: MediaTime) -> Result<TrackInfo, CmError>;
    fn stats(&self) -> TrackStats;

    /// Events since the last call. Called by the session's `tick`.
    fn poll(&mut self) -> Vec<TrackNote> {
        Vec::new()
    }

    /// The live meter (mic and system audio).
    fn level(&self) -> Option<LevelHandle> {
        None
    }

    /// Recorder-specific live numbers for the HUD (screen late %, fragments, ...).
    fn live(&self) -> Value {
        Value::Null
    }

    /// Facts for `session.json` (`devices.<kind>`), available after `start`.
    fn device_info(&self) -> Value {
        Value::Null
    }

    /// Extra top-level `session.json` keys (for example `cursor: {clicks: "unavailable"}`),
    /// available after `start`.
    fn session_extra(&self) -> Option<(String, Value)> {
        None
    }

    /// Notes to raise right after a successful `start` (for example a degraded cursor track).
    fn start_notes(&self) -> Vec<TrackNote> {
        Vec::new()
    }

    /// Estimated disk write rate, bits per second, for the disk guard.
    fn bitrate_bps(&self) -> u64 {
        0
    }

    /// What crash recovery needs that only the running track knows (an audio track's first
    /// sample time and file names, R3-2). The session polls it and copies the first `Some` into
    /// `recording.lock`.
    fn recovery_hint(&self) -> Option<Value> {
        None
    }

    /// Files this track may leave in the bundle, removed when the track fails.
    fn files(&self) -> &'static [&'static str] {
        match self.kind() {
            TrackKind::Screen => &["screen.mp4"],
            TrackKind::Camera => &["camera.mp4"],
            TrackKind::Mic => &["mic.wav", "mic.denoised.wav"],
            TrackKind::System => &["system.wav"],
            TrackKind::Cursor => &["cursor.jsonl", "cursors"],
        }
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

type NoteQueue = Arc<Mutex<Vec<TrackNote>>>;

// ---- screen --------------------------------------------------------------------------------

/// The screen track. Required: if it fails to start the recording is not created, and if it halts
/// by itself (window closed, disk full) the track asks the session to stop.
pub struct ScreenTrack<S: ScreenSource, E: VideoEncoder<Frame = S::Frame>> {
    rec: ScreenRecorder<S, E>,
    notes: NoteQueue,
    bitrate_bps: u64,
}

impl<S: ScreenSource, E: VideoEncoder<Frame = S::Frame>> ScreenTrack<S, E> {
    /// `opts.listener`, if set, keeps receiving the recorder's events too.
    pub fn new(
        plan: CapturePlan,
        source: S,
        make_encoder: impl FnMut(&EncoderConfig) -> Result<E, CmError> + Send + 'static,
        mut opts: RecorderOptions,
    ) -> Self {
        let notes: NoteQueue = Arc::default();
        let q = Arc::clone(&notes);
        let inner = opts.listener.take();
        opts.listener = Some(Arc::new(move |ev: ScreenEvent| {
            if let Some(n) = screen_note(&ev) {
                lock(&q).push(n);
            }
            if let Some(l) = &inner {
                l(ev);
            }
        }));
        let bitrate_bps = plan.video.bitrate_bps;
        ScreenTrack {
            rec: ScreenRecorder::new(plan, source, make_encoder).with_options(opts),
            notes,
            bitrate_bps,
        }
    }

    /// The wrapped recorder (latency, status).
    pub fn recorder(&self) -> &ScreenRecorder<S, E> {
        &self.rec
    }
}

fn screen_note(ev: &ScreenEvent) -> Option<TrackNote> {
    Some(match ev {
        ScreenEvent::DiskLow { minutes_left } => TrackNote::new(
            NoteLevel::Warning,
            "disk_low",
            format!("About {minutes_left:.0} minutes of disk space left"),
        ),
        ScreenEvent::DiskFull { free_bytes } => TrackNote::new(
            NoteLevel::StopRequested,
            "disk_full",
            format!("Recording stopped: only {} MB free", free_bytes / 1_000_000),
        ),
        ScreenEvent::Degraded { late_pct } => TrackNote::new(
            NoteLevel::Warning,
            "screen_degraded",
            format!("{late_pct:.1}% of screen frames were late over the last 30 s"),
        ),
        ScreenEvent::Healthy => TrackNote::new(
            NoteLevel::Info,
            "screen_healthy",
            "Screen capture is keeping up again",
        ),
        ScreenEvent::Stalled { silence_ms } => TrackNote::new(
            NoteLevel::Warning,
            "capture_stalled",
            format!(
                "Screen capture delivered no frames for {:.1} s (display asleep or capture stuck); \
                 the video repeats the last frame until it resumes",
                *silence_ms as f64 / 1000.0
            ),
        ),
        // A halt for a full disk is reported by the DiskFull event just before; do not repeat it.
        ScreenEvent::Halted(HaltReason::DiskFull) => return None,
        ScreenEvent::Halted(HaltReason::PermissionRevoked(_)) => {
            TrackNote::permission_revoked(Permission::Screen)
        }
        ScreenEvent::Halted(HaltReason::SourceStopped(m)) => TrackNote::new(
            NoteLevel::StopRequested,
            "screen_halted",
            format!("Screen capture ended: {m}"),
        ),
        ScreenEvent::Halted(HaltReason::EncoderFailed(m)) => TrackNote::new(
            NoteLevel::StopRequested,
            "screen_encoder_failed",
            format!("Screen encoder failed: {m}"),
        ),
    })
}

impl<S: ScreenSource, E: VideoEncoder<Frame = S::Frame>> SessionTrack for ScreenTrack<S, E> {
    fn kind(&self) -> TrackKind {
        TrackKind::Screen
    }
    fn required(&self) -> bool {
        true
    }
    fn start(&mut self, ctx: TrackContext) -> Result<(), CmError> {
        self.rec.start(ctx)
    }
    fn pause(&mut self, at: MediaTime) -> Result<(), CmError> {
        self.rec.pause(at)
    }
    fn resume(&mut self, at: MediaTime) -> Result<(), CmError> {
        self.rec.resume(at)
    }
    fn stop(&mut self, at: MediaTime) -> Result<TrackInfo, CmError> {
        self.rec.stop(at)
    }
    fn stats(&self) -> TrackStats {
        self.rec.stats()
    }
    fn poll(&mut self) -> Vec<TrackNote> {
        std::mem::take(&mut *lock(&self.notes))
    }
    fn live(&self) -> Value {
        match self.rec.status() {
            Some(s) => json!({
                "late_pct": s.late_pct,
                "recent_late_pct": s.recent_late_pct,
                "fragments": s.fragments,
                "disk_left_minutes": s.disk_left_minutes,
                "repeated": s.stats.repeated,
                "stalls": s.stats.stalls,
                "stall_ms": s.stats.stall_ms,
                "halted": s.halted,
            }),
            None => Value::Null,
        }
    }
    fn device_info(&self) -> Value {
        let p = self.rec.plan();
        json!({
            "display_id": p.display_id,
            "size": [p.video.width, p.video.height],
            "source_size": [p.source_size.width, p.source_size.height],
            "scale": p.scale.get(),
        })
    }
    fn bitrate_bps(&self) -> u64 {
        self.bitrate_bps
    }
}

// ---- audio ---------------------------------------------------------------------------------

fn audio_note(ev: &AudioEvent) -> TrackNote {
    match ev {
        AudioEvent::DeviceFallback { requested, using } => TrackNote::new(
            NoteLevel::Warning,
            "audio_device_fallback",
            format!("Microphone {requested} is not available; using {using}"),
        ),
        AudioEvent::DeviceChanged { at, from, to } => TrackNote::new(
            NoteLevel::Warning,
            "audio_device_changed",
            format!("Audio device changed from {from} to {to}"),
        )
        .at(*at),
        AudioEvent::DeviceLost { at, name } => TrackNote::new(
            NoteLevel::Warning,
            "audio_device_lost",
            format!("Audio device {name} was lost; recording silence"),
        )
        .at(*at),
        AudioEvent::DigitalSilence { at } => TrackNote::new(
            NoteLevel::Warning,
            "mic_silent",
            "The microphone has delivered only silence for 15 s (muted, or its access was \
             revoked); the recording continues",
        )
        .at(*at),
        AudioEvent::WriteFailed { at, message } => TrackNote::new(
            NoteLevel::Warning,
            "track_failed",
            format!("The audio file could not be written ({message}); the track ends here and the recording continues"),
        )
        .at(*at),
    }
}

/// Raw 32-bit float stereo PCM at 48 kHz is the upper bound of what an audio track writes.
const AUDIO_BITRATE_BPS: u64 = 48_000 * 32 * 2;
/// Camera H.264 at the recorder's default bitrate.
const CAMERA_BITRATE_BPS: u64 = 8_000_000;
/// Cursor JSON lines, a few hundred kbit/s at the worst.
const CURSOR_BITRATE_BPS: u64 = 400_000;

/// What a recording of these tracks writes per second, in bits: the disk guard's estimate (the
/// sum of the tracks' own `bitrate_bps`), known before anything runs so the setup panel and the
/// dialog can warn about disk space ahead of the recording (REC-8.AC4).
pub fn planned_bitrate_bps(
    video_bps: u64,
    camera: bool,
    mic: bool,
    system_audio: bool,
    cursor: bool,
) -> u64 {
    video_bps
        + if camera { CAMERA_BITRATE_BPS } else { 0 }
        + if mic { AUDIO_BITRATE_BPS } else { 0 }
        + if system_audio { AUDIO_BITRATE_BPS } else { 0 }
        + if cursor { CURSOR_BITRATE_BPS } else { 0 }
}

macro_rules! audio_track {
    ($name:ident, $rec:ty, $kind:expr, $what:literal) => {
        #[doc = concat!("The ", $what, " track (optional).")]
        pub struct $name {
            rec: $rec,
        }

        impl $name {
            pub fn new(rec: $rec) -> Self {
                $name { rec }
            }
            pub fn recorder(&self) -> &$rec {
                &self.rec
            }
        }

        impl SessionTrack for $name {
            fn kind(&self) -> TrackKind {
                $kind
            }
            fn start(&mut self, ctx: TrackContext) -> Result<(), CmError> {
                self.rec.start(ctx)
            }
            fn pause(&mut self, at: MediaTime) -> Result<(), CmError> {
                self.rec.pause(at)
            }
            fn resume(&mut self, at: MediaTime) -> Result<(), CmError> {
                self.rec.resume(at)
            }
            fn stop(&mut self, at: MediaTime) -> Result<TrackInfo, CmError> {
                self.rec.stop(at)
            }
            fn stats(&self) -> TrackStats {
                self.rec.stats()
            }
            fn poll(&mut self) -> Vec<TrackNote> {
                self.rec.drain_events().iter().map(audio_note).collect()
            }
            fn level(&self) -> Option<LevelHandle> {
                Some(self.rec.level_handle())
            }
            fn device_info(&self) -> Value {
                match self.rec.device_format() {
                    Some(f) => json!({
                        "name": f.device_name,
                        "id": f.device_id,
                        "sample_rate": f.sample_rate,
                        "channels": f.channels,
                    }),
                    None => Value::Null,
                }
            }
            fn bitrate_bps(&self) -> u64 {
                AUDIO_BITRATE_BPS
            }
            fn recovery_hint(&self) -> Option<Value> {
                self.rec.recovery_hint()
            }
        }
    };
}

audio_track!(MicTrack, MicRecorder, TrackKind::Mic, "microphone");
audio_track!(
    SystemAudioTrack,
    SystemAudioRecorder,
    TrackKind::System,
    "system audio"
);

// ---- camera --------------------------------------------------------------------------------

/// The camera track (optional). `CameraRecorder::start` blocks until the first frame arrives (up
/// to 3 s), which is why the session starts all tracks concurrently.
pub struct CameraTrack {
    rec: CameraRecorder,
    events: Option<Receiver<CameraEvent>>,
}

impl CameraTrack {
    pub fn new(rec: CameraRecorder) -> Self {
        CameraTrack { rec, events: None }
    }
}

impl SessionTrack for CameraTrack {
    fn kind(&self) -> TrackKind {
        TrackKind::Camera
    }
    fn start(&mut self, ctx: TrackContext) -> Result<(), CmError> {
        self.rec.start(ctx)?;
        self.events = self.rec.take_events();
        Ok(())
    }
    fn pause(&mut self, at: MediaTime) -> Result<(), CmError> {
        self.rec.pause(at)
    }
    fn resume(&mut self, at: MediaTime) -> Result<(), CmError> {
        self.rec.resume(at)
    }
    fn stop(&mut self, at: MediaTime) -> Result<TrackInfo, CmError> {
        self.rec.stop(at)
    }
    fn stats(&self) -> TrackStats {
        self.rec.stats()
    }
    fn poll(&mut self) -> Vec<TrackNote> {
        let Some(rx) = &self.events else {
            return Vec::new();
        };
        let mut out = Vec::new();
        while let Ok(ev) = rx.try_recv() {
            out.push(match ev {
                CameraEvent::DeviceLost { at } => TrackNote::new(
                    NoteLevel::Warning,
                    "camera_lost",
                    "The camera was disconnected; the camera track has a gap",
                )
                .at(at),
                CameraEvent::DeviceRestored { at } => {
                    TrackNote::new(NoteLevel::Info, "camera_restored", "The camera is back").at(at)
                }
                CameraEvent::PermissionRevoked { at } => {
                    TrackNote::permission_revoked(Permission::Camera).at(at)
                }
                CameraEvent::WriterFailed { at, message } => TrackNote::new(
                    NoteLevel::Warning,
                    "camera_writer_failed",
                    format!("The camera file could not be written: {message}"),
                )
                .at(at),
            });
        }
        out
    }
    fn bitrate_bps(&self) -> u64 {
        CAMERA_BITRATE_BPS
    }
}

// ---- cursor --------------------------------------------------------------------------------

/// The cursor telemetry track (optional). Missing Input Monitoring is not a failure: the track
/// records moves by polling and says so in `session.json` (REC-7.AC4).
pub struct CursorTrack {
    rec: CursorRecorder,
}

impl CursorTrack {
    pub fn new(rec: CursorRecorder) -> Self {
        CursorTrack { rec }
    }
}

impl SessionTrack for CursorTrack {
    fn kind(&self) -> TrackKind {
        TrackKind::Cursor
    }
    fn start(&mut self, ctx: TrackContext) -> Result<(), CmError> {
        self.rec.start(ctx)
    }
    fn pause(&mut self, at: MediaTime) -> Result<(), CmError> {
        self.rec.pause(at)
    }
    fn resume(&mut self, at: MediaTime) -> Result<(), CmError> {
        self.rec.resume(at)
    }
    fn stop(&mut self, at: MediaTime) -> Result<TrackInfo, CmError> {
        self.rec.stop(at)
    }
    fn stats(&self) -> TrackStats {
        self.rec.stats()
    }
    fn session_extra(&self) -> Option<(String, Value)> {
        let s = self.rec.status()?;
        Some((
            "cursor".into(),
            json!({
                "clicks": s.clicks,
                "scroll": s.scroll,
                "move_source": s.move_source,
                "permission_missing": s.missing_permission(),
            }),
        ))
    }
    fn start_notes(&self) -> Vec<TrackNote> {
        self.rec
            .status()
            .map(|s| {
                s.warnings
                    .iter()
                    .map(|w| {
                        TrackNote::new(NoteLevel::Warning, "cursor_degraded", w.message.clone())
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
    fn bitrate_bps(&self) -> u64 {
        CURSOR_BITRATE_BPS
    }
}

// ---- anything else -------------------------------------------------------------------------

/// Any other recorder, for tests and future tracks.
pub struct PlainTrack {
    rec: Box<dyn TrackRecorder>,
    required: bool,
}

impl PlainTrack {
    pub fn new(rec: Box<dyn TrackRecorder>, required: bool) -> Self {
        PlainTrack { rec, required }
    }
}

impl SessionTrack for PlainTrack {
    fn kind(&self) -> TrackKind {
        self.rec.kind()
    }
    fn required(&self) -> bool {
        self.required
    }
    fn start(&mut self, ctx: TrackContext) -> Result<(), CmError> {
        self.rec.start(ctx)
    }
    fn pause(&mut self, at: MediaTime) -> Result<(), CmError> {
        self.rec.pause(at)
    }
    fn resume(&mut self, at: MediaTime) -> Result<(), CmError> {
        self.rec.resume(at)
    }
    fn stop(&mut self, at: MediaTime) -> Result<TrackInfo, CmError> {
        self.rec.stop(at)
    }
    fn stats(&self) -> TrackStats {
        self.rec.stats()
    }
}
