//! The recording engine: what turns `start_recording` and friends into a running
//! [`RecordingSession`] and keeps it healthy (REC-1..REC-8, AGT-2, NFR-6, NFR-7, NFR-11).
//!
//! # State machine
//!
//! ```text
//!            start_recording (after confirmation)
//! Idle ──► Countdown ──► Starting ──► Recording ◄──► Paused ──► Stopping ──► Idle
//!             │ stop / discard / pause / Esc:        │  ▲           │ stop / discard / tick
//!             └─ cancelled, no files created         │  └ Resuming ◄┘ (resume countdown)
//!                                                    └─────────────────► Stopping
//! ```
//!
//! One recording at a time: a second start is refused with `busy` ("A recording is already
//! running", REC-1.AC6), also before any confirmation dialog opens. Each recording is driven by one
//! **driver task** that owns the session lifecycle: commands (`pause`, `resume`, `stop`, `discard`,
//! skip) arrive on a channel and run one at a time, and a 1 s tick calls
//! [`RecordingSession::tick`] (disk guard, recorder events). Because everything runs in that one
//! task there are no races between a user's Stop and an automatic stop; whoever comes second gets
//! the same final answer.
//!
//! # Ids
//!
//! `recording_id` (`rec_<ms hex>_<seq>`) identifies a *run* and exists from the moment a start is
//! accepted, even during the countdown. `capture_id` (`rec_<16 chars>`, the library id) exists from
//! `recording.started` and is what `stop_recording` returns. They are different strings on purpose:
//! a discarded or cancelled run has a recording id and no capture.
//!
//! # Events (all on the daemon bus, all carry `recording_id`)
//!
//! | Event | When | Data (besides `recording_id`) |
//! | --- | --- | --- |
//! | `recording.countdown` | each second of the countdown | `remaining`, `total`, `context` (`start`, `resume`) |
//! | `recording.starting` | the countdown is over and the tracks are being started (R3-4); `begin` waits for the slowest track (camera up to 3 s, ScreenCaptureKit up to 20 s), so the HUD and the indicator show now | `by`, `indicator`, `tracks_planned`, `countdown_s`, `restart_of` |
//! | `recording.started` | every track is delivering | `capture_id`, `title`, `live_at_ms`, `by`, `tracks`, `warnings`, `indicator`, `restart_of`, `max_duration_s` |
//! | `recording.paused` / `recording.resumed` | pause, resume | `recorded_ms` (`paused_ms` on resume) |
//! | `recording.mic_muted` / `recording.mic_unmuted` | `set_mic_muted` changed the state (AUD-6) | `at_ms` (session time), `recorded_ms` |
//! | `recording.stats` | 1 Hz while recording or paused | the status object of `get_recording_status` |
//! | `recording.health` | every 5 s | `status` (`ok`, `degraded`), `late_frame_pct`, `disk_left_minutes` |
//! | `recording.warning` | a track or the disk guard noted something | `code`, `message`, `track`, `level` |
//! | `recording.stopped` | the bundle is closed (or the stop failed) | `capture_id`, `duration_s`, `tracks`, `reason`, `error?` |
//! | `recording.discarded` | take deleted, countdown cancelled, or the start failed | `capture_id` (null before the session existed), `reason` (`discard`, `restart`, `countdown_cancelled`, `starter_gone`, `start_failed` with `error: { code, message }`, R4-1) |
//! | `export.skipped` | the automatic export was not queued because FFmpeg is missing, once per run (R5-3) | `capture_id`, `title`, `reason: "ffmpeg_not_found"`, `message` |
//! | `capture.created` | with `recording.stopped` | `capture_id`, `by` |
//! | `recording.selftest` | the 60 fps self-test (REC-3), see below | `selftest_id`, `state`, `remaining_s`, `late_pct`, `indicator`, `result?`, `error?` |
//!
//! `reason` is who or what ended the run: `user` (a stop call), `max_duration`, `disk_full`,
//! `screen_halted`, `screen_encoder_failed`, `permission_revoked` (the OS removed Screen Recording
//! access mid-recording; `stopped` then also carries `permission` and `message`), `shutdown`, `engine_failed` (the driver panicked or
//! was dropped), `engine_dropped`, `no_frames` (stopped before the first frame: the empty bundle
//! was deleted, there is no capture) or `error`.
//!
//! `recording.warning` codes a client may see: `disk_low`, `disk_full`, `screen_degraded`,
//! `capture_stalled` (no frames for over 1 s, R3-7), `track_failed` (a track could not be
//! written any more and ends where its file ends, R3-1), `mic_silent`, `camera_lost`,
//! `audio_device_*`, `fps60_unverified_with_camera`, `screen_stopped_early`, `permission_revoked`
//! (with `permission`: `screen`, `camera` or `microphone`; only the screen one stops the run).
//!
//! # The indicator facts (REC-5, NFR-11)
//!
//! `recording.started` and `get_recording_status` carry an `indicator` object: what the HUD, the
//! area brackets, the agent toast and the tray need, so no client has to guess. There is no way to
//! start a recording without these facts being published, and no action hides them.
//!
//! ```json
//! { "area": { "kind": "display|region|window", "display_id": 1, "window_id": null,
//!             "bounds": [x, y, w, h], "source_size": [w, h], "scale": 2.0 },
//!   "output": { "width": 3024, "height": 1964, "fps": 30, "codec": "hevc" },
//!   "camera": { "shape": "circle", "size": "m", "position": "bottom-left", "mirror": true } | null,
//!   "mic": true, "system_audio": false, "cursor": true,
//!   "started_by": { "kind": "user" | "agent", "client": "claude-desktop", "tool": "start_recording" } }
//! ```
//!
//! `bounds` are physical px in the global desktop space (`[x, y, w, h]`); a region is already offset
//! by its display.
//!
//! # The 60 fps self-test (REC-3)
//!
//! `run_fps_selftest` records ~10 s of the chosen target at 60 fps on the real sources (screen
//! only), measures late and dropped frames, **deletes the output** and stores the verdict under the
//! `recording.fps60_selftest` setting (see [`super::selftest`]). `start_recording { fps: 60 }` is
//! accepted only while a fresh passing result covers the request. The test is a recording as far as
//! privacy goes (NFR-11): it is Confirm tier for agents, publishes the same indicator facts
//! (`recording.selftest` with `state: "started"` carries `indicator`, so the HUD shows the usual
//! border and "recording" state) and appears in `get_recording_status` as `selftest`. It is
//! exclusive with a recording: whichever runs first makes the other fail with `busy`.
//!
//! | `recording.selftest` `state` | Meaning |
//! | --- | --- |
//! | `started` | the self-test was accepted and capture is starting (published before the tracks are built, like `recording.starting`, R4-9); `total_s`, `remaining_s`, `indicator` |
//! | `running` | every tick: `remaining_s`, `late_pct` so far |
//! | `passed` / `failed` | verdict, with `result` (the stored entry plus `stored`, `stalls`, `stall_ms`); more than 500 ms of capture stalls fails the test even with 0% late frames (R3-7) |
//! | `cancelled` | `stop_recording` / `discard_recording` (or the caller going away, or shutdown); the output is gone |
//! | `error` | the run could not finish (`error` has the shared code and message) |
//!
//! # Jobs
//!
//! Just before the tracks start, the engine holds the `export`, `transcribe` and (unless
//! `storage.pause_uploads_while_recording` is `false`) `upload` and `publish` job types (EXP-4.AC2, STO-3.AC3).
//! The holds are an RAII value owned by the driver: they are released on stop, discard, a failed
//! start, a cancelled countdown, a panic and when the task is dropped. A restart keeps them across
//! the discard so exports do not flap.
//!
//! After a normal stop (never a discard or the self-test) the engine releases the holds, then
//! enqueues the `export` job of the interim exporter when `export.auto_on_stop` is on (default) and
//! an exporter is registered, and stores the camera layout at stop in `edits[0].layout.camera`; see
//! [`crate::export`].
//!
//! # Failure handling
//!
//! * A panic in the driver is caught by a supervisor: the session is stopped and finalized if it
//!   can be (`reason: "engine_failed"`), the engine returns to Idle and the holds are released.
//! * A stop that fails leaves the bundle with its lock, so the next launch recovers it (REC-8).
//! * A stop slower than [`EngineOptions::stop_timeout`] (the hardware encoder's own finish can
//!   take 60 s) answers the caller with an error but keeps the engine in `Stopping` (no new
//!   recording, job holds kept) until the blocking job really ends, then publishes the result
//!   (`recording.stopped`) like any other stop (R3-10).
//! * A restart holds the engine slot from the discard of the old take to the start of the new
//!   one: nothing can slip in between (R3-10).
//! * The plan the user approved is the plan that runs: the daemon never re-plans after a
//!   confirmation, so settings another client changes while a dialog is open do not change what
//!   is recorded (R2-1), and a discard or restart is pinned to the recording id the dialog
//!   described (R2-2).
//! * [`RecordingEngine::shutdown`] stops a running recording and waits a bounded time.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use cm_capture::disk::{assess_disk, DiskStatus, FreeSpace, SystemFreeSpace};
use cm_capture::record::{
    plan_capture, CapturePlan, Exclusions, ScreenSettings, WindowProvider, MAX_PAUSES,
};
use cm_capture::{CaptureTarget, Capturer, Targets};
use cm_core::{
    Actor, CmError, HostClock, MediaTime, Permission, SystemHostClock, TrackInfo, TrackKind,
};
use cm_encode::VideoSettings;
use cm_jobs::JobQueue;
use cm_library::index::CaptureRecord;
use cm_library::rec::SessionDisplay;
use cm_library::Library;
use serde::Serialize;
use serde_json::{json, Map, Value};
use time::format_description::well_known::Rfc3339;
use tokio::sync::{mpsc, oneshot, watch, Notify};

use super::params::{
    countdown_setting, resume_countdown_setting, ExcludeParams, ResolvedStart, StartParams,
};
use super::selftest::{self, Fingerprint, Fps60Verdict, SelftestEntry, SelftestParams};
use super::session::{RecordingSession, SessionConfig, SessionNote, SessionStatus, TrackState};
use super::sources::{BuildContext, DeviceNames, PreflightWarning, SensorRequest, SourceFactory};
use super::tracks::{planned_bitrate_bps, NoteLevel};
use crate::action::{ExternalParams, ScreenshotTarget};
use crate::clock::Clock;
use crate::daemon::lock_library;
use crate::engines::SettingsStore;
use crate::error::DaemonError;
use crate::events::{self, EventBus};

/// Password managers and the like that never appear in a recording (REC-6 default list). Matched
/// by bundle id on macOS. Settings -> Privacy extends it (`privacy.excluded_apps`); a request can
/// add to it, never remove from it.
pub const DEFAULT_EXCLUDED_BUNDLE_IDS: &[&str] = &[
    "com.1password.1password",
    "com.agilebits.onepassword7",
    "com.bitwarden.desktop",
    "org.keepassxc.keepassxc",
    "com.dashlane.dashlanephonefinal",
    "com.apple.Passwords",
];

/// Job types held while a recording runs.
const HELD_ALWAYS: [&str; 3] = ["export", "transcribe", "translate"];
/// Held with the uploads unless `storage.pause_uploads_while_recording` is off: a publish is an
/// upload over the same connection.
const HELD_UPLOAD: [&str; 2] = ["upload", "publish"];
/// Setting that turns the upload hold off (STO-3: "Pause uploads while recording", default on).
pub const PAUSE_UPLOADS_SETTING: &str = "storage.pause_uploads_while_recording";

/// Tunables of the engine. The defaults are the product values; tests shrink them.
#[derive(Clone)]
pub struct EngineOptions {
    /// How often the driver ticks (recorder events, disk guard, `recording.stats`). 1 s.
    pub tick_every: Duration,
    /// Length of one countdown step (REC-4: 1 s). Tests shorten it.
    pub countdown_step: Duration,
    /// `recording.health` every this many ticks (REC-3: every 5 s).
    pub health_every_ticks: u32,
    /// Longest wait for the tracks to stop and the bundle to be finalized.
    pub stop_timeout: Duration,
    /// Longest wait [`RecordingEngine::shutdown`] gives a running recording.
    pub shutdown_timeout: Duration,
    /// The host clock the session clock runs on.
    pub host: Arc<dyn HostClock>,
    /// Free-space probe for the disk guard (NFR-7).
    pub free_space: Arc<dyn FreeSpace>,
    /// How often the session re-checks the disk (REC-3: 5 s).
    pub disk_check_every: Duration,
    pub app_version: String,
    /// CaptureMann's own process: all of its windows are excluded from the screen track (REC-6).
    pub own_pid: Option<u32>,
    pub default_excluded_bundle_ids: Vec<String>,
    /// Length of the 60 fps self-test (REC-3: 10 s). Tests shrink it.
    pub selftest_duration: Duration,
    /// Where the self-test writes its temporary files (default: the system temp folder). The
    /// folder it creates there is removed when the test ends, however it ends.
    pub selftest_dir: Option<PathBuf>,
    /// Whether an FFmpeg for the automatic export exists (default: look for it, see
    /// `cm_media::tools::locate`). Tests inject a fixed answer (R5-3).
    pub ffmpeg_probe: Option<Arc<dyn Fn() -> bool + Send + Sync>>,
    /// Windows of `own_pid` that are captured anyway: the host's library/editor window
    /// (REC-6.AC8). Queried when the plan is made and by the capture backend while recording.
    /// `None` (default): every window of the own process is excluded.
    pub kept_windows: Option<Arc<dyn Fn() -> Vec<u32> + Send + Sync>>,
}

impl Default for EngineOptions {
    fn default() -> Self {
        EngineOptions {
            tick_every: Duration::from_secs(1),
            countdown_step: Duration::from_secs(1),
            health_every_ticks: 5,
            stop_timeout: Duration::from_secs(20),
            shutdown_timeout: Duration::from_secs(10),
            host: Arc::new(SystemHostClock),
            free_space: Arc::new(SystemFreeSpace),
            disk_check_every: Duration::from_secs(5),
            app_version: env!("CARGO_PKG_VERSION").to_owned(),
            own_pid: Some(std::process::id()),
            default_excluded_bundle_ids: DEFAULT_EXCLUDED_BUNDLE_IDS
                .iter()
                .map(|s| (*s).to_owned())
                .collect(),
            selftest_duration: selftest::DEFAULT_DURATION,
            selftest_dir: None,
            ffmpeg_probe: None,
            kept_windows: None,
        }
    }
}

impl std::fmt::Debug for EngineOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EngineOptions")
            .field("tick_every", &self.tick_every)
            .field("stop_timeout", &self.stop_timeout)
            .field("own_pid", &self.own_pid)
            .field("kept_windows", &self.kept_windows.is_some())
            .finish_non_exhaustive()
    }
}

/// Where a recording is in its life (see the module docs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Countdown,
    Starting,
    Recording,
    Paused,
    /// Paused, with the resume countdown running.
    Resuming,
    Stopping,
}

impl Phase {
    pub fn as_str(self) -> &'static str {
        match self {
            Phase::Countdown => "countdown",
            Phase::Starting => "starting",
            Phase::Recording => "recording",
            Phase::Paused => "paused",
            Phase::Resuming => "resuming",
            Phase::Stopping => "stopping",
        }
    }
}

/// What the engine needs from the daemon.
pub(crate) struct EngineDeps {
    pub events: EventBus,
    pub library: Arc<Mutex<Library>>,
    pub capturer: Arc<Capturer>,
    pub settings: Arc<dyn SettingsStore>,
    pub jobs: Option<JobQueue>,
    /// The destinations file, for the automatic uploads (STO-3); `None` means none.
    pub destinations: Option<cm_storage::DestinationStore>,
    pub clock: Arc<dyn Clock>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

fn rfc3339(t: time::OffsetDateTime) -> String {
    t.format(&Rfc3339).unwrap_or_default()
}

fn kind_name(k: TrackKind) -> &'static str {
    k.as_str()
}

// ---- job holds ---------------------------------------------------------------------------

/// The job types held for the length of a recording; released on drop.
pub struct JobHolds {
    queue: Option<JobQueue>,
    held: Vec<&'static str>,
}

impl std::fmt::Debug for JobHolds {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JobHolds")
            .field("held", &self.held)
            .finish_non_exhaustive()
    }
}

impl JobHolds {
    fn acquire(queue: Option<&JobQueue>, include_upload: bool) -> JobHolds {
        let mut held = Vec::new();
        if let Some(q) = queue {
            let types = HELD_ALWAYS
                .iter()
                .copied()
                .chain(HELD_UPLOAD.iter().copied().filter(|_| include_upload));
            for t in types {
                // A type someone else already holds stays theirs.
                if !q.is_type_held(t) {
                    q.pause_type(t);
                    held.push(t);
                }
            }
        }
        JobHolds {
            queue: queue.cloned(),
            held,
        }
    }
}

impl Drop for JobHolds {
    fn drop(&mut self) {
        if let Some(q) = &self.queue {
            for t in self.held.drain(..) {
                q.resume_type(t);
            }
        }
    }
}

// ---- prepared start ----------------------------------------------------------------------

/// A validated, planned start: everything known before the first file is created. Its
/// `summary` and `details` are what the confirmation dialog shows (AGT-2).
#[derive(Debug)]
pub struct PreparedStart {
    resolved: ResolvedStart,
    plan: CapturePlan,
    targets: Targets,
    library_root: PathBuf,
    warnings: Vec<PreflightWarning>,
    indicator: Value,
    restart_of: Option<String>,
    holds: Option<JobHolds>,
    /// The `camera.*` settings read when the request was planned (R2-1: snapshot, never re-read).
    camera_settings: Map<String, Value>,
    /// One sentence: what will happen.
    pub summary: String,
    /// Flat `label -> text` rows: target, output, sensors, destination, limits.
    pub details: Value,
    /// The same plan as typed parameters, for a client that words the dialog itself (P3).
    pub params: Value,
}

// ---- shared per-recording state -----------------------------------------------------------

type Reply = oneshot::Sender<Result<Value, DaemonError>>;

struct DiscardDone {
    value: Value,
    holds: Option<JobHolds>,
}

enum Cmd {
    Pause(Reply),
    Resume(Reply),
    Skip(Reply),
    Stop {
        reason: String,
        reply: Reply,
    },
    Discard {
        reason: String,
        keep_holds: bool,
        reply: oneshot::Sender<Result<DiscardDone, DaemonError>>,
    },
}

#[derive(Debug)]
struct Meta {
    phase: Phase,
    countdown_remaining: Option<u32>,
    capture_id: Option<String>,
    title: Option<String>,
    /// The last status the driver computed (served when the session is busy).
    status: Option<Value>,
    warnings: Vec<Value>,
    pauses: usize,
    stop_reason: Option<String>,
    started_at: Option<String>,
}

struct Shared {
    id: String,
    tx: mpsc::UnboundedSender<Cmd>,
    meta: Mutex<Meta>,
    session: Mutex<Option<RecordingSession>>,
    resolved: ResolvedStart,
    by: String,
    indicator: Value,
    /// The `camera.*` settings when the recording was accepted, to tell a bubble change made
    /// during the recording from the request itself (layout at stop, see [`crate::export`]).
    camera_at_start: Map<String, Value>,
    restart_of: Option<String>,
}

struct Last {
    id: String,
    value: Value,
}

#[derive(Default)]
struct State {
    active: Option<Arc<Shared>>,
    /// The 60 fps self-test, which excludes a recording (and the other way round).
    selftest: Option<Arc<SelftestShared>>,
    last: Option<Last>,
    seq: u64,
}

pub(crate) struct EngineInner {
    deps: EngineDeps,
    factory: Arc<dyn SourceFactory>,
    opts: EngineOptions,
    state: Mutex<State>,
    /// `export.skipped` was published (once per daemon run, R5-3).
    export_hint_sent: AtomicBool,
}

/// The recording engine of a [`crate::Daemon`]. Cheap to clone.
#[derive(Clone)]
pub struct RecordingEngine {
    inner: Arc<EngineInner>,
}

/// Names of the actions the engine runs itself.
const ENGINE_ACTIONS: [&str; 10] = [
    "start_recording",
    "pause_recording",
    "resume_recording",
    "set_mic_muted",
    "stop_recording",
    "get_recording_status",
    "discard_recording",
    "restart_recording",
    "skip_countdown",
    "run_fps_selftest",
];

mod api;
mod describe;
mod driver;
mod inner;
mod outcome;
mod selftest_run;
#[cfg(test)]
mod tests;

use self::describe::*;
use self::driver::*;
use self::outcome::*;
use self::selftest_run::*;

pub use self::outcome::clock_text;
pub use self::selftest_run::PreparedSelftest;
