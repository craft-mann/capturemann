//! Windows camera checks on real hardware (Media Foundation). All `#[ignore]`: run by hand on the
//! test laptop with
//! `cargo test -p cm-camera --test windows_camera -- --ignored --nocapture --test-threads=1`
//! (steps in tests/manual/CAM-1.md, "Windows"). They never prompt: with no camera or with camera
//! access off they print SKIPPED and pass. Nothing is written to disk.

#![cfg(windows)]
#![allow(clippy::unwrap_used)]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use cm_camera::{
    check_camera_permission, list_cameras, CameraEvent, CameraFrame, CameraPermission,
    CameraResolution, FrameSink, OpenRequest, SourceRole,
};

#[derive(Default)]
struct Collect {
    stamps: Mutex<Vec<(u64, u64)>>,
    first: Mutex<Option<cm_camera::RgbaImage>>,
    events: Mutex<Vec<CameraEvent>>,
}

impl FrameSink for Collect {
    fn on_frame(&self, f: &CameraFrame) {
        self.stamps
            .lock()
            .unwrap()
            .push((f.capture_host_ns, f.delivered_host_ns));
        let mut first = self.first.lock().unwrap();
        if first.is_none() {
            *first = f.native.as_ref().and_then(|n| n.to_rgba(160, true));
        }
    }
    fn on_event(&self, e: CameraEvent) {
        self.events.lock().unwrap().push(e);
    }
}

fn camera_or_skip() -> Option<cm_camera::CameraInfo> {
    let state = check_camera_permission();
    if state != CameraPermission::Authorized {
        eprintln!("SKIPPED: camera permission is {state:?}");
        return None;
    }
    let cams = list_cameras().unwrap();
    if cams.is_empty() {
        eprintln!("SKIPPED: no camera found");
        return None;
    }
    cams.into_iter().next()
}

#[test]
#[ignore = "opens each camera briefly to read its modes (no recording)"]
fn lists_cameras_with_modes() {
    let cams = list_cameras().unwrap();
    eprintln!("{}", serde_json::to_string_pretty(&cams).unwrap());
    assert!(cams.iter().filter(|c| c.is_default).count() <= 1);
    for c in &cams {
        assert!(!c.id.is_empty() && !c.name.is_empty());
    }
}

#[test]
#[ignore = "opens the real camera for 3 s (frames are discarded)"]
fn default_camera_delivers_steady_stamped_frames() {
    let Some(cam) = camera_or_skip() else { return };
    let sink = Arc::new(Collect::default());
    let mut src = cm_camera::system_source();
    let info = src
        .open(
            &OpenRequest {
                device_id: Some(cam.id.clone()),
                resolution: CameraResolution::P720,
                fps: 30,
                role: SourceRole::Record,
                first_frame_timeout: Duration::from_secs(5),
            },
            sink.clone(),
        )
        .unwrap();
    std::thread::sleep(Duration::from_secs(3));
    src.close();
    eprintln!("opened {} ({})", info.device_name, info.device_id);
    let stamps = sink.stamps.lock().unwrap().clone();
    assert!(
        stamps.len() >= 30,
        "expected about 90 frames in 3 s, got {}",
        stamps.len()
    );
    // Capture stamps rise, never lead the delivery, and trail it by less than a second.
    let mut worst_ms = 0.0f64;
    for w in stamps.windows(2) {
        assert!(w[1].0 >= w[0].0, "capture stamps went backwards");
    }
    for (capture, delivered) in &stamps {
        assert!(capture <= delivered);
        worst_ms = worst_ms.max((delivered - capture) as f64 / 1e6);
    }
    eprintln!(
        "frames {}, worst delivery latency {worst_ms:.1} ms",
        stamps.len()
    );
    assert!(worst_ms < 1000.0);
    let img = sink
        .first
        .lock()
        .unwrap()
        .take()
        .expect("a preview picture");
    assert!(img.width <= 160 && img.height <= 160);
    let lit = img.data.chunks(4).any(|p| p[0] > 8 || p[1] > 8 || p[2] > 8);
    eprintln!(
        "first picture {}x{}, not all black: {lit}",
        img.width, img.height
    );
    assert!(sink
        .events
        .lock()
        .unwrap()
        .iter()
        .all(|e| !matches!(e, CameraEvent::Error(_))));
}
