//! The voice sections of the AI page (board V8): speaker labels and the text model. A child of
//! `ai_view` so it can use the page's fields; the rules are in `crate::ai::voice` and the
//! buttons' work in `ui::ai_voice`. The built-in text model only (maintainer decision Oct 27,
//! 2026): no other server, no Remote, no API key or first-send notice. Board V8 has no design for
//! the built-in engine, so its list is plain rows in the same card, with the page's own buttons and
//! progress bar.

use gpui::{div, prelude::*, px, AnyElement, Context, FontWeight, Role, SharedString, Window};

use cm_i18n::{tr, tr_ctx};

use super::{action_icon, progress_bar, status_glyph, AiPage};
use crate::ai::builtin::{self, BuiltInRow};
use crate::ai::models::RowStatus;
use crate::ai::voice::{SpeakerCard, TestState, TextVm, VoiceVm};
use crate::theme::{scale, type_scale, Theme};
use crate::ui::ai_voice as ops;
use crate::ui::components::{
    badge, button_with_icon, card, dot, icon, label_fill, shrink_text, switch, ButtonKind,
};
use crate::ui::kit;
use crate::ui::paint::rgba_of;
use crate::ui::settings::widgets::{
    caption, disabled_button, list_card, settings_row, small_button, small_button_with_icon,
};
use crate::ui::text_field::TextFieldEvent;

/// Which box of the voice sections an event came from.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum VoiceField {
    Name,
}

/// Width of the name box, px.
const NAME_BOX_W: f32 = 200.;
/// Minimum width of a column of the two-column page, px (they stack below this).
pub(super) const COLUMN_MIN_W: f32 = 440.;

/// A section title: 15 px medium, then a mist subtitle.
pub(super) fn section_title(t: &Theme, title: String, sub: String) -> AnyElement {
    div()
        .flex()
        .flex_wrap()
        .items_baseline()
        .gap_x(px(10.))
        .child(
            div()
                .text_size(px(type_scale::BODY))
                .font_weight(FontWeight::MEDIUM)
                .child(title),
        )
        .child(caption(t, sub))
        .into_any_element()
}

impl AiPage {
    pub(super) fn voice_field_event(
        &mut self,
        which: VoiceField,
        ev: &TextFieldEvent,
        cx: &mut Context<Self>,
    ) {
        if matches!(ev, TextFieldEvent::Submitted) {
            self.voice_save(which, cx);
        }
    }

    /// Saves what a box holds (Enter or its Save button).
    pub(super) fn voice_save(&mut self, which: VoiceField, cx: &mut Context<Self>) {
        let app = self.app.clone();
        match which {
            VoiceField::Name => {
                let text = self.name_field.read(cx).text().to_owned();
                ops::set_display_name(&app, &text, cx);
            }
        }
    }

    /// Puts the saved name into the box the first time and whenever it changes from outside;
    /// typing alone never moves it.
    pub(super) fn voice_sync_fields(&mut self, cx: &mut Context<Self>) {
        let name = self.app.read(cx).ai.voice.speakers.display_name.clone();
        if self.synced.as_deref() != Some(name.as_str()) {
            self.name_field.update(cx, |f, cx| {
                f.load(&name, crate::ui::text_field::CaretAt::End, cx)
            });
            self.synced = Some(name);
        }
    }

    // ---- speaker labels (AI-8) -----------------------------------------------------------

    pub(super) fn speakers_section(&self, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let sp = self.app.read(cx).ai.voice.speakers.clone();
        let name_dirty = sp.name_dirty(self.name_field.read(cx).text());
        let app = self.app.clone();
        let enabled = sp.auto_enabled();
        let on = sp.auto_on();
        let auto_switch = switch(
            t,
            "ai-auto-speakers",
            tr!("Find speakers after each transcript"),
            on,
            enabled,
        )
        .on_click(cx.listener(move |_, _, _, cx| {
            if enabled {
                ops::set_auto_speakers(&app, !on, cx);
            }
        }));
        let name_row = {
            let mut c = div().flex().items_center().gap(px(8.)).child(
                div()
                    .w(px(NAME_BOX_W))
                    .min_w_0()
                    .child(self.name_field.clone()),
            );
            if name_dirty {
                c =
                    c.child(small_button(t, "ai-name-save", &tr!("Save")).on_click(
                        cx.listener(|this, _, _, cx| this.voice_save(VoiceField::Name, cx)),
                    ));
            }
            c.into_any_element()
        };
        div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(section_title(
                t,
                tr!("Speaker labels"),
                tr!("Who spoke when, from the voices"),
            ))
            .child(
                list_card(t)
                    .id("ai-speakers")
                    .role(Role::Group)
                    .aria_label(SharedString::from(tr!("Speaker labels")))
                    // The two models ship inside the app: nothing to download or import. Only a
                    // build without them says so.
                    .children((sp.card() == SpeakerCard::Unavailable).then(|| {
                        settings_row(
                            t,
                            Some("audio-lines"),
                            tr!("Speaker model"),
                            Some(SharedString::from(tr!("Not available in this build"))),
                            None,
                            false,
                        )
                    }))
                    .child(settings_row(
                        t,
                        Some("zap"),
                        tr!("Find speakers after each transcript"),
                        Some(SharedString::from(if enabled {
                            tr!("Starts when a transcript is ready")
                        } else {
                            tr!("Not available in this build")
                        })),
                        Some(auto_switch.into_any_element()),
                        false,
                    ))
                    .child(settings_row(
                        t,
                        Some("pencil"),
                        tr!("Your name in transcripts"),
                        Some(SharedString::from(tr!(
                            "Used for \u{201c}My microphone only hears me\u{201d}"
                        ))),
                        Some(name_row),
                        true,
                    )),
            )
            .into_any_element()
    }

    // ---- the text model (AI-7) -----------------------------------------------------------

    pub(super) fn text_model_section(
        &self,
        t: &Theme,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let v: VoiceVm = self.app.read(cx).ai.voice.clone();
        let tm = &v.text;
        let mut body = div().flex().flex_col().gap(px(14.));
        body = body.child(self.builtin_models(t, tm, v.memory_gb(), cx));
        if tm.ready() {
            body = body.child(self.test_row(t, tm, cx));
        }
        div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(section_title(
                t,
                tr!("Text model"),
                tr!("Reports and translation to any language"),
            ))
            .child(
                card(t)
                    .id("ai-text-model")
                    .role(Role::Group)
                    .aria_label(SharedString::from(tr!("Text model")))
                    .p(px(16.))
                    .child(body),
            )
            .into_any_element()
    }

    /// The model list (download, use, remove, progress, licence, the recommended one) and the
    /// engine's state. With no model in use, reports and translation to other languages are off.
    fn builtin_models(
        &self,
        t: &Theme,
        tm: &TextVm,
        mem_gb: Option<u32>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let b = self.app.read(cx).builtin.clone();
        let rows = b.rows(mem_gb, tm.in_use());
        let mut col = div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(caption(t, builtin::list_caption(mem_gb)));
        if rows.is_empty() {
            col = col.child(caption(
                t,
                if b.loaded {
                    tr!("No built-in model is available in this version yet. Check for an update.")
                } else {
                    tr!("Looking for the built-in models\u{2026}")
                },
            ));
        } else {
            let mut list = list_card(t)
                .id("ai-builtin-models")
                .role(Role::List)
                .aria_label(SharedString::from(tr!("Built-in text models")));
            let last = rows.len() - 1;
            for (i, r) in rows.iter().enumerate() {
                list = list.child(self.builtin_row(t, r, i != last, cx));
            }
            col = col.child(list);
        }
        let problem = tm.engine.is_problem();
        col.child(
            div()
                .id("ai-builtin-engine")
                .role(Role::Status)
                .flex()
                .items_center()
                .gap(px(8.))
                .text_size(px(13.))
                .text_color(rgba_of(if problem { t.warning } else { t.mist }))
                .child(dot(if problem { t.warning } else { t.mist }, 6.))
                .child(label_fill(tm.engine.text())),
        )
        .into_any_element()
    }

    /// One built-in model: name, size and memory, licence, state, progress, buttons.
    fn builtin_row(
        &self,
        t: &Theme,
        r: &BuiltInRow,
        divider: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let row = &r.row;
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
            let element_id = SharedString::from(format!("ai-builtin-{}-{:?}", row.id, a));
            let handler = cx.listener(move |_, _, _, cx| {
                ops::builtin_row_action(&app, &id, action.clone(), cx)
            });
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
        let progress = row.progress.as_ref().map(|p| {
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
        });
        let facts = [row.size.clone(), row.memory.clone()]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" \u{00b7} ");
        div()
            .id(SharedString::from(format!("ai-builtin-row-{}", row.id)))
            .role(Role::ListItem)
            .aria_label(SharedString::from(tr!(
                "{name}, {size}, {status}",
                name = row.label,
                size = row.size,
                status = row.status.text()
            )))
            .px(px(14.))
            .py(px(12.))
            .flex()
            .flex_col()
            .gap(px(10.))
            .when(divider, |d| {
                d.border_b_1().border_color(rgba_of(t.row_divider()))
            })
            .when(r.in_use, |d| d.bg(rgba_of(t.raised)))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(10.))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(180.))
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .items_center()
                                    .gap(px(8.))
                                    .child(
                                        div()
                                            .text_size(px(type_scale::UI))
                                            .font_weight(FontWeight::MEDIUM)
                                            .child(SharedString::from(row.label.clone())),
                                    )
                                    .children(r.recommended.then(|| {
                                        div()
                                            .text_size(px(12.))
                                            .text_color(rgba_of(t.success))
                                            .child(builtin::recommended_tag())
                                    }))
                                    .children(
                                        (!r.license.is_empty())
                                            .then(|| badge(t, r.license.clone())),
                                    ),
                            )
                            .child(caption(t, facts))
                            .children((!row.hint.is_empty()).then(|| caption(t, row.hint.clone())))
                            .children(r.memory_warning().map(|w| {
                                div()
                                    .text_size(px(12.))
                                    .text_color(rgba_of(t.warning))
                                    .child(SharedString::from(w))
                            })),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .text_size(px(13.))
                            .text_color(rgba_of(status_color))
                            .child(icon(glyph, 14., tc.icon))
                            .child(shrink_text(row.status.text())),
                    ),
            )
            .children(progress)
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

    /// The Test button and its result (time, structured output).
    fn test_row(&self, t: &Theme, tm: &TextVm, cx: &mut Context<Self>) -> AnyElement {
        let app = self.app.clone();
        let running = tm.test == TestState::Running;
        let lines = tm.test_lines();
        let failed = matches!(tm.test, TestState::Failed(_));
        let passed = matches!(tm.test, TestState::Passed { .. });
        let button = if running {
            disabled_button(
                t,
                "ai-text-test",
                &tr_ctx!("text model", "Test"),
                &tr!("Testing\u{2026}"),
                false,
            )
            .into_any_element()
        } else {
            button_with_icon(
                t,
                "ai-text-test",
                tr_ctx!("text model", "Test"),
                ButtonKind::Secondary,
                Some("play"),
            )
            .on_click(cx.listener(move |_, _, _, cx| ops::text_test(&app, cx)))
            .into_any_element()
        };
        div()
            .id("ai-text-test-result")
            .role(Role::Status)
            .p(px(12.))
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(14.))
            .rounded(px(scale::RADIUS_MD))
            .border_1()
            .border_color(rgba_of(t.border))
            .child(button)
            .children(lines.map(|(head, detail)| {
                div()
                    .flex_1()
                    .min_w(px(160.))
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .text_size(px(type_scale::UI))
                            .text_color(rgba_of(if passed {
                                t.success
                            } else if failed {
                                t.warning
                            } else {
                                t.text_2
                            }))
                            .children(passed.then(|| icon("circle-check", 14., t.success)))
                            .children(failed.then(|| icon("triangle-alert", 14., t.warning)))
                            .child(shrink_text(head)),
                    )
                    .children((!detail.is_empty()).then(|| caption(t, detail)))
            }))
            .into_any_element()
    }
}
