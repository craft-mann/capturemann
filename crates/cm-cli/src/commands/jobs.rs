//! `jobs` subcommands: get, list, cancel, retry and watch.

use std::fmt::Write as _;

use serde_json::{json, Map, Value};

use super::{plain, Ctx};
use crate::error::CliError;

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

pub(super) async fn jobs_get(cx: &mut Ctx<'_>, id: String) -> Result<(), CliError> {
    let mut c = cx.client().await?;
    let r = c.call("get_job", json!({ "job_id": id })).await?;
    cx.emit(&r, job_detail);
    Ok(())
}

pub(super) async fn jobs_list(
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

pub(super) async fn jobs_cancel(cx: &mut Ctx<'_>, id: String) -> Result<(), CliError> {
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

pub(super) async fn jobs_retry(cx: &mut Ctx<'_>, id: String) -> Result<(), CliError> {
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

pub(super) async fn jobs_watch(
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
