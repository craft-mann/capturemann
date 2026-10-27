//! The built-in text models (AI-7, provider `embedded`): list, download, select and remove the
//! GGUF files the bundled `llama-server` runs. The same flow as the speech models in
//! [`super::models`], on their own store and job type.

use super::*;

impl Inner {
    /// The text models, from the service, off the async threads.
    pub(super) async fn text_model_list(&self) -> Result<TextModelList, DaemonError> {
        let svc = self.ai.text_models.clone();
        blocking(move || svc.list()).await
    }

    /// The dialog text and details of a Confirm-tier text-model action, from the live list, so the
    /// dialog says exactly what will happen. Refuses a request that cannot run now.
    pub(super) async fn plan_text_model(
        &self,
        p: &AiParams,
    ) -> Result<(String, Value, Value), DaemonError> {
        let list = self.text_model_list().await?;
        let (model, download) = match &p.op {
            AiOp::DownloadTextModel { model } => (model, true),
            AiOp::RemoveTextModel { model } => (model, false),
            _ => return Ok((summary(p), p.to_json(), confirm_params(p))),
        };
        let info = list
            .models
            .iter()
            .find(|m| &m.info.id == model)
            .ok_or_else(|| unknown_text_model(model, &list))?;
        let info_ = &info.info;
        if download && info_.installed {
            return Err(
                CmError::invalid_field("model", format!("`{model}` is already installed")).into(),
            );
        }
        if !download && !info_.installed {
            return Err(CmError::not_found(format!("`{model}` is not installed")).into());
        }
        let mut details = Map::new();
        details.insert("model".into(), json!(info_.id));
        details.insert("name".into(), json!(info_.label));
        details.insert("size_mb".into(), json!(info_.size_mb));
        details.insert("license".into(), json!(info.license));
        if download {
            details.insert("destination".into(), json!(list.dir));
            details.insert(
                "network".into(),
                json!("Downloads the model from the internet"),
            );
            let params = json!({ "model": info_.id, "name": info_.label, "size_mb": info_.size_mb,
                                 "destination": list.dir });
            Ok((
                format!(
                    "Download the text model \"{}\" (about {} MB, {} licence) from the internet",
                    info_.label, info_.size_mb, info.license
                ),
                Value::Object(details),
                params,
            ))
        } else {
            if let Some(path) = &info_.path {
                details.insert("file".into(), json!(path));
            }
            let params = json!({ "model": info_.id, "name": info_.label, "size_mb": info_.size_mb,
                                 "file": info_.path });
            Ok((
                format!(
                    "Delete the text model \"{}\" ({} MB) from this computer",
                    info_.label, info_.size_mb
                ),
                Value::Object(details),
                params,
            ))
        }
    }

    pub(super) async fn download_text_model(
        &self,
        actor: &Actor,
        model: String,
    ) -> Result<Value, DaemonError> {
        let list = self.text_model_list().await?;
        match list.models.iter().find(|m| m.info.id == model) {
            None => return Err(unknown_text_model(&model, &list).into()),
            Some(m) if m.info.installed => {
                return Err(CmError::invalid_field(
                    "model",
                    format!("`{model}` is already installed"),
                )
                .into())
            }
            Some(_) => {}
        }
        let queue = self.job_queue()?.queue.clone();
        if !queue.has_handler(JOB_TEXT_MODEL_DOWNLOAD) {
            return Err(DaemonError::NotImplemented(
                "text model downloads are not available: this daemon has no model downloader registered"
                    .to_owned(),
            ));
        }
        let by = actor.to_string();
        let id = model.clone();
        let (job, reused) = blocking(move || {
            let active = queue.list(&JobFilter {
                job_type: Some(JOB_TEXT_MODEL_DOWNLOAD.to_owned()),
                states: Some(vec![JobState::Queued, JobState::Running, JobState::Paused]),
                limit: Some(50),
            })?;
            if let Some(j) = active
                .into_iter()
                .find(|j| j.inputs.get("model").and_then(Value::as_str) == Some(id.as_str()))
            {
                return Ok((j, true));
            }
            let job = queue.enqueue(NewJob::new(
                JOB_TEXT_MODEL_DOWNLOAD,
                json!({ "model": id, "by": by }),
            ))?;
            Ok((job, false))
        })
        .await?;
        Ok(
            json!({ "job_id": job.id, "model": model, "state": job.state.as_str(), "reused": reused }),
        )
    }

    pub(super) async fn select_text_model(&self, model: String) -> Result<Value, DaemonError> {
        let svc = self.ai.text_models.clone();
        let id = model.clone();
        blocking(move || {
            let installed = svc
                .list()?
                .models
                .iter()
                .any(|m| m.info.id == id && m.info.installed);
            if !installed {
                return Err(CmError::invalid_field(
                    "model",
                    format!("`{id}` is not installed; download it first"),
                ));
            }
            svc.select(&id)
        })
        .await?;
        // Use means use: the built-in engine is turned on for this model, the same as the
        // button on the AI page (the CLI and agents get the same behaviour).
        self.write_text_choice(Some(&model)).await?;
        self.events.publish(
            events::MODEL_CHANGED,
            json!({ "model": model, "change": "selected", "kind": "text" }),
        );
        Ok(json!({ "active": model, "provider": "embedded" }))
    }

    pub(super) async fn remove_text_model(&self, model: String) -> Result<Value, DaemonError> {
        // A model being downloaded, or being used by the running engine, cannot be pulled away.
        if let Some(rt) = &self.jobs {
            let q = rt.queue.clone();
            let id = model.clone();
            blocking(move || {
                let active = q.list(&JobFilter {
                    job_type: Some(JOB_TEXT_MODEL_DOWNLOAD.to_owned()),
                    states: Some(vec![JobState::Queued, JobState::Running, JobState::Paused]),
                    limit: Some(200),
                })?;
                if active
                    .iter()
                    .any(|j| j.inputs.get("model").and_then(Value::as_str) == Some(id.as_str()))
                {
                    return Err(CmError::busy(format!(
                        "The model `{id}` is being downloaded; wait for it to finish or cancel the job first"
                    )));
                }
                Ok(())
            })
            .await?;
        }
        let status = self.ai.text_engine.status();
        if status.model.as_deref() == Some(model.as_str()) {
            if status.in_flight > 0 {
                return Err(CmError::busy(format!(
                    "The text model `{model}` is being used by a running job; wait for it to finish"
                ))
                .into());
            }
            // Idle but loaded: unload it first so the file is not in use.
            self.ai.text_engine.stop();
        }
        // Removing the model that is in use turns the built-in engine off.
        let in_use = self
            .text_model_config()
            .is_ok_and(|c| c.provider == cm_ai::text_model::Provider::Embedded && c.model == model);
        let svc = self.ai.text_models.clone();
        let id = model.clone();
        blocking(move || svc.remove(&id)).await?;
        if in_use {
            self.write_text_choice(None).await?;
        }
        self.events.publish(
            events::MODEL_CHANGED,
            json!({ "model": model, "change": "removed", "kind": "text" }),
        );
        Ok(json!({ "model": model, "removed": true }))
    }
}

impl Inner {
    /// Writes the text-model choice: `Some(id)` is provider `embedded` with that model, `None`
    /// is provider `none` with no model. Settings are the app's own, so this does not go through
    /// the app-only check that refuses an agent's `set_settings`.
    async fn write_text_choice(&self, model: Option<&str>) -> Result<(), DaemonError> {
        let mut values = Map::new();
        values.insert(
            cm_ai::text_model::KEY_PROVIDER.to_owned(),
            json!(if model.is_some() { "embedded" } else { "none" }),
        );
        values.insert(
            cm_ai::text_model::KEY_EMBEDDED_MODEL.to_owned(),
            json!(model.unwrap_or("")),
        );
        self.settings.set(&values)?;
        Ok(())
    }
}

fn unknown_text_model(model: &str, list: &TextModelList) -> CmError {
    CmError::invalid_field(
        "model",
        format!(
            "unknown text model `{model}`; use one of: {}",
            if list.models.is_empty() {
                "none (no text model is available in this build yet)".to_owned()
            } else {
                list.models
                    .iter()
                    .map(|m| m.info.id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        ),
    )
}
