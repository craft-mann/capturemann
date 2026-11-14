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
}

impl Scripted {
    fn new(page: SyntheticPage) -> Self {
        Scripted {
            page,
            pos: Arc::default(),
            support: AutoScrollSupport::Available,
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
