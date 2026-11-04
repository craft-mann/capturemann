//! Parameter search for the Whisper decode (release 0.2.7, W3). One model load, many cells: each
//! cell is a [`DecodeParams`] spec, each set a clip with its language and (optionally) a reference
//! text and speech spans. The run is the app's own: `transcribe_pieces` over the quiet-input lift
//! over the model. Prints one JSON line per (cell, set) to stdout (and appends to `--out`).
//!
//! ```text
//! cargo run --release -p cm-ai --example param_search -- --model <ggml> --model-id large-v3-turbo
//!     --sets sets.json --cells cells.tsv [--out results.jsonl] [--vad <ggml silero>] [--no-lift]
//!     [--only-sets a,b] [--only-cells x,y]
//! ```
//!
//! `sets.json`: `[{"label", "audio", "language", "reference"?, "speech"?, "rttm"?, "diar"?,
//! "signals"?}]`. `speech` is a JSON list of `[start,end]` spans; `rttm` a reference RTTM file;
//! `diar` a JSON list of `[start,end]` speaker-engine spans in seconds. With `signals: true` the
//! reference-silent stretches are those of at least 3 s where energy, Silero and the speaker engine
//! (`diar`, when given) all say there is no speech (definition in `silent_spans`). `cells.tsv`:
//! `name<TAB>spec` per line (`#` comments), the spec as `DecodeParams::parse` reads it.
//!
//! WER-exact keeps tone marks and `đ` (NFC, case and punctuation folded); `wer_fold` is the old
//! `measure::word_errors` (diacritics dropped). The exact figure is computed here, not in
//! `measure.rs`, so the two can be compared until the files are unified.

#![allow(clippy::unwrap_used)]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

use cm_ai::audio::decode_16k_mono;
use cm_ai::failure_kinds::{dense_unit_indices, sentence_runs, word_coverage};
use cm_ai::measure::{word_errors, WordErrors};
use cm_ai::pieces::transcribe_pieces;
use cm_ai::pipeline::{Configured, Normalize};
use cm_ai::silero::{SileroVad, FRAME};
use cm_ai::transcribe::{Pcm16k, Transcriber, Transcript};
use cm_ai::whisper::{DecodeParams, WhisperTranscriber};
use cm_core::{CmError, MediaTime};
use serde_json::{json, Value};
use sha2::Digest;

fn option(args: &[String], flag: &str) -> Option<String> {
    let i = args.iter().position(|a| a == flag)?;
    args.get(i + 1).cloned()
}

/// Counts the Whisper calls and the audio they read.
struct Counting<'a> {
    inner: &'a WhisperTranscriber,
    calls: Mutex<Vec<(f64, f64)>>,
}

impl Transcriber for Counting<'_> {
    fn transcribe(
        &self,
        audio: Pcm16k<'_>,
        language: Option<&str>,
        progress: &mut dyn FnMut(f32) -> bool,
    ) -> Result<Transcript, CmError> {
        self.transcribe_prompted(audio, language, None, progress)
    }

    fn transcribe_prompted(
        &self,
        audio: Pcm16k<'_>,
        language: Option<&str>,
        prompt: Option<&str>,
        progress: &mut dyn FnMut(f32) -> bool,
    ) -> Result<Transcript, CmError> {
        let secs = audio.samples.len() as f64 / 16_000.0;
        let began = Instant::now();
        let r = self
            .inner
            .transcribe_prompted(audio, language, prompt, progress);
        self.calls
            .lock()
            .unwrap()
            .push((secs, began.elapsed().as_secs_f64()));
        r
    }
}

/// Words as scored exactly: NFC, lower case, punctuation dropped, tone marks and `đ` kept.
fn exact_words(text: &str) -> Vec<String> {
    let nfc = icu_normalizer::ComposingNormalizerBorrowed::new_nfc();
    nfc.normalize(text)
        .split_whitespace()
        .map(|w| {
            w.chars()
                .filter(|c| c.is_alphanumeric())
                .flat_map(char::to_lowercase)
                .collect::<String>()
        })
        .filter(|w| !w.is_empty())
        .collect()
}

fn exact_errors(reference: &str, hypothesis: &str) -> WordErrors {
    let r = exact_words(reference);
    let h = exact_words(hypothesis);
    let (n, m) = (r.len(), h.len());
    let mut cost = vec![vec![0usize; m + 1]; n + 1];
    for (i, row) in cost.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, c) in cost[0].iter_mut().enumerate() {
        *c = j;
    }
    for i in 1..=n {
        for j in 1..=m {
            let same = usize::from(r[i - 1] != h[j - 1]);
            cost[i][j] = (cost[i - 1][j - 1] + same)
                .min(cost[i - 1][j] + 1)
                .min(cost[i][j - 1] + 1);
        }
    }
    let mut out = WordErrors {
        reference_words: n,
        ..WordErrors::default()
    };
    let (mut i, mut j) = (n, m);
    while i > 0 || j > 0 {
        if i > 0 && j > 0 && cost[i][j] == cost[i - 1][j - 1] + usize::from(r[i - 1] != h[j - 1]) {
            if r[i - 1] != h[j - 1] {
                out.substitutions += 1;
            }
            i -= 1;
            j -= 1;
        } else if i > 0 && cost[i][j] == cost[i - 1][j] + 1 {
            out.deletions += 1;
            i -= 1;
        } else {
            out.insertions += 1;
            j -= 1;
        }
    }
    out
}

fn spans_json(path: &str) -> Vec<(f64, f64)> {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn rttm_spans(path: &str) -> Vec<(f64, f64)> {
    let mut v: Vec<(f64, f64)> = std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            (f.first() == Some(&"SPEAKER") && f.len() > 4).then(|| {
                let a: f64 = f[3].parse().unwrap();
                (a, a + f[4].parse::<f64>().unwrap())
            })
        })
        .collect();
    v.sort_by(|a, b| a.0.total_cmp(&b.0));
    v
}

/// Reference-silent stretches by signals, not by words: 100 ms frames where (1) the frame energy
/// is under `floor + 0.3 * (p90 - floor)` (floor = 10th percentile of the frame levels, p90 = 90th),
/// (2) Silero's probability is under 0.32 for the whole frame, and (3) no speaker-engine span
/// (widened by 0.5 s) covers it, in runs of at least 3 s, trimmed by 0.25 s at each end.
fn silent_spans(samples: &[f32], vad: &SileroVad, diar: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let hop = 1600;
    let frames = samples.len() / hop;
    let db: Vec<f64> = (0..frames)
        .map(|k| {
            let s = &samples[k * hop..(k + 1) * hop];
            let r = (s.iter().map(|x| f64::from(*x).powi(2)).sum::<f64>() / hop as f64).sqrt();
            20.0 * r.max(1e-9).log10()
        })
        .collect();
    let mut sorted = db.clone();
    sorted.sort_by(f64::total_cmp);
    let (floor, p90) = (sorted[frames / 10], sorted[frames * 9 / 10]);
    let limit = floor + 0.3 * (p90 - floor);
    let probs = vad.probabilities(samples).unwrap();
    let silent: Vec<bool> = (0..frames)
        .map(|k| {
            let (a, b) = (k as f64 * 0.1, (k + 1) as f64 * 0.1);
            let (fa, fb) = (
                (a * 16_000.0 / FRAME as f64) as usize,
                ((b * 16_000.0 / FRAME as f64).ceil() as usize).min(probs.len()),
            );
            let vad_speech = probs[fa.min(probs.len())..fb].iter().any(|p| *p >= 0.32);
            let diar_speech = diar.iter().any(|&(x, y)| b > x - 0.5 && a < y + 0.5);
            db[k] < limit && !vad_speech && !diar_speech
        })
        .collect();
    let mut out = Vec::new();
    let mut k = 0;
    while k < frames {
        if !silent[k] {
            k += 1;
            continue;
        }
        let a = k;
        while k < frames && silent[k] {
            k += 1;
        }
        let (x, y) = (a as f64 * 0.1, k as f64 * 0.1);
        if y - x >= 3.0 {
            out.push((x + 0.25, y - 0.25));
        }
    }
    out
}

fn mid(w: &cm_ai::transcribe::Word) -> f64 {
    (w.start.ticks() + w.end.ticks()) as f64 / 2.0 / cm_core::TICKS_PER_SECOND as f64
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let model = PathBuf::from(option(&args, "--model").expect("--model"));
    let model_id = option(&args, "--model-id").expect("--model-id");
    let sets: Vec<Value> =
        serde_json::from_str(&std::fs::read_to_string(option(&args, "--sets").unwrap()).unwrap())
            .unwrap();
    let only_sets: Option<Vec<String>> =
        option(&args, "--only-sets").map(|s| s.split(',').map(str::to_owned).collect());
    let only_cells: Option<Vec<String>> =
        option(&args, "--only-cells").map(|s| s.split(',').map(str::to_owned).collect());
    let cells: Vec<(String, String)> =
        std::fs::read_to_string(option(&args, "--cells").expect("--cells"))
            .unwrap()
            .lines()
            .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
            .map(|l| {
                let (n, s) = l.split_once('\t').unwrap_or((l, ""));
                (n.trim().to_owned(), s.trim().to_owned())
            })
            .filter(|(n, _)| only_cells.as_ref().is_none_or(|o| o.contains(n)))
            .collect();
    let out_path = option(&args, "--out");
    let lift = !args.iter().any(|a| a == "--no-lift");
    let vad = option(&args, "--vad").map(|m| SileroVad::load(Path::new(&m)).unwrap());

    // Load every set once; compute its signal-silent stretches once.
    struct Set {
        label: String,
        language: String,
        samples: Vec<f32>,
        reference: Option<String>,
        speech: Vec<(f64, f64)>,
        silent: Option<Vec<(f64, f64)>>,
    }
    let mut loaded = Vec::new();
    for s in &sets {
        let label = s["label"].as_str().unwrap().to_owned();
        if only_sets.as_ref().is_some_and(|o| !o.contains(&label)) {
            continue;
        }
        let samples = decode_16k_mono(Path::new(s["audio"].as_str().unwrap())).unwrap();
        let speech = if let Some(p) = s["speech"].as_str() {
            spans_json(p)
        } else if let Some(p) = s["rttm"].as_str() {
            rttm_spans(p)
        } else {
            Vec::new()
        };
        let silent = (s["signals"].as_bool() == Some(true)).then(|| {
            let diar = s["diar"].as_str().map(spans_json).unwrap_or_default();
            silent_spans(
                &samples,
                vad.as_ref().expect("--vad is needed for signal sets"),
                &diar,
            )
        });
        loaded.push(Set {
            label,
            language: s["language"].as_str().unwrap_or("en").to_owned(),
            reference: s["reference"]
                .as_str()
                .map(|p| std::fs::read_to_string(p).unwrap().trim().to_owned()),
            samples,
            speech,
            silent,
        });
    }

    let mut whisper = WhisperTranscriber::load(&model, &model_id, true).unwrap();
    let mut out_file = out_path.as_ref().map(|p| {
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(p)
            .unwrap()
    });
    for (cell, spec) in &cells {
        whisper.set_decode(DecodeParams::parse(spec).unwrap());
        for set in &loaded {
            let audio_secs = set.samples.len() as f64 / 16_000.0;
            let counting = Counting {
                inner: &whisper,
                calls: Mutex::new(Vec::new()),
            };
            let configured = Configured {
                inner: &counting,
                vad: None,
                gate: None,
                normalize: lift.then(Normalize::default),
            };
            let began = Instant::now();
            let (tr, report) = transcribe_pieces(
                &configured,
                Pcm16k {
                    samples: &set.samples,
                    start: MediaTime::ZERO,
                },
                Some(&set.language),
                None,
                &mut |_| true,
            )
            .unwrap();
            let secs = began.elapsed().as_secs_f64();
            let calls = counting.calls.into_inner().unwrap();
            let text = tr.text();
            let mut row = json!({
                "model": model_id, "cell": cell, "spec": spec, "set": set.label,
                "audio_secs": audio_secs, "transcribe_secs": secs, "rtf": audio_secs / secs,
                "words": tr.words.len(), "whisper_calls": calls.len(),
                "audio_read_secs": calls.iter().map(|c| c.0).sum::<f64>(),
                "call_secs_total": calls.iter().map(|c| c.1).sum::<f64>(),
                "loop_runs": sentence_runs(&tr.words, 3).len(),
                "dense_repeat_words": dense_unit_indices(&tr.words, 5, 60.0).len(),
                "pieces": report.pieces, "split": report.split, "repeats_dropped": report.repeats_dropped,
                "text_sha": format!("{:x}", sha2::Sha256::digest(text.as_bytes()))[..12].to_owned(),
                "peak_rss_mb": cm_core::perf::peak_rss_bytes().map(|b| b / 1_000_000),
                "lift": lift,
            });
            if let Some(r) = &set.reference {
                let e = exact_errors(r, &text);
                let f = word_errors(r, &text);
                row["reference_words"] = json!(e.reference_words);
                row["wer_exact"] = json!(e.wer());
                row["ins"] = json!(e.insertions);
                row["del"] = json!(e.deletions);
                row["sub"] = json!(e.substitutions);
                row["wer_fold"] = json!(f.wer());
            }
            if !set.speech.is_empty() {
                let outside = tr
                    .words
                    .iter()
                    .filter(|w| {
                        let m = mid(w);
                        !set.speech.iter().any(|&(a, b)| m > a - 0.5 && m < b + 0.5)
                    })
                    .count();
                row["outside_speech_words"] = json!(outside);
                let c = word_coverage(&set.speech, &tr.words, 1.0, 5.0);
                row["coverage"] = json!(c.ratio());
                row["uncovered_secs"] = json!(c.uncovered_secs());
            }
            if let Some(sil) = &set.silent {
                let inside: Vec<f64> = tr
                    .words
                    .iter()
                    .map(mid)
                    .filter(|m| sil.iter().any(|&(a, b)| *m >= a && *m <= b))
                    .collect();
                // Units: words in a silent stretch less than 2 s apart; "invented-like" = 3+ words.
                let mut units = 0;
                let mut run = 0;
                let mut last = f64::NEG_INFINITY;
                for m in &inside {
                    if m - last > 2.0 {
                        if run >= 3 {
                            units += 1;
                        }
                        run = 0;
                    }
                    run += 1;
                    last = *m;
                }
                if run >= 3 {
                    units += 1;
                }
                let silent_secs: f64 = sil.iter().map(|(a, b)| b - a).sum();
                row["silent_secs"] = json!(silent_secs);
                row["silent_spans"] = json!(sil.len());
                row["silent_words"] = json!(inside.len());
                row["silent_units"] = json!(units);
                row["silent_units_per_hour"] = json!(units as f64 / (audio_secs / 3600.0));
            }
            let line = row.to_string();
            println!("{line}");
            if let Some(f) = out_file.as_mut() {
                writeln!(f, "{line}").unwrap();
            }
        }
    }
}
