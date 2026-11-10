//! The `cm-audio` processor for the edited export: AUD-4 gains and mutes, cuts, pitch-preserving
//! speed changes, optional loudness normalisation and the true-peak limiter, on decoded PCM.

use std::io::Read;
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};

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
pub fn decode_pcm(ffmpeg: &Path, file: &Path, channels: u16) -> Result<Vec<f32>, CmError> {
    decode_pcm_with(ffmpeg, file, channels, 0, &AtomicBool::new(false), &|_| {})
}

/// [`decode_pcm`] for a long file (EDT-6): FFmpeg's output is read in 1 MiB pieces and converted
/// into the one result vector as it arrives, so the peak is the result itself (not the raw bytes
/// plus the samples, which doubled it). `expect_samples` (the length the manifest promises, 0 when
/// unknown) reserves the vector once instead of growing it by doubling; `progress` is told the
/// samples read so far; when `cancel` is set the FFmpeg process is killed and the call returns an
/// error at once.
#[allow(clippy::chunks_exact_to_as_chunks)]
pub fn decode_pcm_with(
    ffmpeg: &Path,
    file: &Path,
    channels: u16,
    expect_samples: usize,
    cancel: &AtomicBool,
    progress: &dyn Fn(usize),
) -> Result<Vec<f32>, CmError> {
    let mut cmd = cm_platform::process::command(ffmpeg);
    cmd.args(["-hide_banner", "-nostdin", "-v", "error", "-i"])
        .arg(file)
        .args(["-vn", "-f", "f32le", "-ac"])
        .arg(channels.to_string())
        .arg("-ar")
        .arg(AUDIO_RATE.to_string())
        .arg("pipe:1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // One spawn at a time (see `cm_platform::process::spawn`), then read without the lock.
    let mut child = cm_platform::process::spawn(&mut cmd)
        .map_err(|e| CmError::internal(format!("cannot run FFmpeg to read audio: {e}")))?;
    let (Some(mut stdout), Some(stderr)) = (child.stdout.take(), child.stderr.take()) else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(CmError::internal("FFmpeg gave no output pipe"));
    };
    // The error text is read on its own thread so a chatty FFmpeg can never block on a full pipe.
    let err_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stderr.take(64 * 1024).read_to_end(&mut buf);
        String::from_utf8_lossy(&buf).into_owned()
    });
    // Reserve once, with a little room; at most 8 GiB of samples on a wrong promise.
    let reserve = expect_samples.saturating_add(expect_samples / 64 + 4096);
    let mut out: Vec<f32> = Vec::with_capacity(reserve.min(2 << 30));
    let mut buf = vec![0u8; 1 << 20];
    let mut carry = 0usize; // bytes of an unfinished sample at the start of `buf`
    let mut cancelled = false;
    loop {
        if cancel.load(Ordering::Relaxed) {
            cancelled = true;
            break;
        }
        let n = match stdout.read(&mut buf[carry..]) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        };
        let have = carry + n;
        let whole = have - have % 4;
        out.extend(
            buf[..whole]
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])),
        );
        buf.copy_within(whole..have, 0);
        carry = have - whole;
        progress(out.len());
    }
    if cancelled {
        let _ = child.kill();
    }
    drop(stdout);
    let status = child.wait();
    let err = err_thread.join().unwrap_or_default();
    if cancelled {
        return Err(CmError::internal("reading the audio was cancelled"));
    }
    if !status.is_ok_and(|s| s.success()) {
        return Err(CmError::internal(format!(
            "FFmpeg could not read `{}`: {}",
            file.display(),
            err.lines().next().unwrap_or("")
        )));
    }
    Ok(out)
}

/// Interleaved f32 samples as little-endian bytes, for FFmpeg's `-f f32le` input.
pub fn f32le_bytes(samples: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(samples.len() * 4);
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}
