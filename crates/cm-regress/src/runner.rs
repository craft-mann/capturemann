//! Runs the selected cases one by one, each in its own sandbox and thread with a hard time limit.

use crate::ctx::{Ctx, Options};
use crate::hygiene;
use crate::model::{not_runnable, Case, RunEnv, Status, Stop};
use crate::proc::Tracker;
use crate::report::CaseResult;
use crate::sys;
use std::path::PathBuf;
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

/// Picks the cases a run covers: all runnable ones of the tier, or the ones `--only` names (an id, or
/// an area or id prefix such as `mcp` or `record.`). Not-runnable ones stay in the list as SKIP.
pub fn select<'a>(all: &'a [Case], only: &[String], tier: crate::model::Tier) -> Vec<&'a Case> {
    all.iter()
        .filter(|c| {
            if only.is_empty() {
                c.tier <= tier
            } else {
                only.iter().any(|o| {
                    c.id == o
                        || c.area == o
                        || c.id.starts_with(&format!("{}.", o.trim_end_matches('.')))
                })
            }
        })
        .collect()
}

pub struct Runner {
    pub opts: Arc<Options>,
    pub env: RunEnv,
    pub work: PathBuf,
    counter: u32,
}

impl Runner {
    pub fn new(opts: Arc<Options>, env: RunEnv) -> Runner {
        let work = std::env::temp_dir().join(format!("cmr{}", std::process::id()));
        Runner {
            opts,
            env,
            work,
            counter: 0,
        }
    }

    pub fn run_case(&mut self, case: &Case) -> CaseResult {
        let mut res = CaseResult {
            id: case.id.to_owned(),
            title: case.title.to_owned(),
            area: case.area.to_owned(),
            tier: case.tier.name().to_owned(),
            status: Status::Pass.name().to_owned(),
            duration_ms: 0,
            message: String::new(),
            evidence: Vec::new(),
            specs: case.specs.iter().map(|s| (*s).to_owned()).collect(),
            needs: case.needs.iter().map(|n| n.name().to_owned()).collect(),
        };
        if let Some(reason) = not_runnable(case, &self.env) {
            res.status = Status::Skip.name().to_owned();
            res.message = reason;
            return res;
        }
        self.counter += 1;
        let root = self.work.join(self.counter.to_string());
        let _ = std::fs::remove_dir_all(&root);
        if let Err(e) = std::fs::create_dir_all(&root) {
            res.status = Status::Fail.name().to_owned();
            res.message = format!("cannot create the sandbox {}: {e}", root.display());
            return res;
        }
        let evidence: Arc<Mutex<Vec<String>>> = Arc::default();
        let tracker: Tracker = Arc::default();
        let before = hygiene::product_pids();
        let started_at = SystemTime::now();
        let start = Instant::now();
        let limit = Duration::from_secs_f64(case.timeout_s as f64 * self.opts.timeout_scale)
            + Duration::from_secs(15);

        let ctx = Ctx::new(
            self.opts.clone(),
            root.clone(),
            evidence.clone(),
            tracker.clone(),
            self.counter,
        );
        let (tx, rx) = channel();
        let run = case.run;
        let ctx = match ctx {
            Ok(c) => c,
            Err(e) => {
                res.status = Status::Fail.name().to_owned();
                res.message = format!("cannot prepare the sandbox: {e}");
                return res;
            }
        };
        std::thread::spawn(move || {
            let mut ctx = ctx;
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(&mut ctx)));
            let r = r.unwrap_or_else(|_| {
                Err(Stop::Fail(
                    "the case panicked (a bug in the suite)".to_owned(),
                ))
            });
            // The framework stops the daemon the case left running; that is not a stray.
            let _ = ctx.stop_daemon();
            let started = ctx.started_product;
            let allow = ctx.log_allow.clone();
            let _ = tx.send((r, started, allow));
        });
        let outcome = rx.recv_timeout(limit);
        let (result, started_product, log_allow) = match outcome {
            Ok(v) => v,
            Err(_) => (
                Err(Stop::Fail(format!("timed out after {} s", limit.as_secs()))),
                true,
                Vec::new(),
            ),
        };
        // Anything the case still had running is killed; a program left behind by a passing case is a failure.
        let leftovers: Vec<u32> = tracker.lock().map(|t| t.clone()).unwrap_or_default();
        for p in &leftovers {
            sys::kill_tree(*p);
        }
        std::thread::sleep(Duration::from_millis(300));
        // A helper still exiting after its daemon was stopped (cm-overlay once, twice in one macOS run) is
        // given a few seconds; a program that outlives that is a stray.
        let strays = hygiene::settle(
            || hygiene::strays_since(&before),
            Duration::from_secs(4),
            Duration::from_millis(200),
        );
        for (p, _) in &strays {
            sys::kill_tree(*p);
        }

        match result {
            Ok(()) => {}
            Err(Stop::Skip(why)) => {
                res.status = Status::Skip.name().to_owned();
                res.message = why;
            }
            Err(Stop::Fail(why)) => {
                res.status = Status::Fail.name().to_owned();
                res.message = why;
            }
        }
        // Hygiene applies to cases that passed on their own (a failed case already says why).
        if res.status == "PASS" {
            let mut problems = Vec::new();
            if !leftovers.is_empty() || !strays.is_empty() {
                let names: Vec<String> = strays.iter().map(|(p, n)| format!("{n} ({p})")).collect();
                problems.push(format!(
                    "left programs running: {} {}",
                    names.join(", "),
                    if leftovers.is_empty() {
                        String::new()
                    } else {
                        format!("tracked pids {leftovers:?}")
                    }
                ));
            }
            let (bad, warns) = hygiene::scan_logs(&root.join("config"), &log_allow);
            if warns > 0 {
                evidence
                    .lock()
                    .map(|mut e| {
                        e.push(format!(
                            "log hygiene: {warns} WARN line(s) in the case's logs"
                        ))
                    })
                    .ok();
            }
            if !bad.is_empty() {
                for b in bad.iter().take(5) {
                    evidence
                        .lock()
                        .map(|mut e| e.push(format!("log: {b}")))
                        .ok();
                }
                problems.push(format!(
                    "{} ERROR or panic line(s) in the logs, first: {}",
                    bad.len(),
                    bad[0]
                ));
            }
            if started_product {
                let crashes = hygiene::app_crashes_since(started_at - Duration::from_secs(2));
                for c in &crashes {
                    evidence
                        .lock()
                        .map(|mut e| e.push(format!("crash event: {c}")))
                        .ok();
                }
                if !crashes.is_empty() {
                    problems.push(format!(
                        "{} crash event(s) of our programs in the Application log",
                        crashes.len()
                    ));
                }
            }
            if !problems.is_empty() {
                res.status = Status::Fail.name().to_owned();
                res.message = problems.join("; ");
            }
        }
        res.duration_ms = start.elapsed().as_millis() as u64;
        res.evidence = evidence.lock().map(|e| e.clone()).unwrap_or_default();
        self.cleanup(&root, &mut res);
        res
    }

    fn cleanup(&self, root: &std::path::Path, res: &mut CaseResult) {
        if self.opts.keep {
            // Never keep pictures of the screen.
            for f in Ctx::files_under(root) {
                if f.extension().and_then(|e| e.to_str()) == Some("png") {
                    let _ = std::fs::remove_file(f);
                }
            }
            res.evidence.push(format!("kept: {}", root.display()));
        } else {
            let mut tries = 0;
            while std::fs::remove_dir_all(root).is_err() && tries < 10 && root.exists() {
                tries += 1;
                std::thread::sleep(Duration::from_millis(300));
            }
        }
    }

    pub fn finish(&self) {
        if !self.opts.keep {
            let _ = std::fs::remove_dir_all(&self.work);
        }
    }
}
