//! STO-5 edge case: an original-bundle upload whose files change while the job runs. At the end
//! of the job the changed files (at least `manifest.json`) and the new ones are sent again into
//! the same claimed folder, replacing the remote copy, at most three rounds. Everything runs
//! against the fake Google Drive and a temp folder: no network, no account. Tested with fakes.

#![allow(clippy::unwrap_used)]

mod common;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use cm_core::{Actor, MediaTime, TrackInfo, TrackKind, TrackStats};
use cm_daemon::storage_cloud::{AccountTokens, CloudBackend};
use cm_daemon::{Daemon, DaemonError};
use cm_library::rec::{RecBundle, SessionInfo};
use cm_storage::fake_drive::{Call as DriveCall, FakeDrive, FakeTokens, Fault as DriveFault};
use cm_storage::{
    AutoUpload, CloudAccount, Destination, DestinationStore, Kind, LinkMode, TokenSource, What,
    DEFAULT_FOLDER_PATTERN,
};
use common::{builder, desktop};
use serde_json::{json, Map, Value};
use time::macros::datetime;

const GDRIVE: &str = "gdrive-1";

struct Shared(Arc<FakeTokens>);

#[async_trait]
impl TokenSource for Shared {
    async fn access_token(&self, rejected: Option<&str>) -> Result<String, cm_core::CmError> {
        self.0.access_token(rejected).await
    }
}

struct Tokens(Arc<FakeTokens>);

impl AccountTokens for Tokens {
    fn source(&self, _account: &str) -> Arc<dyn TokenSource> {
        Arc::new(Shared(self.0.clone()))
    }
}

struct Rig {
    daemon: Daemon,
    drive: Arc<FakeDrive>,
    id: String,
    dir: PathBuf,
    nas: PathBuf,
    _cfg: tempfile::TempDir,
    _lib: tempfile::TempDir,
}

fn drive_dest() -> Destination {
    Destination {
        id: "drive".into(),
        name: "School Drive".into(),
        kind: Kind::GoogleDrive,
        path: PathBuf::new(),
        account: Some(CloudAccount {
            account: GDRIVE.into(),
            label: "teacher@example.org".into(),
            folder_id: None,
            folder_name: "CaptureMann".into(),
            link_mode: LinkMode::None,
            app_folder: false,
        }),
        folder_pattern: DEFAULT_FOLDER_PATTERN.into(),
        what: What::Original,
        auto_upload: AutoUpload::Off,
        default: false,
    }
}

fn nas_dest(path: &Path) -> Destination {
    Destination {
        id: "nas".into(),
        name: "Office NAS".into(),
        kind: Kind::Local,
        path: path.to_path_buf(),
        account: None,
        folder_pattern: DEFAULT_FOLDER_PATTERN.into(),
        what: What::Original,
        auto_upload: AutoUpload::Off,
        default: true,
    }
}

async fn rig() -> Rig {
    let cfg = tempfile::tempdir().unwrap();
    let nas = cfg.path().join("nas");
    std::fs::create_dir(&nas).unwrap();
    let dests = DestinationStore::new(cfg.path().join("config"));
    dests.save(&[nas_dest(&nas), drive_dest()]).unwrap();
    let drive = Arc::new(FakeDrive::new());
    let tokens = Arc::new(FakeTokens::new("tok-1"));

    let (b, lib) = builder(desktop());
    let queue = cm_daemon::library_job_queue(lib.path()).unwrap();
    let daemon = b
        .jobs(queue)
        .destinations(dests)
        .cloud_backend(CloudBackend {
            tokens: Arc::new(Tokens(tokens)),
            http: drive.clone(),
        })
        .build();
    daemon
        .register_job_handler(
            cm_jobs::JOB_UPLOAD,
            daemon.upload_handler(Duration::from_millis(20)),
        )
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
    std::fs::write(rec.dir().join("screen.mp4"), vec![7u8; 300_000]).unwrap();
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
        drive,
        id: m.id,
        dir,
        nas,
        _cfg: cfg,
        _lib: lib,
    }
}

async fn call(r: &Rig, method: &str, p: Value) -> Result<Value, DaemonError> {
    r.daemon.execute_method(Actor::User, 0, method, &p).await
}

async fn wait_done(r: &Rig, job_id: &str) -> Value {
    for _ in 0..1500 {
        let j = call(r, "get_job", json!({ "job_id": job_id }))
            .await
            .unwrap();
        if matches!(j["state"].as_str(), Some("done" | "failed" | "cancelled")) {
            return j;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("the upload job did not finish");
}

async fn start(r: &Rig, destination: &str, extra: Value) -> String {
    let mut p = json!({ "destination_id": destination, "what": "original" });
    p.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    if p.get("file").is_none() {
        p["capture_id"] = json!(r.id);
    }
    let v = call(r, "upload", p).await.unwrap();
    v["job_id"].as_str().unwrap().to_owned()
}

/// Every file the plan sends, relative to the bundle (what the walk would list).
fn bundle_files(dir: &Path) -> Vec<String> {
    fn rec(dir: &Path, root: &Path, out: &mut Vec<String>) {
        let mut es: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap())
            .collect();
        es.sort_by_key(|e| e.file_name());
        for e in es {
            let p = e.path();
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
    let mut v = Vec::new();
    rec(dir, dir, &mut v);
    v
}

/// Rewrites `manifest.json` the way an annotation or edit save does: a new field, new size.
fn touch_manifest(dir: &Path, n: u32) {
    let p = dir.join("manifest.json");
    let mut v: Value = serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap();
    v["test_edit"] = json!(format!("edit number {n} {}", "x".repeat(n as usize * 7)));
    std::fs::write(&p, serde_json::to_vec_pretty(&v).unwrap()).unwrap();
}

fn remote_files_named(r: &Rig, name: &str) -> Vec<Vec<u8>> {
    r.drive
        .files()
        .into_iter()
        .filter(|f| !f.is_folder && f.name == name)
        .map(|f| f.data)
        .collect()
}

/// The `test_edit` field of the manifest on Drive and of the local one (the local manifest also
/// gets the upload record when the job ends, which the remote copy cannot have).
fn manifest_edit(bytes: &[u8]) -> Value {
    serde_json::from_slice::<Value>(bytes).unwrap()["test_edit"].clone()
}

fn patches(r: &Rig) -> usize {
    r.drive
        .log()
        .iter()
        .filter(|l| l.starts_with("PATCH "))
        .count()
}

fn resent_paths(job: &Value) -> Vec<String> {
    job["result"]["resent"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["path"].as_str().unwrap().to_owned())
        .collect()
}

fn notes(job: &Value) -> String {
    job["result"]["notes"].to_string()
}

#[tokio::test]
async fn a_manifest_saved_during_the_upload_is_sent_again_into_the_same_folder() {
    let r = rig().await;
    let n = bundle_files(&r.dir).len();
    assert!(n >= 2, "{:?}", bundle_files(&r.dir));
    let dir = r.dir.clone();
    // The second file is starting: the user saves an edit.
    r.drive
        .on_nth_call(DriveCall::Init, 2, move || touch_manifest(&dir, 1));
    let job = wait_done(&r, &start(&r, "drive", json!({})).await).await;
    assert_eq!(job["state"], "done", "{job}");

    let local = std::fs::read(r.dir.join("manifest.json")).unwrap();
    let remote = remote_files_named(&r, "manifest.json");
    assert_eq!(remote.len(), 1, "the file was replaced, not added again");
    assert_eq!(manifest_edit(&remote[0]), manifest_edit(&local));
    assert!(
        manifest_edit(&remote[0]).is_string(),
        "the edit reached Drive"
    );
    assert_eq!(patches(&r), 1);
    let resent = resent_paths(&job);
    assert_eq!(resent.len(), 1, "{resent:?}");
    assert!(resent[0].ends_with("/manifest.json"), "{resent:?}");
    // Still one bundle folder.
    let names: Vec<_> = r
        .drive
        .folders()
        .into_iter()
        .filter(|f| f.name.contains(".cmrec"))
        .collect();
    assert_eq!(names.len(), 1);
    // The record has the final state: the new manifest's size counts.
    let rec = cm_library::uploads_of(&r.dir)
        .into_iter()
        .find(|u| u["what"] == "original")
        .unwrap();
    // What Drive holds, byte for byte (the local manifest has since got this very record).
    let total: u64 = r
        .drive
        .files()
        .iter()
        .filter(|f| !f.is_folder)
        .map(|f| f.data.len() as u64)
        .sum();
    assert_eq!(rec["bytes"], total);
    assert_eq!(rec["files"], n);
    assert!(r.drive.violations().is_empty());
}

#[tokio::test]
async fn a_second_change_during_the_resend_is_sent_in_a_second_round() {
    let r = rig().await;
    let n = bundle_files(&r.dir).len();
    let (d1, d2) = (r.dir.clone(), r.dir.clone());
    r.drive
        .on_nth_call(DriveCall::Init, 2, move || touch_manifest(&d1, 1));
    // The first resend (the Init after the n files) is also overtaken by an edit.
    r.drive
        .on_nth_call(DriveCall::Init, n + 1, move || touch_manifest(&d2, 2));
    let job = wait_done(&r, &start(&r, "drive", json!({})).await).await;
    assert_eq!(job["state"], "done", "{job}");
    assert_eq!(resent_paths(&job).len(), 2, "{job}");
    assert_eq!(patches(&r), 2);
    let remote = remote_files_named(&r, "manifest.json");
    assert_eq!(remote.len(), 1);
    assert_eq!(
        manifest_edit(&remote[0]),
        manifest_edit(&std::fs::read(r.dir.join("manifest.json")).unwrap())
    );
    assert!(manifest_edit(&remote[0])
        .as_str()
        .unwrap()
        .contains("number 2"));
    assert!(!notes(&job).contains("kept changing"), "{job}");
}

#[tokio::test]
async fn a_bundle_that_never_stops_changing_is_given_up_after_three_rounds() {
    let r = rig().await;
    let n = bundle_files(&r.dir).len();
    let d = r.dir.clone();
    r.drive
        .on_nth_call(DriveCall::Init, 2, move || touch_manifest(&d, 1));
    for k in 1..=3usize {
        let d = r.dir.clone();
        r.drive.on_nth_call(DriveCall::Init, n + k, move || {
            touch_manifest(&d, 1 + k as u32)
        });
    }
    let job = wait_done(&r, &start(&r, "drive", json!({})).await).await;
    assert_eq!(job["state"], "done", "the upload itself finished: {job}");
    assert_eq!(patches(&r), 3, "no fourth round");
    assert!(notes(&job).contains("kept changing"), "{job}");
    // One manifest on Drive, never a duplicate.
    assert_eq!(remote_files_named(&r, "manifest.json").len(), 1);
}

#[tokio::test]
async fn a_new_file_is_added_and_a_removed_one_is_noted_not_deleted() {
    let r = rig().await;
    std::fs::write(r.dir.join("0-first.bin"), b"first").unwrap();
    let n = bundle_files(&r.dir).len();
    let dir = r.dir.clone();
    r.drive.on_nth_call(DriveCall::Init, 2, move || {
        // The first file is already on Drive; now it disappears and another one appears.
        std::fs::remove_file(dir.join("0-first.bin")).unwrap();
        std::fs::write(dir.join("zz-new.txt"), b"brand new").unwrap();
        // None of these is ever sent.
        std::fs::write(dir.join("recording.lock"), b"{}").unwrap();
        std::fs::write(dir.join("manifest.json.tmp-123"), b"half").unwrap();
        std::fs::create_dir_all(dir.join("exports")).unwrap();
        std::fs::write(dir.join("exports/later.mp4"), b"export made meanwhile").unwrap();
    });
    let job = wait_done(&r, &start(&r, "drive", json!({})).await).await;
    assert_eq!(job["state"], "done", "{job}");

    assert_eq!(
        remote_files_named(&r, "zz-new.txt"),
        vec![b"brand new".to_vec()]
    );
    // Same folder as the rest of the bundle.
    let new = r
        .drive
        .files()
        .into_iter()
        .find(|f| f.name == "zz-new.txt")
        .unwrap();
    assert!(
        r.drive
            .path_of(&new.id)
            .ends_with("/Lesson.cmrec/zz-new.txt"),
        "{}",
        r.drive.path_of(&new.id)
    );
    // The removed file stays on Drive and the job says so.
    assert_eq!(remote_files_named(&r, "0-first.bin").len(), 1);
    assert!(notes(&job).contains("0-first.bin was removed"), "{job}");
    // Skipped on purpose.
    for skipped in ["recording.lock", "manifest.json.tmp-123", "later.mp4"] {
        assert!(remote_files_named(&r, skipped).is_empty(), "{skipped}");
    }
    let rec = cm_library::uploads_of(&r.dir)
        .into_iter()
        .find(|u| u["what"] == "original")
        .unwrap();
    // The files that were sent: the first plan (n) and the new one.
    assert_eq!(rec["files"], n + 1);
}

#[tokio::test]
async fn a_restart_during_the_resend_continues_without_sending_the_bundle_again() {
    let r = rig().await;
    let n = bundle_files(&r.dir).len();
    let dir = r.dir.clone();
    r.drive
        .on_nth_call(DriveCall::Init, 2, move || touch_manifest(&dir, 1));
    // The resend (the Init after the n files) is refused for good: the job fails there.
    r.drive.inject_after(
        DriveCall::Init,
        n,
        DriveFault::Status {
            status: 403,
            retry_after: None,
            reason: "storageQuotaExceeded",
            message: "The user's Drive storage quota has been exceeded.",
        },
        1,
    );
    let job_id = start(&r, "drive", json!({})).await;
    let failed = wait_done(&r, &job_id).await;
    assert_eq!(failed["state"], "failed", "{failed}");
    assert_eq!(r.drive.count(DriveCall::Init), n + 1);

    call(&r, "retry_job", json!({ "job_id": job_id }))
        .await
        .unwrap();
    let ok = wait_done(&r, &job_id).await;
    assert_eq!(ok["state"], "done", "{ok}");
    // Only the changed file went again: n files, the refused resend, the good resend.
    assert_eq!(r.drive.count(DriveCall::Init), n + 2);
    assert_eq!(patches(&r), 2);
    let remote = remote_files_named(&r, "manifest.json");
    assert_eq!(remote.len(), 1);
    assert_eq!(
        manifest_edit(&remote[0]),
        manifest_edit(&std::fs::read(r.dir.join("manifest.json")).unwrap())
    );
    let folders: Vec<_> = r
        .drive
        .folders()
        .into_iter()
        .filter(|f| f.name.contains(".cmrec"))
        .collect();
    assert_eq!(
        folders.len(),
        1,
        "still the folder the first attempt claimed"
    );
}

#[tokio::test]
async fn a_restart_after_a_new_file_appeared_matches_the_sent_files_by_place() {
    let r = rig().await;
    let n = bundle_files(&r.dir).len();
    // A file that sorts first appears while the job runs; then the job fails at its last file
    // and is retried: the plan now has a different order than the first attempt's.
    let dir = r.dir.clone();
    r.drive.on_nth_call(DriveCall::Init, 2, move || {
        std::fs::write(dir.join("0-early.txt"), b"early").unwrap();
    });
    r.drive.inject_after(
        DriveCall::Init,
        n,
        DriveFault::Status {
            status: 403,
            retry_after: None,
            reason: "storageQuotaExceeded",
            message: "The user's Drive storage quota has been exceeded.",
        },
        1,
    );
    let job_id = start(&r, "drive", json!({})).await;
    let failed = wait_done(&r, &job_id).await;
    assert_eq!(failed["state"], "failed", "{failed}");
    call(&r, "retry_job", json!({ "job_id": job_id }))
        .await
        .unwrap();
    let ok = wait_done(&r, &job_id).await;
    assert_eq!(ok["state"], "done", "{ok}");
    // Every file exists once on Drive, nothing was sent twice.
    for f in bundle_files(&r.dir) {
        let name = f.rsplit('/').next().unwrap();
        assert_eq!(remote_files_named(&r, name).len(), 1, "{f}");
    }
    assert_eq!(
        remote_files_named(&r, "0-early.txt"),
        vec![b"early".to_vec()]
    );
}

#[tokio::test]
async fn a_plain_single_file_upload_is_unchanged_and_never_resent() {
    let r = rig().await;
    let file = r._cfg.path().join("loose.txt");
    std::fs::write(&file, b"version one").unwrap();
    let f2 = file.clone();
    r.drive.on_nth_call(DriveCall::Init, 1, move || {
        std::fs::write(&f2, b"version two, longer").unwrap();
    });
    let job = wait_done(
        &r,
        &start(
            &r,
            "drive",
            json!({ "file": file.to_str().unwrap(), "what": null }),
        )
        .await,
    )
    .await;
    assert_eq!(job["state"], "done", "{job}");
    assert_eq!(r.drive.count(DriveCall::Init), 1);
    assert_eq!(patches(&r), 0);
    assert!(resent_paths(&job).is_empty());
    assert_eq!(remote_files_named(&r, "loose.txt").len(), 1);
}

#[tokio::test]
async fn an_unchanged_bundle_sends_nothing_twice() {
    let r = rig().await;
    let job = wait_done(&r, &start(&r, "drive", json!({})).await).await;
    assert_eq!(job["state"], "done", "{job}");
    assert_eq!(patches(&r), 0);
    assert!(resent_paths(&job).is_empty());
    assert!(!notes(&job).contains("changing"));
}

#[tokio::test]
async fn a_local_destination_gets_the_new_manifest_over_the_old_one() {
    let r = rig().await;
    // A big last file gives the editor time to save while the job copies it.
    std::fs::write(r.dir.join("zz-big.bin"), vec![1u8; 80_000_000]).unwrap();
    let dir = r.dir.clone();
    let nas = r.nas.clone();
    let editor = std::thread::spawn(move || {
        // Wait until the manifest is on the NAS, then save an edit.
        for _ in 0..4000 {
            let found = walk_for(&nas, "manifest.json");
            if found.is_some() {
                touch_manifest(&dir, 1);
                return true;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        false
    });
    let job = wait_done(&r, &start(&r, "nas", json!({})).await).await;
    assert!(editor.join().unwrap(), "the manifest never reached the NAS");
    assert_eq!(job["state"], "done", "{job}");
    let copy = walk_for(&r.nas, "manifest.json").unwrap();
    assert_eq!(
        manifest_edit(&std::fs::read(&copy).unwrap()),
        manifest_edit(&std::fs::read(r.dir.join("manifest.json")).unwrap())
    );
    assert!(manifest_edit(&std::fs::read(&copy).unwrap()).is_string());
    assert!(resent_paths(&job)
        .iter()
        .any(|p| p.ends_with("manifest.json")));
    // No `manifest (2).json` beside it and no temp file left behind.
    let all = walk_all(&r.nas);
    assert!(
        all.iter()
            .all(|p| !p.contains("(2)") && !p.ends_with(".part")),
        "{all:?}"
    );
}

fn walk_all(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.push(p.display().to_string());
            }
        }
    }
    out
}

fn walk_for(root: &Path, name: &str) -> Option<PathBuf> {
    walk_all(root)
        .into_iter()
        .map(PathBuf::from)
        .find(|p| p.file_name().is_some_and(|n| n == name))
}
