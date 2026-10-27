//! Speaker labels (AI-8): who spoke when. The model-independent part.
//!
//! What is here: the [`Diarizer`] trait (the seam where the real speaker model plugs in once spike
//! S7 has chosen one), a scripted fake, [`NoSpeakerModel`] (what production uses until then),
//! the pure logic around any diarizer (numbering by first speech, headset mode, word-to-speaker
//! assignment, rename, merge and reassign, the "Name: text" exports, `diarization.json`) and a
//! check that no voiceprint is ever written to disk (AI-8.AC6).
//!
//! What is NOT here: any ONNX, `ort` or model code. A real [`Diarizer`] is a later task.
//!
//! # Data
//!
//! * `diarization.json` in the bundle is the raw result of one run ([`DiarizationFile`]): segments
//!   `{ start, end, speaker }` in session time, the model id, when it was made.
//! * The manifest keys `speakers` (`[{ id, name }]`) and `speaker_segments` (the segments as the
//!   user left them after merges and reassignments) are the working copy. Every user change to
//!   them is one undoable revision (`cm_library::rec_edit::apply_extra`).
//! * Words get their speaker when read ([`assign_words`]), so a transcript that arrives (or is
//!   made again) after the diarization still gets speakers, and `transcript.json` never changes.
//!
//! # Privacy
//!
//! A [`Diarizer`] may use speaker embeddings (voiceprints) inside one call. They are never part of
//! any type in this module, so they cannot be stored. [`find_voiceprint_files`] is the test helper
//! that scans a bundle and a config folder for anything that looks like one.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use cm_core::{CmError, MediaTime, PauseGap, TICKS_PER_SECOND};
use cm_library::manifest::RecManifest;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::audio::SAMPLE_RATE;
use crate::caption::CaptionLine;
use crate::recording::file_to_session;
use crate::transcribe::{media_secs, Pcm16k, Word};

/// The diarization file inside a bundle.
pub const DIARIZATION_FILE: &str = "diarization.json";
/// `diarization.json` format version.
pub const DIARIZATION_VERSION: u32 = 1;
/// Manifest key: `[{ id, name }]`.
pub const MANIFEST_SPEAKERS: &str = "speakers";
/// Manifest key: the working segments.
pub const MANIFEST_SEGMENTS: &str = "speaker_segments";
/// A recording shorter than this is not diarized (AI-8 edge cases), in seconds.
pub const MIN_SECONDS: f64 = 10.0;
/// Setting: the name the user's own words get in headset mode (AI-8.AC7).
pub const SETTING_DISPLAY_NAME: &str = "profile.display_name";
/// The name when the setting is empty.
pub const DEFAULT_DISPLAY_NAME: &str = "Me";
/// Longest speaker name, in characters.
pub const MAX_NAME_CHARS: usize = 60;
/// The speaker id of the user in headset mode.
pub const ME_ID: u32 = 1;

/// One stretch of one speaker's voice, in session time (after [`diarize_audio`]) or in file time
/// (what a [`Diarizer`] returns).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Segment {
    pub start: MediaTime,
    pub end: MediaTime,
    pub speaker: u32,
    /// How loud this speaker is over the segment (RMS, 0.0 when unknown). Decides who wins when
    /// two speakers talk at once.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub energy: f32,
}

fn is_zero(v: &f32) -> bool {
    *v == 0.0
}

impl Segment {
    pub fn new(start: MediaTime, end: MediaTime, speaker: u32) -> Segment {
        Segment {
            start,
            end,
            speaker,
            energy: 0.0,
        }
    }
}

/// A speaker and the name shown for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Speaker {
    pub id: u32,
    pub name: String,
}

/// `diarization.json`: the raw result of one run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiarizationFile {
    pub version: u32,
    /// The model that made it.
    pub model: String,
    /// When (RFC 3339).
    pub created: String,
    /// The run used headset mode (the mic track is the user).
    #[serde(default)]
    pub headset: bool,
    /// Why there is nothing, or a caveat (one speaker, too short).
    #[serde(default)]
    pub note: Option<String>,
    pub segments: Vec<Segment>,
}

/// The working state of a recording: the speakers and their segments.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SpeakerState {
    pub speakers: Vec<Speaker>,
    pub segments: Vec<Segment>,
}

impl SpeakerState {
    /// The name of a speaker, `Speaker <id>` when it is not in the list.
    pub fn name_of(&self, id: u32) -> String {
        name_of(&self.speakers, id)
    }

    /// The manifest values of this state (`speakers`, `speaker_segments`).
    pub fn manifest_values(&self) -> Map<String, Value> {
        let mut m = Map::new();
        m.insert(
            MANIFEST_SPEAKERS.into(),
            serde_json::to_value(&self.speakers).unwrap_or(Value::Null),
        );
        m.insert(
            MANIFEST_SEGMENTS.into(),
            serde_json::to_value(&self.segments).unwrap_or(Value::Null),
        );
        m
    }

    /// The state stored in a manifest; `None` when the recording was never diarized.
    pub fn from_manifest(m: &RecManifest) -> Option<SpeakerState> {
        let speakers: Vec<Speaker> =
            serde_json::from_value(m.extra.get(MANIFEST_SPEAKERS)?.clone()).ok()?;
        let segments: Vec<Segment> = m
            .extra
            .get(MANIFEST_SEGMENTS)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();
        Some(SpeakerState { speakers, segments })
    }

    /// `[{ id, name }]` as JSON.
    pub fn speakers_json(&self) -> Value {
        serde_json::to_value(&self.speakers).unwrap_or(Value::Null)
    }
}

/// The name of speaker `id` in `speakers`.
pub fn name_of(speakers: &[Speaker], id: u32) -> String {
    speakers
        .iter()
        .find(|s| s.id == id)
        .map_or_else(|| format!("Speaker {id}"), |s| s.name.clone())
}

// ---- the model seam -------------------------------------------------------------------------

/// Hints that help clustering (AI-8): an exact number, or a range. `num_speakers` wins when the
/// range is given too.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiarizeHints {
    pub num_speakers: Option<u32>,
    pub min_speakers: Option<u32>,
    pub max_speakers: Option<u32>,
}

impl DiarizeHints {
    /// `invalid_input` for a count outside 1 to 32 or a range the wrong way round.
    pub fn check(&self) -> Result<(), CmError> {
        let bad = |field: &str, why: &str| Err(CmError::invalid_field(field, why));
        for (name, v) in [
            ("num_speakers", self.num_speakers),
            ("min_speakers", self.min_speakers),
            ("max_speakers", self.max_speakers),
        ] {
            if v.is_some_and(|n| !(1..=32).contains(&n)) {
                return bad(name, "must be 1 to 32");
            }
        }
        if let (Some(lo), Some(hi)) = (self.min_speakers, self.max_speakers) {
            if lo > hi {
                return bad("min_speakers", "must not be more than max_speakers");
            }
        }
        Ok(())
    }
}

/// Finds who speaks when in 16 kHz mono audio. Segment times are `audio.start` plus the position
/// in `audio.samples` (file time, like [`crate::transcribe::Transcriber`]); speaker labels are
/// arbitrary numbers, [`diarize_audio`] numbers them by first speech. `progress` gets 0.0 to 1.0
/// and may return `false` to cancel ([`crate::transcribe::cancelled`]).
///
/// The real implementation (ONNX, chosen by spike S7) is a later task. Speaker embeddings may live
/// inside a call and must never be returned or stored.
pub trait Diarizer: Send + Sync {
    /// The model id written to `diarization.json`.
    fn model_id(&self) -> String;

    /// Whether a model is installed and usable; the error says what is missing.
    fn ready(&self) -> Result<(), CmError> {
        Ok(())
    }

    fn diarize(
        &self,
        audio: Pcm16k<'_>,
        hints: &DiarizeHints,
        progress: &mut dyn FnMut(f32) -> bool,
    ) -> Result<Vec<Segment>, CmError>;
}

/// The marker text of the error [`NoSpeakerModel`] returns.
const NO_SPEAKER_MODEL: &str = "No speaker model is installed, so speakers cannot be told apart. \
CaptureMann releases include the two speaker models; this build does not (a developer build needs \
`make sherpa` before `make bundle`, or the files can be added with `import_model`). Nothing is \
downloaded automatically.";

/// `invalid_input`; the daemon attaches `data.reason = "no_speaker_model"` ([`error_data`]).
pub fn no_speaker_model() -> CmError {
    CmError::invalid_input(NO_SPEAKER_MODEL)
}

/// Whether an error is [`no_speaker_model`].
pub fn is_no_speaker_model(e: &CmError) -> bool {
    matches!(e, CmError::InvalidInput { message } if message == NO_SPEAKER_MODEL)
}

/// `{"reason": "no_speaker_model"}` for [`no_speaker_model`], `None` for any other error.
pub fn error_data(e: &CmError) -> Option<Value> {
    is_no_speaker_model(e).then(|| json!({ "reason": "no_speaker_model" }))
}

/// The seam for the real model: always answers [`no_speaker_model`]. Production wiring uses it
/// until spike S7 decides and a real [`Diarizer`] replaces it.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoSpeakerModel;

impl Diarizer for NoSpeakerModel {
    fn model_id(&self) -> String {
        "none".into()
    }
    fn ready(&self) -> Result<(), CmError> {
        Err(no_speaker_model())
    }
    fn diarize(
        &self,
        _: Pcm16k<'_>,
        _: &DiarizeHints,
        _: &mut dyn FnMut(f32) -> bool,
    ) -> Result<Vec<Segment>, CmError> {
        Err(no_speaker_model())
    }
}

/// A fake [`Diarizer`] that returns a fixed script, for tests of everything built on the trait.
/// Script times are offsets from the start of the audio (shifted by `audio.start`). It records the
/// hints of every run.
#[derive(Debug, Clone)]
pub struct ScriptedDiarizer {
    pub model: String,
    pub segments: Vec<Segment>,
    pub steps: u32,
    pub hints_seen: std::sync::Arc<std::sync::Mutex<Vec<DiarizeHints>>>,
}

impl ScriptedDiarizer {
    pub fn new(segments: Vec<Segment>) -> ScriptedDiarizer {
        ScriptedDiarizer {
            model: "scripted".into(),
            segments,
            steps: 4,
            hints_seen: Default::default(),
        }
    }

    /// From `(start s, end s, speaker label, energy)`.
    pub fn from_secs(script: &[(f64, f64, u32, f32)]) -> ScriptedDiarizer {
        ScriptedDiarizer::new(
            script
                .iter()
                .map(|&(s, e, spk, energy)| Segment {
                    start: media_secs(s),
                    end: media_secs(e),
                    speaker: spk,
                    energy,
                })
                .collect(),
        )
    }
}

impl Diarizer for ScriptedDiarizer {
    fn model_id(&self) -> String {
        self.model.clone()
    }

    fn diarize(
        &self,
        audio: Pcm16k<'_>,
        hints: &DiarizeHints,
        progress: &mut dyn FnMut(f32) -> bool,
    ) -> Result<Vec<Segment>, CmError> {
        self.hints_seen
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(*hints);
        let steps = self.steps.max(1);
        for i in 1..=steps {
            if !progress(i as f32 / steps as f32) {
                return Err(crate::transcribe::cancelled());
            }
        }
        Ok(self
            .segments
            .iter()
            .map(|s| Segment {
                start: s.start + audio.start,
                end: s.end + audio.start,
                ..*s
            })
            .collect())
    }
}

// ---- running a diarizer ---------------------------------------------------------------------

/// The audio of one recording on the file-time axis (16 kHz mono).
pub struct DiarizeInput<'a> {
    /// The mix of the tracks (mic and system), for the normal mode.
    pub mix: &'a [f32],
    /// The microphone alone (headset mode).
    pub mic: Option<&'a [f32]>,
    /// System audio alone (headset mode diarizes only this).
    pub system: Option<&'a [f32]>,
    /// File time of sample 0.
    pub start: MediaTime,
    /// Pause gaps, to put file times on the session clock.
    pub gaps: &'a [PauseGap],
}

/// Choices of one run.
#[derive(Debug, Clone)]
pub struct DiarizeOptions<'a> {
    pub hints: DiarizeHints,
    /// "My microphone only hears me": the mic track is the user (AI-8.AC7).
    pub headset: bool,
    /// The user's name for headset mode.
    pub display_name: &'a str,
    /// RFC 3339 time for `diarization.json`.
    pub created: &'a str,
}

/// A finished run: the file to store and the speakers to show.
#[derive(Debug, Clone, PartialEq)]
pub struct Diarized {
    pub file: DiarizationFile,
    pub speakers: Vec<Speaker>,
}

impl Diarized {
    /// The working state a run starts from.
    pub fn state(&self) -> SpeakerState {
        SpeakerState {
            speakers: self.speakers.clone(),
            segments: self.file.segments.clone(),
        }
    }
}

/// The user's name for headset mode: the setting, else "Me".
pub fn display_name_from(settings: &Map<String, Value>) -> String {
    settings
        .get(SETTING_DISPLAY_NAME)
        .and_then(Value::as_str)
        .map(|s| clean_name(s).unwrap_or_default())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_DISPLAY_NAME.to_owned())
}

/// A name trimmed, with inner whitespace collapsed; `None` when it is empty after that.
pub fn clean_name(raw: &str) -> Option<String> {
    let name: String = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    let name: String = name.chars().filter(|c| !c.is_control()).collect();
    let name: String = name.chars().take(MAX_NAME_CHARS).collect();
    let name = name.trim().to_owned();
    (!name.is_empty()).then_some(name)
}

const WINDOW_SECS: f64 = 0.1;

/// RMS of each 100 ms window.
fn window_rms(samples: &[f32]) -> Vec<f32> {
    let n = (f64::from(SAMPLE_RATE) * WINDOW_SECS) as usize;
    samples
        .chunks(n)
        .map(|c| (c.iter().map(|s| s * s).sum::<f32>() / c.len() as f32).sqrt())
        .collect()
}

/// Mean RMS of `samples` over `[start, end)` (file time relative to `base`).
fn rms_between(samples: &[f32], base: MediaTime, start: MediaTime, end: MediaTime) -> f32 {
    let at = |t: MediaTime| (((t - base).as_secs_f64().max(0.0)) * f64::from(SAMPLE_RATE)) as usize;
    let (a, b) = (at(start).min(samples.len()), at(end).min(samples.len()));
    if b <= a {
        return 0.0;
    }
    (samples[a..b].iter().map(|s| s * s).sum::<f32>() / (b - a) as f32).sqrt()
}

/// Where the microphone is active (file time): windows above a level relative to the loudest
/// speech on the track, bridged over short dips, with each stretch's level as `energy`.
pub fn mic_activity(mic: &[f32], start: MediaTime, speaker: u32) -> Vec<Segment> {
    let rms = window_rms(mic);
    let mut sorted: Vec<f32> = rms.iter().copied().filter(|v| *v > 0.0).collect();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let Some(p95) = sorted.get(sorted.len() * 95 / 100).copied() else {
        return Vec::new();
    };
    let threshold = (p95 * 0.1).max(1e-3);
    // Dips up to 300 ms inside speech do not end it; a burst under 200 ms is not speech.
    let (max_dip, min_len) = (3usize, 2usize);
    let mut out = Vec::new();
    let mut run: Option<(usize, usize)> = None;
    let close = |run: (usize, usize), out: &mut Vec<Segment>| {
        if run.1 - run.0 >= min_len {
            let slice = &rms[run.0..run.1];
            let energy = slice.iter().sum::<f32>() / slice.len() as f32;
            out.push(Segment {
                start: start + media_secs(run.0 as f64 * WINDOW_SECS),
                end: start + media_secs(run.1 as f64 * WINDOW_SECS),
                speaker,
                energy,
            });
        }
    };
    for (i, &r) in rms.iter().enumerate() {
        if r > threshold {
            match run {
                Some((s, e)) if i - e <= max_dip + 1 => run = Some((s, i + 1)),
                Some(done) => {
                    close(done, &mut out);
                    run = Some((i, i + 1));
                }
                None => run = Some((i, i + 1)),
            }
        }
    }
    if let Some(done) = run {
        close(done, &mut out);
    }
    out
}

/// Renumbers raw speaker labels `first..` in order of first speech, drops empty segments, sorts
/// by start. Returns the segments and the labels' new ids in order.
fn number_by_first_speech(mut raw: Vec<Segment>, first: u32) -> (Vec<Segment>, Vec<u32>) {
    raw.retain(|s| s.end > s.start);
    raw.sort_by_key(|s| (s.start, s.end));
    let mut map: BTreeMap<u32, u32> = BTreeMap::new();
    let mut order = Vec::new();
    for s in &mut raw {
        let next = first + map.len() as u32;
        let id = *map.entry(s.speaker).or_insert_with(|| {
            order.push(next);
            next
        });
        s.speaker = id;
    }
    (raw, order)
}

/// Runs `diarizer` over a recording's audio and returns the finished result (AI-8): session
/// times, speakers numbered and named "Speaker 1..N" by first speech, the headset rules, the
/// short-recording and one-speaker notes. Nothing is written; see [`write_diarization`].
pub fn diarize_audio(
    diarizer: &dyn Diarizer,
    input: &DiarizeInput<'_>,
    opts: &DiarizeOptions<'_>,
    progress: &mut dyn FnMut(f32) -> bool,
) -> Result<Diarized, CmError> {
    opts.hints.check()?;
    let seconds = |n: usize| n as f64 / f64::from(SAMPLE_RATE);
    let longest = [Some(input.mix), input.mic, input.system]
        .into_iter()
        .flatten()
        .map(<[f32]>::len)
        .max()
        .unwrap_or(0);
    let model = diarizer.model_id();
    let finish = |segments: Vec<Segment>, speakers: Vec<Speaker>, note: Option<String>| Diarized {
        file: DiarizationFile {
            version: DIARIZATION_VERSION,
            model: model.clone(),
            created: opts.created.to_owned(),
            headset: opts.headset,
            note,
            segments,
        },
        speakers,
    };
    if seconds(longest) < MIN_SECONDS {
        return Ok(finish(
            Vec::new(),
            Vec::new(),
            Some(format!(
                "Skipped: the recording is shorter than {MIN_SECONDS:.0} seconds, too short to tell speakers apart"
            )),
        ));
    }
    if !progress(0.0) {
        return Err(crate::transcribe::cancelled());
    }
    let to_session = |segs: Vec<Segment>| -> Vec<Segment> {
        segs.into_iter()
            .map(|s| Segment {
                start: file_to_session(input.gaps, s.start, false),
                end: file_to_session(input.gaps, s.end, true),
                ..s
            })
            .collect()
    };
    let display = clean_name(opts.display_name).unwrap_or_else(|| DEFAULT_DISPLAY_NAME.to_owned());

    if !opts.headset {
        let mut raw = diarizer.diarize(
            Pcm16k {
                samples: input.mix,
                start: input.start,
            },
            &opts.hints,
            progress,
        )?;
        fill_energy(&mut raw, input.mix, input.start);
        let (segments, ids) = number_by_first_speech(to_session(raw), 1);
        let speakers: Vec<Speaker> = ids
            .iter()
            .enumerate()
            .map(|(i, id)| Speaker {
                id: *id,
                name: format!("Speaker {}", i + 1),
            })
            .collect();
        let note = (speakers.len() == 1).then(one_speaker_note);
        return Ok(finish(segments, speakers, note));
    }

    // Headset: the microphone is the user, only the system track is diarized (AI-8.AC7).
    let mic = input.mic.ok_or_else(|| {
        CmError::invalid_input("Headset mode needs a microphone track, and this recording has none")
    })?;
    let mut me = mic_activity(mic, input.start, ME_ID);
    let others = match input.system {
        Some(system) if seconds(system.len()) >= MIN_SECONDS => {
            let mut raw = diarizer.diarize(
                Pcm16k {
                    samples: system,
                    start: input.start,
                },
                &opts.hints,
                progress,
            )?;
            fill_energy(&mut raw, system, input.start);
            raw
        }
        _ => Vec::new(),
    };
    // The system speakers are numbered after the user's id.
    let (others, ids) = number_by_first_speech(to_session(others), ME_ID + 1);
    me = to_session(me);
    let mut speakers = Vec::new();
    if !me.is_empty() {
        speakers.push(Speaker {
            id: ME_ID,
            name: display,
        });
    }
    speakers.extend(ids.iter().enumerate().map(|(i, id)| Speaker {
        id: *id,
        name: format!("Speaker {}", i + 1),
    }));
    let mut segments = me;
    segments.extend(others);
    segments.sort_by_key(|s| (s.start, s.end));
    let note = (speakers.len() == 1).then(one_speaker_note);
    Ok(finish(segments, speakers, note))
}

/// One audio track of a bundle: its file and the session time of its first sample.
fn track_file(bundle: &Path, m: &RecManifest, name: &str) -> Option<(PathBuf, MediaTime)> {
    let t = m.tracks.get(name)?;
    if t.get("missing").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    let file = t
        .get("denoised_file")
        .and_then(Value::as_str)
        .or_else(|| t.get("file").and_then(Value::as_str))?;
    let path = bundle.join(file);
    let start = MediaTime::from_ticks(t.get("start").and_then(Value::as_i64).unwrap_or(0));
    path.is_file().then_some((path, start))
}

/// Diarizes a recording bundle: decodes the mic and system tracks with FFmpeg, puts them on one
/// axis and calls [`diarize_audio`]. It does not store anything ([`write_diarization`]).
/// `invalid_input` when the recording has no microphone or system-audio track. (Needs FFmpeg; the
/// logic is tested through [`diarize_audio`].)
pub fn diarize_recording(
    bundle: &Path,
    diarizer: &dyn Diarizer,
    opts: &DiarizeOptions<'_>,
    progress: &mut dyn FnMut(f32) -> bool,
) -> Result<Diarized, CmError> {
    let manifest = cm_library::rec::read_manifest(bundle)?;
    let mic = track_file(bundle, &manifest, "mic");
    let system = track_file(bundle, &manifest, "system");
    if mic.is_none() && system.is_none() {
        return Err(CmError::invalid_input(
            "This recording has no microphone or system-audio track to tell speakers apart",
        ));
    }
    if opts.headset && mic.is_none() {
        return Err(CmError::invalid_input(
            "Headset mode needs a microphone track, and this recording has none",
        ));
    }
    let gaps = &manifest.gaps;
    let file_start = |t: &Option<(PathBuf, MediaTime)>| {
        t.as_ref()
            .map(|(_, s)| cm_core::edit::session_to_file(gaps, *s))
    };
    let base = [file_start(&mic), file_start(&system)]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(MediaTime::ZERO);
    let place = |t: &Option<(PathBuf, MediaTime)>| -> Result<Option<Vec<f32>>, CmError> {
        let Some((file, start)) = t else {
            return Ok(None);
        };
        let samples = crate::audio::decode_16k_mono(file)?;
        let offset = ((cm_core::edit::session_to_file(gaps, *start) - base).as_secs_f64()
            * f64::from(SAMPLE_RATE))
        .round() as usize;
        let mut out = Vec::new();
        crate::audio::mix_at(&mut out, &samples, offset);
        Ok(Some(out))
    };
    let (mic_buf, system_buf) = (place(&mic)?, place(&system)?);
    let mut mix: Vec<f32> = Vec::new();
    for b in [&mic_buf, &system_buf].into_iter().flatten() {
        crate::audio::mix_at(&mut mix, b, 0);
    }
    diarize_audio(
        diarizer,
        &DiarizeInput {
            mix: &mix,
            mic: mic_buf.as_deref(),
            system: system_buf.as_deref(),
            start: base,
            gaps,
        },
        opts,
        progress,
    )
}

fn one_speaker_note() -> String {
    "Only one speaker was detected, so every word is labelled Speaker 1".to_owned()
}

/// Gives segments without a level the RMS of the audio they cover.
fn fill_energy(segments: &mut [Segment], samples: &[f32], base: MediaTime) {
    for s in segments.iter_mut().filter(|s| s.energy == 0.0) {
        s.energy = rms_between(samples, base, s.start, s.end);
    }
}

// ---- words and turns ------------------------------------------------------------------------

/// The speaker of one word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WordLabel {
    pub speaker: Option<u32>,
    /// More than one speaker was active at the word's midpoint.
    pub overlap: bool,
}

fn midpoint(a: MediaTime, b: MediaTime) -> MediaTime {
    MediaTime::from_ticks((a.ticks() + b.ticks()) / 2)
}

/// Each word's speaker: the one active at the word's midpoint (AI-8). When several are, the one
/// with the highest `energy` wins (the earliest segment on a tie) and the word is marked as
/// overlap. A word in a gap takes the nearest segment's speaker. `None` only when there are no
/// segments.
pub fn assign_words(words: &[Word], segments: &[Segment]) -> Vec<WordLabel> {
    words
        .iter()
        .map(|w| {
            let mid = midpoint(w.start, w.end);
            let active: Vec<&Segment> = segments
                .iter()
                .filter(|s| s.start <= mid && mid < s.end)
                .collect();
            let mut distinct: Vec<u32> = active.iter().map(|s| s.speaker).collect();
            distinct.sort_unstable();
            distinct.dedup();
            if let Some(best) = active.iter().fold(None::<&&Segment>, |best, s| match best {
                Some(b) if b.energy >= s.energy => Some(b),
                _ => Some(s),
            }) {
                return WordLabel {
                    speaker: Some(best.speaker),
                    overlap: distinct.len() > 1,
                };
            }
            let nearest = segments.iter().min_by_key(|s| {
                if mid < s.start {
                    (s.start - mid).ticks()
                } else {
                    (mid - s.end).ticks()
                }
            });
            WordLabel {
                speaker: nearest.map(|s| s.speaker),
                overlap: false,
            }
        })
        .collect()
}

/// One speaker's consecutive words.
#[derive(Debug, Clone, PartialEq)]
pub struct Turn {
    pub speaker: Option<u32>,
    pub start: MediaTime,
    pub end: MediaTime,
    pub text: String,
    /// Some word of the turn was spoken over another speaker.
    pub overlap: bool,
}

/// Groups words into turns: a new turn starts when the speaker changes.
pub fn turns(words: &[Word], labels: &[WordLabel]) -> Vec<Turn> {
    let mut out: Vec<Turn> = Vec::new();
    for (w, l) in words.iter().zip(labels) {
        match out.last_mut() {
            Some(t) if t.speaker == l.speaker => {
                t.end = t.end.max(w.end);
                t.text.push(' ');
                t.text.push_str(w.text.trim());
                t.overlap |= l.overlap;
            }
            _ => out.push(Turn {
                speaker: l.speaker,
                start: w.start,
                end: w.end,
                text: w.text.trim().to_owned(),
                overlap: l.overlap,
            }),
        }
    }
    out
}

/// A caption line's speaker: the one talking longest within it. The flag says another speaker
/// covers a third or more of the line (overlap).
pub fn line_speaker(line: &CaptionLine, segments: &[Segment]) -> Option<(u32, bool)> {
    let mut per: BTreeMap<u32, i64> = BTreeMap::new();
    for s in segments {
        let a = s.start.max(line.start);
        let b = s.end.min(line.end);
        if b > a {
            *per.entry(s.speaker).or_default() += (b - a).ticks();
        }
    }
    if per.is_empty() {
        let mid = midpoint(line.start, line.end);
        let words = [Word {
            text: String::new(),
            start: mid,
            end: mid,
            confidence: 1.0,
        }];
        return assign_words(&words, segments)[0]
            .speaker
            .map(|s| (s, false));
    }
    let total = (line.end - line.start).ticks().max(1);
    let best = per
        .iter()
        .max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(a.0)))
        .map(|(k, v)| (*k, *v))?;
    let overlap = per.iter().any(|(k, v)| *k != best.0 && *v * 3 >= total);
    Some((best.0, overlap))
}

// ---- rename, merge, reassign ----------------------------------------------------------------

/// One reassignment: the time range `[start, end)` goes to `speaker`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Assign {
    pub start: MediaTime,
    pub end: MediaTime,
    pub speaker: u32,
}

/// One `set_speakers` call. Applied in order: renames, merges, reassignments.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SpeakerEdit {
    pub rename: Vec<(u32, String)>,
    pub merge: Vec<(u32, u32)>,
    pub assign: Vec<Assign>,
}

impl SpeakerEdit {
    pub fn is_empty(&self) -> bool {
        self.rename.is_empty() && self.merge.is_empty() && self.assign.is_empty()
    }
}

/// Joins touching or overlapping segments of one speaker.
fn coalesce(mut segs: Vec<Segment>) -> Vec<Segment> {
    segs.sort_by_key(|s| (s.speaker, s.start, s.end));
    let mut out: Vec<Segment> = Vec::new();
    for s in segs {
        match out.last_mut() {
            Some(l) if l.speaker == s.speaker && s.start <= l.end => {
                l.end = l.end.max(s.end);
                l.energy = l.energy.max(s.energy);
            }
            _ => out.push(s),
        }
    }
    out.sort_by_key(|s| (s.start, s.end, s.speaker));
    out
}

/// Applies an edit to a state and returns the new one (the old one is untouched). Everything is
/// checked first: `invalid_input` for an unknown speaker, an empty or repeated name, a merge into
/// itself, an empty range; on error nothing changes.
pub fn apply_edit(state: &SpeakerState, edit: &SpeakerEdit) -> Result<SpeakerState, CmError> {
    if edit.is_empty() {
        return Err(CmError::invalid_input(
            "Nothing to change: give rename, merge or assign",
        ));
    }
    let mut next = state.clone();
    let known = |s: &SpeakerState, id: u32, field: &str| {
        if s.speakers.iter().any(|x| x.id == id) {
            Ok(())
        } else {
            let ids: Vec<String> = s.speakers.iter().map(|x| x.id.to_string()).collect();
            Err(CmError::invalid_field(
                field,
                format!(
                    "there is no speaker {id}; the speakers are: {}",
                    if ids.is_empty() {
                        "none".to_owned()
                    } else {
                        ids.join(", ")
                    }
                ),
            ))
        }
    };
    for (id, raw) in &edit.rename {
        known(&next, *id, "rename")?;
        let name = clean_name(raw)
            .ok_or_else(|| CmError::invalid_field("rename", "a name must not be empty"))?;
        if next
            .speakers
            .iter()
            .any(|s| s.id != *id && s.name.to_lowercase() == name.to_lowercase())
        {
            return Err(CmError::invalid_field(
                "rename",
                format!("another speaker is already called \"{name}\"; merge them instead"),
            ));
        }
        if let Some(s) = next.speakers.iter_mut().find(|s| s.id == *id) {
            s.name = name;
        }
    }
    for (from, into) in &edit.merge {
        known(&next, *from, "merge")?;
        known(&next, *into, "merge")?;
        if from == into {
            return Err(CmError::invalid_field(
                "merge",
                "a speaker cannot be merged into itself",
            ));
        }
        next.speakers.retain(|s| s.id != *from);
        for s in &mut next.segments {
            if s.speaker == *from {
                s.speaker = *into;
            }
        }
        next.segments = coalesce(std::mem::take(&mut next.segments));
    }
    for a in &edit.assign {
        known(&next, a.speaker, "assign")?;
        if a.end <= a.start {
            return Err(CmError::invalid_field("assign", "end must be after start"));
        }
        let mut kept = Vec::with_capacity(next.segments.len() + 2);
        for s in &next.segments {
            if s.end <= a.start || s.start >= a.end {
                kept.push(*s);
                continue;
            }
            if s.start < a.start {
                kept.push(Segment { end: a.start, ..*s });
            }
            if s.end > a.end {
                kept.push(Segment { start: a.end, ..*s });
            }
        }
        kept.push(Segment::new(a.start, a.end, a.speaker));
        next.segments = coalesce(kept);
    }
    Ok(next)
}

// ---- the bundle -----------------------------------------------------------------------------

/// `diarization.json` in a bundle.
pub fn diarization_path(bundle: &Path) -> PathBuf {
    bundle.join(DIARIZATION_FILE)
}

/// Reads `diarization.json`; `None` when there is none.
pub fn read_diarization(bundle: &Path) -> Result<Option<DiarizationFile>, CmError> {
    let path = diarization_path(bundle);
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(CmError::internal(format!(
                "cannot read `{}`: {e}",
                path.display()
            )))
        }
    };
    let file: DiarizationFile = serde_json::from_str(&text)
        .map_err(|e| CmError::invalid_input(format!("`{}` is damaged: {e}", path.display())))?;
    if file.version > DIARIZATION_VERSION {
        return Err(CmError::invalid_input(format!(
            "`{}` was written by a newer CaptureMann (version {})",
            path.display(),
            file.version
        )));
    }
    Ok(Some(file))
}

/// Writes `diarization.json` through a temporary file.
pub fn write_diarization(bundle: &Path, file: &DiarizationFile) -> Result<(), CmError> {
    let path = diarization_path(bundle);
    let tmp = bundle.join(format!(".{DIARIZATION_FILE}.tmp"));
    let bytes = serde_json::to_vec_pretty(file)
        .map_err(|e| CmError::internal(format!("diarization: {e}")))?;
    fs::write(&tmp, bytes)
        .and_then(|()| fs::rename(&tmp, &path))
        .map_err(|e| {
            let _ = fs::remove_file(&tmp);
            CmError::internal(format!("cannot write `{}`: {e}", path.display()))
        })
}

// ---- exports --------------------------------------------------------------------------------

/// A caption line with the name of whoever speaks it.
#[derive(Debug, Clone, PartialEq)]
pub struct SpokenLine {
    pub line: CaptionLine,
    pub speaker: Option<String>,
}

fn stamp(t: MediaTime, comma: bool) -> String {
    let ms = ((t.ticks().max(0) as f64) * 1000.0 / TICKS_PER_SECOND as f64).round() as i64;
    let sep = if comma { ',' } else { '.' };
    format!(
        "{:02}:{:02}:{:02}{sep}{:03}",
        ms / 3_600_000,
        ms / 60_000 % 60,
        ms / 1000 % 60,
        ms % 1000
    )
}

fn kept(lines: &[SpokenLine]) -> impl Iterator<Item = &SpokenLine> {
    lines.iter().filter(|l| !l.line.text.trim().is_empty())
}

/// WebVTT with voice tags: `<v Linh>text`. `&`, `<` and `>` are escaped in names and text.
pub fn to_vtt_voiced(lines: &[SpokenLine]) -> String {
    let esc = |s: &str| {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    };
    let mut out = String::from("WEBVTT\n\n");
    for l in kept(lines) {
        let tag = l
            .speaker
            .as_deref()
            .map_or_else(String::new, |n| format!("<v {}>", esc(n)));
        out.push_str(&format!(
            "{} --> {}\n{tag}{}\n\n",
            stamp(l.line.start, false),
            stamp(l.line.end.max(l.line.start), false),
            esc(l.line.text.trim())
        ));
    }
    out
}

/// SubRip. With `names` every cue with a known speaker starts with `NAME: ` in capitals; without
/// it the output is the plain SRT.
pub fn to_srt_named(lines: &[SpokenLine], names: bool) -> String {
    let mut out = String::new();
    for (n, l) in kept(lines).enumerate() {
        let prefix = match (&l.speaker, names) {
            (Some(s), true) => format!("{}: ", s.to_uppercase()),
            _ => String::new(),
        };
        out.push_str(&format!(
            "{}\n{} --> {}\n{prefix}{}\n\n",
            n + 1,
            stamp(l.line.start, true),
            stamp(l.line.end.max(l.line.start), true),
            l.line.text.trim()
        ));
    }
    out
}

/// Consecutive lines of one speaker as one turn: `(speaker, start, text)`.
fn text_turns(lines: &[SpokenLine]) -> Vec<(Option<String>, MediaTime, String)> {
    let mut out: Vec<(Option<String>, MediaTime, String)> = Vec::new();
    for l in kept(lines) {
        let text = l.line.text.split_whitespace().collect::<Vec<_>>().join(" ");
        match out.last_mut() {
            Some((s, _, t)) if *s == l.speaker && l.speaker.is_some() => {
                t.push(' ');
                t.push_str(&text);
            }
            _ => out.push((l.speaker.clone(), l.line.start, text)),
        }
    }
    out
}

fn clock(t: MediaTime, hours: bool) -> String {
    let secs = t.ticks().max(0) / TICKS_PER_SECOND;
    let (h, m, s) = (secs / 3600, secs / 60 % 60, secs % 60);
    if hours {
        format!("{h:02}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

/// Plain text, one turn per paragraph: `[mm:ss] Name: text` (the stamp only with `timestamps`).
/// Lines without a speaker have no name.
pub fn to_txt_speakers(lines: &[SpokenLine], timestamps: bool) -> String {
    let hours = kept(lines).any(|l| l.line.start.ticks() >= 3600 * TICKS_PER_SECOND);
    let mut out = String::new();
    for (speaker, start, text) in text_turns(lines) {
        if timestamps {
            out.push_str(&format!("[{}] ", clock(start, hours)));
        }
        if let Some(s) = speaker {
            out.push_str(&format!("{s}: "));
        }
        out.push_str(&text);
        out.push('\n');
    }
    out
}

/// Markdown, one turn per paragraph: `**[mm:ss] Name:** text`.
pub fn to_markdown_speakers(lines: &[SpokenLine]) -> String {
    let hours = kept(lines).any(|l| l.line.start.ticks() >= 3600 * TICKS_PER_SECOND);
    let esc = |s: &str| s.replace('*', "\\*").replace('_', "\\_");
    let mut out = String::new();
    for (speaker, start, text) in text_turns(lines) {
        let label = match speaker {
            Some(s) => format!("[{}] {}:", clock(start, hours), esc(&s)),
            None => format!("[{}]", clock(start, hours)),
        };
        out.push_str(&format!("**{label}** {text}\n\n"));
    }
    out
}

// ---- no voiceprints on disk (AI-8.AC6) ------------------------------------------------------

/// Names and keys that mean "a voiceprint".
const FORBIDDEN_WORDS: [&str; 5] = ["voiceprint", "embedding", "xvector", "x-vector", "dvector"];
const FORBIDDEN_EXTENSIONS: [&str; 5] = ["npy", "npz", "emb", "vp", "embeddings"];

/// Test helper (AI-8.AC6): every file under `roots` that looks like a stored voiceprint. A file
/// is suspect when its name contains `voiceprint`, `embedding`, `xvector` or `dvector`, when its
/// extension is `npy`, `npz`, `emb`, `vp` or `embeddings`, or when a small JSON or text file
/// mentions one of those words. Model files (`.onnx`, `.bin`) are not voiceprints and are not
/// opened. The list is empty when everything is clean.
pub fn find_voiceprint_files(roots: &[&Path]) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(rd) = fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
                continue;
            }
            let name = p
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_lowercase();
            let ext = p
                .extension()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_lowercase();
            let mut bad = FORBIDDEN_WORDS.iter().any(|w| name.contains(w))
                || FORBIDDEN_EXTENSIONS.contains(&ext.as_str());
            let texty = matches!(ext.as_str(), "json" | "txt" | "jsonl" | "md" | "toml");
            if !bad && texty && e.metadata().is_ok_and(|m| m.len() <= 8 * 1024 * 1024) {
                if let Ok(text) = fs::read_to_string(&p) {
                    let text = text.to_lowercase();
                    bad = FORBIDDEN_WORDS.iter().any(|w| text.contains(w));
                }
            }
            if bad {
                out.push(p);
            }
        }
    }
    let mut out = Vec::new();
    for r in roots {
        walk(r, &mut out);
    }
    out.sort();
    out
}

#[cfg(test)]
mod tests;
