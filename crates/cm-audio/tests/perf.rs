//! CPU cost of the post-capture work (AUD-3.AC2: < 5% of one core with suppression on). The
//! measurement is the pipeline running a minute of 44.1 kHz stereo-capable mic audio as fast as it
//! can: cost per second of audio = share of one core in real time.
//!
//! `cargo test -p cm-audio --release --test perf -- --ignored --nocapture`

#![allow(clippy::unwrap_used)]

use std::sync::Arc;
use std::time::Instant;

use cm_audio::pipeline::{Pipeline, PipelineConfig};
use cm_audio::{AudioChunk, SourceFormat};
use cm_core::{ManualHostClock, MediaTime, SessionClock, TrackKind};

fn run(rate: u32, channels: u16, denoise: bool, secs: f64) -> f64 {
    let dir = tempfile::tempdir().unwrap();
    let clock = SessionClock::with_host(Arc::new(ManualHostClock::new(1_000_000_000)));
    let fmt = SourceFormat {
        sample_rate: rate,
        channels,
        device_name: "perf".into(),
        device_id: "perf".into(),
    };
    let mut p = Pipeline::new(
        PipelineConfig {
            dir: dir.path().to_path_buf(),
            clock,
            kind: TrackKind::Mic,
            raw_file: "mic.wav".into(),
            denoised_file: denoise.then(|| "mic.denoised.wav".into()),
            out_channels: channels,
            resync_ms: 20.0,
            stamp_offset_ms: 0.0,
        },
        &fmt,
        Arc::default(),
    )
    .unwrap();
    let chunk_frames = (rate / 100) as usize;
    let n_chunks = (secs * 100.0) as usize;
    let mut x = 1u32;
    let data: Vec<f32> = (0..chunk_frames * usize::from(channels))
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            (x as f32 / u32::MAX as f32 - 0.5) * 0.1
        })
        .collect();
    let t = Instant::now();
    for i in 0..n_chunks {
        p.push_chunk(AudioChunk {
            host_ns: 1_000_000_000 + (i as u64) * 10_000_000,
            channels,
            data: data.clone(),
        });
    }
    p.finish(MediaTime::from_secs(secs as i64 + 1)).unwrap();
    t.elapsed().as_secs_f64() / secs
}

#[test]
#[ignore = "measurement; run in release"]
fn cpu_share_per_second_of_audio() {
    for (rate, ch, dn) in [
        (48_000, 1, false),
        (48_000, 1, true),
        (44_100, 1, true),
        (48_000, 2, true),
    ] {
        let share = run(rate, ch, dn, 30.0);
        eprintln!(
            "{rate} Hz, {ch} ch, denoise {dn}: {:.2}% of one core",
            share * 100.0
        );
    }
}
