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
    /// The largest sample is never lifted past this level (dBFS).
    pub peak_limit_db: f64,
    /// Audio is lifted only when its largest sample has at least this much room (dB) below the
    /// peak limit: a signal that already reaches near full scale is loud, not quiet, however low
    /// its average is (a meeting with sparse loud voices), and a small lift only perturbs the decode.
    pub min_headroom_db: f64,
    /// A first-order high-pass at this frequency before the lift (Hz), if any.
    pub high_pass_hz: Option<f32>,
}

impl Default for Normalize {
    fn default() -> Self {
        Normalize {
            below_db: -26.0,
            target_db: -20.0,
            max_gain_db: 15.0,
            peak_limit_db: -1.0,
            min_headroom_db: 12.0,
            high_pass_hz: None,
        }
    }
}

/// Setting keys of the decoder options (all in `settings.json`, all with an off switch).
pub const KEY_LIFT_QUIET_INPUT: &str = "ai.lift_quiet_input";
pub const KEY_GATE: &str = "ai.hallucination_gate";
pub const KEY_VAD_OPINION: &str = "ai.vad_opinion";
pub const KEY_CARRY_TEXT: &str = "ai.carry_previous_text";
/// Read the microphone as recorded (on, the default since 0.3.1) or the noise-reduced copy (off),
/// unless the request names its audio. A library with no stored value reads the original
/// microphone; a stored value is kept as it is (no migration).
pub const KEY_RAW_MIC: &str = "ai.transcribe_raw_mic";
/// Words the decoder stacked on one instant are spread over the room around them.
pub const KEY_REPAIR_WORD_TIMES: &str = "ai.repair_word_times";
/// Windows read again (loops, broken words, speech with no words) are decoded with beam search.
pub const KEY_CAREFUL_REREADS: &str = "ai.careful_rereads";

/// Which of the review's behaviours are on for a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PipelineSettings {
    /// Quiet input is lifted (to -20 dBFS RMS, at most 15 dB) before decoding.
    pub lift_quiet_input: bool,
    /// The hallucination gate marks words the audio does not hold; they are read again.
    pub gate: bool,
    /// The voice-activity model, when it is installed, is the gate's second opinion.
    pub vad_opinion: bool,
    /// A window carries the text of the one before it (whisper.cpp's default). Off: no carry-over
    /// (`n_max_text_ctx` 0), which also leaves the glossary prompt out of the decode (the glossary's
    /// spellings are still applied to the words afterwards).
    pub carry_previous_text: bool,
    /// Transcription reads the original microphone track (the default; the noise-reduced copy
    /// only when this is off or the request asks for it). A request names its audio explicitly
    /// with `original_audio`.
    pub transcribe_raw_mic: bool,
    /// Stacks of words with collapsed times are spread (see [`crate::wordtimes`]).
    pub repair_word_times: bool,
    /// Windows read again by the pipeline use beam search (5 beams, no temperature fallback); the
    /// first reading is unchanged.
    pub careful_rereads: bool,
}

impl Default for PipelineSettings {
    fn default() -> Self {
        PipelineSettings {
            lift_quiet_input: true,
            gate: false,
            vad_opinion: false,
            carry_previous_text: true,
            transcribe_raw_mic: true,
            repair_word_times: true,
            careful_rereads: false,
        }
    }
}

impl PipelineSettings {
    /// The user's choices from the settings map; anything unset is the default.
    pub fn from_settings(m: &serde_json::Map<String, serde_json::Value>) -> Self {
        let d = Self::default();
        let b = |k: &str, dflt: bool| {
            m.get(k)
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(dflt)
        };
        PipelineSettings {
            lift_quiet_input: b(KEY_LIFT_QUIET_INPUT, d.lift_quiet_input),
            gate: b(KEY_GATE, d.gate),
            vad_opinion: b(KEY_VAD_OPINION, d.vad_opinion),
            carry_previous_text: b(KEY_CARRY_TEXT, d.carry_previous_text),
            transcribe_raw_mic: b(KEY_RAW_MIC, d.transcribe_raw_mic),
            repair_word_times: b(KEY_REPAIR_WORD_TIMES, d.repair_word_times),
            careful_rereads: b(KEY_CAREFUL_REREADS, d.careful_rereads),
        }
    }

    /// Everything as in 0.2.6, quiet-input lift included (0.2.6 read the noise-reduced copy).
    pub fn all_off() -> Self {
        PipelineSettings {
            lift_quiet_input: false,
            gate: false,
            vad_opinion: false,
            carry_previous_text: true,
            transcribe_raw_mic: false,
            repair_word_times: false,
            careful_rereads: false,
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
        let peak = v.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        let headroom = n.peak_limit_db - levels::db(f64::from(peak));
        if levels::db(levels::rms_over(&v, &[])) < n.below_db && headroom >= n.min_headroom_db {
            levels::normalize_rms_capped_peak(&mut v, n.target_db, n.max_gain_db, n.peak_limit_db);
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

    fn transcribe_careful(
        &self,
        audio: Pcm16k<'_>,
        language: Option<&str>,
        prompt: Option<&str>,
        progress: &mut dyn FnMut(f32) -> bool,
    ) -> Result<Transcript, CmError> {
        match self.prepared(audio.samples) {
            Some(v) => self.inner.transcribe_careful(
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
                .transcribe_careful(audio, language, prompt, progress),
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

/// A decoder whose words come back with collapsed times spread (see [`crate::wordtimes`]). Wraps
/// any [`Transcriber`], also [`Configured`]; leaving it out is the off switch.
pub struct RepairTimes<'a> {
    pub inner: &'a dyn Transcriber,
}

impl Transcriber for RepairTimes<'_> {
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
        let end = audio.start
            + cm_core::MediaTime::from_ticks(
                audio.samples.len() as i64 * cm_core::TICKS_PER_SECOND / 16_000,
            );
        let mut t = self
            .inner
            .transcribe_prompted(audio, language, prompt, progress)?;
        crate::wordtimes::repair(&mut t.words, Some(end));
        Ok(t)
    }

    fn transcribe_careful(
        &self,
        audio: Pcm16k<'_>,
        language: Option<&str>,
        prompt: Option<&str>,
        progress: &mut dyn FnMut(f32) -> bool,
    ) -> Result<Transcript, CmError> {
        let end = audio.start
            + cm_core::MediaTime::from_ticks(
                audio.samples.len() as i64 * cm_core::TICKS_PER_SECOND / 16_000,
            );
        let mut t = self
            .inner
            .transcribe_careful(audio, language, prompt, progress)?;
        crate::wordtimes::repair(&mut t.words, Some(end));
        Ok(t)
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
        self.inner.voice_activity(samples)
    }

    fn gate(&self) -> Option<GateConfig> {
        self.inner.gate()
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
            ScriptedTranscriber::new(Vec::new()).transcribe(audio, None, &mut |_| true)
        }
    }

    fn tone(db: f64, secs: usize) -> Vec<f32> {
        let a = 10f64.powf(db / 20.0) * 2f64.sqrt();
        (0..secs * 16_000)
            .map(|i| (a * (f64::from(i as u32) * 0.05).sin()) as f32)
            .collect()
    }

    #[test]
    fn the_settings_default_to_the_stack_and_each_has_an_off_switch() {
        let none = serde_json::Map::new();
        assert_eq!(
            PipelineSettings::from_settings(&none),
            PipelineSettings::default()
        );
        let mut m = serde_json::Map::new();
        for k in [
            KEY_LIFT_QUIET_INPUT,
            KEY_GATE,
            KEY_VAD_OPINION,
            KEY_RAW_MIC,
            KEY_REPAIR_WORD_TIMES,
            KEY_CAREFUL_REREADS,
        ] {
            m.insert(k.into(), serde_json::Value::Bool(false));
        }
        m.insert(KEY_CARRY_TEXT.into(), serde_json::Value::Bool(true));
        assert_eq!(
            PipelineSettings::from_settings(&m),
            PipelineSettings::all_off()
        );
        // No stored key: the original microphone is read (no migration is needed for a library
        // that never stored the key). A stored `false` keeps the noise-reduced copy; a stored
        // `true` is kept too.
        assert!(PipelineSettings::from_settings(&none).transcribe_raw_mic);
        let mut off = serde_json::Map::new();
        off.insert(KEY_RAW_MIC.into(), serde_json::Value::Bool(false));
        assert!(!PipelineSettings::from_settings(&off).transcribe_raw_mic);
        let mut on = serde_json::Map::new();
        on.insert(KEY_RAW_MIC.into(), serde_json::Value::Bool(true));
        assert!(PipelineSettings::from_settings(&on).transcribe_raw_mic);
    }

    #[test]
    fn stacked_word_times_are_spread_by_the_wrapper_and_never_past_the_audio() {
        let w = |t: &str, a: f64| crate::transcribe::Word {
            text: t.into(),
            start: MediaTime::from_secs_f64(a).unwrap(),
            end: MediaTime::from_secs_f64(a).unwrap(),
            confidence: 0.9,
        };
        let stacked = vec![w("zed", 1.9), w("nim", 1.9), w("qux", 1.9), w("blorp", 1.9)];
        let inner = ScriptedTranscriber::new(stacked);
        let r = RepairTimes { inner: &inner };
        let x = vec![0.0f32; 32_000]; // 2 s
        let t = r
            .transcribe(
                Pcm16k {
                    samples: &x,
                    start: MediaTime::ZERO,
                },
                None,
                &mut |_| true,
            )
            .unwrap();
        let end = MediaTime::from_secs_f64(2.0).unwrap();
        assert!(
            t.words.iter().all(|w| w.end > w.start && w.end <= end),
            "{:?}",
            t.words
        );
        assert_eq!(t.words.len(), 4);
    }

    #[test]
    fn a_low_rms_signal_with_a_loud_peak_is_not_lifted() {
        let engine = Level(Mutex::new(Vec::new()));
        let c = Configured {
            inner: &engine,
            vad: None,
            gate: None,
            normalize: Some(Normalize::default()),
        };
        // Quiet tone (about -34 dBFS RMS) with one loud click at -4 dBFS: 3 dB of headroom, less than
        // the 12 dB a really quiet recording leaves, so it is left alone.
        let mut x = tone(-34.0, 2);
        x[100] = 10f32.powf(-4.0 / 20.0);
        let before = levels::db(levels::rms_over(&x, &[]));
        c.transcribe(
            Pcm16k {
                samples: &x,
                start: MediaTime::ZERO,
            },
            None,
            &mut |_| true,
        )
        .unwrap();
        let seen = engine.0.lock().unwrap().clone();
        assert!((seen[0] - before).abs() < 0.05, "{seen:?}");
        // A click at -16 dBFS leaves 15 dB: lifted, but never past the peak limit.
        let mut z = tone(-34.0, 2);
        z[100] = 10f32.powf(-16.0 / 20.0);
        let before_z = levels::db(levels::rms_over(&z, &[]));
        c.transcribe(
            Pcm16k {
                samples: &z,
                start: MediaTime::ZERO,
            },
            None,
            &mut |_| true,
        )
        .unwrap();
        let seen = engine.0.lock().unwrap().clone();
        assert!(
            seen[1] - before_z > 5.0 && seen[1] - before_z <= 15.05,
            "{seen:?}"
        );
        // No headroom at all: left alone.
        let mut y = tone(-34.0, 2);
        y[100] = 1.0;
        let before = levels::db(levels::rms_over(&y, &[]));
        c.transcribe(
            Pcm16k {
                samples: &y,
                start: MediaTime::ZERO,
            },
            None,
            &mut |_| true,
        )
        .unwrap();
        let seen = engine.0.lock().unwrap().clone();
        assert!((seen[2] - before).abs() < 0.05, "{seen:?}");
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
