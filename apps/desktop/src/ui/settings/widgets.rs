//! Small kit components shared by the Settings pages.

use gpui::{
    div, prelude::*, px, AnyElement, Div, ElementId, FontWeight, Role, SharedString, Stateful,
    Toggled,
};

use cm_i18n::tr;

use crate::theme::{scale, type_scale, Theme};
use crate::ui::components::{card, icon, icon_size};
use crate::ui::paint::rgba_of;

/// A card whose rows run edge to edge (dividers instead of padding).
pub fn list_card(t: &Theme) -> Div {
    card(t).p(px(0.)).flex().flex_col().overflow_hidden()
}

/// One row of a list card: min 56 px tall, divider below unless `last`.
pub fn card_row(t: &Theme, last: bool) -> Div {
    div()
        .min_h(px(56.))
        .px(px(18.))
        .py(px(12.))
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(14.))
        .when(!last, |d| d.border_b_1().border_color(rgba_of(t.raised)))
}

/// Caption text (12 px, `mist`).
pub fn caption(t: &Theme, text: impl Into<SharedString>) -> Div {
    div()
        .text_size(px(type_scale::CAPTION))
        .text_color(rgba_of(t.mist))
        .child(text.into())
}

/// A button that cannot be used yet: muted, no pointer, no click handler. It stays in the
/// tab order and carries its reason for screen readers (NFR-13).
pub fn disabled_button(
    t: &Theme,
    id: impl Into<ElementId>,
    label: &str,
    reason: &str,
    full_width: bool,
) -> Stateful<Div> {
    let focus = t.text;
    div()
        .id(id)
        .role(Role::Button)
        .aria_label(SharedString::from(tr!(
            "{label}, unavailable",
            label = label
        )))
        .aria_description(SharedString::from(reason.to_owned()))
        .focusable()
        .tab_stop(true)
        .h(px(scale::CONTROL_H))
        .px(px(16.))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .rounded(px(scale::RADIUS_MD))
        .border_1()
        .border_color(rgba_of(t.border))
        .bg(rgba_of(t.raised))
        .text_color(rgba_of(t.mist))
        .text_size(px(type_scale::UI))
        .cursor_default()
        .when(full_width, |d| d.w_full())
        .focus_visible(move |s| s.border_color(rgba_of(focus)))
        .child(SharedString::from(label.to_owned()))
}

/// A compact secondary button (32 px, radius 8) for card headers such as Copy.
pub fn small_button(t: &Theme, id: impl Into<ElementId>, label: &str) -> Stateful<Div> {
    small_button_with_icon(t, id, label, None)
}

/// [`small_button`] with an optional leading icon: 14 px, 6 px gap, the label's colour,
/// decorative (the aria label stays the visible label).
pub fn small_button_with_icon(
    t: &Theme,
    id: impl Into<ElementId>,
    label: &str,
    leading: Option<&'static str>,
) -> Stateful<Div> {
    let (hover, focus) = (rgba_of(t.raised), rgba_of(t.text));
    div()
        .id(id)
        .role(Role::Button)
        .aria_label(SharedString::from(label.to_owned()))
        .focusable()
        .tab_stop(true)
        .h(px(32.))
        .px(px(12.))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .rounded(px(scale::RADIUS_SM))
        .border_1()
        .border_color(rgba_of(t.line))
        .text_color(rgba_of(t.text_2))
        .gap(px(icon_size::SMALL_GAP))
        .text_size(px(type_scale::CAPTION))
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .focus_visible(move |s| s.border_color(focus))
        .children(leading.map(|n| icon(n, icon_size::SMALL, t.text_2)))
        .child(SharedString::from(label.to_owned()))
}

/// One option of a segmented control; the caller adds `.on_click`.
pub fn segment(t: &Theme, id: impl Into<ElementId>, label: &str, active: bool) -> Stateful<Div> {
    segment_with_icon(t, id, label, active, None)
}

/// [`segment`] with an optional leading icon (Theme, Shape): 14 px, 6 px gap, the label's
/// colour, decorative.
pub fn segment_with_icon(
    t: &Theme,
    id: impl Into<ElementId>,
    label: &str,
    active: bool,
    leading: Option<&'static str>,
) -> Stateful<Div> {
    let (hover, fg, focus) = (rgba_of(t.raised), rgba_of(t.text_2), rgba_of(t.text));
    let icon_color = if active { t.text } else { t.text_2 };
    div()
        .id(id)
        .role(Role::RadioButton)
        .aria_label(SharedString::from(label.to_owned()))
        .aria_toggled(if active {
            Toggled::True
        } else {
            Toggled::False
        })
        .focusable()
        .tab_stop(true)
        .h(px(SEGMENT_H))
        .min_w(px(44.))
        .px(px(14.))
        .flex()
        .flex_none()
        .whitespace_nowrap()
        .items_center()
        .justify_center()
        .gap(px(icon_size::SMALL_GAP))
        .rounded(px(scale::RADIUS_SM))
        .border_1()
        .border_color(gpui::transparent_black())
        .text_size(px(type_scale::UI))
        .cursor_pointer()
        .when(active, |d| {
            d.bg(rgba_of(t.segment_selected()))
                .text_color(rgba_of(t.text))
                .font_weight(FontWeight::MEDIUM)
        })
        .when(!active, |d| d.text_color(fg).hover(move |s| s.bg(hover)))
        .focus_visible(move |s| s.border_color(focus))
        .children(leading.map(|n| icon(n, icon_size::SMALL, icon_color)))
        .child(SharedString::from(label.to_owned()))
}

/// Segment height inside the track: the 40 px control minus the 3 px padding on each side and the
/// 1 px border.
pub const SEGMENT_H: f32 = scale::CONTROL_H - 2. * SEGMENT_PAD - 2.;
/// Padding between the track and its segments (kit: 3 px).
pub const SEGMENT_PAD: f32 = 3.;

/// The segmented control container (40 px tall per row, radius 10). In a narrow column the
/// options wrap onto a second row instead of shrinking until their labels overlap.
pub fn segmented(t: &Theme, id: impl Into<ElementId>, aria: &str) -> Stateful<Div> {
    div()
        .id(id)
        .role(Role::RadioGroup)
        .aria_label(SharedString::from(aria.to_owned()))
        .min_h(px(scale::CONTROL_H))
        .min_w_0()
        .p(px(SEGMENT_PAD))
        .flex()
        .flex_wrap()
        .items_center()
        .content_center()
        .gap(px(2.))
        .rounded(px(scale::RADIUS_MD))
        .border_1()
        .border_color(rgba_of(t.line))
        .bg(rgba_of(t.field))
}

/// Settings row (kit): a 34 px icon tile, the label 14/500 over a 12.5 px `mist` hint, and the
/// controls right-aligned. Put rows in a [`settings_group`]; pass `last` for the final row so no
/// divider is drawn under it. `tile` is an icon name from the registry.
pub fn settings_row(
    t: &Theme,
    tile: Option<&'static str>,
    label: impl Into<SharedString>,
    hint: Option<SharedString>,
    control: Option<AnyElement>,
    last: bool,
) -> Div {
    div()
        .min_h(px(ROW_MIN_H))
        .px(px(18.))
        .py(px(14.))
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(14.))
        .when(!last, |d| {
            d.border_b_1().border_color(rgba_of(t.row_divider()))
        })
        .children(tile.map(|n| {
            div()
                .size(px(ROW_TILE))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(scale::RADIUS_MD))
                .bg(rgba_of(t.icon_tile()))
                .child(icon(n, icon_size::NAV, t.text_2))
        }))
        .child(
            div()
                .flex_1()
                .min_w(px(180.))
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(
                    div()
                        .text_size(px(type_scale::UI))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgba_of(t.text))
                        .child(label.into()),
                )
                .children(hint.map(|h| {
                    div()
                        .text_size(px(ROW_HINT_SIZE))
                        .text_color(rgba_of(t.mist))
                        .child(h)
                })),
        )
        .children(control.map(|c| div().flex().flex_none().items_center().gap(px(8.)).child(c)))
}

/// The icon tile of a settings row, px.
pub const ROW_TILE: f32 = 34.;
pub const ROW_MIN_H: f32 = 68.;
pub const ROW_HINT_SIZE: f32 = 12.5;

/// A footer action row of a settings group ("Add local folder...", "Add app..."): same padding as
/// a row, no tile, the action left-aligned.
pub fn footer_row(t: &Theme, action: AnyElement) -> Div {
    let _ = t;
    div()
        .px(px(18.))
        .py(px(12.))
        .flex()
        .items_center()
        .child(action)
}

/// The title row of a settings group: the title (13/600) with an optional faint note beside it.
pub fn group_header(t: &Theme, title: &str, note: Option<SharedString>) -> Div {
    div()
        .flex()
        .items_baseline()
        .gap(px(12.))
        .child(
            div()
                .text_size(px(type_scale::UI))
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgba_of(t.text_2))
                .child(SharedString::from(title.to_owned())),
        )
        .children(note.map(|n| {
            div()
                .text_size(px(type_scale::CAPTION))
                .text_color(rgba_of(t.mist))
                .child(n)
        }))
}

/// A settings group around a card the caller built (so it can carry its own id, role and aria
/// label): [`group_header`] over `card`, 10 px apart.
pub fn group_with_card(
    t: &Theme,
    title: &str,
    note: Option<SharedString>,
    card: impl IntoElement,
) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(10.))
        .child(group_header(t, title, note))
        .child(card)
}

/// Page header with the kit's icon tile (50 px, radius 14, `surface`, 1 px `line`, a 24 px icon),
/// the H1 (28/600) and the one-line subtitle, and an optional right-aligned element.
pub fn page_header_tile(
    t: &Theme,
    tile: &'static str,
    title: &str,
    subtitle: &str,
    right: Option<AnyElement>,
) -> impl IntoElement {
    div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(14.))
        .child(
            div()
                .size(px(HEADER_TILE))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(scale::RADIUS_LG))
                .border_1()
                .border_color(rgba_of(t.line))
                .bg(rgba_of(t.surface))
                .child(icon(tile, 24., t.text)),
        )
        .child(
            div()
                .mr_auto()
                .flex()
                .flex_col()
                .gap(px(4.))
                .child(
                    div()
                        .text_size(px(28.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(SharedString::from(title.to_owned())),
                )
                .child(
                    div()
                        .text_size(px(type_scale::UI))
                        .text_color(rgba_of(t.text_3))
                        .child(SharedString::from(subtitle.to_owned())),
                ),
        )
        .children(right)
}

/// The header icon tile of a settings page, px.
pub const HEADER_TILE: f32 = 50.;

/// A caption line that carries a warning (`warning` colour, 12 px), optionally as an alert.
pub fn warning_caption(t: &Theme, text: impl Into<SharedString>) -> Div {
    div()
        .text_size(px(type_scale::CAPTION))
        .text_color(rgba_of(t.warning))
        .child(text.into())
}

/// A settings group: a title row (with an optional note beside it), then one card whose `rows`
/// ([`settings_row`]s, the last with `last = true` unless a `footer` follows) are separated by
/// 1 px dividers, and an optional [`footer_row`].
#[allow(dead_code)] // for pages that build their rows as a list
pub fn settings_group(
    t: &Theme,
    title: &str,
    note: Option<SharedString>,
    rows: Vec<AnyElement>,
    footer: Option<AnyElement>,
) -> Div {
    group_with_card(t, title, note, list_card(t).children(rows).children(footer))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Color;

    #[test]
    fn segment_geometry_follows_the_kit() {
        assert_eq!(SEGMENT_PAD, 3.);
        // Track = padding + segment + padding + the 1 px border on each side = a 40 px control.
        assert_eq!(SEGMENT_H + 2. * SEGMENT_PAD + 2., scale::CONTROL_H);
    }

    #[test]
    fn settings_row_geometry_follows_the_kit() {
        assert_eq!(ROW_TILE, 34.);
        assert_eq!(ROW_HINT_SIZE, 12.5);
        const { assert!(ROW_MIN_H >= ROW_TILE + 2. * 14.) };
    }

    /// The kit's row, tile and segment colours exist in both themes and stay distinct.
    #[test]
    fn row_colours() {
        let d = Theme::DARK;
        assert_eq!(d.segment_selected(), Color::rgb(0x2A2A30));
        assert_eq!(d.icon_tile(), Color::rgb(0x1A1A1E));
        assert_eq!(d.row_divider(), Color::rgb(0x1E1E22));
        assert_ne!(d.segment_selected(), d.field);
        let l = Theme::LIGHT;
        assert_ne!(l.segment_selected(), l.field);
        assert_ne!(l.row_divider(), l.surface);
    }
}
