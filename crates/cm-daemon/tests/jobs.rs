//! Jobs over JSON-RPC: `get_job` / `list_jobs` / `cancel_job`, `job.*` events, restart
//! recovery, holds and policy. A test-only handler stands in for export/upload (AGT-1, AGT-3,
//! EXP-4, STO-3, NFR-8).

#![allow(clippy::unwrap_used)]

mod common;

use async_trait::async_trait;
use cm_core::CmError;
use cm_daemon::{library_job_queue, library_jobs_path, Daemon, Tier};
use cm_daemon::{ClientPolicy, DaemonBuilder};
use cm_jobs::{Job, JobContext, JobHandler, NewJob};
use common::{builder, desktop, Client};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;

const TYPE: &str = "test_job";
const SECRET: &str = "https://upload.example/session/abc";

/// Does `total` steps; each needs a permit from `gate`. Saves a checkpoint (holding a fake
/// session URL that agents must never see) and reports progress per step, resumes from the
/// checkpoint, and stops on any interrupt.
struct Steps {
    gate: Arc<Semaphore>,
    total: u32,
    cleaned: Arc<AtomicUsize>,
}

impl Steps {
    fn new(total: u32) -> (Arc<Steps>, Arc<Semaphore>) {
        let gate = Arc::new(Semaphore::new(0));
        (
            Arc::new(Steps {
                gate: gate.clone(),
                total,
                cleaned: Arc::default(),
            }),
            gate,
        )
    }
}

#[async_trait]
impl JobHandler for Steps {
    async fn run(&self, mut ctx: JobContext) -> Result<Value, CmError> {
        let start = ctx
            .job
            .checkpoint
            .as_ref()
            .and_then(|c| c["step"].as_u64())
            .unwrap_or(0) as u32;
        for step in start..self.total {
            tokio::select! {
                p = self.gate.acquire() => p.unwrap().forget(),
                i = ctx.cancel.interrupted() => return Err(CmError::internal(format!("{i:?}"))),
            }
            ctx.reporter
                .save_checkpoint(&json!({"step": step + 1, "session_url": SECRET}))?;
            ctx.reporter.report(
                f64::from(step + 1) / f64::from(self.total),
                Some(format!("step {}", step + 1)),
            );
        }
        Ok(json!({"steps": self.total, "resumed_from": start}))
    }

    async fn on_cancel(&self, _job: &Job) {
        self.cleaned.fetch_add(1, Ordering::SeqCst);
    }
}

fn ok(r: &Value) -> &Value {
    assert!(r.get("error").is_none(), "unexpected error: {r}");
    &r["result"]
}

fn err_code(r: &Value) -> &str {
    r["error"]["data"]["code"].as_str().unwrap()
}

fn with_queue(b: DaemonBuilder, root: &std::path::Path) -> Daemon {
    b.jobs(library_job_queue(root).unwrap()).build()
}

async fn jobs_daemon(steps: u32) -> (Daemon, Arc<Steps>, Arc<Semaphore>, tempfile::TempDir) {
    let (b, dir) = builder(desktop());
    let d = with_queue(b, dir.path());
    let (h, gate) = Steps::new(steps);
    d.register_job_handler(TYPE, h.clone()).unwrap();
    d.start_jobs().unwrap();
    (d, h, gate, dir)
}

fn enqueue(d: &Daemon) -> String {
    d.job_queue()
        .unwrap()
        .enqueue(NewJob::new(TYPE, json!({"capture_id": "rec_1"})))
        .unwrap()
        .id
}

/// Polls `get_job` until `pred` holds (5 s limit).
async fn wait_job(c: &mut Client, id: &str, pred: impl Fn(&Value) -> bool) -> Value {
    for _ in 0..500 {
        let r = c.call("get_job", json!({"job_id": id})).await;
        let j = ok(&r).clone();
        if pred(&j) {
            return j;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("job {id} never reached the expected state");
}

async fn event(c: &mut Client) -> Value {
    tokio::time::timeout(Duration::from_secs(5), c.next_event())
        .await
        .expect("no event within 5 s")
}

/// EXP-4 / STO-3 / AGT-1: enqueue, watch progress through `get_job`/`list_jobs`, receive
/// `job.progress` and `job.done` on the event stream, never see the checkpoint.
#[tokio::test]
async fn exp_4_progress_and_done_over_jsonrpc() {
    let (d, _h, gate, _dir) = jobs_daemon(3).await;
    let mut c = Client::connect(&d, "x");
    c.hello("claude-desktop").await;
    let sub = c
        .call("events.subscribe", json!({"topics": ["job.*"]}))
        .await;
    assert_eq!(ok(&sub)["subscribed"], true);

    let id = enqueue(&d);
    let j = wait_job(&mut c, &id, |j| j["state"] == "running").await;
    assert_eq!(j["type"], TYPE);
    assert_eq!(j["inputs"]["capture_id"], "rec_1");

    gate.add_permits(1);
    let j = wait_job(&mut c, &id, |j| j["progress"].as_f64().unwrap() > 0.3).await;
    assert_eq!(j["message"], "step 1");
    assert!(!j.to_string().contains(SECRET), "checkpoint must not leak");
    assert!(j.get("checkpoint").is_none());

    let list = c.call("list_jobs", json!({"state": "running"})).await;
    let jobs = ok(&list)["jobs"].as_array().unwrap().clone();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0]["job_id"], id);
    assert!(!list.to_string().contains(SECRET));
    let none = c.call("list_jobs", json!({"state": "done"})).await;
    assert!(ok(&none)["jobs"].as_array().unwrap().is_empty());

    gate.add_permits(2);
    let mut names = Vec::new();
    let done = loop {
        let ev = event(&mut c).await;
        names.push(ev["event"].as_str().unwrap().to_owned());
        assert_eq!(ev["data"]["job_id"], id);
        assert_eq!(ev["data"]["type"], TYPE);
        if ev["event"] == "job.done" {
            break ev;
        }
        assert_eq!(ev["event"], "job.progress");
    };
    assert_eq!(done["data"]["result"]["steps"], 3);
    assert_eq!(names.last().unwrap(), "job.done");
    assert!(names.iter().filter(|n| *n == "job.progress").count() >= 3);

    let j = wait_job(&mut c, &id, |j| j["state"] == "done").await;
    assert_eq!(j["progress"], 1.0);
    assert_eq!(j["result"]["steps"], 3);
    assert!(j["finished_at"].is_string());
    let filtered = c
        .call(
            "list_jobs",
            json!({"type": TYPE, "state": "done", "limit": 1}),
        )
        .await;
    assert_eq!(ok(&filtered)["jobs"][0]["job_id"], id);
    let other = c.call("list_jobs", json!({"type": "upload"})).await;
    assert!(ok(&other)["jobs"].as_array().unwrap().is_empty());
}

/// A failing handler produces `job.failed` with the shared error payload.
#[tokio::test]
async fn exp_4_failed_job_emits_job_failed() {
    let (b, dir) = builder(desktop());
    let d = with_queue(b, dir.path());
    d.register_job_handler(
        "boom",
        cm_jobs::handler_fn(|_ctx| async { Err(CmError::not_found("source bundle moved")) }),
    )
    .unwrap();
    let mut c = Client::connect(&d, "x");
    c.hello("claude-desktop").await;
    c.call("events.subscribe", json!({"topics": ["job.*"]}))
        .await;
    let id = d
        .job_queue()
        .unwrap()
        .enqueue(NewJob::new("boom", json!({})))
        .unwrap()
        .id;
    // No explicit start_jobs: the first job action starts the worker lazily.
    let j = wait_job(&mut c, &id, |j| j["state"] == "failed").await;
    assert_eq!(
        j["error"]["code"], "not_found",
        "EXP-4: the handler error code is kept"
    );
    let ev = event(&mut c).await;
    assert_eq!(ev["event"], "job.failed");
    assert_eq!(ev["data"]["job_id"], id);
    assert_eq!(ev["data"]["state"], "failed");
    assert!(ev["data"]["error"]["message"]
        .as_str()
        .unwrap()
        .contains("source bundle moved"));
}

/// EXP-4.AC4 / AGT-3: `cancel_job` stops a running job, calls `on_cancel`, is idempotent, and
/// a cancelled job emits no `job.*` event.
#[tokio::test]
async fn exp_4_ac4_cancel_is_idempotent_and_silent_on_the_bus() {
    let (d, h, gate, _dir) = jobs_daemon(3).await;
    let mut c = Client::connect(&d, "x");
    c.hello("claude-desktop").await;
    c.call("events.subscribe", json!({"topics": ["job.*"]}))
        .await;

    let id = enqueue(&d);
    wait_job(&mut c, &id, |j| j["state"] == "running").await;
    let r = c.call("cancel_job", json!({"job_id": id})).await;
    assert_eq!(ok(&r)["cancelling"], true);
    let j = wait_job(&mut c, &id, |j| j["state"] == "cancelled").await;
    assert!(j["finished_at"].is_string());

    // Idempotent: same final state, nothing else happens, also with a request_id replay.
    for params in [
        json!({"job_id": id}),
        json!({"job_id": id, "request_id": "c1"}),
        json!({"job_id": id, "request_id": "c1"}),
    ] {
        let r = c.call("cancel_job", params).await;
        assert_eq!(ok(&r)["state"], "cancelled");
        assert_eq!(ok(&r)["cancelling"], false);
    }
    for _ in 0..100 {
        if h.cleaned.load(Ordering::SeqCst) == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(h.cleaned.load(Ordering::SeqCst), 1, "on_cancel ran once");

    // A queued job cancels at once; unknown ids are not_found; finished jobs are refused.
    let (h2, _gate2) = Steps::new(1);
    d.register_job_handler("other", h2).unwrap();
    d.hold_job_type("other");
    let queued = d
        .job_queue()
        .unwrap()
        .enqueue(NewJob::new("other", json!({})))
        .unwrap()
        .id;
    let r = c.call("cancel_job", json!({"job_id": queued})).await;
    assert_eq!(ok(&r)["state"], "cancelled");
    let r = c.call("cancel_job", json!({"job_id": "nope"})).await;
    assert_eq!(err_code(&r), "not_found");
    let r = c.call("cancel_job", json!({})).await;
    assert_eq!(err_code(&r), "invalid_input");

    // The bus only carries the three agent-interface names: a later job's events are the
    // first ones this client sees.
    let id2 = enqueue(&d);
    gate.add_permits(3);
    let mut seen = Vec::new();
    loop {
        let ev = event(&mut c).await;
        seen.push(ev.clone());
        if ev["event"] == "job.done" {
            break;
        }
    }
    assert!(seen.iter().all(|e| e["data"]["job_id"] == id2), "{seen:?}");
    let r = c.call("cancel_job", json!({"job_id": id2})).await;
    assert_eq!(
        err_code(&r),
        "invalid_input",
        "done jobs cannot be cancelled"
    );
}

/// AGT-2.AC4: a read-only client may read jobs but not cancel them.
#[tokio::test]
async fn agt_2_ac4_read_only_client_reads_jobs_but_cannot_cancel() {
    let (d, _h, _gate, _dir) = jobs_daemon(2).await;
    d.policies().set("viewer", ClientPolicy::ReadOnly);
    d.policies().set("banned", ClientPolicy::Blocked);
    let id = enqueue(&d);
    let mut c = Client::connect(&d, "x");
    c.hello("viewer").await;
    let r = c.call("list_jobs", json!({})).await;
    assert_eq!(ok(&r)["jobs"].as_array().unwrap().len(), 1);
    let r = c.call("get_job", json!({"job_id": id})).await;
    assert_eq!(ok(&r)["job_id"], id);
    let r = c.call("cancel_job", json!({"job_id": id})).await;
    assert_eq!(err_code(&r), "user_declined");
    assert_eq!(r["error"]["data"]["data"]["reason"], "client_read_only");
    let still = c.call("get_job", json!({"job_id": id})).await;
    assert_ne!(ok(&still)["state"], "cancelled");

    let mut b = Client::connect(&d, "x");
    // R2-5: even `session.hello` is refused for a blocked name, and the name sticks.
    let hello = b.call("session.hello", json!({"client": "banned"})).await;
    assert_eq!(hello["error"]["data"]["data"]["reason"], "client_blocked");
    let r = b.call("list_jobs", json!({})).await;
    assert_eq!(r["error"]["data"]["data"]["reason"], "client_blocked");
}

/// STO-3.AC4: a job that failed can be retried with `retry_job` and then completes; a repeated
/// call (or the same `request_id`) is harmless, and a finished job is refused.
#[tokio::test]
async fn sto_3_ac4_retry_job_requeues_a_failed_job() {
    let (b, dir) = builder(desktop());
    let d = with_queue(b, dir.path());
    let healthy = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = healthy.clone();
    d.register_job_handler(
        "flaky",
        cm_jobs::handler_fn(move |_ctx| {
            let flag = flag.clone();
            async move {
                if flag.load(Ordering::SeqCst) {
                    Ok(json!({"ok": true}))
                } else {
                    Err(CmError::not_found("the destination folder is gone"))
                }
            }
        }),
    )
    .unwrap();
    let mut c = Client::connect(&d, "x");
    c.hello("claude-desktop").await;
    let id = d
        .job_queue()
        .unwrap()
        .enqueue(NewJob::new("flaky", json!({})))
        .unwrap()
        .id;
    wait_job(&mut c, &id, |j| j["state"] == "failed").await;

    healthy.store(true, Ordering::SeqCst);
    let r = c
        .call("retry_job", json!({"job_id": id, "request_id": "retry-1"}))
        .await;
    assert_eq!(ok(&r)["job_id"], id);
    let done = wait_job(&mut c, &id, |j| j["state"] == "done").await;
    assert_eq!(done["result"]["ok"], true);
    // The same request again returns the first answer and does not run the job again.
    let again = c
        .call("retry_job", json!({"job_id": id, "request_id": "retry-1"}))
        .await;
    assert_eq!(ok(&again), ok(&r));
    // A finished job cannot be retried.
    let r = c.call("retry_job", json!({"job_id": id})).await;
    assert_eq!(err_code(&r), "invalid_input");
    let r = c.call("retry_job", json!({"job_id": "nope"})).await;
    assert_eq!(err_code(&r), "not_found");
}

/// AGT-2: the job actions are Free; only `cancel_job` and `retry_job` mutate.
#[test]
fn job_actions_are_free_tier() {
    for (name, mutating) in [
        ("get_job", false),
        ("list_jobs", false),
        ("cancel_job", true),
        ("retry_job", true),
    ] {
        let s = cm_daemon::spec_for(name).unwrap();
        assert_eq!((s.tier, s.mutating), (Tier::Free, mutating), "{name}");
        assert!(!cm_daemon::is_internal(name));
    }
}

/// Without a queue the job actions say so instead of pretending the list is empty.
#[tokio::test]
async fn no_queue_is_a_clear_not_implemented() {
    let (b, _dir) = builder(desktop());
    let d = b.build();
    assert!(d.job_queue().is_none());
    d.start_jobs().unwrap();
    assert!(!d.hold_job_type("export"));
    assert!(d.register_job_handler(TYPE, Steps::new(1).0).is_err());
    let mut c = Client::connect(&d, "x");
    c.hello("claude-desktop").await;
    for (m, p) in [
        ("list_jobs", json!({})),
        ("get_job", json!({"job_id": "j"})),
        ("cancel_job", json!({"job_id": "j"})),
        ("retry_job", json!({"job_id": "j"})),
    ] {
        let r = c.call(m, p).await;
        assert_eq!(err_code(&r), "not_implemented", "{m}");
        assert_eq!(r["error"]["code"], -32099);
    }
}

/// EXP-4.AC3, STO-3.AC4, NFR-8: after a clean shutdown the job resumes from its checkpoint.
#[tokio::test]
async fn nfr_8_clean_restart_resumes_from_the_checkpoint() {
    let dir = tempfile::tempdir().unwrap();
    let id = {
        let b = builder_in(dir.path());
        let d = with_queue(b, dir.path());
        let (h, gate) = Steps::new(4);
        d.register_job_handler(TYPE, h).unwrap();
        d.start_jobs().unwrap();
        let mut c = Client::connect(&d, "x");
        c.hello("claude-desktop").await;
        let id = enqueue(&d);
        gate.add_permits(2);
        wait_job(&mut c, &id, |j| j["message"] == "step 2").await;
        d.shutdown_jobs().await;
        let j = ok(&c.call("get_job", json!({"job_id": id})).await).clone();
        assert_eq!(j["state"], "queued", "interrupted, not failed: {j}");
        id
    };
    assert!(library_jobs_path(dir.path()).is_file());

    let b = builder_in(dir.path());
    let d = with_queue(b, dir.path());
    let (h, gate) = Steps::new(4);
    d.register_job_handler(TYPE, h).unwrap();
    gate.add_permits(4);
    d.start_jobs().unwrap();
    let mut c = Client::connect(&d, "x");
    c.hello("claude-desktop").await;
    let j = wait_job(&mut c, &id, |j| j["state"] == "done").await;
    assert_eq!(j["result"]["resumed_from"], 2);
}

/// NFR-8: the process dies mid-job (the runtime is dropped with the job `running`); the next
/// daemon on the same database recovers it and resumes from the last checkpoint.
#[tokio::test]
async fn nfr_8_crash_mid_job_recovers_and_resumes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().to_path_buf();
    let id = std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let id = rt.block_on(async {
            let b = builder_in(&path);
            let d = with_queue(b, &path);
            let (h, gate) = Steps::new(4);
            d.register_job_handler(TYPE, h).unwrap();
            d.start_jobs().unwrap();
            let mut c = Client::connect(&d, "x");
            c.hello("claude-desktop").await;
            let id = enqueue(&d);
            gate.add_permits(3);
            wait_job(&mut c, &id, |j| j["message"] == "step 3").await;
            // A crash runs no destructors.
            std::mem::forget(d);
            id
        });
        rt.shutdown_background();
        id
    })
    .join()
    .unwrap();

    let b = builder_in(dir.path());
    let d = with_queue(b, dir.path());
    let (h, gate) = Steps::new(4);
    d.register_job_handler(TYPE, h).unwrap();
    gate.add_permits(4);
    d.start_jobs().unwrap();
    let mut c = Client::connect(&d, "x");
    c.hello("claude-desktop").await;
    let j = wait_job(&mut c, &id, |j| j["state"] == "done").await;
    assert_eq!(j["result"]["resumed_from"], 3, "{j}");
}

/// EXP-4.AC2 / STO-3.AC3: holding a job type pauses its running job at a checkpoint; releasing
/// it resumes. (The recording engine calls these; there is no action.)
#[tokio::test]
async fn exp_4_ac2_hold_pauses_and_resume_continues() {
    let (d, _h, gate, _dir) = jobs_daemon(2).await;
    let mut c = Client::connect(&d, "x");
    c.hello("claude-desktop").await;
    let id = enqueue(&d);
    gate.add_permits(1);
    wait_job(&mut c, &id, |j| j["message"] == "step 1").await;

    assert!(d.hold_job_type(TYPE));
    assert!(d.is_job_type_held(TYPE));
    let j = wait_job(&mut c, &id, |j| j["state"] == "paused").await;
    assert_eq!(j["pause_reason"], format!("hold:type:{TYPE}"));
    gate.add_permits(1);
    tokio::time::sleep(Duration::from_millis(100)).await;
    let j = wait_job(&mut c, &id, |_| true).await;
    assert_eq!(j["state"], "paused", "a held type does not run");

    assert!(d.resume_job_type(TYPE));
    assert!(!d.is_job_type_held(TYPE));
    let j = wait_job(&mut c, &id, |j| j["state"] == "done").await;
    assert_eq!(j["result"]["resumed_from"], 1);
    assert!(d.hold_all_jobs() && d.resume_all_jobs());
}

/// Builder for a library at `root` (the directory may already hold one).
fn builder_in(root: &std::path::Path) -> DaemonBuilder {
    let lib = cm_library::Library::open_or_create(root).unwrap();
    Daemon::builder(cm_capture::Capturer::new(desktop()).with_own_pid(None), lib)
}
