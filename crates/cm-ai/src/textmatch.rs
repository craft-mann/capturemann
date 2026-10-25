//! Text matching shared by caption find and replace (the app and the `replace_in_captions`
//! action) and by the glossary: Vietnamese-exact, display-free.
//!
//! Text is compared in NFC, so a search typed with precomposed letters finds text stored with
//! combining marks and the other way round (macOS input methods produce either). Nothing else is
//! folded away: `dê` never matches `đê`, and `e` never matches `ê`, because the comparison is per
//! character after composing. Without "match case" each character is lowercased (`Đ` finds `đ`).
//! A match starts and ends on a grapheme boundary, so a search for `a` does not stop between a
//! letter and a mark that could not be composed with it. With "whole word" the characters just
//! outside a match must not be letters or digits.
//!
//! Positions are byte ranges into the NFC text given to [`matches_in`].
//!
//! The caption row helpers ([`plain`], [`wrap_rows`], [`normalize`], [`editable_text`]) are here
//! too, so the editor and the daemon store a replaced line the same way.

use std::ops::Range;

use icu_normalizer::ComposingNormalizerBorrowed;
use unicode_segmentation::UnicodeSegmentation;

use crate::caption::{MAX_ROWS, MAX_ROW_CHARS};

/// `text` composed to NFC.
pub fn nfc(text: &str) -> String {
    ComposingNormalizerBorrowed::new_nfc()
        .normalize(text)
        .into_owned()
}

/// The characters of `s` as compared: lowercased unless `match_case`, each with the byte range of
/// the source character it came from.
fn folded(s: &str, match_case: bool) -> Vec<(char, Range<usize>)> {
    let mut out = Vec::with_capacity(s.len());
    for (b, c) in s.char_indices() {
        let r = b..b + c.len_utf8();
        if match_case {
            out.push((c, r));
        } else {
            for l in c.to_lowercase() {
                out.push((l, r.clone()));
            }
        }
    }
    out
}

/// `s` composed and, unless `match_case`, lowercased: the key two texts are equal by.
pub fn key(s: &str, match_case: bool) -> String {
    folded(&nfc(s), match_case)
        .into_iter()
        .map(|(c, _)| c)
        .collect()
}

/// Whether the grapheme just before `at` (or just after, with `before` false) starts with a
/// letter or digit.
fn word_char_next_to(text: &str, at: usize, before: bool) -> bool {
    let g = if before {
        text[..at].graphemes(true).next_back()
    } else {
        text[at..].graphemes(true).next()
    };
    g.and_then(|g| g.chars().next()).is_some_and(is_word_char)
}

/// The byte ranges of `query` in `text` (leftmost, not overlapping). `text` should be NFC; the
/// query is composed here. An empty query finds nothing.
pub fn matches_in_opts(
    text: &str,
    query: &str,
    match_case: bool,
    whole_word: bool,
) -> Vec<Range<usize>> {
    let query = nfc(query);
    let needle: Vec<char> = folded(&query, match_case)
        .into_iter()
        .map(|(c, _)| c)
        .collect();
    if needle.is_empty() {
        return Vec::new();
    }
    let hay = folded(text, match_case);
    let mut bounds: Vec<usize> = text.grapheme_indices(true).map(|(b, _)| b).collect();
    bounds.push(text.len());
    let on_boundary = |b: usize| bounds.binary_search(&b).is_ok();
    let mut out = Vec::new();
    let mut i = 0;
    while i + needle.len() <= hay.len() {
        let hit = hay[i..i + needle.len()]
            .iter()
            .zip(&needle)
            .all(|((a, _), b)| a == b);
        if hit {
            let (start, end) = (hay[i].1.start, hay[i + needle.len() - 1].1.end);
            // A lowercase expansion can split one source character: then only whole ones count.
            let whole = i == 0 || hay[i - 1].1 != hay[i].1;
            let whole_end = hay
                .get(i + needle.len())
                .is_none_or(|n| n.1 != hay[i + needle.len() - 1].1);
            let word_ok = !whole_word
                || (!word_char_next_to(text, start, true) && !word_char_next_to(text, end, false));
            if whole && whole_end && on_boundary(start) && on_boundary(end) && word_ok {
                out.push(start..end);
                i += needle.len();
                continue;
            }
        }
        i += 1;
    }
    out
}

/// [`matches_in_opts`] without the whole-word rule.
pub fn matches_in(text: &str, query: &str, match_case: bool) -> Vec<Range<usize>> {
    matches_in_opts(text, query, match_case, false)
}

/// `text` with the (sorted, non-overlapping) `ranges` replaced by `with`.
pub fn replaced(text: &str, ranges: &[Range<usize>], with: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for r in ranges {
        out.push_str(&text[at..r.start]);
        out.push_str(with);
        at = r.end;
    }
    out.push_str(&text[at..]);
    out
}

// ---- caption rows --------------------------------------------------------------------------

/// The text on one line.
pub fn plain(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn wrap_paragraph(text: &str) -> Vec<String> {
    let mut rows: Vec<String> = Vec::new();
    let mut cur = String::new();
    for w in text.split_whitespace() {
        if cur.is_empty() {
            cur.push_str(w);
        } else if cur.chars().count() + 1 + w.chars().count() <= MAX_ROW_CHARS {
            cur.push(' ');
            cur.push_str(w);
        } else {
            rows.push(std::mem::take(&mut cur));
            cur.push_str(w);
        }
    }
    if !cur.is_empty() {
        rows.push(cur);
    }
    rows
}

/// Greedy word wrap at 42 characters. A word longer than a row gets a row of its own. A line break
/// in `text` always ends a row (the user typed it), and each paragraph is wrapped on its own.
pub fn wrap_rows(text: &str) -> Vec<String> {
    text.split('\n').flat_map(wrap_paragraph).collect()
}

/// The text as stored: wrapped rows joined by `\n`.
pub fn normalize(text: &str) -> String {
    wrap_rows(text).join("\n")
}

/// What the in-place field starts with for stored text: the words on one flow (the field wraps
/// them to its width) unless the stored breaks are not just the automatic wrap, in which case the
/// user's own line breaks are kept.
pub fn editable_text(stored: &str) -> String {
    let flow = plain(stored);
    if normalize(&flow) == stored {
        flow
    } else {
        stored.to_owned()
    }
}

/// Whether `c` belongs inside a word: a letter, a digit or a combining mark (text that is not
/// composed yet, such as Vietnamese typed with combining accents, keeps its marks).
pub fn is_word_char(c: char) -> bool {
    c.is_alphanumeric()
        || matches!(c, '\u{0300}'..='\u{036F}' | '\u{1AB0}'..='\u{1AFF}' | '\u{1DC0}'..='\u{1DFF}'
            | '\u{20D0}'..='\u{20FF}' | '\u{FE20}'..='\u{FE2F}')
}

/// One find-and-replace over a caption line's stored text. Returns the new stored text and the
/// number of replacements, or `None` when nothing matched (the line is left exactly as it was).
/// The words are matched on one flow (a match may span the line's row break, which the automatic
/// split puts near the middle) in NFC, and a changed line is split into rows again like the lines
/// the transcript produces. Used by the `replace_in_captions` action; the editor's own find keeps
/// the user's typed breaks.
pub fn replace_in_line(
    stored: &str,
    find: &str,
    with: &str,
    match_case: bool,
    whole_word: bool,
) -> Option<(String, usize)> {
    let text = nfc(&plain(stored));
    let ranges = matches_in_opts(&text, find, match_case, whole_word);
    if ranges.is_empty() {
        return None;
    }
    let new = replaced(&text, &ranges, with);
    Some((crate::caption::rows_for(&new).join("\n"), ranges.len()))
}

/// Whether a stored line obeys the row rules (at most [`MAX_ROWS`] rows of [`MAX_ROW_CHARS`]).
pub fn rows_ok(stored: &str) -> bool {
    let rows: Vec<&str> = stored.split('\n').collect();
    rows.len() <= MAX_ROWS && rows.iter().all(|r| r.chars().count() <= MAX_ROW_CHARS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn de_never_matches_d_with_stroke() {
        assert!(matches_in("con đê này", "dê", false).is_empty());
        assert_eq!(matches_in("con dê này", "dê", false).len(), 1);
        assert_eq!(matches_in("ĐÊ", "đê", false).len(), 1);
    }

    #[test]
    fn nfc_and_nfd_are_equal() {
        let nfd = "Cha\u{0300}o";
        assert_eq!(matches_in(&nfc(nfd), "chào", false).len(), 1);
        assert_eq!(key(nfd, false), key("chào", false));
    }

    #[test]
    fn whole_word_needs_non_letters_around() {
        assert_eq!(
            matches_in_opts("a cat concat cat.", "cat", false, true).len(),
            2
        );
        assert_eq!(matches_in_opts("tiếp tiếpp", "tiếp", false, true).len(), 1);
        assert!(matches_in_opts("catế", "cat", false, true).is_empty());
    }

    #[test]
    fn replacing_in_a_line_rewraps_and_reports_no_match() {
        // The stored row break sits between the two words: the match still spans it.
        let (t, n) = replace_in_line(
            "Open the Logo\nDrive now and then again please",
            "logo drive",
            "Google Drive",
            false,
            false,
        )
        .unwrap();
        assert_eq!(n, 1);
        assert_eq!(t, "Open the Google Drive\nnow and then again please");
        assert!(replace_in_line("abc", "zzz", "x", false, false).is_none());
        assert!(rows_ok("a\nb"));
        assert!(!rows_ok("a\nb\nc"));
    }
}
