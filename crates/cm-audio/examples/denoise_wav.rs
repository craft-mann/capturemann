//! Not shipped: runs the recorder's RNNoise pass (`Denoiser`, 48 kHz mono) over a file, so a
//! recording that has only a raw track can be measured raw against noise-reduced. Needs FFmpeg
//! (`CM_FFMPEG` or `PATH`).
//!
//! ```text
//! cargo run --release -p cm-audio --example denoise_wav -- <in> <out.wav>
//! ```

#![allow(clippy::unwrap_used, clippy::chunks_exact_to_as_chunks)]

use std::io::{Read, Write};
use std::process::{Command, Stdio};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (input, output) = (&args[0], &args[1]);
    let ffmpeg = std::env::var("CM_FFMPEG").unwrap_or_else(|_| "ffmpeg".into());
    let mut raw = Vec::new();
    Command::new(&ffmpeg)
        .args([
            "-nostdin", "-v", "error", "-i", input, "-vn", "-ac", "1", "-ar", "48000", "-f",
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
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect();
    let mut d = cm_audio::denoise::Denoiser::new(1);
    let mut out = Vec::with_capacity(samples.len());
    d.process(&samples, &mut out);
    d.finish(&mut out);
    let mut child = Command::new(&ffmpeg)
        .args([
            "-nostdin",
            "-v",
            "error",
            "-y",
            "-f",
            "f32le",
            "-ar",
            "48000",
            "-ac",
            "1",
            "-i",
            "pipe:0",
            "-c:a",
            "pcm_f32le",
            output,
        ])
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    for chunk in out.chunks(1 << 16) {
        let bytes: Vec<u8> = chunk.iter().flat_map(|s| s.to_le_bytes()).collect();
        stdin.write_all(&bytes).unwrap();
    }
    drop(stdin);
    assert!(child.wait().unwrap().success());
    println!("{} samples", out.len());
}
