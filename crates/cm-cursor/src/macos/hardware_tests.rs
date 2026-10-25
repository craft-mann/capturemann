//! Real-hardware tests, all `#[ignore]`. See tests/manual/REC-7.md.
//!
//! `cargo test -p cm-cursor -- --ignored --nocapture --test-threads=1`
//!
//! The injection test additionally needs `CM_ALLOW_INPUT_INJECTION=1`, Accessibility (post event
//! access) and Input Monitoring for the terminal; it briefly moves the real cursor.

use std::time::{Duration, Instant};

use cm_core::{HostClock, SessionClock, SystemHostClock, TrackContext, TrackRecorder};

use super::{sys, MacSource};
use crate::recorder::{CursorConfig, CursorRecorder};
use crate::schema::{Button, CursorEvent};
use crate::source::{CursorSource, RawKind, RawSender};
use crate::Availability;
use crate::CursorLog;

fn stats(name: &str, v: &mut [f64]) {
    if v.is_empty() {
        println!("{name}: no samples");
        return;
    }
    v.sort_by(|a, b| a.total_cmp(b));
    let p = |q: f64| v[((v.len() - 1) as f64 * q) as usize];
    let mean = v.iter().sum::<f64>() / v.len() as f64;
    println!(
        "{name}: n={} min {:.3} mean {:.3} p50 {:.3} p95 {:.3} max {:.3} ms",
        v.len(),
        v[0],
        mean,
        p(0.5),
        p(0.95),
        v[v.len() - 1]
    );
}

/// Records 3 s with the real source and the user's real (idle or moving) mouse. No injection.
#[test]
#[ignore = "needs a display session; optional Input Monitoring"]
fn rec7_real_source_header_images_and_cpu() {
    let dir = tempfile::tempdir().unwrap();
    let target = super::main_display_target(None);
    println!("target: {target:?}");
    let mut rec = CursorRecorder::new(CursorConfig::new(target));
    let clock = SessionClock::start();
    let cpu0 = sys::inject::cpu_seconds();
    let t0 = Instant::now();
    rec.start(TrackContext {
        dir: dir.path().to_path_buf(),
        clock: clock.clone(),
    })
    .unwrap();
    let status = rec.status().unwrap();
    println!("status: {status:?}");
    std::thread::sleep(Duration::from_secs(3));
    let info = rec.stop(clock.now()).unwrap();
    let wall = t0.elapsed().as_secs_f64();
    let cpu = sys::inject::cpu_seconds() - cpu0;
    println!(
        "CPU {:.3} s over {wall:.1} s = {:.2}% of one core",
        cpu,
        cpu / wall * 100.0
    );
    let log = CursorLog::read(&dir.path().join("cursor.jsonl")).unwrap();
    println!("stats {:?}", info.stats);
    println!("events {}, truncated {}", log.events.len(), log.truncated);
    let images: Vec<_> = log
        .events
        .iter()
        .filter(|e| matches!(e, CursorEvent::Cursor { .. }))
        .collect();
    println!("cursor image events: {images:?}");
    assert!(
        !images.is_empty(),
        "the current system cursor should be reported at start"
    );
    let idx = CursorLog::image_index(dir.path()).unwrap();
    for (id, e) in &idx.images {
        println!(
            "{id}: {}x{} px hotspot {:?} scale {}",
            e.size[0], e.size[1], e.hotspot, e.scale
        );
        assert!(dir.path().join("cursors").join(&e.file).exists());
    }
    assert!(
        log.events
            .iter()
            .any(|e| matches!(e, CursorEvent::Move { .. })),
        "initial position anchor"
    );
    if status.clicks == crate::Availability::Unavailable {
        assert_eq!(log.header.clicks, Availability::Unavailable);
    }
    // tempdir is deleted on drop: it contains the user's cursor positions.
}

/// The denied path on the real OS: access probe forced to "denied", real position polling.
#[test]
#[ignore = "needs a display session"]
fn rec7_ac4_real_polling_fallback() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = CursorConfig::new(super::main_display_target(None));
    let src = MacSource::with_access_probe(&cfg, Box::new(|| false));
    let mut rec = CursorRecorder::with_source(cfg, Box::new(src));
    let clock = SessionClock::start();
    rec.start(TrackContext {
        dir: dir.path().to_path_buf(),
        clock: clock.clone(),
    })
    .unwrap();
    let st = rec.status().unwrap();
    assert_eq!(st.clicks, Availability::Unavailable);
    assert_eq!(st.move_source, crate::MoveSource::Poll);
    std::thread::sleep(Duration::from_millis(1500));
    let info = rec.stop(clock.now()).unwrap();
    let log = CursorLog::read(&dir.path().join("cursor.jsonl")).unwrap();
    println!(
        "fallback: {} events, details {}",
        log.events.len(),
        info.details
    );
    assert_eq!(log.header.clicks, Availability::Unavailable);
    assert!(log
        .events
        .iter()
        .any(|e| matches!(e, CursorEvent::Move { .. })));
}

/// S4 selfcheck on the production source: 400 posted moves at 120 Hz plus 30 middle-click pairs.
/// MOVES THE REAL CURSOR. Never runs unless the user sets CM_ALLOW_INPUT_INJECTION=1.
#[test]
#[ignore = "moves the real cursor; needs CM_ALLOW_INPUT_INJECTION=1, Accessibility and Input Monitoring"]
fn rec7_ac1_ac2_injected_timestamp_accuracy_and_click_completeness() {
    if std::env::var("CM_ALLOW_INPUT_INJECTION").as_deref() != Ok("1") {
        println!("SKIPPED: set CM_ALLOW_INPUT_INJECTION=1 to allow synthetic mouse input");
        return;
    }
    if !sys::post_access() || !sys::listen_access() {
        println!(
            "SKIPPED: post access {} / listen access {}: grant Accessibility and Input Monitoring",
            sys::post_access(),
            sys::listen_access()
        );
        return;
    }
    let cfg = CursorConfig::new(super::main_display_target(None));
    let mut src = MacSource::new(&cfg);
    let (tx, rx) = RawSender::channel();
    let status = src.start(tx).unwrap();
    assert_eq!(status.clicks, Availability::Available);
    let host = SystemHostClock;
    let w = sys::display_bounds(sys::main_display_id()).size.w;
    std::thread::sleep(Duration::from_millis(300));

    let mut posted_moves = Vec::new(); // (x, post_ns)
    let start = Instant::now();
    for i in 0..400u32 {
        let target = start + Duration::from_secs_f64(f64::from(i) / 120.0);
        while Instant::now() < target {
            std::hint::spin_loop();
        }
        let x = w - 600.0 + f64::from(i);
        let ns = host.now_ns();
        sys::inject::post(5, x, 300.0, 0);
        posted_moves.push((x, ns));
    }
    let mut posted_down = Vec::new();
    let mut posted_up = Vec::new();
    for _ in 0..30 {
        let p = (w - 300.0, 300.0);
        sys::inject::post(5, p.0, p.1, 0);
        std::thread::sleep(Duration::from_millis(5));
        let ns = host.now_ns();
        sys::inject::post(25, p.0, p.1, 2);
        posted_down.push(ns);
        std::thread::sleep(Duration::from_millis(15));
        let ns = host.now_ns();
        sys::inject::post(26, p.0, p.1, 2);
        posted_up.push(ns);
        std::thread::sleep(Duration::from_millis(30));
    }
    std::thread::sleep(Duration::from_millis(500));
    src.stop();

    let evs: Vec<_> = rx
        .try_iter()
        .filter_map(|m| match m {
            crate::pipeline::Msg::Raw(e) => Some(e),
            _ => None,
        })
        .collect();
    let mut move_err = Vec::new();
    for (x, post_ns) in &posted_moves {
        if let Some(e) = evs
            .iter()
            .find(|e| matches!(e.kind, RawKind::Move { gx, .. } if (gx - x).abs() < 0.5))
        {
            move_err.push((e.host_ns as f64 - *post_ns as f64) / 1e6);
        }
    }
    let downs: Vec<_> = evs
        .iter()
        .filter(|e| {
            matches!(
                &e.kind,
                RawKind::Button {
                    down: true,
                    button: Button::Middle,
                    ..
                }
            )
        })
        .collect();
    let ups: Vec<_> = evs
        .iter()
        .filter(|e| {
            matches!(
                &e.kind,
                RawKind::Button {
                    down: false,
                    button: Button::Middle,
                    ..
                }
            )
        })
        .collect();
    let mut click_err: Vec<f64> = downs
        .iter()
        .zip(&posted_down)
        .map(|(e, p)| (e.host_ns as f64 - *p as f64) / 1e6)
        .collect();
    click_err.extend(
        ups.iter()
            .zip(&posted_up)
            .map(|(e, p)| (e.host_ns as f64 - *p as f64) / 1e6),
    );
    println!(
        "moves: posted {}, seen {}",
        posted_moves.len(),
        move_err.len()
    );
    println!("clicks: down {}/30, up {}/30", downs.len(), ups.len());
    stats("move event-ts minus post time", &mut move_err);
    stats("click event-ts minus post time", &mut click_err);
    assert_eq!((downs.len(), ups.len()), (30, 30), "no dropped clicks");
    let med = {
        let mut v: Vec<f64> = click_err.iter().map(|x| x.abs()).collect();
        v.sort_by(|a, b| a.total_cmp(b));
        v[v.len() / 2]
    };
    assert!(med < 5.0, "median click timestamp error {med} ms");
}
