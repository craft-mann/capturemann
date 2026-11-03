//! The real engine behind the AI actions: adapters from the service traits of [`crate::ai`] to
//! `cm-ai` (model store, whisper.cpp, captions, suggestions).
//!
//! Nothing here loads a model or touches the network until an action asks for it: the Whisper
//! context is built per `transcribe` job from the active model and dropped when the job ends, so
//! an app with no model installed starts instantly and holds no model memory. Downloads happen
//! only in [`ModelService::download`], which the daemon runs for an explicit `download_model`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use cm_ai::caption::{build_captions, map_lines_to_timeline, to_srt, to_txt, to_vtt, CaptionLine};
use cm_ai::model::{DownloadOutcome, DownloadProgress, ModelList, ModelStore};
use cm_ai::recording::TranscribeOptions;
use cm_ai::suggest::silence::{
    activity_spans, decode_mono_f32, silence_cuts, veto_speech, SilenceOptions, ANALYSIS_RATE,
};
use cm_ai::suggest::zoom::{suggest_zooms, ZoomOptions};
use cm_ai::suggest::{filler::filler_cuts, merge_cuts, CutSuggestion, ZoomSuggestion};
use cm_ai::text_model::engine::EngineManager;
use cm_ai::text_model::store::{ResolvedTextModel, TextModelList, TextModelStore};
use cm_ai::transcribe::{is_cancelled, Transcriber, Transcript};
use cm_ai::whisper::WhisperTranscriber;
use cm_core::{CmError, EditList, MediaTime};
use cm_cursor::CursorLog;
use cm_library::rec::read_manifest;
use cm_library::rec_edit::load_edits;
use serde_json::Value;

use crate::ai::{
    AiServices, CaptionFormat, CaptionService, DiarizeRequest, ModelService, SpeakerService,
    SuggestService, TextModelService, TranscribeRequest, TranscribeService, TranslateOutput,
    TranslateRequest, TranslateService,
};

/// The services backed by `cm-ai`. Plug into [`crate::DaemonBuilder::ai`].
pub fn real_services() -> AiServices {
    services(None, None)
}

/// [`real_services`] with a fixed model folder (instead of the config dir's) and, optionally, a
/// transcriber used in place of whisper.cpp. For tests and embedding: the model store, captions
/// and suggestions are still the real ones, and a model must still be installed to transcribe.
pub fn services_with(root: PathBuf, engine: Option<Arc<dyn Transcriber>>) -> AiServices {
    services(Some(root), engine)
}

fn services(root: Option<PathBuf>, engine: Option<Arc<dyn Transcriber>>) -> AiServices {
    let text_root = text_root(&root);
    AiServices {
        models: Arc::new(RealModels { root: root.clone() }),
        text_models: Arc::new(RealTextModels {
            root: text_root.clone(),
        }),
        // The helper (`llama-server`) starts on the first text request, not now.
        text_engine: EngineManager::system(text_root.join("engine.log")),
        transcribe: Arc::new(RealTranscribe {
            root: root.clone(),
            engine: engine.clone(),
        }),
        captions: Arc::new(RealCaptions),
        suggest: Arc::new(RealSuggest),
        translate: Arc::new(RealTranslate {
            root: root.clone(),
            engine,
        }),
        // AI-8 (spike S7): sherpa-onnx with the two speaker models from the model store. The
        // models are looked up on every run, so one installed while the app is open works at
        // once; without them `diarize` answers `no_speaker_model`.
        speakers: Arc::new(RealSpeakers {
            diarizer: Arc::new(cm_ai::speaker_engine::SherpaDiarizer::new(store(&root))),
        }),
    }
}

/// Speaker labels with a [`Diarizer`] (the seam where the real model plugs in).
struct RealSpeakers {
    diarizer: Arc<dyn cm_ai::speakers::Diarizer>,
}

impl SpeakerService for RealSpeakers {
    fn ready(&self) -> Result<(), CmError> {
        self.diarizer.ready()
    }

    fn diarize(
        &self,
        req: &DiarizeRequest,
        progress: &mut dyn FnMut(f32) -> bool,
    ) -> Result<Option<cm_ai::speakers::Diarized>, CmError> {
        self.diarizer.ready()?;
        let opts = cm_ai::speakers::DiarizeOptions {
            hints: req.hints,
            headset: req.headset,
            display_name: &req.display_name,
            created: &req.created,
        };
        match cm_ai::speakers::diarize_recording(
            &req.bundle_dir,
            self.diarizer.as_ref(),
            &opts,
            progress,
        ) {
            Ok(d) => Ok(Some(d)),
            Err(e) if is_cancelled(&e) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

/// Opens the store: `root` when given, else the config dir (`CAPTUREMANN_CONFIG_DIR` aware). A bad
/// catalog override only produces a warning on stderr; the built-in catalog is used.
fn store(root: &Option<PathBuf>) -> ModelStore {
    let (store, warning) = match root {
        Some(r) => ModelStore::open(r.clone()),
        None => ModelStore::open_default(),
    };
    if let Some(w) = warning {
        log::warn!("{w}");
    }
    store
}

/// The text-model store's folder: `<root>/text` for a fixed root, else `<config dir>/ai/text`.
fn text_root(root: &Option<PathBuf>) -> PathBuf {
    match root {
        Some(r) => r.join("text"),
        None => TextModelStore::default_root(),
    }
}

struct RealTextModels {
    root: PathBuf,
}

impl RealTextModels {
    fn store(&self) -> TextModelStore {
        let (store, warning) = TextModelStore::open(self.root.clone());
        if let Some(w) = warning {
            log::warn!("{w}");
        }
        store
    }
}

impl TextModelService for RealTextModels {
    fn list(&self) -> Result<TextModelList, CmError> {
        Ok(self.store().list())
    }

    fn download(
        &self,
        model: &str,
        progress: &mut dyn FnMut(DownloadProgress) -> bool,
    ) -> Result<bool, CmError> {
        match self.store().download(model, progress)? {
            DownloadOutcome::Done(_) => Ok(true),
            DownloadOutcome::Cancelled => Ok(false),
        }
    }

    fn select(&self, model: &str) -> Result<(), CmError> {
        self.store().select(model)
    }

    fn remove(&self, model: &str) -> Result<(), CmError> {
        self.store().remove(model).map(|_| ())
    }

    fn resolve(&self, requested: Option<&str>) -> Result<ResolvedTextModel, CmError> {
        self.store().resolve(requested)
    }
}

struct RealModels {
    root: Option<PathBuf>,
}

impl ModelService for RealModels {
    fn list(&self) -> Result<ModelList, CmError> {
        Ok(store(&self.root).list())
    }

    fn download(
        &self,
        model: &str,
        progress: &mut dyn FnMut(DownloadProgress) -> bool,
    ) -> Result<bool, CmError> {
        match store(&self.root).download(model, progress)? {
            DownloadOutcome::Done(_) => Ok(true),
            DownloadOutcome::Cancelled => Ok(false),
        }
    }

    fn import(&self, model: &str, path: &Path) -> Result<(), CmError> {
        store(&self.root).import(model, path).map(|_| ())
    }

    fn select(&self, model: &str) -> Result<(), CmError> {
        store(&self.root).select(model)
    }

    fn remove(&self, model: &str) -> Result<(), CmError> {
        store(&self.root).remove(model).map(|_| ())
    }
}

struct RealTranscribe {
    root: Option<PathBuf>,
    engine: Option<Arc<dyn Transcriber>>,
}

impl TranscribeService for RealTranscribe {
    fn transcribe(
        &self,
        req: &TranscribeRequest,
        progress: &mut dyn FnMut(f32) -> bool,
    ) -> Result<Option<Transcript>, CmError> {
        self.transcribe_resumable(req, &mut |_| {}, progress)
    }

    fn transcribe_resumable(
        &self,
        req: &TranscribeRequest,
        on_checkpoint: &mut dyn FnMut(serde_json::Value),
        progress: &mut dyn FnMut(f32) -> bool,
    ) -> Result<Option<Transcript>, CmError> {
        // The model is resolved and loaded for this job only.
        let (id, path) = store(&self.root).resolve(Some(&req.model))?;
        let whisper;
        let base: &dyn Transcriber = match &self.engine {
            Some(e) => e.as_ref(),
            None => {
                let mut w = WhisperTranscriber::load(&path, &id, true)?;
                if !req.pipeline.carry_previous_text {
                    w = w.with_n_max_text_ctx(0);
                }
                whisper = w;
                &whisper
            }
        };
        // The decoder options around the engine (each has a setting and an off switch).
        let vad = (req.pipeline.vad_opinion && req.pipeline.gate)
            .then(|| {
                let root = self.root.clone().unwrap_or_else(ModelStore::default_root);
                cm_ai::silero::vad_store(&root).path_of(cm_ai::silero::MODEL_ID)
            })
            .flatten()
            .and_then(|p| cm_ai::silero::SileroVad::load(&p).ok());
        let configured = cm_ai::pipeline::Configured {
            inner: base,
            vad: vad.as_ref(),
            gate: req.pipeline.gate.then(Default::default),
            normalize: req
                .pipeline
                .lift_quiet_input
                .then(cm_ai::pipeline::Normalize::default),
        };
        let engine: &dyn Transcriber = &configured;
        let language = req.language.as_deref();
        // The glossary's terms prime the model (AI-6a); the job replaces the wrong spellings.
        let prompt = cm_ai::glossary::initial_prompt(&req.glossary);
        let opts = TranscribeOptions {
            language,
            include_system: false,
            model_id: Some(&id),
            prompt: prompt.as_deref(),
            original_audio: req.original_audio,
            variant: &format!("{:?}", req.pipeline),
        };
        match cm_ai::recording::transcribe_recording_resumable(
            &req.bundle_dir,
            engine,
            &opts,
            req.resume.as_ref(),
            on_checkpoint,
            progress,
        ) {
            Ok(t) => Ok(Some(t)),
            Err(e) if is_cancelled(&e) => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn live_step(
        &self,
        req: &crate::ai::LiveRequest,
        run: &mut Option<cm_ai::recording::live::LiveRun>,
        progress: &mut dyn FnMut(f32) -> bool,
    ) -> Result<usize, CmError> {
        let prompt = cm_ai::glossary::initial_prompt(&req.glossary);
        let (id, path) = store(&self.root).resolve(Some(&req.model))?;
        if run.is_none() {
            *run = cm_ai::recording::live::LiveRun::open(
                &req.bundle_dir,
                &id,
                req.language.as_deref(),
                prompt.as_deref(),
                false,
                &format!("{:?}", req.pipeline),
            );
        }
        let Some(r) = run.as_mut() else {
            return Ok(0);
        };
        let whisper;
        let engine: &dyn Transcriber = match &self.engine {
            Some(e) => e.as_ref(),
            None => {
                // A background job beside a recording: two threads, so the recorder's audio path
                // is never starved (the GPU does the work).
                let mut w = WhisperTranscriber::load(&path, &id, true)?.with_threads(2);
                if !req.pipeline.carry_previous_text {
                    w = w.with_n_max_text_ctx(0);
                }
                whisper = w;
                &whisper
            }
        };
        let vad = (req.pipeline.vad_opinion && req.pipeline.gate)
            .then(|| {
                let root = self.root.clone().unwrap_or_else(ModelStore::default_root);
                cm_ai::silero::vad_store(&root).path_of(cm_ai::silero::MODEL_ID)
            })
            .flatten()
            .and_then(|p| cm_ai::silero::SileroVad::load(&p).ok());
        let configured = cm_ai::pipeline::Configured {
            inner: engine,
            vad: vad.as_ref(),
            gate: req.pipeline.gate.then(Default::default),
            normalize: req
                .pipeline
                .lift_quiet_input
                .then(cm_ai::pipeline::Normalize::default),
        };
        match r.step(&configured, progress) {
            Err(e) if is_cancelled(&e) => Ok(0),
            other => other,
        }
    }
}

/// Caption translation (AI-5). Only the Whisper engine exists here: the text model is the AI-7
/// provider, so [`TranslateService::has_text_model`] stays false and a non-English target fails
/// with `no_text_model`.
struct RealTranslate {
    root: Option<PathBuf>,
    engine: Option<Arc<dyn Transcriber>>,
}

impl TranslateService for RealTranslate {
    fn whisper(
        &self,
        req: &TranslateRequest,
        progress: &mut dyn FnMut(f32) -> bool,
    ) -> Result<Option<TranslateOutput>, CmError> {
        // The model is resolved and loaded for this job only, like a transcription. Turbo cannot
        // translate; the daemon never asks for it, and this refuses it all the same.
        let (id, path) = store(&self.root).resolve(Some(&req.model))?;
        if !cm_ai::translate::model_can_translate(&id) {
            return Err(cm_ai::translate::turbo_cannot_translate(&id));
        }
        let whisper;
        let engine: &dyn Transcriber = match &self.engine {
            Some(e) => e.as_ref(),
            None => {
                whisper = WhisperTranscriber::load(&path, &id, true)?;
                &whisper
            }
        };
        match cm_ai::translate::translate_with_whisper(
            &req.bundle_dir,
            engine,
            Some(&req.source_language),
            &id,
            &req.source_lines,
            progress,
        ) {
            Ok(lines) => Ok(Some(TranslateOutput {
                lines,
                model: id,
                untranslated: 0,
            })),
            Err(e) if is_cancelled(&e) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

struct RealCaptions;

impl CaptionService for RealCaptions {
    fn lines(&self, transcript: &Transcript) -> Result<Vec<CaptionLine>, CmError> {
        Ok(build_captions(&transcript.words))
    }

    fn render(
        &self,
        lines: &[CaptionLine],
        format: CaptionFormat,
        edits: &EditList,
    ) -> Result<String, CmError> {
        // Cuts remove lines and shift later ones, speed scales times (AI-1.AC4).
        let mapped = map_lines_to_timeline(lines, &edits.timeline());
        Ok(match format {
            CaptionFormat::Srt => to_srt(&mapped),
            CaptionFormat::Vtt => to_vtt(&mapped),
            CaptionFormat::Txt => to_txt(&mapped, false),
            CaptionFormat::TxtTimed => to_txt(&mapped, true),
            CaptionFormat::Md => cm_ai::speakers::to_markdown_speakers(
                &mapped
                    .into_iter()
                    .map(|line| cm_ai::speakers::SpokenLine {
                        line,
                        speaker: None,
                    })
                    .collect::<Vec<_>>(),
            ),
        })
    }
}

struct RealSuggest;

/// The cursor log of a bundle, `invalid_input` when the cursor was not recorded.
fn cursor_log(
    bundle: &Path,
    manifest: &cm_library::manifest::RecManifest,
) -> Result<CursorLog, CmError> {
    let file = manifest
        .tracks
        .get("cursor")
        .and_then(|t| t.get("file"))
        .and_then(Value::as_str)
        .unwrap_or("cursor.jsonl");
    let path = bundle.join(file);
    if !path.is_file() {
        return Err(CmError::invalid_input("This recording has no cursor data"));
    }
    CursorLog::read(&path)
}

/// The noise-reduced microphone file when the track keeps the raw one beside it.
fn noise_reduced_copy(
    bundle: &Path,
    manifest: &cm_library::manifest::RecManifest,
) -> Option<PathBuf> {
    let t = manifest.tracks.get("mic")?;
    let (file, raw) = (
        t.get("file").and_then(Value::as_str)?,
        t.get("raw_file").and_then(Value::as_str)?,
    );
    let path = bundle.join(file);
    (file != raw && path.is_file()).then_some(path)
}

impl SuggestService for RealSuggest {
    fn zooms(&self, bundle_dir: &Path) -> Result<Vec<ZoomSuggestion>, CmError> {
        let manifest = read_manifest(bundle_dir)?;
        let log = cursor_log(bundle_dir, &manifest)?;
        let edits = load_edits(&manifest);
        suggest_zooms(&log, &edits, &manifest.gaps, &ZoomOptions::default())
    }

    fn cuts(
        &self,
        bundle_dir: &Path,
        transcript: Option<&Transcript>,
    ) -> Result<Vec<CutSuggestion>, CmError> {
        let manifest = read_manifest(bundle_dir)?;
        let edits = load_edits(&manifest);
        // Silence from the microphone, or from system audio when there is no microphone.
        let track = ["mic", "system"].iter().find_map(|name| {
            let t = manifest.tracks.get(*name)?;
            if t.get("missing").and_then(Value::as_bool) == Some(true) {
                return None;
            }
            // The pauses are judged on the track as recorded when there is one: a noise-reduced
            // copy pushes quiet stretches (and quiet speech) down, which hides where the room
            // noise and the speech really are.
            let start = t.get("start").and_then(Value::as_i64).unwrap_or(0);
            ["raw_file", "file"].iter().find_map(|key| {
                let path = bundle_dir.join(t.get(*key).and_then(Value::as_str)?);
                path.is_file().then(|| (path, MediaTime::from_ticks(start)))
            })
        });
        let mut silence = Vec::new();
        if let Some((file, start)) = track {
            let tools = cm_media::tools::locate()?;
            let samples = decode_mono_f32(&tools.ffmpeg, &file, ANALYSIS_RATE)?;
            let opts = SilenceOptions::default();
            silence = silence_cuts(&samples, ANALYSIS_RATE, start, &manifest.gaps, &opts)?;
            // Where the transcript or the speaker engine says someone speaks, no pause.
            let mut speech: Vec<cm_core::edit::Span> = transcript
                .map(|t| {
                    t.words
                        .iter()
                        .map(|w| cm_core::edit::Span::new(w.start, w.end))
                        .collect()
                })
                .unwrap_or_default();
            if let Some(v) = manifest.extra.get(cm_ai::speakers::MANIFEST_SEGMENTS) {
                if let Ok(segs) = serde_json::from_value::<Vec<cm_ai::speakers::Segment>>(v.clone())
                {
                    speech.extend(
                        segs.iter()
                            .map(|s| cm_core::edit::Span::new(s.start, s.end)),
                    );
                }
            }
            // The noise-reduced copy, when the track has one besides the raw file: steady room noise
            // is gone there, so a stretch well over its own floor is something speech-like that the
            // raw level rule and the transcript may both have missed (a quiet speaker).
            if let Some(den) = noise_reduced_copy(bundle_dir, &manifest) {
                let x = decode_mono_f32(&tools.ffmpeg, &den, ANALYSIS_RATE)?;
                speech.extend(activity_spans(
                    &x,
                    ANALYSIS_RATE,
                    start,
                    &manifest.gaps,
                    20.0,
                    0.2,
                ));
            }
            silence = veto_speech(silence, &speech, &opts);
        } else if transcript.is_none() {
            return Err(CmError::invalid_input(
                "This recording has no audio track to analyse",
            ));
        }
        let fillers = transcript.map(|t| filler_cuts(t, &[])).unwrap_or_default();
        Ok(merge_cuts(silence, fillers, &edits, &manifest.gaps))
    }
}
