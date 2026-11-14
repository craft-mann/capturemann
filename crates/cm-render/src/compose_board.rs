//! Boards (BRD-1): several flattened screenshots laid out in one picture.
//!
//! CPU only (`tiny-skia`, the bundled fonts), so it runs headless and gives the same bytes on
//! every machine. The sources are the rendered pictures of the screenshots (annotations and
//! redactions already applied); this module never sees an original or an op, so a blurred area
//! cannot come back (BRD-1.AC2).
//!
//! Layouts: [`LayoutKind::Pair`] (two, side by side or stacked, scaled to one height or width),
//! [`LayoutKind::Grid`] (filled in order, `columns` wide), [`LayoutKind::Strip`] (a numbered row
//! or column) and [`LayoutKind::Free`] (a rectangle per source). [`plan`] is pure geometry;
//! [`render_board`] draws it.

use cm_core::CmError;
use image::{imageops::FilterType, RgbaImage};
use serde::{Deserialize, Serialize};
use tiny_skia::{FillRule, Mask, Paint, PathBuilder, Pixmap, PixmapPaint, Rect, Transform};

use crate::ops::Color;
use crate::text::{self, Fonts, TextStyle};

/// Fewest and most sources of a board (BRD-1).
pub const MIN_SOURCES: usize = 2;
pub const MAX_SOURCES: usize = 12;
/// The board is scaled down as a whole above these limits.
pub const MAX_EDGE: u32 = 16_384;
pub const MAX_PIXELS: u64 = 100_000_000;

/// Which arrangement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LayoutKind {
    Pair,
    Grid,
    Strip,
    Free,
}

impl LayoutKind {
    pub fn as_str(self) -> &'static str {
        match self {
            LayoutKind::Pair => "pair",
            LayoutKind::Grid => "grid",
            LayoutKind::Strip => "strip",
            LayoutKind::Free => "free",
        }
    }
}

/// Pair and Strip: in a row (`side_by_side`) or a column (`stacked`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Arrangement {
    #[default]
    SideBySide,
    Stacked,
}

/// A rectangle on the canvas (Free layout), in board pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Slot {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

/// The layout of a board (the `layout` of `create_board`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoardLayout {
    pub kind: LayoutKind,
    /// Pair and Strip.
    #[serde(default)]
    pub arrangement: Arrangement,
    /// Grid: columns (default 2 for up to 4 images, else 3).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub columns: Option<u32>,
    /// Free: one rectangle per source, in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub slots: Vec<Slot>,
    /// Free: the canvas size; default is the slots' extent plus the margin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canvas: Option<[u32; 2]>,
}

impl BoardLayout {
    pub fn new(kind: LayoutKind) -> Self {
        BoardLayout {
            kind,
            arrangement: Arrangement::SideBySide,
            columns: None,
            slots: Vec::new(),
            canvas: None,
        }
    }
}

/// The board's background. Text form: `transparent`, `white`, `light_grey`, `dark` or `#RRGGBB`
/// (a tint colour).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Background {
    Transparent,
    #[default]
    White,
    LightGrey,
    Dark,
    Tint(Color),
}

impl Background {
    pub fn parse(s: &str) -> Result<Background, CmError> {
        Ok(match s {
            "transparent" => Background::Transparent,
            "white" => Background::White,
            "light_grey" => Background::LightGrey,
            "dark" => Background::Dark,
            other => Background::Tint(other.parse::<Color>().map_err(|_| {
                CmError::invalid_field(
                    "background",
                    "must be transparent, white, light_grey, dark or a #RRGGBB colour",
                )
            })?),
        })
    }

    pub fn as_string(self) -> String {
        match self {
            Background::Transparent => "transparent".into(),
            Background::White => "white".into(),
            Background::LightGrey => "light_grey".into(),
            Background::Dark => "dark".into(),
            Background::Tint(c) => format!("#{:02X}{:02X}{:02X}", c.r, c.g, c.b),
        }
    }

    /// The fill, with alpha.
    fn rgba(self) -> [u8; 4] {
        match self {
            Background::Transparent => [0, 0, 0, 0],
            Background::White => [255, 255, 255, 255],
            Background::LightGrey => [236, 237, 240, 255],
            Background::Dark => [24, 25, 28, 255],
            Background::Tint(c) => [c.r, c.g, c.b, 255],
        }
    }

    /// True when text on it should be light.
    fn is_dark(self) -> bool {
        let [r, g, b, a] = self.rgba();
        a > 0 && (0.299 * f64::from(r) + 0.587 * f64::from(g) + 0.114 * f64::from(b)) < 140.0
    }

    /// The same background composed on white (a JPEG or other lossy export).
    pub fn is_transparent(self) -> bool {
        matches!(self, Background::Transparent)
    }
}

impl Serialize for Background {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.as_string())
    }
}

impl<'de> Deserialize<'de> for Background {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Background::parse(&s).map_err(serde::de::Error::custom)
    }
}

/// The options of a board (the `options` of `create_board`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BoardOptions {
    /// Space between images, px (0 to 200).
    pub gap: u32,
    /// Space around the board, px (0 to 400).
    pub margin: u32,
    pub background: Background,
    /// Corner radius of each image, px (0 to 100).
    pub radius: u32,
    pub shadow: bool,
    /// One caption per source (empty strings draw nothing); empty list: no captions.
    pub captions: Vec<String>,
    pub title: Option<String>,
    /// Pair: BEFORE and AFTER labels on the two images.
    pub before_after: bool,
    /// The two label words (default `BEFORE`, `AFTER`); the app passes translated ones.
    pub label_words: Option<[String; 2]>,
}

impl Default for BoardOptions {
    fn default() -> Self {
        BoardOptions {
            gap: 24,
            margin: 32,
            background: Background::White,
            radius: 0,
            shadow: false,
            captions: Vec::new(),
            title: None,
            before_after: false,
            label_words: None,
        }
    }
}

impl BoardOptions {
    pub fn validate(&self) -> Result<(), CmError> {
        let range = |name: &str, v: u32, max: u32| {
            if v > max {
                Err(CmError::invalid_field(
                    &format!("options.{name}"),
                    format!("must be 0 to {max}"),
                ))
            } else {
                Ok(())
            }
        };
        range("gap", self.gap, 200)?;
        range("margin", self.margin, 400)?;
        range("radius", self.radius, 100)?;
        if self.captions.len() > MAX_SOURCES {
            return Err(CmError::invalid_field(
                "options.captions",
                "has more entries than a board has images",
            ));
        }
        for (name, t) in self
            .captions
            .iter()
            .map(|c| ("captions", c))
            .chain(self.title.iter().map(|t| ("title", t)))
        {
            if t.chars().count() > 200 {
                return Err(CmError::invalid_field(
                    &format!("options.{name}"),
                    "is longer than 200 characters",
                ));
            }
        }
        Ok(())
    }
}

/// An image rectangle on the canvas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placed {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

/// The geometry of a board.
#[derive(Debug, Clone, PartialEq)]
pub struct BoardPlan {
    pub width: u32,
    pub height: u32,
    pub cells: Vec<Placed>,
    /// Text size unit in px (captions; the title is 1.6x, labels 0.9x).
    pub unit: f32,
    /// Where the title baseline box starts.
    pub title_at: Option<(i32, i32)>,
    /// Height reserved under each image for its caption.
    pub caption_h: u32,
    /// 1.0, or the factor the whole board was scaled down by to stay within the limits.
    pub scale: f64,
}

fn invalid(msg: impl Into<String>) -> CmError {
    CmError::invalid_input(msg.into())
}

fn scaled(v: u32, k: f64) -> u32 {
    (f64::from(v) * k).round() as u32
}

fn text_unit(cells: &[(u32, u32)]) -> f32 {
    let m = cells.iter().map(|&(w, h)| w.min(h)).min().unwrap_or(300) as f32;
    (m / 30.0).clamp(14.0, 80.0)
}

/// Lays out `sizes` (the pictures' pixel sizes). Pure.
pub fn plan(
    sizes: &[(u32, u32)],
    layout: &BoardLayout,
    opts: &BoardOptions,
) -> Result<BoardPlan, CmError> {
    opts.validate()?;
    let n = sizes.len();
    if !(MIN_SOURCES..=MAX_SOURCES).contains(&n) {
        return Err(CmError::invalid_field(
            "capture_ids",
            format!("a board needs {MIN_SOURCES} to {MAX_SOURCES} screenshots, got {n}"),
        ));
    }
    if sizes.iter().any(|&(w, h)| w == 0 || h == 0) {
        return Err(invalid("a source picture is empty"));
    }
    if layout.kind == LayoutKind::Pair && n != 2 {
        return Err(CmError::invalid_field(
            "layout.kind",
            "pair needs exactly 2 screenshots; use grid or strip for more",
        ));
    }
    let first = plan_k(sizes, layout, opts, 1.0)?;
    let area = u64::from(first.width) * u64::from(first.height);
    if first.width <= MAX_EDGE && first.height <= MAX_EDGE && area <= MAX_PIXELS {
        return Ok(first);
    }
    let k = (f64::from(MAX_EDGE) / f64::from(first.width.max(first.height)))
        .min((MAX_PIXELS as f64 / area as f64).sqrt())
        * 0.995;
    let p = plan_k(sizes, layout, opts, k)?;
    if p.width == 0 || p.height == 0 {
        return Err(invalid("the board would be empty"));
    }
    Ok(p)
}

fn plan_k(
    sizes: &[(u32, u32)],
    layout: &BoardLayout,
    opts: &BoardOptions,
    k: f64,
) -> Result<BoardPlan, CmError> {
    let n = sizes.len();
    let gap = scaled(opts.gap, k) as i32;
    let margin = scaled(opts.margin, k) as i32;
    let src: Vec<(u32, u32)> = sizes
        .iter()
        .map(|&(w, h)| (scaled(w, k).max(1), scaled(h, k).max(1)))
        .collect();
    let has_caption = |i: usize| opts.captions.get(i).is_some_and(|c| !c.trim().is_empty());
    let any_caption = (0..n).any(has_caption);

    // Cell sizes first.
    let horizontal = layout.arrangement == Arrangement::SideBySide;
    let mut dims: Vec<(u32, u32)> = Vec::with_capacity(n);
    let (mut cols, mut rows) = (n, 1usize);
    match layout.kind {
        LayoutKind::Pair | LayoutKind::Strip => {
            if horizontal {
                let h = src.iter().map(|s| s.1).min().unwrap_or(1);
                dims.extend(src.iter().map(|&(w0, h0)| {
                    (
                        ((f64::from(w0) * f64::from(h) / f64::from(h0)).round() as u32).max(1),
                        h,
                    )
                }));
            } else {
                let w = src.iter().map(|s| s.0).min().unwrap_or(1);
                dims.extend(src.iter().map(|&(w0, h0)| {
                    (
                        w,
                        ((f64::from(h0) * f64::from(w) / f64::from(w0)).round() as u32).max(1),
                    )
                }));
                (cols, rows) = (1, n);
            }
        }
        LayoutKind::Grid => {
            let c = layout
                .columns
                .map_or(if n <= 4 { 2 } else { 3 }, |c| c as usize);
            if !(1..=6).contains(&c) {
                return Err(CmError::invalid_field("layout.columns", "must be 1 to 6"));
            }
            cols = c.min(n);
            rows = n.div_ceil(cols);
            let cw = src.iter().map(|s| s.0).min().unwrap_or(1);
            let ch = src.iter().map(|s| s.1).min().unwrap_or(1);
            dims.extend(src.iter().map(|&(w0, h0)| {
                // Contain inside the uniform cell.
                let f = (f64::from(cw) / f64::from(w0)).min(f64::from(ch) / f64::from(h0));
                (
                    ((f64::from(w0) * f).round() as u32).max(1),
                    ((f64::from(h0) * f).round() as u32).max(1),
                )
            }));
            // Every cell is cw x ch; `dims` holds the fitted picture inside it.
            let unit = text_unit(&[(cw, ch)]);
            let caption_h = if any_caption {
                (unit * 1.2).ceil() as u32 + (unit * 0.5) as u32
            } else {
                0
            };
            let title_h = title_band(opts, unit, gap);
            let width = margin * 2 + cw as i32 * cols as i32 + gap * (cols as i32 - 1);
            let height = margin * 2
                + title_h
                + (ch + caption_h) as i32 * rows as i32
                + gap * (rows as i32 - 1);
            let cells = (0..n)
                .map(|i| {
                    let (r, c) = (i / cols, i % cols);
                    let x0 = margin + c as i32 * (cw as i32 + gap);
                    let y0 = margin + title_h + r as i32 * ((ch + caption_h) as i32 + gap);
                    let (w, h) = dims[i];
                    Placed {
                        x: x0 + (cw - w) as i32 / 2,
                        y: y0 + (ch - h) as i32 / 2,
                        w,
                        h,
                    }
                })
                .collect();
            return Ok(BoardPlan {
                width: width as u32,
                height: height as u32,
                cells,
                unit,
                title_at: opts.title.as_ref().map(|_| (margin, margin)),
                caption_h,
                scale: k,
            });
        }
        LayoutKind::Free => {
            if layout.slots.len() != n {
                return Err(CmError::invalid_field(
                    "layout.slots",
                    format!("free needs one slot per screenshot ({n})"),
                ));
            }
            let mut cells = Vec::with_capacity(n);
            for (i, s) in layout.slots.iter().enumerate() {
                if s.w == 0 || s.h == 0 {
                    return Err(CmError::invalid_field(
                        &format!("layout.slots[{i}]"),
                        "has no size",
                    ));
                }
                cells.push(Placed {
                    x: (f64::from(s.x) * k).round() as i32,
                    y: (f64::from(s.y) * k).round() as i32,
                    w: scaled(s.w, k).max(1),
                    h: scaled(s.h, k).max(1),
                });
            }
            let unit = text_unit(&cells.iter().map(|c| (c.w, c.h)).collect::<Vec<_>>());
            let (mut w, mut h) = layout
                .canvas
                .map_or((0, 0), |c| (scaled(c[0], k), scaled(c[1], k)));
            if layout.canvas.is_none() {
                for c in &cells {
                    w = w.max((c.x.max(0) as u32) + c.w + margin as u32);
                    h = h.max((c.y.max(0) as u32) + c.h + margin as u32);
                }
            }
            if w == 0 || h == 0 {
                return Err(invalid("the canvas is empty"));
            }
            return Ok(BoardPlan {
                width: w,
                height: h,
                cells,
                unit,
                title_at: opts.title.as_ref().map(|_| (margin, margin / 2)),
                caption_h: if any_caption { (unit * 1.7) as u32 } else { 0 },
                scale: k,
            });
        }
    }

    // Pair and Strip: one row or one column.
    let unit = text_unit(&dims);
    let caption_h = if any_caption {
        (unit * 1.2).ceil() as u32 + (unit * 0.5) as u32
    } else {
        0
    };
    let title_h = title_band(opts, unit, gap);
    let label_h = label_band(opts, layout.kind, unit) as i32;
    let mut cells = Vec::with_capacity(n);
    let (mut x, mut y) = (margin, margin + title_h + label_h);
    for &(w, h) in &dims {
        cells.push(Placed { x, y, w, h });
        if horizontal {
            x += w as i32 + gap;
        } else {
            y += (h + caption_h) as i32 + gap + label_h;
        }
    }
    let _ = (cols, rows);
    let (width, height) = if horizontal {
        let w = x - gap + margin;
        let h = margin * 2
            + title_h
            + label_h
            + dims.iter().map(|d| d.1).max().unwrap_or(0) as i32
            + caption_h as i32;
        (w, h)
    } else {
        let w = margin * 2 + dims.iter().map(|d| d.0).max().unwrap_or(0) as i32;
        (w, y - gap - label_h + margin)
    };
    Ok(BoardPlan {
        width: width as u32,
        height: height as u32,
        cells,
        unit,
        title_at: opts.title.as_ref().map(|_| (margin, margin)),
        caption_h,
        scale: k,
    })
}

/// The band above each Pair picture that holds its BEFORE or AFTER label, so the label never
/// covers the screenshot (maintainer, Nov 14, 2026). Zero when there are no labels.
pub fn label_band(opts: &BoardOptions, kind: LayoutKind, unit: f32) -> u32 {
    if opts.before_after && kind == LayoutKind::Pair {
        (unit * 0.9 * 1.35).ceil() as u32 + 8
    } else {
        0
    }
}

fn title_band(opts: &BoardOptions, unit: f32, gap: i32) -> i32 {
    match &opts.title {
        Some(t) if !t.trim().is_empty() => (unit * 1.6 * 1.2).ceil() as i32 + gap.max(8),
        _ => 0,
    }
}

fn to_pixmap(img: &RgbaImage) -> Result<Pixmap, CmError> {
    let (w, h) = img.dimensions();
    let mut pm = Pixmap::new(w, h).ok_or_else(|| invalid("a picture is too large"))?;
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
    RgbaImage::from_raw(pm.width(), pm.height(), raw).unwrap_or_default()
}

fn round_rect(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<tiny_skia::Path> {
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    let mut pb = PathBuilder::new();
    if r <= 0.5 {
        pb.push_rect(Rect::from_xywh(x, y, w, h)?);
        return pb.finish();
    }
    let k = 0.552_284_8 * r;
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.cubic_to(x + w - r + k, y, x + w, y + r - k, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.cubic_to(x + w, y + h - r + k, x + w - r + k, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.cubic_to(x + r - k, y + h, x, y + h - r + k, x, y + h - r);
    pb.line_to(x, y + r);
    pb.cubic_to(x, y + r - k, x + r - k, y, x + r, y);
    pb.close();
    pb.finish()
}

#[allow(clippy::too_many_arguments)]
fn draw_text(
    pm: &mut Pixmap,
    fonts: &mut Fonts,
    s: &str,
    px: f32,
    color: Color,
    bg: Option<Color>,
    at: (i32, i32),
    max_w: Option<f32>,
) -> Result<(), CmError> {
    let style = TextStyle {
        px,
        color,
        outline: None,
        bg,
        wrap_w: max_w,
        center: false,
    };
    if let Some(l) = text::layout(fonts, s, &style)? {
        pm.draw_pixmap(
            at.0 - l.pad,
            at.1 - l.pad,
            l.pixmap.as_ref(),
            &PixmapPaint::default(),
            Transform::identity(),
            None,
        );
    }
    Ok(())
}

/// Draws a board from the rendered `sources` (2 to 12, in order). `sources` are never modified.
pub fn render_board(
    sources: &[RgbaImage],
    layout: &BoardLayout,
    opts: &BoardOptions,
) -> Result<RgbaImage, CmError> {
    let sizes: Vec<(u32, u32)> = sources.iter().map(RgbaImage::dimensions).collect();
    let plan = plan(&sizes, layout, opts)?;
    let mut canvas =
        Pixmap::new(plan.width, plan.height).ok_or_else(|| invalid("the board is too large"))?;
    let [r, g, b, a] = opts.background.rgba();
    if a > 0 {
        canvas.fill(tiny_skia::Color::from_rgba8(r, g, b, a));
    }
    let dark = opts.background.is_dark();
    let ink = if dark {
        Color::rgb(0xF2, 0xF2, 0xF4)
    } else {
        Color::rgb(0x1B, 0x1C, 0x1F)
    };
    let radius = (f64::from(opts.radius) * plan.scale) as f32;
    let mut fonts = Fonts::bundled();

    if let (Some(t), Some(at)) = (&opts.title, plan.title_at) {
        draw_text(
            &mut canvas,
            &mut fonts,
            t,
            plan.unit * 1.6,
            ink,
            None,
            at,
            None,
        )?;
    }
    for (i, (src, cell)) in sources.iter().zip(&plan.cells).enumerate() {
        let resized = if src.dimensions() == (cell.w, cell.h) {
            src.clone()
        } else {
            image::imageops::resize(src, cell.w, cell.h, FilterType::Lanczos3)
        };
        let mut pic = to_pixmap(&resized)?;
        drop(resized);
        let (x, y) = (cell.x as f32, cell.y as f32);
        if opts.shadow {
            let spread = (plan.unit * 0.5).max(6.0);
            let steps = 8;
            for s in (1..=steps).rev() {
                let grow = spread * s as f32 / steps as f32;
                if let Some(p) = round_rect(
                    x - grow,
                    y - grow + spread * 0.4,
                    cell.w as f32 + 2.0 * grow,
                    cell.h as f32 + 2.0 * grow,
                    radius + grow,
                ) {
                    let mut paint = Paint::default();
                    paint.set_color_rgba8(0, 0, 0, 9);
                    paint.anti_alias = true;
                    canvas.fill_path(&p, &paint, FillRule::Winding, Transform::identity(), None);
                }
            }
        }
        if radius > 0.5 {
            if let (Some(p), Some(mut mask)) = (
                round_rect(0.0, 0.0, cell.w as f32, cell.h as f32, radius),
                Mask::new(cell.w, cell.h),
            ) {
                mask.fill_path(&p, FillRule::Winding, true, Transform::identity());
                pic.apply_mask(&mask);
            }
        }
        canvas.draw_pixmap(
            cell.x,
            cell.y,
            pic.as_ref(),
            &PixmapPaint::default(),
            Transform::identity(),
            None,
        );
        drop(pic);

        // BEFORE and AFTER (Pair).
        if opts.before_after && layout.kind == LayoutKind::Pair {
            let words = opts
                .label_words
                .clone()
                .unwrap_or_else(|| ["BEFORE".to_owned(), "AFTER".to_owned()]);
            let bg = if i == 0 {
                Color::rgb(0x55, 0x5B, 0x66)
            } else {
                Color::rgb(0x1F, 0x8A, 0x4C)
            };
            // In the band above the picture, left-aligned with it.
            let band = label_band(opts, layout.kind, plan.unit) as i32;
            draw_text(
                &mut canvas,
                &mut fonts,
                &words[i.min(1)],
                plan.unit * 0.9,
                Color::WHITE,
                Some(bg),
                (cell.x, cell.y - band + 3),
                None,
            )?;
        }
        // Strip: a number on each frame.
        if layout.kind == LayoutKind::Strip {
            let d = plan.unit * 1.7;
            let (cx, cy) = (x + plan.unit * 0.3 + d / 2.0, y + plan.unit * 0.3 + d / 2.0);
            let mut pb = PathBuilder::new();
            pb.push_circle(cx, cy, d / 2.0);
            if let Some(p) = pb.finish() {
                let mut paint = Paint::default();
                paint.set_color_rgba8(0x1B, 0x1C, 0x1F, 235);
                paint.anti_alias = true;
                canvas.fill_path(&p, &paint, FillRule::Winding, Transform::identity(), None);
            }
            let s = (i + 1).to_string();
            let px = plan.unit * 0.95;
            let tw = s.chars().count() as f32 * px * 0.56;
            draw_text(
                &mut canvas,
                &mut fonts,
                &s,
                px,
                Color::WHITE,
                None,
                ((cx - tw / 2.0) as i32, (cy - px * 0.6) as i32),
                None,
            )?;
        }
        // Caption under the image.
        if let Some(c) = opts.captions.get(i).filter(|c| !c.trim().is_empty()) {
            draw_text(
                &mut canvas,
                &mut fonts,
                c,
                plan.unit,
                ink,
                None,
                (cell.x, cell.y + cell.h as i32 + (plan.unit * 0.3) as i32),
                Some(cell.w.max(40) as f32),
            )?;
        }
    }
    Ok(from_pixmap(&canvas))
}

/// Composes a transparent board on white (a JPEG or another export without alpha).
pub fn flatten_on_white(img: &RgbaImage) -> RgbaImage {
    let mut out = img.clone();
    for p in out.pixels_mut() {
        let a = u32::from(p[3]);
        for c in 0..3 {
            p[c] = ((u32::from(p[c]) * a + 255 * (255 - a) + 127) / 255) as u8;
        }
        p[3] = 255;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32, c: [u8; 4]) -> RgbaImage {
        RgbaImage::from_pixel(w, h, image::Rgba(c))
    }

    /// BRD-1.AC1: two pictures of different sizes, side by side, scaled to the same height with
    /// the gap and the margin as set.
    #[test]
    fn pair_scales_to_one_height_with_gap_and_margin() {
        let opts = BoardOptions {
            gap: 20,
            margin: 10,
            ..Default::default()
        };
        let p = plan(
            &[(400, 200), (300, 300)],
            &BoardLayout::new(LayoutKind::Pair),
            &opts,
        )
        .unwrap();
        assert_eq!(p.cells[0].h, 200);
        assert_eq!(p.cells[1].h, 200);
        assert_eq!((p.cells[0].w, p.cells[1].w), (400, 200));
        assert_eq!(p.cells[0].x, 10);
        assert_eq!(p.cells[1].x, 10 + 400 + 20);
        assert_eq!((p.width, p.height), (10 + 400 + 20 + 200 + 10, 220));
        let img = render_board(
            &[
                solid(400, 200, [255, 0, 0, 255]),
                solid(300, 300, [0, 0, 255, 255]),
            ],
            &BoardLayout::new(LayoutKind::Pair),
            &opts,
        )
        .unwrap();
        assert_eq!(img.dimensions(), (p.width, p.height));
        assert_eq!(img.get_pixel(5, 5).0, [255, 255, 255, 255]);
        assert_eq!(img.get_pixel(100, 100).0, [255, 0, 0, 255]);
        assert_eq!(img.get_pixel(10 + 400 + 20 + 50, 100).0, [0, 0, 255, 255]);
        assert_eq!(img.get_pixel(10 + 400 + 10, 100).0, [255, 255, 255, 255]);
    }

    #[test]
    fn stacked_pair_scales_to_one_width() {
        let layout = BoardLayout {
            arrangement: Arrangement::Stacked,
            ..BoardLayout::new(LayoutKind::Pair)
        };
        let p = plan(&[(400, 200), (200, 100)], &layout, &BoardOptions::default()).unwrap();
        assert_eq!((p.cells[0].w, p.cells[1].w), (200, 200));
        assert_eq!(p.cells[1].y, 32 + 100 + 24);
    }

    #[test]
    fn grid_fills_in_order_and_free_needs_slots() {
        let sizes = vec![(100, 80); 5];
        let p = plan(
            &sizes,
            &BoardLayout::new(LayoutKind::Grid),
            &BoardOptions::default(),
        )
        .unwrap();
        // 5 images: 3 columns, 2 rows.
        assert_eq!(p.cells[3].y, p.cells[0].y + 80 + 24);
        assert_eq!(p.cells[2].x, 32 + 2 * (100 + 24));
        let e = plan(
            &sizes[..2],
            &BoardLayout::new(LayoutKind::Free),
            &BoardOptions::default(),
        )
        .unwrap_err();
        assert!(e.to_string().contains("layout.slots"));
        let free = BoardLayout {
            slots: vec![
                Slot {
                    x: 0,
                    y: 0,
                    w: 50,
                    h: 50,
                },
                Slot {
                    x: 60,
                    y: 10,
                    w: 40,
                    h: 40,
                },
            ],
            ..BoardLayout::new(LayoutKind::Free)
        };
        let p = plan(&sizes[..2], &free, &BoardOptions::default()).unwrap();
        assert_eq!((p.width, p.height), (132, 82));
    }

    #[test]
    fn the_count_and_the_options_are_checked() {
        let one = [(10, 10)];
        assert!(plan(
            &one,
            &BoardLayout::new(LayoutKind::Grid),
            &BoardOptions::default()
        )
        .is_err());
        let three = [(10, 10); 3];
        assert!(plan(
            &three,
            &BoardLayout::new(LayoutKind::Pair),
            &BoardOptions::default()
        )
        .is_err());
        let o = BoardOptions {
            gap: 500,
            ..Default::default()
        };
        assert!(o
            .validate()
            .unwrap_err()
            .to_string()
            .contains("options.gap"));
        assert_eq!(Background::parse("#102030").unwrap().as_string(), "#102030");
        assert!(Background::parse("blue").is_err());
    }

    #[test]
    fn a_huge_board_is_scaled_down_as_a_whole() {
        let sizes = vec![(9000, 9000); 2];
        let p = plan(
            &sizes,
            &BoardLayout::new(LayoutKind::Pair),
            &BoardOptions::default(),
        )
        .unwrap();
        assert!(p.width <= MAX_EDGE && u64::from(p.width) * u64::from(p.height) <= MAX_PIXELS);
    }

    #[test]
    fn labels_numbers_captions_title_and_shadow_draw_something() {
        let opts = BoardOptions {
            title: Some("Release".into()),
            captions: vec!["one".into(), "two".into()],
            before_after: true,
            shadow: true,
            radius: 12,
            background: Background::Dark,
            ..Default::default()
        };
        let src = [
            solid(300, 200, [200, 200, 200, 255]),
            solid(300, 200, [100, 100, 100, 255]),
        ];
        let a = render_board(&src, &BoardLayout::new(LayoutKind::Pair), &opts).unwrap();
        let strip = BoardLayout::new(LayoutKind::Strip);
        let b = render_board(&src, &strip, &BoardOptions::default()).unwrap();
        assert!(a.height() > 200 && b.width() > 600);
        // Rounded corner: the picture's own corner pixel is background, not grey.
        let cell = plan(&[(300, 200); 2], &BoardLayout::new(LayoutKind::Pair), &opts)
            .unwrap()
            .cells[0];
        let corner = a.get_pixel(cell.x as u32, cell.y as u32);
        assert_ne!(corner.0, [200, 200, 200, 255]);
        // Deterministic.
        let a2 = render_board(&src, &BoardLayout::new(LayoutKind::Pair), &opts).unwrap();
        assert_eq!(a.as_raw(), a2.as_raw());
    }

    #[test]
    fn transparent_background_keeps_alpha_and_flattens_on_white() {
        let opts = BoardOptions {
            background: Background::Transparent,
            ..Default::default()
        };
        let src = [solid(40, 40, [0, 0, 0, 255]), solid(40, 40, [0, 0, 0, 255])];
        let img = render_board(&src, &BoardLayout::new(LayoutKind::Pair), &opts).unwrap();
        assert_eq!(img.get_pixel(1, 1).0[3], 0);
        assert_eq!(
            flatten_on_white(&img).get_pixel(1, 1).0,
            [255, 255, 255, 255]
        );
    }
}
