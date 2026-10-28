//! Baseline accuracy run for the release 0.2.3 work (not shipped, never downloads anything).
//!
//! ```text
//! cargo run --release -p cm-ai --example baseline -- <clips dir> \
//!   --sherpa <dir with the two .onnx files> [--whisper <ggml model>] [--language en] [--out <dir>]
//! ```
//!
//! For every `<name>.wav` in the folder with a `<name>.rttm.txt` (and `<name>.txt`) from
//! `tests/tools/make_reference_clips.py` or `measure draft`: finds the speakers the way the app
//! does (`diarize_audio`, both with the speaker count unknown and with the true count) and prints
//! DER against the reference; with `--whisper` it also transcribes piece by piece like the app
//! (`transcribe_pieces`) and prints WER against `<name>.txt`. Results are also written to
//! `<out>/<name>.found.rttm.txt` and `<out>/<name>.hyp.txt` (default: next to the clips).

#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::time::Instant;

use cm_ai::audio::decode_16k_mono;
use cm_ai::measure::{diarization_errors, parse_turns, word_errors, Turn};
use cm_ai::model::{Catalog, ModelStore};
use cm_ai::pieces::transcribe_pieces;
use cm_ai::speaker_engine::SherpaDiarizer;
use cm_ai::speakers::{diarize_audio, DiarizeHints, DiarizeInput, DiarizeOptions};
use cm_ai::transcribe::Pcm16k;
use cm_ai::whisper::WhisperTranscriber;
use cm_core::MediaTime;

fn option(args: &[String], flag: &str) -> Option<String> {
    let i = args.iter().position(|a| a == flag)?;
    args.get(i + 1).cloned()
}

fn model_store(dir: &Path, models: &Path) -> ModelStore {
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
                "campplus-sv-en-voxceleb-16k.onnx",
                "3dspeaker_speech_campplus_sv_en_voxceleb_16k.onnx",
            ][..],
        ),
    ] {
        let from = names
            .iter()
            .map(|n| models.join(n))
            .find(|p| p.is_file())
            .unwrap_or_else(|| panic!("none of {names:?} in {}", models.display()));
        let file = &store.catalog().get(id).unwrap().file;
        std::os::unix::fs::symlink(from.canonicalize().unwrap(), store.models_dir().join(file))
            .unwrap();
    }
    store
}

fn found_turns(samples: &[f32], models: &Path, count: Option<u32>) -> (Vec<Turn>, f64) {
    let dir = tempfile::tempdir().unwrap();
    let d = SherpaDiarizer::new(model_store(dir.path(), models));
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
        eprintln!("usage: baseline <clips dir> --sherpa <models dir> [--whisper <ggml>] [--language en] [--out <dir>]");
        std::process::exit(2);
    };
    let clips = PathBuf::from(clips);
    let out = option(&args, "--out").map_or(clips.clone(), PathBuf::from);
    std::fs::create_dir_all(&out).unwrap();
    let whisper = option(&args, "--whisper").map(|m| {
        WhisperTranscriber::load(Path::new(&m), "baseline", true).expect("load the whisper model")
    });
    let language = option(&args, "--language");

    let mut wavs: Vec<PathBuf> = std::fs::read_dir(&clips)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "wav"))
        .collect();
    wavs.sort();
    println!("| clip | s | speakers | DER auto | found | DER true count | WER | time |");
    println!("|---|---|---|---|---|---|---|---|");
    for wav in wavs {
        let name = wav.file_stem().unwrap().to_string_lossy().into_owned();
        let Ok(rttm) = std::fs::read_to_string(clips.join(format!("{name}.rttm.txt"))) else {
            continue;
        };
        let reference = parse_turns(&rttm).unwrap();
        let samples = decode_16k_mono(&wav).unwrap();
        let secs = samples.len() as f64 / 16_000.0;
        let truth = diarization_errors(&reference, &[], 0.25).reference_speakers;

        let (auto, t_auto) = found_turns(&samples, Path::new(&sherpa), None);
        let (exact, t_exact) = found_turns(&samples, Path::new(&sherpa), Some(truth as u32));
        let d_auto = diarization_errors(&reference, &auto, 0.25);
        let d_exact = diarization_errors(&reference, &exact, 0.25);
        let save = |turns: &[Turn], file: String| {
            let text: String = turns
                .iter()
                .map(|t| format!("{:.3} {:.3} {}\n", t.start, t.end - t.start, t.speaker))
                .collect();
            std::fs::write(out.join(file), text).unwrap();
        };
        save(&auto, format!("{name}.found.rttm.txt"));
        save(&exact, format!("{name}.found-n.rttm.txt"));

        let mut wer = "-".to_owned();
        let mut t_wer = 0.0;
        if let (Some(w), Ok(text)) = (
            &whisper,
            std::fs::read_to_string(clips.join(format!("{name}.txt"))),
        ) {
            let began = Instant::now();
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
            t_wer = began.elapsed().as_secs_f64();
            let hyp = t
                .words
                .iter()
                .map(|w| w.text.trim())
                .collect::<Vec<_>>()
                .join(" ");
            std::fs::write(out.join(format!("{name}.hyp.txt")), &hyp).unwrap();
            let e = word_errors(&text, &hyp);
            wer = format!(
                "{:.1} % (S{} I{} D{})",
                e.wer() * 100.0,
                e.substitutions,
                e.insertions,
                e.deletions
            );
        }
        println!(
            "| {name} | {secs:.0} | {truth} | {:.1} % (miss {:.1}, fa {:.1}, conf {:.1}) | {} | {:.1} % | {wer} | diar {t_auto:.0}+{t_exact:.0} s, asr {t_wer:.0} s |",
            d_auto.der() * 100.0,
            d_auto.missed,
            d_auto.false_alarm,
            d_auto.confusion,
            d_auto.found_speakers,
            d_exact.der() * 100.0,
        );
    }
}
