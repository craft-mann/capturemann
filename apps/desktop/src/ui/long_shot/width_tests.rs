//! The Long Shot bar against long translations, without a window (design S20): measured in the
//! bundled Geist fonts like `layout_width_tests`, in English, in Vietnamese and 40 % longer than
//! English. The rule asserted is "scrollWidth <= clientWidth" for every part that is shown, at
//! several display widths, plus the S20 label rules (mode label at most 14 characters; counter
//! and size never truncate, so they are always inside the window).

use cm_i18n::{with_language, Lang};

use super::bar::{self, BarPlan, BarTexts, MODE_LABEL_MAX};
use crate::text_metrics_tests::{width, Face};

const MONO: Face = Face::MonoRegular;
const SANS: Face = Face::Regular;

fn texts(lang: Lang) -> BarTexts {
    with_language(lang, BarTexts::longest)
}

/// The same texts 40 % longer (the Vietnamese pseudo-length of S20).
fn longer(t: &BarTexts) -> BarTexts {
    let grow = |s: &str| {
        let extra = (s.chars().count() as f32 * 0.4).ceil() as usize;
        format!("{s}{}", "n".repeat(extra))
    };
    BarTexts {
        frames: grow(&t.frames),
        length: t.length.clone(),
        scroll_back: grow(&t.scroll_back),
        mode: grow(&t.mode),
        esc: grow(&t.esc),
    }
}

/// What the shown parts need at least, measured in the fonts (px), with the fixed chrome of the
/// bar taken from the same constants as the plan.
fn needed(t: &BarTexts, p: &BarPlan) -> f32 {
    let count = width(&t.frames, MONO, 15.0) + 16.0;
    let second = if p.second_line {
        width(&t.length, MONO, 11.0).max(width(&t.scroll_back, SANS, 11.0) + 16.0)
    } else {
        0.0
    };
    let counter = 12.0 + count.max(second);
    let mode = 26.0
        + if p.mode_label {
            6.0 + width(&t.mode, SANS, 12.0)
        } else {
            0.0
        };
    let mut parts = vec![44.0, counter, 1.0, 80.0, 40.0, mode];
    if p.esc_hint {
        parts.push(14.0 + width("Esc", MONO, 12.0) + 6.0 + width(&t.esc, SANS, 12.0) + 6.0);
    }
    14.0 + parts.iter().sum::<f32>() + 6.0 * (parts.len() - 1) as f32
}

/// The estimate the plan works with stays above the measured width of every text of the bar, in
/// all three languages.
#[test]
fn the_estimate_is_never_below_the_real_width() {
    for lang in [Lang::En, Lang::Vi, Lang::EnXa] {
        let t = texts(lang);
        for (what, s, size, mono) in [
            ("frames", &t.frames, 15.0, true),
            ("length", &t.length, 11.0, true),
            ("scroll back", &t.scroll_back, 11.0, false),
            ("mode", &t.mode, 12.0, false),
            ("esc", &t.esc, 12.0, false),
        ] {
            let real = width(s, if mono { MONO } else { SANS }, size as f32);
            let est = bar::est(s, size, mono) as f32;
            assert!(
                est >= real,
                "{lang:?} {what} {s:?}: estimate {est:.1} px < real {real:.1} px"
            );
        }
    }
}

/// S20: whatever is shown fits the window, in English, Vietnamese and 40 % longer, from a wide
/// display down to a narrow one; the window never exceeds 640 px or the display minus 48.
#[test]
fn the_shown_parts_fit_the_bar_in_every_language() {
    for (name, t) in [
        ("EN", texts(Lang::En)),
        ("VI", texts(Lang::Vi)),
        ("EN +40%", longer(&texts(Lang::En))),
        ("VI +40%", longer(&texts(Lang::Vi))),
    ] {
        for display in [2560.0, 1440.0, 800.0, 700.0, 600.0, 520.0, 480.0, 420.0] {
            for preview_button in [false, true] {
                let p = bar::plan(display, &t, preview_button);
                let w = p.width as f32;
                assert!(
                    w <= bar::cap(display) as f32 + 0.5 && w <= bar::BAR_MAX as f32,
                    "{name} on {display}: window {w} px over the cap"
                );
                let extra = if preview_button { 46.0 } else { 0.0 };
                let need = needed(&t, &p) + extra;
                let bare = BarPlan {
                    esc_hint: false,
                    mode_label: false,
                    second_line: false,
                    width: 0.0,
                };
                if needed(&t, &bare) + extra > bar::cap(display) as f32 {
                    // Narrower than the bare bar (Stop, count, direction, Pause, mode icon):
                    // nothing more can give way.
                    continue;
                }
                assert!(
                    need <= w + 0.5,
                    "{name} on {display} (preview button {preview_button}): the shown parts need \
                     {need:.0} px, the window is {w:.0} px"
                );
            }
        }
    }
}

/// S20: the Esc hint goes first, then the mode label, then the second line, in this order for
/// every language as the display narrows.
#[test]
fn the_collapse_order_is_the_same_in_every_language() {
    for lang in [Lang::En, Lang::Vi] {
        let t = texts(lang);
        let mut last = (true, true, true);
        let mut seen_all = false;
        for display in (400..=1400).rev().step_by(10) {
            let p = bar::plan(f64::from(display), &t, false);
            let now = (p.esc_hint, p.mode_label, p.second_line);
            assert!(
                now.0 <= last.0 && now.1 <= last.1 && now.2 <= last.2,
                "{lang:?} on {display}: {now:?} after {last:?}"
            );
            // The mode label never goes while the hint is still there; the second line never
            // goes while the label is.
            assert!(!now.0 || now.1, "{lang:?} on {display}: hint without label");
            assert!(!now.1 || now.2, "{lang:?} on {display}: label without size");
            seen_all |= now == (true, true, true);
            last = now;
        }
        assert!(seen_all, "{lang:?}: the whole bar never fits");
    }
}

/// S20: the mode label is at most 14 characters (it does not wrap), in every language.
#[test]
fn the_mode_labels_are_at_most_14_characters() {
    for lang in [Lang::En, Lang::Vi] {
        let (auto, hand) = with_language(lang, || (cm_i18n::tr!("Auto"), cm_i18n::tr!("Hand")));
        for label in [auto, hand] {
            assert!(
                label.chars().count() <= MODE_LABEL_MAX,
                "{lang:?}: {label:?} has more than {MODE_LABEL_MAX} characters"
            );
        }
    }
}

/// The passive bar (Stop, the count, an Agent chip) fits the narrowest display the S20 rule
/// allows (the display minus 48 leaves a window of at least 300 px on a 348 px display).
#[test]
fn the_passive_bar_fits_in_every_language() {
    for lang in [Lang::En, Lang::Vi] {
        let (t, chip) = with_language(lang, || (BarTexts::longest(), bar::agent_chip_text()));
        let w = bar::passive_width(&t, &chip) as f32;
        let counter =
            12.0 + (width(&t.frames, MONO, 15.0) + 16.0).max(width(&t.length, MONO, 11.0));
        let chip_w = 40.0 + width(&chip, SANS, 12.0);
        let need = 14.0 + 44.0 + counter + chip_w + 12.0;
        assert!(
            need <= w + 0.5,
            "{lang:?}: needs {need:.0} px, window {w:.0} px"
        );
        assert!(w <= bar::BAR_MAX as f32);
    }
}
