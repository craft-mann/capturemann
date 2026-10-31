use super::*;

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
            ..crate::platform_window::popup_base()
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
