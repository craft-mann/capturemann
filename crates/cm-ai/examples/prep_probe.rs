//! Audio-preparation experiments (0.2.7 plan, W5/W6). Not shipped; never downloads anything. Runs the
//! app's `transcribe_pieces` on one file with a candidate from `cm_ai::prep` and stores the
//! transcript (word times in source time), so `tests/tools/wer_exact.py` and `silence_probe
//! --transcript` can score it.
//!
//! ```text
//! cargo run --release -p cm-ai --example prep_probe -- --audio <file> --model <ggml> --model-id <id>
//!     --language vi --save <transcript.json> [--trim] [--region-lift] [--after-speech]
//!     [--lift off] [--min-silence 8] [--keep 1]
//! ```
//! Default (no flag): the shipped path (per-call lift). `--trim`: silences over `--min-silence`
//! seconds cut to `2 * --keep`, times mapped back. `--region-lift`: the lift per speech region
//! instead of per call. `--after-speech`: pieces cut just after speech (audio over 600 s only).

#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::time::Instant;

use cm_ai::audio::decode_16k_mono;
use cm_ai::pieces::transcribe_pieces;
use cm_ai::prep;
use cm_ai::transcribe::{media_secs, Pcm16k, Transcriber, Transcript};
use cm_ai::whisper::WhisperTranscriber;
use cm_core::{CmError, MediaTime, TICKS_PER_SECOND};
use serde_json::json;

fn option(args: &[String], flag: &str) -> Option<String> {
    let i = args.iter().position(|a| a == flag)?;
    args.get(i + 1).cloned()
}

struct RegionLifted<'a>(&'a dyn Transcriber);

impl Transcriber for RegionLifted<'_> {
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
        let regions = prep::loud_regions(audio.samples, 0.25, 0.3, 0.16);
        let v = prep::lift_per_region(audio.samples, &regions, prep::RegionLift::default());
        self.0.transcribe_prompted(
            Pcm16k {
                samples: &v,
                start: audio.start,
            },
            language,
            prompt,
            progress,
        )
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |f: &str| args.iter().any(|a| a == f);
    let samples = decode_16k_mono(Path::new(&option(&args, "--audio").expect("--audio"))).unwrap();
    let model = PathBuf::from(option(&args, "--model").unwrap_or_default());
    let model_id = option(&args, "--model-id").unwrap_or_default();
    let language = option(&args, "--language").unwrap_or_else(|| "en".into());
    let lift_call = option(&args, "--lift").is_none_or(|v| v != "off");
    if flag("--dry") {
        let (a, m) = if flag("--trim") {
            prep::trim_quiet(&samples, 8.0, 1.0)
        } else {
            (samples.clone(), prep::OffsetMap::identity(samples.len()))
        };
        println!(
            "{}",
            json!({"audio_secs": samples.len() as f64 / 16000.0, "decoded_secs": a.len() as f64 / 16000.0,
                   "removed_secs": m.removed_secs(samples.len()),
                   "removed": m.parts.windows(2).map(|w| json!([w[0].1.end as f64 / 16000.0, w[1].1.start as f64 / 16000.0])).collect::<Vec<_>>()})
        );
        return;
    }
    let whisper = WhisperTranscriber::load_with(&model, &model_id, true, false).unwrap();
    let configured = cm_ai::pipeline::Configured {
        inner: &whisper,
        vad: None,
        gate: None,
        normalize: (lift_call && !flag("--region-lift"))
            .then(cm_ai::pipeline::Normalize::default),
    };
    let region = RegionLifted(&whisper);
    let engine: &dyn Transcriber = if flag("--region-lift") {
        &region
    } else {
        &configured
    };

    // Trim.
    let (audio, map) = if flag("--trim") {
        prep::trim_quiet(
            &samples,
            option(&args, "--min-silence")
                .and_then(|v| v.parse().ok())
                .unwrap_or(8.0),
            option(&args, "--keep")
                .and_then(|v| v.parse().ok())
                .unwrap_or(1.0),
        )
    } else {
        (samples.clone(), prep::OffsetMap::identity(samples.len()))
    };
    let began = Instant::now();
    let mut tr: Transcript;
    let mut pieces = 0;
    if flag("--after-speech") {
        let mut words = Vec::new();
        let mut last: Option<Transcript> = None;
        for r in prep::plan_after_speech(&audio) {
            let (t, rep) = transcribe_pieces(
                engine,
                Pcm16k {
                    samples: &audio[r.clone()],
                    start: media_secs(r.start as f64 / 16_000.0),
                },
                Some(&language),
                None,
                &mut |_| true,
            )
            .unwrap();
            pieces += rep.pieces;
            words.extend(t.words.clone());
            last = Some(t);
        }
        tr = last.unwrap();
        tr.words = words;
    } else {
        let (t, rep) = transcribe_pieces(
            engine,
            Pcm16k {
                samples: &audio,
                start: MediaTime::ZERO,
            },
            Some(&language),
            None,
            &mut |_| true,
        )
        .unwrap();
        pieces = rep.pieces;
        tr = t;
    }
    let to_src = |t: MediaTime| {
        media_secs(map.to_source_secs(t.ticks() as f64 / TICKS_PER_SECOND as f64))
    };
    for w in &mut tr.words {
        w.start = to_src(w.start);
        w.end = to_src(w.end);
    }
    tr.sounds.clear();
    if let Some(s) = option(&args, "--save") {
        std::fs::write(s, serde_json::to_string(&tr).unwrap()).unwrap();
    }
    println!(
        "{}",
        json!({"words": tr.words.len(), "pieces": pieces, "audio_secs": samples.len() as f64 / 16000.0,
               "decoded_secs": audio.len() as f64 / 16000.0, "removed_secs": map.removed_secs(samples.len()),
               "wall_secs": began.elapsed().as_secs_f64(), "model": model_id})
    );
}
