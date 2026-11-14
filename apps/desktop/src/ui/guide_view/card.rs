//! The Input Monitoring card of Click Guide (GDE-1.AC5, design S3): a small panel with the "Not
//! granted" pill and the line "It never records what you type", and the buttons Grant access and
//! Cancel. Nothing records until the permission is granted. While the card is open it reads the
//! permission twice a second; when it turns granted the card closes and the guide starts.

use std::time::Duration;

use cm_i18n::tr;
use gpui::{
    div, prelude::*, px, Context, Entity, FontWeight, Global, Render, Role, SharedString, Window,
    WindowHandle,
};

use crate::permissions::{Id as PermId, State as PermState};
use crate::theme::{scale, type_scale, Theme};
use crate::ui::app::{self, AppModel};
use crate::ui::components::{button, button_with_icon, icon, ButtonKind};
use crate::ui::kit::{pill, Tone};
use crate::ui::paint::{rgba_of, ui_features, UI_FONT};
use crate::ui::recording::{harden, panel_options, pointer_display, visible_part};

const CARD_W: f64 = 440.0;
const CARD_H: f64 = 210.0;

/// The open card, if any (one at a time).
#[derive(Default)]
struct Holder(Option<WindowHandle<InputMonitoringCard>>);
impl Global for Holder {}

pub struct InputMonitoringCard {
    app: Entity<AppModel>,
    /// What the pill says: Not granted, or Granted once the system turned it on.
    state: PermState,
    /// "Grant access" was pressed: the system dialog or the Settings pane was asked for.
    asked: bool,
}

/// Shows the card over the display under the pointer. `then` runs once the permission turns
/// granted (the guide starts); Cancel and closing run nothing.
pub fn open(
    app: &Entity<AppModel>,
    then: impl FnOnce(&Entity<AppModel>, &mut gpui::App) + 'static,
    cx: &mut gpui::App,
) {
    if cx.try_global::<Holder>().is_some_and(|h| h.0.is_some()) {
        return;
    }
    let Some(display) = pointer_display(cx) else {
        return;
    };
    let v = visible_part(&display.1);
    let rect = crate::recording::geometry::Rect::new(
        v.x + (v.w - CARD_W) / 2.0,
        v.y + (v.h - CARD_H) / 3.0,
        CARD_W,
        CARD_H,
    );
    let app2 = app.clone();
    let h = cx
        .open_window(
            panel_options(display, rect, true, false),
            move |window, cx| {
                // Not recorded: the card is about a recording that has not started, but a shared
                // screen should never show a permission dialog either.
                harden(window, false);
                cx.new(|cx| InputMonitoringCard::new(app2, cx))
            },
        )
        .ok();
    let Some(h) = h else { return };
    crate::surfaces::opened(crate::surfaces::Surface::Dialog, h);
    cx.set_global(Holder(Some(h)));
    let app3 = app.clone();
    let mut then = Some(then);
    cx.spawn(async move |cx| loop {
        cx.background_executor()
            .timer(Duration::from_millis(500))
            .await;
        let open = cx.update(|cx| {
            cx.try_global::<Holder>()
                .and_then(|h| h.0)
                .is_some_and(|w| w == h)
        });
        if !open {
            break;
        }
        let granted = cx.update(|cx| {
            app::refresh_permissions(&app3, cx);
            app3.read(cx).perms.state(PermId::InputMonitoring) == PermState::Granted
        });
        if granted {
            cx.update(|cx| {
                close(cx);
                if let Some(f) = then.take() {
                    f(&app3, cx);
                }
            });
            break;
        }
    })
    .detach();
}

/// Closes the card.
pub fn close(cx: &mut gpui::App) {
    if let Some(h) = cx.try_global::<Holder>().and_then(|h| h.0) {
        crate::surfaces::close(cx, h);
    }
    cx.set_global(Holder(None));
}

impl InputMonitoringCard {
    fn new(app: Entity<AppModel>, _cx: &mut Context<Self>) -> Self {
        InputMonitoringCard {
            app,
            state: PermState::NotGranted,
            asked: false,
        }
    }
}

impl Render for InputMonitoringCard {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = Theme::overlay();
        self.state = self.app.read(cx).perms.state(PermId::InputMonitoring);
        let granted = self.state == PermState::Granted;
        let pill_tone = if granted { Tone::Success } else { Tone::Brass };
        let pill_text = if granted {
            tr!("Granted")
        } else {
            tr!("Not granted")
        };
        let asked = self.asked;
        div()
            .id("guide-input-card")
            .role(Role::Dialog)
            .aria_label(tr!("Click Guide needs Input Monitoring"))
            .size_full()
            .flex()
            .flex_col()
            .gap(px(12.))
            .p(px(20.))
            .rounded(px(scale::RADIUS_LG))
            .border_1()
            .border_color(rgba_of(t.line))
            .bg(rgba_of(t.surface))
            .font_family(UI_FONT)
            .font_features(ui_features())
            .text_color(rgba_of(t.text))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(icon("mouse-pointer-click", 22., t.text_2))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .text_size(px(type_scale::TITLE))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(tr!("Click Guide needs Input Monitoring")),
                    )
                    .child(pill(&t, pill_tone, None, pill_text)),
            )
            .child(
                div()
                    .text_size(px(type_scale::UI))
                    .text_color(rgba_of(t.text_2))
                    .child(SharedString::from(format!(
                        "{} {}",
                        tr!("It never records what you type."),
                        tr!("Click Guide only needs to know where you click. Nothing records until you grant access.")
                    ))),
            )
            .when(asked && !granted, |d| {
                d.child(
                    div()
                        .text_size(px(type_scale::CAPTION))
                        .text_color(rgba_of(t.text_3))
                        .child(tr!(
                            "Turn CaptureMann on in System Settings, Privacy and Security, Input Monitoring. This card closes by itself and the guide starts."
                        )),
                )
            })
            .child(div().flex_1())
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(
                        button(&t, "guide-input-cancel", tr!("Cancel"), ButtonKind::Secondary)
                            .on_click(|_, window, cx| {
                                close(cx);
                                let _ = window;
                            }),
                    )
                    .child(
                        button_with_icon(
                            &t,
                            "guide-input-grant",
                            tr!("Grant access\u{2026}"),
                            ButtonKind::Primary,
                            None,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.asked = true;
                            app::permission_action(
                                &this.app,
                                PermId::InputMonitoring,
                                crate::permissions::Action::Grant,
                                cx,
                            );
                            cx.notify();
                        })),
                    ),
            )
    }
}
