//! Presentation statistics for the preview test: what reached the screen, and how evenly.

use std::time::{Duration, Instant};

/// Records the instant each *new* frame was handed to the window, and how often the window
/// redrew without one. Pure bookkeeping, so it is unit tested without a display.
#[derive(Debug)]
pub struct PresentStats {
    period: Duration,
    started: Option<Instant>,
    last: Option<Instant>,
    presented: u64,
    repeats: u64,
    late: u64,
    jittered: u64,
    worst_gap: Duration,
    last_index: Option<u32>,
    skipped: u64,
}

/// A finished run.
#[derive(Clone, Debug, PartialEq)]
pub struct PresentSummary {
    pub wall: Duration,
    /// New frames shown per second.
    pub fps: f64,
    pub presented: u64,
    /// Redraws that had no new frame (display refresh faster than the source, or a late frame).
    pub repeats: u64,
    /// New frames whose gap to the previous one exceeded 2 frame periods: a whole source frame
    /// slot went by without a new image (a visible hitch).
    pub late: u64,
    /// Gaps over 1.5 periods. On a 120 Hz display a 60 fps frame can land one refresh late
    /// (25 ms instead of 16.7 ms) without a visible hitch, so this is jitter, not a miss.
    pub jittered: u64,
    /// Frame indices the source produced that were never shown (overwritten in the slot).
    pub skipped: u64,
    pub worst_gap_ms: f64,
}

impl PresentStats {
    /// `fps` is the source rate (60).
    pub fn new(fps: u32) -> PresentStats {
        PresentStats {
            period: Duration::from_secs_f64(1.0 / f64::from(fps)),
            started: None,
            last: None,
            presented: 0,
            repeats: 0,
            late: 0,
            jittered: 0,
            worst_gap: Duration::ZERO,
            last_index: None,
            skipped: 0,
        }
    }

    /// A redraw showed frame `index` for the first time at `now`.
    pub fn new_frame(&mut self, index: u32, now: Instant) {
        self.started.get_or_insert(now);
        if let Some(prev) = self.last {
            let gap = now.saturating_duration_since(prev);
            self.worst_gap = self.worst_gap.max(gap);
            if gap > self.period.mul_f64(2.0) {
                self.late += 1;
            }
            if gap > self.period.mul_f64(1.5) {
                self.jittered += 1;
            }
        }
        if let Some(prev) = self.last_index {
            self.skipped += u64::from(index.wrapping_sub(prev).saturating_sub(1));
        }
        self.last = Some(now);
        self.last_index = Some(index);
        self.presented += 1;
    }

    /// A redraw happened with no new frame.
    pub fn repeat(&mut self) {
        if self.started.is_some() {
            self.repeats += 1;
        }
    }

    pub fn summary(&self, now: Instant) -> PresentSummary {
        let wall = self
            .started
            .map_or(Duration::ZERO, |s| now.saturating_duration_since(s));
        let secs = wall.as_secs_f64();
        PresentSummary {
            wall,
            fps: if secs > 0.0 {
                self.presented as f64 / secs
            } else {
                0.0
            },
            presented: self.presented,
            repeats: self.repeats,
            late: self.late,
            jittered: self.jittered,
            skipped: self.skipped,
            worst_gap_ms: self.worst_gap.as_secs_f64() * 1000.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(t0: Instant, ms: u64) -> Instant {
        t0 + Duration::from_millis(ms)
    }

    /// A steady 60 fps run: no late frames, no skips.
    #[test]
    fn steady_sixty_has_no_late_or_skipped() {
        let t0 = Instant::now();
        let mut s = PresentStats::new(60);
        for i in 0..60u32 {
            s.new_frame(i, t0 + Duration::from_secs_f64(f64::from(i) / 60.0));
        }
        let sum = s.summary(t0 + Duration::from_secs(1));
        assert_eq!((sum.presented, sum.late, sum.skipped), (60, 0, 0));
        assert!((sum.fps - 60.0).abs() < 1.0, "{}", sum.fps);
    }

    /// One 50 ms stall counts as one late (and jittered) frame and the skipped indices are counted.
    #[test]
    fn a_stall_is_late_and_skips_are_counted() {
        let t0 = Instant::now();
        let mut s = PresentStats::new(60);
        s.new_frame(0, at(t0, 0));
        s.new_frame(1, at(t0, 17));
        s.new_frame(4, at(t0, 67)); // frames 2 and 3 never shown
        s.new_frame(5, at(t0, 84));
        let sum = s.summary(at(t0, 100));
        assert_eq!(
            (sum.late, sum.jittered, sum.skipped, sum.presented),
            (1, 1, 2, 4)
        );
        assert!((sum.worst_gap_ms - 50.0).abs() < 0.5);
    }

    /// A 25 ms gap (one 120 Hz refresh late) is jitter, not a late frame.
    #[test]
    fn one_refresh_late_is_jitter_only() {
        let t0 = Instant::now();
        let mut s = PresentStats::new(60);
        s.new_frame(0, at(t0, 0));
        s.new_frame(1, at(t0, 26));
        let sum = s.summary(at(t0, 30));
        assert_eq!((sum.late, sum.jittered), (0, 1));
    }

    #[test]
    fn repeats_count_only_after_the_first_frame() {
        let t0 = Instant::now();
        let mut s = PresentStats::new(60);
        s.repeat();
        s.new_frame(0, t0);
        s.repeat();
        assert_eq!(s.summary(t0).repeats, 1);
    }
}
