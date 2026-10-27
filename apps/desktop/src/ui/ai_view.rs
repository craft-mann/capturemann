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
    caption, list_card, settings_row, small_button, small_button_with_icon,
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
    term_field: Entity<TextField>,
    heard_field: Entity<TextField>,
    /// The voice sections' box (V8): your name.
    name_field: Entity<TextField>,
    /// What the name box was last loaded with (`None`: never).
    synced: Option<String>,
    _subs: Vec<Subscription>,
}

impl AiPage {
    pub fn new(app: Entity<AppModel>, cx: &mut Context<Self>) -> Self {
        let term_field = cx
            .new(|cx| TextField::new(tr!("Glossary term"), tr!("Google Drive"), cx).without_icon());
        let heard_field = cx.new(|cx| {
            TextField::new(tr!("Often heard as"), tr!("Logo Drive, Gu gồ Drai"), cx).without_icon()
        });
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
            glossary_edit: None,
            glossary_error: None,
            term_field,
            heard_field,
            _subs: subs,
        }
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
        let (rows, show_banner, load_error, action_error, busy, auto, active) = {
            let m = self.app.read(cx);
            (
                m.ai.rows(),
                m.ai.notice_visible(NoticeSite::AiPage),
                m.ai.load_error.clone(),
                m.ai.action_error.clone(),
                m.ai.busy,
                m.ai.auto_transcribe,
                m.ai.active().map(|a| a.label.clone()),
            )
        };
        let model_cards: Vec<AnyElement> =
            rows.iter().map(|r| self.model_card(&t, r, cx)).collect();
        let app = self.app.clone();
        let auto_switch = switch(
            &t,
            "ai-auto",
            tr!("Transcribe after each recording"),
            auto,
            true,
        )
        .on_click(cx.listener(move |_, _, _, cx| ai::set_auto_transcribe(&app, !auto, cx)));
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
                                        .child(tr!("Transcripts, speakers, translations and reports, made on this Mac.")),
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
                                .child(list_card(&t).child(settings_row(
                                    &t,
                                    Some("captions"),
                                    tr!("Transcribe after each recording"),
                                    Some(SharedString::from(match active {
                                        Some(a) => tr!("Uses {model}", model = a),
                                        None => tr!("Needs a model above"),
                                    })),
                                    Some(auto_switch.into_any_element()),
                                    true,
                                )))
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
                                .child(glossary),
                        ),
                )
        )
    }
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
            m.transcribe.status_text(capture_id),
            m.transcribe.error_of(capture_id),
            m.ai.can_transcribe(),
            m.ai.notice_visible(NoticeSite::Transcribe),
        )
    };
    let id_of = |what: &str| SharedString::from(format!("{prefix}-{what}"));
    let (open, run, cap) = (app.clone(), app.clone(), capture_id.to_owned());
    let action: AnyElement = if let Some(text) = running {
        disabled_button(t, id_of("transcribe"), &text, &tr!("Working"), false).into_any_element()
    } else if can {
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
        .children(error.map(|e| {
            div()
                .text_size(px(type_scale::CAPTION))
                .text_color(rgba_of(t.warning))
                .child(SharedString::from(e))
        }))
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
    if needs {
        let (app, id) = (app.clone(), capture_id.to_owned());
        cx.defer(move |cx| ai::ensure_transcript_info(&app, &id, cx));
    }
}
