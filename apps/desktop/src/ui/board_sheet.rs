//! The Make a board sheet (BRD-1, design board S10), drawn inside the Studio window. State and
//! words: `crate::board_flow`; **Finish board** calls `create_board` as the user. The layout lives
//! only in this sheet until Finish, so it stays editable; after Finish the board is an ordinary
//! screenshot capture. Not checked by eye in this build (no display): the state is unit tested in
//! `board_flow`.
//!
//! The canvas is a schematic of the board at its real proportions (the plan the renderer will
//! use, one box per picture with its title); the real picture is made at Finish. In the Free
//! layout the pictures are dragged on the canvas (move, and resize by a corner) with pink snap
//! guides and a pink gap measure while dragging (design board S10); the geometry is in
//! `crate::board_flow` (`snap_move`, `snap_resize`, `gap_measure`).

use cm_i18n::{tr, trn};
use cm_render::compose_board::LayoutKind;
use gpui::{
    div, img, prelude::*, px, AnyElement, App, Context, CursorStyle, Entity, FocusHandle,
    FontWeight, KeyDownEvent, MouseButton, ObjectFit, Render, RenderImage, Role, SharedString,
    Subscription, Window,
};
use serde_json::{json, Value};
use std::sync::Arc;

use crate::backend::Backend;
use crate::board_flow::{layout_label, BgChoice, BoardSheet, Grip, LAYOUTS};
use crate::daemon_text::call_error_text;
use crate::theme::{scale, Theme};
use crate::ui::app::{self, AppModel};
use crate::ui::bulk_sheet::seg;
use crate::ui::components::{button_with_icon, icon, switch, ButtonKind};
use crate::ui::paint::{rgba_of, TextExt};
use crate::ui::studio::ThemeGlobal;
use crate::ui::text_field::{TextField, TextFieldEvent};
use crate::ui::thumbs::{Thumb, ThumbCache};

/// The widest and tallest the sheet gets; it shrinks with the window (see `Geometry`).
const SHEET_MAX_W: f32 = 1180.;
const SHEET_MAX_H: f32 = 820.;
/// Space kept free around the sheet in the window.
const SHEET_MARGIN: f32 = 24.;
const SHEET_PAD: f32 = 24.;
const COL_GAP: f32 = 16.;
const HEADER_H: f32 = 64.;
const FOOTER_H: f32 = 66.;
/// What sits under the canvas (the size chip and the hints).
const UNDER_CANVAS_H: f32 = 170.;

/// The sheet's sizes for a window: the panel, the three columns and the canvas box. Pure, so the
/// responsive rules are unit tested.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Geometry {
    pub sheet_w: f32,
    pub sheet_h: f32,
    pub left_w: f32,
    pub inspector_w: f32,
    pub canvas_w: f32,
    pub canvas_h: f32,
}

impl Geometry {
    pub(crate) fn for_window(win_w: f32, win_h: f32) -> Geometry {
        let sheet_w = (win_w - 2. * SHEET_MARGIN).clamp(640., SHEET_MAX_W);
        let sheet_h = (win_h - 2. * SHEET_MARGIN).clamp(420., SHEET_MAX_H);
        let narrow = sheet_w < 900.;
        let (left_w, inspector_w) = if narrow { (180., 240.) } else { (220., 280.) };
        let body_w = sheet_w - 2. * SHEET_PAD;
        let body_h = sheet_h - HEADER_H - FOOTER_H;
        Geometry {
            sheet_w,
            sheet_h,
            left_w,
            inspector_w,
            canvas_w: (body_w - left_w - inspector_w - 2. * COL_GAP).max(160.),
            canvas_h: (body_h - UNDER_CANVAS_H).max(160.),
        }
    }
}

pub struct BoardSheetView {
    app: Entity<AppModel>,
    thumbs: Entity<ThumbCache>,
    preview: PreviewCache,
    title_field: Entity<TextField>,
    tint_field: Entity<TextField>,
    seen_epoch: u64,
    focus: FocusHandle,
    focused: bool,
    _subs: Vec<Subscription>,
}

impl BoardSheetView {
    pub fn new(app: Entity<AppModel>, cx: &mut Context<Self>) -> Self {
        let observe = cx.observe(&app, |_, _, cx| cx.notify());
        let backend = app.read(cx).backend.clone();
        let thumbs = cx.new(|_| ThumbCache::new(backend));
        let thumbs_observe = cx.observe(&thumbs, |_, _, cx| cx.notify());
        let title_field =
            cx.new(|cx| TextField::new(tr!("Board title"), tr!("Board title"), cx).without_icon());
        let tint_field =
            cx.new(|cx| TextField::new(tr!("Tint colour"), "#DCE6F5", cx).without_icon());
        let subs = vec![
            observe,
            thumbs_observe,
            cx.subscribe(&title_field, |this, _, ev: &TextFieldEvent, cx| {
                this.on_field(ev, cx, |s, t| s.title = t.to_owned());
            }),
            cx.subscribe(&tint_field, |this, _, ev: &TextFieldEvent, cx| {
                this.on_field(ev, cx, |s, t| s.tint = t.to_owned());
            }),
        ];
        BoardSheetView {
            app,
            thumbs,
            preview: PreviewCache::default(),
            title_field,
            tint_field,
            seen_epoch: u64::MAX,
            focus: cx.focus_handle(),
            focused: false,
            _subs: subs,
        }
    }

    fn on_field(
        &mut self,
        ev: &TextFieldEvent,
        cx: &mut Context<Self>,
        set: fn(&mut BoardSheet, &str),
    ) {
        match ev {
            TextFieldEvent::Changed(text) => {
                let text = text.clone();
                self.app.update(cx, |m, cx| {
                    if let Some(s) = m.bulk.board.as_mut() {
                        set(s, &text);
                    }
                    cx.notify();
                });
            }
            TextFieldEvent::Submitted => finish(&self.app, cx),
            TextFieldEvent::Escaped => close(&self.app, cx),
        }
    }

    fn sync_fields(&mut self, cx: &mut Context<Self>) {
        let (epoch, vals) = {
            let m = self.app.read(cx);
            (
                m.bulk.epoch,
                m.bulk
                    .board
                    .as_ref()
                    .map(|s| (s.title.clone(), s.tint.clone())),
            )
        };
        if epoch == self.seen_epoch {
            return;
        }
        self.seen_epoch = epoch;
        if let Some((title, tint)) = vals {
            self.title_field.update(cx, |f, cx| f.set_text(&title, cx));
            self.tint_field.update(cx, |f, cx| f.set_text(&tint, cx));
        }
    }
}

fn update(app: &Entity<AppModel>, cx: &mut App, f: impl FnOnce(&mut BoardSheet)) {
    app.update(cx, |m, cx| {
        if let Some(s) = m.bulk.board.as_mut() {
            f(s);
        }
        cx.notify();
    });
}

fn row() -> gpui::Div {
    div().flex().flex_wrap().items_center().gap(px(8.))
}

/// The board picture drawn by the export's renderer from the library thumbnails (BR-01), kept
/// until the setup, the sizes or the set of loaded thumbnails changes.
#[derive(Default)]
pub struct PreviewCache {
    key: String,
    image: Option<Arc<RenderImage>>,
}

impl PreviewCache {
    fn image(
        &mut self,
        s: &BoardSheet,
        thumbs: &Entity<ThumbCache>,
        max_edge: u32,
        cx: &mut App,
    ) -> Option<Arc<RenderImage>> {
        let opts = s.options().ok()?;
        let layout = s.layout_json();
        let loaded: Vec<Option<Arc<RenderImage>>> = s
            .sources
            .iter()
            .map(|src| match thumbs.update(cx, |c, cx| c.get(&src.id, cx)) {
                Thumb::Ready(i) => Some(i),
                _ => None,
            })
            .collect();
        let sizes: Vec<(u32, u32)> = s
            .sources
            .iter()
            .map(|x| (x.width.max(1), x.height.max(1)))
            .collect();
        let key = format!(
            "{layout:?}|{opts:?}|{sizes:?}|{max_edge}|{:?}",
            loaded
                .iter()
                .map(|i| i.as_ref().map(|i| i.id))
                .collect::<Vec<_>>()
        );
        if key == self.key {
            return self.image.clone();
        }
        self.key = key;
        self.image = render_preview_image(&sizes, &loaded, &layout, &opts, max_edge);
        self.image.clone()
    }
}

/// Draws the preview. Thumbnails not loaded yet are drawn as a flat tile of the same shape.
fn render_preview_image(
    sizes: &[(u32, u32)],
    loaded: &[Option<Arc<RenderImage>>],
    layout: &cm_render::compose_board::BoardLayout,
    opts: &cm_render::compose_board::BoardOptions,
    max_edge: u32,
) -> Option<Arc<RenderImage>> {
    let thumbs: Vec<image::RgbaImage> = sizes
        .iter()
        .zip(loaded)
        .map(|(&(w, h), t)| {
            t.as_ref()
                .and_then(|i| {
                    let sz = i.size(0);
                    let bytes = i.as_bytes(0)?;
                    // GPUI keeps BGRA; the renderer wants RGBA.
                    let mut raw = bytes.to_vec();
                    for p in raw.as_chunks_mut::<4>().0.iter_mut() {
                        p.swap(0, 2);
                    }
                    image::RgbaImage::from_raw(u32::from(sz.width), u32::from(sz.height), raw)
                })
                .unwrap_or_else(|| {
                    let k = (160.0 / w.max(h) as f32).min(1.0);
                    image::RgbaImage::from_pixel(
                        ((w as f32 * k) as u32).max(1),
                        ((h as f32 * k) as u32).max(1),
                        image::Rgba([0x3A, 0x3C, 0x42, 0xFF]),
                    )
                })
        })
        .collect();
    let (mut rgba, _) =
        cm_render::compose_board::render_preview(sizes, &thumbs, layout, opts, max_edge).ok()?;
    for p in rgba.as_chunks_mut::<4>().0.iter_mut() {
        p.swap(0, 2);
    }
    Some(Arc::new(RenderImage::new([image::Frame::new(rgba)])))
}

/// The canvas: the board at its real proportions, a box per picture.
fn canvas(
    t: &Theme,
    s: &BoardSheet,
    app: &Entity<AppModel>,
    thumbs: &Entity<ThumbCache>,
    preview: &mut PreviewCache,
    (canvas_w, canvas_h): (f32, f32),
    cx: &mut App,
) -> AnyElement {
    let frame = div()
        .w(px(canvas_w))
        .h(px(canvas_h))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(scale::RADIUS_LG))
        .border_1()
        .border_color(rgba_of(t.border))
        .bg(rgba_of(t.surface));
    let Ok(plan) = s.plan() else {
        return frame
            .child(
                div()
                    .text_size(px(13.))
                    .text_color(rgba_of(t.warning))
                    .child(SharedString::from(s.plan().err().unwrap_or_default())),
            )
            .into_any_element();
    };
    let k = (canvas_w - 40.) / plan.width as f32;
    let k = k.min((canvas_h - 40.) / plan.height as f32);
    let (w, h) = (plan.width as f32 * k, plan.height as f32 * k);
    let bg = bg_color(t, s.background, &s.tint);
    let free = s.layout == LayoutKind::Free;
    // Window pixels per slot pixel (a board too large for its limits is scaled down by the plan).
    let kd = k * plan.scale as f32;
    let mut board = div()
        .relative()
        .w(px(w))
        .h(px(h))
        .flex_none()
        .overflow_hidden()
        .bg(rgba_of(bg));
    if free {
        let (a_move, a_up, a_out) = (app.clone(), app.clone(), app.clone());
        board = board
            .on_mouse_move(move |ev, _, cx| {
                if ev.pressed_button != Some(MouseButton::Left) {
                    // The button was let go outside the window: finish the drag.
                    update(&a_move, cx, |s| s.end_drag());
                    return;
                }
                let at = (f32::from(ev.position.x), f32::from(ev.position.y));
                let (alt, shift) = (ev.modifiers.alt, ev.modifiers.shift);
                update(&a_move, cx, |s| s.drag_to(at, kd, alt, shift));
            })
            .on_mouse_up(MouseButton::Left, move |_, _, cx| {
                update(&a_up, cx, |s| s.end_drag())
            })
            .on_mouse_up_out(MouseButton::Left, move |_, _, cx| {
                update(&a_out, cx, |s| s.end_drag())
            });
    }
    // The picture is the export's own drawing at a smaller scale (BR-01): margins, radius,
    // shadow, stamps, captions and title are the renderer's, not a second drawing here.
    let max_edge = ((canvas_w.max(canvas_h) * 2.) as u32).clamp(256, 1800);
    if let Some(image) = preview.image(s, thumbs, max_edge, cx) {
        board = board.child(
            img(image)
                .absolute()
                .left(px(0.))
                .top(px(0.))
                .w(px(w))
                .h(px(h))
                .object_fit(ObjectFit::Fill),
        );
    }
    if free {
        // An invisible grab area per picture; the picture itself is in the image above.
        for (i, cell) in plan.cells.iter().enumerate() {
            let a = app.clone();
            board = board.child(
                div()
                    .absolute()
                    .left(px(cell.x as f32 * k))
                    .top(px(cell.y as f32 * k))
                    .w(px(cell.w as f32 * k))
                    .h(px(cell.h as f32 * k))
                    .cursor(CursorStyle::OpenHand)
                    .on_mouse_down(MouseButton::Left, move |ev, _, cx| {
                        let at = (f32::from(ev.position.x), f32::from(ev.position.y));
                        update(&a, cx, |s| s.begin_drag(i, Grip::Move, at));
                    }),
            );
        }
    }
    if free {
        // Corner handles on every picture (a corner drag resizes, keeping the proportions).
        for (i, cell) in plan.cells.iter().enumerate() {
            let (x0, y0) = (cell.x as f32 * k, cell.y as f32 * k);
            let (x1, y1) = (x0 + cell.w as f32 * k, y0 + cell.h as f32 * k);
            for (grip, cx_, cy_, cursor) in [
                (Grip::TopLeft, x0, y0, CursorStyle::ResizeUpLeftDownRight),
                (Grip::TopRight, x1, y0, CursorStyle::ResizeUpRightDownLeft),
                (Grip::BottomLeft, x0, y1, CursorStyle::ResizeUpRightDownLeft),
                (
                    Grip::BottomRight,
                    x1,
                    y1,
                    CursorStyle::ResizeUpLeftDownRight,
                ),
            ] {
                let a = app.clone();
                board = board.child(
                    div()
                        .absolute()
                        .left(px(cx_ - 5.))
                        .top(px(cy_ - 5.))
                        .w(px(10.))
                        .h(px(10.))
                        .rounded(px(2.))
                        .border_1()
                        .border_color(rgba_of(t.agent))
                        .bg(rgba_of(crate::theme::Color::rgb(0xFFFFFF)))
                        .cursor(cursor)
                        .on_mouse_down(MouseButton::Left, move |ev, _, cx| {
                            cx.stop_propagation();
                            let at = (f32::from(ev.position.x), f32::from(ev.position.y));
                            update(&a, cx, |s| s.begin_drag(i, grip, at));
                        }),
                );
            }
        }
        // Pink snap guides and the gap measure, only while dragging.
        let pink = crate::theme::Color::rgb(0xF472B6);
        for g in &s.guides {
            let at = g.at as f32 * kd;
            board = board.child(
                if g.vertical {
                    div().absolute().left(px(at)).top(px(0.)).w(px(1.)).h_full()
                } else {
                    div().absolute().top(px(at)).left(px(0.)).h(px(1.)).w_full()
                }
                .bg(rgba_of(pink)),
            );
        }
        if let Some(m) = s.measure.filter(|_| s.drag.is_some()) {
            let (x0, y0) = (m.from.0 as f32 * kd, m.from.1 as f32 * kd);
            let (x1, y1) = (m.to.0 as f32 * kd, m.to.1 as f32 * kd);
            let line = if (y1 - y0).abs() < 0.5 {
                div()
                    .absolute()
                    .left(px(x0.min(x1)))
                    .top(px(y0))
                    .w(px((x1 - x0).abs()))
                    .h(px(1.))
            } else {
                div()
                    .absolute()
                    .left(px(x0))
                    .top(px(y0.min(y1)))
                    .w(px(1.))
                    .h(px((y1 - y0).abs()))
            };
            board = board.child(line.bg(rgba_of(pink))).child(
                div()
                    .absolute()
                    .left(px((x0 + x1) / 2. + 3.))
                    .top(px((y0 + y1) / 2. + 3.))
                    .px(px(5.))
                    .rounded(px(4.))
                    .bg(rgba_of(pink))
                    .text_size(px(10.))
                    .mono()
                    .text_color(rgba_of(crate::theme::Color::rgb(0x18191C)))
                    .child(SharedString::from(format!("{} px", m.px))),
            );
        }
    }
    frame.child(board).into_any_element()
}

fn bg_color(t: &Theme, b: BgChoice, tint: &str) -> crate::theme::Color {
    match b {
        BgChoice::Transparent => t.field,
        BgChoice::White => crate::theme::Color::rgb(0xFFFFFF),
        BgChoice::LightGrey => crate::theme::Color::rgb(0xECEDF0),
        BgChoice::Dark => crate::theme::Color::rgb(0x18191C),
        BgChoice::Tint => parse_tint(tint).unwrap_or(crate::theme::Color::rgb(0xDCE6F5)),
    }
}

fn parse_tint(s: &str) -> Option<crate::theme::Color> {
    let h = s.trim().strip_prefix('#')?;
    if h.len() != 6 {
        return None;
    }
    let n = u32::from_str_radix(h, 16).ok()?;
    Some(crate::theme::Color::rgb(n))
}

/// A segmented row of number choices that always stays on one line.
fn preset(
    t: &Theme,
    app: &Entity<AppModel>,
    id: &'static str,
    values: &[u32],
    current: u32,
    set: fn(&mut BoardSheet, u32),
) -> gpui::Div {
    row()
        .flex_nowrap()
        .gap(px(4.))
        .children(values.iter().map(|&v| {
            let a = app.clone();
            seg(
                t,
                SharedString::from(format!("{id}-{v}")),
                v.to_string(),
                current == v,
            )
            .flex_1()
            .min_w(px(0.))
            .justify_center()
            .px(px(2.))
            .on_click(move |_, _, cx| update(&a, cx, |s| set(s, v)))
        }))
}

fn field_label(t: &Theme, text: String) -> gpui::Div {
    div()
        .text_size(px(13.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgba_of(t.text_2))
        .child(SharedString::from(text))
}

/// One setting of the inspector: its label (with the value) above the control.
fn block(t: &Theme, label: String, hint: Option<String>, body: AnyElement) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(8.))
        .child(field_label(t, label))
        .child(body)
        .children(hint.map(|h| {
            div()
                .text_size(px(12.))
                .text_color(rgba_of(t.mist))
                .child(SharedString::from(h))
        }))
        .into_any_element()
}

/// A label on the left and its switch on the right.
fn toggle_row(t: &Theme, switch: impl IntoElement, label: String) -> AnyElement {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(8.))
        .child(
            div()
                .flex_1()
                .min_w(px(0.))
                .text_size(px(13.))
                .text_color(rgba_of(t.text))
                .child(SharedString::from(label)),
        )
        .child(switch)
        .into_any_element()
}

impl Render for BoardSheetView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = cx.global::<ThemeGlobal>().0;
        self.sync_fields(cx);
        let Some(s) = self.app.read(cx).bulk.board.clone() else {
            self.focused = false;
            return div().into_any_element();
        };
        if !self.focused {
            self.focused = true;
            window.focus(&self.focus, cx);
        }
        let app = self.app.clone();

        let geo = {
            let v = window.viewport_size();
            Geometry::for_window(f32::from(v.width), f32::from(v.height))
        };

        // Left column: layouts and the source list.
        let layouts = div()
            .flex()
            .flex_wrap()
            .gap(px(6.))
            .children(LAYOUTS.map(|k| {
                let a = app.clone();
                let enabled = s.layout_enabled(k);
                let b = seg(
                    &t,
                    SharedString::from(format!("board-layout-{}", k.as_str())),
                    layout_label(k),
                    s.layout == k,
                )
                .w(px((geo.left_w - 6.) / 2.))
                .h(px(44.))
                .flex_none()
                .justify_center();
                if enabled {
                    b.on_click(move |_, _, cx| update(&a, cx, |s| s.set_layout(k)))
                } else {
                    b.opacity(0.45)
                }
            }));
        let n = s.sources.len();
        let thumbs = self.thumbs.clone();
        let sources =
            div()
                .flex()
                .flex_col()
                .gap(px(4.))
                .children(s.sources.iter().enumerate().map(|(i, src)| {
                    let (a_up, a_down, a_rm) = (app.clone(), app.clone(), app.clone());
                    let mini = div()
                        .w(px(36.))
                        .h(px(26.))
                        .flex_none()
                        .rounded(px(4.))
                        .overflow_hidden()
                        .bg(rgba_of(t.raised))
                        .children(match thumbs.update(cx, |c, cx| c.get(&src.id, cx)) {
                            Thumb::Ready(image) => Some(
                                img(image)
                                    .size_full()
                                    .rounded(px(4.))
                                    .object_fit(ObjectFit::Cover),
                            ),
                            _ => None,
                        });
                    div()
                        .flex()
                        .items_center()
                        .gap(px(4.))
                        .h(px(40.))
                        .px(px(6.))
                        .rounded(px(scale::RADIUS_SM))
                        .border_1()
                        .border_color(rgba_of(t.border))
                        .bg(rgba_of(t.field))
                        .child(mini)
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .pl(px(2.))
                                .truncate()
                                .text_size(px(12.5))
                                .child(SharedString::from(src.title.clone())),
                        )
                        .child(
                            small_icon(
                                &t,
                                SharedString::from(format!("board-up-{i}")),
                                "arrow-up",
                                tr!("Move up"),
                                i > 0,
                            )
                            .on_click(move |_, _, cx| update(&a_up, cx, |s| s.move_source(i, -1))),
                        )
                        .child(
                            small_icon(
                                &t,
                                SharedString::from(format!("board-down-{i}")),
                                "arrow-down",
                                tr!("Move down"),
                                i + 1 < n,
                            )
                            .on_click(move |_, _, cx| update(&a_down, cx, |s| s.move_source(i, 1))),
                        )
                        .child(
                            small_icon(
                                &t,
                                SharedString::from(format!("board-rm-{i}")),
                                "x",
                                tr!("Remove from the board"),
                                n > 2,
                            )
                            .on_click(move |_, _, cx| update(&a_rm, cx, |s| s.remove_source(i))),
                        )
                }));
        let left =
            div()
                .id("board-left")
                .w(px(geo.left_w))
                .flex_none()
                .min_h(px(0.))
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap(px(14.))
                .child(field_label(&t, tr!("Layout")))
                .child(layouts)
                .child(div().text_size(px(12.5)).text_color(rgba_of(t.mist)).child(
                    SharedString::from(trn!(
                        "{n} screenshot, use the arrows to change the order",
                        "{n} screenshots, use the arrows to change the order",
                        n
                    )),
                ))
                .child(sources);

        // Inspector: one stacked block per setting, the column scrolls when it does not fit.
        let a = app.clone();
        let arrangement = (matches!(s.layout, LayoutKind::Pair | LayoutKind::Strip)).then(|| {
            let (a1, a2) = (a.clone(), a.clone());
            block(
                &t,
                tr!("Arrangement"),
                None,
                row()
                    .child(
                        seg(&t, "board-side", tr!("Side by side"), !s.stacked)
                            .flex_1()
                            .justify_center()
                            .px(px(6.))
                            .on_click(move |_, _, cx| update(&a1, cx, |s| s.stacked = false)),
                    )
                    .child(
                        seg(&t, "board-stack", tr!("Stacked"), s.stacked)
                            .flex_1()
                            .justify_center()
                            .px(px(6.))
                            .on_click(move |_, _, cx| update(&a2, cx, |s| s.stacked = true)),
                    )
                    .flex_nowrap()
                    .into_any_element(),
            )
        });
        let columns = (s.layout == LayoutKind::Grid).then(|| {
            let a = a.clone();
            block(
                &t,
                tr!("Columns"),
                None,
                row()
                    .flex_nowrap()
                    .children([0u32, 2, 3, 4].map(|c| {
                        let a = a.clone();
                        seg(
                            &t,
                            SharedString::from(format!("board-cols-{c}")),
                            if c == 0 { tr!("Auto") } else { c.to_string() },
                            s.columns.unwrap_or(0) == c,
                        )
                        .flex_1()
                        .justify_center()
                        .px(px(4.))
                        .on_click(move |_, _, cx| {
                            update(&a, cx, |s| s.columns = (c != 0).then_some(c))
                        })
                    }))
                    .into_any_element(),
            )
        });
        let labels = (s.layout == LayoutKind::Pair).then(|| {
            let a = a.clone();
            toggle_row(
                &t,
                switch(
                    &t,
                    "board-ba",
                    tr!("BEFORE and AFTER labels"),
                    s.before_after,
                    true,
                )
                .on_click(move |_, _, cx| update(&a, cx, |s| s.before_after = !s.before_after)),
                tr!("BEFORE and AFTER labels"),
            )
        });
        let gap = block(
            &t,
            format!("{}  \u{b7}  {} px", tr!("Gap"), s.gap),
            None,
            preset(&t, &a, "board-gap", &[0, 8, 16, 24, 40], s.gap, |s, v| {
                s.gap = v
            })
            .into_any_element(),
        );
        let margin = block(
            &t,
            format!("{}  \u{b7}  {} px", tr!("Outer margin"), s.margin),
            None,
            preset(
                &t,
                &a,
                "board-margin",
                &[0, 16, 40, 64, 96],
                s.margin,
                |s, v| s.margin = v,
            )
            .into_any_element(),
        );
        let swatches = row().gap(px(10.)).children(BgChoice::ALL.map(|b| {
            let a = a.clone();
            let on = s.background == b;
            let ring = if on { t.text } else { t.border };
            div()
                .id(SharedString::from(format!("board-bg-{b:?}")))
                .role(Role::Button)
                .aria_label(SharedString::from(b.label()))
                .aria_selected(on)
                .focusable()
                .tab_stop(true)
                .size(px(30.))
                .flex_none()
                .rounded(px(15.))
                .border_2()
                .border_color(rgba_of(ring))
                .p(px(2.))
                .cursor_pointer()
                .child(
                    div()
                        .size_full()
                        .rounded(px(12.))
                        .border_1()
                        .border_color(rgba_of(t.border))
                        .bg(rgba_of(bg_color(&t, b, &s.tint))),
                )
                .on_click(move |_, _, cx| update(&a, cx, |s| s.background = b))
        }));
        let background = block(
            &t,
            format!("{}  \u{b7}  {}", tr!("Background"), s.background.label()),
            Some(tr!(
                "Transparent is for PNG; a JPEG export puts it on white"
            )),
            div()
                .flex()
                .flex_col()
                .gap(px(8.))
                .child(swatches)
                .children(
                    (s.background == BgChoice::Tint)
                        .then(|| div().w_full().h(px(32.)).child(self.tint_field.clone())),
                )
                .into_any_element(),
        );
        let radius = block(
            &t,
            format!("{}  \u{b7}  {} px", tr!("Corner radius"), s.radius),
            None,
            preset(&t, &a, "board-radius", &[0, 8, 12, 24], s.radius, |s, v| {
                s.radius = v
            })
            .into_any_element(),
        );
        let shadow = {
            let a1 = a.clone();
            toggle_row(
                &t,
                switch(&t, "board-shadow", tr!("Shadow"), s.shadow, true)
                    .on_click(move |_, _, cx| update(&a1, cx, |s| s.shadow = !s.shadow)),
                tr!("Shadow"),
            )
        };
        let captions = {
            let a2 = a.clone();
            toggle_row(
                &t,
                switch(&t, "board-captions", tr!("Captions"), s.captions, true)
                    .on_click(move |_, _, cx| update(&a2, cx, |s| s.captions = !s.captions)),
                tr!("Captions"),
            )
        };
        let title = block(
            &t,
            tr!("Title"),
            None,
            div()
                .h(px(32.))
                .child(self.title_field.clone())
                .into_any_element(),
        );
        let inspector = div()
            .id("board-inspector")
            .w(px(geo.inspector_w))
            .flex_none()
            .min_h(px(0.))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(12.))
            .rounded(px(scale::RADIUS_LG))
            .border_1()
            .border_color(rgba_of(t.border))
            .bg(rgba_of(t.surface))
            .p(px(14.))
            .children(arrangement)
            .children(columns)
            .children(labels)
            .child(gap)
            .child(margin)
            .child(background)
            .child(radius)
            .child(shadow)
            .child(captions)
            .child(title);

        let canvas_el = canvas(
            &t,
            &s,
            &app,
            &self.thumbs,
            &mut self.preview,
            (geo.canvas_w, geo.canvas_h),
            cx,
        );
        let centre = div()
            .id("board-centre")
            .flex_1()
            .min_w(px(0.))
            .min_h(px(0.))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(10.))
            .child(canvas_el)
            .child(
                div()
                    .px(px(10.))
                    .py(px(3.))
                    .rounded(px(scale::RADIUS_PILL))
                    .border_1()
                    .border_color(rgba_of(t.border))
                    .text_size(px(12.))
                    .mono()
                    .text_color(rgba_of(t.text_2))
                    .child(SharedString::from(s.size_chip())),
            )
            .children((s.layout == LayoutKind::Free).then(|| {
                div()
                    .w(px(geo.canvas_w))
                    .text_size(px(12.5))
                    .text_color(rgba_of(t.mist))
                    .child(tr!(
                        "Drag a picture to place it, drag a corner to resize it. Hold Alt to turn snapping off, Shift to keep a line."
                    ))
            }))
            .child(
                div()
                    .w(px(geo.canvas_w))
                    .text_size(px(12.5))
                    .text_color(rgba_of(t.mist))
                    .child(tr!("Finish copies the screenshots into the board, with their annotations drawn in. The originals are not changed.")),
            );

        let (a_cancel, a_finish, a_close) = (app.clone(), app.clone(), app.clone());
        let can = s.can_finish();
        let panel = div()
            .id("board-sheet")
            .role(Role::Dialog)
            .aria_label(SharedString::from(s.title_text()))
            .w(px(geo.sheet_w))
            .h(px(geo.sheet_h))
            .flex()
            .flex_col()
            .rounded(px(18.))
            .border_1()
            .border_color(rgba_of(t.line))
            .bg(rgba_of(t.sidebar))
            .text_color(rgba_of(t.text))
            .occlude()
            .child(
                div()
                    .px(px(24.))
                    .pt(px(20.))
                    .pb(px(12.))
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(icon("layout-panel-left", 22., t.text))
                    .child(
                        div()
                            .text_size(px(20.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(tr!("New board")),
                    )
                    .child(crate::ui::preview_label::label(
                        &t,
                        crate::ui::preview_label::Size::M,
                        crate::ui::preview_label::PreviewFeature::Boards,
                        None,
                        Some(app.clone()),
                    ))
                    .child(div().flex_1())
                    .child(
                        div()
                            .id("board-close")
                            .role(Role::Button)
                            .aria_label(tr!("Close"))
                            .focusable()
                            .tab_stop(true)
                            .size(px(36.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(8.))
                            .cursor_pointer()
                            .hover(|s| s.bg(rgba_of(t.raised)))
                            .child(icon("x", 18., t.text_2))
                            .on_click(move |_, _, cx| close(&a_close, cx)),
                    ),
            )
            .child(
                div()
                    .id("board-body")
                    .flex_1()
                    .min_h(px(0.))
                    .px(px(SHEET_PAD))
                    .pb(px(16.))
                    .flex()
                    .gap(px(COL_GAP))
                    .child(left)
                    .child(centre)
                    .child(inspector),
            )
            .children(s.error.clone().map(|e| {
                div()
                    .px(px(24.))
                    .pb(px(8.))
                    .text_size(px(13.))
                    .text_color(rgba_of(t.warning))
                    .child(SharedString::from(e))
            }))
            .child(
                div()
                    .px(px(24.))
                    .py(px(16.))
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(12.))
                    .border_t_1()
                    .border_color(rgba_of(t.border))
                    .child(
                        button_with_icon(
                            &t,
                            "board-cancel",
                            tr!("Cancel"),
                            ButtonKind::Secondary,
                            None,
                        )
                        .on_click(move |_, _, cx| close(&a_cancel, cx)),
                    )
                    .child({
                        let b = button_with_icon(
                            &t,
                            "board-finish",
                            if s.busy {
                                tr!("Making the board\u{2026}")
                            } else {
                                tr!("Finish board")
                            },
                            ButtonKind::Primary,
                            None,
                        );
                        if can {
                            b.on_click(move |_, _, cx| finish(&a_finish, cx))
                        } else {
                            b.opacity(0.45)
                        }
                    }),
            );
        let key_app = app.clone();
        div()
            .id("board-scrim")
            .track_focus(&self.focus)
            .on_key_down(move |ev: &KeyDownEvent, _, cx| {
                if ev.keystroke.key == "escape" {
                    close(&key_app, cx);
                }
            })
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba_of(t.modal_scrim))
            .occlude()
            .child(panel)
            .into_any_element()
    }
}

fn small_icon(
    t: &Theme,
    id: SharedString,
    glyph: &'static str,
    aria: String,
    enabled: bool,
) -> gpui::Stateful<gpui::Div> {
    let hover = rgba_of(t.raised);
    div()
        .id(id)
        .role(Role::Button)
        .aria_label(SharedString::from(aria))
        .focusable()
        .tab_stop(enabled)
        .size(px(22.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(4.))
        .when(enabled, |d| d.cursor_pointer().hover(move |s| s.bg(hover)))
        .when(!enabled, |d| d.opacity(0.3))
        .child(icon(glyph, 13., t.text_2))
}

// ---- the calls ------------------------------------------------------------------------------

/// Opens the sheet for these screenshots `(id, title, width, height)`.
pub fn open(app: &Entity<AppModel>, shots: Vec<(String, String, u32, u32)>, cx: &mut App) {
    use crate::board_flow::BoardSource;
    app.update(cx, |m, cx| {
        m.bulk.sheet = None;
        m.bulk.board = Some(BoardSheet::new(
            shots
                .into_iter()
                .map(|(id, title, width, height)| BoardSource {
                    id,
                    title,
                    width,
                    height,
                })
                .collect(),
        ));
        m.bulk.epoch += 1;
        cx.notify();
    });
    // The one-time intro card is not shown over the sheet (QA 0.3.6 W-6): it comes when the
    // sheet closes (see `close`).
    refine_sizes(app, cx);
}

/// A source with a crop counts with its cropped size on the canvas (the size its picture has in
/// the board). The library row only knows the original size, so the ops are read (`get_capture`
/// without the image, read-only) and the sizes are corrected a moment after the sheet opens.
fn refine_sizes(app: &Entity<AppModel>, cx: &mut App) {
    let ids: Vec<String> = match app.read(cx).bulk.board.as_ref() {
        Some(s) => s.sources.iter().map(|x| x.id.clone()).collect(),
        None => return,
    };
    let backend = app.read(cx).backend.clone();
    let Some(daemon) = backend.daemon.clone() else {
        return;
    };
    let app = app.clone();
    cx.spawn(async move |cx| {
        let found = backend
            .rt
            .spawn(async move {
                let mut out = Vec::new();
                for id in ids {
                    let p = json!({ "capture_id": id, "include_image": false });
                    if let Ok(v) = Backend::call_coded(&daemon, "get_capture", p).await {
                        out.push((id, v["ops"].clone()));
                    }
                }
                out
            })
            .await
            .unwrap_or_default();
        cx.update(|cx| {
            update(&app, cx, |s| {
                for (id, ops) in &found {
                    if let Some(src) = s.sources.iter_mut().find(|x| &x.id == id) {
                        let (w, h) = crate::board_flow::cropped_size(ops, (src.width, src.height));
                        src.width = w;
                        src.height = h;
                    }
                }
            })
        });
    })
    .detach();
}

pub fn close(app: &Entity<AppModel>, cx: &mut App) {
    app.update(cx, |m, cx| {
        m.bulk.board = None;
        cx.notify();
    });
    // Design S24, once: the intro to Boards (and Send feedback) shows after the sheet, never over
    // it.
    crate::ui::preview_label::show_intro_if_due(
        app,
        crate::ui::preview_label::PreviewFeature::Boards,
        cx,
    );
}

/// Finish board: `create_board` as the user, then the new board opens in the editor.
pub fn finish(app: &Entity<AppModel>, cx: &mut App) {
    let params: Value = {
        let m = app.read(cx);
        let Some(s) = m.bulk.board.as_ref().filter(|s| s.can_finish()) else {
            return;
        };
        match s.request() {
            Ok(p) => p,
            Err(e) => {
                let app = app.clone();
                cx.defer(move |cx| update(&app, cx, |s| s.error = Some(e)));
                return;
            }
        }
    };
    let backend = app.read(cx).backend.clone();
    let Some(daemon) = backend.daemon.clone() else {
        update(app, cx, |s| {
            s.error = Some(tr!(
                "Making a board needs the CaptureMann engine, which is not running."
            ))
        });
        return;
    };
    update(app, cx, |s| {
        s.busy = true;
        s.error = None;
    });
    let app2 = app.clone();
    cx.spawn(async move |cx| {
        let res = backend
            .rt
            .spawn(async move { Backend::call_coded(&daemon, "create_board", params).await })
            .await;
        cx.update(|cx| match res {
            Ok(Ok(v)) => {
                let id = v["capture_id"].as_str().unwrap_or("").to_owned();
                close(&app2, cx);
                app::push_toast(
                    &app2,
                    crate::toast::ToastSpec::new(
                        crate::toast::ToastKind::Success,
                        tr!("Board created"),
                    ),
                    cx,
                );
                if !id.is_empty() {
                    app::set_route(&app2, app::open_route(&id), cx);
                }
            }
            Ok(Err(e)) => {
                let msg = call_error_text(&e);
                update(&app2, cx, |s| {
                    s.busy = false;
                    s.error = Some(msg);
                });
            }
            Err(e) => update(&app2, cx, |s| {
                s.busy = false;
                s.error = Some(e.to_string());
            }),
        });
        let _ = json!(null);
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_columns_fit_the_sheet_at_the_default_window_sizes() {
        for (w, h) in [(1180., 792.), (1346., 827.), (1920., 1080.), (900., 600.)] {
            let g = Geometry::for_window(w, h);
            let used = g.left_w + g.inspector_w + g.canvas_w + 2. * COL_GAP + 2. * SHEET_PAD;
            assert!(used <= g.sheet_w + 0.5, "{w}x{h}: {used} > {}", g.sheet_w);
            assert!(g.sheet_w <= w - 2. * SHEET_MARGIN + 0.5 || g.sheet_w == 640.);
            assert!(g.sheet_h <= h || g.sheet_h == 420.);
            assert!(g.canvas_w >= 160. && g.canvas_h >= 160.);
        }
    }

    #[test]
    fn a_larger_window_gives_the_canvas_more_room() {
        let small = Geometry::for_window(1180., 792.);
        let big = Geometry::for_window(1600., 1000.);
        assert!(big.canvas_w > small.canvas_w && big.canvas_h > small.canvas_h);
    }
}
