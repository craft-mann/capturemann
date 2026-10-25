//! Redesign building blocks (Phase 0, `design/redesign/CLAUDE.md` component specs): the kit
//! keycap, pills and statuses, callouts, navigation rows and stat tiles. Buttons, the icon
//! button, the switch and the toolbar live in `components`; settings rows, groups and the
//! segmented control in `settings::widgets`; the tooltip in `tooltip`.

use gpui::{
    div, prelude::*, px, AnyElement, Div, ElementId, FontWeight, InteractiveElement, Role,
    SharedString, Stateful, Toggled,
};

use crate::theme::{scale, type_scale, Color, Mode, Theme};
use crate::ui::components::{ellipsis_text, icon, icon_size};
use crate::ui::paint::{rgba_of, TextExt};

pub const KEYCAP_MIN_W: f32 = 26.;
pub const KEYCAP_H: f32 = 28.;
pub const PILL_H: f32 = 24.;

/// One key of a shortcut at the kit's size: min 26 x 28, `raised`, a 2 px bottom border, Geist
/// Mono 13 (kit "Keycap"). `components::keycap` is the compact form for tooltips and hints.
pub fn keycap_lg(t: &Theme, label: impl Into<SharedString>) -> Div {
    div()
        .min_w(px(KEYCAP_MIN_W))
        .h(px(KEYCAP_H))
        .px(px(7.))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .rounded(px(scale::RADIUS_XS))
        .border_1()
        .border_b_2()
        .border_color(rgba_of(t.line))
        .bg(rgba_of(t.raised))
        .text_size(px(13.))
        .text_color(rgba_of(t.text_2))
        .mono()
        .child(label.into())
}

const MODIFIER_SYMBOLS: &str = "\u{2318}\u{2325}\u{21E7}\u{2303}";

/// Splits a shortcut into one string per key: `⌥⇧R` gives `⌥`, `⇧`, `R`; `⌘Esc` gives `⌘`,
/// `Esc`; `Ctrl+Shift+R` gives `Ctrl`, `Shift`, `R`; `Ctrl++` ends in a `+` key.
pub fn split_shortcut(s: &str) -> Vec<String> {
    let s = s.trim();
    if s.is_empty() {
        return Vec::new();
    }
    if s.chars().count() > 1 && s.contains('+') {
        let mut keys: Vec<String> = s.split('+').map(|k| k.trim().to_owned()).collect();
        if keys.last().is_some_and(String::is_empty) {
            keys.pop();
            keys.push("+".to_owned());
        }
        keys.retain(|k| !k.is_empty());
        return keys;
    }
    let mut keys = Vec::new();
    let mut rest = s;
    while let Some(c) = rest.chars().next() {
        if !MODIFIER_SYMBOLS.contains(c) {
            break;
        }
        keys.push(c.to_string());
        rest = &rest[c.len_utf8()..];
    }
    if !rest.is_empty() {
        keys.push(rest.to_owned());
    }
    keys
}

/// A shortcut as one [`keycap_lg`] per key (see [`split_shortcut`]).
pub fn shortcut_caps(t: &Theme, shortcut: &str) -> Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(4.))
        .children(
            split_shortcut(shortcut)
                .into_iter()
                .map(|k| keycap_lg(t, k)),
        )
}

/// The tone of a pill or callout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// Built in, Coming soon.
    Neutral,
    /// Brass / warning: Default, Reconnect needed.
    Brass,
    Success,
    /// Anything an agent did or asks for.
    Agent,
    /// Failed or blocked: the soft rec on a rec tint.
    Danger,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToneColors {
    /// Label colour.
    pub fg: Color,
    /// Leading icon colour.
    pub icon: Color,
    /// Tinted fill (none for neutral).
    pub bg: Color,
    /// 1 px border (`line` for neutral).
    pub border: Color,
}

/// Fill and border alpha of a tinted tone, percent (kit: 6 to 14 and 30 to 45).
pub const TONE_BG_ALPHA: u32 = 10;
pub const TONE_BORDER_ALPHA: u32 = 38;

pub fn tone_colors(t: &Theme, tone: Tone) -> ToneColors {
    let dark = t.mode == Mode::Dark;
    let tinted = |c: Color, label: Color| ToneColors {
        fg: label,
        icon: c,
        bg: c.with_alpha(TONE_BG_ALPHA),
        border: c.with_alpha(TONE_BORDER_ALPHA),
    };
    match tone {
        Tone::Neutral => ToneColors {
            fg: t.text_3,
            icon: t.text_3,
            bg: Color(0),
            border: t.line,
        },
        // The kit gives no light warning or success: their label uses `text_2` there.
        Tone::Brass => tinted(t.warning, if dark { t.warning } else { t.text_2 }),
        Tone::Success => tinted(t.success, if dark { t.success } else { t.text_2 }),
        Tone::Agent => tinted(t.agent, t.agent_text),
        Tone::Danger => tinted(t.rec_soft, if dark { t.rec_soft } else { t.text_2 }),
    }
}

/// Pill: 24 px, 1 px border, a leading 12 px icon (replaces the dot), label 12 px.
pub fn pill(
    t: &Theme,
    tone: Tone,
    leading: Option<&'static str>,
    label: impl Into<SharedString>,
) -> Div {
    let c = tone_colors(t, tone);
    div()
        .h(px(PILL_H))
        .px(px(10.))
        .flex()
        .flex_none()
        .items_center()
        .gap(px(icon_size::PILL_GAP))
        .rounded(px(scale::RADIUS_PILL))
        .border_1()
        .border_color(rgba_of(c.border))
        .bg(rgba_of(c.bg))
        .max_w_full()
        .text_size(px(type_scale::CAPTION))
        .text_color(rgba_of(c.fg))
        .children(leading.map(|n| icon(n, icon_size::PILL, c.icon)))
        .child(ellipsis_text(label))
}

/// What a status says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusKind {
    Ok,
    Warning,
    Error,
    /// Checking, loading: a spinner glyph (static with reduced motion).
    Pending,
    Neutral,
}

/// The tone and icon of a status (`design/icons/icon-map.md`). Warning and Error differ in tone
/// and in glyph, so they are not told apart by colour alone.
pub fn status_parts(kind: StatusKind) -> (Tone, &'static str) {
    match kind {
        StatusKind::Ok => (Tone::Success, "circle-check"),
        StatusKind::Warning => (Tone::Brass, "triangle-alert"),
        StatusKind::Error => (Tone::Danger, "octagon-alert"),
        StatusKind::Pending => (Tone::Neutral, "loader-circle"),
        StatusKind::Neutral => (Tone::Neutral, "circle-dashed"),
    }
}

/// A status as a [`pill`].
pub fn status(t: &Theme, kind: StatusKind, label: impl Into<SharedString>) -> Div {
    let (tone, glyph) = status_parts(kind);
    pill(t, tone, Some(glyph), label)
}

/// Callout: a tinted card with a leading icon, a title, an optional body line and an optional
/// action on the right (the permission warning, the AI local banner, the agent chip).
pub fn callout(
    t: &Theme,
    tone: Tone,
    glyph: &'static str,
    title: impl Into<SharedString>,
    body: Option<SharedString>,
    action: Option<AnyElement>,
) -> Div {
    let c = tone_colors(t, tone);
    div()
        .p(px(14.))
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(icon_size::CARD_GAP))
        .rounded(px(scale::RADIUS_LG))
        .border_1()
        .border_color(rgba_of(c.border))
        .bg(rgba_of(c.bg))
        .child(icon(glyph, icon_size::CARD, c.icon))
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
                        .child(title.into()),
                )
                .children(body.map(|b| {
                    div()
                        .text_size(px(13.))
                        .text_color(rgba_of(t.text_3))
                        .child(b)
                })),
        )
        .children(action)
}

/// The accessible name of a nav row: the label, with its count when it has one.
pub fn nav_aria(label: &str, count: Option<usize>) -> String {
    match count {
        Some(n) => format!("{label}, {n}"),
        None => label.to_owned(),
    }
}

/// A navigation row (sidebar, settings sidebar): 40 px, radius 8, an 18 px icon, a 10 px gap, the
/// label, an optional right-aligned count (`mist`, 12 px, tabular), `raised` when active. The
/// caller adds `.on_click`.
pub fn nav_row(
    t: &Theme,
    id: impl Into<ElementId>,
    glyph: &'static str,
    label: impl Into<SharedString>,
    count: Option<usize>,
    active: bool,
) -> Stateful<Div> {
    let label: SharedString = label.into();
    let (hover, focus) = (rgba_of(t.raised), rgba_of(t.text));
    let fg = if active { t.text } else { t.text_2 };
    div()
        .id(id)
        .role(Role::Button)
        .aria_label(SharedString::from(nav_aria(&label, count)))
        .aria_toggled(if active {
            Toggled::True
        } else {
            Toggled::False
        })
        .focusable()
        .tab_stop(true)
        .h(px(scale::CONTROL_H))
        .px(px(10.))
        .flex()
        .items_center()
        .gap(px(icon_size::NAV_GAP))
        .rounded(px(scale::RADIUS_SM))
        .border_1()
        .border_color(gpui::transparent_black())
        .text_size(px(type_scale::UI))
        .text_color(rgba_of(fg))
        .cursor_pointer()
        .when(active, |d| {
            d.bg(rgba_of(t.raised)).font_weight(FontWeight::MEDIUM)
        })
        .when(!active, |d| d.hover(move |s| s.bg(hover)))
        .focus_visible(move |s| s.border_color(focus))
        .child(icon(glyph, icon_size::NAV, fg))
        .child(div().flex_1().truncate().child(label))
        .children(count.map(|n| {
            div()
                .flex_none()
                .text_size(px(type_scale::CAPTION))
                .text_color(rgba_of(t.mist))
                .tnum()
                .child(SharedString::from(n.to_string()))
        }))
}

/// A stat tile: a big tabular value over a caption (library counts, the details rail).
pub fn stat_tile(t: &Theme, label: impl Into<SharedString>, value: impl Into<SharedString>) -> Div {
    div()
        .min_w(px(120.))
        .p(px(14.))
        .flex()
        .flex_col()
        .gap(px(4.))
        .rounded(px(scale::RADIUS_LG))
        .border_1()
        .border_color(rgba_of(t.border))
        .bg(rgba_of(t.surface))
        .child(
            div()
                .text_size(px(type_scale::H2))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgba_of(t.text))
                .tnum()
                .child(value.into()),
        )
        .child(
            div()
                .text_size(px(type_scale::CAPTION))
                .text_color(rgba_of(t.mist))
                .child(label.into()),
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::contrast;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| (*x).to_owned()).collect()
    }

    #[test]
    fn shortcuts_split_into_keys() {
        assert_eq!(
            split_shortcut("\u{2325}\u{21E7}R"),
            s(&["\u{2325}", "\u{21E7}", "R"])
        );
        assert_eq!(split_shortcut("\u{2318}Esc"), s(&["\u{2318}", "Esc"]));
        assert_eq!(
            split_shortcut("\u{2325}\u{21E7}1"),
            s(&["\u{2325}", "\u{21E7}", "1"])
        );
        assert_eq!(split_shortcut("Ctrl+Shift+R"), s(&["Ctrl", "Shift", "R"]));
        assert_eq!(split_shortcut("Ctrl++"), s(&["Ctrl", "+"]));
        assert_eq!(split_shortcut("+"), s(&["+"]));
        assert_eq!(split_shortcut("F5"), s(&["F5"]));
        assert_eq!(split_shortcut("\u{21B5}"), s(&["\u{21B5}"]));
        assert_eq!(split_shortcut("\u{2318}"), s(&["\u{2318}"]));
        assert!(split_shortcut("  ").is_empty());
    }

    #[test]
    fn keycap_size_follows_the_kit() {
        assert_eq!((KEYCAP_MIN_W, KEYCAP_H), (26., 28.));
        assert_eq!(PILL_H, 24.);
    }

    /// Tinted tones sit at 6 to 14 % fill and 30 to 45 % border; neutral has no fill.
    #[test]
    fn tone_tints_stay_in_the_kit_range() {
        let pct = |c: Color| f64::from(c.a()) / 255.0 * 100.0;
        for theme in [Theme::DARK, Theme::LIGHT] {
            for tone in [Tone::Brass, Tone::Success, Tone::Agent, Tone::Danger] {
                let c = tone_colors(&theme, tone);
                assert!((6.0..=14.0).contains(&pct(c.bg)), "{tone:?}");
                assert!((30.0..=45.0).contains(&pct(c.border)), "{tone:?}");
            }
            assert_eq!(tone_colors(&theme, Tone::Neutral).bg.a(), 0);
            assert_eq!(tone_colors(&theme, Tone::Neutral).border, theme.line);
        }
    }

    #[test]
    fn tone_labels_are_readable() {
        for theme in [Theme::DARK, Theme::LIGHT] {
            for tone in [
                Tone::Neutral,
                Tone::Brass,
                Tone::Success,
                Tone::Agent,
                Tone::Danger,
            ] {
                let c = tone_colors(&theme, tone);
                let bg = c.bg.over(theme.surface);
                let bg = if c.bg.a() == 0 { theme.surface } else { bg };
                assert!(contrast(c.fg, bg) >= 4.5, "{:?} {tone:?}", theme.mode);
            }
        }
        let d = Theme::DARK;
        assert_eq!(tone_colors(&d, Tone::Agent).icon, d.agent);
        assert_eq!(tone_colors(&d, Tone::Agent).fg, d.agent_text);
        assert_eq!(tone_colors(&d, Tone::Danger).fg, d.rec_soft);
    }

    #[test]
    fn statuses_differ_in_shape_not_only_colour() {
        let all = [
            StatusKind::Ok,
            StatusKind::Warning,
            StatusKind::Error,
            StatusKind::Pending,
            StatusKind::Neutral,
        ];
        let glyphs: std::collections::HashSet<_> = all.iter().map(|k| status_parts(*k).1).collect();
        assert_eq!(glyphs.len(), all.len());
        assert_eq!(status_parts(StatusKind::Ok).0, Tone::Success);
        assert_eq!(status_parts(StatusKind::Warning).0, Tone::Brass);
    }

    #[test]
    fn nav_aria_names_the_count() {
        assert_eq!(nav_aria("Recordings", Some(24)), "Recordings, 24");
        assert_eq!(nav_aria("Settings", None), "Settings");
    }
}
