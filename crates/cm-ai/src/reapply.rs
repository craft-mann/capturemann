//! Keeping the user's caption edits when a transcript is made again (AI-6a, piece 3).
//!
//! Caption lines the user edited belong to the old transcript. After a re-generate, an edited line
//! is kept where **the same words were heard again**: the new automatic line must overlap the
//! edited line by at least [`MIN_OVERLAP`] of the shorter of the two spans, and its automatic text
//! must equal the old automatic text of that line. Every other edit is dropped and counted.
//!
//! What counts as an edit: a stored line whose text differs from every automatic line it overlaps
//! (the lines [`crate::caption::build_captions`] builds from the OLD transcript). A line that is
//! only re-timed, or that equals an automatic line, is not an edit and is rebuilt from the new
//! words like the rest. Structural edits (a line split in two, two lines merged, a line added by
//! hand) cannot be mapped one to one and are dropped; they are counted so the user can redo them.
//!
//! Text is compared by whitespace-normalised NFC (case counts: a capital letter is an edit).

use crate::caption::CaptionLine;
use crate::textmatch::{key, plain};

/// How much of the shorter span two lines must share to be "the same line".
pub const MIN_OVERLAP: f64 = 0.5;

/// The outcome of [`reapply_edits`].
#[derive(Debug, Clone, PartialEq)]
pub struct Reapplied {
    /// The new automatic lines, with the user's text on the lines whose edit was kept.
    pub lines: Vec<CaptionLine>,
    /// Edits kept.
    pub reapplied: usize,
    /// Edits that found no matching new line.
    pub dropped: usize,
}

fn same_text(a: &str, b: &str) -> bool {
    key(&plain(a), true) == key(&plain(b), true)
}

fn overlap_ticks(a: &CaptionLine, b: &CaptionLine) -> i64 {
    (a.end.min(b.end) - a.start.max(b.start)).ticks().max(0)
}

fn shares_enough(a: &CaptionLine, b: &CaptionLine) -> Option<i64> {
    let shorter = (a.end - a.start).ticks().min((b.end - b.start).ticks());
    let ov = overlap_ticks(a, b);
    (shorter > 0 && ov as f64 >= shorter as f64 * MIN_OVERLAP).then_some(ov)
}

/// The stored lines the user changed: those whose text differs from every automatic line of the
/// old transcript that they overlap.
pub fn user_edits(old_auto: &[CaptionLine], stored: &[CaptionLine]) -> Vec<CaptionLine> {
    stored
        .iter()
        .filter(|s| {
            !old_auto
                .iter()
                .any(|a| overlap_ticks(a, s) > 0 && same_text(&a.text, &s.text))
        })
        .cloned()
        .collect()
}

/// Maps the user's edits onto the new automatic lines. See the module docs for the rule.
pub fn reapply_edits(
    old_auto: &[CaptionLine],
    stored: &[CaptionLine],
    new_auto: &[CaptionLine],
) -> Reapplied {
    let edits = user_edits(old_auto, stored);
    // claims[n] = the edits that want new line n.
    let mut claims: Vec<Vec<usize>> = vec![Vec::new(); new_auto.len()];
    let mut dropped = 0;
    for (ei, e) in edits.iter().enumerate() {
        let olds: Vec<&CaptionLine> = old_auto
            .iter()
            .filter(|a| shares_enough(a, e).is_some())
            .collect();
        // Exactly one old automatic line: a merged or split line has no one-to-one mapping.
        let target = match olds.as_slice() {
            [one] => new_auto
                .iter()
                .enumerate()
                .filter(|(_, n)| same_text(&n.text, &one.text))
                .filter_map(|(ni, n)| shares_enough(n, e).map(|ov| (ni, ov)))
                .max_by_key(|(_, ov)| *ov)
                .map(|(ni, _)| ni),
            _ => None,
        };
        match target {
            Some(ni) => claims[ni].push(ei),
            None => dropped += 1,
        }
    }
    let mut lines = new_auto.to_vec();
    let mut reapplied = 0;
    for (ni, c) in claims.iter().enumerate() {
        match c.as_slice() {
            [] => {}
            [one] => {
                lines[ni].text = edits[*one].text.clone();
                reapplied += 1;
            }
            // Two edits want the same line (the user split it): neither can stand alone.
            many => dropped += many.len(),
        }
    }
    Reapplied {
        lines,
        reapplied,
        dropped,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcribe::media_secs;

    fn l(s: f64, e: f64, t: &str) -> CaptionLine {
        CaptionLine {
            start: media_secs(s),
            end: media_secs(e),
            text: t.into(),
        }
    }

    #[test]
    fn an_edit_on_the_same_words_is_kept_with_the_new_times() {
        let old = [l(0.0, 2.0, "open logo drive"), l(2.0, 4.0, "then save")];
        let stored = [l(0.0, 2.0, "Open Google Drive"), l(2.0, 4.0, "then save")];
        let new = [l(0.1, 2.2, "open logo drive"), l(2.3, 4.1, "then save")];
        let r = reapply_edits(&old, &stored, &new);
        assert_eq!((r.reapplied, r.dropped), (1, 0));
        assert_eq!(r.lines[0].text, "Open Google Drive");
        assert_eq!(r.lines[0].start, media_secs(0.1));
        assert_eq!(r.lines[1].text, "then save");
    }

    #[test]
    fn an_edit_is_dropped_when_the_words_changed() {
        let old = [l(0.0, 2.0, "open logo drive")];
        let stored = [l(0.0, 2.0, "Open Google Drive")];
        let new = [l(0.0, 2.0, "open logo drives")];
        let r = reapply_edits(&old, &stored, &new);
        assert_eq!((r.reapplied, r.dropped), (0, 1));
        assert_eq!(r.lines, new.to_vec());
    }

    #[test]
    fn a_line_that_barely_overlaps_is_not_the_same_line() {
        let old = [l(0.0, 4.0, "a b c")];
        let stored = [l(0.0, 4.0, "A B C")];
        // The new line shares 1 s of its 4 s: under half of the shorter span.
        let new = [l(3.0, 7.0, "a b c")];
        let r = reapply_edits(&old, &stored, &new);
        assert_eq!((r.reapplied, r.dropped), (0, 1));
        // Exactly half is enough.
        let new = [l(2.0, 6.0, "a b c")];
        assert_eq!(reapply_edits(&old, &stored, &new).reapplied, 1);
    }

    #[test]
    fn unedited_lines_and_empty_inputs_are_not_counted() {
        let old = [l(0.0, 2.0, "a"), l(2.0, 4.0, "b")];
        let r = reapply_edits(&old, &old, &[l(0.0, 2.0, "a2")]);
        assert_eq!((r.reapplied, r.dropped), (0, 0));
        assert_eq!(r.lines[0].text, "a2");
        let r = reapply_edits(&[], &[], &[]);
        assert_eq!((r.reapplied, r.dropped, r.lines.len()), (0, 0, 0));
        // Whitespace and wrapping do not make an edit.
        assert!(user_edits(
            &[l(0.0, 2.0, "one two three")],
            &[l(0.0, 2.0, "one two\nthree")]
        )
        .is_empty());
    }

    #[test]
    fn vietnamese_text_compares_in_nfc_and_case_counts() {
        let old = [l(0.0, 2.0, "chào bạn")];
        let nfd = "cha\u{0300}o ba\u{0323}n";
        assert!(user_edits(&old, &[l(0.0, 2.0, nfd)]).is_empty());
        assert_eq!(user_edits(&old, &[l(0.0, 2.0, "Chào bạn")]).len(), 1);
        // dê is not đê.
        assert_eq!(
            user_edits(&[l(0.0, 2.0, "con dê")], &[l(0.0, 2.0, "con đê")]).len(),
            1
        );
    }

    #[test]
    fn merged_split_and_added_lines_are_dropped_and_counted() {
        let old = [l(0.0, 2.0, "one two"), l(2.0, 4.0, "three four")];
        let new = old.clone();
        // Merged by hand: two old lines overlap the edit.
        let merged = [l(0.0, 4.0, "One two three four")];
        let r = reapply_edits(&old, &merged, &new);
        assert_eq!((r.reapplied, r.dropped), (0, 1));
        assert_eq!(r.lines, new.to_vec());
        // Split by hand: both halves want the same new line.
        let split = [l(0.0, 1.0, "One"), l(1.0, 2.0, "two!")];
        let r = reapply_edits(&old[..1], &split, &new[..1]);
        assert_eq!((r.reapplied, r.dropped), (0, 2));
        // Added by hand where nothing was said.
        let added = [l(0.0, 2.0, "one two"), l(5.0, 6.0, "[applause]")];
        let r = reapply_edits(&old[..1], &added, &new[..1]);
        assert_eq!((r.reapplied, r.dropped), (0, 1));
    }
}
