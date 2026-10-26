//! `export`: starts an export job and follows it to the end.

use cm_core::ErrorCode;
use serde_json::json;

use super::{ai, plain, Ctx};
use crate::args::ExportArgs;
use crate::error::CliError;

/// `export <id> [--preset quick] [--wait]`: `export_video`, then (with `--wait`) polls the job.
pub(super) async fn export_video(cx: &mut Ctx<'_>, a: ExportArgs) -> Result<(), CliError> {
    // `--captions srt|vtt|txt` writes the caption file (AI-1) instead of rendering a video.
    if let Some(format) = a.captions {
        if a.timestamps && format != "txt" {
            return Err(CliError::invalid_input(
                "--timestamps only applies to --captions txt (SRT and VTT always have times)",
            ));
        }
        return ai::export_captions(cx, a.id, format, a.timestamps, a.language).await;
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
