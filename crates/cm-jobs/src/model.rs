//! Job data model and events.

use cm_core::{CmError, ErrorCode, ErrorPayload};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub type JobId = String;

/// Well-known job types (the type is an open string; these are the defaults with limits).
pub const JOB_EXPORT: &str = "export";
pub const JOB_UPLOAD: &str = "upload";
pub const JOB_TRANSCRIBE: &str = "transcribe";
pub const JOB_PUBLISH: &str = "publish";
pub const JOB_AI_METADATA: &str = "ai_metadata";
/// Copies the library to a new folder (LIB-2, `move_library`).
pub const JOB_LIBRARY_MOVE: &str = "library_move";

/// Job lifecycle state. `Cancelled` is in EXP-4 ("queued -> running -> done/failed/cancelled")
/// but not in the five states of the technical design; see the crate report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Queued,
    Running,
    Paused,
    Failed,
    Done,
    Cancelled,
}

impl JobState {
    pub fn as_str(self) -> &'static str {
        match self {
            JobState::Queued => "queued",
            JobState::Running => "running",
            JobState::Paused => "paused",
            JobState::Failed => "failed",
            JobState::Done => "done",
            JobState::Cancelled => "cancelled",
        }
    }

    pub fn parse(s: &str) -> Option<JobState> {
        Some(match s {
            "queued" => JobState::Queued,
            "running" => JobState::Running,
            "paused" => JobState::Paused,
            "failed" => JobState::Failed,
            "done" => JobState::Done,
            "cancelled" => JobState::Cancelled,
            _ => return None,
        })
    }

    /// Done, failed or cancelled (a failed job can still be re-queued with `retry`).
    pub fn is_finished(self) -> bool {
        matches!(
            self,
            JobState::Done | JobState::Failed | JobState::Cancelled
        )
    }
}

/// Last error recorded on a job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobError {
    pub code: ErrorCode,
    pub reason: String,
}

/// Why a paused job is paused. `user` pauses need an explicit `resume`; `hold:*` pauses are
/// released by `resume_type` / `resume_all`.
pub const PAUSE_USER: &str = "user";
pub const PAUSE_HOLD_ALL: &str = "hold:all";
/// A handler returned `auth_expired`: the account must sign in again. The job waits (`paused`,
/// the reason in `error`), keeps its checkpoint and does not use up an attempt; `resume` (or
/// `JobQueue::resume_auth_expired` once the account is back) re-queues it. Unlike `hold:*`, a restart
/// leaves it paused.
pub const PAUSE_AUTH_EXPIRED: &str = "auth_expired";
pub fn pause_hold_type(job_type: &str) -> String {
    format!("hold:type:{job_type}")
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Job {
    pub id: JobId,
    pub job_type: String,
    pub inputs: Value,
    pub state: JobState,
    /// 0..=1.
    pub progress: f64,
    pub message: Option<String>,
    /// Number of times the job was started (interruptions by pause/shutdown/crash don't count).
    pub attempts: u32,
    pub depends_on: Vec<JobId>,
    /// Resumable state saved by the handler after each chunk. Never serialized: it can hold
    /// upload session URLs, which agents must not read.
    #[serde(skip_serializing, default)]
    pub checkpoint: Option<Value>,
    pub result: Option<Value>,
    /// Last error (kept while the job waits for a retry).
    pub error: Option<JobError>,
    pub pause_reason: Option<String>,
    /// Not eligible to start before this time (retry backoff).
    pub not_before_ms: i64,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    /// Start of the current/last attempt (basis for elapsed/ETA).
    pub started_at_ms: Option<i64>,
    pub finished_at_ms: Option<i64>,
}

impl Job {
    /// The `job_failed` error (data `job_id`, `reason`) for a failed job.
    pub fn failure(&self) -> Option<CmError> {
        if self.state != JobState::Failed {
            return None;
        }
        let reason = self
            .error
            .as_ref()
            .map_or_else(|| "unknown".to_owned(), |e| e.reason.clone());
        Some(CmError::job_failed(self.id.clone(), reason))
    }
}

/// A job to enqueue.
#[derive(Debug, Clone)]
pub struct NewJob {
    pub job_type: String,
    pub inputs: Value,
    pub depends_on: Vec<JobId>,
    /// Caller-chosen id (must be unused); generated when `None`.
    pub id: Option<JobId>,
}

impl NewJob {
    pub fn new(job_type: impl Into<String>, inputs: Value) -> Self {
        NewJob {
            job_type: job_type.into(),
            inputs,
            depends_on: Vec::new(),
            id: None,
        }
    }

    pub fn depends_on(mut self, id: impl Into<JobId>) -> Self {
        self.depends_on.push(id.into());
        self
    }

    pub fn with_id(mut self, id: impl Into<JobId>) -> Self {
        self.id = Some(id.into());
        self
    }
}

/// Filter for `list_jobs` (newest first).
#[derive(Debug, Clone, Default)]
pub struct JobFilter {
    pub job_type: Option<String>,
    pub states: Option<Vec<JobState>>,
    pub limit: Option<usize>,
}

/// Typed queue events; `cm-daemon` bridges them onto the action bus.
#[derive(Debug, Clone, PartialEq)]
pub enum JobEvent {
    Queued {
        job_id: JobId,
        job_type: String,
    },
    Started {
        job_id: JobId,
        job_type: String,
        attempt: u32,
    },
    Progress {
        job_id: JobId,
        job_type: String,
        progress: f64,
        message: Option<String>,
    },
    StateChanged {
        job_id: JobId,
        job_type: String,
        from: JobState,
        to: JobState,
        reason: Option<String>,
    },
    /// Done, failed or cancelled. `error` is the `job_failed` payload for failures.
    Finished {
        job_id: JobId,
        job_type: String,
        state: JobState,
        result: Option<Value>,
        error: Option<ErrorPayload>,
    },
    /// Job left `running` by a crash/quit and was re-queued (EXP-4.AC3: tell the user).
    Recovered {
        job_id: JobId,
        job_type: String,
    },
}

impl JobEvent {
    pub fn job_id(&self) -> &str {
        match self {
            JobEvent::Queued { job_id, .. }
            | JobEvent::Started { job_id, .. }
            | JobEvent::Progress { job_id, .. }
            | JobEvent::StateChanged { job_id, .. }
            | JobEvent::Finished { job_id, .. }
            | JobEvent::Recovered { job_id, .. } => job_id,
        }
    }

    /// Name of the agent-interface event this maps to (`job.progress`, `job.done`,
    /// `job.failed`), if any. Other events are internal to the GUI/CLI.
    pub fn bus_event_name(&self) -> Option<&'static str> {
        match self {
            JobEvent::Progress { .. } => Some("job.progress"),
            JobEvent::Finished {
                state: JobState::Done,
                ..
            } => Some("job.done"),
            JobEvent::Finished {
                state: JobState::Failed,
                ..
            } => Some("job.failed"),
            _ => None,
        }
    }
}
