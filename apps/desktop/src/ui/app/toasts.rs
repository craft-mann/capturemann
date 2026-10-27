use super::*;

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

pub(super) fn toast_action(app: &Entity<AppModel>, action: ToastAction, cx: &mut App) {
    match action {
        ToastAction::OpenLibrary => navigate(app, Route::Library, cx),
        ToastAction::SuggestReport(id) => crate::ui::ai::open_report_setup(app, &id, cx),
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
        ToastAction::UploadFile(id, file) => {
            crate::ui::upload_ui::upload(app, Some(&id), Some(&file), cx)
        }
    }
}
