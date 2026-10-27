//! AI-8 speaker labels through the daemon with a scripted speaker model ([`FakeAi`]): the
//! `diarize` job, `set_speakers` as one undoable revision, `get_transcript` with speakers,
//! the named exports, headset mode, `no_speaker_model`, and no voiceprint on disk. The model
//! itself waits for spike S7; nothing here loads one.

#![allow(clippy::unwrap_used)]

mod common;

use std::time::Duration;

use cm_core::{Actor, ErrorCode, MediaTime, TrackInfo, TrackKind, TrackStats};
use cm_daemon::testing::FakeAi;
use cm_daemon::{Daemon, DaemonError};
use common::{builder, desktop};
use serde_json::{json, Map, Value};
use time::macros::datetime;

fn agent(n: &str) -> Actor {
    Actor::agent(n).unwrap()
}

fn code(e: &DaemonError) -> ErrorCode {
    match e {
        DaemonError::Cm(c) => c.code(),
        DaemonError::WithData { error, .. } => error.code(),
        other => panic!("unexpected {other:?}"),
    }
}

fn reason(e: &DaemonError) -> String {
    match e {
        DaemonError::WithData { data, .. } => data["reason"].as_str().unwrap_or("").to_owned(),
        _ => String::new(),
    }
}

struct Rig {
    daemon: Daemon,
    ai: FakeAi,
    id: String,
    dir: std::path::PathBuf,
    tmp: tempfile::TempDir,
}

/// A finished 60 s recording with a mic track.
fn recording(tmp: &std::path::Path) -> (String, std::path::PathBuf) {
    recording_with(tmp, json!({}))
}

/// The same, with the manifest `settings` (`preset`, `people` of an audio recording).
fn recording_with(tmp: &std::path::Path, settings: Value) -> (String, std::path::PathBuf) {
    use cm_library::rec::{RecBundle, SessionInfo};
    let now = datetime!(2026-10-22 10:00:00 UTC);
    let session = SessionInfo {
        clock_origin_ns: 1,
        timebase: "uptime_raw_ns".into(),
        displays: vec![],
        devices: json!({}),
        app_version: "0.0.1".into(),
        extra: Map::new(),
    };
    let rec = RecBundle::create(tmp, "Interview", now, settings, &session).unwrap();
    std::fs::write(rec.dir().join("screen.mp4"), b"master bytes").unwrap();
    std::fs::write(rec.dir().join("mic.wav"), b"audio").unwrap();
    let track = |kind, file: &str| TrackInfo {
        kind,
        file: file.into(),
        start: MediaTime::ZERO,
        end: MediaTime::from_secs(60),
        details: json!({}),
        stats: TrackStats::default(),
    };
    let tracks = vec![
        track(TrackKind::Screen, "screen.mp4"),
        track(TrackKind::Mic, "mic.wav"),
    ];
    let dir = rec.dir().to_path_buf();
    let m = rec.finalize(&tracks, now).unwrap();
    (m.id, dir)
}

/// Two speakers: the model's labels 5 (first, 0 to 2 s) and 9 (2 s on). The fake transcript has
/// "Hello and welcome" at 1.0 to 2.3 s and "to the demo" at 2.4 to 3.4 s.
const SCRIPT: [(f64, f64, u32, f32); 2] = [(0.0, 2.0, 5, 0.2), (2.0, 30.0, 9, 0.2)];

async fn rig(ai: FakeAi) -> Rig {
    rig_with(ai, json!({})).await
}

async fn rig_with(ai: FakeAi, settings: Value) -> Rig {
    let (b, tmp) = builder(desktop());
    let queue = cm_daemon::library_job_queue(tmp.path()).unwrap();
    let daemon = b.jobs(queue).ai(ai.services()).build();
    daemon.register_ai_handlers().unwrap();
    daemon.start_jobs().unwrap();
    let (id, dir) = recording_with(tmp.path(), settings);
    daemon
        .execute_method(Actor::User, 0, "rescan_library", &json!({}))
        .await
        .unwrap();
    Rig {
        daemon,
        ai,
        id,
        dir,
        tmp,
    }
}

async fn speaker_rig() -> Rig {
    rig(FakeAi::new()
        .with_small_installed()
        .with_speaker_script(&SCRIPT))
    .await
}

async fn call(r: &Rig, who: Actor, method: &str, mut p: Value) -> Result<Value, DaemonError> {
    if p.get("capture_id").is_none() && !matches!(method, "list_jobs" | "get_job" | "set_settings")
    {
        p["capture_id"] = json!(r.id);
    }
    r.daemon.execute_method(who, 0, method, &p).await
}

async fn user(r: &Rig, method: &str, p: Value) -> Result<Value, DaemonError> {
    call(r, Actor::User, method, p).await
}

async fn job_done(r: &Rig, job_id: &str) -> Value {
    for _ in 0..500 {
        let j = r
            .daemon
            .execute_method(Actor::User, 0, "get_job", &json!({"job_id": job_id}))
            .await
            .unwrap();
        if matches!(j["state"].as_str(), Some("done" | "failed" | "cancelled")) {
            return j;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("timed out");
}

async fn transcribe_and_diarize(r: &Rig, extra: Value) -> Value {
    let s = user(r, "transcribe", json!({})).await.unwrap();
    job_done(r, s["job_id"].as_str().unwrap()).await;
    let d = user(r, "diarize", extra).await.unwrap();
    let job = job_done(r, d["job_id"].as_str().unwrap()).await;
    assert_eq!(job["state"], "done", "{job}");
    job
}

fn manifest(dir: &std::path::Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(dir.join("manifest.json")).unwrap()).unwrap()
}

fn names(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap().to_owned())
        .collect()
}

fn word_speakers(t: &Value) -> Vec<u64> {
    t["transcript"]["words"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["speaker"].as_u64().unwrap())
        .collect()
}

/// Edge case: no speaker model installed. `invalid_input` with `data.reason = "no_speaker_model"`
/// and nothing is queued. Production wiring (the real services) answers the same until S7.
#[tokio::test]
async fn ai_8_without_a_speaker_model_says_no_speaker_model() {
    let r = rig(FakeAi::new().with_small_installed()).await;
    let e = user(&r, "diarize", json!({})).await.unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
    assert_eq!(reason(&e), "no_speaker_model");
    let jobs = user(&r, "list_jobs", json!({})).await.unwrap();
    assert!(jobs["jobs"].as_array().unwrap().is_empty());

    // The production wiring: NoSpeakerModel behind the real services.
    let models = tempfile::tempdir().unwrap();
    let (b, tmp) = builder(desktop());
    let queue = cm_daemon::library_job_queue(tmp.path()).unwrap();
    let daemon = b
        .jobs(queue)
        .ai(cm_daemon::ai_engine::services_with(
            models.path().to_path_buf(),
            None,
        ))
        .build();
    daemon.register_ai_handlers().unwrap();
    daemon.start_jobs().unwrap();
    let (id, _) = recording(tmp.path());
    daemon
        .execute_method(Actor::User, 0, "rescan_library", &json!({}))
        .await
        .unwrap();
    let e = daemon
        .execute_method(Actor::User, 0, "diarize", &json!({"capture_id": id}))
        .await
        .unwrap_err();
    assert_eq!(reason(&e), "no_speaker_model");
}

/// AI-8: `diarize` is a job; it stores `diarization.json` and the manifest's `speakers`, names
/// them Speaker 1..N by first speech, publishes `speakers.ready`, and `get_transcript` carries
/// `speaker` per word and `speakers`.
#[tokio::test]
async fn ai_8_diarize_job_stores_speakers_and_get_transcript_shows_them() {
    let r = speaker_rig().await;
    let mut rx = r.daemon.subscribe();
    let job = transcribe_and_diarize(&r, json!({"num_speakers": 2})).await;
    assert_eq!(
        names(&job["result"]["speakers"]),
        vec!["Speaker 1", "Speaker 2"]
    );
    assert_eq!(job["result"]["segments"].as_array().unwrap().len(), 2);
    // The hints reached the model.
    let calls = r.ai.diarize_calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].hints.num_speakers, Some(2));
    assert!(!calls[0].headset);

    let file: Value =
        serde_json::from_str(&std::fs::read_to_string(r.dir.join("diarization.json")).unwrap())
            .unwrap();
    assert_eq!(file["model"], "scripted");
    assert!(file["created"].as_str().unwrap().starts_with("20"));
    assert_eq!(file["segments"].as_array().unwrap().len(), 2);
    let m = manifest(&r.dir);
    assert_eq!(
        m["speakers"],
        json!([{"id": 1, "name": "Speaker 1"}, {"id": 2, "name": "Speaker 2"}])
    );
    assert_eq!(m["speaker_segments"].as_array().unwrap().len(), 2);

    let mut seen = vec![];
    while let Ok(e) = rx.try_recv() {
        seen.push(e);
    }
    assert!(seen
        .iter()
        .any(|e| e.name == "speakers.ready" && e.data["speakers"] == 2));

    let t = user(&r, "get_transcript", json!({})).await.unwrap();
    assert_eq!(names(&t["speakers"]), vec!["Speaker 1", "Speaker 2"]);
    // Hello, and, welcome (midpoints before and at 2.0 s) / to, the, demo.
    assert_eq!(word_speakers(&t), vec![1, 1, 2, 2, 2, 2]);
}

/// Before a diarization `get_transcript` is unchanged (no `speakers`, no `speaker`).
#[tokio::test]
async fn ai_8_get_transcript_is_unchanged_before_diarization() {
    let r = speaker_rig().await;
    let s = user(&r, "transcribe", json!({})).await.unwrap();
    job_done(&r, s["job_id"].as_str().unwrap()).await;
    let t = user(&r, "get_transcript", json!({})).await.unwrap();
    assert!(t.get("speakers").is_none());
    assert!(t["transcript"]["words"][0].get("speaker").is_none());
}

/// AI-8.AC4: a rename is ONE revision tagged with the caller; the transcript, the TXT export and
/// the speaker list say "Linh"; one undo restores "Speaker 2", redo brings "Linh" back.
#[tokio::test]
async fn ai_8_ac4_rename_is_one_undoable_revision() {
    let r = speaker_rig().await;
    transcribe_and_diarize(&r, json!({})).await;
    let revs = manifest(&r.dir)["revisions"].as_array().unwrap().len();
    let out = call(
        &r,
        agent("claude"),
        "set_speakers",
        json!({"rename": {"2": "Linh"}}),
    )
    .await
    .unwrap();
    assert_eq!(out["changed"], true);
    assert_eq!(out["by"], "agent:claude");
    assert_eq!(names(&out["speakers"]), vec!["Speaker 1", "Linh"]);
    assert_eq!(
        manifest(&r.dir)["revisions"].as_array().unwrap().len(),
        revs + 1
    );

    let t = user(&r, "get_transcript", json!({})).await.unwrap();
    assert_eq!(names(&t["speakers"]), vec!["Speaker 1", "Linh"]);
    let txt = user(&r, "export_captions", json!({"format": "txt"}))
        .await
        .unwrap();
    let text = std::fs::read_to_string(txt["path"].as_str().unwrap()).unwrap();
    assert_eq!(text, "Speaker 1: Hello and welcome\nLinh: to the demo\n");

    let u = user(&r, "undo", json!({})).await.unwrap();
    assert!(u["revision"].as_u64().unwrap() > out["revision"].as_u64().unwrap());
    assert_eq!(
        names(&manifest(&r.dir)["speakers"]),
        vec!["Speaker 1", "Speaker 2"]
    );
    user(&r, "redo", json!({})).await.unwrap();
    assert_eq!(
        names(&manifest(&r.dir)["speakers"]),
        vec!["Speaker 1", "Linh"]
    );

    // The same names again change nothing and make no revision.
    let again = user(&r, "set_speakers", json!({"rename": {"2": "Linh"}}))
        .await
        .unwrap();
    assert_eq!(again["changed"], false);
}

/// AI-8.AC5: merging Speaker 2 into Speaker 1 leaves no word with Speaker 2, undo restores both.
/// Reassigning a range moves the words in it.
#[tokio::test]
async fn ai_8_ac5_merge_and_reassign() {
    let r = speaker_rig().await;
    transcribe_and_diarize(&r, json!({})).await;
    user(
        &r,
        "set_speakers",
        json!({"merge": [{"from": 2, "into": 1}]}),
    )
    .await
    .unwrap();
    let t = user(&r, "get_transcript", json!({})).await.unwrap();
    assert_eq!(names(&t["speakers"]), vec!["Speaker 1"]);
    assert_eq!(word_speakers(&t), vec![1; 6]);
    user(&r, "undo", json!({})).await.unwrap();
    let t = user(&r, "get_transcript", json!({})).await.unwrap();
    assert_eq!(names(&t["speakers"]), vec!["Speaker 1", "Speaker 2"]);
    assert_eq!(word_speakers(&t), vec![1, 1, 2, 2, 2, 2]);

    // 2.35 s to 3.0 s ("the" and "to" are at 2.4 to 2.7 s) goes to Speaker 1.
    user(
        &r,
        "set_speakers",
        json!({"assign": [{"start": 2.35, "end": 2.75, "speaker": 1}]}),
    )
    .await
    .unwrap();
    let t = user(&r, "get_transcript", json!({})).await.unwrap();
    assert_eq!(word_speakers(&t), vec![1, 1, 2, 1, 1, 2]);
    user(&r, "undo", json!({})).await.unwrap();
    let t = user(&r, "get_transcript", json!({})).await.unwrap();
    assert_eq!(word_speakers(&t), vec![1, 1, 2, 2, 2, 2]);
}

#[tokio::test]
async fn ai_8_set_speakers_checks_its_input() {
    let r = speaker_rig().await;
    // Nothing diarized yet.
    let e = user(&r, "set_speakers", json!({"rename": {"1": "A"}}))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
    assert!(e.to_string().contains("diarize"));
    transcribe_and_diarize(&r, json!({})).await;
    for bad in [
        json!({}),
        json!({"rename": {"9": "Ghost"}}),
        json!({"rename": {"1": "  "}}),
        json!({"rename": {"x": "A"}}),
        json!({"merge": [{"from": 1, "into": 1}]}),
        json!({"merge": [{"from": 1}]}),
        json!({"assign": [{"start": 3, "end": 2, "speaker": 1}]}),
        json!({"assign": [{"start": 1, "end": 2, "speaker": 7}]}),
        json!({"rename": {"1": "A"}, "extra": 1}),
    ] {
        let e = user(&r, "set_speakers", bad.clone()).await.unwrap_err();
        assert_eq!(code(&e), ErrorCode::InvalidInput, "{bad}");
    }
    assert_eq!(
        names(&manifest(&r.dir)["speakers"]),
        vec!["Speaker 1", "Speaker 2"]
    );
}

/// AI-8: VTT gets voice tags, SRT gets a "NAME:" prefix only on request, TXT and Markdown get
/// "Name: text" per turn with timestamps.
#[tokio::test]
async fn ai_8_exports_carry_the_speaker_names() {
    let r = speaker_rig().await;
    transcribe_and_diarize(&r, json!({})).await;
    user(
        &r,
        "set_speakers",
        json!({"rename": {"1": "Trung", "2": "Linh"}}),
    )
    .await
    .unwrap();
    let read = |v: &Value| std::fs::read_to_string(v["path"].as_str().unwrap()).unwrap();

    let vtt = read(
        &user(&r, "export_captions", json!({"format": "vtt"}))
            .await
            .unwrap(),
    );
    assert!(
        vtt.contains("00:00:01.000 --> 00:00:02.300\n<v Trung>Hello and welcome\n"),
        "{vtt}"
    );
    assert!(vtt.contains("<v Linh>to the demo"), "{vtt}");

    let srt = read(
        &user(&r, "export_captions", json!({"format": "srt"}))
            .await
            .unwrap(),
    );
    assert!(
        !srt.contains("TRUNG") && srt.contains("\nHello and welcome\n"),
        "{srt}"
    );
    let srt = user(
        &r,
        "export_captions",
        json!({"format": "srt", "speaker_names": true}),
    )
    .await
    .unwrap();
    assert_eq!(srt["speaker_names"], true);
    let srt = read(&srt);
    assert!(
        srt.contains("TRUNG: Hello and welcome") && srt.contains("LINH: to the demo"),
        "{srt}"
    );

    let txt = read(
        &user(
            &r,
            "export_captions",
            json!({"format": "txt", "timestamps": true}),
        )
        .await
        .unwrap(),
    );
    assert_eq!(
        txt,
        "[00:01] Trung: Hello and welcome\n[00:02] Linh: to the demo\n"
    );

    let md = user(&r, "export_captions", json!({"format": "md"}))
        .await
        .unwrap();
    assert!(md["path"].as_str().unwrap().ends_with(".md"));
    assert_eq!(
        read(&md),
        "**[00:01] Trung:** Hello and welcome\n\n**[00:02] Linh:** to the demo\n\n"
    );

    // The switch only exists for SRT.
    let e = user(
        &r,
        "export_captions",
        json!({"format": "vtt", "speaker_names": true}),
    )
    .await
    .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
}

/// Without speakers the exports are what they were, and Markdown still works.
#[tokio::test]
async fn ai_8_exports_without_speakers_are_unchanged() {
    let r = speaker_rig().await;
    let s = user(&r, "transcribe", json!({})).await.unwrap();
    job_done(&r, s["job_id"].as_str().unwrap()).await;
    let vtt = user(&r, "export_captions", json!({"format": "vtt"}))
        .await
        .unwrap();
    let text = std::fs::read_to_string(vtt["path"].as_str().unwrap()).unwrap();
    assert!(
        text.starts_with("WEBVTT\n\n00:00:01.000 --> 00:00:02.300\nHello and welcome"),
        "{text}"
    );
    assert_eq!(vtt["speaker_names"], false);
    let md = user(&r, "export_captions", json!({"format": "md"}))
        .await
        .unwrap();
    let text = std::fs::read_to_string(md["path"].as_str().unwrap()).unwrap();
    assert!(text.starts_with("**[00:01]** Hello and welcome"), "{text}");
}

/// AI-8.AC7: headset mode labels the words spoken on the microphone with `profile.display_name`
/// (default "Me") and diarizes only the system track.
#[tokio::test]
async fn ai_8_ac7_headset_mode_labels_the_mic_as_the_user() {
    let r = rig(FakeAi::new()
        .with_small_installed()
        .with_speaker_script(&[(10.0, 20.0, 3, 0.2), (22.0, 30.0, 8, 0.2)]))
    .await;
    // The fake microphone is loud while the transcript's words are spoken.
    r.ai.set_mic_activity(0.8, 3.6);
    transcribe_and_diarize(&r, json!({"headset": true})).await;
    assert!(r.ai.diarize_calls()[0].headset);
    assert_eq!(r.ai.diarize_calls()[0].display_name, "Me");
    let t = user(&r, "get_transcript", json!({})).await.unwrap();
    assert_eq!(names(&t["speakers"]), vec!["Me", "Speaker 1", "Speaker 2"]);
    assert_eq!(word_speakers(&t), vec![1; 6]);

    // The setting renames the user; an agent may set it too.
    call(
        &r,
        agent("claude"),
        "set_settings",
        json!({"values": {"profile.display_name": "Trung"}}),
    )
    .await
    .unwrap();
    let d = user(&r, "diarize", json!({"headset": true})).await.unwrap();
    job_done(&r, d["job_id"].as_str().unwrap()).await;
    assert_eq!(r.ai.diarize_calls()[1].display_name, "Trung");
    let t = user(&r, "get_transcript", json!({})).await.unwrap();
    assert_eq!(names(&t["speakers"])[0], "Trung");
}

/// A failed or interrupted run keeps the previous result; a new run replaces it and drops the
/// undo steps of the old one.
#[tokio::test]
async fn ai_8_a_new_run_replaces_the_old_one_and_a_failed_run_changes_nothing() {
    let r = speaker_rig().await;
    transcribe_and_diarize(&r, json!({})).await;
    user(&r, "set_speakers", json!({"rename": {"2": "Linh"}}))
        .await
        .unwrap();
    r.ai.fail_diarize_with(cm_core::CmError::internal("model crashed"));
    let d = user(&r, "diarize", json!({})).await.unwrap();
    let job = job_done(&r, d["job_id"].as_str().unwrap()).await;
    assert_eq!(job["state"], "failed");
    assert_eq!(
        names(&manifest(&r.dir)["speakers"]),
        vec!["Speaker 1", "Linh"]
    );

    let d = user(&r, "diarize", json!({})).await.unwrap();
    let job = job_done(&r, d["job_id"].as_str().unwrap()).await;
    assert_eq!(job["state"], "done");
    assert_eq!(
        names(&manifest(&r.dir)["speakers"]),
        vec!["Speaker 1", "Speaker 2"]
    );
    // The old rename's undo step is gone: names stay as the new run set them.
    let _ = user(&r, "undo", json!({})).await;
    assert_eq!(
        names(&manifest(&r.dir)["speakers"]),
        vec!["Speaker 1", "Speaker 2"]
    );
}

/// AI-8.AC6: no voiceprint or embedding file is written to the bundle, the library or the
/// config/model folders, however many times speakers are found and changed.
#[tokio::test]
async fn ai_8_ac6_no_voiceprint_is_ever_written() {
    let r = speaker_rig().await;
    transcribe_and_diarize(&r, json!({})).await;
    user(&r, "set_speakers", json!({"rename": {"2": "Linh"}}))
        .await
        .unwrap();
    user(&r, "export_captions", json!({"format": "vtt"}))
        .await
        .unwrap();
    let found = cm_ai::speakers::find_voiceprint_files(&[r.dir.as_path(), r.tmp.path()]);
    assert!(found.is_empty(), "voiceprints on disk: {found:?}");
    // The helper itself does catch a planted one.
    std::fs::write(r.dir.join("speaker_embedding.npy"), b"x").unwrap();
    assert_eq!(
        cm_ai::speakers::find_voiceprint_files(&[r.dir.as_path()]).len(),
        1
    );
}

#[tokio::test]
async fn ai_8_diarize_refuses_what_it_cannot_do() {
    let r = speaker_rig().await;
    for bad in [
        json!({"num_speakers": 0}),
        json!({"min_speakers": 5, "max_speakers": 2}),
        json!({"headset": "yes"}),
        json!({"mystery": 1}),
    ] {
        let e = user(&r, "diarize", bad.clone()).await.unwrap_err();
        assert_eq!(code(&e), ErrorCode::InvalidInput, "{bad}");
    }
    let e = user(&r, "diarize", json!({"capture_id": "rec_nope"}))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::NotFound);
}

// ---- the automatic speaker step (ai.auto_speakers, on by default) -----------------------------

async fn diarize_jobs(r: &Rig) -> usize {
    user(r, "list_jobs", json!({"type": "diarize"}))
        .await
        .unwrap()["jobs"]
        .as_array()
        .unwrap()
        .len()
}

async fn transcribe_now(r: &Rig) {
    let s = user(r, "transcribe", json!({})).await.unwrap();
    let j = job_done(r, s["job_id"].as_str().unwrap()).await;
    assert_eq!(j["state"], "done", "{j}");
}

async fn settle() {
    tokio::time::sleep(Duration::from_millis(250)).await;
}

async fn until_jobs_done(r: &Rig, n: usize) {
    for _ in 0..300 {
        let jobs = user(r, "list_jobs", json!({"type": "diarize"}))
            .await
            .unwrap();
        let jobs = jobs["jobs"].as_array().unwrap();
        if jobs.len() == n
            && jobs
                .iter()
                .all(|j| matches!(j["state"].as_str(), Some("done" | "failed")))
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the automatic speaker run did not finish");
}

fn ready_event(r: &Rig) {
    r.daemon
        .event_bus()
        .publish("transcript.ready", json!({"capture_id": r.id}));
}

/// AI-8: with the setting absent (default on), a finished transcript starts one `diarize` job
/// as the user with the preset's count; a repeated `transcript.ready` for the same transcript
/// does not start a second one, and the result is shown on the words like a manual run.
#[tokio::test]
async fn ai_8_auto_speakers_run_after_a_transcript_with_the_presets_count() {
    let r = rig_with(
        FakeAi::new()
            .with_small_installed()
            .with_speaker_script(&SCRIPT),
        json!({"mode": "audio", "preset": "room"}),
    )
    .await;
    let mut rx = r.daemon.subscribe();
    r.daemon.start_ai_auto_speakers();
    tokio::task::yield_now().await;
    transcribe_now(&r).await;
    until_jobs_done(&r, 1).await;
    let calls = r.ai.diarize_calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].hints.num_speakers, Some(2));
    assert!(!calls[0].headset);
    // The app learns of the run (job id) so the Speakers tab shows its progress.
    let mut started = vec![];
    while let Ok(e) = rx.try_recv() {
        if e.name == "speakers.started" {
            started.push(e);
        }
    }
    assert_eq!(started.len(), 1);
    assert_eq!(started[0].data["capture_id"], json!(r.id));
    assert_eq!(started[0].data["automatic"], json!(true));
    assert_eq!(started[0].data["num_speakers"], json!(2));
    ready_event(&r);
    ready_event(&r);
    settle().await;
    assert_eq!(diarize_jobs(&r).await, 1);
    let t = user(&r, "get_transcript", json!({})).await.unwrap();
    assert_eq!(names(&t["speakers"]), vec!["Speaker 1", "Speaker 2"]);
    let jobs = user(&r, "list_jobs", json!({"type": "diarize"}))
        .await
        .unwrap();
    assert_eq!(jobs["jobs"][0]["inputs"]["by"], "user");
}

/// The recording's own `people` answer beats the preset; no preset and no answer is three.
#[tokio::test]
async fn ai_8_auto_speakers_use_the_stored_people_answer() {
    for (settings, want) in [
        (json!({"mode": "audio", "preset": "room", "people": 4}), 4),
        (json!({"mode": "audio", "preset": "lecture"}), 3),
        (json!({}), 3),
        (
            json!({"mode": "audio", "preset": "room", "people": null}),
            2,
        ),
    ] {
        let r = rig_with(
            FakeAi::new()
                .with_small_installed()
                .with_speaker_script(&SCRIPT),
            settings.clone(),
        )
        .await;
        r.daemon.start_ai_auto_speakers();
        tokio::task::yield_now().await;
        transcribe_now(&r).await;
        until_jobs_done(&r, 1).await;
        assert_eq!(
            r.ai.diarize_calls()[0].hints.num_speakers,
            Some(want),
            "{settings}"
        );
    }
}

/// Nothing starts: the setting is off, no speaker model, a voice note, one person, a recording
/// that is still being recorded.
#[tokio::test]
async fn ai_8_auto_speakers_skip_cases() {
    // Setting off.
    let r = speaker_rig().await;
    user(
        &r,
        "set_settings",
        json!({"values": {"ai.auto_speakers": false}}),
    )
    .await
    .unwrap();
    r.daemon.start_ai_auto_speakers();
    tokio::task::yield_now().await;
    transcribe_now(&r).await;
    settle().await;
    assert_eq!(diarize_jobs(&r).await, 0);

    // No speaker model: quiet, no job, no failed job.
    let r = rig(FakeAi::new().with_small_installed()).await;
    r.daemon.start_ai_auto_speakers();
    tokio::task::yield_now().await;
    transcribe_now(&r).await;
    settle().await;
    assert_eq!(diarize_jobs(&r).await, 0);

    // A voice note, and a recording with one person.
    for settings in [
        json!({"mode": "audio", "preset": "note"}),
        json!({"mode": "audio", "preset": "room", "people": 1}),
    ] {
        let r = rig_with(
            FakeAi::new()
                .with_small_installed()
                .with_speaker_script(&SCRIPT),
            settings,
        )
        .await;
        r.daemon.start_ai_auto_speakers();
        tokio::task::yield_now().await;
        transcribe_now(&r).await;
        settle().await;
        assert_eq!(diarize_jobs(&r).await, 0);
        assert!(r.ai.diarize_calls().is_empty());
    }

    // Still being recorded.
    let r = speaker_rig().await;
    r.daemon.start_ai_auto_speakers();
    tokio::task::yield_now().await;
    std::fs::write(r.dir.join("recording.lock"), b"{}").unwrap();
    ready_event(&r);
    settle().await;
    assert_eq!(diarize_jobs(&r).await, 0);
}

/// A recording that already has speakers (here renamed by the user) keeps them when its
/// transcript is made again; the names still reach the new words.
#[tokio::test]
async fn ai_8_auto_speakers_never_overwrite_an_existing_result() {
    let r = speaker_rig().await;
    transcribe_and_diarize(&r, json!({})).await;
    user(&r, "set_speakers", json!({"rename": {"2": "Linh"}}))
        .await
        .unwrap();
    r.daemon.start_ai_auto_speakers();
    tokio::task::yield_now().await;
    transcribe_now(&r).await;
    ready_event(&r);
    settle().await;
    assert_eq!(diarize_jobs(&r).await, 1, "only the manual run");
    assert_eq!(r.ai.diarize_calls().len(), 1);
    let t = user(&r, "get_transcript", json!({})).await.unwrap();
    assert_eq!(names(&t["speakers"]), vec!["Speaker 1", "Linh"]);
    assert_eq!(word_speakers(&t).len(), 6);
}

/// A failed automatic run is one failed job; the same transcript is not retried.
#[tokio::test]
async fn ai_8_a_failed_auto_run_is_not_repeated_for_the_same_transcript() {
    let r = speaker_rig().await;
    r.ai.fail_diarize_with(cm_core::CmError::internal("model crashed"));
    r.daemon.start_ai_auto_speakers();
    tokio::task::yield_now().await;
    transcribe_now(&r).await;
    until_jobs_done(&r, 1).await;
    ready_event(&r);
    settle().await;
    assert_eq!(diarize_jobs(&r).await, 1);
    assert_eq!(r.ai.diarize_calls().len(), 1);
}

/// The default and the count rules, without a daemon.
#[test]
fn ai_8_auto_speakers_defaults() {
    use cm_daemon::ai::{auto_speaker_count, auto_speakers_enabled};
    let mut s = Map::new();
    assert!(auto_speakers_enabled(&s));
    s.insert("ai.auto_speakers".into(), json!(false));
    assert!(!auto_speakers_enabled(&s));
    assert_eq!(auto_speaker_count(None, Some("room")), 2);
    assert_eq!(auto_speaker_count(None, Some("note")), 1);
    assert_eq!(auto_speaker_count(None, Some("online")), 3);
    assert_eq!(auto_speaker_count(None, None), 3);
    assert_eq!(auto_speaker_count(Some(5), Some("note")), 5);
    assert_eq!(auto_speaker_count(Some(9), Some("room")), 2);
}
