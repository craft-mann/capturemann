//! `edit` subcommands: trims, cuts, speed, layout and zoom edits on a recording.

use serde_json::{json, Value};

use super::{plain, Ctx};
use crate::args::EditCmd;
use crate::error::CliError;

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

pub(super) async fn edit(cx: &mut Ctx<'_>, cmd: EditCmd) -> Result<(), CliError> {
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
