//! `text <id>` (ANN-5): the text in a screenshot, optionally copied (`recognize_text`).

use std::fmt::Write as _;

use serde_json::json;

use super::{plain, Ctx};
use crate::args::TextArgs;
use crate::error::CliError;

/// `--region x,y,w,h` as four numbers.
fn parse_region(s: &str) -> Result<[f64; 4], CliError> {
    let bad = || CliError::invalid_input("--region needs four numbers: x,y,w,h (original pixels)");
    let parts: Vec<&str> = s.split(',').map(str::trim).collect();
    if parts.len() != 4 {
        return Err(bad());
    }
    let mut out = [0.0; 4];
    for (slot, p) in out.iter_mut().zip(parts) {
        *slot = p
            .parse::<f64>()
            .ok()
            .filter(|n| n.is_finite())
            .ok_or_else(bad)?;
    }
    Ok(out)
}

pub(super) async fn text(cx: &mut Ctx<'_>, a: TextArgs) -> Result<(), CliError> {
    let mut p = json!({ "capture_id": a.id });
    if let Some(r) = &a.region {
        p["region"] = json!(parse_region(r)?);
    }
    if a.copy {
        p["copy_to_clipboard"] = json!(true);
    }
    if a.lines {
        p["format"] = json!("lines");
    }
    let mut c = cx.client().await?;
    let r = c.call("recognize_text", p).await?;
    // The text is the output (so it pipes); what the copy did goes to stderr.
    if !cx.json {
        for w in r["warnings"].as_array().into_iter().flatten() {
            let _ = writeln!(cx.env.err, "warning: {}", plain(w));
        }
        if a.copy && r["copied_to_clipboard"] == true {
            let _ = writeln!(
                cx.env.err,
                "copied {} word(s) to the clipboard",
                plain(&r["word_count"])
            );
        }
    }
    cx.emit(&r, |r| {
        if !a.lines {
            return plain(&r["text"]);
        }
        let mut s = String::new();
        for l in r["lines"].as_array().into_iter().flatten() {
            let n = |i: usize| l["rect"][i].as_f64().unwrap_or(0.0);
            let _ = writeln!(
                s,
                "[{}, {}, {}, {}]  {:.2}  {}",
                n(0),
                n(1),
                n(2),
                n(3),
                l["confidence"].as_f64().unwrap_or(0.0),
                plain(&l["text"])
            );
        }
        s
    });
    Ok(())
}
