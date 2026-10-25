//! Mic vs system-audio alignment probe (NFR-3 for the audio tracks, AUD-1.AC2 / AUD-2.AC1).
//!
//! Records the microphone and system audio together on one session clock for up to 10 s and
//! reports where a beep that you play through the speakers lands in each file. Both tracks are
//! stamped with OS capture time, so after the acoustic delay (about 3 ms per meter) the two onsets
//! must agree within a few milliseconds. `analyze` is read-only.
//!
//!   cargo run -p cm-audio --example sync_probe -- record --yes --secs 8 --out /tmp/audio-sync
//!   (within the 8 s, run in another terminal:  afplay /System/Library/Sounds/Ping.aiff)
//!   cargo run -p cm-audio --example sync_probe -- analyze --out /tmp/audio-sync   (reads starts.json)
//!
//! `record` records the microphone: it refuses to run without `--yes`, never prompts for
//! permission (it prints the state and stops), and the files are yours to delete afterwards.

use std::path::PathBuf;
use std::time::Duration;

use cm_audio::{
    microphone_permission, screen_recording_permission, MicConfig, MicRecorder, PermissionState,
    SystemAudioConfig, SystemAudioRecorder,
};
use cm_core::{SessionClock, TrackContext, TrackRecorder};

fn arg(name: &str) -> Option<String> {
    let a: Vec<String> = std::env::args().collect();
    a.iter()
        .position(|x| x == name)
        .and_then(|i| a.get(i + 1).cloned())
}

fn onsets(path: &PathBuf, start_s: f64) -> Vec<f64> {
    let mut r = match hound::WavReader::open(path) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{}: {e}", path.display());
            return Vec::new();
        }
    };
    let ch = usize::from(r.spec().channels);
    let rate = r.spec().sample_rate as f64;
    let s: Vec<f32> = r.samples::<f32>().map(|v| v.unwrap_or(0.0)).collect();
    // Envelope in 1 ms steps; an onset is the first step above 6x the median envelope level and 0.01.
    let step = (rate / 1000.0) as usize * ch;
    let env: Vec<f32> = s
        .chunks(step)
        .map(|c| c.iter().fold(0.0f32, |m, v| m.max(v.abs())))
        .collect();
    let mut sorted = env.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let floor = sorted.get(sorted.len() / 2).copied().unwrap_or(0.0);
    let thr = (floor * 6.0).max(0.01);
    let mut out = Vec::new();
    let mut quiet_ms = 1000;
    for (i, e) in env.iter().enumerate() {
        if *e > thr {
            if quiet_ms >= 300 {
                out.push(start_s + i as f64 / 1000.0);
            }
            quiet_ms = 0;
        } else {
            quiet_ms += 1;
        }
    }
    out
}

fn main() {
    let cmd = std::env::args().nth(1).unwrap_or_default();
    let out = PathBuf::from(arg("--out").unwrap_or_else(|| "/tmp/audio-sync".into()));
    match cmd.as_str() {
        "record" => {
            if !std::env::args().any(|a| a == "--yes") {
                eprintln!("record turns the microphone on; pass --yes to confirm.");
                std::process::exit(2);
            }
            let secs: u64 = arg("--secs")
                .and_then(|s| s.parse().ok())
                .unwrap_or(8)
                .min(10);
            let (mic_ok, sys_ok) = (
                microphone_permission() == PermissionState::Authorized,
                screen_recording_permission() == PermissionState::Authorized,
            );
            println!(
                "microphone: {:?}, screen recording: {:?}",
                microphone_permission(),
                screen_recording_permission()
            );
            if !mic_ok {
                eprintln!("microphone is not authorized for this terminal; nothing recorded.");
                std::process::exit(4);
            }
            std::fs::create_dir_all(&out).expect("create out dir");
            let clock = SessionClock::start();
            let ctx = TrackContext {
                dir: out.clone(),
                clock: clock.clone(),
            };
            let mut mic = MicRecorder::new(MicConfig::default());
            let mut sys = SystemAudioRecorder::new(SystemAudioConfig::default());
            mic.start(ctx.clone()).expect("mic start");
            if sys_ok {
                if let Err(e) = sys.start(ctx) {
                    eprintln!("system audio not started: {e}");
                }
            }
            println!("recording {secs} s... play a beep now");
            std::thread::sleep(Duration::from_secs(secs));
            let m = mic.stop(clock.now()).expect("mic stop");
            println!("mic: {}", serde_json::to_string(&m).unwrap_or_default());
            let mut starts = serde_json::json!({"mic_start": m.start.as_secs_f64()});
            if let Ok(s) = sys.stop(clock.now()) {
                println!("system: {}", serde_json::to_string(&s).unwrap_or_default());
                starts["system_start"] = serde_json::json!(s.start.as_secs_f64());
            }
            let _ = std::fs::write(out.join("starts.json"), starts.to_string());
        }
        "analyze" => {
            let starts: serde_json::Value = std::fs::read_to_string(out.join("starts.json"))
                .ok()
                .and_then(|t| serde_json::from_str(&t).ok())
                .unwrap_or_default();
            let mic_start = starts["mic_start"].as_f64().unwrap_or(0.0);
            let sys_start = starts["system_start"].as_f64().unwrap_or(0.0);
            let m = onsets(&out.join("mic.wav"), mic_start);
            let s = onsets(&out.join("system.wav"), sys_start);
            println!("mic onsets (session s):    {m:?}");
            println!("system onsets (session s): {s:?}");
            for (a, b) in m.iter().zip(s.iter()) {
                println!("mic - system = {:+.1} ms", (a - b) * 1000.0);
            }
        }
        _ => {
            eprintln!("usage: sync_probe record --yes [--secs N] [--out DIR] | analyze [--out DIR]")
        }
    }
}
