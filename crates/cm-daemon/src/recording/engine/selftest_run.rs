use super::*;

// ---- the 60 fps self-test (REC-3) ------------------------------------------------------------

pub(super) fn selftest_busy() -> CmError {
    CmError::busy("The 60 fps self-test is running; wait for it to finish or stop it")
}

/// A validated, planned self-test: what the confirmation dialog shows.
#[derive(Debug)]
pub struct PreparedSelftest {
    resolved: ResolvedStart,
    plan: CapturePlan,
    targets: Targets,
    fingerprint: Fingerprint,
    indicator: Value,
    /// One sentence: what will happen.
    pub summary: String,
    /// Flat `label -> text` rows.
    pub details: Value,
    /// The same plan as typed parameters (P3), `duration_s` included.
    pub params: Value,
}

pub(super) struct SelftestShared {
    id: String,
    by: String,
    cancelled: AtomicBool,
    wake: Notify,
    done: watch::Sender<bool>,
    /// The last status published (served by `get_recording_status`).
    status: Mutex<Value>,
}

/// Removes the self-test's temporary folder when dropped, however the test ends.
pub(super) struct TempFolder(pub(super) PathBuf);

impl TempFolder {
    pub(super) fn create(path: PathBuf) -> Result<Self, CmError> {
        std::fs::create_dir_all(&path).map_err(|e| {
            CmError::internal(format!(
                "cannot create the self-test folder {}: {e}",
                path.display()
            ))
        })?;
        // The test video is a capture of the screen: owner only (R3-16).
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700));
        }
        Ok(TempFolder(path))
    }

    fn remove(&self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl Drop for TempFolder {
    fn drop(&mut self) {
        self.remove();
    }
}

/// Prefix of the self-test's temporary folders.
pub(super) const SELFTEST_DIR_PREFIX: &str = "capturemann-selftest-";

/// Removes self-test folders in `dir` that a killed process left behind (75 to 100 MB of screen
/// capture each) and that are older than `min_age`. Returns how many were removed (R3-16).
pub(crate) fn sweep_selftest_dirs(dir: &Path, min_age: Duration) -> usize {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return 0;
    };
    let mut n = 0;
    for e in rd.flatten() {
        let name = e.file_name();
        if !name.to_string_lossy().starts_with(SELFTEST_DIR_PREFIX) {
            continue;
        }
        let old_enough = e
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age >= min_age);
        if old_enough && e.path().is_dir() && std::fs::remove_dir_all(e.path()).is_ok() {
            n += 1;
        }
    }
    n
}

/// Cancels the self-test when the caller of `run_fps_selftest` goes away.
struct CancelOnDrop(Option<Arc<SelftestShared>>);

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        if let Some(sh) = self.0.take() {
            sh.cancelled.store(true, Ordering::SeqCst);
            sh.wake.notify_one();
        }
    }
}

fn selftest_cancelled() -> DaemonError {
    DaemonError::Declined {
        reason: "selftest_cancelled",
        message: "The 60 fps self-test was cancelled".to_owned(),
    }
}

impl RecordingEngine {
    /// Whether the 60 fps self-test is running.
    pub fn is_selftest_active(&self) -> bool {
        lock(&self.inner.state).selftest.is_some()
    }

    /// Validates `run_fps_selftest` and plans it (target, size, codec, permissions) without
    /// creating anything. Errors here (`invalid_input`, `not_found`, `busy`,
    /// `permission_missing`) surface before the confirmation dialog opens.
    pub async fn prepare_selftest(
        &self,
        actor: &Actor,
        params: &ExternalParams,
    ) -> Result<PreparedSelftest, DaemonError> {
        let parsed = SelftestParams::parse(&params.rest)?;
        self.ensure_idle()?;
        let settings = self.inner.deps.settings.all().unwrap_or_default();
        let target = match parsed.target {
            Some(t) => t,
            None => {
                let eng = self.inner.clone();
                let targets =
                    crate::daemon::blocking(move || eng.deps.capturer.list_targets_with(false))
                        .await?;
                let d = targets
                    .displays
                    .iter()
                    .find(|d| d.is_primary)
                    .or_else(|| targets.displays.first())
                    .ok_or_else(|| {
                        CmError::not_found(
                            "No display found. If the screen is locked or asleep, unlock it and try again.",
                        )
                    })?;
                ScreenshotTarget::Display { display_id: d.id }
            }
        };
        let start = StartParams {
            mode: RecordMode::Screen,
            preset: None,
            people: None,
            target: Some(target),
            size: parsed.size,
            fps: Some(60),
            codec: parsed.codec,
            quality: None,
            camera: None,
            mic: None,
            system_audio: false,
            cursor: Some(false),
            cursor_separate: None,
            max_duration_s: None,
            countdown_s: Some(0),
            title: None,
            exclude: ExcludeParams::default(),
        };
        let mut resolved = start.resolve(&settings);
        resolved.max_duration_s = Some(self.inner.opts.selftest_duration.as_secs_f64());
        let exclusions = self.exclusions(&resolved, &settings);
        let (plan, targets, _warnings, names, root) =
            self.plan_blocking(&resolved, exclusions).await?;
        let fingerprint = Fingerprint::new(
            plan.display_id,
            &plan.video,
            resolved.size,
            resolved.codec,
            &self.inner.opts.app_version,
        );
        let mut indicator = indicator_json(&resolved, &plan, &targets, actor);
        if let Some(o) = indicator.as_object_mut() {
            o.insert("selftest".into(), json!(true));
            if let Some(by) = o.get_mut("started_by").and_then(Value::as_object_mut) {
                by.insert("tool".into(), json!("run_fps_selftest"));
            }
        }
        let secs = self.inner.opts.selftest_duration.as_secs_f64();
        let (_, mut details, mut params) =
            describe(&resolved, &plan, &targets, &names, &root, false);
        if let Some(o) = params.as_object_mut() {
            o.insert("duration_s".into(), json!(secs));
        }
        let target_text = details["target"]
            .as_str()
            .unwrap_or("the screen")
            .to_owned();
        let summary = format!(
            "Run the 60 fps self-test: record {} at {} x {}, 60 fps, for {} to check that this \
             machine keeps up. Nothing is saved: the test video is deleted when the test ends",
            target_text.split(',').next().unwrap_or("the screen"),
            plan.video.width,
            plan.video.height,
            duration_words(secs),
        );
        if let Some(o) = details.as_object_mut() {
            o.shift_remove("countdown");
            o.insert(
                "save_to".into(),
                json!("Nothing is saved: the test video is deleted when the test ends"),
            );
            o.insert("max_duration".into(), json!(duration_words(secs)));
            o.insert(
                "purpose".into(),
                json!("Measures late and dropped frames at 60 fps; only the result (pass or fail) is kept"),
            );
        }
        Ok(PreparedSelftest {
            resolved,
            plan,
            targets,
            fingerprint,
            indicator,
            summary,
            details,
            params,
        })
    }

    fn reserve_selftest(
        &self,
        actor: &Actor,
        prep: &PreparedSelftest,
    ) -> Result<Arc<SelftestShared>, DaemonError> {
        let mut st = lock(&self.inner.state);
        if st.active.is_some() {
            return Err(CmError::busy("A recording is already running").into());
        }
        if st.selftest.is_some() {
            return Err(selftest_busy().into());
        }
        st.seq += 1;
        let ms = self.inner.deps.clock.now().unix_timestamp_nanos() / 1_000_000;
        let id = format!("st_{ms:012x}_{:04}", st.seq);
        let total = self.inner.opts.selftest_duration.as_secs_f64();
        let (done, _) = watch::channel(false);
        let sh = Arc::new(SelftestShared {
            status: Mutex::new(json!({
                "selftest_id": id, "state": "starting", "total_s": total,
                "remaining_s": total.ceil(), "late_pct": 0.0, "indicator": prep.indicator,
            })),
            id,
            by: actor.to_string(),
            cancelled: AtomicBool::new(false),
            wake: Notify::new(),
            done,
        });
        st.selftest = Some(sh.clone());
        Ok(sh)
    }

    /// Runs the 60 fps self-test and returns its verdict (`state: "passed" | "failed"`). Takes
    /// the length of the test. If the caller goes away the test is cancelled and its output
    /// deleted. Fails with `busy` while a recording or another self-test runs.
    pub async fn run_selftest(
        &self,
        actor: &Actor,
        prep: PreparedSelftest,
    ) -> Result<Value, DaemonError> {
        let sh = self.reserve_selftest(actor, &prep)?;
        let mut guard = CancelOnDrop(Some(sh.clone()));
        let (tx, rx) = oneshot::channel();
        let eng = self.inner.clone();
        let task_sh = sh.clone();
        let handle = tokio::spawn(async move {
            let outcome = selftest_body(&eng, &task_sh, prep).await;
            eng.selftest_finish(&task_sh);
            let _ = tx.send(outcome);
        });
        let eng = self.inner.clone();
        let watch_sh = sh.clone();
        tokio::spawn(async move {
            if handle.await.is_err() {
                // The task died: leave no half-state behind. Its temp folder was dropped with it.
                eng.publish_selftest(
                    &watch_sh,
                    json!({ "state": "error",
                            "error": { "code": "internal", "message": "the self-test stopped unexpectedly" } }),
                );
                eng.selftest_finish(&watch_sh);
            }
        });
        let out = match rx.await {
            Ok(r) => r,
            Err(_) => Err(DaemonError::Internal(
                "the self-test stopped unexpectedly".to_owned(),
            )),
        };
        guard.0 = None;
        out
    }

    /// Cancels the running self-test (`id`: its id, or `None` for whichever runs) and waits until
    /// its output is deleted. `None` when there is no such self-test.
    pub async fn cancel_selftest(&self, id: Option<&str>) -> Option<Value> {
        let sh = lock(&self.inner.state)
            .selftest
            .clone()
            .filter(|s| id.is_none_or(|i| i == s.id))?;
        let mut rx = sh.done.subscribe();
        sh.cancelled.store(true, Ordering::SeqCst);
        sh.wake.notify_one();
        let wait = self.inner.opts.stop_timeout + Duration::from_secs(5);
        let _ = tokio::time::timeout(wait, rx.wait_for(|d| *d)).await;
        Some(json!({ "selftest_id": sh.id, "state": "cancelled", "cancelled": true }))
    }

    pub(super) fn selftest_status(&self) -> Option<Value> {
        lock(&self.inner.state)
            .selftest
            .as_ref()
            .map(|s| lock(&s.status).clone())
    }
}

impl EngineInner {
    fn publish_selftest(&self, sh: &SelftestShared, mut data: Value) {
        if let Some(o) = data.as_object_mut() {
            o.insert("selftest_id".into(), json!(sh.id));
            o.insert("by".into(), json!(sh.by));
            let mut st = lock(&sh.status);
            if let Some(cur) = st.as_object_mut() {
                for (k, v) in o.iter() {
                    cur.insert(k.clone(), v.clone());
                }
            }
        }
        self.deps.events.publish(events::RECORDING_SELFTEST, data);
    }

    fn selftest_finish(&self, sh: &SelftestShared) {
        {
            let mut st = lock(&self.state);
            if st.selftest.as_ref().is_some_and(|s| s.id == sh.id) {
                st.selftest = None;
            }
        }
        let _ = sh.done.send(true);
    }
}

/// The body of one self-test: record into a temp folder, measure, delete, store the verdict.
async fn selftest_body(
    eng: &Arc<EngineInner>,
    sh: &Arc<SelftestShared>,
    prep: PreparedSelftest,
) -> Result<Value, DaemonError> {
    let include_upload = eng
        .deps
        .settings
        .all()
        .ok()
        .and_then(|s| s.get(PAUSE_UPLOADS_SETTING).and_then(Value::as_bool))
        .unwrap_or(true);
    // The same background work pauses as for a recording, so it does not skew the measurement.
    let _holds = JobHolds::acquire(eng.deps.jobs.as_ref(), include_upload);
    let folder = TempFolder::create(
        eng.opts
            .selftest_dir
            .clone()
            .unwrap_or_else(std::env::temp_dir)
            .join(format!("{SELFTEST_DIR_PREFIX}{}", sh.id)),
    )?;
    let total = eng.opts.selftest_duration;
    let total_s = total.as_secs_f64();
    let cancelled = |eng: &Arc<EngineInner>| {
        eng.publish_selftest(sh, json!({ "state": "cancelled", "remaining_s": 0 }));
        Err(selftest_cancelled())
    };
    if sh.cancelled.load(Ordering::SeqCst) {
        return cancelled(eng);
    }

    // Begin: the same tracks as a recording, minus every sensor, into the temp folder.
    // R3-4 / R4-9: the indicator facts go out before the tracks are built, like
    // `recording.starting` for a recording: the self-test captures real pixels, so the HUD and
    // the indicator must not wait for the slowest start.
    eng.publish_selftest(
        sh,
        json!({ "state": "started", "total_s": total_s, "remaining_s": total_s.ceil(),
                "late_pct": 0.0, "indicator": prep.indicator }),
    );
    let begin = {
        let eng = eng.clone();
        let resolved = prep.resolved.clone();
        let plan = prep.plan.clone();
        let targets = prep.targets.clone();
        let parent = folder.0.clone();
        let by = sh.by.clone();
        tokio::task::spawn_blocking(move || {
            let tracks = eng.factory.build(&BuildContext {
                params: &resolved,
                plan: &plan,
                targets: &targets,
            })?;
            let mut cfg = SessionConfig::new(&parent);
            cfg.title = Some("60 fps self-test".to_owned());
            let mut settings = resolved.settings_json(&by);
            if let Some(o) = settings.as_object_mut() {
                o.insert("selftest".into(), json!(true));
            }
            cfg.settings = settings;
            cfg.displays = targets
                .displays
                .iter()
                .map(|d| SessionDisplay {
                    id: d.id,
                    name: d.name.clone(),
                    bounds: d.bounds,
                    scale: d.scale,
                    is_primary: d.is_primary,
                })
                .collect();
            cfg.app_version = eng.opts.app_version.clone();
            cfg.host = eng.opts.host.clone();
            cfg.free_space = eng.opts.free_space.clone();
            cfg.disk_check_every = eng.opts.disk_check_every;
            RecordingSession::begin(cfg, tracks)
        })
        .await
    };
    let session = match begin {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => return fail_selftest(eng, sh, e.into()),
        Err(e) => {
            return fail_selftest(
                eng,
                sh,
                CmError::internal(format!("worker failed: {e}")).into(),
            )
        }
    };
    let slot = Arc::new(Mutex::new(Some(session)));

    // Run for the whole length, reporting each tick.
    enum Ended {
        Elapsed,
        Cancelled,
        Halted(String, String),
    }
    let deadline = tokio::time::Instant::now() + total;
    let mut ticker = tokio::time::interval(eng.opts.tick_every);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    ticker.tick().await;
    let mut last_late = 0.0_f64;
    let mut reported_running = false;
    let ended = loop {
        if sh.cancelled.load(Ordering::SeqCst) {
            break Ended::Cancelled;
        }
        // `biased`, ticker first: after a stall the tick and the deadline are both ready, and a
        // random pick could skip every `running` report of a short test (flaky test fix).
        tokio::select! {
            biased;
            _ = ticker.tick() => {
                eng.factory.tick_hook();
                let s2 = slot.clone();
                let ticked = tokio::task::spawn_blocking(move || {
                    lock(&s2).as_mut().map(|s| (s.tick(), s.status()))
                })
                .await
                .ok()
                .flatten();
                let Some((notes, st)) = ticked else { continue };
                last_late = st
                    .tracks
                    .iter()
                    .find(|t| t.kind == TrackKind::Screen)
                    .and_then(|t| t.live["late_pct"].as_f64())
                    .unwrap_or(last_late);
                let left = deadline.saturating_duration_since(tokio::time::Instant::now());
                eng.publish_selftest(
                    sh,
                    json!({ "state": "running", "remaining_s": left.as_secs_f64().ceil(),
                            "late_pct": last_late, "total_s": total_s }),
                );
                reported_running = true;
                if let Some(n) = notes.iter().find(|n| n.level == NoteLevel::StopRequested) {
                    break Ended::Halted(n.code.clone(), n.message.clone());
                }
            }
            () = sh.wake.notified() => {}
            () = tokio::time::sleep_until(deadline) => break Ended::Elapsed,
        }
    };
    if matches!(ended, Ended::Elapsed) && !reported_running {
        // A stall swallowed every tick: say how far the test got before the verdict.
        eng.publish_selftest(
            sh,
            json!({ "state": "running", "remaining_s": 0, "late_pct": last_late,
                    "total_s": total_s }),
        );
    }

    let taken = lock(&slot).take();
    let Some(mut session) = taken else {
        return fail_selftest(
            eng,
            sh,
            CmError::internal("the self-test session is gone").into(),
        );
    };
    let stop_timeout = eng.opts.stop_timeout;
    match ended {
        Ended::Cancelled | Ended::Halted(..) => {
            let job = tokio::task::spawn_blocking(move || session.discard());
            let _ = tokio::time::timeout(stop_timeout, job).await;
            folder.remove();
            return match ended {
                Ended::Halted(code, message) => {
                    let e = if code == "disk_full" {
                        CmError::disk_full(message)
                    } else {
                        CmError::internal(format!("The self-test stopped early: {message}"))
                    };
                    fail_selftest(eng, sh, e.into())
                }
                _ => cancelled(eng),
            };
        }
        Ended::Elapsed => {}
    }
    let stopping = tokio::task::spawn_blocking(move || session.stop());
    let stopped = match tokio::time::timeout(stop_timeout, stopping).await {
        Ok(Ok(Ok(s))) => s,
        Ok(Ok(Err(e))) => return fail_selftest(eng, sh, e.into()),
        Ok(Err(e)) => {
            return fail_selftest(
                eng,
                sh,
                CmError::internal(format!("worker failed: {e}")).into(),
            )
        }
        Err(_) => {
            return fail_selftest(
                eng,
                sh,
                CmError::internal("Stopping the self-test took too long").into(),
            )
        }
    };
    let screen = stopped
        .tracks
        .iter()
        .find(|t| t.kind == TrackKind::Screen)
        .cloned();
    drop(stopped);
    // The test video has done its job: delete it before anything is reported.
    folder.remove();
    let Some(screen) = screen else {
        return fail_selftest(
            eng,
            sh,
            CmError::internal("the self-test recorded no screen track").into(),
        );
    };

    // Measure.
    let stats = screen.stats;
    let frames = stats.written;
    let dropped = stats.dropped;
    let dropped_pct = 100.0 * dropped as f64 / (frames + dropped).max(1) as f64;
    let late_pct = screen.details["late_pct"]
        .as_f64()
        .unwrap_or_else(|| 100.0 * stats.late as f64 / (frames + dropped).max(1) as f64);
    let stall_ms = screen.details["stall_ms"].as_u64().unwrap_or(0);
    let stalls = screen.details["stalls"].as_u64().unwrap_or(0);
    let passed = selftest::judge_with_stalls(late_pct, dropped_pct, frames, stall_ms);
    let now = eng.deps.clock.now();
    let fp = &prep.fingerprint;
    let entry = SelftestEntry {
        passed,
        late_pct,
        dropped,
        dropped_pct,
        frames,
        at: rfc3339(now),
        display: fp.display,
        size: [fp.width, fp.height],
        codec: fp.codec.clone(),
        app_version: fp.app_version.clone(),
        duration_s: total_s,
        stall_ms,
    };

    // Store the verdict (a settings key; the desktop persists it in settings.json).
    let store = {
        let settings = eng.deps.settings.clone();
        let entry = entry.clone();
        tokio::task::spawn_blocking(move || -> Result<Map<String, Value>, CmError> {
            let all = settings.all()?;
            let value = selftest::store_value(&all, entry, now);
            let mut m = Map::new();
            m.insert(selftest::SETTING_KEY.to_owned(), value);
            settings.set(&m)
        })
        .await
    };
    let (stored, store_error, after) = match store {
        Ok(Ok(all)) => (true, None, all),
        Ok(Err(e)) => (false, Some(e.to_string()), Map::new()),
        Err(e) => (false, Some(format!("worker failed: {e}")), Map::new()),
    };
    // The same answer a following `start_recording { fps: 60 }` will get.
    let verdict = if stored {
        selftest::evaluate(&after, fp, now)
    } else {
        Fps60Verdict::Allowed
    };
    let mut result = json!({
        "selftest_id": sh.id,
        "state": if passed { "passed" } else { "failed" },
        "passed": passed,
        "fps": 60,
        "late_pct": late_pct,
        "dropped": dropped,
        "dropped_pct": dropped_pct,
        "frames": frames,
        "stalls": stalls,
        "stall_ms": stall_ms,
        "duration_s": total_s,
        "size": [fp.width, fp.height],
        "size_label": fp.size_label(),
        "codec": fp.codec,
        "display_id": fp.display,
        "at": entry.at,
        "threshold": { "late_pct": selftest::MAX_LATE_PCT, "dropped_pct": selftest::MAX_DROPPED_PCT },
        "stored": stored,
        "store_error": store_error,
        "setting": selftest::SETTING_KEY,
        "message": if passed {
            format!("60 fps is stable on this machine at {}", fp.size_label())
        } else {
            String::new()
        },
    });
    if let (Fps60Verdict::Rejected(r), Some(o)) = (&verdict, result.as_object_mut()) {
        if !passed || stored {
            o.insert("message".into(), json!(r.message));
            o.insert("suggestion".into(), r.suggestion.clone());
        }
    }
    eng.publish_selftest(
        sh,
        json!({ "state": if passed { "passed" } else { "failed" }, "remaining_s": 0,
                "late_pct": late_pct, "result": result }),
    );
    Ok(result)
}

/// Reports a self-test that could not finish and returns the error.
fn fail_selftest(
    eng: &Arc<EngineInner>,
    sh: &SelftestShared,
    e: DaemonError,
) -> Result<Value, DaemonError> {
    eng.publish_selftest(
        sh,
        json!({ "state": "error", "remaining_s": 0, "error": error_json(&e) }),
    );
    Err(e)
}
