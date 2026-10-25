//! Live frames for the webcam bubble (CAM-1.AC1, CAM-4), before and during a recording.
//!
//! # Integration (documented decision, see `cm_camera::preview`)
//!
//! The daemon runs inside this process, so the recorder and the bubble share **one**
//! `AVCaptureSession` per camera: [`cm_camera::CameraPreview`] adds a video-data output to that same
//! session and removes it again when dropped, so the device is never opened twice and the bubble
//! shows the frames that are recorded. Frames arrive on the camera thread as small RGBA images
//! (640 px long edge, 15 fps, mirror applied for display only) and wait in a [`FrameSlot`]; the
//! bubble window picks the newest one up on its own timer, so a slow UI never backs up the camera.
//! The zero-copy IOSurface path of `PreviewFrame::io_surface_id` is not used: a 240 px bubble does
//! not need it, and GPUI 0.3.9 has no IOSurface-backed image source.
//!
//! `--demo-recording` swaps the camera for [`cm_camera::FakeCameraSource`] so the bubble can be
//! seen without touching the camera.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use cm_camera::{CameraPreview, FakeCameraSource, PreviewOptions, RgbaImage};
use cm_core::CmError;
use cm_i18n::tr;

/// Longest edge of a preview frame handed to the bubble (640 px covers a 320 pt bubble at 2x).
pub const PREVIEW_EDGE: u32 = 640;
/// Frames per second the bubble asks for.
pub const PREVIEW_FPS: u32 = 15;

/// Where the newest frame waits for the bubble.
///
/// The frame is swizzled to BGRA (the byte order GPUI's atlas expects) once, on the camera thread,
/// and then moved: camera thread -> slot -> bubble -> `RenderImage`, with no further copy of the
/// 1.2 MB buffer (R4-18). Only the bubble consumes frames, so taking one removes it.
#[derive(Default)]
pub struct FrameSlot {
    latest: Mutex<Option<image::RgbaImage>>,
    present: AtomicBool,
    seq: AtomicU64,
    mirror: AtomicBool,
    /// Why there are no frames (permission, camera busy, unplugged), for the placeholder.
    error: Mutex<Option<String>>,
}

impl FrameSlot {
    pub fn new(mirror: bool) -> Arc<FrameSlot> {
        let s = FrameSlot::default();
        s.mirror.store(mirror, Ordering::Relaxed);
        Arc::new(s)
    }

    pub fn set_mirror(&self, mirror: bool) {
        self.mirror.store(mirror, Ordering::Relaxed);
    }

    pub fn mirror(&self) -> bool {
        self.mirror.load(Ordering::Relaxed)
    }

    /// Stores `frame` (RGBA) as the newest one, converted to BGRA in place.
    pub fn push(&self, frame: RgbaImage) {
        let (w, h) = (frame.width, frame.height);
        let Some(buf) = image::RgbaImage::from_raw(w, h, to_bgra(frame).data) else {
            return;
        };
        *self.latest.lock().unwrap_or_else(PoisonError::into_inner) = Some(buf);
        self.present.store(true, Ordering::Release);
        self.seq.fetch_add(1, Ordering::Release);
        *self.error.lock().unwrap_or_else(PoisonError::into_inner) = None;
    }

    pub fn set_error(&self, message: Option<String>) {
        *self.error.lock().unwrap_or_else(PoisonError::into_inner) = message;
    }

    pub fn error(&self) -> Option<String> {
        self.error
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Takes the newest frame (BGRA) if it is newer than `seen`, with its sequence number.
    pub fn take_newer(&self, seen: u64) -> Option<(u64, image::RgbaImage)> {
        let now = self.seq.load(Ordering::Acquire);
        if now == seen {
            return None;
        }
        let f = self
            .latest
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()?;
        Some((now, f))
    }

    /// Whether the camera has delivered a frame since the last [`FrameSlot::clear`] (the bubble
    /// keeps showing the one it took).
    pub fn has_frame(&self) -> bool {
        self.present.load(Ordering::Acquire)
    }

    /// Forgets the frame (the camera stopped), so the placeholder shows.
    pub fn clear(&self) {
        *self.latest.lock().unwrap_or_else(PoisonError::into_inner) = None;
        self.present.store(false, Ordering::Release);
        self.seq.fetch_add(1, Ordering::Release);
    }
}

/// RGBA to BGRA in place: the byte order GPUI's atlas expects.
pub fn to_bgra(mut img: RgbaImage) -> RgbaImage {
    for px in img.data.as_chunks_mut::<4>().0 {
        px.swap(0, 2);
    }
    img
}

/// Which camera backs the bubble.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewSource {
    /// The real camera (the one the recorder uses).
    System,
    /// Synthetic frames (`--demo-recording`).
    Fake,
}

/// A running preview. Dropping it stops the frames (and releases the camera session if nothing
/// else uses it).
pub struct PreviewRunner {
    _preview: CameraPreview,
    stop_feeder: Arc<AtomicBool>,
}

impl Drop for PreviewRunner {
    fn drop(&mut self) {
        self.stop_feeder.store(true, Ordering::Relaxed);
    }
}

/// Starts a preview into `slot`. Blocks until the first frame or the timeout (run it off the UI
/// thread). Errors: `permission_missing(camera)`, `not_found`, `device_busy`.
pub fn start(
    source: PreviewSource,
    device_id: Option<String>,
    slot: Arc<FrameSlot>,
) -> Result<PreviewRunner, CmError> {
    let opts = PreviewOptions {
        device_id,
        fps: PREVIEW_FPS,
        ..PreviewOptions::default()
    };
    let (on_frame_slot, on_event_slot) = (slot.clone(), slot);
    let on_frame = move |f: &cm_camera::PreviewFrame| {
        if let Some(img) = f.to_rgba(PREVIEW_EDGE, on_frame_slot.mirror()) {
            on_frame_slot.push(img);
        }
    };
    let on_event = move |e: cm_camera::CameraEvent| {
        on_event_slot.set_error(match e {
            cm_camera::CameraEvent::Disconnected => Some(tr!("The camera was disconnected")),
            cm_camera::CameraEvent::Error(m) => Some(m),
            _ => None,
        });
    };
    let stop_feeder = Arc::new(AtomicBool::new(false));
    match source {
        PreviewSource::System => {
            let preview = CameraPreview::start(opts, on_frame, on_event)?;
            Ok(PreviewRunner {
                _preview: preview,
                stop_feeder,
            })
        }
        PreviewSource::Fake => {
            let (src, handle) = FakeCameraSource::new((1280, 720));
            let preview = CameraPreview::start_with(Box::new(src), opts, on_frame, on_event)?;
            let stop = stop_feeder.clone();
            std::thread::spawn(move || {
                let mut ns: u64 = 1_000_000_000;
                while !stop.load(Ordering::Relaxed) {
                    if !handle.deliver(ns, 5_000_000) {
                        break;
                    }
                    ns += 66_666_667;
                    std::thread::sleep(Duration::from_millis(66));
                }
            });
            Ok(PreviewRunner {
                _preview: preview,
                stop_feeder,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn img(v: u8) -> RgbaImage {
        RgbaImage {
            width: 2,
            height: 1,
            data: vec![v, 1, 2, 255, v, 3, 4, 255],
        }
    }

    /// CAM-1.AC1: the bubble gets the newest frame, once.
    #[test]
    fn the_slot_hands_out_each_frame_once() {
        let s = FrameSlot::new(true);
        assert!(s.take_newer(0).is_none());
        s.push(img(10));
        let (seq, f) = s.take_newer(0).unwrap();
        // Swizzled once on the way in: red and blue swapped.
        assert_eq!(f.as_raw()[2], 10);
        assert!(s.take_newer(seq).is_none());
        assert!(s.has_frame(), "the bubble keeps showing the frame it took");
        s.push(img(11));
        s.push(img(12));
        // A slow UI skips straight to the newest.
        assert_eq!(s.take_newer(seq).unwrap().1.as_raw()[2], 12);
        s.clear();
        assert!(!s.has_frame());
    }

    #[test]
    fn bgra_swaps_red_and_blue() {
        let f = to_bgra(img(9));
        assert_eq!(&f.data[..4], &[2, 1, 9, 255]);
    }

    #[test]
    fn mirror_flag_is_shared() {
        let s = FrameSlot::new(true);
        assert!(s.mirror());
        s.set_mirror(false);
        assert!(!s.mirror());
    }

    /// CAM-4: a fake camera feeds the slot with mirrored frames, and stops when dropped.
    #[test]
    fn fake_preview_feeds_the_slot() {
        let slot = FrameSlot::new(true);
        let run = start(PreviewSource::Fake, None, slot.clone()).unwrap();
        let mut got = None;
        for _ in 0..50 {
            if let Some(f) = slot.take_newer(0) {
                got = Some(f);
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let (_, f) = got.expect("a frame within one second");
        assert!(f.width() > 0 && f.width() <= PREVIEW_EDGE);
        // Mirrored synthetic gradient: the first column of the image is the source's last one
        // (red is byte 2 once swizzled to BGRA).
        assert!(f.as_raw()[2] > 200, "{}", f.as_raw()[2]);
        slot.set_mirror(false);
        drop(run);
    }

    #[test]
    fn camera_errors_show_in_the_placeholder() {
        let s = FrameSlot::new(true);
        s.set_error(Some("The camera was disconnected".into()));
        assert_eq!(s.error().as_deref(), Some("The camera was disconnected"));
        s.push(img(1));
        assert_eq!(s.error(), None);
    }
}
