//! Design-kit components: buttons, keycaps, badges, status pills, cards, the logo and icons.

use gpui::{
    div, prelude::*, px, svg, AnyElement, Div, ElementId, FontWeight, InteractiveElement, Role,
    SharedString, Stateful, Svg,
};

use crate::theme::{scale, type_scale, Color, Theme};
use crate::ui::paint::{rgba_of, TextExt};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    /// The neutral primary action (OD-17): dark = `text` fill with ink text; light = ink fill with
    /// white text.
    Primary,
    Secondary,
    Ghost,
    /// Record, Stop, Start recording: rec red with ink text.
    Record,
    /// Destructive confirmation (Delete, Discard): rec red with ink text, used with a trash-2 icon.
    /// Same look as `Record`; the variant says what the button does.
    Danger,
}

/// The colours of a button kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ButtonColors {
    pub bg: Color,
    pub hover: Color,
    /// Label and icon colour.
    pub fg: Color,
    pub border: Color,
}

impl ButtonKind {
    pub fn colors(self, t: &Theme) -> ButtonColors {
        let none = Color(0);
        match self {
            ButtonKind::Primary => ButtonColors {
                bg: t.primary_bg,
                hover: t.primary_hover,
                fg: t.primary_fg,
                border: none,
            },
            ButtonKind::Record | ButtonKind::Danger => ButtonColors {
                bg: t.rec,
                hover: t.rec_hover,
                fg: t.on_rec,
                border: none,
            },
            ButtonKind::Secondary => ButtonColors {
                bg: t.surface_2,
                hover: t.raised,
                fg: t.text,
                border: t.line,
            },
            ButtonKind::Ghost => ButtonColors {
                bg: none,
                hover: t.raised,
                fg: t.text_2,
                border: none,
            },
        }
    }

    /// Filled kinds are semibold.
    fn filled(self) -> bool {
        matches!(
            self,
            ButtonKind::Primary | ButtonKind::Record | ButtonKind::Danger
        )
    }
}

/// Icon sizes and gaps by control (`design/icons/CLAUDE.md` section 3).
pub mod icon_size {
    /// Leading icon on a 40 px `button`, and the gap to its label.
    pub const BUTTON: f32 = 16.;
    pub const BUTTON_GAP: f32 = 8.;
    /// Leading icon on a 32 px `small_button`, a chip or a segment, and the gap.
    pub const SMALL: f32 = 14.;
    pub const SMALL_GAP: f32 = 6.;
    /// Inside pills and status labels.
    pub const PILL: f32 = 12.;
    pub const PILL_GAP: f32 = 6.;
    /// Glyph of an icon-only button and its hit target.
    pub const ICON_BUTTON: f32 = 18.;
    /// Sidebar nav rows, record-sheet rows, HUD.
    pub const NAV: f32 = 18.;
    pub const NAV_GAP: f32 = 10.;
    /// Card and banner leading icon.
    pub const CARD: f32 = 18.;
    pub const CARD_GAP: f32 = 12.;
}

/// Kit button: 40 px tall, radius 10, 14 px. Focus ring on keyboard focus, accessible name
/// (NFR-13). The caller adds `.on_click` and trailing keycaps.
pub fn button(
    t: &Theme,
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    kind: ButtonKind,
) -> Stateful<Div> {
    button_with_icon(t, id, label, kind, None)
}

/// [`button`] with an optional leading icon: 16 px, 8 px gap, the label's colour (ink on the
/// rec buttons). The icon is decorative: the aria label stays the visible label.
pub fn button_with_icon(
    t: &Theme,
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    kind: ButtonKind,
    leading: Option<&'static str>,
) -> Stateful<Div> {
    let label: SharedString = label.into();
    let c = kind.colors(t);
    let (hover, focus) = (c.hover, t.text);
    let px_h = match kind {
        ButtonKind::Secondary => 16.,
        ButtonKind::Ghost => 12.,
        _ => 18.,
    };
    div()
        .id(id)
        .role(Role::Button)
        .aria_label(label.clone())
        .focusable()
        .tab_stop(true)
        .h(px(scale::CONTROL_H))
        .px(px(px_h))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .gap(px(icon_size::BUTTON_GAP))
        .rounded(px(scale::RADIUS_MD))
        .border_1()
        .border_color(rgba_of(c.border))
        .bg(rgba_of(c.bg))
        .text_color(rgba_of(c.fg))
        .text_size(px(type_scale::UI))
        .font_weight(if kind.filled() {
            FontWeight::SEMIBOLD
        } else {
            FontWeight::NORMAL
        })
        .cursor_pointer()
        .hover(move |s| s.bg(rgba_of(hover)))
        .focus_visible(move |s| s.border_color(rgba_of(focus)))
        .children(leading.map(|name| icon(name, icon_size::BUTTON, c.fg)))
        .child(label)
}

/// Icon-only button: an 18 px glyph in a 44 px hit target (radius 10). The aria label is required
/// and is also the tooltip text (`tooltip` adds keycaps). `pressed` shows a toggled state with a
/// background (Snap, Mirror, Webcam), not only a different glyph. The caller adds `.on_click`.
pub fn icon_button(
    t: &Theme,
    id: impl Into<ElementId>,
    glyph: &'static str,
    aria: impl Into<SharedString>,
    pressed: bool,
    tooltip_keys: Option<Vec<String>>,
) -> Stateful<Div> {
    let aria: SharedString = aria.into();
    let (hover, focus) = (rgba_of(t.raised), rgba_of(t.text));
    let fg = if pressed { t.text } else { t.text_2 };
    div()
        .id(id)
        .role(Role::Button)
        .aria_label(aria.clone())
        .aria_toggled(if pressed {
            gpui::Toggled::True
        } else {
            gpui::Toggled::False
        })
        .focusable()
        .tab_stop(true)
        .size(px(scale::TARGET))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .rounded(px(scale::RADIUS_MD))
        .border_1()
        .border_color(gpui::transparent_black())
        .cursor_pointer()
        .when(pressed, |d| d.bg(rgba_of(t.raised)))
        .when(!pressed, |d| d.hover(move |s| s.bg(hover)))
        .focus_visible(move |s| s.border_color(focus))
        .when_some(tooltip_keys, |d, keys| {
            d.tooltip(crate::ui::tooltip::tip(aria.clone(), keys))
        })
        .child(icon(glyph, icon_size::ICON_BUTTON, fg))
}

/// A solid record-red dot.
pub fn rec_dot(t: &Theme, size: f32) -> Div {
    div()
        .size(px(size))
        .flex_none()
        .rounded(px(size / 2.))
        .bg(rgba_of(t.rec))
}

/// A small status dot.
pub fn dot(color: Color, size: f32) -> Div {
    div()
        .size(px(size))
        .flex_none()
        .rounded(px(size / 2.))
        .bg(rgba_of(color))
}

/// One keycap: Geist Mono 12, `raised` background, radius 6, padding 3 x 7.
pub fn keycap(t: &Theme, label: impl Into<SharedString>) -> Div {
    div()
        .min_w(px(22.))
        .px(px(7.))
        .py(px(3.))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .rounded(px(scale::RADIUS_XS))
        .border_1()
        .border_color(rgba_of(t.border))
        .bg(rgba_of(t.raised))
        .text_size(px(type_scale::CAPTION))
        .text_color(rgba_of(t.text_2))
        .mono()
        .child(label.into())
}

/// A row of keycaps for one shortcut.
pub fn keycaps(t: &Theme, labels: &[String]) -> Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(4.))
        .children(labels.iter().map(|l| keycap(t, l.clone())))
}

/// Neutral tag: mono, 1 px `line` border, radius 6 (the "v0.1" badge).
pub fn badge(t: &Theme, text: impl Into<SharedString>) -> Div {
    div()
        .px(px(6.))
        .py(px(1.))
        .flex()
        .flex_none()
        .items_center()
        .rounded(px(scale::RADIUS_XS))
        .border_1()
        .border_color(rgba_of(t.line))
        .text_size(px(11.))
        .text_color(rgba_of(t.mist))
        .mono()
        .child(text.into())
}

/// Status pill: radius 999, 1 px `line`, 12 px, dot plus label.
pub fn status_pill(t: &Theme, dot_color: Color, label: impl Into<SharedString>) -> Div {
    div()
        .h(px(26.))
        .px(px(10.))
        .flex()
        .flex_none()
        .items_center()
        .gap(px(6.))
        .rounded(px(scale::RADIUS_PILL))
        .border_1()
        .border_color(rgba_of(t.line))
        .max_w_full()
        .text_size(px(type_scale::CAPTION))
        .text_color(rgba_of(t.text_2))
        .child(dot(dot_color, 6.))
        .child(ellipsis_text(label))
}

/// Card: `surface`, 1 px `border`, radius 14, padding 16.
pub fn card(t: &Theme) -> Div {
    div()
        .p(px(16.))
        .rounded(px(scale::RADIUS_LG))
        .border_1()
        .border_color(rgba_of(t.border))
        .bg(rgba_of(t.surface))
}

/// Section label: 12 px / 500 in `mist`.
pub fn section_label(t: &Theme, text: &str) -> Div {
    div()
        .text_size(px(type_scale::CAPTION))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgba_of(t.mist))
        .child(SharedString::from(text.to_owned()))
}

/// A switch (36 x 20 track, 16 px thumb) as a 44 px target. The caller adds `.on_click`.
pub fn switch(
    t: &Theme,
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    on: bool,
    enabled: bool,
) -> Stateful<Div> {
    let focus = rgba_of(t.text);
    div()
        .id(id)
        .role(Role::Switch)
        .aria_label(label.into())
        .aria_toggled(if on {
            gpui::Toggled::True
        } else {
            gpui::Toggled::False
        })
        .focusable()
        .tab_stop(true)
        .h(px(scale::TARGET))
        .px(px(4.))
        .flex()
        .flex_none()
        .items_center()
        .rounded(px(scale::RADIUS_SM))
        .border_1()
        .border_color(gpui::transparent_black())
        .when(enabled, |d| d.cursor_pointer())
        .when(!enabled, |d| d.cursor_default().opacity(0.5))
        .focus_visible(move |s| s.border_color(focus))
        .child(
            div()
                .w(px(36.))
                .h(px(20.))
                .rounded(px(10.))
                .p(px(2.))
                .flex()
                .when(on, |d| d.justify_end())
                .bg(rgba_of(if on { t.text } else { t.line }))
                .child(div().size(px(16.)).rounded(px(8.)).bg(rgba_of(if on {
                    t.ground
                } else {
                    t.mist
                }))),
        )
}

/// An official Lucide icon (2 px stroke, round caps) from `assets/icons/<name>.svg`; the name
/// must be listed in `paint::ASSETS`.
pub fn icon(name: &'static str, size: f32, color: Color) -> Svg {
    svg()
        .path(SharedString::from(format!("icons/{name}.svg")))
        .size(px(size))
        .flex_none()
        .text_color(rgba_of(color))
}

/// Text that takes the rest of a flex row and wraps when it is longer (a label beside an icon, a
/// note in a card). GPUI never measures a text item narrower than its whole text, so without
/// `min_w(0)` a longer translation runs out of the row (`locales/README.md`, "Layout: long
/// translations"). English that fits looks the same as a bare text child.
pub fn label_fill(text: impl Into<SharedString>) -> Div {
    div().flex_1().min_w(px(0.)).child(text.into())
}

/// Text that keeps its natural width while it fits and shrinks, wrapping, when the row is too
/// narrow: [`label_fill`] without taking the free space (for centred or spaced-out rows).
pub fn shrink_text(text: impl Into<SharedString>) -> Div {
    div().min_w(px(0.)).child(text.into())
}

/// Single-line text that is cut with an ellipsis when the row is too narrow. Give the control the
/// full text as a tooltip or aria label.
pub fn ellipsis_text(text: impl Into<SharedString>) -> Div {
    div()
        .min_w(px(0.))
        .max_w_full()
        .truncate()
        .child(text.into())
}

/// The in-app logo: three crop corners in `text` plus the rec bubble (viewBox 64: corners
/// stroke 6, bubble r 11 at 47,47). Vector, so crisp at any scale factor.
pub fn logo(t: &Theme, size: f32) -> Div {
    let k = size / 64.;
    let d = 22. * k;
    div()
        .relative()
        .size(px(size))
        .flex_none()
        .child(
            svg()
                .path("brand/mark-corners.svg")
                .size(px(size))
                .text_color(rgba_of(t.text)),
        )
        .child(
            div()
                .absolute()
                .left(px(47. * k - d / 2.))
                .top(px(47. * k - d / 2.))
                .size(px(d))
                .rounded(px(d / 2.))
                .bg(rgba_of(t.rec)),
        )
}

/// The breadcrumb link of a page toolbar ("Screenshots /", "Recordings /"): caption size, muted,
/// a keyboard-focusable button. The caller adds `.on_click` (and a tooltip if it has one).
pub fn crumb_link(
    t: &Theme,
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    aria: impl Into<SharedString>,
) -> Stateful<Div> {
    let (hover_fg, fg, focus) = (rgba_of(t.text), rgba_of(t.mist), rgba_of(t.text));
    div()
        .id(id)
        .role(Role::Button)
        .aria_label(aria)
        .focusable()
        .tab_stop(true)
        .self_start()
        .border_1()
        .border_color(gpui::transparent_black())
        .rounded(px(scale::RADIUS_XS))
        .cursor_pointer()
        .text_size(px(type_scale::CAPTION))
        .text_color(fg)
        .hover(move |s| s.text_color(hover_fg))
        .focus_visible(move |s| s.border_color(focus))
        .child(label.into())
}

/// The top bar of the editors and the details page (kit `Editor` top bar): the crumb over the
/// title on the left, optional actions on the right, a bottom border, wrapping when narrow. One
/// function so the screenshot editor, the recording editor and the recording details page match.
pub fn page_toolbar(
    t: &Theme,
    id: &'static str,
    aria: impl Into<SharedString>,
    crumb: impl IntoElement,
    title: SharedString,
    actions: Option<AnyElement>,
) -> Stateful<Div> {
    page_toolbar_actions(t, id, aria, crumb, title, actions.into_iter().collect())
}

/// [`page_toolbar`] with several actions: each is its own element, laid out in a row 8 px apart
/// that wraps with the bar.
pub fn page_toolbar_actions(
    t: &Theme,
    id: &'static str,
    aria: impl Into<SharedString>,
    crumb: impl IntoElement,
    title: SharedString,
    actions: Vec<AnyElement>,
) -> Stateful<Div> {
    div()
        .id(id)
        .role(Role::Toolbar)
        .aria_label(aria.into())
        .flex_none()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(12.))
        .px(px(24.))
        .py(px(14.))
        .bg(rgba_of(t.ground))
        .border_b_1()
        .border_color(rgba_of(t.border))
        .child(
            div()
                .flex_1()
                .min_w(px(220.))
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(crumb)
                .child(
                    div()
                        .id(SharedString::from(format!("{id}-title")))
                        .role(Role::Heading)
                        .aria_level(1)
                        .aria_label(title.clone())
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_size(px(type_scale::TITLE))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title),
                ),
        )
        .when(!actions.is_empty(), |d| {
            d.child(
                div()
                    .flex()
                    .flex_none()
                    .flex_wrap()
                    .items_center()
                    .gap(px(8.))
                    .children(actions),
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// OD-17: Primary is neutral, Record and Danger are rec red with ink text, and every filled
    /// kind is readable.
    #[test]
    fn button_kinds_follow_od_17() {
        for t in [Theme::DARK, Theme::LIGHT] {
            let p = ButtonKind::Primary.colors(&t);
            assert_eq!((p.bg, p.fg), (t.primary_bg, t.primary_fg));
            assert_ne!(p.bg, t.rec);
            for k in [ButtonKind::Record, ButtonKind::Danger] {
                let c = k.colors(&t);
                assert_eq!((c.bg, c.fg, c.hover), (t.rec, t.on_rec, t.rec_hover));
            }
            assert_eq!(ButtonKind::Record.colors(&t), ButtonKind::Danger.colors(&t));
            for k in [
                ButtonKind::Primary,
                ButtonKind::Record,
                ButtonKind::Danger,
                ButtonKind::Secondary,
            ] {
                let c = k.colors(&t);
                assert!(crate::theme::contrast(c.fg, c.bg) >= 4.5, "{k:?}");
            }
            let s = ButtonKind::Secondary.colors(&t);
            assert_eq!((s.bg, s.border), (t.surface_2, t.line));
        }
    }

    #[test]
    fn icon_sizes_follow_the_icon_guide() {
        use icon_size::*;
        assert_eq!((BUTTON, BUTTON_GAP), (16., 8.));
        assert_eq!((SMALL, SMALL_GAP), (14., 6.));
        assert_eq!((PILL, PILL_GAP), (12., 6.));
        assert_eq!((ICON_BUTTON, NAV, NAV_GAP), (18., 18., 10.));
        const { assert!(ICON_BUTTON <= scale::TARGET) };
        assert_eq!(scale::TARGET, 44.);
    }
}
