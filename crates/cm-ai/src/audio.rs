//! Audio in the shape Whisper wants: mono 16 kHz `f32`, decoded with the FFmpeg sidecar the same
//! way `cm-media` finds it (`CM_FFMPEG`, next to the executable, `PATH`).

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};

use cm_core::CmError;

/// Sample rate Whisper expects.
pub const SAMPLE_RATE: u32 = 16_000;

/// Decodes the first audio stream of `file` to mono 16 kHz `f32` samples. Memory is 4 bytes per
/// sample (about 230 MB for a one-hour recording).
pub fn decode_16k_mono(file: &Path) -> Result<Vec<f32>, CmError> {
    let _t = cm_core::perf::span("audio", "decode");
    let tools = cm_media::tools::locate()?;
    let mut child = Command::new(&tools.ffmpeg)
        .args(["-nostdin", "-v", "error", "-i"])
        .arg(file)
        .args(["-vn", "-ac", "1", "-ar"])
        .arg(SAMPLE_RATE.to_string())
        .args(["-f", "f32le", "pipe:1"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| CmError::internal(format!("cannot run FFmpeg: {e}")))?;
    let mut out = child
        .stdout
        .take()
        .ok_or_else(|| CmError::internal("FFmpeg has no output"))?;
    let mut samples: Vec<f32> = Vec::new();
    let mut buf = vec![0u8; 256 * 1024];
    let mut carry: Vec<u8> = Vec::new();
    loop {
        let n = out
            .read(&mut buf)
            .map_err(|e| CmError::internal(format!("reading FFmpeg output: {e}")))?;
        if n == 0 {
            break;
        }
        carry.extend_from_slice(&buf[..n]);
        let whole = carry.len() / 4 * 4;
        let (words, _) = carry[..whole].as_chunks::<4>();
        samples.extend(words.iter().map(|b| f32::from_le_bytes(*b)));
        carry.drain(..whole);
    }
    let status = child
        .wait()
        .map_err(|e| CmError::internal(format!("FFmpeg: {e}")))?;
    if !status.success() {
        let mut err = String::new();
        if let Some(mut s) = child.stderr.take() {
            let _ = s.read_to_string(&mut err);
        }
        let err = err.lines().last().unwrap_or("unknown error").to_owned();
        return Err(CmError::invalid_input(format!(
            "FFmpeg could not read the audio of {}: {err}",
            file.display()
        )));
    }
    Ok(samples)
}

/// Adds `src` into `dst` starting at sample `offset`, growing `dst` with silence as needed.
/// Results are clamped to -1.0..=1.0 so two loud tracks do not wrap.
pub fn mix_at(dst: &mut Vec<f32>, src: &[f32], offset: usize) {
    if dst.len() < offset + src.len() {
        dst.resize(offset + src.len(), 0.0);
    }
    for (d, s) in dst[offset..].iter_mut().zip(src) {
        *d = (*d + *s).clamp(-1.0, 1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mix_at_places_grows_and_clamps() {
        let mut d = vec![0.5, 0.5];
        mix_at(&mut d, &[0.75, 0.25, 0.25], 1);
        assert_eq!(d, vec![0.5, 1.0, 0.25, 0.25]);
        let mut e = Vec::new();
        mix_at(&mut e, &[0.1], 3);
        assert_eq!(e, vec![0.0, 0.0, 0.0, 0.1]);
    }

    /// Decoding a generated 1 s tone gives about 16000 mono samples at the tone's level.
    #[test]
    fn decodes_a_generated_tone_to_16k_mono() {
        let Ok(tools) = cm_media::tools::locate() else {
            println!("SKIPPED: FFmpeg not found");
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let wav = dir.path().join("tone.wav");
        let ok = Command::new(&tools.ffmpeg)
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=1",
                "-ar",
                "44100",
                "-ac",
                "2",
                "-y",
            ])
            .arg(&wav)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        assert!(ok, "could not generate the tone");
        let s = decode_16k_mono(&wav).unwrap();
        assert!((15_900..=16_100).contains(&s.len()), "{} samples", s.len());
        let peak = s.iter().fold(0f32, |m, v| m.max(v.abs()));
        assert!((0.1..=1.0).contains(&peak), "peak {peak}");
    }

    #[test]
    fn a_missing_file_is_a_clear_error() {
        if cm_media::tools::locate().is_err() {
            println!("SKIPPED: FFmpeg not found");
            return;
        }
        let e = decode_16k_mono(Path::new("/nonexistent/none.wav")).unwrap_err();
        assert_eq!(e.code().as_str(), "invalid_input");
    }
}
