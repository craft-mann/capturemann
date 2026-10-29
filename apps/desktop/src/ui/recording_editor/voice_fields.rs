//! What the Speakers and Report tabs keep between frames (redesign V5 and V6) and the calls they
//! send. The rules are in `crate::recedit::voice_panels` (unit tested); the panels are drawn in
//! `speakers_tab.rs` and `report_tab.rs`.
//!
//! Every change goes through the daemon as the user: `diarize`, `set_speakers`, `generate_report`,
//! `export_report`, `delete_report` and `edit_report`, so the CLI, MCP and the undo stack see the
//! same thing. Nothing here touches an engine.

use gpui::{AppContext, Context, Entity, Subscription, Window};

use cm_i18n::{tr, tr_ctx};

use super::{RecordingEditor, Session};
use crate::ai::jobs::VoiceJobKind;
use crate::recedit::voice_panels::{self as rules, Draft, NameError};
use crate::ui::text_field::{CaretAt, TextField, TextFieldEvent};

/// How long "Cancel" on a report waits for the job to stop before the card is cleared anyway.
const CANCEL_WAIT_TRIES: u32 = 40;
const CANCEL_WAIT_STEP: std::time::Duration = std::time::Duration::from_millis(250);

/// Which field an event came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FieldId {
    Rename,
    Attendee,
    Competency,
}

/// What the Speakers tab says after a run or a failed start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum SpeakerNote {
    /// The speaker model is not installed (the daemon answered `no_speaker_model`).
    NoModel,
    Failed(String),
}

/// What the Report tab says after a failed start or run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ReportNote {
    /// No local text model is set up (`no_text_model`).
    NoTextModel,
    Failed(String),
}

/// The state of both tabs.
pub(super) struct VoicePanels {
    // ---- the fields
    pub rename: Entity<TextField>,
    pub title: Entity<TextField>,
    pub attendee: Entity<TextField>,
    pub agenda: Entity<TextField>,
    pub role: Entity<TextField>,
    pub competency: Entity<TextField>,
    pub markdown: Entity<TextField>,
    // ---- speakers
    /// The speaker being renamed.
    pub renaming: Option<u32>,
    pub rename_error: Option<NameError>,
    /// "Merge into...": the speaker being merged away and the one chosen.
    pub merging: Option<(u32, Option<u32>)>,
    /// "How many people?" (`None`: not sure).
    pub people: Option<u32>,
    /// The user picked an answer (even "Not sure"); until then the preset's default applies
    /// (`rules::default_people`).
    pub people_chosen: bool,
    /// "My microphone only hears me".
    pub headset: bool,
    pub speaker_note: Option<SpeakerNote>,
    // ---- report
    /// The template chosen for the next report (`None`: the suggested one).
    pub template: Option<String>,
    pub draft: Draft,
    /// The language of the note text (`None`: the transcript's).
    pub language: Option<&'static str>,
    pub language_open: bool,
    /// The setup form is shown although the recording has reports ("New").
    pub setup: bool,
    /// The report shown (`None`: the newest).
    pub shown: Option<String>,
    /// Editing the Markdown of the shown report.
    pub editing: bool,
    pub confirm_delete: bool,
    pub reports_open: bool,
    pub report_note: Option<ReportNote>,
    /// `get_text_model` was read for this editor.
    pub text_asked: bool,
    /// The report to regenerate when "Regenerate" started the run.
    pub regenerating: Option<String>,
    /// The Translate panel's chosen language.
    pub translate_pick: Option<&'static str>,
    /// "Include citation times" under the report: Markdown, TXT and Copy print `[mm:ss]` after
    /// the items. Off by default (the report view always shows them as chips).
    pub include_times: bool,
    _subs: Vec<Subscription>,
}

impl VoicePanels {
    pub(super) fn new(cx: &mut Context<RecordingEditor>) -> VoicePanels {
        let line = |label: String, hint: String, cx: &mut Context<RecordingEditor>| {
            cx.new(|cx| TextField::new(label, hint, cx).without_icon().height(36.))
        };
        let rename = cx.new(|cx| {
            TextField::new(tr!("Speaker name"), tr!("Name"), cx)
                .without_icon()
                .height(32.)
        });
        let title = line(tr!("Title"), tr!("Name of the meeting"), cx);
        let attendee = line(
            tr!("Add an attendee"),
            tr!("Add a name and press Enter"),
            cx,
        );
        let role = line(tr!("Role"), tr!("The role the interview is for"), cx);
        let competency = line(
            tr!("Add a competency"),
            tr!("Add a competency and press Enter"),
            cx,
        );
        let agenda = cx.new(|cx| {
            TextField::new(tr!("Agenda"), tr!("Topics, one per line"), cx)
                .without_icon()
                .multiline(2, 5)
        });
        let markdown = cx.new(|cx| {
            TextField::new(tr!("Report Markdown"), tr!("Markdown"), cx)
                .without_icon()
                .multiline(12, 24)
        });
        let sub = |id: FieldId, f: &Entity<TextField>, cx: &mut Context<RecordingEditor>| {
            cx.subscribe(f, move |this, _f, ev: &TextFieldEvent, cx| {
                this.voice_field_event(id, ev, cx)
            })
        };
        let subs = vec![
            sub(FieldId::Rename, &rename, cx),
            sub(FieldId::Attendee, &attendee, cx),
            sub(FieldId::Competency, &competency, cx),
        ];
        VoicePanels {
            rename,
            title,
            attendee,
            agenda,
            role,
            competency,
            markdown,
            renaming: None,
            rename_error: None,
            merging: None,
            people: None,
            people_chosen: false,
            headset: false,
            speaker_note: None,
            template: None,
            draft: Draft::default(),
            language: None,
            language_open: false,
            include_times: false,
            setup: false,
            shown: None,
            editing: false,
            confirm_delete: false,
            reports_open: false,
            report_note: None,
            text_asked: false,
            regenerating: None,
            translate_pick: None,
            _subs: subs,
        }
    }

    /// A different recording opened: nothing typed or chosen carries over.
    pub(super) fn reset(&mut self, cx: &mut Context<RecordingEditor>) {
        for f in [
            &self.rename,
            &self.title,
            &self.attendee,
            &self.agenda,
            &self.role,
            &self.competency,
            &self.markdown,
        ] {
            f.update(cx, |f, cx| f.clear(cx));
        }
        self.renaming = None;
        self.rename_error = None;
        self.merging = None;
        self.people = None;
        self.people_chosen = false;
        self.headset = false;
        self.speaker_note = None;
        self.template = None;
        self.draft = Draft::default();
        self.language = None;
        self.language_open = false;
        self.setup = false;
        self.shown = None;
        self.editing = false;
        self.confirm_delete = false;
        self.reports_open = false;
        self.report_note = None;
        self.regenerating = None;
        self.translate_pick = None;
    }

    /// One of these fields has the keyboard.
    pub(super) fn field_focused(&self, window: &Window, cx: &gpui::App) -> bool {
        [
            &self.rename,
            &self.title,
            &self.attendee,
            &self.agenda,
            &self.role,
            &self.competency,
            &self.markdown,
        ]
        .iter()
        .any(|f| f.read(cx).focus.is_focused(window))
    }
}

/// The template the form has chosen: the user's pick, else the preset's, else Summary.
pub(super) fn pick_template(chosen: Option<&str>, preset: Option<&str>) -> String {
    chosen
        .map(str::to_owned)
        .or_else(|| rules::suggested_template(preset).map(str::to_owned))
        .unwrap_or_else(|| "summary".to_owned())
}

/// The reason of a failed call or job, as a note for the Speakers tab.
pub(super) fn speaker_note_of(reason: &str, message: &str) -> SpeakerNote {
    if reason == "no_speaker_model" {
        SpeakerNote::NoModel
    } else {
        SpeakerNote::Failed(message.to_owned())
    }
}

/// The reason of a failed call or job, as a note for the Report tab.
pub(super) fn report_note_of(reason: &str, message: &str) -> ReportNote {
    if reason == "no_text_model" {
        ReportNote::NoTextModel
    } else if reason == "engine_missing" {
        ReportNote::Failed(tr!(
            "The built-in text engine is not part of this copy of CaptureMann. Reinstall CaptureMann, or use a build that includes the engine."
        ))
    } else {
        ReportNote::Failed(message.to_owned())
    }
}

impl RecordingEditor {
    /// The jobs that ended since the last frame, and the first read of the text model.
    pub(super) fn voice_jobs_sync(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.id.clone() else { return };
        let ended = self
            .app
            .update(cx, |m, _| m.transcribe.take_voice_ended(&id));
        for (kind, result) in ended {
            match (kind, result) {
                (VoiceJobKind::Speakers, Err(why)) => {
                    self.vp.speaker_note = Some(speaker_note_of(&why, &why));
                }
                (VoiceJobKind::Report, Err(why)) => {
                    self.vp.report_note = Some(report_note_of(&why, &why));
                }
                (VoiceJobKind::Speakers, Ok(())) => self.vp.speaker_note = None,
                (VoiceJobKind::Report, Ok(())) => {
                    self.vp.report_note = None;
                    self.vp.setup = false;
                    self.vp.shown = None;
                    self.vp.regenerating = None;
                    self.vp.editing = false;
                }
            }
            self.voice_ui.stale = true;
        }
        // The Report tab and the Translate panel need to know whether a text model is set up.
        if self.voice_ui.translate_open {
            // Whether Whisper has a model that can translate (read once, if nobody has yet).
            crate::ui::ai::ensure_loaded(&self.app, cx);
        }
        let needs_text = self.tab == super::tabs::Inspector::Report || self.voice_ui.translate_open;
        if needs_text && !self.vp.text_asked {
            self.vp.text_asked = true;
            if !self.app.read(cx).ai.voice.text.loaded {
                crate::ui::ai_voice::refresh_text_model(&self.app, cx);
            }
        }
    }

    // ---- fields ---------------------------------------------------------------------------

    pub(super) fn voice_field_event(
        &mut self,
        which: FieldId,
        ev: &TextFieldEvent,
        cx: &mut Context<Self>,
    ) {
        match (which, ev) {
            (FieldId::Rename, TextFieldEvent::Submitted) => {
                if let Some(id) = self.vp.renaming {
                    self.speaker_commit_rename(id, cx);
                }
            }
            (FieldId::Rename, TextFieldEvent::Escaped) => {
                self.vp.renaming = None;
                self.vp.rename_error = None;
                cx.notify();
            }
            (FieldId::Rename, TextFieldEvent::Changed(_)) => {
                self.vp.rename_error = None;
                cx.notify();
            }
            (FieldId::Attendee, TextFieldEvent::Submitted) => {
                let text = self.vp.attendee.read(cx).text().to_owned();
                if rules::add_chip(
                    &mut self.vp.draft.attendees,
                    &text,
                    cm_ai::report::MAX_ATTENDEES,
                    cm_ai::report::MAX_NAME_CHARS,
                ) {
                    self.vp.attendee.update(cx, |f, cx| f.clear(cx));
                }
                cx.notify();
            }
            (FieldId::Competency, TextFieldEvent::Submitted) => {
                let text = self.vp.competency.read(cx).text().to_owned();
                if rules::add_chip(
                    &mut self.vp.draft.competencies,
                    &text,
                    cm_ai::report::MAX_COMPETENCIES,
                    cm_ai::report::MAX_COMPETENCY_CHARS,
                ) {
                    self.vp.competency.update(cx, |f, cx| f.clear(cx));
                }
                cx.notify();
            }
            _ => {}
        }
    }

    // ---- speakers -------------------------------------------------------------------------

    /// "Find speakers": `diarize` with the answers of the tab.
    pub(super) fn speakers_find(&mut self, cx: &mut Context<Self>) {
        let (Some(id), Some(client)) = (self.id.clone(), crate::ui::ai::client_of(&self.app, cx))
        else {
            return;
        };
        if !self.app.update(cx, |m, _| {
            m.transcribe.voice_job_begin(&id, VoiceJobKind::Speakers)
        }) {
            return;
        }
        self.vp.speaker_note = None;
        let (preset, stored) = self
            .session()
            .map(|s| (s.voice.preset.clone(), s.voice.people))
            .unwrap_or_default();
        let people = rules::effective_people(
            self.vp.people_chosen,
            self.vp.people,
            stored,
            preset.as_deref(),
        );
        let headset = self.vp.headset;
        let app = self.app.clone();
        cx.notify();
        cx.spawn(async move |this, cx| {
            // "5+" asks for at least five; the other answers are exact.
            let (exact, at_least) = match people {
                Some(n) if n >= 5 => (None, Some(n)),
                other => (other, None),
            };
            let res = client.diarize(&id, exact, at_least, headset).await;
            this.update(cx, |this, cx| {
                app.update(cx, |m, _| match &res {
                    Ok(job) => m
                        .transcribe
                        .voice_job_adopt(&id, VoiceJobKind::Speakers, job),
                    Err(_) => m.transcribe.voice_job_abort(&id, VoiceJobKind::Speakers),
                });
                if let Err(e) = res {
                    let reason = if e.is_no_speaker_model() {
                        "no_speaker_model"
                    } else {
                        ""
                    };
                    this.vp.speaker_note = Some(speaker_note_of(reason, &e.message));
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// The pencil: the row turns into a field holding the name.
    pub(super) fn speaker_start_rename(&mut self, id: u32, cx: &mut Context<Self>) {
        let Some(s) = self.session() else { return };
        let name = s.voice.name_of(id);
        self.vp.renaming = Some(id);
        self.vp.rename_error = None;
        self.vp.merging = None;
        self.vp
            .rename
            .update(cx, |f, cx| f.load(&name, CaretAt::End, cx));
        cx.notify();
    }

    /// Enter in the rename field: `set_speakers { rename }`.
    pub(super) fn speaker_commit_rename(&mut self, id: u32, cx: &mut Context<Self>) {
        let typed = self.vp.rename.read(cx).text().to_owned();
        let Some(s) = self.session() else { return };
        match rules::checked_name(&typed, id, &s.voice.speakers) {
            Err(e) => {
                self.vp.rename_error = Some(e);
                cx.notify();
            }
            Ok(None) => {
                self.vp.renaming = None;
                cx.notify();
            }
            Ok(Some(name)) => {
                self.vp.renaming = None;
                self.vp.rename_error = None;
                self.speakers_apply(vec![(id, name)], Vec::new(), cx);
            }
        }
    }

    /// "Merge": `set_speakers { merge }`.
    pub(super) fn speaker_commit_merge(&mut self, cx: &mut Context<Self>) {
        if let Some((from, Some(into))) = self.vp.merging.take() {
            self.speakers_apply(Vec::new(), vec![(from, into)], cx);
        }
        cx.notify();
    }

    fn speakers_apply(
        &mut self,
        rename: Vec<(u32, String)>,
        merge: Vec<(u32, u32)>,
        cx: &mut Context<Self>,
    ) {
        let (Some(id), Some(client)) = (self.id.clone(), crate::ui::ai::client_of(&self.app, cx))
        else {
            return;
        };
        let app = self.app.clone();
        cx.spawn(async move |this, cx| {
            let res = client.set_speakers(&id, &rename, &merge).await;
            this.update(cx, |this, cx| {
                // The names and labels changed in the bundle (and the undo stack): read them again.
                this.voice_ui.stale = true;
                this.ai.reload = true;
                if let Err(e) = res {
                    let msg = e.message.clone();
                    cx.defer(move |cx| {
                        crate::ui::app::push_toast(
                            &app,
                            crate::flows::error_toast_live(
                                || tr!("Couldn\u{2019}t change the speakers"),
                                msg.clone(),
                            ),
                            cx,
                        )
                    });
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// "Play sample": the speaker's longest turn, from its start to its end.
    pub(super) fn speaker_play_sample(&mut self, id: u32, cx: &mut Context<Self>) {
        let Some(s) = self.session() else { return };
        let Some(seg) = s
            .voice
            .segments
            .iter()
            .filter(|g| g.speaker == id)
            .max_by_key(|g| g.end.ticks() - g.start.ticks())
        else {
            return;
        };
        let (start, end) = (seg.start, seg.end);
        s.model.seek(start);
        s.model.playing = true;
        s.pacer.stop();
        // `usize::MAX` is no caption line: only the stop at `end` applies.
        self.ai.line_play = Some(crate::ai::caption_view::LinePlay {
            line: usize::MAX,
            end,
        });
        cx.notify();
    }

    // ---- reports --------------------------------------------------------------------------

    /// The template the setup form shows chosen.
    pub(super) fn report_template_id(&self, s: &Session) -> String {
        pick_template(self.vp.template.as_deref(), s.voice.preset.as_deref())
    }

    /// "Write ...": `generate_report` with the template, the context and the language.
    pub(super) fn report_generate(&mut self, cx: &mut Context<Self>) {
        let (Some(id), Some(client)) = (self.id.clone(), crate::ui::ai::client_of(&self.app, cx))
        else {
            return;
        };
        let preset = self.session().and_then(|s| s.voice.preset.clone());
        let template = pick_template(self.vp.template.as_deref(), preset.as_deref());
        let Some(t) = cm_ai::report::find_template(&template) else {
            return;
        };
        if !self.app.update(cx, |m, _| {
            m.transcribe.voice_job_begin(&id, VoiceJobKind::Report)
        }) {
            return;
        }
        let mut draft = self.vp.draft.clone();
        draft.title = self.vp.title.read(cx).text().to_owned();
        draft.agenda = self.vp.agenda.read(cx).text().to_owned();
        draft.role = self.vp.role.read(cx).text().to_owned();
        let context = rules::context_json(t, &draft);
        let language = self.vp.language;
        let replace = self.vp.regenerating.clone();
        self.vp.report_note = None;
        let app = self.app.clone();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let res = client
                .generate_report(&id, &template, language, context, replace.as_deref())
                .await;
            this.update(cx, |this, cx| {
                app.update(cx, |m, _| match &res {
                    Ok(job) => m.transcribe.voice_job_adopt(&id, VoiceJobKind::Report, job),
                    Err(_) => m.transcribe.voice_job_abort(&id, VoiceJobKind::Report),
                });
                if let Err(e) = res {
                    let reason = if e.is_no_text_model() {
                        "no_text_model"
                    } else {
                        ""
                    };
                    this.vp.report_note = Some(report_note_of(reason, &e.message));
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// "Cancel" while a report is being written: `cancel_job`.
    pub(super) fn report_cancel(&mut self, cx: &mut Context<Self>) {
        let (Some(id), Some(client)) = (self.id.clone(), crate::ui::ai::client_of(&self.app, cx))
        else {
            return;
        };
        let Some(job) = self
            .app
            .read(cx)
            .transcribe
            .voice_job_id(&id, VoiceJobKind::Report)
        else {
            return;
        };
        if job.starts_with("pending:") {
            return;
        }
        // The daemon sends no event for a cancelled job (a running handler stops a moment after
        // `cancel_job` answers), so the card would stay at "Writing the report" for ever: wait until
        // the job has stopped, then drop it from the running set.
        let (app, capture) = (self.app.clone(), id);
        cx.spawn(async move |_this, cx| {
            let _ = client.cancel_job(&job).await;
            for _ in 0..CANCEL_WAIT_TRIES {
                match client.job_state(&job).await.as_deref() {
                    Some("queued" | "running") => {
                        cx.background_executor().timer(CANCEL_WAIT_STEP).await;
                    }
                    _ => break,
                }
            }
            cx.update(|cx| {
                app.update(cx, |m, cx| {
                    m.transcribe.voice_ready(&capture, VoiceJobKind::Report);
                    cx.notify();
                })
            });
        })
        .detach();
    }

    /// Save a copy: `export_report` as Markdown or text; the toast says where it went.
    pub(super) fn report_export(
        &mut self,
        report_id: String,
        format: &'static str,
        cx: &mut Context<Self>,
    ) {
        let (Some(id), Some(client)) = (self.id.clone(), crate::ui::ai::client_of(&self.app, cx))
        else {
            return;
        };
        let app = self.app.clone();
        let include_times = self.vp.include_times;
        cx.spawn(async move |_this, cx| {
            let res = client
                .export_report(&id, &report_id, format, include_times)
                .await;
            cx.update(|cx| {
                let spec = match res {
                    Ok(path) => crate::toast::ToastSpec::new(
                        crate::toast::ToastKind::Success,
                        tr!("Report saved"),
                    )
                    .body(path.display().to_string())
                    .button(
                        tr_ctx!("button", "Show in Finder"),
                        crate::toast::ToastAction::RevealPath(path.display().to_string()),
                    ),
                    Err(e) => crate::flows::error_toast_live(
                        || tr!("Couldn\u{2019}t save the report"),
                        e.message.clone(),
                    ),
                };
                crate::ui::app::push_toast(&app, spec, cx);
            });
        })
        .detach();
    }

    /// Delete: `delete_report` (one undo step).
    pub(super) fn report_delete(&mut self, report_id: String, cx: &mut Context<Self>) {
        let (Some(id), Some(client)) = (self.id.clone(), crate::ui::ai::client_of(&self.app, cx))
        else {
            return;
        };
        self.vp.confirm_delete = false;
        self.vp.shown = None;
        self.vp.editing = false;
        let app = self.app.clone();
        cx.spawn(async move |this, cx| {
            let res = client.delete_report(&id, &report_id).await;
            this.update(cx, |this, cx| {
                this.voice_ui.stale = true;
                if let Err(e) = res {
                    let msg = e.message.clone();
                    cx.defer(move |cx| {
                        crate::ui::app::push_toast(
                            &app,
                            crate::flows::error_toast_live(
                                || tr!("Couldn\u{2019}t delete the report"),
                                msg.clone(),
                            ),
                            cx,
                        )
                    });
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// Edit as Markdown: the field takes the report's text.
    pub(super) fn report_start_edit(&mut self, markdown: &str, cx: &mut Context<Self>) {
        self.vp.editing = true;
        self.vp.confirm_delete = false;
        self.vp
            .markdown
            .update(cx, |f, cx| f.load(markdown, CaretAt::Start, cx));
        cx.notify();
    }

    /// Save the edit: `edit_report`.
    pub(super) fn report_save_edit(&mut self, report_id: String, cx: &mut Context<Self>) {
        let (Some(id), Some(client)) = (self.id.clone(), crate::ui::ai::client_of(&self.app, cx))
        else {
            return;
        };
        let text = self.vp.markdown.read(cx).text().to_owned();
        if text.trim().is_empty() {
            return;
        }
        self.vp.editing = false;
        let app = self.app.clone();
        cx.spawn(async move |this, cx| {
            let res = client.edit_report(&id, &report_id, &text).await;
            this.update(cx, |this, cx| {
                this.voice_ui.stale = true;
                if let Err(e) = res {
                    let msg = e.message.clone();
                    cx.defer(move |cx| {
                        crate::ui::app::push_toast(
                            &app,
                            crate::flows::error_toast_live(
                                || tr!("Couldn\u{2019}t save the report"),
                                msg.clone(),
                            ),
                            cx,
                        )
                    });
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }
}
