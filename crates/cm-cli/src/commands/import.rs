//! `import`: `import_media`.

use std::fmt::Write as _;

use cm_core::ErrorCode;
use serde_json::{json, Value};

use super::library::absolute_path;
use super::{plain, Ctx};
use crate::args::ImportArgs;
use crate::error::CliError;

fn human(bytes: u64) -> String {
    const U: [&str; 4] = ["B", "KB", "MB", "GB"];
    let (mut v, mut u) = (bytes as f64, 0);
    while v >= 1000.0 && u < U.len() - 1 {
        v /= 1000.0;
        u += 1;
    }
    if u == 0 {
        format!("{bytes} B")
    } else {
        format!("{v:.1} {}", U[u])
    }
}

fn row_line(r: &Value) -> String {
    let status = match r["status"].as_str() {
        Some("ready") => "ready".to_owned(),
        Some("prepared") => format!(
            "prepared for editing ({})",
            r["reasons"]
                .as_array()
                .map(|a| a
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(", "))
                .unwrap_or_default()
        ),
        _ => format!(
            "can't import: {}",
            r["error"]["message"].as_str().unwrap_or("unknown reason")
        ),
    };
    let mut meta = String::new();
    if let Some(d) = r["duration_s"].as_f64() {
        let _ = write!(meta, " {:.1} s", d);
    }
    if let Some(s) = r["size"].as_array() {
        let _ = write!(meta, " {}x{}", plain(&s[0]), plain(&s[1]));
    }
    if let Some(c) = r["video_codec"].as_str() {
        let _ = write!(meta, " {c}");
    }
    if let Some(c) = r["audio_codec"].as_str() {
        let _ = write!(meta, " +{c}");
    }
    format!(
        "{}  ({}{}): {status}",
        plain(&r["name"]),
        human(r["size_bytes"].as_u64().unwrap_or(0)),
        meta
    )
}

pub(super) async fn import_media(cx: &mut Ctx<'_>, a: ImportArgs) -> Result<(), CliError> {
    let mut paths = Vec::new();
    for f in &a.files {
        paths.push(absolute_path(f)?);
    }
    if a.titles.len() > paths.len() {
        return Err(CliError::invalid_input(
            "there are more --title values than files".to_owned(),
        ));
    }
    let mut p = json!({ "paths": paths });
    if !a.titles.is_empty() {
        let mut titles = a.titles.clone();
        titles.resize(paths.len(), String::new());
        p["titles"] = json!(titles);
    }
    if a.remove_originals {
        p["remove_originals"] = json!(true);
    }
    if a.dry_run {
        p["dry_run"] = json!(true);
    }
    if a.transcribe {
        let mut t = json!({});
        if let Some(l) = &a.language {
            t["language"] = json!(l);
        }
        let mut after = json!({ "transcribe": if t.as_object().is_some_and(|o| o.is_empty()) { json!(true) } else { t } });
        if a.diarize {
            after["diarize"] = json!(true);
        }
        if let Some(r) = &a.report {
            after["report_template"] = json!(r);
        }
        p["after"] = after;
    }
    let params = cx.mutating(p, "");
    let mut c = cx.client().await?;
    let started = c.call("import_media", params).await?;
    if a.dry_run {
        cx.emit(&started, |r| {
            r["files"]
                .as_array()
                .map(|f| f.iter().map(row_line).collect::<Vec<_>>().join("\n"))
                .unwrap_or_default()
        });
        return Ok(());
    }
    let job_id = plain(&started["job_id"]);
    if !a.wait {
        cx.emit(&started, |r| {
            let mut s = String::new();
            for cap in r["captures"].as_array().into_iter().flatten() {
                if cap["status"] == "queued" {
                    let _ = writeln!(
                        s,
                        "queued {} -> {}",
                        plain(&cap["path"]),
                        plain(&cap["capture_id"])
                    );
                } else {
                    let _ = writeln!(
                        s,
                        "skipped {}: {}",
                        plain(&cap["path"]),
                        cap["error"]["message"]
                            .as_str()
                            .unwrap_or("can't be imported")
                    );
                }
            }
            let _ = write!(
                s,
                "Importing: job {job_id}\n  watch it with `capturemann jobs get {job_id}`"
            );
            s
        });
        return Ok(());
    }
    let mut last_pct = -1i64;
    let deadline = a
        .timeout
        .map(|s| tokio::time::Instant::now() + std::time::Duration::from_secs_f64(s));
    loop {
        let j = c.call("get_job", json!({ "job_id": job_id })).await?;
        match j["state"].as_str() {
            Some("done") => {
                let skipped: Vec<Value> = started["captures"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|c| c["status"] != "queued")
                    .cloned()
                    .collect();
                let mut out = j.clone();
                out["skipped"] = json!(skipped);
                cx.emit(&out, |j| {
                    let mut s = String::new();
                    for f in j["result"]["files"].as_array().into_iter().flatten() {
                        if f["status"] == "imported" {
                            let _ = write!(
                                s,
                                "imported {} -> {}",
                                plain(&f["path"]),
                                plain(&f["capture_id"])
                            );
                            match f["trash"]["state"].as_str() {
                                Some("trashed") => s.push_str(" (original moved to the Trash)"),
                                Some(_) => s.push_str(
                                    " (the original could not be moved to the Trash and stays)",
                                ),
                                None => {}
                            }
                            // `--transcribe`: the transcript is a job of its own, not part of the import.
                            if let Some(t) = f["after_jobs"]["transcribe"].as_object() {
                                match t.get("state").and_then(Value::as_str) {
                                    Some("queued") => {
                                        let _ = write!(
                                            s,
                                            "\n  transcript queued as job {} (it runs on its own; `capturemann jobs get {}`)",
                                            plain(&t["job_id"]),
                                            plain(&t["job_id"])
                                        );
                                    }
                                    Some("skipped") => {
                                        let _ = write!(
                                            s,
                                            "\n  transcript skipped: {}",
                                            plain(&t["reason"])
                                        );
                                    }
                                    _ => {}
                                }
                            }
                            s.push('\n');
                        } else {
                            let _ = writeln!(
                                s,
                                "failed {}: {}",
                                plain(&f["path"]),
                                f["error"]["message"].as_str().unwrap_or("unknown")
                            );
                        }
                    }
                    for k in j["skipped"].as_array().into_iter().flatten() {
                        let _ = writeln!(
                            s,
                            "skipped {}: {}",
                            plain(&k["path"]),
                            k["error"]["message"]
                                .as_str()
                                .unwrap_or("can't be imported")
                        );
                    }
                    s.trim_end().to_owned()
                });
                return Ok(());
            }
            Some("failed") => {
                let reason = j["error"]["reason"]
                    .as_str()
                    .unwrap_or("unknown")
                    .to_owned();
                let code = match j["error"]["code"].as_str() {
                    Some("disk_full") => ErrorCode::DiskFull,
                    Some("permission_missing") => ErrorCode::PermissionMissing,
                    _ => ErrorCode::JobFailed,
                };
                let mut e = CliError::new(code, reason);
                e.data = json!({ "job_id": job_id });
                return Err(e);
            }
            Some("cancelled") => {
                return Err(CliError::new(
                    ErrorCode::JobFailed,
                    "the import was cancelled; the files already imported are in the library",
                ));
            }
            _ => {}
        }
        if let Some(d) = deadline {
            if tokio::time::Instant::now() >= d {
                let mut e = CliError::new(
                    ErrorCode::Busy,
                    format!(
                        "the import is still running after {} s; it keeps running in the daemon (job {job_id})",
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
                let _ = writeln!(
                    cx.env.err,
                    "{}... {pct}%",
                    j["message"].as_str().unwrap_or("importing")
                );
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
}
