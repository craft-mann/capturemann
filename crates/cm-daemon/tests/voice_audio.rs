//! Audio-only recording (VOC-1) through the daemon, over fake sources: `start_recording
//! { mode: "audio" }`, the Confirm dialog, markers, permissions, the sleep assertion and the
//! stop reasons. No microphone, no screen, no real power assertion.

#![allow(clippy::unwrap_used)]

mod common;

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use cm_capture::disk::FakeFreeSpace;
use cm_core::{Actor, CmError, ErrorCode, ManualHostClock, Permission};
use cm_daemon::recording::FakeIdleSleep;
use cm_daemon::testing::{FakeSources, FixedConfirmer};
use cm_daemon::{
    Clock, ConfirmationRequest, Confirmer, Daemon, DaemonError, Decision, EngineOptions, Event,
    ManualClock,
};
use cm_jobs::{JobContext, JobHandler, JOB_EXPORT};
use common::{builder, desktop};
use serde_json::{json, Value};
use tokio::sync::broadcast::Receiver;

const ORIGIN_NS: u64 = 5_000_000_000;
const SEC: u64 = 1_000_000_000;
const TICKS: f64 = 120_000.0;

struct SharedClock(Arc<ManualClock>);

impl Clock for SharedClock {
    fn now(&self) -> time::OffsetDateTime {
        self.0.now()
    }
}

struct Shared(Arc<FixedConfirmer>);

#[async_trait]
impl Confirmer for Shared {
    async fn confirm(&self, r: &ConfirmationRequest) -> Decision {
        self.0.confirm(r).await
    }
}

struct Rig {
    daemon: Daemon,
    sources: FakeSources,
    host: Arc<ManualHostClock>,
    wall: Arc<ManualClock>,
    sleep: FakeIdleSleep,
    dir: tempfile::TempDir,
    confirmer: Arc<FixedConfirmer>,
}

fn rig_with(sources: FakeSources) -> Rig {
    let (b, dir) = builder(desktop());
    let host = Arc::new(ManualHostClock::new(ORIGIN_NS));
    let free = Arc::new(FakeFreeSpace::new(500_000_000_000));
    let wall = Arc::new(ManualClock::new(
        time::macros::datetime!(2026-10-26 09:00 UTC),
    ));
    let sleep = FakeIdleSleep::new();
    let queue = cm_daemon::library_job_queue(dir.path()).unwrap();
    let opts = EngineOptions {
        tick_every: Duration::from_millis(15),
        countdown_step: Duration::from_millis(40),
        health_every_ticks: 2,
        stop_timeout: Duration::from_secs(10),
        shutdown_timeout: Duration::from_secs(10),
        host: host.clone(),
        free_space: free,
        disk_check_every: Duration::ZERO,
        own_pid: None,
        idle_sleep: Some(Arc::new(sleep.clone())),
        ffmpeg_probe: Some(Arc::new(|| true)),
        ..EngineOptions::default()
    };
    let confirmer = Arc::new(FixedConfirmer::new(Decision::Approve));
    let daemon = b
        .jobs(queue)
        .clock(SharedClock(wall.clone()))
        .recording_sources(sources.clone())
        .recording_options(opts)
        .confirmer(Shared(confirmer.clone()))
        .build();
    daemon
        .register_job_handler(JOB_EXPORT, Arc::new(StubExport))
        .unwrap();
    daemon.start_jobs().unwrap();
    Rig {
        daemon,
        sources,
        host,
        wall,
        sleep,
        dir,
        confirmer,
    }
}

fn rig() -> Rig {
    rig_with(FakeSources::new())
}

fn agent(name: &str) -> Actor {
    Actor::agent(name).unwrap()
}

async fn user(d: &Daemon, method: &str, params: Value) -> Result<Value, DaemonError> {
    d.execute_method(Actor::User, 0, method, &params).await
}

async fn as_agent(d: &Daemon, method: &str, params: Value) -> Result<Value, DaemonError> {
    d.execute_method(agent("claude"), 0, method, &params).await
}

fn audio(extra: Value) -> Value {
    let mut p = json!({ "mode": "audio", "countdown_s": 0 });
    for (k, v) in extra.as_object().unwrap() {
        p[k] = v.clone();
    }
    p
}

fn code(e: &DaemonError) -> ErrorCode {
    e.code().unwrap_or_else(|| panic!("no shared code: {e:?}"))
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

fn manifest(r: &Rig, rec_path: &str) -> Value {
    let text = std::fs::read_to_string(r.dir.path().join(rec_path).join("manifest.json")).unwrap();
    serde_json::from_str(&text).unwrap()
}

/// The bundle folder a stop result names.
fn path_of(stopped: &Value) -> String {
    stopped["path"].as_str().unwrap().to_owned()
}

/// Accepts the export job and does nothing: the audio rules must hold with an exporter around.
struct StubExport;

#[async_trait]
impl JobHandler for StubExport {
    async fn run(&self, _ctx: JobContext) -> Result<Value, CmError> {
        Ok(json!({}))
    }
}

// ---- AC1: mic only ----------------------------------------------------------------------------

/// VOC-1.AC1: microphone only, 60 s: the bundle is `kind: "audio"` with a mic track of 60 s and
/// no screen, camera or cursor track, and Screen Recording access was never asked for (it is
/// missing in this rig and the recording still starts).
#[tokio::test]
async fn voc1_ac1_mic_only_records_an_audio_bundle_and_never_needs_screen_recording() {
    let r = rig_with(FakeSources::new().with_missing_permission(Permission::Screen));
    let started = user(
        &r.daemon,
        "start_recording",
        audio(json!({ "title": "Interview" })),
    )
    .await
    .unwrap();
    assert_eq!(started["mode"], "audio");
    assert_eq!(started["tracks"], json!(["mic"]));
    assert_eq!(started["indicator"]["mode"], "audio");
    assert!(started["indicator"]["area"].is_null());
    assert_eq!(started["indicator"]["cursor"], false);
    // No screen plan was made and the screen was not touched.
    assert!(r.sources.last_plan().is_none());
    assert!(r.sources.handle(cm_core::TrackKind::Screen).is_none());
    assert!(r.daemon.recording_active());

    r.host.advance_ns(60 * SEC);
    let stopped = user(&r.daemon, "stop_recording", json!({})).await.unwrap();
    assert!(
        (stopped["duration_s"].as_f64().unwrap() - 60.0).abs() < 0.02,
        "{stopped}"
    );
    assert_eq!(stopped["reason"], "user");
    let kinds: Vec<&str> = stopped["tracks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["mic"]);

    let path = path_of(&stopped);
    let m = manifest(&r, &path);
    assert_eq!(m["kind"], "audio");
    assert_eq!(m["title"], "Interview");
    assert_eq!(m["settings"]["mode"], "audio");
    let tracks: Vec<&String> = m["tracks"].as_object().unwrap().keys().collect();
    assert_eq!(tracks, ["mic"]);
    for gone in ["screen.mp4", "camera.mp4", "cursor.jsonl"] {
        assert!(!r.dir.path().join(&path).join(gone).exists(), "{gone}");
    }
    // The library lists it as audio only; no automatic video export was queued.
    let audio_list = user(&r.daemon, "list_library", json!({ "kind": "audio" }))
        .await
        .unwrap();
    assert_eq!(audio_list["captures"].as_array().unwrap().len(), 1);
    assert_eq!(audio_list["captures"][0]["kind"], "audio");
    let rec_list = user(&r.daemon, "list_library", json!({ "kind": "recording" }))
        .await
        .unwrap();
    assert!(rec_list["captures"].as_array().unwrap().is_empty());
    // An exporter and FFmpeg exist in this rig, and still no video export was queued.
    let jobs = user(&r.daemon, "list_jobs", json!({})).await.unwrap();
    assert!(jobs["jobs"].as_array().unwrap().is_empty(), "{jobs}");
}

/// Sources and presets: system audio is its own track; `online` adds it to the microphone;
/// `mic: false` with nothing else is refused.
#[tokio::test]
async fn voc1_sources_and_presets() {
    let r = rig();
    let s = user(
        &r.daemon,
        "start_recording",
        audio(json!({ "preset": "online" })),
    )
    .await
    .unwrap();
    assert_eq!(s["tracks"], json!(["mic", "system"]));
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();

    let s = user(
        &r.daemon,
        "start_recording",
        audio(json!({ "preset": "online", "system_audio": false })),
    )
    .await
    .unwrap();
    assert_eq!(
        s["tracks"],
        json!(["mic"]),
        "the request wins over the preset"
    );
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();

    let s = user(
        &r.daemon,
        "start_recording",
        audio(json!({ "mic": false, "system_audio": true })),
    )
    .await
    .unwrap();
    assert_eq!(s["tracks"], json!(["system"]));
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();

    for bad in [
        audio(json!({ "mic": false })),
        audio(json!({ "preset": "party" })),
        audio(json!({ "mode": "video" })),
        json!({ "target": { "display_id": 1 }, "preset": "room" }),
    ] {
        let e = user(&r.daemon, "start_recording", bad.clone())
            .await
            .unwrap_err();
        assert_eq!(code(&e), ErrorCode::InvalidInput, "{bad}");
    }
    assert_eq!(r.sources.builds(), 3);
}

/// Edge cases: system audio without Screen Recording is `permission_missing` (`screen`) with the
/// macOS explanation, before any dialog; a missing microphone permission likewise.
#[tokio::test]
async fn voc1_permissions_fail_before_the_dialog() {
    let r = rig_with(FakeSources::new().with_missing_permission(Permission::Screen));
    let e = as_agent(
        &r.daemon,
        "start_recording",
        audio(json!({ "system_audio": true })),
    )
    .await
    .unwrap_err();
    assert_eq!(code(&e), ErrorCode::PermissionMissing);
    assert_eq!(e.to_rpc_error()["data"]["data"]["permission"], "screen");
    assert!(
        e.to_string()
            .contains("System audio needs Screen Recording permission on macOS"),
        "{e}"
    );
    assert!(r.confirmer.shown().is_empty());
    assert!(bundles(&r).is_empty());
    assert_eq!(r.sources.builds(), 0);

    let r = rig_with(FakeSources::new().with_missing_permission(Permission::Microphone));
    let e = as_agent(&r.daemon, "start_recording", audio(json!({})))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::PermissionMissing);
    assert_eq!(e.to_rpc_error()["data"]["data"]["permission"], "microphone");
    assert!(r.confirmer.shown().is_empty());
}

/// A screen recording (or its countdown) running makes an audio start `busy`, as REC-1.AC6.
#[tokio::test]
async fn voc1_a_running_recording_makes_the_start_busy() {
    let r = rig();
    user(
        &r.daemon,
        "start_recording",
        json!({ "target": { "display_id": 1 }, "countdown_s": 0 }),
    )
    .await
    .unwrap();
    let e = as_agent(&r.daemon, "start_recording", audio(json!({})))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::Busy);
    assert!(r.confirmer.shown().is_empty());
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
}

// ---- AC9, AC7, AC8: the agent interface ----------------------------------------------------------

/// VOC-1.AC9: `mode: "audio"` together with any screen-only field fails with `invalid_input`
/// naming the field, for the user and for an agent, with no dialog and nothing started.
#[tokio::test]
async fn voc1_ac9_audio_mode_refuses_screen_only_fields() {
    let r = rig();
    for (field, value) in [
        ("target", json!({ "display_id": 1 })),
        ("camera", json!(true)),
        ("fps", json!(30)),
        ("size", json!("720p")),
        ("codec", json!("h264")),
        ("quality", json!("high")),
        ("cursor", json!(true)),
        ("cursor_separate", json!(true)),
        ("exclude", json!({ "bundle_ids": ["x"] })),
    ] {
        let p = audio(json!({ field: value }));
        for who in ["agent", "user"] {
            let e = if who == "agent" {
                as_agent(&r.daemon, "start_recording", p.clone()).await
            } else {
                user(&r.daemon, "start_recording", p.clone()).await
            }
            .unwrap_err();
            assert_eq!(code(&e), ErrorCode::InvalidInput, "{field} as {who}");
            assert!(
                e.to_string().contains(field) && e.to_string().contains("audio"),
                "{field}: {e}"
            );
        }
    }
    assert!(r.confirmer.shown().is_empty(), "no dialog opened");
    assert_eq!(r.sources.builds(), 0);
    assert!(bundles(&r).is_empty());
    assert!(!r.daemon.recording_active());
}

/// VOC-1.AC8 and AC7: the Confirm dialog says "Record audio" with the microphone name and no
/// screen target, carries the consent sentence (also when `recording.consent_notice` is off),
/// and on approval a `kind: "audio"` recording starts.
#[tokio::test]
async fn voc1_ac7_ac8_the_agent_dialog_says_record_audio_with_the_mic_and_the_consent_line() {
    let r = rig();
    user(
        &r.daemon,
        "set_settings",
        json!({ "values": { "recording.consent_notice": false } }),
    )
    .await
    .unwrap();
    let started = as_agent(
        &r.daemon,
        "start_recording",
        audio(json!({ "mic": true, "preset": "room", "max_duration_s": 3600, "title": "1:1" })),
    )
    .await
    .unwrap();
    assert_eq!(started["indicator"]["started_by"]["kind"], "agent");
    assert_eq!(started["tracks"], json!(["mic"]));
    let shown = r.confirmer.shown();
    assert_eq!(shown.len(), 1);
    let q = &shown[0];
    assert_eq!(q.tool, "start_recording");
    assert!(q.summary.starts_with("Record audio"), "{}", q.summary);
    assert!(q.summary.contains("Fake Microphone"), "{}", q.summary);
    assert!(!q.summary.contains("display"), "{}", q.summary);
    let rows = q.details.as_object().unwrap();
    for gone in [
        "target",
        "screen",
        "camera",
        "cursor",
        "output",
        "excluded_windows",
    ] {
        assert!(!rows.contains_key(gone), "no `{gone}` row: {rows:?}");
    }
    assert!(rows["microphone"]
        .as_str()
        .unwrap()
        .starts_with("On: Fake Microphone"));
    assert_eq!(rows["system_audio"], "Off");
    assert_eq!(rows["max_duration"], "1 h");
    assert_eq!(rows["consent"], "Let everyone know they're being recorded.");
    assert_eq!(q.params["mode"], "audio");
    assert_eq!(
        q.params["consent"],
        "Let everyone know they're being recorded."
    );
    assert_eq!(q.params["consent_notice"], false);
    assert!(q.params.get("target").is_none());

    let stopped = user(&r.daemon, "stop_recording", json!({})).await.unwrap();
    assert_eq!(manifest(&r, &path_of(&stopped))["kind"], "audio");
}

/// A declined audio recording creates nothing and holds nothing.
#[tokio::test]
async fn voc1_a_declined_audio_recording_leaves_nothing() {
    let (b, dir) = builder(desktop());
    let confirmer = Arc::new(FixedConfirmer::new(Decision::Decline));
    let sleep = FakeIdleSleep::new();
    let daemon = b
        .recording_sources(FakeSources::new())
        .recording_options(EngineOptions {
            own_pid: None,
            idle_sleep: Some(Arc::new(sleep.clone())),
            ..EngineOptions::default()
        })
        .confirmer(Shared(confirmer.clone()))
        .build();
    let e = as_agent(&daemon, "start_recording", audio(json!({})))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::UserDeclined);
    assert!(!daemon.recording_active());
    assert_eq!(sleep.acquired(), 0);
    assert!(std::fs::read_dir(dir.path())
        .unwrap()
        .flatten()
        .all(|e| !e.file_name().to_string_lossy().ends_with(".cmrec")));
}

// ---- markers ----------------------------------------------------------------------------------------

/// VOC-1.AC4: a marker at 02:00 of recorded time stays at 120 s although a 30 s pause came
/// before it; markers set after stop sort in, carry who set them, and undo.
#[tokio::test]
async fn voc1_ac4_markers_are_in_recorded_time_and_survive_pauses() {
    let r = rig();
    let mut rx = r.daemon.subscribe();
    user(&r.daemon, "start_recording", audio(json!({})))
        .await
        .unwrap();
    r.host.advance_ns(60 * SEC);
    let m1 = user(&r.daemon, "add_marker", json!({ "label": "Budget" }))
        .await
        .unwrap();
    assert!(
        (m1["marker"]["at"].as_f64().unwrap() - 60.0).abs() < 0.05,
        "{m1}"
    );
    assert_eq!(m1["marker"]["label"], "Budget");
    assert_eq!(m1["marker"]["by"], "user");
    assert_eq!(
        wait_event(&mut rx, "recording.marker").await.data["at_ms"],
        60_000
    );

    user(&r.daemon, "pause_recording", json!({})).await.unwrap();
    r.host.advance_ns(30 * SEC);
    // A marker while paused sits at the paused timer, not at the wall time.
    let paused = as_agent(&r.daemon, "add_marker", json!({})).await.unwrap();
    assert!(
        (paused["marker"]["at"].as_f64().unwrap() - 60.0).abs() < 0.05,
        "{paused}"
    );
    assert_eq!(paused["marker"]["by"], "agent:claude");
    user(&r.daemon, "resume_recording", json!({}))
        .await
        .unwrap();
    r.host.advance_ns(60 * SEC);
    let m2 = user(&r.daemon, "add_marker", json!({ "label": "Decision" }))
        .await
        .unwrap();
    let at = m2["marker"]["at"].as_f64().unwrap();
    assert!(
        (at - 120.0).abs() < 0.05,
        "marker at {at} s, not 150 s: {m2}"
    );
    assert_eq!(m2["count"], 3);

    let stopped = user(&r.daemon, "stop_recording", json!({})).await.unwrap();
    let id = stopped["capture_id"].as_str().unwrap().to_owned();
    let path = path_of(&stopped);
    let m = manifest(&r, &path);
    let markers = m["markers"].as_array().unwrap();
    assert_eq!(markers.len(), 3);
    let ticks: Vec<f64> = markers.iter().map(|x| x["at"].as_f64().unwrap()).collect();
    assert!((ticks[2] - 120.0 * TICKS).abs() < 0.05 * TICKS, "{ticks:?}");
    assert_eq!(markers[2]["label"], "Decision");
    assert!(markers[1].get("label").is_none());
    assert!(markers
        .windows(2)
        .all(|w| w[0]["at"].as_f64() <= w[1]["at"].as_f64()));

    // After stop: `capture_id` + `at`, one revision, one undo step.
    let before = m["revisions"].as_array().unwrap().len();
    let added = as_agent(
        &r.daemon,
        "add_marker",
        json!({ "capture_id": id, "at": 90, "label": "Late note" }),
    )
    .await
    .unwrap();
    assert_eq!(added["action"], "add_marker");
    assert_eq!(added["by"], "agent:claude");
    assert_eq!(added["can_undo"], true);
    let m = manifest(&r, &path);
    assert_eq!(m["markers"].as_array().unwrap().len(), 4);
    assert_eq!(
        m["markers"][2]["label"], "Late note",
        "sorted between 60 and 120 s"
    );
    assert_eq!(m["revisions"].as_array().unwrap().len(), before + 1);
    assert_eq!(
        m["revisions"].as_array().unwrap().last().unwrap()["by"],
        "agent:claude"
    );
    user(&r.daemon, "undo", json!({ "capture_id": id }))
        .await
        .unwrap();
    assert_eq!(manifest(&r, &path)["markers"].as_array().unwrap().len(), 3);

    // Outside the recording is `invalid_input`; so is a mix of the two forms.
    for bad in [
        json!({ "capture_id": id, "at": 500 }),
        json!({ "capture_id": id }),
        json!({ "at": 5 }),
    ] {
        let e = user(&r.daemon, "add_marker", bad.clone())
            .await
            .unwrap_err();
        assert_eq!(code(&e), ErrorCode::InvalidInput, "{bad}");
    }
}

/// Without a recording the running-recording form is `not_found`; during the countdown
/// `invalid_input`.
/// The HUD's "Add a label (optional)" after a marker: `at` with `label` names the marker set at
/// that time and adds none; no marker there is `not_found`; `at` without a label is refused.
#[tokio::test]
async fn voc1_a_marker_can_be_labelled_after_it_was_set() {
    let r = rig();
    user(&r.daemon, "start_recording", audio(json!({})))
        .await
        .unwrap();
    r.host.advance_ns(12 * SEC);
    let m = user(&r.daemon, "add_marker", json!({})).await.unwrap();
    let at = m["marker"]["at"].as_f64().unwrap();
    let named = user(
        &r.daemon,
        "add_marker",
        json!({ "at": at, "label": "Budget" }),
    )
    .await
    .unwrap();
    assert_eq!(named["marker"]["label"], "Budget");
    assert_eq!(named["count"], 1, "{named}");
    let e = user(&r.daemon, "add_marker", json!({ "at": 99.0, "label": "x" }))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::NotFound);
    let e = user(&r.daemon, "add_marker", json!({ "at": at }))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
    let stopped = user(&r.daemon, "stop_recording", json!({})).await.unwrap();
    let m = manifest(&r, &path_of(&stopped));
    let markers = m["markers"].as_array().unwrap();
    assert_eq!(markers.len(), 1);
    assert_eq!(markers[0]["label"], "Budget");
}

#[tokio::test]
async fn voc1_a_marker_needs_a_running_recording() {
    let r = rig();
    let e = user(&r.daemon, "add_marker", json!({})).await.unwrap_err();
    assert_eq!(code(&e), ErrorCode::NotFound);
    let daemon = r.daemon.clone();
    let start = tokio::spawn(async move {
        user(
            &daemon,
            "start_recording",
            json!({ "mode": "audio", "countdown_s": 3 }),
        )
        .await
    });
    tokio::time::sleep(Duration::from_millis(60)).await;
    let e = user(&r.daemon, "add_marker", json!({})).await.unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
    let _ = start.await;
}

// ---- long sessions ------------------------------------------------------------------------------------

/// The idle-sleep assertion is held for the length of an audio recording and released however it
/// ends; a screen recording does not take it.
#[tokio::test]
async fn voc1_idle_sleep_is_held_while_recording_and_released_after() {
    let r = rig();
    assert_eq!(r.sleep.active(), 0);
    user(&r.daemon, "start_recording", audio(json!({})))
        .await
        .unwrap();
    assert_eq!((r.sleep.acquired(), r.sleep.active()), (1, 1));
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
    assert_eq!(r.sleep.active(), 0);

    user(&r.daemon, "start_recording", audio(json!({})))
        .await
        .unwrap();
    assert_eq!(r.sleep.active(), 1);
    user(&r.daemon, "discard_recording", json!({}))
        .await
        .unwrap();
    assert_eq!(r.sleep.active(), 0);

    user(
        &r.daemon,
        "start_recording",
        json!({ "target": { "display_id": 1 }, "countdown_s": 0 }),
    )
    .await
    .unwrap();
    assert_eq!(
        r.sleep.acquired(),
        2,
        "a screen recording takes no assertion"
    );
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
}

/// `max_duration_s` ends the recording with that reason; without it the
/// `recording.audio_max_duration_s` setting (default 4 h) applies; the limits are checked.
#[tokio::test]
async fn voc1_max_duration_stops_the_recording_and_the_setting_is_the_default() {
    let r = rig();
    let mut rx = r.daemon.subscribe();
    let s = user(&r.daemon, "start_recording", audio(json!({})))
        .await
        .unwrap();
    assert_eq!(s["max_duration_s"], 4.0 * 3600.0, "the default is 4 h");
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();

    user(
        &r.daemon,
        "set_settings",
        json!({ "values": { "recording.audio_max_duration_s": 7200 } }),
    )
    .await
    .unwrap();
    let s = user(&r.daemon, "start_recording", audio(json!({})))
        .await
        .unwrap();
    assert_eq!(s["max_duration_s"], 7200.0);
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();

    user(
        &r.daemon,
        "start_recording",
        audio(json!({ "max_duration_s": 5 })),
    )
    .await
    .unwrap();
    r.host.advance_ns(6 * SEC);
    let stopped = wait_event(&mut rx, "recording.stopped").await;
    // (the two earlier stops are in the channel first)
    let mut stopped = stopped;
    while stopped.data["reason"] != "max_duration" {
        stopped = wait_event(&mut rx, "recording.stopped").await;
    }
    assert!(stopped.data["capture_id"].is_string());
    assert!(!r.daemon.recording_active());

    // 12 h is the longest; the setting is checked and is not a thing an agent may switch off.
    let e = user(
        &r.daemon,
        "start_recording",
        audio(json!({ "max_duration_s": 43_201 })),
    )
    .await
    .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
    let e = user(
        &r.daemon,
        "set_settings",
        json!({ "values": { "recording.audio_max_duration_s": 50_000 } }),
    )
    .await
    .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
    let e = as_agent(
        &r.daemon,
        "set_settings",
        json!({ "values": { "recording.consent_notice": false } }),
    )
    .await
    .unwrap_err();
    assert!(
        e.to_rpc_error().to_string().contains("app_only_setting"),
        "the consent notice is app-only: {e}"
    );
    let e = user(
        &r.daemon,
        "set_settings",
        json!({ "values": { "recording.consent_notice": "no" } }),
    )
    .await
    .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
}

/// Edge case: the Mac goes to sleep. The wall clock jumps while the recording timer does not;
/// the recording stops with `reason: "system_sleep"` and keeps what was captured.
#[tokio::test]
async fn voc1_a_sleeping_computer_stops_the_recording_with_system_sleep() {
    let r = rig();
    let mut rx = r.daemon.subscribe();
    user(&r.daemon, "start_recording", audio(json!({})))
        .await
        .unwrap();
    r.host.advance_ns(30 * SEC);
    // Ten seconds of ordinary wall time is no sleep ...
    r.wall.advance(time::Duration::seconds(10));
    tokio::time::sleep(Duration::from_millis(80)).await;
    assert!(r.daemon.recording_active());
    // ... but an hour that the recording clock did not see is.
    r.wall.advance(time::Duration::hours(1));
    let stopped = wait_event(&mut rx, "recording.stopped").await;
    assert_eq!(stopped.data["reason"], "system_sleep");
    let id = stopped.data["capture_id"].as_str().unwrap();
    assert!((stopped.data["duration_s"].as_f64().unwrap() - 30.0).abs() < 0.05);
    let list = user(&r.daemon, "list_library", json!({ "kind": "audio" }))
        .await
        .unwrap();
    assert_eq!(list["captures"][0]["id"], id, "kept what was captured");
    assert_eq!(r.sleep.active(), 0);
}

/// A screen recording does not stop for a clock jump (the rule is for audio recordings).
#[tokio::test]
async fn voc1_a_screen_recording_ignores_clock_jumps() {
    let r = rig();
    user(
        &r.daemon,
        "start_recording",
        json!({ "target": { "display_id": 1 }, "countdown_s": 0 }),
    )
    .await
    .unwrap();
    r.wall.advance(time::Duration::hours(1));
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(r.daemon.recording_active());
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
}

/// An audio recording has no video: export, publish and the like say so instead of failing
/// obscurely, while the audio tools (edit, captions) accept it.
#[tokio::test]
async fn voc1_an_audio_recording_has_no_video_to_export_but_can_be_edited() {
    let r = rig();
    user(&r.daemon, "start_recording", audio(json!({})))
        .await
        .unwrap();
    r.host.advance_ns(20 * SEC);
    let stopped = user(&r.daemon, "stop_recording", json!({})).await.unwrap();
    let id = stopped["capture_id"].as_str().unwrap();
    let e = user(&r.daemon, "export_video", json!({ "capture_id": id }))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
    assert!(e.to_string().contains("audio recording"), "{e}");
    // Trim works on an audio recording: no screen track is needed to find its end.
    let t = user(
        &r.daemon,
        "trim",
        json!({ "capture_id": id, "start": 2, "end": 15 }),
    )
    .await
    .unwrap();
    assert_eq!(t["revision"], 2, "{t}");
}

/// VOC-1: `people` (1 to 8) is the answer to "How many people?"; it is stored in the manifest
/// settings next to the preset (no migration), shown in the agent's dialog, and refused outside
/// audio mode or out of range.
#[tokio::test]
async fn voc1_people_is_stored_in_the_manifest_settings_and_validated() {
    let r = rig();
    for bad in [
        audio(json!({ "people": 0 })),
        audio(json!({ "people": 9 })),
        audio(json!({ "people": "four" })),
        audio(json!({ "people": 2.5 })),
        json!({ "target": { "display_id": 1 }, "people": 2 }),
    ] {
        let e = user(&r.daemon, "start_recording", bad.clone())
            .await
            .unwrap_err();
        assert_eq!(code(&e), ErrorCode::InvalidInput, "{bad}");
    }
    let started = as_agent(
        &r.daemon,
        "start_recording",
        audio(json!({ "preset": "room", "people": 4 })),
    )
    .await
    .unwrap();
    assert_eq!(started["tracks"], json!(["mic"]));
    let shown = r.confirmer.shown();
    assert_eq!(shown.last().unwrap().details["people"], 4);
    assert_eq!(shown.last().unwrap().params["people"], 4);
    let stopped = user(&r.daemon, "stop_recording", json!({})).await.unwrap();
    let m = manifest(&r, &path_of(&stopped));
    assert_eq!(m["settings"]["people"], 4, "{m}");
    assert_eq!(m["settings"]["preset"], "room");

    // Absent stays absent (unknown).
    user(&r.daemon, "start_recording", audio(json!({})))
        .await
        .unwrap();
    let stopped = user(&r.daemon, "stop_recording", json!({})).await.unwrap();
    let m = manifest(&r, &path_of(&stopped));
    assert!(m["settings"]["people"].is_null(), "{m}");
}
