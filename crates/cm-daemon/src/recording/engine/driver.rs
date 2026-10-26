use super::*;

// ---- the driver ---------------------------------------------------------------------------

enum Flow {
    Continue,
    End,
}

enum CountdownEnd {
    Elapsed,
    Skipped,
    /// A pause during the countdown (REC-4.AC4): the caller of pause gets its answer.
    Cancelled(Option<Reply>),
    /// A stop or discard arrived.
    Interrupted(Cmd),
    /// Nobody waits for `start_recording` any more.
    StarterGone,
}

pub(super) struct Driver {
    pub(super) eng: Arc<EngineInner>,
    pub(super) sh: Arc<Shared>,
    pub(super) rx: mpsc::UnboundedReceiver<Cmd>,
    /// Released on every way out of the driver, including a panic.
    pub(super) holds: Option<JobHolds>,
    pub(super) ticks: u32,
    /// Permissions the last tick's poll found revoked: one is acted on only when it shows on
    /// two ticks in a row (PRM-1), so a transient OS answer cannot end a take.
    pub(super) revoked_prev: Vec<Permission>,
}

fn cancelled_error() -> DaemonError {
    DaemonError::Declined {
        reason: "countdown_cancelled",
        message: "The recording was cancelled before it started".to_owned(),
    }
}

impl Driver {
    fn publish(&self, name: &str, data: Value) {
        self.eng.publish(&self.sh, name, data);
    }

    fn set_phase(&self, p: Phase) {
        lock(&self.sh.meta).phase = p;
    }

    fn phase(&self) -> Phase {
        lock(&self.sh.meta).phase
    }

    fn acquire_holds(&self) -> JobHolds {
        let include_upload = self
            .eng
            .deps
            .settings
            .all()
            .ok()
            .and_then(|s| s.get(PAUSE_UPLOADS_SETTING).and_then(Value::as_bool))
            .unwrap_or(true);
        JobHolds::acquire(self.eng.deps.jobs.as_ref(), include_upload)
    }

    async fn with_session<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut RecordingSession) -> Result<T, CmError> + Send + 'static,
    ) -> Result<T, CmError> {
        let sh = self.sh.clone();
        let joined = tokio::task::spawn_blocking(move || {
            let mut g = lock(&sh.session);
            match g.as_mut() {
                Some(s) => f(s),
                None => Err(CmError::internal("the recording session is gone")),
            }
        })
        .await;
        match joined {
            Ok(r) => r,
            Err(e) => Err(CmError::internal(format!("worker failed: {e}"))),
        }
    }

    pub(super) async fn run(
        mut self,
        mut prep: PreparedStart,
        ready: oneshot::Sender<Result<Value, DaemonError>>,
    ) {
        let mut ready = Some(ready);
        // Holds carried over by a restart live in the driver, so every way out releases them
        // before anyone is answered.
        self.holds = prep.holds.take();
        if prep.resolved.countdown_s > 0 {
            let total = prep.resolved.countdown_s;
            match self.countdown(total, "start", ready.as_mut()).await {
                CountdownEnd::Elapsed | CountdownEnd::Skipped => {}
                CountdownEnd::StarterGone => {
                    self.cancel_start("starter_gone", None, None, ready.take());
                    return;
                }
                CountdownEnd::Cancelled(reply) => {
                    self.cancel_start("countdown_cancelled", reply, None, ready.take());
                    return;
                }
                CountdownEnd::Interrupted(Cmd::Discard { reply, .. }) => {
                    self.cancel_start("countdown_cancelled", None, Some(reply), ready.take());
                    return;
                }
                CountdownEnd::Interrupted(Cmd::Stop { reply, .. }) => {
                    self.cancel_start("countdown_cancelled", Some(reply), None, ready.take());
                    return;
                }
                CountdownEnd::Interrupted(_) => {
                    self.cancel_start("countdown_cancelled", None, None, ready.take());
                    return;
                }
            }
        }

        self.set_phase(Phase::Starting);
        if self.holds.is_none() {
            self.holds = Some(self.acquire_holds());
        }
        // R3-4: tell the HUD and the indicator now. `begin` waits for the slowest track (the
        // camera up to 3 s, ScreenCaptureKit up to 20 s); the indicator must not wait for it
        // (REC-5.AC1: 200 ms, NFR-11).
        self.publish(
            events::RECORDING_STARTING,
            json!({
                "by": self.sh.by,
                "indicator": self.sh.indicator,
                "tracks_planned": tracks_planned(&prep.resolved),
                "countdown_s": prep.resolved.countdown_s,
                "restart_of": prep.restart_of,
            }),
        );
        let session = match self.begin(&prep).await {
            Ok(s) => s,
            Err(e) => {
                self.holds = None;
                let err: DaemonError = e.into();
                // R4-1: UIs that showed the starting state reset on this.
                self.publish(
                    events::RECORDING_DISCARDED,
                    json!({ "capture_id": null, "reason": "start_failed",
                            "error": error_json(&err) }),
                );
                self.eng.conclude(
                    &self.sh,
                    json!({ "recording_id": self.sh.id, "state": "failed", "error": error_json(&err) }),
                );
                if let Some(r) = ready.take() {
                    let _ = r.send(Err(err));
                }
                return;
            }
        };

        let status = session.status();
        let live_at = session.live_at();
        let capture_id = session.capture_id().to_owned();
        let title = session.title().to_owned();
        let started: Vec<&str> = status
            .tracks
            .iter()
            .filter(|t| t.state == TrackState::Running)
            .map(|t| kind_name(t.kind))
            .collect();
        let mut warnings: Vec<Value> = session.warnings().iter().map(note_json).collect();
        for w in &prep.warnings {
            warnings.push(
                json!({ "track": w.track, "code": w.code, "message": w.message,
                                  "level": "warning", "at_ms": null }),
            );
        }
        let started_at = rfc3339(self.eng.deps.clock.now());
        *lock(&self.sh.session) = Some(session);
        {
            let mut m = lock(&self.sh.meta);
            m.capture_id = Some(capture_id.clone());
            m.title = Some(title.clone());
            m.warnings = warnings.clone();
            m.started_at = Some(started_at.clone());
            m.status = Some(session_status_json(&status));
            m.phase = Phase::Recording;
            m.countdown_remaining = None;
        }
        let started_json = json!({
            "recording_id": self.sh.id,
            "capture_id": capture_id,
            "title": title,
            "state": "recording",
            "live_at_ms": live_at.as_millis_floor(),
            "started_at": started_at,
            "by": self.sh.by,
            "countdown_s": prep.resolved.countdown_s,
            "tracks": started,
            "warnings": warnings,
            "indicator": self.sh.indicator,
            "restart_of": prep.restart_of,
            "max_duration_s": prep.resolved.max_duration_s,
        });
        self.publish(events::RECORDING_STARTED, started_json.clone());
        for w in &warnings {
            self.publish(events::RECORDING_WARNING, w.clone());
        }
        if let Some(r) = ready.take() {
            let _ = r.send(Ok(started_json));
        }
        drop(prep);
        self.run_loop().await;
    }

    async fn begin(&self, prep: &PreparedStart) -> Result<RecordingSession, CmError> {
        let eng = self.eng.clone();
        let resolved = prep.resolved.clone();
        let plan = prep.plan.clone();
        let targets = prep.targets.clone();
        let root = prep.library_root.clone();
        let by = self.sh.by.clone();
        let joined = tokio::task::spawn_blocking(move || {
            let tracks = eng.factory.build(&BuildContext {
                params: &resolved,
                plan: &plan,
                targets: &targets,
            })?;
            let mut cfg = SessionConfig::new(&root);
            cfg.title = resolved.title.clone();
            cfg.settings = resolved.settings_json(&by);
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
        .await;
        match joined {
            Ok(r) => r,
            Err(e) => Err(CmError::internal(format!("worker failed: {e}"))),
        }
    }

    /// The run ended before any file existed (countdown cancelled).
    fn cancel_start(
        &mut self,
        reason: &str,
        stop_reply: Option<Reply>,
        discard_reply: Option<oneshot::Sender<Result<DiscardDone, DaemonError>>>,
        ready: Option<oneshot::Sender<Result<Value, DaemonError>>>,
    ) {
        lock(&self.sh.meta).countdown_remaining = None;
        let value = json!({
            "recording_id": self.sh.id, "state": "cancelled", "cancelled": true,
            "capture_id": null, "reason": reason,
        });
        self.publish(
            events::RECORDING_DISCARDED,
            json!({ "capture_id": null, "reason": reason }),
        );
        self.holds = None;
        self.eng.conclude(&self.sh, value.clone());
        if let Some(r) = stop_reply {
            let _ = r.send(Ok(value.clone()));
        }
        if let Some(r) = discard_reply {
            let _ = r.send(Ok(DiscardDone {
                value: value.clone(),
                holds: None,
            }));
        }
        if let Some(r) = ready {
            let _ = r.send(Err(cancelled_error()));
        }
    }

    /// Counts down one second at a time, answering commands as they arrive.
    async fn countdown(
        &mut self,
        total: u32,
        context: &'static str,
        mut ready: Option<&mut oneshot::Sender<Result<Value, DaemonError>>>,
    ) -> CountdownEnd {
        for remaining in (1..=total).rev() {
            lock(&self.sh.meta).countdown_remaining = Some(remaining);
            self.publish(
                events::RECORDING_COUNTDOWN,
                json!({ "remaining": remaining, "total": total, "context": context }),
            );
            let sleep = tokio::time::sleep(self.eng.opts.countdown_step);
            tokio::pin!(sleep);
            loop {
                let gone = async {
                    match ready.as_mut() {
                        Some(r) => r.closed().await,
                        None => std::future::pending().await,
                    }
                };
                tokio::select! {
                    () = &mut sleep => break,
                    () = gone => {
                        lock(&self.sh.meta).countdown_remaining = None;
                        return CountdownEnd::StarterGone;
                    }
                    cmd = self.rx.recv() => match cmd {
                        None => {
                            lock(&self.sh.meta).countdown_remaining = None;
                            return CountdownEnd::Cancelled(None);
                        }
                        Some(Cmd::Skip(r)) => {
                            lock(&self.sh.meta).countdown_remaining = None;
                            let _ = r.send(Ok(json!({ "recording_id": self.sh.id, "skipped": true })));
                            return CountdownEnd::Skipped;
                        }
                        Some(Cmd::Pause(r)) => {
                            lock(&self.sh.meta).countdown_remaining = None;
                            return CountdownEnd::Cancelled(Some(r));
                        }
                        Some(Cmd::Resume(r)) => {
                            let _ = r.send(if context == "start" {
                                Err(CmError::invalid_input("The recording has not started yet").into())
                            } else {
                                Ok(json!({ "state": "resuming", "changed": false }))
                            });
                        }
                        Some(other) => {
                            lock(&self.sh.meta).countdown_remaining = None;
                            return CountdownEnd::Interrupted(other);
                        }
                    },
                }
            }
        }
        lock(&self.sh.meta).countdown_remaining = None;
        CountdownEnd::Elapsed
    }

    async fn run_loop(&mut self) {
        let mut ticker = tokio::time::interval(self.eng.opts.tick_every);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        ticker.tick().await;
        loop {
            tokio::select! {
                cmd = self.rx.recv() => match cmd {
                    Some(c) => {
                        if matches!(self.handle(c).await, Flow::End) {
                            return;
                        }
                    }
                    None => {
                        let _ = self.stop("engine_dropped").await;
                        return;
                    }
                },
                _ = ticker.tick() => {
                    if matches!(self.on_tick().await, Flow::End) {
                        return;
                    }
                }
            }
        }
    }

    async fn handle(&mut self, cmd: Cmd) -> Flow {
        let mut next = Some(cmd);
        while let Some(c) = next.take() {
            match c {
                Cmd::Pause(reply) => {
                    let r = self.pause().await;
                    let _ = reply.send(r);
                    return Flow::Continue;
                }
                Cmd::Resume(reply) => {
                    next = self.resume(reply).await;
                    if next.is_none() {
                        return Flow::Continue;
                    }
                }
                Cmd::Skip(reply) => {
                    let _ = reply.send(Ok(json!({ "recording_id": self.sh.id, "skipped": false })));
                    return Flow::Continue;
                }
                Cmd::Stop { reason, reply } => {
                    let r = self.stop(&reason).await;
                    let _ = reply.send(r);
                    return Flow::End;
                }
                Cmd::Discard {
                    reason,
                    keep_holds,
                    reply,
                } => {
                    let r = self.discard(&reason, keep_holds).await;
                    let _ = reply.send(r);
                    return Flow::End;
                }
            }
        }
        Flow::Continue
    }

    async fn pause(&mut self) -> Result<Value, DaemonError> {
        match self.phase() {
            Phase::Paused | Phase::Resuming => {
                return Ok(
                    json!({ "recording_id": self.sh.id, "state": "paused", "changed": false }),
                )
            }
            Phase::Recording => {}
            other => {
                return Err(CmError::invalid_input(format!(
                    "The recording is {}, not running",
                    other.as_str()
                ))
                .into())
            }
        }
        if lock(&self.sh.meta).pauses >= MAX_PAUSES {
            return Err(CmError::invalid_input(
                "Too many pauses \u{2014} stop and start a new recording",
            )
            .into());
        }
        let st = self
            .with_session(|s| {
                s.pause()?;
                Ok(s.status())
            })
            .await?;
        {
            let mut m = lock(&self.sh.meta);
            m.phase = Phase::Paused;
            m.pauses += 1;
            m.status = Some(session_status_json(&st));
        }
        let recorded = st.recorded.as_millis_floor();
        self.publish(events::RECORDING_PAUSED, json!({ "recorded_ms": recorded }));
        Ok(
            json!({ "recording_id": self.sh.id, "state": "paused", "changed": true,
                   "recorded_ms": recorded }),
        )
    }

    /// Resumes after the resume countdown. Returns a command that interrupted the countdown and
    /// still needs handling.
    async fn resume(&mut self, reply: Reply) -> Option<Cmd> {
        match self.phase() {
            Phase::Recording => {
                let _ = reply.send(Ok(
                    json!({ "recording_id": self.sh.id, "state": "recording", "changed": false }),
                ));
                return None;
            }
            Phase::Paused => {}
            other => {
                let _ = reply.send(Err(CmError::invalid_input(format!(
                    "The recording is {}, not paused",
                    other.as_str()
                ))
                .into()));
                return None;
            }
        }
        let settings = self.eng.deps.settings.all().unwrap_or_default();
        let n = resume_countdown_setting(&settings);
        if n > 0 {
            self.set_phase(Phase::Resuming);
            let cancelled = |reply: Reply, id: &str| {
                let _ = reply.send(Ok(
                    json!({ "recording_id": id, "state": "paused", "resumed": false, "cancelled": true }),
                ));
            };
            match self.countdown(n, "resume", None).await {
                CountdownEnd::Elapsed | CountdownEnd::Skipped => {}
                CountdownEnd::Cancelled(r) => {
                    self.set_phase(Phase::Paused);
                    if let Some(r) = r {
                        let _ = r.send(Ok(
                            json!({ "recording_id": self.sh.id, "state": "paused", "changed": false }),
                        ));
                    }
                    cancelled(reply, &self.sh.id);
                    return None;
                }
                CountdownEnd::Interrupted(cmd) => {
                    self.set_phase(Phase::Paused);
                    cancelled(reply, &self.sh.id);
                    return Some(cmd);
                }
                CountdownEnd::StarterGone => {}
            }
        }
        let r = self
            .with_session(|s| {
                s.resume()?;
                Ok(s.status())
            })
            .await;
        match r {
            Ok(st) => {
                {
                    let mut m = lock(&self.sh.meta);
                    m.phase = Phase::Recording;
                    m.status = Some(session_status_json(&st));
                }
                self.publish(
                    events::RECORDING_RESUMED,
                    json!({ "recorded_ms": st.recorded.as_millis_floor(),
                            "paused_ms": st.paused_total.as_millis_floor() }),
                );
                let _ = reply.send(Ok(
                    json!({ "recording_id": self.sh.id, "state": "recording", "changed": true,
                            "paused_ms": st.paused_total.as_millis_floor() }),
                ));
            }
            Err(e) => {
                self.set_phase(Phase::Paused);
                let _ = reply.send(Err(e.into()));
            }
        }
        None
    }

    async fn on_tick(&mut self) -> Flow {
        self.eng.factory.tick_hook();
        self.ticks += 1;
        if !matches!(
            self.phase(),
            Phase::Recording | Phase::Paused | Phase::Resuming
        ) {
            return Flow::Continue;
        }
        let factory = self.eng.factory.clone();
        let prev = std::mem::take(&mut self.revoked_prev);
        let ticked = self
            .with_session(move |s| {
                let mut notes = s.tick();
                let seen = factory.revoked_permissions(&s.running_kinds());
                for p in seen.iter().filter(|p| prev.contains(p)) {
                    notes.extend(s.report_permission_revoked(*p));
                }
                Ok((notes, s.status(), seen))
            })
            .await
            .map(|(notes, st, seen)| {
                self.revoked_prev = seen;
                (notes, st)
            });
        let (notes, st) = match ticked {
            Ok(x) => x,
            Err(e) => {
                self.publish(
                    events::RECORDING_WARNING,
                    json!({ "code": "tick_failed", "level": "warning", "track": null,
                            "message": e.to_string() }),
                );
                return Flow::Continue;
            }
        };
        let mut stop: Option<String> = None;
        for n in &notes {
            if n.level == NoteLevel::Info {
                continue;
            }
            let w = note_json(n);
            lock(&self.sh.meta).warnings.push(w.clone());
            self.publish(events::RECORDING_WARNING, w);
            if n.level == NoteLevel::StopRequested && stop.is_none() {
                stop = Some(n.code.clone());
            }
        }
        lock(&self.sh.meta).status = Some(session_status_json(&st));
        let snapshot = {
            let m = lock(&self.sh.meta);
            build_status(&self.sh, &m, Some(&st))
        };
        self.publish(events::RECORDING_STATS, snapshot.clone());
        if self
            .ticks
            .is_multiple_of(self.eng.opts.health_every_ticks.max(1))
        {
            let late = snapshot["recent_late_frame_pct"].as_f64().unwrap_or(0.0);
            self.publish(
                events::RECORDING_HEALTH,
                json!({
                    "status": if late > 2.0 { "degraded" } else { "ok" },
                    "late_frame_pct": snapshot["late_frame_pct"],
                    "recent_late_frame_pct": snapshot["recent_late_frame_pct"],
                    "disk_left_minutes": snapshot["disk_left_minutes"],
                }),
            );
        }
        if stop.is_none() {
            stop = st.stop_requested.clone();
        }
        if stop.is_none() {
            if let Some(max) = self.sh.resolved.max_duration_s {
                if st.recorded.as_millis_floor() as f64 >= max * 1000.0 {
                    stop = Some("max_duration".to_owned());
                }
            }
        }
        match stop {
            Some(reason) => {
                let _ = self.stop(&reason).await;
                Flow::End
            }
            None => Flow::Continue,
        }
    }

    async fn stop(&mut self, reason: &str) -> Result<Value, DaemonError> {
        self.set_phase(Phase::Stopping);
        lock(&self.sh.meta).stop_reason = Some(reason.to_owned());
        let session = lock(&self.sh.session).take();
        let Some(session) = session else {
            return Err(self.fail(CmError::internal("the recording session is gone").into()));
        };
        let lib = self.eng.deps.library.clone();
        let by = self.sh.by.clone();
        let reason_owned = reason.to_owned();
        let layout = self.eng.layout_at_stop(&self.sh);
        let mut job = tokio::task::spawn_blocking(move || {
            finalize_blocking(session, &lib, &reason_owned, &by, layout)
        });
        let res = tokio::time::timeout(self.eng.opts.stop_timeout, &mut job).await;
        match res {
            Ok(joined) => {
                // Exports and uploads resume before anyone is told the recording is over.
                self.holds = None;
                match joined {
                    Ok(Ok(done)) => {
                        let v = outcome_json(&self.sh, &done, reason);
                        self.eng.finish_stop(&self.sh, &done, reason, &v);
                        Ok(v)
                    }
                    Ok(Err(e)) => Err(self.fail(e.into())),
                    Err(e) => Err(self.fail(DaemonError::Internal(format!("worker failed: {e}")))),
                }
            }
            Err(_) => {
                // R3-10: the encoder's own stop can take longer than this wait (SCK 10 s +
                // hardware encoder 60 s + camera 30 s). The blocking job keeps running, so the
                // engine stays in `Stopping` (no new recording, job holds kept) until it truly
                // ends, and the late result is published then.
                let eng = self.eng.clone();
                let sh = self.sh.clone();
                let holds = self.holds.take();
                let reason = reason.to_owned();
                tokio::spawn(async move {
                    let joined = job.await;
                    drop(holds);
                    match joined {
                        Ok(Ok(done)) => {
                            let v = outcome_json(&sh, &done, &reason);
                            eng.finish_stop(&sh, &done, &reason, &v);
                        }
                        Ok(Err(e)) => {
                            eng.fail_stop(&sh, &DaemonError::from(e));
                        }
                        Err(e) => {
                            eng.fail_stop(
                                &sh,
                                &DaemonError::Internal(format!("worker failed: {e}")),
                            );
                        }
                    }
                });
                Err(CmError::internal(format!(
                    "Stopping is taking longer than {:.1} s. It keeps finishing in the background: \
                     the recording appears in the library when it is done (a new recording cannot \
                     start until then)",
                    self.eng.opts.stop_timeout.as_secs_f64()
                ))
                .into())
            }
        }
    }

    /// A stop that did not produce a capture: publish the terminal event, return to Idle.
    fn fail(&self, e: DaemonError) -> DaemonError {
        self.eng.fail_stop(&self.sh, &e);
        e
    }

    async fn discard(
        &mut self,
        reason: &str,
        keep_holds: bool,
    ) -> Result<DiscardDone, DaemonError> {
        self.set_phase(Phase::Stopping);
        let session = lock(&self.sh.session).take();
        let capture_id = lock(&self.sh.meta).capture_id.clone();
        if let Some(session) = session {
            let job = tokio::task::spawn_blocking(move || session.discard());
            let _ = tokio::time::timeout(self.eng.opts.stop_timeout, job).await;
        }
        let holds = if keep_holds { self.holds.take() } else { None };
        self.holds = None;
        let value = json!({
            "recording_id": self.sh.id, "state": "discarded", "discarded": true,
            "capture_id": capture_id, "reason": reason,
        });
        self.publish(
            events::RECORDING_DISCARDED,
            json!({ "capture_id": capture_id, "reason": reason }),
        );
        if keep_holds {
            // A restart: the slot stays taken until the new take replaces it (R3-10).
            self.eng.conclude_keeping_slot(&self.sh, value.clone());
        } else {
            self.eng.conclude(&self.sh, value.clone());
        }
        Ok(DiscardDone { value, holds })
    }
}
