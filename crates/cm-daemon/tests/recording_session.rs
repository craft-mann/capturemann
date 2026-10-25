//! The recording session over every recorder's fake source: start, pause, resume, stop, partial
//! failure, clock/origin alignment, manifest content, the disk guard and crash recovery
//! (REC-1..REC-4, REC-8, NFR-3, NFR-6, NFR-7). No display, no devices, no real time.

#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cm_audio::{
    AudioSource, DeviceSelector, FakeAudioSource, FakeConfig, FakeSignal, MicConfig, MicRecorder,
};
use cm_camera::{
    CameraConfig, CameraEvent, CameraRecorder, FakeCameraHandle, FakeCameraSource,
    FakeWriterFactory,
};
use cm_capture::disk::FakeFreeSpace;
use cm_capture::record::fake::{FakeScreenHandle, FakeScreenSource};
use cm_capture::record::{plan_capture, RecorderOptions, ScreenSettings};
use cm_capture::{CaptureTarget, DisplayInfo, Targets};
use cm_core::{
    CmError, ErrorCode, ManualHostClock, MediaTime, PauseGap, PhysicalPoint, PhysicalRect,
    PhysicalSize, Rect, ScaleFactor, TrackKind, TrackRecorder,
};
use cm_cursor::{
    Button, CursorConfig, CursorEvent, CursorLog, CursorRecorder, CursorStatus, CursorTarget,
    FakeCursorSource, MoveSource,
};
use cm_daemon::recording::{
    CameraTrack, CursorTrack, MicTrack, NoteLevel, PlainTrack, RecordingSession, ScreenTrack,
    SessionConfig, SessionState, SessionTrack, TrackState,
};
use cm_encode::synthetic::SyntheticEncoder;
use cm_encode::{is_hardware_encoder_unavailable, scan_file, VideoSettings};
use cm_library::manifest::RecManifest;
use cm_library::rec::LOCK_FILE;
use cm_library::Library;
use serde_json::Value;

const ORIGIN: u64 = 5_000_000_000;
const MIC_OFFSET_MS: u64 = 100;
const SCREEN_OFFSET_MS: u64 = 250;
const CAMERA_OFFSET_MS: u64 = 300;

fn ms(v: i64) -> MediaTime {
    MediaTime::from_millis(v)
}

fn secs(t: MediaTime) -> f64 {
    t.as_secs_f64()
}

fn targets(width: u32, height: u32) -> Targets {
    Targets {
        displays: vec![DisplayInfo {
            id: 1,
            name: "d".into(),
            bounds: PhysicalRect {
                origin: PhysicalPoint { x: 0, y: 0 },
                size: PhysicalSize { width, height },
            },
            scale: ScaleFactor::new(1.0).unwrap(),
            is_primary: true,
            is_builtin: false,
        }],
        windows: vec![],
    }
}

#[derive(Clone, Copy)]
struct Which {
    mic: bool,
    camera: bool,
    cursor: bool,
}

const ALL: Which = Which {
    mic: true,
    camera: true,
    cursor: true,
};
const SCREEN_CURSOR: Which = Which {
    mic: false,
    camera: false,
    cursor: true,
};

struct Rig {
    host: Arc<ManualHostClock>,
    free: Arc<FakeFreeSpace>,
    root: tempfile::TempDir,
    screen: FakeScreenHandle,
    mic: Option<cm_audio::FakeHandle>,
    cursor: Option<cm_cursor::FakeHandle>,
    camera: Option<FakeCameraHandle>,
    camera_writers: Option<FakeWriterFactory>,
    // delivery cursors
    next_frame: i64,
    next_cam: i64,
    next_move: i64,
    mic_dev_ms: i64,
    now_ms: i64,
}

type Tracks = Vec<Box<dyn SessionTrack>>;

struct Options {
    mic_fault_after: Option<u64>,
    which: Which,
    width: u32,
    height: u32,
    cursor_status: Option<CursorStatus>,
    camera_open_error: Option<CmError>,
    mic_start_error: Option<CmError>,
    screen_permission: bool,
    /// The "hardware" encoder rejects the plan: the start fails (REC-2, no software fallback).
    reject_hardware: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            mic_fault_after: None,
            which: ALL,
            width: 1920,
            height: 1080,
            cursor_status: None,
            camera_open_error: None,
            mic_start_error: None,
            screen_permission: true,
            reject_hardware: false,
        }
    }
}

fn build(o: Options) -> (Rig, Tracks) {
    let host = Arc::new(ManualHostClock::new(ORIGIN));
    let free = Arc::new(FakeFreeSpace::new(500_000_000_000));
    let root = tempfile::tempdir().unwrap();
    let mut tracks: Tracks = Vec::new();

    let mut s = ScreenSettings::new(CaptureTarget::Display { display_id: 1 });
    s.video = VideoSettings {
        fps: 30,
        ..Default::default()
    };
    let plan = plan_capture(&s, &targets(o.width, o.height)).unwrap();
    let (src, screen) = if o.screen_permission {
        FakeScreenSource::new()
    } else {
        FakeScreenSource::without_permission()
    };
    let reject = o.reject_hardware;
    tracks.push(Box::new(ScreenTrack::new(
        plan,
        src,
        move |cfg| {
            if reject {
                Err(cm_encode::hardware_encoder_unavailable())
            } else {
                SyntheticEncoder::create(cfg)
            }
        },
        RecorderOptions {
            heartbeat: false,
            free_space: free.clone(),
            ..Default::default()
        },
    )));

    let mut mic = None;
    if o.which.mic {
        let (src, h) = FakeAudioSource::new(FakeConfig {
            host_start_ns: ORIGIN + MIC_OFFSET_MS * 1_000_000,
            signal: FakeSignal::Constant(0.5),
            ..Default::default()
        });
        if let Some(e) = o.mic_start_error {
            h.fail_next_start(e);
        }
        let mut src = Some(src);
        let mut rec = MicRecorder::with_factory(
            MicConfig {
                denoise: false,
                ..Default::default()
            },
            Box::new(move |_sel: &DeviceSelector| {
                src.take()
                    .map(|s| Box::new(s) as Box<dyn AudioSource>)
                    .ok_or_else(|| CmError::internal("fake mic already used"))
            }),
        );
        if let Some(n) = o.mic_fault_after {
            rec = rec.with_write_fault_after(n);
        }
        tracks.push(Box::new(MicTrack::new(rec)));
        mic = Some(h);
    }

    let mut cursor = None;
    if o.which.cursor {
        let (src, h) = match o.cursor_status {
            Some(st) => FakeCursorSource::new(st),
            None => FakeCursorSource::full(),
        };
        let target = CursorTarget::display(1, Rect::new(0.0, 0.0, 1920.0, 1080.0), 1.0);
        tracks.push(Box::new(CursorTrack::new(CursorRecorder::with_source(
            CursorConfig::new(target),
            Box::new(src),
        ))));
        cursor = Some(h);
    }

    let mut camera = None;
    let mut camera_writers = None;
    if o.which.camera {
        let (src, h) = FakeCameraSource::new((1920, 1080));
        if let Some(e) = o.camera_open_error {
            h.fail_open_with(e);
        }
        let writers = FakeWriterFactory::new();
        tracks.push(Box::new(CameraTrack::new(CameraRecorder::new(
            CameraConfig::default(),
            Box::new(src),
            Arc::new(writers.clone()),
        ))));
        camera = Some(h);
        camera_writers = Some(writers);
    }

    (
        Rig {
            host,
            free,
            root,
            screen,
            mic,
            cursor,
            camera,
            camera_writers,
            next_frame: 0,
            next_cam: 0,
            next_move: 0,
            mic_dev_ms: 0,
            now_ms: 0,
        },
        tracks,
    )
}

impl Rig {
    fn config(&self) -> SessionConfig {
        let mut c = SessionConfig::new(self.root.path());
        c.title = Some("Test".into());
        c.host = self.host.clone();
        c.free_space = self.free.clone();
        c.disk_check_every = Duration::ZERO;
        c
    }

    fn set(&mut self, t_ms: i64) {
        self.now_ms = t_ms;
        self.host
            .set_ns(ORIGIN + u64::try_from(t_ms).unwrap() * 1_000_000);
    }

    /// Delivers everything the fake devices would have produced up to `t_ms`, in order, with the
    /// host clock following. Screen at 30 fps from 250 ms, camera at 30 fps from 300 ms, mic from
    /// 100 ms, cursor moves every 50 ms.
    fn run_until(&mut self, t_ms: i64) {
        while self.now_ms < t_ms {
            let step_end = (self.now_ms + 10).min(t_ms);
            self.set(step_end);
            let host_ns = |ms: i64| ORIGIN + u64::try_from(ms).unwrap() * 1_000_000;
            loop {
                let f = i64::try_from(SCREEN_OFFSET_MS).unwrap() + self.next_frame * 1000 / 30;
                if f > step_end {
                    break;
                }
                // Delivered 8 ms after the capture stamp (the clock is at least that far on).
                self.screen
                    .frame(host_ns(f), u64::try_from(self.next_frame).unwrap());
                self.next_frame += 1;
            }
            if let Some(cam) = &self.camera {
                loop {
                    let f = i64::try_from(CAMERA_OFFSET_MS).unwrap() + self.next_cam * 1000 / 30;
                    if f > step_end {
                        break;
                    }
                    cam.deliver(host_ns(f), 5_000_000);
                    self.next_cam += 1;
                }
            }
            if let Some(cur) = &self.cursor {
                while self.next_move * 50 <= step_end {
                    let x = (self.next_move * 7 % 1900) as f64;
                    cur.mv(host_ns(self.next_move * 50), x, 100.0);
                    self.next_move += 1;
                }
            }
            if let Some(mic) = &self.mic {
                let dev_end = step_end - i64::try_from(MIC_OFFSET_MS).unwrap();
                if dev_end > self.mic_dev_ms {
                    mic.advance((dev_end - self.mic_dev_ms) as f64 / 1000.0);
                    self.mic_dev_ms = dev_end;
                }
            }
        }
    }
}

fn read_manifest(dir: &Path) -> RecManifest {
    let text = std::fs::read_to_string(dir.join("manifest.json")).unwrap();
    RecManifest::from_json(&text)
        .map_err(|_| "bad manifest")
        .unwrap()
}

fn session_json(dir: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(dir.join("session.json")).unwrap()).unwrap()
}

fn track_start_ms(m: &RecManifest, kind: &str) -> f64 {
    m.tracks[kind]["start"].as_i64().unwrap() as f64 / 120.0
}

fn track_end_ms(m: &RecManifest, kind: &str) -> f64 {
    m.tracks[kind]["end"].as_i64().unwrap() as f64 / 120.0
}

// ---- tests ------------------------------------------------------------------------------------

#[test]
fn rec1_to_rec4_all_tracks_start_pause_resume_stop() {
    let (mut rig, tracks) = build(Options::default());
    let mut s = RecordingSession::begin(rig.config(), tracks).unwrap();
    let dir = s.dir().to_owned();
    assert_eq!(s.state(), SessionState::Recording);
    assert!(
        dir.join(LOCK_FILE).exists(),
        "the lock marks a live recording"
    );
    assert!(dir.extension().is_some_and(|e| e == "cmrec"));

    rig.run_until(1500);
    let st = s.status();
    assert_eq!(st.tracks.len(), 4);
    assert!(st.tracks.iter().all(|t| t.state == TrackState::Running));
    assert!(
        st.tracks
            .iter()
            .find(|t| t.kind == TrackKind::Screen)
            .unwrap()
            .stats
            .written
            > 30
    );
    s.pause().unwrap();
    assert_eq!(s.state(), SessionState::Paused);
    assert!(s.pause().is_err(), "double pause is refused");
    rig.run_until(2500);
    s.resume().unwrap();
    assert!(s.resume().is_err(), "resume without pause is refused");
    assert_eq!(
        s.gaps(),
        &[PauseGap {
            from: ms(1500),
            to: ms(2500)
        }]
    );
    rig.run_until(4000);
    let stopped = s.stop().unwrap();
    assert_eq!(s.state(), SessionState::Stopped);
    assert_eq!(stopped.paused, ms(1000));
    assert_eq!(stopped.wall_end, ms(4000));
    let (manifest, dir2) = stopped.finalize().unwrap();
    assert_eq!(dir, dir2);
    assert!(!dir.join(LOCK_FILE).exists(), "finalize removes the lock");

    // Manifest completeness.
    let m = read_manifest(&dir);
    assert_eq!(m.id, manifest.id);
    for k in ["screen", "mic", "cursor", "camera"] {
        assert!(m.tracks.contains_key(k), "manifest has {k}");
    }
    assert_eq!(
        m.gaps,
        vec![PauseGap {
            from: ms(1500),
            to: ms(2500)
        }]
    );
    // 4 s of wall clock minus the 1 s pause.
    assert!(
        (secs(m.duration.unwrap()) - 3.0).abs() < 0.05,
        "duration {:?}",
        m.duration
    );
    assert_eq!(m.extra["live_at"], 0);
    assert!(m.extra.get("warnings").is_none(), "{:?}", m.extra);

    // Clock/origin alignment: every track keeps its own first-sample stamp on the one clock.
    assert!((track_start_ms(&m, "screen") - 250.0).abs() < 20.0);
    assert!((track_start_ms(&m, "mic") - 100.0).abs() < 2.0);
    assert!((track_start_ms(&m, "camera") - 300.0).abs() < 40.0);
    assert!(track_start_ms(&m, "cursor") >= 0.0);

    // session.json carries the origin and the live_at offset.
    let sj = session_json(&dir);
    assert_eq!(sj["clock"]["origin_ns"], ORIGIN);
    assert_eq!(sj["session"]["live_at"], 0);
    assert_eq!(sj["session"]["origin_ns"], ORIGIN);
    assert_eq!(sj["session"]["started_tracks"].as_array().unwrap().len(), 4);
    assert_eq!(sj["devices"]["mic"]["name"], "Fake Mic");

    // Screen: gap removed from the file, constant frame rate.
    let scan = scan_file(&dir.join("screen.mp4")).unwrap();
    assert!((secs(scan.start().unwrap()) - 0.25).abs() < 0.04);
    assert!(
        (secs(scan.end().unwrap()) - 3.0).abs() < 0.05,
        "file end {:?}",
        scan.end()
    );
    let expected_frames = (3.0 - 0.25) * 30.0;
    assert!(
        (scan.frame_count() as f64 - expected_frames).abs() <= 3.0,
        "{} frames",
        scan.frame_count()
    );

    // Mic: no hole in the file; 4 s - 0.1 s start - 1 s pause.
    let wav = cm_audio::read_info(&dir.join("mic.wav")).unwrap();
    assert_eq!(wav.sample_rate, 48_000);
    let want = (4.0 - 0.1 - 1.0) * 48_000.0;
    assert!(
        (wav.frames() as f64 - want).abs() < 1500.0,
        "{} frames, want {want}",
        wav.frames()
    );

    // Cursor: parses, in session time, nothing inside the pause except the re-anchor.
    let log = CursorLog::read(&dir.join("cursor.jsonl")).unwrap();
    assert!(!log.truncated);
    assert!(!log.events.is_empty());
    let end = 4000 * 120;
    assert!(log.events.iter().all(|e| (0..=end).contains(&e.t())));
    assert!(
        log.events
            .iter()
            .all(|e| !(e.t() > 1500 * 120 + 120 && e.t() < 2500 * 120)),
        "no cursor events inside the pause"
    );

    // Camera: frames written, none in the pause, and (like the screen) no hole in the file.
    let cam = &m.tracks["camera"]["stats"];
    assert!(cam["written"].as_u64().unwrap() > 60, "{cam}");
    let log = rig.camera_writers.as_ref().unwrap().log();
    // File time has the 1 s gap removed: continuous PTS, one frame interval apart, no hole.
    assert!(log
        .appended
        .windows(2)
        .all(|w| w[1].0 - w[0].0 == w[0].1 || w[1].0 - w[0].0 <= ms(40)));
    let first = log.appended.first().unwrap().0;
    let last_end = log.appended.last().map(|(p, d)| *p + *d).unwrap();
    assert!((secs(first) - 0.3).abs() < 0.04, "first {first:?}");
    // 4 s wall minus the 1 s pause (the camera's last frame ends a frame or so before the stop).
    assert!((secs(last_end) - 3.0).abs() < 0.1, "end {last_end:?}");
    // Manifest start/end are session time, like the screen's; the gap maps them.
    assert!(
        (secs(last_end) - (track_end_ms(&m, "camera") / 1000.0 - 1.0)).abs() < 0.05,
        "file end + gap == manifest end"
    );
    assert_eq!(m.tracks["camera"]["pauses"][0]["from"], 1500 * 120);
    let frag = &m.tracks["camera"]["fragments"];
    assert!(frag["max_s"].as_f64().unwrap() <= 2.05, "{frag}");
}

/// REC-2: there is no software fallback. When the hardware encoder rejects the plan the start
/// fails with `platform_rejected` and the stable reason `hardware_encoder_unavailable`, no bundle
/// is left and the tracks that did start are stopped.
#[test]
fn rec2_encoder_rejection_fails_the_start_cleanly() {
    let (rig, tracks) = build(Options {
        reject_hardware: true,
        ..Default::default()
    });
    let mic = rig.mic.clone().unwrap();
    let cam = rig.camera.clone().unwrap();
    let e = RecordingSession::begin(rig.config(), tracks)
        .err()
        .expect("must fail");
    assert_eq!(e.code(), ErrorCode::PlatformRejected);
    assert!(is_hardware_encoder_unavailable(&e), "{e:?}");
    assert_eq!(
        e.data(),
        Some(serde_json::json!({ "platform_code": "hardware_encoder_unavailable" }))
    );
    assert!(e.to_string().contains("no usable hardware H.264 encoder"));
    assert_eq!(
        std::fs::read_dir(rig.root.path()).unwrap().count(),
        0,
        "no bundle is left"
    );
    assert!(!mic.is_started(), "mic stopped");
    assert!(!cam.is_open(), "camera closed");
}

#[test]
fn rec2_no_rejection_means_no_encoder_warning() {
    let (rig, tracks) = build(Options {
        which: SCREEN_CURSOR,
        ..Default::default()
    });
    let s = RecordingSession::begin(rig.config(), tracks).unwrap();
    assert!(!s.warnings().iter().any(|w| w.code.contains("encoder")));
}

#[test]
fn stop_while_paused_closes_the_final_gap() {
    let (mut rig, tracks) = build(Options::default());
    let mut s = RecordingSession::begin(rig.config(), tracks).unwrap();
    rig.run_until(1000);
    s.pause().unwrap();
    rig.run_until(2000);
    let stopped = s.stop().unwrap();
    assert_eq!(
        stopped.gaps,
        vec![PauseGap {
            from: ms(1000),
            to: ms(2000)
        }]
    );
    let (m, dir) = stopped.finalize().unwrap();
    assert_eq!(m.gaps.len(), 1);
    // R3-6 / REC-4.AC5: every track lists the same final gap under `pauses` (the session's), and
    // `gaps` is not a pause list any more.
    for kind in ["screen", "camera", "mic", "cursor"] {
        let p = m.tracks[kind]["pauses"].as_array().unwrap();
        assert_eq!(p.len(), 1, "{kind}: {p:?}");
        assert_eq!(p[0]["from"], 1000 * 120, "{kind}");
        assert_eq!(p[0]["to"], 2000 * 120, "{kind}");
    }
    assert!(m.tracks["screen"].get("gaps").is_none());
    assert!(m.tracks["cursor"].get("pause_gaps").is_none());
    let scan = scan_file(&dir.join("screen.mp4")).unwrap();
    assert!(
        (secs(scan.end().unwrap()) - 1.0).abs() < 0.05,
        "the screen file ends at the pause point: {:?}",
        scan.end()
    );
}

#[test]
fn rec1_screen_failure_aborts_and_leaves_nothing_behind() {
    let (rig, tracks) = build(Options {
        screen_permission: false,
        ..Default::default()
    });
    let e = RecordingSession::begin(rig.config(), tracks)
        .err()
        .expect("must fail");
    assert_eq!(e.code(), ErrorCode::PermissionMissing);
    assert_eq!(
        std::fs::read_dir(rig.root.path()).unwrap().count(),
        0,
        "no bundle is left"
    );
}

#[test]
fn screen_failure_stops_the_tracks_that_did_start() {
    let (rig, tracks) = build(Options {
        screen_permission: false,
        ..Default::default()
    });
    let mic = rig.mic.clone().unwrap();
    let cam = rig.camera.clone().unwrap();
    assert!(RecordingSession::begin(rig.config(), tracks).is_err());
    assert!(!mic.is_started(), "mic stopped");
    assert!(!cam.is_open(), "camera closed");
}

#[test]
fn optional_tracks_that_fail_to_start_are_warnings_not_aborts() {
    let (mut rig, tracks) = build(Options {
        camera_open_error: Some(CmError::device_busy("camera is busy")),
        mic_start_error: Some(CmError::internal("no mic")),
        ..Default::default()
    });
    let mut s = RecordingSession::begin(rig.config(), tracks).unwrap();
    let dir = s.dir().to_owned();
    let codes: Vec<_> = s.warnings().iter().map(|w| w.code.clone()).collect();
    assert!(codes.contains(&"camera_failed".to_string()), "{codes:?}");
    assert!(codes.contains(&"mic_failed".to_string()), "{codes:?}");
    let st = s.status();
    let state = |k| {
        st.tracks
            .iter()
            .find(|t| t.kind == k)
            .unwrap()
            .state
            .clone()
    };
    assert!(matches!(state(TrackKind::Camera), TrackState::Failed(_)));
    assert!(matches!(state(TrackKind::Mic), TrackState::Failed(_)));
    assert_eq!(state(TrackKind::Screen), TrackState::Running);

    rig.run_until(2000);
    s.pause().unwrap();
    s.resume().unwrap();
    rig.run_until(3000);
    let (m, _) = s.stop().unwrap().finalize().unwrap();
    assert!(m.tracks.contains_key("screen") && m.tracks.contains_key("cursor"));
    assert!(!m.tracks.contains_key("camera") && !m.tracks.contains_key("mic"));
    let w = m.extra["warnings"].as_array().unwrap();
    assert_eq!(w.len(), 2, "{w:?}");
    assert!(w
        .iter()
        .any(|x| x["track"] == "camera" && x["code"] == "camera_failed"));
    assert!(!dir.join("camera.mp4").exists());
    assert!(!dir.join("mic.wav").exists());
    assert_eq!(
        m.extra["session"]["requested_tracks"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    assert_eq!(
        m.extra["session"]["recorded_tracks"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn a_camera_with_no_frames_is_dropped_at_stop_with_a_warning() {
    let (mut rig, tracks) = build(Options::default());
    rig.camera = None; // never deliver a camera frame
    let mut s = RecordingSession::begin(rig.config(), tracks).unwrap();
    rig.run_until(1500);
    let stopped = s.stop().unwrap();
    assert!(stopped.tracks.iter().all(|t| t.kind != TrackKind::Camera));
    assert!(stopped.warnings.iter().any(|w| w.code == "camera_failed"));
    let (m, dir) = stopped.finalize().unwrap();
    assert!(!m.tracks.contains_key("camera"));
    assert!(m.extra["warnings"].as_array().unwrap().len() == 1);
    assert!(!dir.join("camera.mp4").exists());
}

#[test]
fn rec7_ac4_degraded_cursor_is_a_warning_and_session_json_says_so() {
    let (mut rig, tracks) = build(Options {
        which: SCREEN_CURSOR,
        cursor_status: Some(CursorStatus::permission_fallback(
            cm_core::Permission::InputMonitoring,
            "Input Monitoring is off",
        )),
        ..Default::default()
    });
    let mut s = RecordingSession::begin(rig.config(), tracks).unwrap();
    assert!(s.warnings().iter().any(|w| w.code == "cursor_degraded"));
    let sj = session_json(s.dir());
    assert_eq!(sj["cursor"]["clicks"], "unavailable");
    assert_eq!(sj["cursor"]["permission_missing"], "input_monitoring");
    rig.run_until(500);
    drop(s.stop().unwrap().finalize().unwrap());
}

#[test]
fn tracks_start_concurrently_and_live_at_is_when_the_last_one_returned() {
    struct Slow(TrackKind, u64);
    impl TrackRecorder for Slow {
        fn kind(&self) -> TrackKind {
            self.0
        }
        fn start(&mut self, ctx: cm_core::TrackContext) -> Result<(), CmError> {
            std::thread::sleep(Duration::from_millis(self.1));
            std::fs::write(ctx.dir.join(format!("{}.dat", self.0.as_str())), b"x").unwrap();
            Ok(())
        }
        fn pause(&mut self, _: MediaTime) -> Result<(), CmError> {
            Ok(())
        }
        fn resume(&mut self, _: MediaTime) -> Result<(), CmError> {
            Ok(())
        }
        fn stop(&mut self, at: MediaTime) -> Result<cm_core::TrackInfo, CmError> {
            Ok(cm_core::TrackInfo {
                kind: self.0,
                file: format!("{}.dat", self.0.as_str()),
                start: ms(0),
                end: at,
                details: Value::Null,
                stats: Default::default(),
            })
        }
        fn stats(&self) -> cm_core::TrackStats {
            Default::default()
        }
    }
    let root = tempfile::tempdir().unwrap();
    let mut cfg = SessionConfig::new(root.path());
    cfg.title = Some("slow".into());
    let tracks: Tracks = vec![
        Box::new(PlainTrack::new(
            Box::new(Slow(TrackKind::Screen, 200)),
            true,
        )),
        Box::new(PlainTrack::new(
            Box::new(Slow(TrackKind::Camera, 1500)),
            false,
        )),
        Box::new(PlainTrack::new(Box::new(Slow(TrackKind::Mic, 900)), false)),
    ];
    let t0 = Instant::now();
    let mut s = RecordingSession::begin(cfg, tracks).unwrap();
    let took = t0.elapsed();
    // The bounds are far apart on purpose (R3 flaky test): sequential starts would take the sum
    // (2600 ms); concurrent ones the slowest (1500 ms). A loaded machine adds scheduling noise,
    // never a second.
    assert!(
        took < Duration::from_millis(2300),
        "starts must overlap (sum is 2600 ms), took {took:?}"
    );
    let live = s.live_at();
    assert!(
        live >= ms(1490) && live < ms(2300),
        "live_at is the slowest start: {live:?}"
    );
    let sj = session_json(s.dir());
    assert_eq!(sj["session"]["live_at"], live.ticks());
    let start_ms = &sj["session"]["start_ms"];
    assert!(start_ms["camera"].as_f64().unwrap() >= 1490.0);
    assert!(start_ms["screen"].as_f64().unwrap() < 1000.0);
    // The HUD clock starts at live_at: it reads (time since begin returned), not the start-up.
    assert!(s.status().recorded < ms(1000));
    let (m, _) = s.stop().unwrap().finalize().unwrap();
    assert_eq!(m.extra["live_at"], live.ticks());
}

#[test]
fn discard_stops_everything_and_removes_the_bundle() {
    let (rig, tracks) = build(Options::default());
    let mic = rig.mic.clone().unwrap();
    let s = RecordingSession::begin(rig.config(), tracks).unwrap();
    let dir = s.dir().to_owned();
    assert!(dir.exists());
    s.discard();
    assert!(!dir.exists());
    assert!(!mic.is_started());
}

#[test]
fn aud_hud_gets_a_live_mic_level() {
    let (mut rig, tracks) = build(Options::default());
    let mut s = RecordingSession::begin(rig.config(), tracks).unwrap();
    rig.run_until(800);
    let lv = s.level_handle(TrackKind::Mic).unwrap().read();
    assert!(lv.peak_dbfs > -7.0 && lv.peak_dbfs < -5.0, "{lv:?}");
    let st = s.status();
    let mic = st.tracks.iter().find(|t| t.kind == TrackKind::Mic).unwrap();
    assert!(mic.level.unwrap().peak_dbfs > -7.0);
    assert!(s.level_handle(TrackKind::Screen).is_none());
    let _ = s.stop();
}

#[test]
fn camera_loss_is_a_manifest_warning_and_the_recording_goes_on() {
    let (mut rig, tracks) = build(Options::default());
    let mut s = RecordingSession::begin(rig.config(), tracks).unwrap();
    rig.run_until(1000);
    rig.camera
        .as_ref()
        .unwrap()
        .event(CameraEvent::Disconnected);
    let notes = s.tick();
    assert!(
        notes
            .iter()
            .any(|n| n.code == "camera_lost" && n.level == NoteLevel::Warning),
        "{notes:?}"
    );
    rig.run_until(2000);
    let (m, _) = s.stop().unwrap().finalize().unwrap();
    assert!(m.tracks.contains_key("camera"));
    let w = m.extra["warnings"].as_array().unwrap();
    assert!(w.iter().any(|x| x["code"] == "camera_lost"), "{w:?}");
}

#[test]
fn rec1_screen_target_closed_requests_a_stop_and_keeps_the_file() {
    let (mut rig, tracks) = build(Options {
        which: SCREEN_CURSOR,
        ..Default::default()
    });
    let mut s = RecordingSession::begin(rig.config(), tracks).unwrap();
    rig.run_until(2500);
    rig.screen.close_target("window closed");
    let notes = s.tick();
    assert!(
        notes
            .iter()
            .any(|n| n.level == NoteLevel::StopRequested && n.code == "screen_halted"),
        "{notes:?}"
    );
    assert_eq!(s.stop_requested(), Some("screen_halted"));
    rig.run_until(3000);
    let (m, dir) = s.stop().unwrap().finalize().unwrap();
    assert!(m.tracks.contains_key("screen"));
    assert!(m.tracks["screen"]["stopped_early"].is_object());
    assert!(scan_file(&dir.join("screen.mp4")).unwrap().frame_count() > 30);
}

#[test]
fn nfr7_disk_guard_warns_then_requests_a_stop() {
    let (mut rig, tracks) = build(Options {
        which: SCREEN_CURSOR,
        // 4K60-class bitrate so that "under 10 minutes" is reachable just above 1 GB.
        width: 3840,
        height: 2160,
        ..Default::default()
    });
    let mut s = RecordingSession::begin(rig.config(), tracks).unwrap();
    rig.run_until(500);
    assert!(s.tick().is_empty());
    assert!(s.status().disk_minutes_left.unwrap() > 10.0);

    rig.free.set(1_300_000_000);
    rig.run_until(600);
    let n = s.tick();
    assert!(
        n.iter()
            .any(|n| n.code == "disk_low" && n.level == NoteLevel::Warning),
        "{n:?}"
    );
    assert_eq!(
        n.iter().filter(|n| n.code == "disk_low").count(),
        1,
        "R3-15: the screen's and the session's disk check report one warning together: {n:?}"
    );
    assert!(
        s.tick().iter().all(|n| n.code != "disk_low"),
        "reported once"
    );
    assert_eq!(s.stop_requested(), None);

    rig.free.set(900_000_000);
    rig.run_until(700);
    let n = s.tick();
    assert!(
        n.iter()
            .any(|n| n.code == "disk_full" && n.level == NoteLevel::StopRequested),
        "{n:?}"
    );
    assert_eq!(s.stop_requested(), Some("disk_full"));
    // Stopping still produces a valid, complete bundle.
    let (m, _) = s.stop().unwrap().finalize().unwrap();
    let codes: Vec<_> = m.extra["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["code"].as_str().unwrap().to_owned())
        .collect();
    assert!(codes.contains(&"disk_low".into()) && codes.contains(&"disk_full".into()));
}

#[test]
fn nfr7_a_nearly_full_disk_refuses_to_start() {
    let (rig, tracks) = build(Options::default());
    rig.free.set(800_000_000);
    let e = RecordingSession::begin(rig.config(), tracks).err().unwrap();
    assert_eq!(e.code(), ErrorCode::DiskFull);
}

#[test]
fn track_kinds_are_validated() {
    let (rig, mut tracks) = build(Options {
        which: SCREEN_CURSOR,
        ..Default::default()
    });
    tracks.remove(0);
    let e = RecordingSession::begin(rig.config(), tracks).err().unwrap();
    assert_eq!(e.code(), ErrorCode::InvalidInput);
}

#[test]
fn rec8_ac1_crash_after_a_pause_then_recover_keeps_media_gaps_and_warnings() {
    let (mut rig, tracks) = build(Options {
        which: Which {
            mic: true,
            camera: false,
            cursor: true,
        },
        mic_start_error: None,
        ..Default::default()
    });
    let mut s = RecordingSession::begin(rig.config(), tracks).unwrap();
    let dir = s.dir().to_owned();
    rig.run_until(2000);
    s.pause().unwrap();
    rig.run_until(2500);
    s.resume().unwrap();
    rig.run_until(7000);
    // A warning raised before the crash survives in the lock.
    rig.free.set(1_200_000_000);
    let _ = s.tick();

    // Crash: the process dies without stop(). Leaking the session keeps its files exactly as
    // they are on disk (the open fragment of each track is lost); the writer threads need a
    // moment to put the finished fragments down (the cursor writer flushes every 800 ms of real time).
    std::mem::forget(s);
    std::thread::sleep(Duration::from_millis(1200));
    let screen = dir.join("screen.mp4");
    // The owner is gone: its owner lock and a dead pid (`forget` leaked the open handle here).
    let _ = std::fs::remove_file(dir.join(cm_library::rec::OWNER_FILE));
    let lock_path = dir.join(LOCK_FILE);
    let mut lock: Value =
        serde_json::from_str(&std::fs::read_to_string(&lock_path).unwrap()).unwrap();
    lock["pid"] = Value::from(4_000_000u32);
    std::fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let mut lib = Library::open_or_create(rig.root.path()).unwrap();
    let summary = lib.recover_incomplete().unwrap();
    assert_eq!(summary.recovered.len(), 1, "{summary:?}");
    let rep = &summary.recovered[0];
    assert!(rep.lost_ms_max <= 2000);
    assert!(!dir.join(LOCK_FILE).exists());

    let m = read_manifest(&dir);
    assert_eq!(
        m.gaps,
        vec![PauseGap {
            from: ms(2000),
            to: ms(2500)
        }]
    );
    assert!(m.tracks.contains_key("screen") && m.tracks.contains_key("cursor"));
    assert!(m.tracks.contains_key("mic"));
    // R3-2 / REC-8.AC2: the recovered mic keeps its start offset (the 100 ms of the fake device,
    // not 0), its channels, the denoise-less file and the pause that falls inside it.
    let mic = &m.tracks["mic"];
    assert!(
        (track_start_ms(&m, "mic") - 100.0).abs() < 2.0,
        "mic start {}",
        mic["start"]
    );
    assert_eq!(mic["file"], "mic.wav");
    assert_eq!(mic["channels"], 1);
    assert_eq!(mic["pauses"][0]["from"], 2000 * 120);
    assert_eq!(mic["pauses"][0]["to"], 2500 * 120);
    assert!(mic.get("start_unknown").is_none());
    assert!(m.extra.contains_key("recovered"));
    assert_eq!(m.extra["live_at"], 0, "extras survive recovery");
    assert!(
        m.extra["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w["code"] == "disk_low" || w["code"] == "disk_full"),
        "{:?}",
        m.extra.get("warnings")
    );
    // Recorded 7 s - 0.5 s pause; at most one 2 s fragment (plus the cut one) is lost.
    let scan = scan_file(&screen).unwrap();
    let file_s = secs(scan.end().unwrap());
    assert!(file_s >= 6.5 - 2.1 - 0.1, "recovered {file_s} s");
    assert!(file_s <= 6.6);
    // The cursor log is complete up to its last whole line.
    let log = CursorLog::read(&dir.join("cursor.jsonl")).unwrap();
    assert!(log
        .events
        .iter()
        .any(|e| matches!(e, CursorEvent::Move { .. })));
    assert!(
        lib.get(&rep.id).is_ok(),
        "the recovered recording is indexed"
    );
    let _ = (Button::Left, MoveSource::Fake);
}

#[test]
fn the_stopped_session_is_indexed_in_the_library() {
    let root = tempfile::tempdir().unwrap();
    let mut lib = Library::open_or_create(root.path()).unwrap();
    let (mut rig, tracks) = build(Options {
        which: SCREEN_CURSOR,
        ..Default::default()
    });
    rig.root = root;
    let mut s = RecordingSession::begin(rig.config(), tracks).unwrap();
    rig.run_until(1500);
    let rec = s.stop().unwrap().finish_into(&mut lib).unwrap();
    assert_eq!(rec.kind, cm_library::manifest::CaptureKind::Recording);
    assert!(rec.duration.unwrap() > ms(1000));
    assert_eq!(lib.count().unwrap(), 1);
    let _ = Mutex::new(());
}

/// A scripted track for the failure paths of the session.
struct Probe {
    kind: TrackKind,
    file: &'static str,
    /// Called from `start` with the bundle folder.
    on_start: Box<dyn FnMut(&Path) + Send>,
    fail_stop: bool,
    stopped: Arc<std::sync::atomic::AtomicBool>,
}

impl TrackRecorder for Probe {
    fn kind(&self) -> TrackKind {
        self.kind
    }
    fn start(&mut self, ctx: cm_core::TrackContext) -> Result<(), CmError> {
        std::fs::write(ctx.dir.join(self.file), b"audio so far").unwrap();
        (self.on_start)(&ctx.dir);
        Ok(())
    }
    fn pause(&mut self, _at: MediaTime) -> Result<(), CmError> {
        Ok(())
    }
    fn resume(&mut self, _at: MediaTime) -> Result<(), CmError> {
        Ok(())
    }
    fn stop(&mut self, _at: MediaTime) -> Result<cm_core::TrackInfo, CmError> {
        self.stopped
            .store(true, std::sync::atomic::Ordering::SeqCst);
        if self.fail_stop {
            return Err(CmError::internal("the last flush failed"));
        }
        Ok(cm_core::TrackInfo {
            kind: self.kind,
            file: self.file.into(),
            start: MediaTime::ZERO,
            end: MediaTime::ZERO,
            details: serde_json::json!({}),
            stats: cm_core::TrackStats::default(),
        })
    }
    fn stats(&self) -> cm_core::TrackStats {
        cm_core::TrackStats::default()
    }
}

fn probe(
    kind: TrackKind,
    file: &'static str,
    fail_stop: bool,
    on_start: impl FnMut(&Path) + Send + 'static,
) -> (Box<dyn SessionTrack>, Arc<std::sync::atomic::AtomicBool>) {
    let stopped = Arc::new(std::sync::atomic::AtomicBool::new(false));
    (
        Box::new(PlainTrack::new(
            Box::new(Probe {
                kind,
                file,
                on_start: Box::new(on_start),
                fail_stop,
                stopped: stopped.clone(),
            }),
            false,
        )),
        stopped,
    )
}

/// R3-1: an optional track that fails to finish at stop keeps its files (hours of audio must not
/// vanish because the last flush failed); only a track that failed to START is cleaned up.
#[test]
fn r3_1_a_track_that_fails_at_stop_keeps_its_files() {
    let (mut rig, mut tracks) = build(Options {
        which: SCREEN_CURSOR,
        ..Default::default()
    });
    let (sys, _) = probe(TrackKind::System, "system.wav", true, |_| {});
    tracks.push(sys);
    let mut s = RecordingSession::begin(rig.config(), tracks).unwrap();
    let dir = s.dir().to_owned();
    rig.run_until(1500);
    let stopped = s.stop().unwrap();
    assert!(
        dir.join("system.wav").exists(),
        "the failed track's file is kept"
    );
    let w = stopped
        .warnings
        .iter()
        .find(|w| w.code == "system_failed")
        .expect("a warning");
    assert!(w.message.contains("files were kept"), "{}", w.message);
    let (m, _) = stopped.finalize().unwrap();
    assert!(!m.tracks.contains_key("system"));
    assert!(dir.join("system.wav").exists());
}

/// R3-1: a write error mid-recording ends the mic track where its file ends; the file stays, the
/// manifest says where (`truncated_at`) and the session warns once with `track_failed`.
#[test]
fn r3_1_a_disk_write_error_truncates_the_mic_track_and_keeps_the_audio() {
    let (mut rig, tracks) = build(Options {
        which: Which {
            mic: true,
            camera: false,
            cursor: false,
        },
        mic_fault_after: Some(96_000), // 0.5 s of mono f32
        ..Default::default()
    });
    let mut s = RecordingSession::begin(rig.config(), tracks).unwrap();
    let dir = s.dir().to_owned();
    rig.run_until(3000);
    // The recorder reports the failure while it runs.
    std::thread::sleep(Duration::from_millis(200));
    let notes = s.tick();
    let (m, _) = s.stop().unwrap().finalize().unwrap();
    assert!(
        notes.iter().any(|n| n.code == "track_failed"
            && n.track == Some(TrackKind::Mic)
            && n.level == NoteLevel::Warning),
        "{notes:?}"
    );
    let w = m.extra["warnings"].as_array().unwrap();
    assert_eq!(
        w.iter().filter(|x| x["code"] == "track_failed").count(),
        1,
        "{w:?}"
    );
    let mic = &m.tracks["mic"];
    let t = mic["truncated_at"].as_i64().unwrap();
    assert!(t > 0 && t < 3000 * 120, "{t}");
    let wav = cm_audio::read_info(&dir.join("mic.wav")).unwrap();
    assert!(
        wav.frames() > 20_000 && wav.frames() < 25_000,
        "{}",
        wav.frames()
    );
    assert!(mic["end"].as_i64().unwrap() <= t);
}

/// R3-14: if the session cannot write its own metadata after the tracks started, the tracks are
/// stopped (not left running with no owner) and the half-made bundle is removed.
#[test]
fn r3_14_a_metadata_failure_stops_the_tracks_that_started() {
    let (rig, mut tracks) = build(Options::default());
    let mic = rig.mic.clone().unwrap();
    // This track sabotages the bundle: session.json becomes a folder, so the rewrite fails.
    let (bad, bad_stopped) = probe(TrackKind::System, "system.wav", false, |dir| {
        let p = dir.join("session.json");
        std::fs::remove_file(&p).unwrap();
        std::fs::create_dir(&p).unwrap();
    });
    tracks.push(bad);
    let e = RecordingSession::begin(rig.config(), tracks).err().unwrap();
    assert!(!e.to_string().is_empty());
    assert!(!mic.is_started(), "the mic was stopped");
    assert!(bad_stopped.load(std::sync::atomic::Ordering::SeqCst));
    assert!(!rig.camera.clone().unwrap().is_open());
    assert_eq!(
        std::fs::read_dir(rig.root.path()).unwrap().count(),
        0,
        "no half-made bundle is left"
    );
}

/// R3-5: stopping before the first frame deletes the empty bundle (no lock left for recovery to
/// retry on every launch) and returns the "no frames" error.
#[test]
fn r3_5_stop_before_the_first_frame_deletes_the_empty_bundle() {
    struct NoFrames;
    impl TrackRecorder for NoFrames {
        fn kind(&self) -> TrackKind {
            TrackKind::Screen
        }
        fn start(&mut self, ctx: cm_core::TrackContext) -> Result<(), CmError> {
            std::fs::write(ctx.dir.join("screen.mp4"), b"init only").unwrap();
            Ok(())
        }
        fn pause(&mut self, _: MediaTime) -> Result<(), CmError> {
            Ok(())
        }
        fn resume(&mut self, _: MediaTime) -> Result<(), CmError> {
            Ok(())
        }
        fn stop(&mut self, _: MediaTime) -> Result<cm_core::TrackInfo, CmError> {
            Err(cm_encode::no_frames_error())
        }
        fn stats(&self) -> cm_core::TrackStats {
            Default::default()
        }
    }
    let root = tempfile::tempdir().unwrap();
    let mut cfg = SessionConfig::new(root.path());
    cfg.title = Some("empty".into());
    let tracks: Tracks = vec![Box::new(PlainTrack::new(Box::new(NoFrames), true))];
    let mut s = RecordingSession::begin(cfg, tracks).unwrap();
    let dir = s.dir().to_owned();
    let e = s.stop().expect_err("must fail");
    assert!(cm_encode::is_no_frames(&e), "{e}");
    assert!(!dir.exists(), "the empty bundle is gone");
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
}

/// R3-5: a screen file that ended early because the disk failed at the end (the writer halted)
/// is finalized, with a warning; the take is not lost.
#[test]
fn r3_5_a_writer_halt_at_stop_is_finalized_with_a_warning() {
    struct Halted;
    impl TrackRecorder for Halted {
        fn kind(&self) -> TrackKind {
            TrackKind::Screen
        }
        fn start(&mut self, ctx: cm_core::TrackContext) -> Result<(), CmError> {
            std::fs::write(ctx.dir.join("screen.mp4"), b"x").unwrap();
            Ok(())
        }
        fn pause(&mut self, _: MediaTime) -> Result<(), CmError> {
            Ok(())
        }
        fn resume(&mut self, _: MediaTime) -> Result<(), CmError> {
            Ok(())
        }
        fn stop(&mut self, at: MediaTime) -> Result<cm_core::TrackInfo, CmError> {
            Ok(cm_core::TrackInfo {
                kind: TrackKind::Screen,
                file: "screen.mp4".into(),
                start: ms(0),
                end: at,
                details: serde_json::json!({
                    "stopped_early": { "reason": "disk_full" },
                    "writer_halt": { "reason": "disk_full", "message": "no space left" },
                }),
                stats: Default::default(),
            })
        }
        fn stats(&self) -> cm_core::TrackStats {
            Default::default()
        }
    }
    let root = tempfile::tempdir().unwrap();
    let mut cfg = SessionConfig::new(root.path());
    cfg.title = Some("full".into());
    let tracks: Tracks = vec![Box::new(PlainTrack::new(Box::new(Halted), true))];
    let mut s = RecordingSession::begin(cfg, tracks).unwrap();
    let stopped = s.stop().unwrap();
    assert!(
        stopped
            .warnings
            .iter()
            .any(|w| w.code == "screen_stopped_early" && w.message.contains("no space left")),
        "{:?}",
        stopped.warnings
    );
    let (m, dir) = stopped.finalize().unwrap();
    assert!(dir.join("manifest.json").exists());
    assert!(!dir.join(LOCK_FILE).exists());
    assert!(m.tracks.contains_key("screen"));
}

/// R3-21: Screen Recording access revoked mid-recording (SCK `didStopWithError`): the recorder
/// halts, the session asks to stop (`screen_halted`) and keeps what was captured.
#[test]
fn r3_21_a_screen_stream_error_halts_and_keeps_the_file() {
    let (mut rig, tracks) = build(Options {
        which: SCREEN_CURSOR,
        ..Default::default()
    });
    let mut s = RecordingSession::begin(rig.config(), tracks).unwrap();
    rig.run_until(2000);
    rig.screen
        .stop_with_error("The user declined to share the screen");
    let notes = s.tick();
    let n = notes
        .iter()
        .find(|n| n.code == "screen_halted")
        .expect("screen_halted");
    assert_eq!(n.level, NoteLevel::StopRequested);
    assert!(n.message.contains("declined"), "{}", n.message);
    let (m, dir) = s.stop().unwrap().finalize().unwrap();
    assert_eq!(
        m.tracks["screen"]["stopped_early"]["reason"],
        "source_stopped"
    );
    assert!(scan_file(&dir.join("screen.mp4")).unwrap().frame_count() > 30);
}

/// R3-21: a camera runtime error (AVCaptureSession runtime-error notification) is a warning with
/// a gap; the recording is not stopped for an optional track.
#[test]
fn r3_21_a_camera_runtime_error_warns_and_does_not_stop() {
    let (mut rig, tracks) = build(Options::default());
    let mut s = RecordingSession::begin(rig.config(), tracks).unwrap();
    rig.run_until(1000);
    rig.camera
        .as_ref()
        .unwrap()
        .event(CameraEvent::Error("capture session runtime error".into()));
    let notes = s.tick();
    assert!(
        notes
            .iter()
            .any(|n| n.code == "camera_lost" && n.level == NoteLevel::Warning),
        "{notes:?}"
    );
    assert_eq!(
        s.stop_requested(),
        None,
        "optional tracks never stop the recording"
    );
    rig.run_until(2000);
    assert_eq!(s.state(), SessionState::Recording);
    let _ = s.stop();
}

// ---- AUD-6: mic mute ---------------------------------------------------------------------------

/// The f32 samples of a mono WAV the recorder wrote (RIFF header, one `data` chunk).
fn wav_samples(path: &Path) -> Vec<f32> {
    let b = std::fs::read(path).unwrap();
    let at = b.windows(4).position(|w| w == b"data").unwrap() + 8;
    b[at..]
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| f32::from_le_bytes(*c))
        .collect()
}

const MUTE_MIC: Which = Which {
    mic: true,
    camera: false,
    cursor: true,
};

/// AUD-6: the muted spans reach the files exactly (digital silence for the muted frames, the
/// 0.5 constant of the fake mic elsewhere, a gapless file across the pause), the manifest lists
/// them in session time next to the pause gaps, and the track is never restarted.
#[test]
fn aud6_muted_spans_reach_the_file_and_the_manifest_across_a_pause() {
    let (mut rig, tracks) = build(Options {
        which: MUTE_MIC,
        ..Default::default()
    });
    let mut s = RecordingSession::begin(rig.config(), tracks).unwrap();
    let dir = s.dir().to_owned();
    assert!(!s.status().mic_muted);
    rig.run_until(2000);
    let m = s.set_mic_muted(true).unwrap();
    assert!(m.changed);
    assert_eq!(m.at, ms(2000));
    assert!(s.status().mic_muted);
    let again = s.set_mic_muted(true).unwrap();
    assert!(!again.changed);
    assert_eq!(again.at, ms(2000), "the time of the change in force");
    rig.run_until(3000);
    s.pause().unwrap();
    rig.run_until(3500);
    s.resume().unwrap();
    assert!(s.is_mic_muted(), "a pause does not unmute");
    rig.run_until(4000);
    let u = s.set_mic_muted(false).unwrap();
    assert!(u.changed);
    assert_eq!(u.at, ms(4000));
    rig.run_until(5000);
    let stopped = s.stop().unwrap();
    stopped.finalize().unwrap();

    let m = read_manifest(&dir);
    assert_eq!(
        m.extra["mic_muted"],
        serde_json::json!([{ "from": 2000 * 120, "to": 4000 * 120 }])
    );
    assert_eq!(m.extra["session"]["mic_muted"], m.extra["mic_muted"]);
    assert_eq!(
        m.tracks["mic"]["muted"],
        serde_json::json!([{ "from": 2000 * 120, "to": 4000 * 120 }])
    );
    assert_eq!(m.gaps.len(), 1);

    // The mic starts 100 ms in; the pause (3000..3500) is not in the file. Session 2000 is file
    // frame (2000 - 100) * 48; session 4000 is (4000 - 500 - 100) * 48.
    let samples = wav_samples(&dir.join("mic.wav"));
    assert_eq!(samples.len(), (5000 - 500 - 100) * 48);
    let (from, to) = (1900 * 48, 3400 * 48);
    assert!(samples[..from - 2].iter().all(|v| *v == 0.5));
    assert!(
        samples[from + 2..to - 2].iter().all(|v| *v == 0.0),
        "digital silence for the muted span"
    );
    assert!(samples[to + 2..].iter().all(|v| *v == 0.5));
}

/// AUD-6: mute needs a running recording with a mic track.
#[test]
fn aud6_mute_is_refused_without_a_mic_track_or_a_running_recording() {
    let (rig, tracks) = build(Options {
        which: SCREEN_CURSOR,
        ..Default::default()
    });
    let mut s = RecordingSession::begin(rig.config(), tracks).unwrap();
    let e = s.set_mic_muted(true).unwrap_err();
    assert_eq!(e.code(), ErrorCode::InvalidInput);
    assert!(e.to_string().contains("no microphone"), "{e}");
    assert!(!s.status().mic_muted);
    let _ = s.stop().unwrap();
    assert_eq!(
        s.set_mic_muted(true).unwrap_err().code(),
        ErrorCode::InvalidInput
    );
}

/// AUD-6 with REC-8: the spans are kept in the lock, so a crash while muted recovers them; the
/// span that was open has `to: null`.
#[test]
fn aud6_crash_while_muted_recovers_the_spans() {
    let (mut rig, tracks) = build(Options {
        which: MUTE_MIC,
        ..Default::default()
    });
    let mut s = RecordingSession::begin(rig.config(), tracks).unwrap();
    let dir = s.dir().to_owned();
    rig.run_until(1000);
    s.set_mic_muted(true).unwrap();
    rig.run_until(2000);
    s.set_mic_muted(false).unwrap();
    rig.run_until(3000);
    s.set_mic_muted(true).unwrap();
    rig.run_until(4000);

    std::mem::forget(s);
    std::thread::sleep(Duration::from_millis(1200));
    let _ = std::fs::remove_file(dir.join(cm_library::rec::OWNER_FILE));
    let lock_path = dir.join(LOCK_FILE);
    let mut lock: Value =
        serde_json::from_str(&std::fs::read_to_string(&lock_path).unwrap()).unwrap();
    assert_eq!(
        lock["extra"]["mic_muted"],
        serde_json::json!([
            { "from": 1000 * 120, "to": 2000 * 120 },
            { "from": 3000 * 120, "to": null },
        ]),
        "the lock keeps the spans as they happen"
    );
    lock["pid"] = Value::from(4_000_000u32);
    std::fs::write(&lock_path, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let mut lib = Library::open_or_create(rig.root.path()).unwrap();
    let summary = lib.recover_incomplete().unwrap();
    assert_eq!(summary.recovered.len(), 1, "{summary:?}");
    let m = read_manifest(&dir);
    assert_eq!(
        m.extra["mic_muted"],
        serde_json::json!([
            { "from": 1000 * 120, "to": 2000 * 120 },
            { "from": 3000 * 120, "to": null },
        ])
    );
    // The manifest still parses as a whole (additive field).
    assert!(m.tracks.contains_key("mic"));
}
