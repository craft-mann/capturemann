//! Platform-neutral camera types: device descriptions, configuration, frames and events.

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// What kind of device a camera is. `Continuity` is an iPhone/iPad used as a webcam.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CameraKind {
    BuiltIn,
    External,
    Continuity,
    /// Software cameras (OBS Virtual Camera, Snap Camera, ...). Best effort, see `list_cameras`.
    Virtual,
}

/// One capture mode a camera offers (deduplicated by size; frame-rate range merged).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CameraMode {
    pub width: u32,
    pub height: u32,
    pub min_fps: f64,
    pub max_fps: f64,
}

/// A camera as listed by `list_devices` (specs/02-agent-interface.md, CAM-4: `{ id, name, max_size }`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CameraInfo {
    /// Stable device id (AVCaptureDevice `uniqueID`). Store this as "last camera".
    pub id: String,
    pub name: String,
    pub kind: CameraKind,
    /// True for the system default camera (AVCaptureDevice default for video).
    pub is_default: bool,
    /// Largest mode by pixel count.
    pub max_size: [u32; 2],
    /// Highest frame rate on any mode.
    pub max_fps: f64,
    /// Supported modes, largest first.
    pub modes: Vec<CameraMode>,
    /// Another application is using the device. Informational: most cameras can be shared.
    pub in_use_elsewhere: bool,
}

impl CameraInfo {
    /// True if a mode of at least `res` pixels in height exists at `fps` (within 0.5 fps).
    pub fn supports(&self, res: CameraResolution, fps: u32) -> bool {
        self.modes.iter().any(|m| {
            m.height >= res.height()
                && m.min_fps - 0.5 <= f64::from(fps)
                && f64::from(fps) <= m.max_fps + 0.5
        })
    }
}

/// Webcam track setting (specs/00-product-spec.md: Off, 720p, 1080p; Off is "no recorder").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CameraResolution {
    #[serde(rename = "720p")]
    P720,
    #[serde(rename = "1080p")]
    P1080,
}

impl CameraResolution {
    pub fn height(self) -> u32 {
        match self {
            CameraResolution::P720 => 720,
            CameraResolution::P1080 => 1080,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            CameraResolution::P720 => "720p",
            CameraResolution::P1080 => "1080p",
        }
    }
}

/// Default for [`CameraConfig::latency_compensation_ms`].
///
/// 0 ms: the S6 run showed the camera's sample PTS arriving about 75 ms after the screen's, but
/// that is the *delivery* latency of the pipeline; whether the PTS itself is late relative to the
/// optical event is exactly what the flash test measures (tests/manual/CAM-2.md). Update this
/// constant (or the per-device table that replaces it) from that measurement; until then it is
/// not applied.
pub const DEFAULT_LATENCY_COMPENSATION_MS: f64 = 0.0;

/// How the camera track is recorded.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CameraConfig {
    /// `None` = the system default camera.
    pub device_id: Option<String>,
    pub resolution: CameraResolution,
    /// Output frame rate of the track (the device is asked for this rate; default 30).
    pub fps: u32,
    /// Display mirror flag. Metadata only: stored in `TrackInfo.details.mirror`, never applied to
    /// the stored pixels (CAM-2 AC3).
    pub mirror: bool,
    /// Milliseconds the camera's sample PTS is *later* than the real event; frames are shifted
    /// earlier by this much on the session clock. Negative shifts later.
    pub latency_compensation_ms: f64,
    /// How long `start` waits for the first frame before deciding the device is busy/dead.
    pub first_frame_timeout_ms: u64,
}

impl Default for CameraConfig {
    fn default() -> Self {
        CameraConfig {
            device_id: None,
            resolution: CameraResolution::P1080,
            fps: 30,
            mirror: true,
            latency_compensation_ms: DEFAULT_LATENCY_COMPENSATION_MS,
            first_frame_timeout_ms: 3000,
        }
    }
}

impl CameraConfig {
    pub fn first_frame_timeout(&self) -> Duration {
        Duration::from_millis(self.first_frame_timeout_ms)
    }
}

/// A frame as delivered by the OS, before any retiming.
#[derive(Debug)]
pub struct CameraFrame {
    /// The sample's presentation time on the host clock (`CLOCK_UPTIME_RAW` ns): when the camera
    /// stack says the frame was captured. This is what the recorder retimes from.
    pub capture_host_ns: u64,
    /// Host clock when the callback received the frame. Used only for latency statistics.
    pub delivered_host_ns: u64,
    pub width: u32,
    pub height: u32,
    /// The platform sample (macOS: a retained `CMSampleBuffer`). `None` for synthetic frames.
    pub native: Option<crate::NativeSample>,
}

/// Device-level events from a source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CameraEvent {
    /// The device was unplugged or the system took it away. No more frames until `Reconnected`.
    Disconnected,
    /// The device is delivering again.
    Reconnected,
    /// A fatal runtime error (the platform's message).
    Error(String),
    /// The OS withdrew camera access while the session was running (AVFoundation
    /// `AVErrorApplicationIsNotAuthorizedToUseDevice`). Typed so the daemon can say why and
    /// offer "Open Permissions" (PRM-1).
    PermissionRevoked,
    /// The OS dropped this many frames before delivering them (the consumer was too slow).
    FramesDropped(u32),
}

/// Receiver of frames and events. Called on the platform's capture thread: keep it short.
pub trait FrameSink: Send + Sync {
    fn on_frame(&self, frame: &CameraFrame);
    fn on_event(&self, event: CameraEvent);
}

/// Which capture the source is opened for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceRole {
    Record,
    /// Preview at roughly this many frames per second.
    Preview {
        fps: u32,
    },
}

/// What a source needs to open a device.
#[derive(Debug, Clone)]
pub struct OpenRequest {
    pub device_id: Option<String>,
    pub resolution: CameraResolution,
    pub fps: u32,
    pub role: SourceRole,
    pub first_frame_timeout: Duration,
}

/// What an opened source reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceInfo {
    pub device_id: String,
    pub device_name: String,
}

/// A camera engine. macOS: the shared AVCaptureSession; tests: [`crate::FakeCameraSource`].
pub trait CameraSource: Send {
    /// Starts delivering to `sink`. Blocks until the first frame arrives or the timeout elapses.
    /// Errors: `permission_missing(camera)`, `not_found`, `device_busy`.
    fn open(
        &mut self,
        req: &OpenRequest,
        sink: std::sync::Arc<dyn FrameSink>,
    ) -> Result<SourceInfo, cm_core::CmError>;
    /// Stops delivering. Idempotent.
    fn close(&mut self);
}
