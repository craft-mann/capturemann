//! Live preview for the bubble and the camera picker (CAM-4), without starting a recording.
//!
//! # Integration (decision)
//!
//! One `AVCaptureSession` per camera, owned by the process that records (the daemon, since
//! clients only talk to `cm-daemon`). The preview is an extra video-data output on that same
//! session, added when a [`CameraPreview`] starts and removed when the last one stops. The
//! recorder and previews therefore never open the device twice.
//!
//! Frames reach the overlay bubble as **small RGBA images** ([`PreviewFrame::to_rgba`], default
//! 640 px long edge, 15 fps, about 0.6 MB per frame), sent over the daemon's local socket or a
//! shared-memory ring and uploaded as a texture by `apps/overlay`. Alternatives rejected:
//!
//! * *Shared IOSurface lookup by id*: camera IOSurfaces are not global (`kIOSurfaceIsGlobal` is
//!   deprecated and the camera daemon does not set it); passing one between processes needs a
//!   Mach port or XPC transfer through a bootstrap-registered service, a lot of machinery for a
//!   240 px bubble. [`PreviewFrame::io_surface_id`] stays available for an in-process consumer
//!   (desktop app embedding the daemon) that wants a zero-copy texture.
//! * *Overlay opens its own session*: simplest, but a second client on the device. Built-in and
//!   most UVC cameras are multiplexed by macOS and work; some external, virtual and Continuity
//!   cameras are single-client or silently reduce the resolution/frame rate of the first client,
//!   which would break `camera.mp4`. It also shows different frames than the ones recorded.
//!   Keep it only as a debugging fallback.
//!
//! Mirror is applied in [`PreviewFrame::to_rgba`] (display only; the recording is never flipped).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use cm_core::CmError;

use crate::types::{
    CameraEvent, CameraFrame, CameraResolution, CameraSource, FrameSink, OpenRequest, SourceInfo,
    SourceRole,
};

/// Tightly packed RGBA8 (not premultiplied), row 0 at the top.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RgbaImage {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

/// One preview frame. Cheap to clone (the platform buffer is reference counted).
#[derive(Debug)]
pub struct PreviewFrame {
    pub width: u32,
    pub height: u32,
    pub capture_host_ns: u64,
    pub delivered_host_ns: u64,
    native: Option<crate::NativeSample>,
}

impl PreviewFrame {
    /// Downscales to at most `max_edge` px on the long side (nearest sample) and converts to
    /// RGBA, flipping horizontally when `mirror`. `None` if the buffer cannot be read (not BGRA).
    /// Synthetic frames (fakes) return a deterministic gradient.
    pub fn to_rgba(&self, max_edge: u32, mirror: bool) -> Option<RgbaImage> {
        match &self.native {
            Some(n) => n.to_rgba(max_edge, mirror),
            None => Some(synthetic_rgba(self.width, self.height, max_edge, mirror)),
        }
    }

    /// The IOSurface id of the frame's pixel buffer, for same-process zero-copy use.
    pub fn io_surface_id(&self) -> Option<u32> {
        self.native.as_ref().and_then(|n| n.io_surface_id())
    }
}

/// Target size when scaling `w x h` so the long edge is at most `max_edge` (never upscales).
pub fn scaled_size(w: u32, h: u32, max_edge: u32) -> (u32, u32) {
    let long = w.max(h).max(1);
    if long <= max_edge {
        return (w.max(1), h.max(1));
    }
    let s = |v: u32| ((u64::from(v) * u64::from(max_edge) / u64::from(long)).max(1)) as u32;
    (s(w), s(h))
}

fn synthetic_rgba(w: u32, h: u32, max_edge: u32, mirror: bool) -> RgbaImage {
    let (tw, th) = scaled_size(w, h, max_edge);
    let mut data = Vec::with_capacity((tw * th * 4) as usize);
    for y in 0..th {
        for x in 0..tw {
            let sx = if mirror { tw - 1 - x } else { x };
            data.extend_from_slice(&[
                (sx * 255 / tw.max(1)) as u8,
                (y * 255 / th.max(1)) as u8,
                0,
                255,
            ]);
        }
    }
    RgbaImage {
        width: tw,
        height: th,
        data,
    }
}

/// Preview settings.
#[derive(Debug, Clone, PartialEq)]
pub struct PreviewOptions {
    /// `None` = the system default camera.
    pub device_id: Option<String>,
    /// Frames per second delivered to the callback (the camera may run faster).
    pub fps: u32,
    /// Session resolution to ask for if this preview is the first user of the session. If a
    /// recorder already configured the session, its resolution stays.
    pub resolution: CameraResolution,
    pub first_frame_timeout: Duration,
}

impl Default for PreviewOptions {
    fn default() -> Self {
        PreviewOptions {
            device_id: None,
            fps: 15,
            resolution: CameraResolution::P720,
            first_frame_timeout: Duration::from_secs(3),
        }
    }
}

type PreviewCallback = dyn Fn(&PreviewFrame) + Send + Sync;

struct PreviewSink {
    cb: Box<PreviewCallback>,
    min_gap_ns: u64,
    last_ns: AtomicU64,
    on_event: Box<dyn Fn(CameraEvent) + Send + Sync>,
}

impl FrameSink for PreviewSink {
    fn on_frame(&self, f: &CameraFrame) {
        // Throttle on the capture stamp; the platform source already thins frames before this.
        let last = self.last_ns.load(Ordering::Relaxed);
        if last != 0 && f.capture_host_ns.saturating_sub(last) + 2_000_000 < self.min_gap_ns {
            return;
        }
        self.last_ns.store(f.capture_host_ns, Ordering::Relaxed);
        (self.cb)(&PreviewFrame {
            width: f.width,
            height: f.height,
            capture_host_ns: f.capture_host_ns,
            delivered_host_ns: f.delivered_host_ns,
            native: f.native.clone(),
        });
    }

    fn on_event(&self, e: CameraEvent) {
        (self.on_event)(e);
    }
}

/// A running preview. Dropping it stops it (and releases the session if nothing else uses it).
pub struct CameraPreview {
    source: Box<dyn CameraSource>,
    info: SourceInfo,
}

impl CameraPreview {
    /// Starts a preview on the platform camera. Does not record anything. Errors:
    /// `permission_missing(camera)`, `not_found`, `device_busy`.
    pub fn start(
        opts: PreviewOptions,
        on_frame: impl Fn(&PreviewFrame) + Send + Sync + 'static,
        on_event: impl Fn(CameraEvent) + Send + Sync + 'static,
    ) -> Result<Self, CmError> {
        Self::start_with(crate::platform::system_source(), opts, on_frame, on_event)
    }

    /// Starts a preview on any source (tests use the fake).
    pub fn start_with(
        mut source: Box<dyn CameraSource>,
        opts: PreviewOptions,
        on_frame: impl Fn(&PreviewFrame) + Send + Sync + 'static,
        on_event: impl Fn(CameraEvent) + Send + Sync + 'static,
    ) -> Result<Self, CmError> {
        let fps = opts.fps.clamp(1, 60);
        let sink = Arc::new(PreviewSink {
            cb: Box::new(on_frame),
            min_gap_ns: 1_000_000_000 / u64::from(fps),
            last_ns: AtomicU64::new(0),
            on_event: Box::new(on_event),
        });
        let info = source.open(
            &OpenRequest {
                device_id: opts.device_id,
                resolution: opts.resolution,
                fps: 30,
                role: SourceRole::Preview { fps },
                first_frame_timeout: opts.first_frame_timeout,
            },
            sink,
        )?;
        Ok(CameraPreview { source, info })
    }

    pub fn device(&self) -> &SourceInfo {
        &self.info
    }

    pub fn stop(mut self) {
        self.source.close();
    }
}

impl Drop for CameraPreview {
    fn drop(&mut self) {
        self.source.close();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::fake::FakeCameraSource;

    #[test]
    fn scaling_keeps_aspect_and_never_upscales() {
        assert_eq!(scaled_size(1920, 1080, 640), (640, 360));
        assert_eq!(scaled_size(1080, 1920, 640), (360, 640));
        assert_eq!(scaled_size(320, 240, 640), (320, 240));
    }

    #[test]
    fn preview_throttles_to_its_fps_and_stops_on_drop() {
        let (src, handle) = FakeCameraSource::new((1280, 720));
        let got = Arc::new(Mutex::new(Vec::new()));
        let g = got.clone();
        let p = CameraPreview::start_with(
            Box::new(src),
            PreviewOptions {
                fps: 10,
                ..PreviewOptions::default()
            },
            move |f| g.lock().unwrap().push(f.capture_host_ns),
            |_| {},
        )
        .unwrap();
        assert_eq!(p.device().device_name, "Fake Camera (preview)");
        // 30 fps camera for 1 s: expect about 10 frames.
        for i in 0..30u64 {
            handle.deliver(1_000_000_000 + i * 33_333_333, 5_000_000);
        }
        let n = got.lock().unwrap().len();
        assert!((9..=11).contains(&n), "{n}");
        assert!(matches!(
            handle.last_request().unwrap().role,
            SourceRole::Preview { fps: 10 }
        ));
        drop(p);
        assert!(!handle.is_open());
        assert!(!handle.deliver(9_000_000_000, 0));
    }

    #[test]
    fn mirror_flips_horizontally_in_the_preview_only() {
        let f = PreviewFrame {
            width: 100,
            height: 50,
            capture_host_ns: 0,
            delivered_host_ns: 0,
            native: None,
        };
        let a = f.to_rgba(100, false).unwrap();
        let b = f.to_rgba(100, true).unwrap();
        assert_eq!(a.data.len(), 100 * 50 * 4);
        assert_eq!(a.data[0], 0);
        assert!(b.data[0] > 200); // the first column of the mirrored image is the last of the source
    }
}
