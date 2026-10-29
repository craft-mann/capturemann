//! The real speaker engine (AI-8, spike S7): sherpa-onnx's offline diarization (pyannote
//! segmentation-3.0, then CAM++ Chinese-English voice embeddings, then clustering) behind the [`Diarizer`] trait.
//!
//! * [`SherpaDiarizer`] finds the two model files in the model store ([`SEGMENTATION_ID`] and
//!   [`EMBEDDING_ID`] rows of `data/speaker_models.json`) every time it runs: a copy in the user's
//!   models folder, else the pair that ships inside the app (SHA-256 checked once per process). A
//!   missing pair answers [`no_speaker_model`]. It never downloads anything.
//! * The engine itself sits behind [`TurnEngine`]. [`SherpaEngine`] (feature `sherpa`) is the real
//!   one; tests use a scripted engine, so everything around it (the hint rules, folding tiny
//!   clusters, flicker clean-up) is tested without the libraries or the models.
//! * **The hint matters.** Spike S7: on a 2 to 3 person recording the library's default clustering
//!   threshold found 29 speakers. So `num_speakers` (the Record sheet's "How many people?" and the
//!   Speakers tab's choice) is passed as the exact cluster count. Without it the engine runs with
//!   a high threshold ([`AUTO_THRESHOLD`]) and folds clusters that hold almost no speech into the
//!   nearest voice; that count is a best-effort suggestion and the user can merge speakers.
//! * Overlapping turns are kept as the engine reports them; the louder speaker wins when words are
//!   assigned ([`crate::speakers`]). Turns shorter than [`FLICKER_SECS`] that sit inside one other
//!   speaker's talk are given to that speaker and same-speaker turns closer than [`MERGE_GAP_SECS`]
//!   are joined ([`tidy_turns`]).
//! * Voiceprints (embeddings) stay inside the engine call. Nothing here writes a file (AI-8.AC6).
//!
//! Progress: the engine has one blocking call, so progress is 0 then 1, and a cancel is noticed
//! before and after the call.

use std::path::PathBuf;
use std::sync::Arc;

use cm_core::CmError;

use crate::model::ModelStore;
use crate::speakers::{no_speaker_model, DiarizeHints, Diarizer, Segment};
use crate::transcribe::{cancelled, media_secs, Pcm16k};

/// Catalogue id of the segmentation model file.
pub const SEGMENTATION_ID: &str = "speaker-segmentation";
/// Catalogue id of the voice-embedding model file.
pub const EMBEDDING_ID: &str = "speaker-embedding";
/// The model id written to `diarization.json`.
pub const MODEL_ID: &str = "sherpa-onnx-2.0.1+pyannote-segmentation-3.0+campplus-zh-en-advanced";

/// Clustering threshold when the number of people is not given. The library default (0.5) found 29
/// speakers on a 2 to 3 person file with the English-only voice model (S7), so this is deliberately
/// high and the tiny clusters it leaves are folded ([`fold_small_speakers`]). With the Chinese-English
/// CAM++ model the best value on 8 English clips was 1.0, and 1.2 already merged everybody into one
/// or two speakers (`verification/benchmarks/speaker-detection-2026-10-29.md`: mean DER 22 % at 1.0
/// against 49 % at 1.2; 17 % on 6 clips held back). The edge is close, so do not raise it without
/// measuring. A suggestion, not a promise.
pub const AUTO_THRESHOLD: f32 = 1.0;
/// A speaker with less speech than this (seconds) or [`FOLD_FRACTION`] of all speech is folded
/// into the nearest other voice when the count is automatic.
pub const FOLD_MIN_SECS: f64 = 8.0;
/// See [`FOLD_MIN_SECS`].
pub const FOLD_FRACTION: f64 = 0.04;
/// A turn shorter than this, between turns of one other speaker, is a flicker (seconds).
pub const FLICKER_SECS: f64 = 0.5;
/// Turns of one speaker closer than this are joined (seconds).
pub const MERGE_GAP_SECS: f64 = 0.5;
/// Audio shorter than this has nothing to cluster (seconds).
const MIN_AUDIO_SECS: f64 = 1.0;
/// Most speakers an automatic run keeps (`max_speakers` lowers it).
pub const AUTO_MAX_SPEAKERS: u32 = 8;

/// The two model files of the engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpeakerModels {
    pub segmentation: PathBuf,
    pub embedding: PathBuf,
}

/// The paths of the installed speaker models, `None` unless both are installed. "Installed" means
/// a copy in the user's models folder or the one that ships with the app (the user's copy wins;
/// a bundled file that fails its SHA-256 check does not count, see [`resolve_models`]).
pub fn speaker_models(store: &ModelStore) -> Option<SpeakerModels> {
    Some(SpeakerModels {
        segmentation: store.path_of(SEGMENTATION_ID)?,
        embedding: store.path_of(EMBEDDING_ID)?,
    })
}

/// Like [`speaker_models`], but says why when they cannot be used: a damaged bundled file gives
/// its own error (`internal`), anything else missing gives [`no_speaker_model`].
pub fn resolve_models(store: &ModelStore) -> Result<SpeakerModels, CmError> {
    if let Some(m) = speaker_models(store) {
        return Ok(m);
    }
    for id in [SEGMENTATION_ID, EMBEDDING_ID] {
        if let Some(problem) = store.bundled_problem(id) {
            return Err(CmError::internal(problem));
        }
    }
    Err(no_speaker_model())
}

/// One turn as the engine reports it (seconds from the start of the audio; the speaker is a
/// cluster number).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Turn {
    pub start: f64,
    pub end: f64,
    pub speaker: i32,
}

/// How to cluster: an exact count, or (count `None`) a distance threshold.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClusterParams {
    pub num_clusters: Option<u32>,
    pub threshold: f32,
}

/// The engine seam under [`SherpaDiarizer`]: 16 kHz mono samples in, turns out.
///
/// `progress` gets 0.0 to 1.0 for this run and returns `false` to stop it; a stopped run answers
/// [`cancelled`]. The real engine passes the stop to sherpa-onnx, which (2.0.1) ignores it and
/// finishes the run first, so the cancelled answer comes when the run would have ended anyway.
pub trait TurnEngine: Send + Sync {
    fn run(
        &self,
        models: &SpeakerModels,
        samples: &[f32],
        params: ClusterParams,
        progress: &mut dyn FnMut(f32) -> bool,
    ) -> Result<Vec<Turn>, CmError>;
}

/// A [`Diarizer`] backed by the speaker models in a [`ModelStore`].
pub struct SherpaDiarizer {
    store: ModelStore,
    engine: Arc<dyn TurnEngine>,
}

impl SherpaDiarizer {
    /// The production diarizer: sherpa-onnx when this build has it, else an engine that says so.
    pub fn new(store: ModelStore) -> SherpaDiarizer {
        SherpaDiarizer {
            store,
            engine: default_engine(),
        }
    }

    /// With another engine (tests).
    pub fn with_engine(store: ModelStore, engine: Arc<dyn TurnEngine>) -> SherpaDiarizer {
        SherpaDiarizer { store, engine }
    }
}

impl Diarizer for SherpaDiarizer {
    fn model_id(&self) -> String {
        MODEL_ID.to_owned()
    }

    fn ready(&self) -> Result<(), CmError> {
        resolve_models(&self.store).map(|_| ())
    }

    fn diarize(
        &self,
        audio: Pcm16k<'_>,
        hints: &DiarizeHints,
        progress: &mut dyn FnMut(f32) -> bool,
    ) -> Result<Vec<Segment>, CmError> {
        let models = resolve_models(&self.store)?;
        if !progress(0.0) {
            return Err(cancelled());
        }
        let seconds = audio.samples.len() as f64 / 16_000.0;
        let turns = if seconds < MIN_AUDIO_SECS {
            Vec::new()
        } else {
            run_with_hints(
                self.engine.as_ref(),
                &models,
                audio.samples,
                hints,
                progress,
            )?
        };
        if !progress(1.0) {
            return Err(cancelled());
        }
        Ok(turns
            .into_iter()
            .map(|t| Segment {
                start: media_secs(t.start) + audio.start,
                end: media_secs(t.end) + audio.start,
                speaker: u32::try_from(t.speaker.max(0)).unwrap_or(0),
                energy: 0.0,
            })
            .collect())
    }
}

/// The clustering rules around one engine: an exact `num_speakers` wins; otherwise an automatic
/// pass (high threshold, tiny clusters folded) and, if the result is outside `min_speakers` /
/// `max_speakers` (default maximum [`AUTO_MAX_SPEAKERS`]), one more pass with that exact count.
/// The result is tidied ([`tidy_turns`]).
pub fn run_with_hints(
    engine: &dyn TurnEngine,
    models: &SpeakerModels,
    samples: &[f32],
    hints: &DiarizeHints,
    progress: &mut dyn FnMut(f32) -> bool,
) -> Result<Vec<Turn>, CmError> {
    let exact = |n: u32| ClusterParams {
        num_clusters: Some(n),
        threshold: AUTO_THRESHOLD,
    };
    let turns = if let Some(n) = hints.num_speakers {
        engine.run(models, samples, exact(n), progress)?
    } else {
        let auto = ClusterParams {
            num_clusters: None,
            threshold: AUTO_THRESHOLD,
        };
        // Most runs end with this pass, so it takes nearly all of the bar; the rare second pass
        // below keeps the bar where it is (progress never goes back) and can still be stopped.
        let first = fold_small_speakers(engine.run(models, samples, auto, &mut |f| {
            progress(f * FIRST_PASS_SHARE)
        })?);
        let count = speaker_count(&first);
        let max = hints.max_speakers.unwrap_or(AUTO_MAX_SPEAKERS);
        let min = hints.min_speakers.unwrap_or(1);
        let mut hold = |_: f32| progress(FIRST_PASS_SHARE);
        if count > max {
            engine.run(models, samples, exact(max), &mut hold)?
        } else if count < min {
            engine.run(models, samples, exact(min), &mut hold)?
        } else {
            first
        }
    };
    Ok(tidy_turns(turns))
}

/// The share of the progress bar the first pass of an automatic run takes (see [`run_with_hints`]).
const FIRST_PASS_SHARE: f32 = 0.95;

fn speaker_count(turns: &[Turn]) -> u32 {
    let mut ids: Vec<i32> = turns.iter().map(|t| t.speaker).collect();
    ids.sort_unstable();
    ids.dedup();
    u32::try_from(ids.len()).unwrap_or(u32::MAX)
}

/// Gives the turns of speakers who hold almost no speech (under [`FOLD_MIN_SECS`] seconds or
/// [`FOLD_FRACTION`] of all speech) to the speaker of the nearest turn in time that belongs to a
/// kept speaker. When every speaker is that small, nothing changes.
pub fn fold_small_speakers(mut turns: Vec<Turn>) -> Vec<Turn> {
    use std::collections::BTreeMap;
    let mut total: BTreeMap<i32, f64> = BTreeMap::new();
    for t in &turns {
        *total.entry(t.speaker).or_default() += (t.end - t.start).max(0.0);
    }
    let all: f64 = total.values().sum();
    let limit = FOLD_MIN_SECS.max(all * FOLD_FRACTION);
    let kept: Vec<i32> = total
        .iter()
        .filter(|(_, d)| **d >= limit)
        .map(|(s, _)| *s)
        .collect();
    if kept.is_empty() || kept.len() == total.len() {
        return turns;
    }
    let anchors: Vec<Turn> = turns
        .iter()
        .filter(|t| kept.contains(&t.speaker))
        .copied()
        .collect();
    for t in &mut turns {
        if kept.contains(&t.speaker) {
            continue;
        }
        let mid = (t.start + t.end) / 2.0;
        let distance = |a: &Turn| {
            if mid < a.start {
                a.start - mid
            } else if mid > a.end {
                mid - a.end
            } else {
                0.0
            }
        };
        if let Some(near) = anchors
            .iter()
            .min_by(|a, b| distance(a).total_cmp(&distance(b)))
        {
            t.speaker = near.speaker;
        }
    }
    turns
}

/// Removes flicker and joins close turns of one speaker: a turn shorter than [`FLICKER_SECS`]
/// whose neighbours (the turns starting just before and just after it, each within
/// [`MERGE_GAP_SECS`]) belong to one other speaker is given to that speaker; then the turns of
/// each speaker that are less than [`MERGE_GAP_SECS`] apart are joined. Output is sorted by start.
/// Overlaps between different speakers stay (the louder speaker wins at word assignment).
pub fn tidy_turns(mut turns: Vec<Turn>) -> Vec<Turn> {
    turns.retain(|t| t.end > t.start);
    turns.sort_by(|a, b| a.start.total_cmp(&b.start).then(a.end.total_cmp(&b.end)));
    // Flicker: decided on the original sequence, then applied.
    let mut relabel: Vec<(usize, i32)> = Vec::new();
    for i in 0..turns.len() {
        let t = turns[i];
        if t.end - t.start >= FLICKER_SECS {
            continue;
        }
        let prev_any = i.checked_sub(1).map(|j| turns[j]);
        let next_any = turns.get(i + 1).copied();
        let (Some(p), Some(n)) = (prev_any, next_any) else {
            continue;
        };
        if p.speaker == n.speaker
            && p.speaker != t.speaker
            && t.start - p.end <= MERGE_GAP_SECS
            && n.start - t.end <= MERGE_GAP_SECS
        {
            relabel.push((i, p.speaker));
        }
    }
    for (i, s) in relabel {
        turns[i].speaker = s;
    }
    // Join close turns of one speaker.
    let mut by_speaker: std::collections::BTreeMap<i32, Vec<Turn>> = Default::default();
    for t in turns {
        by_speaker.entry(t.speaker).or_default().push(t);
    }
    let mut out: Vec<Turn> = Vec::new();
    for (_, list) in by_speaker {
        let mut cur: Option<Turn> = None;
        for t in list {
            match cur {
                Some(ref mut c) if t.start - c.end <= MERGE_GAP_SECS => c.end = c.end.max(t.end),
                _ => {
                    if let Some(c) = cur.take() {
                        out.push(c);
                    }
                    cur = Some(t);
                }
            }
        }
        out.extend(cur);
    }
    out.sort_by(|a, b| a.start.total_cmp(&b.start).then(a.end.total_cmp(&b.end)));
    out
}

// ---- the engines --------------------------------------------------------------------------

#[cfg(feature = "sherpa")]
fn default_engine() -> Arc<dyn TurnEngine> {
    Arc::new(SherpaEngine::default())
}

#[cfg(not(feature = "sherpa"))]
fn default_engine() -> Arc<dyn TurnEngine> {
    Arc::new(MissingEngine)
}

/// The engine of a build without the `sherpa` feature.
#[cfg(not(feature = "sherpa"))]
struct MissingEngine;

#[cfg(not(feature = "sherpa"))]
impl TurnEngine for MissingEngine {
    fn run(
        &self,
        _: &SpeakerModels,
        _: &[f32],
        _: ClusterParams,
        _: &mut dyn FnMut(f32) -> bool,
    ) -> Result<Vec<Turn>, CmError> {
        Err(CmError::invalid_input(
            "This build of CaptureMann has no speaker engine, so speakers cannot be told apart.",
        ))
    }
}

/// Turns the engine's chunk counts into one rising fraction. The engine counts chunks twice, once
/// while it finds speech (segmentation) and once while it computes the voice prints (embeddings),
/// and starts again from 0 for the second step; the clustering at the end reports nothing, so the
/// bar stops short of 1.0 (the caller sets 1.0 when the run returns).
#[derive(Debug, Default)]
#[cfg_attr(not(feature = "sherpa"), allow(dead_code))]
struct ProgressBar {
    steps_done: u32,
    last_done: i32,
    best: f32,
}

#[cfg_attr(not(feature = "sherpa"), allow(dead_code))]
impl ProgressBar {
    /// The two counted steps, each half of the bar, with the last 2 % left for the clustering.
    fn fraction(&mut self, done: i32, total: i32) -> f32 {
        if done < self.last_done {
            self.steps_done += 1;
        }
        self.last_done = done;
        let within = if total > 0 {
            (done as f32 / total as f32).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let f = ((self.steps_done as f32 + within) / 2.0).min(1.0) * 0.98;
        self.best = self.best.max(f);
        self.best
    }
}

/// sherpa-onnx offline diarization (feature `sherpa`).
#[cfg(feature = "sherpa")]
#[derive(Debug, Clone, Default)]
pub struct SherpaEngine {
    /// Threads for each model; 0 picks `min(4, cores)`.
    pub threads: i32,
}

#[cfg(feature = "sherpa")]
impl TurnEngine for SherpaEngine {
    fn run(
        &self,
        models: &SpeakerModels,
        samples: &[f32],
        params: ClusterParams,
        progress: &mut dyn FnMut(f32) -> bool,
    ) -> Result<Vec<Turn>, CmError> {
        use sherpa_onnx::{
            FastClusteringConfig, OfflineSpeakerDiarization, OfflineSpeakerDiarizationConfig,
            OfflineSpeakerSegmentationModelConfig, OfflineSpeakerSegmentationPyannoteModelConfig,
            SpeakerEmbeddingExtractorConfig,
        };
        let threads = if self.threads > 0 {
            self.threads
        } else {
            std::thread::available_parallelism()
                .map_or(2, |n| n.get().min(4))
                .try_into()
                .unwrap_or(2)
        };
        let path = |p: &PathBuf| p.to_str().map(str::to_owned);
        let (Some(seg), Some(emb)) = (path(&models.segmentation), path(&models.embedding)) else {
            return Err(CmError::invalid_input(
                "The speaker model path is not valid text",
            ));
        };
        let config = OfflineSpeakerDiarizationConfig {
            segmentation: OfflineSpeakerSegmentationModelConfig {
                pyannote: OfflineSpeakerSegmentationPyannoteModelConfig {
                    model: Some(seg),
                    window_shift_ratio: 0.1,
                },
                num_threads: threads,
                ..Default::default()
            },
            embedding: SpeakerEmbeddingExtractorConfig {
                model: Some(emb),
                num_threads: threads,
                ..Default::default()
            },
            clustering: FastClusteringConfig {
                num_clusters: params
                    .num_clusters
                    .map_or(-1, |n| i32::try_from(n).unwrap_or(i32::MAX)),
                threshold: params.threshold,
                compute_confidence: false,
            },
            min_duration_on: 0.3,
            min_duration_off: 0.5,
        };
        let engine = OfflineSpeakerDiarization::create(&config).ok_or_else(|| {
            CmError::invalid_input(
                "The speaker models could not be loaded. Reinstall CaptureMann; if you imported a copy of a model, remove it with `remove_model`.",
            )
        })?;
        if engine.sample_rate() != 16_000 {
            return Err(CmError::internal(format!(
                "the speaker model wants {} Hz audio, not 16000",
                engine.sample_rate()
            )));
        }
        let mut stopped = false;
        let mut bar = ProgressBar::default();
        let result = engine.process_with_progress(samples, &mut |done, total| {
            if !progress(bar.fraction(done, total)) {
                stopped = true;
            }
            !stopped
        });
        if stopped {
            return Err(cancelled());
        }
        let result =
            result.ok_or_else(|| CmError::internal("the speaker engine failed on this audio"))?;
        Ok(result
            .sort_by_start_time()
            .into_iter()
            .map(|s| Turn {
                start: f64::from(s.start),
                end: f64::from(s.end),
                speaker: s.speaker,
            })
            .collect())
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod bundled_tests;
