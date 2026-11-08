//! REC-7 on a Linux X11 desktop: records the real pointer while `xdotool` moves it, clicks and
//! scrolls (run on the lab laptop only, never on the maintainer's Mac). `--ignored`.
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used)]

use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use cm_core::{Rect, SessionClock, SystemHostClock, TrackContext, TrackRecorder};
use cm_cursor::{
    Button, CursorConfig, CursorEvent, CursorLog, CursorRecorder, CursorTarget, CURSOR_FILE,
};

fn xdotool(args: &[&str]) {
    let st = Command::new("xdotool")
        .args(args)
        .status()
        .expect("xdotool");
    assert!(st.success(), "xdotool {args:?}");
}

#[test]
#[ignore = "moves the real pointer with xdotool (lab laptop, X11 only)"]
fn moves_clicks_scroll_and_images_are_recorded() {
    if !cm_cursor::preflight()
        .clicks
        .eq(&cm_cursor::Availability::Available)
    {
        eprintln!("SKIPPED: not an X11 session");
        return;
    }
    let target = CursorTarget::display(1, Rect::new(0.0, 0.0, 1920.0, 1080.0), 1.0);
    let mut rec = CursorRecorder::new(CursorConfig::new(target));
    let dir = tempfile::tempdir().unwrap();
    let clock = SessionClock::with_host(Arc::new(SystemHostClock));
    rec.start(TrackContext {
        dir: dir.path().to_path_buf(),
        clock: clock.clone(),
    })
    .unwrap();
    std::thread::sleep(Duration::from_millis(300));
    xdotool(&["mousemove", "200", "200"]);
    std::thread::sleep(Duration::from_millis(100));
    xdotool(&["mousemove", "600", "400"]);
    std::thread::sleep(Duration::from_millis(100));
    xdotool(&["click", "1"]);
    std::thread::sleep(Duration::from_millis(100));
    xdotool(&["click", "5"]);
    std::thread::sleep(Duration::from_millis(100));
    xdotool(&["mousemove", "800", "500"]);
    std::thread::sleep(Duration::from_millis(400));
    let t_stop = clock.now();
    rec.stop(t_stop).unwrap();
    let log = CursorLog::read(&dir.path().join(CURSOR_FILE)).unwrap();
    for e in &log.events {
        eprintln!("{e:?}");
    }
    let has = |f: &dyn Fn(&CursorEvent) -> bool| log.events.iter().any(f);
    assert!(has(&|e| matches!(
        e,
        CursorEvent::Move { x: 600, y: 400, .. }
    )));
    assert!(has(&|e| matches!(
        e,
        CursorEvent::Move { x: 800, y: 500, .. }
    )));
    assert!(has(&|e| matches!(
        e,
        CursorEvent::Down {
            button: Button::Left,
            x: 600,
            y: 400,
            ..
        }
    )));
    assert!(has(&|e| matches!(
        e,
        CursorEvent::Up {
            button: Button::Left,
            ..
        }
    )));
    assert!(has(&|e| matches!(e, CursorEvent::Scroll { dy: -1, .. })));
    assert!(
        has(&|e| matches!(e, CursorEvent::Cursor { .. })),
        "no cursor image"
    );
    // Events are time ordered and inside the recording.
    let ts: Vec<i64> = log
        .events
        .iter()
        .map(|e| match e {
            CursorEvent::Move { t, .. }
            | CursorEvent::Down { t, .. }
            | CursorEvent::Up { t, .. }
            | CursorEvent::Scroll { t, .. }
            | CursorEvent::Cursor { t, .. } => *t,
            _ => 0,
        })
        .collect();
    assert!(ts.windows(2).all(|w| w[0] <= w[1]), "{ts:?}");
}
