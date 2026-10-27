//! The Report tab (redesign V5 and V6, AI-9). Setup: six templates in a 2 x 3 grid (the preset's
//! one marked "suggested"), the context the template uses (title, attendees as chips, agenda;
//! role and competencies for interview notes), the output language and the primary "Write ..."
//! button with the model line and the Local badge. A run shows its progress with Cancel. A report
//! is shown in wide mode: switcher, Edit as Markdown, Regenerate, New, Delete, the meta line (date,
//! model, language, "N items left out"), the sections with a time chip on every cited item that
//! seeks, and the foot (Markdown, TXT, Copy).
//!
//! Only local text models are offered (Ollama or LM Studio on this Mac). Without one the tab says
//! so and points to the AI page; the daemon's `no_text_model` answer is handled the same way.
//! Reports are written by `generate_report`, saved by `export_report`, changed by `edit_report`
//! and `delete_report`; this file sends none of them itself (see `voice_fields.rs`). Not seen by
//! eye yet (`tests/manual/VOC-2.md`).
//!
//! Not built: the left column of V6 (the cited moment with its context lines and a player, the
//! overview strip with citation ticks) and the action-item checkboxes (a ticked item has nowhere
//! to be stored).

use cm_ai::report::store::Report;
use cm_i18n::{tr, tr_ctx, trn};
use gpui::{
    div, prelude::*, px, AnyElement, ClipboardItem, Context, FontWeight, Role, SharedString,
};

use super::speakers_tab::{card, row_icon};
use super::tabs::{micro, panel, Inspector};
use super::voice_fields::ReportNote;
use super::{RecordingEditor, Session};
use crate::ai::jobs::VoiceJobKind;
use crate::ai::voice::ProviderChoice;
use crate::recedit::voice::{language_name, speaker_colour, TRANSLATE_TARGETS};
use crate::recedit::voice_panels::{self as rules, ItemView, SectionView};
use crate::theme::{scale, type_scale, Color, Theme};
use crate::ui::components::{button_with_icon, dot, icon, label_fill, shrink_text, ButtonKind};
use crate::ui::kit::{callout, pill, Tone};
use crate::ui::paint::{rgba_of, TextExt};
use crate::ui::settings::widgets::{segment, segmented, small_button_with_icon};

/// What the Report tab knows about the text model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum TextState {
    /// `get_text_model` has not answered yet: the daemon decides on Write.
    Unknown,
    /// A local model is set up (its name).
    Ready(String),
    /// Provider None, or no model chosen.
    Missing,
}

/// The template's name in the current language.
fn template_name(id: &str, fallback: &str) -> String {
    match id {
        "summary" => tr_ctx!("report template", "Summary"),
        "minutes" => tr_ctx!("report template", "Meeting minutes"),
        "one_on_one" => tr_ctx!("report template", "One-on-one notes"),
        "interview" => tr_ctx!("report template", "Interview notes"),
        "product_recap" => tr_ctx!("report template", "Product recap"),
        "lecture" => tr_ctx!("report template", "Lecture notes"),
        _ => fallback.to_owned(),
    }
}

fn template_glyph(id: &str) -> &'static str {
    match id {
        "summary" => "file-text",
        "minutes" => "list-checks",
        "one_on_one" => "users",
        "interview" => "info",
        "product_recap" => "presentation",
        "lecture" => "graduation-cap",
        _ => "file-text",
    }
}

/// The section's title in the current language.
fn section_title(id: &str, fallback: &str) -> String {
    match id {
        "summary" => tr_ctx!("report section", "Summary"),
        "attendees" => tr_ctx!("report section", "Attendees"),
        "topics" => tr_ctx!("report section", "Topics"),
        "decisions" => tr_ctx!("report section", "Decisions"),
        "action_items" => tr_ctx!("report section", "Action items"),
        "open_questions" => tr_ctx!("report section", "Open questions"),
        "marked_moments" => tr_ctx!("report section", "Marked moments"),
        "updates" => tr_ctx!("report section", "Updates"),
        "wins" => tr_ctx!("report section", "Wins"),
        "concerns" => tr_ctx!("report section", "Concerns"),
        "feedback_given" => tr_ctx!("report section", "Feedback given"),
        "feedback_received" => tr_ctx!("report section", "Feedback received"),
        "agreed_actions" => tr_ctx!("report section", "Agreed actions"),
        "follow_ups" => tr_ctx!("report section", "Follow-ups"),
        "role_candidate" => tr_ctx!("report section", "Role and candidate"),
        "questions_answers" => tr_ctx!("report section", "Questions and answers"),
        "evidence" => tr_ctx!("report section", "Evidence per competency"),
        "candidate_questions" => tr_ctx!("report section", "Candidate\u{2019}s questions"),
        "product_audience" => tr_ctx!("report section", "Product and audience"),
        "features_shown" => tr_ctx!("report section", "Features shown"),
        "questions_asked" => tr_ctx!("report section", "Questions asked"),
        "concerns_raised" => tr_ctx!("report section", "Concerns raised"),
        "next_steps" => tr_ctx!("report section", "Next steps"),
        "key_points" => tr_ctx!("report section", "Key points"),
        "definitions" => tr_ctx!("report section", "Definitions"),
        "examples" => tr_ctx!("report section", "Examples"),
        "review_questions" => tr_ctx!("report section", "Review questions"),
        _ => fallback.to_owned(),
    }
}

/// The label of an item's other field in the current language.
fn field_label(id: &str, fallback: &str) -> String {
    match id {
        "role" => tr_ctx!("report field", "Role"),
        "due" => tr_ctx!("report field", "Due"),
        "key_points" => tr_ctx!("report field", "Key points"),
        "answer" => tr_ctx!("report field", "Answer"),
        "quote" => tr_ctx!("report field", "Quote"),
        "note" => tr_ctx!("report field", "What it shows"),
        "competency" => tr_ctx!("report field", "Competency"),
        "audience" => tr_ctx!("report field", "Audience"),
        "product" => tr_ctx!("report field", "Product"),
        "definition" => tr_ctx!("report field", "Definition"),
        "people" => tr_ctx!("report field", "People"),
        "question" => tr_ctx!("report field", "Question"),
        "topic" => tr_ctx!("report field", "Topic"),
        _ => fallback.to_owned(),
    }
}

/// "Unassigned" as the daemon writes it, in the current language.
fn person_label(name: &str) -> String {
    if name.eq_ignore_ascii_case("unassigned") {
        tr_ctx!("report owner", "Unassigned")
    } else {
        name.to_owned()
    }
}

impl RecordingEditor {
    /// What the tab knows about the text model, from the AI page's state.
    pub(super) fn text_state(&self, cx: &gpui::App) -> TextState {
        let t = &self.app.read(cx).ai.voice.text;
        if !t.loaded {
            TextState::Unknown
        } else if t.provider == ProviderChoice::OnThisMac && !t.model.trim().is_empty() {
            TextState::Ready(t.model.clone())
        } else {
            TextState::Missing
        }
    }

    /// The report the view shows: the chosen one, else the newest.
    fn shown_report<'a>(&self, s: &'a Session) -> Option<&'a Report> {
        let reports = &s.voice.reports;
        self.vp
            .shown
            .as_ref()
            .and_then(|id| reports.iter().find(|r| &r.id == id))
            .or_else(|| reports.last())
    }

    /// The tab shows a report (the wide inspector), not the setup or a run.
    pub(super) fn report_view_open(&self, s: &Session, cx: &gpui::App) -> bool {
        let id = self.id.clone().unwrap_or_default();
        !s.voice.reports.is_empty()
            && !self.vp.setup
            && self
                .app
                .read(cx)
                .transcribe
                .voice_job(&id, VoiceJobKind::Report)
                .is_none()
    }

    pub(super) fn report_panel(
        &self,
        t: &Theme,
        s: &Session,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = self.id.clone().unwrap_or_default();
        let running = self
            .app
            .read(cx)
            .transcribe
            .voice_job(&id, VoiceJobKind::Report)
            .cloned();
        if let Some(job) = running {
            return self.report_running(t, s, job.progress, cx);
        }
        if self.report_view_open(s, cx) {
            if let Some(r) = self.shown_report(s) {
                return self.report_view(t, s, r, cx);
            }
        }
        self.report_setup(t, s, cx)
    }

    // ---- setup ----------------------------------------------------------------------------

    fn report_setup(&self, t: &Theme, s: &Session, cx: &mut Context<Self>) -> AnyElement {
        let id = self.id.clone().unwrap_or_default();
        let text = self.text_state(cx);
        let chosen = self.report_template_id(s);
        let suggested = rules::suggested_template(s.voice.preset.as_deref());
        let template = cm_ai::report::find_template(&chosen);
        let name = template_name(&chosen, template.map_or("", |t| t.name.as_str()));

        let local = pill(
            t,
            Tone::Success,
            Some("laptop"),
            tr_ctx!("report badge", "Local"),
        );
        let mut body: Vec<AnyElement> = Vec::new();
        if !self.ai.has_transcript {
            body.push(
                super::tabs::hint_box(
                    t,
                    "captions",
                    tr!("A report is written from the transcript. Transcribe the recording first."),
                )
                .into_any_element(),
            );
            body.push(crate::ui::ai_view::transcribe_row(
                t, &self.app, &id, "re", cx,
            ));
            return panel(t, Inspector::Report, tr!("New report"), body, Vec::new());
        }
        body.push(div().flex().justify_end().child(local).into_any_element());
        body.push(self.template_grid(t, &chosen, suggested, cx));
        if let Some(n) = template.and_then(|t| t.notes_only.as_deref()) {
            let _ = n;
            body.push(
                callout(
                    t,
                    Tone::Neutral,
                    "info",
                    tr!(
                        "Notes only \u{2014} CaptureMann doesn\u{2019}t score candidates. Using \
                         AI to assess candidates is regulated in some places."
                    ),
                    None,
                    None,
                )
                .into_any_element(),
            );
        }
        if let Some(tpl) = template {
            body.push(self.context_form(t, tpl, cx));
        }
        body.push(self.language_row(t, s, cx));
        // The note of a failed start or run, and what a missing text model means.
        match (&self.vp.report_note, &text) {
            (Some(ReportNote::Failed(msg)), _) => body.push(
                callout(
                    t,
                    Tone::Danger,
                    "circle-alert",
                    tr!("Couldn\u{2019}t write the report"),
                    Some(SharedString::from(msg.clone())),
                    None,
                )
                .into_any_element(),
            ),
            (Some(ReportNote::NoTextModel), _) | (None, TextState::Missing) => {
                body.push(self.no_text_model_card(t))
            }
            _ => {}
        }
        let write = button_with_icon(
            t,
            "re-write-report",
            tr!("Write {name}", name = name.to_lowercase()),
            ButtonKind::Primary,
            Some("sparkles"),
        )
        .w_full();
        let can =
            text != TextState::Missing && self.vp.report_note != Some(ReportNote::NoTextModel);
        let mut foot = vec![div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(if can {
                write
                    .on_click(cx.listener(|this, _, _, cx| this.report_generate(cx)))
                    .into_any_element()
            } else {
                write.opacity(0.45).cursor_default().into_any_element()
            })
            .children(match &text {
                TextState::Ready(model) => Some(
                    div()
                        .text_size(px(type_scale::CAPTION))
                        .text_color(rgba_of(t.mist))
                        .text_center()
                        .child(tr!(
                            "{model} \u{00B7} runs on this Mac",
                            model = model.clone()
                        )),
                ),
                _ => None,
            })
            .into_any_element()];
        if !s.voice.reports.is_empty() {
            foot.insert(
                0,
                small_button_with_icon(t, "re-report-back", &tr!("Back to the report"), None)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.vp.setup = false;
                        cx.notify();
                    }))
                    .into_any_element(),
            );
        }
        panel(
            t,
            Inspector::Report,
            tr!("Every item links to the moment it was said"),
            body,
            vec![div()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(8.))
                .children(foot)
                .into_any_element()],
        )
    }

    /// "No local text model": points to the AI page.
    fn no_text_model_card(&self, t: &Theme) -> AnyElement {
        let app = self.app.clone();
        let action = small_button_with_icon(
            t,
            "re-report-open-ai",
            &tr!("Open AI page"),
            Some("sparkles"),
        )
        .on_click(move |_, _, cx| {
            let app = app.clone();
            cx.defer(move |cx| crate::ui::app::set_route(&app, crate::ui::app::Route::Ai, cx));
        })
        .into_any_element();
        callout(
            t,
            Tone::Brass,
            "triangle-alert",
            tr!("No text model is set up"),
            Some(
                tr!(
                "Reports are written by a text model on this Mac (Ollama or LM Studio). Choose \
                 one on the AI page; nothing is sent anywhere else."
            )
                .into(),
            ),
            Some(action),
        )
        .id("re-no-text-model")
        .into_any_element()
    }

    /// The six templates in a 2 x 3 grid.
    fn template_grid(
        &self,
        t: &Theme,
        chosen: &str,
        suggested: Option<&str>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let cells = cm_ai::report::templates()
            .iter()
            .map(|tpl| {
                let on = tpl.id == chosen;
                let tid = tpl.id.clone();
                let name = template_name(&tpl.id, &tpl.name);
                let hint = suggested == Some(tpl.id.as_str());
                let (fg, focus) = (rgba_of(t.text), rgba_of(t.text));
                div()
                    .id(SharedString::from(format!("re-template-{}", tpl.id)))
                    .role(Role::RadioButton)
                    .aria_label(SharedString::from(name.clone()))
                    .aria_toggled(if on {
                        gpui::Toggled::True
                    } else {
                        gpui::Toggled::False
                    })
                    .focusable()
                    .tab_stop(true)
                    .w(gpui::relative(0.485))
                    .min_h(px(48.))
                    .px(px(12.))
                    .py(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .rounded(px(scale::RADIUS_MD))
                    .border_1()
                    .border_color(rgba_of(if on { t.line } else { t.border }))
                    .bg(rgba_of(if on { t.raised } else { t.surface_2 }))
                    .text_size(px(type_scale::UI))
                    .text_color(if on { fg } else { rgba_of(t.text_2) })
                    .cursor_pointer()
                    .focus_visible(move |s| s.border_color(focus))
                    .child(icon(template_glyph(&tpl.id), 16., t.text_2))
                    // The "suggested" tag sits under the name: beside it the name had no room
                    // and broke inside words.
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .flex()
                            .flex_col()
                            .child(div().min_w(px(0.)).child(name))
                            .children(hint.then(|| {
                                div()
                                    .text_size(px(11.))
                                    .text_color(rgba_of(t.agent_text))
                                    .child(tr_ctx!("report template", "suggested"))
                            })),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.vp.template = Some(tid.clone());
                        cx.notify();
                    }))
            })
            .collect::<Vec<_>>();
        div()
            .id("re-templates")
            .role(Role::RadioGroup)
            .aria_label(tr!("Report template"))
            .flex()
            .flex_wrap()
            .justify_between()
            .gap(px(8.))
            .children(cells)
            .into_any_element()
    }

    /// The context fields the template uses.
    fn context_form(
        &self,
        t: &Theme,
        tpl: &cm_ai::report::Template,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let wants = |f: &str| tpl.context_fields.iter().any(|x| x == f);
        let labelled = |label: String, child: AnyElement| {
            div()
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(micro(t, label))
                .child(child)
        };
        let chips = |items: &[String], which: u8, cx: &mut Context<Self>| {
            div()
                .flex()
                .flex_wrap()
                .gap(px(6.))
                .children(items.iter().enumerate().map(|(i, name)| {
                    small_button_with_icon(
                        t,
                        SharedString::from(format!("re-chip-{which}-{i}")),
                        name,
                        Some("x"),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let list = if which == 0 {
                            &mut this.vp.draft.attendees
                        } else {
                            &mut this.vp.draft.competencies
                        };
                        if i < list.len() {
                            list.remove(i);
                        }
                        cx.notify();
                    }))
                }))
        };
        let mut col = div().flex().flex_col().gap(px(12.)).child(
            div()
                .text_size(px(type_scale::CAPTION))
                .text_color(rgba_of(t.mist))
                .child(tr!("Context (optional, makes owners and names right)")),
        );
        if wants("title") {
            col = col.child(labelled(
                tr!("Title"),
                self.vp.title.clone().into_any_element(),
            ));
        }
        if wants("role") {
            col = col.child(labelled(
                tr!("Role"),
                self.vp.role.clone().into_any_element(),
            ));
        }
        if wants("competencies") {
            col = col.child(labelled(
                tr!("Competencies"),
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(chips(&self.vp.draft.competencies, 1, cx))
                    .child(self.vp.competency.clone())
                    .into_any_element(),
            ));
        }
        if wants("attendees") {
            col = col.child(labelled(
                tr!("Attendees"),
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(chips(&self.vp.draft.attendees, 0, cx))
                    .child(self.vp.attendee.clone())
                    .into_any_element(),
            ));
        }
        if wants("agenda") {
            col = col.child(labelled(
                tr!("Agenda"),
                self.vp.agenda.clone().into_any_element(),
            ));
        }
        col.into_any_element()
    }

    /// "Write in  Same as transcript (English)": the languages open as chips.
    fn language_row(&self, t: &Theme, s: &Session, cx: &mut Context<Self>) -> AnyElement {
        let own = self.transcript_language();
        let same = if own.is_empty() {
            tr!("Same as the transcript")
        } else {
            tr!(
                "Same as the transcript ({language})",
                language = language_name(&own)
            )
        };
        let current = self
            .vp
            .language
            .map(language_name)
            .unwrap_or_else(|| same.clone());
        let toggle = small_button_with_icon(
            t,
            "re-report-language",
            &current,
            Some(if self.vp.language_open {
                "chevron-down"
            } else {
                "languages"
            }),
        )
        .on_click(cx.listener(|this, _, _, cx| {
            this.vp.language_open = !this.vp.language_open;
            cx.notify();
        }));
        let _ = s;
        let mut col = div().flex().flex_col().gap(px(8.)).child(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(10.))
                .child(
                    shrink_text(tr!("Write in"))
                        .text_size(px(type_scale::UI))
                        .text_color(rgba_of(t.text_2)),
                )
                .child(toggle),
        );
        if self.vp.language_open {
            let mut seg = segmented(t, "re-report-languages", &tr!("Report language")).child(
                segment(t, "re-report-lang-same", &same, self.vp.language.is_none()).on_click(
                    cx.listener(|this, _, _, cx| {
                        this.vp.language = None;
                        this.vp.language_open = false;
                        cx.notify();
                    }),
                ),
            );
            for code in TRANSLATE_TARGETS {
                seg = seg.child(
                    segment(
                        t,
                        SharedString::from(format!("re-report-lang-{code}")),
                        &language_name(code),
                        self.vp.language == Some(code),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.vp.language = Some(code);
                        this.vp.language_open = false;
                        cx.notify();
                    })),
                );
            }
            col = col.child(seg);
        }
        col.into_any_element()
    }

    /// A run: the progress card and Cancel.
    fn report_running(
        &self,
        t: &Theme,
        _s: &Session,
        progress: Option<f32>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pct = progress.map(|p| (p * 100.0).round() as u32);
        let head = match pct {
            Some(p) => tr!("Writing the report\u{2026} {percent}%", percent = p),
            None => tr!("Writing the report\u{2026}"),
        };
        let bar = div().h(px(4.)).rounded(px(2.)).bg(rgba_of(t.raised)).child(
            div()
                .h_full()
                .w(gpui::relative(progress.unwrap_or(0.05).clamp(0.0, 1.0)))
                .rounded(px(2.))
                .bg(rgba_of(t.text)),
        );
        let body = vec![card(t)
            .id("re-report-progress")
            .p(px(14.))
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(icon("loader-circle", 16., t.text_2))
                    .child(label_fill(head).text_size(px(type_scale::UI))),
            )
            .child(bar)
            .child(
                div()
                    .text_size(px(type_scale::CAPTION))
                    .text_color(rgba_of(t.mist))
                    .child(tr!(
                        "Long recordings are read in parts, then merged. You can keep editing."
                    )),
            )
            .into_any_element()];
        let cancel = button_with_icon(
            t,
            "re-report-cancel",
            tr_ctx!("button", "Cancel"),
            ButtonKind::Secondary,
            None,
        )
        .w_full()
        .on_click(cx.listener(|this, _, _, cx| this.report_cancel(cx)))
        .into_any_element();
        panel(
            t,
            Inspector::Report,
            tr!("Every item links to the moment it was said"),
            body,
            vec![cancel],
        )
    }

    // ---- a report -------------------------------------------------------------------------

    fn report_view(
        &self,
        t: &Theme,
        s: &Session,
        r: &Report,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let rid = r.id.clone();
        let sections = rules::sections(r);
        let name = template_name(&r.template, &r.template_name);
        let n_reports = s.voice.reports.len();

        // ---- header: the switcher and the actions
        let switcher = small_button_with_icon(
            t,
            "re-report-switcher",
            &name,
            Some(template_glyph(&r.template)),
        )
        .h(px(36.))
        .on_click(cx.listener(|this, _, _, cx| {
            this.vp.reports_open = !this.vp.reports_open;
            cx.notify();
        }));
        let edit_md = r.markdown_text();
        let edit = row_icon(t, "re-report-edit", "pen-line", tr!("Edit as Markdown")).on_click({
            let md = r.markdown.clone().unwrap_or(edit_md);
            cx.listener(move |this, _, _, cx| this.report_start_edit(&md, cx))
        });
        let regen =
            row_icon(t, "re-report-regenerate", "refresh-cw", tr!("Regenerate")).on_click({
                let (rid, tpl) = (rid.clone(), r.template.clone());
                cx.listener(move |this, _, _, cx| {
                    // Same template and language; the context is the form's.
                    this.vp.regenerating = Some(rid.clone());
                    this.vp.template = Some(tpl.clone());
                    this.report_generate(cx);
                })
            });
        let new = row_icon(t, "re-report-new", "plus", tr!("New report")).on_click(cx.listener(
            |this, _, _, cx| {
                this.vp.setup = true;
                this.vp.regenerating = None;
                this.vp.editing = false;
                this.vp.template = None;
                cx.notify();
            },
        ));
        let del = row_icon(t, "re-report-delete", "trash-2", tr!("Delete the report")).on_click(
            cx.listener(|this, _, _, cx| {
                this.vp.confirm_delete = !this.vp.confirm_delete;
                cx.notify();
            }),
        );
        let header = div()
            .flex_none()
            .px(px(20.))
            .pt(px(20.))
            .pb(px(12.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .border_b_1()
            .border_color(rgba_of(t.row_divider()))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(8.))
                    .child(switcher)
                    .child(
                        div()
                            .text_size(px(type_scale::CAPTION))
                            .text_color(rgba_of(t.mist))
                            .child(trn!("{n} report", "{n} reports", n_reports)),
                    )
                    .child(div().flex_1())
                    .child(edit)
                    .child(regen)
                    .child(new)
                    .child(del),
            )
            .child(self.report_meta(t, r))
            .children(self.vp.reports_open.then(|| self.report_list(t, s, r, cx)))
            .children(self.vp.confirm_delete.then(|| {
                let rid = rid.clone();
                callout(
                    t,
                    Tone::Danger,
                    "trash-2",
                    tr!("Delete this report?"),
                    Some(tr!("Undo brings it back.").into()),
                    Some(
                        div()
                            .flex()
                            .gap(px(8.))
                            .child(
                                small_button_with_icon(
                                    t,
                                    "re-report-delete-cancel",
                                    &tr_ctx!("button", "Cancel"),
                                    None,
                                )
                                .on_click(cx.listener(
                                    |this, _, _, cx| {
                                        this.vp.confirm_delete = false;
                                        cx.notify();
                                    },
                                )),
                            )
                            .child(
                                small_button_with_icon(
                                    t,
                                    "re-report-delete-yes",
                                    &tr_ctx!("button", "Delete"),
                                    Some("trash-2"),
                                )
                                .on_click(cx.listener(
                                    move |this, _, _, cx| this.report_delete(rid.clone(), cx),
                                )),
                            )
                            .into_any_element(),
                    ),
                )
            }));

        // ---- the body: the sections, or the Markdown being edited
        let body: AnyElement = if self.vp.editing {
            div()
                .flex()
                .flex_col()
                .gap(px(10.))
                .child(self.vp.markdown.clone())
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap(px(8.))
                        .child(
                            small_button_with_icon(
                                t,
                                "re-report-edit-cancel",
                                &tr_ctx!("button", "Cancel"),
                                None,
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.vp.editing = false;
                                cx.notify();
                            })),
                        )
                        .child(
                            small_button_with_icon(
                                t,
                                "re-report-edit-save",
                                &tr_ctx!("button", "Save"),
                                Some("save"),
                            )
                            .on_click({
                                let rid = rid.clone();
                                cx.listener(move |this, _, _, cx| {
                                    this.report_save_edit(rid.clone(), cx)
                                })
                            }),
                        ),
                )
                .into_any_element()
        } else if sections.is_empty() {
            super::tabs::hint_box(
                t,
                "info",
                tr!("This report was made with a template this version doesn\u{2019}t know."),
            )
            .into_any_element()
        } else {
            div()
                .flex()
                .flex_col()
                .gap(px(20.))
                .children(
                    sections
                        .iter()
                        .map(|sec| self.section_view(t, s, r, sec, cx)),
                )
                .into_any_element()
        };

        // ---- the foot
        let markdown = r.markdown_text();
        let shown_md = r.markdown.clone().unwrap_or(markdown);
        let foot = div()
            .flex_none()
            .px(px(20.))
            .py(px(12.))
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.))
            .border_t_1()
            .border_color(rgba_of(t.row_divider()))
            .child(
                shrink_text(tr!("Save a copy"))
                    .text_size(px(type_scale::CAPTION))
                    .text_color(rgba_of(t.mist)),
            )
            .child(
                small_button_with_icon(t, "re-report-md", &tr!("Markdown"), Some("download"))
                    .mono()
                    .on_click({
                        let rid = rid.clone();
                        cx.listener(move |this, _, _, cx| this.report_export(rid.clone(), "md", cx))
                    }),
            )
            .child(
                small_button_with_icon(t, "re-report-txt", &tr!("TXT"), Some("download"))
                    .mono()
                    .on_click({
                        let rid = rid.clone();
                        cx.listener(move |this, _, _, cx| {
                            this.report_export(rid.clone(), "txt", cx)
                        })
                    }),
            )
            .child(div().flex_1())
            .child(
                small_button_with_icon(t, "re-report-copy", &tr!("Copy"), Some("copy")).on_click(
                    move |_, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(shown_md.clone()))
                    },
                ),
            );

        div()
            .id("re-panel-re-tab-report")
            .role(Role::Group)
            .aria_label(tr!("{title} panel", title = Inspector::Report.label()))
            .flex_1()
            .min_w(px(0.))
            .h_full()
            .flex()
            .flex_col()
            .rounded(px(scale::RADIUS_XL))
            .border_1()
            .border_color(rgba_of(t.border))
            .bg(rgba_of(t.surface))
            .overflow_hidden()
            .child(header)
            .child(
                div()
                    .id("re-report-body")
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .px(px(20.))
                    .py(px(16.))
                    .child(body),
            )
            .child(foot)
            .into_any_element()
    }

    /// "Today 10:51 . model . language . N items left out".
    fn report_meta(&self, t: &Theme, r: &Report) -> AnyElement {
        let mut parts: Vec<String> = vec![rules::created_day(&r.created)];
        if !r.model.is_empty() {
            parts.push(if r.provider == "local" {
                tr!("{model} \u{00B7} on this Mac", model = r.model.clone())
            } else {
                r.model.clone()
            });
        }
        if !r.language.is_empty() {
            parts.push(language_name(&r.language));
        }
        if r.edited {
            parts.push(tr_ctx!("report", "edited"));
        }
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(10.))
            .text_size(px(type_scale::CAPTION))
            .text_color(rgba_of(t.mist))
            .child(shrink_text(parts.join(" \u{00B7} ")))
            .children((r.dropped_uncited > 0).then(|| {
                shrink_text(trn!(
                    "{n} item left out: no matching moment",
                    "{n} items left out: no matching moment",
                    r.dropped_uncited
                ))
                .text_color(rgba_of(t.warning))
            }))
            .into_any_element()
    }

    /// The reports of the recording, to switch between.
    fn report_list(
        &self,
        t: &Theme,
        s: &Session,
        shown: &Report,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let rows = s
            .voice
            .reports
            .iter()
            .rev()
            .map(|r| {
                let rid = r.id.clone();
                let label = format!(
                    "{} \u{00B7} {}",
                    template_name(&r.template, &r.template_name),
                    rules::created_day(&r.created)
                );
                segment(
                    t,
                    SharedString::from(format!("re-report-pick-{}", r.id)),
                    &label,
                    r.id == shown.id,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.vp.shown = Some(rid.clone());
                    this.vp.reports_open = false;
                    this.vp.editing = false;
                    this.vp.confirm_delete = false;
                    cx.notify();
                }))
            })
            .collect::<Vec<_>>();
        segmented(t, "re-report-list", &tr!("Reports of this recording"))
            .children(rows)
            .into_any_element()
    }

    /// One section: its title and its items.
    fn section_view(
        &self,
        t: &Theme,
        s: &Session,
        r: &Report,
        sec: &SectionView,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let title = section_title(&sec.id, &sec.title).to_uppercase();
        let head = div()
            .text_size(px(12.))
            .font_weight(FontWeight::MEDIUM)
            .text_color(rgba_of(t.mist))
            .child(SharedString::from(title));
        let owner = rules::has_owner(&r.template, &sec.id);
        let items: Vec<AnyElement> = if sec.items.is_empty() {
            vec![div()
                .text_size(px(13.))
                .text_color(rgba_of(t.mist))
                .child(tr!("Nothing found"))
                .into_any_element()]
        } else if sec.from_markers {
            vec![div()
                .flex()
                .flex_wrap()
                .gap(px(8.))
                .children(
                    sec.items
                        .iter()
                        .enumerate()
                        .map(|(i, it)| self.moment_chip(t, &sec.id, i, it, cx)),
                )
                .into_any_element()]
        } else {
            sec.items
                .iter()
                .enumerate()
                .map(|(i, it)| self.item_row(t, s, &sec.id, i, it, owner, cx))
                .collect()
        };
        div()
            .id(SharedString::from(format!("re-section-{}", sec.id)))
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(head)
            .children(items)
            .into_any_element()
    }

    /// A marked moment: bookmark, label and its time.
    fn moment_chip(
        &self,
        t: &Theme,
        sec: &str,
        i: usize,
        it: &ItemView,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let at = it.at.first().copied();
        let chip = div()
            .id(SharedString::from(format!("re-moment-{sec}-{i}")))
            .role(Role::Button)
            .aria_label(SharedString::from(it.head.clone()))
            .px(px(10.))
            .py(px(6.))
            .flex()
            .items_center()
            .gap(px(8.))
            .rounded(px(scale::RADIUS_MD))
            .border_1()
            .border_color(rgba_of(t.border))
            .bg(rgba_of(t.ground))
            .text_size(px(13.5))
            .cursor_pointer()
            .child(icon("bookmark", 14., t.text_3))
            .child(shrink_text(it.head.clone()))
            .children(at.map(|a| {
                div()
                    .mono()
                    .tnum()
                    .text_size(px(12.))
                    .text_color(rgba_of(t.mist))
                    .child(SharedString::from(rules::at_label(a)))
            }));
        match at {
            Some(a) => chip
                .on_click(cx.listener(move |this, _, _, cx| this.voice_seek(rules::at_time(a), cx)))
                .into_any_element(),
            None => chip.into_any_element(),
        }
    }

    /// One item: the text, its other fields, the person and a time chip for every citation.
    #[allow(clippy::too_many_arguments)]
    fn item_row(
        &self,
        t: &Theme,
        s: &Session,
        sec: &str,
        i: usize,
        it: &ItemView,
        owner: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let person = it.person.as_ref().map(|name| {
            let colour: Option<Color> = s
                .voice
                .speakers
                .iter()
                .position(|sp| sp.name.eq_ignore_ascii_case(name))
                .map(speaker_colour);
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap(px(6.))
                .text_size(px(13.))
                .text_color(rgba_of(if owner && colour.is_none() {
                    t.mist
                } else {
                    t.text_2
                }))
                .child(dot(colour.unwrap_or(t.line), 10.))
                .child(SharedString::from(person_label(name)))
        });
        let extra = it
            .extra
            .iter()
            .map(|(id, label, value)| {
                SharedString::from(format!("{}: {value}", field_label(id, label)))
            })
            .collect::<Vec<_>>();
        let chips = it.at.iter().enumerate().map(|(k, &a)| {
            div()
                .id(SharedString::from(format!("re-cite-{sec}-{i}-{k}")))
                .role(Role::Button)
                .aria_label(SharedString::from(tr!(
                    "Jump to {time}",
                    time = rules::at_label(a)
                )))
                .flex_none()
                .h(px(26.))
                .px(px(8.))
                .flex()
                .items_center()
                .gap(px(4.))
                .rounded(px(scale::RADIUS_SM))
                .border_1()
                .border_color(rgba_of(t.line))
                .cursor_pointer()
                .hover({
                    let c = rgba_of(t.raised);
                    move |s| s.bg(c)
                })
                .child(icon("play", 11., t.text_2))
                .child(
                    div()
                        .mono()
                        .tnum()
                        .text_size(px(12.))
                        .text_color(rgba_of(t.text_2))
                        .child(SharedString::from(rules::at_label(a))),
                )
                .on_click(cx.listener(move |this, _, _, cx| this.voice_seek(rules::at_time(a), cx)))
        });
        div()
            .id(SharedString::from(format!("re-item-{sec}-{i}")))
            .role(Role::ListItem)
            .flex()
            .items_start()
            .gap(px(10.))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .text_size(px(15.))
                            .line_height(px(22.))
                            .child(SharedString::from(it.head.clone())),
                    )
                    .children(extra.into_iter().map(|e| {
                        div()
                            .text_size(px(13.))
                            .text_color(rgba_of(t.text_3))
                            .child(e)
                    })),
            )
            .children(person)
            .child(div().flex().flex_wrap().gap(px(4.)).children(chips))
            .into_any_element()
    }
}
