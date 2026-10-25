//! Camera enumeration, preview frames and the camera track (CAM-1..CAM-4).
//!
//! * [`list_cameras`] and [`check_camera_permission`] feed `list_devices` and the setup panel.
//! * [`CameraPreview`] delivers live frames for the bubble/picker without recording.
//! * [`CameraRecorder`] implements `cm_core::TrackRecorder` and writes `camera.mp4`:
//!   H.264 (hardware), 30 fps, fragmented every 2 s, no B-frames, timestamps from the capture PTS
//!   on the session clock (see `retime`), `mirror` kept as metadata only.
//!
//! One `AVCaptureSession` per camera per process: the recorder and any previews share it (see
//! `preview` for the bubble integration decision). Everything platform-specific lives in the
//! `mac` module (the only place with `unsafe`); the recorder logic is platform neutral and tested
//! with [`FakeCameraSource`] / [`FakeWriterFactory`].

#![cfg_attr(test, allow(clippy::unwrap_used))]

pub mod fake;
pub mod flash;
pub mod latency;
pub mod mp4;
pub mod permission;
pub mod preview;
pub mod recorder;
pub mod retime;
pub mod types;
pub mod watcher;
pub mod writer;

#[cfg(target_os = "macos")]
mod mac;
#[cfg(target_os = "macos")]
use mac as platform;
#[cfg(not(target_os = "macos"))]
mod unsupported;
#[cfg(not(target_os = "macos"))]
use unsupported as platform;

pub use fake::{FakeCameraHandle, FakeCameraSource, FakeWriterFactory, WriterLog};
pub use latency::{LatencyStats, LatencySummary};
pub use permission::{check_camera_permission, request_camera_access, CameraPermission};
pub use platform::NativeSample;
pub use preview::{CameraPreview, PreviewFrame, PreviewOptions, RgbaImage};
pub use recorder::{CameraRecorder, RecorderEvent, CAMERA_FILE};
pub use retime::{Retimed, Retimer};
pub use types::*;
pub use watcher::{
    change_event_name, change_payload, diff_devices, system_watcher, CameraWatcher, ChangeSink,
    DeviceChange, FakeCameraWatcher, FakeWatcherHandle, PollingWatcher,
};
pub use writer::{
    AppendOutcome, FragmentInfo, FragmentWriter, WriterFactory, WriterReport, WriterSpec,
    FRAGMENT_SECONDS,
};

use cm_core::CmError;

/// Lists the cameras. Needs no permission on macOS for names/ids, and never prompts. Includes
/// built-in, external (USB/UVC), Continuity and virtual cameras; the default camera is flagged.
///
/// Virtual cameras: CoreMediaIO *camera extensions* (OBS 30+, Camo, ...) are real AVFoundation
/// devices and are listed. Legacy DAL plugins appear only if the host process may load them (they
/// are blocked in hardened-runtime apps without the library-validation exception, and macOS 13+
/// is deprecating them), so their listing is best effort. Continuity Camera needs
/// `NSCameraUseContinuityCameraDeviceType = YES` in the bundle's Info.plist to be reported as its
/// own type; without it the iPhone is still listed, flagged `external`.
pub fn list_cameras() -> Result<Vec<CameraInfo>, CmError> {
    platform::list_cameras()
}

/// Picks the camera to use: the remembered one if it is still present, else the default (CAM-4
/// AC2). The bool is true when the remembered camera was missing and a fallback was chosen.
pub fn resolve_camera<'a>(
    cameras: &'a [CameraInfo],
    remembered: Option<&str>,
) -> Result<(&'a CameraInfo, bool), CmError> {
    if let Some(id) = remembered {
        if let Some(c) = cameras.iter().find(|c| c.id == id) {
            return Ok((c, false));
        }
    }
    let fallback = cameras
        .iter()
        .find(|c| c.is_default)
        .or_else(|| cameras.first())
        .ok_or_else(|| CmError::not_found("No camera found"))?;
    Ok((fallback, remembered.is_some()))
}

/// Measures camera delivery latency (capture PTS to callback) for `secs` seconds (1..=10)
/// through the real source, discarding the frames (nothing is written). Requires camera
/// permission; the caller is responsible for consent. See `examples/sync_probe.rs`.
pub fn probe_delivery_latency(
    device_id: Option<&str>,
    resolution: CameraResolution,
    secs: u64,
) -> Result<LatencyReport, CmError> {
    use std::sync::{Arc, Mutex};
    struct Probe(Mutex<(LatencyStats, Vec<u64>)>);
    impl FrameSink for Probe {
        fn on_frame(&self, f: &CameraFrame) {
            if let Ok(mut g) = self.0.lock() {
                g.0.push(f.delivered_host_ns, f.capture_host_ns);
                g.1.push(f.capture_host_ns);
            }
        }
        fn on_event(&self, _e: CameraEvent) {}
    }
    check_camera_permission().require()?;
    let secs = secs.clamp(1, 10);
    let probe = Arc::new(Probe(Mutex::new((LatencyStats::default(), Vec::new()))));
    let mut src = platform::system_source();
    let info = src.open(
        &OpenRequest {
            device_id: device_id.map(str::to_string),
            resolution,
            fps: 30,
            role: SourceRole::Record,
            first_frame_timeout: std::time::Duration::from_secs(3),
        },
        probe.clone(),
    )?;
    std::thread::sleep(std::time::Duration::from_secs(secs));
    src.close();
    let g = probe
        .0
        .lock()
        .map_err(|_| CmError::internal("probe poisoned"))?;
    let gaps: Vec<f32> =
        g.1.windows(2)
            .map(|w| (w[1] as f64 - w[0] as f64) as f32 / 1e6)
            .collect();
    Ok(LatencyReport {
        device: info,
        frames: g.1.len(),
        latency: g.0.summary(),
        interval_ms: latency::summarize(&gaps),
    })
}

/// Result of [`probe_delivery_latency`].
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct LatencyReport {
    #[serde(skip)]
    pub device: SourceInfo,
    pub frames: usize,
    /// `delivered - capture PTS` in ms.
    pub latency: Option<LatencySummary>,
    /// Gaps between consecutive capture PTS in ms.
    pub interval_ms: Option<LatencySummary>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cam(id: &str, default: bool) -> CameraInfo {
        CameraInfo {
            id: id.into(),
            name: format!("Cam {id}"),
            kind: CameraKind::BuiltIn,
            is_default: default,
            max_size: [1920, 1080],
            max_fps: 30.0,
            modes: vec![CameraMode {
                width: 1920,
                height: 1080,
                min_fps: 1.0,
                max_fps: 30.0,
            }],
            in_use_elsewhere: false,
        }
    }

    #[test]
    fn remembered_camera_wins_and_missing_falls_back_to_default() {
        let cams = [cam("a", false), cam("b", true)];
        let (c, fell_back) = resolve_camera(&cams, Some("a")).unwrap();
        assert_eq!((c.id.as_str(), fell_back), ("a", false));
        let (c, fell_back) = resolve_camera(&cams, Some("gone")).unwrap();
        assert_eq!((c.id.as_str(), fell_back), ("b", true));
        let (c, fell_back) = resolve_camera(&cams, None).unwrap();
        assert_eq!((c.id.as_str(), fell_back), ("b", false));
    }

    #[test]
    fn no_cameras_is_not_found() {
        let e = resolve_camera(&[], None).unwrap_err();
        assert_eq!(e.code(), cm_core::ErrorCode::NotFound);
    }

    #[test]
    fn enumeration_shape_serializes_for_list_devices() {
        let v = serde_json::to_value(cam("a", true)).unwrap();
        assert_eq!(v["id"], "a");
        assert_eq!(v["kind"], "built_in");
        assert_eq!(v["is_default"], true);
        assert_eq!(v["max_size"], serde_json::json!([1920, 1080]));
        assert_eq!(v["modes"][0]["height"], 1080);
        assert!(cam("a", true).supports(CameraResolution::P1080, 30));
        assert!(!cam("a", true).supports(CameraResolution::P1080, 60));
    }

    #[test]
    fn resolution_settings_serialize_like_the_product_spec() {
        assert_eq!(
            serde_json::to_string(&CameraResolution::P720).unwrap(),
            "\"720p\""
        );
        assert_eq!(
            serde_json::from_str::<CameraResolution>("\"1080p\"").unwrap(),
            CameraResolution::P1080
        );
    }
}
