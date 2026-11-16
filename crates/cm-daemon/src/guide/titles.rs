//! Draft step titles (GDE-1): `Click "Save"` from the text recognized nearest the click, and
//! `Click here` when that text is not a user-interface label.
//!
//! The recognized text is **data from the screen**: it can be a name, an address, a number from
//! a table, or noise from a picture. A title only uses it when it looks like the label of a
//! control, by general rules (short, made of words, balanced punctuation, no address, no long
//! number, plausible confidence); otherwise the generic wording is used. The step says which it
//! got (`title_source` in the guide), and the setting `guide.titles_from_screen` (default on)
//! turns the use of screen text off altogether, in which case the screen is not even read.

use cm_render::ocr::TextLine;

/// Farthest a line may be from the click (logical px, times the display scale) to name it.
pub const MAX_DISTANCE: f64 = 80.0;
/// Longest label used in a title, in characters. A longer line is a sentence, not a label.
pub const MAX_LABEL_CHARS: usize = 40;
/// Most words of a label.
pub const MAX_LABEL_WORDS: usize = 6;
/// Longest single word of a label (an id, a hash or a path is longer).
const MAX_WORD_CHARS: usize = 20;
/// Most digits in a label in all (a phone number, a card, a code or a table cell has more).
const MAX_DIGITS: usize = 3;
/// A reported confidence below this (and above 0, which engines use for "not reported") makes
/// the line noise.
const MIN_CONFIDENCE: f32 = 0.5;
/// The settings key of the switch (app-only: an agent cannot change it).
pub const SETTING_TITLES_FROM_SCREEN: &str = "guide.titles_from_screen";

/// How far from the click the text is read, in logical px: the label's own reach
/// ([`MAX_DISTANCE`]) plus room for a wide label and a line height. Only this neighbourhood is
/// sent to the text reader, not the whole screen.
pub const READ_RADIUS_X: f64 = MAX_DISTANCE + 260.0;
pub const READ_RADIUS_Y: f64 = MAX_DISTANCE + 40.0;

fn distance(line: &TextLine, click: (f64, f64)) -> f64 {
    let r = line.rect.normalized();
    let dx = (r.x - click.0).max(0.0).max(click.0 - (r.x + r.w));
    let dy = (r.y - click.1).max(0.0).max(click.1 - (r.y + r.h));
    dx.hypot(dy)
}

/// `text` as a label, or `None` when it does not look like the label of a control.
pub fn clean_label(text: &str, confidence: f32) -> Option<String> {
    if confidence > 0.0 && confidence < MIN_CONFIDENCE {
        return None;
    }
    let t = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let t = t.replace('"', "'");
    let words: Vec<&str> = t.split(' ').filter(|w| !w.is_empty()).collect();
    if words.is_empty() || words.len() > MAX_LABEL_WORDS || t.chars().count() > MAX_LABEL_CHARS {
        return None;
    }
    // Brackets and quotes come in pairs: a stray `)` is a cut-off row of a table or a log.
    for (open, close) in [('(', ')'), ('[', ']'), ('{', '}')] {
        if t.matches(open).count() != t.matches(close).count() {
            return None;
        }
    }
    let letters = t.chars().filter(|c| c.is_alphabetic()).count();
    let digits = t.chars().filter(char::is_ascii_digit).count();
    let one_wide_letter = letters == 1 && t.chars().any(|c| c.is_alphabetic() && !c.is_ascii());
    if (letters < 2 && !one_wide_letter) || digits > MAX_DIGITS || digits > letters {
        return None;
    }
    // Words that are only punctuation (`|||`, `**`) are decoration: at most a third of them.
    let bare = words
        .iter()
        .filter(|w| !w.chars().any(char::is_alphanumeric))
        .count();
    if bare * 3 > words.len() {
        return None;
    }
    for w in &words {
        let core = w.trim_matches(|c: char| !c.is_alphanumeric());
        // `Save)))`: more than a few marks around a word is not text.
        if w.chars().count() - core.chars().count() > 3 && !core.is_empty() {
            return None;
        }
        if core.chars().count() > MAX_WORD_CHARS {
            return None;
        }
        // Inside a word only a hyphen, an apostrophe, `&` and `+` are plain text; `@`, `/`, `.`,
        // `_`, `:` and the like mean an address, a path or an identifier.
        if core
            .chars()
            .any(|c| !c.is_alphanumeric() && !matches!(c, '-' | '\'' | '’' | '&' | '+'))
        {
            return None;
        }
        // A word that mixes letters and digits is a code (`A1B2C3`) unless it is short (`2FA`,
        // `MP4`, `Win11`).
        let has_digit = core.chars().any(|c| c.is_ascii_digit());
        let has_letter = core.chars().any(char::is_alphabetic);
        if has_digit && has_letter && core.chars().count() > 5 {
            return None;
        }
    }
    Some(t)
}

/// The label nearest `click` (physical px) that looks like a control's label, or `None` when no
/// such text is within [`MAX_DISTANCE`] logical px (`scale` physical px each).
pub fn nearest_label(lines: &[TextLine], click: (f64, f64), scale: f64) -> Option<String> {
    let max = MAX_DISTANCE * if scale >= 1.0 { scale } else { 1.0 };
    lines
        .iter()
        .map(|l| (distance(l, click), l))
        .filter(|(d, _)| *d <= max)
        .filter_map(|(d, l)| clean_label(&l.text, l.confidence).map(|t| (d, t)))
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, t)| t)
}

/// `Click`, `Double-click`, `Right-click` or `Middle-click`.
pub fn verb(button: &str, count: u32) -> &'static str {
    match (button, count) {
        (_, 2..) => "Double-click",
        ("right", _) => "Right-click",
        ("middle", _) => "Middle-click",
        _ => "Click",
    }
}

/// `Click "Save"`, or `Click here` with no label.
pub fn compose(button: &str, count: u32, label: Option<&str>) -> String {
    let verb = verb(button, count);
    match label {
        Some(l) => format!("{verb} \"{l}\""),
        None => format!("{verb} here"),
    }
}

/// `Click "Save"`, `Double-click "Save"`, `Right-click "Save"`; `Click here` with no label near.
pub fn draft_title(
    lines: &[TextLine],
    click: (f64, f64),
    scale: f64,
    button: &str,
    count: u32,
) -> String {
    compose(button, count, nearest_label(lines, click, scale).as_deref())
}

/// Whether the setting (`guide.titles_from_screen`) allows naming a step from screen text:
/// on unless it is set to `false`.
pub fn titles_from_screen(settings: &serde_json::Map<String, serde_json::Value>) -> bool {
    settings
        .get(SETTING_TITLES_FROM_SCREEN)
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(t: &str, x: f64, y: f64) -> TextLine {
        TextLine::laid_out(t, x, y, 8.0, 16.0)
    }

    #[test]
    fn names_the_nearest_text() {
        let lines = [line("Cancel", 10.0, 10.0), line("Save", 200.0, 10.0)];
        assert_eq!(
            draft_title(&lines, (210.0, 18.0), 1.0, "left", 1),
            "Click \"Save\""
        );
        assert_eq!(
            draft_title(&lines, (210.0, 18.0), 1.0, "left", 2),
            "Double-click \"Save\""
        );
        assert_eq!(
            draft_title(&lines, (210.0, 18.0), 1.0, "right", 1),
            "Right-click \"Save\""
        );
    }

    #[test]
    fn no_text_near_is_click_here() {
        let lines = [line("Save", 10.0, 10.0)];
        assert_eq!(
            draft_title(&lines, (900.0, 600.0), 1.0, "left", 1),
            "Click here"
        );
        assert_eq!(draft_title(&[], (1.0, 1.0), 1.0, "left", 1), "Click here");
    }

    #[test]
    fn the_distance_is_logical_px_so_a_retina_click_names_text_as_far_away() {
        let lines = [line("No preview", 100.0, 100.0)];
        // 120 physical px below the text: too far at scale 1, 60 points at scale 2.
        let click = (140.0, 100.0 + 16.0 + 120.0);
        assert_eq!(draft_title(&lines, click, 1.0, "left", 1), "Click here");
        assert_eq!(
            draft_title(&lines, click, 2.0, "left", 1),
            "Click \"No preview\""
        );
    }

    /// QA Linux pass 1: `Click "ROW 0006) EEE"`. Recognized text is used only when it looks like
    /// a label; anything else (a sentence, an address, a number, a cut-off table row, noise, a
    /// doubtful read) gives the generic title.
    #[test]
    fn text_that_is_not_a_label_gives_the_generic_title() {
        for noise in [
            "ROW 0006) EEE",
            "Owner: jane.doe@example.com",
            "https://example.com/a/b",
            "/Users/jane/Documents/plan.pdf",
            "4111 1111 1111 1111",
            "+84 912 345 678",
            "Your payment of 120 dollars was received on the third of May",
            &"word ".repeat(30),
            "a8f3c9d2e1b7a6f5c4d3e2f1a0b9c8d7",
            "|||| ~~ ** ##",
            "x",
            "[ Total",
        ] {
            let lines = [line(noise, 0.0, 0.0)];
            assert_eq!(
                draft_title(&lines, (5.0, 5.0), 1.0, "left", 1),
                "Click here",
                "{noise}"
            );
        }
        // A doubtful read is noise too; 0 means the engine does not report a confidence.
        let mut doubtful = line("Save", 0.0, 0.0);
        doubtful.confidence = 0.2;
        assert_eq!(
            draft_title(&[doubtful], (5.0, 5.0), 1.0, "left", 1),
            "Click here"
        );
        let mut unreported = line("Save", 0.0, 0.0);
        unreported.confidence = 0.0;
        assert_eq!(
            draft_title(&[unreported], (5.0, 5.0), 1.0, "left", 1),
            "Click \"Save\""
        );
    }

    #[test]
    fn ordinary_labels_are_kept() {
        for label in [
            "Save",
            "Save and continue",
            "SIGN IN",
            "Sign in with 2FA",
            "Đăng nhập",
            "Next (2 of 3)",
            "Don't allow",
            "Q&A",
            "Open…",
            "保存",
        ] {
            assert_eq!(
                clean_label(label, 0.9).as_deref(),
                Some(label),
                "{label} should be a label"
            );
        }
    }

    #[test]
    fn a_label_cannot_hold_a_quote_and_the_nearest_label_wins_over_nearer_noise() {
        let lines = [line("\"Save\"", 0.0, 0.0)];
        assert_eq!(
            nearest_label(&lines, (5.0, 5.0), 1.0).as_deref(),
            Some("'Save'")
        );
        let lines = [line("ROW 0006) EEE", 0.0, 0.0), line("Save", 60.0, 0.0)];
        assert_eq!(
            draft_title(&lines, (5.0, 5.0), 1.0, "left", 1),
            "Click \"Save\""
        );
    }

    #[test]
    fn the_setting_is_on_unless_it_is_false() {
        let mut m = serde_json::Map::new();
        assert!(titles_from_screen(&m));
        m.insert(SETTING_TITLES_FROM_SCREEN.into(), serde_json::json!(false));
        assert!(!titles_from_screen(&m));
        m.insert(SETTING_TITLES_FROM_SCREEN.into(), serde_json::json!(true));
        assert!(titles_from_screen(&m));
    }
}
