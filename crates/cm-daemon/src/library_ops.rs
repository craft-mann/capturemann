//! Library management actions: `rename_capture`, `delete_capture` (the local part) and the
//! internal `rescan_library`.
//!
//! Every change of a capture's folder runs under that capture's lock (the same lock `annotate`,
//! `undo` and `export_video` take), so no handler holds a `ShotBundle` of a folder that is being
//! renamed or moved away: the handlers open bundles transiently and drop them before the lock is
//! released. A capture that is being recorded (its `recording.lock` exists), or that an export,
//! upload or publish job names while the job is queued, running or paused, is refused with
//! `busy` (the message names the job). A failed or finished job does not block.
//!
//! # Delete
//!
//! `delete_capture` moves the bundle folder to the system Trash through a [`Trasher`], then
//! drops the index row and publishes `capture.deleted`. With `also_delete_remote: true` the
//! recorded remote copies are deleted first (see [`crate::remote_delete`]); a failed remote step
//! keeps the local capture and reports what was and was not deleted.
//! Confirm tier: an agent waits for the dialog, which names the capture, its size, says the
//! folder is moved to the Trash and, with `also_delete_remote`, lists every remote copy that is
//! deleted (and every one that is left) and that this is permanent; the in-process user (the
//! desktop shows its own confirmation, see [`Daemon::describe_delete`]) skips it.

use cm_core::{Actor, CmError};
use cm_jobs::{JobFilter, JobState, JOB_EXPORT, JOB_PUBLISH, JOB_UPLOAD};
use cm_library::index::CaptureRecord;
use cm_library::rec::{LOCK_FILE, OWNER_FILE};
use serde_json::{json, Value};
use std::path::Path;
use time::format_description::well_known::Rfc3339;

use crate::action::{ExternalParams, RenameCaptureParams};
use crate::daemon::{blocking, lock_library, Daemon, Inner};
use crate::error::DaemonError;
use crate::events;
use crate::remote_delete::{self, RemotePlan};

/// Moves a folder to the system Trash. A trait so tests never touch the real Trash.
pub trait Trasher: Send + Sync {
    /// Moves `path` (a bundle folder) to the Trash, recoverable by the user.
    fn trash(&self, path: &Path) -> Result<(), CmError>;
}

/// The operating system's Trash: `NSFileManager trashItemAtURL` on macOS. Other platforms have no
/// implementation yet and fail with `not_implemented` (nothing is deleted).
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemTrasher;

impl Trasher for SystemTrasher {
    #[cfg(target_os = "macos")]
    fn trash(&self, path: &Path) -> Result<(), CmError> {
        use objc2_foundation::{NSFileManager, NSString, NSURL};
        let path = path.to_str().ok_or_else(|| {
            CmError::invalid_input("the bundle path is not valid UTF-8; it was not deleted")
        })?;
        let url = NSURL::fileURLWithPath(&NSString::from_str(path));
        NSFileManager::defaultManager()
            .trashItemAtURL_resultingItemURL_error(&url, None)
            .map_err(|e| {
                CmError::internal(format!(
                    "cannot move to the Trash: {}",
                    e.localizedDescription()
                ))
            })
    }

    #[cfg(not(target_os = "macos"))]
    fn trash(&self, _path: &Path) -> Result<(), CmError> {
        Err(CmError::not_implemented(
            "moving a capture to the Trash is not available on this platform yet",
        ))
    }
}

/// `1.2 MB` (MB = 10^6 bytes).
pub(crate) fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut v = bytes as f64;
    let mut unit = 0;
    while v >= 1000.0 && unit < UNITS.len() - 1 {
        v /= 1000.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{v:.1} {}", UNITS[unit])
    }
}

/// The delete request as the dialog shows it.
pub(crate) struct DeletePlan {
    pub summary: String,
    pub details: Value,
    /// The same plan as typed parameters (P3): `title`, `id`, `size_bytes`, `remote_copies`,
    /// `remote_items`.
    pub params: Value,
    /// The remote copies and what happens to each; `None` without `also_delete_remote`. What runs
    /// after the approval is this plan, not a new look at the manifest.
    pub remote: Option<RemotePlan>,
}

/// What a delete will do, as the confirmation dialog shows it (for a client that asks the user
/// itself, such as the desktop app).
#[derive(Debug, Clone, PartialEq)]
pub struct DeleteDescription {
    pub summary: String,
    /// Rows in the dialog's order: label to text.
    pub details: Value,
    /// The remote copies as structured data (`params.remote_items`).
    pub remote_items: Value,
}

fn wants_remote(p: &ExternalParams) -> bool {
    p.rest
        .get("also_delete_remote")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

impl Inner {
    pub(crate) fn library_view(&self) -> LibraryView {
        LibraryView::new(self.jobs.as_ref().map(|j| j.queue.clone()))
    }

    /// `rename_capture`: title and folder name, under the capture lock.
    pub(crate) async fn rename_capture(
        &self,
        actor: &Actor,
        p: RenameCaptureParams,
    ) -> Result<Value, DaemonError> {
        let _serial = self.capture_locks.lock(&p.capture_id).await;
        let library = self.library.clone();
        let this = self.library_view();
        let id = p.capture_id.clone();
        let out = blocking(move || {
            let mut lib = lock_library(&library);
            let rec = lib.get(&id)?;
            this.ensure_idle(&rec, &lib.bundle_path(&rec), "rename")?;
            let renamed = lib.rename(&id, &p.title)?;
            Ok(json!({
                "capture_id": renamed.id,
                "title": renamed.title,
                "path": lib.bundle_path(&renamed),
                "kind": renamed.kind.as_str(),
            }))
        })
        .await?;
        self.events.publish(
            events::CAPTURE_REVISED,
            json!({
                "capture_id": out["capture_id"], "title": out["title"],
                "renamed": true, "by": actor,
            }),
        );
        Ok(out)
    }

    /// Validates a `delete_capture` before any dialog and describes it for the dialog.
    pub(crate) async fn plan_delete(&self, p: &ExternalParams) -> Result<DeletePlan, DaemonError> {
        let remote = wants_remote(p);
        let id = p.capture_id.clone().unwrap_or_default();
        let (library, this) = (self.library.clone(), self.library_view());
        let (dests, accounts) = (self.destinations.clone(), self.publish_accounts.clone());
        blocking(move || {
            let lib = lock_library(&library);
            let rec = lib.get(&id)?;
            let dir = lib.bundle_path(&rec);
            this.ensure_idle(&rec, &dir, "delete")?;
            let size = human_size(rec.size_bytes);
            let remote =
                remote.then(|| remote_delete::plan(&dir, dests.as_ref(), accounts.as_ref()));
            let deletes = remote.as_ref().map_or(0, |r| r.will_delete().count());
            let mut summary = format!("Delete \"{}\" ({size})", rec.title);
            if deletes > 0 {
                summary.push_str(&format!(
                    " and permanently delete {deletes} remote cop{}",
                    if deletes == 1 { "y" } else { "ies" }
                ));
            }
            // Row order is the dialog's order.
            let mut rows = serde_json::Map::new();
            rows.insert("capture".into(), json!(rec.title));
            rows.insert("id".into(), json!(rec.id));
            rows.insert("kind".into(), json!(rec.kind.as_str()));
            rows.insert(
                "created".into(),
                json!(rec.created.format(&Rfc3339).unwrap_or_default()),
            );
            rows.insert("size".into(), json!(size));
            if let Some(r) = &remote {
                r.rows(&mut rows);
            }
            rows.insert(
                "result".into(),
                json!(if deletes > 0 {
                    "The remote copies are deleted, then the capture is moved to the Trash"
                } else {
                    "Moved to the Trash"
                }),
            );
            let params = json!({
                "title": rec.title,
                "id": rec.id,
                "kind": rec.kind.as_str(),
                "size_bytes": rec.size_bytes,
                "remote_copies": deletes,
                // The copies as structured data (state, n, form, path, place, reason ...): the
                // app words the remote rows from these (see `RemotePlan::items_json`).
                "remote_items": remote.as_ref().map_or_else(|| json!([]), RemotePlan::items_json),
            });
            Ok(DeletePlan {
                summary,
                details: Value::Object(rows),
                params,
                remote,
            })
        })
        .await
    }

    /// `delete_capture` (local part): the bundle folder goes to the Trash, the row leaves the
    /// index.
    pub(crate) async fn delete_capture(
        &self,
        actor: &Actor,
        p: ExternalParams,
        approved: Option<DeletePlan>,
    ) -> Result<Value, DaemonError> {
        let id = p.capture_id.clone().unwrap_or_default();
        let _serial = self.capture_locks.lock(&id).await;
        let (library, this, trasher) = (
            self.library.clone(),
            self.library_view(),
            self.trasher.clone(),
        );

        // The remote part runs first, and exactly as the dialog showed it.
        let mut remote_json = None;
        let mut deleted_any = false;
        if wants_remote(&p) {
            let (lib2, this2, id2) = (library.clone(), this.clone(), id.clone());
            let (dests, accounts) = (self.destinations.clone(), self.publish_accounts.clone());
            let pinned = approved.and_then(|a| a.remote);
            let (title, dir, plan) = blocking(move || {
                let lib = lock_library(&lib2);
                let rec = lib.get(&id2)?;
                let dir = lib.bundle_path(&rec);
                this2.ensure_idle(&rec, &dir, "delete")?;
                let plan = match pinned {
                    Some(p) => p,
                    None => remote_delete::plan(&dir, dests.as_ref(), accounts.as_ref()),
                };
                Ok((rec.title, dir, plan))
            })
            .await?;
            let outcomes = self.run_remote_plan(&plan).await;
            deleted_any = outcomes
                .iter()
                .any(|(o, _)| matches!(o, remote_delete::Outcome::Deleted));
            let at = self.clock.now().format(&Rfc3339).unwrap_or_default();
            let entries = remote_delete::manifest_entries(&plan, &outcomes, &at);
            let summary = remote_delete::summary_json(&plan, &outcomes);
            // The outcome goes into the manifest (and the records of the copies that are gone
            // out of it) whether or not the capture stays.
            let (lib3, dir3) = (library.clone(), dir.clone());
            let wrote = blocking(move || {
                if entries.is_empty() || !dir3.is_dir() {
                    return Ok(None);
                }
                if let Err(e) = cm_library::record_remote_deletions(&dir3, entries) {
                    return Ok(Some(e.to_string()));
                }
                let _ = lock_library(&lib3).index_dir(&dir3);
                Ok(None)
            })
            .await?;
            if outcomes.iter().any(|(o, _)| o.is_failure()) {
                let first = outcomes.iter().find_map(|(_, e)| e.as_ref());
                let list = |key: &str| -> String {
                    summary[key]
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .map(|v| {
                                    v.as_str().map(str::to_owned).unwrap_or_else(|| {
                                        format!(
                                            "{} ({}: {})",
                                            v["item"].as_str().unwrap_or("?"),
                                            v["error"].as_str().unwrap_or("error"),
                                            v["message"].as_str().unwrap_or("")
                                        )
                                    })
                                })
                                .collect::<Vec<_>>()
                                .join("; ")
                        })
                        .unwrap_or_default()
                };
                let mut message = format!(
                    "Some remote copies could not be deleted, so \"{title}\" was kept on this \
                     computer."
                );
                let done: Vec<String> = ["deleted", "already_gone"]
                    .iter()
                    .map(|k| list(k))
                    .filter(|s| !s.is_empty())
                    .collect();
                if !done.is_empty() {
                    message.push_str(&format!(" Deleted: {}.", done.join("; ")));
                }
                let not_done: Vec<String> = ["failed", "not_tried"]
                    .iter()
                    .map(|k| list(k))
                    .filter(|s| !s.is_empty())
                    .collect();
                message.push_str(&format!(" Not deleted: {}.", not_done.join("; ")));
                message.push_str(
                    " Fix the problem and delete again; only the copies that are left are tried.",
                );
                let error = match first {
                    Some(e) => remote_delete::same_code(e, message),
                    None => CmError::internal(message),
                };
                return Err(DaemonError::WithData {
                    error,
                    data: json!({
                        "capture_id": id,
                        "kept_local": true,
                        "remote": summary,
                    }),
                });
            }
            let mut v = summary;
            if let Some(detail) = wrote {
                v["warning"] = json!(format!(
                    "The outcome was not saved in the manifest: {detail}"
                ));
                // The same warning as a stable kind and its parameter, for a client that
                // words it itself.
                v["warning_kind"] = json!("outcome_not_saved");
                v["warning_detail"] = json!(detail);
            }
            remote_json = Some(v);
        }

        let out = blocking(move || {
            let mut lib = lock_library(&library);
            let rec = lib.get(&id)?;
            let dir = lib.bundle_path(&rec);
            this.ensure_idle(&rec, &dir, "delete")?;
            if dir.exists() {
                trasher.trash(&dir)?;
            }
            lib.remove(&id)?;
            Ok(json!({
                "capture_id": rec.id,
                "title": rec.title,
                "kind": rec.kind.as_str(),
                "size_bytes": rec.size_bytes,
                "moved_to_trash": true,
                "deleted_remote": deleted_any,
            }))
        })
        .await?;
        let mut out = out;
        if let Some(r) = remote_json {
            out["remote"] = r;
        }
        self.events.publish(
            events::CAPTURE_DELETED,
            json!({ "capture_id": out["capture_id"], "title": out["title"], "by": actor }),
        );
        Ok(out)
    }

    /// `rescan_library`: rebuilds the index from the folders, under the library lock.
    pub(crate) async fn rescan_library(&self) -> Result<Value, DaemonError> {
        let library = self.library.clone();
        blocking(move || {
            let r = lock_library(&library).rescan()?;
            Ok(json!({
                "indexed": r.indexed,
                "removed": r.removed,
                "broken": r.broken,
                "incomplete": r.incomplete,
                "duplicate_ids": r.duplicate_ids,
                "duplicates": r.duplicates.iter().map(|d| json!({
                    "id": d.id, "kept": d.kept, "ignored": d.ignored,
                })).collect::<Vec<_>>(),
            }))
        })
        .await
    }
}

/// What the blocking closures need of [`Inner`]: the job queue for the export check. `Inner` is
/// not `Clone`, so the handlers carry this small copy into `spawn_blocking`.
impl Daemon {
    /// What `delete_capture` would do, in the dialog's own words: the rows the Confirm dialog of
    /// an agent's call shows. For a client that asks the user itself before it calls
    /// `delete_capture` as the user (the desktop app): with `also_delete_remote` it lists each
    /// remote copy that will be deleted, each one that is left, and that this is permanent.
    pub async fn describe_delete(
        &self,
        capture_id: &str,
        also_delete_remote: bool,
    ) -> Result<DeleteDescription, DaemonError> {
        let mut rest = serde_json::Map::new();
        rest.insert("also_delete_remote".into(), json!(also_delete_remote));
        let plan = self
            .inner
            .plan_delete(&ExternalParams {
                capture_id: Some(capture_id.to_owned()),
                request_id: None,
                rest,
            })
            .await?;
        Ok(DeleteDescription {
            summary: plan.summary,
            details: plan.details,
            remote_items: plan.params["remote_items"].clone(),
        })
    }
}

#[derive(Clone)]
pub(crate) struct LibraryView {
    queue: Option<cm_jobs::JobQueue>,
}

impl LibraryView {
    pub(crate) fn new(queue: Option<cm_jobs::JobQueue>) -> Self {
        LibraryView { queue }
    }

    /// The id of a queued, running or paused job of `job_type` that names the capture.
    fn active_job(&self, job_type: &str, capture_id: &str) -> Result<Option<String>, CmError> {
        let Some(queue) = &self.queue else {
            return Ok(None);
        };
        let active = queue.list(&JobFilter {
            job_type: Some(job_type.to_owned()),
            states: Some(vec![JobState::Queued, JobState::Running, JobState::Paused]),
            limit: Some(500),
        })?;
        Ok(active
            .iter()
            .find(|j| j.inputs.get("capture_id").and_then(Value::as_str) == Some(capture_id))
            .map(|j| j.id.to_string()))
    }

    pub(crate) fn ensure_idle(
        &self,
        rec: &CaptureRecord,
        dir: &Path,
        what: &str,
    ) -> Result<(), CmError> {
        if dir.join(LOCK_FILE).exists() || dir.join(OWNER_FILE).exists() {
            return Err(CmError::busy(format!(
                "`{}` is still being recorded; stop the recording before you {what} it",
                rec.id
            )));
        }
        if self.active_job(JOB_EXPORT, &rec.id)?.is_some() {
            return Err(CmError::busy(format!(
                "`{}` is being exported; wait for the export to finish or cancel it before you {what} it",
                rec.id
            )));
        }
        // An upload or a publish reads the bundle's files and writes its record into the
        // manifest: not while the folder is renamed or moved to the Trash under it.
        for (job_type, doing, noun) in [
            (JOB_UPLOAD, "uploaded", "upload"),
            (JOB_PUBLISH, "published", "publish"),
        ] {
            if let Some(job) = self.active_job(job_type, &rec.id)? {
                return Err(CmError::busy(format!(
                    "`{}` is being {doing} (job `{job}`); wait for the {noun} to finish or cancel \
                     the job before you {what} it",
                    rec.id
                )));
            }
        }
        Ok(())
    }
}
