//! `settings`, `daemon`, `mcp` and `devices`.

use std::fmt::Write as _;

use cm_ipc::Endpoint;
use serde_json::{json, Map, Value};

use super::{plain, Ctx};
use crate::client::{describe, Client};
use crate::error::CliError;
use crate::launch::process_launcher;

/// Runs `cm-mcp` with this process's stdio and returns when it exits. `--socket` is forwarded
/// only when given, so the server otherwise resolves the endpoint itself.
pub(super) fn mcp(cx: &Ctx<'_>, http: bool, port: Option<u16>) -> Result<(), CliError> {
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

pub(super) async fn settings_get(cx: &mut Ctx<'_>, keys: Vec<String>) -> Result<(), CliError> {
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

pub(super) async fn settings_set(cx: &mut Ctx<'_>, pairs: Vec<String>) -> Result<(), CliError> {
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

pub(super) async fn daemon_start(
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

pub(super) async fn daemon_status(cx: &mut Ctx<'_>) -> Result<(), CliError> {
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

pub(super) async fn devices(cx: &mut Ctx<'_>) -> Result<(), CliError> {
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

/// `diagnostics`: the report text, or the whole answer with `--json`.
pub(super) async fn diagnostics(cx: &mut Ctx<'_>) -> Result<(), CliError> {
    let mut c = cx.client().await?;
    let r = c.call("get_diagnostics", json!({})).await?;
    cx.emit(&r, |r| plain(&r["text"]));
    Ok(())
}

/// A time argument: seconds (`12.5`) or ticks (`t:1500000`), passed on for the daemon to check.
fn time_value(field: &str, s: &str) -> Result<Value, CliError> {
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

/// `cursor <id>`: the cursor track of a recording (`get_cursor_track`).
pub(super) async fn cursor_track(
    cx: &mut Ctx<'_>,
    id: String,
    from: Option<String>,
    to: Option<String>,
    limit: Option<u64>,
) -> Result<(), CliError> {
    let mut p = json!({ "capture_id": id });
    if let Some(f) = from {
        p["from"] = time_value("from", &f)?;
    }
    if let Some(t) = to {
        p["to"] = time_value("to", &t)?;
    }
    if let Some(l) = limit {
        p["limit"] = json!(l);
    }
    let mut c = cx.client().await?;
    let r = c.call("get_cursor_track", p).await?;
    cx.emit(&r, |r| {
        let rate = r["tick_rate"].as_f64().unwrap_or(120_000.0);
        let mut s = format!(
            "{}: {} of {} events (clicks {}, scroll {}, moves from {})\n",
            plain(&r["capture_id"]),
            plain(&r["count"]),
            plain(&r["total_in_range"]),
            plain(&r["clicks"]),
            plain(&r["scroll"]),
            plain(&r["move_source"]),
        );
        for e in r["events"].as_array().into_iter().flatten() {
            let t = e["t"].as_f64().unwrap_or(0.0) / rate;
            let kind = plain(&e["type"]);
            let _ = write!(s, "  {t:9.3}s  {kind:<6}");
            match kind.as_str() {
                "move" | "down" | "up" => {
                    let _ = write!(s, " {}, {}", plain(&e["x"]), plain(&e["y"]));
                    if kind != "move" {
                        let _ = write!(s, "  {}", plain(&e["button"]));
                    }
                }
                "scroll" => {
                    let _ = write!(s, " {}, {}", plain(&e["dx"]), plain(&e["dy"]));
                }
                "cursor" => {
                    let _ = write!(s, " {}", plain(&e["id"]));
                }
                _ => {}
            }
            s.push('\n');
        }
        if let Some(n) = r["next_from"].as_i64() {
            let _ = writeln!(s, "More: run again with --from t:{n}");
        }
        s
    });
    Ok(())
}

/// `platform-rules <platform>`: limits, required fields and presets (`get_platform_rules`).
pub(super) async fn platform_rules(cx: &mut Ctx<'_>, platform: String) -> Result<(), CliError> {
    let mut c = cx.client().await?;
    let r = c
        .call("get_platform_rules", json!({ "platform": platform }))
        .await?;
    cx.emit(&r, |r| {
        let l = &r["limits"];
        let list = |v: &Value| {
            v.as_array()
                .into_iter()
                .flatten()
                .map(plain)
                .collect::<Vec<_>>()
                .join(", ")
        };
        let mut s = format!("{}\n", plain(&r["platform"]));
        let _ = writeln!(s, "  max file size: {} bytes", plain(&l["max_bytes"]));
        let _ = writeln!(s, "  max duration: {} s", plain(&l["max_duration_secs"]));
        let _ = writeln!(s, "  video files: {}", list(&l["video_extensions"]));
        let _ = writeln!(
            s,
            "  title: up to {} characters",
            plain(&l["title_max_chars"])
        );
        let _ = writeln!(
            s,
            "  description: up to {} bytes",
            plain(&l["description_max_bytes"])
        );
        let _ = writeln!(
            s,
            "  tags: up to {} characters in all",
            plain(&l["tags_max_total_chars"])
        );
        let _ = writeln!(
            s,
            "  thumbnail: up to {} bytes ({})",
            plain(&l["thumbnail_max_bytes"]),
            list(&l["thumbnail_extensions"])
        );
        let _ = writeln!(s, "  uploads per day: {}", plain(&l["uploads_per_day"]));
        let _ = writeln!(s, "  required: {}", list(&r["required_fields"]));
        let _ = writeln!(
            s,
            "  privacy: {} (default {})",
            list(&r["privacy_options"]),
            plain(&r["default_privacy"])
        );
        let _ = writeln!(
            s,
            "  recommended preset: {}",
            plain(&r["recommended_preset"])
        );
        for p in r["presets"].as_array().into_iter().flatten() {
            let _ = writeln!(s, "    {}  {}", plain(&p["id"]), plain(&p["label"]));
        }
        if let Some(w) = r["warning"].as_str() {
            let _ = writeln!(s, "  note: {w}");
        }
        s
    });
    Ok(())
}
