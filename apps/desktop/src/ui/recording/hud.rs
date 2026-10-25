//! The recording HUD (DESIGN 5.2, REC-5): a floating bar, radius 16, padding 6, gap 6, 44 px
//! buttons with radius 11. Order: Stop, timer, divider, Pause, Mic (five-bar meter), Webcam, Draw
//! (disabled), divider, Restart, Discard (Restart sits just left of Discard, decision OD-4).
//!
//! The window is the bar under a 36 px transparent strip that carries the agent chip, the health
//! chip and the hover hint (tooltips would be clipped by a window this small). It opens without
//! taking focus from the app being recorded (a non-activating panel; a click on it makes it key,
//! after which Tab reaches the buttons), is always on top and draggable, and is excluded from
//! capture. During the 60 fps self-test (REC-3) it shrinks to Stop and a "Testing 60 fps" chip. Dragging uses the native window move; after the drag settles the bar
//! snaps to an edge and its position is remembered per display (REC-5.AC4).

use std::time::{Duration, Instant};

use cm_i18n::{tr, tr_ctx};
use gpui::{
    div, prelude::*, px, Context, Entity, FontWeight, MouseButton, Render, Role, SharedString,
    Subscription, Task, Window,
};

use crate::recording::drag::DragState;
use crate::recording::geometry::{self, snap_hud};
use crate::recording::hud_layout::{self, HudControl, HudItem};
use crate::recording::model::{mic_bars, Health, Phase};
use crate::shortcuts::ShortcutAction;
use crate::theme::{scale, type_scale, Color, Theme};
use crate::ui::app::AppModel;
use crate::ui::components::{dot, ellipsis_text, icon, keycaps};
use crate::ui::kit::{pill, Tone};
use crate::ui::paint::{rgba_of, ui_features, TextExt, UI_FONT};
use crate::ui::recording::{self as rec, RecState, HUD_BAR};

/// HUD fill: `rgba(17,17,19,0.96)` (the kit; no blur, DESIGN 4 native caveat).
const FILL: Color = crate::theme::derived::HUD_FILL;
/// How long after the last move the HUD settles and snaps.
const SETTLE: Duration = Duration::from_millis(300);

/// The hover hint of a HUD control. Kept as an id and turned into text when the strip is drawn,
/// so a language switch changes it with the rest of the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HudHint {
    CancelTest,
    Stop,
    Resume,
    Pause,
    MicOn,
    MicOff,
    WebcamOff,
    HideBubble,
    ShowBubble,
    Draw,
    Restart,
    Discard,
}

impl HudHint {
    fn text(self) -> String {
        match self {
            HudHint::CancelTest => tr!("Cancel the 60 fps test"),
            HudHint::Stop => tr!("Stop recording"),
            HudHint::Resume => tr_ctx!("recording hud", "Resume"),
            HudHint::Pause => tr_ctx!("recording hud", "Pause"),
            HudHint::MicOn => tr!("Microphone is recording. Muting isn\u{2019}t available yet."),
            HudHint::MicOff => tr!("Microphone is off for this recording"),
            HudHint::WebcamOff => tr!("Webcam off for this recording"),
            HudHint::HideBubble => tr!("Hide webcam bubble"),
            HudHint::ShowBubble => tr!("Show webcam bubble"),
            HudHint::Draw => tr!("Draw \u{2014} Coming soon"),
            HudHint::Restart => tr!("Restart \u{2014} discard this take and start over"),
            HudHint::Discard => tr!("Discard this take"),
        }
    }
}

/// The text of every HUD hover hint (for the pseudo-locale net in `i18n_net_tests`).
#[cfg(test)]
pub(crate) fn all_hints() -> Vec<String> {
    use HudHint::*;
    [
        CancelTest, Stop, Resume, Pause, MicOn, MicOff, WebcamOff, HideBubble, ShowBubble, Draw,
        Restart, Discard,
    ]
    .into_iter()
    .map(HudHint::text)
    .collect()
}

pub struct HudView {
    app: Entity<AppModel>,
    rec: Entity<RecState>,
    hint: Option<(HudHint, Vec<String>)>,
    /// The user started a drag; the next settled move is theirs (and is remembered).
    drag: DragState,
    settle: Option<Task<()>>,
    _subs: Vec<Subscription>,
}

impl HudView {
    pub fn new(
        app: Entity<AppModel>,
        rec: Entity<RecState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let subs = vec![
            cx.observe(&rec, |_, _, cx| cx.notify()),
            cx.observe_window_bounds(window, |this, window, cx| this.moved(window, cx)),
        ];
        HudView {
            app,
            rec,
            hint: None,
            drag: DragState::default(),
            settle: None,
            _subs: subs,
        }
    }

    fn moved(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let now = Instant::now();
        if !self
            .drag
            .is_live(now, crate::platform_window::left_button_down())
        {
            return;
        }
        self.drag.moved(now);
        self.settle = Some(cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(SETTLE).await;
            this.update_in(cx, |this, window, cx| this.settled(window, cx))
                .ok();
        }));
    }

    /// The drag ended: keep the HUD on its display, snap it to an edge, remember it.
    fn settled(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.drag.clear();
        let Some((x, y, w, h)) = crate::platform_window::frame(window) else {
            return;
        };
        let now = geometry::Rect::new(x, y, w, h);
        let Some((_, display)) = rec::display_for(cx, &now) else {
            return;
        };
        let snapped = snap_hud(&now, &display);
        if (snapped.x - now.x).abs() > 0.5 || (snapped.y - now.y).abs() > 0.5 {
            crate::platform_window::move_to(window, snapped.x, snapped.y);
        }
        let app = self.app.clone();
        cx.defer(move |cx| rec::remember_hud(&app, &display, &snapped, cx));
    }

    fn set_hint(&mut self, hint: Option<(HudHint, Vec<String>)>, cx: &mut Context<Self>) {
        if hint != self.hint {
            self.hint = hint;
            cx.notify();
        }
    }
}

/// A 44 px HUD button: radius 11, optional `raised` fill for an active toggle, focus ring.
fn hud_button(
    t: &Theme,
    id: &'static str,
    label: &str,
    active: bool,
    enabled: bool,
) -> gpui::Stateful<gpui::Div> {
    hud_button_with(t, id, label, active, enabled, t.raised)
}

/// [`hud_button`] with its own hover fill (Stop brightens its `rec` instead).
fn hud_button_with(
    t: &Theme,
    id: &'static str,
    label: &str,
    active: bool,
    enabled: bool,
    hover_fill: Color,
) -> gpui::Stateful<gpui::Div> {
    let (hover, focus) = (rgba_of(hover_fill), rgba_of(t.text));
    div()
        .id(id)
        .role(Role::Button)
        .aria_label(SharedString::from(label.to_owned()))
        .focusable()
        .tab_stop(true)
        .relative()
        .size(px(scale::TARGET))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(11.))
        .border_1()
        .border_color(gpui::transparent_black())
        .when(active, |d| d.bg(rgba_of(t.raised)))
        .when(enabled, |d| d.cursor_pointer().hover(move |s| s.bg(hover)))
        .when(!enabled, |d| d.cursor_default())
        .focus_visible(move |s| s.border_color(focus))
        // A click on a button is not the start of a window drag.
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
}

fn divider(t: &Theme) -> gpui::Div {
    div().w(px(1.)).h(px(24.)).flex_none().bg(rgba_of(t.line))
}

/// The five-bar mic meter inside the Mic button (REC-5.AC15).
fn meter(t: &Theme, lit: u8, live: bool) -> gpui::Div {
    div()
        .absolute()
        .bottom(px(5.))
        .left(px(11.))
        .flex()
        .items_end()
        .gap(px(2.))
        .children((1..=5u8).map(|n| {
            let on = live && n <= lit;
            div()
                .w(px(3.))
                .h(px(2. + f32::from(n)))
                .rounded(px(1.))
                .bg(rgba_of(if on { t.text } else { t.line }))
        }))
}

impl Render for HudView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = Theme::overlay();
        let now = Instant::now();
        let (state, platform, shortcuts, reduced_motion) = {
            let r = self.rec.read(cx);
            let m = self.app.read(cx);
            (
                r.hud.clone(),
                m.platform,
                m.shortcuts.clone(),
                r.reduced_motion,
            )
        };
        let paused = state.is_paused();
        let live = matches!(state.phase, Phase::Recording | Phase::Paused);
        let acting = matches!(state.phase, Phase::Recording | Phase::Paused);
        let timer_text = state.timer_text(now);
        let key = |a: ShortcutAction| shortcuts.get(a).keycaps(platform);
        let bubble_shown = state.camera_on() && !state.bubble_hidden;
        let mic_on = state.mic_on();
        let bars = mic_bars(if paused { None } else { state.mic_dbfs });

        // Pulse: the dot blinks once a second while recording, rests in `mist` while paused.
        // With reduced motion the dot does not blink (R4-13).
        let pulse_on = reduced_motion || state.recorded_ms(now) / 500 % 2 == 0;
        let dot_color = if paused {
            t.mist
        } else if pulse_on {
            t.rec
        } else {
            crate::theme::derived::REC_DIM
        };

        let app = self.app.clone();
        // Stop is dead while the take is being saved: a second click would only fail (R4-23).
        let can_stop = state.phase != Phase::Stopping;
        let stop_hint = if state.phase == Phase::SelfTest {
            HudHint::CancelTest
        } else {
            HudHint::Stop
        };
        let stop = hud_button_with(
            &t,
            "hud-stop",
            &stop_hint.text(),
            false,
            can_stop,
            t.rec_hover,
        )
        .bg(rgba_of(t.rec))
        .when(!can_stop, |d| d.opacity(0.5))
        .on_hover(cx.listener(move |this, over: &bool, _, cx| {
            this.set_hint(over.then(|| (stop_hint, Vec::new())), cx);
        }))
        .when(can_stop, |d| {
            let app = app.clone();
            d.on_click(move |_, _, cx| {
                let app = app.clone();
                cx.defer(move |cx| rec::stop(&app, cx));
            })
        })
        // Stop is the only filled control: the kit's stop glyph and its label in ink.
        .w(px(hud_layout::STOP_W as f32))
        .gap(px(8.))
        .text_size(px(type_scale::UI))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgba_of(t.on_rec))
        .child(icon(hud_layout::STOP_ICON, 16., t.on_rec))
        .child(hud_layout::stop_label());

        let timer = div()
            .id("hud-timer")
            .role(Role::Timer)
            .aria_label(SharedString::from(state.status_label(now)))
            .w(px(hud_layout::TIMER_W as f32))
            .flex_none()
            .px(px(12.))
            .flex()
            .items_center()
            .gap(px(8.))
            .text_size(px(15.))
            .text_color(rgba_of(if paused { t.text_3 } else { t.text }))
            .mono()
            .tnum()
            .child(dot(dot_color, 8.))
            .child(SharedString::from(timer_text))
            // The timer is the grip: dragging it moves the HUD.
            .cursor_grab()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, _| {
                    this.drag.press(Instant::now());
                    window.start_window_move();
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.drag.clear()),
            );

        let pause_label = if paused {
            tr!("Resume recording")
        } else {
            tr!("Pause recording")
        };
        let pause = hud_button(
            &t,
            "hud-pause",
            &pause_label,
            false,
            state.phase != Phase::Resuming && live,
        )
        .on_hover(cx.listener({
            let keys = key(ShortcutAction::PauseRecording);
            move |this, over: &bool, _, cx| {
                this.set_hint(
                    over.then(|| {
                        (
                            if this.rec.read(cx).hud.is_paused() {
                                HudHint::Resume
                            } else {
                                HudHint::Pause
                            },
                            keys.clone(),
                        )
                    }),
                    cx,
                );
            }
        }))
        .on_click({
            let app = app.clone();
            move |_, _, cx| {
                let app = app.clone();
                cx.defer(move |cx| rec::toggle_pause(&app, cx));
            }
        })
        .child(icon(hud_layout::pause_icon(paused), 18., t.text));

        // Mic: a level meter and the state. Muting is not an engine feature yet, so the button
        // reports and does not pretend (REC-5.AC15).
        let mic = hud_button(
            &t,
            "hud-mic",
            &if mic_on {
                tr!("Microphone on, level meter")
            } else {
                tr!("Microphone off")
            },
            mic_on,
            false,
        )
        .on_hover(cx.listener(move |this, over: &bool, _, cx| {
            this.set_hint(
                over.then(|| {
                    (
                        if mic_on {
                            HudHint::MicOn
                        } else {
                            HudHint::MicOff
                        },
                        Vec::new(),
                    )
                }),
                cx,
            );
        }))
        .child(div().mt(px(-5.)).child(icon(
            hud_layout::mic_icon(mic_on),
            18.,
            if mic_on { t.text } else { t.mist },
        )))
        .when(mic_on, |d| d.child(meter(&t, bars, true)));

        let cam_on = state.camera_on();
        let webcam_hint = if !cam_on {
            HudHint::WebcamOff
        } else if bubble_shown {
            HudHint::HideBubble
        } else {
            HudHint::ShowBubble
        };
        let webcam = hud_button(&t, "hud-webcam", &webcam_hint.text(), bubble_shown, cam_on)
            .on_hover(cx.listener(move |this, over: &bool, _, cx| {
                this.set_hint(over.then(|| (webcam_hint, Vec::new())), cx);
            }))
            .when(cam_on, |d| {
                let app = app.clone();
                d.on_click(move |_, _, cx| {
                    let app = app.clone();
                    cx.defer(move |cx| rec::toggle_bubble(&app, cx));
                })
            })
            .child(icon(
                hud_layout::camera_icon(cam_on),
                18.,
                if cam_on { t.text } else { t.mist },
            ));

        // Draw is REC-9 (P2): disabled with the hint "Coming soon"; activating it does nothing.
        let draw = hud_button(
            &t,
            "hud-draw",
            &tr!("Draw on screen, coming soon"),
            false,
            false,
        )
        .aria_description(tr!("Coming soon"))
        .on_hover(cx.listener(|this, over: &bool, _, cx| {
            this.set_hint(over.then(|| (HudHint::Draw, Vec::new())), cx);
        }))
        .child(icon(hud_layout::DRAW_ICON, 18., t.mist));

        let restart = hud_button(&t, "hud-restart", &tr!("Restart recording"), false, acting)
            .on_hover(cx.listener(|this, over: &bool, _, cx| {
                this.set_hint(over.then(|| (HudHint::Restart, Vec::new())), cx);
            }))
            .when(acting, |d| {
                let app = app.clone();
                d.on_click(move |_, _, cx| {
                    let app = app.clone();
                    cx.defer(move |cx| rec::ask_discard(&app, true, cx));
                })
            })
            .child(icon(
                hud_layout::RESTART_ICON,
                18.,
                if acting { t.text_2 } else { t.mist },
            ));

        let discard = hud_button(&t, "hud-discard", &tr!("Discard recording"), false, acting)
            .on_hover(cx.listener(|this, over: &bool, _, cx| {
                this.set_hint(over.then(|| (HudHint::Discard, Vec::new())), cx);
            }))
            .when(acting, |d| {
                let app = app.clone();
                d.on_click(move |_, _, cx| {
                    let app = app.clone();
                    cx.defer(move |cx| rec::ask_discard(&app, false, cx));
                })
            })
            // The destructive control has the red icon (kit); disabled it rests in `mist`.
            .child(icon(
                hud_layout::DISCARD_ICON,
                18.,
                if acting { t.rec_soft } else { t.mist },
            ));

        if state.phase == Phase::SelfTest {
            return self.selftest_view(&t, stop, &state, cx).into_any_element();
        }
        // The agent chip sits inline at the end of the bar when an agent started the take.
        let agent = state
            .started_by_agent()
            .map(|(client, _)| client.to_owned());
        let chip = agent.as_deref().map(|client| {
            let w = hud_layout::agent_chip_width(client) as f32;
            div()
                .id("hud-agent-chip")
                .role(Role::Status)
                .aria_label(SharedString::from(tr!(
                    "Started by agent {client}",
                    client = client
                )))
                .w(px(w))
                .flex_none()
                .child(
                    pill(
                        &t,
                        Tone::Agent,
                        Some(hud_layout::AGENT_ICON),
                        hud_layout::agent_chip_text(client),
                    )
                    .w_full()
                    .px(px(10.))
                    .overflow_hidden()
                    .whitespace_nowrap(),
                )
        });
        // The slots in the model's order (`hud_layout::hud_items`).
        let (
            mut stop,
            mut timer,
            mut pause,
            mut mic,
            mut webcam,
            mut draw,
            mut restart,
            mut discard,
        ) = (
            Some(stop.into_any_element()),
            Some(timer.into_any_element()),
            Some(pause.into_any_element()),
            Some(mic.into_any_element()),
            Some(webcam.into_any_element()),
            Some(draw.into_any_element()),
            Some(restart.into_any_element()),
            Some(discard.into_any_element()),
        );
        let mut chip = chip.map(IntoElement::into_any_element);
        let slots = hud_layout::hud_items(agent.is_some())
            .into_iter()
            .filter_map(|item| match item {
                HudItem::Divider => Some(divider(&t).into_any_element()),
                HudItem::Control(c) => match c {
                    HudControl::Stop => stop.take(),
                    HudControl::Timer => timer.take(),
                    HudControl::Pause => pause.take(),
                    HudControl::Mic => mic.take(),
                    HudControl::Camera => webcam.take(),
                    HudControl::Draw => draw.take(),
                    HudControl::Restart => restart.take(),
                    HudControl::Discard => discard.take(),
                    HudControl::AgentChip => chip.take(),
                },
            });
        let bar = div()
            .id("rec-hud")
            .role(Role::Toolbar)
            .aria_label(tr!("Recording controls"))
            .w(px(hud_layout::bar_width(agent.as_deref()) as f32))
            .h(px(HUD_BAR.1 as f32))
            .flex_none()
            .p(px(hud_layout::PAD as f32))
            .flex()
            .items_center()
            .gap(px(hud_layout::GAP as f32))
            .rounded(px(scale::RADIUS_XL))
            .border_1()
            .border_color(rgba_of(t.line))
            .bg(rgba_of(FILL))
            .children(slots);

        // The strip above the bar: hover hint, else the agent and health chips.
        let strip = {
            let base = div()
                .h(px(rec::HUD_STRIP as f32))
                .flex_none()
                .flex()
                .items_center()
                .pb(px(6.))
                .gap(px(8.));
            if let Some((hint, keys)) = self.hint.clone() {
                let text = hint.text();
                base.child(
                    div()
                        .h(px(26.))
                        .max_w_full()
                        .px(px(10.))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .rounded(px(scale::RADIUS_XS))
                        .border_1()
                        .border_color(rgba_of(t.line))
                        .bg(rgba_of(FILL))
                        .text_size(px(type_scale::CAPTION))
                        .text_color(rgba_of(t.text_2))
                        .child(ellipsis_text(text))
                        .when(!keys.is_empty(), |d| d.child(keycaps(&t, &keys))),
                )
            } else {
                let health = state.health == Health::Degraded;
                let warning = state
                    .warnings
                    .last()
                    .map(|w| crate::daemon_text::warning_text(&w.message));
                base.when(health, |d| {
                    d.child(
                        div()
                            .id("hud-health-chip")
                            .role(Role::Status)
                            .aria_label(tr!("Recording is dropping frames"))
                            .h(px(26.))
                            .px(px(10.))
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .rounded(px(scale::RADIUS_PILL))
                            .border_1()
                            .border_color(rgba_of(t.line))
                            .bg(rgba_of(FILL))
                            .text_color(rgba_of(t.text_2))
                            .text_size(px(type_scale::CAPTION))
                            .child(icon("triangle-alert", 12., t.warning))
                            .child(ellipsis_text(tr!("Dropping frames"))),
                    )
                })
                .when_some(warning.filter(|_| !health), |d, w| {
                    d.child(
                        div()
                            .id("hud-warning-chip")
                            .role(Role::Status)
                            .aria_label(SharedString::from(w.clone()))
                            .h(px(26.))
                            .max_w(px(360.))
                            .px(px(10.))
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .rounded(px(scale::RADIUS_PILL))
                            .border_1()
                            .border_color(rgba_of(t.line))
                            .bg(rgba_of(FILL))
                            .text_color(rgba_of(t.text_2))
                            .text_size(px(type_scale::CAPTION))
                            .child(icon("triangle-alert", 12., t.warning))
                            .child(div().truncate().child(SharedString::from(w))),
                    )
                })
            }
        };

        div()
            .size_full()
            .flex()
            .flex_col()
            .justify_end()
            .font_family(UI_FONT)
            .font_features(ui_features())
            .text_color(rgba_of(t.text))
            .text_size(px(type_scale::UI))
            .font_weight(FontWeight::NORMAL)
            .child(strip)
            .child(bar)
            .into_any_element()
    }
}

impl HudView {
    /// The 60 fps self-test (REC-3): the screen is being captured, so the indicator shows around
    /// it and this small bar says so. Stop cancels the test and deletes its output.
    fn selftest_view(
        &self,
        t: &Theme,
        stop: gpui::Stateful<gpui::Div>,
        state: &crate::recording::model::HudState,
        _cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let left = state
            .selftest_remaining_s
            .map(|s| tr!("{secs} s left", secs = s.ceil().max(0.0) as i64));
        let label = match &left {
            Some(l) => tr!("Testing 60 fps \u{00B7} {left}", left = l),
            None => tr!("Testing 60 fps"),
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .justify_end()
            .items_center()
            .font_family(UI_FONT)
            .font_features(ui_features())
            .text_color(rgba_of(t.text))
            .text_size(px(type_scale::UI))
            .child(
                div()
                    .id("rec-hud")
                    .role(Role::Toolbar)
                    .aria_label(tr!("60 fps test controls"))
                    .h(px(HUD_BAR.1 as f32))
                    .flex_none()
                    .p(px(6.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .rounded(px(scale::RADIUS_XL))
                    .border_1()
                    .border_color(rgba_of(t.line))
                    .bg(rgba_of(FILL))
                    .child(stop)
                    .child(
                        div()
                            .id("hud-selftest")
                            .role(Role::Status)
                            .aria_label(SharedString::from(label.clone()))
                            .px(px(12.))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .text_size(px(15.))
                            .mono()
                            .tnum()
                            .child(dot(t.rec, 8.))
                            .child(SharedString::from(label)),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cm_i18n::{with_language, Lang};

    /// I18N P2: the hover hints are ids rendered on demand. English is what the bar always said.
    #[test]
    fn hover_hints_render_in_the_current_language() {
        assert_eq!(HudHint::Stop.text(), "Stop recording");
        assert_eq!(HudHint::Pause.text(), "Pause");
        assert_eq!(HudHint::Draw.text(), "Draw \u{2014} Coming soon");
        assert_eq!(
            HudHint::Restart.text(),
            "Restart \u{2014} discard this take and start over"
        );
        let pseudo = with_language(Lang::EnXa, || HudHint::Discard.text());
        assert_ne!(pseudo, "Discard this take");
        assert!(pseudo.starts_with('['), "{pseudo}");
    }
}
