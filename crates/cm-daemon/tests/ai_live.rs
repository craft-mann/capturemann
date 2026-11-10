//! M2h through the daemon: an audio recording is followed while it grows (the real adapters over
//! `cm-ai`, a scripted engine, no Whisper model, no hardware), the pieces it finished are saved in
//! the bundle, and the `transcribe` job after Stop gives the batch result from fewer engine runs.
//! Needs FFmpeg for the decode (prints SKIPPED without it). Discard deletes the saved state.

#![allow(clippy::unwrap_used)]

mod common;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use cm_ai::transcribe::{media_secs, Pcm16k, Transcriber, Transcript, Word, TRANSCRIPT_VERSION};
use cm_core::wav::WavWriter;
use cm_core::{Actor, CmError};
use cm_daemon::ai_engine::services_with;
use common::{builder, desktop};
use serde_json::{json, Value};

const RATE: usize = 16_000;

/// One word per second of the audio it is given, its text a checksum of that second.
struct Checksum {
    runs: Arc<AtomicUsize>,
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
            model: "small".into(),
            words,
            note: None,
            sounds: Vec::new(),
            segments: Vec::new(),
            dropped: Vec::new(),
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

struct Rig {
    daemon: cm_daemon::Daemon,
    lib: tempfile::TempDir,
    runs: Arc<AtomicUsize>,
    _models: tempfile::TempDir,
}

async fn rig() -> Option<Rig> {
    if cm_media::tools::locate().is_err() {
        eprintln!("SKIPPED: FFmpeg was not found");
        return None;
    }
    let models = tempfile::tempdir().unwrap();
    let runs = Arc::new(AtomicUsize::new(0));
    let (b, lib) = builder(desktop());
    let queue = cm_daemon::library_job_queue(lib.path()).unwrap();
    let daemon = b
        .jobs(queue)
        .ai(services_with(
            models.path().to_path_buf(),
            Some(Arc::new(Checksum { runs: runs.clone() })),
        ))
        .build();
    daemon.register_ai_handlers().unwrap();
    daemon.start_jobs().unwrap();
    let model = models.path().join("ggml-small.bin");
    let mut bytes = vec![0u8; 21_000_000];
    bytes[..4].copy_from_slice(&[0x6c, 0x6d, 0x67, 0x67]);
    std::fs::write(&model, bytes).unwrap();
    daemon
        .execute_method(
            Actor::User,
            0,
            "import_model",
            &json!({"model": "small", "path": model.to_str().unwrap()}),
        )
        .await
        .unwrap();
    daemon
        .execute_method(
            Actor::User,
            0,
            "set_settings",
            &json!({"values": {"ai.auto_transcribe": true, "ai.auto_speakers": false, "ai.live_poll_ms": 100}}),
        )
        .await
        .unwrap();
    Some(Rig {
        daemon,
        lib,
        runs,
        _models: models,
    })
}

async fn set_poll(r: &Rig, ms: u64) {
    r.daemon
        .execute_method(
            Actor::User,
            0,
            "set_settings",
            &json!({"values": {"ai.live_poll_ms": ms}}),
        )
        .await
        .unwrap();
}

fn start_bundle(r: &Rig, id: &str) -> std::path::PathBuf {
    let dir = r.lib.path().join(format!("{id}.cmrec"));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("recording.lock"),
        json!({"id": id, "gaps": [], "tracks": {"mic": {"start": 0, "file": "mic.wav"}}})
            .to_string(),
    )
    .unwrap();
    dir
}

fn finish_bundle(dir: &std::path::Path, id: &str) {
    std::fs::write(
        dir.join("manifest.json"),
        json!({
            "schema": 1, "id": id, "title": "Live talk", "created": "2026-11-03T10:00:00Z",
            "tick_rate": 120000, "kind": "audio",
            "tracks": {"mic": {"file": "mic.wav", "start": 0}},
            "edits": [], "revisions": [], "gaps": [], "duration": 1200 * 120000,
        })
        .to_string(),
    )
    .unwrap();
    let _ = std::fs::remove_file(dir.join("recording.lock"));
}

async fn wait_for(what: &str, mut f: impl FnMut() -> bool) {
    for _ in 0..1500 {
        if f() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("timed out waiting for {what}");
}

/// The pieces read while the recording ran make the job at Stop shorter, not different.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn m2h_a_recording_is_followed_and_the_job_at_stop_resumes() {
    let Some(r) = rig().await else { return };
    set_poll(&r, 100).await;
    r.daemon.start_ai_auto_transcribe();
    tokio::task::yield_now().await;
    let dir = start_bundle(&r, "rec_live_a");
    let x = audio(1200);
    let mut w = WavWriter::create(&dir.join("mic.wav"), 1, RATE as u32).unwrap();
    r.daemon.event_bus().publish(
        "recording.started",
        json!({"capture_id": "rec_live_a", "mode": "audio", "tracks": ["mic"]}),
    );
    // The recorder writes; the follower reads the first piece once 10 minutes are on disk.
    for chunk in x[..650 * RATE].chunks(130 * RATE) {
        w.write(chunk).unwrap();
        w.patch().unwrap();
        tokio::time::sleep(Duration::from_millis(60)).await;
    }
    wait_for("the saved state", || {
        dir.join("transcript.live.json").is_file()
    })
    .await;
    // The follower has done its part; it is slowed so that it does not start a second piece
    // while the rest is written (what it does at Stop is cancel the piece in hand).
    set_poll(&r, 3_600_000).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let during = r.runs.load(Ordering::SeqCst);
    assert!(during >= 1);

    // The rest, Stop, and the automatic job.
    w.write(&x[650 * RATE..]).unwrap();
    w.finalize().unwrap();
    finish_bundle(&dir, "rec_live_a");
    r.daemon
        .execute_method(Actor::User, 0, "rescan_library", &json!({}))
        .await
        .unwrap();
    let before_stop = r.runs.load(Ordering::SeqCst);
    r.daemon.event_bus().publish(
        "recording.stopped",
        json!({"capture_id": "rec_live_a", "tracks": ["mic"], "reason": "stopped"}),
    );
    wait_for("the transcript", || dir.join("transcript.json").is_file()).await;
    let t: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("transcript.json")).unwrap())
            .unwrap();
    let words: Vec<String> = t["words"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["text"].as_str().unwrap().to_owned())
        .collect();

    // The batch result: the same engine over the same audio, one run per piece of the plan.
    let batch_runs = Arc::new(AtomicUsize::new(0));
    let batch = cm_ai::pieces::transcribe_pieces(
        &Checksum {
            runs: batch_runs.clone(),
        },
        Pcm16k {
            samples: &x,
            start: cm_core::MediaTime::ZERO,
        },
        None,
        None,
        &mut |_| true,
    )
    .unwrap()
    .0;
    let want: Vec<String> = batch.words.iter().map(|w| w.text.clone()).collect();
    assert_eq!(words, want, "word for word the batch transcript");
    let total = r.runs.load(Ordering::SeqCst);
    // Every piece is read once, except the one in hand at Stop, which may have been dropped and
    // is then read again.
    let batch_total = batch_runs.load(Ordering::SeqCst);
    assert!(
        total - before_stop < batch_total,
        "the job after Stop read {} pieces, the batch {batch_total}",
        total - before_stop
    );
    assert_eq!(total - during, batch_total - during, "{total} {during}");
    assert!(
        (batch_total..=batch_total + 1).contains(&total),
        "{total} runs, batch {batch_total}"
    );
    assert!(
        !dir.join("transcript.live.json").exists(),
        "the state is used up"
    );
}

/// A discarded recording leaves no saved state.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn m2h_a_discarded_recording_deletes_the_saved_state() {
    let Some(r) = rig().await else { return };
    set_poll(&r, 100).await;
    r.daemon.start_ai_auto_transcribe();
    tokio::task::yield_now().await;
    let dir = start_bundle(&r, "rec_live_b");
    let x = audio(700);
    let mut w = WavWriter::create(&dir.join("mic.wav"), 1, RATE as u32).unwrap();
    r.daemon.event_bus().publish(
        "recording.started",
        json!({"capture_id": "rec_live_b", "mode": "audio", "tracks": ["mic"]}),
    );
    w.write(&x).unwrap();
    w.patch().unwrap();
    wait_for("the saved state", || {
        dir.join("transcript.live.json").is_file()
    })
    .await;
    r.daemon.event_bus().publish(
        "recording.discarded",
        json!({"capture_id": "rec_live_b", "reason": "discarded"}),
    );
    wait_for("the state to go", || {
        !dir.join("transcript.live.json").exists()
    })
    .await;
}

/// A screen recording is not followed, and a recording with the setting off is not either.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn m2h_screen_recordings_and_the_off_switch_start_nothing() {
    let Some(r) = rig().await else { return };
    set_poll(&r, 100).await;
    r.daemon.start_ai_auto_transcribe();
    tokio::task::yield_now().await;
    let dir = start_bundle(&r, "rec_live_c");
    let x = audio(700);
    let mut w = WavWriter::create(&dir.join("mic.wav"), 1, RATE as u32).unwrap();
    w.write(&x).unwrap();
    w.patch().unwrap();
    r.daemon.event_bus().publish(
        "recording.started",
        json!({"capture_id": "rec_live_c", "mode": "screen", "tracks": ["screen", "mic"]}),
    );
    tokio::time::sleep(Duration::from_millis(700)).await;
    assert!(!dir.join("transcript.live.json").exists());
    assert_eq!(r.runs.load(Ordering::SeqCst), 0);

    r.daemon
        .execute_method(
            Actor::User,
            0,
            "set_settings",
            &json!({"values": {"ai.transcribe_while_recording": false}}),
        )
        .await
        .unwrap();
    r.daemon.event_bus().publish(
        "recording.started",
        json!({"capture_id": "rec_live_c", "mode": "audio", "tracks": ["mic"]}),
    );
    tokio::time::sleep(Duration::from_millis(700)).await;
    assert!(!dir.join("transcript.live.json").exists());
    assert_eq!(r.runs.load(Ordering::SeqCst), 0);
}

/// M2i: the exports write sounds as bracketed lines; a transcript's words text never has them.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn m2i_exports_write_sounds_and_the_words_do_not_have_them() {
    let Some(r) = rig().await else { return };
    let dir = start_bundle(&r, "rec_sounds");
    std::fs::remove_file(dir.join("recording.lock")).unwrap();
    std::fs::write(dir.join("mic.wav"), b"").unwrap();
    finish_bundle(&dir, "rec_sounds");
    let t = cm_ai::transcribe::Transcript {
        version: 1,
        language: "en".into(),
        model: "small".into(),
        words: ["hello", "there", "again"]
            .iter()
            .enumerate()
            .map(|(i, w)| Word {
                text: (*w).into(),
                start: media_secs(i as f64 * 20.0),
                end: media_secs(i as f64 * 20.0 + 0.5),
                confidence: 0.9,
            })
            .collect(),
        note: None,
        sounds: vec![cm_ai::transcribe::Sound {
            kind: cm_ai::transcribe::SoundKind::Unclear,
            start: media_secs(8.0),
            end: media_secs(14.0),
        }],
        segments: Vec::new(),
        dropped: Vec::new(),
    };
    std::fs::write(dir.join("transcript.json"), serde_json::to_vec(&t).unwrap()).unwrap();
    r.daemon
        .execute_method(Actor::User, 0, "rescan_library", &json!({}))
        .await
        .unwrap();
    let out = r
        .daemon
        .execute_method(
            Actor::User,
            0,
            "export_captions",
            &json!({"capture_id": "rec_sounds", "format": "srt"}),
        )
        .await
        .unwrap();
    let text = std::fs::read_to_string(out["path"].as_str().unwrap()).unwrap();
    assert!(text.contains("[unclear]"), "{text}");
    assert!(text.contains("hello") && text.contains("again"), "{text}");
    // The words-only readers: the transcript text and the caption lines built from the words.
    assert!(!t.text().contains('['));
    assert!(cm_ai::caption::build_captions(&t.words)
        .iter()
        .all(|l| !l.text.contains('[')));
}
