//! The start sheet's Audio mode (VOC-1, board V1 `VoiceRecordSheet`): the mode switch, the five
//! preset cards, the microphone row with its device picker and live 40-segment meter, the system
//! audio switch, "How many people?", the "After you stop" chips, the consent line and the footer
//! line. The rules are `recording::audio_sheet` (pure, tested); this file draws them and runs
//! the level meter.
//!
//! The meter reads `cm_audio::MicMonitor` (a level only, no files) while Audio mode is open, the
//! microphone is on and the OS already allowed it; it never asks for permission by itself, and it
//! is closed before a recording starts and when the sheet closes. Starting goes through the
//! `start_recording` action with `mode: "audio"`, like the shortcut and an agent.

use std::time::{Duration, Instant};

use cm_audio::{DeviceSelector, LevelHandle, MicMonitor};
use cm_i18n::{tr, tr_ctx};
use gpui::{
    div, prelude::*, px, Context, FontWeight, KeyDownEvent, Role, SharedString, Task, Window,
};
use serde_json::json;

use super::{ButtonKind, SheetMode, StartSheet};
use crate::permissions::{self, Id as PermId};
use crate::recording::audio_sheet::{
    self as model, AudioSheetModel, People, Preset, METER_SEGMENTS,
};
use crate::recording::sheet::{Note, Perm, TargetsLoad};
use crate::shortcuts::ShortcutAction;
use crate::theme::{scale, type_scale, Theme};
use crate::ui::app;
use crate::ui::components::{
    button, button_with_icon, ellipsis_text, icon, icon_size, keycaps, label_fill, shrink_text,
    switch,
};
use crate::ui::paint::{rgba_of, TextExt};
use crate::ui::settings::audio_check_model::{segments, MicMeter, Seg};
use crate::ui::settings::widgets::caption;

/// What the meter keeps between frames.
pub(super) struct MeterState {
    monitor: Option<MicMonitor>,
    level: Option<LevelHandle>,
    starting: bool,
    /// Bumped by every start and stop, so a start that finishes late can tell it is stale.
    generation: u64,
    running_device: Option<Option<String>>,
    meter: MicMeter,
    clock: Instant,
    tick: Option<Task<()>>,
    tick_running: bool,
}

impl Default for MeterState {
    fn default() -> Self {
        MeterState {
            monitor: None,
            level: None,
            starting: false,
            generation: 0,
            running_device: None,
            meter: MicMeter::default(),
            clock: Instant::now(),
            tick: None,
            tick_running: false,
        }
    }
}

impl StartSheet {
    /// Switches the mode (the header switch, ⌥⇧V from the tray, the launcher). The window is
    /// re-fitted by the next render (the audio sheet is wider).
    pub fn set_mode(&mut self, mode: SheetMode, window: &mut Window, cx: &mut Context<Self>) {
        if self.mode == mode {
            return;
        }
        self.mode = mode;
        if mode == SheetMode::Screen {
            self.mic_stop();
            // The displays were never listed when the sheet opened in Audio mode.
            if matches!(self.model.targets, TargetsLoad::Loading) {
                self.load(cx);
            }
            self.load_windows(cx);
        } else {
            self.mic_sync(cx);
        }
        // The webcam preview follows the mode: off in Audio, back on in Screen.
        self.sync_preview(cx);
        window.focus(&self.focus, cx);
        cx.notify();
    }

    // ---- the meter -------------------------------------------------------------------------

    /// Starts or stops the monitor to match the sheet: Audio mode, microphone on, allowed by the
    /// OS, and the chosen device. Safe to call any time.
    pub(super) fn mic_sync(&mut self, cx: &mut Context<Self>) {
        let want = self.mode == SheetMode::Audio
            && self.audio.mic
            && self.audio.devices.mic_permission == Perm::Authorized;
        if !want {
            self.mic_stop();
            return;
        }
        let device = self.audio.effective_mic_device();
        let running = self.meter.monitor.is_some() || self.meter.starting;
        if running && self.meter.running_device.as_ref() == Some(&device) {
            return;
        }
        if running {
            self.mic_stop();
        }
        self.mic_start(device, cx);
    }

    /// Closes the microphone (the orange dot goes out).
    pub(super) fn mic_stop(&mut self) {
        self.meter.generation += 1;
        self.meter.starting = false;
        self.meter.monitor = None;
        self.meter.level = None;
        self.meter.running_device = None;
        self.meter.meter.reset();
    }

    fn mic_start(&mut self, device: Option<String>, cx: &mut Context<Self>) {
        self.meter.generation += 1;
        let generation = self.meter.generation;
        self.meter.starting = true;
        self.meter.meter.reset();
        self.meter.running_device = Some(device.clone());
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
                if this.meter.generation != generation {
                    // Stopped while the microphone was opening: let it go at once.
                    drop(result);
                    return;
                }
                this.meter.starting = false;
                match result {
                    Ok(monitor) => {
                        this.meter.level = Some(monitor.level_handle());
                        this.meter.monitor = Some(monitor);
                        this.mic_ensure_tick(cx);
                    }
                    Err(_) => this.meter.running_device = None,
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// The meter is polled about 30 times a second while the monitor runs.
    fn mic_ensure_tick(&mut self, cx: &mut Context<Self>) {
        if self.meter.tick_running {
            return;
        }
        self.meter.tick_running = true;
        self.meter.tick = Some(cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(33))
                .await;
            let alive = this
                .update(cx, |this, cx| {
                    let now = this.meter.clock.elapsed().as_millis() as u64;
                    let Some(level) = &this.meter.level else {
                        this.meter.tick_running = false;
                        return false;
                    };
                    let l = level.read();
                    this.meter.meter.push(now, l.peak_dbfs, l.clip_events);
                    cx.notify();
                    true
                })
                .unwrap_or(false);
            if !alive {
                break;
            }
        }));
    }

    // ---- actions ---------------------------------------------------------------------------

    /// Start in Audio mode: the microphone is asked about first when macOS never did (the user
    /// just pressed Start, so this is not silent), then `start_recording { mode: "audio" }`.
    pub(super) fn start_audio(&mut self, cx: &mut Context<Self>) {
        let Some(params) = self.audio.params() else {
            return;
        };
        let (os, state) = {
            let m = self.app.read(cx);
            (m.perms.os, m.perms.state(PermId::Microphone))
        };
        let (mic, _) = self.audio.effective_sources();
        if mic && permissions::ask_before_use(os, PermId::Microphone, state) {
            self.ask_before_use(PermId::Microphone, cx);
            return;
        }
        // The engine opens the microphone itself: let the meter's go first.
        self.mic_stop();
        let remembered = self.audio.remembered();
        let app = self.app.clone();
        cx.defer(move |cx| super::rec::start_audio(&app, params, Some(remembered), cx));
    }

    fn audio_choose_preset(&mut self, preset: Preset, cx: &mut Context<Self>) {
        self.audio.choose(preset);
        self.mic_sync(cx);
        cx.notify();
    }

    fn audio_dismiss_consent(&mut self, cx: &mut Context<Self>) {
        self.audio.consent_dismissed = true;
        self.audio.consent_notice = false;
        let app = self.app.clone();
        cx.defer(move |cx| app::set_setting(&app, model::KEY_CONSENT, json!(false), cx));
        cx.notify();
    }

    // ---- drawing ---------------------------------------------------------------------------

    /// The Screen / Audio switch of the header (board V1): two segments with their shortcut.
    pub(super) fn mode_switch(
        &self,
        t: &Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let (platform, shortcuts) = {
            let a = self.app.read(cx);
            (a.platform, a.shortcuts.clone())
        };
        let hint = |a: ShortcutAction| -> String { shortcuts.get(a).keycaps(platform).concat() };
        let (screen, audio) = model::mode_labels();
        let seg = |mode: SheetMode, glyph: &'static str, label: String, keys: String| {
            let on = self.mode == mode;
            let fg = if on { t.text } else { t.mist };
            let (hover, focus) = (rgba_of(t.raised), rgba_of(t.text));
            div()
                .id(SharedString::from(match mode {
                    SheetMode::Screen => "mode-screen",
                    SheetMode::Audio => "mode-audio",
                }))
                .role(Role::RadioButton)
                .aria_label(SharedString::from(label.clone()))
                .aria_toggled(if on {
                    gpui::Toggled::True
                } else {
                    gpui::Toggled::False
                })
                .focusable()
                .tab_stop(true)
                .h(px(32.))
                .px(px(12.))
                .flex()
                .flex_none()
                .items_center()
                .gap(px(7.))
                .rounded(px(scale::RADIUS_SM))
                .border_1()
                .border_color(gpui::transparent_black())
                .when(on, |d| d.bg(rgba_of(t.segment_selected())))
                .when(!on, |d| d.cursor_pointer().hover(move |s| s.bg(hover)))
                .focus_visible(move |s| s.border_color(focus))
                .text_size(px(13.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgba_of(fg))
                .child(icon(glyph, 14., fg))
                .child(label)
                .child(
                    div()
                        .text_size(px(10.5))
                        .text_color(rgba_of(t.faint))
                        .mono()
                        .child(keys),
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.set_mode(mode, window, cx);
                }))
        };
        div()
            .id("sheet-mode")
            .role(Role::RadioGroup)
            .aria_label(tr!("Mode"))
            .flex_none()
            .flex()
            .gap(px(2.))
            .p(px(3.))
            .rounded(px(11.))
            .bg(rgba_of(t.field))
            .border_1()
            .border_color(rgba_of(t.border))
            .child(seg(
                SheetMode::Screen,
                "monitor",
                screen,
                hint(ShortcutAction::ToggleRecording),
            ))
            .child(seg(
                SheetMode::Audio,
                "mic",
                audio,
                hint(ShortcutAction::ToggleAudioRecording),
            ))
    }

    fn preset_card(
        &self,
        t: &Theme,
        preset: Preset,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let on = self.audio.preset == preset;
        let (hover, focus) = (rgba_of(t.raised), rgba_of(t.text));
        div()
            .id(SharedString::from(format!("preset-{}", preset.key())))
            .role(Role::RadioButton)
            .aria_label(SharedString::from(preset.title()))
            .aria_toggled(if on {
                gpui::Toggled::True
            } else {
                gpui::Toggled::False
            })
            .focusable()
            .tab_stop(true)
            .flex_1()
            .min_w(px(0.))
            .p(px(12.))
            .flex()
            .flex_col()
            .items_start()
            .gap(px(8.))
            .rounded(px(12.))
            .border_1()
            .border_color(rgba_of(if on {
                t.rec.with_alpha(60)
            } else {
                crate::theme::derived::PICKER_CARD_BORDER
            }))
            .bg(rgba_of(if on { t.rec.with_alpha(8) } else { t.surface_2 }))
            .cursor_pointer()
            .when(!on, |d| d.hover(move |s| s.bg(hover)))
            .focus_visible(move |s| s.border_color(focus))
            .child(icon(
                preset.icon(),
                18.,
                if on { t.rec_soft } else { t.text_3 },
            ))
            .child(
                div()
                    .min_w(px(0.))
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgba_of(t.text))
                            .child(SharedString::from(preset.title())),
                    )
                    .child(
                        div()
                            .text_size(px(11.5))
                            .line_height(px(15.5))
                            .text_color(rgba_of(t.mist))
                            .child(SharedString::from(preset.blurb())),
                    ),
            )
            .on_click(cx.listener(move |this, _, _, cx| this.audio_choose_preset(preset, cx)))
    }

    /// The 40-segment meter of the microphone row: grey below the good range, green inside it,
    /// amber above; dark when nothing is listening.
    fn meter_row(&self, t: &Theme) -> gpui::Stateful<gpui::Div> {
        let now = self.meter.clock.elapsed().as_millis() as u64;
        let live = self.audio.mic && self.meter.monitor.is_some();
        let reading = self.meter.meter.reading(now);
        let segs = if live {
            segments(METER_SEGMENTS, reading.level_db, Some(reading.peak_db))
        } else {
            vec![Seg::Unlit; METER_SEGMENTS]
        };
        div()
            .id("mic-level")
            .role(Role::Meter)
            .aria_label(tr!("Microphone level"))
            .w_full()
            .flex()
            .gap(px(2.))
            .children(segs.into_iter().map(|s| {
                let color = match s {
                    Seg::Unlit => t.line,
                    Seg::Quiet => t.mist,
                    Seg::Good => t.success,
                    Seg::Loud => t.warning,
                };
                div().flex_1().h(px(8.)).rounded(px(1.5)).bg(rgba_of(color))
            }))
    }

    fn picker_button(&self, t: &Theme, cx: &mut Context<Self>) -> gpui::Stateful<gpui::Div> {
        let (hover, focus) = (rgba_of(t.raised), rgba_of(t.text));
        div()
            .id("mic-picker")
            .role(Role::Button)
            .aria_label(SharedString::from(tr!(
                "Microphone: {name}",
                name = self.audio.mic_name()
            )))
            .focusable()
            .tab_stop(true)
            .h(px(26.))
            .max_w(px(320.))
            .px(px(9.))
            .flex()
            .items_center()
            .gap(px(6.))
            .rounded(px(7.))
            .border_1()
            .border_color(rgba_of(t.line))
            .bg(rgba_of(t.field))
            .text_size(px(type_scale::CAPTION))
            .text_color(rgba_of(t.text_2))
            .cursor_pointer()
            .hover(move |s| s.bg(hover))
            .focus_visible(move |s| s.border_color(focus))
            .child(ellipsis_text(self.audio.mic_name()))
            .child(icon("chevron-down", 11., t.mist))
            .on_click(cx.listener(|this, _, _, cx| {
                this.audio.picker_open = !this.audio.picker_open;
                cx.notify();
            }))
    }

    fn picker_list(&self, t: &Theme, cx: &mut Context<Self>) -> gpui::Stateful<gpui::Div> {
        let current = self.audio.mic_name();
        let mics = self.audio.devices.mics.clone();
        div()
            .id("mic-list")
            .role(Role::ListBox)
            .aria_label(tr!("Microphones"))
            .flex()
            .flex_col()
            .gap(px(2.))
            .children(mics.into_iter().map(|d| {
                let on = d.name == current;
                let (hover, focus) = (rgba_of(t.raised), rgba_of(t.text));
                let id = d.id.clone();
                div()
                    .id(SharedString::from(format!("mic-{}", d.id)))
                    .role(Role::ListBoxOption)
                    .aria_label(SharedString::from(d.name.clone()))
                    .aria_toggled(if on {
                        gpui::Toggled::True
                    } else {
                        gpui::Toggled::False
                    })
                    .focusable()
                    .tab_stop(true)
                    .h(px(30.))
                    .px(px(10.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .rounded(px(7.))
                    .border_1()
                    .border_color(gpui::transparent_black())
                    .when(on, |x| x.bg(rgba_of(t.segment_selected())))
                    .when(!on, |x| x.hover(move |s| s.bg(hover)))
                    .focus_visible(move |s| s.border_color(focus))
                    .cursor_pointer()
                    .text_size(px(13.))
                    .text_color(rgba_of(if on { t.text } else { t.text_2 }))
                    .child(
                        div()
                            .size(px(14.))
                            .flex_none()
                            .children(on.then(|| icon("check", 14., t.text))),
                    )
                    .child(ellipsis_text(d.name.clone()))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.audio.mic_device = Some(id.clone());
                        this.audio.picker_open = false;
                        this.mic_sync(cx);
                        cx.notify();
                    }))
            }))
    }

    fn note_line(&self, t: &Theme, note: &Note, pane: &'static str) -> gpui::Div {
        super::note_view(t, note, pane)
    }

    /// One row of the sources card: a 32 px tile, the content, the control.
    fn source_row(
        t: &Theme,
        glyph: &'static str,
        content: impl IntoElement,
        control: impl IntoElement,
        first: bool,
    ) -> gpui::Div {
        div()
            .px(px(14.))
            .py(px(12.))
            .flex()
            .items_center()
            .gap(px(12.))
            .when(!first, |d| {
                d.border_t_1().border_color(rgba_of(t.row_divider()))
            })
            .child(
                div()
                    .size(px(32.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(9.))
                    .bg(rgba_of(t.icon_tile()))
                    .child(icon(glyph, 16., t.text)),
            )
            .child(content)
            .child(control)
    }

    fn chip(t: &Theme, glyph: &'static str, label: String, on: bool) -> gpui::Div {
        let fg = if on { t.text_2 } else { t.mist };
        div()
            .h(px(26.))
            .px(px(10.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(6.))
            .rounded(px(scale::RADIUS_PILL))
            .border_1()
            .border_color(rgba_of(t.line))
            .text_color(rgba_of(fg))
            .child(icon(glyph, 12., fg))
            .child(SharedString::from(label))
    }

    fn people_control(&self, t: &Theme, cx: &mut Context<Self>) -> gpui::Stateful<gpui::Div> {
        div()
            .id("people")
            .role(Role::RadioGroup)
            .aria_label(tr!("How many people"))
            .flex_none()
            .flex()
            .gap(px(2.))
            .p(px(3.))
            .rounded(px(9.))
            .bg(rgba_of(t.field))
            .border_1()
            .border_color(rgba_of(t.border))
            .children(People::ALL.into_iter().map(|p| {
                let on = self.audio.people == p;
                let (hover, focus) = (rgba_of(t.raised), rgba_of(t.text));
                div()
                    .id(SharedString::from(format!("people-{}", p.label())))
                    .role(Role::RadioButton)
                    .aria_label(SharedString::from(p.label()))
                    .aria_toggled(if on {
                        gpui::Toggled::True
                    } else {
                        gpui::Toggled::False
                    })
                    .focusable()
                    .tab_stop(true)
                    .h(px(26.))
                    .px(px(9.))
                    .flex()
                    .items_center()
                    .rounded(px(6.))
                    .border_1()
                    .border_color(gpui::transparent_black())
                    .when(on, |d| d.bg(rgba_of(t.segment_selected())))
                    .when(!on, |d| d.cursor_pointer().hover(move |s| s.bg(hover)))
                    .focus_visible(move |s| s.border_color(focus))
                    .text_size(px(type_scale::CAPTION))
                    .text_color(rgba_of(if on { t.text } else { t.mist }))
                    .child(SharedString::from(p.label()))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.audio.people = p;
                        cx.notify();
                    }))
            }))
    }

    pub(super) fn render_audio(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let t = Theme::overlay();
        let (platform, shortcuts) = {
            let a = self.app.read(cx);
            (a.platform, a.shortcuts.clone())
        };
        // The AI page's facts (what runs after stop) and the library's free space, read as the
        // sheet draws: both are cheap and always current.
        {
            let a = self.app.read(cx);
            self.audio.transcribe_model =
                a.ai.auto_transcribe
                    .then(|| a.ai.active().map(|m| m.label.clone()))
                    .flatten();
            let root = a.backend.library_root.clone();
            if self.audio.free_bytes.is_none() {
                use cm_capture::disk::{FreeSpace, SystemFreeSpace};
                self.audio.free_bytes = root.and_then(|r| SystemFreeSpace.free_bytes(&r));
            }
        }
        self.fit(window, cx);
        let m = self.audio.clone();
        let blocker = m.blocker();
        let can_start = blocker.is_none();
        let marker_keys = shortcuts.get(ShortcutAction::AddMarker).keycaps(platform);

        // Header.
        let header = div()
            .flex_shrink_0()
            .px(px(22.))
            .pt(px(18.))
            .pb(px(6.))
            .flex()
            .items_center()
            .gap(px(14.))
            .child(
                div()
                    .size(px(40.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(12.))
                    .border_1()
                    .border_color(rgba_of(t.rec.with_alpha(45)))
                    .bg(rgba_of(t.rec.with_alpha(14)))
                    .child(icon("mic", 20., t.rec_soft)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .text_size(px(20.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(tr_ctx!("record sheet", "Record")),
                    )
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(rgba_of(t.mist))
                            .child(ellipsis_text(model::subtitle())),
                    ),
            )
            .child(self.mode_switch(&t, cx))
            .child(
                button(&t, "sheet-close", "", ButtonKind::Ghost)
                    .aria_label(tr!("Close"))
                    .w(px(34.))
                    .h(px(34.))
                    .px(px(0.))
                    .child(icon("x", 17., t.text_3))
                    .on_click(cx.listener(|this, _, _, cx| this.cancel(cx))),
            );

        // Presets.
        let presets = div()
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(caption(&t, tr!("What are you recording?")))
            .child(
                div()
                    .id("audio-presets")
                    .role(Role::RadioGroup)
                    .aria_label(tr!("What are you recording?"))
                    .flex()
                    .gap(px(8.))
                    .children(Preset::ALL.into_iter().map(|p| self.preset_card(&t, p, cx))),
            );

        // Microphone row: name, device picker, meter; the switch at the right.
        let mic_blocked = m.mic_blocked();
        let mic_switch = switch(
            &t,
            "toggle-mic",
            tr!("Microphone"),
            m.mic && !mic_blocked,
            true,
        )
        .on_click(cx.listener(|this, _, _, cx| {
            this.audio.mic = !this.audio.mic;
            if this.audio.mic {
                this.ask_before_use(PermId::Microphone, cx);
            }
            this.mic_sync(cx);
            cx.notify();
        }));
        let mic_content = div()
            .flex_1()
            .min_w(px(0.))
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(
                        div()
                            .text_size(px(type_scale::UI))
                            .font_weight(FontWeight::MEDIUM)
                            .child(tr!("Microphone")),
                    )
                    .child(self.picker_button(&t, cx)),
            )
            .child(self.meter_row(&t))
            .children(m.picker_open.then(|| self.picker_list(&t, cx)))
            .children(
                m.mic_note()
                    .map(|n| self.note_line(&t, &n, "Privacy_Microphone")),
            );

        // System audio row.
        let sys_supported = m.system_supported();
        let sys_switch = switch(
            &t,
            "toggle-system",
            tr!("System audio"),
            m.system_audio && sys_supported,
            sys_supported,
        )
        .when(sys_supported, |d| {
            d.on_click(cx.listener(|this, _, _, cx| {
                this.audio.system_audio = !this.audio.system_audio;
                cx.notify();
            }))
        });
        let sys_content = div()
            .flex_1()
            .min_w(px(0.))
            .flex()
            .flex_col()
            .gap(px(2.))
            .child(
                div()
                    .text_size(px(type_scale::UI))
                    .font_weight(FontWeight::MEDIUM)
                    .child(tr!("System audio")),
            )
            .child(
                div()
                    .text_size(px(type_scale::CAPTION))
                    .text_color(rgba_of(t.mist))
                    .child(AudioSheetModel::system_hint(platform)),
            )
            .children(
                m.system_note()
                    .map(|n| self.note_line(&t, &n, "Privacy_ScreenCapture")),
            );

        // How many people (optional).
        let people_content = div()
            .flex_1()
            .min_w(px(0.))
            .flex()
            .flex_col()
            .gap(px(2.))
            .child(
                div()
                    .flex()
                    .items_baseline()
                    .gap(px(6.))
                    .text_size(px(type_scale::UI))
                    .font_weight(FontWeight::MEDIUM)
                    .child(shrink_text(tr!("How many people?")))
                    .child(
                        div()
                            .font_weight(FontWeight::NORMAL)
                            .text_color(rgba_of(t.faint))
                            .child(tr_ctx!("people hint", "optional")),
                    ),
            )
            .child(
                div()
                    .text_size(px(type_scale::CAPTION))
                    .text_color(rgba_of(t.mist))
                    .child(tr!("Helps tell the voices apart for speaker labels.")),
            );

        let sources = div()
            .flex_none()
            .flex()
            .flex_col()
            .overflow_hidden()
            .rounded(px(scale::RADIUS_LG))
            .border_1()
            .border_color(rgba_of(t.border))
            .child(Self::source_row(&t, "mic", mic_content, mic_switch, true))
            .child(Self::source_row(
                &t,
                "volume-2",
                sys_content,
                sys_switch,
                false,
            ))
            .children(m.people_visible().then(|| {
                Self::source_row(
                    &t,
                    "users",
                    people_content,
                    self.people_control(&t, cx),
                    false,
                )
            }));

        // After you stop.
        let [transcribe, speakers, report] = m.after_stop();
        let chevron = || icon("chevron-right", 12., t.faint);
        let after = div()
            .flex_none()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.))
            .text_size(px(12.5))
            .text_color(rgba_of(t.mist))
            .child(shrink_text(tr!("After you stop")))
            .child(Self::chip(&t, "captions", transcribe.0, transcribe.1))
            .child(chevron())
            .child(Self::chip(&t, "users", speakers.0, speakers.1))
            .child(chevron())
            .child(Self::chip(&t, "file-text", report.0, report.1));

        // The consent line (VOC-1.AC7).
        let consent = m.consent_visible().then(|| {
            let (hover, focus) = (rgba_of(t.raised), rgba_of(t.text));
            div()
                .id("consent-note")
                .role(Role::Note)
                .flex_none()
                .px(px(14.))
                .py(px(11.))
                .flex()
                .items_center()
                .gap(px(10.))
                .rounded(px(12.))
                .bg(rgba_of(t.agent.with_alpha(7)))
                .border_1()
                .border_color(rgba_of(t.agent.with_alpha(30)))
                .text_size(px(13.))
                .text_color(rgba_of(t.text_2))
                .child(icon("users", 15., t.agent_text))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgba_of(t.text))
                        .child(AudioSheetModel::consent_text()),
                )
                .child(
                    div()
                        .id("consent-dismiss")
                        .role(Role::Button)
                        .aria_label(tr!("Don\u{2019}t show again"))
                        .focusable()
                        .tab_stop(true)
                        .h(px(26.))
                        .px(px(9.))
                        .flex()
                        .flex_none()
                        .items_center()
                        .rounded(px(7.))
                        .border_1()
                        .border_color(gpui::transparent_black())
                        .text_size(px(type_scale::CAPTION))
                        .text_color(rgba_of(t.agent_text))
                        .cursor_pointer()
                        .hover(move |s| s.bg(hover))
                        .focus_visible(move |s| s.border_color(focus))
                        .child(tr!("Don\u{2019}t show again"))
                        .on_click(cx.listener(|this, _, _, cx| this.audio_dismiss_consent(cx))),
                )
        });

        // Disk, lid and length line.
        let fact = |glyph: &'static str, text: String, warn: bool| {
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .child(icon(glyph, 13., if warn { t.warning } else { t.mist }))
                .child(label_fill(text))
        };
        let facts = div()
            .flex_none()
            .flex()
            .flex_wrap()
            .gap(px(18.))
            .text_size(px(type_scale::CAPTION))
            .text_color(rgba_of(if m.disk_short() { t.warning } else { t.mist }))
            .child(fact("hard-drive", m.disk_text(), m.disk_short()))
            .children(
                m.lid_visible()
                    .then(|| fact("laptop", AudioSheetModel::lid_text(), false)),
            )
            .child(fact("hourglass", m.limit_text(), false));

        let body = div()
            .id("sheet-body")
            .track_scroll(&self.scroll)
            .flex_1()
            .min_h(px(0.))
            .overflow_y_scroll()
            .px(px(22.))
            .pt(px(14.))
            .pb(px(18.))
            .flex()
            .flex_col()
            .gap(px(16.))
            .child(presets)
            .child(sources)
            .child(after)
            .children(consent)
            .child(facts);

        let footer = div()
            .flex_shrink_0()
            .px(px(22.))
            .py(px(14.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .border_t_1()
            .border_color(rgba_of(t.border))
            .bg(rgba_of(t.sidebar))
            .children(blocker.map(|b| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(icon_size::SMALL_GAP))
                    .text_size(px(type_scale::CAPTION))
                    .text_color(rgba_of(t.warning))
                    .child(icon("triangle-alert", icon_size::SMALL, t.warning))
                    .child(label_fill(b))
            }))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(10.))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .text_size(px(type_scale::CAPTION))
                            .text_color(rgba_of(t.mist))
                            .child(icon("bookmark", 13., t.mist))
                            .child(shrink_text(tr!("Mark a moment while recording")))
                            .child(keycaps(&t, &marker_keys)),
                    )
                    .child(
                        button(&t, "sheet-cancel", tr!("Cancel"), ButtonKind::Secondary)
                            .on_click(cx.listener(|this, _, _, cx| this.cancel(cx))),
                    )
                    .child(
                        button_with_icon(
                            &t,
                            "sheet-start",
                            tr!("Start recording"),
                            ButtonKind::Record,
                            Some("mic"),
                        )
                        .when(!can_start, |d| d.opacity(0.45).cursor_default())
                        .on_click(cx.listener(move |this, _, _, cx| this.start(cx)))
                        .child(
                            div()
                                .px(px(6.))
                                .py(px(1.))
                                .rounded(px(5.))
                                .bg(rgba_of(t.on_rec.with_alpha(16)))
                                .text_size(px(type_scale::CAPTION))
                                .mono()
                                .child("\u{21B5}"),
                        ),
                    ),
            );

        let panel = div()
            .id("start-sheet")
            .role(Role::Dialog)
            .aria_label(tr!("Record audio"))
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .rounded(px(scale::RADIUS_DIALOG_LG))
            .border_1()
            .border_color(rgba_of(t.line))
            .bg(rgba_of(crate::theme::derived::SHEET_FILL))
            .child(header)
            .child(body)
            .child(footer);

        div()
            .track_focus(&self.focus)
            .size_full()
            .font_family(crate::ui::paint::UI_FONT)
            .font_features(crate::ui::paint::ui_features())
            .text_color(rgba_of(t.text))
            .text_size(px(type_scale::UI))
            .on_key_down(
                cx.listener(|this, ev: &KeyDownEvent, window, cx| this.on_key(ev, window, cx)),
            )
            .child(panel)
    }
}
