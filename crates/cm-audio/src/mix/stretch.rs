//! Pitch-preserving time stretch (EDT-2): WSOLA, waveform-similarity overlap-add.
//!
//! Frames of `WIN` samples are copied from the input at a hop of `HOP * speed` and overlapped at
//! a hop of `HOP` in the output. Each frame start is nudged by up to `SEARCH` samples to the
//! place whose beginning best continues the previous frame, so periodic signals (voice) stay
//! in phase and the pitch does not change. The search runs on a 4x decimated mono mix and is
//! refined at full rate.

const WIN: usize = 1024;
const HOP: usize = WIN / 2;
const SEARCH: usize = 384;
const DECIM: usize = 4;

fn hann() -> Vec<f32> {
    (0..WIN)
        .map(|i| (0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / WIN as f64).cos()) as f32)
        .collect()
}

/// Normalized correlation of `reference` with `cand`, up to the constant energy of `reference`.
fn score(reference: &[f32], cand: &[f32]) -> f64 {
    let mut dot = 0f64;
    let mut energy = 1e-9f64;
    for (a, b) in reference.iter().zip(cand) {
        dot += f64::from(*a) * f64::from(*b);
        energy += f64::from(*b) * f64::from(*b);
    }
    dot / energy.sqrt()
}

/// Stretches interleaved `input` so that it lasts `out_frames` frames. `speed` is the playback
/// factor: input frames consumed per output frame (2.0 = twice as fast, 0.5 = half speed).
/// Pitch is unchanged. A speed of 1 returns the input cut or zero-padded to `out_frames`.
pub fn stretch(input: &[f32], channels: usize, speed: f64, out_frames: usize) -> Vec<f32> {
    let channels = channels.max(1);
    let in_frames = input.len() / channels;
    let mut out = vec![0f32; out_frames * channels];
    if out_frames == 0 || in_frames == 0 {
        return out;
    }
    if (speed - 1.0).abs() < 1e-6 {
        let n = in_frames.min(out_frames) * channels;
        out[..n].copy_from_slice(&input[..n]);
        return out;
    }
    let win = hann();
    // Zero padding so every search and copy stays inside the buffer.
    let lead = SEARCH + DECIM;
    let last_nominal = ((out_frames + 2 * HOP) as f64 * speed).ceil() as usize;
    let padded_frames = lead + in_frames.max(last_nominal) + WIN + SEARCH + 2 * DECIM + HOP;
    let mut pad = vec![0f32; padded_frames * channels];
    pad[lead * channels..(lead + in_frames) * channels]
        .copy_from_slice(&input[..in_frames * channels]);
    let mono: Vec<f32> = (0..padded_frames)
        .map(|i| pad[i * channels..(i + 1) * channels].iter().sum::<f32>() / channels as f32)
        .collect();
    let coarse: Vec<f32> = mono
        .chunks(DECIM)
        .map(|c| c.iter().sum::<f32>() / DECIM as f32)
        .collect();

    let mut buf = vec![0f32; (out_frames + 2 * WIN) * channels];
    let mut prev: Option<usize> = None;
    let mut k = 0usize;
    while k * HOP < out_frames + HOP {
        let nominal = lead as f64 + (k * HOP) as f64 * speed;
        let start = match prev {
            None => lead,
            Some(p) => {
                // The previous frame's second half is what this frame's first half must match.
                let r0 = p + HOP;
                let centre = nominal.round() as isize;
                let lo = (centre - SEARCH as isize).max(DECIM as isize) as usize;
                let hi = (centre + SEARCH as isize) as usize;
                let reference: Vec<f32> = mono[r0..r0 + HOP]
                    .chunks(DECIM)
                    .map(|c| c.iter().sum::<f32>() / DECIM as f32)
                    .collect();
                let mut best = (f64::MIN, lo);
                let mut s = lo - lo % DECIM;
                while s <= hi {
                    let c0 = s / DECIM;
                    let sc = score(&reference, &coarse[c0..c0 + HOP / DECIM]);
                    if sc > best.0 {
                        best = (sc, s);
                    }
                    s += DECIM;
                }
                // Refine at full rate around the coarse winner.
                let reference = &mono[r0..r0 + HOP];
                let mut fine = (f64::MIN, best.1);
                let from = best.1.saturating_sub(DECIM).max(DECIM);
                for s in from..=best.1 + DECIM {
                    let sc = score(reference, &mono[s..s + HOP]);
                    if sc > fine.0 {
                        fine = (sc, s);
                    }
                }
                fine.1
            }
        };
        let o = k * HOP;
        for j in 0..WIN {
            // The first frame has no earlier frame to cross-fade with: keep its rising half flat.
            let w = if k == 0 && j < HOP { 1.0 } else { win[j] };
            for c in 0..channels {
                buf[(o + j) * channels + c] += w * pad[(start + j) * channels + c];
            }
        }
        prev = Some(start);
        k += 1;
    }
    out.copy_from_slice(&buf[..out_frames * channels]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: usize = 48_000;

    fn tone(freqs: &[(f64, f64)], secs: f64) -> Vec<f32> {
        (0..(secs * RATE as f64) as usize)
            .map(|i| {
                freqs
                    .iter()
                    .map(|(f, a)| {
                        a * (2.0 * std::f64::consts::PI * f * i as f64 / RATE as f64).sin()
                    })
                    .sum::<f64>() as f32
            })
            .collect()
    }

    /// Upward zero crossings per second over `x`.
    fn crossing_freq(x: &[f32]) -> f64 {
        let n = x.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count();
        n as f64 / (x.len() as f64 / RATE as f64)
    }

    /// Fundamental from the autocorrelation peak between 60 and 500 Hz.
    fn autocorr_freq(x: &[f32]) -> f64 {
        let (lo, hi) = (RATE / 500, RATE / 60);
        let mut best = (f64::MIN, lo);
        for lag in lo..hi {
            let c: f64 = x[..x.len() - hi]
                .iter()
                .zip(&x[lag..])
                .map(|(a, b)| f64::from(*a) * f64::from(*b))
                .sum();
            if c > best.0 {
                best = (c, lag);
            }
        }
        RATE as f64 / best.1 as f64
    }

    /// EDT-2.AC2: at 1.5x a 440 Hz tone keeps its pitch within 3% and the length shrinks by 1.5.
    #[test]
    fn edt2_ac2_pitch_is_preserved_at_1_5x() {
        let x = tone(&[(440.0, 0.5)], 3.0);
        let out_frames = (x.len() as f64 / 1.5).round() as usize;
        let y = stretch(&x, 1, 1.5, out_frames);
        assert_eq!(y.len(), out_frames);
        let mid = &y[RATE / 4..y.len() - RATE / 4];
        let f = crossing_freq(mid);
        assert!((f - 440.0).abs() / 440.0 < 0.03, "{f}");
    }

    /// EDT-2.AC2 on a voice-like signal: a 140 Hz fundamental with harmonics at 1.5x and 0.75x.
    #[test]
    fn edt2_ac2_voice_like_fundamental_survives_both_directions() {
        let x = tone(
            &[(140.0, 0.4), (280.0, 0.25), (420.0, 0.15), (560.0, 0.1)],
            4.0,
        );
        for speed in [1.5, 0.75, 2.0] {
            let out_frames = (x.len() as f64 / speed).round() as usize;
            let y = stretch(&x, 1, speed, out_frames);
            let mid = &y[RATE / 2..RATE / 2 + RATE];
            let f = autocorr_freq(mid);
            assert!((f - 140.0).abs() / 140.0 < 0.03, "speed {speed}: {f}");
        }
    }

    #[test]
    fn speed_one_copies_and_pads() {
        let x: Vec<f32> = (0..100).map(|i| i as f32).collect();
        assert_eq!(&stretch(&x, 1, 1.0, 50)[..], &x[..50]);
        let padded = stretch(&x, 1, 1.0, 120);
        assert_eq!(&padded[..100], &x[..]);
        assert!(padded[100..].iter().all(|v| *v == 0.0));
    }

    #[test]
    fn stereo_channels_stay_aligned() {
        let m = tone(&[(300.0, 0.4)], 2.0);
        let x: Vec<f32> = m.iter().flat_map(|v| [*v, -*v]).collect();
        let y = stretch(&x, 2, 1.5, m.len() * 2 / 3);
        assert!(y.chunks(2).all(|f| (f[0] + f[1]).abs() < 1e-6));
        assert!(y.iter().any(|v| v.abs() > 0.1));
    }

    #[test]
    fn empty_and_tiny_inputs_do_not_panic() {
        assert!(stretch(&[], 1, 2.0, 10).iter().all(|v| *v == 0.0));
        assert_eq!(stretch(&[0.5; 10], 1, 2.0, 5).len(), 5);
        assert_eq!(stretch(&[0.5; 10], 1, 0.25, 40).len(), 40);
        assert!(stretch(&[0.5; 10], 1, 8.0, 0).is_empty());
    }
}
