//! Plug-in surface for later crates: `JobHandler`, `JobContext`, `ProgressReporter`,
//! `CancelToken`.

use std::future::Future;
use std::sync::Arc;

use cm_core::CmError;
use serde_json::Value;
use tokio::sync::watch;

use crate::model::{Job, JobEvent};
use crate::queue::Inner;

/// Why a running handler is being asked to stop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interrupt {
    /// `cancel_job`: clean up partial output (EXP-4.AC4) and stop.
    Cancel,
    /// User pause or an automatic hold (recording started): save a checkpoint and stop.
    Pause,
    /// The worker is shutting down: save a checkpoint and stop; the job is re-queued.
    Shutdown,
}

/// Cooperative interruption signal. Handlers should poll `is_interrupted` between chunks or
/// `select!` on `interrupted()`, then return any `Err` (the queue decides the final state from
/// the interrupt kind, not from the error).
#[derive(Debug, Clone)]
pub struct CancelToken {
    pub(crate) rx: watch::Receiver<Option<Interrupt>>,
}

impl CancelToken {
    /// A token that is never interrupted (handler unit tests).
    pub fn never() -> (CancelToken, watch::Sender<Option<Interrupt>>) {
        let (tx, rx) = watch::channel(None);
        (CancelToken { rx }, tx)
    }

    pub fn interrupt(&self) -> Option<Interrupt> {
        *self.rx.borrow()
    }

    pub fn is_interrupted(&self) -> bool {
        self.interrupt().is_some()
    }

    pub fn is_cancelled(&self) -> bool {
        self.interrupt() == Some(Interrupt::Cancel)
    }

    /// Resolves once interrupted (immediately if already so).
    pub async fn interrupted(&mut self) -> Interrupt {
        loop {
            if let Some(i) = *self.rx.borrow_and_update() {
                return i;
            }
            if self.rx.changed().await.is_err() {
                // Sender dropped without interrupting: never resolves.
                std::future::pending::<()>().await;
            }
        }
    }
}

/// Reports progress and saves resumable state for one running job.
#[derive(Clone)]
pub struct ProgressReporter {
    pub(crate) inner: Arc<Inner>,
    pub(crate) job_id: String,
    pub(crate) job_type: String,
}

impl ProgressReporter {
    pub fn job_id(&self) -> &str {
        &self.job_id
    }

    /// Persists and broadcasts progress (`fraction` clamped to 0..=1).
    pub fn report(&self, fraction: f64, message: impl Into<Option<String>>) {
        let message: Option<String> = message.into();
        let now = self.inner.cfg.clock.now_ms();
        if let Ok(Some(job)) =
            self.inner
                .store
                .set_progress(&self.job_id, fraction, message.as_deref(), now)
        {
            self.inner.emit(JobEvent::Progress {
                job_id: self.job_id.clone(),
                job_type: self.job_type.clone(),
                progress: job.progress,
                message: job.message,
            });
        }
    }

    /// Saves the resumable-state blob (call after every chunk; STO-3, NFR-8). It is handed back
    /// in `JobContext::job.checkpoint` when the job is resumed or restarted.
    pub fn save_checkpoint(&self, checkpoint: &Value) -> Result<(), CmError> {
        let now = self.inner.cfg.clock.now_ms();
        self.inner
            .store
            .save_checkpoint(&self.job_id, checkpoint, now)
            .map(|_| ())
    }
}

/// Everything a handler gets for one attempt.
pub struct JobContext {
    /// Snapshot at start: inputs, attempt number, and the last saved checkpoint.
    pub job: Job,
    pub reporter: ProgressReporter,
    pub cancel: CancelToken,
}

/// Executes jobs of one type. `Err(e)` with `e.is_retryable()` is retried with backoff;
/// any other error fails the job at once with `e`'s message as the readable reason.
/// (A 5xx response must be mapped to `CmError::network` by the handler.)
#[async_trait::async_trait]
pub trait JobHandler: Send + Sync {
    async fn run(&self, ctx: JobContext) -> Result<Value, CmError>;

    /// Called after a job was cancelled, so partial output can be deleted (EXP-4.AC4).
    async fn on_cancel(&self, _job: &Job) {}
}

struct FnHandler<F>(F);

#[async_trait::async_trait]
impl<F, Fut> JobHandler for FnHandler<F>
where
    F: Fn(JobContext) -> Fut + Send + Sync,
    Fut: Future<Output = Result<Value, CmError>> + Send,
{
    async fn run(&self, ctx: JobContext) -> Result<Value, CmError> {
        (self.0)(ctx).await
    }
}

/// Wraps an async closure as a handler.
pub fn handler_fn<F, Fut>(f: F) -> Arc<dyn JobHandler>
where
    F: Fn(JobContext) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Value, CmError>> + Send + 'static,
{
    Arc::new(FnHandler(f))
}
