//! RGBA to NV12 on the CPU, for feeding GPUI's `surface()` from the compositor's output.
//!
//! The inverse of [`super::frame::nv12_to_bgra`]: BT.601 full range (`420f`), chroma averaged over
//! each 2x2 block. Used by the editor preview; the compositor's own NV12 path is for input only.

use image::RgbaImage;

use super::frame::Nv12Frame;

/// Converts `img` to full-range NV12. Odd sizes are cropped by one pixel to the even size NV12
/// needs.
pub fn rgba_to_nv12(img: &RgbaImage) -> Nv12Frame {
    let w = (img.width() as usize) & !1;
    let h = (img.height() as usize) & !1;
    let src = img.as_raw();
    let stride = img.width() as usize * 4;
    let mut y = vec![0u8; w * h];
    let mut uv = vec![128u8; w * h / 2];
    for row in 0..h {
        for x in 0..w {
            let p = &src[row * stride + x * 4..row * stride + x * 4 + 3];
            let (r, g, b) = (i32::from(p[0]), i32::from(p[1]), i32::from(p[2]));
            y[row * w + x] = ((77 * r + 150 * g + 29 * b) >> 8).clamp(0, 255) as u8;
        }
    }
    for brow in 0..h / 2 {
        for bx in 0..w / 2 {
            let (mut sr, mut sg, mut sb) = (0i32, 0i32, 0i32);
            for (dy, dx) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
                let o = (brow * 2 + dy) * stride + (bx * 2 + dx) * 4;
                sr += i32::from(src[o]);
                sg += i32::from(src[o + 1]);
                sb += i32::from(src[o + 2]);
            }
            let (r, g, b) = (sr / 4, sg / 4, sb / 4);
            let cb = 128 + ((-43 * r - 85 * g + 128 * b) >> 8);
            let cr = 128 + ((128 * r - 107 * g - 21 * b) >> 8);
            uv[brow * w + bx * 2] = cb.clamp(0, 255) as u8;
            uv[brow * w + bx * 2 + 1] = cr.clamp(0, 255) as u8;
        }
    }
    Nv12Frame {
        width: w,
        height: h,
        y,
        uv,
    }
}

/// RGBA to the packed BGRA that GPUI's `img()` atlas wants (R and B swapped, alpha 255).
pub fn rgba_to_bgra(img: &RgbaImage) -> RgbaImage {
    let mut out = img.clone();
    for p in out.pixels_mut() {
        p.0 = [p.0[2], p.0[1], p.0[0], 255];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preview::frame::nv12_to_bgra;
    use image::Rgba;

    /// A flat colour survives RGBA -> NV12 -> BGRA within the quantisation of the conversion.
    #[test]
    fn flat_colours_round_trip() {
        for rgb in [
            [200u8, 40, 40],
            [30, 160, 90],
            [20, 30, 220],
            [255, 255, 255],
            [0, 0, 0],
        ] {
            let img = RgbaImage::from_pixel(8, 8, Rgba([rgb[0], rgb[1], rgb[2], 255]));
            let bgra = nv12_to_bgra(&rgba_to_nv12(&img));
            for px in bgra.as_chunks::<4>().0 {
                for (got, want) in [(px[2], rgb[0]), (px[1], rgb[1]), (px[0], rgb[2])] {
                    assert!(got.abs_diff(want) <= 6, "{got} vs {want} for {rgb:?}");
                }
            }
        }
    }

    #[test]
    fn plane_sizes_and_odd_sizes() {
        let f = rgba_to_nv12(&RgbaImage::new(7, 5));
        assert_eq!((f.width, f.height), (6, 4));
        assert_eq!(f.y.len(), 24);
        assert_eq!(f.uv.len(), 12);
    }

    #[test]
    fn bgra_swaps_red_and_blue() {
        let img = RgbaImage::from_pixel(1, 1, Rgba([1, 2, 3, 9]));
        assert_eq!(rgba_to_bgra(&img).get_pixel(0, 0).0, [3, 2, 1, 255]);
    }
}
