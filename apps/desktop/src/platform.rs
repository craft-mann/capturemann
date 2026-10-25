//! OS integration that must run on the main thread: global hotkeys and the tray icon
//! (CAP-2). Menus are native (muda); nothing here is custom-drawn (DESIGN section 3).

use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::GlobalHotKeyManager;
use std::collections::HashMap;
use std::str::FromStr;
use tray_icon::menu::{CheckMenuItem, Menu, MenuId, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

use crate::shortcuts::{Chord, ShortcutAction, ShortcutMap};
use cm_i18n::{tr, tr_ctx};

/// A chord as the OS-level hotkey type.
pub fn chord_to_hotkey(c: &Chord) -> Result<HotKey, String> {
    let mut mods = Modifiers::empty();
    if c.ctrl {
        mods |= Modifiers::CONTROL;
    }
    if c.alt {
        mods |= Modifiers::ALT;
    }
    if c.shift {
        mods |= Modifiers::SHIFT;
    }
    if c.meta {
        mods |= Modifiers::META;
    }
    let code_name = if c.key.len() == 1 {
        let ch = c.key.chars().next().unwrap_or('a');
        if ch.is_ascii_digit() {
            format!("Digit{ch}") // i18n-ignore: a key code name
        } else {
            format!("Key{}", ch.to_ascii_uppercase()) // i18n-ignore: a key code name
        }
    } else if c.key == "space" {
        "Space".to_owned()
    } else {
        // f1..f24
        c.key.to_ascii_uppercase()
    };
    let code =
        Code::from_str(&code_name).map_err(|_| tr!("unsupported key `{key}`", key = c.key))?;
    Ok(HotKey::new(Some(mods), code))
}

/// Keeps the OS hotkey registrations alive (a GPUI global) and lets Settings rebind them.
pub struct HotkeyHolder(pub GlobalHotKeyManager);
impl gpui::Global for HotkeyHolder {}

/// Moves one registration from `old` to `new`. On failure (another app owns `new`) the old
/// registration is restored, and the error says so. Returns the ids `(old, new)` of the hotkeys.
pub fn rebind_hotkey(
    manager: &GlobalHotKeyManager,
    old: &Chord,
    new: &Chord,
) -> Result<(u32, u32), String> {
    let old_hk = chord_to_hotkey(old)?;
    let new_hk = chord_to_hotkey(new)?;
    if old_hk.id() == new_hk.id() {
        return Ok((old_hk.id(), new_hk.id()));
    }
    // Unregister first: a Carbon hot key is per combination, and a failed registration of the new
    // one must leave the old working.
    let _ = manager.unregister(old_hk);
    match manager.register(new_hk) {
        Ok(()) => Ok((old_hk.id(), new_hk.id())),
        Err(e) => {
            let _ = manager.register(old_hk);
            Err(e.to_string())
        }
    }
}

/// Per-shortcut OS registration outcome.
pub type Registration = Vec<(ShortcutAction, Result<(), String>)>;

/// Registers every available shortcut. A failure (another app owns the combination) is
/// recorded per shortcut and does not stop the others (CAP-2.AC3).
pub fn register_shortcuts(
    manager: &GlobalHotKeyManager,
    map: &ShortcutMap,
) -> (HashMap<u32, ShortcutAction>, Registration) {
    let mut ids = HashMap::new();
    let mut status = Vec::new();
    for (action, chord) in map.iter() {
        if !action.is_available() {
            status.push((action, Err(tr!("Not available yet"))));
            continue;
        }
        let result = chord_to_hotkey(chord).and_then(|hk| {
            manager
                .register(hk)
                .map(|()| {
                    ids.insert(hk.id(), action);
                })
                .map_err(|e| e.to_string())
        });
        status.push((action, result));
    }
    (ids, status)
}

/// Menu item ids of the tray menu (CAP-2).
pub mod ids {
    pub const CAPTURE_REGION: &str = "capture_region";
    pub const CAPTURE_WINDOW: &str = "capture_window";
    pub const CAPTURE_DISPLAY: &str = "capture_display";
    pub const OPEN_LIBRARY: &str = "open_library";
    pub const OPEN_LAUNCHER: &str = "open_launcher";
    pub const SETTINGS: &str = "settings";
    pub const PERMISSIONS: &str = "permissions";
    pub const QUIT: &str = "quit";
    pub const RECORD: &str = "record";
    pub const REC_STOP: &str = "rec_stop";
    pub const REC_PAUSE: &str = "rec_pause";
}

/// What the tray icon shows. `Recording` (red dot, not a template image) is on from the first
/// countdown until the take ends (REC-5.AC7). `Paused` (the frame with two pause bars) is shown
/// while a take is paused or about to resume; `Attention` (an amber "!") says something needs the
/// user: a failed upload, a needed reconnect (redesign, `design/icons/CLAUDE.md` section 8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayState {
    Idle,
    Recording,
    Paused,
    Attention,
}

impl TrayState {
    /// A take is running or paused (the menu then leads with Stop).
    pub fn take_on_screen(self) -> bool {
        matches!(self, TrayState::Recording | TrayState::Paused)
    }
}

const TRAY_TEMPLATE_2X: &[u8] = include_bytes!("../assets/tray/TrayTemplate@2x.png");
const TRAY_RECORDING_2X: &[u8] = include_bytes!("../assets/tray/tray-recording-on-dark@2x.png");
const TRAY_PAUSED_2X: &[u8] = include_bytes!("../assets/tray/TrayPausedTemplate@2x.png");
const TRAY_ATTENTION_2X: &[u8] = include_bytes!("../assets/tray/TrayAttentionTemplate@2x.png");

/// Embedded PNG for the macOS menu bar: `TrayTemplate` (a template image, so macOS tints it
/// for the menu bar) while idle; `tray-recording-on-dark` (not a template, the red must show)
/// while recording. Paused and Attention are template images too (the kit says to use the
/// template version on macOS).
pub fn mac_tray_png(state: TrayState) -> (&'static [u8], bool) {
    match state {
        TrayState::Idle => (TRAY_TEMPLATE_2X, true),
        TrayState::Recording => (TRAY_RECORDING_2X, false),
        TrayState::Paused => (TRAY_PAUSED_2X, true),
        TrayState::Attention => (TRAY_ATTENTION_2X, true),
    }
}

/// File name of the Windows notification-area `.ico` for a taskbar theme and state.
pub fn windows_tray_ico_name(light_taskbar: bool, state: TrayState) -> &'static str {
    match (state, light_taskbar) {
        (TrayState::Idle, false) => "tray-dark.ico",
        (TrayState::Idle, true) => "tray-light.ico",
        (TrayState::Recording, false) => "tray-recording-on-dark.ico",
        (TrayState::Recording, true) => "tray-recording-on-light.ico",
        (TrayState::Paused, false) => "tray-paused-on-dark.ico",
        (TrayState::Paused, true) => "tray-paused-on-light.ico",
        (TrayState::Attention, false) => "tray-attention-on-dark.ico",
        (TrayState::Attention, true) => "tray-attention-on-light.ico",
    }
}

#[cfg(target_os = "windows")]
fn windows_tray_ico(light_taskbar: bool, state: TrayState) -> &'static [u8] {
    match windows_tray_ico_name(light_taskbar, state) {
        "tray-light.ico" => include_bytes!("../assets/tray/tray-light.ico"),
        "tray-recording-on-dark.ico" => include_bytes!("../assets/tray/tray-recording-on-dark.ico"),
        "tray-recording-on-light.ico" => {
            include_bytes!("../assets/tray/tray-recording-on-light.ico")
        }
        "tray-paused-on-dark.ico" => include_bytes!("../assets/tray/tray-paused-on-dark.ico"),
        "tray-paused-on-light.ico" => include_bytes!("../assets/tray/tray-paused-on-light.ico"),
        "tray-attention-on-dark.ico" => {
            include_bytes!("../assets/tray/tray-attention-on-dark.ico")
        }
        "tray-attention-on-light.ico" => {
            include_bytes!("../assets/tray/tray-attention-on-light.ico")
        }
        _ => include_bytes!("../assets/tray/tray-dark.ico"),
    }
}

/// Decodes embedded PNG/ICO bytes (the largest ICO frame) to the tray icon type.
fn decode_icon(bytes: &[u8]) -> Result<Icon, String> {
    let img = image::load_from_memory(bytes)
        .map_err(|e| e.to_string())?
        .into_rgba8();
    let (w, h) = img.dimensions();
    Icon::from_rgba(img.into_raw(), w, h).map_err(|e| e.to_string())
}

/// The tray icon for a state on this platform and whether macOS should treat it as a
/// template image. Windows assumes a dark taskbar (the Windows default).
fn tray_icon_for(state: TrayState) -> Result<(Icon, bool), String> {
    #[cfg(target_os = "windows")]
    {
        decode_icon(windows_tray_ico(false, state)).map(|i| (i, false))
    }
    #[cfg(not(target_os = "windows"))]
    {
        let (png, template) = mac_tray_png(state);
        decode_icon(png).map(|i| (i, template))
    }
}

/// 512 px macOS app icon (Apple grid with shadow) from the design kit.
#[cfg(target_os = "macos")]
const APP_ICON_PNG: &[u8] = include_bytes!("../assets/brand/app-icon-512.png");

/// True when running from `CaptureMann.app/Contents/MacOS`.
#[cfg(target_os = "macos")]
fn bundled() -> bool {
    std::env::current_exe().is_ok_and(|p| p.to_string_lossy().contains(".app/Contents/MacOS/"))
}

/// Sets the Dock icon at run time for an unbundled run when `CM_DEV_DOCK_ICON` is set (a
/// bundled app gets it from `AppIcon.icns` through `Info.plist`; doing it at run time costs
/// about 15 MB of RSS, NFR-5). GPUI 0.3.9 has no API for this, so it goes through AppKit.
/// Must run on the main thread; a no-op elsewhere.
#[cfg(target_os = "macos")]
pub fn set_dock_icon() {
    use objc2::{AnyThread, MainThreadMarker};
    use objc2_app_kit::{NSApplication, NSImage};
    use objc2_foundation::NSData;
    // A bundled app already has its icon from `Info.plist`; setting it again costs ~15 MB of
    // resident memory (measured), and so does it in an unbundled dev run, where it is opt-in.
    if bundled() || std::env::var_os("CM_DEV_DOCK_ICON").is_none() {
        return;
    }
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let data = NSData::with_bytes(APP_ICON_PNG);
    let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) else {
        return;
    };
    // SAFETY: called on the main thread (`mtm`) with a valid, retained NSImage.
    unsafe { NSApplication::sharedApplication(mtm).setApplicationIconImage(Some(&image)) };
}

#[cfg(not(target_os = "macos"))]
pub fn set_dock_icon() {}

/// The ids of the Record / Pause / Stop group in menu order (CAP-2.AC4): while recording, Stop is
/// first and sits at the top of the menu, so it is the item under the pointer.
pub fn recording_group(recording: bool) -> [&'static str; 3] {
    if recording {
        [ids::REC_STOP, ids::REC_PAUSE, ids::RECORD]
    } else {
        [ids::RECORD, ids::REC_PAUSE, ids::REC_STOP]
    }
}

/// Where the Record / Pause / Stop group starts: after the three capture items and the delay
/// submenu when idle, at the top while recording.
pub fn recording_group_start(recording: bool) -> usize {
    if recording {
        0
    } else {
        4
    }
}

/// The text of a tray menu item in the current UI language. One place for the strings, so
/// building the menu and [`TrayHandle::relabel`] (a live language switch) cannot disagree.
/// `Pause recording` is set by [`TrayHandle::apply_plan`] (it becomes `Resume recording`).
pub fn menu_text(id: &str) -> String {
    match id {
        ids::CAPTURE_REGION => tr!("Capture region"),
        ids::CAPTURE_WINDOW => tr!("Capture window"),
        ids::CAPTURE_DISPLAY => tr!("Capture display"),
        ids::RECORD => tr!("Record\u{2026}"),
        ids::REC_PAUSE => tr!("Pause recording"),
        ids::REC_STOP => tr!("Stop recording"),
        ids::OPEN_LIBRARY => tr!("Open library"),
        ids::OPEN_LAUNCHER => tr!("Open launcher"),
        ids::SETTINGS => tr_ctx!("tray menu", "Settings"),
        ids::PERMISSIONS => tr!("Permissions\u{2026}"),
        ids::QUIT => tr!("Quit CaptureMann"),
        "recent" => tr!("Recent captures"),
        "delay" => tr!("Capture delay"),
        _ => id.to_owned(),
    }
}

pub struct TrayHandle {
    menu: Menu,
    tray: TrayIcon,
    recent: Submenu,
    /// "Capture delay" choices (CAP-5), one check mark among them.
    delays: Vec<(u32, CheckMenuItem)>,
    delay_menu: Submenu,
    record: MenuItem,
    stop: MenuItem,
    pause: MenuItem,
    /// The other plain items, by id, for a live language switch.
    plain: Vec<(&'static str, MenuItem)>,
    state: std::cell::Cell<TrayState>,
}

impl TrayHandle {
    /// Switches the icon between idle (template on macOS) and recording (red, non-template).
    pub fn set_state(&self, state: TrayState) {
        match tray_icon_for(state) {
            Ok((icon, template)) => {
                // Template icons exist on macOS only (tray-icon has no such method elsewhere).
                #[cfg(target_os = "macos")]
                let res = if template {
                    self.tray.set_icon_templated(Some(icon))
                } else {
                    self.tray.set_icon(Some(icon))
                };
                #[cfg(not(target_os = "macos"))]
                let res = {
                    let _ = template;
                    self.tray.set_icon(Some(icon))
                };
                if let Err(e) = res {
                    eprintln!("cm-desktop: could not switch the tray icon: {e}");
                }
            }
            Err(e) => eprintln!("cm-desktop: could not decode the tray icon: {e}"),
        }
    }

    /// Applies the recording state (REC-5.AC7): the icon only when the state changes, the tooltip
    /// with the elapsed time, and the Record / Stop / Pause menu items.
    pub fn apply_plan(&self, plan: &crate::recording::tray::TrayPlan) {
        if self.state.get() != plan.state {
            self.state.set(plan.state);
            self.set_state(plan.state);
            self.reorder(plan.state.take_on_screen());
        }
        let _ = self.tray.set_tooltip(Some(plan.tooltip.clone()));
        self.record.set_enabled(plan.record_enabled);
        self.stop.set_enabled(plan.stop_enabled);
        self.pause.set_enabled(plan.pause_enabled);
        self.pause.set_text(&plan.pause_label);
    }

    /// Moves the Record / Pause / Stop group: Stop first while recording (CAP-2.AC4).
    fn reorder(&self, recording: bool) {
        let item = |id: &str| -> &MenuItem {
            match id {
                ids::REC_STOP => &self.stop,
                ids::REC_PAUSE => &self.pause,
                _ => &self.record,
            }
        };
        for id in [ids::RECORD, ids::REC_PAUSE, ids::REC_STOP] {
            let _ = self.menu.remove(item(id));
        }
        let start = recording_group_start(recording);
        for (i, id) in recording_group(recording).into_iter().enumerate() {
            let _ = self.menu.insert(item(id), start + i);
        }
    }

    /// Shows which capture delay is selected (CAP-5); the menu toggles a check by itself on a
    /// click, so this also puts the mark back where the model says it is.
    pub fn set_delay(&self, secs: u32) {
        for (s, item) in &self.delays {
            item.set_checked(*s == secs);
        }
    }

    /// Rewrites every menu text in the current UI language (a live language switch): the plain
    /// items, the submenu titles, the delay choices and the Record / Stop labels. The Pause label
    /// and the tooltip come from the next [`TrayHandle::apply_plan`].
    pub fn relabel(&self, recents: &[(String, String)]) {
        for (id, item) in &self.plain {
            item.set_text(menu_text(id));
        }
        self.record.set_text(menu_text(ids::RECORD));
        self.stop.set_text(menu_text(ids::REC_STOP));
        self.pause.set_text(menu_text(ids::REC_PAUSE));
        self.recent.set_text(menu_text("recent"));
        self.delay_menu.set_text(menu_text("delay"));
        for (secs, item) in &self.delays {
            item.set_text(crate::delay::choice_label(*secs));
        }
        self.set_recents(recents);
    }

    /// Replaces the "Recent captures" entries (last 5, CAP-2).
    pub fn set_recents(&self, recents: &[(String, String)]) {
        while !self.recent.items().is_empty() {
            self.recent.remove_at(0);
        }
        if recents.is_empty() {
            let none = MenuItem::new(tr!("No captures yet"), false, None);
            let _ = self.recent.append(&none);
            return;
        }
        for (id, title) in recents.iter().take(5) {
            let item = MenuItem::with_id(MenuId::new(format!("recent:{id}")), title, true, None);
            let _ = self.recent.append(&item);
        }
    }
}

/// Builds the tray icon and its native menu. Must run on the main thread after the
/// application started.
pub fn build_tray() -> Result<TrayHandle, String> {
    let menu = Menu::new();
    let item =
        |id: &str, enabled: bool| MenuItem::with_id(MenuId::new(id), menu_text(id), enabled, None);
    let record = item(ids::RECORD, true);
    let pause = item(ids::REC_PAUSE, false);
    let stop = item(ids::REC_STOP, false);
    let recent = Submenu::with_id(MenuId::new("recent"), menu_text("recent"), true);
    let none = MenuItem::new(tr!("No captures yet"), false, None);
    recent.append(&none).map_err(|e| e.to_string())?;
    let delay_menu = Submenu::with_id(MenuId::new("delay"), menu_text("delay"), true);
    let plain: Vec<(&'static str, MenuItem)> = [
        ids::CAPTURE_REGION,
        ids::CAPTURE_WINDOW,
        ids::CAPTURE_DISPLAY,
        ids::OPEN_LIBRARY,
        ids::OPEN_LAUNCHER,
        ids::SETTINGS,
        ids::PERMISSIONS,
        ids::QUIT,
    ]
    .into_iter()
    .map(|id| (id, item(id, true)))
    .collect();
    let plain_item = |id: &str| -> &MenuItem {
        plain
            .iter()
            .find(|(i, _)| *i == id)
            .map(|(_, m)| m)
            .expect("every plain id is in the list above")
    };
    let mut delays = Vec::new();
    for secs in crate::delay::CHOICES {
        let check = CheckMenuItem::with_id(
            MenuId::new(crate::delay::menu_id(secs)),
            crate::delay::choice_label(secs),
            true,
            secs == 0,
            None,
        );
        delay_menu.append(&check).map_err(|e| e.to_string())?;
        delays.push((secs, check));
    }
    menu.append_items(&[
        plain_item(ids::CAPTURE_REGION),
        plain_item(ids::CAPTURE_WINDOW),
        plain_item(ids::CAPTURE_DISPLAY),
        &delay_menu,
        &record,
        &pause,
        &stop,
        &PredefinedMenuItem::separator(),
        plain_item(ids::OPEN_LIBRARY),
        plain_item(ids::OPEN_LAUNCHER),
        &recent,
        &PredefinedMenuItem::separator(),
        plain_item(ids::SETTINGS),
        plain_item(ids::PERMISSIONS),
        &PredefinedMenuItem::separator(),
        plain_item(ids::QUIT),
    ])
    .map_err(|e| e.to_string())?;
    let (icon, template) = tray_icon_for(TrayState::Idle)?;
    let builder = TrayIconBuilder::new()
        .with_menu(Box::new(menu.clone()))
        .with_tooltip("CaptureMann");
    #[cfg(target_os = "macos")]
    let builder = if template {
        builder.with_icon_templated(icon)
    } else {
        builder.with_icon(icon)
    };
    #[cfg(not(target_os = "macos"))]
    let builder = {
        let _ = template;
        builder.with_icon(icon)
    };
    let tray = builder.build().map_err(|e| e.to_string())?;
    Ok(TrayHandle {
        menu,
        tray,
        recent,
        delays,
        delay_menu,
        plain,
        record,
        stop,
        pause,
        state: std::cell::Cell::new(TrayState::Idle),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shortcuts::Platform;

    /// CAP-2: every default chord converts to an OS hotkey.
    #[test]
    fn default_chords_convert() {
        for p in [Platform::Mac, Platform::Windows] {
            for (a, c) in ShortcutMap::defaults(p).iter() {
                assert!(chord_to_hotkey(c).is_ok(), "{a:?}");
            }
        }
    }

    #[test]
    fn hotkey_ids_are_distinct_per_action() {
        let map = ShortcutMap::defaults(Platform::Mac);
        let mut seen = std::collections::HashSet::new();
        for (_, c) in map.iter() {
            assert!(seen.insert(chord_to_hotkey(c).unwrap().id()));
        }
    }

    /// CAP-2.AC4: while recording the menu starts with Stop; idle it keeps the usual order.
    #[test]
    fn cap2_ac4_stop_is_first_while_recording() {
        assert_eq!(recording_group(true)[0], ids::REC_STOP);
        assert_eq!(recording_group_start(true), 0);
        assert_eq!(
            recording_group(false),
            [ids::RECORD, ids::REC_PAUSE, ids::REC_STOP]
        );
        // Capture region, window, display and the delay submenu come first (CAP-5).
        assert_eq!(recording_group_start(false), 4);
        let mut a = recording_group(true);
        let mut b = recording_group(false);
        a.sort_unstable();
        b.sort_unstable();
        assert_eq!(a, b, "the same three items either way");
    }

    #[test]
    fn function_keys_and_space() {
        assert!(chord_to_hotkey(&Chord::parse("ctrl+f5").unwrap()).is_ok());
        assert!(chord_to_hotkey(&Chord::parse("alt+space").unwrap()).is_ok());
    }

    /// design/CLAUDE.md section 7: the idle menu bar icon is a template image, the recording
    /// one is not (the red must show).
    #[test]
    fn mac_tray_icons_follow_the_kit() {
        let (idle, idle_template) = mac_tray_png(TrayState::Idle);
        let (rec, rec_template) = mac_tray_png(TrayState::Recording);
        assert!(idle_template && !rec_template);
        let idle = image::load_from_memory(idle).unwrap().into_rgba8();
        let rec = image::load_from_memory(rec).unwrap().into_rgba8();
        assert_eq!(idle.dimensions(), (32, 32));
        assert_eq!(rec.dimensions(), (32, 32));
        // A template image carries only alpha: every visible pixel is one gray.
        assert!(idle
            .pixels()
            .filter(|p| p[3] > 0)
            .all(|p| p[0] == p[1] && p[1] == p[2]));
        // The recording icon has a red pixel.
        assert!(rec
            .pixels()
            .any(|p| p[3] > 200 && p[0] > 200 && p[1] < 120 && p[2] < 100));
        assert!(decode_icon(TRAY_TEMPLATE_2X).is_ok());
    }

    /// The two new states are template images on macOS (32 px at 2x) and distinct from Idle.
    #[test]
    fn mac_tray_paused_and_attention_are_templates() {
        let idle = mac_tray_png(TrayState::Idle).0;
        for state in [TrayState::Paused, TrayState::Attention] {
            let (png, template) = mac_tray_png(state);
            assert!(template, "{state:?}");
            assert_ne!(png, idle, "{state:?}");
            let img = image::load_from_memory(png).unwrap().into_rgba8();
            assert_eq!(img.dimensions(), (32, 32), "{state:?}");
            assert!(img
                .pixels()
                .filter(|p| p[3] > 0)
                .all(|p| p[0] == p[1] && p[1] == p[2]));
        }
        assert_ne!(
            mac_tray_png(TrayState::Paused).0,
            mac_tray_png(TrayState::Attention).0
        );
    }

    #[test]
    fn windows_tray_new_states_name_their_icons() {
        assert_eq!(
            windows_tray_ico_name(false, TrayState::Paused),
            "tray-paused-on-dark.ico"
        );
        assert_eq!(
            windows_tray_ico_name(true, TrayState::Paused),
            "tray-paused-on-light.ico"
        );
        assert_eq!(
            windows_tray_ico_name(false, TrayState::Attention),
            "tray-attention-on-dark.ico"
        );
        assert_eq!(
            windows_tray_ico_name(true, TrayState::Attention),
            "tray-attention-on-light.ico"
        );
    }

    #[test]
    fn windows_tray_variants() {
        assert_eq!(
            windows_tray_ico_name(false, TrayState::Idle),
            "tray-dark.ico"
        );
        assert_eq!(
            windows_tray_ico_name(true, TrayState::Idle),
            "tray-light.ico"
        );
        assert_eq!(
            windows_tray_ico_name(false, TrayState::Recording),
            "tray-recording-on-dark.ico"
        );
        assert_eq!(
            windows_tray_ico_name(true, TrayState::Recording),
            "tray-recording-on-light.ico"
        );
        // Every variant is a valid ICO that decodes.
        for name in [
            "tray-dark.ico",
            "tray-light.ico",
            "tray-recording-on-dark.ico",
            "tray-recording-on-light.ico",
            "tray-paused-on-dark.ico",
            "tray-paused-on-light.ico",
            "tray-attention-on-dark.ico",
            "tray-attention-on-light.ico",
        ] {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("assets/tray")
                .join(name);
            let bytes = std::fs::read(path).unwrap();
            assert!(image::load_from_memory(&bytes).is_ok(), "{name}");
        }
    }
}
