//! Offline look-ahead limiter and the true-peak ceiling loop (AUD-4: limiter at -1 dBTP).

use super::loudness::true_peak_db;

/// Gain reduction in a symmetric window of `look` samples either side: the running minimum of
/// the needed gain, then a box average of the same width. Every averaged value is at most the
/// gain any sample inside the window needs, so no sample ends above the ceiling.
fn limit_once(samples: &mut [f32], channels: usize, rate: u32, ceiling: f64) {
    let frames = samples.len() / channels;
    if frames == 0 {
        return;
    }
    let look = ((f64::from(rate) * 0.005) as usize).max(1);
    let need: Vec<f64> = (0..frames)
        .map(|i| {
            let peak = samples[i * channels..(i + 1) * channels]
                .iter()
                .fold(0f64, |m, v| m.max(f64::from(v.abs())));
            if peak > ceiling {
                ceiling / peak
            } else {
                1.0
            }
        })
        .collect();
    if need.iter().all(|&g| g >= 1.0) {
        return;
    }
    // Sliding minimum over [i - look, i + look] with a monotonic deque.
    let mut mins = vec![1.0f64; frames];
    let mut dq: std::collections::VecDeque<usize> = std::collections::VecDeque::new();
    let mut next = 0usize;
    for (i, m) in mins.iter_mut().enumerate() {
        let hi = (i + look).min(frames - 1);
        while next <= hi {
            while dq.back().is_some_and(|&b| need[b] >= need[next]) {
                dq.pop_back();
            }
            dq.push_back(next);
            next += 1;
        }
        while dq.front().is_some_and(|&f| f + look < i) {
            dq.pop_front();
        }
        *m = need[*dq.front().expect("window is never empty")];
    }
    // Box average of the same window (prefix sums; outside the signal counts as 1.0).
    let mut prefix = Vec::with_capacity(frames + 1);
    prefix.push(0.0);
    for m in &mins {
        prefix.push(prefix.last().expect("seeded") + m);
    }
    let width = (2 * look + 1) as f64;
    for i in 0..frames {
        let lo = i.saturating_sub(look);
        let hi = (i + look + 1).min(frames);
        let outside = (2 * look + 1) - (hi - lo);
        let g = (prefix[hi] - prefix[lo] + outside as f64) / width;
        for s in &mut samples[i * channels..(i + 1) * channels] {
            *s = (f64::from(*s) * g) as f32;
        }
    }
}

/// Limits `samples` (interleaved) so the true peak is at most `ceiling_dbtp`. Sample-peak limiting
/// is repeated with a lower ceiling until the oversampled measurement agrees (at most 6 passes).
pub fn limit_true_peak(samples: &mut [f32], channels: usize, rate: u32, ceiling_dbtp: f64) {
    let channels = channels.max(1);
    let mut ceiling_db = ceiling_dbtp;
    for _ in 0..6 {
        limit_once(samples, channels, rate, 10f64.powf(ceiling_db / 20.0));
        let tp = true_peak_db(samples, channels);
        if tp <= ceiling_dbtp + 1e-3 {
            return;
        }
        ceiling_db -= (tp - ceiling_dbtp) + 0.05;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(i: usize, rate: u32, amp: f64) -> f32 {
        (amp * (2.0 * std::f64::consts::PI * 997.0 * i as f64 / f64::from(rate)).sin()) as f32
    }

    /// AUD-4.AC2: a burst 6 dB over the ceiling ends at or below -1 dBTP, quiet parts untouched.
    #[test]
    fn aud4_ac2_limiter_holds_the_true_peak_ceiling() {
        let rate = 48_000u32;
        let burst = rate as usize / 2..rate as usize;
        let mut x: Vec<f32> = (0..rate as usize * 2)
            .map(|i| tone(i, rate, if burst.contains(&i) { 2.0 } else { 0.1 }))
            .collect();
        limit_true_peak(&mut x, 1, rate, -1.0);
        assert!(true_peak_db(&x, 1) <= -1.0 + 1e-3);
        let far = rate as usize * 3 / 2 + 7;
        assert!((x[far] - tone(far, rate, 0.1)).abs() < 1e-4);
    }

    #[test]
    fn a_quiet_signal_is_not_touched() {
        let mut x = vec![0.1f32, -0.2, 0.3, -0.1];
        let before = x.clone();
        limit_true_peak(&mut x, 1, 48_000, -1.0);
        assert_eq!(x, before);
    }
}
