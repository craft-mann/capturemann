//! The Long Shot HUD and its side preview (design S2). Two small always-on-top panels that are
//! excluded from screen capture and never take the focus from the window being scrolled
//! (`platform_window::set_no_activate`). They only show what `long_shot_state` said and call the
//! daemon: Pause is `long_shot_state { paused }`, Stop is the daemon's cancel for the call's
//! `request_id` (what was captured is kept).

use gpui::{
    div, img, prelude::*, px, App, Context, DisplayId, Entity, FontWeight, Global, MouseButton,
    ObjectFit, Render, Role, SharedString, Window, WindowHandle,
};
use serde_json::json;

use cm_i18n::tr;

use super::geometry::HUD_W;
use super::LongModel;
use crate::backend::Backend;
use crate::recording::geometry::Rect;
use crate::theme::{type_scale, Theme};
use crate::ui::components::{icon, rec_dot};
use crate::ui::paint::{rgba_of, ui_features, TextExt, UI_FONT};
use crate::ui::recording::{harden, panel_options};

/// The floating preview opened by the HUD's Preview button, if any.
#[derive(Default)]
struct Floating(Option<WindowHandle<LongPreview>>);
impl Global for Floating {}

pub fn floating_handle(cx: &App) -> Option<WindowHandle<LongPreview>> {
    cx.try_global::<Floating>().and_then(|f| f.0)
}

/// Forgets the floating preview after it was closed with the other windows.
pub fn clear_floating(cx: &mut App) {
    cx.set_global(Floating(None));
}

/// Opens the HUD at `rect` (global points) on `display`.
pub fn open_hud(
    model: &Entity<LongModel>,
    display: (DisplayId, Rect),
    rect: Rect,
    cx: &mut App,
) -> Option<WindowHandle<LongHud>> {
    let m = model.clone();
    // Movable: the HUD can be dragged aside for this Long Shot (S2b rule 4).
    let h = cx
        .open_window(
            panel_options(display, rect, false, true),
            move |window, cx| {
                harden(window, false);
                crate::platform_window::set_no_activate(window);
                cx.new(|cx| LongHud::new(m, cx))
            },
        )
        .ok()?;
    crate::surfaces::opened(crate::surfaces::Surface::LongShotHud, h);
    Some(h)
}

/// Opens the side preview at `rect` (global points) on `display`.
pub fn open_preview(
    model: &Entity<LongModel>,
    display: (DisplayId, Rect),
    rect: Rect,
    cx: &mut App,
) -> Option<WindowHandle<LongPreview>> {
    let m = model.clone();
    let h = cx
        .open_window(
            panel_options(display, rect, false, false),
            move |window, cx| {
                harden(window, false);
                crate::platform_window::set_no_activate(window);
                cx.new(|cx| LongPreview::new(m, cx))
            },
        )
        .ok()?;
    crate::surfaces::opened(crate::surfaces::Surface::LongShotPreview, h);
    Some(h)
}

pub struct LongHud {
    model: Entity<LongModel>,
}

impl LongHud {
    fn new(model: Entity<LongModel>, cx: &mut Context<Self>) -> Self {
        cx.observe(&model, |_, _, cx| cx.notify()).detach();
        LongHud { model }
    }

    fn stop(&mut self, cx: &mut Context<Self>) {
        let (backend, id) = {
            let m = self.model.read(cx);
            (m.app.read(cx).backend.clone(), m.request_id.clone())
        };
        if let Some(d) = &backend.daemon {
            d.cancel_delayed_capture(&id);
        }
    }

    fn pause(&mut self, cx: &mut Context<Self>) {
        let (backend, id, paused) = self.model.update(cx, |m, cx| {
            let paused = !m.state.paused;
            // Shown at once; the next poll confirms it.
            m.state.paused = paused;
            cx.notify();
            (m.app.read(cx).backend.clone(), m.request_id.clone(), paused)
        });
        let Some(daemon) = backend.daemon.clone() else {
            return;
        };
        backend.rt.spawn(async move {
            let _ = Backend::call(
                &daemon,
                "long_shot_state",
                json!({ "request_id": id, "paused": paused }),
            )
            .await;
        });
    }

    /// The Preview button of a HUD with no room beside the area: opens or closes the floating
    /// panel over the far side of the display.
    fn toggle_preview(&mut self, cx: &mut Context<Self>) {
        if let Some(h) = cx.try_global::<Floating>().and_then(|f| f.0) {
            crate::surfaces::close(cx, h);
            cx.set_global(Floating(None));
            return;
        }
        let model = self.model.clone();
        let (display, rect) = model.read(cx).floating;
        let opened = open_preview(&model, (display, rect), rect, cx);
        cx.set_global(Floating(opened));
    }
}

/// One half of the direction segment: an icon in a 36 x 28 cell, `raised` when it is the
/// direction of this Long Shot (design S2).
fn seg(t: &Theme, glyph: &'static str, label: String, active: bool) -> gpui::Stateful<gpui::Div> {
    div()
        .id(glyph)
        .role(Role::Image)
        .w(px(36.))
        .h(px(28.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(8.))
        .aria_label(SharedString::from(label))
        .when(active, |d| d.bg(rgba_of(t.raised)))
        .child(icon(glyph, 16., if active { t.text } else { t.text_3 }))
}

fn hud_button(
    t: &Theme,
    id: &'static str,
    glyph: &'static str,
    label: String,
) -> gpui::Stateful<gpui::Div> {
    let hover = rgba_of(t.raised);
    div()
        .id(id)
        .role(Role::Button)
        .aria_label(SharedString::from(label.clone()))
        .size(px(40.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(10.))
        .cursor_pointer()
        .hover(move |s| s.bg(hover))
        .tooltip(crate::ui::tooltip::tip(label, Vec::new()))
        .child(icon(glyph, 18., t.text_2))
        // A press on a button is not the start of a window drag (Windows declares the bar as a
        // drag region; `occlude` keeps it off the button).
        .occlude()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
}

impl Render for LongHud {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = Theme::overlay();
        let (state, preview_button) = {
            let m = self.model.read(cx);
            (
                (
                    m.state.frames_text(),
                    m.state.length_text(),
                    m.state.paused,
                    m.state.is_auto(),
                    m.state.is_horizontal(),
                ),
                m.preview_button,
            )
        };
        let (frames, length, paused, auto, horizontal) = state;
        let mut bar = div()
            .id("long-hud")
            .role(Role::Toolbar)
            .aria_label(tr!("Long Shot"))
            .w(px(HUD_W as f32))
            .h(px(56.))
            .flex()
            .items_center()
            .gap(px(6.))
            .px(px(6.))
            .rounded(px(16.))
            .border_1()
            .border_color(rgba_of(t.line))
            .bg(rgba_of(t.surface))
            .font_family(UI_FONT)
            .font_features(ui_features())
            .cursor_grab()
            .map(crate::platform_window::drag_grip)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _, window, _| window.start_window_move()),
            );
        bar = bar
            .child(
                div()
                    .id("long-hud-stop")
                    .role(Role::Button)
                    .aria_label(tr!("Stop"))
                    .size(px(44.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(12.))
                    .bg(rgba_of(t.rec))
                    .cursor_pointer()
                    .child(div().size(px(14.)).rounded(px(3.)).bg(rgba_of(t.ink)))
                    .occlude()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(|this, _, _, cx| this.stop(cx))),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .px(px(6.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(rec_dot(&t, 8.))
                            .child(
                                div()
                                    .mono()
                                    .text_size(px(15.))
                                    .text_color(rgba_of(t.text))
                                    .child(SharedString::from(frames)),
                            ),
                    )
                    .child(
                        div()
                            .mono()
                            .text_size(px(11.))
                            .text_color(rgba_of(t.mist))
                            .child(SharedString::from(length)),
                    ),
            )
            .child(div().w(px(1.)).h(px(28.)).bg(rgba_of(t.line)))
            // The direction is chosen before Start (the stitcher works along one axis), so the
            // control is shown as a label of what this Long Shot does.
            .child(
                div()
                    .flex()
                    .items_center()
                    .p(px(2.))
                    .gap(px(2.))
                    .rounded(px(10.))
                    .border_1()
                    .border_color(rgba_of(t.line))
                    .child(seg(&t, "arrow-up-down", tr!("Vertical"), !horizontal))
                    .child(seg(&t, "arrow-left-right", tr!("Horizontal"), horizontal)),
            )
            .child(
                hud_button(
                    &t,
                    "long-hud-pause",
                    if paused { "rec-play" } else { "rec-pause" },
                    if paused { tr!("Resume") } else { tr!("Pause") },
                )
                .on_click(cx.listener(|this, _, _, cx| this.pause(cx))),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .px(px(6.))
                    .text_size(px(type_scale::CAPTION))
                    .text_color(rgba_of(t.text_3))
                    .child(icon("chevrons-down", 14., t.text_3))
                    .child(SharedString::from(if auto {
                        tr!("Auto")
                    } else {
                        tr!("Hand")
                    })),
            );
        if preview_button {
            bar = bar.child(
                hud_button(&t, "long-hud-preview", "image", tr!("Preview"))
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_preview(cx))),
            );
        }
        bar
    }
}

pub struct LongPreview {
    model: Entity<LongModel>,
}

impl LongPreview {
    fn new(model: Entity<LongModel>, cx: &mut Context<Self>) -> Self {
        cx.observe(&model, |_, _, cx| cx.notify()).detach();
        LongPreview { model }
    }
}

impl Render for LongPreview {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = Theme::overlay();
        let st = &self.model.read(cx).state;
        let panel_w = f32::from(window.viewport_size().width);
        let inner_w = (panel_w - 24.0).max(40.0);
        let size_text = st.length_text();
        let mut stage = div()
            .flex_1()
            .min_h(px(0.))
            .overflow_hidden()
            .flex()
            .flex_col()
            // The newest rows are at the bottom: the picture grows upwards out of the panel.
            .justify_end()
            .rounded(px(8.))
            .bg(rgba_of(t.ink));
        if let Some(image) = st.preview.clone() {
            let (pw, ph) = st.preview_px;
            let ratio = if pw == 0 { 1.0 } else { ph as f32 / pw as f32 };
            stage = stage.child(
                img(image)
                    .w(px(inner_w.min(pw as f32)))
                    .h(px(inner_w.min(pw as f32) * ratio))
                    .flex_none()
                    .object_fit(ObjectFit::Fill),
            );
        }
        let tags = st.band_tags();
        let progress = st.fraction();
        let status = st.status_text();
        div()
            .id("long-preview")
            .size_full()
            .flex()
            .flex_col()
            .gap(px(8.))
            .p(px(12.))
            .rounded(px(16.))
            .border_1()
            .border_color(rgba_of(t.line))
            .bg(rgba_of(t.surface))
            .font_family(UI_FONT)
            .font_features(ui_features())
            .child(
                div()
                    .mono()
                    .text_size(px(type_scale::CAPTION))
                    .text_color(rgba_of(t.text_2))
                    .child(SharedString::from(size_text)),
            )
            .child(stage)
            .when(!tags.is_empty(), |d| {
                d.child(
                    div()
                        .flex()
                        .gap(px(6.))
                        .children(tags.into_iter().map(|tag| {
                            div()
                                .px(px(8.))
                                .py(px(2.))
                                .rounded(px(6.))
                                .border_1()
                                .border_color(rgba_of(t.line))
                                .text_size(px(11.))
                                .text_color(rgba_of(t.text_3))
                                .child(SharedString::from(tag))
                        })),
                )
            })
            .child(
                div()
                    .text_size(px(type_scale::CAPTION))
                    .font_weight(FontWeight::NORMAL)
                    .text_color(rgba_of(t.text_3))
                    .child(SharedString::from(status)),
            )
            .child(
                div()
                    .h(px(4.))
                    .w_full()
                    .rounded(px(2.))
                    .bg(rgba_of(t.raised))
                    .child(
                        div()
                            .h_full()
                            .w(gpui::relative(progress))
                            .rounded(px(2.))
                            .bg(rgba_of(t.rec)),
                    ),
            )
    }
}
