//! [`Daemon`]: the action bus. Wires policy, idempotency, the confirmation gate, per-capture
//! serialization, the engines and the event stream behind one entry point.
//!
//! Order of checks for [`Daemon::execute`]:
//!
//! 1. Per-client policy (agents only): blocked or read-only clients get `user_declined`
//!    with `data.reason`, no dialog (AGT-2.AC4).
//! 2. Idempotency (`request_id`): a repeat returns the first result, even for Confirm-tier
//!    actions, without asking again (AGT-3.AC4). Keyed by actor + `request_id`, scoped to the
//!    action name; only successes are remembered, so a failed call can be retried.
//! 3. Confirmation (Confirm tier, agents only): queued, one dialog at a time, 5 min. The tier an
//!    agent's call is gated by is the *effective* tier: the table's tier changed by the user's
//!    per-tool permissions (`agents.tool_policy`, [`crate::tool_policy`]), read at call time.
//! 4. The action runs; actions on one capture are serialized (AGT-3).
//!
//! The user (`Actor::User`, the in-process GUI) skips 1 and 3: the GUI has its own confirmation
//! UI. Socket clients are always agents (see [`crate::rpc`]).

use cm_capture::Capturer;
use cm_core::{Actor, CmError};
use cm_library::Library;
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;
use tokio::sync::broadcast;

use crate::action::{Action, ActionSpec, Tier, ACTIONS};
use crate::ai::AiServices;
use crate::clock::{Clock, SystemClock};
use crate::engines::{
    Clipboard, ExternalActions, MemorySettings, NoClipboard, NoExternalActions, SettingsStore,
};
use crate::error::DaemonError;
use crate::events::{self, Event, EventBus, JobEvents};
use crate::frames::{FrameCache, DEFAULT_FRAME_TTL};
use crate::gate::{
    ActivityDecision, ActivityEntry, ActivityLog, Approval, ClientPolicies, ConfirmationGate,
    Confirmer, DenyAll, DEFAULT_CONFIRM_TIMEOUT,
};
use crate::jobs::JobsRuntime;
use crate::library_ops::{SystemTrasher, Trasher};
use crate::recording::engine::EngineDeps;
use crate::recording::{
    EngineOptions, PreparedSelftest, PreparedStart, RecordingEngine, SourceFactory, SystemSources,
};
use crate::recovery::{recover_recordings, RecoveredRecordings};

/// One request to the bus.
#[derive(Debug, Clone)]
pub struct ActionCall {
    /// Who asks. Agents are subject to policy and the confirmation gate.
    pub actor: Actor,
    /// Connection id (see [`Daemon::new_session`]); 0 means "no session". Scopes "allow for
    /// this session".
    pub session: u64,
    pub action: Action,
}

impl ActionCall {
    pub fn new(actor: Actor, action: Action) -> Self {
        ActionCall {
            actor,
            session: 0,
            action,
        }
    }
}

/// Per-key async locks (per capture, per `request_id`).
#[derive(Default)]
pub(crate) struct LockMap {
    map: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

impl LockMap {
    pub(crate) async fn lock(&self, key: &str) -> tokio::sync::OwnedMutexGuard<()> {
        let m = {
            let mut g = self.map.lock().unwrap_or_else(PoisonError::into_inner);
            if g.len() > 256 {
                g.retain(|_, l| Arc::strong_count(l) > 1);
            }
            g.entry(key.to_owned()).or_default().clone()
        };
        m.lock_owned().await
    }
}

/// Cancel tokens of in-flight `capture_screenshot` calls (R1-11, CAP-5.AC2).
#[derive(Default)]
pub(crate) struct DelayedCaptures {
    next: AtomicU64,
    map: Mutex<HashMap<u64, (Option<String>, cm_capture::CancelToken)>>,
}

/// Cancels its capture when dropped, and unregisters it. Dropping the `execute` future (the
/// connection closed) therefore ends a capture that is still waiting out its delay.
pub(crate) struct DelayGuard<'a> {
    reg: &'a DelayedCaptures,
    id: u64,
    token: cm_capture::CancelToken,
}

impl DelayGuard<'_> {
    pub(crate) fn token(&self) -> cm_capture::CancelToken {
        self.token.clone()
    }
}

impl Drop for DelayGuard<'_> {
    fn drop(&mut self) {
        self.token.cancel();
        self.reg
            .map
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&self.id);
    }
}

impl DelayedCaptures {
    pub(crate) fn register(&self, request_id: Option<String>) -> DelayGuard<'_> {
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        let token = cm_capture::CancelToken::new();
        self.map
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(id, (request_id, token.clone()));
        DelayGuard {
            reg: self,
            id,
            token,
        }
    }

    fn cancel_matching(&self, pred: impl Fn(&Option<String>) -> bool) -> usize {
        let g = self.map.lock().unwrap_or_else(PoisonError::into_inner);
        let mut n = 0;
        for (rid, token) in g.values() {
            if pred(rid) && !token.is_cancelled() {
                token.cancel();
                n += 1;
            }
        }
        n
    }
}

pub(crate) struct Inner {
    pub(crate) capturer: Arc<Capturer>,
    pub(crate) library: Arc<Mutex<Library>>,
    pub(crate) clipboard: Arc<dyn Clipboard>,
    /// On-device text recognition behind `suggest_redactions` (ANN-2.AC4).
    pub(crate) recognizer: Arc<dyn cm_render::ocr::TextRecognizer>,
    pub(crate) settings: Arc<dyn SettingsStore>,
    pub(crate) external: Arc<dyn ExternalActions>,
    pub(crate) trasher: Arc<dyn Trasher>,
    /// Free space of a volume (the library move's space check).
    pub(crate) free_space: Arc<dyn cm_capture::disk::FreeSpace>,
    /// Copies the files of a library move.
    pub(crate) move_copier: Arc<dyn crate::library_move::FileCopier>,
    pub(crate) clock: Arc<dyn Clock>,
    pub(crate) events: EventBus,
    /// The engines behind the AI actions (M5); [`AiServices::default`] has none.
    pub(crate) ai: AiServices,
    /// The destinations file (M6); `None` means no destination exists (headless tests).
    pub(crate) destinations: Option<cm_storage::DestinationStore>,
    /// Providers and token sources of the cloud destinations (STO-2).
    pub(crate) cloud: crate::storage_cloud::CloudBackend,
    /// The publish accounts file (PUB-1); `None` means publishing is not set up (headless tests).
    pub(crate) publish_accounts: Option<cm_publish::PublishAccountStore>,
    /// Retry and chunk settings of the YouTube provider (tests shorten the backoff).
    pub(crate) youtube: cm_publish::YouTubeConfig,
    /// Keychain access and last test result of the AI-7 text model.
    pub(crate) text_model: crate::text_model_ops::TextModelState,
    gate: ConfirmationGate,
    policies: ClientPolicies,
    activity: ActivityLog,
    idempotency: Mutex<crate::idem::ResultCache>,
    request_locks: LockMap,
    pub(crate) capture_locks: Arc<LockMap>,
    /// Delayed screenshots that can still be cancelled (CAP-5.AC2).
    pub(crate) delayed: DelayedCaptures,
    pub(crate) frames: Arc<FrameCache>,
    /// The persistent job queue, when one was configured ([`DaemonBuilder::jobs`]).
    pub(crate) jobs: Option<JobsRuntime>,
    /// The recording engine, when one was configured ([`DaemonBuilder::recording_sources`]).
    pub(crate) recording: Option<RecordingEngine>,
    next_session: AtomicU64,
    confirm_timeout: Duration,
    /// The library watcher task, once started (ends when the daemon is dropped).
    pub(crate) watcher: Mutex<Option<crate::watcher::WatcherGuard>>,
    /// The running camera watcher; dropping it (or `stop`) ends its thread.
    camera_watch: Mutex<Option<Box<dyn cm_camera::CameraWatcher>>>,
}

pub(crate) fn lock_library(l: &Mutex<Library>) -> MutexGuard<'_, Library> {
    l.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Runs blocking engine work (capture delay, disk, SQLite, rendering) off the async threads.
pub(crate) async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, CmError> + Send + 'static,
) -> Result<T, DaemonError> {
    match tokio::task::spawn_blocking(f).await {
        Ok(r) => r.map_err(DaemonError::from),
        Err(e) => Err(DaemonError::Internal(format!("worker failed: {e}"))),
    }
}

/// The daemon core. Cheap to clone (all clones share state).
#[derive(Clone)]
pub struct Daemon {
    pub(crate) inner: Arc<Inner>,
}

/// Builds a [`Daemon`] from its engines. Everything but the capturer and the library has a
/// headless default: no clipboard, in-memory settings, no recording/publishing engine, and a
/// confirmer that declines.
pub struct DaemonBuilder {
    capturer: Capturer,
    library: Library,
    clipboard: Arc<dyn Clipboard>,
    recognizer: Arc<dyn cm_render::ocr::TextRecognizer>,
    settings: Arc<dyn SettingsStore>,
    external: Arc<dyn ExternalActions>,
    trasher: Arc<dyn Trasher>,
    free_space: Arc<dyn cm_capture::disk::FreeSpace>,
    move_copier: Arc<dyn crate::library_move::FileCopier>,
    ai: AiServices,
    destinations: Option<cm_storage::DestinationStore>,
    cloud: crate::storage_cloud::CloudBackend,
    publish_accounts: Option<cm_publish::PublishAccountStore>,
    youtube: cm_publish::YouTubeConfig,
    text_secrets: Option<Arc<dyn cm_auth::SecretStore>>,
    confirmer: Arc<dyn Confirmer>,
    confirm_timeout: Duration,
    clock: Arc<dyn Clock>,
    frame_ttl: Duration,
    jobs: Option<cm_jobs::JobQueue>,
    recording: Option<Arc<dyn SourceFactory>>,
    recording_options: EngineOptions,
    kept_windows: Option<Arc<dyn Fn() -> Vec<u32> + Send + Sync>>,
    camera_watcher: Option<Box<dyn cm_camera::CameraWatcher>>,
    /// `recording_system` was used: audio recordings hold the OS power assertion (VOC-1).
    system_idle_sleep: bool,
}

impl DaemonBuilder {
    pub fn new(capturer: Capturer, library: Library) -> Self {
        DaemonBuilder {
            capturer,
            library,
            clipboard: Arc::new(NoClipboard),
            recognizer: cm_render::ocr::system_recognizer(),
            settings: Arc::new(MemorySettings::default()),
            external: Arc::new(NoExternalActions),
            trasher: Arc::new(SystemTrasher),
            free_space: Arc::new(cm_capture::disk::SystemFreeSpace),
            move_copier: Arc::new(crate::library_move::StdCopier),
            ai: AiServices::default(),
            destinations: None,
            cloud: crate::storage_cloud::CloudBackend::default(),
            publish_accounts: None,
            youtube: cm_publish::YouTubeConfig::default(),
            text_secrets: None,
            confirmer: Arc::new(DenyAll),
            confirm_timeout: DEFAULT_CONFIRM_TIMEOUT,
            clock: Arc::new(SystemClock),
            frame_ttl: DEFAULT_FRAME_TTL,
            jobs: None,
            recording: None,
            recording_options: EngineOptions::default(),
            kept_windows: None,
            camera_watcher: None,
            system_idle_sleep: false,
        }
    }

    /// Turns on the recording engine with these track sources. Without it the recording actions
    /// (`start_recording` and the rest) fail with `not_implemented` (or, for `start_recording`,
    /// go to [`DaemonBuilder::external_actions`]). Tests pass
    /// [`crate::testing::FakeSources`]; embedders with a UI use [`DaemonBuilder::recording_system`].
    pub fn recording_sources(mut self, sources: impl SourceFactory + 'static) -> Self {
        self.recording = Some(Arc::new(sources));
        self
    }

    /// The recording engine on the real screen, camera, microphone, system-audio and cursor
    /// sources of this OS.
    ///
    /// Only turn this on in a process that shows the recording indicator (REC-5, NFR-11): the
    /// engine publishes the facts the HUD needs (`recording.started`), but drawing the HUD, the
    /// area outline and the tray state is the host's job.
    pub fn recording_system(self) -> Self {
        let mut me = self.recording_sources(SystemSources);
        me.system_idle_sleep = true;
        if me.camera_watcher.is_none() {
            me.camera_watcher = Some(cm_camera::system_watcher());
        }
        me
    }

    /// Watches the cameras: the daemon starts the watcher when it is built and publishes
    /// `camera.connected` / `camera.disconnected` for every change, idle or recording. Stopped by
    /// [`Daemon::shutdown_recording`] (or when the daemon is dropped).
    /// [`DaemonBuilder::recording_system`] installs the OS watcher unless one is set; tests pass
    /// a [`cm_camera::FakeCameraWatcher`].
    pub fn camera_watcher(mut self, w: impl cm_camera::CameraWatcher + 'static) -> Self {
        self.camera_watcher = Some(Box::new(w));
        self
    }

    /// Tunables of the recording engine (tick period, timeouts, clocks). Defaults are the
    /// product values.
    pub fn recording_options(mut self, options: EngineOptions) -> Self {
        self.recording_options = options;
        self
    }

    /// The windows of this process that recordings and screenshots still capture although the
    /// process is excluded as a whole (REC-6): the host's library/editor window. The provider
    /// is queried when a recording is planned, again every second while it records, and on every
    /// display or region screenshot. Without one, every window of the own process is excluded.
    /// Recorder UI (HUD, countdown, bubble, toast, dialogs) must never be in the answer.
    pub fn kept_windows(mut self, provider: impl Fn() -> Vec<u32> + Send + Sync + 'static) -> Self {
        self.kept_windows = Some(Arc::new(provider));
        self
    }

    /// Gives the daemon a persistent job queue (EXP-4, STO-3). Default: none, and `get_job`,
    /// `list_jobs` and `cancel_job` fail with `not_implemented`. For the standard location
    /// (`<library>/.capturemann/jobs.sqlite`) pass [`crate::library_job_queue`]. The worker
    /// starts with [`Daemon::start_jobs`] (or on the first job action).
    pub fn jobs(mut self, queue: cm_jobs::JobQueue) -> Self {
        self.jobs = Some(queue);
        self
    }

    pub fn clipboard(mut self, c: impl Clipboard + 'static) -> Self {
        self.clipboard = Arc::new(c);
        self
    }

    /// Where the destinations of `list_destinations` and `upload` are stored (the app writes the
    /// file, the daemon only reads it). Without it no destination exists.
    pub fn destinations(mut self, store: cm_storage::DestinationStore) -> Self {
        self.destinations = Some(store);
        self
    }

    /// The token sources and HTTP client of the cloud destinations. The default has no
    /// connected account: cloud uploads fail with `auth_expired`.
    pub fn cloud_backend(mut self, cloud: crate::storage_cloud::CloudBackend) -> Self {
        self.cloud = cloud;
        self
    }

    /// Where the publish accounts of `list_publish_accounts` and `publish_video` are stored (the
    /// app writes the file, the daemon only reads it; the daily upload counter is written next to
    /// it). Without it no account exists and `publish_video` answers `not_implemented`. The
    /// token sources and HTTP client are the ones of [`DaemonBuilder::cloud_backend`].
    pub fn publish_accounts(mut self, store: cm_publish::PublishAccountStore) -> Self {
        self.publish_accounts = Some(store);
        self
    }

    /// Where the text model's API key is read from, by the name in `ai.text.key_name` (the
    /// system keychain when not set; tests pass a memory store).
    pub fn text_model_secrets(mut self, store: Arc<dyn cm_auth::SecretStore>) -> Self {
        self.text_secrets = Some(store);
        self
    }

    /// The YouTube provider's chunk size and retry timing (tests shorten the backoff).
    pub fn youtube_config(mut self, cfg: cm_publish::YouTubeConfig) -> Self {
        self.youtube = cfg;
        self
    }

    /// The text recognizer behind `suggest_redactions`. Default: the OS recognizer (Apple Vision
    /// on macOS; none on Windows yet, where the action answers `not_implemented`). Tests pass
    /// [`cm_render::ocr::ScriptedRecognizer`].
    pub fn text_recognizer(mut self, r: impl cm_render::ocr::TextRecognizer + 'static) -> Self {
        self.recognizer = Arc::new(r);
        self
    }

    pub fn settings(mut self, s: impl SettingsStore + 'static) -> Self {
        self.settings = Arc::new(s);
        self
    }

    pub fn external_actions(mut self, e: impl ExternalActions + 'static) -> Self {
        self.external = Arc::new(e);
        self
    }

    /// Where `delete_capture` sends bundle folders. Default: the system Trash
    /// ([`SystemTrasher`]); tests pass [`crate::testing::FakeTrasher`].
    pub fn trasher(mut self, t: impl Trasher + 'static) -> Self {
        self.trasher = Arc::new(t);
        self
    }

    /// Free-space reading of the library move's space check. Default: the real volume; tests pass
    /// [`cm_capture::disk::FakeFreeSpace`].
    pub fn free_space(mut self, f: impl cm_capture::disk::FreeSpace + 'static) -> Self {
        self.free_space = Arc::new(f);
        self
    }

    /// How the library move copies files (tests break it on purpose).
    pub fn move_copier(mut self, c: impl crate::library_move::FileCopier + 'static) -> Self {
        self.move_copier = Arc::new(c);
        self
    }

    /// The engines behind the local-AI actions (speech models, transcription, captions,
    /// suggestions). Default: none; every AI action then fails with `not_implemented` and the
    /// Confirm-tier ones are refused before a dialog opens. Register the job executors with
    /// [`Daemon::register_ai_handlers`].
    pub fn ai(mut self, services: AiServices) -> Self {
        self.ai = services;
        self
    }

    pub fn confirmer(mut self, c: impl Confirmer + 'static) -> Self {
        self.confirmer = Arc::new(c);
        self
    }

    /// Default 5 minutes (AGT-2.AC3).
    pub fn confirm_timeout(mut self, t: Duration) -> Self {
        self.confirm_timeout = t;
        self
    }

    /// How long a frozen frame (`freeze_display`) is kept without use; default 2 minutes.
    pub fn frame_ttl(mut self, t: Duration) -> Self {
        self.frame_ttl = t;
        self
    }

    pub fn clock(mut self, c: impl Clock + 'static) -> Self {
        self.clock = Arc::new(c);
        self
    }

    pub fn build(self) -> Daemon {
        let events = EventBus::new(self.clock.clone());
        let mut recording_options = self.recording_options.clone();
        if self.system_idle_sleep && recording_options.idle_sleep.is_none() {
            // The one place the real power assertion is wired (VOC-1).
            recording_options.idle_sleep = Some(Arc::new(crate::recording::SystemIdleSleep));
        }
        let mut capturer = self.capturer;
        if let Some(p) = &self.kept_windows {
            if recording_options.kept_windows.is_none() {
                recording_options.kept_windows = Some(p.clone());
            }
            let p = p.clone();
            capturer = capturer.with_kept_windows(cm_capture::WindowProvider::from_arc(p));
        }
        let capturer = Arc::new(capturer);
        let library = Arc::new(Mutex::new(self.library));
        let recording = self.recording.map(|factory| {
            RecordingEngine::new(
                EngineDeps {
                    events: events.clone(),
                    library: library.clone(),
                    capturer: capturer.clone(),
                    settings: self.settings.clone(),
                    jobs: self.jobs.clone(),
                    destinations: self.destinations.clone(),
                    clock: self.clock.clone(),
                },
                factory,
                recording_options.clone(),
            )
        });
        let camera_watch = self.camera_watcher.map(|mut w| {
            let bus = events.clone();
            let sink: cm_camera::ChangeSink = Arc::new(move |c| {
                bus.publish(
                    cm_camera::change_event_name(&c),
                    cm_camera::change_payload(&c),
                );
            });
            if let Err(e) = w.start(sink) {
                // No camera events is not fatal: the Record sheet still lists cameras on open.
                tracing_warn(&e);
            }
            w
        });
        Daemon {
            inner: Arc::new(Inner {
                capturer,
                library,
                clipboard: self.clipboard,
                recognizer: self.recognizer,
                settings: self.settings,
                external: self.external,
                trasher: self.trasher,
                free_space: self.free_space,
                move_copier: self.move_copier,
                clock: self.clock,
                ai: self.ai,
                destinations: self.destinations,
                cloud: self.cloud,
                publish_accounts: self.publish_accounts,
                youtube: self.youtube,
                text_model: crate::text_model_ops::TextModelState::new(self.text_secrets),
                gate: ConfirmationGate::new(self.confirmer, self.confirm_timeout, events.clone()),
                events,
                policies: ClientPolicies::default(),
                activity: ActivityLog::default(),
                idempotency: Mutex::new(crate::idem::ResultCache::default()),
                request_locks: LockMap::default(),
                capture_locks: Arc::new(LockMap::default()),
                delayed: DelayedCaptures::default(),
                frames: Arc::new(FrameCache::new(self.frame_ttl)),
                jobs: self.jobs.map(JobsRuntime::new),
                recording,
                next_session: AtomicU64::new(1),
                confirm_timeout: self.confirm_timeout,
                watcher: Mutex::new(None),
                camera_watch: Mutex::new(camera_watch),
            }),
        }
    }
}

fn tracing_warn(e: &CmError) {
    log::warn!("camera watcher not started: {e}");
}

pub(crate) fn humanize(d: Duration) -> String {
    let s = d.as_secs();
    if s >= 60 && s.is_multiple_of(60) {
        let m = s / 60;
        format!("{m} minute{}", if m == 1 { "" } else { "s" })
    } else {
        format!("{s} s")
    }
}

impl Daemon {
    pub fn builder(capturer: Capturer, library: Library) -> DaemonBuilder {
        DaemonBuilder::new(capturer, library)
    }

    /// The exposed actions with their tiers (what `actions.list` returns, plus availability).
    pub fn actions(&self) -> Vec<(ActionSpec, bool)> {
        ACTIONS
            .iter()
            .map(|s| {
                let available = self.inner.supports(s);
                (*s, available)
            })
            .collect()
    }

    /// The tier an **agent's** call to `tool` is gated by right now: the table's tier changed by
    /// the live `agents.tool_policy` setting (a tightened Free tool is Confirm, `start_recording`
    /// set to Allow is Free). `Actor::User` is never asked, so its tier is the table's. `None`
    /// for a name that is not an exposed action. Read from the settings on every call, so a
    /// change applies to the next call and a dialog that is already open is not affected.
    pub fn effective_tier(&self, tool: &str, actor: &Actor) -> Option<Tier> {
        let spec = crate::action::spec_for(tool)?;
        Some(match actor {
            Actor::User => spec.tier,
            Actor::Agent(_) => self.inner.effective_tier(spec),
        })
    }

    /// [`Daemon::actions`] with the effective tier an agent's call has (`actions.list`).
    pub fn actions_effective(&self) -> Vec<(ActionSpec, bool, Tier)> {
        let entries = self.inner.tool_entries();
        ACTIONS
            .iter()
            .map(|s| {
                (
                    *s,
                    self.inner.supports(s),
                    crate::tool_policy::effective_tier(s, &entries),
                )
            })
            .collect()
    }

    /// Per-client allow-list (Settings -> Agents).
    pub fn policies(&self) -> &ClientPolicies {
        &self.inner.policies
    }

    /// The Agent activity log, newest last, last 30 days (AGT-2.AC6).
    pub fn activity(&self) -> Vec<ActivityEntry> {
        self.inner.activity.entries(self.inner.clock.now())
    }

    /// Subscribes to the event stream.
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.inner.events.subscribe()
    }

    /// The bus itself, for layers that publish their own events (recording, jobs).
    pub fn event_bus(&self) -> &EventBus {
        &self.inner.events
    }

    /// Forwards a job-queue event source onto the bus as `job.progress` / `job.done` /
    /// `job.failed` (the `cm-jobs` extension point). The task ends with the source.
    pub fn bridge_job_events(
        &self,
        mut source: impl JobEvents + 'static,
    ) -> tokio::task::JoinHandle<()> {
        let bus = self.inner.events.clone();
        tokio::spawn(async move {
            while let Some(ev) = source.next_event().await {
                events::publish_job_event(&bus, ev);
            }
        })
    }

    /// The job queue, if the daemon has one (to enqueue work from engines in this process).
    pub fn job_queue(&self) -> Option<&cm_jobs::JobQueue> {
        self.inner.jobs.as_ref().map(|j| &j.queue)
    }

    /// Plugs in the executor of a job type (export, upload, transcribe, publish, ...). Jobs of
    /// a type without a handler stay queued. Errors with `not_implemented` without a queue.
    pub fn register_job_handler(
        &self,
        job_type: impl Into<String>,
        handler: Arc<dyn cm_jobs::JobHandler>,
    ) -> Result<(), DaemonError> {
        match self.job_queue() {
            Some(q) => {
                q.register_handler(job_type, handler);
                Ok(())
            }
            None => Err(DaemonError::NotImplemented(
                crate::jobs::NO_QUEUE.to_owned(),
            )),
        }
    }

    /// Recovers interrupted jobs (`running` -> `queued`, with their checkpoint), bridges job
    /// events onto the bus and starts the worker. Needs a tokio runtime. Idempotent; a no-op
    /// without a queue. Call it once at startup, after registering handlers and applying any
    /// holds that should be active (an ongoing recording).
    pub fn start_jobs(&self) -> Result<(), CmError> {
        match &self.inner.jobs {
            Some(j) => j.start(&self.inner),
            None => Ok(()),
        }
    }

    /// Starts the library watcher (see [`crate::watcher`]) with the default timing. Idempotent.
    /// Needs a tokio runtime. The headless daemon and the app call it at startup.
    pub fn start_library_watcher(&self) -> Result<(), CmError> {
        self.start_library_watcher_with(crate::watcher::WatchOptions::default())
    }

    /// [`Daemon::start_library_watcher`] with explicit timing (tests).
    pub fn start_library_watcher_with(
        &self,
        opts: crate::watcher::WatchOptions,
    ) -> Result<(), CmError> {
        if tokio::runtime::Handle::try_current().is_err() {
            return Err(CmError::internal(
                "the library watcher needs a tokio runtime",
            ));
        }
        let mut slot = self
            .inner
            .watcher
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if slot.is_none() {
            *slot = Some(crate::watcher::spawn(
                self.inner.library.clone(),
                self.inner.events.clone(),
                opts,
            ));
        }
        Ok(())
    }

    /// Stops the library watcher (it also stops when the last clone of the daemon is dropped).
    pub fn stop_library_watcher(&self) {
        self.inner
            .watcher
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
    }

    /// Stops the worker: running handlers checkpoint and their jobs return to `queued`, to
    /// resume on the next start (NFR-8). Dropping the last clone of the daemon does this in
    /// the background.
    pub async fn shutdown_jobs(&self) {
        self.shutdown_jobs_within(crate::jobs::DEFAULT_JOBS_SHUTDOWN_TIMEOUT)
            .await;
    }

    /// [`Daemon::shutdown_jobs`] with an explicit bound on the wait for running handlers.
    pub async fn shutdown_jobs_within(&self, limit: Duration) {
        if let Some(j) = &self.inner.jobs {
            j.shutdown(limit).await;
        }
        // The built-in text engine's helper ends with the app (it also ends by itself when this
        // process is gone, see `cm_ai::text_model::engine`).
        let engine = self.inner.ai.text_engine.clone();
        let _ = blocking(move || {
            engine.stop();
            Ok(())
        })
        .await;
    }

    /// Holds a job type: running jobs of it pause at a checkpoint and queued ones wait
    /// (recording pauses exports and uploads, EXP-4.AC2, STO-3.AC3). Returns whether a queue
    /// exists. Not an action: only the recording engine calls this.
    pub fn hold_job_type(&self, job_type: &str) -> bool {
        self.job_queue().map(|q| q.pause_type(job_type)).is_some()
    }

    /// Releases [`Daemon::hold_job_type`]; held jobs resume from their checkpoint.
    pub fn resume_job_type(&self, job_type: &str) -> bool {
        self.job_queue().map(|q| q.resume_type(job_type)).is_some()
    }

    /// Holds every job type.
    pub fn hold_all_jobs(&self) -> bool {
        self.job_queue().map(|q| q.pause_all()).is_some()
    }

    /// Releases [`Daemon::hold_all_jobs`].
    pub fn resume_all_jobs(&self) -> bool {
        self.job_queue().map(|q| q.resume_all()).is_some()
    }

    /// An account was connected again: uploads that paused with `auth_expired` continue from their
    /// checkpoint (STO-2.AC5). Returns how many were resumed; 0 without a queue. Not an action:
    /// only the app, which runs the connect flow, calls this.
    pub fn resume_auth_expired_jobs(&self) -> usize {
        self.job_queue()
            .and_then(|q| q.resume_auth_expired().ok())
            .unwrap_or(0)
    }

    /// Whether a job type is currently held.
    pub fn is_job_type_held(&self, job_type: &str) -> bool {
        self.job_queue().is_some_and(|q| q.is_type_held(job_type))
    }

    /// Cancels the delayed `capture_screenshot` that was sent with this `request_id`, if it is
    /// still waiting out its delay (CAP-5.AC2: Esc). The waiting call then fails with
    /// `invalid_input` ("capture was cancelled") and nothing is stored. Returns whether one was
    /// cancelled.
    pub fn cancel_delayed_capture(&self, request_id: &str) -> bool {
        self.inner
            .delayed
            .cancel_matching(|r| r.as_deref() == Some(request_id))
            > 0
    }

    /// Cancels every delayed capture still waiting (the picker's Esc); returns how many.
    pub fn cancel_all_delayed_captures(&self) -> usize {
        self.inner.delayed.cancel_matching(|_| true)
    }

    /// A fresh connection id for [`ActionCall::session`].
    pub fn new_session(&self) -> u64 {
        self.inner.next_session.fetch_add(1, Ordering::SeqCst)
    }

    /// The connection closed: forgets its "allow for this session" approvals.
    pub fn end_session(&self, session: u64) {
        self.inner.gate.end_session(session);
    }

    /// Parses `method` + `params` and runs it. Unknown and Never-tier names are
    /// `MethodNotFound` (AGT-2.AC5).
    pub async fn execute_method(
        &self,
        actor: Actor,
        session: u64,
        method: &str,
        params: &Value,
    ) -> Result<Value, DaemonError> {
        if crate::action::spec_for(method).is_none() {
            return Err(DaemonError::MethodNotFound(format!(
                "unknown method `{method}`"
            )));
        }
        let action = Action::parse(method, params)?;
        self.execute(ActionCall {
            actor,
            session,
            action,
        })
        .await
    }

    fn log(&self, client: &str, action: &Action, summary: String, decision: ActivityDecision) {
        self.log_with(client, action, summary, decision, None);
    }

    fn log_with(
        &self,
        client: &str,
        action: &Action,
        summary: String,
        decision: ActivityDecision,
        approval: Option<Approval>,
    ) {
        self.inner.activity.record(ActivityEntry {
            at: self.inner.clock.now(),
            client: client.to_owned(),
            tool: action.method().to_owned(),
            summary,
            decision,
            approval,
        });
    }

    /// Recovers recordings interrupted by a crash or power loss (REC-8): truncates every track to
    /// its last complete fragment, builds the manifest, indexes the bundle and removes the lock.
    /// Emits `recording.recovered { capture_id, lost_ms }` for each one and returns the summary
    /// (the app shows "Recovered recording from `<time>` (lost up to 2 s)"). Call it once at
    /// startup, before serving clients. Blocking file work; a recording of this very process is
    /// left alone. Works without a recording engine.
    pub fn recover_recordings(&self) -> Result<RecoveredRecordings, DaemonError> {
        recover_recordings(&self.inner.library, &self.inner.events).map_err(DaemonError::from)
    }

    /// Stops a running recording and finalizes its bundle, waiting a bounded time (the
    /// engine's `shutdown_timeout`, 10 s by default). A countdown is cancelled. Call it when the
    /// app or the headless daemon quits, before [`Daemon::shutdown_jobs`]. A no-op without an
    /// engine or a recording.
    pub async fn shutdown_recording(&self) {
        self.stop_camera_watch();
        if let Some(e) = &self.inner.recording {
            e.shutdown().await;
        }
    }

    /// Stops the camera watcher (no more `camera.*` events). Idempotent.
    pub fn stop_camera_watch(&self) {
        let w = self
            .inner
            .camera_watch
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(mut w) = w {
            w.stop();
        }
    }

    /// Whether the 60 fps self-test (`run_fps_selftest`) is running. It excludes a recording and
    /// is not counted by [`Daemon::recording_active`].
    pub fn selftest_active(&self) -> bool {
        self.inner
            .recording
            .as_ref()
            .is_some_and(RecordingEngine::is_selftest_active)
    }

    /// Whether a recording is in progress (any phase, countdown included).
    pub fn recording_active(&self) -> bool {
        self.inner
            .recording
            .as_ref()
            .is_some_and(RecordingEngine::is_active)
    }

    /// Runs one action: exactly one undo step for editing actions, one result, events.
    pub async fn execute(&self, call: ActionCall) -> Result<Value, DaemonError> {
        let ActionCall {
            actor,
            session,
            action,
        } = call;
        // Frozen frames idle past their TTL go first, whoever calls.
        self.inner.frames.evict_expired(self.inner.clock.now());
        let spec = action.spec();
        let agent = match &actor {
            Actor::Agent(name) => Some(name.clone()),
            Actor::User => None,
        };

        // 1. Policy.
        if let Some(name) = &agent {
            if let Some(reason) = self.inner.policies.refusal(name, &spec) {
                let (decision, message) = if reason != "client_read_only" {
                    (
                        ActivityDecision::ClientBlocked,
                        format!("Client `{name}` is blocked"),
                    )
                } else {
                    (
                        ActivityDecision::ClientReadOnly,
                        format!("Client `{name}` is read-only; `{}` was refused", spec.name),
                    )
                };
                self.log(name, &action, action.summary(), decision);
                return Err(DaemonError::Declined { reason, message });
            }
        }

        // R2-11: "Space skips the countdown" is the user's own control. An agent approved with a
        // 3 s countdown must not be able to shorten what the user saw in the dialog.
        if let (Some(name), Action::SkipCountdown(_)) = (&agent, &action) {
            self.log(name, &action, action.summary(), ActivityDecision::Declined);
            return Err(DaemonError::Declined {
                reason: "first_party_only",
                message: "`skip_countdown` is the user's control (Space in the countdown); an agent cannot shorten the countdown the user approved".to_owned(),
            });
        }

        // 2. Idempotency. The lock makes concurrent repeats wait for the first call.
        let key = action.request_id().map(|rid| format!("{actor}\u{0}{rid}"));
        let _request_guard = match &key {
            Some(k) => Some(self.inner.request_locks.lock(k).await),
            None => None,
        };
        if let Some(k) = &key {
            let hit = self
                .inner
                .idempotency
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .get(k, self.inner.clock.now());
            if let Some((method, value)) = hit {
                if method != spec.name {
                    return Err(CmError::invalid_field(
                        "request_id",
                        format!("already used for `{method}`"),
                    )
                    .into());
                }
                return Ok(value);
            }
        }

        // 3. Confirmation. For the recording engine this is also where the request is planned
        // (targets, size, permissions, device names): a bad request, a missing permission or a
        // recording already running fails here, before any dialog opens, and the dialog shows
        // exactly what the plan says will happen (AGT-2).
        let engine = self.inner.recording.as_ref();
        let mut prepared: Option<(PreparedStart, tokio::time::Instant)> = None;
        let mut prepared_test: Option<(PreparedSelftest, tokio::time::Instant)> = None;
        // R2-2: the recording a discard dialog described is the one a discard deletes.
        let mut approved_discard: Option<String> = None;
        // `publish_video` is planned like a recording: validated, and the dialog built from the
        // same plan that runs after the approval.
        let mut prepared_publish: Option<crate::publish_ops::PreparedPublish> = None;
        // `delete_capture` is planned too: with `also_delete_remote` the dialog lists the remote
        // copies, and exactly that list is deleted.
        let mut prepared_delete: Option<crate::library_ops::DeletePlan> = None;
        // `move_library` is checked before its dialog (every precondition), and the dialog says
        // what the check found.
        let mut prepared_move: Option<crate::library_move::MovePrepared> = None;
        // AGT-2.AC9: the tier this call is gated by. The table's tier for the user; for an
        // agent, changed by the live per-tool permissions (read now, so a setting changed while
        // this call waits behind another dialog applies to the next call, not to this one).
        let tier = if agent.is_some() {
            self.inner.effective_tier(&spec)
        } else {
            spec.tier
        };
        if let (Some(name), Tier::Confirm, Tier::Free) = (&agent, tier, spec.tier) {
            // A tightened Free tool: the generic dialog, built from the call's own parameters.
            if !self.inner.supports(&spec) {
                return Err(DaemonError::NotImplemented(format!(
                    "`{}` is not implemented yet",
                    spec.name
                )));
            }
            let (summary, details, params) =
                crate::tool_policy::ask_first_dialog(name, &spec, &action.details());
            let outcome = self
                .inner
                .gate
                .confirm(
                    name,
                    session,
                    &spec,
                    crate::tool_policy::KIND_ASK_FIRST,
                    summary.clone(),
                    details,
                    params,
                )
                .await;
            let (decision, err) = outcome.resolve(self.inner.confirm_timeout);
            self.log_with(
                name,
                &action,
                summary,
                decision,
                matches!(
                    decision,
                    ActivityDecision::Approved | ActivityDecision::ApprovedForSession
                )
                .then_some(Approval::User),
            );
            if let Some(e) = err {
                return Err(match e {
                    DaemonError::Cm(CmError::UserDeclined { message }) => DaemonError::Declined {
                        reason: crate::tool_policy::DECLINED_REASON,
                        message,
                    },
                    other => other,
                });
            }
        } else if let (Some(name), Tier::Free, Tier::Confirm) = (&agent, tier, spec.tier) {
            // `start_recording` set to Allow: no dialog. The engine plans and starts it below
            // exactly as after an approval (HUD, tray indicator and the agent toast included);
            // the log says the user's setting approved it.
            if !self.inner.supports(&spec) {
                return Err(DaemonError::NotImplemented(format!(
                    "`{}` is not implemented yet",
                    spec.name
                )));
            }
            self.log_with(
                name,
                &action,
                action.summary(),
                ActivityDecision::Approved,
                Some(Approval::Policy),
            );
        } else if spec.tier == Tier::Confirm {
            if !self.inner.supports(&spec) {
                return Err(DaemonError::NotImplemented(format!(
                    "`{}` is not implemented yet",
                    spec.name
                )));
            }
            let (summary, details, params) = match (engine, &action) {
                (Some(e), Action::StartRecording(p)) => {
                    let pr = e.prepare_start(&actor, p).await?;
                    let sd = (pr.summary.clone(), pr.details.clone(), pr.params.clone());
                    prepared = Some((pr, tokio::time::Instant::now()));
                    sd
                }
                (Some(e), Action::RunFpsSelftest(p)) => {
                    let pr = e.prepare_selftest(&actor, p).await?;
                    let sd = (pr.summary.clone(), pr.details.clone(), pr.params.clone());
                    prepared_test = Some((pr, tokio::time::Instant::now()));
                    sd
                }
                (Some(e), Action::RestartRecording(_)) => {
                    let pr = e.prepare_restart(&actor).await?;
                    let sd = (pr.summary.clone(), pr.details.clone(), pr.params.clone());
                    prepared = Some((pr, tokio::time::Instant::now()));
                    sd
                }
                (Some(e), Action::DiscardRecording(p)) => {
                    let (id, summary, details, params) =
                        e.describe_discard_pinned(p.recording_id.as_deref())?;
                    approved_discard = Some(id);
                    (summary, details, params)
                }
                (_, Action::DeleteCapture(p))
                    if !self.inner.external.supports("delete_capture") =>
                {
                    let plan = self.inner.plan_delete(p).await?;
                    let sd = (
                        plan.summary.clone(),
                        plan.details.clone(),
                        plan.params.clone(),
                    );
                    prepared_delete = Some(plan);
                    sd
                }
                (_, Action::PublishVideo(p)) if !self.inner.external.supports("publish_video") => {
                    let plan = self.inner.plan_publish(p).await?;
                    let sd = (
                        plan.summary.clone(),
                        plan.details.clone(),
                        plan.params.clone(),
                    );
                    prepared_publish = Some(plan);
                    sd
                }
                (_, Action::MoveLibrary(p)) => {
                    let pr = self.inner.plan_move_library(p).await?;
                    let sd = (pr.summary.clone(), pr.details.clone(), pr.params.clone());
                    prepared_move = Some(pr);
                    sd
                }
                // The dialog is built from the live model list: name, size, where it goes.
                (_, Action::Ai(p)) => self.inner.plan_ai(p).await?,
                _ => (action.summary(), action.details(), action.confirm_params()),
            };
            if let Some(name) = &agent {
                let outcome = self
                    .inner
                    .gate
                    .confirm(
                        name,
                        session,
                        &spec,
                        spec.name,
                        summary.clone(),
                        details,
                        params,
                    )
                    .await;
                let (decision, err) = outcome.resolve(self.inner.confirm_timeout);
                self.log_with(
                    name,
                    &action,
                    summary,
                    decision,
                    matches!(
                        decision,
                        ActivityDecision::Approved | ActivityDecision::ApprovedForSession
                    )
                    .then_some(Approval::User),
                );
                if let Some(e) = err {
                    return Err(e);
                }
            }
        }

        // 4. Run exactly what the dialog showed (R2-1): the approved plan is never made again
        // from live settings another agent may have changed while the dialog was open. If the
        // world changed meanwhile (a display went away, a recording started), starting the
        // approved plan fails with a clear error instead of recording something the user did
        // not see.
        let prepared = prepared.map(|(p, _)| p);
        let prepared_test = prepared_test.map(|(p, _)| p);
        let action = match (action, approved_discard) {
            (Action::DiscardRecording(mut p), Some(id)) => {
                p.recording_id = Some(id);
                Action::DiscardRecording(p)
            }
            (a, _) => a,
        };
        let result = if let Some(pr) = prepared_move {
            self.inner.run_move_library(pr).await?
        } else {
            match (action, prepared_publish, prepared_delete) {
                (Action::PublishVideo(p), Some(plan), _) => {
                    self.inner.run_publish(&actor, plan, p.request_id).await?
                }
                (Action::DeleteCapture(p), _, Some(plan)) => {
                    self.inner.delete_capture(&actor, p, Some(plan)).await?
                }
                (action, _, _) => self.run(&actor, action, prepared, prepared_test).await?,
            }
        };
        if let Some(k) = key {
            let now = self.inner.clock.now();
            self.inner
                .idempotency
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .record(&k, now, (spec.name.to_owned(), result.clone()));
        }
        Ok(result)
    }

    async fn run(
        &self,
        actor: &Actor,
        action: Action,
        prepared: Option<PreparedStart>,
        prepared_test: Option<PreparedSelftest>,
    ) -> Result<Value, DaemonError> {
        let inner = &self.inner;
        let reason = if actor.is_agent() { "agent" } else { "user" };
        match action {
            Action::ListTargets => inner.list_targets(true).await,
            Action::ListDisplays => inner.list_targets(false).await,
            Action::CaptureScreenshot(p) => inner.capture_screenshot(actor, p).await,
            Action::CaptureFromFrame(p) => inner.capture_from_frame(actor, p).await,
            Action::FreezeDisplay(p) => inner.freeze_display(actor, p).await,
            Action::ReleaseFrame(p) => inner.release_frame(&p),
            Action::WindowThumbnail(p) => inner.window_thumbnail(p).await,
            Action::GetJob(p) => inner.get_job(p).await,
            Action::ListJobs(p) => inner.list_jobs(p).await,
            Action::CancelJob(p) => inner.cancel_job(p).await,
            Action::RetryJob(p) => inner.retry_job(p).await,
            Action::ExportVideo(p) => inner.export_video(actor, p).await,
            Action::ExportAudio(p) => inner.export_audio(actor, p).await,
            Action::Annotate(p) => inner.annotate(actor, p).await,
            Action::GetCapture(p) => inner.get_capture(p).await,
            Action::SuggestRedactions(p) => inner.suggest_redactions(p).await,
            Action::GetSettings(p) => inner.get_settings(p),
            Action::SetSettings(p) => inner.set_settings(actor, p).await,
            Action::Undo(p) => inner.undo_redo(actor, p, true).await,
            Action::Redo(p) => inner.undo_redo(actor, p, false).await,
            Action::Edit(p) => inner.edit_capture(actor, p).await,
            Action::Ai(p) => inner.ai_op(actor, p).await,
            Action::ListDestinations => inner.list_destinations().await,
            Action::ListPublishAccounts => inner.list_publish_accounts().await,
            Action::GetTextModel => inner.get_text_model().await,
            Action::TestTextModel => inner.test_text_model().await,
            Action::Upload(p) => inner.upload(actor, p).await,
            Action::CaptionOptions(p) => inner.set_caption_options(actor, p).await,
            Action::ListLibrary(p) => inner.list_library(p).await,
            Action::RenameCapture(p) => inner.rename_capture(actor, p).await,
            Action::RescanLibrary => inner.rescan_library().await,
            Action::MoveLibrary(p) => {
                let pr = inner.plan_move_library(&p).await?;
                inner.run_move_library(pr).await
            }
            Action::StartRecording(p) => match &inner.recording {
                Some(e) => {
                    let prep = match prepared {
                        Some(prep) => prep,
                        None => e.prepare_start(actor, &p).await?,
                    };
                    e.start(actor, prep).await
                }
                None => {
                    inner
                        .external
                        .execute(actor, &Action::StartRecording(p))
                        .await
                }
            },
            Action::RestartRecording(_) => {
                let e = inner.engine()?;
                let prep = match prepared {
                    Some(prep) => prep,
                    None => e.prepare_restart(actor).await?,
                };
                e.restart(actor, prep).await
            }
            Action::RunFpsSelftest(p) => {
                let e = inner.engine()?;
                let prep = match prepared_test {
                    Some(prep) => prep,
                    None => e.prepare_selftest(actor, &p).await?,
                };
                e.run_selftest(actor, prep).await
            }
            Action::PauseRecording(p) => inner.engine()?.pause(p.recording_id.as_deref()).await,
            Action::ResumeRecording(p) => inner.engine()?.resume(p.recording_id.as_deref()).await,
            Action::AddMarker(p) => {
                if p.capture_id.is_some() {
                    inner.add_marker_stored(actor, p).await
                } else {
                    inner
                        .engine()?
                        .add_marker(actor, p.recording_id.as_deref(), p.label.as_deref(), p.at)
                        .await
                }
            }
            Action::SetMicMuted(p) => {
                inner
                    .engine()?
                    .set_mic_muted(p.recording_id.as_deref(), p.muted)
                    .await
            }
            Action::SkipCountdown(p) => {
                inner
                    .engine()?
                    .skip_countdown(p.recording_id.as_deref())
                    .await
            }
            Action::StopRecording(p) => {
                inner
                    .engine()?
                    .stop(p.recording_id.as_deref(), reason)
                    .await
            }
            Action::DiscardRecording(p) => {
                inner
                    .engine()?
                    .discard(p.recording_id.as_deref(), reason)
                    .await
            }
            Action::GetRecordingStatus(p) => {
                inner.engine()?.status(p.recording_id.as_deref()).await
            }
            Action::ListDevices => match &inner.recording {
                Some(e) => e.devices().await,
                None => blocking(|| Ok(SystemSources.devices())).await,
            },
            Action::DeleteCapture(p) if !inner.external.supports("delete_capture") => {
                inner.delete_capture(actor, p, None).await
            }
            a @ (Action::PublishVideo(_) | Action::DeleteCapture(_)) => {
                inner.external.execute(actor, &a).await
            }
        }
    }
}

impl Inner {
    /// The entries of `agents.tool_policy` the rules allow (an unreadable store or a bad value
    /// means the defaults).
    fn tool_entries(&self) -> std::collections::BTreeMap<String, crate::tool_policy::ToolSetting> {
        match self.settings.all() {
            Ok(all) => crate::tool_policy::parse_lenient(all.get(crate::tool_policy::KEY)),
            Err(_) => Default::default(),
        }
    }

    /// The tier an agent's call to `spec` is gated by (see [`Daemon::effective_tier`]).
    pub(crate) fn effective_tier(&self, spec: &ActionSpec) -> Tier {
        crate::tool_policy::effective_tier(spec, &self.tool_entries())
    }

    fn engine(&self) -> Result<&RecordingEngine, DaemonError> {
        self.recording.as_ref().ok_or_else(|| {
            DaemonError::NotImplemented(
                "recording is not available: this daemon has no recording engine".to_owned(),
            )
        })
    }

    /// Whether this daemon can run `spec`: Confirm-tier actions need an engine or an external
    /// handler (the check happens before any dialog opens).
    pub(crate) fn supports(&self, spec: &ActionSpec) -> bool {
        let n = spec.name;
        // The local part of `delete_capture` is built in; an external engine, if it claims the
        // action, takes over (it then owns the remote copies too).
        if n == "delete_capture" || n == "move_library" {
            return true;
        }
        // The built-in YouTube publisher needs the accounts file; an external engine, if it
        // claims the action, takes over.
        if n == "publish_video" {
            return self.publish_accounts.is_some() || self.external.supports(n);
        }
        // AI actions need the AI engine of `DaemonBuilder::ai`, not an external handler.
        if crate::ai::is_ai_method(n) {
            return self.ai.supports(n);
        }
        if RecordingEngine::handles(n) {
            if self.recording.is_some() {
                return true;
            }
            // Without an engine only `start_recording` may still be served by an external one.
            return n == "start_recording" && self.external.supports(n);
        }
        spec.tier != Tier::Confirm || self.external.supports(n)
    }
}

/// Settings as a JSON object result.
pub(crate) fn settings_result(map: Map<String, Value>) -> Value {
    json!({ "settings": map })
}
