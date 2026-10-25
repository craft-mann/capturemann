//! Recorder and reader tests on a fake source and a manual clock (no display, no permissions).

use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use cm_core::{
    ErrorCode, ManualHostClock, MediaTime, Permission, Rect, SessionClock, TrackContext, TrackInfo,
    TrackRecorder,
};

use crate::image::tests_support::sample;
use crate::*;

const ORIGIN_NS: u64 = 5_000_000_000;

fn ms(m: u64) -> u64 {
    ORIGIN_NS + m * 1_000_000
}

fn target_2x() -> CursorTarget {
    CursorTarget::display(1, Rect::new(0.0, 0.0, 1000.0, 500.0), 2.0)
}

struct Rig {
    rec: CursorRecorder,
    h: FakeHandle,
    dir: tempfile::TempDir,
    clock: SessionClock,
}

fn rig_with(status: CursorStatus, target: CursorTarget) -> Rig {
    let (src, h) = FakeCursorSource::new(status);
    let mut rec = CursorRecorder::with_source(CursorConfig::new(target), Box::new(src));
    let dir = tempfile::tempdir().unwrap();
    let clock = SessionClock::with_host(Arc::new(ManualHostClock::new(ORIGIN_NS)));
    rec.start(TrackContext {
        dir: dir.path().to_path_buf(),
        clock: clock.clone(),
    })
    .unwrap();
    Rig { rec, h, dir, clock }
}

fn rig() -> Rig {
    rig_with(CursorStatus::full(MoveSource::Fake), target_2x())
}

fn finish(r: &mut Rig, at_ms: i64) -> (TrackInfo, CursorLog) {
    let info = r.rec.stop(MediaTime::from_millis(at_ms)).unwrap();
    let log = CursorLog::read(&r.dir.path().join(CURSOR_FILE)).unwrap();
    (info, log)
}

fn moves(log: &CursorLog) -> Vec<(i64, i32, i32)> {
    log.events
        .iter()
        .filter_map(|e| match e {
            CursorEvent::Move { t, x, y, .. } => Some((*t, *x, *y)),
            _ => None,
        })
        .collect()
}

#[test]
fn host_timestamps_become_ticks_from_the_os_event_time() {
    let mut r = rig();
    r.h.mv(ms(1500), 10.0, 20.0); // 1.5 s after the origin
    r.h.click(ms(2000), 10.0, 20.0, Button::Left, true);
    let (info, log) = finish(&mut r, 3000);
    assert_eq!(log.header.tick_rate, 120_000);
    assert_eq!(
        log.events[0],
        CursorEvent::Move {
            t: 180_000,
            x: 20,
            y: 40,
            outside: false
        }
    );
    assert!(matches!(
        log.events[1],
        CursorEvent::Down {
            t: 240_000,
            button: Button::Left,
            ..
        }
    ));
    assert_eq!(info.kind, cm_core::TrackKind::Cursor);
    assert_eq!(info.file, "cursor.jsonl");
    assert_eq!(info.end, MediaTime::from_millis(3000));
    assert_eq!(info.stats.written, 2);
    assert_eq!(info.stats.delivered, 2);
}

#[test]
fn twenty_clicks_give_twenty_downs_and_ups_with_modifier_flags_only() {
    let mut r = rig();
    for i in 0..20u64 {
        let t = 100 + i * 50;
        r.h.send(
            ms(t),
            RawKind::Button {
                gx: 5.0,
                gy: 5.0,
                button: Button::Left,
                down: true,
                mods: vec![Modifier::Shift],
            },
        );
        r.h.click(ms(t + 20), 5.0, 5.0, Button::Left, false);
    }
    let (_, log) = finish(&mut r, 2000);
    let downs = log
        .events
        .iter()
        .filter(|e| matches!(e, CursorEvent::Down { .. }))
        .count();
    let ups = log
        .events
        .iter()
        .filter(|e| matches!(e, CursorEvent::Up { .. }))
        .count();
    assert_eq!((downs, ups), (20, 20));
    let raw = fs::read_to_string(r.dir.path().join(CURSOR_FILE)).unwrap();
    assert!(raw.contains("\"mods\":[\"shift\"]"));
    assert!(!raw.contains("keycode") && !raw.contains("char"));
}

#[test]
fn moves_are_rate_limited_to_120hz_and_the_last_position_survives() {
    let mut r = rig();
    for i in 0..100u64 {
        r.h.mv(ms(1000 + i), i as f64, 0.0); // 1 kHz mouse for 100 ms
    }
    let (info, log) = finish(&mut r, 2000);
    let m = moves(&log);
    assert!((12..=14).contains(&m.len()), "{} moves", m.len());
    assert!(
        m.windows(2).all(|w| w[1].0 - w[0].0 >= 1000 - 120),
        "spacing {m:?}"
    );
    assert_eq!(
        m.last().unwrap().1,
        198,
        "final position (99 pt * 2) is kept"
    );
    assert_eq!(info.stats.delivered, 100);
    assert!(info.details["coalesced_moves"].as_u64().unwrap() > 80);
}

#[test]
fn unchanged_positions_are_not_rewritten() {
    let mut r = rig();
    for i in 0..10u64 {
        r.h.mv(ms(1000 + i * 20), 7.0, 7.0);
    }
    let (_, log) = finish(&mut r, 2000);
    assert_eq!(moves(&log).len(), 1);
}

#[test]
fn region_target_reports_region_relative_px_and_outside() {
    let display = Rect::new(0.0, 0.0, 1000.0, 500.0);
    let t = CursorTarget::region_on_display(1, display, Rect::new(100.0, 50.0, 200.0, 100.0), 2.0);
    let mut r = rig_with(CursorStatus::full(MoveSource::Fake), t);
    r.h.mv(ms(100), 100.0, 50.0); // region top-left
    r.h.mv(ms(200), 150.0, 75.0);
    r.h.mv(ms(300), 90.0, 75.0); // left of the region
    r.h.click(ms(400), 400.0, 75.0, Button::Right, true); // right of the region
    let (_, log) = finish(&mut r, 1000);
    let e = &log.events;
    assert_eq!(
        e[0],
        CursorEvent::Move {
            t: 12_000,
            x: 0,
            y: 0,
            outside: false
        }
    );
    assert_eq!(
        e[1],
        CursorEvent::Move {
            t: 24_000,
            x: 100,
            y: 50,
            outside: false
        }
    );
    assert_eq!(
        e[2],
        CursorEvent::Move {
            t: 36_000,
            x: -20,
            y: 50,
            outside: true
        }
    );
    assert!(matches!(
        e[3],
        CursorEvent::Down {
            x: 600,
            y: 50,
            outside: true,
            ..
        }
    ));
    assert_eq!(log.header.target.size.width, 400);
}

#[test]
fn pause_writes_nothing_between_pause_and_resume() {
    let mut r = rig();
    r.h.mv(ms(500), 1.0, 1.0);
    r.rec.pause(MediaTime::from_millis(1000)).unwrap();
    r.h.mv(ms(1200), 2.0, 2.0);
    r.h.click(ms(1300), 2.0, 2.0, Button::Left, true);
    r.h.click(ms(1400), 3.0, 3.0, Button::Left, false);
    r.h.image(ms(1500), sample(9));
    r.rec.resume(MediaTime::from_millis(2000)).unwrap();
    r.h.mv(ms(2500), 4.0, 4.0);
    let (info, log) = finish(&mut r, 3000);
    let (from, to) = (
        MediaTime::from_millis(1000).ticks(),
        MediaTime::from_millis(2000).ticks(),
    );
    for e in &log.events {
        assert!(!(e.t() > from && e.t() < to), "event inside the gap: {e:?}");
    }
    assert!(!log
        .events
        .iter()
        .any(|e| matches!(e, CursorEvent::Down { .. } | CursorEvent::Up { .. })));
    // Resume re-anchors with the latest position and the image that changed during the gap.
    let at_resume: Vec<_> = log.events.iter().filter(|e| e.t() == to).collect();
    assert!(matches!(at_resume[0], CursorEvent::Move { x: 6, y: 6, .. }));
    assert!(matches!(at_resume[1], CursorEvent::Cursor { .. }));
    assert_eq!(info.details["paused_discarded"], 4);
    assert_eq!(info.details["pause_gaps"][0]["from"], from);
}

#[test]
fn permission_fallback_records_moves_and_marks_clicks_unavailable() {
    let status = CursorStatus::permission_fallback(Permission::InputMonitoring, "denied");
    let mut r = rig_with(status, target_2x());
    let hud = r.rec.status().unwrap();
    assert!(hud.is_degraded());
    assert_eq!(hud.missing_permission(), Some(Permission::InputMonitoring));
    assert_eq!(hud.warnings[0].code, ErrorCode::PermissionMissing);
    r.h.mv(ms(100), 1.0, 1.0);
    r.h.click(ms(150), 1.0, 1.0, Button::Left, true); // would never arrive; ignored if it does
    r.h.mv(ms(300), 2.0, 2.0);
    let (info, log) = finish(&mut r, 1000);
    assert_eq!(log.header.clicks, Availability::Unavailable);
    assert_eq!(log.header.scroll, Availability::Unavailable);
    assert_eq!(log.header.move_source, MoveSource::Poll);
    assert_eq!(
        log.header.permission_missing,
        Some(Permission::InputMonitoring)
    );
    assert_eq!(moves(&log).len(), 2);
    assert!(!log
        .events
        .iter()
        .any(|e| matches!(e, CursorEvent::Down { .. })));
    assert_eq!(info.details["clicks"], "unavailable");
    assert_eq!(info.details["permission_missing"], "input_monitoring");
    assert_eq!(info.details["warnings"][0]["code"], "permission_missing");
}

#[test]
fn images_dedupe_and_cursor_events_reference_them() {
    let mut r = rig();
    r.h.image(ms(0), sample(1));
    r.h.image(ms(300), sample(2));
    r.h.image(ms(600), sample(1)); // back to the first: same file
    r.h.image(ms(650), sample(1)); // already in effect: no event
    let (info, log) = finish(&mut r, 1000);
    let ids: Vec<_> = log
        .events
        .iter()
        .filter_map(|e| match e {
            CursorEvent::Cursor {
                id, hotspot, scale, ..
            } => Some((id.clone(), *hotspot, *scale)),
            _ => None,
        })
        .collect();
    assert_eq!(ids.len(), 3);
    assert_eq!(ids[0].0, ids[2].0);
    assert_ne!(ids[0].0, ids[1].0);
    assert_eq!(ids[0].1, [1, 2]);
    let idx = CursorLog::image_index(r.dir.path()).unwrap();
    assert_eq!(idx.images.len(), 2);
    for id in idx.images.keys() {
        let png = fs::read(r.dir.path().join("cursors").join(format!("{id}.png"))).unwrap();
        assert_eq!(&png[1..4], b"PNG");
    }
    assert_eq!(info.details["images"], 2);
}

#[test]
fn lines_stay_in_time_order_and_pre_origin_events_are_dropped() {
    let mut r = rig();
    r.h.mv(ORIGIN_NS - 5_000_000, 1.0, 1.0); // before the session origin
    r.h.mv(ms(500), 2.0, 2.0);
    r.h.image(ms(900), sample(1)); // polled image stamped later than...
    r.h.click(ms(800), 2.0, 2.0, Button::Left, true); // ...an event that arrives after it
    let (info, log) = finish(&mut r, 1000);
    assert!(log.events.windows(2).all(|w| w[0].t() <= w[1].t()));
    assert_eq!(info.details["before_origin"], 1);
    assert!(log.events.iter().all(|e| e.t() >= 0));
}

#[test]
fn lines_reach_the_file_within_about_a_second_without_stop() {
    let mut r = rig();
    r.h.mv(ms(100), 3.0, 3.0);
    std::thread::sleep(Duration::from_millis(1400));
    let log = CursorLog::read(&r.dir.path().join(CURSOR_FILE)).unwrap();
    assert_eq!(moves(&log).len(), 1, "flushed before stop (crash safety)");
    let _ = r.rec.stop(MediaTime::from_millis(2000)).unwrap();
}

#[test]
fn recorder_lifecycle_errors() {
    let (src, _h) = FakeCursorSource::full();
    let mut rec = CursorRecorder::with_source(CursorConfig::new(target_2x()), Box::new(src));
    assert!(rec.pause(MediaTime::ZERO).is_err());
    assert!(rec.stop(MediaTime::ZERO).is_err());
    let mut r = rig();
    let a = r.rec.stop(MediaTime::from_millis(10)).unwrap();
    let b = r.rec.stop(MediaTime::from_millis(20)).unwrap();
    assert_eq!(a, b, "second stop returns the same info");
    assert_eq!(r.rec.kind(), cm_core::TrackKind::Cursor);
    let _ = &r.clock;
}

// --- reader -----------------------------------------------------------------------------------

fn sample_file() -> Vec<u8> {
    let mut r = rig();
    for i in 0..5u64 {
        r.h.mv(ms(100 + i * 100), (i * 10) as f64, 0.0);
    }
    r.h.click(ms(700), 1.0, 1.0, Button::Left, true);
    let _ = r.rec.stop(MediaTime::from_millis(1000)).unwrap();
    fs::read(r.dir.path().join(CURSOR_FILE)).unwrap()
}

#[test]
fn reader_tolerates_a_truncated_last_line() {
    let full = sample_file();
    let log_full = CursorLog::parse(&full).unwrap();
    assert!(!log_full.truncated);
    // Cut in the middle of the last line, at every possible byte of it.
    let last_start = full[..full.len() - 1]
        .iter()
        .rposition(|&b| b == b'\n')
        .unwrap()
        + 1;
    for cut in last_start + 1..full.len() - 1 {
        let log = CursorLog::parse(&full[..cut]).unwrap_or_else(|e| panic!("cut {cut}: {e}"));
        assert_eq!(log.events.len(), log_full.events.len() - 1, "cut {cut}");
        assert!(log.truncated, "cut {cut}");
    }
    // Header-only and header-torn files.
    let first_nl = full.iter().position(|&b| b == b'\n').unwrap();
    assert_eq!(
        CursorLog::parse(&full[..=first_nl]).unwrap().events.len(),
        0
    );
    assert!(CursorLog::parse(&full[..first_nl / 2]).is_err());
}

#[test]
fn reader_rejects_corruption_in_the_middle_and_foreign_headers() {
    let full = String::from_utf8(sample_file()).unwrap();
    let mut lines: Vec<&str> = full.lines().collect();
    lines[2] = "{not json";
    let bad = lines.join("\n") + "\n";
    assert!(CursorLog::parse(bad.as_bytes()).is_err());
    assert!(CursorLog::parse(b"").is_err());
    assert!(CursorLog::parse(b"{\"type\":\"move\",\"t\":1,\"x\":0,\"y\":0}\n").is_err());
    let newer = full.replacen("\"schema\":1", "\"schema\":99", 1);
    assert!(CursorLog::parse(newer.as_bytes()).is_err());
    // Unknown event types are skipped.
    let fwd = format!(
        "{}\n{{\"type\":\"future\",\"t\":3}}\n",
        full.lines().next().unwrap()
    );
    assert_eq!(CursorLog::parse(fwd.as_bytes()).unwrap().events.len(), 0);
}

#[test]
fn reader_ranges_and_position_lookup() {
    let log = CursorLog::parse(&sample_file()).unwrap();
    let (a, b) = (MediaTime::from_millis(200), MediaTime::from_millis(400));
    let r = log.range(a, b);
    assert!(r.iter().all(|e| e.t() >= a.ticks() && e.t() < b.ticks()));
    assert_eq!(r.len(), 2);
    assert!(log.range(b, a).is_empty());
    assert_eq!(
        log.iter_range(MediaTime::ZERO, MediaTime::from_secs(10))
            .count(),
        log.events.len()
    );
    assert_eq!(
        log.position_at(MediaTime::from_millis(350)),
        Some((40, 0, false))
    );
    assert_eq!(log.position_at(MediaTime::from_millis(50)), None);
}

#[test]
fn bundle_contains_no_typed_text() {
    // REC-7.AC3: the only keyboard-derived data is the modifier flag set; scan every file.
    let mut r = rig();
    r.h.send(
        ms(100),
        RawKind::Button {
            gx: 1.0,
            gy: 1.0,
            button: Button::Left,
            down: true,
            mods: vec![Modifier::Cmd, Modifier::Alt],
        },
    );
    r.h.send(ms(200), RawKind::Scroll { dx: 5, dy: -9 });
    r.h.image(ms(0), sample(4));
    let _ = r.rec.stop(MediaTime::from_millis(500)).unwrap();
    let text = fs::read_to_string(r.dir.path().join(CURSOR_FILE)).unwrap();
    assert!(
        text.contains("\"type\":\"scroll\",\"t\":24000,\"dx\":1,\"dy\":-1"),
        "{text}"
    );
    fn scan(dir: &Path, out: &mut String) {
        for e in fs::read_dir(dir).unwrap().flatten() {
            if e.path().is_dir() {
                scan(&e.path(), out);
            } else if e
                .path()
                .extension()
                .is_some_and(|x| x == "jsonl" || x == "json")
            {
                out.push_str(&fs::read_to_string(e.path()).unwrap());
            }
        }
    }
    let mut all = String::new();
    scan(r.dir.path(), &mut all);
    assert!(!all.contains("key") && !all.contains("text"));
}

#[test]
fn a_source_that_fails_to_start_leaves_no_files_in_the_bundle() {
    struct Failing;
    impl CursorSource for Failing {
        fn start(&mut self, _tx: RawSender) -> Result<CursorStatus, cm_core::CmError> {
            Err(cm_core::CmError::internal("event tap failed"))
        }
        fn stop(&mut self) {}
    }
    let mut rec = CursorRecorder::with_source(CursorConfig::new(target_2x()), Box::new(Failing));
    let dir = tempfile::tempdir().unwrap();
    let clock = SessionClock::with_host(Arc::new(ManualHostClock::new(ORIGIN_NS)));
    let err = rec
        .start(TrackContext {
            dir: dir.path().to_path_buf(),
            clock,
        })
        .unwrap_err();
    assert_eq!(err.code(), ErrorCode::Internal);
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
}
