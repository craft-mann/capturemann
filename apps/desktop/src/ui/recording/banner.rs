//! The recording banner (REC-5 on Linux): the HUD's job inside the Studio window.
//!
//! Where a window over the recorded screen would end up in the recording (Linux, see
//! `recording::surface`), the timer and the controls are a strip at the top of the Studio window:
//! a `rec` dot, the state, the recorded time, Pause or Resume, mute, Discard and Stop; during a
//! countdown the digit with Skip and Cancel. A second line says that the tray icon and the
//! desktop's screen-share indicator also show the recording and can stop it. The strip is not
//! drawn when nothing runs. It observes the recording state only, so the 10 Hz timer repaints
//! this view and not the whole Studio.

use std::time::Instant;

use cm_i18n::tr;
use gpui::{
    div, prelude::*, px, Context, Entity, FontWeight, Render, Role, SharedString, Subscription,
    Window,
};

use crate::recording::surface::{self, Banner};
use crate::theme::{type_scale, Theme};
use crate::ui::app::AppModel;
use crate::ui::components::{button, button_with_icon, dot, icon, label_fill, ButtonKind};
use crate::ui::paint::{rgba_of, ui_features, TextExt, UI_FONT};
use crate::ui::recording::{self as rec, RecState};
use crate::ui::studio::ThemeGlobal;

pub struct RecordingBanner {
    app: Entity<AppModel>,
    rec: Entity<RecState>,
    _sub: Subscription,
}

impl RecordingBanner {
    pub fn new(app: Entity<AppModel>, cx: &mut Context<Self>) -> Self {
        let rec = app.read(cx).rec.clone();
        let sub = cx.observe(&rec, |_, _, cx| cx.notify());
        RecordingBanner {
            app,
            rec,
            _sub: sub,
        }
    }
}

impl Render for RecordingBanner {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The floating HUD owns the controls on the systems that can keep it out of the picture.
        if surface::surface() != surface::Surface::InWindow {
            return div().into_any_element();
        }
        let now = Instant::now();
        let Some(b) = surface::banner(&self.rec.read(cx).hud, now) else {
            return div().into_any_element();
        };
        let t = cx.global::<ThemeGlobal>().0;
        view(&t, &b, &self.app).into_any_element()
    }
}

fn view(t: &Theme, b: &Banner, app: &Entity<AppModel>) -> impl IntoElement {
    let dot_color = if b.live { t.rec } else { t.mist };
    let timer = b.timer.clone().map(|text| {
        div()
            .id("banner-timer")
            .role(Role::Timer)
            .aria_label(SharedString::from(text.clone()))
            .text_size(px(15.))
            .mono()
            .tnum()
            .text_color(rgba_of(t.text))
            .child(SharedString::from(text))
    });
    let mut actions: Vec<gpui::AnyElement> = Vec::new();
    if b.countdown {
        let a = app.clone();
        actions.push(
            button(t, "banner-skip", tr!("Start now"), ButtonKind::Secondary)
                .on_click(move |_, _, cx| {
                    let a = a.clone();
                    cx.defer(move |cx| rec::skip_countdown(&a, cx));
                })
                .into_any_element(),
        );
        let a = app.clone();
        actions.push(
            button(t, "banner-cancel", tr!("Cancel"), ButtonKind::Secondary)
                .on_click(move |_, _, cx| {
                    let a = a.clone();
                    cx.defer(move |cx| rec::cancel_countdown(&a, cx));
                })
                .into_any_element(),
        );
    }
    if let Some(resume) = b.pause {
        let a = app.clone();
        let (label, glyph) = if resume {
            (tr!("Resume recording"), "play")
        } else {
            (tr!("Pause recording"), "rec-pause")
        };
        actions.push(
            button_with_icon(t, "banner-pause", label, ButtonKind::Secondary, Some(glyph))
                .on_click(move |_, _, cx| {
                    let a = a.clone();
                    cx.defer(move |cx| rec::toggle_pause(&a, cx));
                })
                .into_any_element(),
        );
    }
    if let Some(muted) = b.mic {
        let a = app.clone();
        let (label, glyph) = if muted {
            (tr!("Unmute microphone"), "rec-mic-off")
        } else {
            (tr!("Mute microphone"), "mic")
        };
        actions.push(
            button_with_icon(t, "banner-mic", label, ButtonKind::Secondary, Some(glyph))
                .on_click(move |_, _, cx| {
                    let a = a.clone();
                    cx.defer(move |cx| rec::toggle_mic_mute(&a, cx));
                })
                .into_any_element(),
        );
    }
    if b.discard {
        let a = app.clone();
        actions.push(
            button_with_icon(
                t,
                "banner-discard",
                tr!("Discard"),
                ButtonKind::Ghost,
                Some("trash-2"),
            )
            .on_click(move |_, _, cx| {
                let a = a.clone();
                cx.defer(move |cx| rec::ask_discard(&a, false, cx));
            })
            .into_any_element(),
        );
    }
    if b.stop {
        let a = app.clone();
        actions.push(
            button_with_icon(
                t,
                "banner-stop",
                tr!("Stop recording"),
                ButtonKind::Record,
                Some(crate::recording::hud_layout::STOP_ICON),
            )
            .on_click(move |_, _, cx| {
                let a = a.clone();
                cx.defer(move |cx| rec::stop(&a, cx));
            })
            .into_any_element(),
        );
    }
    div()
        .id("recording-banner")
        .role(Role::Status)
        .aria_label(SharedString::from(b.label.clone()))
        .flex_none()
        .w_full()
        .px(px(16.))
        .py(px(10.))
        .flex()
        .flex_col()
        .gap(px(4.))
        .bg(rgba_of(t.surface_2))
        .border_b_1()
        .border_color(rgba_of(t.rec.with_alpha(60)))
        .font_family(UI_FONT)
        .font_features(ui_features())
        .text_color(rgba_of(t.text))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(12.))
                .child(dot(dot_color, 10.))
                .child(
                    div()
                        .text_size(px(type_scale::UI))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(SharedString::from(b.label.clone())),
                )
                .children(timer)
                .child(div().flex_1().min_w(px(0.)))
                .child(div().flex().items_center().gap(px(8.)).children(actions)),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .text_size(px(type_scale::CAPTION))
                .text_color(rgba_of(t.mist))
                .child(icon("monitor", 13., t.mist))
                .child(label_fill(tr!(
                    "Your desktop and the tray icon also show that the screen is being recorded. Stop from either one if this window is hidden."
                ))),
        )
}
