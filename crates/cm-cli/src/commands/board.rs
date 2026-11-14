//! `board` (`create_board`, BRD-1) and `export-images` (`export_images`, BLK-1).

use std::fmt::Write as _;

use cm_core::ErrorCode;
use serde_json::{json, Value};

use super::library::absolute_path;
use super::{plain, Ctx};
use crate::args::{BoardArgs, ExportImagesArgs};
use crate::error::CliError;

/// `x,y,w,h` as four integers.
fn parse_slot(s: &str) -> Result<Value, CliError> {
    let bad = || CliError::invalid_input("--slot needs four integers: x,y,w,h (board pixels)");
    let n: Vec<i64> = s
        .split(',')
        .map(|p| p.trim().parse::<i64>().map_err(|_| bad()))
        .collect::<Result<_, _>>()?;
    if n.len() != 4 {
        return Err(bad());
    }
    Ok(json!({ "x": n[0], "y": n[1], "w": n[2], "h": n[3] }))
}

pub(super) async fn board(cx: &mut Ctx<'_>, a: BoardArgs) -> Result<(), CliError> {
    let mut layout = json!({ "kind": a.layout });
    if a.stacked {
        layout["arrangement"] = json!("stacked");
    }
    if let Some(c) = a.columns {
        layout["columns"] = json!(c);
    }
    if !a.slots.is_empty() {
        layout["slots"] = json!(a
            .slots
            .iter()
            .map(|s| parse_slot(s))
            .collect::<Result<Vec<_>, _>>()?);
    }
    let mut options = json!({});
    for (key, v) in [("gap", a.gap), ("margin", a.margin), ("radius", a.radius)] {
        if let Some(v) = v {
            options[key] = json!(v);
        }
    }
    if let Some(b) = &a.background {
        options["background"] = json!(b);
    }
    if a.shadow {
        options["shadow"] = json!(true);
    }
    if a.before_after {
        options["before_after"] = json!(true);
    }
    if !a.captions.is_empty() {
        options["captions"] = json!(a.captions);
    }
    if let Some(t) = &a.board_title {
        options["title"] = json!(t);
    }
    let mut p = json!({ "capture_ids": a.ids, "layout": layout, "options": options });
    if let Some(t) = &a.name {
        p["title"] = json!(t);
    }
    let params = cx.mutating(p, "");
    let mut c = cx.client().await?;
    let mut r = c.call("create_board", params).await?;
    // The picture is large and useless in a terminal: `--json` keeps everything else.
    if let Some(o) = r.as_object_mut() {
        o.remove("image");
    }
    cx.emit(&r, |r| {
        format!(
            "created board {} ({} x {}, {})\n{}",
            plain(&r["capture_id"]),
            plain(&r["width"]),
            plain(&r["height"]),
            plain(&r["layout"]),
            plain(&r["path"])
        )
    });
    Ok(())
}

pub(super) async fn export_images(cx: &mut Ctx<'_>, a: ExportImagesArgs) -> Result<(), CliError> {
    let mut p = json!({ "capture_ids": a.ids, "dest": absolute_path(&a.dest)? });
    let sizes = [
        ("longest", a.longest.map(|v| json!(v))),
        ("width", a.width.map(|v| json!(v))),
        ("height", a.height.map(|v| json!(v))),
        ("percent", a.percent.map(|v| json!(v))),
    ];
    let mut size = json!({});
    for (k, v) in sizes {
        if let Some(v) = v {
            size[k] = v;
        }
    }
    if a.allow_larger {
        size["allow_larger"] = json!(true);
    }
    if size.as_object().is_some_and(|o| !o.is_empty()) {
        p["size"] = size;
    }
    if let Some(f) = &a.format {
        p["format"] = json!(f);
    }
    if let Some(q) = a.quality {
        p["quality"] = json!(q);
    }
    if a.watermark_text.is_some() || a.watermark_image.is_some() {
        let mut w = json!({});
        if let Some(t) = &a.watermark_text {
            w["text"] = json!(t);
        }
        if let Some(i) = &a.watermark_image {
            w["image_path"] = json!(absolute_path(i)?);
        }
        if let Some(v) = &a.watermark_position {
            w["position"] = json!(v);
        }
        if let Some(v) = a.watermark_opacity {
            w["opacity"] = json!(v);
        }
        if let Some(v) = a.watermark_size {
            w["size"] = json!(v);
        }
        p["watermark"] = w;
    }
    if a.blueprint {
        p["blueprint"] = json!(true);
    }
    if let Some(n) = &a.name_pattern {
        p["name_pattern"] = json!(n);
    }
    let params = cx.mutating(p, "");
    let mut c = cx.client().await?;
    let started = c.call("export_images", params).await?;
    let job_id = plain(&started["job_id"]);
    if !a.wait {
        cx.emit(&started, |r| {
            format!(
                "Exporting {} image(s): job {job_id}\n  watch it with `capturemann jobs get {job_id}`",
                plain(&r["count"])
            )
        });
        return Ok(());
    }
    let deadline = a
        .timeout
        .map(|s| tokio::time::Instant::now() + std::time::Duration::from_secs_f64(s));
    loop {
        let j = c.call("get_job", json!({ "job_id": job_id })).await?;
        match j["state"].as_str() {
            Some("done") => {
                cx.emit(&j, |j| {
                    let r = &j["result"];
                    let mut s = String::new();
                    for f in r["files"].as_array().into_iter().flatten() {
                        if f["status"] == "exported" {
                            let _ = writeln!(s, "exported {}", plain(&f["path"]));
                        } else {
                            let _ = writeln!(
                                s,
                                "skipped {}: {}",
                                plain(&f["capture_id"]),
                                plain(&f["reason"])
                            );
                        }
                    }
                    let _ = write!(s, "{}", plain(&r["summary"]));
                    s
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
                    "the export was cancelled; the files already written stay in the folder",
                ));
            }
            _ => {}
        }
        if let Some(d) = deadline {
            if tokio::time::Instant::now() >= d {
                let mut e = CliError::new(
                    ErrorCode::Busy,
                    format!(
                        "the export is still running after {} s; it keeps running in the daemon (job {job_id})",
                        a.timeout.unwrap_or_default()
                    ),
                );
                e.data = json!({ "job_id": job_id, "state": j["state"], "timed_out": true });
                return Err(e);
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
}
