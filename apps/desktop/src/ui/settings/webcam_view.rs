//! Settings -> Webcam & audio, board R13: the preview card, the Camera and Microphone dropdowns
//! and the live-camera switch. The rules are in `webcam_model` (pure, tested with fakes); this
//! file lists the devices, draws and runs the preview.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use cm_i18n::{tr, tr_ctx};
use gpui::{
    div, img, prelude::*, px, AnyElement, Context, Div, ObjectFit, RenderImage, Role, SharedString,
    Subscription, Task, Window,
};
use serde_json::json;

use super::rows::tile;
use super::webcam_model::{
    fallback_note_text, live_device, preview_bubble, preview_caption, saved_device, setting_value,
    DeviceKind, DeviceOption, Dropdown, Hint, LiveInputs, LiveStep, PreviewBubble,
};
use super::widgets::{caption, settings_row, small_button_with_icon};
use super::SettingsView;
use crate::backend::Backend;
use crate::permissions::Id as PermId;
use crate::recording::geometry::{BubbleShape, BUBBLE_RING};
use crate::recording::preview::{self, FrameSlot, PreviewRunner};
use crate::recording::sheet::Devices;
use crate::recording::start::{KEY_CAMERA_DEVICE, KEY_MIC_DEVICE};
use crate::theme::{scale, type_scale, Theme};
use crate::ui::app::{self, Route};
use crate::ui::components::{icon, icon_size, label_fill};
use crate::ui::paint::rgba_of;
use crate::ui::settings::model::SettingsSection;

/// The wireframe screen of the preview card, px (16:9).
const SCREEN_W: f32 = 640.;
const SCREEN_H: f32 = 360.;

/// What the Webcam & audio page keeps between frames.
pub struct WebcamState {
    devices: Devices,
    /// The device list was asked for since the page was entered.
    listed: bool,
    seen_epoch: u64,
    /// Names seen in this session, for a saved device that is gone.
    names: HashMap<String, String>,
    /// The dropdown that is open.
    open: Option<DeviceKind>,
    /// The "Show live camera" switch. Never saved.
    live_requested: bool,
    live: Option<PreviewRunner>,
    starting: bool,
    /// Bumped by every start and stop, so a start that finishes late can tell it is stale.
    generation: u64,
    frames: Arc<FrameSlot>,
    image: Option<Arc<RenderImage>>,
    seen_seq: u64,
    pull: Option<Task<()>>,
    window_active: bool,
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
            live_requested: false,
            live: None,
            starting: false,
            generation: 0,
            frames: FrameSlot::new(true),
            image: None,
            seen_seq: 0,
            pull: None,
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

    fn webcam_load_devices(&mut self, cx: &mut Context<Self>) {
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

    fn webcam_dropdown(&self, kind: DeviceKind, cx: &Context<Self>) -> Dropdown {
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

    /// The Camera or Microphone row: the closed dropdown, and under it the open list and the
    /// note about a saved device that is not plugged in.
    pub(super) fn device_rows(
        &self,
        t: &Theme,
        kind: DeviceKind,
        last: bool,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let dd = self.webcam_dropdown(kind, cx);
        let open = self.webcam.open == Some(kind);
        let (id, tile_name, hint) = match kind {
            DeviceKind::Camera => (
                "webcam-camera",
                tile::CAMERA,
                tr!("Used for the bubble and the camera track."),
            ),
            DeviceKind::Mic => (
                "webcam-microphone",
                tile::DEVICES,
                tr!("System default input unless you pick one."),
            ),
        };
        let label = kind_label(kind);
        let aria = tr!(
            "{device_kind}: {device}",
            device_kind = label.clone(),
            device = dd.trigger.clone()
        );
        let trigger = self
            .webcam_trigger(t, id, &dd, &aria, open, cx, kind)
            .into_any_element();
        let mut out = vec![settings_row(
            t,
            Some(tile_name),
            SharedString::from(label),
            Some(SharedString::from(hint)),
            Some(trigger),
            last && !open && !dd.fallback_note,
        )
        .into_any_element()];
        if open {
            out.push(self.webcam_list(t, id, kind, &dd, last && !dd.fallback_note, cx));
        }
        if dd.fallback_note {
            out.push(
                div()
                    .px(px(18.))
                    .pb(px(12.))
                    .when(!last, |d| d.border_b_1().border_color(rgba_of(t.border)))
                    .child(
                        div()
                            .flex()
                            .items_start()
                            .gap(px(8.))
                            .text_size(px(type_scale::CAPTION))
                            .text_color(rgba_of(t.text_2))
                            .child(icon("triangle-alert", icon_size::SMALL, t.warning))
                            .child(label_fill(fallback_note_text())),
                    )
                    .into_any_element(),
            );
        }
        out
    }

    #[allow(clippy::too_many_arguments)]
    fn webcam_trigger(
        &self,
        t: &Theme,
        id: &'static str,
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
            .h(px(40.))
            .w(px(300.))
            .max_w_full()
            .px(px(12.))
            .flex()
            .items_center()
            .gap(px(8.))
            .rounded(px(scale::RADIUS_MD))
            .border_1()
            .border_color(rgba_of(t.line))
            .bg(rgba_of(t.field))
            .text_size(px(type_scale::UI))
            .text_color(rgba_of(t.text))
            .cursor_pointer()
            .hover(move |s| s.bg(hover))
            .focus_visible(move |s| s.border_color(focus))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .truncate()
                    .child(SharedString::from(dd.trigger.clone())),
            )
            .children(hint.map(|h| hint_pill(t, h)))
            .child(icon("arrow-down", icon_size::SMALL, t.text_3))
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
        last: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let rows = dd
            .options
            .iter()
            .enumerate()
            .map(|(i, o)| self.webcam_option(t, id, kind, i, o, cx));
        div()
            .px(px(18.))
            .pb(px(14.))
            .when(!last, |d| d.border_b_1().border_color(rgba_of(t.border)))
            .child(
                div()
                    .id(SharedString::from(format!("{id}-list")))
                    .role(Role::ListBox)
                    .aria_label(SharedString::from(kind_label(kind)))
                    .p(px(4.))
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .rounded(px(scale::RADIUS_MD))
                    .border_1()
                    .border_color(rgba_of(t.line))
                    .bg(rgba_of(t.field))
                    .children(rows),
            )
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
            .text_size(px(type_scale::UI))
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

    // ---- the preview card ------------------------------------------------------------------

    /// The card at the top of the page: a wireframe screen with the bubble as set below, the
    /// caption pill and the live-camera switch.
    pub(super) fn preview_card(&self, t: &Theme, cx: &mut Context<Self>) -> AnyElement {
        let snap = self.app.read(cx).backend.settings.snapshot();
        let defaults = crate::recording::start::SheetDefaults::from_settings(&snap);
        let b = defaults.bubble;
        let geometry = preview_bubble(b.size, b.shape, b.mirror, b.position);
        let granted = self.webcam_permission(cx);
        let live_on = self.webcam.live_requested && self.webcam.live.is_some();
        let screen = self.preview_screen(t, &geometry, b.shape, live_on);

        let pill = div()
            .min_h(px(28.))
            .min_w(px(0.))
            .max_w_full()
            .px(px(10.))
            .flex()
            .items_center()
            .gap(px(6.))
            .rounded(px(scale::RADIUS_PILL))
            .bg(rgba_of(t.ground))
            .text_size(px(type_scale::CAPTION))
            .text_color(rgba_of(t.text_2))
            .child(icon("eye", icon_size::PILL, t.text_3))
            .child(label_fill(preview_caption(b.size, b.shape, b.mirror)));
        let control: AnyElement = if granted {
            let (label, glyph) = if self.webcam.live_requested {
                (tr!("Hide live camera"), "video-off")
            } else {
                (tr!("Show live camera"), "video")
            };
            small_button_with_icon(t, "webcam-live-toggle", &label, Some(glyph))
                .aria_toggled(if self.webcam.live_requested {
                    gpui::Toggled::True
                } else {
                    gpui::Toggled::False
                })
                .on_click(cx.listener(|this, _, _, cx| {
                    this.webcam.live_requested = !this.webcam.live_requested;
                    this.webcam_sync(cx);
                    cx.notify();
                }))
                .into_any_element()
        } else {
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(icon("triangle-alert", icon_size::SMALL, t.warning))
                .child(
                    div()
                        .text_size(px(type_scale::CAPTION))
                        .text_color(rgba_of(t.text_2))
                        .child(SharedString::from(tr!("Camera permission is missing"))),
                )
                .child(
                    small_button_with_icon(
                        t,
                        "webcam-open-privacy",
                        &tr!("Open System Settings"),
                        None,
                    )
                    .on_click(|_, _, _| app::open_privacy_pane("Privacy_Camera")),
                )
                .into_any_element()
        };
        div()
            .id("webcam-preview-card")
            .role(Role::Group)
            .aria_label(SharedString::from(tr!("Webcam bubble preview")))
            .p(px(20.))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(12.))
            .rounded(px(scale::RADIUS_LG))
            .border_1()
            .border_color(rgba_of(t.border))
            .bg(rgba_of(t.surface))
            .child(screen)
            .child(
                div()
                    .w(px(SCREEN_W))
                    .max_w_full()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap(px(8.))
                    .child(pill)
                    .child(control),
            )
            .children((!granted).then(|| {
                div().w(px(SCREEN_W)).max_w_full().child(caption(
                    t,
                    tr!("Camera access is off. Turn it on in System Settings; you can also record without the camera."),
                ))
            }))
            .into_any_element()
    }

    fn webcam_permission(&self, cx: &Context<Self>) -> bool {
        self.app.read(cx).perms.state(PermId::Camera).is_granted()
    }

    fn preview_screen(
        &self,
        t: &Theme,
        g: &PreviewBubble,
        shape: BubbleShape,
        live_on: bool,
    ) -> Div {
        let r = g.scaled(f64::from(SCREEN_W));
        let ring = (BUBBLE_RING as f32 * SCREEN_W / 1440.).max(2.);
        let radius = (g.radius as f32 * SCREEN_W / 1440.).max(2.);
        let (bw, bh) = (r.w as f32, r.h as f32);
        let block = |x: f32, y: f32, w: f32, h: f32, r: f32| {
            div()
                .absolute()
                .left(px(x * SCREEN_W))
                .top(px(y * SCREEN_H))
                .w(px(w * SCREEN_W))
                .h(px(h * SCREEN_H))
                .rounded(px(r))
                .bg(rgba_of(t.raised))
        };
        let inner_r = (radius - ring).max(0.);
        let inside: AnyElement = match (&self.webcam.image, live_on) {
            (Some(image), true) => img(image.clone())
                .size_full()
                .rounded(px(inner_r))
                .object_fit(ObjectFit::Cover)
                .into_any_element(),
            _ => silhouette(t, bw - 2. * ring, bh - 2. * ring, g.flip, shape).into_any_element(),
        };
        div()
            .relative()
            .flex_none()
            .w(px(SCREEN_W))
            .h(px(SCREEN_H))
            .rounded(px(scale::RADIUS_MD))
            .overflow_hidden()
            .bg(rgba_of(t.field))
            .child(block(0.05, 0.12, 0.18, 0.76, 6.))
            .child(block(0.27, 0.12, 0.68, 0.12, 6.))
            .child(block(0.27, 0.32, 0.43, 0.56, 6.))
            .child(
                div()
                    .id("webcam-preview-bubble")
                    .absolute()
                    .left(px(r.x as f32))
                    .top(px(r.y as f32))
                    .w(px(bw))
                    .h(px(bh))
                    .rounded(px(radius))
                    .border(px(ring))
                    .border_color(rgba_of(t.rec))
                    .overflow_hidden()
                    .bg(rgba_of(t.surface_2))
                    .child(inside),
            )
    }

    // ---- the live camera -------------------------------------------------------------------

    fn webcam_inputs(&self, cx: &Context<Self>) -> LiveInputs {
        let on_page =
            self.app.read(cx).route == Route::Settings && self.section == SettingsSection::Webcam;
        LiveInputs {
            requested: self.webcam.live_requested,
            permission: self.webcam_permission(cx),
            on_page,
            window_active: self.webcam.window_active,
        }
    }

    /// Starts or stops the live preview to match the page, the window, the permission and the
    /// switch. Called by everything that can change one of them; safe to call any time.
    pub(super) fn webcam_sync(&mut self, cx: &mut Context<Self>) {
        let inputs = self.webcam_inputs(cx);
        if !inputs.on_page {
            // Leaving the page: the list is looked at again next time, a list is closed.
            self.webcam.listed = false;
            self.webcam.open = None;
        }
        self.webcam.live_requested = inputs.requested_after();
        let inputs = LiveInputs {
            requested: self.webcam.live_requested,
            ..inputs
        };
        let running = self.webcam.live.is_some() || self.webcam.starting;
        match inputs.step(running) {
            LiveStep::Keep => {}
            LiveStep::Stop => self.webcam_stop(cx),
            LiveStep::Start => self.webcam_start(cx),
        }
        self.webcam.frames.set_mirror(
            crate::recording::start::SheetDefaults::from_settings(
                &self.app.read(cx).backend.settings.snapshot(),
            )
            .bubble
            .mirror,
        );
    }

    fn webcam_stop(&mut self, cx: &mut Context<Self>) {
        self.webcam.generation += 1;
        self.webcam.starting = false;
        // Dropping the runner stops the frames and, with them, the camera's light.
        self.webcam.live = None;
        self.webcam.pull = None;
        self.webcam.frames.clear();
        if let Some(old) = self.webcam.image.take() {
            cx.drop_image(old, None);
        }
        self.webcam.seen_seq = 0;
        cx.notify();
    }

    fn webcam_start(&mut self, cx: &mut Context<Self>) {
        self.webcam.generation += 1;
        let generation = self.webcam.generation;
        self.webcam.starting = true;
        let snap = self.app.read(cx).backend.settings.snapshot();
        let saved = saved_device(&snap, KEY_CAMERA_DEVICE);
        let device = live_device(&self.webcam.devices.cameras, saved.as_deref());
        let source = self.app.read(cx).rec.read(cx).preview_source;
        let slot = self.webcam.frames.clone();
        slot.clear();
        cx.spawn(async move |this, cx| {
            let s = slot.clone();
            let result = cx
                .background_executor()
                .spawn(async move { preview::start(source, device, s) })
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
                        this.webcam.live_requested = false;
                        let app = this.app.clone();
                        let text = e.to_string();
                        cx.defer(move |cx| {
                            app::push_toast(
                                &app,
                                crate::toast::ToastSpec::new(
                                    crate::toast::ToastKind::Warning,
                                    tr!("The camera preview didn\u{2019}t start"),
                                )
                                .body(text),
                                cx,
                            )
                        });
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Takes the newest frame about 15 times a second while the preview runs.
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
                    if let Some((seq, frame)) = slot.take_newer(this.webcam.seen_seq) {
                        this.webcam.seen_seq = seq;
                        let new = Arc::new(RenderImage::new([image::Frame::new(frame)]));
                        if let Some(old) = this.webcam.image.replace(new) {
                            cx.drop_image(old, None);
                        }
                        cx.notify();
                    }
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

/// The placeholder person in the bubble. `flip` moves the head to the other side so a mirrored
/// preview looks different from an unmirrored one.
fn silhouette(t: &Theme, w: f32, h: f32, flip: bool, shape: BubbleShape) -> Div {
    let head = (w.min(h) * 0.34).max(6.);
    let shoulders_w = w * 0.70;
    let shift = w * 0.07 * if flip { -1. } else { 1. };
    let round = matches!(shape, BubbleShape::Circle);
    div()
        .relative()
        .size_full()
        .overflow_hidden()
        .bg(rgba_of(t.raised))
        .child(
            div()
                .absolute()
                .left(px((w - shoulders_w) / 2. + shift))
                .top(px(h * 0.62))
                .w(px(shoulders_w))
                .h(px(h))
                .rounded_t(px(shoulders_w / 2.))
                .bg(rgba_of(t.line)),
        )
        .child(
            div()
                .absolute()
                .left(px((w - head) / 2. + shift))
                .top(px(h * if round { 0.24 } else { 0.2 }))
                .size(px(head))
                .rounded(px(head / 2.))
                .bg(rgba_of(t.text_3.with_alpha(55))),
        )
}
