//! Golden-frame and pixel tests for the headless GPU compositor (EDT-3, CAM-3, EXP-1).
//!
//! Goldens live in `tests/golden/compose_*.png`. A pixel counts as different when a channel is off
//! by more than 8/255 (see `common::assert_golden`); a golden passes when at most 0.5% of pixels
//! differ, which absorbs GPU-to-GPU rounding at anti-aliased edges but not a wrong shape, size or
//! color. Regenerate after an intentional change: `UPDATE_GOLDEN=1 cargo test -p cm-render
//! --test compose`, then review the PNG diffs.
//!
//! Without a GPU adapter every test prints `SKIPPED` and passes.
#![allow(clippy::unwrap_used)]

mod common;

use cm_render::compose::{
    Background, Bubble, BubbleShape, CameraLayer, Frame, Layout, PixelFormat, Ring, Scene,
    ScreenLayer,
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

/// A screen stand-in: smooth gradient with a dark title bar and a bright marker block at the
/// top left, so crops and flips are visible.
fn screen_img(w: u32, h: u32) -> RgbaImage {
    RgbaImage::from_fn(w, h, |x, y| {
        let r = (60 + 120 * x / w) as u8;
        let g = (90 + 100 * y / h) as u8;
        let mut p = [r, g, 200];
        if y < h / 9 {
            p = [34, 38, 46];
        }
        if x < w / 10 && y < h / 5 && y >= h / 9 {
            p = [255, 214, 10];
        }
        Rgba([p[0], p[1], p[2], 255])
    })
}

/// Camera stand-in: left half red, right half blue, a white dot in the middle.
fn camera_img(w: u32, h: u32) -> RgbaImage {
    RgbaImage::from_fn(w, h, |x, y| {
        let dx = x as i64 - i64::from(w) / 2;
        let dy = y as i64 - i64::from(h) / 2;
        if dx * dx + dy * dy < i64::from(h / 8) * i64::from(h / 8) {
            Rgba([255, 255, 255, 255])
        } else if x < w / 2 {
            Rgba([220, 40, 40, 255])
        } else {
            Rgba([40, 80, 220, 255])
        }
    })
}

fn px(img: &RgbaImage, x: u32, y: u32) -> [u8; 3] {
    let p = img.get_pixel(x, y).0;
    [p[0], p[1], p[2]]
}

fn near(a: [u8; 3], b: [u8; 3], tol: u8) -> bool {
    a.iter().zip(b.iter()).all(|(x, y)| x.abs_diff(*y) <= tol)
}

const GRAY: [u8; 3] = [200, 204, 210];

/// EDT-3.AC1: 8% padding, 12 px corners, shadow off: the screen is inset by 8% of the width.
#[test]
fn edt3_ac1_padding_and_radius() {
    let Some(mut c) = gpu() else { return };
    let s = screen_img(320, 180);
    let mut scene = Scene::new((640, 360), frame(&s));
    scene.background = Background::Color(GRAY);
    scene.padding_pct = 8.0;
    scene.radius_px = 12.0;
    let out = c.render(&scene).unwrap();
    assert_golden("compose_padding_radius", &out, LIMIT);

    // 8% of 640 = 51.2 -> 51 at the sides and 8% of 360 = 28.8 -> 29 above and below; the 16:9
    // screen fills the 538 x 302 area by height: 302 px tall, 537 wide, centered.
    let (x0, y0, w, h) = (52u32, 29u32, 537u32, 302u32);
    assert!(
        near(px(&out, 10, 10), GRAY, 1),
        "padding shows the background"
    );
    assert!(near(px(&out, 320, 20), GRAY, 1), "top padding");
    assert!(
        near(px(&out, 45, 180), GRAY, 1) && !near(px(&out, 60, 180), GRAY, 20),
        "the left inset is 8% of the width"
    );
    assert!(
        !near(px(&out, x0 + w / 2, y0 + h / 2), GRAY, 20),
        "the screen is in the middle"
    );
    // The very corner pixel of the tile is cut by the 12 px radius, the pixel 12 px in is not.
    assert!(
        near(px(&out, x0, y0), GRAY, 6),
        "rounded corner shows background"
    );
    assert!(
        !near(px(&out, x0 + 12, y0 + 12), GRAY, 20),
        "the screen is drawn at the inner corner"
    );
    assert!(
        !near(px(&out, x0 + 1, y0 + h / 2), GRAY, 20),
        "straight edge is covered"
    );
    assert!(
        near(px(&out, x0 - 2, y0 + h / 2), GRAY, 1),
        "outside the edge is background"
    );
}

/// EDT-3.AC2: a background image fills the frame (cover) and is not stretched.
#[test]
fn edt3_ac2_image_background_covers_without_distortion() {
    let Some(mut c) = gpu() else { return };
    // A 200x100 image (2:1): white everywhere except a black marker at image x 12..14.
    let bg = RgbaImage::from_fn(200, 100, |x, _| {
        if (12..14).contains(&x) {
            Rgba([0, 0, 0, 255])
        } else {
            Rgba([255, 255, 255, 255])
        }
    });
    // A 4:3 screen in a 16:9 frame leaves 40 px bars on each side, where the background shows.
    let s = screen_img(160, 120);
    let mut scene = Scene::new((320, 180), frame(&s));
    scene.background = Background::Image(frame(&bg));
    let out = c.render(&scene).unwrap();
    assert_golden("compose_bg_image", &out, LIMIT);
    // Cover: scale max(320/200, 180/100) = 1.8, the image is 360 wide and 20 px are cut from
    // each side. The marker (image x 12..14) lands on output x 1.6..5.2.
    assert!(
        near(px(&out, 3, 90), [0, 0, 0], 40),
        "marker visible, column 3"
    );
    assert!(
        near(px(&out, 3, 5), [0, 0, 0], 40),
        "marker spans the full height"
    );
    assert!(near(px(&out, 3, 175), [0, 0, 0], 40));
    assert!(
        near(px(&out, 12, 90), [255, 255, 255], 2),
        "no stretch: column 12 is white"
    );
    assert!(near(px(&out, 30, 90), [255, 255, 255], 2));
}

/// EDT-3.AC3: background off, no padding, matching aspect: the screen fills the frame exactly.
#[test]
fn edt3_ac3_no_background_fills_edge_to_edge() {
    let Some(mut c) = gpu() else { return };
    let s = screen_img(320, 180);
    let scene = Scene::new((320, 180), frame(&s));
    let out = c.render(&scene).unwrap();
    let bad = out
        .pixels()
        .zip(s.pixels())
        .filter(|(a, b)| a.0.iter().zip(b.0.iter()).any(|(x, y)| x.abs_diff(*y) > 2))
        .count();
    assert_eq!(bad, 0, "same size, no padding: pixel for pixel");
}

#[test]
fn background_gradient_runs_in_the_asked_direction() {
    let Some(mut c) = gpu() else { return };
    let s = screen_img(160, 90);
    let mut scene = Scene::new((320, 180), frame(&s));
    scene.padding_pct = 20.0;
    // 90 degrees: left to right.
    scene.background = Background::Gradient {
        from: [255, 0, 0],
        to: [0, 0, 255],
        angle_deg: 90.0,
    };
    let out = c.render(&scene).unwrap();
    assert_golden("compose_gradient", &out, LIMIT);
    let (l, r) = (px(&out, 2, 90), px(&out, 317, 90));
    assert!(l[0] > 230 && l[2] < 25, "left is `from`: {l:?}");
    assert!(r[2] > 230 && r[0] < 25, "right is `to`: {r:?}");
    // 180 degrees: top to bottom.
    scene.background = Background::Gradient {
        from: [255, 0, 0],
        to: [0, 0, 255],
        angle_deg: 180.0,
    };
    let out = c.render(&scene).unwrap();
    let (t, b) = (px(&out, 160, 1), px(&out, 160, 178));
    assert!(t[0] > 230 && b[2] > 230, "top {t:?} bottom {b:?}");
}

fn bubble_scene<'a>(
    out: (u32, u32),
    s: &'a RgbaImage,
    cam: &'a RgbaImage,
    bubble: Bubble,
) -> Scene<'a> {
    let mut scene = Scene::new(out, frame(s));
    scene.background = Background::Color(GRAY);
    scene.padding_pct = 4.0;
    scene.radius_px = 8.0;
    scene.shadow = true;
    scene.camera = Some(CameraLayer {
        frame: frame(cam),
        mirror: false,
    });
    scene.layout = Layout::ScreenBubble(bubble);
    scene
}

/// CAM-1/CAM-3: a circle bubble with a ring, bottom left; the camera is cropped, not stretched.
#[test]
fn cam3_circle_bubble_with_ring() {
    let Some(mut c) = gpu() else { return };
    let s = screen_img(320, 180);
    let cam = camera_img(160, 120);
    let bubble = Bubble {
        shape: BubbleShape::Circle,
        size: 0.4,
        center: (0.18, 0.78),
        ring: Some(Ring {
            width_px: 4.0,
            color: [255, 255, 255, 255],
        }),
    };
    let out = c
        .render(&bubble_scene((640, 360), &s, &cam, bubble))
        .unwrap();
    assert_golden("compose_bubble_circle", &out, LIMIT);
    // d = 0.4 * 360 = 144, center (115, 281) after clamping; margin 7 px keeps it inside.
    let (cx, cy) = (115, 281);
    assert!(
        near(px(&out, cx, cy), [255, 255, 255], 12),
        "white camera dot in the center"
    );
    assert!(
        near(px(&out, cx - 40, cy), [220, 40, 40], 12),
        "camera left half is red"
    );
    assert!(
        near(px(&out, cx + 40, cy), [40, 80, 220], 12),
        "camera right half is blue"
    );
    assert!(
        near(px(&out, cx, cy - 70), [255, 255, 255], 12),
        "ring at the top of the circle"
    );
    // The square's corner is outside the circle: the screen shows there, not the camera.
    let corner = px(&out, cx - 68, cy - 68);
    assert!(
        !near(corner, [220, 40, 40], 30),
        "outside the circle: {corner:?}"
    );
}

#[test]
fn cam3_rounded_bubble_with_ring_top_right() {
    let Some(mut c) = gpu() else { return };
    let s = screen_img(320, 180);
    let cam = camera_img(160, 120);
    let bubble = Bubble {
        shape: BubbleShape::Rounded,
        size: 0.3,
        center: (0.85, 0.2),
        ring: Some(Ring {
            width_px: 3.0,
            color: [255, 214, 10, 255],
        }),
    };
    let out = c
        .render(&bubble_scene((640, 360), &s, &cam, bubble))
        .unwrap();
    assert_golden("compose_bubble_rounded", &out, LIMIT);
}

/// CAM-3.AC3: half way through a smooth change from the bottom-left bubble to camera full, the
/// camera has grown past the bubble but does not cover the frame yet and the screen is still
/// there around it; at the ends it equals the plain layouts.
#[test]
fn cam3_ac3_smooth_change_draws_the_camera_between_its_places() {
    let Some(mut c) = gpu() else { return };
    let s = screen_img(320, 180);
    let cam = camera_img(160, 120);
    let start = Bubble {
        shape: BubbleShape::Circle,
        size: 0.3,
        center: (0.15, 0.8),
        ring: None,
    };
    let mut scene = bubble_scene((640, 360), &s, &cam, start);
    scene.layout = Layout::CameraFull;
    let from = Layout::ScreenBubble(start);
    let plain_from = c
        .render(&bubble_scene((640, 360), &s, &cam, start))
        .unwrap();
    let start_blend = {
        let p = cm_render::compose::plan_blend(&scene, from, 0.0).unwrap();
        c.render_with_plan(&scene, p).unwrap()
    };
    assert_eq!(
        start_blend.as_raw(),
        plain_from.as_raw(),
        "t = 0 is the old layout"
    );
    let mid = {
        let p = cm_render::compose::plan_blend(&scene, from, 0.5).unwrap();
        c.render_with_plan(&scene, p).unwrap()
    };
    assert_golden("compose_blend_bubble_to_full", &mid, LIMIT);
    // The marker block of the screen (top left) is still visible: the camera is not full yet.
    assert!(
        near(px(&mid, 50, 65), [255, 214, 10], 30),
        "the screen is still around the growing camera"
    );
    assert!(
        near(px(&mid, 100, 200), [220, 40, 40], 30),
        "the camera is under the old bubble's place and larger than it"
    );
    let end_blend = {
        let p = cm_render::compose::plan_blend(&scene, from, 1.0).unwrap();
        c.render_with_plan(&scene, p).unwrap()
    };
    let full = c.render(&scene).unwrap();
    assert_eq!(end_blend.as_raw(), full.as_raw(), "t = 1 is the new layout");
}

/// CAM-3.AC4: in a 9:16 export the bubble stays fully inside the frame even when it is asked to
/// sit past the edge.
#[test]
fn cam3_ac4_portrait_output_keeps_the_bubble_inside() {
    let Some(mut c) = gpu() else { return };
    let s = screen_img(320, 180);
    let cam = camera_img(160, 120);
    let bubble = Bubble {
        shape: BubbleShape::Circle,
        size: 0.3,
        center: (1.2, 1.1),
        ring: Some(Ring {
            width_px: 3.0,
            color: [255, 255, 255, 255],
        }),
    };
    let out = c
        .render(&bubble_scene((360, 640), &s, &cam, bubble))
        .unwrap();
    assert_golden("compose_portrait_bubble", &out, LIMIT);
    // d = 108; the center is clamped to (360-54-7, 640-54-7) = (299, 579).
    assert!(
        near(px(&out, 299, 579), [255, 255, 255], 12),
        "camera dot where it was clamped"
    );
    assert!(near(px(&out, 299 + 30, 579), [40, 80, 220], 12));
}

#[test]
fn cam3_mirror_flips_the_camera() {
    let Some(mut c) = gpu() else { return };
    let s = screen_img(320, 180);
    let cam = camera_img(160, 120);
    let mut scene = Scene::new((320, 180), frame(&s));
    scene.camera = Some(CameraLayer {
        frame: frame(&cam),
        mirror: true,
    });
    scene.layout = Layout::CameraFull;
    let out = c.render(&scene).unwrap();
    assert!(
        near(px(&out, 40, 90), [40, 80, 220], 12),
        "left shows the camera's right half"
    );
    assert!(near(px(&out, 280, 90), [220, 40, 40], 12));
}

/// CAM-3.AC2: camera full shows only the camera. Side by side: screen 2/3, camera 1/3.
#[test]
fn cam3_camera_full_and_side_by_side() {
    let Some(mut c) = gpu() else { return };
    let s = screen_img(320, 180);
    let cam = camera_img(160, 120);
    let mut scene = Scene::new((640, 360), frame(&s));
    scene.background = Background::Color(GRAY);
    scene.padding_pct = 3.0;
    scene.radius_px = 10.0;
    scene.camera = Some(CameraLayer {
        frame: frame(&cam),
        mirror: false,
    });
    scene.layout = Layout::CameraFull;
    let full = c.render(&scene).unwrap();
    assert_golden("compose_camera_full", &full, LIMIT);
    assert!(near(px(&full, 150, 180), [220, 40, 40], 12));
    assert!(near(px(&full, 500, 180), [40, 80, 220], 12));

    scene.layout = Layout::SideBySide;
    let sbs = c.render(&scene).unwrap();
    assert_golden("compose_side_by_side", &sbs, LIMIT);
}

/// A zoom crop shows only the asked part of the screen.
#[test]
fn edt4_crop_shows_the_marker_block_only() {
    let Some(mut c) = gpu() else { return };
    let s = screen_img(320, 180);
    let mut scene = Scene::new((200, 100), frame(&s));
    // The top-left 25% x 25% region contains the title bar and the yellow marker.
    scene.screen = Some(ScreenLayer {
        frame: frame(&s),
        crop: Some([0.0, 0.0, 0.25, 0.25]),
    });
    let out = c.render(&scene).unwrap();
    let yellow = out
        .pixels()
        .filter(|p| near([p[0], p[1], p[2]], [255, 214, 10], 6))
        .count();
    assert!(
        yellow > 200,
        "the zoomed crop shows the marker large: {yellow}"
    );
}

/// Annotation overlays are alpha-blended over the screen, inside the screen tile only.
#[test]
fn annotation_overlay_is_blended_over_the_screen() {
    let Some(mut c) = gpu() else { return };
    let s = screen_img(320, 180);
    let mut ov = RgbaImage::from_pixel(320, 180, Rgba([0, 0, 0, 0]));
    for y in 60..120 {
        for x in 100..220 {
            ov.put_pixel(x, y, Rgba([255, 0, 0, 255]));
        }
    }
    let mut scene = Scene::new((320, 180), frame(&s));
    scene.annotations = Some(frame(&ov));
    let out = c.render(&scene).unwrap();
    assert!(
        near(px(&out, 160, 90), [255, 0, 0], 2),
        "inside the overlay"
    );
    assert!(
        near(px(&out, 10, 90), px(&s, 10, 90), 2),
        "outside the overlay"
    );
}

/// BT.709 limited-range NV12 and BGRA inputs come out as the same colors as RGBA.
#[test]
fn pixel_formats_nv12_and_bgra() {
    let Some(mut c) = gpu() else { return };
    let (w, h) = (64u32, 32u32);
    // Pure red in BT.709 limited range: Y 63, U 102, V 240.
    let (y8, u8_, v8) = (63u8, 102u8, 240u8);
    let mut nv12 = vec![y8; (w * h) as usize];
    for _ in 0..(w * h / 2) / 2 {
        nv12.push(u8_);
        nv12.push(v8);
    }
    let scene_nv12 = Scene::new(
        (64, 32),
        Frame {
            width: w,
            height: h,
            format: PixelFormat::Nv12,
            data: &nv12,
        },
    );
    let out = c.render(&scene_nv12).unwrap();
    let got = px(&out, 32, 16);
    assert!(near(got, [255, 0, 0], 8), "NV12 red decoded as {got:?}");

    let bgra: Vec<u8> = (0..w * h).flat_map(|_| [10u8, 20, 200, 255]).collect();
    let scene_bgra = Scene::new(
        (64, 32),
        Frame {
            width: w,
            height: h,
            format: PixelFormat::Bgra8,
            data: &bgra,
        },
    );
    let out = c.render(&scene_bgra).unwrap();
    assert!(
        near(px(&out, 5, 5), [200, 20, 10], 1),
        "BGRA is swapped to RGB"
    );
}

#[test]
fn bad_input_is_an_error_not_a_crash() {
    let Some(mut c) = gpu() else { return };
    let short = [0u8; 10];
    let scene = Scene::new((64, 64), Frame::rgba(16, 16, &short));
    assert!(c.render(&scene).is_err());
    let s = screen_img(16, 16);
    let scene = Scene::new((0, 64), frame(&s));
    assert!(c.render(&scene).is_err());
}

/// The same scene twice gives the same bytes (texture reuse must not leak state).
#[test]
fn rendering_is_deterministic_across_frames_and_sizes() {
    let Some(mut c) = gpu() else { return };
    let s = screen_img(320, 180);
    let cam = camera_img(160, 120);
    let scene = bubble_scene(
        (320, 180),
        &s,
        &cam,
        Bubble {
            ring: Some(Ring {
                width_px: 2.0,
                color: [255, 255, 255, 255],
            }),
            ..Bubble::bottom_left()
        },
    );
    let a = c.render(&scene).unwrap();
    // A different size in between forces the output and readback buffers to be rebuilt.
    let other = Scene::new((100, 50), frame(&s));
    c.render(&other).unwrap();
    let b = c.render(&scene).unwrap();
    assert_eq!(a, b);
}

/// EXP-1: a 4K-class screen shown smaller is area-averaged, not point-sampled. Vertical stripes of
/// period 3 (one white column, two black) have an area average of 1/3 white; a single bilinear tap
/// at a 3x reduction never touches the white column and renders pure black.
#[test]
fn exp1_downscale_is_area_filtered() {
    let Some(mut c) = gpu() else { return };
    let src = RgbaImage::from_fn(3840, 2160, |x, _| {
        if x % 3 == 0 {
            Rgba([255, 255, 255, 255])
        } else {
            Rgba([0, 0, 0, 255])
        }
    });
    let scene = Scene::new((1280, 720), frame(&src));
    let out = c.render(&scene).unwrap();
    let mut worst = 0i32;
    for y in (8..out.height() - 8).step_by(37) {
        for x in 8..out.width() - 8 {
            let p = out.get_pixel(x, y).0;
            worst = worst.max((i32::from(p[0]) - 85).abs());
        }
    }
    assert!(
        worst <= 12,
        "stripes alias after the reduction: off by {worst}"
    );
}
