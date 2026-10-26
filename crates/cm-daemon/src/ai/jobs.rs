use super::*;

// ---- settings ------------------------------------------------------------------------------

/// Installed RAM in GB, when this OS can tell without extra dependencies.
pub fn system_ram_gb() -> Option<u64> {
    #[cfg(target_os = "macos")]
    {
        let out = std::process::Command::new("sysctl")
            .args(["-n", "hw.memsize"])
            .output()
            .ok()?;
        let bytes: u64 = String::from_utf8_lossy(&out.stdout).trim().parse().ok()?;
        Some(bytes / 1_000_000_000)
    }
    #[cfg(target_os = "linux")]
    {
        let text = std::fs::read_to_string("/proc/meminfo").ok()?;
        let kb: u64 = text
            .lines()
            .find_map(|l| l.strip_prefix("MemTotal:"))?
            .split_whitespace()
            .next()?
            .parse()
            .ok()?;
        Some(kb / 1_000_000)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        None
    }
}

/// Whether the settings ask for an automatic transcription: the user's choice if there is one,
/// otherwise on only when the machine has [`AUTO_MIN_RAM_GB`] of RAM or more (AI-1).
pub fn auto_transcribe_enabled(settings: &Map<String, Value>, ram_gb: Option<u64>) -> bool {
    match settings
        .get(SETTING_AUTO_TRANSCRIBE)
        .and_then(Value::as_bool)
    {
        Some(choice) => choice,
        None => ram_gb.is_some_and(|g| g >= AUTO_MIN_RAM_GB),
    }
}

// ---- jobs ----------------------------------------------------------------------------------

pub(super) fn no_model_error() -> DaemonError {
    DaemonError::WithData {
        error: CmError::invalid_input(
            "No speech model is installed, so nothing can be transcribed. Open the AI page in \
             CaptureMann, or run `capturemann ai models`, to download or import one. Nothing is \
             downloaded automatically.",
        ),
        data: json!({
            "reason": "no_model",
            "hint": "capturemann ai models download small",
        }),
    }
}

pub(super) fn model_not_installed(model: &str, installed: &[String]) -> DaemonError {
    DaemonError::WithData {
        error: CmError::invalid_input(format!(
            "The speech model `{model}` is not installed. Installed: {}. Download it from the AI \
             page or with `capturemann ai models download {model}`.",
            if installed.is_empty() {
                "none".to_owned()
            } else {
                installed.join(", ")
            }
        )),
        data: json!({ "reason": "model_not_installed", "model": model }),
    }
}

/// Writes `bytes` to `dir/name` through a temporary file, so a reader never sees half of it.
pub(super) fn write_atomic(dir: &Path, name: &str, bytes: &[u8]) -> Result<PathBuf, CmError> {
    let target = dir.join(name);
    let tmp = dir.join(format!(".{name}.tmp"));
    let written = std::fs::write(&tmp, bytes).and_then(|()| std::fs::rename(&tmp, &target));
    if let Err(e) = written {
        // Never leave a half-written temp file behind (the target is untouched).
        let _ = std::fs::remove_file(&tmp);
        return Err(CmError::internal(format!(
            "cannot write `{}`: {e}",
            target.display()
        )));
    }
    Ok(target)
}

/// Sets a flag when the job is interrupted (cancel, hold or shutdown), for blocking code.
fn interrupt_flag(ctx: &JobContext) -> (Arc<AtomicBool>, tokio::task::JoinHandle<()>) {
    let flag = Arc::new(AtomicBool::new(false));
    let watcher = {
        let mut token = ctx.cancel.clone();
        let flag = flag.clone();
        tokio::spawn(async move {
            token.interrupted().await;
            flag.store(true, Ordering::SeqCst);
        })
    };
    (flag, watcher)
}

/// Runs `model_download` jobs.
pub(super) struct ModelDownloadHandler {
    pub(super) events: EventBus,
    pub(super) service: Arc<dyn ModelService>,
}

#[async_trait]
impl JobHandler for ModelDownloadHandler {
    async fn run(&self, ctx: JobContext) -> Result<Value, CmError> {
        let model = ctx
            .job
            .inputs
            .get("model")
            .and_then(Value::as_str)
            .ok_or_else(|| CmError::invalid_field("model", "is required"))?
            .to_owned();
        let (flag, watcher) = interrupt_flag(&ctx);
        let (service, events, reporter, run_flag, id) = (
            self.service.clone(),
            self.events.clone(),
            ctx.reporter.clone(),
            flag.clone(),
            model.clone(),
        );
        let joined = tokio::task::spawn_blocking(move || {
            let mut last = -1.0_f64;
            service.download(&id, &mut |p: DownloadProgress| {
                let fraction = if p.bytes_total == 0 {
                    0.0
                } else {
                    (p.bytes_done as f64 / p.bytes_total as f64).clamp(0.0, 1.0)
                };
                if fraction >= 1.0 || fraction - last >= 0.01 {
                    last = fraction;
                    reporter.report(
                        fraction,
                        Some(format!(
                            "Downloading {} {}%",
                            p.model,
                            (fraction * 100.0) as u32
                        )),
                    );
                    events.publish(
                        events::MODEL_DOWNLOAD_PROGRESS,
                        json!({ "model": p.model, "bytes_done": p.bytes_done,
                                "bytes_total": p.bytes_total, "fraction": fraction }),
                    );
                }
                !run_flag.load(Ordering::SeqCst)
            })
        })
        .await;
        watcher.abort();
        match joined {
            Ok(Ok(true)) => {
                self.events.publish(
                    events::MODEL_CHANGED,
                    json!({ "model": model, "change": "downloaded" }),
                );
                Ok(json!({ "model": model }))
            }
            Ok(Ok(false)) => Err(CmError::internal("The download was interrupted")),
            Ok(Err(e)) => Err(e),
            Err(e) => Err(CmError::internal(format!("download worker failed: {e}"))),
        }
    }
}

/// Runs `transcribe` jobs.
pub(super) struct TranscribeHandler {
    pub(super) library: Arc<std::sync::Mutex<cm_library::Library>>,
    pub(super) events: EventBus,
    pub(super) service: Arc<dyn TranscribeService>,
    pub(super) captions: Arc<dyn CaptionService>,
    pub(super) settings: Arc<dyn crate::engines::SettingsStore>,
}

#[async_trait]
impl JobHandler for TranscribeHandler {
    async fn run(&self, ctx: JobContext) -> Result<Value, CmError> {
        let inputs = &ctx.job.inputs;
        let capture_id = inputs
            .get("capture_id")
            .and_then(Value::as_str)
            .ok_or_else(|| CmError::invalid_field("capture_id", "is required"))?
            .to_owned();
        let model = inputs
            .get("model")
            .and_then(Value::as_str)
            .ok_or_else(|| CmError::invalid_field("model", "is required"))?
            .to_owned();
        let language = inputs
            .get("language")
            .and_then(Value::as_str)
            .map(str::to_owned);
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
            .map_err(|e| match e {
                DaemonError::Cm(c) => c,
                other => CmError::internal(other.to_string()),
            })?
        };
        let (flag, watcher) = interrupt_flag(&ctx);
        // The glossary as it is when the run starts (AI-6a); a damaged setting is ignored.
        let glossary = cm_ai::glossary::from_value(
            self.settings
                .all()
                .ok()
                .as_ref()
                .and_then(|m| m.get(cm_ai::glossary::SETTING_GLOSSARY)),
        );
        let req = TranscribeRequest {
            capture_id: capture_id.clone(),
            bundle_dir: dir.clone(),
            language,
            model: model.clone(),
            glossary: glossary.clone(),
        };
        let (service, reporter, run_flag) = (self.service.clone(), ctx.reporter.clone(), flag);
        let joined = tokio::task::spawn_blocking(move || {
            let mut last = -1.0_f32;
            service.transcribe(&req, &mut |p| {
                if p >= 1.0 || p - last >= 0.01 {
                    last = p;
                    reporter.report(
                        f64::from(p),
                        Some(format!("Transcribing {}%", (p * 100.0) as u32)),
                    );
                }
                !run_flag.load(Ordering::SeqCst)
            })
        })
        .await;
        watcher.abort();
        let mut transcript = match joined {
            Ok(Ok(Some(t))) => t,
            Ok(Ok(None)) => return Err(CmError::internal("The transcription was interrupted")),
            Ok(Err(e)) => return Err(e),
            Err(e) => {
                return Err(CmError::internal(format!(
                    "transcription worker failed: {e}"
                )))
            }
        };
        // What Whisper tends to get wrong for this user, spelled their way (AI-6a). The words are
        // replaced before anything is stored, so captions, TXT and the suggestions all see them.
        let (words, glossary_replaced) =
            cm_ai::glossary::apply(std::mem::take(&mut transcript.words), &glossary);
        transcript.words = words;
        let bytes = serde_json::to_vec_pretty(&transcript)
            .map_err(|e| CmError::internal(format!("transcript: {e}")))?;
        let by = inputs
            .get("by")
            .and_then(Value::as_str)
            .and_then(|b| b.parse::<Actor>().ok())
            .unwrap_or(Actor::User);
        let (library, write_dir) = (self.library.clone(), dir);
        let (captions, new_transcript) = (self.captions.clone(), transcript.clone());
        // The new transcript replaces the old one in one rename, only now that the run finished:
        // a failed or cancelled run never reached this point, so the previous transcript is intact.
        // Caption lines the user edited belong to the old transcript. They are kept where the same
        // words were heard again (`cm_ai::reapply`, AI-6a) and dropped otherwise; either way the
        // caption change is ONE undoable revision (`undo` brings the old lines back).
        let (path, replaced, revision, shown, reapplied, dropped) = blocking(move || {
            let old_transcript: Option<Transcript> =
                std::fs::read_to_string(write_dir.join(TRANSCRIPT_FILE))
                    .ok()
                    .and_then(|t| serde_json::from_str(&t).ok());
            let replaced = write_dir.join(TRANSCRIPT_FILE).is_file();
            let manifest = cm_library::read_manifest(&write_dir)?;
            let stored = cm_library::rec_edit::captions_of(&manifest).cloned();
            // Work out what survives before the old transcript is overwritten.
            let kept = stored.as_ref().map(|v| {
                let stored_lines: Vec<CaptionLine> =
                    serde_json::from_value(v.clone()).unwrap_or_default();
                let old_auto = old_transcript.as_ref().and_then(|t| captions.lines(t).ok());
                let new_auto = captions.lines(&new_transcript).ok();
                match (old_auto, new_auto) {
                    (Some(old), Some(new)) => {
                        let r = cm_ai::reapply::reapply_edits(&old, &stored_lines, &new);
                        if r.reapplied > 0 {
                            (json!(r.lines), r.reapplied, r.dropped)
                        } else {
                            (json!([]), 0, r.dropped)
                        }
                    }
                    // No way to compare: nothing can be kept.
                    _ => (json!([]), 0, stored_lines.len()),
                }
            });
            let p = write_atomic(&write_dir, TRANSCRIPT_FILE, &bytes)?;
            let mut lib = lock_library(&library);
            let (revision, reapplied, dropped) = match kept {
                Some((lines, reapplied, dropped)) => {
                    let out = cm_library::rec_edit::apply_captions(
                        &write_dir,
                        lines,
                        by,
                        time::OffsetDateTime::now_utc(),
                    )?;
                    (Some(out.revision), reapplied, dropped)
                }
                None => (None, 0, 0),
            };
            // The recording's FIRST transcript switches the preview captions on (export stays off),
            // unless the user already decided: see `default_captions_on_first_transcript`. Not a
            // revision, so there is no extra undo step.
            let shown = if replaced {
                false
            } else {
                cm_library::rec_edit::default_captions_on_first_transcript(&write_dir)?
            };
            // The bundle changed: refresh the index row.
            lib.index_dir(&write_dir)?;
            Ok((p, replaced, revision, shown, reapplied, dropped))
        })
        .await
        .map_err(|e| match e {
            DaemonError::Cm(c) => c,
            other => CmError::internal(other.to_string()),
        })?;
        if let Some(rev) = &revision {
            self.events.publish(
                events::CAPTURE_REVISED,
                json!({ "capture_id": capture_id, "revision": rev.rev,
                        "action": rev.action, "by": rev.by }),
            );
        }
        self.events.publish(
            events::TRANSCRIPT_READY,
            json!({ "capture_id": capture_id, "model": transcript.model,
                    "language": transcript.language, "words": transcript.words.len(),
                    "replaced": replaced, "captions_cleared": revision.is_some(),
                    "captions_shown": shown, "reapplied": reapplied, "dropped": dropped,
                    "glossary_replaced": glossary_replaced }),
        );
        Ok(json!({ "capture_id": capture_id, "model": transcript.model,
                   "language": transcript.language, "words": transcript.words.len(),
                   "path": path, "replaced": replaced,
                   "captions_cleared": revision.is_some(),
                   "captions_shown": shown, "reapplied": reapplied, "dropped": dropped,
                   "glossary_replaced": glossary_replaced }))
    }
}

/// The recording's mic or system track is what a transcript is made from.
pub(super) fn has_speech_track(manifest: &cm_library::manifest::RecManifest) -> bool {
    ["mic", "system"].iter().any(|k| {
        manifest
            .tracks
            .get(*k)
            .is_some_and(|t| t.get("missing") != Some(&json!(true)))
    })
}

/// Enqueues a transcription unless an equal one is queued, running or paused (then that job is
/// returned, `true` = reused).
pub(super) fn enqueue_transcribe(
    queue: &JobQueue,
    capture_id: &str,
    model: &str,
    language: Option<&str>,
    by: &str,
    auto: bool,
) -> Result<(Job, bool), CmError> {
    static ENQUEUE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _one_at_a_time = ENQUEUE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let active = queue.list(&JobFilter {
        job_type: Some(JOB_TRANSCRIBE.to_owned()),
        states: Some(vec![JobState::Queued, JobState::Running, JobState::Paused]),
        limit: Some(200),
    })?;
    if let Some(j) = active.into_iter().find(|j| {
        j.inputs.get("capture_id").and_then(Value::as_str) == Some(capture_id)
            && j.inputs.get("model").and_then(Value::as_str) == Some(model)
            && j.inputs.get("language").and_then(Value::as_str) == language
    }) {
        return Ok((j, true));
    }
    let job = queue.enqueue(NewJob::new(
        JOB_TRANSCRIBE,
        json!({ "capture_id": capture_id, "model": model, "language": language,
                "by": by, "auto": auto }),
    ))?;
    Ok((job, false))
}
