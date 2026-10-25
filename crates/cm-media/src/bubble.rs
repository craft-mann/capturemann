//! Pre-rendered bubble assets the filter graph reads: an 8-bit **mask** (white inside the outer
//! shape, anti-aliased edge) that becomes the camera's alpha, and the **ring** (the 4 px `rec`
//! ring `#FF4F3A`, transparent inside and outside) drawn over it. Both come from a signed-distance
//! rounded rectangle; a circle is a rounded rectangle whose radius is half its side.

use std::path::Path;

use cm_core::CmError;
use image::{GrayImage, Luma, Rgba, RgbaImage};

use crate::layout::{BubbleGeom, RING_RGB};

/// Distance from `(px, py)` (relative to the box centre) to a rounded rectangle with half sizes
/// `(hx, hy)` and corner radius `r`; negative inside.
fn sdf(px: f64, py: f64, hx: f64, hy: f64, r: f64) -> f64 {
    let qx = px.abs() - (hx - r);
    let qy = py.abs() - (hy - r);
    qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - r
}

/// Pixel coverage 0..1 from a signed distance (anti-aliased over one pixel).
fn coverage(d: f64) -> f64 {
    (0.5 - d).clamp(0.0, 1.0)
}

/// The mask: coverage of the outer shape.
pub fn mask_image(g: &BubbleGeom) -> GrayImage {
    let (hx, hy) = (f64::from(g.w) / 2.0, f64::from(g.h) / 2.0);
    GrayImage::from_fn(g.w, g.h, |x, y| {
        let d = sdf(
            f64::from(x) + 0.5 - hx,
            f64::from(y) + 0.5 - hy,
            hx,
            hy,
            g.radius_px,
        );
        Luma([(coverage(d) * 255.0).round() as u8])
    })
}

/// The ring: coverage(outer) minus coverage(outer inset by `ring_px`), in `rec` red.
pub fn ring_image(g: &BubbleGeom) -> RgbaImage {
    let (hx, hy) = (f64::from(g.w) / 2.0, f64::from(g.h) / 2.0);
    let ring = f64::from(g.ring_px);
    let inner_r = (g.radius_px - ring).max(0.0);
    RgbaImage::from_fn(g.w, g.h, |x, y| {
        let (px, py) = (f64::from(x) + 0.5 - hx, f64::from(y) + 0.5 - hy);
        let outer = coverage(sdf(px, py, hx, hy, g.radius_px));
        let inner = coverage(sdf(px, py, hx - ring, hy - ring, inner_r));
        let a = (outer * (1.0 - inner)).clamp(0.0, 1.0);
        Rgba([
            RING_RGB[0],
            RING_RGB[1],
            RING_RGB[2],
            (a * 255.0).round() as u8,
        ])
    })
}

/// Writes `mask.png` and `ring.png` into `dir`; returns their paths.
pub fn write_assets(
    dir: &Path,
    g: &BubbleGeom,
) -> Result<(std::path::PathBuf, std::path::PathBuf), CmError> {
    // A full disk while writing the PNGs is `disk_full`, like the video itself.
    let io = |what: &str, e: &image::ImageError| match e {
        image::ImageError::IoError(ioe) => {
            crate::run::map_io_error(&format!("cannot write the bubble {what}"), ioe)
        }
        other => CmError::internal(format!("cannot write the bubble {what}: {other}")),
    };
    let (mask, ring) = (dir.join("mask.png"), dir.join("ring.png"));
    mask_image(g).save(&mask).map_err(|e| io("mask", &e))?;
    ring_image(g).save(&ring).map_err(|e| io("ring", &e))?;
    Ok((mask, ring))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn geom(w: u32, h: u32, ring: u32, r: f64) -> BubbleGeom {
        BubbleGeom {
            x: 0,
            y: 0,
            w,
            h,
            ring_px: ring,
            radius_px: r,
        }
    }

    /// CAM-1 / DESIGN 5.4: a circle with a 4 px rec ring that is part of the outer size.
    #[test]
    fn circle_mask_and_ring() {
        let g = geom(100, 100, 4, 50.0);
        let m = mask_image(&g);
        assert_eq!(m.get_pixel(50, 50).0[0], 255, "centre inside");
        assert_eq!(m.get_pixel(1, 1).0[0], 0, "corner outside the circle");
        assert_eq!(m.get_pixel(98, 98).0[0], 0);
        assert!(m.get_pixel(50, 1).0[0] > 200, "top edge inside");
        let r = ring_image(&g);
        // Centre: transparent. On the ring (2 px in from the top edge): rec red, opaque.
        assert_eq!(r.get_pixel(50, 50).0[3], 0);
        let p = r.get_pixel(50, 2).0;
        assert_eq!([p[0], p[1], p[2]], [0xFF, 0x4F, 0x3A]);
        assert!(p[3] > 240, "ring opaque: {p:?}");
        // Just inside the ring and outside the circle: transparent.
        assert_eq!(r.get_pixel(50, 6).0[3], 0);
        assert_eq!(r.get_pixel(0, 0).0[3], 0);
        // The ring is 4 px: rows 0..4 at the top centre are (nearly) opaque, row 4 is not.
        assert!(r.get_pixel(50, 3).0[3] > 240);
        assert!(r.get_pixel(50, 4).0[3] < 15);
    }

    #[test]
    fn rounded_rectangle_has_square_ish_corners_with_a_radius() {
        let g = geom(120, 90, 4, 12.0);
        let m = mask_image(&g);
        assert_eq!(m.get_pixel(0, 0).0[0], 0, "the very corner is cut");
        assert_eq!(m.get_pixel(12, 12).0[0], 255);
        assert!(m.get_pixel(60, 0).0[0] > 200, "straight edge");
        let r = ring_image(&g);
        assert!(r.get_pixel(60, 1).0[3] > 240 && r.get_pixel(1, 45).0[3] > 240);
    }
}
