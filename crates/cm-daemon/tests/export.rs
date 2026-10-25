//! `export_video`, the `export` job and the auto-export after a recording stops (REC-1, EXP-1,
//! EXP-4), with fake recording sources and a stub exporter (no FFmpeg), plus FFmpeg-gated runs of
//! the real exporter on a synthesized bundle.

#![allow(clippy::unwrap_used)]

mod common;

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use cm_core::{Actor, CmError, ErrorCode, ManualHostClock};
use cm_daemon::testing::FakeSources;
use cm_daemon::{Daemon, DaemonError, EngineOptions, Event};
use cm_jobs::{JobContext, JobHandler, JobQueue, JOB_EXPORT};
use common::{builder, desktop};
use serde_json::{json, Value};
use tokio::sync::broadcast::Receiver;

const ORIGIN_NS: u64 = 5_000_000_000;

/// Records every run, and whether the export hold was still on when it started.
#[derive(Clone)]
struct Stub {
    queue: JobQueue,
    runs: Arc<Mutex<Vec<Value>>>,
    held_when_run: Arc<AtomicBool>,
    /// Wait for an interrupt (a held / cancelled job) instead of finishing.
    block_first: Arc<AtomicBool>,
    started: Arc<AtomicUsize>,
}

#[async_trait]
impl JobHandler for Stub {
    async fn run(&self, ctx: JobContext) -> Result<Value, CmError> {
        self.runs.lock().unwrap().push(ctx.job.inputs.clone());
        self.started.fetch_add(1, Ordering::SeqCst);
        if self.queue.is_type_held(JOB_EXPORT) {
            self.held_when_run.store(true, Ordering::SeqCst);
        }
        if self.block_first.swap(false, Ordering::SeqCst) {
            let mut t = ctx.cancel.clone();
            t.interrupted().await;
            return Err(CmError::internal("interrupted"));
        }
        ctx.reporter.report(0.5, Some("half".to_owned()));
        Ok(json!({ "path": "/stub/out.mp4", "size_bytes": 1 }))
    }
}

struct Rig {
    daemon: Daemon,
    stub: Stub,
    _dir: tempfile::TempDir,
}

fn rig(with_handler: bool) -> Rig {
    rig_on(with_handler, desktop())
}

fn rig_on(with_handler: bool, backend: cm_capture::fake::FakeBackend) -> Rig {
    rig_ffmpeg(with_handler, backend, true)
}

/// `ffmpeg`: whether the engine finds an FFmpeg for the automatic export (R5-3), regardless of
/// what is installed on the machine running the tests.
fn rig_ffmpeg(with_handler: bool, backend: cm_capture::fake::FakeBackend, ffmpeg: bool) -> Rig {
    let (b, dir) = builder(backend);
    let host = Arc::new(ManualHostClock::new(ORIGIN_NS));
    let queue = cm_daemon::library_job_queue(dir.path()).unwrap();
    let opts = EngineOptions {
        tick_every: Duration::from_millis(15),
        countdown_step: Duration::from_millis(30),
        host,
        own_pid: None,
        disk_check_every: Duration::ZERO,
        selftest_duration: Duration::from_millis(100),
        ffmpeg_probe: Some(Arc::new(move || ffmpeg)),
        ..EngineOptions::default()
    };
    let daemon = b
        .jobs(queue.clone())
        .recording_sources(FakeSources::new())
        .recording_options(opts)
        .build();
    let stub = Stub {
        queue,
        runs: Arc::default(),
        held_when_run: Arc::default(),
        block_first: Arc::default(),
        started: Arc::default(),
    };
    if with_handler {
        daemon
            .register_job_handler(JOB_EXPORT, Arc::new(stub.clone()))
            .unwrap();
    }
    daemon.start_jobs().unwrap();
    Rig {
        daemon,
        stub,
        _dir: dir,
    }
}

async fn user(d: &Daemon, method: &str, params: Value) -> Result<Value, DaemonError> {
    d.execute_method(Actor::User, 0, method, &params).await
}

fn code(e: &DaemonError) -> ErrorCode {
    match e {
        DaemonError::Cm(c) => c.code(),
        DaemonError::WithData { error, .. } => error.code(),
        DaemonError::NotImplemented(_) => ErrorCode::NotImplemented,
        other => panic!("unexpected {other:?}"),
    }
}

async fn start(d: &Daemon, extra: Value) -> Value {
    let mut p = json!({ "target": { "display_id": 1 }, "countdown_s": 0 });
    for (k, v) in extra.as_object().unwrap() {
        p[k] = v.clone();
    }
    user(d, "start_recording", p).await.unwrap()
}

async fn export_jobs(d: &Daemon) -> Vec<Value> {
    let r = user(d, "list_jobs", json!({ "type": "export" }))
        .await
        .unwrap();
    r["jobs"].as_array().unwrap().clone()
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

async fn wait_job(d: &Daemon, id: &str, state: &str) -> Value {
    until(async || {
        let j = user(d, "get_job", json!({ "job_id": id })).await.unwrap();
        (j["state"] == state).then_some(j)
    })
    .await
}

fn drain(rx: &mut Receiver<Event>) -> Vec<Event> {
    let mut v = Vec::new();
    while let Ok(e) = rx.try_recv() {
        v.push(e);
    }
    v
}

// ---- auto-export -------------------------------------------------------------------------------

/// REC-1: a normal stop enqueues one `export` job (preset `quick`, `auto: true`, `by` the starter)
/// after the holds are released, so it runs at once; `export.queued`, `job.*` and `export` events
/// tell clients.
#[tokio::test]
async fn rec1_stop_enqueues_the_export_after_the_holds_are_released() {
    let r = rig(true);
    let mut rx = r.daemon.subscribe();
    let s = start(&r.daemon, json!({ "title": "Lesson" })).await;
    assert!(r.daemon.is_job_type_held("export"), "held while recording");
    assert!(export_jobs(&r.daemon).await.is_empty());
    let capture_id = s["capture_id"].as_str().unwrap().to_owned();
    let stopped = user(&r.daemon, "stop_recording", json!({})).await.unwrap();
    assert_eq!(stopped["capture_id"], capture_id.as_str());
    assert!(!r.daemon.is_job_type_held("export"));

    let jobs = until(async || {
        let j = export_jobs(&r.daemon).await;
        (!j.is_empty()).then_some(j)
    })
    .await;
    assert_eq!(jobs.len(), 1);
    let id = jobs[0]["job_id"].as_str().unwrap().to_owned();
    assert_eq!(jobs[0]["inputs"]["capture_id"], capture_id.as_str());
    assert_eq!(jobs[0]["inputs"]["preset"], "quick");
    assert_eq!(jobs[0]["inputs"]["auto"], true);
    assert_eq!(jobs[0]["inputs"]["by"], "user");
    let done = wait_job(&r.daemon, &id, "done").await;
    assert_eq!(done["result"]["path"], "/stub/out.mp4");
    assert!(
        !r.stub.held_when_run.load(Ordering::SeqCst),
        "the job ran after the release, not while held"
    );
    let ev = drain(&mut rx);
    let n: Vec<&str> = ev.iter().map(|e| e.name.as_str()).collect();
    let stopped_at = n.iter().position(|e| *e == "recording.stopped").unwrap();
    let queued_at = n.iter().position(|e| *e == "export.queued").unwrap();
    assert!(queued_at > stopped_at, "{n:?}");
    let q = &ev[queued_at];
    assert_eq!(q.data["job_id"], id.as_str());
    assert_eq!(q.data["capture_id"], capture_id.as_str());
    assert_eq!(q.data["auto"], true);
    assert!(n.contains(&"job.done"), "{n:?}");
}

/// A discard, a cancelled countdown and the 60 fps self-test never export.
#[tokio::test]
async fn rec1_no_export_after_discard_cancelled_countdown_or_selftest() {
    let r = rig(true);
    start(&r.daemon, json!({})).await;
    user(&r.daemon, "discard_recording", json!({}))
        .await
        .unwrap();

    // A countdown that is cancelled never creates a recording.
    let d = r.daemon.clone();
    let starting = tokio::spawn(async move {
        user(
            &d,
            "start_recording",
            json!({ "target": { "display_id": 1 }, "countdown_s": 10 }),
        )
        .await
    });
    until(async || {
        let st = user(&r.daemon, "get_recording_status", json!({}))
            .await
            .ok()?;
        (st["state"] == "countdown").then_some(())
    })
    .await;
    let ans = user(&r.daemon, "stop_recording", json!({})).await.unwrap();
    assert_eq!(ans["cancelled"], true);
    assert!(starting.await.unwrap().is_err());

    let t = user(
        &r.daemon,
        "run_fps_selftest",
        json!({ "target": { "display_id": 1 } }),
    )
    .await
    .unwrap();
    assert_eq!(t["state"], "passed");
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert!(export_jobs(&r.daemon).await.is_empty());
    assert_eq!(r.stub.started.load(Ordering::SeqCst), 0);
}

/// `export.auto_on_stop` (default true): off means no job; on again means a job.
#[tokio::test]
async fn rec1_auto_export_follows_the_setting() {
    let r = rig(true);
    let all = user(&r.daemon, "get_settings", json!({})).await.unwrap();
    assert!(
        all["settings"].get("export.auto_on_stop").is_none(),
        "unset = default on"
    );
    user(
        &r.daemon,
        "set_settings",
        json!({ "values": { "export.auto_on_stop": false } }),
    )
    .await
    .unwrap();
    start(&r.daemon, json!({})).await;
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert!(export_jobs(&r.daemon).await.is_empty());

    user(
        &r.daemon,
        "set_settings",
        json!({ "values": { "export.auto_on_stop": true } }),
    )
    .await
    .unwrap();
    start(&r.daemon, json!({})).await;
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
    until(async || (export_jobs(&r.daemon).await.len() == 1).then_some(())).await;
}

/// R5-3: without FFmpeg the automatic export is skipped (it could only fail, with an error toast
/// after every recording): no job, one `export.skipped { reason: ffmpeg_not_found }` per run of
/// the app, and the recording is saved.
#[tokio::test]
async fn r5_3_auto_export_is_skipped_once_when_ffmpeg_is_missing() {
    let r = rig_ffmpeg(true, desktop(), false);
    let mut rx = r.daemon.subscribe();
    start(&r.daemon, json!({})).await;
    let stopped = user(&r.daemon, "stop_recording", json!({})).await.unwrap();
    start(&r.daemon, json!({})).await;
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(export_jobs(&r.daemon).await.is_empty());
    assert_eq!(r.stub.started.load(Ordering::SeqCst), 0);
    let ev = drain(&mut rx);
    let skipped: Vec<_> = ev.iter().filter(|e| e.name == "export.skipped").collect();
    assert_eq!(skipped.len(), 1, "once per run");
    assert_eq!(skipped[0].data["reason"], "ffmpeg_not_found");
    assert_eq!(skipped[0].data["capture_id"], stopped["capture_id"]);
    assert!(skipped[0].data["message"]
        .as_str()
        .unwrap()
        .contains("FFmpeg"));
    assert!(ev.iter().all(|e| e.name != "export.queued"));
    // With FFmpeg present nothing is skipped.
    let ok = rig_ffmpeg(true, desktop(), true);
    let mut rx2 = ok.daemon.subscribe();
    start(&ok.daemon, json!({})).await;
    user(&ok.daemon, "stop_recording", json!({})).await.unwrap();
    until(async || (export_jobs(&ok.daemon).await.len() == 1).then_some(())).await;
    assert!(drain(&mut rx2).iter().all(|e| e.name != "export.skipped"));
}

/// No exporter registered (a daemon that never asked for one): no job is left queued forever.
#[tokio::test]
async fn auto_export_needs_a_registered_exporter() {
    let r = rig(false);
    start(&r.daemon, json!({})).await;
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(export_jobs(&r.daemon).await.is_empty());
}

/// EXP-4.AC2: a running export is held when a new recording starts (it stops at its interrupt)
/// and runs again from the beginning after that recording stops.
#[tokio::test]
async fn exp4_ac2_a_new_recording_holds_a_running_export_and_it_resumes_after() {
    let r = rig(true);
    r.stub.block_first.store(true, Ordering::SeqCst);
    start(&r.daemon, json!({})).await;
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
    // The first run is blocked inside the handler.
    until(async || (r.stub.started.load(Ordering::SeqCst) == 1).then_some(())).await;
    let id = export_jobs(&r.daemon).await[0]["job_id"]
        .as_str()
        .unwrap()
        .to_owned();

    start(&r.daemon, json!({})).await; // holds `export`: the running job is paused
    let held = until(async || {
        let j = user(&r.daemon, "get_job", json!({ "job_id": id }))
            .await
            .unwrap();
        (j["state"] == "paused").then_some(j)
    })
    .await;
    assert_eq!(held["pause_reason"], "hold:type:export");
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
    // The first job restarts (a second run) and another job exists for the second recording.
    wait_job(&r.daemon, &id, "done").await;
    assert!(r.stub.started.load(Ordering::SeqCst) >= 2);
    until(async || {
        let jobs = export_jobs(&r.daemon).await;
        (jobs.len() == 2 && jobs.iter().all(|j| j["state"] == "done")).then_some(())
    })
    .await;
}

// ---- export_video ---------------------------------------------------------------------------

/// AUD-4: `export_video { loudness_lufs }` is validated, reaches the job's inputs, and an export
/// at another level is its own job (not a reuse of the one without a target).
#[tokio::test]
async fn aud4_export_video_loudness_target_is_validated_and_part_of_the_job() {
    let r = rig(true);
    r.stub.block_first.store(true, Ordering::SeqCst);
    user(
        &r.daemon,
        "set_settings",
        json!({ "values": { "export.auto_on_stop": false } }),
    )
    .await
    .unwrap();
    let s = start(&r.daemon, json!({})).await;
    let capture_id = s["capture_id"].as_str().unwrap().to_owned();
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();

    let plain = user(
        &r.daemon,
        "export_video",
        json!({ "capture_id": capture_id }),
    )
    .await
    .unwrap();
    let loud = user(
        &r.daemon,
        "export_video",
        json!({ "capture_id": capture_id, "loudness_lufs": -14.0 }),
    )
    .await
    .unwrap();
    assert_ne!(loud["job_id"], plain["job_id"]);
    assert_eq!(loud["reused"], false);
    assert_eq!(loud["loudness_lufs"], -14.0);
    let same = user(
        &r.daemon,
        "export_video",
        json!({ "capture_id": capture_id, "loudness_lufs": -14 }),
    )
    .await
    .unwrap();
    assert_eq!(same["job_id"], loud["job_id"]);
    assert_eq!(same["reused"], true);
    let j = user(&r.daemon, "get_job", json!({ "job_id": loud["job_id"] }))
        .await
        .unwrap();
    assert_eq!(j["inputs"]["loudness_lufs"], -14.0);
    for bad in [json!(-3.0), json!(-80.0), json!("loud")] {
        let e = user(
            &r.daemon,
            "export_video",
            json!({ "capture_id": capture_id, "loudness_lufs": bad }),
        )
        .await
        .unwrap_err();
        assert!(e.to_string().contains("loudness_lufs"), "{e}");
    }
    for id in [plain["job_id"].clone(), loud["job_id"].clone()] {
        let _ = user(&r.daemon, "cancel_job", json!({ "job_id": id })).await;
    }
}

/// EXP-1 / EXP-4: `export_video` returns the job id; `quick` is the only preset; the job's
/// inputs carry the caller; an equal active job is reused; `request_id` replays.
#[tokio::test]
async fn export_video_enqueues_validates_and_is_idempotent() {
    let r = rig(true);
    r.stub.block_first.store(true, Ordering::SeqCst);
    user(
        &r.daemon,
        "set_settings",
        json!({ "values": { "export.auto_on_stop": false } }),
    )
    .await
    .unwrap();
    let s = start(&r.daemon, json!({})).await;
    let capture_id = s["capture_id"].as_str().unwrap().to_owned();
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();

    let agent = Actor::agent("claude").unwrap();
    let call = json!({ "capture_id": capture_id, "request_id": "req-1" });
    let a = r
        .daemon
        .execute_method(agent.clone(), 0, "export_video", &call)
        .await
        .unwrap();
    let job_id = a["job_id"].as_str().unwrap().to_owned();
    assert_eq!(a["preset"], "quick");
    assert_eq!(a["reused"], false);
    // Same request id: the first result, no second job.
    let again = r
        .daemon
        .execute_method(agent, 0, "export_video", &call)
        .await
        .unwrap();
    assert_eq!(again, a);
    // A different request for the same capture and preset reuses the active job.
    let reuse = user(
        &r.daemon,
        "export_video",
        json!({ "capture_id": capture_id }),
    )
    .await
    .unwrap();
    assert_eq!(reuse["job_id"], job_id.as_str());
    assert_eq!(reuse["reused"], true);
    assert_eq!(export_jobs(&r.daemon).await.len(), 1);
    let j = user(&r.daemon, "get_job", json!({ "job_id": job_id }))
        .await
        .unwrap();
    assert_eq!(j["inputs"]["by"], "agent:claude");
    assert_eq!(j["inputs"]["auto"], false);
    // Cancelling the blocked first run ends the job as cancelled (EXP-4.AC4 path).
    until(async || (r.stub.started.load(Ordering::SeqCst) == 1).then_some(())).await;
    user(&r.daemon, "cancel_job", json!({ "job_id": job_id }))
        .await
        .unwrap();
    wait_job(&r.daemon, &job_id, "cancelled").await;
}

#[tokio::test]
async fn export_video_errors() {
    let r = rig(true);
    // Unknown capture.
    let e = user(
        &r.daemon,
        "export_video",
        json!({ "capture_id": "rec_nope" }),
    )
    .await
    .unwrap_err();
    assert_eq!(code(&e), ErrorCode::NotFound);
    // The custom form (size, codec, quality, captions, out_dir) is not available yet; an unknown
    // preset and a preset/format mismatch name what is wrong.
    for (p, want) in [
        (
            json!({ "capture_id": "x", "size": "1080p" }),
            "not available yet",
        ),
        (
            json!({ "capture_id": "x", "codec": "hevc" }),
            "not available yet",
        ),
        (
            json!({ "capture_id": "x", "preset": "tiktok" }),
            "unknown preset",
        ),
        (
            json!({ "capture_id": "x", "preset": "webm", "format": "gif" }),
            "does not match",
        ),
        (json!({ "capture_id": "x", "width": 640 }), "gif"),
    ] {
        let e = user(&r.daemon, "export_video", p).await.unwrap_err();
        assert_eq!(code(&e), ErrorCode::InvalidInput);
        assert!(e.to_string().contains(want), "{e}");
    }
    // A screenshot is not a video.
    let shot = user(
        &r.daemon,
        "capture_screenshot",
        json!({ "target": { "display_id": 1 } }),
    )
    .await
    .unwrap();
    let e = user(
        &r.daemon,
        "export_video",
        json!({ "capture_id": shot["capture_id"] }),
    )
    .await
    .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
    assert!(e.to_string().contains("screenshot"), "{e}");

    // No exporter registered: not_implemented, not a job that waits forever.
    let bare = rig(false);
    let s = start(&bare.daemon, json!({})).await;
    user(&bare.daemon, "stop_recording", json!({}))
        .await
        .unwrap();
    let e = user(
        &bare.daemon,
        "export_video",
        json!({ "capture_id": s["capture_id"] }),
    )
    .await
    .unwrap_err();
    assert_eq!(code(&e), ErrorCode::NotImplemented);
}

/// Writes the screen track's size and fps into a recording's manifest (the fake sources leave
/// them out; a real recording always has them).
fn give_screen_a_size(r: &Rig, title: &str, size: (u32, u32), fps: u32) {
    let p = r._dir.path().join(format!("{title}.cmrec/manifest.json"));
    let mut m: Value = serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
    m["tracks"]["screen"]["size"] = json!([size.0, size.1]);
    m["tracks"]["screen"]["fps"] = json!(fps);
    std::fs::write(&p, serde_json::to_string_pretty(&m).unwrap()).unwrap();
}

/// EXP-2 / EXP-3: `export_video` takes a preset id (or `format` gif / webm with width, fps and
/// max_mb), checks it against the recording before it queues anything (EXP-2.AC3), and keeps the
/// preset and its options in the job: a different preset or option is a different job.
#[tokio::test]
async fn exp2_export_video_checks_presets_and_keeps_them_in_the_job() {
    let r = rig(true);
    r.stub.block_first.store(true, Ordering::SeqCst);
    user(
        &r.daemon,
        "set_settings",
        json!({ "values": { "export.auto_on_stop": false } }),
    )
    .await
    .unwrap();
    let s = start(&r.daemon, json!({ "title": "P" })).await;
    let capture_id = s["capture_id"].as_str().unwrap().to_owned();
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
    give_screen_a_size(&r, "P", (1920, 1080), 30);
    let export = |extra: Value| {
        let mut p = json!({ "capture_id": capture_id });
        p.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let d = r.daemon.clone();
        async move { user(&d, "export_video", p).await }
    };

    // EXP-2.AC3: a 1080p recording cannot be exported as youtube-4k; the hint names the preset.
    let e = export(json!({ "preset": "youtube-4k" })).await.unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
    assert!(
        e.to_string().contains("enlarge") && e.to_string().contains("`youtube-1080p`"),
        "{e}"
    );
    assert!(
        export_jobs(&r.daemon).await.is_empty(),
        "nothing was queued"
    );

    // A platform preset is its own job, reused for an equal request.
    let yt = export(json!({ "preset": "youtube-1080p" })).await.unwrap();
    assert_eq!(yt["preset"], "youtube-1080p");
    assert_eq!(yt["reused"], false);
    assert_eq!(
        export(json!({ "preset": "youtube-1080p" })).await.unwrap()["job_id"],
        yt["job_id"]
    );
    let vertical = export(json!({ "preset": "vertical-1080" })).await.unwrap();
    assert_ne!(vertical["job_id"], yt["job_id"]);

    // GIF: `format` is the preset; the options are part of the job.
    let gif = export(json!({ "format": "gif", "width": 640, "fps": 10, "max_mb": 5 }))
        .await
        .unwrap();
    assert_eq!(gif["preset"], "gif");
    let j = user(&r.daemon, "get_job", json!({ "job_id": gif["job_id"] }))
        .await
        .unwrap();
    assert_eq!(
        (
            j["inputs"]["preset"].as_str(),
            j["inputs"]["width"].as_u64(),
            j["inputs"]["fps"].as_f64(),
            j["inputs"]["max_mb"].as_f64()
        ),
        (Some("gif"), Some(640), Some(10.0), Some(5.0))
    );
    let same = export(json!({ "preset": "gif", "width": 640, "fps": 10, "max_mb": 5 }))
        .await
        .unwrap();
    assert_eq!(same["job_id"], gif["job_id"]);
    assert_eq!(same["reused"], true);
    let other = export(json!({ "preset": "gif", "width": 480 }))
        .await
        .unwrap();
    assert_ne!(other["job_id"], gif["job_id"]);

    // Options the preset does not allow fail now, with the reason.
    for (p, want) in [
        (
            json!({ "preset": "gif", "width": 700 }),
            "480, 640, 800, 1024",
        ),
        (json!({ "preset": "gif", "fps": 12 }), "10, 15, 24"),
        (json!({ "preset": "youtube-1080p", "width": 640 }), "gif"),
        (json!({ "width": 640 }), "gif"),
    ] {
        let e = export(p).await.unwrap_err();
        assert_eq!(code(&e), ErrorCode::InvalidInput);
        assert!(e.to_string().contains(want), "{e}");
    }
    assert_eq!(export_jobs(&r.daemon).await.len(), 4);
    for id in [
        yt["job_id"].clone(),
        vertical["job_id"].clone(),
        gif["job_id"].clone(),
        other["job_id"].clone(),
    ] {
        let _ = user(&r.daemon, "cancel_job", json!({ "job_id": id })).await;
    }
}

// ---- the layout at stop ------------------------------------------------------------------------

fn manifest_of(r: &Rig, title: &str) -> Value {
    let p = r._dir.path().join(format!("{title}.cmrec/manifest.json"));
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}

/// The bubble layout at stop lands in `edits[0].layout.camera` as fractions of the recorded area
/// (1000x600 pt): a medium circle bottom-left (the default) keeps the 24 pt margin. A bubble changed during the
/// recording through the `camera.*` settings (what the desktop writes on a drag) is picked up; the
/// request's own values are not overridden by unrelated defaults.
#[tokio::test]
async fn layout_at_stop_is_stored_in_the_first_edit() {
    // A 2000x1200 px display at scale 2 is a 1000x600 pt area.
    let r = rig_on(
        false,
        cm_capture::fake::FakeBackend::new().with_display(1, 0, 0, 2000, 1200, 2.0),
    );
    start(
        &r.daemon,
        json!({ "title": "A", "camera": { "shape": "circle", "size": "m", "mirror": false } }),
    )
    .await;
    // Defaults in the settings must not override the request's bubble.
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
    let m = manifest_of(&r, "A");
    let cam = &m["edits"][0]["layout"]["camera"];
    assert_eq!(cam["visible"], true);
    assert_eq!(cam["shape"], "circle");
    assert_eq!(cam["mirror"], false);
    let rect = cam["rect"].as_array().unwrap();
    let f = |i: usize| rect[i].as_f64().unwrap();
    assert!((f(2) - 240.0 / 1000.0).abs() < 1e-3, "{cam}");
    // No `position` in the request: bottom-left, 24 pt from the left and bottom edges.
    assert!((f(0) - 24.0 / 1000.0).abs() < 1e-3, "{cam}");
    assert!((f(1) - (600.0 - 24.0 - 240.0) / 600.0).abs() < 1e-3);

    // Dragged during the recording: the desktop writes camera.position.
    start(
        &r.daemon,
        json!({ "title": "B", "camera": { "size": "s", "position": "top-left" } }),
    )
    .await;
    user(
        &r.daemon,
        "set_settings",
        json!({ "values": { "camera.position": [0.5, 0.5], "camera.shape": "rounded" } }),
    )
    .await
    .unwrap();
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
    let m = manifest_of(&r, "B");
    let cam = &m["edits"][0]["layout"]["camera"];
    assert_eq!(cam["shape"], "rounded");
    assert_eq!(cam["size"], "s");
    let rect = cam["rect"].as_array().unwrap();
    // Centre at the middle of the area: x = (0.5 * 1000 - 80) / 1000.
    assert!((rect[0].as_f64().unwrap() - 0.42).abs() < 1e-3, "{cam}");

    // No camera: no layout.
    start(&r.daemon, json!({ "title": "C" })).await;
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
    assert!(manifest_of(&r, "C")["edits"][0].get("layout").is_none());
}
