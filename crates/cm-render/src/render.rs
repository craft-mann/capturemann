//! CPU renderer: paints ops onto a copy of the base image with `tiny-skia`.

use cm_core::CmError;
use image::RgbaImage;
use tiny_skia::{
    FillRule, LineCap, LineJoin, Paint, Path, PathBuilder, Pixmap, PixmapPaint, Rect, Stroke,
    Transform,
};

use crate::ops::{validate_op, AnnotationOp, Color, OpKind};
use crate::redact::{self, ordered, Region};
use crate::text::{self, Fonts, TextStyle};

/// Stateful renderer holding the font system (so repeated renders do not re-parse fonts).
pub struct Renderer {
    fonts: Fonts,
}

impl Default for Renderer {
    fn default() -> Self {
        Self::new()
    }
}

impl Renderer {
    /// Deterministic renderer: bundled Inter only, so output is identical on every machine.
    /// Text outside Inter's coverage (CJK, emoji) renders as missing-glyph boxes.
    pub fn new() -> Self {
        Renderer {
            fonts: Fonts::bundled(),
        }
    }

    /// Bundled fonts plus installed system fonts (e.g. color emoji, other scripts). Glyph output
    /// then depends on the machine's fonts; `new` already covers CJK and emoji deterministically.
    pub fn with_system_fonts() -> Self {
        Renderer {
            fonts: Fonts::with_system_fonts(),
        }
    }

    /// Renders `ops` over `base`, in order. `base` is never modified.
    ///
    /// All op coordinates are physical px of `base`. Ops partly or wholly outside the image are
    /// clipped, not rejected. If the list contains `crop` ops, the **last** one selects the
    /// visible rect (clipped to the image) and the result has that size; ops are never re-based.
    ///
    /// Errors: `invalid_input` naming `ops[i]` for out-of-range parameters or an empty crop.
    pub fn render(&mut self, base: &RgbaImage, ops: &[AnnotationOp]) -> Result<RgbaImage, CmError> {
        for (i, op) in ops.iter().enumerate() {
            validate_op(i, op)?;
        }
        let (w, h) = base.dimensions();
        let mut pm = to_pixmap(base)?;
        let mut crop: Option<(usize, [f64; 4])> = None;
        for (i, op) in ops.iter().enumerate() {
            match &op.kind {
                OpKind::Crop { rect } => crop = Some((i, rect.to_array())),
                kind => self.apply(&mut pm, kind).map_err(|e| {
                    // Name the op (R1-9): the layer-size and text-length refusals come from
                    // the text layout, which does not know its index.
                    if e.code() == cm_core::ErrorCode::InvalidInput {
                        CmError::invalid_field(&format!("ops[{i}]"), "text is too large to render")
                    } else {
                        e
                    }
                })?,
            }
        }
        let out = from_pixmap(&pm);
        match crop {
            None => Ok(out),
            Some((i, rect)) => {
                let r = Region::outward_round(rect, w, h).ok_or_else(|| {
                    CmError::invalid_field(&format!("ops[{i}]"), "crop rect is outside the image")
                })?;
                Ok(
                    image::imageops::crop_imm(&out, r.x0, r.y0, r.x1 - r.x0, r.y1 - r.y0)
                        .to_image(),
                )
            }
        }
    }

    fn apply(&mut self, pm: &mut Pixmap, kind: &OpKind) -> Result<(), CmError> {
        match kind {
            OpKind::Arrow {
                from,
                to,
                color,
                width,
            } => draw_arrow(pm, from.to_array(), to.to_array(), *color, *width),
            OpKind::Line {
                from,
                to,
                color,
                width,
            } => {
                let mut pb = PathBuilder::new();
                pb.move_to(from.x as f32, from.y as f32);
                pb.line_to(to.x as f32, to.y as f32);
                stroke(pm, pb.finish(), *color, *width, LineCap::Round);
            }
            OpKind::Rect {
                rect,
                color,
                width,
                fill,
                radius,
            } => {
                let path = rounded_rect(&rect.to_array(), *radius);
                if let (Some(f), Some(p)) = (fill, path.as_ref()) {
                    fill_path(pm, p, *f, 255);
                }
                stroke_opt(pm, path, *color, *width, LineCap::Butt);
            }
            OpKind::Ellipse { rect, color, width } => {
                let r = norm_rect(&rect.to_array())
                    .and_then(|(x, y, w, h)| Rect::from_xywh(x, y, w, h));
                stroke_opt(
                    pm,
                    r.and_then(PathBuilder::from_oval),
                    *color,
                    *width,
                    LineCap::Butt,
                );
            }
            OpKind::Highlight {
                rect,
                color,
                opacity,
            } => {
                if let Some(p) = rounded_rect(&rect.to_array(), 0.0) {
                    fill_path(pm, &p, *color, (opacity * 255.0).round() as u8);
                }
            }
            OpKind::Text {
                at,
                text,
                size,
                color,
                outline,
                bg,
            } => {
                let style = TextStyle {
                    px: *size as f32,
                    color: *color,
                    outline: *outline,
                    bg: *bg,
                    wrap_w: None,
                    center: false,
                };
                if let Some(layer) = text::layout(&mut self.fonts, text, &style)? {
                    blit(
                        pm,
                        &layer.pixmap,
                        at.x.round() as i32 - layer.pad,
                        at.y.round() as i32 - layer.pad,
                    );
                }
            }
            OpKind::Step { at, n, size, color } => {
                let r = (*size / 2.0) as f32;
                if let Some(rect) =
                    Rect::from_xywh(at.x as f32 - r, at.y as f32 - r, r * 2.0, r * 2.0)
                {
                    if let Some(p) = PathBuilder::from_oval(rect) {
                        fill_path(pm, &p, *color, 255);
                    }
                }
                // Numerals fit inside the disc: shrink for 2+ digits.
                let digits = n.to_string();
                let px = (*size as f32) * if digits.len() <= 1 { 0.56 } else { 0.46 };
                let style = TextStyle {
                    px,
                    color: Color::WHITE,
                    outline: None,
                    bg: None,
                    wrap_w: None,
                    center: false,
                };
                if let Some(layer) = text::layout(&mut self.fonts, &digits, &style)? {
                    // centre horizontally on the advance box, vertically on the cap height
                    let x = at.x as f32 - layer.text_w / 2.0;
                    let y = at.y as f32 + 0.364 * px - layer.baseline;
                    blit(
                        pm,
                        &layer.pixmap,
                        x.round() as i32 - layer.pad,
                        y.round() as i32 - layer.pad,
                    );
                }
            }
            OpKind::Blur { rect, strength } => {
                if let Some(r) = Region::outward(rect.to_array(), pm.width(), pm.height()) {
                    redact::blur(pm, r, *strength);
                }
            }
            OpKind::Pixelate { rect, block } => {
                if let Some(r) = Region::outward(rect.to_array(), pm.width(), pm.height()) {
                    redact::pixelate(pm, r, *block);
                }
            }
            OpKind::Crop { .. } => {}
        }
        Ok(())
    }
}

/// Renders with a fresh deterministic [`Renderer`]. Prefer reusing a `Renderer` in loops.
pub fn render_annotations(base: &RgbaImage, ops: &[AnnotationOp]) -> Result<RgbaImage, CmError> {
    Renderer::new().render(base, ops)
}

impl Region {
    /// Nearest-pixel rounding for crop rects, clipped to the image.
    pub(crate) fn outward_round(rect: [f64; 4], w: u32, h: u32) -> Option<Region> {
        let (x0, x1) = ordered(rect[0], rect[0] + rect[2]);
        let (y0, y1) = ordered(rect[1], rect[1] + rect[3]);
        let clip = |v: f64, max: u32| v.round().clamp(0.0, f64::from(max)) as u32;
        let (x0, x1, y0, y1) = (clip(x0, w), clip(x1, w), clip(y0, h), clip(y1, h));
        (x1 > x0 && y1 > y0).then_some(Region { x0, y0, x1, y1 })
    }
}

fn to_pixmap(img: &RgbaImage) -> Result<Pixmap, CmError> {
    let (w, h) = img.dimensions();
    let mut pm = Pixmap::new(w, h)
        .ok_or_else(|| CmError::invalid_input("image must be non-empty and within size limits"))?;
    for (d, s) in pm
        .data_mut()
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(img.as_raw().as_chunks::<4>().0.iter())
    {
        let a = u32::from(s[3]);
        for c in 0..3 {
            d[c] = ((u32::from(s[c]) * a + 127) / 255) as u8;
        }
        d[3] = s[3];
    }
    Ok(pm)
}

fn from_pixmap(pm: &Pixmap) -> RgbaImage {
    let mut raw = Vec::with_capacity(pm.data().len());
    for p in pm.pixels() {
        let c = p.demultiply();
        raw.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
    }
    RgbaImage::from_raw(pm.width(), pm.height(), raw).unwrap_or_else(|| RgbaImage::new(0, 0))
}

fn blit(pm: &mut Pixmap, layer: &Pixmap, x: i32, y: i32) {
    pm.draw_pixmap(
        x,
        y,
        layer.as_ref(),
        &PixmapPaint::default(),
        Transform::identity(),
        None,
    );
}

fn paint(c: Color, alpha: u8) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color_rgba8(c.r, c.g, c.b, alpha);
    p.anti_alias = true;
    p
}

fn fill_path(pm: &mut Pixmap, path: &Path, c: Color, alpha: u8) {
    pm.fill_path(
        path,
        &paint(c, alpha),
        FillRule::Winding,
        Transform::identity(),
        None,
    );
}

fn stroke_opt(pm: &mut Pixmap, path: Option<Path>, c: Color, width: f64, cap: LineCap) {
    if let Some(p) = path {
        let s = Stroke {
            width: width as f32,
            line_cap: cap,
            line_join: LineJoin::Miter,
            ..Stroke::default()
        };
        pm.stroke_path(&p, &paint(c, 255), &s, Transform::identity(), None);
    }
}

fn stroke(pm: &mut Pixmap, path: Option<Path>, c: Color, width: f64, cap: LineCap) {
    stroke_opt(pm, path, c, width, cap);
}

/// Normalizes a possibly negative-extent `[x, y, w, h]`.
fn norm_rect(r: &[f64; 4]) -> Option<(f32, f32, f32, f32)> {
    let (x0, x1) = ordered(r[0], r[0] + r[2]);
    let (y0, y1) = ordered(r[1], r[1] + r[3]);
    let (w, h) = ((x1 - x0) as f32, (y1 - y0) as f32);
    (w > 0.0 && h > 0.0).then_some((x0 as f32, y0 as f32, w, h))
}

fn rounded_rect(rect: &[f64; 4], radius: f64) -> Option<Path> {
    let (x, y, w, h) = norm_rect(rect)?;
    let r = (radius as f32).min(w / 2.0).min(h / 2.0);
    if r <= 0.0 {
        return PathBuilder::from_rect(Rect::from_xywh(x, y, w, h)?).into();
    }
    let k = r * (1.0 - 0.552_284_8);
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.cubic_to(x + w - k, y, x + w, y + k, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.cubic_to(x + w, y + h - k, x + w - k, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.cubic_to(x + k, y + h, x, y + h - k, x, y + h - r);
    pb.line_to(x, y + r);
    pb.cubic_to(x, y + k, x + k, y, x + r, y);
    pb.close();
    pb.finish()
}

/// Arrow: shaft plus a filled triangular head at `to`.
/// Head length `max(4*width, 8)` (clamped to the arrow length), half-width `2*width`.
fn draw_arrow(pm: &mut Pixmap, from: [f64; 2], to: [f64; 2], color: Color, width: f64) {
    let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
    let len = dx.hypot(dy);
    if len < 1e-6 {
        return;
    }
    let (ux, uy) = (dx / len, dy / len);
    let head = (4.0 * width).max(8.0).min(len);
    let half = 2.0 * width;
    let (bx, by) = (to[0] - ux * head, to[1] - uy * head);
    // shaft ends inside the head so there is no anti-aliasing seam
    let (sx, sy) = (bx + ux * head * 0.5, by + uy * head * 0.5);
    let mut pb = PathBuilder::new();
    pb.move_to(from[0] as f32, from[1] as f32);
    pb.line_to(sx as f32, sy as f32);
    stroke(pm, pb.finish(), color, width, LineCap::Butt);

    let (px, py) = (-uy, ux);
    let mut pb = PathBuilder::new();
    pb.move_to(to[0] as f32, to[1] as f32);
    pb.line_to((bx + px * half) as f32, (by + py * half) as f32);
    pb.line_to((bx - px * half) as f32, (by - py * half) as f32);
    pb.close();
    if let Some(p) = pb.finish() {
        fill_path(pm, &p, color, 255);
    }
}
