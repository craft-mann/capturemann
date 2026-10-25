//! The EDT-4 and EDT-5 edits through the daemon: `set_zooms`, `set_cursor_style` and
//! `set_reframe`. Each is Free, one revision and one undo step, and changes only
//! `manifest.edits[0]`.

#![allow(clippy::unwrap_used)]

mod common;

use cm_core::{Actor, ErrorCode, MediaTime, TrackInfo, TrackKind, TrackStats};
use cm_daemon::{ClientPolicy, Daemon, DaemonError};
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

/// A finished 60 s recording with a fake screen master.
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

fn zoom(at: f64, len: f64) -> Value {
    json!({"at": at, "length": len, "focus": [0.5, 0.4]})
}

fn zooms_of(v: &Value) -> Vec<(i64, i64)> {
    v["edits"]["zooms"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|z| {
                    (
                        z["at"].as_i64().unwrap() / 120_000,
                        (z["at"].as_i64().unwrap() + z["length"].as_i64().unwrap()) / 120_000,
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

/// EDT-4: zooms are added, shorten their neighbours, can be replaced or cleared, and each call is
/// one undo step tagged with the caller.
#[tokio::test]
async fn edt4_set_zooms_adds_replaces_clears_and_undoes() {
    let r = rig().await;
    let master = std::fs::read(r.dir.join("screen.mp4")).unwrap();
    let a = call(
        &r,
        agent("claude"),
        "set_zooms",
        json!({"zooms": [zoom(15.0, 3.0)]}),
    )
    .await
    .unwrap();
    assert_eq!(a["by"], "agent:claude");
    assert_eq!(a["action"], "set_zooms");
    assert_eq!(zooms_of(&a), vec![(15, 18)]);
    let z = &a["edits"]["zooms"][0];
    assert_eq!(z["scale"], 1.8, "the default scale");
    assert_eq!(z["follow_cursor"], false);
    assert_eq!(z["focus"], json!([0.5, 0.4]));

    // A second region overlapping the first shortens it (EDT-4 edge case).
    let b = call(
        &r,
        Actor::User,
        "set_zooms",
        json!({"zooms": [zoom(17.0, 5.0)]}),
    )
    .await
    .unwrap();
    assert_eq!(zooms_of(&b), vec![(15, 17), (17, 22)]);

    // Several in one call are one undo step.
    let c = call(
        &r,
        Actor::User,
        "set_zooms",
        json!({"zooms": [zoom(30.0, 2.0), zoom(40.0, 2.0)],
               "replace": true}),
    )
    .await
    .unwrap();
    assert_eq!(
        zooms_of(&c),
        vec![(30, 32), (40, 42)],
        "replace swaps them all"
    );
    let d = call(
        &r,
        Actor::User,
        "set_zooms",
        json!({"zooms": [], "replace": true}),
    )
    .await
    .unwrap();
    assert!(zooms_of(&d).is_empty(), "an empty replace clears");
    assert!(manifest(&r.dir)["edits"][0].get("zooms").is_none());

    // Undo walks back one call at a time.
    let u = call(&r, Actor::User, "undo", json!({})).await.unwrap();
    assert_eq!(zooms_of(&u), vec![(30, 32), (40, 42)]);
    let u = call(&r, Actor::User, "undo", json!({})).await.unwrap();
    assert_eq!(zooms_of(&u), vec![(15, 17), (17, 22)]);
    let u = call(&r, Actor::User, "undo", json!({})).await.unwrap();
    assert_eq!(zooms_of(&u), vec![(15, 18)]);
    assert_eq!(std::fs::read(r.dir.join("screen.mp4")).unwrap(), master);
}

#[tokio::test]
async fn edt4_a_zoom_over_a_cut_is_clipped_and_bad_zooms_are_refused() {
    let r = rig().await;
    call(
        &r,
        Actor::User,
        "cut_segments",
        json!({"ranges": [[10, 14]]}),
    )
    .await
    .unwrap();
    let a = call(
        &r,
        Actor::User,
        "set_zooms",
        json!({"zooms": [zoom(12.0, 6.0)]}),
    )
    .await
    .unwrap();
    assert_eq!(
        zooms_of(&a),
        vec![(14, 18)],
        "the part over the cut is dropped"
    );

    for bad in [
        json!({"zooms": []}),
        json!({"zooms": [{"at": 1, "length": 0.2, "focus": [0.5, 0.5]}]}),
        json!({"zooms": [{"at": 1, "length": 3, "focus": [0.5, 0.5], "scale": 5}]}),
        json!({"zooms": [{"at": 1, "length": 3, "focus": [1.5, 0.5]}]}),
        json!({"zooms": [{"at": 1, "length": 3}]}),
        json!({"zooms": [{"at": 1, "length": 3, "focus": [0.5, 0.5], "tilt": 1}]}),
        json!({"zooms": "all"}),
        json!({"zooms": [zoom(200.0, 3.0)]}),
    ] {
        let e = call(&r, Actor::User, "set_zooms", bad.clone())
            .await
            .unwrap_err();
        assert_eq!(code(&e), ErrorCode::InvalidInput, "{bad}");
    }
}

#[tokio::test]
async fn edt4_set_cursor_style_is_stored_validated_and_undoable() {
    let r = rig().await;
    let a = call(
        &r,
        agent("claude"),
        "set_cursor_style",
        json!({"size": 2, "smoothing": "light", "click_effect": "ripple"}),
    )
    .await
    .unwrap();
    assert_eq!(a["edits"]["cursor"]["size"], 2.0);
    assert_eq!(a["edits"]["cursor"]["smoothing"], "light");
    assert_eq!(a["edits"]["cursor"]["click_effect"], "ripple");
    assert_eq!(a["edits"]["cursor"]["hide_idle"], false);
    let b = call(
        &r,
        Actor::User,
        "set_cursor_style",
        json!({"hide_idle": true}),
    )
    .await
    .unwrap();
    assert_eq!(
        b["edits"]["cursor"]["size"], 2.0,
        "fields left out keep their value"
    );
    assert_eq!(b["edits"]["cursor"]["hide_idle"], true);
    assert_eq!(manifest(&r.dir)["edits"][0]["cursor"]["hide_idle"], true);
    for bad in [
        json!({}),
        json!({"size": 0.5}),
        json!({"size": 4}),
        json!({"size": "big"}),
        json!({"smoothing": "wobbly"}),
        json!({"click_effect": "boom"}),
        json!({"hide_idle": "yes"}),
        json!({"colour": "red"}),
    ] {
        let e = call(&r, Actor::User, "set_cursor_style", bad.clone())
            .await
            .unwrap_err();
        assert_eq!(code(&e), ErrorCode::InvalidInput, "{bad}");
    }
    let u = call(&r, Actor::User, "undo", json!({})).await.unwrap();
    assert_eq!(u["edits"]["cursor"]["hide_idle"], false);
    let u = call(&r, Actor::User, "undo", json!({})).await.unwrap();
    assert!(
        u["edits"].get("cursor").is_none(),
        "back to the default style"
    );
}

/// EDT-5.AC3: a 9:16 reframe is stored under its own aspect; nothing else changes.
#[tokio::test]
async fn edt5_set_reframe_is_per_aspect_and_per_segment() {
    let r = rig().await;
    call(&r, Actor::User, "split", json!({"at": 30}))
        .await
        .unwrap();
    let a = call(
        &r,
        agent("claude"),
        "set_reframe",
        json!({"aspect": "9:16", "mode": "fixed", "focus": [0.2, 0.5]}),
    )
    .await
    .unwrap();
    assert_eq!(a["edits"]["reframe"]["9:16"]["mode"], "fixed");
    let b = call(
        &r,
        Actor::User,
        "set_reframe",
        json!({"aspect": "9:16", "focus": [0.8, 0.5], "at": 45}),
    )
    .await
    .unwrap();
    let w = b["edits"]["reframe"]["9:16"]["windows"].as_array().unwrap();
    assert_eq!(w.len(), 2, "one window per segment");
    assert_eq!(w[0]["focus"], json!([0.2, 0.5]));
    assert_eq!(w[1]["focus"], json!([0.8, 0.5]));
    assert_eq!(
        w[1]["from"],
        30 * 120_000,
        "the second segment starts at the split"
    );
    assert!(
        b["edits"]["reframe"].get("1:1").is_none(),
        "other aspects untouched"
    );

    let c = call(
        &r,
        Actor::User,
        "set_reframe",
        json!({"aspect": "1:1", "mode": "follow"}),
    )
    .await
    .unwrap();
    assert_eq!(c["edits"]["reframe"]["1:1"]["mode"], "follow");
    assert_eq!(c["edits"]["reframe"]["9:16"]["mode"], "fixed");

    for bad in [
        json!({"aspect": "4:3", "mode": "fit"}),
        json!({"aspect": "9:16"}),
        json!({"aspect": "9:16", "mode": "zoomy"}),
        json!({"aspect": "9:16", "focus": [2.0, 0.5]}),
        json!({"aspect": "9:16", "focus": [0.5]}),
        json!({"mode": "fit"}),
    ] {
        let e = call(&r, Actor::User, "set_reframe", bad.clone())
            .await
            .unwrap_err();
        assert_eq!(code(&e), ErrorCode::InvalidInput, "{bad}");
    }
    let u = call(&r, Actor::User, "undo", json!({})).await.unwrap();
    assert!(u["edits"]["reframe"].get("1:1").is_none());
}

#[tokio::test]
async fn the_view_edits_are_free_idempotent_and_refused_for_read_only_clients() {
    let r = rig().await;
    let p = json!({"zooms": [zoom(5.0, 2.0)], "request_id": "z-1"});
    let a = call(&r, agent("claude"), "set_zooms", p.clone())
        .await
        .unwrap();
    let b = call(&r, agent("claude"), "set_zooms", p).await.unwrap();
    assert_eq!(a["revision"], b["revision"], "a retry is one edit");
    r.daemon.policies().set("viewer", ClientPolicy::ReadOnly);
    for (m, p) in [
        ("set_zooms", json!({"zooms": [zoom(5.0, 2.0)]})),
        ("set_cursor_style", json!({"size": 2})),
        ("set_reframe", json!({"aspect": "9:16", "mode": "fit"})),
    ] {
        let e = call(&r, agent("viewer"), m, p).await.unwrap_err();
        assert!(matches!(e, DaemonError::Declined { .. }), "{m}: {e:?}");
    }
}
