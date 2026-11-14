//! Spike S3 harness (Click Guide): how long a frame of the followed target takes, which is the
//! delay between a mouse-down and the pixels a step shows.
//!
//! ```text
//! cargo run -p cm-daemon --example guide_latency -- --display 1 --samples 60 --budget-ms 40
//! ```
//!
//! It takes `--samples` frames of the display with the real capture backend (this reads the
//! screen: run it only when you mean to, on each system, with the test app in front) and prints
//! the percentiles. S3 passes on a system when 95 % of the frames are taken within the budget,
//! the click-to-change time of the test app (pass `--budget-ms`). When it does not, the
//! rolling buffer (`cm_daemon::guide::ring::FrameRing`) takes the frame of the last 100 ms
//! instead. The example makes no click and writes nothing.

use std::time::Instant;

use cm_capture::{CaptureRequest, CaptureTarget, Capturer};
use cm_daemon::guide::ring::LatencyStats;

fn arg(name: &str) -> Option<String> {
    let a: Vec<String> = std::env::args().collect();
    a.iter()
        .position(|x| x == name)
        .and_then(|i| a.get(i + 1))
        .cloned()
}

fn main() {
    let samples: usize = arg("--samples").and_then(|s| s.parse().ok()).unwrap_or(60);
    let budget: f64 = arg("--budget-ms")
        .and_then(|s| s.parse().ok())
        .unwrap_or(50.0);
    let capturer = Capturer::system();
    let display = arg("--display")
        .and_then(|s| s.parse().ok())
        .or_else(|| {
            capturer
                .list_targets_with(false)
                .ok()
                .and_then(|t| t.displays.iter().find(|d| d.is_primary).map(|d| d.id))
        })
        .unwrap_or(1);
    let mut ms = Vec::with_capacity(samples);
    for _ in 0..samples {
        let t = Instant::now();
        match capturer.capture(
            CaptureRequest::new(CaptureTarget::Display {
                display_id: display,
            }),
            None,
        ) {
            Ok(Some(_)) => ms.push(t.elapsed().as_secs_f64() * 1000.0),
            Ok(None) => {}
            Err(e) => {
                eprintln!("capture failed: {e}");
                std::process::exit(1);
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
    let Some(s) = LatencyStats::from_ms(&ms) else {
        eprintln!("no frames");
        std::process::exit(1);
    };
    println!(
        "frames={} p50={:.1} ms p95={:.1} ms max={:.1} ms budget={budget} ms -> S3 {}",
        s.n,
        s.p50_ms,
        s.p95_ms,
        s.max_ms,
        if s.passes(budget) {
            "PASS (direct frame on mouse-down)"
        } else {
            "FAIL (use the rolling buffer)"
        }
    );
}
