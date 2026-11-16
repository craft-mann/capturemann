#![allow(clippy::unwrap_used)]
//! BR-01: the board sheet's preview is drawn by the same code as the export, so it looks the
//! same. For each layout the export is shrunk to the preview's size and compared with it.

use cm_render::compose_board::{
    plan, render_board, render_preview, Arrangement, Background, BoardLayout, BoardOptions,
    LayoutKind, Slot,
};
use image::{imageops::FilterType, Rgba, RgbaImage};

/// A picture with a smooth gradient and a frame, so scaling it two ways gives the same result
/// within a small error.
fn picture(w: u32, h: u32, seed: u8) -> RgbaImage {
    RgbaImage::from_fn(w, h, |x, y| {
        let edge = x < 6 || y < 6 || x + 6 >= w || y + 6 >= h;
        if edge {
            return Rgba([40, 40, 60, 255]);
        }
        Rgba([
            (x * 200 / w) as u8 ^ seed,
            (y * 200 / h) as u8,
            80u8.wrapping_add(seed.wrapping_mul(30)),
            255,
        ])
    })
}

/// Mean absolute difference per channel (0 to 255) and the share of pixels differing by more
/// than 48.
fn compare(a: &RgbaImage, b: &RgbaImage) -> (f64, f64) {
    assert_eq!(a.dimensions(), b.dimensions());
    let (mut sum, mut big) = (0u64, 0u64);
    for (p, q) in a.pixels().zip(b.pixels()) {
        let mut worst = 0;
        for c in 0..4 {
            let d = u64::from(p[c].abs_diff(q[c]));
            sum += d;
            worst = worst.max(d);
        }
        if worst > 48 {
            big += 1;
        }
    }
    let n = u64::from(a.width()) * u64::from(a.height());
    (sum as f64 / (n * 4) as f64, big as f64 / n as f64)
}

fn check(name: &str, sizes: &[(u32, u32)], layout: BoardLayout, opts: BoardOptions) {
    let full: Vec<RgbaImage> = sizes
        .iter()
        .enumerate()
        .map(|(i, &(w, h))| picture(w, h, i as u8 * 40))
        .collect();
    let export = render_board(&full, &layout, &opts).unwrap();
    // Thumbnails like the library's: a quarter of the size.
    let thumbs: Vec<RgbaImage> = full
        .iter()
        .map(|p| {
            image::imageops::resize(
                p,
                (p.width() / 4).max(1),
                (p.height() / 4).max(1),
                FilterType::Triangle,
            )
        })
        .collect();
    let max_edge = export.width().max(export.height()) / 3;
    let (preview, ps) = render_preview(sizes, &thumbs, &layout, &opts, max_edge).unwrap();
    assert!(ps < 1.0 && ps > 0.0, "{name}: preview scale {ps}");
    let p = plan(sizes, &layout, &opts).unwrap();
    assert_eq!(
        preview.dimensions(),
        (
            (f64::from(p.width) * ps).round() as u32,
            (f64::from(p.height) * ps).round() as u32
        ),
        "{name}: the preview has the plan's proportions"
    );
    let small = image::imageops::resize(
        &export,
        preview.width(),
        preview.height(),
        FilterType::Triangle,
    );
    let (mean, big) = compare(&small, &preview);
    eprintln!("{name}: mean {mean:.2}, big {:.4}", big);
    assert!(mean < 1.5, "{name}: mean difference {mean:.2}");
    assert!(
        big < 0.015,
        "{name}: {:.2}% of pixels differ a lot",
        big * 100.0
    );
}

fn dressed() -> BoardOptions {
    BoardOptions {
        gap: 24,
        margin: 40,
        radius: 12,
        shadow: true,
        background: Background::LightGrey,
        captions: vec![
            "First".into(),
            "Second".into(),
            "Third".into(),
            "Fourth".into(),
        ],
        title: Some("Release board".into()),
        ..Default::default()
    }
}

#[test]
fn preview_matches_the_export_for_pair() {
    let mut o = dressed();
    o.captions.truncate(2);
    o.before_after = true;
    check(
        "pair",
        &[(1600, 1000), (1400, 1100)],
        BoardLayout::new(LayoutKind::Pair),
        o.clone(),
    );
    let mut l = BoardLayout::new(LayoutKind::Pair);
    l.arrangement = Arrangement::Stacked;
    check("pair stacked", &[(1600, 1000), (1400, 1100)], l, o);
}

#[test]
fn preview_matches_the_export_for_strip() {
    let mut o = dressed();
    o.captions.truncate(3);
    check(
        "strip",
        &[(1200, 800), (1000, 900), (1400, 800)],
        BoardLayout::new(LayoutKind::Strip),
        o,
    );
}

#[test]
fn preview_matches_the_export_for_grid() {
    check(
        "grid",
        &[(1600, 1000), (1400, 1100), (1500, 900), (1300, 1000)],
        BoardLayout::new(LayoutKind::Grid),
        dressed(),
    );
    let mut o = dressed();
    o.background = Background::Dark;
    o.shadow = false;
    o.radius = 0;
    o.captions.clear();
    check(
        "grid dark",
        &[(1600, 1000), (1400, 1100), (1500, 900)],
        BoardLayout::new(LayoutKind::Grid),
        o,
    );
}

#[test]
fn preview_matches_the_export_for_free() {
    let mut l = BoardLayout::new(LayoutKind::Free);
    l.slots = vec![
        Slot {
            x: 60,
            y: 120,
            w: 1200,
            h: 800,
        },
        Slot {
            x: 900,
            y: 600,
            w: 1000,
            h: 700,
        },
        Slot {
            x: 200,
            y: 1000,
            w: 800,
            h: 600,
        },
    ];
    let mut o = dressed();
    o.captions.truncate(3);
    check("free", &[(1200, 800), (1000, 700), (800, 600)], l, o);
}
