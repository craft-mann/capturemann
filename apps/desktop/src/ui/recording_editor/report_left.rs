//! The left column of the Report view (redesign V6, AI-9): the cited moment with its context
//! lines and a player, the overview strip with one tick per citation (coloured by section), the
//! legend and the two notes. It replaces the "now speaking" card and the lanes while the Report
//! tab shows a report of an audio recording. Also the sends of the Report view that touch the
//! checkboxes (`check_report_item`).
//!
//! A click on a time chip in the report seeks and remembers which item it came from
//! ([`Cite`]); the card then shows the line that holds that moment and its neighbours. Not seen
//! by eye yet (`tests/manual/VOC-2.md`).

use cm_i18n::{tr, tr_ctx};
use gpui::{div, prelude::*, px, AnyElement, Context, FontWeight, Role, SharedString};

use super::audio_view::avatar;
use super::tabs::Inspector;
use super::{RecordingEditor, Session};
use crate::recedit::keys::Cmd;
use crate::recedit::voice::{clock_tenths, speaker_colour, speaker_index};
use crate::recedit::voice_panels::{self as rules, TickKind};
use crate::theme::{scale, type_scale, Color, Theme};
use crate::ui::components::{button_with_icon, dot, icon, label_fill, shrink_text, ButtonKind};
use crate::ui::paint::{rgba_of, TextExt};

/// The item of the report a time chip was clicked on.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Cite {
    pub section: String,
    pub section_title: String,
    /// The item's place in its section, from 0.
    pub index: usize,
    /// The cited second.
    pub at: f64,
}

/// What the left column remembers.
#[derive(Debug, Default)]
pub(super) struct ReportLeft {
    pub cite: Option<Cite>,
}

/// The colour of a tick kind (the legend uses the same).
fn tick_colour(t: &Theme, k: TickKind) -> Color {
    match k {
        TickKind::Decision => t.success,
        TickKind::Action => t.agent,
        TickKind::Question => t.warning,
        TickKind::Marked => t.text,
        TickKind::Other => t.mist,
    }
}

impl RecordingEditor {
    /// The Report tab shows a report of an audio recording: the left column is the V6 one.
    pub(super) fn report_left_open(&self, s: &Session, cx: &gpui::App) -> bool {
        s.voice.audio && self.tab == Inspector::Report && self.report_view_open(s, cx)
    }

    /// A time chip of the report was clicked: seek and remember the item.
    pub(super) fn report_cite(
        &mut self,
        section: &str,
        section_title: &str,
        index: usize,
        at: f64,
        cx: &mut Context<Self>,
    ) {
        self.rleft.cite = Some(Cite {
            section: section.to_owned(),
            section_title: section_title.to_owned(),
            index,
            at,
        });
        self.voice_seek(rules::at_time(at), cx);
    }

    /// A checkbox: `check_report_item` as the user (one undo step). The report is read again when
    /// the revision arrives.
    pub(super) fn report_check(
        &mut self,
        report_id: String,
        section: String,
        index: usize,
        on: bool,
        cx: &mut Context<Self>,
    ) {
        let (Some(id), Some(client)) = (self.id.clone(), crate::ui::ai::client_of(&self.app, cx))
        else {
            return;
        };
        let app = self.app.clone();
        cx.spawn(async move |this, cx| {
            let res = client
                .check_report_item(&id, &report_id, &section, index, on)
                .await;
            this.update(cx, |this, cx| {
                this.voice_ui.stale = true;
                if let Err(e) = res {
                    let msg = e.message.clone();
                    cx.defer(move |cx| {
                        crate::ui::app::push_toast(
                            &app,
                            crate::flows::error_toast_live(
                                || tr!("Couldn\u{2019}t tick the item"),
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

    /// The column: the cited moment, the overview strip with its legend, the notes.
    pub(super) fn report_left_column(
        &self,
        t: &Theme,
        s: &Session,
        r: &cm_ai::report::store::Report,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let sections = rules::sections(r);
        div()
            .flex_1()
            .min_w(px(0.))
            .min_h(px(0.))
            .flex()
            .flex_col()
            .gap(px(16.))
            .child(self.cited_card(t, s, cx))
            .child(self.citation_overview(t, s, &sections, cx))
            .child(self.report_notes(t))
            .into_any_element()
    }

    /// "Cited by "Decisions . 1" - playing from 18:22", three lines and the player.
    fn cited_card(&self, t: &Theme, s: &Session, cx: &mut Context<Self>) -> AnyElement {
        let (lines, turns, _) = self.voice_turns(s);
        let spans: Vec<(f64, f64)> = lines
            .iter()
            .map(|l| (l.start.as_secs_f64(), l.end.as_secs_f64()))
            .collect();
        let cite = self.rleft.cite.as_ref();
        let ctx = cite.and_then(|c| rules::cited_context(&spans, c.at));
        let speaker_of = |line: usize| {
            turns
                .iter()
                .find(|t| t.lines.contains(&line))
                .and_then(|t| t.speaker)
        };

        let head: AnyElement = match cite {
            Some(c) => div()
                .flex()
                .items_center()
                .gap(px(8.))
                .text_size(px(type_scale::UI))
                .text_color(rgba_of(t.text_3))
                .child(icon("captions", 15., t.mist))
                .child(label_fill(tr!(
                    "Cited by \u{201C}{section} \u{00B7} {n}\u{201D}, playing from {time}",
                    section = c.section_title.clone(),
                    n = rules::cite_number(c.index),
                    time = rules::at_label(c.at)
                )))
                .into_any_element(),
            None => div()
                .flex()
                .items_center()
                .gap(px(8.))
                .text_size(px(type_scale::UI))
                .text_color(rgba_of(t.mist))
                .child(icon("captions", 15., t.mist))
                .child(label_fill(tr!(
                    "Click a time in the report to see the moment it comes from."
                )))
                .into_any_element(),
        };

        let line_row = |i: usize, hit: bool| -> AnyElement {
            let l = &lines[i];
            let sp = speaker_of(i);
            let (name, colour) = match sp {
                Some(id) => (
                    s.voice.name_of(id),
                    speaker_colour(speaker_index(&s.voice.speakers, id)),
                ),
                None => (String::new(), t.line),
            };
            let mut row = div()
                .id(SharedString::from(format!("re-cited-line-{i}")))
                .flex()
                .items_start()
                .gap(px(12.))
                .p(px(12.))
                .rounded(px(scale::RADIUS_MD))
                .child(avatar(
                    t,
                    if name.is_empty() { "?" } else { &name },
                    colour,
                    32.,
                ))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
                                .items_baseline()
                                .gap(px(8.))
                                .children((!name.is_empty()).then(|| {
                                    shrink_text(name.clone())
                                        .text_size(px(type_scale::UI))
                                        .font_weight(FontWeight::SEMIBOLD)
                                }))
                                .child(
                                    div()
                                        .mono()
                                        .tnum()
                                        .text_size(px(type_scale::CAPTION))
                                        .text_color(rgba_of(t.mist))
                                        .child(SharedString::from(clock_tenths(l.start))),
                                ),
                        )
                        .child(
                            div()
                                .text_size(px(if hit { 18. } else { 16. }))
                                .line_height(px(if hit { 26. } else { 24. }))
                                .text_color(rgba_of(if hit { t.text } else { t.text_3 }))
                                .child(SharedString::from(l.text.replace('\n', " "))),
                        ),
                );
            if hit {
                row = row
                    .bg(rgba_of(t.ground))
                    .border_l_2()
                    .border_color(rgba_of(t.text));
            }
            row.into_any_element()
        };
        let context: Vec<AnyElement> = match ctx {
            Some(c) => c
                .before
                .map(|i| line_row(i, false))
                .into_iter()
                .chain(Some(line_row(c.hit, true)))
                .chain(c.after.map(|i| line_row(i, false)))
                .collect(),
            None => Vec::new(),
        };

        let m = &s.model;
        let play = div()
            .id("re-cited-play")
            .role(Role::Button)
            .aria_label(if m.playing {
                tr_ctx!("button", "Pause")
            } else {
                tr_ctx!("button", "Play")
            })
            .focusable()
            .tab_stop(true)
            .size(px(52.))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .rounded(px(26.))
            .bg(rgba_of(t.primary_bg))
            .cursor_pointer()
            .hover({
                let h = rgba_of(t.text_2);
                move |st| st.bg(h)
            })
            .focus_visible({
                let f = rgba_of(t.rec);
                move |st| st.border_1().border_color(f)
            })
            .child(icon(
                if m.playing { "rec-pause" } else { "rec-play" },
                20.0,
                t.primary_fg,
            ))
            .on_click(cx.listener(|this, _, _, cx| this.run(Cmd::PlayPause, cx)));
        let player = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(16.))
            .child(play)
            .child(
                div()
                    .mono()
                    .tnum()
                    .text_size(px(22.))
                    .flex()
                    .gap(px(8.))
                    .child(
                        div()
                            .text_color(rgba_of(t.text))
                            .child(SharedString::from(clock_tenths(m.playhead))),
                    )
                    .child(div().text_color(rgba_of(t.mist)).child("/"))
                    .child(
                        div()
                            .text_color(rgba_of(t.mist))
                            .child(SharedString::from(clock_tenths(m.basis.end))),
                    ),
            )
            .child(div().flex_1())
            .child(
                button_with_icon(
                    t,
                    "re-open-transcript",
                    tr!("Open in transcript"),
                    ButtonKind::Secondary,
                    Some("captions"),
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.tab = Inspector::Transcript;
                    cx.notify();
                })),
            );

        div()
            .id("re-cited-moment")
            .role(Role::Group)
            .aria_label(tr!("Cited moment"))
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(12.))
            .p(px(20.))
            .rounded(px(scale::RADIUS_XL))
            .border_1()
            .border_color(rgba_of(t.border))
            .bg(rgba_of(t.surface))
            .child(head)
            .children(context)
            .child(player)
            .into_any_element()
    }

    /// "Where the report's items come from": the whole recording with a tick per citation.
    fn citation_overview(
        &self,
        t: &Theme,
        s: &Session,
        sections: &[rules::SectionView],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let end = s.model.basis.end;
        let ticks: Vec<(f32, Color)> = rules::citation_ticks(sections)
            .into_iter()
            .filter(|(_, k)| *k != TickKind::Marked) // the strip draws the markers itself
            .map(|(at, k)| {
                (
                    crate::recedit::voice::fraction_of(rules::at_time(at), end),
                    tick_colour(t, k),
                )
            })
            .collect();
        let legend = |k: TickKind, label: String| {
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .text_size(px(type_scale::CAPTION))
                .text_color(rgba_of(t.text_3))
                .child(dot(tick_colour(t, k), 10.))
                .child(shrink_text(label))
        };
        div()
            .id("re-citation-overview")
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(10.))
            .p(px(20.))
            .rounded(px(scale::RADIUS_XL))
            .border_1()
            .border_color(rgba_of(t.border))
            .bg(rgba_of(t.surface))
            .child(self.overview_strip_with(t, s, ticks, false, cx))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_x(px(16.))
                    .gap_y(px(6.))
                    .child(legend(
                        TickKind::Decision,
                        tr_ctx!("report section", "Decisions"),
                    ))
                    .child(legend(
                        TickKind::Action,
                        tr_ctx!("report section", "Action items"),
                    ))
                    .child(legend(
                        TickKind::Question,
                        tr_ctx!("report section", "Open questions"),
                    ))
                    .child(legend(
                        TickKind::Marked,
                        tr_ctx!("report section", "Marked moments"),
                    )),
            )
            .into_any_element()
    }

    /// The two notes under the strip (a dashed box).
    fn report_notes(&self, t: &Theme) -> AnyElement {
        let note = |glyph: &'static str, text: String| {
            div()
                .flex()
                .items_start()
                .gap(px(10.))
                .text_size(px(type_scale::UI))
                .text_color(rgba_of(t.text_3))
                .child(icon(glyph, 15., t.mist))
                .child(label_fill(text))
        };
        div()
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(8.))
            .p(px(16.))
            .rounded(px(scale::RADIUS_XL))
            .border_1()
            .border_dashed()
            .border_color(rgba_of(t.line))
            .child(note(
                "bookmark",
                tr!("Markers you added while recording appear as \u{201C}Marked moments\u{201D}."),
            ))
            .child(note(
                "users",
                tr!("Owners can only be attendees or speakers; anyone else becomes \u{201C}Unassigned\u{201D}."),
            ))
            .into_any_element()
    }
}
