//! Snapping and the frame grid (EDT-1: "Snapping (toggle N): to the playhead, segment boundaries,
//! silence boundaries and frame boundaries. Edits land on exact frames of the screen track;
//! keyboard ←/→ moves one frame, Shift+←/→ one second.").

use cm_core::{EditList, MediaTime, Span, TICKS_PER_SECOND};

/// How near (px) a time must be to a feature to snap to it.
pub const SNAP_PX: f32 = 8.0;

/// Frames of the screen track in session time: frame `n` starts at `origin + n / fps`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameGrid {
    pub origin: MediaTime,
    pub fps: f64,
}

impl FrameGrid {
    /// A grid from the manifest's `screen.fps` (30 when it is missing or odd) and `screen.start`.
    pub fn new(origin: MediaTime, fps: Option<f64>) -> FrameGrid {
        let fps = fps
            .filter(|f| f.is_finite() && *f >= 1.0 && *f <= 240.0)
            .unwrap_or(30.0);
        FrameGrid { origin, fps }
    }

    fn ticks_per_frame(&self) -> f64 {
        TICKS_PER_SECOND as f64 / self.fps
    }

    /// The frame boundary nearest `t`.
    pub fn round(&self, t: MediaTime) -> MediaTime {
        let n = ((t.ticks() - self.origin.ticks()) as f64 / self.ticks_per_frame()).round();
        MediaTime::from_ticks(self.origin.ticks() + (n * self.ticks_per_frame()).round() as i64)
    }

    /// `t` moved by `frames` whole frames (landing on a boundary).
    pub fn step(&self, t: MediaTime, frames: i64) -> MediaTime {
        let at = self.round(t);
        let n = ((at.ticks() - self.origin.ticks()) as f64 / self.ticks_per_frame()).round() as i64;
        MediaTime::from_ticks(
            self.origin.ticks() + ((n + frames) as f64 * self.ticks_per_frame()).round() as i64,
        )
    }

    /// `t` moved by `secs` whole seconds, then put on a frame boundary.
    pub fn step_secs(&self, t: MediaTime, secs: i64) -> MediaTime {
        self.round(MediaTime::from_ticks(t.ticks() + secs * TICKS_PER_SECOND))
    }
}

/// The times a drag can snap to: the playhead, the trim points, the splits, the cut edges and the
/// ends of the recording. `skip` leaves out what is being dragged.
pub fn candidates(list: &EditList, playhead: MediaTime, end: MediaTime) -> Vec<MediaTime> {
    let mut c = vec![MediaTime::ZERO, end, playhead, list.start, list.end];
    c.extend(list.splits.iter().copied());
    for s in &list.cuts {
        c.extend([s.from, s.to]);
    }
    c.sort();
    c.dedup();
    c
}

/// Snaps `t` to the nearest candidate within `threshold` (`ticks_per_px * SNAP_PX`), else to the
/// frame grid. With snapping off only the frame grid applies (edits still land on frames).
pub fn snap(
    t: MediaTime,
    points: &[MediaTime],
    threshold: MediaTime,
    snapping: bool,
    grid: &FrameGrid,
) -> MediaTime {
    if snapping {
        let near = points
            .iter()
            .copied()
            .filter(|p| (*p - t).abs() <= threshold)
            .min_by_key(|p| (*p - t).abs());
        if let Some(p) = near {
            return p;
        }
    }
    grid.round(t)
}

/// A range ordered `from <= to`, snapped at both ends.
pub fn snap_range(
    a: MediaTime,
    b: MediaTime,
    points: &[MediaTime],
    threshold: MediaTime,
    snapping: bool,
    grid: &FrameGrid,
) -> Span {
    let (a, b) = (
        snap(a, points, threshold, snapping, grid),
        snap(b, points, threshold, snapping, grid),
    );
    Span::new(a.min(b), a.max(b))
}

/// The threshold as time for a zoom level.
pub fn threshold(ticks_per_px: f64) -> MediaTime {
    MediaTime::from_ticks((ticks_per_px * f64::from(SNAP_PX)).round() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cm_core::EditContext;

    fn secs(s: f64) -> MediaTime {
        MediaTime::from_secs_f64(s).unwrap()
    }

    fn list() -> EditList {
        EditList::full(&EditContext {
            end: secs(60.0),
            live_at: None,
            gaps: &[],
        })
        .split(secs(20.0))
        .unwrap()
    }

    /// EDT-1 precision: keyboard steps move one frame, Shift one second, and land on frames.
    #[test]
    fn edt1_arrow_keys_step_one_frame_or_one_second() {
        let g = FrameGrid::new(MediaTime::ZERO, Some(30.0));
        let t = secs(1.0);
        let next = g.step(t, 1);
        assert_eq!((next - t).ticks(), TICKS_PER_SECOND / 30);
        assert_eq!(g.step(next, -1), t);
        assert_eq!(g.step_secs(t, 1), secs(2.0));
        assert_eq!(g.step_secs(t, -1), MediaTime::ZERO);
    }

    /// EDT-1: edits land on exact frames, counted from the screen track's first sample.
    #[test]
    fn edt1_ac1_frames_are_counted_from_the_track_start() {
        let origin = MediaTime::from_ticks(36_000); // 300 ms
        let g = FrameGrid::new(origin, Some(60.0));
        let r = g.round(secs(2.0));
        assert_eq!((r - origin).ticks() % (TICKS_PER_SECOND / 60), 0);
        assert!((r - secs(2.0)).abs().ticks() <= TICKS_PER_SECOND / 120 + 1);
    }

    #[test]
    fn missing_or_odd_fps_defaults_to_30() {
        assert_eq!(FrameGrid::new(MediaTime::ZERO, None).fps, 30.0);
        assert_eq!(FrameGrid::new(MediaTime::ZERO, Some(0.0)).fps, 30.0);
        assert_eq!(FrameGrid::new(MediaTime::ZERO, Some(f64::NAN)).fps, 30.0);
    }

    /// Snapping (N on) pulls a drag onto the split, the playhead and the ends.
    #[test]
    fn snapping_pulls_to_features_within_the_threshold() {
        let l = list();
        let g = FrameGrid::new(MediaTime::ZERO, Some(30.0));
        let pts = candidates(&l, secs(40.0), secs(60.0));
        let th = threshold(MediaTime::from_millis(10).ticks() as f64);
        assert_eq!(snap(secs(20.03), &pts, th, true, &g), secs(20.0));
        assert_eq!(snap(secs(40.02), &pts, th, true, &g), secs(40.0));
        assert_eq!(snap(secs(0.04), &pts, th, true, &g), MediaTime::ZERO);
        // Far from everything: the frame grid.
        let far = snap(secs(30.01), &pts, th, true, &g);
        assert_eq!(far, g.round(secs(30.01)));
    }

    /// With snapping off the feature pull is gone but the frame grid stays.
    #[test]
    fn snapping_off_still_lands_on_frames() {
        let l = list();
        let g = FrameGrid::new(MediaTime::ZERO, Some(30.0));
        let pts = candidates(&l, secs(40.0), secs(60.0));
        let th = threshold(100_000.0);
        let t = secs(20.01);
        assert_eq!(snap(t, &pts, th, false, &g), g.round(t));
        // 20.01 s is 10 ms from the split: with snapping on it would be pulled to exactly 20 s.
        assert_eq!(snap(t, &pts, th, true, &g), secs(20.0));
    }

    #[test]
    fn ranges_come_out_ordered() {
        let g = FrameGrid::new(MediaTime::ZERO, Some(30.0));
        let r = snap_range(secs(15.0), secs(10.0), &[], MediaTime::ZERO, false, &g);
        assert_eq!(r, Span::new(secs(10.0), secs(15.0)));
    }
}
