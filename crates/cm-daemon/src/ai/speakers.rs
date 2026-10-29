//! Speaker labels (AI-8): the `diarize` job and the `set_speakers` action.
//!
//! The model is behind [`SpeakerService`]; until spike S7 picks one, production answers
//! `invalid_input` with `data.reason = "no_speaker_model"`. The rest (storing `diarization.json`
//! and the manifest's `speakers` / `speaker_segments`, rename, merge, reassign as one undoable
//! revision) is model-independent and tested with a scripted fake.

use cm_ai::speakers as spk;

use super::*;

/// `invalid_input` errors of the speaker model carry `data.reason`.
pub(super) fn speaker_error(e: CmError) -> DaemonError {
    match spk::error_data(&e) {
        Some(data) => DaemonError::WithData { error: e, data },
        None => DaemonError::Cm(e),
    }
}

/// Runs `diarize` jobs.
pub(super) struct DiarizeHandler {
    pub(super) library: Arc<std::sync::Mutex<cm_library::Library>>,
    pub(super) events: EventBus,
    pub(super) service: Arc<dyn SpeakerService>,
    pub(super) settings: Arc<dyn crate::engines::SettingsStore>,
}

fn count(inputs: &Value, key: &str) -> Option<u32> {
    inputs.get(key).and_then(Value::as_u64).map(|n| n as u32)
}

#[async_trait]
impl JobHandler for DiarizeHandler {
    async fn run(&self, ctx: JobContext) -> Result<Value, CmError> {
        let inputs = &ctx.job.inputs;
        let capture_id = inputs
            .get("capture_id")
            .and_then(Value::as_str)
            .ok_or_else(|| CmError::invalid_field("capture_id", "is required"))?
            .to_owned();
        let headset = inputs
            .get("headset")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let hints = spk::DiarizeHints {
            num_speakers: count(inputs, "num_speakers"),
            min_speakers: count(inputs, "min_speakers"),
            max_speakers: count(inputs, "max_speakers"),
        };
        let dir = {
            let (library, id) = (self.library.clone(), capture_id.clone());
            blocking(move || {
                let lib = lock_library(&library);
                let r = lib.get(&id)?;
                let dir = lib.bundle_path(&r);
                if !dir.is_dir() {
                    return Err(CmError::not_found(format!(
                        "The recording folder `{}` was moved or deleted",
                        dir.display()
                    )));
                }
                Ok(dir)
            })
            .await
            .map_err(into_cm)?
        };
        let display_name = spk::display_name_from(&self.settings.all().ok().unwrap_or_default());
        let created = time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default();
        let req = DiarizeRequest {
            capture_id: capture_id.clone(),
            bundle_dir: dir.clone(),
            hints,
            headset,
            display_name,
            created,
        };
        let (flag, watcher) = interrupt_flag(&ctx);
        let (service, reporter, run_flag) = (self.service.clone(), ctx.reporter.clone(), flag);
        let joined = tokio::task::spawn_blocking(move || {
            let mut last = -1.0_f32;
            service.diarize(&req, &mut |p| {
                if p >= 1.0 || p - last >= 0.01 {
                    last = p;
                    reporter.report(
                        f64::from(p),
                        Some(format!("Finding the speakers {}%", (p * 100.0) as u32)),
                    );
                }
                !run_flag.load(Ordering::SeqCst)
            })
        })
        .await;
        watcher.abort();
        let done = match joined {
            Ok(Ok(Some(d))) => d,
            Ok(Ok(None)) => return Err(CmError::internal("Finding the speakers was interrupted")),
            Ok(Err(e)) => return Err(e),
            Err(e) => return Err(CmError::internal(format!("speaker worker failed: {e}"))),
        };
        // Replace the stored result in one go, only now that the run finished. A new run is not
        // a user edit: no revision, and `set_extra` drops undo steps of the older run.
        let (library, write_dir, stored) = (self.library.clone(), dir, done.clone());
        blocking(move || {
            spk::write_diarization(&write_dir, &stored.file)?;
            let changes = stored
                .state()
                .manifest_values()
                .into_iter()
                .map(|(k, v)| (k, Some(v)))
                .collect();
            cm_library::rec_edit::set_extra(&write_dir, changes)?;
            lock_library(&library).index_dir(&write_dir)?;
            Ok(())
        })
        .await
        .map_err(into_cm)?;
        let result = json!({
            "capture_id": capture_id,
            "model": done.file.model,
            "headset": done.file.headset,
            "note": done.file.note,
            "speakers": done.speakers,
            "segments": done.file.segments,
        });
        self.events.publish(
            events::SPEAKERS_READY,
            json!({ "capture_id": capture_id, "model": done.file.model,
                    "speakers": done.speakers.len(), "segments": done.file.segments.len(),
                    "note": done.file.note, "headset": done.file.headset }),
        );
        Ok(result)
    }
}

fn into_cm(e: DaemonError) -> CmError {
    match e {
        DaemonError::Cm(c) | DaemonError::WithData { error: c, .. } => c,
        other => CmError::internal(other.to_string()),
    }
}

impl Inner {
    /// `diarize`: checks the recording and the model, then queues a `diarize` job.
    pub(super) async fn diarize(
        &self,
        actor: &Actor,
        capture_id: String,
        hints: spk::DiarizeHints,
        headset: bool,
    ) -> Result<Value, DaemonError> {
        let _serial = self.capture_locks.lock(&capture_id).await;
        let service = self.ai.speakers.clone();
        blocking(move || service.ready())
            .await
            .map_err(|e| match e {
                DaemonError::Cm(c) => speaker_error(c),
                other => other,
            })?;
        let queue = self.job_queue()?.queue.clone();
        if !queue.has_handler(JOB_DIARIZE) {
            return Err(DaemonError::NotImplemented(
                "speaker labels are not available: this daemon has no diarizer registered"
                    .to_owned(),
            ));
        }
        let (library, this, id) = (
            self.library.clone(),
            self.library_view(),
            capture_id.clone(),
        );
        blocking(move || {
            let lib = lock_library(&library);
            let rec = lib.get(&id)?;
            if !rec.kind.is_recording() {
                return Err(CmError::invalid_input(format!(
                    "`{id}` is a screenshot; only recordings have speakers"
                )));
            }
            let dir = lib.bundle_path(&rec);
            this.ensure_idle(&rec, &dir, "diarize")?;
            let manifest = cm_library::read_manifest(&dir)?;
            if !has_speech_track(&manifest) {
                return Err(CmError::invalid_input(format!(
                    "`{id}` has no microphone or system-audio track to tell speakers apart"
                )));
            }
            let mic = manifest
                .tracks
                .get("mic")
                .is_some_and(|t| t.get("missing") != Some(&json!(true)));
            if headset && !mic {
                return Err(CmError::invalid_input(format!(
                    "`{id}` has no microphone track, so headset mode cannot be used"
                )));
            }
            Ok(())
        })
        .await?;
        let by = actor.to_string();
        let (id, h) = (capture_id.clone(), hints);
        let (job, reused) =
            blocking(move || enqueue_diarize(&queue, &id, &h, headset, &by)).await?;
        Ok(json!({
            "job_id": job.id,
            "capture_id": capture_id,
            "state": job.state.as_str(),
            "reused": reused,
            "headset": headset,
        }))
    }

    /// The automatic speaker step for one transcript (see [`Daemon::start_ai_auto_speakers`]).
    /// Quietly does nothing unless everything is in place; `tried` remembers the transcript
    /// version (file size and time) a run was queued for. Answers whether a run is queued (or
    /// was already running) for the recording.
    pub(super) async fn auto_diarize(
        &self,
        capture_id: &str,
        tried: &mut std::collections::HashMap<String, String>,
    ) -> Result<bool, DaemonError> {
        let Some(rt) = &self.jobs else {
            return Ok(false);
        };
        if !rt.queue.has_handler(JOB_DIARIZE) {
            return Ok(false);
        }
        let settings = self.settings.all().unwrap_or_default();
        if !auto_speakers_enabled(&settings) {
            return Ok(false);
        }
        let service = self.ai.speakers.clone();
        if !service.available() || blocking(move || service.ready()).await.is_err() {
            return Ok(false);
        }
        let (library, id) = (self.library.clone(), capture_id.to_owned());
        let plan = blocking(move || {
            let lib = lock_library(&library);
            let rec = lib.get(&id)?;
            if !rec.kind.is_recording() {
                return Ok(None);
            }
            let dir = lib.bundle_path(&rec);
            // Still being recorded (or exported, uploaded ...): not now.
            if dir.join(cm_library::rec::LOCK_FILE).exists()
                || dir.join(cm_library::rec::OWNER_FILE).exists()
            {
                return Ok(None);
            }
            let manifest = cm_library::read_manifest(&dir)?;
            if !has_speech_track(&manifest) {
                return Ok(None);
            }
            // Any earlier result stays: the user may have renamed, merged or reassigned. A new
            // transcript is matched to the segments by time when it is read.
            if spk::SpeakerState::from_manifest(&manifest).is_some()
                || spk::diarization_path(&dir).exists()
            {
                return Ok(None);
            }
            let preset = manifest
                .settings
                .get("preset")
                .and_then(Value::as_str)
                .map(str::to_owned);
            let people = manifest
                .settings
                .get("people")
                .and_then(Value::as_u64)
                .and_then(|n| u32::try_from(n).ok());
            let count = auto_speaker_count(people, preset.as_deref());
            let version = std::fs::metadata(dir.join(TRANSCRIPT_FILE))
                .ok()
                .map(|m| {
                    let t = m
                        .modified()
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map_or(0, |d| d.as_nanos());
                    format!("{}:{t}", m.len())
                })
                .unwrap_or_default();
            Ok(Some((count, version)))
        })
        .await;
        let Ok(Some((count, version))) = plan else {
            return Ok(false);
        };
        if count <= 1 || tried.get(capture_id) == Some(&version) {
            return Ok(false);
        }
        let hints = spk::DiarizeHints {
            num_speakers: Some(count),
            ..spk::DiarizeHints::default()
        };
        // The same checks and the same job as the `diarize` action; a refusal is not reported.
        if let Ok(started) = self
            .diarize(&Actor::User, capture_id.to_owned(), hints, false)
            .await
        {
            tried.insert(capture_id.to_owned(), version);
            if started["reused"] != json!(true) {
                self.events.publish(
                    events::SPEAKERS_STARTED,
                    json!({ "capture_id": capture_id, "job_id": started["job_id"],
                            "num_speakers": count, "automatic": true }),
                );
            }
            return Ok(true);
        }
        Ok(false)
    }

    /// `set_speakers`: rename, merge and reassign as ONE revision (AI-8.AC4, AC5).
    pub(super) async fn set_speakers(
        &self,
        actor: &Actor,
        capture_id: String,
        edit: spk::SpeakerEdit,
    ) -> Result<Value, DaemonError> {
        let _serial = self.capture_locks.lock(&capture_id).await;
        let (library, this, now, by) = (
            self.library.clone(),
            self.library_view(),
            self.clock.now(),
            actor.clone(),
        );
        let id = capture_id.clone();
        let out = blocking(move || {
            let mut lib = lock_library(&library);
            let rec = lib.get(&id)?;
            if !rec.kind.is_recording() {
                return Err(CmError::invalid_input(format!(
                    "`{id}` is a screenshot; speakers belong to recordings"
                )));
            }
            let dir = lib.bundle_path(&rec);
            this.ensure_idle(&rec, &dir, "change the speakers of")?;
            let manifest = cm_library::read_manifest(&dir)?;
            let state = spk::SpeakerState::from_manifest(&manifest).ok_or_else(|| {
                CmError::invalid_input(format!("`{id}` has no speakers yet; run `diarize` first"))
            })?;
            let next = spk::apply_edit(&state, &edit)?;
            if next == state {
                let (can_undo, can_redo) = cm_library::rec_edit::history_flags(&manifest);
                return Ok(json!({
                    "capture_id": id, "changed": false, "revision": null,
                    "speakers": state.speakers, "can_undo": can_undo, "can_redo": can_redo,
                }));
            }
            let changes = next
                .manifest_values()
                .into_iter()
                .map(|(k, v)| (k, Some(v)))
                .collect();
            let out = cm_library::rec_edit::apply_extra(&dir, "set_speakers", changes, by, now)?;
            lib.index_dir(&dir)?;
            Ok(json!({
                "capture_id": id, "changed": true,
                "revision": out.revision.rev, "action": out.revision.action,
                "by": out.revision.by, "speakers": next.speakers,
                "can_undo": out.can_undo, "can_redo": out.can_redo,
            }))
        })
        .await?;
        if out["changed"] == json!(true) {
            self.events.publish(
                events::CAPTURE_REVISED,
                json!({ "capture_id": capture_id, "revision": out["revision"],
                        "action": out["action"], "by": out["by"] }),
            );
        }
        Ok(out)
    }
}

/// Queues a diarization unless an equal one is queued, running or paused.
fn enqueue_diarize(
    queue: &JobQueue,
    capture_id: &str,
    hints: &spk::DiarizeHints,
    headset: bool,
    by: &str,
) -> Result<(Job, bool), CmError> {
    static ENQUEUE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _one_at_a_time = ENQUEUE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let inputs = json!({
        "capture_id": capture_id, "headset": headset, "by": by,
        "num_speakers": hints.num_speakers, "min_speakers": hints.min_speakers,
        "max_speakers": hints.max_speakers,
    });
    let active = queue.list(&JobFilter {
        job_type: Some(JOB_DIARIZE.to_owned()),
        states: Some(vec![JobState::Queued, JobState::Running, JobState::Paused]),
        limit: Some(200),
    })?;
    let same = |j: &Job| {
        [
            "capture_id",
            "headset",
            "num_speakers",
            "min_speakers",
            "max_speakers",
        ]
        .iter()
        .all(|k| j.inputs.get(*k) == inputs.get(*k))
    };
    if let Some(j) = active.into_iter().find(same) {
        return Ok((j, true));
    }
    Ok((queue.enqueue(NewJob::new(JOB_DIARIZE, inputs))?, false))
}

/// The speaker data of a recording for `get_transcript` (AI-8, additive): each word gets
/// `speaker` (the id, and `overlap: true` when others spoke at once) and the result gets
/// `speakers` (`[{ id, name }]`). `None` when the recording was never diarized.
pub(super) fn annotate_transcript(
    transcript: &Transcript,
    state: &spk::SpeakerState,
) -> (Value, Value) {
    let labels = spk::assign_words(&transcript.words, &state.segments);
    let mut value = serde_json::to_value(transcript).unwrap_or(Value::Null);
    if let Some(words) = value.get_mut("words").and_then(Value::as_array_mut) {
        for (w, l) in words.iter_mut().zip(&labels) {
            if let (Some(o), Some(id)) = (w.as_object_mut(), l.speaker) {
                o.insert("speaker".into(), json!(id));
                if l.overlap {
                    o.insert("overlap".into(), json!(true));
                }
            }
        }
    }
    (value, state.speakers_json())
}
