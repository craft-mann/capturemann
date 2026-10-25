//! Real-hardware recording tests: need a desktop session, Screen Recording permission for the
//! process that hosts `cargo test`, and (for the decode checks) ffprobe. All are `#[ignore]`.
//! Script and pass criteria: tests/manual/REC-1.md.
//!
//! ```text
//! cargo test -p cm-capture --test record_real -- --ignored --nocapture --test-threads=1
//! ```
//!
//! Recorded media can contain personal screen content: every test records into a temp directory
//! that is deleted when the test ends, pass or fail.
#![cfg(target_os = "macos")]
#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use cm_capture::record::{plan_capture, ScreenSettings, SystemScreenRecorder};
use cm_capture::{CaptureTarget, Capturer, RegionRect, Targets};
use cm_core::{
    CmError, MediaTime, PhysicalPoint, PhysicalRect, PhysicalSize, SessionClock, TrackContext,
    TrackInfo, TrackRecorder,
};
use cm_encode::{scan_file, Mp4Scan, OutputSize, VideoSettings};

fn targets_or_skip() -> Option<Targets> {
    match Capturer::system().list_targets() {
        Ok(t) => Some(t),
        Err(CmError::PermissionMissing { .. }) => {
            eprintln!(
                "SKIPPED: Screen Recording permission is not granted to the process hosting \
                 cargo test (see tests/manual/REC-1.md)"
            );
            None
        }
        Err(e) => panic!("{e}"),
    }
}

struct Run {
    info: TrackInfo,
    scan: Mp4Scan,
    wall_secs: f64,
    latency: cm_capture::record::Latency,
}

/// Records `secs` seconds (with optional pause in the middle) into a temp bundle directory.
fn record(settings: ScreenSettings, targets: &Targets, secs: f64, pause_secs: f64) -> Option<Run> {
    let plan = plan_capture(&settings, targets).unwrap();
    println!(
        "plan: {}x{} @ {} fps {} {} Mbps, source {}x{}",
        plan.video.width,
        plan.video.height,
        plan.video.fps,
        plan.video.codec.as_str(),
        plan.video.bitrate_bps as f64 / 1e6,
        plan.source_size.width,
        plan.source_size.height
    );
    let mut rec = SystemScreenRecorder::system(plan);
    let dir = tempfile::tempdir().unwrap();
    let ctx = TrackContext {
        dir: dir.path().to_owned(),
        clock: SessionClock::start(),
    };
    let clock = ctx.clock.clone();
    match rec.start(ctx) {
        Ok(()) => {}
        Err(CmError::PermissionMissing { .. }) => {
            eprintln!("SKIPPED: permission missing at start");
            return None;
        }
        Err(e) => panic!("start failed: {e}"),
    }
    let wall = std::time::Instant::now();
    if pause_secs > 0.0 {
        std::thread::sleep(Duration::from_secs_f64(secs / 2.0));
        rec.pause(clock.now()).unwrap();
        std::thread::sleep(Duration::from_secs_f64(pause_secs));
        rec.resume(clock.now()).unwrap();
        std::thread::sleep(Duration::from_secs_f64(secs / 2.0));
    } else {
        std::thread::sleep(Duration::from_secs_f64(secs));
    }
    let at = clock.now();
    let info = rec.stop(at).unwrap();
    let wall_secs = wall.elapsed().as_secs_f64();
    let latency = rec.latency();
    let path = dir.path().join(&info.file);
    let scan = scan_file(&path).unwrap();
    println!(
        "file: {} bytes, {} frames, {:.3} s, {} fragments {:?}",
        scan.total_len,
        scan.frame_count(),
        scan.duration().map_or(0.0, MediaTime::as_secs_f64),
        scan.fragments.len(),
        scan.fragment_durations_secs()
    );
    println!("stats: {:?}", info.stats);
    println!(
        "details: {}",
        serde_json::to_string_pretty(&info.details).unwrap()
    );
    println!("latency: {latency:?}");
    ffprobe_check(&path, &scan);
    Some(Run {
        info,
        scan,
        wall_secs,
        latency,
    })
}

/// ffprobe must read the file and count the same frames as the box parser.
fn ffprobe_check(path: &Path, scan: &Mp4Scan) {
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-count_frames",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=codec_name,width,height,r_frame_rate,nb_read_frames",
            "-of",
            "csv=p=0",
        ])
        .arg(path)
        .output();
    match out {
        Ok(o) if o.status.success() => {
            let text = String::from_utf8_lossy(&o.stdout).trim().to_string();
            println!("ffprobe: {text}");
            let frames: u64 = text.rsplit(',').next().unwrap().parse().unwrap();
            assert_eq!(
                frames,
                scan.frame_count(),
                "ffprobe and the parser disagree"
            );
        }
        other => eprintln!("ffprobe unavailable or failed ({other:?}): decode check skipped"),
    }
}

fn first_display(t: &Targets) -> (u32, PhysicalSize) {
    let d = t
        .displays
        .iter()
        .find(|d| d.is_primary)
        .unwrap_or(&t.displays[0]);
    (d.id, d.bounds.size)
}

/// Samples this process's resident memory once a second (NFR-5) until dropped.
struct RssSampler {
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    max_mb: std::sync::Arc<std::sync::atomic::AtomicU64>,
    handle: Option<std::thread::JoinHandle<()>>,
}

fn ps_field(field: &str) -> Option<String> {
    let out = Command::new("ps")
        .args([
            "-o",
            &format!("{field}="),
            "-p",
            &std::process::id().to_string(),
        ])
        .output()
        .ok()?;
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// CPU seconds used by this process so far (`ps` cputime, `[[dd-]hh:]mm:ss.xx`).
fn cpu_secs() -> f64 {
    let Some(t) = ps_field("cputime") else {
        return 0.0;
    };
    t.split(':')
        .fold(0.0, |acc, p| acc * 60.0 + p.parse::<f64>().unwrap_or(0.0))
}

impl RssSampler {
    fn start() -> Self {
        use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
        let stop = std::sync::Arc::new(AtomicBool::new(false));
        let max_mb = std::sync::Arc::new(AtomicU64::new(0));
        let (s, m) = (stop.clone(), max_mb.clone());
        let handle = std::thread::spawn(move || {
            while !s.load(Ordering::SeqCst) {
                if let Some(kb) = ps_field("rss").and_then(|v| v.parse::<u64>().ok()) {
                    m.fetch_max(kb / 1024, Ordering::SeqCst);
                }
                std::thread::sleep(Duration::from_millis(500));
            }
        });
        RssSampler {
            stop,
            max_mb,
            handle: Some(handle),
        }
    }

    fn finish(mut self) -> u64 {
        use std::sync::atomic::Ordering;
        self.stop.store(true, Ordering::SeqCst);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
        self.max_mb.load(Ordering::SeqCst)
    }
}

/// Prints the permission state so a skipped run is explained.
#[test]
#[ignore = "needs a real display"]
fn permission_state() {
    let t = Capturer::system().list_targets();
    println!("list_targets: {:?}", t.as_ref().map(|t| t.displays.len()));
}

/// REC-1.AC1 and REC-8: 3 s of the real primary display, native size, 30 fps.
#[test]
#[ignore = "needs a real display and screen permission"]
fn rec1_ac1_records_three_seconds_of_the_real_display() {
    let Some(t) = targets_or_skip() else { return };
    let (id, size) = first_display(&t);
    let mut s = ScreenSettings::new(CaptureTarget::Display { display_id: id });
    s.video = VideoSettings {
        fps: 30,
        ..Default::default()
    };
    let Some(r) = record(s, &t, 3.0, 0.0) else {
        return;
    };
    let init = r.scan.init.as_ref().unwrap();
    assert!(init.moov_first);
    assert_eq!(
        (init.width, init.height),
        (size.width & !1, size.height & !1)
    );
    let dur = r.scan.duration().unwrap().as_secs_f64();
    assert!((dur - 3.0).abs() <= 0.25, "recorded {dur} s");
    assert!(r.scan.is_clean());
    println!(
        "start latency {:?} ms, stop latency {:?} ms",
        r.latency.first_frame_ms, r.latency.stop_ms
    );
}

/// REC-1.AC3: a 1280x720 region is a 1280x720 file.
#[test]
#[ignore = "needs a real display and screen permission"]
fn rec1_ac3_region_1280x720() {
    let Some(t) = targets_or_skip() else { return };
    let (id, size) = first_display(&t);
    if size.width < 1380 || size.height < 820 {
        eprintln!("display too small for the region test");
        return;
    }
    let mut s = ScreenSettings::new(CaptureTarget::Region {
        display_id: id,
        rect: RegionRect::Physical(PhysicalRect {
            origin: PhysicalPoint { x: 100, y: 80 },
            size: PhysicalSize {
                width: 1280,
                height: 720,
            },
        }),
    });
    s.video.fps = 30;
    let Some(r) = record(s, &t, 2.0, 0.0) else {
        return;
    };
    let init = r.scan.init.as_ref().unwrap();
    assert_eq!((init.width, init.height), (1280, 720));
}

/// REC-2.AC3 / REC-1.AC1: 1080p output of the native display.
#[test]
#[ignore = "needs a real display and screen permission"]
fn rec2_1080p_output_has_even_width_and_exact_height() {
    let Some(t) = targets_or_skip() else { return };
    let (id, _) = first_display(&t);
    let mut s = ScreenSettings::new(CaptureTarget::Display { display_id: id });
    s.video.size = OutputSize::P1080;
    s.video.fps = 30;
    let Some(r) = record(s, &t, 2.0, 0.0) else {
        return;
    };
    let init = r.scan.init.as_ref().unwrap();
    assert_eq!(init.height, 1080);
    assert_eq!(init.width % 2, 0);
}

/// REC-4.AC1 on real hardware: 2 s, 3 s paused, 2 s is a 4 s file.
#[test]
#[ignore = "needs a real display and screen permission"]
fn rec4_pause_resume_real_display() {
    let Some(t) = targets_or_skip() else { return };
    let (id, _) = first_display(&t);
    let mut s = ScreenSettings::new(CaptureTarget::Display { display_id: id });
    s.video.size = OutputSize::P1080;
    s.video.fps = 30;
    let Some(r) = record(s, &t, 4.0, 3.0) else {
        return;
    };
    let dur = r.scan.duration().unwrap().as_secs_f64();
    assert!(
        (dur - 4.0).abs() <= 0.3,
        "file is {dur} s, wall {:.1} s",
        r.wall_secs
    );
    assert_eq!(r.info.details["gaps"].as_array().unwrap().len(), 1);
}

/// REC-6: exclusions (own process by pid, an app by bundle id, a window id that does not exist)
/// build a valid ScreenCaptureKit filter and the recording runs. That a specific window is absent
/// from the pixels is the manual check in tests/manual/REC-1.md (S6 `verify-bubble` proved the
/// mechanism).
#[test]
#[ignore = "needs a real display and screen permission"]
fn rec6_exclusions_build_a_valid_filter() {
    let Some(t) = targets_or_skip() else { return };
    let (id, _) = first_display(&t);
    let mut s = ScreenSettings::new(CaptureTarget::Display { display_id: id });
    s.video.size = OutputSize::P720;
    s.video.fps = 30;
    s.show_cursor = false;
    s.exclusions.pids = vec![std::process::id()];
    s.exclusions.bundle_ids = vec!["com.apple.finder".into()];
    s.exclusions.window_ids = vec![0xFFFF_FFF0];
    let Some(r) = record(s, &t, 1.5, 0.0) else {
        return;
    };
    assert!(r.scan.frame_count() >= 40);
    assert_eq!(r.info.details["cursor"], false);
}

/// NFR-2 / REC-3.AC3 / REC-8: 20 s at 60 fps, native size. Late frames <= 1% (S6: 0.26-0.33%),
/// 1200 +- 2 frames, fragments about 2 s.
#[test]
#[ignore = "needs a real display and screen permission; takes about 25 s"]
fn nfr2_twenty_seconds_at_60fps() {
    let Some(t) = targets_or_skip() else { return };
    let (id, _) = first_display(&t);
    let mut s = ScreenSettings::new(CaptureTarget::Display { display_id: id });
    s.video.fps = 60;
    let sampler = RssSampler::start();
    let cpu0 = cpu_secs();
    let Some(r) = record(s, &t, 20.0, 0.0) else {
        return;
    };
    let cpu = cpu_secs() - cpu0;
    let rss = sampler.finish();
    println!(
        "NFR-5: RSS max {rss} MB (limit 600); process CPU {:.1}% of one core over {:.1} s",
        100.0 * cpu / r.wall_secs,
        r.wall_secs
    );
    assert!(rss < 600, "RSS {rss} MB");
    let late_pct = r.info.details["late_pct"].as_f64().unwrap();
    println!(
        "NFR-2: late {late_pct:.3}% (late {} of {} written), frames {}",
        r.info.stats.late,
        r.info.stats.written,
        r.scan.frame_count()
    );
    assert!(late_pct <= 1.0, "late frames {late_pct}%");
    let n = r.scan.frame_count();
    assert!((1198..=1202).contains(&n), "{n} frames for 20 s at 60 fps");
    let durs = r.scan.fragment_durations_secs();
    for d in &durs[..durs.len() - 1] {
        assert!((1.5..=2.6).contains(d), "fragment {d} s");
    }
}

// ---- REC-8.AC1 on real hardware: kill -9 mid-recording, recover on next launch -----------------

const CHILD_ENV: &str = "CM_REC8_CHILD_ROOT";

/// Runs inside the child process spawned by `rec8_ac1_kill9_then_recover`; does nothing otherwise.
#[test]
#[ignore = "child half of rec8_ac1_kill9_then_recover"]
fn rec8_child_recorder() {
    let Ok(root) = std::env::var(CHILD_ENV) else {
        return;
    };
    let lib = cm_library::Library::open_or_create(Path::new(&root)).unwrap();
    let Some(t) = targets_or_skip() else { return };
    let (id, _) = first_display(&t);
    let mut s = ScreenSettings::new(CaptureTarget::Display { display_id: id });
    s.video.size = OutputSize::P1080;
    s.video.fps = 30;
    let plan = plan_capture(&s, &t).unwrap();
    let bundle = lib
        .create_recording(
            Some("Crash test"),
            time::OffsetDateTime::now_utc(),
            serde_json::json!({}),
            &cm_library::rec::SessionInfo {
                clock_origin_ns: 0,
                timebase: "uptime_raw_ns".into(),
                displays: vec![],
                devices: serde_json::json!({}),
                app_version: "test".into(),
                extra: Default::default(),
            },
        )
        .unwrap();
    let mut rec = SystemScreenRecorder::system(plan);
    rec.start(TrackContext {
        dir: bundle.dir().to_owned(),
        clock: SessionClock::start(),
    })
    .unwrap();
    println!("child recording into {}", bundle.dir().display());
    std::thread::sleep(Duration::from_secs(120)); // the parent kills us long before
}

/// REC-8.AC1: a recording process killed with SIGKILL after about 7 s leaves a recording that
/// opens on the next launch with at least (elapsed - 2 s) of media, and the file decodes.
#[test]
#[ignore = "needs a real display and screen permission; takes about 15 s"]
fn rec8_ac1_kill9_then_recover() {
    if targets_or_skip().is_none() {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("lib");
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "rec8_child_recorder", "--nocapture"])
        .env(CHILD_ENV, &root)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    // Wait until the child has written its first complete fragment (about 2 s after the first
    // frame), then let it record 3 more seconds and kill it at a point inside a fragment.
    let started = std::time::Instant::now();
    let screen = || {
        std::fs::read_dir(&root).ok().and_then(|rd| {
            rd.flatten()
                .map(|e| e.path().join("screen.mp4"))
                .find(|p| p.is_file())
        })
    };
    loop {
        if screen().is_some_and(|p| scan_file(&p).is_ok_and(|s| !s.fragments.is_empty())) {
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(40),
            "child never wrote a fragment"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    let first_fragment_seen = std::time::Instant::now();
    std::thread::sleep(Duration::from_secs(3));
    child.kill().unwrap(); // SIGKILL
    child.wait().unwrap();
    // Media time at the kill: the first fragment (about 2 s) existed when we looked.
    let elapsed = 2.0 + first_fragment_seen.elapsed().as_secs_f64();

    // "Next launch".
    let mut lib = cm_library::Library::open(&root).unwrap();
    let summary = lib.recover_incomplete().unwrap();
    assert!(summary.failed.is_empty(), "{:?}", summary.failed);
    assert_eq!(summary.recovered.len(), 1);
    let r = &summary.recovered[0];
    println!(
        "media elapsed about {elapsed:.2} s, recovered {:.2} s, tracks {:?}",
        r.duration.as_secs_f64(),
        r.tracks
    );
    assert!(
        r.duration.as_secs_f64() >= elapsed - 2.0 - 0.3,
        "recovered {:.2} s of {elapsed:.2} s",
        r.duration.as_secs_f64()
    );
    let path = r.dir.join("screen.mp4");
    let scan = scan_file(&path).unwrap();
    assert!(scan.is_clean());
    ffprobe_check(&path, &scan);
    assert!(!r.dir.join("recording.lock").exists());
}
