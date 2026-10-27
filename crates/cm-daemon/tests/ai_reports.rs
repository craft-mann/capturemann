//! AI-9 reports through the daemon with a scripted text model: no network, no real model. The
//! provider settings point at loopback (the model itself is the test double), so the local-only
//! rule is exercised for real.

#![allow(clippy::unwrap_used)]

mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use cm_ai::text_model::{JsonReply, JsonRequest, TextModel, TextModelError};
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

/// Answers from a closure; records the requests; can be switched to fail.
struct Scripted {
    seen: Mutex<Vec<JsonRequest>>,
    fail: Mutex<bool>,
    answer: Mutex<Value>,
}

impl TextModel for Scripted {
    fn complete_json(&self, req: &JsonRequest) -> Result<JsonReply, TextModelError> {
        self.seen.lock().unwrap().push(req.clone());
        if *self.fail.lock().unwrap() {
            return Err(TextModelError {
                kind: cm_ai::text_model::ErrorKind::Unreachable,
                message: "Open the AI page and check the built-in text model".into(),
            });
        }
        Ok(JsonReply {
            value: self.answer.lock().unwrap().clone(),
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
    let now = datetime!(2026-10-26 10:00:00 UTC);
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

fn minutes_answer() -> Value {
    json!({
        "summary": [{"text": "They plan the pricing sheet.", "at": [1.0]}],
        "attendees": [{"name": "Linh", "at": [1.0]}],
        "topics": [],
        "decisions": [{"text": "Send by Friday", "at": [10.0]}],
        "action_items": [
            {"task": "Send the sheet", "owner": "Linh", "due": "Friday", "at": [10.0]},
            {"task": "Invented task", "owner": "Linh", "due": "", "at": [5000.0]}
        ],
        "open_questions": []
    })
}

async fn rig(words: usize, provider: Option<&str>) -> Rig {
    let (b, tmp) = builder(desktop());
    let queue = cm_daemon::library_job_queue(tmp.path()).unwrap();
    let confirmer = Arc::new(FixedConfirmer::new(Decision::Decline));
    let ai = FakeAi::new();
    // The built-in provider needs a downloaded model to pass the setup check; the scripted
    // model replaces the engine.
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
        let transcript = Transcript {
            version: 1,
            language: "en".into(),
            model: "small".into(),
            words: (0..words)
                .map(|i| Word {
                    text: format!("word{i}"),
                    start: t(i as f64),
                    end: t(i as f64 + 0.9),
                    confidence: 0.9,
                })
                .collect(),
            note: None,
        };
        std::fs::write(
            dir.join("transcript.json"),
            serde_json::to_vec(&transcript).unwrap(),
        )
        .unwrap();
    }
    daemon
        .execute_method(Actor::User, 0, "rescan_library", &json!({}))
        .await
        .unwrap();
    let model = Arc::new(Scripted {
        seen: Mutex::new(vec![]),
        fail: Mutex::new(false),
        answer: Mutex::new(minutes_answer()),
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
        set_provider(&r, p).await;
    }
    r
}

async fn set_provider(r: &Rig, provider: &str) {
    call(
        r,
        Actor::User,
        "set_settings",
        json!({ "values": { "ai.text.provider": provider,
                            "ai.text.model": "qwen3:8b" } }),
    )
    .await
    .unwrap();
}

async fn call(r: &Rig, who: Actor, method: &str, mut p: Value) -> Result<Value, DaemonError> {
    if method != "set_settings"
        && method != "get_job"
        && method != "list_report_templates"
        && p.get("capture_id").is_none()
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

async fn generate(r: &Rig, p: Value) -> Value {
    let started = user(r, "generate_report", p).await.unwrap();
    job_done(r, started["job_id"].as_str().unwrap()).await
}

fn drain(rx: &mut Receiver<Event>) -> Vec<Event> {
    let mut v = vec![];
    while let Ok(e) = rx.try_recv() {
        v.push(e);
    }
    v
}

#[tokio::test]
async fn templates_are_listed_and_the_edge_cases_are_refused() {
    let r = rig(60, None).await;
    let l = user(&r, "list_report_templates", json!({})).await.unwrap();
    assert_eq!(l["templates"].as_array().unwrap().len(), 6);
    assert!(l["templates"][3]["notes_only"]
        .as_str()
        .unwrap()
        .starts_with("Notes only"));
    // Provider none: invalid_input / no_text_model.
    let e = user(&r, "generate_report", json!({"template": "minutes"}))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
    assert_eq!(reason(&e), "no_text_model");
    // Unknown template and a context the template does not use.
    let e = user(&r, "generate_report", json!({"template": "poem"}))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
    let e = user(
        &r,
        "generate_report",
        json!({"template": "minutes", "context": {"role": "Engineer"}}),
    )
    .await
    .unwrap_err();
    assert!(e.to_string().contains("context.role"), "{e}");
    // No transcript.
    let none = rig(0, Some("embedded")).await;
    let e = user(&none, "generate_report", json!({"template": "minutes"}))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::NotFound);
    // Too little speech.
    let short = rig(12, Some("embedded")).await;
    let e = user(&short, "generate_report", json!({"template": "summary"}))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
    assert!(
        e.to_string().contains("Too little speech for a report"),
        "{e}"
    );
}

/// Scope decision (Oct 27, 2026): only the built-in text model. Any other provider (`local`,
/// `openai_compatible`, `anthropic`) fails as `no_text_model`, with no dialog and no request.
#[tokio::test]
async fn only_the_built_in_text_model_is_allowed() {
    let r = rig(60, None).await;
    for provider in ["openai_compatible", "anthropic", "local"] {
        set_provider(&r, provider).await;
        for who in [Actor::User, Actor::agent("claude").unwrap()] {
            let e = call(&r, who, "generate_report", json!({"template": "summary"}))
                .await
                .unwrap_err();
            assert_eq!(code(&e), ErrorCode::InvalidInput, "{provider}");
            assert_eq!(reason(&e), "no_text_model", "{provider}");
        }
    }
    assert!(r.model.seen.lock().unwrap().is_empty());
}

#[tokio::test]
async fn generate_get_export_and_drop_the_uncited() {
    let r = rig(60, Some("embedded")).await;
    let mut rx = r.daemon.subscribe();
    // A marker 20 s in, and the agent path (Free for a local model).
    user(&r, "add_marker", json!({"at": 20, "label": "pricing"}))
        .await
        .unwrap();
    let started = call(
        &r,
        Actor::agent("claude").unwrap(),
        "generate_report",
        json!({"template": "minutes", "language": "vi",
               "context": {"title": "Weekly", "attendees": ["Linh Tran", "Minh"]}}),
    )
    .await
    .unwrap();
    let job = job_done(&r, started["job_id"].as_str().unwrap()).await;
    assert_eq!(job["state"], "done", "{job}");
    let res = &job["result"];
    let rid = res["report_id"].as_str().unwrap().to_owned();
    assert_eq!(res["dropped_uncited"], 1, "AC3");
    assert_eq!(res["language"], "vi", "AC7");

    // AC7: the language reached the model.
    let req = r.model.seen.lock().unwrap()[0].clone();
    assert!(req
        .system
        .unwrap()
        .contains("Write all the note text in Vietnamese"));
    let user_prompt = &req.messages[0].content;
    assert!(user_prompt.contains("Attendees: Linh Tran, Minh"));
    assert!(user_prompt.contains("[20.0] pricing"));

    let got = user(&r, "get_report", json!({"report_id": rid}))
        .await
        .unwrap();
    assert_eq!(got["report"]["dropped_uncited"], 1);
    assert_eq!(got["report"]["language"], "vi");
    assert_eq!(got["report"]["provider"], "embedded");
    assert_eq!(got["report"]["model"], "qwen3.5-4b");
    let items = got["report"]["content"]["action_items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["owner"], "Linh");
    assert_eq!(
        got["report"]["content"]["marked_moments"][0]["label"],
        "pricing"
    );
    assert!(got["markdown"].as_str().unwrap().starts_with("# Weekly"));
    assert!(r.dir.join("reports").join(format!("{rid}.json")).is_file());
    assert!(r.dir.join("reports").join(format!("{rid}.md")).is_file());

    // The list, without an id.
    let list = user(&r, "get_report", json!({})).await.unwrap();
    assert_eq!(list["reports"].as_array().unwrap().len(), 1);
    assert_eq!(list["latest"]["report_id"], rid);

    // Exports never overwrite.
    let md = user(
        &r,
        "export_report",
        json!({"report_id": rid, "format": "md"}),
    )
    .await
    .unwrap();
    let md2 = user(
        &r,
        "export_report",
        json!({"report_id": rid, "format": "md"}),
    )
    .await
    .unwrap();
    assert_ne!(md["path"], md2["path"]);
    let txt = user(
        &r,
        "export_report",
        json!({"report_id": rid, "format": "txt"}),
    )
    .await
    .unwrap();
    let text = std::fs::read_to_string(txt["path"].as_str().unwrap()).unwrap();
    assert!(text.starts_with("WEEKLY") && !text.contains("**"), "{text}");

    let events = drain(&mut rx);
    let ready = events.iter().find(|e| e.name == "report.ready").unwrap();
    assert_eq!(ready.data["report_id"], rid);
    assert_eq!(ready.data["dropped_uncited"], 1);
}

/// AI-9.AC4 through the daemon: the stored Interview report has no judging field.
#[tokio::test]
async fn interview_report_has_no_score_field() {
    let r = rig(60, Some("embedded")).await;
    *r.model.answer.lock().unwrap() = json!({
        "questions_answers": [{"question": "Why us?", "answer": "The mission.", "at": [3.0]}],
        "evidence": [{"competency": "Ownership", "quote": "word4 word5", "note": "", "at": [4.0]}],
        "candidate_questions": [], "follow_ups": []
    });
    let job = generate(
        &r,
        json!({"template": "interview",
               "context": {"role": "Engineer", "competencies": ["Ownership"]}}),
    )
    .await;
    assert_eq!(job["state"], "done", "{job}");
    let got = user(
        &r,
        "get_report",
        json!({"report_id": job["result"]["report_id"]}),
    )
    .await
    .unwrap();
    assert!(cm_ai::report::Template::forbidden_judging_fields(&got["report"]).is_empty());
    assert!(got["markdown"]
        .as_str()
        .unwrap()
        .contains("Notes only \u{2014} CaptureMann doesn't score candidates."));
    assert_eq!(
        got["report"]["content"]["role_candidate"][0]["role"],
        "Engineer"
    );
}

/// AI-9.AC6: edit then undo returns the previous text; a failed regenerate keeps the old report;
/// delete is undoable.
#[tokio::test]
async fn edit_undo_failed_regenerate_and_delete() {
    let r = rig(60, Some("embedded")).await;
    let job = generate(&r, json!({"template": "minutes"})).await;
    let rid = job["result"]["report_id"].as_str().unwrap().to_owned();
    let original = user(&r, "get_report", json!({"report_id": rid}))
        .await
        .unwrap();

    let e = user(
        &r,
        "edit_report",
        json!({"report_id": rid, "markdown": "# Mine\n\nMy own words.\n"}),
    )
    .await
    .unwrap();
    assert_eq!(e["action"], "edit_report");
    let edited = user(&r, "get_report", json!({"report_id": rid}))
        .await
        .unwrap();
    assert_eq!(edited["report"]["edited"], true);
    assert_eq!(edited["markdown"], "# Mine\n\nMy own words.\n");
    assert_eq!(
        std::fs::read_to_string(r.dir.join("reports").join(format!("{rid}.md"))).unwrap(),
        "# Mine\n\nMy own words.\n"
    );

    user(&r, "undo", json!({})).await.unwrap();
    let back = user(&r, "get_report", json!({"report_id": rid}))
        .await
        .unwrap();
    assert_eq!(back["report"]["edited"], false);
    assert_eq!(back["markdown"], original["markdown"]);
    assert_eq!(
        std::fs::read_to_string(r.dir.join("reports").join(format!("{rid}.md"))).unwrap(),
        original["markdown"].as_str().unwrap()
    );
    user(&r, "redo", json!({})).await.unwrap();
    assert_eq!(
        user(&r, "get_report", json!({"report_id": rid}))
            .await
            .unwrap()["markdown"],
        "# Mine\n\nMy own words.\n"
    );
    user(&r, "undo", json!({})).await.unwrap();

    // A regenerate that fails leaves the report as it was.
    let mut rx = r.daemon.subscribe();
    *r.model.fail.lock().unwrap() = true;
    let failed = generate(&r, json!({"template": "minutes", "report_id": rid})).await;
    assert_eq!(failed["state"], "failed", "{failed}");
    let after = user(&r, "get_report", json!({"report_id": rid}))
        .await
        .unwrap();
    assert_eq!(after["markdown"], original["markdown"]);
    assert_eq!(after["report"]["v"], original["report"]["v"]);
    let ev = drain(&mut rx);
    let f = ev.iter().find(|e| e.name == "report.failed").unwrap();
    assert_eq!(f.data["reason"], "text_model_unreachable");

    // A regenerate that works replaces it in place (same id), one revision.
    *r.model.fail.lock().unwrap() = false;
    let ok = generate(&r, json!({"template": "minutes", "report_id": rid})).await;
    assert_eq!(ok["state"], "done", "{ok}");
    assert_eq!(ok["result"]["report_id"], rid);
    let list = user(&r, "get_report", json!({})).await.unwrap();
    assert_eq!(list["reports"].as_array().unwrap().len(), 1);

    // Several reports per capture; delete one and undo the delete.
    *r.model.answer.lock().unwrap() =
        json!({"summary": [{"text": "Short.", "at": [2.0]}], "key_points": []});
    let second = generate(&r, json!({"template": "summary"})).await;
    let rid2 = second["result"]["report_id"].as_str().unwrap().to_owned();
    assert_ne!(rid, rid2);
    assert_eq!(
        user(&r, "get_report", json!({})).await.unwrap()["reports"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    user(&r, "delete_report", json!({"report_id": rid2}))
        .await
        .unwrap();
    assert!(!r.dir.join("reports").join(format!("{rid2}.json")).exists());
    let e = user(&r, "get_report", json!({"report_id": rid2}))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::NotFound);
    user(&r, "undo", json!({})).await.unwrap();
    assert!(user(&r, "get_report", json!({"report_id": rid2}))
        .await
        .is_ok());
    assert!(r.dir.join("reports").join(format!("{rid2}.json")).is_file());
}

/// AI-9: a ticked task is stored in the report as one revision; undo and redo restore it; only
/// task sections and existing items can be ticked; a regenerate starts with none ticked.
#[tokio::test]
async fn check_report_item_is_stored_and_undoable() {
    let r = rig(60, Some("embedded")).await;
    let job = generate(&r, json!({"template": "minutes"})).await;
    let rid = job["result"]["report_id"].as_str().unwrap().to_owned();
    let n = user(&r, "get_report", json!({"report_id": rid}))
        .await
        .unwrap()["report"]["content"]["action_items"]
        .as_array()
        .unwrap()
        .len();
    assert!(n >= 1);

    let c = user(
        &r,
        "check_report_item",
        json!({"report_id": rid, "section": "action_items", "index": 0}),
    )
    .await
    .unwrap();
    assert_eq!(c["action"], "check_report_item");
    assert_eq!(c["changed"], true);
    let got = user(&r, "get_report", json!({"report_id": rid}))
        .await
        .unwrap();
    assert_eq!(got["report"]["checked"], json!(["action_items:0"]));
    // The live file other tools read has it too.
    let live: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(r.dir.join("reports").join(format!("{rid}.json"))).unwrap(),
    )
    .unwrap();
    assert_eq!(live["checked"], json!(["action_items:0"]));
    assert_eq!(got["report"]["edited"], false);

    // The same tick again is not a revision.
    let again = user(
        &r,
        "check_report_item",
        json!({"report_id": rid, "section": "action_items", "index": 0, "checked": true}),
    )
    .await
    .unwrap();
    assert_eq!(again["changed"], false);

    // Undo, redo, untick.
    user(&r, "undo", json!({})).await.unwrap();
    let back = user(&r, "get_report", json!({"report_id": rid}))
        .await
        .unwrap();
    assert!(back["report"].get("checked").is_none());
    user(&r, "redo", json!({})).await.unwrap();
    assert_eq!(
        user(&r, "get_report", json!({"report_id": rid}))
            .await
            .unwrap()["report"]["checked"],
        json!(["action_items:0"])
    );
    user(
        &r,
        "check_report_item",
        json!({"report_id": rid, "section": "action_items", "index": 0, "checked": false}),
    )
    .await
    .unwrap();
    assert!(user(&r, "get_report", json!({"report_id": rid}))
        .await
        .unwrap()["report"]
        .get("checked")
        .is_none());

    // Refusals: a section that has no tasks, an item that is not there, an unknown report.
    for (params, want) in [
        (
            json!({"report_id": rid, "section": "decisions", "index": 0}),
            ErrorCode::InvalidInput,
        ),
        (
            json!({"report_id": rid, "section": "action_items", "index": n}),
            ErrorCode::InvalidInput,
        ),
        (
            json!({"report_id": "rpt_00000000", "section": "action_items", "index": 0}),
            ErrorCode::NotFound,
        ),
    ] {
        let e = user(&r, "check_report_item", params).await.unwrap_err();
        assert_eq!(code(&e), want);
    }

    // A regenerate starts with nothing ticked.
    user(
        &r,
        "check_report_item",
        json!({"report_id": rid, "section": "action_items", "index": 0}),
    )
    .await
    .unwrap();
    let ok = generate(&r, json!({"template": "minutes", "report_id": rid})).await;
    assert_eq!(ok["state"], "done", "{ok}");
    assert!(user(&r, "get_report", json!({"report_id": rid}))
        .await
        .unwrap()["report"]
        .get("checked")
        .is_none());
}
