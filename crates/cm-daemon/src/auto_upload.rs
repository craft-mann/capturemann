//! Automatic upload (STO-3), provider-agnostic.
//!
//! Each destination has an [`AutoUpload`] mode (`destinations.json`, written by the app):
//!
//! * `on_stop`: when a recording stops normally (same trigger and guards as the auto-export:
//!   never for a discard, a cancelled countdown or the 60 fps self-test) an `upload` job of the
//!   destination's `what` is queued at once (STO-3.AC1).
//! * `on_export`: when an `export` job of a capture is queued (by `export_video` or by the
//!   auto-export), an `upload` job that **depends on it** is queued, so it starts only after
//!   the export succeeded (STO-3.AC2).
//!
//! If the item contains an export (`what` is `export` or `both`) and `on_stop` is used, the job
//! depends on the export of the same stop (the auto-export's job, or a default `quick` export
//! queued here when the auto-export is off). When that export fails, `cm-jobs` fails the upload
//! with the reason `dependency <job id> failed`: it is skipped and says why, and `retry` of the
//! export brings the upload back. When no export can be made at all (no exporter registered,
//! no FFmpeg) the upload is not queued and `upload.skipped` carries the reason.
//!
//! Idempotent: a capture, destination and `what` never has two active (queued, running or
//! paused) automatic uploads. Uploads are held while a recording runs by the recording engine
//! (`storage.pause_uploads_while_recording`), like any `upload` job.

use std::sync::Mutex;

use cm_core::CmError;
use cm_jobs::{Job, JobFilter, JobQueue, JobState, NewJob, JOB_UPLOAD};
use cm_storage::{AutoUpload, Destination, DestinationStore, What};
use serde_json::{json, Value};

use crate::events::{EventBus, UPLOAD_QUEUED, UPLOAD_SKIPPED};

/// What an automatic upload came of.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// A new job was queued.
    Queued {
        destination_id: String,
        job_id: String,
    },
    /// An active job for the same capture, destination and `what` already exists.
    AlreadyQueued {
        destination_id: String,
        job_id: String,
    },
    /// Not queued; `reason` says why (also published as `upload.skipped`).
    Skipped {
        destination_id: String,
        reason: String,
    },
}

/// What the caller has for the export part of an upload.
pub(crate) enum ExportState<'a> {
    /// The upload waits for this export job.
    Job(&'a Job),
    /// No export exists and none can be made; the reason is shown.
    Unavailable(String),
}

/// Queues the automatic uploads of `trigger` for a capture. `export` is the export job the
/// upload may wait for. Returns one outcome per destination that has the trigger on.
#[allow(clippy::too_many_arguments)]
pub(crate) fn enqueue(
    queue: &JobQueue,
    store: &DestinationStore,
    events: &EventBus,
    capture_id: &str,
    title: &str,
    by: &str,
    trigger: AutoUpload,
    export: &ExportState<'_>,
) -> Result<Vec<Outcome>, CmError> {
    if trigger == AutoUpload::Off || !queue.has_handler(JOB_UPLOAD) {
        return Ok(Vec::new());
    }
    // List-then-enqueue is one step per process (as for exports), so two triggers at once
    // (the stop and a manual export, say) cannot create two jobs.
    static ENQUEUE_LOCK: Mutex<()> = Mutex::new(());
    let _one_at_a_time = ENQUEUE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let destinations: Vec<Destination> = store
        .list()?
        .into_iter()
        .filter(|d| d.auto_upload == trigger)
        .collect();
    let mut out = Vec::new();
    for d in destinations {
        out.push(enqueue_one(
            queue, events, &d, capture_id, title, by, trigger, export,
        )?);
    }
    Ok(out)
}

#[allow(clippy::too_many_arguments)]
fn enqueue_one(
    queue: &JobQueue,
    events: &EventBus,
    d: &Destination,
    capture_id: &str,
    title: &str,
    by: &str,
    trigger: AutoUpload,
    export: &ExportState<'_>,
) -> Result<Outcome, CmError> {
    let what: What = d.what;
    let skip = |reason: String| {
        events.publish(
            UPLOAD_SKIPPED,
            json!({ "capture_id": capture_id, "title": title, "destination_id": d.id,
                    "destination": d.name, "reason": reason }),
        );
        Outcome::Skipped {
            destination_id: d.id.clone(),
            reason,
        }
    };
    // `on_export` always waits for the export that triggered it; `on_stop` only when the item
    // contains an export.
    let wants_dep = trigger == AutoUpload::OnExport || what.wants_export();
    let dep = match (wants_dep, export) {
        (false, _) => None,
        (true, ExportState::Job(j)) => Some((*j).clone()),
        (true, ExportState::Unavailable(why)) => {
            if what.wants_export() {
                return Ok(skip(format!("{why}; nothing to upload yet")));
            }
            None
        }
    };

    let what_str = serde_json::to_value(what)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default();
    let active = queue.list(&JobFilter {
        job_type: Some(JOB_UPLOAD.to_owned()),
        states: Some(vec![JobState::Queued, JobState::Running, JobState::Paused]),
        limit: Some(500),
    })?;
    if let Some(j) = active.iter().find(|j| {
        j.inputs.get("capture_id").and_then(Value::as_str) == Some(capture_id)
            && j.inputs.get("destination_id").and_then(Value::as_str) == Some(d.id.as_str())
            && j.inputs.get("what").and_then(Value::as_str) == Some(what_str.as_str())
    }) {
        return Ok(Outcome::AlreadyQueued {
            destination_id: d.id.clone(),
            job_id: j.id.clone(),
        });
    }

    let mut new = NewJob::new(
        JOB_UPLOAD,
        json!({
            "destination_id": d.id,
            "folder": null,
            "by": by,
            "capture_id": capture_id,
            "what": what,
            "auto": true,
            "trigger": trigger.as_str(),
        }),
    );
    if let Some(e) = &dep {
        new = new.depends_on(e.id.clone());
    }
    let job = queue.enqueue(new)?;
    events.publish(
        UPLOAD_QUEUED,
        json!({ "job_id": job.id, "capture_id": capture_id, "title": title,
                "destination_id": d.id, "destination": d.name, "what": what,
                "trigger": trigger.as_str(), "export_job_id": dep.as_ref().map(|e| e.id.clone()) }),
    );
    Ok(Outcome::Queued {
        destination_id: d.id.clone(),
        job_id: job.id,
    })
}
