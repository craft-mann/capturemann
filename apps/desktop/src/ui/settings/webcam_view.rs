//! Settings -> Webcam & audio, boards R13 and R13b: the Camera check card and the Camera and
//! Microphone dropdowns. The rules are in `webcam_model` (pure, tested with fakes); this file
//! lists the devices, draws the card and runs the preview. The camera is off until the user
//! presses "Turn on camera".

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use cm_i18n::{tr, tr_ctx};
use gpui::{
    div, img, prelude::*, px, AnyElement, Context, Div, ObjectFit, RenderImage, Role, SharedString,
    Subscription, Task, Window,
};
use serde_json::json;

use super::check_widgets::{
    action_button, check_card, chip, control_button, footnote, state_block, Block, Tone,
};
use super::webcam_model::{
    camera_card, classify_start_error, fallback_note_text, live_device, readout, saved_device,
    setting_value, CamFailure, CameraCard, CheckSession, DeviceKind, DeviceOption, Dropdown,
    Effect, Facts, FpsMeter, Hint, StartError,
};
use super::SettingsView;
use crate::backend::Backend;
use crate::permissions::Id as PermId;
use crate::recording::preview::{self, FrameSlot, PreviewRunner};
use crate::recording::sheet::Devices;
use crate::recording::start::{KEY_CAMERA_DEVICE, KEY_MIC_DEVICE};
use crate::theme::{scale, type_scale, Theme};
use crate::ui::app::{self, Route};
use crate::ui::components::{icon, icon_size, label_fill};
use crate::ui::paint::{rgba_of, TextExt};
use crate::ui::settings::model::SettingsSection;

/// Frames a second the check asks the camera for, so the rate it shows is the camera's.
const CHECK_FPS: u32 = 30;

/// What the Webcam & audio page keeps between frames.
pub struct WebcamState {
    devices: Devices,
    /// The device list was asked for since the page was entered.
    listed: bool,
    seen_epoch: u64,
    /// Names seen in this session, for a saved device that is gone.
    names: HashMap<String, String>,
    /// The dropdown that is open.
    pub(super) open: Option<DeviceKind>,
    /// The camera check: off until "Turn on camera". Never saved.
    pub(super) cam: CheckSession,
    live: Option<PreviewRunner>,
    starting: bool,
    /// Bumped by every start and stop, so a start that finishes late can tell it is stale.
    generation: u64,
    /// The device the running preview opened (`None` inside: the system default).
    running_device: Option<Option<String>>,
    /// The device the dropdown asks for now, to notice a change.
    last_desired: Option<Option<String>>,
    failure: Option<CamFailure>,
    frames: Arc<FrameSlot>,
    image: Option<Arc<RenderImage>>,
    seen_seq: u64,
    pull: Option<Task<()>>,
    size: Option<(u32, u32)>,
    fps: FpsMeter,
    pub(super) clock: Instant,
    pub(super) window_active: bool,
    activation: Option<Subscription>,
}

impl WebcamState {
    pub fn new() -> WebcamState {
        WebcamState {
            devices: Devices::default(),
            listed: false,
            seen_epoch: 0,
            names: HashMap::new(),
            open: None,
            cam: CheckSession::default(),
            live: None,
            starting: false,
            generation: 0,
            running_device: None,
            last_desired: None,
            failure: None,
            frames: FrameSlot::new(true),
            image: None,
            seen_seq: 0,
            pull: None,
            size: None,
            fps: FpsMeter::default(),
            clock: Instant::now(),
            window_active: true,
            activation: None,
        }
    }
}

impl Default for WebcamState {
    fn default() -> Self {
        WebcamState::new()
    }
}

fn kind_label(kind: DeviceKind) -> String {
    match kind {
        DeviceKind::Camera => tr_ctx!("settings row", "Camera"),
        DeviceKind::Mic => tr_ctx!("settings row", "Microphone"),
    }
}

impl SettingsView {
    // ---- facts -----------------------------------------------------------------------------

    /// What decides whether the camera or the microphone of the check may be on.
    pub(super) fn check_facts(&self, perm: PermId, cx: &Context<Self>) -> Facts {
        let m = self.app.read(cx);
        Facts {
            permission: m.perms.state(perm).is_granted(),
            on_page: m.route == Route::Settings && self.section == SettingsSection::Webcam,
            window_active: self.webcam.window_active,
            recording: m.rec.read(cx).hud.phase.is_active(),
        }
    }

    /// Settings -> Permissions. Nothing is asked on the way: the request happens there.
    pub(super) fn open_permissions(&mut self, cx: &mut Context<Self>) {
        let app = self.app.clone();
        app::open_permissions_page(&app, cx);
    }

    // ---- devices ---------------------------------------------------------------------------

    /// Lists the devices the first time the page is drawn, and again when a camera was plugged in
    /// or out (the Record sheet's `camera_list_epoch`).
    pub(super) fn webcam_ensure_devices(&mut self, cx: &mut Context<Self>) {
        let epoch = self.app.read(cx).rec.read(cx).camera_list_epoch;
        if !self.webcam.listed || epoch != self.webcam.seen_epoch {
            self.webcam.listed = true;
            self.webcam.seen_epoch = epoch;
            self.webcam_load_devices(cx);
        }
    }

    pub(super) fn webcam_load_devices(&mut self, cx: &mut Context<Self>) {
        let backend = self.app.read(cx).backend.clone();
        let Some(daemon) = backend.daemon.clone() else {
            return;
        };
        let rt = backend.rt.clone();
        cx.spawn(async move |this, cx| {
            let reply = rt
                .spawn(async move { Backend::call(&daemon, "list_devices", json!({})).await })
                .await;
            this.update(cx, |this, cx| {
                if let Ok(Ok(v)) = reply {
                    let devices = Devices::parse(&v);
                    for d in devices.cameras.iter().chain(&devices.mics) {
                        this.webcam.names.insert(d.id.clone(), d.name.clone());
                    }
                    this.webcam.devices = devices;
                    this.webcam_sync(cx);
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn webcam_dropdown(&self, kind: DeviceKind, cx: &Context<Self>) -> Dropdown {
        let snap = self.app.read(cx).backend.settings.snapshot();
        let (key, list) = match kind {
            DeviceKind::Camera => (KEY_CAMERA_DEVICE, &self.webcam.devices.cameras),
            DeviceKind::Mic => (KEY_MIC_DEVICE, &self.webcam.devices.mics),
        };
        let saved = saved_device(&snap, key);
        let known = saved
            .as_deref()
            .and_then(|s| self.webcam.names.get(s))
            .map(String::as_str);
        Dropdown::build(
            kind,
            list,
            self.webcam.devices.loaded,
            saved.as_deref(),
            known,
        )
    }

    /// The device a check opens now: the saved one when it is connected, else the system default
    /// (`None`). Before the list is known the saved id is used as it is.
    pub(super) fn desired_device(&self, kind: DeviceKind, cx: &Context<Self>) -> Option<String> {
        let snap = self.app.read(cx).backend.settings.snapshot();
        let (key, list) = match kind {
            DeviceKind::Camera => (KEY_CAMERA_DEVICE, &self.webcam.devices.cameras),
            DeviceKind::Mic => (KEY_MIC_DEVICE, &self.webcam.devices.mics),
        };
        let saved = saved_device(&snap, key);
        if self.webcam.devices.loaded {
            live_device(list, saved.as_deref())
        } else {
            saved
        }
    }

    /// The 34 px dropdown of a check card, the open list under it and the note about a saved
    /// device that is not plugged in. `trigger` goes in the controls row; `below` under it.
    pub(super) fn device_dropdown(
        &self,
        t: &Theme,
        kind: DeviceKind,
        cx: &mut Context<Self>,
    ) -> (AnyElement, Vec<AnyElement>) {
        let dd = self.webcam_dropdown(kind, cx);
        let open = self.webcam.open == Some(kind);
        let (id, glyph) = match kind {
            DeviceKind::Camera => ("webcam-camera", Some("camera")),
            // The microphone card is narrow: its dropdown has no leading icon (as in the board).
            DeviceKind::Mic => ("webcam-microphone", None),
        };
        let aria = tr!(
            "{device_kind}: {device}",
            device_kind = kind_label(kind),
            device = dd.trigger.clone()
        );
        let trigger = self
            .webcam_trigger(t, id, glyph, &dd, &aria, open, cx, kind)
            .into_any_element();
        let mut below = Vec::new();
        if open {
            below.push(self.webcam_list(t, id, kind, &dd, cx));
        }
        if dd.fallback_note {
            below.push(
                div()
                    .w_full()
                    .flex()
                    .items_start()
                    .gap(px(8.))
                    .text_size(px(type_scale::CAPTION))
                    .text_color(rgba_of(t.text_3))
                    .child(icon("triangle-alert", icon_size::SMALL, t.warning))
                    .child(label_fill(fallback_note_text()))
                    .into_any_element(),
            );
        }
        (trigger, below)
    }

    #[allow(clippy::too_many_arguments)]
    fn webcam_trigger(
        &self,
        t: &Theme,
        id: &'static str,
        glyph: Option<&'static str>,
        dd: &Dropdown,
        aria: &str,
        open: bool,
        cx: &mut Context<Self>,
        kind: DeviceKind,
    ) -> gpui::Stateful<Div> {
        let (hover, focus) = (rgba_of(t.raised), rgba_of(t.text));
        let hint = dd.trigger_hint;
        div()
            .id(id)
            .role(Role::Button)
            .aria_label(SharedString::from(aria.to_owned()))
            .aria_expanded(open)
            .focusable()
            .tab_stop(true)
            .h(px(34.))
            .flex_1()
            .flex_basis(px(200.))
            .min_w(px(0.))
            .px(px(12.))
            .flex()
            .items_center()
            .gap(px(8.))
            .rounded(px(9.))
            .border_1()
            .border_color(rgba_of(t.line))
            .bg(rgba_of(t.field))
            .text_size(px(13.))
            .text_color(rgba_of(t.text))
            .cursor_pointer()
            .hover(move |s| s.bg(hover))
            .focus_visible(move |s| s.border_color(focus))
            .children(glyph.map(|g| icon(g, icon_size::SMALL, t.mist)))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .truncate()
                    .child(SharedString::from(dd.trigger.clone())),
            )
            .children(hint.map(|h| hint_pill(t, h)))
            .child(icon("arrow-down", 13., t.mist))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.webcam.open = if this.webcam.open == Some(kind) {
                    None
                } else {
                    // Devices come and go without an event (a microphone): look again.
                    this.webcam_load_devices(cx);
                    Some(kind)
                };
                cx.notify();
            }))
    }

    fn webcam_list(
        &self,
        t: &Theme,
        id: &'static str,
        kind: DeviceKind,
        dd: &Dropdown,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let rows = dd
            .options
            .iter()
            .enumerate()
            .map(|(i, o)| self.webcam_option(t, id, kind, i, o, cx));
        div()
            .id(SharedString::from(format!("{id}-list")))
            .role(Role::ListBox)
            .aria_label(SharedString::from(kind_label(kind)))
            .w_full()
            .p(px(4.))
            .flex()
            .flex_col()
            .gap(px(2.))
            .rounded(px(scale::RADIUS_MD))
            .border_1()
            .border_color(rgba_of(t.line))
            .bg(rgba_of(t.field))
            .children(rows)
            .into_any_element()
    }

    fn webcam_option(
        &self,
        t: &Theme,
        id: &'static str,
        kind: DeviceKind,
        index: usize,
        o: &DeviceOption,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (hover, focus) = (rgba_of(t.raised), rgba_of(t.text));
        let key = match kind {
            DeviceKind::Camera => KEY_CAMERA_DEVICE,
            DeviceKind::Mic => KEY_MIC_DEVICE,
        };
        let value = setting_value(&o.value);
        let app = self.app.clone();
        let name = o.name.clone();
        let aria = match o.hint {
            Some(h) => format!("{name}, {}", h.label()),
            None => name.clone(),
        };
        div()
            .id(SharedString::from(format!("{id}-option-{index}")))
            .role(Role::ListBoxOption)
            .aria_label(SharedString::from(aria))
            .aria_selected(o.chosen)
            .focusable()
            .tab_stop(true)
            .min_h(px(36.))
            .px(px(10.))
            .flex()
            .items_center()
            .gap(px(8.))
            .rounded(px(scale::RADIUS_SM))
            .border_1()
            .border_color(gpui::transparent_black())
            .text_size(px(13.))
            .text_color(rgba_of(if o.chosen { t.text } else { t.text_2 }))
            .when(o.chosen, |d| d.bg(rgba_of(t.raised)))
            .cursor_pointer()
            .hover(move |s| s.bg(hover))
            .focus_visible(move |s| s.border_color(focus))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .truncate()
                    .child(SharedString::from(name)),
            )
            .children(o.hint.map(|h| hint_pill(t, h)))
            .children(o.chosen.then(|| icon("check", icon_size::SMALL, t.text)))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.webcam.open = None;
                app::set_setting(&app, key, value.clone(), cx);
                cx.notify();
            }))
            .into_any_element()
    }

    // ---- the Camera check card -------------------------------------------------------------

    fn camera_name(&self, cx: &Context<Self>) -> String {
        self.webcam_dropdown(DeviceKind::Camera, cx).trigger
    }

    /// Camera check (board R13b, card 1): the live feed, or the block that says why not.
    pub(super) fn camera_check_card(
        &self,
        t: &Theme,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let f = self.check_facts(PermId::Camera, cx);
        let snap = self.app.read(cx).backend.settings.snapshot();
        let mirror = crate::recording::start::SheetDefaults::from_settings(&snap)
            .bubble
            .mirror;
        let card = camera_card(
            &self.webcam.cam,
            f,
            self.webcam.failure.as_ref(),
            self.webcam.image.is_some(),
        );
        let (trigger, below) = self.device_dropdown(t, DeviceKind::Camera, cx);

        // Header: icon, title, the chip and the mono readout.
        let live = card == CameraCard::Live;
        let header = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(10.))
            .px(px(16.))
            .py(px(14.))
            .child(icon("camera", 16., t.text))
            .child(
                div()
                    .mr_auto()
                    .text_size(px(15.))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(SharedString::from(tr_ctx!("check card", "Camera check"))),
            )
            .children(match card {
                CameraCard::Live => Some(chip(
                    t,
                    Tone::Success,
                    24.,
                    tr_ctx!("camera status", "Live"),
                    None,
                    true,
                )),
                CameraCard::Starting => Some(chip(
                    t,
                    Tone::Info,
                    24.,
                    tr_ctx!("camera status", "Starting\u{2026}"),
                    None,
                    true,
                )),
                _ => None,
            })
            .children(
                live.then(|| {
                    readout(self.webcam.size, self.webcam.fps.fps()).map(|r| {
                        div()
                            .mono()
                            .text_size(px(11.5))
                            .text_color(rgba_of(t.mist))
                            .child(SharedString::from(r))
                    })
                })
                .flatten(),
            );

        // The feed, or the block in its place (same 16:9 box).
        let feed = div()
            .mx(px(16.))
            .relative()
            .aspect_ratio(16. / 9.)
            .rounded(px(12.))
            .overflow_hidden();
        let framed = |d: Div| {
            d.border_1()
                .border_color(rgba_of(t.border))
                .bg(rgba_of(t.field))
        };
        let feed: AnyElement = match (&card, &self.webcam.image) {
            (CameraCard::Live, Some(image)) => framed(feed)
                .id("webcam-live-feed")
                .role(Role::Image)
                .aria_label(SharedString::from(tr!("Live camera")))
                .child(img(image.clone()).size_full().object_fit(ObjectFit::Cover))
                .children(mirror.then(|| mirrored_tag(t)))
                .into_any_element(),
            (CameraCard::Starting, _) => framed(feed)
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(13.))
                .text_color(rgba_of(t.text_3))
                .child(SharedString::from(tr!("Starting the camera\u{2026}")))
                .into_any_element(),
            _ => feed
                .flex()
                .items_center()
                .justify_center()
                .p(px(12.))
                .child(self.camera_block(t, &card, cx))
                .into_any_element(),
        };

        let turn_off = matches!(card, CameraCard::Live | CameraCard::Starting).then(|| {
            control_button(
                t,
                "webcam-turn-off",
                "video-off",
                &tr!("Turn off camera"),
                cx.listener(|this, _, _, cx| this.camera_turn_off(cx)),
            )
            .into_any_element()
        });

        check_card(t, "camera-check", &tr_ctx!("check card", "Camera check"))
            .child(header)
            .child(feed)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .px(px(16.))
                    .pt(px(12.))
                    .pb(px(14.))
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
                    .child(footnote(
                        t,
                        tr!("Preview only. Nothing is recorded, and the camera turns off when you leave this page."),
                    )),
            )
    }

    /// The block of a camera state that is not the live feed (spec, section 4).
    fn camera_block(&self, t: &Theme, card: &CameraCard, cx: &mut Context<Self>) -> AnyElement {
        let on =
            |label: String, glyph: &'static str, id: &'static str, press: Press| -> AnyElement {
                action_button(
                    t,
                    id,
                    glyph,
                    &label,
                    cx.listener(move |this, _, _, cx| match press {
                        Press::TurnOn => this.camera_turn_on(cx),
                        Press::Allow => this.camera_allow_access(cx),
                    }),
                )
                .into_any_element()
            };
        let (block, action) = match card {
            CameraCard::Off => (
                Block {
                    tone: Tone::Info,
                    glyph: "camera",
                    title: tr!("Camera is off"),
                    text: tr!("Nothing is shown until you ask. The light on your camera stays off."),
                },
                Some(on(tr!("Turn on camera"), "video", "webcam-turn-on", Press::TurnOn)),
            ),
            CameraCard::NoPermission => (
                Block {
                    tone: Tone::Warning,
                    glyph: "shield-check",
                    title: tr!("Camera access needed"),
                    text: tr!("CaptureMann isn\u{2019}t allowed to use the camera yet."),
                },
                Some(on(
                    tr!("Allow access"),
                    "external-link",
                    "webcam-allow-access",
                    Press::Allow,
                )),
            ),
            CameraCard::InUse => (
                Block {
                    tone: Tone::Warning,
                    glyph: "camera-off",
                    title: tr!("Camera is in use"),
                    text: tr!(
                        "Another app is using {camera}. Close it, or pick another camera.",
                        camera = self.camera_name(cx)
                    ),
                },
                Some(on(tr!("Try again"), "refresh-cw", "webcam-try-again", Press::TurnOn)),
            ),
            CameraCard::NotFound => (
                Block {
                    tone: Tone::Error,
                    glyph: "usb",
                    title: tr!("No camera found"),
                    text: tr!("Plug in a camera, or record without the bubble."),
                },
                Some(on(tr!("Look again"), "refresh-cw", "webcam-look-again", Press::TurnOn)),
            ),
            CameraCard::Other(_) => (
                Block {
                    tone: Tone::Warning,
                    glyph: "camera-off",
                    title: tr!("The camera didn\u{2019}t start"),
                    text: tr!("CaptureMann couldn\u{2019}t open the camera. Try again, or pick another camera."),
                },
                Some(on(tr!("Try again"), "refresh-cw", "webcam-try-again", Press::TurnOn)),
            ),
            CameraCard::Paused => (
                Block {
                    tone: Tone::Info,
                    glyph: "cm-record",
                    title: tr!("Recording in progress"),
                    text: tr!("Recording in progress, the check is paused."),
                },
                None,
            ),
            // Drawn by the caller.
            CameraCard::Live | CameraCard::Starting => return div().into_any_element(),
        };
        // The card around it is the frame: no second border or fill, and the full width.
        state_block(t, "webcam-state", block, action)
            .w_full()
            .border_color(gpui::transparent_black())
            .bg(gpui::transparent_black())
            .into_any_element()
    }

    // ---- the buttons -----------------------------------------------------------------------

    /// Turn on camera, Try again and Look again.
    fn camera_turn_on(&mut self, cx: &mut Context<Self>) {
        let f = self.check_facts(PermId::Camera, cx);
        if self.webcam.failure.take().is_some() {
            // Look again: a camera may have been plugged in.
            self.webcam_load_devices(cx);
        }
        self.webcam.cam.press_start(f);
        self.webcam_sync(cx);
        cx.notify();
    }

    fn camera_turn_off(&mut self, cx: &mut Context<Self>) {
        self.webcam.cam.press_stop();
        self.webcam.failure = None;
        self.webcam_sync(cx);
        cx.notify();
    }

    fn camera_allow_access(&mut self, cx: &mut Context<Self>) {
        if self.webcam.cam.press_allow_access() == Effect::OpenPermissions {
            self.open_permissions(cx);
        }
    }

    // ---- the live camera -------------------------------------------------------------------

    /// Starts or stops the live preview to match the page, the window, the permission, the
    /// recording and the user's wish (and the microphone check with it). Called by everything
    /// that can change one of them; safe to call any time.
    pub(super) fn webcam_sync(&mut self, cx: &mut Context<Self>) {
        let f = self.check_facts(PermId::Camera, cx);
        if !f.on_page {
            // Leaving the page: the list is looked at again next time, a list is closed.
            self.webcam.listed = false;
            self.webcam.open = None;
            self.webcam.failure = None;
        }
        let desired = self.desired_device(DeviceKind::Camera, cx);
        if self.webcam.last_desired.as_ref() != Some(&desired) {
            // Another camera was picked: the feed switches, and an old failure is forgotten.
            self.webcam.last_desired = Some(desired.clone());
            self.webcam.failure = None;
        }
        let mut running = self.webcam.live.is_some() || self.webcam.starting;
        // A failure (the camera was unplugged under the preview) ends it.
        if running && self.webcam.failure.is_some() {
            self.webcam_stop(cx);
            running = false;
        }
        if running && self.webcam.running_device.as_ref() != Some(&desired) {
            self.webcam_stop(cx);
            running = false;
        }
        match self.webcam.cam.sync(f, running) {
            Some(Effect::Start) => self.webcam_start(desired, cx),
            Some(Effect::Stop) => self.webcam_stop(cx),
            _ => {}
        }
        self.webcam.frames.set_mirror(
            crate::recording::start::SheetDefaults::from_settings(
                &self.app.read(cx).backend.settings.snapshot(),
            )
            .bubble
            .mirror,
        );
        self.audio_sync(cx);
    }

    fn webcam_stop(&mut self, cx: &mut Context<Self>) {
        self.webcam.generation += 1;
        self.webcam.starting = false;
        // Dropping the runner stops the frames and, with them, the camera's light.
        self.webcam.live = None;
        self.webcam.pull = None;
        self.webcam.running_device = None;
        self.webcam.frames.clear();
        self.webcam.size = None;
        self.webcam.fps.reset();
        if let Some(old) = self.webcam.image.take() {
            cx.drop_image(old, None);
        }
        self.webcam.seen_seq = 0;
        cx.notify();
    }

    fn webcam_start(&mut self, device: Option<String>, cx: &mut Context<Self>) {
        self.webcam.generation += 1;
        let generation = self.webcam.generation;
        self.webcam.starting = true;
        self.webcam.running_device = Some(device.clone());
        self.webcam.fps.reset();
        let source = self.app.read(cx).rec.read(cx).preview_source;
        let slot = self.webcam.frames.clone();
        slot.clear();
        slot.set_error(None);
        cx.spawn(async move |this, cx| {
            let s = slot.clone();
            let result = cx
                .background_executor()
                .spawn(async move { preview::start_at(source, device, s, CHECK_FPS) })
                .await;
            this.update(cx, |this, cx| {
                if this.webcam.generation != generation {
                    // Stopped while the camera was starting: let it go at once.
                    drop(result);
                    return;
                }
                this.webcam.starting = false;
                match result {
                    Ok(run) => {
                        this.webcam.live = Some(run);
                        this.webcam_start_pulling(cx);
                    }
                    Err(e) => {
                        this.webcam.running_device = None;
                        match classify_start_error(e.code(), &e.to_string()) {
                            StartError::NoPermission => this.webcam.cam.start_failed(true),
                            StartError::Failure(why) => {
                                this.webcam.cam.start_failed(false);
                                this.webcam.failure = Some(why);
                            }
                        }
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Takes the newest frame about 15 times a second while the preview runs, measures the frame
    /// rate and watches for a camera that went away.
    fn webcam_start_pulling(&mut self, cx: &mut Context<Self>) {
        self.webcam.pull = Some(cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(Duration::from_millis(66))
                .await;
            let alive = this
                .update(cx, |this, cx| {
                    if this.webcam.live.is_none() {
                        return false;
                    }
                    let slot = this.webcam.frames.clone();
                    let now = this.webcam.clock.elapsed().as_millis() as u64;
                    this.webcam.fps.update(slot.pushed(), now);
                    this.webcam.size = slot.source_size();
                    if slot.error().is_some() && this.webcam.failure.is_none() {
                        // Unplugged (or taken away) mid-preview: "No camera found", no crash.
                        // The next sync closes the preview.
                        this.webcam.failure = Some(CamFailure::NotFound);
                        this.webcam.cam.start_failed(false);
                        cx.notify();
                        return true;
                    }
                    if let Some((seq, frame)) = slot.take_newer(this.webcam.seen_seq) {
                        this.webcam.seen_seq = seq;
                        let new = Arc::new(RenderImage::new([image::Frame::new(frame)]));
                        if let Some(old) = this.webcam.image.replace(new) {
                            cx.drop_image(old, None);
                        }
                    }
                    cx.notify();
                    true
                })
                .unwrap_or(false);
            if !alive {
                break;
            }
        }));
    }

    /// Once per view: follows the window coming to the front and going away.
    pub(super) fn webcam_watch_window(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.webcam.activation.is_some() {
            return;
        }
        self.webcam.window_active = window.is_window_active();
        self.webcam.activation = Some(cx.observe_window_activation(window, |this, window, cx| {
            this.webcam.window_active = window.is_window_active();
            this.webcam_sync(cx);
            cx.notify();
        }));
    }
}

#[derive(Clone, Copy)]
enum Press {
    TurnOn,
    Allow,
}

/// "Mirrored, like the bubble", at the bottom left of the feed.
fn mirrored_tag(t: &Theme) -> Div {
    div()
        .absolute()
        .left(px(10.))
        .bottom(px(10.))
        .max_w(px(300.))
        .min_h(px(24.))
        .px(px(9.))
        .flex()
        .items_center()
        .gap(px(6.))
        .rounded(px(7.))
        .bg(gpui::rgba(0x0A0A0BB8))
        .text_size(px(11.5))
        .text_color(rgba_of(t.text_2))
        .child(icon("flip-horizontal-2", 12., t.text_2))
        .child(label_fill(tr!("Mirrored, like the bubble")))
}

fn hint_pill(t: &Theme, h: Hint) -> Div {
    let (fg, border) = match h {
        Hint::Default => (t.text_3, t.line),
        Hint::NotConnected => (t.warning, t.warning.with_alpha(40)),
    };
    div()
        .flex_none()
        .h(px(20.))
        .px(px(8.))
        .flex()
        .items_center()
        .rounded(px(scale::RADIUS_PILL))
        .border_1()
        .border_color(rgba_of(border))
        .text_size(px(11.))
        .text_color(rgba_of(fg))
        .child(SharedString::from(h.label()))
}
