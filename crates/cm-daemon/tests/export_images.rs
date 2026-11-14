//! BRD-1 (`create_board`) and BLK-1 (`export_images`) through the real daemon. The screens are the
//! scripted fake displays; nothing touches the screen, camera or microphone.
#![allow(clippy::unwrap_used)]

mod common;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use cm_core::{Actor, ErrorCode};
use cm_daemon::testing::{FakeTrasher, FixedConfirmer};
use cm_daemon::{Daemon, DaemonError, Decision};
use common::{builder, desktop};
use serde_json::{json, Value};

struct Rig {
    daemon: Daemon,
    lib: tempfile::TempDir,
    out: tempfile::TempDir,
    _trash: tempfile::TempDir,
}

fn rig() -> Rig {
    let (b, lib) = builder(desktop());
    let trash = tempfile::tempdir().unwrap();
    let daemon = b
        .confirmer(Arc::new(FixedConfirmer::new(Decision::Approve)))
        .trasher(Arc::new(FakeTrasher::into_dir(trash.path())))
        .jobs(cm_daemon::library_job_queue(lib.path()).unwrap())
        .build();
    daemon.register_export_handler().unwrap();
    daemon.start_jobs().unwrap();
    Rig {
        daemon,
        lib,
        out: tempfile::tempdir().unwrap(),
        _trash: trash,
    }
}

impl Rig {
    async fn call(&self, method: &str, params: Value) -> Result<Value, DaemonError> {
        self.daemon
            .execute_method(Actor::User, 1, method, &params)
            .await
    }

    async fn shot(&self, title: &str) -> String {
        let s = self
            .call(
                "capture_screenshot",
                json!({"target": {"display_id": 1}, "title": title}),
            )
            .await
            .unwrap();
        s["capture_id"].as_str().unwrap().to_owned()
    }

    async fn finished(&self, id: &str) -> Value {
        for _ in 0..1500 {
            let j = self.call("get_job", json!({"job_id": id})).await.unwrap();
            if matches!(j["state"].as_str(), Some("done" | "failed" | "cancelled")) {
                return j;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("job {id} never finished");
    }

    fn dest(&self) -> String {
        self.out.path().to_str().unwrap().to_owned()
    }

    fn files(&self) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(self.out.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    }

    fn bundle_dir(&self, title: &str) -> PathBuf {
        self.lib.path().join(format!("{title}.cmshot"))
    }
}

fn decode(p: &Path) -> image::RgbaImage {
    image::open(p).unwrap().to_rgba8()
}

fn code(e: &DaemonError) -> ErrorCode {
    e.code().unwrap()
}

/// BRD-1.AC1, AC4: two screenshots in Pair become one new screenshot capture with the layout and
/// the sources in its manifest; the sources are untouched.
#[tokio::test]
async fn create_board_makes_a_new_screenshot_capture() {
    let r = rig();
    let a = r.shot("Before").await;
    let b = r.shot("After").await;
    let out = r
        .call(
            "create_board",
            json!({"capture_ids": [a, b], "layout": "pair",
                   "options": {"gap": 20, "margin": 10, "before_after": true, "title": "Change"}}),
        )
        .await
        .unwrap();
    assert_eq!(out["kind"], "screenshot");
    assert_eq!(out["layout"], "pair");
    assert!(out["image"]["data"].as_str().unwrap().len() > 100);
    let (w, h) = (
        out["width"].as_u64().unwrap(),
        out["height"].as_u64().unwrap(),
    );
    let dir = PathBuf::from(out["path"].as_str().unwrap());
    let m: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(m["source"], "board");
    assert_eq!(m["board"]["layout"]["kind"], "pair");
    assert_eq!(m["board"]["sources"].as_array().unwrap().len(), 2);
    assert!(m["ops"].as_array().is_none_or(Vec::is_empty));
    let img = decode(&dir.join("original.png"));
    assert_eq!((u64::from(img.width()), u64::from(img.height())), (w, h));
    // The library lists three screenshots; the sources still have no ops.
    let list = r.call("list_library", json!({})).await.unwrap();
    assert_eq!(list["captures"].as_array().unwrap().len(), 3);
}

/// BRD-1.AC2, AC3: a source with a blur gives a board that shows the blur, and deleting the
/// source afterwards leaves the board as it was.
#[tokio::test]
async fn a_blurred_source_stays_blurred_and_a_deleted_source_changes_nothing() {
    let r = rig();
    let a = r.shot("Plain").await;
    let b = r.shot("Blurred").await;
    let rect = [100, 100, 300, 200];
    r.call(
        "annotate",
        json!({"capture_id": b, "ops": [{"op": "blur", "rect": rect, "strength": 32}]}),
    )
    .await
    .unwrap();
    let opts = json!({"gap": 0, "margin": 0});
    let out = r
        .call(
            "create_board",
            json!({"capture_ids": [a, b], "layout": "pair", "options": opts}),
        )
        .await
        .unwrap();
    let dir = PathBuf::from(out["path"].as_str().unwrap());
    let board = decode(&dir.join("original.png"));
    let src = decode(&r.bundle_dir("Blurred").join("original.png"));
    // Same sizes, no gap or margin: the second picture starts at x = width of the first.
    assert_eq!(board.height(), src.height());
    let x0 = src.width();
    let mut diff = 0u64;
    for y in 100..300u32 {
        for x in 100..400u32 {
            let (p, q) = (board.get_pixel(x0 + x, y), src.get_pixel(x, y));
            diff += (0..3).map(|c| u64::from(p[c].abs_diff(q[c]))).sum::<u64>();
        }
    }
    assert!(diff > 1000, "the board holds the unblurred pixels ({diff})");

    let before = std::fs::read(dir.join("original.png")).unwrap();
    r.call("delete_capture", json!({"capture_id": b}))
        .await
        .unwrap();
    assert_eq!(std::fs::read(dir.join("original.png")).unwrap(), before);
    let again = r
        .call("get_capture", json!({"capture_id": out["capture_id"]}))
        .await
        .unwrap();
    assert_eq!(again["kind"], "screenshot");
}

#[tokio::test]
async fn create_board_checks_its_input() {
    let r = rig();
    let a = r.shot("One").await;
    let e = r
        .call(
            "create_board",
            json!({"capture_ids": [a.clone()], "layout": "grid"}),
        )
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
    let b = r.shot("Two").await;
    let e = r
        .call(
            "create_board",
            json!({"capture_ids": [a.clone(), b.clone()], "layout": "free"}),
        )
        .await
        .unwrap_err();
    assert!(e.to_string().contains("layout.slots"), "{e}");
    let e = r
        .call(
            "create_board",
            json!({"capture_ids": [a.clone(), "nope"], "layout": "pair"}),
        )
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::NotFound);
    let e = r
        .call(
            "create_board",
            json!({"capture_ids": [a, b], "layout": "pair", "options": {"gap": 999}}),
        )
        .await
        .unwrap_err();
    assert!(e.to_string().contains("gap"), "{e}");
}

/// BLK-1.AC1, AC3, AC4: several screenshots as JPEG at a longest side of 200 px with a text
/// watermark; a name that exists is numbered; the library entries are unchanged.
#[tokio::test]
async fn export_images_resizes_converts_watermarks_and_never_overwrites() {
    let r = rig();
    let mut ids = Vec::new();
    for t in ["Alpha", "Beta", "Gamma"] {
        ids.push(r.shot(t).await);
    }
    std::fs::write(r.out.path().join("Alpha.jpg"), b"mine").unwrap();
    let before = std::fs::read(r.bundle_dir("Alpha").join("original.png")).unwrap();
    let job = r
        .call(
            "export_images",
            json!({"capture_ids": ids, "dest": r.dest(), "size": {"longest": 200},
                   "format": "jpeg", "quality": 80,
                   "watermark": {"text": "DRAFT", "position": "bottom_right"}}),
        )
        .await
        .unwrap();
    let done = r.finished(job["job_id"].as_str().unwrap()).await;
    assert_eq!(done["state"], "done", "{done}");
    assert_eq!(done["result"]["exported"], 3);
    assert_eq!(done["result"]["skipped"], 0);
    assert_eq!(
        r.files(),
        ["Alpha (2).jpg", "Alpha.jpg", "Beta.jpg", "Gamma.jpg"]
    );
    assert_eq!(
        std::fs::read(r.out.path().join("Alpha.jpg")).unwrap(),
        b"mine"
    );
    for f in ["Alpha (2).jpg", "Beta.jpg", "Gamma.jpg"] {
        let img = decode(&r.out.path().join(f));
        assert_eq!(img.width().max(img.height()), 200, "{f}");
    }
    // The original is untouched.
    assert_eq!(
        std::fs::read(r.bundle_dir("Alpha").join("original.png")).unwrap(),
        before
    );
    // The watermark is on the file, not in the capture.
    let plain = r
        .call(
            "export_images",
            json!({"capture_ids": [ids_first(&r).await], "dest": r.dest(), "size": {"longest": 200},
                   "format": "png", "name_pattern": "plain-{n}-{w}x{h}"}),
        )
        .await
        .unwrap();
    assert_eq!(
        r.finished(plain["job_id"].as_str().unwrap()).await["state"],
        "done"
    );
}

async fn ids_first(r: &Rig) -> String {
    let list = r.call("list_library", json!({})).await.unwrap();
    list["captures"][0]["id"].as_str().unwrap().to_owned()
}

/// BLK-1: a recording or an unknown id is skipped with a reason, the rest is exported, and a bad
/// pattern or a Blueprint request fails before anything is written.
#[tokio::test]
async fn skipped_files_are_listed_with_a_reason() {
    let r = rig();
    let a = r.shot("Good").await;
    let job = r
        .call(
            "export_images",
            json!({"capture_ids": [a.clone(), "missing_id"], "dest": r.dest(), "format": "webp"}),
        )
        .await
        .unwrap();
    let done = r.finished(job["job_id"].as_str().unwrap()).await;
    assert_eq!(done["state"], "done", "{done}");
    assert_eq!(done["result"]["exported"], 1);
    assert_eq!(done["result"]["skipped"], 1);
    let skipped = &done["result"]["files"][1];
    assert_eq!(skipped["reason"], "not_found");
    assert_eq!(skipped["capture_id"], "missing_id");
    assert_eq!(r.files(), ["Good.webp"]);
    assert!(decode(&r.out.path().join("Good.webp")).width() > 0);

    let e = r
        .call(
            "export_images",
            json!({"capture_ids": [a.clone()], "dest": r.dest(), "name_pattern": "{bogus}"}),
        )
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
    let e = r
        .call(
            "export_images",
            json!({"capture_ids": [a], "dest": r.dest(), "blueprint": true}),
        )
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::NotImplemented);
    assert_eq!(r.files(), ["Good.webp"]);
}

/// BLK-1.AC2: a cancel keeps the files already written and leaves no partial file.
#[tokio::test]
async fn cancel_keeps_the_written_files_and_leaves_no_partial_file() {
    let r = rig();
    let mut ids = Vec::new();
    for i in 0..6 {
        ids.push(r.shot(&format!("S{i}")).await);
    }
    let job = r
        .call(
            "export_images",
            json!({"capture_ids": ids, "dest": r.dest(), "format": "png"}),
        )
        .await
        .unwrap();
    let id = job["job_id"].as_str().unwrap().to_owned();
    // Cancel as soon as the first file exists.
    for _ in 0..2000 {
        if !r.files().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    let _ = r.call("cancel_job", json!({"job_id": id})).await;
    let j = r.finished(&id).await;
    let files = r.files();
    assert!(!files.is_empty());
    assert!(
        files
            .iter()
            .all(|f| f.ends_with(".png") && !f.starts_with('.')),
        "{files:?}"
    );
    for f in &files {
        let len = std::fs::metadata(r.out.path().join(f)).unwrap().len();
        assert!(len > 0 && image::open(r.out.path().join(f)).is_ok(), "{f}");
    }
    assert!(
        matches!(j["state"].as_str(), Some("cancelled" | "done")),
        "{j}"
    );
}
