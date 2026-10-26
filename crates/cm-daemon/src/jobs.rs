//! Wiring of the persistent job queue (`cm-jobs`) into the daemon (EXP-4, STO-3, NFR-8).
//!
//! * The daemon *owns* an optional [`JobQueue`] ([`crate::DaemonBuilder::jobs`]). Without one the
//!   job actions fail with `not_implemented` (see [`NO_QUEUE`]); nothing is created implicitly,
//!   so embedders and tests that never asked for jobs get no files and no background tasks.
//! * [`library_job_queue`] opens the default queue: `<library root>/.capturemann/jobs.sqlite`.
//!   It is a dedicated file, not the library index: the index is a rebuildable cache of the
//!   bundles, a job (inputs, checkpoint, retry state) is not.
//! * [`crate::Daemon::start_jobs`] subscribes the event bridge, runs `recover()` (jobs left
//!   `running` by a crash go back to `queued`, EXP-4.AC3) and starts the worker. It is
//!   idempotent, and also runs lazily on the first job action. The builder cannot do it because
//!   `build()` may be called outside a tokio runtime (the desktop app does).
//! * [`crate::Daemon::shutdown_jobs`] asks running handlers to checkpoint and waits for them, at
//!   most [`DEFAULT_JOBS_SHUTDOWN_TIMEOUT`] (10 s); a handler that does not stop is abandoned and
//!   its job resumes at the next start.
//!   Dropping the last `Daemon` clone does the same in the background when a runtime exists.
//!
//! # Bus events
//!
//! `cm-jobs` events are bridged with [`cm_jobs::JobEvent::bus_event_name`]: `job.progress`
//! (`{job_id, type, progress, message}`), `job.done` (`{job_id, type, state, result}`) and
//! `job.failed` (`{job_id, type, state, error}`, `error` being the shared error payload). A
//! cancelled job emits **no** bus event (the spec lists only those three); clients learn about
//! cancellation from `get_job` / `list_jobs` (`state: "cancelled"`) and `cancel_job`'s result.
//! The job's `checkpoint` is never part of any result or event (it can hold upload session
//! URLs).

use async_trait::async_trait;
use cm_core::CmError;
use cm_jobs::{Job, JobEvent as QueueEvent, JobFilter, JobQueue, JobState, QueueConfig, Worker};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use tokio::sync::broadcast;
use tokio::sync::broadcast::error::RecvError;

use crate::action::{CancelJobParams, GetJobParams, ListJobsParams, RetryJobParams};
use crate::daemon::{blocking, Inner};
use crate::error::DaemonError;
use crate::events::{JobEvent, JobEventKind, JobEvents};

/// Where the default queue lives, relative to the library root.
pub const JOBS_RELATIVE_PATH: &str = ".capturemann/jobs.sqlite";

pub(crate) const NO_QUEUE: &str = "background jobs are not available: this daemon has no job queue";

/// Path of the default job database for a library root.
pub fn library_jobs_path(library_root: &Path) -> PathBuf {
    library_root.join(JOBS_RELATIVE_PATH)
}

/// Opens (creating it if needed) the default job queue of a library, with the spec's
/// concurrency and retry defaults ([`QueueConfig::default`]).
pub fn library_job_queue(library_root: &Path) -> Result<JobQueue, CmError> {
    let path = library_jobs_path(library_root);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|e| CmError::internal(format!("cannot create `{}`: {e}", dir.display())))?;
    }
    JobQueue::open(path, QueueConfig::default())
}

/// Adapts the queue's typed events to the bus bridge. Events without a bus name
/// (queued, started, state changes, recovered, cancelled) are skipped.
struct QueueEvents(broadcast::Receiver<QueueEvent>);

#[async_trait]
impl JobEvents for QueueEvents {
    async fn next_event(&mut self) -> Option<JobEvent> {
        loop {
            let ev = match self.0.recv().await {
                Ok(ev) => ev,
                // A slow bus loses progress ticks, never the stream.
                Err(RecvError::Lagged(_)) => continue,
                Err(RecvError::Closed) => return None,
            };
            if let Some(mapped) = map_event(&ev) {
                return Some(mapped);
            }
        }
    }
}

fn map_event(ev: &QueueEvent) -> Option<JobEvent> {
    // The one pause that needs the user: the account must sign in again.
    if let QueueEvent::StateChanged {
        job_id,
        job_type,
        to: JobState::Paused,
        reason: Some(reason),
        ..
    } = ev
    {
        return (reason == cm_jobs::PAUSE_AUTH_EXPIRED).then(|| JobEvent {
            kind: JobEventKind::Paused,
            job_id: job_id.clone(),
            data: json!({ "type": job_type, "state": "paused", "pause_reason": reason }),
        });
    }
    ev.bus_event_name()?;
    Some(match ev {
        QueueEvent::Progress {
            job_id,
            job_type,
            progress,
            message,
        } => JobEvent {
            kind: JobEventKind::Progress,
            job_id: job_id.clone(),
            data: json!({ "type": job_type, "progress": progress, "message": message }),
        },
        QueueEvent::Finished {
            job_id,
            job_type,
            state,
            result,
            error,
        } => JobEvent {
            kind: if *state == JobState::Done {
                JobEventKind::Done
            } else {
                JobEventKind::Failed
            },
            job_id: job_id.clone(),
            data: if *state == JobState::Done {
                json!({ "type": job_type, "state": state.as_str(), "result": result })
            } else {
                json!({ "type": job_type, "state": state.as_str(), "error": error })
            },
        },
        _ => return None,
    })
}

enum Phase {
    NotStarted,
    Running {
        worker: Worker,
        bridge: tokio::task::JoinHandle<()>,
    },
    Stopped,
}

/// The queue plus the lifecycle of its worker and bridge.
pub(crate) struct JobsRuntime {
    pub(crate) queue: JobQueue,
    phase: Mutex<Phase>,
}

impl JobsRuntime {
    pub(crate) fn new(queue: JobQueue) -> Self {
        JobsRuntime {
            queue,
            phase: Mutex::new(Phase::NotStarted),
        }
    }

    fn phase(&self) -> std::sync::MutexGuard<'_, Phase> {
        self.phase.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Subscribe the bridge, `recover()`, start the worker. No-op unless never started.
    pub(crate) fn start(&self, inner: &Inner) -> Result<(), CmError> {
        let mut phase = self.phase();
        if !matches!(*phase, Phase::NotStarted) {
            return Ok(());
        }
        let handle = tokio::runtime::Handle::try_current()
            .map_err(|_| CmError::internal("the job queue needs a tokio runtime to start"))?;
        let bus = inner.events.clone();
        let mut source = QueueEvents(self.queue.subscribe());
        // Subscribed before `recover()` so recovery progress is not missed.
        let bridge = handle.spawn(async move {
            while let Some(ev) = source.next_event().await {
                crate::events::publish_job_event(&bus, ev);
            }
        });
        let recovered = self.queue.recover()?;
        republish_export_queued(inner, &self.queue, &recovered);
        let worker = {
            let _guard = handle.enter();
            self.queue.spawn_worker()
        };
        *phase = Phase::Running { worker, bridge };
        Ok(())
    }

    /// Stops the worker (handlers checkpoint; their jobs return to `queued`), waiting at most
    /// `limit` for them; a handler still running then is abandoned and its job is re-queued
    /// by `recover()` at the next start.
    pub(crate) async fn shutdown(&self, limit: std::time::Duration) {
        let running = std::mem::replace(&mut *self.phase(), Phase::Stopped);
        if let Phase::Running { worker, bridge } = running {
            if !worker.shutdown_timeout(Some(limit)).await {
                log::warn!(
                    "a job handler did not stop within {limit:?}; abandoning it \
                     (the job resumes at the next start)"
                );
            }
            // Let the bridge flush what the handlers reported while stopping.
            tokio::task::yield_now().await;
            bridge.abort();
        }
    }
}

impl Drop for JobsRuntime {
    fn drop(&mut self) {
        let phase = std::mem::replace(
            &mut *self.phase.lock().unwrap_or_else(PoisonError::into_inner),
            Phase::Stopped,
        );
        if let Phase::Running { worker, bridge } = phase {
            bridge.abort();
            if let Ok(h) = tokio::runtime::Handle::try_current() {
                h.spawn(worker.shutdown());
            }
        }
    }
}

/// How long [`crate::Daemon::shutdown_jobs`] waits for running handlers by default.
pub const DEFAULT_JOBS_SHUTDOWN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// EXP-4.AC3: after a restart the surviving export jobs (recovered from `running`, or still
/// queued or held) resume on their own, but no client saw them being queued in this process.
/// Announce each one again as `export.queued` with `recovered: true` so the UI shows it.
fn republish_export_queued(inner: &Inner, queue: &JobQueue, recovered: &[String]) {
    let Ok(jobs) = queue.list(&JobFilter {
        job_type: Some(cm_jobs::JOB_EXPORT.to_owned()),
        states: Some(vec![JobState::Queued, JobState::Paused, JobState::Running]),
        limit: Some(200),
    }) else {
        return;
    };
    // Oldest first, as they were queued.
    for job in jobs.iter().rev() {
        let capture_id = job
            .inputs
            .get("capture_id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let title = crate::daemon::lock_library(&inner.library)
            .get(capture_id)
            .map_or_else(|_| capture_id.to_owned(), |r| r.title);
        inner.events.publish(
            crate::events::EXPORT_QUEUED,
            json!({
                "job_id": job.id,
                "capture_id": capture_id,
                "title": title,
                "preset": job.inputs.get("preset").cloned().unwrap_or(Value::Null),
                "auto": job.inputs.get("auto").and_then(Value::as_bool).unwrap_or(false),
                "recovered": true,
                "was_interrupted": recovered.contains(&job.id),
            }),
        );
    }
}

fn rfc3339(ms: i64) -> Value {
    OffsetDateTime::from_unix_timestamp_nanos(i128::from(ms) * 1_000_000)
        .ok()
        .and_then(|t| t.format(&Rfc3339).ok())
        .map_or(Value::Null, Value::String)
}

/// The agent-visible job. No `checkpoint`, ever.
pub(crate) fn job_json(job: &Job, now_ms: i64, held: bool) -> Value {
    let elapsed_ms = job.started_at_ms.map(|s| {
        job.finished_at_ms
            .unwrap_or(now_ms)
            .saturating_sub(s)
            .max(0)
    });
    let eta_ms = match (job.state, elapsed_ms) {
        (JobState::Running, Some(e)) if job.progress >= 0.01 && job.progress < 1.0 => {
            #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
            let eta = (e as f64 * (1.0 - job.progress) / job.progress) as i64;
            Some(eta)
        }
        _ => None,
    };
    json!({
        "job_id": job.id,
        "type": job.job_type,
        "state": job.state.as_str(),
        "progress": job.progress,
        "message": job.message,
        "attempts": job.attempts,
        "depends_on": job.depends_on,
        "inputs": job.inputs,
        "result": job.result,
        "error": job.error,
        "pause_reason": job.pause_reason,
        // A recording holds exports and uploads (EXP-4.AC2): the job waits, it is not stuck.
        "held": held && matches!(job.state, JobState::Queued | JobState::Paused),
        "created_at": rfc3339(job.created_at_ms),
        "started_at": job.started_at_ms.map_or(Value::Null, rfc3339),
        "finished_at": job.finished_at_ms.map_or(Value::Null, rfc3339),
        "elapsed_ms": elapsed_ms,
        "eta_ms": eta_ms,
    })
}

impl Inner {
    pub(crate) fn job_queue(&self) -> Result<&JobsRuntime, DaemonError> {
        let rt = self
            .jobs
            .as_ref()
            .ok_or_else(|| DaemonError::NotImplemented(NO_QUEUE.to_owned()))?;
        rt.start(self)?;
        Ok(rt)
    }

    pub(crate) async fn get_job(&self, p: GetJobParams) -> Result<Value, DaemonError> {
        let q = self.job_queue()?.queue.clone();
        blocking(move || {
            let job = q.get(&p.job_id)?;
            Ok(job_json(
                &job,
                q.config().clock.now_ms(),
                q.is_type_held(&job.job_type),
            ))
        })
        .await
    }

    pub(crate) async fn list_jobs(&self, p: ListJobsParams) -> Result<Value, DaemonError> {
        let q = self.job_queue()?.queue.clone();
        blocking(move || {
            let jobs = q.list(&JobFilter {
                job_type: p.job_type,
                states: p.state.map(|s| vec![s]),
                limit: Some(p.limit),
            })?;
            let now = q.config().clock.now_ms();
            let jobs: Vec<Value> = jobs
                .iter()
                .map(|j| job_json(j, now, q.is_type_held(&j.job_type)))
                .collect();
            Ok(json!({ "jobs": jobs }))
        })
        .await
    }

    /// Idempotent: cancelling a cancelled job returns it unchanged. A running job is signalled
    /// and reaches `cancelled` once its handler stops (`cancelling: true` until then); a done
    /// or failed job is `invalid_input`.
    pub(crate) async fn cancel_job(&self, p: CancelJobParams) -> Result<Value, DaemonError> {
        let q = self.job_queue()?.queue.clone();
        blocking(move || {
            let job = q.cancel(&p.job_id)?;
            let mut v = job_json(
                &job,
                q.config().clock.now_ms(),
                q.is_type_held(&job.job_type),
            );
            let cancelling = job.state == JobState::Running;
            v["cancelling"] = json!(cancelling);
            if cancelling {
                // R2-12: a handler that finishes its work before it notices the cancel ends
                // `done`; the reply only promises that the stop was requested.
                v["note"] = json!(
                    "Stop requested. If the job was about to finish it can still end as `done`; check get_job."
                );
            }
            Ok(v)
        })
        .await
    }

    /// Puts a `failed` or `cancelled` job back in the queue (attempts reset; a failed job keeps its
    /// checkpoint, a cancelled one starts over) and resumes one that paused with `auth_expired`.
    /// Idempotent: a job that is already queued or running is returned as it is; a `done` job or
    /// one the user paused is `invalid_input`.
    pub(crate) async fn retry_job(&self, p: RetryJobParams) -> Result<Value, DaemonError> {
        let q = self.job_queue()?.queue.clone();
        blocking(move || {
            let job = q.retry(&p.job_id)?;
            Ok(job_json(
                &job,
                q.config().clock.now_ms(),
                q.is_type_held(&job.job_type),
            ))
        })
        .await
    }
}

#[cfg(test)]
mod map_event_tests {
    use super::*;

    fn changed(to: JobState, reason: Option<&str>) -> QueueEvent {
        QueueEvent::StateChanged {
            job_id: "j1".into(),
            job_type: "upload".into(),
            from: JobState::Running,
            to,
            reason: reason.map(str::to_owned),
        }
    }

    /// STO-3 notifications: only the pause that needs the user is announced (`job.paused`).
    #[test]
    fn only_an_auth_expired_pause_reaches_the_bus() {
        let ev = map_event(&changed(JobState::Paused, Some("auth_expired"))).unwrap();
        assert_eq!(ev.kind, JobEventKind::Paused);
        assert_eq!(ev.job_id, "j1");
        assert_eq!(ev.data["pause_reason"], "auth_expired");
        assert_eq!(ev.data["type"], "upload");
        assert!(map_event(&changed(JobState::Paused, Some("user"))).is_none());
        assert!(map_event(&changed(JobState::Paused, Some("hold:recording"))).is_none());
        assert!(map_event(&changed(JobState::Queued, Some("auth_expired"))).is_none());
        assert!(map_event(&changed(JobState::Running, None)).is_none());
    }
}
