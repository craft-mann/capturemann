//! Caption translation (AI-5): the `translate_captions` action and its `translate` job, and the
//! `language` variants of `get_captions`, `set_captions`, `replace_in_captions` and
//! `export_captions`. A translation lives in the bundle as `translations/<lang>.json`
//! ([`cm_ai::translate::Translation`]); the original captions and the transcript are never touched.
//!
//! Two engines (`engine`: `auto`, `whisper`, `text_model`): Whisper re-runs the audio with the
//! translate task (English only, offline; the `large-v3-turbo` model is never used for it), and a
//! text model translates the caption lines (AI-7; not plugged in yet, so a non-English target
//! fails with `invalid_input` and `data.reason = "no_text_model"`). Nothing is ever downloaded
//! here: a missing Whisper model fails with `data.reason = "translation_model_missing"`.
//!
//! Edits to a translation are written to its file (no manifest revision, so no undo step yet) and
//! survive a re-translate where the source caption line is unchanged (AI-5.AC5).

use cm_ai::translate::{self as tr, Engine, EngineChoice, Translation, TranslationLine};

use super::*;

/// Job type of a caption translation.
pub const JOB_TRANSLATE: &str = "translate";

/// What a translation run needs.
#[derive(Debug, Clone, PartialEq)]
pub struct TranslateRequest {
    pub capture_id: String,
    pub bundle_dir: PathBuf,
    /// The recording's language (`transcript.language`).
    pub source_language: String,
    /// A code from [`cm_ai::translate::language_code`].
    pub target_language: String,
    /// The Whisper model to load (Whisper engine); empty for the text model.
    pub model: String,
    /// The caption lines to translate: the edited ones, else the generated ones.
    pub source_lines: Vec<CaptionLine>,
    /// Glossary terms to leave as written (the text model engine).
    pub keep_as_written: Vec<String>,
}

/// What a translation run produced; the job stores it with the hand edits carried over.
#[derive(Debug, Clone, PartialEq)]
pub struct TranslateOutput {
    pub lines: Vec<TranslationLine>,
    /// The model that did the work (the Whisper model id, or the text model's name).
    pub model: String,
    pub untranslated: usize,
}

/// Runs a translation (`cm_ai::translate`). Blocking.
pub trait TranslateService: Send + Sync {
    fn available(&self) -> bool {
        true
    }
    /// Whether a text model is set up (AI-7). Decides `auto` for targets other than English.
    fn has_text_model(&self) -> bool {
        false
    }
    /// Whisper's translate task on the recording's audio, into English. `progress` returns `false`
    /// to interrupt, which gives `Ok(None)`.
    fn whisper(
        &self,
        req: &TranslateRequest,
        progress: &mut dyn FnMut(f32) -> bool,
    ) -> Result<Option<TranslateOutput>, CmError>;
    /// The text model, line by line in batches. The default is "no text model".
    fn text_model(
        &self,
        req: &TranslateRequest,
        progress: &mut dyn FnMut(f32) -> bool,
    ) -> Result<Option<TranslateOutput>, CmError> {
        let _ = (req, progress);
        Err(tr::no_text_model())
    }
}

impl TranslateService for Unavailable {
    fn available(&self) -> bool {
        false
    }
    fn whisper(
        &self,
        _: &TranslateRequest,
        _: &mut dyn FnMut(f32) -> bool,
    ) -> Result<Option<TranslateOutput>, CmError> {
        Err(not_built())
    }
}

/// An error with the `data.reason` the translation helpers know.
fn reasoned(e: CmError) -> DaemonError {
    match tr::error_data(&e) {
        Some(data) => DaemonError::WithData { error: e, data },
        None => e.into(),
    }
}

fn load_translation(dir: &Path, id: &str, language: &str) -> Result<Translation, CmError> {
    tr::read_translation(dir, language)?.ok_or_else(|| {
        CmError::not_found(format!(
            "`{id}` has no `{language}` translation; run `translate_captions` with \
             `target_language: \"{language}\"` first"
        ))
    })
}

/// The transcript and the caption lines translations are made from: the edited lines when there
/// are some, else the lines built from the transcript.
fn read_source(
    dir: &Path,
    id: &str,
    captions: &dyn CaptionService,
) -> Result<(Transcript, Vec<CaptionLine>), CmError> {
    let path = dir.join(TRANSCRIPT_FILE);
    let text = std::fs::read_to_string(&path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            CmError::not_found(format!(
                "`{id}` has no transcript to translate; run `transcribe` first"
            ))
        } else {
            CmError::internal(format!("cannot read `{}`: {e}", path.display()))
        }
    })?;
    let transcript: Transcript = serde_json::from_str(&text)
        .map_err(|e| CmError::internal(format!("`{}` is not a transcript: {e}", path.display())))?;
    let manifest = cm_library::read_manifest(dir)?;
    let lines = match cm_library::rec_edit::captions_of(&manifest) {
        Some(v) => serde_json::from_value(v.clone())
            .map_err(|e| CmError::internal(format!("stored captions: {e}")))?,
        None => captions.lines(&transcript)?,
    };
    Ok((transcript, lines))
}

fn now_rfc3339() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

/// The reply shape of a stored translation (lean lines, so they can go straight back to
/// `set_captions`).
fn translation_json(id: &str, t: &Translation) -> Value {
    let flagged = |f: fn(&TranslationLine) -> bool| -> Vec<usize> {
        t.lines
            .iter()
            .enumerate()
            .filter(|(_, l)| f(l))
            .map(|(i, _)| i)
            .collect()
    };
    json!({
        "capture_id": id,
        "language": t.language,
        "lines": t.captions(),
        "source": "translation",
        "engine": t.engine,
        "model": t.model,
        "created": t.created,
        "edited": t.edited,
        "edited_lines": flagged(|l| l.edited),
        "untranslated_lines": flagged(|l| l.untranslated),
    })
}

impl Inner {
    /// `translate_captions`: checks everything that can be checked now, then enqueues the job.
    pub(super) async fn translate_captions(
        &self,
        actor: &Actor,
        capture_id: String,
        target: String,
        choice: EngineChoice,
    ) -> Result<Value, DaemonError> {
        let _serial = self.capture_locks.lock(&capture_id).await;
        let (library, this, id, captions) = (
            self.library.clone(),
            self.library_view(),
            capture_id.clone(),
            self.ai.captions.clone(),
        );
        let source_language = blocking(move || {
            let lib = lock_library(&library);
            let rec = lib.get(&id)?;
            if rec.kind != CaptureKind::Recording {
                return Err(CmError::invalid_input(format!(
                    "`{id}` is a screenshot; only recordings have captions to translate"
                )));
            }
            let dir = lib.bundle_path(&rec);
            this.ensure_idle(&rec, &dir, "translate")?;
            let (transcript, lines) = read_source(&dir, &id, captions.as_ref())?;
            if lines.is_empty() {
                return Err(CmError::invalid_input(format!(
                    "`{id}` has no caption lines to translate (the transcript has no speech)"
                )));
            }
            Ok(transcript.language)
        })
        .await?;
        let engine = tr::choose_engine(
            choice,
            &target,
            &source_language,
            self.ai.translate.has_text_model(),
        )
        .map_err(reasoned)?;
        let model = match engine {
            Engine::TextModel => String::new(),
            Engine::Whisper => {
                let list = self.model_list().await?;
                let installed: Vec<String> = list
                    .models
                    .iter()
                    .filter(|m| m.installed)
                    .map(|m| m.id.clone())
                    .collect();
                tr::choose_translation_model(&installed, list.active.as_deref(), None)
                    .map_err(reasoned)?
            }
        };
        let queue = self.job_queue()?.queue.clone();
        if !queue.has_handler(JOB_TRANSLATE) {
            return Err(DaemonError::NotImplemented(
                "translation is not available: this daemon has no translator registered".to_owned(),
            ));
        }
        let by = actor.to_string();
        let inputs = json!({
            "capture_id": capture_id, "target_language": target, "engine": engine.as_str(),
            "model": model, "source_language": source_language, "by": by,
        });
        let (id, lang) = (capture_id.clone(), target.clone());
        let (job, reused) = blocking(move || enqueue_translate(&queue, &id, &lang, inputs)).await?;
        Ok(json!({
            "job_id": job.id,
            "capture_id": capture_id,
            "language": target,
            "engine": engine.as_str(),
            "model": model,
            "source_language": source_language,
            "state": job.state.as_str(),
            "reused": reused,
        }))
    }

    /// `get_captions` and `export_captions` take the same optional `language`.
    pub(super) async fn get_captions_in(
        &self,
        capture_id: &str,
        language: Option<&str>,
    ) -> Result<Value, DaemonError> {
        let Some(language) = language else {
            return self.get_captions(capture_id).await;
        };
        let (library, id, language) = (
            self.library.clone(),
            capture_id.to_owned(),
            language.to_owned(),
        );
        blocking(move || {
            let (dir, _) = recording_bundle(&library, &id)?;
            Ok(translation_json(
                &id,
                &load_translation(&dir, &id, &language)?,
            ))
        })
        .await
    }

    /// `set_captions { language }`: replaces the lines of a translation. A line that keeps its
    /// start and text keeps its record; a changed or new line is marked edited.
    pub(super) async fn set_translation_lines(
        &self,
        capture_id: String,
        language: String,
        lines: Vec<CaptionLine>,
    ) -> Result<Value, DaemonError> {
        let _serial = self.capture_locks.lock(&capture_id).await;
        let (library, this, captions, id) = (
            self.library.clone(),
            self.library_view(),
            self.ai.captions.clone(),
            capture_id.clone(),
        );
        blocking(move || {
            let lib = lock_library(&library);
            let rec = lib.get(&id)?;
            if rec.kind != CaptureKind::Recording {
                return Err(CmError::invalid_input(format!(
                    "`{id}` is a screenshot; captions belong to recordings"
                )));
            }
            let dir = lib.bundle_path(&rec);
            this.ensure_idle(&rec, &dir, "caption")?;
            let old = load_translation(&dir, &id, &language)?;
            let source = read_source(&dir, &id, captions.as_ref())
                .map(|(_, l)| l)
                .unwrap_or_default();
            let new = tr::with_edited_lines(&old, &lines, &source);
            tr::write_translation(&dir, &new)?;
            Ok(json!({
                "capture_id": id,
                "language": language,
                "lines": new.lines.len(),
                "edited": new.edited,
                "revision": null,
            }))
        })
        .await
    }

    /// `replace_in_captions { language }`: find and replace over a translation's lines.
    pub(super) async fn replace_in_translation(
        &self,
        capture_id: String,
        language: String,
        pairs: Vec<Replacement>,
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
            if rec.kind != CaptureKind::Recording {
                return Err(CmError::invalid_input(format!(
                    "`{id}` is a screenshot; captions belong to recordings"
                )));
            }
            let dir = lib.bundle_path(&rec);
            this.ensure_idle(&rec, &dir, "caption")?;
            let old = load_translation(&dir, &id, &language)?;
            let mut lines = old.captions();
            let (mut total, mut touched) = (0usize, vec![false; lines.len()]);
            for p in &pairs {
                for (i, l) in lines.iter_mut().enumerate() {
                    if let Some((text, n)) = cm_ai::textmatch::replace_in_line(
                        &l.text,
                        &p.find,
                        &p.replace,
                        p.match_case,
                        p.whole_word,
                    ) {
                        l.text = text;
                        total += n;
                        touched[i] = true;
                    }
                }
            }
            if total == 0 {
                return Ok(json!({
                    "capture_id": id, "language": language, "replacements": 0,
                    "lines_touched": 0, "revision": null, "changed": false,
                }));
            }
            if let Some((i, _)) = lines
                .iter()
                .enumerate()
                .find(|(i, l)| touched[*i] && !cm_ai::textmatch::rows_ok(&l.text))
            {
                return Err(CmError::invalid_input(format!(
                    "the replacement would make caption line {} longer than {MAX_CAPTION_ROWS} rows of {MAX_CAPTION_ROW_CHARS} characters; nothing was changed",
                    i + 1
                )));
            }
            let new = tr::with_edited_lines(&old, &lines, &[]);
            tr::write_translation(&dir, &new)?;
            Ok(json!({
                "capture_id": id, "language": language, "replacements": total,
                "lines_touched": touched.iter().filter(|t| **t).count(),
                "revision": null, "changed": true,
            }))
        })
        .await
    }

    /// `export_captions { language }`: SRT, VTT or TXT of a translation in `exports/`, mapped
    /// through the edit list like the original captions.
    pub(super) async fn export_captions_in(
        &self,
        capture_id: &str,
        format: CaptionFormat,
        language: Option<&str>,
        speaker_names: bool,
    ) -> Result<Value, DaemonError> {
        // A translation has no speaker data; `speaker_names` only applies to the original.
        let Some(language) = language else {
            return self
                .export_captions(capture_id, format, speaker_names)
                .await;
        };
        let _serial = self.capture_locks.lock(capture_id).await;
        let (library, id, captions, language) = (
            self.library.clone(),
            capture_id.to_owned(),
            self.ai.captions.clone(),
            language.to_owned(),
        );
        blocking(move || {
            let (dir, manifest) = recording_bundle(&library, &id)?;
            let title = lock_library(&library).get(&id)?.title;
            let t = load_translation(&dir, &id, &language)?;
            let lines = t.captions();
            let edits = cm_library::rec_edit::load_edits(&manifest);
            let text = captions.render(&lines, format, &edits)?;
            let out_dir = dir.join("exports");
            std::fs::create_dir_all(&out_dir).map_err(|e| {
                CmError::internal(format!("cannot create `{}`: {e}", out_dir.display()))
            })?;
            let safe_title = title.replace(['/', '\\', ':'], "-");
            let name = if format.is_text() {
                format!("{safe_title}.{language}.txt")
            } else {
                format!("{safe_title} - captions.{language}.{}", format.ext())
            };
            let path = write_atomic(&out_dir, &name, text.as_bytes())?;
            lock_library(&library).index_dir(&dir)?;
            Ok(json!({
                "capture_id": id,
                "language": language,
                "format": format.ext(),
                "path": path,
                "lines": lines.len(),
            }))
        })
        .await
    }
}

/// Enqueues a translation unless an equal one (same capture and language) is queued, running or
/// paused; then that job is returned (`true` = reused).
fn enqueue_translate(
    queue: &JobQueue,
    capture_id: &str,
    language: &str,
    inputs: Value,
) -> Result<(Job, bool), CmError> {
    static ENQUEUE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _one_at_a_time = ENQUEUE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let active = queue.list(&JobFilter {
        job_type: Some(JOB_TRANSLATE.to_owned()),
        states: Some(vec![JobState::Queued, JobState::Running, JobState::Paused]),
        limit: Some(200),
    })?;
    if let Some(j) = active.into_iter().find(|j| {
        j.inputs.get("capture_id").and_then(Value::as_str) == Some(capture_id)
            && j.inputs.get("target_language").and_then(Value::as_str) == Some(language)
    }) {
        return Ok((j, true));
    }
    Ok((queue.enqueue(NewJob::new(JOB_TRANSLATE, inputs))?, false))
}

/// Runs `translate` jobs.
pub(super) struct TranslateHandler {
    pub(super) library: Arc<std::sync::Mutex<cm_library::Library>>,
    pub(super) events: EventBus,
    pub(super) service: Arc<dyn TranslateService>,
    pub(super) captions: Arc<dyn CaptionService>,
    pub(super) settings: Arc<dyn crate::engines::SettingsStore>,
}

#[async_trait]
impl JobHandler for TranslateHandler {
    async fn run(&self, ctx: JobContext) -> Result<Value, CmError> {
        let text = |k: &str| {
            ctx.job
                .inputs
                .get(k)
                .and_then(Value::as_str)
                .map(str::to_owned)
        };
        let required = |k: &str| text(k).ok_or_else(|| CmError::invalid_field(k, "is required"));
        let capture_id = required("capture_id")?;
        let target = required("target_language")?;
        let source_language = required("source_language")?;
        let model = text("model").unwrap_or_default();
        let engine = match required("engine")?.as_str() {
            "whisper" => Engine::Whisper,
            _ => Engine::TextModel,
        };
        let (library, id, captions) = (
            self.library.clone(),
            capture_id.clone(),
            self.captions.clone(),
        );
        let (dir, source_lines) = blocking(move || {
            let lib = lock_library(&library);
            let rec = lib.get(&id)?;
            let dir = lib.bundle_path(&rec);
            if !dir.is_dir() {
                return Err(CmError::not_found(format!(
                    "The recording folder `{}` was moved or deleted",
                    dir.display()
                )));
            }
            let (_, lines) = read_source(&dir, &id, captions.as_ref())?;
            Ok((dir, lines))
        })
        .await
        .map_err(|e| match e {
            DaemonError::Cm(c) => c,
            other => CmError::internal(other.to_string()),
        })?;
        let keep_as_written = cm_ai::glossary::from_value(
            self.settings
                .all()
                .ok()
                .as_ref()
                .and_then(|m| m.get(cm_ai::glossary::SETTING_GLOSSARY)),
        )
        .into_iter()
        .map(|g| g.term)
        .collect();
        let req = TranslateRequest {
            capture_id: capture_id.clone(),
            bundle_dir: dir.clone(),
            source_language,
            target_language: target.clone(),
            model: model.clone(),
            source_lines,
            keep_as_written,
        };
        let (flag, watcher) = interrupt_flag(&ctx);
        let (service, reporter) = (self.service.clone(), ctx.reporter.clone());
        let joined = tokio::task::spawn_blocking(move || {
            let mut last = -1.0_f32;
            let mut progress = |p: f32| {
                if p >= 1.0 || p - last >= 0.01 {
                    last = p;
                    reporter.report(
                        f64::from(p),
                        Some(format!("Translating {}%", (p * 100.0) as u32)),
                    );
                }
                !flag.load(Ordering::SeqCst)
            };
            match engine {
                Engine::Whisper => service.whisper(&req, &mut progress),
                Engine::TextModel => service.text_model(&req, &mut progress),
            }
        })
        .await;
        watcher.abort();
        let out = match joined {
            Ok(Ok(Some(o))) => o,
            Ok(Ok(None)) => return Err(CmError::internal("The translation was interrupted")),
            Ok(Err(e)) => return Err(e),
            Err(e) => return Err(CmError::internal(format!("translation worker failed: {e}"))),
        };
        // The new file replaces the old one in one rename, with the user's hand edits carried over
        // where the source line is unchanged (AI-5.AC5).
        let (library, write_dir, language) = (self.library.clone(), dir, target.clone());
        let produced_model = out.model.clone();
        let (translation, carried, path) = blocking(move || {
            let old = tr::read_translation(&write_dir, &language).ok().flatten();
            let (t, carried) = tr::assemble(
                &language,
                engine,
                &out.model,
                &now_rfc3339(),
                out.lines,
                old.as_ref(),
            );
            let path = tr::write_translation(&write_dir, &t)?;
            lock_library(&library).index_dir(&write_dir)?;
            Ok((t, carried, path))
        })
        .await
        .map_err(|e| match e {
            DaemonError::Cm(c) => c,
            other => CmError::internal(other.to_string()),
        })?;
        let untranslated = translation.untranslated();
        let result = json!({
            "capture_id": capture_id, "language": target, "engine": engine.as_str(),
            "model": produced_model, "lines": translation.lines.len(),
            "untranslated": untranslated, "path": path,
            "kept_edits": carried.kept, "dropped_edits": carried.dropped,
        });
        self.events
            .publish(events::TRANSLATION_READY, result.clone());
        Ok(result)
    }
}
