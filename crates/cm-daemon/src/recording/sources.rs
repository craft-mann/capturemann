//! Where the engine gets its tracks, permissions and device lists: the [`SourceFactory`] seam.
//!
//! [`SystemSources`] is the real thing (ScreenCaptureKit and VideoToolbox, `cpal`, AVFoundation,
//! the cursor event tap). Tests and embedders without hardware inject a fake factory (see
//! `cm_daemon::testing::FakeSources`). Every method is synchronous and may block: the engine
//! calls them from `spawn_blocking`.
//!
//! None of the checks here ever prompts for a permission: a missing one is reported and the
//! caller decides (REC-1 edge case "Screen Recording permission missing").

use cm_capture::record::CapturePlan;
use cm_capture::Targets;
use cm_core::CmError;
use serde_json::Value;

use super::params::ResolvedStart;
use super::tracks::SessionTrack;

/// Which sensors a request turns on; what [`SourceFactory::preflight`] checks.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SensorRequest {
    pub mic: bool,
    pub mic_device: Option<String>,
    pub camera: bool,
    pub camera_device: Option<String>,
    pub system_audio: bool,
    pub cursor: bool,
}

impl SensorRequest {
    pub fn from_resolved(r: &ResolvedStart) -> Self {
        SensorRequest {
            mic: r.mic.is_some(),
            mic_device: r.mic.as_ref().and_then(|m| m.device_id.clone()),
            camera: r.camera.is_some(),
            camera_device: r.camera.as_ref().and_then(|c| c.device_id.clone()),
            system_audio: r.system_audio,
            cursor: r.cursor,
        }
    }
}

/// A non-blocking finding of the preflight (the recording still starts).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreflightWarning {
    /// The track the warning is about (`cursor`), if any.
    pub track: Option<String>,
    pub code: String,
    pub message: String,
}

/// Result of the permission preflight.
#[derive(Debug, Default)]
pub struct Preflight {
    /// Permissions or devices a requested sensor needs and does not have. The first one is
    /// returned to the caller as the error (`permission_missing(screen|microphone|camera)`).
    pub errors: Vec<CmError>,
    /// For example `input_monitoring_missing`: cursor clicks will not be recorded (REC-7.AC4).
    pub warnings: Vec<PreflightWarning>,
}

/// Human names of the chosen devices, for the confirmation dialog.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DeviceNames {
    pub mic: Option<String>,
    pub camera: Option<String>,
}

/// Everything a factory may need to build the tracks.
pub struct BuildContext<'a> {
    pub params: &'a ResolvedStart,
    /// The resolved screen plan (size, codec, crop, exclusions).
    pub plan: &'a CapturePlan,
    pub targets: &'a Targets,
}

/// Builds the tracks of a recording and answers device questions.
pub trait SourceFactory: Send + Sync {
    /// Permission and device checks for the requested sensors (screen is always checked).
    fn preflight(&self, req: &SensorRequest) -> Preflight;

    /// Names of the microphone and camera that would be used.
    fn device_names(&self, req: &SensorRequest) -> DeviceNames;

    /// The tracks, screen first. The screen track must be required (see
    /// [`SessionTrack::required`]).
    fn build(&self, cx: &BuildContext<'_>) -> Result<Vec<Box<dyn SessionTrack>>, CmError>;

    /// The `list_devices` result: `cameras`, `mics`, `system_audio` and the permission states.
    fn devices(&self) -> Value;

    /// Permissions the OS no longer grants among those the `running` tracks need (PRM-1):
    /// Screen for the screen track, Microphone for the mic, Camera for the camera. Polled by the
    /// engine every tick while recording, never prompts, and must be cheap. The engine only acts
    /// on a permission reported on two ticks in a row. Real factories read the OS state; the
    /// default is "nothing revoked".
    fn revoked_permissions(&self, running: &[cm_core::TrackKind]) -> Vec<cm_core::Permission> {
        let _ = running;
        Vec::new()
    }

    /// Called by the engine every tick on its async thread. A test seam (fault injection); real
    /// factories do nothing.
    fn tick_hook(&self) {}
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
mod real {
    use super::*;
    use cm_audio::{
        list_audio_devices, microphone_permission, screen_recording_permission, DeviceSelector,
        MicConfig, MicRecorder, SystemAudioConfig, SystemAudioRecorder,
    };
    use cm_camera::{
        check_camera_permission, list_cameras, resolve_camera, CameraConfig, CameraRecorder,
    };
    use cm_capture::record::RecorderOptions;
    use cm_core::{Permission, Rect};
    use cm_cursor::{CursorConfig, CursorRecorder, CursorTarget, TargetKind};
    use serde_json::json;

    use crate::recording::tracks::{
        CameraTrack, CursorTrack, MicTrack, ScreenTrack, SystemAudioTrack,
    };

    /// The real sources for this OS.
    #[derive(Debug, Default, Clone, Copy)]
    pub struct SystemSources;

    fn screen_track(plan: &CapturePlan) -> Box<dyn SessionTrack> {
        #[cfg(target_os = "macos")]
        let source = cm_capture::record::sck::SckSource::new();
        #[cfg(target_os = "windows")]
        let source = cm_capture::record::WgcSource;
        // VideoToolbox on macOS; when it rejects the plan the start fails (REC-2: no software
        // fallback in v0.1). Windows has no encoder yet.
        Box::new(ScreenTrack::new(
            plan.clone(),
            source,
            cm_encode::HardwareEncoder::create,
            RecorderOptions::default(),
        ))
    }

    /// The cursor's recorded area (see [`CursorTarget`]): macOS global space is logical points,
    /// Windows physical px.
    pub(super) fn cursor_target(plan: &CapturePlan, targets: &Targets) -> CursorTarget {
        let (div, tscale) = if cfg!(windows) {
            (1.0, 1.0)
        } else {
            (plan.scale.get(), plan.scale.get())
        };
        let rect = |x: f64, y: f64, w: f64, h: f64| Rect::new(x / div, y / div, w / div, h / div);
        let display = plan
            .display_id
            .and_then(|id| targets.displays.iter().find(|d| d.id == id));
        match (&plan.target, display) {
            (cm_capture::CaptureTarget::Display { display_id }, _) => {
                #[cfg(target_os = "macos")]
                {
                    cm_cursor::main_display_target(Some(*display_id))
                }
                #[cfg(not(target_os = "macos"))]
                {
                    let d = display
                        .cloned()
                        .unwrap_or_else(|| targets.displays[0].clone());
                    let _ = display_id;
                    CursorTarget::display(
                        d.id,
                        rect(
                            f64::from(d.bounds.origin.x),
                            f64::from(d.bounds.origin.y),
                            f64::from(d.bounds.size.width),
                            f64::from(d.bounds.size.height),
                        ),
                        tscale,
                    )
                }
            }
            (cm_capture::CaptureTarget::Region { .. }, Some(d)) => {
                let crop = plan.crop.unwrap_or(d.bounds);
                CursorTarget::from_global_rect(
                    TargetKind::Region,
                    Some(d.id),
                    rect(
                        f64::from(d.bounds.origin.x) + f64::from(crop.origin.x),
                        f64::from(d.bounds.origin.y) + f64::from(crop.origin.y),
                        f64::from(crop.size.width),
                        f64::from(crop.size.height),
                    ),
                    tscale,
                )
            }
            (cm_capture::CaptureTarget::Window { window_id }, _) => {
                let w = targets.windows.iter().find(|w| w.id == *window_id);
                match w {
                    Some(w) => CursorTarget::from_global_rect(
                        TargetKind::Window,
                        plan.display_id,
                        rect(
                            f64::from(w.bounds.origin.x),
                            f64::from(w.bounds.origin.y),
                            f64::from(w.bounds.size.width),
                            f64::from(w.bounds.size.height),
                        ),
                        tscale,
                    ),
                    None => CursorTarget::from_global_rect(
                        TargetKind::Window,
                        plan.display_id,
                        rect(
                            0.0,
                            0.0,
                            f64::from(plan.source_size.width),
                            f64::from(plan.source_size.height),
                        ),
                        tscale,
                    ),
                }
            }
            (_, None) => CursorTarget::from_global_rect(
                TargetKind::Region,
                plan.display_id,
                rect(
                    0.0,
                    0.0,
                    f64::from(plan.source_size.width),
                    f64::from(plan.source_size.height),
                ),
                tscale,
            ),
        }
    }

    impl SourceFactory for SystemSources {
        fn preflight(&self, req: &SensorRequest) -> Preflight {
            let mut out = Preflight::default();
            // Screen Recording is needed by the screen track and by system audio.
            if !screen_recording_permission().is_authorized() {
                out.errors.push(CmError::permission_missing(
                    Permission::Screen,
                    "Screen Recording access is off. Turn it on in System Settings > Privacy & \
                     Security > Screen Recording, then restart CaptureMann.",
                ));
            }
            if req.mic {
                if let Err(e) = cm_audio::require_microphone() {
                    out.errors.push(e);
                }
            }
            if req.camera {
                if let Err(e) = check_camera_permission().require() {
                    out.errors.push(e);
                }
            }
            if req.cursor {
                if let Some(p) = cm_cursor::preflight().missing_permission() {
                    out.warnings.push(PreflightWarning {
                        track: Some("cursor".to_owned()),
                        code: format!("{}_missing", p.as_str()),
                        message: "Cursor clicks will not be recorded (only the position): \
                                  Input Monitoring access is off."
                            .to_owned(),
                    });
                }
            }
            out
        }

        fn device_names(&self, req: &SensorRequest) -> DeviceNames {
            let mic = req.mic.then(|| {
                let devices = list_audio_devices();
                let want = req.mic_device.as_deref();
                devices
                    .mics
                    .iter()
                    .find(|m| match want {
                        Some(id) => m.id == id,
                        None => m.default,
                    })
                    .map(|m| m.name.clone())
                    .unwrap_or_else(|| match want {
                        Some(id) => id.to_owned(),
                        None => "the default microphone".to_owned(),
                    })
            });
            let camera = req.camera.then(|| {
                list_cameras()
                    .ok()
                    .and_then(|cams| {
                        resolve_camera(&cams, req.camera_device.as_deref())
                            .ok()
                            .map(|(c, _)| c.name.clone())
                    })
                    .unwrap_or_else(|| "the default camera".to_owned())
            });
            DeviceNames { mic, camera }
        }

        fn build(&self, cx: &BuildContext<'_>) -> Result<Vec<Box<dyn SessionTrack>>, CmError> {
            let p = cx.params;
            let mut tracks = vec![screen_track(cx.plan)];
            if p.cursor {
                tracks.push(Box::new(CursorTrack::new(CursorRecorder::new(
                    CursorConfig::new(cursor_target(cx.plan, cx.targets)),
                ))));
            }
            if let Some(m) = &p.mic {
                let cfg = MicConfig {
                    device: m
                        .device_id
                        .as_deref()
                        .map_or(DeviceSelector::Default, DeviceSelector::parse),
                    denoise: p.mic_denoise(),
                    ..MicConfig::default()
                };
                tracks.push(Box::new(MicTrack::new(MicRecorder::new(cfg))));
            }
            if p.system_audio {
                tracks.push(Box::new(SystemAudioTrack::new(SystemAudioRecorder::new(
                    SystemAudioConfig::default(),
                ))));
            }
            if let Some(c) = &p.camera {
                let cfg = CameraConfig {
                    device_id: c.device_id.clone(),
                    resolution: c.resolution,
                    mirror: c.mirror,
                    ..CameraConfig::default()
                };
                tracks.push(Box::new(CameraTrack::new(CameraRecorder::system(cfg))));
            }
            Ok(tracks)
        }

        fn revoked_permissions(&self, running: &[cm_core::TrackKind]) -> Vec<Permission> {
            use cm_core::TrackKind;
            let mut out = Vec::new();
            if running.contains(&TrackKind::Screen)
                && !screen_recording_permission().is_authorized()
            {
                out.push(Permission::Screen);
            }
            if running.contains(&TrackKind::Mic) && !microphone_permission().is_authorized() {
                out.push(Permission::Microphone);
            }
            if running.contains(&TrackKind::Camera) && !check_camera_permission().is_authorized() {
                out.push(Permission::Camera);
            }
            out
        }

        fn devices(&self) -> Value {
            let audio = list_audio_devices();
            let (cameras, cameras_error) = match list_cameras() {
                Ok(c) => (c, None),
                Err(e) => (Vec::new(), Some(e.to_string())),
            };
            let cursor = cm_cursor::preflight();
            json!({
                "cameras": cameras,
                "cameras_error": cameras_error,
                "camera_permission": check_camera_permission(),
                "mics": audio.mics,
                "microphone_permission": microphone_permission(),
                "microphone_permission_message": audio.microphone_permission_message,
                "system_audio": audio.system_audio,
                "screen_recording_permission": screen_recording_permission(),
                "cursor": {
                    "clicks": cursor.clicks,
                    "missing_permission": cursor.missing_permission().map(Permission::as_str),
                },
            })
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub use real::SystemSources;

/// The real sources are only built for macOS and Windows.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemSources;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
impl SourceFactory for SystemSources {
    fn preflight(&self, _req: &SensorRequest) -> Preflight {
        Preflight {
            errors: vec![CmError::invalid_input(
                "Recording is supported on macOS and Windows only",
            )],
            warnings: Vec::new(),
        }
    }

    fn device_names(&self, _req: &SensorRequest) -> DeviceNames {
        DeviceNames::default()
    }

    fn build(&self, _cx: &BuildContext<'_>) -> Result<Vec<Box<dyn SessionTrack>>, CmError> {
        Err(CmError::invalid_input(
            "Recording is supported on macOS and Windows only",
        ))
    }

    fn devices(&self) -> Value {
        serde_json::json!({
            "cameras": [], "cameras_error": "not supported on this OS", "mics": [],
            "system_audio": { "supported": false, "backend": "none" },
        })
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::real::cursor_target;
    use cm_capture::record::{plan_capture, ScreenSettings};
    use cm_capture::{CaptureTarget, DisplayInfo, RegionRect, Targets, WindowInfo};
    use cm_core::{PhysicalPoint, PhysicalRect, PhysicalSize, ScaleFactor};

    fn rect(x: i32, y: i32, w: u32, h: u32) -> PhysicalRect {
        PhysicalRect {
            origin: PhysicalPoint { x, y },
            size: PhysicalSize {
                width: w,
                height: h,
            },
        }
    }

    fn targets() -> Targets {
        Targets {
            displays: vec![DisplayInfo {
                id: 2,
                name: "ext".into(),
                // A second display to the right of a 1512 pt wide main display, 2x.
                bounds: rect(3024, 0, 3024, 1964),
                scale: ScaleFactor::new(2.0).unwrap(),
                is_primary: false,
            }],
            windows: vec![WindowInfo {
                id: 7,
                title: "w".into(),
                app: "a".into(),
                pid: 1,
                bounds: rect(3224, 100, 800, 600),
                scale: ScaleFactor::new(2.0).unwrap(),
                display_id: Some(2),
                minimized: false,
            }],
        }
    }

    /// REC-7: the cursor's recorded area uses the global logical space on macOS: a region is the
    /// display origin plus the crop, in points, and the target size is the screen track's size.
    #[test]
    fn region_and_window_cursor_targets_are_in_global_logical_space() {
        let t = targets();
        let region = plan_capture(
            &ScreenSettings::new(CaptureTarget::Region {
                display_id: 2,
                rect: RegionRect::Physical(rect(100, 80, 1280, 720)),
            }),
            &t,
        )
        .unwrap();
        let c = cursor_target(&region, &t);
        assert_eq!(c.origin, [1512.0 + 50.0, 40.0]);
        assert_eq!((c.size.width, c.size.height), (1280, 720));
        assert!((c.scale - 2.0).abs() < 1e-9);

        let window = plan_capture(
            &ScreenSettings::new(CaptureTarget::Window { window_id: 7 }),
            &t,
        )
        .unwrap();
        let c = cursor_target(&window, &t);
        assert_eq!(c.origin, [1612.0, 50.0]);
        assert_eq!((c.size.width, c.size.height), (800, 600));
    }
}
