//! The desktop app's embedded daemon and the glue to it.
//!
//! The app hosts `cm_daemon::Daemon` in process, with the real capturer and a `cm-library`
//! library, and also serves the daemon's IPC endpoint so `cm-cli` and `cm-mcp` share the
//! same state. Every GUI action goes through the daemon as `Actor::User`; this module never
//! calls an engine crate directly (AGT-1). The only exception is reading a poster file whose
//! path the daemon returned.

use base64::Engine as _;
use cm_capture::Capturer;
use cm_core::{Actor, CmError};
use cm_daemon::{
    ClientPolicy, Clipboard, ConfirmationRequest, Confirmer, Daemon, Endpoint, IpcServer,
    SettingsStore,
};
use cm_i18n::tr;
use cm_library::Library;
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::clipboard::SystemClipboard;
use crate::library::{LibraryStatus, PageCollector};
use crate::overlay::OverlayLauncher;
use crate::save_folder::{self, FolderResult, SaveFolderConfig};
use crate::settings::{self, FileSettings};

/// Pages of this size are requested (the daemon's maximum).
const PAGE_LIMIT: u64 = 200;
/// Safety cap on how many captures the library view loads (client-side sort needs them all).
pub const LOAD_CAP: usize = 5_000;

/// Lets the daemon and the UI share one `FileSettings`.
#[derive(Clone)]
pub struct SharedSettings(pub Arc<FileSettings>);

impl SettingsStore for SharedSettings {
    fn all(&self) -> Result<Map<String, Value>, CmError> {
        let mut all = self.0.all()?;
        // The policies that restrict agents are not readable or writable by agents.
        all.remove(settings::KEY_CLIENT_POLICIES);
        all.remove(settings::KEY_DENY_UNKNOWN);
        Ok(all)
    }
    fn set(&self, values: &Map<String, Value>) -> Result<Map<String, Value>, CmError> {
        if values.contains_key(settings::KEY_CLIENT_POLICIES) {
            return Err(CmError::invalid_field(
                "values",
                format!(
                    "`{}` is not available to clients",
                    settings::KEY_CLIENT_POLICIES
                ),
            ));
        }
        let mut all = self.0.set(values)?;
        all.remove(settings::KEY_CLIENT_POLICIES);
        all.remove(settings::KEY_DENY_UNKNOWN);
        Ok(all)
    }
}

/// Which recording sources the embedded daemon runs on (REC-1).
#[derive(Clone)]
pub enum RecordingMode {
    /// No recording engine: `start_recording` is refused (tests that never record).
    Off,
    /// The real screen, camera, microphone, system audio and cursor sources. The app shows the
    /// indicator (HUD, brackets, tray), which is what makes this allowed (REC-5, NFR-11).
    System,
    /// Scripted fake tracks (`--demo-recording`): the real engine and events, no devices.
    Fake(cm_daemon::testing::FakeSources),
}

pub struct StartConfig {
    /// Configured library folder (LIB-2); `None` when it could not be determined.
    pub library_path: Option<PathBuf>,
    /// Internal-disk folder used while the configured one is unavailable (LIB-2.AC2).
    pub fallback_path: PathBuf,
    pub settings: Arc<FileSettings>,
    /// IPC endpoint to serve; `None` runs the daemon in process only.
    pub endpoint: Option<Endpoint>,
    pub overlay_binary: Option<PathBuf>,
    pub recording: RecordingMode,
}

/// Result of opening the library at startup.
#[derive(Debug)]
pub struct OpenedLibrary {
    pub library: Option<Library>,
    pub status: LibraryStatus,
    pub root: Option<PathBuf>,
}

/// LIB-2: open the configured library, else the fallback with an "unavailable" status.
pub fn open_library(
    configured: Option<&Path>,
    fallback: &Path,
    open: impl Fn(&Path) -> Result<Library, CmError>,
) -> OpenedLibrary {
    let reason = match configured {
        Some(p) => match open(p) {
            Ok(l) => {
                return OpenedLibrary {
                    library: Some(l),
                    status: LibraryStatus::Loading,
                    root: Some(p.to_owned()),
                }
            }
            Err(e) => e.to_string(),
        },
        None => tr!("no library folder could be determined"),
    };
    let path = configured
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let hint = configured
        .map(settings::unavailable_hint)
        .unwrap_or_else(|| tr!("your library folder"));
    match open(fallback) {
        Ok(l) => OpenedLibrary {
            library: Some(l),
            status: LibraryStatus::Unavailable {
                hint,
                path,
                reason,
                fallback_in_use: true,
            },
            root: Some(fallback.to_owned()),
        },
        Err(e) => OpenedLibrary {
            library: None,
            status: LibraryStatus::Unavailable {
                hint,
                path,
                reason: tr!(
                    "{reason}; the fallback folder failed too: {e}",
                    reason = reason,
                    e = e
                ),
                fallback_in_use: false,
            },
            root: None,
        },
    }
}

/// Loads `agents.client_policies` into the daemon; bad entries are dropped with one log line.
fn apply_stored_policies(daemon: &Daemon, file: &FileSettings) {
    let loaded =
        settings::parse_client_policies(file.snapshot().get(settings::KEY_CLIENT_POLICIES));
    if loaded.dropped > 0 {
        log::warn!(
            "ignored {} unusable entr{} in {}",
            loaded.dropped,
            if loaded.dropped == 1 { "y" } else { "ies" },
            settings::KEY_CLIENT_POLICIES
        );
    }
    for (name, policy) in loaded.policies {
        daemon.policies().set(&name, policy);
    }
    // Fail closed: when the policy store was unreadable and could not be restored, unknown
    // clients are blocked until the user reviews the list (Settings -> Agents).
    if file.deny_unknown() {
        daemon.policies().set_default(ClientPolicy::Blocked);
        if let Err(e) = file.set_deny_unknown(true) {
            log::warn!("cannot remember the deny-unknown-clients state: {e}");
        }
    }
}

/// Runs one display listing and one freeze (then releases the frame) so the first real
/// capture does not pay the one-time setup of the capture stack. Failures (no permission,
/// no display) are ignored: the real capture reports them.
pub async fn warm_up(daemon: &Daemon) {
    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
    let Ok(targets) = Backend::call(daemon, "list_targets", json!({ "windows": false })).await
    else {
        return;
    };
    let Some(id) = targets.pointer("/displays/0/id").and_then(Value::as_u64) else {
        return;
    };
    let frozen = Backend::call(
        daemon,
        "freeze_display",
        json!({ "display_id": id, "transport": "file" }),
    )
    .await;
    if let Some(frame) = frozen
        .ok()
        .and_then(|v| v.get("frame_id").and_then(Value::as_str).map(str::to_owned))
    {
        let _ = Backend::call(daemon, "release_frame", json!({ "frame_id": frame })).await;
    }
}

pub struct Backend {
    pub rt: tokio::runtime::Handle,
    pub daemon: Option<Daemon>,
    pub initial_status: LibraryStatus,
    pub library_root: Option<PathBuf>,
    pub settings: Arc<FileSettings>,
    pub overlay: OverlayLauncher,
    /// Set when the IPC endpoint could not be served (another daemon owns it).
    pub ipc_warning: Option<String>,
    pub endpoint: Option<Endpoint>,
    clipboard: Arc<dyn Clipboard>,
    /// Recordings a crash left behind that this start recovered (REC-8); the app tells the user.
    pub recovered: Vec<cm_daemon::RecoveredRecording>,
    /// The recording engine is on (the HUD can be shown).
    pub recording_enabled: bool,
}

impl Backend {
    /// Builds the daemon on `rt` and starts serving IPC. `capturer` and `clipboard` are
    /// injected so tests run without a display.
    pub fn start(
        rt: &tokio::runtime::Runtime,
        cfg: StartConfig,
        capturer: Capturer,
        clipboard: Arc<dyn Clipboard>,
        confirmer: impl Confirmer + 'static,
    ) -> Backend {
        let opened = open_library(cfg.library_path.as_deref(), &cfg.fallback_path, |p| {
            Library::open_or_create(p)
        });
        let mut ipc_warning = None;
        let mut recovered = Vec::new();
        let recording_enabled = !matches!(cfg.recording, RecordingMode::Off);
        let daemon = opened.library.map(|lib| {
            // Persistent jobs (EXP-4, STO-3): `<library>/.capturemann/jobs.sqlite`.
            let jobs = cm_daemon::library_job_queue(lib.root());
            let mut builder = Daemon::builder(capturer, lib)
                .clipboard(ArcClipboard(clipboard.clone()))
                .settings(SharedSettings(cfg.settings.clone()))
                .confirmer(confirmer)
                .ai(cm_daemon::ai_engine::real_services())
                .destinations(cm_storage::DestinationStore::new(
                    cm_daemon::config::config_dir(),
                ))
                // Cloud drives: tokens from the OS keychain, connected in Settings → Destinations
                // (the connect flow shares this token source's cache).
                .cloud_backend(cm_daemon::storage_cloud::CloudBackend::system())
                // Publish accounts (PUB-1, PUB-2): written by the connect flow, read here.
                .publish_accounts(cm_publish::PublishAccountStore::new(
                    cm_daemon::config::config_dir(),
                ));
            match jobs {
                Ok(queue) => builder = builder.jobs(queue),
                Err(e) => log::warn!("background jobs are unavailable: {e}"),
            }
            // REC-6.AC8: recordings and screenshots include the library window; every other
            // own window (HUD, bubble, toasts, ...) stays excluded by pid.
            builder = builder.kept_windows(crate::studio_window::kept_windows);
            builder = match &cfg.recording {
                RecordingMode::Off => builder,
                RecordingMode::System => builder.recording_system(),
                RecordingMode::Fake(sources) => builder.recording_sources(sources.clone()),
            };
            let daemon = builder.build();
            // The quick exporter (FFmpeg sidecar): one combined MP4 after every recording.
            if let Err(e) = daemon.register_export_handler() {
                log::warn!("export is unavailable: {e}");
            }
            // Uploads to the user's destinations (STO-1, STO-3).
            if let Err(e) = daemon.register_upload_handler() {
                log::warn!("upload is unavailable: {e}");
            }
            // Publishing to YouTube (PUB-2).
            if let Err(e) = daemon.register_publish_handler() {
                log::warn!("publishing is unavailable: {e}");
            }
            // Moving the library folder (LIB-2): the copy job.
            if let Err(e) = daemon.register_library_move_handler() {
                log::warn!("moving the library is unavailable: {e}");
            }
            // Local AI (M5): the job executors for model downloads and transcription. Nothing is
            // downloaded or loaded until the user asks; the automatic transcription after a
            // recording needs an installed model.
            if let Err(e) = daemon.register_ai_handlers() {
                log::warn!("local AI is unavailable: {e}");
            }
            {
                // Recovers interrupted jobs and starts the worker (needs the runtime).
                let _guard = rt.enter();
                if let Err(e) = daemon.start_jobs() {
                    log::warn!("cannot start the job worker: {e}");
                }
                daemon.start_ai_auto_transcribe();
                // LIB-1: bundles copied in, restored or deleted outside the app appear on
                // their own (`capture.created` / `capture.revised` / `capture.deleted`).
                if let Err(e) = daemon.start_library_watcher() {
                    log::warn!("the library watcher is unavailable: {e}");
                }
            }
            // REC-8: recover recordings a crash left behind, before any client is served.
            match daemon.recover_recordings() {
                Ok(r) => recovered = r.recovered,
                Err(e) => log::warn!("recording recovery failed: {e}"),
            }
            // AGT-2.AC4: stored client policies apply before any connection is served.
            apply_stored_policies(&daemon, &cfg.settings);
            if let Some(endpoint) = &cfg.endpoint {
                let bound = {
                    let _guard = rt.enter();
                    IpcServer::bind(endpoint)
                };
                match bound {
                    Ok(server) => {
                        let d = daemon.clone();
                        rt.spawn(async move {
                            let _ = server.run(d).await;
                        });
                    }
                    Err(e) => {
                        ipc_warning = Some(tr!(
                            "The CLI and MCP server can\u{2019}t connect to this app: {e}",
                            e = e
                        ));
                    }
                }
            }
            daemon
        });
        Backend {
            rt: rt.handle().clone(),
            daemon,
            initial_status: opened.status,
            library_root: opened.root,
            settings: cfg.settings,
            overlay: OverlayLauncher::new(
                cfg.overlay_binary,
                match &cfg.endpoint {
                    Some(Endpoint::Path(p)) => Some(p.clone()),
                    _ => None,
                },
            ),
            ipc_warning,
            endpoint: cfg.endpoint,
            clipboard,
            recovered,
            recording_enabled,
        }
    }

    /// Quit: stops a running recording and finalizes its bundle (bounded wait), then stops the job
    /// worker (REC-8, NFR-6). Safe to call with nothing running.
    pub async fn shutdown(daemon: &Daemon) {
        daemon.shutdown_recording().await;
        daemon.shutdown_jobs().await;
    }

    /// One daemon call as the user.
    pub async fn call(daemon: &Daemon, method: &str, params: Value) -> Result<Value, String> {
        daemon
            .execute_method(Actor::User, 0, method, &params)
            .await
            .map_err(|e| e.to_string())
    }

    /// Like [`Backend::call`], but keeps the shared error code (a cancelled countdown, a missing
    /// permission and a busy engine are told apart by it).
    pub async fn call_coded(
        daemon: &Daemon,
        method: &str,
        params: Value,
    ) -> Result<Value, CallError> {
        daemon
            .execute_method(Actor::User, 0, method, &params)
            .await
            .map_err(|e| CallError {
                code: e.code(),
                permission: permission_of(&e),
                message: e.to_string(),
                data: match &e {
                    cm_daemon::DaemonError::WithData { data, .. } => Some(data.clone()),
                    _ => None,
                },
            })
    }

    /// LIB-1: every capture matching `query`, `kind` and `status`, newest first (paged internally).
    pub async fn list_library(
        daemon: &Daemon,
        query: &str,
        kind: Option<&'static str>,
        status: Option<&'static str>,
    ) -> Result<PageCollector, String> {
        let mut collector = PageCollector::new(LOAD_CAP);
        let mut cursor: Option<String> = None;
        loop {
            let mut p = json!({ "limit": PAGE_LIMIT });
            if !query.trim().is_empty() {
                p["query"] = json!(query.trim());
            }
            if let Some(k) = kind {
                p["kind"] = json!(k);
            }
            if let Some(s) = status {
                p["status"] = json!(s);
            }
            if let Some(c) = &cursor {
                p["cursor"] = json!(c);
            }
            let page = Self::call(daemon, "list_library", p).await?;
            cursor = collector.push(&page)?;
            if cursor.is_none() {
                return Ok(collector);
            }
        }
    }

    /// Bundle folder of a capture (for its poster).
    pub async fn capture_dir(daemon: &Daemon, id: &str) -> Result<PathBuf, String> {
        let v = Self::call(
            daemon,
            "get_capture",
            json!({ "capture_id": id, "include_image": false }),
        )
        .await?;
        v.get("path")
            .and_then(Value::as_str)
            .map(PathBuf::from)
            .ok_or_else(|| "get_capture returned no path".to_owned())
    }

    /// Title and rendered image of a capture as PNG bytes (annotations included).
    pub async fn capture_png(daemon: &Daemon, id: &str) -> Result<(String, Vec<u8>), String> {
        let v = Self::call(
            daemon,
            "get_capture",
            json!({ "capture_id": id, "include_image": true }),
        )
        .await?;
        let data = v
            .pointer("/image/data")
            .and_then(Value::as_str)
            .ok_or_else(|| "get_capture returned no image".to_owned())?;
        let title = v
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or(id)
            .to_owned();
        base64::engine::general_purpose::STANDARD
            .decode(data)
            .map(|png| (title, png))
            .map_err(|e| format!("bad image data: {e}"))
    }

    /// CAP-4: after a capture finished, copy the rendered image to the clipboard when the
    /// setting is on (default on). A clipboard failure never loses the capture: it is
    /// already in the library.
    pub async fn after_capture(
        &self,
        capture_id: &str,
        copy_enabled: bool,
    ) -> Result<AfterCapture, String> {
        let daemon = self
            .daemon
            .clone()
            .ok_or_else(|| tr!("The library is not available"))?;
        let (title, png) = Self::capture_png(&daemon, capture_id).await?;
        let copy = self.copy_bytes(&png, copy_enabled).await;
        let folder = self.save_bytes_to_folder(&png).await;
        Ok(AfterCapture {
            capture_id: capture_id.to_owned(),
            title,
            copy,
            folder,
        })
    }

    async fn copy_bytes(&self, png: &[u8], enabled: bool) -> CopyResult {
        if !enabled {
            return CopyResult::Skipped;
        }
        let cb = self.clipboard.clone();
        let bytes = png.to_vec();
        match tokio::task::spawn_blocking(move || cb.copy_png(&bytes)).await {
            Ok(Ok(())) => CopyResult::Copied,
            Ok(Err(e)) => CopyResult::Failed(e.to_string()),
            Err(e) => CopyResult::Failed(e.to_string()),
        }
    }

    /// CAP-4.AC2 to AC4: with "Also save a PNG to a folder" on, writes the PNG there. A failure
    /// is a result to show, never an error: the capture is already in the library.
    async fn save_bytes_to_folder(&self, png: &[u8]) -> FolderResult {
        let cfg = SaveFolderConfig::from_settings(&self.settings.snapshot());
        if !cfg.enabled {
            return FolderResult::Skipped;
        }
        let bytes = png.to_vec();
        let folder = cfg
            .folder
            .as_ref()
            .map_or_else(String::new, |f| f.display().to_string());
        tokio::task::spawn_blocking(move || {
            save_folder::save_capture(&cfg, &bytes, save_folder::local_now())
        })
        .await
        .unwrap_or_else(|e| FolderResult::Failed {
            folder,
            reason: e.to_string(),
        })
    }

    /// CAP-4: the rendered capture saved to the folder again, for an annotation save (the editor
    /// calls this once per burst of edits).
    pub async fn save_capture_to_folder(&self, capture_id: &str) -> Result<FolderResult, String> {
        let daemon = self
            .daemon
            .clone()
            .ok_or_else(|| tr!("The library is not available"))?;
        if !SaveFolderConfig::from_settings(&self.settings.snapshot()).enabled {
            return Ok(FolderResult::Skipped);
        }
        let (_, png) = Self::capture_png(&daemon, capture_id).await?;
        Ok(self.save_bytes_to_folder(&png).await)
    }

    /// Copies a capture again (toast "Copy" button). Only the clipboard: the folder copy was
    /// made once, after the capture.
    pub async fn copy_capture(&self, capture_id: &str) -> Result<(), String> {
        let daemon = self
            .daemon
            .clone()
            .ok_or_else(|| tr!("The library is not available"))?;
        let (_, png) = Self::capture_png(&daemon, capture_id).await?;
        match self.copy_bytes(&png, true).await {
            CopyResult::Failed(e) => Err(e),
            CopyResult::Copied | CopyResult::Skipped => Ok(()),
        }
    }
}

/// What a library rescan found: how many bundles are indexed now and how many index rows were
/// dropped because their folder is gone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RescanSummary {
    pub indexed: usize,
    pub removed: usize,
}

impl Backend {
    /// R1-6: rebuilds the index from the bundle folders (copied-in bundles appear, deleted ones
    /// stop being ghost rows) with the daemon's internal `rescan_library` action, in process, on
    /// the daemon's own library handle under its library lock (bundles being recorded are
    /// skipped by the rescan itself).
    pub async fn rescan_library(daemon: &Daemon) -> Result<RescanSummary, String> {
        let r = Self::call(daemon, "rescan_library", json!({})).await?;
        let n = |k: &str| r.get(k).and_then(Value::as_u64).unwrap_or(0) as usize;
        Ok(RescanSummary {
            indexed: n("indexed"),
            removed: n("removed"),
        })
    }
}

/// A failed daemon call with its shared error code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallError {
    pub code: Option<cm_core::ErrorCode>,
    /// Which OS permission a `permission_missing` error names (`data.permission`).
    pub permission: Option<cm_core::Permission>,
    pub message: String,
    /// The error's code-specific `data` (`delete_capture` with a failed remote copy: `remote`).
    pub data: Option<Value>,
}

impl CallError {
    /// An error with a message and nothing else (a join failure, a missing daemon).
    pub fn message(message: impl Into<String>) -> CallError {
        CallError {
            code: None,
            permission: None,
            message: message.into(),
            data: None,
        }
    }
}

/// The permission a `permission_missing` error names.
fn permission_of(e: &cm_daemon::DaemonError) -> Option<cm_core::Permission> {
    match e {
        cm_daemon::DaemonError::Cm(CmError::PermissionMissing { permission, .. })
        | cm_daemon::DaemonError::WithData {
            error: CmError::PermissionMissing { permission, .. },
            ..
        } => Some(*permission),
        _ => None,
    }
}

/// The synthetic event [`forward_events`] sends when the bus overran this client and events were
/// lost: the UI re-reads the state it keeps (recording status, library) instead of trusting the
/// stream (R4-3).
pub const RESYNC_EVENT: &str = "app.resync";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CopyResult {
    Copied,
    Skipped,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AfterCapture {
    pub capture_id: String,
    pub title: String,
    pub copy: CopyResult,
    /// CAP-4: the "also save a PNG to a folder" step.
    pub folder: FolderResult,
}

/// `Clipboard` is object safe but the builder wants an owned `impl`.
struct ArcClipboard(Arc<dyn Clipboard>);

impl Clipboard for ArcClipboard {
    fn copy_png(&self, png: &[u8]) -> Result<(), CmError> {
        self.0.copy_png(png)
    }
}

/// The clipboard the app uses by default.
pub fn system_clipboard() -> Arc<dyn Clipboard> {
    Arc::new(SystemClipboard::default())
}

/// Forwards daemon events to the UI thread's channel (`capture.created` refreshes the
/// library, `confirmation.*` is informational).
pub fn forward_events(
    rt: &tokio::runtime::Handle,
    daemon: &Daemon,
) -> tokio::sync::mpsc::UnboundedReceiver<cm_daemon::events::Event> {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let mut sub = daemon.subscribe();
    rt.spawn(async move {
        loop {
            match sub.recv().await {
                Ok(ev) => {
                    if tx.send(ev).is_err() {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    let ev = cm_daemon::events::Event {
                        seq: 0,
                        name: RESYNC_EVENT.to_owned(),
                        at: time::OffsetDateTime::now_utc(),
                        data: Value::Null,
                    };
                    if tx.send(ev).is_err() {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
    rx
}

/// A one-off confirmation used by `--demo-confirm` to exercise the dialog by hand.
pub fn demo_confirmation() -> ConfirmationRequest {
    ConfirmationRequest {
        id: "conf_demo".to_owned(),
        client: "claude-desktop".to_owned(),
        tool: "start_recording".to_owned(),
        summary: "Start recording display 1 at 3024 x 1964, 30 fps, with the microphone on"
            .to_owned(),
        details: json!({"target": "Display 1", "size": "3024 x 1964", "fps": 30, "microphone": true, "camera": false}),
        kind: "start_recording".to_owned(),
        params: json!({}),
        allow_session: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cm_capture::fake::FakeBackend;
    use cm_daemon::testing::RecordingClipboard;
    use cm_daemon::DenyAll;

    fn fake_capturer() -> Capturer {
        Capturer::new(FakeBackend::new().with_display(1, 0, 0, 40, 30, 1.0)).with_own_pid(None)
    }

    struct Fixture {
        rt: tokio::runtime::Runtime,
        backend: Backend,
        clip: Arc<RecordingClipboard>,
        _dir: tempfile::TempDir,
    }

    fn fixture() -> Fixture {
        fixture_with(RecordingMode::Off)
    }

    fn fixture_with(recording: RecordingMode) -> Fixture {
        fixture_prepared(recording, |_| {})
    }

    /// Like [`fixture_with`], after `prepare` ran on the (still empty) config directory.
    fn fixture_prepared(
        recording: RecordingMode,
        prepare: impl FnOnce(&std::path::Path),
    ) -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        prepare(dir.path());
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        let clip = Arc::new(RecordingClipboard::default());
        let backend = Backend::start(
            &rt,
            StartConfig {
                library_path: Some(dir.path().join("lib")),
                fallback_path: dir.path().join("fallback"),
                settings: Arc::new(FileSettings::load(dir.path().join("settings.json"))),
                endpoint: Some(Endpoint::Path(dir.path().join("d.sock"))),
                overlay_binary: None,
                recording,
            },
            fake_capturer(),
            Arc::new(clip.clone()),
            DenyAll,
        );
        Fixture {
            rt,
            backend,
            clip,
            _dir: dir,
        }
    }

    /// LIB-1 + AGT-1: the GUI lists the library through the daemon as the user, and a
    /// capture made by another client over the socket shows up (shared state).
    #[test]
    fn lists_captures_made_over_ipc() {
        let f = fixture();
        let daemon = f.backend.daemon.clone().unwrap();
        assert!(f.backend.ipc_warning.is_none());
        let ep = f.backend.endpoint.clone().unwrap();
        f.rt.block_on(async {
            use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
            let stream = cm_daemon::connect(&ep).await.unwrap();
            let (r, mut w) = tokio::io::split(stream);
            let req = json!({"jsonrpc":"2.0","id":1,"method":"capture_screenshot",
                "params":{"target":{"display_id":1},"title":"From CLI"}});
            w.write_all(format!("{req}\n").as_bytes()).await.unwrap();
            let mut lines = BufReader::new(r).lines();
            let resp = lines.next_line().await.unwrap().unwrap();
            assert!(resp.contains("capture_id"), "{resp}");

            let got = Backend::list_library(&daemon, "", None, None)
                .await
                .unwrap();
            assert_eq!(got.items.len(), 1);
            assert_eq!(got.items[0].title, "From CLI");
            let none = Backend::list_library(&daemon, "zzzz", None, None)
                .await
                .unwrap();
            assert!(none.items.is_empty());
            let recs = Backend::list_library(&daemon, "", Some("recording"), None)
                .await
                .unwrap();
            assert!(recs.items.is_empty());
        });
    }

    /// CAP-4.AC1: after a capture the clipboard holds the rendered PNG.
    #[test]
    fn after_capture_copies_to_the_clipboard() {
        let f = fixture();
        let daemon = f.backend.daemon.clone().unwrap();
        f.rt.block_on(async {
            let v = Backend::call(
                &daemon,
                "capture_screenshot",
                json!({"target": {"display_id": 1}}),
            )
            .await
            .unwrap();
            let id = v["capture_id"].as_str().unwrap().to_owned();
            let done = f.backend.after_capture(&id, true).await.unwrap();
            assert_eq!(done.copy, CopyResult::Copied);
            let png = f.clip.last_png().unwrap();
            assert_eq!(&png[1..4], b"PNG");
            let skipped = f.backend.after_capture(&id, false).await.unwrap();
            assert_eq!(skipped.copy, CopyResult::Skipped);
            let dir = Backend::capture_dir(&daemon, &id).await.unwrap();
            assert!(dir.join("original.png").is_file());
            assert!(f.backend.after_capture("shot_missing", true).await.is_err());
        });
    }

    /// CAP-4.AC2 to AC4: with the folder save on, the PNG lands in the folder (a second capture
    /// keeps both); an unplugged folder fails with a reason while the capture stays in the
    /// library; with it off nothing is written.
    #[test]
    fn cap4_after_capture_saves_to_the_folder() {
        use crate::save_folder::FolderResult;
        let f = fixture();
        let daemon = f.backend.daemon.clone().unwrap();
        let out = tempfile::tempdir().unwrap();
        let set = |enabled: bool, folder: &std::path::Path| {
            let v = json!({
                settings::KEY_SAVE_FOLDER_ENABLED: enabled,
                settings::KEY_SAVE_FOLDER: folder.display().to_string(),
                settings::KEY_SAVE_PATTERN: "shot.png",
            });
            f.backend.settings.set(v.as_object().unwrap()).unwrap();
        };
        f.rt.block_on(async {
            let v = Backend::call(
                &daemon,
                "capture_screenshot",
                json!({"target": {"display_id": 1}}),
            )
            .await
            .unwrap();
            let id = v["capture_id"].as_str().unwrap().to_owned();
            // Off by default.
            let off = f.backend.after_capture(&id, false).await.unwrap();
            assert_eq!(off.folder, FolderResult::Skipped);
            assert_eq!(std::fs::read_dir(out.path()).unwrap().count(), 0);
            // On: two captures, two files.
            set(true, out.path());
            let a = f.backend.after_capture(&id, false).await.unwrap();
            let b = f.backend.after_capture(&id, false).await.unwrap();
            assert_eq!(a.folder, FolderResult::Saved(out.path().join("shot.png")));
            assert_eq!(
                b.folder,
                FolderResult::Saved(out.path().join("shot (2).png"))
            );
            let bytes = std::fs::read(out.path().join("shot.png")).unwrap();
            assert_eq!(&bytes[1..4], b"PNG");
            // Offline folder: a warning result, the capture is still there.
            let gone = out.path().join("no-such-drive");
            set(true, &gone);
            let c = f.backend.after_capture(&id, true).await.unwrap();
            assert_eq!(c.copy, CopyResult::Copied);
            assert_eq!(
                c.folder,
                FolderResult::Failed {
                    folder: gone.display().to_string(),
                    reason: "drive not available".into()
                }
            );
            assert!(Backend::capture_dir(&daemon, &id)
                .await
                .unwrap()
                .join("original.png")
                .is_file());
            // The toast's Copy button does not write another file.
            set(true, out.path());
            f.backend.copy_capture(&id).await.unwrap();
            assert_eq!(std::fs::read_dir(out.path()).unwrap().count(), 2);
        });
    }

    /// CAP-4 / R1-1: after an edit the toast's Copy (and the editor's copy) puts the RENDERED
    /// image on the clipboard, annotations applied, never the untouched original.
    #[test]
    fn copy_after_an_edit_is_the_rendered_image_not_the_original() {
        let f = fixture();
        let daemon = f.backend.daemon.clone().unwrap();
        f.rt.block_on(async {
            let shot = Backend::call(
                &daemon,
                "capture_screenshot",
                json!({"target": {"display_id": 1}}),
            )
            .await
            .unwrap();
            let id = shot["capture_id"].as_str().unwrap().to_owned();
            let dir = Backend::capture_dir(&daemon, &id).await.unwrap();
            let original = std::fs::read(dir.join("original.png")).unwrap();
            // A filled rectangle changes pixels the original does not have.
            Backend::call(
                &daemon,
                "annotate",
                json!({"capture_id": id, "ops": [
                    {"op": "rect", "rect": [2, 2, 20, 15], "color": "#2E8BFF", "width": 4, "fill": "#2E8BFF"}
                ]}),
            )
            .await
            .unwrap();
            f.backend.copy_capture(&id).await.unwrap();
            let copied = f.clip.last_png().unwrap();
            let decode = |b: &[u8]| image::load_from_memory(b).unwrap().to_rgba8();
            assert_ne!(
                decode(&copied).as_raw(),
                decode(&original).as_raw(),
                "the clipboard must hold the annotated image"
            );
            // And it is exactly what the editor shows: the daemon's render.
            let (_, rendered) = Backend::capture_png(&daemon, &id).await.unwrap();
            assert_eq!(decode(&copied).as_raw(), decode(&rendered).as_raw());
        });
    }

    /// REC-5, AGT-2, NFR-11: the user starts a recording from the app without a dialog (in
    /// process, `Actor::User`), an agent over the socket still hits the gate (the app has no UI in
    /// this test, so the gate declines), and the engine is off unless the app turns it on.
    #[test]
    fn the_user_records_without_a_dialog_and_agents_do_not() {
        use cm_daemon::testing::FakeSources;
        let f = fixture_with(RecordingMode::Fake(FakeSources::new()));
        assert!(f.backend.recording_enabled);
        let daemon = f.backend.daemon.clone().unwrap();
        f.rt.block_on(async {
            let started = Backend::call(
                &daemon,
                "start_recording",
                json!({"target": {"display_id": 1}, "countdown_s": 0}),
            )
            .await
            .unwrap();
            assert_eq!(started["state"], "recording");
            assert_eq!(started["indicator"]["started_by"]["kind"], "user");
            assert!(daemon.recording_active());
            let status = Backend::call(&daemon, "get_recording_status", json!({}))
                .await
                .unwrap();
            assert_eq!(status["state"], "recording");
            // Stopping through the app's quit path finalizes the bundle.
            Backend::shutdown(&daemon).await;
            assert!(!daemon.recording_active());
            let recs = Backend::list_library(&daemon, "", Some("recording"), None)
                .await
                .unwrap();
            assert_eq!(recs.items.len(), 1);
        });
        let off = fixture();
        assert!(!off.backend.recording_enabled);
        let d = off.backend.daemon.clone().unwrap();
        off.rt.block_on(async {
            let e = Backend::call(&d, "start_recording", json!({"target": {"display_id": 1}}))
                .await
                .unwrap_err();
            assert!(!e.is_empty());
        });
    }

    /// LIB-2.AC2: an unopenable library falls back to the internal folder and says so.
    #[test]
    fn unavailable_library_uses_the_fallback() {
        let dir = tempfile::tempdir().unwrap();
        let fallback = dir.path().join("fb");
        let blocked = dir.path().join("file");
        std::fs::write(&blocked, "x").unwrap();
        let opened = open_library(Some(&blocked.join("sub")), &fallback, |p| {
            Library::open_or_create(p)
        });
        assert!(opened.library.is_some());
        assert_eq!(opened.root.as_deref(), Some(fallback.as_path()));
        let LibraryStatus::Unavailable {
            fallback_in_use,
            path,
            ..
        } = opened.status
        else {
            panic!("expected unavailable");
        };
        assert!(fallback_in_use);
        assert!(path.ends_with("sub"));

        // Both fail: no daemon, still a clear state.
        let opened = open_library(Some(&blocked.join("a")), &blocked.join("b"), |p| {
            Library::open_or_create(p)
        });
        assert!(opened.library.is_none());
        assert!(matches!(
            opened.status,
            LibraryStatus::Unavailable {
                fallback_in_use: false,
                ..
            }
        ));
        // No configured path at all.
        let opened = open_library(None, &fallback, Library::open_or_create);
        assert!(matches!(opened.status, LibraryStatus::Unavailable { .. }));
    }

    /// A second app instance cannot take the socket: the first keeps it and the second
    /// reports a warning but still works in process.
    #[test]
    fn second_instance_reports_ipc_warning() {
        let f = fixture();
        let ep = f.backend.endpoint.clone();
        let dir = tempfile::tempdir().unwrap();
        let second = Backend::start(
            &f.rt,
            StartConfig {
                library_path: Some(dir.path().join("lib")),
                fallback_path: dir.path().join("fb"),
                settings: Arc::new(FileSettings::load(dir.path().join("s.json"))),
                endpoint: ep,
                overlay_binary: None,
                recording: RecordingMode::Off,
            },
            fake_capturer(),
            Arc::new(Arc::new(RecordingClipboard::default())),
            DenyAll,
        );
        assert!(second.ipc_warning.is_some());
        assert!(second.daemon.is_some());
    }

    /// AGT-2.AC4 fail closed: when the settings file was unreadable and held policies that
    /// could not be restored, unknown clients are blocked (named ones keep their policy, and
    /// there are none here), and the choice survives a restart until the user allows them.
    #[test]
    fn unreadable_policy_store_blocks_unknown_clients_until_allowed() {
        let f = fixture_prepared(RecordingMode::Off, |dir| {
            std::fs::write(
                dir.join("settings.json"),
                "{\"agents.client_policies\": {\"a\": \"blocked\"}, broken",
            )
            .unwrap();
        });
        assert!(f.backend.settings.load_report().policies_unreadable);
        let daemon = f.backend.daemon.clone().unwrap();
        assert_eq!(
            daemon.policies().default_policy(),
            cm_daemon::ClientPolicy::Blocked
        );
        let refused = f.rt.block_on(async {
            let mut c = cm_daemon::connect(&f.backend.endpoint.clone().unwrap()).await.unwrap();
            use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
            let (r, mut w) = tokio::io::split(&mut c);
            w.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"session.hello\",\"params\":{\"client\":\"stranger\"}}\n")
                .await
                .unwrap();
            let mut line = String::new();
            BufReader::new(r).read_line(&mut line).await.unwrap();
            serde_json::from_str::<Value>(&line).unwrap()
        });
        assert_eq!(
            refused["error"]["data"]["data"]["reason"], "default_deny",
            "{refused}"
        );
        // The state is remembered: a restart without the broken file still blocks.
        assert!(f.backend.settings.deny_unknown());
        let again = FileSettings::load(f._dir.path().join("settings.json"));
        assert!(!again.load_report().policies_unreadable);
        assert!(again.deny_unknown());
        // "Allow unknown clients" in Settings -> Agents.
        daemon
            .policies()
            .set_default(cm_daemon::ClientPolicy::Allowed);
        f.backend.settings.set_deny_unknown(false).unwrap();
        assert!(!FileSettings::load(f._dir.path().join("settings.json")).deny_unknown());
        assert_eq!(
            daemon.policies().default_policy(),
            cm_daemon::ClientPolicy::Allowed
        );
    }

    /// R1-6: the rescan is the daemon's `rescan_library` on its own handle, not a second one.
    #[test]
    fn rescan_goes_through_the_daemon() {
        let f = fixture();
        let daemon = f.backend.daemon.clone().unwrap();
        f.rt.block_on(async {
            let shot = Backend::call(
                &daemon,
                "capture_screenshot",
                json!({"target": {"display_id": 1}}),
            )
            .await
            .unwrap();
            let id = shot["capture_id"].as_str().unwrap().to_owned();
            let dir = Backend::capture_dir(&daemon, &id).await.unwrap();
            let before = Backend::rescan_library(&daemon).await.unwrap();
            assert_eq!(
                before,
                RescanSummary {
                    indexed: 1,
                    removed: 0
                }
            );
            std::fs::remove_dir_all(dir).unwrap();
            let after = Backend::rescan_library(&daemon).await.unwrap();
            assert_eq!(
                after,
                RescanSummary {
                    indexed: 0,
                    removed: 1
                }
            );
        });
    }
}
