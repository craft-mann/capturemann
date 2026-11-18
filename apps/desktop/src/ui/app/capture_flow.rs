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

/// The delay for this capture, read from the shared settings store (what `set_settings` from
/// the CLI or an agent wrote), not from the cached model value; the model follows.
pub(crate) fn current_capture_delay(app: &Entity<AppModel>, cx: &mut App) -> u32 {
    // Quick Capture: the panel's Delay tile is its own choice, never the setting (QC-1.AC6).
    if let Some(ctx) = &app.read(cx).quick.ctx {
        return ctx.run.delay_s;
    }
    let secs = crate::delay::from_setting(
        app.read(cx)
            .backend
            .settings
            .snapshot()
            .get(KEY_CAPTURE_DELAY),
    );
    if app.read(cx).capture_delay_s != secs {
        app.update(cx, |m, cx| {
            m.capture_delay_s = secs;
            cx.notify();
        });
        if let Some(t) = &app.read(cx).tray {
            t.set_delay(secs);
        }
    }
    secs
}

/// Hotkey, tray menu or toolbar button: wait out the chosen delay if one is set (CAP-5), then
/// run the overlay and CAP-4.
pub fn start_capture(app: &Entity<AppModel>, mode: CaptureMode, cx: &mut App) {
    // Any other way in forgets a Quick Capture run that never finished (a picker closed with Esc).
    app.update(cx, |m, _| m.quick.ctx = None);
    start_capture_inner(app, mode, cx);
}

/// [`start_capture`] without forgetting the Quick Capture run (its own start).
pub(crate) fn start_capture_inner(app: &Entity<AppModel>, mode: CaptureMode, cx: &mut App) {
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
    let secs = current_capture_delay(app, cx);
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
        let quick = m.quick.ctx.as_ref().map(|c| c.run.copy);
        m.copy_override
            .take()
            .or(quick)
            .unwrap_or(m.copy_after_capture)
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
        let area = std::sync::Arc::new(std::sync::Mutex::new(None));
        let area2 = area.clone();
        let flow = backend
            .rt
            .spawn(async move {
                match b.overlay.run_with_area(mode).await {
                    Ok((OverlayOutcome::Captured { capture_id }, got)) => {
                        *area2.lock().unwrap_or_else(|e| e.into_inner()) = got;
                        let done = b.after_capture(&capture_id, copy).await;
                        Ok(CaptureFlow::Done(done))
                    }
                    Ok((OverlayOutcome::Cancelled, _)) => Ok(CaptureFlow::Cancelled),
                    Ok((OverlayOutcome::SwitchToList, _)) => Ok(CaptureFlow::SwitchToList),
                    Err(e) => Err(e),
                }
            })
            .await;
        let flow = match flow {
            Ok(Ok(flow)) => flow,
            Ok(Err(e)) => CaptureFlow::Overlay(e),
            Err(e) => CaptureFlow::Failed(e.to_string()),
        };
        let got = *area.lock().unwrap_or_else(|e| e.into_inner());
        cx.update(|cx| {
            if let Some((display_id, rect)) = got {
                crate::ui::quick_flow::note_area(&app2, display_id, rect, cx);
            }
            finish_capture(&app2, flow, cx)
        });
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
    crate::ui::quick_flow::note_window(app, window_id, cx);
    // CAP-5: the remembered delay applies to a window chosen from the list too (the pill counts
    // down, the daemon captures that window afterwards, Esc cancels).
    let secs = current_capture_delay(app, cx);
    if backend.daemon.is_some() && secs > 0 {
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
    log::info!(
        "capture ended: {}",
        match &flow {
            CaptureFlow::Done(Ok(_)) => "saved".to_owned(),
            CaptureFlow::Done(Err(e)) => format!("saved, but the finish failed: {e}"),
            CaptureFlow::Cancelled => "cancelled".to_owned(),
            CaptureFlow::SwitchToList => "switch to the window list".to_owned(),
            CaptureFlow::Overlay(e) => format!("overlay error: {e:?}"),
            CaptureFlow::Failed(e) => format!("failed: {e}"),
        }
    );
    app.update(cx, |m, _| m.capture_busy = false);
    // A Quick Capture run ends with its capture; Tab to the window list (R19) keeps it.
    let quick = if matches!(flow, CaptureFlow::SwitchToList) {
        None
    } else {
        app.update(cx, |m, _| m.quick.ctx.take())
    };
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
            match quick {
                // QC-3: the bar, or the Edit / Undo toast, instead of the usual toast.
                Some(ctx) => crate::ui::quick_flow::finish_shot(app, ctx, &done, cx),
                None => push_toast(app, spec, cx),
            }
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
