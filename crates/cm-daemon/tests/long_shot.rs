//! CAP-6 Long Shot through the action bus: `capture_screenshot { mode: "long" }` (AC7), tested
//! with a scripted screen and scroller (no display, no input injection).

#![allow(clippy::unwrap_used)]

mod common;

use std::sync::{Arc, Mutex};

use cm_capture::long_shot::fake::SyntheticPage;
use cm_capture::long_shot::{AutoScrollSupport, Direction, FrameSource, Scroller};
use cm_capture::{CaptureTarget, Capturer, Frame};
use cm_core::{Actor, CmError, ErrorCode};
use cm_daemon::long_shot_ops::LongShotSystem;
use cm_daemon::testing::FixedConfirmer;
use cm_daemon::{Daemon, DaemonError, Decision};
use common::{builder, desktop};
use serde_json::{json, Value};

/// One viewport shared by the frame source and the scroller (both must be `Send`).
#[derive(Clone)]
struct Scripted {
    page: SyntheticPage,
    pos: Arc<Mutex<u32>>,
    support: AutoScrollSupport,
    /// Window ids the run asked to bring to the front.
    raised: Arc<Mutex<Vec<u32>>>,
}

impl Scripted {
    fn new(page: SyntheticPage) -> Self {
        Scripted {
            page,
            pos: Arc::default(),
            support: AutoScrollSupport::Available,
            raised: Arc::default(),
        }
    }
}

impl FrameSource for Scripted {
    fn grab(&mut self) -> Result<Frame, CmError> {
        Ok(self.page.frame_at(*self.pos.lock().unwrap()))
    }
}

impl Scroller for Scripted {
    fn scroll(&mut self, _d: Direction, amount_px: u32) -> Result<(), CmError> {
        let mut p = self.pos.lock().unwrap();
        let max = self.page.content_len - (self.page.view - self.page.header - self.page.footer);
        *p = (*p + amount_px).min(max);
        Ok(())
    }
}

impl LongShotSystem for Scripted {
    fn frames(&self, _c: Arc<Capturer>, _t: CaptureTarget) -> Box<dyn FrameSource + Send> {
        Box::new(self.clone())
    }
    fn scroller(&self, _anchor: Option<(i32, i32)>) -> Result<Box<dyn Scroller + Send>, CmError> {
        Ok(Box::new(self.clone()))
    }
    fn auto_scroll(&self) -> AutoScrollSupport {
        self.support
    }
    fn raise_window(&self, window_id: u32) -> bool {
        self.raised.lock().unwrap().push(window_id);
        true
    }
}

fn agent() -> Actor {
    Actor::agent("claude").unwrap()
}

fn daemon(sys: Scripted, decision: Decision) -> (Daemon, Arc<FixedConfirmer>, tempfile::TempDir) {
    let confirmer = Arc::new(FixedConfirmer::new(decision));
    let (b, dir) = builder(desktop());
    let d = b.confirmer(confirmer.clone()).long_shot_system(sys).build();
    (d, confirmer, dir)
}

fn params(extra: Value) -> Value {
    let mut p = json!({"mode": "long", "target": {"window_id": 1}});
    for (k, v) in extra.as_object().unwrap() {
        p[k] = v.clone();
    }
    p
}

async fn call(d: &Daemon, who: Actor, p: Value) -> Result<Value, DaemonError> {
    d.execute_method(who, 0, "capture_screenshot", &p).await
}

/// AC7: an agent's Auto scroll is Confirm tier (the dialog is shown, and approving runs it); the
/// result is the stitched image, stored like any capture.
#[tokio::test]
async fn cap_6_ac7_agent_auto_scroll_is_confirmed_then_returns_the_image() {
    let page = SyntheticPage::new(24, 800, 300).with_bands(20, 20);
    let (d, confirmer, _dir) = daemon(Scripted::new(page.clone()), Decision::Approve);
    let out = call(&d, agent(), params(json!({"scroll": "auto"})))
        .await
        .unwrap();
    assert_eq!(confirmer.shown().len(), 1, "the dialog was shown");
    assert_eq!(out["height"], page.expected().height());
    assert_eq!(out["width"], 24);
    assert_eq!(out["source"], "long_shot");
    assert_eq!(out["long_shot"]["end"], "reached_end");
    assert_eq!(out["long_shot"]["header_px"], 20);
    assert!(out["capture_id"].as_str().is_some());
    assert_eq!(out["image"]["mime_type"], "image/png");
}

#[tokio::test]
async fn cap_6_ac7_declining_runs_nothing() {
    let page = SyntheticPage::new(24, 800, 300);
    let sys = Scripted::new(page);
    let pos = sys.pos.clone();
    let (d, _c, _dir) = daemon(sys, Decision::Decline);
    let e = call(&d, agent(), params(json!({}))).await.unwrap_err();
    assert_eq!(e.code().unwrap(), ErrorCode::UserDeclined);
    assert_eq!(*pos.lock().unwrap(), 0, "the page was not scrolled");
}

/// AC7: Hand scroll is refused for agents, before any dialog.
#[tokio::test]
async fn cap_6_ac7_hand_scroll_is_refused_for_agents() {
    let (d, confirmer, _dir) = daemon(
        Scripted::new(SyntheticPage::new(24, 800, 300)),
        Decision::Approve,
    );
    let e = call(&d, agent(), params(json!({"scroll": "hand"})))
        .await
        .unwrap_err();
    assert_eq!(e.code().unwrap(), ErrorCode::InvalidInput);
    let DaemonError::WithData { data, .. } = &e else {
        panic!("{e:?}")
    };
    assert_eq!(data["reason"], "needs_user");
    assert!(confirmer.shown().is_empty());
}

/// The user's own Auto scroll needs no confirmation.
#[tokio::test]
async fn cap_6_the_user_needs_no_confirmation() {
    let (d, confirmer, _dir) = daemon(
        Scripted::new(SyntheticPage::new(24, 800, 300)),
        Decision::Decline,
    );
    let out = call(&d, Actor::User, params(json!({}))).await.unwrap();
    assert!(confirmer.shown().is_empty());
    assert_eq!(out["long_shot"]["end"], "reached_end");
}

/// QA Windows pass 3: Auto scroll of a window brings it to the front before the first step (a
/// window that covers its centre would get the wheel); Hand scroll does not touch the windows.
#[tokio::test]
async fn cap_6_auto_scroll_raises_the_target_window_first() {
    let sys = Scripted::new(SyntheticPage::new(24, 800, 300));
    let raised = sys.raised.clone();
    let pos = sys.pos.clone();
    let (d, _c, _dir) = daemon(sys, Decision::Approve);
    call(&d, Actor::User, params(json!({"scroll": "auto"})))
        .await
        .unwrap();
    assert_eq!(*raised.lock().unwrap(), vec![1]);
    assert!(*pos.lock().unwrap() > 0);
}

/// AC4 through the action: `max_px` stops the capture with the limit note, and the image is saved.
#[tokio::test]
async fn cap_6_ac4_max_px_stops_with_the_note_and_saves() {
    let (d, _c, _dir) = daemon(
        Scripted::new(SyntheticPage::new(24, 3000, 300)),
        Decision::Approve,
    );
    let out = call(&d, agent(), params(json!({"max_px": 700})))
        .await
        .unwrap();
    assert_eq!(out["height"], 700);
    assert_eq!(out["long_shot"]["end"], "limit_reached");
    let warnings = out["warnings"].to_string();
    assert!(
        warnings.contains("Reached the longest image CaptureMann can edit"),
        "{warnings}"
    );
}

/// AC6 (daemon side): without Accessibility, Auto scroll answers `permission_missing`.
#[tokio::test]
async fn cap_6_ac6_auto_scroll_without_accessibility() {
    let mut sys = Scripted::new(SyntheticPage::new(24, 800, 300));
    sys.support = AutoScrollSupport::NeedsAccessibility;
    let (d, _c, _dir) = daemon(sys, Decision::Approve);
    let e = call(&d, Actor::User, params(json!({}))).await.unwrap_err();
    assert_eq!(e.code().unwrap(), ErrorCode::PermissionMissing);
}

#[tokio::test]
async fn cap_6_parameters_are_validated() {
    let (d, _c, _dir) = daemon(
        Scripted::new(SyntheticPage::new(24, 800, 300)),
        Decision::Approve,
    );
    for p in [
        params(json!({"max_px": 20000})),
        params(json!({"max_px": 0})),
        params(json!({"direction": "diagonal"})),
        params(json!({"scroll": "robot"})),
        params(json!({"delay_s": 31})),
        json!({"mode": "wide", "target": {"window_id": 1}}),
        json!({"direction": "horizontal", "target": {"window_id": 1}}),
        json!({"mode": "long", "target": {"pick": true}}),
    ] {
        let e = call(&d, Actor::User, p.clone()).await.unwrap_err();
        assert_eq!(e.code().unwrap(), ErrorCode::InvalidInput, "{p}");
    }
}

/// A scroller that takes its time, so a test can look at the run while it goes on.
#[derive(Clone)]
struct Slow(Scripted);

impl Scroller for Slow {
    fn scroll(&mut self, d: Direction, amount_px: u32) -> Result<(), CmError> {
        std::thread::sleep(std::time::Duration::from_millis(40));
        self.0.scroll(d, amount_px)
    }
}

impl LongShotSystem for Slow {
    fn frames(&self, c: Arc<Capturer>, t: CaptureTarget) -> Box<dyn FrameSource + Send> {
        self.0.frames(c, t)
    }
    fn scroller(&self, _anchor: Option<(i32, i32)>) -> Result<Box<dyn Scroller + Send>, CmError> {
        Ok(Box::new(self.clone()))
    }
    fn auto_scroll(&self) -> AutoScrollSupport {
        AutoScrollSupport::Available
    }
}

async fn state(d: &Daemon, extra: Value) -> Value {
    let mut p = json!({"request_id": "ls-1"});
    for (k, v) in extra.as_object().unwrap() {
        p[k] = v.clone();
    }
    d.execute_method(Actor::User, 0, "long_shot_state", &p)
        .await
        .unwrap()
}

/// CAP-6 HUD: `long_shot_state` reports the counts and a preview picture, Pause holds the run,
/// Resume lets it finish, and the stored manifest says `source: "long_shot"` with the frame count.
#[tokio::test]
async fn cap_6_hud_state_pause_resume_and_manifest_source() {
    let page = SyntheticPage::new(24, 3000, 300);
    let (b, _dir) = builder(desktop());
    let d = b.long_shot_system(Slow(Scripted::new(page))).build();
    // Before: nothing runs under this id.
    assert_eq!(state(&d, json!({})).await["running"], false);
    let d2 = d.clone();
    let run = tokio::spawn(async move {
        call(
            &d2,
            Actor::User,
            params(json!({"request_id": "ls-1", "max_px": 16384})),
        )
        .await
    });
    // Wait for the first picture.
    let mut s = Value::Null;
    for _ in 0..300 {
        s = state(&d, json!({})).await;
        if s["running"] == true && s["preview"].is_object() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(s["running"], true, "{s}");
    assert!(s["preview"]["png_base64"].as_str().unwrap().len() > 20);
    assert!(s["preview"]["width"].as_u64().unwrap() <= 360);
    // Asking again with a newer seq gives the counts but no picture.
    let s = state(&d, json!({"since_seq": u64::MAX})).await;
    assert!(s.get("preview").is_none());
    // Pause: the frame count stops moving.
    state(&d, json!({"paused": true})).await;
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let a = state(&d, json!({})).await;
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    let b2 = state(&d, json!({})).await;
    assert_eq!(a["paused"], true);
    assert_eq!(a["frames"], b2["frames"], "paused: no new frame");
    state(&d, json!({"paused": false})).await;
    let out = run.await.unwrap().unwrap();
    assert_eq!(out["long_shot"]["end"], "reached_end");
    assert_eq!(state(&d, json!({})).await["running"], false);
    // The manifest carries the source and the frame count.
    let path = std::path::PathBuf::from(out["path"].as_str().unwrap());
    let manifest: Value =
        serde_json::from_slice(&std::fs::read(path.with_file_name("manifest.json")).unwrap())
            .unwrap();
    assert_eq!(manifest["source"], "long_shot");
    assert_eq!(manifest["frames"], out["long_shot"]["frames"]);
}

/// Stop while paused ends the run and keeps what was captured.
#[tokio::test]
async fn cap_6_stop_while_paused_keeps_the_image() {
    let page = SyntheticPage::new(24, 3000, 300);
    let (b, _dir) = builder(desktop());
    let d = b.long_shot_system(Slow(Scripted::new(page))).build();
    let d2 = d.clone();
    let run =
        tokio::spawn(
            async move { call(&d2, Actor::User, params(json!({"request_id": "ls-1"}))).await },
        );
    for _ in 0..300 {
        if state(&d, json!({})).await["running"] == true {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    state(&d, json!({"paused": true})).await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert!(d.cancel_delayed_capture("ls-1"));
    let out = run.await.unwrap().unwrap();
    assert_eq!(out["long_shot"]["end"], "stopped_by_user");
    assert!(out["capture_id"].as_str().is_some());
}

/// QA, Windows, Nov 15, 2026: a Long Shot started from the CLI or MCP has no HUD and no
/// `request_id` to name, and Esc did not end it. The app's Esc key calls `cancel_long_shots`: it
/// stops every running Long Shot, whoever started it, and each keeps what it captured.
#[tokio::test]
async fn cap_6_esc_stops_a_run_that_has_no_request_id_and_keeps_the_frames() {
    let page = SyntheticPage::new(24, 6000, 300);
    let (b, _dir) = builder(desktop());
    let d = b.long_shot_system(Slow(Scripted::new(page))).build();
    let mut events = d.subscribe();
    let d2 = d.clone();
    let run = tokio::spawn(async move { call(&d2, Actor::User, params(json!({}))).await });
    // The run announces itself, so the app knows to grab Esc.
    loop {
        let ev = tokio::time::timeout(std::time::Duration::from_secs(10), events.recv())
            .await
            .unwrap()
            .unwrap();
        if ev.name == "capture.long_shot.started" {
            assert!(ev.data["request_id"].is_null());
            assert_eq!(ev.data["scroll"], "auto");
            assert_eq!(ev.data["direction"], "vertical");
            break;
        }
    }
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    assert_eq!(d.cancel_long_shots(), 1);
    let out = run.await.unwrap().unwrap();
    assert_eq!(out["long_shot"]["end"], "stopped_by_user");
    assert!(out["long_shot"]["frames"].as_u64().unwrap() >= 1);
    assert!(out["capture_id"].as_str().is_some(), "the image is kept");
    assert_eq!(d.cancel_long_shots(), 0, "nothing is running any more");
    let mut ended = false;
    while let Ok(ev) = events.try_recv() {
        ended |= ev.name == "capture.long_shot.ended";
    }
    assert!(ended, "the end is announced");
}

/// `long_shot_state { stop: true }` (a client that knows the `request_id`) ends the run too.
#[tokio::test]
async fn cap_6_long_shot_state_stop_ends_the_run_and_keeps_the_image() {
    let page = SyntheticPage::new(24, 6000, 300);
    let (b, _dir) = builder(desktop());
    let d = b.long_shot_system(Slow(Scripted::new(page))).build();
    let d2 = d.clone();
    let run =
        tokio::spawn(
            async move { call(&d2, Actor::User, params(json!({"request_id": "ls-1"}))).await },
        );
    for _ in 0..300 {
        if state(&d, json!({})).await["running"] == true {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    let s = state(&d, json!({"stop": true})).await;
    assert_eq!(s["stopped"], true, "{s}");
    let out = run.await.unwrap().unwrap();
    assert_eq!(out["long_shot"]["end"], "stopped_by_user");
    assert!(out["capture_id"].as_str().is_some());
}

/// S18: the started event says who started the run, how and where, so the app can show a small
/// passive HUD for a Long Shot it did not start. The area is in the display's own physical px.
#[tokio::test]
async fn cap_6_started_event_says_who_how_and_where_for_the_passive_hud() {
    let page = SyntheticPage::new(24, 800, 300).with_bands(20, 20);
    let (d, _confirmer, _dir) = daemon(Scripted::new(page), Decision::Approve);
    let mut events = d.subscribe();
    let out = call(
        &d,
        agent(),
        json!({"mode": "long", "target": {"window_id": 10}, "scroll": "auto", "request_id": "mcp-7"}),
    )
    .await
    .unwrap();
    assert!(out["capture_id"].as_str().is_some());
    let mut started = None;
    let mut progress = Vec::new();
    while let Ok(ev) = events.try_recv() {
        match ev.name.as_str() {
            "capture.long_shot.started" => started = Some(ev.data),
            "capture.long_shot.progress" => progress.push(ev.data),
            _ => {}
        }
    }
    let s = started.expect("the started event");
    assert_eq!(s["request_id"], "mcp-7");
    assert_eq!(s["client"], "claude");
    assert_eq!(s["scroll"], "auto");
    assert_eq!(s["display_id"], 1);
    assert_eq!(s["area"], json!([20, 10, 100, 80]));
    assert_eq!(s["scale"], 2.0);
    // The progress event carries the join flag; a run that ends at the end of the page has no
    // failed join.
    assert!(!progress.is_empty());
    assert!(progress.iter().all(|p| p["join_ok"] == true));
}
