//! The annotation editor (DESIGN 5.5, Studio mode): toolbar, tool palette, canvas on a
//! true-black letterbox, inspector and status line. This file only draws `EditorDoc` and
//! forwards events; the behavior lives in `crate::editor` (unit tested).
//!
//! Accessibility (NFR-13): every control is a focusable element with a role and an accessible
//! name; the canvas is keyboard-operable (tool keys, `[`/`]` to pick an annotation, arrows to
//! move, Alt+arrows to resize, Enter to place, Delete, Cmd/Ctrl+Z).

use cm_core::{Color as OpColor, OpKind, Pt, Rc};
use cm_i18n::{tr, tr_ctx, trn};
use gpui::{
    anchored, canvas, deferred, div, img, point, prelude::*, px, size, AccessibleAction,
    AnyElement, App, BorderStyle, Bounds, ClickEvent, ContentMask, Context, CursorStyle, Div,
    ElementId, Entity, FocusHandle, FontWeight, Hsla, KeyDownEvent, KeyUpEvent, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, ObjectFit, PathBuilder, PinchEvent, Pixels,
    Render, Role, ScrollDelta, ScrollWheelEvent, SharedString, Subscription, Toggled, Window,
};
use std::cell::Cell;
use std::rc::Rc as Shared;

use crate::editor::geom::Handle;
use crate::editor::keys;
use crate::editor::state::Interaction;
use crate::editor::tools::{
    palette_name, Param, ParamValue, Tool, OPACITIES, PALETTE, STEP_SIZES, TEXT_SIZES, WIDTHS,
};
use crate::editor::{Insets, KeyCommand, Mods, Viewport};
use crate::theme::{scale, type_scale, Color, Mode, Theme};
use crate::ui::app::{self, AppModel, Route};
use crate::ui::components::{
    button, button_with_icon, card, crumb_link, icon, icon_button, keycaps, page_toolbar_actions,
    section_label, shrink_text, ButtonKind,
};
use crate::ui::editor_doc::{EditorDoc, Outcome, OverlayData, Phase, RedactPhase, Status};
use crate::ui::kit::{status, StatusKind};
use crate::ui::paint::{rgba_of, TextExt};
use crate::ui::studio::ThemeGlobal;
use crate::ui::tooltip;

/// Tool buttons are 44 px hit targets (DESIGN 4); the rail card adds 6 px padding and a border.
const TOOL_BTN: f32 = 44.;
const TOOL_RAIL_W: f32 = TOOL_BTN + 14.;
const INSPECTOR_W: f32 = 300.;
/// Gap between the window edge of the canvas and the floating pills.
const PILL_MARGIN: f32 = 16.;
/// Height of the transport pill (44 px buttons plus padding and border).
const TRANSPORT_H: f32 = TOOL_BTN + 14.;
/// Canvas space the floating pills take: the image is fitted in the rest (redesign R4).
const CHROME_INSETS: Insets = Insets {
    left: PILL_MARGIN as f64 + TOOL_RAIL_W as f64 + PILL_MARGIN as f64,
    top: 0.0,
    right: 0.0,
    bottom: PILL_MARGIN as f64 + TRANSPORT_H as f64 + PILL_MARGIN as f64,
};
const HANDLE: f32 = 8.;
/// Pointer tolerance for hit tests, logical px.
const HIT_PX: f64 = 6.;

/// Whether `tool` opens a new group in the tool pill (draw | redact | crop dividers).
fn tool_group_starts(tool: Tool) -> bool {
    matches!(tool, Tool::Arrow | Tool::Blur | Tool::Crop)
}

fn mod_label() -> &'static str {
    if cfg!(target_os = "macos") {
        "\u{2318}"
    } else {
        // i18n-ignore (key name)
        "Ctrl"
    }
}

fn shift_label() -> &'static str {
    if cfg!(target_os = "macos") {
        "\u{21E7}"
    } else {
        // i18n-ignore (key name)
        "Shift"
    }
}

/// Selected-state border of segmented controls and tools: `rec` on dark, `ink` on light
/// (DESIGN 4, kit `Editor` and `EditorLight`).
fn accent(t: &Theme) -> Color {
    if t.mode == Mode::Dark {
        t.rec
    } else {
        t.ink
    }
}

/// Keycaps inside the primary button: tinted from the button's own text color so they read on
/// `rec` (dark) and on `ink` (light).
fn primary_keycaps(t: &Theme, keys: &[String]) -> impl IntoElement {
    let tint = Color((t.primary_fg.0 & 0xFFFF_FF00) | 0x29);
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(4.))
        .children(keys.iter().map(move |k| {
            div()
                .min_w(px(22.))
                .px(px(6.))
                .py(px(2.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(scale::RADIUS_XS))
                .bg(rgba_of(tint))
                .text_size(px(type_scale::CAPTION))
                .font_weight(FontWeight::NORMAL)
                .mono()
                .child(SharedString::from(k.clone()))
        }))
}

fn gpui_rgb(c: OpColor) -> gpui::Rgba {
    gpui::rgb((u32::from(c.r) << 16) | (u32::from(c.g) << 8) | u32::from(c.b))
}

fn mods_of(m: gpui::Modifiers) -> Mods {
    Mods {
        shift: m.shift,
        alt: m.alt,
        secondary: if cfg!(target_os = "macos") {
            m.platform
        } else {
            m.control
        },
    }
}

pub struct EditorView {
    app: Entity<AppModel>,
    doc: Option<Entity<EditorDoc>>,
    capture_id: String,
    viewport: Viewport,
    /// Canvas bounds in window px, as painted last frame.
    canvas_bounds: Shared<Cell<Bounds<Pixels>>>,
    root_focus: FocusHandle,
    canvas_focus: FocusHandle,
    want_focus: bool,
    /// The keyboard shortcuts popover (hidden by default; opened by the keyboard button).
    shortcuts_open: bool,
    space_down: bool,
    /// Last pointer position while panning (window px).
    pan_from: Option<(f32, f32)>,
    last_epoch: u64,
    _subs: Vec<Subscription>,
}

impl EditorView {
    pub fn new(app: Entity<AppModel>, cx: &mut Context<Self>) -> Self {
        let epoch = app.read(cx).library_epoch;
        let sub = cx.observe(&app, |this, app, cx| {
            let e = app.read(cx).library_epoch;
            if e != this.last_epoch {
                this.last_epoch = e;
                if let Some(doc) = &this.doc {
                    doc.update(cx, |d, cx| d.refresh_if_changed(cx));
                }
            }
        });
        let mut viewport = Viewport::new(Rc::new(0.0, 0.0, 1.0, 1.0), (800.0, 500.0), 2.0);
        viewport.set_insets(CHROME_INSETS);
        viewport.fit_view();
        EditorView {
            app,
            doc: None,
            capture_id: String::new(),
            viewport,
            canvas_bounds: Shared::new(Cell::new(Bounds::default())),
            root_focus: cx.focus_handle(),
            canvas_focus: cx.focus_handle(),
            want_focus: false,
            shortcuts_open: false,
            space_down: false,
            pan_from: None,
            last_epoch: epoch,
            _subs: vec![sub],
        }
    }

    pub fn is_open(&self) -> bool {
        self.doc.is_some()
    }

    /// Opens `id` (no-op if it is already open).
    pub fn set_capture(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.doc.is_some() && self.capture_id == id {
            return;
        }
        self.close(cx);
        self.capture_id = id.to_owned();
        let app = self.app.clone();
        let id = id.to_owned();
        let doc = cx.new(|cx| EditorDoc::open(app, id, cx));
        self._subs.push(cx.observe(&doc, |_, _, cx| cx.notify()));
        self.doc = Some(doc);
        self.want_focus = true;
        self.viewport.fit_view();
        self.space_down = false;
        self.pan_from = None;
        cx.notify();
    }

    /// The editor was left: pending saves (and the copies queued after them, CAP-4) finish and
    /// the images are released.
    pub fn close(&mut self, cx: &mut Context<Self>) {
        if let Some(doc) = self.doc.take() {
            doc.update(cx, |d, cx| d.close(cx));
            self.capture_id.clear();
            // Keep only the epoch observer.
            self._subs.truncate(1);
        }
    }

    fn canvas_size(&self) -> (f64, f64) {
        let b = self.canvas_bounds.get();
        (
            f64::from(f32::from(b.size.width)),
            f64::from(f32::from(b.size.height)),
        )
    }

    fn to_image(&self, pos: gpui::Point<Pixels>) -> Pt {
        let b = self.canvas_bounds.get();
        self.viewport.to_image(
            f64::from(f32::from(pos.x - b.origin.x)),
            f64::from(f32::from(pos.y - b.origin.y)),
        )
    }

    fn local(&self, pos: gpui::Point<Pixels>) -> (f64, f64) {
        let b = self.canvas_bounds.get();
        (
            f64::from(f32::from(pos.x - b.origin.x)),
            f64::from(f32::from(pos.y - b.origin.y)),
        )
    }

    fn tol(&self) -> f64 {
        self.viewport.image_len(HIT_PX)
    }

    fn leave(&mut self, cx: &mut Context<Self>) {
        app::set_route(&self.app, Route::Screenshots, cx);
    }

    // ---- events -------------------------------------------------------------------------

    fn on_key_down(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = self.doc.clone() else { return };
        let k = &ev.keystroke;
        let mods = mods_of(k.modifiers);
        let editing = doc.read(cx).state.text.is_some();
        if editing {
            if mods.secondary && k.key == "v" {
                if let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) {
                    doc.update(cx, |d, cx| d.text_paste(&text, cx));
                }
                cx.stop_propagation();
                return;
            }
            let used = doc.update(cx, |d, cx| {
                d.text_key(&k.key, k.key_char.as_deref(), mods, cx)
            });
            if used {
                cx.stop_propagation();
                return;
            }
        }
        if k.key == "escape" && self.shortcuts_open && !editing {
            self.shortcuts_open = false;
            cx.stop_propagation();
            cx.notify();
            return;
        }
        let canvas_focused = self.canvas_focus.is_focused(window);
        if k.key == "space" && canvas_focused && !editing {
            self.space_down = true;
            cx.stop_propagation();
            return;
        }
        let Some(cmd) = keys::interpret(&k.key, mods, canvas_focused) else {
            return;
        };
        cx.stop_propagation();
        match cmd {
            KeyCommand::ZoomIn => self.viewport.zoom_step(true),
            KeyCommand::ZoomOut => self.viewport.zoom_step(false),
            KeyCommand::ZoomFit => self.viewport.fit_view(),
            KeyCommand::ZoomActual => self.viewport.actual_size(),
            KeyCommand::Back => self.leave(cx),
            KeyCommand::SaveAs => self.save_as(cx),
            other => {
                let (cw, ch) = self.canvas_size();
                let center = self.viewport.to_image(cw / 2.0, ch / 2.0);
                let out = doc.update(cx, |d, cx| d.command(other, center, cx));
                if out == Outcome::Leave {
                    self.leave(cx);
                }
            }
        }
        cx.notify();
    }

    fn on_key_up(&mut self, ev: &KeyUpEvent, _w: &mut Window, cx: &mut Context<Self>) {
        if ev.keystroke.key == "space" && self.space_down {
            self.space_down = false;
            self.pan_from = None;
            cx.notify();
        }
    }

    fn on_mouse_down(&mut self, ev: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = self.doc.clone() else { return };
        window.focus(&self.canvas_focus, cx);
        if self.shortcuts_open {
            // A press on the canvas is "outside" the shortcuts popover.
            self.shortcuts_open = false;
            cx.notify();
        }
        if ev.button == MouseButton::Middle || (self.space_down && ev.button == MouseButton::Left) {
            self.pan_from = Some((f32::from(ev.position.x), f32::from(ev.position.y)));
            cx.notify();
            return;
        }
        if ev.button != MouseButton::Left {
            return;
        }
        let p = self.to_image(ev.position);
        let tol = self.tol();
        doc.update(cx, |d, cx| {
            d.pointer_down(p, mods_of(ev.modifiers), ev.click_count, tol, cx)
        });
    }

    fn on_mouse_move(&mut self, ev: &MouseMoveEvent, _w: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = self.doc.clone() else { return };
        if let Some((lx, ly)) = self.pan_from {
            let (x, y) = (f32::from(ev.position.x), f32::from(ev.position.y));
            self.viewport.pan_by(f64::from(x - lx), f64::from(y - ly));
            self.pan_from = Some((x, y));
            cx.notify();
            return;
        }
        let p = self.to_image(ev.position);
        let tol = self.tol();
        doc.update(cx, |d, cx| {
            d.pointer_move(p, mods_of(ev.modifiers), tol, cx)
        });
    }

    fn on_mouse_up(&mut self, ev: &MouseUpEvent, _w: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = self.doc.clone() else { return };
        if self.pan_from.take().is_some() {
            cx.notify();
            return;
        }
        if ev.button != MouseButton::Left {
            return;
        }
        let p = self.to_image(ev.position);
        let tol = self.tol();
        doc.update(cx, |d, cx| d.pointer_up(p, mods_of(ev.modifiers), tol, cx));
    }

    fn on_scroll(&mut self, ev: &ScrollWheelEvent, _w: &mut Window, cx: &mut Context<Self>) {
        let (lx, ly) = self.local(ev.position);
        let d = match ev.delta {
            ScrollDelta::Pixels(p) => (f64::from(f32::from(p.x)), f64::from(f32::from(p.y))),
            ScrollDelta::Lines(l) => (f64::from(l.x) * 20.0, f64::from(l.y) * 20.0),
        };
        let m = mods_of(ev.modifiers);
        if m.secondary || ev.modifiers.control {
            self.viewport.zoom_by_at((d.1 * 0.01).exp(), lx, ly);
        } else {
            self.viewport.pan_by(d.0, d.1);
        }
        cx.notify();
    }

    fn on_pinch(&mut self, ev: &PinchEvent, _w: &mut Window, cx: &mut Context<Self>) {
        let (lx, ly) = self.local(ev.position);
        self.viewport
            .zoom_by_at((1.0 + f64::from(ev.delta)).max(0.2), lx, ly);
        cx.notify();
    }

    fn save_as(&mut self, cx: &mut Context<Self>) {
        let Some(doc) = self.doc.clone() else { return };
        let name = crate::editor::session::suggested_file_name(&doc.read(cx).title);
        let dir = dirs::picture_dir()
            .or_else(dirs::home_dir)
            .unwrap_or_else(std::env::temp_dir);
        let rx = cx.prompt_for_new_path(&dir, Some(&name));
        cx.spawn(async move |_, cx| {
            if let Ok(Ok(Some(path))) = rx.await {
                doc.update(cx, |d, cx| d.save_to(path, cx));
            }
        })
        .detach();
    }

    fn reveal_label() -> String {
        if cfg!(target_os = "macos") {
            tr!("Reveal in Finder")
        } else {
            tr!("Show in folder")
        }
    }

    fn reveal(&mut self, cx: &mut Context<Self>) {
        if let Some(doc) = &self.doc {
            let dir = doc.read(cx).dir.clone();
            cx.reveal_path(&dir);
        }
    }
}

// ---- rendering --------------------------------------------------------------------------

impl Render for EditorView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.global::<ThemeGlobal>().0;
        let Some(doc_e) = self.doc.clone() else {
            return div().size_full().bg(rgba_of(t.ground)).into_any_element();
        };
        let phase = doc_e.read(cx).phase.clone();
        match phase {
            Phase::Loading => self.status_screen(&t, &tr!("Opening the capture\u{2026}"), None),
            Phase::Failed(e) => self.status_screen(
                &t,
                &tr!("Couldn\u{2019}t open this capture"),
                Some(e.text()),
            ),
            Phase::Ready => {
                if self.want_focus {
                    self.want_focus = false;
                    window.focus(&self.canvas_focus, cx);
                }
                self.ready(&t, doc_e, window, cx)
            }
        }
    }
}

/// What the UI needs from the document, copied out so the document is not borrowed while
/// listeners are built.
struct Snap {
    tool: Tool,
    can_undo: bool,
    can_redo: bool,
    has_ops: bool,
    title: String,
    image: (u32, u32),
    image_handle: Option<std::sync::Arc<gpui::RenderImage>>,
    summary: String,
    hover: Option<Pt>,
    /// Pointer position in image px, number, diameter in image px, color.
    step_hint: Option<(Pt, u32, f64, OpColor)>,
    status: Status,
    selected: bool,
    editing_text: bool,
    has_crop: bool,
    inspector_title: String,
    params: Vec<(Param, ParamValue)>,
    warning: Option<String>,
    /// Auto-redact (ANN-2.AC4): shown only when the OS has a text recognizer.
    redact_available: bool,
    redact: RedactPhase,
    /// Kind label, masked preview, confidence of each proposal.
    proposals: Vec<(String, String, f64)>,
}

impl Snap {
    fn of(doc: &EditorDoc, _zoom: f64) -> Snap {
        let st = &doc.state;
        let idle = st.interaction == Interaction::Idle;
        let step_hint = match (st.tool, st.hover, idle) {
            (Tool::Step, Some(p), true) => {
                let style = st.styles.get(Tool::Step);
                Some((p, st.next_step_number(), style.size, style.color))
            }
            _ => None,
        };
        Snap {
            tool: st.tool,
            can_undo: st.can_undo || doc.pending() > 0,
            can_redo: st.can_redo,
            has_ops: !st.ops.is_empty(),
            title: doc.title.clone(),
            image: doc.image_size(),
            image_handle: doc.display_image(),
            summary: st.selection_summary(),
            hover: st.hover,
            step_hint,
            status: doc.status.clone(),
            selected: st.selected.is_some(),
            editing_text: st.text.is_some(),
            has_crop: st.crop_rect().is_some(),
            inspector_title: st.inspector_title(),
            params: st
                .inspector_params()
                .iter()
                .filter_map(|p| st.inspector_value(*p).map(|v| (*p, v)))
                .collect(),
            warning: doc.warnings.first().cloned(),
            redact_available: crate::editor::redact::available(),
            redact: doc.redact.clone(),
            proposals: st
                .proposals
                .iter()
                .map(|p| (p.kind_label(), p.preview.clone(), p.confidence))
                .collect(),
        }
    }
}

/// Selected state for toggle-like controls.
fn toggled(on: bool) -> Toggled {
    if on {
        Toggled::True
    } else {
        Toggled::False
    }
}

/// "16 pixels": the accessible name of a size button.
fn pixels_label(v: f64) -> String {
    trn!("{n} pixel", "{n} pixels", v.round() as u64)
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

impl EditorView {
    fn status_screen(&self, t: &Theme, title: &str, detail: Option<String>) -> AnyElement {
        let app = self.app.clone();
        div()
            .id("editor-status")
            .role(Role::Group)
            .aria_label(SharedString::from(title.to_owned()))
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(12.))
            .bg(rgba_of(t.ground))
            .text_color(rgba_of(t.text))
            .child(
                div()
                    .text_size(px(type_scale::TITLE))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(SharedString::from(title.to_owned())),
            )
            .when_some(detail, |d, e| {
                d.child(
                    div()
                        .max_w(px(420.))
                        .text_color(rgba_of(t.text_2))
                        .child(SharedString::from(e)),
                )
            })
            .child(
                button(
                    t,
                    "editor-back",
                    tr!("Back to library"),
                    ButtonKind::Secondary,
                )
                .on_click(move |_, _, cx| app::set_route(&app, Route::Screenshots, cx)),
            )
            .into_any_element()
    }

    fn ready(
        &mut self,
        t: &Theme,
        doc_e: Entity<EditorDoc>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Keep the viewport in step with the canvas size, the crop and the window's DPR.
        let measured = self.canvas_size();
        if measured.0 > 8.0 && measured.1 > 8.0 {
            self.viewport.set_canvas(measured);
        }
        self.viewport.set_insets(CHROME_INSETS);
        self.viewport.dpr = f64::from(window.scale_factor());
        let content = doc_e.read(cx).state.content_rect();
        self.viewport.set_content(content);

        let overlay = doc_e.update(cx, |d, _| d.overlay_data());
        let snap = Snap::of(doc_e.read(cx), self.viewport.zoom());
        let toolbar = self.toolbar(t, &snap, cx);
        let canvas = self.canvas_pane(t, &snap, overlay, cx);
        let pills = self.floating_chrome(t, &snap, cx);
        let popover = self.shortcuts_open.then(|| self.shortcuts_popover(t, cx));
        let inspector = self.inspector(t, &snap, cx);
        let warn = snap.warning.clone();

        div()
            .id("editor")
            .role(Role::Group)
            .aria_label(tr!("Annotation editor"))
            // i18n-ignore (key context id)
            .key_context("Editor")
            .track_focus(&self.root_focus)
            .on_key_down(cx.listener(Self::on_key_down))
            .on_key_up(cx.listener(Self::on_key_up))
            .size_full()
            .flex()
            .flex_col()
            .bg(rgba_of(t.ground))
            .text_color(rgba_of(t.text))
            .child(toolbar)
            .when_some(warn, |d, w| {
                d.child(
                    div()
                        .id("editor-warning")
                        .role(Role::Alert)
                        .aria_label(SharedString::from(tr!("Warning: {w}", w = w)))
                        .px(px(16.))
                        .py(px(6.))
                        .bg(rgba_of(crate::theme::derived::WARNING_TINT))
                        .text_color(rgba_of(t.warning))
                        .text_size(px(type_scale::CAPTION))
                        .child(SharedString::from(w)),
                )
            })
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .flex()
                    .gap(px(16.))
                    .px(px(24.))
                    .py(px(16.))
                    // The stage: the canvas fills it; the pills are absolute siblings above it,
                    // so the canvas element and its hit testing are untouched.
                    .child(
                        div()
                            .relative()
                            .flex_1()
                            .min_w(px(0.))
                            .min_h(px(0.))
                            .flex()
                            .child(canvas)
                            .child(pills)
                            .children(popover),
                    )
                    .child(inspector),
            )
            .into_any_element()
    }

    // ---- toolbar ------------------------------------------------------------------------

    /// A DESIGN toolbar button with keycaps; `enabled == false` dims it and drops the action.
    #[allow(clippy::too_many_arguments)]
    fn tbtn(
        &self,
        t: &Theme,
        id: &'static str,
        label: String,
        glyph: Option<&'static str>,
        kind: ButtonKind,
        keys: &[String],
        enabled: bool,
        on: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let b = button_with_icon(t, id, label.clone(), kind, glyph)
            .when(kind == ButtonKind::Primary && !keys.is_empty(), |b| {
                b.child(primary_keycaps(t, keys))
            })
            .tooltip(tooltip::tip(label, keys.to_vec()));
        if enabled {
            b.on_click(cx.listener(move |this, _: &ClickEvent, window, cx| on(this, window, cx)))
                .into_any_element()
        } else {
            b.opacity(0.45).cursor_default().into_any_element()
        }
    }

    /// Icon (or short text) button, 44 px target, kit radius 10; `enabled == false` dims it.
    #[allow(clippy::too_many_arguments)]
    fn ibtn(
        &self,
        t: &Theme,
        id: &'static str,
        label: String,
        content: AnyElement,
        keys: &[String],
        enabled: bool,
        on: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let hover = rgba_of(t.raised);
        let focus = rgba_of(t.text);
        let b = div()
            .id(id)
            .role(Role::Button)
            .aria_label(label.clone())
            .focusable()
            .tab_stop(true)
            .h(px(TOOL_BTN))
            .min_w(px(TOOL_BTN))
            .px(px(8.))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .rounded(px(scale::RADIUS_MD))
            .border_1()
            .border_color(gpui::transparent_black())
            .text_size(px(type_scale::CAPTION))
            .text_color(rgba_of(t.text_2))
            .mono()
            .hover(move |s| s.bg(hover))
            .focus_visible(move |s| s.border_color(focus))
            .tooltip(tooltip::tip(label, keys.to_vec()))
            .child(content);
        if enabled {
            b.cursor_pointer()
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| on(this, window, cx)))
                .into_any_element()
        } else {
            b.opacity(0.4).cursor_default().into_any_element()
        }
    }

    /// Header (kit `Editor` top bar): breadcrumb and title on the left, Save as / Reveal /
    /// Copy image on the right. It wraps instead of clipping when the window is narrow.
    fn toolbar(&self, t: &Theme, s: &Snap, cx: &mut Context<Self>) -> AnyElement {
        let title = if s.title.is_empty() {
            self.capture_id.clone()
        } else {
            s.title.clone()
        };
        let m = mod_label().to_owned();
        let shifted = |k: &str| {
            if cfg!(target_os = "macos") {
                vec![shift_label().to_owned(), m.clone(), k.to_owned()]
            } else {
                vec![m.clone(), shift_label().to_owned(), k.to_owned()]
            }
        };
        let back_keys = vec![m.clone(), "[".to_owned()];
        let crumb = crumb_link(
            t,
            "editor-back",
            tr_ctx!("breadcrumb", "Screenshots /"),
            tr!("Library"),
        )
        .aria_keyshortcuts("[")
        .tooltip(tooltip::tip(tr!("Back to library"), back_keys))
        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.leave(cx)));
        let reveal = Self::reveal_label();
        let actions: Vec<AnyElement> = vec![
            // Icon-only: the aria label and the tooltip carry the (platform) name.
            self.reveal_button(t, reveal, cx),
            self.tbtn(
                t,
                "editor-save-as",
                tr!("Save as\u{2026}"),
                Some("save"),
                ButtonKind::Secondary,
                &shifted("S"),
                true,
                |this, _, cx| this.save_as(cx),
                cx,
            ),
            self.tbtn(
                t,
                "editor-copy",
                tr!("Copy image"),
                Some("copy"),
                ButtonKind::Primary,
                &shifted("C"),
                true,
                |this, _, cx| {
                    if let Some(d) = &this.doc {
                        d.update(cx, |d, cx| d.copy_image(cx));
                    }
                },
                cx,
            ),
        ];
        page_toolbar_actions(
            t,
            "editor-toolbar",
            tr!("Editor toolbar"),
            crumb,
            SharedString::from(title),
            actions,
        )
        .into_any_element()
    }

    /// The folder button of the header: a 40 px secondary square with the `folder-open` icon.
    fn reveal_button(&self, t: &Theme, label: String, cx: &mut Context<Self>) -> AnyElement {
        let c = ButtonKind::Secondary.colors(t);
        let (hover, focus) = (rgba_of(c.hover), rgba_of(t.text));
        div()
            .id("editor-reveal")
            .role(Role::Button)
            .aria_label(label.clone())
            .focusable()
            .tab_stop(true)
            .size(px(scale::CONTROL_H))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .rounded(px(scale::RADIUS_MD))
            .border_1()
            .border_color(rgba_of(c.border))
            .bg(rgba_of(c.bg))
            .cursor_pointer()
            .hover(move |s| s.bg(hover))
            .focus_visible(move |s| s.border_color(focus))
            .tooltip(tooltip::tip(label, Vec::new()))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.reveal(cx)))
            .child(icon("folder-open", 16., c.fg))
            .into_any_element()
    }

    /// The floating chrome over the canvas (redesign R4): the tool pill on the left and the
    /// transport pill at the bottom. Both are absolute SIBLINGS of the canvas: their wrappers
    /// have no hitbox (pointer events beside the pills reach the canvas) and the pills occlude
    /// (events on them never reach the canvas).
    fn floating_chrome(&self, t: &Theme, s: &Snap, cx: &mut Context<Self>) -> AnyElement {
        let tools = div()
            .absolute()
            .left(px(PILL_MARGIN))
            .top(px(PILL_MARGIN))
            .bottom(px(PILL_MARGIN))
            .flex()
            .items_center()
            .child(self.tool_rail(t, s.tool, cx));
        let transport = div()
            .absolute()
            .left(px(CHROME_INSETS.left as f32))
            .right_0()
            .bottom(px(PILL_MARGIN))
            .flex()
            .justify_center()
            .child(self.transport(t, s, cx));
        div()
            .absolute()
            .size_full()
            .child(tools)
            .child(transport)
            .into_any_element()
    }

    /// A thin divider between button groups inside a pill.
    fn pill_divider(t: &Theme) -> Div {
        div()
            .w(px(1.))
            .h(px(24.))
            .mx(px(4.))
            .flex_none()
            .bg(rgba_of(t.line))
    }

    /// The pill's surface: `surface`, `line` border, radius 14, above the canvas.
    fn pill_surface(t: &Theme) -> Div {
        div()
            .occlude()
            .p(px(6.))
            .rounded(px(scale::RADIUS_LG))
            .border_1()
            .border_color(rgba_of(t.line))
            .bg(rgba_of(t.surface))
    }

    /// The view pill under the canvas: undo, redo, (auto-redact), revert, zoom, Fit, 100% and
    /// the keyboard-shortcuts toggle.
    fn transport(&self, t: &Theme, s: &Snap, cx: &mut Context<Self>) -> AnyElement {
        let (can_undo, can_redo, has_ops) = (s.can_undo, s.can_redo, s.has_ops);
        let m = mod_label().to_owned();
        let plain = |k: &str| vec![m.clone(), k.to_owned()];
        let shifted = |k: &str| {
            if cfg!(target_os = "macos") {
                vec![shift_label().to_owned(), m.clone(), k.to_owned()]
            } else {
                vec![m.clone(), shift_label().to_owned(), k.to_owned()]
            }
        };
        let c = t.text_2;
        let zoom = format!("{:.0}%", self.viewport.zoom_percent());
        let open = self.shortcuts_open;

        Self::pill_surface(t)
            .id("editor-transport")
            .role(Role::Toolbar)
            .aria_label(tr!("Undo and zoom"))
            .max_w_full()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(2.))
            .child(self.ibtn(
                t,
                "editor-undo",
                tr!("Undo"),
                icon("undo-2", 18., c).into_any_element(),
                &plain("Z"),
                can_undo,
                |this, _, cx| {
                    if let Some(d) = &this.doc {
                        d.update(cx, |d, cx| d.undo(cx));
                    }
                },
                cx,
            ))
            .child(self.ibtn(
                t,
                "editor-redo",
                tr!("Redo"),
                icon("redo-2", 18., c).into_any_element(),
                &shifted("Z"),
                can_redo,
                |this, _, cx| {
                    if let Some(d) = &this.doc {
                        d.update(cx, |d, cx| d.redo(cx));
                    }
                },
                cx,
            ))
            .when(s.redact_available, |d| {
                d.child(
                    self.ibtn(
                        t,
                        "editor-auto-redact",
                        tr!("Auto-redact"),
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(icon("eye-off", 16., c))
                            .child(tr!("Auto-redact"))
                            .into_any_element(),
                        &[],
                        s.redact != RedactPhase::Running,
                        |this, _, cx| {
                            if let Some(d) = &this.doc {
                                d.update(cx, |d, cx| d.auto_redact(cx));
                            }
                        },
                        cx,
                    ),
                )
            })
            .child(self.ibtn(
                t,
                "editor-revert",
                tr!("Revert to original"),
                icon("rotate-ccw", 18., c).into_any_element(),
                &[],
                has_ops,
                |this, _, cx| {
                    if let Some(d) = &this.doc {
                        d.update(cx, |d, cx| d.revert(cx));
                    }
                },
                cx,
            ))
            .child(Self::pill_divider(t))
            .child(self.ibtn(
                t,
                "editor-zoom-out",
                tr!("Zoom out"),
                icon("zoom-out", 18., c).into_any_element(),
                &plain("-"),
                true,
                |this, _, _| this.viewport.zoom_step(false),
                cx,
            ))
            .child(
                div()
                    .id("editor-zoom")
                    .role(Role::Label)
                    .aria_label(SharedString::from(tr_ctx!(
                        "zoom level",
                        "Zoom {zoom}",
                        zoom = zoom
                    )))
                    .min_w(px(48.))
                    .flex()
                    .justify_center()
                    .text_size(px(type_scale::CAPTION))
                    .text_color(rgba_of(t.text))
                    .mono()
                    .tnum()
                    .child(SharedString::from(zoom)),
            )
            .child(self.ibtn(
                t,
                "editor-zoom-in",
                tr!("Zoom in"),
                icon("zoom-in", 18., c).into_any_element(),
                &plain("+"),
                true,
                |this, _, _| this.viewport.zoom_step(true),
                cx,
            ))
            .child(self.ibtn(
                t,
                "editor-zoom-fit",
                tr_ctx!("zoom", "Fit"),
                icon("shrink", 18., c).into_any_element(),
                &plain("0"),
                true,
                |this, _, _| this.viewport.fit_view(),
                cx,
            ))
            .child(
                self.ibtn(
                    t,
                    "editor-zoom-100",
                    "100%".to_owned(),
                    div()
                        .text_color(rgba_of(t.mist))
                        .child("100%")
                        .into_any_element(),
                    &plain("1"),
                    true,
                    |this, _, _| this.viewport.actual_size(),
                    cx,
                ),
            )
            .child(Self::pill_divider(t))
            .child(
                icon_button(
                    t,
                    "editor-shortcuts",
                    "keyboard",
                    tr!("Keyboard shortcuts"),
                    open,
                    Some(Vec::new()),
                )
                .aria_expanded(open)
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.shortcuts_open = !this.shortcuts_open;
                    cx.notify();
                })),
            )
            .into_any_element()
    }

    /// The shortcuts popover: hidden by default, opened by the keyboard button, closed by Esc,
    /// the button again or a press on the canvas. Anchored above the transport pill at the
    /// canvas' bottom-right (the `deferred(anchored())` pattern of the library's card menu).
    fn shortcuts_popover(&self, t: &Theme, _cx: &mut Context<Self>) -> AnyElement {
        let b = self.canvas_bounds.get();
        let at = point(
            b.origin.x + b.size.width - px(PILL_MARGIN),
            b.origin.y + b.size.height - px(PILL_MARGIN + TRANSPORT_H + 8.),
        );
        let m = mod_label().to_owned();
        let row = |keys: Vec<String>, text: String| {
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(12.))
                .child(
                    div()
                        .min_w(px(0.))
                        .text_size(px(13.))
                        .text_color(rgba_of(t.text_3))
                        .child(text),
                )
                .child(keycaps(t, &keys))
        };
        let body = div()
            .id("editor-shortcuts-popover")
            .occlude()
            .role(Role::Dialog)
            .aria_label(tr!("Keyboard shortcuts"))
            .w(px(280.))
            .p(px(14.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .rounded(px(scale::RADIUS_LG))
            .border_1()
            .border_color(rgba_of(t.line))
            .bg(rgba_of(t.surface))
            .text_color(rgba_of(t.text))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .text_size(px(type_scale::UI))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(icon("keyboard", 16., t.text_2))
                    .child(shrink_text(tr!("Shortcuts"))),
            )
            .child(row(
                vec![shift_label().to_owned()],
                tr!("Constrain angle or shape"),
            ))
            .child(row(vec!["[".into(), "]".into()], tr!("Pick an annotation")))
            .child(row(
                vec![
                    "\u{2190}".into(),
                    "\u{2191}".into(),
                    "\u{2192}".into(),
                    "\u{2193}".into(),
                ],
                tr!("Move (Alt resizes)"),
            ))
            .child(row(vec!["Enter".into()], tr!("Place at the center")))
            .child(row(vec![m.clone(), "C".into()], tr!("Copy annotation")))
            .child(row(vec![m, "V".into()], tr!("Paste (+10 px)")))
            .child(row(vec!["Space".into()], tr!("Hold and drag to pan")));
        deferred(
            anchored()
                .position(at)
                .anchor(gpui::Anchor::BottomRight)
                .snap_to_window_with_margin(px(8.))
                .child(body),
        )
        .with_priority(10)
        .into_any_element()
    }

    // ---- tool palette -------------------------------------------------------------------

    /// The tool pill: 11 tools in three groups (draw, redact, crop), 44 px targets. The selected
    /// tool is the board's filled tile (accent fill, button-text glyph).
    fn tool_rail(&self, t: &Theme, active: Tool, cx: &mut Context<Self>) -> AnyElement {
        let mut rail = Self::pill_surface(t)
            .id("editor-tools")
            .role(Role::Toolbar)
            .aria_label(tr!("Annotation tools"))
            .aria_orientation(gpui::Orientation::Vertical)
            .w(px(TOOL_RAIL_W))
            .max_h_full()
            .flex_none()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(2.))
            .overflow_y_scroll();
        for (i, tool) in Tool::ALL.into_iter().enumerate() {
            if tool_group_starts(tool) {
                rail = rail.child(
                    div()
                        .w(px(24.))
                        .h(px(1.))
                        .my(px(4.))
                        .flex_none()
                        .bg(rgba_of(t.line)),
                );
            }
            let on = tool == active;
            let key = tool.key().to_ascii_uppercase().to_string();
            let tip_key = vec![key.clone()];
            let hover = rgba_of(t.raised);
            rail = rail.child(
                div()
                    .id(ElementId::from(("tool", i)))
                    .role(Role::Button)
                    .aria_label(tool.label())
                    .aria_description(SharedString::from(tr!("Shortcut {key}", key = key)))
                    .aria_keyshortcuts(SharedString::from(key))
                    .aria_toggled(toggled(on))
                    .focusable()
                    .tab_stop(true)
                    .size(px(TOOL_BTN))
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .rounded(px(scale::RADIUS_MD))
                    .border_1()
                    .border_color(gpui::transparent_black())
                    .cursor_pointer()
                    .when(on, |d| d.bg(rgba_of(accent(t))))
                    .when(!on, |d| d.hover(move |s| s.bg(hover)))
                    .focus_visible(|s| s.border_color(rgba_of(t.text)))
                    .tooltip(tooltip::tip(tool.label(), tip_key.clone()))
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        if let Some(d) = &this.doc {
                            d.update(cx, |d, cx| d.set_tool(tool, cx));
                        }
                        window.focus(&this.canvas_focus, cx);
                    }))
                    .child(icon(
                        tool.icon(),
                        20.,
                        if on { t.primary_fg } else { t.text_2 },
                    )),
            );
        }
        rail.into_any_element()
    }

    // ---- canvas -------------------------------------------------------------------------

    fn canvas_pane(
        &mut self,
        t: &Theme,
        s: &Snap,
        overlay: OverlayData,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let vp = self.viewport;
        let z = vp.zoom();
        let (sx, sy, sw, sh) = vp.screen_rect();
        let (iw, ih) = s.image;
        let tool = s.tool;
        let cell = self.canvas_bounds.clone();
        let entity = cx.entity();
        let used = vp.canvas;
        let theme = *t;
        let cursor = if self.pan_from.is_some() {
            CursorStyle::ClosedHand
        } else if self.space_down {
            CursorStyle::OpenHand
        } else {
            match tool {
                Tool::Select => CursorStyle::Arrow,
                Tool::Text => CursorStyle::IBeam,
                _ => CursorStyle::Crosshair,
            }
        };
        let summary = s.summary.clone();
        let step_hint = s.step_hint.map(|(p, n, size, color)| {
            let (x, y) = vp.to_screen(p);
            (x, y, n, size * z, color)
        });
        let image = s.image_handle.clone();

        let image_layer = image.map(|im| {
            div()
                .absolute()
                .left(px(sx as f32))
                .top(px(sy as f32))
                .w(px(sw as f32))
                .h(px(sh as f32))
                .overflow_hidden()
                .child(
                    img(im)
                        .absolute()
                        .left(px((-vp.content.x * z) as f32))
                        .top(px((-vp.content.y * z) as f32))
                        .w(px((f64::from(iw) * z) as f32))
                        .h(px((f64::from(ih) * z) as f32))
                        .object_fit(ObjectFit::Fill),
                )
        });

        let measure = canvas(
            move |bounds, window, cx| {
                cell.set(bounds);
                let w = f64::from(f32::from(bounds.size.width));
                let h = f64::from(f32::from(bounds.size.height));
                if (w - used.0).abs() > 0.5 || (h - used.1).abs() > 0.5 {
                    let e = entity.clone();
                    window.defer(cx, move |_w, cx| e.update(cx, |_, cx| cx.notify()));
                }
            },
            move |bounds, (), window, _cx| paint_overlay(bounds, &overlay, vp, &theme, window),
        )
        .absolute()
        .size_full();

        div()
            .flex_1()
            .min_w(px(0.))
            .min_h(px(0.))
            .w_full()
            .flex()
            .child(
                div()
                    .id("editor-canvas")
                    .role(Role::Canvas)
                    .aria_label(SharedString::from(tr!(
                        "Screenshot canvas. {summary}",
                        summary = summary
                    )))
                    .aria_description(tr!(
                        "Press a tool key such as A for arrow, then Enter to place it. \
                         Bracket keys pick an annotation, arrow keys move it, Delete removes it."
                    ))
                    .track_focus(&self.canvas_focus)
                    .tab_stop(true)
                    .relative()
                    .flex_1()
                    .min_w(px(0.))
                    .min_h(px(0.))
                    .overflow_hidden()
                    .rounded(px(scale::RADIUS_LG))
                    .border_1()
                    .border_color(rgba_of(t.border))
                    .focus_visible(|s| s.border_color(rgba_of(t.text)))
                    .bg(rgba_of(t.ink))
                    .cursor(cursor)
                    .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
                    .on_mouse_down(MouseButton::Middle, cx.listener(Self::on_mouse_down))
                    .on_mouse_move(cx.listener(Self::on_mouse_move))
                    .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
                    .on_mouse_up(MouseButton::Middle, cx.listener(Self::on_mouse_up))
                    .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
                    .on_scroll_wheel(cx.listener(Self::on_scroll))
                    .on_pinch(cx.listener(Self::on_pinch))
                    .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                        if !*hovered {
                            if let Some(d) = &this.doc {
                                d.update(cx, |d, cx| d.pointer_leave(cx));
                            }
                        }
                    }))
                    .child(dot_grid(t))
                    .children(image_layer)
                    .child(measure)
                    .children(step_hint.map(|(x, y, n, d, c)| {
                        // The next step number rides on the cursor (ANN-1).
                        let d = d.max(14.0);
                        div()
                            .absolute()
                            .left(px((x - d / 2.0) as f32))
                            .top(px((y - d / 2.0) as f32))
                            .size(px(d as f32))
                            .rounded_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .opacity(0.8)
                            .bg(gpui_rgb(c))
                            .text_color(gpui::white())
                            .text_size(px((d * 0.5) as f32))
                            .font_weight(FontWeight::SEMIBOLD)
                            .tnum()
                            .child(SharedString::from(n.to_string()))
                    })),
            )
            .into_any_element()
    }

    // ---- inspector ----------------------------------------------------------------------

    fn doc_action(
        &self,
        f: impl Fn(&mut EditorDoc, &mut Context<EditorDoc>) + 'static,
    ) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
        let doc = self.doc.clone();
        move |_, _, cx| {
            if let Some(d) = &doc {
                d.update(cx, |d, cx| f(d, cx));
            }
        }
    }

    fn set_param_action(
        &self,
        v: ParamValue,
    ) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
        self.doc_action(move |d, cx| d.set_param(v, cx))
    }

    fn seg_button(
        &self,
        t: &Theme,
        id: ElementId,
        text: String,
        label: String,
        on: bool,
        action: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let hover = rgba_of(t.raised);
        div()
            .id(id)
            .role(Role::Button)
            .aria_label(SharedString::from(label))
            .aria_toggled(toggled(on))
            .focusable()
            .tab_stop(true)
            .h(px(scale::CONTROL_H))
            .flex_1()
            .min_w(px(0.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(scale::RADIUS_SM))
            .border_1()
            .border_color(rgba_of(if on { accent(t) } else { t.line }))
            .cursor_pointer()
            .text_size(px(13.))
            .tnum()
            .when(on, |d| d.bg(rgba_of(t.raised)).text_color(rgba_of(t.text)))
            .when(!on, |d| {
                d.text_color(rgba_of(t.text_3)).hover(move |s| s.bg(hover))
            })
            .focus_visible(|s| s.border_color(rgba_of(t.text)))
            .on_click(action)
            .child(SharedString::from(text))
            .into_any_element()
    }

    /// One annotation-palette swatch. The palette colors are content colors (what gets drawn
    /// on the image), not UI colors.
    fn swatch(&self, t: &Theme, i: usize, name: String, c: OpColor, on: bool) -> AnyElement {
        let hover = rgba_of(t.raised);
        div()
            .id(ElementId::from(("swatch", i)))
            .role(Role::Button)
            .aria_label(name)
            .aria_toggled(toggled(on))
            .focusable()
            .tab_stop(true)
            .h(px(36.))
            .flex_1()
            .min_w(px(0.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(scale::RADIUS_SM))
            .border_1()
            .border_color(rgba_of(if on { accent(t) } else { Color(0) }))
            .when(on, |d| d.bg(rgba_of(t.raised)))
            .when(!on, |d| d.hover(move |s| s.bg(hover)))
            .cursor_pointer()
            .focus_visible(|s| s.border_color(rgba_of(t.text)))
            .on_click(self.set_param_action(ParamValue::Color(c)))
            .child(
                div()
                    .size(px(20.))
                    .rounded_full()
                    .border_1()
                    .border_color(rgba_of(t.line))
                    .bg(gpui_rgb(c)),
            )
            .into_any_element()
    }

    /// `- value +` stepper (blur strength, pixel size) exposed as a spin button.
    #[allow(clippy::too_many_arguments)]
    fn stepper(
        &self,
        t: &Theme,
        id: &'static str,
        label: String,
        (dec_label, inc_label): (String, String),
        value: u32,
        (lo, hi): (u32, u32),
        make: fn(u32) -> ParamValue,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let step = 2;
        let dec = make(value.saturating_sub(step).max(lo));
        let inc = make((value + step).min(hi));
        let doc = self.doc.clone();
        let doc2 = self.doc.clone();
        let _ = cx;
        div()
            .flex()
            .items_center()
            .gap(px(6.))
            .child(
                icon_button(
                    t,
                    (id, 0usize),
                    "minus",
                    SharedString::from(dec_label),
                    false,
                    None,
                )
                .border_color(rgba_of(t.line))
                .bg(rgba_of(t.surface_2))
                .on_click(self.set_param_action(dec)),
            )
            .child(
                div()
                    .id(ElementId::from((id, 1usize)))
                    .role(Role::SpinButton)
                    .aria_label(label)
                    .aria_numeric_value(f64::from(value))
                    .aria_min_numeric_value(f64::from(lo))
                    .aria_max_numeric_value(f64::from(hi))
                    .aria_numeric_value_step(f64::from(step))
                    .focusable()
                    .tab_stop(true)
                    .on_a11y_action(AccessibleAction::Increment, move |_, _, cx| {
                        if let Some(d) = &doc {
                            d.update(cx, |d, cx| d.set_param(inc, cx));
                        }
                    })
                    .on_a11y_action(AccessibleAction::Decrement, move |_, _, cx| {
                        if let Some(d) = &doc2 {
                            d.update(cx, |d, cx| d.set_param(dec, cx));
                        }
                    })
                    .flex_1()
                    .min_w(px(48.))
                    .h(px(scale::CONTROL_H))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(scale::RADIUS_MD))
                    .bg(rgba_of(t.raised))
                    .border_1()
                    .border_color(rgba_of(t.line))
                    .focus_visible(|s| s.border_color(rgba_of(t.text)))
                    .text_size(px(type_scale::UI))
                    .mono()
                    .tnum()
                    .child(SharedString::from(value.to_string())),
            )
            .child(
                icon_button(
                    t,
                    (id, 2usize),
                    "plus",
                    SharedString::from(inc_label),
                    false,
                    None,
                )
                .border_color(rgba_of(t.line))
                .bg(rgba_of(t.surface_2))
                .on_click(self.set_param_action(inc)),
            )
            .into_any_element()
    }

    fn param_group(
        &self,
        t: &Theme,
        p: Param,
        cur: ParamValue,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let body: AnyElement =
            match (p, cur) {
                (Param::Color, ParamValue::Color(c)) => div()
                    .flex()
                    .gap(px(2.))
                    .children(PALETTE.iter().enumerate().map(|(i, (name, col))| {
                        self.swatch(t, i, palette_name(name), *col, *col == c)
                    }))
                    .into_any_element(),
                (Param::Width, ParamValue::Width(w)) => self.segments(
                    t,
                    "width",
                    WIDTHS
                        .iter()
                        .map(|v| (*v, format!("{v:.0} px"), pixels_label(*v))),
                    w,
                    ParamValue::Width,
                ),
                (Param::TextSize, ParamValue::TextSize(w)) => self.segments(
                    t,
                    "tsize",
                    TEXT_SIZES
                        .iter()
                        .map(|v| (*v, format!("{v:.0}"), pixels_label(*v))),
                    w,
                    ParamValue::TextSize,
                ),
                (Param::StepSize, ParamValue::StepSize(w)) => self.segments(
                    t,
                    "ssize",
                    STEP_SIZES
                        .iter()
                        .map(|v| (*v, format!("{v:.0}"), pixels_label(*v))),
                    w,
                    ParamValue::StepSize,
                ),
                (Param::Opacity, ParamValue::Opacity(w)) => self.segments(
                    t,
                    "opacity",
                    OPACITIES.iter().map(|v| {
                        (
                            *v,
                            format!("{:.0}%", v * 100.0),
                            tr!("{n} percent", n = format!("{:.0}", v * 100.0)),
                        )
                    }),
                    w,
                    ParamValue::Opacity,
                ),
                (Param::Strength, ParamValue::Strength(v)) => self.stepper(
                    t,
                    "strength",
                    tr_ctx!("annotation setting", "Blur strength"),
                    (tr!("Decrease blur strength"), tr!("Increase blur strength")),
                    v,
                    (4, 32),
                    ParamValue::Strength,
                    cx,
                ),
                (Param::Block, ParamValue::Block(v)) => self.stepper(
                    t,
                    "block",
                    tr_ctx!("annotation setting", "Pixel size"),
                    (tr!("Decrease pixel size"), tr!("Increase pixel size")),
                    v,
                    (6, 32),
                    ParamValue::Block,
                    cx,
                ),
                _ => return None,
            };
        Some(
            div()
                .flex()
                .flex_col()
                .gap(px(8.))
                .child(section_label(t, &p.label()))
                .child(body)
                .into_any_element(),
        )
    }

    fn segments(
        &self,
        t: &Theme,
        id: &'static str,
        options: impl Iterator<Item = (f64, String, String)>,
        current: f64,
        make: fn(f64) -> ParamValue,
    ) -> AnyElement {
        div()
            .flex()
            .gap(px(6.))
            .children(options.enumerate().map(|(i, (v, text, label))| {
                self.seg_button(
                    t,
                    ElementId::from((id, i)),
                    text,
                    label,
                    near(v, current),
                    self.set_param_action(make(v)),
                )
            }))
            .into_any_element()
    }

    fn inspector(&self, t: &Theme, s: &Snap, cx: &mut Context<Self>) -> AnyElement {
        let title = s.inspector_title.clone();
        let tool_icon = s.tool.icon();
        let subtitle = if s.selected {
            tr!("Changes apply to the selected annotation.")
        } else if s.tool == Tool::Select {
            tr!("Pick a tool, or click an annotation to edit it.")
        } else {
            tr!("Settings for new annotations.")
        };
        let params: Vec<AnyElement> = s
            .params
            .iter()
            .filter_map(|(p, v)| self.param_group(t, *p, *v, cx))
            .collect();
        let selected = s.selected && !s.editing_text;
        let has_crop = s.has_crop;
        let (iw, ih) = s.image;
        let pointer = s
            .hover
            .map(|p| format!("{:.0}, {:.0}", p.x.round(), p.y.round()))
            .unwrap_or_else(|| "-".into());
        let m = mod_label().to_owned();

        let info_row = |label: String, value: String| {
            div()
                .flex()
                .items_center()
                .justify_between()
                .text_size(px(13.))
                .child(
                    div()
                        .min_w(px(0.))
                        .text_color(rgba_of(t.text_3))
                        .child(label),
                )
                .child(
                    div()
                        .text_color(rgba_of(t.text))
                        .mono()
                        .tnum()
                        .child(SharedString::from(value)),
                )
        };
        let col = || card(t).flex().flex_col().gap(px(12.)).flex_none();

        let status_card = self.statusbar(t, s);
        div()
            .id("editor-inspector")
            .role(Role::Group)
            .aria_label(tr!("Inspector"))
            .w(px(INSPECTOR_W))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(
                div()
                    .id("editor-inspector-scroll")
                    .flex_1()
                    .min_h(px(0.))
                    .flex()
                    .flex_col()
                    .gap(px(14.))
                    .overflow_y_scroll()
                    .child(
                        col()
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(4.))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(8.))
                                            .child(icon(tool_icon, 18., t.text))
                                            .child(
                                                div()
                                                    .id("inspector-title")
                                                    .min_w(px(0.))
                                                    .role(Role::Heading)
                                                    .aria_level(2)
                                                    .aria_label(SharedString::from(title.clone()))
                                                    .text_size(px(type_scale::BODY))
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .child(SharedString::from(title)),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(type_scale::CAPTION))
                                            .text_color(rgba_of(t.mist))
                                            .child(subtitle),
                                    ),
                            )
                            .children(params),
                    )
                    .when(
                        s.redact_available
                            && (!s.proposals.is_empty() || s.redact != RedactPhase::Idle),
                        |d| d.child(self.redact_card(t, s)),
                    )
                    .when(s.tool == Tool::Crop, |d| {
                        d.child(
                            col()
                                .child(section_label(t, &tr_ctx!("tool", "Crop")))
                                .child(
                                    div()
                                        .text_size(px(13.))
                                        .text_color(rgba_of(t.text_3))
                                        .child(tr!(
                                    "Drag on the image to set the visible area. The original and \
                                     the annotations outside it are kept."
                                )),
                                )
                                .when(has_crop, |d| {
                                    d.child(
                                        button(
                                            t,
                                            "crop-reset",
                                            tr!("Reset crop"),
                                            ButtonKind::Secondary,
                                        )
                                        .on_click(self.doc_action(|d, cx| d.clear_crop(cx))),
                                    )
                                }),
                        )
                    })
                    .when(selected, |d| {
                        d.child(
                            col().child(section_label(t, &tr!("Annotation"))).child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .gap(px(8.))
                                    .child(
                                        button_with_icon(
                                            t,
                                            "ann-duplicate",
                                            tr!("Duplicate"),
                                            ButtonKind::Secondary,
                                            Some("copy"),
                                        )
                                        .on_click(self.doc_action(|d, cx| d.duplicate(cx)))
                                        .child(keycaps(t, &[m.clone(), "D".into()])),
                                    )
                                    .child(
                                        // An undoable immediate delete, not a confirmation: neutral.
                                        button_with_icon(
                                            t,
                                            "ann-delete",
                                            tr_ctx!("button", "Delete"),
                                            ButtonKind::Secondary,
                                            Some("trash-2"),
                                        )
                                        .on_click(self.doc_action(|d, cx| d.delete_selected(cx)))
                                        .child(keycaps(t, &["Del".to_owned()])),
                                    ),
                            ),
                        )
                    })
                    .child(
                        div()
                            .id("inspector-image-info")
                            .role(Role::Group)
                            .aria_label(SharedString::from(tr!(
                                "Image {iw} by {ih} pixels. Pointer {pointer}",
                                iw = iw,
                                ih = ih,
                                pointer = pointer
                            )))
                            .child(
                                col()
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(8.))
                                            .child(icon("image", 16., t.mist))
                                            .child(section_label(
                                                t,
                                                &tr_ctx!("inspector", "Image"),
                                            )),
                                    )
                                    .child(info_row(
                                        tr_ctx!("image info", "Size"),
                                        format!("{iw} \u{D7} {ih} px"),
                                    ))
                                    .child(info_row(tr_ctx!("image info", "Pointer"), pointer)),
                            ),
                    ),
            )
            .child(status_card)
            .into_any_element()
    }

    /// Auto-redact proposals (ANN-2.AC4): what was found, Accept all / Accept one / Dismiss.
    /// Nothing is blurred until the user accepts.
    fn redact_card(&self, t: &Theme, s: &Snap) -> AnyElement {
        let note = match &s.redact {
            RedactPhase::Running => tr!("Looking for emails and keys\u{2026}"),
            RedactPhase::NothingFound => tr!("No emails or keys found in this image."),
            RedactPhase::Failed(e) => tr!("Couldn\u{2019}t look for text: {e}", e = e.text()),
            RedactPhase::Idle => trn!(
                "{n} suggested. Nothing is blurred until you accept. Click a dashed box on the \
                 image to accept just that one.",
                "{n} suggested. Nothing is blurred until you accept. Click a dashed box on the \
                 image to accept just that one.",
                s.proposals.len()
            ),
        };
        let rows: Vec<AnyElement> = s
            .proposals
            .iter()
            .enumerate()
            .map(|(i, (kind, preview, conf))| {
                let label = tr!(
                    "{kind} {preview}, confidence {percent} percent",
                    kind = kind,
                    preview = preview,
                    percent = format!("{:.0}", conf * 100.0)
                );
                div()
                    .id(("redact-row", i))
                    .role(Role::Group)
                    .aria_label(SharedString::from(label))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.))
                    .child(
                        div()
                            .min_w(px(0.))
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .text_color(rgba_of(t.text))
                                    .child(kind.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(type_scale::CAPTION))
                                    .text_color(rgba_of(t.text_3))
                                    .mono()
                                    .child(SharedString::from(preview.clone())),
                            ),
                    )
                    .child(
                        button(
                            t,
                            ("redact-accept", i),
                            tr_ctx!("button", "Accept"),
                            ButtonKind::Secondary,
                        )
                        .on_click(self.doc_action(move |d, cx| d.accept_proposal(Some(i), cx))),
                    )
                    .into_any_element()
            })
            .collect();
        let has = !s.proposals.is_empty();
        card(t)
            .id("editor-redact")
            .role(Role::Group)
            .aria_label(tr!("Auto-redact suggestions"))
            .flex()
            .flex_col()
            .gap(px(12.))
            .flex_none()
            .child(section_label(t, &tr!("Auto-redact")))
            .child(
                div()
                    .id("redact-note")
                    .role(Role::Status)
                    .aria_label(SharedString::from(note.clone()))
                    .text_size(px(13.))
                    .text_color(rgba_of(t.text_3))
                    .child(SharedString::from(note)),
            )
            .children(rows)
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(8.))
                    .when(has, |d| {
                        d.child(
                            button(
                                t,
                                "redact-accept-all",
                                tr!("Accept all"),
                                ButtonKind::Primary,
                            )
                            .on_click(self.doc_action(|d, cx| d.accept_proposal(None, cx))),
                        )
                    })
                    .child(
                        button(
                            t,
                            "redact-dismiss",
                            if has {
                                tr_ctx!("button", "Dismiss")
                            } else {
                                tr_ctx!("button", "Close")
                            },
                            ButtonKind::Secondary,
                        )
                        .on_click(self.doc_action(|d, cx| d.dismiss_redactions(cx))),
                    ),
            )
            .into_any_element()
    }

    /// The save-status card pinned to the bottom of the inspector: the status pill with its
    /// icon (saved, saving, failed) and the selection summary under it.
    fn statusbar(&self, t: &Theme, s: &Snap) -> AnyElement {
        let (text, kind) = match &s.status {
            Status::Saved => (tr!("All changes saved"), StatusKind::Ok),
            Status::Saving => (tr!("Saving\u{2026}"), StatusKind::Pending),
            Status::Failed(e) => (tr!("Couldn\u{2019}t save: {e}", e = e), StatusKind::Warning),
        };
        let summary = s.summary.clone();
        card(t)
            .id("editor-statusbar")
            .role(Role::Status)
            .aria_label(SharedString::from(format!("{text}. {summary}")))
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(
                div()
                    .flex()
                    .child(status(t, kind, SharedString::from(text))),
            )
            .child(
                div()
                    .min_w(px(0.))
                    .text_size(px(type_scale::CAPTION))
                    .text_color(rgba_of(t.text_3))
                    .mono()
                    .child(SharedString::from(summary)),
            )
            .into_any_element()
    }
}

// ---- overlay painting -------------------------------------------------------------------

/// Dot-grid backdrop of the canvas (board R4): small `line` squares on a 24 px lattice, painted
/// under the image. Capped so a huge window never paints more than a few thousand quads.
fn dot_grid(t: &Theme) -> impl IntoElement {
    let color = hsla(t.line);
    canvas(
        |_, _, _| (),
        move |bounds, (), window, _cx| {
            let step = DOT_STEP;
            let cols = (f32::from(bounds.size.width) / step).ceil() as usize + 1;
            let rows = (f32::from(bounds.size.height) / step).ceil() as usize + 1;
            if cols * rows > 12_000 {
                return;
            }
            for r in 0..rows {
                for c in 0..cols {
                    let x = bounds.origin.x + px(c as f32 * step + step / 2.0 - 1.0);
                    let y = bounds.origin.y + px(r as f32 * step + step / 2.0 - 1.0);
                    window.paint_quad(gpui::fill(
                        Bounds {
                            origin: point(x, y),
                            size: size(px(2.), px(2.)),
                        },
                        color,
                    ));
                }
            }
        },
    )
    .absolute()
    .size_full()
}

const DOT_STEP: f32 = 24.;

fn hsla(c: crate::theme::Color) -> Hsla {
    Hsla::from(rgba_of(c))
}

fn paint_overlay(
    bounds: Bounds<Pixels>,
    o: &OverlayData,
    vp: Viewport,
    t: &Theme,
    window: &mut Window,
) {
    let mut vp = vp;
    let (w, h) = (
        f64::from(f32::from(bounds.size.width)),
        f64::from(f32::from(bounds.size.height)),
    );
    if w > 8.0 && h > 8.0 {
        vp.set_canvas((w, h));
    }
    let origin = bounds.origin;
    let z = vp.zoom();
    let pt = |p: Pt| -> gpui::Point<Pixels> {
        let (x, y) = vp.to_screen(p);
        point(origin.x + px(x as f32), origin.y + px(y as f32))
    };
    let rect_b = |r: Rc| -> Bounds<Pixels> {
        let r = r.normalized();
        let a = pt(Pt::new(r.x, r.y));
        let b = pt(Pt::new(r.x + r.w, r.y + r.h));
        Bounds {
            origin: a,
            size: size(b.x - a.x, b.y - a.y),
        }
    };
    window.with_content_mask(Some(ContentMask { bounds }), |window| {
        // Redactions whose render is still on its way: opaque, so no pixel shows (ANN-2).
        for r in &o.masks {
            window.paint_quad(gpui::fill(rect_b(*r), hsla(t.raised)));
        }
        // Auto-redact proposals: dashed outlines over the still-visible text (ANN-2.AC4).
        for r in &o.proposals {
            let b = rect_b(*r);
            let mut tint = hsla(accent(t));
            tint.a = 0.14;
            window.paint_quad(gpui::fill(b, tint));
            window.paint_quad(gpui::quad(
                b,
                0.,
                gpui::transparent_black(),
                1.5,
                hsla(accent(t)),
                BorderStyle::Dashed,
            ));
        }
        // Crop: dim outside the crop rect.
        if let Some(c) = o.crop {
            let full = rect_b(Rc::new(
                vp.content.x,
                vp.content.y,
                vp.content.w,
                vp.content.h,
            ));
            let cb = rect_b(c);
            let dim = hsla(t.scrim);
            let (fx0, fy0) = (full.origin.x, full.origin.y);
            let (fx1, fy1) = (fx0 + full.size.width, fy0 + full.size.height);
            let (cx0, cy0) = (cb.origin.x, cb.origin.y);
            let (cx1, cy1) = (cx0 + cb.size.width, cy0 + cb.size.height);
            let strips = [
                (fx0, fy0, fx1, cy0),
                (fx0, cy1, fx1, fy1),
                (fx0, cy0, cx0, cy1),
                (cx1, cy0, fx1, cy1),
            ];
            for (x0, y0, x1, y1) in strips {
                if x1 > x0 && y1 > y0 {
                    window.paint_quad(gpui::fill(
                        Bounds {
                            origin: point(x0, y0),
                            size: size(x1 - x0, y1 - y0),
                        },
                        dim,
                    ));
                }
            }
            window.paint_quad(gpui::quad(
                cb,
                0.,
                gpui::transparent_black(),
                1.5,
                gpui::white(),
                BorderStyle::Solid,
            ));
        }
        // The shape being dragged out (outline only; redactions get an opaque mask).
        if let Some(d) = &o.draft {
            paint_draft(d, &pt, &rect_b, z, t, window);
        }
        // Selection box and handles.
        if let Some(ch) = &o.chrome {
            let is_line = ch
                .handles
                .iter()
                .any(|(h, _)| matches!(h, Handle::From | Handle::To));
            if !is_line {
                window.paint_quad(gpui::quad(
                    rect_b(ch.bounds),
                    0.,
                    gpui::transparent_black(),
                    1.,
                    hsla(t.text),
                    BorderStyle::Solid,
                ));
            }
            for (_, p) in &ch.handles {
                let c = pt(*p);
                let half = px(HANDLE / 2.0);
                window.paint_quad(gpui::quad(
                    Bounds {
                        origin: point(c.x - half, c.y - half),
                        size: size(px(HANDLE), px(HANDLE)),
                    },
                    px(2.),
                    gpui::white(),
                    1.5,
                    hsla(t.text),
                    BorderStyle::Solid,
                ));
            }
        }
        // Text frame and caret.
        if let Some(tg) = &o.text {
            window.paint_quad(gpui::quad(
                rect_b(tg.frame),
                0.,
                gpui::transparent_black(),
                1.,
                hsla(t.text),
                BorderStyle::Dashed,
            ));
            let c = pt(tg.caret);
            let ch = px((tg.caret_h * z) as f32);
            window.paint_quad(gpui::fill(
                Bounds {
                    origin: c,
                    size: size(px(2.), ch),
                },
                hsla(t.text),
            ));
        }
    });
}

fn stroke_line(
    window: &mut Window,
    pts: &[gpui::Point<Pixels>],
    width: f32,
    color: Hsla,
    closed: bool,
) {
    if pts.len() < 2 {
        return;
    }
    let mut pb = PathBuilder::stroke(px(width.max(1.5)));
    if closed {
        pb.add_polygon(pts, true);
    } else {
        pb.move_to(pts[0]);
        for p in &pts[1..] {
            pb.line_to(*p);
        }
    }
    if let Ok(path) = pb.build() {
        window.paint_path(path, color);
    }
}

fn paint_draft(
    d: &OpKind,
    pt: &dyn Fn(Pt) -> gpui::Point<Pixels>,
    rect_b: &dyn Fn(Rc) -> Bounds<Pixels>,
    z: f64,
    t: &Theme,
    window: &mut Window,
) {
    match d {
        OpKind::Arrow {
            from,
            to,
            color,
            width,
        }
        | OpKind::Line {
            from,
            to,
            color,
            width,
        } => {
            let c = Hsla::from(gpui_rgb(*color));
            let w = (*width * z) as f32;
            stroke_line(window, &[pt(*from), pt(*to)], w, c, false);
            if matches!(d, OpKind::Arrow { .. }) {
                let ang = (to.y - from.y).atan2(to.x - from.x);
                let len = (width * 4.0).max(14.0);
                for side in [-1.0_f64, 1.0] {
                    let a = ang + std::f64::consts::PI + side * 0.5;
                    let tip = Pt::new(to.x + len * a.cos(), to.y + len * a.sin());
                    stroke_line(window, &[pt(*to), pt(tip)], w, c, false);
                }
            }
        }
        OpKind::Rect {
            rect, color, width, ..
        } => {
            window.paint_quad(gpui::quad(
                rect_b(*rect),
                0.,
                gpui::transparent_black(),
                px(((*width * z) as f32).max(1.5)),
                Hsla::from(gpui_rgb(*color)),
                BorderStyle::Solid,
            ));
        }
        OpKind::Ellipse { rect, color, width } => {
            let r = rect.normalized();
            let (cx, cy, rx, ry) = (r.x + r.w / 2.0, r.y + r.h / 2.0, r.w / 2.0, r.h / 2.0);
            let pts: Vec<_> = (0..64)
                .map(|i| {
                    let a = f64::from(i) / 64.0 * std::f64::consts::TAU;
                    pt(Pt::new(cx + rx * a.cos(), cy + ry * a.sin()))
                })
                .collect();
            stroke_line(
                window,
                &pts,
                (*width * z) as f32,
                Hsla::from(gpui_rgb(*color)),
                true,
            );
        }
        OpKind::Highlight {
            rect,
            color,
            opacity,
        } => {
            let mut c = Hsla::from(gpui_rgb(*color));
            c.a = *opacity as f32;
            window.paint_quad(gpui::fill(rect_b(*rect), c));
        }
        OpKind::Blur { rect, .. } | OpKind::Pixelate { rect, .. } => {
            // Opaque: the pixels under a redaction are hidden from the first frame of the
            // drag, before the real renderer has produced the effect.
            let b = rect_b(*rect);
            window.paint_quad(gpui::fill(b, hsla(t.raised)));
            window.paint_quad(gpui::quad(
                b,
                0.,
                gpui::transparent_black(),
                1.,
                hsla(t.text),
                BorderStyle::Dashed,
            ));
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Redesign R4: the tool pill lists the 11 tools in the shipped order, with dividers before
    /// Arrow, Blur and Crop only.
    #[test]
    fn tool_pill_order_and_groups() {
        let ids: Vec<&str> = Tool::ALL.iter().map(|t| t.id()).collect();
        assert_eq!(
            ids,
            [
                "select",
                "arrow",
                "line",
                "rect",
                "ellipse",
                "text",
                "highlight",
                "step",
                "blur",
                "pixelate",
                "crop"
            ]
        );
        let starts: Vec<&str> = Tool::ALL
            .iter()
            .filter(|t| tool_group_starts(**t))
            .map(|t| t.id())
            .collect();
        assert_eq!(starts, ["arrow", "blur", "crop"]);
    }

    /// The fit insets are exactly the pills' footprint plus their margins, so the fitted image
    /// never sits under either pill, and the canvas itself keeps the full stage rect.
    #[test]
    fn chrome_insets_match_the_pills() {
        assert_eq!(
            CHROME_INSETS.left,
            f64::from(PILL_MARGIN * 2.0 + TOOL_RAIL_W)
        );
        assert_eq!(
            CHROME_INSETS.bottom,
            f64::from(PILL_MARGIN * 2.0 + TRANSPORT_H)
        );
        assert_eq!((CHROME_INSETS.top, CHROME_INSETS.right), (0.0, 0.0));
        let mut v = Viewport::new(Rc::new(0.0, 0.0, 3024.0, 1964.0), (900.0, 600.0), 2.0);
        v.set_insets(CHROME_INSETS);
        v.fit_view();
        let (x, y, w, h) = v.screen_rect();
        assert!(x >= CHROME_INSETS.left && y + h <= 600.0 - CHROME_INSETS.bottom);
        assert!(x + w <= 900.0 && y >= 0.0);
    }

    #[test]
    fn reveal_label_is_platform_neutral() {
        let l = EditorView::reveal_label();
        assert!(l == "Reveal in Finder" || l == "Show in folder");
        assert_eq!(l == "Reveal in Finder", cfg!(target_os = "macos"));
    }
}
