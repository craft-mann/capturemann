//! Shared helpers for the integration tests.
#![allow(dead_code, clippy::unwrap_used)]

use image::{Rgba, RgbaImage};
use std::path::PathBuf;

/// Deterministic synthetic "screenshot": light gradient, a dark title bar, and a block of
/// high-contrast stripes (stand-in for text that redaction must destroy).
pub fn base_image(w: u32, h: u32) -> RgbaImage {
    RgbaImage::from_fn(w, h, |x, y| {
        let g = 200 + ((x * 40) / w.max(1)) as u8;
        let mut p = [g, g.saturating_sub((y * 30 / h.max(1)) as u8), 230u8];
        if y < h / 8 {
            p = [40, 44, 52];
        }
        // stripes block in the lower-left area
        if x > w / 10 && x < w / 2 && y > h / 2 && y < h * 3 / 4 && (x / 2) % 2 == 0 {
            p = [10, 10, 10];
        }
        Rgba([p[0], p[1], p[2], 255])
    })
}

/// Pseudo-random noise image (xorshift); every pixel is effectively unique.
pub fn noise_image(w: u32, h: u32) -> RgbaImage {
    let mut s: u32 = 0x1234_5678;
    RgbaImage::from_fn(w, h, |_, _| {
        let mut next = || {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            (s >> 8) as u8
        };
        Rgba([next(), next(), next(), 255])
    })
}

pub fn ops(json: &str) -> Vec<cm_render::AnnotationOp> {
    cm_render::parse_ops_str(json).unwrap()
}

/// Fraction of pixels whose max channel difference exceeds `tol`.
pub fn diff_fraction(a: &RgbaImage, b: &RgbaImage, tol: u8) -> f64 {
    assert_eq!(a.dimensions(), b.dimensions(), "size mismatch");
    let bad = a
        .pixels()
        .zip(b.pixels())
        .filter(|(p, q)| {
            p.0.iter()
                .zip(q.0.iter())
                .any(|(x, y)| x.abs_diff(*y) > tol)
        })
        .count();
    bad as f64 / (a.width() as f64 * a.height() as f64)
}

pub fn golden_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(format!("{name}.png"))
}

/// Compares against `tests/golden/<name>.png` (<= `max_frac` of pixels may differ by > 8/255).
/// `UPDATE_GOLDEN=1 cargo test -p cm-render` (re)writes the fixtures instead of comparing.
pub fn assert_golden(name: &str, actual: &RgbaImage, max_frac: f64) {
    let path = golden_path(name);
    if std::env::var_os("UPDATE_GOLDEN").is_some_and(|v| v != "0") {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, cm_render::encode_png(actual).unwrap()).unwrap();
        return;
    }
    let expected = image::open(&path)
        .unwrap_or_else(|e| {
            panic!(
                "missing golden {}: {e} (run with UPDATE_GOLDEN=1 to create)",
                path.display()
            )
        })
        .to_rgba8();
    let frac = diff_fraction(actual, &expected, 8);
    assert!(
        frac <= max_frac,
        "golden {name}: {:.3}% of pixels differ (limit {:.3}%)",
        frac * 100.0,
        max_frac * 100.0
    );
}
