//! The camera track's file writer, behind a small trait so it can later be swapped for the shared
//! fragmented-MP4 writer in `cm-encode`.
//!
//! Contract: frames arrive already retimed (strictly increasing `pts` in file ticks: session time
//! minus the pause gaps, relative to the session origin, not rebased to the first frame), the writer encodes H.264 without
//! B-frames and emits a fragmented MP4 whose fragments are about [`FRAGMENT_SECONDS`] long, so a
//! crash loses at most one fragment (REC-8). The file's media timescale is 120,000.

use std::path::Path;

use cm_core::{CmError, MediaTime};

use crate::types::CameraFrame;

/// Fragment length (specs/01-technical-design.md, "Recording pipeline").
pub const FRAGMENT_SECONDS: u32 = 2;
/// Output cap: CAM-2 records the camera's native size up to 1080p.
pub const MAX_OUTPUT_HEIGHT: u32 = 1080;

/// What the writer is created with (from the first frame's real size).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriterSpec {
    /// Output size (already limited to [`MAX_OUTPUT_HEIGHT`] and even).
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate_bps: u32,
}

impl WriterSpec {
    /// Spec for frames of `src` size: native size capped at 1080p, ~8 Mbps at 1080p30 scaled by area.
    pub fn for_source(src: (u32, u32), fps: u32) -> Self {
        let (width, height) = output_size(src.0, src.1, MAX_OUTPUT_HEIGHT);
        WriterSpec {
            width,
            height,
            fps,
            bitrate_bps: bitrate_for(width, height, fps),
        }
    }
}

/// Native size, scaled down (aspect kept) to at most `max_h` tall, both sides even.
pub fn output_size(w: u32, h: u32, max_h: u32) -> (u32, u32) {
    let (mut w, mut h) = (w.max(2), h.max(2));
    if h > max_h {
        w = (u64::from(w) * u64::from(max_h) / u64::from(h)) as u32;
        h = max_h;
    }
    (w & !1, h & !1)
}

/// About 8 Mbps at 1920x1080x30, proportional to pixels and frame rate, within 1.5..12 Mbps.
pub fn bitrate_for(w: u32, h: u32, fps: u32) -> u32 {
    let px = u64::from(w) * u64::from(h);
    let bps = 8_000_000u64 * px / (1920 * 1080) * u64::from(fps.max(1)) / 30;
    bps.clamp(1_500_000, 12_000_000) as u32
}

/// Result of appending one frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppendOutcome {
    Written,
    /// The encoder input could not take the frame now (back-pressure); the frame was dropped.
    NotReady,
    /// The writer has failed; it will not accept more.
    Failed(String),
}

/// One finished (or flushed) fragment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FragmentInfo {
    pub start: MediaTime,
    pub duration: MediaTime,
    pub bytes: u64,
}

/// What a finished file contains.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WriterReport {
    pub fragments: Vec<FragmentInfo>,
    pub bytes: u64,
}

pub trait FragmentWriter: Send {
    /// Encodes `frame` at file time `pts` (session time minus pause gaps) for `duration`.
    fn append(&mut self, frame: &CameraFrame, pts: MediaTime, duration: MediaTime)
        -> AppendOutcome;
    /// Forces out the fragment in progress (used when the device disappears, so everything
    /// captured so far is on disk). Best effort.
    fn flush(&mut self);
    /// Finishes the file.
    fn finish(&mut self) -> Result<WriterReport, CmError>;
}

pub trait WriterFactory: Send + Sync {
    fn create(&self, path: &Path, spec: &WriterSpec) -> Result<Box<dyn FragmentWriter>, CmError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_is_native_up_to_1080p_and_even() {
        assert_eq!(output_size(1920, 1080, 1080), (1920, 1080));
        assert_eq!(output_size(1280, 960, 1080), (1280, 960)); // non-standard mode kept native
        assert_eq!(output_size(3840, 2160, 1080), (1920, 1080));
        assert_eq!(output_size(2561, 1441, 1080), (1918, 1080));
    }

    #[test]
    fn bitrate_matches_the_spec_at_1080p30() {
        assert_eq!(bitrate_for(1920, 1080, 30), 8_000_000);
        assert_eq!(bitrate_for(1280, 720, 30), 3_555_555);
        assert_eq!(bitrate_for(320, 240, 30), 1_500_000);
    }
}
