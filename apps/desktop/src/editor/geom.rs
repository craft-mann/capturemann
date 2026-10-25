//! Geometry of annotation ops: bounds, hit testing, handles, move/resize, constraints and
//! parameter edits. Pure functions over `cm_core::OpKind` in original-image px.

use cm_core::annotation::{BLUR_STRENGTH_RANGE, PIXELATE_BLOCK_RANGE};
use cm_core::{AnnotationOp, Color, OpKind, Pt, Rc};
use std::collections::HashMap;

use super::tools::{Param, ParamValue};

/// Extent of a text op relative to its `at`, measured by rendering (so hit boxes match the
/// final render exactly, including font fallback).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextBox {
    pub dx: f64,
    pub dy: f64,
    pub w: f64,
    pub h: f64,
}

/// Measures text with `cm-render` itself and caches the result.
pub struct TextMetrics {
    renderer: cm_render::Renderer,
    cache: HashMap<(String, u64), TextBox>,
}

impl Default for TextMetrics {
    fn default() -> Self {
        TextMetrics {
            renderer: cm_render::Renderer::new(),
            cache: HashMap::new(),
        }
    }
}

impl TextMetrics {
    pub fn measure(&mut self, text: &str, size: f64) -> TextBox {
        let key = (text.to_owned(), size.to_bits());
        if let Some(b) = self.cache.get(&key) {
            return *b;
        }
        let b = self.measure_uncached(text, size);
        if self.cache.len() > 256 {
            self.cache.clear();
        }
        self.cache.insert(key, b);
        b
    }

    /// Distance between baselines of consecutive lines, measured from the renderer.
    pub fn line_height(&mut self, size: f64) -> f64 {
        // i18n-ignore (text to measure, not shown)
        let one = self.measure("Ag", size).h;
        // i18n-ignore
        let two = self.measure("Ag\nAg", size).h;
        let d = two - one;
        if d > 1.0 {
            d
        } else {
            size * 1.25
        }
    }

    fn fallback(text: &str, size: f64) -> TextBox {
        let lines = text.lines().count().max(1) as f64;
        let cols = text
            .lines()
            .map(|l| l.chars().count())
            .max()
            .unwrap_or(1)
            .max(1) as f64;
        TextBox {
            dx: 0.0,
            dy: 0.0,
            w: cols * size * 0.6,
            h: lines * size * 1.25,
        }
    }

    fn measure_uncached(&mut self, text: &str, size: f64) -> TextBox {
        if text.trim().is_empty() || !size.is_finite() || size <= 0.0 {
            return Self::fallback(text, size.max(1.0));
        }
        let lines = text.lines().count().max(1) as f64;
        let cols = text
            .lines()
            .map(|l| l.chars().count())
            .max()
            .unwrap_or(1)
            .max(1) as f64;
        let pad = (size * 0.5 + 8.0).ceil();
        let w = (cols * size * 1.3 + 2.0 * pad).ceil().clamp(32.0, 4096.0) as u32;
        let h = (lines * size * 1.7 + 2.0 * pad).ceil().clamp(32.0, 4096.0) as u32;
        let base = image::RgbaImage::new(w, h);
        let op = AnnotationOp::new(
            "m",
            OpKind::Text {
                at: Pt::new(pad, pad),
                text: text.to_owned(),
                size,
                color: Color::WHITE,
                outline: Some(Color::BLACK),
                bg: None,
            },
        );
        let Ok(out) = self.renderer.render(&base, &[op]) else {
            return Self::fallback(text, size);
        };
        let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0u32, 0u32);
        for (x, y, p) in out.enumerate_pixels() {
            if p.0[3] > 8 {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x + 1);
                y1 = y1.max(y + 1);
            }
        }
        if x1 <= x0 || y1 <= y0 {
            return Self::fallback(text, size);
        }
        TextBox {
            dx: f64::from(x0) - pad,
            dy: f64::from(y0) - pad,
            w: f64::from(x1 - x0),
            h: f64::from(y1 - y0),
        }
    }
}

/// Selection handle of an op.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handle {
    From,
    To,
    Nw,
    N,
    Ne,
    E,
    Se,
    S,
    Sw,
    W,
    /// Text and step badges scale from one corner handle.
    Scale,
}

fn rect_of(kind: &OpKind) -> Option<Rc> {
    match kind {
        OpKind::Rect { rect, .. }
        | OpKind::Ellipse { rect, .. }
        | OpKind::Highlight { rect, .. }
        | OpKind::Blur { rect, .. }
        | OpKind::Pixelate { rect, .. }
        | OpKind::Crop { rect } => Some(rect.normalized()),
        _ => None,
    }
}

fn with_rect(kind: &OpKind, r: Rc) -> OpKind {
    let mut k = kind.clone();
    match &mut k {
        OpKind::Rect { rect, .. }
        | OpKind::Ellipse { rect, .. }
        | OpKind::Highlight { rect, .. }
        | OpKind::Blur { rect, .. }
        | OpKind::Pixelate { rect, .. }
        | OpKind::Crop { rect } => *rect = r,
        _ => {}
    }
    k
}

/// Axis-aligned bounds of the drawn shape (stroke width not included), original px.
pub fn bounds(kind: &OpKind, tm: &mut TextMetrics) -> Rc {
    match kind {
        OpKind::Arrow { from, to, .. } | OpKind::Line { from, to, .. } => Rc::new(
            from.x.min(to.x),
            from.y.min(to.y),
            (from.x - to.x).abs(),
            (from.y - to.y).abs(),
        ),
        OpKind::Text { at, text, size, .. } => {
            let b = tm.measure(text, *size);
            Rc::new(at.x + b.dx, at.y + b.dy, b.w, b.h)
        }
        OpKind::Step { at, size, .. } => {
            Rc::new(at.x - size / 2.0, at.y - size / 2.0, *size, *size)
        }
        other => rect_of(other).unwrap_or(Rc::new(0.0, 0.0, 0.0, 0.0)),
    }
}

fn dist_to_segment(p: Pt, a: Pt, b: Pt) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    let t = if len2 == 0.0 {
        0.0
    } else {
        (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0)
    };
    ((p.x - (a.x + t * dx)).powi(2) + (p.y - (a.y + t * dy)).powi(2)).sqrt()
}

fn inside(r: Rc, p: Pt, pad: f64) -> bool {
    p.x >= r.x - pad && p.x <= r.x + r.w + pad && p.y >= r.y - pad && p.y <= r.y + r.h + pad
}

/// Whether `p` selects this op. `tol` is the pointer tolerance in image px.
pub fn hit(kind: &OpKind, p: Pt, tol: f64, tm: &mut TextMetrics) -> bool {
    match kind {
        OpKind::Arrow {
            from, to, width, ..
        }
        | OpKind::Line {
            from, to, width, ..
        } => dist_to_segment(p, *from, *to) <= width / 2.0 + tol,
        OpKind::Rect {
            rect, width, fill, ..
        } => {
            let r = rect.normalized();
            let reach = width / 2.0 + tol;
            if !inside(r, p, reach) {
                return false;
            }
            if fill.is_some() {
                return true;
            }
            // Outline only: the band along the edge.
            let inner = Rc::new(
                r.x + reach,
                r.y + reach,
                r.w - 2.0 * reach,
                r.h - 2.0 * reach,
            );
            inner.w <= 0.0 || inner.h <= 0.0 || !inside(inner, p, 0.0)
        }
        OpKind::Ellipse { rect, width, .. } => {
            let r = rect.normalized();
            let (rx, ry) = (r.w / 2.0, r.h / 2.0);
            let reach = width / 2.0 + tol;
            if rx <= 0.0 || ry <= 0.0 {
                return inside(r, p, reach);
            }
            let d = (((p.x - (r.x + rx)) / rx).powi(2) + ((p.y - (r.y + ry)) / ry).powi(2)).sqrt();
            (d - 1.0).abs() * rx.min(ry) <= reach
        }
        OpKind::Step { at, size, .. } => {
            ((p.x - at.x).powi(2) + (p.y - at.y).powi(2)).sqrt() <= size / 2.0 + tol
        }
        OpKind::Crop { .. } => false,
        other => inside(bounds(other, tm), p, tol),
    }
}

/// Topmost op under `p`, or `None`. Crop ops are not selectable like other ops.
pub fn topmost_hit(ops: &[AnnotationOp], p: Pt, tol: f64, tm: &mut TextMetrics) -> Option<usize> {
    (0..ops.len())
        .rev()
        .find(|&i| hit(&ops[i].kind, p, tol, tm))
}

/// Handles of an op with their positions.
pub fn handles(kind: &OpKind, tm: &mut TextMetrics) -> Vec<(Handle, Pt)> {
    match kind {
        OpKind::Arrow { from, to, .. } | OpKind::Line { from, to, .. } => {
            vec![(Handle::From, *from), (Handle::To, *to)]
        }
        OpKind::Text { .. } | OpKind::Step { .. } => {
            let b = bounds(kind, tm);
            vec![(Handle::Scale, Pt::new(b.x + b.w, b.y + b.h))]
        }
        other => {
            let Some(r) = rect_of(other) else {
                return Vec::new();
            };
            let (x1, y1) = (r.x + r.w, r.y + r.h);
            let (mx, my) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
            vec![
                (Handle::Nw, Pt::new(r.x, r.y)),
                (Handle::N, Pt::new(mx, r.y)),
                (Handle::Ne, Pt::new(x1, r.y)),
                (Handle::E, Pt::new(x1, my)),
                (Handle::Se, Pt::new(x1, y1)),
                (Handle::S, Pt::new(mx, y1)),
                (Handle::Sw, Pt::new(r.x, y1)),
                (Handle::W, Pt::new(r.x, my)),
            ]
        }
    }
}

/// The handle under `p` (within `tol` image px), preferring corners and endpoints.
pub fn hit_handle(kind: &OpKind, p: Pt, tol: f64, tm: &mut TextMetrics) -> Option<Handle> {
    let mut best: Option<(f64, Handle)> = None;
    for (h, at) in handles(kind, tm) {
        let d = ((p.x - at.x).powi(2) + (p.y - at.y).powi(2)).sqrt();
        if d <= tol && best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, h));
        }
    }
    best.map(|(_, h)| h)
}

/// Moves the op by `(dx, dy)`.
pub fn translate(kind: &OpKind, dx: f64, dy: f64) -> OpKind {
    let mv = |p: &Pt| Pt::new(p.x + dx, p.y + dy);
    let mut k = kind.clone();
    match &mut k {
        OpKind::Arrow { from, to, .. } | OpKind::Line { from, to, .. } => {
            *from = mv(from);
            *to = mv(to);
        }
        OpKind::Text { at, .. } | OpKind::Step { at, .. } => *at = mv(at),
        other => {
            if let Some(r) = rect_of(other) {
                return with_rect(other, Rc::new(r.x + dx, r.y + dy, r.w, r.h));
            }
        }
    }
    k
}

/// Snaps the direction `a -> b` to a multiple of 45 degrees (ANN-1: Shift constrains
/// arrows and lines to 0/45/90), keeping the length.
pub fn constrain_angle(a: Pt, b: Pt) -> Pt {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len = (dx * dx + dy * dy).sqrt();
    if len == 0.0 {
        return b;
    }
    let step = std::f64::consts::FRAC_PI_4;
    let ang = (dy.atan2(dx) / step).round() * step;
    Pt::new(a.x + len * ang.cos(), a.y + len * ang.sin())
}

/// Moves `p` so that `anchor -> p` is a square (ANN-1.AC3).
pub fn square_from(anchor: Pt, p: Pt) -> Pt {
    let (dx, dy) = (p.x - anchor.x, p.y - anchor.y);
    let side = dx.abs().max(dy.abs());
    Pt::new(
        anchor.x + side * if dx < 0.0 { -1.0 } else { 1.0 },
        anchor.y + side * if dy < 0.0 { -1.0 } else { 1.0 },
    )
}

fn rect_between(a: Pt, b: Pt) -> Rc {
    Rc::new(a.x, a.y, b.x - a.x, b.y - a.y).normalized()
}

/// Smallest accepted drawn extent in image px; smaller drags are treated as clicks.
pub const MIN_DRAG: f64 = 3.0;

/// A new op of `tool` dragged from `a` to `b` (`shift` constrains, ANN-1.AC3). `None` when
/// the drag is too small to be a shape or the tool does not drag out a shape.
pub fn shape_from_drag(
    tool: super::tools::Tool,
    style: &super::tools::ToolStyle,
    a: Pt,
    b: Pt,
    shift: bool,
    image: (u32, u32),
) -> Option<OpKind> {
    use super::tools::Tool;
    let big_enough = |r: Rc| r.w >= MIN_DRAG && r.h >= MIN_DRAG;
    match tool {
        Tool::Arrow | Tool::Line => {
            let to = if shift { constrain_angle(a, b) } else { b };
            if ((to.x - a.x).powi(2) + (to.y - a.y).powi(2)).sqrt() < MIN_DRAG {
                return None;
            }
            Some(if tool == Tool::Arrow {
                OpKind::Arrow {
                    from: a,
                    to,
                    color: style.color,
                    width: style.width,
                }
            } else {
                OpKind::Line {
                    from: a,
                    to,
                    color: style.color,
                    width: style.width,
                }
            })
        }
        Tool::Rect | Tool::Ellipse | Tool::Highlight | Tool::Blur | Tool::Pixelate | Tool::Crop => {
            let (a, b) = if tool == Tool::Crop {
                let clamp = |p: Pt| {
                    Pt::new(
                        p.x.clamp(0.0, f64::from(image.0)),
                        p.y.clamp(0.0, f64::from(image.1)),
                    )
                };
                (clamp(a), clamp(b))
            } else {
                (a, b)
            };
            let b = if shift && matches!(tool, Tool::Rect | Tool::Ellipse) {
                square_from(a, b)
            } else {
                b
            };
            let rect = rect_between(a, b);
            if !big_enough(rect) {
                return None;
            }
            Some(match tool {
                Tool::Rect => OpKind::Rect {
                    rect,
                    color: style.color,
                    width: style.width,
                    fill: None,
                    radius: 0.0,
                },
                Tool::Ellipse => OpKind::Ellipse {
                    rect,
                    color: style.color,
                    width: style.width,
                },
                Tool::Highlight => OpKind::Highlight {
                    rect,
                    color: style.color,
                    opacity: style.opacity,
                },
                Tool::Blur => OpKind::Blur {
                    rect,
                    strength: style.strength,
                },
                Tool::Pixelate => OpKind::Pixelate {
                    rect,
                    block: style.block,
                },
                _ => OpKind::Crop { rect },
            })
        }
        _ => None,
    }
}

/// Result of dragging `handle` of `orig` to `p`.
pub fn resize(orig: &OpKind, handle: Handle, p: Pt, shift: bool, tm: &mut TextMetrics) -> OpKind {
    let mut k = orig.clone();
    match (&mut k, handle) {
        (OpKind::Arrow { from, to, .. } | OpKind::Line { from, to, .. }, Handle::From) => {
            *from = if shift { constrain_angle(*to, p) } else { p };
        }
        (OpKind::Arrow { from, to, .. } | OpKind::Line { from, to, .. }, Handle::To) => {
            *to = if shift { constrain_angle(*from, p) } else { p };
        }
        (OpKind::Text { at, size, .. }, Handle::Scale) => {
            let b = bounds(orig, tm);
            let denom = (b.x + b.w - at.x).max(1.0);
            let ratio = ((p.x - at.x) / denom).max(0.05);
            *size = (*size * ratio).clamp(8.0, 400.0);
        }
        (OpKind::Step { at, size, .. }, Handle::Scale) => {
            *size = (2.0 * (p.x - at.x).abs().max((p.y - at.y).abs())).clamp(10.0, 400.0);
        }
        _ => {
            if let Some(r) = rect_of(orig) {
                let (x1, y1) = (r.x + r.w, r.y + r.h);
                let corner = |anchor: Pt| {
                    let q = if shift { square_from(anchor, p) } else { p };
                    rect_between(anchor, q)
                };
                let nr = match handle {
                    Handle::Nw => corner(Pt::new(x1, y1)),
                    Handle::Ne => corner(Pt::new(r.x, y1)),
                    Handle::Se => corner(Pt::new(r.x, r.y)),
                    Handle::Sw => corner(Pt::new(x1, r.y)),
                    Handle::N => rect_between(Pt::new(r.x, y1), Pt::new(x1, p.y)),
                    Handle::S => rect_between(Pt::new(r.x, r.y), Pt::new(x1, p.y)),
                    Handle::W => rect_between(Pt::new(x1, r.y), Pt::new(p.x, y1)),
                    Handle::E => rect_between(Pt::new(r.x, r.y), Pt::new(p.x, y1)),
                    _ => r,
                };
                return with_rect(orig, nr);
            }
        }
    }
    k
}

/// Keyboard resize (Alt+arrows): grows the bottom-right / end point by `(dx, dy)`.
pub fn resize_by(kind: &OpKind, dx: f64, dy: f64, tm: &mut TextMetrics) -> OpKind {
    let mut k = kind.clone();
    match &mut k {
        OpKind::Arrow { to, .. } | OpKind::Line { to, .. } => {
            to.x += dx;
            to.y += dy;
        }
        OpKind::Text { size, .. } | OpKind::Step { size, .. } => {
            *size = (*size + dx + dy).clamp(8.0, 400.0);
        }
        other => {
            if let Some(r) = rect_of(other) {
                let _ = tm;
                let nr = Rc::new(r.x, r.y, (r.w + dx).max(MIN_DRAG), (r.h + dy).max(MIN_DRAG));
                return with_rect(other, nr);
            }
        }
    }
    k
}

/// Renumbers step badges 1, 2, 3 in list order (ANN-1.AC2).
pub fn renumber_steps(ops: &mut [AnnotationOp]) {
    let mut n = 0;
    for op in ops {
        if let OpKind::Step { n: slot, .. } = &mut op.kind {
            n += 1;
            *slot = n;
        }
    }
}

/// Which inspector controls apply to this op.
pub fn params_of(kind: &OpKind) -> &'static [Param] {
    super::tools::Tool::for_kind(kind).params()
}

/// Current value of `p` on this op, if it has that parameter.
pub fn param_value(kind: &OpKind, p: Param) -> Option<ParamValue> {
    match (kind, p) {
        (
            OpKind::Arrow { color, .. }
            | OpKind::Line { color, .. }
            | OpKind::Rect { color, .. }
            | OpKind::Ellipse { color, .. }
            | OpKind::Text { color, .. }
            | OpKind::Highlight { color, .. }
            | OpKind::Step { color, .. },
            Param::Color,
        ) => Some(ParamValue::Color(*color)),
        (
            OpKind::Arrow { width, .. }
            | OpKind::Line { width, .. }
            | OpKind::Rect { width, .. }
            | OpKind::Ellipse { width, .. },
            Param::Width,
        ) => Some(ParamValue::Width(*width)),
        (OpKind::Text { size, .. }, Param::TextSize) => Some(ParamValue::TextSize(*size)),
        (OpKind::Step { size, .. }, Param::StepSize) => Some(ParamValue::StepSize(*size)),
        (OpKind::Highlight { opacity, .. }, Param::Opacity) => Some(ParamValue::Opacity(*opacity)),
        (OpKind::Blur { strength, .. }, Param::Strength) => Some(ParamValue::Strength(*strength)),
        (OpKind::Pixelate { block, .. }, Param::Block) => Some(ParamValue::Block(*block)),
        _ => None,
    }
}

/// The op with one parameter changed, or `None` when the op has no such parameter
/// (ANN-1.AC5: only the selected op changes). Values are clamped to the schema's ranges.
pub fn with_param(kind: &OpKind, v: ParamValue) -> Option<OpKind> {
    param_value(kind, v.param())?;
    let mut k = kind.clone();
    match (&mut k, v) {
        (
            OpKind::Arrow { color, .. }
            | OpKind::Line { color, .. }
            | OpKind::Rect { color, .. }
            | OpKind::Ellipse { color, .. }
            | OpKind::Text { color, .. }
            | OpKind::Highlight { color, .. }
            | OpKind::Step { color, .. },
            ParamValue::Color(c),
        ) => *color = c,
        (
            OpKind::Arrow { width, .. }
            | OpKind::Line { width, .. }
            | OpKind::Rect { width, .. }
            | OpKind::Ellipse { width, .. },
            ParamValue::Width(w),
        ) => *width = w.clamp(1.0, 64.0),
        (OpKind::Text { size, .. }, ParamValue::TextSize(s))
        | (OpKind::Step { size, .. }, ParamValue::StepSize(s)) => *size = s.clamp(8.0, 400.0),
        (OpKind::Highlight { opacity, .. }, ParamValue::Opacity(o)) => {
            *opacity = o.clamp(0.05, 1.0)
        }
        (OpKind::Blur { strength, .. }, ParamValue::Strength(s)) => {
            *strength = s.clamp(*BLUR_STRENGTH_RANGE.start(), *BLUR_STRENGTH_RANGE.end())
        }
        (OpKind::Pixelate { block, .. }, ParamValue::Block(b)) => {
            *block = b.clamp(*PIXELATE_BLOCK_RANGE.start(), *PIXELATE_BLOCK_RANGE.end())
        }
        _ => return None,
    }
    Some(k)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::tools::{Tool, ToolStyle};

    fn style() -> ToolStyle {
        ToolStyle::defaults(Tool::Rect)
    }

    fn rect_op(x: f64, y: f64, w: f64, h: f64) -> OpKind {
        OpKind::Rect {
            rect: Rc::new(x, y, w, h),
            color: Color::BLACK,
            width: 4.0,
            fill: None,
            radius: 0.0,
        }
    }

    /// ANN-1.AC1: dragging an arrow from (120, 300) to (380, 210) yields exactly those
    /// coordinates, red and 4 px.
    #[test]
    fn arrow_drag_keeps_the_dragged_coordinates() {
        let k = shape_from_drag(
            Tool::Arrow,
            &ToolStyle::defaults(Tool::Arrow),
            Pt::new(120.0, 300.0),
            Pt::new(380.0, 210.0),
            false,
            (1000, 800),
        )
        .unwrap();
        assert_eq!(
            k,
            OpKind::Arrow {
                from: Pt::new(120.0, 300.0),
                to: Pt::new(380.0, 210.0),
                color: Color::rgb(0xFF, 0x3B, 0x3B),
                width: 4.0
            }
        );
    }

    /// ANN-1.AC3: Shift makes a rectangle's width equal its height, in every drag direction;
    /// the same for ellipses (circles).
    #[test]
    fn shift_makes_squares_and_circles() {
        for (b, name) in [
            (Pt::new(180.0, 140.0), "down-right"),
            (Pt::new(20.0, 140.0), "down-left"),
            (Pt::new(180.0, 10.0), "up-right"),
            (Pt::new(5.0, 5.0), "up-left"),
        ] {
            for tool in [Tool::Rect, Tool::Ellipse] {
                let k = shape_from_drag(tool, &style(), Pt::new(100.0, 80.0), b, true, (500, 500))
                    .unwrap();
                let r = rect_of(&k).unwrap();
                assert!((r.w - r.h).abs() < 1e-9, "{tool:?} {name}: {r:?}");
            }
        }
        let free = shape_from_drag(
            Tool::Rect,
            &style(),
            Pt::new(100.0, 80.0),
            Pt::new(180.0, 140.0),
            false,
            (500, 500),
        )
        .unwrap();
        let r = rect_of(&free).unwrap();
        assert_eq!((r.w, r.h), (80.0, 60.0));
    }

    /// ANN-1: Shift constrains arrows and lines to 0/45/90 degrees.
    #[test]
    fn shift_snaps_lines_to_45_degree_steps() {
        let a = Pt::new(0.0, 0.0);
        for (b, expect) in [
            (Pt::new(100.0, 8.0), (100.0, 0.0)),
            (Pt::new(60.0, 55.0), (57.5, 57.5)),
            (Pt::new(3.0, -90.0), (0.0, -90.0)),
        ] {
            let c = constrain_angle(a, b);
            let len = (b.x * b.x + b.y * b.y).sqrt();
            let clen = (c.x * c.x + c.y * c.y).sqrt();
            assert!((len - clen).abs() < 1e-9, "length kept");
            if expect.1 == 0.0 || expect.0 == 0.0 {
                assert!(
                    (c.x - expect.0).abs() < 1.0 && (c.y - expect.1).abs() < 1.0,
                    "{c:?}"
                );
            } else {
                assert!((c.x.abs() - c.y.abs()).abs() < 1e-9);
            }
        }
    }

    #[test]
    fn tiny_drags_do_not_create_shapes() {
        let s = style();
        assert!(shape_from_drag(
            Tool::Rect,
            &s,
            Pt::new(1.0, 1.0),
            Pt::new(2.0, 2.0),
            false,
            (9, 9)
        )
        .is_none());
        assert!(shape_from_drag(
            Tool::Arrow,
            &s,
            Pt::new(1.0, 1.0),
            Pt::new(1.0, 2.0),
            false,
            (9, 9)
        )
        .is_none());
        // Select, text and step do not drag out shapes.
        assert!(shape_from_drag(
            Tool::Step,
            &s,
            Pt::new(0.0, 0.0),
            Pt::new(90.0, 90.0),
            false,
            (99, 99)
        )
        .is_none());
    }

    /// ANN-1: crop is clamped to the image so the renderer never sees an outside rect.
    #[test]
    fn crop_drag_is_clamped_to_the_image() {
        let k = shape_from_drag(
            Tool::Crop,
            &style(),
            Pt::new(-50.0, -20.0),
            Pt::new(1500.0, 900.0),
            false,
            (1200, 800),
        )
        .unwrap();
        assert_eq!(
            k,
            OpKind::Crop {
                rect: Rc::new(0.0, 0.0, 1200.0, 800.0)
            }
        );
    }

    #[test]
    fn outline_rect_is_selected_on_its_edge_not_its_hollow_middle() {
        let mut tm = TextMetrics::default();
        let r = rect_op(100.0, 100.0, 200.0, 100.0);
        assert!(hit(&r, Pt::new(100.0, 150.0), 3.0, &mut tm), "left edge");
        assert!(
            hit(&r, Pt::new(200.0, 98.0), 3.0, &mut tm),
            "top edge within tolerance"
        );
        assert!(
            !hit(&r, Pt::new(200.0, 150.0), 3.0, &mut tm),
            "hollow middle"
        );
        assert!(!hit(&r, Pt::new(50.0, 50.0), 3.0, &mut tm));
        let mut filled = r.clone();
        if let OpKind::Rect { fill, .. } = &mut filled {
            *fill = Some(Color::WHITE);
        }
        assert!(hit(&filled, Pt::new(200.0, 150.0), 3.0, &mut tm));
    }

    #[test]
    fn arrows_ellipses_steps_and_highlights_hit_test() {
        let mut tm = TextMetrics::default();
        let a = OpKind::Arrow {
            from: Pt::new(0.0, 0.0),
            to: Pt::new(100.0, 0.0),
            color: Color::BLACK,
            width: 4.0,
        };
        assert!(hit(&a, Pt::new(50.0, 4.0), 3.0, &mut tm));
        assert!(!hit(&a, Pt::new(50.0, 20.0), 3.0, &mut tm));
        let e = OpKind::Ellipse {
            rect: Rc::new(0.0, 0.0, 200.0, 100.0),
            color: Color::BLACK,
            width: 4.0,
        };
        assert!(hit(&e, Pt::new(0.0, 50.0), 3.0, &mut tm));
        assert!(hit(&e, Pt::new(100.0, 0.0), 3.0, &mut tm));
        assert!(!hit(&e, Pt::new(100.0, 50.0), 3.0, &mut tm));
        let s = OpKind::Step {
            at: Pt::new(50.0, 50.0),
            n: 1,
            size: 28.0,
            color: Color::BLACK,
        };
        assert!(hit(&s, Pt::new(60.0, 55.0), 2.0, &mut tm));
        assert!(!hit(&s, Pt::new(80.0, 80.0), 2.0, &mut tm));
        let h = OpKind::Highlight {
            rect: Rc::new(10.0, 10.0, 50.0, 20.0),
            color: Color::BLACK,
            opacity: 0.3,
        };
        assert!(hit(&h, Pt::new(30.0, 20.0), 2.0, &mut tm));
    }

    /// Text hit boxes come from the renderer itself, so clicking the drawn glyphs selects it.
    #[test]
    fn text_bounds_come_from_the_renderer() {
        let mut tm = TextMetrics::default();
        let t = OpKind::Text {
            at: Pt::new(130.0, 320.0),
            text: "Click Save".into(),
            size: 24.0,
            color: Color::WHITE,
            outline: Some(Color::BLACK),
            bg: None,
        };
        let b = bounds(&t, &mut tm);
        assert!(b.w > 60.0 && b.w < 200.0, "{b:?}");
        assert!(b.h > 12.0 && b.h < 50.0, "{b:?}");
        assert!(
            (b.x - 130.0).abs() < 12.0 && (b.y - 320.0).abs() < 20.0,
            "{b:?}"
        );
        assert!(hit(
            &t,
            Pt::new(b.x + b.w / 2.0, b.y + b.h / 2.0),
            2.0,
            &mut tm
        ));
        assert!(!hit(&t, Pt::new(b.x + b.w + 40.0, b.y), 2.0, &mut tm));
        // A second line makes the box taller.
        let t2 = match t {
            OpKind::Text {
                at,
                size,
                color,
                outline,
                bg,
                ..
            } => OpKind::Text {
                at,
                text: "Click\nSave".into(),
                size,
                color,
                outline,
                bg,
            },
            _ => unreachable!(),
        };
        assert!(bounds(&t2, &mut tm).h > b.h * 1.5);
    }

    #[test]
    fn resize_corner_flips_through_the_anchor_and_edges_move_one_axis() {
        let mut tm = TextMetrics::default();
        let r = rect_op(100.0, 100.0, 100.0, 50.0);
        let k = resize(&r, Handle::Se, Pt::new(250.0, 90.0), false, &mut tm);
        assert_eq!(rect_of(&k).unwrap(), Rc::new(100.0, 90.0, 150.0, 10.0));
        let k = resize(&r, Handle::Nw, Pt::new(300.0, 300.0), false, &mut tm);
        assert_eq!(rect_of(&k).unwrap(), Rc::new(200.0, 150.0, 100.0, 150.0));
        let k = resize(&r, Handle::E, Pt::new(400.0, 999.0), false, &mut tm);
        assert_eq!(rect_of(&k).unwrap(), Rc::new(100.0, 100.0, 300.0, 50.0));
        let k = resize(&r, Handle::N, Pt::new(999.0, 60.0), false, &mut tm);
        assert_eq!(rect_of(&k).unwrap(), Rc::new(100.0, 60.0, 100.0, 90.0));
        let k = resize(&r, Handle::Se, Pt::new(260.0, 130.0), true, &mut tm);
        let nr = rect_of(&k).unwrap();
        assert_eq!(nr.w, nr.h);
    }

    #[test]
    fn arrow_endpoints_and_step_scale_resize() {
        let mut tm = TextMetrics::default();
        let a = OpKind::Arrow {
            from: Pt::new(0.0, 0.0),
            to: Pt::new(10.0, 10.0),
            color: Color::BLACK,
            width: 4.0,
        };
        let k = resize(&a, Handle::To, Pt::new(50.0, 7.0), false, &mut tm);
        assert!(
            matches!(k, OpKind::Arrow { to, from, .. } if to == Pt::new(50.0, 7.0) && from == Pt::new(0.0, 0.0))
        );
        let k = resize(&a, Handle::From, Pt::new(-5.0, -5.0), false, &mut tm);
        assert!(matches!(k, OpKind::Arrow { from, .. } if from == Pt::new(-5.0, -5.0)));
        let s = OpKind::Step {
            at: Pt::new(100.0, 100.0),
            n: 1,
            size: 28.0,
            color: Color::BLACK,
        };
        let k = resize(&s, Handle::Scale, Pt::new(130.0, 120.0), false, &mut tm);
        assert!(matches!(k, OpKind::Step { size, .. } if (size - 60.0).abs() < 1e-9));
    }

    #[test]
    fn handles_exist_for_every_op_kind() {
        let mut tm = TextMetrics::default();
        assert_eq!(handles(&rect_op(0.0, 0.0, 10.0, 10.0), &mut tm).len(), 8);
        let arrow = OpKind::Line {
            from: Pt::new(0.0, 0.0),
            to: Pt::new(5.0, 5.0),
            color: Color::BLACK,
            width: 2.0,
        };
        assert_eq!(handles(&arrow, &mut tm).len(), 2);
        let blur = OpKind::Blur {
            rect: Rc::new(0.0, 0.0, 10.0, 10.0),
            strength: 12,
        };
        assert_eq!(handles(&blur, &mut tm).len(), 8);
        assert_eq!(
            hit_handle(&blur, Pt::new(10.5, 10.5), 2.0, &mut tm),
            Some(Handle::Se)
        );
        assert_eq!(hit_handle(&blur, Pt::new(50.0, 50.0), 2.0, &mut tm), None);
    }

    /// ANN-1.AC2: deleting step 2 of 3 leaves badges 1 and 2.
    #[test]
    fn steps_renumber_in_list_order() {
        let step = |id: &str, n| {
            AnnotationOp::new(
                id,
                OpKind::Step {
                    at: Pt::new(0.0, 0.0),
                    n,
                    size: 28.0,
                    color: Color::BLACK,
                },
            )
        };
        let mut ops = vec![step("a", 1), step("b", 2), step("c", 3)];
        ops.remove(1);
        renumber_steps(&mut ops);
        let ns: Vec<u32> = ops
            .iter()
            .filter_map(|o| match o.kind {
                OpKind::Step { n, .. } => Some(n),
                _ => None,
            })
            .collect();
        assert_eq!(ns, vec![1, 2]);
        assert_eq!(ops[1].id, "c", "ids stay stable");
    }

    /// ANN-1.AC5: changing a color touches only that op's color and keeps everything else.
    #[test]
    fn param_edit_changes_only_the_named_field_and_clamps() {
        let r = rect_op(1.0, 2.0, 30.0, 40.0);
        let blue = Color::rgb(0x2E, 0x8B, 0xFF);
        let k = with_param(&r, ParamValue::Color(blue)).unwrap();
        assert_eq!(
            k,
            OpKind::Rect {
                rect: Rc::new(1.0, 2.0, 30.0, 40.0),
                color: blue,
                width: 4.0,
                fill: None,
                radius: 0.0
            }
        );
        assert!(
            with_param(&r, ParamValue::Strength(9)).is_none(),
            "rect has no blur strength"
        );
        let blur = OpKind::Blur {
            rect: Rc::new(0.0, 0.0, 9.0, 9.0),
            strength: 12,
        };
        assert!(matches!(
            with_param(&blur, ParamValue::Strength(99)),
            Some(OpKind::Blur { strength: 32, .. })
        ));
        assert!(matches!(
            with_param(&blur, ParamValue::Strength(1)),
            Some(OpKind::Blur { strength: 4, .. })
        ));
        assert!(
            with_param(&blur, ParamValue::Color(blue)).is_none(),
            "redactions have no color"
        );
        let px = OpKind::Pixelate {
            rect: Rc::new(0.0, 0.0, 9.0, 9.0),
            block: 12,
        };
        assert!(matches!(
            with_param(&px, ParamValue::Block(2)),
            Some(OpKind::Pixelate { block: 6, .. })
        ));
    }

    #[test]
    fn translate_moves_every_kind() {
        let a = OpKind::Arrow {
            from: Pt::new(1.0, 1.0),
            to: Pt::new(2.0, 2.0),
            color: Color::BLACK,
            width: 4.0,
        };
        assert!(
            matches!(translate(&a, 10.0, 5.0), OpKind::Arrow { from, to, .. } if from == Pt::new(11.0, 6.0) && to == Pt::new(12.0, 7.0))
        );
        let c = OpKind::Crop {
            rect: Rc::new(0.0, 0.0, 5.0, 5.0),
        };
        assert_eq!(
            translate(&c, 3.0, 4.0),
            OpKind::Crop {
                rect: Rc::new(3.0, 4.0, 5.0, 5.0)
            }
        );
    }

    #[test]
    fn topmost_wins_and_crop_is_never_hit() {
        let mut tm = TextMetrics::default();
        let ops = vec![
            AnnotationOp::new(
                "a",
                OpKind::Highlight {
                    rect: Rc::new(0.0, 0.0, 100.0, 100.0),
                    color: Color::BLACK,
                    opacity: 0.3,
                },
            ),
            AnnotationOp::new(
                "b",
                OpKind::Blur {
                    rect: Rc::new(20.0, 20.0, 30.0, 30.0),
                    strength: 12,
                },
            ),
            AnnotationOp::new(
                "c",
                OpKind::Crop {
                    rect: Rc::new(0.0, 0.0, 100.0, 100.0),
                },
            ),
        ];
        assert_eq!(
            topmost_hit(&ops, Pt::new(30.0, 30.0), 2.0, &mut tm),
            Some(1)
        );
        assert_eq!(
            topmost_hit(&ops, Pt::new(80.0, 80.0), 2.0, &mut tm),
            Some(0)
        );
        assert_eq!(topmost_hit(&ops, Pt::new(500.0, 80.0), 2.0, &mut tm), None);
    }
}
