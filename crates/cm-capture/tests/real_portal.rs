//! The Wayland capture path against a real desktop portal. Ignored: it needs a Wayland session
//! and takes a screenshot (the desktop may ask once). Run with
//! `cargo test -p cm-capture --test real_portal -- --ignored --nocapture`.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used)]

use cm_capture::{CaptureRequest, CaptureTarget, Capturer};

fn on_wayland() -> bool {
    std::env::var("WAYLAND_DISPLAY").is_ok_and(|v| !v.is_empty())
}

#[test]
#[ignore = "needs a Wayland desktop with the screenshot portal"]
fn the_portal_returns_a_desktop_sized_picture() {
    if !on_wayland() {
        println!("SKIPPED: no WAYLAND_DISPLAY");
        return;
    }
    let capturer = Capturer::system();
    let targets = capturer.list_displays().unwrap();
    assert_eq!(targets.displays.len(), 1);
    let d = &targets.displays[0];
    println!("display {} {:?}", d.name, d.bounds.size);
    assert!(d.bounds.size.width > 0 && d.bounds.size.height > 0);
    assert!(targets.windows.is_empty());
    let shot = capturer
        .capture(
            CaptureRequest::new(CaptureTarget::Display { display_id: d.id }),
            None,
        )
        .unwrap()
        .unwrap();
    let frame = shot.frame;
    assert_eq!(frame.width(), d.bounds.size.width);
    assert_eq!(frame.height(), d.bounds.size.height);
    // A desktop is not one flat colour.
    let (pixels, _) = frame.rgba().as_chunks::<4>();
    assert!(
        pixels.iter().any(|p| *p != pixels[0]),
        "the picture is blank"
    );
}

#[test]
#[ignore = "needs a Wayland desktop with the screenshot portal"]
fn two_captures_in_a_row_both_finish() {
    if !on_wayland() {
        println!("SKIPPED: no WAYLAND_DISPLAY");
        return;
    }
    let capturer = Capturer::system();
    for _ in 0..3 {
        let shot = capturer
            .capture(
                CaptureRequest::new(CaptureTarget::Display { display_id: 1 }),
                None,
            )
            .unwrap()
            .unwrap();
        assert!(shot.frame.width() > 0);
    }
}
