//! Screenshots, window and display targets, captures, annotations and undo/redo.

use std::fmt::Write as _;
use std::path::Path;

use base64::Engine as _;
use cm_core::ErrorCode;
use serde_json::{json, Value};

use super::{load_ops, plain, Ctx};
use crate::args::{AnnotateArgs, GetArgs, RevisionArgs, ScreenshotArgs};
use crate::error::CliError;

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

pub(super) fn parse_region(s: &str) -> Result<[i64; 4], CliError> {
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
pub(super) fn resolve_window(targets: &Value, needle: &str) -> Result<u64, CliError> {
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

pub(super) fn primary_display(targets: &Value) -> Result<u64, CliError> {
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

pub(super) async fn targets(cx: &mut Ctx<'_>, displays_only: bool) -> Result<(), CliError> {
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

pub(super) async fn screenshot(cx: &mut Ctx<'_>, a: ScreenshotArgs) -> Result<(), CliError> {
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

pub(super) async fn get_capture(cx: &mut Ctx<'_>, a: GetArgs) -> Result<(), CliError> {
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

pub(super) async fn annotate(cx: &mut Ctx<'_>, a: AnnotateArgs) -> Result<(), CliError> {
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

pub(super) async fn undo_redo(
    cx: &mut Ctx<'_>,
    a: RevisionArgs,
    undo: bool,
) -> Result<(), CliError> {
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
