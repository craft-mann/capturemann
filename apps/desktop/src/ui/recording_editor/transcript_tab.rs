//! The Transcript tab of an audio recording (VOC-2, redesign V4): the wide panel with the language
//! switcher (the original and each translation) and "Translate...", the turns with their speaker,
//! time chip and "overlap" chip, the marker dividers between them, the line under the playhead
//! highlighted and kept in view, and the footer with TXT, SRT and VTT and the Speaker names switch.
//! The grouping is in `crate::recedit::voice` (unit tested). Not seen by eye yet
//! (`tests/manual/VOC-2.md`).
//!
//! Reading only: the lines are edited in the Captions tab of a screen recording and by the
//! caption tools; editing them here is a later step (TODO(design): the board draws no edit
//! affordance).

use std::time::Instant;

use cm_i18n::{tr, tr_ctx};
use gpui::{div, prelude::*, px, AnyElement, Context, FontWeight, Role, SharedString, Stateful};

use super::audio_view::{avatar, speakers_label};
use super::tabs::{hint, Inspector};
use super::{RecordingEditor, Session};
use crate::ai::caption_view::{follow_allowed, follow_target};
use crate::ai::client::CaptionFormat;
use crate::recedit::voice::{
    self, clock, language_name, line_at, row_of_line, speaker_colour, speaker_index,
    translate_choices, Row,
};
use crate::theme::{scale, type_scale, Theme};
use crate::ui::components::{
    button_with_icon, ellipsis_text, icon, label_fill, shrink_text, switch, ButtonKind,
};
use crate::ui::kit::{callout, Tone};
use crate::ui::paint::{rgba_of, TextExt};
use crate::ui::scroll::fill_scroll;
use crate::ui::settings::widgets::{segment, segmented, small_button_with_icon};

impl RecordingEditor {
    pub(super) fn transcript_panel(
        &self,
        t: &Theme,
        s: &Session,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ai = &self.ai;
        let id = self.id.clone().unwrap_or_default();
        let shell = |body: Vec<AnyElement>| {
            super::tabs::panel(
                t,
                Inspector::Transcript,
                tr!("Whole recording"),
                body,
                Vec::new(),
            )
        };
        if ai.loading && !ai.has_transcript {
            return shell(vec![
                hint(t, "captions", tr!("Loading\u{2026}")).into_any_element()
            ]);
        }
        if !ai.has_transcript {
            // A run is on its way (the automatic one after a recording, or a click): say so, and
            // that nothing needs pressing, instead of the "No transcript yet" of an idle tab.
            let running = self.app.read(cx).transcribe.status_text(&id).is_some();
            let message = if running {
                tr!(
                    "The transcript is being written on this Mac. A long recording takes several \
                     minutes. You don\u{2019}t need to press anything: it appears here when it\u{2019}s \
                     done, and speaker labels follow."
                )
            } else {
                tr!(
                    "No transcript yet. Transcribe the recording to read it here; it runs on \
                     this Mac."
                )
            };
            return shell(vec![
                hint(t, "captions", message).into_any_element(),
                crate::ui::ai_view::transcribe_row(t, &self.app, &id, "re", cx),
            ]);
        }
        // Re-generate: another model or language can give a better transcript, so the tab offers
        // the details page's panel (model, language, what it replaces). The summary of the stored
        // transcript says whether the user edited the captions.
        crate::ui::ai_view::prime_transcript_info(&self.app, &id, cx);
        let (running, draft, installed, edited) = {
            let m = self.app.read(cx);
            (
                m.transcribe.status_text(&id),
                m.transcribe.regen_of(&id).cloned(),
                m.ai.installed_models()
                    .into_iter()
                    .map(|i| (i.id.clone(), i.label.clone(), i.active))
                    .collect::<Vec<_>>(),
                matches!(
                    m.transcribe.transcripts.state(&id),
                    Some(crate::ai::summary::TranscriptState::Present(i)) if i.edited
                ),
            )
        };
        let regen_panel = draft.filter(|_| running.is_none()).map(|d| {
            div()
                .px(px(20.))
                .py(px(12.))
                .child(crate::ui::recording::transcript_card::regen_panel(
                    t, &self.app, &id, "re-tr", &d, &installed, edited,
                ))
        });
        let ui = &self.voice_ui;
        let (lines, turns, rows) = self.voice_turns(s);
        let playhead = s.model.playhead;
        let active = line_at(&lines, playhead);

        // ---- following the playhead (not right after a manual scroll)
        let since = ui.last_manual.get().map(|at: Instant| at.elapsed());
        if let Some(i) = active.and_then(|l| row_of_line(&rows, &turns, l)) {
            if follow_allowed(since, false) {
                if let Some(r) = follow_target(Some(i), ui.followed.get()) {
                    ui.scroll.scroll_to_item(r);
                    ui.followed.set(Some(r));
                }
            }
        } else {
            ui.followed.set(None);
        }

        // ---- header: title, subtitle, language switcher
        let own_language = self.transcript_language();
        let n_speakers = s.voice.speakers.len();
        let sub = [
            (!own_language.is_empty()).then(|| language_name(&own_language)),
            speakers_label(n_speakers),
            Some(tr!("click a time to jump there")),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" \u{00B7} ");
        let switcher = self.language_switcher(t, s, &own_language, cx);
        let header = div()
            .flex_none()
            .px(px(20.))
            .pt(px(20.))
            .pb(px(12.))
            .flex()
            .flex_col()
            .gap(px(12.))
            .border_b_1()
            .border_color(rgba_of(t.row_divider()))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(
                        div()
                            .id("re-panel-title-re-tab-transcript")
                            .role(Role::Heading)
                            .aria_level(2)
                            .aria_label(Inspector::Transcript.label())
                            .text_size(px(16.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(Inspector::Transcript.label()),
                    )
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(rgba_of(t.mist))
                            .child(ellipsis_text(SharedString::from(sub))),
                    ),
            )
            .child(switcher)
            .children(
                ui.translate_open
                    .then(|| self.translate_chooser(t, s, &own_language, cx)),
            )
            // A new transcript is being written (Re-generate, or an automatic run).
            .children(
                running
                    .clone()
                    .map(|text| hint(t, "loader-circle", text).into_any_element()),
            );

        // ---- the rows
        let list_rows: Vec<AnyElement> = rows
            .iter()
            .enumerate()
            .map(|(ri, r)| match r {
                Row::Marker(i) => {
                    let m = &s.voice.markers[*i];
                    marker_row(t, ri, &m.label, m.at)
                }
                Row::Turn(i) => {
                    let turn = &turns[*i];
                    self.turn_row(t, s, ri, turn, &lines, active, cx)
                }
            })
            .collect();
        let list: Stateful<gpui::Div> = div()
            .id("re-transcript-list")
            .role(Role::List)
            .aria_label(tr!("Transcript"))
            .flex()
            .flex_col()
            .children(list_rows);
        let list = if lines.is_empty() {
            list.child(div().p(px(20.)).child(hint(
                t,
                "captions",
                tr!("There are no lines in this language yet."),
            )))
        } else {
            list
        };
        let last_manual = ui.last_manual.clone();
        let list = fill_scroll(t, &ui.scroll, list, move || {
            last_manual.set(Some(Instant::now()))
        });

        // ---- footer: copies and the Speaker names switch
        let export = |id: &'static str, aria: String, fmt: CaptionFormat| {
            small_button_with_icon(t, id, fmt.label(), Some("download"))
                .aria_label(SharedString::from(aria))
                .mono()
                .on_click(cx.listener(move |this, _, _, cx| this.voice_export(fmt, cx)))
        };
        let has_speakers = n_speakers > 0;
        let names_on = ui.speaker_names && has_speakers;
        let footer = div()
            .flex_none()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.))
            .px(px(20.))
            .py(px(12.))
            .border_t_1()
            .border_color(rgba_of(t.border))
            .bg(rgba_of(t.sidebar))
            .child(
                div()
                    .text_size(px(12.5))
                    .text_color(rgba_of(t.mist))
                    .mr(px(4.))
                    .child(tr!("Save a copy")),
            )
            .child(export("re-tr-txt", tr!("Save TXT"), CaptionFormat::Txt))
            .child(export("re-tr-srt", tr!("Save SRT"), CaptionFormat::Srt))
            .child(export("re-tr-vtt", tr!("Save VTT"), CaptionFormat::Vtt))
            .child(
                small_button_with_icon(t, "re-tr-regen", &tr!("Re-generate"), Some("refresh-cw"))
                    .opacity(if running.is_some() { 0.45 } else { 1.0 })
                    .on_click({
                        let (app, cap, busy) = (self.app.clone(), id.clone(), running.is_some());
                        move |_, _, cx| {
                            if busy {
                                return;
                            }
                            let (app, cap) = (app.clone(), cap.clone());
                            cx.defer(move |cx| crate::ui::ai::open_regen(&app, &cap, cx));
                        }
                    }),
            )
            .child(div().flex_1())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .text_size(px(12.5))
                    .text_color(rgba_of(if has_speakers { t.text_2 } else { t.faint }))
                    .child(icon("users", 13., t.mist))
                    .child(shrink_text(tr!("Speaker names")))
                    .child(
                        switch(
                            t,
                            "re-tr-names",
                            tr!("Put the speakers\u{2019} names in the SRT file"),
                            names_on,
                            has_speakers,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.voice_ui.speaker_names = !this.voice_ui.speaker_names;
                            cx.notify();
                        })),
                    ),
            );

        div()
            .id("re-panel-re-tab-transcript")
            .role(Role::Group)
            .aria_label(SharedString::from(tr!(
                "{title} panel",
                title = Inspector::Transcript.label()
            )))
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
            .children(regen_panel)
            .child(list)
            .child(footer)
            .into_any_element()
    }

    /// The language the recording was transcribed in (empty when the daemon did not say).
    pub(super) fn transcript_language(&self) -> String {
        self.ai.transcript_language.clone().unwrap_or_default()
    }

    /// "English original | Tieng Viet translated" and the Translate... button.
    fn language_switcher(
        &self,
        t: &Theme,
        s: &Session,
        own: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let current = self.voice_ui.lang.clone();
        let chip = |code: Option<String>, name: String, kind: String| {
            let active = current == code;
            let key = code.clone().unwrap_or_default();
            segment(
                t,
                SharedString::from(format!("re-lang-{key}")),
                &name,
                active,
            )
            .child(
                div()
                    .ml(px(6.))
                    .mono()
                    .text_size(px(11.))
                    .text_color(rgba_of(t.mist))
                    .child(SharedString::from(kind)),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.voice_ui.lang = code.clone();
                this.voice_ui.followed.set(None);
                cx.notify();
            }))
        };
        let own_name = if own.is_empty() {
            tr_ctx!("language", "Original")
        } else {
            language_name(own)
        };
        let mut group = segmented(t, "re-lang", &tr!("Transcript language")).child(chip(
            None,
            own_name,
            tr_ctx!("transcript language", "original"),
        ));
        for (code, _) in &s.voice.translations {
            group = group.child(chip(
                Some(code.clone()),
                language_name(code),
                tr_ctx!("transcript language", "translated"),
            ));
        }
        for code in &self.voice_ui.translating {
            if s.voice.lines_for(code).is_none() {
                group = group.child(
                    div()
                        .px(px(12.))
                        .text_size(px(type_scale::UI))
                        .text_color(rgba_of(t.mist))
                        .child(SharedString::from(tr!(
                            "{language} \u{2026}",
                            language = language_name(code)
                        ))),
                );
            }
        }
        let busy = !self.voice_ui.translating.is_empty();
        let translate = small_button_with_icon(
            t,
            "re-translate",
            &tr!("Translate\u{2026}"),
            Some("languages"),
        )
        .h(px(scale::CONTROL_H))
        .when(busy, |d| d.opacity(0.5))
        .on_click(cx.listener(move |this, _, _, cx| {
            if this.voice_ui.translating.is_empty() {
                this.voice_ui.translate_open = !this.voice_ui.translate_open;
                cx.notify();
            }
        }));
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.))
            .child(group)
            .child(translate)
            .into_any_element()
    }

    /// The Translate panel (V5): the target language, the engine the daemon will pick (Whisper from
    /// the audio into English, else the text model line by line), the glossary note, then Cancel
    /// and Translate. Without a text model a language other than English cannot be translated
    /// (`translate_captions` would answer `no_text_model`), and into English Whisper needs Large v3
    /// or Medium (`translation_model_missing`); in both cases the panel says so, with a link to the
    /// AI page, and Translate is off instead of failing into a toast.
    fn translate_chooser(
        &self,
        t: &Theme,
        s: &Session,
        own: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let choices = translate_choices(own, &s.voice);
        let Some(pick) = self
            .vp
            .translate_pick
            .filter(|p| choices.contains(p))
            .or_else(|| choices.first().copied())
        else {
            return div().into_any_element();
        };
        let english = pick == "en";
        let text = self.text_state(cx);
        // Known before the click, so a missing model is drawn here and not met as a toast.
        let whisper = self.app.read(cx).ai.whisper_translate();
        let missing = if english {
            match whisper {
                crate::ai::models::WhisperTranslate::Missing { active, large_mb } => Some((
                    tr!("Large v3 isn\u{2019}t installed"),
                    translation_model_note(active, large_mb),
                )),
                _ => None,
            }
        } else if text == super::report_tab::TextState::Missing {
            Some((
                tr!(
                    "{language} needs a text model",
                    language = language_name(pick)
                ),
                tr!(
                    "Whisper only translates into English. Choose a text model on this Mac \
                     on the AI page."
                ),
            ))
        } else {
            None
        };
        let blocked = missing.is_some();
        let from = if own.is_empty() {
            tr!("Times are kept")
        } else {
            tr!(
                "From {language} \u{00B7} times are kept",
                language = language_name(own)
            )
        };
        let chips = choices.iter().map(|&code| {
            segment(
                t,
                SharedString::from(format!("re-translate-{code}")),
                &language_name(code),
                code == pick,
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.vp.translate_pick = Some(code);
                cx.notify();
            }))
        });
        let engine = if english {
            (
                "audio-lines",
                tr!("Whisper, from the audio"),
                tr!(
                    "Fully offline, no text model needed. It uses Large v3 or Medium; Large v3 \
                     Turbo can\u{2019}t translate."
                ),
            )
        } else {
            (
                "languages",
                tr!("Text model, line by line"),
                match &text {
                    super::report_tab::TextState::Ready(model) => tr!(
                        "{model} on this Mac. Any language; each line keeps its start and end.",
                        model = model.clone()
                    ),
                    _ => tr!("Any language; each line keeps its start and end."),
                },
            )
        };
        let engine_card = div()
            .id("re-translate-engine")
            .p(px(12.))
            .flex()
            .items_start()
            .gap(px(10.))
            .rounded(px(scale::RADIUS_MD))
            .border_1()
            .border_color(rgba_of(t.line))
            .bg(rgba_of(t.ground))
            .child(div().pt(px(2.)).child(icon(engine.0, 16., t.text_2)))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .text_size(px(type_scale::UI))
                            .font_weight(FontWeight::MEDIUM)
                            .child(engine.1),
                    )
                    .child(
                        div()
                            .text_size(px(type_scale::CAPTION))
                            .text_color(rgba_of(t.mist))
                            .child(engine.2),
                    ),
            );
        let app = self.app.clone();
        let warn = missing.map(|(title, body)| {
            callout(
                t,
                Tone::Brass,
                "triangle-alert",
                title,
                Some(body.into()),
                Some(
                    small_button_with_icon(
                        t,
                        "re-translate-open-ai",
                        &tr!("Open AI page"),
                        Some("sparkles"),
                    )
                    .on_click(move |_, _, cx| {
                        let app = app.clone();
                        cx.defer(move |cx| {
                            crate::ui::app::set_route(&app, crate::ui::app::Route::Ai, cx)
                        });
                    })
                    .into_any_element(),
                ),
            )
        });
        let go = button_with_icon(
            t,
            "re-translate-go",
            tr!("Translate"),
            ButtonKind::Primary,
            Some("languages"),
        );
        let go = if blocked {
            go.opacity(0.45).cursor_default().into_any_element()
        } else {
            go.on_click(cx.listener(move |this, _, _, cx| this.voice_translate(pick, cx)))
                .into_any_element()
        };
        div()
            .id("re-translate-choices")
            .role(Role::Group)
            .aria_label(tr!("Translate transcript"))
            .flex()
            .flex_col()
            .gap(px(12.))
            .p(px(14.))
            .rounded(px(scale::RADIUS_MD))
            .border_1()
            .border_color(rgba_of(t.border))
            .bg(rgba_of(t.surface))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(icon("languages", 18., t.text_2))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(px(16.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(tr!("Translate transcript")),
                            )
                            .child(
                                div()
                                    .text_size(px(type_scale::CAPTION))
                                    .text_color(rgba_of(t.mist))
                                    .child(from),
                            ),
                    ),
            )
            .child(
                div()
                    .text_size(px(type_scale::CAPTION))
                    .text_color(rgba_of(t.mist))
                    .child(tr!("To")),
            )
            .child(segmented(t, "re-translate-to", &tr!("Translate to")).children(chips))
            .child(
                div()
                    .text_size(px(type_scale::CAPTION))
                    .text_color(rgba_of(t.mist))
                    .child(tr!("Engine \u{00B7} chosen for you")),
            )
            .child(engine_card)
            .children(warn)
            .children((!english).then(|| {
                div()
                    .flex()
                    .items_start()
                    .gap(px(10.))
                    .text_size(px(13.))
                    .text_color(rgba_of(t.text_3))
                    .child(div().pt(px(2.)).child(icon("type", 16., t.mist)))
                    .child(label_fill(tr!("Terms in your glossary stay as written.")))
            }))
            .child(
                // True since edits to a translation are undo steps and survive a re-translate
                // (AI-5.AC5).
                div()
                    .flex()
                    .items_start()
                    .gap(px(10.))
                    .text_size(px(13.))
                    .text_color(rgba_of(t.text_3))
                    .child(div().pt(px(2.)).child(icon("pencil", 16., t.mist)))
                    .child(label_fill(tr!(
                        "Lines you edit in a translation are kept when you translate again."
                    ))),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(
                        button_with_icon(
                            t,
                            "re-translate-cancel",
                            tr_ctx!("button", "Cancel"),
                            ButtonKind::Secondary,
                            None,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.voice_ui.translate_open = false;
                            cx.notify();
                        })),
                    )
                    .child(go),
            )
            .into_any_element()
    }

    /// One turn: the avatar, the name and the time chip (a click seeks), the "overlap" chip, and
    /// the lines (a click on a line seeks to it; the one under the playhead is highlighted).
    #[allow(clippy::too_many_arguments)]
    fn turn_row(
        &self,
        t: &Theme,
        s: &Session,
        row: usize,
        turn: &voice::Turn,
        lines: &[cm_ai::caption::CaptionLine],
        active: Option<usize>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let live = active.is_some_and(|a| turn.lines.contains(&a));
        let who = turn
            .speaker
            .map(|id| (s.voice.name_of(id), speaker_index(&s.voice.speakers, id)));
        let start = turn.start;
        let time = div()
            .id(SharedString::from(format!("re-turn-time-{row}")))
            .role(Role::Button)
            .aria_label(SharedString::from(tr!(
                "Jump to {time}",
                time = clock(start)
            )))
            .mono()
            .tnum()
            .text_size(px(12.5))
            .text_color(rgba_of(t.mist))
            .cursor_pointer()
            .hover({
                let c = rgba_of(t.text);
                move |st| st.text_color(c)
            })
            .child(SharedString::from(clock(start)))
            .on_click(cx.listener(move |this, _, _, cx| this.voice_seek(start, cx)));
        let head = div()
            .flex()
            .items_center()
            .gap(px(10.))
            .children(who.as_ref().map(|(name, ix)| {
                let c = speaker_colour(*ix);
                div()
                    .text_size(px(type_scale::UI))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgba_of(if name.starts_with("Speaker ") {
                        c
                    } else {
                        t.text
                    }))
                    .child(SharedString::from(name.clone()))
            }))
            .child(time)
            .children(turn.overlap.then(|| {
                div()
                    .px(px(8.))
                    .py(px(1.))
                    .rounded(px(999.))
                    .border_1()
                    .border_color(rgba_of(t.warning.with_alpha(45)))
                    .bg(rgba_of(t.warning.with_alpha(10)))
                    .text_size(px(11.5))
                    .text_color(rgba_of(t.warning))
                    .child(tr_ctx!("transcript turn", "overlap"))
            }));
        let body = div()
            .flex()
            .flex_col()
            .gap(px(2.))
            .children(turn.lines.iter().map(|&li| {
                let line = &lines[li];
                let is_active = active == Some(li);
                let at = line.start;
                div()
                    .id(SharedString::from(format!("re-line-{row}-{li}")))
                    .role(Role::Button)
                    .aria_label(SharedString::from(line.text.clone()))
                    .text_size(px(14.5))
                    .line_height(px(22.))
                    .text_color(rgba_of(if is_active { t.text } else { t.text_2 }))
                    .cursor_pointer()
                    .child(SharedString::from(line.text.replace('\n', " ")))
                    .on_click(cx.listener(move |this, _, _, cx| this.voice_seek(at, cx)))
            }));
        let avatar_el = who.map(|(name, ix)| avatar(t, &name, speaker_colour(ix), 28.));
        div()
            .id(SharedString::from(format!("re-turn-{row}")))
            .role(Role::ListItem)
            .px(px(20.))
            .py(px(14.))
            .flex()
            .gap(px(12.))
            .border_b_1()
            .border_color(rgba_of(t.row_divider()))
            .border_l_2()
            .border_color(if live {
                rgba_of(t.text)
            } else {
                gpui::transparent_black().into()
            })
            .when(live, |d| d.bg(rgba_of(t.raised.with_alpha(55))))
            .children(avatar_el.map(|a| div().pt(px(1.)).child(a)))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(head)
                    .child(body),
            )
            .into_any_element()
    }

    /// TXT, SRT or VTT of the language shown, written to the recording's `exports/` folder; SRT
    /// has the speakers' names when the switch is on.
    pub(super) fn voice_export(&mut self, format: CaptionFormat, cx: &mut Context<Self>) {
        let (Some(id), Some(client)) = (self.id.clone(), crate::ui::ai::client_of(&self.app, cx))
        else {
            return;
        };
        let lang = self.voice_ui.lang.clone();
        let names = self.voice_ui.speaker_names;
        let timestamps = self.app.read(cx).transcribe.txt_timestamps;
        let app = self.app.clone();
        cx.spawn(async move |_this, cx| {
            let res = client
                .export_captions_for(&id, format, timestamps, lang.as_deref(), names)
                .await;
            cx.update(|cx| {
                let spec = match res {
                    Ok(path) => crate::toast::ToastSpec::new(
                        crate::toast::ToastKind::Success,
                        tr!("{format} saved", format = format.label()),
                    )
                    .body(path.display().to_string())
                    .button(
                        tr_ctx!("button", "Show in Finder"),
                        crate::toast::ToastAction::RevealPath(path.display().to_string()),
                    ),
                    Err(e) => crate::flows::error_toast_live(
                        || tr!("Couldn\u{2019}t save the transcript file"),
                        e.message.clone(),
                    ),
                };
                crate::ui::app::push_toast(&app, spec, cx);
            });
        })
        .detach();
    }
}

/// A marker between turns: a bookmark glyph, the label, the time and a hairline.
/// What to do when Whisper has no model that can translate (V5): says which active model only
/// transcribes, and what to install; nothing is downloaded from here.
fn translation_model_note(active: Option<String>, large_mb: Option<u32>) -> String {
    let size = large_mb.map(crate::ai::models::size_label);
    match (active, size) {
        (Some(model), Some(size)) => tr!(
            "Your active model, {model}, only transcribes. Install Large v3 ({size}) or Medium \
             on the AI page to translate to English.",
            model = model,
            size = size
        ),
        (Some(model), None) => tr!(
            "Your active model, {model}, only transcribes. Install Large v3 or Medium on the AI \
             page to translate to English.",
            model = model
        ),
        (None, Some(size)) => tr!(
            "Install Large v3 ({size}) or Medium on the AI page to translate to English.",
            size = size
        ),
        (None, None) => tr!("Install Large v3 or Medium on the AI page to translate to English."),
    }
}

fn marker_row(t: &Theme, row: usize, label: &str, at: cm_core::MediaTime) -> AnyElement {
    div()
        .id(SharedString::from(format!("re-marker-row-{row}")))
        .role(Role::ListItem)
        .aria_label(SharedString::from(tr!(
            "Marker {label} at {time}",
            label = label,
            time = clock(at)
        )))
        .px(px(20.))
        .py(px(8.))
        .flex()
        .items_center()
        .gap(px(8.))
        .text_size(px(13.))
        .text_color(rgba_of(t.text_2))
        .child(icon("bookmark", 14., t.mist))
        .child(
            div()
                .font_weight(FontWeight::MEDIUM)
                .child(SharedString::from(if label.is_empty() {
                    tr_ctx!("marker", "Marker")
                } else {
                    label.to_owned()
                })),
        )
        .child(
            div()
                .mono()
                .tnum()
                .text_size(px(12.))
                .text_color(rgba_of(t.mist))
                .child(SharedString::from(clock(at))),
        )
        .child(div().flex_1().h(px(1.)).bg(rgba_of(t.row_divider())))
        .into_any_element()
}
