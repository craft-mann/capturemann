//! Voice activity detection with the Silero model through whisper.cpp (0.2.7 transcription review,
//! step 3), and a decoder wrapper that reads only the speech: [`VadFirst`].
//!
//! The model file (`ggml-silero-v5.1.2.bin`, MIT, 885,098 bytes) is a downloaded, checksummed file
//! like the other models; nothing here fetches it. The speech probabilities of the model (one per
//! 32 ms frame) are turned into regions by our own small, tested rule ([`regions`]) so that the
//! far-field settings of the review are plain parameters, and so that the share of speech under any
//! stretch ([`speech_share`]) can be asked for, which is the second opinion the hallucination gate
//! needs.

use std::path::Path;
use std::sync::Mutex;

use cm_core::{CmError, MediaTime};
use whisper_rs::{WhisperVadContext, WhisperVadContextParams};

use crate::transcribe::{media_secs, Pcm16k, Transcriber, Transcript, Word};

/// Samples per probability (32 ms at 16 kHz).
pub const FRAME: usize = 512;
const RATE: f64 = 16_000.0;

/// The model's probabilities for audio.
pub struct SileroVad {
    ctx: Mutex<WhisperVadContext>,
}

impl SileroVad {
    pub fn load(model: &Path) -> Result<SileroVad, CmError> {
        let mut params = WhisperVadContextParams::new();
        params.set_use_gpu(false);
        params.set_n_threads(2);
        let ctx = WhisperVadContext::new(&model.to_string_lossy(), params).map_err(|e| {
            CmError::invalid_input(format!(
                "{} is not a usable VAD model ({e})",
                model.display()
            ))
        })?;
        Ok(SileroVad {
            ctx: Mutex::new(ctx),
        })
    }

    /// One speech probability per [`FRAME`] samples of 16 kHz audio.
    pub fn probabilities(&self, samples: &[f32]) -> Result<Vec<f32>, CmError> {
        let mut ctx = self.ctx.lock().unwrap_or_else(|p| p.into_inner());
        ctx.detect_speech(samples)
            .map_err(|e| CmError::internal(format!("voice activity detection failed: {e}")))?;
        Ok(ctx.probabilities().to_vec())
    }
}

/// How probabilities become regions. The defaults are the review's far-field values for Vietnamese.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VadParams {
    /// A frame is speech at or above this probability.
    pub threshold: f32,
    /// A region shorter than this is dropped (seconds).
    pub min_speech_s: f64,
    /// A gap shorter than this does not end a region (seconds).
    pub min_silence_s: f64,
    /// Each region is widened by this much on both sides (seconds).
    pub pad_s: f64,
    /// A region longer than this is cut (seconds), at its least speech-like frame.
    pub max_speech_s: f64,
    /// Regions closer than this are one (seconds), applied after padding.
    pub merge_gap_s: f64,
}

impl Default for VadParams {
    fn default() -> Self {
        VadParams {
            threshold: 0.32,
            min_speech_s: 0.15,
            min_silence_s: 0.3,
            pad_s: 0.35,
            max_speech_s: 25.0,
            merge_gap_s: 1.0,
        }
    }
}

fn secs(frame: usize) -> f64 {
    frame as f64 * FRAME as f64 / RATE
}

/// Speech regions `(start, end)` in seconds from the probabilities.
pub fn regions(probs: &[f32], p: &VadParams, total_secs: f64) -> Vec<(f64, f64)> {
    // Frames above the threshold, bridged over gaps shorter than min_silence_s.
    let bridge = (p.min_silence_s * RATE / FRAME as f64).ceil() as usize;
    let mut runs: Vec<(usize, usize)> = Vec::new();
    for (i, &v) in probs.iter().enumerate() {
        if v < p.threshold {
            continue;
        }
        match runs.last_mut() {
            Some(last) if i - last.1 < bridge => last.1 = i + 1,
            _ => runs.push((i, i + 1)),
        }
    }
    let mut out: Vec<(f64, f64)> = runs
        .into_iter()
        .map(|(a, b)| (secs(a), secs(b)))
        .filter(|(a, b)| b - a >= p.min_speech_s)
        .map(|(a, b)| ((a - p.pad_s).max(0.0), (b + p.pad_s).min(total_secs)))
        .collect();
    // Merge regions that touch after padding or sit closer than the merge gap.
    let mut merged: Vec<(f64, f64)> = Vec::new();
    for r in out.drain(..) {
        match merged.last_mut() {
            Some(last) if r.0 - last.1 < p.merge_gap_s => last.1 = last.1.max(r.1),
            _ => merged.push(r),
        }
    }
    // Cut the too long ones at the least speech-like frame near the middle of what is left.
    let mut cut: Vec<(f64, f64)> = Vec::new();
    for (a, b) in merged {
        let mut from = a;
        while b - from > p.max_speech_s {
            let (lo, hi) = (from + p.max_speech_s * 0.6, from + p.max_speech_s);
            let (fl, fh) = (
                (lo * RATE / FRAME as f64) as usize,
                (hi * RATE / FRAME as f64) as usize,
            );
            let at = (fl..fh.min(probs.len()))
                .min_by(|&x, &y| probs[x].total_cmp(&probs[y]))
                .map_or(hi, secs);
            cut.push((from, at));
            from = at;
        }
        cut.push((from, b));
    }
    cut
}

/// The share of `a..b` (seconds) whose frames are at or above `threshold`.
pub fn speech_share(probs: &[f32], a: f64, b: f64, threshold: f32) -> f64 {
    let (fa, fb) = (
        (a * RATE / FRAME as f64) as usize,
        ((b * RATE / FRAME as f64).ceil() as usize).min(probs.len()),
    );
    if fb <= fa {
        return 0.0;
    }
    probs[fa..fb].iter().filter(|p| **p >= threshold).count() as f64 / (fb - fa) as f64
}

/// A decoder that reads the speech regions of what it is given, each as a call of its own, and
/// leaves the rest unread: whisper.cpp's concatenated VAD mode stitches regions together; this
/// does not.
pub struct VadFirst<'a> {
    pub inner: &'a dyn Transcriber,
    pub vad: &'a SileroVad,
    pub params: VadParams,
}

impl Transcriber for VadFirst<'_> {
    fn transcribe(
        &self,
        audio: Pcm16k<'_>,
        language: Option<&str>,
        progress: &mut dyn FnMut(f32) -> bool,
    ) -> Result<Transcript, CmError> {
        self.transcribe_prompted(audio, language, None, progress)
    }

    fn transcribe_prompted(
        &self,
        audio: Pcm16k<'_>,
        language: Option<&str>,
        prompt: Option<&str>,
        progress: &mut dyn FnMut(f32) -> bool,
    ) -> Result<Transcript, CmError> {
        let total = audio.samples.len() as f64 / RATE;
        let probs = self.vad.probabilities(audio.samples)?;
        let regions = regions(&probs, &self.params, total);
        let mut words: Vec<Word> = Vec::new();
        let mut first: Option<Transcript> = None;
        let mut language = language.map(str::to_owned);
        for (i, &(a, b)) in regions.iter().enumerate() {
            let (sa, sb) = (
                (a * RATE) as usize,
                ((b * RATE) as usize).min(audio.samples.len()),
            );
            if sb <= sa {
                continue;
            }
            let (lo, hi) = (
                i as f32 / regions.len() as f32,
                (i + 1) as f32 / regions.len() as f32,
            );
            let t = self.inner.transcribe_prompted(
                Pcm16k {
                    samples: &audio.samples[sa..sb],
                    start: audio.start + media_secs(a),
                },
                language.as_deref(),
                prompt,
                &mut |p| progress(lo + (hi - lo) * p),
            )?;
            if language.as_deref().is_none_or(|l| l.contains(','))
                && !t.words.is_empty()
                && !t.language.is_empty()
            {
                language = Some(t.language.clone());
            }
            words.extend(t.words.iter().cloned());
            if first.is_none() {
                first = Some(t);
            }
        }
        let _ = progress(1.0);
        let base = first.unwrap_or_else(|| Transcript {
            version: crate::transcribe::TRANSCRIPT_VERSION,
            language: language.clone().unwrap_or_default(),
            model: String::new(),
            words: Vec::new(),
            note: None,
            sounds: Vec::new(),
            segments: Vec::new(),
        });
        let _ = MediaTime::ZERO;
        Ok(Transcript {
            language: language.unwrap_or(base.language),
            note: words.is_empty().then(|| "No speech detected".to_owned()),
            words,
            segments: Vec::new(),
            ..base
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 32 ms frames: `speech` frames of probability 0.9 between silences of 0.02.
    fn probs(pattern: &[(usize, f32)]) -> Vec<f32> {
        pattern
            .iter()
            .flat_map(|&(n, p)| std::iter::repeat_n(p, n))
            .collect()
    }

    #[test]
    fn short_gaps_do_not_split_and_short_blips_are_dropped() {
        // 1 s speech, 0.2 s gap, 1 s speech, 3 s silence, 0.1 s blip, 3 s silence.
        let p = probs(&[
            (31, 0.9),
            (6, 0.02),
            (31, 0.9),
            (94, 0.02),
            (3, 0.9),
            (94, 0.02),
        ]);
        let total = p.len() as f64 * 0.032;
        let r = regions(&p, &VadParams::default(), total);
        assert_eq!(r.len(), 1, "{r:?}");
        assert!(r[0].0 < 0.05 && (r[0].1 - 2.4).abs() < 0.3, "{r:?}");
    }

    #[test]
    fn regions_close_after_padding_are_one_and_far_ones_are_not() {
        // speech 1 s, gap 1.0 s, speech 1 s: padding 0.35 each side leaves a 0.3 s hole: merged.
        let near = probs(&[(31, 0.9), (31, 0.02), (31, 0.9)]);
        assert_eq!(
            regions(&near, &VadParams::default(), near.len() as f64 * 0.032).len(),
            1
        );
        let far = probs(&[(31, 0.9), (156, 0.02), (31, 0.9)]);
        assert_eq!(
            regions(&far, &VadParams::default(), far.len() as f64 * 0.032).len(),
            2
        );
    }

    #[test]
    fn a_region_over_the_maximum_is_cut_at_its_quietest_frame() {
        // 40 s of speech with one dip at 21 s of probability 0.4 (still speech for the threshold).
        let mut p = vec![0.9f32; (40.0 / 0.032) as usize];
        p[(21.0 / 0.032) as usize] = 0.35;
        let r = regions(&p, &VadParams::default(), 40.0);
        assert!(r.len() >= 2, "{r:?}");
        assert!(r.iter().all(|(a, b)| b - a <= 25.1), "{r:?}");
        assert!((r[0].1 - 21.0).abs() < 1.0, "cut near the dip: {r:?}");
    }

    #[test]
    fn the_speech_share_counts_frames_over_the_threshold() {
        let p = probs(&[(31, 0.9), (31, 0.02)]);
        assert!((speech_share(&p, 0.0, 1.984, 0.3) - 0.5).abs() < 0.05);
        assert_eq!(speech_share(&p, 5.0, 6.0, 0.3), 0.0);
    }
}
