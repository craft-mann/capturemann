//! The delayed-capture pill (CAP-5, DESIGN 5.3 styling): a 48 px disc in the top right corner of
//! the display with the seconds left and a `rec` ring that drains over the whole delay. The window
//! never activates CaptureMann and takes no keyboard focus (a menu the user wants to capture must
//! stay open); a click on the pill makes it key, and then Esc cancels (CAP-5.AC2). It is excluded
//! from capture.
//!
//! Display captures wait in the daemon (`capture_screenshot { delay_s, request_id }`), so the
//! frame is taken by the same code an agent's delayed capture uses, and Esc is
//! `Daemon::cancel_delayed_capture`. Region and Window captures wait here, then open the picker,
//! so the freeze happens when the delay ends (CAP-5.AC3).

use std::time::{Duration, Instant};

use cm_i18n::tr;
use gpui::{
    canvas, div, point, prelude::*, px, App, Context, Entity, FocusHandle, FontWeight,
    KeyDownEvent, PathBuilder, Render, Role, SharedString, Window, WindowHandle,
};
use serde_json::json;

use crate::backend::Backend;
use crate::delay::{self, key_action, KeyAction};
use crate::overlay::CaptureMode;
use crate::theme::Theme;
use crate::ui::app::{self, AppModel, CaptureFlow};
use crate::ui::paint::{hsla_of, rgba_of, ui_features, TextExt, UI_FONT};
use crate::ui::recording::{harden, panel_options, pointer_display, visible_part};

/// A delay that is running.
pub struct DelayRun {
    pub request_id: String,
    pub started: Instant,
    pub total: Duration,
    pub reduced_motion: bool,
    /// Esc was pressed: the timer path stops, the daemon path was told to stop.
    pub cancelled: bool,
    win: Option<WindowHandle<DelayPill>>,
}

pub struct DelayPill {
    app: Entity<AppModel>,
    focus: FocusHandle,
}

impl DelayPill {
    fn new(app: Entity<AppModel>, reduced_motion: bool, cx: &mut Context<Self>) -> DelayPill {
        // The digit changes every second and the ring drains smoothly: repaint at ~30 fps, or
        // once a second with reduced motion (R4-13).
        let every = if reduced_motion { 250 } else { 33 };
        cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(every))
                .await;
            if this.update(cx, |_, cx| cx.notify()).is_err() {
                break;
            }
        })
        .detach();
        DelayPill {
            app,
            focus: cx.focus_handle(),
        }
    }
}

impl Render for DelayPill {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = Theme::overlay();
        let (digit, frac) = match &self.app.read(cx).delay_run {
            Some(r) => {
                let e = r.started.elapsed();
                (
                    delay::digit(e, r.total),
                    delay::ring_fraction(e, r.total, !r.reduced_motion),
                )
            }
            None => (1, 0.0),
        };
        let ring = canvas(
            |_, _, _| (),
            move |bounds, (), window, _| {
                let c = bounds.center();
                let r = f32::from(bounds.size.width) / 2.0 - delay::RING as f32 / 2.0;
                let steps = 48usize;
                let used = ((steps as f64) * frac).round() as usize;
                if used < 2 {
                    return;
                }
                let mut pb = PathBuilder::stroke(px(delay::RING as f32));
                for i in 0..=used {
                    let a = -std::f32::consts::FRAC_PI_2
                        + (i as f32 / steps as f32) * std::f32::consts::TAU;
                    let p = point(c.x + px(r * a.cos()), c.y + px(r * a.sin()));
                    if i == 0 {
                        pb.move_to(p);
                    } else {
                        pb.line_to(p);
                    }
                }
                if let Ok(path) = pb.build() {
                    window.paint_path(path, hsla_of(Theme::overlay().rec));
                }
            },
        )
        .absolute()
        .size_full();
        let app = self.app.clone();
        div()
            .id("delay-pill")
            .role(Role::Status)
            .aria_label(SharedString::from(delay::label(digit)))
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .font_family(UI_FONT)
            .font_features(ui_features())
            // A click makes the window key (it opens without focus), so Esc reaches it.
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, _, window, cx| window.focus(&this.focus, cx)),
            )
            .on_key_down(cx.listener(move |_, ev: &KeyDownEvent, _, cx| {
                if key_action(&ev.keystroke.key) == KeyAction::Cancel {
                    let app = app.clone();
                    cx.defer(move |cx| cancel(&app, cx));
                }
            }))
            .child(
                div()
                    .relative()
                    .size(px(delay::PILL as f32))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(delay::PILL as f32 / 2.0))
                    .border_1()
                    .border_color(rgba_of(t.line))
                    .bg(rgba_of(t.surface))
                    .child(ring)
                    .child(
                        div()
                            .text_size(px(delay::DIGIT_SIZE))
                            .line_height(px(delay::DIGIT_SIZE))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgba_of(t.text))
                            .mono()
                            .tnum()
                            .child(SharedString::from(digit.to_string())),
                    ),
            )
    }
}

/// The bare Esc key, registered as a global hotkey only while a delay runs, so Esc cancels
/// without the pill taking focus (CAP-5.AC2). Other apps get Esc back the moment the run ends.
fn escape_hotkey() -> global_hotkey::hotkey::HotKey {
    global_hotkey::hotkey::HotKey::new(None, global_hotkey::hotkey::Code::Escape)
}

/// The hotkey id of [`escape_hotkey`], for the command handler.
pub fn escape_hotkey_id() -> u32 {
    escape_hotkey().id()
}

/// Registers Esc. Failure (another app owns it) leaves the click-then-Esc path on the pill.
fn grab_escape(cx: &App) {
    if let Some(h) = cx.try_global::<crate::platform::HotkeyHolder>() {
        if let Err(e) = h.0.register(escape_hotkey()) {
            eprintln!("cm-desktop: could not register Esc for the delay: {e}");
        }
    }
}

fn release_escape(cx: &App) {
    if let Some(h) = cx.try_global::<crate::platform::HotkeyHolder>() {
        let _ = h.0.unregister(escape_hotkey());
    }
}

/// Starts a capture after `secs` seconds with the pill on screen. Ignored while another capture
/// or delay is running (CAP-2).
pub fn start(app: &Entity<AppModel>, mode: CaptureMode, secs: u32, cx: &mut App) {
    start_inner(app, mode, secs, None, cx);
}

/// The same delay for a window already chosen in the window picker (R19): the daemon waits and
/// then captures that window by id, Esc cancels like a delayed display capture (CAP-5.AC2).
pub fn start_window(app: &Entity<AppModel>, window_id: u32, secs: u32, cx: &mut App) {
    start_inner(
        app,
        CaptureMode::Window,
        secs,
        Some(json!({ "window_id": window_id })),
        cx,
    );
}

fn start_inner(
    app: &Entity<AppModel>,
    mode: CaptureMode,
    secs: u32,
    fixed_target: Option<serde_json::Value>,
    cx: &mut App,
) {
    let (backend, busy, seq) = {
        let m = app.read(cx);
        (m.backend.clone(), m.capture_busy, m.delay_seq + 1)
    };
    if busy {
        return;
    }
    // No library: the plain path shows the "Capture unavailable" toast.
    let Some(daemon) = backend.daemon.clone() else {
        app::run_capture(app, mode, cx);
        return;
    };
    let target = pointer_display(cx);
    let request_id = delay::request_id(seq);
    let total = Duration::from_secs(u64::from(secs));
    let reduced_motion = crate::platform_window::reduce_motion();
    app.update(cx, |m, _| {
        m.capture_busy = true;
        m.delay_seq = seq;
        m.delay_run = Some(DelayRun {
            request_id: request_id.clone(),
            started: Instant::now(),
            total,
            reduced_motion,
            cancelled: false,
            win: None,
        });
    });
    grab_escape(cx);
    let win = target.and_then(|display| {
        let global = delay::window_rect(&visible_part(&display.1));
        let app2 = app.clone();
        cx.open_window(
            panel_options(display, global, false, false),
            move |window, cx| {
                harden(window, false);
                cx.new(|cx| DelayPill::new(app2, reduced_motion, cx))
            },
        )
        .ok()
    });
    app.update(cx, |m, _| {
        if let Some(r) = &mut m.delay_run {
            r.win = win;
        }
    });

    // Display mode waits in the daemon: Esc is `cancel_delayed_capture(request_id)`.
    let display_id = target.and_then(|(id, _)| u32::try_from(u64::from(id)).ok());
    let daemon_target = match (fixed_target, mode, display_id) {
        (Some(t), _, _) => Some(t),
        (None, CaptureMode::Display, Some(display_id)) => Some(json!({ "display_id": display_id })),
        _ => None,
    };
    if let Some(daemon_target) = daemon_target {
        // A launcher action's one-off copy choice (LNC-1 Actions), else the setting. Region and
        // Window read it when the picker opens at the end of the delay.
        let copy = app::take_copy(app, cx);
        let app2 = app.clone();
        cx.spawn(async move |cx| {
            let b = backend.clone();
            let rid = request_id.clone();
            let flow = backend
                .rt
                .spawn(async move {
                    let params = json!({
                        "target": daemon_target,
                        "delay_s": secs,
                        "request_id": rid,
                    });
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
            cx.update(|cx| {
                // Esc cancels the waiting call, which then fails: that is not an error to show.
                let cancelled = was_cancelled(&app2, cx);
                close_pill(&app2, cx);
                let flow = if cancelled {
                    CaptureFlow::Cancelled
                } else {
                    flow
                };
                app::finish_capture(&app2, flow, cx);
            });
        })
        .detach();
        return;
    }

    // Region and Window: wait here, then open the picker so the freeze is at the end of the delay.
    let app2 = app.clone();
    cx.spawn(async move |cx| loop {
        cx.background_executor()
            .timer(Duration::from_millis(50))
            .await;
        let step = cx.update(|cx| match &app2.read(cx).delay_run {
            Some(r) if !r.cancelled && r.started.elapsed() >= r.total => Step::Fire,
            Some(r) if !r.cancelled => Step::Wait,
            _ => Step::Stop,
        });
        match step {
            Step::Wait => {}
            Step::Stop => break,
            Step::Fire => {
                cx.update(|cx| {
                    close_pill(&app2, cx);
                    app2.update(cx, |m, _| m.capture_busy = false);
                    app::run_capture(&app2, mode, cx);
                });
                break;
            }
        }
    })
    .detach();
}

enum Step {
    Wait,
    Fire,
    Stop,
}

fn was_cancelled(app: &Entity<AppModel>, cx: &App) -> bool {
    app.read(cx).delay_run.as_ref().is_some_and(|r| r.cancelled)
}

/// Closes the pill and forgets the run.
fn close_pill(app: &Entity<AppModel>, cx: &mut App) {
    release_escape(cx);
    let run = app.update(cx, |m, _| m.delay_run.take());
    if let Some(win) = run.and_then(|r| r.win) {
        let _ = win.update(cx, |_, window, _| window.remove_window());
    }
}

/// CAP-5.AC2: Esc. A Display delay is cancelled in the daemon, where the capture waits; a Region
/// or Window delay is only a timer here. Nothing is captured either way.
pub fn cancel(app: &Entity<AppModel>, cx: &mut App) {
    let (request_id, daemon) = {
        let m = app.read(cx);
        let Some(run) = m.delay_run.as_ref().filter(|r| !r.cancelled) else {
            return;
        };
        (run.request_id.clone(), m.backend.daemon.clone())
    };
    app.update(cx, |m, _| {
        if let Some(r) = &mut m.delay_run {
            r.cancelled = true;
        }
        // Nothing will be captured, so a launcher action's one-off copy choice is spent.
        m.copy_override = None;
    });
    let waiting_in_daemon = daemon
        .as_ref()
        .is_some_and(|d| d.cancel_delayed_capture(&request_id));
    if !waiting_in_daemon {
        // The timer path (or the call already finished): no daemon call will report back.
        close_pill(app, cx);
        app.update(cx, |m, _| m.capture_busy = false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CAP-5.AC2: Esc is a global key during a delay, and no shortcut chord collides with it.
    #[test]
    fn cap5_ac2_escape_hotkey_is_distinct_from_every_default_shortcut() {
        let esc = escape_hotkey_id();
        let map = crate::shortcuts::ShortcutMap::defaults(crate::shortcuts::Platform::Mac);
        for (_, chord) in map.iter() {
            let id = crate::platform::chord_to_hotkey(chord).unwrap().id();
            assert_ne!(id, esc);
        }
    }

    /// CAP-5.AC2: Esc is the only key that cancels, and it reaches `cancel` through the same
    /// mapping the pill uses.
    #[test]
    fn cap5_ac2_only_escape_cancels() {
        assert_eq!(key_action("escape"), KeyAction::Cancel);
        assert_eq!(key_action("enter"), KeyAction::None);
    }
}
