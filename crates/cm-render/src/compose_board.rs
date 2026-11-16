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
            margin: 40,
            background: Background::LightGrey,
            radius: 12,
            shadow: true,
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
    /// Factor the gap, margin, radius and shadow were enlarged by so that the spacing keeps
    /// its proportion on large pictures (1.0 to 3.0; 1.0 for Free, where the user places it).
    pub spacing: f64,
}

/// Pictures this small (shortest side, px) get the gap and margin as set; larger ones get them
/// enlarged in proportion, up to three times.
const SPACING_REF: f64 = 400.0;

fn spacing(cells: &[(u32, u32)]) -> f64 {
    let m = cells.iter().map(|&(w, h)| w.min(h)).min().unwrap_or(1);
    (f64::from(m) / SPACING_REF).clamp(1.0, 3.0)
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
    let gap0 = scaled(opts.gap, k);
    let margin0 = scaled(opts.margin, k);
    let margin = margin0 as i32;
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
            let sp = spacing(&[(cw, ch)]);
            let gap = (f64::from(gap0) * sp).round() as i32;
            let margin = (f64::from(margin0) * sp).round() as i32;
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
                spacing: sp,
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
                spacing: 1.0,
            });
        }
    }

    // Pair and Strip: one row or one column.
    let sp = spacing(&dims);
    let gap = (f64::from(gap0) * sp).round() as i32;
    let margin = (f64::from(margin0) * sp).round() as i32;
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
        spacing: sp,
    })
}

/// The band above each Pair picture that holds its BEFORE or AFTER label, so the label never
/// covers the screenshot (maintainer, Nov 14, 2026). Zero when there are no labels.
pub fn label_band(opts: &BoardOptions, kind: LayoutKind, unit: f32) -> u32 {
    if opts.before_after && kind == LayoutKind::Pair {
        let (_, h, _) = stamp_metrics(unit * STAMP_K);
        h.ceil() as u32 + (unit * 0.35).ceil() as u32
    } else {
        0
    }
}

/// Stamp text size relative to the text unit.
const STAMP_K: f32 = 1.3;

/// The BEFORE / AFTER stamp (design S10): a white pill with a coloured border and coloured bold
/// letters. Returns the vertical padding, the stamp height and the border width for text of
/// `px`.
fn stamp_metrics(px: f32) -> (f32, f32, f32) {
    let border = (px * 0.12).max(2.0);
    let pad_y = px * 0.12;
    (pad_y, px * 1.2 + 2.0 * pad_y + 2.0 * border, border)
}

/// Draws `s` bold-looking (the bundled Inter is regular only: a second pass one step to the
/// side) with its box centred in `x0..x0+w`.
#[allow(clippy::too_many_arguments)]
fn draw_text_centered(
    pm: &mut Pixmap,
    fonts: &mut Fonts,
    s: &str,
    px: f32,
    color: Color,
    (x0, w): (i32, i32),
    y: i32,
    wrap: Option<f32>,
    bold: bool,
) -> Result<(), CmError> {
    let style = TextStyle {
        px,
        color,
        outline: None,
        bg: None,
        wrap_w: wrap,
        center: true,
    };
    if let Some(l) = text::layout(fonts, s, &style)? {
        let x = x0 + ((w as f32 - l.text_w) / 2.0).max(0.0) as i32 - l.pad;
        let passes = if bold { 2 } else { 1 };
        for k in 0..passes {
            let dx = (k as f32 * (px * 0.035).max(1.0)).round() as i32;
            pm.draw_pixmap(
                x + dx,
                y - l.pad,
                l.pixmap.as_ref(),
                &PixmapPaint::default(),
                Transform::identity(),
                None,
            );
        }
    }
    Ok(())
}

/// Draws the stamp for `word` with its left edge at `x` and its middle at `mid_y`, tilted a
/// little like a rubber stamp; never wider than `max_w` (a long translation shrinks).
#[allow(clippy::too_many_arguments)]
fn draw_stamp(
    canvas: &mut Pixmap,
    fonts: &mut Fonts,
    word: &str,
    px: f32,
    color: Color,
    x: f32,
    mid_y: f32,
    max_w: f32,
) -> Result<(), CmError> {
    let measure = |fonts: &mut Fonts, px: f32| -> Result<f32, CmError> {
        let st = TextStyle {
            px,
            color,
            outline: None,
            bg: None,
            wrap_w: None,
            center: false,
        };
        Ok(text::layout(fonts, word, &st)?.map_or(0.0, |l| l.text_w))
    };
    let mut px = px;
    let mut tw = measure(fonts, px)?;
    let wanted = |px: f32, tw: f32| tw + px * 1.1 + 2.0 * (px * 0.12).max(2.0);
    if wanted(px, tw) > max_w && tw > 0.0 {
        px = (px * max_w / wanted(px, tw)).max(8.0);
        tw = measure(fonts, px)?;
    }
    let (pad_y, h, border) = stamp_metrics(px);
    let w = wanted(px, tw).ceil();
    let margin = 6.0;
    let (pw, ph) = ((w + 2.0 * margin) as u32, (h + 2.0 * margin) as u32);
    let Some(mut st) = Pixmap::new(pw, ph) else {
        return Ok(());
    };
    let rgb = |c: Color, a: u8| {
        let mut p = Paint::default();
        p.set_color_rgba8(c.r, c.g, c.b, a);
        p.anti_alias = true;
        p
    };
    let r = px * 0.4;
    if let Some(path) = round_rect(margin, margin, w, h, r) {
        st.fill_path(
            &path,
            &rgb(Color::WHITE, 235),
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
    if let Some(path) = round_rect(
        margin + border / 2.0,
        margin + border / 2.0,
        w - border,
        h - border,
        (r - border / 2.0).max(0.0),
    ) {
        let stroke = tiny_skia::Stroke {
            width: border,
            ..Default::default()
        };
        st.stroke_path(
            &path,
            &rgb(color, 255),
            &stroke,
            Transform::identity(),
            None,
        );
    }
    let ty = margin as i32 + (pad_y + border) as i32;
    draw_text_centered(
        &mut st,
        fonts,
        word,
        px,
        color,
        (margin as i32, w as i32),
        ty,
        None,
        true,
    )?;
    let (ox, oy) = (x - margin, mid_y - ph as f32 / 2.0);
    let paint = PixmapPaint {
        quality: tiny_skia::FilterQuality::Bilinear,
        ..PixmapPaint::default()
    };
    canvas.draw_pixmap(
        ox.round() as i32,
        oy.round() as i32,
        st.as_ref(),
        &paint,
        Transform::from_rotate_at(
            -3.0,
            ox.round() + pw as f32 / 2.0,
            oy.round() + ph as f32 / 2.0,
        ),
        None,
    );
    Ok(())
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
    let radius = (f64::from(opts.radius) * plan.scale * plan.spacing) as f32;
    let mut fonts = Fonts::bundled();

    if let (Some(t), Some(at)) = (&opts.title, plan.title_at) {
        draw_text_centered(
            &mut canvas,
            &mut fonts,
            t,
            plan.unit * 1.6,
            ink,
            (0, plan.width as i32),
            at.1,
            Some(plan.width as f32 * 0.9),
            true,
        )?;
    }
    // Every shadow first, so one picture's shadow never darkens its neighbour.
    if opts.shadow {
        for cell in &plan.cells {
            draw_shadow(&mut canvas, cell, radius);
        }
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
            let color = if i == 0 {
                Color::rgb(0xDC, 0x26, 0x26)
            } else {
                Color::rgb(0x16, 0xA3, 0x4A)
            };
            // A stamp in the band above the picture, so it never covers the screenshot.
            let band = label_band(opts, layout.kind, plan.unit) as f32;
            draw_stamp(
                &mut canvas,
                &mut fonts,
                &words[i.min(1)],
                plan.unit * STAMP_K,
                color,
                cell.x as f32 + plan.unit * 0.3,
                cell.y as f32 - band / 2.0 - plan.unit * 0.05,
                (cell.w as f32 * 0.9).max(40.0),
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
            draw_text_centered(
                &mut canvas,
                &mut fonts,
                c,
                plan.unit,
                ink,
                (cell.x, cell.w as i32),
                cell.y + cell.h as i32 + (plan.unit * 0.3) as i32,
                Some(cell.w.max(40) as f32),
                false,
            )?;
        }
    }
    Ok(from_pixmap(&canvas))
}

/// A soft drop shadow (design: `0 12px 30px rgba(0,0,0,.18)`): many faint rings, deterministic.
fn draw_shadow(canvas: &mut Pixmap, cell: &Placed, radius: f32) {
    let short = cell.w.min(cell.h) as f32;
    let blur = (short * 0.06).clamp(8.0, 36.0);
    let dy = blur * 0.4;
    let rings = 18;
    let mut paint = Paint::default();
    paint.set_color_rgba8(0, 0, 0, 3);
    paint.anti_alias = true;
    for s in (0..rings).rev() {
        let grow = blur * (s as f32 / (rings - 1) as f32) - blur * 0.25;
        if let Some(p) = round_rect(
            cell.x as f32 - grow,
            cell.y as f32 - grow + dy,
            cell.w as f32 + 2.0 * grow,
            cell.h as f32 + 2.0 * grow,
            (radius + grow).max(0.0),
        ) {
            canvas.fill_path(&p, &paint, FillRule::Winding, Transform::identity(), None);
        }
    }
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
            background: Background::White,
            shadow: false,
            radius: 0,
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
        assert_eq!(p.cells[1].y, 40 + 100 + 24);
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
        assert_eq!(p.cells[2].x, 40 + 2 * (100 + 24));
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
        assert_eq!((p.width, p.height), (140, 90));
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
            shadow: false,
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

    #[test]
    fn spacing_grows_with_big_pictures_but_not_small_or_free_ones() {
        let opts = BoardOptions {
            gap: 20,
            margin: 30,
            ..Default::default()
        };
        let pair = BoardLayout::new(LayoutKind::Pair);
        let small = plan(&[(300, 300), (300, 300)], &pair, &opts).unwrap();
        assert_eq!((small.cells[0].x, small.spacing), (30, 1.0));
        let big = plan(&[(1200, 800), (1200, 800)], &pair, &opts).unwrap();
        assert_eq!(big.spacing, 2.0);
        assert_eq!(big.cells[0].x, 60);
        assert_eq!(big.cells[1].x, 60 + 1200 + 40);
        let huge = plan(&[(4000, 3000), (4000, 3000)], &pair, &opts).unwrap();
        assert_eq!(huge.spacing, 3.0);
    }

    /// The pair labels are stamps: a coloured border (red BEFORE, green AFTER) around a light
    /// pill, in the band above each picture, and the picture itself stays untouched.
    #[test]
    fn pair_labels_are_coloured_stamps_in_the_band() {
        for words in [None, Some(["TRƯỚC".to_owned(), "SAU".to_owned()])] {
            let opts = BoardOptions {
                before_after: true,
                shadow: false,
                background: Background::Dark,
                label_words: words,
                ..Default::default()
            };
            let src = [
                solid(600, 400, [30, 60, 200, 255]),
                solid(600, 400, [30, 60, 200, 255]),
            ];
            let layout = BoardLayout::new(LayoutKind::Pair);
            let img = render_board(&src, &layout, &opts).unwrap();
            let plan = plan(&[(600, 400); 2], &layout, &opts).unwrap();
            let c = plan.cells[0];
            let (mut red, mut white) = (0, 0);
            for y in 0..c.y as u32 {
                for x in c.x as u32..(c.x as u32 + c.w) {
                    let p = img.get_pixel(x, y).0;
                    if p[0] > 190 && p[1] < 80 && p[2] < 80 {
                        red += 1;
                    }
                    if p[0] > 230 && p[1] > 230 && p[2] > 230 {
                        white += 1;
                    }
                }
            }
            assert!(red > 50 && white > 200, "red {red} white {white}");
            let mut green = 0;
            let c1 = plan.cells[1];
            for y in 0..c1.y as u32 {
                for x in c1.x as u32..(c1.x as u32 + c1.w) {
                    let p = img.get_pixel(x, y).0;
                    if p[1] > 140 && p[0] < 60 && p[2] < 110 {
                        green += 1;
                    }
                }
            }
            assert!(green > 50, "green {green}");
            // The picture is not covered.
            assert_eq!(
                img.get_pixel(c.x as u32 + 5, c.y as u32 + 5).0,
                [30, 60, 200, 255]
            );
        }
    }

    /// One picture's shadow is not painted over its neighbour.
    #[test]
    fn a_shadow_never_darkens_the_neighbouring_picture() {
        let opts = BoardOptions {
            gap: 4,
            ..Default::default()
        };
        let src = [
            solid(300, 300, [250, 250, 250, 255]),
            solid(300, 300, [250, 250, 250, 255]),
        ];
        let layout = BoardLayout::new(LayoutKind::Pair);
        let img = render_board(&src, &layout, &opts).unwrap();
        let c = plan(&[(300, 300); 2], &layout, &opts).unwrap().cells[0];
        let p = img.get_pixel(c.x as u32 + 299, c.y as u32 + 150).0;
        assert_eq!(p, [250, 250, 250, 255]);
    }

    /// Writes sample boards for a look by eye: `CM_BOARD_OUT=<dir> cargo test -p cm-render
    /// --lib write_sample_boards -- --ignored`.
    #[test]
    #[ignore]
    fn write_sample_boards() {
        let Ok(dir) = std::env::var("CM_BOARD_OUT") else {
            return;
        };
        let shot = |w: u32, h: u32, k: u8| {
            let mut im = RgbaImage::from_pixel(w, h, image::Rgba([255, 255, 255, 255]));
            for y in 0..h {
                for x in 0..w {
                    if y < h / 12 {
                        im.put_pixel(x, y, image::Rgba([240, 240, 243, 255]));
                    } else if x < w / 6 {
                        im.put_pixel(x, y, image::Rgba([246, 246, 248, 255]));
                    } else if (x / (w / 14)).is_multiple_of(2)
                        && y > h / 2
                        && y < h - h / 8 - (x % 97) * h / 400
                    {
                        im.put_pixel(x, y, image::Rgba([255, 140u8.saturating_add(k), 30, 255]));
                    }
                }
            }
            im
        };
        let sources = [shot(1800, 1100, 0), shot(1900, 1200, 40)];
        let pair = BoardLayout::new(LayoutKind::Pair);
        let cases: Vec<(&str, BoardLayout, BoardOptions, usize)> = vec![
            (
                "pair-en",
                pair.clone(),
                BoardOptions {
                    before_after: true,
                    title: Some("Billing page: before and after".into()),
                    captions: vec![
                        "Team plan, card on file".into(),
                        "Business plan after the upgrade".into(),
                    ],
                    ..Default::default()
                },
                2,
            ),
            (
                "pair-vi",
                pair.clone(),
                BoardOptions {
                    before_after: true,
                    label_words: Some(["TRƯỚC".into(), "SAU".into()]),
                    title: Some("Trang thanh toán: trước và sau".into()),
                    ..Default::default()
                },
                2,
            ),
            (
                "pair-dark-noshadow",
                pair,
                BoardOptions {
                    before_after: true,
                    background: Background::Dark,
                    shadow: false,
                    ..Default::default()
                },
                2,
            ),
            (
                "grid4",
                BoardLayout::new(LayoutKind::Grid),
                BoardOptions {
                    captions: vec!["a".into(), "b".into(), "c".into(), "d".into()],
                    ..Default::default()
                },
                4,
            ),
        ];
        for (name, layout, opts, n) in cases {
            let src: Vec<RgbaImage> = (0..n).map(|i| sources[i % 2].clone()).collect();
            let img = render_board(&src, &layout, &opts).unwrap();
            let small = image::imageops::resize(
                &img,
                img.width().min(1400),
                img.height() * img.width().min(1400) / img.width(),
                FilterType::Lanczos3,
            );
            small.save(format!("{dir}/{name}.png")).unwrap();
        }
    }
}
