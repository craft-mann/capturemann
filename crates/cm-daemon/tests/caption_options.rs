//! `set_caption_options` through the daemon (AI-1): the show and burn-in switches are Free, one
//! revision and one undo step, and leave the caption lines and the edit list alone.

#![allow(clippy::unwrap_used)]

mod common;

use cm_core::{Actor, ErrorCode, MediaTime, TrackInfo, TrackKind, TrackStats};
use cm_daemon::{Daemon, DaemonError};
use cm_library::rec::{RecBundle, SessionInfo};
use common::{builder, desktop};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};
use time::macros::datetime;

fn agent(n: &str) -> Actor {
    Actor::agent(n).unwrap()
}

fn code(e: &DaemonError) -> ErrorCode {
    e.code().unwrap()
}

struct Rig {
    daemon: Daemon,
    id: String,
    dir: PathBuf,
    _tmp: tempfile::TempDir,
}

async fn rig() -> Rig {
    let (b, tmp) = builder(desktop());
    let daemon = b.build();
    let now = datetime!(2026-10-22 10:00:00 UTC);
    let session = SessionInfo {
        clock_origin_ns: 1,
        timebase: "uptime_raw_ns".into(),
        displays: vec![],
        devices: json!({}),
        app_version: "0.0.1".into(),
        extra: Map::new(),
    };
    let rec = RecBundle::create(tmp.path(), "Lesson", now, json!({}), &session).unwrap();
    std::fs::write(rec.dir().join("screen.mp4"), b"master bytes").unwrap();
    let dir = rec.dir().to_path_buf();
    let info = TrackInfo {
        kind: TrackKind::Screen,
        file: "screen.mp4".into(),
        start: MediaTime::ZERO,
        end: MediaTime::from_secs(60),
        details: json!({}),
        stats: TrackStats::default(),
    };
    let m = rec.finalize(&[info], now).unwrap();
    daemon
        .execute_method(Actor::User, 0, "rescan_library", &json!({}))
        .await
        .unwrap();
    Rig {
        daemon,
        id: m.id,
        dir,
        _tmp: tmp,
    }
}

async fn call(r: &Rig, who: Actor, method: &str, mut p: Value) -> Result<Value, DaemonError> {
    p["capture_id"] = json!(r.id);
    r.daemon.execute_method(who, 0, method, &p).await
}

fn manifest(dir: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(dir.join("manifest.json")).unwrap()).unwrap()
}

/// AI-1: each switch is one revision tagged with the caller; undo and redo restore it; the
/// caption lines and the edit list are untouched; a repeat changes nothing and records nothing.
#[tokio::test]
async fn ai1_caption_options_are_one_undoable_revision_each() {
    let r = rig().await;
    let master = std::fs::read(r.dir.join("screen.mp4")).unwrap();
    let lines = json!([{"start": 120000, "end": 480000, "text": "Hello there"}]);
    call(
        &r,
        Actor::User,
        "set_captions",
        json!({"lines": [{"start": 1.0, "end": 4.0, "text": "Hello there"}]}),
    )
    .await
    .unwrap();
    let edits_before = manifest(&r.dir)["edits"].clone();

    let a = call(
        &r,
        agent("claude"),
        "set_caption_options",
        json!({"enabled": true}),
    )
    .await
    .unwrap();
    assert_eq!(a["action"], "set_caption_options");
    assert_eq!(a["by"], "agent:claude");
    assert_eq!(
        (a["enabled"].clone(), a["burn_in"].clone()),
        (json!(true), json!(false))
    );
    assert_eq!(a["changed"], true);
    let m = manifest(&r.dir);
    assert_eq!(m["captions"]["enabled"], true);
    assert_eq!(m["captions"]["burn_in"], false);
    assert_eq!(
        m["captions"]["lines"].as_array().unwrap().len(),
        1,
        "lines kept"
    );
    assert_eq!(m["edits"], edits_before, "the edit list is unchanged");
    let _ = lines;

    // The same value again records nothing.
    let same = call(
        &r,
        Actor::User,
        "set_caption_options",
        json!({"enabled": true}),
    )
    .await
    .unwrap();
    assert_eq!(same["changed"], false);
    assert!(same.get("revision").is_none());

    let b = call(
        &r,
        Actor::User,
        "set_caption_options",
        json!({"burn_in": true}),
    )
    .await
    .unwrap();
    assert_eq!(b["by"], "user");
    assert_eq!(
        (b["enabled"].clone(), b["burn_in"].clone()),
        (json!(true), json!(true))
    );

    // Undo takes back the burn-in only, then the first switch; redo brings both back.
    let u = call(&r, Actor::User, "undo", json!({})).await.unwrap();
    assert_eq!(u["action"], "undo");
    let m = manifest(&r.dir);
    assert_eq!(
        (
            m["captions"]["enabled"].clone(),
            m["captions"]["burn_in"].clone()
        ),
        (json!(true), json!(false))
    );
    call(&r, Actor::User, "undo", json!({})).await.unwrap();
    assert_eq!(manifest(&r.dir)["captions"]["enabled"], false);
    call(&r, Actor::User, "redo", json!({})).await.unwrap();
    call(&r, Actor::User, "redo", json!({})).await.unwrap();
    let m = manifest(&r.dir);
    assert_eq!(
        (
            m["captions"]["enabled"].clone(),
            m["captions"]["burn_in"].clone()
        ),
        (json!(true), json!(true))
    );
    assert_eq!(m["captions"]["lines"].as_array().unwrap().len(), 1);
    assert_eq!(
        std::fs::read(r.dir.join("screen.mp4")).unwrap(),
        master,
        "master untouched"
    );
}

#[tokio::test]
async fn ai1_caption_options_validate_their_input() {
    let r = rig().await;
    for bad in [json!({}), json!({"enabled": "yes"}), json!({"burn": true})] {
        let e = call(&r, Actor::User, "set_caption_options", bad.clone())
            .await
            .unwrap_err();
        assert_eq!(code(&e), ErrorCode::InvalidInput, "{bad}");
    }
    let e = r
        .daemon
        .execute_method(
            Actor::User,
            0,
            "set_caption_options",
            &json!({"capture_id": "rec_nope", "enabled": true}),
        )
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::NotFound);
}

/// A retry with the same `request_id` replays the first answer and changes nothing twice.
#[tokio::test]
async fn ai1_caption_options_request_id_replays() {
    let r = rig().await;
    let p = json!({"enabled": true, "request_id": "co-1"});
    let first = call(&r, agent("claude"), "set_caption_options", p.clone())
        .await
        .unwrap();
    let again = call(&r, agent("claude"), "set_caption_options", p)
        .await
        .unwrap();
    assert_eq!(first, again);
    assert_eq!(manifest(&r.dir)["revisions"].as_array().unwrap().len(), 2);
}
