//! Transcription benchmark (release 0.2.7, M2a/M2d): word error rate, real-time factor, how many
//! Whisper calls the repeat re-runs cost, speech coverage and broken-token counts, for one audio
//! file with a reference text. Prints one JSON line. Not shipped; never downloads anything.
//!
//! ```text
//! cargo run --release -p cm-ai --example asr_bench -- --audio <file> --reference <text file>
//!     --model <ggml file> --model-id <id> [--language en] [--gpu on|off] [--speech <speech.json>]
//!     [--tile-to-secs N] [--label <short id>] [--words-out <file>] [--decode key=value,...]
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
use cm_ai::measure::levels;
use cm_ai::measure::{word_errors, word_errors_exact, word_errors_without, DISFLUENCIES};
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

    fn transcribe_careful(
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
            .transcribe_careful(audio, language, prompt, progress);
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
    // `--lead-secs N [--lead-db X]`: N seconds of room noise (default -55 dBFS RMS) before the speech,
    // the weak-signal start of a file where Whisper writes text that nobody said.
    if let Some(n) = option(&args, "--lead-secs").and_then(|v| v.parse::<f64>().ok()) {
        let db = option(&args, "--lead-db")
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(-55.0);
        let mut lead = levels::noise((n * 16_000.0) as usize, 10f64.powf(db / 20.0));
        lead.extend_from_slice(&samples);
        samples = lead;
        speech = speech.iter().map(|(a, b)| (a + n, b + n)).collect();
    }
    // Quiet-input variants (M2g item 3): `--quiet-db X` scales the speech to X dBFS RMS and adds
    // room noise (`--noise-db`, default -60); `--gate` passes it through the gating-denoiser model;
    // `--normalize peak|loudness` is the step being measured, applied after those.
    if let Some(level) = option(&args, "--quiet-db").and_then(|v| v.parse::<f64>().ok()) {
        let noise_db = option(&args, "--noise-db")
            .and_then(|v| v.parse().ok())
            .unwrap_or(-60.0);
        samples = levels::quiet_version(&samples, &speech, level, noise_db);
    }
    if args.iter().any(|a| a == "--gate") {
        samples = levels::gate_denoise(&samples);
    }
    // --agc: 80 Hz high-pass, then the whole clip's RMS to -20 dBFS with the gain capped at +15 dB
    // (the review's "raw mic + loudness normalisation per piece"; the clips here are one piece).
    if args.iter().any(|a| a == "--agc") {
        levels::high_pass(&mut samples, 80.0);
        levels::normalize_rms_capped(&mut samples, -20.0, 15.0);
    }
    // --gain-only: the same loudness step without the high-pass, and only when the clip is quieter
    // than --gain-below dBFS RMS (default -26): already-loud audio is left alone.
    if args.iter().any(|a| a == "--gain-only") {
        let below: f64 = option(&args, "--gain-below")
            .and_then(|v| v.parse().ok())
            .unwrap_or(-26.0);
        if levels::db(levels::rms_over(&samples, &[])) < below {
            if args.iter().any(|a| a == "--no-peak-limit") {
                levels::normalize_rms_capped(&mut samples, -20.0, 15.0);
            } else {
                levels::normalize_rms_capped_peak(&mut samples, -20.0, 15.0, -1.0);
            }
        }
    }
    match option(&args, "--normalize").as_deref() {
        Some("peak") => levels::normalize_peak(&mut samples, -3.0),
        Some("loudness") => levels::normalize_loudness(&mut samples, -20.0),
        _ => {}
    }
    let audio_secs = samples.len() as f64 / 16_000.0;

    let loaded = Instant::now();
    let flash = args.iter().any(|a| a == "--flash-attn");
    let careful = args.iter().any(|a| a == "--careful-rereads");
    let mut whisper = WhisperTranscriber::load_with(&model, &model_id, gpu, flash).unwrap();
    // --decode "beam=5,temp_inc=0,logprob=-0.7": decode parameters (see `DecodeParams::parse`).
    if let Some(spec) = option(&args, "--decode") {
        whisper = whisper.with_decode(cm_ai::whisper::DecodeParams::parse(&spec).unwrap());
    }
    if let Some(n) = option(&args, "--n-max-text-ctx").and_then(|v| v.parse::<i32>().ok()) {
        whisper = whisper.with_n_max_text_ctx(n);
    }
    if careful {
        whisper = whisper.with_careful_rereads();
    }
    let prompt = option(&args, "--prompt");
    let load_secs = loaded.elapsed().as_secs_f64();
    let counting = Counting {
        inner: &whisper,
        calls: Mutex::new(Vec::new()),
    };
    let began = Instant::now();
    // --vad <ggml silero model>: read only the speech regions, each as a call of its own.
    let vad = option(&args, "--vad")
        .map(|m| cm_ai::silero::SileroVad::load(std::path::Path::new(&m)).unwrap());
    let vad_first = vad.as_ref().map(|v| cm_ai::silero::VadFirst {
        inner: &counting,
        vad: v,
        params: {
            let mut p = cm_ai::silero::VadParams::default();
            if let Some(v) = option(&args, "--vad-threshold").and_then(|v| v.parse().ok()) {
                p.threshold = v;
            }
            if let Some(v) = option(&args, "--vad-min-silence").and_then(|v| v.parse().ok()) {
                p.min_silence_s = v;
            }
            if let Some(v) = option(&args, "--vad-pad").and_then(|v| v.parse().ok()) {
                p.pad_s = v;
            }
            p
        },
    });
    // --gate [--vad-opinion <silero model>]: the hallucination gate, with the voice-activity model
    // as the second opinion when given. Quiet input is lifted by the --gain-only step above.
    let opinion = option(&args, "--vad-opinion")
        .map(|m| cm_ai::silero::SileroVad::load(std::path::Path::new(&m)).unwrap());
    let configured = cm_ai::pipeline::Configured {
        inner: &counting,
        vad: opinion.as_ref(),
        gate: args.iter().any(|a| a == "--gate").then(|| {
            let mut g = cm_ai::pieces::gate::GateConfig::default();
            let off = option(&args, "--gate-off").unwrap_or_default();
            g.use_no_speech = !off.contains("ns");
            g.use_rate = !off.contains("rate");
            g.use_collapse = !off.contains("collapse");
            g.use_vad = !off.contains("vad");
            if let Some(v) = option(&args, "--gate-ns").and_then(|v| v.parse().ok()) {
                g.no_speech = v;
            }
            g
        }),
        normalize: args
            .iter()
            .any(|a| a == "--lift")
            .then(cm_ai::pipeline::Normalize::default),
    };
    let repaired = cm_ai::pipeline::RepairTimes {
        inner: if configured.gate.is_some() || configured.normalize.is_some() {
            &configured
        } else {
            &counting
        },
    };
    let engine: &dyn Transcriber = match &vad_first {
        Some(w) => w,
        None if args.iter().any(|a| a == "--repair-times") => &repaired,
        None if configured.gate.is_some() || configured.normalize.is_some() => &configured,
        None => &counting,
    };
    let (tr, report) = transcribe_pieces(
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
    let secs = began.elapsed().as_secs_f64();
    let calls = counting.calls.into_inner().unwrap();
    let read: f64 = calls.iter().map(|c| c.0).sum();
    let hyp = tr.text();
    if let Some(p) = option(&args, "--words-out") {
        std::fs::write(p, &hyp).unwrap();
    }
    if let Some(p) = option(&args, "--transcript-out") {
        // The transcript as the app stores it (words with times), for rescoring and comparison.
        std::fs::write(p, serde_json::to_string(&tr).unwrap()).unwrap();
    }
    let e = word_errors(&reference_all, &hyp);
    let e2 = word_errors_without(&reference_all, &hyp, DISFLUENCIES);
    let ex = word_errors_exact(&reference_all, &hyp);
    let h = health(&tr.words, &language);
    let spread = spread_repeats(&tr.words, 4, 10.0, 5);
    // Insertions over reference-silent regions: words whose middle is more than half a second from
    // every speech span, and the loops (a sentence 3 times in a row, or 5 in a minute).
    let outside = if speech.is_empty() {
        None
    } else {
        Some(
            tr.words
                .iter()
                .filter(|w| {
                    let mid = (w.start.ticks() + w.end.ticks()) as f64
                        / 2.0
                        / cm_core::TICKS_PER_SECOND as f64;
                    !speech.iter().any(|&(a, b)| mid > a - 0.5 && mid < b + 0.5)
                })
                .count(),
        )
    };
    let loops = cm_ai::failure_kinds::sentence_runs(&tr.words, 3).len();
    let dense = cm_ai::failure_kinds::dense_unit_indices(&tr.words, 5, 60.0).len();
    let exact = json!({
        "wer_exact": ex.wer(), "substitutions_exact": ex.substitutions,
        "insertions_exact": ex.insertions, "deletions_exact": ex.deletions,
    });
    let mut out = json!({
        "outside_speech_words": outside, "loop_runs": loops, "dense_repeat_words": dense,
        "label": label, "decode": option(&args, "--decode"), "model": model_id, "quiet_db": option(&args, "--quiet-db"), "gate": args.iter().any(|a| a == "--gate"), "normalize": option(&args, "--normalize"), "lead_secs": option(&args, "--lead-secs"), "language": language, "gpu": gpu,
        "audio_secs": audio_secs, "load_secs": load_secs, "transcribe_secs": secs,
        "rtf": audio_secs / secs,
        "reference_words": e.reference_words, "words": tr.words.len(),
        "wer": e.wer(), "wer_without_hesitations": e2.wer(),
        "substitutions": e.substitutions, "insertions": e.insertions, "deletions": e.deletions,
        "pieces": report.pieces, "split": report.split, "other_audio": report.other_audio,
        "repeats_dropped": report.repeats_dropped, "broken_reread": report.broken_reread, "broken_reread_kept": report.broken_reread_kept, "broken_dropped": report.broken_dropped,
        "whisper_calls": calls.len(), "audio_read_secs": read, "reread_factor": read / audio_secs,
        "call_secs_total": calls.iter().map(|c| c.1).sum::<f64>(),
        "health": {"replacement_char": h.replacement_char, "foreign_script": h.foreign_script,
                   "low_confidence": h.low_confidence, "broken_and_doubtful": h.broken_and_doubtful},
        "spread_repeats": spread.iter().map(|s| json!({"text": s.text, "count": s.count})).collect::<Vec<_>>(),
        "sounds": tr.sounds.len(),
        "peak_rss_mb": cm_core::perf::peak_rss_bytes().map(|b| b / 1_000_000),
    });
    out.as_object_mut()
        .unwrap()
        .extend(exact.as_object().unwrap().clone());
    if !speech.is_empty() {
        let c = word_coverage(&speech, &tr.words, 1.0, 5.0);
        out["coverage"] = json!({
            "speech_secs": c.speech_secs, "covered_secs": c.covered_secs, "ratio": c.ratio(),
            "uncovered_regions": c.uncovered.len(), "uncovered_secs": c.uncovered_secs(),
        });
    }
    println!("{out}");
}
