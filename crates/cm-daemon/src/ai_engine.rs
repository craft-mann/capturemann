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
use cm_ai::recording::{transcribe_recording, TranscribeOptions};
use cm_ai::suggest::silence::{decode_mono_f32, silence_cuts, SilenceOptions, ANALYSIS_RATE};
use cm_ai::suggest::zoom::{suggest_zooms, ZoomOptions};
use cm_ai::suggest::{filler::filler_cuts, merge_cuts, CutSuggestion, ZoomSuggestion};
use cm_ai::transcribe::{is_cancelled, Transcriber, Transcript};
use cm_ai::whisper::WhisperTranscriber;
use cm_core::{CmError, EditList, MediaTime};
use cm_cursor::CursorLog;
use cm_library::rec::read_manifest;
use cm_library::rec_edit::load_edits;
use serde_json::Value;

use crate::ai::{
    AiServices, CaptionFormat, CaptionService, ModelService, SuggestService, TranscribeRequest,
    TranscribeService,
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
    AiServices {
        models: Arc::new(RealModels { root: root.clone() }),
        transcribe: Arc::new(RealTranscribe { root, engine }),
        captions: Arc::new(RealCaptions),
        suggest: Arc::new(RealSuggest),
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
        eprintln!("cm-daemon: {w}");
    }
    store
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
        // The model is resolved and loaded for this job only.
        let (id, path) = store(&self.root).resolve(Some(&req.model))?;
        let whisper;
        let engine: &dyn Transcriber = match &self.engine {
            Some(e) => e.as_ref(),
            None => {
                whisper = WhisperTranscriber::load(&path, &id, true)?;
                &whisper
            }
        };
        let language = req.language.as_deref();
        // The glossary's terms prime the model (AI-6a); the job replaces the wrong spellings.
        let prompt = cm_ai::glossary::initial_prompt(&req.glossary);
        let opts = TranscribeOptions {
            language,
            include_system: false,
            model_id: Some(&id),
            prompt: prompt.as_deref(),
        };
        match transcribe_recording(&req.bundle_dir, engine, &opts, progress) {
            Ok(t) => Ok(Some(t)),
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
            let file = t.get("file").and_then(Value::as_str)?;
            let start = t.get("start").and_then(Value::as_i64).unwrap_or(0);
            let path = bundle_dir.join(file);
            path.is_file().then(|| (path, MediaTime::from_ticks(start)))
        });
        let mut silence = Vec::new();
        if let Some((file, start)) = track {
            let tools = cm_media::tools::locate()?;
            let samples = decode_mono_f32(&tools.ffmpeg, &file, ANALYSIS_RATE)?;
            silence = silence_cuts(
                &samples,
                ANALYSIS_RATE,
                start,
                &manifest.gaps,
                &SilenceOptions::default(),
            )?;
        } else if transcript.is_none() {
            return Err(CmError::invalid_input(
                "This recording has no audio track to analyse",
            ));
        }
        let fillers = transcript.map(|t| filler_cuts(t, &[])).unwrap_or_default();
        Ok(merge_cuts(silence, fillers, &edits, &manifest.gaps))
    }
}
