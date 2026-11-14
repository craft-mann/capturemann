//! The Click Guide bar (GDE-1, design board S3): a small always-on-top panel with the step count,
//! Pause, Note step, Undo step and Stop. It is excluded from screen capture, and its rectangle is
//! given to the daemon so a click on it is never a step (GDE-1.AC2). It only calls the daemon's
//! actions (`start_guide`, `set_guide`, `stop_guide`) and polls `get_guide` for its numbers.

use std::time::Duration;

use cm_i18n::tr;
use gpui::{
    div, prelude::*, px, App, Context, Entity, FontWeight, Global, Render, Role, SharedString,
    Window, WindowHandle,
};
use serde_json::{json, Value};

use crate::backend::Backend;
use crate::recording::geometry::Rect;
use crate::theme::{type_scale, Theme};
use crate::toast::{ToastKind, ToastSpec};
use crate::ui::app::{self, AppModel, Route};
use crate::ui::components::{icon, rec_dot};
use crate::ui::paint::{rgba_of, ui_features, TextExt, UI_FONT};
use crate::ui::recording::{harden, panel_options, pointer_display, visible_part};

/// The bar's size, points.
pub const BAR_W: f64 = 470.0;
pub const BAR_H: f64 = 64.0;
const MARGIN: f64 = 12.0;

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
}

pub struct GuideHud {
    app: Entity<AppModel>,
    shown: Shown,
}

/// The bar's rectangle: centred above the followed display's visible part, at the top.
pub fn bar_rect(visible: &Rect) -> Rect {
    Rect::new(
        visible.x + (visible.w - BAR_W) / 2.0,
        visible.y + MARGIN,
        BAR_W,
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

/// "New Click Guide": follows the display under the pointer, shows the bar and starts the guide
/// (a 3 s countdown first). Nothing is recorded until the countdown ends; Stop during it cancels.
pub fn start(app: &Entity<AppModel>, cx: &mut App) {
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
            super::card::open(app, start, cx);
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
    let rect = bar_rect(&visible_part(&display.1));
    let gpui_id = u32::try_from(u64::from(display.0)).unwrap_or(1);
    let app2 = app.clone();
    let h = cx
        .open_window(
            panel_options(display, rect, false, false),
            move |window, cx| {
                harden(window, false);
                cx.new(|cx| GuideHud::new(app2, cx))
            },
        )
        .ok();
    let Some(h) = h else { return };
    crate::surfaces::opened(crate::surfaces::Surface::GuideHud, h);
    cx.set_global(Holder(Some(h)));
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
                    json!({ "target": { "display_id": display_id }, "countdown_s": 3 }),
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

/// Closes the bar.
pub fn close(cx: &mut App) {
    if let Some(h) = cx.try_global::<Holder>().and_then(|h| h.0) {
        crate::surfaces::close(cx, h);
    }
    cx.set_global(Holder(None));
}

impl GuideHud {
    fn new(app: Entity<AppModel>, cx: &mut Context<Self>) -> Self {
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
            let v = backend
                .rt
                .spawn(async move {
                    let p = id.map_or_else(|| json!({}), |i| json!({ "guide_id": i }));
                    Backend::call(&daemon, "get_guide", p).await.ok()
                })
                .await
                .ok()
                .flatten();
            if let Some(v) = v {
                let _ = this.update(cx, |this, cx| {
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
                    };
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
        }
    }

    fn act(&mut self, method: &'static str, params: Value, cx: &mut Context<Self>) {
        let backend = self.app.read(cx).backend.clone();
        let Some(daemon) = backend.daemon.clone() else {
            return;
        };
        let app = self.app.clone();
        let stopping = method == "stop_guide";
        cx.spawn(async move |_, cx| {
            let r = backend
                .rt
                .spawn(async move { Backend::call(&daemon, method, params).await })
                .await;
            if !stopping {
                return;
            }
            cx.update(|cx| {
                close(cx);
                match r {
                    Ok(Ok(v)) if v["discarded"] == true => toast(
                        &app,
                        ToastSpec::new(ToastKind::Info, tr!("No steps were taken"))
                            .body(tr!("The empty guide was not kept.")),
                        cx,
                    ),
                    Ok(Ok(v)) => {
                        if let Some(id) = v["guide_id"].as_str() {
                            app::navigate(&app, Route::Guide(id.to_owned()), cx);
                        }
                    }
                    Ok(Err(e)) => toast(
                        &app,
                        crate::flows::error_toast(&tr!("The guide did not stop cleanly"), e),
                        cx,
                    ),
                    Err(_) => {}
                }
            });
        })
        .detach();
    }
}

fn hud_button(
    t: &Theme,
    id: &'static str,
    glyph: &'static str,
    label: String,
) -> gpui::Stateful<gpui::Div> {
    let hover = rgba_of(t.raised);
    div()
        .id(id)
        .role(Role::Button)
        .aria_label(SharedString::from(label.clone()))
        .h(px(40.))
        .px(px(10.))
        .flex()
        .items_center()
        .gap(px(6.))
        .rounded(px(10.))
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .text_size(px(type_scale::CAPTION))
        .text_color(rgba_of(t.text_2))
        .child(icon(glyph, 16., t.text_2))
        .child(SharedString::from(label))
}

impl Render for GuideHud {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = Theme::overlay();
        let paused = self.shown.state == "paused";
        let counting = self.shown.state == "countdown";
        let headline = if counting {
            tr!("Starting\u{2026}")
        } else {
            tr!("Step {n}", n = self.shown.steps + 1)
        };
        let sub = if paused {
            tr!("Paused \u{b7} clicks are not recorded")
        } else {
            tr!("Click Guide \u{b7} clicks only")
        };
        div()
            .id("guide-hud")
            .role(Role::Toolbar)
            .aria_label(tr!("Click Guide"))
            .size_full()
            .flex()
            .items_center()
            .gap(px(6.))
            .px(px(8.))
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
                    .h(px(40.))
                    .px(px(14.))
                    .flex()
                    .items_center()
                    .rounded(px(10.))
                    .bg(rgba_of(t.rec))
                    .text_color(rgba_of(t.on_rec))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(px(type_scale::UI))
                    .cursor_pointer()
                    .child(tr!("Stop"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.act("stop_guide", json!({}), cx);
                    })),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .px(px(6.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(rec_dot(&t, 8.))
                            .child(
                                div()
                                    .mono()
                                    .text_size(px(15.))
                                    .text_color(rgba_of(t.text))
                                    .child(SharedString::from(headline)),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgba_of(t.text_3))
                            .child(SharedString::from(sub)),
                    ),
            )
            .child(div().w(px(1.)).h(px(28.)).bg(rgba_of(t.line)))
            .child(
                hud_button(
                    &t,
                    "guide-hud-pause",
                    if paused { "rec-play" } else { "rec-pause" },
                    if paused { tr!("Resume") } else { tr!("Pause") },
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.act("set_guide", json!({ "paused": !paused }), cx);
                })),
            )
            .child(
                hud_button(&t, "guide-hud-note", "file-text", tr!("Note step")).on_click(
                    cx.listener(|this, _, _, cx| {
                        this.act(
                            "set_guide",
                            json!({ "note_step": { "title": tr!("New note step") } }),
                            cx,
                        );
                    }),
                ),
            )
            .child(
                hud_button(&t, "guide-hud-undo", "undo-2", tr!("Undo step")).on_click(cx.listener(
                    |this, _, _, cx| {
                        this.act("set_guide", json!({ "remove_last": true }), cx);
                    },
                )),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
