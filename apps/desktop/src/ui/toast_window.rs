//! Toasts (DESIGN section 4, redesign R17): Overlay mode, one bottom-right stack of at most
//! three, 440 px wide, a 3 px tone stripe, a 30 px tinted icon tile whose glyph differs by kind,
//! title 14/600, small actions with icons and a dismiss button, in a transparent always-on-top
//! window sized to the stack so it never blocks clicks elsewhere.

use gpui::{
    canvas, div, prelude::*, px, Bounds, Context, Entity, FontWeight, Pixels, Render, Role,
    SharedString, Window,
};

use cm_i18n::tr;

use crate::theme::{scale, type_scale, Theme};
use crate::toast::TOAST_INSET;
use crate::toast::{Toast, ToastKind, TOAST_WIDTH};
use crate::ui::app::{self, AppModel, Command};
use crate::ui::components::icon;
use crate::ui::components::keycaps;
use crate::ui::kit::{tone_colors, Tone};
use crate::ui::paint::{rgba_of, ui_features, TextExt, UI_FONT};
use crate::ui::settings::widgets::small_button_with_icon;

pub struct ToastHost {
    app: Entity<AppModel>,
}

impl ToastHost {
    /// The window stays while its stack changes: the host repaints from the model (R4-23), so
    /// hover and keyboard focus survive a new toast and progress updates do not flicker.
    pub fn new(app: Entity<AppModel>, cx: &mut Context<Self>) -> Self {
        cx.observe(&app, |_, _, cx| cx.notify()).detach();
        ToastHost { app }
    }
}

/// The tone of a toast kind. Agent blue is only for the agent toast; Warning is brass; Error is
/// the soft rec of the kit's error status (`kit::status_parts`); Info is neutral and Success
/// green. The icon shape also differs by kind (`ToastKind::icon`).
pub fn kind_tone(k: ToastKind) -> Tone {
    match k {
        ToastKind::Info => Tone::Neutral,
        ToastKind::Success => Tone::Success,
        ToastKind::Warning => Tone::Brass,
        ToastKind::Error => Tone::Danger,
        ToastKind::Agent => Tone::Agent,
    }
}

fn toast_view(t: &Theme, toast: &Toast, app: &Entity<AppModel>) -> gpui::Div {
    let id = toast.id;
    let dismiss_app = app.clone();
    let hover_app = app.clone();
    let tone = tone_colors(t, kind_tone(toast.kind));
    let (stripe, tile_bg) = if kind_tone(toast.kind) == Tone::Neutral {
        (t.line, t.icon_tile())
    } else {
        (tone.icon, tone.bg)
    };
    let buttons = toast.buttons.iter().enumerate().map(|(i, b)| {
        let (app, action) = (app.clone(), b.action.clone());
        small_button_with_icon(
            t,
            SharedString::from(format!("toast-{id}-{i}")),
            &b.label,
            Some(b.action.icon()),
        )
        .bg(rgba_of(t.surface_2))
        .on_click(move |_, _, cx| {
            let (app, action) = (app.clone(), action.clone());
            // The toast window is mid-update here; act after the update ends.
            cx.defer(move |cx| {
                app::handle_command(&app, Command::ToastAction(action), cx);
                app::dismiss_toast(&app, id, cx);
            });
        })
    });
    let hints = toast.hints.iter().map(|h| {
        div()
            .flex()
            .items_center()
            .gap(px(6.))
            .child(
                div()
                    .min_w(px(0.))
                    .text_size(px(type_scale::CAPTION))
                    .text_color(rgba_of(t.text_3))
                    .child(SharedString::from(h.text.clone())),
            )
            .child(keycaps(t, &h.keys))
    });
    let (hover_bg, focus) = (rgba_of(t.raised), rgba_of(t.text));
    div().w(px(TOAST_WIDTH)).p(px(TOAST_INSET)).child(
        div()
            .id(SharedString::from(format!("toast-{id}")))
            .role(if toast.kind == ToastKind::Error {
                Role::Alert
            } else {
                Role::Status
            })
            .aria_label(SharedString::from(match &toast.body {
                Some(b) => tr!("{title}. {body}", title = toast.title, body = b),
                None => toast.title.clone(),
            }))
            .on_hover(move |hovered, _, cx| {
                let (app, hovered) = (hover_app.clone(), *hovered);
                cx.defer(move |cx| app::set_toast_hover(&app, id, hovered, cx));
            })
            .relative()
            .overflow_hidden()
            .w_full()
            .rounded(px(scale::RADIUS_LG))
            .border_1()
            .border_color(rgba_of(t.border))
            .bg(rgba_of(t.surface_2))
            // The tone stripe (the kit's inset 3 px shadow), clipped by the card's corners.
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .bottom_0()
                    .w(px(3.))
                    .bg(rgba_of(stripe)),
            )
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap(px(12.))
                    .pl(px(17.))
                    .pr(px(8.))
                    .py(px(14.))
                    .child(
                        div()
                            .size(px(30.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(9.))
                            .bg(rgba_of(tile_bg))
                            .child(icon(toast.kind.icon(), 16., tone.icon)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .child(
                                div()
                                    .min_h(px(30.))
                                    .flex()
                                    .items_center()
                                    .text_size(px(type_scale::UI))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(SharedString::from(toast.title.clone())),
                            )
                            .children(toast.agent.clone().map(|a| {
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .items_center()
                                    .gap(px(8.))
                                    .text_size(px(type_scale::CAPTION))
                                    .mono()
                                    .child(
                                        div()
                                            .text_color(rgba_of(t.agent_text))
                                            .child(SharedString::from(a.tool)),
                                    )
                                    .child(div().text_color(rgba_of(t.mist)).child(
                                        SharedString::from(tr!("from {client}", client = a.client)),
                                    ))
                            }))
                            .children(toast.body.clone().map(|b| {
                                div()
                                    .text_size(px(13.))
                                    .text_color(rgba_of(t.text_3))
                                    .child(SharedString::from(b))
                            }))
                            .when(!toast.hints.is_empty(), |d| {
                                d.child(
                                    div()
                                        .flex()
                                        .flex_wrap()
                                        .items_center()
                                        .gap(px(14.))
                                        .pt(px(4.))
                                        .children(hints),
                                )
                            })
                            .when(!toast.buttons.is_empty(), |d| {
                                d.child(
                                    div()
                                        .flex()
                                        .flex_wrap()
                                        .gap(px(8.))
                                        .pt(px(6.))
                                        .children(buttons),
                                )
                            }),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("toast-{id}-dismiss")))
                            .role(Role::Button)
                            .aria_label(tr!("Dismiss notification"))
                            .focusable()
                            .tab_stop(true)
                            .size(px(28.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(scale::RADIUS_SM))
                            .border_1()
                            .border_color(gpui::transparent_black())
                            .cursor_pointer()
                            .hover(move |s| s.bg(hover_bg))
                            .focus_visible(move |s| s.border_color(focus))
                            .child(icon("x", 16., t.text_3))
                            .on_click(move |_, _, cx| {
                                let app = dismiss_app.clone();
                                cx.defer(move |cx| app::dismiss_toast(&app, id, cx));
                            }),
                    ),
            ),
    )
}

impl Render for ToastHost {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = Theme::overlay();
        let toasts: Vec<Toast> = self.app.read(cx).toasts.toasts().to_vec();
        let app = self.app.clone();
        // The window is transparent: only the cards are drawn, anchored at the bottom. The wrapper
        // is as tall as its cards; a canvas over it reports that height so the window can be fitted
        // to what was really painted instead of to an estimate.
        div()
            .size_full()
            .flex()
            .flex_col()
            .justify_end()
            .items_center()
            .font_family(UI_FONT)
            .font_features(ui_features())
            .text_color(rgba_of(t.text))
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_col()
                    .children(toasts.iter().map(|toast| toast_view(&t, toast, &self.app)))
                    .child(
                        canvas(
                            |_, _, _| (),
                            move |bounds: Bounds<Pixels>, _, window, cx| {
                                let measured = bounds.size.height.as_f32().ceil();
                                let current = window.bounds().size.height.as_f32();
                                if measured > 0. && (measured - current).abs() > 1. {
                                    let app = app.clone();
                                    cx.defer(move |cx| app::fit_toast_window(&app, measured, cx));
                                }
                            },
                        )
                        .absolute()
                        .size_full(),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// R17: agent blue only for the agent kind; error and warning differ in tone too (and in
    /// glyph, `ToastKind::icon`).
    #[test]
    fn tones_per_kind() {
        assert_eq!(kind_tone(ToastKind::Agent), Tone::Agent);
        for k in [
            ToastKind::Info,
            ToastKind::Success,
            ToastKind::Warning,
            ToastKind::Error,
        ] {
            assert_ne!(kind_tone(k), Tone::Agent);
        }
        assert_eq!(kind_tone(ToastKind::Error), Tone::Danger);
        assert_eq!(kind_tone(ToastKind::Warning), Tone::Brass);
    }
}
