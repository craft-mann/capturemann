use super::*;

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
