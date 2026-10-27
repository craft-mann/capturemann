//! `get_text_model`, `test_text_model` and the caller gate (AI-7) through the daemon, against a
//! scripted OpenAI-compatible server on loopback. No external network, no model.

#![allow(clippy::unwrap_used)]

mod common;

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;

use cm_auth::{MemoryStore, SecretStore};
use cm_core::Actor;
use cm_daemon::{Daemon, DaemonError};
use common::{builder, desktop};
use serde_json::{json, Value};

fn rig() -> (Daemon, tempfile::TempDir) {
    let store = Arc::new(MemoryStore::default());
    store.set("anthropic-key", "sk-ant-TOPSECRET").unwrap();
    let (b, dir) = builder(desktop());
    (b.text_model_secrets(store).build(), dir)
}

async fn user(d: &Daemon, method: &str, p: Value) -> Result<Value, DaemonError> {
    d.execute_method(Actor::User, 0, method, &p).await
}

async fn settings(d: &Daemon, values: Value) {
    user(d, "set_settings", json!({ "values": values }))
        .await
        .unwrap();
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

fn reason(e: &DaemonError) -> String {
    match e {
        DaemonError::WithData { data, .. } => data["reason"].as_str().unwrap_or("").to_owned(),
        DaemonError::Declined { reason, .. } => (*reason).to_owned(),
        _ => String::new(),
    }
}

#[tokio::test]
async fn provider_none_by_default_and_no_text_model_for_callers() {
    let (d, _dir) = rig();
    let g = user(&d, "get_text_model", json!({})).await.unwrap();
    assert_eq!(g["provider"], "none");
    assert_eq!(g["local"], true);
    assert_eq!(g["context_tokens"], 8192);
    assert_eq!(g["timeout_s"], 300);
    assert_eq!(g["structured_output"], Value::Null);
    let e = user(&d, "test_text_model", json!({})).await.unwrap_err();
    assert_eq!(reason(&e), "no_text_model");
    assert!(
        matches!(&e, DaemonError::WithData { error, .. } if error.code().as_str() == "invalid_input")
    );
    // AI-7.AC2: a caller is told the same.
    assert_eq!(
        reason(&d.text_model_gate(100).unwrap_err()),
        "no_text_model"
    );
}

#[tokio::test]
async fn settings_are_app_only() {
    let (d, _dir) = rig();
    settings(
        &d,
        json!({ "ai.text.provider": "local", "ai.text.context_tokens": 16384 }),
    )
    .await;
    let g = user(&d, "get_text_model", json!({})).await.unwrap();
    assert_eq!(g["provider"], "local");
    assert_eq!(g["context_tokens"], 16384);
    let agent = Actor::agent("claude").unwrap();
    for key in [
        "ai.text.provider",
        "ai.text.base_url",
        "ai.text.model",
        "ai.text.key_name",
        "ai.text.context_tokens",
        "ai.text.timeout_s",
    ] {
        let e = d
            .execute_method(
                agent.clone(),
                0,
                "set_settings",
                &json!({ "values": { key: "x" } }),
            )
            .await
            .unwrap_err();
        assert_eq!(reason(&e), "app_only_setting", "{key}");
    }
}

#[tokio::test]
async fn test_button_against_the_scripted_local_server() {
    let (d, _dir) = rig();
    let port = serve_ok();
    settings(
        &d,
        json!({ "ai.text.provider": "local", "ai.text.base_url": format!("http://127.0.0.1:{port}/v1"),
                "ai.text.model": "qwen3:8b", "ai.text.timeout_s": 10 }),
    )
    .await;
    let t = user(&d, "test_text_model", json!({})).await.unwrap();
    assert_eq!(t["ok"], true, "{t}");
    assert_eq!(t["model"], "qwen3:8b");
    assert_eq!(t["structured_output"], true);
    assert!(t["latency_ms"].is_u64());
    let g = user(&d, "get_text_model", json!({})).await.unwrap();
    assert_eq!(g["structured_output"], true);
    assert_eq!(g["local"], true);
    // Free for an agent, and no gate: a local provider keeps text on this machine.
    let a = d
        .execute_method(
            Actor::agent("claude").unwrap(),
            0,
            "test_text_model",
            &json!({}),
        )
        .await
        .unwrap();
    assert_eq!(a["ok"], true);
    assert!(d.text_model_gate(5000).unwrap().is_none());
    assert!(d.text_model_client().is_ok());
}

#[tokio::test]
async fn an_unreachable_server_is_an_answer_not_an_error() {
    let (d, _dir) = rig();
    let port = {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    settings(
        &d,
        json!({ "ai.text.provider": "local", "ai.text.base_url": format!("http://127.0.0.1:{port}/v1"),
                "ai.text.model": "m" }),
    )
    .await;
    let t = user(&d, "test_text_model", json!({})).await.unwrap();
    assert_eq!(t["ok"], false);
    assert_eq!(t["reason"], "text_model_unreachable");
    assert!(t["message"]
        .as_str()
        .unwrap()
        .contains("Start Ollama or LM Studio"));
}

/// Local text models only for now (maintainer, Oct 26, 2026): a remote provider in the
/// settings file is treated as `none`, so nothing is ever sent to it.
#[tokio::test]
async fn a_remote_provider_in_the_settings_is_unavailable() {
    let (d, _dir) = rig();
    for provider in ["anthropic", "openai_compatible"] {
        settings(
            &d,
            json!({ "ai.text.provider": provider, "ai.text.model": "claude-x",
                    "ai.text.base_url": "https://llm.example.com/v1",
                    "ai.text.key_name": "k" }),
        )
        .await;
        let g = user(&d, "get_text_model", json!({})).await.unwrap();
        assert_eq!(g["provider"], "none", "{g}");
        assert!(d.text_model_gate(10).is_err());
        assert!(user(&d, "test_text_model", json!({})).await.is_err());
    }
}

#[tokio::test]
async fn a_local_provider_pointed_at_the_lan_is_remote_and_never_called() {
    let (d, _dir) = rig();
    settings(
        &d,
        json!({ "ai.text.provider": "local", "ai.text.base_url": "http://192.168.1.50:11434/v1",
                "ai.text.model": "m" }),
    )
    .await;
    let g = user(&d, "get_text_model", json!({})).await.unwrap();
    assert_eq!(g["local"], false);
    let e = user(&d, "test_text_model", json!({})).await.unwrap_err();
    assert!(matches!(e, DaemonError::WithData { .. }));
    assert!(d.text_model_gate(10).is_err());
}
