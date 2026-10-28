//! Accuracy numbers for the release 0.2.3 work (not shipped). Two subcommands:
//!
//! ```text
//! cargo run -p cm-ai --example measure -- wer <reference.txt> <transcript.txt|transcript.json> [--range a:b]
//! cargo run -p cm-ai --example measure -- der <reference.rttm> <result> [--collar 0.25] [--range a:b]
//! cargo run -p cm-ai --example measure -- draft <bundle.cmrec> --range a:b --out <dir> [--name n]
//! ```
//!
//! `wer` folds case, diacritics and punctuation like the repeat checks and prints the word error
//! rate with its substitutions, insertions and deletions. `der` takes a reference turn list
//! (`start duration speaker` per line, or RTTM) and a result in the same format, or a
//! `diarization.json` from a bundle, and prints the diarization error rate with missed speech,
//! false alarm, speaker confusion and the speaker counts. References are hand-made and stay out of
//! the repository; only the numbers go in `verification/`.
//!
//! `--range a:b` (seconds) limits a result to that stretch, for a reference made from part of a
//! recording: times stay those of the recording. `draft` writes what a bundle has now for that
//! stretch as `<name>.rttm.txt` (one turn per line) and `<name>.txt` (the text of line n is turn
//! n), to be corrected by hand and then used as the reference.

use std::path::Path;
use std::process::ExitCode;

use cm_ai::measure::{diarization_errors, parse_turns, word_errors, Turn};
use cm_ai::speakers::{assign_words, turns, DiarizationFile};
use cm_ai::transcribe::{Transcript, Word};

fn read(path: &str) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))
}

/// The value of `--flag value` in `args`, if given.
fn option(args: &[String], flag: &str) -> Result<Option<String>, String> {
    match args.iter().position(|a| a == flag) {
        None => Ok(None),
        Some(i) => args
            .get(i + 1)
            .cloned()
            .map(Some)
            .ok_or_else(|| format!("{flag} needs a value")),
    }
}

fn range_of(args: &[String]) -> Result<Option<(f64, f64)>, String> {
    let Some(text) = option(args, "--range")? else {
        return Ok(None);
    };
    let bad = || format!("bad range `{text}`, expected seconds like 120:420");
    let (a, b) = text.split_once(':').ok_or_else(bad)?;
    let (a, b): (f64, f64) = (a.parse().map_err(|_| bad())?, b.parse().map_err(|_| bad())?);
    if !(a >= 0.0 && b > a) {
        return Err(bad());
    }
    Ok(Some((a, b)))
}

fn json_bundle_file(path: &str, name: &str) -> String {
    let p = Path::new(path);
    if p.is_dir() {
        p.join(name).to_string_lossy().into_owned()
    } else {
        path.to_owned()
    }
}

fn transcript_of(path: &str) -> Result<Transcript, String> {
    let path = json_bundle_file(path, "transcript.json");
    serde_json::from_str(&read(&path)?).map_err(|e| format!("{path}: {e}"))
}

/// The words whose midpoint lies in the range (all words without one).
fn words_in(t: &Transcript, range: Option<(f64, f64)>) -> Vec<Word> {
    t.words
        .iter()
        .filter(|w| {
            let mid = (w.start.as_secs_f64() + w.end.as_secs_f64()) / 2.0;
            range.is_none_or(|(a, b)| mid >= a && mid < b)
        })
        .cloned()
        .collect()
}

fn text_of(path: &str, range: Option<(f64, f64)>) -> Result<String, String> {
    if path.ends_with(".json") || Path::new(path).is_dir() {
        let words = words_in(&transcript_of(path)?, range);
        return Ok(words
            .iter()
            .map(|w| w.text.trim())
            .collect::<Vec<_>>()
            .join(" "));
    }
    read(path)
}

fn turns_of(path: &str, range: Option<(f64, f64)>) -> Result<Vec<Turn>, String> {
    let is_json = Path::new(path).extension().is_some_and(|e| e == "json");
    let mut turns: Vec<Turn> = if is_json || Path::new(path).is_dir() {
        let path = json_bundle_file(path, "diarization.json");
        let file: DiarizationFile =
            serde_json::from_str(&read(&path)?).map_err(|e| format!("{path}: {e}"))?;
        file.segments
            .iter()
            .map(|s| Turn {
                start: s.start.as_secs_f64(),
                end: s.end.as_secs_f64(),
                speaker: s.speaker.to_string(),
            })
            .collect()
    } else {
        parse_turns(&read(path)?).map_err(|e| format!("{path}: {e}"))?
    };
    if let Some((a, b)) = range {
        turns = turns
            .into_iter()
            .map(|t| Turn {
                start: t.start.max(a),
                end: t.end.min(b),
                ..t
            })
            .filter(|t| t.end > t.start)
            .collect();
    }
    Ok(turns)
}

/// A draft reference from a bundle: one turn per run of words of one speaker.
fn draft(bundle: &str, range: (f64, f64), out: &str, name: &str) -> Result<(), String> {
    let transcript = transcript_of(bundle)?;
    let file: DiarizationFile = {
        let path = json_bundle_file(bundle, "diarization.json");
        serde_json::from_str(&read(&path)?).map_err(|e| format!("{path}: {e}"))?
    };
    let words = words_in(&transcript, Some(range));
    let labels = assign_words(&words, &file.segments);
    let (mut rttm, mut text) = (String::new(), String::new());
    for t in turns(&words, &labels) {
        let who = t.speaker.map_or("?".to_owned(), |s| format!("S{s}"));
        let (a, b) = (t.start.as_secs_f64(), t.end.as_secs_f64());
        rttm.push_str(&format!("{a:.3} {:.3} {who}\n", b - a));
        text.push_str(&t.text);
        text.push('\n');
    }
    std::fs::create_dir_all(out).map_err(|e| format!("{out}: {e}"))?;
    let base = Path::new(out).join(name);
    std::fs::write(base.with_extension("rttm.txt"), rttm).map_err(|e| e.to_string())?;
    std::fs::write(base.with_extension("txt"), text).map_err(|e| e.to_string())?;
    println!(
        "wrote {name}.rttm.txt and {name}.txt in {out}: {} words, {} s ({}:{}). Correct speakers and times in the first, words in the second; line n of each is one turn (lines may be added or removed together).",
        words.len(),
        range.1 - range.0,
        range.0,
        range.1
    );
    Ok(())
}

fn run(args: &[String]) -> Result<(), String> {
    let range = range_of(args)?;
    let pos: Vec<&String> = {
        // Positional arguments: everything that is neither a flag nor a flag's value.
        let mut v = Vec::new();
        let mut skip = false;
        for a in args {
            if skip {
                skip = false;
            } else if a.starts_with("--") {
                skip = true;
            } else {
                v.push(a);
            }
        }
        v
    };
    match pos.as_slice() {
        [cmd, reference, hypothesis] if *cmd == "wer" => {
            let e = word_errors(&read(reference)?, &text_of(hypothesis, range)?);
            println!(
                "WER {:.2} %  ({} words: {} substitutions, {} insertions, {} deletions)",
                e.wer() * 100.0,
                e.reference_words,
                e.substitutions,
                e.insertions,
                e.deletions
            );
            Ok(())
        }
        [cmd, reference, found] if *cmd == "der" => {
            let collar = match option(args, "--collar")? {
                None => 0.25,
                Some(v) => v.parse().map_err(|_| format!("bad collar `{v}`"))?,
            };
            let d = diarization_errors(&turns_of(reference, None)?, &turns_of(found, range)?, collar);
            println!(
                "DER {:.2} %  (scored {:.1} s: missed {:.1} s, false alarm {:.1} s, confusion {:.1} s)",
                d.der() * 100.0,
                d.scored,
                d.missed,
                d.false_alarm,
                d.confusion
            );
            println!(
                "speakers: found {}, true {}  (collar {collar} s)",
                d.found_speakers, d.reference_speakers
            );
            Ok(())
        }
        [cmd, bundle] if *cmd == "draft" => {
            let range = range.ok_or("draft needs --range a:b")?;
            let out = option(args, "--out")?.ok_or("draft needs --out <dir>")?;
            let default = Path::new(bundle)
                .file_stem()
                .map_or("draft".into(), |s| s.to_string_lossy().into_owned());
            let name = option(args, "--name")?.unwrap_or(default);
            draft(bundle, range, &out, &name)
        }
        _ => Err("usage: measure wer <reference.txt> <transcript> [--range a:b] | measure der <reference> <result> [--collar s] [--range a:b] | measure draft <bundle> --range a:b --out <dir> [--name n]".into()),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
