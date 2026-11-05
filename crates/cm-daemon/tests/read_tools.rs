//! `get_cursor_track` (REC-7) and `get_platform_rules` (EXP-2) through the daemon: read-only,
//! Free for agents, strict parameters, shared error codes. No hardware: the recording is a bundle
//! written by hand with a `cursor.jsonl`.

#![allow(clippy::unwrap_used)]

mod common;

use cm_core::{Actor, ErrorCode, MediaTime, TrackInfo, TrackKind, TrackStats};
use cm_daemon::{Daemon, DaemonError};
use common::{builder, desktop};
use serde_json::{json, Map, Value};
use time::macros::datetime;

fn code(e: &DaemonError) -> ErrorCode {
    match e {
        DaemonError::Cm(c) => c.code(),
        DaemonError::WithData { error, .. } => error.code(),
        DaemonError::NotImplemented(_) => ErrorCode::NotImplemented,
        other => panic!("unexpected {other:?}"),
    }
}

fn cursor_text() -> String {
    let target =
        cm_cursor::CursorTarget::display(1, cm_core::Rect::new(0.0, 0.0, 100.0, 50.0), 2.0);
    let header = json!({
        "type": "session", "schema": 1, "tick_rate": 120000, "clock": "test", "platform": "test",
        "target": target, "clicks": "available", "scroll": "available",
        "move_source": "fake", "permission_missing": null,
    });
    let mut s = format!("{header}\n");
    for i in 0..6i64 {
        s.push_str(&format!(
            "{}\n",
            json!({"t": i * 120_000, "type": "move", "x": i * 10, "y": 5})
        ));
    }
    s.push_str(&format!(
        "{}\n",
        json!({"t": 600_000, "type": "down", "button": "left", "x": 50, "y": 5})
    ));
    s
}

fn recording(root: &std::path::Path, with_cursor: bool) -> String {
    use cm_library::rec::{RecBundle, SessionInfo};
    let now = datetime!(2026-11-05 10:00:00 UTC);
    let session = SessionInfo {
        clock_origin_ns: 1,
        timebase: "uptime_raw_ns".into(),
        displays: vec![],
        devices: json!({}),
        app_version: "0.0.1".into(),
        extra: Map::new(),
    };
    let rec = RecBundle::create(root, "Cursor", now, json!({}), &session).unwrap();
    std::fs::write(rec.dir().join("screen.mp4"), b"master").unwrap();
    let mut tracks = vec![TrackInfo {
        kind: TrackKind::Screen,
        file: "screen.mp4".into(),
        start: MediaTime::ZERO,
        end: MediaTime::from_secs(10),
        details: json!({}),
        stats: TrackStats::default(),
    }];
    if with_cursor {
        std::fs::write(rec.dir().join("cursor.jsonl"), cursor_text()).unwrap();
        tracks.push(TrackInfo {
            kind: TrackKind::Cursor,
            file: "cursor.jsonl".into(),
            start: MediaTime::ZERO,
            end: MediaTime::from_secs(10),
            details: json!({}),
            stats: TrackStats::default(),
        });
    }
    rec.finalize(&tracks, now).unwrap().id
}

async fn rig(with_cursor: bool) -> (Daemon, String, tempfile::TempDir) {
    let (b, tmp) = builder(desktop());
    let daemon = b.build();
    let id = recording(tmp.path(), with_cursor);
    daemon
        .execute_method(Actor::User, 0, "rescan_library", &json!({}))
        .await
        .unwrap();
    (daemon, id, tmp)
}

fn agent() -> Actor {
    Actor::agent("claude").unwrap()
}

#[tokio::test]
async fn cursor_track_returns_the_events_in_a_range_for_an_agent() {
    let (d, id, _tmp) = rig(true).await;
    let v = d
        .execute_method(agent(), 0, "get_cursor_track", &json!({"capture_id": id}))
        .await
        .unwrap();
    assert_eq!(v["total_in_range"], 7);
    assert_eq!(v["events"][6]["type"], "down");
    assert_eq!(v["clicks"], "available");
    assert!(v["next_from"].is_null());

    // Seconds, half-open: [1 s, 3 s) holds the moves at 1 s and 2 s only.
    let v = d
        .execute_method(
            agent(),
            0,
            "get_cursor_track",
            &json!({"capture_id": id, "from": 1, "to": 3}),
        )
        .await
        .unwrap();
    assert_eq!(v["count"], 2);
    assert_eq!(v["events"][0]["x"], 10);

    // A page of 3 says where the next one starts.
    let v = d
        .execute_method(
            agent(),
            0,
            "get_cursor_track",
            &json!({"capture_id": id, "limit": 3}),
        )
        .await
        .unwrap();
    assert_eq!(v["count"], 3);
    assert_eq!(v["next_from"], 360_000);
}

#[tokio::test]
async fn cursor_track_errors_use_the_shared_codes() {
    let (d, id, _tmp) = rig(false).await;
    let e = d
        .execute_method(agent(), 0, "get_cursor_track", &json!({"capture_id": id}))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::NotFound);
    let e = d
        .execute_method(
            agent(),
            0,
            "get_cursor_track",
            &json!({"capture_id": "nope"}),
        )
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::NotFound);
    let e = d
        .execute_method(agent(), 0, "get_cursor_track", &json!({}))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
}

#[tokio::test]
async fn platform_rules_for_youtube_and_the_other_cases() {
    let (d, _id, _tmp) = rig(false).await;
    let v: Value = d
        .execute_method(
            agent(),
            0,
            "get_platform_rules",
            &json!({"platform": "YouTube"}),
        )
        .await
        .unwrap();
    assert_eq!(v["platform"], "youtube");
    assert_eq!(v["limits"]["title_max_chars"], 100);
    assert_eq!(v["limits"]["uploads_per_day"], 100);
    assert_eq!(v["recommended_preset"], "youtube-1080p");
    assert!(v["privacy_options"]
        .as_array()
        .unwrap()
        .contains(&json!("unlisted")));
    for p in ["tiktok", "facebook"] {
        let e = d
            .execute_method(agent(), 0, "get_platform_rules", &json!({"platform": p}))
            .await
            .unwrap_err();
        assert_eq!(code(&e), ErrorCode::NotImplemented);
    }
    let e = d
        .execute_method(
            agent(),
            0,
            "get_platform_rules",
            &json!({"platform": "vimeo"}),
        )
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
}

#[test]
fn both_tools_are_free_read_only_actions() {
    for name in ["get_cursor_track", "get_platform_rules"] {
        let s = cm_daemon::ACTIONS.iter().find(|a| a.name == name).unwrap();
        assert_eq!(s.tier, cm_daemon::Tier::Free, "{name}");
        assert!(!s.mutating, "{name}");
    }
}
