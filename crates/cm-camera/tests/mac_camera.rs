//! macOS hardware checks. `list_cameras` and the permission check run everywhere; the tests that
//! turn the camera on are `#[ignore]` and additionally need `CM_CAMERA_TEST_YES=1`, an already
//! granted camera permission (nothing here prompts) and run at most a few seconds.
//! Run: `CM_CAMERA_TEST_YES=1 cargo test -p cm-camera --test mac_camera -- --ignored --nocapture`
//! The recording is written to a temp dir and deleted at the end (it contains whoever is in front
//! of the camera).

#![cfg(target_os = "macos")]
#![allow(clippy::unwrap_used)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use cm_camera::{
    check_camera_permission, list_cameras, mp4, CameraConfig, CameraPreview, CameraRecorder,
    PreviewOptions,
};
use cm_core::{SessionClock, TrackContext, TrackRecorder};

fn opted_in() -> bool {
    std::env::var("CM_CAMERA_TEST_YES").is_ok_and(|v| v == "1")
        && check_camera_permission().is_authorized()
}

#[test]
fn enumeration_works_without_permission_or_cameras() {
    let cams = list_cameras().unwrap();
    for c in &cams {
        assert!(!c.id.is_empty() && !c.name.is_empty());
        assert!(c
            .modes
            .windows(2)
            .all(|w| w[0].width * w[0].height >= w[1].width * w[1].height));
        assert_eq!(c.max_size[0] > 0, !c.modes.is_empty());
    }
    assert!(cams.iter().filter(|c| c.is_default).count() <= 1);
    println!(
        "{} camera(s), permission {:?}",
        cams.len(),
        check_camera_permission()
    );
}

#[test]
#[ignore = "turns the camera on for about 6 s; needs CM_CAMERA_TEST_YES=1 and granted permission"]
fn records_and_previews_on_one_shared_session() {
    if !opted_in() {
        eprintln!("skipped: set CM_CAMERA_TEST_YES=1 with camera permission granted");
        return;
    }
    let dir = std::env::temp_dir().join(format!("cm-camera-hw-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();

    let previews = Arc::new(AtomicUsize::new(0));
    let p2 = previews.clone();
    let preview = CameraPreview::start(
        PreviewOptions::default(),
        move |f| {
            if p2.fetch_add(1, Ordering::Relaxed) == 0 {
                let img = f.to_rgba(320, true).unwrap();
                assert_eq!(img.data.len(), (img.width * img.height * 4) as usize);
            }
        },
        |_| {},
    )
    .unwrap();

    let clock = SessionClock::start();
    let mut rec = CameraRecorder::system(CameraConfig::default());
    rec.start(TrackContext {
        dir: dir.clone(),
        clock: clock.clone(),
    })
    .unwrap();
    std::thread::sleep(Duration::from_secs(5));
    let info = rec.stop(clock.now()).unwrap();
    drop(preview);

    let rep = mp4::inspect_file(&dir.join("camera.mp4")).unwrap();
    println!("{}", serde_json::to_string_pretty(&info.details).unwrap());
    println!(
        "stats {:?}; fragments {:?}",
        info.stats, rep.fragment_starts_s
    );
    // Decodability and timestamps, when ffmpeg is installed (post-recording check only).
    let file = dir.join("camera.mp4");
    let decode = std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(&file)
        .args(["-f", "null", "-"])
        .output();
    let series = cm_camera::flash::luma_series(&file);
    let _ = std::fs::remove_dir_all(&dir);
    if let Ok(d) = decode {
        assert!(
            d.status.success() && d.stderr.is_empty(),
            "ffmpeg: {}",
            String::from_utf8_lossy(&d.stderr)
        );
        let s = series.unwrap();
        println!(
            "ffprobe frames {}, first pts {:.4} s, last {:.4} s",
            s.times_s.len(),
            s.times_s[0],
            s.times_s[s.times_s.len() - 1]
        );
        assert_eq!(s.times_s.len() as u64, info.stats.written);
        assert!((s.times_s[0] - info.start.as_secs_f64()).abs() < 0.002);
        assert!(s.times_s.windows(2).all(|w| w[1] > w[0]));
    }

    assert!(rep.moov_first);
    assert_eq!(rep.timescale, 120_000);
    assert!(info.stats.written >= 100, "{:?}", info.stats);
    assert!(
        previews.load(Ordering::Relaxed) >= 20,
        "preview kept running during the recording"
    );
    assert!(info.start.as_secs_f64() < 1.0);
}
