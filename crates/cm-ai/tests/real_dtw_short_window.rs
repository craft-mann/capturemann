//! Runs the real whisper.cpp model with DTW token times on clips that make a decode window end
//! within its first 160 ms (a short noise burst, then silence). Upstream whisper.cpp 1.8.3 aborted
//! the whole process there (`WHISPER_ASSERT(filter_width < a->ne[2])` in `median_filter`, see
//! vendor/README-CaptureMann.md); the vendored copy skips DTW for such a window and the words keep
//! the default token times. Ignored by default and it never downloads anything.
//!
//! ```text
//! CM_WHISPER_MODEL=/path/to/ggml-large-v3-turbo.bin \
//!   cargo test -p cm-ai --test real_dtw_short_window -- --ignored --nocapture
//! ```
//!
//! Optional: `CM_WHISPER_MODEL_ID` (default: taken from the file name, `ggml-<id>.bin`; it must be a
//! stock id so the DTW alignment heads are known), `CM_WHISPER_AUDIO` (one clip of your own, a
//! 16 kHz mono 16-bit WAV, run instead of the synthetic clips). The test passes when every run
//! completes; it prints, per clip, the word count, the wall time and how many windows skipped DTW
//! (`EXERCISED` when at least one did, so the guard was really hit by this model on this machine).
//! Nothing it prints is transcript text.

#![allow(clippy::unwrap_used)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use cm_ai::transcribe::{Pcm16k, Transcriber};
use cm_ai::whisper::WhisperTranscriber;
use cm_core::MediaTime;

const RATE: usize = 16_000;

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// Windows that skipped DTW, counted from whisper.cpp's own log line.
static DTW_SKIPPED: AtomicUsize = AtomicUsize::new(0);

unsafe extern "C" fn count_dtw_skips(
    _level: whisper_rs::whisper_rs_sys::ggml_log_level,
    text: *const std::ffi::c_char,
    _user: *mut std::ffi::c_void,
) {
    if text.is_null() {
        return;
    }
    // SAFETY: whisper.cpp passes a NUL-terminated string that lives for the call.
    let s = unsafe { std::ffi::CStr::from_ptr(text) }.to_string_lossy();
    if s.contains("DTW") && s.contains("skip") {
        DTW_SKIPPED.fetch_add(1, Ordering::Relaxed);
    }
}

/// A deterministic pseudo-random sequence (no dependency).
fn noise(seed: &mut u64) -> f32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    ((*seed >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0) as f32
}

/// Synthetic clips: a short sound, then silence. Each is a whole decode window whose segment, if
/// the model writes one for the sound, ends almost at once.
fn synthetic_clips() -> Vec<(String, Vec<f32>)> {
    let mut out = Vec::new();
    let mut seed = 0x9E37_79B9_7F4A_7C15u64;
    for &(burst_ms, silence_s) in &[(50usize, 2.0f32), (20, 1.5), (120, 3.0), (80, 10.0)] {
        let mut s = vec![0.0f32; (RATE as f32 * silence_s) as usize];
        for x in s.iter_mut().take(RATE * burst_ms / 1000) {
            *x = 0.6 * noise(&mut seed);
        }
        out.push((format!("noise {burst_ms} ms + {silence_s} s silence"), s));
    }
    // A click (one-sided impulse with a fast decay) then silence.
    let mut click = vec![0.0f32; RATE * 2];
    for (i, x) in click.iter_mut().enumerate().take(RATE / 50) {
        *x = 0.9 * (-(i as f32) / 40.0).exp();
    }
    out.push(("click + 2 s silence".to_owned(), click));
    // A 1 kHz beep then silence.
    let mut beep = vec![0.0f32; RATE * 3];
    for (i, x) in beep.iter_mut().enumerate().take(RATE / 10) {
        *x = 0.5 * (i as f32 * 1000.0 * std::f32::consts::TAU / RATE as f32).sin();
    }
    out.push(("1 kHz beep 100 ms + 3 s silence".to_owned(), beep));
    // Low noise floor with a burst at the start (a room with a door closing).
    let mut floor = vec![0.0f32; RATE * 4];
    for (i, x) in floor.iter_mut().enumerate() {
        *x = if i < RATE / 20 { 0.7 } else { 0.004 } * noise(&mut seed);
    }
    out.push(("burst 50 ms + 4 s noise floor".to_owned(), floor));
    out
}

fn load_wav(path: &std::path::Path) -> Vec<f32> {
    let mut r = hound::WavReader::open(path).expect("open the WAV");
    let spec = r.spec();
    assert!(
        spec.channels == 1 && spec.sample_rate as usize == RATE && spec.bits_per_sample == 16,
        "CM_WHISPER_AUDIO must be a 16 kHz mono 16-bit WAV"
    );
    r.samples::<i16>()
        .map(|s| f32::from(s.unwrap()) / 32768.0)
        .collect()
}

#[test]
#[ignore = "needs a real model; see the file docs"]
fn a_window_that_ends_at_once_does_not_abort_the_process() {
    let Some(model) = env_path("CM_WHISPER_MODEL") else {
        println!("SKIPPED: set CM_WHISPER_MODEL (nothing is downloaded)");
        return;
    };
    let model_id = std::env::var("CM_WHISPER_MODEL_ID")
        .ok()
        .unwrap_or_else(|| {
            let stem = model.file_stem().unwrap().to_string_lossy().into_owned();
            stem.strip_prefix("ggml-").unwrap_or(&stem).to_owned()
        });
    let t0 = Instant::now();
    let transcriber =
        WhisperTranscriber::load(&model, &model_id, true).expect("load the Whisper model");
    println!(
        "MODEL {} as `{model_id}` loaded in {:.1} s",
        model.display(),
        t0.elapsed().as_secs_f64()
    );
    // After `load` (which installs whisper-rs's hooks): count whisper.cpp's own log lines.
    // SAFETY: the callback has no user data and outlives the process.
    unsafe {
        whisper_rs::whisper_rs_sys::whisper_log_set(Some(count_dtw_skips), std::ptr::null_mut());
    }

    let clips = match env_path("CM_WHISPER_AUDIO") {
        Some(p) => vec![(p.display().to_string(), load_wav(&p))],
        None => synthetic_clips(),
    };
    let mut exercised = 0usize;
    for (name, samples) in &clips {
        let before = DTW_SKIPPED.load(Ordering::Relaxed);
        let t = Instant::now();
        let out = transcriber
            .transcribe(
                Pcm16k {
                    samples,
                    start: MediaTime::ZERO,
                },
                Some("en"),
                &mut |_| true,
            )
            .expect("the run completes");
        let skipped = DTW_SKIPPED.load(Ordering::Relaxed) - before;
        exercised += skipped;
        // Every word keeps a non-negative, ordered span whatever the token times were.
        for w in &out.words {
            assert!(w.end >= w.start, "{name}: a word ends before it starts");
        }
        assert!(
            out.words.windows(2).all(|p| p[1].start >= p[0].start),
            "{name}: words are out of order"
        );
        println!(
            "CLIP {name}: {:.1} s audio, {} word(s), {} window(s) skipped DTW, {:.2} s wall",
            samples.len() as f64 / RATE as f64,
            out.words.len(),
            skipped,
            t.elapsed().as_secs_f64()
        );
    }
    println!(
        "{}: {exercised} window(s) skipped DTW over {} clip(s)",
        if exercised > 0 {
            "EXERCISED"
        } else {
            "NOT EXERCISED (this model wrote no segment that ends within 160 ms)"
        },
        clips.len()
    );
}
