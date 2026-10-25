//! REC-3: the 60 fps self-test and the gate it opens, through the daemon over fake sources.
//! AC1: a user who selects 60 fps gets the self-test before the setting counts; AC2: a failed
//! self-test keeps 60 fps off and suggests a lower size. Real time with a 120 ms "10 s" test.

#![allow(clippy::unwrap_used)]

mod common;

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use cm_capture::disk::FakeFreeSpace;
use cm_core::{Actor, CmError, ErrorCode, ManualHostClock, Permission};
use cm_daemon::testing::{FakeSources, FixedConfirmer};
use cm_daemon::{
    Clock, ConfirmationRequest, Confirmer, Daemon, DaemonError, Decision, EngineOptions, Event,
    ManualClock, MemorySettings, SettingsStore,
};
use common::{builder, desktop};
use serde_json::{json, Map, Value};
use time::macros::datetime;
use tokio::sync::broadcast::Receiver;

const SEC: u64 = 1_000_000_000;

struct Shared(Arc<FixedConfirmer>);

#[async_trait]
impl Confirmer for Shared {
    async fn confirm(&self, r: &ConfirmationRequest) -> Decision {
        self.0.confirm(r).await
    }
}

/// Settings that outlive a daemon (to show the result is persisted and loaded).
#[derive(Clone, Default)]
struct SharedSettings(Arc<MemorySettings>);

impl SettingsStore for SharedSettings {
    fn all(&self) -> Result<Map<String, Value>, CmError> {
        self.0.all()
    }
    fn set(&self, v: &Map<String, Value>) -> Result<Map<String, Value>, CmError> {
        self.0.set(v)
    }
}

#[derive(Clone)]
struct SharedClock(Arc<ManualClock>);

impl Clock for SharedClock {
    fn now(&self) -> time::OffsetDateTime {
        self.0.now()
    }
}

struct Rig {
    daemon: Daemon,
    sources: FakeSources,
    settings: SharedSettings,
    clock: Arc<ManualClock>,
    lib: tempfile::TempDir,
    st_dir: tempfile::TempDir,
    confirmer: Option<Arc<FixedConfirmer>>,
}

fn options(st_dir: &std::path::Path, ms: u64) -> EngineOptions {
    EngineOptions {
        tick_every: Duration::from_millis(15),
        countdown_step: Duration::from_millis(20),
        stop_timeout: Duration::from_secs(10),
        shutdown_timeout: Duration::from_secs(10),
        host: Arc::new(ManualHostClock::new(5 * SEC)),
        free_space: Arc::new(FakeFreeSpace::new(500_000_000_000)),
        disk_check_every: Duration::ZERO,
        own_pid: None,
        selftest_duration: Duration::from_millis(ms),
        selftest_dir: Some(st_dir.to_owned()),
        ..EngineOptions::default()
    }
}

fn rig_full(
    sources: FakeSources,
    decision: Option<Decision>,
    ms: u64,
    settings: SharedSettings,
) -> Rig {
    let (b, lib) = builder(desktop());
    let st_dir = tempfile::tempdir().unwrap();
    let clock = Arc::new(ManualClock::new(datetime!(2026-10-21 09:00 UTC)));
    let mut b = b
        .recording_sources(sources.clone())
        .recording_options(options(st_dir.path(), ms))
        .settings(settings.clone())
        .clock(SharedClock(clock.clone()));
    let confirmer = decision.map(|d| Arc::new(FixedConfirmer::new(d)));
    if let Some(c) = &confirmer {
        b = b.confirmer(Shared(c.clone()));
    }
    Rig {
        daemon: b.build(),
        sources,
        settings,
        clock,
        lib,
        st_dir,
        confirmer,
    }
}

fn rig() -> Rig {
    rig_full(FakeSources::new(), None, 120, SharedSettings::default())
}

fn agent() -> Actor {
    Actor::agent("claude-desktop").unwrap()
}

async fn user(d: &Daemon, method: &str, params: Value) -> Result<Value, DaemonError> {
    d.execute_method(Actor::User, 0, method, &params).await
}

fn test_params() -> Value {
    json!({ "target": { "display_id": 1 } })
}

fn start_params(fps: u32) -> Value {
    json!({ "target": { "display_id": 1 }, "countdown_s": 0, "fps": fps })
}

fn leftovers(dir: &std::path::Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect()
}

fn bundles(r: &Rig) -> Vec<String> {
    leftovers(r.lib.path())
        .into_iter()
        .filter(|n| n.ends_with(".cmrec"))
        .collect()
}

fn drain(rx: &mut Receiver<Event>) -> Vec<Event> {
    let mut v = Vec::new();
    while let Ok(e) = rx.try_recv() {
        v.push(e);
    }
    v
}

async fn until<T>(mut f: impl FnMut() -> Option<T>) -> T {
    for _ in 0..1000 {
        if let Some(v) = f() {
            return v;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("condition not reached in 10 s");
}

fn code(e: &DaemonError) -> ErrorCode {
    e.code().unwrap_or_else(|| panic!("no shared code: {e:?}"))
}

/// REC-3.AC1: a fresh install has no result, so 60 fps is refused, with the spec's suggestion;
/// 30 fps is unaffected.
#[tokio::test]
async fn rec3_ac1_fresh_install_refuses_60_with_a_suggestion() {
    let r = rig();
    let e = user(&r.daemon, "start_recording", start_params(60))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
    assert_eq!(e.rpc_code(), -32602);
    assert_eq!(e.exit_code(), 2);
    let p = e.payload();
    assert_eq!(p["code"], "invalid_input");
    assert!(p["message"].as_str().unwrap().contains("60 fps"), "{p}");
    let s = &p["data"]["suggestion"];
    assert_eq!(s["fps"], 30);
    assert_eq!(s["reason"], "not_validated");
    assert_eq!(s["run_selftest"]["method"], "run_fps_selftest");
    assert_eq!(s["run_selftest"]["cli"], "capturemann record selftest");
    // Nothing was created and nothing asked.
    assert!(bundles(&r).is_empty());
    assert_eq!(r.sources.builds(), 0);
    assert!(!r.daemon.recording_active());
    // 30 fps still works.
    user(&r.daemon, "start_recording", start_params(30))
        .await
        .unwrap();
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
}

/// A pass: the verdict is stored (readable through `get_settings`), the test output is gone, the
/// library is untouched, and 60 fps is accepted. The events carry the indicator facts.
#[tokio::test]
async fn a_passing_selftest_is_stored_deleted_and_unlocks_60() {
    let r = rig();
    let mut rx = r.daemon.subscribe();
    let v = user(&r.daemon, "run_fps_selftest", test_params())
        .await
        .unwrap();
    assert_eq!(v["state"], "passed");
    assert_eq!(v["passed"], true);
    assert_eq!(v["fps"], 60);
    assert_eq!(v["late_pct"], 0.0);
    assert_eq!(v["dropped"], 0);
    assert_eq!(v["stored"], true);
    assert_eq!(v["display_id"], 1);
    assert_eq!(v["setting"], "recording.fps60_selftest");
    assert!(v["selftest_id"].as_str().unwrap().starts_with("st_"));

    // Nothing persistent: no bundle in the library, no folder left in the temp location, no
    // capture indexed.
    assert!(bundles(&r).is_empty(), "{:?}", leftovers(r.lib.path()));
    assert!(leftovers(r.st_dir.path()).is_empty());
    let lib = user(&r.daemon, "list_library", json!({})).await.unwrap();
    assert!(lib["captures"].as_array().unwrap().is_empty());
    assert!(!r.daemon.selftest_active());

    // The verdict is a setting.
    let got = user(
        &r.daemon,
        "get_settings",
        json!({ "keys": ["recording.fps60_selftest"] }),
    )
    .await
    .unwrap();
    let store = &got["settings"]["recording.fps60_selftest"];
    assert_eq!(store["version"], 1);
    let entries = store["results"].as_object().unwrap();
    assert_eq!(entries.len(), 1);
    let e = entries.values().next().unwrap();
    assert_eq!(e["passed"], true);
    assert_eq!(e["display"], 1);
    assert_eq!(e["codec"], v["codec"]);
    assert_eq!(e["at"], "2026-10-21T09:00:00Z");
    assert_eq!(e["app_version"], env!("CARGO_PKG_VERSION"));

    // Events: started (with the indicator facts), running, passed.
    let ev = drain(&mut rx);
    let tests: Vec<&Event> = ev
        .iter()
        .filter(|e| e.name == "recording.selftest")
        .collect();
    let states: Vec<&str> = tests
        .iter()
        .map(|e| e.data["state"].as_str().unwrap())
        .collect();
    assert_eq!(states.first(), Some(&"started"), "{states:?}");
    assert_eq!(states.last(), Some(&"passed"), "{states:?}");
    assert!(states.contains(&"running"), "{states:?}");
    let started = &tests[0].data;
    assert_eq!(started["indicator"]["selftest"], true);
    assert_eq!(started["indicator"]["output"]["fps"], 60);
    assert_eq!(started["indicator"]["area"]["kind"], "display");
    assert_eq!(started["indicator"]["mic"], false);
    assert_eq!(started["indicator"]["camera"], Value::Null);
    assert_eq!(started["indicator"]["started_by"]["kind"], "user");
    assert_eq!(
        started["indicator"]["started_by"]["tool"],
        "run_fps_selftest"
    );
    assert!(started["total_s"].as_f64().is_some());
    let running = tests.iter().find(|e| e.data["state"] == "running").unwrap();
    assert!(running.data["remaining_s"].is_number() && running.data["late_pct"].is_number());
    assert_eq!(tests.last().unwrap().data["result"]["passed"], true);
    // It is not a recording: no recording.* lifecycle events, no capture.
    for n in ["recording.started", "recording.stopped", "capture.created"] {
        assert!(ev.iter().all(|e| e.name != n), "{n}");
    }

    // The gate opens: 60 fps is accepted for the same display, size and codec.
    let s = user(&r.daemon, "start_recording", start_params(60))
        .await
        .unwrap();
    assert_eq!(s["indicator"]["output"]["fps"], 60);
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
}

/// REC-3.AC2: a failed self-test (more than 1% late frames) keeps 60 fps off and offers 30 fps
/// (and a lower size when there is one) with the spec's wording.
#[tokio::test]
async fn rec3_ac2_a_failed_selftest_keeps_60_off() {
    let r = rig_full(
        FakeSources::new().with_screen_quality(4.0, 0),
        None,
        120,
        SharedSettings::default(),
    );
    let v = user(&r.daemon, "run_fps_selftest", test_params())
        .await
        .unwrap();
    assert_eq!(v["state"], "failed");
    assert_eq!(v["passed"], false);
    assert_eq!(v["late_pct"], 4.0);
    assert_eq!(v["stored"], true);
    assert!(
        v["message"]
            .as_str()
            .unwrap()
            .contains("isn't stable on this machine"),
        "{v}"
    );
    assert_eq!(v["suggestion"]["fps"], 30);
    assert!(
        leftovers(r.st_dir.path()).is_empty(),
        "output deleted on a fail too"
    );
    assert!(bundles(&r).is_empty());

    let e = user(&r.daemon, "start_recording", start_params(60))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
    let p = e.payload();
    assert_eq!(p["data"]["reason"], "failed");
    assert_eq!(p["data"]["suggestion"]["fps"], 30);
    assert!(
        p["message"].as_str().unwrap().contains("isn't stable"),
        "{p}"
    );
    assert_eq!(r.sources.builds(), 1, "only the self-test built tracks");

    // The machine got better: a later pass replaces the fail.
    r.sources.set_screen_quality(0.2, 0);
    r.clock.advance(time::Duration::hours(1));
    let v = user(&r.daemon, "run_fps_selftest", test_params())
        .await
        .unwrap();
    assert_eq!(v["state"], "passed");
    user(&r.daemon, "start_recording", start_params(60))
        .await
        .unwrap();
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
}

/// Dropped frames count too (NFR-2 1% rule): 3% dropped fails even with no late frames.
#[tokio::test]
async fn dropped_frames_fail_the_selftest() {
    let r = rig_full(
        FakeSources::new().with_screen_quality(0.0, 20),
        None,
        120,
        SharedSettings::default(),
    );
    let v = user(&r.daemon, "run_fps_selftest", test_params())
        .await
        .unwrap();
    assert_eq!(v["state"], "failed", "{v}");
    assert_eq!(v["dropped"], 20);
    assert!(v["dropped_pct"].as_f64().unwrap() > 1.0);
    // Exactly at the limit passes: 1% late.
    r.sources.set_screen_quality(1.0, 0);
    r.clock.advance(time::Duration::hours(1));
    let v = user(&r.daemon, "run_fps_selftest", test_params())
        .await
        .unwrap();
    assert_eq!(v["state"], "passed", "{v}");
}

/// A pass is not forever: after 30 days (or for other display, codec or output size) the gate
/// refuses again and says why.
#[tokio::test]
async fn stale_and_changed_settings_are_refused() {
    let r = rig();
    user(&r.daemon, "run_fps_selftest", test_params())
        .await
        .unwrap();
    // 29 days later it still holds.
    r.clock.advance(time::Duration::days(29));
    user(&r.daemon, "start_recording", start_params(60))
        .await
        .unwrap();
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
    // 31 days after the run it is stale.
    r.clock.advance(time::Duration::days(2));
    let e = user(&r.daemon, "start_recording", start_params(60))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
    assert_eq!(e.payload()["data"]["suggestion"]["reason"], "stale");
    assert!(e.to_string().contains("older than 30 days"), "{e}");

    // Running it again makes it current.
    user(&r.daemon, "run_fps_selftest", test_params())
        .await
        .unwrap();
    user(&r.daemon, "start_recording", start_params(60))
        .await
        .unwrap();
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();

    // Another display and another codec were never tested.
    let other_display = json!({ "target": { "display_id": 2 }, "countdown_s": 0, "fps": 60 });
    let e = user(&r.daemon, "start_recording", other_display)
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
    assert_eq!(e.payload()["data"]["suggestion"]["reason"], "not_validated");
    let codec = json!({ "target": { "display_id": 1 }, "countdown_s": 0, "fps": 60,
                        "codec": "h264" });
    let first = user(&r.daemon, "start_recording", codec.clone()).await;
    // The auto codec for this small display is H.264, so naming it is the same machine and
    // setup; the other codec is the one that was never tested.
    let other = if first.is_ok() {
        user(&r.daemon, "stop_recording", json!({})).await.unwrap();
        json!({ "target": { "display_id": 1 }, "countdown_s": 0, "fps": 60, "codec": "hevc" })
    } else {
        codec
    };
    let e = user(&r.daemon, "start_recording", other).await.unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
}

/// The result survives a restart: a second daemon on the same settings accepts 60 fps.
#[tokio::test]
async fn the_result_is_persisted_and_loaded() {
    let first = rig();
    user(&first.daemon, "run_fps_selftest", test_params())
        .await
        .unwrap();
    let second = rig_full(FakeSources::new(), None, 120, first.settings.clone());
    second.clock.advance(time::Duration::days(1));
    let s = user(&second.daemon, "start_recording", start_params(60))
        .await
        .unwrap();
    assert_eq!(s["indicator"]["output"]["fps"], 60);
    user(&second.daemon, "stop_recording", json!({}))
        .await
        .unwrap();
}

/// A default of 60 in the settings never blocks a recording: it falls back to 30 with a warning
/// until the self-test passes.
#[tokio::test]
async fn a_default_of_60_falls_back_to_30_with_a_warning() {
    let r = rig();
    user(
        &r.daemon,
        "set_settings",
        json!({ "values": { "recording.fps": 60 } }),
    )
    .await
    .unwrap();
    let p = json!({ "target": { "display_id": 1 }, "countdown_s": 0 });
    let s = user(&r.daemon, "start_recording", p.clone()).await.unwrap();
    assert_eq!(s["indicator"]["output"]["fps"], 30);
    let w = s["warnings"].as_array().unwrap();
    assert!(w.iter().any(|w| w["code"] == "fps60_not_validated"), "{s}");
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
    user(&r.daemon, "run_fps_selftest", test_params())
        .await
        .unwrap();
    let s = user(&r.daemon, "start_recording", p).await.unwrap();
    assert_eq!(s["indicator"]["output"]["fps"], 60);
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
}

/// Nobody can forge a pass through `set_settings`.
#[tokio::test]
async fn the_result_key_cannot_be_written_by_clients() {
    let r = rig();
    let forged =
        json!({ "values": { "recording.fps60_selftest": { "version": 1, "results": {} } } });
    let e = user(&r.daemon, "set_settings", forged.clone())
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
    assert!(e.to_string().contains("self-test"), "{e}");
    let e = r
        .daemon
        .execute_method(agent(), 0, "set_settings", &forged)
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
    assert!(r
        .settings
        .all()
        .unwrap()
        .get("recording.fps60_selftest")
        .is_none());
}

/// Bad input is refused with a field name before anything else.
#[tokio::test]
async fn selftest_validates_input_before_anything() {
    let r = rig();
    for (p, field) in [
        (json!({ "size": "8k" }), "size"),
        (json!({ "codec": "vp9" }), "codec"),
        (json!({ "fps": 60 }), "fps"),
        (json!({ "target": { "display_id": "one" } }), "target"),
    ] {
        let e = user(&r.daemon, "run_fps_selftest", p.clone())
            .await
            .unwrap_err();
        assert_eq!(code(&e), ErrorCode::InvalidInput, "{p}");
        assert!(e.to_string().starts_with(field), "{p}: {e}");
    }
    let e = user(
        &r.daemon,
        "run_fps_selftest",
        json!({ "target": { "display_id": 99 } }),
    )
    .await
    .unwrap_err();
    assert_eq!(code(&e), ErrorCode::NotFound);
    assert_eq!(r.sources.builds(), 0);
}

/// With no `target` the primary display is tested.
#[tokio::test]
async fn the_default_target_is_the_primary_display() {
    let r = rig();
    let v = user(&r.daemon, "run_fps_selftest", json!({}))
        .await
        .unwrap();
    assert_eq!(v["state"], "passed");
    assert_eq!(v["display_id"], 1);
}

/// A missing Screen Recording permission fails before any dialog, file or event.
#[tokio::test]
async fn a_missing_permission_fails_before_anything() {
    let r = rig_full(
        FakeSources::new().with_missing_permission(Permission::Screen),
        Some(Decision::Approve),
        120,
        SharedSettings::default(),
    );
    let e = r
        .daemon
        .execute_method(agent(), 1, "run_fps_selftest", &test_params())
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::PermissionMissing);
    assert!(r.confirmer.as_ref().unwrap().shown().is_empty());
    assert_eq!(r.sources.builds(), 0);
}

/// NFR-11: it captures real pixels, so an agent needs the user's confirmation, and the dialog
/// says what happens. In process the user runs it directly with no dialog.
#[tokio::test]
async fn an_agent_needs_confirmation_and_the_user_does_not() {
    let r = rig_full(
        FakeSources::new(),
        Some(Decision::Approve),
        120,
        SharedSettings::default(),
    );
    user(&r.daemon, "run_fps_selftest", test_params())
        .await
        .unwrap();
    assert!(
        r.confirmer.as_ref().unwrap().shown().is_empty(),
        "no dialog for the user"
    );
    let s = r.daemon.new_session();
    r.clock.advance(time::Duration::minutes(1));
    let v = r
        .daemon
        .execute_method(agent(), s, "run_fps_selftest", &test_params())
        .await
        .unwrap();
    assert_eq!(v["state"], "passed");
    let shown = r.confirmer.as_ref().unwrap().shown();
    assert_eq!(shown.len(), 1);
    let q = &shown[0];
    assert_eq!(q.client, "claude-desktop");
    assert_eq!(q.tool, "run_fps_selftest");
    // P3: the typed plan, including how long the test runs.
    assert_eq!(q.kind, "run_fps_selftest");
    assert_eq!(q.params["fps"], 60);
    assert_eq!(q.params["duration_s"], 0.12);
    assert_eq!(q.params["target"]["type"], "display");
    assert!(q.summary.contains("60 fps self-test"), "{}", q.summary);
    assert!(q.summary.contains("Nothing is saved"), "{}", q.summary);
    let d = q.details.as_object().unwrap();
    for k in [
        "target",
        "output",
        "save_to",
        "microphone",
        "camera",
        "purpose",
    ] {
        assert!(d.contains_key(k), "{k}");
    }
    assert_eq!(d["microphone"], "Off");
    assert_eq!(d["camera"], "Off");
    assert!(d["output"].as_str().unwrap().contains("60 fps"));
    // The indicator facts of an agent-started test name the client.
    let log = r.daemon.activity();
    assert_eq!(log.len(), 1);
    assert_eq!(log[0].tool, "run_fps_selftest");
}

/// A declined dialog records nothing and stores nothing.
#[tokio::test]
async fn a_declined_selftest_does_nothing() {
    let r = rig_full(
        FakeSources::new(),
        Some(Decision::Decline),
        120,
        SharedSettings::default(),
    );
    let e = r
        .daemon
        .execute_method(agent(), 1, "run_fps_selftest", &test_params())
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::UserDeclined);
    assert_eq!(r.sources.builds(), 0);
    assert!(r
        .settings
        .all()
        .unwrap()
        .get("recording.fps60_selftest")
        .is_none());
    assert!(leftovers(r.st_dir.path()).is_empty());
}

/// Without a recording engine the action is `not_implemented` (the shared code, exit 1).
#[tokio::test]
async fn without_an_engine_the_selftest_is_not_implemented() {
    let (b, _dir) = builder(desktop());
    let d = b.build();
    let e = user(&d, "run_fps_selftest", test_params())
        .await
        .unwrap_err();
    assert!(matches!(e, DaemonError::NotImplemented(_)), "{e:?}");
    assert_eq!(e.code(), Some(ErrorCode::NotImplemented));
    assert_eq!(e.rpc_code(), -32099);
    assert_eq!(e.exit_code(), 1);
}

/// A recording running makes the test `busy`, before any dialog; the recording is untouched.
#[tokio::test]
async fn the_selftest_is_busy_while_a_recording_runs() {
    let r = rig_full(
        FakeSources::new(),
        Some(Decision::Approve),
        120,
        SharedSettings::default(),
    );
    let s = user(&r.daemon, "start_recording", start_params(30))
        .await
        .unwrap();
    let builds = r.sources.builds();
    for who in [Actor::User, agent()] {
        let e = r
            .daemon
            .execute_method(who, 3, "run_fps_selftest", &test_params())
            .await
            .unwrap_err();
        assert_eq!(code(&e), ErrorCode::Busy);
    }
    assert!(
        r.confirmer.as_ref().unwrap().shown().is_empty(),
        "no dialog when busy"
    );
    assert_eq!(r.sources.builds(), builds);
    let st = user(&r.daemon, "get_recording_status", json!({}))
        .await
        .unwrap();
    assert_eq!(st["recording_id"], s["recording_id"]);
    assert_eq!(st["state"], "recording");
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
}

/// While the test runs, a recording (or a second test) is `busy`; the status says a self-test
/// runs; `stop_recording` cancels it, deletes its output and stores nothing.
#[tokio::test]
async fn busy_during_the_selftest_and_cancel_deletes_the_output() {
    let r = rig_full(FakeSources::new(), None, 5_000, SharedSettings::default());
    let mut rx = r.daemon.subscribe();
    let d = r.daemon.clone();
    let run = tokio::spawn(async move { user(&d, "run_fps_selftest", test_params()).await });
    until(|| r.daemon.selftest_active().then_some(())).await;
    // The status reports it, with the indicator facts.
    let status = user(&r.daemon, "get_recording_status", json!({}))
        .await
        .unwrap();
    assert_eq!(status["state"], "selftest");
    assert_eq!(status["recording_id"], Value::Null);
    assert_eq!(status["selftest"]["indicator"]["output"]["fps"], 60);
    assert!(!r.daemon.recording_active());
    // The temp output exists while it runs.
    until(|| (!leftovers(r.st_dir.path()).is_empty()).then_some(())).await;

    let e = user(&r.daemon, "start_recording", start_params(30))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::Busy);
    assert!(e.to_string().contains("self-test"), "{e}");
    let e = user(&r.daemon, "run_fps_selftest", test_params())
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::Busy);

    // Cancel.
    let c = user(&r.daemon, "stop_recording", json!({})).await.unwrap();
    assert_eq!(c["state"], "cancelled");
    assert_eq!(c["cancelled"], true);
    let e = run.await.unwrap().unwrap_err();
    match &e {
        DaemonError::Declined { reason, .. } => assert_eq!(*reason, "selftest_cancelled"),
        other => panic!("{other:?}"),
    }
    assert!(!r.daemon.selftest_active());
    assert!(leftovers(r.st_dir.path()).is_empty(), "output deleted");
    assert!(bundles(&r).is_empty());
    assert!(r
        .settings
        .all()
        .unwrap()
        .get("recording.fps60_selftest")
        .is_none());
    let ev = drain(&mut rx);
    let states: Vec<&str> = ev
        .iter()
        .filter(|e| e.name == "recording.selftest")
        .map(|e| e.data["state"].as_str().unwrap())
        .collect();
    assert_eq!(states.last(), Some(&"cancelled"), "{states:?}");
    // 60 fps is still refused, and a recording can start again.
    let e = user(&r.daemon, "start_recording", start_params(60))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
    user(&r.daemon, "start_recording", start_params(30))
        .await
        .unwrap();
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
}

/// If the caller goes away mid-test the test is cancelled and its output deleted.
#[tokio::test]
async fn a_vanished_caller_cancels_the_selftest() {
    let r = rig_full(FakeSources::new(), None, 5_000, SharedSettings::default());
    let d = r.daemon.clone();
    let run = tokio::spawn(async move { user(&d, "run_fps_selftest", test_params()).await });
    until(|| r.daemon.selftest_active().then_some(())).await;
    until(|| (!leftovers(r.st_dir.path()).is_empty()).then_some(())).await;
    run.abort();
    until(|| (!r.daemon.selftest_active()).then_some(())).await;
    until(|| leftovers(r.st_dir.path()).is_empty().then_some(())).await;
    assert!(r
        .settings
        .all()
        .unwrap()
        .get("recording.fps60_selftest")
        .is_none());
    user(&r.daemon, "start_recording", start_params(30))
        .await
        .unwrap();
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
}

/// Shutting the daemon down mid-test cancels it too.
#[tokio::test]
async fn shutdown_cancels_a_running_selftest() {
    let r = rig_full(FakeSources::new(), None, 5_000, SharedSettings::default());
    let d = r.daemon.clone();
    let run = tokio::spawn(async move { user(&d, "run_fps_selftest", test_params()).await });
    until(|| r.daemon.selftest_active().then_some(())).await;
    r.daemon.shutdown_recording().await;
    assert!(!r.daemon.selftest_active());
    assert!(run.await.unwrap().is_err());
    assert!(leftovers(r.st_dir.path()).is_empty());
}

/// A track that cannot start fails the test cleanly: no folder left, nothing stored, `busy`
/// is released.
#[tokio::test]
async fn a_failing_screen_start_leaves_nothing() {
    let r = rig_full(
        FakeSources::new().with_failing_start(cm_core::TrackKind::Screen),
        None,
        120,
        SharedSettings::default(),
    );
    let mut rx = r.daemon.subscribe();
    let e = user(&r.daemon, "run_fps_selftest", test_params())
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::DeviceBusy);
    assert!(leftovers(r.st_dir.path()).is_empty());
    assert!(!r.daemon.selftest_active());
    assert!(r
        .settings
        .all()
        .unwrap()
        .get("recording.fps60_selftest")
        .is_none());
    let ev = drain(&mut rx);
    let last = ev
        .iter()
        .rev()
        .find(|e| e.name == "recording.selftest")
        .unwrap();
    assert_eq!(last.data["state"], "error");
    assert_eq!(last.data["error"]["code"], "device_busy");
}

/// `actions.list` style listing: the action is Confirm tier, mutating, not internal.
#[test]
fn the_action_is_confirm_tier_and_public() {
    let s = cm_daemon::spec_for("run_fps_selftest").unwrap();
    assert_eq!(s.tier, cm_daemon::Tier::Confirm);
    assert!(s.mutating);
    assert!(!cm_daemon::is_internal("run_fps_selftest"));
}

/// R3-7: capture stalls fail the self-test even when every frame that arrived was on time (a
/// stalled capture used to leave `late_pct` at 0 and let the test pass).
#[tokio::test]
async fn r3_7_stalls_fail_the_selftest() {
    let r = rig_full(
        FakeSources::new().with_screen_stall_ms(900),
        None,
        120,
        SharedSettings::default(),
    );
    let v = user(&r.daemon, "run_fps_selftest", test_params())
        .await
        .unwrap();
    assert_eq!(v["state"], "failed", "{v}");
    assert_eq!(v["late_pct"], 0.0);
    assert_eq!(v["stall_ms"], 900);
    assert_eq!(v["stalls"], 1);
    // Under the limit it passes.
    let ok = rig_full(
        FakeSources::new().with_screen_stall_ms(400),
        None,
        120,
        SharedSettings::default(),
    );
    let v = user(&ok.daemon, "run_fps_selftest", test_params())
        .await
        .unwrap();
    assert_eq!(v["state"], "passed", "{v}");
}

/// R4-9: while the self-test runs, `get_recording_status` says so and carries the same indicator
/// facts as a recording; the `started` event (with the indicator) is the first event.
#[tokio::test]
async fn r4_9_the_selftest_shows_the_indicator_in_status_and_events() {
    let r = rig_full(FakeSources::new(), None, 3000, SharedSettings::default());
    let mut rx = r.daemon.subscribe();
    let d = r.daemon.clone();
    let run = tokio::spawn(async move { user(&d, "run_fps_selftest", test_params()).await });
    let st = {
        let mut got = None;
        for _ in 0..500 {
            let s = user(&r.daemon, "get_recording_status", json!({}))
                .await
                .unwrap();
            if s["state"] == "selftest" {
                got = Some(s);
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        got.expect("status says selftest")
    };
    assert_eq!(st["recording_id"], Value::Null);
    let ind = &st["selftest"]["indicator"];
    assert_eq!(ind["selftest"], true);
    assert_eq!(ind["area"]["kind"], "display");
    assert_eq!(ind["output"]["fps"], 60);
    assert_eq!(ind["started_by"]["tool"], "run_fps_selftest");
    let first = drain(&mut rx)
        .into_iter()
        .find(|e| e.name == "recording.selftest")
        .unwrap();
    assert_eq!(first.data["state"], "started");
    assert_eq!(first.data["indicator"]["selftest"], true);
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
    let _ = run.await;
}
