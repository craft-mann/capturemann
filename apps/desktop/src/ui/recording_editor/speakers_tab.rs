//! The Speakers tab (redesign V5, AI-8). Labelled state: one row per speaker (colour, name,
//! share of the talk time with a bar, Play sample, Rename and Merge into...), the "My microphone
//! only hears me" switch and the privacy line. Not-run state: an empty card, "How many people?",
//! the notice when there is no speaker model and Open AI page, with Find speakers dimmed until a
//! model is installed.
//!
//! Renaming and merging go through `set_speakers`, Find speakers through `diarize`; nothing here
//! calls an engine. This version ships no speaker model (spike S7 has not picked one), so the
//! daemon answers `no_speaker_model` and the tab says "not available yet" instead of pretending.
//! Not seen by eye yet (`tests/manual/VOC-2.md`).

use cm_ai::speakers::Speaker;
use cm_i18n::{tr, tr_ctx};
use gpui::{
    div, prelude::*, px, relative, AnyElement, Context, FontWeight, Role, SharedString, Stateful,
};

use super::audio_view::avatar;
use super::tabs::{panel, Inspector};
use super::voice_fields::SpeakerNote;
use super::{RecordingEditor, Session};
use crate::ai::jobs::VoiceJobKind;
use crate::ai::voice::SpeakerCard;
use crate::recedit::voice::speaker_colour;
use crate::recedit::voice_panels::{self as rules, NameError, PEOPLE_CHOICES};
use crate::theme::{scale, type_scale, Color, Theme};
use crate::ui::components::{button_with_icon, icon, label_fill, shrink_text, switch, ButtonKind};
use crate::ui::kit::{callout, Tone};
use crate::ui::paint::{rgba_of, TextExt};
use crate::ui::settings::widgets::{segment, segmented, small_button_with_icon};

/// A 30 px icon button of a speaker row.
pub(super) fn row_icon(
    t: &Theme,
    id: impl Into<gpui::ElementId>,
    glyph: &'static str,
    aria: impl Into<SharedString>,
) -> Stateful<gpui::Div> {
    let (hover, focus) = (rgba_of(t.raised), rgba_of(t.text));
    div()
        .id(id)
        .role(Role::Button)
        .aria_label(aria.into())
        .focusable()
        .tab_stop(true)
        .size(px(30.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(scale::RADIUS_SM))
        .border_1()
        .border_color(gpui::transparent_black())
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .focus_visible(move |s| s.border_color(focus))
        .child(icon(glyph, 16., t.text_3))
}

/// A bordered card of the panels.
pub(super) fn card(t: &Theme) -> gpui::Div {
    div()
        .rounded(px(scale::RADIUS_LG))
        .border_1()
        .border_color(rgba_of(t.border))
        .bg(rgba_of(t.ground))
}

/// "4 found from the voices" (the scope line).
fn found_line(n: usize) -> String {
    tr!("{n} found from the voices \u{00B7} whole recording", n = n)
}

impl RecordingEditor {
    pub(super) fn speakers_panel(
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
            .voice_job(&id, VoiceJobKind::Speakers)
            .cloned();
        let card_state = self.app.read(cx).ai.voice.speakers.card();
        let has_speakers = !s.voice.speakers.is_empty();
        let scope = if has_speakers {
            found_line(s.voice.speakers.len())
        } else {
            tr!("Who spoke when, from the voices")
        };
        let mut body: Vec<AnyElement> = Vec::new();
        if has_speakers {
            body.push(self.speaker_list(t, s, cx));
        } else {
            body.push(empty_card(t));
            body.push(self.people_row(t, cx));
        }
        // The notice: what Find speakers needs, or why a run failed.
        match (&self.vp.speaker_note, &card_state, has_speakers) {
            (Some(SpeakerNote::Failed(msg)), _, _) => body.push(
                callout(
                    t,
                    Tone::Danger,
                    "circle-alert",
                    tr!("Couldn\u{2019}t find the speakers"),
                    Some(SharedString::from(msg.clone())),
                    None,
                )
                .into_any_element(),
            ),
            (Some(SpeakerNote::NoModel), _, _) | (None, SpeakerCard::Unavailable, false) => body
                .push(
                    callout(
                        t,
                        Tone::Brass,
                        "download",
                        tr!("Speaker labels aren\u{2019}t available yet"),
                        Some(
                            tr!(
                            "They need a separate speaker model, and this version doesn\u{2019}t \
                             include one. Nothing is downloaded until you ask."
                        )
                            .into(),
                        ),
                        None,
                    )
                    .into_any_element(),
                ),
            (None, SpeakerCard::Offer(m), false) => body.push(
                callout(
                    t,
                    Tone::Brass,
                    "download",
                    tr!("The speaker model isn\u{2019}t installed"),
                    Some(
                        tr!(
                            "It\u{2019}s a separate download on the AI page ({label}, {licence}, \
                             {size} MB); the size is shown first and nothing is fetched until you \
                             ask.",
                            label = m.label.clone(),
                            licence = m.licence.clone(),
                            size = m.size_mb
                        )
                        .into(),
                    ),
                    None,
                )
                .into_any_element(),
            ),
            _ => {}
        }
        body.push(self.headset_row(t, s, cx));
        body.push(privacy_line(t));

        // The foot: Open AI page and Find speakers (dimmed without a model), or the progress.
        let ready = matches!(card_state, SpeakerCard::Installed(_));
        let mut foot: Vec<AnyElement> = Vec::new();
        if let Some(job) = &running {
            let label = match job.progress {
                Some(p) => tr!(
                    "Finding speakers\u{2026} {percent}%",
                    percent = (p * 100.0).round() as u32
                ),
                None => tr!("Finding speakers\u{2026}"),
            };
            foot.push(
                button_with_icon(
                    t,
                    "re-find-speakers",
                    label,
                    ButtonKind::Secondary,
                    Some("loader-circle"),
                )
                .opacity(0.6)
                .cursor_default()
                .into_any_element(),
            );
        } else {
            if !ready {
                let app = self.app.clone();
                foot.push(
                    button_with_icon(
                        t,
                        "re-open-ai",
                        tr!("Open AI page"),
                        ButtonKind::Secondary,
                        Some("sparkles"),
                    )
                    .on_click(move |_, _, cx| {
                        let app = app.clone();
                        cx.defer(move |cx| {
                            crate::ui::app::set_route(&app, crate::ui::app::Route::Ai, cx)
                        });
                    })
                    .into_any_element(),
                );
            }
            let label = if has_speakers {
                tr!("Find again")
            } else {
                tr!("Find speakers")
            };
            let kind = if has_speakers {
                ButtonKind::Secondary
            } else {
                ButtonKind::Primary
            };
            let b = button_with_icon(
                t,
                "re-find-speakers",
                label,
                kind,
                Some(if has_speakers { "refresh-cw" } else { "users" }),
            );
            foot.push(if ready {
                b.on_click(cx.listener(|this, _, _, cx| this.speakers_find(cx)))
                    .into_any_element()
            } else {
                b.opacity(0.45).cursor_default().into_any_element()
            });
        }
        panel(t, Inspector::Speakers, scope, body, foot)
    }

    /// "How many people?": Not sure, 2, 3, 4, 5+.
    fn people_row(&self, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let mut seg = segmented(t, "re-people", &tr!("How many people?"));
        for choice in PEOPLE_CHOICES {
            let label = match choice {
                None => tr!("Not sure"),
                Some(n) if n >= 5 => format!("{n}+"),
                Some(n) => n.to_string(),
            };
            let on = self.vp.people == choice;
            seg = seg.child(
                segment(
                    t,
                    SharedString::from(format!("re-people-{}", choice.unwrap_or(0))),
                    &label,
                    on,
                )
                .min_w(px(36.))
                .px(px(10.))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.vp.people = choice;
                    cx.notify();
                })),
            );
        }
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap(px(10.))
            .child(
                shrink_text(tr!("How many people?"))
                    .text_size(px(type_scale::UI))
                    .text_color(rgba_of(t.text_2)),
            )
            .child(seg)
            .into_any_element()
    }

    /// "My microphone only hears me" (`headset`): needs both a microphone and a system track.
    fn headset_row(&self, t: &Theme, s: &Session, cx: &mut Context<Self>) -> AnyElement {
        let usable = s.voice.has_mic && s.voice.has_system;
        let on = self.vp.headset && usable;
        let hint = if usable {
            tr!("For headset calls: the mic track is you, only the call is split into voices.")
        } else {
            tr!("Needs a microphone track and a system audio track.")
        };
        let sw = switch(
            t,
            "re-headset",
            tr!("My microphone only hears me"),
            on,
            usable,
        );
        div()
            .flex()
            .items_start()
            .gap(px(10.))
            .child(div().pt(px(2.)).child(icon("mic", 16., t.mist)))
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
                            .child(tr!("My microphone only hears me")),
                    )
                    .child(
                        div()
                            .text_size(px(type_scale::CAPTION))
                            .text_color(rgba_of(t.mist))
                            .child(hint),
                    ),
            )
            .child(if usable {
                sw.on_click(cx.listener(|this, _, _, cx| {
                    this.vp.headset = !this.vp.headset;
                    cx.notify();
                }))
                .into_any_element()
            } else {
                sw.into_any_element()
            })
            .into_any_element()
    }

    /// The card with one row per speaker.
    fn speaker_list(&self, t: &Theme, s: &Session, cx: &mut Context<Self>) -> AnyElement {
        let shares = rules::talk_shares(&s.voice.speakers, &s.voice.segments);
        let n = s.voice.speakers.len();
        let rows = s
            .voice
            .speakers
            .iter()
            .enumerate()
            .map(|(ix, sp)| {
                let share = shares.get(ix).map_or(0.0, |(_, f)| *f);
                self.speaker_row(t, s, sp, ix, share, ix + 1 == n, cx)
            })
            .collect::<Vec<_>>();
        card(t)
            .id("re-speakers")
            .role(Role::List)
            .aria_label(tr!("Speakers"))
            .overflow_hidden()
            .flex()
            .flex_col()
            .children(rows)
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn speaker_row(
        &self,
        t: &Theme,
        s: &Session,
        sp: &Speaker,
        ix: usize,
        share: f32,
        last: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = sp.id;
        let colour: Color = speaker_colour(ix);
        let generic = sp.name.starts_with("Speaker ");
        let renaming = self.vp.renaming == Some(id);
        let name_el: AnyElement = if renaming {
            div()
                .flex_1()
                .min_w(px(0.))
                .child(self.vp.rename.clone())
                .into_any_element()
        } else {
            label_fill(SharedString::from(sp.name.clone()))
                .text_size(px(15.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgba_of(if generic { colour } else { t.text }))
                .into_any_element()
        };
        let pct = rules::percent(share);
        let can_play = s.voice.segments.iter().any(|g| g.speaker == id);
        let play = row_icon(
            t,
            SharedString::from(format!("re-speaker-play-{id}")),
            "play",
            tr!("Play a sample of {name}", name = sp.name.clone()),
        );
        let play = if can_play {
            play.on_click(cx.listener(move |this, _, _, cx| this.speaker_play_sample(id, cx)))
                .into_any_element()
        } else {
            play.opacity(0.4).cursor_default().into_any_element()
        };
        let rename = row_icon(
            t,
            SharedString::from(format!("re-speaker-rename-{id}")),
            "pencil",
            tr!("Rename {name}", name = sp.name.clone()),
        )
        .on_click(cx.listener(move |this, _, window, cx| {
            this.speaker_start_rename(id, cx);
            let focus = this.vp.rename.read(cx).focus.clone();
            window.focus(&focus, cx);
        }));
        let merge = row_icon(
            t,
            SharedString::from(format!("re-speaker-merge-{id}")),
            "merge",
            tr!("Merge {name} into another speaker", name = sp.name.clone()),
        );
        let merge = if s.voice.speakers.len() > 1 {
            merge
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.vp.renaming = None;
                    this.vp.merging = match this.vp.merging {
                        Some((f, _)) if f == id => None,
                        _ => Some((id, None)),
                    };
                    cx.notify();
                }))
                .into_any_element()
        } else {
            merge.opacity(0.4).cursor_default().into_any_element()
        };
        let head = div()
            .flex()
            .items_center()
            .gap(px(10.))
            .child(avatar(t, &sp.name, colour, 28.))
            .child(name_el)
            .child(
                div()
                    .flex_none()
                    .mono()
                    .tnum()
                    .text_size(px(12.5))
                    .text_color(rgba_of(t.mist))
                    .child(SharedString::from(format!("{pct}%"))),
            )
            .child(play)
            .child(rename)
            .child(merge);
        let bar = div().h(px(4.)).rounded(px(2.)).bg(rgba_of(t.raised)).child(
            div()
                .h_full()
                .w(relative(share.clamp(0.0, 1.0)))
                .rounded(px(2.))
                .bg(rgba_of(colour)),
        );
        let error = renaming.then_some(self.vp.rename_error).flatten().map(|e| {
            div()
                .text_size(px(type_scale::CAPTION))
                .text_color(rgba_of(t.warning))
                .child(match e {
                    NameError::Empty => tr!("Type a name."),
                    NameError::TooLong => tr!("Use 60 characters or fewer."),
                    NameError::Taken => tr!("Another speaker already has that name."),
                })
        });
        let merging = match self.vp.merging {
            Some((from, into)) if from == id => Some(self.merge_row(t, s, from, into, cx)),
            _ => None,
        };
        div()
            .id(SharedString::from(format!("re-speaker-{id}")))
            .role(Role::ListItem)
            .px(px(14.))
            .py(px(12.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .when(!last, |d| {
                d.border_b_1().border_color(rgba_of(t.row_divider()))
            })
            .child(head)
            .child(bar)
            .children(error)
            .children(merging)
            .into_any_element()
    }

    /// "Merge into [speaker]  Merge": the chips are the other speakers.
    fn merge_row(
        &self,
        t: &Theme,
        s: &Session,
        from: u32,
        into: Option<u32>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let chips = rules::merge_targets(&s.voice.speakers, from)
            .into_iter()
            .map(|sp| {
                let target = sp.id;
                let on = into == Some(target);
                segment(
                    t,
                    SharedString::from(format!("re-merge-into-{target}")),
                    &sp.name,
                    on,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.vp.merging = Some((from, Some(target)));
                    cx.notify();
                }))
            })
            .collect::<Vec<_>>();
        let go =
            button_with_icon(t, "re-merge-go", tr!("Merge"), ButtonKind::Primary, None).h(px(32.));
        let go = if into.is_some() {
            go.on_click(cx.listener(|this, _, _, cx| this.speaker_commit_merge(cx)))
                .into_any_element()
        } else {
            go.opacity(0.45).cursor_default().into_any_element()
        };
        div()
            .id("re-merge-row")
            .p(px(10.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .rounded(px(scale::RADIUS_MD))
            .border_1()
            .border_color(rgba_of(t.line))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .text_size(px(type_scale::UI))
                    .text_color(rgba_of(t.text_2))
                    .child(icon("merge", 16., t.text_3))
                    .child(label_fill(tr!("Merge into"))),
            )
            .child(segmented(t, "re-merge-targets", &tr!("Merge into")).children(chips))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(
                        small_button_with_icon(
                            t,
                            "re-merge-cancel",
                            &tr_ctx!("button", "Cancel"),
                            None,
                        )
                        .h(px(32.))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.vp.merging = None;
                            cx.notify();
                        })),
                    )
                    .child(go),
            )
            .into_any_element()
    }
}

/// The not-run card: a dashed frame, the glyph tile, the title and a line.
fn empty_card(t: &Theme) -> AnyElement {
    div()
        .id("re-speakers-empty")
        .px(px(20.))
        .py(px(28.))
        .flex()
        .flex_col()
        .items_center()
        .gap(px(10.))
        .rounded(px(scale::RADIUS_LG))
        .border_1()
        .border_dashed()
        .border_color(rgba_of(t.line))
        .child(
            div()
                .size(px(44.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(12.))
                .bg(rgba_of(t.raised))
                .child(icon("users", 20., t.text_2)),
        )
        .child(
            shrink_text(tr!("Works with one microphone on the table"))
                .text_size(px(15.))
                .font_weight(FontWeight::MEDIUM)
                .text_center(),
        )
        .child(
            shrink_text(tr!(
                "Speakers are told apart by their voices, not by device."
            ))
            .text_size(px(13.))
            .text_color(rgba_of(t.text_3))
            .text_center(),
        )
        .into_any_element()
}

/// "Voices are compared in memory only..." with a lock.
fn privacy_line(t: &Theme) -> AnyElement {
    div()
        .flex()
        .items_start()
        .gap(px(10.))
        .child(div().pt(px(2.)).child(icon("lock", 16., t.mist)))
        .child(
            label_fill(tr!(
                "Voices are compared in memory only. No voiceprint is saved, and people are \
                 never recognised across recordings."
            ))
            .text_size(px(13.))
            .text_color(rgba_of(t.text_3)),
        )
        .into_any_element()
}
