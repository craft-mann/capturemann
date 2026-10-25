//! The recorded-area indicator (DESIGN 5.2, REC-5.AC9, AC10): the kit's four white crop brackets
//! (4 px, radius 6, 28 px arms), a 2 px `rec` outline at 70% opacity that turns dashed while
//! paused, the size chips above the top-left corner and, for a region, the 55% scrim outside it.
//!
//! One transparent window covers the recorded display, ignores the mouse and is excluded from
//! capture, so none of it ever appears in `screen.mp4`.

use cm_i18n::tr;
use gpui::{
    div, prelude::*, px, Context, Entity, Render, Role, SharedString, Subscription, Window,
};

use crate::recording::geometry::{
    self, AreaKind, Rect, BRACKET_RADIUS, BRACKET_STROKE, OUTLINE_OPACITY_PERCENT, OUTLINE_STROKE,
    SCRIM_OPACITY_PERCENT,
};
use crate::theme::{type_scale, Color, Theme};
use crate::ui::app::AppModel;
use crate::ui::paint::{rgba_of, TextExt, UI_FONT};
use crate::ui::recording::RecState;

pub struct IndicatorView {
    rec: Entity<RecState>,
    /// The covered display, in global points.
    display: Rect,
    _sub: Subscription,
}

/// Everything the indicator draws depends on this: the 10 Hz timer and the mic meter touch the
/// shared state far more often, and repainting a display-sized window for them is waste (R4-17).
type DrawKey = (bool, Option<Rect>, Option<String>);

fn draw_key(r: &RecState) -> DrawKey {
    (r.hud.is_paused(), r.area, r.hud.recording_id.clone())
}

impl IndicatorView {
    pub fn new(
        _app: Entity<AppModel>,
        rec: Entity<RecState>,
        display: Rect,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut last = draw_key(rec.read(cx));
        let sub = cx.observe(&rec, move |_, rec, cx| {
            let key = draw_key(rec.read(cx));
            if key != last {
                last = key;
                cx.notify();
            }
        });
        IndicatorView {
            rec,
            display,
            _sub: sub,
        }
    }
}

/// One bracket: an L-shaped corner. `corner` is 0 top-left, 1 top-right, 2 bottom-left,
/// 3 bottom-right.
fn bracket(r: &Rect, corner: usize) -> gpui::Div {
    let w = px(BRACKET_STROKE as f32);
    let radius = px(BRACKET_RADIUS as f32);
    let d = div()
        .absolute()
        .left(px(r.x as f32))
        .top(px(r.y as f32))
        .w(px(r.w as f32))
        .h(px(r.h as f32))
        .border_color(rgba_of(crate::theme::derived::WHITE));
    match corner {
        0 => d.border_t(w).border_l(w).rounded_tl(radius),
        1 => d.border_t(w).border_r(w).rounded_tr(radius),
        2 => d.border_b(w).border_l(w).rounded_bl(radius),
        _ => d.border_b(w).border_r(w).rounded_br(radius),
    }
}

fn chip(t: &Theme, text: SharedString, color: Color) -> gpui::Div {
    div()
        .h(px(geometry::CHIP_HEIGHT as f32))
        .px(px(8.))
        .flex()
        .items_center()
        .rounded(px(6.))
        .border_1()
        .border_color(rgba_of(t.line))
        .bg(rgba_of(t.ink))
        .text_size(px(type_scale::CAPTION))
        .text_color(rgba_of(color))
        .mono()
        .child(text)
}

impl Render for IndicatorView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = Theme::overlay();
        let r = self.rec.read(cx);
        let (Some(ind), Some(area)) = (r.hud.indicator.as_ref(), r.area) else {
            return div().size_full().into_any_element();
        };
        let paused = r.hud.is_paused();
        let local_display = Rect::new(0.0, 0.0, self.display.w, self.display.h);
        let rel = area.relative_to(&self.display);
        let kind = ind.area.kind;
        let (first, second) =
            geometry::size_chips(ind.area.source_size, ind.area.scale, ind.output);
        let (cx0, cy0) = geometry::chips_origin(&rel, &local_display);
        let outline = t.rec.with_alpha(OUTLINE_OPACITY_PERCENT);
        let scrim = crate::theme::derived::BLACK.with_alpha(SCRIM_OPACITY_PERCENT);
        let strips = if kind.dims_outside() {
            geometry::scrim_strips(&rel, &local_display)
        } else {
            Vec::new()
        };
        let brackets = geometry::brackets(&rel, &local_display);
        let label = match kind {
            AreaKind::Display => tr!("Recording display, {size}", size = first),
            AreaKind::Window => tr!("Recording window, {size}", size = first),
            AreaKind::Region => tr!("Recording region, {size}", size = first),
        };
        div()
            .id("rec-indicator")
            .role(Role::Group)
            .aria_label(SharedString::from(label))
            .size_full()
            .relative()
            .font_family(UI_FONT)
            .children(strips.into_iter().map(|s| {
                div()
                    .absolute()
                    .left(px(s.x as f32))
                    .top(px(s.y as f32))
                    .w(px(s.w as f32))
                    .h(px(s.h as f32))
                    .bg(rgba_of(scrim))
            }))
            // The outline runs along the selection edge, inside it, so it never hides content
            // the recording would miss.
            .child(
                div()
                    .absolute()
                    .left(px(rel.x as f32))
                    .top(px(rel.y as f32))
                    .w(px(rel.w as f32))
                    .h(px(rel.h as f32))
                    .border(px(OUTLINE_STROKE as f32))
                    .border_color(rgba_of(outline))
                    .when(paused, |d| d.border_dashed()),
            )
            .children(brackets.iter().enumerate().map(|(i, b)| bracket(b, i)))
            .child(
                div()
                    .absolute()
                    .left(px(cx0 as f32))
                    .top(px(cy0 as f32))
                    .flex()
                    .gap(px(8.))
                    .child(chip(&t, SharedString::from(first), t.text))
                    .child(chip(&t, SharedString::from(second), t.text_3)),
            )
            .into_any_element()
    }
}
