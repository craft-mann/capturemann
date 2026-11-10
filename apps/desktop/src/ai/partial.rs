//! The first part of a long transcription, shown while the rest is still being made (AI-1.AC10):
//! the daemon writes `transcript.partial.json` after every finished piece and `get_transcript`
//! answers with `partial: true`. This module is display-free: it turns that answer into rows
//! (a start time and a text) and words the banner. The page shows them read-only; nothing here
//! edits, exports or sends the partial transcript anywhere.

use cm_ai::caption::CaptionLine;
use cm_ai::transcribe::Word;
use cm_core::MediaTime;
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
    /// The words with their times (session clock, like the final transcript's) and the caption
    /// lines built from them the way a finished transcript's are: the playback words line reads
    /// these (AI-1.AC11).
    pub words: Vec<Word>,
    pub lines: Vec<CaptionLine>,
}

/// What the playback words line shows while a transcript is partial.
#[derive(Debug, Clone, PartialEq)]
pub enum PartialNow {
    /// The playhead is inside the covered part and under a line.
    Words(crate::recedit::voice::NowSpeaking),
    /// The playhead is inside the covered part but before the first word.
    Idle,
    /// The playhead is past the covered part: those words are still being made.
    Pending,
}

impl PartialView {
    /// The words line at `playhead`: the same lookup as a finished transcript's
    /// (`voice::now_speaking`), fed with the partial words; past the covered part it answers
    /// [`PartialNow::Pending`].
    pub fn now_at(&self, playhead: MediaTime) -> PartialNow {
        if playhead.as_secs_f64() >= self.covered_secs {
            return PartialNow::Pending;
        }
        match crate::recedit::voice::now_speaking(&self.lines, &self.words, &[], playhead) {
            Some(n) => PartialNow::Words(n),
            None => PartialNow::Idle,
        }
    }
}

/// The line shown where the words of the playhead are not ready yet.
pub fn pending_line() -> String {
    tr!("The words for this part are still being made.")
}

/// Reads a `get_transcript` answer: `Some` only when it is marked `partial: true`.
pub fn parse_partial(answer: &Value) -> Option<PartialView> {
    if answer.get("partial") != Some(&Value::Bool(true)) {
        return None;
    }
    let raw = answer
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
    let words: Vec<Word> = raw
        .iter()
        .map(|(start, end, text)| Word {
            text: text.clone(),
            start: MediaTime::from_ticks((start * TICKS_PER_SEC).round() as i64),
            end: MediaTime::from_ticks((end * TICKS_PER_SEC).round() as i64),
            confidence: 1.0,
        })
        .collect();
    Some(PartialView {
        rows: rows_of(&raw),
        covered_secs: answer.get("covered_secs").and_then(Value::as_f64)?,
        total_secs: answer.get("total_secs").and_then(Value::as_f64)?,
        lines: crate::ai::captions::lines_from_words(&words),
        words,
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

    fn two_sentences() -> PartialView {
        let a = json!({"partial": true, "covered_secs": 60.0, "total_secs": 600.0,
            "transcript": {"words": [w(1.0, 1.4, "Hello"), w(1.5, 2.0, "there"),
                                     w(10.0, 10.5, "Next"), w(10.6, 11.0, "part")]}});
        parse_partial(&a).unwrap()
    }

    fn at(secs: f64) -> MediaTime {
        MediaTime::from_secs_f64(secs).unwrap()
    }

    #[test]
    fn the_words_line_follows_the_partial_words_inside_the_covered_part() {
        let v = two_sentences();
        assert_eq!(v.words.len(), 4);
        match v.now_at(at(1.6)) {
            PartialNow::Words(n) => {
                assert!(n.text.starts_with("Hello there"), "{}", n.text);
                assert_eq!(n.speaker, None);
                // The current word is marked, as for a finished transcript.
                let r = n.highlight.expect("the word under the playhead");
                assert_eq!(&n.text[r], "there");
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(v.now_at(at(0.2)), PartialNow::Idle);
    }

    #[test]
    fn beyond_the_covered_part_the_line_says_the_words_are_still_being_made() {
        let v = two_sentences();
        assert_eq!(v.now_at(at(60.0)), PartialNow::Pending);
        assert_eq!(v.now_at(at(500.0)), PartialNow::Pending);
        assert!(pending_line().contains("still being made"));
    }

    #[test]
    fn the_partial_lookup_gives_what_the_final_one_gives_for_the_same_words() {
        let v = two_sentences();
        let direct = crate::recedit::voice::now_speaking(&v.lines, &v.words, &[], at(10.7));
        assert_eq!(v.now_at(at(10.7)), PartialNow::Words(direct.unwrap()));
    }
}
