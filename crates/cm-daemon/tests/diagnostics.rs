//! `get_diagnostics` (DIAG-1.AC5, AC6) through the daemon: every section is there, an agent may
//! read it, and what it holds is only what the report is allowed to say.

#![allow(clippy::unwrap_used)]

mod common;

use cm_core::Actor;
use common::{builder, desktop};
use serde_json::{json, Value};

async fn diagnostics(actor: Actor) -> Value {
    let (b, _dir) = builder(desktop());
    let d = b.build();
    d.execute_method(
        Actor::User,
        0,
        "set_settings",
        &json!({"values": {"recording.fps": 30, "ai.languages": "vi,en"}}),
    )
    .await
    .unwrap();
    d.execute_method(actor, 0, "get_diagnostics", &json!({}))
        .await
        .unwrap()
}

fn titles(v: &Value) -> Vec<String> {
    v["sections"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["title"].as_str().unwrap().to_owned())
        // Only a Linux machine with a gap (no portal, no PipeWire) has this section.
        .filter(|t| t != "Missing on this system")
        .collect()
}

#[tokio::test]
async fn the_report_has_every_section_and_the_text_matches_them() {
    let v = diagnostics(Actor::User).await;
    assert_eq!(
        titles(&v),
        [
            "App",
            "System",
            "Permissions",
            "Models and engines",
            "Settings",
            "Library",
            "Last recording",
            "Last runs",
            "Warnings and errors (newest last)",
        ]
    );
    let text = v["text"].as_str().unwrap();
    for t in titles(&v) {
        assert!(text.contains(&format!("## {t}")), "{t}");
    }
    assert!(text.contains("CaptureMann "), "{text}");
    assert!(text.contains("recording.fps = 30"), "{text}");
    assert!(text.contains("ai.languages = \"vi,en\""), "{text}");
    assert!(text.contains("0 captures"), "{text}");
    assert!(text.contains("no recording yet"), "{text}");
}

#[tokio::test]
async fn an_agent_may_read_it_and_gets_the_same_sections() {
    let v = diagnostics(Actor::agent("claude").unwrap()).await;
    assert_eq!(titles(&v).len(), 9);
}

#[tokio::test]
async fn extra_parameters_are_refused() {
    let (b, _dir) = builder(desktop());
    let d = b.build();
    let e = d
        .execute_method(Actor::User, 0, "get_diagnostics", &json!({"x": 1}))
        .await
        .unwrap_err();
    assert!(e.to_string().contains('x'), "{e}");
}
