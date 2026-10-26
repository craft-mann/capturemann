use super::*;

impl EngineInner {
    pub(super) fn publish(&self, sh: &Shared, name: &str, mut data: Value) {
        if let Some(o) = data.as_object_mut() {
            o.insert("recording_id".into(), json!(sh.id));
        }
        self.deps.events.publish(name, data);
    }

    /// Records the outcome but leaves `active` set: used while a restart swaps the slot.
    pub(super) fn conclude_keeping_slot(&self, sh: &Shared, last: Value) {
        lock(&self.state).last = Some(Last {
            id: sh.id.clone(),
            value: last,
        });
    }

    /// The slot of a restarted take is free again (the restart could not take it over).
    pub(super) fn release_replaced(&self, old: &str) {
        let mut st = lock(&self.state);
        if st.active.as_ref().is_some_and(|a| a.id == old) {
            st.active = None;
        }
    }

    pub(super) fn conclude(&self, sh: &Shared, last: Value) {
        let mut st = lock(&self.state);
        st.last = Some(Last {
            id: sh.id.clone(),
            value: last,
        });
        if st.active.as_ref().is_some_and(|a| a.id == sh.id) {
            st.active = None;
        }
    }

    /// A driver that died (panic or abort): finish the recording if it can still be finished.
    pub(super) async fn salvage(self: &Arc<Self>, sh: &Arc<Shared>, panicked: bool) {
        let still_active = lock(&self.state)
            .active
            .as_ref()
            .is_some_and(|a| a.id == sh.id);
        if !still_active {
            return;
        }
        self.publish(
            sh,
            events::RECORDING_WARNING,
            json!({ "code": "engine_failed", "level": "warning", "track": null,
                    "message": if panicked { "The recording engine failed; saving what was recorded" }
                               else { "The recording engine was stopped; saving what was recorded" } }),
        );
        let session = lock(&sh.session).take();
        let mut outcome = json!({ "recording_id": sh.id, "reason": "engine_failed" });
        if let Some(session) = session {
            let lib = self.deps.library.clone();
            let by = sh.by.clone();
            let layout = self.layout_at_stop(sh);
            let r = crate::daemon::blocking(move || {
                finalize_blocking(session, &lib, "engine_failed", &by, layout)
            })
            .await;
            match r {
                Ok(done) => {
                    outcome = outcome_json(sh, &done, "engine_failed");
                    self.publish_stopped(sh, &done, "engine_failed");
                    self.auto_export(sh, &done.record.id, &done.record.title);
                }
                Err(e) => {
                    self.publish(
                        sh,
                        events::RECORDING_STOPPED,
                        json!({ "capture_id": null, "duration_s": null, "tracks": [],
                                "reason": "error", "error": error_json(&e) }),
                    );
                }
            }
        }
        self.conclude(sh, outcome);
    }

    /// A stop that did not produce a capture: the terminal event, then back to Idle.
    pub(super) fn fail_stop(&self, sh: &Shared, e: &DaemonError) {
        let capture_id = lock(&sh.meta).capture_id.clone();
        // Stopped before the first frame: the empty bundle was deleted (R3-5); say that, not a
        // generic error.
        let reason = match e {
            DaemonError::Cm(c) if cm_encode::is_no_frames(c) => "no_frames",
            _ => "error",
        };
        self.publish(
            sh,
            events::RECORDING_STOPPED,
            json!({ "capture_id": capture_id, "duration_s": null, "tracks": [],
                    "reason": reason, "error": error_json(e) }),
        );
        self.conclude(
            sh,
            json!({ "recording_id": sh.id, "capture_id": capture_id, "state": "failed",
                    "reason": reason, "error": error_json(e) }),
        );
    }

    /// A stop that produced a capture: events, back to Idle, then the automatic export (after
    /// the job holds were released).
    pub(super) fn finish_stop(&self, sh: &Shared, done: &StopDone, reason: &str, outcome: &Value) {
        self.publish_stopped(sh, done, reason);
        self.conclude(sh, outcome.clone());
        self.auto_export(sh, &done.record.id, &done.record.title);
    }

    fn publish_stopped(&self, sh: &Shared, done: &StopDone, reason: &str) {
        let mut data = json!({
            "capture_id": done.record.id,
            "title": done.record.title,
            "duration_s": done.duration().as_secs_f64(),
            "tracks": done.tracks.iter().map(|t| kind_name(t.kind)).collect::<Vec<_>>(),
            "reason": reason,
        });
        if let Some((p, message)) = revoked_permission(reason, &done.warnings) {
            data["permission"] = json!(p.as_str());
            data["message"] = json!(message);
        }
        self.publish(sh, events::RECORDING_STOPPED, data);
        self.deps.events.publish(
            events::CAPTURE_CREATED,
            json!({ "capture_id": done.record.id, "by": sh.by }),
        );
    }
}

impl EngineInner {
    /// The camera layout to store in the manifest at stop (see [`crate::export::layout_at_stop`]).
    pub(super) fn layout_at_stop(&self, sh: &Shared) -> Option<Value> {
        let now = self.deps.settings.all().unwrap_or_default();
        crate::export::layout_at_stop(&sh.indicator, &sh.camera_at_start, &now)
    }

    /// REC-1 auto-export: enqueues the `export` job for a recording that just stopped normally,
    /// when `export.auto_on_stop` is on (default) and an exporter is registered. Never for a
    /// discard, a cancelled countdown or the 60 fps self-test (they do not pass through here).
    /// Failures are not the recording's problem: the take is saved either way.
    fn auto_export(&self, sh: &Shared, capture_id: &str, title: &str) {
        let export = self.queue_export(sh, capture_id, title, false);
        self.auto_upload(sh, capture_id, title, export);
    }

    /// STO-3 auto upload after a normal stop: the destinations set to `on_stop` get their
    /// `upload` job (after the export it needs, which is rendered here when the automatic export
    /// is off), and those set to `on_export` get one that waits for the export of this stop.
    /// `export` is what the auto-export came to. Failures are logged and never the recording's
    /// problem.
    fn auto_upload(
        &self,
        sh: &Shared,
        capture_id: &str,
        title: &str,
        export: Result<cm_jobs::Job, String>,
    ) {
        use crate::auto_upload::{enqueue, ExportState};
        use cm_storage::AutoUpload;
        let (Some(queue), Some(store)) = (&self.deps.jobs, &self.deps.destinations) else {
            return;
        };
        let Ok(list) = store.list() else {
            return;
        };
        let on_stop: Vec<_> = list
            .iter()
            .filter(|d| d.auto_upload == AutoUpload::OnStop)
            .collect();
        let mut export = export;
        if on_stop.iter().any(|d| d.what.wants_export()) && export.is_err() {
            // The automatic export is off or failed to queue: render the default export for the
            // upload anyway (it is the destination's choice, not the export setting's).
            export = self.queue_export(sh, capture_id, title, true);
        }
        let state = match &export {
            Ok(j) => ExportState::Job(j),
            Err(why) => ExportState::Unavailable(why.clone()),
        };
        for trigger in [AutoUpload::OnStop, AutoUpload::OnExport] {
            // `on_export` needs the export job itself; without one there is nothing to wait for.
            if trigger == AutoUpload::OnExport && export.is_err() {
                continue;
            }
            if let Err(e) = enqueue(
                queue,
                store,
                &self.deps.events,
                capture_id,
                title,
                &sh.by,
                trigger,
                &state,
            ) {
                log::warn!("automatic upload not queued: {e}");
            }
        }
    }

    /// Enqueues the default (`quick`) export of a recording that just stopped. `force` ignores
    /// the `export.auto_on_stop` setting (an upload needs the export). `Err` says why no job
    /// exists.
    fn queue_export(
        &self,
        sh: &Shared,
        capture_id: &str,
        title: &str,
        force: bool,
    ) -> Result<cm_jobs::Job, String> {
        let Some(queue) = &self.deps.jobs else {
            return Err("there is no job queue".to_owned());
        };
        let settings = self.deps.settings.all().unwrap_or_default();
        if !queue.has_handler(cm_jobs::JOB_EXPORT) {
            return Err("this daemon has no exporter".to_owned());
        }
        if !force && !crate::export::auto_export_enabled(&settings) {
            return Err("the automatic export is off".to_owned());
        }
        // R5-3: on a machine without FFmpeg the export can only fail, with an error toast after
        // every recording. Say so once and keep the take.
        let have_ffmpeg = match &self.opts.ffmpeg_probe {
            Some(p) => p(),
            None => cm_media::tools::locate().is_ok(),
        };
        if !have_ffmpeg {
            if !self.export_hint_sent.swap(true, Ordering::SeqCst) {
                self.deps.events.publish(
                    events::EXPORT_SKIPPED,
                    json!({ "capture_id": capture_id, "title": title,
                            "reason": "ffmpeg_not_found",
                            "message": cm_media::tools::NOT_FOUND_MESSAGE }),
                );
            }
            return Err("FFmpeg was not found".to_owned());
        }
        crate::export::enqueue_export(
            queue,
            &self.deps.events,
            capture_id,
            title,
            cm_media::PRESET_QUICK,
            None,
            crate::export::ExportOpts::default(),
            &sh.by,
            true,
        )
        .map(|(job, _)| job)
        .map_err(|e| e.to_string())
    }
}
