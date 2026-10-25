//! `list_destinations` and `upload` through the daemon (STO-1, STO-3, STO-5): the job copies a
//! file, an export or a whole bundle to a local destination, waits while it is offline, and never
//! overwrites.

#![allow(clippy::unwrap_used)]

mod common;

use std::path::{Path, PathBuf};
use std::time::Duration;

use cm_core::{Actor, ErrorCode, MediaTime, TrackInfo, TrackKind, TrackStats};
use cm_daemon::{Daemon, DaemonError};
use cm_library::rec::{RecBundle, SessionInfo};
use cm_storage::{Destination, DestinationStore, Kind, What, DEFAULT_FOLDER_PATTERN};
use common::{builder, desktop};
use serde_json::{json, Map, Value};
use time::macros::datetime;

struct Rig {
    daemon: Daemon,
    store: DestinationStore,
    id: String,
    dir: PathBuf,
    nas: PathBuf,
    tmp: tempfile::TempDir,
    _lib: tempfile::TempDir,
}

fn dest(id: &str, path: &Path, what: What, default: bool) -> Destination {
    Destination {
        id: id.into(),
        name: id.into(),
        kind: Kind::Local,
        path: path.to_path_buf(),
        account: None,
        folder_pattern: DEFAULT_FOLDER_PATTERN.into(),
        what,
        auto_upload: cm_storage::AutoUpload::Off,
        default,
    }
}

async fn rig(poll: Duration) -> Rig {
    let tmp = tempfile::tempdir().unwrap();
    let nas = tmp.path().join("nas");
    std::fs::create_dir(&nas).unwrap();
    let store = DestinationStore::new(tmp.path().join("config"));
    store
        .save(&[dest("nas", &nas, What::Original, true)])
        .unwrap();

    let (b, lib) = builder(desktop());
    let queue = cm_daemon::library_job_queue(lib.path()).unwrap();
    let daemon = b.jobs(queue).destinations(store.clone()).build();
    daemon
        .register_job_handler(cm_jobs::JOB_UPLOAD, daemon.upload_handler(poll))
        .unwrap();
    daemon.start_jobs().unwrap();

    let now = datetime!(2026-10-22 10:00:00 UTC);
    let session = SessionInfo {
        clock_origin_ns: 1,
        timebase: "uptime_raw_ns".into(),
        displays: vec![],
        devices: json!({}),
        app_version: "0.0.1".into(),
        extra: Map::new(),
    };
    let rec = RecBundle::create(lib.path(), "Lesson", now, json!({}), &session).unwrap();
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
        store,
        id: m.id,
        dir,
        nas,
        tmp,
        _lib: lib,
    }
}

async fn call(r: &Rig, who: Actor, method: &str, p: Value) -> Result<Value, DaemonError> {
    r.daemon.execute_method(who, 0, method, &p).await
}

async fn done(r: &Rig, job_id: &str) -> Value {
    for _ in 0..500 {
        let j = call(r, Actor::User, "get_job", json!({ "job_id": job_id }))
            .await
            .unwrap();
        if matches!(j["state"].as_str(), Some("done" | "failed" | "cancelled")) {
            return j;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("the upload job did not finish");
}

fn add_export(r: &Rig) -> PathBuf {
    std::fs::create_dir_all(r.dir.join("exports")).unwrap();
    let file = r.dir.join("exports/Lesson - quick.mp4");
    std::fs::write(&file, b"exported video").unwrap();
    cm_library::record_export(
        &r.dir,
        json!({ "file": "exports/Lesson - quick.mp4", "preset": "quick" }),
        Actor::User,
        datetime!(2026-10-22 11:00:00 UTC),
    )
    .unwrap();
    file
}

fn files_under(root: &Path) -> Vec<String> {
    fn rec(dir: &Path, root: &Path, out: &mut Vec<String>) {
        for e in std::fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                rec(&p, root, out);
            } else {
                out.push(
                    p.strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }
    let mut out = Vec::new();
    rec(root, root, &mut out);
    out.sort();
    out
}

fn code(e: &DaemonError) -> ErrorCode {
    e.code().unwrap()
}

#[tokio::test]
async fn list_destinations_shows_the_store_and_whether_each_is_online() {
    let r = rig(Duration::from_millis(20)).await;
    let missing = r.tmp.path().join("gone");
    let mut all = r.store.list().unwrap();
    all.push(dest("usb", &missing, What::Export, false));
    r.store.save(&all).unwrap();
    let v = call(
        &r,
        Actor::agent("viewer").unwrap(),
        "list_destinations",
        json!({}),
    )
    .await
    .unwrap();
    let list = v["destinations"].as_array().unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0]["id"], "nas");
    assert_eq!(list[0]["online"], true);
    assert_eq!(list[0]["default"], true);
    assert_eq!(list[1]["online"], false);
}

#[tokio::test]
async fn no_store_means_no_destinations_and_upload_says_so() {
    let (b, _lib) = builder(desktop());
    let d = b.build();
    let v = d
        .execute_method(Actor::User, 0, "list_destinations", &json!({}))
        .await
        .unwrap();
    assert_eq!(v["destinations"], json!([]));
    let e = d
        .execute_method(Actor::User, 0, "upload", &json!({ "file": "/x" }))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::NotFound);
}

// STO-1.AC1
#[tokio::test]
async fn a_file_lands_at_the_pattern_path_with_the_same_bytes() {
    let r = rig(Duration::from_millis(20)).await;
    let src = r.tmp.path().join("notes.txt");
    std::fs::write(&src, b"hello").unwrap();
    let v = call(
        &r,
        Actor::User,
        "upload",
        json!({ "file": src, "folder": "Misc/Notes" }),
    )
    .await
    .unwrap();
    let j = done(&r, v["job_id"].as_str().unwrap()).await;
    assert_eq!(j["state"], "done", "{j}");
    assert_eq!(
        std::fs::read(r.nas.join("Misc/Notes/notes.txt")).unwrap(),
        b"hello"
    );
    assert_eq!(j["result"]["bytes"], 5);
}

// STO-5.AC1: the bundle reopens from the copy; a second copy gets its own folder.
#[tokio::test]
async fn an_original_bundle_is_copied_whole_and_never_mixed_with_an_earlier_copy() {
    let r = rig(Duration::from_millis(20)).await;
    for _ in 0..2 {
        let v = call(&r, Actor::User, "upload", json!({ "capture_id": r.id }))
            .await
            .unwrap();
        assert_eq!(v["what"], "original");
        let j = done(&r, v["job_id"].as_str().unwrap()).await;
        assert_eq!(j["state"], "done", "{j}");
    }
    let files = files_under(&r.nas);
    let bundle = r.dir.file_name().unwrap().to_string_lossy().into_owned();
    let second = bundle.replacen(".cmrec", " (2).cmrec", 1);
    let base = "CaptureMann/2026/10/Lesson";
    for name in [&bundle, &second] {
        assert!(
            files.contains(&format!("{base}/{name}/manifest.json")),
            "{files:?}"
        );
        assert!(
            files.contains(&format!("{base}/{name}/screen.mp4")),
            "{files:?}"
        );
    }
}

// STO-5.AC2
#[tokio::test]
async fn export_only_sends_just_the_export_and_needs_one() {
    let r = rig(Duration::from_millis(20)).await;
    let e = call(
        &r,
        Actor::User,
        "upload",
        json!({ "capture_id": r.id, "what": "export" }),
    )
    .await
    .unwrap_err();
    assert_eq!(code(&e), ErrorCode::NotFound);
    assert!(e.to_string().contains("export_video"), "{e}");

    add_export(&r);
    let v = call(
        &r,
        Actor::User,
        "upload",
        json!({ "capture_id": r.id, "what": "export" }),
    )
    .await
    .unwrap();
    let j = done(&r, v["job_id"].as_str().unwrap()).await;
    assert_eq!(j["state"], "done", "{j}");
    assert_eq!(
        files_under(&r.nas),
        vec!["CaptureMann/2026/10/Lesson/Lesson - quick.mp4".to_owned()]
    );
}

// STO-1.AC2
#[tokio::test]
async fn an_offline_destination_waits_and_finishes_when_it_returns() {
    let r = rig(Duration::from_millis(30)).await;
    let usb = r.tmp.path().join("usb");
    r.store
        .save(&[dest("usb", &usb, What::Original, true)])
        .unwrap();
    let src = r.tmp.path().join("a.txt");
    std::fs::write(&src, b"later").unwrap();
    let v = call(
        &r,
        Actor::User,
        "upload",
        json!({ "file": src, "folder": "x" }),
    )
    .await
    .unwrap();
    let id = v["job_id"].as_str().unwrap().to_owned();

    tokio::time::sleep(Duration::from_millis(250)).await;
    let j = call(&r, Actor::User, "get_job", json!({ "job_id": id }))
        .await
        .unwrap();
    assert_eq!(j["state"], "running", "{j}");
    assert_eq!(j["message"], "Destination offline", "{j}");

    std::fs::create_dir(&usb).unwrap();
    let j = done(&r, &id).await;
    assert_eq!(j["state"], "done", "{j}");
    assert_eq!(std::fs::read(usb.join("x/a.txt")).unwrap(), b"later");
}

#[tokio::test]
async fn bad_requests_name_the_problem() {
    let r = rig(Duration::from_millis(20)).await;
    let who = Actor::User;
    let e = call(
        &r,
        who.clone(),
        "upload",
        json!({ "capture_id": r.id, "file": "/x" }),
    )
    .await
    .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
    let e = call(&r, who.clone(), "upload", json!({}))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
    let e = call(
        &r,
        who.clone(),
        "upload",
        json!({ "capture_id": r.id, "destination_id": "zzz" }),
    )
    .await
    .unwrap_err();
    assert_eq!(code(&e), ErrorCode::NotFound);
    let e = call(
        &r,
        who.clone(),
        "upload",
        json!({ "capture_id": r.id, "what": "everything" }),
    )
    .await
    .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
    let e = call(
        &r,
        who.clone(),
        "upload",
        json!({ "file": "/definitely/not/here" }),
    )
    .await
    .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
    let e = call(&r, who, "upload", json!({ "capture_id": "nope" }))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::NotFound);
}

/// An agent may upload (Free tier) but the destination list is the user's: nothing in the
/// request can name a path of its own.
#[tokio::test]
async fn agents_upload_to_a_listed_destination_only() {
    let r = rig(Duration::from_millis(20)).await;
    let a = Actor::agent("claude").unwrap();
    let e = call(
        &r,
        a.clone(),
        "upload",
        json!({ "capture_id": r.id, "path": "/tmp/evil" }),
    )
    .await
    .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
    let v = call(&r, a, "upload", json!({ "capture_id": r.id }))
        .await
        .unwrap();
    assert_eq!(
        done(&r, v["job_id"].as_str().unwrap()).await["state"],
        "done"
    );
}
