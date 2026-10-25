//! Recording edits through the daemon (EDT-1 to EDT-3): `trim`, `cut_segments`,
//! `restore_segments`, `split`, `set_speed`, `set_background`, `set_audio_mix` (AUD-4), and
//! `undo` / `redo` on a recording.

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

/// A finished 60 s recording with a fake screen master, indexed by `rescan_library`.
async fn rig() -> Rig {
    rig_with(false).await
}

/// The same with a fake camera master when `camera` is set (CAM-3).
async fn rig_with(camera: bool) -> Rig {
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
    let mut tracks = vec![info];
    if camera {
        std::fs::write(rec.dir().join("camera.mp4"), b"camera bytes").unwrap();
        tracks.push(TrackInfo {
            kind: TrackKind::Camera,
            file: "camera.mp4".into(),
            start: MediaTime::ZERO,
            end: MediaTime::from_secs(60),
            details: json!({}),
            stats: TrackStats::default(),
        });
    }
    let m = rec.finalize(&tracks, now).unwrap();
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

/// EDT-1.AC1: trim 2..58 s of a 60 s recording; the result reports 56 s.
#[tokio::test]
async fn edt1_ac1_trim_reports_56_seconds() {
    let r = rig().await;
    let out = call(&r, Actor::User, "trim", json!({"start": 2.0, "end": 58.0}))
        .await
        .unwrap();
    assert_eq!(out["duration_s"], 56.0);
    assert_eq!(out["action"], "trim");
    assert_eq!(out["can_undo"], true);
    assert_eq!(manifest(&r.dir)["edits"][0]["in"], 240_000);
}

/// EDT-1.AC4: masters are byte-identical after trims, cuts and undo.
#[tokio::test]
async fn edt1_ac4_masters_are_untouched() {
    let r = rig().await;
    let master = r.dir.join("screen.mp4");
    let before = std::fs::read(&master).unwrap();
    call(&r, Actor::User, "trim", json!({"start": 2, "end": 58}))
        .await
        .unwrap();
    call(
        &r,
        Actor::User,
        "cut_segments",
        json!({"ranges": [[10, 15], [20, 22]]}),
    )
    .await
    .unwrap();
    call(&r, Actor::User, "undo", json!({})).await.unwrap();
    assert_eq!(std::fs::read(&master).unwrap(), before);
}

/// EDT-1.AC5 and AGT-3: an agent's three ranges are one revision tagged `agent:<name>` and one
/// undo step; restore and undo return the content.
#[tokio::test]
async fn edt1_ac5_three_ranges_are_one_undo_step() {
    let r = rig().await;
    let mut sub = r.daemon.subscribe();
    let out = call(
        &r,
        agent("claude"),
        "cut_segments",
        json!({"ranges": [[1, 2], [5, 6], [9, 10]]}),
    )
    .await
    .unwrap();
    assert_eq!(out["by"], "agent:claude");
    assert_eq!(out["duration_s"], 57.0);
    let ev = sub.recv().await.unwrap();
    assert_eq!(ev.name, "capture.revised");
    assert_eq!(ev.data["action"], "cut_segments");
    let undone = call(&r, Actor::User, "undo", json!({})).await.unwrap();
    assert_eq!(undone["changed"], true);
    assert_eq!(undone["duration_s"], 60.0);
    assert_eq!(undone["can_redo"], true);
    let redone = call(&r, Actor::User, "redo", json!({})).await.unwrap();
    assert_eq!(redone["duration_s"], 57.0);
    // Nothing more to redo is not an error.
    let none = call(&r, Actor::User, "redo", json!({})).await.unwrap();
    assert_eq!(none["changed"], false);
    assert_eq!(none["revision"], Value::Null);
}

/// EDT-1.AC3: restore returns the original content (one undo step either way).
#[tokio::test]
async fn edt1_ac3_restore_returns_the_content() {
    let r = rig().await;
    call(
        &r,
        Actor::User,
        "cut_segments",
        json!({"ranges": [[10, 15]]}),
    )
    .await
    .unwrap();
    let out = call(
        &r,
        Actor::User,
        "restore_segments",
        json!({"ranges": [[10, 15]]}),
    )
    .await
    .unwrap();
    assert_eq!(out["duration_s"], 60.0);
    let back = call(&r, Actor::User, "undo", json!({})).await.unwrap();
    assert_eq!(back["duration_s"], 55.0);
}

/// EDT-1 edge cases: a cut leaving under 0.5 s is `invalid_input` and writes nothing.
#[tokio::test]
async fn edt1_nothing_left_to_export_is_refused() {
    let r = rig().await;
    let before = std::fs::read(r.dir.join("manifest.json")).unwrap();
    let e = call(
        &r,
        Actor::User,
        "cut_segments",
        json!({"ranges": [[0.1, 60]]}),
    )
    .await
    .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
    assert!(e.to_string().contains("Nothing left to export"), "{e}");
    assert_eq!(std::fs::read(r.dir.join("manifest.json")).unwrap(), before);
}

/// EDT-2.AC1 and AC3: speed shortens the segment; above 2x the audio is muted unless kept.
#[tokio::test]
async fn edt2_speed_duration_and_audio() {
    let r = rig().await;
    let out = call(
        &r,
        Actor::User,
        "set_speed",
        json!({"range": [10, 30], "factor": 2.0}),
    )
    .await
    .unwrap();
    assert_eq!(out["duration_s"], 50.0);
    assert_eq!(out["edits"]["speeds"][0]["factor"], 2.0);
    assert_eq!(out["edits"]["splits"], json!([1_200_000, 3_600_000]));
    let fast = call(
        &r,
        Actor::User,
        "set_speed",
        json!({"range": [40, 60], "factor": 4.0, "keep_audio": true}),
    )
    .await
    .unwrap();
    assert_eq!(fast["edits"]["speeds"][1]["keep_audio"], true);
    let e = call(
        &r,
        Actor::User,
        "set_speed",
        json!({"range": [1, 2], "factor": 12}),
    )
    .await
    .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
}

/// EDT-3: background settings are validated and stored; split adds a boundary.
#[tokio::test]
async fn edt3_background_and_split() {
    let r = rig().await;
    let out = call(
        &r,
        Actor::User,
        "set_background",
        json!({"kind": "color", "value": "#112233", "padding_pct": 8, "radius_px": 12, "shadow": true}),
    )
    .await
    .unwrap();
    assert_eq!(out["edits"]["background"]["radius_px"], 12);
    for bad in [
        json!({"kind": "color", "value": "#112233", "padding_pct": 30}),
        json!({"kind": "color", "value": "#112233", "radius_px": 40}),
        json!({"kind": "color", "value": "blue"}),
        json!({"kind": "gradient", "value": "#000"}),
    ] {
        let e = call(&r, Actor::User, "set_background", bad)
            .await
            .unwrap_err();
        assert_eq!(code(&e), ErrorCode::InvalidInput);
    }
    let s = call(&r, Actor::User, "split", json!({"at": 30}))
        .await
        .unwrap();
    assert_eq!(s["edits"]["splits"], json!([3_600_000]));
    let e = call(&r, Actor::User, "split", json!({"at": 30}))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
}

/// EDT-3 (M4 gate finding F4): an image background that is missing or does not decode is refused
/// when it is set (`invalid_input` naming the path), not later as a failed export; a PNG or a JPEG,
/// by absolute path or relative to the recording's folder, is accepted.
#[tokio::test]
async fn edt3_image_background_is_checked_when_it_is_set() {
    let r = rig().await;
    let before = manifest(&r.dir)["edits"].clone();
    let missing = r.dir.join("nope.png");
    let e = call(
        &r,
        Actor::User,
        "set_background",
        json!({"kind": "image", "value": missing}),
    )
    .await
    .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
    assert!(e.to_string().contains("does not exist"), "{e}");
    // A file that is not an image.
    std::fs::write(r.dir.join("bad.png"), b"not an image").unwrap();
    let e = call(
        &r,
        Actor::User,
        "set_background",
        json!({"kind": "image", "value": "bad.png"}),
    )
    .await
    .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
    assert!(e.to_string().contains("PNG or JPEG"), "{e}");
    // No value at all.
    let e = call(&r, Actor::User, "set_background", json!({"kind": "image"}))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
    // Nothing was stored by the refused calls.
    assert_eq!(manifest(&r.dir)["edits"], before);

    let img = image::RgbImage::from_pixel(8, 6, image::Rgb([200, 30, 30]));
    img.save(r.dir.join("bg.png")).unwrap();
    img.save(r.dir.join("bg.jpg")).unwrap();
    let abs = r.dir.join("bg.png");
    for value in [json!(abs), json!("bg.png"), json!("bg.jpg")] {
        let out = call(
            &r,
            Actor::User,
            "set_background",
            json!({"kind": "image", "value": value}),
        )
        .await
        .unwrap();
        assert_eq!(out["edits"]["background"]["kind"], "image");
    }
}

/// AUD-4: the mix is stored, partial updates keep the rest, it is one undo step tagged with the
/// caller, bad values are refused, and the masters stay byte-identical.
#[tokio::test]
async fn aud4_audio_mix_is_stored_validated_and_undoable() {
    let r = rig().await;
    let master = std::fs::read(r.dir.join("screen.mp4")).unwrap();
    let a = call(
        &r,
        agent("claude"),
        "set_audio_mix",
        json!({"mic_gain_db": 0, "system_gain_db": -6}),
    )
    .await
    .unwrap();
    assert_eq!(a["edits"]["audio"]["system_gain_db"], -6.0);
    assert_eq!(a["by"], "agent:claude");
    let b = call(
        &r,
        Actor::User,
        "set_audio_mix",
        json!({"system_mute": true, "denoise": false}),
    )
    .await
    .unwrap();
    assert_eq!(b["edits"]["audio"]["system_gain_db"], -6.0);
    assert_eq!(b["edits"]["audio"]["system_mute"], true);
    assert_eq!(b["edits"]["audio"]["denoise"], false);
    assert_eq!(manifest(&r.dir)["edits"][0]["audio"]["system_mute"], true);
    for bad in [
        json!({}),
        json!({"mic_gain_db": 13}),
        json!({"system_gain_db": -200}),
        json!({"mic_gain_db": "loud"}),
        json!({"mic_mute": "yes"}),
    ] {
        let e = call(&r, Actor::User, "set_audio_mix", bad)
            .await
            .unwrap_err();
        assert_eq!(code(&e), ErrorCode::InvalidInput);
    }
    // One undo step each: the first undo returns to the -6 dB state with the mute off.
    let undone = call(&r, Actor::User, "undo", json!({})).await.unwrap();
    assert_eq!(undone["edits"]["audio"]["system_mute"], false);
    assert_eq!(undone["edits"]["audio"]["system_gain_db"], -6.0);
    let undone = call(&r, Actor::User, "undo", json!({})).await.unwrap();
    assert_eq!(undone["edits"]["audio"]["system_gain_db"], 0.0);
    assert_eq!(std::fs::read(r.dir.join("screen.mp4")).unwrap(), master);
}

/// `request_id` makes a retried edit one edit.
#[tokio::test]
async fn edits_are_idempotent_with_a_request_id() {
    let r = rig().await;
    let p = json!({"ranges": [[10, 15]], "request_id": "req-1"});
    let a = call(&r, agent("claude"), "cut_segments", p.clone())
        .await
        .unwrap();
    let b = call(&r, agent("claude"), "cut_segments", p).await.unwrap();
    assert_eq!(a["revision"], b["revision"]);
    let m = manifest(&r.dir);
    assert_eq!(m["edit_history"]["steps"].as_array().unwrap().len(), 1);
}

/// A read-only client is refused; a screenshot is not an editable recording.
#[tokio::test]
async fn read_only_clients_and_screenshots_are_refused() {
    let r = rig().await;
    r.daemon.policies().set("viewer", ClientPolicy::ReadOnly);
    let e = call(&r, agent("viewer"), "trim", json!({"start": 1, "end": 5}))
        .await
        .unwrap_err();
    assert!(matches!(e, DaemonError::Declined { .. }), "{e:?}");
    let shot = r
        .daemon
        .execute_method(
            Actor::User,
            0,
            "capture_screenshot",
            &json!({"target": {"display_id": 1}}),
        )
        .await
        .unwrap();
    let e = r
        .daemon
        .execute_method(
            Actor::User,
            0,
            "trim",
            &json!({"capture_id": shot["capture_id"], "start": 1, "end": 5}),
        )
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
}

/// An unfinished recording is `busy`.
#[tokio::test]
async fn editing_a_recording_in_progress_is_busy() {
    let r = rig().await;
    std::fs::write(r.dir.join("recording.lock"), "{}").unwrap();
    let e = call(&r, Actor::User, "split", json!({"at": 5}))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::Busy);
}

/// CAM-3.AC5: each layout change is one undo step tagged with the caller; AC2: camera full over
/// 0..15 s is stored as a range; a later range overwrites the overlap; masters stay untouched.
#[tokio::test]
async fn cam3_layout_changes_are_stored_and_one_undo_step_each() {
    let r = rig_with(true).await;
    let camera = std::fs::read(r.dir.join("camera.mp4")).unwrap();
    let a = call(
        &r,
        agent("claude"),
        "set_camera_layout",
        json!({"range": [0, 15], "layout": "camera"}),
    )
    .await
    .unwrap();
    assert_eq!(a["action"], "set_camera_layout");
    assert_eq!(a["by"], "agent:claude");
    let seg = &a["edits"]["layout"]["segments"][0];
    assert_eq!(
        (seg["from"].clone(), seg["to"].clone()),
        (json!(0), json!(1_800_000))
    );
    assert_eq!(seg["mode"], "camera");
    let b = call(
        &r,
        Actor::User,
        "set_camera_layout",
        json!({"range": [10, 30], "layout": "bubble", "shape": "rounded",
               "rect": [0.05, 0.05, 0.2, 0.2], "ring": false, "transition": "smooth"}),
    )
    .await
    .unwrap();
    let segs = b["edits"]["layout"]["segments"].as_array().unwrap();
    assert_eq!(segs.len(), 2);
    assert_eq!(segs[0]["to"], 1_200_000);
    assert_eq!(segs[1]["shape"], "rounded");
    assert_eq!(segs[1]["ring"], 0.0);
    assert_eq!(segs[1]["transition"], "smooth");
    // One undo step each: the first undo removes only the second range.
    let undone = call(&r, Actor::User, "undo", json!({})).await.unwrap();
    assert_eq!(
        undone["edits"]["layout"]["segments"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(undone["edits"]["layout"]["segments"][0]["to"], 1_800_000);
    let undone = call(&r, Actor::User, "undo", json!({})).await.unwrap();
    assert!(undone["edits"]["layout"].get("segments").is_none());
    assert_eq!(std::fs::read(r.dir.join("camera.mp4")).unwrap(), camera);
}

/// CAM-3 edge case: without a camera track only the `screen` layout applies; bad fields are
/// refused with `invalid_input`; a retry with the same request id replays.
#[tokio::test]
async fn cam3_needs_a_camera_track_and_validates_fields() {
    let r = rig().await;
    let e = call(
        &r,
        Actor::User,
        "set_camera_layout",
        json!({"range": [0, 5], "layout": "camera"}),
    )
    .await
    .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
    assert!(e.to_string().contains("no camera track"), "{e}");
    call(
        &r,
        Actor::User,
        "set_camera_layout",
        json!({"range": [0, 5], "layout": "screen"}),
    )
    .await
    .unwrap();
    let r = rig_with(true).await;
    for bad in [
        json!({"range": [0, 5], "layout": "wide"}),
        json!({"range": [0, 5], "layout": "camera", "shape": "circle"}),
        json!({"range": [0, 5], "layout": "bubble", "rect": [0.1, 0.1, 0.01, 0.01]}),
        json!({"range": [70, 80], "layout": "screen"}),
    ] {
        let e = call(&r, Actor::User, "set_camera_layout", bad)
            .await
            .unwrap_err();
        assert_eq!(code(&e), ErrorCode::InvalidInput);
    }
    let p = json!({"range": [1, 4], "layout": "side_by_side", "request_id": "same"});
    let first = call(&r, Actor::User, "set_camera_layout", p.clone())
        .await
        .unwrap();
    let again = call(&r, Actor::User, "set_camera_layout", p).await.unwrap();
    assert_eq!(first["revision"], again["revision"]);
}
