//! The publish records of a capture (PUB-2).
//!
//! When a `publish` job finishes, one record is added to `publishes[]` in the capture's
//! `manifest.json` and the index row is refreshed. Like the upload records these are bookkeeping
//! about a copy that lives elsewhere, not edits: no revision is added, so there is nothing to
//! undo. `list_library` items and `get_capture` return them as `publishes`.
//!
//! A record is `{ platform, account_id, account, remote_id, url, title, privacy,
//! privacy_requested, status, warnings, at }`. `privacy` is what the platform reports (it may
//! differ from `privacy_requested` when an unverified API project is locked to private), `status`
//! is `processing`, `processed` or `privacy_locked`, `account` is the label the user sees (never
//! the keychain key).

use cm_publish::{Privacy, PublishAccount, PublishStatus, Published};
use serde_json::{json, Value};

use crate::daemon::lock_library;

/// The wire name of a [`PublishStatus`].
pub(crate) fn status_str(s: PublishStatus) -> &'static str {
    match s {
        PublishStatus::Processing => "processing",
        PublishStatus::Processed => "processed",
        PublishStatus::PrivacyLocked => "privacy_locked",
    }
}

pub(crate) fn build(
    account: &PublishAccount,
    video_title: &str,
    requested: Privacy,
    published: &Published,
    at: &str,
) -> Value {
    json!({
        "platform": account.platform,
        "account_id": account.id,
        "account": account.name,
        "remote_id": published.remote_id,
        "url": published.url,
        "title": video_title,
        "privacy": published.privacy.as_str(),
        "privacy_requested": requested.as_str(),
        "status": status_str(published.status),
        "warnings": published.warnings,
        "at": at,
    })
}

/// Writes the record into the capture's manifest and refreshes the index row, under the
/// capture's lock (see `upload_records::save`). The bundle may have been deleted meanwhile; that
/// is an error for the caller to report, not to retry.
pub(crate) async fn save(
    library: &std::sync::Arc<std::sync::Mutex<cm_library::Library>>,
    locks: &crate::daemon::LockMap,
    capture_id: &str,
    record: Value,
) -> Result<(), cm_core::CmError> {
    let _serial = locks.lock(capture_id).await;
    let (library, id) = (library.clone(), capture_id.to_owned());
    tokio::task::spawn_blocking(move || save_blocking(&library, &id, record))
        .await
        .map_err(|e| cm_core::CmError::internal(format!("worker failed: {e}")))?
}

fn save_blocking(
    library: &std::sync::Mutex<cm_library::Library>,
    capture_id: &str,
    record: Value,
) -> Result<(), cm_core::CmError> {
    let dir: std::path::PathBuf = {
        let lib = lock_library(library);
        let r = lib.get(capture_id)?;
        lib.bundle_path(&r)
    };
    cm_library::record_publish(&dir, record)?;
    lock_library(library).index_dir(&dir)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_record_names_the_label_and_what_the_platform_reported() {
        let account = PublishAccount {
            id: "yt1".into(),
            platform: "youtube".into(),
            name: "My channel".into(),
            account_id: "UC123".into(),
            default_privacy: Privacy::Unlisted,
        };
        let published = Published {
            remote_id: "abc".into(),
            url: "https://youtu.be/abc".into(),
            status: PublishStatus::PrivacyLocked,
            privacy: Privacy::Private,
            warnings: vec!["kept private".into()],
        };
        let r = build(&account, "Demo", Privacy::Unlisted, &published, "t");
        assert_eq!(r["account"], "My channel");
        assert_eq!(r["account_id"], "yt1");
        assert_eq!(r["privacy"], "private");
        assert_eq!(r["privacy_requested"], "unlisted");
        assert_eq!(r["status"], "privacy_locked");
        assert_eq!(r["warnings"][0], "kept private");
        assert!(!r.to_string().contains("UC123"));
    }
}
