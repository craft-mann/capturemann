//! Runs the real whisper.cpp model on real speech. Ignored by default and it never downloads
//! anything: you point it at files you already have.
//!
//! ```text
//! CM_WHISPER_MODEL=/path/to/ggml-small.bin \
//! CM_WHISPER_AUDIO=/path/to/speech.wav \
//! CM_WHISPER_REFERENCE=/path/to/reference.txt \
//!   cargo test -p cm-ai --test real_model -- --ignored --nocapture
//! ```
//!
//! A 16 kHz mono 16-bit WAV is read directly; other formats need FFmpeg. Paths may contain spaces.
//! It prints a `SPEED` line (audio seconds, wall seconds, real-time factor, model file, CPU threads)
//! and the transcription time (as a multiple of real time) and the word error rate against
//! the reference. With `CM_WHISPER_ENFORCE_AC1=1` it asserts AI-1.AC1 (WER at most 12 % for the
//! `small` model); use a 10-minute English recording for AC1 and AC2 (at most 5 minutes on an
//! Apple M-series 16 GB machine). Optional: `CM_WHISPER_LANGUAGE` (default `auto`).

#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::time::Instant;

use cm_ai::audio::{decode_16k_mono, SAMPLE_RATE};
use cm_ai::transcribe::{word_error_rate, Pcm16k, Transcriber};
use cm_ai::whisper::WhisperTranscriber;
use cm_core::MediaTime;

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// A 16 kHz mono 16-bit WAV is read directly (no FFmpeg, so it works on a machine without one);
/// any other file goes through FFmpeg like the app does.
fn load_audio(path: &Path) -> Vec<f32> {
    if let Ok(mut r) = hound::WavReader::open(path) {
        let spec = r.spec();
        if spec.channels == 1
            && spec.sample_rate == SAMPLE_RATE
            && spec.bits_per_sample == 16
            && spec.sample_format == hound::SampleFormat::Int
        {
            return r
                .samples::<i16>()
                .map(|s| f32::from(s.expect("read a WAV sample")) / 32768.0)
                .collect();
        }
    }
    decode_16k_mono(path).expect("decode the audio (other formats need FFmpeg)")
}

#[test]
#[ignore = "needs a real model and speech audio; see the file docs"]
fn real_model_transcribes_speech() {
    let (Some(model), Some(audio)) = (env_path("CM_WHISPER_MODEL"), env_path("CM_WHISPER_AUDIO"))
    else {
        println!("SKIPPED: set CM_WHISPER_MODEL and CM_WHISPER_AUDIO (nothing is downloaded)");
        return;
    };
    let samples = load_audio(&audio);
    let secs = samples.len() as f64 / f64::from(SAMPLE_RATE);
    // `CM_WHISPER_GPU=0` forces the CPU path (to tell a GPU failure from a model failure).
    let gpu = std::env::var("CM_WHISPER_GPU").map_or(true, |v| v != "0");
    let t = WhisperTranscriber::load(&model, "real", gpu).expect("load the model");
    let language = std::env::var("CM_WHISPER_LANGUAGE").ok();
    let began = Instant::now();
    let out = t
        .transcribe(
            Pcm16k {
                samples: &samples,
                start: MediaTime::ZERO,
            },
            language.as_deref(),
            &mut |_| true,
        )
        .expect("transcribe");
    let took = began.elapsed().as_secs_f64();
    // One line to quote: audio seconds, wall seconds, real-time factor (wall / audio, below 1 is
    // faster than real time), the model file and the CPU thread count.
    println!(
        "SPEED audio_s={secs:.1} wall_s={took:.1} rtf={:.3} model={} threads={} gpu_requested={gpu}",
        took / secs.max(1e-9),
        model
            .file_name()
            .map_or_else(|| model.display().to_string(), |n| n.to_string_lossy().into_owned()),
        t.threads()
    );
    println!(
        "audio {secs:.1} s, transcribed in {took:.1} s ({:.2}x real time), language {}, {} words",
        secs / took.max(1e-9),
        out.language,
        out.words.len()
    );
    println!("text: {}", out.text());
    assert!(!out.words.is_empty(), "no words: {:?}", out.note);
    assert!(out
        .words
        .windows(2)
        .all(|w| w[0].start <= w[1].start && w[0].start <= w[0].end));
    if let Some(reference) = env_path("CM_WHISPER_REFERENCE") {
        let reference = std::fs::read_to_string(reference).expect("read the reference");
        let wer = word_error_rate(&reference, &out.text());
        println!("word error rate {:.1} %", wer * 100.0);
        if std::env::var_os("CM_WHISPER_ENFORCE_AC1").is_some() {
            assert!(wer <= 0.12, "AI-1.AC1: WER {wer:.3} is above 0.12");
        }
    }
}

/// Regression for whisper error -6: an abort callback that answered "abort" at random made every
/// run fail. A run that is not cancelled must finish, and a cancelled one must stop with the
/// cancelled error, not a "speech recognition failed".
#[test]
#[ignore = "needs a real model and speech audio; see the file docs"]
fn real_model_cancel_works_and_a_normal_run_is_not_aborted() {
    let (Some(model), Some(audio)) = (env_path("CM_WHISPER_MODEL"), env_path("CM_WHISPER_AUDIO"))
    else {
        println!("SKIPPED: set CM_WHISPER_MODEL and CM_WHISPER_AUDIO (nothing is downloaded)");
        return;
    };
    let one = load_audio(&audio);
    // About 30 s: long enough for progress to be reported before the run ends.
    let samples: Vec<f32> = one.iter().copied().cycle().take(one.len() * 6).collect();
    let t = WhisperTranscriber::load(&model, "real", true).expect("load the model");
    let input = || Pcm16k {
        samples: &samples,
        start: MediaTime::ZERO,
    };
    let ok = t
        .transcribe(input(), Some("en"), &mut |_| true)
        .expect("an uncancelled run finishes");
    assert!(!ok.words.is_empty());
    let err = t
        .transcribe(input(), Some("en"), &mut |_| false)
        .expect_err("a cancelled run stops");
    assert!(
        cm_ai::transcribe::is_cancelled(&err),
        "cancelled, not failed: {err:?}"
    );
}
