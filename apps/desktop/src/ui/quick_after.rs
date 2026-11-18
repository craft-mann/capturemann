//! The after-capture bar (QC-3): six actions under the captured area, each one key away. A small
//! floating window like the HUD, excluded from capture, closed by Esc, a click elsewhere or, when
//! the pointer is not over it, a long idle time. The capture is already in the library.

use std::time::{Duration, Instant};

use cm_i18n::{tr, tr_ctx};
use gpui::{
    div, prelude::*, px, App, Context, Entity, FocusHandle, FontWeight, Global, KeyDownEvent,
    Render, Role, SharedString, Window, WindowHandle,
};

use crate::quick::place::bar_rect;
use crate::quick::Area;
use crate::recording::geometry::Rect;
use crate::theme::{scale, type_scale, Theme};
use crate::ui::app::AppModel;
use crate::ui::bar_layout::text_w;
use crate::ui::components::icon;
use crate::ui::paint::{rgba_of, ui_features, TextExt, UI_FONT};
use crate::ui::recording::{panel_options, pointer_display, visible_part};

/// The bar closes by itself after this long without the pointer over it.
pub const IDLE_CLOSE: Duration = Duration::from_secs(20);
const BAR_H: f64 = 52.0;
const PAD: f64 = 6.0;
const BTN_PAD: f64 = 10.0;
const GAP: f64 = 4.0;
const ICON_W: f64 = 16.0;
const KEY_W: f64 = 20.0;

/// One action of the bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Act {
    Edit,
    Copy,
    Save,
    Text,
    LongShot,
    Record,
}

impl Act {
    pub const ALL: [Act; 6] = [
        Act::Edit,
        Act::Copy,
        Act::Save,
        Act::Text,
        Act::LongShot,
        Act::Record,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Act::Edit => "e",
            Act::Copy => "c",
            Act::Save => "s",
            Act::Text => "t",
            Act::LongShot => "l",
            Act::Record => "r",
        }
    }

    pub fn from_key(k: &str) -> Option<Act> {
        Act::ALL.into_iter().find(|a| a.key() == k)
    }

    pub fn label(self) -> String {
        match self {
            Act::Edit => tr_ctx!("after-capture bar", "Edit"),
            Act::Copy => tr_ctx!("after-capture bar", "Copy"),
            Act::Save => tr_ctx!("after-capture bar", "Save"),
            Act::Text => tr_ctx!("after-capture bar", "Text"),
            Act::LongShot => tr_ctx!("after-capture bar", "Long Shot"),
            Act::Record => tr_ctx!("after-capture bar", "Record"),
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Act::Edit => "pencil",
            Act::Copy => "copy",
            Act::Save => "save",
            Act::Text => "scan-text",
            Act::LongShot => "long-shot",
            Act::Record => "cm-record",
        }
    }

    /// The accessible name: "Edit, E".
    pub fn aria(self) -> String {
        tr_ctx!(
            "after-capture bar",
            "{action}, {key}",
            action = self.label(),
            key = self.key().to_uppercase()
        )
    }
}

/// The width of the bar and whether the labels fit: icons and keys always show, the labels give
/// way when the bar would pass `limit`.
pub fn bar_width(labels: &[String], limit: f64) -> (f64, bool) {
    let label = |l: &String| text_w(l, type_scale::UI - 1., false, 500);
    let with_labels: f64 = 2.0 * PAD
        + 2.0
        + labels
            .iter()
            .map(|l| 2.0 * BTN_PAD + ICON_W + 6.0 + label(l) + 6.0 + KEY_W + GAP)
            .sum::<f64>();
    if with_labels <= limit {
        return (with_labels, true);
    }
    let compact =
        2.0 * PAD + 2.0 + labels.len() as f64 * (2.0 * BTN_PAD + ICON_W + 6.0 + KEY_W + GAP);
    (compact.min(limit), false)
}

struct Slot(Option<WindowHandle<AfterBar>>);
impl Global for Slot {}

/// Opens the bar for `capture_id`, placed from `area` (the display, then the pointer's display, when
/// the area is not known).
pub fn open(app: &Entity<AppModel>, area: Option<Area>, capture_id: String, cx: &mut App) {
    close(cx);
    let backend = app.read(cx).backend.clone();
    let Some(daemon) = backend.daemon.clone() else {
        return;
    };
    let app2 = app.clone();
    cx.spawn(async move |cx| {
        let listed = backend
            .rt
            .spawn(async move {
                crate::backend::Backend::call(
                    &daemon,
                    "list_targets",
                    serde_json::json!({ "windows": true }),
                )
                .await
            })
            .await;
        cx.update(|cx| {
            let placed = listed
                .ok()
                .and_then(|r| r.ok())
                .and_then(|v| crate::quick::area::parse_targets(&v))
                .and_then(|t| {
                    crate::quick::area::place(
                        &t,
                        &area.unwrap_or(Area::Display { display_id: None }),
                    )
                });
            let points = placed.map(|p| {
                let g = [
                    p.display_px[0] + p.rect[0],
                    p.display_px[1] + p.rect[1],
                    p.rect[2],
                    p.rect[3],
                ];
                crate::ui::long_shot::geometry::area_in_points(g, p.scale)
            });
            open_at(&app2, points, area, capture_id, cx);
        });
    })
    .detach();
}

fn open_at(
    app: &Entity<AppModel>,
    points: Option<Rect>,
    area: Option<Area>,
    capture_id: String,
    cx: &mut App,
) {
    let display = match points.and_then(|p| crate::ui::recording::display_for(cx, &p)) {
        Some(d) => d,
        None => match pointer_display(cx) {
            Some(d) => d,
            None => return,
        },
    };
    let visible = visible_part(&display.1);
    let labels: Vec<String> = Act::ALL.iter().map(|a| a.label()).collect();
    let (w, show_labels) = bar_width(
        &labels,
        (visible.w - 2.0 * crate::quick::place::EDGE).max(120.0),
    );
    // No known area: the bar sits low on the display the pointer is on.
    let area_rect = points.unwrap_or(visible);
    let global = bar_rect(&area_rect, w, BAR_H, &visible);
    let (app2, id) = (app.clone(), capture_id);
    let opened = cx.open_window(
        panel_options(display, global, true, false),
        move |window, cx| {
            crate::platform_window::exclude_from_capture(window);
            cx.new(|cx| AfterBar::new(app2, id, area, show_labels, window, cx))
        },
    );
    if let Ok(h) = opened {
        crate::surfaces::opened(crate::surfaces::Surface::QuickAfter, h);
        cx.set_global(Slot(Some(h)));
        crate::surfaces::activate_app(cx, "after-capture bar opened");
    }
}

/// Closes the bar if one is open.
pub fn close(cx: &mut App) {
    if let Some(h) = cx.try_global::<Slot>().and_then(|s| s.0) {
        crate::surfaces::close(cx, h);
        cx.set_global(Slot(None));
    }
}

pub struct AfterBar {
    app: Entity<AppModel>,
    capture_id: String,
    area: Option<Area>,
    labels: bool,
    focus: FocusHandle,
    hovered: bool,
    touched: Instant,
    was_active: bool,
    can_text: bool,
}

impl AfterBar {
    fn new(
        app: Entity<AppModel>,
        capture_id: String,
        area: Option<Area>,
        labels: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        // A click elsewhere closes the bar (QC-3 "Dismiss"); the capture stays in the library.
        cx.observe_window_activation(window, |this, window, cx| {
            if window.is_window_active() {
                this.was_active = true;
            } else if this.was_active {
                this.dismiss(window, cx);
            }
        })
        .detach();
        // The idle close: never while the pointer is over the bar (QC-3.AC6).
        cx.spawn_in(window, async move |this, cx| loop {
            cx.background_executor().timer(Duration::from_secs(1)).await;
            let gone = this
                .update_in(cx, |this, window, cx| {
                    if this.hovered {
                        this.touched = Instant::now();
                    } else if this.touched.elapsed() >= IDLE_CLOSE {
                        this.dismiss(window, cx);
                        return true;
                    }
                    false
                })
                .unwrap_or(true);
            if gone {
                break;
            }
        })
        .detach();
        AfterBar {
            app,
            capture_id,
            area,
            labels,
            focus,
            hovered: false,
            touched: Instant::now(),
            was_active: false,
            can_text: crate::editor::copytext::available(),
        }
    }

    fn dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        crate::surfaces::close_self(window);
        cx.set_global(Slot(None));
    }

    fn enabled(&self, a: Act) -> bool {
        match a {
            Act::Text => self.can_text,
            Act::Record => crate::backend::recording_supported(),
            _ => true,
        }
    }

    fn run(&mut self, a: Act, window: &mut Window, cx: &mut Context<Self>) {
        if !self.enabled(a) {
            return;
        }
        self.dismiss(window, cx);
        let (app, id, area) = (self.app.clone(), self.capture_id.clone(), self.area);
        // The bar needs a moment to leave the screen before a capture or a recording starts.
        cx.defer(move |cx| match a {
            Act::Edit => crate::ui::quick_flow::bar_edit(&app, &id, cx),
            Act::Copy => crate::ui::quick_flow::bar_copy(&app, &id, cx),
            Act::Save => crate::ui::quick_flow::bar_save(&app, &id, cx),
            Act::Text => crate::ui::quick_flow::bar_text(&app, &id, cx),
            Act::LongShot => crate::ui::quick_flow::bar_long_shot(&app, area, cx),
            Act::Record => crate::ui::quick_flow::bar_record(&app, area, cx),
        });
    }

    fn on_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.touched = Instant::now();
        let k = &ev.keystroke;
        if k.modifiers.platform || k.modifiers.control || k.modifiers.alt {
            return;
        }
        if k.key == "escape" {
            self.dismiss(window, cx);
            cx.stop_propagation();
            return;
        }
        if let Some(a) = Act::from_key(&k.key) {
            self.run(a, window, cx);
            cx.stop_propagation();
        }
    }
}

impl Render for AfterBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = Theme::overlay();
        let labels = self.labels;
        let buttons = Act::ALL.into_iter().map(|a| {
            let enabled = self.enabled(a);
            let record = a == Act::Record;
            let (fg, hover) = (if enabled { t.text } else { t.mist }, rgba_of(t.raised));
            let name = if enabled {
                a.aria()
            } else {
                tr_ctx!(
                    "after-capture bar",
                    "{action}, unavailable",
                    action = a.label()
                )
            };
            div()
                .id(SharedString::from(format!("qc-after-{}", a.key())))
                .role(Role::Button)
                .aria_label(SharedString::from(name))
                .focusable()
                .tab_stop(true)
                .h(px(44.))
                .px(px(BTN_PAD as f32))
                .flex()
                .flex_none()
                .items_center()
                .gap(px(6.))
                .rounded(px(scale::RADIUS_MD))
                .border_1()
                .border_color(gpui::transparent_black())
                .text_size(px(type_scale::UI - 1.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgba_of(fg))
                .when(enabled, |d| d.cursor_pointer().hover(move |s| s.bg(hover)))
                .when(!enabled, |d| d.opacity(0.5))
                .focus_visible(|s| s.border_color(rgba_of(Theme::overlay().text)))
                .on_click(cx.listener(move |this, _, window, cx| this.run(a, window, cx)))
                .child(icon(
                    a.icon(),
                    16.,
                    if record && enabled { t.rec } else { fg },
                ))
                .when(labels, |d| d.child(SharedString::from(a.label())))
                .child(
                    div()
                        .min_w(px(KEY_W as f32))
                        .px(px(5.))
                        .py(px(1.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(scale::RADIUS_XS))
                        .bg(rgba_of(t.raised))
                        .text_size(px(11.))
                        .text_color(rgba_of(t.text_3))
                        .mono()
                        .child(SharedString::from(a.key().to_uppercase())),
                )
        });
        div()
            .id("quick-after-bar")
            .role(Role::Toolbar)
            .aria_label(tr!("After capture"))
            .size_full()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key))
            .on_hover(cx.listener(|this, over: &bool, _, _| {
                this.hovered = *over;
                this.touched = Instant::now();
            }))
            .font_family(UI_FONT)
            .font_features(ui_features())
            .text_color(rgba_of(t.text))
            .child(
                div()
                    .size_full()
                    .px(px(PAD as f32))
                    .flex()
                    .items_center()
                    .gap(px(GAP as f32))
                    .rounded(px(scale::RADIUS_LG))
                    .border_1()
                    .border_color(rgba_of(t.line))
                    .bg(rgba_of(t.surface))
                    .children(buttons),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// QC-3.AC1: six actions, each with its key.
    #[test]
    fn qc3_ac1_the_bar_has_six_actions_with_their_keys() {
        let keys: Vec<_> = Act::ALL.iter().map(|a| a.key()).collect();
        assert_eq!(keys, ["e", "c", "s", "t", "l", "r"]);
        for a in Act::ALL {
            assert_eq!(Act::from_key(a.key()), Some(a));
            assert!(a.aria().ends_with(&a.key().to_uppercase()));
        }
        assert_eq!(Act::from_key("x"), None);
    }

    /// QC-3: the labels fit at a normal display width; on a narrow one they give way to icons and
    /// keys and the bar never passes the limit.
    #[test]
    fn the_bar_fits_or_drops_its_labels() {
        let labels: Vec<String> = Act::ALL.iter().map(|a| a.label()).collect();
        let (w, shown) = bar_width(&labels, 1400.0);
        assert!(shown && w < 760.0, "{w}");
        let (w, shown) = bar_width(&labels, 420.0);
        assert!(!shown && w <= 420.0);
    }
}
