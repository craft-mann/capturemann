//! Spike S3, headless part: the rolling buffer and the latency measuring harness.
//!
//! GDE-1 takes the frame of a step on **mouse-down**. If a system delivers the click late, or
//! the frame source answers slowly, the frame would show the state after the click. The fallback
//! the spec names is a rolling buffer of the target: a feeder keeps the frames of the last
//! [`WINDOW_NS`] (100 ms), and a step takes the newest frame that is **not newer than the click**.
//!
//! [`FrameRing`] is that buffer and is pure (any frame type, caller-supplied times), so it is
//! tested here without a screen. [`LatencyStats`] is the harness's arithmetic: the example
//! `guide_latency` measures `Capturer::capture` on the real screen on a person's request and
//! prints these numbers. S3 passes on a system when 95 % of the frames show the state before
//! the click, which with the direct path means `p95(latency) < click-to-effect time of the test
//! app`; a system where it does not turns the ring on.

use std::collections::VecDeque;

/// How far back the ring reaches, host nanoseconds (100 ms).
pub const WINDOW_NS: u64 = 100_000_000;

/// Frames of the last [`WINDOW_NS`], oldest first.
#[derive(Debug)]
pub struct FrameRing<T> {
    items: VecDeque<(u64, T)>,
}

impl<T> Default for FrameRing<T> {
    fn default() -> Self {
        FrameRing {
            items: VecDeque::new(),
        }
    }
}

impl<T> FrameRing<T> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds the frame taken at `taken_ns` and drops frames older than the window (the newest
    /// frame is always kept, even if the feeder stalls).
    pub fn push(&mut self, taken_ns: u64, frame: T) {
        self.items.push_back((taken_ns, frame));
        while self.items.len() > 1
            && self
                .items
                .front()
                .is_some_and(|(t, _)| taken_ns.saturating_sub(*t) > WINDOW_NS)
        {
            self.items.pop_front();
        }
    }

    /// The newest frame taken at or before `click_ns` minus `margin_ns` (the time the OS needs
    /// to deliver the event), or the oldest frame when none is old enough.
    pub fn before(&self, click_ns: u64, margin_ns: u64) -> Option<&T> {
        let limit = click_ns.saturating_sub(margin_ns);
        self.items
            .iter()
            .rev()
            .find(|(t, _)| *t <= limit)
            .or_else(|| self.items.front())
            .map(|(_, f)| f)
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

/// Percentiles of a list of latencies.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LatencyStats {
    pub n: usize,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub max_ms: f64,
}

impl LatencyStats {
    /// `None` for an empty list.
    pub fn from_ms(samples: &[f64]) -> Option<Self> {
        if samples.is_empty() {
            return None;
        }
        let mut v = samples.to_vec();
        v.sort_by(f64::total_cmp);
        let pick = |q: f64| v[(((v.len() - 1) as f64) * q).ceil() as usize];
        Some(LatencyStats {
            n: v.len(),
            p50_ms: pick(0.5),
            p95_ms: pick(0.95),
            max_ms: v[v.len() - 1],
        })
    }

    /// Whether 95 % of the frames were taken within `budget_ms` of the click (the S3 pass rule
    /// with `budget_ms` = the test app's click-to-change time).
    pub fn passes(&self, budget_ms: f64) -> bool {
        self.p95_ms <= budget_ms
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_the_frame_before_the_click() {
        let mut r = FrameRing::new();
        for i in 0..10u64 {
            r.push(i * 16_000_000, i);
        }
        // Click at 100 ms, event delivery margin 20 ms: the frame at 80 ms or earlier.
        assert_eq!(r.before(100_000_000, 20_000_000), Some(&5));
        // Frames older than the window were dropped.
        assert!(r.len() <= 8);
    }

    #[test]
    fn a_stalled_feeder_still_has_a_frame() {
        let mut r = FrameRing::new();
        r.push(0, "a");
        r.push(500_000_000, "b");
        assert_eq!(r.len(), 2.min(r.len()));
        assert!(r.before(1_000_000_000, 0).is_some());
        assert_eq!(FrameRing::<u8>::new().before(1, 0), None);
    }

    #[test]
    fn percentiles() {
        let s = LatencyStats::from_ms(&(1..=100).map(f64::from).collect::<Vec<_>>()).unwrap();
        assert_eq!((s.p50_ms, s.p95_ms, s.max_ms), (51.0, 96.0, 100.0));
        assert!(s.passes(100.0) && !s.passes(50.0));
        assert!(LatencyStats::from_ms(&[]).is_none());
    }
}
