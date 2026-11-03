//! The hallucination gate (0.2.7 transcription review, step 4): words that the decoder wrote over
//! audio that does not hold them are marked, from signals only. It marks; the piece repair
//! ([`super::Job::repair`]) reads the marked stretch again on its own without the prompt, and the
//! re-read goes through the same gate.
//!
//! Signals, strongest first, each read on its own (a decoder that is sure of invented text has a
//! good log-probability, so no signal is AND-ed with it):
//! 1. the model's own no-speech probability for the segment;
//! 2. a second opinion on the audio under the segment: the speech share by the voice-activity model
//!    is low while the plain energy rule sees speech-like level for a good part of it (text is held
//!    up by sound that is not speech);
//! 3. timing that collapsed: three or more words in a row of at most 20 ms, or more words per
//!    second of speech-like time than speech can hold.
//!
//! No text is compared with any list. Quiet real speech is kept by the second opinion: it needs the
//! voice-activity model to disagree with the energy rule, not just a low level.

use crate::transcribe::{SegmentStat, Word};

/// Thresholds of the gate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GateConfig {
    /// A segment with at least this no-speech probability is marked.
    pub no_speech: f32,
    /// A segment whose voice-activity share is under this, with an energy share of at least
    /// `energy_share` and a span of at least `min_secs`, is marked.
    pub vad_share_low: f64,
    pub energy_share: f64,
    pub min_secs: f64,
    /// Words at most this long (milliseconds), three in a row, are a collapse.
    pub collapse_ms: f64,
    /// More words than this per second of the segment are a collapse.
    pub max_words_per_sec: f64,
    /// Which signals are in use (each can be switched off; all on by default).
    pub use_no_speech: bool,
    pub use_vad: bool,
    pub use_collapse: bool,
    pub use_rate: bool,
}

impl Default for GateConfig {
    fn default() -> Self {
        GateConfig {
            no_speech: 0.6,
            vad_share_low: 0.1,
            energy_share: 0.3,
            min_secs: 2.0,
            collapse_ms: 20.0,
            max_words_per_sec: 7.0,
            use_no_speech: true,
            use_vad: true,
            use_collapse: true,
            use_rate: true,
        }
    }
}

/// What the gate knows about the audio, in the seconds of the piece's audio.
pub struct Evidence<'a> {
    /// Speech probabilities per frame (frame length in seconds below) from the voice-activity
    /// model, from the start of the piece.
    pub vad: Option<(&'a [f32], f64)>,
    /// Threshold for a frame to count as speech.
    pub vad_threshold: f32,
    /// Speech-like stretches by the energy rule ([`crate::vad::speech_regions`]), in seconds.
    pub energy: &'a [(f64, f64)],
    /// Where the piece starts, in the seconds the word times use.
    pub origin: f64,
}

fn share_energy(regions: &[(f64, f64)], a: f64, b: f64) -> f64 {
    let inside: f64 = regions
        .iter()
        .map(|&(x, y)| (y.min(b) - x.max(a)).max(0.0))
        .sum();
    inside / (b - a).max(1e-6)
}

fn share_vad(probs: &[f32], frame: f64, thr: f32, a: f64, b: f64) -> f64 {
    let (fa, fb) = (
        (a / frame) as usize,
        ((b / frame).ceil() as usize).min(probs.len()),
    );
    if fb <= fa {
        return 0.0;
    }
    probs[fa..fb].iter().filter(|p| **p >= thr).count() as f64 / (fb - fa) as f64
}

/// For each word, whether it is marked. `times` gives a word's `(start, end)` in seconds of the
/// word times; `segments` are the decoder's segments for the same audio, in the same time.
pub fn mark(
    words: &[Word],
    times: &dyn Fn(&Word) -> (f64, f64),
    segments: &[SegmentStat],
    seg_secs: &dyn Fn(&SegmentStat) -> (f64, f64),
    ev: &Evidence<'_>,
    cfg: &GateConfig,
) -> Vec<bool> {
    let mut out = vec![false; words.len()];
    // Timing collapse: runs of very short words.
    let mut run = 0usize;
    for (i, w) in words.iter().enumerate() {
        let (a, b) = times(w);
        if cfg.use_collapse && (b - a) * 1000.0 <= cfg.collapse_ms {
            run += 1;
            if run >= 3 {
                out[i + 1 - run..=i].iter_mut().for_each(|m| *m = true);
            }
        } else {
            run = 0;
        }
    }
    for seg in segments {
        let (sa, sb) = seg_secs(seg);
        let inside: Vec<usize> = words
            .iter()
            .enumerate()
            .filter(|(_, w)| {
                let (a, _) = times(w);
                a >= sa - 0.05 && a < sb + 0.05
            })
            .map(|(i, _)| i)
            .collect();
        if inside.is_empty() {
            continue;
        }
        let secs = (sb - sa).max(1e-6);
        let mut marked = cfg.use_no_speech && seg.no_speech >= cfg.no_speech;
        // Words per second of speech-like time under the segment.
        // (When the energy rule found nothing to go by, the whole segment counts.)
        let speech_secs = if ev.energy.is_empty() {
            secs
        } else {
            (share_energy(ev.energy, sa - ev.origin, sb - ev.origin) * secs).max(0.25)
        };
        if cfg.use_rate
            && inside.len() as f64 / speech_secs > cfg.max_words_per_sec
            && inside.len() >= 4
        {
            marked = true;
        }
        if cfg.use_vad && !marked && secs >= cfg.min_secs && inside.len() >= 2 {
            if let Some((probs, frame)) = ev.vad {
                let vad = share_vad(
                    probs,
                    frame,
                    ev.vad_threshold,
                    sa - ev.origin,
                    sb - ev.origin,
                );
                let energy = share_energy(ev.energy, sa - ev.origin, sb - ev.origin);
                marked = vad < cfg.vad_share_low && energy >= cfg.energy_share;
            }
        }
        if marked {
            inside.iter().for_each(|&i| out[i] = true);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcribe::media_secs;

    fn w(text: &str, a: f64, d: f64) -> Word {
        Word {
            text: text.into(),
            start: media_secs(a),
            end: media_secs(a + d),
            confidence: 0.95,
        }
    }

    fn seg(a: f64, b: f64, ns: f32) -> SegmentStat {
        SegmentStat {
            start: media_secs(a),
            end: media_secs(b),
            no_speech: ns,
            avg_logprob: -0.2,
        }
    }

    fn run(words: &[Word], segs: &[SegmentStat], ev: &Evidence<'_>) -> Vec<bool> {
        let t = |w: &Word| (w.start.as_secs_f64(), w.end.as_secs_f64());
        let s = |s: &SegmentStat| (s.start.as_secs_f64(), s.end.as_secs_f64());
        mark(words, &t, segs, &s, ev, &GateConfig::default())
    }

    #[test]
    fn a_segment_the_model_calls_not_speech_is_marked_on_its_own() {
        let words = [w("aa", 1.0, 0.4), w("bb", 1.5, 0.4)];
        let ev = Evidence {
            vad: None,
            vad_threshold: 0.3,
            energy: &[(0.0, 10.0)],
            origin: 0.0,
        };
        assert_eq!(run(&words, &[seg(0.9, 2.0, 0.8)], &ev), [true, true]);
        assert_eq!(run(&words, &[seg(0.9, 2.0, 0.1)], &ev), [false, false]);
    }

    #[test]
    fn energy_that_the_voice_model_does_not_hear_marks_a_long_segment_but_quiet_speech_stays() {
        // 8 s of speech-like level; the voice model finds 3 % speech: the 94 s signature, in shape.
        let words: Vec<Word> = (0..6)
            .map(|i| w("xx", 1.0 + f64::from(i) * 1.3, 0.3))
            .collect();
        let silent = vec![0.02f32; 400];
        let speaking = vec![0.9f32; 400];
        let energy = [(0.0, 12.0)];
        let ev = Evidence {
            vad: Some((&silent, 0.032)),
            vad_threshold: 0.3,
            energy: &energy,
            origin: 0.0,
        };
        let segs = [seg(0.9, 9.5, 0.05)];
        assert!(run(&words, &segs, &ev).iter().all(|m| *m));
        // The voice model hears speech: kept. The energy rule does not see it either: kept.
        let ev2 = Evidence {
            vad: Some((&speaking, 0.032)),
            vad_threshold: 0.3,
            energy: &energy,
            origin: 0.0,
        };
        assert!(run(&words, &segs, &ev2).iter().all(|m| !*m));
        let ev3 = Evidence {
            vad: Some((&silent, 0.032)),
            vad_threshold: 0.3,
            energy: &[],
            origin: 0.0,
        };
        assert!(run(&words, &segs, &ev3).iter().all(|m| !*m));
    }

    #[test]
    fn collapsed_timing_is_marked_whatever_the_text() {
        let words = [
            w("a", 1.0, 0.4),
            w("b", 2.0, 0.01),
            w("c", 2.01, 0.01),
            w("d", 2.02, 0.01),
            w("e", 3.0, 0.4),
        ];
        let ev = Evidence {
            vad: None,
            vad_threshold: 0.3,
            energy: &[(0.0, 10.0)],
            origin: 0.0,
        };
        assert_eq!(run(&words, &[], &ev), [false, true, true, true, false]);
        // More words than speech can hold in a second.
        let many: Vec<Word> = (0..12)
            .map(|i| w("q", 1.0 + f64::from(i) * 0.08, 0.07))
            .collect();
        assert!(run(&many, &[seg(1.0, 2.0, 0.0)], &ev).iter().all(|m| *m));
    }
}
