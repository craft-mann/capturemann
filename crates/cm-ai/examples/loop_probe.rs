//! Read-only (not shipped): how many short sentences are repeated 3 or more times in a row in a
//! bundle's transcript, and what the audio and the speaker engine say under them. Prints counts and
//! times only, never text.
//!
//! ```text
//! cargo run --release -p cm-ai --example loop_probe -- <bundle.cmrec> [--min 3]
//! ```

#![allow(clippy::unwrap_used)]

use std::path::Path;

use cm_ai::audio::decode_16k_mono;
use cm_ai::failure_kinds::sentence_runs;
use cm_ai::recording::read_transcript;
use serde_json::json;

fn value_of_args(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let bundle = Path::new(args.first().expect("<bundle>"));
    let min: usize = args
        .iter()
        .position(|a| a == "--min")
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(3);
    // --transcribe <ggml> --model-id <id>: transcribe the bundle again now (the app's job path,
    // nothing is stored) and look at that transcript instead of the stored one.
    let value_of = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    // --windows <ggml> --model-id <id> [--pad 40]: read the audio around each looped run of the
    // stored transcript on its own with the plain decoder (no repair, no pieces) and print how
    // many looped runs each reading has. Whisper's parameters come from CM_EXP when the build has
    // that measurement hook; this binary does not ship.
    if let (Some(model), Some(id)) = (
        value_of_args(&args, "--windows"),
        value_of_args(&args, "--model-id"),
    ) {
        let pad: f64 = value_of_args(&args, "--pad")
            .and_then(|v| v.parse().ok())
            .unwrap_or(40.0);
        let stored = read_transcript(bundle).unwrap().expect("a transcript");
        let engine =
            cm_ai::whisper::WhisperTranscriber::load(Path::new(&model), &id, true).unwrap();
        let name = {
            let m = cm_library::rec::read_manifest(bundle).unwrap();
            m.tracks
                .get("mic")
                .and_then(|t| t.get("denoised_file").or_else(|| t.get("file")))
                .and_then(|v| v.as_str())
                .unwrap()
                .to_owned()
        };
        let audio = decode_16k_mono(&bundle.join(name)).unwrap();
        let sec = |m: cm_core::MediaTime| m.ticks() as f64 / cm_core::TICKS_PER_SECOND as f64;
        let mut rows = Vec::new();
        for r in sentence_runs(&stored.words, 3) {
            let (a, b) = (
                sec(stored.words[r.first].start),
                sec(stored.words[r.end - 1].end),
            );
            let (from, to) = ((a - pad).max(0.0), b + pad);
            let (sa, sb) = (
                (from * 16_000.0) as usize,
                ((to * 16_000.0) as usize).min(audio.len()),
            );
            let out = cm_ai::transcribe::Transcriber::transcribe(
                &engine,
                cm_ai::transcribe::Pcm16k {
                    samples: &audio[sa..sb],
                    start: cm_core::MediaTime::ZERO,
                },
                Some("vi"),
                &mut |_| true,
            )
            .unwrap();
            let runs = sentence_runs(&out.words, 3);
            rows.push(json!({
                "at_s": a.round(), "stored_words": r.end - r.first,
                "window_words": out.words.len(), "window_runs": runs.len(),
                "window_words_in_runs": runs.iter().map(|x| x.end - x.first).sum::<usize>(),
                "dense_words": cm_ai::failure_kinds::dense_unit_indices(&out.words, 5, 60.0).len(),
            }));
        }
        println!("{}", json!({ "windows": rows }));
        return;
    }
    // --segments <ggml> --model-id <id> --from S --to S: decode that part of the noise-reduced
    // track with the plain decoder and print, for every segment, its numbers: no-speech
    // probability, mean token log-probability, mean word confidence, words, the raw level and the
    // share of its span in speech-like level. Numbers only.
    if let (Some(model), Some(id)) = (
        value_of_args(&args, "--segments"),
        value_of_args(&args, "--model-id"),
    ) {
        let from: f64 = value_of_args(&args, "--from")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.0);
        let to: f64 = value_of_args(&args, "--to")
            .and_then(|v| v.parse().ok())
            .unwrap_or(1e9);
        let manifest = cm_library::rec::read_manifest(bundle).unwrap();
        let file = |k: &str| {
            manifest
                .tracks
                .get("mic")
                .and_then(|t| t.get(k))
                .and_then(|v| v.as_str())
                .map(str::to_owned)
        };
        let den =
            decode_16k_mono(&bundle.join(file("denoised_file").or_else(|| file("file")).unwrap()))
                .unwrap();
        let raw = decode_16k_mono(&bundle.join(file("raw_file").or_else(|| file("file")).unwrap()))
            .unwrap();
        let (i, j) = (
            (from * 16_000.0) as usize,
            ((to * 16_000.0) as usize).min(den.len()),
        );
        let engine =
            cm_ai::whisper::WhisperTranscriber::load(Path::new(&model), &id, true).unwrap();
        let out = cm_ai::transcribe::Transcriber::transcribe(
            &engine,
            cm_ai::transcribe::Pcm16k {
                samples: &den[i..j],
                start: cm_core::MediaTime::ZERO,
            },
            Some("vi"),
            &mut |_| true,
        )
        .unwrap();
        let regions = cm_ai::vad::speech_regions(&raw[i..j], 0.3);
        let sec = |m: cm_core::MediaTime| m.ticks() as f64 / cm_core::TICKS_PER_SECOND as f64;
        let rows: Vec<_> = out
            .segments
            .iter()
            .map(|s| {
                let (a, b) = (sec(s.start), sec(s.end));
                let words: Vec<&cm_ai::transcribe::Word> = out.words.iter().filter(|w| sec(w.start) >= a - 0.01 && sec(w.start) < b + 0.01).collect();
                let conf = words.iter().map(|w| w.confidence).sum::<f32>() / words.len().max(1) as f32;
                let (x, y) = ((a * 16_000.0) as usize, ((b * 16_000.0) as usize).min(raw[i..j].len()));
                let sl = &raw[i..j][x.min(y)..y];
                let level = 20.0 * f64::from(((sl.iter().map(|v| v * v).sum::<f32>() / sl.len().max(1) as f32).sqrt()).max(1e-9)).log10();
                let inside: usize = regions.iter().map(|&(p, q)| q.min(y).saturating_sub(p.max(x))).sum();
                json!({
                    "at_s": ((a + from) * 10.0).round() / 10.0, "secs": ((b - a) * 10.0).round() / 10.0,
                    "no_speech": s.no_speech, "avg_logprob": s.avg_logprob, "words": words.len(),
                    "mean_conf": conf, "raw_dbfs": level, "vad_share": inside as f64 / (y - x).max(1) as f64,
                })
            })
            .collect();
        println!("{}", json!({ "segments": rows }));
        return;
    }
    // --around <seconds> [--pad 20]: sentence units of the stored transcript near that time with
    // their duration, word confidence, audio level (raw and noise-reduced), the share of the unit
    // that lies in speech-like level (`vad::speech_regions` of the whole file) and the speaker
    // engine's speech under it. Numbers only, no text.
    if let Some(at) = value_of_args(&args, "--around").and_then(|v| v.parse::<f64>().ok()) {
        let pad: f64 = value_of_args(&args, "--pad")
            .and_then(|v| v.parse().ok())
            .unwrap_or(20.0);
        let stored = read_transcript(bundle).unwrap().expect("a transcript");
        let manifest = cm_library::rec::read_manifest(bundle).unwrap();
        let file = |k: &str| {
            manifest
                .tracks
                .get("mic")
                .and_then(|t| t.get(k))
                .and_then(|v| v.as_str())
                .map(str::to_owned)
        };
        let raw = decode_16k_mono(&bundle.join(file("raw_file").or_else(|| file("file")).unwrap()))
            .unwrap();
        let den =
            decode_16k_mono(&bundle.join(file("denoised_file").or_else(|| file("file")).unwrap()))
                .unwrap();
        let regions_raw = cm_ai::vad::speech_regions(&raw, 0.3);
        let regions_den = cm_ai::vad::speech_regions(&den, 0.3);
        let segs: Vec<cm_ai::speakers::Segment> = manifest
            .extra
            .get(cm_ai::speakers::MANIFEST_SEGMENTS)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        let sec = |m: cm_core::MediaTime| m.ticks() as f64 / cm_core::TICKS_PER_SECOND as f64;
        let level = |x: &[f32], a: f64, b: f64| {
            let (i, j) = (
                (a * 16_000.0) as usize,
                ((b * 16_000.0) as usize).min(x.len()),
            );
            let sl = &x[i.min(j)..j];
            20.0 * f64::from(
                ((sl.iter().map(|s| s * s).sum::<f32>() / sl.len().max(1) as f32).sqrt()).max(1e-9),
            )
            .log10()
        };
        let share = |regions: &[(usize, usize)], a: f64, b: f64| {
            let (i, j) = ((a * 16_000.0) as usize, (b * 16_000.0) as usize);
            let inside: usize = regions
                .iter()
                .map(|&(x, y)| y.min(j).saturating_sub(x.max(i)))
                .sum();
            inside as f64 / (j - i).max(1) as f64
        };
        let mut rows = Vec::new();
        let mut start = 0;
        for (i, w) in stored.words.iter().enumerate() {
            let ends = w.text.trim_end().ends_with(['.', '?', '!', '…']);
            if ends || i + 1 - start >= 6 || i + 1 == stored.words.len() {
                let (a, b) = (sec(stored.words[start].start), sec(stored.words[i].end));
                if a > at - pad && a < at + pad {
                    let conf = stored.words[start..=i]
                        .iter()
                        .map(|w| w.confidence)
                        .sum::<f32>()
                        / (i + 1 - start) as f32;
                    let spk: f64 = segs
                        .iter()
                        .map(|s| (sec(s.end).min(b) - sec(s.start).max(a)).max(0.0))
                        .sum();
                    rows.push(json!({
                        "at_s": (a * 10.0).round() / 10.0, "secs": ((b - a) * 10.0).round() / 10.0, "words": i + 1 - start,
                        "mean_confidence": conf, "raw_dbfs": level(&raw, a, b), "denoised_dbfs": level(&den, a, b),
                        "vad_share_raw": share(&regions_raw, a, b), "vad_share_denoised": share(&regions_den, a, b),
                        "speaker_engine_secs": spk,
                    }));
                }
                start = i + 1;
            }
        }
        println!("{}", json!({ "units": rows }));
        return;
    }
    let t = if let (Some(model), Some(id)) = (value_of("--transcribe"), value_of("--model-id")) {
        let engine =
            cm_ai::whisper::WhisperTranscriber::load(Path::new(&model), &id, true).unwrap();
        let opts = cm_ai::recording::TranscribeOptions {
            language: value_of("--language").as_deref().map(|_| "vi"),
            model_id: Some(&id),
            ..Default::default()
        };
        cm_ai::recording::transcribe_recording(bundle, &engine, &opts, &mut |_| true).unwrap()
    } else {
        read_transcript(bundle).unwrap().expect("a transcript")
    };
    let manifest = cm_library::rec::read_manifest(bundle).unwrap();
    let raw = manifest
        .tracks
        .get("mic")
        .and_then(|m| m.get("raw_file").or_else(|| m.get("file")))
        .and_then(|v| v.as_str())
        .unwrap();
    let audio = decode_16k_mono(&bundle.join(raw)).unwrap();
    let segs: Vec<cm_ai::speakers::Segment> = manifest
        .extra
        .get(cm_ai::speakers::MANIFEST_SEGMENTS)
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    let secs = |m: cm_core::MediaTime| m.ticks() as f64 / cm_core::TICKS_PER_SECOND as f64;
    let runs = sentence_runs(&t.words, min);
    let mut rows = Vec::new();
    let (mut words_in, mut secs_in, mut speaker_in) = (0usize, 0.0, 0.0);
    for r in &runs {
        let (a, b) = (secs(t.words[r.first].start), secs(t.words[r.end - 1].end));
        let (ia, ib) = (
            (a * 16_000.0) as usize,
            ((b * 16_000.0) as usize).min(audio.len()),
        );
        let slice = &audio[ia.min(ib)..ib];
        let rms = (slice.iter().map(|s| s * s).sum::<f32>() / slice.len().max(1) as f32).sqrt();
        // Speech seconds of the speaker engine inside the run.
        let spk: f64 = segs
            .iter()
            .map(|s| (secs(s.end).min(b) - secs(s.start).max(a)).max(0.0))
            .sum();
        words_in += r.end - r.first;
        secs_in += b - a;
        speaker_in += spk;
        rows.push(json!({
            "at_s": a.round(), "words": r.end - r.first, "unit_words": r.unit_words, "count": r.count,
            "secs": (b - a * 1.0).max(0.0) - 0.0, "words_per_s": (r.end - r.first) as f64 / (b - a).max(0.1),
            "level_dbfs": 20.0 * f64::from(rms.max(1e-9)).log10(),
            "speaker_engine_secs": spk,
        }));
    }
    let dense = cm_ai::failure_kinds::dense_unit_indices(&t.words, 5, 60.0).len();
    println!(
        "{}",
        json!({
            "dense_repeat_words": dense, "sounds": t.sounds.len(),
            "sounds_secs": t.sounds.iter().map(|s| secs(s.end) - secs(s.start)).sum::<f64>(),
            "transcript_words": t.words.len(), "min_count": min, "runs": runs.len(),
            "words_in_runs": words_in, "secs_in_runs": secs_in, "speaker_engine_secs_in_runs": speaker_in,
            "rows": rows,
        })
    );
}
