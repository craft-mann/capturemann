//! AI normalisation of a transcript (AI-10): punctuation, capital letters and paragraph breaks
//! from the built-in text model, **without changing a word**.
//!
//! * [`invariant`]: the check. After dropping punctuation and case but keeping tone marks, the
//!   answer for a turn must have exactly the input's words ([`invariant::check_turn`]).
//! * [`turns`]: the units sent to the model (speaker turns; long ones cut at a pause).
//! * [`engine`]: batches, the retry and the fall back to the raw text ([`engine::normalize`]).
//! * [`prompt`]: the instructions, versioned ([`prompt::PROMPT_VERSION`]).
//! * [`score`]: punctuation F1 and capital accuracy against a punctuated reference (benchmark).
//! * this file: the sidecar `transcript.normalized.json` ([`Normalized`]). `transcript.json` is
//!   never touched; the sidecar says which transcript it was made from ([`words_hash`]) so a
//!   reader can tell when it is out of date.

pub mod engine;
pub mod invariant;
pub mod prompt;
pub mod score;
pub mod turns;

use std::path::{Path, PathBuf};

use cm_core::{CmError, MediaTime};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::transcribe::Word;

pub use self::engine::{normalize, NormalizeError, Options, Stats, TurnResult};

/// The sidecar's file name, next to `transcript.json`.
pub const NORMALIZED_FILE: &str = "transcript.normalized.json";
/// Progress of an interrupted run, removed when the run ends.
pub const PARTIAL_FILE: &str = "transcript.normalized.partial.json";
pub const NORMALIZED_VERSION: u32 = 1;
/// Job type of a normalisation run.
pub const JOB_NORMALIZE: &str = "normalize";

/// One turn in the sidecar.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NormalizedTurn {
    pub start: MediaTime,
    pub end: MediaTime,
    #[serde(flatten)]
    pub result: TurnResult,
}

/// The numbers of a finished run.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Summary {
    pub turns: usize,
    pub normalized: usize,
    pub rejected: usize,
    pub words: usize,
    pub rejected_words: usize,
    pub model_calls: usize,
    pub retried: usize,
}

/// `transcript.normalized.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Normalized {
    pub version: u32,
    pub language: String,
    /// The text model id.
    pub model: String,
    pub prompt_version: u32,
    pub created: String,
    /// The transcript this was made from: its word count and [`words_hash`].
    pub words: usize,
    pub words_hash: String,
    pub summary: Summary,
    pub turns: Vec<NormalizedTurn>,
}

/// A hash of the word keys of a transcript: changes when a word is added, removed or respelled
/// (case and punctuation do not count).
pub fn words_hash(words: &[Word]) -> String {
    let mut h = Sha256::new();
    for w in words {
        h.update(invariant::word_key(&w.text).as_bytes());
        h.update(b"\n");
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

impl Normalized {
    /// Builds the sidecar from a finished run.
    pub fn from_run(
        words: &[Word],
        language: &str,
        model: &str,
        created: &str,
        results: Vec<TurnResult>,
        stats: &Stats,
    ) -> Normalized {
        let mut summary = Summary {
            turns: results.len(),
            words: words.len(),
            model_calls: stats.model_calls,
            retried: stats.retried,
            ..Summary::default()
        };
        let turns = results
            .into_iter()
            .map(|r| {
                if r.normalized {
                    summary.normalized += 1;
                } else {
                    summary.rejected += 1;
                    summary.rejected_words += r.len;
                }
                let start = words.get(r.first).map_or(MediaTime::ZERO, |w| w.start);
                let end = words
                    .get(r.first + r.len.saturating_sub(1))
                    .map_or(start, |w| w.end);
                NormalizedTurn {
                    start,
                    end,
                    result: r,
                }
            })
            .collect();
        Normalized {
            version: NORMALIZED_VERSION,
            language: language.to_owned(),
            model: model.to_owned(),
            prompt_version: prompt::PROMPT_VERSION,
            created: created.to_owned(),
            words: words.len(),
            words_hash: words_hash(words),
            summary,
            turns,
        }
    }

    /// Whether the sidecar was made from exactly these words.
    pub fn matches(&self, words: &[Word]) -> bool {
        self.words == words.len() && self.words_hash == words_hash(words)
    }

    /// The text of every turn (paragraph breaks kept), turns separated by a blank line.
    pub fn text(&self) -> String {
        let mut out = String::new();
        for (i, t) in self.turns.iter().enumerate() {
            if i > 0 {
                out.push_str("\n\n");
            }
            let r = &t.result;
            for (k, d) in r.display.iter().enumerate() {
                if k > 0 {
                    if r.breaks.contains(&k) {
                        out.push('\n');
                    } else if !r.glued.contains(&k) {
                        out.push(' ');
                    }
                }
                out.push_str(d);
            }
        }
        out
    }

    /// The display string of every word of the transcript, in order (the raw words where a turn
    /// was rejected).
    pub fn display_words(&self) -> Vec<&str> {
        self.turns
            .iter()
            .flat_map(|t| t.result.display.iter().map(String::as_str))
            .collect()
    }
}

pub fn normalized_path(bundle: &Path) -> PathBuf {
    bundle.join(NORMALIZED_FILE)
}

pub fn partial_path(bundle: &Path) -> PathBuf {
    bundle.join(PARTIAL_FILE)
}

/// Reads the sidecar; `None` when there is none.
pub fn read(bundle: &Path) -> Result<Option<Normalized>, CmError> {
    let path = normalized_path(bundle);
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(CmError::internal(format!(
                "cannot read `{}`: {e}",
                path.display()
            )))
        }
    };
    serde_json::from_str(&text).map(Some).map_err(|e| {
        CmError::internal(format!(
            "`{}` is not a normalised transcript: {e}",
            path.display()
        ))
    })
}

/// Writes the sidecar (temporary file, then rename) and removes the partial file.
pub fn write(bundle: &Path, n: &Normalized) -> Result<PathBuf, CmError> {
    let path = normalized_path(bundle);
    let tmp = bundle.join(format!("{NORMALIZED_FILE}.tmp"));
    let bytes = serde_json::to_vec_pretty(n)
        .map_err(|e| CmError::internal(format!("cannot write the normalised transcript: {e}")))?;
    std::fs::write(&tmp, bytes)
        .and_then(|()| std::fs::rename(&tmp, &path))
        .map_err(|e| CmError::internal(format!("cannot write `{}`: {e}", path.display())))?;
    let _ = std::fs::remove_file(partial_path(bundle));
    Ok(path)
}

#[cfg(test)]
mod tests;
