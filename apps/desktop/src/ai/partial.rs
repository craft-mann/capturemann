//! The first part of a long transcription, shown while the rest is still being made (AI-1.AC10):
//! the daemon writes `transcript.partial.json` after every finished piece and `get_transcript`
//! answers with `partial: true`. This module is display-free: it turns that answer into rows
//! (a start time and a text) and words the banner. The page shows them read-only; nothing here
//! edits, exports or sends the partial transcript anywhere.

use cm_i18n::{tr, trn};
use serde_json::Value;

/// Ticks per second of a `MediaTime`.
const TICKS_PER_SEC: f64 = 120_000.0;
/// A pause this long ends a row.
const PAUSE_SECS: f64 = 1.5;
/// A row never grows past this many characters (it ends at the next word boundary after).
const MAX_ROW_CHARS: usize = 110;

/// One row of the partial transcript.
#[derive(Debug, Clone, PartialEq)]
pub struct PartialRow {
    pub start_secs: f64,
    pub text: String,
}

/// The first part of a transcript that is still being written.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PartialView {
    pub rows: Vec<PartialRow>,
    pub covered_secs: f64,
    pub total_secs: f64,
}

/// Reads a `get_transcript` answer: `Some` only when it is marked `partial: true`.
pub fn parse_partial(answer: &Value) -> Option<PartialView> {
    if answer.get("partial") != Some(&Value::Bool(true)) {
        return None;
    }
    let words = answer
        .get("transcript")?
        .get("words")?
        .as_array()?
        .iter()
        .filter_map(|w| {
            let text = w.get("text")?.as_str()?.trim();
            let start = w.get("start")?.as_f64()? / TICKS_PER_SEC;
            let end = w.get("end").and_then(Value::as_f64).unwrap_or(0.0) / TICKS_PER_SEC;
            (!text.is_empty()).then(|| (start, end.max(start), text.to_owned()))
        })
        .collect::<Vec<_>>();
    Some(PartialView {
        rows: rows_of(&words),
        covered_secs: answer.get("covered_secs").and_then(Value::as_f64)?,
        total_secs: answer.get("total_secs").and_then(Value::as_f64)?,
    })
}

/// Words `(start, end, text)` in rows: a row ends after a pause, at a sentence end once it has
/// some length, or when it is long.
pub fn rows_of(words: &[(f64, f64, String)]) -> Vec<PartialRow> {
    let mut rows: Vec<PartialRow> = Vec::new();
    let mut open: Option<(f64, String, f64)> = None;
    for (start, end, text) in words {
        if let Some((_, row_text, last_end)) = &open {
            let ended =
                row_text.ends_with(['.', '?', '!', '\u{3002}']) && row_text.chars().count() >= 40;
            if start - last_end > PAUSE_SECS || ended || row_text.chars().count() >= MAX_ROW_CHARS {
                if let Some((s, t, _)) = open.take() {
                    rows.push(PartialRow {
                        start_secs: s,
                        text: t,
                    });
                }
            }
        }
        match &mut open {
            Some((_, row_text, last_end)) => {
                row_text.push(' ');
                row_text.push_str(text);
                *last_end = *end;
            }
            None => open = Some((*start, text.clone(), *end)),
        }
    }
    if let Some((s, t, _)) = open {
        rows.push(PartialRow {
            start_secs: s,
            text: t,
        });
    }
    rows
}

/// `m:ss`, or `h:mm:ss` from an hour on.
pub fn clock(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

/// The line above the rows: how much is ready, and that the rest follows by itself.
pub fn banner(covered_secs: f64) -> String {
    let minutes = (covered_secs / 60.0).floor().max(1.0) as usize;
    trn!(
        "Transcribing: the first {n} minute is ready. The rest follows.",
        "Transcribing: the first {n} minutes are ready. The rest follows.",
        minutes
    )
}

/// What the read-only list says it is, for a screen reader and the tooltip of the list.
pub fn list_label() -> String {
    tr!("Partial transcript")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn w(start: f64, end: f64, text: &str) -> Value {
        json!({"text": text, "start": (start * TICKS_PER_SEC) as i64,
               "end": (end * TICKS_PER_SEC) as i64, "confidence": 0.9})
    }

    #[test]
    fn a_final_transcript_is_not_a_partial_one() {
        let a = json!({"capture_id": "x", "partial": false,
                       "transcript": {"words": [w(0.0, 1.0, "hi")]}});
        assert_eq!(parse_partial(&a), None);
        assert_eq!(parse_partial(&json!({"transcript": {"words": []}})), None);
    }

    #[test]
    fn words_become_rows_at_pauses_and_the_covered_time_is_kept() {
        let a = json!({"partial": true, "covered_secs": 600.0, "total_secs": 4400.0,
            "transcript": {"words": [w(1.0, 1.4, "Hello"), w(1.5, 2.0, "there"),
                                     w(10.0, 10.5, "Next"), w(10.6, 11.0, "part")]}});
        let v = parse_partial(&a).unwrap();
        assert_eq!(v.covered_secs, 600.0);
        assert_eq!(v.total_secs, 4400.0);
        assert_eq!(
            v.rows,
            vec![
                PartialRow {
                    start_secs: 1.0,
                    text: "Hello there".into()
                },
                PartialRow {
                    start_secs: 10.0,
                    text: "Next part".into()
                }
            ]
        );
    }

    #[test]
    fn a_long_run_of_words_is_cut_into_rows() {
        let words: Vec<(f64, f64, String)> = (0..60)
            .map(|i| {
                (
                    f64::from(i) * 0.3,
                    f64::from(i) * 0.3 + 0.2,
                    "word".to_owned(),
                )
            })
            .collect();
        let rows = rows_of(&words);
        assert!(rows.len() >= 2);
        assert!(rows
            .iter()
            .all(|r| r.text.chars().count() <= MAX_ROW_CHARS + 6));
        assert_eq!(
            rows.iter()
                .map(|r| r.text.split(' ').count())
                .sum::<usize>(),
            60
        );
    }

    #[test]
    fn the_clock_and_the_banner_read_in_minutes() {
        assert_eq!(clock(75.0), "1:15");
        assert_eq!(clock(3725.0), "1:02:05");
        assert!(banner(25.0 * 60.0 + 30.0).contains("first 25 minutes"));
        assert!(banner(10.0).contains("first 1 minute is"));
    }
}
