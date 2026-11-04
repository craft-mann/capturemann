//! Scores a stored transcript with no decode: errors against a reference (folded and exact, tone
//! marks kept) and the counters that need no reference. Prints one JSON object of counts.
//!
//! `rescore --hyp <transcript.json or text> [--reference <txt>] [--until <secs>] [--speech <json>]
//!   [--language <code>]`
//!
//! A `.json` hypothesis is a transcript (`words[].text`, `start`/`end` in ticks, `sounds`);
//! `--until` keeps the words that start before that many seconds (the first 5 minutes of a longer
//! file). `--speech` is a list of `[start, end]` seconds with speech: words more than half a
//! second away from all of them are counted as outside speech, and the share of the speech with
//! words is the coverage.

use cm_ai::failure_kinds::word_coverage;
use cm_ai::failure_kinds::{dense_unit_indices, health, impossible_token_runs, sentence_runs};
use cm_ai::measure::{word_errors, word_errors_exact};
use cm_ai::transcribe::Word;
use serde_json::{json, Value};

fn option(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1).cloned())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = option(&args, "--hyp").expect("--hyp");
    let raw = std::fs::read_to_string(&path).expect("hypothesis");
    let until: f64 = option(&args, "--until")
        .and_then(|v| v.parse().ok())
        .unwrap_or(f64::MAX);
    let ticks = cm_core::TICKS_PER_SECOND as f64;
    let (words, sounds): (Vec<Word>, usize) = if path.ends_with(".json") {
        let v: Value = serde_json::from_str(&raw).expect("json");
        let all: Vec<Word> = serde_json::from_value(v["words"].clone()).expect("words");
        let sounds = v["sounds"].as_array().map_or(0, Vec::len);
        (
            all.into_iter()
                .filter(|w| w.start.ticks() as f64 / ticks < until)
                .collect(),
            sounds,
        )
    } else {
        (Vec::new(), 0)
    };
    let hyp = if path.ends_with(".json") {
        words
            .iter()
            .map(|w| w.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        raw
    };
    let mut out = json!({
        "words": words.len(),
        "last_word_secs": words.last().map(|w| w.end.ticks() as f64 / ticks),
        "sounds": sounds,
        "loop_runs": sentence_runs(&words, 3).len(),
        "dense_repeat_words": dense_unit_indices(&words, 5, 60.0).len(),
        "bare_token_runs": impossible_token_runs(&words, 5, 7.0, 0.5).len(),
    });
    if let Some(l) = option(&args, "--language") {
        let h = health(&words, &l);
        out["health"] = json!({"replacement_char": h.replacement_char, "foreign_script": h.foreign_script,
            "low_confidence": h.low_confidence, "broken_and_doubtful": h.broken_and_doubtful});
    }
    if let Some(r) = option(&args, "--reference") {
        let reference = std::fs::read_to_string(r).expect("reference");
        let (f, x) = (
            word_errors(&reference, &hyp),
            word_errors_exact(&reference, &hyp),
        );
        out["reference_words"] = json!(f.reference_words);
        out["folded"] = json!({"wer": f.wer(), "sub": f.substitutions, "del": f.deletions, "ins": f.insertions});
        out["exact"] = json!({"wer": x.wer(), "sub": x.substitutions, "del": x.deletions, "ins": x.insertions});
    }
    if let Some(p) = option(&args, "--speech") {
        let speech: Vec<(f64, f64)> =
            serde_json::from_str(&std::fs::read_to_string(p).expect("speech"))
                .expect("speech json");
        let outside = words
            .iter()
            .filter(|w| {
                let mid = (w.start.ticks() + w.end.ticks()) as f64 / 2.0 / ticks;
                !speech.iter().any(|&(a, b)| mid > a - 0.5 && mid < b + 0.5)
            })
            .count();
        let c = word_coverage(&speech, &words, 1.0, 5.0);
        out["outside_speech_words"] = json!(outside);
        out["coverage"] = json!(c.ratio());
    }
    println!("{out}");
}
