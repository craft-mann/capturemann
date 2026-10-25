//! Uploads in the UI's display-free logic (STO-3 notifications): which upload jobs are running,
//! and which toast a daemon event earns. The window glue (`ui::upload_ui`) feeds events in and
//! turns [`Effect`]s into toasts; everything that decides lives here and is tested without a
//! display.
//!
//! Events used: `upload.queued` (an automatic upload: capture, title, destination), `job.progress`,
//! `job.done` (its `result` names the destination, the files and the `share_link`), `job.failed`
//! (the readable reason), and `job.paused` with `pause_reason: "auth_expired"` (the account must
//! sign in again). A job the app has not heard of (an upload started by `upload` from the CLI or
//! an agent) is looked up once with `get_job` ([`Effect::Lookup`], then [`UploadTracker::learn`]).
//!
//! * Done: "Uploaded to <destination>" with "Copy link" (when there is a link) and "Open" (the link
//!   for a cloud destination, the file in the file manager for a local folder).
//! * Failed: "Upload to <destination> failed" with the reason and "Retry" (`retry_job`).
//! * `auth_expired`: one "Reconnect needed for <destination>" toast per destination until an
//!   upload to it runs again, with a button that opens Settings → Destinations.

use std::collections::{HashMap, HashSet};

use cm_i18n::{tr, tr_ctx};
use serde_json::Value;

use crate::clipboard::TextClipboard;
use crate::toast::{ToastAction, ToastKind, ToastSpec};

/// What the tracker needs to know about a destination, from `destinations.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DestInfo {
    pub name: String,
    /// A folder on this computer (its files are shown in the file manager, there is no link).
    pub local: bool,
}

/// Looks a destination up by id.
pub type Lookup<'a> = &'a dyn Fn(&str) -> Option<DestInfo>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Running,
    /// Waiting for the account to sign in again.
    Paused,
    Done,
    Failed,
}

/// One upload job as the UI sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct UploadStatus {
    pub job_id: String,
    pub capture_id: Option<String>,
    pub title: Option<String>,
    pub destination_id: Option<String>,
    pub destination: Option<String>,
    /// 0..=1.
    pub progress: f64,
    pub phase: Phase,
    pub share_link: Option<String>,
    pub reason: Option<String>,
    /// Order of arrival.
    pub seq: u64,
}

/// What the window glue should do after an event.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    Done {
        job_id: String,
        capture_id: Option<String>,
        title: Option<String>,
        destination: String,
        /// A local destination: the file that was written.
        local_path: Option<String>,
        share_link: Option<String>,
        /// Something the user should know that is not a failure (for example: public links are
        /// forbidden, so this is an organization link).
        note: Option<String>,
    },
    Failed {
        job_id: String,
        title: Option<String>,
        destination: String,
        reason: String,
    },
    Reconnect {
        destination_id: String,
        destination: String,
    },
    /// The job is not known: ask the daemon (`get_job`), call [`UploadTracker::learn`] with the
    /// answer, then apply the same event again.
    Lookup { job_id: String },
}

/// The running and recently finished uploads.
#[derive(Debug, Default)]
pub struct UploadTracker {
    jobs: HashMap<String, UploadStatus>,
    next_seq: u64,
    /// Jobs a lookup was already asked for (a failed lookup must not loop).
    asked: HashSet<String>,
    /// Destinations the user was told to reconnect, until an upload to them runs again.
    reconnect_told: HashSet<String>,
    /// Destinations with an upload failed or paused as `auth_expired` that the user has not yet
    /// dismissed, retried or reconnected: the tray's Attention state while it is not empty.
    attention: HashSet<String>,
}

/// The `error.data.reason` of a `job.failed` event (the readable reason), else its message.
fn failure_reason(data: &Value) -> String {
    let e = &data["error"];
    e["data"]["reason"]
        .as_str()
        .or_else(|| e["message"].as_str())
        .map(str::to_owned)
        .unwrap_or_else(|| tr!("The upload failed."))
}

fn text(v: &Value) -> Option<String> {
    v.as_str().filter(|s| !s.is_empty()).map(str::to_owned)
}

impl UploadTracker {
    fn insert(&mut self, mut s: UploadStatus) {
        self.next_seq += 1;
        s.seq = self.next_seq;
        self.jobs.insert(s.job_id.clone(), s);
    }

    /// Registers a job the app did not see being queued, from a `get_job` / `list_jobs` row.
    /// Anything that is not an upload job is ignored.
    pub fn learn(&mut self, job: &Value) {
        let Some(job_id) = text(&job["job_id"]) else {
            return;
        };
        if job["type"] != "upload" || self.jobs.contains_key(&job_id) {
            return;
        }
        let phase = match job["state"].as_str() {
            Some("done") => Phase::Done,
            Some("failed") => Phase::Failed,
            Some("paused") if job["pause_reason"] == "auth_expired" => Phase::Paused,
            _ => Phase::Running,
        };
        self.insert(UploadStatus {
            job_id,
            capture_id: text(&job["inputs"]["capture_id"]),
            title: None,
            destination_id: text(&job["inputs"]["destination_id"]),
            destination: None,
            progress: job["progress"].as_f64().unwrap_or(0.0).clamp(0.0, 1.0),
            phase,
            share_link: None,
            reason: None,
            seq: 0,
        });
    }

    fn name_of(&self, s: &UploadStatus, lookup: Lookup) -> String {
        s.destination_id
            .as_deref()
            .and_then(lookup)
            .map(|d| d.name)
            .or_else(|| s.destination.clone())
            .unwrap_or_else(|| tr_ctx!("fallback name", "the destination"))
    }

    /// Applies one daemon event; returns what to show.
    pub fn on_event(&mut self, name: &str, data: &Value, lookup: Lookup) -> Vec<Effect> {
        let job_id = text(&data["job_id"]).unwrap_or_default();
        if name == "upload.queued" {
            self.insert(UploadStatus {
                job_id,
                capture_id: text(&data["capture_id"]),
                title: text(&data["title"]),
                destination_id: text(&data["destination_id"]),
                destination: text(&data["destination"]),
                progress: 0.0,
                phase: Phase::Running,
                share_link: None,
                reason: None,
                seq: 0,
            });
            return Vec::new();
        }
        if data["type"] != "upload" || job_id.is_empty() {
            return Vec::new();
        }
        // `job.done` carries everything it needs; the others need to know the job.
        if name != "job.done" && !self.jobs.contains_key(&job_id) {
            return if self.asked.insert(job_id.clone()) {
                vec![Effect::Lookup { job_id }]
            } else {
                Vec::new()
            };
        }
        match name {
            "job.progress" => {
                let Some(j) = self.jobs.get_mut(&job_id) else {
                    return Vec::new();
                };
                j.progress = data["progress"].as_f64().unwrap_or(0.0).clamp(0.0, 1.0);
                // Running again: after Reconnect, or after Retry.
                if j.phase != Phase::Done {
                    j.phase = Phase::Running;
                }
                // An upload to this destination runs again: a later `auth_expired` is news again.
                if let Some(d) = j.destination_id.clone() {
                    self.reconnect_told.remove(&d);
                    self.attention.remove(&d);
                }
                Vec::new()
            }
            "job.done" => self.done(&job_id, data, lookup),
            "job.failed" => {
                let reason = failure_reason(data);
                let (dest_id, dest_name, title) = {
                    let j = self.jobs.get_mut(&job_id).expect("known job");
                    j.phase = Phase::Failed;
                    j.reason = Some(reason.clone());
                    (
                        j.destination_id.clone(),
                        j.destination.clone(),
                        j.title.clone(),
                    )
                };
                let status = self.jobs[&job_id].clone();
                let destination = self.name_of(&status, lookup);
                // A sign-in that is gone is not a failure to retry: reconnect first.
                if data["error"]["code"] == "auth_expired" {
                    return self.reconnect(dest_id, dest_name.or(Some(destination)));
                }
                vec![Effect::Failed {
                    job_id,
                    title,
                    destination,
                    reason,
                }]
            }
            "job.paused" if data["pause_reason"] == "auth_expired" => {
                let status = {
                    let j = self.jobs.get_mut(&job_id).expect("known job");
                    j.phase = Phase::Paused;
                    j.clone()
                };
                let destination = self.name_of(&status, lookup);
                self.reconnect(status.destination_id, Some(destination))
            }
            _ => Vec::new(),
        }
    }

    fn reconnect(&mut self, destination_id: Option<String>, name: Option<String>) -> Vec<Effect> {
        let id = destination_id.unwrap_or_default();
        self.attention.insert(id.clone());
        if !self.reconnect_told.insert(id.clone()) {
            return Vec::new();
        }
        vec![Effect::Reconnect {
            destination_id: id,
            destination: name.unwrap_or_else(|| tr_ctx!("fallback name", "the destination")),
        }]
    }

    fn done(&mut self, job_id: &str, data: &Value, lookup: Lookup) -> Vec<Effect> {
        let result = &data["result"];
        let dest_id = text(&result["destination_id"]);
        if !self.jobs.contains_key(job_id) {
            self.insert(UploadStatus {
                job_id: job_id.to_owned(),
                capture_id: text(&result["capture_id"]),
                title: None,
                destination_id: dest_id.clone(),
                destination: None,
                progress: 1.0,
                phase: Phase::Done,
                share_link: None,
                reason: None,
                seq: 0,
            });
        }
        let share_link = text(&result["share_link"]);
        let (title, capture_id, known_name) = {
            let j = self.jobs.get_mut(job_id).expect("inserted above");
            j.phase = Phase::Done;
            j.progress = 1.0;
            j.share_link.clone_from(&share_link);
            if j.destination_id.is_none() {
                j.destination_id.clone_from(&dest_id);
            }
            (j.title.clone(), j.capture_id.clone(), j.destination.clone())
        };
        if let Some(d) = &dest_id {
            self.reconnect_told.remove(d);
            self.attention.remove(d);
        }
        let info = dest_id.as_deref().and_then(lookup);
        let destination = info
            .as_ref()
            .map(|d| d.name.clone())
            .or_else(|| text(&result["destination"]))
            .or(known_name)
            .unwrap_or_else(|| tr_ctx!("fallback name", "the destination"));
        let local = info.as_ref().is_some_and(|d| d.local);
        // A local destination answers with the path it wrote as the file's id.
        let local_path = local
            .then(|| text(&result["files"][0]["remote_id"]))
            .flatten();
        let note = result["notes"]
            .as_array()
            .and_then(|n| n.iter().find_map(text));
        vec![Effect::Done {
            job_id: job_id.to_owned(),
            capture_id,
            title,
            destination,
            local_path,
            share_link,
            note,
        }]
    }

    /// The uploads of a capture that are not finished, oldest first (the details page).
    pub fn running_of(&self, capture_id: &str) -> Vec<&UploadStatus> {
        let mut v: Vec<&UploadStatus> = self
            .jobs
            .values()
            .filter(|j| {
                j.capture_id.as_deref() == Some(capture_id)
                    && matches!(j.phase, Phase::Running | Phase::Paused)
            })
            .collect();
        v.sort_by_key(|j| j.seq);
        v
    }

    /// Keeps memory bounded over a long session: at most `keep` finished jobs stay.
    pub fn prune(&mut self, keep: usize) {
        let mut finished: Vec<(u64, String)> = self
            .jobs
            .values()
            .filter(|j| matches!(j.phase, Phase::Done | Phase::Failed))
            .map(|j| (j.seq, j.job_id.clone()))
            .collect();
        finished.sort_unstable();
        let drop = finished.len().saturating_sub(keep);
        for (_, id) in finished.into_iter().take(drop) {
            self.jobs.remove(&id);
            self.asked.remove(&id);
        }
    }

    /// The user signed in again: the next `auth_expired` for the destination is news again.
    pub fn reconnected(&mut self, destination_id: &str) {
        self.reconnect_told.remove(destination_id);
        self.attention.remove(destination_id);
    }

    /// The tray's Attention rule: an upload to some destination is failed or paused as
    /// `auth_expired` and the user has not yet dismissed the "Reconnect needed" toast, retried the
    /// upload or signed in again.
    pub fn needs_attention(&self) -> bool {
        !self.attention.is_empty()
    }

    /// The user dismissed the "Reconnect needed" toast of a destination: it was seen.
    pub fn acknowledged(&mut self, destination_id: &str) {
        self.attention.remove(destination_id);
    }
}

// ---- copy the link when done (STO-4.AC1) -------------------------------------------------------

/// Whether `storage.copy_link_when_done` is on in a settings snapshot (absent means on).
pub fn copy_link_enabled(settings: &serde_json::Map<String, Value>) -> bool {
    settings
        .get(crate::settings::KEY_COPY_LINK_WHEN_DONE)
        .and_then(Value::as_bool)
        .unwrap_or(true)
}

/// What [`auto_copy`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutoCopy {
    /// The link is on the clipboard (the toast says so).
    Copied,
    /// The setting is off, or the upload has no link (a local folder, link mode none).
    Skipped,
    /// The clipboard refused; the toast keeps its "Copy link" button.
    Failed(String),
}

/// A finished upload with a share link puts it on the clipboard when `enabled`. On success the
/// toast gets the note "Link copied to the clipboard". Anything that is not a finished upload with a
/// link is left alone.
pub fn auto_copy(effect: &mut Effect, enabled: bool, clipboard: &dyn TextClipboard) -> AutoCopy {
    let Effect::Done {
        share_link: Some(link),
        note,
        ..
    } = effect
    else {
        return AutoCopy::Skipped;
    };
    if !enabled {
        return AutoCopy::Skipped;
    }
    if let Err(e) = clipboard.set_text(link) {
        return AutoCopy::Failed(e);
    }
    let copied = tr!("Link copied to the clipboard");
    *note = Some(match note.take() {
        Some(n) => format!("{n} \u{00B7} {copied}"),
        None => copied,
    });
    AutoCopy::Copied
}

// ---- toasts ----------------------------------------------------------------------------------------

/// What the toast of an [`Effect`] is, and the key under which it is shown (a newer toast with the
/// same key replaces the older one in place).
pub fn toast_for(effect: &Effect) -> Option<(String, ToastSpec)> {
    match effect {
        Effect::Done {
            job_id,
            title,
            destination,
            local_path,
            share_link,
            note,
            ..
        } => Some((
            format!("upload:{job_id}"),
            uploaded_toast(
                title.as_deref(),
                destination,
                local_path.as_deref(),
                share_link.as_deref(),
                note.as_deref(),
            ),
        )),
        Effect::Failed {
            job_id,
            title,
            destination,
            reason,
        } => Some((
            format!("upload:{job_id}"),
            upload_failed_toast(job_id, title.as_deref(), destination, reason),
        )),
        Effect::Reconnect {
            destination_id,
            destination,
        } => Some((
            format!("reconnect:{destination_id}"),
            reconnect_toast(destination),
        )),
        Effect::Lookup { .. } => None,
    }
}

/// "Uploaded to <destination>" with "Copy link" and "Open". Without a link a cloud upload has
/// nothing to open; a local one is shown in the file manager.
pub fn uploaded_toast(
    title: Option<&str>,
    destination: &str,
    local_path: Option<&str>,
    share_link: Option<&str>,
    note: Option<&str>,
) -> ToastSpec {
    let head = if local_path.is_some() {
        tr!("Copied to {destination}", destination = destination)
    } else {
        tr!("Uploaded to {destination}", destination = destination)
    };
    let mut spec = ToastSpec::new(ToastKind::Success, head);
    let mut body = Vec::new();
    if let Some(t) = title {
        body.push(t.to_owned());
    }
    if let Some(n) = note {
        body.push(n.to_owned());
    }
    if !body.is_empty() {
        spec = spec.body(body.join(" \u{00B7} "));
    }
    if let Some(link) = share_link {
        spec = spec
            .button(tr!("Copy link"), ToastAction::CopyLink(link.to_owned()))
            .button(
                tr_ctx!("button", "Open"),
                ToastAction::OpenLink(link.to_owned()),
            );
    } else if let Some(path) = local_path {
        spec = spec.button(
            tr_ctx!("button", "Open"),
            ToastAction::RevealPath(path.to_owned()),
        );
    }
    spec
}

/// [`upload_failed_toast`] as a live toast: it is worded again when the language changes.
pub fn upload_failed_toast(
    job_id: &str,
    title: Option<&str>,
    destination: &str,
    reason: &str,
) -> ToastSpec {
    let (job_id, title, destination, reason) = (
        job_id.to_owned(),
        title.map(str::to_owned),
        destination.to_owned(),
        reason.to_owned(),
    );
    ToastSpec::live(move || {
        upload_failed_toast_now(&job_id, title.as_deref(), &destination, &reason)
    })
}

/// "Upload to <destination> failed" with the reason and "Retry".
fn upload_failed_toast_now(
    job_id: &str,
    title: Option<&str>,
    destination: &str,
    reason: &str,
) -> ToastSpec {
    let body = match title {
        Some(t) => format!("{t} \u{00B7} {reason}"),
        None => reason.to_owned(),
    };
    ToastSpec::new(
        ToastKind::Error,
        tr!("Upload to {destination} failed", destination = destination),
    )
    .body(body)
    .button(
        tr_ctx!("button", "Retry"),
        ToastAction::RetryJob(job_id.to_owned()),
    )
}

/// [`reconnect_toast`] as a live toast: it is worded again when the language changes.
pub fn reconnect_toast(destination: &str) -> ToastSpec {
    let destination = destination.to_owned();
    ToastSpec::live(move || reconnect_toast_now(&destination))
}

/// "Reconnect needed for <destination>": the uploads wait and go on after signing in again.
fn reconnect_toast_now(destination: &str) -> ToastSpec {
    ToastSpec::new(
        ToastKind::Warning,
        tr!(
            "Reconnect needed for {destination}",
            destination = destination
        ),
    )
    .body(tr!(
        "Uploads are waiting. They go on after you sign in again."
    ))
    .button(
        tr_ctx!("button", "Reconnect\u{2026}"),
        ToastAction::OpenDestinations,
    )
}

/// The details page line of a running upload: "Uploading to Google Drive: 40%".
pub fn running_text(s: &UploadStatus, destination: &str) -> String {
    match s.phase {
        Phase::Paused => tr!(
            "Waiting for you to reconnect {destination}",
            destination = destination
        ),
        _ => tr!(
            "Uploading to {destination}: {pct}%",
            destination = destination,
            pct = (s.progress * 100.0).floor() as u32
        ),
    }
}

#[cfg(test)]
mod copy_tests;
#[cfg(test)]
mod tests;
