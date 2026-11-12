//! The 0.3.3 ops: callout and stamp (ANN-4), step label (ANN-6), spotlight (ANN-7).

use cm_core::annotation::{
    step_label, CalloutStyle, Color, OpKind, Pt, Rc, SpotMode, SpotShape, SpotShapeKind, StampKind,
};
use cm_core::CmError;
use tiny_skia::{
    FillRule, LineCap, LineJoin, Mask, Paint, Path, PathBuilder, Pixmap, PixmapPaint, Rect, Stroke,
    Transform,
};

use crate::redact::{self, Region};
use crate::render::{blit, fill_path, paint, rounded_rect};
use crate::text::{self, Fonts, TextLayer, TextStyle};

/// Horizontal text padding of a callout, in em.
const PAD_X_EM: f64 = 0.6;
/// Vertical text padding of a callout, in em.
const PAD_Y_EM: f64 = 0.4;

/// The measured geometry of a callout (ANN-4): the drawn box and the text layer.
pub(crate) struct CalloutGeom {
    pub boxr: Rc,
    pub layer: Option<TextLayer>,
}

/// `max(rect, measured text + padding)`; the box keeps the stored top-left corner.
pub(crate) fn measure_callout(
    fonts: &mut Fonts,
    kind: &OpKind,
) -> Result<Option<CalloutGeom>, CmError> {
    let OpKind::Callout {
        rect,
        text: t,
        size,
        color,
        ..
    } = kind
    else {
        return Ok(None);
    };
    let mut b = rect.normalized();
    let style = TextStyle {
        px: *size as f32,
        color: *color,
        outline: None,
        bg: None,
        wrap_w: None,
        center: false,
    };
    let layer = text::layout(fonts, t, &style)?;
    if let Some(l) = &layer {
        let tw = f64::from(l.text_w.ceil());
        let th = f64::from(l.pixmap.height()) - 2.0 * f64::from(l.pad);
        b.w = b.w.max(tw + 2.0 * (size * PAD_X_EM).round());
        b.h = b.h.max(th + 2.0 * (size * PAD_Y_EM).round());
    }
    Ok(Some(CalloutGeom { boxr: b, layer }))
}

/// Tail triangle: two base points just inside the box edge nearest the tip, and the tip.
struct Tail {
    b1: [f64; 2],
    b2: [f64; 2],
    tip: [f64; 2],
}

fn tail_for(b: Rc, tip: Pt, radius: f64, size: f64) -> Option<Tail> {
    let dxo = (b.x - tip.x).max(tip.x - (b.x + b.w)).max(0.0);
    let dyo = (b.y - tip.y).max(tip.y - (b.y + b.h)).max(0.0);
    if dxo == 0.0 && dyo == 0.0 {
        return None; // the tip is inside the box: no tail
    }
    let vertical_edge = dxo > dyo; // the tip is left or right of the box
    let (lo, hi, along) = if vertical_edge {
        (b.y, b.y + b.h, tip.y)
    } else {
        (b.x, b.x + b.w, tip.x)
    };
    let len = hi - lo;
    let rr = radius.min(len / 2.0).max(0.0);
    let bh = (size * 0.5).min(len * 0.3).max(1.0);
    let (cmin, cmax) = (lo + rr + bh, hi - rr - bh);
    let c = if cmin <= cmax {
        along.clamp(cmin, cmax)
    } else {
        (lo + hi) / 2.0
    };
    // Edge coordinate and a 2 px inward step (hides the anti-aliasing seam).
    let (e, inward) = if vertical_edge {
        if tip.x < b.x {
            (b.x, 2.0)
        } else {
            (b.x + b.w, -2.0)
        }
    } else if tip.y < b.y {
        (b.y, 2.0)
    } else {
        (b.y + b.h, -2.0)
    };
    let map = |a: f64| {
        if vertical_edge {
            [e + inward, a]
        } else {
            [a, e + inward]
        }
    };
    Some(Tail {
        b1: map(c - bh),
        b2: map(c + bh),
        tip: [tip.x, tip.y],
    })
}

pub(crate) fn draw_callout(
    pm: &mut Pixmap,
    fonts: &mut Fonts,
    kind: &OpKind,
) -> Result<(), CmError> {
    let OpKind::Callout {
        tail,
        size,
        bg,
        radius,
        style,
        ..
    } = kind
    else {
        return Ok(());
    };
    let Some(g) = measure_callout(fonts, kind)? else {
        return Ok(());
    };
    let b = g.boxr;
    // The `box` look is a plain rectangle: its corners are square whatever `radius` says.
    let r = if *style == CalloutStyle::Box {
        0.0
    } else {
        *radius
    };
    if let Some(body) = rounded_rect(&b.to_array(), r) {
        fill_path(pm, &body, *bg, 255);
    }
    if let Some(t) = tail_for(b, *tail, r, *size) {
        let mut pb = PathBuilder::new();
        pb.move_to(t.b1[0] as f32, t.b1[1] as f32);
        match style {
            CalloutStyle::Box => {
                pb.line_to(t.tip[0] as f32, t.tip[1] as f32);
                pb.line_to(t.b2[0] as f32, t.b2[1] as f32);
            }
            CalloutStyle::Bubble => {
                // Concave sides: the control points sit close to the tail's axis.
                let bc = [(t.b1[0] + t.b2[0]) / 2.0, (t.b1[1] + t.b2[1]) / 2.0];
                let ctl = |p: [f64; 2]| {
                    [
                        (bc[0] + (t.tip[0] - bc[0]) * 0.5 + (p[0] - bc[0]) * 0.1) as f32,
                        (bc[1] + (t.tip[1] - bc[1]) * 0.5 + (p[1] - bc[1]) * 0.1) as f32,
                    ]
                };
                let (c1, c2) = (ctl(t.b1), ctl(t.b2));
                pb.quad_to(c1[0], c1[1], t.tip[0] as f32, t.tip[1] as f32);
                pb.quad_to(c2[0], c2[1], t.b2[0] as f32, t.b2[1] as f32);
            }
        }
        pb.close();
        if let Some(p) = pb.finish() {
            fill_path(pm, &p, *bg, 255);
        }
    }
    if let Some(l) = &g.layer {
        let th = f64::from(l.pixmap.height()) - 2.0 * f64::from(l.pad);
        let x = b.x + (*size * PAD_X_EM).round();
        let y = b.y + (b.h - th) / 2.0;
        blit(
            pm,
            &l.pixmap,
            x.round() as i32 - l.pad,
            y.round() as i32 - l.pad,
        );
    }
    Ok(())
}

fn hole_path(s: &SpotShape) -> Option<Path> {
    match s.kind {
        SpotShapeKind::Rect => rounded_rect(&s.rect.to_array(), s.radius),
        SpotShapeKind::Ellipse => {
            let n = s.rect.normalized();
            Rect::from_xywh(n.x as f32, n.y as f32, n.w as f32, n.h as f32)
                .and_then(PathBuilder::from_oval)
        }
    }
}

/// One dim (or blur) layer over everything outside the holes (ANN-7). It only changes pixels
/// already composited, so a blur or pixelate drawn before it stays redacted.
pub(crate) fn draw_spotlight(pm: &mut Pixmap, kind: &OpKind) {
    let OpKind::Spotlight {
        shapes,
        dim,
        mode,
        strength,
    } = kind
    else {
        return;
    };
    let (w, h) = (pm.width(), pm.height());
    let Some(mut mask) = Mask::new(w, h) else {
        return;
    };
    for s in shapes {
        if let Some(p) = hole_path(s) {
            mask.fill_path(&p, FillRule::Winding, true, Transform::identity());
        }
    }
    mask.invert(); // now: coverage outside the holes
    match mode {
        SpotMode::Dim => {
            let mut p = Paint::default();
            p.set_color_rgba8(0, 0, 0, (dim * 255.0).round() as u8);
            if let Some(full) = Rect::from_xywh(0.0, 0.0, w as f32, h as f32) {
                pm.fill_rect(full, &p, Transform::identity(), Some(&mask));
            }
        }
        SpotMode::Blur => {
            let mut blurred = pm.clone();
            redact::blur(
                &mut blurred,
                Region {
                    x0: 0,
                    y0: 0,
                    x1: w,
                    y1: h,
                },
                *strength,
            );
            pm.draw_pixmap(
                0,
                0,
                blurred.as_ref(),
                &PixmapPaint::default(),
                Transform::identity(),
                Some(&mask),
            );
        }
    }
}

fn stroke_round(pm: &mut Pixmap, path: Option<Path>, c: Color, width: f32) {
    if let Some(p) = path {
        let s = Stroke {
            width,
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            ..Stroke::default()
        };
        pm.stroke_path(&p, &paint(c, 255), &s, Transform::identity(), None);
    }
}

fn dot(pm: &mut Pixmap, x: f32, y: f32, r: f32, c: Color) {
    if let Some(p) =
        Rect::from_xywh(x - r, y - r, r * 2.0, r * 2.0).and_then(PathBuilder::from_oval)
    {
        fill_path(pm, &p, c, 255);
    }
}

pub(crate) fn draw_stamp(
    pm: &mut Pixmap,
    fonts: &mut Fonts,
    at: Pt,
    kind: StampKind,
    size: f64,
    color: Color,
    label: Option<&str>,
) -> Result<(), CmError> {
    let (cx, cy, s) = (at.x as f32, at.y as f32, size as f32);
    if kind == StampKind::Label {
        let Some(t) = label.filter(|t| !t.is_empty()) else {
            return Ok(());
        };
        let style = TextStyle {
            px: s * 0.5,
            color: Color::WHITE,
            outline: None,
            bg: None,
            wrap_w: None,
            center: false,
        };
        if let Some(l) = text::layout(fonts, t, &style)? {
            let th = l.pixmap.height() as f32 - 2.0 * l.pad as f32;
            let (pw, ph) = (l.text_w.ceil() + s * 0.8, s.max(th));
            let rect = [
                f64::from(cx - pw / 2.0),
                f64::from(cy - ph / 2.0),
                f64::from(pw),
                f64::from(ph),
            ];
            if let Some(p) = rounded_rect(&rect, f64::from(ph / 2.0)) {
                fill_path(pm, &p, color, 255);
            }
            blit(
                pm,
                &l.pixmap,
                (cx - l.text_w / 2.0).round() as i32 - l.pad,
                (cy - th / 2.0).round() as i32 - l.pad,
            );
        }
        return Ok(());
    }
    // A disc in the stamp colour with a white symbol.
    dot(pm, cx, cy, s / 2.0, color);
    let w = Color::WHITE;
    let pt = |x: f32, y: f32| (cx + x * s, cy + y * s);
    let mut pb = PathBuilder::new();
    match kind {
        StampKind::Check => {
            let (a, b, c) = (pt(-0.21, 0.0), pt(-0.06, 0.15), pt(0.22, -0.16));
            pb.move_to(a.0, a.1);
            pb.line_to(b.0, b.1);
            pb.line_to(c.0, c.1);
            stroke_round(pm, pb.finish(), w, s * 0.12);
        }
        StampKind::Cross => {
            for (p, q) in [
                (pt(-0.17, -0.17), pt(0.17, 0.17)),
                (pt(0.17, -0.17), pt(-0.17, 0.17)),
            ] {
                let mut pb = PathBuilder::new();
                pb.move_to(p.0, p.1);
                pb.line_to(q.0, q.1);
                stroke_round(pm, pb.finish(), w, s * 0.12);
            }
        }
        StampKind::Exclaim => {
            let (a, b) = (pt(0.0, -0.26), pt(0.0, 0.05));
            pb.move_to(a.0, a.1);
            pb.line_to(b.0, b.1);
            stroke_round(pm, pb.finish(), w, s * 0.12);
            let d = pt(0.0, 0.24);
            dot(pm, d.0, d.1, s * 0.07, w);
        }
        StampKind::Question => {
            let (p0, c1, c2, p1) = (
                pt(-0.15, -0.12),
                pt(-0.15, -0.3),
                pt(0.15, -0.3),
                pt(0.15, -0.12),
            );
            let (c3, c4, p2) = (pt(0.15, 0.0), pt(0.0, 0.02), pt(0.0, 0.1));
            pb.move_to(p0.0, p0.1);
            pb.cubic_to(c1.0, c1.1, c2.0, c2.1, p1.0, p1.1);
            pb.cubic_to(c3.0, c3.1, c4.0, c4.1, p2.0, p2.1);
            stroke_round(pm, pb.finish(), w, s * 0.1);
            let d = pt(0.0, 0.25);
            dot(pm, d.0, d.1, s * 0.06, w);
        }
        StampKind::Label => {}
    }
    Ok(())
}

/// Step badge: disc, label (number or letters, ANN-6) and the optional text pill.
pub(crate) fn draw_step(pm: &mut Pixmap, fonts: &mut Fonts, kind: &OpKind) -> Result<(), CmError> {
    let OpKind::Step {
        at,
        n,
        size,
        color,
        style,
        start,
        text: label,
    } = kind
    else {
        return Ok(());
    };
    let r = (*size / 2.0) as f32;
    if let Some(rect) = Rect::from_xywh(at.x as f32 - r, at.y as f32 - r, r * 2.0, r * 2.0) {
        if let Some(p) = PathBuilder::from_oval(rect) {
            fill_path(pm, &p, *color, 255);
        }
    }
    // The label fits inside the disc: shrink for 2+ characters.
    let digits = step_label(*style, *start, *n);
    let len = digits.chars().count();
    let k = match len {
        0 | 1 => 0.56,
        2 => 0.46,
        _ if style.is_default() => 0.46,
        _ => 0.36,
    };
    let px = (*size as f32) * k;
    let ts = TextStyle {
        px,
        color: Color::WHITE,
        outline: None,
        bg: None,
        wrap_w: None,
        center: false,
    };
    if let Some(layer) = text::layout(fonts, &digits, &ts)? {
        let x = at.x as f32 - layer.text_w / 2.0;
        let y = at.y as f32 + 0.364 * px - layer.baseline;
        blit(
            pm,
            &layer.pixmap,
            x.round() as i32 - layer.pad,
            y.round() as i32 - layer.pad,
        );
    }
    if let Some(t) = label.as_deref().filter(|t| !t.is_empty()) {
        let lpx = (*size as f32 * 0.5).max(10.0);
        let ls = TextStyle {
            px: lpx,
            color: Color::WHITE,
            outline: None,
            bg: None,
            wrap_w: None,
            center: false,
        };
        if let Some(l) = text::layout(fonts, t, &ls)? {
            let th = l.pixmap.height() as f32 - 2.0 * l.pad as f32;
            let (padx, pady) = (lpx * 0.6, lpx * 0.3);
            let (pw, ph) = (l.text_w.ceil() + 2.0 * padx, th + 2.0 * pady);
            let gap = *size as f32 * 0.2;
            let mut x = at.x as f32 + r + gap;
            if x + pw > pm.width() as f32 {
                x = at.x as f32 - r - gap - pw; // flip to the left near the right edge
            }
            let rect = [
                f64::from(x),
                f64::from(at.y as f32 - ph / 2.0),
                f64::from(pw),
                f64::from(ph),
            ];
            if let Some(p) = rounded_rect(&rect, f64::from(ph / 2.0)) {
                fill_path(pm, &p, Color::rgb(0x11, 0x11, 0x11), 230);
            }
            blit(
                pm,
                &l.pixmap,
                (x + padx).round() as i32 - l.pad,
                (at.y as f32 - th / 2.0).round() as i32 - l.pad,
            );
        }
    }
    Ok(())
}
