use super::*;

/// Quits. A recording in progress is stopped and its bundle closed first (REC-8, NFR-6): waiting
/// for the engine can take a moment, so the quit happens when it is done (at most 10 s).
/// What Quit has to wait for before the app may close.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuitWait {
    /// A recording is open: its bundle is closed first.
    Recording,
    /// Jobs are running (a transcription, an upload): they stop at a checkpoint first.
    Jobs,
}

/// `None`: nothing to wait for, quit at once.
pub fn quit_wait(recording_active: bool, running_jobs: usize) -> Option<QuitWait> {
    if recording_active {
        Some(QuitWait::Recording)
    } else if running_jobs > 0 {
        Some(QuitWait::Jobs)
    } else {
        None
    }
}

pub fn quit(app: &Entity<AppModel>, cx: &mut App) {
    if app.update(cx, |m, _| std::mem::replace(&mut m.quitting, true)) {
        return;
    }
    let backend = app.read(cx).backend.clone();
    let Some(daemon) = backend.daemon.clone() else {
        cx.quit();
        return;
    };
    let recording = daemon.recording_active();
    let jobs = daemon.job_queue().map_or(0, |q| q.running_count());
    let Some(wait) = quit_wait(recording, jobs) else {
        cx.quit();
        return;
    };
    // Saving a long recording, or a job stopping at its checkpoint, can take a few seconds: say so
    // instead of going quiet (R4-23). GPUI itself waits only 200 ms for the quit handler and logs
    // an error after that, so the wait happens here, before `cx.quit()`.
    push_toast(
        app,
        match wait {
            QuitWait::Recording => {
                ToastSpec::new(ToastKind::Info, tr!("Saving the recording\u{2026}"))
                    .body(tr!("CaptureMann closes as soon as it is saved."))
            }
            QuitWait::Jobs => {
                ToastSpec::new(ToastKind::Info, tr!("Stopping background work\u{2026}")).body(tr!(
                    "Unfinished jobs go on the next time CaptureMann opens."
                ))
            }
        },
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
        let _ = cm_platform::process::command("open")
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

/// Another client of the daemon (the CLI, an MCP agent) stored settings: the model keeps a
/// cached copy of the ones the UI shows (theme, language, copy after capture, capture delay), so
/// re-read them from the shared store and bring the tray and the windows along. Idempotent: the
/// app's own writes send the same event.
pub fn sync_from_settings(app: &Entity<AppModel>, cx: &mut App) {
    let snap = app.read(cx).backend.settings.snapshot();
    let theme = snap
        .get(KEY_THEME)
        .and_then(|v| v.as_str())
        .and_then(ThemePreference::parse);
    let copy = snap
        .get(KEY_COPY_AFTER_CAPTURE)
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let language = crate::settings::language_from_settings(&snap);
    let delay = crate::delay::from_setting(snap.get(KEY_CAPTURE_DELAY));
    let (theme_changed, language_changed, delay_changed) = app.update(cx, |m, cx| {
        let t = theme.is_some_and(|t| t != m.theme_pref);
        if let Some(t) = theme {
            m.theme_pref = t;
        }
        let l = m.language != language;
        m.language = language;
        let d = m.capture_delay_s != delay;
        m.capture_delay_s = delay;
        m.copy_after_capture = copy;
        cx.notify();
        (t, l, d)
    });
    if delay_changed {
        if let Some(t) = &app.read(cx).tray {
            t.set_delay(delay);
        }
    }
    if theme_changed {
        apply_theme(app, cx);
    }
    if language_changed {
        apply_language(app, cx);
    }
}

#[cfg(test)]
mod quit_tests {
    use super::*;

    /// `timed out waiting on app_will_quit` (0.2.5 log): a running job was stopped inside GPUI's
    /// 200 ms quit window. Jobs now make Quit wait first, like a recording does.
    #[test]
    fn quit_waits_for_a_recording_or_a_running_job() {
        assert_eq!(quit_wait(false, 0), None);
        assert_eq!(quit_wait(true, 0), Some(QuitWait::Recording));
        assert_eq!(quit_wait(false, 2), Some(QuitWait::Jobs));
        assert_eq!(quit_wait(true, 2), Some(QuitWait::Recording));
    }
}
