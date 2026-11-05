//! The MCP server (`cm-mcp`) as an agent sees it: handshake, tool list against the spec, read-only
//! calls, the Confirm tier, idempotency, error mapping, events, and parity with the CLI.

use super::common::{seed_screenshot, write_clips, SEEDED_SHOT};
use crate::check;
use crate::ctx::Ctx;
use crate::mcp::{McpClient, ToolResult};
use crate::model::{fail, skip, Case, Stop, Tier, ALL, R};
use crate::proc::exit_text;
use crate::spec;
use crate::sys;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

pub fn cases() -> Vec<Case> {
    vec![
        Case {
            id: "mcp.handshake",
            title: "initialize answers with the server name, the product version and tool support",
            area: "mcp",
            specs: &["AGT-2", "MCP protocol"],
            tier: Tier::Smoke,
            needs: &[],
            platforms: ALL,
            timeout_s: 90,
            run: handshake,
        },
        Case {
            id: "mcp.tools_list",
            title: "tools/list matches the tool table of the spec (no drift either way)",
            area: "mcp",
            specs: &["AGT-2", "specs/02-agent-interface.md tool table"],
            tier: Tier::Smoke,
            needs: &[],
            platforms: ALL,
            timeout_s: 90,
            run: tools_list,
        },
        Case {
            id: "mcp.read_only_calls",
            title: "the read-only tools answer without error",
            area: "mcp",
            specs: &["AGT-2"],
            tier: Tier::Standard,
            needs: &[],
            platforms: ALL,
            timeout_s: 120,
            run: read_only_calls,
        },
        Case {
            id: "mcp.confirm_tier",
            title: "start_recording and delete_capture do not run without an on-screen confirmation",
            area: "mcp",
            specs: &["AGT-POLICY", "tier: Confirm"],
            tier: Tier::Standard,
            needs: &[],
            platforms: ALL,
            timeout_s: 120,
            run: confirm_tier,
        },
        Case {
            id: "mcp.policy_allow",
            title: "agents.tool_policy allow for start_recording is honoured by the daemon (CLI and MCP)",
            area: "mcp",
            specs: &["OD-24", "AGT-POLICY"],
            tier: Tier::Standard,
            needs: &[],
            platforms: ALL,
            timeout_s: 150,
            run: policy_allow,
        },
        Case {
            id: "mcp.request_id",
            title: "a repeated request_id returns the first result and acts once",
            area: "mcp",
            specs: &["AGT idempotency"],
            tier: Tier::Standard,
            needs: &[],
            platforms: ALL,
            timeout_s: 120,
            run: request_id,
        },
        Case {
            id: "mcp.errors",
            title: "errors map to structuredContent.code, JSON-RPC codes and CLI exit codes",
            area: "mcp",
            specs: &["specs/README.md shared error codes", "AGT-1"],
            tier: Tier::Standard,
            needs: &[],
            platforms: ALL,
            timeout_s: 120,
            run: errors,
        },
        Case {
            id: "mcp.events",
            title: "daemon events arrive as notifications (capture.revised, job.*) for changes any client makes",
            area: "mcp",
            specs: &["AGT-2 events", "recording events"],
            tier: Tier::Standard,
            needs: &[],
            platforms: ALL,
            timeout_s: 150,
            run: events,
        },
        Case {
            id: "mcp.cli_parity",
            title: "CLI and MCP return the same fields for library list, settings and targets",
            area: "mcp",
            specs: &["AGT-1: every tool has the same name as its CLI subcommand"],
            tier: Tier::Standard,
            needs: &[],
            platforms: ALL,
            timeout_s: 120,
            run: cli_parity,
        },
        Case {
            id: "mcp.http",
            title: "cm-mcp --http needs the bearer token and answers initialize",
            area: "mcp",
            specs: &["AGT-2 transport"],
            tier: Tier::Standard,
            needs: &[],
            platforms: ALL,
            timeout_s: 120,
            run: http,
        },
    ]
}

fn open(ctx: &mut Ctx) -> R<McpClient> {
    let mut c = McpClient::spawn(ctx, &[])?;
    match c.initialize() {
        Ok(_) => Ok(c),
        Err(e) => fail(format!("initialize failed: {e}\n{}", c.transcript())),
    }
}

fn call(ctx: &mut Ctx, c: &mut McpClient, name: &str, args: Value) -> R<ToolResult> {
    let _ = ctx;
    match c.call(name, args) {
        Ok(Ok(r)) => Ok(r),
        Ok(Err((code, msg))) => fail(format!("{name}: JSON-RPC error {code}: {msg}")),
        Err(e) => fail(format!("{name}: {e}\n{}", c.transcript())),
    }
}

fn handshake(ctx: &mut Ctx) -> R {
    ctx.start_daemon()?;
    let mut c = McpClient::spawn(ctx, &[])?;
    let init = c
        .initialize()
        .map_err(|e| Stop::Fail(format!("{e}\n{}", c.transcript())))?;
    let name = init
        .pointer("/serverInfo/name")
        .and_then(Value::as_str)
        .unwrap_or("");
    let version = init
        .pointer("/serverInfo/version")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    ctx.note(format!(
        "server {name} {version}, protocol {}",
        init.get("protocolVersion").unwrap_or(&Value::Null)
    ));
    check!(name == "capturemann", "serverInfo.name is '{name}'");
    check!(
        init.pointer("/capabilities/tools").is_some(),
        "the server does not declare tools"
    );
    check!(
        init.get("protocolVersion")
            .and_then(Value::as_str)
            .is_some_and(|p| !p.is_empty()),
        "no protocolVersion"
    );
    check!(
        init.get("instructions")
            .and_then(Value::as_str)
            .is_some_and(|i| i.len() > 100),
        "no usage instructions"
    );
    let cli = ctx.cli_text(&["--version"]).ok();
    drop(cli);
    let mut v = ctx.product_cmd("capturemann");
    v.arg("--version");
    let out = ctx.run(v, 20)?;
    let cli_version = out
        .stdout
        .trim()
        .trim_start_matches("capturemann")
        .trim()
        .to_owned();
    check!(
        version == cli_version,
        "cm-mcp reports version {version} but capturemann is {cli_version}"
    );
    c.close();
    Ok(())
}

fn tools_list(ctx: &mut Ctx) -> R {
    ctx.start_daemon()?;
    let mut c = open(ctx)?;
    let tools = c.list_tools().map_err(Stop::Fail)?;
    let names: Vec<String> = tools
        .iter()
        .filter_map(|t| t.get("name").and_then(Value::as_str).map(str::to_owned))
        .collect();
    ctx.note(format!("{} tools", names.len()));
    check!(names.len() >= 60, "only {} tools listed", names.len());
    let mut uniq = names.clone();
    uniq.sort();
    uniq.dedup();
    check!(uniq.len() == names.len(), "a tool name is listed twice");
    let mut problems = Vec::new();
    for t in &tools {
        let n = t.get("name").and_then(Value::as_str).unwrap_or("?");
        if t.get("description")
            .and_then(Value::as_str)
            .is_none_or(|d| d.trim().is_empty())
        {
            problems.push(format!("{n}: no description"));
        }
        if t.pointer("/inputSchema/type").and_then(Value::as_str) != Some("object") {
            problems.push(format!("{n}: inputSchema is not an object schema"));
        }
    }
    let table = spec::parse_tool_table(spec::SPEC_TEXT);
    let (unlisted, mut missing) = spec::drift(&table, &names);
    // Rows of the spec table that no build has ever registered (found Nov 5, 2026). The case reports them
    // every run and fails on any NEW difference; remove a name here when the spec or the server is fixed.
    const KNOWN_SPEC_ONLY: &[&str] = &["export_image"];
    let known: Vec<String> = missing
        .iter()
        .filter(|m| KNOWN_SPEC_ONLY.contains(&m.as_str()))
        .cloned()
        .collect();
    missing.retain(|m| !KNOWN_SPEC_ONLY.contains(&m.as_str()));
    if !known.is_empty() {
        ctx.note(format!(
            "known drift (spec lists, server lacks): {}",
            known.join(", ")
        ));
    }
    for k in KNOWN_SPEC_ONLY {
        if names.iter().any(|n| n == k) {
            problems.push(format!("{k} is now served: remove it from KNOWN_SPEC_ONLY"));
        }
    }
    if !unlisted.is_empty() {
        problems.push(format!(
            "tools the spec table does not list: {}",
            unlisted.join(", ")
        ));
    }
    if !missing.is_empty() {
        problems.push(format!(
            "tools the spec lists as built but the server lacks: {}",
            missing.join(", ")
        ));
    }
    c.close();
    if problems.is_empty() {
        Ok(())
    } else {
        fail(problems.join("; "))
    }
}

fn read_only_calls(ctx: &mut Ctx) -> R {
    seed_screenshot(ctx)?;
    ctx.start_daemon()?;
    let mut c = open(ctx)?;
    let mut calls: Vec<(&str, Value)> = vec![
        ("get_settings", json!({})),
        ("list_library", json!({})),
        (
            "get_capture",
            json!({"capture_id": SEEDED_SHOT, "include_image": false}),
        ),
        ("get_diagnostics", json!({})),
        ("list_models", json!({})),
        ("get_text_model", json!({})),
        ("list_jobs", json!({})),
        ("list_destinations", json!({})),
        ("list_report_templates", json!({})),
        ("list_text_models", json!({})),
    ];
    if sys::has_desktop() {
        calls.push(("list_targets", json!({"windows": false})));
        calls.push(("list_devices", json!({})));
    }
    let mut problems = Vec::new();
    for (name, args) in calls {
        match c.call(name, args) {
            Ok(Ok(r)) if !r.is_error => ctx.note(format!("{name}: ok, {} chars", r.text.len())),
            Ok(Ok(r)) => problems.push(format!(
                "{name}: {} {}",
                r.error_code().unwrap_or_default(),
                crate::util::cut(&r.text, 120)
            )),
            Ok(Err((code, msg))) => problems.push(format!("{name}: JSON-RPC {code} {msg}")),
            Err(e) => {
                problems.push(format!("{name}: {e}"));
                break;
            }
        }
    }
    c.close();
    if problems.is_empty() {
        Ok(())
    } else {
        fail(problems.join("; "))
    }
}

fn confirm_tier(ctx: &mut Ctx) -> R {
    if sys::PLATFORM == "macos" && !sys::has_desktop() {
        // The daemon checks the target before it asks, and a locked Mac lists no display.
        return skip(
            "the Mac screen is locked or off: no display exists to name in start_recording",
        );
    }
    seed_screenshot(ctx)?;
    ctx.start_daemon()?;
    let mut c = open(ctx)?;
    // A real display id when there is a desktop: the call may validate the target before it asks.
    let display_id = if sys::has_desktop() {
        ctx.primary_display().map_or(1, |d| d.id)
    } else {
        1
    };
    let r = call(
        ctx,
        &mut c,
        "start_recording",
        json!({"target": {"display_id": display_id}}),
    )?;
    ctx.note(format!(
        "start_recording: error={} code={:?} {}",
        r.is_error,
        r.error_code(),
        crate::util::cut(&r.text, 140)
    ));
    check!(
        r.is_error && r.error_code().as_deref() == Some("user_declined"),
        "start_recording without a confirmation gave {:?} (is_error {})",
        r.error_code(),
        r.is_error
    );
    let s = call(ctx, &mut c, "get_recording_status", json!({}))?;
    let state = s
        .json()
        .and_then(|j| j.get("state").and_then(Value::as_str).map(str::to_owned))
        .unwrap_or_default();
    ctx.note(format!(
        "get_recording_status after the refusal: state '{state}'"
    ));
    check!(
        !matches!(
            state.as_str(),
            "recording" | "starting" | "countdown" | "paused"
        ),
        "a recording state '{state}' exists after a refused start"
    );
    let d = call(
        ctx,
        &mut c,
        "delete_capture",
        json!({"capture_id": SEEDED_SHOT}),
    )?;
    check!(
        d.is_error && d.error_code().as_deref() == Some("user_declined"),
        "delete_capture without a confirmation gave {:?}",
        d.error_code()
    );
    let list = call(ctx, &mut c, "list_library", json!({}))?;
    check!(
        list.text.contains(SEEDED_SHOT),
        "the capture is gone after a refused delete"
    );
    c.close();
    Ok(())
}

fn policy_allow(ctx: &mut Ctx) -> R {
    // With the user's own setting (written to the config folder, which an agent cannot do), the daemon
    // stops asking: the call proceeds to validation and fails on the bogus display instead.
    ctx.allow_recording_without_dialog()?;
    ctx.start_daemon()?;
    let r = ctx.cli(&["record", "start", "--display", "987654", "--countdown", "0"])?;
    ctx.note(format!(
        "CLI with policy allow: {} {:?}",
        exit_text(r.code()),
        r.error_code()
    ));
    check!(r.error_code().as_deref() == Some("not_found"), "with the policy set to allow, the CLI start of a bogus display gave {:?} (expected not_found, i.e. no confirmation asked)", r.error_code());
    let mut c = open(ctx)?;
    let m = call(
        ctx,
        &mut c,
        "start_recording",
        json!({"target": {"display_id": 987_654}}),
    )?;
    ctx.note(format!("MCP with policy allow: {:?}", m.error_code()));
    check!(
        m.error_code().as_deref() == Some("not_found"),
        "MCP start_recording gave {:?}, expected not_found",
        m.error_code()
    );
    c.close();
    Ok(())
}

fn request_id(ctx: &mut Ctx) -> R {
    seed_screenshot(ctx)?;
    ctx.start_daemon()?;
    let mut c = open(ctx)?;
    let a = call(
        ctx,
        &mut c,
        "rename_capture",
        json!({"capture_id": SEEDED_SHOT, "title": "First", "request_id": "regress-1"}),
    )?;
    check!(!a.is_error, "first rename failed: {}", a.text);
    let b = call(
        ctx,
        &mut c,
        "rename_capture",
        json!({"capture_id": SEEDED_SHOT, "title": "Second", "request_id": "regress-1"}),
    )?;
    check!(!b.is_error, "replayed rename failed: {}", b.text);
    let title = b
        .json()
        .and_then(|j| j.get("title").and_then(Value::as_str).map(str::to_owned))
        .unwrap_or_default();
    check!(
        title == "First",
        "the repeated request_id acted again (title '{title}')"
    );
    let list = call(ctx, &mut c, "list_library", json!({}))?;
    check!(
        list.text.contains("\"First\""),
        "the library does not show the first title"
    );
    check!(
        !list.text.contains("\"Second\""),
        "the second title reached the library"
    );
    // A new id does act.
    let d = call(
        ctx,
        &mut c,
        "rename_capture",
        json!({"capture_id": SEEDED_SHOT, "title": "Third", "request_id": "regress-2"}),
    )?;
    check!(
        d.json()
            .and_then(|j| j.get("title").and_then(Value::as_str).map(str::to_owned))
            .as_deref()
            == Some("Third"),
        "a new request_id did not act"
    );
    c.close();
    Ok(())
}

fn errors(ctx: &mut Ctx) -> R {
    seed_screenshot(ctx)?;
    ctx.start_daemon()?;
    let mut c = open(ctx)?;
    let nf = call(
        ctx,
        &mut c,
        "get_capture",
        json!({"capture_id": "shot_NOPE"}),
    )?;
    check!(
        nf.is_error && nf.error_code().as_deref() == Some("not_found"),
        "unknown capture: {:?}",
        nf.error_code()
    );
    let cli = ctx.cli(&["capture", "get", "shot_NOPE"])?;
    check!(
        cli.code() == Some(2) && cli.error_code().as_deref() == Some("not_found"),
        "CLI unknown capture: {} {:?}",
        exit_text(cli.code()),
        cli.error_code()
    );
    // Missing required argument: a tool error or a JSON-RPC invalid params error, never success.
    match c
        .call("rename_capture", json!({"capture_id": SEEDED_SHOT}))
        .map_err(Stop::Fail)?
    {
        Ok(r) => check!(r.is_error, "rename_capture without a title succeeded"),
        Err((code, _)) => check!(
            code == -32602,
            "missing argument gave JSON-RPC {code}, expected -32602"
        ),
    }
    match c.call("no_such_tool", json!({})).map_err(Stop::Fail)? {
        Ok(r) => check!(r.is_error, "an unknown tool succeeded"),
        Err((code, _)) => check!(
            code == -32602 || code == -32601,
            "unknown tool gave JSON-RPC {code}"
        ),
    }
    let t = Duration::from_secs(10);
    let r = c
        .request("no/such/method", json!({}), t)
        .map_err(Stop::Fail)?;
    check!(
        r.error.as_ref().is_some_and(|e| e.0 == -32601),
        "unknown method gave {:?}, expected -32601",
        r.error
    );
    c.close();
    Ok(())
}

fn events(ctx: &mut Ctx) -> R {
    seed_screenshot(ctx)?;
    let clips = write_clips(ctx)?;
    ctx.start_daemon()?;
    let mut c = open(ctx)?;
    // Another client (the CLI) changes the library; this client is told.
    ctx.cli_ok(&["library", "rename", SEEDED_SHOT, "Seen by events"])?;
    let notes = c.drain_notifications(Duration::from_secs(5)).to_vec();
    let revised = notes.iter().find(|n| {
        n.pointer("/params/logger").and_then(Value::as_str) == Some("capturemann.events")
            && n.pointer("/params/data/event").and_then(Value::as_str) == Some("capture.revised")
            && n.pointer("/params/data/data/capture_id")
                .and_then(Value::as_str)
                == Some(SEEDED_SHOT)
    });
    ctx.note(format!(
        "{} notification(s); capture.revised seen: {}",
        notes.len(),
        revised.is_some()
    ));
    check!(
        revised.is_some(),
        "no capture.revised event after a rename by another client; got {:?}",
        notes
            .iter()
            .filter_map(|n| n.pointer("/params/data/event").and_then(Value::as_str))
            .collect::<Vec<_>>()
    );
    // A job: import through the tool and watch job events.
    let path = clips.join("clip-h264.mp4").to_string_lossy().into_owned();
    let r = call(ctx, &mut c, "import_media", json!({"paths": [path]}))?;
    check!(!r.is_error, "import_media failed: {}", r.text);
    let deadline = std::time::Instant::now() + ctx.secs(90);
    let mut job_events: Vec<String> = Vec::new();
    while std::time::Instant::now() < deadline {
        let notes = c.drain_notifications(Duration::from_secs(2)).to_vec();
        job_events = notes
            .iter()
            .filter_map(|n| n.pointer("/params/data/event").and_then(Value::as_str))
            .filter(|e| e.starts_with("job.") || e.starts_with("capture."))
            .map(str::to_owned)
            .collect();
        if job_events
            .iter()
            .any(|e| e == "job.done" || e == "job.failed")
        {
            break;
        }
    }
    ctx.note(format!("events after import: {}", job_events.join(", ")));
    check!(
        job_events.iter().any(|e| e == "job.done"),
        "no job.done event after import_media; events: {job_events:?}"
    );
    check!(
        job_events.iter().any(|e| e == "capture.created"),
        "no capture.created event after the import; events: {job_events:?}"
    );
    c.close();
    Ok(())
}

fn ids(v: &Value, key: &str) -> Vec<String> {
    v.get(key)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|c| c.get("id").and_then(Value::as_str).map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn keys(v: &Value) -> Vec<String> {
    v.as_object()
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default()
}

fn cli_parity(ctx: &mut Ctx) -> R {
    seed_screenshot(ctx)?;
    ctx.start_daemon()?;
    let mut c = open(ctx)?;
    let cli = ctx.cli_ok(&["library", "list"])?;
    let mcp = call(ctx, &mut c, "list_library", json!({}))?
        .json()
        .unwrap_or(Value::Null);
    check!(
        ids(&cli, "captures") == ids(&mcp, "captures"),
        "library ids differ: CLI {:?}, MCP {:?}",
        ids(&cli, "captures"),
        ids(&mcp, "captures")
    );
    let (a, b) = (cli["captures"][0].clone(), mcp["captures"][0].clone());
    check!(
        keys(&a) == keys(&b),
        "library entry fields differ: CLI {:?}, MCP {:?}",
        keys(&a),
        keys(&b)
    );
    ctx.cli_ok(&["settings", "set", "recording.fps", "60"])?;
    let cli = ctx.cli_ok(&["settings", "get", "recording.fps"])?;
    let mcp = call(
        ctx,
        &mut c,
        "get_settings",
        json!({"keys": ["recording.fps"]}),
    )?
    .json()
    .unwrap_or(Value::Null);
    check!(
        cli.pointer("/settings/recording.fps") == mcp.pointer("/settings/recording.fps"),
        "settings differ: CLI {cli}, MCP {mcp}"
    );
    if sys::has_desktop() {
        let cli = ctx.cli_ok(&["targets", "--displays-only"])?;
        let mcp = call(ctx, &mut c, "list_targets", json!({"windows": false}))?
            .json()
            .unwrap_or(Value::Null);
        check!(
            ids(&cli, "displays") == ids(&mcp, "displays"),
            "display ids differ between CLI and MCP"
        );
        check!(
            cli.pointer("/displays/0/bounds") == mcp.pointer("/displays/0/bounds"),
            "display bounds differ between CLI and MCP"
        );
    }
    c.close();
    Ok(())
}

/// One HTTP/1.1 request on a fresh connection; returns (status code, body text).
fn http_post(port: u16, token: Option<&str>, body: &str) -> Result<(u16, String), String> {
    let mut s = TcpStream::connect(("127.0.0.1", port)).map_err(|e| format!("connect: {e}"))?;
    s.set_read_timeout(Some(Duration::from_secs(8))).ok();
    let auth = token
        .map(|t| format!("Authorization: Bearer {t}\r\n"))
        .unwrap_or_default();
    let req = format!(
        "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n{auth}Content-Type: application/json\r\nAccept: application/json, text/event-stream\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    );
    s.write_all(req.as_bytes())
        .map_err(|e| format!("send: {e}"))?;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    // The server may keep an event stream open: stop at the first complete JSON-RPC answer or the timeout.
    loop {
        match s.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                let t = String::from_utf8_lossy(&buf);
                if t.contains("\"result\"")
                    || t.contains("\"error\"")
                    || t.starts_with("HTTP/1.1 401")
                {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    let text = String::from_utf8_lossy(&buf).into_owned();
    let status = text
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| format!("no HTTP status in: {}", crate::util::cut(&text, 120)))?;
    Ok((status, text))
}

fn http(ctx: &mut Ctx) -> R {
    ctx.start_daemon()?;
    let err_path = ctx.root.join("mcp-http.err.txt");
    let err = std::fs::File::create(&err_path)?;
    let ep = ctx.endpoint.clone();
    let mut cmd = ctx.product_cmd("cm-mcp");
    cmd.args(["--http", "--socket", &ep])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(err);
    let mut child = cmd
        .spawn()
        .map_err(|e| Stop::Fail(format!("cm-mcp --http did not start: {e}")))?;
    if let Ok(mut t) = ctx.tracker.lock() {
        t.push(child.id());
    }
    let info = crate::proc::wait_for(ctx.secs(30), Duration::from_millis(200), || {
        let t = std::fs::read_to_string(&err_path).ok()?;
        let port = t
            .lines()
            .find_map(|l| l.split("127.0.0.1:").nth(1))?
            .split('/')
            .next()?
            .parse::<u16>()
            .ok()?;
        let token = t
            .lines()
            .find_map(|l| l.strip_prefix("bearer token: "))?
            .trim()
            .to_owned();
        Some((port, token))
    });
    let result = (|| -> R {
        let Some((port, token)) = info else {
            return fail(format!(
                "cm-mcp --http printed no address and token: {}",
                crate::util::cut(&std::fs::read_to_string(&err_path).unwrap_or_default(), 200)
            ));
        };
        let body = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "cm-regress", "version": "0"}}}).to_string();
        let (no_token, _) = http_post(port, None, &body).map_err(Stop::Fail)?;
        let (bad_token, _) = http_post(port, Some("not-the-token"), &body).map_err(Stop::Fail)?;
        let (ok, text) = http_post(port, Some(&token), &body).map_err(Stop::Fail)?;
        ctx.note(format!(
            "port {port}: no token {no_token}, wrong token {bad_token}, right token {ok}"
        ));
        check!(
            no_token == 401 || no_token == 403,
            "a request without the token got HTTP {no_token}"
        );
        check!(
            bad_token == 401 || bad_token == 403,
            "a request with a wrong token got HTTP {bad_token}"
        );
        check!(
            ok == 200,
            "initialize with the token got HTTP {ok}: {}",
            crate::util::cut(&text, 200)
        );
        check!(
            text.contains("capturemann"),
            "the initialize answer does not name the server"
        );
        Ok(())
    })();
    sys::kill_tree(child.id());
    let _ = child.kill();
    let _ = child.wait();
    if let Ok(mut t) = ctx.tracker.lock() {
        t.retain(|p| *p != child.id());
    }
    result
}
