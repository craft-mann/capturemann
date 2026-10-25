//! The timeline: ruler, track bars, hatched cuts, split markers, trim handles, the playhead and the
//! toolbar above it (DESIGN §4 and §5.5). Drawing only; the events are in `mod.rs` and the logic
//! in `crate::recedit`.

use gpui::{
    canvas, div, point, prelude::*, px, AnyElement, Bounds, ContentMask, Context, CursorStyle,
    Hsla, MouseButton, PathBuilder, Pixels, Role, SharedString,
};

use cm_i18n::{tr, tr_ctx};

use super::{Drag, RecordingEditor, RowKind, Session};
use crate::recedit::geom::{
    self, hatch_lines, row_top, speed_label, tick_step_secs, SegKind, HANDLE_W, HATCH_SPACING,
    LABEL_W, ROW_GAP, ROW_H, RULER_H,
};
use crate::recedit::keys::{help_shown, Cmd};
use crate::recedit::model::Selection;
use crate::theme::{scale, type_scale, Color, Theme};
use crate::ui::components::{ellipsis_text, icon, keycap};
use crate::ui::paint::{rgba_of, TextExt};

/// Gap between the label column and the track area, px (the zoom lane uses the same).
pub(super) const BODY_GAP: f32 = 12.0;
/// Height of a toolbar button, px (R3: taller than the 32 px small button).
const TOOL_H: f32 = 34.0;
/// Spacing of the microphone bar's stripes, px.
const STRIPE: f32 = 4.0;

/// The glyph of a track row.
fn row_icon(k: RowKind) -> &'static str {
    match k {
        RowKind::Screen => "monitor",
        RowKind::Camera => "video",
        RowKind::Mic => "mic",
    }
}

fn hsla(c: Color) -> Hsla {
    Hsla::from(rgba_of(c))
}

/// `m:ss`, or `m:ss.f` when the ticks are closer than a second.
pub(crate) fn ruler_label(secs: f64, step: f64) -> String {
    let whole = secs.floor() as i64;
    let (m, s) = (whole / 60, whole % 60);
    if step < 1.0 {
        format!(
            "{m}:{s:02}.{}",
            ((secs - secs.floor()) * 10.0).round() as i64 % 10
        )
    } else {
        format!("{m}:{s:02}")
    }
}

impl RecordingEditor {
    /// The timeline card (toolbar, tracks, zoom lane) and the key help under it.
    pub(super) fn timeline_section(
        &self,
        t: &Theme,
        s: &Session,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .rounded(px(scale::RADIUS_XL))
                    .border_1()
                    .border_color(rgba_of(t.border))
                    .bg(rgba_of(t.surface))
                    .child(self.toolbar(t, s, cx))
                    .child(
                        div()
                            .px(px(12.))
                            .pb(px(12.))
                            .pt(px(4.))
                            .flex()
                            .flex_col()
                            .child(self.timeline_body(t, s, cx))
                            .children(self.captions_lane(t, s, cx))
                            .child(self.zoom_lane(t, s, cx)),
                    ),
            )
            .child(self.help_line(t, s))
            .into_any_element()
    }

    /// A toolbar button: a 16 px glyph, an optional label and a faint key, a tooltip with the
    /// name and the key, and a pressed state drawn with a background. `aria` is the accessible
    /// name (it keeps the key, as before).
    #[allow(clippy::too_many_arguments)]
    fn tool_button(
        t: &Theme,
        id: &'static str,
        aria: String,
        glyph: &'static str,
        tip_label: String,
        key: Option<&'static str>,
        label: Option<String>,
        enabled: bool,
        pressed: Option<bool>,
    ) -> gpui::Stateful<gpui::Div> {
        let (hover, focus) = (rgba_of(t.raised), rgba_of(t.text));
        let keys: Vec<String> = key.map(|k| vec![k.to_owned()]).unwrap_or_default();
        let is_on = pressed == Some(true);
        div()
            .id(id)
            .role(Role::Button)
            .aria_label(SharedString::from(aria))
            .when_some(pressed, |d, on| {
                d.aria_toggled(if on {
                    gpui::Toggled::True
                } else {
                    gpui::Toggled::False
                })
            })
            .focusable()
            .tab_stop(true)
            .h(px(TOOL_H))
            .px(px(10.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(6.))
            .rounded(px(scale::RADIUS_SM))
            .border_1()
            .border_color(gpui::transparent_black())
            .text_size(px(type_scale::UI))
            .text_color(rgba_of(if is_on { t.text } else { t.text_2 }))
            .when(is_on, |d| d.bg(rgba_of(t.raised)))
            .when(enabled, |d| {
                d.cursor_pointer()
                    .when(!is_on, |d| d.hover(move |s| s.bg(hover)))
            })
            .when(!enabled, |d| d.opacity(0.4).cursor_default())
            .focus_visible(move |s| s.border_color(focus))
            .tooltip(crate::ui::tooltip::tip(tip_label, keys))
            .child(icon(glyph, 16., if is_on { t.text } else { t.text_2 }))
            .children(label)
            .children(key.map(|k| {
                div()
                    .mono()
                    .text_size(px(11.))
                    .text_color(rgba_of(t.faint))
                    .child(k)
            }))
    }

    /// One command button; clicks run `cmd` only when `enabled`.
    fn tool(
        &self,
        spec: (&'static str, String, &'static str, String, &'static str),
        enabled: bool,
        cmd: Cmd,
        t: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (id, aria, glyph, tip_label, key) = spec;
        let b = Self::tool_button(
            t,
            id,
            aria,
            glyph,
            tip_label,
            Some(key),
            None,
            enabled,
            None,
        );
        if enabled {
            b.on_click(cx.listener(move |this, _, _, cx| this.run(cmd, cx)))
                .into_any_element()
        } else {
            b.into_any_element()
        }
    }

    fn toolbar(&self, t: &Theme, s: &Session, cx: &mut Context<Self>) -> AnyElement {
        let m = &s.model;
        let sel = m.selection;
        let can_cut = matches!(sel, Selection::Range(_) | Selection::Segment(_));
        let can_restore = match sel {
            Selection::Cut(_) => true,
            Selection::Range(r) => m.list.cuts.iter().any(|c| c.from < r.to && r.from < c.to),
            _ => false,
        };
        let divider = || div().w(px(1.)).h(px(20.)).mx(px(6.)).bg(rgba_of(t.line));
        let history = |id: &'static str,
                       glyph: &'static str,
                       label: String,
                       enabled: bool,
                       cmd: Cmd,
                       cx: &mut Context<Self>| {
            let b = Self::tool_button(
                t,
                id,
                label.clone(),
                glyph,
                label,
                None,
                None,
                enabled,
                None,
            );
            if enabled {
                b.on_click(cx.listener(move |this, _, _, cx| this.run(cmd, cx)))
                    .into_any_element()
            } else {
                b.into_any_element()
            }
        };
        let snap = Self::tool_button(
            t,
            "re-snap",
            if m.snapping {
                tr!("Snap (N): on")
            } else {
                tr!("Snap (N): off")
            },
            "magnet",
            tr_ctx!("timeline", "Snapping"),
            Some("N"),
            Some(tr_ctx!("timeline", "Snap")),
            true,
            Some(m.snapping),
        )
        .on_click(cx.listener(|this, _, _, cx| this.run(Cmd::ToggleSnap, cx)));
        let zoom_btn = |id: &'static str,
                        aria: String,
                        glyph: &'static str,
                        f: f64,
                        cx: &mut Context<Self>| {
            Self::tool_button(t, id, aria.clone(), glyph, aria, None, None, true, None).on_click(
                cx.listener(move |this, _, _, cx| {
                    if let Some(s) = this.session() {
                        let head = s.model.playhead;
                        crate::recedit::range::zoom_keeping_playhead(&mut s.geom, f, head);
                    }
                    cx.notify();
                }),
            )
        };
        let zoom_out = zoom_btn(
            "re-zoom-out",
            tr!("Zoom the timeline out"),
            "zoom-out",
            1.0 / 1.5,
            cx,
        );
        let zoom_in = zoom_btn(
            "re-zoom-in",
            tr!("Zoom the timeline in"),
            "zoom-in",
            1.5,
            cx,
        );
        let zoom_note = crate::recedit::range::is_zoomed(&s.geom).then(|| {
            let (from, to) = crate::recedit::range::range_label_parts(&s.geom);
            div()
                .id("re-zoom-range")
                .mr(px(8.))
                .text_size(px(type_scale::CAPTION))
                .text_color(rgba_of(t.mist))
                .child(SharedString::from(tr!(
                    "Timeline zoomed to {from}-{to}",
                    from = from,
                    to = to
                )))
        });
        let undo = history("re-undo", "undo-2", tr!("Undo"), m.can_undo, Cmd::Undo, cx);
        let redo = history("re-redo", "redo-2", tr!("Redo"), m.can_redo, Cmd::Redo, cx);
        div()
            .id("re-toolbar")
            .role(Role::Toolbar)
            .aria_label(tr!("Timeline tools"))
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(2.))
            .px(px(12.))
            .py(px(8.))
            .child(self.tool(
                (
                    "re-in",
                    tr!("In (I)"),
                    "cm-trim-in",
                    tr!("Set In point"),
                    "I",
                ),
                true,
                Cmd::SetIn,
                t,
                cx,
            ))
            .child(self.tool(
                (
                    "re-out",
                    tr!("Out (O)"),
                    "cm-trim-out",
                    tr!("Set Out point"),
                    "O",
                ),
                true,
                Cmd::SetOut,
                t,
                cx,
            ))
            .child(self.tool(
                (
                    "re-split",
                    tr!("Split (S)"),
                    "cm-split",
                    tr!("Split at the playhead"),
                    "S",
                ),
                true,
                Cmd::Split,
                t,
                cx,
            ))
            .child(self.tool(
                (
                    "re-cut",
                    tr!("Cut (\u{232B})"),
                    "scissors",
                    tr!("Cut the selection"),
                    "\u{232B}",
                ),
                can_cut,
                Cmd::Delete,
                t,
                cx,
            ))
            .child(self.tool(
                (
                    "re-restore",
                    tr!("Restore (R)"),
                    "archive-restore",
                    tr!("Restore the cut"),
                    "R",
                ),
                can_restore,
                Cmd::Restore,
                t,
                cx,
            ))
            .child(divider())
            .child(undo)
            .child(redo)
            .child(divider())
            .child(snap)
            .child(div().flex_1())
            .children(zoom_note)
            .child(zoom_out)
            .child(zoom_in)
            .into_any_element()
    }

    /// The key help under the timeline: one keycap and one word each.
    fn help_line(&self, t: &Theme, s: &Session) -> AnyElement {
        let note = s.model.note;
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_x(px(14.))
            .gap_y(px(6.))
            .text_size(px(type_scale::CAPTION))
            .text_color(rgba_of(t.mist))
            .when_some(note, |d, n| {
                d.child(
                    div()
                        .text_color(rgba_of(t.warning))
                        .child(SharedString::from(n.to_string())),
                )
            })
            .children(help_shown().into_iter().map(|(k, what)| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(keycap(t, k))
                    .child(what)
            }))
            .into_any_element()
    }

    fn timeline_body(&self, t: &Theme, s: &Session, cx: &mut Context<Self>) -> AnyElement {
        let rows = s.rows.len();
        let body_h = geom::body_height(rows);
        let g = s.geom;
        let segs = s.model.segments();
        let list = &s.model.list;
        let ph = s.model.playhead;
        let x_of = |tm| g.x_of(tm);
        let drag = s.drag;

        // ---- labels --------------------------------------------------------------------
        let labels = div()
            .relative()
            .w(px(LABEL_W))
            .h(px(body_h))
            .flex_none()
            .children(s.rows.iter().enumerate().map(|(i, r)| {
                div()
                    .absolute()
                    .left(px(0.))
                    .top(px(row_top(i)))
                    .w(px(LABEL_W))
                    .h(px(ROW_H))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .text_size(px(13.))
                    .text_color(rgba_of(t.text_2))
                    .child(icon(row_icon(r.kind), 14., t.mist))
                    .child(ellipsis_text(r.kind.label()))
            }));

        // ---- ruler ---------------------------------------------------------------------
        let step = tick_step_secs(&g, 72.0);
        let first = (g.scroll.as_secs_f64() / step).floor() * step;
        let mut ticks = Vec::new();
        let mut at = first;
        loop {
            let tm = cm_core::MediaTime::from_secs_f64(at.max(0.0)).unwrap_or(g.end);
            let x = x_of(tm);
            if x > g.width + 1.0 {
                break;
            }
            if x >= -1.0 {
                ticks.push((x, ruler_label(at, step)));
            }
            at += step;
            if ticks.len() > 400 {
                break;
            }
        }
        let ruler = ticks.into_iter().map(|(x, label)| {
            div()
                .absolute()
                .left(px(x))
                .top(px(0.))
                .h(px(RULER_H))
                .flex()
                .items_end()
                .child(div().w(px(1.)).h(px(8.)).bg(rgba_of(t.line)))
                .child(
                    div()
                        .pl(px(4.))
                        .pb(px(2.))
                        .text_size(px(type_scale::CAPTION))
                        .text_color(rgba_of(t.mist))
                        .mono()
                        .child(SharedString::from(label)),
                )
        });

        // ---- track bars ----------------------------------------------------------------
        let mut bars: Vec<AnyElement> = Vec::new();
        let mut cut_rects: Vec<(f32, f32)> = Vec::new();
        let selected_seg = match s.model.selection {
            Selection::Segment(i) => Some(i),
            _ => None,
        };
        let selected_cut = match s.model.selection {
            Selection::Cut(i) => Some(i),
            _ => None,
        };
        let top0 = row_top(0);
        for seg in &segs {
            let (x0, x1) = (x_of(seg.span.from), x_of(seg.span.to));
            if x1 < 0.0 || x0 > g.width {
                continue;
            }
            let (x0c, x1c) = (x0.max(-4.0), x1.min(g.width + 4.0));
            let w = (x1c - x0c).max(1.0);
            match seg.kind {
                SegKind::Kept {
                    index,
                    speed,
                    keep_audio: _,
                } => {
                    let selected = selected_seg == Some(index);
                    let sped = (speed - 1.0).abs() > 1e-6;
                    bars.push(
                        div()
                            .absolute()
                            .left(px(x0c + 0.5))
                            .top(px(top0))
                            .w(px((w - 1.0).max(1.0)))
                            .h(px(ROW_H))
                            .rounded(px(8.))
                            .border_1()
                            .border_color(rgba_of(if selected { t.text } else { t.line }))
                            .when(selected, |d| d.border_2())
                            .bg(rgba_of(t.raised))
                            .flex()
                            .items_center()
                            .px(px(6.))
                            .when(sped && w > 40.0, |d| {
                                d.child(
                                    div()
                                        .px(px(6.))
                                        .py(px(1.))
                                        .rounded(px(6.))
                                        .border_1()
                                        .border_color(rgba_of(t.line))
                                        .bg(rgba_of(t.surface))
                                        .mono()
                                        .text_size(px(11.))
                                        .text_color(rgba_of(t.text))
                                        .child(SharedString::from(speed_label(speed))),
                                )
                            })
                            .into_any_element(),
                    );
                }
                SegKind::Cut { index } => {
                    let selected = selected_cut == Some(index);
                    cut_rects.push((x0c, x1c));
                    bars.push(
                        div()
                            .absolute()
                            .left(px(x0c))
                            .top(px(top0))
                            .w(px(w))
                            .h(px(ROW_H))
                            .rounded(px(6.))
                            .border_1()
                            .border_color(rgba_of(if selected { t.text } else { t.line }))
                            .when(selected, |d| d.border_2())
                            .bg(rgba_of(t.surface))
                            .into_any_element(),
                    );
                }
                SegKind::Trimmed => {}
            }
        }
        for (i, row) in s.rows.iter().enumerate().skip(1) {
            let (x0, x1) = (x_of(row.from).max(-4.0), x_of(row.to).min(g.width + 4.0));
            if x1 <= x0 {
                continue;
            }
            let (fill, border) = match row.kind {
                RowKind::Camera => (t.rec.with_alpha(22), t.rec.with_alpha(60)),
                _ => (t.raised, t.line),
            };
            let stripes = (row.kind == RowKind::Mic).then(|| {
                // The microphone bar is drawn as waveform stripes (R3): thin vertical lines.
                let color = hsla(t.mist.with_alpha(110));
                canvas(
                    |bounds: Bounds<Pixels>, _, _| bounds,
                    move |_, bounds: Bounds<Pixels>, window, _| {
                        let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
                        let mut x = STRIPE / 2.0;
                        while x < w {
                            let mut pb = PathBuilder::stroke(px(1.));
                            pb.move_to(point(bounds.origin.x + px(x), bounds.origin.y + px(5.)));
                            pb.line_to(point(
                                bounds.origin.x + px(x),
                                bounds.origin.y + px(h - 5.),
                            ));
                            if let Ok(p) = pb.build() {
                                window.paint_path(p, color);
                            }
                            x += STRIPE;
                        }
                    },
                )
                .absolute()
                .size_full()
            });
            bars.push(
                div()
                    .absolute()
                    .left(px(x0))
                    .top(px(row_top(i)))
                    .w(px(x1 - x0))
                    .h(px(ROW_H))
                    .rounded(px(8.))
                    .border_1()
                    .border_color(rgba_of(border))
                    .bg(rgba_of(fill))
                    .overflow_hidden()
                    .children(stripes)
                    .into_any_element(),
            );
        }

        // Trimmed ends are dimmed over every row.
        let (xin, xout) = (x_of(list.start), x_of(list.end));
        let dim = |x0: f32, x1: f32| {
            let (x0, x1) = (x0.clamp(0.0, g.width), x1.clamp(0.0, g.width));
            div()
                .absolute()
                .left(px(x0))
                .top(px(RULER_H))
                .w(px((x1 - x0).max(0.0)))
                .h(px(body_h - RULER_H))
                .bg(rgba_of(t.ground.with_alpha(62)))
        };
        let dims = [dim(0.0, xin), dim(xout, g.width)];

        // ---- overlays: selection, splits, handles, playhead -----------------------------
        let range = match s.model.selection {
            Selection::Range(r) => Some(r),
            _ => None,
        };
        let range_box = range.map(|r| {
            let (a, b) = (x_of(r.from).max(0.0), x_of(r.to).min(g.width));
            div()
                .absolute()
                .left(px(a))
                .top(px(RULER_H))
                .w(px((b - a).max(1.0)))
                .h(px(body_h - RULER_H))
                .border_1()
                .border_color(rgba_of(t.agent))
                .bg(rgba_of(t.agent.with_alpha(18)))
        });
        let anchor = s.model.range_anchor.map(|a| {
            div()
                .absolute()
                .left(px(x_of(a) - 1.0))
                .top(px(0.))
                .w(px(2.))
                .h(px(RULER_H))
                .bg(rgba_of(t.agent))
        });
        let splits = list.splits.iter().map(|sp| {
            div()
                .absolute()
                .left(px(x_of(*sp) - 1.0))
                .top(px(top0 - 3.0))
                .w(px(2.))
                .h(px(ROW_H + 6.))
                .bg(rgba_of(t.text_3))
        });
        let (in_t, out_t, dragging_in, dragging_out) = match drag {
            Some(Drag::Handle { is_in: true, t: d }) => (d, list.end, true, false),
            Some(Drag::Handle { is_in: false, t: d }) => (list.start, d, false, true),
            _ => (list.start, list.end, false, false),
        };
        let handle = |x: f32, is_in: bool, active: bool| {
            div()
                .absolute()
                .left(px(if is_in { x - HANDLE_W } else { x }))
                .top(px(top0 - 3.0))
                .w(px(HANDLE_W))
                .h(px(body_h - top0 - 1.0))
                .rounded_l(px(if is_in { 5. } else { 0. }))
                .rounded_r(px(if is_in { 0. } else { 5. }))
                .bg(rgba_of(if active { t.agent } else { t.text }))
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .w(px(2.))
                        .h(px(14.))
                        .rounded(px(1.))
                        .bg(rgba_of(t.ground)),
                )
        };
        let handles = [
            handle(x_of(in_t), true, dragging_in),
            handle(x_of(out_t), false, dragging_out),
        ];
        let px_ph = x_of(ph);
        let playhead = div()
            .absolute()
            .left(px(px_ph - 1.0))
            .top(px(0.))
            .w(px(2.))
            .h(px(body_h))
            .bg(rgba_of(t.text))
            .child(
                div()
                    .absolute()
                    .left(px(-4.))
                    .top(px(0.))
                    .w(px(10.))
                    .h(px(8.))
                    .rounded_b(px(3.))
                    .bg(rgba_of(t.text)),
            );

        // ---- hatching and measuring ----------------------------------------------------
        let line_color = hsla(t.mist.with_alpha(70));
        let (hatch_top, hatch_h) = (top0, (rows as f32) * (ROW_H + ROW_GAP) - ROW_GAP);
        let cell = self.area.clone();
        let entity = cx.entity();
        let used = g.width;
        let surface = canvas(
            move |bounds: Bounds<Pixels>, window, cx| {
                cell.set(bounds);
                let w = f32::from(bounds.size.width);
                if (w - used).abs() > 0.5 && w > 8.0 {
                    let e = entity.clone();
                    window.defer(cx, move |_w, cx| {
                        e.update(cx, |this, cx| {
                            if let Some(s) = this.session() {
                                s.geom.resize(w);
                            }
                            cx.notify();
                        })
                    });
                }
                bounds
            },
            move |_bounds, bounds: Bounds<Pixels>, window, _cx| {
                window.with_content_mask(Some(ContentMask { bounds }), |window| {
                    for (x0, x1) in &cut_rects {
                        for ((xa, ya), (xb, yb)) in
                            hatch_lines(*x0 + 1.0, *x1 - 1.0, hatch_h, HATCH_SPACING)
                        {
                            let mut pb = PathBuilder::stroke(px(1.));
                            pb.move_to(point(
                                bounds.origin.x + px(xa),
                                bounds.origin.y + px(hatch_top + ya),
                            ));
                            pb.line_to(point(
                                bounds.origin.x + px(xb),
                                bounds.origin.y + px(hatch_top + yb),
                            ));
                            if let Ok(path) = pb.build() {
                                window.paint_path(path, line_color);
                            }
                        }
                    }
                });
            },
        )
        .absolute()
        .size_full();

        let area = div()
            .id("re-timeline")
            .role(Role::Slider)
            .aria_label(tr!("Timeline"))
            .aria_description(tr!(
                "Left and right arrows move the playhead one frame, with Shift one second."
            ))
            .relative()
            .flex_1()
            .min_w(px(0.))
            .h(px(body_h))
            .overflow_hidden()
            .cursor(CursorStyle::Arrow)
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_down(MouseButton::Middle, cx.listener(Self::on_mouse_down))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up(MouseButton::Middle, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_scroll_wheel(cx.listener(Self::on_scroll))
            .child(surface)
            .children(ruler)
            .children(bars)
            .children(dims)
            .children(range_box)
            .children(anchor)
            .children(splits)
            .children(handles)
            .child(playhead);

        div()
            .flex()
            .gap(px(BODY_GAP))
            .child(labels)
            .child(area)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::contrast;

    #[test]
    fn ruler_labels_are_minutes_and_seconds() {
        assert_eq!(ruler_label(0.0, 5.0), "0:00");
        assert_eq!(ruler_label(75.0, 5.0), "1:15");
        assert_eq!(ruler_label(1.5, 0.5), "0:01.5");
        assert_eq!(ruler_label(3600.0, 60.0), "60:00");
    }

    /// NFR-13 / DESIGN §4: in both the dark and the light editor the marks the user needs (the
    /// playhead and handles in `text`, the hatching and the cut outline in `mist`/`line`, the
    /// ruler in `mist`) stay visible on the track area (`surface`) and the bars (`raised`).
    #[test]
    fn timeline_marks_are_visible_in_dark_and_light() {
        for th in [Theme::DARK, Theme::LIGHT] {
            assert!(
                contrast(th.text, th.surface) >= 7.0,
                "{:?} playhead",
                th.mode
            );
            assert!(
                contrast(th.text, th.raised) >= 7.0,
                "{:?} handle on bar",
                th.mode
            );
            assert!(
                contrast(th.mist, th.surface) >= 4.5,
                "{:?} ruler text",
                th.mode
            );
            // Hatching is a graphic, not text: the 3:1 rule for UI components.
            let hatch = th.mist.with_alpha(70).over(th.surface);
            assert!(contrast(hatch, th.surface) >= 1.5, "{:?} hatching", th.mode);
            assert!(
                contrast(th.text_3, th.surface) >= 4.5,
                "{:?} split marker",
                th.mode
            );
            assert!(
                contrast(th.agent, th.surface) >= 3.0,
                "{:?} selection",
                th.mode
            );
        }
    }
}
