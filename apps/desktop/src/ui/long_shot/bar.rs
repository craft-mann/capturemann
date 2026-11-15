//! How wide the Long Shot HUD is and what gives way on a narrow display (design S18 and S20).
//! Pure arithmetic on estimated text widths, so the window can be opened at one fixed width (the
//! HUD does not jump while the run goes on) and the rule is tested without a window.
//!
//! S20 "Long Shot bar": at most 640 px and at most the display width minus 48. The counter and
//! the size are mono and never truncate; the mode label is at most 14 characters and never wraps.
//! What gives way first: 1. the "Esc to stop" hint, 2. the mode label (the icon stays), 3. the
//! second line (the size). The collapsed parts keep their full text as tooltip and accessible
//! name.

use cm_i18n::tr;

/// The widest the bar gets, px.
pub const BAR_MAX: f64 = 640.0;
/// Room kept between the bar and the display edges, px (both sides together).
pub const DISPLAY_MARGIN: f64 = 48.0;
/// The mode label is at most this many characters (S20).
pub const MODE_LABEL_MAX: usize = 14;

/// Window padding (6 px each side) and the 1 px border.
const FRAME: f64 = 12.0 + 2.0;
const GAP: f64 = 6.0;
const STOP: f64 = 44.0;
const DIVIDER: f64 = 1.0;
/// The direction label: two 36 px cells, 2 px padding, 2 px gap, 1 px border.
const DIRECTION: f64 = 72.0 + 4.0 + 2.0 + 2.0;
const PAUSE: f64 = 40.0;
/// The mode: a 14 px icon, a 6 px gap, 6 px padding each side.
const MODE_ICON: f64 = 14.0 + 12.0;
const MODE_GAP: f64 = 6.0;
/// The Esc hint: a keycap (7 px padding twice, mono "Esc"), a 6 px gap, 6 px padding.
const KEYCAP_PAD: f64 = 14.0;
const HINT_GAP: f64 = 6.0;
const HINT_PAD: f64 = 6.0;
/// The counter block: 6 px padding each side.
const COUNTER_PAD: f64 = 12.0;
/// The count row: a gap of 8 px and the 8 px dot after the number.
const DOT_SLOT: f64 = 8.0 + 8.0;
/// The Agent chip of the passive bar: 10 px padding twice, a 12 px icon and a 6 px gap.
const CHIP_FIXED: f64 = 20.0 + 12.0 + 6.0 + 2.0;

/// An upper bound of a text's width in px: a mono face is 0.62 em, the proportional one 0.64 em
/// (measured against the bundled Geist fonts by the layout test, which also checks that this
/// stays above the real width in English, in Vietnamese and 40 % longer).
pub fn est(text: &str, size: f64, mono: bool) -> f64 {
    let em = if mono { 0.62 } else { 0.64 };
    text.chars().count() as f64 * size * em
}

/// The texts the bar is made of, in the current language, at their longest.
#[derive(Debug, Clone, PartialEq)]
pub struct BarTexts {
    /// The frame count at its widest ("999 frames").
    pub frames: String,
    /// The size line at its widest ("16,384 / 16,384 px").
    pub length: String,
    /// "Scroll back up a little".
    pub scroll_back: String,
    /// "Hand" or "Auto".
    pub mode: String,
    /// "to stop" (after the Esc keycap).
    pub esc: String,
}

impl BarTexts {
    pub fn longest() -> BarTexts {
        BarTexts {
            frames: tr!("{n} frames", n = 999),
            length: "16,384 / 16,384 px".to_owned(),
            scroll_back: tr!("Scroll back up a little"),
            // The longer of the two modes.
            mode: [tr!("Auto"), tr!("Hand")]
                .into_iter()
                .max_by_key(|s| s.chars().count())
                .unwrap_or_default(),
            esc: tr!("to stop"),
        }
    }
}

/// What the bar shows and how wide its window is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BarPlan {
    pub esc_hint: bool,
    pub mode_label: bool,
    pub second_line: bool,
    /// The window's width, px.
    pub width: f64,
}

/// The widest the bar may be on a display `display_w` wide.
pub fn cap(display_w: f64) -> f64 {
    BAR_MAX.min((display_w - DISPLAY_MARGIN).max(0.0))
}

fn counter(t: &BarTexts, second_line: bool) -> f64 {
    let count = est(&t.frames, 15.0, true) + DOT_SLOT;
    let second = if second_line {
        est(&t.length, 11.0, true).max(est(&t.scroll_back, 11.0, false) + DOT_SLOT)
    } else {
        0.0
    };
    COUNTER_PAD + count.max(second)
}

fn width(
    t: &BarTexts,
    preview_button: bool,
    esc_hint: bool,
    mode_label: bool,
    second_line: bool,
) -> f64 {
    let mode = MODE_ICON
        + if mode_label {
            MODE_GAP + est(&t.mode, 12.0, false)
        } else {
            0.0
        };
    let mut parts = vec![
        STOP,
        counter(t, second_line),
        DIVIDER,
        DIRECTION,
        PAUSE,
        mode,
    ];
    if preview_button {
        parts.push(PAUSE);
    }
    if esc_hint {
        parts.push(
            KEYCAP_PAD + est("Esc", 12.0, true) + HINT_GAP + est(&t.esc, 12.0, false) + HINT_PAD,
        );
    }
    FRAME + parts.iter().sum::<f64>() + GAP * (parts.len() - 1) as f64
}

/// The bar for a display `display_w` wide: everything while it fits, then the Esc hint goes, then
/// the mode label, then the second line. `preview_button`: the bar also carries the Preview button.
pub fn plan(display_w: f64, t: &BarTexts, preview_button: bool) -> BarPlan {
    let max = cap(display_w);
    let steps = [
        (true, true, true),
        (false, true, true),
        (false, false, true),
        (false, false, false),
    ];
    for (esc_hint, mode_label, second_line) in steps {
        let w = width(t, preview_button, esc_hint, mode_label, second_line);
        if w <= max {
            return BarPlan {
                esc_hint,
                mode_label,
                second_line,
                width: w.ceil(),
            };
        }
    }
    BarPlan {
        esc_hint: false,
        mode_label: false,
        second_line: false,
        width: max.floor(),
    }
}

/// The passive bar of a Long Shot an agent or the CLI started: Stop, the count and the Agent
/// chip (no direction, Pause or mode). `chip` is the chip's text.
pub fn passive_width(t: &BarTexts, chip: &str) -> f64 {
    let chip_w = CHIP_FIXED + est(chip, 12.0, false);
    (FRAME + STOP + counter(t, true) + chip_w + GAP * 2.0).ceil()
}

/// The mode label's Agent chip text.
pub fn agent_chip_text() -> String {
    tr!("Agent")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts() -> BarTexts {
        BarTexts {
            frames: "999 frames".into(),
            length: "16,384 / 16,384 px".into(),
            scroll_back: "Scroll back up a little".into(),
            mode: "Hand".into(),
            esc: "to stop".into(),
        }
    }

    /// S20: everything fits on a wide display, in at most 640 px.
    #[test]
    fn the_whole_bar_fits_a_wide_display() {
        let p = plan(1920.0, &texts(), false);
        assert!(p.esc_hint && p.mode_label && p.second_line);
        assert!(p.width <= BAR_MAX, "{}", p.width);
    }

    /// S20: the Esc hint is the first thing to go, then the mode label (the icon stays), then the
    /// second line.
    #[test]
    fn what_gives_way_first_on_a_narrow_display() {
        let t = texts();
        let full = width(&t, false, true, true, true);
        let no_hint = width(&t, false, false, true, true);
        let icon_only = width(&t, false, false, false, true);
        let bare = width(&t, false, false, false, false);
        assert!(full > no_hint && no_hint > icon_only && icon_only > bare);
        // Just narrower than the full bar: only the hint goes.
        let p = plan(full - 1.0 + DISPLAY_MARGIN, &t, false);
        assert_eq!(
            (p.esc_hint, p.mode_label, p.second_line),
            (false, true, true)
        );
        let p = plan(no_hint - 1.0 + DISPLAY_MARGIN, &t, false);
        assert_eq!(
            (p.esc_hint, p.mode_label, p.second_line),
            (false, false, true)
        );
        let p = plan(icon_only - 1.0 + DISPLAY_MARGIN, &t, false);
        assert_eq!(
            (p.esc_hint, p.mode_label, p.second_line),
            (false, false, false)
        );
        // The window is never wider than the display allows.
        for w in [300.0, 420.0, 520.0, 700.0, 1440.0] {
            assert!(plan(w, &t, false).width <= cap(w) + 1.0);
        }
    }

    /// S20: at most 640 px and at most the display minus 48.
    #[test]
    fn the_cap_is_640_and_display_minus_48() {
        assert_eq!(cap(1920.0), 640.0);
        assert_eq!(cap(500.0), 452.0);
        assert_eq!(cap(10.0), 0.0);
    }

    /// The passive bar is Stop, the count and the chip: narrower than the full one.
    #[test]
    fn the_passive_bar_is_small() {
        let t = texts();
        let w = passive_width(&t, "Agent");
        assert!(w < width(&t, false, false, false, true) + 60.0);
        assert!(w > 200.0 && w < 400.0, "{w}");
    }
}
