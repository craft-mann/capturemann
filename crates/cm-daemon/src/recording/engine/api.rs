use super::*;

impl RecordingEngine {
    pub(crate) fn new(
        deps: EngineDeps,
        factory: Arc<dyn SourceFactory>,
        opts: EngineOptions,
    ) -> Self {
        // A killed self-test leaves its screen capture behind: clean up what is older than a
        // minute, in the default and the configured folder (R3-16).
        let min_age = Duration::from_secs(60);
        sweep_selftest_dirs(&std::env::temp_dir(), min_age);
        if let Some(d) = &opts.selftest_dir {
            sweep_selftest_dirs(d, min_age);
        }
        RecordingEngine {
            inner: Arc::new(EngineInner {
                deps,
                factory,
                opts,
                state: Mutex::new(State::default()),
                export_hint_sent: AtomicBool::new(false),
            }),
        }
    }

    /// Whether the engine runs `method` (so a Confirm-tier action is available).
    pub fn handles(method: &str) -> bool {
        ENGINE_ACTIONS.contains(&method)
    }

    /// Whether the 60 fps self-test is running.
    pub fn selftest_running(&self) -> bool {
        lock(&self.inner.state).selftest.is_some()
    }

    /// Whether a recording is in progress (any phase).
    pub fn is_active(&self) -> bool {
        lock(&self.inner.state).active.is_some()
    }

    /// The `list_devices` result.
    pub async fn devices(&self) -> Result<Value, DaemonError> {
        let f = self.inner.factory.clone();
        crate::daemon::blocking(move || Ok(f.devices())).await
    }

    // ---- start ---------------------------------------------------------------------------

    /// Validates a `start_recording` request and plans it (targets, size, permissions, device
    /// names) without creating anything. Errors here (`invalid_input`, `not_found`, `busy`,
    /// `permission_missing`) surface before the confirmation dialog opens.
    pub async fn prepare_start(
        &self,
        actor: &Actor,
        params: &ExternalParams,
    ) -> Result<PreparedStart, DaemonError> {
        let parsed = StartParams::parse(&params.rest)?;
        self.ensure_idle()?;
        let settings = self.inner.deps.settings.all().unwrap_or_default();
        let mut resolved = parsed.resolve(&settings);
        // The camera and microphone the request left unnamed: the devices set in Settings ->
        // Webcam & audio when they are connected, else the system default.
        if crate::recording::params::wants_configured_device(&resolved, &settings) {
            if let Ok(devices) = self.devices().await {
                let (cameras, mics) = crate::recording::params::listed_device_ids(&devices);
                resolved.apply_default_devices(&settings, &cameras, &mics);
            }
        }
        // 60 fps needs a passing self-test (REC-3): refused when asked for, dropped to 30 when it
        // only came from the `recording.fps` default.
        let gate = Some(parsed.fps.is_some());
        self.prepare(actor, resolved, settings, None, gate).await
    }

    /// Plans a restart: the running recording's settings, a fresh countdown (REC-5, OD-4).
    pub async fn prepare_restart(&self, actor: &Actor) -> Result<PreparedStart, DaemonError> {
        let sh = self.active_for(None)?;
        let settings = self.inner.deps.settings.all().unwrap_or_default();
        let mut resolved = sh.resolved.clone();
        resolved.countdown_s = countdown_setting(&settings);
        // A restart keeps the settings the running take was accepted with: no gate.
        self.prepare(actor, resolved, settings, Some(sh.id.clone()), None)
            .await
    }

    pub(super) fn ensure_idle(&self) -> Result<(), DaemonError> {
        let st = lock(&self.inner.state);
        if st.active.is_some() {
            return Err(CmError::busy("A recording is already running").into());
        }
        if st.selftest.is_some() {
            return Err(selftest_busy().into());
        }
        Ok(())
    }

    pub(super) fn exclusions(
        &self,
        r: &ResolvedStart,
        settings: &Map<String, Value>,
    ) -> Exclusions {
        let mut bundle_ids: Vec<String> = Vec::new();
        let stored = settings
            .get("privacy.excluded_apps")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned);
        for id in self
            .inner
            .opts
            .default_excluded_bundle_ids
            .iter()
            .cloned()
            .chain(stored)
            .chain(r.exclude.bundle_ids.iter().cloned())
        {
            if !bundle_ids.contains(&id) {
                bundle_ids.push(id);
            }
        }
        // The kept windows only matter while the own process is excluded as a whole.
        let provider = self
            .inner
            .opts
            .own_pid
            .and(self.inner.opts.kept_windows.clone())
            .map(WindowProvider::from_arc);
        Exclusions {
            window_ids: r.exclude.window_ids.clone(),
            pids: self.inner.opts.own_pid.into_iter().collect(),
            bundle_ids,
            keep_window_ids: provider
                .as_ref()
                .map(WindowProvider::ids)
                .unwrap_or_default(),
            keep_provider: provider,
        }
    }

    async fn prepare(
        &self,
        actor: &Actor,
        resolved: ResolvedStart,
        settings: Map<String, Value>,
        restart_of: Option<String>,
        fps60_gate: Option<bool>,
    ) -> Result<PreparedStart, DaemonError> {
        let mut resolved = resolved;
        if resolved.mode == RecordMode::Audio {
            return self
                .prepare_audio(actor, resolved, restart_of, &settings)
                .await;
        }
        let exclusions = self.exclusions(&resolved, &settings);
        let (mut plan, targets, mut warnings, names, root) =
            self.plan_blocking(&resolved, exclusions).await?;
        if let (Some(explicit), 60) = (fps60_gate, plan.video.fps) {
            let fp = Fingerprint::new(
                plan.display_id,
                &plan.video,
                resolved.size,
                resolved.codec,
                &self.inner.opts.app_version,
            );
            if let Fps60Verdict::Rejected(r) =
                selftest::evaluate(&settings, &fp, self.inner.deps.clock.now())
            {
                if explicit {
                    return Err(r.into_error());
                }
                resolved.fps = 30;
                let exclusions = self.exclusions(&resolved, &settings);
                let again = self.plan_blocking(&resolved, exclusions).await?;
                plan = again.0;
                warnings.push(PreflightWarning {
                    track: None,
                    code: "fps60_not_validated".to_owned(),
                    message: format!("Recording at 30 fps. {}", r.message),
                });
            }
        }
        // R3-8: the 60 fps verdict was measured with the screen only. With the camera on the
        // machine has more to do, so say it was not verified for that combination.
        if plan.video.fps == 60 && resolved.camera.is_some() {
            warnings.push(PreflightWarning {
                track: Some("camera".to_owned()),
                code: "fps60_unverified_with_camera".to_owned(),
                message: "The 60 fps check measured the screen alone. With the camera on, frames \
                          may arrive late; watch the health indicator or record at 30 fps"
                    .to_owned(),
            });
        }
        // R3-9 / REC-8.AC4: the low-disk warning belongs before the recording, in the plan.
        let bitrate = planned_bitrate_bps(
            plan.video.bitrate_bps,
            resolved.camera.is_some(),
            resolved.mic.is_some(),
            resolved.system_audio,
            resolved.cursor,
        );
        let disk = self.disk_plan(&root, bitrate, &mut warnings)?;
        let indicator = indicator_json(&resolved, &plan, &targets, actor);
        let (summary, mut details, mut params) = describe(
            &resolved,
            &plan,
            &targets,
            &names,
            &root,
            restart_of.is_some(),
        );
        if let (Some((text, typed)), Some(o)) = (disk, details.as_object_mut()) {
            o.insert("disk_space".into(), json!(text));
            if let Some(p) = params.as_object_mut() {
                p.insert("disk".into(), typed);
            }
        }
        let camera_settings = crate::export::camera_settings(&settings);
        Ok(PreparedStart {
            resolved,
            plan: Some(plan),
            targets: Some(targets),
            library_root: root,
            warnings,
            indicator,
            restart_of,
            holds: None,
            camera_settings,
            summary,
            details,
            params,
        })
    }

    /// The disk part of a plan (R3-9 / REC-8.AC4): refuses to start with under 1 GB free, adds the
    /// `disk_low` warning, and returns the dialog row (text and typed values) when the free space
    /// is known.
    fn disk_plan(
        &self,
        root: &Path,
        bitrate: u64,
        warnings: &mut Vec<PreflightWarning>,
    ) -> Result<Option<(String, Value)>, DaemonError> {
        let Some(free) = self.inner.opts.free_space.free_bytes(root) else {
            return Ok(None);
        };
        Ok(match assess_disk(free, bitrate) {
            DiskStatus::Full { free_bytes } => {
                return Err(CmError::disk_full(format!(
                    "Only {} MB free; recording needs more than 1 GB free to start",
                    free_bytes / 1_000_000
                ))
                .into());
            }
            DiskStatus::Low { minutes_left } => {
                warnings.push(PreflightWarning {
                    track: None,
                    code: "disk_low".to_owned(),
                    message: format!(
                        "About {minutes_left:.0} minutes of disk space left at this setting"
                    ),
                });
                Some((
                    disk_text(free, minutes_left),
                    disk_params(free, minutes_left),
                ))
            }
            DiskStatus::Ok { minutes_left } => Some((
                disk_text(free, minutes_left),
                disk_params(free, minutes_left),
            )),
        })
    }

    /// Plans an audio recording (VOC-1): the permission preflight (a microphone-only plan never
    /// asks for Screen Recording; system audio without it is `permission_missing` `screen`), the
    /// device names and the disk check. There is no screen to plan.
    async fn prepare_audio(
        &self,
        actor: &Actor,
        resolved: ResolvedStart,
        restart_of: Option<String>,
        settings: &Map<String, Value>,
    ) -> Result<PreparedStart, DaemonError> {
        let eng = self.inner.clone();
        let req = SensorRequest::from_resolved(&resolved);
        let (mut warnings, names, root) = crate::daemon::blocking(move || {
            let pre = eng.factory.preflight(&req);
            if let Some(e) = pre.errors.into_iter().next() {
                return Err(e);
            }
            let names = eng.factory.device_names(&req);
            let root = lock_library(&eng.deps.library).root().to_owned();
            Ok((pre.warnings, names, root))
        })
        .await?;
        let bitrate = planned_bitrate_bps(
            0,
            false,
            resolved.mic.is_some(),
            resolved.system_audio,
            false,
        );
        let disk = self.disk_plan(&root, bitrate, &mut warnings)?;
        let indicator = indicator_json_audio(&resolved, actor);
        let consent = settings
            .get("recording.consent_notice")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let (summary, mut details, mut params) =
            describe_audio(&resolved, &names, &root, restart_of.is_some(), consent);
        if let (Some((text, typed)), Some(o)) = (disk, details.as_object_mut()) {
            o.insert("disk_space".into(), json!(text));
            if let Some(p) = params.as_object_mut() {
                p.insert("disk".into(), typed);
            }
        }
        Ok(PreparedStart {
            resolved,
            plan: None,
            targets: None,
            library_root: root,
            warnings,
            indicator,
            restart_of,
            holds: None,
            camera_settings: Map::new(),
            summary,
            details,
            params,
        })
    }

    /// Plans `resolved` against the current targets and runs the permission preflight. Blocking
    /// work, off the async threads.
    pub(super) async fn plan_blocking(
        &self,
        resolved: &ResolvedStart,
        exclusions: Exclusions,
    ) -> Result<
        (
            CapturePlan,
            Targets,
            Vec<PreflightWarning>,
            DeviceNames,
            PathBuf,
        ),
        DaemonError,
    > {
        let eng = self.inner.clone();
        let r2 = resolved.clone();
        crate::daemon::blocking(move || {
            let target = r2
                .target
                .ok_or_else(|| CmError::internal("a screen recording has no target"))?;
            let want_windows = matches!(target, ScreenshotTarget::Window { .. });
            let targets = eng.deps.capturer.list_targets_with(want_windows)?;
            let mut screen = ScreenSettings::new(target.to_capture_target());
            screen.video = VideoSettings {
                size: r2.size,
                fps: r2.fps,
                codec: r2.codec,
                quality: r2.quality,
            };
            screen.exclusions = exclusions;
            // "Record cursor separately" (EDT-4): the pointer is not in the picture.
            screen.show_cursor = !r2.cursor_separate;
            let plan = plan_capture(&screen, &targets)?;
            let req = SensorRequest::from_resolved(&r2);
            let pre = eng.factory.preflight(&req);
            if let Some(e) = pre.errors.into_iter().next() {
                return Err(e);
            }
            let names = eng.factory.device_names(&req);
            let root = lock_library(&eng.deps.library).root().to_owned();
            Ok((plan, targets, pre.warnings, names, root))
        })
        .await
    }

    /// Starts the recording and returns when every track is delivering (after the countdown),
    /// with `{ recording_id, capture_id, live_at_ms, indicator, ... }`. If the caller goes away
    /// before that, the countdown is cancelled.
    pub async fn start(&self, actor: &Actor, prep: PreparedStart) -> Result<Value, DaemonError> {
        self.start_replacing(actor, prep, None).await
    }

    /// [`RecordingEngine::start`], taking over the slot of `replacing` (a restart): nothing else
    /// can start between the discard of the old take and the start of the new one.
    async fn start_replacing(
        &self,
        actor: &Actor,
        prep: PreparedStart,
        replacing: Option<&str>,
    ) -> Result<Value, DaemonError> {
        let (sh, rx) = match self.reserve(actor, &prep, replacing) {
            Ok(x) => x,
            Err(e) => {
                if let Some(old) = replacing {
                    self.inner.release_replaced(old);
                }
                return Err(e);
            }
        };
        let (ready_tx, ready_rx) = oneshot::channel();
        let driver = Driver {
            eng: self.inner.clone(),
            sh: sh.clone(),
            rx,
            holds: None,
            ticks: 0,
            revoked_prev: Vec::new(),
            idle_guard: None,
            last_tick_wall: None,
        };
        let handle = tokio::spawn(driver.run(prep, ready_tx));
        let eng = self.inner.clone();
        tokio::spawn(async move {
            if let Err(e) = handle.await {
                eng.salvage(&sh, e.is_panic()).await;
            }
        });
        match ready_rx.await {
            Ok(r) => r,
            Err(_) => Err(DaemonError::Internal(
                "the recording engine stopped unexpectedly while starting".to_owned(),
            )),
        }
    }

    fn reserve(
        &self,
        actor: &Actor,
        prep: &PreparedStart,
        replacing: Option<&str>,
    ) -> Result<(Arc<Shared>, mpsc::UnboundedReceiver<Cmd>), DaemonError> {
        let mut st = lock(&self.inner.state);
        if st
            .active
            .as_ref()
            .is_some_and(|a| Some(a.id.as_str()) != replacing)
        {
            return Err(CmError::busy("A recording is already running").into());
        }
        if st.selftest.is_some() {
            return Err(selftest_busy().into());
        }
        st.seq += 1;
        let ms = self.inner.deps.clock.now().unix_timestamp_nanos() / 1_000_000;
        let id = format!("rec_{ms:012x}_{:04}", st.seq);
        let (tx, rx) = mpsc::unbounded_channel();
        let sh = Arc::new(Shared {
            id,
            tx,
            meta: Mutex::new(Meta {
                phase: if prep.resolved.countdown_s > 0 {
                    Phase::Countdown
                } else {
                    Phase::Starting
                },
                countdown_remaining: None,
                capture_id: None,
                title: None,
                status: None,
                warnings: Vec::new(),
                pauses: 0,
                stop_reason: None,
                started_at: None,
            }),
            session: Mutex::new(None),
            resolved: prep.resolved.clone(),
            by: actor.to_string(),
            indicator: prep.indicator.clone(),
            camera_at_start: prep.camera_settings.clone(),
            restart_of: prep.restart_of.clone(),
        });
        st.active = Some(sh.clone());
        Ok((sh, rx))
    }

    // ---- commands ------------------------------------------------------------------------

    fn active_for(&self, id: Option<&str>) -> Result<Arc<Shared>, DaemonError> {
        let st = lock(&self.inner.state);
        match (&st.active, id) {
            (Some(a), Some(i)) if a.id != i => Err(CmError::not_found(format!(
                "`{i}` is not the running recording (that is `{}`)",
                a.id
            ))
            .into()),
            (Some(a), _) => Ok(a.clone()),
            (None, Some(i)) => {
                Err(CmError::not_found(format!("Recording `{i}` is not running")).into())
            }
            (None, None) => Err(CmError::not_found("No recording is running").into()),
        }
    }

    fn last_matching(&self, id: Option<&str>) -> Option<Value> {
        let st = lock(&self.inner.state);
        st.last
            .as_ref()
            .filter(|l| id == Some(l.id.as_str()))
            .map(|l| l.value.clone())
    }

    async fn ask(
        &self,
        id: Option<&str>,
        make: impl FnOnce(Reply) -> Cmd,
    ) -> Result<Value, DaemonError> {
        let sh = self.active_for(id)?;
        let (tx, rx) = oneshot::channel();
        sh.tx
            .send(make(tx))
            .map_err(|_| CmError::not_found("The recording has already ended"))?;
        rx.await.unwrap_or_else(|_| {
            Err(CmError::not_found("The recording ended before it could answer").into())
        })
    }

    /// Pauses (REC-4). During the countdown it cancels the start (REC-4.AC4).
    pub async fn pause(&self, id: Option<&str>) -> Result<Value, DaemonError> {
        self.ask(id, Cmd::Pause).await
    }

    /// Resumes, after the resume countdown (default 1 s).
    pub async fn resume(&self, id: Option<&str>) -> Result<Value, DaemonError> {
        self.ask(id, Cmd::Resume).await
    }

    /// Mutes or unmutes the microphone (AUD-6): `{ recording_id, muted, changed, at_ms }`
    /// (`at_ms` is the session time of the change). Works while recording, paused or in the
    /// resume countdown; it does not go through the driver's command queue, so it never
    /// interrupts a countdown. Without a recording: `not_found` (like pause); during the start
    /// countdown or without a microphone track: `invalid_input`.
    pub async fn set_mic_muted(&self, id: Option<&str>, muted: bool) -> Result<Value, DaemonError> {
        let sh = self.active_for(id)?;
        let phase = lock(&sh.meta).phase;
        if !matches!(phase, Phase::Recording | Phase::Paused | Phase::Resuming) {
            return Err(CmError::invalid_input(format!(
                "The recording is {}, not running",
                phase.as_str()
            ))
            .into());
        }
        let sh2 = sh.clone();
        let joined = tokio::task::spawn_blocking(move || {
            let mut g = lock(&sh2.session);
            match g.as_mut() {
                Some(s) => s.set_mic_muted(muted).map(|c| (c, s.status())),
                None => Err(CmError::internal("the recording session is gone")),
            }
        })
        .await;
        let (change, st) = match joined {
            Ok(r) => r?,
            Err(e) => return Err(CmError::internal(format!("worker failed: {e}")).into()),
        };
        lock(&sh.meta).status = Some(session_status_json(&st));
        let at_ms = change.at.as_millis_floor();
        if change.changed {
            let name = if muted {
                events::RECORDING_MIC_MUTED
            } else {
                events::RECORDING_MIC_UNMUTED
            };
            self.inner.publish(
                &sh,
                name,
                json!({ "at_ms": at_ms, "recorded_ms": st.recorded.as_millis_floor() }),
            );
        }
        Ok(json!({
            "recording_id": sh.id,
            "muted": st.mic_muted,
            "changed": change.changed,
            "at_ms": at_ms,
        }))
    }

    /// Sets a marker on the running recording at the current recorded time (VOC-1.AC4): the HUD
    /// timer, so a pause does not move it. `{ recording_id, marker: { at, at_ms, label, by },
    /// count }`. With `relabel_at` (the `at` of a marker set earlier) nothing is added: the marker
    /// at that time gets `label` (`not_found` when there is none). Works while recording, paused
    /// or in the resume countdown; during the start
    /// countdown it is `invalid_input`, without a recording `not_found`.
    pub async fn add_marker(
        &self,
        actor: &Actor,
        id: Option<&str>,
        label: Option<&str>,
        relabel_at: Option<cm_core::MediaTime>,
    ) -> Result<Value, DaemonError> {
        let sh = self.active_for(id)?;
        let phase = lock(&sh.meta).phase;
        if !matches!(phase, Phase::Recording | Phase::Paused | Phase::Resuming) {
            return Err(CmError::invalid_input(format!(
                "The recording is {}, not running",
                phase.as_str()
            ))
            .into());
        }
        let (sh2, by, label2) = (sh.clone(), actor.to_string(), label.map(str::to_owned));
        let joined = tokio::task::spawn_blocking(move || {
            let mut g = lock(&sh2.session);
            match g.as_mut() {
                Some(s) => match relabel_at {
                    Some(at) => s.relabel_marker(at, label2.as_deref()),
                    None => s.add_marker(label2.as_deref(), &by),
                }
                .map(|m| (m, s.markers().len())),
                None => Err(CmError::internal("the recording session is gone")),
            }
        })
        .await;
        let (marker, count) = match joined {
            Ok(r) => r?,
            Err(e) => return Err(CmError::internal(format!("worker failed: {e}")).into()),
        };
        let at_ms = marker.at.as_millis_floor();
        self.inner.publish(
            &sh,
            events::RECORDING_MARKER,
            json!({ "at_ms": at_ms, "label": marker.label, "by": marker.by, "count": count }),
        );
        Ok(json!({
            "recording_id": sh.id,
            "marker": {
                "at": marker.at.as_secs_f64(),
                "at_ms": at_ms,
                "label": marker.label,
                "by": marker.by,
            },
            "count": count,
        }))
    }

    /// Skips the running countdown (Space, REC-1.AC5).
    pub async fn skip_countdown(&self, id: Option<&str>) -> Result<Value, DaemonError> {
        self.ask(id, Cmd::Skip).await
    }

    /// Stops and finalizes: `{ recording_id, capture_id, duration_s, tracks, reason, ... }`.
    /// Valid while recording, paused, resuming and during the countdown (which then cancels
    /// with nothing recorded). Stopping a recording that already ended on its own returns the
    /// same result with `already_stopped: true`.
    pub async fn stop(&self, id: Option<&str>, reason: &str) -> Result<Value, DaemonError> {
        if let Some(v) = self.cancel_selftest(id).await {
            return Ok(v);
        }
        let already = |mut v: Value| {
            v["already_stopped"] = json!(true);
            v
        };
        let sh = match self.active_for(id) {
            Ok(sh) => sh,
            Err(e) => {
                return match self.last_matching(id) {
                    Some(v) => Ok(already(v)),
                    None => Err(e),
                }
            }
        };
        let (tx, rx) = oneshot::channel();
        let sent = sh
            .tx
            .send(Cmd::Stop {
                reason: reason.to_owned(),
                reply: tx,
            })
            .is_ok();
        let answer = if sent { rx.await.ok() } else { None };
        match answer {
            Some(r) => r,
            // The recording ended on its own just as this call arrived: same answer, no error.
            None => match self.last_matching(Some(&sh.id)) {
                Some(v) => Ok(already(v)),
                None => Err(CmError::not_found("The recording has already ended").into()),
            },
        }
    }

    /// Deletes the take (REC-5.AC3): stops the tracks and removes the bundle. During the
    /// countdown it cancels. Nothing is indexed, nothing is recoverable.
    pub async fn discard(&self, id: Option<&str>, reason: &str) -> Result<Value, DaemonError> {
        if let Some(v) = self.cancel_selftest(id).await {
            return Ok(v);
        }
        let sh = self.active_for(id)?;
        let (tx, rx) = oneshot::channel();
        sh.tx
            .send(Cmd::Discard {
                reason: reason.to_owned(),
                keep_holds: false,
                reply: tx,
            })
            .map_err(|_| CmError::not_found("The recording has already ended"))?;
        match rx.await {
            Ok(r) => r.map(|d| d.value),
            Err(_) => Err(CmError::not_found("The recording ended before it could answer").into()),
        }
    }

    /// Discards the take and starts a new one with `prep` (made by
    /// [`RecordingEngine::prepare_restart`]), keeping the job holds across the gap.
    pub async fn restart(
        &self,
        actor: &Actor,
        mut prep: PreparedStart,
    ) -> Result<Value, DaemonError> {
        let expected = prep.restart_of.clone();
        let sh = self.active_for(expected.as_deref())?;
        let (tx, rx) = oneshot::channel();
        sh.tx
            .send(Cmd::Discard {
                reason: "restart".to_owned(),
                keep_holds: true,
                reply: tx,
            })
            .map_err(|_| CmError::not_found("The recording has already ended"))?;
        let done = rx
            .await
            .map_err(|_| CmError::not_found("The recording ended before it could restart"))??;
        prep.holds = done.holds;
        let mut v = self
            .start_replacing(actor, prep, expected.as_deref())
            .await?;
        v["restart_of"] = json!(expected);
        Ok(v)
    }

    /// A one-sentence description of what a discard deletes, for the confirmation dialog.
    pub fn describe_discard(&self, id: Option<&str>) -> Result<(String, Value), DaemonError> {
        self.describe_discard_pinned(id).map(|(_, s, d, _)| (s, d))
    }

    /// Like [`RecordingEngine::describe_discard`], also returning the id of the recording the
    /// description is about: the discard that follows the dialog must target that id (R2-2).
    pub fn describe_discard_pinned(
        &self,
        id: Option<&str>,
    ) -> Result<(String, String, Value, Value), DaemonError> {
        let sh = self.active_for(id)?;
        let meta = lock(&sh.meta);
        let recorded = meta
            .status
            .as_ref()
            .and_then(|s| s["recorded_ms"].as_i64())
            .unwrap_or(0);
        Ok((
            sh.id.clone(),
            "Discard the current recording and delete its files".to_owned(),
            json!({
                "recording": sh.id,
                "title": meta.title,
                "recorded": clock_text(recorded),
                "this_cannot_be_undone": "The take is deleted and cannot be recovered",
            }),
            json!({ "recording": sh.id, "title": meta.title, "recorded_ms": recorded }),
        ))
    }

    /// Stops a running recording for good (app quit, daemon shutdown) and waits a bounded time.
    /// A countdown is cancelled.
    pub async fn shutdown(&self) {
        let _ = self.cancel_selftest(None).await;
        let Ok(sh) = self.active_for(None) else {
            return;
        };
        let (tx, rx) = oneshot::channel();
        if sh
            .tx
            .send(Cmd::Stop {
                reason: "shutdown".to_owned(),
                reply: tx,
            })
            .is_ok()
        {
            let _ = tokio::time::timeout(self.inner.opts.shutdown_timeout, rx).await;
        }
    }

    // ---- status --------------------------------------------------------------------------

    /// `get_recording_status`: the live numbers the HUD shows (see the module docs), or
    /// `{ state: "idle", last: {...} }`.
    pub async fn status(&self, id: Option<&str>) -> Result<Value, DaemonError> {
        let active = {
            let st = lock(&self.inner.state);
            match (&st.active, id) {
                (Some(a), Some(i)) if a.id != i => None,
                (Some(a), _) => Some(a.clone()),
                _ => None,
            }
        };
        if let Some(sh) = active {
            return Ok(status_value(&sh));
        }
        if let Some(s) = self.selftest_status() {
            let mine = id.is_none() || s["selftest_id"].as_str() == id;
            if mine {
                let mut v = s;
                // A self-test is a recording for the indicator's purposes, but it has no
                // recording id: clients tell it apart by `selftest`.
                return Ok(
                    json!({ "state": "selftest", "recording_id": null, "selftest": v.take(),
                                  "last": null }),
                );
            }
        }
        let st = lock(&self.inner.state);
        match (id, &st.last) {
            (Some(i), Some(l)) if l.id == i => {
                let mut v = l.value.clone();
                v["state"] = json!("stopped");
                Ok(v)
            }
            (Some(i), _) => Err(CmError::not_found(format!("Recording `{i}` is not known")).into()),
            (None, last) => Ok(json!({
                "state": "idle",
                "recording_id": null,
                "selftest": null,
                "last": last.as_ref().map(|l| l.value.clone()),
            })),
        }
    }
}
