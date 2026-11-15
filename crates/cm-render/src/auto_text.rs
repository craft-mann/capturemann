//! The Auto default colour of a new Text annotation (ANN-1.AC8, design request D10).
//!
//! A new Text annotation without a colour is stored with `#111111` or `#FFFFFF`, whichever has
//! the higher contrast with the picture under its box, plus a 2 px outline in the other colour
//! when the picture is busy. The decision is made once, when the text is placed (by the editor)
//! or created (by `annotate`), and the result is stored in the op: rendering never decides a
//! colour. The function is pure, so the same picture and position always give the same answer.

use cm_core::annotation::Color;
use image::RgbaImage;

/// The dark text colour (`Dark` in the panel).
pub const AUTO_DARK: Color = Color::rgb(0x11, 0x11, 0x11);
/// The light text colour (`Light` in the panel).
pub const AUTO_LIGHT: Color = Color::WHITE;
/// Mean relative luminance above which dark text is chosen (the point where `#111111` and
/// `#FFFFFF` have the same contrast, 0.1916, rounded as the design states it).
pub const SWITCH_LUMINANCE: f64 = 0.19;
/// The share of sampled pixels that must reach [`MIN_CONTRAST`] before no outline is added.
pub const READABLE_SHARE: f64 = 0.9;
/// WCAG contrast ratio of normal text.
pub const MIN_CONTRAST: f64 = 4.5;
/// Width in px (physical, of the original) of the default text box.
pub const BOX_WIDTH: f64 = 240.0;
/// Height of the default box as a multiple of the text size.
pub const BOX_HEIGHT_EM: f64 = 1.25;
/// Pixels sampled around the default box.
pub const MARGIN: f64 = 8.0;
/// Width in px of the outline Auto adds on a busy picture.
pub const OUTLINE_WIDTH: f64 = 2.0;

/// What Auto chose for a new text.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AutoText {
    /// `#111111` or `#FFFFFF`.
    pub color: Color,
    /// The other of the two colours when the picture is busy (2 px, [`OUTLINE_WIDTH`]); else `None`.
    pub outline: Option<Color>,
    /// Mean relative luminance (0 to 1) of the sampled picture.
    pub luminance: f64,
    /// Fewer than 90 % of the sampled pixels reach 4.5:1 against `color`.
    pub busy: bool,
}

fn lin(c: u8) -> f64 {
    let c = f64::from(c) / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// WCAG relative luminance of an sRGB colour.
pub fn relative_luminance(c: Color) -> f64 {
    0.2126 * lin(c.r) + 0.7152 * lin(c.g) + 0.0722 * lin(c.b)
}

/// WCAG contrast ratio of two relative luminances.
pub fn contrast(a: f64, b: f64) -> f64 {
    let (hi, lo) = if a >= b { (a, b) } else { (b, a) };
    (hi + 0.05) / (lo + 0.05)
}

/// Chooses the colour of a new text of `size` px placed with its top-left at `at` (physical px
/// of `base`, the picture with the base edits applied and no annotations).
///
/// Samples the default box (`size` x 1.25 high, 240 px wide) plus 8 px around it, clamped to the
/// image. A pixel with alpha is laid over white. An empty sample (a click outside the image)
/// counts as a light picture. Deterministic.
pub fn auto_text_style(base: &RgbaImage, at: (f64, f64), size: f64) -> AutoText {
    let (w, h) = (base.width() as i64, base.height() as i64);
    let size = if size.is_finite() && size > 0.0 {
        size
    } else {
        24.0
    };
    let (ax, ay) = if at.0.is_finite() && at.1.is_finite() {
        at
    } else {
        (0.0, 0.0)
    };
    let x0 = ((ax - MARGIN).floor() as i64).clamp(0, w);
    let y0 = ((ay - MARGIN).floor() as i64).clamp(0, h);
    let x1 = ((ax + BOX_WIDTH + MARGIN).ceil() as i64).clamp(0, w);
    let y1 = ((ay + size * BOX_HEIGHT_EM + MARGIN).ceil() as i64).clamp(0, h);
    let mut lums: Vec<f64> = Vec::new();
    for y in y0..y1 {
        for x in x0..x1 {
            let p = base.get_pixel(x as u32, y as u32).0;
            let a = f64::from(p[3]) / 255.0;
            let over = |c: u8| (f64::from(c) * a + 255.0 * (1.0 - a)).round() as u8;
            lums.push(relative_luminance(Color::rgb(
                over(p[0]),
                over(p[1]),
                over(p[2]),
            )));
        }
    }
    let luminance = if lums.is_empty() {
        1.0
    } else {
        lums.iter().sum::<f64>() / lums.len() as f64
    };
    let dark = luminance >= SWITCH_LUMINANCE;
    let (color, other) = if dark {
        (AUTO_DARK, AUTO_LIGHT)
    } else {
        (AUTO_LIGHT, AUTO_DARK)
    };
    let lc = relative_luminance(color);
    let busy = !lums.is_empty() && {
        let ok = lums
            .iter()
            .filter(|l| contrast(**l, lc) >= MIN_CONTRAST)
            .count();
        (ok as f64) < READABLE_SHARE * lums.len() as f64
    };
    AutoText {
        color,
        outline: busy.then_some(other),
        luminance,
        busy,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn solid(w: u32, h: u32, c: [u8; 4]) -> RgbaImage {
        RgbaImage::from_pixel(w, h, Rgba(c))
    }

    #[test]
    fn light_picture_gets_dark_text_without_outline() {
        let r = auto_text_style(&solid(600, 400, [250, 250, 250, 255]), (50.0, 50.0), 24.0);
        assert_eq!(r.color, AUTO_DARK);
        assert_eq!(r.outline, None);
        assert!(!r.busy);
        assert!(r.luminance > 0.9);
    }

    #[test]
    fn dark_picture_gets_white_text_without_outline() {
        let r = auto_text_style(
            &solid(600, 400, [0x1E, 0x1E, 0x1E, 255]),
            (50.0, 50.0),
            24.0,
        );
        assert_eq!(r.color, AUTO_LIGHT);
        assert_eq!(r.outline, None);
        assert!(!r.busy);
    }

    #[test]
    fn busy_picture_adds_an_outline_in_the_other_colour() {
        // Alternating light and dark columns: the mean is mid grey, neither colour reaches 4.5:1
        // on enough pixels.
        let mut img = solid(600, 400, [255, 255, 255, 255]);
        for (x, _, p) in img.enumerate_pixels_mut() {
            if x % 2 == 0 {
                *p = Rgba([0, 0, 0, 255]);
            }
        }
        let r = auto_text_style(&img, (50.0, 50.0), 24.0);
        assert!(r.busy);
        let o = r.outline.expect("outline");
        assert_ne!(o, r.color);
        assert!(o == AUTO_DARK || o == AUTO_LIGHT);
        assert_eq!(OUTLINE_WIDTH, 2.0);
    }

    #[test]
    fn switch_point_is_a_mean_luminance_of_0_19() {
        // Grey levels either side of the switch point (luminance 0.19 is about sRGB 120).
        let below = auto_text_style(&solid(300, 100, [118, 118, 118, 255]), (0.0, 0.0), 24.0);
        let above = auto_text_style(&solid(300, 100, [122, 122, 122, 255]), (0.0, 0.0), 24.0);
        assert!(below.luminance < SWITCH_LUMINANCE && above.luminance > SWITCH_LUMINANCE);
        assert_eq!(below.color, AUTO_LIGHT);
        assert_eq!(above.color, AUTO_DARK);
        // #111111 and #FFFFFF contrast equally at the switch point.
        let l = relative_luminance(AUTO_DARK);
        assert!((contrast(0.1916, l) - contrast(0.1916, 1.0)).abs() < 0.01);
    }

    #[test]
    fn is_deterministic() {
        let mut img = solid(400, 300, [200, 30, 60, 255]);
        for (x, y, p) in img.enumerate_pixels_mut() {
            p.0[1] = (x * 7 + y * 3) as u8;
        }
        let a = auto_text_style(&img, (20.5, 33.25), 32.0);
        assert_eq!(a, auto_text_style(&img, (20.5, 33.25), 32.0));
    }

    #[test]
    fn transparent_pixels_count_as_white() {
        let r = auto_text_style(&solid(300, 100, [0, 0, 0, 0]), (0.0, 0.0), 24.0);
        assert_eq!(r.color, AUTO_DARK);
        let half = auto_text_style(&solid(300, 100, [0, 0, 0, 255]), (0.0, 0.0), 24.0);
        assert_eq!(half.color, AUTO_LIGHT);
    }

    #[test]
    fn only_the_box_and_its_margin_are_sampled_and_edges_clamp() {
        // Left half dark, right half light; a box at the left edge sees only dark.
        let mut img = solid(1000, 200, [255, 255, 255, 255]);
        for (x, _, p) in img.enumerate_pixels_mut() {
            if x < 400 {
                *p = Rgba([0, 0, 0, 255]);
            }
        }
        assert_eq!(auto_text_style(&img, (10.0, 10.0), 24.0).color, AUTO_LIGHT);
        assert_eq!(auto_text_style(&img, (600.0, 10.0), 24.0).color, AUTO_DARK);
        // Beyond the right and bottom edges, negative origin, and outside: no panic.
        assert_eq!(auto_text_style(&img, (990.0, 195.0), 24.0).color, AUTO_DARK);
        let _ = auto_text_style(&img, (-500.0, -500.0), 24.0);
        let _ = auto_text_style(&img, (5000.0, 5000.0), 24.0);
        // 8 px around: a dark band 7 px left of the box counts, one 12 px away does not.
        assert_eq!(auto_text_style(&img, (407.0, 10.0), 24.0).color, AUTO_DARK);
    }
}
