//! Golden-frame and pixel tests for the caption layer (AI-1: burned-in captions, AC5 safe area).
//!
//! Goldens live in `tests/golden/compose_caption_*.png`; regenerate after an intentional change
//! with `UPDATE_GOLDEN=1 cargo test -p cm-render --test compose_caption` and review the diffs.
//! Without a GPU adapter every test prints `SKIPPED` and passes.
#![allow(clippy::unwrap_used)]

mod common;

use cm_render::compose::{
    caption_px, safe_insets, Background, Bubble, BubbleShape, CameraLayer, CaptionLayer, Frame,
    Layout, Scene,
};
use cm_render::Compositor;
use common::assert_golden;
use image::{Rgba, RgbaImage};

const LIMIT: f64 = 0.005;

fn gpu() -> Option<Compositor> {
    match Compositor::new() {
        Ok(c) => Some(c),
        Err(e) => {
            println!("SKIPPED: no GPU compositor ({e})");
            None
        }
    }
}

fn frame(img: &RgbaImage) -> Frame<'_> {
    Frame::rgba(img.width(), img.height(), img.as_raw())
}

/// A bright, busy picture: the caption box must stay readable on it.
fn screen_img(w: u32, h: u32) -> RgbaImage {
    RgbaImage::from_fn(w, h, |x, y| {
        let stripe = ((x / 24 + y / 24) % 2) as u8;
        Rgba([
            200 + 40 * stripe,
            210 - 20 * stripe,
            (120 + 120 * x / w) as u8,
            255,
        ])
    })
}

fn camera_img(w: u32, h: u32) -> RgbaImage {
    RgbaImage::from_fn(w, h, |x, _| {
        if x < w / 2 {
            Rgba([220, 40, 40, 255])
        } else {
            Rgba([40, 80, 220, 255])
        }
    })
}

/// Pixels that differ from `base` anywhere in `region` (x0, y0, x1, y1).
fn changed_in(base: &RgbaImage, out: &RgbaImage, region: (u32, u32, u32, u32)) -> usize {
    let mut n = 0;
    for y in region.1..region.3 {
        for x in region.0..region.2 {
            if base.get_pixel(x, y) != out.get_pixel(x, y) {
                n += 1;
            }
        }
    }
    n
}

fn render(c: &mut Compositor, out: (u32, u32), text: Option<&str>) -> RgbaImage {
    let s = screen_img(out.0, out.1);
    let mut scene = Scene::new(out, frame(&s));
    scene.background = Background::None;
    scene.captions = text.map(|t| CaptionLayer::for_output(t, out));
    c.render(&scene).unwrap()
}

/// AI-1: a short line on 16:9 is a white-on-60%-black box near the bottom, centered.
#[test]
fn ai1_burned_in_caption_16_9() {
    let Some(mut c) = gpu() else { return };
    let base = render(&mut c, (960, 540), None);
    let out = render(&mut c, (960, 540), Some("Hello, this is a caption"));
    assert_golden("compose_caption_16x9", &out, LIMIT);
    // Nothing changes in the top 80% of the frame.
    assert_eq!(changed_in(&base, &out, (0, 0, 960, 430)), 0);
    // The caption sits in the lower part and is centered horizontally.
    let ins = safe_insets((960, 540));
    let (mut x_min, mut x_max, mut y_max) = (u32::MAX, 0u32, 0u32);
    for y in 430..540 {
        for x in 0..960 {
            if base.get_pixel(x, y) != out.get_pixel(x, y) {
                x_min = x_min.min(x);
                x_max = x_max.max(x);
                y_max = y_max.max(y);
            }
        }
    }
    assert!(x_max > x_min, "something was drawn");
    let center = f64::from(x_min + x_max) / 2.0;
    assert!((center - 480.0).abs() <= 2.0, "centered, got {center}");
    assert!(
        f64::from(y_max) <= 540.0 * (1.0 - f64::from(ins[3])) + 1.0,
        "bottom margin kept: {y_max}"
    );
    // White text exists inside the box, and the box darkens its surroundings (60% black).
    let bright = (x_min..=x_max)
        .flat_map(|x| (430..540).map(move |y| (x, y)))
        .filter(|&(x, y)| out.get_pixel(x, y).0[..3].iter().all(|&v| v >= 250))
        .count();
    assert!(bright > 50, "white glyph pixels: {bright}");
}

/// AI-1.AC5: on 9:16 the caption stays out of the bottom 20% and the right 12%.
#[test]
fn ai1_ac5_burned_in_caption_9_16_stays_in_the_safe_area() {
    let Some(mut c) = gpu() else { return };
    let (w, h) = (540u32, 960u32);
    let base = render(&mut c, (w, h), None);
    let out = render(
        &mut c,
        (w, h),
        Some("A caption on a vertical video that has to stay inside the platform safe area"),
    );
    assert_golden("compose_caption_9x16", &out, LIMIT);
    let bottom_20 = (h as f32 * 0.80).ceil() as u32 + 1;
    let right_12 = (w as f32 * 0.88).ceil() as u32 + 1;
    assert_eq!(
        changed_in(&base, &out, (0, bottom_20, w, h)),
        0,
        "the bottom 20% is clear"
    );
    assert_eq!(
        changed_in(&base, &out, (right_12, 0, w, h)),
        0,
        "the right 12% is clear"
    );
    assert!(changed_in(&base, &out, (0, 0, w, h)) > 200, "drawn");
}

/// A long line wraps into two rows inside the safe width; the box grows taller, not wider.
#[test]
fn ai1_long_line_wraps_to_two_rows() {
    let Some(mut c) = gpu() else { return };
    let (w, h) = (960u32, 540u32);
    let long = "This is a long caption line that does not fit on one row of the frame so it wraps";
    let base = render(&mut c, (w, h), None);
    let one = render(&mut c, (w, h), Some("Short line"));
    let two = render(&mut c, (w, h), Some(long));
    assert_golden("compose_caption_two_rows", &two, LIMIT);
    let rows = |img: &RgbaImage| {
        (0..h)
            .filter(|&y| (0..w).any(|x| base.get_pixel(x, y) != img.get_pixel(x, y)))
            .count()
    };
    assert!(rows(&two) > rows(&one) + 20, "taller with two rows");
    let left = ((w as f32) * 0.05).floor() as u32 - 1;
    let right = ((w as f32) * 0.95).ceil() as u32 + 1;
    assert_eq!(changed_in(&base, &two, (0, 0, left, h)), 0, "left margin");
    assert_eq!(changed_in(&base, &two, (right, 0, w, h)), 0, "right margin");
}

/// The font size scales with the output height.
#[test]
fn ai1_caption_text_scales_with_the_output_height() {
    let l = CaptionLayer::new("x");
    assert!(caption_px(&l, (3840, 2160)) > caption_px(&l, (1920, 1080)) * 1.99);
}

/// The caption moves above a bottom-left bubble it would cover.
#[test]
fn ai1_caption_keeps_off_the_camera_bubble() {
    let Some(mut c) = gpu() else { return };
    let (w, h) = (960u32, 540u32);
    let s = screen_img(w, h);
    let cam = camera_img(320, 240);
    let mut scene = Scene::new((w, h), frame(&s));
    scene.camera = Some(CameraLayer {
        frame: frame(&cam),
        mirror: false,
    });
    scene.layout = Layout::ScreenBubble(Bubble::bottom_left());
    let without = c.render(&scene).unwrap();
    scene.captions = Some(CaptionLayer::for_output(
        "A caption wide enough to run over the bubble at the bottom left of the frame",
        (w, h),
    ));
    let with = c.render(&scene).unwrap();
    assert_golden("compose_caption_over_bubble", &with, LIMIT);
    // The bubble's own rectangle (from the plan) is untouched.
    let r = cm_render::compose::plan(&scene)
        .unwrap()
        .camera
        .unwrap()
        .rect;
    let bubble = (
        r[0] as u32,
        r[1] as u32,
        ((r[0] + r[2]) as u32).min(w),
        ((r[1] + r[3]) as u32).min(h),
    );
    assert_eq!(
        changed_in(&without, &with, bubble),
        0,
        "the bubble stays visible"
    );
    assert!(changed_in(&without, &with, (0, 0, w, h)) > 200, "drawn");
}

/// A short caption next to a large bubble stays at the bottom and slides into the free side
/// (it does not jump to the top of the frame).
#[test]
fn ai1_caption_slides_beside_a_large_bubble() {
    let Some(mut c) = gpu() else { return };
    let (w, h) = (640u32, 360u32);
    let s = screen_img(w, h);
    let cam = camera_img(320, 240);
    let mut scene = Scene::new((w, h), frame(&s));
    scene.camera = Some(CameraLayer {
        frame: frame(&cam),
        mirror: false,
    });
    scene.layout = Layout::ScreenBubble(Bubble {
        shape: BubbleShape::Circle,
        size: 0.66,
        center: (1.0, 1.0),
        ring: None,
    });
    let without = c.render(&scene).unwrap();
    scene.captions = Some(CaptionLayer::for_output("Hello there friend", (w, h)));
    let with = c.render(&scene).unwrap();
    assert_golden("compose_caption_beside_bubble", &with, LIMIT);
    let r = cm_render::compose::plan(&scene)
        .unwrap()
        .camera
        .unwrap()
        .rect;
    let bubble = (
        r[0] as u32,
        r[1] as u32,
        ((r[0] + r[2]) as u32).min(w),
        ((r[1] + r[3]) as u32).min(h),
    );
    assert_eq!(
        changed_in(&without, &with, bubble),
        0,
        "the bubble is clear"
    );
    assert!(
        changed_in(&without, &with, (0, h * 4 / 5, w, h)) > 100,
        "the caption stayed at the bottom"
    );
    assert_eq!(
        changed_in(&without, &with, (0, 0, w, h * 3 / 4)),
        0,
        "nothing in the upper three quarters"
    );
}

/// With a background and padding the caption sits inside the screen picture and never hangs off
/// its bottom edge onto the background strip.
#[test]
fn ai1_caption_stays_inside_a_padded_picture() {
    let Some(mut c) = gpu() else { return };
    let (w, h) = (960u32, 540u32);
    let s = screen_img(w, h);
    let mut scene = Scene::new((w, h), frame(&s));
    scene.background = Background::Color([232, 233, 236]);
    scene.padding_pct = 8.0;
    scene.radius_px = 12.0;
    let without = c.render(&scene).unwrap();
    scene.captions = Some(CaptionLayer::for_output(
        "Hello, this caption belongs on the picture",
        (w, h),
    ));
    let with = c.render(&scene).unwrap();
    assert_golden("compose_caption_padded", &with, LIMIT);
    let tile = cm_render::compose::plan(&scene)
        .unwrap()
        .screen
        .unwrap()
        .rect;
    let (tx0, ty0, tx1, ty1) = (
        tile[0] as u32,
        tile[1] as u32,
        (tile[0] + tile[2]) as u32,
        (tile[1] + tile[3]) as u32,
    );
    assert!(
        changed_in(&without, &with, (tx0 + 4, ty0, tx1 - 4, ty1)) > 200,
        "drawn on the picture"
    );
    assert_eq!(
        changed_in(&without, &with, (0, ty1, w, h)),
        0,
        "nothing on the background strip below the picture"
    );
    assert_eq!(
        changed_in(&without, &with, (0, 0, tx0.saturating_sub(1), h)),
        0,
        "nothing on the background to the left"
    );
}

#[test]
fn empty_caption_draws_nothing() {
    let Some(mut c) = gpu() else { return };
    let base = render(&mut c, (640, 360), None);
    let out = render(&mut c, (640, 360), Some("   "));
    assert_eq!(base, out);
}
