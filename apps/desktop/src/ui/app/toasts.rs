use super::*;

/// Where the toasts are drawn: inside the Studio window (`true`) or in a window of their own.
/// A separate toast window cannot be always-on-top or unfocused on Wayland, so there the stack
/// is a banner in the library window for as long as that window exists; once it is closed the
/// toasts get their own window again. (An "only while active" rule was tried: Wayland reports
/// the window active and inactive within moments as the toast window comes and goes, so the
/// stack flapped.) `cloaked`: a capture is
/// in progress (`capture_cloak`) and no toast may be on screen at all.
pub fn toasts_in_window(wayland: bool, studio_open: bool, has_toasts: bool, cloaked: bool) -> bool {
    wayland && studio_open && has_toasts && !cloaked
}

/// How long the toast window stays (parked, 1 px) after its last toast went, so the next toast
/// reuses it instead of a window being destroyed and created again.
pub const TOAST_IDLE_GRACE: Duration = Duration::from_secs(2);
/// The least time between closing a toast window and opening the next one. GPUI on macOS logs
/// `window not found` about 120 times a second for as long as a toast window opened within
/// milliseconds of the previous one's close lives (seen in the 0.2.6 logs: five of 45 closes, each
/// followed by a new window 13 to 32 ms later, and each burst lasting that window's whole life).
pub const TOAST_REOPEN_GAP: Duration = Duration::from_millis(600);

/// What `sync_toasts` does with the toast window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowPlan {
    /// No window and none wanted.
    Nothing,
    /// Close it now.
    Close,
    /// No toast to show: keep the window parked for this much longer, then close it.
    Park(Duration),
    /// There is a window: fit it to the stack.
    Reuse,
    /// Open one now.
    Open,
    /// A window was closed too recently: open one after this long.
    WaitToOpen(Duration),
}

/// The rule for the toast window's life. `immediate`: it must go at once when nothing is to be
/// shown (a capture hides the windows, or the toasts are banners in Studio). `idle_for`: how long
/// the stack has been empty; `since_closed`: how long ago a toast window was closed.
pub fn window_plan(
    want_cards: bool,
    immediate: bool,
    have_window: bool,
    idle_for: Duration,
    since_closed: Option<Duration>,
) -> WindowPlan {
    if !want_cards {
        return match (have_window, immediate) {
            (false, _) => WindowPlan::Nothing,
            (true, true) => WindowPlan::Close,
            (true, false) if idle_for >= TOAST_IDLE_GRACE => WindowPlan::Close,
            (true, false) => WindowPlan::Park(TOAST_IDLE_GRACE - idle_for),
        };
    }
    if have_window {
        return WindowPlan::Reuse;
    }
    match since_closed {
        Some(d) if d < TOAST_REOPEN_GAP => WindowPlan::WaitToOpen(TOAST_REOPEN_GAP - d),
        _ => WindowPlan::Open,
    }
}

pub fn push_toast(app: &Entity<AppModel>, spec: ToastSpec, cx: &mut App) {
    log::info!(
        "toast queued: {:?}, {} button(s), {} already queued, window {}",
        spec.kind,
        spec.buttons.len(),
        app.read(cx).toasts.toasts().len(),
        if app.read(cx).toast_window.is_some() {
            "exists"
        } else {
            "none"
        }
    );
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

/// Shrinks the window of an emptied stack to one pixel in its corner, so it blocks nothing while
/// it waits for the next toast.
fn park_toast_window(cx: &mut App, handle: WindowHandle<ToastHost>) {
    let Some(b) = cx.primary_display().map(|d| d.bounds()) else {
        return;
    };
    let (x, y) = toast_origin((b.size.width.as_f32(), b.size.height.as_f32()), 1.);
    let w = f64::from(crate::toast::TOAST_WIDTH);
    let (x, y) = (
        f64::from(b.origin.x.as_f32() + x) + w - 1.,
        f64::from(b.origin.y.as_f32() + y),
    );
    let native = handle
        .update(cx, |_, window, _| {
            crate::platform_window::NativeWindow::of(window)
        })
        .ok()
        .flatten();
    if let Some(native) = native {
        // Resize from a task, never inside a window update (see `NativeWindow`).
        cx.spawn(async move |_| native.set_frame(x, y, 1., 1.))
            .detach();
    }
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
        let (empty, height, immediate) = {
            let m = app.read(cx);
            let stack = m.toasts.toasts();
            log::debug!(
                "sync_toasts: {} toasts, studio_open {}, in_studio {}",
                stack.len(),
                m.studio_open,
                m.toasts_in_studio()
            );
            // No window of their own while they are banners in Studio or a capture hides them.
            let immediate = m.toasts_in_studio() || crate::capture_cloak::is_hidden();
            (
                stack.is_empty() || immediate,
                crate::toast::stack_height(stack),
                immediate,
            )
        };
        let existing = app.read(cx).toast_window;
        let now = Instant::now();
        let (idle_for, since_closed) = {
            let m = app.read(cx);
            (
                m.toast_idle_since.map_or(Duration::ZERO, |t| now - t),
                m.toast_closed_at.map(|t| now - t),
            )
        };
        let plan = window_plan(
            !empty,
            immediate,
            existing.is_some(),
            idle_for,
            since_closed,
        );
        log::debug!("sync_toasts: window plan {plan:?}");
        // Looks at the window again when the grace or the gap is over.
        let look_again = |app: &Entity<AppModel>, after: Duration, cx: &mut App| {
            let app2 = app.clone();
            let task = cx.spawn(async move |cx| {
                cx.background_executor().timer(after).await;
                cx.update(|cx| sync_toasts(&app2, cx));
            });
            app.update(cx, |m, _| m.toast_window_timer = Some(task));
        };
        if empty {
            match plan {
                WindowPlan::Close => {
                    if let Some(h) = existing {
                        app.update(cx, |m, _| {
                            m.toast_window = None;
                            m.toast_idle_since = None;
                            m.toast_closed_at = Some(now);
                            m.toast_window_timer = None;
                        });
                        crate::surfaces::close(cx, h);
                    }
                }
                WindowPlan::Park(rest) => {
                    if let Some(h) = existing {
                        if app.read(cx).toast_idle_since.is_none() {
                            app.update(cx, |m, _| {
                                m.toast_idle_since = Some(now);
                                m.toast_fit = 1.;
                            });
                            park_toast_window(cx, h);
                        }
                        look_again(app, rest, cx);
                    }
                }
                _ => {}
            }
        } else if let WindowPlan::WaitToOpen(wait) = plan {
            // The toasts stay queued; the window opens when the gap since the last close is over.
            look_again(app, wait, cx);
        } else {
            app.update(cx, |m, _| {
                m.toast_idle_since = None;
                m.toast_window_timer = None;
            });
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
                    crate::surfaces::close(cx, h);
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
                        ..crate::platform_window::popup_base()
                    },
                    move |window, cx| {
                        // R4-6: a sticky toast must not end up in the next screenshot or recording.
                        // No window shadow either: a transparent window otherwise keeps a faint
                        // rectangular halo around the whole stack (the panel the owner saw).
                        crate::ui::recording::harden(window, false);
                        cx.new(|cx| ToastHost::new(app2, cx))
                    },
                );
                if let Err(e) = &opened {
                    // Without this line a toast that cannot get a window is silent.
                    log::error!("the toast window could not be opened: {e:#}");
                }
                if let Ok(handle) = opened {
                    log::info!(
                        "toast window opened: {win_w} x {win_h} at ({}, {}), display {}",
                        f32::from(origin.x),
                        f32::from(origin.y),
                        if display.is_some() {
                            "known"
                        } else {
                            "unknown"
                        }
                    );
                    crate::surfaces::opened(crate::surfaces::Surface::Toast, handle);
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
    app.update(cx, |_, cx| cx.notify()); // the Studio banners follow the queue
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
        ToastAction::CancelImport(id) => crate::ui::import_actions::cancel(app, &id, cx),
        ToastAction::UploadFile(id, file) => {
            crate::ui::upload_ui::upload(app, Some(&id), Some(&file), cx)
        }
    }
}

#[cfg(test)]
mod window_plan_tests {
    use super::*;

    const MS: fn(u64) -> Duration = Duration::from_millis;

    /// The 0.2.6 log burst: a toast window closed and a new one opened 13 to 32 ms later. A new
    /// toast right after the stack emptied reuses the parked window instead.
    #[test]
    fn a_toast_right_after_the_last_one_reuses_the_window() {
        let parked = window_plan(false, false, true, MS(30), None);
        assert_eq!(parked, WindowPlan::Park(TOAST_IDLE_GRACE - MS(30)));
        assert_eq!(
            window_plan(true, false, true, MS(30), None),
            WindowPlan::Reuse
        );
    }

    #[test]
    fn the_parked_window_closes_after_the_grace() {
        assert_eq!(
            window_plan(false, false, true, TOAST_IDLE_GRACE, None),
            WindowPlan::Close
        );
        assert_eq!(
            window_plan(false, false, false, Duration::ZERO, None),
            WindowPlan::Nothing
        );
    }

    /// A capture (or the Studio banners) removes the window at once, grace or not.
    #[test]
    fn a_capture_closes_the_window_at_once() {
        assert_eq!(
            window_plan(false, true, true, Duration::ZERO, None),
            WindowPlan::Close
        );
    }

    /// No window is opened within the gap of the previous close.
    #[test]
    fn a_window_is_not_opened_right_after_one_was_closed() {
        assert_eq!(
            window_plan(true, false, false, Duration::ZERO, Some(MS(20))),
            WindowPlan::WaitToOpen(TOAST_REOPEN_GAP - MS(20))
        );
        assert_eq!(
            window_plan(true, false, false, Duration::ZERO, Some(TOAST_REOPEN_GAP)),
            WindowPlan::Open
        );
        assert_eq!(
            window_plan(true, false, false, Duration::ZERO, None),
            WindowPlan::Open
        );
    }

    /// Whatever the order of toasts and timers, two window opens are never closer than the gap to
    /// a close before them: replays a stream of events through the plan.
    #[test]
    fn no_open_ever_follows_a_close_within_the_gap() {
        let mut closed_at: Option<u64> = None;
        let mut window = false;
        let mut idle_since: Option<u64> = None;
        for t in (0..20_000u64).step_by(10) {
            // Toasts present in bursts: 0..300, 320..400 (20 ms after), 6000..6100, 8000..8050.
            let want = (0..300).contains(&t)
                || (320..400).contains(&t)
                || (6_000..6_100).contains(&t)
                || (8_000..8_050).contains(&t);
            let idle_for = idle_since.map_or(Duration::ZERO, |s| MS(t - s));
            let since = closed_at.map(|c| MS(t - c));
            match window_plan(want, false, window, idle_for, since) {
                WindowPlan::Close => {
                    window = false;
                    idle_since = None;
                    closed_at = Some(t);
                }
                WindowPlan::Park(_) => {
                    idle_since.get_or_insert(t);
                }
                WindowPlan::Reuse => idle_since = None,
                WindowPlan::Open => {
                    if let Some(c) = closed_at {
                        assert!(
                            t - c >= TOAST_REOPEN_GAP.as_millis() as u64,
                            "{t} after {c}"
                        );
                    }
                    window = true;
                    idle_since = None;
                }
                WindowPlan::WaitToOpen(_) | WindowPlan::Nothing => {}
            }
        }
    }
}

#[cfg(test)]
mod placement_tests {
    use super::toasts_in_window;

    #[test]
    fn banners_only_on_wayland_while_studio_is_active_and_nothing_is_cloaked() {
        assert!(toasts_in_window(true, true, true, false));
        assert!(
            !toasts_in_window(false, true, true, false),
            "X11, macOS, Windows"
        );
        assert!(
            !toasts_in_window(true, false, true, false),
            "no Studio window"
        );
        assert!(
            !toasts_in_window(true, true, false, false),
            "nothing to show"
        );
        assert!(
            !toasts_in_window(true, true, true, true),
            "a capture is running"
        );
    }
}
