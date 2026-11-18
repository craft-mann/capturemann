//! `presets list` (`list_presets`, QC-5) and the lookup `--preset` uses to learn a preset's kind.

use std::fmt::Write as _;

use serde_json::{json, Value};

use super::{plain, Ctx};
use crate::client::Client;
use crate::error::CliError;

/// `presets list`: one line per preset.
pub(super) async fn list(cx: &mut Ctx<'_>) -> Result<(), CliError> {
    let mut c = cx.client().await?;
    let r = c.call("list_presets", json!({})).await?;
    cx.emit(&r, |r| {
        let mut s = String::new();
        let list = r["presets"].as_array().cloned().unwrap_or_default();
        if list.is_empty() {
            s.push_str(
                "No presets. Make one in the CaptureMann app (Settings > Capture > Presets).\n",
            );
        }
        for p in &list {
            let _ = writeln!(
                s,
                "{}  \"{}\"  {}",
                plain(&p["id"]),
                plain(&p["name"]),
                plain(&p["summary"])
            );
        }
        if let Some(n) = r["notice"].as_str() {
            let _ = writeln!(s, "Notice: {n}");
        }
        s
    });
    Ok(())
}

/// The preset a `--preset` value names (id first, then name, ignoring case); `None` when there
/// is none, so the daemon answers `invalid_input` naming it.
pub(super) async fn find(c: &mut Client, name: &str) -> Result<Option<Value>, CliError> {
    let r = c.call("list_presets", json!({})).await?;
    let list = r["presets"].as_array().cloned().unwrap_or_default();
    let by_id = list.iter().find(|p| p["id"].as_str() == Some(name));
    let found = by_id.or_else(|| {
        list.iter().find(|p| {
            p["name"]
                .as_str()
                .is_some_and(|n| n.trim().eq_ignore_ascii_case(name.trim()))
        })
    });
    Ok(found.cloned())
}
