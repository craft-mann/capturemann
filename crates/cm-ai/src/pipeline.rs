//! Decoder options that sit around the engine (0.2.7 transcription review): loudness lift of quiet
//! input, the voice-activity opinion, and the hallucination gate's switch. [`Configured`] wraps any
//! [`Transcriber`]; leaving it off is leaving the wrapper out, so every behaviour here has an off
//! switch and the engine itself is unchanged.

use crate::measure::levels;
use crate::pieces::gate::GateConfig;
use crate::silero::{SileroVad, FRAME};
use crate::transcribe::{Pcm16k, Transcriber, Transcript};
use cm_core::CmError;

/// How quiet input is lifted before it is decoded.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Normalize {
    /// Audio at or above this RMS level (dBFS) is left alone.
    pub below_db: f64,
    /// The level aimed at (dBFS RMS).
    pub target_db: f64,
    /// The most gain applied (dB).
    pub max_gain_db: f64,
    /// A first-order high-pass at this frequency before the lift (Hz), if any.
    pub high_pass_hz: Option<f32>,
}

impl Default for Normalize {
    fn default() -> Self {
        Normalize {
            below_db: -26.0,
            target_db: -20.0,
            max_gain_db: 15.0,
            high_pass_hz: None,
        }
    }
}

/// A decoder with the options of the review around it.
pub struct Configured<'a> {
    pub inner: &'a dyn Transcriber,
    pub vad: Option<&'a SileroVad>,
    pub gate: Option<GateConfig>,
    pub normalize: Option<Normalize>,
}

impl Configured<'_> {
    /// The audio as it is decoded: lifted when it is quiet, per call (a piece, or a re-read).
    fn prepared(&self, samples: &[f32]) -> Option<Vec<f32>> {
        let n = self.normalize?;
        let mut v = samples.to_vec();
        if let Some(hz) = n.high_pass_hz {
            levels::high_pass(&mut v, hz);
        }
        if levels::db(levels::rms_over(&v, &[])) < n.below_db {
            levels::normalize_rms_capped(&mut v, n.target_db, n.max_gain_db);
        }
        Some(v)
    }
}

impl Transcriber for Configured<'_> {
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
        match self.prepared(audio.samples) {
            Some(v) => self.inner.transcribe_prompted(
                Pcm16k {
                    samples: &v,
                    start: audio.start,
                },
                language,
                prompt,
                progress,
            ),
            None => self
                .inner
                .transcribe_prompted(audio, language, prompt, progress),
        }
    }

    fn translate_to_english(
        &self,
        audio: Pcm16k<'_>,
        source_language: Option<&str>,
        progress: &mut dyn FnMut(f32) -> bool,
    ) -> Result<Transcript, CmError> {
        self.inner
            .translate_to_english(audio, source_language, progress)
    }

    fn voice_activity(&self, samples: &[f32]) -> Option<(Vec<f32>, f64)> {
        let vad = self.vad?;
        vad.probabilities(samples)
            .ok()
            .map(|p| (p, FRAME as f64 / 16_000.0))
    }

    fn gate(&self) -> Option<GateConfig> {
        self.gate
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcribe::ScriptedTranscriber;
    use cm_core::MediaTime;
    use std::sync::Mutex;

    /// Remembers the level of the audio it is given.
    struct Level(Mutex<Vec<f64>>);

    impl Transcriber for Level {
        fn transcribe(
            &self,
            audio: Pcm16k<'_>,
            _l: Option<&str>,
            _p: &mut dyn FnMut(f32) -> bool,
        ) -> Result<Transcript, CmError> {
            self.0
                .lock()
                .unwrap()
                .push(levels::db(levels::rms_over(audio.samples, &[])));
            Ok(ScriptedTranscriber::new(Vec::new()).transcribe(audio, None, &mut |_| true)?)
        }
    }

    fn tone(db: f64, secs: usize) -> Vec<f32> {
        let a = 10f64.powf(db / 20.0) * 2f64.sqrt();
        (0..secs * 16_000)
            .map(|i| (a * (f64::from(i as u32) * 0.05).sin()) as f32)
            .collect()
    }

    #[test]
    fn quiet_audio_is_lifted_with_a_cap_and_loud_audio_is_left_alone() {
        let engine = Level(Mutex::new(Vec::new()));
        let c = Configured {
            inner: &engine,
            vad: None,
            gate: None,
            normalize: Some(Normalize::default()),
        };
        for db in [-50.0, -30.0, -15.0] {
            let x = tone(db, 2);
            c.transcribe(
                Pcm16k {
                    samples: &x,
                    start: MediaTime::ZERO,
                },
                None,
                &mut |_| true,
            )
            .unwrap();
        }
        let seen = engine.0.lock().unwrap().clone();
        assert!(
            (seen[0] - -35.0).abs() < 0.5,
            "-50 lifted by the 15 dB cap: {seen:?}"
        );
        assert!(
            (seen[1] - -20.0).abs() < 0.5,
            "-30 lifted to the target: {seen:?}"
        );
        assert!((seen[2] - -15.0).abs() < 0.5, "-15 left alone: {seen:?}");
        // Off: the wrapper without the option changes nothing.
        let engine = Level(Mutex::new(Vec::new()));
        let off = Configured {
            inner: &engine,
            vad: None,
            gate: None,
            normalize: None,
        };
        let x = tone(-50.0, 2);
        off.transcribe(
            Pcm16k {
                samples: &x,
                start: MediaTime::ZERO,
            },
            None,
            &mut |_| true,
        )
        .unwrap();
        assert!((engine.0.lock().unwrap()[0] - -50.0).abs() < 0.5);
    }
}
