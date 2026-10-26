//! Finding where the speech starts and ends in a recording's audio.
//!
//! Whisper's default word times are unreliable around silence: a clip that is silent for the first
//! two seconds still gets its first words at 0.0, so captions appear before anyone speaks and, with
//! the recording's in-point at the first moment every track is live, the first lines fall before
//! the in-point and are lost. The model's DTW times fix most of it (see `whisper.rs`); this module
//! gives the one thing the model cannot know, where the sound actually begins, so no word starts
//! before it. (Cutting the silence off the audio instead changes what the model recognises.) Pure
//! and display-free.

use crate::audio::SAMPLE_RATE;

/// One analysis frame: 20 ms.
const FRAME: usize = (SAMPLE_RATE as usize) / 50;
/// A dip shorter than this (in frames, 100 ms) does not end a burst of speech.
const MAX_DIP_FRAMES: usize = 5;
/// A burst shorter than this (in frames, 160 ms) is a click or a breath, not speech.
const MIN_SPEECH_FRAMES: usize = 8;

/// The sample range `[start, end)` from the first to the last speech, or `None` when no speech
/// was found (a silent or very short recording).
pub fn speech_bounds(samples: &[f32]) -> Option<(usize, usize)> {
    let frames: Vec<f32> = samples
        .as_chunks::<FRAME>()
        .0
        .iter()
        .map(|c| (c.iter().map(|s| s * s).sum::<f32>() / FRAME as f32).sqrt())
        .collect();
    if frames.len() < MIN_SPEECH_FRAMES {
        return None;
    }
    let peak = frames.iter().copied().fold(0.0f32, f32::max);
    let mut sorted = frames.clone();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let floor = sorted[sorted.len() / 10];
    // Well above the room, and not far below the loudest part (-34 dB); never below -80 dBFS.
    let threshold = (floor * 4.0).max(peak * 0.02).max(1e-4);
    if peak <= threshold {
        return None;
    }
    let mut first: Option<usize> = None;
    let mut last = 0usize;
    let mut run_start: Option<usize> = None;
    let mut run_end = 0usize;
    let close = |s: usize, e: usize, first: &mut Option<usize>, last: &mut usize| {
        if e - s >= MIN_SPEECH_FRAMES {
            first.get_or_insert(s);
            *last = e;
        }
    };
    for (i, &r) in frames.iter().enumerate() {
        if r > threshold {
            match run_start {
                Some(_) if i - run_end <= MAX_DIP_FRAMES + 1 => {}
                Some(s) => {
                    close(s, run_end, &mut first, &mut last);
                    run_start = Some(i);
                }
                None => run_start = Some(i),
            }
            run_end = i + 1;
        }
    }
    if let Some(s) = run_start {
        close(s, run_end, &mut first, &mut last);
    }
    let first = first?;
    Some((first * FRAME, (last * FRAME).min(samples.len())))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(secs: f64, amp: f32) -> Vec<f32> {
        (0..(secs * f64::from(SAMPLE_RATE)) as usize)
            .map(|i| amp * (i as f32 * 0.31).sin())
            .collect()
    }

    fn silence(secs: f64) -> Vec<f32> {
        vec![0.0; (secs * f64::from(SAMPLE_RATE)) as usize]
    }

    fn secs(n: usize) -> f64 {
        n as f64 / f64::from(SAMPLE_RATE)
    }

    #[test]
    fn leading_and_trailing_silence_are_left_out() {
        let mut a = silence(2.0);
        a.extend(tone(3.0, 0.2));
        a.extend(silence(1.5));
        let (s, e) = speech_bounds(&a).unwrap();
        assert!((secs(s) - 2.0).abs() < 0.05, "{}", secs(s));
        assert!((secs(e) - 5.0).abs() < 0.05, "{}", secs(e));
    }

    #[test]
    fn a_click_before_the_speech_does_not_count_as_speech() {
        let mut a = silence(0.05);
        a.extend(tone(0.06, 0.5)); // a button click at the start of the recording
        a.extend(silence(1.8));
        a.extend(tone(2.0, 0.2));
        let (s, _) = speech_bounds(&a).unwrap();
        assert!(secs(s) > 1.5, "{}", secs(s));
    }

    #[test]
    fn short_pauses_inside_the_speech_keep_it_one_span() {
        let mut a = silence(1.0);
        a.extend(tone(1.0, 0.2));
        a.extend(silence(0.08));
        a.extend(tone(1.0, 0.2));
        a.extend(silence(2.0));
        a.extend(tone(1.0, 0.2));
        let (s, e) = speech_bounds(&a).unwrap();
        assert!((secs(s) - 1.0).abs() < 0.05);
        // The long silence in the middle stays inside: the span is first to last speech.
        assert!((secs(e) - secs(a.len())).abs() < 0.05, "{}", secs(e));
    }

    #[test]
    fn silence_and_tiny_clips_have_no_span() {
        assert_eq!(speech_bounds(&silence(3.0)), None);
        assert_eq!(speech_bounds(&tone(0.05, 0.3)), None);
        assert_eq!(speech_bounds(&[]), None);
    }

    #[test]
    fn speech_from_the_first_sample_starts_at_zero() {
        let mut a = tone(2.0, 0.2);
        a.extend(silence(1.0));
        let (s, _) = speech_bounds(&a).unwrap();
        assert_eq!(s, 0);
    }
}
