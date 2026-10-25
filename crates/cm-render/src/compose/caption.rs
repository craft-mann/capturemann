//! The caption layer (AI-1): one caption line drawn over the finished frame.
//!
//! Style (specs/features/07-ai.md): white text, weight 600 on a rounded 60% black box, size
//! scaling with the output height, kept inside the platform's safe area.
//!
//! * **Font.** Inter, from the bundled `Inter-Regular.ttf`. The bundle has no SemiBold file (direct
//!   font downloads were not available when the fonts were added, see `assets/README.md`), so
//!   the weight is built by thickening the glyphs by [`EMBOLDEN_EM`] of the font size. Dropping
//!   `Inter-SemiBold.ttf` next to the Regular file and registering it in `text.rs` replaces
//!   this without touching the layout.
//! * **Size.** [`CaptionLayer::size`] (default [`CAPTION_SIZE_FRAC`] = 4.5%) of the output's
//!   shorter side, at least [`MIN_CAPTION_PX`]: the height on landscape (the spec's "scales with
//!   the export height") and the width on 9:16, where 4.5% of the height would be oversized
//!   (86 px on 1080x1920 wraps a normal line into five rows).
//! * **Box.** Text centered inside it; padding [`PAD_X_EM`] and [`PAD_Y_EM`] of the font size,
//!   corner radius [`RADIUS_EM`], fill black at 60%.
//! * **Position.** Horizontally centered in the area left by the side insets, with its bottom edge
//!   at the bottom inset. [`CaptionLayer::for_output`] picks insets from the aspect ratio: 16:9
//!   leaves 6% at the bottom, 1:1 12%, and 9:16 leaves the bottom 20% and the right 12% clear
//!   (the TikTok UI, EXP-2). Lines wrap inside the area; a caption never leaves it.
//! * **Camera bubble.** The bubble is never hidden by a caption. If the box would cover it, the
//!   box keeps its height and slides into the wider free side of the bubble when it fits there;
//!   when it fits on neither side it moves up to sit just above the bubble (never above the top
//!   inset).
//!
//! The layer is blended on the CPU after the GPU frame is read back: it touches a small
//! rectangle, keeps the shaders unchanged and makes the result deterministic.

use cm_core::CmError;
use image::RgbaImage;

use crate::ops::Color;
use crate::text::{self, Fonts, TextStyle};

/// Default font size as a fraction of the output's shorter side.
pub const CAPTION_SIZE_FRAC: f32 = 0.045;
/// Smallest font size in px.
pub const MIN_CAPTION_PX: f32 = 14.0;
/// Box opacity: 60% of 255.
pub const BOX_ALPHA: u8 = 153;
/// Horizontal and vertical box padding as a fraction of the font size.
pub const PAD_X_EM: f32 = 0.55;
pub const PAD_Y_EM: f32 = 0.30;
/// Box corner radius as a fraction of the font size.
pub const RADIUS_EM: f32 = 0.32;
/// How much the glyphs are thickened (per side) to read as weight 600, as a fraction of the font
/// size; at least one pixel from 28 px up.
pub const EMBOLDEN_EM: f32 = 0.018;
/// Gap above a bubble the caption moves to, as a fraction of the output height.
const AVOID_GAP_FRAC: f32 = 0.012;

/// The caption to draw now.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CaptionLayer<'a> {
    /// Plain text; `\n` starts a new row. Empty text draws nothing.
    pub text: &'a str,
    /// Font size as a fraction of the output's shorter side.
    pub size: f32,
    /// Free margins as `[left, top, right, bottom]`: left and right as fractions of the width,
    /// top and bottom as fractions of the height. The caption stays inside what is left.
    pub insets: [f32; 4],
}

impl<'a> CaptionLayer<'a> {
    /// Landscape defaults (16:9 and wider).
    pub fn new(text: &'a str) -> Self {
        CaptionLayer {
            text,
            size: CAPTION_SIZE_FRAC,
            insets: safe_insets((16, 9)),
        }
    }

    /// The insets that suit an output of this size (16:9, 1:1, 9:16).
    pub fn for_output(text: &'a str, output: (u32, u32)) -> Self {
        CaptionLayer {
            text,
            size: CAPTION_SIZE_FRAC,
            insets: safe_insets(output),
        }
    }
}

/// `[left, top, right, bottom]` free margins for an output of this size (see the module docs).
pub fn safe_insets(output: (u32, u32)) -> [f32; 4] {
    let (w, h) = (output.0.max(1) as f32, output.1.max(1) as f32);
    let aspect = w / h;
    if aspect < 0.9 {
        [0.04, 0.0, 0.12, 0.20]
    } else if aspect < 1.2 {
        [0.05, 0.0, 0.05, 0.12]
    } else {
        [0.05, 0.0, 0.05, 0.06]
    }
}

/// The font size in px for this layer on this output.
pub fn caption_px(layer: &CaptionLayer<'_>, output: (u32, u32)) -> f32 {
    (layer.size * output.0.min(output.1) as f32).max(MIN_CAPTION_PX)
}

/// The area a caption may use: its left and right edges and the lowest it may reach. The frame's
/// safe insets set it; when the screen picture is a padded, nearly full-width tile (a background
/// with padding), the caption is anchored inside that picture instead (a little in from its
/// edges, the bottom margin measured from the picture's bottom), so it never hangs off the
/// screen onto the background. A narrow tile (a 16:9 screen letterboxed in a 9:16 frame) keeps
/// the frame's safe area.
fn area(
    layer: &CaptionLayer<'_>,
    output: (u32, u32),
    content: Option<[f32; 4]>,
) -> (f32, f32, f32) {
    let (w, h) = (output.0 as f32, output.1 as f32);
    let mut x0 = (w * layer.insets[0]).ceil();
    let mut x1 = (w * (1.0 - layer.insets[2])).floor();
    let mut bottom = h * (1.0 - layer.insets[3]);
    if let Some(c) = content.filter(|c| c[2] >= 0.8 * w && c[3] >= 0.5 * h) {
        let side = 0.02 * c[2];
        x0 = x0.max((c[0] + side).ceil());
        x1 = x1.min((c[0] + c[2] - side).floor());
        bottom = bottom.min(c[1] + c[3] - layer.insets[3].max(0.04) * c[3]);
    }
    (x0, x1.max(x0 + 1.0), bottom)
}

/// The widest the text may be, in px (the usable width minus the box padding). `content` is the
/// screen picture's rectangle `[x, y, w, h]` when there is one (see [`place`]).
pub fn max_text_width(
    layer: &CaptionLayer<'_>,
    output: (u32, u32),
    content: Option<[f32; 4]>,
) -> f32 {
    let px = caption_px(layer, output);
    let (x0, x1, _) = area(layer, output, content);
    ((x1 - x0) - 2.0 * PAD_X_EM * px).max(2.0 * px)
}

/// Where the box goes: `[x, y, w, h]` in whole output pixels. `text_w` and `text_h` are the
/// laid-out text size; `avoid` is a rectangle (the camera bubble) the box must not cover;
/// `content` is the screen picture's rectangle, which the caption stays inside when it is a
/// padded, nearly full-width tile.
pub fn place(
    layer: &CaptionLayer<'_>,
    output: (u32, u32),
    text_w: f32,
    text_h: f32,
    avoid: Option<[f32; 4]>,
    content: Option<[f32; 4]>,
) -> [f32; 4] {
    let h = output.1 as f32;
    let px = caption_px(layer, output);
    let (x0, x1, bottom_limit) = area(layer, output, content);
    // Never wider than the usable area, whole pixels, so the box cannot cross its edge.
    let box_w = (text_w + 2.0 * PAD_X_EM * px).ceil().min(x1 - x0);
    let box_h = (text_h + 2.0 * PAD_Y_EM * px).ceil();
    let center_in = |from: f32, to: f32| {
        (((from + to) / 2.0 - box_w / 2.0).round()).clamp(from, (to - box_w).max(from))
    };
    let mut x = center_in(x0, x1);
    let mut bottom = bottom_limit;
    if let Some(a) = avoid {
        let overlaps_x = x < a[0] + a[2] && x + box_w > a[0];
        let overlaps_y = bottom > a[1] && bottom - box_h < a[1] + a[3];
        if overlaps_x && overlaps_y {
            // Stay at the bottom and slide into the wider free side of the bubble when the box
            // fits there; otherwise sit just above the bubble.
            let gap = (AVOID_GAP_FRAC * h).ceil();
            let left = (x0, (a[0] - gap).floor());
            let right = ((a[0] + a[2] + gap).ceil(), x1);
            let side = if left.1 - left.0 >= right.1 - right.0 {
                left
            } else {
                right
            };
            if side.1 - side.0 >= box_w {
                x = center_in(side.0, side.1);
            } else {
                bottom = a[1] - gap;
            }
        }
    }
    let top = (bottom - box_h).max(h * layer.insets[1]);
    [x, top.round(), box_w, box_h]
}

/// Thickens white premultiplied text by `r` px (a max filter over a disc on the alpha).
fn embolden(pix: &mut tiny_skia::Pixmap, r: i32) {
    if r < 1 {
        return;
    }
    let (w, h) = (pix.width() as i32, pix.height() as i32);
    let alpha: Vec<u8> = pix.data().as_chunks::<4>().0.iter().map(|p| p[3]).collect();
    let data = pix.data_mut();
    for y in 0..h {
        for x in 0..w {
            let mut m = 0u8;
            for dy in -r..=r {
                for dx in -r..=r {
                    if dx * dx + dy * dy > r * r {
                        continue;
                    }
                    let (sx, sy) = (x + dx, y + dy);
                    if sx >= 0 && sy >= 0 && sx < w && sy < h {
                        m = m.max(alpha[(sy * w + sx) as usize]);
                    }
                }
            }
            let o = ((y * w + x) * 4) as usize;
            data[o..o + 4].copy_from_slice(&[m, m, m, m]);
        }
    }
}

/// Coverage (0..=1) of pixel center `(px, py)` by a rounded rectangle, anti-aliased by 1 px.
fn rounded_coverage(px: f32, py: f32, rect: [f32; 4], radius: f32) -> f32 {
    let (cx, cy) = (rect[0] + rect[2] / 2.0, rect[1] + rect[3] / 2.0);
    let (hx, hy) = (rect[2] / 2.0 - radius, rect[3] / 2.0 - radius);
    let (dx, dy) = ((px - cx).abs() - hx, (py - cy).abs() - hy);
    let outside = (dx.max(0.0).powi(2) + dy.max(0.0).powi(2)).sqrt();
    let inside = dx.max(dy).min(0.0);
    let sd = outside + inside - radius;
    (0.5 - sd).clamp(0.0, 1.0)
}

/// Draws the caption over `img`. `avoid` is the bubble rectangle and `content` the screen
/// picture's rectangle, if any.
pub(crate) fn blend(
    img: &mut RgbaImage,
    layer: &CaptionLayer<'_>,
    avoid: Option<[f32; 4]>,
    content: Option<[f32; 4]>,
    fonts: &mut Fonts,
) -> Result<(), CmError> {
    let text = layer.text.trim();
    if text.is_empty() {
        return Ok(());
    }
    let output = (img.width(), img.height());
    let px = caption_px(layer, output);
    let style = TextStyle {
        px,
        color: Color::WHITE,
        outline: None,
        bg: None,
        wrap_w: Some(max_text_width(layer, output, content)),
        center: true,
    };
    let Some(mut tl) = text::layout(fonts, text, &style)? else {
        return Ok(());
    };
    let r = if px >= 28.0 {
        ((px * EMBOLDEN_EM).round() as i32).clamp(1, tl.pad)
    } else {
        0
    };
    embolden(&mut tl.pixmap, r);
    let text_h = (tl.pixmap.height() as i32 - 2 * tl.pad) as f32;
    let rect = place(layer, output, tl.text_w.ceil(), text_h, avoid, content);
    let radius = (RADIUS_EM * px).min(rect[3] / 2.0);

    let (w, h) = (img.width() as i32, img.height() as i32);
    let x_from = (rect[0] as i32).max(0);
    let x_to = ((rect[0] + rect[2]) as i32).min(w);
    let y_from = (rect[1] as i32).max(0);
    let y_to = ((rect[1] + rect[3]) as i32).min(h);
    for y in y_from..y_to {
        for x in x_from..x_to {
            let cov = rounded_coverage(x as f32 + 0.5, y as f32 + 0.5, rect, radius);
            if cov <= 0.0 {
                continue;
            }
            let a = (f32::from(BOX_ALPHA) * cov).round() as u32;
            let p = img.get_pixel_mut(x as u32, y as u32);
            for c in 0..3 {
                p.0[c] = ((u32::from(p.0[c]) * (255 - a) + 127) / 255) as u8;
            }
        }
    }
    // Text on top: its layer is premultiplied white; the layer carries `pad` px of margin on every
    // side, so its origin sits `pad` px up and left of the first glyph.
    let tx = rect[0] as i32 + ((rect[2] - tl.text_w.ceil()) / 2.0).round() as i32 - tl.pad;
    let ty = rect[1] as i32 + (PAD_Y_EM * px).round() as i32 - tl.pad;
    let (tw, th) = (tl.pixmap.width() as i32, tl.pixmap.height() as i32);
    let src = tl.pixmap.data();
    for sy in 0..th {
        for sx in 0..tw {
            let (x, y) = (tx + sx, ty + sy);
            if x < 0 || y < 0 || x >= w || y >= h {
                continue;
            }
            let s = &src[((sy * tw + sx) * 4) as usize..][..4];
            if s[3] == 0 {
                continue;
            }
            let inv = 255 - u32::from(s[3]);
            let p = img.get_pixel_mut(x as u32, y as u32);
            for (d, &sv) in p.0.iter_mut().zip(s).take(3) {
                *d = (u32::from(sv) + (u32::from(*d) * inv + 127) / 255).min(255) as u8;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// AI-1.AC5: on a 9:16 output the caption box stays out of the bottom 20% and the right 12%.
    #[test]
    fn ai1_ac5_vertical_box_stays_in_the_safe_area() {
        let out = (1080, 1920);
        let layer = CaptionLayer::for_output("A long caption line that wraps", out);
        let max_w = max_text_width(&layer, out, None);
        let r = place(&layer, out, max_w, 200.0, None, None);
        assert!(r[1] + r[3] <= 1920.0 * 0.80 + 0.5, "bottom {:?}", r);
        assert!(r[0] + r[2] <= 1080.0 * 0.88 + 0.5, "right {:?}", r);
        assert!(r[0] >= 1080.0 * 0.04 - 0.5, "left {:?}", r);
    }

    #[test]
    fn landscape_box_is_centered_with_a_small_bottom_margin() {
        let out = (1920, 1080);
        let layer = CaptionLayer::for_output("Hello", out);
        let r = place(&layer, out, 300.0, 58.0, None, None);
        assert!(((r[0] + r[2] / 2.0) - 960.0).abs() <= 1.0, "centered {r:?}");
        assert!((r[1] + r[3] - 1080.0 * 0.94).abs() <= 1.0, "bottom {r:?}");
    }

    #[test]
    fn size_scales_with_height_and_has_a_floor() {
        let l = CaptionLayer::new("x");
        assert!((caption_px(&l, (1920, 1080)) - 48.6).abs() < 0.01);
        assert!((caption_px(&l, (3840, 2160)) - 97.2).abs() < 0.01);
        assert_eq!(caption_px(&l, (320, 180)), MIN_CAPTION_PX);
    }

    /// The box moves above a bubble it would cover and stays put when they do not meet.
    #[test]
    fn the_box_moves_above_a_bubble_it_would_cover() {
        let out = (1920, 1080);
        let layer = CaptionLayer::for_output("Hello", out);
        let bubble = [20.0, 800.0, 238.0, 238.0];
        let r = place(&layer, out, 1700.0, 58.0, Some(bubble), None);
        assert!(r[1] + r[3] <= bubble[1], "above the bubble: {r:?}");
        let far = [1700.0, 800.0, 200.0, 200.0];
        let r2 = place(&layer, out, 300.0, 58.0, Some(far), None);
        assert!(
            (r2[1] + r2[3] - 1080.0 * 0.94).abs() <= 1.0,
            "unmoved: {r2:?}"
        );
    }

    /// A box that fits beside the bubble slides sideways and stays at the bottom, clear of it.
    #[test]
    fn the_box_slides_beside_a_bubble_when_it_fits() {
        let out = (1920, 1080);
        let layer = CaptionLayer::for_output("Hello", out);
        let bubble = [1400.0, 600.0, 480.0, 440.0];
        // Centered it would reach x 810..1110: free of the bubble: unmoved. Wide enough to meet it:
        let wide = place(&layer, out, 1100.0, 58.0, Some(bubble), None);
        assert!(
            wide[0] + wide[2] <= bubble[0],
            "left of the bubble: {wide:?}"
        );
        assert!(
            (wide[1] + wide[3] - 1080.0 * 0.94).abs() <= 1.0,
            "still at the bottom: {wide:?}"
        );
    }

    /// With a background and padding the caption stays inside the screen picture, not on the
    /// background around it; a narrow (letterboxed) picture keeps the frame's safe area.
    #[test]
    fn the_box_stays_inside_a_padded_screen_picture() {
        let out = (1920, 1080);
        let layer = CaptionLayer::for_output("Hello", out);
        // 8% padding: the picture is [154, 86, 1612, 907] (16:9 inside the padded area).
        let tile = [154.0, 86.0, 1612.0, 907.0];
        let r = place(&layer, out, 600.0, 58.0, None, Some(tile));
        assert!(
            r[1] + r[3] <= tile[1] + tile[3],
            "above the picture's bottom: {r:?}"
        );
        assert!(
            r[1] + r[3] <= tile[1] + tile[3] - 0.04 * tile[3] + 1.0,
            "a margin above it: {r:?}"
        );
        // The text wraps inside the picture, not the frame.
        let w = max_text_width(&layer, out, Some(tile));
        assert!(
            w < max_text_width(&layer, out, None),
            "narrower than the frame's: {w}"
        );
        let wide = place(&layer, out, w, 58.0, None, Some(tile));
        assert!(
            wide[0] >= tile[0] && wide[0] + wide[2] <= tile[0] + tile[2],
            "{wide:?}"
        );
        // A 16:9 picture in a 9:16 frame is narrow: the frame's safe area applies.
        let vertical = (1080, 1920);
        let v = CaptionLayer::for_output("Hello", vertical);
        let strip = [0.0, 656.0, 1080.0, 608.0];
        let r = place(&v, vertical, 500.0, 90.0, None, Some(strip));
        assert!(r[1] + r[3] <= 1920.0 * 0.80 + 0.5, "{r:?}");
        assert!(
            r[1] + r[3] > strip[1] + strip[3],
            "below the picture, in the safe area: {r:?}"
        );
    }

    #[test]
    fn rounded_coverage_cuts_the_corner() {
        let rect = [10.0, 10.0, 100.0, 40.0];
        assert!(rounded_coverage(60.0, 30.0, rect, 12.0) > 0.99);
        assert!(rounded_coverage(10.5, 10.5, rect, 12.0) < 0.05);
        assert!(rounded_coverage(5.0, 30.0, rect, 12.0) < 0.01);
    }
}
