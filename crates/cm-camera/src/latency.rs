//! Delivery-latency statistics: how long after its capture stamp a frame reached us.

use serde::{Deserialize, Serialize};

/// Median/percentile summary in milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LatencySummary {
    pub count: usize,
    pub min_ms: f64,
    pub median_ms: f64,
    pub p95_ms: f64,
    pub max_ms: f64,
}

/// Collects `delivered - captured` samples (capped) and the lateness baseline.
#[derive(Debug, Default)]
pub struct LatencyStats {
    samples: Vec<f32>,
}

const MAX_SAMPLES: usize = 400_000;
/// Frames before "late" is judged, and the warm-up the settled baseline skips (the first frames
/// of a capture session arrive late while the camera starts up).
pub const BASELINE_FRAMES: usize = 30;
/// The settled baseline is the median of frames [`BASELINE_FRAMES`] to this one (R3-24).
pub const BASELINE_END_FRAMES: usize = 120;

impl LatencyStats {
    pub fn push(&mut self, delivered_host_ns: u64, capture_host_ns: u64) {
        if self.samples.len() < MAX_SAMPLES {
            let ms = (delivered_host_ns as i128 - capture_host_ns as i128) as f64 / 1e6;
            self.samples.push(ms as f32);
        }
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// The steady-state delivery lag. Provisionally the median of the first
    /// [`BASELINE_FRAMES`] samples (so lateness can be judged early); from
    /// [`BASELINE_END_FRAMES`] frames on, the median of frames 30 to 120, which leaves out the
    /// warm-up (R3-24: the first frames used to be the baseline, warm-up included).
    pub fn baseline_ms(&self) -> Option<f64> {
        let n = self.samples.len();
        if n >= BASELINE_END_FRAMES {
            Some(percentile(
                &self.samples[BASELINE_FRAMES..BASELINE_END_FRAMES],
                0.5,
            ))
        } else if n >= BASELINE_FRAMES {
            Some(percentile(&self.samples[..BASELINE_FRAMES], 0.5))
        } else {
            None
        }
    }

    pub fn summary(&self) -> Option<LatencySummary> {
        summarize(&self.samples)
    }
}

fn percentile(samples: &[f32], q: f64) -> f64 {
    let mut v = samples.to_vec();
    v.sort_by(|a, b| a.total_cmp(b));
    let idx = ((v.len() - 1) as f64 * q).round() as usize;
    f64::from(v[idx])
}

/// Summary of raw millisecond samples.
pub fn summarize(samples: &[f32]) -> Option<LatencySummary> {
    if samples.is_empty() {
        return None;
    }
    Some(LatencySummary {
        count: samples.len(),
        min_ms: percentile(samples, 0.0),
        median_ms: percentile(samples, 0.5),
        p95_ms: percentile(samples, 0.95),
        max_ms: percentile(samples, 1.0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_reports_median_and_p95() {
        let mut s = LatencyStats::default();
        for i in 0..100u64 {
            s.push(1_000_000_000 + i * 1_000_000, 1_000_000_000);
        }
        let sum = s.summary().unwrap();
        assert_eq!(sum.count, 100);
        assert_eq!(sum.min_ms, 0.0);
        assert!((sum.median_ms - 50.0).abs() <= 1.0);
        assert!((sum.p95_ms - 94.0).abs() <= 1.5);
        assert_eq!(sum.max_ms, 99.0);
        assert_eq!(s.baseline_ms().map(|b| b.round()), Some(15.0));
    }

    /// R3-24: warm-up frames (slow while the camera starts) do not set the baseline.
    #[test]
    fn the_settled_baseline_skips_the_warm_up() {
        let mut s = LatencyStats::default();
        for i in 0..150u64 {
            let lag_ms = if i < 30 { 200 } else { 12 };
            s.push(1_000_000_000 + lag_ms * 1_000_000, 1_000_000_000);
            if i == 40 {
                // Provisional: still the warm-up median.
                assert_eq!(s.baseline_ms().map(|b| b.round()), Some(200.0));
            }
        }
        assert_eq!(s.baseline_ms().map(|b| b.round()), Some(12.0));
    }

    #[test]
    fn empty_has_no_summary() {
        assert!(LatencyStats::default().summary().is_none());
        assert!(LatencyStats::default().baseline_ms().is_none());
    }
}
