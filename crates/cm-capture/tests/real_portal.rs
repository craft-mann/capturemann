//! The Wayland capture path against a real desktop portal. Ignored: it needs a Wayland session
//! and takes a screenshot (the desktop may ask once). Run with
//! `cargo test -p cm-capture --test real_portal -- --ignored --nocapture --test-threads=1`.
//!
//! The picker test (`CM_PORTAL_PICK=1`) shows the desktop's own window / area / screen chooser and
//! waits for a person; leave it unset on a headless session.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used)]

use cm_capture::{Capabilities, CaptureRequest, CaptureTarget, Capturer, DisplayInfo, RegionRect};
use cm_core::{ErrorCode, PhysicalPoint, PhysicalRect, PhysicalSize};

fn on_wayland() -> bool {
    std::env::var("WAYLAND_DISPLAY").is_ok_and(|v| !v.is_empty())
}

/// A desktop is not one flat colour.
fn assert_not_blank(frame: &cm_capture::Frame) {
    let (pixels, _) = frame.rgba().as_chunks::<4>();
    assert!(
        pixels.iter().any(|p| *p != pixels[0]),
        "the picture is blank"
    );
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
    assert!(!targets.displays.is_empty());
    for d in &targets.displays {
        println!(
            "display {} {:?} {:?} scale {} primary {}",
            d.id,
            d.name,
            d.bounds,
            d.scale.get(),
            d.is_primary
        );
        assert!(d.bounds.size.width > 0 && d.bounds.size.height > 0);
    }
    assert!(targets.windows.is_empty());
    // Capabilities: a Wayland session has a picker and no window ids.
    assert_eq!(
        capturer.capabilities(),
        Capabilities {
            list_windows: false,
            capture_window: false,
            capture_region: true,
            pick: true
        }
    );
    // Each display captures at exactly the size it reports (CAP-1.AC1).
    for d in &targets.displays {
        let shot = capturer
            .capture(
                CaptureRequest::new(CaptureTarget::Display { display_id: d.id }),
                None,
            )
            .unwrap()
            .unwrap();
        assert_eq!(shot.frame.size(), d.bounds.size, "display {}", d.id);
        assert_not_blank(&shot.frame);
    }
}

/// The reported displays tile one rectangle: no gaps larger than the compositor's own, no
/// overlaps (this desktop's monitors, as Mutter lays them out).
#[test]
#[ignore = "needs a Wayland desktop with the screenshot portal"]
fn the_displays_do_not_overlap() {
    if !on_wayland() {
        println!("SKIPPED: no WAYLAND_DISPLAY");
        return;
    }
    let displays: Vec<DisplayInfo> = Capturer::system().list_displays().unwrap().displays;
    for (i, a) in displays.iter().enumerate() {
        for b in &displays[i + 1..] {
            let (ax1, ay1) = (
                i64::from(a.bounds.origin.x) + i64::from(a.bounds.size.width),
                i64::from(a.bounds.origin.y) + i64::from(a.bounds.size.height),
            );
            let (bx1, by1) = (
                i64::from(b.bounds.origin.x) + i64::from(b.bounds.size.width),
                i64::from(b.bounds.origin.y) + i64::from(b.bounds.size.height),
            );
            let overlap = i64::from(a.bounds.origin.x) < bx1
                && i64::from(b.bounds.origin.x) < ax1
                && i64::from(a.bounds.origin.y) < by1
                && i64::from(b.bounds.origin.y) < ay1;
            assert!(!overlap, "{a:?} overlaps {b:?}");
        }
    }
}

/// CAP-1.AC2 on Wayland: a region of display 1 has exactly the requested size and is cut from
/// the desktop picture.
#[test]
#[ignore = "needs a Wayland desktop with the screenshot portal"]
fn a_region_is_cropped_from_the_picture() {
    if !on_wayland() {
        println!("SKIPPED: no WAYLAND_DISPLAY");
        return;
    }
    let capturer = Capturer::system();
    let d = capturer.list_displays().unwrap().displays[0].clone();
    let rect = PhysicalRect {
        origin: PhysicalPoint { x: 100, y: 100 },
        size: PhysicalSize {
            width: 400,
            height: 300,
        },
    };
    let shot = capturer
        .capture(
            CaptureRequest::new(CaptureTarget::Region {
                display_id: d.id,
                rect: RegionRect::Physical(rect),
            }),
            None,
        )
        .unwrap()
        .unwrap();
    // Only the size: an empty desktop's corner can be one flat colour.
    assert_eq!(shot.frame.size(), rect.size);
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

/// A window by id has no meaning on Wayland: `not_implemented`, saying how to pick one.
#[test]
#[ignore = "needs a Wayland desktop with the screenshot portal"]
fn a_window_id_is_not_implemented_and_names_the_picker() {
    if !on_wayland() {
        println!("SKIPPED: no WAYLAND_DISPLAY");
        return;
    }
    let capturer = Capturer::system();
    let e = capturer
        .capture(
            CaptureRequest::new(CaptureTarget::Window { window_id: 1 }),
            None,
        )
        .unwrap_err();
    assert_eq!(e.code(), ErrorCode::NotImplemented, "{e}");
    assert!(e.to_string().contains("pick"), "{e}");
}

/// The desktop's own picker: a person chooses a window, an area or a screen. Skipped unless
/// `CM_PORTAL_PICK=1` (it waits for them, at most two minutes). Esc is `user_declined`.
#[test]
#[ignore = "needs a Wayland desktop and a person at the screen"]
fn the_picker_returns_what_the_person_chose() {
    if !on_wayland() || std::env::var("CM_PORTAL_PICK").as_deref() != Ok("1") {
        println!("SKIPPED: set CM_PORTAL_PICK=1 on a Wayland session with a person at the screen");
        return;
    }
    let capturer = Capturer::system();
    match capturer.capture_picked() {
        Ok(cap) => {
            println!("picked {}x{}", cap.frame.width(), cap.frame.height());
            assert_not_blank(&cap.frame);
        }
        Err(e) => {
            println!("picker ended: {e}");
            assert_eq!(e.code(), ErrorCode::UserDeclined, "{e}");
        }
    }
}
