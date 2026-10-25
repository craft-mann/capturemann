//! Application state and the command dispatcher.
//!
//! `AppModel` is plain shared state (an entity so views can observe it). Everything that
//! opens windows or talks to the daemon is a free function taking `&Entity<AppModel>` and
//! `&mut App`, so no window is ever opened while the model is borrowed (a new window renders
//! immediately and reads the model).

use cm_i18n::{tr, tr_ctx};
use gpui::{
    point, px, size, App, AppContext as _, Bounds, Entity, Global, Task, TitlebarOptions,
    WindowBounds, WindowHandle, WindowKind, WindowOptions,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::backend::{Backend, CopyResult};
use crate::confirm::{ConfirmUiMsg, DialogModel};
use crate::flows;
use crate::library::LocalClock;
use crate::nav::NavCounts;
use crate::overlay::{CaptureMode, OverlayOutcome};
use crate::permissions::{self, Action as PermAction, Id as PermId, Provider as PermProvider};
use crate::platform::TrayHandle;
use crate::settings::{KEY_CAPTURE_DELAY, KEY_COPY_AFTER_CAPTURE, KEY_LANGUAGE, KEY_THEME};
use crate::shortcuts::{Platform, ShortcutAction, ShortcutMap};
use crate::theme::{Theme, ThemePreference};
use crate::toast::{ToastAction, ToastKind, ToastQueue, ToastSpec};
use crate::ui::confirm_window::ConfirmDialog;
use crate::ui::paint::is_light;
use crate::ui::recording::{self, RecState};
use crate::ui::settings::model::SettingsSection;
use crate::ui::studio::{Studio, ThemeGlobal};
use crate::ui::toast_window::ToastHost;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    /// All captures: the mixed library grid (nav item "All captures", `Filter::All`) and the
    /// default landing page (OD-16).
    Library,
    /// The screenshots library (nav item "Screenshots", the grid filtered to screenshots).
    Screenshots,
    /// The recordings library (the grid filtered to recordings).
    Recordings,
    /// Not built yet: publishing and drive destinations show an honest "Not available yet" page.
    Published,
    Destinations,
    /// Agents & CLI: MCP status and the agent activity log.
    Agents,
    /// The annotation editor for a screenshot (ANN-1..3).
    Editor(String),
    /// A recording's details card (LIB-1).
    Recording(String),
    /// The recording editor: preview, timeline, inspector (EDT-1 to EDT-3).
    RecordingEditor(String),
    /// The publish sheet for a recording (PUB-2).
    Publish(String),
    Settings,
    /// AI: the speech-model manager and the AI switches (M5, `specs/features/07-ai.md`). Its own
    /// sidebar section, not a page of Settings.
    Ai,
    /// The permissions setup screen (PRM-1): first run, and "Reopen setup" in Settings. A screen
    /// of the main window, not a separate window.
    Permissions,
}

/// What hotkeys, the tray menu, window actions and background tasks ask the app to do.
#[derive(Debug, Clone)]
pub enum Command {
    Hotkey(u32),
    TrayMenu(String),
    Capture(CaptureMode),
    ShowLibrary,
    ShowSettings,
    OpenCapture(String),
    ToastAction(ToastAction),
    Quit,
}

pub struct AppModel {
    pub backend: Arc<Backend>,
    pub shortcuts: ShortcutMap,
    /// Result of registering each shortcut with the OS (CAP-2.AC3).
    pub registration: Vec<(ShortcutAction, Result<(), String>)>,
    pub platform: Platform,
    pub clock: LocalClock,
    pub toasts: ToastQueue,
    pub route: Route,
    /// The last library page shown (All captures, Screenshots, Recordings or Published): where
    /// "Back to library" in the settings sidebar returns to.
    pub library_route: Route,
    /// Bumped whenever the library may have changed; the library view reloads.
    pub library_epoch: u64,
    /// Captures whose files changed since the library view last reloaded (`capture.created` and
    /// `capture.revised`): only their thumbnails are dropped (R1-7).
    pub changed_captures: Vec<String>,
    /// R1-6: when the library index was last rebuilt from the bundle folders, whether one is
    /// running, and how many bundles the last one indexed.
    pub last_rescan: Option<Instant>,
    pub rescan_busy: bool,
    pub last_indexed: Option<usize>,
    /// Quit was asked for; a second request (menu, shortcut, dock) does nothing while the first
    /// waits for a recording to be saved.
    pub quitting: bool,
    /// A capture overlay is open: further capture shortcuts are ignored (CAP-2 edge case).
    pub capture_busy: bool,
    /// CAP-5: the capture delay in seconds (0 = none), shared by the tray submenu and the capture
    /// toolbar, remembered in `capture.delay_s`; used by every capture start.
    pub capture_delay_s: u32,
    /// A launcher action's one-off "copy / don't copy" for the capture starting now (LNC-1
    /// Actions); read once by [`take_copy`], else the `copy_after_capture` setting applies.
    pub copy_override: Option<bool>,
    /// CAP-5: the delay that is running, with its pill.
    pub delay_run: Option<crate::ui::delay_pill::DelayRun>,
    pub delay_seq: u64,
    pub theme_pref: ThemePreference,
    /// The UI language the user picked (`appearance.language`, English until they pick one).
    /// [`apply_language`] makes it the language of the whole UI.
    pub language: cm_i18n::Lang,
    /// `--theme dark|light` debug flag: wins over the saved preference and the OS.
    pub theme_override: Option<ThemePreference>,
    /// Real library counts for the sidebar, set by the library view after an unfiltered load.
    pub counts: NavCounts,
    pub copy_after_capture: bool,
    /// The AI page: speech models, downloads the user started, the notice and the switches.
    pub ai: crate::ai::models::ModelsVm,
    /// Transcriptions in flight (the details card and the recording editor read them).
    pub transcribe: crate::ai::jobs::TranscribeVm,
    /// How many agent activity entries were already handled (toasts, sidebar card).
    pub agent_seen: usize,
    /// Last captures made in this run, newest first (tray "Recent captures").
    pub recents: Vec<(String, String)>,
    pub hotkey_ids: HashMap<u32, ShortcutAction>,
    pub tray: Option<TrayHandle>,
    /// Recording windows' shared state (HUD, countdown, indicator, bubble, start sheet).
    pub rec: Entity<RecState>,
    /// PRM-1: where permission states come from (the OS, or a fake in the demo flags).
    pub perm_provider: Arc<dyn PermProvider>,
    pub perms: permissions::Snapshot,
    /// Whether Screen Recording was granted when this process first looked; `None` until then.
    pub screen_at_start: Option<bool>,
    /// A permission request is waiting for the OS dialog (PRM-1.AC3).
    pub perm_busy: Option<PermId>,
    /// Permissions whose Grant or Open System Settings button was pressed in this run (drives the
    /// stale-grant hint, `permissions::stale_grant_hint`).
    pub perm_tried: std::collections::HashSet<PermId>,
    /// Set by "Open Permissions…": the Settings view switches to that page and clears it.
    pub settings_page_request: Option<SettingsSection>,
    studio: Option<WindowHandle<Studio>>,
    toast_window: Option<WindowHandle<ToastHost>>,
    /// The stack height the toast window was last fitted to (measured from the painted cards), so
    /// a measurement that cannot be honoured (a pile taller than the display) does not loop.
    toast_fit: f32,
    confirm_windows: Vec<(String, WindowHandle<ConfirmDialog>)>,
    toast_timer: Option<Task<()>>,
}

impl Global for ThemeGlobal {}

pub struct AppHandle(pub Entity<AppModel>);
impl Global for AppHandle {}

impl AppModel {
    /// How many confirmation dialogs are open (the main window is dimmed while any is).
    pub fn confirms_open(&self) -> usize {
        self.confirm_windows.len()
    }

    pub fn new(
        backend: Arc<Backend>,
        shortcuts: ShortcutMap,
        platform: Platform,
        clock: LocalClock,
        rec: Entity<RecState>,
    ) -> Self {
        let settings = backend.settings.snapshot();
        let theme_pref = settings
            .get(KEY_THEME)
            .and_then(|v| v.as_str())
            .and_then(ThemePreference::parse)
            .unwrap_or_default();
        let copy_after_capture = settings
            .get(KEY_COPY_AFTER_CAPTURE)
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        AppModel {
            backend,
            shortcuts,
            registration: Vec::new(),
            platform,
            clock,
            toasts: ToastQueue::default(),
            route: Route::Library,
            library_route: Route::Library,
            library_epoch: 0,
            changed_captures: Vec::new(),
            last_rescan: None,
            rescan_busy: false,
            last_indexed: None,
            quitting: false,
            capture_busy: false,
            capture_delay_s: crate::delay::from_setting(settings.get(KEY_CAPTURE_DELAY)),
            copy_override: None,
            delay_run: None,
            delay_seq: 0,
            theme_pref,
            language: crate::settings::language_from_settings(&settings),
            theme_override: None,
            counts: NavCounts::default(),
            copy_after_capture,
            ai: crate::ai::models::ModelsVm {
                notice_dismissed: settings
                    .get(crate::ui::ai::KEY_NOTICE_DISMISSED)
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                auto_transcribe: settings
                    .get(crate::ui::ai::KEY_AUTO_TRANSCRIBE)
                    .and_then(|v| v.as_bool())
                    .unwrap_or_else(crate::ui::ai::default_auto_transcribe),
                glossary: cm_ai::glossary::from_value(settings.get(crate::ui::ai::KEY_GLOSSARY)),
                ..Default::default()
            },
            transcribe: crate::ai::jobs::TranscribeVm::default(),
            agent_seen: 0,
            recents: Vec::new(),
            hotkey_ids: HashMap::new(),
            tray: None,
            rec,
            perm_provider: Arc::new(permissions::SystemProvider),
            perms: permissions::Snapshot::unchecked(permissions::Os::current()),
            screen_at_start: None,
            perm_busy: None,
            perm_tried: Default::default(),
            settings_page_request: None,
            studio: None,
            toast_window: None,
            toast_fit: 0.,
            confirm_windows: Vec::new(),
            toast_timer: None,
        }
    }
}

/// Polls the daemon's agent activity log every 2 s: refreshes the sidebar card and shows the
/// agent toast for each newly allowed agent action. The log is the only source of agent
/// identity the daemon exposes (events carry no actor), so this is a cheap poll, not a push.
pub fn start_agent_watch(app: &Entity<AppModel>, cx: &mut App) {
    let app2 = app.clone();
    cx.spawn(async move |cx| loop {
        cx.background_executor().timer(Duration::from_secs(2)).await;
        cx.update(|cx| {
            let (backend, seen) = {
                let m = app2.read(cx);
                (m.backend.clone(), m.agent_seen)
            };
            let Some(daemon) = backend.daemon.as_ref() else {
                return;
            };
            let activity = daemon.activity();
            let fresh = crate::agent_status::new_entries(seen, &activity);
            let total = activity.len();
            if fresh.is_empty() && total == seen {
                return;
            }
            let toasts: Vec<ToastSpec> = fresh
                .iter()
                .filter(|e| crate::agent_status::started_by_agent(e.decision))
                // The recording controller shows its own agent toast from the start event.
                .filter(|e| e.tool != "start_recording")
                .map(|e| ToastSpec::agent(e.tool.clone(), e.client.clone()))
                .collect();
            app2.update(cx, |m, cx| {
                m.agent_seen = total;
                cx.notify();
            });
            for spec in toasts {
                push_toast(&app2, spec, cx);
            }
        });
    })
    .detach();
}

/// Publishes the Studio window's CGWindowID twice a second (main thread), so recordings and
/// screenshots include the library window while it is on screen and drop it as soon as it is
/// closed or minimized (REC-6.AC8). The daemon's provider reads the published value from
/// capture threads; see `studio_window`.
pub fn start_studio_window_watch(app: &Entity<AppModel>, cx: &mut App) {
    let app2 = app.clone();
    cx.spawn(async move |cx| loop {
        cx.update(|cx| sync_studio_window(&app2, cx));
        cx.background_executor()
            .timer(Duration::from_millis(500))
            .await;
    })
    .detach();
}

/// One refresh of the published Studio window id.
pub fn sync_studio_window(app: &Entity<AppModel>, cx: &mut App) {
    let id = match app.read(cx).studio {
        Some(handle) => handle
            .update(cx, |_, window, _| {
                crate::platform_window::capture_window_id(window)
            })
            .ok()
            .flatten(),
        None => None,
    };
    crate::studio_window::publish(id);
}

/// How often the app looks at the bundle folders when it comes to the front (R1-6).
pub const RESCAN_EVERY: Duration = Duration::from_secs(10);

/// Whether a rescan is due: never while one runs, and at most every [`RESCAN_EVERY`] unless
/// `force`d.
pub fn rescan_due(last: Option<Instant>, busy: bool, now: Instant, force: bool) -> bool {
    !busy && (force || last.is_none_or(|t| now.saturating_duration_since(t) >= RESCAN_EVERY))
}

/// Whether a daemon error says the capture's bundle is gone (`no bundle at ...`, `no capture ...`):
/// the library row is a ghost and a rescan drops it (R1-6).
pub fn looks_missing(message: &str) -> bool {
    message.starts_with("no bundle at")
        || message.contains("no capture `")
        || message.contains("No such file or directory")
}

/// A capture could not be opened with `message`: when its folder is gone, rescan so the ghost row
/// disappears (R1-6).
pub fn note_open_failed(message: &str, cx: &mut App) {
    if !looks_missing(message) {
        return;
    }
    if let Some(app) = cx.try_global::<AppHandle>().map(|h| h.0.clone()) {
        request_rescan(&app, true, cx);
    }
}

/// Remembers that capture `id` changed and tells the views (the library reloads; only this
/// capture's thumbnail is dropped).
pub fn capture_changed(app: &Entity<AppModel>, id: Option<&str>, cx: &mut App) {
    app.update(cx, |m, cx| {
        if let Some(id) = id {
            if !m.changed_captures.iter().any(|c| c == id) && m.changed_captures.len() < 256 {
                m.changed_captures.push(id.to_owned());
            }
        }
        m.library_epoch += 1;
        cx.notify();
    });
}

/// R1-6: rebuilds the library index from the bundle folders on a worker thread (bundles copied in
/// appear, deleted ones stop being ghost rows). Called at startup, when the app comes to the front
/// (throttled to once per [`RESCAN_EVERY`]) and when opening a capture failed because its folder
/// is gone (`force`). The library reloads when something changed.
pub fn request_rescan(app: &Entity<AppModel>, force: bool, cx: &mut App) {
    let (daemon, due) = {
        let m = app.read(cx);
        (
            m.backend.daemon.clone(),
            rescan_due(m.last_rescan, m.rescan_busy, Instant::now(), force),
        )
    };
    let Some(daemon) = daemon.filter(|_| due) else {
        return;
    };
    app.update(cx, |m, _| {
        m.rescan_busy = true;
        m.last_rescan = Some(Instant::now());
    });
    let (rt, app2) = (app.read(cx).backend.rt.clone(), app.clone());
    cx.spawn(async move |cx| {
        let res = rt
            .spawn(async move { crate::backend::Backend::rescan_library(&daemon).await })
            .await;
        cx.update(|cx| {
            let summary = match res {
                Ok(Ok(s)) => Some(s),
                Ok(Err(e)) => {
                    eprintln!("cm-desktop: library rescan failed: {e}");
                    None
                }
                Err(e) => {
                    eprintln!("cm-desktop: library rescan failed: {e}");
                    None
                }
            };
            let changed = app2.update(cx, |m, _| {
                m.rescan_busy = false;
                let Some(s) = summary else { return false };
                let changed = s.removed > 0 || m.last_indexed != Some(s.indexed);
                m.last_indexed = Some(s.indexed);
                changed
            });
            if changed {
                capture_changed(&app2, None, cx);
            }
        });
    })
    .detach();
}

/// Recomputes the Studio theme from the preference and the OS appearance.
pub fn apply_theme(app: &Entity<AppModel>, cx: &mut App) {
    let pref = {
        let m = app.read(cx);
        m.theme_override.unwrap_or(m.theme_pref)
    };
    let mode = pref.resolve(Some(is_light(cx.window_appearance())));
    let theme = Theme::for_mode(mode);
    let changed = cx
        .try_global::<ThemeGlobal>()
        .is_none_or(|g| g.0.mode != theme.mode);
    if changed {
        cx.set_global(ThemeGlobal(theme));
        cx.refresh_windows();
    }
}

/// Makes the chosen UI language the language of every surface, live (no restart): sets the global
/// language (which bumps the `cm_i18n` epoch), rewrites the tray and menu bar texts, and
/// repaints every window (main, HUD, toasts, confirm dialogs, Record sheet, bubble), the way
/// [`apply_theme`] does for a theme. Call it at startup before the first window, and after the
/// language setting changed. Text already turned into a string (a toast on screen) keeps the
/// language it was made in; the next one uses the new language.
pub fn apply_language(app: &Entity<AppModel>, cx: &mut App) {
    let lang = app.read(cx).language;
    cm_i18n::set_language(lang);
    let rec = app.read(cx).rec.clone();
    {
        let m = app.read(cx);
        if let Some(tray) = &m.tray {
            tray.relabel(&m.recents);
        }
    }
    // The tray plan (tooltip, Pause / Resume text) is cached: drop it so the next sync rewrites it.
    rec.update(cx, |r, _| r.last_plan = None);
    crate::ui::recording::sync_tray(app, cx);
    cx.set_menus(crate::ui::studio::app_menus());
    // Sticky toasts on screen are worded again (and the toast window resized to the new text).
    if app.update(cx, |m, _| m.toasts.relabel()) {
        sync_toasts(app, cx);
    }
    cx.refresh_windows();
}

/// Changes the route only. Use from inside the Studio window's own event handlers (its
/// window is mid-update there, so it must not be re-entered).
pub fn set_route(app: &Entity<AppModel>, route: Route, cx: &mut App) {
    // Leaving the setup screen by any means counts as dismissing it for this launch (PRM-1.AC11).
    if app.read(cx).route == Route::Permissions && route != Route::Permissions {
        dismiss_first_run(app, cx);
    }
    // Looking at the publishes is how a failed publish is acknowledged (the tray's Attention).
    if matches!(route, Route::Published | Route::Publish(_)) {
        crate::ui::upload_ui::publishes_seen(app, cx);
    }
    app.update(cx, |m, cx| {
        if crate::nav::is_library_page(&route) {
            m.library_route = route.clone();
        }
        m.route = route;
        cx.notify();
    });
}

/// Changes the route and shows the Studio window (hotkeys, tray, toasts).
pub fn navigate(app: &Entity<AppModel>, route: Route, cx: &mut App) {
    set_route(app, route, cx);
    open_studio(app, cx);
}

/// Where "Open Permissions…" (banner, toasts, tray, launcher, Record sheet) lands: the
/// Permissions page of Settings, in the main window (PRM-1.AC12).
pub fn permissions_entry() -> (Route, SettingsSection) {
    (Route::Settings, SettingsSection::Permissions)
}

/// What the app does at launch (PRM-1.AC5, AC12). A debug-flag route wins; otherwise the setup
/// screen when it is due; otherwise the library, except for `--background` launches, which open no
/// window. A due setup screen is the only reason `--background` opens one.
pub fn startup_route(show_setup: bool, background: bool, explicit: Option<Route>) -> Option<Route> {
    if explicit.is_some() {
        explicit
    } else if show_setup {
        Some(Route::Permissions)
    } else if background {
        None
    } else {
        Some(Route::Library)
    }
}

/// Opens (or raises) the main window on Settings -> Permissions.
pub fn open_permissions_page(app: &Entity<AppModel>, cx: &mut App) {
    let (route, page) = permissions_entry();
    app.update(cx, |m, _| m.settings_page_request = Some(page));
    navigate(app, route, cx);
}

/// Opens (or raises) the main window on the setup screen.
pub fn open_setup_screen(app: &Entity<AppModel>, cx: &mut App) {
    navigate(app, Route::Permissions, cx);
}

// ---- permissions (PRM-1) -------------------------------------------------------------------------

/// Reads every permission state again (cheap, never prompts) and notifies when something changed.
pub fn refresh_permissions(app: &Entity<AppModel>, cx: &mut App) {
    app.update(cx, |m, cx| {
        let snap = permissions::probe(&*m.perm_provider, m.screen_at_start);
        if m.screen_at_start.is_none() && snap.checked {
            // The raw state at the first look; later turns of this row to Granted need a restart.
            m.screen_at_start = Some(snap.state(PermId::ScreenRecording).is_granted());
        }
        if snap != m.perms {
            m.perms = snap;
            cx.notify();
        }
    });
}

/// Runs the action of a permission card button.
pub fn permission_action(app: &Entity<AppModel>, id: PermId, action: PermAction, cx: &mut App) {
    if matches!(action, PermAction::Grant | PermAction::OpenSettings) {
        app.update(cx, |m, cx| {
            m.perm_tried.insert(id);
            cx.notify();
        });
    }
    match action {
        PermAction::Grant => request_permission(app, id, cx),
        PermAction::OpenSettings => permissions::open_settings(permissions::Os::current(), id),
        PermAction::Restart => restart(app, cx),
    }
}

/// Asks the OS for `id` (this may open a system dialog and wait for the answer), then reads the
/// states again. Only ever called from a button or just before the user turns the feature on.
pub fn request_permission(app: &Entity<AppModel>, id: PermId, cx: &mut App) {
    request_permission_then(app, id, cx, |_| {});
}

/// [`request_permission`], then `then` runs once the answer is in (the Record sheet reloads its
/// devices).
pub fn request_permission_then(
    app: &Entity<AppModel>,
    id: PermId,
    cx: &mut App,
    then: impl FnOnce(&mut App) + 'static,
) {
    let (provider, rt) = {
        let m = app.read(cx);
        if m.perm_busy.is_some() {
            return;
        }
        (m.perm_provider.clone(), m.backend.rt.clone())
    };
    app.update(cx, |m, cx| {
        m.perm_busy = Some(id);
        cx.notify();
    });
    let app2 = app.clone();
    cx.spawn(async move |cx| {
        let _ = rt
            .spawn_blocking(move || {
                provider.request(id);
            })
            .await;
        cx.update(|cx| {
            app2.update(cx, |m, cx| {
                m.perm_busy = None;
                cx.notify();
            });
            refresh_permissions(&app2, cx);
            then(cx);
        });
    })
    .detach();
}

/// The `.app` that `exe` (`X.app/Contents/MacOS/bin`) lives in.
pub fn bundle_of(exe: &std::path::Path) -> Option<std::path::PathBuf> {
    let macos = exe.parent()?;
    let contents = macos.parent()?;
    let app = contents.parent()?;
    (macos.file_name()? == "MacOS"
        && contents.file_name()? == "Contents"
        && app.extension()? == "app")
        .then(|| app.to_path_buf())
}

/// Relaunches the bundle (`open -n`, once this process has exited so the two never share the daemon
/// socket) and quits. A bare dev binary cannot relaunch itself: say so.
pub fn restart(app: &Entity<AppModel>, cx: &mut App) {
    let bundle = std::env::current_exe().ok().and_then(|e| bundle_of(&e));
    let Some(bundle) = bundle else {
        push_toast(
            app,
            ToastSpec::new(ToastKind::Info, tr!("Quit and start CaptureMann again")).body(
                tr!("This copy is not running from a CaptureMann.app, so it can\u{2019}t relaunch itself."),
            ),
            cx,
        );
        return;
    };
    #[cfg(unix)]
    {
        // The helper waits for this process to be gone (it may be saving a recording first), so the
        // two instances never overlap on the library, the socket or the shortcuts (R4-10).
        let _ = std::process::Command::new("sh")
            .arg("-c")
            .arg(crate::instance::relaunch_script(std::process::id()))
            .arg(&bundle)
            .spawn();
    }
    #[cfg(not(unix))]
    let _ = bundle;
    quit(app, cx);
}

/// At startup: read the states and report whether the setup screen is due (first launch with a
/// gap, or a required permission is missing). `force` makes it due regardless (the demo flags).
/// Everything granted is recorded as onboarded without showing anything.
pub fn prepare_permissions(app: &Entity<AppModel>, force: bool, cx: &mut App) -> bool {
    refresh_permissions(app, cx);
    let (snap, onboarded) = {
        let m = app.read(cx);
        let onboarded = m
            .backend
            .settings
            .snapshot()
            .get(crate::settings::KEY_PERMISSIONS_ONBOARDED)
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        (m.perms.clone(), onboarded)
    };
    if permissions::onboard_silently(&snap, onboarded) {
        mark_onboarded(app, cx);
    }
    force || permissions::show_first_run(&snap, onboarded)
}

fn mark_onboarded(app: &Entity<AppModel>, cx: &mut App) {
    let settings = app.read(cx).backend.settings.clone();
    let _ = settings.set_internal(
        crate::settings::KEY_PERMISSIONS_ONBOARDED,
        serde_json::Value::Bool(true),
    );
}

/// The setup screen was left in any way: remember the onboarding unless a required permission is
/// still missing (then it reappears next launch).
pub fn dismiss_first_run(app: &Entity<AppModel>, cx: &mut App) {
    if permissions::closing_marks_onboarded(&app.read(cx).perms) {
        mark_onboarded(app, cx);
    }
}

/// Centers a just-opened dialog on the visible part of the display with the pointer and puts it
/// in front of every other window of the app, key and focused. Deferred so it runs after the
/// window exists and outside its own update (the frame change re-enters GPUI's resize callback).
pub(crate) fn raise_centered<V: 'static>(handle: gpui::WindowHandle<V>, cx: &mut App) {
    let visible = crate::ui::recording::pointer_display(cx)
        .map(|(_, r)| crate::ui::recording::visible_part(&r))
        .map(|v| (v.x, v.y, v.w, v.h));
    cx.defer(move |cx| {
        let _ = handle.update(cx, |_, window, _| {
            crate::platform_window::bring_to_front(window, visible);
            window.activate_window();
        });
    });
}

/// "Continue" / "Skip optional" on the setup screen: remember it was seen.
pub fn finish_first_run(app: &Entity<AppModel>, cx: &mut App) {
    mark_onboarded(app, cx);
}

/// Opens the Studio window, or brings the existing one to the front.
///
/// Capture exclusion (R4-6, REC-6.AC8): the recording windows (HUD, countdown, indicator,
/// bubble), the toasts, the start sheet, the confirmation dialogs and the launcher are hidden
/// from screen capture (`sharingType = none`) and excluded by pid. Studio (library, editor and
/// the setup screens are this one window) is deliberately NOT hardened and IS captured: it is
/// what the user sees on screen, so a recording or screenshot includes it while it is visible.
/// Its window id reaches the engine through `studio_window` (`start_studio_window_watch`).
pub fn open_studio(app: &Entity<AppModel>, cx: &mut App) {
    let existing = app.read(cx).studio;
    if let Some(handle) = existing {
        if handle
            .update(cx, |_, window, _| {
                window.activate_window();
                crate::platform_window::bring_to_front(window, None);
            })
            .is_ok()
        {
            cx.activate(true);
            return;
        }
    }
    let bounds = Bounds::centered(None, size(px(1180.), px(760.)), cx);
    let app2 = app.clone();
    let opened = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some("CaptureMann".into()),
                appears_transparent: false,
                traffic_light_position: None,
            }),
            window_min_size: Some(size(px(720.), px(480.))),
            app_id: Some("app.capturemann.desktop".into()),
            ..Default::default()
        },
        move |window, cx| cx.new(|cx| Studio::new(app2, window, cx)),
    );
    match opened {
        Ok(handle) => {
            app.update(cx, |m, _| m.studio = Some(handle));
            sync_studio_window(app, cx);
            cx.activate(true);
        }
        Err(e) => push_toast(
            app,
            flows::error_toast_live(|| tr!("Couldn\u{2019}t open the window"), e.to_string()),
            cx,
        ),
    }
}

pub fn push_toast(app: &Entity<AppModel>, spec: ToastSpec, cx: &mut App) {
    app.update(cx, |m, cx| {
        m.toasts.push(spec, Instant::now());
        cx.notify();
    });
    sync_toasts(app, cx);
}

/// Removes the agent toasts that carry a Stop button (the recording ended).
pub fn dismiss_stop_toasts(app: &Entity<AppModel>, cx: &mut App) {
    let ids: Vec<u64> = app
        .read(cx)
        .toasts
        .with_action(|a| matches!(a, ToastAction::StopRecording(_)));
    for id in ids {
        dismiss_toast(app, id, cx);
    }
}

pub fn dismiss_toast(app: &Entity<AppModel>, id: u64, cx: &mut App) {
    app.update(cx, |m, cx| {
        m.toasts.dismiss(id);
        cx.notify();
    });
    sync_toasts(app, cx);
    // A dismissed "Reconnect needed" toast has been seen: the tray drops its Attention state.
    crate::ui::upload_ui::toast_dismissed(app, id, cx);
}

/// The pointer entered or left a toast (hover pauses its auto-dismiss clock, redesign R17).
pub fn set_toast_hover(app: &Entity<AppModel>, id: u64, hovered: bool, cx: &mut App) {
    let changed = app.update(cx, |m, cx| {
        let c = m.toasts.set_hover(id, hovered, Instant::now());
        if c {
            cx.notify();
        }
        c
    });
    if changed {
        // Re-arm the single expiry timer for the new clocks.
        sync_toasts(app, cx);
    }
}

/// Where the toast window of height `win_h` sits on a display of `size` (points): bottom-right,
/// 16 px from the edge and 48 above the Dock. One stack holds every kind, the agent toast too
/// (redesign R17; it was top-right before).
pub fn toast_origin(size: (f32, f32), win_h: f32) -> (f32, f32) {
    let w = crate::toast::TOAST_WIDTH;
    (size.0 - w - 16., size.1 - win_h - 48.)
}

/// Resizes the toast window to the height the cards really painted at (`measured`), growing it
/// upwards so the stack stays anchored at the bottom. The height estimate in `toast::estimate_height`
/// sizes the window first; this corrects it, so the top card is never cut off and no empty strip
/// stays above the stack. Never taller than the display leaves room for.
pub(crate) fn fit_toast_window(app: &Entity<AppModel>, measured: f32, cx: &mut App) {
    let Some(handle) = app.read(cx).toast_window else {
        return;
    };
    let display = cx.primary_display().map(|d| d.bounds());
    let height = display.map_or(measured, |b| {
        measured.min((b.size.height.as_f32() - 96.).max(60.))
    });
    let height = height.ceil();
    if (app.read(cx).toast_fit - height).abs() < 0.5 {
        return;
    }
    let Some(b) = display else {
        return;
    };
    let (x, y) = toast_origin((b.size.width.as_f32(), b.size.height.as_f32()), height);
    let (x, y) = (
        f64::from(b.origin.x.as_f32() + x),
        f64::from(b.origin.y.as_f32() + y),
    );
    let native = handle
        .update(cx, |_, window, _| {
            crate::platform_window::NativeWindow::of(window)
        })
        .ok()
        .flatten();
    let Some(native) = native else {
        return;
    };
    app.update(cx, |m, _| m.toast_fit = height);
    // Resize from a task, never inside a window update (see `NativeWindow`).
    cx.spawn(async move |_| {
        native.set_frame(
            x,
            y,
            f64::from(crate::toast::TOAST_WIDTH),
            f64::from(height),
        );
    })
    .detach();
}

/// Keeps the toast window in step with the queue. A window that exists is updated in place (its
/// host repaints from the model, and it is resized when the stack got taller or shorter), so hover,
/// focus and progress animations survive new toasts (R4-23); a window is created only for a stack
/// that had none, and closed when the stack empties. Re-creates nothing it does not have to, and
/// arms the auto-dismiss timer.
pub(crate) fn sync_toasts(app: &Entity<AppModel>, cx: &mut App) {
    {
        let (empty, height) = {
            let m = app.read(cx);
            let stack = m.toasts.toasts();
            (stack.is_empty(), crate::toast::stack_height(stack))
        };
        let existing = app.read(cx).toast_window;
        if empty {
            if let Some(h) = existing {
                app.update(cx, |m, _| m.toast_window = None);
                let _ = h.update(cx, |_, window, _| window.remove_window());
            }
        } else {
            let display = cx.primary_display().map(|d| d.bounds());
            // Never taller than the display leaves room for (a pile of toasts on a short screen).
            let height = display.map_or(height, |b| {
                height.min((b.size.height.as_f32() - 96.).max(60.))
            });
            let (win_w, win_h) = (crate::toast::TOAST_WIDTH, height);
            let origin = match display {
                Some(b) => {
                    let (x, y) =
                        toast_origin((b.size.width.as_f32(), b.size.height.as_f32()), win_h);
                    point(b.origin.x + px(x), b.origin.y + px(y))
                }
                None => point(px(100.), px(100.)),
            };
            let mut keep_existing = existing;
            if let Some(h) = existing {
                // Resize from a task, never inside a window update (see `NativeWindow`).
                let native = h
                    .update(cx, |_, window, _| {
                        crate::platform_window::NativeWindow::of(window)
                    })
                    .ok()
                    .flatten();
                if let Some(native) = native {
                    app.update(cx, |m, _| m.toast_fit = win_h);
                    let (x, y) = (
                        f64::from(f32::from(origin.x)),
                        f64::from(f32::from(origin.y)),
                    );
                    cx.spawn(async move |_| {
                        native.set_frame(x, y, f64::from(win_w), f64::from(win_h));
                    })
                    .detach();
                } else {
                    // No native window to resize (not macOS): fall back to a new window.
                    app.update(cx, |m, _| m.toast_window = None);
                    let _ = h.update(cx, |_, window, _| window.remove_window());
                    keep_existing = None;
                }
            }
            if keep_existing.is_none() {
                let app2 = app.clone();
                let opened = cx.open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(Bounds {
                            origin,
                            size: size(px(win_w), px(win_h)),
                        })),
                        titlebar: None,
                        focus: false,
                        show: true,
                        kind: WindowKind::PopUp,
                        is_movable: false,
                        is_resizable: false,
                        is_minimizable: false,
                        // Only the cards are drawn: no panel behind the stack.
                        window_background: gpui::WindowBackgroundAppearance::Transparent,
                        ..Default::default()
                    },
                    move |window, cx| {
                        // R4-6: a sticky toast must not end up in the next screenshot or recording.
                        // No window shadow either: a transparent window otherwise keeps a faint
                        // rectangular halo around the whole stack (the panel the owner saw).
                        crate::ui::recording::harden(window, false);
                        cx.new(|cx| ToastHost::new(app2, cx))
                    },
                );
                if let Ok(handle) = opened {
                    app.update(cx, |m, _| {
                        m.toast_window = Some(handle);
                        m.toast_fit = win_h;
                    });
                }
            }
        }
    }
    if app.read(cx).toasts.is_empty() {
        app.update(cx, |m, _| m.toast_timer = None);
        return;
    }
    // One timer for the next expiry.
    let next = app.read(cx).toasts.next_expiry(Instant::now());
    let app3 = app.clone();
    let timer = next.map(|d| {
        cx.spawn(async move |cx| {
            cx.background_executor()
                .timer(d + Duration::from_millis(20))
                .await;
            cx.update(|cx| {
                let changed = app3.update(cx, |m, cx| {
                    let c = m.toasts.expire(Instant::now());
                    if c {
                        cx.notify();
                    }
                    c
                });
                // Either something expired, or only sticky toasts remain: re-arm or close.
                if changed {
                    sync_toasts(&app3, cx);
                }
            });
        })
    });
    app.update(cx, |m, _| m.toast_timer = timer);
}

pub fn handle_command(app: &Entity<AppModel>, cmd: Command, cx: &mut App) {
    match cmd {
        Command::Hotkey(id) => {
            let action = app.read(cx).hotkey_ids.get(&id).copied();
            match action {
                Some(ShortcutAction::CaptureRegion) => start_capture(app, CaptureMode::Region, cx),
                Some(ShortcutAction::CaptureWindow) => start_capture(app, CaptureMode::Window, cx),
                Some(ShortcutAction::CaptureDisplay) => {
                    start_capture(app, CaptureMode::Display, cx)
                }
                Some(ShortcutAction::OpenLibrary) => navigate(app, Route::Library, cx),
                Some(ShortcutAction::OpenLauncher) => crate::ui::launcher_window::toggle(app, cx),
                Some(ShortcutAction::ToggleRecording) => recording::toggle_hotkey(app, cx),
                Some(ShortcutAction::PauseRecording) => recording::pause_hotkey(app, cx),
                Some(ShortcutAction::DiscardRecording) => recording::ask_discard(app, false, cx),
                Some(ShortcutAction::RestartRecording) => recording::ask_discard(app, true, cx),
                Some(ShortcutAction::AnnotateLast) => annotate_last(app, cx),
                None if id == crate::ui::delay_pill::escape_hotkey_id() => {
                    crate::ui::delay_pill::cancel(app, cx)
                }
                None => {}
            }
        }
        Command::TrayMenu(id) => {
            if let Some(rest) = id.strip_prefix("recent:") {
                navigate(app, Route::Editor(rest.to_owned()), cx);
                return;
            }
            if let Some(secs) = crate::delay::from_menu_id(&id) {
                set_capture_delay(app, secs, cx);
                return;
            }
            match id.as_str() {
                "capture_region" => start_capture(app, CaptureMode::Region, cx),
                "capture_window" => start_capture(app, CaptureMode::Window, cx),
                "capture_display" => start_capture(app, CaptureMode::Display, cx),
                "open_library" => navigate(app, Route::Library, cx),
                "open_launcher" => crate::ui::launcher_window::toggle(app, cx),
                "settings" => navigate(app, Route::Settings, cx),
                crate::platform::ids::PERMISSIONS => open_permissions_page(app, cx),
                crate::platform::ids::RECORD => recording::open_sheet(app, None, None, cx),
                crate::platform::ids::REC_STOP => {
                    if app.read(cx).rec.read(cx).hud.phase
                        == crate::recording::model::Phase::Countdown
                    {
                        recording::cancel_countdown(app, cx);
                    } else {
                        recording::stop(app, cx);
                    }
                }
                crate::platform::ids::REC_PAUSE => recording::pause_hotkey(app, cx),
                "quit" => quit(app, cx),
                _ => {}
            }
        }
        Command::Capture(mode) => start_capture(app, mode, cx),
        Command::ShowLibrary => navigate(app, Route::Library, cx),
        Command::ShowSettings => navigate(app, Route::Settings, cx),
        Command::OpenCapture(id) => navigate(app, Route::Editor(id), cx),
        Command::ToastAction(a) => toast_action(app, a, cx),
        Command::Quit => quit(app, cx),
    }
}

/// The keyboard route to a toast's Annotate button (R4-12): the newest capture of this run in the
/// editor.
pub fn annotate_last(app: &Entity<AppModel>, cx: &mut App) {
    let last = app.read(cx).recents.first().map(|(id, _)| id.clone());
    match last {
        Some(id) => navigate(app, open_route(&id), cx),
        None => push_toast(
            app,
            ToastSpec::new(ToastKind::Info, tr!("No capture to annotate yet"))
                .body(tr!("Take a screenshot first.")),
            cx,
        ),
    }
}

/// Binds `action` to `chord` (CAP-2.AC2): refuses a chord another action uses ("Already used by
/// ..."), moves the OS registration (refusing one another app owns), saves it as `shortcuts.*`
/// and updates the table every window reads. Errors are plain sentences for the Settings page.
/// Why a shortcut was not rebound. Kept as a reason and worded when drawn
/// ([`RebindError::text`]), so the line follows a language switch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RebindError {
    /// The pressed combination has no modifier.
    NeedsModifier,
    /// The key cannot be part of a shortcut.
    BadKey,
    /// Another action already uses the combination.
    Conflict(crate::shortcuts::Conflict),
    /// The OS refused to register it (another app uses it).
    Register,
}

impl RebindError {
    pub fn text(&self) -> String {
        match self {
            RebindError::NeedsModifier => {
                tr!("Hold a modifier key (Control, Option or Shift, Command or Windows) too.")
            }
            RebindError::BadKey => tr!(
                "That key can\u{2019}t be used in a shortcut. Try a letter, a digit, Space or F1 to F24."
            ),
            RebindError::Conflict(c) => c.to_string(),
            RebindError::Register => {
                tr!("Couldn\u{2019}t register \u{2014} used by another app")
            }
        }
    }
}

pub fn rebind_shortcut(
    app: &Entity<AppModel>,
    action: ShortcutAction,
    chord: crate::shortcuts::Chord,
    cx: &mut App,
) -> Result<(), RebindError> {
    let (map, old) = {
        let m = app.read(cx);
        (m.shortcuts.clone(), m.shortcuts.get(action))
    };
    map.check_rebind(action, &chord)
        .map_err(RebindError::Conflict)?;
    if old == chord {
        return Ok(());
    }
    let ids = match cx.try_global::<crate::platform::HotkeyHolder>() {
        Some(h) => Some(
            crate::platform::rebind_hotkey(&h.0, &old, &chord).map_err(|e| {
                eprintln!("cm-desktop: could not register {}: {e}", action.label());
                RebindError::Register
            })?,
        ),
        None => None,
    };
    set_setting(app, action.key(), serde_json::json!(chord.to_setting()), cx);
    app.update(cx, |m, cx| {
        m.shortcuts = map.with(action, chord);
        if let Some((old_id, new_id)) = ids {
            m.hotkey_ids.remove(&old_id);
            m.hotkey_ids.insert(new_id, action);
        }
        for (a, res) in &mut m.registration {
            if *a == action {
                *res = Ok(());
            }
        }
        cx.notify();
    });
    Ok(())
}

fn toast_action(app: &Entity<AppModel>, action: ToastAction, cx: &mut App) {
    match action {
        ToastAction::OpenLibrary => navigate(app, Route::Library, cx),
        ToastAction::OpenCapture(id) => navigate(app, open_route(&id), cx),
        ToastAction::CopyCapture(id) => {
            let backend = app.read(cx).backend.clone();
            let app2 = app.clone();
            cx.spawn(async move |cx| {
                let b = backend.clone();
                let res = backend
                    .rt
                    .spawn(async move { b.copy_capture(&id).await })
                    .await;
                cx.update(|cx| match res {
                    Ok(Ok(())) => push_toast(
                        &app2,
                        ToastSpec::new(ToastKind::Success, tr!("Copied to clipboard")),
                        cx,
                    ),
                    Ok(Err(e)) => push_toast(
                        &app2,
                        flows::error_toast_live(|| tr!("Couldn\u{2019}t copy"), e),
                        cx,
                    ),
                    Err(e) => push_toast(
                        &app2,
                        flows::error_toast_live(|| tr!("Couldn\u{2019}t copy"), e.to_string()),
                        cx,
                    ),
                });
            })
            .detach();
        }
        ToastAction::SaveCaptureAs(id) => save_capture_as(app, &id, cx),
        ToastAction::DeleteCapture(id) => {
            let title = app
                .read(cx)
                .recents
                .iter()
                .find(|(i, _)| *i == id)
                .map_or_else(|| tr!("this capture"), |(_, t)| t.clone());
            delete_capture(app, &id, &title, cx)
        }
        ToastAction::OpenScreenRecordingSettings => open_screen_recording_settings(),
        ToastAction::OpenPrivacyPane(pane) => open_privacy_pane(pane),
        ToastAction::OpenPermissions => open_permissions_page(app, cx),
        ToastAction::PlayExport(id) => crate::ui::export_ui::play(app, &id, cx),
        ToastAction::RevealExport(id) => crate::ui::export_ui::reveal(app, &id, cx),
        ToastAction::CopyLink(link) => crate::ui::upload_ui::copy_link(app, &link, cx),
        ToastAction::OpenLink(link) => crate::ui::upload_ui::open_link(app, &link, cx),
        ToastAction::RevealPath(path) => crate::ui::upload_ui::reveal_path(app, &path, cx),
        ToastAction::RetryJob(id) => crate::ui::upload_ui::retry(app, &id, cx),
        ToastAction::OpenDestinations => crate::ui::upload_ui::open_destinations(app, cx),
        ToastAction::StopRecording(id) => {
            crate::ui::recording::stop_recording_if_running(app, &id, cx)
        }
        ToastAction::CancelJob(id) => crate::ui::upload_ui::cancel(app, &id, cx),
    }
}

/// CAP-4.AC1 "Save as": renders the capture (annotations included), asks where to put it and
/// writes the PNG. Screenshots only; a recording's file is the exported MP4.
pub fn save_capture_as(app: &Entity<AppModel>, id: &str, cx: &mut App) {
    let backend = app.read(cx).backend.clone();
    let (app2, id) = (app.clone(), id.to_owned());
    cx.spawn(async move |cx| {
        let Some(daemon) = backend.daemon.clone() else {
            return;
        };
        let (d, i) = (daemon, id.clone());
        let rendered = backend
            .rt
            .spawn(async move { Backend::capture_png(&d, &i).await })
            .await;
        let (title, png) = match rendered {
            Ok(Ok(v)) => v,
            Ok(Err(e)) => {
                cx.update(|cx| {
                    push_toast(
                        &app2,
                        flows::error_toast_live(|| tr!("Couldn\u{2019}t save the image"), e),
                        cx,
                    )
                });
                return;
            }
            Err(e) => {
                cx.update(|cx| {
                    push_toast(
                        &app2,
                        flows::error_toast_live(
                            || tr!("Couldn\u{2019}t save the image"),
                            e.to_string(),
                        ),
                        cx,
                    )
                });
                return;
            }
        };
        let name = crate::editor::session::suggested_file_name(&title);
        let dir = dirs::picture_dir()
            .or_else(dirs::home_dir)
            .unwrap_or_else(std::env::temp_dir);
        let rx = cx.update(|cx| cx.prompt_for_new_path(&dir, Some(&name)));
        let Ok(Ok(Some(path))) = rx.await else {
            return;
        };
        let p = path.clone();
        let written = backend
            .rt
            .spawn_blocking(move || crate::editor::session::write_png(&p, &png))
            .await;
        cx.update(|cx| match written {
            Ok(Ok(())) => push_toast(
                &app2,
                ToastSpec::new(ToastKind::Success, tr!("Saved the image"))
                    .body(path.display().to_string()),
                cx,
            ),
            Ok(Err(e)) => push_toast(
                &app2,
                flows::error_toast_live(|| tr!("Couldn\u{2019}t save the image"), e),
                cx,
            ),
            Err(e) => push_toast(
                &app2,
                flows::error_toast_live(|| tr!("Couldn\u{2019}t save the image"), e.to_string()),
                cx,
            ),
        });
    })
    .detach();
}

/// Asks with the app's own confirmation dialog; `then` runs with the answer.
pub fn ask_confirm(
    app: &Entity<AppModel>,
    model: DialogModel,
    then: impl FnOnce(&Entity<AppModel>, cm_daemon::Decision, &mut App) + 'static,
    cx: &mut App,
) {
    let id = model.id.clone();
    let (tx, rx) = tokio::sync::oneshot::channel();
    handle_confirm_msg(app, ConfirmUiMsg::Show { model, reply: tx }, cx);
    let app2 = app.clone();
    cx.spawn(async move |cx| {
        let decision = rx.await.unwrap_or(cm_daemon::Decision::Decline);
        cx.update(|cx| {
            // The window removed itself; forget its entry.
            handle_confirm_msg(&app2, ConfirmUiMsg::Close { id }, cx);
            then(&app2, decision, cx);
        });
    })
    .detach();
}

/// CAP-4.AC1 / LIB-1 "Delete": asks, then `delete_capture` (the capture moves to the Trash). A
/// capture with copies online gets the remote question too (the box is off by default, so the
/// copies stay unless the user ticks it); the decisions and the words live in
/// `crate::delete_flow`.
pub fn delete_capture(app: &Entity<AppModel>, id: &str, title: &str, cx: &mut App) {
    use crate::delete_flow::{self, DaemonApi, Level};
    let backend = app.read(cx).backend.clone();
    let Some(daemon) = backend.daemon.clone() else {
        return;
    };
    let (app, id, title) = (app.clone(), id.to_owned(), title.to_owned());
    cx.spawn(async move |cx| {
        // What is online for this capture (the daemon reads the manifest; nothing is sent).
        let api = std::sync::Arc::new(DaemonApi(daemon));
        let prompt = {
            let (api, id) = (api.clone(), id.clone());
            backend
                .rt
                .spawn(async move { delete_flow::prepare(api.as_ref(), &id).await })
                .await
                .unwrap_or_default()
        };
        cx.update(|cx| {
            let model = prompt.dialog(&title);
            ask_confirm(
                &app,
                model,
                move |app, decision, cx| {
                    let Some(also_remote) = delete_flow::also_remote(decision, &prompt) else {
                        return;
                    };
                    let (app2, rt) = (app.clone(), backend.rt.clone());
                    cx.spawn(async move |cx| {
                        let res = rt
                            .spawn(async move {
                                delete_flow::finish(api.as_ref(), &id, also_remote).await
                            })
                            .await;
                        cx.update(|cx| match res {
                            Ok(n) => {
                                if n.local_deleted {
                                    capture_changed(&app2, None, cx);
                                }
                                let kind = match n.level {
                                    Level::Success => ToastKind::Success,
                                    Level::Error => ToastKind::Error,
                                };
                                let mut spec = ToastSpec::new(kind, n.title);
                                if let Some(b) = n.body {
                                    spec = spec.body(b);
                                }
                                push_toast(&app2, spec, cx);
                            }
                            Err(e) => push_toast(
                                &app2,
                                flows::error_toast_live(
                                    || tr!("Couldn\u{2019}t delete the capture"),
                                    e.to_string(),
                                ),
                                cx,
                            ),
                        });
                    })
                    .detach();
                },
                cx,
            );
        });
    })
    .detach();
}

/// LIB-1 "Rename": `rename_capture { capture_id, title }`. The daemon has no such action yet, so
/// the call is expected to say `not_implemented` or `method not found`; the user is told that
/// plainly and nothing else happens.
pub fn rename_capture(app: &Entity<AppModel>, id: &str, title: &str, cx: &mut App) {
    let title = title.trim().to_owned();
    if title.is_empty() {
        return;
    }
    let backend = app.read(cx).backend.clone();
    let Some(daemon) = backend.daemon.clone() else {
        return;
    };
    let (app2, id) = (app.clone(), id.to_owned());
    cx.spawn(async move |cx| {
        let params = serde_json::json!({ "capture_id": id, "title": title });
        let res = backend
            .rt
            .spawn(async move { Backend::call_coded(&daemon, "rename_capture", params).await })
            .await;
        cx.update(|cx| match res {
            Ok(Ok(_)) => capture_changed(&app2, Some(&id), cx),
            Ok(Err(e)) => push_toast(
                &app2,
                unavailable_or_error(&tr!("Couldn\u{2019}t rename the capture"), &e),
                cx,
            ),
            Err(e) => push_toast(
                &app2,
                flows::error_toast_live(
                    || tr!("Couldn\u{2019}t rename the capture"),
                    e.to_string(),
                ),
                cx,
            ),
        });
    })
    .detach();
}

/// An action the daemon does not offer yet (`not_implemented`, or an unknown method) is "not
/// available yet", not a failure the user caused.
pub fn unavailable_or_error(title: &str, e: &crate::backend::CallError) -> ToastSpec {
    if is_unavailable(e) {
        ToastSpec::new(ToastKind::Info, tr!("Not available yet"))
            .body(tr!("This action isn\u{2019}t built into CaptureMann yet."))
    } else {
        flows::error_toast(title, crate::daemon_text::call_error_text(e))
    }
}

/// Whether a failed call means "this action does not exist in this build".
pub fn is_unavailable(e: &crate::backend::CallError) -> bool {
    e.code == Some(cm_core::ErrorCode::NotImplemented)
        || e.message.to_ascii_lowercase().contains("method not found")
        || e.message.to_ascii_lowercase().contains("unknown method")
        || e.message.to_ascii_lowercase().contains("not implemented")
}

/// A recording opens its details card, a screenshot the editor (recording ids start with `rec_`).
pub fn open_route(id: &str) -> Route {
    if id.starts_with("rec_") {
        Route::Recording(id.to_owned())
    } else {
        Route::Editor(id.to_owned())
    }
}

/// Quits. A recording in progress is stopped and its bundle closed first (REC-8, NFR-6): waiting
/// for the engine can take a moment, so the quit happens when it is done (at most 10 s).
pub fn quit(app: &Entity<AppModel>, cx: &mut App) {
    if app.update(cx, |m, _| std::mem::replace(&mut m.quitting, true)) {
        return;
    }
    let backend = app.read(cx).backend.clone();
    let Some(daemon) = backend.daemon.clone() else {
        cx.quit();
        return;
    };
    if !daemon.recording_active() {
        cx.quit();
        return;
    }
    // Saving a long recording can take a few seconds: say so instead of going quiet (R4-23).
    push_toast(
        app,
        ToastSpec::new(ToastKind::Info, tr!("Saving the recording\u{2026}"))
            .body(tr!("CaptureMann closes as soon as it is saved.")),
        cx,
    );
    cx.spawn(async move |cx| {
        let _ = backend
            .rt
            .spawn(async move { Backend::shutdown(&daemon).await })
            .await;
        cx.update(|cx| cx.quit());
    })
    .detach();
}

fn open_screen_recording_settings() {
    open_privacy_pane("Privacy_ScreenCapture");
}

/// Opens a System Settings -> Privacy & Security pane (`Privacy_Camera`, `Privacy_Microphone`,
/// `Privacy_ScreenCapture`). macOS only.
pub fn open_privacy_pane(pane: &str) {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .arg(format!(
                "x-apple.systempreferences:com.apple.preference.security?{pane}"
            ))
            .spawn();
    }
    #[cfg(not(target_os = "macos"))]
    let _ = pane;
}

/// CAP-5: the tray submenu and the capture toolbar choose the delay here: both show the new
/// choice at once and it is remembered across restarts (`capture.delay_s`).
pub fn set_capture_delay(app: &Entity<AppModel>, secs: u32, cx: &mut App) {
    let secs = if crate::delay::CHOICES.contains(&secs) {
        secs
    } else {
        0
    };
    app.update(cx, |m, cx| {
        m.capture_delay_s = secs;
        cx.notify();
    });
    if let Some(t) = &app.read(cx).tray {
        t.set_delay(secs);
    }
    set_setting(app, KEY_CAPTURE_DELAY, serde_json::json!(secs), cx);
}

/// Hotkey, tray menu or toolbar button: wait out the chosen delay if one is set (CAP-5), then
/// run the overlay and CAP-4.
pub fn start_capture(app: &Entity<AppModel>, mode: CaptureMode, cx: &mut App) {
    // R19: a window screenshot starts in the window list. Tab there ("Pick on screen") runs the
    // hover mode through `start_hover_capture`, which keeps the CAP-5 delay. A window chosen in
    // the list is captured when Enter is pressed, without a delay.
    if mode == CaptureMode::Window {
        crate::ui::window_picker::open(app, cx);
        return;
    }
    start_hover_capture(app, mode, cx);
}

/// The overlay path of [`start_capture`]: the delay pill if a delay is set (CAP-5), then the
/// overlay and CAP-4.
pub(crate) fn start_hover_capture(app: &Entity<AppModel>, mode: CaptureMode, cx: &mut App) {
    let secs = app.read(cx).capture_delay_s;
    if secs > 0 {
        crate::ui::delay_pill::start(app, mode, secs, cx);
    } else {
        run_capture(app, mode, cx);
    }
}

/// Whether the capture starting now is copied: a launcher action's one-off choice (spent here),
/// else the "copy after capture" setting (CAP-4).
pub(crate) fn take_copy(app: &Entity<AppModel>, cx: &mut App) -> bool {
    app.update(cx, |m, _| {
        m.copy_override.take().unwrap_or(m.copy_after_capture)
    })
}

/// How one capture ended, for [`finish_capture`].
pub(crate) enum CaptureFlow {
    Done(Result<crate::backend::AfterCapture, String>),
    /// Esc in the picker or in the delay: nothing was captured.
    Cancelled,
    /// Tab in the hover mode: the window picker opens instead (R19).
    SwitchToList,
    Overlay(crate::overlay::OverlayError),
    Failed(String),
}

/// The capture itself: the overlay (or the delay's daemon call), then CAP-4.
pub(crate) fn run_capture(app: &Entity<AppModel>, mode: CaptureMode, cx: &mut App) {
    crate::trace::mark("start_capture");
    let (backend, busy) = {
        let m = app.read(cx);
        (m.backend.clone(), m.capture_busy)
    };
    if busy {
        return; // CAP-2: shortcut pressed while the overlay is already open is ignored.
    }
    if backend.daemon.is_none() {
        push_toast(
            app,
            flows::error_toast_live(
                || tr!("Capture unavailable"),
                tr!("The library couldn\u{2019}t be opened, so there is nowhere to save captures."),
            ),
            cx,
        );
        return;
    }
    app.update(cx, |m, _| m.capture_busy = true);
    // Read now, not inside the task: a launcher action's one-off choice is for this capture only.
    let copy = take_copy(app, cx);
    let app2 = app.clone();
    cx.spawn(async move |cx| {
        let b = backend.clone();
        let flow = backend
            .rt
            .spawn(async move {
                match b.overlay.run(mode).await {
                    Ok(OverlayOutcome::Captured { capture_id }) => {
                        let done = b.after_capture(&capture_id, copy).await;
                        Ok(CaptureFlow::Done(done))
                    }
                    Ok(OverlayOutcome::Cancelled) => Ok(CaptureFlow::Cancelled),
                    Ok(OverlayOutcome::SwitchToList) => Ok(CaptureFlow::SwitchToList),
                    Err(e) => Err(e),
                }
            })
            .await;
        let flow = match flow {
            Ok(Ok(flow)) => flow,
            Ok(Err(e)) => CaptureFlow::Overlay(e),
            Err(e) => CaptureFlow::Failed(e.to_string()),
        };
        cx.update(|cx| finish_capture(&app2, flow, cx));
    })
    .detach();
}

/// R19: captures the window the picker chose, by id, through the same path as every screenshot
/// (CAP-4 clipboard and folder copy, the result toast, the recents list). The picker is already
/// closed; a window that is gone by now ends in the error toast like any failed capture.
pub(crate) fn run_window_capture(app: &Entity<AppModel>, window_id: u32, cx: &mut App) {
    let (backend, busy) = {
        let m = app.read(cx);
        (m.backend.clone(), m.capture_busy)
    };
    if busy {
        return; // CAP-2
    }
    // CAP-5: the remembered delay applies to a window chosen from the list too (the pill counts
    // down, the daemon captures that window afterwards, Esc cancels).
    if backend.daemon.is_some() && app.read(cx).capture_delay_s > 0 {
        let secs = app.read(cx).capture_delay_s;
        crate::ui::delay_pill::start_window(app, window_id, secs, cx);
        return;
    }
    let Some(daemon) = backend.daemon.clone() else {
        push_toast(
            app,
            flows::error_toast_live(
                || tr!("Capture unavailable"),
                tr!("The library couldn\u{2019}t be opened, so there is nowhere to save captures."),
            ),
            cx,
        );
        return;
    };
    app.update(cx, |m, _| m.capture_busy = true);
    let copy = take_copy(app, cx);
    let app2 = app.clone();
    cx.spawn(async move |cx| {
        // The picker window and the outline are excluded from capture, but the window server
        // needs a moment to take them off screen and hand focus back.
        cx.background_executor()
            .timer(Duration::from_millis(
                crate::launcher::commands::CAPTURE_DELAY_MS,
            ))
            .await;
        let b = backend.clone();
        let flow = backend
            .rt
            .spawn(async move {
                let params = serde_json::json!({ "target": { "window_id": window_id } });
                match Backend::call_coded(&daemon, "capture_screenshot", params).await {
                    Ok(v) => match v["capture_id"].as_str() {
                        Some(id) => CaptureFlow::Done(b.after_capture(id, copy).await),
                        None => CaptureFlow::Failed(tr!("The capture has no id")),
                    },
                    Err(e) => CaptureFlow::Failed(crate::daemon_text::call_error_text(&e)),
                }
            })
            .await
            .unwrap_or_else(|e| CaptureFlow::Failed(e.to_string()));
        cx.update(|cx| finish_capture(&app2, flow, cx));
    })
    .detach();
}

/// Ends a capture: frees the capture slot, then the result toast and the recents list.
pub(crate) fn finish_capture(app: &Entity<AppModel>, flow: CaptureFlow, cx: &mut App) {
    app.update(cx, |m, _| m.capture_busy = false);
    match flow {
        CaptureFlow::Done(Ok(done)) => {
            let spec = {
                let m = app.read(cx);
                flows::capture_done_toast(&done, m.platform, &m.shortcuts)
            };
            app.update(cx, |m, cx| {
                m.recents.retain(|(id, _)| *id != done.capture_id);
                m.recents
                    .insert(0, (done.capture_id.clone(), done.title.clone()));
                m.recents.truncate(5);
                m.library_epoch += 1;
                cx.notify();
            });
            refresh_tray_recents(app, cx);
            push_toast(app, spec, cx);
            // CAP-4.AC4: the capture is in the library; the folder copy is what failed.
            if let Some(warning) = flows::folder_warning_toast(&done.folder) {
                push_toast(app, warning, cx);
            }
            if matches!(done.copy, CopyResult::Failed(_)) {
                eprintln!("cm-desktop: clipboard copy failed for {}", done.capture_id);
            }
        }
        CaptureFlow::Done(Err(e)) => push_toast(
            app,
            flows::error_toast_live(|| tr!("Captured, but couldn\u{2019}t finish"), e),
            cx,
        ),
        CaptureFlow::Cancelled => {}
        CaptureFlow::SwitchToList => crate::ui::window_picker::open(app, cx),
        CaptureFlow::Overlay(e) => push_toast(app, flows::overlay_error_toast(&e), cx),
        CaptureFlow::Failed(e) => push_toast(
            app,
            flows::error_toast_live(|| tr!("Capture failed"), e),
            cx,
        ),
    }
}

pub fn refresh_tray_recents(app: &Entity<AppModel>, cx: &mut App) {
    let m = app.read(cx);
    if let Some(tray) = &m.tray {
        tray.set_recents(&m.recents);
    }
}

/// CAP-4: asks for the folder of "Also save a PNG to a folder" and stores it; with `enable` the
/// setting is switched on as well (turning it on before any folder was chosen).
pub fn choose_save_folder(app: &Entity<AppModel>, enable: bool, cx: &mut App) {
    let rx = cx.prompt_for_paths(gpui::PathPromptOptions {
        files: false,
        directories: true,
        multiple: false,
        prompt: Some(tr_ctx!("folder dialog", "Choose folder").into()),
    });
    let app2 = app.clone();
    cx.spawn(async move |cx| {
        let Ok(Ok(Some(paths))) = rx.await else {
            return;
        };
        let Some(path) = paths.first().map(|p| p.display().to_string()) else {
            return;
        };
        cx.update(|cx| {
            set_setting(
                &app2,
                crate::settings::KEY_SAVE_FOLDER,
                serde_json::json!(path),
                cx,
            );
            if enable {
                set_setting(
                    &app2,
                    crate::settings::KEY_SAVE_FOLDER_ENABLED,
                    serde_json::json!(true),
                    cx,
                );
            }
        });
    })
    .detach();
}

/// Persists one setting through the daemon (as the user) and applies it.
pub fn set_setting(
    app: &Entity<AppModel>,
    key: &'static str,
    value: serde_json::Value,
    cx: &mut App,
) {
    let backend = app.read(cx).backend.clone();
    let app2 = app.clone();
    cx.spawn(async move |cx| {
        let Some(daemon) = backend.daemon.clone() else {
            return;
        };
        let v = value.clone();
        let res = backend
            .rt
            .spawn(async move {
                Backend::call(
                    &daemon,
                    "set_settings",
                    serde_json::json!({ "values": { key: v } }),
                )
                .await
            })
            .await;
        cx.update(|cx| match res {
            Ok(Ok(_)) => {
                app2.update(cx, |m, cx| {
                    match key {
                        KEY_THEME => {
                            if let Some(p) = value.as_str().and_then(ThemePreference::parse) {
                                m.theme_pref = p;
                            }
                        }
                        KEY_COPY_AFTER_CAPTURE => {
                            m.copy_after_capture = value.as_bool().unwrap_or(true)
                        }
                        KEY_LANGUAGE => {
                            m.language = cm_i18n::Lang::from_setting(value.as_str());
                        }
                        _ => {}
                    }
                    cx.notify();
                });
                apply_theme(&app2, cx);
                if key == KEY_LANGUAGE {
                    apply_language(&app2, cx);
                }
            }
            Ok(Err(e)) => push_toast(
                &app2,
                flows::error_toast_live(|| tr!("Couldn\u{2019}t save the setting"), e),
                cx,
            ),
            Err(e) => push_toast(
                &app2,
                flows::error_toast_live(|| tr!("Couldn\u{2019}t save the setting"), e.to_string()),
                cx,
            ),
        });
    })
    .detach();
}

/// Applies one message from the daemon-side `UiConfirmer`.
pub fn handle_confirm_msg(app: &Entity<AppModel>, msg: ConfirmUiMsg, cx: &mut App) {
    match msg {
        ConfirmUiMsg::Show { model, reply } => open_confirm(app, model, reply, cx),
        ConfirmUiMsg::Close { id } => {
            let found = app.update(cx, |m, cx| {
                let i = m.confirm_windows.iter().position(|(x, _)| *x == id);
                let h = i.map(|i| m.confirm_windows.remove(i).1);
                cx.notify(); // the main window's scrim follows the dialogs
                h
            });
            if let Some(h) = found {
                let _ = h.update(cx, |_, window, _| window.remove_window());
            }
        }
    }
}

pub(crate) fn open_confirm(
    app: &Entity<AppModel>,
    model: DialogModel,
    reply: tokio::sync::oneshot::Sender<cm_daemon::Decision>,
    cx: &mut App,
) {
    // On the display with the pointer, centered on its visible part, never taller than it.
    let display = crate::ui::recording::pointer_display(cx);
    let visible = display.map(|(_, r)| crate::ui::recording::visible_part(&r));
    let wanted = crate::confirm::dialog_height(&model);
    let height = visible.map_or(wanted, |v| {
        crate::confirm::fit_dialog_height(wanted, v.h as f32)
    });
    let (win_w, win_h) = (crate::confirm::DIALOG_WIDTH, height);
    let bounds = match (display, visible) {
        (Some((_, d)), Some(v)) => {
            let g = crate::recording::geometry::Rect::new(
                v.x + (v.w - f64::from(win_w)) / 2.0,
                v.y + (v.h - f64::from(win_h)) / 2.0,
                f64::from(win_w),
                f64::from(win_h),
            )
            .relative_to(&d);
            Bounds {
                origin: point(px(g.x as f32), px(g.y as f32)),
                size: size(px(win_w), px(win_h)),
            }
        }
        _ => Bounds::centered(None, size(px(win_w), px(win_h)), cx),
    };
    let id = model.id.clone();
    let opened = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: None,
            focus: true,
            show: true,
            kind: WindowKind::PopUp,
            is_movable: false,
            is_resizable: false,
            is_minimizable: false,
            display_id: display.map(|(id, _)| id),
            ..Default::default()
        },
        move |window, cx| {
            crate::platform_window::exclude_from_capture(window);
            cx.new(|cx| ConfirmDialog::new(model, reply, window, cx))
        },
    );
    if let Ok(handle) = opened {
        app.update(cx, |m, cx| {
            m.confirm_windows.push((id, handle));
            cx.notify(); // the main window's scrim follows the dialogs
        });
        cx.activate(true);
        raise_centered(handle, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permissions::{FakeProvider, Id, Os, State};

    const MISSING: [(Id, State); 4] = [
        (Id::ScreenRecording, State::NotGranted),
        (Id::Microphone, State::NotDetermined),
        (Id::Camera, State::Denied),
        (Id::InputMonitoring, State::NotGranted),
    ];

    fn show(raw: &[(Id, State)], at_start: bool, onboarded: bool) -> bool {
        let p = FakeProvider::new(Os::Mac, raw);
        permissions::show_first_run(&permissions::probe(&p, Some(at_start)), onboarded)
    }

    /// PRM-1.AC12: the startup route is the setup screen when the rule says show, else the library.
    #[test]
    fn startup_route_follows_the_first_run_rule() {
        let due = show(&MISSING, false, true);
        assert_eq!(
            startup_route(due, false, None),
            Some(Route::Permissions),
            "required missing: setup screen even when onboarded"
        );
        let granted = [
            (Id::ScreenRecording, State::Granted),
            (Id::Microphone, State::Granted),
            (Id::Camera, State::Granted),
            (Id::InputMonitoring, State::Granted),
        ];
        let due = show(&granted, true, false);
        assert_eq!(startup_route(due, false, None), Some(Route::Library));
        // Optional missing: only on the first launch.
        let mut optional = granted;
        optional[2].1 = State::Denied;
        assert_eq!(
            startup_route(show(&optional, true, false), false, None),
            Some(Route::Permissions)
        );
        assert_eq!(
            startup_route(show(&optional, true, true), false, None),
            Some(Route::Library)
        );
    }

    /// PRM-1.AC12: `--background` opens no window, except when the setup screen is due.
    #[test]
    fn background_opens_a_window_only_for_the_setup_screen() {
        assert_eq!(startup_route(false, true, None), None);
        assert_eq!(startup_route(true, true, None), Some(Route::Permissions));
    }

    /// R1-6: the rescan runs at most every 10 s when the app comes to the front, never twice at
    /// once, and always when asked to.
    /// CAP-4.AC1 / LIB-1: an action the daemon lacks reads as "not available yet".
    #[test]
    fn missing_actions_are_not_failures() {
        use crate::backend::CallError;
        let ni = CallError {
            code: Some(cm_core::ErrorCode::NotImplemented),
            permission: None,
            message: "`delete_capture` is not implemented yet".into(),
            data: None,
        };
        assert!(is_unavailable(&ni));
        assert_eq!(
            unavailable_or_error(&tr!("x"), &ni).title,
            "Not available yet"
        );
        let nf = CallError::message("Method not found: rename_capture");
        assert!(is_unavailable(&nf));
        let real = CallError::message("disk full");
        assert!(!is_unavailable(&real));
        assert_eq!(
            unavailable_or_error(&tr!("Couldn\u{2019}t delete"), &real).title,
            "Couldn\u{2019}t delete"
        );
    }

    /// R4-23: the toast window sits bottom-right above the Dock (the agent toast joins it, R17).
    #[test]
    fn toast_windows_keep_their_corners() {
        let (x, y) = toast_origin((1440., 900.), 100.);
        assert_eq!((x, y), (1440. - 440. - 16., 900. - 100. - 48.));
        // A taller stack grows upward, the right edge stays.
        let (x2, y2) = toast_origin((1440., 900.), 200.);
        assert_eq!((x2, y2), (x, y - 100.));
    }

    #[test]
    fn r1_6_a_missing_bundle_is_recognized() {
        assert!(looks_missing("no bundle at `/lib/Shot.cmshot`"));
        assert!(looks_missing("no capture `shot_1`"));
        assert!(!looks_missing("The library is not available."));
        assert!(!looks_missing("permission denied"));
    }

    #[test]
    fn r1_6_rescan_is_throttled() {
        let t = Instant::now();
        assert!(rescan_due(None, false, t, false));
        assert!(!rescan_due(
            Some(t),
            false,
            t + Duration::from_secs(9),
            false
        ));
        assert!(rescan_due(
            Some(t),
            false,
            t + Duration::from_secs(10),
            false
        ));
        assert!(!rescan_due(None, true, t, true), "never two at once");
        assert!(rescan_due(Some(t), false, t, true), "forced");
    }

    #[test]
    fn explicit_debug_route_wins() {
        assert_eq!(
            startup_route(true, false, Some(Route::Agents)),
            Some(Route::Agents)
        );
    }

    /// Every "Open Permissions…" entry point lands on Settings -> Permissions in the main window.
    #[test]
    fn entry_points_map_to_the_permissions_page() {
        assert_eq!(
            permissions_entry(),
            (Route::Settings, SettingsSection::Permissions)
        );
    }

    /// Auto-advance happens only once everything is plainly granted; navigating away records the
    /// onboarding unless Screen Recording is still missing.
    #[test]
    fn auto_advance_and_dismiss_by_navigation() {
        let p = FakeProvider::new(Os::Mac, &MISSING);
        let s = permissions::probe(&p, Some(false));
        assert!(!s.all_granted() && !permissions::closing_marks_onboarded(&s));
        p.set(Id::ScreenRecording, State::Granted);
        let s = permissions::probe(&p, Some(true));
        assert!(
            !s.all_granted(),
            "optional ones missing: stay on the screen"
        );
        assert!(permissions::closing_marks_onboarded(&s));
        for id in [Id::Microphone, Id::Camera, Id::InputMonitoring] {
            p.set(id, State::Granted);
        }
        assert!(permissions::probe(&p, Some(true)).all_granted());
        // Restart required is not "all set" and does not mark onboarded.
        let r = permissions::probe(&p, Some(false));
        assert!(!r.all_granted() && !permissions::closing_marks_onboarded(&r));
    }
}
