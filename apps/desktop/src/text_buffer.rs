//! Text editing state for the one-line fields (LIB-1 search box, the setup fields) and the
//! multi-line description field (PUB-2). The caret, backspace and delete move by grapheme
//! cluster (a letter with combining marks, an emoji sequence); offsets are bytes inside and
//! UTF-16 units at the input-method border (the OS counts text in UTF-16). A buffer holds line
//! breaks only when they are inserted with [`TextBuffer::insert_multiline`] (or a `multiline`
//! input-method call); the one-line [`TextBuffer::insert`] drops them.
//!
//! Input-method composition (I18N S2): the text being composed ("marked" text, shown underlined)
//! is part of the buffer, so `text()` includes it; [`TextBuffer::replace_range_utf16`] commits,
//! [`TextBuffer::mark_range_utf16`] replaces the marked text with a new draft,
//! [`TextBuffer::cancel_composition`] drops it. Every other edit or caret move first commits it
//! (the OS would have asked for that through `unmarkText`).

use std::ops::Range;

use unicode_segmentation::{GraphemeCursor, UnicodeSegmentation};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TextBuffer {
    text: String,
    /// Byte offset of the caret, always on a char boundary.
    cursor: usize,
    /// "Select all" is the only selection (Cmd/Ctrl+A); the next edit replaces everything.
    all_selected: bool,
    /// The column (in chars) vertical moves aim for, so a short line does not pull the caret
    /// left for good. Cleared by every other move or edit.
    goal_col: Option<usize>,
    /// The byte range being composed by an input method (never empty), on char boundaries.
    marked: Option<Range<usize>>,
}

/// Line breaks as `\n`, tabs as four spaces, other control characters dropped.
fn clean_multiline(s: &str) -> String {
    let unified = s.replace("\r\n", "\n").replace('\r', "\n");
    let mut out = String::with_capacity(unified.len());
    for c in unified.chars() {
        match c {
            '\n' => out.push('\n'),
            '\t' => out.push_str("    "),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

impl TextBuffer {
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn all_selected(&self) -> bool {
        self.all_selected
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// The selected text: the whole text after "select all", otherwise nothing (a field has no
    /// partial selection). What Copy and Cut put on the clipboard.
    pub fn selected_text(&self) -> Option<&str> {
        (self.all_selected && !self.text.is_empty()).then_some(self.text.as_str())
    }

    /// Text before and after the caret.
    pub fn split(&self) -> (&str, &str) {
        self.text.split_at(self.cursor)
    }

    /// Inserts typed or pasted text; control characters and newlines are dropped.
    /// Returns whether the text changed.
    pub fn insert(&mut self, s: &str) -> bool {
        let clean: String = s.chars().filter(|c| !c.is_control()).collect();
        self.put(&clean)
    }

    /// Inserts typed or pasted text and keeps its line breaks (`\r\n` and `\r` become `\n`,
    /// tabs become four spaces, other control characters are dropped).
    pub fn insert_multiline(&mut self, s: &str) -> bool {
        self.put(&clean_multiline(s))
    }

    fn put(&mut self, clean: &str) -> bool {
        if clean.is_empty() {
            return false;
        }
        self.goal_col = None;
        self.marked = None;
        if self.all_selected {
            self.text.clear();
            self.cursor = 0;
            self.all_selected = false;
        }
        self.text.insert_str(self.cursor, clean);
        self.cursor += clean.len();
        true
    }

    /// Deletes the grapheme cluster before the caret (the selection when everything is selected).
    pub fn backspace(&mut self) -> bool {
        self.goal_col = None;
        self.marked = None;
        if self.all_selected {
            return self.clear();
        }
        let start = self.prev_boundary(self.cursor);
        if start == self.cursor {
            return false;
        }
        self.text.replace_range(start..self.cursor, "");
        self.cursor = start;
        true
    }

    /// Deletes the grapheme cluster after the caret.
    pub fn delete(&mut self) -> bool {
        self.goal_col = None;
        self.marked = None;
        if self.all_selected {
            return self.clear();
        }
        let end = self.next_boundary(self.cursor);
        if end == self.cursor {
            return false;
        }
        self.text.replace_range(self.cursor..end, "");
        true
    }

    /// The grapheme boundary before `at` (`at` itself at the start of the text).
    fn prev_boundary(&self, at: usize) -> usize {
        let mut gc = GraphemeCursor::new(at, self.text.len(), true);
        gc.prev_boundary(&self.text, 0).ok().flatten().unwrap_or(at)
    }

    /// The grapheme boundary after `at` (`at` itself at the end of the text).
    fn next_boundary(&self, at: usize) -> usize {
        let mut gc = GraphemeCursor::new(at, self.text.len(), true);
        gc.next_boundary(&self.text, 0).ok().flatten().unwrap_or(at)
    }

    pub fn clear(&mut self) -> bool {
        let changed = !self.text.is_empty();
        self.text.clear();
        self.cursor = 0;
        self.all_selected = false;
        self.goal_col = None;
        self.marked = None;
        changed
    }

    pub fn left(&mut self) {
        self.all_selected = false;
        self.goal_col = None;
        self.marked = None;
        self.cursor = self.prev_boundary(self.cursor);
    }

    pub fn right(&mut self) {
        self.all_selected = false;
        self.goal_col = None;
        self.marked = None;
        self.cursor = self.next_boundary(self.cursor);
    }

    pub fn home(&mut self) {
        self.all_selected = false;
        self.goal_col = None;
        self.marked = None;
        self.cursor = 0;
    }

    pub fn end(&mut self) {
        self.all_selected = false;
        self.goal_col = None;
        self.marked = None;
        self.cursor = self.text.len();
    }

    /// Moves the caret to `byte` (clamped to the text and to a grapheme boundary). Commits the
    /// text being composed.
    pub fn set_cursor(&mut self, byte: usize) {
        let mut at = byte.min(self.text.len());
        while !self.text.is_char_boundary(at) {
            at -= 1;
        }
        // Land on the start of a cluster (a click between a letter and its mark).
        let mut gc = GraphemeCursor::new(at, self.text.len(), true);
        if !gc.is_boundary(&self.text, 0).unwrap_or(true) {
            at = self.prev_boundary(at);
        }
        self.cursor = at;
        self.all_selected = false;
        self.goal_col = None;
        self.marked = None;
    }

    /// Start of the line the caret is on (after the previous line break).
    pub fn line_start(&self) -> usize {
        self.text[..self.cursor].rfind('\n').map_or(0, |i| i + 1)
    }

    /// End of the line the caret is on (before the next line break).
    pub fn line_end(&self) -> usize {
        self.text[self.cursor..]
            .find('\n')
            .map_or(self.text.len(), |i| self.cursor + i)
    }

    /// Home in a multi-line field: the start of this line.
    pub fn home_line(&mut self) {
        let at = self.line_start();
        self.set_cursor(at);
    }

    /// End in a multi-line field: the end of this line.
    pub fn end_line(&mut self) {
        let at = self.line_end();
        self.set_cursor(at);
    }

    /// Number of lines (a trailing line break starts an empty line).
    pub fn line_count(&self) -> usize {
        self.text.matches('\n').count() + 1
    }

    /// Up one line (by line breaks, not by wrapping), keeping the column. On the first line the
    /// caret goes to the start of the text.
    pub fn up(&mut self) {
        let col = self.goal_col.unwrap_or_else(|| self.column());
        let start = self.line_start();
        if start == 0 {
            self.set_cursor(0);
            return;
        }
        let target = {
            let prev_end = start - 1;
            let prev_start = self.text[..prev_end].rfind('\n').map_or(0, |i| i + 1);
            self.byte_at_col(prev_start, prev_end, col)
        };
        self.set_cursor(target);
        self.goal_col = Some(col);
    }

    /// Down one line, keeping the column. On the last line the caret goes to the end.
    pub fn down(&mut self) {
        let col = self.goal_col.unwrap_or_else(|| self.column());
        let end = self.line_end();
        if end >= self.text.len() {
            self.set_cursor(self.text.len());
            return;
        }
        let target = {
            let next_start = end + 1;
            let next_end = self.text[next_start..]
                .find('\n')
                .map_or(self.text.len(), |i| next_start + i);
            self.byte_at_col(next_start, next_end, col)
        };
        self.set_cursor(target);
        self.goal_col = Some(col);
    }

    /// Puts the caret `col` chars into the first (`last_line == false`) or last line of the text
    /// (at the end of that line when it is shorter): how the caption list keeps the caret's place
    /// when it moves to the next or previous line.
    pub fn set_cursor_column(&mut self, last_line: bool, col: usize) {
        let (start, end) = if last_line {
            let start = self.text.rfind('\n').map_or(0, |i| i + 1);
            (start, self.text.len())
        } else {
            (0, self.text.find('\n').unwrap_or(self.text.len()))
        };
        let at = self.byte_at_col(start, end, col);
        self.set_cursor(at);
    }

    /// The caret's column in chars from the start of its line.
    pub fn column(&self) -> usize {
        self.text[self.line_start()..self.cursor].chars().count()
    }

    fn byte_at_col(&self, start: usize, end: usize, col: usize) -> usize {
        self.text[start..end]
            .char_indices()
            .nth(col)
            .map_or(end, |(i, _)| start + i)
    }

    pub fn select_all(&mut self) {
        self.goal_col = None;
        self.marked = None;
        self.all_selected = !self.text.is_empty();
        self.cursor = self.text.len();
    }
}

/// Input-method composition. The OS addresses text in UTF-16 units.
impl TextBuffer {
    /// The text being composed, in bytes.
    pub fn marked_range(&self) -> Option<Range<usize>> {
        self.marked.clone()
    }

    pub fn is_composing(&self) -> bool {
        self.marked.is_some()
    }

    /// The text being composed, in UTF-16 units.
    pub fn marked_range_utf16(&self) -> Option<Range<usize>> {
        self.marked.as_ref().map(|m| self.range_to_utf16(m))
    }

    /// Length of the text in UTF-16 units.
    pub fn len_utf16(&self) -> usize {
        self.text.encode_utf16().count()
    }

    pub fn byte_to_utf16(&self, byte: usize) -> usize {
        let mut at = byte.min(self.text.len());
        while !self.text.is_char_boundary(at) {
            at -= 1;
        }
        self.text[..at].encode_utf16().count()
    }

    pub fn range_to_utf16(&self, r: &Range<usize>) -> Range<usize> {
        self.byte_to_utf16(r.start)..self.byte_to_utf16(r.end)
    }

    /// The byte offset of a UTF-16 offset. An offset inside a surrogate pair rounds to the start
    /// of the char (`round_up`: to its end); an offset past the end is the end.
    pub fn utf16_to_byte(&self, utf16: usize, round_up: bool) -> usize {
        let mut units = 0;
        for (i, c) in self.text.char_indices() {
            if units >= utf16 {
                return i;
            }
            units += c.len_utf16();
            if units > utf16 {
                return if round_up { i + c.len_utf8() } else { i };
            }
        }
        self.text.len()
    }

    /// A UTF-16 range as bytes (start rounded down, end rounded up, ordered).
    pub fn range_from_utf16(&self, r: &Range<usize>) -> Range<usize> {
        let (a, b) = (r.start.min(r.end), r.start.max(r.end));
        self.utf16_to_byte(a, false)..self.utf16_to_byte(b, true)
    }

    /// The selection as UTF-16 offsets: everything when all is selected, else the caret.
    pub fn selection_utf16(&self) -> Range<usize> {
        if self.all_selected {
            0..self.len_utf16()
        } else {
            let c = self.byte_to_utf16(self.cursor);
            c..c
        }
    }

    /// The text of a UTF-16 range, and the range actually used (clamped to the text).
    pub fn text_for_range_utf16(&self, r: &Range<usize>) -> (String, Range<usize>) {
        let bytes = self.range_from_utf16(r);
        (
            self.text[bytes.clone()].to_owned(),
            self.range_to_utf16(&bytes),
        )
    }

    /// What an input-method call without a range means: the text being composed, else the
    /// selection, else the caret.
    fn target_range(&self, range: Option<&Range<usize>>) -> Range<usize> {
        match range {
            Some(r) => self.range_from_utf16(r),
            None => match &self.marked {
                Some(m) => m.clone(),
                None if self.all_selected => 0..self.text.len(),
                None => self.cursor..self.cursor,
            },
        }
    }

    fn clean_for(s: &str, multiline: bool) -> String {
        if multiline {
            clean_multiline(s)
        } else {
            s.chars().filter(|c| !c.is_control()).collect()
        }
    }

    /// `insertText:replacementRange:`: replaces `range` (default: see `target_range`) with `text`
    /// and ends the composition, with the caret after the text. Returns whether the text changed.
    pub fn replace_range_utf16(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        multiline: bool,
    ) -> bool {
        let clean = Self::clean_for(text, multiline);
        let target = self.target_range(range.as_ref());
        let changed = self.text[target.clone()] != clean;
        self.text.replace_range(target.clone(), &clean);
        self.cursor = target.start + clean.len();
        self.marked = None;
        self.all_selected = false;
        self.goal_col = None;
        changed
    }

    /// `setMarkedText:selectedRange:replacementRange:`: replaces `range` (default: the text being
    /// composed, else the selection) with the draft `text` and marks it. `selected` is the caret
    /// or selection inside the draft in UTF-16 units; the buffer keeps only its end as the caret.
    /// An empty draft ends the composition. Returns whether the text changed.
    pub fn mark_range_utf16(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        multiline: bool,
    ) -> bool {
        let clean = Self::clean_for(text, multiline);
        let target = self.target_range(range.as_ref());
        let changed = self.text[target.clone()] != clean;
        self.text.replace_range(target.clone(), &clean);
        self.all_selected = false;
        self.goal_col = None;
        if clean.is_empty() {
            self.marked = None;
            self.cursor = target.start;
            return changed;
        }
        self.marked = Some(target.start..target.start + clean.len());
        let inside = selected.map_or(clean.len(), |s| {
            let units = s.start.max(s.end);
            let mut n = 0;
            let mut at = clean.len();
            for (i, c) in clean.char_indices() {
                if n >= units {
                    at = i;
                    break;
                }
                n += c.len_utf16();
            }
            at
        });
        self.cursor = target.start + inside;
        changed
    }

    /// `unmarkText`: the draft becomes ordinary text.
    pub fn unmark(&mut self) {
        self.marked = None;
    }

    /// Escape during a composition that the input method did not take: removes the draft.
    /// Returns whether the text changed.
    pub fn cancel_composition(&mut self) -> bool {
        match self.marked.take() {
            Some(m) => {
                self.text.replace_range(m.clone(), "");
                self.cursor = m.start;
                self.goal_col = None;
                true
            }
            None => false,
        }
    }

    /// `setSelectedRange`: a range that covers the whole text selects all; any other puts the
    /// caret at its end (partial selections do not exist in this field). The text being composed
    /// stays marked.
    pub fn set_selection_utf16(&mut self, r: &Range<usize>) {
        let bytes = self.range_from_utf16(r);
        if self.marked.is_some() {
            self.cursor = bytes.end;
        } else if bytes.start == 0 && bytes.end == self.text.len() && !self.text.is_empty() {
            self.select_all();
        } else {
            self.set_cursor(bytes.end);
        }
    }

    /// Grapheme clusters in the text (what the user sees as characters).
    pub fn grapheme_count(&self) -> usize {
        self.text.graphemes(true).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_caret_can_be_put_in_a_column_of_the_first_or_last_line() {
        let mut b = TextBuffer::default();
        b.insert_multiline("first line\nsecond\nthird one");
        b.set_cursor_column(false, 4);
        assert_eq!(b.split().0, "firs");
        b.set_cursor_column(true, 5);
        assert_eq!(b.split().0, "first line\nsecond\nthird");
        // Shorter than the column: the end of the line.
        b.set_cursor_column(false, 99);
        assert_eq!(b.split().0, "first line");
        b.set_cursor_column(true, 99);
        assert_eq!(b.split().1, "");
        // One line: first and last are the same.
        let mut c = TextBuffer::default();
        c.insert("Chào bạn");
        c.set_cursor_column(true, 4);
        assert_eq!(c.split().0, "Chào");
    }

    #[test]
    fn only_a_select_all_has_text_to_copy() {
        let mut b = TextBuffer::default();
        b.insert("Việt Nam");
        assert_eq!(b.selected_text(), None);
        b.select_all();
        assert_eq!(b.selected_text(), Some("Việt Nam"));
        b.left();
        assert_eq!(b.selected_text(), None);
        let mut e = TextBuffer::default();
        e.select_all();
        assert_eq!(e.selected_text(), None);
    }

    #[test]
    fn typing_and_caret_movement() {
        let mut b = TextBuffer::default();
        assert!(b.insert("readness"));
        for _ in 0..4 {
            b.left();
        }
        assert_eq!(b.split(), ("read", "ness"));
        b.insert("i");
        assert_eq!(b.text(), "readiness");
        b.end();
        assert_eq!(b.split().1, "");
        b.home();
        assert_eq!(b.split().0, "");
    }

    #[test]
    fn multibyte_characters_keep_boundaries() {
        let mut b = TextBuffer::default();
        b.insert("a\u{00E9}\u{4E2D}\u{1F600}");
        assert!(b.backspace());
        assert_eq!(b.text(), "a\u{00E9}\u{4E2D}");
        b.left();
        assert_eq!(b.split().0, "a\u{00E9}");
        assert!(b.delete());
        assert_eq!(b.text(), "a\u{00E9}");
        b.home();
        assert!(!b.backspace());
        assert!(b.delete());
        assert_eq!(b.text(), "\u{00E9}");
    }

    #[test]
    fn select_all_then_type_replaces() {
        let mut b = TextBuffer::default();
        b.insert("old");
        b.select_all();
        assert!(b.all_selected());
        b.insert("new");
        assert_eq!(b.text(), "new");
        b.select_all();
        assert!(b.backspace());
        assert!(b.is_empty());
        b.select_all();
        assert!(!b.all_selected(), "nothing to select in an empty field");
    }

    #[test]
    fn control_characters_and_newlines_are_dropped() {
        let mut b = TextBuffer::default();
        assert!(!b.insert("\n\t"));
        assert!(b.insert("a\nb"));
        assert_eq!(b.text(), "ab");
    }

    #[test]
    fn multiline_insert_keeps_line_breaks_and_normalizes_them() {
        let mut b = TextBuffer::default();
        assert!(b.insert_multiline("one\r\ntwo\rthree\n\tfour\u{7}"));
        assert_eq!(b.text(), "one\ntwo\nthree\n    four");
        assert_eq!(b.line_count(), 4);
        assert!(!b.insert_multiline("\u{7}"));
        // The one-line insert still drops them.
        let mut s = TextBuffer::default();
        s.insert("a\r\nb");
        assert_eq!(s.text(), "ab");
    }

    #[test]
    fn enter_in_the_middle_splits_the_line_and_backspace_joins_it_again() {
        let mut b = TextBuffer::default();
        b.insert_multiline("abcd");
        b.left();
        b.left();
        b.insert_multiline("\n");
        assert_eq!(b.text(), "ab\ncd");
        assert_eq!(b.split(), ("ab\n", "cd"));
        assert!(b.backspace());
        assert_eq!(b.text(), "abcd");
        assert_eq!(b.split(), ("ab", "cd"));
    }

    #[test]
    fn home_and_end_work_on_the_current_line() {
        let mut b = TextBuffer::default();
        b.insert_multiline("first\nsecond\nthird");
        b.up();
        assert_eq!(b.line_start(), 6);
        assert_eq!(b.line_end(), 12);
        b.home_line();
        assert_eq!(b.split().0, "first\n");
        b.end_line();
        assert_eq!(b.split().0, "first\nsecond");
        b.set_cursor(0);
        b.end_line();
        assert_eq!(b.split().0, "first");
    }

    #[test]
    fn up_and_down_keep_the_column_across_short_lines() {
        let mut b = TextBuffer::default();
        b.insert_multiline("abcdef\nxy\nghijkl");
        // The caret is at column 6 of the last line.
        assert_eq!(b.column(), 6);
        b.up();
        assert_eq!(b.split().0, "abcdef\nxy", "clamped to the short line");
        b.up();
        assert_eq!(b.split().0, "abcdef", "the goal column 6 comes back");
        b.up();
        assert_eq!(b.cursor(), 0, "the first line goes to the start");
        b.down();
        assert_eq!(b.column(), 0);
        b.set_cursor(b.text().len());
        b.down();
        assert_eq!(b.cursor(), b.text().len(), "the last line goes to the end");
    }

    #[test]
    fn vertical_moves_count_chars_not_bytes() {
        let mut b = TextBuffer::default();
        b.insert_multiline("\u{e9}\u{4e2d}\u{1f600}x\nabcd");
        b.set_cursor(0);
        b.right();
        b.right();
        assert_eq!(b.column(), 2);
        b.down();
        assert_eq!(b.split().0, "\u{e9}\u{4e2d}\u{1f600}x\nab");
        b.up();
        assert_eq!(b.split().0, "\u{e9}\u{4e2d}");
    }

    #[test]
    fn typing_or_moving_sideways_forgets_the_goal_column() {
        let mut b = TextBuffer::default();
        b.insert_multiline("abcdef\nxy\nghijkl");
        b.up();
        b.insert_multiline("!");
        b.up();
        assert_eq!(b.column(), 3, "column 3 after `xy!`");
        let mut c = TextBuffer::default();
        c.insert_multiline("abcdef\nxy\nghijkl");
        c.up();
        c.left();
        c.up();
        assert_eq!(c.column(), 1);
    }

    #[test]
    fn set_cursor_clamps_and_lands_on_a_char_boundary() {
        let mut b = TextBuffer::default();
        b.insert("a\u{4e2d}b");
        b.set_cursor(2);
        assert_eq!(b.split().0, "a");
        b.set_cursor(99);
        assert_eq!(b.split().1, "");
        b.select_all();
        b.set_cursor(1);
        assert!(!b.all_selected());
    }

    #[test]
    fn select_all_then_enter_replaces_everything_with_a_line_break() {
        let mut b = TextBuffer::default();
        b.insert_multiline("a\nb");
        b.select_all();
        b.insert_multiline("\n");
        assert_eq!(b.text(), "\n");
        assert_eq!(b.line_count(), 2);
    }

    // ---- grapheme clusters (I18N S2) ----------------------------------------------------------

    /// Vietnamese "ệ" decomposed: e + dot below + circumflex.
    const E_NFD: &str = "e\u{323}\u{302}";

    #[test]
    fn backspace_removes_a_decomposed_vietnamese_letter_as_one_unit() {
        let mut b = TextBuffer::default();
        b.insert(&format!("Vi{E_NFD}t"));
        b.left();
        assert!(b.backspace());
        assert_eq!(b.text(), "Vit", "e and both marks go together");
        assert_eq!(b.split(), ("Vi", "t"));
    }

    #[test]
    fn precomposed_letters_delete_as_one_unit_too() {
        let mut b = TextBuffer::default();
        b.insert("Vi\u{1ec7}t");
        b.left();
        assert!(b.backspace());
        assert_eq!(b.text(), "Vit");
    }

    #[test]
    fn delete_and_arrows_move_over_a_whole_cluster() {
        let mut b = TextBuffer::default();
        b.insert(&format!("a{E_NFD}b"));
        b.home();
        b.right();
        b.right();
        assert_eq!(b.split().0, format!("a{E_NFD}"));
        b.left();
        assert_eq!(b.split(), ("a", &format!("{E_NFD}b")[..]));
        assert!(b.delete());
        assert_eq!(b.text(), "ab");
        b.end();
        assert!(!b.delete());
        b.home();
        assert!(!b.backspace());
    }

    #[test]
    fn emoji_sequences_are_one_cluster() {
        let family = "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}";
        let flag = "\u{1f1fb}\u{1f1f3}";
        let thumbs = "\u{1f44d}\u{1f3fd}";
        let mut b = TextBuffer::default();
        b.insert(&format!("x{family}{flag}{thumbs}"));
        assert_eq!(b.grapheme_count(), 4);
        assert!(b.backspace());
        assert_eq!(b.text(), format!("x{family}{flag}"));
        assert!(b.backspace());
        assert_eq!(b.text(), format!("x{family}"));
        b.left();
        assert_eq!(b.split().0, "x");
        assert!(b.delete());
        assert_eq!(b.text(), "x");
    }

    #[test]
    fn set_cursor_inside_a_cluster_lands_on_its_start() {
        let mut b = TextBuffer::default();
        b.insert(&format!("a{E_NFD}b"));
        b.set_cursor(2);
        assert_eq!(b.cursor(), 1, "between e and its dot");
        b.set_cursor(1 + E_NFD.len());
        assert_eq!(b.cursor(), 1 + E_NFD.len(), "after the cluster is fine");
    }

    // ---- UTF-16 offsets ------------------------------------------------------------------------

    #[test]
    fn utf16_offsets_count_surrogate_pairs_as_two() {
        let mut b = TextBuffer::default();
        b.insert("a\u{1f600}\u{1ec7}b");
        assert_eq!(b.len_utf16(), 5);
        assert_eq!(b.byte_to_utf16(0), 0);
        assert_eq!(b.byte_to_utf16(1), 1);
        assert_eq!(b.byte_to_utf16(5), 3);
        assert_eq!(b.byte_to_utf16(b.text().len()), 5);
        assert_eq!(b.utf16_to_byte(1, false), 1);
        assert_eq!(b.utf16_to_byte(3, false), 5);
        assert_eq!(b.utf16_to_byte(99, false), b.text().len());
        // Inside the pair: down to its start, up to its end.
        assert_eq!(b.utf16_to_byte(2, false), 1);
        assert_eq!(b.utf16_to_byte(2, true), 5);
        assert_eq!(b.range_from_utf16(&(2..2)), 1..5);
        #[allow(clippy::reversed_empty_ranges)]
        let reversed = 3..1;
        assert_eq!(
            b.range_from_utf16(&reversed),
            1..5,
            "reversed ranges are ordered"
        );
    }

    #[test]
    fn text_for_range_clamps_to_the_text() {
        let mut b = TextBuffer::default();
        b.insert(&format!("a{E_NFD}b"));
        let (t, used) = b.text_for_range_utf16(&(1..99));
        assert_eq!(t, format!("{E_NFD}b"));
        assert_eq!(used, 1..5);
        assert_eq!(b.selection_utf16(), 5..5);
        b.select_all();
        assert_eq!(b.selection_utf16(), 0..5);
    }

    // ---- composition ---------------------------------------------------------------------------

    /// What macOS does for Telex "as": setMarkedText("a"), setMarkedText("\u{e1}"), then a space
    /// commits with insertText("\u{e1} ").
    #[test]
    fn telex_a_then_s_makes_an_acute_a_and_a_space_commits() {
        let mut b = TextBuffer::default();
        assert!(b.mark_range_utf16(None, "a", Some(1..1), false));
        assert_eq!(b.text(), "a");
        assert_eq!(b.marked_range(), Some(0..1));
        assert!(b.mark_range_utf16(None, "\u{e1}", Some(1..1), false));
        assert_eq!(b.text(), "\u{e1}");
        assert_eq!(
            b.marked_range(),
            Some(0..2),
            "the draft replaced the old one"
        );
        assert!(b.replace_range_utf16(None, "\u{e1} ", false));
        assert_eq!(b.text(), "\u{e1} ");
        assert!(!b.is_composing());
        assert_eq!(b.cursor(), 3);
    }

    #[test]
    fn telex_dd_makes_d_with_stroke() {
        let mut b = TextBuffer::default();
        b.mark_range_utf16(None, "d", Some(1..1), false);
        b.mark_range_utf16(None, "\u{111}", Some(1..1), false);
        assert_eq!(b.text(), "\u{111}");
        b.unmark();
        assert_eq!(b.text(), "\u{111}");
        assert!(!b.is_composing());
    }

    #[test]
    fn telex_uow_makes_the_horn_pair() {
        let mut b = TextBuffer::default();
        for draft in ["u", "uo", "u\u{1a1}", "\u{1b0}\u{1a1}"] {
            b.mark_range_utf16(None, draft, None, false);
        }
        assert_eq!(b.text(), "\u{1b0}\u{1a1}");
        assert_eq!(b.marked_range_utf16(), Some(0..2));
        b.replace_range_utf16(None, "\u{1b0}\u{1a1}", false);
        assert_eq!(b.text(), "\u{1b0}\u{1a1}");
        assert_eq!(b.cursor(), 4);
    }

    #[test]
    fn composing_after_committed_text_marks_only_the_draft() {
        // V i e e t j -> "Việt": the marks of the last syllable are redrawn each key.
        let mut b = TextBuffer::default();
        b.insert("Vi");
        for draft in ["e", "\u{ea}", "\u{ea}t", "\u{1ec7}t"] {
            b.mark_range_utf16(None, draft, None, false);
        }
        assert_eq!(b.text(), "Vi\u{1ec7}t");
        assert_eq!(b.marked_range(), Some(2..6));
        assert_eq!(b.marked_range_utf16(), Some(2..4));
        b.replace_range_utf16(None, "\u{1ec7}t ", false);
        assert_eq!(b.text(), "Vi\u{1ec7}t ");
        assert_eq!(b.cursor(), b.text().len());
    }

    #[test]
    fn the_os_may_pass_the_marked_range_explicitly() {
        let mut b = TextBuffer::default();
        b.insert("ab");
        b.mark_range_utf16(None, "c", None, false);
        // setMarkedText with a replacement range equal to the draft (UTF-16 2..3).
        b.mark_range_utf16(Some(2..3), "\u{e7}", None, false);
        assert_eq!(b.text(), "ab\u{e7}");
        assert_eq!(b.marked_range(), Some(2..4));
        b.replace_range_utf16(Some(2..3), "\u{e7}", false);
        assert_eq!(b.text(), "ab\u{e7}");
        assert!(!b.is_composing());
    }

    #[test]
    fn a_replacement_range_in_committed_text_replaces_it() {
        // Press-and-hold accents and "reconversion" replace text that is not marked.
        let mut b = TextBuffer::default();
        b.insert("hello");
        assert!(b.replace_range_utf16(Some(1..2), "\u{e9}", false));
        assert_eq!(b.text(), "h\u{e9}llo");
        assert_eq!(b.cursor(), 3);
    }

    #[test]
    fn escape_cancels_the_draft_and_keeps_the_rest() {
        let mut b = TextBuffer::default();
        b.insert("Vi");
        b.mark_range_utf16(None, "\u{ea}", None, false);
        assert!(b.cancel_composition());
        assert_eq!(b.text(), "Vi");
        assert_eq!(b.cursor(), 2);
        assert!(!b.is_composing());
        assert!(!b.cancel_composition(), "nothing to cancel");
    }

    #[test]
    fn an_empty_draft_ends_the_composition() {
        // Backspace on a one-letter draft: setMarkedText("").
        let mut b = TextBuffer::default();
        b.insert("x");
        b.mark_range_utf16(None, "a", None, false);
        assert!(b.mark_range_utf16(None, "", None, false));
        assert_eq!(b.text(), "x");
        assert!(!b.is_composing());
        assert_eq!(b.cursor(), 1);
    }

    #[test]
    fn composing_over_a_select_all_replaces_everything() {
        let mut b = TextBuffer::default();
        b.insert("old title");
        b.select_all();
        b.mark_range_utf16(None, "n", None, false);
        assert_eq!(b.text(), "n");
        assert!(!b.all_selected());
        assert_eq!(b.marked_range(), Some(0..1));
        b.mark_range_utf16(None, "\u{1b0}", None, false);
        b.replace_range_utf16(None, "\u{1b0}", false);
        assert_eq!(b.text(), "\u{1b0}");
    }

    #[test]
    fn the_caret_can_sit_inside_the_draft() {
        // Japanese clause navigation: setMarkedText("abc", selectedRange 1..1).
        let mut b = TextBuffer::default();
        b.mark_range_utf16(None, "abc", Some(1..1), false);
        assert_eq!(b.cursor(), 1);
        assert_eq!(b.selection_utf16(), 1..1);
        // A selection inside the draft keeps its end as the caret.
        b.mark_range_utf16(None, "abc", Some(0..2), false);
        assert_eq!(b.cursor(), 2);
        // Beyond the draft: the end.
        b.mark_range_utf16(None, "abc", Some(9..9), false);
        assert_eq!(b.cursor(), 3);
        // A surrogate pair counts two units.
        b.mark_range_utf16(None, "\u{1f600}x", Some(2..2), false);
        assert_eq!(b.cursor(), 4);
    }

    #[test]
    fn set_selection_keeps_the_draft_marked() {
        let mut b = TextBuffer::default();
        b.mark_range_utf16(None, "abc", None, false);
        b.set_selection_utf16(&(1..1));
        assert_eq!(b.cursor(), 1);
        assert_eq!(b.marked_range(), Some(0..3));
        b.unmark();
        b.set_selection_utf16(&(0..3));
        assert!(b.all_selected());
        b.set_selection_utf16(&(1..2));
        assert!(!b.all_selected());
        assert_eq!(b.cursor(), 2);
    }

    #[test]
    fn other_edits_and_moves_commit_the_draft() {
        for act in 0..6 {
            let mut b = TextBuffer::default();
            b.mark_range_utf16(None, "ab", None, false);
            match act {
                0 => b.left(),
                1 => b.right(),
                2 => b.end(),
                3 => b.select_all(),
                4 => b.set_cursor(1),
                _ => {
                    b.insert("!");
                }
            }
            assert!(!b.is_composing(), "action {act}");
            assert!(b.text().starts_with("ab") || b.text() == "!");
        }
        // A paste while composing lands after the committed draft.
        let mut b = TextBuffer::default();
        b.mark_range_utf16(None, "ab", None, false);
        b.insert("PASTE");
        assert_eq!(b.text(), "abPASTE");
        let mut c = TextBuffer::default();
        c.mark_range_utf16(None, "ab", None, false);
        assert!(c.backspace());
        assert_eq!(c.text(), "a", "the committed draft loses its last letter");
    }

    #[test]
    fn clean_text_follows_the_field_mode() {
        let mut one = TextBuffer::default();
        one.mark_range_utf16(None, "a\nb\t", None, false);
        assert_eq!(one.text(), "ab");
        assert_eq!(one.marked_range(), Some(0..2));
        let mut multi = TextBuffer::default();
        multi.replace_range_utf16(None, "a\r\nb\t", true);
        assert_eq!(multi.text(), "a\nb    ");
        // A draft of only control characters is empty.
        let mut c = TextBuffer::default();
        assert!(!c.mark_range_utf16(None, "\u{7}", None, false));
        assert!(!c.is_composing());
    }

    #[test]
    fn composing_on_the_second_line_of_a_multi_line_field() {
        let mut b = TextBuffer::default();
        b.insert_multiline("first line\nse");
        b.mark_range_utf16(None, "c", None, true);
        b.mark_range_utf16(None, "\u{1ed1}", None, true);
        assert_eq!(b.text(), "first line\nse\u{1ed1}");
        assert_eq!(b.line_start(), 11);
        assert_eq!(b.marked_range_utf16(), Some(13..14));
        // A line break from the input method (Enter commits first, then breaks the line).
        b.replace_range_utf16(None, "\u{1ed1}", true);
        b.insert_multiline("\n");
        assert_eq!(b.line_count(), 3);
        // Composing at the start of a line in the middle of the text.
        b.set_cursor(11);
        b.mark_range_utf16(None, "x", None, true);
        assert_eq!(b.marked_range(), Some(11..12));
        assert_eq!(b.split().0, "first line\nx");
        let (t, used) = b.text_for_range_utf16(&(11..12));
        assert_eq!((t.as_str(), used), ("x", 11..12));
        assert!(b.cancel_composition());
        assert_eq!(b.text(), "first line\nse\u{1ed1}\n");
    }

    #[test]
    fn a_decomposed_draft_deletes_as_one_unit_after_commit() {
        let mut b = TextBuffer::default();
        b.mark_range_utf16(None, E_NFD, None, false);
        assert_eq!(b.marked_range_utf16(), Some(0..3));
        b.replace_range_utf16(None, E_NFD, false);
        assert!(b.backspace());
        assert!(b.is_empty());
    }
}
