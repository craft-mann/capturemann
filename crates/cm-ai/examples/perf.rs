//! One performance measurement (PERF-1, release 0.2.4): decode, transcribe or find speakers on one
//! audio file in this process, and print one JSON line. Not shipped, never downloads anything.
//!
//! ```text
//! cargo run --release -p cm-ai --example perf -- <task> <audio file> [--secs N] [--tile-to-secs N]
//!     [--model <ggml file> --model-id <id>] [--gpu on|off] [--threads N] [--language vi]
//!     [--sherpa <dir with the two speaker .onnx files>] [--label <short id for the report>]
//! ```
//!
//! Tasks: `decode` (FFmpeg to 16 kHz mono), `transcribe` (decode, load, then the app's piece-by-piece
//! transcription), `speakers` (decode, then the app's speaker finding with the speaker count unknown).
//! `--secs` keeps the first N seconds; `--tile-to-secs` repeats the audio end to end up to N seconds
//! (how a short file stands in for a long one). Peak memory is the process's, so the driver
//! (`tests/tools/perf_matrix.py`) starts one process per configuration. The audio's name never
//! appears in the output, only `--label`.

#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::time::Instant;

use cm_ai::audio::decode_16k_mono;
use cm_ai::model::{Catalog, ModelStore};
use cm_ai::pieces::transcribe_pieces;
use cm_ai::speaker_engine::SherpaDiarizer;
use cm_ai::speakers::{diarize_audio, DiarizeHints, DiarizeInput, DiarizeOptions};
use cm_ai::transcribe::Pcm16k;
use cm_ai::whisper::WhisperTranscriber;
use cm_core::MediaTime;
use serde_json::json;

fn option(args: &[String], flag: &str) -> Option<String> {
    let i = args.iter().position(|a| a == flag)?;
    args.get(i + 1).cloned()
}

fn speaker_store(dir: &Path, models: &Path) -> ModelStore {
    let store = ModelStore::with_catalog(dir, Catalog::builtin());
    std::fs::create_dir_all(store.models_dir()).unwrap();
    for (id, name) in [
        (
            cm_ai::speaker_engine::SEGMENTATION_ID,
            "pyannote-segmentation-3-0.onnx",
        ),
        (
            cm_ai::speaker_engine::EMBEDDING_ID,
            "campplus-sv-zh-en-16k-advanced.onnx",
        ),
    ] {
        let file = &store.catalog().get(id).unwrap().file;
        std::os::unix::fs::symlink(
            models.join(name).canonicalize().unwrap(),
            store.models_dir().join(file),
        )
        .unwrap();
    }
    store
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let task = args.first().expect("task: decode | transcribe | speakers");
    let audio_path = PathBuf::from(args.get(1).expect("audio file"));
    let label = option(&args, "--label").unwrap_or_else(|| "audio".into());

    let began = Instant::now();
    let mut samples = decode_16k_mono(&audio_path).unwrap();
    let decode_secs = began.elapsed().as_secs_f64();
    if let Some(n) = option(&args, "--secs").and_then(|s| s.parse::<f64>().ok()) {
        samples.truncate((n * 16_000.0) as usize);
    }
    if let Some(n) = option(&args, "--tile-to-secs").and_then(|s| s.parse::<f64>().ok()) {
        let want = (n * 16_000.0) as usize;
        let one = samples.clone();
        while samples.len() < want {
            let room = want - samples.len();
            samples.extend_from_slice(&one[..one.len().min(room)]);
        }
    }
    let audio_secs = samples.len() as f64 / 16_000.0;

    let mut out = json!({
        "task": task, "label": label, "audio_secs": audio_secs, "decode_secs": decode_secs,
    });
    let work_started = Instant::now();
    match task.as_str() {
        "decode" => {}
        "transcribe" => {
            let model = PathBuf::from(option(&args, "--model").expect("--model <ggml file>"));
            let model_id = option(&args, "--model-id").expect("--model-id <id>");
            let gpu = option(&args, "--gpu").is_none_or(|g| g != "off");
            let loaded = Instant::now();
            let mut t = WhisperTranscriber::load(&model, &model_id, gpu).unwrap();
            if let Some(n) = option(&args, "--threads").and_then(|s| s.parse().ok()) {
                t = t.with_threads(n);
            }
            let load_secs = loaded.elapsed().as_secs_f64();
            let language = option(&args, "--language");
            let began = Instant::now();
            let (tr, report) = transcribe_pieces(
                &t,
                Pcm16k {
                    samples: &samples,
                    start: MediaTime::ZERO,
                },
                language.as_deref(),
                None,
                &mut |_| true,
            )
            .unwrap();
            let secs = began.elapsed().as_secs_f64();
            // The words go to a file only when asked (to compare two runs word for word).
            if let Some(p) = option(&args, "--words-out") {
                std::fs::write(p, tr.text()).unwrap();
            }
            out["model"] = json!(model_id);
            out["gpu"] = json!(gpu);
            out["threads"] = json!(t.threads());
            out["load_secs"] = json!(load_secs);
            out["transcribe_secs"] = json!(secs);
            out["rtf"] = json!(audio_secs / secs);
            out["words"] = json!(tr.words.len());
            out["language"] = json!(tr.language);
            out["pieces"] = json!(report.pieces);
            out["split"] = json!(report.split);
            out["repeats_dropped"] = json!(report.repeats_dropped);
        }
        "speakers" => {
            let models = PathBuf::from(option(&args, "--sherpa").expect("--sherpa <dir>"));
            let dir = tempfile::tempdir().unwrap();
            let d = SherpaDiarizer::new(speaker_store(dir.path(), &models));
            let input = DiarizeInput {
                mix: &samples,
                mic: None,
                system: None,
                start: MediaTime::ZERO,
                gaps: &[],
            };
            let opts = DiarizeOptions {
                hints: DiarizeHints::default(),
                headset: false,
                display_name: "Me",
                created: "2026-10-29T00:00:00Z",
            };
            let began = Instant::now();
            let r = diarize_audio(&d, &input, &opts, &mut |_| true).unwrap();
            let secs = began.elapsed().as_secs_f64();
            out["speakers_secs"] = json!(secs);
            out["rtf"] = json!(audio_secs / secs);
            out["speakers"] = json!(r
                .file
                .segments
                .iter()
                .map(|s| s.speaker)
                .collect::<std::collections::BTreeSet<_>>()
                .len());
            out["turns"] = json!(r.file.segments.len());
        }
        other => panic!("unknown task {other}"),
    }
    out["work_secs"] = json!(work_started.elapsed().as_secs_f64());
    out["peak_rss_mb"] = json!(cm_core::perf::peak_rss_bytes().map(|b| b / 1_000_000));
    out["phases"] = json!(cm_core::perf::records()
        .iter()
        .map(|r| (
            r.run.clone(),
            json!(r
                .phases
                .iter()
                .map(|p| (p.name.clone(), json!({"secs": p.secs, "count": p.count})))
                .collect::<serde_json::Map<_, _>>())
        ))
        .collect::<serde_json::Map<_, _>>());
    println!("{out}");
}
