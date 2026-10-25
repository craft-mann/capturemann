//! Preview rendering. The canvas shows exactly what `cm-render` produces for the current op
//! list, so blur and pixelate are real redactions in every preview (ANN-2): there is no
//! overlay trick that could reveal the pixels underneath, and the preview matches the final
//! render (same deterministic renderer the daemon uses).

use cm_core::AnnotationOp;
use image::RgbaImage;

/// Swaps R and B in place: GPUI's image atlas expects BGRA.
pub fn into_bgra(mut img: RgbaImage) -> RgbaImage {
    for px in img.as_chunks_mut::<4>().0 {
        px.swap(0, 2);
    }
    img
}

/// Renders `ops` (without crop) over `base` and returns BGRA pixels for GPUI.
pub fn render_bgra(base: &RgbaImage, ops: &[AnnotationOp]) -> Result<RgbaImage, String> {
    let out = if ops.is_empty() {
        base.clone()
    } else {
        cm_render::render_annotations(base, ops).map_err(|e| e.to_string())?
    };
    Ok(into_bgra(out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cm_core::{Color, OpKind, Pt, Rc};

    fn checker(w: u32, h: u32) -> RgbaImage {
        RgbaImage::from_fn(w, h, |x, y| {
            if (x / 2 + y / 2) % 2 == 0 {
                image::Rgba([250, 10, 10, 255])
            } else {
                image::Rgba([10, 10, 250, 255])
            }
        })
    }

    /// The preview is the deterministic renderer's output, channel-swapped for GPUI.
    #[test]
    fn preview_equals_the_final_render() {
        let base = checker(64, 48);
        let ops = vec![
            AnnotationOp::new(
                "a",
                OpKind::Arrow {
                    from: Pt::new(5.0, 5.0),
                    to: Pt::new(50.0, 30.0),
                    color: Color::WHITE,
                    width: 4.0,
                },
            ),
            AnnotationOp::new(
                "b",
                OpKind::Blur {
                    rect: Rc::new(10.0, 10.0, 20.0, 20.0),
                    strength: 12,
                },
            ),
        ];
        let want = cm_render::render_annotations(&base, &ops).unwrap();
        let got = render_bgra(&base, &ops).unwrap();
        for (w, g) in want.pixels().zip(got.pixels()) {
            assert_eq!([w.0[2], w.0[1], w.0[0], w.0[3]], g.0);
        }
        // Deterministic.
        assert_eq!(render_bgra(&base, &ops).unwrap(), got);
    }

    /// ANN-2.AC1 (preview): inside a pixelate rect the preview has block-constant colors,
    /// never the original pattern; inside a blur rect the sharp checker is gone.
    #[test]
    fn redactions_never_show_original_pixels_in_the_preview() {
        let base = checker(64, 64);
        let rect = Rc::new(8.0, 8.0, 48.0, 48.0);
        for kind in [
            OpKind::Pixelate { rect, block: 12 },
            OpKind::Blur { rect, strength: 12 },
        ] {
            let out = render_bgra(&base, &[AnnotationOp::new("r", kind.clone())]).unwrap();
            // Count pixels inside the rect equal to a pure original color.
            let mut same = 0;
            let mut total = 0;
            for y in 14..50 {
                for x in 14..50 {
                    total += 1;
                    let o = base.get_pixel(x, y).0;
                    let p = out.get_pixel(x, y).0;
                    if [o[2], o[1], o[0], o[3]] == p {
                        same += 1;
                    }
                }
            }
            assert!(
                same * 10 < total,
                "{kind:?}: {same}/{total} original pixels survived"
            );
            // Outside the rect nothing changed.
            let o = base.get_pixel(2, 2).0;
            assert_eq!(out.get_pixel(2, 2).0, [o[2], o[1], o[0], o[3]]);
        }
    }

    #[test]
    fn no_ops_returns_the_original() {
        let base = checker(8, 8);
        assert_eq!(render_bgra(&base, &[]).unwrap(), into_bgra(base));
    }
}
