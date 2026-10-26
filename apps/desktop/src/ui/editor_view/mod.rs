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

mod canvas;
mod chrome;
mod input;
mod inspector;

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
