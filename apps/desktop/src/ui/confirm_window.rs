//! The agent confirmation dialog (AGT-2, DESIGN 5.6). Overlay mode: dark in both themes, a
//! floating panel with an outer ring and Level 5 depth. Default focus is Cancel and Enter is
//! ignored for the first second.

use cm_daemon::Decision;
use cm_i18n::tr;
use gpui::{
    div, prelude::*, px, ClickEvent, Context, FocusHandle, FontWeight, KeyDownEvent, Render, Role,
    SharedString, Toggled, Window,
};
use std::time::Instant;
use tokio::sync::oneshot;

use crate::confirm::{DialogModel, EnterGuard};
use crate::theme::{scale, type_scale, Theme};
use crate::ui::components::{button, button_with_icon, icon, ButtonKind};
use crate::ui::kit::{pill, Tone};
use crate::ui::paint::{rgba_of, ui_features, TextExt, UI_FONT};
use crate::ui::studio::{FocusNext, FocusPrev};

pub struct ConfirmDialog {
    model: DialogModel,
    reply: Option<oneshot::Sender<Decision>>,
    guard: EnterGuard,
    session_checked: bool,
    cancel_focus: FocusHandle,
}

impl ConfirmDialog {
    pub fn new(
        model: DialogModel,
        reply: oneshot::Sender<Decision>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let cancel_focus = cx.focus_handle();
        // AGT-2 / DESIGN 5.6: default focus is Cancel.
        window.focus(&cancel_focus, cx);
        ConfirmDialog {
            model,
            reply: Some(reply),
            guard: EnterGuard::new(Instant::now()),
            session_checked: false,
            cancel_focus,
        }
    }

    fn answer(&mut self, decision: Decision, window: &mut Window) {
        if let Some(tx) = self.reply.take() {
            let _ = tx.send(decision);
        }
        window.remove_window();
    }

    /// Keyboard activation (Enter or Space on a focused button) is ignored for 1 s;
    /// mouse clicks are always accepted (AGT-2.AC2).
    fn accepts(&self, ev: &ClickEvent) -> bool {
        !matches!(ev, ClickEvent::Keyboard(_)) || self.guard.accepts_enter(Instant::now())
    }
}

impl Render for ConfirmDialog {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = Theme::overlay();
        let m = self.model.clone();
        let checked = self.session_checked;
        let tool_label = crate::daemon_text::row_label("tool");
        let details = m.details.iter().map(|(k, v)| {
            let mono = *k == tool_label;
            div()
                .flex()
                .gap(px(12.))
                .text_size(px(13.))
                .child(
                    div()
                        .w(px(84.))
                        .flex_none()
                        .text_color(rgba_of(t.mist))
                        .child(SharedString::from(k.clone())),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .text_color(rgba_of(t.text))
                        .when(mono, |d| d.mono())
                        .child(SharedString::from(v.clone())),
                )
        });
        let primary_kind = if m.primary_is_danger {
            ButtonKind::Danger
        } else if m.primary_is_record {
            ButtonKind::Record
        } else {
            ButtonKind::Primary
        };
        let decision_for_primary = move |checked: bool| {
            if checked {
                Decision::ApproveForSession
            } else {
                Decision::Approve
            }
        };
        // The title tile: a red tint for Record and the destructive confirms (OD-17), neutral for
        // everything else.
        let red_tile = m.primary_is_danger || m.primary_is_record;
        let tile = div()
            .size(px(44.))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(12.))
            .border_1()
            .border_color(rgba_of(if red_tile {
                t.rec.with_alpha(38)
            } else {
                t.border
            }))
            .bg(rgba_of(if red_tile {
                t.rec.with_alpha(12)
            } else {
                t.icon_tile()
            }))
            .child(icon(
                m.title_icon(),
                20.,
                if red_tile { t.rec_soft } else { t.text_2 },
            ));
        // The agent chip (bot, agent blue) sits at the right of the title row; the app's own
        // questions (Discard, library Delete) have none.
        let chip = m.agent_name().map(|name| {
            div()
                .id("agent-chip")
                .aria_label(SharedString::from(tr!("Requested by {name}", name = name)))
                .flex_none()
                .child(pill(&t, Tone::Agent, Some("bot"), name.to_owned()))
        });
        // The text scrolls when the window had to be capped to the display; the buttons stay.
        let body = div()
            .id("confirm-body")
            .flex_1()
            .min_h(px(0.))
            .overflow_y_scroll()
            .p(px(24.))
            .pb(px(16.))
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(14.))
                    .child(tile)
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .text_size(px(type_scale::TITLE))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(SharedString::from(m.title.clone())),
                    )
                    .children(chip),
            )
            .child(
                div()
                    .text_size(px(type_scale::UI))
                    .text_color(rgba_of(t.text_2))
                    .child(SharedString::from(m.summary.clone())),
            )
            .when(!m.details.is_empty(), |d| {
                d.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(8.))
                        .p(px(12.))
                        .rounded(px(scale::RADIUS_MD))
                        .border_1()
                        .border_color(rgba_of(t.border))
                        .bg(rgba_of(t.field))
                        .children(details),
                )
            })
            .when_some(m.checkbox_label(), |d, label| {
                d.child(
                    div()
                        .id("allow-session")
                        .role(Role::CheckBox)
                        .aria_label(SharedString::from(label.clone()))
                        .aria_toggled(if checked {
                            Toggled::True
                        } else {
                            Toggled::False
                        })
                        .focusable()
                        .tab_stop(true)
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .cursor_pointer()
                        .rounded(px(scale::RADIUS_SM))
                        .border_1()
                        .border_color(gpui::transparent_black())
                        .focus_visible(|s| s.border_color(rgba_of(t.text)))
                        .on_click(cx.listener(|this, ev: &ClickEvent, _, cx| {
                            if this.accepts(ev) {
                                this.session_checked = !this.session_checked;
                                cx.notify();
                            }
                        }))
                        .child(
                            div()
                                .size(px(18.))
                                .flex_none()
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(5.))
                                .border_1()
                                .border_color(rgba_of(if checked { t.primary_bg } else { t.line }))
                                .bg(rgba_of(if checked { t.primary_bg } else { t.raised }))
                                .when(checked, |d| d.child(icon("check", 12., t.primary_fg))),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .text_size(px(13.))
                                .text_color(rgba_of(t.text_2))
                                .child(SharedString::from(label)),
                        ),
                )
            });
        let footer = div()
            .flex()
            .flex_shrink_0()
            .justify_end()
            .gap(px(8.))
            .px(px(24.))
            .py(px(16.))
            .border_t_1()
            .border_color(rgba_of(t.border))
            .child(
                button(&t, "cancel", tr!("Cancel"), ButtonKind::Secondary)
                    .h(px(scale::CONTROL_H))
                    .track_focus(&self.cancel_focus)
                    .on_click(cx.listener(|this, ev: &ClickEvent, window, _| {
                        if this.accepts(ev) {
                            this.answer(Decision::Decline, window);
                        }
                    })),
            )
            .child(
                button_with_icon(
                    &t,
                    "primary",
                    m.primary_label.clone(),
                    primary_kind,
                    m.primary_icon(),
                )
                .h(px(scale::CONTROL_H))
                .on_click(cx.listener(move |this, ev: &ClickEvent, window, _| {
                    if this.accepts(ev) {
                        let d = decision_for_primary(this.session_checked);
                        this.answer(d, window);
                    }
                })),
            );
        let panel = div()
            .id("confirm-dialog")
            .role(Role::AlertDialog)
            .aria_label(SharedString::from(format!("{}. {}", m.title, m.summary)))
            .size_full()
            .flex()
            .flex_col()
            .bg(rgba_of(t.surface))
            .border_1()
            .border_color(rgba_of(t.line))
            .child(body)
            .child(footer);
        div()
            .size_full()
            .bg(rgba_of(t.surface))
            .font_family(UI_FONT)
            .font_features(ui_features())
            .text_color(rgba_of(t.text))
            .text_size(px(type_scale::UI))
            .on_action(cx.listener(|_, _: &FocusNext, window, cx| window.focus_next(cx)))
            .on_action(cx.listener(|_, _: &FocusPrev, window, cx| window.focus_prev(cx)))
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, _| {
                if ev.keystroke.key == "escape" {
                    this.answer(Decision::Decline, window);
                }
            }))
            .child(panel)
    }
}
