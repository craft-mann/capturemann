//! Phase timestamps for the hotkey -> overlay path (NFR-1), behind `CM_TRACE=1`.
//!
//! Each mark prints one JSON line on stderr with the milliseconds since the first mark
//! (`ms`, monotonic) and the wall clock in microseconds (`wall_us`, comparable with the
//! overlay's own `CM_OVERLAY_TRACE` lines, which carry the same field). Without the variable a
//! mark costs one relaxed atomic load.

use std::sync::OnceLock;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

static START: OnceLock<Option<Instant>> = OnceLock::new();

fn start() -> Option<Instant> {
    *START.get_or_init(|| std::env::var_os("CM_TRACE").map(|_| Instant::now()))
}

/// Call once at startup so `ms` counts from the process start.
pub fn init() {
    let _ = start();
}

pub fn enabled() -> bool {
    start().is_some()
}

pub fn mark(name: &str) {
    let Some(t0) = start() else {
        return;
    };
    let wall_us = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_micros());
    eprintln!(
        "{{\"cm_desktop_trace\":\"{name}\",\"ms\":{:.3},\"wall_us\":{wall_us}}}",
        t0.elapsed().as_secs_f64() * 1e3
    );
}
