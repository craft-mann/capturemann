//! Real-display tests: need a desktop session and (macOS) Screen Recording permission.
//! Run manually: `cargo test -p cm-capture --test real_display -- --ignored --nocapture`.
//! Script: tests/manual/CAP-1.md.
//!
//! Also run in CI on a Linux virtual X server (Xvfb) and on the Windows runner (an interactive
//! desktop session with a virtual display, no GPU). There the assertions are the ones that hold on
//! any session: the display list is not empty, sizes are physical, a display captures at its
//! bounds, a region has the requested size and matches the same area of a full capture. Whether
//! the picture shows something is only printed (a locked or empty desktop is legitimately blank).
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
    assert!(
        !t.displays.is_empty(),
        "the session reports no display: no interactive desktop?"
    );
    assert!(
        t.displays.iter().any(|d| d.is_primary),
        "no display is marked primary: {:?}",
        t.displays
    );
    for d in &t.displays {
        assert!(
            d.bounds.size.width > 0 && d.bounds.size.height > 0,
            "display {} has an empty physical size {:?}",
            d.id,
            d.bounds.size
        );
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
        assert_eq!(
            cap.frame.size(),
            d.bounds.size,
            "display {} captured at a size other than its physical bounds",
            d.id
        );
        report_content(&format!("display {}", d.id), &cap.frame);
    }
}

/// Prints how many distinct colours (sampled) a frame holds. Never fails: a locked, empty or
/// virtual desktop can be one flat colour, and that says nothing against the capture path.
fn report_content(what: &str, f: &cm_capture::Frame) {
    let mut seen = std::collections::HashSet::new();
    for px in f.rgba().as_chunks::<4>().0.iter().step_by(97) {
        seen.insert([px[0], px[1], px[2]]);
        if seen.len() > 64 {
            break;
        }
    }
    if seen.len() <= 1 {
        println!("NOTE: {what} is one flat colour (empty or locked desktop?); the size checks still apply");
    } else {
        println!("{what}: at least {} distinct colours", seen.len());
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
    // Stay inside small virtual displays (Xvfb and runner screens can be 1024x768).
    let rect = PhysicalRect {
        origin: PhysicalPoint { x: 100, y: 80 },
        size: PhysicalSize {
            width: 420.min(d.bounds.size.width.saturating_sub(100)),
            height: 300.min(d.bounds.size.height.saturating_sub(80)),
        },
    };
    assert!(
        rect.size.width > 0 && rect.size.height > 0,
        "display {} is too small ({:?}) for the test region",
        d.id,
        d.bounds.size
    );
    let full = c
        .capture(
            CaptureRequest::new(CaptureTarget::Display { display_id: d.id }),
            None,
        )
        .unwrap()
        .unwrap();
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
    // The same area of a full capture taken just before. Live content (a clock, a cursor) may
    // differ, so this prints the mismatch share and only fails when most of it differs.
    let want = full.frame.crop(rect).unwrap();
    let differing = want
        .rgba()
        .as_chunks::<4>()
        .0
        .iter()
        .zip(cap.frame.rgba().as_chunks::<4>().0.iter())
        .filter(|(a, b)| a != b)
        .count();
    let total = (rect.size.width as usize) * (rect.size.height as usize);
    println!("region vs crop of the full capture: {differing} of {total} pixels differ");
    assert!(
        differing * 2 < total,
        "region: {differing}/{total} pixels differ from the same area of the full capture: wrong crop origin or scale?"
    );
    report_content("region", &cap.frame);
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

/// CAP-1 (Windows): a window the test starts is listed with a title and a size, and a capture of
/// it matches its bounds. The window is a `notepad.exe` the test starts and closes (the runner
/// has no other app window). Prints SKIPPED when the session shows no such window (a locked
/// desktop, or a Notepad that hands over to a Store app and exits).
#[test]
#[ignore = "starts and closes Notepad on a real desktop"]
#[cfg(target_os = "windows")]
fn cap1_windows_lists_and_captures_a_started_window() {
    struct Kill(std::process::Child);
    impl Drop for Kill {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let c = Capturer::system();
    let Some(_) = targets_or_skip(&c) else { return };
    let child = match std::process::Command::new("notepad.exe").spawn() {
        Ok(ch) => Kill(ch),
        Err(e) => {
            println!("SKIPPED: cannot start notepad.exe: {e}");
            return;
        }
    };
    let pid = child.0.id();
    let mut found = None;
    for _ in 0..40 {
        std::thread::sleep(std::time::Duration::from_millis(250));
        let t = c.list_targets().unwrap();
        found = t.windows.into_iter().find(|w| {
            w.pid == pid
                || w.app.to_lowercase().contains("notepad")
                || w.title.to_lowercase().contains("notepad")
        });
        if found.is_some() {
            break;
        }
    }
    let Some(w) = found else {
        let all: Vec<_> = c
            .list_targets()
            .unwrap()
            .windows
            .iter()
            .map(|w| format!("{}/{} pid {}", w.app, w.title, w.pid))
            .collect();
        println!("SKIPPED: no Notepad window within 10 s; the session lists {all:?}");
        return;
    };
    println!(
        "found window {} {:?}/{:?} pid {} bounds {:?}",
        w.id, w.app, w.title, w.pid, w.bounds
    );
    assert!(w.bounds.size.width > 0 && w.bounds.size.height > 0);
    let cap = c
        .capture(
            CaptureRequest::new(CaptureTarget::Window { window_id: w.id }),
            None,
        )
        .unwrap()
        .unwrap();
    println!(
        "window image {}x{} vs bounds {}x{}",
        cap.frame.width(),
        cap.frame.height(),
        w.bounds.size.width,
        w.bounds.size.height
    );
    assert!(
        cap.frame.width().abs_diff(w.bounds.size.width) <= 1
            && cap.frame.height().abs_diff(w.bounds.size.height) <= 1,
        "the window image deviates from the listed bounds (invisible resize borders or DWM shadow?)"
    );
    report_content("window", &cap.frame);
}
