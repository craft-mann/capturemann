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
    args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned()
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
    if let (Some(model), Some(id)) = (value_of_args(&args, "--windows"), value_of_args(&args, "--model-id")) {
        let pad: f64 = value_of_args(&args, "--pad").and_then(|v| v.parse().ok()).unwrap_or(40.0);
        let stored = read_transcript(bundle).unwrap().expect("a transcript");
        let engine = cm_ai::whisper::WhisperTranscriber::load(Path::new(&model), &id, true).unwrap();
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
            let (a, b) = (sec(stored.words[r.first].start), sec(stored.words[r.end - 1].end));
            let (from, to) = ((a - pad).max(0.0), b + pad);
            let (sa, sb) = ((from * 16_000.0) as usize, ((to * 16_000.0) as usize).min(audio.len()));
            let out = cm_ai::transcribe::Transcriber::transcribe(
                &engine,
                cm_ai::transcribe::Pcm16k { samples: &audio[sa..sb], start: cm_core::MediaTime::ZERO },
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
