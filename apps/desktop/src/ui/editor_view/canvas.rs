use super::*;

impl EditorView {
    pub(super) fn canvas_pane(
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
