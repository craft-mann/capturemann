//! The caption editor's rules, display-free (AI-1): lines are at most 42 characters per row,
//! 2 rows, 1 to 7 s. The editor never rewrites what the user typed beyond wrapping it; a line that
//! breaks a rule is kept and flagged, so nothing the user wrote is silently cut.

use cm_ai::caption::CaptionLine;
use cm_core::MediaTime;
use cm_i18n::{fmt, tr, trn};

pub const MAX_CHARS: usize = 42;
pub const MAX_ROWS: usize = 2;
pub const MIN_SECS: f64 = 1.0;
pub const MAX_SECS: f64 = 7.0;
/// A line added at the playhead lasts this long (shortened to fit before the next line).
pub const NEW_LINE_SECS: f64 = 2.0;

// Wrapping and the editable flow live in `cm_ai::textmatch`, shared with the daemon's
// `replace_in_captions`, so a replaced line is stored the same way by the app and by agents.
pub use cm_ai::textmatch::{editable_text, normalize, plain, wrap_rows};

/// Where a caret sits in `target`, given where it sat in `raw` (the field's text, which may have
/// extra spaces or its own line breaks that [`normalize`] changed): the index (in chars) after the
/// same number of non-space characters. Both texts hold the same words, so this lands on the
/// same place. A caret past the end gives the length of `target`.
pub fn caret_char_index(raw: &str, caret_byte: usize, target: &str) -> usize {
    let mut cut = caret_byte.min(raw.len());
    while !raw.is_char_boundary(cut) {
        cut -= 1;
    }
    let want = raw[..cut].chars().filter(|c| !c.is_whitespace()).count();
    if want == 0 {
        return 0;
    }
    let mut seen = 0;
    for (i, c) in target.chars().enumerate() {
        if !c.is_whitespace() {
            seen += 1;
            if seen == want {
                return i + 1;
            }
        }
    }
    target.chars().count()
}

/// The byte offset of the `char_index`-th char of `text`, moved back to the start of the grapheme
/// cluster it falls in (never between a letter and its accent, nor inside an emoji).
fn grapheme_floor(text: &str, char_index: usize) -> usize {
    use unicode_segmentation::UnicodeSegmentation;
    let byte = text
        .char_indices()
        .nth(char_index)
        .map_or(text.len(), |(b, _)| b);
    if byte >= text.len() {
        return text.len();
    }
    text.grapheme_indices(true)
        .map(|(b, _)| b)
        .take_while(|&b| b <= byte)
        .last()
        .unwrap_or(0)
}

/// The two trimmed halves of `text` split at the `char_index`-th char (moved back to a grapheme
/// boundary), `None` when either half would be empty (the caret is at an end).
fn caret_halves(text: &str, char_index: usize) -> Option<(&str, &str)> {
    let at = grapheme_floor(text, char_index);
    let (a, b) = (text[..at].trim(), text[at..].trim());
    (!a.is_empty() && !b.is_empty()).then_some((a, b))
}

/// The previous (`forward == false`) or next line of `count`, `None` at either end.
pub fn neighbor(i: usize, count: usize, forward: bool) -> Option<usize> {
    if forward {
        (i + 1 < count).then_some(i + 1)
    } else {
        i.checked_sub(1).filter(|_| i < count)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum LineIssue {
    Empty,
    TooManyRows(usize),
    /// Row number (1-based) and its length.
    RowTooLong(usize, usize),
    TooShort(f64),
    TooLong(f64),
    OverlapsPrevious,
}

impl LineIssue {
    pub fn message(&self) -> String {
        match self {
            LineIssue::Empty => tr!("This caption is empty"),
            LineIssue::TooManyRows(n) => {
                tr!("{n} rows (the limit is {limit})", n = n, limit = MAX_ROWS)
            }
            LineIssue::RowTooLong(r, n) => {
                trn!(
                    "Row {row} has {n} character (the limit is {limit})",
                    "Row {row} has {n} characters (the limit is {limit})",
                    *n,
                    row = r,
                    limit = MAX_CHARS
                )
            }
            LineIssue::TooShort(s) => tr!(
                "Shows for {secs} s (at least {limit} s)",
                secs = fmt::decimal(*s, 1),
                limit = fmt::decimal(MIN_SECS, 0)
            ),
            LineIssue::TooLong(s) => tr!(
                "Shows for {secs} s (at most {limit} s)",
                secs = fmt::decimal(*s, 1),
                limit = fmt::decimal(MAX_SECS, 0)
            ),
            LineIssue::OverlapsPrevious => tr!("Starts before the previous caption ends"),
        }
    }
}

/// Why a new caption cannot be added at the playhead. Worded when drawn, so a note still on
/// screen follows a language switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddAtError {
    /// The playhead is inside a caption.
    InsideCaption,
    /// Less than [`MIN_SECS`] before the next caption.
    NoRoom,
}

impl AddAtError {
    pub fn message(self) -> String {
        match self {
            AddAtError::InsideCaption => {
                tr!("The playhead is inside a caption: move it to a gap")
            }
            AddAtError::NoRoom => tr!("There is less than a second before the next caption"),
        }
    }
}

fn secs(a: MediaTime, b: MediaTime) -> f64 {
    (b - a).as_secs_f64()
}

/// Every rule the line breaks. `prev_end` is the end of the line before it.
pub fn issues(line: &CaptionLine, prev_end: Option<MediaTime>) -> Vec<LineIssue> {
    let mut out = Vec::new();
    let rows = wrap_rows(&line.text);
    if rows.is_empty() {
        out.push(LineIssue::Empty);
    }
    if rows.len() > MAX_ROWS {
        out.push(LineIssue::TooManyRows(rows.len()));
    }
    for (i, r) in rows.iter().enumerate() {
        let n = r.chars().count();
        if n > MAX_CHARS {
            out.push(LineIssue::RowTooLong(i + 1, n));
        }
    }
    let d = secs(line.start, line.end);
    if d < MIN_SECS {
        out.push(LineIssue::TooShort(d));
    } else if d > MAX_SECS {
        out.push(LineIssue::TooLong(d));
    }
    if prev_end.is_some_and(|p| line.start < p) {
        out.push(LineIssue::OverlapsPrevious);
    }
    out
}

/// The caption list being edited.
#[derive(Debug, Clone, Default)]
pub struct CaptionEditor {
    lines: Vec<CaptionLine>,
    pub selected: Option<usize>,
    dirty: bool,
}

impl CaptionEditor {
    pub fn new(mut lines: Vec<CaptionLine>) -> CaptionEditor {
        lines.sort_by_key(|l| l.start);
        CaptionEditor {
            lines,
            selected: None,
            dirty: false,
        }
    }

    pub fn lines(&self) -> &[CaptionLine] {
        &self.lines
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Edited since the last save (`set_captions`).
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn mark_saved(&mut self) {
        self.dirty = false;
    }

    /// The issues of line `i` (against the line before it).
    pub fn issues_of(&self, i: usize) -> Vec<LineIssue> {
        let prev = i
            .checked_sub(1)
            .and_then(|p| self.lines.get(p))
            .map(|l| l.end);
        self.lines
            .get(i)
            .map(|l| issues(l, prev))
            .unwrap_or_default()
    }

    /// How many lines break a rule.
    pub fn problem_count(&self) -> usize {
        (0..self.lines.len())
            .filter(|&i| !self.issues_of(i).is_empty())
            .count()
    }

    pub fn select(&mut self, i: Option<usize>) {
        self.selected = i.filter(|&i| i < self.lines.len());
    }

    /// The seek target of clicking line `i`: its start.
    pub fn seek_target(&self, i: usize) -> Option<MediaTime> {
        self.lines.get(i).map(|l| l.start)
    }

    /// The line on screen at `t` (the playhead), for highlighting.
    pub fn line_at(&self, t: MediaTime) -> Option<usize> {
        self.lines.iter().position(|l| l.start <= t && t < l.end)
    }

    pub fn set_text(&mut self, i: usize, text: &str) -> bool {
        let Some(l) = self.lines.get_mut(i) else {
            return false;
        };
        let n = normalize(text);
        if l.text == n {
            return false;
        }
        l.text = n;
        self.dirty = true;
        true
    }

    pub fn remove(&mut self, i: usize) -> bool {
        if i >= self.lines.len() {
            return false;
        }
        self.lines.remove(i);
        self.selected = match self.selected {
            Some(s) if s == i => None,
            Some(s) if s > i => Some(s - 1),
            s => s,
        };
        self.dirty = true;
        true
    }

    /// Joins line `i` and the next: one line from the start of the first to the end of the second.
    pub fn merge_with_next(&mut self, i: usize) -> bool {
        if i + 1 >= self.lines.len() {
            return false;
        }
        let next = self.lines.remove(i + 1);
        let l = &mut self.lines[i];
        l.text = normalize(&format!("{} {}", plain(&l.text), plain(&next.text)));
        l.end = next.end;
        self.selected = Some(i);
        self.dirty = true;
        true
    }

    /// Splits line `i` at the middle word; the time is split in proportion to the characters.
    /// A line with fewer than two words cannot be split.
    pub fn split(&mut self, i: usize) -> bool {
        let Some(l) = self.lines.get(i) else {
            return false;
        };
        let words: Vec<&str> = l.text.split_whitespace().collect();
        if words.len() < 2 {
            return false;
        }
        let k = words.len() / 2;
        let a = words[..k].join(" ");
        let b = words[k..].join(" ");
        let (ca, cb) = (a.chars().count() as f64, b.chars().count() as f64);
        let span = (l.end - l.start).ticks() as f64;
        let mid = l.start + MediaTime::from_ticks((span * ca / (ca + cb)).round() as i64);
        let first = CaptionLine {
            start: l.start,
            end: mid,
            text: normalize(&a),
        };
        let second = CaptionLine {
            start: mid,
            end: l.end,
            text: normalize(&b),
        };
        self.lines[i] = first;
        self.lines.insert(i + 1, second);
        self.selected = Some(i);
        self.dirty = true;
        true
    }

    /// Whether [`split_at`](Self::split_at) would split at the caret (both halves hold text)
    /// rather than fall back to the middle word.
    pub fn splits_at_caret(&self, i: usize, char_index: usize) -> bool {
        self.lines
            .get(i)
            .is_some_and(|l| caret_halves(&editable_text(&l.text), char_index).is_some())
    }

    /// Splits line `i` at the text cursor: `char_index` counts chars of
    /// [`editable_text`] of the line (see [`caret_char_index`]) and is moved back to a grapheme
    /// boundary. Both halves are trimmed and must hold text; with the caret at the start or the
    /// end the middle-word [`split`](Self::split) is used instead. The time is divided in
    /// proportion to the characters, as in `split`.
    pub fn split_at(&mut self, i: usize, char_index: usize) -> bool {
        let Some(l) = self.lines.get(i) else {
            return false;
        };
        let text = editable_text(&l.text);
        let Some((a, b)) = caret_halves(&text, char_index) else {
            return self.split(i);
        };
        let (ca, cb) = (a.chars().count() as f64, b.chars().count() as f64);
        let span = (l.end - l.start).ticks() as f64;
        let mid = l.start + MediaTime::from_ticks((span * ca / (ca + cb)).round() as i64);
        let first = CaptionLine {
            start: l.start,
            end: mid,
            text: normalize(a),
        };
        let second = CaptionLine {
            start: mid,
            end: l.end,
            text: normalize(b),
        };
        self.lines[i] = first;
        self.lines.insert(i + 1, second);
        self.selected = Some(i);
        self.dirty = true;
        true
    }

    /// A new empty line at `t` (the playhead). It lasts [`NEW_LINE_SECS`], shortened to end where
    /// the next line starts. `Err` says why there is no room.
    pub fn add_at(&mut self, t: MediaTime) -> Result<usize, AddAtError> {
        if self.lines.iter().any(|l| l.start <= t && t < l.end) {
            return Err(AddAtError::InsideCaption);
        }
        let at = self
            .lines
            .iter()
            .position(|l| l.start > t)
            .unwrap_or(self.lines.len());
        let want =
            t + MediaTime::from_ticks((NEW_LINE_SECS * cm_core::TICKS_PER_SECOND as f64) as i64);
        let end = match self.lines.get(at) {
            Some(n) => want.min(n.start),
            None => want,
        };
        if secs(t, end) < MIN_SECS {
            return Err(AddAtError::NoRoom);
        }
        self.lines.insert(
            at,
            CaptionLine {
                start: t,
                end,
                text: String::new(),
            },
        );
        self.selected = Some(at);
        self.dirty = true;
        Ok(at)
    }
}

/// A pause this long between two words starts a new caption.
const PAUSE_BREAK_SECS: f64 = 1.0;
/// A caption is closed once it is this long, so it stays under [`MAX_SECS`].
const SOFT_MAX_SECS: f64 = 6.0;

/// Caption lines from recognised words, for a recording whose captions were never edited: a new
/// line starts at a pause, at the end of a sentence once the line has shown for a second, when a
/// third row would be needed, or when the line would outlast [`SOFT_MAX_SECS`]. (The daemon's
/// `caption` module builds the stored lines the same way; this is the fallback when only the
/// words are available.)
pub fn lines_from_words(words: &[cm_ai::transcribe::Word]) -> Vec<CaptionLine> {
    let mut out: Vec<CaptionLine> = Vec::new();
    let mut text = String::new();
    let mut start = MediaTime::ZERO;
    let mut end = MediaTime::ZERO;
    let flush =
        |text: &mut String, start: MediaTime, end: MediaTime, out: &mut Vec<CaptionLine>| {
            if !text.trim().is_empty() {
                out.push(CaptionLine {
                    start,
                    end,
                    text: normalize(text),
                });
            }
            text.clear();
        };
    for w in words {
        let word = w.text.trim();
        if word.is_empty() {
            continue;
        }
        if !text.is_empty() {
            let gap = secs(end, w.start);
            let joined = format!("{text} {word}");
            let too_many_rows = wrap_rows(&joined).len() > MAX_ROWS;
            let too_long = secs(start, w.end) > SOFT_MAX_SECS;
            let sentence_over = text.ends_with(['.', '?', '!']) && secs(start, end) >= MIN_SECS;
            if gap >= PAUSE_BREAK_SECS || too_many_rows || too_long || sentence_over {
                flush(&mut text, start, end, &mut out);
            }
        }
        if text.is_empty() {
            start = w.start;
            text.push_str(word);
        } else {
            text.push(' ');
            text.push_str(word);
        }
        end = w.end;
    }
    flush(&mut text, start, end, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use cm_ai::transcribe::Word;

    fn w(text: &str, a: f64, b: f64) -> Word {
        Word {
            text: text.into(),
            start: t(a),
            end: t(b),
            confidence: 0.9,
        }
    }

    #[test]
    fn words_become_lines_at_pauses_and_sentence_ends() {
        let words = [
            w("Hello", 0.0, 0.4),
            w("there.", 0.5, 1.2),
            w("This", 1.3, 1.5),
            w("is", 1.5, 1.6),
            w("a", 1.6, 1.7),
            w("test", 1.7, 2.0),
            // a 1.5 s pause starts a new caption
            w("Next", 3.5, 3.8),
            w("one", 3.8, 4.1),
        ];
        let lines = lines_from_words(&words);
        assert_eq!(
            lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(),
            vec!["Hello there.", "This is a test", "Next one"]
        );
        assert_eq!(lines[0].start, t(0.0));
        assert_eq!(lines[0].end, t(1.2));
        assert_eq!(lines[2].start, t(3.5));
    }

    #[test]
    fn long_speech_never_needs_a_third_row_or_outlasts_seven_seconds() {
        let words: Vec<Word> = (0..60)
            .map(|i| w("wordy", f64::from(i) * 0.4, f64::from(i) * 0.4 + 0.35))
            .collect();
        let lines = lines_from_words(&words);
        assert!(lines.len() > 2);
        for l in &lines {
            assert!(wrap_rows(&l.text).len() <= MAX_ROWS);
            assert!(secs(l.start, l.end) <= MAX_SECS);
        }
        // Every word is kept, in order.
        let joined: Vec<&str> = lines
            .iter()
            .flat_map(|l| l.text.split_whitespace())
            .collect();
        assert_eq!(joined.len(), 60);
        assert!(lines_from_words(&[]).is_empty());
        assert!(lines_from_words(&[w("  ", 0.0, 1.0)]).is_empty());
    }

    fn t(s: f64) -> MediaTime {
        MediaTime::from_secs_f64(s).unwrap()
    }

    fn line(a: f64, b: f64, text: &str) -> CaptionLine {
        CaptionLine {
            start: t(a),
            end: t(b),
            text: text.into(),
        }
    }

    #[test]
    fn wrapping_is_greedy_and_keeps_long_words_whole() {
        assert_eq!(wrap_rows("hello world"), vec!["hello world"]);
        let text = "the quick brown fox jumps over the lazy dog again and again";
        let rows = wrap_rows(text);
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|r| r.chars().count() <= MAX_CHARS));
        assert_eq!(plain(&rows.join(" ")), plain(text));
        let long = "x".repeat(50);
        assert_eq!(
            wrap_rows(&format!("a {long} b")),
            vec!["a".to_owned(), long, "b".into()]
        );
        assert!(wrap_rows("   ").is_empty());
    }

    /// AI-1: at most 42 characters per row, 2 rows, 1 to 7 s.
    #[test]
    fn rules_flag_rows_length_and_duration() {
        assert!(issues(&line(0.0, 2.0, "Fine"), None).is_empty());
        assert_eq!(issues(&line(0.0, 2.0, ""), None), vec![LineIssue::Empty]);
        let three = "one two three four five six seven eight nine ten eleven twelve thirteen \
                     fourteen fifteen sixteen seventeen eighteen";
        assert!(matches!(
            issues(&line(0.0, 3.0, three), None)[0],
            LineIssue::TooManyRows(3)
        ));
        let long = "y".repeat(45);
        assert_eq!(
            issues(&line(0.0, 3.0, &long), None),
            vec![LineIssue::RowTooLong(1, 45)]
        );
        assert!(matches!(
            issues(&line(0.0, 0.5, "Short"), None)[0],
            LineIssue::TooShort(_)
        ));
        assert!(matches!(
            issues(&line(0.0, 8.0, "Long"), None)[0],
            LineIssue::TooLong(_)
        ));
        assert_eq!(
            issues(&line(1.0, 3.0, "Overlap"), Some(t(2.0))),
            vec![LineIssue::OverlapsPrevious]
        );
        assert_eq!(
            LineIssue::TooManyRows(3).message(),
            "3 rows (the limit is 2)"
        );
        assert_eq!(
            LineIssue::RowTooLong(2, 50).message(),
            "Row 2 has 50 characters (the limit is 42)"
        );
    }

    #[test]
    fn editing_text_wraps_and_marks_dirty() {
        let mut e = CaptionEditor::new(vec![line(0.0, 2.0, "Hello")]);
        assert!(!e.is_dirty());
        assert!(!e.set_text(0, "Hello"), "no change, not dirty");
        assert!(!e.is_dirty());
        assert!(e.set_text(0, "  Hello   there  "));
        assert_eq!(e.lines()[0].text, "Hello there");
        assert!(e.is_dirty());
        e.mark_saved();
        assert!(!e.is_dirty());
    }

    #[test]
    fn clicking_a_line_seeks_to_its_start_and_the_playhead_finds_the_line() {
        let e = CaptionEditor::new(vec![line(1.0, 3.0, "a"), line(4.0, 6.0, "b")]);
        assert_eq!(e.seek_target(1), Some(t(4.0)));
        assert_eq!(e.seek_target(5), None);
        assert_eq!(e.line_at(t(1.5)), Some(0));
        assert_eq!(e.line_at(t(3.5)), None);
        assert_eq!(e.line_at(t(4.0)), Some(1));
    }

    #[test]
    fn remove_keeps_the_selection_pointing_at_the_same_line() {
        let mut e = CaptionEditor::new(vec![
            line(0.0, 2.0, "a"),
            line(3.0, 5.0, "b"),
            line(6.0, 8.0, "c"),
        ]);
        e.select(Some(2));
        assert!(e.remove(0));
        assert_eq!(e.selected, Some(1));
        assert_eq!(e.lines()[1].text, "c");
        assert!(e.remove(1));
        assert_eq!(e.selected, None);
        assert!(!e.remove(9));
    }

    #[test]
    fn merge_joins_text_and_spans_both_times() {
        let mut e = CaptionEditor::new(vec![line(0.0, 2.0, "Hello"), line(2.5, 4.0, "world")]);
        assert!(e.merge_with_next(0));
        assert_eq!(e.lines().len(), 1);
        assert_eq!(e.lines()[0].text, "Hello world");
        assert_eq!(e.lines()[0].start, t(0.0));
        assert_eq!(e.lines()[0].end, t(4.0));
        assert!(
            !e.merge_with_next(0),
            "the last line has nothing to merge with"
        );
    }

    #[test]
    fn split_divides_by_words_and_by_characters() {
        let mut e = CaptionEditor::new(vec![line(0.0, 4.0, "aaaa bbbb")]);
        assert!(e.split(0));
        assert_eq!(e.lines().len(), 2);
        assert_eq!(e.lines()[0].text, "aaaa");
        assert_eq!(e.lines()[1].text, "bbbb");
        assert_eq!(e.lines()[0].end, e.lines()[1].start);
        assert_eq!(e.lines()[0].end, t(2.0), "equal characters, equal time");
        assert_eq!(e.lines()[1].end, t(4.0));
        let mut one = CaptionEditor::new(vec![line(0.0, 2.0, "word")]);
        assert!(!one.split(0), "one word cannot be split");
    }

    #[test]
    fn add_at_the_playhead_needs_a_gap_and_a_second() {
        let mut e = CaptionEditor::new(vec![line(0.0, 2.0, "a"), line(5.0, 7.0, "b")]);
        assert_eq!(
            e.add_at(t(1.0)),
            Err(AddAtError::InsideCaption),
            "inside a caption"
        );
        assert_eq!(
            AddAtError::InsideCaption.message(),
            "The playhead is inside a caption: move it to a gap"
        );
        // A gap of 3 s: the new line lasts the default 2 s.
        let i = e.add_at(t(2.5)).unwrap();
        assert_eq!(i, 1);
        assert_eq!(e.lines()[1].end, t(4.5));
        assert_eq!(e.selected, Some(1));
        // Only 0.4 s before the next caption: refused.
        assert_eq!(e.add_at(t(4.6)), Err(AddAtError::NoRoom));
        assert_eq!(
            AddAtError::NoRoom.message(),
            "There is less than a second before the next caption"
        );
        // After the last line there is always room.
        let last = e.add_at(t(8.0)).unwrap();
        assert_eq!(last, 3);
        assert!(e.is_dirty());
    }

    #[test]
    fn problems_are_counted_per_line() {
        let e = CaptionEditor::new(vec![
            line(0.0, 2.0, "Fine"),
            line(2.0, 2.4, "Too short"),
            line(3.0, 5.0, ""),
        ]);
        assert_eq!(e.problem_count(), 2);
        assert!(e.issues_of(0).is_empty());
    }

    /// The row-length message is a plural message (I18N P2).
    #[test]
    fn the_row_issue_counts_characters_with_a_plural_message() {
        assert_eq!(
            LineIssue::RowTooLong(1, 1).message(),
            "Row 1 has 1 character (the limit is 42)"
        );
        assert_eq!(
            LineIssue::RowTooLong(2, 50).message(),
            "Row 2 has 50 characters (the limit is 42)"
        );
        cm_i18n::with_language(cm_i18n::Lang::Vi, || {
            let m = LineIssue::RowTooLong(2, 50).message();
            assert!(
                m.contains('2') && m.contains("50") && m.contains("42"),
                "{m}"
            );
        });
    }

    #[test]
    fn typed_line_breaks_survive_and_count_as_rows() {
        assert_eq!(normalize("one two\nthree"), "one two\nthree");
        assert_eq!(normalize("one\n\n  \nthree"), "one\nthree");
        // The automatic wrap is not kept as the user's own breaks.
        let long = "the quick brown fox jumps over the lazy dog again and again";
        let stored = normalize(long);
        assert!(stored.contains('\n'));
        assert_eq!(editable_text(&stored), long);
        // A deliberate break stays in the field text.
        assert_eq!(editable_text("short\nrows"), "short\nrows");
        // Three typed rows break the 2-row rule.
        let l = line(0.0, 3.0, &normalize("a\nb\nc"));
        assert!(matches!(issues(&l, None)[0], LineIssue::TooManyRows(3)));
    }

    #[test]
    fn the_caret_maps_across_spacing_changes() {
        // Raw has doubled spaces; the stored text has single ones.
        let raw = "ab  cd   ef";
        let stored = normalize(raw);
        assert_eq!(stored, "ab cd ef");
        // Caret after "cd" in raw (byte 6) is after "cd" in stored (char 5).
        assert_eq!(caret_char_index(raw, 6, &stored), 5);
        assert_eq!(caret_char_index(raw, 0, &stored), 0);
        assert_eq!(caret_char_index(raw, 99, &stored), 8);
    }

    #[test]
    fn splitting_at_the_caret_divides_text_and_time_by_characters() {
        let mut e = CaptionEditor::new(vec![line(0.0, 4.0, "abcd efghij")]);
        // After "abcd": the halves are 4 and 6 characters.
        assert!(e.split_at(0, 4));
        let l = e.lines();
        assert_eq!((l[0].text.as_str(), l[1].text.as_str()), ("abcd", "efghij"));
        assert_eq!(l[0].end, l[1].start);
        assert!((l[0].end.as_secs_f64() - 1.6).abs() < 1e-3);
        assert_eq!(l[1].end, t(4.0));
        assert!(e.is_dirty());
        assert_eq!(e.selected, Some(0));
    }

    #[test]
    fn splitting_inside_a_word_is_allowed_and_trims_the_halves() {
        let mut e = CaptionEditor::new(vec![line(0.0, 2.0, "hello world")]);
        assert!(e.split_at(0, 3));
        assert_eq!(e.lines()[0].text, "hel");
        assert_eq!(e.lines()[1].text, "lo world");
        let mut e = CaptionEditor::new(vec![line(0.0, 2.0, "hello world")]);
        assert!(e.split_at(0, 6), "the space goes to neither half");
        assert_eq!(e.lines()[0].text, "hello");
        assert_eq!(e.lines()[1].text, "world");
    }

    #[test]
    fn a_caret_at_either_end_falls_back_to_the_middle_word() {
        for at in [0, 1000] {
            let mut e = CaptionEditor::new(vec![line(0.0, 4.0, "one two three four")]);
            assert!(e.split_at(0, at));
            assert_eq!(e.lines()[0].text, "one two");
            assert_eq!(e.lines()[1].text, "three four");
        }
        // One word and the caret at the end: nothing to split, as before.
        let mut e = CaptionEditor::new(vec![line(0.0, 4.0, "single")]);
        assert!(!e.split_at(0, 6));
        assert!(!e.is_dirty());
        assert!(!e.split_at(5, 1));
    }

    #[test]
    fn splitting_vietnamese_works_in_nfc_and_nfd_and_never_cuts_a_letter() {
        let nfc = "Chào các bạn, hôm nay";
        let nfd = "Cha\u{0300}o ca\u{0301}c ba\u{0323}n, ho\u{0302}m nay";
        // After "Chào" (4 chars in NFC, 5 scalars in NFD).
        let mut a = CaptionEditor::new(vec![line(0.0, 4.0, nfc)]);
        assert!(a.split_at(0, 4));
        assert_eq!(a.lines()[0].text, "Chào");
        assert_eq!(a.lines()[1].text, "các bạn, hôm nay");
        let mut b = CaptionEditor::new(vec![line(0.0, 4.0, nfd)]);
        assert!(b.split_at(0, 5));
        assert_eq!(b.lines()[0].text, "Cha\u{0300}o");
        // A caret between the base letter and its accent moves back to before the letter.
        let mut c = CaptionEditor::new(vec![line(0.0, 4.0, nfd)]);
        assert!(c.split_at(0, 3));
        assert_eq!(c.lines()[0].text, "Ch");
        assert_eq!(
            c.lines()[1].text,
            "a\u{0300}o ca\u{0301}c ba\u{0323}n, ho\u{0302}m nay"
        );
        // Marks stacked on one letter (a, circumflex, acute) stay whole.
        let mut d = CaptionEditor::new(vec![line(0.0, 4.0, "xa\u{0302}\u{0301}y")]);
        assert!(d.split_at(0, 3));
        assert_eq!(d.lines()[0].text, "x");
    }

    #[test]
    fn splitting_keeps_emoji_whole_and_works_across_a_typed_break() {
        let mut e = CaptionEditor::new(vec![line(0.0, 4.0, "hi \u{1F44D}\u{1F3FD} there")]);
        // Index 4 is between the thumb and its skin-tone modifier.
        assert!(e.split_at(0, 4));
        assert_eq!(e.lines()[0].text, "hi");
        assert_eq!(e.lines()[1].text, "\u{1F44D}\u{1F3FD} there");
        let mut m = CaptionEditor::new(vec![line(0.0, 4.0, "first row\nsecond row")]);
        assert!(m.split_at(0, 10));
        assert_eq!(m.lines()[0].text, "first row");
        assert_eq!(m.lines()[1].text, "second row");
    }

    #[test]
    fn the_ui_can_tell_a_caret_split_from_the_fallback() {
        let e = CaptionEditor::new(vec![line(0.0, 4.0, "one two three")]);
        assert!(e.splits_at_caret(0, 4));
        assert!(!e.splits_at_caret(0, 0));
        assert!(!e.splits_at_caret(0, 13));
        assert!(!e.splits_at_caret(3, 1));
    }

    #[test]
    fn a_split_and_the_undo_of_nothing_keep_the_rules_per_half() {
        // Each half is wrapped on its own, so a long line splits into two valid ones.
        let long = "the quick brown fox jumps over the lazy dog again and again";
        let mut e = CaptionEditor::new(vec![line(0.0, 6.0, &normalize(long))]);
        assert!(!e.issues_of(0).is_empty() || e.lines()[0].text.contains('\n'));
        assert!(e.split_at(0, 30));
        assert!(e
            .lines()
            .iter()
            .all(|l| wrap_rows(&l.text).len() <= MAX_ROWS));
        assert_eq!(
            plain(&format!("{} {}", e.lines()[0].text, e.lines()[1].text)),
            plain(long)
        );
    }

    #[test]
    fn neighbours_stop_at_the_ends() {
        assert_eq!(neighbor(0, 3, true), Some(1));
        assert_eq!(neighbor(2, 3, true), None);
        assert_eq!(neighbor(0, 3, false), None);
        assert_eq!(neighbor(2, 3, false), Some(1));
        assert_eq!(neighbor(0, 0, true), None);
    }
}
