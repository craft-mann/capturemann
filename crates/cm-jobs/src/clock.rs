//! Injectable time and jitter sources so retry/backoff tests are deterministic.

use std::collections::hash_map::RandomState;
use std::fmt::Debug;
use std::hash::{BuildHasher, Hasher};
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Wall-clock source in milliseconds since the Unix epoch.
pub trait Clock: Send + Sync + Debug {
    fn now_ms(&self) -> i64;
}

/// The real clock.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
            .unwrap_or(0)
    }
}

/// A clock that only moves when told to (tests, and downstream crates' tests).
#[derive(Debug, Default)]
pub struct ManualClock(AtomicI64);

impl ManualClock {
    pub fn new(start_ms: i64) -> Self {
        ManualClock(AtomicI64::new(start_ms))
    }

    pub fn advance_ms(&self, ms: i64) {
        self.0.fetch_add(ms, Ordering::SeqCst);
    }

    pub fn set_ms(&self, ms: i64) {
        self.0.store(ms, Ordering::SeqCst);
    }
}

impl Clock for ManualClock {
    fn now_ms(&self) -> i64 {
        self.0.load(Ordering::SeqCst)
    }
}

/// Source of jitter samples in `[0, 1)`.
pub trait Jitter: Send + Sync + Debug {
    fn sample(&self) -> f64;
}

/// Pseudo-random jitter (std `RandomState` seeding; no extra dependency).
#[derive(Debug, Default)]
pub struct RandomJitter(AtomicU64);

impl Jitter for RandomJitter {
    fn sample(&self) -> f64 {
        let n = self.0.fetch_add(1, Ordering::Relaxed);
        let mut h = RandomState::new().build_hasher();
        h.write_u64(n);
        // 53 random bits -> [0, 1)
        (h.finish() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// Always returns the same sample (tests).
#[derive(Debug, Clone, Copy)]
pub struct FixedJitter(pub f64);

impl Jitter for FixedJitter {
    fn sample(&self) -> f64 {
        self.0
    }
}
