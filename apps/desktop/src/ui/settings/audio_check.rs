//! Settings -> Webcam & audio, board R13b: the Audio check card. The rules and the levels are in
//! `audio_check_model` (pure, tested with fakes); this file runs the microphone monitor and the
//! system audio check and draws the card.
//!
//! The microphone is off until the user presses "Test microphone": opening the page opens no
//! device and asks for nothing. The meter reads `cm_audio::MicMonitor` (a level only, no files).
//! The system audio check runs only for "Play test sound".

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use cm_audio::{DeviceSelector, LevelHandle, MicMonitor};
use cm_i18n::{tr, tr_ctx};
use gpui::{
    div, prelude::*, px, relative, AnyElement, Context, Div, FontWeight, Role, SharedString, Task,
};

use super::audio_check_model::{
    band, format_db, mic_card, mic_tip, segments, system_runner, MicCard, MicMeter, MicReading,
    MicTip, SystemCheck, SystemCheckRunner, SystemStatus, Verdict, METER_FLOOR_DB, SEGMENTS,
};
use super::check_widgets::{
    action_button, check_card, chip, control_button, footnote, inner_panel, segment_row,
    state_block, Block, Tone,
};
use super::webcam_model::{CheckSession, DeviceKind, Effect};
use super::SettingsView;
use crate::permissions::Id as PermId;
use crate::theme::Theme;
use crate::ui::components::{icon, label_fill};
use crate::ui::paint::{rgba_of, TextExt};

/// What the Audio check keeps between frames.
pub struct AudioCheckState {
    /// The microphone check: off until "Test microphone". Never saved.
    pub(super) mic: CheckSession,
    monitor: Option<MicMonitor>,
    level: Option<LevelHandle>,
    starting: bool,
    /// Bumped by every start and stop, so a start that finishes late can tell it is stale.
    generation: u64,
    running_device: Option<Option<String>>,
    last_desired: Option<Option<String>>,
    failed: bool,
    meter: MicMeter,
    system: SystemCheck,
    system_generation: u64,
    system_level: Arc<AtomicU32>,
    runner: Arc<dyn SystemCheckRunner>,
    tick: Option<Task<()>>,
    tick_running: bool,
}

impl AudioCheckState {
    pub fn new() -> AudioCheckState {
        AudioCheckState::with_runner(system_runner())
    }

    pub fn with_runner(runner: Arc<dyn SystemCheckRunner>) -> AudioCheckState {
        AudioCheckState {
            mic: CheckSession::default(),
            monitor: None,
            level: None,
            starting: false,
            generation: 0,
            running_device: None,
            last_desired: None,
            failed: false,
            meter: MicMeter::default(),
            system: SystemCheck::default(),
            system_generation: 0,
            system_level: Arc::new(AtomicU32::new(METER_FLOOR_DB.to_bits())),
            runner,
            tick: None,
            tick_running: false,
        }
    }
}

impl Default for AudioCheckState {
    fn default() -> Self {
        AudioCheckState::new()
    }
}

/// Opens the Sound pane of System Settings (to lower the input volume).
fn open_sound_settings() {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.Sound-Settings.extension")
            .spawn();
    }
}

#[derive(Clone, Copy)]
enum Press {
    TestMic,
    AllowMic,
    ChooseMic,
    SoundSettings,
    Play,
    AllowSystem,
}

impl SettingsView {
    fn mic_now_ms(&self) -> u64 {
        self.webcam.clock.elapsed().as_millis() as u64
    }

    // ---- lifecycle -------------------------------------------------------------------------

    /// Starts or stops the microphone monitor to match the page, the window, the permission, the
    /// recording and the user's wish. Called from `webcam_sync`; safe to call any time.
    pub(super) fn audio_sync(&mut self, cx: &mut Context<Self>) {
        let f = self.check_facts(PermId::Microphone, cx);
        if !f.on_page {
            self.audio.system.reset();
            self.audio.system_generation += 1;
            self.audio.failed = false;
        }
        let desired = self.desired_device(DeviceKind::Mic, cx);
        if self.audio.last_desired.as_ref() != Some(&desired) {
            self.audio.last_desired = Some(desired.clone());
            self.audio.failed = false;
        }
        let mut running = self.audio.monitor.is_some() || self.audio.starting;
        if running && self.audio.running_device.as_ref() != Some(&desired) {
            // Another microphone was picked: the meter restarts on it.
            self.audio_stop_mic();
            running = false;
        }
        match self.audio.mic.sync(f, running) {
            Some(Effect::Start) => self.audio_start_mic(desired, cx),
            Some(Effect::Stop) => self.audio_stop_mic(),
            _ => {}
        }
    }

    fn audio_stop_mic(&mut self) {
        self.audio.generation += 1;
        self.audio.starting = false;
        // Dropping the monitor closes the device (the orange dot goes out).
        self.audio.monitor = None;
        self.audio.level = None;
        self.audio.running_device = None;
        self.audio.meter.reset();
    }

    fn audio_start_mic(&mut self, device: Option<String>, cx: &mut Context<Self>) {
        self.audio.generation += 1;
        let generation = self.audio.generation;
        self.audio.starting = true;
        self.audio.failed = false;
        self.audio.meter.reset();
        self.audio.running_device = Some(device.clone());
        let selector = match device {
            Some(id) => DeviceSelector::Id(id),
            None => DeviceSelector::Default,
        };
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { MicMonitor::start(&selector) })
                .await;
            this.update(cx, |this, cx| {
                if this.audio.generation != generation {
                    // Stopped while the microphone was opening: let it go at once.
                    drop(result);
                    return;
                }
                this.audio.starting = false;
                match result {
                    Ok(monitor) => {
                        this.audio.level = Some(monitor.level_handle());
                        this.audio.monitor = Some(monitor);
                        this.audio_ensure_tick(cx);
                    }
                    Err(e) => {
                        this.audio.running_device = None;
                        let denied = e.code() == cm_core::ErrorCode::PermissionMissing;
                        this.audio.mic.start_failed(denied);
                        this.audio.failed = !denied;
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// The meter and the system check are polled about 30 times a second while either runs.
    fn audio_ensure_tick(&mut self, cx: &mut Context<Self>) {
        if self.audio.tick_running {
            return;
        }
        self.audio.tick_running = true;
        self.audio.tick = Some(cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(33))
                .await;
            let alive = this
                .update(cx, |this, cx| this.audio_tick(cx))
                .unwrap_or(false);
            if !alive {
                break;
            }
        }));
    }

    fn audio_tick(&mut self, cx: &mut Context<Self>) -> bool {
        let now = self.mic_now_ms();
        let mut alive = false;
        if let Some(level) = &self.audio.level {
            let l = level.read();
            self.audio.meter.push(now, l.peak_dbfs, l.clip_events);
            alive = true;
        }
        if self.audio.system.is_running() {
            let db = f32::from_bits(self.audio.system_level.load(Ordering::Relaxed));
            self.audio.system.progress(db);
            alive = true;
        }
        if alive {
            cx.notify();
        } else {
            self.audio.tick_running = false;
        }
        alive
    }

    // ---- the buttons -----------------------------------------------------------------------

    fn audio_press(&mut self, press: Press, cx: &mut Context<Self>) {
        match press {
            Press::TestMic => {
                let f = self.check_facts(PermId::Microphone, cx);
                self.audio.failed = false;
                self.audio.mic.press_start(f);
                self.webcam_sync(cx);
            }
            Press::AllowMic => {
                if self.audio.mic.press_allow_access() == Effect::OpenPermissions {
                    self.open_permissions(cx);
                }
            }
            Press::ChooseMic => {
                self.webcam.open = Some(DeviceKind::Mic);
                self.webcam_load_devices(cx);
            }
            Press::SoundSettings => open_sound_settings(),
            Press::Play => self.system_play(cx),
            Press::AllowSystem => self.open_permissions(cx),
        }
        cx.notify();
    }

    fn mic_turn_off(&mut self, cx: &mut Context<Self>) {
        self.audio.mic.press_stop();
        self.audio.failed = false;
        self.webcam_sync(cx);
        cx.notify();
    }

    /// Play test sound: the chime plays in another process and system capture listens (about
    /// 2 s). Nothing runs without the Screen Recording permission, or during a recording.
    fn system_play(&mut self, cx: &mut Context<Self>) {
        let m = self.app.read(cx);
        let permission = m.perms.state(PermId::ScreenRecording).is_granted();
        let recording = m.rec.read(cx).hud.phase.is_active();
        if !self.audio.system.press_play(permission, recording) {
            return;
        }
        self.audio.system_generation += 1;
        let generation = self.audio.system_generation;
        self.audio
            .system_level
            .store(METER_FLOOR_DB.to_bits(), Ordering::Relaxed);
        let runner = self.audio.runner.clone();
        let level = self.audio.system_level.clone();
        self.audio_ensure_tick(cx);
        cx.spawn(async move |this, cx| {
            let outcome = cx
                .background_executor()
                .spawn(async move { runner.run(&|p| level.store(p.to_bits(), Ordering::Relaxed)) })
                .await;
            this.update(cx, |this, cx| {
                if this.audio.system_generation == generation {
                    this.audio.system.finish(outcome);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    // ---- the card --------------------------------------------------------------------------

    /// Audio check (board R13b, card 2): the microphone and the system audio, each with its own
    /// verdict.
    pub(super) fn audio_check_card(
        &self,
        t: &Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let header = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(10.))
            .px(px(16.))
            .py(px(14.))
            .child(icon("audio-lines", 16., t.text))
            .child(
                div()
                    .mr_auto()
                    .text_size(px(15.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(SharedString::from(tr_ctx!("check card", "Audio check"))),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(rgba_of(t.mist))
                    .child(SharedString::from(tr!("Microphone and system audio"))),
            );
        check_card(t, "audio-check", &tr_ctx!("check card", "Audio check"))
            .child(header)
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(12.))
                    .px(px(16.))
                    .pb(px(14.))
                    .child(self.mic_section(t, cx))
                    .child(self.system_section(t, cx))
                    .child(footnote(
                        t,
                        tr!("Level only. Nothing is recorded; the mic turns off when you leave."),
                    )),
            )
    }

    fn sub_heading(
        &self,
        t: &Theme,
        glyph: &'static str,
        title: String,
        status: Option<Div>,
    ) -> Div {
        div()
            .flex()
            .items_center()
            .gap(px(8.))
            .child(icon(glyph, 14., t.text_2))
            .child(
                div()
                    .mr_auto()
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(SharedString::from(title)),
            )
            .children(status)
    }

    fn press_button(
        &self,
        t: &Theme,
        id: &'static str,
        glyph: &'static str,
        label: String,
        press: Press,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        action_button(
            t,
            id,
            glyph,
            &label,
            cx.listener(move |this, _, _, cx| this.audio_press(press, cx)),
        )
        .into_any_element()
    }

    fn mic_section(&self, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let f = self.check_facts(PermId::Microphone, cx);
        let now = self.mic_now_ms();
        let card = mic_card(
            &self.audio.mic,
            f,
            self.audio.failed,
            &self.audio.meter,
            now,
        );
        let reading = self.audio.meter.reading(now);
        let (trigger, below) = self.device_dropdown(t, DeviceKind::Mic, cx);
        let running = matches!(card, MicCard::Running | MicCard::Starting | MicCard::Silent);

        let status_chip = (card == MicCard::Running)
            .then(|| reading.verdict.map(|v| verdict_chip(t, v)))
            .flatten();
        let turn_off = running.then(|| {
            control_button(
                t,
                "mic-turn-off",
                "mic-off",
                &tr!("Turn off microphone"),
                cx.listener(|this, _, _, cx| this.mic_turn_off(cx)),
            )
            .into_any_element()
        });

        let body: Option<AnyElement> = match card {
            MicCard::Running | MicCard::Starting => Some(meter_panel(t, &reading)),
            MicCard::Off => Some(
                self.mic_block(
                    t,
                    "mic-off-state",
                    Block {
                        tone: Tone::Info,
                        glyph: "mic",
                        title: tr!("Microphone is off"),
                        text: tr!("Nothing is heard until you ask. The orange microphone dot stays off."),
                    },
                    Some(self.press_button(
                        t,
                        "mic-test",
                        "mic",
                        tr!("Test microphone"),
                        Press::TestMic,
                        cx,
                    )),
                ),
            ),
            MicCard::NoPermission => Some(
                self.mic_block(
                    t,
                    "mic-no-permission",
                    Block {
                        tone: Tone::Warning,
                        glyph: "shield-check",
                        title: tr!("Microphone access needed"),
                        text: tr!("CaptureMann isn\u{2019}t allowed to use the microphone yet."),
                    },
                    Some(self.press_button(
                        t,
                        "mic-allow-access",
                        "external-link",
                        tr!("Allow access"),
                        Press::AllowMic,
                        cx,
                    )),
                ),
            ),
            MicCard::Silent => Some(
                self.mic_block(
                    t,
                    "mic-silent",
                    Block {
                        tone: Tone::Warning,
                        glyph: "mic-off",
                        title: tr!("No sound coming in"),
                        text: tr!("The mic is selected but silent. It may be muted with a hardware switch."),
                    },
                    Some(self.press_button(
                        t,
                        "mic-choose",
                        "arrow-down",
                        tr!("Choose mic"),
                        Press::ChooseMic,
                        cx,
                    )),
                ),
            ),
            MicCard::Failed => Some(
                self.mic_block(
                    t,
                    "mic-failed",
                    Block {
                        tone: Tone::Warning,
                        glyph: "mic-off",
                        title: tr!("The microphone didn\u{2019}t start"),
                        text: tr!("CaptureMann couldn\u{2019}t open the microphone. Pick another one, or try again."),
                    },
                    Some(self.press_button(
                        t,
                        "mic-try-again",
                        "refresh-cw",
                        tr!("Try again"),
                        Press::TestMic,
                        cx,
                    )),
                ),
            ),
            MicCard::Paused => Some(self.mic_block(
                t,
                "mic-paused",
                Block {
                    tone: Tone::Info,
                    glyph: "cm-record",
                    title: tr!("Recording in progress"),
                    text: tr!("Recording in progress, the check is paused."),
                },
                None,
            )),
        };
        let tip = (card == MicCard::Running)
            .then(|| mic_tip(&reading))
            .flatten()
            .map(|tip| match tip {
                MicTip::TooQuiet => self.mic_block(
                    t,
                    "mic-too-quiet",
                    Block {
                        tone: Tone::Warning,
                        glyph: "mic",
                        title: tr!("Too quiet"),
                        text: tr!("Your voice barely reaches the good range. Move closer or pick another mic."),
                    },
                    Some(self.press_button(
                        t,
                        "mic-choose-quiet",
                        "arrow-down",
                        tr!("Choose mic"),
                        Press::ChooseMic,
                        cx,
                    )),
                ),
                MicTip::Clipping => self.mic_block(
                    t,
                    "mic-clipping",
                    Block {
                        tone: Tone::Error,
                        glyph: "volume-2",
                        title: tr!("Too loud, clipping"),
                        text: tr!("Peaks hit the top. Lower the input volume in System Settings."),
                    },
                    Some(self.press_button(
                        t,
                        "mic-sound-settings",
                        "external-link",
                        tr!("Sound settings"),
                        Press::SoundSettings,
                        cx,
                    )),
                ),
            });

        div()
            .flex_1()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(self.sub_heading(t, "mic", tr_ctx!("audio check", "Microphone"), status_chip))
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(8.))
                    .child(trigger)
                    .children(turn_off),
            )
            .children(below)
            .children(body)
            .children(tip)
            .into_any_element()
    }

    /// A state block of the microphone sub-section, as wide as the card.
    fn mic_block(
        &self,
        t: &Theme,
        id: &'static str,
        block: Block,
        action: Option<AnyElement>,
    ) -> AnyElement {
        state_block(t, id, block, action)
            .w_full()
            .into_any_element()
    }

    fn system_section(&self, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let status = self.audio.system.status().clone();
        let status_chip = match status {
            SystemStatus::Heard => Some(chip(
                t,
                Tone::Success,
                22.,
                tr_ctx!("system audio status", "Heard"),
                Some("circle-check"),
                false,
            )),
            SystemStatus::Running => Some(chip(
                t,
                Tone::Info,
                22.,
                tr_ctx!("system audio status", "Listening\u{2026}"),
                None,
                true,
            )),
            _ => None,
        };
        let body: AnyElement = match status {
            SystemStatus::Idle | SystemStatus::Running | SystemStatus::Heard => {
                let running = status == SystemStatus::Running;
                let play = control_button(
                    t,
                    "system-play",
                    "play",
                    &tr!("Play test sound"),
                    cx.listener(|this, _, _, cx| this.audio_press(Press::Play, cx)),
                )
                .h(px(32.))
                .when(running, |d| d.opacity(0.5).cursor_default());
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(
                        inner_panel(t)
                            .id("system-audio-panel")
                            .role(Role::Meter)
                            .aria_label(SharedString::from(tr!("System audio level")))
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap(px(12.))
                            .px(px(12.))
                            .py(px(10.))
                            .child(
                                div().flex_1().flex_basis(px(120.)).min_w(px(0.)).child(
                                    segment_row(t, &self.audio.system.segments(), 18., 3., 2.),
                                ),
                            )
                            .child(play),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .line_height(px(17.))
                            .text_color(rgba_of(t.mist))
                            .child(SharedString::from(tr!(
                                "Plays a short chime and checks that CaptureMann can hear it. Uses the Screen Recording permission."
                            ))),
                    )
                    .into_any_element()
            }
            SystemStatus::NotHeard => state_block(
                t,
                "system-not-heard",
                Block {
                    tone: Tone::Warning,
                    glyph: "volume-2",
                    title: tr!("Chime not heard"),
                    text: tr!("System audio needs Screen Recording access. Check your output is not muted."),
                },
                Some(self.press_button(
                    t,
                    "system-allow-access",
                    "external-link",
                    tr!("Allow access"),
                    Press::AllowSystem,
                    cx,
                )),
            )
            .into_any_element(),
            SystemStatus::NoPermission => state_block(
                t,
                "system-no-permission",
                Block {
                    tone: Tone::Warning,
                    glyph: "shield-check",
                    title: tr!("Screen Recording access needed"),
                    text: tr!("System audio comes with Screen Recording, and CaptureMann isn\u{2019}t allowed to use it yet."),
                },
                Some(self.press_button(
                    t,
                    "system-allow-access",
                    "external-link",
                    tr!("Allow access"),
                    Press::AllowSystem,
                    cx,
                )),
            )
            .into_any_element(),
            SystemStatus::Failed(_) => state_block(
                t,
                "system-failed",
                Block {
                    tone: Tone::Warning,
                    glyph: "volume-2",
                    title: tr!("The test sound didn\u{2019}t play"),
                    text: tr!("The check could not run. Try again."),
                },
                Some(self.press_button(
                    t,
                    "system-try-again",
                    "refresh-cw",
                    tr!("Try again"),
                    Press::Play,
                    cx,
                )),
            )
            .into_any_element(),
        };
        div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .pt(px(12.))
            .border_t_1()
            .border_color(rgba_of(t.raised))
            .child(self.sub_heading(
                t,
                "volume-2",
                tr_ctx!("audio check", "System audio"),
                status_chip,
            ))
            .child(body)
            .into_any_element()
    }
}

/// The verdict chip next to "Microphone".
fn verdict_chip(t: &Theme, v: Verdict) -> Div {
    match v {
        Verdict::TooQuiet => chip(
            t,
            Tone::Warning,
            22.,
            tr_ctx!("mic verdict", "Too quiet"),
            Some("triangle-alert"),
            false,
        ),
        Verdict::Good => chip(
            t,
            Tone::Success,
            22.,
            tr_ctx!("mic verdict", "Good level"),
            Some("circle-check"),
            false,
        ),
        Verdict::TooLoud => chip(
            t,
            Tone::Error,
            22.,
            tr_ctx!("mic verdict", "Too loud"),
            Some("octagon-alert"),
            false,
        ),
    }
}

/// The level meter: the prompt, the large dB readout, 32 segments under the "good range" band.
fn meter_panel(t: &Theme, r: &MicReading) -> AnyElement {
    let (left, width) = band();
    let segs = segments(SEGMENTS, r.level_db, Some(r.peak_db));
    let band_color = t.success;
    inner_panel(t)
        .id("mic-meter")
        .role(Role::Meter)
        .aria_label(SharedString::from(tr!(
            "Microphone level, {level}",
            level = format_db(r.peak_db)
        )))
        .flex_1()
        .flex()
        .flex_col()
        .justify_center()
        .gap(px(8.))
        .p(px(14.))
        .child(
            div()
                .flex()
                .items_baseline()
                .justify_between()
                .gap(px(8.))
                .child(
                    label_fill(SharedString::from(tr!(
                        "Speak as you would while recording"
                    )))
                    .text_size(px(12.))
                    .text_color(rgba_of(t.mist)),
                )
                .child(
                    div()
                        .flex_none()
                        .mono()
                        .tnum()
                        .text_size(px(18.))
                        .text_color(rgba_of(t.text))
                        .child(SharedString::from(format_db(r.peak_db))),
                ),
        )
        .child(
            div()
                .relative()
                .w_full()
                .h(px(54.))
                .pt(px(16.))
                .child(
                    div()
                        .absolute()
                        .top(px(0.))
                        .left(relative(left))
                        .w(relative(width))
                        .h(px(10.))
                        .rounded_t(px(3.))
                        .border_t_1()
                        .border_l_1()
                        .border_r_1()
                        .border_color(rgba_of(band_color.with_alpha(45)))
                        .child(
                            div()
                                .absolute()
                                .left(px(6.))
                                .top(px(-9.))
                                .px(px(4.))
                                .bg(rgba_of(t.field))
                                .text_size(px(10.))
                                .text_color(rgba_of(band_color))
                                .whitespace_nowrap()
                                .child(SharedString::from(tr!("good range"))),
                        ),
                )
                .child(segment_row(t, &segs, 38., 3., 3.)),
        )
        .child(
            div()
                .flex()
                .justify_between()
                .mono()
                .text_size(px(10.))
                .text_color(rgba_of(t.faint))
                .child(SharedString::from(tr_ctx!("meter end", "quiet")))
                .child(SharedString::from(tr_ctx!("meter end", "loud"))),
        )
        .into_any_element()
}
