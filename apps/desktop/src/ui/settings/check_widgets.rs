//! Pieces shared by the Camera check and the Audio check cards (board R13b): the card shell, the
//! status chip, the state block (tinted icon tile, title, one sentence, ONE button), the action
//! button, the lock footnote and the segment meter. Sizes and colours are the mockup's; the
//! colours come from the theme tokens (`success`, `warning`, `rec_soft`, `agent_text`).

use gpui::{
    div, prelude::*, px, AnyElement, App, ClickEvent, Div, ElementId, FontWeight, Role,
    SharedString, Stateful, Window,
};

use super::audio_check_model::Seg;
use crate::theme::{scale, type_scale, Color, Theme};
use crate::ui::components::{icon, icon_size, label_fill};
use crate::ui::paint::rgba_of;

/// The tone of a chip or a state block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// Nothing is wrong: the check is simply not running.
    Info,
    Warning,
    Error,
    Success,
}

pub fn tone_color(t: &Theme, tone: Tone) -> Color {
    match tone {
        Tone::Info => t.agent_text,
        Tone::Warning => t.warning,
        Tone::Error => t.rec_soft,
        Tone::Success => t.success,
    }
}

/// The check cards: `surface`, 1 px `border`, radius 18.
pub fn check_card(t: &Theme, id: &'static str, aria: &str) -> Stateful<Div> {
    div()
        .id(id)
        .role(Role::Group)
        .aria_label(SharedString::from(aria.to_owned()))
        .relative()
        .flex()
        .flex_col()
        .rounded(px(scale::RADIUS_DIALOG))
        .bg(rgba_of(t.surface))
        .border_1()
        .border_color(rgba_of(t.border))
}

/// The inner panel (the meter, the camera feed): `field`, 1 px `raised`, radius 12.
pub fn inner_panel(t: &Theme) -> Div {
    div()
        .rounded(px(12.))
        .bg(rgba_of(t.field))
        .border_1()
        .border_color(rgba_of(t.raised))
}

/// The status chip: 7 px dot or a 12 px icon, then the label; the tone at 10 % with a 40 % border.
pub fn chip(
    t: &Theme,
    tone: Tone,
    height: f32,
    label: impl Into<SharedString>,
    glyph: Option<&'static str>,
    dot: bool,
) -> Div {
    let c = tone_color(t, tone);
    div()
        .flex_none()
        .h(px(height))
        .px(px(if height > 22. { 9. } else { 8. }))
        .flex()
        .items_center()
        .gap(px(6.))
        .rounded(px(scale::RADIUS_PILL))
        .bg(rgba_of(c.with_alpha(10)))
        .border_1()
        .border_color(rgba_of(c.with_alpha(40)))
        .text_size(px(if height > 22. { 12. } else { 11.5 }))
        .text_color(rgba_of(c))
        .when(dot, |d| {
            d.child(div().size(px(7.)).rounded_full().bg(rgba_of(c)))
        })
        .children(glyph.map(|g| icon(g, icon_size::PILL, c)))
        .child(label.into())
}

/// A secondary action button, 30 px: the state block's one button.
pub fn action_button(
    t: &Theme,
    id: impl Into<ElementId>,
    glyph: &'static str,
    label: &str,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let (hover, focus) = (rgba_of(t.raised), rgba_of(t.text));
    div()
        .id(id)
        .role(Role::Button)
        .aria_label(SharedString::from(label.to_owned()))
        .focusable()
        .tab_stop(true)
        .h(px(30.))
        .px(px(10.))
        .flex()
        .flex_none()
        .items_center()
        .gap(px(6.))
        .rounded(px(scale::RADIUS_SM))
        .border_1()
        .border_color(rgba_of(t.line))
        .bg(rgba_of(t.surface_2))
        .text_size(px(12.5))
        .text_color(rgba_of(t.text))
        .whitespace_nowrap()
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .focus_visible(move |s| s.border_color(focus))
        .child(icon(glyph, 13., t.text))
        .child(SharedString::from(label.to_owned()))
        .on_click(on_click)
}

/// A 34 px button next to a dropdown ("Turn off camera", "Test microphone").
pub fn control_button(
    t: &Theme,
    id: impl Into<ElementId>,
    glyph: &'static str,
    label: &str,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let (hover, focus) = (rgba_of(t.raised), rgba_of(t.text));
    div()
        .id(id)
        .role(Role::Button)
        .aria_label(SharedString::from(label.to_owned()))
        .focusable()
        .tab_stop(true)
        .h(px(34.))
        .px(px(12.))
        .flex()
        .flex_none()
        .items_center()
        .gap(px(7.))
        .rounded(px(9.))
        .border_1()
        .border_color(rgba_of(t.line))
        .bg(rgba_of(t.surface_2))
        .text_size(px(13.))
        .text_color(rgba_of(t.text))
        .whitespace_nowrap()
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .focus_visible(move |s| s.border_color(focus))
        .child(icon(glyph, 14., t.text))
        .child(SharedString::from(label.to_owned()))
        .on_click(on_click)
}

/// What a state block says.
pub struct Block {
    pub tone: Tone,
    pub glyph: &'static str,
    pub title: String,
    pub text: String,
}

/// A failure or "off" block: a 30 px tile in the tone at 12 %, the title, one plain sentence and
/// the one `action`. Warning and error blocks have the tone at 30 % as their border.
pub fn state_block(
    t: &Theme,
    id: &'static str,
    b: Block,
    action: Option<AnyElement>,
) -> Stateful<Div> {
    let c = tone_color(t, b.tone);
    let border = match b.tone {
        Tone::Warning | Tone::Error => c.with_alpha(30),
        _ => t.raised,
    };
    div()
        .id(id)
        .role(Role::Group)
        .aria_label(SharedString::from(b.title.clone()))
        .flex()
        .flex_col()
        .items_start()
        .gap(px(10.))
        .p(px(14.))
        .rounded(px(12.))
        .bg(rgba_of(t.field))
        .border_1()
        .border_color(rgba_of(border))
        .child(
            div()
                .w_full()
                .flex()
                .items_center()
                .gap(px(10.))
                .child(
                    div()
                        .size(px(30.))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(9.))
                        .bg(rgba_of(c.with_alpha(12)))
                        .child(icon(b.glyph, 15., c)),
                )
                .child(
                    label_fill(SharedString::from(b.title))
                        .text_size(px(14.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgba_of(t.text)),
                ),
        )
        .child(
            div()
                .w_full()
                .text_size(px(12.5))
                .line_height(px(18.))
                .text_color(rgba_of(t.text_3))
                .child(SharedString::from(b.text)),
        )
        .children(action)
}

/// The lock line under a card: "Preview only. Nothing is recorded, ...".
pub fn footnote(t: &Theme, text: impl Into<SharedString>) -> Div {
    div()
        .w_full()
        .flex()
        .items_start()
        .gap(px(7.))
        .text_size(px(type_scale::CAPTION))
        .text_color(rgba_of(t.mist))
        .child(div().pt(px(2.)).child(icon("lock", 12., t.mist)))
        .child(label_fill(text.into()))
}

pub fn seg_color(t: &Theme, s: Seg) -> Color {
    match s {
        Seg::Unlit => t.raised,
        Seg::Quiet => t.mist,
        Seg::Good => t.success,
        Seg::Loud => t.rec_soft,
    }
}

/// A row of meter segments, `height` px tall, `gap` px apart.
pub fn segment_row(t: &Theme, segs: &[Seg], height: f32, gap: f32, radius: f32) -> Div {
    div()
        .w_full()
        .flex()
        .gap(px(gap))
        .children(segs.iter().map(|s| {
            div()
                .flex_1()
                .min_w(px(0.))
                .h(px(height))
                .rounded(px(radius))
                .bg(rgba_of(seg_color(t, *s)))
        }))
}
