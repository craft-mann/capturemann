use super::*;
use crate::ui::field_style::canvas as canvas_tok;

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
                Tool::Text | Tool::SelectText => CursorStyle::IBeam,
                _ => CursorStyle::Crosshair,
            }
        };
        let summary = s.summary.clone();
        let step_hint = s.step_hint.clone().map(|(p, n, size, color)| {
            let (x, y) = vp.to_screen(p);
            (x, y, n, size * z, color)
        });
        let image = s.image_handle.clone();
        // The callout's tail tip is a blue dot (painted below) with a "Tail tip" label.
        let tail_label = overlay
            .chrome
            .as_ref()
            .and_then(|c| c.handles.iter().find(|(h, _)| *h == Handle::Tail))
            .map(|(_, p)| vp.to_screen(*p));

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

        // "Type text" (just placed) and the hint pill (selected, not editing) are text, so they
        // are elements over the canvas rather than quads.
        let placeholder = overlay.text.as_ref().and_then(|tg| {
            let text = tg.placeholder.clone()?;
            let (x, y) = vp.to_screen(Pt::new(tg.frame.x, tg.frame.y));
            let mut c = Hsla::from(gpui_rgb(tg.color));
            c.a = placeholder_alpha(tg.light);
            Some((x as f32, y as f32, (tg.size * z) as f32, c, text))
        });
        let hint_pill = overlay
            .selected_text
            .as_ref()
            .filter(|st| st.hint)
            .map(|st| {
                let (x0, _) = vp.to_screen(Pt::new(st.frame.x, st.frame.y));
                let (x1, y1) =
                    vp.to_screen(Pt::new(st.frame.x + st.frame.w, st.frame.y + st.frame.h));
                (((x0 + x1) / 2.0) as f32, y1 as f32)
            });
        // While a text annotation is open the canvas is the platform's text input (composition).
        let editing_text = overlay.text.is_some();
        let input_focus = self.canvas_focus.clone();
        let input_entity = entity.clone();
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
            move |bounds, (), window, cx| {
                if editing_text {
                    window.handle_input(
                        &input_focus,
                        gpui::ElementInputHandler::new(bounds, input_entity.clone()),
                        cx,
                    );
                }
                paint_overlay(bounds, &overlay, vp, &theme, window)
            },
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
                    .children(tail_label.map(|(x, y)| {
                        div()
                            .absolute()
                            .left(px(x as f32 + 10.))
                            .top(px(y as f32 - 10.))
                            .px(px(6.))
                            .py(px(2.))
                            .rounded(px(scale::RADIUS_XS))
                            .bg(rgba_of(t.raised))
                            .border_1()
                            .border_color(rgba_of(palette::INFO_BLUE))
                            .text_size(px(11.))
                            .text_color(rgba_of(t.text))
                            .child(SharedString::from(tr_ctx!("callout handle", "Tail tip")))
                    }))
                    .children(placeholder.map(|(x, y, size, color, text)| {
                        div()
                            .absolute()
                            .left(px(x))
                            .top(px(y))
                            .text_size(px(size))
                            .text_color(color)
                            .whitespace_nowrap()
                            .child(SharedString::from(text))
                    }))
                    .children(hint_pill.map(|(cx_, y)| {
                        const W: f32 = 240.;
                        div()
                            .absolute()
                            .left(px(cx_ - W / 2.))
                            .top(px(y + HINT_GAP))
                            .w(px(W))
                            .flex()
                            .justify_center()
                            .child(
                                div()
                                    .px(px(8.))
                                    .py(px(3.))
                                    .rounded_full()
                                    .bg(rgba_of(t.raised))
                                    .border_1()
                                    .border_color(rgba_of(t.border))
                                    .text_size(px(11.))
                                    .text_color(rgba_of(t.text))
                                    .whitespace_nowrap()
                                    .child(SharedString::from(tr!(
                                        "Double-click or Enter to edit"
                                    ))),
                            )
                    }))
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
                            .child(SharedString::from(n))
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
            let image = Rc::new(0.0, 0.0, f64::from(o.image.0), f64::from(o.image.1));
            paint_draft(d, &pt, &rect_b, z, t, (image, o.spot_exists), window);
        }
        // Selection box and handles.
        if let Some(ch) = &o.chrome {
            let is_line = ch
                .handles
                .iter()
                .any(|(h, _)| matches!(h, Handle::From | Handle::To));
            let blue = o.selected_text.as_ref();
            if let Some(st) = blue {
                // A selected text box: solid blue outline on a halo, handles below.
                outline_with_halo(window, rect_b(ch.bounds), st.light);
            } else if !is_line {
                window.paint_quad(gpui::quad(
                    rect_b(ch.bounds),
                    0.,
                    gpui::transparent_black(),
                    1.,
                    hsla(t.text),
                    BorderStyle::Solid,
                ));
            }
            for (h, p) in &ch.handles {
                let c = pt(*p);
                if *h == Handle::Tail {
                    // The tail tip: a blue dot with a white ring.
                    let r = px(HANDLE * 0.75);
                    window.paint_quad(gpui::quad(
                        Bounds {
                            origin: point(c.x - r, c.y - r),
                            size: size(r * 2.0, r * 2.0),
                        },
                        r,
                        hsla(palette::INFO_BLUE),
                        2.,
                        gpui::white(),
                        BorderStyle::Solid,
                    ));
                    continue;
                }
                let half = px(HANDLE / 2.0);
                window.paint_quad(gpui::quad(
                    Bounds {
                        origin: point(c.x - half, c.y - half),
                        size: size(px(HANDLE), px(HANDLE)),
                    },
                    px(2.),
                    gpui::white(),
                    1.5,
                    if blue.is_some() {
                        hsla(canvas_tok::OUTLINE)
                    } else {
                        hsla(t.text)
                    },
                    BorderStyle::Solid,
                ));
            }
        }
        // A text box under the pointer that is not selected: thin dashed blue outline.
        if let Some(r) = o.text_hover {
            window.paint_quad(gpui::quad(
                rect_b(r),
                0.,
                gpui::transparent_black(),
                canvas_tok::HOVER_W,
                hsla(canvas_tok::OUTLINE),
                BorderStyle::Dashed,
            ));
        }
        // Text being typed: blue outline on a halo, the composing span, the caret in the text
        // colour (the "Type text" placeholder is an element, see `canvas_pane`).
        if let Some(tg) = &o.text {
            outline_with_halo(window, rect_b(tg.frame), tg.light);
            for m in &tg.marked {
                window.paint_quad(gpui::fill(rect_b(*m), hsla(canvas_tok::SELECTION)));
            }
            let c = pt(tg.caret);
            let ch = px((tg.caret_h * z) as f32);
            window.paint_quad(gpui::fill(
                Bounds {
                    origin: c,
                    size: size(px(2.), ch),
                },
                Hsla::from(gpui_rgb(tg.color)),
            ));
        }
    });
}

/// Gap between a selected text box and its hint pill (px).
const HINT_GAP: f32 = 10.;

/// Alpha (0..=1) of the placeholder over a light or a dark image (spec: 38 % / 45 %).
fn placeholder_alpha(light_image: bool) -> f32 {
    let pct = if light_image {
        canvas_tok::PLACEHOLDER_ON_LIGHT_PCT
    } else {
        canvas_tok::PLACEHOLDER_ON_DARK_PCT
    };
    pct as f32 / 100.0
}

/// `b` grown by `by` px on every side.
fn grow(b: Bounds<Pixels>, by: f32) -> Bounds<Pixels> {
    Bounds {
        origin: point(b.origin.x - px(by), b.origin.y - px(by)),
        size: size(b.size.width + px(2. * by), b.size.height + px(2. * by)),
    }
}

/// The 1.5 px blue outline just outside `b` and the 3 px halo outside that (white on a light
/// image, near-black on a dark one), so the box shows on any screenshot.
fn outline_with_halo(window: &mut Window, b: Bounds<Pixels>, light_image: bool) {
    window.paint_quad(gpui::quad(
        grow(b, canvas_tok::OUTLINE_W + canvas_tok::HALO_W),
        0.,
        gpui::transparent_black(),
        canvas_tok::HALO_W,
        hsla(canvas_tok::halo(light_image)),
        BorderStyle::Solid,
    ));
    window.paint_quad(gpui::quad(
        grow(b, canvas_tok::OUTLINE_W),
        0.,
        gpui::transparent_black(),
        canvas_tok::OUTLINE_W,
        hsla(canvas_tok::OUTLINE),
        BorderStyle::Solid,
    ));
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
    // The image rectangle and whether the image already has a spotlight.
    (image, spot_exists): (Rc, bool),
    window: &mut Window,
) {
    match d {
        OpKind::Spotlight { shapes, dim, .. } => {
            let Some(s) = shapes.first() else { return };
            let r = s.rect.normalized();
            // The effect while dragging: everything outside the hole is dimmed (the bounding
            // box for an ellipse), unless the image already is dimmed by its spotlight.
            if !spot_exists {
                let shade = Hsla {
                    h: 0.,
                    s: 0.,
                    l: 0.,
                    a: (*dim as f32).clamp(0.2, 0.85),
                };
                let (right, bottom) = (r.x + r.w, r.y + r.h);
                let (img_r, img_b) = (image.x + image.w, image.y + image.h);
                for band in [
                    Rc::new(image.x, image.y, image.w, (r.y - image.y).max(0.0)),
                    Rc::new(image.x, bottom, image.w, (img_b - bottom).max(0.0)),
                    Rc::new(image.x, r.y, (r.x - image.x).max(0.0), r.h),
                    Rc::new(right, r.y, (img_r - right).max(0.0), r.h),
                ] {
                    window.paint_quad(gpui::fill(rect_b(band), shade));
                }
            }
            // The hole's outline: white on a dark line so it reads on any picture.
            if s.kind == cm_core::annotation::SpotShapeKind::Ellipse {
                let (cx, cy, rx, ry) = (r.x + r.w / 2.0, r.y + r.h / 2.0, r.w / 2.0, r.h / 2.0);
                let pts: Vec<_> = (0..64)
                    .map(|i| {
                        let a = f64::from(i) / 64.0 * std::f64::consts::TAU;
                        pt(Pt::new(cx + rx * a.cos(), cy + ry * a.sin()))
                    })
                    .collect();
                stroke_line(window, &pts, 3.5, hsla(Color::rgb_a(0x000000, 45)), true);
                stroke_line(window, &pts, 1.5, gpui::white(), true);
            } else {
                let rad = px((s.radius * z) as f32);
                let b = rect_b(r);
                window.paint_quad(gpui::quad(
                    b,
                    rad,
                    gpui::transparent_black(),
                    3.5,
                    hsla(Color::rgb_a(0x000000, 45)),
                    BorderStyle::Solid,
                ));
                window.paint_quad(gpui::quad(
                    b,
                    rad,
                    gpui::transparent_black(),
                    1.5,
                    gpui::white(),
                    BorderStyle::Solid,
                ));
            }
        }
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
        OpKind::Callout { rect, bg, tail, .. } => {
            // The box being dragged out, filled so it reads as the callout it will be.
            let mut fill = Hsla::from(gpui_rgb(*bg));
            fill.a = 0.85;
            let b = rect_b(*rect);
            window.paint_quad(gpui::fill(b, fill));
            let r = rect.normalized();
            let c = Pt::new(r.x + r.w / 2.0, r.y + r.h / 2.0);
            stroke_line(
                window,
                &[pt(c), pt(*tail)],
                (3.0 * z) as f32,
                Hsla::from(gpui_rgb(*bg)),
                false,
            );
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholder_is_stronger_on_dark_images_and_the_halo_flips() {
        assert!((placeholder_alpha(true) - 0.38).abs() < 1e-6);
        assert!((placeholder_alpha(false) - 0.45).abs() < 1e-6);
        assert_eq!(canvas_tok::halo(true), canvas_tok::HALO_ON_LIGHT);
        assert_eq!(canvas_tok::halo(false), canvas_tok::HALO_ON_DARK);
    }
}
