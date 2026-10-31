//! The recording actions over a real Unix socket with fake sources: confirmation (approve,
//! decline, timeout), per-client policy, idempotency, events, and the internal HUD actions as
//! an agent sees them (AGT-1, AGT-2, AGT-3, NFR-11, REC-1, REC-4, REC-5).

#![cfg(unix)]
#![allow(clippy::unwrap_used)]

mod common;

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use cm_capture::disk::FakeFreeSpace;
use cm_core::ManualHostClock;
use cm_daemon::testing::{FakeSources, FixedConfirmer, NeverAnswers};
use cm_daemon::{
    connect, ClientPolicy, ConfirmationRequest, Confirmer, Daemon, Decision, Endpoint,
    EngineOptions, IpcServer,
};
use cm_ipc::{ClientStream, RpcClient, RpcError};
use common::{builder, desktop};
use serde_json::{json, Value};
use tokio::task::JoinHandle;

struct Shared(Arc<FixedConfirmer>);

#[async_trait]
impl Confirmer for Shared {
    async fn confirm(&self, r: &ConfirmationRequest) -> Decision {
        self.0.confirm(r).await
    }
}

struct Rig {
    daemon: Daemon,
    ep: Endpoint,
    sources: FakeSources,
    confirmer: Arc<FixedConfirmer>,
    dir: tempfile::TempDir,
    _server: JoinHandle<()>,
}

fn options() -> EngineOptions {
    EngineOptions {
        tick_every: Duration::from_millis(20),
        countdown_step: Duration::from_millis(30),
        host: Arc::new(ManualHostClock::new(1_000_000_000)),
        free_space: Arc::new(FakeFreeSpace::new(500_000_000_000)),
        disk_check_every: Duration::ZERO,
        own_pid: None,
        ..EngineOptions::default()
    }
}

fn rig(decision: Decision, confirm_timeout: Option<Duration>) -> Rig {
    let (b, dir) = builder(desktop());
    let sources = FakeSources::new();
    let confirmer = Arc::new(FixedConfirmer::new(decision));
    let queue = cm_daemon::library_job_queue(dir.path()).unwrap();
    let mut b = b
        .jobs(queue)
        .recording_sources(sources.clone())
        .recording_options(options())
        .confirmer(Shared(confirmer.clone()));
    if let Some(t) = confirm_timeout {
        b = b.confirm_timeout(t);
    }
    finish(b.build(), sources, confirmer, dir)
}

fn finish(
    daemon: Daemon,
    sources: FakeSources,
    confirmer: Arc<FixedConfirmer>,
    dir: tempfile::TempDir,
) -> Rig {
    let ep = Endpoint::scratch(dir.path(), "d");
    let server = IpcServer::bind(&ep).unwrap();
    let d = daemon.clone();
    let task = tokio::spawn(async move {
        let _ = server.run(d).await;
    });
    Rig {
        daemon,
        ep,
        sources,
        confirmer,
        dir,
        _server: task,
    }
}

struct Sock {
    c: RpcClient<ClientStream>,
}

impl Sock {
    async fn new(ep: &Endpoint, name: &str) -> Sock {
        let mut c = RpcClient::new(connect(ep).await.unwrap());
        c.call("session.hello", json!({ "client": name }))
            .await
            .unwrap();
        Sock { c }
    }

    async fn call(&mut self, method: &str, params: Value) -> Result<Value, Value> {
        match self.c.call(method, params).await {
            Ok(v) => Ok(v),
            Err(RpcError::Rpc(e)) => Err(e),
            Err(e) => panic!("transport: {e}"),
        }
    }
}

fn code(e: &Value) -> &str {
    e["data"]["code"].as_str().unwrap()
}

fn recording_bundles(r: &Rig) -> usize {
    std::fs::read_dir(r.dir.path())
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().ends_with(".cmrec"))
        .count()
}

fn start_params() -> Value {
    json!({ "target": { "display_id": 1 }, "countdown_s": 0, "mic": true })
}

/// AGT-2.AC1 / NFR-11: nothing is recorded until the user answers; the approved call returns
/// the recording id; pause, resume and stop are Free calls from the same client.
#[tokio::test]
async fn agt2_approved_start_over_the_socket_then_free_controls() {
    let r = rig(Decision::Approve, None);
    let mut watcher = Sock::new(&r.ep, "watcher").await;
    watcher
        .call("events.subscribe", json!({ "topics": ["recording.*"] }))
        .await
        .unwrap();
    let mut c = Sock::new(&r.ep, "claude-desktop").await;
    let started = c.call("start_recording", start_params()).await.unwrap();
    assert!(started["recording_id"]
        .as_str()
        .unwrap()
        .starts_with("rec_"));
    assert_eq!(
        started["indicator"]["started_by"]["client"],
        "claude-desktop"
    );
    let shown = r.confirmer.shown();
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].client, "claude-desktop");
    assert!(shown[0].details["microphone"]
        .as_str()
        .unwrap()
        .starts_with("On: Fake Microphone"));

    let st = c.call("get_recording_status", json!({})).await.unwrap();
    assert_eq!(st["state"], "recording");
    assert_eq!(st["indicator"]["started_by"]["kind"], "agent");
    assert_eq!(
        c.call("pause_recording", json!({})).await.unwrap()["state"],
        "paused"
    );
    assert_eq!(
        c.call("resume_recording", json!({})).await.unwrap()["state"],
        "recording"
    );
    let stopped = c
        .call(
            "stop_recording",
            json!({ "recording_id": started["recording_id"] }),
        )
        .await
        .unwrap();
    assert_eq!(stopped["capture_id"], started["capture_id"]);
    assert_eq!(stopped["reason"], "agent");
    assert_eq!(r.confirmer.shown().len(), 1, "only the start asked");

    // The watcher saw the story, as notifications.
    let mut seen = Vec::new();
    while !seen.contains(&"recording.stopped".to_owned()) {
        let ev = watcher.c.next_event().await.unwrap();
        seen.push(ev["event"].as_str().unwrap().to_owned());
    }
    let started_at = seen.iter().position(|e| e == "recording.started").unwrap();
    let paused_at = seen.iter().position(|e| e == "recording.paused").unwrap();
    let resumed_at = seen.iter().position(|e| e == "recording.resumed").unwrap();
    assert!(started_at < paused_at && paused_at < resumed_at);
    assert!(seen.iter().all(|e| e.starts_with("recording.")));
}

/// AGT-2.AC1: declined: `user_declined` (exit code 3 for the CLI), no files, no hold.
#[tokio::test]
async fn agt2_declined_start_over_the_socket() {
    let r = rig(Decision::Decline, None);
    let mut c = Sock::new(&r.ep, "claude").await;
    let e = c.call("start_recording", start_params()).await.unwrap_err();
    assert_eq!(code(&e), "user_declined");
    assert_eq!(e["code"], -32002);
    assert_eq!(recording_bundles(&r), 0);
    assert_eq!(r.sources.builds(), 0);
    assert!(!r.daemon.is_job_type_held("export"));
    assert!(!r.daemon.recording_active());
    let st = c.call("get_recording_status", json!({})).await.unwrap();
    assert_eq!(st["state"], "idle");
    // The log has it.
    let log = r.daemon.activity();
    assert_eq!(log.len(), 1);
    assert_eq!(log[0].tool, "start_recording");
}

/// AGT-2.AC3: no answer: `confirmation_timeout`, the dialog closes, nothing was created.
#[tokio::test]
async fn agt2_ac3_unanswered_start_times_out() {
    let (b, dir) = builder(desktop());
    let sources = FakeSources::new();
    let d = b
        .recording_sources(sources.clone())
        .recording_options(options())
        .confirmer(NeverAnswers)
        .confirm_timeout(Duration::from_millis(150))
        .build();
    let r = finish(
        d,
        sources,
        Arc::new(FixedConfirmer::new(Decision::Decline)),
        dir,
    );
    let mut c = Sock::new(&r.ep, "claude").await;
    let t0 = std::time::Instant::now();
    let e = c.call("start_recording", start_params()).await.unwrap_err();
    assert_eq!(code(&e), "confirmation_timeout");
    assert!(t0.elapsed() >= Duration::from_millis(140));
    assert_eq!(recording_bundles(&r), 0);
    assert_eq!(r.sources.builds(), 0);
    assert!(!r.daemon.recording_active());
}

/// AGT-2.AC4: a read-only client may read (status, devices) and nothing else; no dialog.
#[tokio::test]
async fn agt2_ac4_read_only_client_over_the_socket() {
    let r = rig(Decision::Approve, None);
    r.daemon.policies().set("viewer", ClientPolicy::ReadOnly);
    let mut v = Sock::new(&r.ep, "viewer").await;
    for m in [
        "start_recording",
        "pause_recording",
        "resume_recording",
        "stop_recording",
    ] {
        let p = if m == "start_recording" {
            start_params()
        } else {
            json!({})
        };
        let e = v.call(m, p).await.unwrap_err();
        assert_eq!(code(&e), "user_declined", "{m}");
        assert_eq!(e["data"]["data"]["reason"], "client_read_only", "{m}");
    }
    assert!(r.confirmer.shown().is_empty());
    assert_eq!(
        v.call("get_recording_status", json!({})).await.unwrap()["state"],
        "idle"
    );
    assert!(v.call("list_devices", json!({})).await.unwrap()["mics"].is_array());
    assert!(!r.daemon.recording_active());
}

/// AGT-3.AC4 over the wire: the same request id returns the first result, one dialog, one
/// recording.
#[tokio::test]
async fn agt3_ac4_request_id_replay_over_the_socket() {
    let r = rig(Decision::Approve, None);
    let mut c = Sock::new(&r.ep, "claude").await;
    let mut p = start_params();
    p["request_id"] = json!("go-1");
    let a = c.call("start_recording", p.clone()).await.unwrap();
    let b = c.call("start_recording", p).await.unwrap();
    assert_eq!(a, b);
    assert_eq!(r.confirmer.shown().len(), 1);
    assert_eq!(r.sources.builds(), 1);
    // A different id while recording is busy.
    let mut p2 = start_params();
    p2["request_id"] = json!("go-2");
    let e = c.call("start_recording", p2).await.unwrap_err();
    assert_eq!(code(&e), "busy");
    assert_eq!(e["data"]["message"], "A recording is already running");
    c.call("stop_recording", json!({})).await.unwrap();
}

/// A start with a countdown is cancellable from another connection, and the starter's call
/// ends with a decline.
#[tokio::test]
async fn rec4_countdown_cancel_from_another_client() {
    let r = rig(Decision::Approve, None);
    let mut starter = Sock::new(&r.ep, "claude").await;
    let mut other = Sock::new(&r.ep, "cli").await;
    let mut p = start_params();
    p["countdown_s"] = json!(10);
    let start = tokio::spawn(async move { starter.call("start_recording", p).await });
    for _ in 0..200 {
        if other.call("get_recording_status", json!({})).await.unwrap()["state"] == "countdown" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let ans = other.call("stop_recording", json!({})).await.unwrap();
    assert_eq!(ans["cancelled"], true);
    let e = start.await.unwrap().unwrap_err();
    assert_eq!(code(&e), "user_declined");
    assert_eq!(recording_bundles(&r), 0);
}

/// REC-5: the HUD's Discard and Restart are internal actions. They are listed as internal, they
/// are Confirm tier for agents (a dialog shows; declining keeps the take), and approving
/// deletes the take.
#[tokio::test]
async fn rec5_discard_and_restart_are_gated_for_agents() {
    let r = rig(Decision::Decline, None);
    let mut c = Sock::new(&r.ep, "claude").await;
    let list = c.call("actions.list", json!({})).await.unwrap();
    let row = |n: &str| {
        list.as_array()
            .unwrap()
            .iter()
            .find(|a| a["name"] == n)
            .unwrap()
            .clone()
    };
    for n in ["discard_recording", "restart_recording"] {
        let a = row(n);
        assert_eq!(a["tier"], "confirm", "{n}");
        assert_eq!(a["internal"], true, "{n}");
        assert_eq!(a["available"], true, "{n}");
    }
    assert_eq!(row("skip_countdown")["internal"], true);
    for n in [
        "start_recording",
        "pause_recording",
        "resume_recording",
        "stop_recording",
        "get_recording_status",
        "list_devices",
    ] {
        assert_eq!(row(n)["internal"], false, "{n}");
        assert_eq!(row(n)["available"], true, "{n}");
    }
    assert_eq!(row("get_recording_status")["tier"], "free");

    // The user (in process) starts; the agent tries to discard and is declined.
    r.daemon
        .execute_method(cm_core::Actor::User, 0, "start_recording", &start_params())
        .await
        .unwrap();
    for m in ["discard_recording", "restart_recording"] {
        let e = c.call(m, json!({})).await.unwrap_err();
        assert_eq!(code(&e), "user_declined", "{m}");
    }
    assert!(r.daemon.recording_active());
    assert_eq!(recording_bundles(&r), 1);
    assert_eq!(r.confirmer.shown().len(), 2);

    // The user's own Discard (in process) needs no dialog.
    let d = r
        .daemon
        .execute_method(cm_core::Actor::User, 0, "discard_recording", &json!({}))
        .await
        .unwrap();
    assert_eq!(d["discarded"], true);
    assert_eq!(recording_bundles(&r), 0);
    assert_eq!(r.confirmer.shown().len(), 2, "no dialog for the user");
}

/// Without a `session.hello` the client is `unknown`; the recording is attributed honestly.
#[tokio::test]
async fn the_indicator_names_the_agent_that_started_it() {
    let r = rig(Decision::Approve, None);
    let mut c = Sock::new(&r.ep, "cursor-ide").await;
    let s = c.call("start_recording", start_params()).await.unwrap();
    assert_eq!(s["indicator"]["started_by"]["client"], "cursor-ide");
    assert_eq!(s["indicator"]["started_by"]["tool"], "start_recording");
    let list = r.daemon.activity();
    assert_eq!(list[0].client, "cursor-ide");
    c.call("stop_recording", json!({})).await.unwrap();
}

/// A confirmer that holds the dialog open until the test lets it go (the user thinking).
struct Gated {
    opened: tokio::sync::Notify,
    release: tokio::sync::Notify,
    shown: std::sync::Mutex<Vec<ConfirmationRequest>>,
}

impl Gated {
    fn new() -> Arc<Gated> {
        Arc::new(Gated {
            opened: tokio::sync::Notify::new(),
            release: tokio::sync::Notify::new(),
            shown: std::sync::Mutex::new(Vec::new()),
        })
    }
}

struct GateConfirmer(Arc<Gated>);

#[async_trait]
impl Confirmer for GateConfirmer {
    async fn confirm(&self, r: &ConfirmationRequest) -> Decision {
        self.0.shown.lock().unwrap().push(r.clone());
        self.0.opened.notify_one();
        self.0.release.notified().await;
        Decision::Approve
    }
}

fn gated_rig() -> (Rig, Arc<Gated>) {
    let (b, dir) = builder(desktop());
    let sources = FakeSources::new();
    let gate = Gated::new();
    let queue = cm_daemon::library_job_queue(dir.path()).unwrap();
    let d = b
        .jobs(queue)
        .recording_sources(sources.clone())
        .recording_options(options())
        .confirmer(GateConfirmer(gate.clone()))
        .build();
    (
        finish(
            d,
            sources,
            Arc::new(FixedConfirmer::new(Decision::Approve)),
            dir,
        ),
        gate,
    )
}

/// R2-1: bait and switch. Agent A asks to record with the cursor off and a 4 s countdown (from
/// the settings); while the dialog is open agent B flips the settings. The recording that runs
/// is the one the dialog described, however long the dialog stayed open.
#[tokio::test]
async fn r2_1_the_approved_plan_is_not_remade_from_changed_settings() {
    let (r, gate) = gated_rig();
    let mut a = Sock::new(&r.ep, "agent-a").await;
    let mut b = Sock::new(&r.ep, "agent-b").await;
    b.call(
        "set_settings",
        json!({ "values": { "recording.cursor": false, "recording.countdown_s": 4 } }),
    )
    .await
    .unwrap();
    let mut watcher = Sock::new(&r.ep, "watcher").await;
    watcher
        .call("events.subscribe", json!({ "topics": ["recording.*"] }))
        .await
        .unwrap();
    let call = tokio::spawn(async move {
        a.call("start_recording", json!({ "target": { "display_id": 1 } }))
            .await
    });
    gate.opened.notified().await;
    {
        let shown = gate.shown.lock().unwrap();
        assert_eq!(shown[0].details["cursor"], "Off");
        assert_eq!(shown[0].details["countdown"], "4 s");
    }
    b.call(
        "set_settings",
        json!({ "values": { "recording.cursor": true, "recording.countdown_s": 0 } }),
    )
    .await
    .unwrap();
    // Longer than the old "a plan older than 2 s is made again" window.
    tokio::time::sleep(Duration::from_millis(2200)).await;
    gate.release.notify_one();
    let started = call.await.unwrap().unwrap();
    assert_eq!(started["indicator"]["cursor"], false, "cursor stayed off");
    let mut saw_countdown = false;
    loop {
        let ev = watcher.c.next_event().await.unwrap();
        match ev["event"].as_str().unwrap() {
            "recording.countdown" => saw_countdown = true,
            "recording.started" => break,
            _ => {}
        }
    }
    assert!(saw_countdown, "the 4 s countdown the user saw still ran");
    r.daemon
        .execute_method(cm_core::Actor::User, 0, "stop_recording", &json!({}))
        .await
        .unwrap();
}

/// R2-2: the discard the dialog described is pinned to that recording. If the user stops it and
/// starts another while the dialog is open, approving deletes nothing.
#[tokio::test]
async fn r2_2_discard_is_pinned_to_the_recording_in_the_dialog() {
    let (r, gate) = gated_rig();
    let user = |m: &'static str, p: Value| {
        let d = r.daemon.clone();
        async move {
            d.execute_method(cm_core::Actor::User, 0, m, &p)
                .await
                .unwrap()
        }
    };
    let rec_a = user("start_recording", start_params()).await;
    let mut agent = Sock::new(&r.ep, "agent").await;
    let call = tokio::spawn(async move { agent.call("discard_recording", json!({})).await });
    gate.opened.notified().await;
    assert_eq!(
        gate.shown.lock().unwrap()[0].details["recording"],
        rec_a["recording_id"]
    );
    user("stop_recording", json!({})).await;
    let rec_b = user("start_recording", start_params()).await;
    assert_ne!(rec_a["recording_id"], rec_b["recording_id"]);
    gate.release.notify_one();
    let e = call.await.unwrap().unwrap_err();
    assert_eq!(code(&e), "not_found");
    assert!(r.daemon.recording_active(), "recording B was not deleted");
    assert_eq!(recording_bundles(&r), 2);
    user("stop_recording", json!({})).await;
}

/// R2-2: a restart is pinned the same way.
#[tokio::test]
async fn r2_2_restart_is_pinned_to_the_recording_in_the_dialog() {
    let (r, gate) = gated_rig();
    let user = |m: &'static str, p: Value| {
        let d = r.daemon.clone();
        async move {
            d.execute_method(cm_core::Actor::User, 0, m, &p)
                .await
                .unwrap()
        }
    };
    user("start_recording", start_params()).await;
    let mut agent = Sock::new(&r.ep, "agent").await;
    let call = tokio::spawn(async move { agent.call("restart_recording", json!({})).await });
    gate.opened.notified().await;
    user("stop_recording", json!({})).await;
    let rec_b = user("start_recording", start_params()).await;
    gate.release.notify_one();
    let e = call.await.unwrap().unwrap_err();
    assert_eq!(code(&e), "not_found");
    let st = user("get_recording_status", json!({})).await;
    assert_eq!(st["recording_id"], rec_b["recording_id"]);
    user("stop_recording", json!({})).await;
}

/// R2-10: an agent's title cannot disguise or reorder dialog rows.
#[tokio::test]
async fn r2_10_title_is_sanitized_and_bounded_in_the_dialog() {
    let r = rig(Decision::Approve, None);
    let mut c = Sock::new(&r.ep, "claude").await;
    let mut p = start_params();
    p["title"] = json!(format!(
        "Weekly\nCamera: Off\u{202E}\u{200B} {}",
        "x".repeat(500)
    ));
    c.call("start_recording", p).await.unwrap();
    let shown = r.confirmer.shown();
    let row = shown[0].details["save_to"].as_str().unwrap();
    assert!(!row.contains('\n') && !row.contains('\u{202E}') && !row.contains('\u{200B}'));
    assert!(row.chars().count() < 120 + 200, "bounded: {row}");
    assert!(row.contains('\u{2026}'), "shown truncated");
    // Rows come in the order they were built (R2-10).
    let keys: Vec<&str> = shown[0]
        .details
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(&keys[..3], ["target", "output", "screen"]);
    c.call("stop_recording", json!({})).await.unwrap();
}

/// R2-11: skip_countdown is the user's control; an agent over the socket is refused.
#[tokio::test]
async fn r2_11_an_agent_cannot_skip_the_countdown() {
    let r = rig(Decision::Approve, None);
    let mut c = Sock::new(&r.ep, "claude").await;
    let mut p = start_params();
    p["countdown_s"] = json!(10);
    let mut starter = Sock::new(&r.ep, "starter").await;
    let start = tokio::spawn(async move { starter.call("start_recording", p).await });
    for _ in 0..200 {
        if c.call("get_recording_status", json!({})).await.unwrap()["state"] == "countdown" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let e = c.call("skip_countdown", json!({})).await.unwrap_err();
    assert_eq!(code(&e), "user_declined");
    assert_eq!(e["data"]["data"]["reason"], "first_party_only");
    assert_eq!(
        c.call("get_recording_status", json!({})).await.unwrap()["state"],
        "countdown"
    );
    // The user (in process) still can.
    r.daemon
        .execute_method(cm_core::Actor::User, 0, "skip_countdown", &json!({}))
        .await
        .unwrap();
    start.await.unwrap().unwrap();
    c.call("stop_recording", json!({})).await.unwrap();
}
