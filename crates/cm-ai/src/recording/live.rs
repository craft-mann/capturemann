//! Following a recording that is still being written (M2h): the finished pieces of its microphone
//! file are transcribed while it runs, and the state is saved beside the bundle as
//! `transcript.live.json`. The transcription job that runs at Stop finds that file and goes on from
//! it (`transcribe_recording_resumable` turns it into the batch checkpoint), so only the last
//! piece is left. Everything here is optional: a file that does not fit (another model, language,
//! prompt, track or time base, or a recording that is not the one that was followed) is ignored
//! and the job starts from the first piece, as before.

use std::fs;
use std::path::{Path, PathBuf};

use cm_core::edit::session_to_file;
use cm_core::{CmError, MediaTime, PauseGap};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::audio::{decode_16k_mono, SAMPLE_RATE};
use crate::pieces::{LiveCheckpoint, LiveTranscription, LONG_AUDIO_SECS, PIECE_SECS};
use crate::transcribe::Transcriber;

/// The state file inside the bundle.
pub const LIVE_FILE: &str = "transcript.live.json";

/// What a run was made with: the file only fits a job that asks for the same.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveKey {
    pub model: String,
    pub language: Option<String>,
    pub prompt: Option<String>,
    pub original_audio: bool,
    /// The microphone file that was followed (its name).
    pub file: String,
    /// File time of its first sample, in ticks.
    pub base: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LiveFile {
    key: LiveKey,
    checkpoint: LiveCheckpoint,
}

/// A run over one recording.
pub struct LiveRun {
    bundle: PathBuf,
    key: LiveKey,
    track: PathBuf,
    run: LiveTranscription,
}

/// The microphone of a recording in progress, from `recording.lock`: the file to read and the
/// file time of its first sample. `None` while the recorder has not placed the track yet.
fn mic_of_lock(bundle: &Path, original: bool) -> Option<(PathBuf, MediaTime)> {
    let text = fs::read_to_string(bundle.join("recording.lock")).ok()?;
    let v: Value = serde_json::from_str(&text).ok()?;
    let gaps: Vec<PauseGap> = v
        .get("gaps")
        .and_then(|g| serde_json::from_value(g.clone()).ok())
        .unwrap_or_default();
    let mic = v.get("tracks")?.get("mic")?;
    let key = |k: &str| mic.get(k).and_then(Value::as_str);
    let name = if original {
        key("raw_file").or_else(|| key("file"))?
    } else {
        key("denoised_file").or_else(|| key("file"))?
    };
    let start = MediaTime::from_ticks(mic.get("start").and_then(Value::as_i64)?);
    let path = bundle.join(name);
    path.is_file()
        .then(|| (path, session_to_file(&gaps, start)))
}

impl LiveRun {
    /// Starts following `bundle`. `None` when the microphone is not there yet (try again later).
    pub fn open(
        bundle: &Path,
        model: &str,
        language: Option<&str>,
        prompt: Option<&str>,
        original_audio: bool,
    ) -> Option<LiveRun> {
        let (track, base) = mic_of_lock(bundle, original_audio)?;
        let key = LiveKey {
            model: model.to_owned(),
            language: language.map(str::to_owned),
            prompt: prompt.map(str::to_owned),
            original_audio,
            file: track.file_name()?.to_string_lossy().into_owned(),
            base: base.ticks(),
        };
        let run = LiveTranscription::new(base, language, prompt);
        Some(LiveRun {
            bundle: bundle.to_owned(),
            key,
            track,
            run,
        })
    }

    /// Seconds of the microphone file on disk (from its header, which the writer patches about once
    /// a second), or 0.
    fn secs_on_disk(&self) -> f64 {
        cm_core::wav::read_info(&self.track)
            .map(|i| i.frames() as f64 / f64::from(i.sample_rate.max(1)))
            .unwrap_or(0.0)
    }

    /// Reads every piece the file makes certain. Cheap when none is: the file is decoded only once
    /// it is long enough. Saves the state after a finished piece. Returns how many it finished;
    /// `progress` returning `false` stops the piece in hand (it is read again next time).
    pub fn step(
        &mut self,
        transcriber: &dyn Transcriber,
        progress: &mut dyn FnMut(f32) -> bool,
    ) -> Result<usize, CmError> {
        let done_secs = self.run.done_samples() as f64 / f64::from(SAMPLE_RATE);
        let needed = if self.run.done_samples() == 0 {
            LONG_AUDIO_SECS
        } else {
            done_secs + PIECE_SECS * 1.5
        };
        if self.secs_on_disk() <= needed {
            return Ok(0);
        }
        let samples = decode_16k_mono(&self.track)?;
        let before = self.run.done_samples();
        let r = self.run.advance(transcriber, &samples, progress);
        // Pieces finished before a stop or an error in a later one are kept.
        if self.run.done_samples() > before {
            self.save()?;
        }
        r
    }

    fn save(&self) -> Result<(), CmError> {
        let file = LiveFile {
            key: self.key.clone(),
            checkpoint: self.run.checkpoint(),
        };
        let bytes = serde_json::to_vec(&file).map_err(|e| CmError::internal(e.to_string()))?;
        let target = self.bundle.join(LIVE_FILE);
        let tmp = self.bundle.join(format!(".{LIVE_FILE}.tmp"));
        fs::write(&tmp, bytes)
            .and_then(|()| fs::rename(&tmp, &target))
            .map_err(|e| CmError::internal(format!("cannot write `{}`: {e}", target.display())))
    }
}

/// Deletes the saved state of a bundle (a discarded recording, or a job that used it).
pub fn remove_live_state(bundle: &Path) {
    let _ = fs::remove_file(bundle.join(LIVE_FILE));
}

/// The batch checkpoint a job for `key` may resume from, as JSON: the saved state when it was made
/// with the same `key` and its finished pieces are a prefix of the plan of `samples` (the whole
/// file as the job reads it). `None` otherwise.
pub fn resume_from_live(bundle: &Path, key: &LiveKey, samples: &[f32]) -> Option<Value> {
    let text = fs::read_to_string(bundle.join(LIVE_FILE)).ok()?;
    let file: LiveFile = serde_json::from_str(&text).ok()?;
    if &file.key != key {
        return None;
    }
    let run = LiveTranscription::resume(
        MediaTime::from_ticks(key.base),
        key.language.as_deref(),
        key.prompt.as_deref(),
        file.checkpoint,
    );
    let cp = run.to_batch_checkpoint(samples)?;
    serde_json::to_value(cp).ok()
}
