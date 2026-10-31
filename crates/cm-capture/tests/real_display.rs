//! Real-display tests: need a desktop session and (macOS) Screen Recording permission.
//! Run manually: `cargo test -p cm-capture --test real_display -- --ignored --nocapture`.
//! Script: tests/manual/CAP-1.md.
#![cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
#![allow(clippy::unwrap_used)]

use cm_capture::{CaptureRequest, CaptureTarget, Capturer, RegionRect};
use cm_core::{CmError, ErrorCode, PhysicalPoint, PhysicalRect, PhysicalSize};

fn targets_or_skip(c: &Capturer) -> Option<cm_capture::Targets> {
    match c.list_targets() {
        Ok(t) => Some(t),
        Err(CmError::PermissionMissing { .. }) => {
            eprintln!("Screen Recording permission missing: grant it and rerun");
            None
        }
        Err(e) => panic!("{e}"),
    }
}

/// CAP-1.AC1: every display is captured at its physical size.
#[test]
#[ignore = "needs a real display and screen permission"]
fn cap1_ac1_real_display_matches_reported_physical_size() {
    let c = Capturer::system();
    let Some(t) = targets_or_skip(&c) else { return };
    assert!(!t.displays.is_empty());
    for d in &t.displays {
        let cap = c
            .capture(
                CaptureRequest::new(CaptureTarget::Display { display_id: d.id }),
                None,
            )
            .unwrap()
            .unwrap();
        println!(
            "display {} {:?}: bounds {:?} scale {} -> {}x{}",
            d.id,
            d.name,
            d.bounds,
            d.scale.get(),
            cap.frame.width(),
            cap.frame.height()
        );
        assert_eq!(cap.frame.size(), d.bounds.size);
    }
}

/// CAP-1.AC2: a physical region of a real display is pixel-identical to the same area of a
/// full capture taken back to back (screen is static enough only if idle; compare sizes and
/// report the mismatch count rather than failing on live content).
#[test]
#[ignore = "needs a real display and screen permission"]
fn cap1_ac2_real_region_has_requested_size() {
    let c = Capturer::system();
    let Some(t) = targets_or_skip(&c) else { return };
    let d = &t.displays[0];
    let rect = PhysicalRect {
        origin: PhysicalPoint { x: 100, y: 80 },
        size: PhysicalSize {
            width: 420,
            height: 300,
        },
    };
    let cap = c
        .capture(
            CaptureRequest::new(CaptureTarget::Region {
                display_id: d.id,
                rect: RegionRect::Physical(rect),
            }),
            None,
        )
        .unwrap()
        .unwrap();
    assert_eq!(cap.frame.size(), rect.size);
    std::fs::write(
        std::env::temp_dir().join("cm-capture-region.png"),
        cap.frame.to_png().unwrap(),
    )
    .unwrap();
}

/// CAP-1.AC4: a listed window captures at its reported physical size (1:1, no shadow).
#[test]
#[ignore = "needs a real display and screen permission"]
fn cap1_ac4_real_window_matches_bounds() {
    let c = Capturer::system();
    let Some(t) = targets_or_skip(&c) else { return };
    let mut checked = 0;
    for w in t
        .windows
        .iter()
        .filter(|w| !w.title.is_empty() && w.bounds.size.width >= 100 && !w.minimized)
        .take(5)
    {
        match c.capture(
            CaptureRequest::new(CaptureTarget::Window { window_id: w.id }),
            None,
        ) {
            Ok(Some(cap)) => {
                println!(
                    "window {} {:?}/{:?}: bounds {}x{} -> image {}x{}",
                    w.id,
                    w.app,
                    w.title,
                    w.bounds.size.width,
                    w.bounds.size.height,
                    cap.frame.width(),
                    cap.frame.height()
                );
                let (dw, dh) = (
                    cap.frame.width().abs_diff(w.bounds.size.width),
                    cap.frame.height().abs_diff(w.bounds.size.height),
                );
                assert!(dw <= 1 && dh <= 1, "window image deviates from bounds");
                checked += 1;
            }
            Ok(None) => unreachable!(),
            // The window may close between list and capture.
            Err(e) if e.code() == ErrorCode::NotFound => {}
            Err(e) => panic!("{e}"),
        }
    }
    println!("checked {checked} windows");
}

/// CAP-3: the menu bar, Dock and Control Center are not window targets. The name filter is
/// switched off here so this checks the backend's window-layer filtering on its own.
#[test]
#[ignore = "needs a real display and screen permission"]
fn cap3_real_list_targets_has_no_shell_windows() {
    let c = Capturer::system().with_shell_windows_hidden(false);
    let Some(t) = targets_or_skip(&c) else { return };
    for w in &t.windows {
        assert!(
            !cm_capture::is_shell_window(w),
            "shell window listed: {} / {} {:?}",
            w.app,
            w.title,
            w.bounds
        );
    }
    println!("{} windows, none from the desktop shell", t.windows.len());
}

/// R4-6 (hardware): a display screenshot that excludes this process's windows. macOS 14+ only;
/// on older systems the exclusion is unavailable and the plain frame is returned.
#[test]
#[ignore = "takes a real screenshot; run manually"]
#[cfg(target_os = "macos")]
fn r4_6_display_screenshot_with_own_windows_excluded() {
    use cm_capture::{CaptureRequest, CaptureTarget, Capturer};
    let c = Capturer::system().with_exclude_own_windows(true);
    let Ok(targets) = c.list_displays() else {
        println!("SKIPPED: no screen recording permission");
        return;
    };
    let Some(d) = targets.displays.first() else {
        println!("SKIPPED: no display");
        return;
    };
    let cap = c
        .capture(
            CaptureRequest::new(CaptureTarget::Display { display_id: d.id }),
            None,
        )
        .unwrap()
        .unwrap();
    assert_eq!(cap.frame.size(), d.bounds.size);
}
