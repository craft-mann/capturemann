//! The `cm-audio` processor for the edited export: AUD-4 gains and mutes, cuts, pitch-preserving
//! speed changes, optional loudness normalisation and the true-peak limiter, on decoded PCM.

use std::path::Path;
use std::process::{Command, Stdio};

use cm_audio::mix::{mix_tracks, MixParams, TrackAudio};
use cm_core::CmError;

use crate::edit_plan::{AudioKind, AudioProcessor, FfmpegAudio, MixInput, MixedPcm};
use crate::graph::AUDIO_RATE;

/// Mixes with [`cm_audio::mix`]; the filter-chain methods fall back to [`FfmpegAudio`] and are
/// only used when a caller asks for the graph path.
#[derive(Debug, Clone, Copy, Default)]
pub struct DspAudio;

impl AudioProcessor for DspAudio {
    fn stretch(&self, factor: f64) -> Vec<String> {
        FfmpegAudio.stretch(factor)
    }

    fn track(&self, kind: AudioKind) -> Vec<String> {
        FfmpegAudio.track(kind)
    }

    fn mix(&self, input: &MixInput<'_>) -> Option<Result<MixedPcm, CmError>> {
        let tracks: Vec<TrackAudio<'_>> = input
            .tracks
            .iter()
            .map(|t| {
                let (gain_db, mute) = match t.kind {
                    AudioKind::Mic => (input.audio.mic_gain_db, input.audio.mic_mute),
                    AudioKind::System => (input.audio.system_gain_db, input.audio.system_mute),
                };
                TrackAudio {
                    samples: &t.samples,
                    channels: t.channels,
                    sample_rate: AUDIO_RATE,
                    start: t.start,
                    gain_db,
                    mute,
                }
            })
            .collect();
        let params = MixParams {
            sample_rate: AUDIO_RATE,
            out_channels: input.out_channels,
            loudness_lufs: input.loudness_lufs,
            ..MixParams::default()
        };
        Some(
            mix_tracks(&tracks, input.gaps, input.timeline, &params).map(|m| MixedPcm {
                samples: m.samples,
                channels: m.channels,
                sample_rate: m.sample_rate,
                normalise_gain_db: m.normalise_gain_db,
                integrated_lufs: m.integrated_lufs,
                true_peak_dbtp: m.true_peak_dbtp,
            }),
        )
    }
}

/// Decodes the first audio stream of `file` to interleaved f32 at [`AUDIO_RATE`] with `channels`
/// (1 or 2) channels.
#[allow(clippy::chunks_exact_to_as_chunks)]
pub fn decode_pcm(ffmpeg: &Path, file: &Path, channels: u16) -> Result<Vec<f32>, CmError> {
    let out = Command::new(ffmpeg)
        .args(["-hide_banner", "-nostdin", "-v", "error", "-i"])
        .arg(file)
        .args(["-vn", "-f", "f32le", "-ac"])
        .arg(channels.to_string())
        .arg("-ar")
        .arg(AUDIO_RATE.to_string())
        .arg("pipe:1")
        .stdin(Stdio::null())
        .output()
        .map_err(|e| CmError::internal(format!("cannot run FFmpeg to read audio: {e}")))?;
    if !out.status.success() {
        return Err(CmError::internal(format!(
            "FFmpeg could not read `{}`: {}",
            file.display(),
            String::from_utf8_lossy(&out.stderr)
                .lines()
                .next()
                .unwrap_or("")
        )));
    }
    Ok(out
        .stdout
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect())
}

/// Interleaved f32 samples as little-endian bytes, for FFmpeg's `-f f32le` input.
pub fn f32le_bytes(samples: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(samples.len() * 4);
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}
