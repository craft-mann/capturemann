//! Application state and the command dispatcher.
//!
//! `AppModel` is plain shared state (an entity so views can observe it). Everything that
//! opens windows or talks to the daemon is a free function taking `&Entity<AppModel>` and
//! `&mut App`, so no window is ever opened while the model is borrowed (a new window renders
//! immediately and reads the model).

use cm_i18n::{tr, tr_ctx};
use gpui::{
    point, px, size, App, AppContext as _, Bounds, Entity, Global, Task, WindowBounds,
    WindowHandle, WindowKind, WindowOptions,
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

mod capture_flow;
mod commands;
mod library_ops;
mod permission_flow;
mod routing;
mod system;
mod toasts;
mod watch;

pub use capture_flow::*;
pub use commands::*;
pub use library_ops::*;
pub use permission_flow::*;
pub use routing::*;
pub use system::*;
pub use toasts::*;
pub use watch::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    /// All captures: the mixed library grid (nav item "All captures", `Filter::All`) and the
    /// default landing page (OD-16).
    Library,
    /// The screenshots library (nav item "Screenshots", the grid filtered to screenshots).
    Screenshots,
    /// The recordings library (the grid filtered to recordings).
    Recordings,
    /// The audio library (nav item "Audio", the grid filtered to `kind: "audio"`, VOC-1.AC10).
    Audio,
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
    /// Wayland: the keys the desktop's GlobalShortcuts portal chose, as it describes them.
    pub portal_triggers: crate::platform_hotkeys::PortalTriggers,
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
    /// The library folder change in progress (Settings, General; LIB-2).
    pub library_move: crate::library_move::Phase,
    /// The AI page: speech models, downloads the user started, the notice and the switches.
    pub ai: crate::ai::models::ModelsVm,
    /// The built-in text models of the AI page (AI-7): the list by memory tier and its downloads.
    pub builtin: crate::ai::builtin::BuiltInVm,
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
    /// The Studio window exists (Wayland: toasts are banners in it then).
    pub studio_open: bool,
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

    /// Whether the toasts are drawn inside the Studio window instead of their own window
    /// (`toasts::toasts_in_window`).
    pub fn toasts_in_studio(&self) -> bool {
        toasts::toasts_in_window(
            crate::platform_window::toasts_belong_in_window(),
            self.studio_open,
            !self.toasts.is_empty(),
            crate::capture_cloak::is_hidden(),
        )
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
            portal_triggers: Vec::new(),
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
            library_move: crate::library_move::Phase::default(),
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
                languages: crate::ai::models::languages_from_value(
                    settings.get(crate::ui::ai::KEY_LANGUAGES),
                ),
                voice: crate::ai::voice::VoiceVm::from_settings(&settings),
                ..Default::default()
            },
            builtin: crate::ai::builtin::BuiltInVm::default(),
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
            studio_open: false,
            toast_window: None,
            toast_fit: 0.,
            confirm_windows: Vec::new(),
            toast_timer: None,
        }
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
