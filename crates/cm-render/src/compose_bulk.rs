//! Bulk export (BLK-1) pixel steps: the size rule and the watermark. Both work on the rendered
//! picture only; nothing here knows a capture, an op or a file.

use cm_core::CmError;
use image::{imageops::FilterType, RgbaImage};
use serde::{Deserialize, Serialize};
use tiny_skia::{Pixmap, PixmapPaint, Transform};

use crate::ops::Color;
use crate::text::{self, Fonts, TextStyle};

/// How the exported size is chosen. Percentages are of the original size.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SizeRule {
    /// A width in px (the height follows).
    Width(u32),
    /// A height in px (the width follows).
    Height(u32),
    /// The longest side in px.
    Longest(u32),
    /// A percentage, 1 to 400.
    Percent(f64),
}

/// The size an image of `(w, h)` is exported at. Never larger than the original unless
/// `allow_larger`. Pure; at least 1 px on each side.
pub fn target_size(w: u32, h: u32, rule: &SizeRule, allow_larger: bool) -> (u32, u32) {
    let f = match *rule {
        SizeRule::Width(x) => f64::from(x) / f64::from(w.max(1)),
        SizeRule::Height(x) => f64::from(x) / f64::from(h.max(1)),
        SizeRule::Longest(x) => f64::from(x) / f64::from(w.max(h).max(1)),
        SizeRule::Percent(p) => p / 100.0,
    };
    let f = if allow_larger { f } else { f.min(1.0) };
    (
        ((f64::from(w) * f).round() as u32).max(1),
        ((f64::from(h) * f).round() as u32).max(1),
    )
}

/// Checks a rule's value.
pub fn validate_rule(rule: &SizeRule) -> Result<(), CmError> {
    let ok = match *rule {
        SizeRule::Width(x) | SizeRule::Height(x) | SizeRule::Longest(x) => {
            (1..=16_384).contains(&x)
        }
        SizeRule::Percent(p) => (1.0..=400.0).contains(&p),
    };
    if ok {
        Ok(())
    } else {
        Err(CmError::invalid_field(
            "size",
            "a side must be 1 to 16384 px and a percentage 1 to 400",
        ))
    }
}

/// Resizes to exactly `(w, h)` (Lanczos3); the same picture when it already has that size.
pub fn resize_to(img: &RgbaImage, w: u32, h: u32) -> RgbaImage {
    if img.dimensions() == (w, h) {
        img.clone()
    } else {
        image::imageops::resize(img, w, h, FilterType::Lanczos3)
    }
}

/// Where a watermark sits (a corner, an edge middle or the centre).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Position {
    TopLeft,
    Top,
    TopRight,
    Left,
    Center,
    Right,
    BottomLeft,
    Bottom,
    #[default]
    BottomRight,
}

/// What is drawn.
#[derive(Debug, Clone)]
pub enum Mark {
    Text(String),
    Image(RgbaImage),
}

/// A watermark drawn on the exported picture (BLK-1; never stored as an op).
#[derive(Debug, Clone)]
pub struct Watermark {
    pub mark: Mark,
    pub position: Position,
    /// 0.05 to 1.0.
    pub opacity: f32,
    /// The mark's width as a fraction of the picture's width, 0.05 to 1.0.
    pub size: f32,
}

/// Draws the watermark onto `img`.
pub fn apply_watermark(img: &mut RgbaImage, wm: &Watermark) -> Result<(), CmError> {
    let (w, h) = img.dimensions();
    let opacity = wm.opacity.clamp(0.05, 1.0);
    let want_w = (w as f32 * wm.size.clamp(0.05, 1.0)).max(8.0);
    let layer: Pixmap = match &wm.mark {
        Mark::Text(t) => {
            if t.trim().is_empty() {
                return Ok(());
            }
            let mut fonts = Fonts::bundled();
            let mut style = TextStyle {
                px: 48.0,
                color: Color::WHITE,
                outline: Some(Color::BLACK),
                bg: None,
                wrap_w: None,
                center: false,
            };
            let first = text::layout(&mut fonts, t, &style)?;
            let Some(first) = first else { return Ok(()) };
            // Scale the size so the text is `want_w` wide; wrapping is not used.
            let px = (48.0 * want_w / first.text_w.max(1.0)).clamp(8.0, 600.0);
            style.px = px;
            match text::layout(&mut fonts, t, &style)? {
                Some(l) => l.pixmap,
                None => return Ok(()),
            }
        }
        Mark::Image(src) => {
            let mut tw = want_w.round() as u32;
            let mut th = ((src.height() as f32) * tw as f32 / src.width().max(1) as f32)
                .round()
                .max(1.0) as u32;
            if th > h {
                tw = ((tw as f32) * h as f32 / th as f32).round().max(1.0) as u32;
                th = h;
            }
            let resized = resize_to(src, tw.max(1), th.max(1));
            let mut pm = Pixmap::new(resized.width(), resized.height())
                .ok_or_else(|| CmError::invalid_input("the watermark picture is too large"))?;
            for (d, s) in pm
                .data_mut()
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .zip(resized.as_raw().as_chunks::<4>().0.iter())
            {
                let a = u32::from(s[3]);
                for c in 0..3 {
                    d[c] = ((u32::from(s[c]) * a + 127) / 255) as u8;
                }
                d[3] = s[3];
            }
            pm
        }
    };
    let (lw, lh) = (layer.width() as i32, layer.height() as i32);
    let m = ((w.min(h) as f32) * 0.02).round() as i32 + 4;
    let (iw, ih) = (w as i32, h as i32);
    let x = match wm.position {
        Position::TopLeft | Position::Left | Position::BottomLeft => m,
        Position::Top | Position::Center | Position::Bottom => (iw - lw) / 2,
        _ => iw - lw - m,
    };
    let y = match wm.position {
        Position::TopLeft | Position::Top | Position::TopRight => m,
        Position::Left | Position::Center | Position::Right => (ih - lh) / 2,
        _ => ih - lh - m,
    };
    // Draw on a premultiplied copy of the picture and convert back.
    let mut pm = Pixmap::new(w, h)
        .ok_or_else(|| CmError::invalid_input("the picture is too large to watermark"))?;
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
    pm.draw_pixmap(
        x,
        y,
        layer.as_ref(),
        &PixmapPaint {
            opacity,
            ..PixmapPaint::default()
        },
        Transform::identity(),
        None,
    );
    for (d, p) in img.pixels_mut().zip(pm.pixels()) {
        let c = p.demultiply();
        d.0 = [c.red(), c.green(), c.blue(), c.alpha()];
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_never_grow_unless_allowed() {
        assert_eq!(
            target_size(3200, 1600, &SizeRule::Longest(1600), false),
            (1600, 800)
        );
        assert_eq!(
            target_size(800, 400, &SizeRule::Longest(1600), false),
            (800, 400)
        );
        assert_eq!(
            target_size(800, 400, &SizeRule::Longest(1600), true),
            (1600, 800)
        );
        assert_eq!(
            target_size(1000, 500, &SizeRule::Width(250), false),
            (250, 125)
        );
        assert_eq!(
            target_size(1000, 500, &SizeRule::Height(100), false),
            (200, 100)
        );
        assert_eq!(
            target_size(1000, 500, &SizeRule::Percent(50.0), false),
            (500, 250)
        );
        assert!(validate_rule(&SizeRule::Percent(0.0)).is_err());
        assert!(validate_rule(&SizeRule::Width(0)).is_err());
    }

    #[test]
    fn a_text_watermark_changes_pixels_only_near_its_corner() {
        let base = RgbaImage::from_pixel(400, 300, image::Rgba([20, 40, 60, 255]));
        let mut img = base.clone();
        apply_watermark(
            &mut img,
            &Watermark {
                mark: Mark::Text("DRAFT".into()),
                position: Position::BottomRight,
                opacity: 0.8,
                size: 0.3,
            },
        )
        .unwrap();
        assert_ne!(img.as_raw(), base.as_raw());
        assert_eq!(img.get_pixel(5, 5), base.get_pixel(5, 5));
        let changed_bottom_right = (200..400)
            .flat_map(|x| (200..300).map(move |y| (x, y)))
            .any(|(x, y)| img.get_pixel(x, y) != base.get_pixel(x, y));
        assert!(changed_bottom_right);
    }

    #[test]
    fn an_image_watermark_is_placed_by_position() {
        let base = RgbaImage::from_pixel(200, 200, image::Rgba([0, 0, 0, 255]));
        let mark = RgbaImage::from_pixel(10, 10, image::Rgba([255, 255, 255, 255]));
        let mut img = base.clone();
        apply_watermark(
            &mut img,
            &Watermark {
                mark: Mark::Image(mark),
                position: Position::Center,
                opacity: 1.0,
                size: 0.25,
            },
        )
        .unwrap();
        assert_eq!(img.get_pixel(100, 100).0, [255, 255, 255, 255]);
        assert_eq!(img.get_pixel(5, 5).0, [0, 0, 0, 255]);
    }
}
