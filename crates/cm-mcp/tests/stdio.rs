//! The `cm-mcp` binary over real stdio, speaking raw MCP JSON-RPC (AGT-1.AC1: a client
//! configured with the stdio server lists tools with schemas and can call `list_targets`).

#![allow(clippy::unwrap_used)]

use cm_capture::fake::FakeBackend;
use cm_capture::Capturer;
use cm_daemon::{Daemon, Endpoint, IpcServer};
use cm_library::Library;
use serde_json::{json, Value};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

async fn rpc(
    stdin: &mut tokio::process::ChildStdin,
    out: &mut tokio::io::Lines<BufReader<tokio::process::ChildStdout>>,
    id: u64,
    method: &str,
    params: Value,
) -> Value {
    let msg = json!({"jsonrpc":"2.0","id":id,"method":method,"params":params});
    stdin
        .write_all(format!("{msg}\n").as_bytes())
        .await
        .unwrap();
    stdin.flush().await.unwrap();
    loop {
        let line = out
            .next_line()
            .await
            .unwrap()
            .expect("server closed stdout");
        let v: Value = serde_json::from_str(&line).unwrap();
        if v["id"] == json!(id) {
            return v;
        }
    }
}

#[tokio::test]
async fn agt_1_ac1_stdio_binary_lists_tools_and_calls_list_targets() {
    let dir = tempfile::tempdir().unwrap();
    let lib = Library::open_or_create(dir.path()).unwrap();
    let backend = FakeBackend::new().with_display(1, 0, 0, 400, 300, 2.0);
    let daemon = Daemon::builder(Capturer::new(backend).with_own_pid(None), lib).build();
    let sock = dir.path().join("d.sock");
    let endpoint = Endpoint::Path(sock.clone());
    tokio::spawn(IpcServer::bind(&endpoint).unwrap().run(daemon));

    let mut child = Command::new(env!("CARGO_BIN_EXE_cm-mcp"))
        .arg("--socket")
        .arg(&sock)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap()).lines();

    let init = rpc(
        &mut stdin,
        &mut out,
        1,
        "initialize",
        json!({"protocolVersion":"2025-06-18","capabilities":{},
               "clientInfo":{"name":"claude-desktop","version":"1.0"}}),
    )
    .await;
    assert_eq!(
        init["result"]["serverInfo"]["name"], "capturemann",
        "{init}"
    );
    assert!(init["result"]["capabilities"]["tools"].is_object());
    assert!(init["result"]["capabilities"]["resources"].is_object());
    assert!(init["result"]["capabilities"]["prompts"].is_object());
    stdin
        .write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n")
        .await
        .unwrap();

    let tools = rpc(&mut stdin, &mut out, 2, "tools/list", json!({})).await;
    let list = tools["result"]["tools"].as_array().unwrap();
    // 12 + rename_capture + get_job, list_jobs, cancel_job + export_video + pause/resume/stop_recording, set_mic_muted,
    // get_recording_status, list_devices and run_fps_selftest (the HUD's internal actions are not
    // tools), the eleven recording edit tools and the eleven local-AI tools and list_destinations and upload, and suggest_redactions, and replace_in_captions (AI-6a).
    assert_eq!(list.len(), 56);
    assert!(list
        .iter()
        .all(|t| t["inputSchema"].is_object() && t["outputSchema"].is_object()));

    let res = rpc(&mut stdin, &mut out, 20, "resources/list", json!({})).await;
    assert_eq!(res["result"]["resources"].as_array().unwrap().len(), 3);
    let lib = rpc(
        &mut stdin,
        &mut out,
        21,
        "resources/read",
        json!({"uri":"capturemann://library"}),
    )
    .await;
    assert_eq!(lib["result"]["contents"][0]["uri"], "capturemann://library");
    let bad = rpc(
        &mut stdin,
        &mut out,
        22,
        "resources/read",
        json!({"uri":"capturemann://nope"}),
    )
    .await;
    assert_eq!(bad["error"]["code"], -32002, "{bad}");
    let pr = rpc(&mut stdin, &mut out, 23, "prompts/list", json!({})).await;
    assert_eq!(pr["result"]["prompts"].as_array().unwrap().len(), 6);

    let call = rpc(
        &mut stdin,
        &mut out,
        3,
        "tools/call",
        json!({"name":"list_targets","arguments":{}}),
    )
    .await;
    assert_eq!(call["result"]["isError"], false, "{call}");
    assert_eq!(call["result"]["structuredContent"]["displays"][0]["id"], 1);

    let shot = rpc(
        &mut stdin,
        &mut out,
        4,
        "tools/call",
        json!({"name":"capture_screenshot","arguments":{"target":{"display_id":1}}}),
    )
    .await;
    let content = shot["result"]["content"].as_array().unwrap();
    assert_eq!(content[1]["type"], "image");
    assert_eq!(content[1]["mimeType"], "image/png");
}
