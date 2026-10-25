//! Automatic upload (STO-3) and the upload records (STO-4.AC3), provider-agnostic: fake recording
//! sources, a stub exporter that writes a small file, the real upload job copying to temp-dir
//! destinations. No FFmpeg, no screen, no devices.

#![allow(clippy::unwrap_used)]

mod common;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use cm_core::{Actor, CmError, ManualHostClock};
use cm_daemon::testing::FakeSources;
use cm_daemon::{Daemon, EngineOptions, Event};
use cm_jobs::{JobContext, JobHandler, JOB_EXPORT, JOB_UPLOAD};
use cm_storage::{AutoUpload, Destination, DestinationStore, Kind, What, DEFAULT_FOLDER_PATTERN};
use common::{builder, desktop};
use serde_json::{json, Value};
use tokio::sync::broadcast::Receiver;

/// Writes `exports/<title> - quick.mp4` into the bundle and records it, like the real exporter.
struct StubExport {
    root: PathBuf,
    fail: Arc<AtomicBool>,
}

#[async_trait]
impl JobHandler for StubExport {
    async fn run(&self, ctx: JobContext) -> Result<Value, CmError> {
        if self.fail.load(Ordering::SeqCst) {
            return Err(CmError::internal("the encoder crashed"));
        }
        let id = ctx.job.inputs["capture_id"].as_str().unwrap().to_owned();
        let dir = bundle_dir(&self.root, &id);
        std::fs::create_dir_all(dir.join("exports")).unwrap();
        std::fs::write(dir.join("exports/Take - quick.mp4"), b"exported video").unwrap();
        cm_library::record_export(
            &dir,
            json!({ "file": "exports/Take - quick.mp4", "preset": "quick" }),
            Actor::User,
            time::OffsetDateTime::now_utc(),
        )
        .unwrap();
        Ok(json!({ "path": "x" }))
    }
}

fn bundle_dir(root: &Path, id: &str) -> PathBuf {
    std::fs::read_dir(root)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "cmrec"))
        .find(|p| cm_library::read_manifest(p).is_ok_and(|m| m.id == id))
        .expect("bundle folder")
}

struct Rig {
    daemon: Daemon,
    store: DestinationStore,
    root: PathBuf,
    nas: PathBuf,
    fail_export: Arc<AtomicBool>,
    _dirs: (tempfile::TempDir, tempfile::TempDir),
}

fn dest(id: &str, path: &Path, what: What, auto: AutoUpload) -> Destination {
    Destination {
        id: id.into(),
        name: format!("Dest {id}"),
        kind: Kind::Local,
        path: path.to_path_buf(),
        account: None,
        folder_pattern: DEFAULT_FOLDER_PATTERN.into(),
        what,
        auto_upload: auto,
        default: false,
    }
}

fn rig(what: What, auto: AutoUpload) -> Rig {
    rig_with(&[("nas", what, auto)], true)
}

fn rig_with(dests: &[(&str, What, AutoUpload)], ffmpeg: bool) -> Rig {
    let (b, lib) = builder(desktop());
    let cfg = tempfile::tempdir().unwrap();
    let nas = cfg.path().join("nas");
    std::fs::create_dir(&nas).unwrap();
    let store = DestinationStore::new(cfg.path().join("config"));
    store
        .save(
            &dests
                .iter()
                .map(|(id, w, a)| dest(id, &nas, *w, *a))
                .collect::<Vec<_>>(),
        )
        .unwrap();
    let queue = cm_daemon::library_job_queue(lib.path()).unwrap();
    let opts = EngineOptions {
        tick_every: Duration::from_millis(15),
        countdown_step: Duration::from_millis(30),
        host: Arc::new(ManualHostClock::new(5_000_000_000)),
        own_pid: None,
        disk_check_every: Duration::ZERO,
        selftest_duration: Duration::from_millis(100),
        ffmpeg_probe: Some(Arc::new(move || ffmpeg)),
        ..EngineOptions::default()
    };
    let daemon = b
        .jobs(queue)
        .destinations(store.clone())
        .recording_sources(FakeSources::new())
        .recording_options(opts)
        .build();
    let fail_export = Arc::new(AtomicBool::new(false));
    let root = lib.path().to_path_buf();
    daemon
        .register_job_handler(
            JOB_EXPORT,
            Arc::new(StubExport {
                root: root.clone(),
                fail: fail_export.clone(),
            }),
        )
        .unwrap();
    daemon
        .register_job_handler(JOB_UPLOAD, daemon.upload_handler(Duration::from_millis(20)))
        .unwrap();
    daemon.start_jobs().unwrap();
    Rig {
        daemon,
        store,
        root,
        nas,
        fail_export,
        _dirs: (lib, cfg),
    }
}

async fn call(d: &Daemon, method: &str, p: Value) -> Value {
    d.execute_method(Actor::User, 0, method, &p).await.unwrap()
}

async fn record(d: &Daemon) -> String {
    let s = call(
        d,
        "start_recording",
        json!({ "target": { "display_id": 1 }, "countdown_s": 0, "title": "Take" }),
    )
    .await;
    let id = s["capture_id"].as_str().unwrap().to_owned();
    let stopped = call(d, "stop_recording", json!({})).await;
    assert_eq!(stopped["capture_id"], id.as_str());
    id
}

async fn jobs(d: &Daemon, ty: &str) -> Vec<Value> {
    call(d, "list_jobs", json!({ "type": ty })).await["jobs"]
        .as_array()
        .unwrap()
        .clone()
}

async fn until<T>(mut f: impl AsyncFnMut() -> Option<T>) -> T {
    for _ in 0..400 {
        if let Some(v) = f().await {
            return v;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("condition not reached in 4 s");
}

async fn wait_state(d: &Daemon, id: &str, state: &str) -> Value {
    until(async || {
        let j = call(d, "get_job", json!({ "job_id": id })).await;
        (j["state"] == state).then_some(j)
    })
    .await
}

async fn the_upload(d: &Daemon) -> Value {
    until(async || jobs(d, "upload").await.into_iter().next()).await
}

fn drain(rx: &mut Receiver<Event>) -> Vec<Event> {
    let mut v = Vec::new();
    while let Ok(e) = rx.try_recv() {
        v.push(e);
    }
    v
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

async fn uploads_in_library(d: &Daemon, id: &str) -> Vec<Value> {
    let v = call(d, "list_library", json!({})).await;
    v["captures"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == id)
        .unwrap()["uploads"]
        .as_array()
        .unwrap()
        .clone()
}

// STO-3.AC1 and STO-4.AC3: on stop, an upload job is queued without user action; when it is
// done the capture lists the upload, and no undo step was added.
#[tokio::test]
async fn sto3_ac1_on_stop_queues_the_upload_at_once_and_records_it() {
    let r = rig(What::Original, AutoUpload::OnStop);
    let mut rx = r.daemon.subscribe();
    let id = record(&r.daemon).await;
    let started = std::time::Instant::now();
    let job = the_upload(&r.daemon).await;
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_eq!(job["inputs"]["capture_id"], id.as_str());
    assert_eq!(job["inputs"]["destination_id"], "nas");
    assert_eq!(job["inputs"]["what"], "original");
    assert_eq!(job["inputs"]["auto"], true);
    assert_eq!(job["depends_on"], json!([]), "an original needs no export");
    let done = wait_state(&r.daemon, job["job_id"].as_str().unwrap(), "done").await;
    assert_eq!(done["result"]["destination"], "Dest nas");
    assert!(done["result"]["record_warning"].is_null(), "{done}");
    assert!(files_under(&r.nas)
        .iter()
        .any(|f| f.ends_with("manifest.json")));

    // STO-4.AC3: list_library shows the record; the manifest got no revision for it.
    let list = uploads_in_library(&r.daemon, &id).await;
    assert_eq!(list.len(), 1, "{list:?}");
    assert_eq!(list[0]["destination_id"], "nas");
    assert_eq!(list[0]["destination"], "Dest nas");
    assert_eq!(list[0]["what"], "original");
    assert!(list[0]["bytes"].as_u64().unwrap() > 0);
    assert!(list[0]["at"].as_str().unwrap().contains('T'));
    let m = cm_library::read_manifest(&bundle_dir(&r.root, &id)).unwrap();
    assert!(m.revisions.iter().all(|rev| rev.action != "upload"));

    let ev = drain(&mut rx);
    assert!(ev.iter().any(|e| e.name == "upload.queued"
        && e.data["capture_id"] == id.as_str()
        && e.data["trigger"] == "on_stop"));
}

// STO-3.AC2: an upload of an export waits for the export; the default export is rendered even
// when the automatic export is off; the file sent is the export only.
#[tokio::test]
async fn sto3_ac2_the_upload_depends_on_the_export_and_sends_it() {
    let r = rig(What::Export, AutoUpload::OnStop);
    call(
        &r.daemon,
        "set_settings",
        json!({ "values": { "export.auto_on_stop": false } }),
    )
    .await;
    let id = record(&r.daemon).await;
    let up = the_upload(&r.daemon).await;
    let exports = jobs(&r.daemon, "export").await;
    assert_eq!(
        exports.len(),
        1,
        "the export setting is off, the upload forced one"
    );
    assert_eq!(exports[0]["inputs"]["capture_id"], id.as_str());
    assert_eq!(up["depends_on"], json!([exports[0]["job_id"]]));
    wait_state(&r.daemon, exports[0]["job_id"].as_str().unwrap(), "done").await;
    let done = wait_state(&r.daemon, up["job_id"].as_str().unwrap(), "done").await;
    assert_eq!(done["error"], Value::Null);
    let files = files_under(&r.nas);
    assert_eq!(files.len(), 1, "{files:?}");
    assert!(files[0].ends_with("Take - quick.mp4"), "{files:?}");
    let list = uploads_in_library(&r.daemon, &id).await;
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["what"], "export");
    assert!(list[0]["remote_id"]
        .as_str()
        .unwrap()
        .ends_with("Take - quick.mp4"));
}

#[tokio::test]
async fn sto3_ac2_a_failed_export_skips_the_upload_with_the_reason() {
    let r = rig(What::Export, AutoUpload::OnStop);
    r.fail_export.store(true, Ordering::SeqCst);
    record(&r.daemon).await;
    let up = the_upload(&r.daemon).await;
    let failed = wait_state(&r.daemon, up["job_id"].as_str().unwrap(), "failed").await;
    let reason = failed["error"]["reason"].as_str().unwrap();
    assert!(reason.contains("dependency"), "{reason}");
    assert!(files_under(&r.nas).is_empty());
}

// `both` sends the export and the bundle, and records each.
#[tokio::test]
async fn both_records_the_export_and_the_bundle() {
    let r = rig(What::Both, AutoUpload::OnStop);
    let id = record(&r.daemon).await;
    let up = the_upload(&r.daemon).await;
    wait_state(&r.daemon, up["job_id"].as_str().unwrap(), "done").await;
    let list = uploads_in_library(&r.daemon, &id).await;
    let whats: Vec<&str> = list.iter().map(|u| u["what"].as_str().unwrap()).collect();
    assert_eq!(whats, ["export", "original"]);
    assert!(list[1]["files"].as_u64().unwrap() >= 1);
}

// "On export": an upload waits for any export of the capture, and never twice at once.
#[tokio::test]
async fn on_export_follows_every_export_and_is_idempotent() {
    let r = rig(What::Export, AutoUpload::OnExport);
    let id = record(&r.daemon).await;
    // The automatic export of the stop triggers it.
    let first = the_upload(&r.daemon).await;
    assert_eq!(first["inputs"]["trigger"], "on_export");
    let export_id = jobs(&r.daemon, "export").await[0]["job_id"].clone();
    assert_eq!(first["depends_on"], json!([export_id]));
    wait_state(&r.daemon, first["job_id"].as_str().unwrap(), "done").await;

    // A manual export afterwards queues one more upload; asking twice while it is active
    // (held here so it stays queued) still gives one.
    assert!(r.daemon.hold_job_type("upload"));
    for _ in 0..2 {
        call(
            &r.daemon,
            "export_video",
            json!({ "capture_id": id, "preset": "quick", "loudness_lufs": -16.0 }),
        )
        .await;
    }
    let all = jobs(&r.daemon, "upload").await;
    let active: Vec<_> = all.iter().filter(|j| j["state"] != "done").collect();
    assert_eq!(active.len(), 1, "{all:?}");
    assert_eq!(all.len(), 2);
}

// Off does nothing; a discard, a cancelled countdown and the self-test never upload.
#[tokio::test]
async fn off_discard_and_selftest_queue_nothing() {
    let r = rig(What::Original, AutoUpload::Off);
    record(&r.daemon).await;
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert!(jobs(&r.daemon, "upload").await.is_empty());

    let r = rig(What::Original, AutoUpload::OnStop);
    call(
        &r.daemon,
        "start_recording",
        json!({ "target": { "display_id": 1 }, "countdown_s": 0 }),
    )
    .await;
    call(&r.daemon, "discard_recording", json!({})).await;
    let t = call(
        &r.daemon,
        "run_fps_selftest",
        json!({ "target": { "display_id": 1 } }),
    )
    .await;
    assert_eq!(t["state"], "passed");
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(jobs(&r.daemon, "upload").await.is_empty());
}

// An upload that needs an export, with no exporter possible: skipped, with the reason.
#[tokio::test]
async fn without_ffmpeg_an_export_upload_is_skipped_with_a_note() {
    let r = rig_with(&[("nas", What::Export, AutoUpload::OnStop)], false);
    let mut rx = r.daemon.subscribe();
    let id = record(&r.daemon).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(jobs(&r.daemon, "upload").await.is_empty());
    let ev = drain(&mut rx);
    let skipped = ev.iter().find(|e| e.name == "upload.skipped").unwrap();
    assert_eq!(skipped.data["capture_id"], id.as_str());
    assert_eq!(skipped.data["destination_id"], "nas");
    assert!(skipped.data["reason"].as_str().unwrap().contains("FFmpeg"));
}

// Several destinations: each follows its own mode.
#[tokio::test]
async fn each_destination_follows_its_own_mode() {
    let r = rig_with(
        &[
            ("a", What::Original, AutoUpload::OnStop),
            ("b", What::Original, AutoUpload::Off),
            ("c", What::Original, AutoUpload::OnStop),
        ],
        true,
    );
    record(&r.daemon).await;
    until(async || (jobs(&r.daemon, "upload").await.len() == 2).then_some(())).await;
    let mut ids: Vec<String> = jobs(&r.daemon, "upload")
        .await
        .iter()
        .map(|j| j["inputs"]["destination_id"].as_str().unwrap().to_owned())
        .collect();
    ids.sort();
    assert_eq!(ids, ["a", "c"]);
}

// STO-3.AC3 with an automatic upload: a recording that starts holds it, the stop releases it.
#[tokio::test]
async fn sto3_ac3_a_new_recording_holds_the_automatic_upload() {
    let r = rig(What::Original, AutoUpload::OnStop);
    // Offline destination: the upload job keeps running ("Destination offline") until it is back.
    let away = r.nas.with_extension("away");
    std::fs::rename(&r.nas, &away).unwrap();
    let id = record(&r.daemon).await;
    let up = the_upload(&r.daemon).await;
    let up_id = up["job_id"].as_str().unwrap().to_owned();
    wait_state(&r.daemon, &up_id, "running").await;

    call(
        &r.daemon,
        "start_recording",
        json!({ "target": { "display_id": 1 }, "countdown_s": 0 }),
    )
    .await;
    let held = wait_state(&r.daemon, &up_id, "paused").await;
    assert_eq!(held["pause_reason"], "hold:type:upload");
    call(&r.daemon, "stop_recording", json!({})).await;

    std::fs::rename(&away, &r.nas).unwrap();
    wait_state(&r.daemon, &up_id, "done").await;
    assert_eq!(uploads_in_library(&r.daemon, &id).await.len(), 1);
}

// With the setting off, uploads are not held (the engine's rule, kept for automatic uploads).
#[tokio::test]
async fn automatic_uploads_are_not_held_when_the_setting_is_off() {
    let r = rig(What::Original, AutoUpload::OnStop);
    call(
        &r.daemon,
        "set_settings",
        json!({ "values": { "storage.pause_uploads_while_recording": false } }),
    )
    .await;
    call(
        &r.daemon,
        "start_recording",
        json!({ "target": { "display_id": 1 }, "countdown_s": 0 }),
    )
    .await;
    assert!(!r.daemon.is_job_type_held("upload"));
    call(&r.daemon, "stop_recording", json!({})).await;
}

// The old `auto_on_stop` file still loads through the daemon and reads as "on_stop".
#[tokio::test]
async fn an_old_destinations_file_reads_as_on_stop() {
    let r = rig(What::Original, AutoUpload::Off);
    let cfg = r.store.list().unwrap();
    assert_eq!(cfg.len(), 1);
    let path = r._dirs.1.path().join("config/destinations.json");
    std::fs::write(
        &path,
        format!(
            r#"{{"version":1,"destinations":[{{"id":"nas","name":"NAS","kind":"local","path":{},"auto_on_stop":true}}]}}"#,
            serde_json::to_string(&r.nas).unwrap()
        ),
    )
    .unwrap();
    let v = call(&r.daemon, "list_destinations", json!({})).await;
    assert_eq!(v["destinations"][0]["auto_upload"], "on_stop");
    record(&r.daemon).await;
    assert_eq!(the_upload(&r.daemon).await["inputs"]["trigger"], "on_stop");
}
