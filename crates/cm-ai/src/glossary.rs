//! The personal glossary (AI-6a): names and terms the user wants spelled their way.
//!
//! Stored as the app-only setting [`SETTING_GLOSSARY`] (`ai.glossary`), a JSON array of
//! [`GlossaryEntry`]. It is used twice, both offline:
//!
//! * [`initial_prompt`] builds Whisper's `initial_prompt` from the terms, so the model is more
//!   likely to write them as given. Whisper reads at most about 224 tokens of prompt; the prompt is
//!   capped at [`PROMPT_TOKEN_BUDGET`] estimated tokens (see [`estimate_tokens`]), first terms
//!   first, so a long glossary never makes a run fail.
//! * [`apply`] runs after every transcription and replaces each `heard_as` spelling in the words
//!   with the term. Matching is word by word, case-insensitive and Unicode-NFC-equal, with
//!   punctuation at the edges of a word kept; the diacritics must match exactly (`dê` never
//!   matches `đê`). A spelling can span several words: they merge into one word that keeps the
//!   first start, the last end and the lowest confidence.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::textmatch::{is_word_char, key, nfc};
use crate::transcribe::Word;

/// The setting that holds the glossary.
pub const SETTING_GLOSSARY: &str = "ai.glossary";

/// Estimated tokens the prompt may use. Whisper keeps the last `n_text_ctx / 2 = 224` prompt
/// tokens; the estimate below is deliberately pessimistic for accented text, and this budget
/// leaves a further margin, so the real prompt is always well inside the window.
pub const PROMPT_TOKEN_BUDGET: usize = 160;
/// Most entries, longest term or spelling (characters), most spellings per entry.
pub const MAX_ENTRIES: usize = 100;
pub const MAX_TERM_CHARS: usize = 48;
pub const MAX_HEARD_AS: usize = 8;
/// Largest serialized glossary (bytes): the settings store refuses values over 16 KiB.
pub const MAX_SERIALIZED_BYTES: usize = 15_000;

/// One glossary entry.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct GlossaryEntry {
    /// The correct spelling.
    pub term: String,
    /// Wrong spellings Whisper tends to produce.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub heard_as: Vec<String>,
}

impl GlossaryEntry {
    pub fn new(term: &str, heard_as: &[&str]) -> Self {
        GlossaryEntry {
            term: term.to_owned(),
            heard_as: heard_as.iter().map(|s| (*s).to_owned()).collect(),
        }
    }
}

/// Trims, drops empty and duplicate spellings (NFC, case-insensitive) and the term itself from
/// `heard_as`, and clips to the limits. `None` when the term is empty.
pub fn clean_entry(e: &GlossaryEntry) -> Option<GlossaryEntry> {
    let term: String = e
        .term
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(MAX_TERM_CHARS)
        .collect();
    if term.is_empty() {
        return None;
    }
    let mut seen: Vec<String> = Vec::new();
    let mut heard_as = Vec::new();
    for h in &e.heard_as {
        let h: String = h
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(MAX_TERM_CHARS)
            .collect();
        let k = key(&h, false);
        // A spelling equal to the term (same case) would change nothing; a different case is a fix.
        if h.is_empty() || seen.contains(&k) || key(&h, true) == key(&term, true) {
            continue;
        }
        seen.push(k);
        heard_as.push(h);
        if heard_as.len() == MAX_HEARD_AS {
            break;
        }
    }
    Some(GlossaryEntry { term, heard_as })
}

/// Reads the setting value: an array of entries; anything malformed is skipped (a damaged
/// setting never blocks a transcription). Entries are cleaned and capped at [`MAX_ENTRIES`].
pub fn from_value(v: Option<&Value>) -> Vec<GlossaryEntry> {
    let Some(arr) = v.and_then(Value::as_array) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|x| serde_json::from_value::<GlossaryEntry>(x.clone()).ok())
        .filter_map(|e| clean_entry(&e))
        .take(MAX_ENTRIES)
        .collect()
}

/// Whether `entries` can be stored: within the entry limit and the settings value size.
pub fn fits(entries: &[GlossaryEntry]) -> bool {
    entries.len() <= MAX_ENTRIES
        && serde_json::to_vec(entries).is_ok_and(|b| b.len() <= MAX_SERIALIZED_BYTES)
}

// ---- the Whisper prompt --------------------------------------------------------------------

/// A pessimistic token count for `text`: one token per three ASCII characters (rounded up) and
/// three per other character (Whisper's byte-level vocabulary spends up to three bytes on an
/// accented or non-Latin character that has no token of its own).
pub fn estimate_tokens(text: &str) -> usize {
    let ascii = text.chars().filter(char::is_ascii).count();
    let other = text.chars().count() - ascii;
    ascii.div_ceil(3) + other * 3
}

/// Whisper's `initial_prompt` for these terms: the terms separated by commas, in the order given,
/// as many as fit in [`PROMPT_TOKEN_BUDGET`] estimated tokens (a term that does not fit is
/// skipped and a later, shorter one may still be used). `None` when there is nothing to say.
pub fn initial_prompt(entries: &[GlossaryEntry]) -> Option<String> {
    let mut used = 0usize;
    let mut terms: Vec<&str> = Vec::new();
    for e in entries {
        let term = e.term.trim();
        if term.is_empty() || term.contains('\0') {
            continue;
        }
        // The separator ", " costs about two tokens.
        let cost = estimate_tokens(term) + 2;
        if used + cost > PROMPT_TOKEN_BUDGET {
            continue;
        }
        used += cost;
        terms.push(term);
    }
    (!terms.is_empty()).then(|| terms.join(", "))
}

// ---- replacing what was heard ---------------------------------------------------------------

/// `word` split into leading punctuation, the core and trailing punctuation.
fn split_edges(word: &str) -> (&str, &str, &str) {
    let start = word
        .char_indices()
        .find(|(_, c)| is_word_char(*c))
        .map(|(i, _)| i);
    let Some(start) = start else {
        return (word, "", "");
    };
    let end = word
        .char_indices()
        .rev()
        .find(|(_, c)| is_word_char(*c))
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(word.len());
    (&word[..start], &word[start..end], &word[end..])
}

/// A spelling to look for: the keys of its words.
struct Pattern {
    words: Vec<String>,
    term: usize,
}

/// Replaces every `heard_as` spelling in `words` with its term (see the module docs). Returns the
/// number of replacements. Longer spellings win where two overlap; text produced by a
/// replacement is not searched again. An empty glossary returns the words unchanged.
pub fn apply(words: Vec<Word>, entries: &[GlossaryEntry]) -> (Vec<Word>, usize) {
    let entries: Vec<GlossaryEntry> = entries.iter().filter_map(clean_entry).collect();
    let mut patterns: Vec<Pattern> = Vec::new();
    for (i, e) in entries.iter().enumerate() {
        for h in &e.heard_as {
            let ws: Vec<String> = h
                .split_whitespace()
                .map(|w| key(split_edges(w).1, false))
                .collect();
            if !ws.is_empty() && ws.iter().all(|w| !w.is_empty()) {
                patterns.push(Pattern { words: ws, term: i });
            }
        }
    }
    if patterns.is_empty() || words.is_empty() {
        return (words, 0);
    }
    // Longest first (in words, then in characters), so the best match is the first that fits.
    patterns.sort_by_key(|p| {
        std::cmp::Reverse((
            p.words.len(),
            p.words.iter().map(String::len).sum::<usize>(),
        ))
    });
    // Words are composed first, so a decomposed accent is part of its letter.
    let words: Vec<Word> = words
        .into_iter()
        .map(|w| Word {
            text: nfc(&w.text),
            ..w
        })
        .collect();
    let keys: Vec<String> = words
        .iter()
        .map(|w| key(split_edges(&w.text).1, false))
        .collect();
    let mut out: Vec<Word> = Vec::with_capacity(words.len());
    let mut count = 0;
    let mut i = 0;
    while i < words.len() {
        let hit = patterns.iter().find(|p| {
            i + p.words.len() <= words.len()
                && p.words
                    .iter()
                    .zip(&keys[i..])
                    .all(|(a, b)| !b.is_empty() && a == b)
        });
        match hit {
            Some(p) => {
                let span = &words[i..i + p.words.len()];
                let (lead, _, _) = split_edges(&span[0].text);
                let (_, _, trail) = split_edges(&span[span.len() - 1].text);
                out.push(Word {
                    text: format!("{lead}{}{trail}", entries[p.term].term),
                    start: span[0].start,
                    end: span[span.len() - 1].end,
                    confidence: span
                        .iter()
                        .map(|w| w.confidence)
                        .fold(f32::INFINITY, f32::min),
                });
                count += 1;
                i += p.words.len();
            }
            None => {
                out.push(words[i].clone());
                i += 1;
            }
        }
    }
    (out, count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcribe::media_secs;

    fn w(text: &str, s: f64, e: f64, c: f32) -> Word {
        Word {
            text: text.into(),
            start: media_secs(s),
            end: media_secs(e),
            confidence: c,
        }
    }

    fn texts(ws: &[Word]) -> Vec<&str> {
        ws.iter().map(|w| w.text.as_str()).collect()
    }

    #[test]
    fn an_empty_glossary_changes_nothing() {
        let ws = vec![w("hello", 0.0, 1.0, 0.9)];
        let (out, n) = apply(ws.clone(), &[]);
        assert_eq!((out, n), (ws.clone(), 0));
        let (out, n) = apply(ws.clone(), &[GlossaryEntry::new("Hello", &[])]);
        assert_eq!((out, n), (ws, 0));
    }

    #[test]
    fn a_single_word_is_replaced_case_insensitively_keeping_punctuation() {
        let ws = vec![w("Open", 0.0, 0.5, 0.9), w("Capture-man,", 0.5, 1.0, 0.8)];
        let g = [GlossaryEntry::new("CaptureMann", &["capture-man"])];
        let (out, n) = apply(ws, &g);
        assert_eq!(n, 1);
        assert_eq!(texts(&out), vec!["Open", "CaptureMann,"]);
    }

    #[test]
    fn a_multi_word_variant_merges_with_first_start_last_end_lowest_confidence() {
        let ws = vec![
            w("Use", 0.0, 0.4, 0.9),
            w("Logo", 0.5, 0.8, 0.7),
            w("Drive.", 0.8, 1.2, 0.3),
        ];
        let g = [GlossaryEntry::new("Google Drive", &["logo drive"])];
        let (out, n) = apply(ws, &g);
        assert_eq!(n, 1);
        assert_eq!(texts(&out), vec!["Use", "Google Drive."]);
        assert_eq!(out[1].start, media_secs(0.5));
        assert_eq!(out[1].end, media_secs(1.2));
        assert_eq!(out[1].confidence, 0.3);
    }

    #[test]
    fn vietnamese_diacritics_must_match_exactly() {
        let g = [GlossaryEntry::new("Đê", &["dê"])];
        let (out, n) = apply(vec![w("đê", 0.0, 1.0, 0.9)], &g);
        assert_eq!((texts(&out), n), (vec!["đê"], 0));
        let (out, n) = apply(vec![w("dê", 0.0, 1.0, 0.9)], &g);
        assert_eq!((texts(&out), n), (vec!["Đê"], 1));
        // The other way: heard as "đê" does not touch "dê".
        let g = [GlossaryEntry::new("Đê", &["đê"])];
        let (_, n) = apply(vec![w("dê", 0.0, 1.0, 0.9)], &g);
        assert_eq!(n, 0);
    }

    #[test]
    fn nfd_and_nfc_spellings_are_the_same() {
        // The variant is typed decomposed, Whisper wrote it precomposed.
        let g = [GlossaryEntry::new(
            "Gu gồ Drive",
            &["Gu go\u{0302}\u{0300} Drai"],
        )];
        let ws = vec![
            w("gu", 0.0, 0.2, 0.9),
            w("gồ", 0.2, 0.4, 0.9),
            w("DRAI", 0.4, 0.6, 0.9),
        ];
        let (out, n) = apply(ws, &g);
        assert_eq!(n, 1);
        assert_eq!(texts(&out), vec!["Gu gồ Drive"]);
        // And the reverse: the words are decomposed.
        let g = [GlossaryEntry::new("Tiếp", &["tiếp"])];
        let (out, n) = apply(vec![w("tie\u{0302}\u{0301}p", 0.0, 1.0, 0.9)], &g);
        assert_eq!((texts(&out), n), (vec!["Tiếp"], 1));
    }

    #[test]
    fn the_longer_of_two_overlapping_variants_wins() {
        let g = [
            GlossaryEntry::new("Drive", &["drive"]),
            GlossaryEntry::new("Google Drive", &["logo drive"]),
        ];
        let ws = vec![w("logo", 0.0, 0.5, 0.9), w("drive", 0.5, 1.0, 0.9)];
        let (out, n) = apply(ws, &g);
        assert_eq!((texts(&out), n), (vec!["Google Drive"], 1));
    }

    #[test]
    fn replaced_text_is_not_searched_again() {
        let g = [
            GlossaryEntry::new("b", &["a"]),
            GlossaryEntry::new("c", &["b"]),
        ];
        let (out, n) = apply(vec![w("a", 0.0, 1.0, 0.9)], &g);
        assert_eq!((texts(&out), n), (vec!["b"], 1));
    }

    #[test]
    fn the_prompt_lists_terms_and_is_capped_by_the_budget() {
        assert_eq!(initial_prompt(&[]), None);
        let g = [
            GlossaryEntry::new("Google Drive", &[]),
            GlossaryEntry::new("CaptureMann", &[]),
        ];
        assert_eq!(initial_prompt(&g).unwrap(), "Google Drive, CaptureMann");
        // 100 accented 40-character terms cannot all fit; the first ones are kept in order.
        let many: Vec<GlossaryEntry> = (0..100)
            .map(|i| GlossaryEntry::new(&format!("Tiếng Việt thử nghiệm số {i:03} xin chào"), &[]))
            .collect();
        let p = initial_prompt(&many).unwrap();
        assert!(
            estimate_tokens(&p) <= PROMPT_TOKEN_BUDGET,
            "{}",
            estimate_tokens(&p)
        );
        assert!(p.starts_with("Tiếng Việt thử nghiệm số 000"));
        assert!(p.contains("số 001") || !p.contains("số 099"));
        assert!(!p.contains("số 099"));
        // A term too big for what is left is skipped, a shorter later one is still used.
        let mut g = vec![GlossaryEntry::new(&"ế".repeat(40), &[]); 1];
        g.push(GlossaryEntry::new(&"ế".repeat(40), &[]));
        g.push(GlossaryEntry::new("ok", &[]));
        let p = initial_prompt(&g).unwrap();
        assert!(p.ends_with("ok"));
    }

    #[test]
    fn entries_are_cleaned_and_a_bad_setting_is_ignored() {
        let e = clean_entry(&GlossaryEntry::new(
            "  Google   Drive ",
            &["logo drive", "LOGO  drive", "", "Google Drive", "Gu gồ"],
        ))
        .unwrap();
        assert_eq!(e.term, "Google Drive");
        assert_eq!(e.heard_as, vec!["logo drive", "Gu gồ"]);
        assert!(clean_entry(&GlossaryEntry::new("   ", &["x"])).is_none());
        let v = serde_json::json!([{"term": "A", "heard_as": ["b"]}, {"nope": 1}, 7, {"term": ""}]);
        assert_eq!(from_value(Some(&v)), vec![GlossaryEntry::new("A", &["b"])]);
        assert!(from_value(Some(&serde_json::json!("x"))).is_empty());
        assert!(from_value(None).is_empty());
        assert!(fits(&[GlossaryEntry::new("A", &["b"])]));
    }
}
