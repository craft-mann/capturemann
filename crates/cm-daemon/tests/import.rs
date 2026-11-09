//! IMP-3 and IMP-4: `import_media` and the `import` job through the real daemon, with fixture
//! files made by the `ffmpeg` on `PATH` (SKIPPED when it is missing). Scripted Trash, confirmer,
//! free space and AI engine; no screen, camera or microphone, no real Trash.
#![allow(clippy::unwrap_used)]

mod common;

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use cm_capture::disk::FakeFreeSpace;
use cm_core::{Actor, ErrorCode};
use cm_daemon::testing::{FakeAi, FakeTrasher, FixedConfirmer};
use cm_daemon::{Daemon, DaemonError, Decision, MemorySettings};
use common::{builder, desktop};
use serde_json::{json, Map, Value};

fn ff(args: &[&str]) -> bool {
    Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y"])
        .args(args)
        .stdin(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn have_ffmpeg() -> bool {
    let ok = Command::new("ffmpeg")
        .args(["-hide_banner", "-encoders"])
        .output()
        .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains("libx264"))
        && cm_media::tools::locate().is_ok_and(|t| t.ffprobe.is_some());
    if !ok {
        eprintln!("SKIPPED: FFmpeg with libx264 and ffprobe not found (brew install ffmpeg)");
    }
    ok
}

struct Rig {
    daemon: Daemon,
    confirmer: Arc<FixedConfirmer>,
    trasher: Arc<FakeTrasher>,
    free: Arc<FakeFreeSpace>,
    ai: FakeAi,
    lib: tempfile::TempDir,
    src: tempfile::TempDir,
    trash: tempfile::TempDir,
}

struct Free(Arc<FakeFreeSpace>);
impl cm_capture::disk::FreeSpace for Free {
    fn free_bytes(&self, p: &Path) -> Option<u64> {
        cm_capture::disk::FreeSpace::free_bytes(&*self.0, p)
    }
}

fn rig(decision: Decision, ai: FakeAi) -> Rig {
    let (b, lib) = builder(desktop());
    let trash = tempfile::tempdir().unwrap();
    let confirmer = Arc::new(FixedConfirmer::new(decision));
    let trasher = Arc::new(FakeTrasher::into_dir(trash.path()));
    let free = Arc::new(FakeFreeSpace::new(u64::MAX));
    let mut defaults = Map::new();
    // A hand-edited policy that tries to let a remove-originals call through without asking.
    defaults.insert(
        "agents.tool_policy".into(),
        json!({ "import_media": "allow" }),
    );
    let daemon = b
        .confirmer(confirmer.clone())
        .trasher(trasher.clone())
        .settings(MemorySettings::with_defaults(defaults))
        .free_space(Free(free.clone()))
        .ai(ai.services())
        .jobs(cm_daemon::library_job_queue(lib.path()).unwrap())
        .build();
    daemon.register_import_handler().unwrap();
    daemon.register_ai_handlers().unwrap();
    daemon.start_jobs().unwrap();
    Rig {
        daemon,
        confirmer,
        trasher,
        free,
        ai,
        lib,
        src: tempfile::tempdir().unwrap(),
        trash,
    }
}

impl Rig {
    fn mp4(&self, name: &str) -> PathBuf {
        let p = self.src.path().join(name);
        assert!(ff(&[
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=320x240:rate=30:duration=2",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=44100:duration=2",
            "-ac",
            "2",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            "-shortest",
            p.to_str().unwrap(),
        ]));
        p
    }

    fn wav(&self, name: &str) -> PathBuf {
        let p = self.src.path().join(name);
        assert!(ff(&[
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=2",
            p.to_str().unwrap()
        ]));
        p
    }

    fn flac(&self, name: &str) -> PathBuf {
        let p = self.src.path().join(name);
        assert!(ff(&[
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=1",
            p.to_str().unwrap()
        ]));
        p
    }

    async fn call(&self, who: Actor, params: Value) -> Result<Value, DaemonError> {
        self.daemon
            .execute_method(who, 1, "import_media", &params)
            .await
    }
}

fn agent() -> Actor {
    Actor::agent("claude-desktop").unwrap()
}

fn s(p: &Path) -> &str {
    p.to_str().unwrap()
}

fn reason(e: &DaemonError) -> String {
    match e {
        DaemonError::WithData { data, .. } => data["reason"].as_str().unwrap_or("").to_owned(),
        _ => String::new(),
    }
}

async fn finished(d: &Daemon, id: &str) -> Value {
    for _ in 0..1500 {
        let j = d
            .execute_method(Actor::User, 0, "get_job", &json!({"job_id": id}))
            .await
            .unwrap();
        if matches!(j["state"].as_str(), Some("done" | "failed" | "cancelled")) {
            return j;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("job {id} never finished");
}

fn manifest(dir: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(dir.join("manifest.json")).unwrap()).unwrap()
}

async fn library(d: &Daemon) -> Vec<Value> {
    d.execute_method(Actor::User, 0, "list_library", &json!({}))
        .await
        .unwrap()["captures"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

#[tokio::test]
async fn a_dry_run_classifies_and_starts_nothing() {
    if !have_ffmpeg() {
        return;
    }
    let r = rig(Decision::Approve, FakeAi::new());
    let ok = r.mp4("a.mp4");
    let mkv = r.src.path().join("b.mkv");
    assert!(ff(&["-i", s(&ok), "-c", "copy", s(&mkv)]));
    let flac = r.flac("c.flac");
    let out = r
        .call(
            Actor::User,
            json!({"paths": [s(&ok), s(&mkv), s(&flac)], "dry_run": true}),
        )
        .await
        .unwrap();
    let rows = out["files"].as_array().unwrap();
    assert_eq!(rows[0]["status"], "ready");
    assert_eq!(rows[1]["status"], "prepared");
    assert_eq!(rows[1]["reasons"], json!(["container_mkv"]));
    assert_eq!(rows[2]["status"], "cant_import");
    assert_eq!(rows[2]["error"]["reason"], "unsupported_format");
    assert!(out.get("job_id").is_none());
    assert!(library(&r.daemon).await.is_empty());
    assert!(r.ok_no_staging());
}

impl Rig {
    fn ok_no_staging(&self) -> bool {
        let dir = self.lib.path().join(".capturemann/import");
        !dir.exists() || std::fs::read_dir(dir).unwrap().next().is_none()
    }
}

#[tokio::test]
async fn a_video_and_an_audio_file_become_captures() {
    if !have_ffmpeg() {
        return;
    }
    let r = rig(Decision::Approve, FakeAi::new());
    let (video, audio) = (r.mp4("Phone clip.mp4"), r.wav("Memo.wav"));
    let mut events = r.daemon.subscribe();
    let out = r
        .call(
            Actor::User,
            json!({"paths": [s(&video), s(&audio)], "titles": ["", "Voice memo"]}),
        )
        .await
        .unwrap();
    // At once: the job and the reserved ids.
    let caps = out["captures"].as_array().unwrap().clone();
    assert_eq!(caps.len(), 2);
    assert_eq!(caps[0]["title"], "Phone clip");
    assert_eq!(caps[1]["title"], "Voice memo");
    assert_eq!(caps[0]["status"], "queued");
    let ids: Vec<String> = caps
        .iter()
        .map(|c| c["capture_id"].as_str().unwrap().to_owned())
        .collect();
    let job = finished(&r.daemon, out["job_id"].as_str().unwrap()).await;
    assert_eq!(job["state"], "done", "{job}");
    assert_eq!(job["result"]["imported"], 2);
    assert_eq!(job["result"]["files"][0]["capture_id"], json!(ids[0]));
    // The library.
    let lib = library(&r.daemon).await;
    assert_eq!(lib.len(), 2, "{lib:?}");
    let by_id = |id: &str| {
        lib.iter()
            .find(|c| c["id"] == id || c["capture_id"] == id)
            .cloned()
            .unwrap()
    };
    let (v, a) = (by_id(&ids[0]), by_id(&ids[1]));
    assert_eq!(v["kind"], "recording");
    assert_eq!(a["kind"], "audio");
    // The video bundle: the file as it was, the sound, the source, no cursor or camera.
    let vdir = r.lib.path().join("Phone clip.cmrec");
    assert_eq!(
        std::fs::read(vdir.join("screen.mp4")).unwrap(),
        std::fs::read(&video).unwrap()
    );
    let m = manifest(&vdir);
    assert_eq!(m["id"], json!(ids[0]));
    assert_eq!(m["tracks"]["screen"]["size"], json!([320, 240]));
    assert_eq!(m["tracks"]["mic"]["file"], "mic.wav");
    assert!(m["tracks"].get("cursor").is_none() && m["tracks"].get("camera").is_none());
    assert_eq!(m["source"]["original_name"], "Phone clip.mp4");
    assert_eq!(m["source"]["path"], s(&video));
    assert_eq!(m["source"]["original_file"], "screen.mp4");
    assert_eq!(m["revisions"][0]["action"], "import");
    // 48 kHz, 32-bit float: the RIFF header's format tag, rate and bit depth. FFmpeg 6.x (Ubuntu 24.04)
    // writes 32-bit float as WAVE_FORMAT_EXTENSIBLE (0xFFFE) whose sub-format GUID starts with the
    // float tag (3); newer FFmpeg writes the plain tag 3. Both are float; the rate and the bit depth
    // sit at the same offsets in both layouts.
    let head = std::fs::read(vdir.join("mic.wav")).unwrap();
    assert_eq!(&head[0..4], b"RIFF");
    let tag = u16::from_le_bytes([head[20], head[21]]);
    let float_tag = if tag == 0xFFFE {
        // fmt chunk body starts at 20; cbSize 2, valid bits 2, channel mask 4, then the GUID.
        u16::from_le_bytes([head[44], head[45]])
    } else {
        tag
    };
    assert_eq!(float_tag, 3, "format tag {tag:#x}");
    assert_eq!(
        u32::from_le_bytes([head[24], head[25], head[26], head[27]]),
        48_000
    );
    assert_eq!(u16::from_le_bytes([head[34], head[35]]), 32);
    // The audio bundle.
    let adir = r.lib.path().join("Voice memo.cmrec");
    let am = manifest(&adir);
    assert_eq!(am["kind"], "audio");
    assert!(am["tracks"].get("screen").is_none());
    assert!(adir.join("original.wav").is_file() && adir.join("mic.wav").is_file());
    // The originals are untouched, nothing is left in staging, capture.created came per file.
    assert!(video.is_file() && audio.is_file());
    assert!(r.ok_no_staging());
    let mut created = 0;
    while let Ok(e) = events.try_recv() {
        if e.name == "capture.created" && e.data["import"] == true {
            created += 1;
            assert_eq!(e.data["job_id"], out["job_id"]);
        }
    }
    assert_eq!(created, 2);
}

#[tokio::test]
async fn folders_nothing_importable_and_a_full_disk_are_refused_by_name() {
    if !have_ffmpeg() {
        return;
    }
    let r = rig(Decision::Approve, FakeAi::new());
    let ok = r.mp4("a.mp4");
    // A folder anywhere in the list: nothing is imported.
    let e = r
        .call(Actor::User, json!({"paths": [s(&ok), s(r.src.path())]}))
        .await
        .unwrap_err();
    assert_eq!(e.code(), Some(ErrorCode::InvalidInput));
    assert_eq!(reason(&e), "folder_not_supported");
    assert!(library(&r.daemon).await.is_empty());
    // A missing file.
    let e = r
        .call(
            Actor::User,
            json!({"paths": [s(&r.src.path().join("nope.mov"))]}),
        )
        .await
        .unwrap_err();
    assert_eq!(e.code(), Some(ErrorCode::NotFound));
    // Only unsupported files.
    let flac = r.flac("c.flac");
    let e = r
        .call(Actor::User, json!({"paths": [s(&flac)]}))
        .await
        .unwrap_err();
    assert_eq!(e.code(), Some(ErrorCode::InvalidInput));
    assert_eq!(reason(&e), "unsupported_format");
    // A broken file next to an unsupported one: no single reason.
    let junk = r.src.path().join("junk.mov");
    std::fs::write(&junk, b"not a movie").unwrap();
    let e = r
        .call(Actor::User, json!({"paths": [s(&flac), s(&junk)]}))
        .await
        .unwrap_err();
    assert_eq!(reason(&e), "nothing_to_import");
    // Not enough room.
    r.free.set(1_500_000);
    let e = r
        .call(Actor::User, json!({"paths": [s(&ok)]}))
        .await
        .unwrap_err();
    assert_eq!(e.code(), Some(ErrorCode::DiskFull));
    match &e {
        DaemonError::WithData { data, .. } => {
            assert!(data["needed_bytes"].as_u64().unwrap() > 0);
            assert_eq!(data["free_bytes"], 1_500_000);
        }
        other => panic!("{other:?}"),
    }
    // A file inside the library.
    r.free.set(u64::MAX);
    let inside = r.lib.path().join("x.mp4");
    std::fs::copy(&ok, &inside).unwrap();
    let e = r
        .call(Actor::User, json!({"paths": [s(&inside)]}))
        .await
        .unwrap_err();
    assert_eq!(reason(&e), "inside_library");
    assert!(library(&r.daemon).await.is_empty());
}

#[tokio::test]
async fn one_bad_file_does_not_stop_the_others() {
    if !have_ffmpeg() {
        return;
    }
    let r = rig(Decision::Approve, FakeAi::new());
    let (ok, flac) = (r.mp4("a.mp4"), r.flac("c.flac"));
    let out = r
        .call(Actor::User, json!({"paths": [s(&flac), s(&ok)]}))
        .await
        .unwrap();
    assert_eq!(out["captures"][0]["status"], "cant_import");
    assert_eq!(out["captures"][0]["error"]["reason"], "unsupported_format");
    assert_eq!(out["captures"][1]["status"], "queued");
    let job = finished(&r.daemon, out["job_id"].as_str().unwrap()).await;
    assert_eq!(job["state"], "done");
    assert_eq!(library(&r.daemon).await.len(), 1);
}

#[tokio::test]
async fn a_repeated_request_id_imports_once() {
    if !have_ffmpeg() {
        return;
    }
    let r = rig(Decision::Approve, FakeAi::new());
    let ok = r.mp4("a.mp4");
    let p = json!({"paths": [s(&ok)], "request_id": "imp-1"});
    let first = r.call(Actor::User, p.clone()).await.unwrap();
    let again = r.call(Actor::User, p).await.unwrap();
    assert_eq!(first["job_id"], again["job_id"]);
    assert_eq!(first["captures"], again["captures"]);
    finished(&r.daemon, first["job_id"].as_str().unwrap()).await;
    assert_eq!(library(&r.daemon).await.len(), 1);
}

#[tokio::test]
async fn a_prepared_video_keeps_its_original_and_gets_an_editing_copy() {
    if !have_ffmpeg() {
        return;
    }
    let enc = cm_media::tools::locate()
        .ok()
        .and_then(|t| cm_media::import::prepare::editing_copy_encoders(&t, "h264").ok())
        .is_some_and(|(ladder, _, _)| !ladder.is_empty());
    if !enc {
        eprintln!("SKIPPED: no hardware video encoder");
        return;
    }
    let r = rig(Decision::Approve, FakeAi::new());
    let ok = r.mp4("a.mp4");
    let mkv = r.src.path().join("clip.mkv");
    assert!(ff(&["-i", s(&ok), "-c", "copy", s(&mkv)]));
    let out = r
        .call(Actor::User, json!({"paths": [s(&mkv)]}))
        .await
        .unwrap();
    assert_eq!(out["captures"][0]["status"], "queued");
    let job = finished(&r.daemon, out["job_id"].as_str().unwrap()).await;
    // `ffmpeg -encoders` lists a Media Foundation encoder on a machine whose hardware transform cannot
    // start (a CI runner with no GPU); the product then says so by name, which is the right answer, and
    // there is nothing to check here.
    if job["state"] == "failed"
        && job["error"]["reason"]
            .as_str()
            .is_some_and(|r| r.contains("no hardware video encoder"))
    {
        eprintln!("SKIPPED: the listed video encoder cannot start on this machine");
        return;
    }
    if job["state"] != "done" {
        panic!("{job}");
    }
    let dir = r.lib.path().join("clip.cmrec");
    let m = manifest(&dir);
    assert_eq!(m["tracks"]["screen"]["file"], "screen.edit.mp4");
    assert_eq!(m["tracks"]["screen"]["master"], "original.mkv");
    assert_eq!(m["tracks"]["screen"]["prepared"], json!(["container_mkv"]));
    assert_eq!(m["source"]["original_file"], "original.mkv");
    assert_eq!(
        std::fs::read(dir.join("original.mkv")).unwrap(),
        std::fs::read(&mkv).unwrap()
    );
    assert!(dir.join("screen.edit.mp4").is_file());
}

// ---- IMP-4: remove the originals ----------------------------------------------------------

#[tokio::test]
async fn an_agent_removing_originals_is_asked_with_every_file_listed_then_they_go_to_the_trash() {
    if !have_ffmpeg() {
        return;
    }
    let r = rig(Decision::Approve, FakeAi::new());
    let (a, b, flac) = (r.mp4("a.mp4"), r.wav("b.wav"), r.flac("c.flac"));
    let out = r
        .call(
            agent(),
            json!({"paths": [s(&a), s(&b), s(&flac)], "remove_originals": true}),
        )
        .await
        .unwrap();
    // The dialog was shown despite the hand-edited `allow`, and listed every file.
    let shown = r.confirmer.shown();
    assert_eq!(shown.len(), 1);
    let d = &shown[0];
    assert_eq!(d.kind, "import_remove_originals");
    assert_eq!(d.tool, "import_media");
    assert_eq!(d.params["trash_count"], 2);
    let files = d.params["files"].as_array().unwrap();
    assert_eq!(files.len(), 2);
    assert_eq!(files[0]["path"], s(&a));
    assert_eq!(files[1]["path"], s(&b));
    assert_eq!(d.params["not_imported"][0]["path"], s(&flac));
    assert_eq!(d.params["not_imported"][0]["reason"], "unsupported_format");
    assert!(
        d.summary.contains("2 files") && d.summary.contains("Trash"),
        "{}",
        d.summary
    );
    let rows = d.details.as_object().unwrap();
    assert!(
        rows.contains_key("to_trash_1")
            && rows.contains_key("to_trash_2")
            && rows.contains_key("stays_1")
    );
    // The job imports and then trashes.
    let job = finished(&r.daemon, out["job_id"].as_str().unwrap()).await;
    assert_eq!(job["state"], "done", "{job}");
    assert!(
        !a.exists() && !b.exists(),
        "the originals left their folder"
    );
    assert!(flac.exists(), "an unsupported file stays where it is");
    assert_eq!(job["result"]["trashed"].as_array().unwrap().len(), 2);
    assert!(r.trash.path().join("a.mp4").is_file());
    assert_eq!(library(&r.daemon).await.len(), 2);
    // The copies are complete (the trash step only runs after the install).
    assert!(r.lib.path().join("a.cmrec/screen.mp4").is_file());
}

#[tokio::test]
async fn declining_the_dialog_imports_nothing_and_touches_no_file() {
    if !have_ffmpeg() {
        return;
    }
    let r = rig(Decision::Decline, FakeAi::new());
    let a = r.mp4("a.mp4");
    let e = r
        .call(agent(), json!({"paths": [s(&a)], "remove_originals": true}))
        .await
        .unwrap_err();
    assert_eq!(e.code(), Some(ErrorCode::UserDeclined));
    assert!(a.is_file());
    assert!(library(&r.daemon).await.is_empty());
    assert!(r.trasher.trashed().is_empty());
}

#[tokio::test]
async fn the_user_is_not_asked_and_a_dry_run_never_is() {
    if !have_ffmpeg() {
        return;
    }
    let r = rig(Decision::Decline, FakeAi::new());
    let (a, b) = (r.mp4("a.mp4"), r.mp4("b.mp4"));
    // An agent's dry run with remove_originals: no dialog, nothing moved.
    let out = r
        .call(
            agent(),
            json!({"paths": [s(&a)], "remove_originals": true, "dry_run": true}),
        )
        .await
        .unwrap();
    assert_eq!(out["files"][0]["status"], "ready");
    assert!(r.confirmer.shown().is_empty() && a.is_file());
    // The app's own call (the checkbox is the consent): no dialog, the original goes to the Trash.
    let out = r
        .call(
            Actor::User,
            json!({"paths": [s(&b)], "remove_originals": true}),
        )
        .await
        .unwrap();
    assert!(r.confirmer.shown().is_empty());
    let job = finished(&r.daemon, out["job_id"].as_str().unwrap()).await;
    assert_eq!(job["state"], "done");
    assert!(!b.exists() && r.trash.path().join("b.mp4").is_file());
    // Without remove_originals an agent is not asked either (Free), and the file stays.
    let out = r.call(agent(), json!({"paths": [s(&a)]})).await.unwrap();
    assert!(r.confirmer.shown().is_empty());
    finished(&r.daemon, out["job_id"].as_str().unwrap()).await;
    assert!(a.is_file());
}

#[tokio::test]
async fn a_file_that_changed_after_the_dialog_is_neither_imported_nor_trashed() {
    if !have_ffmpeg() {
        return;
    }
    let r = rig(Decision::Approve, FakeAi::new());
    let (a, b) = (r.mp4("a.mp4"), r.mp4("b.mp4"));
    // Hold the queue so the file can change between the call and the copy.
    r.daemon.hold_all_jobs();
    let out = r
        .call(
            agent(),
            json!({"paths": [s(&a), s(&b)], "remove_originals": true}),
        )
        .await
        .unwrap();
    let mut bytes = std::fs::read(&a).unwrap();
    bytes.extend_from_slice(b"changed");
    std::fs::write(&a, bytes).unwrap();
    r.daemon.resume_all_jobs();
    let job = finished(&r.daemon, out["job_id"].as_str().unwrap()).await;
    assert_eq!(job["state"], "done", "{job}");
    let files = job["result"]["files"].as_array().unwrap();
    assert_eq!(files[0]["status"], "failed");
    assert_eq!(files[0]["error"]["reason"], "changed_since_approval");
    assert_eq!(files[1]["status"], "imported");
    assert!(a.is_file(), "the changed file stays");
    assert!(!b.exists());
    assert_eq!(library(&r.daemon).await.len(), 1);
}

#[tokio::test]
async fn a_trash_that_fails_keeps_the_capture_and_the_original() {
    if !have_ffmpeg() {
        return;
    }
    let r = rig(Decision::Approve, FakeAi::new());
    let a = r.mp4("a.mp4");
    r.trasher.fail_from_now_on();
    let out = r
        .call(
            Actor::User,
            json!({"paths": [s(&a)], "remove_originals": true}),
        )
        .await
        .unwrap();
    let job = finished(&r.daemon, out["job_id"].as_str().unwrap()).await;
    assert_eq!(job["state"], "done");
    assert_eq!(job["result"]["files"][0]["status"], "imported");
    assert_eq!(job["result"]["files"][0]["trash"]["state"], "failed");
    assert!(a.is_file());
    assert_eq!(library(&r.daemon).await.len(), 1);
}

#[tokio::test]
async fn a_cancelled_import_leaves_no_capture_and_the_original() {
    if !have_ffmpeg() {
        return;
    }
    let r = rig(Decision::Approve, FakeAi::new());
    let a = r.mp4("a.mp4");
    r.daemon.hold_all_jobs();
    let out = r
        .call(
            Actor::User,
            json!({"paths": [s(&a)], "remove_originals": true}),
        )
        .await
        .unwrap();
    let id = out["job_id"].as_str().unwrap();
    r.daemon
        .execute_method(Actor::User, 0, "cancel_job", &json!({"job_id": id}))
        .await
        .unwrap();
    r.daemon.resume_all_jobs();
    tokio::time::sleep(Duration::from_millis(400)).await;
    let job = finished(&r.daemon, id).await;
    assert_eq!(job["state"], "cancelled");
    assert!(library(&r.daemon).await.is_empty());
    assert!(a.is_file() && r.ok_no_staging());
}

#[tokio::test]
async fn a_capture_that_exists_is_not_imported_twice_when_a_job_runs_again() {
    if !have_ffmpeg() {
        return;
    }
    let r = rig(Decision::Approve, FakeAi::new());
    let a = r.mp4("a.mp4");
    let out = r
        .call(Actor::User, json!({"paths": [s(&a)]}))
        .await
        .unwrap();
    finished(&r.daemon, out["job_id"].as_str().unwrap()).await;
    let id = out["captures"][0]["capture_id"]
        .as_str()
        .unwrap()
        .to_owned();
    // A job for the same file and reserved id, as a restart after a crash between the move into
    // the library and the checkpoint would run it.
    let meta = std::fs::metadata(&a).unwrap();
    let mtime = meta
        .modified()
        .unwrap()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let q = r.daemon.job_queue().unwrap();
    let job = q
        .enqueue(cm_jobs::NewJob::new(
            cm_jobs::JOB_IMPORT,
            json!({"files": [{"path": s(&a), "name": "a.mp4", "size_bytes": meta.len(),
                              "mtime_ms": mtime, "title": "a", "kind": "video",
                              "capture_id": id, "status": "ready", "prepared": []}],
                   "remove_originals": true, "after": null, "by": "user", "total_bytes": meta.len()}),
        ))
        .unwrap();
    let done = finished(&r.daemon, &job.id).await;
    assert_eq!(done["state"], "done", "{done}");
    assert_eq!(done["result"]["files"][0]["status"], "imported");
    assert_eq!(library(&r.daemon).await.len(), 1, "no duplicate");
    // The original that was left behind by the crash is trashed now.
    assert!(!a.exists());
}

// ---- after: the existing AI actions ------------------------------------------------------

/// The import ends when the files are in the library. `after.transcribe` queues the ordinary
/// `transcribe` job of each capture and does not wait for it: with transcription held, the import
/// job is `done` while the transcribe job is still queued, and it is a job of its own.
#[tokio::test]
async fn after_transcribe_queues_an_independent_job_and_the_import_ends_first() {
    if !have_ffmpeg() {
        return;
    }
    let r = rig(Decision::Approve, FakeAi::new().with_small_installed());
    r.daemon.hold_job_type("transcribe");
    let (a, b) = (r.wav("one.wav"), r.wav("two.wav"));
    let out = r
        .call(
            Actor::User,
            json!({"paths": [s(&a), s(&b)], "after": {"transcribe": true}}),
        )
        .await
        .unwrap();
    let job = finished(&r.daemon, out["job_id"].as_str().unwrap()).await;
    assert_eq!(job["state"], "done", "{job}");
    assert_eq!(job["result"]["imported"], 2);
    let ids: Vec<String> = out["captures"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["capture_id"].as_str().unwrap().to_owned())
        .collect();
    // The result names the queued job of each capture.
    let mut queued = Vec::new();
    for id in &ids {
        let t = &job["result"]["after"][id]["transcribe"];
        assert_eq!(t["state"], "queued", "{job}");
        queued.push(t["job_id"].as_str().unwrap().to_owned());
    }
    // They are real, separate jobs, still waiting (held), not part of the import.
    let list = r
        .daemon
        .execute_method(Actor::User, 0, "list_jobs", &json!({"type": "transcribe"}))
        .await
        .unwrap();
    let jobs = list["jobs"].as_array().unwrap();
    assert_eq!(jobs.len(), 2, "{list}");
    for j in jobs {
        assert!(queued.contains(&j["job_id"].as_str().unwrap().to_owned()));
        assert_ne!(j["state"], "done");
    }
    // Released, each runs on its own and the transcript appears.
    r.daemon.resume_job_type("transcribe");
    for q in &queued {
        assert_eq!(finished(&r.daemon, q).await["state"], "done");
    }
    assert!(r.lib.path().join("one.cmrec/transcript.json").is_file());
    assert_eq!(r.ai.transcribe_calls().len(), 2);
}

#[tokio::test]
async fn after_transcribe_without_a_model_is_skipped_and_the_import_is_unaffected() {
    if !have_ffmpeg() {
        return;
    }
    let r = rig(Decision::Approve, FakeAi::new());
    let a = r.wav("talk.wav");
    let out = r
        .call(
            Actor::User,
            json!({"paths": [s(&a)], "after": {"transcribe": true}}),
        )
        .await
        .unwrap();
    let job = finished(&r.daemon, out["job_id"].as_str().unwrap()).await;
    assert_eq!(job["state"], "done");
    let id = out["captures"][0]["capture_id"].as_str().unwrap();
    assert_eq!(job["result"]["after"][id]["transcribe"]["state"], "skipped");
    assert_eq!(
        job["result"]["after"][id]["transcribe"]["reason"],
        "no_model"
    );
    assert_eq!(library(&r.daemon).await.len(), 1);
}

#[tokio::test]
async fn actions_list_says_which_parameter_asks_first() {
    let r = rig(Decision::Approve, FakeAi::new());
    let list = r.daemon.actions_effective();
    assert!(list.iter().any(|(s, _, _)| s.name == "import_media"));
    let (b, _) = (builder(desktop()).0, ());
    let _ = b;
    assert_eq!(
        cm_daemon::action::confirm_when("import_media"),
        Some(json!({"remove_originals": true}))
    );
    assert_eq!(cm_daemon::action::confirm_when("trim"), None);
}

/// MP3 import (0.3.1, MP3-1): the title tag names the capture unless the caller gave a title, the
/// sound is decoded to `mic.wav`, and the sheet's row says what the import will write.
#[tokio::test]
async fn an_mp3_imports_as_audio_titled_by_its_tag_and_the_row_counts_the_decoded_sound() {
    if !have_ffmpeg() {
        return;
    }
    if !Command::new("ffmpeg")
        .args(["-hide_banner", "-encoders"])
        .output()
        .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains("libmp3lame"))
    {
        eprintln!("SKIPPED: the ffmpeg on PATH has no libmp3lame (fixtures)");
        return;
    }
    let r = rig(Decision::Approve, FakeAi::new());
    let tagged = r.src.path().join("take 3.mp3");
    let plain = r.src.path().join("Plain file.mp3");
    for (p, tag) in [(&tagged, Some("Board meeting")), (&plain, None)] {
        let mut a = vec![
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=44100:duration=2",
            "-c:a",
            "libmp3lame",
        ];
        let meta = tag.map(|t| format!("title={t}"));
        if let Some(m) = &meta {
            a.extend(["-metadata", m.as_str()]);
        }
        a.push(s(p));
        assert!(ff(&a));
    }
    let dry = r
        .call(
            Actor::User,
            json!({"paths": [s(&tagged), s(&plain)], "dry_run": true}),
        )
        .await
        .unwrap();
    let rows = dry["files"].as_array().unwrap();
    assert_eq!(rows[0]["status"], "ready");
    assert_eq!(rows[0]["kind"], "audio");
    assert_eq!(rows[0]["title"], "Board meeting");
    assert_eq!(rows[1]["title"], "Plain file");
    // The decoded sound: 2 s of 48 kHz 32-bit float mono is 384 000 bytes, on top of the copy.
    let size = rows[0]["size_bytes"].as_u64().unwrap();
    assert_eq!(rows[0]["needed_bytes"].as_u64().unwrap(), size + 384_000);
    // An explicit title wins over the tag.
    let out = r
        .call(
            Actor::User,
            json!({"paths": [s(&tagged), s(&plain)], "titles": ["My title", ""]}),
        )
        .await
        .unwrap();
    assert_eq!(out["captures"][0]["title"], "My title");
    assert_eq!(out["captures"][1]["title"], "Plain file");
    let job = finished(&r.daemon, out["job_id"].as_str().unwrap()).await;
    assert_eq!(job["state"], "done", "{job}");
    let dir = r.lib.path().join("My title.cmrec");
    let m = manifest(&dir);
    assert_eq!(m["kind"], "audio");
    assert!(dir.join("mic.wav").is_file());
    assert_eq!(m["source"]["original_name"], "take 3.mp3");
    let master = m["source"]["original_file"].as_str().unwrap();
    assert!(std::path::Path::new(master)
        .extension()
        .is_some_and(|e| e == "mp3"));
    assert!(dir.join(master).is_file());
}
