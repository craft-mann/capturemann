//! The publish sheet's whole conversation with the daemon, in one small module so the contract
//! can be adjusted in one place when the engine behind `publish_video` lands.
//!
//! What this assumes (specs/02-agent-interface.md, PUB-2 "Agent interface"):
//!
//! * `publish_video { capture_id, platform: "youtube", account_id, export_path?, metadata:
//!   { title, description, tags, privacy, made_for_kids, category?, thumbnail? }, request_id }`
//!   is Confirm tier: the call returns after the user answered the daemon's dialog, with
//!   `{ job_id }`. A refusal (`user_declined`, `confirmation_timeout`) or a daemon without the
//!   engine (`not_implemented`) is the call's error, shown as it is.
//! * `account_id` is the id in `publish_accounts.json` (`cm_publish::accounts`).
//! * `export_path` is the export the sheet shows (absolute); when absent the daemon exports
//!   with a platform preset itself.
//! * The job's `type` is `publish`; its events and result are read by [`super::job`].
//! * `retry_job { job_id, request_id }` continues a failed publish.
//! * `list_publish_accounts` may report the day's quota per account as `quota { used, limit }`;
//!   when it does not (or the method does not exist yet) the sheet shows no quota line.

use std::path::Path;

use cm_core::ErrorCode;
use cm_daemon::Daemon;
use cm_publish::PublishMeta;
use serde_json::{json, Map, Value};

use super::form::Quota;
use super::job::{Failure, OwnFailure};
use crate::backend::Backend;

pub const PLATFORM: &str = "youtube";

/// The `publish_video` request.
pub fn publish_params(
    capture_id: &str,
    account_id: &str,
    meta: &PublishMeta,
    export_path: Option<&Path>,
    captions: bool,
    request_id: &str,
) -> Value {
    let mut m = Map::new();
    m.insert("title".into(), json!(meta.title));
    m.insert("description".into(), json!(meta.description));
    m.insert("tags".into(), json!(meta.tags));
    if let Some(p) = meta.privacy {
        m.insert("privacy".into(), json!(p.as_str()));
    }
    if let Some(k) = meta.made_for_kids {
        m.insert("made_for_kids".into(), json!(k));
    }
    if let Some(c) = &meta.category {
        m.insert("category".into(), json!(c));
    }
    if let Some(t) = &meta.thumbnail {
        m.insert("thumbnail".into(), json!(t.display().to_string()));
    }
    if let Some(pl) = &meta.playlist_id {
        m.insert("playlist_id".into(), json!(pl));
    }
    if captions {
        // The daemon makes the SRT from the recording's captions when the publish is approved.
        m.insert("captions".into(), json!(true));
    }
    let mut p = json!({
        "capture_id": capture_id,
        "platform": PLATFORM,
        "account_id": account_id,
        "metadata": Value::Object(m),
        "request_id": request_id,
    });
    if let Some(e) = export_path {
        p["export_path"] = json!(e.display().to_string());
    }
    p
}

/// The job a successful `publish_video` started.
pub fn parse_job_id(answer: &Value) -> Option<String> {
    answer["job_id"].as_str().map(str::to_owned)
}

/// The quota of `account_id` in a `list_publish_accounts` answer.
pub fn parse_quota(answer: &Value, account_id: &str) -> Option<Quota> {
    let entry = answer["accounts"].as_array()?.iter().find(|a| {
        a["id"].as_str() == Some(account_id) || a["account_id"].as_str() == Some(account_id)
    })?;
    let q = &entry["quota"];
    let used = u32::try_from(q["used"].as_u64()?).ok()?;
    let limit = u32::try_from(q["limit"].as_u64()?).ok()?;
    (limit > 0).then_some(Quota { used, limit })
}

fn failure(e: crate::backend::CallError, job_id: Option<&str>) -> Failure {
    Failure {
        code: e.code,
        message: e.message,
        own: None,
        job_id: job_id.map(str::to_owned),
    }
}

/// Starts the publish. The daemon asks the user to confirm first; this returns when they have.
pub async fn submit(daemon: &Daemon, params: Value) -> Result<String, Failure> {
    let answer = Backend::call_coded(daemon, "publish_video", params)
        .await
        .map_err(|e| failure(e, None))?;
    parse_job_id(&answer).ok_or(Failure::own(
        OwnFailure::NoJob,
        Some(ErrorCode::Internal),
        None,
    ))
}

/// "Retry" on a failed job.
pub async fn retry(daemon: &Daemon, job_id: &str, request_id: &str) -> Result<(), Failure> {
    Backend::call_coded(
        daemon,
        "retry_job",
        json!({ "job_id": job_id, "request_id": request_id }),
    )
    .await
    .map(|_| ())
    .map_err(|e| failure(e, Some(job_id)))
}

/// "Cancel" on a running publish (`cancel_job`, as the user): the job object, with `cancelling:
/// true` until the upload has stopped. The typed error is kept (a job that already ended is
/// `invalid_input`).
pub async fn cancel(daemon: &Daemon, job_id: &str) -> Result<Value, crate::backend::CallError> {
    Backend::call_coded(daemon, "cancel_job", json!({ "job_id": job_id })).await
}

/// The job as the daemon has it (`get_job`).
pub async fn job(daemon: &Daemon, job_id: &str) -> Result<Value, String> {
    Backend::call(daemon, "get_job", json!({ "job_id": job_id })).await
}

/// Today's quota of the account, when the daemon reports it.
pub async fn quota(daemon: &Daemon, account_id: &str) -> Option<Quota> {
    let answer = Backend::call(daemon, "list_publish_accounts", json!({}))
        .await
        .ok()?;
    parse_quota(&answer, account_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cm_publish::Privacy;

    #[test]
    fn the_request_is_the_prd_shape_with_only_what_was_chosen() {
        let meta = PublishMeta {
            title: "Demo".into(),
            description: "d".into(),
            tags: vec!["a".into()],
            privacy: Some(Privacy::Unlisted),
            made_for_kids: Some(false),
            thumbnail: Some("/tmp/t.jpg".into()),
            ..PublishMeta::default()
        };
        let p = publish_params(
            "rec_1",
            "youtube-abc",
            &meta,
            Some(Path::new("/lib/rec.mp4")),
            false,
            "ui-publish-1",
        );
        assert_eq!(p["capture_id"], "rec_1");
        assert_eq!(p["platform"], "youtube");
        assert_eq!(p["account_id"], "youtube-abc");
        assert_eq!(p["export_path"], "/lib/rec.mp4");
        assert_eq!(p["request_id"], "ui-publish-1");
        let m = &p["metadata"];
        assert_eq!(m["title"], "Demo");
        assert_eq!(m["privacy"], "unlisted");
        assert_eq!(m["made_for_kids"], false);
        assert_eq!(m["thumbnail"], "/tmp/t.jpg");
        assert!(m.get("category").is_none());
        assert!(m.get("playlist_id").is_none());
        assert!(m.get("captions").is_none());
        // The existing contract still parses it: `capture_id` and `request_id` are the typed
        // fields, everything else rides along.
        let parsed = cm_daemon::Action::parse("publish_video", &p).unwrap();
        assert_eq!(parsed.method(), "publish_video");
        assert_eq!(parsed.capture_id(), Some("rec_1"));
    }

    #[test]
    fn a_playlist_and_captions_ride_in_the_metadata_the_daemon_parses() {
        let meta = PublishMeta {
            title: "Demo".into(),
            playlist_id: Some("PLabc_1".into()),
            ..PublishMeta::default()
        };
        let p = publish_params("rec_1", "a", &meta, None, true, "r");
        assert_eq!(p["metadata"]["playlist_id"], "PLabc_1");
        assert_eq!(p["metadata"]["captions"], true);
        let cm_daemon::Action::PublishVideo(params) =
            cm_daemon::Action::parse("publish_video", &p).unwrap()
        else {
            panic!("publish_video parses to PublishVideo");
        };
        let parsed = cm_daemon::publish_ops::PublishVideoParams::parse(&params).unwrap();
        assert_eq!(parsed.playlist_id.as_deref(), Some("PLabc_1"));
        assert!(parsed.captions);
    }

    #[test]
    fn no_export_path_leaves_the_export_to_the_daemon() {
        let p = publish_params("rec_1", "a", &PublishMeta::default(), None, false, "r");
        assert!(p.get("export_path").is_none());
    }

    #[test]
    fn the_job_and_the_quota_are_read_tolerantly() {
        assert_eq!(parse_job_id(&json!({"job_id":"j1"})).as_deref(), Some("j1"));
        assert_eq!(parse_job_id(&json!({})), None);
        let answer = json!({"accounts":[
            {"account_id":"youtube-a","platform":"youtube","quota":{"used":4,"limit":100}},
            {"id":"youtube-b","quota":{"used":0,"limit":100}}]});
        assert_eq!(
            parse_quota(&answer, "youtube-a"),
            Some(Quota {
                used: 4,
                limit: 100
            })
        );
        assert_eq!(
            parse_quota(&answer, "youtube-b"),
            Some(Quota {
                used: 0,
                limit: 100
            })
        );
        assert_eq!(parse_quota(&answer, "youtube-c"), None);
        assert_eq!(parse_quota(&json!({"accounts":[{"id":"x"}]}), "x"), None);
    }
}
