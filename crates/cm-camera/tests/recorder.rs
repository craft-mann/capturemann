//! CAM-2 recorder behaviour against the fake camera and writer (no hardware, no display).

#![allow(clippy::unwrap_used)]

use std::path::PathBuf;
use std::sync::Arc;

use cm_camera::{
    CameraConfig, CameraEvent, CameraRecorder, CameraResolution, FakeCameraHandle,
    FakeCameraSource, FakeWriterFactory, RecorderEvent,
};
use cm_core::{
    CmError, ErrorCode, ManualHostClock, MediaTime, SessionClock, TrackContext, TrackKind,
    TrackRecorder,
};

const MS: u64 = 1_000_000;
const ORIGIN: u64 = 50_000 * MS;

struct Rig {
    rec: CameraRecorder,
    cam: FakeCameraHandle,
    writers: FakeWriterFactory,
    dir: PathBuf,
    clock: Arc<ManualHostClock>,
}

fn rig(config: CameraConfig, size: (u32, u32)) -> Rig {
    let (src, cam) = FakeCameraSource::new(size);
    let writers = FakeWriterFactory::new();
    let dir = std::env::temp_dir().join(format!(
        "cm-camera-test-{}-{}",
        std::process::id(),
        std::thread::current()
            .name()
            .unwrap_or("t")
            .replace("::", "-")
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let clock = Arc::new(ManualHostClock::new(ORIGIN));
    let rec = CameraRecorder::new(config, Box::new(src), Arc::new(writers.clone()));
    Rig {
        rec,
        cam,
        writers,
        dir,
        clock,
    }
}

impl Rig {
    fn start(&mut self) {
        self.rec
            .start(TrackContext {
                dir: self.dir.clone(),
                clock: SessionClock::with_host(self.clock.clone()),
            })
            .unwrap();
    }

    /// `n` frames at 30 fps starting `first_ms` after the origin, each delivered `lag_ms` late.
    fn frames(&self, first_ms: u64, n: u64, lag_ms: u64) {
        for i in 0..n {
            let t = ORIGIN + first_ms * MS + i * 1_000_000_000 / 30;
            self.clock.set_ns(t + lag_ms * MS);
            self.cam.deliver(t, lag_ms * MS);
        }
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn retimes_from_the_capture_pts_not_the_delivery_time() {
    let mut r = rig(CameraConfig::default(), (1920, 1080));
    r.start();
    // Captured from 100 ms on, but delivered 75 ms late (S6): placement must follow capture time.
    r.frames(100, 90, 75);
    let info = r.rec.stop(MediaTime::from_secs(3)).unwrap();
    let log = r.writers.log();
    assert_eq!(log.appended[0].0.ticks(), 12_000); // 100 ms
    assert_eq!(log.appended[1].0.ticks(), 12_000 + 4000); // + 1/30 s
    assert!(log.appended.windows(2).all(|w| w[0].0 < w[1].0));
    assert_eq!(log.appended[0].1.ticks(), 4000);
    assert_eq!(info.kind, TrackKind::Camera);
    assert_eq!(info.file, "camera.mp4");
    assert_eq!(info.start.ticks(), 12_000);
    assert_eq!(info.end.ticks(), 12_000 + 89 * 4000 + 4000);
    assert_eq!(info.stats.delivered, 90);
    assert_eq!(info.stats.written, 90);
    assert_eq!(info.stats.dropped, 0);
    assert_eq!(info.details["timestamp_source"], "capture_pts_host_clock");
    let lat = &info.details["delivery_latency_ms"];
    assert_eq!(lat["median_ms"], 75.0);
    assert!(r.dir.join("camera.mp4").exists());
}

#[test]
fn latency_compensation_shifts_the_track_earlier() {
    let cfg = CameraConfig {
        latency_compensation_ms: 50.0,
        ..CameraConfig::default()
    };
    let mut r = rig(cfg, (1920, 1080));
    r.start();
    r.frames(100, 30, 0);
    let info = r.rec.stop(MediaTime::from_secs(2)).unwrap();
    assert_eq!(r.writers.log().appended[0].0.ticks(), 6000); // 100 ms - 50 ms
    assert_eq!(info.details["latency_compensation_ms"], 50.0);
}

#[test]
fn frames_before_the_session_origin_are_not_written() {
    let mut r = rig(CameraConfig::default(), (1920, 1080));
    r.start();
    r.cam.deliver(ORIGIN - 50 * MS, 0);
    r.frames(0, 3, 0);
    let info = r.rec.stop(MediaTime::from_secs(1)).unwrap();
    assert_eq!(info.stats.written, 3);
    assert_eq!(info.details["frames"]["before_origin"], 1);
}

#[test]
fn mirror_is_metadata_only() {
    for mirror in [true, false] {
        let cfg = CameraConfig {
            mirror,
            ..CameraConfig::default()
        };
        let mut r = rig(cfg, (1920, 1080));
        r.start();
        r.frames(0, 10, 0);
        let info = r.rec.stop(MediaTime::from_secs(1)).unwrap();
        assert_eq!(info.details["mirror"], mirror);
        // The writer is given no flip request: only size/fps/bitrate.
        let spec = r.writers.log().spec.unwrap();
        assert_eq!((spec.width, spec.height, spec.fps), (1920, 1080, 30));
    }
}

#[test]
fn details_describe_the_track() {
    let mut r = rig(CameraConfig::default(), (1920, 1080));
    r.start();
    r.frames(0, 60, 40);
    let info = r.rec.stop(MediaTime::from_secs(2)).unwrap();
    let d = &info.details;
    assert_eq!(d["size"], serde_json::json!([1920, 1080]));
    assert_eq!(d["fps"], 30);
    assert_eq!(d["codec"], "h264");
    assert_eq!(d["b_frames"], false);
    assert_eq!(d["bitrate_bps"], 8_000_000);
    assert_eq!(d["resolution_setting"], "1080p");
    assert_eq!(d["device"]["name"], "Fake Camera");
    assert_eq!(d["fragment_seconds"], 2);
    assert_eq!(d["measured_fps"], 30.0);
}

#[test]
fn non_standard_and_oversized_modes() {
    let mut r = rig(CameraConfig::default(), (1280, 960));
    r.start();
    r.frames(0, 5, 0);
    let spec = {
        r.rec.stop(MediaTime::from_secs(1)).unwrap();
        r.writers.log().spec.unwrap()
    };
    assert_eq!((spec.width, spec.height), (1280, 960)); // native, not forced to 16:9

    let mut r = rig(CameraConfig::default(), (3840, 2160));
    r.start();
    r.frames(0, 5, 0);
    r.rec.stop(MediaTime::from_secs(1)).unwrap();
    let spec = r.writers.log().spec.unwrap();
    assert_eq!((spec.width, spec.height), (1920, 1080)); // capped at 1080p (CAM-2)
}

#[test]
fn source_receives_the_requested_resolution_and_rate() {
    let cfg = CameraConfig {
        resolution: CameraResolution::P720,
        device_id: Some("cam-2".into()),
        ..CameraConfig::default()
    };
    let mut r = rig(cfg, (1280, 720));
    r.start();
    let req = r.cam.last_request().unwrap();
    assert_eq!(req.resolution, CameraResolution::P720);
    assert_eq!(req.device_id.as_deref(), Some("cam-2"));
    assert_eq!(req.fps, 30);
}

#[test]
fn pause_removes_the_gap_from_file_time_and_resumes_in_the_same_file() {
    let mut r = rig(CameraConfig::default(), (1920, 1080));
    r.start();
    r.frames(0, 30, 10); // 0..1 s
    r.rec.pause(MediaTime::from_secs(1)).unwrap();
    r.frames(1000, 60, 10); // 1..3 s captured while paused
    r.rec.resume(MediaTime::from_secs(3)).unwrap();
    r.frames(3000, 30, 10); // 3..4 s
    let info = r.rec.stop(MediaTime::from_secs(4)).unwrap();
    let log = r.writers.log();
    assert_eq!(log.appended.len(), 60);
    let before_gap = log.appended[29].0;
    let after_gap = log.appended[30].0;
    assert!(before_gap.ticks() < 120_000);
    // 3 s of session time minus the 2 s gap: the file continues right where it left off.
    assert_eq!(after_gap.ticks(), 120_000);
    assert_eq!(after_gap - before_gap, MediaTime::from_ticks(4000));
    assert!(log.appended.windows(2).all(|w| w[1].0 == w[0].0 + w[0].1));
    assert_eq!(info.stats.written, 60);
    assert_eq!(info.stats.dropped, 0);
    assert_eq!(info.details["frames"]["paused_skipped"], 60);
    // The pause is a known gap, not missed frames.
    assert_eq!(info.details["frames"]["missed_slots"], 0);
    assert_eq!(info.stats.late, 0);
    // Gaps stay in session time; the file facts are in file time.
    assert_eq!(
        info.details["pauses"],
        serde_json::json!([{ "from": 120_000, "to": 360_000 }])
    );
    assert_eq!(info.details["timeline"], "file_time_gaps_removed");
    assert_eq!(info.details["file_start"], 0);
    assert_eq!(info.details["file_end"], 240_000); // 2 s
                                                   // TrackInfo is session time, like the screen track.
    assert_eq!(info.start.ticks(), 0);
    assert_eq!(info.end.ticks(), 360_000 + 29 * 4000 + 4000);
}

/// REC-4: continuous PTS across a pause and the 2 s fragment cadence (no hole in the file).
#[test]
fn pause_in_the_middle_keeps_pts_continuous_and_fragments_two_seconds() {
    let mut r = rig(CameraConfig::default(), (1920, 1080));
    r.start();
    r.frames(0, 30 * 5, 5); // 0..5 s
    r.rec.pause(MediaTime::from_secs(5)).unwrap();
    r.frames(5000, 30 * 3, 5); // 3 s paused
    r.rec.resume(MediaTime::from_secs(8)).unwrap();
    r.frames(8000, 30 * 5, 5); // 8..13 s
    let info = r.rec.stop(MediaTime::from_secs(13)).unwrap();
    let log = r.writers.log();
    assert_eq!(log.appended.len(), 300);
    for (i, (pts, dur)) in log.appended.iter().enumerate() {
        assert_eq!(pts.ticks(), 4000 * i as i64, "frame {i}");
        assert_eq!(dur.ticks(), 4000);
    }
    let f = &info.details["fragments"];
    assert_eq!(f["count"], 5); // 10 s of file in 2 s fragments
    assert!((f["min_s"].as_f64().unwrap() - 2.0).abs() < 0.04);
    assert!((f["max_s"].as_f64().unwrap() - 2.0).abs() < 0.04);
    assert_eq!(info.details["file_end"], 1_200_000); // 10 s, not 13 s
    assert_eq!(info.end.ticks(), 13 * 120_000);
}

#[test]
fn pause_still_open_at_stop_is_closed_at_the_stop_time() {
    let mut r = rig(CameraConfig::default(), (1920, 1080));
    r.start();
    r.frames(0, 30, 0);
    r.rec.pause(MediaTime::from_secs(1)).unwrap();
    let info = r.rec.stop(MediaTime::from_secs(5)).unwrap();
    assert_eq!(
        info.details["pauses"],
        serde_json::json!([{ "from": 120_000, "to": 600_000 }])
    );
}

#[test]
fn device_removed_mid_recording_keeps_the_file_valid_and_reports_a_warning() {
    let mut r = rig(CameraConfig::default(), (1920, 1080));
    r.start();
    let events = r.rec.take_events().unwrap();
    r.frames(0, 60, 10); // 2 s
    r.cam.event(CameraEvent::Disconnected);
    assert!(matches!(
        events.try_recv().unwrap(),
        RecorderEvent::DeviceLost { .. }
    ));
    assert_eq!(
        r.writers.log().flushes,
        1,
        "everything captured so far is flushed"
    );
    // Recording continues without the camera; a duplicate event is ignored.
    r.cam.event(CameraEvent::Disconnected);
    assert!(events.try_recv().is_err());
    let info = r.rec.stop(MediaTime::from_secs(10)).unwrap();
    assert!(r.writers.log().finished);
    assert!(r.dir.join("camera.mp4").exists());
    assert_eq!(info.stats.written, 60);
    assert_eq!(
        info.end.ticks(),
        59 * 4000 + 4000,
        "track ends at the last frame"
    );
    let w = &info.details["warnings"][0];
    assert_eq!(w["code"], "camera_disconnected");
}

#[test]
fn camera_access_revoked_mid_recording_flushes_and_reports_a_typed_event() {
    let mut r = rig(CameraConfig::default(), (1920, 1080));
    r.start();
    let events = r.rec.take_events().unwrap();
    r.frames(0, 60, 10);
    r.cam.event(CameraEvent::PermissionRevoked);
    assert!(matches!(
        events.try_recv().unwrap(),
        RecorderEvent::PermissionRevoked { .. }
    ));
    assert_eq!(r.writers.log().flushes, 1);
    let info = r.rec.stop(MediaTime::from_secs(10)).unwrap();
    assert!(r.writers.log().finished, "the file is closed cleanly");
    assert_eq!(info.stats.written, 60);
    assert_eq!(
        info.details["warnings"][0]["code"],
        "camera_permission_revoked"
    );
}

#[test]
fn device_that_comes_back_leaves_a_gap_and_a_restored_event() {
    let mut r = rig(CameraConfig::default(), (1920, 1080));
    r.start();
    let events = r.rec.take_events().unwrap();
    r.frames(0, 30, 10);
    r.cam.event(CameraEvent::Disconnected);
    r.cam.event(CameraEvent::Reconnected);
    r.frames(5000, 30, 10);
    let info = r.rec.stop(MediaTime::from_secs(7)).unwrap();
    assert_eq!(info.stats.written, 60);
    let kinds: Vec<_> = events.try_iter().collect();
    assert!(matches!(kinds[0], RecorderEvent::DeviceLost { .. }));
    assert!(matches!(kinds[1], RecorderEvent::DeviceRestored { .. }));
    let codes: Vec<_> = info.details["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["code"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(codes, ["camera_disconnected", "camera_restored"]);
}

#[test]
fn losing_the_camera_before_any_frame_is_an_error_at_stop_not_a_panic() {
    let mut r = rig(CameraConfig::default(), (1920, 1080));
    r.start();
    r.cam.event(CameraEvent::Error("boom".into()));
    let e = r.rec.stop(MediaTime::from_secs(1)).unwrap_err();
    assert_eq!(e.code(), ErrorCode::Internal);
    assert!(e.to_string().contains("no frames"));
}

#[test]
fn fragments_are_two_seconds() {
    let mut r = rig(CameraConfig::default(), (1920, 1080));
    r.start();
    r.frames(0, 30 * 10 + 15, 5); // 10.5 s
    let info = r.rec.stop(MediaTime::from_secs(11)).unwrap();
    let f = &info.details["fragments"];
    assert_eq!(f["count"], 6); // 5 full + the 0.5 s tail
    let max = f["max_s"].as_f64().unwrap();
    let min = f["min_s"].as_f64().unwrap();
    assert!((max - 2.0).abs() < 0.04, "{max}");
    assert!(min > 0.4 && min < 2.0, "{min}");
}

#[test]
fn back_pressure_and_os_drops_are_counted_not_hidden() {
    let mut r = rig(CameraConfig::default(), (1920, 1080));
    r.writers.not_ready_every(10);
    r.start();
    r.frames(0, 100, 0);
    r.cam.event(CameraEvent::FramesDropped(4));
    let info = r.rec.stop(MediaTime::from_secs(4)).unwrap();
    assert_eq!(info.stats.delivered, 104);
    assert_eq!(info.stats.written, 90);
    assert_eq!(info.stats.dropped, 14);
    // Dropped frames are reported as dropped, not also as late (late = written behind deadline).
    assert_eq!(info.stats.late, 0);
}

#[test]
fn a_delivery_stall_counts_as_late() {
    let mut r = rig(CameraConfig::default(), (1920, 1080));
    r.start();
    r.frames(0, 60, 20);
    // One frame delivered 200 ms behind its steady-state lag.
    let t = ORIGIN + 2000 * MS;
    r.clock.set_ns(t + 220 * MS);
    r.cam.deliver(t, 220 * MS);
    r.frames(2034, 30, 20);
    let info = r.rec.stop(MediaTime::from_secs(4)).unwrap();
    assert_eq!(info.stats.late, 1);
    assert_eq!(info.stats.written, 91);
}

#[test]
fn writer_failure_stops_the_track_but_keeps_what_was_written() {
    let mut r = rig(CameraConfig::default(), (1920, 1080));
    r.writers.fail_after(45);
    r.start();
    let events = r.rec.take_events().unwrap();
    r.frames(0, 90, 0);
    assert!(matches!(
        events.try_recv().unwrap(),
        RecorderEvent::WriterFailed { .. }
    ));
    let info = r.rec.stop(MediaTime::from_secs(3)).unwrap();
    assert_eq!(info.stats.written, 45);
    assert_eq!(info.stats.dropped, 45);
    assert_eq!(info.details["warnings"][0]["code"], "camera_writer_failed");
}

#[test]
fn start_maps_open_errors() {
    for err in [
        CmError::device_busy("Zoom has the camera"),
        CmError::permission_missing(cm_core::Permission::Camera, "denied"),
        CmError::not_found("Camera x not found"),
    ] {
        let code = err.code();
        let mut r = rig(CameraConfig::default(), (1920, 1080));
        r.cam.fail_open_with(err);
        let e = r
            .rec
            .start(TrackContext {
                dir: r.dir.clone(),
                clock: SessionClock::with_host(r.clock.clone()),
            })
            .unwrap_err();
        assert_eq!(e.code(), code);
        // A failed start leaves nothing to stop.
        assert!(r.rec.stop(MediaTime::ZERO).is_err());
    }
}

#[test]
fn stop_closes_the_source_and_ignores_late_frames() {
    let mut r = rig(CameraConfig::default(), (1920, 1080));
    r.start();
    r.frames(0, 10, 0);
    assert!(r.cam.is_open());
    r.rec.stop(MediaTime::from_secs(1)).unwrap();
    assert!(!r.cam.is_open());
    assert!(!r.cam.deliver(ORIGIN + 2000 * MS, 0));
    assert_eq!(r.rec.stats().written, 10);
    assert!(r.rec.stop(MediaTime::from_secs(2)).is_err());
}

#[test]
fn rejects_bad_fps() {
    let cfg = CameraConfig {
        fps: 0,
        ..CameraConfig::default()
    };
    let mut r = rig(cfg, (1920, 1080));
    let e = r
        .rec
        .start(TrackContext {
            dir: r.dir.clone(),
            clock: SessionClock::with_host(r.clock.clone()),
        })
        .unwrap_err();
    assert_eq!(e.code(), ErrorCode::InvalidInput);
}
