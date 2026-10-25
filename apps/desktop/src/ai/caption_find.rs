//! Find and replace over the caption lines (R3c), display-free.
//!
//! The text of every line is compared in NFC, so a search typed with precomposed letters finds
//! text stored with combining marks and the other way round (macOS input methods produce either).
//! Nothing else is folded away: `dê` never matches `đê`, and `e` never matches `ê`, because the
//! comparison is per character after composing. Without "Match case" each character is lowercased
//! (`Đ` finds `đ`). A match starts and ends on a grapheme boundary, so a search for `a` does not
//! stop between a letter and a mark that could not be composed with it.
//!
//! Positions are byte ranges into [`line_text`], the NFC of the text the row shows.

use std::ops::Range;

use cm_ai::caption::CaptionLine;
pub use cm_ai::textmatch::{matches_in, nfc, replaced};

use super::captions::{editable_text, CaptionEditor};

/// The text the find works on for a line: what the row shows (see [`editable_text`]), in NFC.
pub fn line_text(l: &CaptionLine) -> String {
    nfc(&editable_text(&l.text))
}

/// One hit: the line and the byte range in [`line_text`] of that line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    pub line: usize,
    pub range: Range<usize>,
}

/// Every match in every line, in reading order.
pub fn search(lines: &[CaptionLine], query: &str, match_case: bool) -> Vec<Match> {
    lines
        .iter()
        .enumerate()
        .flat_map(|(i, l)| {
            matches_in(&line_text(l), query, match_case)
                .into_iter()
                .map(move |range| Match { line: i, range })
        })
        .collect()
}

impl CaptionEditor {
    /// Replaces one match. `false` when the line or the range no longer exists (the text changed
    /// since the matches were listed).
    pub fn replace_match(&mut self, m: &Match, with: &str) -> bool {
        let Some(l) = self.lines().get(m.line) else {
            return false;
        };
        let text = line_text(l);
        if m.range.end > text.len() || !text.is_char_boundary(m.range.start) {
            return false;
        }
        let new = replaced(&text, std::slice::from_ref(&m.range), with);
        self.set_text(m.line, &new)
    }

    /// Replaces every match in every line. The caller saves once afterwards, so the whole
    /// replacement is one undo step. Returns how many matches were replaced.
    pub fn replace_all(&mut self, query: &str, with: &str, match_case: bool) -> usize {
        let mut count = 0;
        for i in 0..self.lines().len() {
            let text = line_text(&self.lines()[i]);
            let ranges = matches_in(&text, query, match_case);
            if ranges.is_empty() {
                continue;
            }
            count += ranges.len();
            let new = replaced(&text, &ranges, with);
            self.set_text(i, &new);
        }
        count
    }
}

/// The find bar's state: what is typed and which match is current.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FindState {
    pub query: String,
    pub replacement: String,
    pub match_case: bool,
    /// Index into the matches (0-based).
    pub current: usize,
}

impl FindState {
    /// Keeps `current` inside `0..total` (a replaced match shifts the rest up).
    pub fn clamp(&mut self, total: usize) {
        if total == 0 || self.current >= total {
            self.current = if total == 0 { 0 } else { total - 1 };
        }
    }

    /// Moves to the next or previous match, wrapping around.
    pub fn step(&mut self, total: usize, forward: bool) {
        if total == 0 {
            self.current = 0;
        } else if forward {
            self.current = (self.current + 1) % total;
        } else {
            self.current = (self.current + total - 1) % total;
        }
    }

    /// The first match at or after `line` (so typing in the bar jumps to what is on screen).
    pub fn jump_to_line(&mut self, matches: &[Match], line: usize) {
        self.current = matches.iter().position(|m| m.line >= line).unwrap_or(0);
    }

    /// `(n, m)` for "{n} of {m}"; `n` is 0 when nothing matches.
    pub fn position(&self, total: usize) -> (usize, usize) {
        if total == 0 {
            (0, 0)
        } else {
            (self.current.min(total - 1) + 1, total)
        }
    }
}

/// The matches of one line, as ranges (for highlighting its row), with the current one flagged.
pub fn line_ranges(matches: &[Match], line: usize, current: usize) -> Vec<(Range<usize>, bool)> {
    matches
        .iter()
        .enumerate()
        .filter(|(_, m)| m.line == line)
        .map(|(k, m)| (m.range.clone(), k == current))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use cm_core::MediaTime;

    fn line(text: &str) -> CaptionLine {
        CaptionLine {
            start: MediaTime::ZERO,
            end: MediaTime::from_secs_f64(2.0).unwrap(),
            text: text.into(),
        }
    }

    fn editor(texts: &[&str]) -> CaptionEditor {
        CaptionEditor::new(
            texts
                .iter()
                .enumerate()
                .map(|(i, t)| CaptionLine {
                    start: MediaTime::from_secs_f64(i as f64 * 3.0).unwrap(),
                    end: MediaTime::from_secs_f64(i as f64 * 3.0 + 2.0).unwrap(),
                    text: (*t).into(),
                })
                .collect(),
        )
    }

    #[test]
    fn case_is_ignored_unless_asked() {
        let t = "Hello hello HELLO";
        assert_eq!(matches_in(t, "hello", false).len(), 3);
        assert_eq!(matches_in(t, "hello", true), vec![6..11]);
        assert!(matches_in(t, "", false).is_empty());
        assert!(matches_in("", "x", false).is_empty());
    }

    #[test]
    fn matches_do_not_overlap() {
        assert_eq!(matches_in("aaaa", "aa", false), vec![0..2, 2..4]);
    }

    #[test]
    fn a_search_for_de_never_matches_d_with_stroke() {
        // The letter đ is its own letter, not a d with a mark.
        assert!(matches_in("con đê này", "dê", false).is_empty());
        assert_eq!(matches_in("con dê này", "dê", false).len(), 1);
        assert!(matches_in("con dê này", "đê", false).is_empty());
        // And case folding keeps them apart too.
        assert_eq!(matches_in("ĐÊ", "đê", false).len(), 1);
        assert!(matches_in("ĐÊ", "dê", false).is_empty());
    }

    #[test]
    fn diacritics_are_not_folded_away() {
        assert!(matches_in("chào", "chao", false).is_empty());
        assert!(matches_in("chao", "chào", false).is_empty());
        assert!(matches_in("hạnh phúc", "hanh", false).is_empty());
        assert!(matches_in("ca", "cá", false).is_empty());
        // The tone mark decides: ba, bà, bá, bả, bã, bạ are six different words.
        let t = "ba bà bá bả bã bạ";
        for w in ["ba", "bà", "bá", "bả", "bã", "bạ"] {
            let found = matches_in(t, w, false);
            assert_eq!(found.len(), 1, "{w}");
        }
    }

    #[test]
    fn nfc_and_nfd_are_equal_both_ways() {
        let stored_nfd = "Cha\u{0300}o ba\u{0323}n";
        let hay = nfc(stored_nfd);
        assert_eq!(hay, "Chào bạn");
        // Query typed in NFC, text stored in NFD.
        assert_eq!(matches_in(&hay, "chào", false).len(), 1);
        // Query typed in NFD, text in NFC.
        assert_eq!(matches_in("Chào bạn", "ba\u{0323}n", false).len(), 1);
        let lines = [line(stored_nfd)];
        let found = search(&lines, "bạn", false);
        assert_eq!(found.len(), 1);
        // The range indexes the NFC text.
        assert_eq!(&line_text(&lines[0])[found[0].range.clone()], "bạn");
    }

    #[test]
    fn vietnamese_capitals_fold_with_case_off_and_not_with_it_on() {
        assert_eq!(matches_in("Ở góc phải", "ở", false).len(), 1);
        assert!(matches_in("Ở góc phải", "ở", true).is_empty());
        assert_eq!(matches_in("Ở góc phải", "Ở", true).len(), 1);
    }

    #[test]
    fn a_match_never_ends_between_a_letter_and_its_mark() {
        // `e` + a mark with no precomposed form: searching `e` must not cut the cluster.
        let t = "e\u{0301}\u{0328}x";
        assert!(matches_in(&nfc(t), "e", false).is_empty());
        assert_eq!(matches_in("ex", "e", false), vec![0..1]);
    }

    #[test]
    fn emoji_are_found_whole() {
        let t = "ok \u{1F44D}\u{1F3FD} done";
        assert_eq!(matches_in(t, "\u{1F44D}\u{1F3FD}", false).len(), 1);
        assert!(matches_in(t, "\u{1F44D}", false).is_empty());
    }

    #[test]
    fn search_lists_matches_in_reading_order() {
        let e = editor(&["one two", "two three two", "four"]);
        let m = search(e.lines(), "two", false);
        let at: Vec<(usize, usize)> = m.iter().map(|m| (m.line, m.range.start)).collect();
        assert_eq!(at, vec![(0, 4), (1, 0), (1, 10)]);
    }

    #[test]
    fn replacing_one_match_changes_only_that_one() {
        let mut e = editor(&["two and two", "two"]);
        let m = search(e.lines(), "two", false);
        assert!(e.replace_match(&m[1], "2"));
        assert_eq!(e.lines()[0].text, "two and 2");
        assert_eq!(e.lines()[1].text, "two");
        assert!(e.is_dirty());
        // A match listed before an edit that removed its line is refused.
        let stale = Match {
            line: 9,
            range: 0..1,
        };
        assert!(!e.replace_match(&stale, "x"));
    }

    #[test]
    fn replace_all_is_one_change_over_every_line() {
        let mut e = editor(&["Tiếp tục", "tiếp theo", "xong"]);
        let n = e.replace_all("tiếp", "Tiếp", false);
        assert_eq!(n, 2);
        assert_eq!(e.lines()[0].text, "Tiếp tục");
        assert_eq!(e.lines()[1].text, "Tiếp theo");
        assert_eq!(e.lines()[2].text, "xong");
        assert!(e.is_dirty());
        // The replacement can contain the query without looping.
        let mut f = editor(&["a b a"]);
        assert_eq!(f.replace_all("a", "aa", false), 2);
        assert_eq!(f.lines()[0].text, "aa b aa");
        // Nothing found: nothing changes.
        let mut g = editor(&["abc"]);
        assert_eq!(g.replace_all("zzz", "x", false), 0);
        assert!(!g.is_dirty());
    }

    #[test]
    fn replace_all_composes_the_touched_lines_to_nfc_and_leaves_others_alone() {
        let mut e = editor(&["Cha\u{0300}o ba\u{0323}n", "ba\u{0323}n oi"]);
        assert_eq!(e.replace_all("chào", "Xin chào", false), 1);
        assert_eq!(e.lines()[0].text, "Xin chào bạn");
        // The second line had no match and is stored as it was.
        assert_eq!(e.lines()[1].text, "ba\u{0323}n oi");
    }

    #[test]
    fn replacing_wraps_and_keeps_the_row_rules() {
        let mut e = editor(&["x"]);
        let long = "a".repeat(30);
        e.replace_all("x", &format!("{long} {long}"), false);
        assert_eq!(e.lines()[0].text, format!("{long}\n{long}"));
        assert!(e.issues_of(0).is_empty(), "two rows of 30 characters, 2 s");
    }

    #[test]
    fn the_current_match_wraps_around_and_stays_in_range() {
        let mut f = FindState::default();
        f.step(3, true);
        assert_eq!(f.current, 1);
        f.step(3, true);
        f.step(3, true);
        assert_eq!(f.current, 0);
        f.step(3, false);
        assert_eq!(f.current, 2);
        f.step(0, true);
        assert_eq!(f.current, 0);
        f.current = 7;
        f.clamp(3);
        assert_eq!(f.current, 2);
        f.clamp(0);
        assert_eq!(f.current, 0);
        assert_eq!(f.position(0), (0, 0));
        f.current = 1;
        assert_eq!(f.position(3), (2, 3));
    }

    #[test]
    fn typing_jumps_to_the_first_match_at_or_after_a_line() {
        let e = editor(&["a", "b a", "a"]);
        let m = search(e.lines(), "a", false);
        let mut f = FindState::default();
        f.jump_to_line(&m, 1);
        assert_eq!(f.current, 1);
        f.jump_to_line(&m, 3);
        assert_eq!(f.current, 0);
    }

    #[test]
    fn line_ranges_flag_the_current_match() {
        let e = editor(&["a a", "a"]);
        let m = search(e.lines(), "a", false);
        let r = line_ranges(&m, 0, 1);
        assert_eq!(r, vec![(0..1, false), (2..3, true)]);
        assert!(line_ranges(&m, 5, 0).is_empty());
    }
}
