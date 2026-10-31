//! The recording engine through the daemon, over fake sources: the whole state machine of
//! REC-1, REC-4, REC-5, REC-8 and the agent rules of AGT-2 / NFR-11, with no display and no
//! devices. Real time with short ticks (a few milliseconds), a manual host clock for session time.

#![allow(clippy::unwrap_used)]

mod common;

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use cm_capture::disk::FakeFreeSpace;
use cm_core::{Actor, ErrorCode, ManualHostClock, Permission, TrackKind};
use cm_daemon::testing::{FakeSources, FixedConfirmer};
use cm_daemon::{
    ConfirmationRequest, Confirmer, Daemon, DaemonError, Decision, EngineOptions, Event,
};
use common::{builder, desktop};
use serde_json::{json, Value};
use tokio::sync::broadcast::Receiver;

const ORIGIN_NS: u64 = 5_000_000_000;
const SEC: u64 = 1_000_000_000;

struct Rig {
    daemon: Daemon,
    sources: FakeSources,
    host: Arc<ManualHostClock>,
    free: Arc<FakeFreeSpace>,
    dir: tempfile::TempDir,
    /// Where the 60 fps self-test writes its temporary files.
    _st_dir: tempfile::TempDir,
    confirmer: Option<Arc<FixedConfirmer>>,
}

struct Shared(Arc<FixedConfirmer>);

#[async_trait]
impl Confirmer for Shared {
    async fn confirm(&self, r: &ConfirmationRequest) -> Decision {
        self.0.confirm(r).await
    }
}

fn options(host: Arc<ManualHostClock>, free: Arc<FakeFreeSpace>) -> EngineOptions {
    EngineOptions {
        tick_every: Duration::from_millis(15),
        countdown_step: Duration::from_millis(40),
        health_every_ticks: 2,
        stop_timeout: Duration::from_secs(10),
        shutdown_timeout: Duration::from_secs(10),
        host,
        free_space: free,
        disk_check_every: Duration::ZERO,
        own_pid: None,
        selftest_duration: Duration::from_millis(120),
        ..EngineOptions::default()
    }
}

fn rig_with(sources: FakeSources, decision: Option<Decision>) -> Rig {
    rig_tuned(sources, decision, |_| {})
}

fn rig_tuned(
    sources: FakeSources,
    decision: Option<Decision>,
    tune: impl FnOnce(&mut EngineOptions),
) -> Rig {
    let (b, dir) = builder(desktop());
    let host = Arc::new(ManualHostClock::new(ORIGIN_NS));
    let free = Arc::new(FakeFreeSpace::new(500_000_000_000));
    let queue = cm_daemon::library_job_queue(dir.path()).unwrap();
    let st_dir = tempfile::tempdir().unwrap();
    let mut opts = options(host.clone(), free.clone());
    opts.selftest_dir = Some(st_dir.path().to_owned());
    tune(&mut opts);
    let mut b = b
        .jobs(queue)
        .recording_sources(sources.clone())
        .recording_options(opts);
    let confirmer = decision.map(|d| Arc::new(FixedConfirmer::new(d)));
    if let Some(c) = &confirmer {
        b = b.confirmer(Shared(c.clone()));
    }
    Rig {
        daemon: b.build(),
        sources,
        host,
        free,
        dir,
        _st_dir: st_dir,
        confirmer,
    }
}

fn rig() -> Rig {
    rig_with(FakeSources::new(), None)
}

fn agent(name: &str) -> Actor {
    Actor::agent(name).unwrap()
}

async fn user(d: &Daemon, method: &str, params: Value) -> Result<Value, DaemonError> {
    d.execute_method(Actor::User, 0, method, &params).await
}

fn start_params() -> Value {
    json!({ "target": { "display_id": 1 }, "countdown_s": 0 })
}

async fn start(r: &Rig, extra: Value) -> Value {
    let mut p = start_params();
    if let (Some(o), Some(e)) = (p.as_object_mut(), extra.as_object()) {
        for (k, v) in e {
            o.insert(k.clone(), v.clone());
        }
    }
    user(&r.daemon, "start_recording", p).await.unwrap()
}

/// Runs the 60 fps self-test as the user (the fake sources pass it) with these extra params.
async fn pass_selftest(r: &Rig, extra: Value) -> Value {
    let mut p = json!({ "target": { "display_id": 1 } });
    if let (Some(o), Some(e)) = (p.as_object_mut(), extra.as_object()) {
        for (k, v) in e {
            o.insert(k.clone(), v.clone());
        }
    }
    let v = user(&r.daemon, "run_fps_selftest", p).await.unwrap();
    assert_eq!(v["state"], "passed", "{v}");
    v
}

fn err_code(e: &DaemonError) -> ErrorCode {
    e.code().unwrap_or_else(|| panic!("no shared code: {e:?}"))
}

/// Polls `f` (every 10 ms, up to 10 s) until it returns `Some`.
async fn until<T>(mut f: impl FnMut() -> Option<T>) -> T {
    for _ in 0..1000 {
        if let Some(v) = f() {
            return v;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("condition not reached in 10 s");
}

async fn status_where(d: &Daemon, ok: impl Fn(&Value) -> bool) -> Value {
    for _ in 0..1000 {
        let s = user(d, "get_recording_status", json!({})).await.unwrap();
        if ok(&s) {
            return s;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("status never matched");
}

fn drain(rx: &mut Receiver<Event>) -> Vec<Event> {
    let mut v = Vec::new();
    while let Ok(e) = rx.try_recv() {
        v.push(e);
    }
    v
}

fn names(events: &[Event]) -> Vec<&str> {
    events.iter().map(|e| e.name.as_str()).collect()
}

async fn wait_event(rx: &mut Receiver<Event>, name: &str) -> Event {
    let wait = async {
        loop {
            let e = rx.recv().await.unwrap();
            if e.name == name {
                return e;
            }
        }
    };
    tokio::time::timeout(Duration::from_secs(10), wait)
        .await
        .unwrap_or_else(|_| panic!("no `{name}` event"))
}

fn bundles(r: &Rig) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(r.dir.path())
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".cmrec"))
        .collect();
    v.sort();
    v
}

async fn library_recordings(d: &Daemon) -> Vec<Value> {
    let r = user(d, "list_library", json!({ "kind": "recording" }))
        .await
        .unwrap();
    r["captures"].as_array().cloned().unwrap_or_default()
}

fn manifest(r: &Rig, record_path: &str) -> Value {
    let text =
        std::fs::read_to_string(r.dir.path().join(record_path).join("manifest.json")).unwrap();
    serde_json::from_str(&text).unwrap()
}

// ---- REC-1: start, stop -------------------------------------------------------------------

/// REC-1: start a recording with every sensor, watch it, stop it. The result carries what the
/// spec's tool table promises (recording id, capture id, duration, tracks) and the bundle is in
/// the library with a closed manifest. AUD-1 / AUD-2 / CAM-1 inputs reach the tracks.
#[tokio::test]
async fn rec1_start_status_stop_with_every_sensor() {
    let r = rig();
    let mut rx = r.daemon.subscribe();
    let started = start(
        &r,
        json!({
            "mic": { "device_id": "fake-mic", "denoise": false },
            "camera": { "shape": "rounded", "size": "l", "position": "top-left" },
            "system_audio": true,
            "title": "Lesson 1",
        }),
    )
    .await;
    let rec_id = started["recording_id"].as_str().unwrap().to_owned();
    assert!(rec_id.starts_with("rec_"));
    let capture_id = started["capture_id"].as_str().unwrap().to_owned();
    assert_ne!(
        rec_id, capture_id,
        "a run and a library capture have different ids"
    );
    assert_eq!(started["state"], "recording");
    assert_eq!(
        started["tracks"],
        json!(["screen", "cursor", "mic", "system", "camera"])
    );
    // REC-5: the indicator facts for the HUD, the brackets and the tray.
    let ind = &started["indicator"];
    assert_eq!(ind["area"]["kind"], "display");
    assert_eq!(ind["area"]["display_id"], 1);
    assert_eq!(ind["output"]["fps"], 30);
    assert_eq!(ind["camera"]["shape"], "rounded");
    assert_eq!(ind["camera"]["position"], "top-left");
    assert_eq!(ind["started_by"]["kind"], "user");
    assert_eq!(ind["mic"], true);
    assert!(r.daemon.recording_active());

    let st = user(&r.daemon, "get_recording_status", json!({}))
        .await
        .unwrap();
    assert_eq!(st["state"], "recording");
    assert_eq!(st["recording_id"], rec_id.as_str());
    assert_eq!(st["capture_id"], capture_id.as_str());
    assert_eq!(st["tracks"].as_array().unwrap().len(), 5);

    // Time passes on the session clock.
    r.host.advance_ns(7 * SEC);
    let st = status_where(&r.daemon, |s| s["recorded_ms"] == 7000).await;
    assert_eq!(st["paused_ms"], 0);
    tokio::time::sleep(Duration::from_millis(80)).await; // a few ticks

    let stopped = user(
        &r.daemon,
        "stop_recording",
        json!({ "recording_id": rec_id }),
    )
    .await
    .unwrap();
    assert_eq!(stopped["capture_id"], capture_id.as_str());
    assert_eq!(stopped["reason"], "user");
    assert_eq!(stopped["duration_ms"], 7000);
    let kinds: Vec<&str> = stopped["tracks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["screen", "cursor", "mic", "system", "camera"]);
    assert!(!r.daemon.recording_active());

    // The bundle is indexed and its manifest is closed, with the reason and who started it.
    let lib = library_recordings(&r.daemon).await;
    assert_eq!(lib.len(), 1);
    assert_eq!(lib[0]["id"], capture_id.as_str());
    assert_eq!(lib[0]["title"], "Lesson 1");
    let m = manifest(&r, "Lesson 1.cmrec");
    assert_eq!(m["stop_reason"], "user");
    assert_eq!(m["started_by"], "user");
    assert!(!r.dir.path().join("Lesson 1.cmrec/recording.lock").exists());
    // The request is stored as used (settings).
    assert_eq!(m["settings"]["mic"]["denoise"], false);
    assert_eq!(m["settings"]["camera"]["shape"], "rounded");

    // The event stream tells the whole story, in order.
    let events = drain(&mut rx);
    let n = names(&events);
    // R3-4: the starting event (with the indicator) comes before the tracks are up.
    assert_eq!(n.first(), Some(&"recording.starting"));
    assert_eq!(n[1], "recording.started");
    assert_eq!(events[0].data["indicator"]["started_by"]["kind"], "user");
    assert!(events[0].data["tracks_planned"]
        .as_array()
        .unwrap()
        .iter()
        .any(|t| t == "camera"));
    assert!(n.contains(&"recording.stats"));
    let stop_at = n.iter().position(|e| *e == "recording.stopped").unwrap();
    assert_eq!(n[stop_at + 1], "capture.created");
    let ev = &events[stop_at];
    assert_eq!(ev.data["capture_id"], capture_id.as_str());
    assert_eq!(ev.data["reason"], "user");
    assert_eq!(ev.data["duration_s"], 7.0);
    assert_eq!(ev.data["tracks"].as_array().unwrap().len(), 5);
}

/// The default for everything not given: display, 30 fps, no camera, no mic, no system audio,
/// cursor telemetry on (REC-1, REC-3, REC-7).
#[tokio::test]
async fn rec1_defaults_record_screen_and_cursor_only() {
    let r = rig();
    let s = start(&r, json!({})).await;
    assert_eq!(s["tracks"], json!(["screen", "cursor"]));
    assert_eq!(s["indicator"]["mic"], false);
    assert_eq!(s["indicator"]["camera"], Value::Null);
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
}

/// REC-1 AC1/AC3: the plan is resolved against the targets: a region is 1280x720 physical px
/// relative to the display (the screenshot convention), and the HUD gets its global bounds.
#[tokio::test]
async fn rec1_ac3_region_target_uses_physical_px_relative_to_the_display() {
    let r = rig();
    let mut p = start_params();
    p["target"] = json!({ "display_id": 1, "rect": [100, 80, 200, 120] });
    let s = user(&r.daemon, "start_recording", p).await.unwrap();
    let area = &s["indicator"]["area"];
    assert_eq!(area["kind"], "region");
    assert_eq!(area["bounds"], json!([100, 80, 200, 120]));
    assert_eq!(area["source_size"], json!([200, 120]));
    assert_eq!(s["indicator"]["output"]["width"], 200);
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();

    // A region outside the display is invalid input, before anything is created.
    let mut p = start_params();
    p["target"] = json!({ "display_id": 1, "rect": [350, 250, 200, 200] });
    let e = user(&r.daemon, "start_recording", p).await.unwrap_err();
    assert_eq!(err_code(&e), ErrorCode::InvalidInput);
    // An unknown display is not_found.
    let mut p = start_params();
    p["target"] = json!({ "display_id": 99 });
    let e = user(&r.daemon, "start_recording", p).await.unwrap_err();
    assert_eq!(err_code(&e), ErrorCode::NotFound);
    assert_eq!(r.sources.builds(), 1, "failed plans never build tracks");
}

/// Window recording (REC-1.AC2): the plan follows the window's size at start.
#[tokio::test]
async fn rec1_ac2_window_target_is_planned_from_the_window() {
    let r = rig();
    let mut p = start_params();
    p["target"] = json!({ "window_id": 10 });
    let s = user(&r.daemon, "start_recording", p).await.unwrap();
    let area = &s["indicator"]["area"];
    assert_eq!(area["kind"], "window");
    assert_eq!(area["window_id"], 10);
    assert_eq!(area["bounds"], json!([20, 10, 100, 80]));
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
}

/// Settings fill what the request leaves out (`settings set recording.fps 60 ...`).
#[tokio::test]
async fn rec3_settings_provide_the_defaults() {
    let r = rig();
    user(
        &r.daemon,
        "set_settings",
        json!({ "values": { "recording.fps": 60, "recording.size": "720p" } }),
    )
    .await
    .unwrap();
    // A default of 60 without a passing self-test is not an error: it falls back to 30 fps and
    // says why (REC-3).
    let s = start(&r, json!({})).await;
    assert_eq!(s["indicator"]["output"]["fps"], 30);
    assert!(
        s["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w["code"] == "fps60_not_validated"),
        "{s}"
    );
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
    // After a pass for these settings the default holds.
    pass_selftest(&r, json!({})).await;
    let s = start(&r, json!({})).await;
    assert_eq!(s["indicator"]["output"]["fps"], 60);
    assert!(s["warnings"].as_array().unwrap().is_empty(), "{s}");
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
    // The request wins.
    let s = start(&r, json!({ "fps": 30 })).await;
    assert_eq!(s["indicator"]["output"]["fps"], 30);
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
}

// ---- REC-1.AC6: busy ----------------------------------------------------------------------

/// P3: the errors the app shows while recording have stable ids and parameters; the English
/// message (what the CLI and MCP print) is exactly as before.
#[tokio::test]
async fn p3_recording_errors_have_stable_message_ids() {
    use cm_core::message::describe;
    let id_of = |e: &DaemonError| {
        let code = err_code(e);
        describe(code, &e.to_string()).map(|p| (p.id, p.params))
    };
    let r = rig();
    // Nothing runs yet.
    let e = user(&r.daemon, "stop_recording", json!({}))
        .await
        .unwrap_err();
    assert_eq!(e.to_string(), "No recording is running");
    assert_eq!(id_of(&e).unwrap().0, "no_recording_running");
    // An unknown display carries its number.
    let mut p = start_params();
    p["target"] = json!({ "display_id": 99 });
    let e = user(&r.daemon, "start_recording", p).await.unwrap_err();
    assert_eq!(e.to_string(), "Display 99 not found");
    assert_eq!(
        id_of(&e).unwrap(),
        ("display_not_found", vec![("id", "99".to_owned())])
    );
    // A second start.
    start(&r, json!({})).await;
    let e = user(&r.daemon, "start_recording", start_params())
        .await
        .unwrap_err();
    assert_eq!(id_of(&e).unwrap().0, "recording_running");
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
    // Too little disk to start carries the free megabytes.
    r.free.set(900_000_000);
    let e = user(&r.daemon, "start_recording", start_params())
        .await
        .unwrap_err();
    assert_eq!(
        e.to_string(),
        "Only 900 MB free; recording needs more than 1 GB free to start"
    );
    assert_eq!(
        id_of(&e).unwrap(),
        ("disk_low_to_start", vec![("mb", "900".to_owned())])
    );
}

/// REC-1.AC6: a second start is refused with `busy` and the message of the spec, and nothing
/// else changes. It is refused before any dialog (an agent is not asked to approve a start that
/// cannot happen).
#[tokio::test]
async fn rec1_ac6_second_start_is_busy_and_never_asks() {
    let r = rig_with(FakeSources::new(), Some(Decision::Approve));
    let first = start(&r, json!({})).await;
    let e = r
        .daemon
        .execute_method(agent("claude"), 0, "start_recording", &start_params())
        .await
        .unwrap_err();
    assert_eq!(err_code(&e), ErrorCode::Busy);
    assert_eq!(e.to_string(), "A recording is already running");
    assert!(r.confirmer.as_ref().unwrap().shown().is_empty());
    assert_eq!(r.sources.builds(), 1);
    // The first one is untouched.
    let st = user(&r.daemon, "get_recording_status", json!({}))
        .await
        .unwrap();
    assert_eq!(st["recording_id"], first["recording_id"]);
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
}

/// Busy also covers the countdown: the slot is taken from the moment a start is accepted.
#[tokio::test]
async fn rec1_ac6_busy_during_the_countdown() {
    let r = rig();
    let d = r.daemon.clone();
    let first = tokio::spawn(async move {
        user(
            &d,
            "start_recording",
            json!({ "target": { "display_id": 1 }, "countdown_s": 5 }),
        )
        .await
    });
    status_where(&r.daemon, |s| s["state"] == "countdown").await;
    let e = user(&r.daemon, "start_recording", start_params())
        .await
        .unwrap_err();
    assert_eq!(err_code(&e), ErrorCode::Busy);
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
    assert!(first.await.unwrap().is_err());
}

// ---- REC-4: countdown ---------------------------------------------------------------------

/// REC-4: the countdown announces each second and recording starts after it. REC-1.AC5: skipping
/// it starts at once.
#[tokio::test]
async fn rec4_countdown_events_then_start_and_skip() {
    let r = rig();
    let mut rx = r.daemon.subscribe();
    let s = user(
        &r.daemon,
        "start_recording",
        json!({ "target": { "display_id": 1 }, "countdown_s": 3 }),
    )
    .await
    .unwrap();
    assert_eq!(s["countdown_s"], 3);
    let events = drain(&mut rx);
    let remaining: Vec<i64> = events
        .iter()
        .filter(|e| e.name == "recording.countdown")
        .map(|e| e.data["remaining"].as_i64().unwrap())
        .collect();
    assert_eq!(remaining, [3, 2, 1]);
    assert_eq!(
        events.last().map(|e| e.name.as_str()),
        Some("recording.started")
    );
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();

    // Skip (Space): a long countdown ends at once. Seconds-long steps (a 20 s countdown) so that
    // "skipped" and "waited out" are far apart whatever the machine load (flaky-test fix).
    let r = rig_tuned(FakeSources::new(), None, |o| {
        o.countdown_step = Duration::from_secs(2);
    });
    let d = r.daemon.clone();
    let slow = tokio::spawn(async move {
        user(
            &d,
            "start_recording",
            json!({ "target": { "display_id": 1 }, "countdown_s": 10 }),
        )
        .await
    });
    status_where(&r.daemon, |s| s["state"] == "countdown").await;
    let t0 = std::time::Instant::now();
    user(&r.daemon, "skip_countdown", json!({})).await.unwrap();
    slow.await.unwrap().unwrap();
    assert!(
        t0.elapsed() < Duration::from_secs(10),
        "skipped, not waited out (20 s): {:?}",
        t0.elapsed()
    );
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
}

/// REC-4: stop, discard and the pause shortcut during the countdown cancel it: nothing was
/// created (no bundle folder, no tracks built), the starter gets a decline, the engine is idle.
#[tokio::test]
async fn rec4_ac4_cancelling_the_countdown_creates_nothing() {
    for how in ["stop_recording", "discard_recording", "pause_recording"] {
        let r = rig();
        let mut rx = r.daemon.subscribe();
        let d = r.daemon.clone();
        let starting = tokio::spawn(async move {
            user(
                &d,
                "start_recording",
                json!({ "target": { "display_id": 1 }, "countdown_s": 10 }),
            )
            .await
        });
        status_where(&r.daemon, |s| s["state"] == "countdown").await;
        let ans = user(&r.daemon, how, json!({})).await.unwrap();
        assert_eq!(ans["cancelled"], true, "{how}");
        let e = starting.await.unwrap().unwrap_err();
        assert_eq!(err_code(&e), ErrorCode::UserDeclined, "{how}");
        assert!(matches!(
            e,
            DaemonError::Declined {
                reason: "countdown_cancelled",
                ..
            }
        ));
        assert!(bundles(&r).is_empty(), "{how}: no files");
        assert_eq!(r.sources.builds(), 0, "{how}: no tracks built");
        assert!(!r.daemon.recording_active());
        assert!(!r.daemon.is_job_type_held("export"), "{how}: no hold");
        let events = drain(&mut rx);
        assert!(names(&events).contains(&"recording.discarded"), "{how}");
        assert!(!names(&events).contains(&"recording.started"), "{how}");
        // The next start works.
        start(&r, json!({})).await;
        user(&r.daemon, "stop_recording", json!({})).await.unwrap();
    }
}

/// If the caller of a start with a countdown goes away, the countdown is cancelled (an agent that
/// disconnects does not leave a recording about to begin).
#[tokio::test]
async fn rec4_countdown_is_cancelled_when_the_starter_goes_away() {
    let r = rig();
    let d = r.daemon.clone();
    let starting = tokio::spawn(async move {
        user(
            &d,
            "start_recording",
            json!({ "target": { "display_id": 1 }, "countdown_s": 10 }),
        )
        .await
    });
    status_where(&r.daemon, |s| s["state"] == "countdown").await;
    starting.abort();
    until(|| (!r.daemon.recording_active()).then_some(())).await;
    assert!(bundles(&r).is_empty());
}

// ---- REC-4: pause and resume --------------------------------------------------------------

/// REC-4.AC1: 10 s recorded, 20 s paused, 10 s recorded: the file is 20 s and the timer shows
/// 00:20. The paused time is not recorded time, and each track hears about the pause.
#[tokio::test]
async fn rec4_ac1_pause_resume_timer_and_duration() {
    let r = rig();
    user(
        &r.daemon,
        "set_settings",
        json!({ "values": { "recording.resume_countdown_s": 0 } }),
    )
    .await
    .unwrap();
    let mut rx = r.daemon.subscribe();
    start(&r, json!({ "mic": true })).await;
    r.host.advance_ns(10 * SEC);
    let p = user(&r.daemon, "pause_recording", json!({})).await.unwrap();
    assert_eq!(p["state"], "paused");
    assert_eq!(p["recorded_ms"], 10_000);
    r.host.advance_ns(20 * SEC);
    let st = status_where(&r.daemon, |s| s["paused_ms"] == 20_000).await;
    assert_eq!(st["state"], "paused");
    assert_eq!(st["recorded_ms"], 10_000, "the timer stops while paused");
    // Pausing a paused recording changes nothing.
    let again = user(&r.daemon, "pause_recording", json!({})).await.unwrap();
    assert_eq!(again["changed"], false);

    let res = user(&r.daemon, "resume_recording", json!({}))
        .await
        .unwrap();
    assert_eq!(res["state"], "recording");
    assert_eq!(res["paused_ms"], 20_000);
    r.host.advance_ns(10 * SEC);
    let st = status_where(&r.daemon, |s| s["recorded_ms"] == 20_000).await;
    assert_eq!(st["state"], "recording");
    assert_eq!(st["pauses"], 1);

    let stopped = user(&r.daemon, "stop_recording", json!({})).await.unwrap();
    assert_eq!(stopped["duration_ms"], 20_000, "20 s in the file");
    assert_eq!(stopped["paused_ms"], 20_000);
    for kind in [TrackKind::Screen, TrackKind::Mic, TrackKind::Cursor] {
        let calls = r.sources.handle(kind).unwrap().calls();
        assert_eq!(calls, ["start", "pause", "resume", "stop"], "{kind:?}");
    }
    let events = drain(&mut rx);
    let n = names(&events);
    let p = n.iter().position(|e| *e == "recording.paused").unwrap();
    let q = n.iter().position(|e| *e == "recording.resumed").unwrap();
    assert!(p < q);
}

/// REC-4: stopping while paused closes the gap at the stop time and keeps what was recorded.
#[tokio::test]
async fn rec4_stop_while_paused() {
    let r = rig();
    start(&r, json!({})).await;
    r.host.advance_ns(5 * SEC);
    user(&r.daemon, "pause_recording", json!({})).await.unwrap();
    r.host.advance_ns(3 * SEC);
    let stopped = user(&r.daemon, "stop_recording", json!({})).await.unwrap();
    assert_eq!(stopped["duration_ms"], 5000);
    assert_eq!(stopped["paused_ms"], 3000);
}

/// REC-4: the resume countdown (default 1 s) runs while paused, shows its seconds, and a pause
/// during it cancels the resume.
#[tokio::test]
async fn rec4_resume_countdown_and_its_cancel() {
    let r = rig();
    user(
        &r.daemon,
        "set_settings",
        json!({ "values": { "recording.resume_countdown_s": 3 } }),
    )
    .await
    .unwrap();
    let mut rx = r.daemon.subscribe();
    start(&r, json!({})).await;
    user(&r.daemon, "pause_recording", json!({})).await.unwrap();
    drain(&mut rx);

    let d = r.daemon.clone();
    let resuming = tokio::spawn(async move { user(&d, "resume_recording", json!({})).await });
    status_where(&r.daemon, |s| s["state"] == "resuming").await;
    user(&r.daemon, "pause_recording", json!({})).await.unwrap();
    let cancelled = resuming.await.unwrap().unwrap();
    assert_eq!(cancelled["cancelled"], true);
    let st = user(&r.daemon, "get_recording_status", json!({}))
        .await
        .unwrap();
    assert_eq!(st["state"], "paused");
    let ev = drain(&mut rx);
    assert!(ev
        .iter()
        .any(|e| e.name == "recording.countdown" && e.data["context"] == "resume"));
    assert!(!names(&ev).contains(&"recording.resumed"));

    // Resume again and let it run: three seconds of countdown, then recording.
    let t0 = std::time::Instant::now();
    user(&r.daemon, "resume_recording", json!({}))
        .await
        .unwrap();
    assert!(t0.elapsed() >= Duration::from_millis(100));
    let st = user(&r.daemon, "get_recording_status", json!({}))
        .await
        .unwrap();
    assert_eq!(st["state"], "recording");
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
}

/// REC-4: at most 500 pauses.
#[tokio::test]
async fn rec4_pause_limit_is_500() {
    let r = rig();
    user(
        &r.daemon,
        "set_settings",
        json!({ "values": { "recording.resume_countdown_s": 0 } }),
    )
    .await
    .unwrap();
    start(&r, json!({})).await;
    for _ in 0..500 {
        user(&r.daemon, "pause_recording", json!({})).await.unwrap();
        user(&r.daemon, "resume_recording", json!({}))
            .await
            .unwrap();
    }
    let e = user(&r.daemon, "pause_recording", json!({}))
        .await
        .unwrap_err();
    assert_eq!(err_code(&e), ErrorCode::InvalidInput);
    assert!(e.to_string().contains("Too many pauses"), "{e}");
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
}

// ---- permissions, partial failure ----------------------------------------------------------

/// REC-1 edge: a missing permission fails before anything is created and without prompting, for
/// screen, microphone and camera; Input Monitoring is only a warning (REC-7.AC4).
#[tokio::test]
async fn rec1_missing_permissions_fail_before_any_file() {
    for (perm, extra, name) in [
        (Permission::Screen, json!({}), "screen"),
        (Permission::Microphone, json!({ "mic": true }), "microphone"),
        (Permission::Camera, json!({ "camera": true }), "camera"),
    ] {
        let r = rig_with(
            FakeSources::new().with_missing_permission(perm),
            Some(Decision::Approve),
        );
        let mut p = start_params();
        for (k, v) in extra.as_object().unwrap() {
            p[k] = v.clone();
        }
        // As an agent: the error comes before the dialog.
        let e = r
            .daemon
            .execute_method(agent("claude"), 0, "start_recording", &p)
            .await
            .unwrap_err();
        assert_eq!(err_code(&e), ErrorCode::PermissionMissing, "{name}");
        assert_eq!(e.to_rpc_error()["data"]["data"]["permission"], name);
        assert!(
            r.confirmer.as_ref().unwrap().shown().is_empty(),
            "{name}: no dialog"
        );
        assert!(bundles(&r).is_empty(), "{name}: no files");
        assert_eq!(r.sources.builds(), 0);
        assert!(!r.daemon.recording_active());
        assert!(!r.daemon.is_job_type_held("export"));
    }
    // A camera permission only matters when the camera is asked for.
    let r = rig_with(
        FakeSources::new().with_missing_permission(Permission::Camera),
        None,
    );
    start(&r, json!({})).await;
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
}

#[tokio::test]
async fn rec7_ac4_input_monitoring_missing_is_a_warning() {
    let r = rig_with(FakeSources::new().with_cursor_warning(), None);
    let mut rx = r.daemon.subscribe();
    let s = start(&r, json!({})).await;
    let w = s["warnings"].as_array().unwrap();
    assert!(w
        .iter()
        .any(|w| w["code"] == "input_monitoring_missing" && w["track"] == "cursor"));
    assert!(s["tracks"].as_array().unwrap().contains(&json!("cursor")));
    let ev = drain(&mut rx);
    assert!(ev
        .iter()
        .any(|e| e.name == "recording.warning" && e.data["code"] == "input_monitoring_missing"));
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
}

/// REC-1 / session: an optional track that fails to start is a warning, not a failure.
#[tokio::test]
async fn rec1_optional_track_failure_is_a_warning_and_the_recording_goes_on() {
    let r = rig_with(
        FakeSources::new().with_failing_start(TrackKind::Camera),
        None,
    );
    let s = start(&r, json!({ "camera": true, "mic": true })).await;
    assert_eq!(s["tracks"], json!(["screen", "cursor", "mic"]));
    let w = s["warnings"].as_array().unwrap();
    assert!(w.iter().any(|w| w["code"] == "camera_failed"), "{w:?}");
    let stopped = user(&r.daemon, "stop_recording", json!({})).await.unwrap();
    assert!(stopped["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|w| w["code"] == "camera_failed"));
    let lib = library_recordings(&r.daemon).await;
    assert_eq!(lib.len(), 1);
}

/// The screen track is required: if it cannot start nothing is recorded, the bundle is removed,
/// the holds are released and the engine is idle again.
#[tokio::test]
async fn rec1_screen_failure_aborts_and_cleans_up() {
    let r = rig_with(
        FakeSources::new().with_failing_start(TrackKind::Screen),
        None,
    );
    let e = user(&r.daemon, "start_recording", start_params())
        .await
        .unwrap_err();
    assert_eq!(err_code(&e), ErrorCode::DeviceBusy);
    assert!(bundles(&r).is_empty(), "the half-created bundle is removed");
    assert!(!r.daemon.recording_active());
    assert!(!r.daemon.is_job_type_held("export"));
    assert!(!r.daemon.is_job_type_held("upload"));
    let st = user(&r.daemon, "get_recording_status", json!({}))
        .await
        .unwrap();
    assert_eq!(st["state"], "idle");
}

// ---- automatic stops ----------------------------------------------------------------------

/// REC-1 edge: the screen source halts (window closed, display gone): the recording stops by
/// itself, keeps what it has, and says why (`recording.stopped.reason`, manifest `stop_reason`).
#[tokio::test]
async fn rec1_screen_halt_stops_cleanly_with_the_reason() {
    let r = rig();
    let mut rx = r.daemon.subscribe();
    let s = start(&r, json!({})).await;
    let rec_id = s["recording_id"].as_str().unwrap().to_owned();
    r.host.advance_ns(4 * SEC);
    r.sources
        .handle(TrackKind::Screen)
        .unwrap()
        .halt("screen_halted", "Screen capture ended: window closed");
    let ev = wait_event(&mut rx, "recording.stopped").await;
    assert_eq!(ev.data["reason"], "screen_halted");
    assert_eq!(ev.data["duration_s"], 4.0);
    until(|| (!r.daemon.recording_active()).then_some(())).await;
    let lib = library_recordings(&r.daemon).await;
    assert_eq!(lib.len(), 1);
    let path = bundles(&r).pop().unwrap();
    assert_eq!(manifest(&r, &path)["stop_reason"], "screen_halted");
    // A stop call that arrives afterwards gets the same answer, not an error.
    let late = user(
        &r.daemon,
        "stop_recording",
        json!({ "recording_id": rec_id }),
    )
    .await
    .unwrap();
    assert_eq!(late["already_stopped"], true);
    assert_eq!(late["reason"], "screen_halted");
    assert_eq!(late["capture_id"], ev.data["capture_id"]);
    // Without an id there is nothing running.
    let e = user(&r.daemon, "stop_recording", json!({}))
        .await
        .unwrap_err();
    assert_eq!(err_code(&e), ErrorCode::NotFound);
}

/// REC-8.AC3 / NFR-7: under 1 GB free the recording stops cleanly and the file is valid; the
/// low-space warning comes first.
#[tokio::test]
async fn rec8_ac3_disk_full_stops_cleanly() {
    let r = rig();
    let mut rx = r.daemon.subscribe();
    start(&r, json!({})).await;
    r.free.set(900_000_000);
    let ev = wait_event(&mut rx, "recording.stopped").await;
    assert_eq!(ev.data["reason"], "disk_full");
    assert!(ev.data["capture_id"].is_string());
    let path = bundles(&r).pop().unwrap();
    let m = manifest(&r, &path);
    assert_eq!(m["stop_reason"], "disk_full");
    assert!(
        m["warnings"]
            .as_array()
            .is_some_and(|w| w.iter().any(|w| w["code"] == "disk_full")),
        "disk_full is also a warning in the manifest"
    );
}

/// `max_duration_s` (agent input): counted in recorded time, so pauses do not eat it.
#[tokio::test]
async fn rec1_max_duration_stops_automatically() {
    let r = rig();
    user(
        &r.daemon,
        "set_settings",
        json!({ "values": { "recording.resume_countdown_s": 0 } }),
    )
    .await
    .unwrap();
    let mut rx = r.daemon.subscribe();
    start(&r, json!({ "max_duration_s": 5 })).await;
    r.host.advance_ns(3 * SEC);
    user(&r.daemon, "pause_recording", json!({})).await.unwrap();
    r.host.advance_ns(60 * SEC);
    tokio::time::sleep(Duration::from_millis(80)).await;
    assert!(r.daemon.recording_active(), "paused time does not count");
    user(&r.daemon, "resume_recording", json!({}))
        .await
        .unwrap();
    r.host.advance_ns(3 * SEC);
    let ev = wait_event(&mut rx, "recording.stopped").await;
    assert_eq!(ev.data["reason"], "max_duration");
}

/// Track warnings reach the stream and the status, and the health event reports `degraded`
/// semantics through `status` (REC-3.AC4 shape).
#[tokio::test]
async fn rec3_warnings_stats_and_health_events() {
    let r = rig();
    let mut rx = r.daemon.subscribe();
    start(&r, json!({ "mic": true })).await;
    let mic = r.sources.handle(TrackKind::Mic).unwrap();
    mic.set_level(0.5);
    mic.warn("audio_device_changed", "Audio device changed");
    let w = wait_event(&mut rx, "recording.warning").await;
    assert_eq!(w.data["code"], "audio_device_changed");
    assert_eq!(w.data["track"], "mic");
    let h = wait_event(&mut rx, "recording.health").await;
    assert_eq!(h.data["status"], "ok");
    assert!(h.data["late_frame_pct"].is_number());
    let stats = wait_event(&mut rx, "recording.stats").await;
    assert_eq!(stats.data["state"], "recording");
    let st = status_where(&r.daemon, |s| {
        s["mic_level_dbfs"].as_f64().is_some_and(|d| d > -12.0)
    })
    .await;
    // AUD-1: the HUD meter follows the live mic input.
    assert!(st["mic_level_dbfs"].as_f64().unwrap() < 0.0);
    assert!(st["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|w| w["code"] == "audio_device_changed"));
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
}

// ---- discard and restart ------------------------------------------------------------------

/// REC-5.AC3: Discard deletes the take: no folder, nothing in the library, nothing to recover,
/// the holds are released, and `recording.discarded` is published.
#[tokio::test]
async fn rec5_ac3_discard_deletes_the_bundle() {
    let r = rig();
    let mut rx = r.daemon.subscribe();
    let s = start(&r, json!({ "mic": true })).await;
    assert_eq!(bundles(&r).len(), 1);
    assert!(r.daemon.is_job_type_held("export"));
    let d = user(&r.daemon, "discard_recording", json!({}))
        .await
        .unwrap();
    assert_eq!(d["discarded"], true);
    assert_eq!(d["capture_id"], s["capture_id"]);
    assert!(bundles(&r).is_empty());
    assert!(library_recordings(&r.daemon).await.is_empty());
    assert!(!r.daemon.recording_active());
    assert!(!r.daemon.is_job_type_held("export"));
    let ev = drain(&mut rx);
    let disc = ev.iter().find(|e| e.name == "recording.discarded").unwrap();
    assert_eq!(disc.data["capture_id"], s["capture_id"]);
    assert!(!names(&ev).contains(&"recording.stopped"));
    for kind in [TrackKind::Screen, TrackKind::Mic] {
        // The tracks were told to stop before the folder went away.
        assert_eq!(
            r.sources.handle(kind).unwrap().calls().last().unwrap(),
            "stop"
        );
    }
}

/// REC-5.AC12 (OD-4): Restart discards the take and starts a new recording with the same settings
/// and a new recording id, after a new countdown; the job holds stay on across the gap.
#[tokio::test]
async fn rec5_ac12_restart_discards_and_starts_again_with_the_same_settings() {
    let r = rig();
    user(
        &r.daemon,
        "set_settings",
        json!({ "values": { "recording.countdown_s": 2 } }),
    )
    .await
    .unwrap();
    let mut rx = r.daemon.subscribe();
    pass_selftest(&r, json!({ "target": { "display_id": 2 } })).await;
    let first = start(
        &r,
        json!({ "mic": true, "fps": 60, "target": { "display_id": 2 }, "countdown_s": 0 }),
    )
    .await;
    let old_bundle = bundles(&r);
    assert_eq!(old_bundle.len(), 1);
    drain(&mut rx);

    let second = user(&r.daemon, "restart_recording", json!({}))
        .await
        .unwrap();
    assert_ne!(second["recording_id"], first["recording_id"]);
    assert_ne!(second["capture_id"], first["capture_id"]);
    assert_eq!(second["restart_of"], first["recording_id"]);
    assert_eq!(second["indicator"]["area"]["display_id"], 2, "same target");
    assert_eq!(second["indicator"]["output"]["fps"], 60, "same fps");
    assert_eq!(second["tracks"], first["tracks"], "same sensors");
    assert_eq!(
        second["countdown_s"], 2,
        "the countdown setting, not the old request"
    );
    // Only the new take is on disk.
    let now = bundles(&r);
    assert_eq!(now.len(), 1);
    assert_eq!(r.sources.builds(), 3, "the self-test, then two takes");
    let ev = drain(&mut rx);
    let n = names(&ev);
    let discarded = n.iter().position(|e| *e == "recording.discarded").unwrap();
    let countdown = n.iter().position(|e| *e == "recording.countdown").unwrap();
    let started = n.iter().position(|e| *e == "recording.started").unwrap();
    assert!(discarded < countdown && countdown < started, "{n:?}");
    assert!(
        r.daemon.is_job_type_held("export"),
        "holds survive the restart"
    );
    // The discarded take is gone for good; the new one stops normally.
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
    let lib = library_recordings(&r.daemon).await;
    assert_eq!(lib.len(), 1);
    assert_eq!(lib[0]["id"], second["capture_id"]);
}

/// REC-5.AC12: Esc during the new countdown ends the attempt with nothing recorded.
#[tokio::test]
async fn rec5_restart_cancelled_in_its_countdown_leaves_nothing() {
    let r = rig();
    user(
        &r.daemon,
        "set_settings",
        json!({ "values": { "recording.countdown_s": 10 } }),
    )
    .await
    .unwrap();
    start(&r, json!({})).await;
    let d = r.daemon.clone();
    let restarting = tokio::spawn(async move { user(&d, "restart_recording", json!({})).await });
    status_where(&r.daemon, |s| s["state"] == "countdown").await;
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
    let e = restarting.await.unwrap().unwrap_err();
    assert_eq!(err_code(&e), ErrorCode::UserDeclined);
    assert!(bundles(&r).is_empty());
    assert!(library_recordings(&r.daemon).await.is_empty());
    assert!(!r.daemon.is_job_type_held("export"), "holds are released");
    let e = user(&r.daemon, "restart_recording", json!({}))
        .await
        .unwrap_err();
    assert_eq!(err_code(&e), ErrorCode::NotFound);
}

// ---- jobs ---------------------------------------------------------------------------------

/// EXP-4.AC2 / STO-3.AC3: export, transcribe, upload and publish jobs are held while recording
/// and released when it stops. A hold somebody else owns is not released by us; the upload and
/// publish holds follow `storage.pause_uploads_while_recording`.
#[tokio::test]
async fn exp4_ac2_jobs_are_held_while_recording() {
    let r = rig();
    let types = ["export", "transcribe", "upload", "publish"];
    assert!(types.iter().all(|t| !r.daemon.is_job_type_held(t)));
    start(&r, json!({})).await;
    assert!(types.iter().all(|t| r.daemon.is_job_type_held(t)));
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
    assert!(types.iter().all(|t| !r.daemon.is_job_type_held(t)));

    // "Pause uploads while recording" off.
    user(
        &r.daemon,
        "set_settings",
        json!({ "values": { "storage.pause_uploads_while_recording": false } }),
    )
    .await
    .unwrap();
    start(&r, json!({})).await;
    assert!(r.daemon.is_job_type_held("export"));
    assert!(!r.daemon.is_job_type_held("upload"));
    assert!(!r.daemon.is_job_type_held("publish"));
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();

    // A hold that was already there stays after the recording.
    assert!(r.daemon.hold_job_type("export"));
    start(&r, json!({})).await;
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
    assert!(r.daemon.is_job_type_held("export"), "not ours to release");
}

/// Holds are released when the recording fails to stop, when it is shut down and when the
/// engine's driver task panics.
#[tokio::test]
async fn exp4_holds_are_released_on_every_way_out() {
    // Stop fails.
    let r = rig();
    start(&r, json!({})).await;
    r.sources.handle(TrackKind::Screen).unwrap().fail_stop(true);
    let e = user(&r.daemon, "stop_recording", json!({}))
        .await
        .unwrap_err();
    assert_eq!(err_code(&e), ErrorCode::Internal);
    assert!(!r.daemon.is_job_type_held("export"));
    assert!(!r.daemon.recording_active());

    // Shutdown mid-recording.
    let r = rig();
    start(&r, json!({})).await;
    r.daemon.shutdown_recording().await;
    assert!(!r.daemon.is_job_type_held("export"));
    assert!(!r.daemon.recording_active());
}

// ---- shutdown, panic, stop failure --------------------------------------------------------

/// REC-8 / NFR-6: the daemon shutting down with a recording running finalizes it within a
/// bounded time, so the user keeps what was recorded.
#[tokio::test]
async fn rec8_shutdown_mid_recording_finalizes_the_bundle() {
    let r = rig();
    let mut rx = r.daemon.subscribe();
    let s = start(&r, json!({ "mic": true })).await;
    r.host.advance_ns(2 * SEC);
    let t0 = std::time::Instant::now();
    r.daemon.shutdown_recording().await;
    assert!(t0.elapsed() < Duration::from_secs(2));
    assert!(!r.daemon.recording_active());
    let ev = drain(&mut rx);
    let stopped = ev.iter().find(|e| e.name == "recording.stopped").unwrap();
    assert_eq!(stopped.data["reason"], "shutdown");
    assert_eq!(stopped.data["capture_id"], s["capture_id"]);
    let lib = library_recordings(&r.daemon).await;
    assert_eq!(lib.len(), 1, "indexed with a closed manifest");
    assert!(!r
        .dir
        .path()
        .join(bundles(&r).pop().unwrap())
        .join("recording.lock")
        .exists());
    // Shutting down an idle engine is a no-op.
    r.daemon.shutdown_recording().await;
    // A countdown in progress is cancelled by a shutdown.
    let r = rig();
    let d = r.daemon.clone();
    let starting = tokio::spawn(async move {
        user(
            &d,
            "start_recording",
            json!({ "target": { "display_id": 1 }, "countdown_s": 10 }),
        )
        .await
    });
    status_where(&r.daemon, |s| s["state"] == "countdown").await;
    r.daemon.shutdown_recording().await;
    assert!(starting.await.unwrap().is_err());
    assert!(bundles(&r).is_empty());
}

/// A panic in the engine's driver task: the supervisor finalizes the recording
/// (`engine_failed`), releases the holds and the engine accepts a new recording.
#[tokio::test]
async fn engine_panic_is_contained_and_the_recording_is_saved() {
    let r = rig();
    let mut rx = r.daemon.subscribe();
    let s = start(&r, json!({})).await;
    r.host.advance_ns(3 * SEC);
    r.sources.set_panic_on_tick(true);
    let ev = wait_event(&mut rx, "recording.stopped").await;
    assert_eq!(ev.data["reason"], "engine_failed");
    assert_eq!(ev.data["capture_id"], s["capture_id"]);
    until(|| (!r.daemon.recording_active()).then_some(())).await;
    assert!(!r.daemon.is_job_type_held("export"));
    let lib = library_recordings(&r.daemon).await;
    assert_eq!(lib.len(), 1);
    // The engine works again.
    r.sources.set_panic_on_tick(false);
    start(&r, json!({})).await;
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
    assert_eq!(library_recordings(&r.daemon).await.len(), 2);
}

/// A stop that fails leaves the bundle with its lock (the next launch recovers it, REC-8) and
/// the engine idle and usable.
#[tokio::test]
async fn rec8_failed_stop_keeps_the_lock_for_recovery() {
    let r = rig();
    let mut rx = r.daemon.subscribe();
    start(&r, json!({})).await;
    r.sources.handle(TrackKind::Screen).unwrap().fail_stop(true);
    let e = user(&r.daemon, "stop_recording", json!({}))
        .await
        .unwrap_err();
    assert_eq!(err_code(&e), ErrorCode::Internal);
    let dir = r.dir.path().join(bundles(&r).pop().unwrap());
    assert!(dir.join("recording.lock").exists());
    assert!(!dir.join("manifest.json").exists());
    let ev = drain(&mut rx);
    let stopped = ev.iter().find(|e| e.name == "recording.stopped").unwrap();
    assert_eq!(stopped.data["reason"], "error");
    assert!(stopped.data["error"]["message"].is_string());
    assert!(library_recordings(&r.daemon).await.is_empty());
    start(&r, json!({})).await;
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
}

// ---- ids, status, devices ------------------------------------------------------------------

/// Recording ids are monotonic and unique; an unknown id is not_found for every command.
#[tokio::test]
async fn recording_ids_are_monotonic_and_commands_check_them() {
    let r = rig();
    let mut ids = Vec::new();
    for _ in 0..3 {
        let s = start(&r, json!({})).await;
        ids.push(s["recording_id"].as_str().unwrap().to_owned());
        user(&r.daemon, "stop_recording", json!({})).await.unwrap();
    }
    let mut sorted = ids.clone();
    sorted.sort();
    assert_eq!(ids, sorted, "ids sort in start order");
    sorted.dedup();
    assert_eq!(sorted.len(), 3);

    let s = start(&r, json!({})).await;
    for m in [
        "pause_recording",
        "resume_recording",
        "stop_recording",
        "discard_recording",
        "get_recording_status",
        "skip_countdown",
    ] {
        let e = user(&r.daemon, m, json!({ "recording_id": "rec_nope" }))
            .await
            .unwrap_err();
        assert_eq!(err_code(&e), ErrorCode::NotFound, "{m}");
    }
    // Idle status names the last run.
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
    let idle = user(&r.daemon, "get_recording_status", json!({}))
        .await
        .unwrap();
    assert_eq!(idle["state"], "idle");
    assert_eq!(idle["last"]["recording_id"], s["recording_id"]);
    let old = user(
        &r.daemon,
        "get_recording_status",
        json!({ "recording_id": s["recording_id"] }),
    )
    .await
    .unwrap();
    assert_eq!(old["state"], "stopped");
    assert_eq!(old["capture_id"], s["capture_id"]);
    // Nothing to pause or resume when idle.
    for m in ["pause_recording", "resume_recording"] {
        let e = user(&r.daemon, m, json!({})).await.unwrap_err();
        assert_eq!(err_code(&e), ErrorCode::NotFound, "{m}");
    }
}

/// Resuming a recording that is not paused is a no-op answer, not an error; pausing during
/// "starting" waits for the start.
#[tokio::test]
async fn rec4_resume_of_a_running_recording_changes_nothing() {
    let r = rig();
    start(&r, json!({})).await;
    let v = user(&r.daemon, "resume_recording", json!({}))
        .await
        .unwrap();
    assert_eq!(v["changed"], false);
    assert_eq!(v["state"], "recording");
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
}

/// `list_devices` (CAM-4, AUD-1, AUD-2): cameras, mics, system-audio support, permissions.
#[tokio::test]
async fn list_devices_returns_the_factory_listing() {
    let r = rig();
    let v = user(&r.daemon, "list_devices", json!({})).await.unwrap();
    assert_eq!(v["cameras"][0]["id"], "fake-cam");
    assert_eq!(v["mics"][0]["default"], true);
    assert_eq!(v["system_audio"]["supported"], true);
    assert_eq!(v["microphone_permission"], "authorized");
}

/// Settings -> Webcam & audio: `camera.device` and `audio.mic_device` are the devices a start
/// opens when the request names none (CLI `--mic default`, MCP `camera: true`); a named device
/// wins; a configured device that is not connected falls back to the system default and stays
/// saved; off stays off.
#[tokio::test]
async fn configured_devices_are_the_defaults_of_a_start() {
    use cm_daemon::testing::LastDevices;
    let r = rig();
    let set = |v: Value| {
        let d = r.daemon.clone();
        async move {
            user(&d, "set_settings", json!({ "values": v }))
                .await
                .unwrap()
        }
    };
    async fn run(r: &Rig, extra: Value) -> LastDevices {
        let v = start(r, extra).await;
        assert_eq!(v["state"], "recording");
        let seen = r.sources.last_devices();
        user(&r.daemon, "stop_recording", json!({})).await.unwrap();
        seen
    }
    let some = |s: &str| Some(Some(s.to_owned()));
    // Nothing configured: the system default.
    let seen = run(&r, json!({ "camera": true, "mic": "default" })).await;
    assert_eq!((seen.camera, seen.mic), (Some(None), Some(None)));
    // Configured and connected.
    set(json!({ "camera.device": "fake-cam", "audio.mic_device": "fake-mic" })).await;
    let seen = run(&r, json!({ "camera": true, "mic": "default" })).await;
    assert_eq!(
        (seen.camera, seen.mic),
        (some("fake-cam"), some("fake-mic"))
    );
    // A device the request names is used as named.
    let seen = run(
        &r,
        json!({ "camera": { "device_id": "other-cam" }, "mic": { "device_id": "other-mic" } }),
    )
    .await;
    assert_eq!(
        (seen.camera, seen.mic),
        (some("other-cam"), some("other-mic"))
    );
    // Off stays off.
    let seen = run(&r, json!({ "camera": false, "mic": false })).await;
    assert_eq!((seen.camera, seen.mic), (None, None));
    // Configured but unplugged: the system default for this recording, the setting is kept.
    set(json!({ "camera.device": "gone-cam", "audio.mic_device": "gone-mic" })).await;
    let seen = run(&r, json!({ "camera": true, "mic": true })).await;
    assert_eq!((seen.camera, seen.mic), (Some(None), Some(None)));
    let all = user(&r.daemon, "get_settings", json!({})).await.unwrap();
    assert_eq!(all["settings"]["camera.device"], "gone-cam");
}

// ---- confirmation (AGT-2, NFR-11) ---------------------------------------------------------

/// AGT-2: an agent's start waits for the dialog, and the dialog says exactly what will happen:
/// target, size, fps, codec, every sensor, destination, limits (and the requesting client is the
/// chip). Nothing exists until the user approves.
#[tokio::test]
async fn agt2_agent_start_asks_with_the_exact_plan() {
    let r = rig_with(FakeSources::new(), Some(Decision::Approve));
    pass_selftest(&r, json!({ "size": "720p", "codec": "hevc" })).await;
    let p = json!({
        "target": { "display_id": 1 }, "countdown_s": 0, "fps": 60, "codec": "hevc",
        "size": "720p", "mic": true, "camera": { "resolution": "720p" }, "system_audio": true,
        "max_duration_s": 1800, "title": "Demo",
    });
    let s = r
        .daemon
        .execute_method(agent("claude-desktop"), 7, "start_recording", &p)
        .await
        .unwrap();
    assert_eq!(s["indicator"]["started_by"]["kind"], "agent");
    assert_eq!(s["indicator"]["started_by"]["client"], "claude-desktop");
    assert_eq!(s["indicator"]["started_by"]["tool"], "start_recording");
    let shown = r.confirmer.as_ref().unwrap().shown();
    assert_eq!(shown.len(), 1);
    let q = &shown[0];
    assert_eq!(q.client, "claude-desktop");
    assert_eq!(q.tool, "start_recording");
    assert!(q.allow_session);
    for needle in [
        "display 1",
        "60 fps",
        "microphone",
        "camera",
        "system audio",
    ] {
        assert!(q.summary.contains(needle), "{needle} in {}", q.summary);
    }
    let d = q.details.as_object().unwrap();
    let text = |k: &str| {
        d[k].as_str()
            .unwrap_or_else(|| panic!("row {k}"))
            .to_owned()
    };
    assert!(
        text("target").starts_with("Display 1"),
        "{}",
        text("target")
    );
    assert!(text("output").contains("HEVC") && text("output").contains("60 fps"));
    assert!(text("output").contains(" x "), "{}", text("output"));
    assert!(text("microphone").starts_with("On: Fake Microphone"));
    assert!(text("camera").starts_with("On: Fake Camera, 720p"));
    assert_eq!(text("system_audio"), "On");
    assert!(text("cursor").contains("never keystrokes"));
    assert!(text("save_to").contains(r.dir.path().to_str().unwrap()));
    assert!(text("save_to").contains("Demo"));
    assert_eq!(text("max_duration"), "30 min");
    assert_eq!(text("countdown"), "None");
    assert!(d.contains_key("screen"));
    // The activity log keeps the rich sentence.
    let log = r.daemon.activity();
    assert_eq!(log.len(), 1);
    assert!(
        log[0].summary.starts_with("Record display 1"),
        "{}",
        log[0].summary
    );
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
}

/// AGT-2.AC1 / NFR-11: declined means nothing was created and nothing is held.
#[tokio::test]
async fn agt2_declined_start_creates_nothing_and_holds_nothing() {
    let r = rig_with(FakeSources::new(), Some(Decision::Decline));
    let e = r
        .daemon
        .execute_method(agent("claude"), 0, "start_recording", &start_params())
        .await
        .unwrap_err();
    assert_eq!(err_code(&e), ErrorCode::UserDeclined);
    assert_eq!(r.confirmer.as_ref().unwrap().shown().len(), 1);
    assert!(bundles(&r).is_empty());
    assert_eq!(r.sources.builds(), 0);
    assert!(!r.daemon.is_job_type_held("export"));
    assert!(!r.daemon.recording_active());
}

/// An agent's pause, resume and stop are Free (REC-4): the recording was already approved. Its
/// discard and restart delete a take, so they ask.
#[tokio::test]
async fn agt2_agent_controls_are_free_but_discard_and_restart_ask() {
    let r = rig_with(FakeSources::new(), Some(Decision::Approve));
    r.daemon
        .execute_method(agent("claude"), 0, "start_recording", &start_params())
        .await
        .unwrap();
    assert_eq!(r.confirmer.as_ref().unwrap().shown().len(), 1);
    for m in ["pause_recording", "resume_recording"] {
        r.daemon
            .execute_method(agent("claude"), 0, m, &json!({}))
            .await
            .unwrap();
    }
    assert_eq!(
        r.confirmer.as_ref().unwrap().shown().len(),
        1,
        "no dialog for pause/resume"
    );

    // Restart by an agent asks, with the same plan text plus the warning about the old take.
    r.daemon
        .execute_method(agent("claude"), 0, "restart_recording", &json!({}))
        .await
        .unwrap();
    let shown = r.confirmer.as_ref().unwrap().shown();
    assert_eq!(shown.len(), 2);
    assert_eq!(shown[1].tool, "restart_recording");
    assert!(shown[1]
        .summary
        .starts_with("Discard the current take and start over"));
    assert!(shown[1].details["discarded"].is_string());
    assert!(!shown[1].allow_session);

    // Discard by an agent asks.
    r.daemon
        .execute_method(agent("claude"), 0, "discard_recording", &json!({}))
        .await
        .unwrap();
    let shown = r.confirmer.as_ref().unwrap().shown();
    assert_eq!(shown.len(), 3);
    assert_eq!(shown[2].tool, "discard_recording");
    // P3: each request names its kind and carries the plan as typed parameters, next to the
    // English text the CLI and MCP clients read (which is unchanged).
    for (q, kind) in shown
        .iter()
        .zip(["start_recording", "restart_recording", "discard_recording"])
    {
        assert_eq!(q.kind, kind);
        assert_eq!(q.tool, kind);
    }
    let start = &shown[0].params;
    assert_eq!(start["restart"], false);
    assert_eq!(
        start["target"],
        json!({"type": "display", "id": 1, "name": "Fake display 1", "width": 400, "height": 300})
    );
    assert_eq!(
        (
            start["width"].as_u64(),
            start["height"].as_u64(),
            start["fps"].as_u64()
        ),
        (Some(400), Some(300), Some(30))
    );
    assert_eq!(start["codec"], "H.264");
    assert!(start["microphone"].is_null() && start["camera"].is_null());
    assert_eq!(start["system_audio"], false);
    assert_eq!(start["cursor"], true);
    assert_eq!(start["countdown_s"], 0);
    assert!(start["save_to"].is_string() && start["disk"]["free_gb"].is_f64());
    assert!(shown[0]
        .summary
        .starts_with("Record display 1 at 400 x 300, 30 fps, and save it to "));
    assert_eq!(shown[1].params["restart"], true);
    assert_eq!(shown[1].params["countdown_s"], 3);
    assert_eq!(shown[2].params["recorded_ms"], 0);
    assert!(shown[2].params["recording"]
        .as_str()
        .unwrap()
        .starts_with("rec_"));
    assert!(shown[2].summary.contains("delete its files"));
    assert!(bundles(&r).is_empty());
}

/// A declined discard keeps recording (REC-5.AC3: cancelling keeps recording).
#[tokio::test]
async fn rec5_ac3_declined_discard_keeps_recording() {
    let r = rig_with(FakeSources::new(), Some(Decision::Decline));
    start(&r, json!({})).await;
    let e = r
        .daemon
        .execute_method(agent("claude"), 0, "discard_recording", &json!({}))
        .await
        .unwrap_err();
    assert_eq!(err_code(&e), ErrorCode::UserDeclined);
    assert!(r.daemon.recording_active());
    assert_eq!(bundles(&r).len(), 1);
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
}

/// Read-only clients may not control a recording, but they can read its status and the devices.
#[tokio::test]
async fn agt2_read_only_clients_cannot_control_recordings() {
    let r = rig();
    start(&r, json!({})).await;
    r.daemon
        .policies()
        .set("viewer", cm_daemon::ClientPolicy::ReadOnly);
    for m in [
        "pause_recording",
        "resume_recording",
        "stop_recording",
        "start_recording",
        "discard_recording",
        "skip_countdown",
        "set_mic_muted",
    ] {
        let p = if m == "start_recording" {
            start_params()
        } else if m == "set_mic_muted" {
            json!({ "muted": true })
        } else {
            json!({})
        };
        let e = r
            .daemon
            .execute_method(agent("viewer"), 0, m, &p)
            .await
            .unwrap_err();
        assert!(
            matches!(
                e,
                DaemonError::Declined {
                    reason: "client_read_only",
                    ..
                }
            ),
            "{m}: {e:?}"
        );
    }
    for m in ["get_recording_status", "list_devices"] {
        r.daemon
            .execute_method(agent("viewer"), 0, m, &json!({}))
            .await
            .unwrap();
    }
    assert!(r.daemon.recording_active(), "untouched");
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
}

/// AGT-3.AC4: the same `request_id` returns the first result and does not start again.
#[tokio::test]
async fn agt3_ac4_request_id_replays_the_first_start() {
    let r = rig_with(FakeSources::new(), Some(Decision::Approve));
    let mut p = start_params();
    p["request_id"] = json!("start-1");
    let a = r
        .daemon
        .execute_method(agent("claude"), 0, "start_recording", &p)
        .await
        .unwrap();
    let b = r
        .daemon
        .execute_method(agent("claude"), 0, "start_recording", &p)
        .await
        .unwrap();
    assert_eq!(a, b);
    assert_eq!(
        r.confirmer.as_ref().unwrap().shown().len(),
        1,
        "no second dialog"
    );
    assert_eq!(r.sources.builds(), 1);
    // stop is idempotent by request id as well.
    let mut sp = json!({ "request_id": "stop-1" });
    let s1 = r
        .daemon
        .execute_method(agent("claude"), 0, "stop_recording", &sp)
        .await
        .unwrap();
    let s2 = r
        .daemon
        .execute_method(agent("claude"), 0, "stop_recording", &sp)
        .await
        .unwrap();
    assert_eq!(s1, s2);
    sp["x"] = json!(1);
    assert!(r
        .daemon
        .execute_method(agent("claude"), 0, "stop_recording", &sp)
        .await
        .is_err());
}

/// "Allow for this session" still works for start_recording through the engine.
#[tokio::test]
async fn agt2_allow_for_session_skips_the_second_dialog() {
    let (b, dir) = builder(desktop());
    let host = Arc::new(ManualHostClock::new(ORIGIN_NS));
    let free = Arc::new(FakeFreeSpace::new(500_000_000_000));
    let c = Arc::new(FixedConfirmer::new(Decision::ApproveForSession));
    let d = b
        .recording_sources(FakeSources::new())
        .recording_options(options(host, free))
        .confirmer(Shared(c.clone()))
        .build();
    let s = d.new_session();
    for _ in 0..2 {
        d.execute_method(agent("claude"), s, "start_recording", &start_params())
            .await
            .unwrap();
        d.execute_method(Actor::User, 0, "stop_recording", &json!({}))
            .await
            .unwrap();
    }
    assert_eq!(c.shown().len(), 1);
    drop(dir);
}

/// Without an engine the recording actions are `not_implemented` (and `actions.list` says
/// unavailable), as before M3 slice 4; `list_devices` still works.
#[tokio::test]
async fn without_an_engine_recording_is_not_implemented() {
    let (b, _dir) = builder(desktop());
    let d = b.build();
    for m in [
        "start_recording",
        "pause_recording",
        "stop_recording",
        "get_recording_status",
        "discard_recording",
    ] {
        let p = if m == "start_recording" {
            start_params()
        } else {
            json!({})
        };
        let e = d.execute_method(Actor::User, 0, m, &p).await.unwrap_err();
        assert!(matches!(e, DaemonError::NotImplemented(_)), "{m}: {e:?}");
    }
    let avail: Vec<(&str, bool)> = d.actions().iter().map(|(s, a)| (s.name, *a)).collect();
    for (n, a) in avail {
        if matches!(
            n,
            "start_recording" | "pause_recording" | "discard_recording" | "stop_recording"
        ) {
            assert!(!a, "{n}");
        }
    }
    assert!(!d.recording_active());
}

/// Typed validation: every bad input is refused with a field name before anything else happens.
#[tokio::test]
async fn start_recording_validates_input_before_anything() {
    let r = rig_with(FakeSources::new(), Some(Decision::Approve));
    for (p, field) in [
        (json!({}), "target"),
        (json!({ "target": { "display_id": 1 }, "fps": 24 }), "fps"),
        (
            json!({ "target": { "display_id": 1 }, "countdown_s": 99 }),
            "countdown_s",
        ),
        (
            json!({ "target": { "display_id": 1 }, "camera": { "shape": "star" } }),
            "camera.shape",
        ),
        (
            json!({ "target": { "display_id": 1 }, "extra": 1 }),
            "extra",
        ),
    ] {
        let e = r
            .daemon
            .execute_method(agent("claude"), 0, "start_recording", &p)
            .await
            .unwrap_err();
        assert_eq!(err_code(&e), ErrorCode::InvalidInput, "{p}");
        assert!(e.to_string().starts_with(field), "{p}: {e}");
    }
    assert!(r.confirmer.as_ref().unwrap().shown().is_empty());
}

// ---- recovery at startup -----------------------------------------------------------------

/// REC-8: a crashed recording (lock file, dead owner) is recovered at startup:
/// `recover_recordings` indexes it, reports `lost_ms`, and publishes `recording.recovered`.
#[tokio::test]
async fn rec8_recover_recordings_at_startup() {
    use cm_encode::fmp4::synth::{fragment, init_segment, Spec};
    let dir = tempfile::tempdir().unwrap();
    let lib = cm_library::Library::open_or_create(dir.path()).unwrap();
    let session = cm_library::rec::SessionInfo {
        clock_origin_ns: 42,
        timebase: "uptime_raw_ns".into(),
        displays: vec![],
        devices: json!({}),
        app_version: "0.0.1".into(),
        extra: serde_json::Map::new(),
    };
    let now = time::OffsetDateTime::now_utc();
    let b = lib
        .create_recording(Some("Crashed"), now, json!({}), &session)
        .unwrap();
    let spec = Spec {
        width: 1920,
        height: 1080,
        ..Default::default()
    };
    let mut v = init_segment(&spec);
    for i in 0..3u32 {
        v.extend(fragment(&spec, i + 1, u64::from(i) * 240_000, 60, 4000, 64));
    }
    std::fs::write(b.dir().join("screen.mp4"), v).unwrap();
    // The owner is gone: a dead pid in the lock.
    let lock_path = b.dir().join(cm_library::rec::LOCK_FILE);
    let mut lock: Value = serde_json::from_slice(&std::fs::read(&lock_path).unwrap()).unwrap();
    lock["pid"] = json!(0);
    std::fs::write(&lock_path, serde_json::to_vec(&lock).unwrap()).unwrap();
    let capture_id = b.id().to_owned();
    // The recording process "died": the owner lock goes with the bundle handle.
    drop(b);

    let daemon =
        cm_daemon::Daemon::builder(cm_capture::Capturer::new(desktop()).with_own_pid(None), lib)
            .build();
    let mut rx = daemon.subscribe();
    let summary = daemon.recover_recordings().unwrap();
    assert_eq!(summary.recovered.len(), 1);
    let rec = &summary.recovered[0];
    assert_eq!(rec.capture_id, capture_id);
    assert_eq!(rec.title, "Crashed");
    assert!(rec.lost_ms <= 2000, "REC-8: at most one fragment");
    assert!(rec.duration_s > 5.0);
    let ev = drain(&mut rx);
    let e = ev.iter().find(|e| e.name == "recording.recovered").unwrap();
    assert_eq!(e.data["capture_id"], capture_id.as_str());
    assert_eq!(e.data["lost_ms"], rec.lost_ms);
    assert!(names(&ev).contains(&"capture.created"));
    // The recording is in the library now, and a second pass finds nothing.
    let listed = user(&daemon, "list_library", json!({ "kind": "recording" }))
        .await
        .unwrap();
    assert_eq!(listed["captures"][0]["id"], capture_id.as_str());
    let again = daemon.recover_recordings().unwrap();
    assert!(again.recovered.is_empty());
    assert!(!dir.path().join("Crashed.cmrec/recording.lock").exists());
}

/// A recording of this very process is not "crashed": recovery leaves it alone.
#[tokio::test]
async fn rec8_recovery_leaves_a_live_recording_alone() {
    let r = rig();
    start(&r, json!({})).await;
    let summary = r.daemon.recover_recordings().unwrap();
    assert!(summary.recovered.is_empty());
    assert_eq!(summary.in_progress, 1);
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
}

/// A track that panics while the session ticks does not take the recording down: the tick fails
/// with a warning, and the recording still stops and saves.
#[tokio::test]
async fn a_panicking_track_poll_is_a_warning_not_a_crash() {
    let r = rig();
    let mut rx = r.daemon.subscribe();
    start(&r, json!({ "mic": true })).await;
    r.sources
        .handle(TrackKind::Mic)
        .unwrap()
        .panic_in_poll(true);
    let w = wait_event(&mut rx, "recording.warning").await;
    assert_eq!(w.data["code"], "tick_failed");
    assert!(r.daemon.recording_active());
    r.sources
        .handle(TrackKind::Mic)
        .unwrap()
        .panic_in_poll(false);
    let stopped = user(&r.daemon, "stop_recording", json!({})).await.unwrap();
    assert!(stopped["capture_id"].is_string());
    assert_eq!(library_recordings(&r.daemon).await.len(), 1);
}

/// R3-8: the 60 fps verdict is screen-only; with the camera on the start is accepted with a
/// `fps60_unverified_with_camera` warning (without it, none).
#[tokio::test]
async fn r3_8_fps60_with_the_camera_warns_that_it_is_unverified() {
    let r = rig();
    pass_selftest(&r, json!({ "size": "720p", "codec": "hevc" })).await;
    let base = json!({ "target": { "display_id": 1 }, "countdown_s": 0, "fps": 60,
                       "codec": "hevc", "size": "720p" });
    let codes = |v: &Value| -> Vec<String> {
        v["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|w| w["code"].as_str().unwrap().to_owned())
            .collect()
    };
    let s = user(&r.daemon, "start_recording", base.clone())
        .await
        .unwrap();
    assert!(!codes(&s).contains(&"fps60_unverified_with_camera".to_owned()));
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
    let mut with_cam = base;
    with_cam["camera"] = json!({ "resolution": "720p" });
    let s = user(&r.daemon, "start_recording", with_cam).await.unwrap();
    assert!(
        codes(&s).contains(&"fps60_unverified_with_camera".to_owned()),
        "{s}"
    );
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
}

/// R3-9 / REC-8.AC4: the low-disk warning is in the plan, before anything is recorded: the
/// dialog has a `disk_space` row, the start result carries `disk_low`, and the session's own
/// check does not repeat it (R3-15).
#[tokio::test]
async fn r3_9_low_disk_is_warned_before_recording_and_not_repeated() {
    let r = rig_with(FakeSources::new(), Some(Decision::Approve));
    // Camera, mic, system audio and cursor: about 15 Mbps planned, so 1.2 GB is ~9 minutes.
    r.free.set(1_200_000_000);
    let mut rx = r.daemon.subscribe();
    let p = json!({ "target": { "display_id": 1 }, "countdown_s": 0, "mic": true,
                    "system_audio": true, "camera": { "resolution": "720p" } });
    let s = r
        .daemon
        .execute_method(agent("claude-desktop"), 0, "start_recording", &p)
        .await
        .unwrap();
    let shown = r.confirmer.as_ref().unwrap().shown();
    let row = shown[0].details["disk_space"].as_str().unwrap();
    assert!(row.contains("1.2 GB free"), "{row}");
    assert!(row.contains("estimate"), "{row}");
    let disk_low = s["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|w| w["code"] == "disk_low")
        .count();
    assert_eq!(disk_low, 1, "{s}");
    // The runtime check sees the same condition on its first ticks: no second event.
    tokio::time::sleep(Duration::from_millis(150)).await;
    let ev = drain(&mut rx);
    let events = ev
        .iter()
        .filter(|e| e.name == "recording.warning" && e.data["code"] == "disk_low")
        .count();
    assert_eq!(events, 1, "{:?}", names(&ev));
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();

    // Under 1 GB the plan itself refuses, before any dialog.
    r.free.set(900_000_000);
    let shown_before = r.confirmer.as_ref().unwrap().shown().len();
    let e = r
        .daemon
        .execute_method(agent("claude-desktop"), 0, "start_recording", &p)
        .await
        .unwrap_err();
    assert_eq!(err_code(&e), ErrorCode::DiskFull);
    assert_eq!(r.confirmer.as_ref().unwrap().shown().len(), shown_before);
}

/// R3-10: a stop slower than the stop timeout does not free the engine early: it stays
/// `stopping` (no new recording), and the late result is published when the job really ends.
#[tokio::test]
async fn r3_10_a_slow_stop_keeps_the_engine_busy_and_publishes_the_late_result() {
    let r = rig_tuned(FakeSources::new(), None, |o| {
        o.stop_timeout = Duration::from_millis(400);
    });
    let mut rx = r.daemon.subscribe();
    start(&r, json!({})).await;
    r.sources.handle(TrackKind::Screen).unwrap().slow_stop(1500);
    let e = user(&r.daemon, "stop_recording", json!({}))
        .await
        .unwrap_err();
    assert!(e.to_string().contains("taking longer"), "{e}");
    // Still closing: busy, not idle; a new start is refused.
    assert!(r.daemon.recording_active());
    let st = user(&r.daemon, "get_recording_status", json!({}))
        .await
        .unwrap();
    assert_eq!(st["state"], "stopping", "{st}");
    let busy = user(&r.daemon, "start_recording", start_params())
        .await
        .unwrap_err();
    assert_eq!(err_code(&busy), ErrorCode::Busy);
    // Then the job ends: the stopped event, the capture, and Idle.
    let stopped = wait_event(&mut rx, "recording.stopped").await;
    assert!(stopped.data["capture_id"].is_string(), "{:?}", stopped.data);
    until(|| (!r.daemon.recording_active()).then_some(())).await;
    assert_eq!(library_recordings(&r.daemon).await.len(), 1);
    start(&r, json!({})).await;
    // This rig's stop timeout is 0.4 s; closing a bundle can take longer on a slow disk (the
    // Windows runner), which is the same "keeps finishing" outcome. Wait for idle either way.
    let _ = user(&r.daemon, "stop_recording", json!({})).await;
    until(|| (!r.daemon.recording_active()).then_some(())).await;
}

/// R3-10: a restart holds the slot from the discard to the new start: nothing can slip in
/// between (and the take is not deleted for a start that then fails with `busy`).
#[tokio::test]
async fn r3_10_a_restart_cannot_be_overtaken_by_another_start() {
    let r = rig();
    start(&r, json!({})).await;
    r.sources.handle(TrackKind::Screen).unwrap().slow_stop(300);
    let d = r.daemon.clone();
    let restart = tokio::spawn(async move { user(&d, "restart_recording", json!({})).await });
    // Hammer start_recording while the restart discards and starts again.
    let mut won = 0;
    for _ in 0..150 {
        if user(&r.daemon, "start_recording", start_params())
            .await
            .is_ok()
        {
            won += 1;
            break;
        }
        tokio::time::sleep(Duration::from_millis(3)).await;
    }
    let v = restart.await.unwrap().unwrap();
    assert_eq!(won, 0, "no other start slipped into the restart");
    assert!(v["restart_of"].is_string());
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
}

/// R4-1: a start that fails after the countdown tells the UIs: `recording.discarded` with
/// `reason: start_failed` and the error.
#[tokio::test]
async fn r4_1_a_failed_start_publishes_discarded_start_failed() {
    let r = rig_with(
        FakeSources::new().with_failing_start(TrackKind::Screen),
        None,
    );
    let mut rx = r.daemon.subscribe();
    let e = user(&r.daemon, "start_recording", start_params())
        .await
        .unwrap_err();
    let ev = wait_event(&mut rx, "recording.discarded").await;
    assert_eq!(ev.data["reason"], "start_failed");
    assert_eq!(ev.data["error"]["code"], e.code().unwrap().as_str());
    assert!(ev.data["error"]["message"].is_string());
    assert!(!r.daemon.recording_active());
}

/// R3-5: stop before the first frame: `recording.stopped { reason: "no_frames" }`, no capture,
/// the empty bundle is deleted (nothing for recovery to retry), the engine is idle again.
#[tokio::test]
async fn r3_5_stop_before_the_first_frame_reports_no_frames_and_leaves_nothing() {
    let r = rig();
    let mut rx = r.daemon.subscribe();
    start(&r, json!({})).await;
    r.sources
        .handle(TrackKind::Screen)
        .unwrap()
        .stop_before_first_frame(true);
    let e = user(&r.daemon, "stop_recording", json!({}))
        .await
        .unwrap_err();
    assert!(e.to_string().contains("No frames"), "{e}");
    let ev = wait_event(&mut rx, "recording.stopped").await;
    assert_eq!(ev.data["reason"], "no_frames");
    assert!(!r.daemon.recording_active());
    assert!(bundles(&r).is_empty(), "{:?}", bundles(&r));
    let st = user(&r.daemon, "get_recording_status", json!({}))
        .await
        .unwrap();
    assert_eq!(st["state"], "idle");
    assert_eq!(st["last"]["reason"], "no_frames");
    // And a new recording works.
    start(&r, json!({})).await;
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
}

/// REC-6.AC8: the provider's window ids reach the screen plan as the kept windows of the
/// excluded own process; the provider itself travels along so the backend can ask again.
#[tokio::test]
async fn rec6_ac8_kept_windows_provider_reaches_the_plan() {
    let studio = Arc::new(std::sync::atomic::AtomicU32::new(4242));
    let s2 = studio.clone();
    let r = rig_tuned(FakeSources::new(), None, |o| {
        o.own_pid = Some(777);
        o.kept_windows = Some(Arc::new(move || {
            vec![s2.load(std::sync::atomic::Ordering::SeqCst)]
        }));
    });
    start(&r, json!({})).await;
    let ex = r.sources.last_plan().unwrap().exclusions;
    assert_eq!(ex.pids, vec![777]);
    assert_eq!(ex.keep_window_ids, vec![4242]);
    let p = ex.keep_provider.clone().expect("provider travels");
    studio.store(4243, std::sync::atomic::Ordering::SeqCst);
    assert_eq!(p.ids(), vec![4243], "queried live");
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
}

/// Without a provider, or without an own pid, nothing is kept: pure pid exclusion as before.
#[tokio::test]
async fn rec6_ac8_without_a_provider_every_own_window_is_excluded() {
    let r = rig_tuned(FakeSources::new(), None, |o| o.own_pid = Some(777));
    start(&r, json!({})).await;
    let ex = r.sources.last_plan().unwrap().exclusions;
    assert_eq!(ex.pids, vec![777]);
    assert!(ex.keep_window_ids.is_empty() && ex.keep_provider.is_none());
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();

    let r = rig_tuned(FakeSources::new(), None, |o| {
        o.own_pid = None;
        o.kept_windows = Some(Arc::new(|| vec![1]));
    });
    start(&r, json!({})).await;
    let ex = r.sources.last_plan().unwrap().exclusions;
    assert!(ex.pids.is_empty() && ex.keep_window_ids.is_empty());
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
}

/// `DaemonBuilder::kept_windows` is wired to the engine whatever the order of the builder calls.
#[tokio::test]
async fn rec6_ac8_builder_kept_windows_reaches_the_engine() {
    let (b, dir) = builder(desktop());
    let sources = FakeSources::new();
    let mut opts = EngineOptions {
        tick_every: Duration::from_millis(15),
        countdown_step: Duration::from_millis(40),
        own_pid: Some(5),
        ..EngineOptions::default()
    };
    opts.selftest_dir = Some(dir.path().to_owned());
    let d = b
        .kept_windows(|| vec![9])
        .recording_sources(sources.clone())
        .recording_options(opts)
        .build();
    user(&d, "start_recording", start_params()).await.unwrap();
    assert_eq!(
        sources.last_plan().unwrap().exclusions.keep_window_ids,
        vec![9]
    );
    user(&d, "stop_recording", json!({})).await.unwrap();
}

// ---- PRM-1: a permission removed mid-recording ---------------------------------------------

/// Starts with every track, records 4 s, pauses 2 s, records 3 s more (so the pause gap and the
/// session clock are part of what must stay consistent).
async fn rig_after_pause_gap() -> (Rig, Receiver<Event>) {
    let r = rig();
    user(
        &r.daemon,
        "set_settings",
        json!({ "values": { "recording.resume_countdown_s": 0 } }),
    )
    .await
    .unwrap();
    let rx = r.daemon.subscribe();
    start(
        &r,
        json!({ "mic": true, "camera": {}, "system_audio": true }),
    )
    .await;
    r.host.advance_ns(4 * SEC);
    user(&r.daemon, "pause_recording", json!({})).await.unwrap();
    r.host.advance_ns(2 * SEC);
    user(&r.daemon, "resume_recording", json!({}))
        .await
        .unwrap();
    r.host.advance_ns(3 * SEC);
    (r, rx)
}

/// Everything published up to and including the first `name` event.
async fn collect_until(rx: &mut Receiver<Event>, name: &str) -> (Event, Vec<Event>) {
    let mut seen = Vec::new();
    let wait = async {
        loop {
            let e = rx.recv().await.unwrap();
            if e.name == name {
                return e;
            }
            seen.push(e);
        }
    };
    let ev = tokio::time::timeout(Duration::from_secs(10), wait)
        .await
        .unwrap_or_else(|_| panic!("no `{name}` event"));
    (ev, seen)
}

fn assert_clean_revoked_screen_stop(r: &Rig, ev: &Event, warnings: &[Event]) {
    assert_eq!(ev.data["reason"], "permission_revoked");
    assert_eq!(ev.data["permission"], "screen");
    assert!(
        ev.data["message"]
            .as_str()
            .unwrap()
            .contains("Screen Recording permission was removed"),
        "{}",
        ev.data
    );
    // The session clock and the pause gap survive: 4 s + 3 s recorded, 2 s paused.
    assert_eq!(ev.data["duration_s"], 7.0);
    assert!(warnings
        .iter()
        .any(|w| w.data["code"] == "permission_revoked" && w.data["permission"] == "screen"));
    assert!(ev.data["capture_id"].is_string());
    // Every track that was running was finalized, the lock is gone, the bundle is kept.
    for kind in [
        TrackKind::Screen,
        TrackKind::Cursor,
        TrackKind::Mic,
        TrackKind::System,
        TrackKind::Camera,
    ] {
        let calls = r.sources.handle(kind).unwrap().calls();
        assert_eq!(calls.last().map(String::as_str), Some("stop"), "{kind:?}");
    }
    let path = bundles(r).pop().unwrap();
    assert!(
        !r.dir.path().join(&path).join("recording.lock").exists(),
        "the lock is removed"
    );
    let m = manifest(r, &path);
    assert_eq!(m["stop_reason"], "permission_revoked");
    let warns = m["warnings"].to_string();
    assert!(warns.contains("permission_revoked"), "{warns}");
}

/// PRM-1: ScreenCaptureKit reports "user declined TCCs": the whole recording stops cleanly, the
/// take is kept in the library, and the stop says which permission and why.
#[tokio::test]
async fn prm1_screen_permission_revoked_by_the_track_stops_cleanly() {
    let (r, mut rx) = rig_after_pause_gap().await;
    r.sources
        .handle(TrackKind::Screen)
        .unwrap()
        .revoke_permission(Permission::Screen);
    let (ev, events) = collect_until(&mut rx, "recording.stopped").await;
    until(|| (!r.daemon.recording_active()).then_some(())).await;
    assert_clean_revoked_screen_stop(&r, &ev, &events);
    assert_eq!(library_recordings(&r.daemon).await.len(), 1);
}

/// PRM-1: the same stop when only the OS permission poll notices (no track reported anything).
#[tokio::test]
async fn prm1_screen_permission_revoked_seen_by_the_poll_stops_cleanly() {
    let (r, mut rx) = rig_after_pause_gap().await;
    r.sources.revoke_permission(Permission::Screen);
    let (ev, events) = collect_until(&mut rx, "recording.stopped").await;
    until(|| (!r.daemon.recording_active()).then_some(())).await;
    assert_clean_revoked_screen_stop(&r, &ev, &events);
}

/// PRM-1 decision: losing only the microphone keeps the screen recording going. The mic track
/// is kept up to that moment and finalized at the normal stop; a warning says which permission.
#[tokio::test]
async fn prm1_microphone_revoked_keeps_recording_and_warns() {
    let (r, mut rx) = rig_after_pause_gap().await;
    r.sources.revoke_permission(Permission::Microphone);
    let w = loop {
        let e = wait_event(&mut rx, "recording.warning").await;
        if e.data["code"] == "permission_revoked" {
            break e;
        }
    };
    assert_eq!(w.data["permission"], "microphone");
    assert_eq!(w.data["track"], "mic");
    assert_eq!(w.data["level"], "warning");
    // Still recording, and the warning is not repeated every tick.
    tokio::time::sleep(Duration::from_millis(120)).await;
    assert!(r.daemon.recording_active());
    let st = user(&r.daemon, "get_recording_status", json!({}))
        .await
        .unwrap();
    assert_eq!(st["state"], "recording");
    let repeated = drain(&mut rx)
        .iter()
        .filter(|e| e.name == "recording.warning" && e.data["code"] == "permission_revoked")
        .count();
    assert_eq!(repeated, 0);
    r.host.advance_ns(SEC);
    let stopped = user(&r.daemon, "stop_recording", json!({})).await.unwrap();
    assert_eq!(stopped["reason"], "user");
    assert_eq!(stopped["duration_ms"], 8000);
    assert_eq!(stopped["paused_ms"], 2000);
    let tracks: Vec<&str> = stopped["tracks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["kind"].as_str().unwrap())
        .collect();
    assert!(
        tracks.contains(&"mic") && tracks.contains(&"screen"),
        "{tracks:?}"
    );
    let revoked = stopped["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|w| w["code"] == "permission_revoked")
        .count();
    assert_eq!(revoked, 1);
}

/// PRM-1 decision: a camera that loses its permission (the track notices) leaves a gap; the
/// recording continues and the camera file is closed at the normal stop.
#[tokio::test]
async fn prm1_camera_revoked_by_the_track_keeps_recording_and_warns() {
    let (r, mut rx) = rig_after_pause_gap().await;
    r.sources
        .handle(TrackKind::Camera)
        .unwrap()
        .revoke_permission(Permission::Camera);
    let w = loop {
        let e = wait_event(&mut rx, "recording.warning").await;
        if e.data["code"] == "permission_revoked" {
            break e;
        }
    };
    assert_eq!(w.data["permission"], "camera");
    assert_eq!(w.data["track"], "camera");
    assert!(r.daemon.recording_active());
    let stopped = user(&r.daemon, "stop_recording", json!({})).await.unwrap();
    assert_eq!(stopped["reason"], "user");
    assert_eq!(
        r.sources
            .handle(TrackKind::Camera)
            .unwrap()
            .calls()
            .last()
            .map(String::as_str),
        Some("stop")
    );
    assert!(stopped.get("permission").is_none());
}

/// PRM-1: Accessibility / Input Monitoring never stop a recording (the cursor track degrades).
#[tokio::test]
async fn prm1_cursor_permissions_do_not_stop_a_recording() {
    let r = rig();
    start(&r, json!({})).await;
    r.sources.revoke_permission(Permission::InputMonitoring);
    r.sources.revoke_permission(Permission::Accessibility);
    tokio::time::sleep(Duration::from_millis(120)).await;
    assert!(r.daemon.recording_active());
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
}

// ---- AUD-6: mic mute ------------------------------------------------------------------------

async fn mute(r: &Rig, muted: bool) -> Value {
    user(&r.daemon, "set_mic_muted", json!({ "muted": muted }))
        .await
        .unwrap()
}

/// AUD-6: `set_mic_muted` toggles the mic track without stopping it, answers
/// `{ recording_id, muted, changed }`, publishes `recording.mic_muted` / `mic_unmuted` once per
/// real change, shows in the status, and the spans end up in the manifest in session time.
#[tokio::test]
async fn aud6_mute_unmute_events_status_and_manifest_spans() {
    let r = rig();
    user(
        &r.daemon,
        "set_settings",
        json!({ "values": { "recording.resume_countdown_s": 0 } }),
    )
    .await
    .unwrap();
    let mut rx = r.daemon.subscribe();
    let started = start(&r, json!({ "mic": true, "title": "Mute" })).await;
    let rec_id = started["recording_id"].as_str().unwrap().to_owned();
    let st = user(&r.daemon, "get_recording_status", json!({}))
        .await
        .unwrap();
    assert_eq!(st["mic_muted"], false);

    r.host.advance_ns(5 * SEC);
    let m = mute(&r, true).await;
    assert_eq!(m["recording_id"], rec_id.as_str());
    assert_eq!(m["muted"], true);
    assert_eq!(m["changed"], true);
    assert_eq!(m["at_ms"], 5000);
    // Idempotent: the second call changes nothing and publishes nothing.
    let again = mute(&r, true).await;
    assert_eq!(again["muted"], true);
    assert_eq!(again["changed"], false);
    let st = status_where(&r.daemon, |s| s["mic_muted"] == true).await;
    assert_eq!(st["state"], "recording");
    assert_eq!(
        st["tracks"].as_array().unwrap().len(),
        3,
        "the track keeps running"
    );

    // Muting while paused is allowed; here: unmute while paused, mute again, resume muted.
    r.host.advance_ns(2 * SEC);
    user(&r.daemon, "pause_recording", json!({})).await.unwrap();
    r.host.advance_ns(3 * SEC);
    let un = mute(&r, false).await;
    assert_eq!(un["muted"], false);
    assert_eq!(un["changed"], true);
    assert_eq!(un["at_ms"], 10_000);
    mute(&r, true).await;
    r.host.advance_ns(SEC);
    user(&r.daemon, "resume_recording", json!({}))
        .await
        .unwrap();
    r.host.advance_ns(2 * SEC);
    let stopped = user(&r.daemon, "stop_recording", json!({})).await.unwrap();
    assert_eq!(stopped["reason"], "user");

    let calls = r.sources.handle(TrackKind::Mic).unwrap().calls();
    assert_eq!(
        calls,
        ["start", "mute", "pause", "unmute", "mute", "resume", "stop"],
        "never stopped or restarted, and the repeated mute did not reach the track"
    );
    let ev = drain(&mut rx);
    let muted_events: Vec<(String, Value)> = ev
        .iter()
        .filter(|e| e.name.starts_with("recording.mic_"))
        .map(|e| (e.name.clone(), e.data["at_ms"].clone()))
        .collect();
    assert_eq!(
        muted_events,
        [
            ("recording.mic_muted".to_owned(), json!(5000)),
            ("recording.mic_unmuted".to_owned(), json!(10_000)),
            ("recording.mic_muted".to_owned(), json!(10_000)),
        ]
    );
    assert!(ev
        .iter()
        .filter(|e| e.name.starts_with("recording.mic_"))
        .all(|e| e.data["recording_id"] == rec_id.as_str()));

    // The manifest lists the spans in session time (120 000 ticks per second); the one still
    // open at the stop is closed at the stop time (13 s of session: 5 + 2 + 3 + 1 + 2).
    let m = manifest(&r, "Mute.cmrec");
    assert_eq!(
        m["mic_muted"],
        json!([
            { "from": 5 * 120_000, "to": 10 * 120_000 },
            { "from": 10 * 120_000, "to": 13 * 120_000 },
        ])
    );
    assert_eq!(m["session"]["mic_muted"], m["mic_muted"]);
}

/// AUD-6: a missing recording, a recording without a mic track and an unknown id are refused
/// with the shared codes (`not_found` like pause, `invalid_input`).
#[tokio::test]
async fn aud6_mute_needs_a_running_recording_with_a_mic_track() {
    let r = rig();
    let e = user(&r.daemon, "set_mic_muted", json!({ "muted": true }))
        .await
        .unwrap_err();
    assert_eq!(err_code(&e), ErrorCode::NotFound);
    let pause_err = user(&r.daemon, "pause_recording", json!({}))
        .await
        .unwrap_err();
    assert_eq!(err_code(&e), err_code(&pause_err), "same code as pause");

    start(&r, json!({})).await;
    let e = user(&r.daemon, "set_mic_muted", json!({ "muted": true }))
        .await
        .unwrap_err();
    assert_eq!(err_code(&e), ErrorCode::InvalidInput);
    assert!(e.to_string().contains("no microphone"), "{e}");
    let st = user(&r.daemon, "get_recording_status", json!({}))
        .await
        .unwrap();
    assert_eq!(st["mic_muted"], false);
    let e = user(
        &r.daemon,
        "set_mic_muted",
        json!({ "muted": true, "recording_id": "rec_nope" }),
    )
    .await
    .unwrap_err();
    assert_eq!(err_code(&e), ErrorCode::NotFound);
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
}

/// AUD-6: the action is Free for agents (the recording was confirmed already) and bad input is
/// rejected before anything runs.
#[tokio::test]
async fn aud6_agents_mute_without_a_dialog_and_input_is_validated() {
    let r = rig_with(FakeSources::new(), Some(Decision::Approve));
    r.daemon
        .execute_method(
            agent("claude"),
            0,
            "start_recording",
            &json!({ "target": { "display_id": 1 }, "countdown_s": 0, "mic": true }),
        )
        .await
        .unwrap();
    let shown = r.confirmer.as_ref().unwrap().shown().len();
    let v = r
        .daemon
        .execute_method(
            agent("claude"),
            0,
            "set_mic_muted",
            &json!({ "muted": true, "request_id": "m1" }),
        )
        .await
        .unwrap();
    assert_eq!(v["muted"], true);
    assert_eq!(
        r.confirmer.as_ref().unwrap().shown().len(),
        shown,
        "no dialog"
    );
    for bad in [
        json!({}),
        json!({ "muted": "yes" }),
        json!({ "muted": true, "x": 1 }),
    ] {
        let e = user(&r.daemon, "set_mic_muted", bad.clone())
            .await
            .unwrap_err();
        assert_eq!(err_code(&e), ErrorCode::InvalidInput, "{bad}");
    }
    // The agent's mute shows in the status a HUD polls.
    let st = user(&r.daemon, "get_recording_status", json!({}))
        .await
        .unwrap();
    assert_eq!(st["mic_muted"], true);
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
}

/// AUD-6: muting during the resume countdown works and does not cancel the resume.
#[tokio::test]
async fn aud6_mute_during_the_resume_countdown_does_not_interrupt_it() {
    let r = rig();
    user(
        &r.daemon,
        "set_settings",
        json!({ "values": { "recording.resume_countdown_s": 3 } }),
    )
    .await
    .unwrap();
    start(&r, json!({ "mic": true })).await;
    user(&r.daemon, "pause_recording", json!({})).await.unwrap();
    let d = r.daemon.clone();
    let resuming = tokio::spawn(async move { user(&d, "resume_recording", json!({})).await });
    status_where(&r.daemon, |s| s["state"] == "resuming").await;
    let m = mute(&r, true).await;
    assert_eq!(m["changed"], true);
    let res = resuming.await.unwrap().unwrap();
    assert_eq!(res["state"], "recording", "{res}");
    let st = user(&r.daemon, "get_recording_status", json!({}))
        .await
        .unwrap();
    assert_eq!(st["state"], "recording");
    assert_eq!(st["mic_muted"], true);
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
}
