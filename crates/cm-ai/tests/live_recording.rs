//! M2h on a bundle: a microphone file that grows while a live run follows it, then the job at Stop
//! (`transcribe_recording`) resumes from the saved state and gives the batch result with fewer
//! engine runs. Needs FFmpeg (the decode); prints SKIPPED without it. A scripted engine, no model.

#![allow(clippy::unwrap_used)]

use std::sync::atomic::{AtomicUsize, Ordering};

use cm_ai::recording::live::{LiveRun, LIVE_FILE};
use cm_ai::recording::{transcribe_recording, TranscribeOptions};
use cm_ai::transcribe::{media_secs, Pcm16k, Transcriber, Transcript, Word, TRANSCRIPT_VERSION};
use cm_core::wav::WavWriter;
use cm_core::CmError;

const RATE: usize = 16_000;

/// One word per second of the audio it is given, its text a checksum of that second.
struct Checksum {
    runs: AtomicUsize,
}

impl Transcriber for Checksum {
    fn transcribe(
        &self,
        audio: Pcm16k<'_>,
        _language: Option<&str>,
        progress: &mut dyn FnMut(f32) -> bool,
    ) -> Result<Transcript, CmError> {
        self.runs.fetch_add(1, Ordering::SeqCst);
        if !progress(1.0) {
            return Err(cm_ai::transcribe::cancelled());
        }
        let words = audio
            .samples
            .chunks(RATE)
            .enumerate()
            .map(|(i, c)| Word {
                text: format!(
                    "w{}",
                    (c.iter().map(|s| f64::from(s.abs())).sum::<f64>() * 100.0) as u64
                ),
                start: audio.start + media_secs(i as f64),
                end: audio.start + media_secs(i as f64 + 0.5),
                confidence: 0.9,
            })
            .collect();
        Ok(Transcript {
            version: TRANSCRIPT_VERSION,
            language: "en".into(),
            model: "m".into(),
            words,
            note: None,
            sounds: Vec::new(),
            segments: Vec::new(),
        })
    }
}

fn audio(secs: usize) -> Vec<f32> {
    let mut x: u32 = 7;
    let mut v: Vec<f32> = (0..secs * RATE)
        .map(|_| {
            x = x.wrapping_mul(1664525).wrapping_add(1013904223);
            0.2 * ((x >> 9) as f32 / (1 << 22) as f32 - 1.0)
        })
        .collect();
    for centre in [297usize, 611] {
        for s in &mut v[centre * RATE..(centre + 1) * RATE] {
            *s *= 0.001;
        }
    }
    v
}

fn manifest() -> String {
    serde_json::json!({
        "schema": 1, "id": "rec_live", "title": "live", "created": "2026-11-03T10:00:00Z",
        "tick_rate": 120000,
        "tracks": {"mic": {"file": "mic.wav", "start": 0}},
        "edits": [], "revisions": [], "gaps": [],
    })
    .to_string()
}

#[test]
fn the_job_at_stop_resumes_from_the_pieces_read_while_recording() {
    if cm_media::tools::locate().is_err() {
        eprintln!("SKIPPED: FFmpeg was not found");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let wav = dir.path().join("mic.wav");
    std::fs::write(
        dir.path().join("recording.lock"),
        serde_json::json!({"gaps": [], "tracks": {"mic": {"start": 0, "file": "mic.wav"}}})
            .to_string(),
    )
    .unwrap();
    let x = audio(1200);
    let engine = Checksum {
        runs: AtomicUsize::new(0),
    };

    // The recorder writes a chunk at a time; the live run steps after each.
    let mut w = WavWriter::create(&wav, 1, RATE as u32).unwrap();
    let mut run = LiveRun::open(dir.path(), "m", None, None, false).unwrap();
    let mut finished = 0;
    for chunk in x.chunks(150 * RATE).take(7) {
        w.write(chunk).unwrap();
        w.patch().unwrap();
        finished += run.step(&engine, &mut |_| true).unwrap();
    }
    assert!(finished >= 1, "pieces were read while the file grew");
    assert!(dir.path().join(LIVE_FILE).is_file());
    let during = engine.runs.load(Ordering::SeqCst);
    // The rest of the recording, then Stop.
    for chunk in x[7 * 150 * RATE..].chunks(150 * RATE) {
        w.write(chunk).unwrap();
    }
    w.finalize().unwrap();
    std::fs::write(dir.path().join("manifest.json"), manifest()).unwrap();
    std::fs::remove_file(dir.path().join("recording.lock")).unwrap();

    let opts = TranscribeOptions {
        model_id: Some("m"),
        ..TranscribeOptions::default()
    };
    let at_stop = Checksum {
        runs: AtomicUsize::new(0),
    };
    let with_live = transcribe_recording(dir.path(), &at_stop, &opts, &mut |_| true).unwrap();

    // The same job with no saved state: the batch result.
    std::fs::remove_file(dir.path().join(LIVE_FILE)).unwrap();
    let batch_engine = Checksum {
        runs: AtomicUsize::new(0),
    };
    let batch = transcribe_recording(dir.path(), &batch_engine, &opts, &mut |_| true).unwrap();

    assert_eq!(with_live.words, batch.words, "word for word");
    let (a, b) = (
        at_stop.runs.load(Ordering::SeqCst),
        batch_engine.runs.load(Ordering::SeqCst),
    );
    assert!(a < b, "the job at Stop read {a} pieces, the batch {b}");
    assert!(during >= 1);
}
