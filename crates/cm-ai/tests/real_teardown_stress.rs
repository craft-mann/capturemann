//! Stress for the engine teardown abort of Nov 10, 2026 (`LoggingManager::~LoggingManager` out of
//! `SherpaEngine::run`): create, run and destroy the speaker engine many times, from several
//! threads at once, the way the windows of a long recording do. Ignored by default; never
//! downloads; use a synthetic audio file, not a recording of a person.
//!
//! ```text
//! CM_SHERPA_MODELS=<dir> CM_DIARIZE_AUDIO=<16 kHz mono 16-bit WAV> [CM_STRESS_RUNS=30] \
//!   [CM_STRESS_THREADS=3] [CM_STRESS_WINDOW_SECS=60] \
//!   cargo test -p cm-ai --test real_teardown_stress --release -- --ignored --nocapture
//! ```
//! Prints the runs done, the threads and the keep-alive count; an abort ends the process.

#![cfg(feature = "sherpa")]
#![allow(clippy::unwrap_used)]

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use cm_ai::speaker_engine::{ort_keepers, ClusterParams, SherpaEngine, SpeakerModels, TurnEngine};

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

#[test]
#[ignore = "needs CM_SHERPA_MODELS and CM_DIARIZE_AUDIO"]
fn engine_teardown_survives_many_runs_in_parallel() {
    let (Some(models), Some(audio)) = (
        std::env::var_os("CM_SHERPA_MODELS"),
        std::env::var_os("CM_DIARIZE_AUDIO"),
    ) else {
        println!("SKIPPED: set CM_SHERPA_MODELS and CM_DIARIZE_AUDIO");
        return;
    };
    let dir = PathBuf::from(models);
    let models = SpeakerModels {
        segmentation: dir.join("pyannote-segmentation-3-0.onnx"),
        embedding: dir.join("campplus-sv-zh-en-16k-advanced.onnx"),
    };
    let mut reader = hound::WavReader::open(audio).unwrap();
    let samples: Vec<f32> = reader
        .samples::<i16>()
        .map(|s| f32::from(s.unwrap()) / 32768.0)
        .collect();
    let runs = env_usize("CM_STRESS_RUNS", 30);
    let threads = env_usize("CM_STRESS_THREADS", 3);
    let window = env_usize("CM_STRESS_WINDOW_SECS", 60) * 16_000;
    let windows = (samples.len() / window).max(1);
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| loop {
                let n = next.fetch_add(1, Ordering::SeqCst);
                if n >= runs {
                    break;
                }
                let at = (n % windows) * window;
                let end = (at + window).min(samples.len());
                let turns = SherpaEngine::default()
                    .run(
                        &models,
                        &samples[at..end],
                        ClusterParams {
                            num_clusters: None,
                            threshold: cm_ai::speaker_engine::AUTO_THRESHOLD,
                        },
                        &mut |_| true,
                    )
                    .unwrap();
                done.fetch_add(1, Ordering::SeqCst);
                println!("run {n}: {} turns", turns.len());
            });
        }
    });
    println!(
        "STRESS runs={} threads={threads} keepers={}",
        done.load(Ordering::SeqCst),
        ort_keepers()
    );
    assert_eq!(done.load(Ordering::SeqCst), runs);
    assert_eq!(ort_keepers(), 1);
}
