//! The AI page (M5): the speech-model manager and the AI switches, in its own sidebar section.
//! Everything the user may do with a model is a button on its row; nothing is downloaded unless
//! the user presses Download and then confirms. The state and the rules are in `crate::ai`.

use gpui::{
    div, prelude::*, px, AnyElement, Context, Entity, FontWeight, Render, Role, SharedString,
    Subscription, Window,
};

use cm_i18n::{tr, tr_ctx};

mod voice;
use voice::VoiceField;

use crate::ai::glossary::{heard_text, remove, row_hint, upsert, GlossaryError};
use crate::ai::ignore::IgnoreError;
use crate::ui::text_field::{CaretAt, TextField, TextFieldEvent};

use crate::ai::models::{
    notice_body, notice_title, privacy_copy, ModelRow, NoticeSite, RowAction, RowStatus,
};
use crate::theme::{scale, type_scale, Theme};
use crate::ui::ai;
use crate::ui::app::AppModel;
use crate::ui::components::{button_with_icon, card, icon, shrink_text, switch, ButtonKind};
use crate::ui::kit;
use crate::ui::paint::{rgba_of, TextExt};
use crate::ui::settings::widgets::{
    caption, list_card, segment, segmented, settings_row, small_button, small_button_with_icon,
};
use crate::ui::studio::ThemeGlobal;

/// The placeholder of the name box: the stored default.
fn voice_default_name() -> &'static str {
    crate::ai::voice::DEFAULT_DISPLAY_NAME
}

pub struct AiPage {
    app: Entity<AppModel>,
    /// The glossary form: `None` closed, `Some(None)` adding, `Some(Some(i))` editing entry `i`.
    glossary_edit: Option<Option<usize>>,
    glossary_error: Option<GlossaryError>,
    /// The ignore list's box (AI-1.AC13) and the message for a phrase that was not added.
    ignore_field: Entity<TextField>,
    ignore_error: Option<IgnoreError>,
    term_field: Entity<TextField>,
    heard_field: Entity<TextField>,
    /// The voice sections' box (V8): your name.
    name_field: Entity<TextField>,
    /// What the name box was last loaded with (`None`: never).
    synced: Option<String>,
    /// The "Languages you speak" box (`ai.languages`), what it was last loaded with, and the
    /// message for a code that is not a language.
    lang_field: Entity<TextField>,
    lang_synced: Option<String>,
    lang_error: Option<String>,
    _subs: Vec<Subscription>,
}

impl AiPage {
    pub fn new(app: Entity<AppModel>, cx: &mut Context<Self>) -> Self {
        let term_field = cx
            .new(|cx| TextField::new(tr!("Glossary term"), tr!("Google Drive"), cx).without_icon());
        let heard_field = cx.new(|cx| {
            TextField::new(tr!("Often heard as"), tr!("Logo Drive, Gu gồ Drai"), cx).without_icon()
        });
        let ignore_field = cx.new(|cx| {
            TextField::new(
                tr!("Phrase to leave out"),
                tr!("Part of a line, any capitals or accents"),
                cx,
            )
            .without_icon()
        });
        let lang_field = cx
            .new(|cx| TextField::new(tr!("Languages you speak"), tr!("vi, en"), cx).without_icon());
        let me = voice_default_name();
        let name_field =
            cx.new(|cx| TextField::new(tr!("Your name in transcripts"), me, cx).without_icon());
        let voice_subs =
            vec![
                cx.subscribe(&name_field, move |this, _f, ev: &TextFieldEvent, cx| {
                    this.voice_field_event(VoiceField::Name, ev, cx)
                }),
            ];
        let mut subs = vec![
            cx.observe(&app, |_, _, cx| cx.notify()),
            cx.subscribe(&lang_field, |this, _f, ev: &TextFieldEvent, cx| {
                if matches!(ev, TextFieldEvent::Submitted) {
                    this.languages_save(cx);
                }
            }),
            cx.subscribe(&ignore_field, |this, _f, ev: &TextFieldEvent, cx| {
                if matches!(ev, TextFieldEvent::Submitted) {
                    this.ignore_add(cx);
                }
            }),
            cx.subscribe(&term_field, |this, _f, ev: &TextFieldEvent, cx| {
                this.glossary_field_event(ev, cx)
            }),
            cx.subscribe(&heard_field, |this, _f, ev: &TextFieldEvent, cx| {
                this.glossary_field_event(ev, cx)
            }),
        ];
        subs.extend(voice_subs);
        AiPage {
            app,
            name_field,
            synced: None,
            lang_field,
            lang_synced: None,
            lang_error: None,
            glossary_edit: None,
            glossary_error: None,
            ignore_field,
            ignore_error: None,
            term_field,
            heard_field,
            _subs: subs,
        }
    }

    // ---- the languages you speak (`ai.languages`) -----------------------------------------

    /// Puts the saved languages into the box the first time and whenever they change from
    /// outside; typing alone never moves it.
    fn languages_sync(&mut self, cx: &mut Context<Self>) {
        let saved = self.app.read(cx).ai.languages.clone();
        if self.lang_synced.as_deref() != Some(saved.as_str()) {
            let shown = saved.replace(',', ", ");
            self.lang_field
                .update(cx, |f, cx| f.load(&shown, CaretAt::End, cx));
            self.lang_synced = Some(saved);
        }
    }

    /// Enter or Save: cleans the codes, saves them, or says which one is not a language.
    fn languages_save(&mut self, cx: &mut Context<Self>) {
        let text = self.lang_field.read(cx).text().to_owned();
        match ai::set_languages(&self.app, &text, cx) {
            Ok(_) => self.lang_error = None,
            Err(e) => self.lang_error = Some(e),
        }
        cx.notify();
    }

    /// The row under "Transcribe after each recording".
    fn languages_row(&self, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let saved = self.app.read(cx).ai.languages.clone();
        let typed = self.lang_field.read(cx).text().to_owned();
        let dirty = crate::ai::models::canonical_languages(&typed).map_or(true, |c| c != saved);
        let mut control = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .child(div().w(px(180.)).min_w_0().child(self.lang_field.clone()));
        if dirty {
            control = control.child(
                small_button(t, "ai-languages-save", &tr!("Save"))
                    .on_click(cx.listener(|this, _, _, cx| this.languages_save(cx))),
            );
        }
        let hint = match &self.lang_error {
            Some(e) => e.clone(),
            None => tr!("Helps pick the right language for short recordings, for example vi, en"),
        };
        settings_row(
            t,
            Some("languages"),
            tr!("Languages you speak"),
            Some(SharedString::from(hint)),
            Some(control.into_any_element()),
            true,
        )
        .into_any_element()
    }

    // ---- the glossary (AI-6a) --------------------------------------------------------------

    /// Opens the form for a new term (`None`) or for entry `i`, with the keyboard in the first box.
    fn glossary_open(&mut self, at: Option<usize>, window: &mut Window, cx: &mut Context<Self>) {
        let entry = at.and_then(|i| self.app.read(cx).ai.glossary.get(i).cloned());
        let (term, heard) = entry
            .as_ref()
            .map(|e| (e.term.clone(), heard_text(e)))
            .unwrap_or_default();
        self.term_field
            .update(cx, |f, cx| f.load(&term, CaretAt::End, cx));
        self.heard_field
            .update(cx, |f, cx| f.load(&heard, CaretAt::End, cx));
        self.glossary_edit = Some(at);
        self.glossary_error = None;
        window.focus(&self.term_field.read(cx).focus.clone(), cx);
        cx.notify();
    }

    fn glossary_cancel(&mut self, cx: &mut Context<Self>) {
        self.glossary_edit = None;
        self.glossary_error = None;
        cx.notify();
    }

    /// Enter saves, Escape closes the form.
    fn glossary_field_event(&mut self, ev: &TextFieldEvent, cx: &mut Context<Self>) {
        match ev {
            TextFieldEvent::Submitted => self.glossary_save(cx),
            TextFieldEvent::Escaped => self.glossary_cancel(cx),
            TextFieldEvent::Changed(_) => {}
        }
    }

    fn glossary_save(&mut self, cx: &mut Context<Self>) {
        let Some(at) = self.glossary_edit else { return };
        let term = self.term_field.read(cx).text().to_owned();
        let heard = self.heard_field.read(cx).text().to_owned();
        let mut list = self.app.read(cx).ai.glossary.clone();
        match upsert(&mut list, at, &term, &heard) {
            Ok(()) => {
                self.glossary_edit = None;
                self.glossary_error = None;
                ai::set_glossary(&self.app, list, cx);
            }
            Err(e) => self.glossary_error = Some(e),
        }
        cx.notify();
    }

    fn glossary_remove(&mut self, i: usize, cx: &mut Context<Self>) {
        let mut list = self.app.read(cx).ai.glossary.clone();
        if remove(&mut list, i) {
            // The form may be open on an entry that moved or is gone.
            if self.glossary_edit == Some(Some(i)) {
                self.glossary_edit = None;
            }
            ai::set_glossary(&self.app, list, cx);
        }
        cx.notify();
    }

    // ---- the lines to leave out (AI-1.AC13) -------------------------------------------------

    fn ignore_add(&mut self, cx: &mut Context<Self>) {
        let text = self.ignore_field.read(cx).text().to_owned();
        let mut list = self.app.read(cx).ai.ignore_phrases.clone();
        match crate::ai::ignore::add(&mut list, &text) {
            Ok(()) => {
                self.ignore_error = None;
                self.ignore_field
                    .update(cx, |f, cx| f.load("", CaretAt::End, cx));
                ai::set_ignore_phrases(&self.app, list, cx);
            }
            Err(e) => self.ignore_error = Some(e),
        }
        cx.notify();
    }

    fn ignore_remove(&mut self, i: usize, cx: &mut Context<Self>) {
        let mut list = self.app.read(cx).ai.ignore_phrases.clone();
        if crate::ai::ignore::remove(&mut list, i) {
            ai::set_ignore_phrases(&self.app, list, cx);
        }
        cx.notify();
    }

    /// The "Lines to leave out" section: your own list, empty until you add a phrase.
    fn ignore_section(&self, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let phrases = self.app.read(cx).ai.ignore_phrases.clone();
        let mut rows = list_card(t)
            .id("ai-ignore")
            .role(Role::List)
            .aria_label(SharedString::from(tr!("Lines to leave out")));
        if phrases.is_empty() {
            rows = rows.child(div().px(px(18.)).py(px(16.)).child(caption(
                t,
                tr!("No phrases yet. Nothing is left out unless you add one."),
            )));
        }
        for (i, p) in phrases.iter().enumerate() {
            let rm = small_button_with_icon(
                t,
                SharedString::from(format!("ai-ignore-remove-{i}")),
                &tr_ctx!("ignored phrase", "Remove"),
                Some("trash-2"),
            )
            .aria_label(SharedString::from(tr!(
                "Remove {phrase}",
                phrase = p.clone()
            )))
            .on_click(cx.listener(move |this, _, _, cx| this.ignore_remove(i, cx)));
            rows = rows.child(
                settings_row(
                    t,
                    Some("captions"),
                    p.clone(),
                    None,
                    Some(rm.into_any_element()),
                    i + 1 == phrases.len(),
                )
                .id(SharedString::from(format!("ai-ignore-{i}")))
                .role(Role::ListItem)
                .aria_label(SharedString::from(p.clone())),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .text_size(px(type_scale::BODY))
                            .font_weight(FontWeight::MEDIUM)
                            .child(tr!("Lines to leave out")),
                    )
                    .child(caption(
                        t,
                        tr!(
                            "Lines of a new transcript that contain one of these phrases are left out and counted. Capitals, accents and punctuation do not matter."
                        ),
                    )),
            )
            .child(rows)
            .child(self.ignore_field.clone())
            .children(self.ignore_error.map(|e| {
                div()
                    .text_size(px(type_scale::CAPTION))
                    .text_color(rgba_of(t.warning))
                    .child(SharedString::from(e.text()))
            }))
            .child(
                div().flex().child(
                    small_button_with_icon(t, "ai-ignore-add", &tr!("Add a phrase"), Some("plus"))
                        .on_click(cx.listener(|this, _, _, cx| this.ignore_add(cx))),
                ),
            )
            .into_any_element()
    }

    /// The Glossary section: what it is for, the entries, and the add or edit form.
    fn glossary_section(&self, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let entries = self.app.read(cx).ai.glossary.clone();
        let mut rows = list_card(t)
            .id("ai-glossary")
            .role(Role::List)
            .aria_label(SharedString::from(tr!("Glossary")));
        if entries.is_empty() {
            rows = rows.child(div().px(px(18.)).py(px(16.)).child(caption(
                t,
                tr!("No terms yet. Add a name or a word you want spelled your way."),
            )));
        }
        for (i, e) in entries.iter().enumerate() {
            let edit = small_button_with_icon(
                t,
                SharedString::from(format!("ai-gloss-edit-{i}")),
                &tr_ctx!("glossary entry", "Edit"),
                Some("pencil"),
            )
            .aria_label(SharedString::from(tr!(
                "Edit {term}",
                term = e.term.clone()
            )))
            .on_click(
                cx.listener(move |this, _, window, cx| this.glossary_open(Some(i), window, cx)),
            );
            let rm = small_button_with_icon(
                t,
                SharedString::from(format!("ai-gloss-remove-{i}")),
                &tr_ctx!("glossary entry", "Remove"),
                Some("trash-2"),
            )
            .aria_label(SharedString::from(tr!(
                "Remove {term}",
                term = e.term.clone()
            )))
            .on_click(cx.listener(move |this, _, _, cx| this.glossary_remove(i, cx)));
            rows = rows.child(
                settings_row(
                    t,
                    Some("captions"),
                    e.term.clone(),
                    Some(SharedString::from(row_hint(e))),
                    Some(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(px(8.))
                            .child(edit)
                            .child(rm)
                            .into_any_element(),
                    ),
                    i + 1 == entries.len(),
                )
                .id(SharedString::from(format!("ai-gloss-{i}")))
                .role(Role::ListItem)
                .aria_label(SharedString::from(e.term.clone())),
            );
        }
        let form: Option<AnyElement> = self.glossary_edit.map(|at| {
            card(t)
                .id("ai-gloss-form")
                .role(Role::Group)
                .aria_label(SharedString::from(if at.is_some() {
                    tr!("Edit the glossary entry")
                } else {
                    tr!("New glossary entry")
                }))
                .flex()
                .flex_col()
                .gap(px(10.))
                .child(caption(t, tr!("Term, spelled the way you want it")))
                .child(self.term_field.clone())
                .child(caption(
                    t,
                    tr!("Often heard as (optional, separate with commas)"),
                ))
                .child(self.heard_field.clone())
                .children(self.glossary_error.map(|e| {
                    div()
                        .text_size(px(type_scale::CAPTION))
                        .text_color(rgba_of(t.warning))
                        .child(SharedString::from(e.text()))
                }))
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap(px(8.))
                        .child(
                            button_with_icon(
                                t,
                                "ai-gloss-save",
                                tr!("Save"),
                                ButtonKind::Primary,
                                Some("check"),
                            )
                            .on_click(cx.listener(|this, _, _, cx| this.glossary_save(cx))),
                        )
                        .child(
                            small_button(t, "ai-gloss-cancel", &tr!("Cancel"))
                                .on_click(cx.listener(|this, _, _, cx| this.glossary_cancel(cx))),
                        ),
                )
                .into_any_element()
        });
        div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .text_size(px(type_scale::BODY))
                            .font_weight(FontWeight::MEDIUM)
                            .child(tr!("Glossary")),
                    )
                    .child(caption(
                        t,
                        tr!(
                            "Names and terms CaptureMann should spell your way in new transcripts."
                        ),
                    )),
            )
            .child(rows)
            .children(form)
            .children(self.glossary_edit.is_none().then(|| {
                div().flex().child(
                    small_button_with_icon(t, "ai-gloss-add", &tr!("Add a term"), Some("plus"))
                        .on_click(
                            cx.listener(|this, _, window, cx| this.glossary_open(None, window, cx)),
                        ),
                )
            }))
            .into_any_element()
    }
}

/// A thin progress bar (4 px) for a download.
pub fn progress_bar(t: &Theme, label: &str, fraction: f32) -> AnyElement {
    div()
        .id("ai-progress")
        .role(Role::ProgressIndicator)
        .aria_label(SharedString::from(label.to_owned()))
        .w_full()
        .h(px(4.))
        .rounded(px(2.))
        .bg(rgba_of(t.line))
        .child(
            div()
                .h_full()
                .w(gpui::relative(fraction.clamp(0.0, 1.0)))
                .rounded(px(2.))
                .bg(rgba_of(t.text)),
        )
        .into_any_element()
}

/// The quiet notice: an icon, two lines and the buttons given. Used by the AI page (with
/// Dismiss) and, in a thinner form, by the library.
pub fn notice_card(t: &Theme, actions: Vec<AnyElement>) -> gpui::Div {
    card(t)
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(14.))
        .child(icon("bot", 20., t.text_2))
        .child(
            div()
                .flex_1()
                .min_w(px(220.))
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(
                    div()
                        .text_size(px(type_scale::BODY))
                        .font_weight(FontWeight::MEDIUM)
                        .child(notice_title()),
                )
                .child(caption(t, notice_body())),
        )
        .children(actions)
}

impl AiPage {
    fn banner(&self, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let app = self.app.clone();
        notice_card(
            t,
            vec![
                small_button(t, "ai-notice-dismiss", &tr_ctx!("notice", "Dismiss"))
                    .on_click(cx.listener(move |_, _, _, cx| ai::dismiss_notice(&app, cx)))
                    .into_any_element(),
            ],
        )
        .into_any_element()
    }

    /// One model as a card of the grid: icon tile, name and size, description, the status with
    /// its icon, the progress (while downloading or paused) and the actions. Idle cards have a
    /// fixed minimum height; the confirm and downloading states grow it.
    fn model_card(&self, t: &Theme, row: &ModelRow, cx: &mut Context<Self>) -> AnyElement {
        let active = matches!(row.status, RowStatus::Active);
        let (glyph, tone) = status_glyph(&row.status);
        let tc = kit::tone_colors(t, tone);
        let status_color = match row.status {
            RowStatus::Active | RowStatus::Failed(_) => tc.fg,
            _ => t.mist,
        };
        let mut actions: Vec<AnyElement> = Vec::new();
        for a in &row.actions {
            let (app, id, action) = (self.app.clone(), row.id.clone(), a.clone());
            let label = a.label(row.size_mb);
            let element_id = SharedString::from(format!("ai-{}-{:?}", row.id, a));
            let handler =
                cx.listener(move |_, _, _, cx| ai::row_action(&app, &id, action.clone(), cx));
            let glyph = action_icon(a);
            actions.push(if a.is_primary() {
                button_with_icon(t, element_id, label, ButtonKind::Primary, glyph)
                    .on_click(handler)
                    .into_any_element()
            } else {
                small_button_with_icon(t, element_id, &label, glyph)
                    .on_click(handler)
                    .into_any_element()
            });
        }
        card(t)
            .id(SharedString::from(format!("ai-model-{}", row.id)))
            .role(Role::ListItem)
            .aria_label(SharedString::from(tr!(
                "{name}, {size}, {status}",
                name = row.label,
                size = row.size,
                status = row.status.text()
            )))
            .flex_1()
            .min_w(px(CARD_MIN_W))
            .min_h(px(CARD_MIN_H))
            .flex()
            .flex_col()
            .gap(px(12.))
            .when(active, |d| d.border_color(rgba_of(t.success)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(
                        div()
                            .size(px(40.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(scale::RADIUS_MD))
                            .bg(rgba_of(t.icon_tile()))
                            .child(icon(
                                "audio-lines",
                                20.,
                                if active { t.success } else { t.text_2 },
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(px(type_scale::TITLE))
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(SharedString::from(row.label.clone())),
                            )
                            .child(
                                div()
                                    .text_size(px(type_scale::CAPTION))
                                    .text_color(rgba_of(t.mist))
                                    .mono()
                                    .child(SharedString::from(row.size.clone())),
                            )
                            .children((!row.memory.is_empty()).then(|| {
                                div()
                                    .text_size(px(type_scale::CAPTION))
                                    .text_color(rgba_of(t.mist))
                                    .child(SharedString::from(row.memory.clone()))
                            })),
                    ),
            )
            .child(
                div()
                    .text_size(px(13.))
                    .text_color(rgba_of(t.text_3))
                    .child(SharedString::from(row.hint.clone())),
            )
            .child(
                div()
                    .mt_auto()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .text_size(px(13.))
                    .text_color(rgba_of(status_color))
                    .child(icon(glyph, 14., tc.icon))
                    .child(shrink_text(row.status.text())),
            )
            .children(row.progress.as_ref().map(|p| {
                let f = match row.status {
                    RowStatus::Downloading(f) | RowStatus::Paused(f) => f,
                    _ => 0.0,
                };
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(progress_bar(t, &tr!("Download progress"), f))
                    .child(caption(t, p.clone()))
            }))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(8.))
                    .children(actions),
            )
            .into_any_element()
    }
}

/// Minimum width and idle height of a model card, px.
const CARD_MIN_W: f32 = 232.;
const CARD_MIN_H: f32 = 236.;

/// The status glyph and tone of a model card.
pub fn status_glyph(s: &RowStatus) -> (&'static str, kit::Tone) {
    use kit::Tone;
    match s {
        RowStatus::NotInstalled | RowStatus::Paused(_) => ("circle-dashed", Tone::Neutral),
        RowStatus::Installed => ("circle-check", Tone::Neutral),
        RowStatus::Active => ("circle-check", Tone::Success),
        RowStatus::Confirming => ("download", Tone::Neutral),
        RowStatus::Downloading(_) => ("loader-circle", Tone::Neutral),
        RowStatus::Failed(_) => ("triangle-alert", Tone::Brass),
        RowStatus::ConfirmingRemove => ("trash-2", Tone::Neutral),
    }
}

/// The leading icon of an action button; the Cancel buttons have none.
pub fn action_icon(a: &RowAction) -> Option<&'static str> {
    match a {
        RowAction::Download | RowAction::ConfirmDownload => Some("download"),
        RowAction::Import => Some("file-input"),
        RowAction::Select => Some("check"),
        RowAction::StopUsing => Some("x"),
        RowAction::Remove | RowAction::ConfirmRemove => Some("trash-2"),
        RowAction::Resume => Some("rotate-cw"),
        RowAction::Cancel | RowAction::CancelConfirm | RowAction::CancelRemove => None,
    }
}

impl Render for AiPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.global::<ThemeGlobal>().0;
        ai::ensure_loaded(&self.app, cx);
        crate::ui::ai_voice::ensure_voice_loaded(&self.app, cx);
        self.voice_sync_fields(cx);
        self.languages_sync(cx);
        let (rows, show_banner, load_error, action_error, busy, active) = {
            let m = self.app.read(cx);
            (
                m.ai.rows(),
                m.ai.notice_visible(NoticeSite::AiPage),
                m.ai.load_error.clone(),
                m.ai.action_error.clone(),
                m.ai.busy,
                m.ai.active().map(|a| a.label.clone()),
            )
        };
        let model_cards: Vec<AnyElement> =
            rows.iter().map(|r| self.model_card(&t, r, cx)).collect();
        let app = self.app.clone();
        let current = {
            let m = self.app.read(cx);
            crate::ai::models::AutoMode::from_flags(m.ai.auto_transcribe, m.ai.live_transcribe)
        };
        let auto_choice = {
            let mut seg = segmented(&t, "ai-auto-mode", &tr!("Automatic transcription"));
            for mode in crate::ai::models::AutoMode::ALL {
                let app = app.clone();
                seg = seg.child(
                    segment(
                        &t,
                        SharedString::from(format!("ai-auto-{}", mode.key())),
                        &mode.label(),
                        current == mode,
                    )
                    .on_click(cx.listener(move |_, _, _, cx| ai::set_auto_mode(&app, mode, cx))),
                );
            }
            seg
        };
        // "Transcribe from": the stored `ai.transcribe_raw_mic`, or the daemon's default (the
        // original microphone) while nothing is stored. Only a click writes the key.
        let raw_mic = self
            .app
            .read(cx)
            .ai
            .transcribe_raw_mic
            .unwrap_or(cm_ai::pipeline::PipelineSettings::default().transcribe_raw_mic);
        let source_choice = {
            let mut seg = segmented(&t, "ai-transcribe-source", &tr!("Transcribe from"));
            for (raw, id, label) in [
                (true, "ai-source-original", tr!("Original microphone")),
                (false, "ai-source-reduced", tr!("Noise-reduced copy")),
            ] {
                let app = self.app.clone();
                seg = seg.child(
                    segment(&t, id, &label, raw_mic == raw).on_click(cx.listener(
                        move |_, _, _, cx| {
                            if raw_mic != raw {
                                set_transcribe_source(&app, raw, cx);
                            }
                        },
                    )),
                );
            }
            seg
        };
        let cuts_on = !self.app.read(cx).ai.auto_suggest_cuts_off;
        let cuts_switch = switch(
            &t,
            "ai-auto-cuts",
            tr!("Suggest cuts when the editor opens"),
            cuts_on,
            true,
        )
        .on_click({
            let app = self.app.clone();
            cx.listener(move |_, _, _, cx| ai::set_auto_suggest_cuts(&app, !cuts_on, cx))
        });
        let drop_on = !self.app.read(cx).ai.drop_unsupported_off;
        let drop_switch = switch(
            &t,
            "ai-drop-unsupported",
            tr!("Leave out lines the audio does not hold"),
            drop_on,
            true,
        )
        .on_click({
            let app = self.app.clone();
            cx.listener(move |_, _, _, cx| ai::set_drop_unsupported(&app, !drop_on, cx))
        });
        let ignore = self.ignore_section(&t, cx);
        let glossary = self.glossary_section(&t, cx);
        let speakers = self.speakers_section(&t, cx);
        let text_model = self.text_model_section(&t, window, cx);
        let models: AnyElement = if let Some(e) = &load_error {
            card(&t)
                .flex()
                .flex_col()
                .gap(px(8.))
                .child(
                    div()
                        .font_weight(FontWeight::MEDIUM)
                        .child(tr!("Models are not available")),
                )
                .child(caption(&t, e.text()))
                .child(small_button(&t, "ai-retry", &tr!("Try again")).on_click({
                    let app = self.app.clone();
                    cx.listener(move |_, _, _, cx| {
                        app.update(cx, |m, _| m.ai.load_error = None);
                        ai::refresh(&app, cx);
                    })
                }))
                .into_any_element()
        } else if rows.is_empty() {
            card(&t)
                .child(caption(&t, tr!("Reading the model list\u{2026}")))
                .into_any_element()
        } else {
            div()
                .id("ai-model-grid")
                .role(Role::List)
                .aria_label(SharedString::from(tr!("Speech models")))
                .flex()
                .flex_wrap()
                .gap(px(16.))
                .children(model_cards)
                .into_any_element()
        };
        div().id("ai-page").size_full().overflow_y_scroll().child(
            div()
                .max_w(px(1280.))
                .px(px(40.))
                .pt(px(32.))
                .pb(px(48.))
                .flex()
                .flex_col()
                .gap(px(24.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(16.))
                        .child(
                            div()
                                .size(px(52.))
                                .flex_none()
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(scale::RADIUS_LG))
                                .border_1()
                                .border_color(rgba_of(t.line))
                                .bg(rgba_of(t.icon_tile()))
                                .child(icon("sparkles", 24., t.text)),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(4.))
                                .child(
                                    div()
                                        .text_size(px(type_scale::H1))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(tr!("AI models")),
                                )
                                .child(
                                    div()
                                        .text_size(px(type_scale::UI))
                                        .text_color(rgba_of(t.text_3))
                                        .child(tr!(
                                            "Transcripts, speakers, translations and reports, made on {this_computer}.",
                                            this_computer = crate::platform_text::this_computer()
                                        )),
                                ),
                        ),
                )
                .children(show_banner.then(|| self.banner(&t, cx)))
                .child(kit::callout(
                    &t,
                    kit::Tone::Success,
                    "lock",
                    privacy_copy(),
                    None,
                    None,
                ))
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_start()
                        .gap(px(24.))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(voice::COLUMN_MIN_W))
                                .flex()
                                .flex_col()
                                .gap(px(24.))
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(12.))
                                        .child(voice::section_title(
                                            &t,
                                            tr!("Speech to text"),
                                            tr!("Whisper \u{b7} transcripts, captions, translation to English"),
                                        ))
                                        .child(models)
                                        .children(action_error.map(|e| caption(&t, e.text())))
                                        .children(busy.then(|| caption(&t, tr!("Working\u{2026}")))),
                                )
                                .child(
                                    list_card(&t)
                                        .child(settings_row(
                                            &t,
                                            Some("captions"),
                                            tr!("Automatic transcription"),
                                            Some(SharedString::from(format!(
                                                "{}\n{}",
                                                match active {
                                                    Some(a) => tr!("Uses {model}", model = a),
                                                    None => tr!("Needs a model above"),
                                                },
                                                tr!("While recording: audio recordings are read piece by piece as you record, so the transcript is ready sooner after Stop; screen recordings are transcribed after they stop.")
                                            ))),
                                            Some(auto_choice.into_any_element()),
                                            false,
                                        ))
                                        .child(settings_row(
                                            &t,
                                            Some("mic"),
                                            tr!("Transcribe from"),
                                            Some(SharedString::from(tr!(
                                                "The original microphone keeps quiet and distant voices that noise reduction can drop. A recording made without noise reduction has only the original. Applies to transcripts started from now on."
                                            ))),
                                            Some(source_choice.into_any_element()),
                                            false,
                                        ))
                                        .child(settings_row(
                                            &t,
                                            Some("scissors"),
                                            tr!("Suggest cuts when the editor opens"),
                                            Some(SharedString::from(tr!(
                                                "Looks for pauses and filler words; nothing changes until you accept"
                                            ))),
                                            Some(cuts_switch.into_any_element()),
                                            false,
                                        ))
                                        .child(settings_row(
                                            &t,
                                            Some("audio-lines"),
                                            tr!("Leave out lines the audio does not hold"),
                                            Some(SharedString::from(tr!(
                                                "Sometimes a transcript has a sentence that nobody said, over a stretch with no speech. Such lines are left out of new transcripts and counted; the check looks at the sound, not at the words."
                                            ))),
                                            Some(drop_switch.into_any_element()),
                                            false,
                                        ))
                                        .child(self.languages_row(&t, cx)),
                                )
                                .child(speakers),
                        )
                        // The right column holds the text model and the glossary: the glossary
                        // can grow long, and the column no longer ends in empty space.
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(voice::COLUMN_MIN_W))
                                .flex()
                                .flex_col()
                                .gap(px(24.))
                                .child(text_model)
                                .child(glossary)
                                .child(ignore),
                        ),
                )
        )
    }
}

/// "Transcribe from" on the AI page: the choice shows at once, the daemon stores
/// `ai.transcribe_raw_mic` (through `set_settings`, so the CLI and agents see the same value) and
/// the next Re-generate panel opens on it. Called only when the user picks the other option, so a
/// library that never stored the key keeps following the default until then.
fn set_transcribe_source(app: &Entity<AppModel>, original_microphone: bool, cx: &mut gpui::App) {
    app.update(cx, |m, cx| {
        m.ai.transcribe_raw_mic = Some(original_microphone);
        m.transcribe.regen_original_audio = Some(original_microphone);
        cx.notify();
    });
    crate::ui::app::set_setting(
        app,
        cm_ai::pipeline::KEY_RAW_MIC,
        serde_json::Value::Bool(original_microphone),
        cx,
    );
}

/// The Transcribe control of a recording (the details card, the recording editor): the button, its
/// progress text, and, while no speech model is installed, a disabled button with the hint and an
/// Open AI button (it never starts a download). `prefix` keeps the element ids apart.
pub fn transcribe_row(
    t: &Theme,
    app: &Entity<AppModel>,
    capture_id: &str,
    prefix: &'static str,
    cx: &gpui::App,
) -> AnyElement {
    use crate::ai::models::transcribe_hint;
    use crate::ui::app::{self, Route};
    use crate::ui::settings::widgets::disabled_button;
    let (running, error, can, hint) = {
        let m = app.read(cx);
        (
            m.transcribe.view(capture_id),
            m.transcribe.error_of(capture_id),
            m.ai.can_transcribe(),
            m.ai.notice_visible(NoticeSite::Transcribe),
        )
    };
    let id_of = |what: &str| SharedString::from(format!("{prefix}-{what}"));
    let (open, run, cap) = (app.clone(), app.clone(), capture_id.to_owned());
    // A run in progress: the import's strip (stage, percent, bar) with Cancel, for a click, the
    // automatic run after a recording and a job the CLI or an agent started alike.
    if let Some(view) = running {
        return transcribe_progress(t, app, capture_id, prefix, &view);
    }
    let action: AnyElement = if can {
        small_button(t, id_of("transcribe"), &tr_ctx!("button", "Transcribe"))
            .on_click(move |_, _, cx| {
                let (app, cap) = (run.clone(), cap.clone());
                cx.defer(move |cx| ai::transcribe(&app, &cap, cx));
            })
            .into_any_element()
    } else {
        disabled_button(
            t,
            id_of("transcribe"),
            &tr_ctx!("button", "Transcribe"),
            &transcribe_hint(),
            false,
        )
        .into_any_element()
    };
    div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(10.))
        .child(action)
        .children(hint.then(|| {
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(8.))
                .text_size(px(type_scale::CAPTION))
                .text_color(rgba_of(t.mist))
                .child(transcribe_hint())
                .child(
                    small_button(t, id_of("open-ai"), &tr_ctx!("button", "Open AI")).on_click(
                        move |_, _, cx| {
                            let app = open.clone();
                            cx.defer(move |cx| app::set_route(&app, Route::Ai, cx));
                        },
                    ),
                )
        }))
        .children(error.map(|e| transcribe_error(t, app, capture_id, prefix, e, false)))
        .into_any_element()
}

/// A click handler built for one partial row.
pub type ClickFn = Box<dyn Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App)>;

/// The first part of a long transcription, read-only, while the rest is still being made
/// (AI-1.AC10; DESIGN §5.5 "Transcript" rows: a time chip over the text, `row_divider` rules). A
/// banner says how much is ready, and the list grows as pieces finish without losing its scroll
/// position. `None` when the recording has no partial transcript to show. Asking for it is done by
/// `prime_transcript_info` (once per run), so a page opened in the middle of a run fills in.
pub fn partial_transcript(
    t: &Theme,
    app: &Entity<AppModel>,
    capture_id: &str,
    prefix: &'static str,
    seek: Option<&dyn Fn(f64) -> ClickFn>,
    cx: &gpui::App,
) -> Option<AnyElement> {
    let (view, handle) = {
        let m = &app.read(cx).transcribe;
        (
            m.partial_of(capture_id)?.clone(),
            m.partial_scroll(capture_id)?,
        )
    };
    let banner = div()
        .flex()
        .items_start()
        .gap(px(10.))
        .text_size(px(13.))
        .text_color(rgba_of(t.text_3))
        .child(div().pt(px(2.)).child(icon("loader-circle", 16., t.mist)))
        .child(
            div()
                .id(SharedString::from(format!("{prefix}-partial-banner")))
                .flex_1()
                .min_w(px(0.))
                .child(SharedString::from(crate::ai::partial::banner(
                    view.covered_secs,
                ))),
        );
    let rows = view.rows.iter().enumerate().map(|(i, r)| {
        let row = div()
            .id(SharedString::from(format!("{prefix}-partial-row-{i}")))
            .role(Role::ListItem)
            .px(px(14.))
            .py(px(10.))
            .flex()
            .flex_col()
            .gap(px(2.))
            .border_b_1()
            .border_color(rgba_of(t.row_divider()))
            .child(
                div()
                    .mono()
                    .tnum()
                    .text_size(px(12.5))
                    .text_color(rgba_of(t.mist))
                    .child(SharedString::from(crate::ai::partial::clock(r.start_secs))),
            )
            .child(
                div()
                    .text_size(px(14.5))
                    .line_height(px(22.))
                    .text_color(rgba_of(t.text_2))
                    .child(SharedString::from(r.text.clone())),
            );
        // A click seeks playback to the row, as a row of the finished transcript does.
        match seek {
            Some(f) => row
                .cursor_pointer()
                .on_click(f(r.start_secs))
                .into_any_element(),
            None => row.into_any_element(),
        }
    });
    let list = div()
        .id(SharedString::from(format!("{prefix}-partial-list")))
        .role(Role::List)
        .aria_label(SharedString::from(crate::ai::partial::list_label()))
        .w_full()
        .flex()
        .flex_col()
        .children(rows);
    Some(
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(banner)
            .child(
                div()
                    .w_full()
                    .rounded(px(scale::RADIUS_SM))
                    .border_1()
                    .border_color(rgba_of(t.border))
                    .overflow_hidden()
                    .child(crate::ui::scroll::nested_scroll(t, &handle, 260., list)),
            )
            .into_any_element(),
    )
}

/// The progress of a running transcription, the same strip an imported file shows (stage,
/// percent, bar: `import_cards::progress_body`), with Cancel (`cancel_job`, the CLI's too) once
/// the job has an id. On the recording's page, the editor tabs and the audio page.
pub fn transcribe_progress(
    t: &Theme,
    app: &Entity<AppModel>,
    capture_id: &str,
    prefix: &'static str,
    view: &crate::ai::jobs::RunView,
) -> AnyElement {
    let (app, cap) = (app.clone(), capture_id.to_owned());
    let cancel = view.job_id.is_some().then(|| {
        small_button(
            t,
            SharedString::from(format!("{prefix}-transcribe-cancel")),
            &tr_ctx!("button", "Cancel"),
        )
        .on_click(move |_, _, cx| {
            let (app, cap) = (app.clone(), cap.clone());
            cx.defer(move |cx| ai::cancel_transcribe(&app, &cap, cx));
        })
    });
    div()
        .id(SharedString::from(format!("{prefix}-transcribe-progress")))
        .w_full()
        .max_w(px(420.))
        .flex()
        .items_center()
        .gap(px(12.))
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .child(crate::ui::import_cards::progress_body(
                    t,
                    &view.stage,
                    view.fraction.map(f64::from),
                    false,
                )),
        )
        .children(cancel)
        .into_any_element()
}

/// The failure of the last run, with Retry. `has_transcript`: a transcript is stored (a failed
/// Re-generate), so Retry opens the Re-generate panel, which says what a new run replaces,
/// instead of running again at once.
pub fn transcribe_error(
    t: &Theme,
    app: &Entity<AppModel>,
    capture_id: &str,
    prefix: &'static str,
    message: String,
    has_transcript: bool,
) -> AnyElement {
    let (app, cap) = (app.clone(), capture_id.to_owned());
    div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(10.))
        .child(
            div()
                .text_size(px(type_scale::CAPTION))
                .text_color(rgba_of(t.warning))
                .child(SharedString::from(message)),
        )
        .child(
            small_button(
                t,
                SharedString::from(format!("{prefix}-transcribe-retry")),
                &tr_ctx!("button", "Retry"),
            )
            .on_click(move |_, _, cx| {
                let (app, cap) = (app.clone(), cap.clone());
                cx.defer(move |cx| {
                    if has_transcript {
                        ai::open_regen(&app, &cap, cx);
                    } else {
                        ai::transcribe(&app, &cap, cx);
                    }
                });
            }),
        )
        .into_any_element()
}

/// Reads the transcript summary when the page shows a recording whose answer is missing or
/// stale (see `ai::summary::TranscriptCache`). Safe to call on every render.
pub fn prime_transcript_info<V: 'static>(
    app: &Entity<AppModel>,
    capture_id: &str,
    cx: &mut Context<V>,
) {
    let (epoch, needs) = {
        let m = app.read(cx);
        (
            m.transcribe.info_epoch,
            m.transcribe
                .transcripts
                .needs_load(capture_id, m.transcribe.info_epoch),
        )
    };
    let _ = epoch;
    // A long run is going on: read its first part once (later ones come with `transcript.partial`).
    if app.update(cx, |m, _| m.transcribe.take_partial_ask(capture_id)) {
        let (app, id) = (app.clone(), capture_id.to_owned());
        cx.defer(move |cx| ai::read_partial(&app, &id, cx));
    }
    if needs {
        let (app, id) = (app.clone(), capture_id.to_owned());
        cx.defer(move |cx| ai::ensure_transcript_info(&app, &id, cx));
    }
}
