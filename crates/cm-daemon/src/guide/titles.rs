//! Draft step titles (GDE-1): `Click "Save"` from the text recognized nearest the click.

use cm_render::ocr::TextLine;

/// Farthest a line may be from the click (physical px) to name it.
pub const MAX_DISTANCE: f64 = 80.0;
/// Longest label kept in a title, in characters.
pub const MAX_LABEL_CHARS: usize = 40;

fn distance(line: &TextLine, click: (f64, f64)) -> f64 {
    let r = line.rect.normalized();
    let dx = (r.x - click.0).max(0.0).max(click.0 - (r.x + r.w));
    let dy = (r.y - click.1).max(0.0).max(click.1 - (r.y + r.h));
    dx.hypot(dy)
}

/// The label nearest `click`, or `None` when no text is within [`MAX_DISTANCE`].
pub fn nearest_label(lines: &[TextLine], click: (f64, f64)) -> Option<String> {
    lines
        .iter()
        .filter(|l| !l.text.trim().is_empty())
        .map(|l| (distance(l, click), l))
        .filter(|(d, _)| *d <= MAX_DISTANCE)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, l)| {
            let t: String = l
                .text
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .replace('"', "'");
            if t.chars().count() > MAX_LABEL_CHARS {
                let cut: String = t.chars().take(MAX_LABEL_CHARS - 1).collect();
                format!("{}…", cut.trim_end())
            } else {
                t
            }
        })
}

/// `Click "Save"`, `Double-click "Save"`, `Right-click "Save"`; `Click here` with no text near.
pub fn draft_title(lines: &[TextLine], click: (f64, f64), button: &str, count: u32) -> String {
    let verb = match (button, count) {
        (_, 2..) => "Double-click",
        ("right", _) => "Right-click",
        ("middle", _) => "Middle-click",
        _ => "Click",
    };
    match nearest_label(lines, click) {
        Some(l) => format!("{verb} \"{l}\""),
        None => format!("{verb} here"),
    }
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
            draft_title(&lines, (210.0, 18.0), "left", 1),
            "Click \"Save\""
        );
        assert_eq!(
            draft_title(&lines, (210.0, 18.0), "left", 2),
            "Double-click \"Save\""
        );
        assert_eq!(
            draft_title(&lines, (210.0, 18.0), "right", 1),
            "Right-click \"Save\""
        );
    }

    #[test]
    fn no_text_near_is_click_here() {
        let lines = [line("Save", 10.0, 10.0)];
        assert_eq!(draft_title(&lines, (900.0, 600.0), "left", 1), "Click here");
        assert_eq!(draft_title(&[], (1.0, 1.0), "left", 1), "Click here");
    }

    #[test]
    fn long_labels_are_cut_and_quotes_removed() {
        let lines = [line(&format!("\"{}\"", "word ".repeat(30)), 0.0, 0.0)];
        let t = nearest_label(&lines, (5.0, 5.0)).unwrap();
        assert!(t.chars().count() <= MAX_LABEL_CHARS);
        assert!(!t.contains('"'));
    }
}
