//! Summaries and reports (AI-9): `list_report_templates`, `generate_report` (a `report` job),
//! `get_report`, `export_report`, `delete_report` and `edit_report`.
//!
//! The text model is the AI-7 provider and **only a local one** (loopback) is allowed for now: with
//! provider `none` or any non-local provider every report call that would send text fails with
//! `invalid_input` and `data.reason = "no_text_model"` (remote providers are deferred by
//! maintainer decision, Oct 26, 2026). So `generate_report` is Free for agents and nothing
//! leaves the machine.
//!
//! Storage and undo: [`cm_ai::report::store`]. Each create, regenerate, edit and delete is one
//! revision (`rec_edit::apply_extra` on the manifest key `reports`, the index), so `undo` and
//! `redo` bring the previous report back. A regenerate keeps the old report until the new one is
//! written: a failed run changes nothing.

use std::sync::Weak;

use cm_ai::report::{self, generate, store, Line, MarkerIn, ReportContext};
use cm_ai::speakers as spk;
use cm_library::rec_edit;

use super::jobs::write_atomic;
use super::*;
use crate::text_model_ops::setup_error;
use cm_ai::text_model::TextModelError;

/// Job type of a report run.
pub const JOB_REPORT: &str = cm_ai::report::JOB_REPORT;
/// Longest Markdown `edit_report` takes.
pub const MAX_REPORT_MARKDOWN_CHARS: usize = 400_000;

/// File formats of `export_report`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportFormat {
    Md,
    Txt,
}

impl ReportFormat {
    pub fn ext(self) -> &'static str {
        match self {
            ReportFormat::Md => "md",
            ReportFormat::Txt => "txt",
        }
    }
}

/// `list_report_templates`.
pub(super) fn list_templates() -> Value {
    json!({ "templates": report::templates().iter().map(|t| t.summary_json()).collect::<Vec<_>>() })
}

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

fn parse_actor(by: &str) -> Actor {
    by.strip_prefix("agent:")
        .and_then(|n| Actor::agent(n).ok())
        .unwrap_or(Actor::User)
}

/// What a report is made from.
pub(crate) struct Source {
    pub language: String,
    pub lines: Vec<Line>,
    pub speakers: Vec<String>,
    pub markers: Vec<MarkerIn>,
}

/// Reads the transcript (the edited caption lines win), speakers and markers of a bundle.
pub(crate) fn load_source(
    dir: &Path,
    id: &str,
    captions: &dyn CaptionService,
) -> Result<Source, CmError> {
    let path = dir.join(TRANSCRIPT_FILE);
    let text = std::fs::read_to_string(&path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            CmError::not_found(format!(
                "`{id}` has no transcript to write a report from; run `transcribe` first"
            ))
        } else {
            CmError::internal(format!("cannot read `{}`: {e}", path.display()))
        }
    })?;
    let transcript: Transcript = serde_json::from_str(&text)
        .map_err(|e| CmError::internal(format!("`{}` is not a transcript: {e}", path.display())))?;
    let manifest = cm_library::read_manifest(dir)?;
    let caption_lines: Vec<CaptionLine> = match rec_edit::captions_of(&manifest) {
        Some(v) => serde_json::from_value(v.clone())
            .map_err(|e| CmError::internal(format!("stored captions: {e}")))?,
        None => captions.lines(&transcript)?,
    };
    let state = spk::SpeakerState::from_manifest(&manifest);
    let labels = state
        .as_ref()
        .map(|s| spk::assign_words(&transcript.words, &s.segments));
    let mut lines = Vec::with_capacity(caption_lines.len());
    for l in &caption_lines {
        let speaker = match (&state, &labels) {
            (Some(state), Some(labels)) => {
                let mut counts: std::collections::BTreeMap<u32, usize> = Default::default();
                for (w, lab) in transcript.words.iter().zip(labels) {
                    let mid = (w.start.ticks() + w.end.ticks()) / 2;
                    if mid >= l.start.ticks() && mid < l.end.ticks() {
                        if let Some(id) = lab.speaker {
                            *counts.entry(id).or_default() += 1;
                        }
                    }
                }
                counts
                    .into_iter()
                    .max_by_key(|(_, n)| *n)
                    .map(|(id, _)| state.name_of(id))
            }
            _ => None,
        };
        lines.push(Line {
            start: l.start.as_secs_f64(),
            end: l.end.as_secs_f64(),
            speaker,
            text: l.text.clone(),
        });
    }
    let speakers = state
        .map(|s| s.speakers.iter().map(|x| x.name.clone()).collect())
        .unwrap_or_default();
    let markers = manifest
        .markers
        .iter()
        .map(|m| MarkerIn {
            at: cm_ai::recording::file_to_session(&manifest.gaps, m.at, false).as_secs_f64(),
            label: m.label.clone(),
        })
        .collect();
    Ok(Source {
        language: transcript.language,
        lines,
        speakers,
        markers,
    })
}

fn check_report_id(entries: &[store::IndexEntry], id: &str) -> Result<(), CmError> {
    if entries.iter().any(|e| e.id == id) {
        Ok(())
    } else {
        Err(CmError::not_found(format!(
            "this recording has no report `{id}`; `get_report` without a report id lists them"
        )))
    }
}

/// Changes the index as one revision and refreshes the live files.
fn commit_index(
    lib: &mut cm_library::Library,
    dir: &Path,
    action: &str,
    entries: &[store::IndexEntry],
    by: Actor,
    now: time::OffsetDateTime,
) -> Result<rec_edit::EditOutcome, CmError> {
    let out = rec_edit::apply_extra(
        dir,
        action,
        vec![(
            store::MANIFEST_REPORTS.to_owned(),
            Some(store::index_value(entries)),
        )],
        by,
        now,
    )?;
    store::sync_live(dir, entries);
    lib.index_dir(dir)?;
    Ok(out)
}

fn revision_json(capture_id: &str, report_id: &str, out: &rec_edit::EditOutcome) -> Value {
    json!({
        "capture_id": capture_id, "report_id": report_id,
        "revision": out.revision.rev, "action": out.revision.action, "by": out.revision.by,
        "can_undo": out.can_undo, "can_redo": out.can_redo,
    })
}

impl Inner {
    /// Local text model only: provider `none` and every non-local provider answer `no_text_model`.
    fn report_gate(&self) -> Result<cm_ai::text_model::TextModelConfig, DaemonError> {
        let config = self.text_model_config()?;
        if config.provider == cm_ai::text_model::Provider::None || !config.is_local() {
            return Err(setup_error(TextModelError::no_text_model()));
        }
        config.check_endpoint().map_err(setup_error)?;
        Ok(config)
    }

    /// `generate_report`: checks everything that can be checked now, then queues the job.
    pub(super) async fn generate_report(
        &self,
        actor: &Actor,
        capture_id: String,
        template: String,
        language: Option<String>,
        context: ReportContext,
        report_id: Option<String>,
    ) -> Result<Value, DaemonError> {
        let _serial = self.capture_locks.lock(&capture_id).await;
        let (library, this, id, captions, regen) = (
            self.library.clone(),
            self.library_view(),
            capture_id.clone(),
            self.ai.captions.clone(),
            report_id.clone(),
        );
        blocking(move || {
            let lib = lock_library(&library);
            let rec = lib.get(&id)?;
            if !rec.kind.is_recording() {
                return Err(CmError::invalid_input(format!(
                    "`{id}` is a screenshot; reports are made from recordings"
                )));
            }
            let dir = lib.bundle_path(&rec);
            this.ensure_idle(&rec, &dir, "write a report of")?;
            let source = load_source(&dir, &id, captions.as_ref())?;
            report::check_enough_speech(&source.lines)?;
            if let Some(r) = &regen {
                let m = cm_library::read_manifest(&dir)?;
                check_report_id(&store::index_of(&m), r)?;
            }
            Ok(())
        })
        .await?;
        self.report_gate()?;
        let queue = self.job_queue()?.queue.clone();
        if !queue.has_handler(JOB_REPORT) {
            return Err(DaemonError::NotImplemented(
                "reports are not available: this daemon has no report runner registered".to_owned(),
            ));
        }
        let inputs = json!({
            "capture_id": capture_id, "template": template, "language": language,
            "context": context, "report_id": report_id, "by": actor.to_string(),
        });
        let (id, tpl, rid) = (capture_id.clone(), template.clone(), report_id.clone());
        let (job, reused) =
            blocking(move || enqueue_report(&queue, &id, &tpl, rid, inputs)).await?;
        Ok(json!({
            "job_id": job.id, "capture_id": capture_id, "template": template,
            "report_id": report_id, "state": job.state.as_str(), "reused": reused,
        }))
    }

    /// `get_report`: one report, or (without an id) the list and the newest one.
    pub(super) async fn get_report(
        &self,
        capture_id: String,
        report_id: Option<String>,
    ) -> Result<Value, DaemonError> {
        let library = self.library.clone();
        blocking(move || {
            let (dir, manifest) = recording_bundle(&library, &capture_id)?;
            let entries = store::index_of(&manifest);
            store::sync_live(&dir, &entries);
            if let Some(rid) = report_id {
                let r = store::read(&dir, &entries, &rid)?;
                return Ok(r.full_json(&capture_id));
            }
            let mut reports = Vec::new();
            let mut latest = Value::Null;
            for e in &entries {
                let r = store::read_version(&dir, &e.id, e.v)?;
                reports.push(r.summary_json());
                latest = r.full_json(&capture_id);
            }
            Ok(json!({ "capture_id": capture_id, "reports": reports, "latest": latest }))
        })
        .await
    }

    /// `export_report`: the Markdown or plain text in `out_dir` (default `exports/`).
    pub(super) async fn export_report(
        &self,
        capture_id: String,
        report_id: String,
        format: ReportFormat,
        out_dir: Option<String>,
    ) -> Result<Value, DaemonError> {
        let _serial = self.capture_locks.lock(&capture_id).await;
        let library = self.library.clone();
        blocking(move || {
            let (dir, manifest) = recording_bundle(&library, &capture_id)?;
            let entries = store::index_of(&manifest);
            let r = store::read(&dir, &entries, &report_id)?;
            let title = lock_library(&library).get(&capture_id)?.title;
            let target = match out_dir {
                Some(d) => {
                    let p = PathBuf::from(&d);
                    if !p.is_absolute() || !p.is_dir() {
                        return Err(CmError::invalid_field(
                            "out_dir",
                            "must be an existing folder (absolute path)",
                        ));
                    }
                    p
                }
                None => {
                    let p = dir.join("exports");
                    std::fs::create_dir_all(&p).map_err(|e| {
                        CmError::internal(format!("cannot create `{}`: {e}", p.display()))
                    })?;
                    p
                }
            };
            let md = r.markdown_text();
            let body = match format {
                ReportFormat::Md => md,
                ReportFormat::Txt => report::render::plain_text(&md),
            };
            let base = format!("{title} - {}", r.template_name).replace(['/', '\\', ':'], "-");
            // Never overwrite a file that is already there.
            let mut name = format!("{base}.{}", format.ext());
            let mut n = 1;
            while target.join(&name).exists() {
                n += 1;
                name = format!("{base} ({n}).{}", format.ext());
            }
            let path = write_atomic(&target, &name, body.as_bytes())?;
            if target.starts_with(&dir) {
                lock_library(&library).index_dir(&dir)?;
            }
            Ok(json!({
                "capture_id": capture_id, "report_id": report_id,
                "format": format.ext(), "path": path,
            }))
        })
        .await
    }

    /// `delete_report`: one revision; undo brings it back.
    pub(super) async fn delete_report(
        &self,
        actor: &Actor,
        capture_id: String,
        report_id: String,
    ) -> Result<Value, DaemonError> {
        self.change_index(
            actor,
            capture_id,
            report_id,
            "delete_report",
            |entries, id| {
                entries.retain(|e| e.id != id);
                Ok(())
            },
        )
        .await
    }

    /// `edit_report`: the Markdown the user wrote replaces the rendering; the report is marked
    /// edited. One revision.
    pub(super) async fn edit_report(
        &self,
        actor: &Actor,
        capture_id: String,
        report_id: String,
        markdown: String,
    ) -> Result<Value, DaemonError> {
        let _serial = self.capture_locks.lock(&capture_id).await;
        let (library, this, now, by) = (
            self.library.clone(),
            self.library_view(),
            self.clock.now(),
            actor.clone(),
        );
        let out = blocking(move || {
            let mut lib = lock_library(&library);
            let rec = lib.get(&capture_id)?;
            let dir = lib.bundle_path(&rec);
            this.ensure_idle(&rec, &dir, "edit a report of")?;
            let manifest = cm_library::read_manifest(&dir)?;
            let mut entries = store::index_of(&manifest);
            let mut r = store::read(&dir, &entries, &report_id)?;
            r.v = next_version(&dir, &r.id, r.v);
            r.edited = true;
            r.markdown = Some(markdown);
            store::write(&dir, &r)?;
            for e in entries.iter_mut().filter(|e| e.id == r.id) {
                *e = store::entry_of(&r);
            }
            let out = commit_index(&mut lib, &dir, "edit_report", &entries, by, now)?;
            Ok(revision_json(&capture_id, &r.id, &out))
        })
        .await?;
        self.publish_revision(&out);
        Ok(out)
    }

    /// `check_report_item`: ticks or unticks one task item (`section` + `index`). One revision,
    /// so undo and redo restore the tick. Ticking an item that is already ticked changes
    /// nothing and makes no revision.
    pub(super) async fn check_report_item(
        &self,
        actor: &Actor,
        capture_id: String,
        report_id: String,
        section: String,
        index: usize,
        checked: bool,
    ) -> Result<Value, DaemonError> {
        let _serial = self.capture_locks.lock(&capture_id).await;
        let (library, this, now, by) = (
            self.library.clone(),
            self.library_view(),
            self.clock.now(),
            actor.clone(),
        );
        let out = blocking(move || {
            let mut lib = lock_library(&library);
            let rec = lib.get(&capture_id)?;
            let dir = lib.bundle_path(&rec);
            this.ensure_idle(&rec, &dir, "tick an item of a report of")?;
            let manifest = cm_library::read_manifest(&dir)?;
            let mut entries = store::index_of(&manifest);
            let mut r = store::read(&dir, &entries, &report_id)?;
            if !report::is_checkable(&section) {
                return Err(CmError::invalid_field(
                    "section",
                    format!(
                        "`{section}` has no items to tick; use one of {}",
                        report::CHECKABLE_SECTIONS.join(", ")
                    ),
                ));
            }
            let len = r.item_len(&section);
            if index >= len {
                return Err(CmError::invalid_field(
                    "index",
                    format!("`{section}` has {len} item(s); the index counts from 0"),
                ));
            }
            if !r.set_checked(&section, index, checked) {
                return Ok(json!({
                    "capture_id": capture_id, "report_id": r.id, "section": section,
                    "index": index, "checked": checked, "changed": false,
                }));
            }
            r.v = next_version(&dir, &r.id, r.v);
            store::write(&dir, &r)?;
            for e in entries.iter_mut().filter(|e| e.id == r.id) {
                *e = store::entry_of(&r);
            }
            let out = commit_index(&mut lib, &dir, "check_report_item", &entries, by, now)?;
            let mut v = revision_json(&capture_id, &r.id, &out);
            v["section"] = json!(section);
            v["index"] = json!(index);
            v["checked"] = json!(checked);
            v["changed"] = json!(true);
            Ok(v)
        })
        .await?;
        if out["changed"] == json!(true) {
            self.publish_revision(&out);
        }
        Ok(out)
    }

    async fn change_index(
        &self,
        actor: &Actor,
        capture_id: String,
        report_id: String,
        action: &'static str,
        change: impl FnOnce(&mut Vec<store::IndexEntry>, &str) -> Result<(), CmError> + Send + 'static,
    ) -> Result<Value, DaemonError> {
        let _serial = self.capture_locks.lock(&capture_id).await;
        let (library, this, now, by) = (
            self.library.clone(),
            self.library_view(),
            self.clock.now(),
            actor.clone(),
        );
        let out = blocking(move || {
            let mut lib = lock_library(&library);
            let rec = lib.get(&capture_id)?;
            let dir = lib.bundle_path(&rec);
            this.ensure_idle(&rec, &dir, "change the reports of")?;
            let manifest = cm_library::read_manifest(&dir)?;
            let mut entries = store::index_of(&manifest);
            check_report_id(&entries, &report_id)?;
            change(&mut entries, &report_id)?;
            let out = commit_index(&mut lib, &dir, action, &entries, by, now)?;
            Ok(revision_json(&capture_id, &report_id, &out))
        })
        .await?;
        self.publish_revision(&out);
        Ok(out)
    }

    fn publish_revision(&self, out: &Value) {
        self.events.publish(
            events::CAPTURE_REVISED,
            json!({ "capture_id": out["capture_id"], "revision": out["revision"],
                    "action": out["action"], "by": out["by"] }),
        );
    }
}

/// The next version number: above the current one and above every kept file (an undone version
/// is never overwritten, so redo stays possible until the user makes a new change).
fn next_version(dir: &Path, id: &str, current: u32) -> u32 {
    let mut v = current + 1;
    while store::version_path(dir, id, v).exists() {
        v += 1;
    }
    v
}

/// Queues a report unless an equal one (same capture, template and report) is queued, running or
/// paused; then that job is returned (`true` = reused).
fn enqueue_report(
    queue: &JobQueue,
    capture_id: &str,
    template: &str,
    report_id: Option<String>,
    inputs: Value,
) -> Result<(Job, bool), CmError> {
    static ENQUEUE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _one_at_a_time = ENQUEUE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let active = queue.list(&JobFilter {
        job_type: Some(JOB_REPORT.to_owned()),
        states: Some(vec![JobState::Queued, JobState::Running, JobState::Paused]),
        limit: Some(200),
    })?;
    let rid = report_id.map(Value::from).unwrap_or(Value::Null);
    if let Some(j) = active.into_iter().find(|j| {
        j.inputs.get("capture_id").and_then(Value::as_str) == Some(capture_id)
            && j.inputs.get("template").and_then(Value::as_str) == Some(template)
            && j.inputs.get("report_id").cloned().unwrap_or(Value::Null) == rid
    }) {
        return Ok((j, true));
    }
    Ok((queue.enqueue(NewJob::new(JOB_REPORT, inputs))?, false))
}

/// Runs `report` jobs.
pub(super) struct ReportHandler {
    pub(super) inner: Weak<Inner>,
}

#[async_trait]
impl JobHandler for ReportHandler {
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
        let template = ctx
            .job
            .inputs
            .get("template")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        match inner.run_report(&ctx, &capture_id, &template).await {
            Ok(v) => Ok(v),
            Err(e) => {
                let reason = match &e {
                    CmError::JobFailed { reason, .. } => reason.clone(),
                    other => other.code().as_str().to_owned(),
                };
                inner.events.publish(
                    events::REPORT_FAILED,
                    json!({ "capture_id": capture_id, "template": template,
                            "reason": reason, "message": e.to_string() }),
                );
                Err(e)
            }
        }
    }
}

impl Inner {
    async fn run_report(
        self: &Arc<Self>,
        ctx: &JobContext,
        capture_id: &str,
        template_id: &str,
    ) -> Result<Value, CmError> {
        let inputs = &ctx.job.inputs;
        let template = report::find_template(template_id)
            .ok_or_else(|| report::unknown_template(template_id))?;
        let context: ReportContext = inputs
            .get("context")
            .and_then(|c| serde_json::from_value(c.clone()).ok())
            .unwrap_or_default();
        let language = inputs
            .get("language")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let regen = inputs
            .get("report_id")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let by = parse_actor(inputs.get("by").and_then(Value::as_str).unwrap_or("user"));

        let (library, id, captions) = (
            self.library.clone(),
            capture_id.to_owned(),
            self.ai.captions.clone(),
        );
        let source = blocking(move || {
            let lib = lock_library(&library);
            let rec = lib.get(&id)?;
            let dir = lib.bundle_path(&rec);
            if !dir.is_dir() {
                return Err(CmError::not_found(format!(
                    "The recording folder `{}` was moved or deleted",
                    dir.display()
                )));
            }
            let s = load_source(&dir, &id, captions.as_ref())?;
            report::check_enough_speech(&s.lines)?;
            Ok(s)
        })
        .await
        .map_err(into_cm)?;

        // The settings may have changed since the call: check again.
        let config = self.report_gate().map_err(into_cm)?;
        let (me, cfg) = (self.clone(), config.clone());
        let model = blocking(move || me.text_model_client(&cfg))
            .await
            .map_err(into_cm)?;
        let source_language = source.language.clone();
        let effective_language =
            language.filter(|l| l.split('-').next() != source_language.split('-').next());
        let stored_language = effective_language
            .clone()
            .unwrap_or_else(|| source_language.clone());

        let (flag, watcher) = interrupt_flag(ctx);
        let reporter = ctx.reporter.clone();
        let (context2, language2, src_lang) =
            (context.clone(), effective_language, source_language);
        let context_tokens = config.context_tokens as usize;
        let generated = tokio::task::spawn_blocking(move || {
            let input = generate::Input {
                template,
                lines: &source.lines,
                markers: &source.markers,
                context: &context2,
                speakers: &source.speakers,
                language: language2.as_deref(),
                source_language: &src_lang,
                context_tokens,
            };
            let mut last = -1.0_f32;
            generate::generate(model.as_ref(), &input, &mut |p| {
                if p >= 1.0 || p - last >= 0.01 {
                    last = p;
                    reporter.report(
                        f64::from(p),
                        Some(format!("Writing the report {}%", (p * 100.0) as u32)),
                    );
                }
                !flag.load(Ordering::SeqCst)
            })
        })
        .await;
        watcher.abort();
        let generated = match generated {
            Ok(Ok(g)) => g,
            Ok(Err(generate::Error::Cancelled)) => {
                return Err(CmError::internal("The report was interrupted"))
            }
            Ok(Err(generate::Error::Model(e))) => return Err(e.into_cm_error(&ctx.job.id)),
            Err(e) => return Err(CmError::internal(format!("report worker failed: {e}"))),
        };

        // Written only now, after a success: a failed run leaves the old report as it was.
        let _serial = self.capture_locks.lock(capture_id).await;
        let (library, view, now) = (self.library.clone(), self.library_view(), self.clock.now());
        let (cid, model_name, provider) = (
            capture_id.to_owned(),
            config.model.clone(),
            config.provider.as_str().to_owned(),
        );
        let out = blocking(move || {
            let mut lib = lock_library(&library);
            let rec = lib.get(&cid)?;
            let dir = lib.bundle_path(&rec);
            view.ensure_idle(&rec, &dir, "write a report of")?;
            let manifest = cm_library::read_manifest(&dir)?;
            let mut entries = store::index_of(&manifest);
            let created = now_rfc3339();
            // Regenerating replaces the report in place (a new version of the same id), unless
            // it was deleted while the model worked.
            let existing = regen
                .as_deref()
                .and_then(|r| entries.iter().position(|e| e.id == r));
            let (id, v) = match existing {
                Some(i) => (
                    entries[i].id.clone(),
                    next_version(&dir, &entries[i].id, entries[i].v),
                ),
                None => (
                    store::new_id(&entries, &format!("{cid}|{created}|{}", template.id)),
                    1,
                ),
            };
            let r = store::new_report(
                id.clone(),
                v,
                template,
                &model_name,
                &provider,
                &created,
                &stored_language,
                context,
                &generated,
            );
            store::write(&dir, &r)?;
            match existing {
                Some(i) => entries[i] = store::entry_of(&r),
                None => entries.push(store::entry_of(&r)),
            }
            let rev = commit_index(&mut lib, &dir, "generate_report", &entries, by, now)?;
            Ok((r, rev))
        })
        .await
        .map_err(into_cm)?;
        let (r, rev) = out;
        self.events.publish(
            events::CAPTURE_REVISED,
            json!({ "capture_id": capture_id, "revision": rev.revision.rev,
                    "action": rev.revision.action, "by": rev.revision.by }),
        );
        let result = json!({
            "capture_id": capture_id, "report_id": r.id, "template": r.template,
            "dropped_uncited": r.dropped_uncited, "reassigned_names": r.reassigned_names,
            "chunks": r.chunks, "language": r.language, "model": r.model,
            "revision": rev.revision.rev,
        });
        self.events.publish(
            events::REPORT_READY,
            json!({ "capture_id": capture_id, "report_id": r.id, "template": r.template,
                    "dropped_uncited": r.dropped_uncited, "chunks": r.chunks }),
        );
        Ok(result)
    }
}
