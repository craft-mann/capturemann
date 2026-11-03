//! Transcription benchmark (release 0.2.7, M2a/M2d): word error rate, real-time factor, how many
//! Whisper calls the repeat re-runs cost, speech coverage and broken-token counts, for one audio
//! file with a reference text. Prints one JSON line. Not shipped; never downloads anything.
//!
//! ```text
//! cargo run --release -p cm-ai --example asr_bench -- --audio <file> --reference <text file>
//!     --model <ggml file> --model-id <id> [--language en] [--gpu on|off] [--speech <speech.json>]
//!     [--tile-to-secs N] [--label <short id>] [--words-out <file>]
//! ```
//!
//! The run is the app's own: `transcribe_pieces` over a counting wrapper of `WhisperTranscriber`.
//! `calls` is the number of Whisper runs, `audio_read_secs` the audio they were given in total, so
//! `reread_factor = audio_read_secs / audio_secs` is the cost of splitting and re-reading looping
//! pieces (1.0: none). With `--tile-to-secs` the audio is repeated end to end (the reference too),
//! which stands in for a long recording and tends to provoke loops: say so when quoting it.
//! `--speech` is a JSON list of `[start, end]` speech spans (from `tests/tools/make_synthetic_meeting.py`).

#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

use cm_ai::audio::decode_16k_mono;
use cm_ai::failure_kinds::{health, spread_repeats, word_coverage};
use cm_ai::measure::{word_errors, word_errors_without, DISFLUENCIES};
use cm_ai::pieces::transcribe_pieces;
use cm_ai::transcribe::{Pcm16k, Transcriber, Transcript};
use cm_ai::whisper::WhisperTranscriber;
use cm_core::{CmError, MediaTime};
use serde_json::json;

fn option(args: &[String], flag: &str) -> Option<String> {
    let i = args.iter().position(|a| a == flag)?;
    args.get(i + 1).cloned()
}

/// Calls: (seconds of audio given, wall seconds).
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

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let audio_path = PathBuf::from(option(&args, "--audio").expect("--audio <file>"));
    let reference = std::fs::read_to_string(option(&args, "--reference").expect("--reference"))
        .unwrap()
        .trim()
        .to_owned();
    let model = PathBuf::from(option(&args, "--model").expect("--model <ggml file>"));
    let model_id = option(&args, "--model-id").expect("--model-id <id>");
    let language = option(&args, "--language").unwrap_or_else(|| "en".into());
    let gpu = option(&args, "--gpu").is_none_or(|g| g != "off");
    let label = option(&args, "--label").unwrap_or_else(|| "clip".into());

    let mut samples = decode_16k_mono(Path::new(&audio_path)).unwrap();
    let mut reference_all = reference.clone();
    let mut speech: Vec<(f64, f64)> = option(&args, "--speech")
        .map(|p| {
            serde_json::from_str::<Vec<(f64, f64)>>(&std::fs::read_to_string(p).unwrap()).unwrap()
        })
        .unwrap_or_default();
    if let Some(n) = option(&args, "--tile-to-secs").and_then(|s| s.parse::<f64>().ok()) {
        let one = samples.clone();
        let one_secs = one.len() as f64 / 16_000.0;
        let one_speech = speech.clone();
        let mut k = 1;
        while (samples.len() as f64 / 16_000.0) < n {
            samples.extend_from_slice(&one);
            reference_all.push(' ');
            reference_all.push_str(&reference);
            speech.extend(
                one_speech
                    .iter()
                    .map(|(a, b)| (a + k as f64 * one_secs, b + k as f64 * one_secs)),
            );
            k += 1;
        }
    }
    let audio_secs = samples.len() as f64 / 16_000.0;

    let loaded = Instant::now();
    let whisper = WhisperTranscriber::load(&model, &model_id, gpu).unwrap();
    let load_secs = loaded.elapsed().as_secs_f64();
    let counting = Counting {
        inner: &whisper,
        calls: Mutex::new(Vec::new()),
    };
    let began = Instant::now();
    let (tr, report) = transcribe_pieces(
        &counting,
        Pcm16k {
            samples: &samples,
            start: MediaTime::ZERO,
        },
        Some(&language),
        None,
        &mut |_| true,
    )
    .unwrap();
    let secs = began.elapsed().as_secs_f64();
    let calls = counting.calls.into_inner().unwrap();
    let read: f64 = calls.iter().map(|c| c.0).sum();
    let hyp = tr.text();
    if let Some(p) = option(&args, "--words-out") {
        std::fs::write(p, &hyp).unwrap();
    }
    let e = word_errors(&reference_all, &hyp);
    let e2 = word_errors_without(&reference_all, &hyp, DISFLUENCIES);
    let h = health(&tr.words, &language);
    let spread = spread_repeats(&tr.words, 4, 10.0, 5);
    let mut out = json!({
        "label": label, "model": model_id, "language": language, "gpu": gpu,
        "audio_secs": audio_secs, "load_secs": load_secs, "transcribe_secs": secs,
        "rtf": audio_secs / secs,
        "reference_words": e.reference_words, "words": tr.words.len(),
        "wer": e.wer(), "wer_without_hesitations": e2.wer(),
        "substitutions": e.substitutions, "insertions": e.insertions, "deletions": e.deletions,
        "pieces": report.pieces, "split": report.split, "other_audio": report.other_audio,
        "repeats_dropped": report.repeats_dropped,
        "whisper_calls": calls.len(), "audio_read_secs": read, "reread_factor": read / audio_secs,
        "call_secs_total": calls.iter().map(|c| c.1).sum::<f64>(),
        "health": {"replacement_char": h.replacement_char, "foreign_script": h.foreign_script,
                   "low_confidence": h.low_confidence, "broken_and_doubtful": h.broken_and_doubtful},
        "spread_repeats": spread.iter().map(|s| json!({"text": s.text, "count": s.count})).collect::<Vec<_>>(),
        "peak_rss_mb": cm_core::perf::peak_rss_bytes().map(|b| b / 1_000_000),
    });
    if !speech.is_empty() {
        let c = word_coverage(&speech, &tr.words, 1.0, 5.0);
        out["coverage"] = json!({
            "speech_secs": c.speech_secs, "covered_secs": c.covered_secs, "ratio": c.ratio(),
            "uncovered_regions": c.uncovered.len(), "uncovered_secs": c.uncovered_secs(),
        });
    }
    println!("{out}");
}
