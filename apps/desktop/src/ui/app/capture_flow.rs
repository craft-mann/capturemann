use super::*;

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
                log::warn!("clipboard copy failed for {}", done.capture_id);
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
