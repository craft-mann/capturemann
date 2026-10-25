//! Subcommand implementations: build the JSON-RPC params, call the daemon, print the result.

use std::fmt::Write as _;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use base64::Engine as _;
use cm_core::ErrorCode;
use cm_ipc::Endpoint;
use serde_json::{json, Map, Value};

mod ai;

use crate::args::{
    AnnotateArgs, CaptureCmd, Cli, Command, DaemonCmd, EditCmd, ExportArgs, GetArgs, JobsCmd,
    LibraryCmd, LibraryListArgs, LibraryPaging, MoveArgs, PublishArgs, PublishCmd, PublishSendArgs,
    RecordCmd, RecordMicArgs, RecordRefArgs, RecordSelftestArgs, RecordStartArgs, RevisionArgs,
    ScreenshotArgs, SettingsCmd, UploadArgs,
};
use crate::client::{describe, Client};
use crate::error::CliError;
use crate::launch::process_launcher;
use crate::Env;

struct Ctx<'a> {
    json: bool,
    request_id: Option<String>,
    env: &'a mut Env,
    endpoint: Endpoint,
    no_start: bool,
}

/// Runs `cm-mcp` with this process's stdio and returns when it exits. `--socket` is forwarded
/// only when given, so the server otherwise resolves the endpoint itself.
fn mcp(cx: &Ctx<'_>, http: bool, port: Option<u16>) -> Result<(), CliError> {
    let bin = crate::launch::mcp_binary();
    let mut cmd = std::process::Command::new(&bin);
    if http {
        cmd.arg("--http");
    }
    if let Some(port) = port {
        cmd.arg("--port").arg(port.to_string());
    }
    if cx.endpoint != Endpoint::default_for_platform() {
        cmd.arg("--socket").arg(match &cx.endpoint {
            Endpoint::Path(p) => p.as_os_str().to_owned(),
            Endpoint::Pipe(n) => n.into(),
        });
    }
    let status = cmd.status().map_err(|e| {
        CliError::local(
            "mcp_unavailable",
            format!(
                "cannot run `{}`: {e} (set CAPTUREMANN_MCP to the cm-mcp binary)",
                bin.display()
            ),
        )
    })?;
    if status.success() {
        Ok(())
    } else {
        Err(CliError::local(
            "mcp_failed",
            format!("cm-mcp exited with {status}"),
        ))
    }
}

pub async fn execute(cli: Cli, env: &mut Env) -> Result<(), CliError> {
    let endpoint = match &cli.socket {
        None => env.endpoint.clone(),
        Some(s) if cfg!(windows) => Endpoint::Pipe(s.clone()),
        Some(s) => Endpoint::Path(s.into()),
    };
    let mut cx = Ctx {
        json: cli.json,
        request_id: cli.request_id,
        env,
        endpoint,
        no_start: cli.no_start,
    };
    match cli.command {
        Command::Targets { displays_only, .. } => targets(&mut cx, displays_only).await,
        Command::Capture { cmd } => match cmd {
            CaptureCmd::Screenshot(a) => screenshot(&mut cx, a).await,
            CaptureCmd::Get(a) => get_capture(&mut cx, a).await,
        },
        Command::Annotate(a) => annotate(&mut cx, a).await,
        Command::Undo(a) => undo_redo(&mut cx, a, true).await,
        Command::Redo(a) => undo_redo(&mut cx, a, false).await,
        Command::Export(a) => export_video(&mut cx, a).await,
        Command::Library { cmd } => match cmd {
            LibraryCmd::List(a) => library_list(&mut cx, a).await,
            LibraryCmd::Rename { id, title } => library_rename(&mut cx, id, title).await,
            LibraryCmd::Rm { id, also_remote } => library_rm(&mut cx, id, also_remote).await,
            LibraryCmd::Move(a) => library_move(&mut cx, a).await,
            LibraryCmd::Search { text, paging } => {
                library_list(
                    &mut cx,
                    LibraryListArgs {
                        query: Some(text),
                        paging,
                    },
                )
                .await
            }
        },
        Command::Settings { cmd } => match cmd {
            SettingsCmd::Get { keys } => settings_get(&mut cx, keys).await,
            SettingsCmd::Set { pairs } => settings_set(&mut cx, pairs).await,
        },
        Command::Record { cmd } => match cmd {
            RecordCmd::Start(a) => record_start(&mut cx, *a).await,
            RecordCmd::Pause(a) => record_control(&mut cx, "pause_recording", a).await,
            RecordCmd::Resume(a) => record_control(&mut cx, "resume_recording", a).await,
            RecordCmd::Mic(a) => record_mic(&mut cx, a).await,
            RecordCmd::Stop(a) => record_control(&mut cx, "stop_recording", a).await,
            RecordCmd::Status(a) => record_status(&mut cx, a).await,
            RecordCmd::Selftest(a) => record_selftest(&mut cx, a).await,
            RecordCmd::Watch { topics, max_events } => {
                record_watch(&mut cx, topics, max_events).await
            }
        },
        Command::Devices => devices(&mut cx).await,
        Command::Destinations => destinations(&mut cx).await,
        Command::Upload(a) => upload(&mut cx, a).await,
        Command::Publish(a) => publish(&mut cx, a).await,
        Command::Mcp { http, port } => mcp(&cx, http, port),
        Command::Edit { cmd } => edit(&mut cx, cmd).await,
        Command::Ai { cmd } => ai::ai(&mut cx, cmd).await,
        Command::Daemon { cmd } => match cmd {
            DaemonCmd::Start { library } => daemon_start(&mut cx, library).await,
            DaemonCmd::Status => daemon_status(&mut cx).await,
        },
        Command::Jobs { cmd } => match cmd {
            JobsCmd::Get { id } => jobs_get(&mut cx, id).await,
            JobsCmd::List {
                state,
                job_type,
                limit,
            } => jobs_list(&mut cx, state, job_type, limit).await,
            JobsCmd::Cancel { id } => jobs_cancel(&mut cx, id).await,
            JobsCmd::Retry { id } => jobs_retry(&mut cx, id).await,
            JobsCmd::Watch { topics, max_events } => jobs_watch(&mut cx, topics, max_events).await,
        },
    }
}

impl Ctx<'_> {
    async fn client(&self) -> Result<Client, CliError> {
        let launcher = if self.no_start {
            None
        } else {
            self.env.launcher.as_ref()
        };
        Client::connect_or_start(&self.endpoint, launcher).await
    }

    fn print(&mut self, text: &str) {
        let _ = writeln!(self.env.out, "{text}");
    }

    fn print_json(&mut self, v: &Value) {
        let s = serde_json::to_string_pretty(v).unwrap_or_default();
        self.print(&s);
    }

    /// Prints `v` as JSON or via `human`.
    fn emit(&mut self, v: &Value, human: impl FnOnce(&Value) -> String) {
        if self.json {
            self.print_json(v);
        } else {
            let t = human(v);
            self.print(t.trim_end());
        }
    }

    /// `params` plus `request_id` when `--request-id` was given (mutating actions only).
    fn mutating(&self, mut params: Value, suffix: &str) -> Value {
        if let (Some(id), Some(o)) = (&self.request_id, params.as_object_mut()) {
            o.insert("request_id".into(), json!(format!("{id}{suffix}")));
        }
        params
    }
}

// ---- helpers ---------------------------------------------------------------------------

/// Replaces `image.data` (base64) by `image.bytes`, after optionally writing the PNG to `out`.
/// Returns the size written.
fn take_image(result: &mut Value, out: Option<&Path>) -> Result<(), CliError> {
    let Some(image) = result.get_mut("image").filter(|i| i.is_object()) else {
        if let Some(p) = out {
            return Err(CliError::new(
                ErrorCode::Internal,
                format!("the daemon returned no image to write to {}", p.display()),
            ));
        }
        return Ok(());
    };
    let data = image["data"].as_str().unwrap_or_default();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|e| {
            CliError::new(
                ErrorCode::Internal,
                format!("bad image data from the daemon: {e}"),
            )
        })?;
    if let Some(p) = out {
        std::fs::write(p, &bytes).map_err(|e| {
            CliError::new(
                ErrorCode::Internal,
                format!("cannot write `{}`: {e}", p.display()),
            )
        })?;
        result["out"] = json!(p);
    }
    if let Some(o) = result["image"].as_object_mut() {
        o.remove("data");
        o.insert("bytes".into(), json!(bytes.len()));
    }
    Ok(())
}

/// Reads annotation ops: `-` is stdin, text starting with `[` or `{` is inline JSON, anything
/// else is a file. Accepts an array or `{"ops": [...]}` (ANN-1, `--annotate ops.json`).
fn load_ops(spec: &str, flag: &str, stdin: &mut dyn Read) -> Result<Value, CliError> {
    let text = if spec == "-" {
        let mut s = String::new();
        stdin
            .read_to_string(&mut s)
            .map_err(|e| CliError::invalid_input(format!("{flag}: cannot read stdin: {e}")))?;
        s
    } else if spec.trim_start().starts_with(['[', '{']) {
        spec.to_owned()
    } else {
        std::fs::read_to_string(spec)
            .map_err(|e| CliError::invalid_input(format!("{flag}: cannot read `{spec}`: {e}")))?
    };
    let v: Value = serde_json::from_str(&text)
        .map_err(|e| CliError::invalid_input(format!("{flag}: invalid JSON: {e}")))?;
    let ops = match v {
        Value::Object(mut o) if o.contains_key("ops") => o.remove("ops").unwrap_or_default(),
        other => other,
    };
    if !ops.is_array() {
        return Err(CliError::invalid_input(format!(
            "{flag}: expected a JSON array of ops (or an object with an `ops` array)"
        )));
    }
    Ok(ops)
}

fn parse_region(s: &str) -> Result<[i64; 4], CliError> {
    let bad = || CliError::invalid_input("--region: expected X,Y,W,H as whole physical px");
    let parts: Vec<i64> = s
        .split(',')
        .map(|p| p.trim().parse::<i64>())
        .collect::<Result<_, _>>()
        .map_err(|_| bad())?;
    let [x, y, w, h] = parts[..] else {
        return Err(bad());
    };
    if w <= 0 || h <= 0 {
        return Err(CliError::invalid_input(
            "--region: width and height must be positive",
        ));
    }
    Ok([x, y, w, h])
}

/// Window id, or the front-most non-minimized window whose title or app contains `needle`
/// (case-insensitive).
fn resolve_window(targets: &Value, needle: &str) -> Result<u64, CliError> {
    let windows = targets["windows"].as_array().cloned().unwrap_or_default();
    if let Ok(id) = needle.parse::<u64>() {
        if windows.iter().any(|w| w["id"].as_u64() == Some(id)) {
            return Ok(id);
        }
    }
    let lower = needle.to_lowercase();
    let hit = |w: &&Value| {
        w["title"]
            .as_str()
            .unwrap_or("")
            .to_lowercase()
            .contains(&lower)
            || w["app"]
                .as_str()
                .unwrap_or("")
                .to_lowercase()
                .contains(&lower)
    };
    let matches: Vec<&Value> = windows.iter().filter(hit).collect();
    matches
        .iter()
        .find(|w| w["minimized"] != json!(true))
        .or(matches.first())
        .and_then(|w| w["id"].as_u64())
        .ok_or_else(|| {
            CliError::not_found(format!(
                "no window matches `{needle}` (run `capturemann targets` to list windows)"
            ))
        })
}

fn primary_display(targets: &Value) -> Result<u64, CliError> {
    let displays = targets["displays"].as_array().cloned().unwrap_or_default();
    displays
        .iter()
        .find(|d| d["is_primary"] == json!(true))
        .or(displays.first())
        .and_then(|d| d["id"].as_u64())
        .ok_or_else(|| CliError::not_found("no display is available to capture"))
}

/// `WxH at (x, y)` of a physical rect as the daemon serializes it: `origin` is `{x, y}` and
/// `size` is `[width, height]` (an object `{width, height}` is accepted too). Anything
/// missing prints `?`, never `null`.
fn rect_text(b: &Value) -> String {
    let num = |v: &Value| v.as_i64().map_or_else(|| "?".to_owned(), |n| n.to_string());
    let (w, h) = match &b["size"] {
        Value::Array(a) => (
            a.first().map_or_else(|| "?".to_owned(), num),
            a.get(1).map_or_else(|| "?".to_owned(), num),
        ),
        other => (num(&other["width"]), num(&other["height"])),
    };
    format!(
        "{w}x{h} at ({}, {})",
        num(&b["origin"]["x"]),
        num(&b["origin"]["y"])
    )
}

fn plain(v: &Value) -> String {
    v.as_str().map_or_else(|| v.to_string(), str::to_owned)
}

// ---- commands --------------------------------------------------------------------------

async fn targets(cx: &mut Ctx<'_>, displays_only: bool) -> Result<(), CliError> {
    let mut c = cx.client().await?;
    let params = if displays_only {
        json!({ "windows": false })
    } else {
        json!({})
    };
    let r = c.call("list_targets", params).await?;
    cx.emit(&r, |r| {
        let mut s = String::from("Displays\n");
        for d in r["displays"].as_array().into_iter().flatten() {
            let _ = writeln!(
                s,
                "  {}  {}{}  {}  scale {}{}",
                d["id"],
                plain(&d["name"]),
                if d["is_builtin"] == json!(true) {
                    " (built-in)"
                } else {
                    ""
                },
                rect_text(&d["bounds"]),
                d["scale"],
                if d["is_primary"] == json!(true) {
                    "  (primary)"
                } else {
                    ""
                }
            );
        }
        s.push_str("Windows\n");
        for w in r["windows"].as_array().into_iter().flatten() {
            let _ = writeln!(
                s,
                "  {}  {} - {}  {}{}",
                w["id"],
                plain(&w["app"]),
                plain(&w["title"]),
                rect_text(&w["bounds"]),
                if w["minimized"] == json!(true) {
                    "  (minimized)"
                } else {
                    ""
                }
            );
        }
        s
    });
    Ok(())
}

async fn screenshot(cx: &mut Ctx<'_>, a: ScreenshotArgs) -> Result<(), CliError> {
    // Validate local inputs before touching the screen.
    let region = a.region.as_deref().map(parse_region).transpose()?;
    let ops = match &a.annotate {
        Some(spec) => Some(load_ops(spec, "--annotate", &mut cx.env.stdin)?),
        None => None,
    };
    let mut c = cx.client().await?;
    let target = if let Some(w) = &a.window {
        let t = c.call("list_targets", json!({})).await?;
        json!({ "window_id": resolve_window(&t, w)? })
    } else {
        let display = match a.display {
            Some(d) => u64::from(d),
            None => primary_display(&c.call("list_targets", json!({ "windows": false })).await?)?,
        };
        match region {
            Some(rect) => json!({ "display_id": display, "rect": rect }),
            None => json!({ "display_id": display }),
        }
    };
    // R1-1: with `--annotate` the clipboard must get the annotated (and redacted) image, never
    // the original, so the copy is requested with the `annotate` call below.
    let copy_with_capture = a.copy && ops.is_none();
    let mut params = json!({ "target": target, "copy_to_clipboard": copy_with_capture });
    if let Some(d) = a.delay {
        params["delay_s"] = json!(d);
    }
    if let Some(t) = &a.title {
        params["title"] = json!(t);
    }
    let params = cx.mutating(params, "");
    let mut result = c.call("capture_screenshot", params).await?;
    if let Some(ops) = ops {
        let id = plain(&result["capture_id"]);
        let params = cx.mutating(
            json!({ "capture_id": id, "ops": ops, "copy_to_clipboard": a.copy }),
            ":annotate",
        );
        let ann = c.call("annotate", params).await.map_err(|mut e| {
            e.message = format!("{} (the screenshot was kept as {id})", e.message);
            e
        })?;
        result["revision"] = ann["revision"].clone();
        result["image"] = ann["image"].clone();
        // Report what really happened to the clipboard: the annotated image, or a warning.
        result["copied_to_clipboard"] = json!(ann["copied_to_clipboard"] == json!(true));
        if let (Some(w), Some(list)) = (
            ann["warnings"].as_array(),
            result["warnings"].as_array_mut(),
        ) {
            list.extend(w.iter().cloned());
        }
    }
    take_image(&mut result, a.out.as_deref())?;
    cx.emit(&result, |r| {
        let mut s = format!(
            "Captured {} ({}x{} px, {})\n  library: {}\n",
            plain(&r["capture_id"]),
            r["width"],
            r["height"],
            plain(&r["kind"]),
            plain(&r["path"])
        );
        if !r["revision"].is_null() {
            let _ = writeln!(s, "  annotated: revision {}", r["revision"]);
        }
        if let Some(o) = r["out"].as_str() {
            let _ = writeln!(s, "  saved: {o}");
        }
        if r["copied_to_clipboard"] == json!(true) {
            s.push_str("  copied to the clipboard\n");
        }
        for w in r["warnings"].as_array().into_iter().flatten() {
            let _ = writeln!(s, "  warning: {}", plain(w));
        }
        s
    });
    Ok(())
}

async fn get_capture(cx: &mut Ctx<'_>, a: GetArgs) -> Result<(), CliError> {
    let mut params = json!({ "capture_id": a.id, "include_image": a.out.is_some() });
    if let Some(r) = a.revision {
        params["revision"] = json!(r);
    }
    let mut c = cx.client().await?;
    let mut r = c.call("get_capture", params).await?;
    take_image(&mut r, a.out.as_deref())?;
    cx.emit(&r, |r| {
        let mut s = format!(
            "{}  {}\n  kind: {}\n  created: {}\n  revision: {}  (undo: {}, redo: {})\n  ops: {}\n  path: {}\n",
            plain(&r["capture_id"]),
            plain(&r["title"]),
            plain(&r["kind"]),
            plain(&r["created"]),
            r["revision"],
            r["can_undo"],
            r["can_redo"],
            r["ops"].as_array().map_or(0, Vec::len),
            plain(&r["path"]),
        );
        if let Some(o) = r["out"].as_str() {
            let _ = writeln!(s, "  saved: {o}");
        }
        s
    });
    Ok(())
}

/// `annotate <id> --suggest-redactions` / `--redact-emails` (ANN-2.AC4): read-only.
async fn suggest_redactions(cx: &mut Ctx<'_>, a: AnnotateArgs) -> Result<(), CliError> {
    let params = if a.redact_emails {
        json!({ "capture_id": a.id, "kinds": ["email"] })
    } else {
        json!({ "capture_id": a.id })
    };
    let mut c = cx.client().await?;
    let r = c.call("suggest_redactions", params).await?;
    cx.emit(&r, |r| {
        let rects = r["rects"].as_array().map_or(&[][..], Vec::as_slice);
        let mut s = format!(
            "{} proposed redaction(s) for {} (nothing applied)\n",
            rects.len(),
            plain(&r["capture_id"])
        );
        for x in rects {
            let n = |i: usize| x["rect"][i].as_f64().unwrap_or(0.0);
            let _ = writeln!(
                s,
                "  {:<10} [{}, {}, {}, {}]  confidence {:.2}  {}",
                plain(&x["kind"]),
                n(0),
                n(1),
                n(2),
                n(3),
                x["confidence"].as_f64().unwrap_or(0.0),
                plain(&x["text_preview"])
            );
        }
        if !rects.is_empty() {
            s.push_str(
                "  accept with: annotate <id> --ops '[{\"op\":\"blur\",\"rect\":[x,y,w,h]}]'\n",
            );
        }
        s
    });
    Ok(())
}

async fn annotate(cx: &mut Ctx<'_>, a: AnnotateArgs) -> Result<(), CliError> {
    if a.suggest_redactions || a.redact_emails {
        return suggest_redactions(cx, a).await;
    }
    let ops_arg = a.ops.as_deref().unwrap_or_default();
    let ops = load_ops(ops_arg, "--ops", &mut cx.env.stdin)?;
    let params = cx.mutating(
        json!({ "capture_id": a.id, "ops": ops, "replace": a.replace,
                "copy_to_clipboard": a.copy, "return_image": !a.no_image }),
        "",
    );
    let mut c = cx.client().await?;
    let mut r = c.call("annotate", params).await?;
    take_image(&mut r, a.out.as_deref())?;
    cx.emit(&r, |r| {
        let mut s = format!(
            "Annotated {}: revision {} by {}\n",
            plain(&r["capture_id"]),
            r["revision"],
            plain(&r["by"])
        );
        if let Some(o) = r["out"].as_str() {
            let _ = writeln!(s, "  saved: {o}");
        }
        if r["copied_to_clipboard"] == json!(true) {
            s.push_str("  copied to the clipboard\n");
        }
        for w in r["warnings"].as_array().into_iter().flatten() {
            let _ = writeln!(s, "  warning: {}", plain(w));
        }
        s
    });
    Ok(())
}

async fn undo_redo(cx: &mut Ctx<'_>, a: RevisionArgs, undo: bool) -> Result<(), CliError> {
    let params = cx.mutating(json!({ "capture_id": a.id }), "");
    let mut c = cx.client().await?;
    let method = if undo { "undo" } else { "redo" };
    let mut r = c.call(method, params).await?;
    take_image(&mut r, a.out.as_deref())?;
    cx.emit(&r, |r| {
        if r["changed"] == json!(true) {
            format!(
                "{} {}: now at revision {} (undo: {}, redo: {})\n",
                if undo { "Undid" } else { "Redid" },
                plain(&r["capture_id"]),
                r["revision"],
                r["can_undo"],
                r["can_redo"]
            )
        } else {
            format!(
                "Nothing to {} for {}\n",
                if undo { "undo" } else { "redo" },
                plain(&r["capture_id"])
            )
        }
    });
    Ok(())
}

fn paging_params(p: &LibraryPaging, query: Option<String>) -> Value {
    let mut o = Map::new();
    if let Some(q) = query {
        o.insert("query".into(), json!(q));
    }
    if let Some(k) = &p.kind {
        o.insert("kind".into(), json!(k));
    }
    if let Some(s) = &p.status {
        o.insert("status".into(), json!(s));
    }
    if let Some(l) = p.limit {
        o.insert("limit".into(), json!(l));
    }
    if let Some(c) = &p.cursor {
        o.insert("cursor".into(), json!(c));
    }
    Value::Object(o)
}

async fn library_list(cx: &mut Ctx<'_>, a: LibraryListArgs) -> Result<(), CliError> {
    let params = paging_params(&a.paging, a.query);
    let mut c = cx.client().await?;
    let r = c.call("list_library", params).await?;
    cx.emit(&r, |r| {
        let rows = r["captures"].as_array().cloned().unwrap_or_default();
        if rows.is_empty() {
            return "No captures.".to_owned();
        }
        let mut s = String::new();
        for x in &rows {
            let _ = writeln!(
                s,
                "{}  {}  {}x{}  {}  {}{}",
                plain(&x["id"]),
                plain(&x["kind"]),
                x["width"],
                x["height"],
                plain(&x["created"]),
                plain(&x["title"]),
                if x["broken"] == json!(true) {
                    "  (broken)"
                } else {
                    ""
                }
            );
        }
        if let Some(n) = r["next_cursor"].as_str() {
            let _ = writeln!(s, "more: --cursor {n}");
        }
        s
    });
    Ok(())
}

async fn library_rename(cx: &mut Ctx<'_>, id: String, title: String) -> Result<(), CliError> {
    let params = cx.mutating(json!({ "capture_id": id, "title": title }), "");
    let mut c = cx.client().await?;
    let r = c.call("rename_capture", params).await?;
    cx.emit(&r, |r| {
        format!(
            "Renamed {} to \"{}\"\n  library: {}\n",
            plain(&r["capture_id"]),
            plain(&r["title"]),
            plain(&r["path"])
        )
    });
    Ok(())
}

/// A time argument: seconds (`12.5`) or ticks (`t:1500000`), passed on for the daemon to check.
fn time_arg(field: &str, s: &str) -> Result<Value, CliError> {
    if s.strip_prefix("t:")
        .is_some_and(|t| t.parse::<i64>().is_ok())
    {
        return Ok(json!(s));
    }
    s.parse::<f64>()
        .ok()
        .filter(|n| n.is_finite())
        .map(|n| json!(n))
        .ok_or_else(|| {
            CliError::invalid_input(format!("{field}: `{s}` is not seconds or t:<ticks>"))
        })
}

/// `START-END` (`10-15`, `1.5-3`, `t:120000-t:240000`): the dash that separates two times.
fn range_arg(s: &str) -> Result<Value, CliError> {
    for (i, _) in s.match_indices('-').filter(|(i, _)| *i > 0) {
        if let (Ok(a), Ok(b)) = (time_arg("range", &s[..i]), time_arg("range", &s[i + 1..])) {
            return Ok(json!([a, b]));
        }
    }
    Err(CliError::invalid_input(format!(
        "range: `{s}` is not START-END, for example 10-15"
    )))
}

/// `x,y` as a JSON pair of numbers.
fn point_arg(field: &str, s: &str) -> Result<Value, CliError> {
    let parts: Vec<&str> = s.split(',').map(str::trim).collect();
    match parts.as_slice() {
        [x, y] => match (x.parse::<f64>(), y.parse::<f64>()) {
            (Ok(x), Ok(y)) => Ok(json!([x, y])),
            _ => Err(CliError::invalid_input(format!(
                "--{field} `{s}` is not two numbers like 0.5,0.4"
            ))),
        },
        _ => Err(CliError::invalid_input(format!(
            "--{field} `{s}` must be x,y like 0.5,0.4"
        ))),
    }
}

fn range_list(ranges: &[String]) -> Result<Vec<Value>, CliError> {
    ranges.iter().map(|r| range_arg(r)).collect()
}

async fn edit(cx: &mut Ctx<'_>, cmd: EditCmd) -> Result<(), CliError> {
    let (method, id, params) = match cmd {
        EditCmd::Trim { id, start, end } => (
            "trim",
            id,
            json!({"start": time_arg("start", &start)?, "end": time_arg("end", &end)?}),
        ),
        EditCmd::Cut { id, ranges } => (
            "cut_segments",
            id,
            json!({ "ranges": range_list(&ranges)? }),
        ),
        EditCmd::Restore { id, ranges } => (
            "restore_segments",
            id,
            json!({ "ranges": range_list(&ranges)? }),
        ),
        EditCmd::Split { id, at } => ("split", id, json!({"at": time_arg("at", &at)?})),
        EditCmd::Speed {
            id,
            range,
            factor,
            keep_audio,
        } => (
            "set_speed",
            id,
            json!({"range": range_arg(&range)?, "factor": factor, "keep_audio": keep_audio}),
        ),
        EditCmd::Background {
            id,
            kind,
            value,
            padding,
            radius,
            shadow,
        } => {
            let mut p = json!({ "kind": kind });
            if let Some(v) = value {
                // `#a,#b` is a gradient's two stops.
                p["value"] = if kind == "gradient" {
                    json!(v.split(',').map(str::trim).collect::<Vec<_>>())
                } else {
                    json!(v)
                };
            }
            if let Some(v) = padding {
                p["padding_pct"] = json!(v);
            }
            if let Some(v) = radius {
                p["radius_px"] = json!(v);
            }
            if let Some(v) = shadow {
                p["shadow"] = json!(v);
            }
            ("set_background", id, p)
        }
        EditCmd::Audio {
            id,
            mic_gain,
            system_gain,
            mic_mute,
            system_mute,
            denoise,
        } => {
            let mut p = json!({});
            for (k, v) in [
                ("mic_gain_db", mic_gain.map(|v| json!(v))),
                ("system_gain_db", system_gain.map(|v| json!(v))),
                ("mic_mute", mic_mute.map(|v| json!(v))),
                ("system_mute", system_mute.map(|v| json!(v))),
                ("denoise", denoise.map(|v| json!(v))),
            ] {
                if let Some(v) = v {
                    p[k] = v;
                }
            }
            ("set_audio_mix", id, p)
        }
        EditCmd::Camera {
            id,
            range,
            layout,
            shape,
            rect,
            ring,
            mirror,
            smooth,
        } => {
            let mut p = json!({"range": range_arg(&range)?, "layout": layout});
            if let Some(v) = shape {
                p["shape"] = json!(v);
            }
            if let Some(v) = rect {
                let parts: Result<Vec<f64>, _> =
                    v.split(',').map(|x| x.trim().parse::<f64>()).collect();
                p["rect"] = match parts {
                    Ok(r) if r.len() == 4 => json!(r),
                    _ => {
                        return Err(CliError::invalid_input(
                            "--rect must be four numbers: X,Y,W,H as fractions of the frame",
                        ))
                    }
                };
            }
            if let Some(v) = ring {
                p["ring"] = if v == "none" {
                    json!(false)
                } else {
                    json!(v.parse::<f64>().map_err(|_| {
                        CliError::invalid_input("--ring must be a number or `none`")
                    })?)
                };
            }
            if let Some(v) = mirror {
                p["mirror"] = json!(v);
            }
            if smooth {
                p["transition"] = json!("smooth");
            }
            ("set_camera_layout", id, p)
        }
        EditCmd::Zoom {
            id,
            at,
            length,
            focus,
            scale,
            follow_cursor,
            replace,
            clear,
        } => {
            if clear {
                ("set_zooms", id, json!({"zooms": [], "replace": true}))
            } else {
                let (Some(at), Some(length)) = (at, length) else {
                    return Err(CliError::invalid_input(
                        "give --at and --length (or --clear)",
                    ));
                };
                let mut z = json!({
                    "at": time_arg("at", &at)?,
                    "length": time_arg("length", &length)?,
                    "focus": focus.as_deref().map_or(Ok(json!([0.5, 0.5])), |f| point_arg("focus", f))?,
                    "follow_cursor": follow_cursor,
                });
                if let Some(s) = scale {
                    z["scale"] = json!(s);
                }
                ("set_zooms", id, json!({"zooms": [z], "replace": replace}))
            }
        }
        EditCmd::Cursor {
            id,
            size,
            smoothing,
            click_effect,
            hide_idle,
        } => {
            let mut p = json!({});
            for (k, v) in [
                ("size", size.map(|v| json!(v))),
                ("smoothing", smoothing.map(|v| json!(v))),
                ("click_effect", click_effect.map(|v| json!(v))),
                ("hide_idle", hide_idle.map(|v| json!(v))),
            ] {
                if let Some(v) = v {
                    p[k] = v;
                }
            }
            ("set_cursor_style", id, p)
        }
        EditCmd::Reframe {
            id,
            aspect,
            mode,
            focus,
            at,
        } => {
            let mut p = json!({ "aspect": aspect });
            if let Some(m) = mode {
                p["mode"] = json!(m);
            }
            if let Some(f) = focus {
                p["focus"] = point_arg("focus", &f)?;
            }
            if let Some(a) = at {
                p["at"] = time_arg("at", &a)?;
            }
            ("set_reframe", id, p)
        }
    };
    send_edit(cx, method, id, params).await
}

async fn send_edit(
    cx: &mut Ctx<'_>,
    method: &str,
    id: String,
    mut params: Value,
) -> Result<(), CliError> {
    params["capture_id"] = json!(id);
    let params = cx.mutating(params, "");
    let mut c = cx.client().await?;
    let r = c.call(method, params).await?;
    cx.emit(&r, |r| {
        format!(
            "{} {}: the export is now {:.2} s (revision {}, undo: {}, redo: {})\n",
            method,
            plain(&r["capture_id"]),
            r["duration_s"].as_f64().unwrap_or(0.0),
            r["revision"],
            r["can_undo"],
            r["can_redo"]
        )
    });
    Ok(())
}

/// `~`, `~/x` and relative paths made absolute (the daemon wants an absolute path).
fn absolute_path(p: &str) -> Result<String, CliError> {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" });
    let expanded = match (p, &home) {
        ("~", Some(h)) => PathBuf::from(h),
        (p, Some(h)) if p.starts_with("~/") => PathBuf::from(h).join(&p[2..]),
        _ => PathBuf::from(p),
    };
    std::path::absolute(&expanded)
        .map(|a| a.to_string_lossy().into_owned())
        .map_err(|e| CliError::invalid_input(format!("cannot resolve `{p}`: {e}")))
}

async fn library_move(cx: &mut Ctx<'_>, a: MoveArgs) -> Result<(), CliError> {
    let mut p = json!({ "path": absolute_path(&a.path)? });
    if a.fresh {
        p["mode"] = json!("fresh");
    } else if a.use_existing {
        p["mode"] = json!("use_existing");
    }
    let params = cx.mutating(p, "");
    let mut c = cx.client().await?;
    let started = c.call("move_library", params).await?;
    let describe = |r: &Value| {
        format!(
            "{} -> {} ({} captures, {} bytes)",
            plain(&r["from"]),
            plain(&r["to"]),
            plain(&r["bundles"]),
            plain(&r["bytes"])
        )
    };
    if started["job_id"].is_null() {
        cx.emit(&started, |r| {
            format!(
                "Library folder changed ({}): {}\n  Restart CaptureMann to use it. The old files were left where they are.",
                plain(&r["mode"]),
                describe(r)
            )
        });
        return Ok(());
    }
    if !a.wait {
        cx.emit(&started, |r| {
            format!(
                "Moving the library: {}: job {}\n  watch it with `capturemann jobs get {}`; restart CaptureMann when it is done",
                describe(r),
                plain(&r["job_id"]),
                plain(&r["job_id"]),
            )
        });
        return Ok(());
    }
    let job_id = plain(&started["job_id"]);
    let mut last_pct = -1i64;
    let deadline = a
        .timeout
        .map(|s| tokio::time::Instant::now() + std::time::Duration::from_secs_f64(s));
    loop {
        let j = c.call("get_job", json!({ "job_id": job_id })).await?;
        match j["state"].as_str() {
            Some("done") => {
                cx.emit(&j, |j| {
                    format!(
                        "Moved {} captures ({} bytes) to {}\n  Restart CaptureMann to use the new folder. The old copies were not deleted.",
                        plain(&j["result"]["bundles"]),
                        plain(&j["result"]["bytes"]),
                        plain(&j["result"]["to"]),
                    )
                });
                return Ok(());
            }
            Some("failed") => {
                let reason = j["error"]["reason"]
                    .as_str()
                    .unwrap_or("unknown")
                    .to_owned();
                let mut e = CliError::new(ErrorCode::JobFailed, reason);
                e.data = json!({ "job_id": job_id });
                return Err(e);
            }
            Some("cancelled") => {
                return Err(CliError::new(
                    ErrorCode::JobFailed,
                    "the move was cancelled; the old library is still in use",
                ));
            }
            _ => {}
        }
        if let Some(d) = deadline {
            if tokio::time::Instant::now() >= d {
                let mut e = CliError::new(
                    ErrorCode::Busy,
                    format!(
                        "the move is still running after {} s; it keeps running in the daemon (job {job_id})",
                        a.timeout.unwrap_or_default()
                    ),
                );
                e.data = json!({ "job_id": job_id, "state": j["state"], "timed_out": true });
                return Err(e);
            }
        }
        if !cx.json {
            let pct = (j["progress"].as_f64().unwrap_or(0.0) * 100.0).floor() as i64;
            if pct != last_pct {
                last_pct = pct;
                let _ = writeln!(cx.env.err, "copying... {pct}%");
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
}

async fn library_rm(cx: &mut Ctx<'_>, id: String, also_remote: bool) -> Result<(), CliError> {
    let mut params = json!({ "capture_id": id });
    if also_remote {
        params["also_delete_remote"] = json!(true);
    }
    let params = cx.mutating(params, "");
    let mut c = cx.client().await?;
    let r = c.call("delete_capture", params).await?;
    cx.emit(&r, |r| {
        let mut s = format!(
            "Moved {} (\"{}\") to the Trash\n",
            plain(&r["capture_id"]),
            plain(&r["title"])
        );
        if r["remote"].is_object() {
            for (key, label) in [
                ("deleted", "Deleted remotely"),
                ("already_gone", "Already gone"),
            ] {
                for item in r["remote"][key].as_array().into_iter().flatten() {
                    let _ = writeln!(s, "{label}: {}", plain(item));
                }
            }
            for item in r["remote"]["left_in_place"]
                .as_array()
                .into_iter()
                .flatten()
            {
                let _ = writeln!(
                    s,
                    "Left in place: {} ({})",
                    plain(&item["item"]),
                    plain(&item["reason"])
                );
            }
        }
        s
    });
    Ok(())
}

fn settings_lines(r: &Value) -> String {
    let mut s = String::new();
    // Sorted explicitly: with serde_json's `preserve_order` feature (enabled by other workspace crates)
    // object keys would otherwise come back in insertion order.
    let mut entries: Vec<_> = r["settings"].as_object().into_iter().flatten().collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));
    for (k, v) in entries {
        let _ = writeln!(s, "{k} = {}", plain(v));
    }
    if s.is_empty() {
        "No settings are set.".to_owned()
    } else {
        s
    }
}

async fn settings_get(cx: &mut Ctx<'_>, keys: Vec<String>) -> Result<(), CliError> {
    let params = if keys.is_empty() {
        json!({})
    } else {
        json!({ "keys": keys })
    };
    let mut c = cx.client().await?;
    let r = c.call("get_settings", params).await?;
    cx.emit(&r, settings_lines);
    Ok(())
}

async fn settings_set(cx: &mut Ctx<'_>, pairs: Vec<String>) -> Result<(), CliError> {
    if !pairs.len().is_multiple_of(2) {
        return Err(CliError::invalid_input(
            "settings set takes KEY VALUE pairs; the last key has no value",
        ));
    }
    let mut values = Map::new();
    for kv in pairs.chunks(2) {
        // `60`, `true`, `null`, `[1]` are JSON; anything else (`hevc`, `bottom-right`) is text.
        let v = serde_json::from_str::<Value>(&kv[1]).unwrap_or_else(|_| json!(kv[1]));
        values.insert(kv[0].clone(), v);
    }
    let params = cx.mutating(json!({ "values": values }), "");
    let mut c = cx.client().await?;
    let r = c.call("set_settings", params).await?;
    cx.emit(&r, settings_lines);
    Ok(())
}

async fn daemon_start(
    cx: &mut Ctx<'_>,
    library: Option<std::path::PathBuf>,
) -> Result<(), CliError> {
    let launcher = match (&cx.env.launcher, library) {
        (None, _) => None,
        (Some(_), Some(lib)) => Some(process_launcher(Some(lib))),
        (Some(l), None) => Some(l.clone()),
    };
    let was_running = Client::connect_existing(&cx.endpoint).await.is_ok();
    Client::connect_or_start(&cx.endpoint, launcher.as_ref()).await?;
    let r = json!({ "running": true, "started": !was_running,
                    "endpoint": describe(&cx.endpoint) });
    cx.emit(&r, |r| {
        format!(
            "Daemon {} at {}",
            if r["started"] == json!(true) {
                "started"
            } else {
                "already running"
            },
            plain(&r["endpoint"])
        )
    });
    Ok(())
}

async fn daemon_status(cx: &mut Ctx<'_>) -> Result<(), CliError> {
    let mut c = Client::connect_existing(&cx.endpoint).await?;
    let actions = c.call("actions.list", json!({})).await?;
    let r = json!({ "running": true, "endpoint": describe(&cx.endpoint),
                    "actions": actions.as_array().map_or(0, Vec::len) });
    cx.emit(&r, |r| {
        format!(
            "Daemon running at {} ({} actions)",
            plain(&r["endpoint"]),
            r["actions"]
        )
    });
    Ok(())
}

/// `42%`, or `-` when the job has not reported anything.
fn percent(j: &Value) -> String {
    j["progress"]
        .as_f64()
        .map_or_else(|| "-".to_owned(), |p| format!("{:.0}%", p * 100.0))
}

fn job_line(j: &Value) -> String {
    let mut s = format!(
        "{}  {}  {}  {}",
        plain(&j["job_id"]),
        plain(&j["type"]),
        plain(&j["state"]),
        percent(j)
    );
    if let Some(m) = j["message"].as_str().filter(|m| !m.is_empty()) {
        let _ = write!(s, "  {m}");
    }
    if let Some(e) = j["error"]["reason"].as_str() {
        let _ = write!(s, "  failed: {e}");
    }
    s
}

fn job_detail(j: &Value) -> String {
    let mut s = job_line(j);
    for (label, key) in [
        ("created", "created_at"),
        ("started", "started_at"),
        ("finished", "finished_at"),
    ] {
        if let Some(t) = j[key].as_str() {
            let _ = write!(s, "\n  {label}: {t}");
        }
    }
    if let Some(ms) = j["elapsed_ms"].as_i64() {
        let _ = write!(s, "\n  elapsed: {} s", ms / 1000);
    }
    if let Some(ms) = j["eta_ms"].as_i64() {
        let _ = write!(s, "\n  eta: {} s", ms / 1000);
    }
    if !j["result"].is_null() {
        let _ = write!(s, "\n  result: {}", j["result"]);
    }
    s
}

/// `export <id> [--preset quick] [--wait]`: `export_video`, then (with `--wait`) polls the job.
async fn export_video(cx: &mut Ctx<'_>, a: ExportArgs) -> Result<(), CliError> {
    // `--captions srt|vtt|txt` writes the caption file (AI-1) instead of rendering a video.
    if let Some(format) = a.captions {
        if a.timestamps && format != "txt" {
            return Err(CliError::invalid_input(
                "--timestamps only applies to --captions txt (SRT and VTT always have times)",
            ));
        }
        return ai::export_captions(cx, a.id, format, a.timestamps).await;
    }
    let mut p = json!({ "capture_id": a.id, "preset": a.preset });
    if let Some(f) = &a.format {
        // `--format gif` stands for the preset of that name; the default preset yields to it.
        if a.preset == "quick" && f != "mp4" {
            p["preset"] = json!(f);
        } else {
            p["format"] = json!(f);
        }
    }
    if let Some(l) = a.loudness {
        p["loudness_lufs"] = json!(l);
    }
    if a.burn_captions {
        p["captions"] = json!("burn");
    } else if a.no_captions {
        p["captions"] = json!("none");
    }
    for (key, value) in [
        ("width", a.width.map(|w| json!(w))),
        ("fps", a.fps.map(|f| json!(f))),
        ("max_mb", a.max_mb.map(|m| json!(m))),
    ] {
        if let Some(v) = value {
            p[key] = v;
        }
    }
    let params = cx.mutating(p, "");
    let mut c = cx.client().await?;
    let started = c.call("export_video", params).await?;
    if !a.wait {
        cx.emit(&started, |r| {
            format!(
                "Exporting {} ({}): job {}{}\n  watch it with `capturemann jobs get {}`",
                plain(&r["capture_id"]),
                plain(&r["preset"]),
                plain(&r["job_id"]),
                if r["reused"] == json!(true) {
                    " (already running)"
                } else {
                    ""
                },
                plain(&r["job_id"]),
            )
        });
        return Ok(());
    }
    let job_id = plain(&started["job_id"]);
    let mut last_pct = -1i64;
    let mut last_held = false;
    let deadline = a
        .timeout
        .map(|s| tokio::time::Instant::now() + std::time::Duration::from_secs_f64(s));
    loop {
        let j = c.call("get_job", json!({ "job_id": job_id })).await?;
        match j["state"].as_str() {
            Some("done") => {
                cx.emit(&j, |j| {
                    format!(
                        "Exported {}\n  file: {}\n  size: {} bytes, {} s, {}",
                        plain(&j["result"]["capture_id"]),
                        plain(&j["result"]["path"]),
                        plain(&j["result"]["size_bytes"]),
                        plain(&j["result"]["duration_s"]),
                        plain(&j["result"]["encoder"]),
                    )
                });
                return Ok(());
            }
            Some("failed") => {
                let reason = j["error"]["reason"]
                    .as_str()
                    .unwrap_or("unknown")
                    .to_owned();
                let code = j["error"]["code"].as_str().unwrap_or("job_failed");
                // A missing FFmpeg stays `not_found` (exit 2) with its install hint; everything
                // else is `job_failed` (exit 6).
                let mut e = if code == "not_found" {
                    CliError::not_found(reason)
                } else {
                    CliError::new(ErrorCode::JobFailed, reason)
                };
                e.data = json!({ "job_id": job_id });
                return Err(e);
            }
            Some("cancelled") => {
                return Err(CliError::new(
                    ErrorCode::JobFailed,
                    "the export was cancelled",
                ));
            }
            _ => {}
        }
        // A recording running holds exports (EXP-4.AC2): say so instead of looking stuck.
        let held = j["held"] == json!(true) || j["state"] == json!("paused");
        if held && !last_held && !cx.json {
            let _ = writeln!(
                cx.env.err,
                "export is held (paused while a recording runs or by a user pause); waiting..."
            );
        }
        last_held = held;
        if let Some(d) = deadline {
            if tokio::time::Instant::now() >= d {
                let mut e = CliError::new(
                    ErrorCode::Busy,
                    format!(
                        "the export is still {} after {} s; it keeps running in the daemon (job {job_id})",
                        if held { "held" } else { "running" },
                        a.timeout.unwrap_or_default()
                    ),
                );
                e.data = json!({ "job_id": job_id, "state": j["state"], "timed_out": true });
                return Err(e);
            }
        }
        if !cx.json {
            let pct = (j["progress"].as_f64().unwrap_or(0.0) * 100.0).floor() as i64;
            if pct != last_pct {
                last_pct = pct;
                let line = format!("exporting... {pct}%");
                let _ = writeln!(cx.env.err, "{line}");
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
}

async fn jobs_get(cx: &mut Ctx<'_>, id: String) -> Result<(), CliError> {
    let mut c = cx.client().await?;
    let r = c.call("get_job", json!({ "job_id": id })).await?;
    cx.emit(&r, job_detail);
    Ok(())
}

async fn jobs_list(
    cx: &mut Ctx<'_>,
    state: Option<String>,
    job_type: Option<String>,
    limit: Option<usize>,
) -> Result<(), CliError> {
    let mut params = Map::new();
    if let Some(s) = state {
        params.insert("state".into(), json!(s));
    }
    if let Some(t) = job_type {
        params.insert("type".into(), json!(t));
    }
    if let Some(l) = limit {
        params.insert("limit".into(), json!(l));
    }
    let mut c = cx.client().await?;
    let r = c.call("list_jobs", Value::Object(params)).await?;
    cx.emit(&r, |r| {
        let jobs = r["jobs"].as_array().cloned().unwrap_or_default();
        if jobs.is_empty() {
            return "No jobs.".to_owned();
        }
        jobs.iter().map(job_line).collect::<Vec<_>>().join("\n")
    });
    Ok(())
}

async fn jobs_cancel(cx: &mut Ctx<'_>, id: String) -> Result<(), CliError> {
    let params = cx.mutating(json!({ "job_id": id }), "");
    let mut c = cx.client().await?;
    let r = c.call("cancel_job", params).await?;
    cx.emit(&r, |r| {
        if r["cancelling"] == json!(true) {
            format!("Cancelling {}", job_line(r))
        } else {
            job_line(r)
        }
    });
    Ok(())
}

async fn jobs_retry(cx: &mut Ctx<'_>, id: String) -> Result<(), CliError> {
    let params = cx.mutating(json!({ "job_id": id }), "");
    let mut c = cx.client().await?;
    let r = c.call("retry_job", params).await?;
    cx.emit(&r, |r| format!("Retrying {}", job_line(r)));
    Ok(())
}

/// Is `topic` one that can select `job.*` events?
fn wants_jobs(topic: &str) -> bool {
    topic == "*" || topic.starts_with("job.") || topic == "job"
}

async fn jobs_watch(
    cx: &mut Ctx<'_>,
    topics: Vec<String>,
    max_events: Option<usize>,
) -> Result<(), CliError> {
    let topics = if topics.is_empty() {
        vec!["job.*".to_owned()]
    } else {
        topics
    };
    let mut c = cx.client().await?;
    c.call("events.subscribe", json!({ "topics": topics }))
        .await?;
    // Snapshot after subscribing, so nothing that happens in between is missed: the jobs
    // that are not finished yet, as `job.snapshot` events (not counted by `--max-events`).
    if topics.iter().any(|t| wants_jobs(t)) {
        let r = c.call("list_jobs", json!({ "limit": 200 })).await?;
        let active = r["jobs"].as_array().cloned().unwrap_or_default();
        for j in active
            .iter()
            .filter(|j| matches!(j["state"].as_str(), Some("queued" | "running" | "paused")))
        {
            if cx.json {
                let line = json!({ "event": "job.snapshot", "data": j });
                cx.print(&line.to_string());
            } else {
                let line = format!("snapshot  {}", job_line(j));
                cx.print(&line);
            }
        }
        let _ = cx.env.out.flush();
    }
    let mut seen = 0usize;
    while max_events.is_none_or(|m| seen < m) {
        let ev = tokio::select! {
            e = c.next_event() => e?,
            _ = tokio::signal::ctrl_c() => break,
        };
        seen += 1;
        if cx.json {
            // One compact JSON object per line.
            cx.print(&ev.to_string());
        } else if ev["data"]["job_id"].is_string() {
            let d = &ev["data"];
            let mut line = format!(
                "{}  {}  {}  {}",
                plain(&ev["at"]),
                plain(&ev["event"]),
                plain(&d["job_id"]),
                plain(&d["type"])
            );
            if ev["event"] == "job.progress" {
                let _ = write!(line, "  {}", percent(d));
                if let Some(m) = d["message"].as_str() {
                    let _ = write!(line, "  {m}");
                }
            } else if let Some(m) = d["error"]["message"].as_str() {
                let _ = write!(line, "  {m}");
            }
            cx.print(&line);
        } else {
            let line = format!(
                "{}  {}  {}",
                plain(&ev["at"]),
                plain(&ev["event"]),
                ev["data"]
            );
            cx.print(&line);
        }
        let _ = cx.env.out.flush();
    }
    Ok(())
}

// ---- recording -------------------------------------------------------------------------

/// `m:ss` or `h:mm:ss` of a millisecond count (the HUD timer format).
fn clock_text(ms: i64) -> String {
    let s = (ms / 1000).max(0);
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
    } else {
        format!("{:02}:{:02}", s / 60, s % 60)
    }
}

fn camera_json(a: &RecordStartArgs) -> Result<Option<Value>, CliError> {
    let options = a.camera_resolution.is_some()
        || a.camera_shape.is_some()
        || a.camera_size.is_some()
        || a.camera_pos.is_some()
        || a.no_camera_mirror;
    let device = match a.camera.as_deref() {
        Some("off") if options => {
            return Err(CliError::invalid_input(
                "--camera off conflicts with the --camera-* options",
            ))
        }
        Some("off") => return Ok(Some(json!(false))),
        None if !options => return Ok(None),
        None | Some("on" | "default") => None,
        Some(id) => Some(id.to_owned()),
    };
    let mut o = Map::new();
    if let Some(id) = device {
        o.insert("device_id".into(), json!(id));
    }
    if let Some(r) = &a.camera_resolution {
        o.insert("resolution".into(), json!(r));
    }
    if let Some(v) = &a.camera_shape {
        o.insert("shape".into(), json!(v));
    }
    if let Some(v) = &a.camera_size {
        o.insert("size".into(), json!(v));
    }
    if let Some(p) = &a.camera_pos {
        let position = if p.contains(',') {
            let parts: Vec<f64> = p
                .split(',')
                .map(|n| n.trim().parse::<f64>())
                .collect::<Result<_, _>>()
                .map_err(|_| {
                    CliError::invalid_input("--camera-pos: expected a name or X,Y fractions")
                })?;
            json!(parts)
        } else {
            json!(p)
        };
        o.insert("position".into(), position);
    }
    if a.no_camera_mirror {
        o.insert("mirror".into(), json!(false));
    }
    Ok(Some(Value::Object(o)))
}

fn mic_json(a: &RecordStartArgs) -> Result<Option<Value>, CliError> {
    let device = match a.mic.as_deref() {
        Some("off") if a.no_denoise => {
            return Err(CliError::invalid_input(
                "--no-denoise needs the microphone on",
            ))
        }
        Some("off") => return Ok(Some(json!(false))),
        None if a.no_denoise => return Err(CliError::invalid_input("--no-denoise needs --mic")),
        None => return Ok(None),
        Some("default") => None,
        Some(id) => Some(id.to_owned()),
    };
    let mut o = Map::new();
    if let Some(id) = device {
        o.insert("device_id".into(), json!(id));
    }
    if a.no_denoise {
        o.insert("denoise".into(), json!(false));
    }
    Ok(Some(if o.is_empty() {
        json!(true)
    } else {
        Value::Object(o)
    }))
}

fn area_text(ind: &Value) -> String {
    let area = &ind["area"];
    let out = &ind["output"];
    let what = match area["kind"].as_str() {
        Some("window") => format!("window {}", area["window_id"]),
        Some("region") => format!("region of display {}", area["display_id"]),
        _ => format!("display {}", area["display_id"]),
    };
    format!(
        "{what}, {} x {} px, {} fps, {}",
        out["width"],
        out["height"],
        out["fps"],
        plain(&out["codec"])
    )
}

fn warning_lines(v: &Value, s: &mut String) {
    for w in v.as_array().into_iter().flatten() {
        let _ = writeln!(s, "  warning: {}", plain(&w["message"]));
    }
}

async fn record_start(cx: &mut Ctx<'_>, a: RecordStartArgs) -> Result<(), CliError> {
    // Validate local inputs before anything is asked of the user.
    let region = a.region.as_deref().map(parse_region).transpose()?;
    let camera = camera_json(&a)?;
    let mic = mic_json(&a)?;
    let mut c = cx.client().await?;
    let target = if let Some(w) = &a.window {
        let t = c.call("list_targets", json!({})).await?;
        json!({ "window_id": resolve_window(&t, w)? })
    } else {
        let display = match a.display {
            Some(d) => u64::from(d),
            None => primary_display(&c.call("list_targets", json!({ "windows": false })).await?)?,
        };
        match region {
            Some(rect) => json!({ "display_id": display, "rect": rect }),
            None => json!({ "display_id": display }),
        }
    };
    let mut p = Map::new();
    p.insert("target".into(), target);
    if let Some(v) = &a.size {
        p.insert("size".into(), json!(v));
    }
    if let Some(v) = &a.fps {
        p.insert("fps".into(), json!(v.parse::<u32>().unwrap_or(30)));
    }
    if let Some(v) = &a.codec {
        p.insert("codec".into(), json!(v));
    }
    if let Some(v) = &a.quality {
        p.insert("quality".into(), json!(v));
    }
    if let Some(v) = camera {
        p.insert("camera".into(), v);
    }
    if let Some(v) = mic {
        p.insert("mic".into(), v);
    }
    if a.system_audio {
        p.insert("system_audio".into(), json!(true));
    }
    if a.no_cursor {
        p.insert("cursor".into(), json!(false));
    }
    if a.cursor_separate {
        p.insert("cursor_separate".into(), json!(true));
    }
    if let Some(n) = a.countdown {
        p.insert("countdown_s".into(), json!(n));
    }
    if let Some(n) = a.max_duration {
        p.insert("max_duration_s".into(), json!(n));
    }
    if let Some(t) = &a.title {
        p.insert("title".into(), json!(t));
    }
    if !a.exclude_apps.is_empty() || !a.exclude_windows.is_empty() {
        p.insert(
            "exclude".into(),
            json!({ "bundle_ids": a.exclude_apps, "window_ids": a.exclude_windows }),
        );
    }
    let params = cx.mutating(Value::Object(p), "");
    // The call blocks while the user answers the dialog (up to 5 minutes) and during the
    // countdown: say so, on stderr so `--json` output stays clean.
    let _ = writeln!(
        cx.env.err,
        "Waiting for confirmation in the CaptureMann window (up to 5 minutes)..."
    );
    let _ = cx.env.err.flush();
    let r = c.call("start_recording", params).await?;
    cx.emit(&r, |r| {
        let mut s = format!(
            "Recording started: {}\n  capture: {}  \"{}\"\n  {}\n  tracks: {}\n",
            plain(&r["recording_id"]),
            plain(&r["capture_id"]),
            plain(&r["title"]),
            area_text(&r["indicator"]),
            r["tracks"]
                .as_array()
                .map(|t| t.iter().map(plain).collect::<Vec<_>>().join(", "))
                .unwrap_or_default()
        );
        if let Some(m) = r["max_duration_s"].as_f64() {
            let _ = writeln!(s, "  stops by itself after {m} s of recording");
        }
        warning_lines(&r["warnings"], &mut s);
        s.push_str("  stop with `capturemann record stop`\n");
        s
    });
    Ok(())
}

async fn record_selftest(cx: &mut Ctx<'_>, a: RecordSelftestArgs) -> Result<(), CliError> {
    let mut p = Map::new();
    if let Some(d) = a.display {
        p.insert("target".into(), json!({ "display_id": d }));
    }
    if let Some(v) = &a.size {
        p.insert("size".into(), json!(v));
    }
    if let Some(v) = &a.codec {
        p.insert("codec".into(), json!(v));
    }
    let params = cx.mutating(Value::Object(p), "");
    let mut c = cx.client().await?;
    let _ = writeln!(
        cx.env.err,
        "Running the 60 fps self-test: the screen is recorded for about 10 s and the test video is          deleted. If asked, confirm in the CaptureMann window (up to 5 minutes)..."
    );
    let _ = cx.env.err.flush();
    let r = c.call("run_fps_selftest", params).await?;
    cx.emit(&r, |r| {
        let size = r["size"]
            .as_array()
            .map(|a| format!("{} x {} px", plain(&a[0]), plain(&a[1])))
            .unwrap_or_default();
        let mut s = format!(
            "60 fps self-test {} ({size}, {}, display {})\n  late frames: {:.2}% (limit {}%)  dropped: {}\n",
            if r["passed"] == json!(true) { "passed" } else { "failed" },
            plain(&r["codec"]),
            plain(&r["display_id"]),
            r["late_pct"].as_f64().unwrap_or(0.0),
            plain(&r["threshold"]["late_pct"]),
            plain(&r["dropped"]),
        );
        if r["passed"] == json!(true) {
            s.push_str("  `capturemann record start --fps 60` now works for these settings\n");
        } else {
            let _ = writeln!(s, "  {}", plain(&r["message"]));
            if let Some(size) = r["suggestion"]["size"].as_str() {
                let _ = writeln!(s, "  try: capturemann record selftest --size {size}");
            }
        }
        if r["stored"] == json!(false) {
            let _ = writeln!(s, "  warning: the result could not be saved: {}", plain(&r["store_error"]));
        }
        s
    });
    Ok(())
}

async fn record_control(
    cx: &mut Ctx<'_>,
    method: &'static str,
    a: RecordRefArgs,
) -> Result<(), CliError> {
    let mut p = Map::new();
    if let Some(id) = &a.recording_id {
        p.insert("recording_id".into(), json!(id));
    }
    let params = cx.mutating(Value::Object(p), "");
    let mut c = cx.client().await?;
    let r = c.call(method, params).await?;
    cx.emit(&r, |r| match method {
        "stop_recording" => {
            if r["selftest_id"].is_string() {
                return "The 60 fps self-test was cancelled; nothing was kept.".to_owned();
            }
            if r["cancelled"] == json!(true) {
                return "The countdown was cancelled; nothing was recorded.".to_owned();
            }
            let mut s = format!(
                "Stopped {}{}: {}\n  duration: {}\n  tracks: {}\n  saved: {}\n",
                plain(&r["recording_id"]),
                if r["already_stopped"] == json!(true) {
                    " (it had already stopped)"
                } else {
                    ""
                },
                plain(&r["capture_id"]),
                clock_text(r["duration_ms"].as_i64().unwrap_or(0)),
                r["tracks"]
                    .as_array()
                    .map(|t| t
                        .iter()
                        .map(|t| plain(&t["kind"]))
                        .collect::<Vec<_>>()
                        .join(", "))
                    .unwrap_or_default(),
                plain(&r["path"])
            );
            if !matches!(r["reason"].as_str(), Some("user" | "agent") | None) {
                let _ = writeln!(s, "  stopped because: {}", plain(&r["reason"]));
            }
            warning_lines(&r["warnings"], &mut s);
            s
        }
        "pause_recording" => {
            if r["cancelled"] == json!(true) {
                "The countdown was cancelled; nothing was recorded.".to_owned()
            } else if r["changed"] == json!(false) {
                "The recording is already paused.".to_owned()
            } else {
                format!(
                    "Paused at {}",
                    clock_text(r["recorded_ms"].as_i64().unwrap_or(0))
                )
            }
        }
        _ => {
            if r["cancelled"] == json!(true) {
                "The resume was cancelled; the recording stays paused.".to_owned()
            } else if r["changed"] == json!(false) {
                "The recording is already running.".to_owned()
            } else {
                format!(
                    "Recording resumed (paused for {})",
                    clock_text(r["paused_ms"].as_i64().unwrap_or(0))
                )
            }
        }
    });
    Ok(())
}

async fn record_mic(cx: &mut Ctx<'_>, a: RecordMicArgs) -> Result<(), CliError> {
    let mut p = Map::new();
    p.insert("muted".into(), json!(a.mute));
    if let Some(id) = &a.recording_id {
        p.insert("recording_id".into(), json!(id));
    }
    let params = cx.mutating(Value::Object(p), "");
    let mut c = cx.client().await?;
    let r = c.call("set_mic_muted", params).await?;
    cx.emit(&r, |r| {
        let now = if r["muted"] == json!(true) {
            "muted"
        } else {
            "unmuted"
        };
        if r["changed"] == json!(false) {
            format!("The microphone is already {now}.")
        } else {
            format!(
                "Microphone {now} at {}",
                clock_text(r["at_ms"].as_i64().unwrap_or(0))
            )
        }
    });
    Ok(())
}

async fn record_status(cx: &mut Ctx<'_>, a: RecordRefArgs) -> Result<(), CliError> {
    let params = match &a.recording_id {
        Some(id) => json!({ "recording_id": id }),
        None => json!({}),
    };
    let mut c = cx.client().await?;
    let r = c.call("get_recording_status", params).await?;
    cx.emit(&r, |r| {
        let state = plain(&r["state"]);
        if state == "idle" {
            return match r["last"].as_object() {
                Some(l) => format!(
                    "No recording is running. Last: {} ({})",
                    plain(&l["recording_id"]),
                    plain(&l["state"])
                ),
                None => "No recording is running.".to_owned(),
            };
        }
        let mut s = format!("{}  {}\n", plain(&r["recording_id"]), state);
        if state == "countdown" {
            let _ = writeln!(s, "  starts in {} s", r["countdown_remaining"]);
            return s;
        }
        let _ = writeln!(
            s,
            "  recorded {}  (paused {}, {} pause{})",
            clock_text(r["recorded_ms"].as_i64().unwrap_or(0)),
            clock_text(r["paused_ms"].as_i64().unwrap_or(0)),
            r["pauses"],
            if r["pauses"] == 1 { "" } else { "s" }
        );
        if r["capture_id"].is_string() {
            let _ = writeln!(
                s,
                "  capture: {}  \"{}\"",
                plain(&r["capture_id"]),
                plain(&r["title"])
            );
        }
        let tracks: Vec<String> = r["tracks"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|t| format!("{} ({})", plain(&t["kind"]), plain(&t["state"])))
            .collect();
        let _ = writeln!(s, "  tracks: {}", tracks.join(", "));
        if let Some(db) = r["mic_level_dbfs"].as_f64() {
            let _ = writeln!(s, "  mic level: {db:.0} dBFS");
        }
        if r["mic_muted"] == json!(true) {
            let _ = writeln!(s, "  microphone: muted");
        }
        if let Some(p) = r["late_frame_pct"].as_f64() {
            let _ = writeln!(s, "  late frames: {p:.1}%");
        }
        if let Some(m) = r["disk_left_minutes"].as_f64() {
            let _ = writeln!(s, "  disk space for about {m:.0} more minutes");
        }
        warning_lines(&r["warnings"], &mut s);
        s
    });
    Ok(())
}

async fn destinations(cx: &mut Ctx<'_>) -> Result<(), CliError> {
    let mut c = cx.client().await?;
    let r = c.call("list_destinations", json!({})).await?;
    cx.emit(&r, |r| {
        let rows = r["destinations"].as_array().cloned().unwrap_or_default();
        if rows.is_empty() {
            return "No destinations. Add one in Settings > Destinations.\n".to_owned();
        }
        let mut s = String::new();
        for d in &rows {
            let _ = writeln!(
                s,
                "{}  {}  {}  {}  sends {}{}{}",
                plain(&d["id"]),
                plain(&d["name"]),
                plain(&d["kind"]),
                plain(&d["path"]),
                plain(&d["what"]),
                if d["default"] == json!(true) {
                    "  (default)"
                } else {
                    ""
                },
                if d["online"] == json!(true) {
                    ""
                } else {
                    "  (offline)"
                }
            );
        }
        s
    });
    Ok(())
}

async fn upload(cx: &mut Ctx<'_>, a: UploadArgs) -> Result<(), CliError> {
    let mut params = serde_json::Map::new();
    for (k, v) in [
        ("capture_id", a.capture_id),
        ("file", a.file),
        ("destination_id", a.to),
        ("folder", a.folder),
        ("what", a.what),
    ] {
        if let Some(v) = v {
            params.insert(k.to_owned(), json!(v));
        }
    }
    let params = cx.mutating(Value::Object(params), "");
    let mut c = cx.client().await?;
    let r = c.call("upload", params).await?;
    cx.emit(&r, |r| {
        format!(
            "Upload queued: job {}\n  to: {}\n  watch: capturemann jobs get {}\n",
            plain(&r["job_id"]),
            plain(&r["destination_id"]),
            plain(&r["job_id"])
        )
    });
    Ok(())
}

async fn publish(cx: &mut Ctx<'_>, a: PublishArgs) -> Result<(), CliError> {
    match a.cmd {
        Some(PublishCmd::Accounts) => publish_accounts(cx).await,
        None => publish_send(cx, a.send).await,
    }
}

async fn publish_accounts(cx: &mut Ctx<'_>) -> Result<(), CliError> {
    let mut c = cx.client().await?;
    let r = c.call("list_publish_accounts", json!({})).await?;
    cx.emit(&r, |r| {
        let rows = r["accounts"].as_array().cloned().unwrap_or_default();
        if rows.is_empty() {
            return "No publish accounts. Connect one in Settings > Destinations.\n".to_owned();
        }
        let mut s = String::new();
        for a in &rows {
            let _ = writeln!(
                s,
                "{}  {}  {}  default privacy {}  {} of {} uploads used today{}{}",
                plain(&a["account_id"]),
                plain(&a["platform"]),
                plain(&a["name"]),
                plain(&a["default_privacy"]),
                plain(&a["uploads_today"]),
                plain(&a["uploads_limit"]),
                if a["default"] == json!(true) {
                    "  (default)"
                } else {
                    ""
                },
                if a["status"] == json!("needs_reconnect") {
                    "  (reconnect needed)"
                } else {
                    ""
                }
            );
        }
        s
    });
    Ok(())
}

async fn publish_send(cx: &mut Ctx<'_>, a: PublishSendArgs) -> Result<(), CliError> {
    let Some(capture_id) = a.capture_id else {
        return Err(CliError::local(
            "invalid_input",
            "give a capture id (`capturemann publish <capture_id> --title ...`) or `publish accounts`",
        ));
    };
    let mut params = serde_json::Map::new();
    params.insert("capture_id".into(), json!(capture_id));
    params.insert("platform".into(), json!(a.platform));
    for (k, v) in [
        ("account_id", a.account),
        ("title", a.title),
        ("description", a.description),
        ("privacy", a.privacy),
        ("thumbnail", a.thumbnail),
        ("export_preset", a.export_preset),
        ("category", a.category),
        ("playlist_id", a.playlist),
    ] {
        if let Some(v) = v {
            params.insert(k.to_owned(), json!(v));
        }
    }
    if !a.tags.is_empty() {
        params.insert("tags".into(), json!(a.tags));
    }
    if let Some(k) = a.made_for_kids {
        params.insert("made_for_kids".into(), json!(k == "yes"));
    }
    if a.captions {
        params.insert("captions".into(), json!(true));
    }
    let params = cx.mutating(Value::Object(params), "");
    let mut c = cx.client().await?;
    let r = c.call("publish_video", params).await?;
    cx.emit(&r, |r| {
        format!(
            "Publish queued: job {}\n  to: {} ({}) as {}\n  watch: capturemann jobs get {}\n",
            plain(&r["job_id"]),
            plain(&r["platform"]),
            plain(&r["account"]),
            plain(&r["privacy"]),
            plain(&r["job_id"])
        )
    });
    Ok(())
}

async fn devices(cx: &mut Ctx<'_>) -> Result<(), CliError> {
    let mut c = cx.client().await?;
    let r = c.call("list_devices", json!({})).await?;
    cx.emit(&r, |r| {
        let mut s = String::from("Cameras\n");
        if let Some(e) = r["cameras_error"].as_str() {
            let _ = writeln!(s, "  (cannot list cameras: {e})");
        }
        for d in r["cameras"].as_array().into_iter().flatten() {
            let _ = writeln!(
                s,
                "  {}  {}  up to {} x {}{}",
                plain(&d["id"]),
                plain(&d["name"]),
                d["max_size"][0],
                d["max_size"][1],
                if d["is_default"] == json!(true) {
                    "  (default)"
                } else {
                    ""
                }
            );
        }
        let _ = writeln!(s, "  permission: {}", plain(&r["camera_permission"]));
        s.push_str("Microphones\n");
        for d in r["mics"].as_array().into_iter().flatten() {
            let _ = writeln!(
                s,
                "  {}  {}  {} ch, {} Hz{}",
                plain(&d["id"]),
                plain(&d["name"]),
                d["channels"],
                d["sample_rate"],
                if d["default"] == json!(true) {
                    "  (default)"
                } else {
                    ""
                }
            );
        }
        let _ = writeln!(s, "  permission: {}", plain(&r["microphone_permission"]));
        let sys = &r["system_audio"];
        let _ = writeln!(
            s,
            "System audio\n  {} ({}){}",
            if sys["supported"] == json!(true) {
                "supported"
            } else {
                "not supported"
            },
            plain(&sys["backend"]),
            sys["reason"]
                .as_str()
                .map_or(String::new(), |m| format!(": {m}"))
        );
        let _ = writeln!(
            s,
            "Screen recording permission: {}",
            plain(&r["screen_recording_permission"])
        );
        s
    });
    Ok(())
}

async fn record_watch(
    cx: &mut Ctx<'_>,
    topics: Vec<String>,
    max_events: Option<usize>,
) -> Result<(), CliError> {
    let topics = if topics.is_empty() {
        vec!["recording.*".to_owned()]
    } else {
        topics
    };
    let mut c = cx.client().await?;
    c.call("events.subscribe", json!({ "topics": topics }))
        .await?;
    let mut seen = 0usize;
    while max_events.is_none_or(|m| seen < m) {
        let ev = tokio::select! {
            e = c.next_event() => e?,
            _ = tokio::signal::ctrl_c() => break,
        };
        seen += 1;
        if cx.json {
            cx.print(&ev.to_string());
        } else {
            let d = &ev["data"];
            let detail = match ev["event"].as_str() {
                Some("recording.countdown") => format!("{} s", d["remaining"]),
                Some("recording.stats") => format!(
                    "{}  recorded {}",
                    plain(&d["state"]),
                    clock_text(d["recorded_ms"].as_i64().unwrap_or(0))
                ),
                Some("recording.warning") => {
                    format!("{}: {}", plain(&d["code"]), plain(&d["message"]))
                }
                Some("recording.stopped") => {
                    format!("{} ({})", plain(&d["capture_id"]), plain(&d["reason"]))
                }
                Some("recording.selftest") => format!(
                    "{}  remaining {} s  late {}%",
                    plain(&d["state"]),
                    plain(&d["remaining_s"]),
                    d["late_pct"]
                        .as_f64()
                        .map_or_else(String::new, |p| format!("{p:.2}"))
                ),
                _ => d.to_string(),
            };
            let line = format!(
                "{}  {}  {}  {}",
                plain(&ev["at"]),
                plain(&ev["event"]),
                if d["recording_id"].is_null() {
                    plain(&d["selftest_id"])
                } else {
                    plain(&d["recording_id"])
                },
                detail
            );
            cx.print(&line);
        }
        let _ = cx.env.out.flush();
    }
    Ok(())
}
