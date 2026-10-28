//! Transcribing a recording bundle: pick the audio tracks, decode them, run a [`Transcriber`], and
//! put the words on the session clock (every edit time is session time). Also reads and writes
//! `transcript.json` in the bundle.
//!
//! Media files have continuous timestamps; pauses are removed from them, so a position in a file
//! is not a session time once the recording was paused (REC-4). The mic file starts at
//! `tracks.mic.start`; [`file_to_session`] adds back the pause time before each word.

use std::fs;
use std::path::{Path, PathBuf};

use cm_core::edit::session_to_file;
use cm_core::{CmError, MediaTime, PauseGap};
use serde_json::Value;

use crate::audio::{decode_16k_mono, mix_at, SAMPLE_RATE};
use crate::transcribe::{Pcm16k, Transcriber, Transcript, Word, TRANSCRIPT_VERSION};

/// The transcript file inside a bundle.
pub const TRANSCRIPT_FILE: &str = "transcript.json";

/// Choices for one transcription.
#[derive(Debug, Clone, Default)]
pub struct TranscribeOptions<'a> {
    /// `None` or `"auto"` detects the language.
    pub language: Option<&'a str>,
    /// Mix the system-audio track in with the microphone (it is also used alone when the
    /// recording has no microphone).
    pub include_system: bool,
    /// Model id recorded in the transcript when the transcriber does not set one.
    pub model_id: Option<&'a str>,
    /// Whisper's `initial_prompt` (the glossary's terms, see [`crate::glossary::initial_prompt`]).
    pub prompt: Option<&'a str>,
}

/// Path of `transcript.json` in a bundle folder.
pub fn transcript_path(bundle: &Path) -> PathBuf {
    bundle.join(TRANSCRIPT_FILE)
}

/// Reads the bundle's transcript; `None` when it has none yet.
pub fn read_transcript(bundle: &Path) -> Result<Option<Transcript>, CmError> {
    let path = transcript_path(bundle);
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(CmError::internal(format!("{}: {e}", path.display()))),
    };
    let t: Transcript = serde_json::from_str(&text)
        .map_err(|e| CmError::internal(format!("{} is damaged: {e}", path.display())))?;
    if t.version > TRANSCRIPT_VERSION {
        return Err(CmError::invalid_input(format!(
            "{} was written by a newer CaptureMann (format {})",
            path.display(),
            t.version
        )));
    }
    Ok(Some(t))
}

/// Writes the bundle's transcript atomically (a temporary file, then a rename).
pub fn write_transcript(bundle: &Path, t: &Transcript) -> Result<(), CmError> {
    let path = transcript_path(bundle);
    let tmp = bundle.join(format!("{TRANSCRIPT_FILE}.tmp"));
    let body =
        serde_json::to_vec_pretty(t).map_err(|e| CmError::internal(format!("transcript: {e}")))?;
    fs::write(&tmp, body).map_err(|e| CmError::internal(format!("{}: {e}", tmp.display())))?;
    fs::rename(&tmp, &path).map_err(|e| CmError::internal(format!("{}: {e}", path.display())))
}

/// Deletes the bundle's transcript. Returns whether there was one.
pub fn remove_transcript(bundle: &Path) -> Result<bool, CmError> {
    match fs::remove_file(transcript_path(bundle)) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(CmError::internal(format!("transcript: {e}"))),
    }
}

/// Session time of a position in a media file (file time).
///
/// `gaps` are the pause gaps in session time, sorted. A position exactly at a pause belongs
/// before it when `is_end` (a word that ended as the pause began) and after it otherwise (a word
/// that began as the recording resumed).
pub fn file_to_session(gaps: &[PauseGap], file: MediaTime, is_end: bool) -> MediaTime {
    let mut sorted: Vec<&PauseGap> = gaps.iter().collect();
    sorted.sort_by_key(|g| g.from);
    let mut shift = 0i64;
    for g in sorted {
        // Where the pause happened in file time: the pauses before it are not in the file.
        let at = g.from.ticks() - shift;
        let after = file.ticks() > at || (!is_end && file.ticks() == at);
        if !after {
            break;
        }
        shift += (g.to - g.from).ticks();
    }
    MediaTime::from_ticks(file.ticks() + shift)
}

struct AudioTrack {
    file: PathBuf,
    start: MediaTime,
}

fn audio_track(
    bundle: &Path,
    tracks: &serde_json::Map<String, Value>,
    name: &str,
) -> Option<AudioTrack> {
    let t = tracks.get(name)?;
    if t.get("missing").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    // A denoised copy, when a recording stores one, reads better than the raw microphone.
    let file = t
        .get("denoised_file")
        .and_then(Value::as_str)
        .or_else(|| t.get("file").and_then(Value::as_str))?;
    let path = bundle.join(file);
    path.is_file().then(|| AudioTrack {
        file: path,
        start: MediaTime::from_ticks(t.get("start").and_then(Value::as_i64).unwrap_or(0)),
    })
}

/// Transcribes a recording bundle and returns the transcript with word times in session time. It
/// does **not** store it: call [`write_transcript`] (the daemon job does, then publishes
/// `transcript.ready`). `progress` gets 0.0 to 1.0 and may return `false` to cancel
/// ([`crate::transcribe::is_cancelled`] on the error).
///
/// Errors: `invalid_input` when the recording has no microphone or system-audio track.
pub fn transcribe_recording(
    bundle: &Path,
    transcriber: &dyn Transcriber,
    opts: &TranscribeOptions<'_>,
    progress: &mut dyn FnMut(f32) -> bool,
) -> Result<Transcript, CmError> {
    run_recording(bundle, transcriber, opts, false, progress)
}

/// The same audio and the same time mapping as [`transcribe_recording`], but with the Whisper
/// *translate* task (AI-5): the words are English (`language == "en"`). `opts.language` is the
/// spoken language (a hint); `opts.prompt` is not used. Nothing is stored.
pub fn translate_recording(
    bundle: &Path,
    transcriber: &dyn Transcriber,
    opts: &TranscribeOptions<'_>,
    progress: &mut dyn FnMut(f32) -> bool,
) -> Result<Transcript, CmError> {
    run_recording(bundle, transcriber, opts, true, progress)
}

fn run_recording(
    bundle: &Path,
    transcriber: &dyn Transcriber,
    opts: &TranscribeOptions<'_>,
    translate: bool,
    progress: &mut dyn FnMut(f32) -> bool,
) -> Result<Transcript, CmError> {
    let manifest = cm_library::rec::read_manifest(bundle)?;
    let mic = audio_track(bundle, &manifest.tracks, "mic");
    let system = audio_track(bundle, &manifest.tracks, "system");
    let chosen: Vec<AudioTrack> = match (mic, system) {
        (Some(m), Some(s)) if opts.include_system => vec![m, s],
        (Some(m), _) => vec![m],
        (None, Some(s)) => vec![s],
        (None, None) => {
            return Err(CmError::invalid_input(
                "This recording has no microphone or system-audio track to transcribe",
            ))
        }
    };
    if !progress(0.0) {
        return Err(crate::transcribe::cancelled());
    }
    let gaps = &manifest.gaps;
    // Place every track on the file-time axis: file time of its first sample.
    let file_starts: Vec<MediaTime> = chosen
        .iter()
        .map(|t| session_to_file(gaps, t.start))
        .collect();
    let base = file_starts.iter().copied().min().unwrap_or(MediaTime::ZERO);
    let mut mixed: Vec<f32> = Vec::new();
    for (track, file_start) in chosen.iter().zip(&file_starts) {
        let samples = decode_16k_mono(&track.file)?;
        let offset = ((*file_start - base).as_secs_f64() * f64::from(SAMPLE_RATE)).round() as usize;
        mix_at(&mut mixed, &samples, offset);
    }
    let audio = Pcm16k {
        samples: &mixed,
        start: base,
    };
    let mut t = if translate {
        transcriber.translate_to_english(audio, opts.language, progress)?
    } else {
        // Piece by piece, each piece checked for a repetition loop (long recordings used to come
        // back as one phrase repeated thousands of times; see `crate::pieces`).
        crate::pieces::transcribe_pieces(transcriber, audio, opts.language, opts.prompt, progress)?
            .0
    };
    t.words = t
        .words
        .into_iter()
        .map(|w| Word {
            start: file_to_session(gaps, w.start, false),
            end: file_to_session(gaps, w.end, true),
            ..w
        })
        .collect();
    // The model cannot place a word before the sound begins: whatever it timed earlier (words of
    // a clip that opens with silence are pegged to its start) is moved up to the first speech.
    if let Some((from, _)) = crate::vad::speech_bounds(&mixed) {
        let onset_file = base + crate::transcribe::media_secs(from as f64 / f64::from(SAMPLE_RATE));
        let onset = file_to_session(gaps, onset_file, false);
        for w in &mut t.words {
            if w.start < onset {
                w.start = onset;
                w.end = w.end.max(onset + crate::transcribe::media_secs(0.05));
            }
        }
    }
    if t.model.is_empty() {
        if let Some(id) = opts.model_id {
            t.model = id.to_owned();
        }
    }
    Ok(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gap(from: f64, to: f64) -> PauseGap {
        PauseGap {
            from: crate::transcribe::media_secs(from),
            to: crate::transcribe::media_secs(to),
        }
    }

    fn secs(t: MediaTime) -> f64 {
        t.as_secs_f64()
    }

    #[test]
    fn file_time_maps_back_through_pauses() {
        let g = [gap(5.0, 6.0)];
        let m =
            |f: f64, end: bool| secs(file_to_session(&g, crate::transcribe::media_secs(f), end));
        assert_eq!(m(4.0, false), 4.0);
        assert_eq!(m(7.0, false), 8.0);
        // Exactly at the pause: an end stays before it, a start moves after it.
        assert_eq!(m(5.0, true), 5.0);
        assert_eq!(m(5.0, false), 6.0);
    }

    #[test]
    fn two_pauses_accumulate_in_file_coordinates() {
        // Pauses at session 5-6 and 10-12: in file time they are at 5 and 9.
        let g = [gap(10.0, 12.0), gap(5.0, 6.0)];
        let m =
            |f: f64, end: bool| secs(file_to_session(&g, crate::transcribe::media_secs(f), end));
        assert_eq!(m(8.0, false), 9.0);
        assert_eq!(m(9.0, true), 10.0);
        assert_eq!(m(9.0, false), 12.0);
        assert_eq!(m(10.0, false), 13.0);
        assert_eq!(m(2.0, false), 2.0);
    }

    #[test]
    fn it_inverts_session_to_file() {
        let g = [gap(5.0, 6.0), gap(10.0, 12.0)];
        for s in [0.0, 1.5, 4.9, 6.0, 7.25, 9.9, 12.0, 20.0] {
            let session = crate::transcribe::media_secs(s);
            let back = file_to_session(&g, session_to_file(&g, session), false);
            assert_eq!(back, session, "{s}");
        }
    }

    #[test]
    fn transcript_round_trips_in_the_bundle() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read_transcript(dir.path()).unwrap().is_none());
        let t = Transcript {
            version: TRANSCRIPT_VERSION,
            language: "en".into(),
            model: "small".into(),
            words: vec![Word {
                text: "hi".into(),
                start: MediaTime::from_secs(1),
                end: MediaTime::from_secs(2),
                confidence: 0.5,
            }],
            note: None,
        };
        write_transcript(dir.path(), &t).unwrap();
        assert_eq!(read_transcript(dir.path()).unwrap().unwrap(), t);
        assert!(!dir.path().join("transcript.json.tmp").exists());
        assert!(remove_transcript(dir.path()).unwrap());
        assert!(!remove_transcript(dir.path()).unwrap());
    }

    #[test]
    fn a_damaged_or_newer_transcript_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(transcript_path(dir.path()), "{ nope").unwrap();
        assert!(read_transcript(dir.path())
            .unwrap_err()
            .to_string()
            .contains("damaged"));
        fs::write(
            transcript_path(dir.path()),
            r#"{"version":99,"language":"en","model":"x","words":[],"note":null}"#,
        )
        .unwrap();
        assert_eq!(
            read_transcript(dir.path()).unwrap_err().code().as_str(),
            "invalid_input"
        );
    }
}
