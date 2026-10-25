//! AI-4 filler-word suggestions from a transcript.
//!
//! A word is suggested only when it is **isolated** (at least [`MIN_GAP`], 150 ms, of silence
//! before and after it, and the start or end of the transcript counts as a gap) and it **is** a
//! filler as a whole token: "umbrella" is never matched (AI-4.AC3), and neither is a filler that
//! is part of a sentence without pauses around it. The lists are deliberately short and
//! conservative:
//!
//! * English: `um`, `uh`, `uhm`, `erm`, `er`, with stretched spellings ("ummm", "uhhh") folded to
//!   the same word, and the two-word phrase `you know` (only when the phrase as a whole is
//!   isolated). `like`, `so`, `well` and `actually` are real words far too often to suggest.
//! * Vietnamese: `ờ`, `ờm`, `ừm`, `à`. `ừ` ("yes") and `ạ` (a politeness particle) are not
//!   fillers and are left out. Words are compared in Unicode NFC, which is what whisper.cpp
//!   writes.
//! * Any other language gets no built-in list; the caller can pass extra words.
//!
//! Each suggestion cuts the word's own time plus a [`PAD`] (30 ms, at most a quarter of the gap)
//! on each side, so the neighbouring words are not clipped by timestamp error.

use cm_core::MediaTime;

use super::CutSuggestion;
use crate::transcribe::{Transcript, Word};

/// Silence required on each side of a filler.
pub const MIN_GAP: MediaTime = MediaTime::from_millis(150);
/// Extra time cut on each side of a filler (at most a quarter of the gap there).
pub const PAD: MediaTime = MediaTime::from_millis(30);

/// Filler words and phrases for one language.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FillerLists {
    /// Single words, already folded (see [`fold`]).
    pub words: Vec<String>,
    /// Phrases as folded words, for example `["you", "know"]`.
    pub phrases: Vec<Vec<String>>,
}

impl FillerLists {
    /// The built-in list for a language code (`en`, `en-US`, `vi`, ...); empty for others.
    pub fn for_language(code: &str) -> FillerLists {
        let lang = code
            .split(['-', '_'])
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        let words = |w: &[&str]| w.iter().map(|s| fold(s)).collect();
        match lang.as_str() {
            "en" => FillerLists {
                words: words(&["um", "uh", "uhm", "erm", "er"]),
                phrases: vec![vec![fold("you"), fold("know")]],
            },
            "vi" => FillerLists {
                words: words(&["ờ", "ờm", "ừm", "à"]),
                phrases: Vec::new(),
            },
            _ => FillerLists {
                words: Vec::new(),
                phrases: Vec::new(),
            },
        }
    }

    /// Adds user-chosen single words.
    pub fn with_extra(mut self, extra: &[String]) -> FillerLists {
        self.words.extend(extra.iter().map(|w| fold(w)));
        self
    }
}

/// Lower-cases a token, strips punctuation at both ends and folds a run of the same letter to one
/// ("Ummm," becomes "um"). Only letters and digits stay inside a token.
pub fn fold(text: &str) -> String {
    let trimmed = text.trim_matches(|c: char| !c.is_alphanumeric());
    let mut out = String::with_capacity(trimmed.len());
    let mut last: Option<char> = None;
    for c in trimmed.chars().flat_map(char::to_lowercase) {
        if Some(c) != last {
            out.push(c);
        }
        last = Some(c);
    }
    out
}

fn gap_before(words: &[Word], i: usize) -> Option<i64> {
    i.checked_sub(1)
        .map(|p| (words[i].start - words[p].end).ticks())
}

fn gap_after(words: &[Word], i: usize) -> Option<i64> {
    words.get(i + 1).map(|n| (n.start - words[i].end).ticks())
}

/// Suggests the isolated filler words of a transcript, in session time (the transcript is in
/// session time already). `extra` adds single words to the language's list.
pub fn filler_cuts(transcript: &Transcript, extra: &[String]) -> Vec<CutSuggestion> {
    let lists = FillerLists::for_language(&transcript.language).with_extra(extra);
    if lists.words.is_empty() && lists.phrases.is_empty() {
        return Vec::new();
    }
    let words = &transcript.words;
    let folded: Vec<String> = words.iter().map(|w| fold(&w.text)).collect();
    let min_gap = MIN_GAP.ticks();
    let isolated = |first: usize, last: usize| {
        gap_before(words, first).is_none_or(|g| g >= min_gap)
            && gap_after(words, last).is_none_or(|g| g >= min_gap)
    };
    let cut = |first: usize, last: usize, label: String| {
        let pad = |gap: Option<i64>| gap.map_or(PAD.ticks(), |g| PAD.ticks().min(g / 4)).max(0);
        CutSuggestion {
            start: MediaTime::from_ticks(
                words[first].start.ticks() - pad(gap_before(words, first)),
            ),
            end: MediaTime::from_ticks(words[last].end.ticks() + pad(gap_after(words, last))),
            kind: "filler".into(),
            detail: label,
        }
    };

    let mut out = Vec::new();
    let mut i = 0;
    while i < words.len() {
        if words[i].end <= words[i].start {
            i += 1;
            continue;
        }
        // A phrase first (longest match), then a single word.
        let phrase = lists.phrases.iter().find(|p| {
            i + p.len() <= words.len() && p.iter().enumerate().all(|(k, w)| folded[i + k] == *w)
        });
        if let Some(p) = phrase {
            let last = i + p.len() - 1;
            if isolated(i, last) {
                out.push(cut(i, last, p.join(" ")));
                i = last + 1;
                continue;
            }
        }
        if lists.words.iter().any(|w| *w == folded[i]) && isolated(i, i) {
            out.push(cut(i, i, folded[i].clone()));
        }
        i += 1;
    }
    out
}
