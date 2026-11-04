use std::sync::Mutex;

use cm_core::MediaTime;
use serde_json::{json, Value};

use super::*;
use crate::text_model::{ErrorKind, JsonReply, JsonRequest, TextModel, TextModelError};

fn words(texts: &[&str]) -> Vec<Word> {
    texts
        .iter()
        .enumerate()
        .map(|(i, t)| Word {
            text: (*t).to_owned(),
            start: MediaTime::from_secs_f64(i as f64 * 0.5).unwrap(),
            end: MediaTime::from_secs_f64(i as f64 * 0.5 + 0.4).unwrap(),
            confidence: 0.9,
        })
        .collect()
}

/// The items of the `Input:` line of a prompt.
fn items_of(req: &JsonRequest) -> Vec<String> {
    let p = &req.messages[0].content;
    let line = p.lines().find(|l| l.starts_with("Input: ")).unwrap();
    let v: Value = serde_json::from_str(&line["Input: ".len()..]).unwrap();
    v["turns"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap().to_owned())
        .collect()
}

/// Capital first letter and a full stop; a script can break the answer.
struct Fake {
    calls: Mutex<Vec<Vec<String>>>,
    prompts: Mutex<Vec<String>>,
    /// Corrupts the answer for an item containing the first string, replacing it with the
    /// second (on every call when the flag is set, else only when the prompt has no warning).
    break_on: Option<(&'static str, &'static str, bool)>,
    /// Answers the first call with one item too few.
    short_first: bool,
    /// `OutputTooLong` for any call with more than this many items, or words.
    too_long_items: usize,
    too_long_words: usize,
}

impl Fake {
    fn good() -> Fake {
        Fake {
            calls: Mutex::new(vec![]),
            prompts: Mutex::new(vec![]),
            break_on: None,
            short_first: false,
            too_long_items: usize::MAX,
            too_long_words: usize::MAX,
        }
    }
}

fn punctuate(s: &str) -> String {
    let mut c = s.chars();
    let first: String = c
        .next()
        .map(|f| f.to_uppercase().collect())
        .unwrap_or_default();
    format!("{first}{}.", c.as_str())
}

impl TextModel for Fake {
    fn complete_json(&self, req: &JsonRequest) -> Result<JsonReply, TextModelError> {
        let items = items_of(req);
        let n_calls = {
            let mut c = self.calls.lock().unwrap();
            c.push(items.clone());
            c.len()
        };
        self.prompts
            .lock()
            .unwrap()
            .push(req.messages[0].content.clone());
        let words: usize = items.iter().map(|i| i.split_whitespace().count()).sum();
        if items.len() > self.too_long_items || words > self.too_long_words {
            return Err(TextModelError {
                kind: ErrorKind::OutputTooLong,
                message: "too long".into(),
            });
        }
        let strict = req.messages[0].content.contains("WARNING");
        let mut out: Vec<String> = items.iter().map(|i| punctuate(i)).collect();
        if let Some((find, replace, always)) = self.break_on {
            if always || !strict {
                for (o, i) in out.iter_mut().zip(&items) {
                    if i.contains(find) {
                        *o = o.replace(find, replace);
                    }
                }
            }
        }
        if self.short_first && n_calls == 1 {
            out.pop();
        }
        Ok(JsonReply {
            value: json!({ "turns": out }),
            attempts: 1,
        })
    }
}

fn labels(l: &[u32]) -> Vec<Option<u32>> {
    l.iter().map(|x| Some(*x)).collect()
}

fn run(
    model: &dyn TextModel,
    w: &[Word],
    speakers: Option<&[Option<u32>]>,
) -> (Vec<TurnResult>, Stats) {
    let opts = Options::new("vi", 8192);
    normalize(
        model,
        w,
        speakers,
        &opts,
        Vec::new(),
        &mut |_| {},
        &mut |_| true,
    )
    .unwrap()
}

#[test]
fn every_turn_is_punctuated_and_the_words_are_kept() {
    let w = words(&["xin", "chào", "các", "bạn", "hôm", "nay", "trời", "đẹp"]);
    let l = labels(&[1, 1, 1, 1, 2, 2, 2, 2]);
    let fake = Fake::good();
    let (r, stats) = run(&fake, &w, Some(&l));
    assert_eq!(r.len(), 2);
    assert!(r.iter().all(|t| t.normalized && t.reason.is_none()));
    assert_eq!(r[0].display, vec!["Xin", "chào", "các", "bạn."]);
    assert_eq!(r[1].display[0], "Hôm");
    // Both turns went in one call.
    assert_eq!(stats.model_calls, 1);
    assert_eq!(fake.calls.lock().unwrap()[0].len(), 2);
}

#[test]
fn a_changed_tone_mark_is_rejected_then_retried_strictly_then_kept_raw() {
    let w = words(&["ma", "ba", "ta"]);
    let fake = Fake {
        break_on: Some(("ba", "bá", true)),
        ..Fake::good()
    };
    let (r, stats) = run(&fake, &w, None);
    assert_eq!(r.len(), 1);
    assert!(!r[0].normalized);
    assert_eq!(r[0].reason.as_deref(), Some("changed_words"));
    // The raw words stand.
    assert_eq!(r[0].display, vec!["ma", "ba", "ta"]);
    assert_eq!(stats.model_calls, 2);
    assert_eq!(stats.retried, 1);
    assert_eq!(stats.first_substituted, 1);
    // The second call carried the warning.
    assert!(fake.prompts.lock().unwrap()[1].contains("WARNING"));
    assert!(!fake.prompts.lock().unwrap()[0].contains("WARNING"));
}

#[test]
fn a_failed_turn_does_not_spoil_its_neighbours_and_a_retry_can_fix_it() {
    let w = words(&["one", "two", "three", "four"]);
    let l = labels(&[1, 1, 2, 2]);
    // Broken only on the first (non-strict) answer.
    let fake = Fake {
        break_on: Some(("hree", "hrie", false)),
        ..Fake::good()
    };
    let (r, stats) = run(&fake, &w, Some(&l));
    assert!(r.iter().all(|t| t.normalized), "{r:?}");
    assert_eq!(stats.retried, 1);
    assert_eq!(stats.model_calls, 2);
    assert_eq!(r[1].display, vec!["Three", "four."]);
}

#[test]
fn a_wrong_item_count_asks_each_turn_alone() {
    let w = words(&["a", "b", "c", "d"]);
    let l = labels(&[1, 1, 2, 2]);
    let fake = Fake {
        short_first: true,
        ..Fake::good()
    };
    let (r, stats) = run(&fake, &w, Some(&l));
    assert!(r.iter().all(|t| t.normalized));
    assert_eq!(stats.model_calls, 3);
}

#[test]
fn a_reply_that_is_too_long_halves_the_batch_then_the_turn() {
    let texts: Vec<String> = (0..40).map(|i| format!("w{i}")).collect();
    let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
    let w = words(&refs);
    // Three one-word speakers, then one long turn: more than 2 items fail, over 12 words fail.
    let l: Vec<Option<u32>> = (0..40)
        .map(|i| Some(if i < 3 { i as u32 } else { 9 }))
        .collect();
    let fake = Fake {
        too_long_items: 2,
        too_long_words: 12,
        ..Fake::good()
    };
    let (r, _) = run(&fake, &w, Some(&l));
    let covered: usize = r.iter().map(|t| t.len).sum();
    assert_eq!(covered, 40);
    assert!(r.iter().all(|t| t.normalized), "{r:?}");
    // The long turn came back in pieces of at most 12 words.
    assert!(r.iter().all(|t| t.len <= 12));
}

#[test]
fn an_unanswerable_turn_is_raw_and_tagged_too_long() {
    let w = words(&["a", "b", "c", "d", "e"]);
    let fake = Fake {
        too_long_words: 0,
        ..Fake::good()
    };
    let (r, _) = run(&fake, &w, None);
    assert_eq!(r.len(), 1);
    assert!(!r[0].normalized);
    assert_eq!(r[0].reason.as_deref(), Some("too_long"));
}

#[test]
fn words_that_are_only_punctuation_keep_their_place() {
    let w = words(&["hello", "-", "world"]);
    let fake = Fake::good();
    let (r, _) = run(&fake, &w, None);
    assert!(r[0].normalized);
    assert_eq!(r[0].display.len(), 3);
    assert_eq!(r[0].display[1], "-");
    assert_eq!(r[0].display[0], "Hello");
}

#[test]
fn paragraph_breaks_map_back_to_word_indexes() {
    struct Para;
    impl TextModel for Para {
        fn complete_json(&self, req: &JsonRequest) -> Result<JsonReply, TextModelError> {
            let items = items_of(req);
            let out: Vec<String> = items
                .iter()
                .map(|i| {
                    let w: Vec<&str> = i.split(' ').collect();
                    format!("{}.\n{}", w[..2].join(" "), w[2..].join(" "))
                })
                .collect();
            Ok(JsonReply {
                value: json!({ "turns": out }),
                attempts: 1,
            })
        }
    }
    // The third word is only punctuation: it is not sent, and the break lands on the word after.
    let w = words(&["a", "b", "--", "c", "d"]);
    let (r, _) = run(&Para, &w, None);
    assert!(r[0].normalized, "{r:?}");
    assert_eq!(r[0].breaks, vec![3]);
}

#[test]
fn cancelling_stops_the_run() {
    let w = words(&["a", "b"]);
    let opts = Options::new("en", 8192);
    let e = normalize(
        &Fake::good(),
        &w,
        None,
        &opts,
        Vec::new(),
        &mut |_| {},
        &mut |_| false,
    )
    .unwrap_err();
    assert!(matches!(e, NormalizeError::Cancelled));
}

#[test]
fn an_unreachable_model_stops_the_run_with_its_error() {
    struct Down;
    impl TextModel for Down {
        fn complete_json(&self, _: &JsonRequest) -> Result<JsonReply, TextModelError> {
            Err(TextModelError {
                kind: ErrorKind::Unreachable,
                message: "down".into(),
            })
        }
    }
    let w = words(&["a", "b"]);
    let opts = Options::new("en", 8192);
    let e = normalize(&Down, &w, None, &opts, Vec::new(), &mut |_| {}, &mut |_| {
        true
    })
    .unwrap_err();
    assert!(matches!(e, NormalizeError::Model(m) if m.reason() == "text_model_unreachable"));
}

#[test]
fn a_run_goes_on_from_its_checkpoint() {
    let texts: Vec<String> = (0..30).map(|i| format!("w{i}")).collect();
    let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
    let w = words(&refs);
    let l: Vec<Option<u32>> = (0..30).map(|i| Some(i as u32 / 10)).collect();
    let opts = Options {
        batch_turns: 1,
        ..Options::new("en", 8192)
    };
    // Stop after the first checkpoint.
    let mut saved: Vec<TurnResult> = Vec::new();
    let stop = std::cell::Cell::new(false);
    let first = Fake::good();
    let r = normalize(
        &first,
        &w,
        Some(&l),
        &opts,
        Vec::new(),
        &mut |c| {
            saved = c.to_vec();
            stop.set(true);
        },
        &mut |_| !stop.get(),
    );
    assert!(matches!(r, Err(NormalizeError::Cancelled)));
    assert_eq!(saved.len(), 1);
    // The second run asks for the two turns that are left only.
    let second = Fake::good();
    let (all, _) = normalize(
        &second,
        &w,
        Some(&l),
        &opts,
        saved,
        &mut |_| {},
        &mut |_| true,
    )
    .unwrap();
    assert_eq!(all.len(), 3);
    assert_eq!(second.calls.lock().unwrap().len(), 2);
    assert_eq!(all.iter().map(|t| t.len).sum::<usize>(), 30);
    // A checkpoint that does not end on a turn of this run is dropped.
    let third = Fake::good();
    let bogus = vec![TurnResult {
        first: 0,
        len: 3,
        speaker: None,
        normalized: true,
        reason: None,
        display: vec!["x".into(); 3],
        glued: vec![],
        breaks: vec![],
    }];
    let (all, _) = normalize(&third, &w, Some(&l), &opts, bogus, &mut |_| {}, &mut |_| {
        true
    })
    .unwrap();
    assert_eq!(third.calls.lock().unwrap().len(), 3);
    assert_eq!(all.len(), 3);
}

#[test]
fn the_context_before_a_batch_is_the_end_of_the_turn_before() {
    let w = words(&["a", "b", "c", "d"]);
    let l = labels(&[1, 1, 2, 2]);
    let opts = Options {
        batch_turns: 1,
        ..Options::new("en", 8192)
    };
    let fake = Fake::good();
    normalize(
        &fake,
        &w,
        Some(&l),
        &opts,
        Vec::new(),
        &mut |_| {},
        &mut |_| true,
    )
    .unwrap();
    let prompts = fake.prompts.lock().unwrap();
    assert!(!prompts[0].contains("just before"));
    // The model's own punctuated words are the context of the next call.
    assert!(prompts[1].contains("A b."));
}

#[test]
fn the_sidecar_round_trips_and_knows_its_transcript() {
    let w = words(&["xin", "chào", "bạn"]);
    let fake = Fake::good();
    let (r, stats) = run(&fake, &w, None);
    let n = Normalized::from_run(&w, "vi", "gemma", "2026-11-04T10:00:00Z", r, &stats);
    assert_eq!(n.summary.normalized, 1);
    assert_eq!(n.text(), "Xin chào bạn.");
    assert_eq!(n.display_words(), vec!["Xin", "chào", "bạn."]);
    assert!(n.matches(&w));
    // Case and punctuation do not matter; a changed word does.
    let mut same = w.clone();
    same[0].text = "Xin,".into();
    assert!(n.matches(&same));
    same[1].text = "chao".into();
    assert!(!n.matches(&same));
    assert!(!n.matches(&w[..2]));

    let dir = tempfile::tempdir().unwrap();
    assert!(read(dir.path()).unwrap().is_none());
    std::fs::write(partial_path(dir.path()), b"{}").unwrap();
    write(dir.path(), &n).unwrap();
    assert!(!partial_path(dir.path()).exists());
    assert_eq!(read(dir.path()).unwrap().unwrap(), n);
    // transcript.json is not part of this and never written here.
    assert!(!dir.path().join("transcript.json").exists());
}

#[test]
fn text_keeps_breaks_and_glued_words() {
    let n = Normalized {
        version: 1,
        language: "en".into(),
        model: "m".into(),
        prompt_version: 1,
        created: String::new(),
        words: 5,
        words_hash: String::new(),
        summary: Summary::default(),
        turns: vec![NormalizedTurn {
            start: MediaTime::ZERO,
            end: MediaTime::ZERO,
            result: TurnResult {
                first: 0,
                len: 5,
                speaker: None,
                normalized: true,
                reason: None,
                display: ["Twenty-", "one", "days.", "Then", "stop."]
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
                glued: vec![1],
                breaks: vec![3],
            },
        }],
    };
    assert_eq!(n.text(), "Twenty-one days.\nThen stop.");
}

#[test]
fn one_bad_word_keeps_only_a_small_part_raw() {
    let texts: Vec<String> = (0..40).map(|i| format!("w{i}")).collect();
    let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
    let w = words(&refs);
    // "w25" is always damaged: the halves that do not contain it come back clean.
    let fake = Fake {
        break_on: Some(("w25", "w26", true)),
        ..Fake::good()
    };
    let (r, stats) = run(&fake, &w, None);
    assert_eq!(r.iter().map(|t| t.len).sum::<usize>(), 40);
    let raw: usize = r.iter().filter(|t| !t.normalized).map(|t| t.len).sum();
    assert!(raw > 0 && raw < 12, "{raw}");
    assert!(r.iter().filter(|t| t.normalized).count() >= 3);
    assert!(stats.bisected >= 2);
    // The rejected leaf has its raw words; every accepted word kept its text.
    for t in &r {
        for (k, d) in t.display.iter().enumerate() {
            assert!(d
                .to_lowercase()
                .trim_end_matches('.')
                .contains(&format!("w{}", t.first + k)));
        }
    }
}
