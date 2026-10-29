//! Real-model check of the speaker engine (AI-8, spike S7). Ignored by default: it needs the two
//! speaker model files and a 16 kHz mono 16-bit WAV, and it never downloads anything.
//!
//! ```text
//! CM_SHERPA_MODELS=<dir with the two .onnx files> CM_DIARIZE_AUDIO=<speech.wav> \
//!   [CM_DIARIZE_SPEAKERS=2,3] cargo test -p cm-ai --test real_diarize -- --ignored --nocapture
//! ```
//!
//! The model folder holds the segmentation model as `pyannote-segmentation-3-0.onnx` (or the
//! archive's own name `model.onnx`) and the embedding model as `campplus-sv-zh-en-16k-advanced.onnx`
//! (or `3dspeaker_speech_campplus_sv_zh_en_16k-common_advanced.onnx`). Convert audio with
//! `ffmpeg -i in.m4a -ac 1 -ar 16000 -c:a pcm_s16le out.wav`. The test prints the number of
//! turns and speakers, the speech per speaker and the time; it checks that the output is
//! well-formed (sorted, numbered by first speech, no negative spans), never accuracy: there is no
//! ground truth here. Peak memory: run the test binary under `/usr/bin/time -l`.
//! Without the variables, or without the `sherpa` feature, it prints SKIPPED and passes.

#![cfg(feature = "sherpa")]
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::time::Instant;

use cm_ai::model::{Catalog, ModelStore};
use cm_ai::speaker_engine::SherpaDiarizer;
use cm_ai::speakers::{diarize_audio, DiarizeHints, DiarizeInput, DiarizeOptions};
use cm_core::MediaTime;

fn find(dir: &Path, names: &[&str]) -> PathBuf {
    names
        .iter()
        .map(|n| dir.join(n))
        .find(|p| p.is_file())
        .unwrap_or_else(|| panic!("none of {names:?} in {}", dir.display()))
}

fn read_wav(path: &Path) -> Vec<f32> {
    let mut r = hound::WavReader::open(path).unwrap();
    let spec = r.spec();
    assert_eq!(spec.channels, 1, "mono WAV expected");
    assert_eq!(spec.sample_rate, 16_000, "16 kHz WAV expected");
    assert_eq!(spec.bits_per_sample, 16, "16-bit WAV expected");
    r.samples::<i16>()
        .map(|s| f32::from(s.unwrap()) / 32768.0)
        .collect()
}

#[test]
#[ignore = "needs CM_SHERPA_MODELS and CM_DIARIZE_AUDIO"]
fn real_models_tell_speakers_apart() {
    let (Some(models), Some(audio)) = (
        std::env::var_os("CM_SHERPA_MODELS"),
        std::env::var_os("CM_DIARIZE_AUDIO"),
    ) else {
        println!("SKIPPED: set CM_SHERPA_MODELS and CM_DIARIZE_AUDIO");
        return;
    };
    let models = PathBuf::from(models);
    let seg = find(&models, &["pyannote-segmentation-3-0.onnx", "model.onnx"]);
    let emb = find(
        &models,
        &[
            "campplus-sv-zh-en-16k-advanced.onnx",
            "3dspeaker_speech_campplus_sv_zh_en_16k-common_advanced.onnx",
        ],
    );
    // A store whose files are links to the given ones (nothing is copied).
    let dir = tempfile::tempdir().unwrap();
    let store = ModelStore::with_catalog(dir.path(), Catalog::builtin());
    std::fs::create_dir_all(store.models_dir()).unwrap();
    for (id, from) in [
        (cm_ai::speaker_engine::SEGMENTATION_ID, &seg),
        (cm_ai::speaker_engine::EMBEDDING_ID, &emb),
    ] {
        let file = &store.catalog().get(id).unwrap().file;
        std::os::unix::fs::symlink(from, store.models_dir().join(file)).unwrap();
    }

    let samples = read_wav(Path::new(&audio));
    let seconds = samples.len() as f64 / 16_000.0;
    println!("audio {seconds:.1} s");
    let counts = std::env::var("CM_DIARIZE_SPEAKERS").unwrap_or_else(|_| "2,3".into());
    for n in counts.split(',').map(|s| s.trim()) {
        let hints = if n == "auto" {
            DiarizeHints::default()
        } else {
            DiarizeHints {
                num_speakers: Some(n.parse().unwrap()),
                ..Default::default()
            }
        };
        let d = SherpaDiarizer::new(ModelStore::with_catalog(dir.path(), Catalog::builtin()));
        let input = DiarizeInput {
            mix: &samples,
            mic: None,
            system: None,
            start: MediaTime::ZERO,
            gaps: &[],
        };
        let opts = DiarizeOptions {
            hints,
            headset: false,
            display_name: "Me",
            created: "2026-10-27T00:00:00Z",
        };
        let t = Instant::now();
        let out = diarize_audio(&d, &input, &opts, &mut |_| true).unwrap();
        let took = t.elapsed().as_secs_f64();
        let segs = &out.file.segments;
        println!(
            "num_speakers={n}: {} speakers, {} turns, {took:.1} s (RTF {:.3})",
            out.speakers.len(),
            segs.len(),
            took / seconds
        );
        for s in &out.speakers {
            let secs: f64 = segs
                .iter()
                .filter(|g| g.speaker == s.id)
                .map(|g| (g.end - g.start).as_secs_f64())
                .sum();
            let turns = segs.iter().filter(|g| g.speaker == s.id).count();
            println!("  {} (id {}): {secs:.1} s in {turns} turns", s.name, s.id);
        }
        let short = segs
            .iter()
            .filter(|g| (g.end - g.start).as_secs_f64() < 0.5)
            .count();
        println!("  turns under 0.5 s: {short}");
        // Well-formed output, not accuracy.
        assert!(segs.iter().all(|g| g.end > g.start));
        assert!(segs.windows(2).all(|w| w[0].start <= w[1].start));
        if let Ok(k) = n.parse::<usize>() {
            assert!(out.speakers.len() <= k);
        }
        let mut first_seen: Vec<u32> = Vec::new();
        for g in segs {
            if !first_seen.contains(&g.speaker) {
                first_seen.push(g.speaker);
            }
        }
        let mut sorted = first_seen.clone();
        sorted.sort_unstable();
        assert_eq!(first_seen, sorted, "numbered by first speech");
    }
}
