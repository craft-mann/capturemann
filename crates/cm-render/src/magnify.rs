//! Zoom Lens (ANN-8): the loupe is rendered again from `original.png` at `zoom` times the scale,
//! with the vector ops listed before the lens and **every** redaction on top, so it is sharp,
//! shows what was drawn and never reveals what a blur or pixelate hides.

use cm_core::annotation::{Color, MagnifyShape, OpKind, Pt, Rc};
use cm_core::CmError;
use image::RgbaImage;
use tiny_skia::{
    FillRule, LineCap, Mask, PathBuilder, Pixmap, PixmapPaint, Rect, Stroke, Transform,
};

use crate::render::{blit, fill_path, paint, rounded_rect, Renderer};
use crate::text::{self, TextStyle};

/// Loupe corner radius at scale 1 (`rounded` shape).
const ROUNDED_RADIUS: f64 = 12.0;
/// Largest layer a loupe may allocate, in px (a 2x export of a 32 Mpx loupe stays allowed).
const MAX_LAYER_PX: f64 = 130_000_000.0;

/// Maps image coordinates to layer coordinates: `(p - origin) * s`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Xf {
    pub ox: f64,
    pub oy: f64,
    pub s: f64,
}

impl Xf {
    pub fn is_identity(&self) -> bool {
        self.ox == 0.0 && self.oy == 0.0 && self.s == 1.0
    }
    fn pt(&self, p: Pt) -> Pt {
        Pt::new((p.x - self.ox) * self.s, (p.y - self.oy) * self.s)
    }
    fn rc(&self, r: Rc) -> Rc {
        Rc::new(
            (r.x - self.ox) * self.s,
            (r.y - self.oy) * self.s,
            r.w * self.s,
            r.h * self.s,
        )
    }
    fn len(&self, v: f64) -> f64 {
        v * self.s
    }
    fn count(&self, v: u32) -> u32 {
        ((f64::from(v) * self.s).round() as u32).max(1)
    }
}

/// The op with every length, point and rect mapped through `xf` (a copy at another scale).
pub(crate) fn transform_op(kind: &OpKind, xf: &Xf) -> OpKind {
    let mut k = kind.clone();
    if xf.is_identity() {
        return k;
    }
    match &mut k {
        OpKind::Arrow {
            from, to, width, ..
        }
        | OpKind::Line {
            from, to, width, ..
        } => {
            *from = xf.pt(*from);
            *to = xf.pt(*to);
            *width = xf.len(*width);
        }
        OpKind::Rect {
            rect,
            width,
            radius,
            ..
        } => {
            *rect = xf.rc(*rect);
            *width = xf.len(*width);
            *radius = xf.len(*radius);
        }
        OpKind::Ellipse { rect, width, .. } => {
            *rect = xf.rc(*rect);
            *width = xf.len(*width);
        }
        OpKind::Text { at, size, .. } => {
            *at = xf.pt(*at);
            *size = xf.len(*size);
        }
        OpKind::Highlight { rect, .. } => *rect = xf.rc(*rect),
        OpKind::Step { at, size, .. } => {
            *at = xf.pt(*at);
            *size = xf.len(*size);
        }
        OpKind::Callout {
            rect,
            tail,
            size,
            radius,
            ..
        } => {
            *rect = xf.rc(*rect);
            *tail = xf.pt(*tail);
            *size = xf.len(*size);
            *radius = xf.len(*radius);
        }
        OpKind::Spotlight {
            shapes, strength, ..
        } => {
            for s in shapes.iter_mut() {
                s.rect = xf.rc(s.rect);
                s.radius = xf.len(s.radius);
            }
            *strength = xf.count(*strength);
        }
        OpKind::Stamp { at, size, .. } => {
            *at = xf.pt(*at);
            *size = xf.len(*size);
        }
        OpKind::Blur { rect, strength } => {
            *rect = xf.rc(*rect);
            *strength = xf.count(*strength);
        }
        OpKind::Pixelate { rect, block } => {
            *rect = xf.rc(*rect);
            *block = xf.count(*block);
        }
        OpKind::Crop { rect } => *rect = xf.rc(*rect),
        OpKind::Magnify { .. } => {}
        OpKind::Retype { rect, size, .. } => {
            *rect = xf.rc(*rect);
            *size = xf.len(*size);
        }
        OpKind::MoveArea { source, to, .. } => {
            *source = xf.rc(*source);
            *to = to.map(|t| xf.pt(t));
        }
        OpKind::Blueprint { boxes, radius, .. } => {
            for b in boxes.iter_mut() {
                *b = xf.rc(*b);
            }
            *radius = xf.len(*radius);
        }
    }
    k
}

/// The source of a lens as a whole-pixel region of the image: `[x0, y0, x1, y1]`, clamped.
pub(crate) fn source_region(source: Rc, w: u32, h: u32) -> Option<[u32; 4]> {
    let n = source.normalized();
    let clip = |v: f64, max: u32| v.round().clamp(0.0, f64::from(max)) as u32;
    let (x0, y0) = (clip(n.x, w), clip(n.y, h));
    let (x1, y1) = (clip(n.x + n.w, w), clip(n.y + n.h, h));
    (x1 > x0 && y1 > y0).then_some([x0, y0, x1, y1])
}

fn shape_path(shape: MagnifyShape, r: [f64; 4], radius: f64) -> Option<tiny_skia::Path> {
    match shape {
        MagnifyShape::Circle => Rect::from_xywh(r[0] as f32, r[1] as f32, r[2] as f32, r[3] as f32)
            .and_then(PathBuilder::from_oval),
        MagnifyShape::Rounded => rounded_rect(&r, radius),
        MagnifyShape::Rect => rounded_rect(&r, 0.0),
    }
}

/// The two connector lines between the source rectangle and the loupe rectangle (each
/// `[from, to]`); none when they overlap.
fn connector_lines(s: [f64; 4], l: [f64; 4]) -> Option<[[[f64; 2]; 2]; 2]> {
    let (sx1, sy1, lx1, ly1) = (s[0] + s[2], s[1] + s[3], l[0] + l[2], l[1] + l[3]);
    let c = |x: f64, y: f64| [x, y];
    if l[0] >= sx1 {
        Some([[c(sx1, s[1]), c(l[0], l[1])], [c(sx1, sy1), c(l[0], ly1)]])
    } else if lx1 <= s[0] {
        Some([[c(s[0], s[1]), c(lx1, l[1])], [c(s[0], sy1), c(lx1, ly1)]])
    } else if l[1] >= sy1 {
        Some([[c(s[0], sy1), c(l[0], l[1])], [c(sx1, sy1), c(lx1, l[1])]])
    } else if ly1 <= s[1] {
        Some([[c(s[0], s[1]), c(l[0], ly1)], [c(sx1, s[1]), c(lx1, ly1)]])
    } else {
        None
    }
}

impl Renderer {
    /// Draws the lens `ops[index]` onto `pm`, a layer at scale `s` of the whole image.
    pub(crate) fn draw_magnify(
        &mut self,
        pm: &mut Pixmap,
        base: &RgbaImage,
        ops: &[cm_core::AnnotationOp],
        index: usize,
        s: f64,
    ) -> Result<(), CmError> {
        let OpKind::Magnify {
            source,
            at,
            zoom,
            shape,
            border,
            border_width,
            connector,
            badge,
        } = &ops[index].kind
        else {
            return Ok(());
        };
        let Some(reg) = source_region(*source, base.width(), base.height()) else {
            return Ok(());
        };
        let z = zoom * s;
        let (sw, sh) = (f64::from(reg[2] - reg[0]), f64::from(reg[3] - reg[1]));
        let (lw, lh) = ((sw * z).round().max(1.0), (sh * z).round().max(1.0));
        if lw * lh > MAX_LAYER_PX {
            return Err(CmError::invalid_field(
                &format!("ops[{index}]"),
                "the loupe is too large to render",
            ));
        }
        let mut layer = self.layer(base, reg, z)?;
        let xf = Xf {
            ox: f64::from(reg[0]),
            oy: f64::from(reg[1]),
            s: z,
        };
        // Ops listed before the lens, in order, then every later redaction (it hides every copy
        // of the pixels); other lenses and crops are not inside a loupe. An error inside the
        // loupe (an oversized text layer) skips that op only.
        let before = ops[..index].iter();
        let after = ops[index + 1..].iter().filter(|o| o.kind.is_redaction());
        for op in before.chain(after) {
            if matches!(op.kind, OpKind::Magnify { .. } | OpKind::Crop { .. }) {
                continue;
            }
            let _ = self.apply(&mut layer, &transform_op(&op.kind, &xf));
        }

        // Where it goes in the output (scale s): centred on `at`.
        let (lx, ly) = ((at.x * s - lw / 2.0).round(), (at.y * s - lh / 2.0).round());
        let lrect = [lx, ly, lw, lh];
        let radius = ROUNDED_RADIUS * s;
        let bw = border_width * s;
        let (px, py) = (lx as i32, ly as i32);

        if *connector {
            let src = [
                (f64::from(reg[0])) * s,
                (f64::from(reg[1])) * s,
                sw * s,
                sh * s,
            ];
            if let Some(lines) = connector_lines(src, lrect) {
                let w = (bw / 2.0).clamp(s.max(1.0), 3.0 * s.max(1.0)) as f32;
                for [a, b] in lines {
                    let mut pb = PathBuilder::new();
                    pb.move_to(a[0] as f32, a[1] as f32);
                    pb.line_to(b[0] as f32, b[1] as f32);
                    if let Some(p) = pb.finish() {
                        pm.stroke_path(
                            &p,
                            &paint(*border, 255),
                            &Stroke {
                                width: w,
                                line_cap: LineCap::Butt,
                                ..Stroke::default()
                            },
                            Transform::identity(),
                            None,
                        );
                    }
                }
            }
        }

        // The loupe, clipped to its shape.
        if let (Some(path), Some(mut mask)) = (
            shape_path(*shape, lrect, radius),
            Mask::new(pm.width(), pm.height()),
        ) {
            mask.fill_path(&path, FillRule::Winding, true, Transform::identity());
            pm.draw_pixmap(
                px,
                py,
                layer.as_ref(),
                &PixmapPaint::default(),
                Transform::identity(),
                Some(&mask),
            );
        }
        if bw > 0.0 {
            let half = bw / 2.0;
            let inset = [lx + half, ly + half, lw - bw, lh - bw];
            if let Some(p) = shape_path(*shape, inset, (radius - half).max(0.0)) {
                pm.stroke_path(
                    &p,
                    &paint(*border, 255),
                    &Stroke {
                        width: bw as f32,
                        ..Stroke::default()
                    },
                    Transform::identity(),
                    None,
                );
            }
        }
        if *badge {
            self.draw_badge(pm, *shape, lrect, *zoom, s)?;
        }
        Ok(())
    }

    /// The `×3` pill at the bottom right of the loupe.
    fn draw_badge(
        &mut self,
        pm: &mut Pixmap,
        shape: MagnifyShape,
        l: [f64; 4],
        zoom: f64,
        s: f64,
    ) -> Result<(), CmError> {
        let label = format!("\u{d7}{zoom}");
        let px = ((l[2].min(l[3]) * 0.12).clamp(11.0 * s, 28.0 * s)) as f32;
        let style = TextStyle {
            px,
            color: Color::WHITE,
            outline: None,
            bg: None,
            wrap_w: None,
            center: false,
        };
        let Some(t) = text::layout(&mut self.fonts, &label, &style)? else {
            return Ok(());
        };
        let th = f64::from(t.pixmap.height()) - 2.0 * f64::from(t.pad);
        let (padx, pady) = (f64::from(px) * 0.45, f64::from(px) * 0.2);
        let (pw, ph) = (f64::from(t.text_w.ceil()) + 2.0 * padx, th + 2.0 * pady);
        let margin = 6.0 * s;
        // Bottom right corner of the pill: inside the corner, or on the ellipse for a circle.
        let (right, bottom) = if shape == MagnifyShape::Circle {
            let k = std::f64::consts::FRAC_1_SQRT_2 * 0.9;
            (
                l[0] + l[2] / 2.0 + l[2] / 2.0 * k,
                l[1] + l[3] / 2.0 + l[3] / 2.0 * k,
            )
        } else {
            (l[0] + l[2] - margin, l[1] + l[3] - margin)
        };
        let rect = [right - pw, bottom - ph, pw, ph];
        if let Some(p) = rounded_rect(&rect, ph / 2.0) {
            fill_path(pm, &p, Color::rgb(0x11, 0x11, 0x11), 230);
        }
        blit(
            pm,
            &t.pixmap,
            (rect[0] + padx).round() as i32 - t.pad,
            (rect[1] + pady).round() as i32 - t.pad,
        );
        Ok(())
    }
}
