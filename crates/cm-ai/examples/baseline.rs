//! Baseline accuracy run for the release 0.2.3 work (not shipped, never downloads anything).
//!
//! ```text
//! cargo run --release -p cm-ai --example baseline -- <clips dir> \
//!   --sherpa <dir with the two .onnx files> [--whisper <ggml model>] [--language en] [--out <dir>] \
//!   [--thresholds 0.4,0.6,1.2] [--embedding <onnx>] [--no-true-count]
//! ```
//!
//! For every `<name>.wav` in the folder with a `<name>.rttm.txt` (and `<name>.txt`) from
//! `tests/tools/make_reference_clips.py` or `measure draft`: finds the speakers the way the app
//! does (`diarize_audio`, both with the speaker count unknown and with the true count) and prints
//! DER against the reference; with `--whisper` it also transcribes piece by piece like the app
//! (`transcribe_pieces`) and prints WER against `<name>.txt`, plain and without hesitation sounds.
//! `--thresholds` repeats the unknown-count run with other clustering thresholds (default: the
//! app's), `--embedding` swaps the voice-embedding model file, `--no-true-count` skips the run
//! with the true count. Results are also written to
//! `<out>/<name>.found.rttm.txt` and `<out>/<name>.hyp.txt` (default: next to the clips).

#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::time::Instant;

use cm_ai::audio::decode_16k_mono;
use cm_ai::measure::{
    diarization_errors, parse_turns, word_errors, word_errors_without, Turn, DISFLUENCIES,
};
use cm_ai::model::{Catalog, ModelStore};
use cm_ai::pieces::transcribe_pieces;
use cm_ai::speaker_engine::{
    ClusterParams, SherpaDiarizer, SherpaEngine, SpeakerModels, TurnEngine, AUTO_THRESHOLD,
};
use cm_ai::speakers::{diarize_audio, DiarizeHints, DiarizeInput, DiarizeOptions};
use cm_ai::transcribe::Pcm16k;
use cm_ai::whisper::WhisperTranscriber;
use cm_core::MediaTime;

fn option(args: &[String], flag: &str) -> Option<String> {
    let i = args.iter().position(|a| a == flag)?;
    args.get(i + 1).cloned()
}

fn model_store(dir: &Path, models: &Path, embedding: Option<&Path>) -> ModelStore {
    let store = ModelStore::with_catalog(dir, Catalog::builtin());
    std::fs::create_dir_all(store.models_dir()).unwrap();
    for (id, names) in [
        (
            cm_ai::speaker_engine::SEGMENTATION_ID,
            &["pyannote-segmentation-3-0.onnx", "model.onnx"][..],
        ),
        (
            cm_ai::speaker_engine::EMBEDDING_ID,
            &[
                "campplus-sv-zh-en-16k-advanced.onnx",
                "3dspeaker_speech_campplus_sv_zh_en_16k-common_advanced.onnx",
            ][..],
        ),
    ] {
        let from = match embedding.filter(|_| id == cm_ai::speaker_engine::EMBEDDING_ID) {
            Some(e) => e.to_path_buf(),
            None => names
                .iter()
                .map(|n| models.join(n))
                .find(|p| p.is_file())
                .unwrap_or_else(|| panic!("none of {names:?} in {}", models.display())),
        };
        let file = &store.catalog().get(id).unwrap().file;
        std::os::unix::fs::symlink(from.canonicalize().unwrap(), store.models_dir().join(file))
            .unwrap();
    }
    store
}

/// The real engine with the unknown-count clustering threshold replaced (an exact count ignores it).
struct Tuned(Option<f32>);

impl TurnEngine for Tuned {
    fn run(
        &self,
        models: &SpeakerModels,
        samples: &[f32],
        params: ClusterParams,
    ) -> Result<Vec<cm_ai::speaker_engine::Turn>, cm_core::CmError> {
        let threshold = self.0.unwrap_or(params.threshold);
        SherpaEngine::default().run(
            models,
            samples,
            ClusterParams {
                threshold,
                ..params
            },
        )
    }
}

fn found_turns(
    samples: &[f32],
    models: &Path,
    embedding: Option<&Path>,
    count: Option<u32>,
    threshold: Option<f32>,
) -> (Vec<Turn>, f64) {
    let dir = tempfile::tempdir().unwrap();
    let d = SherpaDiarizer::with_engine(
        model_store(dir.path(), models, embedding),
        std::sync::Arc::new(Tuned(threshold)),
    );
    let input = DiarizeInput {
        mix: samples,
        mic: None,
        system: None,
        start: MediaTime::ZERO,
        gaps: &[],
    };
    let opts = DiarizeOptions {
        hints: DiarizeHints {
            num_speakers: count,
            ..Default::default()
        },
        headset: false,
        display_name: "Me",
        created: "2026-10-28T00:00:00Z",
    };
    let began = Instant::now();
    let out = diarize_audio(&d, &input, &opts, &mut |_| true).unwrap();
    let took = began.elapsed().as_secs_f64();
    let turns = out
        .file
        .segments
        .iter()
        .map(|s| Turn {
            start: s.start.as_secs_f64(),
            end: s.end.as_secs_f64(),
            speaker: s.speaker.to_string(),
        })
        .collect();
    (turns, took)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(clips), Some(sherpa)) = (
        args.first().filter(|a| !a.starts_with("--")),
        option(&args, "--sherpa"),
    ) else {
        eprintln!("usage: baseline <clips dir> --sherpa <models dir> [--whisper <ggml>] [--language en] [--out <dir>] [--thresholds a,b] [--embedding <onnx>] [--no-true-count]");
        std::process::exit(2);
    };
    let clips = PathBuf::from(clips);
    let sherpa = PathBuf::from(sherpa);
    let embedding = option(&args, "--embedding").map(PathBuf::from);
    let out = option(&args, "--out").map_or(clips.clone(), PathBuf::from);
    std::fs::create_dir_all(&out).unwrap();
    let whisper = option(&args, "--whisper").map(|m| {
        WhisperTranscriber::load(Path::new(&m), "baseline", true).expect("load the whisper model")
    });
    let language = option(&args, "--language");
    let thresholds: Vec<f32> = option(&args, "--thresholds").map_or_else(
        || vec![AUTO_THRESHOLD],
        |t| t.split(',').map(|v| v.trim().parse().unwrap()).collect(),
    );
    let true_count = !args.iter().any(|a| a == "--no-true-count");

    let mut wavs: Vec<PathBuf> = std::fs::read_dir(&clips)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "wav"))
        .collect();
    wavs.sort();
    let mut head = "| clip | s | speakers |".to_owned();
    let mut rule = "|---|---|---|".to_owned();
    for t in &thresholds {
        head += &format!(" DER @{t} (found) |");
        rule += "---|";
    }
    if true_count {
        head += " DER true count |";
        rule += "---|";
    }
    if whisper.is_some() {
        head += " WER | WER without um/uh |";
        rule += "---|---|";
    }
    println!("{head}\n{rule}");
    // Sums for the mean row: one per threshold, then the true-count run, then the two WERs.
    let mut sums = vec![0.0; thresholds.len() + 3];
    let mut found_hits = vec![0usize; thresholds.len()];
    let mut clips_done = 0usize;
    for wav in wavs {
        let name = wav.file_stem().unwrap().to_string_lossy().into_owned();
        let Ok(rttm) = std::fs::read_to_string(clips.join(format!("{name}.rttm.txt"))) else {
            continue;
        };
        let reference = parse_turns(&rttm).unwrap();
        let samples = decode_16k_mono(&wav).unwrap();
        let secs = samples.len() as f64 / 16_000.0;
        let truth = diarization_errors(&reference, &[], 0.25).reference_speakers;
        let save = |turns: &[Turn], file: String| {
            let text: String = turns
                .iter()
                .map(|t| format!("{:.3} {:.3} {}\n", t.start, t.end - t.start, t.speaker))
                .collect();
            std::fs::write(out.join(file), text).unwrap();
        };
        let mut row = format!("| {name} | {secs:.0} | {truth} |");
        for (i, th) in thresholds.iter().enumerate() {
            let (turns, _) = found_turns(&samples, &sherpa, embedding.as_deref(), None, Some(*th));
            let d = diarization_errors(&reference, &turns, 0.25);
            save(&turns, format!("{name}.found-{th}.rttm.txt"));
            sums[i] += d.der();
            found_hits[i] += usize::from(d.found_speakers == truth);
            row += &format!(" {:.1} % ({}) |", d.der() * 100.0, d.found_speakers);
        }
        if true_count {
            let (turns, _) = found_turns(
                &samples,
                &sherpa,
                embedding.as_deref(),
                Some(truth as u32),
                None,
            );
            let d = diarization_errors(&reference, &turns, 0.25);
            save(&turns, format!("{name}.found-n.rttm.txt"));
            sums[thresholds.len()] += d.der();
            row += &format!(" {:.1} % |", d.der() * 100.0);
        }
        if let (Some(w), Ok(text)) = (
            &whisper,
            std::fs::read_to_string(clips.join(format!("{name}.txt"))),
        ) {
            let (t, _report) = transcribe_pieces(
                w,
                Pcm16k {
                    samples: &samples,
                    start: MediaTime::ZERO,
                },
                language.as_deref(),
                None,
                &mut |_| true,
            )
            .unwrap();
            let hyp = t
                .words
                .iter()
                .map(|w| w.text.trim())
                .collect::<Vec<_>>()
                .join(" ");
            std::fs::write(out.join(format!("{name}.hyp.txt")), &hyp).unwrap();
            let plain = word_errors(&text, &hyp).wer();
            let clean = word_errors_without(&text, &hyp, DISFLUENCIES).wer();
            sums[thresholds.len() + 1] += plain;
            sums[thresholds.len() + 2] += clean;
            row += &format!(" {:.1} % | {:.1} % |", plain * 100.0, clean * 100.0);
        }
        clips_done += 1;
        println!("{row}");
    }
    if clips_done > 0 {
        let n = clips_done as f64;
        let mut row = "| **mean** | | |".to_owned();
        for (i, h) in found_hits.iter().enumerate() {
            row += &format!(
                " {:.1} % (count right {h}/{clips_done}) |",
                sums[i] / n * 100.0
            );
        }
        if true_count {
            row += &format!(" {:.1} % |", sums[thresholds.len()] / n * 100.0);
        }
        if whisper.is_some() {
            row += &format!(
                " {:.1} % | {:.1} % |",
                sums[thresholds.len() + 1] / n * 100.0,
                sums[thresholds.len() + 2] / n * 100.0
            );
        }
        println!("{row}");
    }
}
