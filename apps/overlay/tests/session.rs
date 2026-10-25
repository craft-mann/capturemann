//! The overlay's session against a real daemon (fake screen) on a Unix socket: the freeze
//! leaves nothing in the library, the final capture is the frozen pixels, one screen capture.

#![cfg(unix)]
#![allow(clippy::unwrap_used)]

use cm_capture::fake::{pattern_pixel, FakeBackend};
use cm_capture::Capturer;
use cm_core::{PhysicalPoint, PhysicalRect, PhysicalSize};
use cm_daemon::{Daemon, Endpoint, IpcServer};
use cm_library::Library;
use cm_overlay::picker::{Mode, Outcome};
use cm_overlay::session::{commit, freeze_all, release_all, stream, Choice, Emit, SessionEvent};
use cm_overlay::trace::Tracer;
use std::sync::{Arc, Mutex};

fn rect(x: i32, y: i32, w: u32, h: u32) -> PhysicalRect {
    PhysicalRect {
        origin: PhysicalPoint { x, y },
        size: PhysicalSize {
            width: w,
            height: h,
        },
    }
}

/// CAP-1.AC1/AC2: freeze every display without making a bundle, then a region is exactly the
/// frozen crop (`pixel_identical`), the display choice is the whole frame, and releasing
/// frees the frames. No "picker frame" bundles are left in the library.
#[tokio::test]
async fn cap1_freeze_leaves_no_bundles_and_region_is_the_frozen_crop() {
    let dir = tempfile::tempdir().unwrap();
    let lib = Library::open_or_create(&dir.path().join("lib")).unwrap();
    let backend = FakeBackend::new()
        .with_display(1, 0, 0, 400, 300, 2.0)
        .with_display(2, 400, 0, 200, 100, 1.0);
    let daemon = Daemon::builder(Capturer::new(backend).with_own_pid(None), lib).build();
    let endpoint = Endpoint::Path(dir.path().join("d.sock"));
    let server = IpcServer::bind(&endpoint).unwrap();
    let d2 = daemon.clone();
    tokio::spawn(async move { server.run(d2).await });

    let (targets, frozen) = freeze_all(&endpoint).await.unwrap();
    assert_eq!((targets.displays.len(), frozen.len()), (2, 2));
    assert_eq!(
        frozen[0].frame.pixel(33, 21),
        Some({
            let p = pattern_pixel(33, 21);
            (u32::from(p[0]) << 16) | (u32::from(p[1]) << 8) | u32::from(p[2])
        })
    );
    let listed = daemon
        .execute_method(
            cm_core::Actor::User,
            0,
            "list_library",
            &serde_json::json!({}),
        )
        .await
        .unwrap();
    assert_eq!(
        listed["captures"].as_array().unwrap().len(),
        0,
        "freeze makes no bundle"
    );

    let region = commit(
        &endpoint,
        Mode::Region,
        Choice {
            display_id: 1,
            outcome: Outcome::Region {
                rect: rect(100, 50, 200, 120),
            },
        },
        &frozen,
    )
    .await
    .unwrap();
    assert_eq!(region["pixel_identical"], true);
    assert_eq!(region["width"], 200);

    let whole = commit(
        &endpoint,
        Mode::Display,
        Choice {
            display_id: 2,
            outcome: Outcome::Display,
        },
        &frozen,
    )
    .await
    .unwrap();
    assert_eq!(
        (whole["width"].as_u64(), whole["height"].as_u64()),
        (Some(200), Some(100))
    );

    // Exactly the two chosen captures are in the library, none of them a picker frame.
    let listed = daemon
        .execute_method(
            cm_core::Actor::User,
            0,
            "list_library",
            &serde_json::json!({}),
        )
        .await
        .unwrap();
    assert_eq!(listed["captures"].as_array().unwrap().len(), 2);

    release_all(&endpoint, &frozen).await;
    let gone = commit(
        &endpoint,
        Mode::Display,
        Choice {
            display_id: 1,
            outcome: Outcome::Display,
        },
        &frozen,
    )
    .await
    .unwrap_err();
    assert_eq!(gone.code, "not_found");
}

/// CAP-1.AC1/AC5: the streamed session reports the displays before any frame exists (so windows
/// can open while the freezes run), then each frozen display, and the window list last on its
/// own. The displays-only listing is what lets it start freezing without window enumeration.
#[tokio::test]
async fn cap1_ac1_stream_reports_displays_first_then_frames_then_windows() {
    let dir = tempfile::tempdir().unwrap();
    let lib = Library::open_or_create(&dir.path().join("lib")).unwrap();
    let backend = FakeBackend::new()
        .with_display(1, 0, 0, 400, 300, 2.0)
        .with_display(2, 400, 0, 200, 100, 1.0)
        .with_window(10, "Editor", "Code", 77, 1, (20, 10, 100, 80), false);
    let daemon = Daemon::builder(Capturer::new(backend).with_own_pid(None), lib).build();
    let endpoint = Endpoint::Path(dir.path().join("d.sock"));
    let server = IpcServer::bind(&endpoint).unwrap();
    tokio::spawn(async move { server.run(daemon).await });

    let log: Arc<Mutex<Vec<String>>> = Arc::default();
    let emit: Emit = {
        let log = log.clone();
        Arc::new(move |ev| {
            let line = match ev {
                SessionEvent::Displays(d) => format!("displays:{}", d.len()),
                SessionEvent::Frozen(f) => format!("frozen:{}", f.display.id),
                SessionEvent::Windows(w) => format!("windows:{}", w.len()),
                SessionEvent::Failed(e) => format!("failed:{}", e.code),
            };
            log.lock().unwrap().push(line);
        })
    };
    let tracer = Tracer::new(std::time::Instant::now(), false);
    let frozen = stream(&endpoint, &tracer, &emit).await.unwrap();
    assert_eq!(frozen.len(), 2);
    // The window task is independent of the freezes: give it a moment to report.
    for _ in 0..200 {
        if log
            .lock()
            .unwrap()
            .iter()
            .any(|l| l.starts_with("windows:"))
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let log = log.lock().unwrap().clone();
    assert_eq!(
        log[0], "displays:2",
        "displays come before any frame: {log:?}"
    );
    for want in ["frozen:1", "frozen:2", "windows:1"] {
        assert!(log.iter().any(|l| l == want), "{want} missing in {log:?}");
    }
    let s = tracer.summary();
    assert!(s["displays_listed"].as_f64() < s["frozen"].as_f64());
    release_all(&endpoint, &frozen).await;
}

/// With no daemon the stream fails before anything is emitted, with the shared exit code 5.
#[tokio::test]
async fn cap1_ac1_stream_without_a_reachable_daemon_is_daemon_unavailable() {
    let dir = tempfile::tempdir().unwrap();
    let endpoint = Endpoint::Path(dir.path().join("nobody.sock"));
    let emit: Emit = Arc::new(|_| {});
    let tracer = Tracer::new(std::time::Instant::now(), false);
    let Err(e) = stream(&endpoint, &tracer, &emit).await else {
        panic!("no daemon should fail");
    };
    assert_eq!(e.code, "daemon_unavailable");
}
