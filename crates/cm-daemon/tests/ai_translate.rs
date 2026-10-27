//! Caption translation (AI-5) through the daemon with a scripted engine ([`FakeAi`]): the engine
//! choice and the turbo rule, `translations/<lang>.json`, the `language` variants of the caption
//! actions, per-language exports, and edits that survive a re-translate. No Whisper, no network,
//! no text model (the provider is AI-7 and not part of this stage).

#![allow(clippy::unwrap_used)]

mod common;

use std::sync::Arc;
use std::time::Duration;

use cm_ai::transcribe::{Transcript, Word};
use cm_core::{Actor, ErrorCode, MediaTime, TrackInfo, TrackKind, TrackStats};
use cm_daemon::testing::{FakeAi, FixedConfirmer};
use cm_daemon::{Daemon, DaemonError, Decision, Event};
use common::{builder, desktop};
use serde_json::{json, Map, Value};
use time::macros::datetime;
use tokio::sync::broadcast::Receiver;

fn code(e: &DaemonError) -> ErrorCode {
    match e {
        DaemonError::Cm(c) => c.code(),
        DaemonError::WithData { error, .. } => error.code(),
        DaemonError::NotImplemented(_) => ErrorCode::NotImplemented,
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
    confirmer: Arc<FixedConfirmer>,
    _tmp: tempfile::TempDir,
}

fn t(s: f64) -> MediaTime {
    MediaTime::from_secs_f64(s).unwrap()
}

fn word(text: &str, a: f64, b: f64) -> Word {
    Word {
        text: text.into(),
        start: t(a),
        end: t(b),
        confidence: 0.9,
    }
}

fn recording(tmp: &std::path::Path) -> (String, std::path::PathBuf) {
    use cm_library::rec::{RecBundle, SessionInfo};
    let now = datetime!(2026-10-26 10:00:00 UTC);
    let session = SessionInfo {
        clock_origin_ns: 1,
        timebase: "uptime_raw_ns".into(),
        displays: vec![],
        devices: json!({}),
        app_version: "0.0.1".into(),
        extra: Map::new(),
    };
    let rec = RecBundle::create(tmp, "Lesson", now, json!({}), &session).unwrap();
    std::fs::write(rec.dir().join("screen.mp4"), b"master bytes").unwrap();
    std::fs::write(rec.dir().join("mic.wav"), b"audio").unwrap();
    let tracks = vec![
        TrackInfo {
            kind: TrackKind::Screen,
            file: "screen.mp4".into(),
            start: MediaTime::ZERO,
            end: MediaTime::from_secs(60),
            details: json!({}),
            stats: TrackStats::default(),
        },
        TrackInfo {
            kind: TrackKind::Mic,
            file: "mic.wav".into(),
            start: MediaTime::ZERO,
            end: MediaTime::from_secs(60),
            details: json!({}),
            stats: TrackStats::default(),
        },
    ];
    let dir = rec.dir().to_path_buf();
    let m = rec.finalize(&tracks, now).unwrap();
    (m.id, dir)
}

/// A recording whose transcript is in `source` (written straight into the bundle): six words,
/// which the fake caption service cuts into two lines of three.
async fn rig(ai: FakeAi, source: &str) -> Rig {
    let (b, tmp) = builder(desktop());
    let queue = cm_daemon::library_job_queue(tmp.path()).unwrap();
    let confirmer = Arc::new(FixedConfirmer::new(Decision::Decline));
    let daemon = b
        .jobs(queue)
        .confirmer(confirmer.clone())
        .ai(ai.services())
        .build();
    daemon.register_ai_handlers().unwrap();
    daemon.start_jobs().unwrap();
    let (id, dir) = recording(tmp.path());
    let transcript = Transcript {
        version: 1,
        language: source.into(),
        model: "small".into(),
        words: vec![
            word("Xin", 1.0, 1.2),
            word("chào", 1.3, 1.6),
            word("mọi", 1.7, 2.0),
            word("Hôm", 2.4, 2.6),
            word("nay", 2.7, 2.9),
            word("học", 3.0, 3.4),
        ],
        note: None,
    };
    std::fs::write(
        dir.join("transcript.json"),
        serde_json::to_vec(&transcript).unwrap(),
    )
    .unwrap();
    daemon
        .execute_method(Actor::User, 0, "rescan_library", &json!({}))
        .await
        .unwrap();
    Rig {
        daemon,
        ai,
        id,
        dir,
        confirmer,
        _tmp: tmp,
    }
}

async fn call(r: &Rig, who: Actor, method: &str, mut p: Value) -> Result<Value, DaemonError> {
    if matches!(
        method,
        "translate_captions"
            | "get_captions"
            | "set_captions"
            | "replace_in_captions"
            | "export_captions"
    ) && p.get("capture_id").is_none()
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
        let j = user(r, "get_job", json!({"job_id": job_id})).await.unwrap();
        if matches!(j["state"].as_str(), Some("done" | "failed" | "cancelled")) {
            return j;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("timed out");
}

/// Starts a translation and waits for the job; returns the finished job.
async fn translate(r: &Rig, p: Value) -> Value {
    let started = user(r, "translate_captions", p).await.unwrap();
    let job = job_done(r, started["job_id"].as_str().unwrap()).await;
    assert_eq!(job["state"], "done", "{job}");
    job
}

fn drain(rx: &mut Receiver<Event>) -> Vec<Event> {
    let mut v = vec![];
    while let Ok(e) = rx.try_recv() {
        v.push(e);
    }
    v
}

fn texts(v: &Value) -> Vec<String> {
    v["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["text"].as_str().unwrap().to_owned())
        .collect()
}

/// Lines read back from `get_captions` (ticks) in the form `set_captions` takes (`t:<ticks>`).
fn settable(v: &Value) -> Value {
    Value::Array(
        v["lines"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| {
                json!({ "start": format!("t:{}", l["start"]), "end": format!("t:{}", l["end"]),
                        "text": l["text"] })
            })
            .collect(),
    )
}

fn english(first: &str, second: &str) -> Vec<Word> {
    // Two sentences over the two source lines (1.0 to 2.0 and 2.4 to 3.4).
    let mut w = vec![];
    for (i, text) in first.split(' ').enumerate() {
        w.push(word(text, 1.0 + i as f64 * 0.3, 1.2 + i as f64 * 0.3));
    }
    for (i, text) in second.split(' ').enumerate() {
        w.push(word(text, 2.4 + i as f64 * 0.3, 2.6 + i as f64 * 0.3));
    }
    w
}

/// AI-5.AC2 and the engine choice: Whisper into English uses the large-v3 model, the job result
/// names it, `translations/en.json` has the stored shape, and `translation.ready` is published.
#[tokio::test]
async fn ai_5_whisper_translates_to_english_with_large_v3() {
    let ai = FakeAi::new().with_installed(&["small", "medium", "large"], Some("small"));
    let r = rig(ai, "vi").await;
    r.ai.set_translation(english("Hello everyone.", "Today we learn."));
    let mut rx = r.daemon.subscribe();
    let started = user(&r, "translate_captions", json!({"target_language": "en"}))
        .await
        .unwrap();
    // Auto picked Whisper; the active model (small) can translate, so it is the one.
    assert_eq!(started["engine"], "whisper");
    assert_eq!(started["model"], "small");
    let job = job_done(&r, started["job_id"].as_str().unwrap()).await;
    assert_eq!(job["state"], "done", "{job}");
    let res = &job["result"];
    assert_eq!(res["language"], "en");
    assert_eq!(res["lines"], 2);
    assert_eq!(res["untranslated"], 0);
    assert!(
        r.confirmer.shown().is_empty(),
        "translating with Whisper is Free"
    );

    let calls = r.ai.translate_calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].source_language, "vi");
    assert_eq!(calls[0].source_lines.len(), 2);

    let file: Value = serde_json::from_str(
        &std::fs::read_to_string(r.dir.join("translations").join("en.json")).unwrap(),
    )
    .unwrap();
    for k in ["language", "engine", "model", "created", "lines", "edited"] {
        assert!(file.get(k).is_some(), "{k}");
    }
    assert_eq!(file["engine"], "whisper");
    assert_eq!(file["edited"], false);
    assert!(file["created"].as_str().unwrap().contains('T'));
    let lines = file["lines"].as_array().unwrap();
    assert_eq!(lines.len(), 2);
    for l in lines {
        let text = l["text"].as_str().unwrap();
        assert!(text.split('\n').count() <= 2);
        assert!(text.split('\n').all(|row| row.chars().count() <= 42));
    }

    let events = drain(&mut rx);
    let ev = events
        .iter()
        .find(|e| e.name == "translation.ready")
        .expect("translation.ready");
    assert_eq!(ev.data["capture_id"], json!(r.id));
    assert_eq!(ev.data["language"], "en");

    let got = user(&r, "get_captions", json!({"language": "en"}))
        .await
        .unwrap();
    assert_eq!(got["source"], "translation");
    assert_eq!(texts(&got), ["Hello everyone.", "Today we learn."]);
    // The original captions are untouched.
    let orig = user(&r, "get_captions", json!({})).await.unwrap();
    assert_eq!(orig["source"], "generated");
    assert_eq!(texts(&orig), ["Xin chào mọi", "Hôm nay học"]);
}

/// AI-5.AC2: with the turbo model active, the job uses large-v3, else medium, else fails with
/// `translation_model_missing`; nothing is downloaded either way.
#[tokio::test]
async fn ai_5_ac2_the_turbo_model_is_never_used_to_translate() {
    let turbo_active = |ids: &[&str]| FakeAi::new().with_installed(ids, Some("large-v3-turbo"));

    let r = rig(
        turbo_active(&["small", "medium", "large", "large-v3-turbo"]),
        "vi",
    )
    .await;
    let started = user(&r, "translate_captions", json!({"target_language": "en"}))
        .await
        .unwrap();
    assert_eq!(started["model"], "large");
    let job = job_done(&r, started["job_id"].as_str().unwrap()).await;
    assert_eq!(
        job["result"]["model"], "large",
        "the result names the model"
    );
    assert_eq!(r.ai.translate_calls()[0].model, "large");
    let file: Value =
        serde_json::from_str(&std::fs::read_to_string(r.dir.join("translations/en.json")).unwrap())
            .unwrap();
    assert_eq!(file["model"], "large");

    let r = rig(turbo_active(&["small", "medium", "large-v3-turbo"]), "vi").await;
    let started = user(&r, "translate_captions", json!({"target_language": "en"}))
        .await
        .unwrap();
    assert_eq!(started["model"], "medium");

    let r = rig(turbo_active(&["small", "large-v3-turbo"]), "vi").await;
    let e = user(&r, "translate_captions", json!({"target_language": "en"}))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
    assert_eq!(reason(&e), "translation_model_missing");
    assert!(r.ai.downloads_started().is_empty(), "nothing is downloaded");
    assert!(r.ai.translate_calls().is_empty());
    let jobs = user(&r, "list_jobs", json!({})).await.unwrap();
    assert!(jobs["jobs"].as_array().unwrap().is_empty());
    assert!(!r.dir.join("translations").exists());
}

/// A target no engine can reach says so, with the reason a client can act on.
#[tokio::test]
async fn ai_5_other_targets_need_a_text_model() {
    let ai = FakeAi::new().with_installed(&["small", "large"], Some("large"));
    let r = rig(ai, "en").await;
    for engine in ["auto", "text_model"] {
        let e = user(
            &r,
            "translate_captions",
            json!({"target_language": "vi", "engine": engine}),
        )
        .await
        .unwrap_err();
        assert_eq!(code(&e), ErrorCode::InvalidInput);
        assert_eq!(reason(&e), "no_text_model", "{engine}");
    }
    // Whisper cannot reach Vietnamese at all.
    let e = user(
        &r,
        "translate_captions",
        json!({"target_language": "vi", "engine": "whisper"}),
    )
    .await
    .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
    // The recording is already English.
    let e = user(&r, "translate_captions", json!({"target_language": "en"}))
        .await
        .unwrap_err();
    assert_eq!(reason(&e), "same_language");
    // Bad input.
    for bad in [
        json!({"target_language": "../x"}),
        json!({"target_language": "en", "engine": "gpt"}),
        json!({}),
    ] {
        let e = user(&r, "translate_captions", bad).await.unwrap_err();
        assert_eq!(code(&e), ErrorCode::InvalidInput);
    }
    assert!(r.ai.translate_calls().is_empty());
}

#[tokio::test]
async fn ai_5_needs_a_transcript_and_a_translation_to_read() {
    let ai = FakeAi::new().with_installed(&["large"], Some("large"));
    let r = rig(ai, "vi").await;
    // No translation yet: not_found with the way to make one.
    let e = user(&r, "get_captions", json!({"language": "en"}))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::NotFound);
    assert!(e.to_string().contains("translate_captions"), "{e}");
    for m in ["set_captions", "replace_in_captions", "export_captions"] {
        let p = match m {
            "set_captions" => json!({"language": "en", "lines": []}),
            "replace_in_captions" => {
                json!({"language": "en", "replacements": [{"find": "a", "replace": "b"}]})
            }
            _ => json!({"language": "en", "format": "srt"}),
        };
        let e = user(&r, m, p).await.unwrap_err();
        assert_eq!(code(&e), ErrorCode::NotFound, "{m}");
    }
    // No transcript: not_found.
    std::fs::remove_file(r.dir.join("transcript.json")).unwrap();
    let e = user(&r, "translate_captions", json!({"target_language": "en"}))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::NotFound);
    assert!(e.to_string().contains("transcribe"), "{e}");
}

/// AI-5.AC5: an edited line survives a re-translate while its source line is unchanged, and is
/// dropped when the source line was edited.
#[tokio::test]
async fn ai_5_ac5_edits_survive_a_retranslate() {
    let ai = FakeAi::new().with_installed(&["large"], Some("large"));
    let r = rig(ai, "vi").await;
    r.ai.set_translation(english("Hello everyone.", "Today we learn."));
    translate(&r, json!({"target_language": "en"})).await;

    // The user fixes the first line (set_captions with a language).
    let got = user(&r, "get_captions", json!({"language": "en"}))
        .await
        .unwrap();
    let mut lines = settable(&got);
    lines[0]["text"] = json!("Hello, dear friends.");
    let set = user(
        &r,
        "set_captions",
        json!({"language": "en", "lines": lines}),
    )
    .await
    .unwrap();
    assert_eq!(set["edited"], true);
    let got = user(&r, "get_captions", json!({"language": "en"}))
        .await
        .unwrap();
    assert_eq!(got["edited_lines"], json!([0]));
    assert_eq!(got["edited"], true);

    // Re-translate with different Whisper output: the edit stays, the other line is new.
    r.ai.set_translation(english("Hi all.", "We study today."));
    let job = translate(&r, json!({"target_language": "en"})).await;
    assert_eq!(job["result"]["kept_edits"], 1);
    assert_eq!(job["result"]["dropped_edits"], 0);
    let got = user(&r, "get_captions", json!({"language": "en"}))
        .await
        .unwrap();
    assert_eq!(texts(&got), ["Hello, dear friends.", "We study today."]);
    assert_eq!(got["edited_lines"], json!([0]));

    // The user edits the SOURCE line behind the edit: the edit no longer applies.
    let orig = user(&r, "get_captions", json!({})).await.unwrap();
    let mut src = settable(&orig);
    src[0]["text"] = json!("Xin chào các bạn");
    user(&r, "set_captions", json!({"lines": src}))
        .await
        .unwrap();
    r.ai.set_translation(english("Hi all.", "We study today."));
    let job = translate(&r, json!({"target_language": "en"})).await;
    assert_eq!(job["result"]["kept_edits"], 0);
    assert_eq!(job["result"]["dropped_edits"], 1);
    let got = user(&r, "get_captions", json!({"language": "en"}))
        .await
        .unwrap();
    assert_eq!(texts(&got), ["Hi all.", "We study today."]);
    assert_eq!(got["edited"], false);
}

/// AI-5: an edit of a translation (set_captions or replace_in_captions with a language) is a
/// manifest revision and an undo step; undo and redo put the file back; a re-translate drops the
/// older steps so undo cannot bring back lines of the previous run.
#[tokio::test]
async fn ai_5_translation_edits_are_undo_steps() {
    let ai = FakeAi::new().with_installed(&["large"], Some("large"));
    let r = rig(ai, "vi").await;
    r.ai.set_translation(english("Hello everyone.", "Today we learn."));
    translate(&r, json!({"target_language": "en"})).await;
    let mut rx = r.daemon.subscribe();
    let id = json!({"capture_id": r.id});

    let got = user(&r, "get_captions", json!({"language": "en"}))
        .await
        .unwrap();
    let mut lines = settable(&got);
    lines[0]["text"] = json!("Hello, dear friends.");
    let set = user(
        &r,
        "set_captions",
        json!({"language": "en", "lines": lines}),
    )
    .await
    .unwrap();
    assert_eq!(set["action"], "set_captions");
    assert!(set["revision"].as_u64().unwrap() > 0, "{set}");
    assert_eq!(set["can_undo"], true);
    assert!(drain(&mut rx).iter().any(|e| e.name == "capture.revised"));

    let rep = user(
        &r,
        "replace_in_captions",
        json!({"language": "en", "replacements": [{"find": "learn", "replace": "study"}]}),
    )
    .await
    .unwrap();
    assert_eq!(rep["can_undo"], true);
    let now = user(&r, "get_captions", json!({"language": "en"}))
        .await
        .unwrap();
    assert_eq!(texts(&now), ["Hello, dear friends.", "Today we study."]);

    // Undo the replace, then the edit; redo both.
    user(&r, "undo", id.clone()).await.unwrap();
    let g = user(&r, "get_captions", json!({"language": "en"}))
        .await
        .unwrap();
    assert_eq!(texts(&g), ["Hello, dear friends.", "Today we learn."]);
    user(&r, "undo", id.clone()).await.unwrap();
    let g = user(&r, "get_captions", json!({"language": "en"}))
        .await
        .unwrap();
    assert_eq!(texts(&g), ["Hello everyone.", "Today we learn."]);
    assert_eq!(g["edited"], false);
    user(&r, "redo", id.clone()).await.unwrap();
    user(&r, "redo", id.clone()).await.unwrap();
    let g = user(&r, "get_captions", json!({"language": "en"}))
        .await
        .unwrap();
    assert_eq!(texts(&g), ["Hello, dear friends.", "Today we study."]);

    // A re-translate replaces the file: the older steps are gone.
    r.ai.set_translation(english("Hi all.", "We study today."));
    translate(&r, json!({"target_language": "en"})).await;
    let u = user(&r, "undo", id.clone()).await.unwrap();
    assert_eq!(u["changed"], false, "{u}");
    let g = user(&r, "get_captions", json!({"language": "en"}))
        .await
        .unwrap();
    assert_eq!(texts(&g), ["Hello, dear friends.", "Today we study."]);
}

/// Per-language files: SRT, VTT and TXT of a translation, next to the original's, and find and
/// replace on a translation.
#[tokio::test]
async fn ai_5_exports_and_replace_per_language() {
    let ai = FakeAi::new().with_installed(&["large"], Some("large"));
    let r = rig(ai, "vi").await;
    r.ai.set_translation(english("Hello everyone.", "Today we learn."));
    translate(&r, json!({"target_language": "en"})).await;

    let rep = user(
        &r,
        "replace_in_captions",
        json!({"language": "en", "replacements": [{"find": "everyone", "replace": "friends"}]}),
    )
    .await
    .unwrap();
    assert_eq!(rep["replacements"], 1);
    assert_eq!(rep["changed"], true);
    let none = user(
        &r,
        "replace_in_captions",
        json!({"language": "en", "replacements": [{"find": "zzz", "replace": "y"}]}),
    )
    .await
    .unwrap();
    assert_eq!(none["changed"], false);
    // The original captions were not touched.
    let orig = user(&r, "get_captions", json!({})).await.unwrap();
    assert_eq!(texts(&orig), ["Xin chào mọi", "Hôm nay học"]);

    for (format, ext, needle) in [
        ("srt", "srt", "Hello friends."),
        ("vtt", "vtt", "Hello friends."),
        ("txt", "txt", "Today we learn."),
    ] {
        let out = user(
            &r,
            "export_captions",
            json!({"language": "en", "format": format}),
        )
        .await
        .unwrap();
        assert_eq!(out["language"], "en");
        let path = std::path::PathBuf::from(out["path"].as_str().unwrap());
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.contains(".en."), "{name}");
        assert!(name.ends_with(ext), "{name}");
        let body = std::fs::read_to_string(&path).unwrap();
        assert!(body.contains(needle), "{format}: {body}");
    }
    // The original's export is a different file.
    let orig = user(&r, "export_captions", json!({"format": "srt"}))
        .await
        .unwrap();
    assert!(!orig["path"].as_str().unwrap().contains(".en."));
    assert!(std::fs::read_to_string(orig["path"].as_str().unwrap())
        .unwrap()
        .contains("Xin chào mọi"));

    // A language code must be a code.
    let e = user(&r, "get_captions", json!({"language": "../en"}))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
}

/// The same job twice is one job; the agent actor gets the same Free behaviour.
#[tokio::test]
async fn ai_5_an_equal_running_translation_is_reused_and_agents_need_no_confirmation() {
    let ai = FakeAi::new().with_installed(&["large"], Some("large"));
    ai.set_slow(4, 60);
    let r = rig(ai, "vi").await;
    let agent = Actor::agent("claude").unwrap();
    let a = call(
        &r,
        agent.clone(),
        "translate_captions",
        json!({"target_language": "en"}),
    )
    .await
    .unwrap();
    let b = call(
        &r,
        agent,
        "translate_captions",
        json!({"target_language": "en"}),
    )
    .await
    .unwrap();
    assert_eq!(a["job_id"], b["job_id"]);
    assert_eq!(b["reused"], true);
    assert!(r.confirmer.shown().is_empty());
    job_done(&r, a["job_id"].as_str().unwrap()).await;
}
