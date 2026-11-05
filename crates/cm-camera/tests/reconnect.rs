//! Camera hot-plug while recording (CAM-1, Windows backend logic): the reconnecting wrapper over
//! the scripted fake camera, driving a real `CameraRecorder` with the fake writer. No hardware.

#![allow(clippy::unwrap_used)]

use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::{Duration, Instant};

use cm_camera::{
    Backoff, CameraConfig, CameraEvent, CameraRecorder, CameraSource, FakeCameraHandle,
    FakeCameraSource, FakeWriterFactory, FrameSink, OpenRequest, ReconnectingSource, RecorderEvent,
    SourceRole,
};
use cm_core::{
    CmError, ManualHostClock, MediaTime, Permission, SessionClock, TrackContext, TrackRecorder,
};

const MS: u64 = 1_000_000;
const ORIGIN: u64 = 50_000 * MS;

fn fast() -> Backoff {
    Backoff {
        first: Duration::from_millis(2),
        max: Duration::from_millis(5),
    }
}

struct Rig {
    rec: CameraRecorder,
    cam: FakeCameraHandle,
    writers: FakeWriterFactory,
    dir: PathBuf,
    clock: Arc<ManualHostClock>,
    events: Receiver<RecorderEvent>,
}

fn rig() -> Rig {
    let (src, cam) = FakeCameraSource::new((1920, 1080));
    let writers = FakeWriterFactory::new();
    let dir = std::env::temp_dir().join(format!(
        "cm-camera-reconnect-{}-{}",
        std::process::id(),
        std::thread::current()
            .name()
            .unwrap_or("t")
            .replace("::", "-")
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let clock = Arc::new(ManualHostClock::new(ORIGIN));
    let wrapped = ReconnectingSource::new(Box::new(src), fast());
    let mut rec = CameraRecorder::new(
        CameraConfig::default(),
        Box::new(wrapped),
        Arc::new(writers.clone()),
    );
    rec.start(TrackContext {
        dir: dir.clone(),
        clock: SessionClock::with_host(clock.clone()),
    })
    .unwrap();
    let events = rec.take_events().unwrap();
    Rig {
        rec,
        cam,
        writers,
        dir,
        clock,
        events,
    }
}

impl Rig {
    fn frames(&self, first_ms: u64, n: u64) {
        for i in 0..n {
            let t = ORIGIN + first_ms * MS + i * 1_000_000_000 / 30;
            self.clock.set_ns(t);
            assert!(self.cam.deliver(t, 0), "the source is open");
        }
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// The reopen runs on the wrapper's thread: wait (bounded) for the fake to be open again.
fn wait_open(cam: &FakeCameraHandle, opens: usize) {
    let end = Instant::now() + Duration::from_secs(5);
    while cam.open_count() < opens || !cam.is_open() {
        assert!(Instant::now() < end, "the camera was never reopened");
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn unplug_then_replug_resumes_the_track_with_a_gap_on_the_session_clock() {
    let mut r = rig();
    r.frames(0, 30);
    r.cam.event(CameraEvent::Disconnected);
    assert!(matches!(
        r.events.recv_timeout(Duration::from_secs(5)).unwrap(),
        RecorderEvent::DeviceLost { .. }
    ));
    wait_open(&r.cam, 2);
    // The same device is asked for again: the default camera, pinned to the id it settled on.
    assert_eq!(
        r.cam.last_request().unwrap().device_id.as_deref(),
        Some("fake-cam")
    );
    r.frames(5000, 30);
    let info = r.rec.stop(MediaTime::from_secs(7)).unwrap();
    assert_eq!(info.stats.written, 60);
    let log = r.writers.log();
    assert_eq!(log.appended[29].0.ticks(), 29 * 4000);
    assert_eq!(log.appended[30].0.ticks(), 5 * 120_000, "the gap is real");
    let codes: Vec<_> = info.details["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["code"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(codes, ["camera_disconnected", "camera_restored"]);
    assert!(matches!(
        r.events.try_iter().last().unwrap(),
        RecorderEvent::DeviceRestored { .. }
    ));
}

#[test]
fn a_camera_that_is_still_missing_is_retried_until_it_is_back() {
    let mut r = rig();
    r.frames(0, 3);
    r.cam
        .fail_open_with(CmError::not_found("Camera fake-cam not found."));
    r.cam.event(CameraEvent::Disconnected);
    wait_open(&r.cam, 2);
    r.frames(2000, 3);
    let info = r.rec.stop(MediaTime::from_secs(3)).unwrap();
    assert_eq!(info.stats.written, 6);
}

#[test]
fn it_survives_a_second_unplug() {
    let mut r = rig();
    r.frames(0, 3);
    for (i, at) in [1000u64, 2000].into_iter().enumerate() {
        r.cam.event(CameraEvent::Error("device removed".into()));
        wait_open(&r.cam, i + 2);
        r.frames(at, 3);
        assert_eq!(r.writers.log().appended.len(), 3 * (i + 2));
    }
    r.rec.stop(MediaTime::from_secs(4)).unwrap();
}

#[test]
fn access_withdrawn_during_the_retry_is_reported_not_retried_forever() {
    let mut r = rig();
    r.frames(0, 3);
    r.cam
        .fail_open_with(CmError::permission_missing(Permission::Camera, "denied"));
    r.cam.event(CameraEvent::Disconnected);
    let mut got = Vec::new();
    let end = Instant::now() + Duration::from_secs(5);
    while got.len() < 2 && Instant::now() < end {
        if let Ok(e) = r.events.recv_timeout(Duration::from_millis(20)) {
            got.push(e);
        }
    }
    assert!(matches!(got[0], RecorderEvent::DeviceLost { .. }));
    assert!(matches!(got[1], RecorderEvent::PermissionRevoked { .. }));
    assert!(!r.cam.is_open());
    r.rec.stop(MediaTime::from_secs(1)).unwrap();
}

#[test]
fn close_during_the_retry_stops_the_wrapper() {
    let (src, cam) = FakeCameraSource::new((640, 480));
    let mut w = ReconnectingSource::new(Box::new(src), fast());
    struct Null;
    impl FrameSink for Null {
        fn on_frame(&self, _: &cm_camera::CameraFrame) {}
        fn on_event(&self, _: CameraEvent) {}
    }
    let req = OpenRequest {
        device_id: Some("usb-1".into()),
        resolution: cm_camera::CameraResolution::P720,
        fps: 30,
        role: SourceRole::Record,
        first_frame_timeout: Duration::from_secs(1),
    };
    w.open(&req, Arc::new(Null)).unwrap();
    // Every reopen fails.
    cam.fail_open_with(CmError::not_found("gone"));
    cam.event(CameraEvent::Disconnected);
    std::thread::sleep(Duration::from_millis(20));
    w.close(); // joins the supervisor; must not hang
    assert!(!cam.is_open());
    assert_eq!(
        cam.last_request().unwrap().device_id.as_deref(),
        Some("usb-1")
    );
}

#[test]
fn a_preview_is_passed_through_without_a_supervisor() {
    let (src, cam) = FakeCameraSource::new((640, 480));
    let mut w = ReconnectingSource::new(Box::new(src), fast());
    struct Null;
    impl FrameSink for Null {
        fn on_frame(&self, _: &cm_camera::CameraFrame) {}
        fn on_event(&self, _: CameraEvent) {}
    }
    let req = OpenRequest {
        device_id: None,
        resolution: cm_camera::CameraResolution::P720,
        fps: 30,
        role: SourceRole::Preview { fps: 15 },
        first_frame_timeout: Duration::from_secs(1),
    };
    w.open(&req, Arc::new(Null)).unwrap();
    cam.event(CameraEvent::Disconnected);
    std::thread::sleep(Duration::from_millis(30));
    assert!(cam.is_open(), "no reopen for a preview");
    w.close();
}
