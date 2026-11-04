//! Benchmark of the transcript normalisation (AI-10, release 0.2.7, W8). Prints one JSON line.
//! Not shipped. It runs a real `llama-server` and a real model that you point at (it never
//! downloads either), so ask before running it.
//!
//! ```text
//! cargo run --release -p cm-ai --example normalize_bench -- --model <gguf> --model-id <id>
//!     --llama-server <path> --lang vi|en [--ctx 16384] [--label <name>] [--log <file>]
//!     [--batch-words 220] [--batch-turns 12]
//!     ( --refs <sentences.txt> [--group 4] [--limit 120]       one reference sentence per line
//!     | --lines <name.lines.json> [--limit 60]                 turns with punctuation (`text`, `speaker`)
//!     | --bundle <copy of a .cmrec> [--seconds 300] [--dump <file>] )   a real transcript: counts only
//! ```
//!
//! With `--refs` and `--lines` the punctuation and case of the reference are removed, the pass
//! runs on the words, and the result is scored against the reference ([`cm_ai::normalize::score`]).
//! With `--bundle` there is no reference: the rates of the invariant and the time are reported
//! (and `--dump` writes the normalised text to a file for the person running it; nothing from a
//! private recording belongs in a report).

#![allow(clippy::unwrap_used)]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use cm_ai::normalize::invariant::word_key;
use cm_ai::normalize::score::Tally;
use cm_ai::normalize::{normalize, Normalized, Options};
use cm_ai::text_model::engine::{EmbeddedTextModel, EngineManager, EngineSpec, TextEngine};
use cm_ai::text_model::{JsonReply, JsonRequest, TextModel, TextModelConfig, TextModelError};
use cm_ai::transcribe::{Transcript, Word};
use cm_core::MediaTime;
use serde_json::{json, Value};

fn option(args: &[String], flag: &str) -> Option<String> {
    let i = args.iter().position(|a| a == flag)?;
    args.get(i + 1).cloned()
}

/// Counts calls and times each one; passes everything through.
struct Timed<'a> {
    inner: &'a dyn TextModel,
    calls: std::sync::Mutex<Vec<(f64, usize)>>,
    /// `--trace`: every request's input and answer, one JSON line each (public data only).
    trace: Option<std::sync::Mutex<std::fs::File>>,
}

impl TextModel for Timed<'_> {
    fn complete_json(&self, req: &JsonRequest) -> Result<JsonReply, TextModelError> {
        self.complete_json_streaming(req, &mut |_| true)
    }

    fn complete_json_streaming(
        &self,
        req: &JsonRequest,
        on_progress: &mut dyn FnMut(usize) -> bool,
    ) -> Result<JsonReply, TextModelError> {
        let began = Instant::now();
        let mut pieces = 0usize;
        let r = self.inner.complete_json_streaming(req, &mut |n| {
            pieces = n;
            on_progress(n)
        });
        self.calls
            .lock()
            .unwrap()
            .push((began.elapsed().as_secs_f64(), pieces));
        if let Some(f) = &self.trace {
            use std::io::Write;
            let line = json!({
                "secs": began.elapsed().as_secs_f64(), "pieces": pieces,
                "prompt": req.messages[0].content,
                "answer": r.as_ref().map(|x| x.value.clone()).map_err(|e| e.message.clone()),
            });
            let _ = writeln!(f.lock().unwrap(), "{line}");
        }
        r
    }
}

/// The word as the pass gets it: lower case, edges stripped of punctuation, inner characters
/// (hyphen, apostrophe) kept.
fn plain(token: &str) -> String {
    token
        .to_lowercase()
        .trim_matches(|c: char| !c.is_alphanumeric())
        .to_owned()
}

struct Case {
    /// Reference tokens, one per input word, grouped by turn.
    turns: Vec<Vec<String>>,
}

fn word(i: usize, text: &str, gap: f64) -> Word {
    let start = i as f64 * 0.4 + gap;
    Word {
        text: text.to_owned(),
        start: MediaTime::from_secs_f64(start).unwrap(),
        end: MediaTime::from_secs_f64(start + 0.3).unwrap(),
        confidence: 0.9,
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let lang = option(&args, "--lang").unwrap_or_else(|| "en".into());
    let limit: usize = option(&args, "--limit")
        .and_then(|c| c.parse().ok())
        .unwrap_or(usize::MAX);

    // ---- the input
    let mut words: Vec<Word> = Vec::new();
    let mut labels: Vec<Option<u32>> = Vec::new();
    let mut case = Case { turns: Vec::new() };
    let mut bundle_labels = false;
    if let Some(path) = option(&args, "--refs") {
        let group: usize = option(&args, "--group")
            .and_then(|c| c.parse().ok())
            .unwrap_or(4);
        let sentences: Vec<String> = std::fs::read_to_string(path)
            .unwrap()
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(str::to_owned)
            .take(limit.saturating_mul(group))
            .collect();
        for (t, chunk) in sentences.chunks(group).enumerate() {
            let tokens: Vec<String> = chunk
                .iter()
                .flat_map(|s| s.split_whitespace().map(str::to_owned))
                .filter(|t| !word_key(t).is_empty())
                .collect();
            for tok in &tokens {
                words.push(word(words.len(), &plain(tok), 0.0));
                labels.push(Some(t as u32));
            }
            case.turns.push(tokens);
        }
    } else if let Some(path) = option(&args, "--lines") {
        let raw: Vec<Value> =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let mut ids: Vec<String> = Vec::new();
        for (t, v) in raw.iter().take(limit).enumerate() {
            let sp = v["speaker"].as_str().unwrap_or("").to_owned();
            if !ids.contains(&sp) {
                ids.push(sp.clone());
            }
            let _ = t;
            let id = ids.iter().position(|x| *x == sp).unwrap() as u32;
            let tokens: Vec<String> = v["text"]
                .as_str()
                .unwrap()
                .split_whitespace()
                .map(str::to_owned)
                .filter(|t| !word_key(t).is_empty())
                .collect();
            for tok in &tokens {
                words.push(word(words.len(), &plain(tok), 0.0));
                labels.push(Some(id));
            }
            case.turns.push(tokens);
        }
    } else if let Some(dir) = option(&args, "--bundle") {
        let dir = PathBuf::from(dir);
        let t: Transcript =
            serde_json::from_str(&std::fs::read_to_string(dir.join("transcript.json")).unwrap())
                .unwrap();
        let secs: f64 = option(&args, "--seconds")
            .and_then(|c| c.parse().ok())
            .unwrap_or(f64::MAX);
        words = t
            .words
            .into_iter()
            .take_while(|w| w.end.as_secs_f64() <= secs)
            .collect();
        let manifest = cm_library::read_manifest(&dir).ok();
        if let Some(state) = manifest
            .as_ref()
            .and_then(cm_ai::speakers::SpeakerState::from_manifest)
        {
            labels = cm_ai::speakers::assign_words(&words, &state.segments)
                .iter()
                .map(|l| l.speaker)
                .collect();
            bundle_labels = true;
        }
    } else {
        panic!("one of --refs, --lines, --bundle");
    }
    let speakers = (!labels.is_empty() && (bundle_labels || !case.turns.is_empty()))
        .then_some(labels.as_slice());

    // ---- the engine
    let model_path = PathBuf::from(option(&args, "--model").expect("--model <gguf>"));
    let model_id = option(&args, "--model-id").expect("--model-id");
    let ctx: u32 = option(&args, "--ctx")
        .and_then(|c| c.parse().ok())
        .unwrap_or(16384);
    let label = option(&args, "--label").unwrap_or_else(|| "run".into());
    let log = PathBuf::from(
        option(&args, "--log").unwrap_or_else(|| "normalize_bench_engine.log".into()),
    );
    std::env::set_var(
        "CM_LLAMA_SERVER",
        option(&args, "--llama-server").expect("--llama-server"),
    );
    let engine: Arc<dyn TextEngine> = EngineManager::system(log.clone());
    let spec = EngineSpec {
        model_id: model_id.clone(),
        model_path,
        ram_mb: 0,
        context_tokens: ctx,
    };
    let embedded = EmbeddedTextModel::new(engine.clone(), spec, &TextModelConfig::default());
    let started = Instant::now();
    embedded
        .complete_json(&JsonRequest::new(
            "Reply with the JSON object {\"ok\": true}.",
            json!({"type":"object","properties":{"ok":{"type":"boolean"}},"required":["ok"],"additionalProperties":false}),
        ))
        .unwrap();
    let start_secs = started.elapsed().as_secs_f64();
    let timed = Timed {
        inner: &embedded,
        calls: Default::default(),
        trace: option(&args, "--trace")
            .map(|p| std::sync::Mutex::new(std::fs::File::create(p).unwrap())),
    };

    let mut opts = Options::new(&lang, ctx as usize);
    if let Some(n) = option(&args, "--batch-words").and_then(|c| c.parse().ok()) {
        opts.batch_words = n;
    }
    if let Some(n) = option(&args, "--batch-turns").and_then(|c| c.parse().ok()) {
        opts.batch_turns = n;
    }

    let audio_secs = words.last().map_or(0.0, |w| w.end.as_secs_f64());
    let began = Instant::now();
    let mut progress_calls = 0usize;
    let result = normalize(
        &timed,
        &words,
        speakers,
        &opts,
        Vec::new(),
        &mut |_| {},
        &mut |_| {
            progress_calls += 1;
            true
        },
    );
    let wall = began.elapsed().as_secs_f64();
    let (results, stats) = match result {
        Ok(r) => r,
        Err(e) => {
            engine.stop();
            println!("{}", json!({"label": label, "error": format!("{e:?}")}));
            return;
        }
    };
    let n = Normalized::from_run(&words, &lang, &model_id, "", results, &stats);

    // ---- the numbers
    let display = n.display_words();
    // Tone-mark damage, counted with a different code path from the check: every accepted word
    // must have the same letters, tone marks and đ as the input word, ignoring case.
    let mut damaged = 0usize;
    let mut accepted_words = 0usize;
    let mut at = 0usize;
    let mut reasons: std::collections::BTreeMap<String, usize> = Default::default();
    for t in &n.turns {
        let r = &t.result;
        if r.normalized {
            accepted_words += r.len;
            for k in 0..r.len {
                let strip = |s: &str| -> String {
                    s.chars()
                        .filter(|c| c.is_alphanumeric() || ('\u{0300}'..='\u{036F}').contains(c))
                        .flat_map(char::to_lowercase)
                        .collect()
                };
                let a: String = strip(&words[at + k].text);
                let b: String = strip(display[at + k]);
                // Compare after composing both (the stored form is NFC).
                if cm_ai::textmatch::nfc(&a) != cm_ai::textmatch::nfc(&b) {
                    damaged += 1;
                }
            }
        } else if let Some(reason) = &r.reason {
            *reasons.entry(reason.clone()).or_default() += 1;
        }
        at += r.len;
    }
    let mut out = json!({
        "label": label, "model": model_id, "lang": lang, "context_tokens": ctx,
        "turns_in": n.turns.len(), "words": words.len(), "audio_secs": audio_secs,
        "server_start_secs": start_secs, "wall_secs": wall,
        "turns": n.summary.turns, "normalized_turns": n.summary.normalized,
        "rejected_turns": n.summary.rejected,
        "rejected_turn_rate": n.summary.rejected as f64 / n.summary.turns.max(1) as f64,
        "rejected_words": n.summary.rejected_words,
        "rejected_word_rate": n.summary.rejected_words as f64 / words.len().max(1) as f64,
        "rejected_by_reason": reasons,
        "model_calls": stats.model_calls, "retried_turns": stats.retried, "bisected": stats.bisected,
        "first_answer_errors": {"substituted": stats.first_substituted,
            "deleted": stats.first_deleted, "inserted": stats.first_inserted},
        "tone_mark_damage_words": damaged, "accepted_words": accepted_words,
        "secs_per_audio_minute": if audio_secs > 0.0 { wall / (audio_secs / 60.0) } else { 0.0 },
    });
    if !case.turns.is_empty() {
        let mut all = Tally::default();
        let mut ok_only = Tally::default();
        let mut raw = Tally::default();
        let mut pos = 0usize;
        let mut turn_idx = 0usize;
        // Pieces tile the words in order; walk the reference turns over them.
        for (ti, reference) in case.turns.iter().enumerate() {
            let hyp: Vec<String> = display[pos..pos + reference.len()]
                .iter()
                .map(|s| s.to_string())
                .collect();
            let plain_in: Vec<String> = words[pos..pos + reference.len()]
                .iter()
                .map(|w| w.text.clone())
                .collect();
            all.add(reference, &hyp);
            raw.add(reference, &plain_in);
            // The turn counts as accepted when every piece covering it was.
            let mut covered = 0usize;
            let mut good = true;
            while covered < reference.len() {
                let r = &n.turns[turn_idx].result;
                good &= r.normalized;
                covered += r.len;
                turn_idx += 1;
            }
            if good {
                ok_only.add(reference, &hyp);
            }
            pos += reference.len();
            let _ = ti;
        }
        let prf = |p: &cm_ai::normalize::score::Prf| {
            json!({"precision": p.precision(), "recall": p.recall(), "f1": p.f1(),
                   "tp": p.tp, "fp": p.fp, "fn": p.fn_})
        };
        let sc = |t: &Tally| {
            json!({"words": t.words, "comma": prf(&t.comma), "period": prf(&t.period),
                   "question": prf(&t.question), "boundary": prf(&t.boundary),
                   "cap_accuracy": t.cap_accuracy(), "capitals": prf(&t.capitals)})
        };
        out["score_all_turns"] = sc(&all);
        out["score_accepted_turns"] = sc(&ok_only);
        out["score_no_pass"] = sc(&raw);
    }
    if let Some(p) = option(&args, "--dump") {
        std::fs::write(p, n.text()).unwrap();
    }
    let calls = timed.calls.lock().unwrap().clone();
    out["calls"] = json!(calls.len());
    out["streamed_pieces_total"] = json!(calls.iter().map(|c| c.1).sum::<usize>());
    out["call_secs"] = json!(calls.iter().map(|c| c.0).sum::<f64>());
    engine.stop();
    println!("{out}");
}
