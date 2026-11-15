//! Bars that size themselves to their content (design boards S16, S17, S19, S20): pure
//! arithmetic on measured text widths, so the rules are tested without a window.
//!
//! * The Click Guide bar (S17): height 58, width = content, at most 720 and never wider than the
//!   display minus 48. When it would pass its limit the labels turn into 44 px icon buttons (Undo
//!   step, then Note step, then Pause or Resume) and then the second line hides. Stop, the dot and
//!   "Step n" never collapse; a label is never cut with an ellipsis and never wraps.
//! * The Library selection bar (S20): the counter never truncates; Delete, then Export selected,
//!   then Make a board give up their labels; the clear button is always an icon.
//! * The Library header (S16): "Click Guide of a window..." becomes an icon button when the two
//!   guide buttons do not fit in one line.
//!
//! Widths come from [`text_w`], the advance widths of the bundled Geist fonts (the fonts GPUI
//! draws with); kerning is ignored, so a measured width is a hair wider than the drawn text.

use crate::text_metrics_tests::{width, Face};

/// The advance width of `text` in logical px: `mono` is Geist Mono, `weight` picks the face.
pub fn text_w(text: &str, size: f32, mono: bool, weight: u16) -> f64 {
    f64::from(width(text, Face::of(mono, weight), size))
}

/// `s` cut to `max` characters with an ellipsis when it is longer.
pub fn ellipsize(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_owned();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.truncate(out.trim_end().len());
    out.push('\u{2026}');
    out
}

// ---- the Click Guide bar (S17) ------------------------------------------------------------

/// The bar's height, points. It never changes (6 px padding around 44 px targets).
pub const GUIDE_BAR_H: f64 = 58.0;
/// The widest the bar gets, points.
pub const GUIDE_BAR_MAX_W: f64 = 720.0;
/// The room kept between the bar and the display's edge, each side.
pub const DISPLAY_EDGE: f64 = 24.0;
/// The second line shows at most this many characters, then an ellipsis.
pub const SECOND_LINE_CHARS: usize = 40;
/// A followed window's name is cut at this many characters before that.
pub const WINDOW_NAME_CHARS: usize = 28;

pub const BORDER: f64 = 1.0;
pub const PAD: f64 = 6.0;
pub const STOP: f64 = 44.0;
pub const GAP: f64 = 6.0;
pub const TEXT_PAD: f64 = 10.0;
pub const DIVIDER: f64 = 1.0;
pub const BTN_H: f64 = 44.0;
pub const BTN_PAD: f64 = 12.0;
pub const ICON: f64 = 16.0;
pub const ICON_GAP: f64 = 7.0;

/// The widest the bar may be on a display `display_w` points wide.
pub fn guide_bar_limit(display_w: f64) -> f64 {
    GUIDE_BAR_MAX_W.min(display_w - 2.0 * DISPLAY_EDGE).max(0.0)
}

/// One control after the divider. `label_w` is the measured label; `order` is when it gives up
/// its label (lower first), `None` for a text that never collapses.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Slot {
    pub label_w: f64,
    pub order: Option<u8>,
}

impl Slot {
    pub fn button(label_w: f64, order: u8) -> Slot {
        Slot {
            label_w,
            order: Some(order),
        }
    }

    pub fn note(label_w: f64) -> Slot {
        Slot {
            label_w,
            order: None,
        }
    }

    fn width(&self, collapsed: bool) -> f64 {
        match (self.order, collapsed) {
            (Some(_), true) => BTN_H,
            (Some(_), false) => 2.0 * BTN_PAD + ICON + ICON_GAP + self.label_w,
            (None, _) => 2.0 * TEXT_PAD + self.label_w,
        }
    }
}

/// What the bar looks like at one width limit.
#[derive(Debug, Clone, PartialEq)]
pub struct GuideBar {
    /// The bar's width, points (border included).
    pub width: f64,
    /// Per slot: shown as a 44 px icon button.
    pub collapsed: Vec<bool>,
    /// Whether the second line shows.
    pub second: bool,
}

/// Lays the bar out. `head_w` is the dot (or spinner), its gap and the headline; `second_w` the
/// measured second line; `limit` from [`guide_bar_limit`].
pub fn guide_bar(head_w: f64, second_w: f64, slots: &[Slot], limit: f64) -> GuideBar {
    let mut collapsed = vec![false; slots.len()];
    let mut second = true;
    let width_of = |collapsed: &[bool], second: bool| -> f64 {
        let col = head_w.max(if second { second_w } else { 0.0 });
        2.0 * BORDER
            + 2.0 * PAD
            + STOP
            + GAP
            + (2.0 * TEXT_PAD + col)
            + GAP
            + DIVIDER
            + slots
                .iter()
                .zip(collapsed)
                .map(|(s, c)| GAP + s.width(*c))
                .sum::<f64>()
    };
    // The collapse order: by `order`, then the second line.
    let mut order: Vec<usize> = (0..slots.len())
        .filter(|i| slots[*i].order.is_some())
        .collect();
    order.sort_by_key(|i| slots[*i].order);
    for i in order {
        if width_of(&collapsed, second) <= limit {
            break;
        }
        collapsed[i] = true;
    }
    if width_of(&collapsed, second) > limit {
        second = false;
    }
    GuideBar {
        width: width_of(&collapsed, second),
        collapsed,
        second,
    }
}

// ---- the Library selection bar (S20) ------------------------------------------------------

pub const SEL_PAD: f64 = 6.0;
pub const SEL_GAP: f64 = 4.0;
pub const SEL_BTN_H: f64 = 34.0;
pub const SEL_BTN_PAD: f64 = 12.0;
pub const SEL_ICON: f64 = 15.0;
pub const SEL_ICON_GAP: f64 = 7.0;
pub const SEL_COUNTER_PAD: f64 = 12.0;
/// A button label is at most this many characters (the translation rule of S20).
pub const SEL_LABEL_CHARS: usize = 22;

/// The selection bar at one width limit: which of the three buttons keep their label.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SelectionBar {
    pub width: f64,
    /// Make a board, Export selected, Delete.
    pub labels: [bool; 3],
}

/// `counter_w` is the measured "3 selected"; `label_w` the three labels in bar order (Make a
/// board, Export selected..., Delete); `limit` is the content area minus 48. Delete gives up its
/// label first, then Export selected, then Make a board; the clear button is always an icon.
pub fn selection_bar(counter_w: f64, label_w: [f64; 3], limit: f64) -> SelectionBar {
    let width_of = |labels: &[bool; 3]| -> f64 {
        let buttons: f64 = labels
            .iter()
            .zip(label_w)
            .map(|(on, w)| {
                SEL_GAP
                    + if *on {
                        2.0 * SEL_BTN_PAD + SEL_ICON + SEL_ICON_GAP + w
                    } else {
                        SEL_BTN_H
                    }
            })
            .sum();
        2.0 * BORDER
            + 2.0 * SEL_PAD
            + (2.0 * SEL_COUNTER_PAD + counter_w)
            + SEL_GAP
            + 1.0
            + buttons
            + SEL_GAP
            + 1.0
            + SEL_GAP
            + SEL_BTN_H
    };
    let mut labels = [true; 3];
    for drop in [2usize, 1, 0] {
        if width_of(&labels) <= limit {
            break;
        }
        labels[drop] = false;
    }
    SelectionBar {
        width: width_of(&labels),
        labels,
    }
}

// ---- the Library header (S16) -------------------------------------------------------------

/// The width of a 40 px button with a 16 px icon and a label (`px` 16 each side, 1 px border).
pub fn header_button_w(label_w: f64) -> f64 {
    2.0 + 32.0 + 16.0 + 8.0 + label_w
}

/// Whether "Click Guide of a window..." gives up its label: `room` is what the row has left for
/// the two guide buttons.
pub fn window_button_collapses(room: f64, new_label_w: f64, window_label_w: f64) -> bool {
    room < header_button_w(new_label_w) + 8.0 + header_button_w(window_label_w)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slots() -> Vec<Slot> {
        vec![
            Slot::button(40.0, 2),
            Slot::button(58.0, 1),
            Slot::button(56.0, 0),
        ]
    }

    #[test]
    fn a_roomy_display_keeps_every_label() {
        let b = guide_bar(70.0, 130.0, &slots(), guide_bar_limit(1440.0));
        assert_eq!(b.collapsed, [false, false, false]);
        assert!(b.second);
        assert!(b.width < 720.0);
    }

    #[test]
    fn labels_collapse_in_the_designed_order_then_the_second_line() {
        let full = guide_bar(70.0, 130.0, &slots(), 10_000.0).width;
        let mut seen = Vec::new();
        let mut limit = full;
        while limit > 300.0 {
            let b = guide_bar(70.0, 130.0, &slots(), limit);
            let state = (b.collapsed.clone(), b.second);
            if seen.last() != Some(&state) {
                seen.push(state);
            }
            limit -= 1.0;
        }
        let steps: Vec<(Vec<bool>, bool)> = vec![
            (vec![false, false, false], true),
            (vec![false, false, true], true),
            (vec![false, true, true], true),
            (vec![true, true, true], true),
            (vec![true, true, true], false),
        ];
        assert_eq!(seen[..steps.len()], steps[..]);
    }

    #[test]
    fn the_collapse_limit_is_the_smaller_of_720_and_the_display_less_48() {
        assert_eq!(guide_bar_limit(1920.0), 720.0);
        assert_eq!(guide_bar_limit(700.0), 652.0);
    }

    #[test]
    fn a_note_text_never_collapses() {
        let s = vec![Slot::note(80.0), Slot::button(90.0, 0)];
        let b = guide_bar(70.0, 130.0, &s, 100.0);
        assert_eq!(b.collapsed, [false, true]);
        assert!(!b.second);
    }

    #[test]
    fn the_selection_bar_drops_delete_then_export_then_board() {
        let full = selection_bar(70.0, [100.0, 120.0, 40.0], 10_000.0);
        assert_eq!(full.labels, [true, true, true]);
        let mut last = full;
        let mut order = Vec::new();
        let mut limit = full.width;
        while limit > 150.0 {
            let b = selection_bar(70.0, [100.0, 120.0, 40.0], limit);
            if b.labels != last.labels {
                order.push(b.labels);
                last = b;
            }
            limit -= 1.0;
        }
        assert_eq!(
            order,
            [
                [true, true, false],
                [true, false, false],
                [false, false, false]
            ]
        );
    }

    #[test]
    fn the_header_pair_collapses_only_when_both_do_not_fit() {
        let (n, w) = (80.0, 150.0);
        let need = header_button_w(n) + 8.0 + header_button_w(w);
        assert!(!window_button_collapses(need, n, w));
        assert!(window_button_collapses(need - 1.0, n, w));
    }

    /// The selection bar's measured parts in `lang`, with every width stretched by `k` (1.4 is
    /// the board's pseudo-length).
    fn selection(lang: cm_i18n::Lang, n: u64, k: f64, limit: f64) -> (SelectionBar, [String; 3]) {
        cm_i18n::with_language(lang, || {
            let (counter, labels) = crate::ui::library_view::selection_texts(n);
            let w = |s: &str, weight| text_w(s, 13.5, false, weight) * k;
            let bar = selection_bar(
                w(&counter, 500),
                [w(&labels[0], 400), w(&labels[1], 400), w(&labels[2], 400)],
                limit,
            );
            (bar, labels)
        })
    }

    /// S20: the labels are at most 22 characters, the bar never passes the content area less 48
    /// px, and the collapse order is Delete, Export selected, Make a board, in English, in
    /// Vietnamese, in the pseudo-locale and with every width 40 % longer.
    #[test]
    fn s20_the_selection_bar_fits_and_collapses_in_order() {
        for content in [1400.0_f64, 1000.0, 760.0, 620.0, 480.0, 400.0] {
            let limit = content - 48.0;
            for lang in [cm_i18n::Lang::En, cm_i18n::Lang::Vi, cm_i18n::Lang::EnXa] {
                for k in [1.0, 1.4] {
                    let (bar, labels) = selection(lang, 3, k, limit);
                    // The pseudo-locale pads every string by a third: the rule is for real languages.
                    for l in labels.iter().filter(|_| lang != cm_i18n::Lang::EnXa) {
                        assert!(l.chars().count() <= SEL_LABEL_CHARS, "{lang:?}: {l}");
                    }
                    // Delete gives up its label first, Make a board keeps it longest.
                    let on = bar.labels;
                    assert!(!on[2] || on[1], "{on:?}");
                    assert!(!on[1] || on[0], "{on:?}");
                    // Everything collapsed is the narrowest the bar gets; a content area that
                    // cannot hold even that is not one the window can have (its minimum is wider).
                    let narrowest = selection(lang, 3, k, 0.0).0.width;
                    if limit >= narrowest {
                        assert!(
                            bar.width <= limit + 1e-6,
                            "{lang:?} k {k} content {content}: {} > {limit}",
                            bar.width
                        );
                    }
                }
            }
        }
        // A roomy content area keeps every label in English.
        assert_eq!(
            selection(cm_i18n::Lang::En, 3, 1.0, 1400.0 - 48.0).0.labels,
            [true, true, true]
        );
        // And the bar's height does not depend on the count or the language.
        assert_eq!(SEL_BTN_H + 2.0 * SEL_PAD + 2.0 * BORDER, 48.0);
    }

    /// S20: the counter's width is in the bar however many are selected ("12 selected").
    #[test]
    fn s20_the_counter_is_never_truncated() {
        let (few, _) = selection(cm_i18n::Lang::Vi, 3, 1.0, 10_000.0);
        let (many, _) = selection(cm_i18n::Lang::Vi, 128, 1.0, 10_000.0);
        assert!(many.width >= few.width);
    }

    /// S16: in a wide window the header holds both guide buttons; in a narrow one the window
    /// button is an icon.
    #[test]
    fn s16_the_window_button_is_an_icon_only_in_a_narrow_header() {
        let roomy = 1680.0_f32 - 248.0 - 64.0;
        let narrow = 1100.0_f32 - 248.0 - 64.0;
        cm_i18n::with_language(cm_i18n::Lang::En, || {
            assert!(!crate::ui::library_view::guide_buttons_need_icon(
                "All captures",
                Some(214),
                roomy
            ));
            assert!(crate::ui::library_view::guide_buttons_need_icon(
                "All captures",
                Some(214),
                narrow
            ));
        });
    }

    /// QA G-icon: the header width is the window less the sidebar (248) and the page padding
    /// (2 x 32); a 1440 pt window has room for both labelled guide buttons on the Click Guides
    /// page and on All captures, a 900 pt one does not. The numbers are the real label widths.
    #[test]
    fn g_icon_a_1440_window_keeps_both_labels_and_a_900_one_collapses() {
        let row = |window: f32| window - 248.0 - 64.0;
        cm_i18n::with_language(cm_i18n::Lang::En, || {
            for (title, count) in [("Click Guides", Some(3)), ("All captures", Some(214))] {
                assert!(
                    !crate::ui::library_view::guide_buttons_need_icon(title, count, row(1440.0)),
                    "{title}: 1440 pt"
                );
                assert!(
                    crate::ui::library_view::guide_buttons_need_icon(title, count, row(900.0)),
                    "{title}: 900 pt"
                );
            }
        });
    }

    #[test]
    fn a_long_text_is_cut_with_an_ellipsis() {
        assert_eq!(ellipsize("short", 28), "short");
        let cut = ellipsize(
            "Brightdesk \u{2014} Chrome \u{2014} Billing plan and invoices",
            28,
        );
        assert_eq!(cut.chars().count(), 28);
        assert!(cut.ends_with('\u{2026}'));
    }
}
