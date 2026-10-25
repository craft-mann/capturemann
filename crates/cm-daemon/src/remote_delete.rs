//! `delete_capture { also_delete_remote: true }`: deleting the remote copies of a capture.
//!
//! # What is deleted
//!
//! Exactly the copies the capture's manifest records, nothing else:
//!
//! * `uploads[]` of a **Google Drive** destination: the exported file by its id, and an
//!   original-bundle copy by the folder id recorded with it (`folder_id`; Drive deletes the
//!   folder with everything in it, permanently, without the Drive Trash). A bundle copy recorded
//!   before folder ids existed is *left in place* and the dialog says so.
//! * `uploads[]` of a **local** destination: the copied file, or the files of the bundle copy and
//!   then the folders that became empty; only while the destination is still a local folder and
//!   the recorded path is below its root (the provider re-checks with symbolic links resolved).
//!   A file that is gone already is fine; files the user added to the copy stay.
//! * `publishes[]` of **YouTube**: the video by its id (`videos.delete`), permanent.
//!   Nothing here ever changes the privacy of a video.
//! * OneDrive and Dropbox copies, and destinations or accounts that were removed since, are left
//!   in place: the plan says so ("Left in place") instead of failing, and the capture is still
//!   deleted if the user approves.
//!
//! # The plan is the dialog
//!
//! [`RemotePlan`] is built once, before the Confirm dialog, and its rows are the dialog's text:
//! each remote copy with its destination and path (or video title, URL and channel), and that the
//! deletion is permanent. What runs after the approval is that same plan.
//!
//! # Order and failure
//!
//! The remote steps run first. If every step is done (deleted, or already gone), the capture is
//! moved to the Trash. If a step fails, **the local capture stays**: it is the only record of
//! what is left. The error has the code of the first failure (`auth_expired`, `network`,
//! `platform_rejected`, ...) and its `data` lists what was deleted and what was not; the manifest
//! records the outcome of each copy (`remote_deletions`) and drops the records of the copies that
//! are gone, so running the delete again only meets what is left. After a `network` failure the
//! remaining steps are not tried (they would wait on the same outage).

use std::path::Path;

use cm_core::CmError;
use cm_publish::{PublishAccountStore, PublishProvider, YouTubeProvider};
use cm_storage::{Destination, DestinationStore, Kind, LocalProvider, StorageProvider};
use serde_json::{json, Map, Value};

use crate::daemon::Inner;

/// What to do for one recorded remote copy.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Step {
    /// A Google Drive file, or a folder with its content, by id.
    Drive { destination_id: String, id: String },
    /// One copied file in a local destination, by the recorded absolute path.
    LocalFile {
        destination_id: String,
        path: String,
    },
    /// The files of a bundle copy in a local destination (`folder` is below the root).
    LocalBundle {
        destination_id: String,
        folder: String,
        files: Vec<String>,
    },
    /// A YouTube video.
    YouTube {
        account_id: String,
        video_id: String,
    },
    /// Gone already (a local file that no longer exists): nothing to do.
    AlreadyGone,
    /// Not deleted by this action, and why.
    Leave { reason: String },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RemoteItem {
    /// `upload` or `publish`: which list of the manifest the record is in.
    pub kind: &'static str,
    pub record: Value,
    pub step: Step,
    /// The copy as the dialog describes it (English).
    pub text: String,
    /// The same copy as structured data, so a client words it in its own language: `form`
    /// (`drive_file`, `drive_folder`, `local_bundle`, `local_file`, `youtube`, `copy`), the
    /// `path`, `title`, `url`, `channel`, `place` and, for a copy left in place, `reason`
    /// (`{ id, .. }`, ids listed in `specs/features/10-storage.md`).
    pub view: Value,
}

/// The remote copies of one capture and what happens to each.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct RemotePlan {
    pub items: Vec<RemoteItem>,
}

impl RemotePlan {
    pub(crate) fn will_delete(&self) -> impl Iterator<Item = &RemoteItem> {
        self.items
            .iter()
            .filter(|i| !matches!(i.step, Step::Leave { .. } | Step::AlreadyGone))
    }

    /// The copies as structured data for a client that words the dialog itself (the typed
    /// `params.remote_items`): one object per copy with `state` (`delete`, `gone` or `left`),
    /// `n` (its number among the copies of that state, the same as in `remote_delete_N`,
    /// `remote_gone_N` and `remote_left_N`) and the item's `view`.
    pub(crate) fn items_json(&self) -> Value {
        let (mut d, mut g, mut l) = (0, 0, 0);
        let items: Vec<Value> = self
            .items
            .iter()
            .map(|i| {
                let (state, n) = match i.step {
                    Step::AlreadyGone => ("gone", &mut g),
                    Step::Leave { .. } => ("left", &mut l),
                    _ => ("delete", &mut d),
                };
                *n += 1;
                let mut v = i.view.clone();
                v["state"] = json!(state);
                v["n"] = json!(*n);
                v
            })
            .collect();
        Value::Array(items)
    }

    /// The dialog rows for the remote part, in the dialog's order.
    pub(crate) fn rows(&self, rows: &mut Map<String, Value>) {
        let deletes: Vec<&RemoteItem> = self.will_delete().collect();
        if self.items.is_empty() {
            rows.insert(
                "remote".into(),
                json!(
                    "No remote copy is recorded for this capture, so nothing is deleted remotely"
                ),
            );
        }
        for (n, item) in deletes.iter().enumerate() {
            rows.insert(format!("remote_delete_{}", n + 1), json!(item.text));
        }
        for (n, item) in self
            .items
            .iter()
            .filter(|i| matches!(i.step, Step::AlreadyGone))
            .enumerate()
        {
            rows.insert(
                format!("remote_gone_{}", n + 1),
                json!(format!("{} (already gone, nothing to delete)", item.text)),
            );
        }
        for (n, item) in self
            .items
            .iter()
            .filter(|i| matches!(i.step, Step::Leave { .. }))
            .enumerate()
        {
            if let Step::Leave { reason } = &item.step {
                rows.insert(
                    format!("remote_left_{}", n + 1),
                    json!(format!("Left in place: {} ({reason})", item.text)),
                );
            }
        }
        if !deletes.is_empty() {
            rows.insert(
                "permanent".into(),
                json!(
                    "Deleting remote copies is permanent and cannot be undone: Google Drive \
                     files skip the Drive Trash and a deleted YouTube video cannot be restored"
                ),
            );
            rows.insert(
                "order".into(),
                json!(
                    "The remote copies are deleted first; if one fails the capture stays here \
                     and the result says what was and was not deleted"
                ),
            );
        }
    }
}

fn kind_name(k: Kind) -> &'static str {
    match k {
        Kind::Local => "Folder",
        Kind::GoogleDrive => "Google Drive",
        Kind::OneDrive => "OneDrive",
        Kind::Dropbox => "Dropbox",
    }
}

fn str_of<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str).filter(|s| !s.is_empty())
}

/// Plans the deletion of the recorded uploads and publishes of the bundle at `dir`. Reads the
/// manifest and the local files only; nothing is sent anywhere.
pub(crate) fn plan(
    dir: &Path,
    destinations: Option<&DestinationStore>,
    accounts: Option<&PublishAccountStore>,
) -> RemotePlan {
    let mut items = Vec::new();
    for record in cm_library::uploads_of(dir) {
        let dest = str_of(&record, "destination_id")
            .and_then(|id| destinations.and_then(|s| s.get(id).ok()));
        items.push(plan_upload(dir, record, dest));
    }
    for record in cm_library::publishes_of(dir) {
        items.push(plan_publish(record, accounts));
    }
    RemotePlan { items }
}

/// The place of a copy as structured data (the app words it): a local folder by path, or a
/// service with the account and the destination's name.
fn place_view(dest: &Destination) -> Value {
    match (&dest.kind, &dest.account) {
        (Kind::Local, _) => json!({ "type": "folder", "path": dest.path.display().to_string() }),
        (k, Some(a)) => {
            json!({ "type": "service", "service": kind_name(*k), "account": a.label, "name": dest.name })
        }
        (k, None) => json!({ "type": "service", "service": kind_name(*k), "name": dest.name }),
    }
}

/// A copy that is left in place: `view` is the structured description of the copy, `reason` the
/// stable id (and parameters) of why, `reason_en` its English text.
fn leave_item(
    kind: &'static str,
    text: String,
    reason_en: &str,
    reason: Value,
    mut view: Value,
    record: Value,
) -> RemoteItem {
    view["reason"] = reason;
    RemoteItem {
        kind,
        record,
        step: Step::Leave {
            reason: reason_en.to_owned(),
        },
        text,
        view,
    }
}

fn plan_upload(dir: &Path, record: Value, dest: Option<Destination>) -> RemoteItem {
    let what = str_of(&record, "what").unwrap_or("export").to_owned();
    let remote_id = str_of(&record, "remote_id").unwrap_or_default().to_owned();
    let name = str_of(&record, "destination")
        .unwrap_or("a destination")
        .to_owned();
    let shown_path = str_of(&record, "path")
        .map(str::to_owned)
        .unwrap_or_else(|| remote_id.clone());
    let what_key = if what == "original" {
        "original"
    } else {
        "export"
    };
    let what_text = if what == "original" {
        "original bundle"
    } else {
        "export"
    };
    let Some(dest) = dest else {
        let text = format!("{what_text} {shown_path} at \"{name}\"");
        let view = json!({ "form": "copy", "what": what_key, "path": shown_path, "destination_name": name });
        return leave_item(
            "upload",
            text,
            "that destination was removed from Settings",
            json!({ "id": "destination_removed" }),
            view,
            record,
        );
    };
    let place = match (&dest.kind, &dest.account) {
        (Kind::Local, _) => format!("folder \"{}\"", dest.path.display()),
        (k, Some(a)) => format!(
            "{} ({}), destination \"{}\"",
            kind_name(*k),
            a.label,
            dest.name
        ),
        (k, None) => format!("{}, destination \"{}\"", kind_name(*k), dest.name),
    };
    let place_json = place_view(&dest);
    let base_text = format!("{what_text} {shown_path} in {place}");
    let base_view =
        json!({ "form": "copy", "what": what_key, "path": shown_path, "place": place_json });
    let leave = |reason_en: &str, reason: Value, record: Value| {
        leave_item(
            "upload",
            base_text.clone(),
            reason_en,
            reason,
            base_view.clone(),
            record,
        )
    };
    match dest.kind {
        Kind::OneDrive | Kind::Dropbox => leave(
            &format!("deleting on {} is not available yet", kind_name(dest.kind)),
            json!({ "id": "provider_unavailable", "service": kind_name(dest.kind) }),
            record,
        ),
        Kind::GoogleDrive => {
            if what == "original" {
                match str_of(&record, "folder_id") {
                    Some(id) => RemoteItem {
                        kind: "upload",
                        step: Step::Drive {
                            destination_id: dest.id.clone(),
                            id: id.to_owned(),
                        },
                        text: format!("Google Drive: the folder {shown_path} and everything in it ({place})"),
                        view: json!({ "form": "drive_folder", "path": shown_path, "place": place_json }),
                        record,
                    },
                    None => leave(
                        "this copy was made before folder ids were recorded; delete the folder in Google Drive yourself",
                        json!({ "id": "drive_no_folder_id" }),
                        record,
                    ),
                }
            } else if remote_id.is_empty() || remote_id.contains('/') {
                leave(
                    "the record has no Google Drive file id",
                    json!({ "id": "drive_no_file_id" }),
                    record,
                )
            } else {
                RemoteItem {
                    kind: "upload",
                    step: Step::Drive {
                        destination_id: dest.id.clone(),
                        id: remote_id,
                    },
                    text: format!("Google Drive: the file {shown_path} ({place})"),
                    view: json!({ "form": "drive_file", "path": shown_path, "place": place_json }),
                    record,
                }
            }
        }
        Kind::Local => {
            if what == "original" {
                // The files that were copied are the files of the bundle that is being deleted.
                let files = match crate::storage_ops::walk_names(dir) {
                    Ok(f) => f,
                    Err(_) => {
                        return leave(
                            "the bundle cannot be read",
                            json!({ "id": "bundle_unreadable" }),
                            record,
                        );
                    }
                };
                if remote_id.is_empty() || Path::new(&remote_id).is_absolute() {
                    return leave(
                        "the record has no folder inside the destination",
                        json!({ "id": "no_folder_in_destination" }),
                        record,
                    );
                }
                let n = files.len();
                let folder = remote_id.clone();
                RemoteItem {
                    kind: "upload",
                    step: Step::LocalBundle {
                        destination_id: dest.id.clone(),
                        folder,
                        files,
                    },
                    text: format!(
                        "Folder: the {n} copied files of {shown_path} in {place} (files you added there stay)"
                    ),
                    view: json!({ "form": "local_bundle", "files": n, "path": shown_path, "place": place_json }),
                    record,
                }
            } else {
                let p = Path::new(&remote_id);
                if !p.is_absolute() || !p.starts_with(&dest.path) {
                    return leave(
                        "the recorded path is not inside the destination folder",
                        json!({ "id": "path_outside_destination" }),
                        record,
                    );
                }
                let step = if p.is_file() {
                    Step::LocalFile {
                        destination_id: dest.id.clone(),
                        path: remote_id.clone(),
                    }
                } else {
                    Step::AlreadyGone
                };
                RemoteItem {
                    kind: "upload",
                    step,
                    text: format!("Folder: the file {remote_id}"),
                    view: json!({ "form": "local_file", "path": remote_id }),
                    record,
                }
            }
        }
    }
}

fn plan_publish(record: Value, accounts: Option<&PublishAccountStore>) -> RemoteItem {
    let title = str_of(&record, "title").unwrap_or("(untitled)").to_owned();
    let url = str_of(&record, "url").unwrap_or_default().to_owned();
    let channel = str_of(&record, "account").unwrap_or("?").to_owned();
    let platform = str_of(&record, "platform").unwrap_or("?").to_owned();
    let text = format!("YouTube: the video \"{title}\" {url} (channel \"{channel}\")");
    // A record without a title has `title: null`: the app words that case itself.
    let view = json!({
        "form": "youtube",
        "title": str_of(&record, "title"),
        "url": url,
        "channel": channel,
    });
    let leave = |reason_en: &str, reason: Value, record: Value| {
        leave_item(
            "publish",
            text.clone(),
            reason_en,
            reason,
            view.clone(),
            record,
        )
    };
    if platform != "youtube" {
        return leave(
            "deleting on that platform is not available yet",
            json!({ "id": "platform_unsupported" }),
            record,
        );
    }
    let video_id = str_of(&record, "remote_id").unwrap_or_default().to_owned();
    let account_id = str_of(&record, "account_id").unwrap_or_default().to_owned();
    if video_id.is_empty() {
        return leave(
            "the record has no video id",
            json!({ "id": "no_video_id" }),
            record,
        );
    }
    let known = accounts.is_some_and(|s| s.get(&account_id).is_ok());
    if !known {
        return leave(
            "that YouTube account is not connected any more; connect it again to delete the video",
            json!({ "id": "account_not_connected" }),
            record,
        );
    }
    RemoteItem {
        kind: "publish",
        step: Step::YouTube {
            account_id,
            video_id,
        },
        text: text.clone(),
        view: view.clone(),
        record,
    }
}

/// What happened to one item.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Outcome {
    Deleted,
    AlreadyGone,
    Left(String),
    Failed { code: String, message: String },
    NotTried,
}

impl Outcome {
    fn name(&self) -> &'static str {
        match self {
            Outcome::Deleted => "deleted",
            Outcome::AlreadyGone => "already_gone",
            Outcome::Left(_) => "left",
            Outcome::Failed { .. } => "failed",
            Outcome::NotTried => "not_tried",
        }
    }

    pub(crate) fn is_failure(&self) -> bool {
        matches!(self, Outcome::Failed { .. } | Outcome::NotTried)
    }
}

/// The error for a failed remote delete: the code of the failure `e`, a message that starts with
/// the account to reconnect where that applies.
fn failure_text(e: &CmError) -> (String, String) {
    (e.code().as_str().to_owned(), e.to_string())
}

/// Same code as `first`, with another message.
pub(crate) fn same_code(first: &CmError, message: String) -> CmError {
    match first {
        CmError::AuthExpired { .. } => CmError::AuthExpired { message },
        CmError::Network { .. } => CmError::Network { message },
        CmError::PlatformRejected { platform_code, .. } => CmError::PlatformRejected {
            message,
            platform_code: platform_code.clone(),
        },
        CmError::NotImplemented { .. } => CmError::NotImplemented { message },
        CmError::DiskFull { .. } => CmError::DiskFull { message },
        CmError::InvalidInput { .. } => CmError::InvalidInput { message },
        CmError::NotFound { .. } => CmError::NotFound { message },
        _ => CmError::Internal { message },
    }
}

impl Inner {
    /// Runs the plan: every step in order, each one's outcome. Stops trying after a `network`
    /// failure.
    pub(crate) async fn run_remote_plan(
        &self,
        plan: &RemotePlan,
    ) -> Vec<(Outcome, Option<CmError>)> {
        let mut out = Vec::with_capacity(plan.items.len());
        let mut offline = false;
        for item in &plan.items {
            let result: (Outcome, Option<CmError>) = match &item.step {
                Step::Leave { reason } => (Outcome::Left(reason.clone()), None),
                Step::AlreadyGone => (Outcome::AlreadyGone, None),
                _ if offline => (Outcome::NotTried, None),
                step => match self.run_step(step).await {
                    Ok(done) => (done, None),
                    Err(e) => {
                        if matches!(e, CmError::Network { .. }) {
                            offline = true;
                        }
                        let (code, message) = failure_text(&e);
                        (Outcome::Failed { code, message }, Some(e))
                    }
                },
            };
            out.push(result);
        }
        out
    }

    async fn run_step(&self, step: &Step) -> Result<Outcome, CmError> {
        match step {
            Step::Drive { destination_id, id } => {
                let dest = self
                    .remote_destination(destination_id, Kind::GoogleDrive)
                    .await?;
                let provider = self.cloud.provider(&dest)?;
                provider.delete(id).await.map_err(|e| {
                    reconnect_hint(&format!("Google Drive account \"{}\"", dest.name), e)
                })?;
                Ok(Outcome::Deleted)
            }
            Step::LocalFile {
                destination_id,
                path,
            } => {
                let dest = self.remote_destination(destination_id, Kind::Local).await?;
                match LocalProvider::new(&dest.path).delete(path).await {
                    Ok(()) => Ok(Outcome::Deleted),
                    Err(CmError::NotFound { .. }) => Ok(Outcome::AlreadyGone),
                    Err(e) => Err(e),
                }
            }
            Step::LocalBundle {
                destination_id,
                folder,
                files,
            } => {
                let dest = self.remote_destination(destination_id, Kind::Local).await?;
                let (provider, folder, files) = (
                    LocalProvider::new(&dest.path),
                    folder.clone(),
                    files.clone(),
                );
                let r = tokio::task::spawn_blocking(move || {
                    provider.delete_bundle_copy(&folder, &files)
                })
                .await
                .map_err(|e| CmError::internal(format!("worker failed: {e}")))?;
                match r {
                    Ok(_) => Ok(Outcome::Deleted),
                    Err(CmError::NotFound { .. }) => Ok(Outcome::AlreadyGone),
                    Err(e) => Err(e),
                }
            }
            Step::YouTube {
                account_id,
                video_id,
            } => {
                let store = self
                    .publish_accounts
                    .clone()
                    .ok_or_else(|| CmError::not_found("no publish account is set up"))?;
                let id = account_id.clone();
                let account = tokio::task::spawn_blocking(move || store.get(&id))
                    .await
                    .map_err(|e| CmError::internal(format!("worker failed: {e}")))??;
                let provider = YouTubeProvider::new(
                    self.cloud.http.clone(),
                    self.cloud.tokens.source(&account.id),
                    self.youtube.clone(),
                );
                provider.delete_video(video_id).await.map_err(|e| {
                    reconnect_hint(&format!("YouTube account \"{}\"", account.name), e)
                })?;
                Ok(Outcome::Deleted)
            }
            Step::AlreadyGone | Step::Leave { .. } => Ok(Outcome::AlreadyGone),
        }
    }

    /// The destination as it is now, which must still be of `kind` (it may have been edited
    /// since the upload).
    async fn remote_destination(&self, id: &str, kind: Kind) -> Result<Destination, CmError> {
        let store = self
            .destinations
            .clone()
            .ok_or_else(|| CmError::not_found("no destination is set up"))?;
        let id = id.to_owned();
        let dest = tokio::task::spawn_blocking(move || store.get(&id))
            .await
            .map_err(|e| CmError::internal(format!("worker failed: {e}")))??;
        if dest.kind != kind {
            return Err(CmError::invalid_input(format!(
                "destination \"{}\" is not a {} destination any more; nothing was deleted there",
                dest.name,
                kind_name(kind)
            )));
        }
        Ok(dest)
    }
}

fn reconnect_hint(who: &str, e: CmError) -> CmError {
    match e {
        CmError::AuthExpired { message } => CmError::AuthExpired {
            message: format!("Reconnect needed for {who} (auth_expired): {message}"),
        },
        other => other,
    }
}

/// The manifest entries for the outcomes (one per item), see `cm_library::record_remote_deletions`.
pub(crate) fn manifest_entries(
    plan: &RemotePlan,
    outcomes: &[(Outcome, Option<CmError>)],
    at: &str,
) -> Vec<Value> {
    plan.items
        .iter()
        .zip(outcomes)
        .map(|(item, (outcome, _))| {
            let r = &item.record;
            let mut e = Map::new();
            e.insert("kind".into(), json!(item.kind));
            let keys: &[&str] = if item.kind == "upload" {
                &["destination_id", "destination", "what", "remote_id", "path"]
            } else {
                &[
                    "platform",
                    "account_id",
                    "account",
                    "remote_id",
                    "title",
                    "url",
                ]
            };
            for k in keys {
                if let Some(v) = r.get(*k) {
                    e.insert((*k).to_owned(), v.clone());
                }
            }
            e.insert("outcome".into(), json!(outcome.name()));
            match outcome {
                Outcome::Failed { code, message } => {
                    e.insert("error".into(), json!(code));
                    e.insert("message".into(), json!(message));
                }
                Outcome::Left(reason) => {
                    e.insert("reason".into(), json!(reason));
                }
                _ => {}
            }
            e.insert("at".into(), json!(at));
            Value::Object(e)
        })
        .collect()
}

/// The `remote` part of the result and the error data: the text of every item by outcome.
pub(crate) fn summary_json(plan: &RemotePlan, outcomes: &[(Outcome, Option<CmError>)]) -> Value {
    let mut deleted = Vec::new();
    let mut gone = Vec::new();
    let mut left = Vec::new();
    let mut failed = Vec::new();
    let mut not_tried = Vec::new();
    // The same items as structured data, with their outcome: a client that words the result
    // itself reads these (`item` and `message` stay English).
    let mut items = Vec::new();
    for (item, (outcome, _)) in plan.items.iter().zip(outcomes) {
        let mut v = item.view.clone();
        v["outcome"] = json!(outcome.name());
        if let Outcome::Failed { code, message } = outcome {
            v["error"] = json!(code);
            v["message"] = json!(message);
        }
        items.push(v);
        match outcome {
            Outcome::Deleted => deleted.push(json!(item.text)),
            Outcome::AlreadyGone => gone.push(json!(item.text)),
            Outcome::Left(reason) => left.push(json!({ "item": item.text, "reason": reason })),
            Outcome::Failed { code, message } => failed.push(json!({
                "item": item.text, "error": code, "message": message,
            })),
            Outcome::NotTried => not_tried.push(json!(item.text)),
        }
    }
    json!({
        "deleted": deleted,
        "already_gone": gone,
        "left_in_place": left,
        "failed": failed,
        "not_tried": not_tried,
        "items": items,
    })
}
