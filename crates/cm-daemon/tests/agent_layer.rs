//! The agent layer's guard rails: settings policy, per-client policy, resource limits,
//! clipboard of annotated captures, cancellation and recovery announcements. Each test names
//! the review finding it covers (R1-*, R2-*, R5-*).

#![allow(clippy::unwrap_used)]

mod common;

use async_trait::async_trait;
use base64::Engine as _;
use cm_capture::fake::FakeBackend;
use cm_core::{Actor, CmError};
use cm_daemon::testing::{CountingExternal, FakeSources, RecordingClipboard};
use cm_daemon::{ClientPolicy, Daemon, DaemonError};
use cm_jobs::{JobContext, JobHandler, JobQueue, NewJob, JOB_EXPORT};
use common::{builder, daemon, desktop, Client};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Notify;

fn ok(r: &Value) -> &Value {
    assert!(r.get("error").is_none(), "unexpected error: {r}");
    &r["result"]
}

fn err_code(r: &Value) -> &str {
    r["error"]["data"]["code"]
        .as_str()
        .unwrap_or_else(|| panic!("no error code in {r}"))
}

async fn until<T>(mut f: impl AsyncFnMut() -> Option<T>) -> T {
    for _ in 0..500 {
        if let Some(v) = f().await {
            return v;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("condition not reached in 5 s");
}

const OPS: &str = r##"[{"op":"rect","rect":[20,20,50,30],"color":"#2E8BFF","width":3}]"##;
const BLUR: &str = r##"[{"op":"pixelate","rect":[0,0,40,40]}]"##;

// ---- R2-3 / R2-4: settings ----------------------------------------------------------------

#[tokio::test]
async fn r2_3_agents_cannot_write_app_only_settings_but_the_app_can() {
    let (d, _dir) = daemon();
    let mut c = Client::connect(&d, "agent");
    c.hello("rogue").await;
    for key in [
        "library.path",
        "privacy.excluded_apps",
        "shortcuts.capture_region",
        "permissions.onboarded",
        "storage.pause_uploads_while_recording",
        "export.auto_on_stop",
        "recording.hud_positions",
        "agents.client_policies",
    ] {
        let r = c.call("set_settings", json!({"values": {key: "x"}})).await;
        assert_eq!(err_code(&r), "user_declined", "{key}: {r}");
        assert_eq!(
            r["error"]["data"]["data"]["reason"], "app_only_setting",
            "{key}"
        );
    }
    // The failed calls changed nothing.
    let all = c.call("get_settings", json!({})).await;
    assert_eq!(ok(&all)["settings"], json!({}));
    // Ordinary recording settings stay writable for agents, atomically with the check.
    let r = c
        .call("set_settings", json!({"values": {"recording.fps": 30}}))
        .await;
    assert_eq!(ok(&r)["settings"]["recording.fps"], 30);
    let mixed = c
        .call(
            "set_settings",
            json!({"values": {"recording.fps": 24, "library.path": "/tmp/evil"}}),
        )
        .await;
    assert_eq!(err_code(&mixed), "user_declined");
    let now = c
        .call("get_settings", json!({"keys": ["recording.fps"]}))
        .await;
    assert_eq!(ok(&now)["settings"]["recording.fps"], 30, "all or nothing");
    // The in-process GUI (Actor::User) can write them.
    let r = d
        .execute_method(
            Actor::User,
            0,
            "set_settings",
            &json!({"values": {"library.path": "/Volumes/Big/Caps", "shortcuts.capture_region": "alt+1"}}),
        )
        .await
        .unwrap();
    assert_eq!(r["settings"]["library.path"], "/Volumes/Big/Caps");
}

#[tokio::test]
async fn r2_3_value_limits_over_rpc() {
    let (d, _dir) = daemon();
    let mut c = Client::connect(&d, "agent");
    let big = "x".repeat(40_000);
    let r = c
        .call("set_settings", json!({"values": {"editor.notes": big}}))
        .await;
    assert_eq!(err_code(&r), "invalid_input");
    let mut deep = json!(1);
    for _ in 0..20 {
        deep = json!([deep]);
    }
    let r = c
        .call("set_settings", json!({"values": {"editor.deep": deep}}))
        .await;
    assert_eq!(err_code(&r), "invalid_input");
    let long = format!("a.{}", "b".repeat(200));
    let r = c.call("set_settings", json!({"values": {long: 1}})).await;
    assert_eq!(err_code(&r), "invalid_input");
}

#[tokio::test]
async fn r2_4_secret_looking_keys_are_refused_anywhere_in_the_name() {
    let (d, _dir) = daemon();
    let mut c = Client::connect(&d, "agent");
    for key in [
        "auth.access_token",
        "youtube.refresh_token",
        "mcp.bearer_token",
        "x.client_secret",
        "db.password_hash",
        "youtube.api_key_id",
    ] {
        let r = c
            .call("set_settings", json!({"values": {key: "s3cret"}}))
            .await;
        assert_eq!(err_code(&r), "invalid_input", "{key}");
        let r = c.call("get_settings", json!({"keys": [key]})).await;
        assert_eq!(err_code(&r), "invalid_input", "{key}");
    }
    let all = c.call("get_settings", json!({})).await;
    assert_eq!(ok(&all)["settings"], json!({}));
}

// ---- R2-5: client names and policy ----------------------------------------------------------

#[tokio::test]
async fn r2_5_blocked_client_gets_nothing_not_even_events_or_the_action_list() {
    let (d, _dir) = daemon();
    d.policies().set("evil", ClientPolicy::Blocked);
    let mut c = Client::connect(&d, "x");
    // `session.hello` with a blocked name: refused, and the connection keeps that name.
    let r = c.call("session.hello", json!({"client": "evil"})).await;
    assert_eq!(err_code(&r), "user_declined");
    assert_eq!(r["error"]["data"]["data"]["reason"], "client_blocked");
    // Renaming itself to something allowed does not help.
    let r = c.call("session.hello", json!({"client": "friendly"})).await;
    assert_eq!(err_code(&r), "invalid_input", "{r}");
    for (m, p) in [
        ("events.subscribe", json!({})),
        ("actions.list", json!({})),
        ("list_targets", json!({})),
        ("get_settings", json!({})),
    ] {
        let r = c.call(m, p).await;
        assert_eq!(err_code(&r), "user_declined", "{m}: {r}");
        assert_eq!(
            r["error"]["data"]["data"]["reason"], "client_blocked",
            "{m}"
        );
    }
}

#[tokio::test]
async fn default_deny_blocks_unknown_clients_but_not_named_allowed_ones() {
    let (d, _dir) = daemon();
    d.policies().set("trusted", ClientPolicy::Allowed);
    d.policies().set_default(ClientPolicy::Blocked);
    let mut c = Client::connect(&d, "x");
    let r = c.call("session.hello", json!({"client": "stranger"})).await;
    assert_eq!(err_code(&r), "user_declined");
    assert_eq!(r["error"]["data"]["data"]["reason"], "default_deny");
    let r = c.call("list_targets", json!({})).await;
    assert_eq!(r["error"]["data"]["data"]["reason"], "default_deny", "{r}");
    let mut t = Client::connect(&d, "y");
    t.hello("trusted").await;
    let r = t.call("get_settings", json!({})).await;
    assert!(r.get("error").is_none(), "{r}");
    // Back to allowed.
    d.policies().set_default(ClientPolicy::Allowed);
    let mut u = Client::connect(&d, "z");
    u.hello("stranger").await;
}

#[tokio::test]
async fn r2_5_a_connections_name_is_immutable_once_set() {
    let (d, _dir) = daemon();
    let mut c = Client::connect(&d, "x");
    c.hello("first").await;
    c.hello("first").await; // repeating the same name is fine
    let r = c.call("session.hello", json!({"client": "second"})).await;
    assert_eq!(err_code(&r), "invalid_input");
    assert!(r["error"]["message"].as_str().unwrap().contains("first"));
    // A new connection can choose another name.
    let mut c2 = Client::connect(&d, "y");
    c2.hello("second").await;
}

// ---- R2-6: resource limits ------------------------------------------------------------------

/// R2-6 / R1-11: a connection runs at most 64 requests at once (the 65th is `busy`), and
/// closing the connection cancels delayed captures still waiting.
#[tokio::test]
async fn r2_6_inflight_cap_and_r1_11_disconnect_cancels_delayed_captures() {
    let (d, _dir) = daemon();
    let mut c = Client::connect(&d, "x");
    c.hello("slowpoke").await;
    let mut ids = Vec::new();
    for _ in 0..cm_daemon::rpc::MAX_INFLIGHT {
        ids.push(
            c.send(
                "capture_screenshot",
                json!({"target": {"display_id": 1}, "delay_s": 20}),
            )
            .await,
        );
    }
    let extra = c.call("get_settings", json!({})).await;
    assert_eq!(err_code(&extra), "busy", "{extra}");
    drop(c);
    // The delayed captures are cancelled, not left sleeping for 20 s.
    until(async || (d.cancel_all_delayed_captures() == 0).then_some(())).await;
    assert!(ids.len() == cm_daemon::rpc::MAX_INFLIGHT);
    // Nothing was stored.
    let lib = d
        .execute_method(Actor::User, 0, "list_library", &json!({}))
        .await
        .unwrap();
    assert_eq!(lib["captures"], json!([]));
}

/// R1-11 / CAP-5.AC2: a delayed capture can be cancelled by `request_id`.
#[tokio::test]
async fn r1_11_cancel_delayed_capture_by_request_id() {
    let (d, _dir) = daemon();
    let mut c = Client::connect(&d, "x");
    let id = c
        .send(
            "capture_screenshot",
            json!({"target": {"display_id": 1}, "delay_s": 20, "request_id": "delayed-1"}),
        )
        .await;
    until(async || d.cancel_delayed_capture("delayed-1").then_some(())).await;
    let r = tokio::time::timeout(Duration::from_secs(5), c.read_response(&json!(id)))
        .await
        .expect("the cancelled capture answers promptly");
    assert_eq!(err_code(&r), "invalid_input");
    assert!(r["error"]["message"]
        .as_str()
        .unwrap()
        .contains("cancelled"));
    assert!(!d.cancel_delayed_capture("delayed-1"), "already gone");
}

/// R2-6: `inline` frames are limited for agents; `file` and the app are not.
#[tokio::test]
async fn r2_6_inline_frames_are_limited_for_agents() {
    let (b, _dir) = builder(FakeBackend::new().with_display(1, 0, 0, 2500, 1500, 1.0));
    let d = b.build();
    let mut c = Client::connect(&d, "x");
    let r = c
        .call(
            "freeze_display",
            json!({"display_id": 1, "transport": "inline"}),
        )
        .await;
    assert_eq!(
        err_code(&r),
        "invalid_input",
        "{}",
        r.to_string().chars().take(300).collect::<String>()
    );
    let r = c
        .call(
            "freeze_display",
            json!({"display_id": 1, "transport": "file"}),
        )
        .await;
    assert_eq!(ok(&r)["transport"], "file");
    let r = d
        .execute_method(
            Actor::User,
            0,
            "freeze_display",
            &json!({"display_id": 1, "transport": "inline"}),
        )
        .await
        .unwrap();
    assert_eq!(r["transport"], "inline");
}

// ---- R2-13: envelope ------------------------------------------------------------------------

#[tokio::test]
async fn r2_13_id_types_and_batches() {
    let (d, _dir) = daemon();
    let mut c = Client::connect(&d, "x");
    for bad in [r#"{"x":1}"#, "[1]", "true"] {
        c.send_raw(&format!(
            r#"{{"jsonrpc":"2.0","id":{bad},"method":"get_settings"}}"#
        ))
        .await;
        let r = c.read_response(&Value::Null).await;
        assert_eq!(r["error"]["code"], -32600, "id {bad}: {r}");
    }
    // `id: null` is accepted and answered with a null id.
    c.send_raw(r#"{"jsonrpc":"2.0","id":null,"method":"get_settings"}"#)
        .await;
    let r = c.read_response(&Value::Null).await;
    assert!(r.get("result").is_some(), "{r}");
    c.send_raw(r#"[{"jsonrpc":"2.0","id":1,"method":"get_settings"}]"#)
        .await;
    let r = c.read_response(&Value::Null).await;
    assert_eq!(r["error"]["code"], -32600);
    assert!(r["error"]["message"]
        .as_str()
        .unwrap()
        .contains("one request per line"));
    // Fractional and big numeric ids still work.
    c.send_raw(r#"{"jsonrpc":"2.0","id":1.5,"method":"get_settings"}"#)
        .await;
    let r = c.read_response(&json!(1.5)).await;
    assert!(r.get("result").is_some());
}

/// `rpc.cancel` aborts one in-flight request and leaves the others.
#[tokio::test]
async fn r2_14_rpc_cancel_stops_one_request() {
    let (d, _dir) = daemon();
    let mut c = Client::connect(&d, "x");
    let slow = c
        .send(
            "capture_screenshot",
            json!({"target": {"display_id": 1}, "delay_s": 20, "request_id": "slow"}),
        )
        .await;
    // Let the request reach its handler (it is registered with the connection at once).
    tokio::time::sleep(Duration::from_millis(100)).await;
    let r = c.call("rpc.cancel", json!({"id": slow})).await;
    assert_eq!(ok(&r)["cancelled"], true);
    let again = c.call("rpc.cancel", json!({"id": slow})).await;
    assert_eq!(ok(&again)["cancelled"], false);
    // The cancelled request left its delay: the capture is cancelled, the connection works.
    until(async || (d.cancel_all_delayed_captures() == 0).then_some(())).await;
    let r = c.call("get_settings", json!({})).await;
    assert!(r.get("result").is_some());
}

// ---- R1-1: clipboard gets the rendered image ------------------------------------------------

fn decode_png(v: &Value) -> Vec<u8> {
    base64::engine::general_purpose::STANDARD
        .decode(v["image"]["data"].as_str().unwrap())
        .unwrap()
}

#[tokio::test]
async fn r1_1_annotate_copies_the_rendered_image_not_the_original() {
    let clip = Arc::new(RecordingClipboard::default());
    let (b, _dir) = builder(desktop());
    let d = b.clipboard(clip.clone()).build();
    let mut c = Client::connect(&d, "x");
    let shot = c
        .call("capture_screenshot", json!({"target": {"display_id": 1}}))
        .await;
    let shot = ok(&shot).clone();
    let id = shot["capture_id"].as_str().unwrap().to_owned();
    let original = decode_png(&shot);
    assert!(clip.last_png().is_none());

    let ops: Value = serde_json::from_str(OPS).unwrap();
    let a = c
        .call(
            "annotate",
            json!({"capture_id": id, "ops": ops, "copy_to_clipboard": true}),
        )
        .await;
    let a = ok(&a);
    assert_eq!(a["copied_to_clipboard"], true);
    let rendered = decode_png(a);
    assert_ne!(rendered, original, "the annotation changed the pixels");
    assert_eq!(
        clip.last_png().unwrap(),
        rendered,
        "the clipboard holds the rendered PNG"
    );
    assert_ne!(clip.last_png().unwrap(), original);

    // A redaction is applied before the copy too.
    let blur: Value = serde_json::from_str(BLUR).unwrap();
    let a2 = c
        .call(
            "annotate",
            json!({"capture_id": id, "ops": blur, "copy_to_clipboard": true}),
        )
        .await;
    assert_eq!(clip.last_png().unwrap(), decode_png(ok(&a2)));

    // Without the flag nothing is copied.
    let before = clip.last_png();
    let a3 = c
        .call("annotate", json!({"capture_id": id, "ops": ops}))
        .await;
    assert_eq!(ok(&a3)["copied_to_clipboard"], false);
    assert_eq!(clip.last_png(), before);

    // No clipboard in this daemon: a warning, the annotation still succeeds.
    let (d2, _dir2) = daemon();
    let mut c2 = Client::connect(&d2, "x");
    let s2 = c2
        .call("capture_screenshot", json!({"target": {"display_id": 1}}))
        .await;
    let id2 = ok(&s2)["capture_id"].as_str().unwrap().to_owned();
    let a = c2
        .call(
            "annotate",
            json!({"capture_id": id2, "ops": ops, "copy_to_clipboard": true}),
        )
        .await;
    assert_eq!(ok(&a)["copied_to_clipboard"], false);
    assert_eq!(ok(&a)["warnings"].as_array().unwrap().len(), 1);
}

// ---- R1-15: paging --------------------------------------------------------------------------

#[tokio::test]
async fn r1_15_cursor_overflow_status_and_search_hits() {
    let (d, _dir) = daemon();
    let mut c = Client::connect(&d, "x");
    c.call(
        "capture_screenshot",
        json!({"target": {"display_id": 1}, "title": "Quarterly report"}),
    )
    .await;
    for cursor in ["18446744073709551615", "9223372036854775807", "4294967296"] {
        let r = c.call("list_library", json!({"cursor": cursor})).await;
        assert_eq!(err_code(&r), "invalid_input", "{cursor}");
        let r = c
            .call("list_library", json!({"cursor": cursor, "query": "report"}))
            .await;
        assert_eq!(err_code(&r), "invalid_input", "{cursor} with a query");
    }
    // `status` is a real filter now (LIB-1); a value outside the four is `invalid_input`.
    let r = c.call("list_library", json!({"status": "uploaded"})).await;
    assert!(ok(&r)["captures"].as_array().unwrap().is_empty());
    let r = c
        .call("list_library", json!({"status": "not_uploaded"}))
        .await;
    assert_eq!(ok(&r)["captures"].as_array().unwrap().len(), 1);
    let r = c.call("list_library", json!({"status": "sideways"})).await;
    assert_eq!(err_code(&r), "invalid_input");
    // Search hits carry the transcript match slot (null for a title-only match).
    let r = c.call("list_library", json!({"query": "report"})).await;
    let first = &ok(&r)["captures"][0];
    assert_eq!(first["title"], "Quarterly report");
    assert!(first.get("transcript").is_some(), "{first}");
    // Plain listing has no such field.
    let r = c.call("list_library", json!({})).await;
    assert!(ok(&r)["captures"][0].get("transcript").is_none());
}

// ---- R1-17: per-capture serialization -------------------------------------------------------

#[tokio::test]
async fn r1_17_concurrent_annotates_on_one_capture_are_serialized() {
    let (d, _dir) = daemon();
    let mut c = Client::connect(&d, "x");
    let shot = c
        .call("capture_screenshot", json!({"target": {"display_id": 1}}))
        .await;
    let id = ok(&shot)["capture_id"].as_str().unwrap().to_owned();
    let mut tasks = Vec::new();
    for i in 0..8 {
        let d = d.clone();
        let id = id.clone();
        tasks.push(tokio::spawn(async move {
            let op = json!([{"op": "rect", "rect": [i * 5, 10, 20, 20], "color": "#2E8BFF", "width": 2}]);
            d.execute_method(Actor::User, 0, "annotate", &json!({"capture_id": id, "ops": op}))
                .await
                .unwrap()
        }));
    }
    let mut revs: Vec<u64> = Vec::new();
    for t in tasks {
        revs.push(t.await.unwrap()["revision"].as_u64().unwrap());
    }
    revs.sort_unstable();
    revs.dedup();
    assert_eq!(
        revs.len(),
        8,
        "every annotate got its own revision: {revs:?}"
    );
    let g = c
        .call(
            "get_capture",
            json!({"capture_id": id, "include_image": false}),
        )
        .await;
    let g = ok(&g);
    assert_eq!(g["ops"].as_array().unwrap().len(), 8, "no op was lost");
    assert_eq!(g["revision"].as_u64().unwrap(), *revs.last().unwrap());
    assert_eq!(
        g["revisions"].as_array().unwrap().len(),
        8,
        "one revision per annotate"
    );
}

// ---- R2-8: headless confirmer ---------------------------------------------------------------

#[tokio::test]
async fn r2_8_headless_confirm_says_no_confirmation_ui_is_attached() {
    let (b, _dir) = builder(desktop());
    let d = b
        .external_actions(Arc::new(CountingExternal::default()))
        .build();
    let mut c = Client::connect(&d, "x");
    let r = c
        .call("delete_capture", json!({"capture_id": "shot_x"}))
        .await;
    assert_eq!(err_code(&r), "user_declined", "{r}");
    assert_eq!(r["error"]["data"]["data"]["reason"], "no_confirmer");
    assert!(r["error"]["message"]
        .as_str()
        .unwrap()
        .contains("No confirmation UI"));
}

// ---- jobs: export dedupe, recovery announcement, cancel reply, bounded shutdown ---------------

struct Quick;

#[async_trait]
impl JobHandler for Quick {
    async fn run(&self, _ctx: JobContext) -> Result<Value, CmError> {
        Ok(json!({"path": "/stub/out.mp4"}))
    }
}

/// Never finishes and ignores interrupts.
struct Stuck(Arc<Notify>);

#[async_trait]
impl JobHandler for Stuck {
    async fn run(&self, _ctx: JobContext) -> Result<Value, CmError> {
        self.0.notify_one();
        std::future::pending().await
    }
}

fn recording_daemon(with_export_handler: bool) -> (Daemon, JobQueue, tempfile::TempDir) {
    let (b, dir) = builder(desktop());
    let queue = cm_daemon::library_job_queue(dir.path()).unwrap();
    let d = b
        .jobs(queue.clone())
        .recording_sources(FakeSources::new())
        .build();
    if with_export_handler {
        // Held so nothing runs: the test inspects the queue.
        d.register_job_handler(JOB_EXPORT, Arc::new(Quick)).unwrap();
        d.hold_job_type(JOB_EXPORT);
    }
    d.start_jobs().unwrap();
    (d, queue, dir)
}

async fn record_one(d: &Daemon) -> String {
    let s = d
        .execute_method(
            Actor::User,
            0,
            "start_recording",
            &json!({"target": {"display_id": 1}, "countdown_s": 0}),
        )
        .await
        .unwrap();
    let id = s["capture_id"].as_str().unwrap().to_owned();
    d.execute_method(Actor::User, 0, "stop_recording", &json!({}))
        .await
        .unwrap();
    id
}

/// R2-14: two `export_video` calls racing for one capture and preset create one job.
#[tokio::test]
async fn r2_14_export_enqueue_is_atomic() {
    let (d, queue, _dir) = recording_daemon(true);
    let id = record_one(&d).await;
    // The auto-export may already have queued one; the racing manual calls must reuse it.
    let mut tasks = Vec::new();
    for i in 0..12 {
        let d = d.clone();
        let id = id.clone();
        tasks.push(tokio::spawn(async move {
            d.execute_method(
                Actor::Agent(format!("a{i}")),
                0,
                "export_video",
                &json!({"capture_id": id}),
            )
            .await
            .unwrap()
        }));
    }
    for t in tasks {
        t.await.unwrap();
    }
    let jobs = queue
        .list(&cm_jobs::JobFilter {
            job_type: Some(JOB_EXPORT.to_owned()),
            states: None,
            limit: None,
        })
        .unwrap();
    let for_capture = jobs
        .iter()
        .filter(|j| j.inputs["capture_id"] == id.as_str() && j.inputs["preset"] == "quick")
        .count();
    assert_eq!(for_capture, 1, "{jobs:?}");
}

/// R5-5 / EXP-4.AC3: after a restart the surviving export job is announced again.
#[tokio::test]
async fn r5_5_recovered_export_jobs_are_republished_as_export_queued() {
    let dir = tempfile::tempdir().unwrap();
    // First run: an export job is running when the app dies.
    let job_id = {
        let q = cm_daemon::library_job_queue(dir.path()).unwrap();
        let j = q
            .enqueue(NewJob::new(
                JOB_EXPORT,
                json!({"capture_id": "rec_gone", "preset": "quick", "by": "user", "auto": true}),
            ))
            .unwrap();
        q.store().try_start(&j.id, 1).unwrap();
        j.id
    };
    // Second run: a fresh daemon on the same database.
    let (b, _libdir) = builder(desktop());
    let queue = cm_daemon::library_job_queue(dir.path()).unwrap();
    let d = b.jobs(queue).build();
    d.register_job_handler(JOB_EXPORT, Arc::new(Quick)).unwrap();
    d.hold_job_type(JOB_EXPORT);
    let mut rx = d.subscribe();
    d.start_jobs().unwrap();
    let mut seen = None;
    while let Ok(ev) = rx.try_recv() {
        if ev.name == "export.queued" {
            seen = Some(ev);
        }
    }
    let ev = seen.expect("export.queued was republished");
    assert_eq!(ev.data["job_id"], job_id.as_str());
    assert_eq!(ev.data["capture_id"], "rec_gone");
    assert_eq!(ev.data["recovered"], true);
    assert_eq!(ev.data["was_interrupted"], true);
    assert_eq!(ev.data["auto"], true);
    assert_eq!(ev.data["preset"], "quick");
}

/// R2-12: `cancel_job` on a running job says the stop was only requested.
#[tokio::test]
async fn r2_12_cancel_reply_is_honest_about_a_running_job() {
    let (b, dir) = builder(desktop());
    let d = b
        .jobs(cm_daemon::library_job_queue(dir.path()).unwrap())
        .build();
    let entered = Arc::new(Notify::new());
    d.register_job_handler("stuck", Arc::new(Stuck(entered.clone())))
        .unwrap();
    d.start_jobs().unwrap();
    let id = d
        .job_queue()
        .unwrap()
        .enqueue(NewJob::new("stuck", json!({})))
        .unwrap()
        .id;
    entered.notified().await;
    let r = d
        .execute_method(Actor::User, 0, "cancel_job", &json!({"job_id": id}))
        .await
        .unwrap();
    assert_eq!(r["cancelling"], true);
    assert!(r["note"].as_str().unwrap().contains("done"));
}

/// R2-8: shutting the job worker down is bounded even if a handler never stops.
#[tokio::test]
async fn r2_8_shutdown_jobs_is_bounded() {
    let (b, dir) = builder(desktop());
    let d = b
        .jobs(cm_daemon::library_job_queue(dir.path()).unwrap())
        .build();
    let entered = Arc::new(Notify::new());
    d.register_job_handler("stuck", Arc::new(Stuck(entered.clone())))
        .unwrap();
    d.start_jobs().unwrap();
    d.job_queue()
        .unwrap()
        .enqueue(NewJob::new("stuck", json!({})))
        .unwrap();
    entered.notified().await;
    tokio::time::timeout(
        Duration::from_secs(5),
        d.shutdown_jobs_within(Duration::from_millis(200)),
    )
    .await
    .expect("shutdown gave up on the stuck handler");
}

#[allow(dead_code)]
fn unused(_: DaemonError) {}

/// R2-8: `cm-daemon` on an endpoint that something already serves says so and exits 0, before
/// it opens the library or the job queue.
#[cfg(unix)]
#[tokio::test]
async fn r2_8_cm_daemon_exits_clearly_when_an_app_already_serves_the_socket() {
    let (d, dir) = daemon();
    let sock = dir.path().join("app.sock");
    let ep = cm_daemon::Endpoint::Path(sock.clone());
    let server = cm_daemon::IpcServer::bind(&ep).unwrap();
    let task = tokio::spawn(server.run(d));
    let lib = dir.path().join("untouched-lib");
    let out = tokio::task::spawn_blocking(move || {
        std::process::Command::new(env!("CARGO_BIN_EXE_cm-daemon"))
            .arg("--library")
            .arg(&lib)
            .arg("--socket")
            .arg(&sock)
            .env("CAPTUREMANN_CONFIG_DIR", lib.with_extension("cfg"))
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    assert!(out.status.success(), "{:?}", out);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("already serving"), "{err}");
    assert!(
        !dir.path().join("untouched-lib").exists(),
        "the library was not opened"
    );
    task.abort();
}
