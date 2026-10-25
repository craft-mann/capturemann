//! Text for the picker's chips and hints (DESIGN §3): Geist for labels and Geist Mono for sizes,
//! coordinates and keycaps (both bundled, SIL OFL 1.1; see `assets/Geist-OFL-LICENSE.txt`), drawn
//! with `ab_glyph`. Digits use one fixed advance (the widest digit) so live numbers never jiggle,
//! standing in for OpenType `tnum` (DESIGN §3) until the shared text engine is used here.

use ab_glyph::{point, Font, FontRef, PxScale, ScaleFont};

use crate::canvas::Canvas;

const GEIST: &[u8] = include_bytes!("../assets/Geist-Regular.ttf");
const GEIST_MONO: &[u8] = include_bytes!("../assets/GeistMono-Regular.ttf");

/// One loaded face.
struct Face {
    font: FontRef<'static>,
}

impl Face {
    fn digit_advance(&self, px: f32) -> f32 {
        let scaled = self.font.as_scaled(PxScale::from(px));
        ('0'..='9')
            .map(|c| scaled.h_advance(self.font.glyph_id(c)))
            .fold(0.0, f32::max)
    }

    fn advance(&self, c: char, px: f32, digit: f32) -> f32 {
        if c.is_ascii_digit() {
            digit
        } else {
            self.font
                .as_scaled(PxScale::from(px))
                .h_advance(self.font.glyph_id(c))
        }
    }

    fn measure(&self, s: &str, px: f32) -> f32 {
        let digit = self.digit_advance(px);
        s.chars().map(|c| self.advance(c, px, digit)).sum()
    }

    fn line_height(&self, px: f32) -> f32 {
        let s = self.font.as_scaled(PxScale::from(px));
        s.ascent() - s.descent()
    }

    fn draw(&self, canvas: &mut Canvas<'_>, x: f32, y: f32, s: &str, px: f32, color: u32) {
        let scaled = self.font.as_scaled(PxScale::from(px));
        let digit = self.digit_advance(px);
        let baseline = y + scaled.ascent();
        let mut pen = x;
        for c in s.chars() {
            let id = self.font.glyph_id(c);
            let adv = self.advance(c, px, digit);
            // Center a digit inside its fixed cell.
            let glyph_adv = scaled.h_advance(id);
            let off = if c.is_ascii_digit() {
                (adv - glyph_adv) / 2.0
            } else {
                0.0
            };
            let glyph = id.with_scale_and_position(PxScale::from(px), point(pen + off, baseline));
            if let Some(og) = self.font.outline_glyph(glyph) {
                let b = og.px_bounds();
                og.draw(|gx, gy, cov| {
                    let a = (cov.clamp(0.0, 1.0) * 255.0).round() as u32;
                    if a > 0 {
                        let (px_x, px_y) = (b.min.x as i32 + gx as i32, b.min.y as i32 + gy as i32);
                        canvas.put(px_x, px_y, color, a);
                    }
                });
            }
            pen += adv;
        }
    }
}

/// The picker's fonts. Creating one parses the font tables, so keep it around.
pub struct TextPainter {
    ui: Face,
    mono: Face,
}

impl TextPainter {
    pub fn new() -> Option<TextPainter> {
        Some(TextPainter {
            ui: Face {
                font: FontRef::try_from_slice(GEIST).ok()?,
            },
            mono: Face {
                font: FontRef::try_from_slice(GEIST_MONO).ok()?,
            },
        })
    }

    /// Width of `s` at `px` size in Geist, in px.
    pub fn measure(&self, s: &str, px: f32) -> f32 {
        self.ui.measure(s, px)
    }

    /// Width of `s` at `px` size in Geist Mono, in px.
    pub fn measure_mono(&self, s: &str, px: f32) -> f32 {
        self.mono.measure(s, px)
    }

    /// Height from ascent to descent at `px` size (Geist).
    pub fn line_height(&self, px: f32) -> f32 {
        self.ui.line_height(px)
    }

    /// Height from ascent to descent at `px` size (Geist Mono).
    pub fn line_height_mono(&self, px: f32) -> f32 {
        self.mono.line_height(px)
    }

    /// Draws `s` in Geist with its left edge at `x` and its text box top at `y` (ascent-aligned).
    pub fn draw(&self, canvas: &mut Canvas<'_>, x: f32, y: f32, s: &str, px: f32, color: u32) {
        self.ui.draw(canvas, x, y, s, px, color);
    }

    /// [`TextPainter::draw`] in Geist Mono.
    pub fn draw_mono(&self, canvas: &mut Canvas<'_>, x: f32, y: f32, s: &str, px: f32, color: u32) {
        self.mono.draw(canvas, x, y, s, px, color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bundled_font_loads() {
        assert!(TextPainter::new().is_some());
    }

    /// DESIGN §3: numbers never jiggle: every digit string of the same length is as wide.
    #[test]
    fn digits_are_tabular() {
        let t = TextPainter::new().unwrap();
        let a = t.measure("1111", 12.0);
        let b = t.measure("8080", 12.0);
        assert!((a - b).abs() < 0.001, "{a} vs {b}");
        assert!(t.measure("1334 × 972", 12.0) > t.measure("1 × 9", 12.0));
    }

    /// R4-14: the chip text is Geist Mono: every glyph has the same advance, and the sign used in
    /// the size readout exists in both faces.
    #[test]
    fn r4_14_chips_are_monospaced_geist_mono() {
        let t = TextPainter::new().unwrap();
        let (a, b) = (t.measure_mono("iiii", 12.0), t.measure_mono("WWWW", 12.0));
        assert!((a - b).abs() < 0.5, "{a} vs {b}");
        assert!(t.measure("iiii", 12.0) < t.measure("WWWW", 12.0));
        for f in [&t.ui, &t.mono] {
            assert_ne!(
                f.font.glyph_id('\u{d7}').0,
                0,
                "missing the multiplication sign"
            );
        }
    }

    #[test]
    fn drawing_text_puts_ink_on_the_canvas() {
        let t = TextPainter::new().unwrap();
        let mut buf = vec![0u32; 80 * 24];
        let mut c = Canvas::new(&mut buf, 80, 24).unwrap();
        t.draw(&mut c, 2.0, 2.0, "420 × 649", 12.0, 0xFFFFFF);
        assert!(buf.iter().any(|&p| p != 0));
    }
}
