//! Phase timing for the hotkey -> first-frame budget (NFR-1: 150 ms).
//!
//! Every phase is recorded in memory (a few microseconds each). With `CM_OVERLAY_TRACE` set the
//! events are also printed as JSON lines on stderr as they happen, and
//! `--exit-after-first-frame` returns them as the result so a benchmark can read one run's
//! phases from stdout.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

#[derive(Clone)]
pub struct Tracer {
    inner: Arc<Inner>,
}

struct Inner {
    started: Instant,
    print: bool,
    events: Mutex<Vec<(String, f64)>>,
}

impl Tracer {
    /// `started` is the process start as the binary saw it (first line of `main`).
    pub fn new(started: Instant, print: bool) -> Tracer {
        Tracer {
            inner: Arc::new(Inner {
                started,
                print,
                events: Mutex::new(Vec::new()),
            }),
        }
    }

    pub fn elapsed_ms(&self) -> f64 {
        self.inner.started.elapsed().as_secs_f64() * 1e3
    }

    /// Records `name` now; `fields` (a JSON object or null) is printed with it when tracing.
    pub fn mark(&self, name: &str, fields: Value) {
        let ms = self.elapsed_ms();
        self.inner
            .events
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((name.to_owned(), ms));
        if self.inner.print {
            let wall_us = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_micros());
            // `wall_us` lines up with the desktop app's `CM_TRACE` marks (same clock).
            let mut line =
                json!({ "trace": name, "ms_since_start": ms, "wall_us": wall_us as u64 });
            if let (Some(dst), Some(src)) = (line.as_object_mut(), fields.as_object()) {
                dst.extend(src.iter().map(|(k, v)| (k.clone(), v.clone())));
            }
            eprintln!("{line}");
        }
    }

    /// Prints a free-form trace line (mapping details, ...) when tracing is on.
    pub fn note(&self, line: &Value) {
        if self.inner.print {
            eprintln!("{line}");
        }
    }

    /// `{ phase: ms_since_start }`; a repeated phase keeps its first time.
    pub fn summary(&self) -> Value {
        let events = self
            .inner
            .events
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let mut out = serde_json::Map::new();
        for (name, ms) in events.iter() {
            out.entry(name.clone())
                .or_insert_with(|| json!((ms * 100.0).round() / 100.0));
        }
        Value::Object(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_keeps_the_first_time_of_a_phase() {
        let t = Tracer::new(Instant::now(), false);
        t.mark("a", Value::Null);
        let first = t.summary()["a"].as_f64().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        t.mark("a", Value::Null);
        t.mark("b", Value::Null);
        let s = t.summary();
        assert_eq!(s["a"].as_f64().unwrap(), first);
        assert!(s["b"].as_f64().unwrap() > first);
    }
}
