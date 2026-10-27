//! The built-in text engine (AI-7, provider `embedded`) through the daemon, with a scripted
//! engine and store ([`FakeTextEngine`], [`FakeTextModels`]) and a scripted OpenAI-compatible
//! server on loopback. No helper process, no model, no network beyond 127.0.0.1.

#![allow(clippy::unwrap_used)]

mod common;

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::time::Duration;

use cm_ai::text_model::ErrorKind;
use cm_core::{Actor, ErrorCode};
use cm_daemon::testing::{FakeAi, FakeTextEngine, FakeTextModels, FixedConfirmer};
use cm_daemon::{AiServices, Daemon, DaemonError, Decision};
use common::{builder, desktop};
use serde_json::{json, Value};

struct Rig {
    daemon: Daemon,
    models: FakeTextModels,
    engine: FakeTextEngine,
    confirmer: Arc<FixedConfirmer>,
    _tmp: tempfile::TempDir,
}

/// A server that answers every request with `{"ok": true}` as a chat reply.
fn serve_ok() -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for s in l.incoming() {
            let Ok(mut s) = s else { break };
            std::thread::spawn(move || {
                let mut buf = Vec::new();
                let mut chunk = [0u8; 4096];
                loop {
                    let Ok(k) = s.read(&mut chunk) else { return };
                    if k == 0 {
                        return;
                    }
                    buf.extend_from_slice(&chunk[..k]);
                    let text = String::from_utf8_lossy(&buf).to_string();
                    if let Some(h) = text.find("\r\n\r\n") {
                        let len: usize = text[..h]
                            .to_ascii_lowercase()
                            .lines()
                            .find_map(|l| {
                                l.strip_prefix("content-length:")
                                    .map(|v| v.trim().parse().unwrap())
                            })
                            .unwrap_or(0);
                        if buf.len() >= h + 4 + len {
                            break;
                        }
                    }
                }
                let body = json!({ "choices": [{ "message": { "content": "{\"ok\": true}" } }] })
                    .to_string();
                let _ = write!(
                    s,
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
            });
        }
    });
    port
}

async fn rig(models: FakeTextModels, decision: Decision) -> Rig {
    let port = serve_ok();
    let engine = FakeTextEngine::new(format!("http://127.0.0.1:{port}/v1"));
    let (b, tmp) = builder(desktop());
    let queue = cm_daemon::library_job_queue(tmp.path()).unwrap();
    let confirmer = Arc::new(FixedConfirmer::new(decision));
    let services = AiServices {
        text_models: Arc::new(models.clone()),
        text_engine: Arc::new(engine.clone()),
        ..FakeAi::new().services()
    };
    let daemon = b
        .jobs(queue)
        .confirmer(confirmer.clone())
        .ai(services)
        .build();
    daemon.register_ai_handlers().unwrap();
    daemon.start_jobs().unwrap();
    Rig {
        daemon,
        models,
        engine,
        confirmer,
        _tmp: tmp,
    }
}

async fn user(r: &Rig, method: &str, p: Value) -> Result<Value, DaemonError> {
    r.daemon.execute_method(Actor::User, 0, method, &p).await
}

async fn agent(r: &Rig, method: &str, p: Value) -> Result<Value, DaemonError> {
    r.daemon
        .execute_method(Actor::agent("claude").unwrap(), 0, method, &p)
        .await
}

async fn use_embedded(r: &Rig) {
    user(
        r,
        "set_settings",
        json!({ "values": { "ai.text.provider": "embedded" } }),
    )
    .await
    .unwrap();
}

fn code(e: &DaemonError) -> ErrorCode {
    match e {
        DaemonError::Cm(c) => c.code(),
        DaemonError::WithData { error, .. } => error.code(),
        DaemonError::Declined { .. } => ErrorCode::UserDeclined,
        other => panic!("unexpected {other:?}"),
    }
}

fn reason(e: &DaemonError) -> String {
    match e {
        DaemonError::WithData { data, .. } => data["reason"].as_str().unwrap_or("").to_owned(),
        DaemonError::Declined { reason, .. } => (*reason).to_owned(),
        _ => String::new(),
    }
}

async fn job_done(r: &Rig, id: &str) -> Value {
    for _ in 0..500 {
        let j = user(r, "get_job", json!({ "job_id": id })).await.unwrap();
        if matches!(j["state"].as_str(), Some("done" | "failed" | "cancelled")) {
            return j;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("timed out");
}

#[tokio::test]
async fn embedded_with_a_downloaded_model_runs_through_the_engine() {
    let r = rig(
        FakeTextModels::new().with_installed(&["text-16gb"]),
        Decision::Approve,
    )
    .await;
    use_embedded(&r).await;
    let g = user(&r, "get_text_model", json!({})).await.unwrap();
    assert_eq!(g["provider"], "embedded");
    assert_eq!(g["local"], true);
    assert_eq!(g["model"], "text-16gb", "the store's active model");
    assert_eq!(g["host"], "127.0.0.1");
    assert_eq!(g["engine"]["state"], "stopped", "nothing started by itself");
    assert!(r.engine.specs().is_empty());
    // Local: a caller needs no Confirm plan.
    assert!(r.daemon.text_model_gate(5000).unwrap().is_none());

    let t = user(&r, "test_text_model", json!({})).await.unwrap();
    assert_eq!(t["ok"], true, "{t}");
    assert_eq!(t["model"], "text-16gb");
    let specs = r.engine.specs();
    assert_eq!(specs.len(), 1);
    assert_eq!(specs[0].model_id, "text-16gb");
    assert_eq!(specs[0].ram_mb, 8000);
    assert_eq!(specs[0].context_tokens, 8192);
    let g = user(&r, "get_text_model", json!({})).await.unwrap();
    assert_eq!(g["engine"]["state"], "running");
    assert_eq!(g["structured_output"], true);

    // A named built-in model wins over the active one.
    let r2 = rig(
        FakeTextModels::new().with_installed(&["text-8gb", "text-32gb"]),
        Decision::Approve,
    )
    .await;
    use_embedded(&r2).await;
    user(
        &r2,
        "set_settings",
        json!({ "values": { "ai.text.embedded_model": "text-32gb" } }),
    )
    .await
    .unwrap();
    let t = user(&r2, "test_text_model", json!({})).await.unwrap();
    assert_eq!(t["model"], "text-32gb");
}

#[tokio::test]
async fn embedded_without_a_downloaded_model_is_no_text_model() {
    let r = rig(FakeTextModels::new(), Decision::Approve).await;
    use_embedded(&r).await;
    let e = user(&r, "test_text_model", json!({})).await.unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
    assert_eq!(reason(&e), "no_text_model");
    assert_eq!(
        reason(&r.daemon.text_model_gate(10).unwrap_err()),
        "no_text_model"
    );
    assert!(r.engine.specs().is_empty(), "the engine is never started");
}

#[tokio::test]
async fn a_missing_helper_and_a_full_memory_are_answers_with_their_reasons() {
    let r = rig(
        FakeTextModels::new().with_installed(&["text-8gb"]),
        Decision::Approve,
    )
    .await;
    use_embedded(&r).await;
    r.engine.fail_with(Some(ErrorKind::EngineMissing));
    let t = user(&r, "test_text_model", json!({})).await.unwrap();
    assert_eq!(t["ok"], false);
    assert_eq!(t["reason"], "engine_missing");
    r.engine.fail_with(Some(ErrorKind::NotEnoughMemory));
    let t = user(&r, "test_text_model", json!({})).await.unwrap();
    assert_eq!(t["reason"], "not_enough_memory");
    r.engine.fail_with(None);
    assert_eq!(
        user(&r, "test_text_model", json!({})).await.unwrap()["ok"],
        true
    );
}

#[tokio::test]
async fn the_embedded_provider_can_only_be_chosen_by_the_app_not_an_agent() {
    let r = rig(FakeTextModels::new(), Decision::Approve).await;
    for key in ["ai.text.provider", "ai.text.embedded_model"] {
        let e = agent(&r, "set_settings", json!({ "values": { key: "embedded" } }))
            .await
            .unwrap_err();
        assert_eq!(reason(&e), "app_only_setting", "{key}");
    }
    // An unknown value never turns anything on.
    user(
        &r,
        "set_settings",
        json!({ "values": { "ai.text.provider": "embedded-ish" } }),
    )
    .await
    .unwrap();
    assert_eq!(
        user(&r, "get_text_model", json!({})).await.unwrap()["provider"],
        "none"
    );
}

#[tokio::test]
async fn text_model_actions_list_download_select_and_remove() {
    let r = rig(FakeTextModels::new(), Decision::Approve).await;
    let mut rx = r.daemon.subscribe();
    let l = user(&r, "list_text_models", json!({})).await.unwrap();
    let models = l["models"].as_array().unwrap();
    assert_eq!(models.len(), 3);
    assert_eq!(models[0]["id"], "text-8gb");
    assert_eq!(models[0]["tier_gb"], 8);
    assert_eq!(models[0]["license"], "Apache-2.0");
    assert_eq!(models[0]["installed"], false);

    let s = user(&r, "download_text_model", json!({ "model": "text-8gb" }))
        .await
        .unwrap();
    assert_eq!(
        job_done(&r, s["job_id"].as_str().unwrap()).await["state"],
        "done"
    );
    assert_eq!(r.models.installed(), vec!["text-8gb"]);
    let l = user(&r, "list_text_models", json!({})).await.unwrap();
    assert_eq!(l["active"], "text-8gb");
    let mut seen = Vec::new();
    while let Ok(e) = rx.try_recv() {
        seen.push(e);
    }
    assert!(seen
        .iter()
        .any(|e| e.name == "model.download.progress" && e.data["kind"] == "text"));
    assert!(seen.iter().any(|e| e.name == "model.changed"
        && e.data["change"] == "downloaded"
        && e.data["kind"] == "text"));

    // Already installed, unknown id, not installed.
    let e = user(&r, "download_text_model", json!({ "model": "text-8gb" }))
        .await
        .unwrap_err();
    assert!(e.to_string().contains("already installed"), "{e}");
    let e = user(&r, "download_text_model", json!({ "model": "nope" }))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
    let e = user(&r, "select_text_model", json!({ "model": "text-32gb" }))
        .await
        .unwrap_err();
    assert!(e.to_string().contains("not installed"), "{e}");

    user(&r, "select_text_model", json!({ "model": "text-8gb" }))
        .await
        .unwrap();
    let removed = user(&r, "remove_text_model", json!({ "model": "text-8gb" }))
        .await
        .unwrap();
    assert_eq!(removed["removed"], true);
    assert!(r.models.installed().is_empty());
}

#[tokio::test]
async fn agents_are_asked_before_a_download_or_a_removal_and_see_the_plan() {
    let r = rig(FakeTextModels::new(), Decision::Decline).await;
    let e = agent(&r, "download_text_model", json!({ "model": "text-16gb" }))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::UserDeclined);
    assert!(r.models.downloads_started().is_empty());
    let shown = r.confirmer.shown();
    assert_eq!(shown[0].tool, "download_text_model");
    assert!(
        shown[0].summary.contains("text model"),
        "{}",
        shown[0].summary
    );
    assert!(shown[0].summary.contains("5000 MB"), "{}", shown[0].summary);
    assert!(shown[0].summary.contains("internet"));
    assert_eq!(shown[0].details["license"], "Apache-2.0");
    assert_eq!(shown[0].details["destination"], "/text-models");

    let ok = rig(
        FakeTextModels::new().with_installed(&["text-8gb"]),
        Decision::Approve,
    )
    .await;
    agent(&ok, "remove_text_model", json!({ "model": "text-8gb" }))
        .await
        .unwrap();
    assert!(ok.models.installed().is_empty());
    // Reading and choosing need no dialog.
    agent(&ok, "list_text_models", json!({})).await.unwrap();
    assert_eq!(ok.confirmer.shown().len(), 1);
}

#[tokio::test]
async fn removing_the_model_a_job_is_using_is_busy_and_an_idle_one_is_unloaded() {
    let r = rig(
        FakeTextModels::new().with_installed(&["text-8gb"]),
        Decision::Approve,
    )
    .await;
    use_embedded(&r).await;
    user(&r, "test_text_model", json!({})).await.unwrap();
    r.engine.set_in_flight(1);
    let e = user(&r, "remove_text_model", json!({ "model": "text-8gb" }))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::Busy);
    assert_eq!(r.models.installed(), vec!["text-8gb"]);
    r.engine.set_in_flight(0);
    user(&r, "remove_text_model", json!({ "model": "text-8gb" }))
        .await
        .unwrap();
    assert_eq!(r.engine.stops(), 1, "the engine let go of the file first");
}
