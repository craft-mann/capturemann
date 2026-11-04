//! The parts of the Windows.Graphics.Capture screen source that need no Windows API: mapping the
//! frame time WGC reports onto the session's host clock, clamping the region to the frame, and
//! copying a mapped GPU surface into tight rows. Pure Rust, tested on every OS; the capture itself
//! is `sys/win_wgc.rs` and the source glue `record/windows.rs`.

use cm_core::PhysicalRect;
use cm_encode::nv12::SrcRect;

/// WGC stamps a frame with `SystemRelativeTime`, a `TimeSpan` of 100 ns units on the system's
/// performance-counter timeline. The session clock reads `cm_platform::monotonic_ns`, which is
/// `QueryPerformanceCounter` in nanoseconds, so the conversion is a multiplication.
pub fn stamp_to_ns(hundred_ns: i64) -> u64 {
    u64::try_from(hundred_ns).unwrap_or(0).saturating_mul(100)
}

/// A frame stamp more than this old, or further in the future than [`MAX_FUTURE_NS`], is not on
/// the host clock's timeline (a capture stack that stamps from another clock): the arrival time
/// stands in for it.
pub const MAX_PAST_NS: u64 = 30_000_000_000;
pub const MAX_FUTURE_NS: u64 = 1_000_000_000;

/// Decides, once, whether WGC's stamps can be trusted as host-clock times, then applies the
/// decision to every frame. Frames stamped before the session origin must stay stamped before
/// it (the pipeline drops them), so a trusted stamp is used as given; an untrusted clock falls
/// back to arrival time, which is later than the capture by the delivery delay (a few ms).
#[derive(Debug, Clone, Default)]
pub struct StampMapper {
    trusted: Option<bool>,
}

impl StampMapper {
    /// The host-clock time of a frame stamped `stamp_ns` that arrives at `now_ns`.
    pub fn host_ns(&mut self, stamp_ns: u64, now_ns: u64) -> u64 {
        let plausible = stamp_ns <= now_ns.saturating_add(MAX_FUTURE_NS)
            && now_ns.saturating_sub(stamp_ns) <= MAX_PAST_NS;
        let trusted = *self.trusted.get_or_insert(plausible);
        if !trusted {
            return now_ns;
        }
        // A stamp a hair in the future (clock granularity) is clamped to now; it cannot be later
        // than the moment it arrived.
        stamp_ns.min(now_ns)
    }

    /// `Some(false)` once the first frame showed that the stamps are not on the host timeline.
    pub fn trusted(&self) -> Option<bool> {
        self.trusted
    }
}

/// The part of a `frame_w` x `frame_h` frame to keep for a region crop (physical px relative to
/// the frame's top left): the crop clamped to the frame. `None` for no crop, or when nothing of
/// the crop is inside the frame.
pub fn clamp_crop(crop: Option<PhysicalRect>, frame_w: u32, frame_h: u32) -> Option<SrcRect> {
    let c = crop?;
    let (x0, y0) = (i64::from(c.origin.x).max(0), i64::from(c.origin.y).max(0));
    let x1 = (i64::from(c.origin.x) + i64::from(c.size.width)).min(i64::from(frame_w));
    let y1 = (i64::from(c.origin.y) + i64::from(c.size.height)).min(i64::from(frame_h));
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    Some(SrcRect {
        x: x0 as u32,
        y: y0 as u32,
        w: (x1 - x0) as u32,
        h: (y1 - y0) as u32,
    })
}

/// Copies the `w` x `h` pixels at (`x`, `y`) out of mapped surface memory with `pitch` bytes per
/// row (BGRA, four bytes a pixel) into tight rows. `None` when the memory is too short.
pub fn copy_rows(
    src: &[u8],
    pitch: usize,
    x: usize,
    y: usize,
    w: usize,
    h: usize,
) -> Option<Vec<u8>> {
    let row = w.checked_mul(4)?;
    let last_end = (y + h.checked_sub(1)?)
        .checked_mul(pitch)?
        .checked_add(x.checked_mul(4)?)?
        .checked_add(row)?;
    if src.len() < last_end || pitch < x * 4 + row {
        return None;
    }
    let mut out = Vec::with_capacity(row * h);
    for r in 0..h {
        let s = (y + r) * pitch + x * 4;
        out.extend_from_slice(&src[s..s + row]);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cm_core::{PhysicalPoint, PhysicalSize};

    fn rect(x: i32, y: i32, w: u32, h: u32) -> PhysicalRect {
        PhysicalRect {
            origin: PhysicalPoint { x, y },
            size: PhysicalSize {
                width: w,
                height: h,
            },
        }
    }

    #[test]
    fn stamps_are_hundred_nanosecond_units() {
        assert_eq!(stamp_to_ns(10_000_000), 1_000_000_000);
        assert_eq!(stamp_to_ns(0), 0);
        assert_eq!(stamp_to_ns(-5), 0);
        assert_eq!(stamp_to_ns(i64::MAX), u64::MAX);
    }

    #[test]
    fn stamps_on_the_host_timeline_are_used_as_given() {
        let mut m = StampMapper::default();
        let now = 5_000_000_000_000;
        assert_eq!(m.host_ns(now - 20_000_000, now), now - 20_000_000);
        assert_eq!(m.trusted(), Some(true));
        // A frame a few ms in the future (clock granularity) is clamped to its arrival.
        assert_eq!(m.host_ns(now + 3_000_000, now), now);
    }

    #[test]
    fn a_first_frame_before_the_session_origin_stays_before_it() {
        // The origin is taken before the capture starts; the first frame WGC hands over can be
        // 200 ms old (the screen last changed then). It must keep its old time so the pipeline
        // drops it, not be moved to "now".
        let mut m = StampMapper::default();
        let now = 9_000_000_000;
        assert_eq!(m.host_ns(now - 200_000_000, now), now - 200_000_000);
    }

    #[test]
    fn stamps_from_another_clock_fall_back_to_arrival_time() {
        let mut m = StampMapper::default();
        let now = 5_000_000_000_000;
        // Hours away: not this timeline.
        assert_eq!(m.host_ns(1_000_000_000, now), now);
        assert_eq!(m.trusted(), Some(false));
        // The decision sticks, so later frames are not mixed between two timelines.
        assert_eq!(
            m.host_ns(now - 1_000_000, now + 33_000_000),
            now + 33_000_000
        );
        // A stamp from the far future is not on the timeline either.
        let mut m = StampMapper::default();
        assert_eq!(m.host_ns(now + 60_000_000_000, now), now);
    }

    #[test]
    fn crop_is_clamped_to_the_frame() {
        assert_eq!(clamp_crop(None, 100, 100), None);
        let r = clamp_crop(Some(rect(10, 20, 30, 40)), 100, 100).unwrap();
        assert_eq!((r.x, r.y, r.w, r.h), (10, 20, 30, 40));
        let r = clamp_crop(Some(rect(90, 90, 30, 40)), 100, 100).unwrap();
        assert_eq!((r.x, r.y, r.w, r.h), (90, 90, 10, 10));
        let r = clamp_crop(Some(rect(-5, -5, 20, 20)), 100, 100).unwrap();
        assert_eq!((r.x, r.y, r.w, r.h), (0, 0, 15, 15));
        assert_eq!(clamp_crop(Some(rect(100, 0, 10, 10)), 100, 100), None);
        assert_eq!(clamp_crop(Some(rect(0, 0, 0, 10)), 100, 100), None);
    }

    #[test]
    fn rows_are_copied_without_the_pitch_padding() {
        // 4x3 pixels, pitch 24 bytes (8 bytes of padding per row); copy the 2x2 at (1, 1).
        let mut mem = vec![0u8; 24 * 3];
        for (i, b) in mem.iter_mut().enumerate() {
            *b = i as u8;
        }
        let out = copy_rows(&mem, 24, 1, 1, 2, 2).unwrap();
        assert_eq!(out.len(), 16);
        assert_eq!(&out[..8], &mem[24 + 4..24 + 12]);
        assert_eq!(&out[8..], &mem[48 + 4..48 + 12]);
        // The whole surface.
        assert_eq!(copy_rows(&mem, 24, 0, 0, 4, 3).unwrap().len(), 48);
    }

    #[test]
    fn short_or_inconsistent_memory_is_refused_not_read_past() {
        let mem = vec![0u8; 20];
        assert!(copy_rows(&mem, 16, 0, 0, 4, 2).is_none()); // second row would end at 32
        assert!(copy_rows(&mem, 8, 0, 0, 4, 1).is_none()); // pitch smaller than a row
        assert!(copy_rows(&mem, 16, 0, 0, 4, 0).is_none());
        assert!(copy_rows(&mem, usize::MAX, 0, 1, 1, 1).is_none()); // overflow
    }
}
