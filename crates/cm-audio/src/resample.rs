//! Streaming resampler to the 48 kHz track rate (AUD-1: "resampled if the device runs at another rate").
//!
//! `rubato` (MIT OR Apache-2.0, pure Rust) was chosen over hand-rolled or `dasp` interpolation
//! because its FFT resampler is exact for rational ratios (44.1 -> 48 kHz is 147 -> 160), has a
//! documented filter delay we can trim, and stays well under 1% of a core. The wrapper accepts
//! arbitrary chunk sizes, trims the filter delay at the start and flushes it at the end, so the
//! output is aligned with the input and has `round(frames_in * out_rate / in_rate)` frames.

use cm_core::CmError;
use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Indexing, Resampler};

const CHUNK_FRAMES: usize = 480;

/// Interleaved f32 resampler with arbitrary input chunking.
pub struct StreamResampler {
    inner: Option<Fft<f32>>,
    channels: usize,
    in_rate: u32,
    out_rate: u32,
    staging: Vec<f32>,
    scratch_out: Vec<f32>,
    to_trim: usize,
    total_in: u64,
    total_out: u64,
}

impl StreamResampler {
    pub fn new(in_rate: u32, out_rate: u32, channels: u16) -> Result<Self, CmError> {
        let channels = usize::from(channels.max(1));
        let inner = if in_rate == out_rate {
            None
        } else {
            Some(
                Fft::<f32>::new(
                    in_rate as usize,
                    out_rate as usize,
                    CHUNK_FRAMES,
                    channels,
                    FixedSync::Input,
                )
                .map_err(|e| CmError::internal(format!("resampler {in_rate}->{out_rate}: {e}")))?,
            )
        };
        let (to_trim, scratch) = match &inner {
            Some(r) => (r.output_delay(), r.output_frames_max() * channels),
            None => (0, 0),
        };
        Ok(StreamResampler {
            inner,
            channels,
            in_rate,
            out_rate,
            staging: Vec::new(),
            scratch_out: vec![0.0; scratch],
            to_trim,
            total_in: 0,
            total_out: 0,
        })
    }

    pub fn is_passthrough(&self) -> bool {
        self.inner.is_none()
    }

    pub fn in_rate(&self) -> u32 {
        self.in_rate
    }

    fn expected_out(&self) -> u64 {
        (self.total_in as u128 * u128::from(self.out_rate) + u128::from(self.in_rate) / 2) as u64
            / u64::from(self.in_rate).max(1)
    }

    /// Resamples `input` (interleaved) and appends the available output to `out`.
    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) -> Result<(), CmError> {
        let input = &input[..input.len() - input.len() % self.channels];
        self.total_in += (input.len() / self.channels) as u64;
        let Some(r) = self.inner.as_mut() else {
            out.extend_from_slice(input);
            self.total_out += (input.len() / self.channels) as u64;
            return Ok(());
        };
        self.staging.extend_from_slice(input);
        loop {
            let need = r.input_frames_next();
            if self.staging.len() < need * self.channels {
                break;
            }
            let n_out = Self::run(
                r,
                &self.staging[..need * self.channels],
                need,
                &mut self.scratch_out,
                self.channels,
                None,
            )?;
            self.staging.drain(..need * self.channels);
            Self::emit(
                &self.scratch_out[..n_out * self.channels],
                self.channels,
                &mut self.to_trim,
                &mut self.total_out,
                out,
            );
        }
        Ok(())
    }

    /// Flushes the filter tail so the output covers the whole input, then trims or pads to the
    /// exact expected length.
    pub fn finish(&mut self, out: &mut Vec<f32>) -> Result<(), CmError> {
        let expected = self.expected_out();
        let Some(r) = self.inner.as_mut() else {
            return Ok(());
        };
        let ch = self.channels;
        // Partial last chunk (zero padded by the resampler), then zero chunks until the delay is paid.
        let mut guard = 0;
        let mut partial = Some(self.staging.len() / ch);
        while self.total_out < expected && guard < 64 {
            guard += 1;
            let need = r.input_frames_next();
            let mut chunk = vec![0.0f32; need * ch];
            let have = partial.take().unwrap_or(0).min(need);
            chunk[..have * ch].copy_from_slice(&self.staging[..have * ch]);
            self.staging.drain(..have * ch);
            let indexing = Indexing {
                input_offset: 0,
                output_offset: 0,
                partial_len: Some(have),
                active_channels_mask: None,
            };
            let n_out = Self::run(r, &chunk, need, &mut self.scratch_out, ch, Some(indexing))?;
            Self::emit(
                &self.scratch_out[..n_out * ch],
                ch,
                &mut self.to_trim,
                &mut self.total_out,
                out,
            );
        }
        // Trim any overshoot in this call's output; padding is never needed in practice but keep exact.
        if self.total_out > expected {
            let extra = (self.total_out - expected) as usize;
            out.truncate(out.len().saturating_sub(extra * ch));
            self.total_out = expected;
        }
        self.staging.clear();
        Ok(())
    }

    fn run(
        r: &mut Fft<f32>,
        input: &[f32],
        frames: usize,
        scratch: &mut [f32],
        ch: usize,
        indexing: Option<Indexing>,
    ) -> Result<usize, CmError> {
        let max_out = scratch.len() / ch;
        let inp = InterleavedSlice::new(input, ch, frames)
            .map_err(|e| CmError::internal(format!("resampler input: {e}")))?;
        let mut outp = InterleavedSlice::new_mut(scratch, ch, max_out)
            .map_err(|e| CmError::internal(format!("resampler output: {e}")))?;
        let (_, n_out) = r
            .process_into_buffer(&inp, &mut outp, indexing.as_ref())
            .map_err(|e| CmError::internal(format!("resample: {e}")))?;
        Ok(n_out)
    }

    fn emit(
        produced: &[f32],
        ch: usize,
        to_trim: &mut usize,
        total_out: &mut u64,
        out: &mut Vec<f32>,
    ) {
        let frames = produced.len() / ch;
        let skip = (*to_trim).min(frames);
        *to_trim -= skip;
        out.extend_from_slice(&produced[skip * ch..]);
        *total_out += (frames - skip) as u64;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f32, rate: u32, frames: usize, amp: f32) -> Vec<f32> {
        (0..frames)
            .map(|i| amp * (2.0 * std::f32::consts::PI * freq * i as f32 / rate as f32).sin())
            .collect()
    }

    /// Best-fit amplitude and phase error of `freq` in `x` skipping `skip` edge frames (single-bin DFT).
    fn fit(x: &[f32], freq: f32, rate: u32, skip: usize) -> (f32, f32) {
        let (mut re, mut im, mut n) = (0.0f64, 0.0f64, 0usize);
        for (i, &v) in x.iter().enumerate().skip(skip).take(x.len() - 2 * skip) {
            let w = 2.0 * std::f64::consts::PI * f64::from(freq) * i as f64 / f64::from(rate);
            re += f64::from(v) * w.cos();
            im += f64::from(v) * w.sin();
            n += 1;
        }
        let amp = 2.0 * (re * re + im * im).sqrt() / n as f64;
        // x = A sin(wt + phi) -> re = A/2 sin(phi) n, im = A/2 cos(phi) n
        let phi = re.atan2(im);
        (amp as f32, phi as f32)
    }

    fn run(in_rate: u32, frames: usize, chunk: usize, freq: f32) -> Vec<f32> {
        let x = sine(freq, in_rate, frames, 0.5);
        let mut r = StreamResampler::new(in_rate, 48_000, 1).unwrap();
        let mut out = Vec::new();
        for c in x.chunks(chunk) {
            r.process(c, &mut out).unwrap();
        }
        r.finish(&mut out).unwrap();
        out
    }

    #[test]
    fn passthrough_at_48k() {
        let r = StreamResampler::new(48_000, 48_000, 2).unwrap();
        assert!(r.is_passthrough());
        let mut r = r;
        let mut out = Vec::new();
        r.process(&[0.1, 0.2, 0.3, 0.4], &mut out).unwrap();
        r.finish(&mut out).unwrap();
        assert_eq!(out, vec![0.1, 0.2, 0.3, 0.4]);
    }

    #[test]
    fn upsampling_44k1_keeps_the_tone_and_the_duration() {
        // AUD-1.AC3: 44.1 kHz device -> 48 kHz with the correct duration (+-10 ms).
        let secs = 3.0;
        let frames = (44_100.0 * secs) as usize;
        let out = run(44_100, frames, 441, 1000.0);
        let expected = (48_000.0 * secs) as usize;
        assert!(
            (out.len() as i64 - expected as i64).abs() <= 1,
            "{} vs {expected}",
            out.len()
        );
        let (amp, phi) = fit(&out, 1000.0, 48_000, 4_800);
        assert!((amp - 0.5).abs() < 0.005, "amp {amp}");
        // The filter delay was trimmed: the phase of the tone matches an undelayed sine
        // (sub-sample accuracy: < 0.02 rad at 1 kHz is about 3 us).
        assert!(phi.abs() < 0.02, "phase {phi}");
    }

    #[test]
    fn downsampling_96k_and_odd_chunks() {
        let out = run(96_000, 96_000, 777, 5000.0);
        assert!((out.len() as i64 - 48_000).abs() <= 1, "{}", out.len());
        let (amp, _) = fit(&out, 5000.0, 48_000, 4_800);
        assert!((amp - 0.5).abs() < 0.01, "amp {amp}");
    }

    #[test]
    fn tone_above_the_new_nyquist_is_removed_when_downsampling() {
        // 30 kHz at 96 kHz is above 24 kHz and must not alias into the band.
        let out = run(96_000, 96_000, 960, 30_000.0);
        let rms = (out
            .iter()
            .skip(4_800)
            .take(38_400)
            .map(|v| v * v)
            .sum::<f32>()
            / 38_400.0)
            .sqrt();
        assert!(rms < 0.005, "rms {rms}");
    }

    #[test]
    fn stereo_channels_stay_separate() {
        let l = sine(500.0, 44_100, 44_100, 0.4);
        let mut inter = Vec::new();
        for s in &l {
            inter.push(*s);
            inter.push(0.0);
        }
        let mut r = StreamResampler::new(44_100, 48_000, 2).unwrap();
        let mut out = Vec::new();
        for c in inter.chunks(2 * 300) {
            r.process(c, &mut out).unwrap();
        }
        r.finish(&mut out).unwrap();
        let left: Vec<f32> = out.iter().step_by(2).copied().collect();
        let right_peak = out
            .iter()
            .skip(1)
            .step_by(2)
            .fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(right_peak < 1e-4, "{right_peak}");
        let (amp, _) = fit(&left, 500.0, 48_000, 4_800);
        assert!((amp - 0.4).abs() < 0.005);
    }

    #[test]
    fn short_streams_flush_completely() {
        let out = run(44_100, 1_000, 333, 440.0);
        assert!((out.len() as i64 - 1088).abs() <= 1, "{}", out.len());
    }
}
