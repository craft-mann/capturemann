use super::*;

impl Inner {
    pub(super) async fn download_model(
        &self,
        actor: &Actor,
        model: String,
    ) -> Result<Value, DaemonError> {
        let list = self.model_list().await?;
        match list.models.iter().find(|m| m.id == model) {
            None => {
                return Err(
                    CmError::invalid_field("model", format!("unknown model `{model}`")).into(),
                )
            }
            Some(m) if m.installed => {
                return Err(CmError::invalid_field(
                    "model",
                    format!("`{model}` is already installed"),
                )
                .into())
            }
            Some(m) if m.import_only => {
                return Err(super::dispatch::import_only_error(&model).into());
            }
            Some(_) => {}
        }
        let queue = self.job_queue()?.queue.clone();
        if !queue.has_handler(JOB_MODEL_DOWNLOAD) {
            return Err(DaemonError::NotImplemented(
                "model downloads are not available: this daemon has no model downloader registered"
                    .to_owned(),
            ));
        }
        let by = actor.to_string();
        let id = model.clone();
        let (job, reused) = blocking(move || {
            let active = queue.list(&JobFilter {
                job_type: Some(JOB_MODEL_DOWNLOAD.to_owned()),
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
                JOB_MODEL_DOWNLOAD,
                json!({ "model": id, "by": by }),
            ))?;
            Ok((job, false))
        })
        .await?;
        Ok(
            json!({ "job_id": job.id, "model": model, "state": job.state.as_str(), "reused": reused }),
        )
    }

    pub(super) async fn remove_model(&self, model: String) -> Result<Value, DaemonError> {
        // A model in use cannot be pulled away: a queued or running transcription with it, or
        // its own download, is `busy`.
        if let Some(rt) = &self.jobs {
            let q = rt.queue.clone();
            let id = model.clone();
            blocking(move || {
                for (ty, what) in [
                    (JOB_TRANSCRIBE, "is being used by a transcription"),
                    (JOB_MODEL_DOWNLOAD, "is being downloaded"),
                ] {
                    let active = q.list(&JobFilter {
                        job_type: Some(ty.to_owned()),
                        states: Some(vec![JobState::Queued, JobState::Running, JobState::Paused]),
                        limit: Some(200),
                    })?;
                    if active
                        .iter()
                        .any(|j| j.inputs.get("model").and_then(Value::as_str) == Some(id.as_str()))
                    {
                        return Err(CmError::busy(format!(
                            "The model `{id}` {what}; wait for it to finish or cancel the job first"
                        )));
                    }
                }
                Ok(())
            })
            .await?;
        }
        let svc = self.ai.models.clone();
        let id = model.clone();
        blocking(move || svc.remove(&id)).await?;
        self.events.publish(
            events::MODEL_CHANGED,
            json!({ "model": model, "change": "removed" }),
        );
        Ok(json!({ "model": model, "removed": true }))
    }
}
