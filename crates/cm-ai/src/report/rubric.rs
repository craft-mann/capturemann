//! A floor check for a finished report (release 0.2.7, M2c): is every item tied to a moment that
//! exists, do all the speakers of the transcript show up, and are there names or numbers the
//! transcript never contained? Pure string and number matching against the transcript, no model.
//!
//! This is a **floor**, not a measure of correctness: a report can pass and still be wrong about
//! what was said, and the release text must not call a score "accuracy". It exists so a speed-up
//! (prompt reuse, smaller context, a different quantisation) can be compared with the baseline on
//! the same transcripts: a change that makes the floor worse is not a speed-up.
//!
//! The report is scored as stored (after [`super::check`] already removed uncited items and
//! unknown owners), so the generator's own counters (`dropped_uncited`, `reassigned_names`) are
//! reported next to it by the benchmark: they show how often the model needed that safety net.

use std::collections::HashSet;

use serde_json::Value;

use super::check::Checker;
use super::{Line, Source, Template};
use crate::textmatch::{is_word_char, nfc};

/// What the rubric found in one report.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RubricScore {
    /// Items of the template's model sections.
    pub items: usize,
    /// Items with at least one `at` that falls inside a transcript line.
    pub items_with_valid_time: usize,
    /// Items that need a time (their section does not mark `at` optional) and have none.
    pub items_missing_time: usize,
    /// Speakers named in the transcript, and how many of those appear in the report's text.
    pub speakers_expected: usize,
    pub speakers_present: usize,
    /// Speakers that never appear in the report's text.
    pub speakers_missing: Vec<String>,
    /// Numbers in the report that are not in the transcript (as digits or spelled out).
    pub invented_numbers: Vec<String>,
    /// Capitalised words inside a sentence of the report that are not words of the transcript, not
    /// speaker names and not attendees.
    pub invented_names: Vec<String>,
}

impl RubricScore {
    /// The floor: every item that needs a time has a valid one, all speakers appear, nothing
    /// invented. A report with no items passes only when it has no speakers to name.
    pub fn floor_ok(&self) -> bool {
        self.items_missing_time == 0
            && self.speakers_present == self.speakers_expected
            && self.invented_numbers.is_empty()
            && self.invented_names.is_empty()
    }

    /// One line for a benchmark table.
    pub fn summary(&self) -> String {
        format!(
            "items {}, timed {}, missing time {}, speakers {}/{}, invented numbers {}, invented names {}",
            self.items,
            self.items_with_valid_time,
            self.items_missing_time,
            self.speakers_present,
            self.speakers_expected,
            self.invented_numbers.len(),
            self.invented_names.len()
        )
    }
}

fn norm(s: &str) -> String {
    nfc(s).to_lowercase()
}

/// Every string value of `v`, outside the key `at`.
fn strings<'a>(v: &'a Value, out: &mut Vec<&'a str>) {
    match v {
        Value::String(s) => out.push(s),
        Value::Array(a) => a.iter().for_each(|c| strings(c, out)),
        Value::Object(m) => m
            .iter()
            .filter(|(k, _)| k.as_str() != "at")
            .for_each(|(_, c)| strings(c, out)),
        _ => {}
    }
}

const ONES: [&str; 20] = [
    "zero",
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
    "ten",
    "eleven",
    "twelve",
    "thirteen",
    "fourteen",
    "fifteen",
    "sixteen",
    "seventeen",
    "eighteen",
    "nineteen",
];
const TENS: [&str; 10] = [
    "", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety",
];

/// The value of one English number word, ordinals included (`twelfth`, `thirtieth`), as
/// `(value, kind)` where kind 0 is a unit 0..19, 1 is a ten, 2 is `hundred`, 3 is `thousand`.
fn number_word(w: &str) -> Option<(u64, u8)> {
    if let Some(i) = ONES.iter().position(|o| *o == w) {
        return Some((i as u64, 0));
    }
    if let Some(i) = TENS.iter().position(|o| !o.is_empty() && *o == w) {
        return Some((i as u64 * 10, 1));
    }
    match w {
        "hundred" => return Some((100, 2)),
        "thousand" => return Some((1000, 3)),
        "first" => return Some((1, 0)),
        "second" => return Some((2, 0)),
        "third" => return Some((3, 0)),
        "fifth" => return Some((5, 0)),
        "eighth" => return Some((8, 0)),
        "ninth" => return Some((9, 0)),
        "twelfth" => return Some((12, 0)),
        _ => {}
    }
    // "fourth", "sixth", "twentieth", "thirtieth" ...
    let stem = w
        .strip_suffix("ieth")
        .map(|s| format!("{s}y"))
        .or_else(|| w.strip_suffix("th").map(str::to_owned))?;
    number_word(&stem).filter(|(_, k)| *k <= 1)
}

/// The numbers written in `text`: digit runs (`12`, `3.5`, `40,000`) and spelled-out numbers
/// (`thirty-three`, `two hundred and five`, `twenty-eighth`), as strings of the integer value or
/// the digits as written.
pub fn numbers_in(text: &str) -> HashSet<String> {
    numbers(text, true)
}

/// The numbers a *report* states. Digit runs always count; spelled numbers count only when they
/// start with a ten, `hundred` or `thousand` ("twenty-one", "two hundred" does not: "two" alone is
/// a pronoun as often as a number), and ordinals ("second quarter") never do, so ordinary words
/// do not read as invented numbers.
pub fn numbers_stated(text: &str) -> HashSet<String> {
    numbers(text, false)
}

fn numbers(text: &str, everything: bool) -> HashSet<String> {
    let mut out = HashSet::new();
    let lower = norm(text);
    // Digit runs, with `,` between digit groups and `.` inside a decimal kept.
    let chars: Vec<char> = lower.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_ascii_digit() {
            let mut j = i;
            let mut run = String::new();
            while j < chars.len() {
                let c = chars[j];
                let joins = (c == ',' || c == '.')
                    && j + 1 < chars.len()
                    && chars[j + 1].is_ascii_digit()
                    && !run.is_empty();
                if c.is_ascii_digit() {
                    run.push(c);
                } else if joins && c == '.' {
                    run.push('.');
                } else if !joins {
                    break;
                }
                j += 1;
            }
            let trimmed = run.trim_start_matches('0');
            out.insert(if trimmed.is_empty() { "0" } else { trimmed }.to_owned());
            i = j;
        } else {
            i += 1;
        }
    }
    // Spelled numbers: a run of number words, ended by any other word or by punctuation.
    let mut words: Vec<String> = Vec::new();
    let mut cur = String::new();
    for c in lower.chars().chain(std::iter::once(' ')) {
        if c.is_alphabetic() {
            cur.push(c);
        } else {
            if !cur.is_empty() {
                words.push(std::mem::take(&mut cur));
            }
            if c != '-' && c != ' ' {
                words.push(String::new()); // punctuation: ends a number
            }
        }
    }
    let mut total = 0u64;
    let mut current = 0u64;
    let mut any = false;
    // Whether the run began with a ten, hundred or thousand (only those count for a report).
    let mut big = false;
    let flush = |total: &mut u64,
                 current: &mut u64,
                 any: &mut bool,
                 big: &mut bool,
                 out: &mut HashSet<String>| {
        if *any && (everything || *big) {
            out.insert((*total + *current).to_string());
        }
        *total = 0;
        *current = 0;
        *any = false;
        *big = false;
    };
    for w in &words {
        let ordinal = w.ends_with("th") || matches!(w.as_str(), "first" | "second" | "third");
        match number_word(w) {
            Some((_, 0)) if ordinal && !everything => {
                flush(&mut total, &mut current, &mut any, &mut big, &mut out)
            }
            Some((v, kind @ (0 | 1))) => {
                big |= !any && kind == 1;
                current += v;
                any = true;
                if ordinal {
                    flush(&mut total, &mut current, &mut any, &mut big, &mut out);
                }
            }
            Some((v, 2)) => {
                big |= !any || current > 0;
                current = current.max(1) * v;
                any = true;
            }
            Some((v, _)) => {
                big = true;
                total += current.max(1) * v;
                current = 0;
                any = true;
            }
            None if w == "and" && any => {}
            None => flush(&mut total, &mut current, &mut any, &mut big, &mut out),
        }
    }
    flush(&mut total, &mut current, &mut any, &mut big, &mut out);
    out
}

fn words_lower(s: &str) -> Vec<String> {
    norm(s)
        .split(|c: char| !is_word_char(c))
        .filter(|w| !w.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Capitalised words that sit inside a sentence of `text` (not its first word, not after `. `, `: `
/// or a bullet), with trailing punctuation removed.
fn mid_sentence_capitals(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut sentence_start = true;
    for raw in text.split_whitespace() {
        let word: String = raw
            .trim_matches(|c: char| !is_word_char(c))
            .trim_end_matches("'s")
            .to_owned();
        let starts_upper = word.chars().next().is_some_and(char::is_uppercase);
        let has_lower = word.chars().any(char::is_lowercase);
        if !sentence_start && starts_upper && has_lower && !word.is_empty() {
            out.push(word);
        }
        sentence_start = raw.ends_with(['.', '!', '?', ':']) || raw == "-" || raw == "*";
    }
    out
}

/// Scores `content` (the stored `{ section: [item] }` of a report) against the transcript
/// `lines`. `speakers` are the speaker names given to the report (also read from the lines);
/// `attendees` the names the user typed.
pub fn score(
    template: &Template,
    content: &Value,
    lines: &[Line],
    speakers: &[String],
    attendees: &[String],
) -> RubricScore {
    let mut names: Vec<String> = speakers.to_vec();
    for l in lines {
        if let Some(s) = l.speaker.as_deref().filter(|s| !s.trim().is_empty()) {
            if !names.iter().any(|n| n == s) {
                names.push(s.to_owned());
            }
        }
    }
    let checker = Checker::new(lines, &[]);
    let mut s = RubricScore::default();
    let mut texts: Vec<&str> = Vec::new();
    for section in template
        .sections
        .iter()
        .filter(|x| x.source == Source::Model)
    {
        let Some(items) = content.get(&section.id).and_then(Value::as_array) else {
            continue;
        };
        for item in items {
            s.items += 1;
            let timed = item
                .get("at")
                .and_then(Value::as_array)
                .is_some_and(|a| a.iter().filter_map(Value::as_f64).any(|t| checker.cited(t)));
            s.items_with_valid_time += usize::from(timed);
            s.items_missing_time += usize::from(!timed && !section.at_optional);
            strings(item, &mut texts);
        }
    }
    let report_words: HashSet<String> = texts.iter().flat_map(|t| words_lower(t)).collect();

    // Speakers: a name appears when all its words are words of the report text.
    s.speakers_expected = names.len();
    for n in &names {
        let parts = words_lower(n);
        let present = !parts.is_empty() && parts.iter().all(|p| report_words.contains(p));
        if present {
            s.speakers_present += 1;
        } else {
            s.speakers_missing.push(n.clone());
        }
    }

    // Numbers: every number in the report must be one the transcript, a speaker name or an
    // attendee contains.
    let mut known = HashSet::new();
    for l in lines {
        known.extend(numbers_in(&l.text));
    }
    for n in names.iter().chain(attendees) {
        known.extend(numbers_in(n));
    }
    let mut invented: Vec<String> = Vec::new();
    for t in &texts {
        for n in numbers_stated(t) {
            if !known.contains(&n) && !invented.contains(&n) {
                invented.push(n);
            }
        }
    }
    invented.sort();
    s.invented_numbers = invented;

    // Names: capitalised mid-sentence words unknown to the transcript.
    let transcript_words: HashSet<String> =
        lines.iter().flat_map(|l| words_lower(&l.text)).collect();
    let name_words: HashSet<String> = names
        .iter()
        .chain(attendees)
        .flat_map(|n| words_lower(n))
        .collect();
    let mut bad: Vec<String> = Vec::new();
    for t in &texts {
        for w in mid_sentence_capitals(t) {
            let key = norm(&w);
            if !transcript_words.contains(&key) && !name_words.contains(&key) && !bad.contains(&w) {
                bad.push(w);
            }
        }
    }
    bad.sort();
    s.invented_names = bad;
    s
}

#[cfg(test)]
mod tests;
