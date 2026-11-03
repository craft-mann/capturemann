//! Research probe (not shipped, M2j step 5): runs a sherpa-onnx offline transducer (a Vietnamese
//! Zipformer) over single utterances with the recognizer that is already linked for speakers, and
//! prints WER, real-time factor and whether token times come back. It downloads nothing.
//!
//! ```text
//! cargo run --release -p cm-ai --example zipformer_probe -- --model <dir with encoder/decoder/joiner/tokens>
//!     --list <path<TAB>reference per line> [--skip N] [--count N]
//! ```

#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::time::Instant;

use cm_ai::audio::decode_16k_mono;
use cm_ai::measure::word_errors;
use sherpa_onnx::{OfflineRecognizer, OfflineRecognizerConfig, OfflineTransducerModelConfig};

fn option(args: &[String], flag: &str) -> Option<String> {
    let i = args.iter().position(|a| a == flag)?;
    args.get(i + 1).cloned()
}

fn find(dir: &Path, prefix: &str) -> String {
    let mut hits: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(prefix) && n.ends_with(".onnx"))
        })
        .collect();
    // The int8 file when there is one.
    hits.sort_by_key(|p| !p.to_string_lossy().contains("int8"));
    hits[0].to_string_lossy().into_owned()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dir = PathBuf::from(option(&args, "--model").expect("--model <dir>"));
    let list = std::fs::read_to_string(option(&args, "--list").expect("--list")).unwrap();
    let skip: usize = option(&args, "--skip")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let count: usize = option(&args, "--count")
        .and_then(|v| v.parse().ok())
        .unwrap_or(20);
    let mut config = OfflineRecognizerConfig::default();
    config.model_config.transducer = OfflineTransducerModelConfig {
        encoder: Some(find(&dir, "encoder")),
        decoder: Some(find(&dir, "decoder")),
        joiner: Some(find(&dir, "joiner")),
    };
    config.model_config.tokens = Some(dir.join("tokens.txt").to_string_lossy().into_owned());
    config.model_config.num_threads = 4;
    config.decoding_method = Some("greedy_search".into());
    let began = Instant::now();
    let recognizer = OfflineRecognizer::create(&config).expect("recognizer");
    let load = began.elapsed().as_secs_f64();
    let (mut reference, mut hypothesis) = (String::new(), String::new());
    let (mut audio_secs, mut decode_secs) = (0.0, 0.0);
    let mut with_times = 0;
    let mut sample = String::new();
    for line in list.lines().skip(skip).take(count) {
        let Some((path, text)) = line.split_once('\t') else {
            continue;
        };
        let samples = decode_16k_mono(Path::new(path)).unwrap();
        audio_secs += samples.len() as f64 / 16_000.0;
        let t = Instant::now();
        let stream = recognizer.create_stream();
        stream.accept_waveform(16_000, &samples);
        recognizer.decode(&stream);
        decode_secs += t.elapsed().as_secs_f64();
        let r = stream.get_result().unwrap();
        if r.timestamps.as_ref().is_some_and(|t| !t.is_empty()) {
            with_times += 1;
        }
        reference.push(' ');
        reference.push_str(text);
        hypothesis.push(' ');
        hypothesis.push_str(&r.text);
        if sample.is_empty() {
            sample = format!(
                "{} tokens, first times {:?}",
                r.tokens.len(),
                r.timestamps
                    .as_ref()
                    .map(|t| t.iter().take(3).collect::<Vec<_>>())
            );
        }
    }
    let e = word_errors(&reference, &hypothesis);
    println!(
        "{}",
        serde_json::json!({
            "model_dir": dir.file_name().unwrap().to_string_lossy(),
            "audio_secs": audio_secs, "decode_secs": decode_secs, "load_secs": load,
            "rtf": audio_secs / decode_secs, "wer": e.wer(),
            "substitutions": e.substitutions, "insertions": e.insertions, "deletions": e.deletions,
            "reference_words": e.reference_words, "utterances_with_token_times": with_times,
            "sample": sample,
        })
    );
}
