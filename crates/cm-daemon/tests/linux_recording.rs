//! The Linux recording path through the daemon's real actions, over a scripted desktop: the portal
//! and PipeWire are replaced by `ScriptedPw` (a desktop that streams frames), the microphone and
//! system audio by fake sources, the encoder by the synthetic one. Everything else (the sources
//! factory, the plan, the recorder, the session, the engine, the manifest) is the product's, so
//! this runs on any system without a display or a sound server.
//!
//! Real-hardware numbers are in `verification/L1-recording-product-linux-*.md`.

#![allow(clippy::unwrap_used)]

mod common;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cm_audio::{
    AudioDevices, AudioSource, DeviceSelector, FakeAudioSource, FakeConfig, FakeSignal, MicDevice,
    SourceFactory as AudioFactory, SystemAudioSupport,
};
use cm_capture::record::pipewire::{
    MemoryTokenStore, PipeWireScreenSource, ScriptedPw, StreamInfo,
};
use cm_capture::record::{CapturePlan, RecorderOptions};
use cm_capture::PortalStatus;
use cm_core::{Actor, ErrorCode, HostClock, SystemHostClock};
use cm_daemon::recording::sys::linux::{LinuxEnv, LinuxSources};
use cm_daemon::recording::{ScreenTrack, SessionTrack};
use cm_daemon::{Daemon, DaemonError, EngineOptions, Event};
use cm_encode::synthetic::CpuSyntheticEncoder;
use common::{builder, desktop};
use serde_json::{json, Value};
use tokio::sync::broadcast::Receiver;

type SeenTokens = Arc<Mutex<Vec<Arc<Mutex<Option<Option<String>>>>>>>;

/// The scripted machine. Cloning shares the state, so a test keeps a handle after the move.
#[derive(Clone)]
struct FakeEnv {
    pipewire: bool,
    portal: PortalStatus,
    ffmpeg: Option<PathBuf>,
    tokens: MemoryTokenStore,
    /// The desktop ends the share this long after a recording started (a closed portal session).
    lose_after: Option<Duration>,
    /// One entry per recording: the token `open` was called with.
    seen: SeenTokens,
    /// The size the stream reports (the monitor the dialog picked).
    stream: (u32, u32),
    plans: Arc<Mutex<Vec<CapturePlan>>>,
}

impl FakeEnv {
    fn healthy() -> Self {
        FakeEnv {
            pipewire: true,
            portal: PortalStatus {
                available: true,
                monitor: true,
                window: true,
                detail: None,
            },
            ffmpeg: Some(PathBuf::from("/usr/bin/ffmpeg")),
            tokens: MemoryTokenStore::default(),
            lose_after: None,
            seen: Arc::default(),
            stream: (1280, 720),
            plans: Arc::default(),
        }
    }

    fn seen_tokens(&self) -> Vec<Option<String>> {
        self.seen
            .lock()
            .unwrap()
            .iter()
            .map(|s| s.lock().unwrap().clone().expect("open was called"))
            .collect()
    }
}

/// A source that plays audio in real time until the recorder stops it.
fn audio_factory(channels: u16, name: &'static str) -> AudioFactory {
    Box::new(move |_sel: &DeviceSelector| {
        let (src, handle) = FakeAudioSource::new(FakeConfig {
            channels,
            device_name: name.into(),
            device_id: name.into(),
            host_start_ns: SystemHostClock.now_ns(),
            signal: FakeSignal::Sine {
                freq_hz: 440.0,
                amp: 0.3,
            },
            ..Default::default()
        });
        let h = handle.clone();
        std::thread::spawn(move || {
            while !h.is_started() {
                std::thread::sleep(Duration::from_millis(2));
            }
            let mut last = std::time::Instant::now();
            while h.is_started() {
                std::thread::sleep(Duration::from_millis(10));
                let now = std::time::Instant::now();
                h.advance(now.duration_since(last).as_secs_f64());
                last = now;
            }
        });
        Ok(Box::new(src) as Box<dyn AudioSource>)
    })
}

impl LinuxEnv for FakeEnv {
    fn pipewire_available(&self) -> bool {
        self.pipewire
    }
    fn display_session(&self) -> &'static str {
        "wayland"
    }
    fn portal(&self) -> PortalStatus {
        self.portal.clone()
    }
    fn ffmpeg(&self) -> Option<PathBuf> {
        self.ffmpeg.clone()
    }
    fn has_screen_token(&self) -> bool {
        use cm_capture::record::pipewire::TokenStore;
        self.tokens.load().is_some()
    }
    fn token_file(&self) -> PathBuf {
        PathBuf::from("/nonexistent/screencast-token")
    }
    fn screen_track(&self, plan: CapturePlan) -> Box<dyn SessionTrack> {
        self.plans.lock().unwrap().push(plan.clone());
        let backend = ScriptedPw::live(
            StreamInfo {
                width: self.stream.0,
                height: self.stream.1,
                restore_token: Some("tok1".into()),
            },
            30,
            self.lose_after,
        );
        self.seen.lock().unwrap().push(backend.seen_token.clone());
        Box::new(ScreenTrack::new(
            plan,
            PipeWireScreenSource::new(backend, self.tokens.clone()),
            CpuSyntheticEncoder::create,
            RecorderOptions::default(),
        ))
    }
    fn mic_factory(&self) -> AudioFactory {
        audio_factory(1, "pipewire:default-source")
    }
    fn system_audio_factory(&self) -> AudioFactory {
        audio_factory(2, "pipewire:default-monitor")
    }
    fn audio_devices(&self) -> AudioDevices {
        AudioDevices {
            mics: if self.pipewire {
                vec![MicDevice {
                    id: "default".into(),
                    name: "Default microphone (PipeWire)".into(),
                    default: true,
                    channels: 1,
                    sample_rate: 48_000,
                    max_channels: 2,
                    sample_rate_ranges: vec![],
                }]
            } else {
                vec![]
            },
            system_audio: SystemAudioSupport {
                supported: self.pipewire,
                backend: if self.pipewire {
                    "pipewire_monitor"
                } else {
                    "none"
                }
                .into(),
                reason: None,
                permission: cm_audio::PermissionState::Authorized,
                excludes_own_audio: false,
            },
            microphone_permission: cm_audio::PermissionState::Authorized,
            microphone_permission_message: None,
        }
    }
}

struct Rig {
    daemon: Daemon,
    dir: tempfile::TempDir,
    env: FakeEnv,
}

fn rig(env: FakeEnv) -> Rig {
    let (b, dir) = builder(desktop());
    let opts = EngineOptions {
        tick_every: Duration::from_millis(20),
        stop_timeout: Duration::from_secs(10),
        shutdown_timeout: Duration::from_secs(10),
        disk_check_every: Duration::ZERO,
        own_pid: None,
        ..EngineOptions::default()
    };
    let daemon = b
        .recording_sources(LinuxSources::new(env.clone()))
        .recording_options(opts)
        .build();
    Rig { daemon, dir, env }
}

async fn user(d: &Daemon, method: &str, params: Value) -> Result<Value, DaemonError> {
    d.execute_method(Actor::User, 0, method, &params).await
}

async fn wait_event(rx: &mut Receiver<Event>, name: &str) -> Event {
    let wait = async {
        loop {
            let e = rx.recv().await.unwrap();
            if e.name == name {
                return e;
            }
        }
    };
    tokio::time::timeout(Duration::from_secs(15), wait)
        .await
        .unwrap_or_else(|_| panic!("no `{name}` event"))
}

fn bundle(r: &Rig, title: &str) -> PathBuf {
    r.dir.path().join(format!("{title}.cmrec"))
}

fn manifest(r: &Rig, title: &str) -> Value {
    let text = std::fs::read_to_string(bundle(r, title).join("manifest.json")).unwrap();
    serde_json::from_str(&text).unwrap()
}

fn code(e: &DaemonError) -> ErrorCode {
    e.code().unwrap_or_else(|| panic!("no shared code: {e:?}"))
}

fn start_params(extra: Value) -> Value {
    let mut p = json!({ "target": { "display_id": 1 }, "countdown_s": 0 });
    for (k, v) in extra.as_object().unwrap() {
        p[k] = v.clone();
    }
    p
}

/// Start, pause, resume and stop with screen, microphone and system audio: the files exist, the
/// screen track says what it really holds (VP9, the stream's size, not the plan's hint), and the
/// portal token is stored for the next run.
#[tokio::test(flavor = "multi_thread")]
async fn start_pause_resume_stop_records_screen_and_both_audio_tracks() {
    let r = rig(FakeEnv::healthy());
    user(
        &r.daemon,
        "set_settings",
        json!({ "values": { "recording.resume_countdown_s": 0 } }),
    )
    .await
    .unwrap();
    let mut rx = r.daemon.subscribe();
    let started = user(
        &r.daemon,
        "start_recording",
        start_params(json!({ "mic": {}, "system_audio": true, "title": "Linux one" })),
    )
    .await
    .unwrap();
    assert_eq!(started["state"], "recording", "{started}");
    let tracks: Vec<&str> = started["tracks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t.as_str().unwrap())
        .collect();
    assert_eq!(
        tracks,
        ["screen", "mic", "system"],
        "no cursor track on Linux"
    );
    tokio::time::sleep(Duration::from_millis(700)).await;
    let p = user(&r.daemon, "pause_recording", json!({})).await.unwrap();
    assert_eq!(p["state"], "paused", "{p}");
    tokio::time::sleep(Duration::from_millis(300)).await;
    let res = user(&r.daemon, "resume_recording", json!({}))
        .await
        .unwrap();
    assert_eq!(res["state"], "recording", "{res}");
    tokio::time::sleep(Duration::from_millis(700)).await;
    let stopped = user(&r.daemon, "stop_recording", json!({})).await.unwrap();
    assert_eq!(stopped["reason"], "user", "{stopped}");
    assert!(
        stopped["duration_ms"].as_u64().unwrap() >= 1200,
        "pause is not counted: {stopped}"
    );
    let _ = wait_event(&mut rx, "recording.stopped").await;

    let dir = bundle(&r, "Linux one");
    for f in ["screen.mp4", "mic.wav", "system.wav"] {
        assert!(dir.join(f).metadata().unwrap().len() > 100, "{f}");
    }
    let scan = cm_encode::scan_file(&dir.join("screen.mp4")).unwrap();
    assert!(!scan.fragments.is_empty());
    let m = manifest(&r, "Linux one");
    let screen = &m["tracks"]["screen"];
    let d = screen.get("details").unwrap_or(screen);
    assert_eq!(d["codec"], "vp9", "the manifest names the real codec: {d}");
    assert_eq!(d["size"], json!([1280, 720]), "the stream's size wins: {d}");
    assert_eq!(d["cursor"], true, "the pointer is in the picture");
    assert!(d["stopped_early"].is_null(), "{d}");
    // The plan the track was built with had the hint size and the pointer forced into the picture.
    let plans = r.env.plans.lock().unwrap();
    assert_eq!(plans.len(), 1);
    assert!(plans[0].show_cursor);
    assert_eq!(r.env.tokens_value().as_deref(), Some("tok1"));
}

impl FakeEnv {
    fn tokens_value(&self) -> Option<String> {
        use cm_capture::record::pipewire::TokenStore;
        self.tokens.load()
    }
}

/// The restore token from the first recording is handed to the portal on the second (no dialog).
#[tokio::test(flavor = "multi_thread")]
async fn the_second_recording_reuses_the_portal_token() {
    let r = rig(FakeEnv::healthy());
    for title in ["First", "Second"] {
        user(
            &r.daemon,
            "start_recording",
            start_params(json!({ "title": title })),
        )
        .await
        .unwrap();
        tokio::time::sleep(Duration::from_millis(400)).await;
        user(&r.daemon, "stop_recording", json!({})).await.unwrap();
    }
    assert_eq!(
        r.env.seen_tokens(),
        vec![None, Some("tok1".to_string())],
        "the first start has no token, the second reuses the one the portal issued"
    );
}

/// The Record sheet lists the displays the screenshot code sees: an X11 session has ids such as
/// 60, and the recorder (one display, picked in the desktop's dialog) must not answer "Display 60
/// not found" (found in the VM on X11, Oct 31, 2026).
#[tokio::test(flavor = "multi_thread")]
async fn any_listed_display_id_starts_the_one_screen_the_desktop_shares() {
    let r = rig(FakeEnv::healthy());
    let started = user(
        &r.daemon,
        "start_recording",
        json!({ "target": { "display_id": 60 }, "countdown_s": 0, "title": "X11 id" }),
    )
    .await
    .unwrap();
    assert_eq!(started["state"], "recording", "{started}");
    tokio::time::sleep(Duration::from_millis(400)).await;
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
    let plan_target = format!("{:?}", r.env.plans.lock().unwrap()[0].target);
    assert!(plan_target.contains("display_id: 1"), "{plan_target}");
}

/// The desktop ends the share mid-recording (the portal session closed, the compositor went away):
/// the recording stops by itself with what was written, says why, and nothing hangs.
#[tokio::test(flavor = "multi_thread")]
async fn a_lost_stream_ends_the_recording_cleanly_with_a_reason() {
    let mut env = FakeEnv::healthy();
    env.lose_after = Some(Duration::from_millis(900));
    let r = rig(env);
    let mut rx = r.daemon.subscribe();
    user(
        &r.daemon,
        "start_recording",
        start_params(json!({ "mic": {}, "title": "Lost" })),
    )
    .await
    .unwrap();
    let ev = wait_event(&mut rx, "recording.stopped").await;
    assert_eq!(ev.data["reason"], "screen_halted", "{:?}", ev.data);
    assert!(ev.data["capture_id"].is_string());
    let dur = ev.data["duration_s"].as_f64().unwrap();
    assert!((0.5..5.0).contains(&dur), "kept what was recorded: {dur}");
    let m = manifest(&r, "Lost");
    assert_eq!(m["stop_reason"], "screen_halted");
    assert!(
        m["warnings"].to_string().contains("desktop ended"),
        "the user is told why: {}",
        m["warnings"]
    );
    let scan = cm_encode::scan_file(&bundle(&r, "Lost").join("screen.mp4")).unwrap();
    assert!(!scan.fragments.is_empty(), "the file holds the data so far");
    assert!(!r.daemon.recording_active());
    // And a new recording can start right away.
    user(
        &r.daemon,
        "start_recording",
        start_params(json!({ "title": "After" })),
    )
    .await
    .unwrap();
    user(&r.daemon, "stop_recording", json!({})).await.unwrap();
}

/// Missing pieces are named, not hidden: PipeWire, the portal, FFmpeg, the camera, windows.
#[tokio::test(flavor = "multi_thread")]
async fn missing_parts_and_unsupported_requests_answer_clearly() {
    let mut env = FakeEnv::healthy();
    env.pipewire = false;
    env.portal = PortalStatus {
        available: false,
        monitor: false,
        window: false,
        detail: Some("no ScreenCast service".into()),
    };
    env.ffmpeg = None;
    let r = rig(env);
    let e = user(&r.daemon, "start_recording", start_params(json!({})))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::PlatformRejected, "{e:?}");
    assert!(!r.daemon.recording_active());

    let ok = rig(FakeEnv::healthy());
    let e = user(
        &ok.daemon,
        "start_recording",
        start_params(json!({ "camera": {} })),
    )
    .await
    .unwrap_err();
    assert_eq!(code(&e), ErrorCode::NotImplemented, "{e:?}");
    assert!(e.to_string().contains("Camera"), "{e}");

    for target in [
        json!({ "window_id": 10 }),
        json!({ "display_id": 1, "rect": [0, 0, 100, 100] }),
    ] {
        let e = user(
            &ok.daemon,
            "start_recording",
            json!({ "target": target, "countdown_s": 0 }),
        )
        .await
        .unwrap_err();
        assert_eq!(code(&e), ErrorCode::NotImplemented, "{target}: {e:?}");
        assert!(e.to_string().contains("whole screen"), "{e}");
    }
    assert!(!ok.daemon.recording_active());
}

/// `list_devices` says what is there: the portal, PipeWire, the encoder, no camera, the pointer.
#[tokio::test(flavor = "multi_thread")]
async fn list_devices_tells_the_truth_on_linux() {
    let r = rig(FakeEnv::healthy());
    let v = user(&r.daemon, "list_devices", json!({})).await.unwrap();
    assert_eq!(v["platform"], "linux", "{v}");
    assert_eq!(v["screen"]["ready"], true, "{v}");
    assert_eq!(v["screen"]["encoder"]["codec"], "vp9");
    assert_eq!(v["screen"]["targets"], json!(["display"]));
    assert_eq!(v["screen_recording_permission"], "not_determined");
    assert_eq!(v["mic_backend"], "pipewire");
    assert_eq!(v["mics"][0]["id"], "default");
    assert_eq!(v["system_audio"]["backend"], "pipewire_monitor");
    assert_eq!(v["cameras"], json!([]));
    assert!(v["cameras_error"]
        .as_str()
        .unwrap()
        .contains("not available"));
    assert_eq!(v["cursor"]["mode"], "embedded");

    // The diagnostics report says the same in its Permissions section.
    let lines = LinuxSources::new(FakeEnv::healthy())
        .permission_lines()
        .join(
            "
",
        );
    assert!(
        lines.contains("desktop portal ScreenCast offers monitors yes"),
        "{lines}"
    );
    assert!(lines.contains("consent not asked yet"), "{lines}");
    assert!(lines.contains("PipeWire running"), "{lines}");
    assert!(
        lines.contains("camera: not available on Linux yet"),
        "{lines}"
    );
    assert!(lines.contains("software VP9"), "{lines}");

    let mut env = FakeEnv::healthy();
    env.pipewire = false;
    env.ffmpeg = None;
    let r = rig(env);
    let v = user(&r.daemon, "list_devices", json!({})).await.unwrap();
    assert_eq!(v["screen"]["ready"], false);
    let problems = v["screen"]["problems"].to_string();
    assert!(problems.contains("PipeWire is not running"), "{problems}");
    assert!(problems.contains("FFmpeg was not found"), "{problems}");
    assert_eq!(v["mic_backend"], "none");
    let lines = LinuxSources::new(FakeEnv {
        pipewire: false,
        ffmpeg: None,
        ..FakeEnv::healthy()
    })
    .permission_lines()
    .join(
        "
",
    );
    assert!(lines.contains("PipeWire not running"), "{lines}");
    assert!(lines.contains("ffmpeg not found"), "{lines}");
}
