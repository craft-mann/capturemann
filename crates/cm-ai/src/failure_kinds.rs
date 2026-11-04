//! Counters for the three transcript failure kinds found on a real meeting (release 0.2.7, M2d):
//! speech that got no words, one short word recurring after long gaps, and characters that are not
//! text. These are **measures for benchmarks and tests**, not repairs: they say how bad a
//! transcript is, they never change it. A fix (re-reading uncovered speech, dropping spread-out
//! repeats, re-decoding broken tokens) is a separate, later change that these numbers judge.
//! General rules only: gaps, signal, confidence and character classes, never a list of phrases.

use std::collections::HashMap;

use crate::pieces::fold;
use crate::transcribe::Word;

fn secs(t: cm_core::MediaTime) -> f64 {
    t.ticks() as f64 / cm_core::TICKS_PER_SECOND as f64
}

/// How much of the speech has words. `speech` are the stretches a voice-activity or speaker engine
/// calls speech, in seconds; a second of speech is covered when a word lies within `pad` seconds.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Coverage {
    pub speech_secs: f64,
    pub covered_secs: f64,
    /// Stretches of speech with no word near them, at least `min_gap` long, in seconds.
    pub uncovered: Vec<(f64, f64)>,
}

impl Coverage {
    /// Seconds of speech with words over seconds of speech (1.0 when there is no speech).
    pub fn ratio(&self) -> f64 {
        if self.speech_secs <= 0.0 {
            1.0
        } else {
            (self.covered_secs / self.speech_secs).clamp(0.0, 1.0)
        }
    }

    pub fn uncovered_secs(&self) -> f64 {
        self.uncovered.iter().map(|(a, b)| b - a).sum()
    }
}

/// Word coverage of `speech` by `words` (spans in seconds). Resolution: 50 ms.
pub fn coverage(speech: &[(f64, f64)], words: &[(f64, f64)], pad: f64, min_gap: f64) -> Coverage {
    const STEP: f64 = 0.05;
    let mut spans: Vec<(f64, f64)> = words
        .iter()
        .map(|(a, b)| (a - pad, b.max(*a) + pad))
        .collect();
    spans.sort_by(|a, b| a.0.total_cmp(&b.0));
    let near_word = |t: f64| {
        let i = spans.partition_point(|s| s.0 <= t);
        // Spans are sorted by start but may differ in length: look back a little.
        spans[i.saturating_sub(64)..i].iter().any(|s| t <= s.1)
    };
    let mut out = Coverage::default();
    for &(a, b) in speech {
        let n = ((b - a) / STEP).floor() as usize;
        let mut gap_from: Option<f64> = None;
        for k in 0..=n {
            let t = a + k as f64 * STEP;
            let last = k == n;
            let covered = near_word(t);
            if k < n {
                out.speech_secs += STEP;
                if covered {
                    out.covered_secs += STEP;
                }
            }
            match (covered, gap_from) {
                (false, None) if !last => gap_from = Some(t),
                (true, Some(g)) | (false, Some(g)) if covered || last => {
                    let end = if covered { t } else { b };
                    if end - g >= min_gap {
                        out.uncovered.push((g, end));
                    }
                    gap_from = None;
                }
                _ => {}
            }
        }
    }
    out
}

/// [`coverage`] for transcript words.
pub fn word_coverage(speech: &[(f64, f64)], words: &[Word], pad: f64, min_gap: f64) -> Coverage {
    let spans: Vec<(f64, f64)> = words.iter().map(|w| (secs(w.start), secs(w.end))).collect();
    coverage(speech, &spans, pad, min_gap)
}

/// A word that keeps coming back after long gaps (a short word said again and again with silence
/// or other speech-free time before each one, which an in-a-row loop check does not see).
#[derive(Debug, Clone, PartialEq)]
pub struct SpreadRepeat {
    /// The folded text.
    pub text: String,
    /// How many times it occurs with a long gap before it.
    pub count: usize,
    /// The words' start times, seconds.
    pub at: Vec<f64>,
    /// Their mean confidence.
    pub mean_confidence: f32,
}

/// Indexes of the words that make up spread-out repeats (see [`spread_repeats`]).
pub fn spread_repeat_indices(
    words: &[Word],
    max_chars: usize,
    min_gap: f64,
    min_count: usize,
) -> Vec<usize> {
    let mut by_text: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, w) in words.iter().enumerate() {
        let text = fold(&w.text);
        if text.is_empty() || text.chars().count() > max_chars {
            continue;
        }
        let gap = if i == 0 {
            f64::INFINITY
        } else {
            secs(w.start) - secs(words[i - 1].end)
        };
        if gap >= min_gap {
            by_text.entry(text).or_default().push(i);
        }
    }
    let mut out: Vec<usize> = by_text
        .into_values()
        .filter(|v| v.len() >= min_count)
        .flatten()
        .collect();
    out.sort_unstable();
    out
}

/// Words (folded text up to `max_chars` letters) that occur at least `min_count` times, each time
/// after a gap of at least `min_gap` seconds from the word before it. Several occurrences of the
/// same short text, each alone after a long silence, are rarely speech.
pub fn spread_repeats(
    words: &[Word],
    max_chars: usize,
    min_gap: f64,
    min_count: usize,
) -> Vec<SpreadRepeat> {
    let mut by_text: HashMap<String, Vec<(f64, f32)>> = HashMap::new();
    for i in spread_repeat_indices(words, max_chars, min_gap, min_count) {
        let w = &words[i];
        by_text
            .entry(fold(&w.text))
            .or_default()
            .push((secs(w.start), w.confidence));
    }
    let mut out: Vec<SpreadRepeat> = by_text
        .into_iter()
        .map(|(text, v)| SpreadRepeat {
            text,
            count: v.len(),
            mean_confidence: v.iter().map(|x| x.1).sum::<f32>() / v.len() as f32,
            at: v.iter().map(|x| x.0).collect(),
        })
        .collect();
    out.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.text.cmp(&b.text)));
    out
}

/// A short sentence said again and again in a row (`unit` words, `count` times), found on sentence
/// units: a unit ends at a word with sentence punctuation (`.`, `?`, `!`, `…`) or after
/// `MAX_UNIT_WORDS` words, and units are compared with case and punctuation folded. This is the
/// loop that a decoder writes over speech it cannot read (one short sentence, again and again),
/// which the exact-word loop check ([`crate::pieces::find_runs`]) misses because the repeats
/// carry full stops and may be split across lines.
#[derive(Debug, Clone, PartialEq)]
pub struct SentenceRun {
    /// Index of the first word of the run in the word list, and one past the last.
    pub first: usize,
    pub end: usize,
    /// Words in one repeated unit, and how many times it occurs.
    pub unit_words: usize,
    pub count: usize,
    /// The folded text of one unit (compared, never shown).
    pub text: String,
}

/// Longest unit looked for (words).
pub const MAX_UNIT_WORDS: usize = 6;

/// The runs of one sentence unit repeated at least `min_count` times in a row.
pub fn sentence_runs(words: &[Word], min_count: usize) -> Vec<SentenceRun> {
    // Units: (first word, one past the last word, folded text).
    let mut units: Vec<(usize, usize, String)> = Vec::new();
    let mut start = 0;
    for (i, w) in words.iter().enumerate() {
        let ends = w.text.trim_end().ends_with(['.', '?', '!', '…']);
        if ends || i + 1 - start >= MAX_UNIT_WORDS || i + 1 == words.len() {
            let text: Vec<String> = words[start..=i].iter().map(|w| fold(&w.text)).collect();
            units.push((start, i + 1, text.join(" ")));
            start = i + 1;
        }
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i < units.len() {
        let mut j = i + 1;
        while j < units.len() && units[j].2 == units[i].2 && !units[i].2.is_empty() {
            j += 1;
        }
        if j - i >= min_count {
            out.push(SentenceRun {
                first: units[i].0,
                end: units[j - 1].1,
                unit_words: units[i].1 - units[i].0,
                count: j - i,
                text: units[i].2.clone(),
            });
        }
        i = j.max(i + 1);
    }
    out
}

/// Indexes of the words of sentence units (see [`sentence_runs`]) that occur at least `min_count`
/// times within `window_secs` of each other, whether or not they follow each other: the same short
/// sentence coming back five times in a minute is rarely speech, and lines in between do not
/// hide it. Compared after folding, so it is independent of language and wording.
pub fn dense_unit_indices(words: &[Word], min_count: usize, window_secs: f64) -> Vec<usize> {
    let mut units: Vec<(usize, usize, String)> = Vec::new();
    let mut start = 0;
    for (i, w) in words.iter().enumerate() {
        let ends = w.text.trim_end().ends_with(['.', '?', '!', '…']);
        if ends || i + 1 - start >= MAX_UNIT_WORDS || i + 1 == words.len() {
            let text: Vec<String> = words[start..=i].iter().map(|w| fold(&w.text)).collect();
            units.push((start, i + 1, text.join(" ")));
            start = i + 1;
        }
    }
    let mut by_text: HashMap<&str, Vec<usize>> = HashMap::new();
    for (k, u) in units.iter().enumerate() {
        if !u.2.is_empty() {
            by_text.entry(u.2.as_str()).or_default().push(k);
        }
    }
    let mut out = Vec::new();
    for ks in by_text.values().filter(|ks| ks.len() >= min_count) {
        // Units (in time order) that sit in some stretch of `window_secs` holding `min_count` of them.
        let at = |k: usize| secs(words[units[k].0].start);
        let mut marked = vec![false; ks.len()];
        let mut lo = 0;
        for hi in 0..ks.len() {
            while at(ks[hi]) - at(ks[lo]) > window_secs {
                lo += 1;
            }
            if hi + 1 - lo >= min_count {
                marked[lo..=hi].iter_mut().for_each(|m| *m = true);
            }
        }
        for (m, &k) in marked.iter().zip(ks) {
            if *m {
                out.extend(units[k].0..units[k].1);
            }
        }
    }
    out.sort_unstable();
    out
}

/// Stretches where one folded token is written `min_count` or more times in a row at an
/// impossible pace (more than `max_words_per_sec` words a second, a zero-length stretch counting as
/// impossible) or, from eight repeats on, with a mean confidence under `low_confidence`. A word
/// said again a few times at a human pace ("no no no") is not one. Returns `(first, end)` word
/// indexes.
pub fn impossible_token_runs(
    words: &[Word],
    min_count: usize,
    max_words_per_sec: f64,
    low_confidence: f32,
) -> Vec<(usize, usize)> {
    let tokens: Vec<String> = words.iter().map(|w| fold(&w.text)).collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < words.len() {
        let mut j = i + 1;
        while j < words.len() && !tokens[i].is_empty() && tokens[j] == tokens[i] {
            j += 1;
        }
        let count = j - i;
        if count >= min_count {
            let span = secs(words[j - 1].end) - secs(words[i].start);
            let rate = count as f64 / span.max(1e-3);
            let conf = words[i..j].iter().map(|w| w.confidence).sum::<f32>() / count as f32;
            if rate > max_words_per_sec || (count >= 8 && conf < low_confidence) {
                out.push((i, j));
            }
        }
        i = j;
    }
    out
}

/// A coarse writing system, enough to tell Latin text from Hangul or Cyrillic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Script {
    Latin,
    Cyrillic,
    Greek,
    Arabic,
    Hebrew,
    Devanagari,
    Thai,
    Hangul,
    Kana,
    Han,
    Other,
}

/// The script of a letter (`None` for digits, punctuation and symbols).
pub fn script_of(c: char) -> Option<Script> {
    if !c.is_alphabetic() {
        return None;
    }
    let u = c as u32;
    Some(match u {
        0x0041..=0x024F | 0x1E00..=0x1EFF | 0x2C60..=0x2C7F | 0xA720..=0xA7FF | 0xFF21..=0xFF5A => {
            Script::Latin
        }
        0x0370..=0x03FF | 0x1F00..=0x1FFF => Script::Greek,
        0x0400..=0x052F => Script::Cyrillic,
        0x0590..=0x05FF => Script::Hebrew,
        0x0600..=0x06FF | 0x0750..=0x077F => Script::Arabic,
        0x0900..=0x097F => Script::Devanagari,
        0x0E00..=0x0E7F => Script::Thai,
        0x1100..=0x11FF | 0x3130..=0x318F | 0xAC00..=0xD7AF => Script::Hangul,
        0x3040..=0x30FF => Script::Kana,
        0x3400..=0x4DBF | 0x4E00..=0x9FFF => Script::Han,
        _ => Script::Other,
    })
}

/// The scripts a transcript in `language` (`en`, `vi`, ...) may use. Languages not listed return
/// `None`: no claim is made about them.
pub fn expected_scripts(language: &str) -> Option<&'static [Script]> {
    let base = language.split(['-', '_']).next().unwrap_or(language);
    match base {
        "en" | "vi" | "es" | "fr" | "de" | "it" | "pt" | "nl" | "pl" | "tr" | "id" | "sv"
        | "da" | "no" | "fi" | "cs" | "ro" | "hu" => Some(&[Script::Latin]),
        "ru" | "uk" | "bg" => Some(&[Script::Cyrillic, Script::Latin]),
        "ko" => Some(&[Script::Hangul, Script::Latin]),
        "ja" => Some(&[Script::Kana, Script::Han, Script::Latin]),
        "zh" => Some(&[Script::Han, Script::Latin]),
        _ => None,
    }
}

/// Counts of words that do not look like text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Health {
    pub words: usize,
    /// Words containing U+FFFD.
    pub replacement_char: usize,
    /// Words with a letter in a script the language does not use.
    pub foreign_script: usize,
    /// Words under 5 % confidence.
    pub low_confidence: usize,
    /// Words that are foreign-script or U+FFFD *and* under 20 % confidence (the ones a fix would
    /// re-decode first).
    pub broken_and_doubtful: usize,
}

/// How many of `words` are broken for a transcript in `language`.
pub fn health(words: &[Word], language: &str) -> Health {
    let scripts = expected_scripts(language);
    let mut h = Health {
        words: words.len(),
        ..Health::default()
    };
    for w in words {
        let broken = w.text.contains('\u{FFFD}');
        let foreign = scripts.is_some_and(|ok| {
            w.text
                .chars()
                .filter_map(script_of)
                .any(|s| !ok.contains(&s))
        });
        h.replacement_char += usize::from(broken);
        h.foreign_script += usize::from(foreign);
        h.low_confidence += usize::from(w.confidence < 0.05);
        h.broken_and_doubtful += usize::from((broken || foreign) && w.confidence < 0.2);
    }
    h
}

/// Characters that are never text in a transcript: the replacement character, control characters
/// (except white space), zero-width and private-use characters.
fn is_stray(c: char) -> bool {
    c == '\u{FFFD}'
        || (c.is_control() && !c.is_whitespace())
        || matches!(c, '\u{200B}'..='\u{200F}' | '\u{2060}' | '\u{FEFF}' | '\u{E000}'..='\u{F8FF}')
}

/// Whether `text` holds a stray character.
pub fn has_stray_chars(text: &str) -> bool {
    text.chars().any(is_stray)
}

/// `text` without its stray characters.
pub fn strip_stray_chars(text: &str) -> String {
    text.chars().filter(|c| !is_stray(*c)).collect()
}

/// A word that is not text for a transcript in `language`: nothing left once stray characters are
/// removed, or a letter of a script the language does not use on a confidence under 0.2. A word
/// with a stray character beside real letters is not broken (it is cleaned), a foreign letter on
/// high confidence is a name or a loan word and stays.
pub fn is_broken(w: &Word, language: &str) -> bool {
    if strip_stray_chars(&w.text).trim().is_empty() {
        return true;
    }
    let foreign = expected_scripts(language).is_some_and(|ok| {
        w.text
            .chars()
            .filter_map(script_of)
            .any(|s| !ok.contains(&s))
    });
    foreign && w.confidence < 0.2
}

#[cfg(test)]
mod tests;
