//! `library` subcommands: list, search, rename, move and remove.

use std::fmt::Write as _;
use std::path::PathBuf;

use cm_core::ErrorCode;
use serde_json::{json, Map, Value};

use super::{plain, Ctx};
use crate::args::{LibraryListArgs, LibraryPaging, MoveArgs};
use crate::error::CliError;

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

pub(super) async fn library_list(cx: &mut Ctx<'_>, a: LibraryListArgs) -> Result<(), CliError> {
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

pub(super) async fn library_rename(
    cx: &mut Ctx<'_>,
    id: String,
    title: String,
) -> Result<(), CliError> {
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

/// `~`, `~/x` and relative paths made absolute (the daemon wants an absolute path).
fn absolute_path(p: &str) -> Result<String, CliError> {
    let home = cm_platform::dirs::home_dir();
    let expanded = match (p, &home) {
        ("~", Some(h)) => h.clone(),
        (p, Some(h)) if p.starts_with("~/") => h.join(&p[2..]),
        _ => PathBuf::from(p),
    };
    std::path::absolute(&expanded)
        .map(|a| a.to_string_lossy().into_owned())
        .map_err(|e| CliError::invalid_input(format!("cannot resolve `{p}`: {e}")))
}

pub(super) async fn library_move(cx: &mut Ctx<'_>, a: MoveArgs) -> Result<(), CliError> {
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

pub(super) async fn library_rm(
    cx: &mut Ctx<'_>,
    id: String,
    also_remote: bool,
) -> Result<(), CliError> {
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
