//! RNNoise noise suppression (AUD-3) via `nnnoiseless` (BSD-3-Clause, pure Rust).
//!
//! RNNoise works on 480-sample frames of 48 kHz audio scaled to the 16-bit range, one state per
//! channel. The first output frame is not valid (the overlap-add window has no history) and the
//! algorithm lags its input by exactly one frame; both are removed here, so the denoised stream
//! is sample-aligned with the raw stream and has the same length.

use nnnoiseless::{DenoiseState, FRAME_SIZE};

/// Algorithmic delay in frames (samples per channel) that [`Denoiser`] removes.
pub const RNNOISE_DELAY: usize = FRAME_SIZE;
const SCALE: f32 = 32768.0;

/// Multi-channel streaming denoiser with arbitrary input chunking.
pub struct Denoiser {
    states: Vec<Box<DenoiseState<'static>>>,
    channels: usize,
    /// Interleaved input waiting for a full frame.
    pending: Vec<f32>,
    to_drop: usize,
    total_in: u64,
    total_out: u64,
    in_frame: Vec<f32>,
    out_frame: Vec<f32>,
}

impl Denoiser {
    pub fn new(channels: u16) -> Self {
        let channels = usize::from(channels.max(1));
        Denoiser {
            states: (0..channels).map(|_| DenoiseState::new()).collect(),
            channels,
            pending: Vec::new(),
            to_drop: RNNOISE_DELAY,
            total_in: 0,
            total_out: 0,
            in_frame: vec![0.0; FRAME_SIZE],
            out_frame: vec![0.0; FRAME_SIZE],
        }
    }

    fn run_block(&mut self, block: &[f32], out: &mut Vec<f32>) {
        let ch = self.channels;
        let mut denoised = vec![0.0f32; FRAME_SIZE * ch];
        for c in 0..ch {
            for (i, v) in self.in_frame.iter_mut().enumerate() {
                *v = block[i * ch + c] * SCALE;
            }
            self.states[c].process_frame(&mut self.out_frame, &self.in_frame);
            for (i, v) in self.out_frame.iter().enumerate() {
                denoised[i * ch + c] = *v / SCALE;
            }
        }
        let skip = self.to_drop.min(FRAME_SIZE);
        self.to_drop -= skip;
        out.extend_from_slice(&denoised[skip * ch..]);
        self.total_out += (FRAME_SIZE - skip) as u64;
    }

    /// Denoises `input` (interleaved, 48 kHz, nominal -1..1) and appends whatever is ready to `out`.
    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        let input = &input[..input.len() - input.len() % self.channels];
        self.total_in += (input.len() / self.channels) as u64;
        self.pending.extend_from_slice(input);
        let block_len = FRAME_SIZE * self.channels;
        let mut offset = 0;
        while self.pending.len() - offset >= block_len {
            let block: Vec<f32> = self.pending[offset..offset + block_len].to_vec();
            self.run_block(&block, out);
            offset += block_len;
        }
        self.pending.drain(..offset);
    }

    /// Flushes the delay line so the output has exactly as many frames as were fed in.
    pub fn finish(&mut self, out: &mut Vec<f32>) {
        let ch = self.channels;
        let mut guard = 0;
        while self.total_out < self.total_in && guard < 8 {
            guard += 1;
            let have = self.pending.len() / ch;
            let mut block = vec![0.0f32; FRAME_SIZE * ch];
            block[..have * ch].copy_from_slice(&self.pending[..have * ch]);
            self.pending.clear();
            let mut tmp = Vec::new();
            self.run_block(&block, &mut tmp);
            out.extend_from_slice(&tmp);
        }
        if self.total_out > self.total_in {
            let extra = (self.total_out - self.total_in) as usize;
            out.truncate(out.len().saturating_sub(extra * ch));
            self.total_out = self.total_in;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic white noise in [-amp, amp].
    pub(crate) fn noise(n: usize, amp: f32, seed: u32) -> Vec<f32> {
        let mut s = seed.max(1);
        (0..n)
            .map(|_| {
                s ^= s << 13;
                s ^= s >> 17;
                s ^= s << 5;
                (s as f32 / u32::MAX as f32 * 2.0 - 1.0) * amp
            })
            .collect()
    }

    /// Voice-like test signal: harmonic stack with a gliding pitch (so it is not periodic) and a
    /// syllabic envelope.
    pub(crate) fn voice(frames: usize, amp: f32) -> Vec<f32> {
        let mut phase = 0.0f32;
        (0..frames)
            .map(|i| {
                let t = i as f32 / 48_000.0;
                let env = (0.5 + 0.5 * (2.0 * std::f32::consts::PI * 3.0 * t).sin()).powi(2);
                let f0 = 130.0 + 40.0 * (2.0 * std::f32::consts::PI * 0.7 * t).sin();
                phase += 2.0 * std::f32::consts::PI * f0 / 48_000.0;
                let mut v = 0.0;
                for h in 1..=12 {
                    v += (phase * h as f32).sin() / h as f32;
                }
                v * env * amp
            })
            .collect()
    }

    /// Fan-like noise: white noise through a one-pole low-pass.
    pub(crate) fn fan(frames: usize, amp: f32, seed: u32) -> Vec<f32> {
        let mut y = 0.0f32;
        noise(frames, 1.0, seed)
            .into_iter()
            .map(|x| {
                y += 0.05 * (x - y);
                y * amp * 6.0
            })
            .collect()
    }

    fn rms_db(x: &[f32]) -> f32 {
        let ms = x.iter().map(|v| v * v).sum::<f32>() / x.len().max(1) as f32;
        10.0 * ms.max(1e-14).log10()
    }

    #[test]
    fn output_length_matches_input_for_odd_chunks() {
        let x = noise(48_000 + 123, 0.1, 7);
        let mut d = Denoiser::new(1);
        let mut out = Vec::new();
        for c in x.chunks(701) {
            d.process(c, &mut out);
        }
        d.finish(&mut out);
        assert_eq!(out.len(), x.len());
    }

    #[test]
    fn stereo_keeps_channels_independent() {
        let n = 48_000;
        let v = voice(n, 0.3);
        let mut inter = Vec::new();
        for s in &v {
            inter.push(*s);
            inter.push(0.0);
        }
        let mut d = Denoiser::new(2);
        let mut out = Vec::new();
        d.process(&inter, &mut out);
        d.finish(&mut out);
        assert_eq!(out.len(), inter.len());
        let right_peak = out
            .iter()
            .skip(1)
            .step_by(2)
            .fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(right_peak < 1e-3, "{right_peak}");
    }

    #[test]
    fn denoised_output_is_time_aligned_with_the_input() {
        // Find the lag that maximises the cross-correlation between the voice-like input and the
        // denoised output: it must be 0, so the track needs no timing compensation downstream.
        let n = 48_000;
        let v = voice(n, 0.3);
        let mut d = Denoiser::new(1);
        let mut out = Vec::new();
        d.process(&v, &mut out);
        d.finish(&mut out);
        let mut best = (0i32, f32::MIN);
        for lag in -1000i32..=1000 {
            let mut acc = 0.0f32;
            for (i, x) in v.iter().enumerate().take(n - 4_800).skip(4_800) {
                let j = (i as i32 + lag) as usize;
                acc += x * out[j];
            }
            if acc > best.1 {
                best = (lag, acc);
            }
        }
        assert!(best.0.abs() <= 2, "residual lag {} samples", best.0);
    }

    #[test]
    fn suppresses_fan_noise_between_speech_by_more_than_15_db() {
        // AUD-3.AC1 (synthetic fixture: fan noise throughout, speech-like burst in the middle). The
        // real speech + fan recording check lives in tests/manual/AUD-1.md.
        let n = 72_000;
        let mut x = fan(n * 3, 0.03, 5);
        for (i, v) in voice(n, 0.3).into_iter().enumerate() {
            x[n + i] += v;
        }
        let mut d = Denoiser::new(1);
        let mut out = Vec::new();
        d.process(&x, &mut out);
        d.finish(&mut out);
        // Non-speech sections, after the model has settled.
        for r in [24_000..n, 2 * n + 24_000..3 * n] {
            let (raw, den) = (rms_db(&x[r.clone()]), rms_db(&out[r]));
            assert!(raw - den >= 15.0, "raw {raw:.1} dB, denoised {den:.1} dB");
        }
    }

    #[test]
    fn keeps_voice_like_signal_audible() {
        let v = voice(48_000 * 2, 0.3);
        let mut d = Denoiser::new(1);
        let mut out = Vec::new();
        d.process(&v, &mut out);
        d.finish(&mut out);
        let raw = rms_db(&v[9_600..]);
        let den = rms_db(&out[9_600..]);
        assert!(raw - den < 12.0, "raw {raw:.1} dB, denoised {den:.1} dB");
    }
}
