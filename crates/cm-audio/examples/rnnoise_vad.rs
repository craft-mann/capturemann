//! Not shipped: RNNoise's own voice-activity probability (one value per 10 ms frame) for a file,
//! an opinion on "is this speech" that does not depend on Silero or on the speaker engine.
//! Prints one JSON list. Needs FFmpeg (`CM_FFMPEG` or `PATH`).
//!
//! ```text
//! cargo run --release -p cm-audio --example rnnoise_vad -- <audio file>
//! ```

#![allow(clippy::unwrap_used, clippy::chunks_exact_to_as_chunks)]

use std::io::Read;
use std::process::{Command, Stdio};

use nnnoiseless::{DenoiseState, FRAME_SIZE};

fn main() {
    let input = std::env::args().nth(1).unwrap();
    let ffmpeg = std::env::var("CM_FFMPEG").unwrap_or_else(|_| "ffmpeg".into());
    let mut raw = Vec::new();
    Command::new(&ffmpeg)
        .args([
            "-nostdin", "-v", "error", "-i", &input, "-vn", "-ac", "1", "-ar", "48000", "-f",
            "f32le", "pipe:1",
        ])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap()
        .stdout
        .take()
        .unwrap()
        .read_to_end(&mut raw)
        .unwrap();
    let samples: Vec<f32> = raw
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]) * 32768.0)
        .collect();
    let mut st = DenoiseState::new();
    let mut out = [0.0f32; FRAME_SIZE];
    let probs: Vec<f32> = samples
        .chunks_exact(FRAME_SIZE)
        .map(|f| (st.process_frame(&mut out, f) * 1000.0).round() / 1000.0)
        .collect();
    println!("{}", serde_json::to_string(&probs).unwrap());
}
