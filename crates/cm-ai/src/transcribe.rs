//! Transcription (engine stream): the `Transcriber` trait, the whisper.cpp implementation
//! ([`crate::whisper::WhisperTranscriber`]) and the stored transcript. Tests and the daemon use the
//! scripted fake [`ScriptedTranscriber`].

use cm_core::{CmError, MediaTime, TICKS_PER_SECOND};
use serde::{Deserialize, Serialize};

/// Seconds to [`MediaTime`], rounded to the nearest tick (no range check: inputs are audio times).
pub fn media_secs(secs: f64) -> MediaTime {
    MediaTime::from_ticks((secs * TICKS_PER_SECOND as f64).round() as i64)
}

/// `transcript.json` format version.
pub const TRANSCRIPT_VERSION: u32 = 1;

/// One recognised word, in **session time** like every edit time. (`Transcriber` itself returns
/// `audio.start` plus the position in the audio; [`crate::recording::transcribe_recording`] passes
/// the track's file time as the start and converts to session time with the pause gaps.)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Word {
    pub text: String,
    pub start: MediaTime,
    pub end: MediaTime,
    /// 0.0 to 1.0.
    pub confidence: f32,
}

/// `transcript.json` in the bundle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Transcript {
    pub version: u32,
    /// Detected or requested language code (`en`, `vi`, ...).
    pub language: String,
    /// The model id that produced it.
    pub model: String,
    pub words: Vec<Word>,
    /// Empty transcript note, e.g. "No speech detected".
    pub note: Option<String>,
}

impl Transcript {
    /// The words joined by spaces (for a reference comparison or a quick look).
    pub fn text(&self) -> String {
        self.words
            .iter()
            .map(|w| w.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Mono 16 kHz f32 audio, the input Whisper wants. Word times come back as `start` plus the
/// position in `samples`.
pub struct Pcm16k<'a> {
    pub samples: &'a [f32],
    pub start: MediaTime,
}

/// Runs a speech model. `progress` is called with 0.0 to 1.0 and may return `false` to cancel,
/// which ends the call with [`cancelled`].
pub trait Transcriber: Send + Sync {
    fn transcribe(
        &self,
        audio: Pcm16k<'_>,
        language: Option<&str>,
        progress: &mut dyn FnMut(f32) -> bool,
    ) -> Result<Transcript, CmError>;

    /// [`Transcriber::transcribe`] with a hint for the model: `prompt` is the text Whisper reads
    /// as what was said just before (the glossary's terms, see [`crate::glossary::initial_prompt`]).
    /// An implementation that cannot use a prompt ignores it.
    fn transcribe_prompted(
        &self,
        audio: Pcm16k<'_>,
        language: Option<&str>,
        prompt: Option<&str>,
        progress: &mut dyn FnMut(f32) -> bool,
    ) -> Result<Transcript, CmError> {
        let _ = prompt;
        self.transcribe(audio, language, progress)
    }

    /// The Whisper *translate* task (AI-5): the same audio, written in English whatever language
    /// is spoken. `source_language` is a hint (`None`: detect). The returned transcript has
    /// `language == "en"`. An implementation without the task answers `not_implemented`.
    /// Models that were not trained for it (`large-v3-turbo`) refuse with `invalid_input`; the
    /// daemon never picks one ([`crate::translate::choose_translation_model`]).
    fn translate_to_english(
        &self,
        audio: Pcm16k<'_>,
        source_language: Option<&str>,
        progress: &mut dyn FnMut(f32) -> bool,
    ) -> Result<Transcript, CmError> {
        let _ = (audio, source_language, progress);
        Err(CmError::not_implemented(
            "this speech engine cannot translate to English",
        ))
    }
}

const CANCELLED_MESSAGE: &str = "Transcription was cancelled";

/// The error a cancelled transcription returns (`user_declined`: nothing failed).
pub fn cancelled() -> CmError {
    CmError::user_declined(CANCELLED_MESSAGE)
}

/// Whether an error is [`cancelled`].
pub fn is_cancelled(e: &CmError) -> bool {
    matches!(e, CmError::UserDeclined { message } if message == CANCELLED_MESSAGE)
}

/// A fake [`Transcriber`] that returns a fixed script, for tests of everything built on the trait
/// (the daemon job, captions, the UI). Word times in the script are offsets from the start of the
/// audio; they are shifted by `audio.start`. Progress is reported in `steps` equal steps.
#[derive(Debug, Clone)]
pub struct ScriptedTranscriber {
    pub language: String,
    pub model: String,
    pub words: Vec<Word>,
    pub steps: u32,
    /// The prompt of every run so far (`None` when a run had none), for tests of the glossary.
    pub prompts: std::sync::Arc<std::sync::Mutex<Vec<Option<String>>>>,
    /// What the translate task returns (offsets from the start of the audio, like `words`);
    /// `None`: the engine cannot translate.
    pub translation: Option<Vec<Word>>,
    /// The source-language hint of every translate run so far, for tests of the engine choice.
    pub translate_runs: std::sync::Arc<std::sync::Mutex<Vec<Option<String>>>>,
}

impl ScriptedTranscriber {
    pub fn new(words: Vec<Word>) -> Self {
        ScriptedTranscriber {
            language: "en".into(),
            model: "scripted".into(),
            words,
            steps: 4,
            prompts: Default::default(),
            translation: None,
            translate_runs: Default::default(),
        }
    }

    /// The script of the translate task, from `(text, start seconds, end seconds)`.
    pub fn with_translation(mut self, words: &[(&str, f64, f64)]) -> Self {
        self.translation = Some(ScriptedTranscriber::from_secs(words).words);
        self
    }

    /// Builds the script from `(text, start seconds, end seconds)`.
    pub fn from_secs(words: &[(&str, f64, f64)]) -> Self {
        ScriptedTranscriber::new(
            words
                .iter()
                .map(|(t, s, e)| Word {
                    text: (*t).to_owned(),
                    start: media_secs(*s),
                    end: media_secs(*e),
                    confidence: 0.9,
                })
                .collect(),
        )
    }
}

impl Transcriber for ScriptedTranscriber {
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
        _language: Option<&str>,
        prompt: Option<&str>,
        progress: &mut dyn FnMut(f32) -> bool,
    ) -> Result<Transcript, CmError> {
        self.prompts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(prompt.map(str::to_owned));
        for i in 1..=self.steps.max(1) {
            if !progress(i as f32 / self.steps.max(1) as f32) {
                return Err(cancelled());
            }
        }
        let words = self
            .words
            .iter()
            .map(|w| Word {
                text: w.text.clone(),
                start: audio.start + w.start,
                end: audio.start + w.end,
                confidence: w.confidence,
            })
            .collect::<Vec<_>>();
        let note = words.is_empty().then(|| "No speech detected".to_owned());
        Ok(Transcript {
            version: TRANSCRIPT_VERSION,
            language: self.language.clone(),
            model: self.model.clone(),
            words,
            note,
        })
    }

    fn translate_to_english(
        &self,
        audio: Pcm16k<'_>,
        source_language: Option<&str>,
        progress: &mut dyn FnMut(f32) -> bool,
    ) -> Result<Transcript, CmError> {
        let script = self.translation.as_ref().ok_or_else(|| {
            CmError::not_implemented("this speech engine cannot translate to English")
        })?;
        self.translate_runs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(source_language.map(str::to_owned));
        for i in 1..=self.steps.max(1) {
            if !progress(i as f32 / self.steps.max(1) as f32) {
                return Err(cancelled());
            }
        }
        let words = script
            .iter()
            .map(|w| Word {
                start: audio.start + w.start,
                end: audio.start + w.end,
                ..w.clone()
            })
            .collect::<Vec<_>>();
        let note = words.is_empty().then(|| "No speech detected".to_owned());
        Ok(Transcript {
            version: TRANSCRIPT_VERSION,
            language: "en".into(),
            model: self.model.clone(),
            words,
            note,
        })
    }
}

/// Word error rate of `hypothesis` against `reference`: word-level edit distance divided by the
/// number of reference words. Case and punctuation are ignored (apostrophes inside words stay).
/// AI-1.AC1 asks for at most 0.12 with the `small` model. An empty reference gives 0 for an empty
/// hypothesis and 1 otherwise.
pub fn word_error_rate(reference: &str, hypothesis: &str) -> f64 {
    let r = normalize_words(reference);
    let h = normalize_words(hypothesis);
    if r.is_empty() {
        return if h.is_empty() { 0.0 } else { 1.0 };
    }
    let mut prev: Vec<usize> = (0..=h.len()).collect();
    for (i, rw) in r.iter().enumerate() {
        let mut cur = vec![i + 1];
        for (j, hw) in h.iter().enumerate() {
            let sub = prev[j] + usize::from(rw != hw);
            cur.push(sub.min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    prev[h.len()] as f64 / r.len() as f64
}

fn normalize_words(s: &str) -> Vec<String> {
    s.split_whitespace()
        .map(|w| {
            w.chars()
                .filter(|c| c.is_alphanumeric() || *c == '\'')
                .flat_map(char::to_lowercase)
                .collect::<String>()
        })
        .filter(|w| !w.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripted_transcriber_shifts_times_and_reports_progress() {
        let t = ScriptedTranscriber::from_secs(&[("hello", 0.5, 0.9), ("world", 1.0, 1.4)]);
        let mut seen = Vec::new();
        let out = t
            .transcribe(
                Pcm16k {
                    samples: &[],
                    start: MediaTime::from_secs(10),
                },
                None,
                &mut |p| {
                    seen.push(p);
                    true
                },
            )
            .unwrap();
        assert_eq!(out.words[0].start, media_secs(10.5));
        assert_eq!(out.text(), "hello world");
        assert_eq!(seen, vec![0.25, 0.5, 0.75, 1.0]);
        assert!(out.note.is_none());
    }

    #[test]
    fn scripted_transcriber_cancels_and_notes_silence() {
        let t = ScriptedTranscriber::from_secs(&[("a", 0.0, 0.1)]);
        let e = t
            .transcribe(
                Pcm16k {
                    samples: &[],
                    start: MediaTime::ZERO,
                },
                None,
                &mut |_| false,
            )
            .unwrap_err();
        assert!(is_cancelled(&e));
        assert!(!is_cancelled(&CmError::user_declined("no")));
        let silent = ScriptedTranscriber::new(Vec::new())
            .transcribe(
                Pcm16k {
                    samples: &[],
                    start: MediaTime::ZERO,
                },
                None,
                &mut |_| true,
            )
            .unwrap();
        assert_eq!(silent.note.as_deref(), Some("No speech detected"));
    }

    #[test]
    fn word_error_rate_counts_edits_and_ignores_case_and_punctuation() {
        assert_eq!(word_error_rate("The cat sat.", "the CAT sat"), 0.0);
        assert!((word_error_rate("a b c d", "a x c") - 0.5).abs() < 1e-9);
        assert_eq!(word_error_rate("a b", ""), 1.0);
        assert_eq!(word_error_rate("", ""), 0.0);
        assert_eq!(word_error_rate("", "x"), 1.0);
        assert!((word_error_rate("don't stop", "dont stop") - 0.5).abs() < 1e-9);
    }
}
