//! The zoom lane under the timeline and the zoom, cursor and reframe cards of the inspector
//! (EDT-4, EDT-5). Drawing and events only; what each control asks of the daemon is decided in
//! `crate::recedit::viewedit` (unit tested).

use cm_core::{ClickEffect, ReframeMode, Smoothing};
use cm_i18n::{tr, tr_ctx, trn};
use gpui::{div, prelude::*, px, AnyElement, Context, Role, SharedString};

use super::tabs::{ease_diagram, field_row, hint, micro, panel, tile_choice, Inspector, KEY_MARK};
use super::{RecordingEditor, Session};
use crate::recedit::geom::LABEL_W;
use crate::recedit::keys::Cmd;
use crate::recedit::viewedit::{
    PreviewAspect, ZoomAdjust, FOCUS_STEP, LENGTH_STEP_SECS, SCALE_STEP,
};
use crate::theme::{type_scale, Theme};
use crate::ui::components::{button_with_icon, icon, keycap, switch, ButtonKind};
use crate::ui::kit::{callout, Tone};
use crate::ui::paint::{rgba_of, TextExt};
use crate::ui::settings::widgets::{segment, segmented, small_button_with_icon};

const LANE_H: f32 = 26.0;

impl RecordingEditor {
    // ---- handlers ------------------------------------------------------------------------

    fn do_zoom(&mut self, a: ZoomAdjust, cx: &mut Context<Self>) {
        let Some(s) = self.session() else { return };
        let reqs = s.model.adjust_zoom(a);
        self.send(reqs, cx);
    }

    fn do_remove_zoom(&mut self, cx: &mut Context<Self>) {
        let Some(s) = self.session() else { return };
        let reqs = s.model.remove_zoom();
        self.send(reqs, cx);
    }

    fn do_cursor(
        &mut self,
        f: impl FnOnce(&mut crate::recedit::EditorModel) -> Vec<crate::recedit::EditRequest>,
        cx: &mut Context<Self>,
    ) {
        let Some(s) = self.session() else { return };
        let reqs = f(&mut s.model);
        self.send(reqs, cx);
    }

    fn do_preview_aspect(&mut self, a: PreviewAspect, cx: &mut Context<Self>) {
        if let Some(s) = self.session() {
            s.model.set_preview_aspect(a);
        }
        cx.notify();
    }

    // ---- the zoom lane -------------------------------------------------------------------

    /// One strip of zoom regions on the timeline's time axis (EDT-4).
    pub(super) fn zoom_lane(&self, t: &Theme, s: &Session, cx: &mut Context<Self>) -> AnyElement {
        let g = s.geom;
        let selected = s.model.zoom_selected;
        let chips = s.model.zooms().iter().enumerate().map(|(i, z)| {
            let (x0, x1) = (g.x_of(z.at), g.x_of(z.end()));
            let on = selected == Some(z.at);
            let at = z.at;
            let label = format!("{}\u{00D7}", cm_i18n::fmt::decimal(z.scale, 1));
            let follow = z.follow_cursor;
            div()
                .id(SharedString::from(format!("re-zoom-chip-{i}")))
                .role(Role::Button)
                .aria_label(SharedString::from(if on {
                    tr!(
                        "Zoom {scale} times, selected",
                        scale = cm_i18n::fmt::decimal(z.scale, 1)
                    )
                } else {
                    tr!(
                        "Zoom {scale} times",
                        scale = cm_i18n::fmt::decimal(z.scale, 1)
                    )
                }))
                .absolute()
                .left(px(x0))
                .top(px(2.))
                .w(px((x1 - x0).max(8.)))
                .h(px(LANE_H - 4.))
                .rounded(px(6.))
                .border_1()
                .border_color(rgba_of(if on { t.text } else { t.agent }))
                .when(on, |d| d.border_2())
                .bg(rgba_of(t.agent.with_alpha(if on { 56 } else { 30 })))
                .px(px(6.))
                .flex()
                .items_center()
                .overflow_hidden()
                .mono()
                .text_size(px(11.))
                .text_color(rgba_of(t.text))
                .cursor_pointer()
                .gap(px(4.))
                .child(SharedString::from(label))
                .when(follow, |d| d.child(icon("locate", 11., t.text)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(s) = this.session() {
                        s.model.select_zoom(Some(at));
                    }
                    cx.notify();
                }))
        });
        let playhead = div()
            .absolute()
            .left(px(g.x_of(s.model.playhead) - 0.5))
            .top(px(0.))
            .w(px(1.))
            .h(px(LANE_H))
            .bg(rgba_of(t.text.with_alpha(110)));
        let empty = s.model.zooms().is_empty().then(|| {
            let line = tr!("Press {key} to add a zoom at the playhead", key = KEY_MARK);
            let (before, after) = super::tabs::split_at_key(&line);
            div()
                .absolute()
                .left(px(8.))
                .top(px(0.))
                .h(px(LANE_H))
                .flex()
                .items_center()
                .text_size(px(type_scale::CAPTION))
                .text_color(rgba_of(t.mist))
                .gap(px(6.))
                .child(before)
                .child(keycap(t, "Z"))
                .child(after)
        });
        // Same gap as the track body above it, so the time axes line up.
        div()
            .id("re-zoom-lane")
            .role(Role::Group)
            .aria_label(tr!("Zoom lane"))
            .flex()
            .border_t_1()
            .border_color(rgba_of(t.row_divider()))
            .pt(px(8.))
            .gap(px(super::timeline::BODY_GAP))
            .child(
                div()
                    .w(px(LABEL_W))
                    .h(px(LANE_H))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .text_size(px(type_scale::CAPTION))
                    .text_color(rgba_of(t.text_2))
                    .child(icon("zoom-in", 14., t.mist))
                    .child(tr_ctx!("timeline lane", "Zoom")),
            )
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_w(px(0.))
                    .h(px(LANE_H))
                    .overflow_hidden()
                    .children(chips)
                    .children(empty)
                    .child(playhead),
            )
            .into_any_element()
    }

    // ---- the zoom panel ------------------------------------------------------------------

    pub(super) fn zoom_panel(&self, t: &Theme, s: &Session, cx: &mut Context<Self>) -> AnyElement {
        let m = &s.model;
        let sel = m.selected_zoom();
        let n = m.zooms().len();
        let add = button_with_icon(
            t,
            "re-zoom-add",
            tr!("Add zoom"),
            ButtonKind::Primary,
            Some("zoom-in"),
        )
        .aria_label(tr!("Add a zoom at the playhead"))
        .w_full()
        .child(keycap(t, "Z"))
        .on_click(cx.listener(|this, _, _, cx| this.run(Cmd::AddZoom, cx)));
        let mut body: Vec<AnyElement> = vec![ease_diagram(t), add.into_any_element()];
        let mut foot: Vec<AnyElement> = Vec::new();
        match sel {
            Some(z) => {
                body.push(Self::stepper(
                    t,
                    "re-zoom-scale",
                    "maximize",
                    tr_ctx!("zoom", "Scale"),
                    (tr!("Decrease zoom scale"), tr!("Increase zoom scale")),
                    format!("{}\u{00D7}", cm_i18n::fmt::decimal(z.scale, 1)),
                    |this, cx| this.do_zoom(ZoomAdjust::Scale(-SCALE_STEP), cx),
                    |this, cx| this.do_zoom(ZoomAdjust::Scale(SCALE_STEP), cx),
                    cx,
                ));
                body.push(Self::stepper(
                    t,
                    "re-zoom-length",
                    "timer",
                    tr_ctx!("zoom", "Length"),
                    (tr!("Decrease zoom length"), tr!("Increase zoom length")),
                    format!(
                        "{} s",
                        cm_i18n::fmt::decimal(
                            z.length.ticks() as f64 / cm_core::TICKS_PER_SECOND as f64,
                            1
                        )
                    ),
                    |this, cx| this.do_zoom(ZoomAdjust::Length(-LENGTH_STEP_SECS), cx),
                    |this, cx| this.do_zoom(ZoomAdjust::Length(LENGTH_STEP_SECS), cx),
                    cx,
                ));
                body.push(Self::stepper(
                    t,
                    "re-zoom-x",
                    "move-up-right",
                    tr!("Focus left/right"),
                    (
                        tr!("Move the zoom focus left"),
                        tr!("Move the zoom focus right"),
                    ),
                    format!("{:.0}%", z.focus.0 * 100.0),
                    |this, cx| this.do_zoom(ZoomAdjust::Focus(-FOCUS_STEP, 0.0), cx),
                    |this, cx| this.do_zoom(ZoomAdjust::Focus(FOCUS_STEP, 0.0), cx),
                    cx,
                ));
                body.push(Self::stepper(
                    t,
                    "re-zoom-y",
                    "move-up-right",
                    tr!("Focus up/down"),
                    (
                        tr!("Move the zoom focus up"),
                        tr!("Move the zoom focus down"),
                    ),
                    format!("{:.0}%", z.focus.1 * 100.0),
                    |this, cx| this.do_zoom(ZoomAdjust::Focus(0.0, -FOCUS_STEP), cx),
                    |this, cx| this.do_zoom(ZoomAdjust::Focus(0.0, FOCUS_STEP), cx),
                    cx,
                ));
                let follow = z.follow_cursor;
                body.push(
                    field_row(
                        t,
                        "locate",
                        tr!("Follow cursor"),
                        None,
                        switch(t, "re-zoom-follow", tr!("Follow cursor"), follow, true).on_click(
                            cx.listener(move |this, _, _, cx| {
                                this.do_zoom(ZoomAdjust::FollowCursor(!follow), cx)
                            }),
                        ),
                    )
                    .into_any_element(),
                );
                foot.push(
                    small_button_with_icon(
                        t,
                        "re-zoom-remove",
                        &tr!("Remove zoom"),
                        Some("trash-2"),
                    )
                    .aria_label(tr!("Remove the selected zoom"))
                    .on_click(cx.listener(|this, _, _, cx| this.do_remove_zoom(cx)))
                    .into_any_element(),
                );
            }
            None => {
                if m.zooms().is_empty() {
                    body.push(
                        hint(t, "play", tr!("Press Z to add one at the playhead."))
                            .into_any_element(),
                    );
                    body.push(
                        hint(
                            t,
                            "arrow-up-right",
                            tr!("Or select a range first to zoom exactly over it."),
                        )
                        .into_any_element(),
                    );
                } else {
                    body.push(
                        hint(
                            t,
                            "zoom-in",
                            tr!("Select a zoom on the zoom lane to change it."),
                        )
                        .into_any_element(),
                    );
                }
            }
        }
        panel(
            t,
            Inspector::Zoom,
            SharedString::from(trn!(
                "{n} zoom in this recording",
                "{n} zooms in this recording",
                n
            )),
            body,
            foot,
        )
    }

    // ---- the cursor panel ----------------------------------------------------------------

    pub(super) fn cursor_panel(
        &self,
        t: &Theme,
        s: &Session,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let st = s.model.cursor_style();
        let live = s.cursor_separate;
        // Without "Record cursor separately" the cursor is part of the picture: the options stay
        // visible but dimmed and inert, with the reason (EDT-4). GPUI 0.3.9 has no disabled flag
        // for accessibility nodes, so the tab and the note carry the reason.
        let gate = move |el: AnyElement| -> AnyElement {
            if live {
                el
            } else {
                div().opacity(0.45).child(el).into_any_element()
            }
        };
        let size = Self::stepper(
            t,
            "re-cursor-size",
            "maximize",
            tr_ctx!("cursor", "Size"),
            (tr!("Decrease cursor size"), tr!("Increase cursor size")),
            format!("{}\u{00D7}", cm_i18n::fmt::decimal(st.size, 2)),
            move |this, cx| {
                if live {
                    this.do_cursor(|m| m.nudge_cursor_size(-1), cx)
                }
            },
            move |this, cx| {
                if live {
                    this.do_cursor(|m| m.nudge_cursor_size(1), cx)
                }
            },
            cx,
        );
        let mut smoothing = segmented(t, "re-cursor-smoothing", &tr!("Cursor smoothing"));
        for (label, v) in [
            (tr_ctx!("smoothing", "Off"), Smoothing::Off),
            (tr_ctx!("smoothing", "Light"), Smoothing::Light),
            (tr_ctx!("smoothing", "Strong"), Smoothing::Strong),
        ] {
            let seg = segment(
                t,
                SharedString::from(format!("re-smooth-{label}")),
                &label,
                st.smoothing == v,
            )
            .px(px(8.));
            smoothing = smoothing.child(if live {
                seg.on_click(
                    cx.listener(move |this, _, _, cx| this.do_cursor(|m| m.set_smoothing(v), cx)),
                )
                .into_any_element()
            } else {
                seg.into_any_element()
            });
        }
        let mut click = segmented(t, "re-cursor-click", &tr!("Click effect"));
        for (label, v) in [
            (tr_ctx!("click effect", "None"), ClickEffect::None),
            (tr_ctx!("click effect", "Ring"), ClickEffect::Ring),
            (tr_ctx!("click effect", "Ripple"), ClickEffect::Ripple),
        ] {
            let seg = segment(
                t,
                SharedString::from(format!("re-click-{label}")),
                &label,
                st.click_effect == v,
            )
            .px(px(8.));
            click =
                click.child(if live {
                    seg.on_click(cx.listener(move |this, _, _, cx| {
                        this.do_cursor(|m| m.set_click_effect(v), cx)
                    }))
                    .into_any_element()
                } else {
                    seg.into_any_element()
                });
        }
        let idle = st.hide_idle;
        let idle_switch = switch(t, "re-cursor-idle", tr!("Hide when idle"), idle, live);
        let idle_switch =
            if live {
                idle_switch
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.do_cursor(|m| m.set_hide_idle(!idle), cx)
                    }))
                    .into_any_element()
            } else {
                idle_switch.into_any_element()
            };
        let mut body: Vec<AnyElement> = Vec::new();
        if live {
            body.push(
                hint(
                    t,
                    "mouse-pointer-2",
                    tr!(
                        "The cursor is drawn from its recorded path, so it can be bigger and smoother \
                         than the real one."
                    ),
                )
                .into_any_element(),
            );
        } else {
            body.push(
                callout(
                    t,
                    Tone::Brass,
                    "triangle-alert",
                    tr!(
                        "This recording has the cursor in the picture, so it can't be restyled. \
                         Zooms still work. Turn on \u{201C}Record cursor separately\u{201D} before \
                         recording to edit it."
                    ),
                    None,
                    None,
                )
                .into_any_element(),
            );
        }
        body.push(gate(size));
        body.push(gate(
            field_row(t, "move-up-right", tr!("Smoothing"), None, smoothing).into_any_element(),
        ));
        body.push(gate(
            field_row(t, "mouse-pointer-click", tr!("Click effect"), None, click)
                .into_any_element(),
        ));
        body.push(gate(
            field_row(
                t,
                "eye-off",
                tr!("Hide when idle"),
                Some("2 s"),
                idle_switch,
            )
            .into_any_element(),
        ));
        panel(
            t,
            Inspector::Cursor,
            tr!("Whole recording"),
            body,
            Vec::new(),
        )
    }

    // ---- the reframe panel ---------------------------------------------------------------

    pub(super) fn reframe_panel(
        &self,
        t: &Theme,
        s: &Session,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let m = &s.model;
        let mut aspects = div()
            .id("re-aspect")
            .role(Role::RadioGroup)
            .aria_label(tr!("Preview aspect"))
            .flex()
            .gap(px(8.));
        for a in PreviewAspect::ALL {
            let active = m.preview_aspect == a;
            let (w, h) = match a.label() {
                "9:16" => (18., 30.),
                "1:1" => (24., 24.),
                _ => (34., 22.),
            };
            let c = if active { t.text } else { t.text_3 };
            aspects = aspects.child(
                tile_choice(
                    t,
                    SharedString::from(format!("re-aspect-{}", a.label())),
                    a.label(),
                    active,
                )
                .flex_1()
                .h(px(78.))
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(6.))
                .child(
                    div()
                        .w(px(w))
                        .h(px(h))
                        .rounded(px(4.))
                        .border_2()
                        .border_color(rgba_of(c)),
                )
                .child(
                    div()
                        .mono()
                        .text_size(px(type_scale::CAPTION))
                        .child(a.label()),
                )
                .on_click(cx.listener(move |this, _, _, cx| this.do_preview_aspect(a, cx))),
            );
        }
        let mut body: Vec<AnyElement> = vec![aspects.into_any_element()];
        match m.preview_aspect.reframe_aspect() {
            None => {
                body.push(
                    hint(t, "eye", tr!("Preview a 9:16 or 1:1 export to reframe it."))
                        .into_any_element(),
                );
                body.push(
                    hint(
                        t,
                        "shield-check",
                        tr!("The 16:9 export is never changed by a reframe."),
                    )
                    .into_any_element(),
                );
            }
            Some(aspect) => {
                let r = m.reframe_of(aspect);
                let mut modes = segmented(t, "re-reframe-mode", &tr!("Reframe mode"));
                for (label, mode) in [
                    (tr_ctx!("reframe mode", "Fit"), ReframeMode::Fit),
                    (tr_ctx!("reframe mode", "Fixed"), ReframeMode::Fixed),
                    (tr_ctx!("reframe mode", "Follow"), ReframeMode::Follow),
                ] {
                    modes = modes.child(
                        segment(
                            t,
                            SharedString::from(format!("re-mode-{label}")),
                            &label,
                            r.mode == mode,
                        )
                        .flex_1()
                        .px(px(8.))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.do_cursor(|m| m.set_reframe_mode(aspect, mode), cx)
                        })),
                    );
                }
                body.push(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(8.))
                        .child(micro(t, tr_ctx!("reframe", "Mode")))
                        .child(modes)
                        .into_any_element(),
                );
                if r.mode == ReframeMode::Fixed {
                    let f = r.focus_at(m.playhead);
                    body.push(Self::stepper(
                        t,
                        "re-reframe-x",
                        "move-up-right",
                        tr!("Window left/right"),
                        (tr!("Move the window left"), tr!("Move the window right")),
                        format!("{:.0}%", f.0 * 100.0),
                        move |this, cx| {
                            this.do_cursor(|m| m.nudge_reframe(aspect, -FOCUS_STEP, 0.0), cx)
                        },
                        move |this, cx| {
                            this.do_cursor(|m| m.nudge_reframe(aspect, FOCUS_STEP, 0.0), cx)
                        },
                        cx,
                    ));
                    body.push(Self::stepper(
                        t,
                        "re-reframe-y",
                        "move-up-right",
                        tr!("Window up/down"),
                        (tr!("Move the window up"), tr!("Move the window down")),
                        format!("{:.0}%", f.1 * 100.0),
                        move |this, cx| {
                            this.do_cursor(|m| m.nudge_reframe(aspect, 0.0, -FOCUS_STEP), cx)
                        },
                        move |this, cx| {
                            this.do_cursor(|m| m.nudge_reframe(aspect, 0.0, FOCUS_STEP), cx)
                        },
                        cx,
                    ));
                }
                body.push(
                    hint(
                        t,
                        "eye",
                        match r.mode {
                            ReframeMode::Fit => {
                                tr!(
                                    "Fit shows the whole screen on the background. Choose Fixed to \
                                     crop a window, or Follow to keep the window on the cursor."
                                )
                            }
                            ReframeMode::Fixed => {
                                tr!(
                                    "Each segment has its own window: split the recording, then move \
                                     the playhead into a segment to move its window."
                                )
                            }
                            ReframeMode::Follow => {
                                tr!(
                                    "The window follows the cursor and only moves when the cursor \
                                     leaves its middle 80%."
                                )
                            }
                        },
                    )
                    .into_any_element(),
                );
                body.push(
                    hint(
                        t,
                        "shield-check",
                        tr!("The 16:9 export is never changed by a reframe."),
                    )
                    .into_any_element(),
                );
            }
        }
        panel(
            t,
            Inspector::Reframe,
            tr!("Preview another shape for export"),
            body,
            Vec::new(),
        )
    }
}
