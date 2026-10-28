//! `transcribe_recording` on hand-made bundles: audio tracks, pause gaps, session time. Needs
//! FFmpeg to make the audio files; without it each test prints SKIPPED.

#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::process::Command;
use std::sync::Mutex;

use cm_ai::recording::{
    read_transcript, transcribe_recording, write_transcript, TranscribeOptions,
};
use cm_ai::transcribe::{
    is_cancelled, media_secs, Pcm16k, ScriptedTranscriber, Transcriber, Transcript,
};
use cm_core::{CmError, MediaTime};

fn ffmpeg() -> Option<std::path::PathBuf> {
    cm_media::tools::locate().ok().map(|t| t.ffmpeg)
}

/// A mono sine file of `secs` seconds.
fn tone(ffmpeg: &Path, out: &Path, secs: f64) {
    let ok = Command::new(ffmpeg)
        .args(["-v", "error", "-f", "lavfi", "-i"])
        .arg(format!("sine=frequency=300:duration={secs}"))
        .args(["-ar", "48000", "-ac", "1", "-y"])
        .arg(out)
        .status()
        .unwrap()
        .success();
    assert!(ok, "FFmpeg could not make {}", out.display());
}

fn ticks(secs: f64) -> i64 {
    media_secs(secs).ticks()
}

fn manifest(tracks: serde_json::Value, gaps: serde_json::Value) -> String {
    serde_json::json!({
        "schema": 1,
        "id": "rec_test",
        "title": "test",
        "created": "2026-10-22T10:00:00Z",
        "tick_rate": 120000,
        "tracks": tracks,
        "edits": [],
        "revisions": [],
        "gaps": gaps,
    })
    .to_string()
}

/// Remembers what the transcriber was given.
struct Probe {
    seen: Mutex<Option<(usize, MediaTime)>>,
    inner: ScriptedTranscriber,
}

impl Transcriber for Probe {
    fn transcribe(
        &self,
        audio: Pcm16k<'_>,
        language: Option<&str>,
        progress: &mut dyn FnMut(f32) -> bool,
    ) -> Result<Transcript, CmError> {
        *self.seen.lock().unwrap() = Some((audio.samples.len(), audio.start));
        self.inner.transcribe(audio, language, progress)
    }
}

fn secs(t: MediaTime) -> f64 {
    t.as_secs_f64()
}

/// Words come back in session time even when the recording was paused: the mic file has no
/// pause in it, the session clock does.
#[test]
fn words_after_a_pause_land_after_it_in_session_time() {
    let Some(ff) = ffmpeg() else {
        println!("SKIPPED: FFmpeg not found");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    tone(&ff, &dir.path().join("mic.wav"), 4.0);
    // The mic starts 0.1 s into the session; the session was paused from 2.0 s to 3.0 s.
    std::fs::write(
        dir.path().join("manifest.json"),
        manifest(
            serde_json::json!({"mic": {"file": "mic.wav", "start": ticks(0.1), "end": ticks(5.1)}}),
            serde_json::json!([{"from": ticks(2.0), "to": ticks(3.0)}]),
        ),
    )
    .unwrap();
    let t = ScriptedTranscriber::from_secs(&[("before", 0.5, 0.9), ("after", 2.2, 2.6)]);
    let mut seen = Vec::new();
    let out = transcribe_recording(
        dir.path(),
        &t,
        &TranscribeOptions {
            model_id: Some("scripted"),
            ..Default::default()
        },
        &mut |p| {
            seen.push(p);
            true
        },
    )
    .unwrap();
    assert_eq!(out.words.len(), 2);
    // "before": audio offset 0.5 s -> file 0.6 s -> session 0.6 s (the pause is later).
    assert!((secs(out.words[0].start) - 0.6).abs() < 1e-6);
    // "after": audio offset 2.2 s -> file 2.3 s, past the pause at file 2.0 s -> session 3.3 s.
    assert!(
        (secs(out.words[1].start) - 3.3).abs() < 1e-6,
        "{:?}",
        out.words[1]
    );
    assert!((secs(out.words[1].end) - 3.7).abs() < 1e-6);
    assert!(seen.first().is_some_and(|p| *p == 0.0) && seen.last().is_some_and(|p| *p == 1.0));
    // Nothing is stored by transcribing: the caller decides.
    assert!(read_transcript(dir.path()).unwrap().is_none());
    write_transcript(dir.path(), &out).unwrap();
    assert_eq!(read_transcript(dir.path()).unwrap().unwrap(), out);
}

#[test]
fn system_audio_is_mixed_in_only_when_asked_and_alone_without_a_mic() {
    let Some(ff) = ffmpeg() else {
        println!("SKIPPED: FFmpeg not found");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    tone(&ff, &dir.path().join("mic.wav"), 3.0);
    tone(&ff, &dir.path().join("system.wav"), 2.0);
    let both = manifest(
        serde_json::json!({
            "mic": {"file": "mic.wav", "start": ticks(0.1)},
            "system": {"file": "system.wav", "start": ticks(1.0)},
        }),
        serde_json::json!([]),
    );
    std::fs::write(dir.path().join("manifest.json"), both).unwrap();
    let probe = |include_system: bool| {
        let p = Probe {
            seen: Mutex::new(None),
            inner: ScriptedTranscriber::from_secs(&[("x", 0.0, 0.1)]),
        };
        transcribe_recording(
            dir.path(),
            &p,
            &TranscribeOptions {
                include_system,
                ..Default::default()
            },
            &mut |_| true,
        )
        .unwrap();
        let seen = *p.seen.lock().unwrap();
        seen.unwrap()
    };
    // Mic only: 3 s starting at the mic's first sample (file time 0.1 s).
    let (len, start) = probe(false);
    assert!((47_900..=48_100).contains(&len), "{len}");
    assert!((secs(start) - 0.1).abs() < 1e-6);
    // Both: the system track (0.9 s in, 2 s long) ends inside the mic's 3 s, so the length holds.
    let (len, start) = probe(true);
    assert!((47_900..=48_100).contains(&len), "{len}");
    assert!((secs(start) - 0.1).abs() < 1e-6);

    // A recording with only system audio uses it, and a missing mic file is not an error.
    let only_system = manifest(
        serde_json::json!({
            "mic": {"file": "gone.wav", "start": 0, "missing": true},
            "system": {"file": "system.wav", "start": ticks(1.0)},
        }),
        serde_json::json!([]),
    );
    std::fs::write(dir.path().join("manifest.json"), only_system).unwrap();
    let (len, start) = probe(false);
    assert!((31_900..=32_100).contains(&len), "{len}");
    assert!((secs(start) - 1.0).abs() < 1e-6);
}

#[test]
fn a_recording_without_audio_is_an_input_error_and_cancel_is_reported() {
    let Some(ff) = ffmpeg() else {
        println!("SKIPPED: FFmpeg not found");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("manifest.json"),
        manifest(
            serde_json::json!({"screen": {"file": "screen.mp4", "start": 0}}),
            serde_json::json!([]),
        ),
    )
    .unwrap();
    let t = ScriptedTranscriber::from_secs(&[("x", 0.0, 0.1)]);
    let e = transcribe_recording(dir.path(), &t, &TranscribeOptions::default(), &mut |_| true)
        .unwrap_err();
    assert_eq!(e.code().as_str(), "invalid_input");
    assert!(e.to_string().contains("no microphone"), "{e}");

    tone(&ff, &dir.path().join("mic.wav"), 1.0);
    std::fs::write(
        dir.path().join("manifest.json"),
        manifest(
            serde_json::json!({"mic": {"file": "mic.wav", "start": 0}}),
            serde_json::json!([]),
        ),
    )
    .unwrap();
    let e = transcribe_recording(dir.path(), &t, &TranscribeOptions::default(), &mut |_| {
        false
    })
    .unwrap_err();
    assert!(is_cancelled(&e), "{e}");
}

/// A word the model timed before any sound (it pegs the first words of a clip that opens with
/// silence to the start) is moved up to the first speech: a mic file that is silent for 2 s gives
/// words that begin after that silence, not at the start of the track.
#[test]
fn no_word_starts_before_the_speech_does() {
    let Some(ff) = ffmpeg() else {
        println!("SKIPPED: FFmpeg not found");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let ok = Command::new(&ff)
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=300:duration=2",
        ])
        .args(["-af", "adelay=2000", "-ar", "48000", "-ac", "1", "-y"])
        .arg(dir.path().join("mic.wav"))
        .status()
        .unwrap()
        .success();
    assert!(ok);
    // The mic starts 0.1 s into the session; there are no pauses.
    std::fs::write(
        dir.path().join("manifest.json"),
        manifest(
            serde_json::json!({"mic": {"file": "mic.wav", "start": ticks(0.1), "end": ticks(4.1)}}),
            serde_json::json!([]),
        ),
    )
    .unwrap();
    // The model puts "early" at 0.1 to 0.4 s (in the silence) and "late" inside the speech.
    let t = ScriptedTranscriber::from_secs(&[("early", 0.1, 0.4), ("late", 2.5, 2.9)]);
    let out =
        transcribe_recording(dir.path(), &t, &TranscribeOptions::default(), &mut |_| true).unwrap();
    // The speech starts 2.0 s into the file, which is 2.1 s into the session.
    assert!(
        (secs(out.words[0].start) - 2.1).abs() < 0.05,
        "early word at {}",
        secs(out.words[0].start)
    );
    assert!(out.words[0].end > out.words[0].start);
    // A word already inside the speech is left alone (0.1 track start + 2.5).
    assert!(
        (secs(out.words[1].start) - 2.6).abs() < 0.001,
        "late word at {}",
        secs(out.words[1].start)
    );
}

/// A recording stores the noise-reduced microphone as the track's file and the original as
/// `raw_file`. Transcription reads the reduced copy unless the original is asked for (noise
/// reduction can take out soft sounds Whisper needs); a recording without a copy has only one.
#[test]
fn the_original_microphone_is_read_only_when_asked_for() {
    let Some(ff) = ffmpeg() else {
        println!("SKIPPED: FFmpeg not found");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    // Different lengths tell the two files apart: 2 s reduced, 4 s original.
    tone(&ff, &dir.path().join("mic.denoised.wav"), 2.0);
    tone(&ff, &dir.path().join("mic.wav"), 4.0);
    std::fs::write(
        dir.path().join("manifest.json"),
        manifest(
            serde_json::json!({"mic": {
                "file": "mic.denoised.wav", "raw_file": "mic.wav",
                "denoised_file": "mic.denoised.wav", "denoise": true,
                "start": 0, "end": ticks(4.0)}}),
            serde_json::json!([]),
        ),
    )
    .unwrap();
    let seconds_read = |original_audio: bool| {
        let probe = Probe {
            seen: Mutex::new(None),
            inner: ScriptedTranscriber::from_secs(&[("hello", 0.5, 0.9)]),
        };
        transcribe_recording(
            dir.path(),
            &probe,
            &TranscribeOptions {
                model_id: Some("scripted"),
                original_audio,
                ..Default::default()
            },
            &mut |_| true,
        )
        .unwrap();
        let samples = probe.seen.lock().unwrap().unwrap().0;
        samples as f64 / 16_000.0
    };
    assert!(
        (seconds_read(false) - 2.0).abs() < 0.1,
        "default: the reduced copy"
    );
    assert!(
        (seconds_read(true) - 4.0).abs() < 0.1,
        "asked for: the original"
    );

    // A recording made without noise reduction has one file: the choice changes nothing.
    let plain = tempfile::tempdir().unwrap();
    tone(&ff, &plain.path().join("mic.wav"), 3.0);
    std::fs::write(
        plain.path().join("manifest.json"),
        manifest(
            serde_json::json!({"mic": {"file": "mic.wav", "start": 0, "end": ticks(3.0)}}),
            serde_json::json!([]),
        ),
    )
    .unwrap();
    for original_audio in [false, true] {
        let probe = Probe {
            seen: Mutex::new(None),
            inner: ScriptedTranscriber::from_secs(&[("hello", 0.5, 0.9)]),
        };
        transcribe_recording(
            plain.path(),
            &probe,
            &TranscribeOptions {
                model_id: Some("scripted"),
                original_audio,
                ..Default::default()
            },
            &mut |_| true,
        )
        .unwrap();
        let s = probe.seen.lock().unwrap().unwrap().0 as f64 / 16_000.0;
        assert!((s - 3.0).abs() < 0.1, "one file either way: {s}");
    }
}

/// A piece that comes out as a loop from the noise-reduced file is read again from the original
/// microphone (and the reverse): both files of the recording are used, the clean reading is kept.
#[test]
fn a_bad_reading_is_read_again_from_the_other_file_of_the_recording() {
    let Some(ff) = ffmpeg() else {
        println!("SKIPPED: FFmpeg not found");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    tone(&ff, &dir.path().join("mic.denoised.wav"), 2.0);
    tone(&ff, &dir.path().join("mic.wav"), 2.0);
    // Make the two files tell apart: the original is quieter.
    let quiet = dir.path().join("mic.quiet.wav");
    let ok = Command::new(&ff)
        .args(["-v", "error", "-i"])
        .arg(dir.path().join("mic.wav"))
        .args(["-af", "volume=0.1", "-y"])
        .arg(&quiet)
        .status()
        .unwrap()
        .success();
    assert!(ok);
    std::fs::rename(&quiet, dir.path().join("mic.wav")).unwrap();
    std::fs::write(
        dir.path().join("manifest.json"),
        manifest(
            serde_json::json!({"mic": {
                "file": "mic.denoised.wav", "raw_file": "mic.wav",
                "denoised_file": "mic.denoised.wav", "denoise": true,
                "start": 0, "end": ticks(2.0)}}),
            serde_json::json!([]),
        ),
    )
    .unwrap();

    /// Loops on the louder file (the noise-reduced one), reads the quiet one cleanly.
    struct LoopsOnLoud {
        loud_calls: Mutex<usize>,
        quiet_calls: Mutex<usize>,
    }
    impl Transcriber for LoopsOnLoud {
        fn transcribe(
            &self,
            audio: Pcm16k<'_>,
            _: Option<&str>,
            _: &mut dyn FnMut(f32) -> bool,
        ) -> Result<Transcript, CmError> {
            let loud = audio.samples.iter().any(|s| s.abs() > 0.05);
            let words: Vec<cm_ai::transcribe::Word> = if loud {
                *self.loud_calls.lock().unwrap() += 1;
                (0..60)
                    .map(|i| word_at(["da", "tra", "lai", "cho", "minh"][i % 5], i as f64 * 0.03))
                    .collect()
            } else {
                *self.quiet_calls.lock().unwrap() += 1;
                vec![word_at("hello", 0.5), word_at("there", 0.9)]
            };
            Ok(Transcript {
                version: cm_ai::transcribe::TRANSCRIPT_VERSION,
                language: "en".into(),
                model: "scripted".into(),
                words,
                note: None,
            })
        }
    }
    fn word_at(text: &str, at: f64) -> cm_ai::transcribe::Word {
        cm_ai::transcribe::Word {
            text: text.into(),
            start: media_secs(at),
            end: media_secs(at + 0.02),
            confidence: 0.9,
        }
    }
    let engine = LoopsOnLoud {
        loud_calls: Mutex::new(0),
        quiet_calls: Mutex::new(0),
    };
    let out = transcribe_recording(
        dir.path(),
        &engine,
        &TranscribeOptions::default(),
        &mut |_| true,
    )
    .unwrap();
    let text: Vec<&str> = out.words.iter().map(|w| w.text.as_str()).collect();
    assert_eq!(
        text,
        ["hello", "there"],
        "the clean reading of the other file"
    );
    assert_eq!(
        (
            *engine.loud_calls.lock().unwrap(),
            *engine.quiet_calls.lock().unwrap()
        ),
        (1, 1)
    );

    // A recording with one file has nothing to read again from: the loop is cut down instead.
    let one = tempfile::tempdir().unwrap();
    tone(&ff, &one.path().join("mic.wav"), 2.0);
    std::fs::write(
        one.path().join("manifest.json"),
        manifest(
            serde_json::json!({"mic": {"file": "mic.wav", "start": 0, "end": ticks(2.0)}}),
            serde_json::json!([]),
        ),
    )
    .unwrap();
    let engine = LoopsOnLoud {
        loud_calls: Mutex::new(0),
        quiet_calls: Mutex::new(0),
    };
    let out = transcribe_recording(
        one.path(),
        &engine,
        &TranscribeOptions::default(),
        &mut |_| true,
    )
    .unwrap();
    assert!(out.words.len() < 20, "{} words", out.words.len());
}
