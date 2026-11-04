//! Transcript normalisation (AI-10): the `normalize_transcript` action and its `normalize` job,
//! and `get_normalized_transcript`.
//!
//! The built-in text model adds punctuation, capital letters and paragraph breaks to the words
//! of `transcript.json`; an answer that changes a word is rejected ([`cm_ai::normalize`]). The
//! result is the sidecar `transcript.normalized.json`; the transcript, its undo history, search,
//! reports and translations are untouched. The action is Free (it runs the local model and
//! writes a derived file) and can be set to Ask first like any mutating Free tool. With provider
//! `none` it fails with `invalid_input` and `data.reason = "no_text_model"`. An interrupted job
//! goes on from its checkpoint (the results of the batches already done).

use std::sync::Weak;

use cm_ai::normalize::{self as norm, Normalized, TurnResult};
use cm_ai::speakers as spk;

use super::*;
use crate::text_model_ops::setup_error;

/// Job type of a normalisation run.
pub const JOB_NORMALIZE: &str = norm::JOB_NORMALIZE;

fn now_rfc3339() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

fn into_cm(e: DaemonError) -> CmError {
    match e {
        DaemonError::Cm(c) | DaemonError::WithData { error: c, .. } => c,
        other => CmError::internal(other.to_string()),
    }
}

/// The transcript of a bundle.
fn read_transcript(dir: &Path, id: &str) -> Result<Transcript, CmError> {
    let path = dir.join(TRANSCRIPT_FILE);
    let text = std::fs::read_to_string(&path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            CmError::not_found(format!(
                "`{id}` has no transcript to tidy; run `transcribe` first"
            ))
        } else {
            CmError::internal(format!("cannot read `{}`: {e}", path.display()))
        }
    })?;
    serde_json::from_str(&text)
        .map_err(|e| CmError::internal(format!("`{}` is not a transcript: {e}", path.display())))
}

/// One speaker label per word, when the recording was diarized.
fn speaker_labels(dir: &Path, transcript: &Transcript) -> Option<Vec<Option<u32>>> {
    let manifest = cm_library::read_manifest(dir).ok()?;
    let state = spk::SpeakerState::from_manifest(&manifest)?;
    if state.segments.is_empty() {
        return None;
    }
    Some(
        spk::assign_words(&transcript.words, &state.segments)
            .iter()
            .map(|l| l.speaker)
            .collect(),
    )
}

impl Inner {
    /// Provider `none` answers `no_text_model`.
    fn normalize_gate(&self) -> Result<cm_ai::text_model::TextModelConfig, DaemonError> {
        let config = self.text_model_config()?;
        if config.provider == cm_ai::text_model::Provider::None {
            return Err(setup_error(
                cm_ai::text_model::TextModelError::no_text_model(),
            ));
        }
        self.check_text_model(&config)?;
        Ok(config)
    }

    /// `normalize_transcript`: checks what can be checked now, then queues the job.
    pub(super) async fn normalize_transcript(
        &self,
        _actor: &Actor,
        capture_id: String,
        language: Option<String>,
    ) -> Result<Value, DaemonError> {
        let _serial = self.capture_locks.lock(&capture_id).await;
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
                    "`{id}` is a screenshot; this works on recordings"
                )));
            }
            let dir = lib.bundle_path(&rec);
            this.ensure_idle(&rec, &dir, "tidy the transcript of")?;
            let t = read_transcript(&dir, &id)?;
            if t.words.is_empty() {
                return Err(CmError::invalid_input(format!(
                    "`{id}` has no words to tidy"
                )));
            }
            Ok(())
        })
        .await?;
        self.normalize_gate()?;
        let queue = self.job_queue()?.queue.clone();
        if !queue.has_handler(JOB_NORMALIZE) {
            return Err(DaemonError::NotImplemented(
                "normalising is not available: this daemon has no runner registered".to_owned(),
            ));
        }
        let inputs = json!({ "capture_id": capture_id, "language": language });
        let id = capture_id.clone();
        let (job, reused) = blocking(move || enqueue(&queue, &id, inputs)).await?;
        Ok(json!({
            "job_id": job.id, "capture_id": capture_id,
            "state": job.state.as_str(), "reused": reused,
        }))
    }

    /// `get_normalized_transcript`: the sidecar, its text, and whether the transcript changed
    /// since (`stale`).
    pub(super) async fn get_normalized_transcript(
        &self,
        capture_id: String,
    ) -> Result<Value, DaemonError> {
        let library = self.library.clone();
        blocking(move || {
            let (dir, _) = recording_bundle(&library, &capture_id)?;
            let n = norm::read(&dir)?.ok_or_else(|| {
                CmError::not_found(format!(
                    "`{capture_id}` has no normalised transcript; run `normalize_transcript` first"
                ))
            })?;
            let stale = read_transcript(&dir, &capture_id)
                .map(|t| !n.matches(&t.words))
                .unwrap_or(true);
            Ok(json!({
                "capture_id": capture_id, "stale": stale,
                "text": n.text(), "normalized": n,
            }))
        })
        .await
    }
}

/// Queues a run unless one for the capture is queued, running or paused (then that job is
/// returned; `true` = reused).
fn enqueue(queue: &JobQueue, capture_id: &str, inputs: Value) -> Result<(Job, bool), CmError> {
    static ENQUEUE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _one_at_a_time = ENQUEUE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let active = queue.list(&JobFilter {
        job_type: Some(JOB_NORMALIZE.to_owned()),
        states: Some(vec![JobState::Queued, JobState::Running, JobState::Paused]),
        limit: Some(200),
    })?;
    if let Some(j) = active
        .into_iter()
        .find(|j| j.inputs.get("capture_id").and_then(Value::as_str) == Some(capture_id))
    {
        return Ok((j, true));
    }
    Ok((queue.enqueue(NewJob::new(JOB_NORMALIZE, inputs))?, false))
}

/// Runs `normalize` jobs.
pub(super) struct NormalizeHandler {
    pub(super) inner: Weak<Inner>,
}

#[async_trait]
impl JobHandler for NormalizeHandler {
    async fn run(&self, ctx: JobContext) -> Result<Value, CmError> {
        let inner = self
            .inner
            .upgrade()
            .ok_or_else(|| CmError::internal("the daemon is shutting down"))?;
        let capture_id = ctx
            .job
            .inputs
            .get("capture_id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        match inner.run_normalize(&ctx, &capture_id).await {
            Ok(v) => Ok(v),
            Err(e) => {
                let reason = match &e {
                    CmError::JobFailed { reason, .. } => reason.clone(),
                    other => other.code().as_str().to_owned(),
                };
                inner.events.publish(
                    events::NORMALIZE_FAILED,
                    json!({ "capture_id": capture_id, "reason": reason,
                            "message": e.to_string() }),
                );
                Err(e)
            }
        }
    }
}

/// The saved progress of a run: the results so far, valid only for the same words, model and
/// prompt.
fn checkpoint_value(hash: &str, model: &str, results: &[TurnResult]) -> Value {
    json!({ "words_hash": hash, "model": model, "prompt_version": norm::prompt::PROMPT_VERSION,
            "results": results })
}

fn results_from_checkpoint(cp: Option<&Value>, hash: &str, model: &str) -> Vec<TurnResult> {
    let Some(cp) = cp else {
        return Vec::new();
    };
    let same = cp["words_hash"].as_str() == Some(hash)
        && cp["model"].as_str() == Some(model)
        && cp["prompt_version"].as_u64() == Some(u64::from(norm::prompt::PROMPT_VERSION));
    if !same {
        return Vec::new();
    }
    serde_json::from_value(cp["results"].clone()).unwrap_or_default()
}

impl Inner {
    async fn run_normalize(
        self: &Arc<Self>,
        ctx: &JobContext,
        capture_id: &str,
    ) -> Result<Value, CmError> {
        let language_override = ctx
            .job
            .inputs
            .get("language")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let (library, id) = (self.library.clone(), capture_id.to_owned());
        let (transcript, labels) = blocking(move || {
            let lib = lock_library(&library);
            let rec = lib.get(&id)?;
            let dir = lib.bundle_path(&rec);
            if !dir.is_dir() {
                return Err(CmError::not_found(format!(
                    "The recording folder `{}` was moved or deleted",
                    dir.display()
                )));
            }
            let t = read_transcript(&dir, &id)?;
            let labels = speaker_labels(&dir, &t);
            Ok((t, labels))
        })
        .await
        .map_err(into_cm)?;

        // The settings may have changed since the call: check again.
        let config = self.normalize_gate().map_err(into_cm)?;
        let (me, cfg) = (self.clone(), config.clone());
        let model = blocking(move || me.text_model_client(&cfg))
            .await
            .map_err(into_cm)?;
        let language = language_override.unwrap_or_else(|| transcript.language.clone());
        let hash = norm::words_hash(&transcript.words);
        let resume = results_from_checkpoint(ctx.job.checkpoint.as_ref(), &hash, &config.model);

        let (flag, watcher) = interrupt_flag(ctx);
        let reporter = ctx.reporter.clone();
        let saver = ctx.reporter.clone();
        let (words, lang2, model_id, hash2) = (
            transcript.words.clone(),
            language.clone(),
            config.model.clone(),
            hash.clone(),
        );
        let context_tokens = config.context_tokens as usize;
        let started = std::time::Instant::now();
        let ran = tokio::task::spawn_blocking(move || {
            let opts = norm::Options::new(&lang2, context_tokens);
            let mut last = -1.0_f32;
            let mut last_save = std::time::Instant::now();
            norm::normalize(
                model.as_ref(),
                &words,
                labels.as_deref(),
                &opts,
                resume,
                &mut |results| {
                    // A failed save only costs the resume, never the run.
                    if last_save.elapsed().as_secs() >= 5 {
                        last_save = std::time::Instant::now();
                        let _ = saver.save_checkpoint(&checkpoint_value(&hash2, &model_id, results));
                    }
                },
                &mut |p| {
                    if p >= 1.0 || p - last >= 0.01 {
                        last = p;
                        reporter.report(
                            f64::from(p),
                            Some(format!("Adding punctuation {}%", (p * 100.0) as u32)),
                        );
                    }
                    !flag.load(Ordering::SeqCst)
                },
            )
        })
        .await;
        watcher.abort();
        let (results, stats) = match ran {
            Ok(Ok(r)) => r,
            Ok(Err(e)) => return Err(e.into_cm_error(&ctx.job.id)),
            Err(e) => return Err(CmError::internal(format!("normalize worker failed: {e}"))),
        };
        let seconds = started.elapsed().as_secs_f64();

        // Written only now, after a success.
        let _serial = self.capture_locks.lock(capture_id).await;
        let (library, view) = (self.library.clone(), self.library_view());
        let (cid, words, lang, model_name) = (
            capture_id.to_owned(),
            transcript.words,
            language,
            config.model.clone(),
        );
        let n: Normalized = blocking(move || {
            let lib = lock_library(&library);
            let rec = lib.get(&cid)?;
            let dir = lib.bundle_path(&rec);
            view.ensure_idle(&rec, &dir, "tidy the transcript of")?;
            // The transcript must still be the one the model saw.
            let now = read_transcript(&dir, &cid)?;
            if norm::words_hash(&now.words) != norm::words_hash(&words) {
                return Err(CmError::invalid_input(
                    "The transcript changed while it was being tidied; run it again",
                ));
            }
            let n = Normalized::from_run(&words, &lang, &model_name, &now_rfc3339(), results, &stats);
            norm::write(&dir, &n)?;
            Ok(n)
        })
        .await
        .map_err(into_cm)?;
        self.events.publish(
            events::TRANSCRIPT_NORMALIZED,
            json!({ "capture_id": capture_id, "turns": n.summary.turns,
                    "normalized": n.summary.normalized, "rejected": n.summary.rejected,
                    "words": n.summary.words, "model": n.model }),
        );
        Ok(json!({
            "capture_id": capture_id, "turns": n.summary.turns,
            "normalized": n.summary.normalized, "rejected": n.summary.rejected,
            "rejected_words": n.summary.rejected_words, "words": n.summary.words,
            "model_calls": n.summary.model_calls, "retried": n.summary.retried,
            "model": n.model, "language": n.language, "seconds": seconds,
        }))
    }
}
