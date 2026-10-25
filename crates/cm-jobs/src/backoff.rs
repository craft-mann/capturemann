//! Exponential backoff with jitter (STO-3: 1 s -> 5 min, max 20 attempts).

use std::time::Duration;

/// Backoff schedule. `delay(n)` is `base * 2^(n-1)` capped at `max`, then reduced by up to
/// `jitter_ratio` of itself (so the cap is never exceeded and delays never collapse to zero
/// as long as `jitter_ratio < 1`).
#[derive(Debug, Clone, PartialEq)]
pub struct BackoffPolicy {
    pub base: Duration,
    pub max: Duration,
    /// Fraction in `[0, 1]` of the delay that may be shaved off by jitter.
    pub jitter_ratio: f64,
}

impl Default for BackoffPolicy {
    fn default() -> Self {
        BackoffPolicy {
            base: Duration::from_secs(1),
            max: Duration::from_secs(300),
            jitter_ratio: 0.25,
        }
    }
}

impl BackoffPolicy {
    /// Delay before retrying after the `failed_attempts`-th consecutive failure (1-based).
    /// `jitter_sample` is in `[0, 1)`.
    pub fn delay(&self, failed_attempts: u32, jitter_sample: f64) -> Duration {
        let exp = failed_attempts.saturating_sub(1).min(32);
        let raw = self.base.as_secs_f64() * 2f64.powi(i32::try_from(exp).unwrap_or(32));
        let capped = raw.min(self.max.as_secs_f64());
        let ratio = self.jitter_ratio.clamp(0.0, 1.0);
        let sample = if jitter_sample.is_finite() {
            jitter_sample.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let secs = (capped * (1.0 - ratio * sample)).max(0.0);
        Duration::from_secs_f64(secs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doubles_then_caps_at_five_minutes() {
        // STO-3: 1 s -> 5 min.
        let p = BackoffPolicy::default();
        let d: Vec<u64> = (1..=11).map(|n| p.delay(n, 0.0).as_secs()).collect();
        assert_eq!(d, [1, 2, 4, 8, 16, 32, 64, 128, 256, 300, 300]);
        assert_eq!(p.delay(u32::MAX, 0.0), Duration::from_secs(300));
    }

    #[test]
    fn jitter_only_shortens_within_ratio() {
        let p = BackoffPolicy::default();
        let full = p.delay(3, 0.0);
        let low = p.delay(3, 0.999_999);
        assert_eq!(full, Duration::from_secs(4));
        assert!(low < full);
        assert!(low >= Duration::from_secs_f64(4.0 * 0.75 - 0.001));
        assert_eq!(p.delay(3, f64::NAN), full);
    }
}
