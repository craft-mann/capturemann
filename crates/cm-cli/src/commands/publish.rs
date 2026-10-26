//! Destinations, uploads and publishing.

use serde_json::{json, Value};
use std::fmt::Write as _;

use super::{plain, Ctx};
use crate::args::{PublishArgs, PublishCmd, PublishSendArgs, UploadArgs};
use crate::error::CliError;

pub(super) async fn destinations(cx: &mut Ctx<'_>) -> Result<(), CliError> {
    let mut c = cx.client().await?;
    let r = c.call("list_destinations", json!({})).await?;
    cx.emit(&r, |r| {
        let rows = r["destinations"].as_array().cloned().unwrap_or_default();
        if rows.is_empty() {
            return "No destinations. Add one in Settings > Destinations.\n".to_owned();
        }
        let mut s = String::new();
        for d in &rows {
            let _ = writeln!(
                s,
                "{}  {}  {}  {}  sends {}{}{}",
                plain(&d["id"]),
                plain(&d["name"]),
                plain(&d["kind"]),
                plain(&d["path"]),
                plain(&d["what"]),
                if d["default"] == json!(true) {
                    "  (default)"
                } else {
                    ""
                },
                if d["online"] == json!(true) {
                    ""
                } else {
                    "  (offline)"
                }
            );
        }
        s
    });
    Ok(())
}

pub(super) async fn upload(cx: &mut Ctx<'_>, a: UploadArgs) -> Result<(), CliError> {
    let mut params = serde_json::Map::new();
    for (k, v) in [
        ("capture_id", a.capture_id),
        ("file", a.file),
        ("destination_id", a.to),
        ("folder", a.folder),
        ("what", a.what),
    ] {
        if let Some(v) = v {
            params.insert(k.to_owned(), json!(v));
        }
    }
    let params = cx.mutating(Value::Object(params), "");
    let mut c = cx.client().await?;
    let r = c.call("upload", params).await?;
    cx.emit(&r, |r| {
        format!(
            "Upload queued: job {}\n  to: {}\n  watch: capturemann jobs get {}\n",
            plain(&r["job_id"]),
            plain(&r["destination_id"]),
            plain(&r["job_id"])
        )
    });
    Ok(())
}

pub(super) async fn publish(cx: &mut Ctx<'_>, a: PublishArgs) -> Result<(), CliError> {
    match a.cmd {
        Some(PublishCmd::Accounts) => publish_accounts(cx).await,
        None => publish_send(cx, a.send).await,
    }
}

async fn publish_accounts(cx: &mut Ctx<'_>) -> Result<(), CliError> {
    let mut c = cx.client().await?;
    let r = c.call("list_publish_accounts", json!({})).await?;
    cx.emit(&r, |r| {
        let rows = r["accounts"].as_array().cloned().unwrap_or_default();
        if rows.is_empty() {
            return "No publish accounts. Connect one in Settings > Destinations.\n".to_owned();
        }
        let mut s = String::new();
        for a in &rows {
            let _ = writeln!(
                s,
                "{}  {}  {}  default privacy {}  {} of {} uploads used today{}{}",
                plain(&a["account_id"]),
                plain(&a["platform"]),
                plain(&a["name"]),
                plain(&a["default_privacy"]),
                plain(&a["uploads_today"]),
                plain(&a["uploads_limit"]),
                if a["default"] == json!(true) {
                    "  (default)"
                } else {
                    ""
                },
                if a["status"] == json!("needs_reconnect") {
                    "  (reconnect needed)"
                } else {
                    ""
                }
            );
        }
        s
    });
    Ok(())
}

async fn publish_send(cx: &mut Ctx<'_>, a: PublishSendArgs) -> Result<(), CliError> {
    let Some(capture_id) = a.capture_id else {
        return Err(CliError::local(
            "invalid_input",
            "give a capture id (`capturemann publish <capture_id> --title ...`) or `publish accounts`",
        ));
    };
    let mut params = serde_json::Map::new();
    params.insert("capture_id".into(), json!(capture_id));
    params.insert("platform".into(), json!(a.platform));
    for (k, v) in [
        ("account_id", a.account),
        ("title", a.title),
        ("description", a.description),
        ("privacy", a.privacy),
        ("thumbnail", a.thumbnail),
        ("export_preset", a.export_preset),
        ("category", a.category),
        ("playlist_id", a.playlist),
    ] {
        if let Some(v) = v {
            params.insert(k.to_owned(), json!(v));
        }
    }
    if !a.tags.is_empty() {
        params.insert("tags".into(), json!(a.tags));
    }
    if let Some(k) = a.made_for_kids {
        params.insert("made_for_kids".into(), json!(k == "yes"));
    }
    if a.captions {
        params.insert("captions".into(), json!(true));
    }
    let params = cx.mutating(Value::Object(params), "");
    let mut c = cx.client().await?;
    let r = c.call("publish_video", params).await?;
    cx.emit(&r, |r| {
        format!(
            "Publish queued: job {}\n  to: {} ({}) as {}\n  watch: capturemann jobs get {}\n",
            plain(&r["job_id"]),
            plain(&r["platform"]),
            plain(&r["account"]),
            plain(&r["privacy"]),
            plain(&r["job_id"])
        )
    });
    Ok(())
}
