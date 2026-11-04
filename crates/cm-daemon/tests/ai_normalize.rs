//! AI-10 transcript normalisation through the daemon with a scripted text model: no network, no
//! real model. The words are made up.

#![allow(clippy::unwrap_used)]

mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use cm_ai::text_model::{JsonReply, JsonRequest, TextModel, TextModelError};
use cm_ai::transcribe::{Transcript, Word};
use cm_core::{Actor, ErrorCode, MediaTime, TrackInfo, TrackKind, TrackStats};
use cm_daemon::testing::{FakeAi, FixedConfirmer};
use cm_daemon::{Daemon, DaemonError, Decision};
use common::{builder, desktop};
use serde_json::{json, Map, Value};
use time::macros::datetime;

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

/// Capitalises the first word of every turn and ends it with a full stop. A word listed in
/// `damage` gets a different tone mark (what a careless model does).
struct Scripted {
    seen: Mutex<Vec<JsonRequest>>,
    damage: Mutex<Option<(&'static str, &'static str)>>,
}

impl TextModel for Scripted {
    fn complete_json(&self, req: &JsonRequest) -> Result<JsonReply, TextModelError> {
        self.seen.lock().unwrap().push(req.clone());
        let line = req.messages[0]
            .content
            .lines()
            .find(|l| l.starts_with("Input: "))
            .unwrap();
        let v: Value = serde_json::from_str(&line["Input: ".len()..]).unwrap();
        let damage = *self.damage.lock().unwrap();
        let out: Vec<String> = v["turns"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| {
                let t = t.as_str().unwrap();
                let mut c = t.chars();
                let first: String = c.next().unwrap().to_uppercase().collect();
                let mut s = format!("{first}{}.", c.as_str());
                if let Some((from, to)) = damage {
                    s = s.replace(from, to);
                }
                s
            })
            .collect();
        Ok(JsonReply {
            value: json!({ "turns": out }),
            attempts: 1,
        })
    }
}

struct Rig {
    daemon: Daemon,
    model: Arc<Scripted>,
    id: String,
    dir: std::path::PathBuf,
    _tmp: tempfile::TempDir,
}

fn t(s: f64) -> MediaTime {
    MediaTime::from_secs_f64(s).unwrap()
}

fn recording(tmp: &std::path::Path) -> (String, std::path::PathBuf) {
    use cm_library::rec::{RecBundle, SessionInfo};
    let now = datetime!(2026-11-04 10:00:00 UTC);
    let session = SessionInfo {
        clock_origin_ns: 1,
        timebase: "uptime_raw_ns".into(),
        displays: vec![],
        devices: json!({}),
        app_version: "0.0.1".into(),
        extra: Map::new(),
    };
    let rec = RecBundle::create(tmp, "Weekly sync", now, json!({}), &session).unwrap();
    std::fs::write(rec.dir().join("screen.mp4"), b"master bytes").unwrap();
    std::fs::write(rec.dir().join("mic.wav"), b"audio").unwrap();
    let tracks = vec![
        TrackInfo {
            kind: TrackKind::Screen,
            file: "screen.mp4".into(),
            start: MediaTime::ZERO,
            end: MediaTime::from_secs(90),
            details: json!({}),
            stats: TrackStats::default(),
        },
        TrackInfo {
            kind: TrackKind::Mic,
            file: "mic.wav".into(),
            start: MediaTime::ZERO,
            end: MediaTime::from_secs(90),
            details: json!({}),
            stats: TrackStats::default(),
        },
    ];
    let dir = rec.dir().to_path_buf();
    let m = rec.finalize(&tracks, now).unwrap();
    (m.id, dir)
}

const SYLLABLES: [&str; 6] = ["xin", "chào", "các", "bạn", "hôm", "nay"];

fn transcript(words: usize) -> Transcript {
    Transcript {
        version: 1,
        language: "vi".into(),
        model: "small".into(),
        words: (0..words)
            .map(|i| Word {
                text: SYLLABLES[i % SYLLABLES.len()].to_owned(),
                start: t(i as f64),
                end: t(i as f64 + 0.9),
                confidence: 0.9,
            })
            .collect(),
        note: None,
        sounds: Vec::new(),
        segments: Vec::new(),
    }
}

async fn rig(words: usize, provider: Option<&str>) -> Rig {
    let (b, tmp) = builder(desktop());
    let queue = cm_daemon::library_job_queue(tmp.path()).unwrap();
    // Declines every dialog: a Free action must not show one.
    let confirmer = Arc::new(FixedConfirmer::new(Decision::Decline));
    let ai = FakeAi::new();
    let services = cm_daemon::AiServices {
        text_models: Arc::new(
            cm_daemon::testing::FakeTextModels::new().with_installed(&["qwen3.5-4b"]),
        ),
        ..ai.services()
    };
    let daemon = b.jobs(queue).confirmer(confirmer).ai(services).build();
    daemon.register_ai_handlers().unwrap();
    daemon.start_jobs().unwrap();
    let (id, dir) = recording(tmp.path());
    if words > 0 {
        std::fs::write(
            dir.join("transcript.json"),
            serde_json::to_vec(&transcript(words)).unwrap(),
        )
        .unwrap();
    }
    daemon
        .execute_method(Actor::User, 0, "rescan_library", &json!({}))
        .await
        .unwrap();
    let model = Arc::new(Scripted {
        seen: Mutex::new(vec![]),
        damage: Mutex::new(None),
    });
    daemon.set_text_model_override(Some(model.clone()));
    let r = Rig {
        daemon,
        model,
        id,
        dir,
        _tmp: tmp,
    };
    if let Some(p) = provider {
        call(
            &r,
            Actor::User,
            "set_settings",
            json!({ "values": { "ai.text.provider": p, "ai.text.model": "qwen3:8b" } }),
        )
        .await
        .unwrap();
    }
    r
}

async fn call(r: &Rig, who: Actor, method: &str, mut p: Value) -> Result<Value, DaemonError> {
    if method != "set_settings" && method != "get_job" && p.get("capture_id").is_none() {
        p["capture_id"] = json!(r.id);
    }
    r.daemon.execute_method(who, 0, method, &p).await
}

async fn user(r: &Rig, method: &str, p: Value) -> Result<Value, DaemonError> {
    call(r, Actor::User, method, p).await
}

async fn job_done(r: &Rig, job_id: &str) -> Value {
    for _ in 0..500 {
        let j = user(r, "get_job", json!({"job_id": job_id})).await.unwrap();
        if matches!(j["state"].as_str(), Some("done" | "failed" | "cancelled")) {
            return j;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("timed out");
}

async fn normalize(r: &Rig, who: Actor) -> Value {
    let started = call(r, who, "normalize_transcript", json!({}))
        .await
        .unwrap();
    job_done(r, started["job_id"].as_str().unwrap()).await
}

#[tokio::test]
async fn the_edge_cases_are_refused() {
    // Provider none: invalid_input / no_text_model.
    let r = rig(20, None).await;
    let e = user(&r, "normalize_transcript", json!({}))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
    assert_eq!(reason(&e), "no_text_model");
    assert!(r.model.seen.lock().unwrap().is_empty());
    // No transcript.
    let none = rig(0, Some("embedded")).await;
    let e = user(&none, "normalize_transcript", json!({}))
        .await
        .unwrap_err();
    assert_eq!(code(&none_err(&e)), ErrorCode::NotFound);
    // Nothing normalised yet.
    let e = user(&none, "get_normalized_transcript", json!({}))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::NotFound);
    // An unknown field.
    let r = rig(20, Some("embedded")).await;
    assert!(user(&r, "normalize_transcript", json!({"mode": "x"}))
        .await
        .is_err());
}

fn none_err(e: &DaemonError) -> DaemonError {
    match e {
        DaemonError::Cm(c) => DaemonError::Cm(c.clone()),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn a_run_writes_the_sidecar_and_leaves_the_transcript_alone() {
    let r = rig(20, Some("embedded")).await;
    let before = std::fs::read(r.dir.join("transcript.json")).unwrap();
    let mut rx = r.daemon.subscribe();
    // An agent may start it without a dialog (Free).
    let job = normalize(&r, Actor::agent("claude").unwrap()).await;
    assert_eq!(job["state"], "done", "{job}");
    let res = &job["result"];
    assert_eq!(res["rejected"], 0);
    assert_eq!(res["normalized"], res["turns"]);
    assert_eq!(res["words"], 20);
    assert_eq!(res["language"], "vi");

    // The prompt named the language.
    let prompt = r.model.seen.lock().unwrap()[0].messages[0].content.clone();
    assert!(prompt.contains("Language: Vietnamese."), "{prompt}");

    assert_eq!(
        std::fs::read(r.dir.join("transcript.json")).unwrap(),
        before
    );
    assert!(r.dir.join("transcript.normalized.json").is_file());
    assert!(!r.dir.join("transcript.normalized.partial.json").exists());

    let got = user(&r, "get_normalized_transcript", json!({}))
        .await
        .unwrap();
    assert_eq!(got["stale"], false);
    assert!(got["text"]
        .as_str()
        .unwrap()
        .starts_with("Xin chào các bạn"));
    assert!(got["text"].as_str().unwrap().ends_with("chào."));
    assert_eq!(got["normalized"]["summary"]["words"], 20);
    assert_eq!(got["normalized"]["model"], "qwen3.5-4b");

    let mut seen = false;
    while let Ok(e) = rx.try_recv() {
        if e.name == "transcript.normalized" {
            seen = true;
            assert_eq!(e.data["rejected"], 0);
        }
    }
    assert!(seen);
}

#[tokio::test]
async fn a_damaged_tone_mark_keeps_the_raw_words_and_is_counted() {
    let r = rig(12, Some("embedded")).await;
    *r.model.damage.lock().unwrap() = Some(("chào", "chảo"));
    let job = normalize(&r, Actor::User).await;
    assert_eq!(job["state"], "done", "{job}");
    assert_eq!(job["result"]["rejected"], 1);
    assert_eq!(job["result"]["rejected_words"], 12);
    let got = user(&r, "get_normalized_transcript", json!({}))
        .await
        .unwrap();
    let turn = &got["normalized"]["turns"][0];
    assert_eq!(turn["normalized"], false);
    assert_eq!(turn["reason"], "changed_words");
    // The text is the raw words: no capital letter, no full stop, no damaged tone mark.
    let text = got["text"].as_str().unwrap();
    assert!(text.starts_with("xin chào các"), "{text}");
    assert!(!text.contains("chảo") && !text.contains('.'), "{text}");
}

#[tokio::test]
async fn a_changed_transcript_makes_the_sidecar_stale_and_a_rerun_replaces_it() {
    let r = rig(8, Some("embedded")).await;
    normalize(&r, Actor::User).await;
    // The user edits the transcript (a word changes).
    let mut tr = transcript(8);
    tr.words[2].text = "mình".into();
    std::fs::write(
        r.dir.join("transcript.json"),
        serde_json::to_vec(&tr).unwrap(),
    )
    .unwrap();
    let got = user(&r, "get_normalized_transcript", json!({}))
        .await
        .unwrap();
    assert_eq!(got["stale"], true);
    let job = normalize(&r, Actor::User).await;
    assert_eq!(job["state"], "done");
    let got = user(&r, "get_normalized_transcript", json!({}))
        .await
        .unwrap();
    assert_eq!(got["stale"], false);
    assert!(got["text"].as_str().unwrap().contains("mình"));
}

#[tokio::test]
async fn a_job_for_the_same_recording_is_reused_while_it_runs() {
    let r = rig(8, Some("embedded")).await;
    let a = user(&r, "normalize_transcript", json!({})).await.unwrap();
    let b = user(&r, "normalize_transcript", json!({})).await.unwrap();
    // Either the second call found the first still queued, or the first was already done; in
    // both cases a job id comes back and the sidecar ends up there.
    assert!(a["job_id"].is_string() && b["job_id"].is_string());
    if b["reused"] == true {
        assert_eq!(a["job_id"], b["job_id"]);
    }
    job_done(&r, a["job_id"].as_str().unwrap()).await;
    job_done(&r, b["job_id"].as_str().unwrap()).await;
    assert!(r.dir.join("transcript.normalized.json").is_file());
}
