//! Suggested pauses on quiet speech (release 0.2.7, M2g): builds a quiet recording from a clean
//! speech clip with a known speech map, runs the silence rule on it and counts the suggestions
//! that land inside speech. Prints one JSON line per variant. Not shipped.
//!
//! ```text
//! cargo run --release -p cm-ai --example quiet_silence -- --audio <speech.wav> --speech <speech.json>
//!     [--levels -30,-45,-52] [--noise -60] [--write <dir>]
//! ```
//!
//! Variants, for every speech level (dBFS RMS over the speech spans): `raw` (speech plus room noise
//! at `--noise` dBFS RMS) and `gated` (the same through a gating denoiser model: frames below
//! -50 dBFS are pushed down 1.5 dB per dB under the gate, up to 40 dB, which leaves the noise
//! floor near -80 dBFS and lowers quiet syllables by 4 to 9 dB, the shape found on the maintainer's
//! denoised track). `--write` saves the variants as WAV files (for `asr_bench`). The speech map is
//! the truth: a suggestion is *inside speech* when it overlaps a speech span by more than 0.1 s.

#![allow(clippy::unwrap_used)]

use std::path::Path;

use cm_ai::audio::decode_16k_mono;
use cm_ai::suggest::silence::{silence_cuts, SilenceOptions};
use cm_core::MediaTime;
use serde_json::json;

const RATE: u32 = 16_000;

fn option(args: &[String], flag: &str) -> Option<String> {
    let i = args.iter().position(|a| a == flag)?;
    args.get(i + 1).cloned()
}

fn db(rms: f64) -> f64 {
    20.0 * rms.max(1e-12).log10()
}

fn rms_over(samples: &[f32], spans: &[(f64, f64)]) -> f64 {
    let (mut sum, mut n) = (0.0f64, 0usize);
    for &(a, b) in spans {
        let (a, b) = ((a * 16_000.0) as usize, ((b * 16_000.0) as usize).min(samples.len()));
        for s in &samples[a.min(b)..b] {
            sum += f64::from(*s) * f64::from(*s);
            n += 1;
        }
    }
    (sum / n.max(1) as f64).sqrt()
}

/// Uniform noise scaled to `rms` (deterministic).
fn noise(len: usize, rms: f64) -> Vec<f32> {
    let mut x: u64 = 0x2545_F491_4F6C_DD1D;
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            let u = (x >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0;
            (u * rms * 3f64.sqrt()) as f32
        })
        .collect()
}

/// The gating denoiser model described in the file docs.
fn gate(samples: &[f32]) -> Vec<f32> {
    let n = (RATE as usize) / 50;
    let mut out = Vec::with_capacity(samples.len());
    for c in samples.chunks(n) {
        let l = db((c.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>() / c.len() as f64).sqrt());
        let atten = ((-50.0 - l) * 1.5).clamp(0.0, 40.0);
        let g = 10f64.powf(-atten / 20.0) as f32;
        out.extend(c.iter().map(|s| s * g));
    }
    out
}

fn write_wav(path: &Path, samples: &[f32]) {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(path, spec).unwrap();
    for s in samples {
        w.write_sample((s * 32767.0).clamp(-32768.0, 32767.0) as i16).unwrap();
    }
    w.finalize().unwrap();
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let audio = decode_16k_mono(Path::new(&option(&args, "--audio").expect("--audio"))).unwrap();
    let speech: Vec<(f64, f64)> = serde_json::from_str(
        &std::fs::read_to_string(option(&args, "--speech").expect("--speech")).unwrap(),
    )
    .unwrap();
    let levels: Vec<f64> = option(&args, "--levels")
        .unwrap_or_else(|| "-30,-45,-52".into())
        .split(',')
        .map(|v| v.parse().unwrap())
        .collect();
    let noise_db: f64 = option(&args, "--noise").and_then(|v| v.parse().ok()).unwrap_or(-60.0);
    let write = option(&args, "--write");
    let current = rms_over(&audio, &speech);
    let total_speech: f64 = speech.iter().map(|(a, b)| b - a).sum();
    for level in levels {
        let gain = 10f64.powf((level - db(current)) / 20.0) as f32;
        let mut raw: Vec<f32> = audio.iter().map(|s| s * gain).collect();
        for (r, n) in raw.iter_mut().zip(noise(audio.len(), 10f64.powf(noise_db / 20.0))) {
            *r += n;
        }
        let gated = gate(&raw);
        for (name, track) in [("raw", &raw), ("gated", &gated)] {
            if let Some(dir) = &write {
                write_wav(&Path::new(dir).join(format!("q{}_{name}.wav", -level as i64)), track);
            }
            let cuts = silence_cuts(track, RATE, MediaTime::ZERO, &[], &SilenceOptions::default()).unwrap();
            let secs = |t: MediaTime| t.ticks() as f64 / cm_core::TICKS_PER_SECOND as f64;
            let (mut inside, mut inside_secs, mut cut_secs) = (0, 0.0, 0.0);
            for c in &cuts {
                let (a, b) = (secs(c.start), secs(c.end));
                cut_secs += b - a;
                let overlap: f64 = speech.iter().map(|&(s, e)| (b.min(e) - a.max(s)).max(0.0)).sum();
                if overlap > 0.1 {
                    inside += 1;
                }
                inside_secs += overlap;
            }
            // Pauses the rule should find: gaps between speech spans of at least 1.4 s.
            let wanted: Vec<(f64, f64)> = speech
                .windows(2)
                .map(|w| (w[0].1, w[1].0))
                .filter(|(a, b)| b - a >= 1.4)
                .collect();
            let found = wanted
                .iter()
                .filter(|&&(a, b)| cuts.iter().any(|c| secs(c.start) < b && secs(c.end) > a))
                .count();
            let sorted = {
                let mut l: Vec<f64> = track
                    .chunks(320)
                    .filter(|c| c.len() == 320)
                    .map(|c| db((c.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>() / 320.0).sqrt()))
                    .collect();
                l.sort_by(f64::total_cmp);
                l
            };
            println!(
                "{}",
                json!({
                    "variant": name, "speech_db": level, "noise_db": noise_db,
                    "measured_speech_db": db(rms_over(track, &speech)),
                    "floor_p10_db": sorted[sorted.len() / 10], "median_db": sorted[sorted.len() / 2],
                    "speech_secs": total_speech, "suggestions": cuts.len(), "suggested_secs": cut_secs,
                    "inside_speech": inside, "speech_secs_inside_suggestions": inside_secs,
                    "wanted_pauses": wanted.len(), "wanted_found": found,
                })
            );
        }
    }
}
