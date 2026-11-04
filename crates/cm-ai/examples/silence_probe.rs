//! Insertion-over-silence metric (0.2.7 transcription quality plan, W4). Not shipped; never
//! downloads anything. For a transcript and a list of reference-silent spans it reports the words
//! (and the "units": runs of such words less than 1 s apart) written over silence, in total and
//! per hour of silent audio. Prints one JSON line per clip and a last `summary` line.
//!
//! Decode a set made by `tests/tools/make_silence_set.py` (the app's own path: `transcribe_pieces`
//! over `pipeline::Configured`, the quiet-input lift on like the shipped default; the model is
//! loaded once):
//!
//! ```text
//! cargo run --release -p cm-ai --example silence_probe -- --dir <set dir> --model <ggml file>
//!     --model-id <id> [--language vi] [--lift on|off] [--prompt <text>] [--gpu on|off]
//!     [--only <name substring>] [--show-text] [--out <jsonl>] [--save-dir <dir>]
//! ```
//!
//! Count from an existing transcript (a bundle folder or a transcript.json) and a span file:
//!
//! ```text
//! cargo run --release -p cm-ai --example silence_probe -- --transcript <bundle|transcript.json>
//!     (--silent <spans.json> | --speech <speech.json> --secs N) [--guard 0.5]
//! ```
//!
//! `--silent` is a JSON list of `[start, end]` reference-silent spans; `--speech` a list of speech
//! spans, whose complement (each span widened by `--guard`, default 0.5 s, so a word that starts a
//! little early or ends late is not counted) is the silence. A word counts when its middle lies in
//! a silent span. Text is printed only with `--show-text` (synthetic sets); counts and times
//! otherwise, which is what the private meetings need.

#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::time::Instant;

use cm_ai::audio::decode_16k_mono;
use cm_ai::pieces::transcribe_pieces;
use cm_ai::transcribe::{Pcm16k, Transcriber, Word};
use cm_ai::whisper::WhisperTranscriber;
use cm_core::{MediaTime, TICKS_PER_SECOND};
use serde_json::json;

fn option(args: &[String], flag: &str) -> Option<String> {
    let i = args.iter().position(|a| a == flag)?;
    args.get(i + 1).cloned()
}

fn secs(t: MediaTime) -> f64 {
    t.ticks() as f64 / TICKS_PER_SECOND as f64
}

/// Merge overlapping spans, sorted.
fn union(mut v: Vec<(f64, f64)>) -> Vec<(f64, f64)> {
    v.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let mut out: Vec<(f64, f64)> = Vec::new();
    for (a, b) in v {
        match out.last_mut() {
            Some(l) if a <= l.1 => l.1 = l.1.max(b),
            _ => out.push((a, b)),
        }
    }
    out
}

/// The silence that is left of `total` seconds when the speech spans, each widened by `guard`, go.
fn complement(speech: &[(f64, f64)], guard: f64, total: f64) -> Vec<(f64, f64)> {
    let wide = union(
        speech
            .iter()
            .map(|&(a, b)| ((a - guard).max(0.0), (b + guard).min(total)))
            .collect(),
    );
    let mut out = Vec::new();
    let mut at = 0.0;
    for (a, b) in wide {
        if a > at {
            out.push((at, a));
        }
        at = at.max(b);
    }
    if at < total {
        out.push((at, total));
    }
    out
}

struct Counts {
    silent_secs: f64,
    words: usize,
    units: usize,
    spans: Vec<(f64, f64, usize)>,
}

/// Words whose middle is in a silent span, and the runs they form.
fn count(words: &[Word], silent: &[(f64, f64)]) -> (Counts, Vec<usize>) {
    let mut hit = Vec::new();
    for (i, w) in words.iter().enumerate() {
        // A token with no letter or digit (a lone full stop) is punctuation, not written text.
        if !w.text.chars().any(char::is_alphanumeric) {
            continue;
        }
        let mid = (secs(w.start) + secs(w.end)) / 2.0;
        if silent.iter().any(|&(a, b)| mid >= a && mid < b) {
            hit.push(i);
        }
    }
    let mut spans: Vec<(f64, f64, usize)> = Vec::new();
    let mut last_end = f64::NEG_INFINITY;
    for &i in &hit {
        let (s, e) = (secs(words[i].start), secs(words[i].end));
        match spans.last_mut() {
            Some(sp) if s - last_end < 1.0 => {
                sp.1 = e;
                sp.2 += 1;
            }
            _ => spans.push((s, e, 1)),
        }
        last_end = e;
    }
    (
        Counts {
            silent_secs: silent.iter().map(|&(a, b)| b - a).sum(),
            words: hit.len(),
            units: spans.len(),
            spans,
        },
        hit,
    )
}

fn report(
    label: &str,
    words: &[Word],
    silent: &[(f64, f64)],
    show_text: bool,
    extra: serde_json::Value,
) -> (serde_json::Value, Counts) {
    let (c, hit) = count(words, silent);
    let per_hour = |n: usize| {
        if c.silent_secs > 0.0 {
            n as f64 / c.silent_secs * 3600.0
        } else {
            0.0
        }
    };
    let mut v = json!({
        "label": label, "words_total": words.len(), "silent_secs": c.silent_secs,
        "words_over_silence": c.words, "units_over_silence": c.units,
        "words_per_silent_hour": per_hour(c.words), "units_per_silent_hour": per_hour(c.units),
        "spans": c.spans.iter().map(|s| json!([s.0, s.1, s.2])).collect::<Vec<_>>(),
    });
    if show_text {
        v["text"] = json!(hit
            .iter()
            .map(|&i| words[i].text.as_str())
            .collect::<Vec<_>>()
            .join(" "));
        v["hits"] = json!(hit
            .iter()
            .map(|&i| json!([secs(words[i].start), secs(words[i].end), words[i].text]))
            .collect::<Vec<_>>());
    }
    if let (Some(o), Some(e)) = (v.as_object_mut(), extra.as_object()) {
        for (k, x) in e {
            o.insert(k.clone(), x.clone());
        }
    }
    (v, c)
}

fn read_spans(p: &Path) -> Vec<(f64, f64)> {
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let show_text = args.iter().any(|a| a == "--show-text");
    let guard: f64 = option(&args, "--guard")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.5);

    // Counting mode.
    if let Some(t) = option(&args, "--transcript") {
        let p = PathBuf::from(t);
        let tr = if p.is_dir() {
            cm_ai::recording::read_transcript(&p).unwrap().unwrap()
        } else {
            serde_json::from_str::<cm_ai::transcribe::Transcript>(
                &std::fs::read_to_string(&p).unwrap(),
            )
            .unwrap()
        };
        let silent = if let Some(s) = option(&args, "--silent") {
            read_spans(Path::new(&s))
        } else {
            let sp = read_spans(Path::new(
                &option(&args, "--speech").expect("--silent or --speech"),
            ));
            let total: f64 = option(&args, "--secs").expect("--secs").parse().unwrap();
            complement(&sp, guard, total)
        };
        let (v, _) = report("transcript", &tr.words, &silent, show_text, json!({}));
        println!("{v}");
        return;
    }

    // Decoding mode.
    let dir = PathBuf::from(option(&args, "--dir").unwrap_or_default());
    let model = PathBuf::from(option(&args, "--model").expect("--model <ggml file>"));
    let model_id = option(&args, "--model-id").expect("--model-id <id>");
    let language = option(&args, "--language").unwrap_or_else(|| "en".into());
    let gpu = option(&args, "--gpu").is_none_or(|g| g != "off");
    let lift = option(&args, "--lift").is_none_or(|g| g != "off");
    let prompt = option(&args, "--prompt");
    let only = option(&args, "--only");
    let out = option(&args, "--out");

    let whisper = WhisperTranscriber::load_with(&model, &model_id, gpu, false).unwrap();
    let configured = cm_ai::pipeline::Configured {
        inner: &whisper,
        vad: None,
        gate: None,
        normalize: lift.then(cm_ai::pipeline::Normalize::default),
    };
    let engine: &dyn Transcriber = &configured;

    // `--audio <file> --save <transcript.json>` (instead of `--dir`): transcribe one file the way the
    // app does, store the transcript, print the pieces report (counts only).
    if let Some(audio) = option(&args, "--audio") {
        let samples = decode_16k_mono(Path::new(&audio)).unwrap();
        let began = Instant::now();
        let (tr, rep) = transcribe_pieces(
            engine,
            Pcm16k {
                samples: &samples,
                start: MediaTime::ZERO,
            },
            Some(&language),
            prompt.as_deref(),
            &mut |_| true,
        )
        .unwrap();
        if let Some(s) = option(&args, "--save") {
            std::fs::write(s, serde_json::to_string(&tr).unwrap()).unwrap();
        }
        println!(
            "{}",
            json!({"audio_secs": samples.len() as f64 / 16_000.0, "wall_secs": began.elapsed().as_secs_f64(),
                   "words": tr.words.len(), "model": model_id, "lift": lift,
                   "report": serde_json::to_value(rep).unwrap()})
        );
        return;
    }

    let mut wavs: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "wav"))
        .filter(|p| {
            only.as_ref().is_none_or(|o| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .contains(o.as_str())
            })
        })
        .collect();
    wavs.sort();
    let (mut sum_silent, mut sum_words, mut sum_units) = (0.0, 0usize, 0usize);
    let mut lines = Vec::new();
    for wav in wavs {
        let name = wav.file_stem().unwrap().to_string_lossy().to_string();
        let samples = decode_16k_mono(&wav).unwrap();
        let total = samples.len() as f64 / 16_000.0;
        let speech = read_spans(&wav.with_extension("speech.json"));
        let silent = complement(&speech, guard, total);
        let began = Instant::now();
        let (tr, rep) = transcribe_pieces(
            engine,
            Pcm16k {
                samples: &samples,
                start: MediaTime::ZERO,
            },
            Some(&language),
            prompt.as_deref(),
            &mut |_| true,
        )
        .unwrap();
        let wall = began.elapsed().as_secs_f64();
        if let Some(d) = option(&args, "--save-dir") {
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(
                Path::new(&d).join(format!("{name}.transcript.json")),
                serde_json::to_string(&tr).unwrap(),
            )
            .unwrap();
        }
        let (v, c) = report(
            &name,
            &tr.words,
            &silent,
            show_text,
            json!({"model": model_id, "language": language, "lift": lift, "audio_secs": total,
                   "wall_secs": wall, "pieces": rep.pieces,
                   "speech_words": tr.words.len() - c_hit_free(&tr.words, &silent)}),
        );
        sum_silent += c.silent_secs;
        sum_words += c.words;
        sum_units += c.units;
        println!("{v}");
        lines.push(v.to_string());
    }
    let h = |n: usize| {
        if sum_silent > 0.0 {
            n as f64 / sum_silent * 3600.0
        } else {
            0.0
        }
    };
    let s = json!({"summary": true, "model": model_id, "language": language, "lift": lift,
        "silent_secs": sum_silent, "words_over_silence": sum_words, "units_over_silence": sum_units,
        "words_per_silent_hour": h(sum_words), "units_per_silent_hour": h(sum_units)});
    println!("{s}");
    lines.push(s.to_string());
    if let Some(o) = out {
        std::fs::write(o, lines.join("\n") + "\n").unwrap();
    }
}

/// How many words are NOT over silence (so the speech-side count can be reported).
fn c_hit_free(words: &[Word], silent: &[(f64, f64)]) -> usize {
    let (c, _) = count(words, silent);
    c.words
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complement_widens_speech_and_keeps_the_tail() {
        let s = complement(&[(5.0, 8.0)], 0.5, 30.0);
        assert_eq!(s, vec![(0.0, 4.5), (8.5, 30.0)]);
    }
}
