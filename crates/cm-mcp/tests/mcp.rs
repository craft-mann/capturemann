//! MCP server tests: an in-process daemon (fake capture backend) behind an rmcp client.
//! Cites AGT-1 (MCP over one action bus), AGT-2 (tiers, policy) and AGT-3 (undo, idempotency).

#![allow(clippy::unwrap_used)]

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use cm_capture::fake::FakeBackend;
use cm_capture::Capturer;
use cm_daemon::testing::{CountingExternal, FixedConfirmer};
use cm_daemon::{ClientPolicy, Daemon, DaemonBuilder, Decision, Endpoint, IpcServer};
use cm_library::Library;
use cm_mcp::conn::{BoxedRead, BoxedWrite, Connector};
use cm_mcp::McpServer;
use rmcp::model::{
    CallToolRequestParams, CallToolResult, ClientCapabilities, ClientConfig, Implementation,
};
use rmcp::service::{RoleClient, RunningService, ServiceExt};
use serde_json::{json, Value};
use tokio::task::JoinHandle;

type Client = RunningService<RoleClient, ClientConfig>;

fn desktop() -> FakeBackend {
    FakeBackend::new()
        .with_display(1, 0, 0, 400, 300, 2.0)
        .with_display(2, 400, 0, 200, 100, 1.0)
        .with_window(10, "Editor", "Code", 100, 1, (20, 10, 100, 80), false)
}

fn builder() -> (DaemonBuilder, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let lib = Library::open_or_create(dir.path()).unwrap();
    (
        Daemon::builder(Capturer::new(desktop()).with_own_pid(None), lib),
        dir,
    )
}

/// A connector that serves each new connection from `daemon` over an in-memory pipe and
/// remembers the server tasks (so a test can kill the "daemon").
fn duplex_connector(daemon: Daemon) -> (Connector, Arc<Mutex<Vec<JoinHandle<()>>>>) {
    let tasks: Arc<Mutex<Vec<JoinHandle<()>>>> = Arc::default();
    let t = tasks.clone();
    let connector: Connector = Arc::new(move || {
        let daemon = daemon.clone();
        let t = t.clone();
        Box::pin(async move {
            // client <-> proxy <-> daemon: aborting the proxy task is the "crash" (the client
            // sees EOF even though the daemon's own per-connection tasks outlive an abort).
            let (c, mut proxy_c) = tokio::io::duplex(1 << 22);
            let (mut proxy_s, s) = tokio::io::duplex(1 << 22);
            let (sr, sw) = tokio::io::split(s);
            tokio::spawn(cm_daemon::serve_connection(daemon, sr, sw));
            let h = tokio::spawn(async move {
                let _ = tokio::io::copy_bidirectional(&mut proxy_c, &mut proxy_s).await;
            });
            t.lock().unwrap().push(h);
            let (cr, cw) = tokio::io::split(c);
            Ok((Box::new(cr) as BoxedRead, Box::new(cw) as BoxedWrite))
        })
    });
    (connector, tasks)
}

/// Starts an MCP server on `connector` and an rmcp client named `name` connected to it.
async fn mcp_client(connector: Connector, name: &str) -> Client {
    let (c, s) = tokio::io::duplex(1 << 22);
    tokio::spawn(async move {
        if let Ok(running) = McpServer::new(connector).serve(s).await {
            let _ = running.waiting().await;
        }
    });
    ClientConfig::new(
        ClientCapabilities::default(),
        Implementation::new(name, "1.0"),
    )
    .serve(c)
    .await
    .unwrap()
}

async fn call(c: &Client, tool: &str, args: Value) -> CallToolResult {
    let mut p = CallToolRequestParams::new(tool.to_owned());
    if let Value::Object(m) = args {
        p = p.with_arguments(m);
    }
    c.call_tool(p).await.unwrap()
}

fn ok(r: &CallToolResult) -> &Value {
    assert_ne!(r.is_error, Some(true), "unexpected tool error: {r:?}");
    r.structured_content.as_ref().unwrap()
}

fn err_code(r: &CallToolResult) -> &str {
    assert_eq!(r.is_error, Some(true), "expected a tool error: {r:?}");
    r.structured_content.as_ref().unwrap()["code"]
        .as_str()
        .unwrap()
}

#[tokio::test]
async fn agt_1_ac1_lists_all_tools_with_schemas_and_tiers() {
    let (b, _dir) = builder();
    let d = b.build();
    let (conn, _) = duplex_connector(d);
    let c = mcp_client(conn, "claude-desktop").await;
    let tools = c.list_all_tools().await.unwrap();

    let names: BTreeSet<String> = tools.iter().map(|t| t.name.to_string()).collect();
    // Parity with the daemon: every daemon action has a tool and every tool a daemon action
    // with the same tier (AGT-1: one definition per action; AC4 flavor).
    // Internal actions (the overlay's freeze plumbing) are deliberately not tools: the
    // explicit list is `cm_daemon::INTERNAL_ACTIONS`.
    let actions: BTreeSet<String> = cm_daemon::ACTIONS
        .iter()
        .filter(|s| !cm_daemon::is_internal(s.name))
        .map(|s| s.name.to_owned())
        .collect();
    for n in cm_daemon::INTERNAL_ACTIONS {
        assert!(
            !names.contains(*n),
            "internal action `{n}` must not be a tool"
        );
    }
    assert_eq!(names, actions);
    for t in &tools {
        let spec = cm_daemon::spec_for(&t.name).unwrap();
        let desc = t.description.as_deref().unwrap();
        let tier = if spec.tier == cm_daemon::Tier::Confirm {
            "Tier: Confirm"
        } else {
            "Tier: Free"
        };
        assert!(desc.contains(tier), "{}: {desc}", t.name);
        assert_eq!(t.input_schema["type"], "object", "{}", t.name);
        assert_eq!(
            t.output_schema.as_ref().unwrap()["type"],
            "object",
            "{}",
            t.name
        );
        assert_eq!(
            t.annotations.as_ref().unwrap().read_only_hint,
            Some(!spec.mutating),
            "{}",
            t.name
        );
    }
    // Never tier (AGT-2.AC5): nothing about tokens or keys is exposed.
    for n in cm_daemon::NEVER_TIER {
        assert!(!names.contains(*n));
    }
    // Annotate carries the op schema of ANN-1/ANN-2.
    let annotate = tools.iter().find(|t| t.name == "annotate").unwrap();
    let ops = &annotate.input_schema["properties"]["ops"]["items"]["oneOf"];
    assert_eq!(ops.as_array().unwrap().len(), 10);
    c.cancel().await.unwrap();
}

#[tokio::test]
async fn agt_1_ac1_list_targets_over_a_real_unix_socket() {
    let (b, dir) = builder();
    let d = b.build();
    let sock = dir.path().join("daemon.sock");
    let endpoint = Endpoint::Path(sock);
    let server = IpcServer::bind(&endpoint).unwrap();
    tokio::spawn(server.run(d));
    let c = mcp_client(cm_mcp::endpoint_connector(endpoint), "cursor").await;
    let r = call(&c, "list_targets", json!({})).await;
    let v = ok(&r);
    assert_eq!(v["displays"].as_array().unwrap().len(), 2);
    assert_eq!(v["windows"][0]["title"], "Editor");
    // The text block carries the same JSON for clients that ignore structuredContent.
    let text = r.content[0].as_text().unwrap();
    assert_eq!(serde_json::from_str::<Value>(&text.text).unwrap(), *v);
    // CAP-1.AC1: `windows: false` is the displays-only fast path; same displays, no windows.
    let fast = call(&c, "list_targets", json!({"windows": false})).await;
    assert_eq!(ok(&fast)["displays"], v["displays"]);
    assert_eq!(ok(&fast)["windows"], json!([]));
}

#[tokio::test]
async fn agt_1_and_3_capture_annotate_get_undo_with_images_and_identity() {
    let (b, _dir) = builder();
    let (conn, _) = duplex_connector(b.build());
    let c = mcp_client(conn, "claude-desktop").await;

    let shot = call(
        &c,
        "capture_screenshot",
        json!({"target": {"display_id": 1, "rect": [100, 50, 200, 120]}, "request_id": "cap-1"}),
    )
    .await;
    let v = ok(&shot).clone();
    assert_eq!(
        (v["width"].as_u64(), v["height"].as_u64()),
        (Some(200), Some(120))
    );
    // Image as MCP image content, not base64 inside the JSON.
    assert_eq!(v["image"]["content_index"], 1);
    assert!(v["image"].get("data").is_none());
    let img = shot.content[1].as_image().unwrap();
    assert_eq!(img.mime_type, "image/png");
    assert!(!img.data.is_empty());
    let id = v["capture_id"].as_str().unwrap().to_owned();

    // AGT-3.AC4: the same request_id returns the first result and acts once.
    let again = call(
        &c,
        "capture_screenshot",
        json!({"target": {"display_id": 1, "rect": [100, 50, 200, 120]}, "request_id": "cap-1"}),
    )
    .await;
    assert_eq!(ok(&again), &v);
    let lib = call(&c, "list_library", json!({})).await;
    assert_eq!(ok(&lib)["captures"].as_array().unwrap().len(), 1);

    let ops = json!([
        {"op":"arrow","from":[10,10],"to":[60,40],"color":"#FF3B3B","width":4},
        {"op":"rect","rect":[20,20,50,30],"color":"#2E8BFF","width":3},
        {"op":"step","at":[30,30],"n":1}
    ]);
    let ann = call(&c, "annotate", json!({"capture_id": id, "ops": ops})).await;
    assert_eq!(ok(&ann)["revision"], 1);
    // The MCP client's name is the actor (session.hello): `agent:<name>`.
    assert_eq!(ok(&ann)["by"], "agent:claude-desktop");
    assert_ne!(ann.content[1].as_image().unwrap().data, img.data);

    let got = call(&c, "get_capture", json!({"capture_id": id})).await;
    assert_eq!(ok(&got)["ops"].as_array().unwrap().len(), 3);
    assert_eq!(ok(&got)["revisions"][0]["by"], "agent:claude-desktop");
    let no_image = call(
        &c,
        "get_capture",
        json!({"capture_id": id, "include_image": false}),
    )
    .await;
    assert_eq!(no_image.content.len(), 1);
    assert!(ok(&no_image)["image"].is_null());

    // AGT-3.AC1: one undo step reverts the whole three-op call.
    let undo = call(&c, "undo", json!({"capture_id": id})).await;
    assert_eq!(ok(&undo)["changed"], true);
    assert_ne!(
        undo.content[1].as_image().unwrap().data,
        ann.content[1].as_image().unwrap().data
    );
    let got = call(&c, "get_capture", json!({"capture_id": id})).await;
    assert_eq!(ok(&got)["ops"].as_array().unwrap().len(), 0);
    assert_eq!(
        got.content[1].as_image().unwrap().data,
        img.data,
        "back to the original"
    );
    let redo = call(&c, "redo", json!({"capture_id": id})).await;
    assert_eq!(ok(&redo)["changed"], true);
    let redo = call(&c, "redo", json!({"capture_id": id})).await;
    assert_eq!(ok(&redo)["changed"], false);
    assert_eq!(redo.content.len(), 1, "no image when nothing changed");
}

/// ANN-2.AC4 over MCP: read-only, nothing applied, previews masked, schema advertised.
#[tokio::test]
async fn ann_2_ac4_suggest_redactions_proposes_and_applies_nothing() {
    use cm_daemon::testing::{ScriptedRecognizer, TextLine};
    let (b, _dir) = builder();
    let rec = ScriptedRecognizer::new(vec![
        TextLine::laid_out("Owner: jane.doe@example.com", 50.0, 60.0, 10.0, 20.0),
        TextLine::laid_out("Billing: ap@vendor.example.org", 50.0, 140.0, 10.0, 20.0),
    ]);
    let (conn, _) = duplex_connector(b.text_recognizer(rec).build());
    let c = mcp_client(conn, "claude-desktop").await;
    let tools = c.list_all_tools().await.unwrap();
    let t = tools
        .iter()
        .find(|t| t.name == "suggest_redactions")
        .unwrap();
    assert_eq!(t.annotations.as_ref().unwrap().read_only_hint, Some(true));
    assert!(t.description.as_deref().unwrap().contains("Tier: Free"));

    let shot = call(
        &c,
        "capture_screenshot",
        json!({"target": {"display_id": 1}}),
    )
    .await;
    let id = ok(&shot)["capture_id"].as_str().unwrap().to_owned();
    let s = call(&c, "suggest_redactions", json!({"capture_id": id})).await;
    let v = ok(&s);
    assert_eq!(v["rects"].as_array().unwrap().len(), 2, "{v}");
    assert_eq!(v["applied"], false);
    assert!(!v.to_string().contains("jane.doe"));
    let got = call(
        &c,
        "get_capture",
        json!({"capture_id": id, "include_image": false}),
    )
    .await;
    assert_eq!(ok(&got)["ops"].as_array().unwrap().len(), 0);
    assert_eq!(ok(&got)["revision"], 0);
    let bad = call(
        &c,
        "suggest_redactions",
        json!({"capture_id": id, "kinds": ["face"]}),
    )
    .await;
    assert_eq!(err_code(&bad), "invalid_input");
    c.cancel().await.unwrap();
}

#[tokio::test]
async fn settings_round_trip_and_secret_keys_are_refused() {
    let (b, _dir) = builder();
    let (conn, _) = duplex_connector(b.build());
    let c = mcp_client(conn, "claude-code").await;
    let set = call(&c, "set_settings", json!({"values": {"recording.fps": 60}})).await;
    assert_eq!(ok(&set)["settings"]["recording.fps"], 60);
    let got = call(&c, "get_settings", json!({"keys": ["recording.fps"]})).await;
    assert_eq!(ok(&got)["settings"]["recording.fps"], 60);
    // Never tier: tokens and keys are not readable or writable (AGT-2.AC5).
    let bad = call(&c, "get_settings", json!({"keys": ["youtube.api_key"]})).await;
    assert_eq!(err_code(&bad), "invalid_input");
}

#[tokio::test]
async fn daemon_errors_keep_their_shared_code() {
    let (b, _dir) = builder();
    let (conn, _) = duplex_connector(b.build());
    let c = mcp_client(conn, "claude-desktop").await;

    // invalid_input names the op index (ANN-1 edge case).
    let r = call(
        &c,
        "capture_screenshot",
        json!({"target": {"display_id": 1}}),
    )
    .await;
    let id = ok(&r)["capture_id"].as_str().unwrap().to_owned();
    let bad = call(
        &c,
        "annotate",
        json!({"capture_id": id, "ops": [{"op": "zig"}]}),
    )
    .await;
    assert_eq!(err_code(&bad), "invalid_input");
    assert!(bad.structured_content.as_ref().unwrap()["message"]
        .as_str()
        .unwrap()
        .contains("ops[0]"));
    let text = bad.content[0].as_text().unwrap();
    assert!(text.text.starts_with("invalid_input:"), "{}", text.text);

    // not_found.
    let nf = call(&c, "get_capture", json!({"capture_id": "shot_nope"})).await;
    assert_eq!(err_code(&nf), "not_found");

    // Unknown tool is a protocol error, not a tool result.
    let e = c
        .call_tool(CallToolRequestParams::new("get_oauth_token"))
        .await;
    assert!(e.is_err());
}

#[tokio::test]
async fn confirm_tier_stub_is_a_clean_not_implemented() {
    // Headless daemon without engines: no dialog, a clear error.
    let (b, _dir) = builder();
    let (conn, _) = duplex_connector(b.build());
    let c = mcp_client(conn, "claude-desktop").await;
    let r = call(
        &c,
        "publish_video",
        json!({"capture_id": "rec_1", "platform": "youtube"}),
    )
    .await;
    assert_eq!(err_code(&r), "not_implemented");
    let r = call(&c, "start_recording", json!({"target": {"display_id": 1}})).await;
    assert_eq!(err_code(&r), "not_implemented");
}

#[tokio::test]
async fn agt_2_confirm_decline_and_read_only_policy_map_to_user_declined() {
    let engine = Arc::new(CountingExternal::default());
    let (b, _dir) = builder();
    let d = b
        .confirmer(FixedConfirmer::new(Decision::Decline))
        .external_actions(engine.clone())
        .build();
    d.policies().set("viewer", ClientPolicy::ReadOnly);
    let (conn, _) = duplex_connector(d);

    let c = mcp_client(conn.clone(), "claude-desktop").await;
    let r = call(
        &c,
        "publish_video",
        json!({"capture_id": "rec_1", "platform": "youtube"}),
    )
    .await;
    assert_eq!(err_code(&r), "user_declined");
    assert!(engine.calls().is_empty(), "AGT-2.AC1: nothing ran");

    // AGT-2.AC4: a read-only client is refused with data.reason and no dialog.
    let v = mcp_client(conn, "viewer").await;
    let shot = call(&v, "list_targets", json!({})).await;
    assert_ne!(shot.is_error, Some(true), "reads still work");
    let r = call(
        &v,
        "capture_screenshot",
        json!({"target": {"display_id": 1}}),
    )
    .await;
    assert_eq!(err_code(&r), "user_declined");
    assert_eq!(
        r.structured_content.as_ref().unwrap()["data"]["reason"],
        "client_read_only"
    );
}

#[tokio::test]
async fn agt_1_ac5_ten_concurrent_calls_from_two_clients() {
    let (b, _dir) = builder();
    let (conn, _) = duplex_connector(b.build());
    let a = Arc::new(mcp_client(conn.clone(), "client-a").await);
    let b = Arc::new(mcp_client(conn, "client-b").await);
    let mut set = tokio::task::JoinSet::new();
    for i in 0..10 {
        let c = if i % 2 == 0 { a.clone() } else { b.clone() };
        set.spawn(async move {
            if i % 3 == 0 {
                let r = call(
                    &c,
                    "capture_screenshot",
                    json!({"target": {"display_id": 2}}),
                )
                .await;
                ok(&r)["capture_id"].is_string()
            } else {
                let r = call(&c, "list_targets", json!({})).await;
                ok(&r)["displays"].is_array()
            }
        });
    }
    let mut n = 0;
    while let Some(r) = tokio::time::timeout(std::time::Duration::from_secs(20), set.join_next())
        .await
        .unwrap()
    {
        assert!(r.unwrap());
        n += 1;
    }
    assert_eq!(n, 10);
}

#[tokio::test]
async fn daemon_restart_reconnects_on_the_next_call() {
    let (b, _dir) = builder();
    let (conn, tasks) = duplex_connector(b.build());
    let c = mcp_client(conn, "claude-desktop").await;
    ok(&call(&c, "list_targets", json!({})).await);
    // "Crash": drop every server-side connection.
    for h in tasks.lock().unwrap().drain(..) {
        h.abort();
    }
    // The reader notices the closed stream asynchronously; either way the next call works.
    tokio::task::yield_now().await;
    let mut last = None;
    for _ in 0..50 {
        let r = call(&c, "list_targets", json!({})).await;
        if r.is_error != Some(true) {
            last = Some(r);
            break;
        }
        assert_eq!(err_code(&r), "daemon_unavailable");
    }
    assert!(last.is_some(), "reconnected");
}

#[tokio::test]
async fn unreachable_daemon_is_a_tool_error_not_a_hang() {
    let connector: Connector = Arc::new(|| {
        Box::pin(async {
            Err(std::io::Error::new(
                std::io::ErrorKind::ConnectionRefused,
                "refused",
            ))
        })
    });
    let c = mcp_client(connector, "claude-desktop").await;
    // Listing tools does not need the daemon (AGT-1.AC1).
    assert!(!c.list_all_tools().await.unwrap().is_empty());
    let r = call(&c, "list_targets", json!({})).await;
    assert_eq!(err_code(&r), "daemon_unavailable");
}

// ---- local Streamable HTTP (AGT-1.AC3) ----

mod http {
    use super::*;
    use cm_mcp::{serve_http, HttpOptions};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    /// One HTTP/1.1 request on a fresh connection; returns (status, lowercase headers, body).
    async fn request(
        addr: std::net::SocketAddr,
        token: Option<&str>,
        body: &str,
        session: Option<&str>,
    ) -> (u16, String, String) {
        let mut s = TcpStream::connect(addr).await.unwrap();
        let mut req = format!(
            "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nContent-Type: application/json\r\nAccept: application/json, text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n",
            addr.port(),
            body.len()
        );
        if let Some(t) = token {
            req.push_str(&format!("Authorization: Bearer {t}\r\n"));
        }
        if let Some(sid) = session {
            req.push_str(&format!(
                "Mcp-Session-Id: {sid}\r\nMCP-Protocol-Version: 2025-06-18\r\n"
            ));
        }
        req.push_str("\r\n");
        req.push_str(body);
        s.write_all(req.as_bytes()).await.unwrap();
        let mut out = Vec::new();
        let _ =
            tokio::time::timeout(std::time::Duration::from_secs(10), s.read_to_end(&mut out)).await;
        let text = String::from_utf8_lossy(&out).to_string();
        let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
        let status = head.split_whitespace().nth(1).unwrap().parse().unwrap();
        (status, head.to_lowercase(), body.to_owned())
    }

    /// The JSON message inside a response body that is either JSON or one SSE event stream.
    fn json_of(body: &str) -> Value {
        let data: String = body
            .lines()
            .filter_map(|l| l.strip_prefix("data:"))
            .map(str::trim)
            .filter(|l| l.starts_with('{'))
            .collect();
        let raw = if data.is_empty() {
            body.trim().to_owned()
        } else {
            data
        };
        serde_json::from_str(&raw).unwrap_or_else(|e| panic!("not JSON ({e}): {body}"))
    }

    #[tokio::test]
    async fn agt_1_ac3_http_needs_the_bearer_token_and_binds_loopback_only() {
        let (b, _dir) = builder();
        let (conn, _) = duplex_connector(b.build());

        // Non-loopback bind is refused.
        let bad = serve_http(
            conn.clone(),
            HttpOptions {
                addr: "0.0.0.0:0".parse().unwrap(),
                token: "t".into(),
            },
        )
        .await;
        assert!(bad.is_err());

        let server = serve_http(conn, HttpOptions::loopback("s3cret".into()))
            .await
            .unwrap();
        let addr = server.local_addr();
        assert!(addr.ip().is_loopback());

        let init = json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
            "protocolVersion":"2025-06-18","capabilities":{},
            "clientInfo":{"name":"cursor","version":"1"}}})
        .to_string();

        let (status, head, _) = request(addr, None, &init, None).await;
        assert_eq!(status, 401);
        assert!(head.contains("www-authenticate: bearer"));
        let (status, _, _) = request(addr, Some("wrong"), &init, None).await;
        assert_eq!(status, 401);

        let (status, head, body) = request(addr, Some("s3cret"), &init, None).await;
        assert_eq!(status, 200, "{body}");
        assert_eq!(
            json_of(&body)["result"]["serverInfo"]["name"],
            "capturemann"
        );
        let sid = head
            .lines()
            .find_map(|l| l.strip_prefix("mcp-session-id:"))
            .map(|s| s.trim().to_owned())
            .expect("session id header");

        let note = json!({"jsonrpc":"2.0","method":"notifications/initialized"}).to_string();
        let (status, _, _) = request(addr, Some("s3cret"), &note, Some(&sid)).await;
        assert_eq!(status, 202);

        let call = json!({"jsonrpc":"2.0","id":2,"method":"tools/call",
            "params":{"name":"list_targets","arguments":{}}})
        .to_string();
        let (status, _, body) = request(addr, Some("s3cret"), &call, Some(&sid)).await;
        assert_eq!(status, 200, "{body}");
        let v = json_of(&body);
        assert_eq!(v["result"]["isError"], false, "{v}");
        assert_eq!(
            v["result"]["structuredContent"]["displays"]
                .as_array()
                .unwrap()
                .len(),
            2
        );

        // Without the token the same session is refused too.
        let (status, _, _) = request(addr, None, &call, Some(&sid)).await;
        assert_eq!(status, 401);
        server.shutdown();
    }

    /// R2-6: connections beyond the cap are closed at once instead of piling up.
    #[tokio::test]
    async fn r2_6_http_connection_cap() {
        let (b, _dir) = builder();
        let (conn, _) = duplex_connector(b.build());
        let server = serve_http(conn, HttpOptions::loopback("t".into()))
            .await
            .unwrap();
        let addr = server.local_addr();
        let mut held = Vec::new();
        for _ in 0..cm_mcp::http::MAX_HTTP_CONNECTIONS {
            held.push(TcpStream::connect(addr).await.unwrap());
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let mut extra = TcpStream::connect(addr).await.unwrap();
        let mut b = [0u8; 1];
        let n = tokio::time::timeout(std::time::Duration::from_secs(5), extra.read(&mut b))
            .await
            .expect("the connection over the cap is closed, not left open")
            .unwrap_or(0);
        assert_eq!(n, 0);
        // A free slot is reusable.
        held.pop();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let init = json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
            "protocolVersion":"2025-06-18","capabilities":{},
            "clientInfo":{"name":"cursor","version":"1"}}})
        .to_string();
        let (status, _, _) = request(addr, Some("t"), &init, None).await;
        assert_eq!(status, 200);
        server.shutdown();
    }
}

/// EXP-4 / STO-3 / AGT-1: the job tools over MCP: read, list, cancel (idempotent), the
/// read-only policy, and `not_implemented` without a queue.
#[tokio::test]
async fn exp_4_job_tools_get_list_cancel() {
    let (b, dir) = builder();
    let queue = cm_daemon::library_job_queue(dir.path()).unwrap();
    let d = b.jobs(queue).build();
    d.register_job_handler(
        "hold",
        cm_jobs::handler_fn(|mut ctx| async move {
            ctx.reporter.report(0.25, Some("starting".to_owned()));
            ctx.reporter
                .save_checkpoint(&json!({"session_url": "https://secret.example/u"}))?;
            ctx.cancel.interrupted().await;
            Err(cm_core::CmError::internal("stopped"))
        }),
    )
    .unwrap();
    d.start_jobs().unwrap();
    d.policies().set("viewer", ClientPolicy::ReadOnly);
    let id = d
        .job_queue()
        .unwrap()
        .enqueue(cm_jobs::NewJob::new("hold", json!({"capture_id": "rec_1"})))
        .unwrap()
        .id;
    let (conn, _) = duplex_connector(d);
    let c = mcp_client(conn.clone(), "claude-desktop").await;

    let mut job = Value::Null;
    for _ in 0..500 {
        let r = call(&c, "get_job", json!({"job_id": id})).await;
        job = ok(&r).clone();
        if job["state"] == "running" && job["progress"] == 0.25 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(job["message"], "starting", "{job}");
    assert!(
        !job.to_string().contains("secret.example"),
        "no checkpoint over MCP"
    );
    let r = call(&c, "list_jobs", json!({"type": "hold"})).await;
    assert_eq!(ok(&r)["jobs"][0]["job_id"], id);
    let r = call(&c, "get_job", json!({"job_id": "nope"})).await;
    assert_eq!(err_code(&r), "not_found");

    // AGT-2.AC4: read-only clients read jobs but cannot cancel.
    let v = mcp_client(conn, "viewer").await;
    assert_eq!(
        ok(&call(&v, "list_jobs", json!({})).await)["jobs"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let r = call(&v, "cancel_job", json!({"job_id": id})).await;
    assert_eq!(err_code(&r), "user_declined");
    assert_eq!(
        r.structured_content.as_ref().unwrap()["data"]["reason"],
        "client_read_only"
    );

    let r = call(&c, "cancel_job", json!({"job_id": id})).await;
    assert_eq!(ok(&r)["cancelling"], true);
    for _ in 0..500 {
        let r = call(&c, "get_job", json!({"job_id": id})).await;
        if ok(&r)["state"] == "cancelled" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let again = call(&c, "cancel_job", json!({"job_id": id})).await;
    assert_eq!(ok(&again)["state"], "cancelled");
    assert_eq!(ok(&again)["cancelling"], false);
}

#[tokio::test]
async fn job_tools_without_a_queue_are_not_implemented() {
    let (b, _dir) = builder();
    let (conn, _) = duplex_connector(b.build());
    let c = mcp_client(conn, "claude-desktop").await;
    let r = call(&c, "list_jobs", json!({})).await;
    assert_eq!(err_code(&r), "not_implemented");
}

// ---- recording ---------------------------------------------------------------------------

fn recording_daemon(
    decision: Decision,
    sources: cm_daemon::testing::FakeSources,
) -> (Daemon, Arc<FixedConfirmer>, tempfile::TempDir) {
    let (b, dir) = builder();
    let confirmer = Arc::new(FixedConfirmer::new(decision));
    let d = b
        .recording_sources(sources)
        .recording_options(cm_daemon::EngineOptions {
            tick_every: std::time::Duration::from_millis(20),
            countdown_step: std::time::Duration::from_millis(20),
            host: Arc::new(cm_core::ManualHostClock::new(1_000_000_000)),
            free_space: Arc::new(cm_capture::disk::FakeFreeSpace::new(500_000_000_000)),
            disk_check_every: std::time::Duration::ZERO,
            own_pid: None,
            selftest_duration: std::time::Duration::from_millis(100),
            ..cm_daemon::EngineOptions::default()
        })
        .confirmer(confirmer.clone())
        .build();
    (d, confirmer, dir)
}

/// EXP-1 / EXP-4 / AGT-1: `export_video` is a Free, mutating tool with the spec's name; it
/// returns a job id, refuses the presets that do not exist yet, replays a `request_id`, is
/// refused for read-only clients, and fails with `not_implemented` without an exporter.
#[tokio::test]
async fn exp1_export_video_tool() {
    let (b, dir) = builder();
    let queue = cm_daemon::library_job_queue(dir.path()).unwrap();
    let d = b
        .jobs(queue)
        .recording_sources(cm_daemon::testing::FakeSources::new())
        .recording_options(cm_daemon::EngineOptions {
            tick_every: std::time::Duration::from_millis(20),
            countdown_step: std::time::Duration::from_millis(20),
            host: Arc::new(cm_core::ManualHostClock::new(1_000_000_000)),
            free_space: Arc::new(cm_capture::disk::FakeFreeSpace::new(500_000_000_000)),
            disk_check_every: std::time::Duration::ZERO,
            own_pid: None,
            ..cm_daemon::EngineOptions::default()
        })
        .build();
    d.policies().set("viewer", ClientPolicy::ReadOnly);
    // Record one take as the user (auto-export off: this test exports by hand).
    d.execute_method(
        cm_core::Actor::User,
        0,
        "set_settings",
        &json!({"values": {"export.auto_on_stop": false}}),
    )
    .await
    .unwrap();
    let started = d
        .execute_method(
            cm_core::Actor::User,
            0,
            "start_recording",
            &json!({"target": {"display_id": 1}, "countdown_s": 0}),
        )
        .await
        .unwrap();
    d.execute_method(cm_core::Actor::User, 0, "stop_recording", &json!({}))
        .await
        .unwrap();
    let capture_id = started["capture_id"].as_str().unwrap().to_owned();
    d.start_jobs().unwrap();
    let (conn, _) = duplex_connector(d.clone());
    let c = mcp_client(conn.clone(), "claude-desktop").await;

    let tools = c.list_all_tools().await.unwrap();
    let t = tools.iter().find(|t| t.name == "export_video").unwrap();
    assert!(t.description.as_deref().unwrap().contains("Tier: Free"));
    assert_eq!(t.input_schema["required"], json!(["capture_id"]));
    assert_eq!(t.input_schema["additionalProperties"], false);
    assert!(!t
        .annotations
        .as_ref()
        .unwrap()
        .read_only_hint
        .unwrap_or(false));

    // No exporter registered yet.
    let r = call(&c, "export_video", json!({"capture_id": capture_id})).await;
    assert_eq!(err_code(&r), "not_implemented");

    // VOC-2: `export_audio` is the same kind of tool (Free, mutating, strict schema).
    let a = tools.iter().find(|t| t.name == "export_audio").unwrap();
    assert!(a.description.as_deref().unwrap().contains("Tier: Free"));
    assert_eq!(a.input_schema["required"], json!(["capture_id"]));
    assert_eq!(a.input_schema["additionalProperties"], false);
    assert!(!a
        .annotations
        .as_ref()
        .unwrap()
        .read_only_hint
        .unwrap_or(false));
    let r = call(&c, "export_audio", json!({"capture_id": capture_id})).await;
    assert_eq!(err_code(&r), "not_implemented");

    d.register_job_handler(
        "export",
        cm_jobs::handler_fn(|ctx| async move {
            ctx.reporter.report(0.5, Some("half".to_owned()));
            Ok(json!({"path": "/x/out.mp4", "size_bytes": 7, "duration_s": 1.5}))
        }),
    )
    .unwrap();
    let first = call(
        &c,
        "export_video",
        json!({"capture_id": capture_id, "preset": "quick", "request_id": "e1"}),
    )
    .await;
    let job_id = ok(&first)["job_id"].as_str().unwrap().to_owned();
    let replay = call(
        &c,
        "export_video",
        json!({"capture_id": capture_id, "request_id": "e1"}),
    )
    .await;
    assert_eq!(ok(&replay)["job_id"], job_id.as_str(), "request_id replays");
    let mut job = Value::Null;
    for _ in 0..500 {
        job = ok(&call(&c, "get_job", json!({"job_id": job_id})).await).clone();
        if job["state"] == "done" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(job["state"], "done", "{job}");
    assert_eq!(job["result"]["path"], "/x/out.mp4");

    // Presets that do not exist yet, and unknown captures.
    let r = call(
        &c,
        "export_video",
        json!({"capture_id": capture_id, "preset": "youtube-4k"}),
    )
    .await;
    assert_eq!(err_code(&r), "invalid_input");
    let r = call(&c, "export_video", json!({"capture_id": "rec_nope"})).await;
    assert_eq!(err_code(&r), "not_found");

    // AGT-2.AC4: read-only clients cannot start an export.
    let v = mcp_client(conn, "viewer").await;
    let r = call(&v, "export_video", json!({"capture_id": capture_id})).await;
    assert_eq!(err_code(&r), "user_declined");
}

/// REC-3: `run_fps_selftest` is a Confirm tool; a pass opens 60 fps for `start_recording`, which
/// before it fails with `invalid_input` and `data.suggestion`.
#[tokio::test]
async fn rec3_fps_selftest_tool_and_the_60_fps_gate() {
    let (d, confirmer, _dir) =
        recording_daemon(Decision::Approve, cm_daemon::testing::FakeSources::new());
    let (conn, _) = duplex_connector(d);
    let c = mcp_client(conn, "claude-desktop").await;
    let tools = c.list_all_tools().await.unwrap();
    let t = tools.iter().find(|t| t.name == "run_fps_selftest").unwrap();
    let desc = t.description.as_deref().unwrap();
    assert!(desc.contains("Tier: Confirm"), "{desc}");
    assert!(desc.contains("deletes the test video"), "{desc}");
    assert_eq!(t.input_schema["additionalProperties"], false);
    assert!(t.input_schema.get("required").is_none());
    let props = t.input_schema["properties"].as_object().unwrap();
    for k in ["target", "size", "codec", "request_id"] {
        assert!(props.contains_key(k), "{k}");
    }
    assert!(!t
        .annotations
        .as_ref()
        .unwrap()
        .read_only_hint
        .unwrap_or(false));

    let r = call(
        &c,
        "start_recording",
        json!({ "target": { "display_id": 1 }, "countdown_s": 0, "fps": 60 }),
    )
    .await;
    assert_eq!(err_code(&r), "invalid_input");
    let sg = &r.structured_content.as_ref().unwrap()["data"]["suggestion"];
    assert_eq!(sg["fps"], 30, "{r:?}");
    assert_eq!(sg["run_selftest"]["method"], "run_fps_selftest");
    assert!(confirmer.shown().is_empty(), "refused before any dialog");

    let r = call(
        &c,
        "run_fps_selftest",
        json!({ "target": { "display_id": 1 } }),
    )
    .await;
    let v = ok(&r);
    assert_eq!(v["state"], "passed", "{v}");
    assert_eq!(v["stored"], true);
    let shown = confirmer.shown();
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].tool, "run_fps_selftest");
    assert_eq!(shown[0].client, "claude-desktop");

    let r = call(
        &c,
        "start_recording",
        json!({ "target": { "display_id": 1 }, "countdown_s": 0, "fps": 60 }),
    )
    .await;
    assert_eq!(ok(&r)["indicator"]["output"]["fps"], 60);
    let r = call(&c, "stop_recording", json!({})).await;
    ok(&r);
    // A bad input is refused with the field name, before any dialog.
    let r = call(&c, "run_fps_selftest", json!({ "size": "8k" })).await;
    assert_eq!(err_code(&r), "invalid_input");
    c.cancel().await.unwrap();
}

/// AGT-1.AC1 / AGT-2: the recording tools exist with the spec's tiers, schemas and hints:
/// `start_recording` is Confirm, the controls and status are Free, status and devices read-only;
/// the HUD's discard/restart are not tools.
#[tokio::test]
async fn rec_tools_have_the_spec_tiers_and_schemas() {
    let (d, _c, _dir) = recording_daemon(Decision::Approve, cm_daemon::testing::FakeSources::new());
    let (conn, _) = duplex_connector(d);
    let c = mcp_client(conn, "claude-desktop").await;
    let tools = c.list_all_tools().await.unwrap();
    let get = |n: &str| {
        tools
            .iter()
            .find(|t| t.name == n)
            .unwrap_or_else(|| panic!("tool {n}"))
    };
    let start = get("start_recording");
    assert!(start
        .description
        .as_deref()
        .unwrap()
        .contains("Tier: Confirm"));
    // `target` is required for a screen recording only: `mode: "audio"` has none (VOC-1), so the
    // schema asks for a `target` or a `mode` and the daemon says which one is missing.
    assert!(start.input_schema.get("required").is_none());
    assert_eq!(
        start.input_schema["anyOf"],
        json!([{ "required": ["target"] }, { "required": ["mode"] }])
    );
    assert_eq!(start.input_schema["additionalProperties"], false);
    let props = start.input_schema["properties"].as_object().unwrap();
    assert_eq!(props["mode"]["enum"], json!(["screen", "audio"]));
    assert_eq!(
        props["preset"]["enum"],
        json!(["room", "online", "lecture", "product", "note"])
    );
    let marker = get("add_marker");
    assert!(marker
        .description
        .as_deref()
        .unwrap()
        .contains("Tier: Free"));
    assert_eq!(
        marker.annotations.as_ref().unwrap().read_only_hint,
        Some(false)
    );
    for k in ["recording_id", "capture_id", "at", "label", "request_id"] {
        assert!(marker.input_schema["properties"]
            .as_object()
            .unwrap()
            .contains_key(k));
    }
    for k in [
        "target",
        "size",
        "fps",
        "codec",
        "quality",
        "camera",
        "mic",
        "system_audio",
        "cursor",
        "max_duration_s",
        "countdown_s",
        "title",
        "exclude",
        "request_id",
    ] {
        assert!(props.contains_key(k), "start_recording.{k}");
    }
    assert_eq!(props["fps"]["enum"], json!([30, 60]));
    for n in [
        "pause_recording",
        "resume_recording",
        "set_mic_muted",
        "stop_recording",
    ] {
        let t = get(n);
        assert!(
            t.description.as_deref().unwrap().contains("Tier: Free"),
            "{n}"
        );
        assert_eq!(
            t.annotations.as_ref().unwrap().read_only_hint,
            Some(false),
            "{n}"
        );
        assert!(
            t.input_schema["properties"]
                .as_object()
                .unwrap()
                .contains_key("request_id"),
            "{n}"
        );
    }
    for n in ["get_recording_status", "list_devices"] {
        let t = get(n);
        assert_eq!(
            t.annotations.as_ref().unwrap().read_only_hint,
            Some(true),
            "{n}"
        );
        assert!(
            !t.input_schema["properties"]
                .as_object()
                .unwrap()
                .contains_key("request_id"),
            "{n}"
        );
    }
    for n in ["discard_recording", "restart_recording", "skip_countdown"] {
        assert!(tools.iter().all(|t| t.name != n), "{n} is internal");
    }
    // The Confirm description no longer promises `not_implemented` for recording.
    assert!(!start
        .description
        .as_deref()
        .unwrap()
        .contains("Not available in this build yet"));
    // `publish_video` has its engine now (YouTube): the description says how it works.
    let publish = get("publish_video")
        .description
        .as_deref()
        .unwrap()
        .to_owned();
    assert!(!publish.contains("Not available in this build yet"));
    assert!(publish.contains("approve on screen"));
    assert!(publish.contains("over the internet"));
    // `delete_capture` works locally now (the Trash); only the remote part is missing.
    assert!(!get("delete_capture")
        .description
        .as_deref()
        .unwrap()
        .contains("Not available in this build yet"));
    c.cancel().await.unwrap();
}

/// REC-1..REC-4 through MCP: start (the user approves), status, pause, resume, stop; the result
/// carries the ids and tracks the spec's tool table promises.
#[tokio::test]
async fn rec_mcp_start_status_pause_resume_stop() {
    let (d, confirmer, _dir) =
        recording_daemon(Decision::Approve, cm_daemon::testing::FakeSources::new());
    let (conn, _) = duplex_connector(d.clone());
    let c = mcp_client(conn, "claude-desktop").await;

    let dev = call(&c, "list_devices", json!({})).await;
    assert_eq!(ok(&dev)["mics"][0]["id"], "fake-mic");

    let r = call(
        &c,
        "start_recording",
        json!({ "target": { "display_id": 1 }, "countdown_s": 0, "mic": true,
                "camera": { "shape": "circle", "position": "bottom-right" } }),
    )
    .await;
    let started = ok(&r).clone();
    assert!(started["recording_id"]
        .as_str()
        .unwrap()
        .starts_with("rec_"));
    assert_eq!(
        started["tracks"],
        json!(["screen", "cursor", "mic", "camera"])
    );
    assert_eq!(
        started["indicator"]["started_by"]["client"],
        "claude-desktop"
    );
    assert_eq!(confirmer.shown().len(), 1, "AGT-2: the user was asked");
    assert_eq!(confirmer.shown()[0].client, "claude-desktop");

    let st = call(&c, "get_recording_status", json!({})).await;
    assert_eq!(ok(&st)["state"], "recording");
    let p = call(
        &c,
        "pause_recording",
        json!({ "recording_id": started["recording_id"] }),
    )
    .await;
    assert_eq!(ok(&p)["state"], "paused");
    let r = call(&c, "resume_recording", json!({})).await;
    assert_eq!(ok(&r)["state"], "recording");
    // AUD-6: Free, no dialog, idempotent, shown by the status; a bad input is invalid_input.
    let shown = confirmer.shown().len();
    let m = call(&c, "set_mic_muted", json!({ "muted": true })).await;
    assert_eq!(ok(&m)["muted"], true);
    assert_eq!(ok(&m)["changed"], true);
    assert_eq!(ok(&m)["recording_id"], started["recording_id"]);
    let m = call(&c, "set_mic_muted", json!({ "muted": true })).await;
    assert_eq!(ok(&m)["changed"], false);
    let st = call(&c, "get_recording_status", json!({})).await;
    assert_eq!(ok(&st)["mic_muted"], true);
    let bad = call(&c, "set_mic_muted", json!({})).await;
    assert_eq!(err_code(&bad), "invalid_input");
    let m = call(&c, "set_mic_muted", json!({ "muted": false })).await;
    assert_eq!(ok(&m)["muted"], false);
    assert_eq!(confirmer.shown().len(), shown, "set_mic_muted never asks");
    let s = call(&c, "stop_recording", json!({ "request_id": "s-1" })).await;
    let stopped = ok(&s).clone();
    assert_eq!(stopped["capture_id"], started["capture_id"]);
    assert_eq!(stopped["reason"], "agent");
    assert!(stopped["tracks"].as_array().unwrap().len() >= 4);
    // Idempotent by request id, and the capture is in the library.
    let again = call(&c, "stop_recording", json!({ "request_id": "s-1" })).await;
    assert_eq!(ok(&again), &stopped);
    let lib = call(&c, "list_library", json!({ "kind": "recording" })).await;
    assert_eq!(ok(&lib)["captures"][0]["id"], started["capture_id"]);
    assert_eq!(
        confirmer.shown().len(),
        1,
        "pause, resume and stop never ask"
    );
    c.cancel().await.unwrap();
}

/// AGT-2 over MCP: declined and permission errors carry their shared codes in structuredContent;
/// a second start is `busy`; a read-only client cannot control a recording.
#[tokio::test]
async fn rec_mcp_errors_carry_shared_codes() {
    // Declined.
    let (d, _c, _dir) = recording_daemon(Decision::Decline, cm_daemon::testing::FakeSources::new());
    let (conn, _) = duplex_connector(d.clone());
    let c = mcp_client(conn, "claude-desktop").await;
    let r = call(
        &c,
        "start_recording",
        json!({ "target": { "display_id": 1 }, "countdown_s": 0 }),
    )
    .await;
    assert_eq!(err_code(&r), "user_declined");
    assert!(!d.recording_active());
    // Invalid input names the field.
    let r = call(
        &c,
        "start_recording",
        json!({ "target": { "display_id": 1 }, "fps": 24 }),
    )
    .await;
    assert_eq!(err_code(&r), "invalid_input");
    assert!(r.structured_content.as_ref().unwrap()["message"]
        .as_str()
        .unwrap()
        .starts_with("fps"));
    c.cancel().await.unwrap();

    // Permission missing, before the dialog.
    let (d, confirmer, _dir) = recording_daemon(
        Decision::Approve,
        cm_daemon::testing::FakeSources::new().with_missing_permission(cm_core::Permission::Screen),
    );
    let (conn, _) = duplex_connector(d);
    let c = mcp_client(conn, "claude-desktop").await;
    let r = call(
        &c,
        "start_recording",
        json!({ "target": { "display_id": 1 }, "countdown_s": 0 }),
    )
    .await;
    assert_eq!(err_code(&r), "permission_missing");
    assert_eq!(
        r.structured_content.as_ref().unwrap()["data"]["permission"],
        "screen"
    );
    assert!(confirmer.shown().is_empty());
    c.cancel().await.unwrap();

    // Busy, and read-only.
    let (d, _c, _dir) = recording_daemon(Decision::Approve, cm_daemon::testing::FakeSources::new());
    d.policies().set("viewer", ClientPolicy::ReadOnly);
    let (conn, _) = duplex_connector(d);
    let c = mcp_client(conn.clone(), "claude-desktop").await;
    ok(&call(
        &c,
        "start_recording",
        json!({ "target": { "display_id": 1 }, "countdown_s": 0 }),
    )
    .await);
    let r = call(
        &c,
        "start_recording",
        json!({ "target": { "display_id": 1 }, "countdown_s": 0 }),
    )
    .await;
    assert_eq!(err_code(&r), "busy");
    let v = mcp_client(conn, "viewer").await;
    for tool in [
        "pause_recording",
        "resume_recording",
        "stop_recording",
        "set_mic_muted",
    ] {
        let args = if tool == "set_mic_muted" {
            json!({ "muted": true })
        } else {
            json!({})
        };
        let r = call(&v, tool, args).await;
        assert_eq!(err_code(&r), "user_declined", "{tool}");
        assert_eq!(
            r.structured_content.as_ref().unwrap()["data"]["reason"],
            "client_read_only"
        );
    }
    assert_eq!(
        ok(&call(&v, "get_recording_status", json!({})).await)["state"],
        "recording"
    );
    ok(&call(&c, "stop_recording", json!({})).await);
    c.cancel().await.unwrap();
    v.cancel().await.unwrap();
}

/// R2-14: dropping a tool call cancels the request at the daemon, so the confirmation dialog it
/// was waiting on closes instead of lingering.
#[tokio::test]
async fn r2_14_dropped_call_closes_the_pending_confirmation_dialog() {
    use async_trait::async_trait;
    use cm_daemon::{ConfirmationRequest, Confirmer};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    struct DropFlag(Arc<AtomicBool>);
    impl Drop for DropFlag {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }
    struct Hang {
        opened: Arc<AtomicUsize>,
        closed: Arc<AtomicBool>,
    }
    #[async_trait]
    impl Confirmer for Hang {
        async fn confirm(&self, _r: &ConfirmationRequest) -> Decision {
            self.opened.fetch_add(1, Ordering::SeqCst);
            let _flag = DropFlag(self.closed.clone());
            std::future::pending().await
        }
    }
    let (opened, closed) = (
        Arc::new(AtomicUsize::default()),
        Arc::new(AtomicBool::default()),
    );
    let (b, _dir) = builder();
    let daemon = b
        .external_actions(Arc::new(CountingExternal::default()))
        .confirmer(Hang {
            opened: opened.clone(),
            closed: closed.clone(),
        })
        .build();
    let (connector, _) = duplex_connector(daemon);
    let (r, w) = connector().await.unwrap();
    let conn = cm_mcp::DaemonConn::new(r, w);
    conn.call("session.hello", json!({"client": "mcp-test"}))
        .await
        .unwrap()
        .unwrap();
    // The call waits for the dialog; the caller gives up after 300 ms (an MCP cancel).
    let waited = tokio::time::timeout(
        std::time::Duration::from_millis(300),
        conn.call("delete_capture", json!({"capture_id": "shot_1"})),
    )
    .await;
    assert!(waited.is_err(), "the call was still waiting for the user");
    assert_eq!(opened.load(Ordering::SeqCst), 1);
    for _ in 0..200 {
        if closed.load(Ordering::SeqCst) {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert!(
        closed.load(Ordering::SeqCst),
        "the dialog was closed after the cancel"
    );
    // The connection is still usable.
    let r = conn.call("get_settings", json!({})).await.unwrap().unwrap();
    assert!(r.get("settings").is_some());
}

/// `rename_capture` (Free) and `delete_capture` (Confirm: the folder goes to the Trash) through
/// MCP, plus `annotate` without the image.
#[tokio::test]
async fn library_management_tools_rename_delete_and_annotate_without_image() {
    use cm_daemon::testing::FakeTrasher;
    let confirmer = Arc::new(FixedConfirmer::new(Decision::Approve));
    let trasher = Arc::new(FakeTrasher::new());
    let (b, _dir) = builder();
    let d = b
        .confirmer(confirmer.clone())
        .trasher(trasher.clone())
        .build();
    let (conn, _) = duplex_connector(d);
    let c = mcp_client(conn, "claude-desktop").await;
    let shot = call(
        &c,
        "capture_screenshot",
        json!({"target": {"display_id": 1}, "title": "Old title"}),
    )
    .await;
    let id = ok(&shot)["capture_id"].as_str().unwrap().to_owned();

    let renamed = call(
        &c,
        "rename_capture",
        json!({"capture_id": id, "title": "New title", "request_id": "r-1"}),
    )
    .await;
    assert_eq!(ok(&renamed)["title"], "New title");
    assert!(ok(&renamed)["path"]
        .as_str()
        .unwrap()
        .ends_with("New title.cmshot"));
    assert!(confirmer.shown().is_empty(), "Free tier: no dialog");
    let bad = call(&c, "rename_capture", json!({"capture_id": id})).await;
    assert_eq!(err_code(&bad), "invalid_input");

    let ann = call(
        &c,
        "annotate",
        json!({"capture_id": id, "ops": [{"op":"rect","rect":[1,1,5,5]}], "return_image": false}),
    )
    .await;
    assert!(ok(&ann)["image"].is_null());
    assert_eq!(ann.content.len(), 1, "no image block");

    let del = call(&c, "delete_capture", json!({"capture_id": id})).await;
    assert_eq!(ok(&del)["moved_to_trash"], true);
    let shown = confirmer.shown();
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].details["capture"], "New title");
    assert_eq!(shown[0].details["result"], "Moved to the Trash");
    assert_eq!(trasher.trashed().len(), 1);
    let lib = call(&c, "list_library", json!({})).await;
    assert!(ok(&lib)["captures"].as_array().unwrap().is_empty());
    let remote = call(
        &c,
        "delete_capture",
        json!({"capture_id": id, "also_delete_remote": true}),
    )
    .await;
    // The flag is accepted: the capture is gone already, so it is `not_found`, not a refusal of
    // the parameter.
    assert_eq!(err_code(&remote), "not_found");
    c.cancel().await.unwrap();
}

/// LIB-2: `move_library` is a Confirm tool (same schema as the daemon parses, typed errors).
#[tokio::test]
async fn move_library_is_a_confirm_tool_with_typed_errors() {
    let confirmer = Arc::new(FixedConfirmer::new(Decision::Approve));
    let (b, dir) = builder();
    let d = b.confirmer(confirmer.clone()).build();
    let (conn, _) = duplex_connector(d);
    let c = mcp_client(conn, "claude-desktop").await;
    let tools = c.list_all_tools().await.unwrap();
    let t = tools.iter().find(|t| t.name == "move_library").unwrap();
    assert!(t
        .description
        .as_deref()
        .unwrap_or_default()
        .contains("Confirm"));
    // Bad input and a path inside the library fail before any dialog, with a typed reason.
    let bad = call(&c, "move_library", json!({"path": "/x", "mode": "copy"})).await;
    assert_eq!(err_code(&bad), "invalid_input");
    let inside = call(
        &c,
        "move_library",
        json!({"path": dir.path().join("inner"), "mode": "fresh"}),
    )
    .await;
    assert_eq!(err_code(&inside), "invalid_input");
    assert!(confirmer.shown().is_empty());
    // Fresh in another folder: asked, then done at once.
    let other = tempfile::tempdir().unwrap();
    let ok_r = call(
        &c,
        "move_library",
        json!({"path": other.path(), "mode": "fresh"}),
    )
    .await;
    assert_eq!(ok(&ok_r)["restart_required"], true);
    assert!(ok(&ok_r)["job_id"].is_null());
    assert_eq!(confirmer.shown().len(), 1);
    assert_eq!(confirmer.shown()[0].kind, "move_library");
    c.cancel().await.unwrap();
}

// ---- event notifications ---------------------------------------------------------------------

mod notifications {
    #![allow(deprecated)]
    use super::*;
    use rmcp::model::{LoggingLevel, LoggingMessageNotificationParam, SetLevelRequestParams};
    use rmcp::service::NotificationContext;
    use rmcp::{ClientHandler, RoleClient};
    use std::time::Duration;

    type Seen = Arc<Mutex<Vec<LoggingMessageNotificationParam>>>;

    /// Collects every `notifications/message` the server sends.
    #[derive(Clone)]
    struct Collector {
        name: &'static str,
        seen: Seen,
    }

    #[allow(deprecated)]
    impl ClientHandler for Collector {
        fn get_info(&self) -> ClientConfig {
            ClientConfig::new(
                ClientCapabilities::default(),
                Implementation::new(self.name, "1.0"),
            )
        }

        async fn on_logging_message(
            &self,
            params: LoggingMessageNotificationParam,
            _context: NotificationContext<RoleClient>,
        ) {
            self.seen.lock().unwrap().push(params);
        }
    }

    async fn listener(
        connector: Connector,
        name: &'static str,
    ) -> (RunningService<RoleClient, Collector>, Seen) {
        let (c, s) = tokio::io::duplex(1 << 22);
        tokio::spawn(async move {
            if let Ok(running) = McpServer::new(connector).serve(s).await {
                let _ = running.waiting().await;
            }
        });
        let seen: Seen = Arc::default();
        let client = Collector {
            name,
            seen: seen.clone(),
        }
        .serve(c)
        .await
        .unwrap();
        (client, seen)
    }

    #[allow(deprecated)]
    fn events(seen: &Seen) -> Vec<String> {
        seen.lock()
            .unwrap()
            .iter()
            .map(|p| {
                assert_eq!(p.logger.as_deref(), Some("capturemann.events"));
                p.data["event"].as_str().unwrap().to_owned()
            })
            .collect()
    }

    async fn wait_for(seen: &Seen, event: &str) {
        for _ in 0..100 {
            if events(seen).iter().any(|e| e == event) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("no `{event}` notification, saw {:?}", events(seen));
    }

    /// Publishes a probe until the listener has one, so later publishes cannot race the
    /// subscription (the connection opens when the MCP session is initialized).
    async fn wait_for_subscription(bus: &cm_daemon::EventBus, seen: &Seen) {
        for _ in 0..100 {
            bus.publish("job.progress", json!({ "probe": true }));
            if !events(seen).is_empty() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("never subscribed");
    }

    /// AGT-1: daemon events reach an MCP client as notifications, before any tool call, and
    /// the high-rate recording events are left out.
    #[tokio::test]
    #[allow(deprecated)]
    async fn agt_1_daemon_events_arrive_as_mcp_notifications() {
        let (b, _dir) = builder();
        let d = b.build();
        let bus = d.event_bus().clone();
        let (conn, _) = duplex_connector(d);
        let (c, seen) = listener(conn, "claude-desktop").await;
        wait_for_subscription(&bus, &seen).await;

        bus.publish("recording.stats", json!({ "recorded_ms": 1000 }));
        bus.publish("recording.health", json!({ "status": "ok" }));
        let args = json!({"target": {"display_id": 1}});
        let params = CallToolRequestParams::new("capture_screenshot".to_owned())
            .with_arguments(args.as_object().unwrap().clone());
        let r = c.call_tool(params).await.unwrap();
        let id = ok(&r)["capture_id"].as_str().unwrap().to_owned();
        wait_for(&seen, "capture.created").await;
        bus.publish("recording.warning", json!({ "code": "disk_low" }));
        wait_for(&seen, "recording.warning").await;

        let got = events(&seen);
        assert!(!got
            .iter()
            .any(|e| e == "recording.stats" || e == "recording.health"));
        let find = |name: &str| {
            seen.lock()
                .unwrap()
                .iter()
                .find(|p| p.data["event"] == name)
                .cloned()
                .unwrap()
        };
        let created = find("capture.created");
        assert_eq!(created.data["data"]["capture_id"], id);
        assert!(created.data["seq"].is_u64());
        assert!(matches!(created.level, LoggingLevel::Info));
        assert!(matches!(
            find("recording.warning").level,
            LoggingLevel::Warning
        ));
    }

    /// AGT-2: the daemon's per-client policy decides; a blocked client receives no events.
    #[tokio::test]
    async fn agt_2_blocked_client_receives_no_events() {
        let (b, _dir) = builder();
        let d = b.build();
        d.policies().set("blocked", ClientPolicy::Blocked);
        let bus = d.event_bus().clone();
        let (conn, _) = duplex_connector(d);
        let (_c, seen) = listener(conn.clone(), "blocked").await;
        let (_ok, ok_seen) = listener(conn, "claude-desktop").await;
        wait_for_subscription(&bus, &ok_seen).await;
        bus.publish("job.done", json!({ "job_id": "j1" }));
        wait_for(&ok_seen, "job.done").await;
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(events(&seen).is_empty(), "{:?}", events(&seen));
    }

    /// `logging/setLevel` filters notifications by severity.
    #[tokio::test]
    #[allow(deprecated)]
    async fn agt_1_set_level_filters_notifications() {
        let (b, _dir) = builder();
        let d = b.build();
        let bus = d.event_bus().clone();
        let (conn, _) = duplex_connector(d);
        let (c, seen) = listener(conn, "claude-desktop").await;
        wait_for_subscription(&bus, &seen).await;
        c.set_level(SetLevelRequestParams::new(LoggingLevel::Error))
            .await
            .unwrap();
        seen.lock().unwrap().clear();
        bus.publish("job.done", json!({ "job_id": "j2" }));
        bus.publish("job.failed", json!({ "job_id": "j3" }));
        wait_for(&seen, "job.failed").await;
        assert_eq!(events(&seen), vec!["job.failed".to_owned()]);
    }

    /// AI: `model.changed` and `transcript.ready` reach the client; the download ticks do not.
    #[tokio::test]
    async fn ai_model_and_transcript_events_are_forwarded() {
        let (b, _dir) = builder();
        let d = b.build();
        let bus = d.event_bus().clone();
        let (conn, _) = duplex_connector(d);
        let (_c, seen) = listener(conn, "claude-desktop").await;
        wait_for_subscription(&bus, &seen).await;
        seen.lock().unwrap().clear();
        bus.publish(
            "model.download.progress",
            json!({ "model": "small", "fraction": 0.5 }),
        );
        bus.publish(
            "model.changed",
            json!({ "model": "small", "change": "downloaded" }),
        );
        bus.publish("transcript.ready", json!({ "capture_id": "rec_1" }));
        wait_for(&seen, "transcript.ready").await;
        // (The subscription probe may still deliver a late `job.progress`.)
        let got: Vec<String> = events(&seen)
            .into_iter()
            .filter(|e| e != "job.progress")
            .collect();
        assert_eq!(
            got,
            vec!["model.changed".to_owned(), "transcript.ready".to_owned()]
        );
    }
}

/// AI-1 through MCP: the tools exist with the right tiers; `transcribe` without a model is a tool
/// error that carries `data.reason = "no_model"`; a download waits for the user's approval and
/// then runs as a job; nothing is downloaded before.
#[tokio::test]
async fn ai_tools_no_model_error_and_confirmed_download() {
    use cm_daemon::testing::FakeAi;
    let (b, dir) = builder();
    let queue = cm_daemon::library_job_queue(dir.path()).unwrap();
    let ai = FakeAi::new();
    let confirmer = Arc::new(FixedConfirmer::new(Decision::Approve));
    let d = b
        .jobs(queue)
        .confirmer(confirmer.clone())
        .ai(ai.services())
        .build();
    d.register_ai_handlers().unwrap();
    d.start_jobs().unwrap();
    let (conn, _) = duplex_connector(d);
    let c = mcp_client(conn, "claude-desktop").await;

    let tools = c.list_all_tools().await.unwrap();
    let tool = |n: &str| tools.iter().find(|t| t.name == n).unwrap().clone();
    for n in ["download_model", "remove_model"] {
        assert!(
            tool(n)
                .description
                .as_deref()
                .unwrap()
                .contains("Tier: Confirm"),
            "{n}"
        );
    }
    for n in [
        "list_models",
        "transcribe",
        "get_transcript",
        "get_captions",
        "set_captions",
        "replace_in_captions",
        "suggest_cuts",
        "diarize",
        "set_speakers",
    ] {
        assert!(
            tool(n)
                .description
                .as_deref()
                .unwrap()
                .contains("Tier: Free"),
            "{n}"
        );
    }
    assert_eq!(
        tool("list_models")
            .annotations
            .as_ref()
            .unwrap()
            .read_only_hint,
        Some(true)
    );
    assert_eq!(
        tool("remove_model")
            .annotations
            .as_ref()
            .unwrap()
            .destructive_hint,
        Some(true)
    );

    let listed = call(&c, "list_models", json!({})).await;
    assert_eq!(ok(&listed)["models"].as_array().unwrap().len(), 3);
    assert!(ai.downloads_started().is_empty());

    // No model: the error carries the machine-readable reason.
    let r = call(&c, "transcribe", json!({"capture_id": "rec_1"})).await;
    assert_eq!(err_code(&r), "invalid_input");
    assert!(
        r.structured_content
            .as_ref()
            .unwrap()
            .to_string()
            .contains("no_model"),
        "{r:?}"
    );
    assert!(
        ai.downloads_started().is_empty(),
        "nothing is downloaded silently"
    );

    // The user approves the dialog; the download is a job.
    let started = call(&c, "download_model", json!({"model": "small"})).await;
    let job_id = ok(&started)["job_id"].as_str().unwrap().to_owned();
    assert_eq!(confirmer.shown().len(), 1);
    assert!(confirmer.shown()[0].summary.contains("Small"));
    let mut state = String::new();
    for _ in 0..200 {
        let j = call(&c, "get_job", json!({"job_id": job_id})).await;
        state = ok(&j)["state"].as_str().unwrap().to_owned();
        if state == "done" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(state, "done");
    assert_eq!(ai.installed(), vec!["small"]);
    c.cancel().await.unwrap();
}

/// `list_destinations` and `upload` (STO-1, STO-3, STO-5) through MCP: the job copies a file to
/// the destination the user set up; bad input is `invalid_input`, an unknown destination is not
/// found. Tested with a local folder.
#[tokio::test]
async fn sto_1_list_destinations_and_upload_tools() {
    use cm_storage::{Destination, DestinationStore, Kind, What};
    let tmp = tempfile::tempdir().unwrap();
    let nas = tmp.path().join("nas");
    std::fs::create_dir(&nas).unwrap();
    let store = DestinationStore::new(tmp.path().join("config"));
    store
        .save(&[Destination {
            id: "nas".into(),
            name: "NAS".into(),
            kind: Kind::Local,
            path: nas.clone(),
            account: None,
            folder_pattern: "{year}".into(),
            what: What::Original,
            auto_upload: cm_storage::AutoUpload::Off,
            default: true,
        }])
        .unwrap();
    let (b, dir) = builder();
    let queue = cm_daemon::library_job_queue(dir.path()).unwrap();
    let d = b.jobs(queue).destinations(store).build();
    d.register_upload_handler().unwrap();
    d.start_jobs().unwrap();
    let (conn, _) = duplex_connector(d);
    let c = mcp_client(conn, "claude-desktop").await;

    let list = call(&c, "list_destinations", json!({})).await;
    let l = ok(&list)["destinations"].as_array().unwrap();
    assert_eq!(l.len(), 1);
    assert_eq!(l[0]["id"], "nas");
    assert_eq!(l[0]["online"], true);

    let file = tmp.path().join("clip.mp4");
    std::fs::write(&file, b"clip bytes").unwrap();
    let up = call(
        &c,
        "upload",
        json!({"file": file.to_str().unwrap(), "destination_id": "nas", "request_id": "u-1"}),
    )
    .await;
    let job_id = ok(&up)["job_id"].as_str().unwrap().to_owned();
    let mut state = String::new();
    for _ in 0..200 {
        let j = call(&c, "get_job", json!({"job_id": job_id})).await;
        state = ok(&j)["state"].as_str().unwrap().to_owned();
        if state == "done" || state == "failed" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert_eq!(state, "done");

    let both = call(&c, "upload", json!({"file": "a", "capture_id": "b"})).await;
    assert_eq!(err_code(&both), "invalid_input");
    let none = call(&c, "upload", json!({})).await;
    assert_eq!(err_code(&none), "invalid_input");
    let unknown = call(
        &c,
        "upload",
        json!({"file": file.to_str().unwrap(), "destination_id": "nope"}),
    )
    .await;
    assert_eq!(err_code(&unknown), "not_found");
}

/// `list_publish_accounts` and `publish_video` validation through MCP (PUB-1, PUB-2): the list has
/// the channel and the quota; a request that cannot run fails before any dialog.
#[tokio::test]
async fn pub_list_publish_accounts_and_publish_validation_tools() {
    use cm_publish::{Privacy, PublishAccount, PublishAccountStore};
    let tmp = tempfile::tempdir().unwrap();
    let store = PublishAccountStore::new(tmp.path());
    store
        .save(&[PublishAccount {
            id: "youtube-abc".into(),
            platform: "youtube".into(),
            name: "Teacher Channel".into(),
            account_id: "UC1".into(),
            default_privacy: Privacy::Public,
        }])
        .unwrap();
    let (b, dir) = builder();
    let queue = cm_daemon::library_job_queue(dir.path()).unwrap();
    let confirmer = Arc::new(FixedConfirmer::new(Decision::Approve));
    let d = b
        .jobs(queue)
        .publish_accounts(store)
        .confirmer(confirmer.clone())
        .build();
    d.register_publish_handler().unwrap();
    d.start_jobs().unwrap();
    let (conn, _) = duplex_connector(d);
    let c = mcp_client(conn, "claude-desktop").await;

    let list = call(&c, "list_publish_accounts", json!({})).await;
    let a = &ok(&list)["accounts"][0];
    assert_eq!(a["name"], "Teacher Channel");
    assert_eq!(a["default_privacy"], "unlisted", "never public by default");
    assert_eq!(a["quota"]["limit"], 100);

    let bad = call(
        &c,
        "publish_video",
        json!({"capture_id": "rec_1", "platform": "youtube", "privacy": "world"}),
    )
    .await;
    assert_eq!(err_code(&bad), "invalid_input");
    let missing = call(
        &c,
        "publish_video",
        json!({"capture_id": "rec_nope", "platform": "youtube", "title": "T", "made_for_kids": false}),
    )
    .await;
    assert_eq!(err_code(&missing), "not_found");
    assert!(
        confirmer.shown().is_empty(),
        "no dialog for a request that cannot run"
    );
}

// ---- resources and prompts (AGT-1) -------------------------------------------------------

mod resources_and_prompts {
    use super::*;
    use rmcp::model::{
        ErrorCode, GetPromptRequestParams, ReadResourceRequestParams, ResourceContents,
    };
    use rmcp::service::ServiceError;

    fn text_of(r: &rmcp::model::ReadResourceResult) -> Value {
        assert_eq!(r.contents.len(), 1);
        match &r.contents[0] {
            ResourceContents::TextResourceContents {
                text, mime_type, ..
            } => {
                assert_eq!(mime_type.as_deref(), Some("application/json"));
                serde_json::from_str(text).unwrap()
            }
            other => panic!("expected text, got {other:?}"),
        }
    }

    async fn read(c: &Client, uri: &str) -> Result<Value, ServiceError> {
        c.read_resource(ReadResourceRequestParams::new(uri))
            .await
            .map(|r| text_of(&r))
    }

    fn mcp_error(e: ServiceError) -> rmcp::ErrorData {
        match e {
            ServiceError::McpError(d) => d,
            other => panic!("expected an MCP error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn initialize_advertises_resources_and_prompts_and_lists_them() {
        let (b, _dir) = builder();
        let (conn, _) = duplex_connector(b.build());
        let c = mcp_client(conn, "claude-desktop").await;
        let caps = c.peer_info().unwrap().capabilities.clone();
        assert!(caps.resources.is_some() && caps.prompts.is_some() && caps.tools.is_some());
        // No subscribe: the spec does not ask for it.
        assert_ne!(caps.resources.unwrap().subscribe, Some(true));

        let list = c.list_all_resources().await.unwrap();
        let uris: BTreeSet<_> = list.iter().map(|r| r.uri.as_str()).collect();
        assert_eq!(
            uris,
            BTreeSet::from([
                "capturemann://jobs",
                "capturemann://library",
                "capturemann://platforms/youtube/rules"
            ])
        );
        let templates = c.list_resource_templates(None).await.unwrap();
        let t: BTreeSet<_> = templates
            .resource_templates
            .iter()
            .map(|t| t.uri_template.as_str())
            .collect();
        assert_eq!(
            t,
            BTreeSet::from([
                "capturemann://capture/{id}",
                "capturemann://capture/{id}/reports",
                "capturemann://capture/{id}/transcript",
                "capturemann://platforms/{name}/rules"
            ])
        );
        c.cancel().await.unwrap();
    }

    #[tokio::test]
    async fn library_capture_and_rules_are_read_through_the_daemon() {
        let (b, _dir) = builder();
        let (conn, _) = duplex_connector(b.build());
        let c = mcp_client(conn, "claude-desktop").await;
        let shot = call(
            &c,
            "capture_screenshot",
            json!({"target": {"display_id": 1}}),
        )
        .await;
        let id = ok(&shot)["capture_id"].as_str().unwrap().to_owned();

        let lib = read(&c, "capturemann://library").await.unwrap();
        let caps = lib["captures"].as_array().unwrap();
        assert!(caps.iter().any(|x| x["id"] == id.as_str()));

        let cap = read(&c, &format!("capturemann://capture/{id}"))
            .await
            .unwrap();
        assert_eq!(cap["capture_id"], id.as_str());
        assert!(cap["image"].is_null(), "no image in a resource");

        let rules = read(&c, "capturemann://platforms/youtube/rules")
            .await
            .unwrap();
        assert_eq!(rules["title_max_chars"], 100);
        assert!(rules.get("accounts").is_none());

        // This daemon has no speech engine: the daemon's own error comes through with its code.
        let e = mcp_error(
            read(&c, &format!("capturemann://capture/{id}/transcript"))
                .await
                .unwrap_err(),
        );
        assert!(e.data.as_ref().unwrap()["code"].is_string());
        c.cancel().await.unwrap();
    }

    #[tokio::test]
    async fn unknown_uris_and_ids_are_errors() {
        let (b, _dir) = builder();
        let (conn, _) = duplex_connector(b.build());
        let c = mcp_client(conn, "claude-desktop").await;
        for uri in [
            "capturemann://nothing",
            "file:///etc/passwd",
            "capturemann://capture/rec_1/cursor",
            "capturemann://platforms/tiktok/rules",
            "capturemann://capture/no_such_capture",
        ] {
            let e = mcp_error(read(&c, uri).await.unwrap_err());
            assert_eq!(e.code, ErrorCode::RESOURCE_NOT_FOUND, "{uri}");
        }
        let e = mcp_error(read(&c, "capturemann://capture/..%2Fx").await.unwrap_err());
        assert_eq!(e.code, ErrorCode::INVALID_PARAMS);
        // The tool side still works after errors.
        assert_ne!(
            call(&c, "list_targets", json!({})).await.is_error,
            Some(true)
        );
        c.cancel().await.unwrap();
    }

    #[tokio::test]
    async fn per_client_policy_applies_to_resource_reads() {
        let (b, _dir) = builder();
        let d = b.build();
        d.policies().set("viewer", ClientPolicy::ReadOnly);
        d.policies().set("nobody", ClientPolicy::Blocked);
        let (conn, _) = duplex_connector(d);

        // Resources are reads: a read-only client may read them.
        let v = mcp_client(conn.clone(), "viewer").await;
        assert!(read(&v, "capturemann://library").await.is_ok());

        // A blocked client gets the daemon's refusal, not data.
        let n = mcp_client(conn, "nobody").await;
        let e = mcp_error(read(&n, "capturemann://library").await.unwrap_err());
        assert_ne!(e.code, ErrorCode::RESOURCE_NOT_FOUND);
        assert_eq!(e.data.as_ref().unwrap()["data"]["reason"], "client_blocked");
    }

    #[tokio::test]
    async fn jobs_without_a_queue_is_the_daemons_not_implemented() {
        let (b, _dir) = builder();
        let (conn, _) = duplex_connector(b.build());
        let c = mcp_client(conn, "claude-desktop").await;
        let e = mcp_error(read(&c, "capturemann://jobs").await.unwrap_err());
        assert_eq!(e.data.as_ref().unwrap()["code"], "not_implemented");
    }

    #[tokio::test]
    async fn prompts_list_and_get() {
        let (b, _dir) = builder();
        let (conn, _) = duplex_connector(b.build());
        let c = mcp_client(conn, "claude-desktop").await;
        let prompts = c.list_all_prompts().await.unwrap();
        let names: BTreeSet<_> = prompts.iter().map(|p| p.name.as_str()).collect();
        for n in [
            "record-tutorial",
            "publish-everywhere",
            "bug-report-screenshot",
            "lesson-to-drive",
            "caption-cleanup",
            "make-short-clip",
        ] {
            assert!(names.contains(n), "{n}");
        }
        let mut p = GetPromptRequestParams::new("caption-cleanup");
        p = p.with_arguments(
            json!({"capture_id": "rec_9", "glossary": "Kubernetes, CaptureMann"})
                .as_object()
                .unwrap()
                .clone(),
        );
        let r = c.get_prompt(p).await.unwrap();
        let text = serde_json::to_string(&r).unwrap();
        assert!(text.contains("rec_9") && text.contains("replace_in_captions"));
        let e = mcp_error(
            c.get_prompt(GetPromptRequestParams::new("nope"))
                .await
                .unwrap_err(),
        );
        assert_eq!(e.code, ErrorCode::INVALID_PARAMS);
        c.cancel().await.unwrap();
    }
}

/// VOC-1: an agent records audio through MCP: the dialog says "Record audio", a screen-only field
/// is refused with the field's name before any dialog, `add_marker` sets a marker on the running
/// recording and, with `capture_id` and `at`, on the finished one, and `list_library` takes
/// `kind: "audio"`.
#[tokio::test]
async fn voc1_agent_records_audio_and_marks_moments() {
    let (d, confirmer, _dir) =
        recording_daemon(Decision::Approve, cm_daemon::testing::FakeSources::new());
    let (conn, _) = duplex_connector(d);
    let c = mcp_client(conn, "claude-desktop").await;

    let r = call(
        &c,
        "start_recording",
        json!({ "mode": "audio", "countdown_s": 0, "fps": 60 }),
    )
    .await;
    assert_eq!(err_code(&r), "invalid_input");
    assert!(format!("{r:?}").contains("fps"), "{r:?}");
    assert!(confirmer.shown().is_empty(), "refused before any dialog");

    let r = call(
        &c,
        "start_recording",
        json!({ "mode": "audio", "mic": true, "countdown_s": 0, "title": "Interview" }),
    )
    .await;
    let v = ok(&r);
    assert_eq!(v["tracks"], json!(["mic"]));
    assert_eq!(v["mode"], "audio");
    let shown = confirmer.shown();
    assert_eq!(shown.len(), 1);
    assert!(
        shown[0].summary.starts_with("Record audio"),
        "{}",
        shown[0].summary
    );
    assert_eq!(
        shown[0].details["consent"],
        "Let everyone know they're being recorded."
    );

    let m = call(
        &c,
        "add_marker",
        json!({ "label": "Budget", "request_id": "m1" }),
    )
    .await;
    assert_eq!(ok(&m)["marker"]["label"], "Budget");
    assert_eq!(ok(&m)["marker"]["by"], "agent:claude-desktop");
    let replay = call(
        &c,
        "add_marker",
        json!({ "label": "Budget", "request_id": "m1" }),
    )
    .await;
    assert_eq!(
        ok(&replay)["count"],
        1,
        "a repeated request_id sets one marker"
    );

    let stopped = ok(&call(&c, "stop_recording", json!({})).await).clone();
    let id = stopped["capture_id"].as_str().unwrap().to_owned();
    let late = call(
        &c,
        "add_marker",
        json!({ "capture_id": id, "at": 0, "label": "Intro" }),
    )
    .await;
    assert_eq!(ok(&late)["action"], "add_marker");
    let list = call(&c, "list_library", json!({ "kind": "audio" })).await;
    assert_eq!(ok(&list)["captures"][0]["id"], id.as_str());
    c.cancel().await.unwrap();
}
