use super::*;

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
                log::warn!("could not register {}: {e}", action.label());
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
