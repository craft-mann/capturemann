//! ITU-R BS.1770-4 / EBU R128 measurement: K-weighted integrated loudness (LUFS, with the
//! absolute and relative gates) and true peak (4x oversampled, dBTP).

/// Floor reported for silence.
pub const SILENCE_DB: f64 = -200.0;

#[derive(Clone, Copy)]
struct Biquad {
    b: [f64; 3],
    a: [f64; 2],
}

impl Biquad {
    fn run(&self, x: &[f64]) -> Vec<f64> {
        let (mut z1, mut z2) = (0.0, 0.0);
        x.iter()
            .map(|&v| {
                let y = self.b[0] * v + z1;
                z1 = self.b[1] * v - self.a[0] * y + z2;
                z2 = self.b[2] * v - self.a[1] * y;
                y
            })
            .collect()
    }
}

/// The two K-weighting stages for any sample rate (the BS.1770 constants, bilinear transform).
fn k_weighting(rate: u32) -> [Biquad; 2] {
    let fs = f64::from(rate);
    let shelf = {
        let (f0, gain_db, q) = (
            1_681.974_450_955_533,
            3.999_843_853_973_347,
            0.707_175_236_955_419_6,
        );
        let k = (std::f64::consts::PI * f0 / fs).tan();
        let vh = 10f64.powf(gain_db / 20.0);
        let vb = vh.powf(0.499_666_774_154_541_6);
        let a0 = 1.0 + k / q + k * k;
        Biquad {
            b: [
                (vh + vb * k / q + k * k) / a0,
                2.0 * (k * k - vh) / a0,
                (vh - vb * k / q + k * k) / a0,
            ],
            a: [2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0],
        }
    };
    let high_pass = {
        let (f0, q) = (38.135_470_876_024_44, 0.500_327_037_323_877_3);
        let k = (std::f64::consts::PI * f0 / fs).tan();
        let a0 = 1.0 + k / q + k * k;
        Biquad {
            b: [1.0, -2.0, 1.0],
            a: [2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0],
        }
    };
    [shelf, high_pass]
}

fn lk(energy: f64) -> f64 {
    if energy <= 0.0 {
        SILENCE_DB
    } else {
        -0.691 + 10.0 * energy.log10()
    }
}

/// Integrated loudness in LUFS of interleaved samples, or `None` when nothing passes the gates
/// (silence, or shorter than one 400 ms block). Every channel has weight 1.0, so measure the
/// layout that will be exported (a mono signal duplicated to stereo reads 3 LU louder).
pub fn integrated_lufs(samples: &[f32], channels: usize, rate: u32) -> Option<f64> {
    let channels = channels.max(1);
    let frames = samples.len() / channels;
    let block = (f64::from(rate) * 0.4).round() as usize;
    let step = (f64::from(rate) * 0.1).round() as usize;
    if frames < block || step == 0 {
        return None;
    }
    let stages = k_weighting(rate);
    // Prefix sums of the squared K-weighted signal, per channel.
    let prefix: Vec<Vec<f64>> = (0..channels)
        .map(|c| {
            let x: Vec<f64> = (0..frames)
                .map(|i| f64::from(samples[i * channels + c]))
                .collect();
            let y = stages[1].run(&stages[0].run(&x));
            let mut p = Vec::with_capacity(frames + 1);
            p.push(0.0);
            let mut acc = 0.0;
            for v in y {
                acc += v * v;
                p.push(acc);
            }
            p
        })
        .collect();
    let mut energies = Vec::new();
    let mut at = 0;
    while at + block <= frames {
        let e: f64 = prefix
            .iter()
            .map(|p| (p[at + block] - p[at]) / block as f64)
            .sum();
        energies.push(e);
        at += step;
    }
    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
    let above_abs: Vec<f64> = energies
        .iter()
        .copied()
        .filter(|&e| lk(e) > -70.0)
        .collect();
    if above_abs.is_empty() {
        return None;
    }
    let relative = lk(mean(&above_abs)) - 10.0;
    let gated: Vec<f64> = above_abs
        .into_iter()
        .filter(|&e| lk(e) > relative)
        .collect();
    if gated.is_empty() {
        return None;
    }
    Some(lk(mean(&gated)))
}

const TAPS: usize = 49;
const CENTER: usize = 24;

/// Windowed-sinc 4x interpolation taps for the three in-between phases: `(offset, weight)`.
fn phase_taps() -> [Vec<(isize, f64)>; 3] {
    let h = |n: usize| -> f64 {
        let x = (n as f64 - CENTER as f64) / 4.0;
        let sinc = if x == 0.0 {
            1.0
        } else {
            (std::f64::consts::PI * x).sin() / (std::f64::consts::PI * x)
        };
        let t = n as f64 / (TAPS - 1) as f64;
        let w = 0.42 - 0.5 * (2.0 * std::f64::consts::PI * t).cos()
            + 0.08 * (4.0 * std::f64::consts::PI * t).cos();
        sinc * w
    };
    let mut out: [Vec<(isize, f64)>; 3] = Default::default();
    for (i, taps) in out.iter_mut().enumerate() {
        let p = (i + 1) as isize;
        for d in -7isize..=7 {
            let n = p + CENTER as isize + 4 * d;
            if (0..TAPS as isize).contains(&n) {
                taps.push((d, h(n as usize)));
            }
        }
    }
    out
}

/// True peak in dBTP of interleaved samples (4x oversampling, BS.1770-4 Annex 2 style).
pub fn true_peak_db(samples: &[f32], channels: usize) -> f64 {
    let channels = channels.max(1);
    let taps = phase_taps();
    let frames = samples.len() / channels;
    let mut peak = 0f64;
    for c in 0..channels {
        let at = |i: isize| -> f64 {
            if i < 0 || i as usize >= frames {
                0.0
            } else {
                f64::from(samples[i as usize * channels + c])
            }
        };
        for i in 0..frames {
            peak = peak.max(at(i as isize).abs());
            for t in &taps {
                let y: f64 = t.iter().map(|&(d, w)| w * at(i as isize - d)).sum();
                peak = peak.max(y.abs());
            }
        }
    }
    if peak <= 0.0 {
        SILENCE_DB
    } else {
        20.0 * peak.log10()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f64, amp: f64, secs: f64, rate: u32, channels: usize) -> Vec<f32> {
        let n = (secs * f64::from(rate)) as usize;
        let mut v = Vec::with_capacity(n * channels);
        for i in 0..n {
            let s = (amp * (2.0 * std::f64::consts::PI * freq * i as f64 / f64::from(rate)).sin())
                as f32;
            for _ in 0..channels {
                v.push(s);
            }
        }
        v
    }

    /// AUD-4.AC2: a 1 kHz sine at -20 dBFS peak on both channels reads -20.0 LUFS (the BS.1770
    /// reference: a stereo sine reads its own peak level).
    #[test]
    fn aud4_ac2_reference_sine_reads_its_peak_level() {
        let x = sine(1000.0, 0.1, 10.0, 48_000, 2);
        let l = integrated_lufs(&x, 2, 48_000).unwrap();
        assert!((l + 20.0).abs() < 0.1, "{l}");
    }

    /// The absolute gate drops digital silence and the relative gate drops a much quieter part.
    #[test]
    fn aud4_gates_ignore_silence_and_quiet_parts() {
        let loud = sine(1000.0, 0.1, 5.0, 48_000, 2);
        let mut with_silence = loud.clone();
        with_silence.extend(vec![0.0; 5 * 48_000 * 2]);
        let a = integrated_lufs(&with_silence, 2, 48_000).unwrap();
        // Blocks straddling the edge are partly loud and pass the gates, as the standard has it.
        assert!((a + 20.0).abs() < 0.2, "{a}");
        let mut with_quiet = loud;
        with_quiet.extend(sine(1000.0, 0.1 * 0.01, 5.0, 48_000, 2)); // -40 dB: below -10 LU
        let b = integrated_lufs(&with_quiet, 2, 48_000).unwrap();
        assert!((b + 20.0).abs() < 0.2, "{b}");
    }

    #[test]
    fn silence_and_short_signals_have_no_loudness() {
        assert_eq!(integrated_lufs(&vec![0.0; 96_000], 2, 48_000), None);
        assert_eq!(
            integrated_lufs(&sine(1000.0, 0.5, 0.2, 48_000, 2), 2, 48_000),
            None
        );
    }

    /// A fs/4 sine at 45 degrees has samples of 0.707 but a true peak of 1.0.
    #[test]
    fn aud4_true_peak_sees_between_the_samples() {
        let n = 4800;
        let x: Vec<f32> = (0..n)
            .map(|i| {
                (std::f64::consts::FRAC_PI_2 * i as f64 + std::f64::consts::FRAC_PI_4).sin() as f32
            })
            .collect();
        let sample_peak = 20.0 * f64::from(x.iter().fold(0f32, |m, v| m.max(v.abs()))).log10();
        assert!((sample_peak + 3.01).abs() < 0.05);
        let tp = true_peak_db(&x, 1);
        assert!(tp > -0.3 && tp < 0.3, "{tp}");
    }
}
