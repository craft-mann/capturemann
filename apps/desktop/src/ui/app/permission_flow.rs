use super::*;

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
