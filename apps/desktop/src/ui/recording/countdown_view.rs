//! The countdown (DESIGN 5.3): a 160 px disc on the recording area, the digit in Geist Mono
//! 96/600 and a 4 px `rec` ring that drains each second. Space skips, Esc cancels with nothing
//! recorded (REC-1.AC5). The window never activates CaptureMann and takes no keyboard focus when it
//! opens, so the app being recorded keeps the keyboard (R4-11); a click on the disc makes it key,
//! and then Space and Esc work. The global shortcuts cancel it without a click. With the OS asking
//! for reduced motion the ring stays full and nothing repaints between digits (R4-13). The window
//! is excluded from capture.

use cm_i18n::tr;
use std::time::{Duration, Instant};

use gpui::{
    canvas, div, point, prelude::*, px, Context, Entity, FocusHandle, FontWeight, KeyDownEvent,
    PathBuilder, Render, Role, SharedString, Window,
};

use crate::recording::countdown::{self, key_action, KeyAction, DISC, RING, STEP};
use crate::theme::{type_scale, Theme};
use crate::ui::app::AppModel;
use crate::ui::paint::{hsla_of, rgba_of, ui_features, TextExt, UI_FONT};
use crate::ui::recording::{self as rec, RecState};

pub struct CountdownView {
    app: Entity<AppModel>,
    rec: Entity<RecState>,
    focus: FocusHandle,
}

impl CountdownView {
    pub fn new(
        app: Entity<AppModel>,
        rec: Entity<RecState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        cx.observe(&rec, |_, _, cx| cx.notify()).detach();
        // The ring animates: repaint at ~30 fps while the window lives, unless the user asked for
        // reduced motion (then each digit repaints through the model's notification).
        if !rec.read(cx).reduced_motion {
            cx.spawn(async move |this, cx| loop {
                cx.background_executor()
                    .timer(Duration::from_millis(33))
                    .await;
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            })
            .detach();
        }
        CountdownView { app, rec, focus }
    }
}

impl Render for CountdownView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = Theme::overlay();
        let (digit, label, since, resume, animate) = {
            let r = self.rec.read(cx);
            match r.hud.countdown {
                Some(c) => (
                    c.remaining.to_string(),
                    countdown::label(c.remaining, c.resume),
                    c.step_started,
                    c.resume,
                    !r.reduced_motion,
                ),
                None => (
                    String::new(),
                    tr!("Recording starts soon"),
                    Instant::now(),
                    false,
                    !r.reduced_motion,
                ),
            }
        };
        let frac = countdown::ring_fraction(since.elapsed(), STEP, animate);
        let ring = canvas(
            |_, _, _| (),
            move |bounds, (), window, _| {
                let c = bounds.center();
                let r = f32::from(bounds.size.width) / 2.0 - RING as f32 / 2.0;
                let mut pb = PathBuilder::stroke(px(RING as f32));
                let steps = 72usize;
                let used = ((steps as f64) * frac).round() as usize;
                if used < 2 {
                    return;
                }
                for i in 0..=used {
                    let a = -std::f32::consts::FRAC_PI_2
                        + (i as f32 / steps as f32) * std::f32::consts::TAU;
                    let p = point(c.x + px(r * a.cos()), c.y + px(r * a.sin()));
                    if i == 0 {
                        pb.move_to(p);
                    } else {
                        pb.line_to(p);
                    }
                }
                if let Ok(path) = pb.build() {
                    window.paint_path(path, hsla_of(Theme::overlay().rec));
                }
            },
        )
        .absolute()
        .size_full();
        let app = self.app.clone();
        let hint = countdown::hint(resume);
        div()
            .id("countdown")
            .role(Role::Status)
            .aria_label(SharedString::from(label))
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .pt(px(countdown::PAD as f32))
            .font_family(UI_FONT)
            .font_features(ui_features())
            // A click makes the window key (it opens without focus), so Space and Esc reach it.
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, _, window, cx| window.focus(&this.focus, cx)),
            )
            .on_key_down(cx.listener(move |_, ev: &KeyDownEvent, _, cx| {
                let app = app.clone();
                match key_action(&ev.keystroke.key) {
                    KeyAction::Skip => cx.defer(move |cx| rec::skip_countdown(&app, cx)),
                    KeyAction::Cancel => cx.defer(move |cx| rec::cancel_countdown(&app, cx)),
                    KeyAction::None => {}
                }
            }))
            .child(
                div()
                    .relative()
                    .size(px(DISC as f32))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(DISC as f32 / 2.0))
                    .border_1()
                    .border_color(rgba_of(t.line))
                    .bg(rgba_of(t.surface))
                    .child(ring)
                    .child(
                        div()
                            .text_size(px(countdown::DIGIT_SIZE))
                            .line_height(px(countdown::DIGIT_SIZE))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgba_of(t.text))
                            .mono()
                            .tnum()
                            .child(SharedString::from(digit)),
                    ),
            )
            .child(
                div()
                    .h(px(countdown::HINT_ROW as f32))
                    .flex_none()
                    .flex()
                    .items_center()
                    .when(!hint.is_empty(), |row| {
                        row.child(
                            div()
                                .h(px(22.))
                                .px(px(10.))
                                .flex()
                                .items_center()
                                .rounded(px(11.))
                                .border_1()
                                .border_color(rgba_of(t.line))
                                .bg(rgba_of(t.surface))
                                .text_size(px(type_scale::CAPTION))
                                .text_color(rgba_of(t.text_3))
                                .child(hint),
                        )
                    }),
            )
    }
}
