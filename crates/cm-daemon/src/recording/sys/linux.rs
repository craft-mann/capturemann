//! The Linux recording sources (L1, `specs/plans/linux-recording-plan.md`): the screen through the
//! desktop portal's ScreenCast session and a PipeWire stream, encoded as software VP9 by an FFmpeg
//! child process (decisions D-R2, D-R3); the microphone and system audio on PipeWire (D-R5).
//!
//! What is not there yet, and what the user is told:
//!
//! * **Window and region targets**: `not_implemented`; record the whole screen. The compositor's
//!   dialog picks the screen the first time and the portal's restore token (a file in the config
//!   folder, mode 0600) makes later starts silent.
//! * **Cursor**: no telemetry track. The pointer is part of the picture (cursor mode "embedded");
//!   a request for a separate cursor track records the pointer in the picture anyway and says so.
//! * **Camera**: `not_implemented`.
//!
//! [`LinuxEnv`] is everything that touches the machine, so the logic here is tested on any system
//! with a scripted environment (`tests/linux_recording.rs`).

use std::path::PathBuf;

use cm_audio::{
    AudioDevices, DeviceSelector, MicConfig, MicRecorder, SourceFactory as AudioFactory,
    SystemAudioConfig, SystemAudioRecorder,
};
use cm_capture::record::pipewire::NOT_A_DISPLAY_MESSAGE;
use cm_capture::record::pipewire::{FileTokenStore, PipeWireScreenSource, TokenStore};
use cm_capture::record::{CapturePlan, RecorderOptions};
use cm_capture::{CaptureTarget, DisplayInfo, PortalPipeWire, PortalStatus, Targets};
use cm_core::{CmError, PhysicalPoint, PhysicalRect, PhysicalSize, ScaleFactor};
use serde_json::{json, Value};

use crate::recording::params::ResolvedStart;
use crate::recording::sources::{
    BuildContext, DeviceNames, Preflight, PreflightWarning, SensorRequest, SourceFactory,
};
use crate::recording::tracks::{MicTrack, ScreenTrack, SessionTrack, SystemAudioTrack};

/// The id of the one display the portal's dialog stands for.
pub const DESKTOP_DISPLAY_ID: u32 = 1;
/// The size the plan assumes before the stream reports the real one (the recorder re-plans when it
/// connects, `ScreenSource::prepare`).
pub const HINT_SIZE: (u32, u32) = (1920, 1080);

/// Message of the error for a camera request.
pub const CAMERA_MESSAGE: &str = "Camera recording is not available on Linux yet.";
/// What the screen track really is on Linux, shown as a preflight warning.
pub const SOFTWARE_NOTE: &str = "On Linux the video is encoded in software as VP9 (expect more CPU use than a hardware encoder), at the size of the screen you pick in the desktop's share dialog; the size and codec shown above are the plan, the recording's details say what was written.";
/// The cursor sentence shown as a preflight warning and in `list_devices`.
pub const CURSOR_NOTE: &str = "On Linux the pointer is part of the video; there is no separate cursor track (positions and clicks are not recorded) yet.";

/// Everything the Linux sources need from the machine.
pub trait LinuxEnv: Send + Sync + 'static {
    /// The PipeWire daemon is reachable.
    fn pipewire_available(&self) -> bool;
    /// `wayland`, `x11` or `none`.
    fn display_session(&self) -> &'static str;
    /// What the desktop portal offers for screen casting.
    fn portal(&self) -> PortalStatus;
    /// The FFmpeg the VP9 encoder will run, if there is one.
    fn ffmpeg(&self) -> Option<PathBuf>;
    /// A restore token is stored (the portal's dialog will not appear).
    fn has_screen_token(&self) -> bool;
    /// Where the restore token is stored.
    fn token_file(&self) -> PathBuf;
    /// The screen track for `plan`.
    fn screen_track(&self, plan: CapturePlan) -> Box<dyn SessionTrack>;
    fn mic_factory(&self) -> AudioFactory;
    fn system_audio_factory(&self) -> AudioFactory;
    fn audio_devices(&self) -> AudioDevices;
}

/// The machine this process runs on.
#[derive(Debug, Default, Clone, Copy)]
pub struct RealLinuxEnv;

impl LinuxEnv for RealLinuxEnv {
    fn pipewire_available(&self) -> bool {
        cm_audio::pipewire_available()
    }

    fn display_session(&self) -> &'static str {
        let get = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        match get("XDG_SESSION_TYPE").as_deref() {
            Some("wayland") => "wayland",
            Some("x11") => "x11",
            _ if get("WAYLAND_DISPLAY").is_some() => "wayland",
            _ if get("DISPLAY").is_some() => "x11",
            _ => "none",
        }
    }

    fn portal(&self) -> PortalStatus {
        cm_capture::portal_status()
    }

    fn ffmpeg(&self) -> Option<PathBuf> {
        cm_encode::ffmpeg_vp9::find_ffmpeg()
    }

    fn has_screen_token(&self) -> bool {
        FileTokenStore(self.token_file()).load().is_some()
    }

    fn token_file(&self) -> PathBuf {
        crate::config::config_dir().join("screencast-token")
    }

    fn screen_track(&self, plan: CapturePlan) -> Box<dyn SessionTrack> {
        Box::new(ScreenTrack::new(
            plan,
            PipeWireScreenSource::new(PortalPipeWire::new(), FileTokenStore(self.token_file())),
            cm_encode::FfmpegVp9Encoder::create,
            RecorderOptions::default(),
        ))
    }

    fn mic_factory(&self) -> AudioFactory {
        cm_audio::default_mic_factory()
    }

    fn system_audio_factory(&self) -> AudioFactory {
        cm_audio::system_audio_factory(Vec::new())
    }

    fn audio_devices(&self) -> AudioDevices {
        cm_audio::list_audio_devices()
    }
}

/// The recording sources of a Linux desktop over an environment.
pub struct LinuxSources<E: LinuxEnv = RealLinuxEnv> {
    env: E,
}

impl LinuxSources<RealLinuxEnv> {
    pub fn real() -> Self {
        LinuxSources { env: RealLinuxEnv }
    }
}

impl<E: LinuxEnv> LinuxSources<E> {
    pub fn new(env: E) -> Self {
        LinuxSources { env }
    }

    fn pipewire_error(what: &str) -> CmError {
        CmError::platform_rejected(
            format!(
                "{what} needs PipeWire, and it is not running here (no pipewire-0 socket in \
                 XDG_RUNTIME_DIR). Start your desktop's PipeWire session, or install pipewire."
            ),
            Some("pipewire_missing".into()),
        )
    }

    fn mic_track(&self, p: &ResolvedStart) -> Option<Box<dyn SessionTrack>> {
        let m = p.mic.as_ref()?;
        let cfg = MicConfig {
            device: m
                .device_id
                .as_deref()
                .map_or(DeviceSelector::Default, DeviceSelector::parse),
            denoise: p.mic_denoise(),
            ..MicConfig::default()
        };
        Some(Box::new(MicTrack::new(MicRecorder::with_factory(
            cfg,
            self.env.mic_factory(),
        ))))
    }

    fn system_track(&self, p: &ResolvedStart) -> Option<Box<dyn SessionTrack>> {
        p.system_audio.then(|| {
            Box::new(SystemAudioTrack::new(SystemAudioRecorder::with_factory(
                SystemAudioConfig::default(),
                self.env.system_audio_factory(),
            ))) as Box<dyn SessionTrack>
        })
    }

    /// The plain-language state of screen recording for `list_devices` and the diagnostics.
    pub fn screen_report(&self) -> Value {
        let portal = self.env.portal();
        let session = self.env.display_session();
        let pipewire = self.env.pipewire_available();
        let ffmpeg = self.env.ffmpeg();
        let token = self.env.has_screen_token();
        let mut problems: Vec<String> = Vec::new();
        if session == "none" {
            problems
                .push("No desktop session (neither WAYLAND_DISPLAY nor DISPLAY is set).".into());
        }
        if !portal.available {
            problems.push(
                portal.detail.clone().unwrap_or_else(|| {
                    "The desktop portal's ScreenCast service is missing.".into()
                }),
            );
        } else if !portal.monitor {
            problems.push("The desktop portal cannot share a whole screen.".into());
        }
        if !pipewire {
            problems.push("PipeWire is not running.".into());
        }
        if ffmpeg.is_none() {
            problems.push(
                "FFmpeg was not found (it encodes the video: set CM_FFMPEG or install ffmpeg)."
                    .into(),
            );
        }
        json!({
            "backend": "xdg-desktop-portal ScreenCast + PipeWire",
            "session": session,
            "portal": portal,
            "pipewire": pipewire,
            "encoder": {
                "kind": "software",
                "codec": "vp9",
                "program": ffmpeg.map(|p| p.display().to_string()),
            },
            "targets": ["display"],
            "window_and_region": NOT_A_DISPLAY_MESSAGE,
            "consent": if token {
                "granted: a stored portal token makes the next recording start without a dialog"
            } else {
                "not asked yet: the desktop shows its share dialog on the first recording"
            },
            "ready": problems.is_empty(),
            "problems": problems,
        })
    }

    /// Lines for the Permissions part of the diagnostics report.
    pub fn permission_lines(&self) -> Vec<String> {
        let portal = self.env.portal();
        let pw = self.env.pipewire_available();
        let mut out = vec![
            format!("session {}", self.env.display_session()),
            if portal.available {
                format!(
                    "screen: desktop portal ScreenCast offers monitors {}, windows {}; {}",
                    yes_no(portal.monitor),
                    yes_no(portal.window),
                    if self.env.has_screen_token() {
                        "consent stored (no dialog)"
                    } else {
                        "consent not asked yet (a dialog appears on the first recording)"
                    }
                )
            } else {
                format!(
                    "screen: desktop portal ScreenCast missing ({})",
                    portal.detail.unwrap_or_default()
                )
            },
            format!(
                "PipeWire {}",
                if pw {
                    "running"
                } else {
                    "not running (no microphone or system audio, no screen stream)"
                }
            ),
            format!(
                "microphone: {}",
                if pw {
                    "default source through PipeWire (no permission prompt on Linux)"
                } else {
                    "unavailable without PipeWire"
                }
            ),
            format!(
                "system audio: {}",
                if pw {
                    "monitor of the default output through PipeWire"
                } else {
                    "unavailable without PipeWire"
                }
            ),
            "camera: not available on Linux yet".to_owned(),
            "cursor: part of the video (no separate track yet)".to_owned(),
        ];
        out.push(match self.env.ffmpeg() {
            Some(p) => format!(
                "video encoder: software VP9 through ffmpeg at {}",
                p.display()
            ),
            None => "video encoder: ffmpeg not found (recording cannot start)".to_owned(),
        });
        out
    }
}

fn yes_no(b: bool) -> &'static str {
    if b {
        "yes"
    } else {
        "no"
    }
}

/// The permission lines of the diagnostics on Linux, `None` elsewhere.
pub fn diagnostic_permission_lines() -> Option<Vec<String>> {
    cfg!(target_os = "linux").then(|| LinuxSources::real().permission_lines())
}

impl<E: LinuxEnv> SourceFactory for LinuxSources<E> {
    fn preflight(&self, req: &SensorRequest) -> Preflight {
        let mut out = Preflight::default();
        if req.needs_screen_permission() {
            // A screen recording needs the portal and PipeWire; audio-only recordings never get
            // here (system audio alone only needs PipeWire, checked below).
            if !req.audio_only {
                if self.env.display_session() == "none" {
                    out.errors.push(CmError::platform_rejected(
                        "There is no desktop session to record (neither WAYLAND_DISPLAY nor DISPLAY is set).",
                        Some("no_display_session".into()),
                    ));
                } else {
                    let portal = self.env.portal();
                    if !portal.available {
                        out.errors.push(CmError::platform_rejected(
                            format!(
                                "The desktop portal cannot share the screen. {}",
                                portal.detail.unwrap_or_default()
                            ),
                            Some("portal_missing".into()),
                        ));
                    } else if !portal.monitor {
                        out.errors.push(CmError::platform_rejected(
                            "The desktop portal cannot share a whole screen.",
                            Some("portal_no_monitor".into()),
                        ));
                    }
                }
                if !self.env.pipewire_available() {
                    out.errors.push(Self::pipewire_error("Screen recording"));
                }
                if self.env.ffmpeg().is_none() {
                    out.errors.push(CmError::platform_rejected(
                        "Screen recording on Linux needs FFmpeg (it encodes the video with VP9) and none was found. Install ffmpeg or set CM_FFMPEG.",
                        Some("ffmpeg_not_found".into()),
                    ));
                }
            }
        }
        if req.mic && !self.env.pipewire_available() {
            out.errors.push(Self::pipewire_error("The microphone"));
        }
        if req.system_audio && !self.env.pipewire_available() {
            out.errors.push(Self::pipewire_error("System audio"));
        }
        if req.camera {
            out.errors.push(CmError::not_implemented(CAMERA_MESSAGE));
        }
        if !req.audio_only {
            // The start answer still describes the plan (a hint size and the plan's codec); the
            // truth is only known once the desktop's stream is connected.
            out.warnings.push(PreflightWarning {
                track: Some("screen".to_owned()),
                code: "software_vp9_encoder".to_owned(),
                message: SOFTWARE_NOTE.to_owned(),
            });
        }
        if req.cursor {
            out.warnings.push(PreflightWarning {
                track: Some("cursor".to_owned()),
                code: "cursor_in_video".to_owned(),
                message: CURSOR_NOTE.to_owned(),
            });
        }
        out
    }

    fn check_target(&self, target: &CaptureTarget) -> Result<(), CmError> {
        match target {
            CaptureTarget::Display { .. } => Ok(()),
            _ => Err(CmError::not_implemented(NOT_A_DISPLAY_MESSAGE)),
        }
    }

    fn targets(&self) -> Option<Result<Targets, CmError>> {
        let scale = match ScaleFactor::new(1.0) {
            Ok(s) => s,
            Err(e) => return Some(Err(e)),
        };
        Some(Ok(Targets {
            displays: vec![DisplayInfo {
                id: DESKTOP_DISPLAY_ID,
                name: "Screen chosen in the desktop's share dialog".to_owned(),
                bounds: PhysicalRect {
                    origin: PhysicalPoint { x: 0, y: 0 },
                    size: PhysicalSize {
                        width: HINT_SIZE.0,
                        height: HINT_SIZE.1,
                    },
                },
                scale,
                is_primary: true,
                is_builtin: false,
            }],
            windows: Vec::new(),
        }))
    }

    fn device_names(&self, req: &SensorRequest) -> DeviceNames {
        let mic = req.mic.then(|| match req.mic_device.as_deref() {
            Some(id) if !id.eq_ignore_ascii_case("default") => id.to_owned(),
            _ => "the default microphone".to_owned(),
        });
        DeviceNames { mic, camera: None }
    }

    fn build(&self, cx: &BuildContext<'_>) -> Result<Vec<Box<dyn SessionTrack>>, CmError> {
        let p = cx.params;
        if p.camera.is_some() {
            return Err(CmError::not_implemented(CAMERA_MESSAGE));
        }
        let mut plan = cx.plan.clone();
        // No cursor telemetry yet: the pointer stays in the picture even when a separate track
        // was asked for (the preflight warned).
        plan.show_cursor = true;
        let mut tracks = vec![self.env.screen_track(plan)];
        tracks.extend(self.mic_track(p));
        tracks.extend(self.system_track(p));
        Ok(tracks)
    }

    fn build_audio(&self, p: &ResolvedStart) -> Result<Vec<Box<dyn SessionTrack>>, CmError> {
        let mut tracks: Vec<Box<dyn SessionTrack>> = Vec::new();
        tracks.extend(self.mic_track(p));
        tracks.extend(self.system_track(p));
        Ok(tracks)
    }

    fn devices(&self) -> Value {
        let audio = self.env.audio_devices();
        let screen = self.screen_report();
        let pw = self.env.pipewire_available();
        let consent = if self.env.has_screen_token() {
            "authorized"
        } else {
            "not_determined"
        };
        json!({
            "platform": "linux",
            "cameras": [],
            "cameras_error": CAMERA_MESSAGE,
            "mics": audio.mics,
            "mic_backend": if pw { "pipewire" } else { "none" },
            "microphone_permission": "authorized",
            "microphone_permission_message": if pw {
                Value::Null
            } else {
                json!("The microphone needs PipeWire, and it is not running.")
            },
            "system_audio": audio.system_audio,
            "screen": screen,
            // The portal has no query: "authorized" once its restore token is stored.
            "screen_recording_permission": consent,
            "cursor": {
                "clicks": false,
                "mode": "embedded",
                "note": CURSOR_NOTE,
                "missing_permission": Value::Null,
            },
        })
    }
}
