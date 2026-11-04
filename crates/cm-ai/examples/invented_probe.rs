//! Experiments on telling invented text from faint speech (0.2.7 plan section 5). Not shipped. For
//! each span of a recording it decodes the span with one second of context on each side, on its
//! own and without a prompt, with Turbo and Large v3 (both loaded once): unchanged, 6 dB quieter,
//! shifted by 150 ms, with mild noise, and (when a second file is given) from the noise-reduced
//! copy. Prints one JSON line per span with the texts (private recordings: keep the output out of
//! the repository) and one `signature` line when `--signature-secs` is given: the text each model
//! writes for the recording's own quietest stretch.
//!
//! ```text
//! cargo run --release -p cm-ai --example invented_probe -- --raw <wav> [--copy <wav>]
//!     --spans <json [[start, end, "label"], ...]> --turbo <ggml> --large <ggml> --language vi
//!     [--signature-secs 30] [--no-large]
//! ```

#![allow(clippy::unwrap_used)]

use std::path::Path;

use cm_ai::audio::decode_16k_mono;
use cm_ai::measure::levels;
use cm_ai::pipeline::{Configured, Normalize};
use cm_ai::transcribe::{Pcm16k, Transcriber};
use cm_ai::whisper::WhisperTranscriber;
use cm_core::MediaTime;
use serde_json::json;

fn option(args: &[String], flag: &str) -> Option<String> {
    let i = args.iter().position(|a| a == flag)?;
    args.get(i + 1).cloned()
}

fn wrap(w: &WhisperTranscriber) -> Configured<'_> {
    Configured {
        inner: w,
        vad: None,
        gate: None,
        normalize: Some(Normalize::default()),
    }
}

fn decode(engine: &dyn Transcriber, samples: &[f32], language: &str) -> String {
    if samples.len() < 8_000 {
        return String::new();
    }
    engine
        .transcribe_prompted(
            Pcm16k {
                samples,
                start: MediaTime::ZERO,
            },
            Some(language),
            None,
            &mut |_| true,
        )
        .map(|t| t.text())
        .unwrap_or_default()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let language = option(&args, "--language").unwrap_or_else(|| "vi".into());
    let raw = decode_16k_mono(Path::new(&option(&args, "--raw").expect("--raw"))).unwrap();
    let copy = option(&args, "--copy").map(|p| decode_16k_mono(Path::new(&p)).unwrap());
    let turbo = WhisperTranscriber::load_with(
        Path::new(&option(&args, "--turbo").expect("--turbo")),
        "large-v3-turbo",
        true,
        false,
    )
    .unwrap();
    let large = (!args.iter().any(|a| a == "--no-large")).then(|| {
        WhisperTranscriber::load_with(
            Path::new(&option(&args, "--large").expect("--large")),
            "large-v3",
            true,
            false,
        )
        .unwrap()
    });
    let ct = wrap(&turbo);
    let cl = large.as_ref().map(wrap);

    if let Some(secs) = option(&args, "--signature-secs").and_then(|v| v.parse::<f64>().ok()) {
        // The quietest stretch: the window whose 90th-percentile 1 s level is lowest.
        let n = secs as usize;
        let lv: Vec<f64> = raw
            .as_chunks::<16_000>()
            .0
            .iter()
            .map(|c| c.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>() / 16_000.0)
            .collect();
        let mut best = (f64::MAX, 0usize);
        for i in 0..lv.len().saturating_sub(n) {
            let mut w: Vec<f64> = lv[i..i + n].to_vec();
            w.sort_by(f64::total_cmp);
            let p90 = w[w.len() * 9 / 10];
            if p90 < best.0 {
                best = (p90, i);
            }
        }
        let seg = &raw[best.1 * 16_000..(best.1 + n) * 16_000];
        println!(
            "{}",
            json!({"signature": true, "start": best.1, "secs": n,
                   "turbo": decode(&ct, seg, &language),
                   "large": cl.as_ref().map(|c| decode(c, seg, &language))})
        );
    }

    let Some(spans) = option(&args, "--spans") else {
        return;
    };
    let spans: Vec<(f64, f64, String)> =
        serde_json::from_str(&std::fs::read_to_string(spans).unwrap()).unwrap();
    for (a, b, label) in spans {
        let (s0, s1) = (
            (((a - 1.0).max(0.0)) * 16_000.0) as usize,
            (((b + 1.0) * 16_000.0) as usize).min(raw.len()),
        );
        let w = &raw[s0..s1];
        let rms = levels::rms_over(w, &[]);
        let quieter: Vec<f32> = w.iter().map(|s| s * 0.5).collect();
        let shifted = &w[2_400.min(w.len())..];
        let noise = levels::noise(w.len(), rms * 0.1);
        let noisy: Vec<f32> = w.iter().zip(noise).map(|(s, n)| s + n).collect();
        let mut out = json!({
            "label": label, "a": a, "b": b,
            "orig": decode(&ct, w, &language),
            "quiet6": decode(&ct, &quieter, &language),
            "shift150": decode(&ct, shifted, &language),
            "noise": decode(&ct, &noisy, &language),
        });
        if let Some(c) = &cl {
            out["large"] = json!(decode(c, w, &language));
        }
        if let Some(cp) = &copy {
            let wc = &cp[s0.min(cp.len())..s1.min(cp.len())];
            out["copy"] = json!(decode(&ct, wc, &language));
            if let Some(c) = &cl {
                out["large_copy"] = json!(decode(c, wc, &language));
            }
        }
        println!("{out}");
    }
}
