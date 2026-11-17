//! The Click Guide bar (GDE-1, design boards S3, S16, S17): a small always-on-top panel with the
//! step count, Pause, Note step, Undo step and Stop. It is excluded from screen capture, and its
//! rectangle is given to the daemon so a click on it is never a step (GDE-1.AC2). It only calls
//! the daemon's actions (`start_guide`, `set_guide`, `stop_guide`) and polls `get_guide` for its
//! numbers.
//!
//! The bar is 58 px high and as wide as its content (at most 720 px and never wider than the
//! display less 48 px); `ui::bar_layout` holds the arithmetic and the collapse rule (labels turn
//! into icon buttons, then the second line hides).

use std::time::{Duration, Instant};

use cm_i18n::{tr, trn};
use gpui::{
    div, prelude::*, px, App, Context, Entity, Global, Render, Role, SharedString, Window,
    WindowHandle,
};
use serde_json::{json, Value};

use crate::backend::Backend;
use crate::recording::geometry::Rect;
use crate::theme::Theme;
use crate::toast::{ToastAction, ToastKind, ToastSpec};
use crate::ui::app::{self, AppModel, Route};
use crate::ui::bar_layout;
use crate::ui::components::{dot, icon, rec_dot};
use crate::ui::paint::{rgba_of, ui_features, TextExt, UI_FONT};
use crate::ui::recording::{harden, panel_options, pointer_display, visible_part};
use crate::ui::tooltip::tip;

/// The bar's height, points (design S17: never changes). Its width is the content's, see
/// [`bar_width`].
pub const BAR_H: f64 = bar_layout::GUIDE_BAR_H;
const MARGIN: f64 = 12.0;
/// The countdown before the first click is listened to, seconds.
const COUNTDOWN_S: u64 = 3;
/// The step number the width is measured with ("Step 999").
const WIDEST_STEP: usize = 999;

/// The open bar, if any (one guide records at a time).
#[derive(Default)]
struct Holder(Option<WindowHandle<GuideHud>>);
impl Global for Holder {}

/// What the bar shows, from `get_guide`.
#[derive(Debug, Clone, Default)]
struct Shown {
    guide_id: Option<String>,
    /// `countdown`, `recording` or `paused`.
    state: String,
    steps: usize,
    /// The followed window closed: the guide paused itself (design S16).
    window_closed: bool,
}

/// True when the poll shows that the guide this bar follows has ended (stopped from the CLI or an
/// agent, discarded or cancelled): the bar closes whichever client stopped it. `state` is the
/// reply's state (`None`: `get_guide` failed); `followed` is whether the bar already knows the
/// guide's id (before that, a failed read is only the countdown starting).
fn guide_has_ended(followed: bool, state: Option<&str>) -> bool {
    match state {
        Some("countdown" | "recording" | "paused") => false,
        Some(_) => true,
        None => followed,
    }
}

/// A `stop_guide` that failed only because the guide is no longer recording (another client
/// stopped it first): the bar closes without a toast.
fn stop_means_already_ended(message: &str) -> bool {
    message.contains("no Click Guide is recording") || message.contains("is not the one recording")
}

pub struct GuideHud {
    app: Entity<AppModel>,
    shown: Shown,
    /// The title of the followed window, when the guide follows one.
    follow: Option<String>,
    /// The widest the bar may be (S17): 720, and the display less 48.
    limit: f64,
    started: Instant,
    /// The "window closed" toast was shown (once per guide).
    closed_toast: bool,
    /// A guide started by an agent or the CLI: `Some(client)` shows the passive bar.
    passive: Option<Option<String>>,
}

/// What the bar can look like. The window is as wide as the widest of them, so it never moves or
/// resizes while the guide runs; a narrower state leaves its spare room in the text column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Countdown,
    Recording,
    Paused,
    Closed,
}

const MODES: [Mode; 4] = [Mode::Countdown, Mode::Recording, Mode::Paused, Mode::Closed];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Control {
    Pause,
    Note,
    Undo,
    Pick,
    ClosedLabel,
}

#[derive(Debug, Clone)]
struct SlotSpec {
    control: Control,
    glyph: &'static str,
    label: String,
}

/// The words of one state of the bar.
#[derive(Debug, Clone)]
struct Parts {
    headline: String,
    second: String,
    /// The width of the dot or spinner and its gap.
    mark_w: f64,
    slots: Vec<SlotSpec>,
}

const HEAD_SIZE: f32 = 15.0;
const SECOND_SIZE: f32 = 11.0;
const LABEL_SIZE: f32 = 13.0;

/// The second line: the followed window's name (cut at 28 characters) replaces "Click Guide",
/// and the line is cut at 40 characters (the name does not count against them).
fn second_line(mode: Mode, follow: Option<&str>) -> String {
    let (base, name_chars) = match follow {
        Some(name) => {
            let cut = bar_layout::ellipsize(name, bar_layout::WINDOW_NAME_CHARS);
            let n = cut.chars().count();
            (
                tr!(
                    "Following \u{201c}{window}\u{201d} \u{b7} clicks only",
                    window = cut
                ),
                n,
            )
        }
        None => (tr!("Click Guide \u{b7} clicks only"), 0),
    };
    let line = match mode {
        Mode::Countdown => tr!("Stop or Esc cancels"),
        Mode::Recording => base,
        Mode::Paused => tr!("Paused \u{b7} {rest}", rest = base),
        Mode::Closed => tr!("Paused \u{b7} the followed window closed"),
    };
    bar_layout::ellipsize(&line, bar_layout::SECOND_LINE_CHARS + name_chars)
}

/// The words of `mode`; `step` is the number the headline shows, `remaining` the countdown's.
fn parts(mode: Mode, step: usize, remaining: u64, follow: Option<&str>) -> Parts {
    let slot = |control, glyph, label: String| SlotSpec {
        control,
        glyph,
        label,
    };
    let (headline, mark_w) = match mode {
        Mode::Countdown => (
            format!("{} {remaining}", tr!("Starting\u{2026}")),
            16.0 + 8.0,
        ),
        _ => (tr!("Step {n}", n = step), 8.0 + 8.0),
    };
    let slots = match mode {
        Mode::Closed => vec![
            slot(Control::ClosedLabel, "", tr!("Window closed")),
            slot(Control::Pick, "app-window", tr!("Pick a window\u{2026}")),
        ],
        _ => vec![
            slot(
                Control::Pause,
                if mode == Mode::Paused {
                    "rec-play"
                } else {
                    "rec-pause"
                },
                if mode == Mode::Paused {
                    tr!("Resume")
                } else {
                    tr!("Pause")
                },
            ),
            slot(Control::Note, "file-text", tr!("Note step")),
            slot(Control::Undo, "undo-2", tr!("Undo step")),
        ],
    };
    Parts {
        headline,
        second: second_line(mode, follow),
        mark_w,
        slots,
    }
}

/// `mode` laid out within `limit` (design S17). The widths are measured with the worst-case step
/// number so the bar's width does not change as the count grows.
fn layout(mode: Mode, follow: Option<&str>, limit: f64) -> (Parts, bar_layout::GuideBar) {
    let p = parts(mode, WIDEST_STEP, COUNTDOWN_S, follow);
    let head_w = p.mark_w + bar_layout::text_w(&p.headline, HEAD_SIZE, true, 400);
    let second_w = bar_layout::text_w(&p.second, SECOND_SIZE, false, 400);
    let slots: Vec<bar_layout::Slot> = p
        .slots
        .iter()
        .map(|s| {
            let w = bar_layout::text_w(&s.label, LABEL_SIZE, false, 400);
            match s.control {
                Control::Pause => bar_layout::Slot::button(w, 2),
                Control::Note => bar_layout::Slot::button(w, 1),
                Control::Undo | Control::Pick => bar_layout::Slot::button(w, 0),
                Control::ClosedLabel => bar_layout::Slot::note(w),
            }
        })
        .collect();
    let bar = bar_layout::guide_bar(head_w, second_w, &slots, limit);
    (p, bar)
}

/// The window's width: the widest state, at most `limit` (S17).
pub fn bar_width(follow: Option<&str>, limit: f64) -> f64 {
    MODES
        .iter()
        .map(|m| layout(*m, follow, limit).1.width)
        .fold(0.0, f64::max)
}

/// The bar's rectangle: centred above the followed display's visible part, at the top.
pub fn bar_rect(visible: &Rect, width: f64) -> Rect {
    Rect::new(
        visible.x + (visible.w - width) / 2.0,
        visible.y + MARGIN,
        width,
        BAR_H,
    )
}

/// The bar's rectangle in the space the daemon's click events use (GDE-1.AC2): GPUI's points on
/// macOS, where the pointer is reported in points too, and physical px on Windows, where it is
/// not. `display_px` is the display's rectangle in physical px (`None` where points are the
/// OS's space); the scale is what the display's physical width makes of its width in points, so
/// a display at 150 % moves and grows the bar by 1.5, and a second monitor keeps its offset.
pub fn exclusion_in_os_space(
    bar: &Rect,
    display_pts: &Rect,
    display_px: Option<[i64; 4]>,
) -> [f64; 4] {
    let Some(px) = display_px else {
        return [bar.x, bar.y, bar.w, bar.h];
    };
    if display_pts.w <= 0.0 || display_pts.h <= 0.0 {
        return [bar.x, bar.y, bar.w, bar.h];
    }
    let (sx, sy) = (px[2] as f64 / display_pts.w, px[3] as f64 / display_pts.h);
    [
        px[0] as f64 + (bar.x - display_pts.x) * sx,
        px[1] as f64 + (bar.y - display_pts.y) * sy,
        bar.w * sx,
        bar.h * sy,
    ]
}

fn toast(app: &Entity<AppModel>, spec: ToastSpec, cx: &mut App) {
    app::push_toast(app, spec, cx);
}

/// A daemon event about a Click Guide that this app shows or acts on (CG-9).
#[derive(Debug, Clone, PartialEq, Eq)]
enum GuideEvent {
    /// 50 steps are left before the guide holds 500.
    NearLimit {
        remaining: u64,
    },
    /// The guide holds `max` steps and the daemon stops it, saved.
    LimitReached {
        guide_id: String,
        max: u64,
    },
    /// A guide started (`by` is `user` or `agent:<name>`).
    Started {
        guide_id: String,
        by: String,
    },
    /// A click was not saved.
    Error {
        reason: String,
    },
    /// The guide ended, whoever stopped it.
    Stopped {
        guide_id: String,
    },
    /// The followed window was moved or resized: the guide paused itself (BUG-5).
    WindowMoved {
        step_count: u64,
    },
    Other,
}

fn classify_event(name: &str, data: &Value) -> GuideEvent {
    let text = |k: &str| data[k].as_str().unwrap_or_default().to_owned();
    match name {
        "guide.near_limit" => GuideEvent::NearLimit {
            remaining: data["remaining"].as_u64().unwrap_or(50),
        },
        "guide.limit_reached" => GuideEvent::LimitReached {
            guide_id: text("guide_id"),
            max: data["max"].as_u64().unwrap_or(500),
        },
        "guide.error" => GuideEvent::Error {
            reason: if data["code"] == "busy" {
                tr!("CaptureMann is still saving the earlier clicks")
            } else {
                text("message")
            },
        },
        "guide.started" => GuideEvent::Started {
            guide_id: text("guide_id"),
            by: text("by"),
        },
        "guide.stopped" => GuideEvent::Stopped {
            guide_id: text("guide_id"),
        },
        "guide.window_moved" => GuideEvent::WindowMoved {
            step_count: data["step_count"].as_u64().unwrap_or(0),
        },
        _ => GuideEvent::Other,
    }
}

/// The error toast is shown at most once in this time: a run of failed clicks is one message.
const ERROR_TOAST_EVERY: Duration = Duration::from_secs(10);

/// Whether an error toast is due now; remembers the time when it is.
fn error_toast_due(last: &mut Option<Instant>, now: Instant) -> bool {
    if last.is_some_and(|t| now.saturating_duration_since(t) < ERROR_TOAST_EVERY) {
        return false;
    }
    *last = Some(now);
    true
}

static LAST_ERROR_TOAST: std::sync::Mutex<Option<Instant>> = std::sync::Mutex::new(None);

/// Whether the bar, which follows `followed` (`None` before the guide's id is known), closes for
/// the end of guide `ended`.
fn bar_closes_for(followed: Option<&str>, ended: &str) -> bool {
    followed.is_none_or(|f| f == ended)
}

/// A `guide.*` event of the daemon (any client's guide): the 450-step notice, the 500-step stop,
/// a click that was not saved, and the end of the guide (the bar closes whoever stopped it).
pub fn on_event(app: &Entity<AppModel>, name: &str, data: &Value, cx: &mut App) {
    match classify_event(name, data) {
        GuideEvent::NearLimit { remaining } => toast(
            app,
            ToastSpec::new(ToastKind::Warning, tr!("{n} steps left", n = remaining))
                .body(tr!("Stop and start a new guide to keep going.")),
            cx,
        ),
        GuideEvent::LimitReached { guide_id, max } => {
            let mut spec = ToastSpec::new(
                ToastKind::Warning,
                tr!("The guide reached {max} steps and was saved.", max = max),
            )
            .body(tr!("Start a new guide to keep going."));
            if !guide_id.is_empty() {
                spec = spec.button(tr!("Open"), ToastAction::GuideStopOpen(guide_id));
            }
            toast(app, spec, cx);
        }
        GuideEvent::Error { reason } => {
            let due = {
                let mut last = LAST_ERROR_TOAST
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                error_toast_due(&mut last, Instant::now())
            };
            if due && !reason.is_empty() {
                toast(
                    app,
                    ToastSpec::new(
                        ToastKind::Warning,
                        tr!("A click was not saved: {reason}", reason = reason),
                    ),
                    cx,
                );
            }
        }
        GuideEvent::WindowMoved { step_count } => toast(
            app,
            ToastSpec::new(
                ToastKind::Warning,
                tr!("The window moved. The Click Guide is paused."),
            )
            .body(trn!(
                "Your {n} step is kept. Put the window where you want it, then resume.",
                "Your {n} steps are kept. Put the window where you want it, then resume.",
                step_count
            )),
            cx,
        ),
        GuideEvent::Stopped { guide_id } => {
            let Some(hud) = cx.try_global::<Holder>().and_then(|h| h.0) else {
                return;
            };
            let followed = hud
                .update(cx, |this, _, _| this.shown.guide_id.clone())
                .ok()
                .flatten();
            if bar_closes_for(followed.as_deref(), &guide_id) {
                close(cx);
            }
        }
        GuideEvent::Started { guide_id, by } => {
            // CG-2: a guide this app did not start (an agent, the CLI) gets a passive bar too,
            // so a person can always see it and stop it. The app's own start opened its bar
            // before the daemon answered, so a bar that is already open is left alone.
            if passive_for(is_running(cx), &guide_id, &by).is_some() {
                open_passive(app, guide_id, &by, cx);
            }
        }
        GuideEvent::Other => {}
    }
}

/// The width of the passive bar (Stop, "Step n", the agent line and the Agent chip), points,
/// never wider than `limit`.
const PASSIVE_W: f64 = 440.0;

pub fn passive_bar_width(limit: f64) -> f64 {
    PASSIVE_W.min(limit)
}

/// The client of a guide `by` names: `agent:capturemann-cli` is `capturemann-cli`; the user's own
/// (`user`) and an empty name are `None`.
fn passive_client(by: &str) -> Option<String> {
    by.strip_prefix("agent:")
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .map(str::to_owned)
}

/// Whether a `guide.started` event asks for the passive bar (BUG-1): when no bar is open (the
/// app's own start opened its bar before the daemon answered) and the event names a guide. The
/// value is the client to name (`None` inside: say "an agent or the command line").
fn passive_for(bar_open: bool, guide_id: &str, by: &str) -> Option<Option<String>> {
    (!bar_open && !guide_id.is_empty()).then(|| passive_client(by))
}

/// The agent line of the passive bar and the chip's accessible name.
fn passive_line(client: Option<&str>) -> String {
    match client {
        Some(c) => tr!("Started by agent {client}", client = c),
        None => tr!("Started by an agent or the command line"),
    }
}

/// The bar for a guide started elsewhere (CG-2): Stop, the step count and an Agent chip; no Pause,
/// Note or Undo. It follows the display under the pointer and keeps its own rectangle clear of
/// clicks, like the full bar.
fn open_passive(app: &Entity<AppModel>, guide_id: String, by: &str, cx: &mut App) {
    let backend = app.read(cx).backend.clone();
    let Some(daemon) = backend.daemon.clone() else {
        return;
    };
    let Some(display) = pointer_display(cx) else {
        return;
    };
    let visible = visible_part(&display.1);
    let limit = bar_layout::guide_bar_limit(visible.w);
    let rect = bar_rect(&visible, passive_bar_width(limit));
    let gpui_id = u32::try_from(u64::from(display.0)).unwrap_or(1);
    let client = passive_client(by);
    let app2 = app.clone();
    let h = cx
        .open_window(
            panel_options(display, rect, false, false),
            move |window, cx| {
                harden(window, false);
                cx.new(|cx| GuideHud::new_passive(app2, guide_id, client, cx))
            },
        )
        .ok();
    let Some(h) = h else { return };
    crate::surfaces::opened(crate::surfaces::Surface::GuideHud, h);
    cx.set_global(Holder(Some(h)));
    notify_studio(cx);
    let display_px = crate::platform_window::gpui_monitor_rects_px()
        .and_then(|v| v.into_iter().find(|(id, _)| *id == gpui_id).map(|(_, r)| r));
    daemon.set_guide_exclusions(vec![exclusion_in_os_space(&rect, &display.1, display_px)]);
}

/// "New Click Guide": follows the display under the pointer, shows the bar and starts the guide
/// (a 3 s countdown first). Nothing is recorded until the countdown ends; Stop during it cancels.
pub fn start(app: &Entity<AppModel>, cx: &mut App) {
    start_on(app, None, cx);
}

/// "Click Guide of a window": the same bar and countdown, following one window the person chose
/// in the window picker (`title` is its title, for the bar's second line); only clicks inside it
/// become steps (the daemon filters them).
pub fn start_window(app: &Entity<AppModel>, window_id: u32, title: String, cx: &mut App) {
    start_on(app, Some((window_id, title)), cx);
}

/// "Click Guide of a window...": the window picker (the screenshot's, with a "Start guide"
/// button); the chosen window starts the guide through [`start_window`]. The permission gate
/// is checked first, so no window is offered when the guide could not start anyway.
pub fn pick_window(app: &Entity<AppModel>, cx: &mut App) {
    if cx.try_global::<Holder>().is_some_and(|h| h.0.is_some()) {
        return;
    }
    if let super::preflight::Gate::Unavailable(reason) = super::preflight::current() {
        toast(
            app,
            ToastSpec::new(ToastKind::Warning, tr!("Click Guide is not available here"))
                .body(reason),
            cx,
        );
        return;
    }
    crate::ui::window_picker::open_for(app, crate::ui::window_picker::Purpose::ClickGuide, cx);
}

/// The `target` of `start_guide` for a chosen window.
pub fn window_target(window_id: u32) -> Value {
    json!({ "window_id": window_id })
}

fn start_on(app: &Entity<AppModel>, window: Option<(u32, String)>, cx: &mut App) {
    if cx.try_global::<Holder>().is_some_and(|h| h.0.is_some()) {
        return;
    }
    let backend = app.read(cx).backend.clone();
    let Some(daemon) = backend.daemon.clone() else {
        toast(
            app,
            ToastSpec::new(ToastKind::Error, tr!("Click Guide is unavailable"))
                .body(tr!("CaptureMann has no library to save the guide in.")),
            cx,
        );
        return;
    };
    // GDE-1.AC5, AC6: the permission card or the reason, before anything is shown or recorded.
    match super::preflight::current() {
        super::preflight::Gate::Ready => {}
        super::preflight::Gate::NeedsInputMonitoring => {
            let again = window.clone();
            super::card::open(app, move |app, cx| start_on(app, again.clone(), cx), cx);
            return;
        }
        super::preflight::Gate::Unavailable(reason) => {
            toast(
                app,
                ToastSpec::new(ToastKind::Warning, tr!("Click Guide is not available here"))
                    .body(reason),
                cx,
            );
            return;
        }
    }
    let Some(display) = pointer_display(cx) else {
        return;
    };
    let visible = visible_part(&display.1);
    let follow = window.as_ref().map(|(_, t)| t.clone());
    let limit = bar_layout::guide_bar_limit(visible.w);
    // The bar is as wide as its content, and the exclusion below is the same rectangle.
    let rect = bar_rect(&visible, bar_width(follow.as_deref(), limit));
    let window_id = window.map(|(id, _)| id);
    let gpui_id = u32::try_from(u64::from(display.0)).unwrap_or(1);
    let (app2, follow2) = (app.clone(), follow);
    let h = cx
        .open_window(
            panel_options(display, rect, false, false),
            move |window, cx| {
                harden(window, false);
                cx.new(|cx| GuideHud::new(app2, follow2, limit, cx))
            },
        )
        .ok();
    let Some(h) = h else { return };
    crate::surfaces::opened(crate::surfaces::Surface::GuideHud, h);
    cx.set_global(Holder(Some(h)));
    notify_studio(cx); // the sidebar tile shows the run
                       // Esc cancels the countdown (the bar takes no focus, so Esc is a global key until it ends).
    crate::ui::delay_pill::grab_escape(cx);
    // Clicks on the bar are not steps (the bar's own rectangle, in the OS's global space).
    // (Windows reports clicks in physical px: the rectangle is converted with the display's scale.)
    let display_px = crate::platform_window::gpui_monitor_rects_px()
        .and_then(|v| v.into_iter().find(|(id, _)| *id == gpui_id).map(|(_, r)| r));
    daemon.set_guide_exclusions(vec![exclusion_in_os_space(&rect, &display.1, display_px)]);

    let (app3, backend2) = (app.clone(), backend.clone());
    cx.spawn(async move |cx| {
        let started = backend2
            .rt
            .spawn(async move {
                if let Some(id) = window_id {
                    return Backend::call_coded(
                        &daemon,
                        "start_guide",
                        json!({ "target": window_target(id), "countdown_s": COUNTDOWN_S }),
                    )
                    .await;
                }
                let mut display_id = u64::from(gpui_id);
                // GPUI's display id is the engine's only where they agree; the rectangles find
                // the engine's id elsewhere (the same mapping as the delayed capture).
                let listed = Backend::call(&daemon, "list_targets", json!({ "windows": false }))
                    .await
                    .map(|v| crate::recording::monitors::parse_engine_displays(&v))
                    .unwrap_or_default();
                let rects = crate::platform_window::gpui_monitor_rects_px().map(|v| {
                    v.into_iter()
                        .map(|(id, r)| {
                            (
                                id,
                                Rect::new(r[0] as f64, r[1] as f64, r[2] as f64, r[3] as f64),
                            )
                        })
                        .collect::<Vec<_>>()
                });
                if let Some(id) = crate::recording::monitors::engine_display_id(
                    gpui_id,
                    rects.as_deref(),
                    &listed,
                ) {
                    display_id = u64::from(id);
                }
                Backend::call_coded(
                    &daemon,
                    "start_guide",
                    json!({ "target": { "display_id": display_id }, "countdown_s": COUNTDOWN_S }),
                )
                .await
            })
            .await;
        cx.update(|cx| match started {
            Ok(Ok(v)) if v["state"] == "cancelled" => close(cx),
            Ok(Ok(_)) => {}
            Ok(Err(e)) => {
                close(cx);
                let spec = if e.permission.is_some() {
                    ToastSpec::new(
                        ToastKind::Warning,
                        tr!("Click Guide needs Input Monitoring"),
                    )
                    .body(format!(
                        "{} {}",
                        tr!("It never records what you type."),
                        crate::daemon_text::call_error_text(&e)
                    ))
                } else {
                    crate::flows::error_toast(
                        &tr!("Click Guide did not start"),
                        crate::daemon_text::call_error_text(&e),
                    )
                };
                toast(&app3, spec, cx);
            }
            Err(_) => close(cx),
        });
    })
    .detach();
}

/// Whether the guide bar is open (the countdown or the run).
pub fn is_running(cx: &App) -> bool {
    cx.try_global::<Holder>().is_some_and(|h| h.0.is_some())
}

/// The sidebar tile of a running guide: brings the bar to the front.
pub fn bring_to_front(cx: &mut App) {
    let Some(hud) = cx.try_global::<Holder>().and_then(|h| h.0) else {
        return;
    };
    let _ = hud.update(cx, |_, window, cx| {
        crate::surfaces::bring_to_front(cx, window, None, "click guide tile");
    });
}

/// Closes the bar.
pub fn close(cx: &mut App) {
    crate::ui::delay_pill::release_escape(cx);
    if let Some(h) = cx.try_global::<Holder>().and_then(|h| h.0) {
        crate::surfaces::close(cx, h);
    }
    cx.set_global(Holder(None));
    notify_studio(cx);
}

/// Redraws the Studio, so the sidebar's Click Guide tile follows the run (board S14).
fn notify_studio(cx: &mut App) {
    if let Some(app) = cx.try_global::<app::AppHandle>().map(|h| h.0.clone()) {
        app.update(cx, |_, cx| cx.notify());
    }
}

/// What happens after Stop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum After {
    /// Open the finished guide in the editor (Stop on the bar).
    Open,
    /// Pick another window for a new guide (the guide that ended is kept).
    Pick,
}

/// What Stop does with its result: which toast to show and where to go. Pure, so the three
/// outcomes are tested without a daemon (board QA C2).
#[derive(Debug, Clone, PartialEq, Eq)]
enum StopOutcome {
    /// The guide ended with no steps and was not kept.
    Discarded,
    /// The guide is saved: open it, or only pick the next window.
    Saved(String),
    /// Another client stopped it first: nothing to say.
    AlreadyEnded,
    /// The daemon refused: the message for the error toast.
    Failed(String),
    /// The call itself did not complete.
    Lost,
}

fn stop_outcome(r: &Result<Result<Value, String>, ()>) -> StopOutcome {
    match r {
        Ok(Err(e)) if stop_means_already_ended(e) => StopOutcome::AlreadyEnded,
        Ok(Ok(v)) if v["discarded"] == true => StopOutcome::Discarded,
        Ok(Ok(v)) => v["guide_id"]
            .as_str()
            .map_or(StopOutcome::AlreadyEnded, |id| {
                StopOutcome::Saved(id.to_owned())
            }),
        Ok(Err(e)) => StopOutcome::Failed(e.clone()),
        Err(()) => StopOutcome::Lost,
    }
}

/// Stop, whoever asked (the bar's button, the countdown's Esc, the window-closed toast): ends
/// the guide in the daemon, then closes the bar (which refreshes the sidebar tile) and opens the
/// guide's editor or the window picker. It does not need the bar's window, so a toast button
/// works when the bar cannot be updated.
fn stop_flow(app: &Entity<AppModel>, after: After, cx: &mut App) {
    let backend = app.read(cx).backend.clone();
    let Some(daemon) = backend.daemon.clone() else {
        close(cx);
        return;
    };
    let app = app.clone();
    cx.spawn(async move |cx| {
        let r = backend
            .rt
            .spawn(async move { Backend::call(&daemon, "stop_guide", json!({})).await })
            .await
            .map_err(|_| ());
        cx.update(|cx| {
            close(cx);
            match stop_outcome(&r) {
                StopOutcome::Discarded => toast(
                    &app,
                    ToastSpec::new(ToastKind::Info, tr!("No steps were taken"))
                        .body(tr!("The empty guide was not kept.")),
                    cx,
                ),
                StopOutcome::Saved(id) => {
                    if after == After::Open {
                        app::navigate(&app, Route::Guide(id), cx);
                    }
                }
                StopOutcome::Failed(e) => toast(
                    &app,
                    crate::flows::error_toast(&tr!("The guide did not stop cleanly"), e),
                    cx,
                ),
                StopOutcome::AlreadyEnded | StopOutcome::Lost => {}
            }
            if after == After::Pick {
                pick_window(&app, cx);
            }
        });
    })
    .detach();
}

/// Esc during the countdown cancels the guide. Returns whether it did (the Esc key is shared with
/// the delay pill and Long Shot, which are asked when this says no).
pub fn cancel_countdown(cx: &mut App) -> bool {
    let Some(h) = cx.try_global::<Holder>().and_then(|h| h.0) else {
        return false;
    };
    let app = h
        .update(cx, |this, _, _| {
            (this.shown.state == "countdown").then(|| this.app.clone())
        })
        .ok()
        .flatten();
    match app {
        Some(app) => {
            stop_flow(&app, After::Open, cx);
            true
        }
        None => false,
    }
}

/// The toast's "Pick another window": the guide that lost its window is stopped and kept, and
/// the window picker opens for a new one.
pub fn pick_another(app: &Entity<AppModel>, cx: &mut App) {
    if is_running(cx) {
        stop_flow(app, After::Pick, cx);
    } else {
        pick_window(app, cx);
    }
}

/// The agent toast's "Stop" (BUG-1): ends the guide an agent or the CLI started and opens it;
/// nothing when no guide is recording any more.
pub fn stop_running(app: &Entity<AppModel>, cx: &mut App) {
    if is_running(cx) {
        stop_flow(app, After::Open, cx);
    }
}

/// The toast's "Stop and open guide". When the bar is gone already (Stop was pressed there), the
/// guide is simply opened.
pub fn stop_and_open(app: &Entity<AppModel>, guide_id: &str, cx: &mut App) {
    if is_running(cx) {
        stop_flow(app, After::Open, cx);
    } else {
        app::navigate(app, Route::Guide(guide_id.to_owned()), cx);
    }
}

impl GuideHud {
    fn new(
        app: Entity<AppModel>,
        follow: Option<String>,
        limit: f64,
        cx: &mut Context<Self>,
    ) -> Self {
        // The numbers come from the daemon twice a second.
        cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(500))
                .await;
            let Ok(Some((backend, id))) = this.update(cx, |this, cx| {
                this.app.read(cx).backend.daemon.clone().map(|_| {
                    (
                        this.app.read(cx).backend.clone(),
                        this.shown.guide_id.clone(),
                    )
                })
            }) else {
                break;
            };
            let Some(daemon) = backend.daemon.clone() else {
                break;
            };
            let id_known = id.is_some();
            let v = backend
                .rt
                .spawn(async move {
                    let p = id.map_or_else(|| json!({}), |i| json!({ "guide_id": i }));
                    Backend::call(&daemon, "get_guide", p).await.ok()
                })
                .await
                .ok()
                .flatten();
            let ended = guide_has_ended(
                id_known,
                v.as_ref().map(|v| v["state"].as_str().unwrap_or_default()),
            );
            if ended {
                cx.update(close);
                break;
            }
            if let Some(v) = v {
                let _ = this.update(cx, |this, cx| {
                    let was_counting = this.shown.state == "countdown";
                    this.shown = Shown {
                        guide_id: v["guide_id"].as_str().map(str::to_owned),
                        state: v["state"].as_str().unwrap_or_default().to_owned(),
                        steps: usize::try_from(
                            v["recorded_steps"]
                                .as_u64()
                                .or(v["step_count"].as_u64())
                                .unwrap_or(0),
                        )
                        .unwrap_or(0),
                        window_closed: v["window_closed"].as_bool().unwrap_or(false),
                    };
                    if was_counting && this.shown.state != "countdown" {
                        crate::ui::delay_pill::release_escape(cx);
                    }
                    if this.shown.window_closed && !this.closed_toast {
                        this.closed_toast = true;
                        this.tell_window_closed(cx);
                    }
                    cx.notify();
                });
            }
        })
        .detach();
        GuideHud {
            app,
            shown: Shown {
                state: "countdown".into(),
                ..Shown::default()
            },
            follow,
            limit,
            started: Instant::now(),
            closed_toast: false,
            passive: None,
        }
    }

    /// The bar of a guide an agent or the CLI started: no countdown, the guide's id is known.
    fn new_passive(
        app: Entity<AppModel>,
        guide_id: String,
        client: Option<String>,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut hud = Self::new(app, None, PASSIVE_W, cx);
        hud.shown = Shown {
            guide_id: Some(guide_id),
            state: "recording".into(),
            ..Shown::default()
        };
        hud.passive = Some(client);
        hud
    }

    /// The toast of design S16: the window closed, the guide paused, nothing lost.
    fn tell_window_closed(&self, cx: &mut Context<Self>) {
        let name = self
            .follow
            .clone()
            .unwrap_or_else(|| tr!("The followed window"));
        let n = self.shown.steps;
        let mut spec = ToastSpec::new(ToastKind::Warning, tr!("{window} closed", window = name))
            .body(trn!(
                "The Click Guide is paused. Your {n} step is kept.",
                "The Click Guide is paused. Your {n} steps are kept.",
                n
            ))
            .button(tr!("Pick another window"), ToastAction::GuidePickWindow);
        if let Some(id) = &self.shown.guide_id {
            spec = spec.button(
                tr!("Stop and open guide"),
                ToastAction::GuideStopOpen(id.clone()),
            );
        }
        toast(&self.app, spec, cx);
    }

    fn act(&mut self, method: &'static str, params: Value, cx: &mut Context<Self>) {
        let backend = self.app.read(cx).backend.clone();
        let Some(daemon) = backend.daemon.clone() else {
            return;
        };
        cx.spawn(async move |_, _| {
            let _ = backend
                .rt
                .spawn(async move { Backend::call(&daemon, method, params).await })
                .await;
        })
        .detach();
    }

    /// Stop: closes the bar and, by `after`, opens the guide or the window picker.
    fn stop(&mut self, after: After, cx: &mut Context<Self>) {
        let app = self.app.clone();
        cx.defer(move |cx| stop_flow(&app, after, cx));
    }

    fn mode(&self) -> Mode {
        match (self.shown.state.as_str(), self.shown.window_closed) {
            ("countdown", _) => Mode::Countdown,
            ("paused", true) => Mode::Closed,
            ("paused", false) => Mode::Paused,
            _ => Mode::Recording,
        }
    }

    /// The countdown's number: 3, 2, 1 from when the bar opened.
    fn remaining(&self) -> u64 {
        COUNTDOWN_S
            .saturating_sub(self.started.elapsed().as_secs())
            .max(1)
    }
}

/// One control of the bar: 44 px high, its label beside a 16 px icon, or only the icon (a
/// collapsed control keeps the label as its tooltip and its accessible name).
fn hud_button(
    t: &Theme,
    id: &'static str,
    glyph: &'static str,
    label: String,
    collapsed: bool,
    disabled: bool,
) -> gpui::Stateful<gpui::Div> {
    let hover = rgba_of(t.raised);
    let fg = if disabled { t.text_3 } else { t.text_2 };
    div()
        .id(id)
        .role(Role::Button)
        .aria_label(SharedString::from(label.clone()))
        .h(px(bar_layout::BTN_H as f32))
        .flex_none()
        .whitespace_nowrap()
        .flex()
        .items_center()
        .justify_center()
        .gap(px(bar_layout::ICON_GAP as f32))
        .rounded(px(11.))
        .text_size(px(LABEL_SIZE))
        .text_color(rgba_of(fg))
        .when(!disabled, |d| {
            d.cursor_pointer().hover(move |s| s.bg(hover))
        })
        .when(disabled, |d| d.opacity(0.4))
        .when(collapsed, |d| {
            d.w(px(bar_layout::BTN_H as f32))
                .tooltip(tip(label.clone(), Vec::new()))
        })
        .when(!collapsed, |d| d.px(px(bar_layout::BTN_PAD as f32)))
        .child(icon(glyph, bar_layout::ICON as f32, fg))
        .when(!collapsed, |d| d.child(SharedString::from(label)))
}

impl Render for GuideHud {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = Theme::overlay();
        if let Some(client) = self.passive.clone() {
            return self.render_passive(&t, client, cx).into_any_element();
        }
        let mode = self.mode();
        let (_, bar) = layout(mode, self.follow.as_deref(), self.limit);
        let p = parts(
            mode,
            self.shown.steps + 1,
            self.remaining(),
            self.follow.as_deref(),
        );
        let counting = mode == Mode::Countdown;
        let mark = match mode {
            Mode::Countdown => div()
                .size(px(16.))
                .flex_none()
                .rounded(px(8.))
                .border_2()
                .border_color(rgba_of(t.rec))
                .into_any_element(),
            Mode::Recording => rec_dot(&t, 8.).into_any_element(),
            // Paused: the dot goes quiet (mist); a window that closed shows amber.
            Mode::Paused => dot(t.mist, 8.).into_any_element(),
            Mode::Closed => dot(t.warning, 8.).into_any_element(),
        };
        let mut text = div()
            .flex_1()
            .min_w(px(0.))
            .flex()
            .flex_col()
            .px(px(bar_layout::TEXT_PAD as f32))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .whitespace_nowrap()
                    .child(mark)
                    .child(
                        div()
                            .mono()
                            .text_size(px(HEAD_SIZE))
                            .text_color(rgba_of(t.text))
                            .child(SharedString::from(p.headline.clone())),
                    ),
            );
        if bar.second {
            let mut second = div()
                .id("guide-hud-second")
                .whitespace_nowrap()
                .text_size(px(SECOND_SIZE))
                .text_color(rgba_of(t.text_3));
            // The followed window's full title is the tooltip of the second line.
            if let Some(full) = self.follow.clone() {
                second = second.tooltip(tip(full, Vec::new()));
            }
            text = text.child(second.child(SharedString::from(p.second.clone())));
        }
        let mut root = div()
            .id("guide-hud")
            .role(Role::Toolbar)
            .aria_label(tr!("Click Guide"))
            .size_full()
            .flex()
            .items_center()
            .gap(px(bar_layout::GAP as f32))
            .px(px(bar_layout::PAD as f32))
            .rounded(px(16.))
            .border_1()
            .border_color(rgba_of(t.line))
            .bg(rgba_of(t.surface))
            .font_family(UI_FONT)
            .font_features(ui_features())
            .child(
                div()
                    .id("guide-hud-stop")
                    .role(Role::Button)
                    .aria_label(tr!("Stop"))
                    .size(px(bar_layout::STOP as f32))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(11.))
                    .bg(rgba_of(t.rec))
                    .cursor_pointer()
                    .child(div().size(px(14.)).rounded(px(3.)).bg(rgba_of(t.on_rec)))
                    .on_click(cx.listener(|this, _, _, cx| this.stop(After::Open, cx))),
            )
            .child(text)
            .child(div().w(px(1.)).h(px(26.)).flex_none().bg(rgba_of(t.line)));
        let paused = mode == Mode::Paused;
        for (spec, collapsed) in p.slots.iter().zip(bar.collapsed.iter().copied()) {
            let child = match spec.control {
                Control::ClosedLabel => div()
                    .id("guide-hud-closed")
                    .flex_none()
                    .px(px(bar_layout::TEXT_PAD as f32))
                    .whitespace_nowrap()
                    .text_size(px(LABEL_SIZE))
                    .text_color(rgba_of(t.text_3))
                    .child(SharedString::from(spec.label.clone()))
                    .into_any_element(),
                Control::Pause => hud_button(
                    &t,
                    "guide-hud-pause",
                    spec.glyph,
                    spec.label.clone(),
                    collapsed,
                    counting,
                )
                .when(!counting, |b| {
                    b.on_click(cx.listener(move |this, _, _, cx| {
                        this.act("set_guide", json!({ "paused": !paused }), cx);
                    }))
                })
                .into_any_element(),
                Control::Note => hud_button(
                    &t,
                    "guide-hud-note",
                    spec.glyph,
                    spec.label.clone(),
                    collapsed,
                    counting,
                )
                .when(!counting, |b| {
                    b.on_click(cx.listener(|this, _, _, cx| {
                        this.act(
                            "set_guide",
                            json!({ "note_step": { "title": tr!("New note step") } }),
                            cx,
                        );
                    }))
                })
                .into_any_element(),
                Control::Undo => hud_button(
                    &t,
                    "guide-hud-undo",
                    spec.glyph,
                    spec.label.clone(),
                    collapsed,
                    counting,
                )
                .when(!counting, |b| {
                    b.on_click(cx.listener(|this, _, _, cx| {
                        this.act("set_guide", json!({ "remove_last": true }), cx);
                    }))
                })
                .into_any_element(),
                Control::Pick => hud_button(
                    &t,
                    "guide-hud-pick",
                    spec.glyph,
                    spec.label.clone(),
                    collapsed,
                    false,
                )
                .on_click(cx.listener(|this, _, _, cx| this.stop(After::Pick, cx)))
                .into_any_element(),
            };
            root = root.child(child);
        }
        root.into_any_element()
    }
}

impl GuideHud {
    /// The bar of a guide started elsewhere: Stop, "Step n" with the agent line, the Agent chip.
    fn render_passive(
        &self,
        t: &Theme,
        client: Option<String>,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let mode = self.mode();
        let p = parts(mode, self.shown.steps + 1, self.remaining(), None);
        let line = passive_line(client.as_deref());
        let mark = match mode {
            Mode::Paused | Mode::Closed => dot(t.mist, 8.).into_any_element(),
            _ => rec_dot(t, 8.).into_any_element(),
        };
        div()
            .id("guide-hud")
            .role(Role::Toolbar)
            .aria_label(tr!("Click Guide"))
            .size_full()
            .flex()
            .items_center()
            .gap(px(bar_layout::GAP as f32))
            .px(px(bar_layout::PAD as f32))
            .rounded(px(16.))
            .border_1()
            .border_color(rgba_of(t.line))
            .bg(rgba_of(t.surface))
            .font_family(UI_FONT)
            .font_features(ui_features())
            .child(
                div()
                    .id("guide-hud-stop")
                    .role(Role::Button)
                    .aria_label(tr!("Stop"))
                    .size(px(bar_layout::STOP as f32))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(11.))
                    .bg(rgba_of(t.rec))
                    .cursor_pointer()
                    .child(div().size(px(14.)).rounded(px(3.)).bg(rgba_of(t.on_rec)))
                    .on_click(cx.listener(|this, _, _, cx| this.stop(After::Open, cx))),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .flex_col()
                    .px(px(bar_layout::TEXT_PAD as f32))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .whitespace_nowrap()
                            .child(mark)
                            .child(
                                div()
                                    .mono()
                                    .text_size(px(HEAD_SIZE))
                                    .text_color(rgba_of(t.text))
                                    .child(SharedString::from(p.headline)),
                            ),
                    )
                    .child(
                        div()
                            .id("guide-hud-second")
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_size(px(SECOND_SIZE))
                            .text_color(rgba_of(t.text_3))
                            .child(SharedString::from(line.clone())),
                    ),
            )
            .child(
                div()
                    .id("guide-hud-agent-chip")
                    .role(Role::Status)
                    .aria_label(SharedString::from(line.clone()))
                    .tooltip(tip(line, Vec::new()))
                    .flex_none()
                    .child(crate::ui::kit::pill(
                        t,
                        crate::ui::kit::Tone::Agent,
                        Some(crate::recording::hud_layout::AGENT_ICON),
                        crate::ui::long_shot::bar::agent_chip_text(),
                    )),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::bar_layout::{guide_bar_limit, GUIDE_BAR_H, GUIDE_BAR_MAX_W};
    use cm_i18n::{with_language, Lang};

    /// QA C2: what Stop does with the daemon's answer, for the bar's Stop and for the
    /// window-closed toast's two buttons alike (they share one flow that does not need the bar's
    /// window): a saved guide is opened (or the picker follows), an empty one is reported, and
    /// a guide another client stopped first only closes the bar.
    #[test]
    fn c2_stop_outcomes() {
        assert_eq!(
            stop_outcome(&Ok(Ok(
                json!({ "guide_id": "guide_7", "state": "stopped" })
            ))),
            StopOutcome::Saved("guide_7".into())
        );
        assert_eq!(
            stop_outcome(&Ok(Ok(json!({ "discarded": true })))),
            StopOutcome::Discarded
        );
        assert_eq!(
            stop_outcome(&Ok(Err("no Click Guide is recording".into()))),
            StopOutcome::AlreadyEnded
        );
        assert_eq!(
            stop_outcome(&Ok(Err("disk full".into()))),
            StopOutcome::Failed("disk full".into())
        );
        assert_eq!(stop_outcome(&Err(())), StopOutcome::Lost);
    }

    /// A window guide sends the window's id as the target, not a display (GDE-1).
    #[test]
    fn a_window_guide_targets_the_window() {
        assert_eq!(window_target(42), json!({ "window_id": 42 }));
    }

    /// GDE-1.AC2: on a 150 % Windows display the bar's rectangle is given in physical px, so a
    /// click on the bar is never a step; on macOS (no px rectangle) it stays in points.
    #[test]
    fn the_exclusion_is_in_physical_px_on_a_scaled_display() {
        let bar = Rect::new(500.0, 12.0, 470.0, 64.0);
        let display = Rect::new(0.0, 0.0, 1280.0, 720.0);
        let px = [0, 0, 1920, 1080];
        assert_eq!(
            exclusion_in_os_space(&bar, &display, Some(px)),
            [750.0, 18.0, 705.0, 96.0]
        );
        assert_eq!(
            exclusion_in_os_space(&bar, &display, None),
            [500.0, 12.0, 470.0, 64.0]
        );
    }

    /// A second monitor keeps its physical origin and its own scale (here 200 %).
    #[test]
    fn a_second_monitor_keeps_its_offset_and_scale() {
        let display = Rect::new(1280.0, 0.0, 960.0, 540.0);
        let bar = Rect::new(1280.0 + 245.0, 12.0, 470.0, 64.0);
        let px = [1920, 0, 1920, 1080];
        assert_eq!(
            exclusion_in_os_space(&bar, &display, Some(px)),
            [1920.0 + 490.0, 24.0, 940.0, 128.0]
        );
    }

    /// An empty display rectangle must not divide by zero.
    #[test]
    fn an_empty_display_falls_back_to_the_points() {
        let bar = Rect::new(1.0, 2.0, 3.0, 4.0);
        let d = Rect::new(0.0, 0.0, 0.0, 0.0);
        assert_eq!(
            exclusion_in_os_space(&bar, &d, Some([0, 0, 100, 100])),
            [1.0, 2.0, 3.0, 4.0]
        );
    }

    /// S17: the bar is centred on the display and its rectangle (the exclusion the daemon gets)
    /// is as wide as the bar, whatever the language made it.
    #[test]
    fn the_rectangle_follows_the_measured_width() {
        let visible = Rect::new(0.0, 0.0, 1440.0, 900.0);
        for lang in [Lang::En, Lang::Vi, Lang::EnXa] {
            with_language(lang, || {
                let w = bar_width(None, guide_bar_limit(visible.w));
                let r = bar_rect(&visible, w);
                assert_eq!(r.w, w);
                assert_eq!(r.h, GUIDE_BAR_H);
                assert!((r.x * 2.0 + r.w - visible.w).abs() < 1e-6, "centred");
            });
        }
    }

    /// S17 test hooks: height 58, width at most min(720, display - 48), every state of every
    /// language, and the same again with the text 40 % longer (the pseudo-length). The collapse
    /// order is Undo step, then Note step, then Pause or Resume, then the second line.
    #[test]
    fn s17_the_bar_fits_in_every_language_and_display() {
        for display in [1920.0, 1440.0, 1024.0, 800.0, 640.0, 520.0] {
            let limit = guide_bar_limit(display);
            assert!(limit <= GUIDE_BAR_MAX_W.min(display - 48.0) + 1e-9);
            for lang in [Lang::En, Lang::Vi, Lang::EnXa] {
                with_language(lang, || {
                    for follow in [None, Some("Brightdesk \u{2014} Chrome")] {
                        for mode in MODES {
                            let (_, bar) = layout(mode, follow, limit);
                            assert!(
                                bar.width <= limit + 1e-6,
                                "{lang:?} {mode:?} display {display}: {} > {limit}",
                                bar.width
                            );
                            // Pause or Resume collapses only after the other two.
                            let c = &bar.collapsed;
                            if mode != Mode::Closed {
                                assert!(!c[0] || c[1] && c[2], "{c:?}");
                                assert!(!c[1] || c[2], "{c:?}");
                            }
                            // The second line hides only when every label is an icon.
                            if !bar.second {
                                assert!(c.iter().all(|x| *x) || mode == Mode::Closed, "{c:?}");
                            }
                        }
                        assert!(bar_width(follow, limit) <= limit + 1e-6);
                    }
                });
            }
        }
    }

    /// The 40 % longer pseudo-length: the layout still never passes its limit on a roomy display.
    #[test]
    fn s17_forty_percent_longer_labels_collapse_instead_of_clipping() {
        let slots = |k: f64| {
            vec![
                bar_layout::Slot::button(bar_layout::text_w("Resume", 13.0, false, 400) * k, 2),
                bar_layout::Slot::button(bar_layout::text_w("Note step", 13.0, false, 400) * k, 1),
                bar_layout::Slot::button(bar_layout::text_w("Undo step", 13.0, false, 400) * k, 0),
            ]
        };
        let second = bar_layout::text_w(
            "Paused \u{b7} Click Guide \u{b7} clicks only",
            11.0,
            false,
            400,
        );
        for display in [1440.0, 1024.0, 800.0] {
            let limit = guide_bar_limit(display);
            for k in [1.0, 1.4] {
                let bar = bar_layout::guide_bar(80.0, second * k, &slots(k), limit);
                assert!(bar.width <= limit, "k {k} display {display}: {}", bar.width);
            }
        }
        // English at 1.0 is roomy at 1440; the longer one collapses on a narrow display only.
        let roomy = bar_layout::guide_bar(80.0, second, &slots(1.0), guide_bar_limit(1440.0));
        assert_eq!(roomy.collapsed, [false, false, false]);
        let narrow = bar_layout::guide_bar(80.0, second * 1.4, &slots(1.4), guide_bar_limit(640.0));
        assert!(narrow.collapsed.iter().any(|c| *c));
    }

    /// S17: a long window name is cut at 28 characters (and the line at 40 plus the name).
    #[test]
    fn the_followed_windows_name_is_cut_at_28_characters() {
        with_language(Lang::En, || {
            let name = "A very long window title that keeps going and going";
            let line = second_line(Mode::Recording, Some(name));
            let shown: String = line
                .split('\u{201c}')
                .nth(1)
                .and_then(|s| s.split('\u{201d}').next())
                .unwrap()
                .to_owned();
            assert_eq!(shown.chars().count(), 28);
            assert!(shown.ends_with('\u{2026}'));
            assert!(line.contains("clicks only"));
            let p = second_line(Mode::Paused, Some("Chrome"));
            assert!(p.starts_with("Paused \u{b7} "), "{p}");
        });
    }

    /// S17: the countdown shows "Starting... 3", disabled controls and "Stop or Esc cancels".
    #[test]
    fn s17_the_countdown_words() {
        with_language(Lang::En, || {
            let p = parts(Mode::Countdown, 1, 3, None);
            assert_eq!(p.headline, "Starting\u{2026} 3");
            assert_eq!(p.second, "Stop or Esc cancels");
            assert_eq!(p.slots.len(), 3);
        });
    }
}

#[cfg(test)]
mod ended_tests {
    use super::*;

    #[test]
    fn the_bar_closes_when_the_guide_ended_elsewhere() {
        assert!(!guide_has_ended(false, None), "the countdown is starting");
        assert!(!guide_has_ended(true, Some("recording")));
        assert!(!guide_has_ended(true, Some("paused")));
        assert!(!guide_has_ended(false, Some("countdown")));
        assert!(guide_has_ended(true, Some("stopped")));
        assert!(guide_has_ended(true, Some("discarded")));
        assert!(guide_has_ended(true, None), "the guide is gone");
    }

    /// CG-2: an agent's or the CLI's guide gets the passive bar; the client is named from `by`.
    #[test]
    fn cg_2_a_guide_started_elsewhere_is_classified_and_named() {
        assert_eq!(
            classify_event(
                "guide.started",
                &json!({ "guide_id": "g1", "by": "agent:capturemann-cli" })
            ),
            GuideEvent::Started {
                guide_id: "g1".into(),
                by: "agent:capturemann-cli".into()
            }
        );
        assert_eq!(
            passive_client("agent:capturemann-cli").as_deref(),
            Some("capturemann-cli")
        );
        assert_eq!(passive_client("user"), None);
        assert_eq!(passive_client("agent:  "), None);
        assert!(passive_line(Some("x")).contains('x'));
    }

    /// BUG-1: an agent's or the CLI's guide gets the passive bar with the real client's name
    /// ("Started by agent {client}", never a fixed "MCP"); the app's own guide (its bar is open)
    /// and an event with no guide get none.
    #[test]
    fn bug_1_the_passive_bar_opens_for_agent_and_cli_guides_with_the_real_client() {
        assert_eq!(
            passive_for(false, "g1", "agent:claude-desktop"),
            Some(Some("claude-desktop".to_owned()))
        );
        assert_eq!(
            passive_for(false, "g1", "agent:capturemann-cli"),
            Some(Some("capturemann-cli".to_owned()))
        );
        assert_eq!(passive_for(true, "g1", "agent:claude-desktop"), None);
        assert_eq!(passive_for(false, "", "agent:claude-desktop"), None);
        assert_eq!(
            passive_line(Some("claude-desktop")),
            "Started by agent claude-desktop"
        );
        assert_eq!(
            passive_line(Some("capturemann-cli")),
            "Started by agent capturemann-cli"
        );
    }

    /// The passive bar is never wider than the display's limit or the full bar's maximum.
    #[test]
    fn cg_2_the_passive_bar_fits_every_display() {
        for visible_w in [800.0, 1280.0, 2560.0] {
            let limit = bar_layout::guide_bar_limit(visible_w);
            let w = passive_bar_width(limit);
            assert!(
                w <= limit && w <= bar_layout::GUIDE_BAR_MAX_W,
                "{w} in {limit}"
            );
        }
    }

    #[test]
    fn stopping_an_ended_guide_is_quiet() {
        assert!(stop_means_already_ended(
            "invalid_input: no Click Guide is recording"
        ));
        assert!(!stop_means_already_ended("disk full"));
    }
}

#[cfg(test)]
mod cap_tests {
    use super::*;

    /// CG-9: the daemon's three guide events that the bar's owner must show are told apart, and
    /// the others are not.
    #[test]
    fn cg_9_the_guide_events_are_classified() {
        assert_eq!(
            classify_event("guide.near_limit", &json!({ "remaining": 50 })),
            GuideEvent::NearLimit { remaining: 50 }
        );
        assert_eq!(
            classify_event(
                "guide.limit_reached",
                &json!({ "guide_id": "guide_3", "max": 500 })
            ),
            GuideEvent::LimitReached {
                guide_id: "guide_3".into(),
                max: 500
            }
        );
        assert_eq!(
            classify_event("guide.error", &json!({ "message": "disk full" })),
            GuideEvent::Error {
                reason: "disk full".into()
            }
        );
        assert!(matches!(
            classify_event("guide.error", &json!({ "code": "busy", "message": "x" })),
            GuideEvent::Error { reason } if reason != "x"
        ));
        assert_eq!(
            classify_event("guide.stopped", &json!({ "guide_id": "guide_3" })),
            GuideEvent::Stopped {
                guide_id: "guide_3".into()
            }
        );
        assert_eq!(
            classify_event(
                "guide.window_moved",
                &json!({ "guide_id": "guide_3", "step_count": 4 })
            ),
            GuideEvent::WindowMoved { step_count: 4 }
        );
        assert_eq!(
            classify_event("guide.step_added", &json!({})),
            GuideEvent::Other
        );
    }

    /// CG-9: an error toast comes at most once in 10 s.
    #[test]
    fn cg_9_an_error_toast_is_shown_at_most_once_in_ten_seconds() {
        let t0 = Instant::now();
        let mut last = None;
        assert!(error_toast_due(&mut last, t0));
        assert!(!error_toast_due(&mut last, t0 + Duration::from_secs(3)));
        assert!(!error_toast_due(&mut last, t0 + Duration::from_secs(9)));
        assert!(error_toast_due(&mut last, t0 + Duration::from_secs(11)));
        assert!(!error_toast_due(&mut last, t0 + Duration::from_secs(12)));
    }

    /// CG-9: the bar closes for the end of the guide it follows, and for any guide before it
    /// knows its own id (the countdown), but not for another guide's end.
    #[test]
    fn cg_9_the_bar_closes_when_its_guide_stopped_for_any_reason() {
        assert!(bar_closes_for(Some("guide_1"), "guide_1"));
        assert!(bar_closes_for(None, "guide_1"));
        assert!(!bar_closes_for(Some("guide_2"), "guide_1"));
    }
}
