//! Poster/thumbnail generation for screenshots (`thumbs/poster.jpg`).

use crate::bundle::ShotBundle;
use crate::util::{io_err, write_atomic};
use cm_core::CmError;
use image::imageops::FilterType;
use std::path::PathBuf;

/// File name of the screenshot poster inside `thumbs/`.
pub const POSTER_FILE: &str = "poster.jpg";
/// Default longest edge of the poster in physical px (2x of a 256 px grid card).
pub const DEFAULT_POSTER_EDGE: u32 = 512;

/// A picture more than this many times higher than wide (a Long Shot) gets the top-part poster
/// below instead of a whole-picture poster, which would be a few dozen pixels wide (LIB-1).
pub const TALL_RATIO: f64 = 2.0;
/// The card's shape (width over height, 16:10): the top part of a tall picture is cut to it.
const CARD_ASPECT: f64 = 1.6;
/// The share of the top part, from the bottom, that fades out.
const FADE_SHARE: f64 = 0.25;
/// The strongest the fade gets, at the last row.
const FADE_MAX: f64 = 0.85;

/// How the poster of a tall picture is cut: the top `crop_h` source rows, scaled to
/// `out_w` x `out_h` (never upscaled). `None` for a picture that is not tall.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TallPlan {
    pub crop_h: u32,
    pub out_w: u32,
    pub out_h: u32,
}

pub fn tall_plan(w: u32, h: u32, max_edge: u32) -> Option<TallPlan> {
    if w == 0 || max_edge == 0 || f64::from(h) <= TALL_RATIO * f64::from(w) {
        return None;
    }
    let out_w = w.min(max_edge);
    let out_h = ((f64::from(out_w) / CARD_ASPECT).round() as u32).max(1);
    let crop_h = ((f64::from(out_h) * f64::from(w) / f64::from(out_w)).round() as u32).clamp(1, h);
    Some(TallPlan {
        crop_h,
        out_w,
        out_h,
    })
}

/// Blends the last [`FADE_SHARE`] of the rows toward the mean colour of the picture, up to
/// [`FADE_MAX`] at the last row: the cut reads as "more below", not as a hard edge.
fn fade_out(img: &mut image::RgbImage) {
    let (w, h) = img.dimensions();
    if w == 0 || h < 4 {
        return;
    }
    let mut sum = [0u64; 3];
    for p in img.pixels() {
        for (s, c) in sum.iter_mut().zip(p.0) {
            *s += u64::from(c);
        }
    }
    let n = u64::from(w) * u64::from(h);
    let mean = [sum[0] / n, sum[1] / n, sum[2] / n];
    let start = (f64::from(h) * (1.0 - FADE_SHARE)) as u32;
    for y in start..h {
        let t = f64::from(y - start + 1) / f64::from(h - start) * FADE_MAX;
        for x in 0..w {
            let p = img.get_pixel_mut(x, y);
            for (c, m) in p.0.iter_mut().zip(mean) {
                *c = (f64::from(*c) * (1.0 - t) + m as f64 * t).round() as u8;
            }
        }
    }
}

/// Writes `thumbs/poster.jpg`, scaled to fit `max_edge`, from `render.png` when it is current
/// (so annotations show in the grid) and from `original.png` when the bundle has no ops.
/// Never upscales.
///
/// When the bundle has ops but no current render, the original must not be used: it would show
/// whatever a blur or pixelate is meant to hide (R1-12). An existing poster is then kept, unless
/// the ops include a redaction (or ops this build cannot read), in which case the old poster may
/// show the unredacted pixels and is removed. Both cases return an error, so the caller can
/// report that the thumbnail waits for the render.
pub fn write_poster(bundle: &ShotBundle, max_edge: u32) -> Result<PathBuf, CmError> {
    if max_edge == 0 {
        return Err(CmError::invalid_field("max_edge", "must be positive"));
    }
    let src = if bundle.render_is_stale() {
        let m = bundle.manifest();
        if !m.ops.is_empty() || !m.unknown_ops.is_empty() {
            let poster = bundle.thumbs_dir().join(POSTER_FILE);
            let redacts = !m.unknown_ops.is_empty() || m.ops.iter().any(|o| o.kind.is_redaction());
            if redacts {
                let _ = std::fs::remove_file(&poster);
            }
            return Err(CmError::invalid_input(
                "the annotated render is not ready; the thumbnail is not updated",
            ));
        }
        bundle.original_path()
    } else {
        bundle.render_path()
    };
    let img = image::open(&src)
        .map_err(|e| CmError::invalid_input(format!("cannot decode `{}`: {e}", src.display())))?;
    let (w, h) = (img.width(), img.height());
    // LIB-1: a tall picture keeps its top part at the card's scale instead of shrinking to a
    // sliver.
    let tall = tall_plan(w, h, max_edge).map(|plan| {
        let top = img.crop_imm(0, 0, w, plan.crop_h);
        let mut rgb = top
            .resize_exact(plan.out_w, plan.out_h, FilterType::Triangle)
            .to_rgb8();
        fade_out(&mut rgb);
        image::DynamicImage::ImageRgb8(rgb)
    });
    let img = if let Some(t) = tall {
        t
    } else if w.max(h) > max_edge {
        let scale = f64::from(max_edge) / f64::from(w.max(h));
        let nw = ((f64::from(w) * scale).round() as u32).max(1);
        let nh = ((f64::from(h) * scale).round() as u32).max(1);
        img.resize_exact(nw, nh, FilterType::Triangle)
    } else {
        img
    };
    let rgb = img.to_rgb8();
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 85)
        .encode_image(&rgb)
        .map_err(|e| CmError::internal(format!("cannot encode poster: {e}")))?;
    let dir = bundle.thumbs_dir();
    std::fs::create_dir_all(&dir).map_err(|e| io_err("cannot create", &dir, &e))?;
    let path = dir.join(POSTER_FILE);
    write_atomic(&path, &out)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// LIB-1: a 1540 x 8330 Long Shot was one 95 x 512 sliver; now the top part is cut to the
    /// card's shape at the poster's width. Ordinary and moderately tall pictures keep the old path.
    #[test]
    fn lib_1_a_tall_picture_gets_its_top_part_at_the_posters_width() {
        let p = tall_plan(1540, 8330, 512).unwrap();
        assert_eq!((p.out_w, p.out_h), (512, 320));
        // 320 poster rows are 963 source rows: the top part, not the whole picture.
        assert_eq!(p.crop_h, 963);
        // A narrower picture is never upscaled.
        let narrow = tall_plan(300, 3000, 512).unwrap();
        assert_eq!((narrow.out_w, narrow.out_h, narrow.crop_h), (300, 188, 188));
        // Not tall: the old longest-edge poster.
        assert!(tall_plan(1540, 3000, 512).is_none());
        assert!(tall_plan(1920, 1080, 512).is_none());
        // The line: more than twice as high as wide.
        assert!(tall_plan(1000, 2000, 512).is_none());
        assert!(tall_plan(1000, 2001, 512).is_some());
    }

    #[test]
    fn lib_1_the_fade_leaves_the_top_alone_and_dims_the_bottom_rows() {
        let mut img = image::RgbImage::from_pixel(8, 40, image::Rgb([255, 255, 255]));
        for y in 20..40 {
            for x in 0..8 {
                img.put_pixel(x, y, image::Rgb([0, 0, 0]));
            }
        }
        let before = img.clone();
        fade_out(&mut img);
        // The top three quarters are untouched.
        assert!((0..30).all(|y| img.get_pixel(0, y) == before.get_pixel(0, y)));
        // The last row moved toward the mean (127), the first faded row only a little.
        let (first, last) = (img.get_pixel(0, 30).0[0], img.get_pixel(0, 39).0[0]);
        assert!(first < 60 && last > first && last > 80, "{first} {last}");
    }
}
