//! Injectable wall clock (revision timestamps, idempotency expiry, activity-log retention).

use std::sync::{Mutex, PoisonError};
use time::{Duration, OffsetDateTime};

pub trait Clock: Send + Sync {
    fn now(&self) -> OffsetDateTime;
}

/// The real clock (UTC).
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> OffsetDateTime {
        OffsetDateTime::now_utc()
    }
}

/// A clock that only moves when told to (tests).
#[derive(Debug)]
pub struct ManualClock(Mutex<OffsetDateTime>);

impl ManualClock {
    pub fn new(start: OffsetDateTime) -> Self {
        ManualClock(Mutex::new(start))
    }

    pub fn advance(&self, by: Duration) {
        let mut g = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        *g += by;
    }
}

impl Clock for ManualClock {
    fn now(&self) -> OffsetDateTime {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
