//! Cursor, click effects and crop/zoom through the headless GPU compositor (EDT-4, EDT-5).
//!
//! Without a GPU adapter every test prints `SKIPPED` and passes. Goldens are shared with
//! `compose.rs` (`tests/golden/compose_*.png`; regenerate with `UPDATE_GOLDEN=1`).
#![allow(clippy::unwrap_used)]

mod common;

use cm_render::compose::{ClickMark, ClickStyle, CursorLayer, CursorSprite, Frame, Scene};
use cm_render::Compositor;
use common::assert_golden;
use image::{Rgba, RgbaImage};

fn gpu() -> Option<Compositor> {
    match Compositor::new() {
        Ok(c) => Some(c),
        Err(e) => {
            println!("SKIPPED: no GPU compositor ({e})");
            None
        }
    }
}

fn gray(w: u32, h: u32) -> RgbaImage {
    RgbaImage::from_pixel(w, h, Rgba([100, 100, 100, 255]))
}

/// A `n` x `n` opaque red square.
fn red(n: u32) -> RgbaImage {
    RgbaImage::from_pixel(n, n, Rgba([255, 0, 0, 255]))
}

fn layer<'a>(sprite: &'a RgbaImage, hotspot: (f32, f32), pos: (f32, f32)) -> CursorLayer<'a> {
    CursorLayer {
        sprite: Some(CursorSprite {
            frame: Frame::rgba(sprite.width(), sprite.height(), sprite.as_raw()),
            hotspot,
        }),
        pos,
        scale: 1.0,
        opacity: 1.0,
        click_style: ClickStyle::None,
        clicks: &[],
        effect_radius_px: 30.0,
        effect_color: [255, 255, 255, 255],
    }
}

fn is_red(p: &Rgba<u8>) -> bool {
    p[0] > 200 && p[1] < 80 && p[2] < 80
}

/// The bounding box `(x0, y0, x1, y1)` of the red pixels (inclusive), if any.
fn red_box(img: &RgbaImage) -> Option<(u32, u32, u32, u32)> {
    let mut b: Option<(u32, u32, u32, u32)> = None;
    for (x, y, p) in img.enumerate_pixels() {
        if is_red(p) {
            b = Some(match b {
                None => (x, y, x, y),
                Some((x0, y0, x1, y1)) => (x0.min(x), y0.min(y), x1.max(x), y1.max(y)),
            });
        }
    }
    b
}

fn count_red(img: &RgbaImage) -> usize {
    img.pixels().filter(|p| is_red(p)).count()
}

/// EDT-4.AC1: the cursor is drawn at its telemetry position, twice the size at 2x.
#[test]
fn edt4_ac1_cursor_is_drawn_at_its_position_and_scales_with_the_size_setting() {
    let Some(mut c) = gpu() else { return };
    let screen = gray(320, 180);
    let sprite = red(10);
    let mut one = layer(&sprite, (0.0, 0.0), (0.25, 0.25));
    let mut scene = Scene::new((320, 180), Frame::rgba(320, 180, screen.as_raw()));
    scene.cursor = Some(one);
    let a = c.render(&scene).unwrap();
    let (x0, y0, x1, y1) = red_box(&a).unwrap();
    assert_eq!((x0, y0), (80, 45), "the tip is at 25% of the frame");
    assert_eq!((x1 - x0 + 1, y1 - y0 + 1), (10, 10), "natural size at 1x");

    one.scale = 2.0;
    scene.cursor = Some(one);
    let b = c.render(&scene).unwrap();
    let (x0, y0, x1, y1) = red_box(&b).unwrap();
    assert_eq!((x0, y0), (80, 45), "the tip stays put");
    assert_eq!((x1 - x0 + 1, y1 - y0 + 1), (20, 20), "twice the size at 2x");
    assert!(count_red(&b) >= 3 * count_red(&a), "four times the area");
}

/// The cursor lives in the screen's own pixels: a zoom makes it larger and moves it with the
/// content, a hidden cursor (opacity 0) draws nothing.
#[test]
fn edt4_the_cursor_follows_the_zoom_crop() {
    let Some(mut c) = gpu() else { return };
    let screen = gray(320, 180);
    let sprite = red(10);
    let mut scene = Scene::new((320, 180), Frame::rgba(320, 180, screen.as_raw()));
    scene.screen.as_mut().unwrap().crop = Some([0.25, 0.25, 0.5, 0.5]);
    // The hotspot is the sprite's middle; the cursor is in the middle of the frame.
    scene.cursor = Some(layer(&sprite, (5.0, 5.0), (0.5, 0.5)));
    let img = c.render(&scene).unwrap();
    let (x0, y0, x1, y1) = red_box(&img).unwrap();
    assert_eq!(
        (x1 - x0 + 1, y1 - y0 + 1),
        (20, 20),
        "2x zoom doubles the cursor"
    );
    assert_eq!(
        ((x0 + x1) / 2, (y0 + y1) / 2),
        (159, 89),
        "still in the middle"
    );

    // Outside the crop it is not drawn.
    scene.cursor = Some(layer(&sprite, (5.0, 5.0), (0.05, 0.05)));
    assert!(red_box(&c.render(&scene).unwrap()).is_none());

    let mut hidden = layer(&sprite, (5.0, 5.0), (0.5, 0.5));
    hidden.opacity = 0.0;
    scene.cursor = Some(hidden);
    assert!(red_box(&c.render(&scene).unwrap()).is_none());
}

/// Translucent sprite pixels blend with what is under them, without a dark fringe.
#[test]
fn edt4_a_translucent_sprite_blends_over_the_screen() {
    let Some(mut c) = gpu() else { return };
    let screen = gray(64, 64);
    // Half-transparent white, with black RGB in the fully transparent border.
    let mut sprite = RgbaImage::from_pixel(8, 8, Rgba([0, 0, 0, 0]));
    for y in 2..6 {
        for x in 2..6 {
            sprite.put_pixel(x, y, Rgba([255, 255, 255, 128]));
        }
    }
    let mut scene = Scene::new((64, 64), Frame::rgba(64, 64, screen.as_raw()));
    scene.cursor = Some(layer(&sprite, (0.0, 0.0), (0.5, 0.5)));
    let img = c.render(&scene).unwrap();
    let inside = img.get_pixel(32 + 3, 32 + 3).0;
    // 100 + (255 - 100) * 128/255 = about 178.
    assert!(
        (i32::from(inside[0]) - 178).abs() <= 4,
        "blended: {inside:?}"
    );
    let border = img.get_pixel(32 + 1, 32 + 1).0;
    assert_eq!(
        &border[..3],
        &[100, 100, 100],
        "the transparent border leaves the screen alone"
    );
}

fn ring_scene<'a>(
    screen: &'a RgbaImage,
    sprite: &'a RgbaImage,
    style: ClickStyle,
    clicks: &'a [ClickMark],
) -> Scene<'a> {
    let mut scene = Scene::new(
        (screen.width(), screen.height()),
        Frame::rgba(screen.width(), screen.height(), screen.as_raw()),
    );
    let mut l = layer(sprite, (0.0, 0.0), (0.5, 0.5));
    l.sprite = None;
    l.opacity = 1.0;
    l.click_style = style;
    l.clicks = clicks;
    l.effect_radius_px = 40.0;
    l.effect_color = [255, 255, 255, 255];
    scene.cursor = Some(l);
    let _ = sprite;
    scene
}

/// A ring expands from the click and fades; a ripple also fills a soft disc.
#[test]
fn edt4_click_effects_expand_and_fade() {
    let Some(mut c) = gpu() else { return };
    let screen = gray(200, 200);
    let sprite = red(2);
    let click = |p: f32| {
        [ClickMark {
            pos: (0.5, 0.5),
            progress: p,
        }]
    };
    let brightest_ring = |img: &RgbaImage| -> (u32, u32) {
        // Scan along the row through the click: the radius with the brightest pixel.
        let mut best = (0u32, 0u32);
        for x in 100..200u32 {
            let v = u32::from(img.get_pixel(x, 100).0[0]);
            if v > best.1 {
                best = (x - 100, v);
            }
        }
        best
    };
    let early = click(0.2);
    let later = click(0.7);
    let a = c
        .render(&ring_scene(&screen, &sprite, ClickStyle::Ring, &early))
        .unwrap();
    let b = c
        .render(&ring_scene(&screen, &sprite, ClickStyle::Ring, &later))
        .unwrap();
    let (ra, va) = brightest_ring(&a);
    let (rb, vb) = brightest_ring(&b);
    assert!(rb > ra + 5, "the ring grows ({ra} -> {rb})");
    assert!(va > vb, "and fades ({va} -> {vb})");
    assert!(va > 150, "the early ring is clearly visible: {va}");

    // Nothing at the end of the effect, and nothing without a style.
    let done = click(1.0);
    let end = c
        .render(&ring_scene(&screen, &sprite, ClickStyle::Ring, &done))
        .unwrap();
    assert!(end.pixels().all(|p| p.0[0] == 100));
    let none = c
        .render(&ring_scene(&screen, &sprite, ClickStyle::None, &early))
        .unwrap();
    assert!(none.pixels().all(|p| p.0[0] == 100));

    // A ripple has a soft disc inside its ring.
    let rip = c
        .render(&ring_scene(&screen, &sprite, ClickStyle::Ripple, &early))
        .unwrap();
    assert!(
        rip.get_pixel(100, 100).0[0] > 110,
        "the disc lightens the click spot"
    );
    assert_golden("compose_click_ripple", &rip, 0.005);
}

/// The reframe crop of EDT-5 in the compositor: a 9:16 window of a 16:9 frame fills a portrait
/// output edge to edge, showing exactly that part of the screen.
#[test]
fn edt5_a_portrait_crop_fills_the_output_edge_to_edge() {
    let Some(mut c) = gpu() else { return };
    // Left third red, middle green, right third blue.
    let screen = RgbaImage::from_fn(480, 270, |x, _| match x / 160 {
        0 => Rgba([220, 30, 30, 255]),
        1 => Rgba([30, 200, 30, 255]),
        _ => Rgba([30, 30, 220, 255]),
    });
    let w = 9.0f32 / 16.0 / (16.0 / 9.0);
    let mut scene = Scene::new((270, 480), Frame::rgba(480, 270, screen.as_raw()));
    // The window is centered on the middle third.
    scene.screen.as_mut().unwrap().crop = Some([0.5 - w / 2.0, 0.0, w, 1.0]);
    let img = c.render(&scene).unwrap();
    assert_eq!(img.dimensions(), (270, 480));
    for (x, y) in [(0, 0), (269, 0), (0, 479), (269, 479), (135, 240)] {
        let p = img.get_pixel(x, y).0;
        assert!(
            p[1] > 150 && p[0] < 90 && p[2] < 90,
            "({x},{y}) is green: {p:?}"
        );
    }
}
