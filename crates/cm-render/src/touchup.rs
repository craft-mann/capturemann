//! Base edits (ANN-9 Retype, ANN-10 Move area, ANN-11 Blueprint) and the fill they share.
//!
//! The fill is measured **once**, from the ring of pixels just outside a box, and stored in the
//! op (`Fill`), so the editor, the CLI and agents render the same. A render only paints stored
//! fills; the measuring functions here are what the editor and `suggest_blueprint` call when an
//! op is created. No model and no network are involved.

use cm_core::annotation::{
    BlueprintColor, BlueprintStyle, Color, Fill, FillKind, OpKind, Rc, TextAlign,
};
use cm_core::CmError;
use image::RgbaImage;
use tiny_skia::{FillRule, Mask, PathBuilder, Pixmap, PixmapPaint, Rect, Transform};

use crate::redact::Region;
use crate::render::{fill_path, rounded_rect};
use crate::text::{self, Fonts, TextStyle};

/// Width of the ring measured around a box, in px.
pub const RING_PX: u32 = 3;
/// A solid fill is chosen when the ring is flat to this many levels (98th percentile).
pub const SOLID_TOLERANCE: u8 = 4;
/// A fill is "clean" when the ring check (98th percentile of the largest channel difference) is
/// at most this many levels (ANN-9.AC2, S2).
pub const CLEAN_TOLERANCE: u8 = 6;

/// Read-only RGB access to an image or a pixmap (premultiplied alpha is treated as opaque).
pub struct PixView<'a> {
    w: u32,
    h: u32,
    data: &'a [u8],
}

impl<'a> PixView<'a> {
    pub fn from_image(img: &'a RgbaImage) -> Self {
        PixView {
            w: img.width(),
            h: img.height(),
            data: img.as_raw(),
        }
    }
    pub(crate) fn from_pixmap(pm: &'a Pixmap) -> Self {
        PixView {
            w: pm.width(),
            h: pm.height(),
            data: pm.data(),
        }
    }
    fn px(&self, x: u32, y: u32) -> [u8; 3] {
        let o = (y as usize * self.w as usize + x as usize) * 4;
        [self.data[o], self.data[o + 1], self.data[o + 2]]
    }
}

/// The result of measuring the ring around a box (ANN-9, S2).
#[derive(Debug, Clone, PartialEq)]
pub struct FillMeasure {
    pub fill: Fill,
    /// The ring varies too much for a clean fill (a photo, a pattern): the tool says so and
    /// offers Cancel or Use anyway. `fill` is then the better of the two fits.
    pub busy: bool,
    /// Largest channel difference between the fill and any ring pixel.
    pub ring_max_error: u8,
    /// The same at the 98th percentile (a stray glyph edge in the ring does not count), over the
    /// sides of the ring that were used.
    pub ring_p98_error: u8,
    /// The 98th percentile over all four sides, including the ones left out because something
    /// else (a neighbouring line, a border) sits there.
    pub ring_p98_all_sides: u8,
    /// How many of the four sides decided the fill.
    pub sides_used: u8,
    /// Number of ring pixels measured.
    pub ring_pixels: usize,
}

/// Position of a fill at pixel `(x, y)` of a box whose integer region is `r`; `clamp` limits the
/// gradient position to the box (as painted) or lets it run on (for the ring check).
fn fill_at(fill: &Fill, r: Region, x: f64, y: f64, clamp: bool) -> [f64; 3] {
    let c = |i: usize| {
        let k = fill.colors[i.min(fill.colors.len() - 1)];
        [f64::from(k.r), f64::from(k.g), f64::from(k.b)]
    };
    if fill.kind == FillKind::Solid || fill.colors.len() < 2 {
        return c(0);
    }
    let a = fill.angle.unwrap_or(90.0).to_radians();
    let (dx, dy) = (a.cos(), a.sin());
    let (w, h) = (f64::from(r.x1 - r.x0), f64::from(r.y1 - r.y0));
    let l = (w * dx).abs() + (h * dy).abs();
    let (cx, cy) = (f64::from(r.x0) + w / 2.0, f64::from(r.y0) + h / 2.0);
    let mut t = if l < 1e-9 {
        0.5
    } else {
        ((x - cx) * dx + (y - cy) * dy) / l + 0.5
    };
    if clamp {
        t = t.clamp(0.0, 1.0);
    }
    let n = fill.colors.len() - 1;
    let pos = t * n as f64;
    let i = (pos.floor().clamp(0.0, (n - 1) as f64)) as usize;
    let f = pos - i as f64;
    let (p, q) = (c(i), c(i + 1));
    [
        p[0] + (q[0] - p[0]) * f,
        p[1] + (q[1] - p[1]) * f,
        p[2] + (q[2] - p[2]) * f,
    ]
}

fn to_u8(v: f64) -> u8 {
    v.round().clamp(0.0, 255.0) as u8
}

/// Paints `fill` over the integer region `r` (opaque), gradient mapped over `r`.
pub(crate) fn paint_fill(pm: &mut Pixmap, r: Region, fill: &Fill) {
    let stride = pm.width() as usize * 4;
    let data = pm.data_mut();
    for y in r.y0..r.y1 {
        for x in r.x0..r.x1 {
            let v = fill_at(fill, r, f64::from(x) + 0.5, f64::from(y) + 0.5, true);
            let o = y as usize * stride + x as usize * 4;
            data[o..o + 4].copy_from_slice(&[to_u8(v[0]), to_u8(v[1]), to_u8(v[2]), 255]);
        }
    }
}

/// The ring pixels `(x, y)` just outside `inner`, clipped to the image.
fn ring(inner: Region, w: u32, h: u32) -> Vec<(u32, u32)> {
    let (x0, y0) = (
        inner.x0.saturating_sub(RING_PX),
        inner.y0.saturating_sub(RING_PX),
    );
    let (x1, y1) = ((inner.x1 + RING_PX).min(w), (inner.y1 + RING_PX).min(h));
    let mut out = Vec::new();
    for y in y0..y1 {
        for x in x0..x1 {
            let inside = x >= inner.x0 && x < inner.x1 && y >= inner.y0 && y < inner.y1;
            if !inside {
                out.push((x, y));
            }
        }
    }
    out
}

fn max_diff(a: [u8; 3], b: [f64; 3]) -> f64 {
    (0..3)
        .map(|i| (f64::from(a[i]) - b[i]).abs())
        .fold(0.0, f64::max)
}

/// `(max, p98)` of the largest-channel difference between `fill` and the ring pixels.
fn ring_stats(v: &PixView<'_>, inner: Region, pts: &[(u32, u32)], fill: &Fill) -> (u8, u8) {
    let mut errs: Vec<f64> = pts
        .iter()
        .map(|&(x, y)| {
            max_diff(
                v.px(x, y),
                fill_at(fill, inner, f64::from(x) + 0.5, f64::from(y) + 0.5, false),
            )
        })
        .collect();
    if errs.is_empty() {
        return (0, 0);
    }
    errs.sort_by(f64::total_cmp);
    let p98 = errs[((errs.len() as f64 * 0.98).ceil() as usize).clamp(1, errs.len()) - 1];
    (to_u8(errs[errs.len() - 1]), to_u8(p98))
}

/// The ring check of a stored fill against the pixels of `base` around `rect`: `(max, p98)`
/// largest-channel differences in levels.
pub fn ring_error(base: &RgbaImage, rect: Rc, fill: &Fill) -> Option<(u8, u8)> {
    let v = PixView::from_image(base);
    let inner = fill_region(rect, v.w, v.h)?;
    let pts = ring(inner, v.w, v.h);
    (!pts.is_empty()).then(|| ring_stats(&v, inner, &pts, fill))
}

fn median(vals: &mut [u8]) -> u8 {
    vals.sort_unstable();
    vals[vals.len() / 2]
}

/// Measures the fill for `rect` from the ring around it (ANN-9, base-edit rules 5): a solid
/// colour when the ring is flat, a linear gradient when it changes smoothly, otherwise `busy`.
/// A box that covers the whole image has no ring and gets a white solid fill marked busy.
pub fn measure_fill(base: &RgbaImage, rect: Rc) -> FillMeasure {
    let v = PixView::from_image(base);
    measure_fill_in(&v, fill_region(rect, v.w, v.h))
}

/// Pixels the erased region of Retype and Move area grows beyond the box on every side (clamped
/// to the image): a recognizer box can be tight, and a glyph top or tail just outside it would
/// stay as a sliver. The ring is measured outside this padded region. Not stored in the op.
const ERASE_PAD_PX: u32 = 1;

/// The region a Retype or Move area fill covers and measures around: the box rounded outward,
/// grown by `ERASE_PAD_PX`, clipped to the `w x h` image.
pub(crate) fn fill_region(rect: Rc, w: u32, h: u32) -> Option<Region> {
    let r = Region::outward(rect.to_array(), w, h)?;
    Some(Region {
        x0: r.x0.saturating_sub(ERASE_PAD_PX),
        y0: r.y0.saturating_sub(ERASE_PAD_PX),
        x1: (r.x1 + ERASE_PAD_PX).min(w),
        y1: (r.y1 + ERASE_PAD_PX).min(h),
    })
}

/// The ring split into its four sides: top, bottom (with the corners), left and right.
fn ring_sides(inner: Region, w: u32, h: u32) -> [Vec<(u32, u32)>; 4] {
    let (x0, x1) = (
        inner.x0.saturating_sub(RING_PX),
        (inner.x1 + RING_PX).min(w),
    );
    let (y0, y1) = (
        inner.y0.saturating_sub(RING_PX),
        (inner.y1 + RING_PX).min(h),
    );
    let mut sides: [Vec<(u32, u32)>; 4] = Default::default();
    for y in y0..y1 {
        for x in x0..x1 {
            let side = if y < inner.y0 {
                0
            } else if y >= inner.y1 {
                1
            } else if x < inner.x0 {
                2
            } else if x >= inner.x1 {
                3
            } else {
                continue;
            };
            sides[side].push((x, y));
        }
    }
    sides
}

fn solid_fit(v: &PixView<'_>, pts: &[(u32, u32)]) -> Fill {
    let mut ch: [Vec<u8>; 3] = Default::default();
    for &(x, y) in pts {
        let p = v.px(x, y);
        for i in 0..3 {
            ch[i].push(p[i]);
        }
    }
    Fill::solid(Color::rgb(
        median(&mut ch[0]),
        median(&mut ch[1]),
        median(&mut ch[2]),
    ))
}

pub(crate) fn measure_fill_view(v: &PixView<'_>, rect: Rc) -> FillMeasure {
    measure_fill_in(v, Region::outward(rect.to_array(), v.w, v.h))
}

fn measure_fill_in(v: &PixView<'_>, inner: Option<Region>) -> FillMeasure {
    let none = FillMeasure {
        fill: Fill::solid(Color::WHITE),
        busy: true,
        ring_max_error: 255,
        ring_p98_error: 255,
        ring_p98_all_sides: 255,
        sides_used: 0,
        ring_pixels: 0,
    };
    let Some(inner) = inner else {
        return none;
    };
    let sides = ring_sides(inner, v.w, v.h);
    let all: Vec<(u32, u32)> = sides.iter().flatten().copied().collect();
    if all.len() < 8 {
        return none;
    }
    // Candidate side sets, the largest first: a neighbouring text line, an icon or a button
    // border on one side must not decide the fill. A set must keep a third of the ring.
    let mut masks: Vec<u8> = (1u8..16).collect();
    masks.sort_by_key(|m| std::cmp::Reverse(m.count_ones()));
    let points = |m: u8| -> Vec<(u32, u32)> {
        (0..4)
            .filter(|i| m & (1 << i) != 0)
            .flat_map(|i| sides[i].iter().copied())
            .collect()
    };
    let usable = |m: u8, pts: &Vec<(u32, u32)>| {
        // One side alone says nothing about a gradient across the box.
        (m.count_ones() >= 2
            || (sides.iter().filter(|x| !x.is_empty()).count() < 2 && pts.len() >= 8))
            && pts.len() >= 8
            && pts.len() * 3 >= all.len()
    };
    let done = |fill: Fill, pts: &[(u32, u32)], mask: u8, busy: bool| {
        let (max, p98) = ring_stats(v, inner, pts, &fill);
        let (_, p98_all) = ring_stats(v, inner, &all, &fill);
        FillMeasure {
            fill,
            busy,
            ring_max_error: max,
            ring_p98_error: p98,
            ring_p98_all_sides: p98_all,
            sides_used: mask.count_ones() as u8,
            ring_pixels: pts.len(),
        }
    };
    // 1. A solid colour, from as many sides as agree.
    for &m in &masks {
        let pts = points(m);
        if !usable(m, &pts) {
            continue;
        }
        let fill = solid_fit(v, &pts);
        if ring_stats(v, inner, &pts, &fill).1 <= SOLID_TOLERANCE {
            return done(fill, &pts, m, false);
        }
    }
    // 2. A linear gradient.
    for &m in &masks {
        let pts = points(m);
        if !usable(m, &pts) {
            continue;
        }
        if let Some(fill) = fit_linear(v, inner, &pts) {
            if ring_stats(v, inner, &pts, &fill).1 <= CLEAN_TOLERANCE {
                return done(fill, &pts, m, false);
            }
        }
    }
    // 3. Too busy: the better of the two fits over the whole ring.
    let solid = solid_fit(v, &all);
    let (_, sp98) = ring_stats(v, inner, &all, &solid);
    match fit_linear(v, inner, &all) {
        Some(lin) if ring_stats(v, inner, &all, &lin).1 < sp98 => done(lin, &all, 15, true),
        _ => done(solid, &all, 15, true),
    }
}

/// Least-squares linear gradient through the ring pixels: a common direction (the principal
/// direction of the per-channel planes) and, per channel, a line along it.
fn fit_linear(v: &PixView<'_>, inner: Region, pts: &[(u32, u32)]) -> Option<Fill> {
    let (w, h) = (
        f64::from(inner.x1 - inner.x0),
        f64::from(inner.y1 - inner.y0),
    );
    let (cx, cy) = (f64::from(inner.x0) + w / 2.0, f64::from(inner.y0) + h / 2.0);
    let n = pts.len() as f64;
    let xy: Vec<(f64, f64)> = pts
        .iter()
        .map(|&(x, y)| (f64::from(x) + 0.5 - cx, f64::from(y) + 0.5 - cy))
        .collect();
    let (mx, my) = (
        xy.iter().map(|p| p.0).sum::<f64>() / n,
        xy.iter().map(|p| p.1).sum::<f64>() / n,
    );
    let (sxx, syy, sxy) = xy.iter().fold((0.0, 0.0, 0.0), |a, p| {
        let (dx, dy) = (p.0 - mx, p.1 - my);
        (a.0 + dx * dx, a.1 + dy * dy, a.2 + dx * dy)
    });
    let det = sxx * syy - sxy * sxy;
    if det.abs() < 1e-9 * (sxx * syy).max(1e-9) {
        return None;
    }
    // Per-channel plane gradient g = S^-1 * cov(x, c).
    let mut m = [[0.0f64; 2]; 2];
    let mut lum = (0.0, 0.0);
    for c in 0..3 {
        let mc = pts
            .iter()
            .map(|&(x, y)| f64::from(v.px(x, y)[c]))
            .sum::<f64>()
            / n;
        let (mut cxv, mut cyv) = (0.0, 0.0);
        for (p, &(x, y)) in xy.iter().zip(pts) {
            let d = f64::from(v.px(x, y)[c]) - mc;
            cxv += (p.0 - mx) * d;
            cyv += (p.1 - my) * d;
        }
        let gx = (syy * cxv - sxy * cyv) / det;
        let gy = (sxx * cyv - sxy * cxv) / det;
        m[0][0] += gx * gx;
        m[0][1] += gx * gy;
        m[1][1] += gy * gy;
        lum.0 += gx;
        lum.1 += gy;
    }
    // Principal eigenvector of [[a, b], [b, d]].
    let (a, b, d) = (m[0][0], m[0][1], m[1][1]);
    let theta = 0.5 * (2.0 * b).atan2(a - d);
    let mut dir = (theta.cos(), theta.sin());
    if dir.0 * lum.0 + dir.1 * lum.1 < 0.0 {
        dir = (-dir.0, -dir.1);
    }
    if (a + d).sqrt() < 0.01 {
        return None; // no gradient at all: solid
    }
    let dist: Vec<f64> = xy.iter().map(|p| p.0 * dir.0 + p.1 * dir.1).collect();
    let md = dist.iter().sum::<f64>() / n;
    let sdd = dist.iter().map(|d| (d - md) * (d - md)).sum::<f64>();
    if sdd < 1e-9 {
        return None;
    }
    let l = (w * dir.0).abs() + (h * dir.1).abs();
    let mut c0 = [0u8; 3];
    let mut c1 = [0u8; 3];
    for c in 0..3 {
        let mc = pts
            .iter()
            .map(|&(x, y)| f64::from(v.px(x, y)[c]))
            .sum::<f64>()
            / n;
        let cov: f64 = dist
            .iter()
            .zip(pts)
            .map(|(d, &(x, y))| (d - md) * (f64::from(v.px(x, y)[c]) - mc))
            .sum();
        let b = cov / sdd;
        let at = |t: f64| mc + b * (t - md);
        c0[c] = to_u8(at(-l / 2.0));
        c1[c] = to_u8(at(l / 2.0));
    }
    let angle = dir.1.atan2(dir.0).to_degrees().rem_euclid(360.0);
    Some(Fill {
        kind: FillKind::Linear,
        colors: vec![
            Color::rgb(c0[0], c0[1], c0[2]),
            Color::rgb(c1[0], c1[1], c1[2]),
        ],
        angle: Some((angle * 10.0).round() / 10.0),
    })
}

/// Text colour and size measured inside a box (ANN-9, base-edit rules 6).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextMeasure {
    /// The darkest or lightest cluster inside the box (the strokes of the old text).
    pub color: Color,
    /// Font size in px (Inter), from the height of the ink and the letters in `text`.
    pub size: f64,
    /// Height of the ink in px (0 when the box holds no ink).
    pub ink_height: u32,
}

/// Ink pixels differ from the fill by more than this (largest channel, levels).
const INK_MIN: f64 = 48.0;

/// Em heights of Inter: cap height / ascender, x-height, descender.
const EM_ASCENT: f64 = 0.74;
const EM_X: f64 = 0.55;
const EM_DESCENT: f64 = 0.21;

fn expected_ink_em(text: &str) -> Option<f64> {
    if text.is_empty() || !text.chars().all(|c| c.is_ascii() || c.is_alphanumeric()) {
        return None;
    }
    if !text.is_ascii() {
        return None;
    }
    let up = text
        .chars()
        .any(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || "bdfhijklt".contains(c));
    let desc = text.chars().any(|c| "gjpqy".contains(c));
    let top = if up { EM_ASCENT } else { EM_X };
    Some(top + if desc { EM_DESCENT } else { 0.0 })
}

/// Measures the colour and size of the text inside `rect` against the background `fill`.
pub fn measure_text_style(
    base: &RgbaImage,
    rect: Rc,
    fill: &Fill,
    text: Option<&str>,
) -> TextMeasure {
    measure_text_view(&PixView::from_image(base), rect, fill, text)
}

pub(crate) fn measure_text_view(
    v: &PixView<'_>,
    rect: Rc,
    fill: &Fill,
    text: Option<&str>,
) -> TextMeasure {
    let fallback_size = (rect.normalized().h * 0.7).round().max(8.0);
    let Some(r) = Region::outward(rect.to_array(), v.w, v.h) else {
        return TextMeasure {
            color: Color::BLACK,
            size: fallback_size,
            ink_height: 0,
        };
    };
    let mut ink: Vec<(f64, [u8; 3], u32)> = Vec::new();
    for y in r.y0..r.y1 {
        for x in r.x0..r.x1 {
            let bg = fill_at(fill, r, f64::from(x) + 0.5, f64::from(y) + 0.5, true);
            let p = v.px(x, y);
            let d = max_diff(p, bg);
            if d >= INK_MIN {
                ink.push((d, p, y));
            }
        }
    }
    let mid = fill_at(
        fill,
        r,
        f64::from(r.x0 + r.x1) / 2.0,
        f64::from(r.y0 + r.y1) / 2.0,
        true,
    );
    if ink.is_empty() {
        let lum = 0.299 * mid[0] + 0.587 * mid[1] + 0.114 * mid[2];
        return TextMeasure {
            color: if lum > 140.0 {
                Color::BLACK
            } else {
                Color::WHITE
            },
            size: fallback_size,
            ink_height: 0,
        };
    }
    let dmax = ink.iter().map(|i| i.0).fold(0.0, f64::max);
    // Strokes: the strongest quarter of the ink; the ink rows come from pixels over half of it.
    let mut core: Vec<&(f64, [u8; 3], u32)> = ink.iter().filter(|i| i.0 >= dmax * 0.75).collect();
    core.sort_by(|a, b| b.0.total_cmp(&a.0));
    let take = (core.len() / 2).max(1).min(core.len());
    let mut sum = [0u64; 3];
    for i in &core[..take] {
        for (acc, v) in sum.iter_mut().zip(i.1) {
            *acc += u64::from(v);
        }
    }
    let n = take as u64;
    let color = Color::rgb((sum[0] / n) as u8, (sum[1] / n) as u8, (sum[2] / n) as u8);
    let (mut y0, mut y1) = (u32::MAX, 0);
    for i in ink.iter().filter(|i| i.0 >= dmax * 0.5) {
        y0 = y0.min(i.2);
        y1 = y1.max(i.2);
    }
    let ink_h = y1.saturating_sub(y0) + 1;
    let size = match text.and_then(expected_ink_em) {
        Some(em) => (f64::from(ink_h) / em).round().max(6.0),
        None => fallback_size,
    };
    TextMeasure {
        color,
        size,
        ink_height: ink_h,
    }
}

/// The bar colour of one Blueprint line: the text colour lightened toward the background.
fn bar_color(text: Color, bg: [f64; 3]) -> Color {
    let mix = |t: u8, b: f64| to_u8(f64::from(t) + (b - f64::from(t)) * 0.55);
    Color::rgb(mix(text.r, bg[0]), mix(text.g, bg[1]), mix(text.b, bg[2]))
}

/// What a Blueprint stores for one box.
#[derive(Debug, Clone, PartialEq)]
pub struct BlueprintBoxStyle {
    pub fill: Fill,
    pub busy: bool,
    pub color: Color,
}

/// Measures the fill and the auto bar colour of each box in `base` (computed once, stored in the
/// op; `suggest_blueprint` and the editor call this).
pub fn measure_blueprint(base: &RgbaImage, boxes: &[Rc]) -> Vec<BlueprintBoxStyle> {
    measure_blueprint_view(&PixView::from_image(base), boxes)
}

pub(crate) fn measure_blueprint_view(v: &PixView<'_>, boxes: &[Rc]) -> Vec<BlueprintBoxStyle> {
    boxes
        .iter()
        .map(|&b| {
            let m = measure_fill_view(v, b);
            let t = measure_text_view(v, b, &m.fill, None);
            let r = Region::outward(b.to_array(), v.w, v.h);
            let mid = r.map_or([128.0; 3], |r| {
                fill_at(
                    &m.fill,
                    r,
                    f64::from(r.x0 + r.x1) / 2.0,
                    f64::from(r.y0 + r.y1) / 2.0,
                    true,
                )
            });
            BlueprintBoxStyle {
                color: bar_color(t.color, mid),
                fill: m.fill,
                busy: m.busy,
            }
        })
        .collect()
}

fn region_mask(w: u32, h: u32, r: Region) -> Option<Mask> {
    let mut mask = Mask::new(w, h)?;
    let rect = Rect::from_ltrb(r.x0 as f32, r.y0 as f32, r.x1 as f32, r.y1 as f32)?;
    mask.fill_path(
        &PathBuilder::from_rect(rect),
        FillRule::Winding,
        false,
        Transform::identity(),
    );
    Some(mask)
}

/// Retype (ANN-9): the fill over `rect`, then the new text clipped to `rect`. A stored op whose
/// text is wider than `rect` is clipped without a warning; the editor widens the box instead
/// (`fit_retype`), so the text it stores always fits.
pub(crate) fn apply_retype(
    pm: &mut Pixmap,
    fonts: &mut Fonts,
    kind: &OpKind,
) -> Result<(), CmError> {
    let OpKind::Retype {
        rect,
        fill,
        text: t,
        size,
        color,
        weight,
        align,
    } = kind
    else {
        return Ok(());
    };
    let style = TextStyle {
        px: *size as f32,
        color: *color,
        outline: None,
        bg: None,
        wrap_w: None,
        center: false,
    };
    let Some(l) = text::layout(fonts, t, &style)? else {
        // No text: the box is only erased.
        if let Some(r) = fill_region(*rect, pm.width(), pm.height()) {
            paint_fill(pm, r, fill);
        }
        return Ok(());
    };
    let n = rect.normalized();
    let tw = f64::from(l.text_w);
    let x = match align {
        TextAlign::Left => n.x,
        TextAlign::Center => n.x + (n.w - tw) / 2.0,
        TextAlign::Right => n.x + n.w - tw,
    };
    // Vertically centred on the middle of the capital letters, as step badges are.
    let px = *size as f32;
    let y = n.y + n.h / 2.0 + f64::from(0.364 * px) - f64::from(l.baseline);
    let (bx, by) = (x.round() as i32 - l.pad, y.round() as i32 - l.pad);
    // A box fitted to the ink of the old text is about as tall as its capitals, but the new
    // text may carry marks above them (Vietnamese stacks two: Ế, Ộ) and tails below the baseline
    // (g, y, ỵ). The erase and the clip grow vertically to the text's own height (the whole
    // layout, marks included), by at most half the size on each side, so those parts are not cut
    // off. The old text's own marks are covered by the source box, which is padded to the whole
    // line when it comes from a recognized line (`ocr::pad_line_boxes`).
    let max_grow = f64::from(px) * 0.5;
    let top = f64::from(by).clamp(n.y - max_grow, n.y);
    let bottom =
        (f64::from(by) + f64::from(l.pixmap.height())).clamp(n.y + n.h, n.y + n.h + max_grow);
    let grown = Rc::new(n.x, top, n.w, bottom - top);
    let Some(r) = Region::outward(grown.to_array(), pm.width(), pm.height()) else {
        return Ok(());
    };
    // The erase is padded; the new text stays clipped to the (grown) box.
    paint_fill(
        pm,
        fill_region(grown, pm.width(), pm.height()).unwrap_or(r),
        fill,
    );
    let Some(mask) = region_mask(pm.width(), pm.height(), r) else {
        return Ok(());
    };
    let offsets: &[i32] = if *weight >= 600 {
        &[0, ((size * 0.035).round() as i32).max(1)]
    } else {
        &[0]
    };
    for dx in offsets {
        pm.draw_pixmap(
            bx + dx,
            by,
            l.pixmap.as_ref(),
            &PixmapPaint::default(),
            Transform::identity(),
            Some(&mask),
        );
    }
    Ok(())
}

/// The width and height in px the text of a `retype` op needs (Inter), so the editor can widen
/// the box to hold it (ANN-9). `None` for other ops or text that cannot be laid out.
pub fn retype_text_extent(op: &OpKind) -> Option<(f64, f64)> {
    thread_local! {
        static FONTS: std::cell::RefCell<Option<Fonts>> = const { std::cell::RefCell::new(None) };
    }
    let OpKind::Retype {
        text: t,
        size,
        color,
        ..
    } = op
    else {
        return None;
    };
    let style = TextStyle {
        px: *size as f32,
        color: *color,
        outline: None,
        bg: None,
        wrap_w: None,
        center: false,
    };
    FONTS.with(|f| {
        let mut f = f.borrow_mut();
        let fonts = f.get_or_insert_with(Fonts::bundled);
        let l = text::layout(fonts, t, &style).ok().flatten()?;
        Some((
            f64::from(l.text_w),
            f64::from(l.pixmap.height()) - 2.0 * f64::from(l.pad),
        ))
    })
}

/// Move area (ANN-10), in one defined order: copy the source pixels, fill the hole, paste the
/// copy at `to` (so overlapping source and destination give the same result as cut, fill, paste).
pub(crate) fn apply_move_area(pm: &mut Pixmap, kind: &OpKind) {
    let OpKind::MoveArea { source, to, fill } = kind else {
        return;
    };
    let (w, h) = (pm.width(), pm.height());
    let Some(r) = Region::outward_round(source.to_array(), w, h) else {
        return;
    };
    let stride = w as usize * 4;
    let (bw, bh) = ((r.x1 - r.x0) as usize, (r.y1 - r.y0) as usize);
    let mut block = vec![0u8; bw * bh * 4];
    for y in 0..bh {
        let o = (r.y0 as usize + y) * stride + r.x0 as usize * 4;
        block[y * bw * 4..(y + 1) * bw * 4].copy_from_slice(&pm.data()[o..o + bw * 4]);
    }
    paint_fill(pm, fill_region(*source, w, h).unwrap_or(r), fill);
    let Some(to) = to else {
        return;
    };
    let n = source.normalized();
    // The block keeps its offset inside the stored source when that was clipped at the edge.
    let dx = to.x.round() as i64 + (i64::from(r.x0) - n.x.round() as i64);
    let dy = to.y.round() as i64 + (i64::from(r.y0) - n.y.round() as i64);
    let data = pm.data_mut();
    for y in 0..bh as i64 {
        let ty = dy + y;
        if ty < 0 || ty >= i64::from(h) {
            continue;
        }
        for x in 0..bw as i64 {
            let tx = dx + x;
            if tx < 0 || tx >= i64::from(w) {
                continue;
            }
            let s = (y as usize * bw + x as usize) * 4;
            let o = ty as usize * stride + tx as usize * 4;
            data[o..o + 4].copy_from_slice(&block[s..s + 4]);
        }
    }
}

/// Lines of one paragraph: consecutive boxes of similar height, close above each other.
fn paragraphs(boxes: &[Rc]) -> Vec<Vec<usize>> {
    let mut idx: Vec<usize> = (0..boxes.len()).collect();
    let n: Vec<Rc> = boxes.iter().map(|b| b.normalized()).collect();
    idx.sort_by(|&a, &b| n[a].y.total_cmp(&n[b].y).then(n[a].x.total_cmp(&n[b].x)));
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for i in idx {
        let joins = groups.last().and_then(|g| g.last()).is_some_and(|&p| {
            let (a, b) = (n[p], n[i]);
            let hmax = a.h.max(b.h);
            (a.h - b.h).abs() <= 0.4 * hmax
                && b.y - (a.y + a.h) >= -0.3 * hmax
                && b.y - (a.y + a.h) <= hmax
                && (a.x - b.x).abs() <= 3.0 * hmax
        });
        if joins {
            if let Some(g) = groups.last_mut() {
                g.push(i);
            }
        } else {
            groups.push(vec![i]);
        }
    }
    groups
}

/// The bar rectangle `[x0, y0, x1, y1]` (whole px) of each box: inside the box, the same height
/// within a paragraph and an even line pitch (ANN-11.AC2).
pub fn blueprint_bars(boxes: &[Rc], style: BlueprintStyle) -> Vec<[i64; 4]> {
    let frac = match style {
        BlueprintStyle::Bar => 0.5,
        BlueprintStyle::Box => 0.9,
    };
    let n: Vec<Rc> = boxes.iter().map(|b| b.normalized()).collect();
    let mut out = vec![[0i64; 4]; boxes.len()];
    for g in paragraphs(boxes) {
        let mut hs: Vec<f64> = g.iter().map(|&i| n[i].h).collect();
        hs.sort_by(f64::total_cmp);
        let bar_h = ((hs[hs.len() / 2] * frac).round() as i64).max(2);
        let centre = |i: usize| n[i].y + n[i].h / 2.0;
        let first = centre(g[0]);
        let pitch = if g.len() > 1 {
            ((centre(g[g.len() - 1]) - first) / (g.len() - 1) as f64).round()
        } else {
            0.0
        };
        for (k, &i) in g.iter().enumerate() {
            let b = n[i];
            let (by0, by1) = (b.y.round() as i64, (b.y + b.h).round() as i64);
            let h = bar_h.min((by1 - by0).max(1));
            let top = ((first + pitch * k as f64) - bar_h as f64 / 2.0).round() as i64;
            let top = top.clamp(by0, (by1 - h).max(by0));
            out[i] = [b.x.round() as i64, top, (b.x + b.w).round() as i64, top + h];
        }
    }
    out
}

/// Blueprint (ANN-11): every box filled, then the bar drawn. Stored fills and colours are used;
/// missing ones are measured from the pixels as they are when the op starts.
pub(crate) fn apply_blueprint(pm: &mut Pixmap, kind: &OpKind) {
    let OpKind::Blueprint {
        boxes,
        style,
        color,
        radius,
        colors,
        fills,
    } = kind
    else {
        return;
    };
    let measured = if fills.len() != boxes.len()
        || (matches!(color, BlueprintColor::Auto) && colors.len() != boxes.len())
    {
        Some(measure_blueprint_view(&PixView::from_pixmap(pm), boxes))
    } else {
        None
    };
    let fill_of = |i: usize| -> Fill {
        fills
            .get(i)
            .cloned()
            .or_else(|| measured.as_ref().map(|m| m[i].fill.clone()))
            .unwrap_or_else(|| Fill::solid(Color::WHITE))
    };
    let color_of = |i: usize| -> Color {
        match color {
            BlueprintColor::Fixed(c) => *c,
            BlueprintColor::Auto => colors
                .get(i)
                .copied()
                .or_else(|| measured.as_ref().map(|m| m[i].color))
                .unwrap_or(Color::rgb(0x99, 0x99, 0x99)),
        }
    };
    for (i, b) in boxes.iter().enumerate() {
        if let Some(r) = Region::outward(b.to_array(), pm.width(), pm.height()) {
            paint_fill(pm, r, &fill_of(i));
        }
    }
    let bars = blueprint_bars(boxes, *style);
    for (i, bar) in bars.iter().enumerate() {
        let rect = [
            bar[0] as f64,
            bar[1] as f64,
            (bar[2] - bar[0]) as f64,
            (bar[3] - bar[1]) as f64,
        ];
        if let Some(p) = rounded_rect(&rect, *radius) {
            fill_path(pm, &p, color_of(i), 255);
        }
    }
}
