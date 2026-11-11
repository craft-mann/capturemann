use super::*;
use crate::recording::geometry::Rect;

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

/// Margin between the toast window and the edges of the work area (Windows).
pub const TOAST_EDGE: f64 = 16.;
/// Gap between the toast window and the HUD when it is moved above it (Windows).
pub const TOAST_HUD_GAP: f64 = 8.;

fn overlaps(a: &Rect, b: &Rect) -> bool {
    a.x < b.right() && b.x < a.right() && a.y < b.bottom() && b.y < a.bottom()
}

/// Where the toast window of `win_h` sits inside `work` (the display without the taskbar, global
/// points): the lower right corner, `TOAST_EDGE` from both edges. When that would cover `avoid`
/// (the recording HUD, bottom centre) the window is lifted to sit above it. Used on Windows;
/// macOS keeps `toast_origin`.
pub fn toast_rect(work: &Rect, win_h: f64, avoid: Option<&Rect>) -> Rect {
    let w = f64::from(crate::toast::TOAST_WIDTH);
    let x = (work.right() - w - TOAST_EDGE).max(work.x);
    let mut r = Rect::new(x, work.bottom() - win_h - TOAST_EDGE, w, win_h);
    if let Some(a) = avoid {
        if overlaps(&r, a) {
            r.y = a.y - win_h - TOAST_HUD_GAP;
        }
    }
    r.y = r.y.max(work.y);
    r
}

/// What to do with a toast window that exists when the stack changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FitPlan {
    /// Same display, same frame: leave it alone (the host repaints from the model).
    Keep,
    /// Move or resize the native window.
    Move,
    /// Close it and open a new one (another display, or no way to resize it).
    Recreate,
}

/// The rule that stops the toast window being re-created on every sync: only a window on the
/// wrong display, or one that must change its frame and cannot (`has_native` false), is replaced.
pub fn fit_plan(has_native: bool, display_changed: bool, frame_changed: bool) -> FitPlan {
    if display_changed {
        FitPlan::Recreate
    } else if !frame_changed {
        FitPlan::Keep
    } else if has_native {
        FitPlan::Move
    } else {
        FitPlan::Recreate
    }
}

/// The monitor the toasts belong on (Windows): the recorded display while a recording runs, else
/// the display the toast window already is on, else the one holding the pointer, else the first
/// (the primary). `shown` and `displays` are GPUI display ids.
pub fn toast_display_index<I: Copy + PartialEq>(
    displays: &[I],
    recording: Option<I>,
    shown: Option<I>,
    pointer: Option<I>,
) -> Option<usize> {
    [recording, shown, pointer]
        .into_iter()
        .flatten()
        .find_map(|id| displays.iter().position(|d| *d == id))
        .or(if displays.is_empty() { None } else { Some(0) })
}

/// Where toasts go now.
struct ToastTarget {
    /// The display and its bounds (global points).
    display: Option<(gpui::DisplayId, Rect)>,
    /// The part of it a toast may use (the work area on Windows, the display elsewhere).
    work: Rect,
    /// The recording HUD on this display, if there is one to keep clear of.
    avoid: Option<Rect>,
}

fn toast_target(app: &Entity<AppModel>, cx: &App) -> Option<ToastTarget> {
    if !cfg!(windows) {
        let b = cx.primary_display().map(|d| d.bounds())?;
        return Some(ToastTarget {
            display: None,
            work: Rect::new(
                f64::from(b.origin.x.as_f32()),
                f64::from(b.origin.y.as_f32()),
                f64::from(b.size.width.as_f32()),
                f64::from(b.size.height.as_f32()),
            ),
            avoid: None,
        });
    }
    let all = crate::ui::recording::displays(cx);
    let ids: Vec<gpui::DisplayId> = all.iter().map(|(id, _)| *id).collect();
    let recording = {
        let rec = app.read(cx).rec.read(cx);
        rec.display
            .filter(|_| rec.hud.phase.is_active())
            .map(|d| d.0)
    };
    let shown = app.read(cx).toast_display;
    let pointer = crate::ui::recording::pointer_display(cx).map(|d| d.0);
    let i = toast_display_index(&ids, recording, shown, pointer)?;
    let display = all[i];
    let work = crate::ui::recording::visible_part(&display.1);
    // The HUD of a recording on this display, so a toast never covers its buttons.
    let avoid = {
        let rec = app.read(cx).rec.read(cx);
        (recording == Some(display.0) && rec.hud_win.is_some()).then(|| {
            let agent = rec
                .hud
                .started_by_agent()
                .map(|(client, _)| client.to_owned());
            crate::ui::recording::hud_rect(
                app,
                &display.1,
                agent.as_deref(),
                rec.hud.is_audio(),
                cx,
            )
        })
    };
    Some(ToastTarget {
        display: Some(display),
        work,
        avoid,
    })
}

/// The toast window's frame of height `win_h` (global points) on `t`.
fn toast_frame(t: &ToastTarget, win_h: f32) -> (f32, f32, f32, f32) {
    let w = crate::toast::TOAST_WIDTH;
    if cfg!(windows) {
        let r = toast_rect(&t.work, f64::from(win_h), t.avoid.as_ref());
        (r.x as f32, r.y as f32, w, win_h)
    } else {
        let (x, y) = toast_origin((t.work.w as f32, t.work.h as f32), win_h);
        (t.work.x as f32 + x, t.work.y as f32 + y, w, win_h)
    }
}

/// Never taller than the display leaves room for (a pile of toasts on a short screen).
fn clamp_height(t: &ToastTarget, h: f32) -> f32 {
    h.min((t.work.h as f32 - 96.).max(60.))
}

/// Shrinks the window of an emptied stack to one pixel in its corner, so it blocks nothing while
/// it waits for the next toast.
fn park_toast_window(app: &Entity<AppModel>, cx: &mut App, handle: WindowHandle<ToastHost>) {
    let Some(t) = toast_target(app, cx) else {
        return;
    };
    let (x, y, w, _) = toast_frame(&t, 1.);
    let (x, y) = (f64::from(x) + f64::from(w) - 1., f64::from(y));
    let native = handle
        .update(cx, |_, window, _| {
            crate::platform_window::NativeWindow::for_frame(window)
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
    let Some(t) = toast_target(app, cx) else {
        return;
    };
    let height = clamp_height(&t, measured).ceil();
    if (app.read(cx).toast_fit - height).abs() < 0.5 {
        return;
    }
    let (x, y, w, h) = toast_frame(&t, height);
    let native = handle
        .update(cx, |_, window, _| {
            crate::platform_window::NativeWindow::for_frame(window)
        })
        .ok()
        .flatten();
    let Some(native) = native else {
        return;
    };
    app.update(cx, |m, _| {
        m.toast_fit = height;
        m.toast_pos = (x, y);
    });
    // Resize from a task, never inside a window update (see `NativeWindow`).
    cx.spawn(async move |_| {
        native.set_frame(f64::from(x), f64::from(y), f64::from(w), f64::from(h));
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
                            m.toast_display = None;
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
                            park_toast_window(app, cx, h);
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
            let target = toast_target(app, cx);
            let win_h = target.as_ref().map_or(height, |t| clamp_height(t, height));
            let (x, y, win_w, win_h) = match &target {
                Some(t) => toast_frame(t, win_h),
                None => (100., 100., crate::toast::TOAST_WIDTH, win_h),
            };
            let display_id = target.as_ref().and_then(|t| t.display.map(|d| d.0));
            let mut keep_existing = existing;
            if let Some(h) = existing {
                let (shown, shown_pos, shown_fit) = {
                    let m = app.read(cx);
                    (m.toast_display, m.toast_pos, m.toast_fit)
                };
                let moved = (shown_pos.0 - x).abs() > 0.5 || (shown_pos.1 - y).abs() > 0.5;
                let frame_changed = moved || (shown_fit - win_h).abs() > 0.5;
                let native = if frame_changed {
                    // Resize from a task, never inside a window update (see `NativeWindow`).
                    h.update(cx, |_, window, _| {
                        crate::platform_window::NativeWindow::for_frame(window)
                    })
                    .ok()
                    .flatten()
                } else {
                    None
                };
                let plan = fit_plan(
                    native.is_some(),
                    display_id.is_some() && shown.is_some() && shown != display_id,
                    frame_changed,
                );
                log::debug!("sync_toasts: fit plan {plan:?}");
                match (plan, native) {
                    (FitPlan::Move, Some(native)) => {
                        app.update(cx, |m, _| {
                            m.toast_fit = win_h;
                            m.toast_pos = (x, y);
                        });
                        cx.spawn(async move |_| {
                            native.set_frame(
                                f64::from(x),
                                f64::from(y),
                                f64::from(win_w),
                                f64::from(win_h),
                            );
                        })
                        .detach();
                    }
                    (FitPlan::Recreate, _) => {
                        app.update(cx, |m, _| {
                            m.toast_window = None;
                            // Another display is not the quick re-open the gap protects against.
                            m.toast_closed_at = (shown == display_id).then_some(Instant::now());
                        });
                        crate::surfaces::close(cx, h);
                        keep_existing = None;
                    }
                    _ => {}
                }
            }
            if keep_existing.is_none() {
                let app2 = app.clone();
                let opened = cx.open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(Bounds {
                            origin: point(px(x), px(y)),
                            size: size(px(win_w), px(win_h)),
                        })),
                        titlebar: None,
                        focus: false,
                        show: true,
                        kind: WindowKind::PopUp,
                        is_movable: false,
                        is_resizable: false,
                        is_minimizable: false,
                        display_id,
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
                        "toast window opened: {win_w} x {win_h} at ({x}, {y}), display {}",
                        match display_id {
                            Some(d) => format!("{}", u64::from(d)),
                            None => "primary".to_owned(),
                        }
                    );
                    crate::surfaces::opened(crate::surfaces::Surface::Toast, handle);
                    app.update(cx, |m, _| {
                        m.toast_window = Some(handle);
                        m.toast_fit = win_h;
                        m.toast_pos = (x, y);
                        m.toast_display = display_id;
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

/// ANN-5: reads the text in a screenshot (the daemon reads the rendered image and copies the
/// text) and says how many words were copied. Used by the capture toast and the library menu.
pub(crate) fn copy_text_of(app: &Entity<AppModel>, id: &str, cx: &mut App) {
    let backend = app.read(cx).backend.clone();
    let Some(daemon) = backend.daemon.clone() else {
        return;
    };
    let (app2, id) = (app.clone(), id.to_owned());
    cx.spawn(async move |cx| {
        let res = backend
            .rt
            .spawn(async move { crate::editor::copytext::run(&daemon, &id, None).await })
            .await
            .unwrap_or_else(|e| Err(e.to_string()));
        cx.update(|cx| match res {
            Ok(o) => {
                let kind = if o.is_success() {
                    ToastKind::Success
                } else {
                    ToastKind::Info
                };
                push_toast(&app2, ToastSpec::new(kind, o.message()), cx);
            }
            Err(e) => push_toast(
                &app2,
                flows::error_toast_live(|| tr!("Couldn\u{2019}t read the text"), e),
                cx,
            ),
        });
    })
    .detach();
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
        ToastAction::CopyText(id) => copy_text_of(app, &id, cx),
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

#[cfg(test)]
mod toast_placement_tests {
    use super::*;
    use crate::recording::geometry::hud_default;
    use crate::recording::monitors::Monitor;

    const W: f64 = 440.;
    const HUD: (f64, f64) = (739., 150.);

    /// The lab laptop of Nov 5, 2026 (round 3): panel 1920x1080 at 150 % with a 48 px taskbar, and
    /// an external 3840x2160 at 150 % to its right.
    fn lab() -> [Monitor; 2] {
        let m = |id, f: [f64; 4], w: [f64; 4]| Monitor {
            id,
            frame_px: Rect::new(f[0], f[1], f[2], f[3]),
            work_px: Rect::new(w[0], w[1], w[2], w[3]),
            scale: 1.5,
        };
        [
            m(1, [0., 0., 1920., 1080.], [0., 0., 1920., 1032.]),
            m(2, [1920., 0., 3840., 2160.], [1920., 0., 3840., 2160.]),
        ]
    }

    fn physical(m: &Monitor, r: &Rect) -> Rect {
        m.to_physical(r)
    }

    #[test]
    fn without_a_hud_the_toast_sits_in_the_lower_right_of_the_work_area() {
        let ms = lab();
        // Panel: work area 1280 x 688 points. The log of the round: (824, 518) 440 x 154.
        let r = toast_rect(&ms[0].work_points(), 154., None);
        assert_eq!((r.x, r.y, r.w, r.h), (824., 518., W, 154.));
        // External: work area 2560 x 1440 points at x 1280.
        let r = toast_rect(&ms[1].work_points(), 154., None);
        assert_eq!((r.x, r.y), (1280. + 2560. - W - 16., 1440. - 154. - 16.));
        // In physical px it is inside the external monitor, not the panel.
        let p = physical(&ms[1], &r);
        assert!(
            p.x >= 1920. && p.right() <= 5760. && p.bottom() <= 2160.,
            "{p:?}"
        );
    }

    #[test]
    fn a_toast_that_would_cover_the_hud_goes_above_it() {
        let ms = lab();
        let work = ms[0].work_points();
        let hud = hud_default(&work, HUD);
        // The HUD is bottom centre: x 270..1009; the corner toast (824..1264) would cover it.
        let plain = toast_rect(&work, 154., None);
        assert!(overlaps(&plain, &hud));
        let r = toast_rect(&work, 154., Some(&hud));
        assert!(!overlaps(&r, &hud), "{r:?} {hud:?}");
        assert_eq!(r.x, plain.x, "the right edge stays");
        assert_eq!(r.y, hud.y - 154. - TOAST_HUD_GAP);
        // On the 4K external the HUD is far from the right edge: nothing moves.
        let work = ms[1].work_points();
        let hud = hud_default(&work, HUD);
        let r = toast_rect(&work, 154., Some(&hud));
        assert_eq!(r, toast_rect(&work, 154., None));
    }

    #[test]
    fn a_tall_pile_above_the_hud_stays_on_the_display() {
        let work = Rect::new(0., 0., 1280., 688.);
        let hud = Rect::new(270., 538., 739., 150.);
        let r = toast_rect(&work, 600., Some(&hud));
        assert!(r.y >= 0.);
    }

    #[test]
    fn the_display_is_the_recorded_one_then_the_shown_one_then_the_pointer_then_the_primary() {
        let d = [10u32, 20];
        // Recording the external while the pointer is on the panel (round 3, F2).
        assert_eq!(toast_display_index(&d, Some(20), None, Some(10)), Some(1));
        // Recording wins over a toast already shown elsewhere.
        assert_eq!(
            toast_display_index(&d, Some(10), Some(20), Some(20)),
            Some(0)
        );
        // No recording: a window already up stays where it is; else the pointer; else primary.
        assert_eq!(toast_display_index(&d, None, Some(20), Some(10)), Some(1));
        assert_eq!(toast_display_index(&d, None, None, Some(20)), Some(1));
        assert_eq!(toast_display_index(&d, None, None, None), Some(0));
        // An id that is no longer attached is skipped.
        assert_eq!(toast_display_index(&d, Some(99), None, Some(20)), Some(1));
        assert_eq!(toast_display_index::<u32>(&[], Some(1), None, None), None);
    }

    /// Six toasts queued in a row, with a hover and an expiry sync after each (what the 11 minute
    /// log of round 3 shows), open the window once: after the first open every sync reuses it, and
    /// a window that can be resized (macOS, Windows) is only moved, never replaced.
    #[test]
    fn queued_toasts_open_one_window_and_never_recreate_it() {
        let mut have_window = false;
        let mut opens = 0;
        let mut height = 0.;
        for n in 1..=6 {
            for sync in 0..3 {
                // The first sync of a toast (push) grows the stack; the others change nothing.
                let new_height = if sync == 0 {
                    154. * f64::from(n)
                } else {
                    height
                };
                match window_plan(true, false, have_window, Duration::ZERO, None) {
                    WindowPlan::Open => {
                        opens += 1;
                        have_window = true;
                    }
                    WindowPlan::Reuse => {
                        let changed = new_height != height;
                        let native = fit_plan(true, false, changed);
                        assert_eq!(
                            native,
                            if changed {
                                FitPlan::Move
                            } else {
                                FitPlan::Keep
                            }
                        );
                        if !changed {
                            assert_eq!(fit_plan(false, false, false), FitPlan::Keep);
                        }
                    }
                    other => panic!("{other:?}"),
                }
                height = new_height;
            }
        }
        assert_eq!(opens, 1);
    }

    #[test]
    fn a_window_is_recreated_only_for_another_display_or_when_it_cannot_be_resized() {
        assert_eq!(fit_plan(true, false, false), FitPlan::Keep);
        assert_eq!(fit_plan(false, false, false), FitPlan::Keep);
        assert_eq!(fit_plan(true, false, true), FitPlan::Move);
        assert_eq!(fit_plan(false, false, true), FitPlan::Recreate);
        assert_eq!(fit_plan(true, true, false), FitPlan::Recreate);
    }
}
