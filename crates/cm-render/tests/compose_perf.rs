//! Performance smoke for the GPU compositor: ms per composed frame (upload of the screen and
//! camera frames, the draw, and the readback into a CPU image) at 1080p and 4K.
//!
//! Numbers are printed, not asserted tightly: run `cargo test -p cm-render --release --test
//! compose_perf -- --nocapture` and read them. The only assertion is a sanity bound well above
//! any healthy GPU. Without an adapter the test prints `SKIPPED`.
#![allow(clippy::unwrap_used)]

use std::time::Instant;

use cm_render::compose::{
    Background, Bubble, BubbleShape, CameraLayer, Frame, Layout, PixelFormat, Ring, Scene,
};
use cm_render::Compositor;

fn solid(w: u32, h: u32, seed: u8) -> Vec<u8> {
    (0..w * h)
        .flat_map(|i| {
            let v = (i as u8).wrapping_add(seed);
            [v, v.wrapping_mul(3), 255 - v, 255]
        })
        .collect()
}

fn measure(
    c: &mut Compositor,
    label: &str,
    out: (u32, u32),
    screen: (u32, u32),
    format: PixelFormat,
) {
    let screen_data = match format {
        PixelFormat::Nv12 => vec![128u8; (screen.0 * screen.1 * 3 / 2) as usize],
        _ => solid(screen.0, screen.1, 1),
    };
    let cam_data = solid(1280, 720, 7);
    let scene = Scene {
        output: out,
        background: Background::Gradient {
            from: [30, 20, 60],
            to: [120, 60, 200],
            angle_deg: 135.0,
        },
        padding_pct: 6.0,
        radius_px: 12.0,
        shadow: true,
        screen: Some(cm_render::compose::ScreenLayer::new(Frame {
            width: screen.0,
            height: screen.1,
            format,
            data: &screen_data,
        })),
        camera: Some(CameraLayer {
            frame: Frame::rgba(1280, 720, &cam_data),
            mirror: false,
        }),
        layout: Layout::ScreenBubble(Bubble {
            shape: BubbleShape::Circle,
            size: 0.2,
            center: (0.0, 1.0),
            ring: Some(Ring {
                width_px: 4.0,
                color: [255, 255, 255, 255],
            }),
        }),
        annotations: None,
        cursor: None,
        captions: None,
    };
    for _ in 0..3 {
        c.render(&scene).unwrap();
    }
    let n = 30;
    let t = Instant::now();
    for _ in 0..n {
        c.render(&scene).unwrap();
    }
    let ms = t.elapsed().as_secs_f64() * 1000.0 / f64::from(n);
    println!(
        "compose {label}: {ms:.2} ms/frame ({out:?} out, {screen:?} {format:?} screen + 1280x720 camera, adapter {})",
        c.adapter_name()
    );
    // The bound is for a real GPU. A software renderer (the Windows "Basic Render Driver"/WARP of a
    // hosted runner, llvmpipe, SwiftShader) is slow by nature, so only its number is printed.
    let name = c.adapter_name().to_lowercase();
    let software = [
        "basic render",
        "warp",
        "llvmpipe",
        "lavapipe",
        "swiftshader",
        "software",
    ]
    .iter()
    .any(|k| name.contains(k));
    if software {
        println!("(software renderer: the {label} bound is not checked)");
        return;
    }
    assert!(
        ms < 500.0,
        "{label}: {ms} ms/frame is far outside any healthy GPU"
    );
}

#[test]
fn compose_ms_per_frame_1080p_and_4k() {
    let mut c = match Compositor::new() {
        Ok(c) => c,
        Err(e) => {
            println!("SKIPPED: no GPU compositor ({e})");
            return;
        }
    };
    measure(
        &mut c,
        "1080p out, 1080p RGBA",
        (1920, 1080),
        (1920, 1080),
        PixelFormat::Rgba8,
    );
    measure(
        &mut c,
        "1080p out, 4K RGBA",
        (1920, 1080),
        (3840, 2160),
        PixelFormat::Rgba8,
    );
    measure(
        &mut c,
        "4K out, 4K RGBA",
        (3840, 2160),
        (3840, 2160),
        PixelFormat::Rgba8,
    );
    measure(
        &mut c,
        "4K out, 4K NV12",
        (3840, 2160),
        (3840, 2160),
        PixelFormat::Nv12,
    );
}
