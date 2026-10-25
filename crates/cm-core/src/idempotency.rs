//! `request_id` idempotency (AGT-3: repeats within 24 h return the first result).

use std::collections::HashMap;
use time::{Duration, OffsetDateTime};

/// Remembers the first result per `request_id` for a TTL (default 24 h).
#[derive(Debug, Clone)]
pub struct IdempotencyCache<T> {
    ttl: Duration,
    entries: HashMap<String, (OffsetDateTime, T)>,
}

impl<T: Clone> Default for IdempotencyCache<T> {
    fn default() -> Self {
        Self::new(Duration::hours(24))
    }
}

impl<T: Clone> IdempotencyCache<T> {
    pub fn new(ttl: Duration) -> Self {
        IdempotencyCache {
            ttl,
            entries: HashMap::new(),
        }
    }

    /// The first result recorded for `request_id`, if still within the TTL.
    pub fn get(&self, request_id: &str, now: OffsetDateTime) -> Option<T> {
        self.entries
            .get(request_id)
            .filter(|(at, _)| now - *at < self.ttl)
            .map(|(_, v)| v.clone())
    }

    /// Records `value` unless a live entry exists; returns the winning (first) value.
    pub fn record(&mut self, request_id: &str, now: OffsetDateTime, value: T) -> T {
        if let Some(existing) = self.get(request_id, now) {
            return existing;
        }
        self.entries
            .insert(request_id.to_owned(), (now, value.clone()));
        value
    }

    /// Drops expired entries.
    pub fn prune(&mut self, now: OffsetDateTime) {
        let ttl = self.ttl;
        self.entries.retain(|_, (at, _)| now - *at < ttl);
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    #[test]
    fn agt_3_ac4_repeat_returns_first_result() {
        let t0 = datetime!(2026-10-19 09:00 UTC);
        let mut c = IdempotencyCache::default();
        assert_eq!(c.get("r1", t0), None);
        assert_eq!(c.record("r1", t0, 1), 1);
        assert_eq!(c.record("r1", t0 + Duration::hours(1), 2), 1);
        assert_eq!(c.get("r1", t0 + Duration::hours(23)), Some(1));
    }

    #[test]
    fn expires_after_24h() {
        let t0 = datetime!(2026-10-19 09:00 UTC);
        let mut c = IdempotencyCache::default();
        c.record("r1", t0, 1);
        let later = t0 + Duration::hours(24);
        assert_eq!(c.get("r1", later), None);
        assert_eq!(c.record("r1", later, 2), 2);
        c.prune(later + Duration::hours(25));
        assert!(c.is_empty());
    }
}
