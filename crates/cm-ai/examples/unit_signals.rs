//! Speech-evidence signals for the units of a transcript (0.2.7 plan, W2/W4). Not shipped; never
//! downloads anything. For a transcript and the audio it was made from, prints one JSON document
//! with: the piece plan, the units (runs of words less than 0.6 s apart, cut at 15 s) with their
//! energy share, Silero share, speaker-engine share, words per second and mean confidence, the
//! energy regions, and the quiet spans (3 s or more where energy, Silero and the speaker engine all
//! say there is no speech). Times and numbers only, never words, so the output may describe a
//! private recording.
//!
//! ```text
//! cargo run --release -p cm-ai --example unit_signals -- --transcript <transcript.json>
//!     --audio <file> [--diarization <diarization.json>] [--vad <ggml-silero-v5.1.2.bin>]
//! ```

#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use cm_ai::audio::decode_16k_mono;
use cm_ai::transcribe::Transcript;
use cm_core::TICKS_PER_SECOND;
use serde_json::json;

fn option(args: &[String], flag: &str) -> Option<String> {
    let i = args.iter().position(|a| a == flag)?;
    args.get(i + 1).cloned()
}

fn share(flags: &[bool], a: f64, b: f64, step: f64) -> f64 {
    let (i, j) = (
        (a / step) as usize,
        ((b / step).ceil() as usize).min(flags.len()),
    );
    if j <= i {
        return 0.0;
    }
    flags[i..j].iter().filter(|x| **x).count() as f64 / (j - i) as f64
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let tr: Transcript = serde_json::from_str(
        &std::fs::read_to_string(option(&args, "--transcript").expect("--transcript")).unwrap(),
    )
    .unwrap();
    let samples = decode_16k_mono(Path::new(&option(&args, "--audio").expect("--audio"))).unwrap();
    let total = samples.len() as f64 / 16_000.0;
    const STEP: f64 = 0.1;
    let n = (total / STEP).ceil() as usize + 1;

    // Energy: the pipeline's own regions (20 ms frames, 40 % of the way from floor to speech).
    let regions = cm_ai::vad::speech_regions(&samples, 0.16);
    let mut energy = vec![false; n];
    for &(a, b) in &regions {
        for f in energy
            .iter_mut()
            .take(((b as f64 / 16_000.0 / STEP) as usize + 1).min(n))
            .skip((a as f64 / 16_000.0 / STEP) as usize)
        {
            *f = true;
        }
    }
    // Silero: probability per 32 ms.
    let probs = option(&args, "--vad").map(|m| {
        cm_ai::silero::SileroVad::load(&PathBuf::from(m))
            .unwrap()
            .probabilities(&samples)
            .unwrap()
    });
    let silero_flags: Vec<bool> = (0..n)
        .map(|i| {
            probs.as_ref().is_some_and(|p| {
                cm_ai::silero::speech_share(p, i as f64 * STEP, (i + 1) as f64 * STEP, 0.5) > 0.3
            })
        })
        .collect();
    // Speaker engine: its segments.
    let mut diar = vec![false; n];
    let have_diar = option(&args, "--diarization").map(|p| {
        let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
        for s in v["segments"].as_array().unwrap() {
            let (a, b) = (
                s["start"].as_f64().unwrap() / TICKS_PER_SECOND as f64,
                s["end"].as_f64().unwrap() / TICKS_PER_SECOND as f64,
            );
            for f in diar
                .iter_mut()
                .take(((b / STEP) as usize + 1).min(n))
                .skip((a / STEP) as usize)
            {
                *f = true;
            }
        }
    });

    // Units.
    let sec = |t: cm_core::MediaTime| t.ticks() as f64 / TICKS_PER_SECOND as f64;
    let mut units: Vec<(f64, f64, usize, f32)> = Vec::new(); // start, end, words, conf sum
    for w in &tr.words {
        let (s, e) = (sec(w.start), sec(w.end));
        match units.last_mut() {
            Some(u) if s - u.1 < 0.6 && e - u.0 <= 15.0 => {
                u.1 = e.max(u.1);
                u.2 += 1;
                u.3 += w.confidence;
            }
            _ => units.push((s, e, 1, w.confidence)),
        }
    }
    let unit_json: Vec<_> = units
        .iter()
        .map(|&(a, b, k, c)| {
            let len = (b - a).max(0.01);
            json!({
                "a": a, "b": b, "words": k, "rate": k as f64 / len, "conf": c / k as f32,
                "energy": share(&energy, a, b, STEP),
                "silero": if probs.is_some() { Some(share(&silero_flags, a, b, STEP)) } else { None },
                "diar": if have_diar.is_some() { Some(share(&diar, a, b, STEP)) } else { None },
            })
        })
        .collect();

    // Quiet spans: no energy, no Silero, no speaker-engine speech, for 3 s or more.
    let mut quiet: Vec<(f64, f64)> = Vec::new();
    let mut from: Option<usize> = None;
    for i in 0..=n {
        let bad = i < n && (energy[i] || silero_flags[i] || diar[i]);
        if i == n || bad {
            if let Some(s) = from.take() {
                let (a, b) = (s as f64 * STEP, i as f64 * STEP);
                if b - a >= 3.0 {
                    quiet.push((a, b));
                }
            }
        } else if from.is_none() {
            from = Some(i);
        }
    }

    // Piece plan of the audio.
    let pieces: Vec<(f64, f64)> = cm_ai::pieces::plan(&samples)
        .iter()
        .map(|r| (r.start as f64 / 16_000.0, r.end as f64 / 16_000.0))
        .collect();
    let level_db = |a: f64, b: f64| {
        let (i, j) = ((a * 16_000.0) as usize, ((b * 16_000.0) as usize).min(samples.len()));
        if j <= i {
            return -120.0;
        }
        10.0 * (samples[i..j].iter().map(|s| f64::from(*s).powi(2)).sum::<f64>() / (j - i) as f64 + 1e-12).log10()
    };
    let unit_levels: Vec<f64> = units.iter().map(|u| level_db(u.0, u.1)).collect();
    let quiet_levels: Vec<f64> = quiet.iter().map(|q| level_db(q.0, q.1)).collect();
    println!(
        "{}",
        json!({
            "audio_secs": total, "words": tr.words.len(), "pieces": pieces,
            "energy_regions": regions.iter().map(|r| json!([r.0 as f64 / 16_000.0, r.1 as f64 / 16_000.0])).collect::<Vec<_>>(),
            "units": unit_json, "unit_db": unit_levels,
            "quiet_spans": quiet, "quiet_db": quiet_levels,
        })
    );
}
