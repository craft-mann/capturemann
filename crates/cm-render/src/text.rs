//! Text layers via cosmic-text: fill, optional dilated outline, optional background box.

use cm_core::CmError;
use cosmic_text::{
    fontdb, Align, Attrs, Buffer, Color as CtColor, Family, FontSystem, Metrics, Shaping,
    SwashCache,
};
use tiny_skia::Pixmap;

use crate::ops::Color;

/// Bundled UI font (Inter Regular, SIL OFL 1.1; see `assets/Inter-LICENSE.txt`).
static INTER_REGULAR: &[u8] = include_bytes!("../assets/Inter-Regular.ttf");
/// Bundled CJK fallback (Noto Sans SC subset, SIL OFL 1.1; see `assets/README.md`).
static NOTO_SANS_SC: &[u8] = include_bytes!("../assets/NotoSansSC-Subset.ttf");
/// Bundled monochrome emoji fallback (Noto Emoji, SIL OFL 1.1).
static NOTO_EMOJI: &[u8] = include_bytes!("../assets/NotoEmoji-Regular.ttf");

/// Outline stroke radius in px (DESIGN: "2px black outline").
pub(crate) const OUTLINE_PX: i32 = 2;
/// Padding around the text box when a `bg` is drawn.
pub(crate) const BG_PAD: i32 = 4;
const MAX_LAYER_DIM: f32 = 16_384.0;
/// Largest text layer in pixels (R1-9): the fill, output, outline-alpha and copy buffers are
/// each this big (4 bytes per pixel), so 32 Mpx stays near 0.5 GB worst case.
pub(crate) const MAX_LAYER_AREA: f64 = 32_000_000.0;
/// Longest text laid out, in chars (matches cm-core's `MAX_TEXT_CHARS` validation).
const MAX_TEXT_CHARS: usize = cm_core::MAX_TEXT_CHARS;

pub(crate) struct Fonts {
    pub system: FontSystem,
    pub swash: SwashCache,
}

impl Fonts {
    /// Deterministic: only bundled fonts (Inter, plus Noto CJK and emoji fallbacks) are available.
    pub fn bundled() -> Self {
        Self::build(false)
    }

    /// Bundled fonts plus installed system fonts (e.g. color emoji, scripts beyond the bundled
    /// subset); glyph output is then machine dependent.
    pub fn with_system_fonts() -> Self {
        Self::build(true)
    }

    fn build(system_fonts: bool) -> Self {
        let mut db = fontdb::Database::new();
        if system_fonts {
            db.load_system_fonts();
        }
        db.load_font_data(INTER_REGULAR.to_vec());
        db.load_font_data(NOTO_SANS_SC.to_vec());
        db.load_font_data(NOTO_EMOJI.to_vec());
        db.set_sans_serif_family("Inter");
        Fonts {
            system: FontSystem::new_with_locale_and_db("en-US".to_string(), db),
            swash: SwashCache::new(),
        }
    }
}

/// A rendered text layer. `pad` is the margin between the layer edge and the text box origin.
pub(crate) struct TextLayer {
    pub pixmap: Pixmap,
    pub pad: i32,
    /// Advance width of the widest line.
    pub text_w: f32,
    /// Baseline of the first line, relative to the text box top.
    pub baseline: f32,
}

pub(crate) struct TextStyle {
    pub px: f32,
    pub color: Color,
    pub outline: Option<Color>,
    pub bg: Option<Color>,
    /// Wrap lines at this width in px (annotations never wrap: `None`).
    pub wrap_w: Option<f32>,
    /// Center each line inside the widest one (captions); annotations are left aligned.
    pub center: bool,
}

fn blend_px(dst: &mut [u8], r: u8, g: u8, b: u8, a: u8) {
    // premultiplied source-over, integer math
    let mul = |c: u8| ((u32::from(c) * u32::from(a) + 127) / 255) as u8;
    let inv = 255 - u32::from(a);
    let over = |s: u8, d: u8| (u32::from(s) + (u32::from(d) * inv + 127) / 255).min(255) as u8;
    dst[0] = over(mul(r), dst[0]);
    dst[1] = over(mul(g), dst[1]);
    dst[2] = over(mul(b), dst[2]);
    dst[3] = over(a, dst[3]);
}

pub(crate) fn layout(
    fonts: &mut Fonts,
    text: &str,
    style: &TextStyle,
) -> Result<Option<TextLayer>, CmError> {
    if text.is_empty() {
        return Ok(None);
    }
    if text.chars().count() > MAX_TEXT_CHARS {
        return Err(CmError::invalid_input("text is too long to render"));
    }
    let metrics = Metrics::new(style.px, style.px * 1.2);
    let mut buffer = Buffer::new(&mut fonts.system, metrics);
    buffer.set_size(style.wrap_w, None);
    let attrs = Attrs::new().family(Family::Name("Inter"));
    buffer.set_text(text, &attrs, Shaping::Advanced, None);
    buffer.shape_until_scroll(&mut fonts.system, false);

    if style.center {
        // Measure the widest line first, then align every line inside that width.
        let widest = buffer
            .layout_runs()
            .map(|r| r.line_w)
            .fold(0f32, f32::max)
            .ceil();
        for line in buffer.lines.iter_mut() {
            line.set_align(Some(Align::Center));
        }
        buffer.set_size(Some(widest.max(1.0)), None);
        buffer.shape_until_scroll(&mut fonts.system, false);
    }
    let mut max_w = 0f32;
    let mut lines = 0usize;
    let mut baseline = style.px;
    for (i, run) in buffer.layout_runs().enumerate() {
        max_w = max_w.max(run.line_w);
        lines += 1;
        if i == 0 {
            baseline = run.line_y;
        }
    }
    if lines == 0 {
        return Ok(None);
    }
    let outline_r = if style.outline.is_some() {
        OUTLINE_PX
    } else {
        0
    };
    let pad = outline_r.max(if style.bg.is_some() { BG_PAD } else { 0 }) + 2;
    let box_w = max_w.ceil();
    let box_h = (lines as f32 * metrics.line_height).ceil();
    let (lw, lh) = (box_w + 2.0 * pad as f32, box_h + 2.0 * pad as f32);
    if lw > MAX_LAYER_DIM || lh > MAX_LAYER_DIM || f64::from(lw) * f64::from(lh) > MAX_LAYER_AREA {
        return Err(CmError::invalid_input("text is too large to render"));
    }
    let (lw, lh) = (lw as u32, lh as u32);
    let mut fill = Pixmap::new(lw, lh)
        .ok_or_else(|| CmError::invalid_input("text layer allocation failed"))?;
    let stride = lw as usize * 4;
    {
        let data = fill.data_mut();
        let c = CtColor::rgba(style.color.r, style.color.g, style.color.b, 255);
        buffer.draw(&mut fonts.system, &mut fonts.swash, c, |x, y, w, h, col| {
            let a = col.a();
            if a == 0 {
                return;
            }
            for yy in y..y + h as i32 {
                for xx in x..x + w as i32 {
                    let (px, py) = (xx + pad, yy + pad);
                    if px < 0 || py < 0 || px >= lw as i32 || py >= lh as i32 {
                        continue;
                    }
                    let o = py as usize * stride + px as usize * 4;
                    blend_px(&mut data[o..o + 4], col.r(), col.g(), col.b(), a);
                }
            }
        });
    }

    let mut out = Pixmap::new(lw, lh)
        .ok_or_else(|| CmError::invalid_input("text layer allocation failed"))?;
    if let Some(bg) = style.bg {
        let data = out.data_mut();
        let x0 = (pad - BG_PAD).max(0) as usize;
        let y0 = (pad - BG_PAD).max(0) as usize;
        let x1 = ((pad as f32 + box_w) as usize + BG_PAD as usize).min(lw as usize);
        let y1 = ((pad as f32 + box_h) as usize + BG_PAD as usize).min(lh as usize);
        for y in y0..y1 {
            for x in x0..x1 {
                let o = y * stride + x * 4;
                data[o..o + 4].copy_from_slice(&[bg.r, bg.g, bg.b, 255]);
            }
        }
    }
    if let Some(oc) = style.outline {
        let alpha: Vec<u8> = fill
            .data()
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| p[3])
            .collect();
        let data = out.data_mut();
        let r = OUTLINE_PX;
        for y in 0..lh as i32 {
            for x in 0..lw as i32 {
                let mut m = 0u8;
                for dy in -r..=r {
                    for dx in -r..=r {
                        if dx * dx + dy * dy > r * r {
                            continue;
                        }
                        let (sx, sy) = (x + dx, y + dy);
                        if sx >= 0 && sy >= 0 && sx < lw as i32 && sy < lh as i32 {
                            m = m.max(alpha[sy as usize * lw as usize + sx as usize]);
                        }
                    }
                }
                if m > 0 {
                    let o = y as usize * stride + x as usize * 4;
                    blend_px(&mut data[o..o + 4], oc.r, oc.g, oc.b, m);
                }
            }
        }
    }
    // fill over (bg + outline)
    {
        let src: Vec<u8> = fill.data().to_vec();
        let data = out.data_mut();
        for (d, s) in data
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(src.as_chunks::<4>().0.iter())
        {
            if s[3] == 0 {
                continue;
            }
            // `s` is premultiplied: source-over without re-multiplying
            let inv = 255 - u32::from(s[3]);
            for c in 0..4 {
                d[c] = (u32::from(s[c]) + (u32::from(d[c]) * inv + 127) / 255).min(255) as u8;
            }
        }
    }
    Ok(Some(TextLayer {
        pixmap: out,
        pad,
        text_w: max_w,
        baseline,
    }))
}
