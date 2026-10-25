//! Deterministic queue tests. Spec references: EXP-4 (08-export), STO-3 (10-storage),
//! NFR-8 (00-product-spec), "Jobs, secrets and OAuth" (01-technical-design).
#![allow(clippy::unwrap_used)]

use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cm_core::{CmError, ErrorCode};
use cm_jobs::*;
use serde_json::{json, Value};
use tokio::sync::{broadcast, Notify};

const T: Duration = Duration::from_secs(5);

fn cfg() -> QueueConfig {
    QueueConfig {
        poll_interval: Duration::from_millis(2),
        jitter: Arc::new(FixedJitter(0.0)),
        ..QueueConfig::default()
    }
}

fn queue_with(cfg: QueueConfig) -> JobQueue {
    JobQueue::in_memory(cfg).unwrap()
}

/// Waits for an event matching `pred` (with a generous timeout; no sleeping).
async fn wait_event(
    rx: &mut broadcast::Receiver<JobEvent>,
    pred: impl Fn(&JobEvent) -> bool,
) -> JobEvent {
    tokio::time::timeout(T, async {
        loop {
            let ev = rx.recv().await.unwrap();
            if pred(&ev) {
                return ev;
            }
        }
    })
    .await
    .expect("timed out waiting for event")
}

fn to_state(id: &str, state: JobState) -> impl Fn(&JobEvent) -> bool + '_ {
    move |e| matches!(e, JobEvent::StateChanged { job_id, to, .. } if job_id == id && *to == state)
}

fn finished(id: &str) -> impl Fn(&JobEvent) -> bool + '_ {
    move |e| matches!(e, JobEvent::Finished { job_id, .. } if job_id == id)
}

#[tokio::test]
async fn emits_typed_events_and_persists_progress_and_result() {
    let q = queue_with(cfg());
    q.register_handler(
        JOB_EXPORT,
        handler_fn(|ctx| async move {
            ctx.reporter.report(0.5, "halfway".to_owned());
            Ok(json!({"path": "out.mp4"}))
        }),
    );
    let mut rx = q.subscribe();
    let _w = q.spawn_worker();
    let job = q
        .enqueue(NewJob::new(JOB_EXPORT, json!({"capture": "c1"})))
        .unwrap();
    assert_eq!(job.state, JobState::Queued);

    let mut seen = Vec::new();
    loop {
        let ev = tokio::time::timeout(T, rx.recv()).await.unwrap().unwrap();
        let name = match &ev {
            JobEvent::Queued { .. } => "queued",
            JobEvent::Started { attempt, .. } => {
                assert_eq!(*attempt, 1);
                "started"
            }
            JobEvent::Progress {
                progress, message, ..
            } => {
                assert_eq!(*progress, 0.5);
                assert_eq!(message.as_deref(), Some("halfway"));
                assert_eq!(ev.bus_event_name(), Some("job.progress"));
                "progress"
            }
            JobEvent::StateChanged { to, .. } => to.as_str(),
            JobEvent::Finished {
                state,
                result,
                error,
                ..
            } => {
                assert_eq!(*state, JobState::Done);
                assert_eq!(result.as_ref().unwrap()["path"], "out.mp4");
                assert!(error.is_none());
                assert_eq!(ev.bus_event_name(), Some("job.done"));
                seen.push("finished");
                break;
            }
            JobEvent::Recovered { .. } => unreachable!(),
        };
        seen.push(name);
    }
    assert_eq!(
        seen,
        ["queued", "started", "running", "progress", "done", "finished"]
    );
    let done = q.get(&job.id).unwrap();
    assert_eq!(done.state, JobState::Done);
    assert_eq!(done.progress, 1.0);
    assert_eq!(done.message.as_deref(), Some("halfway"));
    assert_eq!(done.result, Some(json!({"path": "out.mp4"})));
    assert_eq!(done.attempts, 1);
    assert!(done.finished_at_ms.is_some());
}

#[tokio::test]
async fn dependency_gates_start_and_failure_skips_dependent() {
    // STO-3.AC2: upload starts only after the export succeeds; skipped (failed with a note)
    // if the export fails. EXP-4 / tech design: "an upload waits on its export".
    let q = queue_with(cfg());
    let order = Arc::new(Mutex::new(Vec::new()));
    for ty in [JOB_EXPORT, JOB_UPLOAD] {
        let order = order.clone();
        q.register_handler(
            ty,
            handler_fn(move |ctx| {
                let order = order.clone();
                async move {
                    order.lock().unwrap().push(ctx.job.job_type.clone());
                    Ok(Value::Null)
                }
            }),
        );
    }
    let export = q.enqueue(NewJob::new(JOB_EXPORT, json!({}))).unwrap();
    let upload = q
        .enqueue(NewJob::new(JOB_UPLOAD, json!({})).depends_on(export.id.clone()))
        .unwrap();
    let mut rx = q.subscribe();
    // Only the export is eligible while it is not done.
    assert_eq!(q.schedule_once(), 1);
    let _w = q.spawn_worker();
    wait_event(&mut rx, finished(&upload.id)).await;
    assert_eq!(*order.lock().unwrap(), ["export", "upload"]);

    // Failing export: dependent fails with a readable reason and never runs.
    let q = queue_with(cfg());
    let ran = Arc::new(AtomicU32::new(0));
    q.register_handler(
        JOB_EXPORT,
        handler_fn(|_| async { Err(CmError::disk_full("no space left")) }),
    );
    let r = ran.clone();
    q.register_handler(
        JOB_UPLOAD,
        handler_fn(move |_| {
            r.fetch_add(1, Ordering::SeqCst);
            async { Ok(Value::Null) }
        }),
    );
    let export = q.enqueue(NewJob::new(JOB_EXPORT, json!({}))).unwrap();
    let upload = q
        .enqueue(NewJob::new(JOB_UPLOAD, json!({})).depends_on(export.id.clone()))
        .unwrap();
    let publish = q
        .enqueue(NewJob::new(JOB_PUBLISH, json!({})).depends_on(upload.id.clone()))
        .unwrap();
    let mut rx = q.subscribe();
    let _w = q.spawn_worker();
    wait_event(&mut rx, finished(&publish.id)).await;
    let u = q.get(&upload.id).unwrap();
    assert_eq!(u.state, JobState::Failed);
    assert_eq!(
        u.error.unwrap().reason,
        format!("dependency {} failed", export.id)
    );
    assert_eq!(q.get(&publish.id).unwrap().state, JobState::Failed);
    assert_eq!(ran.load(Ordering::SeqCst), 0);

    // Retry of the export re-queues the dependents that failed because of it.
    q.retry(&export.id).unwrap();
    assert_eq!(q.get(&upload.id).unwrap().state, JobState::Queued);
    assert_eq!(q.get(&publish.id).unwrap().state, JobState::Queued);

    // Dependency on an already failed job fails immediately.
    let late = q
        .enqueue(NewJob::new(JOB_UPLOAD, json!({})).depends_on(export.id.clone()))
        .unwrap();
    assert_eq!(late.state, JobState::Queued);
}

#[tokio::test]
async fn cycles_are_rejected() {
    let q = queue_with(cfg());
    let a = q.enqueue(NewJob::new("x", json!({}))).unwrap();
    let b = q
        .enqueue(NewJob::new("x", json!({})).depends_on(a.id.clone()))
        .unwrap();
    let e = q.add_dependency(&a.id, &b.id).unwrap_err();
    assert_eq!(e.code(), ErrorCode::InvalidInput);
    assert!(e.to_string().contains("cycle"));
}

#[tokio::test]
async fn per_type_concurrency_limits() {
    // Tech design: one export at a time, two uploads, one transcription.
    let q = queue_with(cfg());
    let gate = Arc::new(Notify::new());
    let cur = Arc::new(Mutex::new(std::collections::HashMap::<String, usize>::new()));
    let peak = Arc::new(Mutex::new(std::collections::HashMap::<String, usize>::new()));
    for ty in [JOB_EXPORT, JOB_UPLOAD, JOB_TRANSCRIBE] {
        let (gate, cur, peak) = (gate.clone(), cur.clone(), peak.clone());
        q.register_handler(
            ty,
            handler_fn(move |ctx| {
                let (gate, cur, peak) = (gate.clone(), cur.clone(), peak.clone());
                async move {
                    let t = ctx.job.job_type.clone();
                    {
                        let mut c = cur.lock().unwrap();
                        let n = c.entry(t.clone()).or_default();
                        *n += 1;
                        let mut p = peak.lock().unwrap();
                        let pk = p.entry(t.clone()).or_default();
                        *pk = (*pk).max(*n);
                    }
                    gate.notified().await;
                    *cur.lock().unwrap().get_mut(&t).unwrap() -= 1;
                    Ok(Value::Null)
                }
            }),
        );
    }
    let mut ids = Vec::new();
    for ty in [JOB_EXPORT, JOB_UPLOAD, JOB_TRANSCRIBE] {
        for _ in 0..4 {
            ids.push(q.enqueue(NewJob::new(ty, json!({}))).unwrap().id);
        }
    }
    // 1 export + 2 uploads + 1 transcription start; the rest wait.
    assert_eq!(q.schedule_once(), 4);
    assert_eq!(q.schedule_once(), 0);
    assert_eq!(q.running_count(), 4);
    let mut rx = q.subscribe();
    let _w = q.spawn_worker();
    // Release jobs one at a time until all 12 are done.
    let mut done = 0;
    while done < 12 {
        gate.notify_one();
        wait_event(&mut rx, |e| matches!(e, JobEvent::Finished { .. })).await;
        done += 1;
    }
    let peak = peak.lock().unwrap();
    assert_eq!(peak[JOB_EXPORT], 1);
    assert_eq!(peak[JOB_UPLOAD], 2);
    assert_eq!(peak[JOB_TRANSCRIBE], 1);
}

#[tokio::test]
async fn limits_are_configurable() {
    let mut c = cfg();
    c.limits.insert(JOB_EXPORT.into(), 3);
    let q = queue_with(c);
    q.register_handler(JOB_EXPORT, handler_fn(|_| std::future::pending()));
    for _ in 0..5 {
        q.enqueue(NewJob::new(JOB_EXPORT, json!({}))).unwrap();
    }
    assert_eq!(q.schedule_once(), 3);
}

#[tokio::test]
async fn network_errors_retry_with_exponential_backoff_using_injected_clock() {
    // NFR-8 / STO-3: exponential backoff 1 s -> 5 min for network errors; deterministic.
    let clock = Arc::new(ManualClock::new(1_000_000));
    let mut c = cfg();
    c.clock = clock.clone();
    let q = queue_with(c);
    let calls = Arc::new(AtomicU32::new(0));
    let n = calls.clone();
    q.register_handler(
        JOB_UPLOAD,
        handler_fn(move |ctx| {
            let n = n.clone();
            async move {
                let attempt = n.fetch_add(1, Ordering::SeqCst) + 1;
                assert_eq!(ctx.job.attempts, attempt);
                if attempt < 3 {
                    Err(CmError::network("connection reset"))
                } else {
                    Ok(json!({"remote_id": "r1"}))
                }
            }
        }),
    );
    let job = q.enqueue(NewJob::new(JOB_UPLOAD, json!({}))).unwrap();
    let mut rx = q.subscribe();

    assert_eq!(q.schedule_once(), 1);
    wait_event(&mut rx, to_state(&job.id, JobState::Queued)).await;
    let j = q.get(&job.id).unwrap();
    assert_eq!(j.not_before_ms, 1_000_000 + 1_000);
    assert_eq!(j.error.as_ref().unwrap().code, ErrorCode::Network);
    // Not yet: backoff has not elapsed.
    assert_eq!(q.schedule_once(), 0);
    clock.advance_ms(999);
    assert_eq!(q.schedule_once(), 0);
    clock.advance_ms(1);
    assert_eq!(q.schedule_once(), 1);
    wait_event(&mut rx, to_state(&job.id, JobState::Queued)).await;
    assert_eq!(q.get(&job.id).unwrap().not_before_ms, 1_001_000 + 2_000);
    clock.advance_ms(2_000);
    assert_eq!(q.schedule_once(), 1);
    wait_event(&mut rx, finished(&job.id)).await;
    let j = q.get(&job.id).unwrap();
    assert_eq!(j.state, JobState::Done);
    assert_eq!(j.attempts, 3);
    assert!(j.error.is_none());
}

#[tokio::test]
async fn gives_up_after_max_attempts_and_can_be_retried() {
    // STO-3.AC4: after the max attempts the job is failed, stays listed, and Retry works.
    let clock = Arc::new(ManualClock::new(0));
    let mut c = cfg();
    c.clock = clock.clone();
    c.max_attempts = 3;
    let q = queue_with(c);
    let healthy = Arc::new(AtomicU32::new(0));
    let h = healthy.clone();
    q.register_handler(
        JOB_UPLOAD,
        handler_fn(move |_| {
            let ok = h.load(Ordering::SeqCst) == 1;
            async move {
                if ok {
                    Ok(Value::Null)
                } else {
                    Err(CmError::network("offline"))
                }
            }
        }),
    );
    let job = q.enqueue(NewJob::new(JOB_UPLOAD, json!({}))).unwrap();
    let mut rx = q.subscribe();
    for _ in 0..3 {
        clock.advance_ms(600_000);
        assert_eq!(q.schedule_once(), 1);
        wait_event(&mut rx, |e| {
            matches!(e, JobEvent::StateChanged { to, from: JobState::Running, .. } if *to != JobState::Running)
        })
        .await;
    }
    let j = q.get(&job.id).unwrap();
    assert_eq!(j.state, JobState::Failed);
    assert_eq!(j.attempts, 3);
    assert!(j
        .error
        .unwrap()
        .reason
        .starts_with("gave up after 3 attempts"));
    assert_eq!(
        q.list(&JobFilter {
            job_type: Some(JOB_UPLOAD.into()),
            ..Default::default()
        })
        .unwrap()
        .len(),
        1
    );
    healthy.store(1, Ordering::SeqCst);
    let mut rx = q.subscribe();
    let j = q.retry(&job.id).unwrap();
    assert_eq!((j.state, j.attempts), (JobState::Queued, 0));
    assert_eq!(q.schedule_once(), 1);
    wait_event(&mut rx, finished(&job.id)).await;
    assert_eq!(q.get(&job.id).unwrap().state, JobState::Done);
}

#[tokio::test]
async fn non_retryable_error_fails_immediately_with_readable_reason() {
    // Tech design: 4xx stops the job with a readable reason; `job_failed` data job_id/reason.
    let q = queue_with(cfg());
    q.register_handler(
        JOB_UPLOAD,
        handler_fn(|_| async {
            Err(CmError::platform_rejected(
                "Video exceeds the 60 s limit",
                Some("400".into()),
            ))
        }),
    );
    let job = q.enqueue(NewJob::new(JOB_UPLOAD, json!({}))).unwrap();
    let mut rx = q.subscribe();
    let _w = q.spawn_worker();
    let ev = wait_event(&mut rx, finished(&job.id)).await;
    let JobEvent::Finished { state, error, .. } = &ev else {
        unreachable!()
    };
    assert_eq!(*state, JobState::Failed);
    assert_eq!(ev.bus_event_name(), Some("job.failed"));
    let payload = error.clone().unwrap();
    assert_eq!(payload.code, ErrorCode::JobFailed);
    let data = payload.data.unwrap();
    assert_eq!(data["job_id"], job.id);
    assert_eq!(data["reason"], "Video exceeds the 60 s limit");
    let j = q.get(&job.id).unwrap();
    assert_eq!(j.attempts, 1);
    assert_eq!(j.error.as_ref().unwrap().code, ErrorCode::PlatformRejected);
    assert_eq!(j.failure().unwrap().exit_code(), 6);
}

#[tokio::test]
async fn handler_panic_fails_the_job() {
    let q = queue_with(cfg());
    q.register_handler(JOB_EXPORT, handler_fn(|_| async { panic!("boom") }));
    let job = q.enqueue(NewJob::new(JOB_EXPORT, json!({}))).unwrap();
    let mut rx = q.subscribe();
    let _w = q.spawn_worker();
    wait_event(&mut rx, finished(&job.id)).await;
    let j = q.get(&job.id).unwrap();
    assert_eq!(j.state, JobState::Failed);
    assert!(j.error.unwrap().reason.contains("panicked"));
    assert_eq!(q.running_count(), 0);
}

/// A resumable "chunked upload": saves a checkpoint per chunk, honors pause.
fn chunked_handler(
    chunk_started: Arc<Notify>,
    starts: Arc<Mutex<Vec<u64>>>,
) -> Arc<dyn JobHandler> {
    handler_fn(move |mut ctx| {
        let (chunk_started, starts) = (chunk_started.clone(), starts.clone());
        async move {
            let mut chunk = ctx
                .job
                .checkpoint
                .as_ref()
                .and_then(|c| c["chunk"].as_u64())
                .unwrap_or(0);
            starts.lock().unwrap().push(chunk);
            while chunk < 4 {
                chunk += 1;
                ctx.reporter
                    .save_checkpoint(&json!({"session": "https://u/1", "chunk": chunk}))?;
                ctx.reporter.report(chunk as f64 / 4.0, None);
                chunk_started.notify_one();
                if chunk == 2 && starts.lock().unwrap().len() == 1 {
                    // First run: wait to be interrupted after chunk 2.
                    ctx.cancel.interrupted().await;
                    return Err(CmError::busy("interrupted"));
                }
            }
            Ok(json!({"chunks": chunk}))
        }
    })
}

#[tokio::test]
async fn type_hold_pauses_running_job_and_resumes_from_checkpoint() {
    // EXP-4.AC2 / STO-3.AC3: recording pauses the job; it resumes after the recording stops.
    let q = queue_with(cfg());
    let chunk = Arc::new(Notify::new());
    let starts = Arc::new(Mutex::new(Vec::new()));
    q.register_handler(JOB_UPLOAD, chunked_handler(chunk.clone(), starts.clone()));
    let mut rx = q.subscribe();
    let _w = q.spawn_worker();
    let job = q.enqueue(NewJob::new(JOB_UPLOAD, json!({}))).unwrap();

    // Wait for chunk 2 to be checkpointed, then "recording starts".
    loop {
        chunk.notified().await;
        if q.store()
            .get(&job.id)
            .unwrap()
            .unwrap()
            .checkpoint
            .is_some_and(|c| c["chunk"] == 2)
        {
            break;
        }
    }
    q.pause_type(JOB_UPLOAD);
    wait_event(&mut rx, to_state(&job.id, JobState::Paused)).await;
    let paused = q.get(&job.id).unwrap();
    assert_eq!(paused.pause_reason.as_deref(), Some("hold:type:upload"));
    assert_eq!(paused.attempts, 0, "interruption is not a failed attempt");
    assert!(q.is_type_held(JOB_UPLOAD));

    // A new job of the held type does not start.
    let other = q.enqueue(NewJob::new(JOB_UPLOAD, json!({}))).unwrap();
    assert_eq!(q.schedule_once(), 0);
    assert_eq!(q.get(&other.id).unwrap().state, JobState::Queued);

    q.resume_type(JOB_UPLOAD);
    wait_event(&mut rx, finished(&job.id)).await;
    assert_eq!(q.get(&job.id).unwrap().state, JobState::Done);
    // Second run started from the saved chunk, not from scratch.
    assert_eq!(starts.lock().unwrap()[..2], [0, 2]);
}

#[tokio::test]
async fn global_hold_and_hold_released_before_handler_returns() {
    let q = queue_with(cfg());
    q.register_handler(
        JOB_EXPORT,
        handler_fn(|mut ctx| async move {
            ctx.cancel.interrupted().await;
            Err(CmError::busy("stopped"))
        }),
    );
    let job = q.enqueue(NewJob::new(JOB_EXPORT, json!({}))).unwrap();
    let mut rx = q.subscribe();
    assert_eq!(q.schedule_once(), 1);
    q.pause_all();
    // Hold released immediately (recording was very short): the job must not stay paused.
    q.resume_all();
    wait_event(&mut rx, |e| matches!(e, JobEvent::StateChanged { job_id, from: JobState::Running, .. } if *job_id == job.id)).await;
    assert_eq!(q.get(&job.id).unwrap().state, JobState::Queued);
}

#[tokio::test]
async fn user_pause_resume_and_cancel_queued() {
    let q = queue_with(cfg());
    let ran = Arc::new(AtomicUsize::new(0));
    let r = ran.clone();
    q.register_handler(
        JOB_EXPORT,
        handler_fn(move |_| {
            r.fetch_add(1, Ordering::SeqCst);
            async { Ok(Value::Null) }
        }),
    );
    let job = q.enqueue(NewJob::new(JOB_EXPORT, json!({}))).unwrap();
    assert_eq!(q.pause(&job.id).unwrap().state, JobState::Paused);
    assert_eq!(q.schedule_once(), 0);
    // A hold release does not resume a user pause.
    q.resume_type(JOB_EXPORT);
    assert_eq!(q.get(&job.id).unwrap().state, JobState::Paused);
    assert_eq!(q.resume(&job.id).unwrap().state, JobState::Queued);
    assert_eq!(q.cancel(&job.id).unwrap().state, JobState::Cancelled);
    assert_eq!(q.cancel(&job.id).unwrap().state, JobState::Cancelled);
    assert_eq!(q.schedule_once(), 0);
    assert_eq!(ran.load(Ordering::SeqCst), 0);
    assert_eq!(q.cancel("nope").unwrap_err().code(), ErrorCode::NotFound);
}

struct CleanupHandler {
    started: Arc<Notify>,
    cleaned: Arc<Mutex<Vec<String>>>,
}

#[async_trait::async_trait]
impl JobHandler for CleanupHandler {
    async fn run(&self, mut ctx: JobContext) -> Result<Value, CmError> {
        self.started.notify_one();
        ctx.cancel.interrupted().await;
        Err(CmError::busy("cancelled"))
    }
    async fn on_cancel(&self, job: &Job) {
        self.cleaned.lock().unwrap().push(job.id.clone());
    }
}

#[tokio::test]
async fn cancel_running_job_runs_cleanup_and_fails_dependents() {
    // EXP-4.AC4: cancel -> partial file deleted (on_cancel hook).
    let q = queue_with(cfg());
    let started = Arc::new(Notify::new());
    let cleaned = Arc::new(Mutex::new(Vec::new()));
    q.register_handler(
        JOB_EXPORT,
        Arc::new(CleanupHandler {
            started: started.clone(),
            cleaned: cleaned.clone(),
        }),
    );
    let job = q.enqueue(NewJob::new(JOB_EXPORT, json!({}))).unwrap();
    let up = q
        .enqueue(NewJob::new(JOB_UPLOAD, json!({})).depends_on(job.id.clone()))
        .unwrap();
    let mut rx = q.subscribe();
    let _w = q.spawn_worker();
    started.notified().await;
    q.cancel(&job.id).unwrap();
    let ev = wait_event(&mut rx, finished(&job.id)).await;
    assert!(matches!(
        ev,
        JobEvent::Finished {
            state: JobState::Cancelled,
            ..
        }
    ));
    assert_eq!(ev.bus_event_name(), None);
    q.wait_no_running().await;
    assert_eq!(*cleaned.lock().unwrap(), std::slice::from_ref(&job.id));
    let u = q.get(&up.id).unwrap();
    assert_eq!(u.state, JobState::Failed);
    assert!(u.error.unwrap().reason.contains("was cancelled"));
    assert_eq!(
        q.cancel(&up.id).unwrap_err().code(),
        ErrorCode::InvalidInput
    );
}

#[tokio::test]
async fn restart_recovery_requeues_running_jobs_with_checkpoint() {
    // EXP-4.AC3 / NFR-8: jobs left running by a crash restart (or resume) automatically.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("jobs.db");
    let (running_id, queued_id, held_id) = {
        let q = JobQueue::open(&path, cfg()).unwrap();
        let a = q
            .enqueue(NewJob::new(JOB_UPLOAD, json!({"file": "a"})))
            .unwrap();
        let b = q
            .enqueue(NewJob::new(JOB_UPLOAD, json!({"file": "b"})))
            .unwrap();
        let c = q.enqueue(NewJob::new(JOB_EXPORT, json!({}))).unwrap();
        // Simulate the crash: job A was mid-upload with a saved session, C was held.
        q.store().try_start(&a.id, 1).unwrap().unwrap();
        assert!(q
            .store()
            .save_checkpoint(&a.id, &json!({"session": "s", "chunk": 7}), 2)
            .unwrap());
        q.store()
            .set_paused(&c.id, "hold:type:export", false, 3)
            .unwrap()
            .unwrap();
        (a.id, b.id, c.id)
    };
    let q = JobQueue::open(&path, cfg()).unwrap();
    assert_eq!(q.get(&running_id).unwrap().state, JobState::Running);
    let mut rx = q.subscribe();
    let recovered = q.recover().unwrap();
    assert_eq!(recovered, [running_id.clone(), held_id.clone()]);
    assert!(
        matches!(rx.recv().await.unwrap(), JobEvent::Recovered { ref job_id, .. } if *job_id == running_id)
    );
    let a = q.get(&running_id).unwrap();
    assert_eq!(a.state, JobState::Queued);
    assert_eq!(a.attempts, 0);
    assert_eq!(a.checkpoint.as_ref().unwrap()["chunk"], 7);
    assert_eq!(q.get(&queued_id).unwrap().state, JobState::Queued);

    // The restarted job sees its checkpoint.
    let seen = Arc::new(Mutex::new(None));
    let s = seen.clone();
    q.register_handler(
        JOB_UPLOAD,
        handler_fn(move |ctx| {
            if ctx.job.id.ends_with(&running_id[4..]) {
                *s.lock().unwrap() = ctx.job.checkpoint.clone();
            }
            async { Ok(Value::Null) }
        }),
    );
    let _w = q.spawn_worker();
    wait_event(&mut rx, finished(&a.id)).await;
    assert_eq!(seen.lock().unwrap().as_ref().unwrap()["session"], "s");
}

#[tokio::test]
async fn shutdown_returns_running_jobs_to_queue() {
    let q = queue_with(cfg());
    let started = Arc::new(Notify::new());
    let s = started.clone();
    q.register_handler(
        JOB_EXPORT,
        handler_fn(move |mut ctx| {
            let s = s.clone();
            async move {
                s.notify_one();
                ctx.cancel.interrupted().await;
                Err(CmError::busy("shutting down"))
            }
        }),
    );
    let w = q.spawn_worker();
    let job = q.enqueue(NewJob::new(JOB_EXPORT, json!({}))).unwrap();
    started.notified().await;
    w.shutdown().await;
    let j = q.get(&job.id).unwrap();
    assert_eq!(j.state, JobState::Queued);
    assert_eq!(j.attempts, 0);
    assert_eq!(q.schedule_once(), 0, "stopped queue does not schedule");
}

#[tokio::test]
async fn unknown_handler_keeps_job_queued_and_list_filters() {
    let q = queue_with(cfg());
    let a = q.enqueue(NewJob::new("mystery", json!({}))).unwrap();
    q.enqueue(NewJob::new(JOB_UPLOAD, json!({}))).unwrap();
    assert_eq!(q.schedule_once(), 0);
    assert_eq!(q.get(&a.id).unwrap().state, JobState::Queued);
    let l = q
        .list(&JobFilter {
            states: Some(vec![JobState::Queued]),
            job_type: Some("mystery".into()),
            limit: Some(5),
        })
        .unwrap();
    assert_eq!(l.len(), 1);
    assert_eq!(q.get("zzz").unwrap_err().code(), ErrorCode::NotFound);
}

// ---- R2-12 ---------------------------------------------------------------------------------

/// R2-12: a poison row (unknown error_code, bad inputs JSON, unknown state) does not make
/// `list`, `eligible` or `get` fail wholesale: it is repaired (marked failed with a reason) and
/// the other jobs keep working.
#[tokio::test]
async fn r2_12_poison_rows_are_skipped_and_marked_failed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("jobs.sqlite");
    let (good, bad1, bad2, bad3) = {
        let q = JobQueue::open(&path, cfg()).unwrap();
        let g = q.enqueue(NewJob::new("export", json!({"a": 1}))).unwrap();
        let b1 = q.enqueue(NewJob::new("export", json!({}))).unwrap();
        let b2 = q.enqueue(NewJob::new("export", json!({}))).unwrap();
        let b3 = q.enqueue(NewJob::new("upload", json!({}))).unwrap();
        (g.id, b1.id, b2.id, b3.id)
    };
    {
        let c = rusqlite::Connection::open(&path).unwrap();
        c.execute(
            "UPDATE jobs SET state='failed', error_code='from_the_future', error_reason='x' WHERE id=?1",
            [&bad1],
        )
        .unwrap();
        c.execute("UPDATE jobs SET inputs='{not json' WHERE id=?1", [&bad2])
            .unwrap();
        c.execute("UPDATE jobs SET state='exploded' WHERE id=?1", [&bad3])
            .unwrap();
    }
    let q = JobQueue::open(&path, cfg()).unwrap();
    // First query: poison rows are skipped, not fatal.
    let first = q
        .list(&JobFilter {
            job_type: None,
            states: None,
            limit: None,
        })
        .unwrap();
    assert!(first.iter().any(|j| j.id == good));
    // Second query: they are readable again, as failed jobs with a reason.
    let all = q
        .list(&JobFilter {
            job_type: None,
            states: None,
            limit: None,
        })
        .unwrap();
    assert_eq!(all.len(), 4, "{all:?}");
    for id in [&bad1, &bad2, &bad3] {
        let j = all.iter().find(|j| &j.id == id).unwrap();
        assert_eq!(j.state, JobState::Failed, "{id}");
        assert!(j.error.is_some(), "{id}");
    }
    // The scheduler still sees the good job.
    let eligible = q.store().eligible(i64::MAX).unwrap();
    assert!(eligible.iter().any(|j| j.id == good));
    assert_eq!(q.get(&bad2).unwrap().inputs, json!({}));
}

/// R2-12: the job database and its side files are owner-only.
#[cfg(unix)]
#[tokio::test]
async fn r2_12_jobs_database_is_0600() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("jobs.sqlite");
    let q = JobQueue::open(&path, cfg()).unwrap();
    let j = q.enqueue(NewJob::new("upload", json!({}))).unwrap();
    q.store().try_start(&j.id, 1).unwrap();
    q.store()
        .save_checkpoint(&j.id, &json!({"session": "https://up/x?t=1"}), 2)
        .unwrap();
    for suffix in ["", "-wal", "-shm"] {
        let p = format!("{}{suffix}", path.display());
        if let Ok(m) = std::fs::metadata(&p) {
            assert_eq!(
                m.permissions().mode() & 0o077,
                0,
                "{p} is group/world accessible"
            );
        }
    }
    // A pre-existing loose file is tightened.
    drop(q);
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    let _q = JobQueue::open(&path, cfg()).unwrap();
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o077,
        0
    );
}

/// R2-12: reasons stored for failed and retried jobs are scrubbed of URL secrets.
#[tokio::test]
async fn r2_12_error_reasons_are_scrubbed() {
    let q = queue_with(cfg());
    q.register_handler(
        JOB_UPLOAD,
        handler_fn(|_ctx| async move {
            Err(CmError::invalid_input(
                "PUT https://up.example.com/session/1?upload_id=SECRET123&sig=abc failed: Bearer tok.en.1",
            ))
        }),
    );
    let mut rx = q.subscribe();
    let _w = q.spawn_worker();
    let job = q.enqueue(NewJob::new(JOB_UPLOAD, json!({}))).unwrap();
    wait_event(&mut rx, finished(&job.id)).await;
    let j = q.get(&job.id).unwrap();
    let reason = j.error.unwrap().reason;
    assert!(!reason.contains("SECRET123"), "{reason}");
    assert!(!reason.contains("tok.en.1"), "{reason}");
    assert!(reason.contains("up.example.com/session/1"), "{reason}");
}

/// R2-12: a cancel that arrives while the handler is finishing normally. The handler's `Ok`
/// wins (the output exists), the job is `done`, and the transition records why.
#[tokio::test]
async fn r2_12_cancel_racing_a_successful_handler_ends_done() {
    let q = queue_with(cfg());
    let entered = Arc::new(Notify::new());
    let go = Arc::new(Notify::new());
    let (e2, g2) = (entered.clone(), go.clone());
    q.register_handler(
        JOB_EXPORT,
        handler_fn(move |_ctx| {
            let (e, g) = (e2.clone(), g2.clone());
            async move {
                e.notify_one();
                g.notified().await; // ignores the cancel signal
                Ok(json!({"path": "out.mp4"}))
            }
        }),
    );
    let mut rx = q.subscribe();
    let _w = q.spawn_worker();
    let job = q.enqueue(NewJob::new(JOB_EXPORT, json!({}))).unwrap();
    entered.notified().await;
    let reply = q.cancel(&job.id).unwrap();
    assert_eq!(
        reply.state,
        JobState::Running,
        "cancel_job reports `cancelling`, not `cancelled`"
    );
    go.notify_one();
    let ev = wait_event(&mut rx, to_state(&job.id, JobState::Done)).await;
    if let JobEvent::StateChanged { reason, .. } = ev {
        assert_eq!(
            reason.as_deref(),
            Some("finished before the cancel took effect")
        );
    }
    assert_eq!(q.get(&job.id).unwrap().state, JobState::Done);
}

/// R2-8: shutdown can be bounded; a handler that never stops is abandoned and its job is
/// re-queued by the next `recover()`.
#[tokio::test]
async fn r2_8_shutdown_timeout_abandons_a_stuck_handler() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("jobs.sqlite");
    let q = JobQueue::open(&path, cfg()).unwrap();
    let entered = Arc::new(Notify::new());
    let e2 = entered.clone();
    q.register_handler(
        JOB_EXPORT,
        handler_fn(move |_ctx| {
            let e = e2.clone();
            async move {
                e.notify_one();
                std::future::pending::<()>().await;
                Ok(json!({}))
            }
        }),
    );
    let w = q.spawn_worker();
    let job = q.enqueue(NewJob::new(JOB_EXPORT, json!({}))).unwrap();
    entered.notified().await;
    let clean = w.shutdown_timeout(Some(Duration::from_millis(200))).await;
    assert!(!clean, "the stuck handler was abandoned");
    drop(q);
    let q2 = JobQueue::open(&path, cfg()).unwrap();
    assert_eq!(q2.recover().unwrap(), vec![job.id.clone()]);
    assert_eq!(q2.get(&job.id).unwrap().state, JobState::Queued);
}

#[tokio::test]
async fn sto3_upload_policy_defaults() {
    // STO-3: 2 uploads at once, 20 attempts, backoff 1 s -> 5 min.
    let c = QueueConfig::default();
    assert_eq!(c.limit_for(JOB_UPLOAD), 2);
    assert_eq!(c.max_attempts_for(JOB_UPLOAD), 20);
    assert_eq!(c.backoff.delay(1, 0.0), Duration::from_secs(1));
    assert_eq!(c.backoff.delay(30, 0.0), Duration::from_secs(300));
}

#[tokio::test]
async fn auth_expired_waits_without_burning_attempts_and_resumes() {
    // An expired sign-in is not a failure: the job waits (paused) with the reason, keeps its
    // checkpoint and attempt budget, and resumes when the account is back.
    let q = queue_with(cfg());
    let signed_in = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = signed_in.clone();
    q.register_handler(
        JOB_UPLOAD,
        handler_fn(move |ctx| {
            let flag = flag.clone();
            async move {
                if !flag.load(Ordering::SeqCst) {
                    ctx.reporter.save_checkpoint(&json!({"done": 1}))?;
                    return Err(CmError::auth_expired("Sign in to Google Drive again"));
                }
                assert_eq!(ctx.job.checkpoint.as_ref().unwrap()["done"], 1);
                Ok(json!({"ok": true}))
            }
        }),
    );
    let job = q.enqueue(NewJob::new(JOB_UPLOAD, json!({}))).unwrap();
    let mut rx = q.subscribe();
    assert_eq!(q.schedule_once(), 1);
    wait_event(&mut rx, to_state(&job.id, JobState::Paused)).await;
    let j = q.get(&job.id).unwrap();
    assert_eq!(j.state, JobState::Paused);
    assert_eq!(j.pause_reason.as_deref(), Some(PAUSE_AUTH_EXPIRED));
    assert_eq!(j.attempts, 0);
    let e = j.error.unwrap();
    assert_eq!(e.code, ErrorCode::AuthExpired);
    assert!(e.reason.contains("Google Drive"), "{}", e.reason);
    // It does not start by itself, and a restart leaves it paused.
    assert_eq!(q.schedule_once(), 0);
    q.recover().unwrap();
    assert_eq!(q.get(&job.id).unwrap().state, JobState::Paused);

    signed_in.store(true, Ordering::SeqCst);
    assert_eq!(q.resume_auth_expired().unwrap(), 1);
    assert_eq!(q.schedule_once(), 1);
    wait_event(&mut rx, finished(&job.id)).await;
    assert_eq!(q.get(&job.id).unwrap().state, JobState::Done);
}

#[tokio::test]
async fn retry_covers_cancelled_and_auth_expired_jobs_and_is_idempotent() {
    // STO-3.AC4: `retry` is what the `retry_job` action calls.
    let q = queue_with(cfg());
    let paused_once = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = paused_once.clone();
    q.register_handler(
        JOB_UPLOAD,
        handler_fn(move |ctx| {
            let flag = flag.clone();
            async move {
                if !flag.load(Ordering::SeqCst) {
                    ctx.reporter.save_checkpoint(&json!({"done": 1}))?;
                    return Err(CmError::auth_expired("Sign in again"));
                }
                Ok(json!({"attempt_checkpoint": ctx.job.checkpoint.is_some()}))
            }
        }),
    );
    // A queued job is returned as it is.
    let held = q.enqueue(NewJob::new(JOB_EXPORT, json!({}))).unwrap();
    assert_eq!(q.retry(&held.id).unwrap().state, JobState::Queued);
    // A cancelled job starts over from the beginning.
    q.cancel(&held.id).unwrap();
    let again = q.retry(&held.id).unwrap();
    assert_eq!((again.state, again.attempts), (JobState::Queued, 0));
    assert!(again.checkpoint.is_none());
    // An auth_expired pause resumes with its checkpoint, without waiting for a sign-in event.
    let job = q.enqueue(NewJob::new(JOB_UPLOAD, json!({}))).unwrap();
    let mut rx = q.subscribe();
    assert_eq!(q.schedule_once(), 1);
    wait_event(&mut rx, to_state(&job.id, JobState::Paused)).await;
    paused_once.store(true, Ordering::SeqCst);
    assert_eq!(q.retry(&job.id).unwrap().state, JobState::Queued);
    assert_eq!(q.retry(&job.id).unwrap().state, JobState::Queued);
    assert_eq!(q.schedule_once(), 1);
    wait_event(&mut rx, finished(&job.id)).await;
    let done = q.get(&job.id).unwrap();
    assert_eq!(done.state, JobState::Done);
    assert_eq!(done.result.unwrap()["attempt_checkpoint"], true);
    // A finished job cannot be retried; a user-paused one is resumed, not retried.
    assert_eq!(
        q.retry(&job.id).unwrap_err().code(),
        ErrorCode::InvalidInput
    );
    let p = q.enqueue(NewJob::new(JOB_EXPORT, json!({}))).unwrap();
    q.pause(&p.id).unwrap();
    assert_eq!(q.retry(&p.id).unwrap_err().code(), ErrorCode::InvalidInput);
}
