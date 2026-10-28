use super::*;

// ---- the daemon side -----------------------------------------------------------------------

impl Daemon {
    /// Registers the executors of the `model_download` and `transcribe` job types with the
    /// engines of [`crate::DaemonBuilder::ai`]. Call it next to
    /// [`Daemon::register_export_handler`], before [`Daemon::start_jobs`], in every process that
    /// owns a daemon. Errors with `not_implemented` without a job queue.
    pub fn register_ai_handlers(&self) -> Result<(), DaemonError> {
        let inner = &self.inner;
        let speech = inner.ai.models.clone();
        self.register_job_handler(
            JOB_MODEL_DOWNLOAD,
            Arc::new(ModelDownloadHandler {
                events: inner.events.clone(),
                download: Arc::new(move |m, p| speech.download(m, p)),
                text: false,
            }),
        )?;
        let text_models = inner.ai.text_models.clone();
        self.register_job_handler(
            JOB_TEXT_MODEL_DOWNLOAD,
            Arc::new(ModelDownloadHandler {
                events: inner.events.clone(),
                download: Arc::new(move |m, p| text_models.download(m, p)),
                text: true,
            }),
        )?;
        self.register_job_handler(
            JOB_TRANSCRIBE,
            Arc::new(TranscribeHandler {
                library: inner.library.clone(),
                events: inner.events.clone(),
                service: inner.ai.transcribe.clone(),
                captions: inner.ai.captions.clone(),
                settings: inner.settings.clone(),
            }),
        )?;
        self.register_job_handler(
            JOB_TRANSLATE,
            Arc::new(TranslateHandler {
                library: inner.library.clone(),
                events: inner.events.clone(),
                service: inner.ai.translate.clone(),
                captions: inner.ai.captions.clone(),
                settings: inner.settings.clone(),
                inner: Arc::downgrade(&self.inner),
            }),
        )?;
        self.register_job_handler(
            JOB_DIARIZE,
            Arc::new(speakers::DiarizeHandler {
                library: inner.library.clone(),
                events: inner.events.clone(),
                service: inner.ai.speakers.clone(),
                settings: inner.settings.clone(),
            }),
        )?;
        self.register_job_handler(
            JOB_REPORT,
            Arc::new(reports::ReportHandler {
                inner: Arc::downgrade(&self.inner),
            }),
        )
    }

    /// Starts the automatic transcription after a recording (setting `ai.auto_transcribe`).
    /// When a recording stops normally the daemon enqueues a `transcribe` job, but only if the
    /// setting is on (default: on with 16 GB of RAM or more), a speech model is installed, a
    /// transcriber is registered and the recording has a microphone or system-audio track.
    /// Without an installed model nothing happens, silently: the app shows its gentle notice
    /// instead, and nothing is downloaded. Needs a tokio runtime; the task ends with the daemon.
    pub fn start_ai_auto_transcribe(&self) -> tokio::task::JoinHandle<()> {
        let mut rx = self.subscribe();
        let weak = Arc::downgrade(&self.inner);
        tokio::spawn(async move {
            while let Ok(ev) = rx.recv().await {
                if ev.name != events::RECORDING_STOPPED {
                    continue;
                }
                let Some(capture_id) = ev.data.get("capture_id").and_then(Value::as_str) else {
                    continue;
                };
                let Some(inner) = weak.upgrade() else { break };
                let capture_id = capture_id.to_owned();
                let _ = inner
                    .auto_transcribe(&capture_id, ev.data.get("tracks"))
                    .await;
            }
        })
    }

    /// Starts the automatic speaker step after a transcript (setting `ai.auto_speakers`, on by
    /// default, AI-8). On every `transcript.ready` (manual or automatic) it queues the same
    /// `diarize` job the action does, as the user, with the count from the recording's own
    /// `people` answer or its preset. It does nothing, silently, when the setting is off, no
    /// speaker model is usable, the recording is still being recorded, has no microphone or
    /// system track, was already diarized (an existing result, edited or not, is never
    /// overwritten; a regenerated transcript is matched to it by time at read) or the count is
    /// one person (a voice note). One transcript starts at most one run. Needs a tokio runtime.
    pub fn start_ai_auto_speakers(&self) -> tokio::task::JoinHandle<()> {
        let mut rx = self.subscribe();
        let weak = Arc::downgrade(&self.inner);
        tokio::spawn(async move {
            let mut tried: std::collections::HashMap<String, String> = Default::default();
            while let Ok(ev) = rx.recv().await {
                if ev.name != events::TRANSCRIPT_READY {
                    continue;
                }
                let Some(capture_id) = ev.data.get("capture_id").and_then(Value::as_str) else {
                    continue;
                };
                let Some(inner) = weak.upgrade() else { break };
                let capture_id = capture_id.to_owned();
                let _ = inner.auto_diarize(&capture_id, &mut tried).await;
            }
        })
    }
}

impl Inner {
    /// The installed models, from the service, off the async threads.
    pub(super) async fn model_list(&self) -> Result<ModelList, DaemonError> {
        let svc = self.ai.models.clone();
        blocking(move || svc.list()).await
    }

    /// The model a transcription uses: the one asked for, else the active one, else the only
    /// or first installed one. `no_model` when nothing is installed.
    pub(super) fn pick_model(
        list: &ModelList,
        wanted: Option<&str>,
    ) -> Result<String, DaemonError> {
        let installed: Vec<&ModelInfo> = list.models.iter().filter(|m| m.installed).collect();
        let ids = || installed.iter().map(|m| m.id.clone()).collect::<Vec<_>>();
        if let Some(w) = wanted {
            return if installed.iter().any(|m| m.id == w) {
                Ok(w.to_owned())
            } else if installed.is_empty() {
                Err(no_model_error())
            } else {
                Err(model_not_installed(w, &ids()))
            };
        }
        installed
            .iter()
            .find(|m| m.active)
            .or_else(|| installed.first())
            .map(|m| m.id.clone())
            .ok_or_else(no_model_error)
    }

    /// The dialog text and details of a Confirm-tier AI action, from the live model list, so the
    /// dialog says exactly what will happen (AGT-2). Refuses a request that cannot run now.
    pub(crate) async fn plan_ai(
        &self,
        p: &AiParams,
    ) -> Result<(String, Value, Value), DaemonError> {
        if matches!(
            p.op,
            AiOp::DownloadTextModel { .. } | AiOp::RemoveTextModel { .. }
        ) {
            return self.plan_text_model(p).await;
        }
        let list = self.model_list().await?;
        let (model, download) = match &p.op {
            AiOp::DownloadModel { model } => (model, true),
            AiOp::RemoveModel { model } => (model, false),
            _ => return Ok((summary(p), p.to_json(), confirm_params(p))),
        };
        let info = list.models.iter().find(|m| &m.id == model).ok_or_else(|| {
            CmError::invalid_field(
                "model",
                format!(
                    "unknown model `{model}`; use one of: {}",
                    list.models
                        .iter()
                        .map(|m| m.id.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            )
        })?;
        if download && info.installed {
            return Err(
                CmError::invalid_field("model", format!("`{model}` is already installed")).into(),
            );
        }
        if !download && !info.installed {
            return Err(CmError::not_found(format!("`{model}` is not installed")).into());
        }
        if download && info.import_only {
            return Err(import_only_error(&info.id).into());
        }
        let kind = if info.kind == cm_ai::model::ModelKind::Speaker {
            "speaker"
        } else {
            "speech"
        };
        let mut details = Map::new();
        details.insert("model".into(), json!(info.id));
        details.insert("name".into(), json!(info.label));
        details.insert("size_mb".into(), json!(info.size_mb));
        if !info.license.is_empty() {
            details.insert("license".into(), json!(info.license));
        }
        if download {
            details.insert("destination".into(), json!(list.dir));
            details.insert(
                "network".into(),
                json!("Downloads the model from the internet"),
            );
            let params = json!({ "model": info.id, "name": info.label, "size_mb": info.size_mb,
                                 "destination": list.dir });
            Ok((
                format!(
                    "Download the {kind} model \"{}\" (about {} MB) from the internet",
                    info.label, info.size_mb
                ),
                Value::Object(details),
                params,
            ))
        } else {
            if let Some(path) = &info.path {
                details.insert("file".into(), json!(path));
            }
            let params = json!({ "model": info.id, "name": info.label, "size_mb": info.size_mb,
                                 "file": info.path });
            Ok((
                format!(
                    "Delete the {kind} model \"{}\" ({} MB) from this computer",
                    info.label, info.size_mb
                ),
                Value::Object(details),
                params,
            ))
        }
    }

    /// Runs one AI action.
    pub(crate) async fn ai_op(&self, actor: &Actor, p: AiParams) -> Result<Value, DaemonError> {
        match p.op {
            AiOp::ListModels => Ok(serde_json::to_value(self.model_list().await?)
                .map_err(|e| DaemonError::Internal(e.to_string()))?),
            AiOp::DownloadModel { model } => self.download_model(actor, model).await,
            AiOp::ImportModel { model, path } => {
                let svc = self.ai.models.clone();
                let id = model.clone();
                let file = PathBuf::from(&path);
                blocking(move || {
                    if !file.is_file() {
                        return Err(CmError::invalid_field(
                            "path",
                            format!("`{}` is not a file", file.display()),
                        ));
                    }
                    svc.import(&id, &file)
                })
                .await?;
                self.events.publish(
                    events::MODEL_CHANGED,
                    json!({ "model": model, "change": "imported" }),
                );
                Ok(json!({ "model": model, "installed": true }))
            }
            AiOp::SelectModel { model } => {
                let svc = self.ai.models.clone();
                let id = model.clone();
                blocking(move || {
                    let installed = svc.list()?.models.iter().any(|m| m.id == id && m.installed);
                    if !installed {
                        return Err(CmError::invalid_field(
                            "model",
                            format!("`{id}` is not installed; download or import it first"),
                        ));
                    }
                    svc.select(&id)
                })
                .await?;
                self.events.publish(
                    events::MODEL_CHANGED,
                    json!({ "model": model, "change": "selected" }),
                );
                Ok(json!({ "active": model }))
            }
            AiOp::RemoveModel { model } => self.remove_model(model).await,
            AiOp::ListTextModels => Ok(serde_json::to_value(self.text_model_list().await?)
                .map_err(|e| DaemonError::Internal(e.to_string()))?),
            AiOp::DownloadTextModel { model } => self.download_text_model(actor, model).await,
            AiOp::SelectTextModel { model } => self.select_text_model(model).await,
            AiOp::RemoveTextModel { model } => self.remove_text_model(model).await,
            AiOp::Transcribe {
                capture_id,
                language,
                model,
                original_audio,
            } => {
                self.transcribe(actor, capture_id, language, model, original_audio)
                    .await
            }
            AiOp::GetTranscript { capture_id } => self.get_transcript(&capture_id).await,
            AiOp::GetCaptions {
                capture_id,
                language,
            } => self.get_captions_in(&capture_id, language.as_deref()).await,
            AiOp::SetCaptions {
                capture_id,
                lines,
                language: None,
            } => self.set_captions(actor, capture_id, lines).await,
            AiOp::SetCaptions {
                capture_id,
                lines,
                language: Some(language),
            } => {
                self.set_translation_lines(actor, capture_id, language, lines)
                    .await
            }
            AiOp::ReplaceInCaptions {
                capture_id,
                replacements,
                language: None,
            } => {
                self.replace_in_captions(actor, capture_id, replacements)
                    .await
            }
            AiOp::ReplaceInCaptions {
                capture_id,
                replacements,
                language: Some(language),
            } => {
                self.replace_in_translation(actor, capture_id, language, replacements)
                    .await
            }
            AiOp::ExportCaptions {
                capture_id,
                format,
                language,
                speaker_names,
            } => {
                self.export_captions_in(&capture_id, format, language.as_deref(), speaker_names)
                    .await
            }
            AiOp::TranslateCaptions {
                capture_id,
                target_language,
                engine,
            } => {
                self.translate_captions(actor, capture_id, target_language, engine)
                    .await
            }
            AiOp::Diarize {
                capture_id,
                hints,
                headset,
            } => self.diarize(actor, capture_id, hints, headset).await,
            AiOp::SetSpeakers { capture_id, edit } => {
                self.set_speakers(actor, capture_id, edit).await
            }
            AiOp::ListReportTemplates => Ok(reports::list_templates()),
            AiOp::GenerateReport {
                capture_id,
                template,
                language,
                context,
                report_id,
            } => {
                self.generate_report(actor, capture_id, template, language, context, report_id)
                    .await
            }
            AiOp::GetReport {
                capture_id,
                report_id,
            } => self.get_report(capture_id, report_id).await,
            AiOp::ExportReport {
                capture_id,
                report_id,
                format,
                out_dir,
                include_times,
            } => {
                self.export_report(capture_id, report_id, format, out_dir, include_times)
                    .await
            }
            AiOp::DeleteReport {
                capture_id,
                report_id,
            } => self.delete_report(actor, capture_id, report_id).await,
            AiOp::EditReport {
                capture_id,
                report_id,
                markdown,
            } => {
                self.edit_report(actor, capture_id, report_id, markdown)
                    .await
            }
            AiOp::CheckReportItem {
                capture_id,
                report_id,
                section,
                index,
                checked,
            } => {
                self.check_report_item(actor, capture_id, report_id, section, index, checked)
                    .await
            }
            AiOp::SuggestZooms { capture_id } => self.suggest(&capture_id, true).await,
            AiOp::SuggestCuts { capture_id } => self.suggest(&capture_id, false).await,
        }
    }
}

/// `download_model` for a file that is only imported (a speaker model whose archive the app does
/// not unpack): `invalid_input` that names `import_model`.
pub(crate) fn import_only_error(model: &str) -> CmError {
    CmError::invalid_field(
        "model",
        format!(
            "`{model}` cannot be downloaded in this version: import the file with `import_model` \
             (list_models says where it comes from)"
        ),
    )
}
