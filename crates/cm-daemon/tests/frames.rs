//! `freeze_display` / `release_frame` / `capture_screenshot { target: { frame_id, rect } }`
//! (CAP-1.AC2: the saved region is the frozen frame, by construction).

#![allow(clippy::unwrap_used)]

mod common;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use base64::Engine as _;
use cm_capture::fake::{pattern_pixel, FakeBackend};
use cm_capture::{CaptureBackend, Capturer, DisplayInfo, Frame, WindowInfo};
use cm_core::CmError;
use cm_daemon::{ClientPolicy, Clock, Daemon, ManualClock};
use cm_library::Library;
use common::{desktop, Client};
use serde_json::{json, Value};
use time::macros::datetime;

/// A fake desktop that counts display captures and changes its pixels after the first one,
/// so a second capture could never match the first.
struct Counting {
    inner: FakeBackend,
    captures: Arc<AtomicUsize>,
}

impl CaptureBackend for Counting {
    fn check_permission(&self) -> Result<(), CmError> {
        self.inner.check_permission()
    }
    fn displays(&self) -> Result<Vec<DisplayInfo>, CmError> {
        self.inner.displays()
    }
    fn windows(&self) -> Result<Vec<WindowInfo>, CmError> {
        self.inner.windows()
    }
    fn capture_display(&self, id: u32) -> Result<Frame, CmError> {
        let n = self.captures.fetch_add(1, Ordering::SeqCst);
        let f = self.inner.capture_display(id)?;
        if n == 0 {
            return Ok(f);
        }
        let (w, h) = (f.width(), f.height());
        Frame::new(w, h, vec![0; w as usize * h as usize * 4])
    }
    fn capture_window(&self, id: u32) -> Result<Frame, CmError> {
        self.inner.capture_window(id)
    }
}

struct SharedClock(Arc<ManualClock>);
impl Clock for SharedClock {
    fn now(&self) -> time::OffsetDateTime {
        self.0.now()
    }
}

fn rig() -> (
    Daemon,
    Arc<AtomicUsize>,
    Arc<ManualClock>,
    tempfile::TempDir,
) {
    let captures = Arc::new(AtomicUsize::new(0));
    let clock = Arc::new(ManualClock::new(datetime!(2026-01-01 00:00 UTC)));
    let dir = tempfile::tempdir().unwrap();
    let lib = Library::open_or_create(dir.path()).unwrap();
    let backend = Counting {
        inner: desktop(),
        captures: captures.clone(),
    };
    let d = Daemon::builder(Capturer::new(backend).with_own_pid(None), lib)
        .clock(SharedClock(clock.clone()))
        .frame_ttl(std::time::Duration::from_secs(60))
        .build();
    (d, captures, clock, dir)
}

fn ok(r: &Value) -> &Value {
    assert!(r.get("error").is_none(), "unexpected error: {r}");
    &r["result"]
}

fn err_code(r: &Value) -> &str {
    r["error"]["data"]["code"].as_str().unwrap()
}

fn png_pixel(b64: &str, x: u32, y: u32) -> [u8; 4] {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .unwrap();
    image::load_from_memory(&bytes)
        .unwrap()
        .to_rgba8()
        .get_pixel(x, y)
        .0
}

async fn library_len(c: &mut Client) -> usize {
    let r = c.call("list_library", json!({})).await;
    ok(&r)["captures"].as_array().unwrap().len()
}

/// CAP-1.AC2: freeze once, crop later: the bundle holds the frozen pixels, the screen is
/// captured exactly once, and freezing alone leaves no bundle and no `capture.created`.
#[tokio::test]
async fn cap1_ac2_region_from_a_frozen_frame_is_the_frozen_pixels() {
    let (d, captures, _clock, _dir) = rig();
    let mut c = Client::connect(&d, "overlay");
    c.hello("capturemann-overlay").await;
    c.call("events.subscribe", json!({"topics": ["capture.*"]}))
        .await;

    let f = c.call("freeze_display", json!({"display_id": 1})).await;
    let f = ok(&f).clone();
    assert_eq!(
        (f["width"].as_u64(), f["height"].as_u64()),
        (Some(400), Some(300))
    );
    assert_eq!(f["scale"], 2.0);
    assert_eq!(f["format"], "rgba8");
    assert_eq!(f["transport"], "file");
    assert_eq!(f["bytes"], 400 * 300 * 4);
    assert_eq!(library_len(&mut c).await, 0, "freezing makes no bundle");

    // The pixels are readable from the file and are the golden pattern.
    let raw = std::fs::read(f["path"].as_str().unwrap()).unwrap();
    assert_eq!(raw.len(), 400 * 300 * 4);
    let px = |x: usize, y: usize| -> [u8; 4] {
        let i = (y * 400 + x) * 4;
        raw[i..i + 4].try_into().unwrap()
    };
    assert_eq!(px(123, 45), pattern_pixel(123, 45));

    let id = f["frame_id"].as_str().unwrap();
    let shot = c
        .call(
            "capture_screenshot",
            json!({"target": {"frame_id": id, "rect": [100, 50, 200, 120]}}),
        )
        .await;
    let shot = ok(&shot);
    assert_eq!(shot["kind"], "region");
    assert_eq!(
        (shot["width"].as_u64(), shot["height"].as_u64()),
        (Some(200), Some(120))
    );
    assert_eq!(shot["frame_id"], id);
    let b64 = shot["image"]["data"].as_str().unwrap();
    assert_eq!(png_pixel(b64, 0, 0), pattern_pixel(100, 50));
    assert_eq!(png_pixel(b64, 199, 119), pattern_pixel(299, 169));
    assert_eq!(
        captures.load(Ordering::SeqCst),
        1,
        "the screen was captured once; the crop did not capture again"
    );
    assert_eq!(library_len(&mut c).await, 1);

    // The first event is the bundle's, none came from the freeze.
    let ev = c.next_event().await;
    assert_eq!(ev["event"], "capture.created");
    assert_eq!(ev["data"]["by"], "agent:capturemann-overlay");
    assert_eq!(ev["data"]["capture_id"], shot["capture_id"]);

    // Same frame, whole display: a display capture of the frozen pixels.
    let whole = c
        .call("capture_screenshot", json!({"target": {"frame_id": id}}))
        .await;
    assert_eq!(ok(&whole)["kind"], "display");
    assert_eq!(
        png_pixel(ok(&whole)["image"]["data"].as_str().unwrap(), 7, 9),
        pattern_pixel(7, 9)
    );
    assert_eq!(captures.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn inline_transport_equals_the_file() {
    let (d, _captures, _clock, _dir) = rig();
    let mut c = Client::connect(&d, "overlay");
    let file = c.call("freeze_display", json!({"display_id": 2})).await;
    let inline = c
        .call(
            "freeze_display",
            json!({"display_id": 2, "transport": "inline"}),
        )
        .await;
    let (file, inline) = (ok(&file), ok(&inline));
    assert_eq!(inline["transport"], "inline");
    assert!(inline.get("path").is_none());
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(inline["data"].as_str().unwrap())
        .unwrap();
    assert_eq!(bytes.len(), 200 * 100 * 4);
    // (The counting backend blanks every capture after the first, so only sizes are compared
    // between the two; the file holds the first, real, frame.)
    let raw = std::fs::read(file["path"].as_str().unwrap()).unwrap();
    assert_eq!(raw.len(), bytes.len());
    assert_eq!(&raw[..4], &pattern_pixel(0, 0));
}

/// Frames expire after the idle TTL and `release_frame` frees them (and their file) at once.
#[tokio::test]
async fn frames_expire_and_release_frees_the_file() {
    let (d, _captures, clock, _dir) = rig();
    let mut c = Client::connect(&d, "overlay");
    let f = c.call("freeze_display", json!({"display_id": 1})).await;
    let (id, path) = (
        ok(&f)["frame_id"].as_str().unwrap().to_owned(),
        std::path::PathBuf::from(ok(&f)["path"].as_str().unwrap()),
    );
    let crop = |id: &str| json!({"target": {"frame_id": id, "rect": [0, 0, 4, 4]}});

    clock.advance(time::Duration::seconds(50));
    let r = c.call("capture_screenshot", crop(&id)).await;
    ok(&r); // use renews the lifetime
    clock.advance(time::Duration::seconds(50));
    let r = c.call("capture_screenshot", crop(&id)).await;
    ok(&r);

    let r = c.call("release_frame", json!({"frame_id": id})).await;
    assert_eq!(ok(&r)["released"], true);
    assert!(!path.exists());
    let r = c.call("release_frame", json!({"frame_id": id})).await;
    assert_eq!(ok(&r)["released"], false, "releasing twice is not an error");
    let r = c.call("capture_screenshot", crop(&id)).await;
    assert_eq!(err_code(&r), "not_found");

    let f = c.call("freeze_display", json!({"display_id": 1})).await;
    let (id, path) = (
        ok(&f)["frame_id"].as_str().unwrap().to_owned(),
        std::path::PathBuf::from(ok(&f)["path"].as_str().unwrap()),
    );
    clock.advance(time::Duration::seconds(61));
    let r = c.call("list_targets", json!({})).await; // any action sweeps
    ok(&r);
    assert!(!path.exists(), "expired frames lose their file");
    let r = c.call("capture_screenshot", crop(&id)).await;
    assert_eq!(err_code(&r), "not_found");
    assert!(r["error"]["message"].as_str().unwrap().contains("freeze"));
}

#[tokio::test]
async fn frame_target_errors_and_policy() {
    let (d, _captures, _clock, _dir) = rig();
    let mut c = Client::connect(&d, "overlay");
    let f = c.call("freeze_display", json!({"display_id": 1})).await;
    let id = ok(&f)["frame_id"].as_str().unwrap().to_owned();
    // A rect outside the frame, and delay_s with a frame, are invalid_input.
    let r = c
        .call(
            "capture_screenshot",
            json!({"target": {"frame_id": id, "rect": [700, 0, 200, 10]}}),
        )
        .await;
    assert_eq!(err_code(&r), "invalid_input");
    let r = c
        .call(
            "capture_screenshot",
            json!({"target": {"frame_id": id}, "delay_s": 1}),
        )
        .await;
    assert_eq!(err_code(&r), "invalid_input");
    let r = c.call("freeze_display", json!({"display_id": 99})).await;
    assert_eq!(err_code(&r), "not_found");

    // freeze_display hands out screen pixels: read-only clients may not (like a screenshot).
    d.policies().set("readonly-agent", ClientPolicy::ReadOnly);
    let mut ro = Client::connect(&d, "ro");
    ro.hello("readonly-agent").await;
    let r = ro.call("freeze_display", json!({"display_id": 1})).await;
    assert_eq!(err_code(&r), "user_declined");

    // actions.list marks the internal plumbing (AGT-1.AC4).
    let list = c.call("actions.list", json!({})).await;
    let rows = ok(&list).as_array().unwrap();
    let internal: Vec<&str> = rows
        .iter()
        .filter(|a| a["internal"] == true)
        .map(|a| a["name"].as_str().unwrap())
        .collect();
    // Deliberate (M3 slice 4): the recording HUD's Discard, Restart and skip-countdown joined the
    // overlay's freeze plumbing as first-party actions without a CLI command or MCP tool; the
    // app's "Rescan library" (`rescan_library`) is internal too.
    assert_eq!(
        internal,
        vec![
            "rescan_library",
            "freeze_display",
            "release_frame",
            "discard_recording",
            "restart_recording",
            "skip_countdown"
        ]
    );
    let f = rows.iter().find(|a| a["name"] == "freeze_display").unwrap();
    assert_eq!(f["tier"], "free");
}
