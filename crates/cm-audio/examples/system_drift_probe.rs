//! System-audio timing probe (Linux, PipeWire): records the monitor of one sink for N seconds on
//! a session clock and reports the track's resyncs, offsets and drift next to where the beeps
//! that were played into that sink landed in `system.wav`.
//!
//! Needs a sink of your own so the session's default is left alone, and a beep file with one beep
//! every 2 s:
//!
//!   pw-cli create-node adapter '{ factory.name=support.null-audio-sink node.name=cm-probe-sink
//!       media.class=Audio/Sink object.linger=true audio.position=[FL FR] }'
//!   cargo run -p cm-audio --example system_drift_probe -- --target cm-probe-sink --secs 60 \
//!       --out /tmp/drift &
//!   pw-play --target cm-probe-sink beeps.wav
//!
//! It records system audio only (no microphone, no screen) and never prompts.

use std::path::PathBuf;
use std::time::Duration;

use cm_audio::{pipewire_monitor_factory, DeviceSelector, SystemAudioConfig, SystemAudioRecorder};
use cm_core::{SessionClock, TrackContext, TrackRecorder};

fn arg(name: &str) -> Option<String> {
    let a: Vec<String> = std::env::args().collect();
    a.iter()
        .position(|x| x == name)
        .and_then(|i| a.get(i + 1).cloned())
}

/// Beep onsets (seconds from the file start): the first 1 ms step above 6x the median envelope,
/// after at least 300 ms of quiet.
fn onsets(path: &PathBuf) -> Vec<f64> {
    let Ok(mut r) = hound::WavReader::open(path) else {
        return Vec::new();
    };
    let ch = usize::from(r.spec().channels);
    let rate = r.spec().sample_rate as f64;
    let s: Vec<f32> = r.samples::<f32>().map(|v| v.unwrap_or(0.0)).collect();
    let step = (rate / 1000.0) as usize * ch;
    let env: Vec<f32> = s
        .chunks(step)
        .map(|c| c.iter().fold(0.0f32, |m, v| m.max(v.abs())))
        .collect();
    let mut sorted = env.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let floor = sorted.get(sorted.len() / 2).copied().unwrap_or(0.0);
    let thr = (floor * 6.0).max(0.01);
    let (mut out, mut quiet_ms) = (Vec::new(), 1000);
    for (i, e) in env.iter().enumerate() {
        if *e > thr {
            if quiet_ms >= 300 {
                out.push(i as f64 / 1000.0);
            }
            quiet_ms = 0;
        } else {
            quiet_ms += 1;
        }
    }
    out
}

fn main() {
    let secs: u64 = arg("--secs").and_then(|s| s.parse().ok()).unwrap_or(60);
    let out = PathBuf::from(arg("--out").unwrap_or_else(|| "/tmp/drift".into()));
    let target = arg("--target");
    std::fs::create_dir_all(&out).expect("create out dir");
    let clock = SessionClock::start();
    let ctx = TrackContext {
        dir: out.clone(),
        clock: clock.clone(),
    };
    let factory: cm_audio::SourceFactory = match target {
        Some(t) => Box::new(move |_| pipewire_monitor_factory()(&DeviceSelector::Id(t.clone()))),
        None => pipewire_monitor_factory(),
    };
    let mut rec = SystemAudioRecorder::with_factory(SystemAudioConfig::default(), factory);
    rec.start(ctx).expect("system audio start");
    println!("recording {secs} s");
    std::thread::sleep(Duration::from_secs(secs));
    let info = rec.stop(clock.now()).expect("system audio stop");
    println!(
        "track start {:.4} s, details: {}",
        info.start.as_secs_f64(),
        serde_json::to_string(&info.details).unwrap_or_default()
    );
    let on = onsets(&out.join("system.wav"));
    println!("beeps found: {}", on.len());
    if on.len() >= 3 {
        let n = on.len() as f64;
        let xs: Vec<f64> = (0..on.len()).map(|i| i as f64).collect();
        let (mx, my) = (xs.iter().sum::<f64>() / n, on.iter().sum::<f64>() / n);
        let num: f64 = xs.iter().zip(&on).map(|(x, y)| (x - mx) * (y - my)).sum();
        let den: f64 = xs.iter().map(|x| (x - mx) * (x - mx)).sum();
        let slope = num / den;
        println!(
            "first beep at {:.4} s in the file; mean spacing {:.5} s (rate error {:+.0} ppm against 2.000 s)",
            on[0],
            slope,
            (slope / 2.0 - 1.0) * 1e6
        );
        let gaps: Vec<String> = on
            .windows(2)
            .map(|w| format!("{:.3}", w[1] - w[0]))
            .collect();
        println!("spacings: {}", gaps.join(" "));
    }
}
