//! End to end over JSON-RPC: capture (fake backend) -> annotate -> get_capture -> undo, plus
//! idempotency, events, policy, errors and the Unix socket transport.

#![allow(clippy::unwrap_used)]

mod common;

use cm_capture::fake::FakeBackend;
use cm_daemon::testing::RecordingClipboard;
use common::{builder, daemon, desktop, Client};
use serde_json::{json, Value};
use std::sync::Arc;

const OPS: &str = r##"[
  {"op":"arrow","from":[10,10],"to":[60,40],"color":"#FF3B3B","width":4},
  {"op":"rect","rect":[20,20,50,30],"color":"#2E8BFF","width":3},
  {"op":"step","at":[30,30],"n":1}
]"##;

fn ok(r: &Value) -> &Value {
    assert!(r.get("error").is_none(), "unexpected error: {r}");
    &r["result"]
}

fn err_code(r: &Value) -> &str {
    r["error"]["data"]["code"].as_str().unwrap()
}

#[tokio::test]
async fn agt_1_and_3_capture_annotate_get_undo_over_jsonrpc() {
    let (d, _dir) = daemon();
    let mut c = Client::connect(&d, "claude");
    c.hello("claude-desktop").await;
    let sub = c
        .call("events.subscribe", json!({"topics": ["capture.*"]}))
        .await;
    assert_eq!(ok(&sub)["subscribed"], true);

    // AGT-1.AC1 flavor: discovery works.
    let t = c.call("list_targets", json!({})).await;
    assert_eq!(ok(&t)["displays"].as_array().unwrap().len(), 2);
    assert_eq!(ok(&t)["windows"][0]["title"], "Editor");

    // CAP-1.AC1: `windows: false` lists the same displays without the window enumeration.
    let fast = c.call("list_targets", json!({"windows": false})).await;
    assert_eq!(ok(&fast)["displays"], ok(&t)["displays"]);
    assert_eq!(ok(&fast)["windows"], json!([]));
    let bad = c.call("list_targets", json!({"windows": 0})).await;
    assert_eq!(err_code(&bad), "invalid_input");

    // CAP-1.AC2: region rect is physical px relative to the display.
    let shot = c
        .call(
            "capture_screenshot",
            json!({"target": {"display_id": 1, "rect": [100, 50, 200, 120]}, "request_id": "cap-1"}),
        )
        .await;
    let shot = ok(&shot).clone();
    assert_eq!(
        (shot["width"].as_u64(), shot["height"].as_u64()),
        (Some(200), Some(120))
    );
    assert_eq!(shot["kind"], "region");
    assert_eq!(shot["image"]["mime_type"], "image/png");
    let id = shot["capture_id"].as_str().unwrap().to_owned();
    assert!(std::path::Path::new(shot["path"].as_str().unwrap()).is_file());

    // Idempotent repeat of the capture: same result, no second bundle (AGT-3.AC4).
    let again = c
        .call(
            "capture_screenshot",
            json!({"target": {"display_id": 1, "rect": [100, 50, 200, 120]}, "request_id": "cap-1"}),
        )
        .await;
    assert_eq!(ok(&again), &shot);
    let lib = c.call("list_library", json!({})).await;
    assert_eq!(ok(&lib)["captures"].as_array().unwrap().len(), 1);

    // annotate: three ops, one call.
    let ops: Value = serde_json::from_str(OPS).unwrap();
    let ann = c
        .call(
            "annotate",
            json!({"capture_id": id, "ops": ops, "request_id": "ann-1"}),
        )
        .await;
    assert_eq!(ok(&ann)["revision"], 1);
    assert_eq!(ok(&ann)["by"], "agent:claude-desktop");
    let ann_again = c
        .call(
            "annotate",
            json!({"capture_id": id, "ops": ops, "request_id": "ann-1"}),
        )
        .await;
    assert_eq!(ok(&ann_again), ok(&ann));

    let got = c.call("get_capture", json!({"capture_id": id})).await;
    let got = ok(&got);
    assert_eq!(
        got["ops"].as_array().unwrap().len(),
        3,
        "repeat must not add ops"
    );
    assert_eq!(got["revision"], 1);
    assert_eq!(got["revisions"][0]["by"], "agent:claude-desktop");
    assert_eq!(got["can_undo"], true);
    assert_ne!(
        got["image"], shot["image"],
        "the render differs from the original"
    );

    // AGT-3.AC1 / ANN-3: one undo step reverts the whole call.
    let undo = c.call("undo", json!({"capture_id": id})).await;
    assert_eq!(ok(&undo)["changed"], true);
    assert_eq!(ok(&undo)["revision"], 2);
    let got = c.call("get_capture", json!({"capture_id": id})).await;
    assert_eq!(ok(&got)["ops"].as_array().unwrap().len(), 0);
    assert_eq!(ok(&got)["image"], shot["image"]);
    let redo = c.call("redo", json!({"capture_id": id})).await;
    assert_eq!(ok(&redo)["changed"], true);
    let got = c.call("get_capture", json!({"capture_id": id})).await;
    assert_eq!(ok(&got)["ops"].as_array().unwrap().len(), 3);
    // Nothing left to redo is a no-op, not an error.
    let redo = c.call("redo", json!({"capture_id": id})).await;
    assert_eq!(ok(&redo)["changed"], false);

    // Events: capture.created, then capture.revised for annotate, undo, redo.
    let mut names = Vec::new();
    for _ in 0..4 {
        let e = c.next_event().await;
        names.push((e["event"].as_str().unwrap().to_owned(), e["data"].clone()));
    }
    assert_eq!(names[0].0, "capture.created");
    assert_eq!(names[0].1["capture_id"], id);
    assert_eq!(names[1].0, "capture.revised");
    assert_eq!(names[1].1["by"], "agent:claude-desktop");
    assert_eq!(names[1].1["revision"], 1);
    assert_eq!(names[2].1["revision"], 2);
    assert_eq!(names[3].1["revision"], 3);
}

#[tokio::test]
async fn agt_3_ac3_failed_annotate_applies_nothing() {
    let (d, _dir) = daemon();
    let mut c = Client::connect(&d, "a");
    c.hello("a").await;
    let shot = c
        .call("capture_screenshot", json!({"target": {"display_id": 2}}))
        .await;
    let id = ok(&shot)["capture_id"].as_str().unwrap().to_owned();
    let first = c
        .call(
            "annotate",
            json!({"capture_id": id, "ops": [{"id":"keep","op":"rect","rect":[1,1,5,5]}]}),
        )
        .await;
    assert_eq!(ok(&first)["revision"], 1);
    // Second op reuses an existing id: the whole call fails, nothing is applied.
    let bad = c
        .call(
            "annotate",
            json!({"capture_id": id, "ops": [
                {"id":"fresh","op":"rect","rect":[2,2,5,5]},
                {"id":"keep","op":"rect","rect":[3,3,5,5]}]}),
        )
        .await;
    assert_eq!(err_code(&bad), "invalid_input");
    assert_eq!(bad["error"]["code"], -32602);
    // Invalid payload in the second op is caught at parse time, also with nothing applied.
    let bad2 = c
        .call(
            "annotate",
            json!({"capture_id": id, "ops": [
                {"op":"rect","rect":[2,2,5,5]}, {"op":"zig"}]}),
        )
        .await;
    assert_eq!(err_code(&bad2), "invalid_input");
    assert!(bad2["error"]["message"]
        .as_str()
        .unwrap()
        .contains("ops[1]"));
    let got = c
        .call(
            "get_capture",
            json!({"capture_id": id, "include_image": false}),
        )
        .await;
    assert_eq!(ok(&got)["ops"].as_array().unwrap().len(), 1);
    assert_eq!(ok(&got)["revision"], 1);
    assert_eq!(ok(&got)["image"], Value::Null);
}

#[tokio::test]
async fn cap_1_and_4_errors_clipboard_and_delay() {
    // Missing OS permission maps to permission_missing with data.permission.
    let (b, _dir) = builder(desktop().without_permission());
    let d = b.build();
    let mut c = Client::connect(&d, "a");
    let r = c.call("list_targets", json!({})).await;
    assert_eq!(err_code(&r), "permission_missing");
    assert_eq!(r["error"]["code"], -32004);
    assert_eq!(r["error"]["data"]["data"]["permission"], "screen");

    // CAP-4: clipboard only when asked; a missing clipboard is a warning, not a failure.
    let clip = Arc::new(RecordingClipboard::default());
    let (b, _dir2) = builder(FakeBackend::new().with_display(1, 0, 0, 40, 30, 1.0));
    let d = b.clipboard(clip.clone()).build();
    let mut c = Client::connect(&d, "a");
    let r = c
        .call("capture_screenshot", json!({"target":{"display_id":1}}))
        .await;
    assert_eq!(ok(&r)["copied_to_clipboard"], false);
    assert!(clip.last_png().is_none());
    let r = c
        .call(
            "capture_screenshot",
            json!({"target":{"display_id":1},"copy_to_clipboard":true}),
        )
        .await;
    assert_eq!(ok(&r)["copied_to_clipboard"], true);
    assert!(clip.last_png().is_some());
    let (d2, _dir3) = daemon();
    let mut c2 = Client::connect(&d2, "a");
    let r = c2
        .call(
            "capture_screenshot",
            json!({"target":{"display_id":2},"copy_to_clipboard":true}),
        )
        .await;
    assert_eq!(ok(&r)["copied_to_clipboard"], false);
    assert_eq!(ok(&r)["warnings"].as_array().unwrap().len(), 1);

    // CAP-5: delay is 0..30.
    let r = c2
        .call(
            "capture_screenshot",
            json!({"target":{"display_id":2},"delay_s":31}),
        )
        .await;
    assert_eq!(err_code(&r), "invalid_input");
    // Unknown display.
    let r = c2
        .call("capture_screenshot", json!({"target":{"display_id":9}}))
        .await;
    assert_eq!(err_code(&r), "not_found");
    assert_eq!(r["error"]["code"], -32001);
}

#[tokio::test]
async fn lib_1_list_library_query_and_paging() {
    let (d, _dir) = daemon();
    let mut c = Client::connect(&d, "a");
    for title in ["Login page", "Settings", "Billing"] {
        let r = c
            .call(
                "capture_screenshot",
                json!({"target":{"display_id":2},"title":title}),
            )
            .await;
        ok(&r);
    }
    let page1 = c.call("list_library", json!({"limit": 2})).await;
    let p1 = ok(&page1);
    assert_eq!(p1["captures"].as_array().unwrap().len(), 2);
    let cursor = p1["next_cursor"].as_str().unwrap();
    let page2 = c
        .call("list_library", json!({"limit": 2, "cursor": cursor}))
        .await;
    assert_eq!(ok(&page2)["captures"].as_array().unwrap().len(), 1);
    assert_eq!(ok(&page2)["next_cursor"], Value::Null);
    let q = c.call("list_library", json!({"query": "billing"})).await;
    assert_eq!(ok(&q)["captures"].as_array().unwrap().len(), 1);
    assert_eq!(ok(&q)["captures"][0]["title"], "Billing");
    let none = c.call("list_library", json!({"kind": "recording"})).await;
    assert_eq!(ok(&none)["captures"].as_array().unwrap().len(), 0);
    let bad = c.call("list_library", json!({"cursor": "x"})).await;
    assert_eq!(err_code(&bad), "invalid_input");
}

#[tokio::test]
async fn settings_roundtrip_and_never_tier_keys() {
    let (d, _dir) = daemon();
    let mut c = Client::connect(&d, "a");
    let r = c
        .call(
            "set_settings",
            json!({"values": {"recording.fps": 60, "recording.codec": "hevc"}}),
        )
        .await;
    assert_eq!(ok(&r)["settings"]["recording.fps"], 60);
    let r = c
        .call("get_settings", json!({"keys": ["recording.codec"]}))
        .await;
    assert_eq!(ok(&r)["settings"], json!({"recording.codec": "hevc"}));
    let r = c.call("get_settings", json!({})).await;
    assert_eq!(ok(&r)["settings"].as_object().unwrap().len(), 2);
    let r = c.call("get_settings", json!({"keys": ["nope.x"]})).await;
    assert_eq!(err_code(&r), "not_found");
    // Tokens and keys are never exposed or writable.
    let r = c
        .call("set_settings", json!({"values": {"youtube.api_key": "k"}}))
        .await;
    assert_eq!(err_code(&r), "invalid_input");
    let r = c
        .call("get_settings", json!({"keys": ["auth.token"]}))
        .await;
    assert_eq!(err_code(&r), "invalid_input");
}

#[tokio::test]
async fn jsonrpc_envelope_errors() {
    let (d, _dir) = daemon();
    let mut c = Client::connect(&d, "a");
    c.send_raw("{not json").await;
    let r = c.read_response(&Value::Null).await;
    assert_eq!(r["error"]["code"], -32700);
    c.send_raw("[1,2]").await;
    let r = c.read_response(&Value::Null).await;
    assert_eq!(r["error"]["code"], -32600);
    c.send_raw(r#"{"jsonrpc":"1.0","id":5,"method":"list_targets"}"#)
        .await;
    let r = c.read_response(&json!(5)).await;
    assert_eq!(r["error"]["code"], -32600);
    let r = c.call("no_such_method", json!({})).await;
    assert_eq!(r["error"]["code"], -32601);
    let r = c.call("annotate", json!([])).await;
    assert_eq!(r["error"]["code"], -32602);
    // String ids are echoed.
    c.send_raw(r#"{"jsonrpc":"2.0","id":"abc","method":"get_settings"}"#)
        .await;
    let r = c.read_response(&json!("abc")).await;
    assert!(r.get("result").is_some());
    // A notification (no id) produces no response; the next call still works.
    c.send_raw(r#"{"jsonrpc":"2.0","method":"get_settings"}"#)
        .await;
    let r = c.call("get_settings", json!({})).await;
    assert!(r.get("result").is_some());
}

#[tokio::test]
async fn agt_2_ac5_never_tier_is_not_exposed() {
    let (d, _dir) = daemon();
    let mut c = Client::connect(&d, "a");
    for name in cm_daemon::NEVER_TIER {
        let r = c.call(name, json!({})).await;
        assert_eq!(r["error"]["code"], -32601, "{name}");
    }
    let list = c.call("actions.list", json!({})).await;
    let names: Vec<_> = ok(&list)
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["name"].as_str().unwrap())
        .collect();
    for n in cm_daemon::NEVER_TIER {
        assert!(!names.contains(n));
    }
    assert!(ok(&list)
        .as_array()
        .unwrap()
        .iter()
        .all(|a| a["tier"] != "never"));
    // Confirm-tier actions are listed but unavailable without an engine.
    let sr = ok(&list)
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["name"] == "start_recording")
        .unwrap();
    assert_eq!(sr["available"], false);
}

#[tokio::test]
async fn agt_1_ac5_ten_concurrent_calls_from_two_clients() {
    let (d, _dir) = daemon();
    let mut a = Client::connect(&d, "a");
    let mut b = Client::connect(&d, "b");
    a.hello("alpha").await;
    b.hello("beta").await;
    let mut ids_a = Vec::new();
    let mut ids_b = Vec::new();
    for i in 0..5 {
        ids_a.push(
            a.send(
                "capture_screenshot",
                json!({"target":{"display_id":2},"title":format!("a{i}")}),
            )
            .await,
        );
        ids_b.push(b.send("list_targets", json!({})).await);
    }
    for id in ids_a {
        let r = a.read_response(&json!(id)).await;
        ok(&r);
    }
    for id in ids_b {
        let r = b.read_response(&json!(id)).await;
        ok(&r);
    }
    let lib = a.call("list_library", json!({})).await;
    assert_eq!(ok(&lib)["captures"].as_array().unwrap().len(), 5);
}

#[tokio::test]
async fn events_subscribe_filters_topics_and_bridges_jobs() {
    use async_trait::async_trait;
    use cm_daemon::{JobEvent, JobEventKind, JobEvents};
    struct Feed(tokio::sync::mpsc::Receiver<JobEvent>);
    #[async_trait]
    impl JobEvents for Feed {
        async fn next_event(&mut self) -> Option<JobEvent> {
            self.0.recv().await
        }
    }
    let (d, _dir) = daemon();
    let mut c = Client::connect(&d, "a");
    let r = c
        .call("events.subscribe", json!({"topics": ["job.*"]}))
        .await;
    ok(&r);
    let (tx, rx) = tokio::sync::mpsc::channel(8);
    let h = d.bridge_job_events(Feed(rx));
    // A capture event must be filtered out; the job events pass.
    let shot = c
        .call("capture_screenshot", json!({"target":{"display_id":2}}))
        .await;
    ok(&shot);
    tx.send(JobEvent {
        kind: JobEventKind::Progress,
        job_id: "job_1".into(),
        data: json!({"progress": 0.5}),
    })
    .await
    .unwrap();
    tx.send(JobEvent {
        kind: JobEventKind::Done,
        job_id: "job_1".into(),
        data: json!({"path": "/x.mp4"}),
    })
    .await
    .unwrap();
    let e1 = c.next_event().await;
    assert_eq!(e1["event"], "job.progress");
    assert_eq!(e1["data"], json!({"job_id":"job_1","progress":0.5}));
    let e2 = c.next_event().await;
    assert_eq!(e2["event"], "job.done");
    drop(tx);
    h.await.unwrap();
    let r = c.call("events.unsubscribe", json!({})).await;
    assert_eq!(ok(&r)["was_subscribed"], true);
}

#[cfg(unix)]
#[tokio::test]
async fn unix_socket_transport_serves_and_cleans_up() {
    use cm_daemon::{connect, Endpoint, IpcServer};
    use std::os::unix::fs::PermissionsExt;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let (d, dir) = daemon();
    let sock = dir.path().join("d.sock");
    let ep = Endpoint::Path(sock.clone());
    // R2-7: a regular file at the path is never deleted.
    std::fs::write(&sock, b"precious").unwrap();
    assert_eq!(
        IpcServer::bind(&ep).err().map(|e| e.kind()),
        Some(std::io::ErrorKind::InvalidInput)
    );
    assert_eq!(std::fs::read(&sock).unwrap(), b"precious");
    std::fs::remove_file(&sock).unwrap();
    // A stale socket file from a crashed daemon is replaced.
    drop(std::os::unix::net::UnixListener::bind(&sock).unwrap());
    assert!(sock.exists());
    let server = IpcServer::bind(&ep).unwrap();
    assert_eq!(
        std::fs::metadata(&sock).unwrap().permissions().mode() & 0o777,
        0o600
    );
    // A second daemon on a live socket is refused.
    assert_eq!(
        IpcServer::bind(&ep).err().map(|e| e.kind()),
        Some(std::io::ErrorKind::AddrInUse)
    );
    let task = tokio::spawn(server.run(d.clone()));

    let stream = connect(&ep).await.unwrap();
    let (r, mut w) = tokio::io::split(stream);
    w.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"list_targets\"}\n")
        .await
        .unwrap();
    let mut lines = BufReader::new(r).lines();
    let line = lines.next_line().await.unwrap().unwrap();
    let v: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(v["id"], 7);
    assert_eq!(v["result"]["displays"].as_array().unwrap().len(), 2);

    task.abort();
    let _ = task.await;
    assert!(!sock.exists(), "socket file removed when the server stops");
}
