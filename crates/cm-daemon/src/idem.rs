//! The bounded `request_id` result cache behind AGT-3.AC4 (R2-6).
//!
//! `cm_core::IdempotencyCache` keeps every result for 24 h and is never pruned on insert, and a
//! `capture_screenshot` result embeds a base64 PNG, so a client sending fresh `request_id`s grew
//! the daemon without bound. This cache holds at most [`MAX_ENTRIES`] results and
//! [`MAX_TOTAL_BYTES`] of serialized JSON, evicting the least recently *inserted* first, and
//! stores a compact copy of big results: an image payload is replaced by
//! `{"mime_type", "omitted": true}` so a replay still returns ids, revisions and paths (the
//! image is one `get_capture` away).

use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use time::{Duration, OffsetDateTime};

pub(crate) const MAX_ENTRIES: usize = 512;
pub(crate) const MAX_TOTAL_BYTES: usize = 64 * 1024 * 1024;
/// Results above this serialized size are stored without their image payloads.
const COMPACT_ABOVE: usize = 64 * 1024;
const TTL: Duration = Duration::hours(24);

struct Entry {
    at: OffsetDateTime,
    bytes: usize,
    value: (String, Value),
}

pub(crate) struct ResultCache {
    entries: HashMap<String, Entry>,
    order: VecDeque<String>,
    total: usize,
    max_entries: usize,
    max_bytes: usize,
}

impl Default for ResultCache {
    fn default() -> Self {
        Self::with_limits(MAX_ENTRIES, MAX_TOTAL_BYTES)
    }
}

/// Drops image payloads and inline frame data from a (cloned) result.
fn compact(mut v: Value) -> Value {
    if let Some(obj) = v.as_object_mut() {
        if let Some(img) = obj.get_mut("image") {
            if img.get("data").is_some() {
                let mime = img.get("mime_type").cloned().unwrap_or(Value::Null);
                *img = json!({ "mime_type": mime, "omitted": true });
            }
        }
        if obj.get("transport").and_then(Value::as_str) == Some("inline") {
            if let Some(d) = obj.get_mut("data") {
                *d = Value::Null;
                obj.insert("data_omitted".into(), Value::Bool(true));
            }
        }
    }
    v
}

impl ResultCache {
    pub(crate) fn with_limits(max_entries: usize, max_bytes: usize) -> Self {
        ResultCache {
            entries: HashMap::new(),
            order: VecDeque::new(),
            total: 0,
            max_entries,
            max_bytes,
        }
    }

    pub(crate) fn get(&self, key: &str, now: OffsetDateTime) -> Option<(String, Value)> {
        self.entries
            .get(key)
            .filter(|e| now - e.at < TTL)
            .map(|e| e.value.clone())
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    #[cfg(test)]
    pub(crate) fn total_bytes(&self) -> usize {
        self.total
    }

    pub(crate) fn record(&mut self, key: &str, now: OffsetDateTime, value: (String, Value)) {
        if self.get(key, now).is_some() {
            return;
        }
        self.remove(key);
        let (method, result) = value;
        let mut bytes = serde_json::to_vec(&result).map_or(0, |v| v.len());
        let result = if bytes > COMPACT_ABOVE {
            let c = compact(result);
            bytes = serde_json::to_vec(&c).map_or(0, |v| v.len());
            c
        } else {
            result
        };
        // A single result larger than the whole budget is not remembered.
        if bytes > self.max_bytes {
            return;
        }
        // Expired entries first, then the oldest.
        let expired: Vec<String> = self
            .entries
            .iter()
            .filter(|(_, e)| now - e.at >= TTL)
            .map(|(k, _)| k.clone())
            .collect();
        for k in expired {
            self.remove(&k);
        }
        while self.entries.len() >= self.max_entries || self.total + bytes > self.max_bytes {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            if let Some(e) = self.entries.remove(&oldest) {
                self.total -= e.bytes;
            }
        }
        self.total += bytes;
        self.order.push_back(key.to_owned());
        self.entries.insert(
            key.to_owned(),
            Entry {
                at: now,
                bytes,
                value: (method, result),
            },
        );
    }

    fn remove(&mut self, key: &str) {
        if let Some(e) = self.entries.remove(key) {
            self.total -= e.bytes;
            self.order.retain(|k| k != key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: OffsetDateTime = time::macros::datetime!(2026-10-19 09:00 UTC);

    #[test]
    fn r2_6_entry_count_is_capped_and_oldest_goes_first() {
        let mut c = ResultCache::with_limits(3, usize::MAX);
        for i in 0..10 {
            c.record(&format!("k{i}"), T0, ("m".into(), json!({ "i": i })));
        }
        assert_eq!(c.len(), 3);
        assert!(c.get("k0", T0).is_none());
        assert!(c.get("k9", T0).is_some());
    }

    #[test]
    fn r2_6_total_bytes_are_capped() {
        let mut c = ResultCache::with_limits(1000, 10_000);
        for i in 0..50 {
            c.record(
                &format!("k{i}"),
                T0,
                ("m".into(), json!({ "s": "x".repeat(1000) })),
            );
        }
        assert!(c.total_bytes() <= 10_000, "{}", c.total_bytes());
        assert!(c.len() < 12);
    }

    #[test]
    fn r2_6_image_payloads_are_dropped_from_cached_results() {
        let mut c = ResultCache::default();
        let big = "A".repeat(1_000_000);
        c.record(
            "k",
            T0,
            (
                "capture_screenshot".into(),
                json!({ "capture_id": "shot_1", "image": { "mime_type": "image/png", "data": big } }),
            ),
        );
        assert!(c.total_bytes() < 1000);
        let (_, v) = c.get("k", T0).unwrap();
        assert_eq!(v["capture_id"], "shot_1");
        assert_eq!(v["image"]["omitted"], true);
        assert!(v["image"].get("data").is_none());
    }

    #[test]
    fn first_result_wins_and_expires_after_24h() {
        let mut c = ResultCache::default();
        c.record("k", T0, ("m".into(), json!(1)));
        c.record("k", T0, ("m".into(), json!(2)));
        assert_eq!(c.get("k", T0).unwrap().1, json!(1));
        assert!(c.get("k", T0 + Duration::hours(25)).is_none());
    }
}
