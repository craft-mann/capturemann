//! The start sheet: a compact launcher-style panel (DESIGN 5.2: radius 16, token styling) for
//! what to record (display, window or region), the webcam bubble with a live preview, the
//! microphone and system audio (REC-1, CAM-4, AUD-1). Choices are remembered (`recording.*`).
//!
//! Size, frame rate, codec, quality, cursor and countdown are not here: they are Settings ->
//! Capture & recording and the engine reads them, so the sheet, the hotkey and agents all record
//! the same way.

use cm_i18n::{tr, tr_ctx};
use gpui::{
    div, prelude::*, px, Context, Entity, FocusHandle, FontWeight, KeyDownEvent, Render, Role,
    ScrollHandle, SharedString, Subscription, Window,
};
use serde_json::json;

use crate::backend::Backend;
use crate::overlay::CaptureMode;
use crate::permissions::{self, Action as PermAction, Id as PermId, State as PermState};
use crate::recording::audio_sheet::{AudioDefaults, AudioSheetModel, AUDIO_SHEET_WIDTH};
use crate::recording::geometry::Rect;
use crate::recording::sheet::{
    clamp_into, fit_height, kind_icon, Devices, Note, SheetModel, TargetsProblem, SHEET_MARGIN,
    SOURCE_ICONS,
};
use crate::recording::start::{RecordingSettings, SheetDefaults, Target, TargetKind};
use crate::recording::window_rows::{
    self as win_rows, KeyOutcome, ListKey, WindowList, WindowsLoad, KEY_LAST_USED,
};
use crate::shortcuts::ShortcutAction;
use crate::theme::{scale, type_scale, Theme};
use crate::ui::app::{self, AppModel};
use crate::ui::components::{
    button, button_with_icon, ellipsis_text, icon, icon_size, keycaps, label_fill, shrink_text,
    switch, ButtonKind,
};
use crate::ui::kit::{tone_colors, Tone};
use crate::ui::paint::{rgba_of, ui_features, TextExt, UI_FONT};
use crate::ui::recording::window_list;
use crate::ui::recording::{self as rec, RecState};
use crate::ui::settings::widgets::caption;
use crate::ui::text_field::{TextField, TextFieldEvent};
use crate::window_picker::LastUsed;

pub use crate::recording::sheet::SHEET_WIDTH;

mod audio;

/// What the sheet records: the screen (display, window or region) or audio only (VOC-1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SheetMode {
    #[default]
    Screen,
    Audio,
}

/// How a sheet is opened: what the caller pre-filled and where the window was placed.
pub struct SheetOpen {
    /// A window or region picked first.
    pub target: Option<Target>,
    /// `Some(true)` for "Record screen + webcam".
    pub camera: Option<bool>,
    /// One-off choices from a launcher action (LNC-1 Actions); empty keeps the remembered ones.
    pub preset: crate::recording::start::SheetPreset,
    /// The window height the sheet opened with.
    pub height: f64,
    /// The display it opened on (global points).
    pub display: Rect,
    /// The mode it opens in (Audio from the tray, the launcher and the mic button).
    pub mode: SheetMode,
}

pub struct StartSheet {
    app: Entity<AppModel>,
    _rec: Entity<RecState>,
    model: SheetModel,
    /// Screen or Audio (VOC-1): which of the two models the body shows.
    mode: SheetMode,
    /// The Audio mode's model (`recording::audio_sheet`).
    audio: AudioSheetModel,
    /// The live microphone meter of the Audio mode.
    meter: audio::MeterState,
    focus: FocusHandle,
    /// The overlay picker is open.
    picking: bool,
    countdown_s: u32,
    /// The launcher action's one-off `fps` and `countdown_s`, sent with Start.
    preset: crate::recording::start::SheetPreset,
    /// The window height last asked for; the window is re-fitted when the content changes.
    fitted_h: f64,
    native: Option<crate::platform_window::NativeWindow>,
    /// The body's scroll state; its overflow is how far the row estimate is off (debug trace).
    scroll: ScrollHandle,
    /// The display the sheet opened on (global points): the live preview bubble goes there too.
    display: Rect,
    /// The last `RecState::camera_list_epoch` this sheet reloaded for.
    seen_camera_epoch: u64,
    /// R20: the window list of the Window kind.
    windows: WindowList,
    /// The list's search box (Cmd+F focuses it).
    search: Entity<TextField>,
    /// The list's scroll state.
    list_scroll: ScrollHandle,
    /// The row thumbnails (the same service as the R19 picker), asked for visible rows only.
    thumbs: Entity<crate::window_picker::thumbs::Thumbs>,
    /// A keyboard move happened: scroll the cursor row into view at the next paint.
    reveal: bool,
    _subs: Vec<Subscription>,
}

/// NSWindow level of the sheet: one above the other pop-ups (101: toasts, the webcam bubble, the
/// HUD) so nothing that overlaps it can cover Start.
const SHEET_LEVEL: isize = 102;

impl StartSheet {
    pub fn new(
        app: Entity<AppModel>,
        rec: Entity<RecState>,
        defaults: SheetDefaults,
        open: SheetOpen,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        crate::platform_window::set_level(window, SHEET_LEVEL);
        let SheetOpen {
            target,
            camera,
            preset,
            height: initial_h,
            display,
            mode,
        } = open;
        let audio = AudioSheetModel::new(&AudioDefaults::from_settings(
            &app.read(cx).backend.settings.snapshot(),
        ));
        let mut model = SheetModel::new(&defaults);
        if let Some(t) = target {
            model.pick(t);
        }
        if let Some(c) = camera {
            model.camera = c;
        }
        preset.apply(&mut model.mic, &mut model.system_audio);
        demo_overrides::kind(&mut model);
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        let countdown_s = crate::recording::start::RecordingSettings::from_settings(
            &app.read(cx).backend.settings.snapshot(),
        )
        .countdown_s;
        let countdown_s = preset.countdown_s.unwrap_or(countdown_s);
        let seen_camera_epoch = rec.read(cx).camera_list_epoch;
        let search = cx.new(|cx| {
            TextField::new(
                tr!("Search windows"),
                tr!("Search windows by title or app"),
                cx,
            )
            .height(34.)
        });
        let subs = vec![
            cx.subscribe(&search, |this, _, ev: &TextFieldEvent, cx| match ev {
                TextFieldEvent::Changed(text) => {
                    this.windows.set_query(text);
                    this.list_scroll.set_offset(gpui::point(px(0.), px(0.)));
                    cx.notify();
                }
                TextFieldEvent::Submitted => this.search_enter(cx),
                // Escape bubbles up to the sheet, which cancels.
                TextFieldEvent::Escaped => {}
            }),
            cx.observe(&rec, |this, rec, cx| {
                // CAM-1: a camera was plugged in or unplugged while the sheet is open.
                let epoch = rec.read(cx).camera_list_epoch;
                if epoch != this.seen_camera_epoch {
                    this.seen_camera_epoch = epoch;
                    this.load_devices(cx);
                }
                cx.notify()
            }),
        ];
        let backend_for_thumbs = app.read(cx).backend.clone();
        let thumbs = cx.new(|_| crate::window_picker::thumbs::Thumbs::new(backend_for_thumbs));
        let mut subs = subs;
        subs.push(cx.observe(&thumbs, |_, _, cx| cx.notify()));
        let mut this = StartSheet {
            app,
            _rec: rec,
            model,
            mode,
            audio,
            meter: audio::MeterState::default(),
            focus,
            picking: false,
            countdown_s,
            preset,
            fitted_h: initial_h,
            native: crate::platform_window::NativeWindow::of(window),
            scroll: ScrollHandle::new(),
            display,
            seen_camera_epoch,
            windows: WindowList::default(),
            search,
            list_scroll: ScrollHandle::new(),
            thumbs,
            reveal: false,
            _subs: subs,
        };
        this.load(cx);
        this.load_windows(cx);
        if std::env::var_os("CM_DEMO_SHEET_TRACE").is_some() {
            // Debug only: re-render now and then so the measured overflow can be read once the
            // layout has settled.
            cx.spawn(async move |this, cx| loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(700))
                    .await;
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            })
            .detach();
        }
        this
    }

    /// Reads the displays and the devices from the daemon (nothing is opened: this only lists).
    /// A failed listing is kept as a failure (REC-1): it is not an empty display list.
    fn load(&mut self, cx: &mut Context<Self>) {
        let backend = self.app.read(cx).backend.clone();
        let Some(daemon) = backend.daemon.clone() else {
            self.model.set_targets_failed(
                Some(cm_core::ErrorCode::DaemonUnavailable),
                &tr!("CaptureMann\u{2019}s background service is not running."),
            );
            return;
        };
        if self.mode == SheetMode::Audio {
            // VOC-1: an audio recording never touches the screen, so the displays are not even
            // listed until the user switches to Screen. Only the devices are read.
            self.load_devices(cx);
            return;
        }
        let rt = backend.rt.clone();
        cx.spawn(async move |this, cx| {
            let (d1, d2) = (daemon.clone(), daemon);
            let displays = rt
                .spawn(async move {
                    Backend::call_coded(&d1, "list_targets", json!({ "windows": false })).await
                })
                .await;
            let devices = rt
                .spawn(async move { Backend::call(&d2, "list_devices", json!({})).await })
                .await;
            this.update(cx, |this, cx| {
                match &displays {
                    Ok(Ok(v)) => this.model.apply_targets(Ok(v)),
                    Ok(Err(e)) => this.model.apply_targets(Err((e.code, e.message.as_str()))),
                    Err(e) => this
                        .model
                        .apply_targets(Err((None, e.to_string().as_str()))),
                }
                if let Ok(Ok(v)) = devices {
                    let d = Devices::parse(&demo_overrides::devices(v));
                    this.audio.set_devices(d.clone());
                    this.model.set_devices(d);
                }
                this.sync_preview(cx);
                this.mic_sync(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Reads only the devices again (a camera appeared or went away, CAM-1). The displays and
    /// the user's choices stay; `set_devices` applies CAM-4.AC2 to the remembered camera.
    fn load_devices(&mut self, cx: &mut Context<Self>) {
        let backend = self.app.read(cx).backend.clone();
        let Some(daemon) = backend.daemon.clone() else {
            return;
        };
        let rt = backend.rt.clone();
        cx.spawn(async move |this, cx| {
            let devices = rt
                .spawn(async move { Backend::call(&daemon, "list_devices", json!({})).await })
                .await;
            this.update(cx, |this, cx| {
                if let Ok(Ok(v)) = devices {
                    let d = Devices::parse(&demo_overrides::devices(v));
                    this.audio.set_devices(d.clone());
                    this.model.set_devices(d);
                    this.sync_preview(cx);
                    this.mic_sync(cx);
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// Retry after a failed listing.
    fn retry(&mut self, cx: &mut Context<Self>) {
        self.model.set_targets_loading();
        cx.notify();
        self.load(cx);
    }

    /// R20: reads the windows (`list_targets`, front to back, own windows omitted) and the
    /// Last used memory. Called when the sheet opens on Window, when the kind changes to Window
    /// and after "Pick on screen" returns.
    fn load_windows(&mut self, cx: &mut Context<Self>) {
        if self.model.kind != TargetKind::Window || self.mode == SheetMode::Audio {
            return;
        }
        let backend = self.app.read(cx).backend.clone();
        let snapshot = backend.settings.snapshot();
        let excluded = RecordingSettings::from_settings(&snapshot).excluded_apps;
        if let Some(v) = snapshot.get(KEY_LAST_USED) {
            self.windows.last_used = LastUsed::from_value(v);
        }
        let Some(daemon) = backend.daemon.clone() else {
            self.windows.set_failed(&tr!(
                "CaptureMann\u{2019}s background service is not running."
            ));
            return;
        };
        self.windows.load = WindowsLoad::Loading;
        let rt = backend.rt.clone();
        cx.spawn(async move |this, cx| {
            let res = rt
                .spawn(async move {
                    Backend::call_coded(&daemon, "list_targets", json!({ "windows": true })).await
                })
                .await;
            this.update(cx, |this, cx| {
                match &res {
                    Ok(Ok(v)) => this.windows.apply(v, &excluded),
                    Ok(Err(e)) => this
                        .windows
                        .set_failed(&crate::daemon_text::error_text(e.code, &e.message)),
                    Err(e) => this.windows.set_failed(&e.to_string()),
                }
                let chosen = this.chosen_window();
                this.windows.cursor_to(chosen);
                let placeholder = this.windows.placeholder();
                this.search.update(cx, |s, cx| {
                    s.set_labels(tr!("Search windows"), placeholder, cx)
                });
                this.reveal = true;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// The id of the window chosen for the recording, if the target is a window.
    fn chosen_window(&self) -> Option<u32> {
        match self.model.picked {
            Some(Target::Window { window_id }) if self.model.kind == TargetKind::Window => {
                Some(window_id)
            }
            _ => None,
        }
    }

    pub(super) fn choose_window(&mut self, id: u32, cx: &mut Context<Self>) {
        self.model.pick(Target::Window { window_id: id });
        self.windows.cursor = Some(id);
        cx.notify();
    }

    /// "Pick on screen": the old hover mode; the choice comes back into the list.
    pub(super) fn pick_on_screen(&mut self, cx: &mut Context<Self>) {
        self.choose(CaptureMode::Window, cx);
    }

    /// An Include chip was pressed (R20): the same state and permission asks as the switches.
    pub(super) fn toggle_chip(
        &mut self,
        source: crate::recording::sheet::Source,
        cx: &mut Context<Self>,
    ) {
        use crate::recording::sheet::Source;
        if !self.model.toggle_source(source) {
            return;
        }
        match source {
            Source::Webcam if self.model.camera => self.ask_before_use(PermId::Camera, cx),
            Source::Microphone if self.model.mic => self.ask_before_use(PermId::Microphone, cx),
            _ => {}
        }
        if source == Source::Webcam {
            self.sync_preview(cx);
        }
        cx.notify();
    }

    fn on_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let k = &ev.keystroke;
        let key = k.key.as_str();
        if key == "escape" {
            self.cancel(cx);
            return;
        }
        if self.mode == SheetMode::Screen && self.model.kind == TargetKind::Window {
            let find = if cfg!(target_os = "macos") {
                k.modifiers.platform
            } else {
                k.modifiers.control
            };
            // Cmd+F (Ctrl+F on Windows) goes to the search box.
            if find && key == "f" {
                let focus = self.search.read(cx).focus.clone();
                window.focus(&focus, cx);
                return;
            }
            let plain = !(k.modifiers.platform || k.modifiers.control || k.modifiers.alt);
            let in_search = self.search.read(cx).focus.is_focused(window);
            let on_sheet = self.focus.is_focused(window);
            // Up and Down also work from the search box; the other keys only when nothing with
            // its own meaning (a chip, the search text) has the focus.
            let list_key = match key {
                "up" if plain && (on_sheet || in_search) => Some(ListKey::Up),
                "down" if plain && (on_sheet || in_search) => Some(ListKey::Down),
                "home" if plain && on_sheet => Some(ListKey::Home),
                "end" if plain && on_sheet => Some(ListKey::End),
                "space" if plain && on_sheet => Some(ListKey::Space),
                "enter" if plain && on_sheet => Some(ListKey::Enter),
                _ => None,
            };
            if let Some(lk) = list_key {
                self.list_key(lk, cx);
                return;
            }
        }
        if key == "enter" {
            self.start(cx);
        }
    }

    /// Enter in the search box: choose the top result (or start when it is already chosen).
    fn search_enter(&mut self, cx: &mut Context<Self>) {
        let chosen = self.chosen_window();
        if self.windows.cursor.is_none() && self.windows.is_searching() {
            self.windows.key(ListKey::Home, chosen);
        }
        let outcome = self.windows.key(ListKey::Enter, chosen);
        self.apply_outcome(outcome, cx);
        self.reveal = true;
        cx.notify();
    }

    fn apply_outcome(&mut self, outcome: KeyOutcome, cx: &mut Context<Self>) {
        match outcome {
            KeyOutcome::Nothing => {}
            KeyOutcome::Select(id) => self.choose_window(id, cx),
            KeyOutcome::Start => self.start(cx),
        }
    }

    /// A key for the list (arrows, Home, End, Space, Enter). Returns whether it was used.
    fn list_key(&mut self, key: ListKey, cx: &mut Context<Self>) -> bool {
        let chosen = self.chosen_window();
        let outcome = self.windows.key(key, chosen);
        self.apply_outcome(outcome, cx);
        self.reveal = true;
        cx.notify();
        true
    }

    /// PRM-1.AC7: turning the camera or microphone on asks the OS first when it never asked, then
    /// reads the devices again. Never silent: this runs from the toggle the user pressed.
    fn ask_before_use(&mut self, id: PermId, cx: &mut Context<Self>) {
        let (os, state) = {
            let m = self.app.read(cx);
            (m.perms.os, m.perms.state(id))
        };
        if !permissions::ask_before_use(os, id, state) {
            return;
        }
        let app = self.app.clone();
        let this = cx.entity();
        cx.defer(move |cx| {
            app::request_permission_then(&app, id, cx, move |cx| {
                this.update(cx, |s, cx| s.load(cx));
            })
        });
    }

    /// CAM-4: the bubble shows the live camera as soon as the camera is on, before recording.
    fn sync_preview(&mut self, cx: &mut Context<Self>) {
        let app = self.app.clone();
        // The webcam bubble belongs to the screen sheet: Audio mode shows no camera preview.
        let (on, dev) = (
            self.mode == SheetMode::Screen
                && self.model.camera
                && self.model.camera_available()
                && !self.model.devices.camera_permission.blocked(),
            self.model.camera_device.clone(),
        );
        let display = self.display;
        cx.defer(move |cx| rec::set_setup_camera(&app, on, dev, Some(display), cx));
    }

    /// Keeps the window as tall as the rows need (up to what the display allows) after the model
    /// changed: the devices arrived, a window or region was chosen, the camera went on or off.
    /// The top-left stays put unless the bottom would leave the visible part of the display.
    fn fit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((x, y, w, h)) = crate::platform_window::frame(window) else {
            return;
        };
        let visible = crate::platform_window::visible_frame_at(x + w / 2.0, y + 8.0)
            .map(|(vx, vy, vw, vh)| rec::demo_short(Rect::new(vx, vy, vw, vh)));
        let Some(visible) = visible else { return };
        let (heights, want_w) = match self.mode {
            SheetMode::Screen => (self.model.heights(), SHEET_WIDTH),
            SheetMode::Audio => (self.audio.heights(), AUDIO_SHEET_WIDTH),
        };
        if std::env::var_os("CM_DEMO_SHEET_TRACE").is_some() {
            // Debug: what the body cannot show at this height is the estimate's error.
            let overflow = f64::from(f32::from(self.scroll.max_offset().y.abs()));
            log::info!(
                "sheet: window {h} est {} overflow {overflow}",
                heights.total()
            );
        }
        let want = fit_height(&heights, visible.h);
        if (want - self.fitted_h).abs() < 0.5 && (want - h).abs() < 1.5 && (want_w - w).abs() < 1.5
        {
            return;
        }
        self.fitted_h = want;
        // A mode with another width keeps the window's centre.
        let x = x - (want_w - w) / 2.0;
        let to = clamp_into(Rect::new(x, y, want_w, want), &visible, SHEET_MARGIN);
        // After the current window update (see `NativeWindow`), or the new size is never drawn.
        if let Some(n) = self.native.clone() {
            cx.spawn(async move |_, _| n.set_frame(to.x, to.y, to.w, to.h))
                .detach();
        }
    }

    fn start(&mut self, cx: &mut Context<Self>) {
        if self.mode == SheetMode::Audio {
            return self.start_audio(cx);
        }
        if self.model.blocker().is_some() || self.picking {
            return;
        }
        let Some(opts) = self.model.options() else {
            return;
        };
        // R20: the window being recorded becomes the first of "Last used" (app-only setting,
        // written like the sheet's other choices, through the daemon's `set_settings`).
        if let Target::Window { window_id } = opts.target {
            if let Some(value) = self.windows.remember(window_id) {
                let backend = self.app.read(cx).backend.clone();
                if let Some(daemon) = backend.daemon.clone() {
                    let mut values = serde_json::Map::new();
                    values.insert(KEY_LAST_USED.to_owned(), value);
                    backend.rt.spawn(async move {
                        let _ = Backend::call(&daemon, "set_settings", json!({ "values": values }))
                            .await;
                    });
                }
            }
        }
        let app = self.app.clone();
        let extra = self.preset.extra_params();
        cx.defer(move |cx| rec::start_with(&app, opts, extra, cx));
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        self.mic_stop();
        let app = self.app.clone();
        cx.defer(move |cx| rec::cancel_sheet(&app, cx));
    }

    fn choose(&mut self, mode: CaptureMode, cx: &mut Context<Self>) {
        if self.picking {
            return;
        }
        self.picking = true;
        cx.notify();
        let backend = self.app.read(cx).backend.clone();
        cx.spawn(async move |this, cx| {
            let b = backend.clone();
            let res = backend
                .rt
                .spawn(async move { b.overlay.pick(mode).await })
                .await;
            this.update(cx, |this, cx| {
                this.picking = false;
                match res {
                    Ok(Ok(Some(p))) => {
                        this.model.pick(p.target());
                        // R20: the chosen window shows in the list (it may be new to it).
                        let chosen = this.chosen_window();
                        this.windows.cursor_to(chosen);
                        this.load_windows(cx);
                    }
                    Ok(Ok(None)) => {}
                    Ok(Err(e)) => {
                        let app = this.app.clone();
                        cx.defer(move |cx| {
                            app::push_toast(&app, crate::flows::overlay_error_toast(&e), cx)
                        });
                    }
                    Err(_) => {}
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

fn note_view(t: &Theme, note: &Note, pane: &'static str) -> gpui::Div {
    match note {
        Note::Info(text) => div()
            .text_size(px(type_scale::CAPTION))
            .text_color(rgba_of(t.mist))
            .child(SharedString::from(text.clone())),
        Note::Problem { text, settings } => div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .items_start()
                    .text_size(px(type_scale::CAPTION))
                    .text_color(rgba_of(t.text_2))
                    .child(icon("triangle-alert", icon_size::SMALL, t.warning))
                    .child(label_fill(text.clone())),
            )
            .when(*settings, |d| {
                d.child(
                    div().flex().child(
                        button(
                            t,
                            "open-system-settings",
                            tr!("Open System Settings"),
                            ButtonKind::Secondary,
                        )
                        .on_click(move |_, _, _| app::open_privacy_pane(pane)),
                    ),
                )
            }),
    }
}

impl Render for StartSheet {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        match self.mode {
            SheetMode::Screen => self.render_screen(window, cx).into_any_element(),
            SheetMode::Audio => self.render_audio(window, cx).into_any_element(),
        }
    }
}

impl StartSheet {
    fn render_screen(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = Theme::overlay();
        let m = self.model.clone();
        let (platform, shortcuts) = {
            let a = self.app.read(cx);
            (a.platform, a.shortcuts.clone())
        };
        let toggle_keys = shortcuts
            .get(ShortcutAction::ToggleRecording)
            .keycaps(platform);
        self.fit(window, cx);
        let blocker = m.blocker();
        let can_start = blocker.is_none() && !self.picking;

        // What to record.
        let kinds = TargetKind::ALL
            .into_iter()
            .map(|k| {
                kind_tile(&t, k, m.kind == k).on_click(cx.listener(move |this, _, _, cx| {
                    this.model.set_kind(k);
                    this.load_windows(cx);
                    cx.notify();
                }))
            })
            .collect::<Vec<_>>();
        // With several displays the chips carry the name and size, so the summary line only
        // shows for one display, a window or a region (M32).
        let show_summary = !(m.kind == TargetKind::Display && m.displays.len() > 1)
            && m.kind != TargetKind::Window;
        let target_row = show_summary.then(|| {
            let summary = m.target_summary();
            let chosen = m.target().is_some();
            div()
                .flex_none()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(12.))
                .child(
                    div()
                        .id("target-summary")
                        .role(Role::Status)
                        .aria_label(SharedString::from(tr!(
                            "Recording target: {summary}",
                            summary = summary
                        )))
                        .min_w(px(0.))
                        .truncate()
                        .text_size(px(13.))
                        .text_color(rgba_of(if chosen { t.text } else { t.text_3 }))
                        .mono()
                        .child(SharedString::from(summary)),
                )
                .when(m.kind != TargetKind::Display, |d| {
                    let (mode, label) = if m.kind == TargetKind::Window {
                        (
                            CaptureMode::Window,
                            if chosen {
                                tr!("Choose another\u{2026}")
                            } else {
                                tr!("Choose a window\u{2026}")
                            },
                        )
                    } else {
                        (
                            CaptureMode::Region,
                            if chosen {
                                tr!("Choose another\u{2026}")
                            } else {
                                tr!("Choose a region\u{2026}")
                            },
                        )
                    };
                    d.child(
                        button_with_icon(
                            &t,
                            "choose-target",
                            &label,
                            ButtonKind::Secondary,
                            Some(kind_icon(m.kind)),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| this.choose(mode, cx))),
                    )
                })
        });
        let problem = m.targets_problem().map(|p| {
            let state = self.app.read(cx).perms.state(PermId::ScreenRecording);
            let line = |text: String| {
                div()
                    .flex()
                    .gap(px(8.))
                    .items_start()
                    .text_size(px(type_scale::CAPTION))
                    .text_color(rgba_of(t.text_2))
                    .child(icon("triangle-alert", icon_size::SMALL, t.warning))
                    .child(label_fill(text))
            };
            match p {
                TargetsProblem::PermissionMissing { text, hint } => {
                    let can_grant = permissions::actions(
                        permissions::Os::current(),
                        PermId::ScreenRecording,
                        if state.is_granted() {
                            PermState::NotGranted
                        } else {
                            state
                        },
                    )
                    .contains(&PermAction::Grant);
                    div()
                        .id("targets-problem")
                        .role(Role::Alert)
                        .flex()
                        .flex_col()
                        .gap(px(6.))
                        .child(line(text))
                        .child(caption(&t, hint))
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
                                .gap(px(8.))
                                .child(
                                    button(
                                        &t,
                                        "open-system-settings",
                                        tr!("Open System Settings"),
                                        ButtonKind::Secondary,
                                    )
                                    .on_click(
                                        move |_, _, _| {
                                            app::open_privacy_pane("Privacy_ScreenCapture")
                                        },
                                    ),
                                )
                                .when(can_grant, |d| {
                                    d.child(
                                        button(
                                            &t,
                                            "grant-screen-access",
                                            tr!("Grant access\u{2026}"),
                                            ButtonKind::Secondary,
                                        )
                                        .on_click(
                                            cx.listener(|this, _, _, cx| {
                                                let app = this.app.clone();
                                                cx.defer(move |cx| {
                                                    app::permission_action(
                                                        &app,
                                                        PermId::ScreenRecording,
                                                        PermAction::Grant,
                                                        cx,
                                                    )
                                                });
                                            }),
                                        ),
                                    )
                                })
                                .child(
                                    button(
                                        &t,
                                        "open-permissions",
                                        tr!("Open Permissions\u{2026}"),
                                        ButtonKind::Ghost,
                                    )
                                    .on_click(cx.listener(
                                        |this, _, _, cx| {
                                            let app = this.app.clone();
                                            cx.defer(move |cx| {
                                                app::open_permissions_page(&app, cx)
                                            });
                                        },
                                    )),
                                ),
                        )
                }
                TargetsProblem::Error { message } => div()
                    .id("targets-problem")
                    .role(Role::Alert)
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(line(message))
                    .child(
                        div().flex().child(
                            button(&t, "retry-targets", tr!("Retry"), ButtonKind::Secondary)
                                .on_click(cx.listener(|this, _, _, cx| this.retry(cx))),
                        ),
                    ),
            }
        });
        let display_choices = (m.kind == TargetKind::Display && m.displays.len() > 1).then(|| {
            div()
                .flex_none()
                .flex()
                .flex_wrap()
                .gap(px(8.))
                .children(m.displays.iter().map(|d| {
                    let id = d.id;
                    let on = m.display_id == Some(id);
                    let label = tr!(
                        "{name} \u{00B7} {w} \u{00D7} {h}",
                        name = d.name,
                        w = d.size[0],
                        h = d.size[1]
                    );
                    chip_button(&t, format!("display-{id}"), &label, on, "monitor").on_click(
                        cx.listener(move |this, _, _, cx| {
                            this.model.display_id = Some(id);
                            cx.notify();
                        }),
                    )
                }))
        });

        // Camera.
        let cam_enabled = m.camera_available();
        let camera_switch = switch(
            &t,
            "toggle-camera",
            tr!("Webcam bubble"),
            m.camera && cam_enabled,
            cam_enabled,
        )
        .when(cam_enabled, |d| {
            d.on_click(cx.listener(|this, _, _, cx| {
                this.model.camera = !this.model.camera;
                if this.model.camera {
                    this.ask_before_use(PermId::Camera, cx);
                }
                this.sync_preview(cx);
                cx.notify();
            }))
        });
        let camera_devices = (m.camera && m.devices.cameras.len() > 1).then(|| {
            div()
                .flex()
                .flex_wrap()
                .gap(px(8.))
                .children(m.devices.cameras.iter().map(|c| {
                    let id = c.id.clone();
                    let on = m.camera_device.as_deref() == Some(c.id.as_str())
                        || (m.camera_device.is_none() && c.is_default);
                    let sel = id.clone();
                    chip_button(&t, format!("camera-{id}"), &c.name, on, "video").on_click(
                        cx.listener(move |this, _, _, cx| {
                            this.model.camera_device = Some(sel.clone());
                            this.model.camera_fell_back = false;
                            this.sync_preview(cx);
                            cx.notify();
                        }),
                    )
                }))
        });

        // Microphone and system audio.
        let mic_blocked = m.devices.mic_permission.blocked();
        let mic_switch = switch(
            &t,
            "toggle-mic",
            tr!("Microphone"),
            m.mic && !mic_blocked,
            true,
        )
        .on_click(cx.listener(|this, _, _, cx| {
            this.model.mic = !this.model.mic;
            if this.model.mic {
                this.ask_before_use(PermId::Microphone, cx);
            }
            cx.notify();
        }));
        let sys_supported = m.system_audio_note().is_none();
        let sys_switch = switch(
            &t,
            "toggle-system",
            tr!("System audio"),
            m.system_audio && sys_supported,
            sys_supported,
        )
        .when(sys_supported, |d| {
            d.on_click(cx.listener(|this, _, _, cx| {
                this.model.system_audio = !this.model.system_audio;
                cx.notify();
            }))
        });
        // The three rows share one card (R9); the problem notes and the camera chips stay under
        // their row (M35 to M37).
        let [cam_icon, mic_icon, sys_icon] = SOURCE_ICONS;
        let camera_extras: Vec<gpui::AnyElement> = camera_devices
            .into_iter()
            .map(IntoElement::into_any_element)
            .chain(
                m.camera_note()
                    .map(|n| note_view(&t, &n, "Privacy_Camera").into_any_element()),
            )
            .collect();
        let (camera_extras, camera_extras_for_chips) = if m.kind == TargetKind::Window {
            (Vec::new(), camera_extras)
        } else {
            (camera_extras, Vec::new())
        };
        let mic_extras: Vec<gpui::AnyElement> = m
            .mic_note()
            .map(|n| note_view(&t, &n, "Privacy_Microphone").into_any_element())
            .into_iter()
            .collect();
        // Every block of the scrolling body is `flex_none`: a shrinkable block with hidden overflow
        // (this card) is squeezed to the viewport instead of making the body scroll, which cut
        // the rows below Microphone.
        let sources = div()
            .flex_none()
            .flex()
            .flex_col()
            .overflow_hidden()
            .rounded(px(scale::RADIUS_LG))
            .border_1()
            .border_color(rgba_of(t.border))
            .child(source_row(
                &t,
                cam_icon,
                &tr!("Webcam bubble"),
                m.camera_hint(),
                camera_switch,
                camera_extras,
                false,
            ))
            .child(source_row(
                &t,
                mic_icon,
                &tr!("Microphone"),
                m.mic_hint(),
                mic_switch,
                mic_extras,
                false,
            ))
            .child(source_row(
                &t,
                sys_icon,
                &tr!("System audio"),
                m.system_hint(),
                sys_switch,
                Vec::new(),
                true,
            ));

        // R20: the Window kind shows the list and the Include chips instead of the target row and
        // the three source rows. The camera choice and the problem notes stay under the chips.
        let window_blocks: Vec<gpui::AnyElement> = if m.kind == TargetKind::Window {
            if self.reveal {
                self.reveal = false;
                window_list::reveal_cursor(&self.windows, &self.list_scroll);
            }
            let find_keys = match platform {
                crate::shortcuts::Platform::Mac => vec!["\u{2318}".to_owned(), "F".to_owned()],
                _ => vec!["Ctrl".to_owned(), "F".to_owned()],
            };
            let chosen = self.chosen_window();
            let mut extras: Vec<gpui::AnyElement> = Vec::new();
            extras.extend(camera_extras_for_chips);
            extras.extend(
                m.mic_note()
                    .map(|n| note_view(&t, &n, "Privacy_Microphone").into_any_element()),
            );
            extras.extend(m.system_audio_note().map(|n| {
                div()
                    .text_size(px(type_scale::CAPTION))
                    .text_color(rgba_of(t.mist))
                    .child(SharedString::from(n))
                    .into_any_element()
            }));
            vec![
                div()
                    .flex_none()
                    .flex()
                    .flex_col()
                    .gap(px(win_rows::SEARCH_GAP))
                    .child(window_list::search_bar(
                        &t,
                        &self.search,
                        &find_keys,
                        self.picking,
                        cx,
                    ))
                    .child(window_list::list_card(
                        &t,
                        &self.windows,
                        chosen,
                        &self.list_scroll,
                        &self.thumbs,
                        cx,
                    ))
                    .into_any_element(),
                div()
                    .flex_none()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(window_list::include_row(&t, m.include_chips(), cx))
                    .children(extras)
                    .into_any_element(),
            ]
        } else {
            Vec::new()
        };

        let countdown_text = if self.countdown_s == 0 {
            tr!("Starts immediately")
        } else {
            tr!(
                "{secs} s countdown \u{00B7} Space skips, Esc cancels",
                secs = self.countdown_s
            )
        };
        // R20: the footer echoes the chosen window ("Docs: Q4 launch plan \u{00B7} Google Chrome").
        let footer_window = (m.kind == TargetKind::Window).then(|| {
            let chosen_text = win_rows::footer_text(self.chosen_window(), &self.windows);
            let text = chosen_text.clone().unwrap_or_else(|| m.target_summary());
            div()
                .id("target-summary")
                .role(Role::Status)
                .aria_label(SharedString::from(tr!(
                    "Recording target: {summary}",
                    summary = text
                )))
                .flex()
                .items_center()
                .gap(px(6.))
                .text_size(px(type_scale::CAPTION))
                .text_color(rgba_of(if chosen_text.is_some() {
                    t.text_2
                } else {
                    t.mist
                }))
                .child(icon("app-window", 13., t.text_2))
                .child(ellipsis_text(text))
        });
        let header = div()
            .flex_shrink_0()
            .px(px(22.))
            .pt(px(20.))
            .pb(px(6.))
            .flex()
            .items_center()
            .gap(px(14.))
            .child(
                div()
                    .size(px(44.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(12.))
                    .border_1()
                    .border_color(rgba_of(t.rec.with_alpha(45)))
                    .bg(rgba_of(t.rec.with_alpha(14)))
                    .child(icon("cm-record", 20., t.rec_soft)),
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
                            .child(ellipsis_text(tr!("Choose what goes in the recording."))),
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
        // The only part that scrolls: when the content is taller than the window (a short
        // display, many permission notes) the rows scroll and the footer stays where it is.
        let body = div()
            .id("sheet-body")
            .track_scroll(&self.scroll)
            .flex_1()
            .min_h(px(0.))
            .overflow_y_scroll()
            .px(px(22.))
            .pt(px(16.))
            .pb(px(20.))
            .flex()
            .flex_col()
            .gap(px(18.))
            .child(
                div()
                    .id("target-kinds")
                    .role(Role::RadioGroup)
                    .aria_label(tr!("What to record"))
                    .flex_none()
                    .flex()
                    .gap(px(8.))
                    .children(kinds),
            )
            .children(problem.map(|p| problem_card(&t, p)))
            .children(display_choices)
            .children(target_row)
            .children(window_blocks)
            .children((m.kind != TargetKind::Window).then_some(sources));
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
                    .items_center()
                    .justify_between()
                    .gap(px(12.))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .children(footer_window)
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.))
                                    .text_size(px(type_scale::CAPTION))
                                    .text_color(rgba_of(t.mist))
                                    .child(icon("timer", 13., t.mist))
                                    .child(shrink_text(countdown_text)),
                            )
                            .children((m.kind != TargetKind::Window).then(|| {
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.))
                                    .child(caption(&t, tr!("Start or stop from anywhere")))
                                    .child(keycaps(&t, &toggle_keys))
                            })),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(10.))
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
                                    Some("cm-record"),
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
                    ),
            );
        let panel = div()
            .id("start-sheet")
            .role(Role::Dialog)
            .aria_label(tr!("Start recording"))
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
            .font_family(UI_FONT)
            .font_features(ui_features())
            .text_color(rgba_of(t.text))
            .text_size(px(type_scale::UI))
            .on_key_down(
                cx.listener(|this, ev: &KeyDownEvent, window, cx| this.on_key(ev, window, cx)),
            )
            .child(panel)
    }
}

/// A selectable pill for a display or camera (R9): 32 px, mono 12, a leading 13 px icon; the
/// selected one is filled (`segment_selected`) with a stronger border.
fn chip_button(
    t: &Theme,
    id: impl Into<gpui::ElementId>,
    label: &str,
    on: bool,
    glyph: &'static str,
) -> gpui::Stateful<gpui::Div> {
    let (hover, focus) = (rgba_of(t.raised), rgba_of(t.text));
    let fg = if on { t.text } else { t.text_2 };
    div()
        .id(id)
        .role(Role::RadioButton)
        .aria_label(SharedString::from(label.to_owned()))
        .aria_toggled(if on {
            gpui::Toggled::True
        } else {
            gpui::Toggled::False
        })
        .focusable()
        .tab_stop(true)
        .h(px(CHIP_H))
        .max_w_full()
        .px(px(12.))
        .flex()
        .items_center()
        .gap(px(7.))
        .rounded(px(scale::RADIUS_PILL))
        .border_1()
        .border_color(rgba_of(if on { t.faint } else { t.line }))
        .when(on, |d| d.bg(rgba_of(t.segment_selected())))
        .text_size(px(type_scale::CAPTION))
        .text_color(rgba_of(fg))
        .mono()
        .cursor_pointer()
        .when(!on, |d| d.hover(move |s| s.bg(hover)))
        .focus_visible(move |s| s.border_color(focus))
        .child(icon(glyph, 13., fg))
        .child(ellipsis_text(label.to_owned()))
}

/// Height of a display or camera chip (the model's `row::CHIPS`).
const CHIP_H: f32 = 32.;

/// One kind tile of the target radiogroup (R9): 84 px tall, the kind's icon over its label; the
/// selected tile has the rec tint (the Record flow's own colour).
fn kind_tile(t: &Theme, k: TargetKind, on: bool) -> gpui::Stateful<gpui::Div> {
    let (hover, focus) = (rgba_of(t.raised), rgba_of(t.text));
    let fg = if on { t.text } else { t.text_2 };
    div()
        .id(SharedString::from(format!("kind-{}", k.key())))
        .role(Role::RadioButton)
        .aria_label(k.label())
        .aria_toggled(if on {
            gpui::Toggled::True
        } else {
            gpui::Toggled::False
        })
        .focusable()
        .tab_stop(true)
        .flex_1()
        .min_w(px(0.))
        .h(px(84.))
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(8.))
        .rounded(px(scale::RADIUS_LG))
        .border_1()
        .border_color(rgba_of(if on { t.rec.with_alpha(60) } else { t.line }))
        .bg(rgba_of(if on {
            t.rec.with_alpha(10)
        } else {
            t.surface_2
        }))
        .text_size(px(13.5))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgba_of(fg))
        .cursor_pointer()
        .when(!on, |d| d.hover(move |s| s.bg(hover)))
        .focus_visible(move |s| s.border_color(focus))
        .child(icon(kind_icon(k), 24., fg))
        .child(k.label())
}

/// The tinted card around a permission or listing problem (the permission callout's tone).
fn problem_card(t: &Theme, inner: gpui::Stateful<gpui::Div>) -> gpui::Div {
    let c = tone_colors(t, Tone::Brass);
    div()
        .flex_none()
        .p(px(14.))
        .rounded(px(scale::RADIUS_LG))
        .border_1()
        .border_color(rgba_of(c.border))
        .bg(rgba_of(c.bg))
        .child(inner)
}

/// One source row of the card (R9): a 34 px icon tile, the label 14/500 over the 12.5 px hint, the
/// switch at the right, and optional extra blocks (camera chips, a problem note) under it.
fn source_row(
    t: &Theme,
    glyph: &'static str,
    label: &str,
    hint: String,
    control: impl IntoElement,
    extras: Vec<gpui::AnyElement>,
    last: bool,
) -> gpui::Div {
    div()
        .px(px(16.))
        .py(px(14.))
        .flex()
        .flex_col()
        .gap(px(10.))
        .when(!last, |d| {
            d.border_b_1().border_color(rgba_of(t.row_divider()))
        })
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(12.))
                .child(
                    div()
                        .size(px(34.))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(scale::RADIUS_MD))
                        .bg(rgba_of(t.icon_tile()))
                        .child(icon(glyph, icon_size::NAV, t.text)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .flex()
                        .flex_col()
                        .gap(px(1.))
                        .child(
                            div()
                                .text_size(px(type_scale::UI))
                                .font_weight(FontWeight::MEDIUM)
                                .child(SharedString::from(label.to_owned())),
                        )
                        .child(
                            div()
                                .text_size(px(12.5))
                                .text_color(rgba_of(t.mist))
                                .child(SharedString::from(hint)),
                        ),
                )
                .child(control),
        )
        .children(extras)
}

/// Debug-only hooks so the tallest states can be looked at without touching the real devices
/// (`CM_DEMO_SHEET_KIND=window|region`, `CM_DEMO_SHEET_DEVICES=<file with a list_devices reply>`).
/// Release builds have neither.
mod demo_overrides {
    use crate::recording::sheet::SheetModel;
    use crate::recording::start::TargetKind;
    use serde_json::Value;

    #[cfg(debug_assertions)]
    pub fn kind(model: &mut SheetModel) {
        match std::env::var("CM_DEMO_SHEET_KIND").as_deref() {
            Ok("display") => model.set_kind(TargetKind::Display),
            Ok("window") => model.set_kind(TargetKind::Window),
            Ok("region") => model.set_kind(TargetKind::Region),
            _ => {}
        }
    }

    #[cfg(debug_assertions)]
    pub fn devices(real: Value) -> Value {
        std::env::var_os("CM_DEMO_SHEET_DEVICES")
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or(real)
    }

    #[cfg(not(debug_assertions))]
    pub fn kind(_model: &mut SheetModel) {
        let _ = TargetKind::Display;
    }

    #[cfg(not(debug_assertions))]
    pub fn devices(real: Value) -> Value {
        real
    }
}
