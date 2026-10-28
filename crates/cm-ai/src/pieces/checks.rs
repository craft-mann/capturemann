//! What makes a piece's words bad, beyond an exact loop (see [`super`]): a soft repetition loop,
//! where the same sentences come round again with small changes.
//!
//! Seen on Oct 28, 2026, on one 88 minute Vietnamese recording read twice with Large v3: the
//! noise-reduced audio had four stretches of 1 to 3 minutes of this, 10.8% of its 8-word groups
//! repeating an earlier one against 2.5% for the original microphone, and an exact loop check
//! ([`super::find_runs`]) saw none of it. The original microphone had its own bad stretch where
//! the other read real words, so a piece that fails the check is read again from the other audio
//! and the better reading is kept. The check is about the shape of the text, not about any
//! particular words.

use std::collections::HashSet;
use std::ops::Range;

use icu_normalizer::DecomposingNormalizerBorrowed;

use crate::transcribe::Word;

/// Words in a group compared for repetition.
const GROUP: usize = 8;
/// A stretch is repetition when this many of every [`WINDOW`] groups repeat an earlier one.
const WINDOW: usize = 60;
const DENSE: usize = 30;
/// Words in repetition stretches from which a piece counts as looping. Real speech repeats
/// sentences now and then (the original microphone had stretches of 64 to 94 words); the soft loops
/// were 249 to 626 words.
pub const SOFT_LOOP_WORDS: usize = 100;

/// A word as it is compared across spellings: lower case, no diacritics, `đ` as `d`, letters and
/// digits only ("Ghiền" and "ghien" are the same word).
pub fn fold(text: &str) -> String {
    let decomposed = DecomposingNormalizerBorrowed::new_nfd().normalize(text);
    decomposed
        .chars()
        .filter(|c| !('\u{300}'..='\u{36f}').contains(c))
        .flat_map(char::to_lowercase)
        .map(|c| if c == 'đ' { 'd' } else { c })
        .filter(|c| c.is_alphanumeric())
        .collect()
}

/// For every word, whether the group of [`GROUP`] words that starts there already appeared earlier
/// in `words` (the last words, with fewer than a full group after them, are `false`).
fn repeated_groups(tokens: &[String]) -> Vec<bool> {
    if tokens.len() < GROUP {
        return vec![false; tokens.len()];
    }
    let mut seen: HashSet<&[String]> = HashSet::new();
    let mut flags = vec![false; tokens.len()];
    for i in 0..=tokens.len() - GROUP {
        flags[i] = !seen.insert(&tokens[i..i + GROUP]);
    }
    flags
}

/// The stretches of `words` where the same sentences come round again: [`DENSE`] of every
/// [`WINDOW`] groups repeat an earlier one, held while half of the last [`DENSE`] still do.
pub fn repeat_regions(words: &[Word]) -> Vec<Range<usize>> {
    let tokens: Vec<String> = words.iter().map(|w| fold(&w.text)).collect();
    let flags = repeated_groups(&tokens);
    let n = flags.len();
    // Prefix sums: `at(a, b)` is how many of flags[a..b] are set.
    let mut sum = vec![0usize; n + 1];
    for i in 0..n {
        sum[i + 1] = sum[i] + usize::from(flags[i]);
    }
    let at = |a: usize, b: usize| sum[b] - sum[a];
    let mut out = Vec::new();
    let mut i = 0;
    while i + WINDOW <= n {
        if at(i, i + WINDOW) >= DENSE {
            let mut j = i + WINDOW;
            while j < n && at(j - DENSE, j) >= DENSE / 2 {
                j += 1;
            }
            out.push(i..(j + GROUP - 1).min(words.len()));
            i = j;
        } else {
            i += 20;
        }
    }
    out
}

/// How many words of `words` are in [`repeat_regions`].
pub fn soft_loop_words(words: &[Word]) -> usize {
    repeat_regions(words).iter().map(Range::len).sum()
}

/// How bad a piece's words are, to choose between two readings of the same audio.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Badness {
    /// Words in exact loops ([`super::find_runs`]).
    pub loop_words: usize,
    /// Words in soft repetition stretches ([`repeat_regions`]).
    pub soft_words: usize,
    /// An exact loop big enough to call the piece a loop ([`super::is_degenerate`]).
    pub degenerate: bool,
}

impl Badness {
    pub fn of(words: &[Word]) -> Badness {
        let runs = super::find_runs(words);
        Badness {
            loop_words: runs.iter().map(super::Run::len).sum(),
            soft_words: soft_loop_words(words),
            degenerate: super::is_degenerate(words),
        }
    }

    /// The piece has to be read again: an exact loop, or a soft loop of [`SOFT_LOOP_WORDS`] or
    /// more.
    pub fn is_bad(&self) -> bool {
        self.degenerate || self.soft_words >= SOFT_LOOP_WORDS
    }

    /// Lower is better: the words that repeat.
    pub fn score(&self) -> usize {
        self.loop_words.max(self.soft_words)
    }
}

/// `words` with the repeats inside [`repeat_regions`] dropped: the first time a group of words is
/// said it stays, the later times go. The last resort when both readings of a piece still repeat.
/// Returns the words and how many were dropped.
pub fn drop_soft_repeats(words: Vec<Word>) -> (Vec<Word>, usize) {
    let regions = repeat_regions(&words);
    if regions.is_empty() {
        return (words, 0);
    }
    let tokens: Vec<String> = words.iter().map(|w| fold(&w.text)).collect();
    let flags = repeated_groups(&tokens);
    let mut drop = vec![false; words.len()];
    for r in &regions {
        for i in r.clone() {
            if flags.get(i).copied().unwrap_or(false) {
                for flag in &mut drop[i..(i + GROUP).min(words.len())] {
                    *flag = true;
                }
            }
        }
    }
    let dropped = drop.iter().filter(|d| **d).count();
    let kept = words
        .into_iter()
        .zip(drop)
        .filter_map(|(w, d)| (!d).then_some(w))
        .collect();
    (kept, dropped)
}
