//! Persistent job queue, retries, progress events.
//!
//! Jobs (export, transcribe, upload, publish, ...) live in SQLite so an app restart, a crash or
//! an agent never loses work (NFR-8, EXP-4, STO-3). Execution is delegated to a
//! [`JobHandler`] registered per job type; this crate has no export/upload logic of its own.
//!
//! - [`JobQueue`]: enqueue/get/list/cancel/pause/resume/retry, holds (`pause_type`),
//!   `recover`, and the typed [`JobEvent`] broadcast.
//! - [`Worker`]: tokio scheduler with per-type concurrency limits, exponential backoff with
//!   jitter for retryable (`network`) errors, and immediate failure with a readable reason
//!   otherwise.

#![cfg_attr(test, allow(clippy::unwrap_used))]

pub mod backoff;
pub mod clock;
pub mod handler;
pub mod model;
mod queue;
pub mod scrub;
pub mod store;

pub use backoff::BackoffPolicy;
pub use clock::{Clock, FixedJitter, Jitter, ManualClock, RandomJitter, SystemClock};
pub use handler::{handler_fn, CancelToken, Interrupt, JobContext, JobHandler, ProgressReporter};
pub use model::{
    pause_hold_type, Job, JobError, JobEvent, JobFilter, JobId, JobState, NewJob, JOB_AI_METADATA,
    JOB_EXPORT, JOB_PUBLISH, JOB_TRANSCRIBE, JOB_UPLOAD, PAUSE_AUTH_EXPIRED, PAUSE_HOLD_ALL,
    PAUSE_USER,
};
pub use queue::{JobQueue, QueueConfig, Worker};
pub use scrub::scrub_secrets;
pub use store::JobStore;
