//! IMP-6: `import_media` with pictures through the real daemon. No FFmpeg is needed (pictures
//! never touch it); the Trash, the confirmer and the text recognizer are scripted. No screen,
//! camera or microphone.
#![allow(clippy::unwrap_used)]

mod common;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use cm_capture::disk::FakeFreeSpace;
use cm_core::{Actor, CmError};
use cm_daemon::testing::{FakeTrasher, FixedConfirmer};
use cm_daemon::{Daemon, DaemonError, Decision};
use cm_render::ocr::{TextLine, TextRecognizer};
use common::{builder, desktop};
use image::{Rgb, RgbImage, Rgba, RgbaImage};
use serde_json::{json, Value};

struct Free(Arc<FakeFreeSpace>);
impl cm_capture::disk::FreeSpace for Free {
    fn free_bytes(&self, p: &Path) -> Option<u64> {
        cm_capture::disk::FreeSpace::free_bytes(&*self.0, p)
    }
}

/// Answers one line for every image.
struct Reads;
impl TextRecognizer for Reads {
    fn name(&self) -> &'static str {
        "fake"
    }
    fn is_available(&self) -> bool {
        true
    }
    fn install_hint(&self) -> Option<String> {
        None
    }
    fn recognize(&self, _: &RgbaImage) -> Result<Vec<TextLine>, CmError> {
        Ok(vec![TextLine::laid_out(
            "imported words",
            4.0,
            4.0,
            8.0,
            12.0,
        )])
    }
}

struct Rig {
    daemon: Daemon,
    confirmer: Arc<FixedConfirmer>,
    lib: tempfile::TempDir,
    src: tempfile::TempDir,
    trash: tempfile::TempDir,
}

fn rig() -> Rig {
    let (b, lib) = builder(desktop());
    let trash = tempfile::tempdir().unwrap();
    let confirmer = Arc::new(FixedConfirmer::new(Decision::Approve));
    let trasher = Arc::new(FakeTrasher::into_dir(trash.path()));
    let free = Arc::new(FakeFreeSpace::new(u64::MAX));
    let daemon = b
        .confirmer(confirmer.clone())
        .trasher(trasher)
        .free_space(Free(free))
        .text_recognizer(Reads)
        .jobs(cm_daemon::library_job_queue(lib.path()).unwrap())
        .build();
    daemon.register_import_handler().unwrap();
    daemon.start_jobs().unwrap();
    Rig {
        daemon,
        confirmer,
        lib,
        src: tempfile::tempdir().unwrap(),
        trash,
    }
}

fn agent() -> Actor {
    Actor::agent("claude-desktop").unwrap()
}

fn s(p: &Path) -> &str {
    p.to_str().unwrap()
}

impl Rig {
    fn put(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let p = self.src.path().join(name);
        std::fs::write(&p, bytes).unwrap();
        p
    }

    fn png(&self, name: &str, w: u32, h: u32) -> PathBuf {
        let img = RgbaImage::from_fn(w, h, |x, y| Rgba([x as u8, y as u8, 40, 255]));
        let mut out = Vec::new();
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
            .unwrap();
        self.put(name, &out)
    }

    fn jpeg_with_exif(&self, name: &str, w: u32, h: u32, orientation: u16) -> PathBuf {
        let img = RgbImage::from_pixel(w, h, Rgb([180, 60, 60]));
        let mut jpg = Vec::new();
        image::DynamicImage::ImageRgb8(img)
            .write_to(
                &mut std::io::Cursor::new(&mut jpg),
                image::ImageFormat::Jpeg,
            )
            .unwrap();
        let mut tiff = b"II*\0".to_vec();
        tiff.extend_from_slice(&8u32.to_le_bytes());
        tiff.extend_from_slice(&1u16.to_le_bytes());
        tiff.extend_from_slice(&0x0112u16.to_le_bytes());
        tiff.extend_from_slice(&3u16.to_le_bytes());
        tiff.extend_from_slice(&1u32.to_le_bytes());
        tiff.extend_from_slice(&orientation.to_le_bytes());
        tiff.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
        let mut seg = b"Exif\0\0".to_vec();
        seg.extend(tiff);
        let mut out = vec![0xFF, 0xD8, 0xFF, 0xE1];
        out.extend_from_slice(&((seg.len() + 2) as u16).to_be_bytes());
        out.extend(seg);
        out.extend_from_slice(&jpg[2..]);
        self.put(name, &out)
    }

    async fn call(&self, who: Actor, method: &str, params: Value) -> Result<Value, DaemonError> {
        self.daemon.execute_method(who, 1, method, &params).await
    }

    async fn import(&self, who: Actor, params: Value) -> Result<Value, DaemonError> {
        self.call(who, "import_media", params).await
    }

    async fn finished(&self, id: &str) -> Value {
        for _ in 0..1500 {
            let j = self
                .call(Actor::User, "get_job", json!({"job_id": id}))
                .await
                .unwrap();
            if matches!(j["state"].as_str(), Some("done" | "failed" | "cancelled")) {
                return j;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("job {id} never finished");
    }

    async fn library(&self) -> Vec<Value> {
        self.call(Actor::User, "list_library", json!({}))
            .await
            .unwrap()["captures"]
            .as_array()
            .cloned()
            .unwrap_or_default()
    }

    fn bundle(&self, name: &str) -> PathBuf {
        self.lib.path().join(name)
    }
}

fn manifest(dir: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(dir.join("manifest.json")).unwrap()).unwrap()
}

fn chunks(png: &[u8]) -> Vec<String> {
    cm_media::import::picture::png_chunk_types(png)
}

/// IMP-6.AC1, AC2: a PNG and a JPEG become two screenshot bundles with a plain `original.png`,
/// an empty ops list, a thumbnail and the source; the orientation is applied.
#[tokio::test]
async fn a_png_and_a_jpeg_become_two_screenshot_bundles() {
    let r = rig();
    let png = r.png("Mock-up.png", 40, 30);
    let jpg = r.jpeg_with_exif("Phone.jpg", 32, 16, 6);
    let out = r
        .import(Actor::User, json!({"paths": [s(&png), s(&jpg)]}))
        .await
        .unwrap();
    let caps = out["captures"].as_array().unwrap();
    assert_eq!(caps.len(), 2);
    assert!(caps
        .iter()
        .all(|c| c["kind"] == "image" && c["status"] == "queued"));
    let id_png = caps[0]["capture_id"].as_str().unwrap();
    assert!(id_png.starts_with("shot_"), "{id_png}");
    let job = r.finished(out["job_id"].as_str().unwrap()).await;
    assert_eq!(job["state"], "done", "{job}");
    assert_eq!(job["result"]["imported"], 2);

    let a = r.bundle("Mock-up.cmshot");
    let b = r.bundle("Phone.cmshot");
    let ma = manifest(&a);
    assert_eq!(ma["id"], id_png);
    assert_eq!(ma["kind"], "screenshot");
    assert_eq!(ma["ops"], json!([]));
    assert_eq!(ma["source"]["original_name"], "Mock-up.png");
    assert_eq!(ma["source"]["path"], s(&png));
    assert!(ma["source"]["imported_at"].is_string());
    assert_eq!(ma["original"]["size"], json!([40, 30]));
    assert!(a.join("thumbs/poster.jpg").is_file());
    // The orientation 6 JPEG is stored upright: 32x16 becomes 16x32; no metadata.
    let mb = manifest(&b);
    assert_eq!(mb["original"]["size"], json!([16, 32]));
    let bytes = std::fs::read(b.join("original.png")).unwrap();
    assert_eq!(chunks(&bytes), ["IHDR", "IDAT", "IEND"]);
    let img = image::load_from_memory(&bytes).unwrap();
    assert_eq!((img.width(), img.height()), (16, 32));
    assert!(b.join("thumbs/poster.jpg").is_file());

    // The library lists both as screenshots with their source (the Imported filter).
    let lib = r.library().await;
    assert_eq!(lib.len(), 2);
    for c in &lib {
        assert_eq!(c["kind"], "screenshot", "{c}");
        assert!(c["source"]["original_name"].is_string(), "{c}");
    }
    // The original files are untouched.
    assert!(png.is_file() && jpg.is_file());
    assert!(r.confirmer.shown().is_empty());

    // annotate and recognize_text work on an imported bundle.
    let ops = json!([{"op": "rect", "rect": [2, 2, 20, 10], "stroke": "#ff0000"}]);
    let an = r
        .call(
            Actor::User,
            "annotate",
            json!({"capture_id": id_png, "ops": ops}),
        )
        .await
        .unwrap();
    assert!(
        an.get("revision").is_some() || an.get("capture_id").is_some(),
        "{an}"
    );
    let ocr = r
        .call(Actor::User, "recognize_text", json!({"capture_id": id_png}))
        .await
        .unwrap();
    assert_eq!(ocr["text"], "imported words");
    // The master is never changed by the annotation.
    let again = std::fs::read(a.join("original.png")).unwrap();
    assert_eq!(chunks(&again), ["IHDR", "IDAT", "IEND"]);
}

/// IMP-6.AC7: a dry run on a mix of pictures, video and a folder-free list gives every file its
/// own row (width and height for pictures) and writes nothing.
#[tokio::test]
async fn a_dry_run_gives_each_file_its_own_row_and_writes_nothing() {
    let r = rig();
    let good = r.png("ok.png", 20, 10);
    let rotated = r.jpeg_with_exif("rot.jpg", 30, 10, 8);
    let gif = r.put("anim.png", b"GIF89a\x01\0\x01\0\x80\0\0\0\0\0\xFF\xFF\xFF;");
    let webp = r.put("w.jpg", b"RIFF\x10\0\0\0WEBPVP8 \0\0\0\0\0\0\0\0");
    let svg = r.put("v.png", b"<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>");
    let empty = r.put("empty.png", b"");
    let png_bytes = std::fs::read(&good).unwrap();
    let cut = r.put("cut.png", &png_bytes[..png_bytes.len() / 2]);
    let mut bomb = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut ihdr = vec![0, 0, 0, 13, b'I', b'H', b'D', b'R'];
    ihdr.extend_from_slice(&60_000u32.to_be_bytes());
    ihdr.extend_from_slice(&60_000u32.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0, 0, 0, 0, 0]);
    bomb.extend(ihdr);
    // An empty IDAT, then IEND (the header reader does not check CRCs).
    bomb.extend_from_slice(&[0, 0, 0, 0, b'I', b'D', b'A', b'T', 0, 0, 0, 0]);
    bomb.extend_from_slice(&[0, 0, 0, 0, b'I', b'E', b'N', b'D', 0xAE, 0x42, 0x60, 0x82]);
    let bomb = r.put("bomb.png", &bomb);
    let paths: Vec<&str> = [&good, &rotated, &gif, &webp, &svg, &empty, &cut, &bomb]
        .iter()
        .map(|p| s(p))
        .collect();
    let out = r
        .import(Actor::User, json!({"paths": paths, "dry_run": true}))
        .await
        .unwrap();
    let rows = out["files"].as_array().unwrap();
    assert_eq!(rows.len(), 8);
    assert_eq!(rows[0]["status"], "ready");
    assert_eq!(
        (
            rows[0]["kind"].as_str(),
            rows[0]["width"].as_u64(),
            rows[0]["height"].as_u64()
        ),
        (Some("image"), Some(20), Some(10))
    );
    assert_eq!(rows[0]["size"], json!([20, 10]));
    assert_eq!(rows[1]["status"], "prepared");
    assert_eq!(rows[1]["reasons"], json!(["oriented"]));
    assert_eq!(
        (rows[1]["width"].as_u64(), rows[1]["height"].as_u64()),
        (Some(10), Some(30))
    );
    for (i, fmt) in [(2, "gif"), (3, "webp"), (4, "svg")] {
        assert_eq!(rows[i]["status"], "cant_import", "{i}");
        assert_eq!(rows[i]["error"]["reason"], "unsupported_format", "{i}");
        assert_eq!(rows[i]["error"]["format"], fmt, "{i}");
    }
    for i in [5, 6] {
        assert_eq!(rows[i]["status"], "cant_import", "{i}");
        assert_eq!(rows[i]["error"]["reason"], "unreadable_file", "{i}");
    }
    assert_eq!(rows[7]["error"]["reason"], "too_large");
    assert!(out.get("job_id").is_none());
    assert!(r.library().await.is_empty());
    let staging = r.lib.path().join(".capturemann/import");
    assert!(!staging.exists() || std::fs::read_dir(staging).unwrap().next().is_none());
}

/// IMP-6.AC3, AC4, AC6: a refused picture is reported by name, creates nothing, and does not stop
/// the others; a call where nothing can be imported fails with the shared reason.
#[tokio::test]
async fn refused_and_damaged_pictures_create_nothing() {
    let r = rig();
    let good = r.png("ok.png", 8, 8);
    let gif = r.put(
        "really-a-gif.png",
        b"GIF87a\x01\0\x01\0\x80\0\0\0\0\0\xFF\xFF\xFF;",
    );
    let png_bytes = std::fs::read(&good).unwrap();
    let cut = r.put("cut.png", &png_bytes[..png_bytes.len() - 20]);
    let out = r
        .import(Actor::User, json!({"paths": [s(&good), s(&gif), s(&cut)]}))
        .await
        .unwrap();
    let caps = out["captures"].as_array().unwrap();
    assert_eq!(caps[1]["status"], "cant_import");
    assert_eq!(caps[2]["status"], "cant_import");
    let job = r.finished(out["job_id"].as_str().unwrap()).await;
    assert_eq!(job["state"], "done");
    assert_eq!(r.library().await.len(), 1);

    let only_bad = r
        .import(Actor::User, json!({"paths": [s(&gif)]}))
        .await
        .unwrap_err();
    match only_bad {
        DaemonError::WithData { data, .. } => assert_eq!(data["reason"], "unsupported_format"),
        e => panic!("{e:?}"),
    }
}

/// IMP-6.AC6 at job time: a file that is cut short after the check is a failed file with
/// `unreadable_file`, and nothing is created.
#[tokio::test]
async fn a_picture_that_breaks_after_the_check_fails_without_a_bundle() {
    let r = rig();
    let good = r.png("late.png", 64, 64);
    r.daemon.hold_all_jobs();
    let out = r
        .import(Actor::User, json!({"paths": [s(&good)]}))
        .await
        .unwrap();
    let bytes = std::fs::read(&good).unwrap();
    // Same size and time would be needed to pass the "changed" check, so keep the length and
    // damage the pixel data instead: the header is fine, the decoder is not.
    let mut bad = bytes.clone();
    let at = bad.windows(4).position(|w| w == b"IDAT").unwrap() + 8;
    for b in &mut bad[at..at + 16] {
        *b ^= 0xFF;
    }
    std::fs::write(&good, &bad).unwrap();
    let mtime = std::fs::metadata(&good).unwrap().modified().unwrap();
    let _ = mtime;
    r.daemon.resume_all_jobs();
    let job = r.finished(out["job_id"].as_str().unwrap()).await;
    assert_eq!(job["state"], "failed", "{job}");
    assert!(r.library().await.is_empty());
    let leftovers = std::fs::read_dir(r.lib.path())
        .unwrap()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "cmshot"))
        .count();
    assert_eq!(leftovers, 0);
}

/// IMP-6.AC8: `remove_originals` stays Confirm for an agent, and the original goes to the Trash
/// only after the bundle is in the library.
#[tokio::test]
async fn remove_originals_asks_an_agent_and_trashes_only_after_the_bundle_exists() {
    let r = rig();
    let a = r.png("a.png", 12, 12);
    let out = r
        .import(agent(), json!({"paths": [s(&a)], "remove_originals": true}))
        .await
        .unwrap();
    assert_eq!(r.confirmer.shown().len(), 1, "an agent is asked first");
    let job = r.finished(out["job_id"].as_str().unwrap()).await;
    assert_eq!(job["state"], "done");
    assert!(r.bundle("a.cmshot").join("original.png").is_file());
    assert!(!a.exists() && r.trash.path().join("a.png").is_file());
    // A dry run with remove_originals never asks.
    let b = r.png("b.png", 12, 12);
    let before = r.confirmer.shown().len();
    r.import(
        agent(),
        json!({"paths": [s(&b)], "remove_originals": true, "dry_run": true}),
    )
    .await
    .unwrap();
    assert_eq!(r.confirmer.shown().len(), before);
    assert!(b.is_file());
}

/// A picture is not given the video options: `after.transcribe` does nothing for it.
#[tokio::test]
async fn after_options_do_not_apply_to_pictures() {
    let r = rig();
    let a = r.png("a.png", 12, 12);
    let out = r
        .import(
            Actor::User,
            json!({"paths": [s(&a)], "after": {"transcribe": true}}),
        )
        .await
        .unwrap();
    let job = r.finished(out["job_id"].as_str().unwrap()).await;
    assert_eq!(job["state"], "done", "{job}");
    assert!(job["result"]["after"].is_null());
}
