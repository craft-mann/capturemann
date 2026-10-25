//! Doubtful words in the caption editor (AI-6a, piece 2).
//!
//! Caption lines carry only text, while Whisper's confidence is per word in `transcript.json`.
//! [`doubtful_spans`] aligns each line's text with the transcript words that fall inside the line
//! in time, and reports the words below [`DOUBTFUL_BELOW`] as byte ranges into the text the row
//! shows ([`line_text`]: the editable flow of the line, NFC). A word the user has edited no longer
//! occurs in the line, so it loses its underline; the alignment is a forward walk that tolerates a
//! few edited words in between ([`LOOKAHEAD`] tokens).
//!
//! Display-free: the editor only draws the ranges and steps through them with [`step`].

use std::ops::Range;

use cm_core::MediaTime;

use crate::caption::CaptionLine;
use crate::textmatch::{editable_text, is_word_char, key, nfc};
use crate::transcribe::Word;

/// A word with a confidence below this is doubtful. Easy to tune: Whisper's token probabilities
/// are skewed high, so 0.6 flags roughly the least certain few percent of a clear recording.
pub const DOUBTFUL_BELOW: f32 = 0.6;
/// How many tokens ahead of the last aligned one a transcript word may be found: words the user
/// replaced or deleted are skipped, a few at a time.
pub const LOOKAHEAD: usize = 4;

/// One doubtful word shown in a line.
#[derive(Debug, Clone, PartialEq)]
pub struct Doubtful {
    /// Index of the caption line.
    pub line: usize,
    /// Byte range in [`line_text`] of that line (the word without edge punctuation).
    pub range: Range<usize>,
    /// Where the word starts in the recording, to seek the playhead.
    pub start: MediaTime,
    pub confidence: f32,
}

/// The text a row shows for a line, NFC; the ranges of [`Doubtful`] index into it.
pub fn line_text(l: &CaptionLine) -> String {
    nfc(&editable_text(&l.text))
}

/// The core of a token (letters and digits, with the inner punctuation kept) as a comparison key,
/// and its byte range inside the token.
fn core(token: &str) -> Option<(String, Range<usize>)> {
    let start = token.char_indices().find(|(_, c)| is_word_char(*c))?.0;
    let end = token
        .char_indices()
        .rev()
        .find(|(_, c)| is_word_char(*c))
        .map(|(i, c)| i + c.len_utf8())?;
    Some((key(&token[start..end], false), start..end))
}

/// Tokens of `text` (split at whitespace) with their core key and the core's byte range in `text`.
fn tokens(text: &str) -> Vec<(String, Range<usize>)> {
    let mut out = Vec::new();
    let mut at = 0;
    for piece in text.split_inclusive(char::is_whitespace) {
        let tok = piece.trim_end_matches(char::is_whitespace);
        if let Some((k, r)) = core(tok) {
            out.push((k, at + r.start..at + r.end));
        }
        at += piece.len();
    }
    out
}

/// All doubtful words of all lines, in reading order. `words` are the transcript's words (session
/// time, as stored), `lines` the caption lines as the editor holds them.
pub fn doubtful_spans(lines: &[CaptionLine], words: &[Word]) -> Vec<Doubtful> {
    let mut out = Vec::new();
    for (li, line) in lines.iter().enumerate() {
        for s in spans_in_text(&line_text(line), line.start, line.end, words) {
            out.push(Doubtful {
                line: li,
                range: s.range,
                start: s.start,
                confidence: s.confidence,
            });
        }
    }
    out
}

/// One doubtful word found in a text.
#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    pub range: Range<usize>,
    pub start: MediaTime,
    pub confidence: f32,
}

/// The doubtful words of one line whose text is `text` (any text: the stored line, or what the
/// in-place field holds right now, so the underline follows the typing) and whose time is
/// `from..to`. `words` must be in time order, as `transcript.json` stores them. The ranges are
/// byte ranges into `text` as given.
pub fn spans_in_text(text: &str, from: MediaTime, to: MediaTime, words: &[Word]) -> Vec<Span> {
    let toks = tokens(text);
    if toks.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut next = 0usize;
    // Words are in time order: skip those that ended before the line began.
    let first = words.partition_point(|w| w.end < from);
    for w in &words[first..] {
        if w.start > to {
            break;
        }
        let mid = MediaTime::from_ticks((w.start.ticks() + w.end.ticks()) / 2);
        if mid < from || mid > to {
            continue;
        }
        let Some((k, _)) = core(&nfc(&w.text)) else {
            continue;
        };
        let window = toks.len().min(next + LOOKAHEAD + 1);
        let Some(found) = (next..window).find(|&i| toks[i].0 == k) else {
            continue;
        };
        next = found + 1;
        if w.confidence < DOUBTFUL_BELOW {
            out.push(Span {
                range: toks[found].1.clone(),
                start: w.start,
                confidence: w.confidence,
            });
        }
    }
    out
}

/// The next (or previous) doubtful word from the caret at byte `caret` of line `line`, wrapping
/// around at the ends. Returns the index into `spans` (which [`doubtful_spans`] returns sorted).
/// Next is the first word that starts after the caret; previous is the last that starts before it.
/// With no caret (`None`: the line is selected or under the playhead but not being edited) the
/// position is the start of the line, and a word at the very start counts as next.
pub fn step(spans: &[Doubtful], line: usize, caret: Option<usize>, forward: bool) -> Option<usize> {
    if spans.is_empty() {
        return None;
    }
    let at = (line, caret.unwrap_or(0));
    let pos = |d: &Doubtful| (d.line, d.range.start);
    if forward {
        Some(
            spans
                .iter()
                .position(|d| pos(d) > at || (caret.is_none() && pos(d) == at))
                .unwrap_or(0),
        )
    } else {
        Some(
            spans
                .iter()
                .rposition(|d| pos(d) < at)
                .unwrap_or(spans.len() - 1),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcribe::media_secs;

    fn w(t: &str, s: f64, e: f64, c: f32) -> Word {
        Word {
            text: t.into(),
            start: media_secs(s),
            end: media_secs(e),
            confidence: c,
        }
    }

    fn line(s: f64, e: f64, t: &str) -> CaptionLine {
        CaptionLine {
            start: media_secs(s),
            end: media_secs(e),
            text: t.into(),
        }
    }

    fn words() -> Vec<Word> {
        vec![
            w("Open", 0.0, 0.4, 0.95),
            w("Logo", 0.4, 0.8, 0.31),
            w("Drive,", 0.8, 1.2, 0.9),
            w("then", 2.0, 2.3, 0.4),
            w("save", 2.3, 2.7, 0.9),
        ]
    }

    #[test]
    fn low_confidence_words_get_ranges_without_punctuation() {
        let lines = [
            line(0.0, 1.5, "Open Logo Drive,"),
            line(1.8, 3.0, "then save"),
        ];
        let d = doubtful_spans(&lines, &words());
        assert_eq!(d.len(), 2);
        assert_eq!((d[0].line, d[0].range.clone()), (0, 5..9));
        assert_eq!(&line_text(&lines[0])[d[0].range.clone()], "Logo");
        assert_eq!((d[1].line, d[1].range.clone()), (1, 0..4));
        assert_eq!(d[1].start, media_secs(2.0));
    }

    #[test]
    fn an_edited_word_loses_its_underline_and_the_others_keep_theirs() {
        let lines = [
            line(0.0, 1.5, "Open Google Drive,"),
            line(1.8, 3.0, "then save"),
        ];
        let d = doubtful_spans(&lines, &words());
        assert_eq!(d.len(), 1, "Logo was replaced: {d:?}");
        assert_eq!(d[0].line, 1);
        // Several words replaced by one still lets the later ones align.
        let lines = [line(0.0, 1.5, "Google Drive"), line(1.8, 3.0, "then save")];
        assert_eq!(doubtful_spans(&lines, &words()).len(), 1);
    }

    #[test]
    fn a_word_outside_the_line_in_time_is_not_used() {
        let lines = [line(0.0, 1.5, "then")];
        assert!(doubtful_spans(&lines, &words()).is_empty());
    }

    #[test]
    fn vietnamese_matches_in_nfc_and_nfd_but_de_is_not_d_with_stroke() {
        let ws = vec![w("đê", 0.0, 0.5, 0.2), w("chào", 0.5, 1.0, 0.3)];
        // The line is stored decomposed; the word is precomposed.
        let l = line(0.0, 2.0, "\u{0111}e\u{0302} cha\u{0300}o");
        let d = doubtful_spans(std::slice::from_ref(&l), &ws);
        assert_eq!(d.len(), 2);
        assert_eq!(&line_text(&l)[d[1].range.clone()], "chào");
        // dê is a different word from đê.
        let l = line(0.0, 2.0, "dê chào");
        let d = doubtful_spans(&[l], &ws);
        assert_eq!(d.len(), 1, "only chào aligns: {d:?}");
    }

    #[test]
    fn rows_wrapped_by_a_newline_still_align() {
        let l = line(0.0, 1.5, "Open Logo\nDrive,");
        assert_eq!(doubtful_spans(&[l], &words()).len(), 1);
    }

    #[test]
    fn stepping_goes_forward_and_back_and_wraps() {
        let lines = [
            line(0.0, 1.5, "Open Logo Drive,"),
            line(1.8, 3.0, "then save"),
        ];
        let d = doubtful_spans(&lines, &words());
        assert_eq!(step(&d, 0, Some(0), true), Some(0));
        assert_eq!(
            step(&d, 0, Some(5), true),
            Some(1),
            "from the start of Logo"
        );
        assert_eq!(step(&d, 1, Some(0), true), Some(0), "wraps");
        assert_eq!(step(&d, 1, Some(0), false), Some(0));
        assert_eq!(step(&d, 0, Some(0), false), Some(1), "wraps back");
        assert_eq!(step(&d, 0, Some(9), false), Some(0), "caret after Logo");
        assert_eq!(step(&[], 0, Some(0), true), None);
        // No caret: the line's own first word counts as next, and previous goes before the line.
        assert_eq!(step(&d, 1, None, true), Some(1));
        assert_eq!(step(&d, 1, None, false), Some(0));
        assert_eq!(step(&d, 0, None, true), Some(0));
    }
}
