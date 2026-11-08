//! The Linux recording sources (L1, `specs/plans/linux-recording-plan.md`): the screen through the
//! desktop portal's ScreenCast session and a PipeWire stream, encoded as software VP9 by an FFmpeg
//! child process (decisions D-R2, D-R3); the microphone and system audio on PipeWire (D-R5).
//!
//! What the user gets, by desktop:
//!
//! * **GNOME** (Wayland or X11 session): GNOME's own screen-cast service gives a monitor or a
//!   rectangle, cropped by the compositor, with no dialog, so display, region and (X11) window
//!   recordings all work and an agent can start them. A window on a Wayland session is not
//!   available (programs cannot learn where windows are): the error says so.
//! * **Other desktops** (KDE, wlroots): the portal. The compositor's dialog picks the screen the
//!   first time and the portal's restore token (a file in the config folder, mode 0600) makes
//!   later starts silent; a window or region is `not_implemented` with the reason.
//! * **Cursor**: no telemetry track. The pointer is part of the picture (cursor mode "embedded");
//!   a request for a separate cursor track records the pointer in the picture anyway and says so.
//! * **Camera**: Video4Linux2 through `cm-camera`, recorded as software VP9 (the camera half is
//!   in `linux_camera.rs`; this file only calls it).
//!
//! [`LinuxEnv`] is everything that touches the machine, so the logic here is tested on any system
//! with a scripted environment (`tests/linux_recording.rs`).

use std::path::PathBuf;

use cm_audio::{
    AudioDevices, DeviceSelector, MicConfig, MicRecorder, SourceFactory as AudioFactory,
    SystemAudioConfig, SystemAudioRecorder,
};
use cm_camera::{CameraConfig, CameraInfo, CameraPermission, CameraRecorder};
use cm_capture::record::pipewire::{FileTokenStore, PipeWireScreenSource, PwTarget, TokenStore};
use cm_capture::record::{CapturePlan, RecorderOptions};
use cm_capture::{CaptureTarget, DisplayInfo, PortalPipeWire, PortalStatus, Targets};
use cm_core::{CmError, PhysicalPoint, PhysicalRect, PhysicalSize, ScaleFactor};
use cm_encode::ffmpeg_vp9::VideoBackend;
use serde_json::{json, Value};

use crate::recording::params::ResolvedStart;
use crate::recording::sources::{
    BuildContext, DeviceNames, Preflight, PreflightWarning, SensorRequest, SourceFactory,
};
use crate::recording::tracks::{
    CameraTrack, MicTrack, ScreenTrack, SessionTrack, SystemAudioTrack,
};

/// The id of the one display the portal's dialog stands for.
pub const DESKTOP_DISPLAY_ID: u32 = 1;
/// The size the plan assumes before the stream reports the real one (the recorder re-plans when it
/// connects, `ScreenSource::prepare`).
pub const HINT_SIZE: (u32, u32) = (1920, 1080);

/// Shown as a preflight warning when no hardware H.264 encoder works (release 0.3.0, O10).
pub const SOFTWARE_NOTE: &str = "No hardware H.264 encoder works on this computer (VA-API), so the video is encoded in software as VP9 (expect more CPU use); the codec shown above is the plan, the recording's details say what was written.";
/// Shown as a preflight warning on a desktop where the portal's dialog picks the screen.
pub const PORTAL_SIZE_NOTE: &str = "The size is the screen you pick in the desktop's share dialog; the size shown above is the plan, the recording's details say what was written.";
/// A window on a Wayland session.
pub const WAYLAND_WINDOW_MESSAGE: &str = "Recording one window is not available in a Wayland session: the desktop does not tell programs where windows are. Record the whole screen or a region, or log in to an X11 session (GNOME on Xorg), where a window can be recorded.";
/// A window or region where only the portal is available.
pub const PORTAL_ONLY_MESSAGE: &str = "Recording a region or a window needs GNOME's screen-cast service, and this desktop only offers the portal's dialog. Record the whole screen; the desktop asks which one the first time.";
/// The cursor sentence shown as a preflight warning and in `list_devices`.
pub const CURSOR_NOTE: &str = "In a Wayland session the pointer is part of the video and there is no separate cursor track (positions and clicks are not recorded): the desktop does not let programs read the pointer. An X11 session records the track.";

/// Everything the Linux sources need from the machine.
pub trait LinuxEnv: Send + Sync + 'static {
    /// The PipeWire daemon is reachable.
    fn pipewire_available(&self) -> bool;
    /// `wayland`, `x11` or `none`.
    fn display_session(&self) -> &'static str;
    /// What the desktop portal offers for screen casting.
    fn portal(&self) -> PortalStatus;
    /// The FFmpeg the video encoder will run, if there is one.
    fn ffmpeg(&self) -> Option<PathBuf>;
    /// GNOME's own screen-cast service is on the session bus (a monitor or area, no dialog).
    fn gnome_screencast(&self) -> bool {
        false
    }
    /// What encodes the video here: VA-API H.264 where it works, else software VP9.
    fn video_backend(&self) -> VideoBackend {
        VideoBackend::Vp9
    }
    /// A restore token is stored (the portal's dialog will not appear).
    fn has_screen_token(&self) -> bool;
    /// Where the restore token is stored.
    fn token_file(&self) -> PathBuf;
    /// The screen track for `plan`.
    fn screen_track(&self, plan: CapturePlan) -> Box<dyn SessionTrack>;
    /// The screen track for `plan` recording `target`. The default ignores the target.
    fn screen_track_for(&self, plan: CapturePlan, target: PwTarget) -> Box<dyn SessionTrack> {
        let _ = target;
        self.screen_track(plan)
    }
    /// The cursor telemetry track (REC-7) for `target`, if this session can record one: an X11
    /// session can (XInput2), a Wayland session cannot.
    fn cursor_track(&self, target: cm_cursor::CursorTarget) -> Option<Box<dyn SessionTrack>> {
        let _ = target;
        None
    }
    fn mic_factory(&self) -> AudioFactory;
    fn system_audio_factory(&self) -> AudioFactory;
    fn audio_devices(&self) -> AudioDevices;
    /// The cameras (`cm_camera::list_cameras`). The default is a machine without cameras, so
    /// scripted environments never touch a device.
    fn cameras(&self) -> Result<Vec<CameraInfo>, CmError> {
        Ok(Vec::new())
    }
    /// Whether the user may open the camera devices.
    fn camera_permission(&self) -> CameraPermission {
        CameraPermission::Authorized
    }
    /// The camera track for `cfg`.
    fn camera_track(&self, cfg: CameraConfig) -> Result<Box<dyn SessionTrack>, CmError> {
        let _ = cfg;
        Err(CmError::not_found("No camera was found."))
    }
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

    fn gnome_screencast(&self) -> bool {
        cm_capture::mutter_screencast_available()
    }

    fn video_backend(&self) -> VideoBackend {
        self.ffmpeg().map_or(VideoBackend::Vp9, |f| {
            cm_encode::ffmpeg_vp9::choose_backend(&f)
        })
    }

    fn has_screen_token(&self) -> bool {
        FileTokenStore(self.token_file()).load().is_some()
    }

    fn token_file(&self) -> PathBuf {
        crate::config::config_dir().join("screencast-token")
    }

    fn screen_track(&self, plan: CapturePlan) -> Box<dyn SessionTrack> {
        self.screen_track_for(plan, PwTarget::Screen)
    }

    fn screen_track_for(&self, plan: CapturePlan, target: PwTarget) -> Box<dyn SessionTrack> {
        Box::new(ScreenTrack::new(
            plan,
            PipeWireScreenSource::new(PortalPipeWire::new(), FileTokenStore(self.token_file()))
                .with_target(target),
            cm_encode::FfmpegVp9Encoder::create,
            RecorderOptions::default(),
        ))
    }

    fn cursor_track(&self, target: cm_cursor::CursorTarget) -> Option<Box<dyn SessionTrack>> {
        (self.display_session() == "x11").then(|| {
            Box::new(crate::recording::tracks::CursorTrack::new(
                cm_cursor::CursorRecorder::new(cm_cursor::CursorConfig::new(target)),
            )) as Box<dyn SessionTrack>
        })
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

    fn cameras(&self) -> Result<Vec<CameraInfo>, CmError> {
        cm_camera::list_cameras()
    }

    fn camera_permission(&self) -> CameraPermission {
        cm_camera::check_camera_permission()
    }

    fn camera_track(&self, cfg: CameraConfig) -> Result<Box<dyn SessionTrack>, CmError> {
        Ok(Box::new(CameraTrack::new(CameraRecorder::system(cfg))))
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
        let gnome = self.env.gnome_screencast();
        let backend = self.env.video_backend();
        let mut problems: Vec<String> = Vec::new();
        if session == "none" {
            problems
                .push("No desktop session (neither WAYLAND_DISPLAY nor DISPLAY is set).".into());
        }
        if !gnome {
            if !portal.available {
                problems.push(portal.detail.clone().unwrap_or_else(|| {
                    "The desktop portal's ScreenCast service is missing.".into()
                }));
            } else if !portal.monitor {
                problems.push("The desktop portal cannot share a whole screen.".into());
            }
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
        let (targets, other): (Vec<&str>, Value) = if gnome {
            let mut t = vec!["display", "region"];
            if session == "x11" {
                t.push("window");
            }
            (
                t,
                json!(if session == "x11" {
                    ""
                } else {
                    WAYLAND_WINDOW_MESSAGE
                }),
            )
        } else {
            (vec!["display"], json!(PORTAL_ONLY_MESSAGE))
        };
        json!({
            "backend": if gnome {
                "GNOME screen-cast service + PipeWire"
            } else {
                "xdg-desktop-portal ScreenCast + PipeWire"
            },
            "session": session,
            "portal": portal,
            "pipewire": pipewire,
            "encoder": {
                "kind": if backend == VideoBackend::Vp9 { "software" } else { "hardware" },
                "codec": backend.codec_name(),
                "description": backend.describe(),
                "program": ffmpeg.map(|p| p.display().to_string()),
            },
            "targets": targets,
            "window_and_region": other,
            "consent": if gnome {
                "not needed: GNOME lets the logged-in user's programs record the screen; the desktop shows its recording indicator"
            } else if token {
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
            if self.env.gnome_screencast() {
                "screen: GNOME's screen-cast service (monitor, region{}; no dialog)".replace(
                    "{}",
                    if self.env.display_session() == "x11" {
                        " and window"
                    } else {
                        ""
                    },
                )
            } else if portal.available {
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
            super::linux_camera::report_line(&self.env.cameras(), self.env.camera_permission()),
            "cursor: part of the video (no separate track yet)".to_owned(),
        ];
        out.push(match self.env.ffmpeg() {
            Some(p) => format!(
                "video encoder: {} through ffmpeg at {}",
                self.env.video_backend().describe(),
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

/// A physical rectangle as GNOME's logical coordinates (the compositor crops in logical px; the
/// stream is then `logical * scale` pixels, the size the plan expects).
fn logical_area(origin: (i32, i32), size: PhysicalSize, scale: f64) -> PwTarget {
    let s = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    let l = |v: i32| (f64::from(v) / s).round() as i32;
    let n = |v: u32| ((f64::from(v) / s).round() as u32).max(1);
    PwTarget::Area {
        x: l(origin.0),
        y: l(origin.1),
        width: n(size.width),
        height: n(size.height),
    }
}

/// What GNOME's screen-cast service records for the plan's target (REC-1: display, region,
/// window). Pure, so it is tested without a desktop.
pub fn pw_target(
    plan: &CapturePlan,
    targets: &Targets,
    session: &str,
) -> Result<PwTarget, CmError> {
    let display = |id: Option<u32>| {
        id.and_then(|id| targets.displays.iter().find(|d| d.id == id))
            .ok_or_else(|| {
                CmError::not_found("The display to record is not in the desktop's layout")
            })
    };
    match plan.target {
        CaptureTarget::Display { display_id } => {
            let d = display(Some(display_id))?;
            Ok(logical_area(
                (d.bounds.origin.x, d.bounds.origin.y),
                d.bounds.size,
                d.scale.get(),
            ))
        }
        CaptureTarget::Region { display_id, .. } => {
            let d = display(Some(display_id))?;
            let crop = plan
                .crop
                .ok_or_else(|| CmError::internal("a region recording has no crop"))?;
            Ok(logical_area(
                (
                    d.bounds.origin.x + crop.origin.x,
                    d.bounds.origin.y + crop.origin.y,
                ),
                crop.size,
                d.scale.get(),
            ))
        }
        CaptureTarget::Window { window_id } => {
            if session != "x11" {
                return Err(CmError::not_implemented(WAYLAND_WINDOW_MESSAGE));
            }
            let w = targets
                .windows
                .iter()
                .find(|w| w.id == window_id)
                .ok_or_else(|| CmError::not_found(format!("Window {window_id} not found")))?;
            Ok(logical_area(
                (w.bounds.origin.x, w.bounds.origin.y),
                w.bounds.size,
                w.scale.get(),
            ))
        }
    }
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
                } else if !self.env.gnome_screencast() {
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
                        "Screen recording on Linux needs FFmpeg (it encodes the video) and none was found. Install ffmpeg or set CM_FFMPEG.",
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
        out.errors.extend(super::linux_camera::preflight(
            req,
            &self.env.cameras(),
            self.env.camera_permission(),
        ));
        if !req.audio_only {
            // The start answer still describes the plan (a hint size and the plan's codec); the
            // truth is only known once the desktop's stream is connected.
            if self.env.video_backend() == VideoBackend::Vp9 {
                out.warnings.push(PreflightWarning {
                    track: Some("screen".to_owned()),
                    code: "software_vp9_encoder".to_owned(),
                    message: SOFTWARE_NOTE.to_owned(),
                });
            }
            if !self.env.gnome_screencast() {
                out.warnings.push(PreflightWarning {
                    track: Some("screen".to_owned()),
                    code: "size_from_share_dialog".to_owned(),
                    message: PORTAL_SIZE_NOTE.to_owned(),
                });
            }
        }
        if req.cursor && self.env.display_session() != "x11" {
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
            _ if !self.env.gnome_screencast() => Err(CmError::not_implemented(PORTAL_ONLY_MESSAGE)),
            CaptureTarget::Window { .. } if self.env.display_session() != "x11" => {
                Err(CmError::not_implemented(WAYLAND_WINDOW_MESSAGE))
            }
            _ => Ok(()),
        }
    }

    fn adapt_target(&self, target: CaptureTarget) -> CaptureTarget {
        match target {
            // Where the portal's dialog picks the screen, any display id means "that one".
            CaptureTarget::Display { .. } if !self.env.gnome_screencast() => {
                CaptureTarget::Display {
                    display_id: DESKTOP_DISPLAY_ID,
                }
            }
            other => other,
        }
    }

    fn targets(&self) -> Option<Result<Targets, CmError>> {
        // On GNOME the displays and windows are the real ones the screenshot code lists, so
        // an id from the Record sheet or `list_targets` means the same thing here.
        if self.env.gnome_screencast() {
            return None;
        }
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
        DeviceNames {
            mic,
            camera: super::linux_camera::device_name(req, &self.env.cameras()),
        }
    }

    fn build(&self, cx: &BuildContext<'_>) -> Result<Vec<Box<dyn SessionTrack>>, CmError> {
        let p = cx.params;
        let mut plan = cx.plan.clone();
        // Without a cursor track (Wayland) the pointer stays in the picture even when a separate
        // track was asked for (the preflight warned). On X11 the user's choice stands.
        let cursor = if p.cursor {
            self.env
                .cursor_track(super::windows::cursor_target(&plan, cx.targets))
        } else {
            None
        };
        if cursor.is_none() {
            plan.show_cursor = true;
        }
        let target = if self.env.gnome_screencast() {
            pw_target(&plan, cx.targets, self.env.display_session())?
        } else {
            PwTarget::Screen
        };
        let mut tracks = vec![self.env.screen_track_for(plan, target)];
        tracks.extend(cursor);
        tracks.extend(self.mic_track(p));
        tracks.extend(self.system_track(p));
        if let Some(c) = &p.camera {
            tracks.push(self.env.camera_track(super::linux_camera::config(c))?);
        }
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
        let mut v = json!({
            "platform": "linux",
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
                "clicks": self.env.display_session() == "x11",
                "mode": if self.env.display_session() == "x11" { "track" } else { "embedded" },
                "note": if self.env.display_session() == "x11" {
                    "The pointer is recorded as a separate track (positions, clicks, shapes)."
                } else {
                    CURSOR_NOTE
                },
                "missing_permission": Value::Null,
            },
        });
        if let Some(o) = v.as_object_mut() {
            o.extend(
                super::linux_camera::devices(self.env.cameras(), self.env.camera_permission())
                    .map(|(k, val)| (k.to_owned(), val)),
            );
        }
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cm_capture::record::{plan_capture, ScreenSettings};
    use cm_capture::{RegionRect, WindowInfo};
    use cm_core::PhysicalRect;

    fn display(id: u32, x: i32, w: u32, h: u32, scale: f64) -> DisplayInfo {
        DisplayInfo {
            id,
            name: format!("d{id}"),
            bounds: PhysicalRect {
                origin: PhysicalPoint { x, y: 0 },
                size: PhysicalSize {
                    width: w,
                    height: h,
                },
            },
            scale: ScaleFactor::new(scale).unwrap(),
            is_primary: id == 1,
            is_builtin: false,
        }
    }

    fn targets() -> Targets {
        Targets {
            displays: vec![
                display(1, 0, 1920, 1080, 1.0),
                display(2, 1920, 2880, 1800, 2.0),
            ],
            windows: vec![WindowInfo {
                id: 7,
                title: "t".into(),
                app: "a".into(),
                pid: 1,
                bounds: PhysicalRect {
                    origin: PhysicalPoint { x: 100, y: 50 },
                    size: PhysicalSize {
                        width: 800,
                        height: 600,
                    },
                },
                scale: ScaleFactor::ONE,
                display_id: Some(1),
                minimized: false,
            }],
        }
    }

    fn plan(target: CaptureTarget) -> CapturePlan {
        plan_capture(&ScreenSettings::new(target), &targets()).unwrap()
    }

    /// REC-1: a display is the compositor's rectangle of that monitor, in logical px.
    #[test]
    fn a_display_maps_to_its_logical_rectangle() {
        let t = pw_target(
            &plan(CaptureTarget::Display { display_id: 2 }),
            &targets(),
            "wayland",
        )
        .unwrap();
        assert_eq!(
            t,
            PwTarget::Area {
                x: 960,
                y: 0,
                width: 1440,
                height: 900
            }
        );
    }

    /// REC-1.AC / CAP-1.AC2: a region is exact (the crop is relative to its display).
    #[test]
    fn a_region_is_offset_by_its_display_and_keeps_its_size() {
        let p = plan(CaptureTarget::Region {
            display_id: 1,
            rect: RegionRect::Physical(PhysicalRect {
                origin: PhysicalPoint { x: 10, y: 20 },
                size: PhysicalSize {
                    width: 640,
                    height: 360,
                },
            }),
        });
        assert_eq!(
            pw_target(&p, &targets(), "x11").unwrap(),
            PwTarget::Area {
                x: 10,
                y: 20,
                width: 640,
                height: 360
            }
        );
    }

    /// A window is its rectangle on X11; on Wayland the error says what works.
    #[test]
    fn a_window_works_on_x11_and_is_refused_with_the_reason_on_wayland() {
        let p = plan(CaptureTarget::Window { window_id: 7 });
        assert_eq!(
            pw_target(&p, &targets(), "x11").unwrap(),
            PwTarget::Area {
                x: 100,
                y: 50,
                width: 800,
                height: 600
            }
        );
        let e = pw_target(&p, &targets(), "wayland").unwrap_err();
        assert!(e.to_string().contains("X11 session"), "{e}");
    }
}
