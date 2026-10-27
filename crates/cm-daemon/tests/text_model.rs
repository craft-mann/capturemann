//! `get_text_model`, `test_text_model` and the caller check (AI-7) through the daemon: provider
//! `none` by default, app-only settings, providers that no longer exist. The built-in engine against a fake helper is in `text_engine.rs`. No network, no model.

#![allow(clippy::unwrap_used)]

mod common;

use cm_core::Actor;
use cm_daemon::{Daemon, DaemonError};
use common::{builder, desktop};
use serde_json::{json, Value};

fn rig() -> (Daemon, tempfile::TempDir) {
    let (b, dir) = builder(desktop());
    (b.build(), dir)
}

async fn user(d: &Daemon, method: &str, p: Value) -> Result<Value, DaemonError> {
    d.execute_method(Actor::User, 0, method, &p).await
}

async fn settings(d: &Daemon, values: Value) {
    user(d, "set_settings", json!({ "values": values }))
        .await
        .unwrap();
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
    assert_eq!(g["context_tokens"], 8192);
    assert_eq!(g["timeout_s"], 300);
    assert_eq!(g["structured_output"], Value::Null);
    let e = user(&d, "test_text_model", json!({})).await.unwrap_err();
    assert_eq!(reason(&e), "no_text_model");
    assert!(
        matches!(&e, DaemonError::WithData { error, .. } if error.code().as_str() == "invalid_input")
    );
    // AI-7.AC2: a caller is told the same.
    assert_eq!(reason(&d.text_model_check().unwrap_err()), "no_text_model");
}

#[tokio::test]
async fn settings_are_app_only() {
    let (d, _dir) = rig();
    settings(
        &d,
        json!({ "ai.text.provider": "embedded", "ai.text.context_tokens": 16384 }),
    )
    .await;
    let g = user(&d, "get_text_model", json!({})).await.unwrap();
    assert_eq!(g["provider"], "embedded");
    assert_eq!(g["context_tokens"], 16384);
    let agent = Actor::agent("claude").unwrap();
    for key in [
        "ai.text.provider",
        "ai.text.base_url",
        "ai.text.model",
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

/// The built-in engine only in 0.2.0 (maintainer, Oct 27, 2026): any other provider in the
/// settings file (`local` included) is treated as `none`, so nothing is ever sent to it.
#[tokio::test]
async fn a_provider_that_no_longer_exists_in_the_settings_reads_as_none() {
    let (d, _dir) = rig();
    for provider in ["anthropic", "openai_compatible", "local"] {
        settings(
            &d,
            json!({ "ai.text.provider": provider, "ai.text.model": "claude-x",
                    "ai.text.base_url": "https://llm.example.com/v1" }),
        )
        .await;
        let g = user(&d, "get_text_model", json!({})).await.unwrap();
        assert_eq!(g["provider"], "none", "{g}");
        assert!(d.text_model_check().is_err());
        assert!(user(&d, "test_text_model", json!({})).await.is_err());
    }
}
