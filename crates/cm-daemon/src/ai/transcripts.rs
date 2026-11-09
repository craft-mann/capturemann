use super::*;

impl Inner {
    pub(super) async fn transcribe(
        &self,
        actor: &Actor,
        capture_id: String,
        language: Option<String>,
        model: Option<String>,
        original_audio: Option<bool>,
    ) -> Result<Value, DaemonError> {
        let _serial = self.capture_locks.lock(&capture_id).await;
        let list = self.model_list().await?;
        let model = Self::pick_model(&list, model.as_deref())?;
        let queue = self.job_queue()?.queue.clone();
        if !queue.has_handler(JOB_TRANSCRIBE) {
            return Err(DaemonError::NotImplemented(
                "transcription is not available: this daemon has no transcriber registered"
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
                    "`{id}` is a screenshot; only recordings have speech to transcribe"
                )));
            }
            let dir = lib.bundle_path(&rec);
            this.ensure_idle(&rec, &dir, "transcribe")?;
            if !has_speech_track(&cm_library::read_manifest(&dir)?) {
                return Err(CmError::invalid_input(format!(
                    "`{id}` has no microphone or system-audio track to transcribe"
                )));
            }
            Ok(())
        })
        .await?;
        let by = actor.to_string();
        // Not named by the request: the setting (the recorded microphone unless it is turned off).
        let original_audio = original_audio
            .unwrap_or_else(|| settings_raw_mic(&self.settings.all().unwrap_or_default()));
        let (id, m, l) = (capture_id.clone(), model.clone(), language.clone());
        let (job, reused) = blocking(move || {
            enqueue_transcribe(&queue, &id, &m, l.as_deref(), &by, false, original_audio)
        })
        .await?;
        Ok(json!({
            "job_id": job.id,
            "capture_id": capture_id,
            "model": model,
            "language": language,
            "original_audio": original_audio,
            "state": job.state.as_str(),
            "reused": reused,
        }))
    }

    /// The automatic transcription of a recording that just stopped (see
    /// [`Daemon::start_ai_auto_transcribe`]). Quietly does nothing unless everything is in place.
    pub(super) async fn auto_transcribe(
        &self,
        capture_id: &str,
        tracks: Option<&Value>,
    ) -> Result<(), DaemonError> {
        let Some(rt) = &self.jobs else { return Ok(()) };
        let speech = tracks.and_then(Value::as_array).is_some_and(|t| {
            t.iter()
                .any(|k| matches!(k.as_str(), Some("mic" | "system")))
        });
        if !speech || !rt.queue.has_handler(JOB_TRANSCRIBE) {
            return Ok(());
        }
        let settings = self.settings.all().unwrap_or_default();
        if !auto_transcribe_enabled(&settings, system_ram_gb()) {
            return Ok(());
        }
        // No model, no job, no notice here: the AI page says so, and nothing is downloaded.
        let Ok(list) = self.model_list().await else {
            return Ok(());
        };
        let Ok(model) = Self::pick_model(&list, None) else {
            return Ok(());
        };
        let raw = settings_raw_mic(&settings);
        let (queue, id, picked) = (rt.queue.clone(), capture_id.to_owned(), model.clone());
        let (job, reused) =
            blocking(move || enqueue_transcribe(&queue, &id, &picked, None, "user", true, raw))
                .await?;
        if !reused {
            self.events.publish(
                events::TRANSCRIPT_STARTED,
                json!({ "capture_id": capture_id, "job_id": job.id, "model": model,
                        "automatic": true }),
            );
        }
        Ok(())
    }

    pub(super) async fn get_transcript(&self, capture_id: &str) -> Result<Value, DaemonError> {
        let (library, id, captions) = (
            self.library.clone(),
            capture_id.to_owned(),
            self.ai.captions.clone(),
        );
        blocking(move || {
            let (dir, manifest) = recording_bundle(&library, &id)?;
            let path = dir.join(TRANSCRIPT_FILE);
            let text = std::fs::read_to_string(&path).map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    CmError::not_found(format!(
                        "`{id}` has no transcript yet; run `transcribe` first"
                    ))
                } else {
                    CmError::internal(format!("cannot read `{}`: {e}", path.display()))
                }
            })?;
            let transcript: Transcript = serde_json::from_str(&text).map_err(|e| {
                CmError::internal(format!("`{}` is not a transcript: {e}", path.display()))
            })?;
            // When the transcript was written (Unix seconds), for the details page.
            let made_at = std::fs::metadata(&path)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs());
            let stored = cm_library::rec_edit::captions_of(&manifest).cloned();
            let (lines, source) = match stored {
                Some(l) => (l, Some("edited")),
                None => match captions.lines(&transcript) {
                    Ok(l) => (json!(l), Some("generated")),
                    Err(_) => (Value::Null, None),
                },
            };
            let options = cm_library::rec_edit::caption_options(&manifest);
            // AI-8 (additive): `speaker` on each word and `speakers` in the result, once the
            // recording has been diarized.
            let mut transcript_json = json!(transcript);
            let mut speakers = None;
            if let Some(state) = cm_ai::speakers::SpeakerState::from_manifest(&manifest) {
                let (t, s) = super::speakers::annotate_transcript(&transcript, &state);
                transcript_json = t;
                speakers = Some(s);
            }
            let mut out = json!({
                "capture_id": id,
                "transcript": transcript_json,
                "made_at": made_at,
                "captions": lines,
                "captions_source": source,
                "caption_options": {"enabled": options.enabled, "burn_in": options.burn_in},
            });
            if let (Some(o), Some(s)) = (out.as_object_mut(), speakers) {
                o.insert("speakers".into(), s);
            }
            Ok(out)
        })
        .await
    }
}

/// Upgrades the `[noise]` sounds of the bundle's transcript to `[unclear]` wherever the speaker
/// engine heard speech (M2i, `cm_ai::sounds::classify`). Safe to call at any time and from either
/// order of the transcript and the speaker run: it does nothing without both. The transcript file
/// is replaced in one rename only when something changed.
pub(super) fn upgrade_sounds(dir: &std::path::Path) {
    let Ok(text) = std::fs::read_to_string(dir.join(TRANSCRIPT_FILE)) else {
        return;
    };
    let Ok(mut transcript) = serde_json::from_str::<Transcript>(&text) else {
        return;
    };
    if transcript.sounds.is_empty() {
        return;
    }
    let Ok(manifest) = cm_library::read_manifest(dir) else {
        return;
    };
    let Some(state) = cm_ai::speakers::SpeakerState::from_manifest(&manifest) else {
        return;
    };
    let evidence: Vec<cm_core::edit::Span> = state
        .segments
        .iter()
        .map(|s| cm_core::edit::Span::new(s.start, s.end))
        .collect();
    let upgraded = cm_ai::sounds::classify(&transcript.sounds, &evidence);
    if upgraded != transcript.sounds {
        transcript.sounds = upgraded;
        if let Ok(bytes) = serde_json::to_vec_pretty(&transcript) {
            let _ = super::jobs::write_atomic(dir, TRANSCRIPT_FILE, &bytes);
        }
    }
}

/// The setting `ai.transcribe_raw_mic` (default on: the original microphone; off: the
/// noise-reduced copy) from a settings map. Read when a transcription starts, so a library with
/// no stored value follows the default and a stored value, on or off, is kept.
fn settings_raw_mic(settings: &serde_json::Map<String, Value>) -> bool {
    cm_ai::pipeline::PipelineSettings::from_settings(settings).transcribe_raw_mic
}
