//! Draft step titles (GDE-1): `Click "Save"` from the text recognized nearest the click.

use cm_render::ocr::TextLine;

/// Farthest a line may be from the click (logical px, times the display scale) to name it.
pub const MAX_DISTANCE: f64 = 80.0;
/// Longest label kept in a title, in characters.
pub const MAX_LABEL_CHARS: usize = 40;

fn distance(line: &TextLine, click: (f64, f64)) -> f64 {
    let r = line.rect.normalized();
    let dx = (r.x - click.0).max(0.0).max(click.0 - (r.x + r.w));
    let dy = (r.y - click.1).max(0.0).max(click.1 - (r.y + r.h));
    dx.hypot(dy)
}

/// The label nearest `click` (physical px), or `None` when no text is within [`MAX_DISTANCE`]
/// logical px (`scale` physical px each).
pub fn nearest_label(lines: &[TextLine], click: (f64, f64), scale: f64) -> Option<String> {
    let max = MAX_DISTANCE * if scale >= 1.0 { scale } else { 1.0 };
    lines
        .iter()
        .filter(|l| !l.text.trim().is_empty())
        .map(|l| (distance(l, click), l))
        .filter(|(d, _)| *d <= max)
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
pub fn draft_title(
    lines: &[TextLine],
    click: (f64, f64),
    scale: f64,
    button: &str,
    count: u32,
) -> String {
    let verb = match (button, count) {
        (_, 2..) => "Double-click",
        ("right", _) => "Right-click",
        ("middle", _) => "Middle-click",
        _ => "Click",
    };
    match nearest_label(lines, click, scale) {
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

    #[test]
    fn long_labels_are_cut_and_quotes_removed() {
        let lines = [line(&format!("\"{}\"", "word ".repeat(30)), 0.0, 0.0)];
        let t = nearest_label(&lines, (5.0, 5.0), 1.0).unwrap();
        assert!(t.chars().count() <= MAX_LABEL_CHARS);
        assert!(!t.contains('"'));
    }
}
