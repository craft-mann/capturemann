use super::*;

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

pub(super) fn open_screen_recording_settings() {
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
