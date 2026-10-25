//! `JobQueue` (API + state machine) and `Worker` (scheduler loop).

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, RwLock};
use std::time::Duration;

use cm_core::{CmError, ErrorCode};
use serde_json::Value;
use tokio::sync::{broadcast, watch, Notify};

use crate::backoff::BackoffPolicy;
use crate::clock::{Clock, Jitter, RandomJitter, SystemClock};
use crate::handler::{CancelToken, Interrupt, JobContext, JobHandler, ProgressReporter};
use crate::model::{
    pause_hold_type, Job, JobEvent, JobFilter, JobId, JobState, NewJob, JOB_EXPORT, JOB_TRANSCRIBE,
    JOB_UPLOAD, PAUSE_AUTH_EXPIRED, PAUSE_HOLD_ALL, PAUSE_USER,
};
use crate::store::JobStore;

/// Queue configuration. `Default` follows the spec: 1 export, 2 uploads, 1 transcription,
/// 20 attempts, backoff 1 s -> 5 min.
#[derive(Debug, Clone)]
pub struct QueueConfig {
    /// Max concurrently running jobs per job type.
    pub limits: HashMap<String, usize>,
    /// Limit for types missing from `limits`.
    pub default_limit: usize,
    pub max_attempts: u32,
    pub max_attempts_by_type: HashMap<String, u32>,
    pub backoff: BackoffPolicy,
    /// How often the scheduler re-checks (backoff expiry has no other wake-up).
    pub poll_interval: Duration,
    pub event_capacity: usize,
    pub clock: Arc<dyn Clock>,
    pub jitter: Arc<dyn Jitter>,
}

impl Default for QueueConfig {
    fn default() -> Self {
        QueueConfig {
            limits: HashMap::from([
                (JOB_EXPORT.to_owned(), 1),
                (JOB_UPLOAD.to_owned(), 2),
                (JOB_TRANSCRIBE.to_owned(), 1),
            ]),
            default_limit: 1,
            max_attempts: 20,
            max_attempts_by_type: HashMap::new(),
            backoff: BackoffPolicy::default(),
            poll_interval: Duration::from_millis(250),
            event_capacity: 1024,
            clock: Arc::new(SystemClock),
            jitter: Arc::new(RandomJitter::default()),
        }
    }
}

impl QueueConfig {
    pub fn limit_for(&self, job_type: &str) -> usize {
        self.limits
            .get(job_type)
            .copied()
            .unwrap_or(self.default_limit)
            .max(1)
    }

    pub fn max_attempts_for(&self, job_type: &str) -> u32 {
        self.max_attempts_by_type
            .get(job_type)
            .copied()
            .unwrap_or(self.max_attempts)
            .max(1)
    }
}

struct RunningEntry {
    job_type: String,
    tx: watch::Sender<Option<Interrupt>>,
    pause_reason: Mutex<String>,
}

#[derive(Default)]
struct Runtime {
    running: HashMap<JobId, Arc<RunningEntry>>,
    held_types: HashSet<String>,
    held_all: bool,
}

impl Runtime {
    fn is_held(&self, job_type: &str) -> bool {
        self.held_all || self.held_types.contains(job_type)
    }

    fn hold_active(&self, reason: &str) -> bool {
        if reason == PAUSE_HOLD_ALL {
            return self.held_all;
        }
        reason
            .strip_prefix("hold:type:")
            .is_some_and(|t| self.held_types.contains(t))
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

pub(crate) struct Inner {
    pub(crate) store: JobStore,
    pub(crate) cfg: QueueConfig,
    events: broadcast::Sender<JobEvent>,
    handlers: RwLock<HashMap<String, Arc<dyn JobHandler>>>,
    rt: Mutex<Runtime>,
    wake: Notify,
    running_tx: watch::Sender<usize>,
    stopping: AtomicBool,
}

impl Inner {
    pub(crate) fn emit(&self, ev: JobEvent) {
        // No subscribers is fine.
        let _ = self.events.send(ev);
    }

    fn now(&self) -> i64 {
        self.cfg.clock.now_ms()
    }

    fn handler(&self, job_type: &str) -> Option<Arc<dyn JobHandler>> {
        self.handlers
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(job_type)
            .cloned()
    }

    fn state_changed(&self, job: &Job, from: JobState, reason: Option<String>) {
        self.emit(JobEvent::StateChanged {
            job_id: job.id.clone(),
            job_type: job.job_type.clone(),
            from,
            to: job.state,
            reason,
        });
    }

    fn finished(&self, job: &Job) {
        self.emit(JobEvent::Finished {
            job_id: job.id.clone(),
            job_type: job.job_type.clone(),
            state: job.state,
            result: job.result.clone(),
            error: job.failure().map(|e| e.to_payload()),
        });
    }

    /// Fails queued/paused dependents (transitively) of a job that failed or was cancelled.
    fn propagate_failure(&self, root: &str, verb: &str) {
        let mut stack = vec![(root.to_owned(), verb.to_owned())];
        while let Some((src, verb)) = stack.pop() {
            let Ok(deps) = self.store.dependents(&src) else {
                continue;
            };
            for dep in deps {
                let Ok(Some(prev)) = self.store.get(&dep) else {
                    continue;
                };
                if !matches!(prev.state, JobState::Queued | JobState::Paused) {
                    continue;
                }
                let reason = format!("dependency {src} {verb}");
                if let Ok(Some(j)) = self.store.fail(
                    &dep,
                    &[JobState::Queued, JobState::Paused],
                    ErrorCode::JobFailed,
                    &reason,
                    self.now(),
                ) {
                    self.state_changed(&j, prev.state, Some(reason));
                    self.finished(&j);
                    stack.push((dep, "failed".to_owned()));
                }
            }
        }
    }

    fn schedule_once(self: &Arc<Self>) -> usize {
        if self.stopping.load(Ordering::SeqCst) {
            return 0;
        }
        let now = self.now();
        let Ok(eligible) = self.store.eligible(now) else {
            return 0;
        };
        let mut started = 0;
        for job in eligible {
            let Some(handler) = self.handler(&job.job_type) else {
                continue;
            };
            let limit = self.cfg.limit_for(&job.job_type);
            let (entry, rx, started_job) = {
                let mut rt = lock(&self.rt);
                if rt.is_held(&job.job_type)
                    || rt.running.contains_key(&job.id)
                    || rt
                        .running
                        .values()
                        .filter(|e| e.job_type == job.job_type)
                        .count()
                        >= limit
                {
                    continue;
                }
                let Ok(Some(started_job)) = self.store.try_start(&job.id, now) else {
                    continue;
                };
                let (tx, rx) = watch::channel(None);
                let entry = Arc::new(RunningEntry {
                    job_type: job.job_type.clone(),
                    tx,
                    pause_reason: Mutex::new(PAUSE_USER.to_owned()),
                });
                rt.running.insert(job.id.clone(), entry.clone());
                self.running_tx.send_replace(rt.running.len());
                (entry, rx, started_job)
            };
            self.emit(JobEvent::Started {
                job_id: started_job.id.clone(),
                job_type: started_job.job_type.clone(),
                attempt: started_job.attempts,
            });
            self.state_changed(&started_job, JobState::Queued, None);
            started += 1;
            let inner = self.clone();
            tokio::spawn(async move {
                inner.run_job(started_job, handler, entry, rx).await;
            });
        }
        started
    }

    async fn run_job(
        self: Arc<Self>,
        job: Job,
        handler: Arc<dyn JobHandler>,
        entry: Arc<RunningEntry>,
        rx: watch::Receiver<Option<Interrupt>>,
    ) {
        let ctx = JobContext {
            job: job.clone(),
            reporter: ProgressReporter {
                inner: self.clone(),
                job_id: job.id.clone(),
                job_type: job.job_type.clone(),
            },
            cancel: CancelToken { rx },
        };
        let h = handler.clone();
        // A separate task so a panicking handler fails the job instead of leaking `running`.
        let res = match tokio::spawn(async move { h.run(ctx).await }).await {
            Ok(r) => r,
            Err(e) => Err(CmError::job_failed(
                job.id.clone(),
                format!("handler panicked: {e}"),
            )),
        };
        self.finish(job, handler, entry, res).await;
    }

    async fn finish(
        &self,
        job: Job,
        handler: Arc<dyn JobHandler>,
        entry: Arc<RunningEntry>,
        res: Result<Value, CmError>,
    ) {
        let now = self.now();
        let interrupt = *entry.tx.borrow();
        let mut cancelled = false;
        let mut failure_verb = None;
        {
            let mut rt = lock(&self.rt);
            match (res, interrupt) {
                (Ok(v), interrupt) => {
                    // A handler that returns `Ok` has finished its work, whatever was asked
                    // meanwhile: the job is `done`, never a half-way `cancelled` (the output
                    // exists). If a cancel had been requested, the transition says so, and
                    // `cancel_job`'s reply promised only "cancelling" (R2-12).
                    let reason = (interrupt == Some(Interrupt::Cancel))
                        .then(|| "finished before the cancel took effect".to_owned());
                    if let Ok(Some(j)) = self.store.finish_done(&job.id, &v, now) {
                        self.state_changed(&j, JobState::Running, reason);
                        self.finished(&j);
                    }
                }
                (Err(_), Some(Interrupt::Cancel)) => {
                    if let Ok(Some(j)) = self.store.set_cancelled(&job.id, now) {
                        self.state_changed(&j, JobState::Running, Some("cancelled".into()));
                        self.finished(&j);
                        cancelled = true;
                        failure_verb = Some("was cancelled");
                    }
                }
                (Err(_), Some(Interrupt::Pause)) => {
                    let reason = lock(&entry.pause_reason).clone();
                    let released = reason != PAUSE_USER && !rt.hold_active(&reason);
                    let out = if released {
                        self.store.requeue(&job.id, 0, None, true, now)
                    } else {
                        self.store.set_paused(&job.id, &reason, true, now)
                    };
                    if let Ok(Some(j)) = out {
                        self.state_changed(&j, JobState::Running, Some(reason));
                    }
                }
                (Err(_), Some(Interrupt::Shutdown)) => {
                    if let Ok(Some(j)) = self.store.requeue(&job.id, 0, None, true, now) {
                        self.state_changed(&j, JobState::Running, Some("shutdown".into()));
                    }
                }
                // The account must sign in again: wait for it instead of burning attempts.
                (Err(e), None) if e.code() == ErrorCode::AuthExpired => {
                    let msg = readable(&e);
                    if let Ok(Some(j)) = self.store.set_paused_auth_expired(&job.id, &msg, now) {
                        self.state_changed(&j, JobState::Running, Some(PAUSE_AUTH_EXPIRED.into()));
                    }
                }
                (Err(e), None) => {
                    let max = self.cfg.max_attempts_for(&job.job_type);
                    let msg = readable(&e);
                    if e.is_retryable() && job.attempts < max {
                        let delay = self
                            .cfg
                            .backoff
                            .delay(job.attempts, self.cfg.jitter.sample());
                        let delay_ms = i64::try_from(delay.as_millis()).unwrap_or(i64::MAX);
                        let reason = format!(
                            "attempt {}/{max} failed, retrying in {delay_ms} ms: {msg}",
                            job.attempts
                        );
                        if let Ok(Some(j)) = self.store.requeue(
                            &job.id,
                            now.saturating_add(delay_ms),
                            Some((e.code(), &msg)),
                            false,
                            now,
                        ) {
                            self.state_changed(&j, JobState::Running, Some(reason));
                        }
                    } else {
                        let reason = if e.is_retryable() {
                            format!("gave up after {} attempts: {msg}", job.attempts)
                        } else {
                            msg
                        };
                        if let Ok(Some(j)) =
                            self.store
                                .fail(&job.id, &[JobState::Running], e.code(), &reason, now)
                        {
                            self.state_changed(&j, JobState::Running, Some(reason));
                            self.finished(&j);
                            failure_verb = Some("failed");
                        }
                    }
                }
            }
            rt.running.remove(&job.id);
            self.running_tx.send_replace(rt.running.len());
        }
        if cancelled {
            if let Ok(Some(j)) = self.store.get(&job.id) {
                handler.on_cancel(&j).await;
            }
        }
        if let Some(verb) = failure_verb {
            self.propagate_failure(&job.id, verb);
        }
        self.wake.notify_one();
    }
}

/// The reason shown to users: the message, or the inner reason of a `job_failed`.
fn readable(e: &CmError) -> String {
    match e {
        CmError::JobFailed { reason, .. } => reason.clone(),
        other => other.to_string(),
    }
}

/// Persistent job queue. Cheap to clone; all clones share state.
#[derive(Clone)]
pub struct JobQueue {
    inner: Arc<Inner>,
}

impl JobQueue {
    pub fn new(store: JobStore, cfg: QueueConfig) -> Self {
        let (events, _) = broadcast::channel(cfg.event_capacity.max(1));
        let (running_tx, _) = watch::channel(0);
        JobQueue {
            inner: Arc::new(Inner {
                store,
                cfg,
                events,
                handlers: RwLock::new(HashMap::new()),
                rt: Mutex::new(Runtime::default()),
                wake: Notify::new(),
                running_tx,
                stopping: AtomicBool::new(false),
            }),
        }
    }

    pub fn open(path: impl AsRef<Path>, cfg: QueueConfig) -> Result<Self, CmError> {
        Ok(Self::new(JobStore::open(path)?, cfg))
    }

    pub fn in_memory(cfg: QueueConfig) -> Result<Self, CmError> {
        Ok(Self::new(JobStore::open_in_memory()?, cfg))
    }

    pub fn store(&self) -> &JobStore {
        &self.inner.store
    }

    pub fn config(&self) -> &QueueConfig {
        &self.inner.cfg
    }

    /// Registers (or replaces) the handler for a job type. Jobs of a type without a handler
    /// stay queued.
    pub fn register_handler(&self, job_type: impl Into<String>, handler: Arc<dyn JobHandler>) {
        self.inner
            .handlers
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(job_type.into(), handler);
        self.inner.wake.notify_one();
    }

    /// Whether a handler is registered for a job type (a type without one leaves its jobs
    /// queued, so engines that enqueue on their own check this first).
    pub fn has_handler(&self, job_type: &str) -> bool {
        self.inner.handler(job_type).is_some()
    }

    /// Typed events for `cm-daemon` to bridge onto the action bus. Slow receivers may see
    /// `RecvError::Lagged`.
    pub fn subscribe(&self) -> broadcast::Receiver<JobEvent> {
        self.inner.events.subscribe()
    }

    /// Resolves once nothing is running.
    pub async fn wait_no_running(&self) {
        let mut rx = self.inner.running_tx.subscribe();
        let _ = rx.wait_for(|n| *n == 0).await;
    }

    pub fn running_count(&self) -> usize {
        lock(&self.inner.rt).running.len()
    }

    // ---- queries ------------------------------------------------------------------------

    /// `get_job`.
    pub fn get(&self, id: &str) -> Result<Job, CmError> {
        self.inner
            .store
            .get(id)?
            .ok_or_else(|| CmError::not_found(format!("job {id} not found")))
    }

    /// `list_jobs` (newest first).
    pub fn list(&self, filter: &JobFilter) -> Result<Vec<Job>, CmError> {
        self.inner.store.list(filter)
    }

    // ---- commands -----------------------------------------------------------------------

    /// Enqueues a job. If a dependency already failed or was cancelled, the new job fails
    /// immediately with a readable reason.
    pub fn enqueue(&self, new: NewJob) -> Result<Job, CmError> {
        let inner = &self.inner;
        let job = inner.store.insert(&new, inner.now())?;
        inner.emit(JobEvent::Queued {
            job_id: job.id.clone(),
            job_type: job.job_type.clone(),
        });
        for dep in &job.depends_on {
            let state = inner.store.get(dep)?.map(|d| d.state);
            if let Some(s @ (JobState::Failed | JobState::Cancelled)) = state {
                let verb = if s == JobState::Failed {
                    "failed"
                } else {
                    "was cancelled"
                };
                let reason = format!("dependency {dep} {verb}");
                if let Some(j) = inner.store.fail(
                    &job.id,
                    &[JobState::Queued],
                    ErrorCode::JobFailed,
                    &reason,
                    inner.now(),
                )? {
                    inner.state_changed(&j, JobState::Queued, Some(reason));
                    inner.finished(&j);
                    return Ok(j);
                }
            }
        }
        inner.wake.notify_one();
        Ok(job)
    }

    /// Adds a dependency (cycles are rejected with `invalid_input`).
    pub fn add_dependency(&self, id: &str, depends_on: &str) -> Result<Job, CmError> {
        self.inner.store.add_dependency(id, depends_on)
    }

    /// `cancel_job`. A running job is signalled and reaches `cancelled` once its handler
    /// returns (then `on_cancel` runs); queued/paused jobs are cancelled at once. Dependents
    /// fail with a readable reason. Cancelling a cancelled job is a no-op.
    pub fn cancel(&self, id: &str) -> Result<Job, CmError> {
        let inner = &self.inner;
        let (job, handler_for_cleanup) = {
            let rt = lock(&inner.rt);
            let job = self.get(id)?;
            match job.state {
                JobState::Cancelled => return Ok(job),
                JobState::Done | JobState::Failed => {
                    return Err(CmError::invalid_field(
                        "job_id",
                        format!("job {id} already {}", job.state.as_str()),
                    ))
                }
                JobState::Running => {
                    let entry = rt.running.get(id).ok_or_else(|| {
                        CmError::busy(format!("job {id} is finishing; try again"))
                    })?;
                    entry.tx.send_replace(Some(Interrupt::Cancel));
                    return Ok(job);
                }
                JobState::Queued | JobState::Paused => {
                    let Some(j) = inner.store.set_cancelled(id, inner.now())? else {
                        return self.get(id);
                    };
                    inner.state_changed(&j, job.state, Some("cancelled".into()));
                    inner.finished(&j);
                    (j.clone(), inner.handler(&j.job_type))
                }
            }
        };
        if let Some(h) = handler_for_cleanup {
            let j = job.clone();
            tokio::spawn(async move { h.on_cancel(&j).await });
        }
        inner.propagate_failure(id, "was cancelled");
        inner.wake.notify_one();
        Ok(job)
    }

    /// User pause. A running job is signalled and reaches `paused` once its handler returns.
    pub fn pause(&self, id: &str) -> Result<Job, CmError> {
        let inner = &self.inner;
        let rt = lock(&inner.rt);
        let job = self.get(id)?;
        match job.state {
            JobState::Paused => Ok(job),
            JobState::Running => {
                let entry = rt
                    .running
                    .get(id)
                    .ok_or_else(|| CmError::busy(format!("job {id} is finishing; try again")))?;
                *lock(&entry.pause_reason) = PAUSE_USER.to_owned();
                entry.tx.send_replace(Some(Interrupt::Pause));
                Ok(job)
            }
            JobState::Queued => match inner.store.set_paused(id, PAUSE_USER, false, inner.now())? {
                Some(j) => {
                    inner.state_changed(&j, JobState::Queued, Some(PAUSE_USER.into()));
                    Ok(j)
                }
                None => self.get(id),
            },
            s => Err(CmError::invalid_field(
                "job_id",
                format!("cannot pause a {} job", s.as_str()),
            )),
        }
    }

    /// Resumes a paused job (it is re-queued and resumes from its checkpoint).
    pub fn resume(&self, id: &str) -> Result<Job, CmError> {
        let inner = &self.inner;
        let job = self.get(id)?;
        match job.state {
            JobState::Paused => match inner.store.resume(id, inner.now())? {
                Some(j) => {
                    inner.state_changed(&j, JobState::Paused, Some("resumed".into()));
                    inner.wake.notify_one();
                    Ok(j)
                }
                None => self.get(id),
            },
            JobState::Queued | JobState::Running => Ok(job),
            s => Err(CmError::invalid_field(
                "job_id",
                format!("cannot resume a {} job", s.as_str()),
            )),
        }
    }

    /// Retries a job (STO-3.AC4 "Retry", the `retry_job` action):
    ///
    /// * `failed`: re-queued with attempts reset and the checkpoint kept, and so are the
    ///   dependents that failed only because of it;
    /// * `cancelled`: re-queued from the start (attempts and checkpoint reset);
    /// * `paused` with reason `auth_expired`: resumed from its checkpoint;
    /// * `queued` or `running`: returned unchanged, so a repeated call is harmless.
    ///
    /// Anything else (`done`, a paused job with another reason) is `invalid_input`.
    pub fn retry(&self, id: &str) -> Result<Job, CmError> {
        let inner = &self.inner;
        let job = self.get(id)?;
        match job.state {
            JobState::Queued | JobState::Running => return Ok(job),
            JobState::Paused if job.pause_reason.as_deref() == Some(PAUSE_AUTH_EXPIRED) => {
                return self.resume(id);
            }
            JobState::Cancelled => {
                let Some(j) = inner.store.reset_cancelled(id, inner.now())? else {
                    return self.get(id);
                };
                inner.state_changed(&j, JobState::Cancelled, Some("retry".into()));
                inner.wake.notify_one();
                return Ok(j);
            }
            JobState::Failed => {}
            s => {
                return Err(CmError::invalid_field(
                    "job_id",
                    format!(
                        "only failed, cancelled or auth_expired jobs can be retried (job is {})",
                        s.as_str()
                    ),
                ));
            }
        }
        let Some(j) = inner.store.reset_for_retry(id, inner.now())? else {
            return self.get(id);
        };
        inner.state_changed(&j, JobState::Failed, Some("retry".into()));
        let mut stack = vec![id.to_owned()];
        while let Some(src) = stack.pop() {
            for dep in inner.store.dependents(&src)? {
                let Some(d) = inner.store.get(&dep)? else {
                    continue;
                };
                let by_dep = d.state == JobState::Failed
                    && d.error
                        .as_ref()
                        .is_some_and(|e| e.reason.starts_with("dependency "));
                if by_dep {
                    if let Some(dj) = inner.store.reset_for_retry(&dep, inner.now())? {
                        inner.state_changed(&dj, JobState::Failed, Some("retry".into()));
                        stack.push(dep);
                    }
                }
            }
        }
        inner.wake.notify_one();
        Ok(j)
    }

    // ---- automatic holds (recording pauses exports/uploads) ------------------------------

    /// Holds a job type: running jobs of the type are interrupted and become `paused`
    /// (reason `hold:type:<type>`), queued ones do not start. EXP-4.AC2 / STO-3.AC3.
    pub fn pause_type(&self, job_type: &str) {
        let mut rt = lock(&self.inner.rt);
        rt.held_types.insert(job_type.to_owned());
        let reason = pause_hold_type(job_type);
        Self::interrupt_running(&rt, |e| e.job_type == job_type, &reason);
    }

    /// Releases a type hold; jobs paused by it are re-queued and resume from their checkpoint.
    pub fn resume_type(&self, job_type: &str) {
        let mut rt = lock(&self.inner.rt);
        rt.held_types.remove(job_type);
        self.release(&mut rt, &pause_hold_type(job_type));
    }

    /// Global hold (all job types).
    pub fn pause_all(&self) {
        let mut rt = lock(&self.inner.rt);
        rt.held_all = true;
        Self::interrupt_running(&rt, |_| true, PAUSE_HOLD_ALL);
    }

    pub fn resume_all(&self) {
        let mut rt = lock(&self.inner.rt);
        rt.held_all = false;
        self.release(&mut rt, PAUSE_HOLD_ALL);
    }

    /// Re-queues every job that paused with `auth_expired` (the account signed in again); they
    /// resume from their checkpoint. Returns how many.
    pub fn resume_auth_expired(&self) -> Result<usize, CmError> {
        let inner = &self.inner;
        let jobs = inner
            .store
            .resume_by_reason(PAUSE_AUTH_EXPIRED, inner.now())?;
        for j in &jobs {
            inner.state_changed(j, JobState::Paused, Some("signed in again".into()));
        }
        inner.wake.notify_one();
        Ok(jobs.len())
    }

    pub fn is_type_held(&self, job_type: &str) -> bool {
        lock(&self.inner.rt).is_held(job_type)
    }

    fn interrupt_running(rt: &Runtime, pick: impl Fn(&RunningEntry) -> bool, reason: &str) {
        for e in rt.running.values().filter(|e| pick(e)) {
            // Don't downgrade a pending cancel/shutdown.
            if e.tx.borrow().is_none() {
                *lock(&e.pause_reason) = reason.to_owned();
                e.tx.send_replace(Some(Interrupt::Pause));
            }
        }
    }

    fn release(&self, _rt: &mut Runtime, reason: &str) {
        let inner = &self.inner;
        if let Ok(jobs) = inner.store.resume_by_reason(reason, inner.now()) {
            for j in jobs {
                inner.state_changed(&j, JobState::Paused, Some("hold released".into()));
            }
        }
        inner.wake.notify_one();
    }

    // ---- recovery and scheduling --------------------------------------------------------

    /// Restart recovery (EXP-4.AC3, NFR-8): jobs left `running` (crash/quit) and automatically
    /// held jobs go back to `queued` with their checkpoint, one `Recovered` event each. The
    /// daemon re-applies any holds (e.g. an active recording) after calling this. Returns the
    /// recovered job ids.
    pub fn recover(&self) -> Result<Vec<JobId>, CmError> {
        let inner = &self.inner;
        let jobs = inner.store.recover(inner.now())?;
        for j in &jobs {
            inner.emit(JobEvent::Recovered {
                job_id: j.id.clone(),
                job_type: j.job_type.clone(),
            });
        }
        inner.wake.notify_one();
        Ok(jobs.into_iter().map(|j| j.id).collect())
    }

    /// Starts every eligible job within the concurrency limits; returns how many started.
    /// The worker calls this on each wake-up; tests may call it directly.
    pub fn schedule_once(&self) -> usize {
        self.inner.schedule_once()
    }

    /// Spawns the scheduler loop on the current tokio runtime (after `recover()`).
    pub fn spawn_worker(&self) -> Worker {
        let inner = self.inner.clone();
        inner.stopping.store(false, Ordering::SeqCst);
        let (stop_tx, mut stop_rx) = watch::channel(false);
        let task = tokio::spawn(async move {
            loop {
                inner.schedule_once();
                tokio::select! {
                    _ = inner.wake.notified() => {}
                    _ = tokio::time::sleep(inner.cfg.poll_interval) => {}
                    _ = stop_rx.changed() => break,
                }
            }
        });
        Worker {
            queue: self.clone(),
            stop_tx,
            task,
        }
    }
}

/// Handle to the running scheduler loop.
pub struct Worker {
    queue: JobQueue,
    stop_tx: watch::Sender<bool>,
    task: tokio::task::JoinHandle<()>,
}

impl Worker {
    pub fn queue(&self) -> &JobQueue {
        &self.queue
    }

    /// Stops scheduling, asks running handlers to stop (they checkpoint) and waits for them.
    /// Interrupted jobs return to `queued` and resume on the next start.
    pub async fn shutdown(self) {
        self.shutdown_timeout(None).await;
    }

    /// [`Worker::shutdown`] that gives up waiting after `limit` (`None`: wait forever).
    /// Returns `false` when handlers were still running at the deadline: they are abandoned
    /// (their rows stay `running` and `recover()` re-queues them at the next start), so a
    /// stuck handler cannot hold up quitting the app (R2-8).
    pub async fn shutdown_timeout(self, limit: Option<Duration>) -> bool {
        let inner = &self.queue.inner;
        inner.stopping.store(true, Ordering::SeqCst);
        self.stop_tx.send_replace(true);
        let _ = self.task.await;
        {
            let rt = lock(&inner.rt);
            for e in rt.running.values() {
                e.tx.send_replace(Some(Interrupt::Shutdown));
            }
        }
        match limit {
            None => {
                self.queue.wait_no_running().await;
                true
            }
            Some(d) => tokio::time::timeout(d, self.queue.wait_no_running())
                .await
                .is_ok(),
        }
    }
}
