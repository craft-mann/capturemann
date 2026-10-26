//! The voice sections of the AI page (board V8): speaker labels and the text model. A child of
//! `ai_view` so it can use the page's fields; the rules are in `crate::ai::voice` and the
//! buttons' work in `ui::ai_voice`. Local text models only for now (maintainer decision
//! Oct 26, 2026): no Remote, Anthropic, API key or first-send notice.

use gpui::{div, prelude::*, px, AnyElement, Context, FontWeight, Role, SharedString, Window};

use cm_i18n::{tr, tr_ctx};

use super::AiPage;
use crate::ai::models::size_label;
use crate::ai::voice::{self, PresetVm, ProviderChoice, SpeakerCard, TestState, TextVm, VoiceVm};
use crate::theme::{scale, type_scale, Theme};
use crate::ui::ai_voice as ops;
use crate::ui::components::{
    badge, button_with_icon, card, dot, icon, shrink_text, switch, ButtonKind,
};
use crate::ui::paint::{rgba_of, TextExt};
use crate::ui::settings::widgets::{
    caption, disabled_button, list_card, segment, segmented, settings_row, small_button,
    small_button_with_icon,
};
use crate::ui::text_field::{TextField, TextFieldEvent};

/// Which box of the voice sections an event came from.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum VoiceField {
    Name,
    Url,
    Model,
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
            VoiceField::Url => {
                let text = self.url_field.read(cx).text().to_owned();
                ops::text_set_url(&app, &text, cx);
            }
            VoiceField::Model => {
                let text = self.model_field.read(cx).text().to_owned();
                ops::text_set_model(&app, &text, cx);
            }
        }
    }

    /// Puts the saved values into the boxes the first time and whenever they change from
    /// outside (a provider switch clears the model); typing alone never moves them.
    pub(super) fn voice_sync_fields(&mut self, cx: &mut Context<Self>) {
        let (name, url, model, loaded) = {
            let v = &self.app.read(cx).ai.voice;
            (
                v.speakers.display_name.clone(),
                v.text.base_url.clone(),
                v.text.model.clone(),
                v.text.loaded,
            )
        };
        if self.synced.0.as_deref() != Some(name.as_str()) {
            self.name_field.update(cx, |f, cx| {
                f.load(&name, crate::ui::text_field::CaretAt::End, cx)
            });
            self.synced.0 = Some(name);
        }
        if loaded && self.synced.1.as_deref() != Some(url.as_str()) {
            self.url_field.update(cx, |f, cx| {
                f.load(&url, crate::ui::text_field::CaretAt::End, cx)
            });
            self.synced.1 = Some(url);
        }
        if loaded && self.synced.2.as_deref() != Some(model.as_str()) {
            self.model_field.update(cx, |f, cx| {
                f.load(&model, crate::ui::text_field::CaretAt::End, cx)
            });
            self.synced.2 = Some(model);
        }
    }

    // ---- speaker labels (AI-8) -----------------------------------------------------------

    pub(super) fn speakers_section(&self, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let sp = self.app.read(cx).ai.voice.speakers.clone();
        let name_dirty = sp.name_dirty(self.name_field.read(cx).text());
        let (hint, control): (String, Option<AnyElement>) = match sp.card() {
            SpeakerCard::Unavailable => (tr!("Speaker model not available yet"), None),
            SpeakerCard::Offer(m) => (
                tr!(
                    "{name} \u{00b7} {licence} \u{00b7} {size}, shown before you download",
                    name = m.label,
                    licence = m.licence,
                    size = size_label(m.size_mb)
                ),
                // The download action arrives with the model (spike S7).
                Some(
                    disabled_button(
                        t,
                        "ai-speaker-download",
                        &tr_ctx!("speaker model", "Download"),
                        &tr!("Downloading speaker models is not available yet."),
                        false,
                    )
                    .into_any_element(),
                ),
            ),
            SpeakerCard::Installed(name) => (tr!("Installed: {name}", name = name), None),
        };
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
                    .child(settings_row(
                        t,
                        Some("audio-lines"),
                        tr!("Speaker model"),
                        Some(SharedString::from(hint)),
                        control,
                        false,
                    ))
                    .child(settings_row(
                        t,
                        Some("zap"),
                        tr!("Find speakers after each transcript"),
                        Some(SharedString::from(if enabled {
                            tr!("Off by default")
                        } else {
                            tr!("Needs a speaker model")
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
        body = body.child(self.provider_control(t, tm, cx));
        body = body.child(match tm.provider {
            ProviderChoice::None => caption(
                t,
                tr!("No text model. Reports and translation to other languages are off."),
            )
            .into_any_element(),
            ProviderChoice::OnThisMac => self.local_servers(t, tm, cx),
        });
        if tm.provider != ProviderChoice::None {
            body = body.child(self.test_row(t, tm, cx));
        }
        body = body
            .child(div().h(px(1.)).bg(rgba_of(t.row_divider())))
            .child(self.recommended(t, v.memory_gb()));
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

    fn provider_control(&self, t: &Theme, tm: &TextVm, cx: &mut Context<Self>) -> AnyElement {
        let mut seg = segmented(t, "ai-text-provider", &tr!("Text model provider"));
        for c in ProviderChoice::ALL {
            let app = self.app.clone();
            seg = seg.child(
                segment(
                    t,
                    SharedString::from(format!("ai-text-provider-{c:?}")),
                    &c.label(),
                    tm.provider == c,
                )
                .flex_1()
                .on_click(cx.listener(move |_, _, _, cx| ops::text_choose_provider(&app, c, cx))),
            );
        }
        seg.into_any_element()
    }

    /// Ollama and LM Studio on loopback, then the model picker.
    fn local_servers(&self, t: &Theme, tm: &TextVm, cx: &mut Context<Self>) -> AnyElement {
        let mut rows = div().flex().flex_col().gap(px(8.));
        for p in &tm.presets {
            rows = rows.child(self.server_row(t, tm, p, cx));
        }
        let app = self.app.clone();
        let check = small_button_with_icon(
            t,
            "ai-text-detect",
            &if tm.detecting {
                tr!("Looking\u{2026}")
            } else {
                tr!("Check again")
            },
            Some("refresh-cw"),
        )
        .on_click(cx.listener(move |_, _, _, cx| ops::detect_servers(&app, cx)));
        let mut col = div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap(px(8.))
                    .child(caption(t, tr!("Servers on this Mac")))
                    .child(check),
            )
            .child(rows);
        let running = tm.current_preset().is_some_and(|p| p.running);
        if running {
            col = col.child(self.model_picker(t, tm, cx));
        } else {
            col = col.child(caption(
                t,
                tr!("Start Ollama or LM Studio, then press Check again."),
            ));
        }
        col.child(self.custom_server(t, tm, cx)).into_any_element()
    }

    fn server_row(
        &self,
        t: &Theme,
        tm: &TextVm,
        p: &PresetVm,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected = tm.current_preset().is_some_and(|c| c.preset == p.preset);
        let (app, id) = (self.app.clone(), p.preset.clone());
        div()
            .id(SharedString::from(format!("ai-server-{}", p.preset)))
            .role(Role::RadioButton)
            .aria_label(SharedString::from(tr!(
                "{name}, {state}",
                name = p.name(),
                state = if p.running {
                    tr!("Running")
                } else {
                    tr!("Not running")
                }
            )))
            .focusable()
            .tab_stop(true)
            .cursor_pointer()
            .min_h(px(scale::TARGET))
            .px(px(14.))
            .py(px(8.))
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(12.))
            .rounded(px(scale::RADIUS_MD))
            .border_1()
            .border_color(rgba_of(if selected { t.line } else { t.border }))
            .when(selected, |d| d.bg(rgba_of(t.raised)))
            .child(icon(
                if selected { "circle-dot" } else { "circle" },
                18.,
                if selected { t.text } else { t.mist },
            ))
            .child(
                div()
                    .flex_1()
                    .min_w(px(120.))
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_size(px(type_scale::UI))
                            .font_weight(FontWeight::MEDIUM)
                            .child(SharedString::from(p.name())),
                    )
                    .child(
                        div()
                            .text_size(px(type_scale::CAPTION))
                            .text_color(rgba_of(t.mist))
                            .mono()
                            .child(SharedString::from(p.address())),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .text_size(px(13.))
                    .text_color(rgba_of(if p.running { t.success } else { t.mist }))
                    .child(dot(if p.running { t.success } else { t.mist }, 6.))
                    .child(shrink_text(if p.running {
                        tr!("Running")
                    } else {
                        tr!("Not running")
                    })),
            )
            .on_click(cx.listener(move |_, _, _, cx| ops::text_choose_preset(&app, &id, cx)))
            .into_any_element()
    }

    /// The model button (name, context size) and, open, the server's models.
    fn model_picker(&self, t: &Theme, tm: &TextVm, cx: &mut Context<Self>) -> AnyElement {
        let app = self.app.clone();
        let current = if tm.model.is_empty() {
            tr!("Choose a model")
        } else {
            tm.model.clone()
        };
        let ctx = tm.model_context().map(TextVm::context_label);
        let button = div()
            .id("ai-text-model-picker")
            .role(Role::ComboBox)
            .aria_label(SharedString::from(tr!("Text model")))
            .focusable()
            .tab_stop(true)
            .cursor_pointer()
            .h(px(scale::CONTROL_H))
            .px(px(14.))
            .flex()
            .items_center()
            .gap(px(10.))
            .rounded(px(scale::RADIUS_MD))
            .border_1()
            .border_color(rgba_of(t.line))
            .bg(rgba_of(t.field))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .text_size(px(type_scale::UI))
                    .text_color(rgba_of(if tm.model.is_empty() { t.mist } else { t.text }))
                    .child(current),
            )
            .children(ctx.map(|c| {
                div()
                    .flex_none()
                    .text_size(px(type_scale::CAPTION))
                    .text_color(rgba_of(t.mist))
                    .mono()
                    .child(c)
            }))
            .child(icon("arrow-down", 14., t.text_2))
            .on_click(cx.listener(move |_, _, _, cx| ops::text_toggle_picker(&app, cx)));
        let mut col = div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(caption(t, tm.picker_caption()))
            .child(button);
        if tm.picker_open {
            let mut list = list_card(t)
                .id("ai-text-model-list")
                .role(Role::ListBox)
                .aria_label(SharedString::from(tr!("Models on this server")));
            let models = tm.server_models();
            if models.is_empty() {
                list = list.child(div().p(px(14.)).child(caption(
                    t,
                    tr!("This server lists no models. Pull or load one first."),
                )));
            }
            for (i, m) in models.iter().enumerate() {
                let (app, id) = (self.app.clone(), m.id.clone());
                let selected = m.id == tm.model;
                list = list.child(
                    div()
                        .id(SharedString::from(format!("ai-text-model-{i}")))
                        .role(Role::ListBoxOption)
                        .aria_label(SharedString::from(m.id.clone()))
                        .focusable()
                        .tab_stop(true)
                        .cursor_pointer()
                        .min_h(px(scale::TARGET))
                        .px(px(14.))
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .when(selected, |d| d.bg(rgba_of(t.raised)))
                        .hover(|s| s.bg(rgba_of(t.raised)))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .text_size(px(type_scale::UI))
                                .child(SharedString::from(m.id.clone())),
                        )
                        .children(m.context_tokens.map(|c| {
                            div()
                                .text_size(px(type_scale::CAPTION))
                                .text_color(rgba_of(t.mist))
                                .mono()
                                .child(TextVm::context_label(c))
                        }))
                        .on_click(
                            cx.listener(move |_, _, _, cx| ops::text_choose_model(&app, &id, cx)),
                        ),
                );
            }
            col = col.child(list);
        }
        col.into_any_element()
    }

    /// A server that is not Ollama or LM Studio: any OpenAI-compatible address on this Mac, and
    /// the model name typed (the page cannot list a custom server's models).
    fn custom_server(&self, t: &Theme, tm: &TextVm, cx: &mut Context<Self>) -> AnyElement {
        let save_row = |field: &gpui::Entity<TextField>, id: &'static str, which: VoiceField| {
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(div().flex_1().min_w_0().child(field.clone()))
                .child(
                    small_button(t, id, &tr!("Save"))
                        .on_click(cx.listener(move |this, _, _, cx| this.voice_save(which, cx))),
                )
        };
        let mut col = div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(caption(t, tr!("Another server on this Mac (address)")))
            .child(save_row(
                &self.url_field,
                "ai-text-url-save",
                VoiceField::Url,
            ));
        if tm.current_preset().is_none() {
            col = col.child(caption(t, tr!("Model"))).child(save_row(
                &self.model_field,
                "ai-text-model-save",
                VoiceField::Model,
            ));
        }
        if let Some(e) = &tm.error {
            col = col.child(
                div()
                    .text_size(px(type_scale::CAPTION))
                    .text_color(rgba_of(t.warning))
                    .child(SharedString::from(e.clone())),
            );
        }
        col.into_any_element()
    }

    /// The Test button and its result (time, structured output, loopback).
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

    /// "Recommended for your Mac": the plan's models by memory, this Mac's real memory.
    fn recommended(&self, t: &Theme, mem_gb: Option<u32>) -> AnyElement {
        let mut list = list_card(t)
            .id("ai-recommended")
            .role(Role::List)
            .aria_label(SharedString::from(tr!("Recommended for your Mac")));
        let rows = voice::recommendations(mem_gb);
        let last = rows.len().saturating_sub(1);
        for (i, (r, fits)) in rows.into_iter().enumerate() {
            list = list.child(
                div()
                    .id(SharedString::from(format!("ai-rec-{}", r.id)))
                    .role(Role::ListItem)
                    .px(px(14.))
                    .py(px(10.))
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(12.))
                    .when(i != last, |d| {
                        d.border_b_1().border_color(rgba_of(t.row_divider()))
                    })
                    .when(fits, |d| d.bg(rgba_of(t.raised)))
                    .child(
                        div()
                            .w(px(72.))
                            .flex_none()
                            .text_size(px(type_scale::CAPTION))
                            .text_color(rgba_of(t.mist))
                            .mono()
                            .child(SharedString::from(r.tier_label())),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(160.))
                            .flex()
                            .flex_col()
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
                                            .child(SharedString::from(r.name)),
                                    )
                                    .children(fits.then(|| {
                                        div()
                                            .text_size(px(12.))
                                            .text_color(rgba_of(t.success))
                                            .child(tr!("fits this Mac"))
                                    }))
                                    .children(r.candidate.then(|| badge(t, tr!("candidate")))),
                            )
                            .child(caption(t, r.note())),
                    )
                    .child(badge(t, r.licence)),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_baseline()
                    .gap_x(px(10.))
                    .child(
                        div()
                            .text_size(px(type_scale::UI))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(tr!("Recommended for your Mac")),
                    )
                    .child(caption(t, voice::recommended_caption(mem_gb))),
            )
            .child(list)
            .child(caption(
                t,
                tr!("Install a model in Ollama or LM Studio, then pick it above. CaptureMann never downloads text models itself. Default decided by S8."),
            ))
            .into_any_element()
    }
}
