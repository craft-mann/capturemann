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
const HELD_ALWAYS: [&str; 2] = ["export", "transcribe"];
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

impl RecordingEngine {
    pub(crate) fn new(
        deps: EngineDeps,
        factory: Arc<dyn SourceFactory>,
        opts: EngineOptions,
    ) -> Self {
        // A killed self-test leaves its screen capture behind: clean up what is older than a
        // minute, in the default and the configured folder (R3-16).
        let min_age = Duration::from_secs(60);
        sweep_selftest_dirs(&std::env::temp_dir(), min_age);
        if let Some(d) = &opts.selftest_dir {
            sweep_selftest_dirs(d, min_age);
        }
        RecordingEngine {
            inner: Arc::new(EngineInner {
                deps,
                factory,
                opts,
                state: Mutex::new(State::default()),
                export_hint_sent: AtomicBool::new(false),
            }),
        }
    }

    /// Whether the engine runs `method` (so a Confirm-tier action is available).
    pub fn handles(method: &str) -> bool {
        ENGINE_ACTIONS.contains(&method)
    }

    /// Whether a recording is in progress (any phase).
    pub fn is_active(&self) -> bool {
        lock(&self.inner.state).active.is_some()
    }

    /// The `list_devices` result.
    pub async fn devices(&self) -> Result<Value, DaemonError> {
        let f = self.inner.factory.clone();
        crate::daemon::blocking(move || Ok(f.devices())).await
    }

    // ---- start ---------------------------------------------------------------------------

    /// Validates a `start_recording` request and plans it (targets, size, permissions, device
    /// names) without creating anything. Errors here (`invalid_input`, `not_found`, `busy`,
    /// `permission_missing`) surface before the confirmation dialog opens.
    pub async fn prepare_start(
        &self,
        actor: &Actor,
        params: &ExternalParams,
    ) -> Result<PreparedStart, DaemonError> {
        let parsed = StartParams::parse(&params.rest)?;
        self.ensure_idle()?;
        let settings = self.inner.deps.settings.all().unwrap_or_default();
        let resolved = parsed.resolve(&settings);
        // 60 fps needs a passing self-test (REC-3): refused when asked for, dropped to 30 when it
        // only came from the `recording.fps` default.
        let gate = Some(parsed.fps.is_some());
        self.prepare(actor, resolved, settings, None, gate).await
    }

    /// Plans a restart: the running recording's settings, a fresh countdown (REC-5, OD-4).
    pub async fn prepare_restart(&self, actor: &Actor) -> Result<PreparedStart, DaemonError> {
        let sh = self.active_for(None)?;
        let settings = self.inner.deps.settings.all().unwrap_or_default();
        let mut resolved = sh.resolved.clone();
        resolved.countdown_s = countdown_setting(&settings);
        // A restart keeps the settings the running take was accepted with: no gate.
        self.prepare(actor, resolved, settings, Some(sh.id.clone()), None)
            .await
    }

    fn ensure_idle(&self) -> Result<(), DaemonError> {
        let st = lock(&self.inner.state);
        if st.active.is_some() {
            return Err(CmError::busy("A recording is already running").into());
        }
        if st.selftest.is_some() {
            return Err(selftest_busy().into());
        }
        Ok(())
    }

    fn exclusions(&self, r: &ResolvedStart, settings: &Map<String, Value>) -> Exclusions {
        let mut bundle_ids: Vec<String> = Vec::new();
        let stored = settings
            .get("privacy.excluded_apps")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned);
        for id in self
            .inner
            .opts
            .default_excluded_bundle_ids
            .iter()
            .cloned()
            .chain(stored)
            .chain(r.exclude.bundle_ids.iter().cloned())
        {
            if !bundle_ids.contains(&id) {
                bundle_ids.push(id);
            }
        }
        // The kept windows only matter while the own process is excluded as a whole.
        let provider = self
            .inner
            .opts
            .own_pid
            .and(self.inner.opts.kept_windows.clone())
            .map(WindowProvider::from_arc);
        Exclusions {
            window_ids: r.exclude.window_ids.clone(),
            pids: self.inner.opts.own_pid.into_iter().collect(),
            bundle_ids,
            keep_window_ids: provider
                .as_ref()
                .map(WindowProvider::ids)
                .unwrap_or_default(),
            keep_provider: provider,
        }
    }

    async fn prepare(
        &self,
        actor: &Actor,
        resolved: ResolvedStart,
        settings: Map<String, Value>,
        restart_of: Option<String>,
        fps60_gate: Option<bool>,
    ) -> Result<PreparedStart, DaemonError> {
        let mut resolved = resolved;
        let exclusions = self.exclusions(&resolved, &settings);
        let (mut plan, targets, mut warnings, names, root) =
            self.plan_blocking(&resolved, exclusions).await?;
        if let (Some(explicit), 60) = (fps60_gate, plan.video.fps) {
            let fp = Fingerprint::new(
                plan.display_id,
                &plan.video,
                resolved.size,
                resolved.codec,
                &self.inner.opts.app_version,
            );
            if let Fps60Verdict::Rejected(r) =
                selftest::evaluate(&settings, &fp, self.inner.deps.clock.now())
            {
                if explicit {
                    return Err(r.into_error());
                }
                resolved.fps = 30;
                let exclusions = self.exclusions(&resolved, &settings);
                let again = self.plan_blocking(&resolved, exclusions).await?;
                plan = again.0;
                warnings.push(PreflightWarning {
                    track: None,
                    code: "fps60_not_validated".to_owned(),
                    message: format!("Recording at 30 fps. {}", r.message),
                });
            }
        }
        // R3-8: the 60 fps verdict was measured with the screen only. With the camera on the
        // machine has more to do, so say it was not verified for that combination.
        if plan.video.fps == 60 && resolved.camera.is_some() {
            warnings.push(PreflightWarning {
                track: Some("camera".to_owned()),
                code: "fps60_unverified_with_camera".to_owned(),
                message: "The 60 fps check measured the screen alone. With the camera on, frames \
                          may arrive late; watch the health indicator or record at 30 fps"
                    .to_owned(),
            });
        }
        // R3-9 / REC-8.AC4: the low-disk warning belongs before the recording, in the plan.
        let bitrate = planned_bitrate_bps(
            plan.video.bitrate_bps,
            resolved.camera.is_some(),
            resolved.mic.is_some(),
            resolved.system_audio,
            resolved.cursor,
        );
        let disk = match self.inner.opts.free_space.free_bytes(&root) {
            Some(free) => match assess_disk(free, bitrate) {
                DiskStatus::Full { free_bytes } => {
                    return Err(CmError::disk_full(format!(
                        "Only {} MB free; recording needs more than 1 GB free to start",
                        free_bytes / 1_000_000
                    ))
                    .into());
                }
                DiskStatus::Low { minutes_left } => {
                    warnings.push(PreflightWarning {
                        track: None,
                        code: "disk_low".to_owned(),
                        message: format!(
                            "About {minutes_left:.0} minutes of disk space left at this setting"
                        ),
                    });
                    Some((
                        disk_text(free, minutes_left),
                        disk_params(free, minutes_left),
                    ))
                }
                DiskStatus::Ok { minutes_left } => Some((
                    disk_text(free, minutes_left),
                    disk_params(free, minutes_left),
                )),
            },
            None => None,
        };
        let indicator = indicator_json(&resolved, &plan, &targets, actor);
        let (summary, mut details, mut params) = describe(
            &resolved,
            &plan,
            &targets,
            &names,
            &root,
            restart_of.is_some(),
        );
        if let (Some((text, typed)), Some(o)) = (disk, details.as_object_mut()) {
            o.insert("disk_space".into(), json!(text));
            if let Some(p) = params.as_object_mut() {
                p.insert("disk".into(), typed);
            }
        }
        let camera_settings = crate::export::camera_settings(&settings);
        Ok(PreparedStart {
            resolved,
            plan,
            targets,
            library_root: root,
            warnings,
            indicator,
            restart_of,
            holds: None,
            camera_settings,
            summary,
            details,
            params,
        })
    }

    /// Plans `resolved` against the current targets and runs the permission preflight. Blocking
    /// work, off the async threads.
    async fn plan_blocking(
        &self,
        resolved: &ResolvedStart,
        exclusions: Exclusions,
    ) -> Result<
        (
            CapturePlan,
            Targets,
            Vec<PreflightWarning>,
            DeviceNames,
            PathBuf,
        ),
        DaemonError,
    > {
        let eng = self.inner.clone();
        let r2 = resolved.clone();
        crate::daemon::blocking(move || {
            let want_windows = matches!(r2.target, ScreenshotTarget::Window { .. });
            let targets = eng.deps.capturer.list_targets_with(want_windows)?;
            let mut screen = ScreenSettings::new(r2.target.to_capture_target());
            screen.video = VideoSettings {
                size: r2.size,
                fps: r2.fps,
                codec: r2.codec,
                quality: r2.quality,
            };
            screen.exclusions = exclusions;
            // "Record cursor separately" (EDT-4): the pointer is not in the picture.
            screen.show_cursor = !r2.cursor_separate;
            let plan = plan_capture(&screen, &targets)?;
            let req = SensorRequest::from_resolved(&r2);
            let pre = eng.factory.preflight(&req);
            if let Some(e) = pre.errors.into_iter().next() {
                return Err(e);
            }
            let names = eng.factory.device_names(&req);
            let root = lock_library(&eng.deps.library).root().to_owned();
            Ok((plan, targets, pre.warnings, names, root))
        })
        .await
    }

    /// Starts the recording and returns when every track is delivering (after the countdown),
    /// with `{ recording_id, capture_id, live_at_ms, indicator, ... }`. If the caller goes away
    /// before that, the countdown is cancelled.
    pub async fn start(&self, actor: &Actor, prep: PreparedStart) -> Result<Value, DaemonError> {
        self.start_replacing(actor, prep, None).await
    }

    /// [`RecordingEngine::start`], taking over the slot of `replacing` (a restart): nothing else
    /// can start between the discard of the old take and the start of the new one.
    async fn start_replacing(
        &self,
        actor: &Actor,
        prep: PreparedStart,
        replacing: Option<&str>,
    ) -> Result<Value, DaemonError> {
        let (sh, rx) = match self.reserve(actor, &prep, replacing) {
            Ok(x) => x,
            Err(e) => {
                if let Some(old) = replacing {
                    self.inner.release_replaced(old);
                }
                return Err(e);
            }
        };
        let (ready_tx, ready_rx) = oneshot::channel();
        let driver = Driver {
            eng: self.inner.clone(),
            sh: sh.clone(),
            rx,
            holds: None,
            ticks: 0,
            revoked_prev: Vec::new(),
        };
        let handle = tokio::spawn(driver.run(prep, ready_tx));
        let eng = self.inner.clone();
        tokio::spawn(async move {
            if let Err(e) = handle.await {
                eng.salvage(&sh, e.is_panic()).await;
            }
        });
        match ready_rx.await {
            Ok(r) => r,
            Err(_) => Err(DaemonError::Internal(
                "the recording engine stopped unexpectedly while starting".to_owned(),
            )),
        }
    }

    fn reserve(
        &self,
        actor: &Actor,
        prep: &PreparedStart,
        replacing: Option<&str>,
    ) -> Result<(Arc<Shared>, mpsc::UnboundedReceiver<Cmd>), DaemonError> {
        let mut st = lock(&self.inner.state);
        if st
            .active
            .as_ref()
            .is_some_and(|a| Some(a.id.as_str()) != replacing)
        {
            return Err(CmError::busy("A recording is already running").into());
        }
        if st.selftest.is_some() {
            return Err(selftest_busy().into());
        }
        st.seq += 1;
        let ms = self.inner.deps.clock.now().unix_timestamp_nanos() / 1_000_000;
        let id = format!("rec_{ms:012x}_{:04}", st.seq);
        let (tx, rx) = mpsc::unbounded_channel();
        let sh = Arc::new(Shared {
            id,
            tx,
            meta: Mutex::new(Meta {
                phase: if prep.resolved.countdown_s > 0 {
                    Phase::Countdown
                } else {
                    Phase::Starting
                },
                countdown_remaining: None,
                capture_id: None,
                title: None,
                status: None,
                warnings: Vec::new(),
                pauses: 0,
                stop_reason: None,
                started_at: None,
            }),
            session: Mutex::new(None),
            resolved: prep.resolved.clone(),
            by: actor.to_string(),
            indicator: prep.indicator.clone(),
            camera_at_start: prep.camera_settings.clone(),
            restart_of: prep.restart_of.clone(),
        });
        st.active = Some(sh.clone());
        Ok((sh, rx))
    }

    // ---- commands ------------------------------------------------------------------------

    fn active_for(&self, id: Option<&str>) -> Result<Arc<Shared>, DaemonError> {
        let st = lock(&self.inner.state);
        match (&st.active, id) {
            (Some(a), Some(i)) if a.id != i => Err(CmError::not_found(format!(
                "`{i}` is not the running recording (that is `{}`)",
                a.id
            ))
            .into()),
            (Some(a), _) => Ok(a.clone()),
            (None, Some(i)) => {
                Err(CmError::not_found(format!("Recording `{i}` is not running")).into())
            }
            (None, None) => Err(CmError::not_found("No recording is running").into()),
        }
    }

    fn last_matching(&self, id: Option<&str>) -> Option<Value> {
        let st = lock(&self.inner.state);
        st.last
            .as_ref()
            .filter(|l| id == Some(l.id.as_str()))
            .map(|l| l.value.clone())
    }

    async fn ask(
        &self,
        id: Option<&str>,
        make: impl FnOnce(Reply) -> Cmd,
    ) -> Result<Value, DaemonError> {
        let sh = self.active_for(id)?;
        let (tx, rx) = oneshot::channel();
        sh.tx
            .send(make(tx))
            .map_err(|_| CmError::not_found("The recording has already ended"))?;
        rx.await.unwrap_or_else(|_| {
            Err(CmError::not_found("The recording ended before it could answer").into())
        })
    }

    /// Pauses (REC-4). During the countdown it cancels the start (REC-4.AC4).
    pub async fn pause(&self, id: Option<&str>) -> Result<Value, DaemonError> {
        self.ask(id, Cmd::Pause).await
    }

    /// Resumes, after the resume countdown (default 1 s).
    pub async fn resume(&self, id: Option<&str>) -> Result<Value, DaemonError> {
        self.ask(id, Cmd::Resume).await
    }

    /// Mutes or unmutes the microphone (AUD-6): `{ recording_id, muted, changed, at_ms }`
    /// (`at_ms` is the session time of the change). Works while recording, paused or in the
    /// resume countdown; it does not go through the driver's command queue, so it never
    /// interrupts a countdown. Without a recording: `not_found` (like pause); during the start
    /// countdown or without a microphone track: `invalid_input`.
    pub async fn set_mic_muted(&self, id: Option<&str>, muted: bool) -> Result<Value, DaemonError> {
        let sh = self.active_for(id)?;
        let phase = lock(&sh.meta).phase;
        if !matches!(phase, Phase::Recording | Phase::Paused | Phase::Resuming) {
            return Err(CmError::invalid_input(format!(
                "The recording is {}, not running",
                phase.as_str()
            ))
            .into());
        }
        let sh2 = sh.clone();
        let joined = tokio::task::spawn_blocking(move || {
            let mut g = lock(&sh2.session);
            match g.as_mut() {
                Some(s) => s.set_mic_muted(muted).map(|c| (c, s.status())),
                None => Err(CmError::internal("the recording session is gone")),
            }
        })
        .await;
        let (change, st) = match joined {
            Ok(r) => r?,
            Err(e) => return Err(CmError::internal(format!("worker failed: {e}")).into()),
        };
        lock(&sh.meta).status = Some(session_status_json(&st));
        let at_ms = change.at.as_millis_floor();
        if change.changed {
            let name = if muted {
                events::RECORDING_MIC_MUTED
            } else {
                events::RECORDING_MIC_UNMUTED
            };
            self.inner.publish(
                &sh,
                name,
                json!({ "at_ms": at_ms, "recorded_ms": st.recorded.as_millis_floor() }),
            );
        }
        Ok(json!({
            "recording_id": sh.id,
            "muted": st.mic_muted,
            "changed": change.changed,
            "at_ms": at_ms,
        }))
    }

    /// Skips the running countdown (Space, REC-1.AC5).
    pub async fn skip_countdown(&self, id: Option<&str>) -> Result<Value, DaemonError> {
        self.ask(id, Cmd::Skip).await
    }

    /// Stops and finalizes: `{ recording_id, capture_id, duration_s, tracks, reason, ... }`.
    /// Valid while recording, paused, resuming and during the countdown (which then cancels
    /// with nothing recorded). Stopping a recording that already ended on its own returns the
    /// same result with `already_stopped: true`.
    pub async fn stop(&self, id: Option<&str>, reason: &str) -> Result<Value, DaemonError> {
        if let Some(v) = self.cancel_selftest(id).await {
            return Ok(v);
        }
        let already = |mut v: Value| {
            v["already_stopped"] = json!(true);
            v
        };
        let sh = match self.active_for(id) {
            Ok(sh) => sh,
            Err(e) => {
                return match self.last_matching(id) {
                    Some(v) => Ok(already(v)),
                    None => Err(e),
                }
            }
        };
        let (tx, rx) = oneshot::channel();
        let sent = sh
            .tx
            .send(Cmd::Stop {
                reason: reason.to_owned(),
                reply: tx,
            })
            .is_ok();
        let answer = if sent { rx.await.ok() } else { None };
        match answer {
            Some(r) => r,
            // The recording ended on its own just as this call arrived: same answer, no error.
            None => match self.last_matching(Some(&sh.id)) {
                Some(v) => Ok(already(v)),
                None => Err(CmError::not_found("The recording has already ended").into()),
            },
        }
    }

    /// Deletes the take (REC-5.AC3): stops the tracks and removes the bundle. During the
    /// countdown it cancels. Nothing is indexed, nothing is recoverable.
    pub async fn discard(&self, id: Option<&str>, reason: &str) -> Result<Value, DaemonError> {
        if let Some(v) = self.cancel_selftest(id).await {
            return Ok(v);
        }
        let sh = self.active_for(id)?;
        let (tx, rx) = oneshot::channel();
        sh.tx
            .send(Cmd::Discard {
                reason: reason.to_owned(),
                keep_holds: false,
                reply: tx,
            })
            .map_err(|_| CmError::not_found("The recording has already ended"))?;
        match rx.await {
            Ok(r) => r.map(|d| d.value),
            Err(_) => Err(CmError::not_found("The recording ended before it could answer").into()),
        }
    }

    /// Discards the take and starts a new one with `prep` (made by
    /// [`RecordingEngine::prepare_restart`]), keeping the job holds across the gap.
    pub async fn restart(
        &self,
        actor: &Actor,
        mut prep: PreparedStart,
    ) -> Result<Value, DaemonError> {
        let expected = prep.restart_of.clone();
        let sh = self.active_for(expected.as_deref())?;
        let (tx, rx) = oneshot::channel();
        sh.tx
            .send(Cmd::Discard {
                reason: "restart".to_owned(),
                keep_holds: true,
                reply: tx,
            })
            .map_err(|_| CmError::not_found("The recording has already ended"))?;
        let done = rx
            .await
            .map_err(|_| CmError::not_found("The recording ended before it could restart"))??;
        prep.holds = done.holds;
        let mut v = self
            .start_replacing(actor, prep, expected.as_deref())
            .await?;
        v["restart_of"] = json!(expected);
        Ok(v)
    }

    /// A one-sentence description of what a discard deletes, for the confirmation dialog.
    pub fn describe_discard(&self, id: Option<&str>) -> Result<(String, Value), DaemonError> {
        self.describe_discard_pinned(id).map(|(_, s, d, _)| (s, d))
    }

    /// Like [`RecordingEngine::describe_discard`], also returning the id of the recording the
    /// description is about: the discard that follows the dialog must target that id (R2-2).
    pub fn describe_discard_pinned(
        &self,
        id: Option<&str>,
    ) -> Result<(String, String, Value, Value), DaemonError> {
        let sh = self.active_for(id)?;
        let meta = lock(&sh.meta);
        let recorded = meta
            .status
            .as_ref()
            .and_then(|s| s["recorded_ms"].as_i64())
            .unwrap_or(0);
        Ok((
            sh.id.clone(),
            "Discard the current recording and delete its files".to_owned(),
            json!({
                "recording": sh.id,
                "title": meta.title,
                "recorded": clock_text(recorded),
                "this_cannot_be_undone": "The take is deleted and cannot be recovered",
            }),
            json!({ "recording": sh.id, "title": meta.title, "recorded_ms": recorded }),
        ))
    }

    /// Stops a running recording for good (app quit, daemon shutdown) and waits a bounded time.
    /// A countdown is cancelled.
    pub async fn shutdown(&self) {
        let _ = self.cancel_selftest(None).await;
        let Ok(sh) = self.active_for(None) else {
            return;
        };
        let (tx, rx) = oneshot::channel();
        if sh
            .tx
            .send(Cmd::Stop {
                reason: "shutdown".to_owned(),
                reply: tx,
            })
            .is_ok()
        {
            let _ = tokio::time::timeout(self.inner.opts.shutdown_timeout, rx).await;
        }
    }

    // ---- status --------------------------------------------------------------------------

    /// `get_recording_status`: the live numbers the HUD shows (see the module docs), or
    /// `{ state: "idle", last: {...} }`.
    pub async fn status(&self, id: Option<&str>) -> Result<Value, DaemonError> {
        let active = {
            let st = lock(&self.inner.state);
            match (&st.active, id) {
                (Some(a), Some(i)) if a.id != i => None,
                (Some(a), _) => Some(a.clone()),
                _ => None,
            }
        };
        if let Some(sh) = active {
            return Ok(status_value(&sh));
        }
        if let Some(s) = self.selftest_status() {
            let mine = id.is_none() || s["selftest_id"].as_str() == id;
            if mine {
                let mut v = s;
                // A self-test is a recording for the indicator's purposes, but it has no
                // recording id: clients tell it apart by `selftest`.
                return Ok(
                    json!({ "state": "selftest", "recording_id": null, "selftest": v.take(),
                                  "last": null }),
                );
            }
        }
        let st = lock(&self.inner.state);
        match (id, &st.last) {
            (Some(i), Some(l)) if l.id == i => {
                let mut v = l.value.clone();
                v["state"] = json!("stopped");
                Ok(v)
            }
            (Some(i), _) => Err(CmError::not_found(format!("Recording `{i}` is not known")).into()),
            (None, last) => Ok(json!({
                "state": "idle",
                "recording_id": null,
                "selftest": null,
                "last": last.as_ref().map(|l| l.value.clone()),
            })),
        }
    }
}

impl EngineInner {
    fn publish(&self, sh: &Shared, name: &str, mut data: Value) {
        if let Some(o) = data.as_object_mut() {
            o.insert("recording_id".into(), json!(sh.id));
        }
        self.deps.events.publish(name, data);
    }

    /// Records the outcome but leaves `active` set: used while a restart swaps the slot.
    fn conclude_keeping_slot(&self, sh: &Shared, last: Value) {
        lock(&self.state).last = Some(Last {
            id: sh.id.clone(),
            value: last,
        });
    }

    /// The slot of a restarted take is free again (the restart could not take it over).
    fn release_replaced(&self, old: &str) {
        let mut st = lock(&self.state);
        if st.active.as_ref().is_some_and(|a| a.id == old) {
            st.active = None;
        }
    }

    fn conclude(&self, sh: &Shared, last: Value) {
        let mut st = lock(&self.state);
        st.last = Some(Last {
            id: sh.id.clone(),
            value: last,
        });
        if st.active.as_ref().is_some_and(|a| a.id == sh.id) {
            st.active = None;
        }
    }

    /// A driver that died (panic or abort): finish the recording if it can still be finished.
    async fn salvage(self: &Arc<Self>, sh: &Arc<Shared>, panicked: bool) {
        let still_active = lock(&self.state)
            .active
            .as_ref()
            .is_some_and(|a| a.id == sh.id);
        if !still_active {
            return;
        }
        self.publish(
            sh,
            events::RECORDING_WARNING,
            json!({ "code": "engine_failed", "level": "warning", "track": null,
                    "message": if panicked { "The recording engine failed; saving what was recorded" }
                               else { "The recording engine was stopped; saving what was recorded" } }),
        );
        let session = lock(&sh.session).take();
        let mut outcome = json!({ "recording_id": sh.id, "reason": "engine_failed" });
        if let Some(session) = session {
            let lib = self.deps.library.clone();
            let by = sh.by.clone();
            let layout = self.layout_at_stop(sh);
            let r = crate::daemon::blocking(move || {
                finalize_blocking(session, &lib, "engine_failed", &by, layout)
            })
            .await;
            match r {
                Ok(done) => {
                    outcome = outcome_json(sh, &done, "engine_failed");
                    self.publish_stopped(sh, &done, "engine_failed");
                    self.auto_export(sh, &done.record.id, &done.record.title);
                }
                Err(e) => {
                    self.publish(
                        sh,
                        events::RECORDING_STOPPED,
                        json!({ "capture_id": null, "duration_s": null, "tracks": [],
                                "reason": "error", "error": error_json(&e) }),
                    );
                }
            }
        }
        self.conclude(sh, outcome);
    }

    /// A stop that did not produce a capture: the terminal event, then back to Idle.
    fn fail_stop(&self, sh: &Shared, e: &DaemonError) {
        let capture_id = lock(&sh.meta).capture_id.clone();
        // Stopped before the first frame: the empty bundle was deleted (R3-5); say that, not a
        // generic error.
        let reason = match e {
            DaemonError::Cm(c) if cm_encode::is_no_frames(c) => "no_frames",
            _ => "error",
        };
        self.publish(
            sh,
            events::RECORDING_STOPPED,
            json!({ "capture_id": capture_id, "duration_s": null, "tracks": [],
                    "reason": reason, "error": error_json(e) }),
        );
        self.conclude(
            sh,
            json!({ "recording_id": sh.id, "capture_id": capture_id, "state": "failed",
                    "reason": reason, "error": error_json(e) }),
        );
    }

    /// A stop that produced a capture: events, back to Idle, then the automatic export (after
    /// the job holds were released).
    fn finish_stop(&self, sh: &Shared, done: &StopDone, reason: &str, outcome: &Value) {
        self.publish_stopped(sh, done, reason);
        self.conclude(sh, outcome.clone());
        self.auto_export(sh, &done.record.id, &done.record.title);
    }

    fn publish_stopped(&self, sh: &Shared, done: &StopDone, reason: &str) {
        let mut data = json!({
            "capture_id": done.record.id,
            "title": done.record.title,
            "duration_s": done.duration().as_secs_f64(),
            "tracks": done.tracks.iter().map(|t| kind_name(t.kind)).collect::<Vec<_>>(),
            "reason": reason,
        });
        if let Some((p, message)) = revoked_permission(reason, &done.warnings) {
            data["permission"] = json!(p.as_str());
            data["message"] = json!(message);
        }
        self.publish(sh, events::RECORDING_STOPPED, data);
        self.deps.events.publish(
            events::CAPTURE_CREATED,
            json!({ "capture_id": done.record.id, "by": sh.by }),
        );
    }
}

impl EngineInner {
    /// The camera layout to store in the manifest at stop (see [`crate::export::layout_at_stop`]).
    fn layout_at_stop(&self, sh: &Shared) -> Option<Value> {
        let now = self.deps.settings.all().unwrap_or_default();
        crate::export::layout_at_stop(&sh.indicator, &sh.camera_at_start, &now)
    }

    /// REC-1 auto-export: enqueues the `export` job for a recording that just stopped normally,
    /// when `export.auto_on_stop` is on (default) and an exporter is registered. Never for a
    /// discard, a cancelled countdown or the 60 fps self-test (they do not pass through here).
    /// Failures are not the recording's problem: the take is saved either way.
    fn auto_export(&self, sh: &Shared, capture_id: &str, title: &str) {
        let export = self.queue_export(sh, capture_id, title, false);
        self.auto_upload(sh, capture_id, title, export);
    }

    /// STO-3 auto upload after a normal stop: the destinations set to `on_stop` get their
    /// `upload` job (after the export it needs, which is rendered here when the automatic export
    /// is off), and those set to `on_export` get one that waits for the export of this stop.
    /// `export` is what the auto-export came to. Failures are logged and never the recording's
    /// problem.
    fn auto_upload(
        &self,
        sh: &Shared,
        capture_id: &str,
        title: &str,
        export: Result<cm_jobs::Job, String>,
    ) {
        use crate::auto_upload::{enqueue, ExportState};
        use cm_storage::AutoUpload;
        let (Some(queue), Some(store)) = (&self.deps.jobs, &self.deps.destinations) else {
            return;
        };
        let Ok(list) = store.list() else {
            return;
        };
        let on_stop: Vec<_> = list
            .iter()
            .filter(|d| d.auto_upload == AutoUpload::OnStop)
            .collect();
        let mut export = export;
        if on_stop.iter().any(|d| d.what.wants_export()) && export.is_err() {
            // The automatic export is off or failed to queue: render the default export for the
            // upload anyway (it is the destination's choice, not the export setting's).
            export = self.queue_export(sh, capture_id, title, true);
        }
        let state = match &export {
            Ok(j) => ExportState::Job(j),
            Err(why) => ExportState::Unavailable(why.clone()),
        };
        for trigger in [AutoUpload::OnStop, AutoUpload::OnExport] {
            // `on_export` needs the export job itself; without one there is nothing to wait for.
            if trigger == AutoUpload::OnExport && export.is_err() {
                continue;
            }
            if let Err(e) = enqueue(
                queue,
                store,
                &self.deps.events,
                capture_id,
                title,
                &sh.by,
                trigger,
                &state,
            ) {
                eprintln!("cm-daemon: automatic upload not queued: {e}");
            }
        }
    }

    /// Enqueues the default (`quick`) export of a recording that just stopped. `force` ignores
    /// the `export.auto_on_stop` setting (an upload needs the export). `Err` says why no job
    /// exists.
    fn queue_export(
        &self,
        sh: &Shared,
        capture_id: &str,
        title: &str,
        force: bool,
    ) -> Result<cm_jobs::Job, String> {
        let Some(queue) = &self.deps.jobs else {
            return Err("there is no job queue".to_owned());
        };
        let settings = self.deps.settings.all().unwrap_or_default();
        if !queue.has_handler(cm_jobs::JOB_EXPORT) {
            return Err("this daemon has no exporter".to_owned());
        }
        if !force && !crate::export::auto_export_enabled(&settings) {
            return Err("the automatic export is off".to_owned());
        }
        // R5-3: on a machine without FFmpeg the export can only fail, with an error toast after
        // every recording. Say so once and keep the take.
        let have_ffmpeg = match &self.opts.ffmpeg_probe {
            Some(p) => p(),
            None => cm_media::tools::locate().is_ok(),
        };
        if !have_ffmpeg {
            if !self.export_hint_sent.swap(true, Ordering::SeqCst) {
                self.deps.events.publish(
                    events::EXPORT_SKIPPED,
                    json!({ "capture_id": capture_id, "title": title,
                            "reason": "ffmpeg_not_found",
                            "message": cm_media::tools::NOT_FOUND_MESSAGE }),
                );
            }
            return Err("FFmpeg was not found".to_owned());
        }
        crate::export::enqueue_export(
            queue,
            &self.deps.events,
            capture_id,
            title,
            cm_media::PRESET_QUICK,
            None,
            crate::export::ExportOpts::default(),
            &sh.by,
            true,
        )
        .map(|(job, _)| job)
        .map_err(|e| e.to_string())
    }
}

// ---- the driver ---------------------------------------------------------------------------

enum Flow {
    Continue,
    End,
}

enum CountdownEnd {
    Elapsed,
    Skipped,
    /// A pause during the countdown (REC-4.AC4): the caller of pause gets its answer.
    Cancelled(Option<Reply>),
    /// A stop or discard arrived.
    Interrupted(Cmd),
    /// Nobody waits for `start_recording` any more.
    StarterGone,
}

struct Driver {
    eng: Arc<EngineInner>,
    sh: Arc<Shared>,
    rx: mpsc::UnboundedReceiver<Cmd>,
    /// Released on every way out of the driver, including a panic.
    holds: Option<JobHolds>,
    ticks: u32,
    /// Permissions the last tick's poll found revoked: one is acted on only when it shows on
    /// two ticks in a row (PRM-1), so a transient OS answer cannot end a take.
    revoked_prev: Vec<Permission>,
}

fn cancelled_error() -> DaemonError {
    DaemonError::Declined {
        reason: "countdown_cancelled",
        message: "The recording was cancelled before it started".to_owned(),
    }
}

impl Driver {
    fn publish(&self, name: &str, data: Value) {
        self.eng.publish(&self.sh, name, data);
    }

    fn set_phase(&self, p: Phase) {
        lock(&self.sh.meta).phase = p;
    }

    fn phase(&self) -> Phase {
        lock(&self.sh.meta).phase
    }

    fn acquire_holds(&self) -> JobHolds {
        let include_upload = self
            .eng
            .deps
            .settings
            .all()
            .ok()
            .and_then(|s| s.get(PAUSE_UPLOADS_SETTING).and_then(Value::as_bool))
            .unwrap_or(true);
        JobHolds::acquire(self.eng.deps.jobs.as_ref(), include_upload)
    }

    async fn with_session<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut RecordingSession) -> Result<T, CmError> + Send + 'static,
    ) -> Result<T, CmError> {
        let sh = self.sh.clone();
        let joined = tokio::task::spawn_blocking(move || {
            let mut g = lock(&sh.session);
            match g.as_mut() {
                Some(s) => f(s),
                None => Err(CmError::internal("the recording session is gone")),
            }
        })
        .await;
        match joined {
            Ok(r) => r,
            Err(e) => Err(CmError::internal(format!("worker failed: {e}"))),
        }
    }

    async fn run(
        mut self,
        mut prep: PreparedStart,
        ready: oneshot::Sender<Result<Value, DaemonError>>,
    ) {
        let mut ready = Some(ready);
        // Holds carried over by a restart live in the driver, so every way out releases them
        // before anyone is answered.
        self.holds = prep.holds.take();
        if prep.resolved.countdown_s > 0 {
            let total = prep.resolved.countdown_s;
            match self.countdown(total, "start", ready.as_mut()).await {
                CountdownEnd::Elapsed | CountdownEnd::Skipped => {}
                CountdownEnd::StarterGone => {
                    self.cancel_start("starter_gone", None, None, ready.take());
                    return;
                }
                CountdownEnd::Cancelled(reply) => {
                    self.cancel_start("countdown_cancelled", reply, None, ready.take());
                    return;
                }
                CountdownEnd::Interrupted(Cmd::Discard { reply, .. }) => {
                    self.cancel_start("countdown_cancelled", None, Some(reply), ready.take());
                    return;
                }
                CountdownEnd::Interrupted(Cmd::Stop { reply, .. }) => {
                    self.cancel_start("countdown_cancelled", Some(reply), None, ready.take());
                    return;
                }
                CountdownEnd::Interrupted(_) => {
                    self.cancel_start("countdown_cancelled", None, None, ready.take());
                    return;
                }
            }
        }

        self.set_phase(Phase::Starting);
        if self.holds.is_none() {
            self.holds = Some(self.acquire_holds());
        }
        // R3-4: tell the HUD and the indicator now. `begin` waits for the slowest track (the
        // camera up to 3 s, ScreenCaptureKit up to 20 s); the indicator must not wait for it
        // (REC-5.AC1: 200 ms, NFR-11).
        self.publish(
            events::RECORDING_STARTING,
            json!({
                "by": self.sh.by,
                "indicator": self.sh.indicator,
                "tracks_planned": tracks_planned(&prep.resolved),
                "countdown_s": prep.resolved.countdown_s,
                "restart_of": prep.restart_of,
            }),
        );
        let session = match self.begin(&prep).await {
            Ok(s) => s,
            Err(e) => {
                self.holds = None;
                let err: DaemonError = e.into();
                // R4-1: UIs that showed the starting state reset on this.
                self.publish(
                    events::RECORDING_DISCARDED,
                    json!({ "capture_id": null, "reason": "start_failed",
                            "error": error_json(&err) }),
                );
                self.eng.conclude(
                    &self.sh,
                    json!({ "recording_id": self.sh.id, "state": "failed", "error": error_json(&err) }),
                );
                if let Some(r) = ready.take() {
                    let _ = r.send(Err(err));
                }
                return;
            }
        };

        let status = session.status();
        let live_at = session.live_at();
        let capture_id = session.capture_id().to_owned();
        let title = session.title().to_owned();
        let started: Vec<&str> = status
            .tracks
            .iter()
            .filter(|t| t.state == TrackState::Running)
            .map(|t| kind_name(t.kind))
            .collect();
        let mut warnings: Vec<Value> = session.warnings().iter().map(note_json).collect();
        for w in &prep.warnings {
            warnings.push(
                json!({ "track": w.track, "code": w.code, "message": w.message,
                                  "level": "warning", "at_ms": null }),
            );
        }
        let started_at = rfc3339(self.eng.deps.clock.now());
        *lock(&self.sh.session) = Some(session);
        {
            let mut m = lock(&self.sh.meta);
            m.capture_id = Some(capture_id.clone());
            m.title = Some(title.clone());
            m.warnings = warnings.clone();
            m.started_at = Some(started_at.clone());
            m.status = Some(session_status_json(&status));
            m.phase = Phase::Recording;
            m.countdown_remaining = None;
        }
        let started_json = json!({
            "recording_id": self.sh.id,
            "capture_id": capture_id,
            "title": title,
            "state": "recording",
            "live_at_ms": live_at.as_millis_floor(),
            "started_at": started_at,
            "by": self.sh.by,
            "countdown_s": prep.resolved.countdown_s,
            "tracks": started,
            "warnings": warnings,
            "indicator": self.sh.indicator,
            "restart_of": prep.restart_of,
            "max_duration_s": prep.resolved.max_duration_s,
        });
        self.publish(events::RECORDING_STARTED, started_json.clone());
        for w in &warnings {
            self.publish(events::RECORDING_WARNING, w.clone());
        }
        if let Some(r) = ready.take() {
            let _ = r.send(Ok(started_json));
        }
        drop(prep);
        self.run_loop().await;
    }

    async fn begin(&self, prep: &PreparedStart) -> Result<RecordingSession, CmError> {
        let eng = self.eng.clone();
        let resolved = prep.resolved.clone();
        let plan = prep.plan.clone();
        let targets = prep.targets.clone();
        let root = prep.library_root.clone();
        let by = self.sh.by.clone();
        let joined = tokio::task::spawn_blocking(move || {
            let tracks = eng.factory.build(&BuildContext {
                params: &resolved,
                plan: &plan,
                targets: &targets,
            })?;
            let mut cfg = SessionConfig::new(&root);
            cfg.title = resolved.title.clone();
            cfg.settings = resolved.settings_json(&by);
            cfg.displays = targets
                .displays
                .iter()
                .map(|d| SessionDisplay {
                    id: d.id,
                    name: d.name.clone(),
                    bounds: d.bounds,
                    scale: d.scale,
                    is_primary: d.is_primary,
                })
                .collect();
            cfg.app_version = eng.opts.app_version.clone();
            cfg.host = eng.opts.host.clone();
            cfg.free_space = eng.opts.free_space.clone();
            cfg.disk_check_every = eng.opts.disk_check_every;
            RecordingSession::begin(cfg, tracks)
        })
        .await;
        match joined {
            Ok(r) => r,
            Err(e) => Err(CmError::internal(format!("worker failed: {e}"))),
        }
    }

    /// The run ended before any file existed (countdown cancelled).
    fn cancel_start(
        &mut self,
        reason: &str,
        stop_reply: Option<Reply>,
        discard_reply: Option<oneshot::Sender<Result<DiscardDone, DaemonError>>>,
        ready: Option<oneshot::Sender<Result<Value, DaemonError>>>,
    ) {
        lock(&self.sh.meta).countdown_remaining = None;
        let value = json!({
            "recording_id": self.sh.id, "state": "cancelled", "cancelled": true,
            "capture_id": null, "reason": reason,
        });
        self.publish(
            events::RECORDING_DISCARDED,
            json!({ "capture_id": null, "reason": reason }),
        );
        self.holds = None;
        self.eng.conclude(&self.sh, value.clone());
        if let Some(r) = stop_reply {
            let _ = r.send(Ok(value.clone()));
        }
        if let Some(r) = discard_reply {
            let _ = r.send(Ok(DiscardDone {
                value: value.clone(),
                holds: None,
            }));
        }
        if let Some(r) = ready {
            let _ = r.send(Err(cancelled_error()));
        }
    }

    /// Counts down one second at a time, answering commands as they arrive.
    async fn countdown(
        &mut self,
        total: u32,
        context: &'static str,
        mut ready: Option<&mut oneshot::Sender<Result<Value, DaemonError>>>,
    ) -> CountdownEnd {
        for remaining in (1..=total).rev() {
            lock(&self.sh.meta).countdown_remaining = Some(remaining);
            self.publish(
                events::RECORDING_COUNTDOWN,
                json!({ "remaining": remaining, "total": total, "context": context }),
            );
            let sleep = tokio::time::sleep(self.eng.opts.countdown_step);
            tokio::pin!(sleep);
            loop {
                let gone = async {
                    match ready.as_mut() {
                        Some(r) => r.closed().await,
                        None => std::future::pending().await,
                    }
                };
                tokio::select! {
                    () = &mut sleep => break,
                    () = gone => {
                        lock(&self.sh.meta).countdown_remaining = None;
                        return CountdownEnd::StarterGone;
                    }
                    cmd = self.rx.recv() => match cmd {
                        None => {
                            lock(&self.sh.meta).countdown_remaining = None;
                            return CountdownEnd::Cancelled(None);
                        }
                        Some(Cmd::Skip(r)) => {
                            lock(&self.sh.meta).countdown_remaining = None;
                            let _ = r.send(Ok(json!({ "recording_id": self.sh.id, "skipped": true })));
                            return CountdownEnd::Skipped;
                        }
                        Some(Cmd::Pause(r)) => {
                            lock(&self.sh.meta).countdown_remaining = None;
                            return CountdownEnd::Cancelled(Some(r));
                        }
                        Some(Cmd::Resume(r)) => {
                            let _ = r.send(if context == "start" {
                                Err(CmError::invalid_input("The recording has not started yet").into())
                            } else {
                                Ok(json!({ "state": "resuming", "changed": false }))
                            });
                        }
                        Some(other) => {
                            lock(&self.sh.meta).countdown_remaining = None;
                            return CountdownEnd::Interrupted(other);
                        }
                    },
                }
            }
        }
        lock(&self.sh.meta).countdown_remaining = None;
        CountdownEnd::Elapsed
    }

    async fn run_loop(&mut self) {
        let mut ticker = tokio::time::interval(self.eng.opts.tick_every);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        ticker.tick().await;
        loop {
            tokio::select! {
                cmd = self.rx.recv() => match cmd {
                    Some(c) => {
                        if matches!(self.handle(c).await, Flow::End) {
                            return;
                        }
                    }
                    None => {
                        let _ = self.stop("engine_dropped").await;
                        return;
                    }
                },
                _ = ticker.tick() => {
                    if matches!(self.on_tick().await, Flow::End) {
                        return;
                    }
                }
            }
        }
    }

    async fn handle(&mut self, cmd: Cmd) -> Flow {
        let mut next = Some(cmd);
        while let Some(c) = next.take() {
            match c {
                Cmd::Pause(reply) => {
                    let r = self.pause().await;
                    let _ = reply.send(r);
                    return Flow::Continue;
                }
                Cmd::Resume(reply) => {
                    next = self.resume(reply).await;
                    if next.is_none() {
                        return Flow::Continue;
                    }
                }
                Cmd::Skip(reply) => {
                    let _ = reply.send(Ok(json!({ "recording_id": self.sh.id, "skipped": false })));
                    return Flow::Continue;
                }
                Cmd::Stop { reason, reply } => {
                    let r = self.stop(&reason).await;
                    let _ = reply.send(r);
                    return Flow::End;
                }
                Cmd::Discard {
                    reason,
                    keep_holds,
                    reply,
                } => {
                    let r = self.discard(&reason, keep_holds).await;
                    let _ = reply.send(r);
                    return Flow::End;
                }
            }
        }
        Flow::Continue
    }

    async fn pause(&mut self) -> Result<Value, DaemonError> {
        match self.phase() {
            Phase::Paused | Phase::Resuming => {
                return Ok(
                    json!({ "recording_id": self.sh.id, "state": "paused", "changed": false }),
                )
            }
            Phase::Recording => {}
            other => {
                return Err(CmError::invalid_input(format!(
                    "The recording is {}, not running",
                    other.as_str()
                ))
                .into())
            }
        }
        if lock(&self.sh.meta).pauses >= MAX_PAUSES {
            return Err(CmError::invalid_input(
                "Too many pauses \u{2014} stop and start a new recording",
            )
            .into());
        }
        let st = self
            .with_session(|s| {
                s.pause()?;
                Ok(s.status())
            })
            .await?;
        {
            let mut m = lock(&self.sh.meta);
            m.phase = Phase::Paused;
            m.pauses += 1;
            m.status = Some(session_status_json(&st));
        }
        let recorded = st.recorded.as_millis_floor();
        self.publish(events::RECORDING_PAUSED, json!({ "recorded_ms": recorded }));
        Ok(
            json!({ "recording_id": self.sh.id, "state": "paused", "changed": true,
                   "recorded_ms": recorded }),
        )
    }

    /// Resumes after the resume countdown. Returns a command that interrupted the countdown and
    /// still needs handling.
    async fn resume(&mut self, reply: Reply) -> Option<Cmd> {
        match self.phase() {
            Phase::Recording => {
                let _ = reply.send(Ok(
                    json!({ "recording_id": self.sh.id, "state": "recording", "changed": false }),
                ));
                return None;
            }
            Phase::Paused => {}
            other => {
                let _ = reply.send(Err(CmError::invalid_input(format!(
                    "The recording is {}, not paused",
                    other.as_str()
                ))
                .into()));
                return None;
            }
        }
        let settings = self.eng.deps.settings.all().unwrap_or_default();
        let n = resume_countdown_setting(&settings);
        if n > 0 {
            self.set_phase(Phase::Resuming);
            let cancelled = |reply: Reply, id: &str| {
                let _ = reply.send(Ok(
                    json!({ "recording_id": id, "state": "paused", "resumed": false, "cancelled": true }),
                ));
            };
            match self.countdown(n, "resume", None).await {
                CountdownEnd::Elapsed | CountdownEnd::Skipped => {}
                CountdownEnd::Cancelled(r) => {
                    self.set_phase(Phase::Paused);
                    if let Some(r) = r {
                        let _ = r.send(Ok(
                            json!({ "recording_id": self.sh.id, "state": "paused", "changed": false }),
                        ));
                    }
                    cancelled(reply, &self.sh.id);
                    return None;
                }
                CountdownEnd::Interrupted(cmd) => {
                    self.set_phase(Phase::Paused);
                    cancelled(reply, &self.sh.id);
                    return Some(cmd);
                }
                CountdownEnd::StarterGone => {}
            }
        }
        let r = self
            .with_session(|s| {
                s.resume()?;
                Ok(s.status())
            })
            .await;
        match r {
            Ok(st) => {
                {
                    let mut m = lock(&self.sh.meta);
                    m.phase = Phase::Recording;
                    m.status = Some(session_status_json(&st));
                }
                self.publish(
                    events::RECORDING_RESUMED,
                    json!({ "recorded_ms": st.recorded.as_millis_floor(),
                            "paused_ms": st.paused_total.as_millis_floor() }),
                );
                let _ = reply.send(Ok(
                    json!({ "recording_id": self.sh.id, "state": "recording", "changed": true,
                            "paused_ms": st.paused_total.as_millis_floor() }),
                ));
            }
            Err(e) => {
                self.set_phase(Phase::Paused);
                let _ = reply.send(Err(e.into()));
            }
        }
        None
    }

    async fn on_tick(&mut self) -> Flow {
        self.eng.factory.tick_hook();
        self.ticks += 1;
        if !matches!(
            self.phase(),
            Phase::Recording | Phase::Paused | Phase::Resuming
        ) {
            return Flow::Continue;
        }
        let factory = self.eng.factory.clone();
        let prev = std::mem::take(&mut self.revoked_prev);
        let ticked = self
            .with_session(move |s| {
                let mut notes = s.tick();
                let seen = factory.revoked_permissions(&s.running_kinds());
                for p in seen.iter().filter(|p| prev.contains(p)) {
                    notes.extend(s.report_permission_revoked(*p));
                }
                Ok((notes, s.status(), seen))
            })
            .await
            .map(|(notes, st, seen)| {
                self.revoked_prev = seen;
                (notes, st)
            });
        let (notes, st) = match ticked {
            Ok(x) => x,
            Err(e) => {
                self.publish(
                    events::RECORDING_WARNING,
                    json!({ "code": "tick_failed", "level": "warning", "track": null,
                            "message": e.to_string() }),
                );
                return Flow::Continue;
            }
        };
        let mut stop: Option<String> = None;
        for n in &notes {
            if n.level == NoteLevel::Info {
                continue;
            }
            let w = note_json(n);
            lock(&self.sh.meta).warnings.push(w.clone());
            self.publish(events::RECORDING_WARNING, w);
            if n.level == NoteLevel::StopRequested && stop.is_none() {
                stop = Some(n.code.clone());
            }
        }
        lock(&self.sh.meta).status = Some(session_status_json(&st));
        let snapshot = {
            let m = lock(&self.sh.meta);
            build_status(&self.sh, &m, Some(&st))
        };
        self.publish(events::RECORDING_STATS, snapshot.clone());
        if self
            .ticks
            .is_multiple_of(self.eng.opts.health_every_ticks.max(1))
        {
            let late = snapshot["recent_late_frame_pct"].as_f64().unwrap_or(0.0);
            self.publish(
                events::RECORDING_HEALTH,
                json!({
                    "status": if late > 2.0 { "degraded" } else { "ok" },
                    "late_frame_pct": snapshot["late_frame_pct"],
                    "recent_late_frame_pct": snapshot["recent_late_frame_pct"],
                    "disk_left_minutes": snapshot["disk_left_minutes"],
                }),
            );
        }
        if stop.is_none() {
            stop = st.stop_requested.clone();
        }
        if stop.is_none() {
            if let Some(max) = self.sh.resolved.max_duration_s {
                if st.recorded.as_millis_floor() as f64 >= max * 1000.0 {
                    stop = Some("max_duration".to_owned());
                }
            }
        }
        match stop {
            Some(reason) => {
                let _ = self.stop(&reason).await;
                Flow::End
            }
            None => Flow::Continue,
        }
    }

    async fn stop(&mut self, reason: &str) -> Result<Value, DaemonError> {
        self.set_phase(Phase::Stopping);
        lock(&self.sh.meta).stop_reason = Some(reason.to_owned());
        let session = lock(&self.sh.session).take();
        let Some(session) = session else {
            return Err(self.fail(CmError::internal("the recording session is gone").into()));
        };
        let lib = self.eng.deps.library.clone();
        let by = self.sh.by.clone();
        let reason_owned = reason.to_owned();
        let layout = self.eng.layout_at_stop(&self.sh);
        let mut job = tokio::task::spawn_blocking(move || {
            finalize_blocking(session, &lib, &reason_owned, &by, layout)
        });
        let res = tokio::time::timeout(self.eng.opts.stop_timeout, &mut job).await;
        match res {
            Ok(joined) => {
                // Exports and uploads resume before anyone is told the recording is over.
                self.holds = None;
                match joined {
                    Ok(Ok(done)) => {
                        let v = outcome_json(&self.sh, &done, reason);
                        self.eng.finish_stop(&self.sh, &done, reason, &v);
                        Ok(v)
                    }
                    Ok(Err(e)) => Err(self.fail(e.into())),
                    Err(e) => Err(self.fail(DaemonError::Internal(format!("worker failed: {e}")))),
                }
            }
            Err(_) => {
                // R3-10: the encoder's own stop can take longer than this wait (SCK 10 s +
                // hardware encoder 60 s + camera 30 s). The blocking job keeps running, so the
                // engine stays in `Stopping` (no new recording, job holds kept) until it truly
                // ends, and the late result is published then.
                let eng = self.eng.clone();
                let sh = self.sh.clone();
                let holds = self.holds.take();
                let reason = reason.to_owned();
                tokio::spawn(async move {
                    let joined = job.await;
                    drop(holds);
                    match joined {
                        Ok(Ok(done)) => {
                            let v = outcome_json(&sh, &done, &reason);
                            eng.finish_stop(&sh, &done, &reason, &v);
                        }
                        Ok(Err(e)) => {
                            eng.fail_stop(&sh, &DaemonError::from(e));
                        }
                        Err(e) => {
                            eng.fail_stop(
                                &sh,
                                &DaemonError::Internal(format!("worker failed: {e}")),
                            );
                        }
                    }
                });
                Err(CmError::internal(format!(
                    "Stopping is taking longer than {:.1} s. It keeps finishing in the background: \
                     the recording appears in the library when it is done (a new recording cannot \
                     start until then)",
                    self.eng.opts.stop_timeout.as_secs_f64()
                ))
                .into())
            }
        }
    }

    /// A stop that did not produce a capture: publish the terminal event, return to Idle.
    fn fail(&self, e: DaemonError) -> DaemonError {
        self.eng.fail_stop(&self.sh, &e);
        e
    }

    async fn discard(
        &mut self,
        reason: &str,
        keep_holds: bool,
    ) -> Result<DiscardDone, DaemonError> {
        self.set_phase(Phase::Stopping);
        let session = lock(&self.sh.session).take();
        let capture_id = lock(&self.sh.meta).capture_id.clone();
        if let Some(session) = session {
            let job = tokio::task::spawn_blocking(move || session.discard());
            let _ = tokio::time::timeout(self.eng.opts.stop_timeout, job).await;
        }
        let holds = if keep_holds { self.holds.take() } else { None };
        self.holds = None;
        let value = json!({
            "recording_id": self.sh.id, "state": "discarded", "discarded": true,
            "capture_id": capture_id, "reason": reason,
        });
        self.publish(
            events::RECORDING_DISCARDED,
            json!({ "capture_id": capture_id, "reason": reason }),
        );
        if keep_holds {
            // A restart: the slot stays taken until the new take replaces it (R3-10).
            self.eng.conclude_keeping_slot(&self.sh, value.clone());
        } else {
            self.eng.conclude(&self.sh, value.clone());
        }
        Ok(DiscardDone { value, holds })
    }
}

// ---- stop result and JSON helpers ----------------------------------------------------------

struct StopDone {
    record: CaptureRecord,
    path: PathBuf,
    tracks: Vec<TrackInfo>,
    warnings: Vec<SessionNote>,
    paused: MediaTime,
    live_at: MediaTime,
    wall_end: MediaTime,
    stop_ms: f64,
}

impl StopDone {
    fn duration(&self) -> MediaTime {
        self.record.duration.unwrap_or_else(|| {
            MediaTime::from_ticks(
                (self.wall_end.ticks() - self.live_at.ticks() - self.paused.ticks()).max(0),
            )
        })
    }
}

/// Stops every track, closes the bundle (`stop_reason` and `started_by` go into the manifest)
/// and indexes it. Blocking.
fn finalize_blocking(
    session: RecordingSession,
    library: &Arc<Mutex<Library>>,
    reason: &str,
    by: &str,
    layout: Option<Value>,
) -> Result<StopDone, CmError> {
    let mut session = session;
    let mut stopped = session.stop()?;
    stopped.set_extra("stop_reason", json!(reason))?;
    stopped.set_extra("started_by", json!(by))?;
    // The camera layout at stop goes into `edits[0].layout` (the export reads it). Only when a
    // camera track really exists.
    if let Some(l) = layout {
        if stopped.tracks.iter().any(|t| t.kind == TrackKind::Camera) {
            stopped.set_extra(cm_library::rec::LAYOUT_KEY, l)?;
        }
    }
    let tracks = stopped.tracks.clone();
    let warnings = stopped.warnings.clone();
    let (paused, live_at, wall_end, stop_ms) = (
        stopped.paused,
        stopped.live_at,
        stopped.wall_end,
        stopped.stop_ms,
    );
    let mut lib = lock_library(library);
    let record = stopped.finish_into(&mut lib)?;
    let path = lib.bundle_path(&record);
    Ok(StopDone {
        record,
        path,
        tracks,
        warnings,
        paused,
        live_at,
        wall_end,
        stop_ms,
    })
}

/// For a stop with `reason: "permission_revoked"`: which permission and the message to show.
fn revoked_permission(reason: &str, warnings: &[SessionNote]) -> Option<(Permission, String)> {
    if reason != "permission_revoked" {
        return None;
    }
    warnings
        .iter()
        .find(|w| w.code == "permission_revoked" && w.level == NoteLevel::StopRequested)
        .and_then(|w| w.permission.map(|p| (p, w.message.clone())))
}

fn outcome_json(sh: &Shared, d: &StopDone, reason: &str) -> Value {
    let dur = d.duration();
    let mut v = json!({
        "recording_id": sh.id,
        "capture_id": d.record.id,
        "title": d.record.title,
        "path": d.path,
        "state": "stopped",
        "reason": reason,
        "duration_s": dur.as_secs_f64(),
        "duration_ms": dur.as_millis_floor(),
        "paused_ms": d.paused.as_millis_floor(),
        "live_at_ms": d.live_at.as_millis_floor(),
        "stop_ms": d.stop_ms,
        "size_bytes": d.record.size_bytes,
        "tracks": d.tracks.iter().map(track_json).collect::<Vec<_>>(),
        "warnings": d.warnings.iter().map(note_json).collect::<Vec<_>>(),
        "restart_of": sh.restart_of,
    });
    if let Some((p, message)) = revoked_permission(reason, &d.warnings) {
        v["permission"] = json!(p.as_str());
        v["message"] = json!(message);
    }
    v
}

fn track_json(t: &TrackInfo) -> Value {
    json!({
        "kind": t.kind,
        "file": t.file,
        "start_ms": t.start.as_millis_floor(),
        "end_ms": t.end.as_millis_floor(),
        "stats": t.stats,
    })
}

fn note_json(n: &SessionNote) -> Value {
    json!({
        "track": n.track.map(TrackKind::as_str),
        "code": n.code,
        "message": n.message,
        "level": match n.level {
            NoteLevel::Info => "info",
            NoteLevel::Warning => "warning",
            NoteLevel::StopRequested => "stop_requested",
        },
        "at_ms": n.at.map(MediaTime::as_millis_floor),
        "permission": n.permission.map(Permission::as_str),
    })
}

fn error_json(e: &DaemonError) -> Value {
    json!({ "code": e.code().map_or("internal", cm_core::ErrorCode::as_str), "message": e.to_string() })
}

/// `mm:ss` (or `h:mm:ss`) of a millisecond count.
pub fn clock_text(ms: i64) -> String {
    let s = (ms / 1000).max(0);
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
    } else {
        format!("{:02}:{:02}", s / 60, s % 60)
    }
}

fn duration_words(secs: f64) -> String {
    let s = secs.round() as u64;
    match (s / 3600, (s / 60) % 60, s % 60) {
        (0, 0, s) => format!("{s} s"),
        (0, m, 0) => format!("{m} min"),
        (0, m, s) => format!("{m} min {s} s"),
        (h, 0, _) => format!("{h} h"),
        (h, m, _) => format!("{h} h {m} min"),
    }
}

/// `disk_space` dialog row: free space and what it is in minutes at the planned bitrate.
fn disk_text(free_bytes: u64, minutes_left: f64) -> String {
    let left = if minutes_left >= 600.0 {
        format!("{:.0} h", minutes_left / 60.0)
    } else if minutes_left >= 120.0 {
        format!("{:.1} h", minutes_left / 60.0)
    } else {
        format!("{minutes_left:.0} min")
    };
    format!(
        "{:.1} GB free, room for about {left} at this setting (an estimate)",
        free_bytes as f64 / 1e9
    )
}

/// The `disk_space` row as typed parameters (P3): free space in GB and the minutes it holds.
fn disk_params(free_bytes: u64, minutes_left: f64) -> Value {
    json!({ "free_gb": free_bytes as f64 / 1e9, "minutes_left": minutes_left })
}

/// The tracks a start will try to record, for `recording.starting`.
fn tracks_planned(r: &ResolvedStart) -> Vec<&'static str> {
    let mut v = vec!["screen"];
    if r.camera.is_some() {
        v.push("camera");
    }
    if r.mic.is_some() {
        v.push("mic");
    }
    if r.system_audio {
        v.push("system");
    }
    if r.cursor {
        v.push("cursor");
    }
    v
}

/// The live numbers of a [`SessionStatus`] for the HUD.
fn session_status_json(st: &SessionStatus) -> Value {
    let mic = st
        .tracks
        .iter()
        .find(|t| t.kind == TrackKind::Mic)
        .and_then(|t| t.level);
    let screen = st.tracks.iter().find(|t| t.kind == TrackKind::Screen);
    let live = |key: &str| screen.map_or(Value::Null, |t| t.live[key].clone());
    json!({
        "recorded_ms": st.recorded.as_millis_floor(),
        "elapsed_ms": st.elapsed.as_millis_floor(),
        "paused_ms": st.paused_total.as_millis_floor(),
        "live_at_ms": st.live_at.as_millis_floor(),
        "session_state": st.state,
        "tracks": st.tracks,
        "mic_level_dbfs": mic.map(|l| l.peak_dbfs),
        "mic_rms_dbfs": mic.map(|l| l.rms_dbfs),
        "mic_clip_events": mic.map(|l| l.clip_events),
        "late_frame_pct": live("late_pct"),
        "recent_late_frame_pct": live("recent_late_pct"),
        "disk_left_minutes": st.disk_minutes_left.map_or_else(|| live("disk_left_minutes"), |m| json!(m)),
        "stop_requested": st.stop_requested,
        "mic_muted": st.mic_muted,
    })
}

fn build_status(sh: &Shared, meta: &Meta, fresh: Option<&SessionStatus>) -> Value {
    let mut v = json!({
        "recording_id": sh.id,
        "state": meta.phase.as_str(),
        "capture_id": meta.capture_id,
        "title": meta.title,
        "countdown_remaining": meta.countdown_remaining,
        "started_by": sh.by,
        "started_at": meta.started_at,
        "restart_of": sh.restart_of,
        "max_duration_s": sh.resolved.max_duration_s,
        "pauses": meta.pauses,
        "stop_reason": meta.stop_reason,
        "warnings": meta.warnings,
        "indicator": sh.indicator,
        "recorded_ms": 0,
        "elapsed_ms": 0,
        "paused_ms": 0,
    });
    let live = match fresh {
        Some(st) => Some(session_status_json(st)),
        None => meta.status.clone(),
    };
    if let (Some(Value::Object(src)), Some(dst)) = (live, v.as_object_mut()) {
        for (k, x) in src {
            dst.insert(k, x);
        }
    }
    v
}

fn status_value(sh: &Shared) -> Value {
    let fresh = sh
        .session
        .try_lock()
        .ok()
        .and_then(|g| g.as_ref().map(RecordingSession::status));
    let meta = lock(&sh.meta);
    build_status(sh, &meta, fresh.as_ref())
}

// ---- the indicator facts and the dialog text ------------------------------------------------

fn rect4(r: &cm_core::PhysicalRect) -> [i64; 4] {
    [
        i64::from(r.origin.x),
        i64::from(r.origin.y),
        i64::from(r.size.width),
        i64::from(r.size.height),
    ]
}

fn indicator_json(
    r: &ResolvedStart,
    plan: &CapturePlan,
    targets: &Targets,
    actor: &Actor,
) -> Value {
    let display = plan
        .display_id
        .and_then(|id| targets.displays.iter().find(|d| d.id == id));
    let (kind, window_id, bounds) = match plan.target {
        CaptureTarget::Display { .. } => ("display", None, display.map(|d| rect4(&d.bounds))),
        CaptureTarget::Region { .. } => (
            "region",
            None,
            display.zip(plan.crop).map(|(d, c)| {
                [
                    i64::from(d.bounds.origin.x) + i64::from(c.origin.x),
                    i64::from(d.bounds.origin.y) + i64::from(c.origin.y),
                    i64::from(c.size.width),
                    i64::from(c.size.height),
                ]
            }),
        ),
        CaptureTarget::Window { window_id } => (
            "window",
            Some(window_id),
            targets
                .windows
                .iter()
                .find(|w| w.id == window_id)
                .map(|w| rect4(&w.bounds)),
        ),
    };
    let started_by = match actor {
        Actor::User => json!({ "kind": "user", "client": null, "tool": null }),
        Actor::Agent(name) => json!({ "kind": "agent", "client": name, "tool": "start_recording" }),
    };
    json!({
        "area": {
            "kind": kind,
            "display_id": plan.display_id,
            "window_id": window_id,
            "bounds": bounds,
            "source_size": [plan.source_size.width, plan.source_size.height],
            "scale": plan.scale.get(),
        },
        "output": {
            "width": plan.video.width, "height": plan.video.height,
            "fps": plan.video.fps, "codec": plan.video.codec.as_str(),
        },
        "camera": r.camera.as_ref().map(super::params::CameraParams::bubble),
        "mic": r.mic.is_some(),
        "system_audio": r.system_audio,
        "cursor": r.cursor,
        "started_by": started_by,
    })
}

/// Longest agent- or app-supplied text shown in a dialog row (R2-10).
const DIALOG_TEXT_MAX: usize = 120;

/// Makes text an agent or another app controls safe to show in a confirmation row: control
/// characters (newlines included) become spaces, bidirectional and invisible formatting marks
/// are dropped (they can reorder or hide what follows), and the text is cut at 120 characters.
pub(crate) fn dialog_text(s: &str) -> String {
    let mut out = String::new();
    let mut n = 0;
    for c in s.chars() {
        let c = if c.is_control() { ' ' } else { c };
        if matches!(
            c,
            '\u{200B}'..='\u{200F}'
                | '\u{202A}'..='\u{202E}'
                | '\u{2060}'..='\u{2064}'
                | '\u{2066}'..='\u{2069}'
                | '\u{061C}'
                | '\u{FEFF}'
                | '\u{00AD}'
        ) {
            continue;
        }
        if n == DIALOG_TEXT_MAX {
            out.push('\u{2026}');
            break;
        }
        out.push(c);
        n += 1;
    }
    out
}

/// The dialog sentence and rows (AGT-2: "shows exactly what will happen").
fn describe(
    r: &ResolvedStart,
    plan: &CapturePlan,
    targets: &Targets,
    names: &DeviceNames,
    root: &Path,
    restart: bool,
) -> (String, Value, Value) {
    let display = plan
        .display_id
        .and_then(|id| targets.displays.iter().find(|d| d.id == id));
    let (short, long) = match plan.target {
        CaptureTarget::Display { display_id } => (
            format!("display {display_id}"),
            format!(
                "Display {display_id}{}, {} x {} px",
                display.map_or(String::new(), |d| format!(" \"{}\"", d.name)),
                plan.source_size.width,
                plan.source_size.height
            ),
        ),
        CaptureTarget::Window { window_id } => {
            let w = targets.windows.iter().find(|w| w.id == window_id);
            (
                w.map_or_else(
                    || format!("window {window_id}"),
                    |w| format!("the window \"{}\"", dialog_text(&w.title)),
                ),
                w.map_or_else(
                    || format!("Window {window_id}"),
                    |w| {
                        format!(
                            "Window \"{}\" of {}, {} x {} px (follows the window)",
                            dialog_text(&w.title),
                            dialog_text(&w.app),
                            plan.source_size.width,
                            plan.source_size.height
                        )
                    },
                ),
            )
        }
        CaptureTarget::Region { display_id, .. } => {
            let c = plan.crop;
            let (w, h) = (plan.source_size.width, plan.source_size.height);
            (
                format!("a {w} x {h} region of display {display_id}"),
                format!(
                    "Region {w} x {h} px at ({}, {}) on display {display_id}",
                    c.map_or(0, |c| c.origin.x),
                    c.map_or(0, |c| c.origin.y)
                ),
            )
        }
    };
    let codec = match plan.video.codec {
        cm_encode::Codec::Hevc => "HEVC",
        cm_encode::Codec::H264 => "H.264",
    };
    let output = format!(
        "{} x {} px, {} fps, {codec}, about {:.0} MB per minute (an estimate: the real size varies with what is on screen)",
        plan.video.width,
        plan.video.height,
        plan.video.fps,
        plan.video.disk_mb_per_minute()
    );
    let mic = match (&r.mic, &names.mic) {
        (None, _) => "Off".to_owned(),
        (Some(_), n) => format!(
            "On: {}{}",
            n.as_deref().unwrap_or("the default microphone"),
            if r.mic_denoise() {
                ", noise suppression on"
            } else {
                ""
            }
        ),
    };
    let camera = match (&r.camera, &names.camera) {
        (None, _) => "Off".to_owned(),
        (Some(c), n) => format!(
            "On: {}, {} (the camera light turns on)",
            n.as_deref().unwrap_or("the default camera"),
            c.resolution.as_str()
        ),
    };
    let mut sensors = Vec::new();
    if r.mic.is_some() {
        sensors.push("the microphone");
    }
    if r.camera.is_some() {
        sensors.push("the camera");
    }
    if r.system_audio {
        sensors.push("system audio");
    }
    let with = match sensors.as_slice() {
        [] => String::new(),
        [a] => format!(" with {a}"),
        [a, b] => format!(" with {a} and {b}"),
        [a, b, c] => format!(" with {a}, {b} and {c}"),
        _ => String::new(),
    };
    let lead = if restart {
        "Discard the current take and start over: record"
    } else {
        "Record"
    };
    let summary = format!(
        "{lead} {short} at {} x {}, {} fps{with}, and save it to {}",
        plan.video.width,
        plan.video.height,
        plan.video.fps,
        root.display()
    );
    let mut details = Map::new();
    details.insert("target".into(), json!(long));
    details.insert("output".into(), json!(output));
    details.insert("screen".into(), json!("On (always recorded)"));
    details.insert("microphone".into(), json!(mic));
    details.insert("camera".into(), json!(camera));
    details.insert(
        "system_audio".into(),
        json!(if r.system_audio { "On" } else { "Off" }),
    );
    details.insert(
        "cursor".into(),
        json!(if r.cursor {
            "Position, clicks and scroll direction (never keystrokes)"
        } else {
            "Off"
        }),
    );
    details.insert(
        "save_to".into(),
        json!(format!(
            "{}{}",
            root.display(),
            r.title
                .as_deref()
                .map_or(String::new(), |t| format!(" as \"{}\"", dialog_text(t)))
        )),
    );
    details.insert(
        "max_duration".into(),
        json!(r
            .max_duration_s
            .map_or_else(|| "No limit".to_owned(), duration_words)),
    );
    details.insert(
        "countdown".into(),
        json!(if r.countdown_s == 0 {
            "None".to_owned()
        } else {
            format!("{} s", r.countdown_s)
        }),
    );
    details.insert(
        "excluded_windows".into(),
        json!("CaptureMann's own windows and your password managers never appear"),
    );
    if restart {
        details.insert(
            "discarded".into(),
            json!("The current take is deleted and cannot be recovered"),
        );
    }
    let params = json!({
        "restart": restart,
        "target": target_params(plan, targets),
        "width": plan.video.width,
        "height": plan.video.height,
        "fps": plan.video.fps,
        "codec": codec,
        "mb_per_minute": plan.video.disk_mb_per_minute(),
        "microphone": r.mic.as_ref().map(|_| json!({
            "device": names.mic,
            "denoise": r.mic_denoise(),
        })),
        "camera": r.camera.as_ref().map(|c| json!({
            "device": names.camera,
            "resolution": c.resolution.as_str(),
        })),
        "system_audio": r.system_audio,
        "cursor": r.cursor,
        "save_to": root.display().to_string(),
        "title": r.title.as_deref().map(dialog_text),
        "max_duration_s": r.max_duration_s,
        "countdown_s": r.countdown_s,
    });
    (summary, Value::Object(details), params)
}

/// The target as typed parameters for the confirmation request (P3): what the dialog names,
/// with texts already made safe for display.
fn target_params(plan: &CapturePlan, targets: &Targets) -> Value {
    let size = (plan.source_size.width, plan.source_size.height);
    match plan.target {
        CaptureTarget::Display { display_id } => {
            let name = plan
                .display_id
                .and_then(|id| targets.displays.iter().find(|d| d.id == id))
                .map(|d| d.name.clone());
            json!({ "type": "display", "id": display_id, "name": name,
                    "width": size.0, "height": size.1 })
        }
        CaptureTarget::Window { window_id } => {
            let w = targets.windows.iter().find(|w| w.id == window_id);
            json!({ "type": "window", "id": window_id,
                    "title": w.map(|w| dialog_text(&w.title)),
                    "app": w.map(|w| dialog_text(&w.app)),
                    "width": size.0, "height": size.1 })
        }
        CaptureTarget::Region { display_id, .. } => {
            let c = plan.crop;
            json!({ "type": "region", "display_id": display_id,
                    "x": c.map_or(0, |c| c.origin.x), "y": c.map_or(0, |c| c.origin.y),
                    "width": size.0, "height": size.1 })
        }
    }
}

// ---- the 60 fps self-test (REC-3) ------------------------------------------------------------

fn selftest_busy() -> CmError {
    CmError::busy("The 60 fps self-test is running; wait for it to finish or stop it")
}

/// A validated, planned self-test: what the confirmation dialog shows.
#[derive(Debug)]
pub struct PreparedSelftest {
    resolved: ResolvedStart,
    plan: CapturePlan,
    targets: Targets,
    fingerprint: Fingerprint,
    indicator: Value,
    /// One sentence: what will happen.
    pub summary: String,
    /// Flat `label -> text` rows.
    pub details: Value,
    /// The same plan as typed parameters (P3), `duration_s` included.
    pub params: Value,
}

struct SelftestShared {
    id: String,
    by: String,
    cancelled: AtomicBool,
    wake: Notify,
    done: watch::Sender<bool>,
    /// The last status published (served by `get_recording_status`).
    status: Mutex<Value>,
}

/// Removes the self-test's temporary folder when dropped, however the test ends.
struct TempFolder(PathBuf);

impl TempFolder {
    fn create(path: PathBuf) -> Result<Self, CmError> {
        std::fs::create_dir_all(&path).map_err(|e| {
            CmError::internal(format!(
                "cannot create the self-test folder {}: {e}",
                path.display()
            ))
        })?;
        // The test video is a capture of the screen: owner only (R3-16).
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700));
        }
        Ok(TempFolder(path))
    }

    fn remove(&self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl Drop for TempFolder {
    fn drop(&mut self) {
        self.remove();
    }
}

/// Prefix of the self-test's temporary folders.
const SELFTEST_DIR_PREFIX: &str = "capturemann-selftest-";

/// Removes self-test folders in `dir` that a killed process left behind (75 to 100 MB of screen
/// capture each) and that are older than `min_age`. Returns how many were removed (R3-16).
pub(crate) fn sweep_selftest_dirs(dir: &Path, min_age: Duration) -> usize {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return 0;
    };
    let mut n = 0;
    for e in rd.flatten() {
        let name = e.file_name();
        if !name.to_string_lossy().starts_with(SELFTEST_DIR_PREFIX) {
            continue;
        }
        let old_enough = e
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age >= min_age);
        if old_enough && e.path().is_dir() && std::fs::remove_dir_all(e.path()).is_ok() {
            n += 1;
        }
    }
    n
}

/// Cancels the self-test when the caller of `run_fps_selftest` goes away.
struct CancelOnDrop(Option<Arc<SelftestShared>>);

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        if let Some(sh) = self.0.take() {
            sh.cancelled.store(true, Ordering::SeqCst);
            sh.wake.notify_one();
        }
    }
}

fn selftest_cancelled() -> DaemonError {
    DaemonError::Declined {
        reason: "selftest_cancelled",
        message: "The 60 fps self-test was cancelled".to_owned(),
    }
}

impl RecordingEngine {
    /// Whether the 60 fps self-test is running.
    pub fn is_selftest_active(&self) -> bool {
        lock(&self.inner.state).selftest.is_some()
    }

    /// Validates `run_fps_selftest` and plans it (target, size, codec, permissions) without
    /// creating anything. Errors here (`invalid_input`, `not_found`, `busy`,
    /// `permission_missing`) surface before the confirmation dialog opens.
    pub async fn prepare_selftest(
        &self,
        actor: &Actor,
        params: &ExternalParams,
    ) -> Result<PreparedSelftest, DaemonError> {
        let parsed = SelftestParams::parse(&params.rest)?;
        self.ensure_idle()?;
        let settings = self.inner.deps.settings.all().unwrap_or_default();
        let target = match parsed.target {
            Some(t) => t,
            None => {
                let eng = self.inner.clone();
                let targets =
                    crate::daemon::blocking(move || eng.deps.capturer.list_targets_with(false))
                        .await?;
                let d = targets
                    .displays
                    .iter()
                    .find(|d| d.is_primary)
                    .or_else(|| targets.displays.first())
                    .ok_or_else(|| {
                        CmError::not_found(
                            "No display found. If the screen is locked or asleep, unlock it and try again.",
                        )
                    })?;
                ScreenshotTarget::Display { display_id: d.id }
            }
        };
        let start = StartParams {
            target,
            size: parsed.size,
            fps: Some(60),
            codec: parsed.codec,
            quality: None,
            camera: None,
            mic: None,
            system_audio: false,
            cursor: Some(false),
            cursor_separate: None,
            max_duration_s: None,
            countdown_s: Some(0),
            title: None,
            exclude: ExcludeParams::default(),
        };
        let mut resolved = start.resolve(&settings);
        resolved.max_duration_s = Some(self.inner.opts.selftest_duration.as_secs_f64());
        let exclusions = self.exclusions(&resolved, &settings);
        let (plan, targets, _warnings, names, root) =
            self.plan_blocking(&resolved, exclusions).await?;
        let fingerprint = Fingerprint::new(
            plan.display_id,
            &plan.video,
            resolved.size,
            resolved.codec,
            &self.inner.opts.app_version,
        );
        let mut indicator = indicator_json(&resolved, &plan, &targets, actor);
        if let Some(o) = indicator.as_object_mut() {
            o.insert("selftest".into(), json!(true));
            if let Some(by) = o.get_mut("started_by").and_then(Value::as_object_mut) {
                by.insert("tool".into(), json!("run_fps_selftest"));
            }
        }
        let secs = self.inner.opts.selftest_duration.as_secs_f64();
        let (_, mut details, mut params) =
            describe(&resolved, &plan, &targets, &names, &root, false);
        if let Some(o) = params.as_object_mut() {
            o.insert("duration_s".into(), json!(secs));
        }
        let target_text = details["target"]
            .as_str()
            .unwrap_or("the screen")
            .to_owned();
        let summary = format!(
            "Run the 60 fps self-test: record {} at {} x {}, 60 fps, for {} to check that this \
             machine keeps up. Nothing is saved: the test video is deleted when the test ends",
            target_text.split(',').next().unwrap_or("the screen"),
            plan.video.width,
            plan.video.height,
            duration_words(secs),
        );
        if let Some(o) = details.as_object_mut() {
            o.shift_remove("countdown");
            o.insert(
                "save_to".into(),
                json!("Nothing is saved: the test video is deleted when the test ends"),
            );
            o.insert("max_duration".into(), json!(duration_words(secs)));
            o.insert(
                "purpose".into(),
                json!("Measures late and dropped frames at 60 fps; only the result (pass or fail) is kept"),
            );
        }
        Ok(PreparedSelftest {
            resolved,
            plan,
            targets,
            fingerprint,
            indicator,
            summary,
            details,
            params,
        })
    }

    fn reserve_selftest(
        &self,
        actor: &Actor,
        prep: &PreparedSelftest,
    ) -> Result<Arc<SelftestShared>, DaemonError> {
        let mut st = lock(&self.inner.state);
        if st.active.is_some() {
            return Err(CmError::busy("A recording is already running").into());
        }
        if st.selftest.is_some() {
            return Err(selftest_busy().into());
        }
        st.seq += 1;
        let ms = self.inner.deps.clock.now().unix_timestamp_nanos() / 1_000_000;
        let id = format!("st_{ms:012x}_{:04}", st.seq);
        let total = self.inner.opts.selftest_duration.as_secs_f64();
        let (done, _) = watch::channel(false);
        let sh = Arc::new(SelftestShared {
            status: Mutex::new(json!({
                "selftest_id": id, "state": "starting", "total_s": total,
                "remaining_s": total.ceil(), "late_pct": 0.0, "indicator": prep.indicator,
            })),
            id,
            by: actor.to_string(),
            cancelled: AtomicBool::new(false),
            wake: Notify::new(),
            done,
        });
        st.selftest = Some(sh.clone());
        Ok(sh)
    }

    /// Runs the 60 fps self-test and returns its verdict (`state: "passed" | "failed"`). Takes
    /// the length of the test. If the caller goes away the test is cancelled and its output
    /// deleted. Fails with `busy` while a recording or another self-test runs.
    pub async fn run_selftest(
        &self,
        actor: &Actor,
        prep: PreparedSelftest,
    ) -> Result<Value, DaemonError> {
        let sh = self.reserve_selftest(actor, &prep)?;
        let mut guard = CancelOnDrop(Some(sh.clone()));
        let (tx, rx) = oneshot::channel();
        let eng = self.inner.clone();
        let task_sh = sh.clone();
        let handle = tokio::spawn(async move {
            let outcome = selftest_body(&eng, &task_sh, prep).await;
            eng.selftest_finish(&task_sh);
            let _ = tx.send(outcome);
        });
        let eng = self.inner.clone();
        let watch_sh = sh.clone();
        tokio::spawn(async move {
            if handle.await.is_err() {
                // The task died: leave no half-state behind. Its temp folder was dropped with it.
                eng.publish_selftest(
                    &watch_sh,
                    json!({ "state": "error",
                            "error": { "code": "internal", "message": "the self-test stopped unexpectedly" } }),
                );
                eng.selftest_finish(&watch_sh);
            }
        });
        let out = match rx.await {
            Ok(r) => r,
            Err(_) => Err(DaemonError::Internal(
                "the self-test stopped unexpectedly".to_owned(),
            )),
        };
        guard.0 = None;
        out
    }

    /// Cancels the running self-test (`id`: its id, or `None` for whichever runs) and waits until
    /// its output is deleted. `None` when there is no such self-test.
    pub async fn cancel_selftest(&self, id: Option<&str>) -> Option<Value> {
        let sh = lock(&self.inner.state)
            .selftest
            .clone()
            .filter(|s| id.is_none_or(|i| i == s.id))?;
        let mut rx = sh.done.subscribe();
        sh.cancelled.store(true, Ordering::SeqCst);
        sh.wake.notify_one();
        let wait = self.inner.opts.stop_timeout + Duration::from_secs(5);
        let _ = tokio::time::timeout(wait, rx.wait_for(|d| *d)).await;
        Some(json!({ "selftest_id": sh.id, "state": "cancelled", "cancelled": true }))
    }

    fn selftest_status(&self) -> Option<Value> {
        lock(&self.inner.state)
            .selftest
            .as_ref()
            .map(|s| lock(&s.status).clone())
    }
}

impl EngineInner {
    fn publish_selftest(&self, sh: &SelftestShared, mut data: Value) {
        if let Some(o) = data.as_object_mut() {
            o.insert("selftest_id".into(), json!(sh.id));
            o.insert("by".into(), json!(sh.by));
            let mut st = lock(&sh.status);
            if let Some(cur) = st.as_object_mut() {
                for (k, v) in o.iter() {
                    cur.insert(k.clone(), v.clone());
                }
            }
        }
        self.deps.events.publish(events::RECORDING_SELFTEST, data);
    }

    fn selftest_finish(&self, sh: &SelftestShared) {
        {
            let mut st = lock(&self.state);
            if st.selftest.as_ref().is_some_and(|s| s.id == sh.id) {
                st.selftest = None;
            }
        }
        let _ = sh.done.send(true);
    }
}

/// The body of one self-test: record into a temp folder, measure, delete, store the verdict.
async fn selftest_body(
    eng: &Arc<EngineInner>,
    sh: &Arc<SelftestShared>,
    prep: PreparedSelftest,
) -> Result<Value, DaemonError> {
    let include_upload = eng
        .deps
        .settings
        .all()
        .ok()
        .and_then(|s| s.get(PAUSE_UPLOADS_SETTING).and_then(Value::as_bool))
        .unwrap_or(true);
    // The same background work pauses as for a recording, so it does not skew the measurement.
    let _holds = JobHolds::acquire(eng.deps.jobs.as_ref(), include_upload);
    let folder = TempFolder::create(
        eng.opts
            .selftest_dir
            .clone()
            .unwrap_or_else(std::env::temp_dir)
            .join(format!("{SELFTEST_DIR_PREFIX}{}", sh.id)),
    )?;
    let total = eng.opts.selftest_duration;
    let total_s = total.as_secs_f64();
    let cancelled = |eng: &Arc<EngineInner>| {
        eng.publish_selftest(sh, json!({ "state": "cancelled", "remaining_s": 0 }));
        Err(selftest_cancelled())
    };
    if sh.cancelled.load(Ordering::SeqCst) {
        return cancelled(eng);
    }

    // Begin: the same tracks as a recording, minus every sensor, into the temp folder.
    // R3-4 / R4-9: the indicator facts go out before the tracks are built, like
    // `recording.starting` for a recording: the self-test captures real pixels, so the HUD and
    // the indicator must not wait for the slowest start.
    eng.publish_selftest(
        sh,
        json!({ "state": "started", "total_s": total_s, "remaining_s": total_s.ceil(),
                "late_pct": 0.0, "indicator": prep.indicator }),
    );
    let begin = {
        let eng = eng.clone();
        let resolved = prep.resolved.clone();
        let plan = prep.plan.clone();
        let targets = prep.targets.clone();
        let parent = folder.0.clone();
        let by = sh.by.clone();
        tokio::task::spawn_blocking(move || {
            let tracks = eng.factory.build(&BuildContext {
                params: &resolved,
                plan: &plan,
                targets: &targets,
            })?;
            let mut cfg = SessionConfig::new(&parent);
            cfg.title = Some("60 fps self-test".to_owned());
            let mut settings = resolved.settings_json(&by);
            if let Some(o) = settings.as_object_mut() {
                o.insert("selftest".into(), json!(true));
            }
            cfg.settings = settings;
            cfg.displays = targets
                .displays
                .iter()
                .map(|d| SessionDisplay {
                    id: d.id,
                    name: d.name.clone(),
                    bounds: d.bounds,
                    scale: d.scale,
                    is_primary: d.is_primary,
                })
                .collect();
            cfg.app_version = eng.opts.app_version.clone();
            cfg.host = eng.opts.host.clone();
            cfg.free_space = eng.opts.free_space.clone();
            cfg.disk_check_every = eng.opts.disk_check_every;
            RecordingSession::begin(cfg, tracks)
        })
        .await
    };
    let session = match begin {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => return fail_selftest(eng, sh, e.into()),
        Err(e) => {
            return fail_selftest(
                eng,
                sh,
                CmError::internal(format!("worker failed: {e}")).into(),
            )
        }
    };
    let slot = Arc::new(Mutex::new(Some(session)));

    // Run for the whole length, reporting each tick.
    enum Ended {
        Elapsed,
        Cancelled,
        Halted(String, String),
    }
    let deadline = tokio::time::Instant::now() + total;
    let mut ticker = tokio::time::interval(eng.opts.tick_every);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    ticker.tick().await;
    let mut last_late = 0.0_f64;
    let mut reported_running = false;
    let ended = loop {
        if sh.cancelled.load(Ordering::SeqCst) {
            break Ended::Cancelled;
        }
        // `biased`, ticker first: after a stall the tick and the deadline are both ready, and a
        // random pick could skip every `running` report of a short test (flaky test fix).
        tokio::select! {
            biased;
            _ = ticker.tick() => {
                eng.factory.tick_hook();
                let s2 = slot.clone();
                let ticked = tokio::task::spawn_blocking(move || {
                    lock(&s2).as_mut().map(|s| (s.tick(), s.status()))
                })
                .await
                .ok()
                .flatten();
                let Some((notes, st)) = ticked else { continue };
                last_late = st
                    .tracks
                    .iter()
                    .find(|t| t.kind == TrackKind::Screen)
                    .and_then(|t| t.live["late_pct"].as_f64())
                    .unwrap_or(last_late);
                let left = deadline.saturating_duration_since(tokio::time::Instant::now());
                eng.publish_selftest(
                    sh,
                    json!({ "state": "running", "remaining_s": left.as_secs_f64().ceil(),
                            "late_pct": last_late, "total_s": total_s }),
                );
                reported_running = true;
                if let Some(n) = notes.iter().find(|n| n.level == NoteLevel::StopRequested) {
                    break Ended::Halted(n.code.clone(), n.message.clone());
                }
            }
            () = sh.wake.notified() => {}
            () = tokio::time::sleep_until(deadline) => break Ended::Elapsed,
        }
    };
    if matches!(ended, Ended::Elapsed) && !reported_running {
        // A stall swallowed every tick: say how far the test got before the verdict.
        eng.publish_selftest(
            sh,
            json!({ "state": "running", "remaining_s": 0, "late_pct": last_late,
                    "total_s": total_s }),
        );
    }

    let taken = lock(&slot).take();
    let Some(mut session) = taken else {
        return fail_selftest(
            eng,
            sh,
            CmError::internal("the self-test session is gone").into(),
        );
    };
    let stop_timeout = eng.opts.stop_timeout;
    match ended {
        Ended::Cancelled | Ended::Halted(..) => {
            let job = tokio::task::spawn_blocking(move || session.discard());
            let _ = tokio::time::timeout(stop_timeout, job).await;
            folder.remove();
            return match ended {
                Ended::Halted(code, message) => {
                    let e = if code == "disk_full" {
                        CmError::disk_full(message)
                    } else {
                        CmError::internal(format!("The self-test stopped early: {message}"))
                    };
                    fail_selftest(eng, sh, e.into())
                }
                _ => cancelled(eng),
            };
        }
        Ended::Elapsed => {}
    }
    let stopping = tokio::task::spawn_blocking(move || session.stop());
    let stopped = match tokio::time::timeout(stop_timeout, stopping).await {
        Ok(Ok(Ok(s))) => s,
        Ok(Ok(Err(e))) => return fail_selftest(eng, sh, e.into()),
        Ok(Err(e)) => {
            return fail_selftest(
                eng,
                sh,
                CmError::internal(format!("worker failed: {e}")).into(),
            )
        }
        Err(_) => {
            return fail_selftest(
                eng,
                sh,
                CmError::internal("Stopping the self-test took too long").into(),
            )
        }
    };
    let screen = stopped
        .tracks
        .iter()
        .find(|t| t.kind == TrackKind::Screen)
        .cloned();
    drop(stopped);
    // The test video has done its job: delete it before anything is reported.
    folder.remove();
    let Some(screen) = screen else {
        return fail_selftest(
            eng,
            sh,
            CmError::internal("the self-test recorded no screen track").into(),
        );
    };

    // Measure.
    let stats = screen.stats;
    let frames = stats.written;
    let dropped = stats.dropped;
    let dropped_pct = 100.0 * dropped as f64 / (frames + dropped).max(1) as f64;
    let late_pct = screen.details["late_pct"]
        .as_f64()
        .unwrap_or_else(|| 100.0 * stats.late as f64 / (frames + dropped).max(1) as f64);
    let stall_ms = screen.details["stall_ms"].as_u64().unwrap_or(0);
    let stalls = screen.details["stalls"].as_u64().unwrap_or(0);
    let passed = selftest::judge_with_stalls(late_pct, dropped_pct, frames, stall_ms);
    let now = eng.deps.clock.now();
    let fp = &prep.fingerprint;
    let entry = SelftestEntry {
        passed,
        late_pct,
        dropped,
        dropped_pct,
        frames,
        at: rfc3339(now),
        display: fp.display,
        size: [fp.width, fp.height],
        codec: fp.codec.clone(),
        app_version: fp.app_version.clone(),
        duration_s: total_s,
        stall_ms,
    };

    // Store the verdict (a settings key; the desktop persists it in settings.json).
    let store = {
        let settings = eng.deps.settings.clone();
        let entry = entry.clone();
        tokio::task::spawn_blocking(move || -> Result<Map<String, Value>, CmError> {
            let all = settings.all()?;
            let value = selftest::store_value(&all, entry, now);
            let mut m = Map::new();
            m.insert(selftest::SETTING_KEY.to_owned(), value);
            settings.set(&m)
        })
        .await
    };
    let (stored, store_error, after) = match store {
        Ok(Ok(all)) => (true, None, all),
        Ok(Err(e)) => (false, Some(e.to_string()), Map::new()),
        Err(e) => (false, Some(format!("worker failed: {e}")), Map::new()),
    };
    // The same answer a following `start_recording { fps: 60 }` will get.
    let verdict = if stored {
        selftest::evaluate(&after, fp, now)
    } else {
        Fps60Verdict::Allowed
    };
    let mut result = json!({
        "selftest_id": sh.id,
        "state": if passed { "passed" } else { "failed" },
        "passed": passed,
        "fps": 60,
        "late_pct": late_pct,
        "dropped": dropped,
        "dropped_pct": dropped_pct,
        "frames": frames,
        "stalls": stalls,
        "stall_ms": stall_ms,
        "duration_s": total_s,
        "size": [fp.width, fp.height],
        "size_label": fp.size_label(),
        "codec": fp.codec,
        "display_id": fp.display,
        "at": entry.at,
        "threshold": { "late_pct": selftest::MAX_LATE_PCT, "dropped_pct": selftest::MAX_DROPPED_PCT },
        "stored": stored,
        "store_error": store_error,
        "setting": selftest::SETTING_KEY,
        "message": if passed {
            format!("60 fps is stable on this machine at {}", fp.size_label())
        } else {
            String::new()
        },
    });
    if let (Fps60Verdict::Rejected(r), Some(o)) = (&verdict, result.as_object_mut()) {
        if !passed || stored {
            o.insert("message".into(), json!(r.message));
            o.insert("suggestion".into(), r.suggestion.clone());
        }
    }
    eng.publish_selftest(
        sh,
        json!({ "state": if passed { "passed" } else { "failed" }, "remaining_s": 0,
                "late_pct": late_pct, "result": result }),
    );
    Ok(result)
}

/// Reports a self-test that could not finish and returns the error.
fn fail_selftest(
    eng: &Arc<EngineInner>,
    sh: &SelftestShared,
    e: DaemonError,
) -> Result<Value, DaemonError> {
    eng.publish_selftest(
        sh,
        json!({ "state": "error", "remaining_s": 0, "error": error_json(&e) }),
    );
    Err(e)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dialog_text_strips_controls_and_bidi_and_bounds_the_length() {
        assert_eq!(dialog_text("a\nb\tc\u{202E}d\u{200B}e"), "a b cde");
        let long = "x".repeat(500);
        let t = dialog_text(&long);
        assert_eq!(t.chars().count(), DIALOG_TEXT_MAX + 1);
        assert!(t.ends_with('\u{2026}'));
        assert_eq!(dialog_text("plain title"), "plain title");
    }

    /// R3-16: a killed self-test's folder is swept at startup when it is old enough; recent
    /// ones (another running test) and unrelated folders are left alone.
    #[cfg(unix)]
    #[test]
    fn r3_16_old_selftest_folders_are_swept() {
        let tmp = tempfile::tempdir().unwrap();
        let old = tmp.path().join(format!("{SELFTEST_DIR_PREFIX}st_old"));
        let recent = tmp.path().join(format!("{SELFTEST_DIR_PREFIX}st_recent"));
        let other = tmp.path().join("something-else");
        for d in [&old, &recent, &other] {
            std::fs::create_dir_all(d.join("inner")).unwrap();
            std::fs::write(d.join("inner/screen.mp4"), b"x").unwrap();
        }
        let long_ago = std::time::SystemTime::now() - Duration::from_secs(3600);
        for d in [&old, &other] {
            std::fs::File::open(d)
                .unwrap()
                .set_modified(long_ago)
                .unwrap();
        }
        assert_eq!(sweep_selftest_dirs(tmp.path(), Duration::from_secs(60)), 1);
        assert!(!old.exists());
        assert!(recent.exists() && other.exists());
        // The self-test's own folder is created owner-only.
        let f = TempFolder::create(tmp.path().join("capturemann-selftest-perm")).unwrap();
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&f.0).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o700);
        }
    }
}
