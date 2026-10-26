//! `record` subcommands, recording status and the live event watch.

use serde_json::{json, Map, Value};
use std::fmt::Write as _;

use super::capture::{parse_region, primary_display, resolve_window};
use super::{plain, Ctx};
use crate::args::{RecordMicArgs, RecordRefArgs, RecordSelftestArgs, RecordStartArgs};
use crate::error::CliError;

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

pub(super) async fn record_start(cx: &mut Ctx<'_>, a: RecordStartArgs) -> Result<(), CliError> {
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

pub(super) async fn record_selftest(
    cx: &mut Ctx<'_>,
    a: RecordSelftestArgs,
) -> Result<(), CliError> {
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

pub(super) async fn record_control(
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

pub(super) async fn record_mic(cx: &mut Ctx<'_>, a: RecordMicArgs) -> Result<(), CliError> {
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

pub(super) async fn record_status(cx: &mut Ctx<'_>, a: RecordRefArgs) -> Result<(), CliError> {
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

pub(super) async fn record_watch(
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
