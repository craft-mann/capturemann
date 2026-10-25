//! The local-AI actions (M5, AI-1, AI-3, AI-4): speech models, transcripts, captions and
//! suggestions, as daemon actions, MCP tools and CLI commands.
//!
//! | Action | Tier | What it does |
//! | --- | --- | --- |
//! | `list_models` | Free | the models, which are installed, which is active |
//! | `download_model { model }` | **Confirm** | a `model_download` job; the only thing here that uses the network |
//! | `import_model { model, path }` | Free | copies a model file the user already has |
//! | `select_model { model }` | Free | the model `transcribe` uses by default |
//! | `remove_model { model }` | **Confirm** | deletes a model file |
//! | `transcribe { capture_id, language?, model? }` | Free | a `transcribe` job; `transcript.json` in the bundle |
//! | `get_transcript { capture_id }` | Free | the transcript and the caption lines |
//! | `set_captions { capture_id, lines }` | Free | edited caption lines: one revision, one undo step |
//! | `replace_in_captions { capture_id, replacements }` | Free | find and replace over the caption lines (AI-6a): one revision, one undo step |
//! | `export_captions { capture_id, format }` | Free | SRT or VTT in `exports/`, mapped through the edit list |
//! | `suggest_zooms`, `suggest_cuts` | Free | suggestions only; accepting uses `set_zooms` / `cut_segments` |
//!
//! # Nothing is downloaded silently
//!
//! A model is fetched only by `download_model` (Confirm for agents; the app's AI page asks the user
//! itself). `transcribe` without an installed model fails with `invalid_input` and
//! `data.reason = "no_model"`, whose message points to the AI page and `capturemann ai models`.
//! The automatic transcription after a recording ([`Daemon::start_ai_auto_transcribe`], setting
//! `ai.auto_transcribe`, default on only with 16 GB of RAM or more) never starts without an
//! installed model, and never touches the network.
//!
//! # The engine behind it
//!
//! The model store, the Whisper engine, caption building and the suggestion algorithms live in
//! `cm-ai` and are plugged in with [`AiServices`] ([`crate::DaemonBuilder::ai`]); the traits below
//! are the whole surface the daemon needs, and tests use scripted fakes. [`AiServices::default`]
//! answers every call with `not_implemented` and a hint, and the Confirm-tier actions are refused
//! before any dialog opens, so a daemon without an engine never asks for something it cannot do.
//!
//! # Events
//!
//! `model.download.progress` (`{ model, bytes_done, bytes_total, fraction }`), `model.changed`
//! (`{ model, change }`, `change` one of `downloaded`, `imported`, `selected`, `removed`) and
//! `transcript.ready` (`{ capture_id, model, language, words, replaced, captions_cleared,
//! captions_shown, reapplied, dropped, glossary_replaced }`); jobs also emit the usual
//! `job.progress`, `job.done` and `job.failed`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use cm_ai::caption::CaptionLine;
use cm_ai::model::{DownloadProgress, ModelInfo, ModelList};
use cm_ai::suggest::{CutSuggestion, ZoomSuggestion};
use cm_ai::transcribe::Transcript;
use cm_core::edit::{parse_time, secs};
use cm_core::{Actor, CmError, EditList};
use cm_jobs::{Job, JobContext, JobFilter, JobHandler, JobQueue, JobState, NewJob, JOB_TRANSCRIBE};
use cm_library::manifest::CaptureKind;
use serde::{Serialize, Serializer};
use serde_json::{json, Map, Value};

use crate::action::{Action, Fields};
use crate::daemon::{blocking, lock_library, Daemon, Inner};
use crate::error::DaemonError;
use crate::events::{self, EventBus};

/// Job type of a user-started model download.
pub const JOB_MODEL_DOWNLOAD: &str = "model_download";
/// The transcript in the bundle folder.
pub const TRANSCRIPT_FILE: &str = "transcript.json";
/// Setting: transcribe after every recording (default on only with at least [`AUTO_MIN_RAM_GB`]).
pub const SETTING_AUTO_TRANSCRIBE: &str = "ai.auto_transcribe";
/// RAM the automatic transcription needs by default (AI-1).
pub const AUTO_MIN_RAM_GB: u64 = 16;

/// Most caption lines one `set_captions` call may carry.
pub const MAX_CAPTION_LINES: usize = 5000;
/// Caption rules (AI-1): rows per line, characters per row, longest line in seconds.
pub const MAX_CAPTION_ROWS: usize = 2;
pub const MAX_CAPTION_ROW_CHARS: usize = 42;
pub const MAX_CAPTION_SECONDS: f64 = 7.0;

/// Most replacement pairs one `replace_in_captions` call may carry, and the longest `find` or
/// `replace` text (characters).
pub const MAX_REPLACEMENTS: usize = 100;
pub const MAX_REPLACE_CHARS: usize = 200;

/// The action names of this module, as MCP tools and CLI commands.
pub const AI_METHODS: &[&str] = &[
    "list_models",
    "download_model",
    "import_model",
    "select_model",
    "remove_model",
    "transcribe",
    "get_transcript",
    "get_captions",
    "set_captions",
    "replace_in_captions",
    "export_captions",
    "suggest_zooms",
    "suggest_cuts",
];

/// Whether `name` is one of [`AI_METHODS`].
pub fn is_ai_method(name: &str) -> bool {
    AI_METHODS.contains(&name)
}

// ---- the services --------------------------------------------------------------------------

/// Caption file formats of `export_captions`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptionFormat {
    Srt,
    Vtt,
    /// Plain text, one caption line per row, no timestamps.
    Txt,
    /// Plain text with a `[mm:ss]` (or `[hh:mm:ss]`) prefix on every line.
    TxtTimed,
}

impl CaptionFormat {
    pub fn ext(self) -> &'static str {
        match self {
            CaptionFormat::Srt => "srt",
            CaptionFormat::Vtt => "vtt",
            CaptionFormat::Txt | CaptionFormat::TxtTimed => "txt",
        }
    }

    pub fn is_text(self) -> bool {
        matches!(self, CaptionFormat::Txt | CaptionFormat::TxtTimed)
    }
}

/// What a transcription needs: the bundle (the service finds and decodes the audio) and choices.
#[derive(Debug, Clone, PartialEq)]
pub struct TranscribeRequest {
    pub capture_id: String,
    pub bundle_dir: PathBuf,
    /// `None`: detect the language.
    pub language: Option<String>,
    /// An installed model id.
    pub model: String,
    /// The user's glossary (setting `ai.glossary`): the service primes the model with its terms
    /// ([`cm_ai::glossary::initial_prompt`]). The job replaces the heard-as spellings afterwards.
    pub glossary: Vec<cm_ai::glossary::GlossaryEntry>,
}

/// The speech-model store (`cm_ai::model`). Blocking: the daemon calls it off the async threads.
pub trait ModelService: Send + Sync {
    /// Whether this build can manage models at all.
    fn available(&self) -> bool {
        true
    }
    fn list(&self) -> Result<ModelList, CmError>;
    /// Downloads a model, resuming a partial file. `progress` returns `false` to interrupt:
    /// the service stops, keeps the partial file for the next try and returns `Ok(false)`.
    /// `Ok(true)` means the model is installed and verified. Network failures are
    /// `CmError::network` (retried by the queue).
    fn download(
        &self,
        model: &str,
        progress: &mut dyn FnMut(DownloadProgress) -> bool,
    ) -> Result<bool, CmError>;
    /// Installs a model from a file the user has (checks it is a usable model).
    fn import(&self, model: &str, path: &Path) -> Result<(), CmError>;
    fn select(&self, model: &str) -> Result<(), CmError>;
    fn remove(&self, model: &str) -> Result<(), CmError>;
}

/// Runs a transcription (`cm_ai::transcribe`): decodes the audio of the bundle and calls the
/// model. Blocking.
pub trait TranscribeService: Send + Sync {
    fn available(&self) -> bool {
        true
    }
    /// `progress` gets 0.0 to 1.0 and returns `false` to interrupt, which gives `Ok(None)`.
    fn transcribe(
        &self,
        req: &TranscribeRequest,
        progress: &mut dyn FnMut(f32) -> bool,
    ) -> Result<Option<Transcript>, CmError>;
}

/// Caption lines and files (`cm_ai::caption`).
pub trait CaptionService: Send + Sync {
    fn available(&self) -> bool {
        true
    }
    /// Builds caption lines from a transcript (42 characters, 2 rows, 1 to 7 s each).
    fn lines(&self, transcript: &Transcript) -> Result<Vec<CaptionLine>, CmError>;
    /// The SRT or VTT text for these lines after the edit list is applied: cuts remove lines and
    /// shift later ones, speed scales times, and nothing references cut content (AI-1.AC4).
    fn render(
        &self,
        lines: &[CaptionLine],
        format: CaptionFormat,
        edits: &EditList,
    ) -> Result<String, CmError>;
}

/// Deterministic suggestions (`cm_ai::suggest`). Blocking.
pub trait SuggestService: Send + Sync {
    fn available(&self) -> bool {
        true
    }
    fn zooms(&self, bundle_dir: &Path) -> Result<Vec<ZoomSuggestion>, CmError>;
    /// Silence from the audio, filler words from the transcript when there is one.
    fn cuts(
        &self,
        bundle_dir: &Path,
        transcript: Option<&Transcript>,
    ) -> Result<Vec<CutSuggestion>, CmError>;
}

/// The hint every unavailable call carries.
pub const NOT_BUILT: &str =
    "local AI is not available in this build yet: the speech engine has not been plugged into the daemon";

fn not_built() -> CmError {
    CmError::not_implemented(NOT_BUILT)
}

/// The default of every service: not available, every call fails with `not_implemented`.
pub struct Unavailable;

impl ModelService for Unavailable {
    fn available(&self) -> bool {
        false
    }
    fn list(&self) -> Result<ModelList, CmError> {
        Err(not_built())
    }
    fn download(
        &self,
        _: &str,
        _: &mut dyn FnMut(DownloadProgress) -> bool,
    ) -> Result<bool, CmError> {
        Err(not_built())
    }
    fn import(&self, _: &str, _: &Path) -> Result<(), CmError> {
        Err(not_built())
    }
    fn select(&self, _: &str) -> Result<(), CmError> {
        Err(not_built())
    }
    fn remove(&self, _: &str) -> Result<(), CmError> {
        Err(not_built())
    }
}

impl TranscribeService for Unavailable {
    fn available(&self) -> bool {
        false
    }
    fn transcribe(
        &self,
        _: &TranscribeRequest,
        _: &mut dyn FnMut(f32) -> bool,
    ) -> Result<Option<Transcript>, CmError> {
        Err(not_built())
    }
}

impl CaptionService for Unavailable {
    fn available(&self) -> bool {
        false
    }
    fn lines(&self, _: &Transcript) -> Result<Vec<CaptionLine>, CmError> {
        Err(not_built())
    }
    fn render(&self, _: &[CaptionLine], _: CaptionFormat, _: &EditList) -> Result<String, CmError> {
        Err(not_built())
    }
}

impl SuggestService for Unavailable {
    fn available(&self) -> bool {
        false
    }
    fn zooms(&self, _: &Path) -> Result<Vec<ZoomSuggestion>, CmError> {
        Err(not_built())
    }
    fn cuts(&self, _: &Path, _: Option<&Transcript>) -> Result<Vec<CutSuggestion>, CmError> {
        Err(not_built())
    }
}

/// The engines behind the AI actions ([`crate::DaemonBuilder::ai`]).
#[derive(Clone)]
pub struct AiServices {
    pub models: Arc<dyn ModelService>,
    pub transcribe: Arc<dyn TranscribeService>,
    pub captions: Arc<dyn CaptionService>,
    pub suggest: Arc<dyn SuggestService>,
}

impl Default for AiServices {
    fn default() -> Self {
        AiServices {
            models: Arc::new(Unavailable),
            transcribe: Arc::new(Unavailable),
            captions: Arc::new(Unavailable),
            suggest: Arc::new(Unavailable),
        }
    }
}

impl AiServices {
    /// Whether the engine behind a Confirm-tier AI action exists (checked before any dialog).
    pub(crate) fn supports(&self, method: &str) -> bool {
        match method {
            "list_models" | "download_model" | "import_model" | "select_model" | "remove_model" => {
                self.models.available()
            }
            _ => true,
        }
    }
}

// ---- the actions ---------------------------------------------------------------------------

/// One validated AI action.
#[derive(Debug, Clone, PartialEq)]
pub enum AiOp {
    ListModels,
    DownloadModel {
        model: String,
    },
    ImportModel {
        model: String,
        path: String,
    },
    SelectModel {
        model: String,
    },
    RemoveModel {
        model: String,
    },
    Transcribe {
        capture_id: String,
        /// `None`: detect.
        language: Option<String>,
        /// `None`: the active model.
        model: Option<String>,
    },
    GetTranscript {
        capture_id: String,
    },
    GetCaptions {
        capture_id: String,
    },
    SetCaptions {
        capture_id: String,
        lines: Vec<CaptionLine>,
    },
    ReplaceInCaptions {
        capture_id: String,
        replacements: Vec<Replacement>,
    },
    ExportCaptions {
        capture_id: String,
        format: CaptionFormat,
    },
    SuggestZooms {
        capture_id: String,
    },
    SuggestCuts {
        capture_id: String,
    },
}

impl AiOp {
    pub fn method(&self) -> &'static str {
        match self {
            AiOp::ListModels => "list_models",
            AiOp::DownloadModel { .. } => "download_model",
            AiOp::ImportModel { .. } => "import_model",
            AiOp::SelectModel { .. } => "select_model",
            AiOp::RemoveModel { .. } => "remove_model",
            AiOp::Transcribe { .. } => "transcribe",
            AiOp::GetTranscript { .. } => "get_transcript",
            AiOp::GetCaptions { .. } => "get_captions",
            AiOp::SetCaptions { .. } => "set_captions",
            AiOp::ReplaceInCaptions { .. } => "replace_in_captions",
            AiOp::ExportCaptions { .. } => "export_captions",
            AiOp::SuggestZooms { .. } => "suggest_zooms",
            AiOp::SuggestCuts { .. } => "suggest_cuts",
        }
    }

    /// The capture the action works on.
    pub fn capture_id(&self) -> Option<&str> {
        match self {
            AiOp::Transcribe { capture_id, .. }
            | AiOp::GetTranscript { capture_id }
            | AiOp::GetCaptions { capture_id }
            | AiOp::SetCaptions { capture_id, .. }
            | AiOp::ReplaceInCaptions { capture_id, .. }
            | AiOp::ExportCaptions { capture_id, .. }
            | AiOp::SuggestZooms { capture_id }
            | AiOp::SuggestCuts { capture_id } => Some(capture_id),
            _ => None,
        }
    }
}

/// One pair of `replace_in_captions`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Replacement {
    pub find: String,
    pub replace: String,
    pub match_case: bool,
    pub whole_word: bool,
}

/// An AI action with its idempotency key.
#[derive(Debug, Clone, PartialEq)]
pub struct AiParams {
    pub op: AiOp,
    pub request_id: Option<String>,
}

impl AiParams {
    /// The input as JSON (what the confirmation dialog and the activity log show).
    pub fn to_json(&self) -> Value {
        let mut o = Map::new();
        match &self.op {
            AiOp::ListModels => {}
            AiOp::DownloadModel { model }
            | AiOp::SelectModel { model }
            | AiOp::RemoveModel { model } => {
                o.insert("model".into(), json!(model));
            }
            AiOp::ImportModel { model, path } => {
                o.insert("model".into(), json!(model));
                o.insert("path".into(), json!(path));
            }
            AiOp::Transcribe {
                capture_id,
                language,
                model,
            } => {
                o.insert("capture_id".into(), json!(capture_id));
                if let Some(l) = language {
                    o.insert("language".into(), json!(l));
                }
                if let Some(m) = model {
                    o.insert("model".into(), json!(m));
                }
            }
            AiOp::GetTranscript { capture_id }
            | AiOp::GetCaptions { capture_id }
            | AiOp::SuggestZooms { capture_id }
            | AiOp::SuggestCuts { capture_id } => {
                o.insert("capture_id".into(), json!(capture_id));
            }
            AiOp::SetCaptions { capture_id, lines } => {
                o.insert("capture_id".into(), json!(capture_id));
                o.insert("lines".into(), json!(lines));
            }
            AiOp::ReplaceInCaptions {
                capture_id,
                replacements,
            } => {
                o.insert("capture_id".into(), json!(capture_id));
                o.insert("replacements".into(), json!(replacements));
            }
            AiOp::ExportCaptions { capture_id, format } => {
                o.insert("capture_id".into(), json!(capture_id));
                o.insert("format".into(), json!(format.ext()));
            }
        }
        Value::Object(o)
    }
}

impl Serialize for AiParams {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut v = self.to_json();
        if let (Some(o), Some(r)) = (v.as_object_mut(), &self.request_id) {
            o.insert("request_id".into(), json!(r));
        }
        v.serialize(s)
    }
}

/// One sentence for the dialog and the activity log.
pub(crate) fn summary(p: &AiParams) -> String {
    match &p.op {
        AiOp::DownloadModel { model } => {
            format!("Download the speech model `{model}` from the internet")
        }
        AiOp::RemoveModel { model } => {
            format!("Delete the speech model `{model}` from this computer")
        }
        AiOp::ImportModel { model, .. } => {
            format!("Install the speech model `{model}` from a file")
        }
        AiOp::SelectModel { model } => format!("Use the speech model `{model}`"),
        AiOp::Transcribe { capture_id, .. } => format!("Transcribe {capture_id}"),
        AiOp::SetCaptions { capture_id, lines } => {
            format!("Set {} caption lines of {capture_id}", lines.len())
        }
        AiOp::ReplaceInCaptions {
            capture_id,
            replacements,
        } => format!(
            "Replace text in the captions of {capture_id} ({} pairs)",
            replacements.len()
        ),
        AiOp::ExportCaptions { capture_id, format } => {
            format!(
                "Export {capture_id} captions as {}",
                format.ext().to_uppercase()
            )
        }
        other => format!("Run {}", other.method()),
    }
}

/// The typed parameters of an AI action's confirmation (P3): the model, capture and format the
/// English `summary` names.
pub(crate) fn confirm_params(p: &AiParams) -> Value {
    match &p.op {
        AiOp::DownloadModel { model }
        | AiOp::RemoveModel { model }
        | AiOp::ImportModel { model, .. }
        | AiOp::SelectModel { model } => json!({ "model": model }),
        AiOp::Transcribe { capture_id, .. } => json!({ "capture_id": capture_id }),
        AiOp::SetCaptions { capture_id, lines } => {
            json!({ "capture_id": capture_id, "lines": lines.len() })
        }
        AiOp::ExportCaptions { capture_id, format } => {
            json!({ "capture_id": capture_id, "format": format.ext().to_uppercase() })
        }
        _ => json!({}),
    }
}

fn model_id(f: &Fields<'_>, key: &str) -> Result<String, CmError> {
    let m = f.str_req(key)?;
    let ok = m.len() <= 40
        && m.chars().next().is_some_and(|c| c.is_ascii_alphanumeric())
        && m.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'));
    if ok {
        Ok(m)
    } else {
        Err(CmError::invalid_field(
            key,
            "must be a model id such as `base`, `small` or `medium`",
        ))
    }
}

fn language(f: &Fields<'_>) -> Result<Option<String>, CmError> {
    match f.str_opt("language")? {
        None => Ok(None),
        Some(l) if l.eq_ignore_ascii_case("auto") => Ok(None),
        Some(l) => {
            let l = l.to_ascii_lowercase();
            let mut parts = l.split('-');
            let head_ok = parts.next().is_some_and(|h| {
                (2..=3).contains(&h.len()) && h.chars().all(|c| c.is_ascii_lowercase())
            });
            let tail_ok = parts.all(|t| {
                (2..=4).contains(&t.len()) && t.chars().all(|c| c.is_ascii_alphanumeric())
            });
            if head_ok && tail_ok && l.len() <= 12 {
                Ok(Some(l))
            } else {
                Err(CmError::invalid_field(
                    "language",
                    "must be `auto` or a language code such as `en`, `vi` or `es`",
                ))
            }
        }
    }
}

/// `lines` of `set_captions`: times as seconds or `"t:<ticks>"`, in order, not overlapping, at
/// most 2 rows of 42 characters and 7 s each (AI-1).
fn caption_lines(f: &Fields<'_>) -> Result<Vec<CaptionLine>, CmError> {
    let arr = f
        .obj
        .get("lines")
        .ok_or_else(|| CmError::invalid_field("lines", "is required"))?
        .as_array()
        .ok_or_else(|| CmError::invalid_field("lines", "must be an array"))?;
    if arr.len() > MAX_CAPTION_LINES {
        return Err(CmError::invalid_field(
            "lines",
            format!("at most {MAX_CAPTION_LINES} lines"),
        ));
    }
    let mut out: Vec<CaptionLine> = Vec::with_capacity(arr.len());
    for (i, item) in arr.iter().enumerate() {
        let at = |k: &str| format!("lines[{i}].{k}");
        let o = item
            .as_object()
            .ok_or_else(|| CmError::invalid_field(&format!("lines[{i}]"), "must be an object"))?;
        for k in o.keys() {
            if !matches!(k.as_str(), "start" | "end" | "text") {
                return Err(CmError::invalid_field(&at(k), "unknown parameter"));
            }
        }
        let time = |k: &str| match o.get(k) {
            Some(v) => parse_time(&at(k), v),
            None => Err(CmError::invalid_field(&at(k), "is required")),
        };
        let (start, end) = (time("start")?, time("end")?);
        if end <= start {
            return Err(CmError::invalid_field(&at("end"), "must be after start"));
        }
        if secs(end) - secs(start) > MAX_CAPTION_SECONDS {
            return Err(CmError::invalid_field(
                &at("end"),
                format!("a line lasts at most {MAX_CAPTION_SECONDS} s"),
            ));
        }
        let text = match o.get("text") {
            Some(Value::String(t)) => t.trim().to_owned(),
            Some(_) => return Err(CmError::invalid_field(&at("text"), "must be a string")),
            None => return Err(CmError::invalid_field(&at("text"), "is required")),
        };
        let rows: Vec<&str> = text.split('\n').collect();
        if text.is_empty() {
            return Err(CmError::invalid_field(&at("text"), "must not be empty"));
        }
        if rows.len() > MAX_CAPTION_ROWS
            || rows
                .iter()
                .any(|r| r.chars().count() > MAX_CAPTION_ROW_CHARS)
        {
            return Err(CmError::invalid_field(
                &at("text"),
                format!("at most {MAX_CAPTION_ROWS} rows of {MAX_CAPTION_ROW_CHARS} characters"),
            ));
        }
        if let Some(prev) = out.last() {
            if start < prev.end {
                return Err(CmError::invalid_field(
                    &at("start"),
                    "lines must be in order and must not overlap",
                ));
            }
        }
        out.push(CaptionLine { start, end, text });
    }
    Ok(out)
}

/// `replacements` of `replace_in_captions`.
fn replacements(f: &Fields<'_>) -> Result<Vec<Replacement>, CmError> {
    let arr = f
        .obj
        .get("replacements")
        .ok_or_else(|| CmError::invalid_field("replacements", "is required"))?
        .as_array()
        .ok_or_else(|| CmError::invalid_field("replacements", "must be an array"))?;
    if arr.is_empty() || arr.len() > MAX_REPLACEMENTS {
        return Err(CmError::invalid_field(
            "replacements",
            format!("give 1 to {MAX_REPLACEMENTS} pairs"),
        ));
    }
    let mut out = Vec::with_capacity(arr.len());
    for (i, item) in arr.iter().enumerate() {
        let at = |k: &str| format!("replacements[{i}].{k}");
        let o = item.as_object().ok_or_else(|| {
            CmError::invalid_field(&format!("replacements[{i}]"), "must be an object")
        })?;
        for k in o.keys() {
            if !matches!(k.as_str(), "find" | "replace" | "match_case" | "whole_word") {
                return Err(CmError::invalid_field(&at(k), "unknown parameter"));
            }
        }
        let text = |k: &str, required_non_empty: bool| match o.get(k) {
            Some(Value::String(s)) => {
                if s.chars().count() > MAX_REPLACE_CHARS {
                    Err(CmError::invalid_field(
                        &at(k),
                        format!("at most {MAX_REPLACE_CHARS} characters"),
                    ))
                } else if required_non_empty && s.trim().is_empty() {
                    Err(CmError::invalid_field(&at(k), "must not be empty"))
                } else {
                    Ok(s.clone())
                }
            }
            Some(_) => Err(CmError::invalid_field(&at(k), "must be a string")),
            None => Err(CmError::invalid_field(&at(k), "is required")),
        };
        let flag = |k: &str| match o.get(k) {
            None => Ok(false),
            Some(Value::Bool(b)) => Ok(*b),
            Some(_) => Err(CmError::invalid_field(&at(k), "must be true or false")),
        };
        out.push(Replacement {
            find: text("find", true)?,
            replace: text("replace", false)?,
            match_case: flag("match_case")?,
            whole_word: flag("whole_word")?,
        });
    }
    Ok(out)
}

/// Validates the input of one AI method.
pub(crate) fn parse(method: &str, f: &Fields<'_>) -> Result<Action, CmError> {
    let op = match method {
        "list_models" => {
            f.only(&[])?;
            AiOp::ListModels
        }
        "download_model" | "select_model" | "remove_model" => {
            f.only(&["model", "request_id"])?;
            let model = model_id(f, "model")?;
            match method {
                "download_model" => AiOp::DownloadModel { model },
                "select_model" => AiOp::SelectModel { model },
                _ => AiOp::RemoveModel { model },
            }
        }
        "import_model" => {
            f.only(&["model", "path", "request_id"])?;
            AiOp::ImportModel {
                model: model_id(f, "model")?,
                path: f.str_req("path")?,
            }
        }
        "transcribe" => {
            f.only(&["capture_id", "language", "model", "request_id"])?;
            let model = match f.str_opt("model")? {
                None => None,
                Some(_) => Some(model_id(f, "model")?),
            };
            AiOp::Transcribe {
                capture_id: f.str_req("capture_id")?,
                language: language(f)?,
                model,
            }
        }
        "get_transcript" | "get_captions" | "suggest_zooms" | "suggest_cuts" => {
            f.only(&["capture_id"])?;
            let capture_id = f.str_req("capture_id")?;
            match method {
                "get_transcript" => AiOp::GetTranscript { capture_id },
                "get_captions" => AiOp::GetCaptions { capture_id },
                "suggest_zooms" => AiOp::SuggestZooms { capture_id },
                _ => AiOp::SuggestCuts { capture_id },
            }
        }
        "set_captions" => {
            f.only(&["capture_id", "lines", "request_id"])?;
            AiOp::SetCaptions {
                capture_id: f.str_req("capture_id")?,
                lines: caption_lines(f)?,
            }
        }
        "replace_in_captions" => {
            f.only(&["capture_id", "replacements", "request_id"])?;
            AiOp::ReplaceInCaptions {
                capture_id: f.str_req("capture_id")?,
                replacements: replacements(f)?,
            }
        }
        "export_captions" => {
            f.only(&["capture_id", "format", "timestamps", "request_id"])?;
            let timestamps = f.bool_or("timestamps", false)?;
            let format = match f.str_req("format")?.as_str() {
                "srt" => CaptionFormat::Srt,
                "vtt" => CaptionFormat::Vtt,
                "txt" if timestamps => CaptionFormat::TxtTimed,
                "txt" => CaptionFormat::Txt,
                _ => {
                    return Err(CmError::invalid_field(
                        "format",
                        "must be `srt`, `vtt` or `txt`",
                    ))
                }
            };
            if timestamps && !format.is_text() {
                return Err(CmError::invalid_field(
                    "timestamps",
                    "only applies to the `txt` format (SRT and VTT always have times)",
                ));
            }
            AiOp::ExportCaptions {
                capture_id: f.str_req("capture_id")?,
                format,
            }
        }
        other => {
            return Err(CmError::invalid_input(format!(
                "unknown AI action `{other}`"
            )))
        }
    };
    Ok(Action::Ai(AiParams {
        op,
        request_id: f.request_id()?,
    }))
}

// ---- settings ------------------------------------------------------------------------------

/// Installed RAM in GB, when this OS can tell without extra dependencies.
pub fn system_ram_gb() -> Option<u64> {
    #[cfg(target_os = "macos")]
    {
        let out = std::process::Command::new("sysctl")
            .args(["-n", "hw.memsize"])
            .output()
            .ok()?;
        let bytes: u64 = String::from_utf8_lossy(&out.stdout).trim().parse().ok()?;
        Some(bytes / 1_000_000_000)
    }
    #[cfg(target_os = "linux")]
    {
        let text = std::fs::read_to_string("/proc/meminfo").ok()?;
        let kb: u64 = text
            .lines()
            .find_map(|l| l.strip_prefix("MemTotal:"))?
            .split_whitespace()
            .next()?
            .parse()
            .ok()?;
        Some(kb / 1_000_000)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        None
    }
}

/// Whether the settings ask for an automatic transcription: the user's choice if there is one,
/// otherwise on only when the machine has [`AUTO_MIN_RAM_GB`] of RAM or more (AI-1).
pub fn auto_transcribe_enabled(settings: &Map<String, Value>, ram_gb: Option<u64>) -> bool {
    match settings
        .get(SETTING_AUTO_TRANSCRIBE)
        .and_then(Value::as_bool)
    {
        Some(choice) => choice,
        None => ram_gb.is_some_and(|g| g >= AUTO_MIN_RAM_GB),
    }
}

// ---- jobs ----------------------------------------------------------------------------------

fn no_model_error() -> DaemonError {
    DaemonError::WithData {
        error: CmError::invalid_input(
            "No speech model is installed, so nothing can be transcribed. Open the AI page in \
             CaptureMann, or run `capturemann ai models`, to download or import one. Nothing is \
             downloaded automatically.",
        ),
        data: json!({
            "reason": "no_model",
            "hint": "capturemann ai models download small",
        }),
    }
}

fn model_not_installed(model: &str, installed: &[String]) -> DaemonError {
    DaemonError::WithData {
        error: CmError::invalid_input(format!(
            "The speech model `{model}` is not installed. Installed: {}. Download it from the AI \
             page or with `capturemann ai models download {model}`.",
            if installed.is_empty() {
                "none".to_owned()
            } else {
                installed.join(", ")
            }
        )),
        data: json!({ "reason": "model_not_installed", "model": model }),
    }
}

/// Writes `bytes` to `dir/name` through a temporary file, so a reader never sees half of it.
fn write_atomic(dir: &Path, name: &str, bytes: &[u8]) -> Result<PathBuf, CmError> {
    let target = dir.join(name);
    let tmp = dir.join(format!(".{name}.tmp"));
    let written = std::fs::write(&tmp, bytes).and_then(|()| std::fs::rename(&tmp, &target));
    if let Err(e) = written {
        // Never leave a half-written temp file behind (the target is untouched).
        let _ = std::fs::remove_file(&tmp);
        return Err(CmError::internal(format!(
            "cannot write `{}`: {e}",
            target.display()
        )));
    }
    Ok(target)
}

/// Sets a flag when the job is interrupted (cancel, hold or shutdown), for blocking code.
fn interrupt_flag(ctx: &JobContext) -> (Arc<AtomicBool>, tokio::task::JoinHandle<()>) {
    let flag = Arc::new(AtomicBool::new(false));
    let watcher = {
        let mut token = ctx.cancel.clone();
        let flag = flag.clone();
        tokio::spawn(async move {
            token.interrupted().await;
            flag.store(true, Ordering::SeqCst);
        })
    };
    (flag, watcher)
}

/// Runs `model_download` jobs.
struct ModelDownloadHandler {
    events: EventBus,
    service: Arc<dyn ModelService>,
}

#[async_trait]
impl JobHandler for ModelDownloadHandler {
    async fn run(&self, ctx: JobContext) -> Result<Value, CmError> {
        let model = ctx
            .job
            .inputs
            .get("model")
            .and_then(Value::as_str)
            .ok_or_else(|| CmError::invalid_field("model", "is required"))?
            .to_owned();
        let (flag, watcher) = interrupt_flag(&ctx);
        let (service, events, reporter, run_flag, id) = (
            self.service.clone(),
            self.events.clone(),
            ctx.reporter.clone(),
            flag.clone(),
            model.clone(),
        );
        let joined = tokio::task::spawn_blocking(move || {
            let mut last = -1.0_f64;
            service.download(&id, &mut |p: DownloadProgress| {
                let fraction = if p.bytes_total == 0 {
                    0.0
                } else {
                    (p.bytes_done as f64 / p.bytes_total as f64).clamp(0.0, 1.0)
                };
                if fraction >= 1.0 || fraction - last >= 0.01 {
                    last = fraction;
                    reporter.report(
                        fraction,
                        Some(format!(
                            "Downloading {} {}%",
                            p.model,
                            (fraction * 100.0) as u32
                        )),
                    );
                    events.publish(
                        events::MODEL_DOWNLOAD_PROGRESS,
                        json!({ "model": p.model, "bytes_done": p.bytes_done,
                                "bytes_total": p.bytes_total, "fraction": fraction }),
                    );
                }
                !run_flag.load(Ordering::SeqCst)
            })
        })
        .await;
        watcher.abort();
        match joined {
            Ok(Ok(true)) => {
                self.events.publish(
                    events::MODEL_CHANGED,
                    json!({ "model": model, "change": "downloaded" }),
                );
                Ok(json!({ "model": model }))
            }
            Ok(Ok(false)) => Err(CmError::internal("The download was interrupted")),
            Ok(Err(e)) => Err(e),
            Err(e) => Err(CmError::internal(format!("download worker failed: {e}"))),
        }
    }
}

/// Runs `transcribe` jobs.
struct TranscribeHandler {
    library: Arc<std::sync::Mutex<cm_library::Library>>,
    events: EventBus,
    service: Arc<dyn TranscribeService>,
    captions: Arc<dyn CaptionService>,
    settings: Arc<dyn crate::engines::SettingsStore>,
}

#[async_trait]
impl JobHandler for TranscribeHandler {
    async fn run(&self, ctx: JobContext) -> Result<Value, CmError> {
        let inputs = &ctx.job.inputs;
        let capture_id = inputs
            .get("capture_id")
            .and_then(Value::as_str)
            .ok_or_else(|| CmError::invalid_field("capture_id", "is required"))?
            .to_owned();
        let model = inputs
            .get("model")
            .and_then(Value::as_str)
            .ok_or_else(|| CmError::invalid_field("model", "is required"))?
            .to_owned();
        let language = inputs
            .get("language")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let dir = {
            let (library, id) = (self.library.clone(), capture_id.clone());
            blocking(move || {
                let lib = lock_library(&library);
                let r = lib.get(&id)?;
                let dir = lib.bundle_path(&r);
                if !dir.is_dir() {
                    return Err(CmError::not_found(format!(
                        "The recording folder `{}` was moved or deleted",
                        dir.display()
                    )));
                }
                Ok(dir)
            })
            .await
            .map_err(|e| match e {
                DaemonError::Cm(c) => c,
                other => CmError::internal(other.to_string()),
            })?
        };
        let (flag, watcher) = interrupt_flag(&ctx);
        // The glossary as it is when the run starts (AI-6a); a damaged setting is ignored.
        let glossary = cm_ai::glossary::from_value(
            self.settings
                .all()
                .ok()
                .as_ref()
                .and_then(|m| m.get(cm_ai::glossary::SETTING_GLOSSARY)),
        );
        let req = TranscribeRequest {
            capture_id: capture_id.clone(),
            bundle_dir: dir.clone(),
            language,
            model: model.clone(),
            glossary: glossary.clone(),
        };
        let (service, reporter, run_flag) = (self.service.clone(), ctx.reporter.clone(), flag);
        let joined = tokio::task::spawn_blocking(move || {
            let mut last = -1.0_f32;
            service.transcribe(&req, &mut |p| {
                if p >= 1.0 || p - last >= 0.01 {
                    last = p;
                    reporter.report(
                        f64::from(p),
                        Some(format!("Transcribing {}%", (p * 100.0) as u32)),
                    );
                }
                !run_flag.load(Ordering::SeqCst)
            })
        })
        .await;
        watcher.abort();
        let mut transcript = match joined {
            Ok(Ok(Some(t))) => t,
            Ok(Ok(None)) => return Err(CmError::internal("The transcription was interrupted")),
            Ok(Err(e)) => return Err(e),
            Err(e) => {
                return Err(CmError::internal(format!(
                    "transcription worker failed: {e}"
                )))
            }
        };
        // What Whisper tends to get wrong for this user, spelled their way (AI-6a). The words are
        // replaced before anything is stored, so captions, TXT and the suggestions all see them.
        let (words, glossary_replaced) =
            cm_ai::glossary::apply(std::mem::take(&mut transcript.words), &glossary);
        transcript.words = words;
        let bytes = serde_json::to_vec_pretty(&transcript)
            .map_err(|e| CmError::internal(format!("transcript: {e}")))?;
        let by = inputs
            .get("by")
            .and_then(Value::as_str)
            .and_then(|b| b.parse::<Actor>().ok())
            .unwrap_or(Actor::User);
        let (library, write_dir) = (self.library.clone(), dir);
        let (captions, new_transcript) = (self.captions.clone(), transcript.clone());
        // The new transcript replaces the old one in one rename, only now that the run finished:
        // a failed or cancelled run never reached this point, so the previous transcript is intact.
        // Caption lines the user edited belong to the old transcript. They are kept where the same
        // words were heard again (`cm_ai::reapply`, AI-6a) and dropped otherwise; either way the
        // caption change is ONE undoable revision (`undo` brings the old lines back).
        let (path, replaced, revision, shown, reapplied, dropped) = blocking(move || {
            let old_transcript: Option<Transcript> =
                std::fs::read_to_string(write_dir.join(TRANSCRIPT_FILE))
                    .ok()
                    .and_then(|t| serde_json::from_str(&t).ok());
            let replaced = write_dir.join(TRANSCRIPT_FILE).is_file();
            let manifest = cm_library::read_manifest(&write_dir)?;
            let stored = cm_library::rec_edit::captions_of(&manifest).cloned();
            // Work out what survives before the old transcript is overwritten.
            let kept = stored.as_ref().map(|v| {
                let stored_lines: Vec<CaptionLine> =
                    serde_json::from_value(v.clone()).unwrap_or_default();
                let old_auto = old_transcript.as_ref().and_then(|t| captions.lines(t).ok());
                let new_auto = captions.lines(&new_transcript).ok();
                match (old_auto, new_auto) {
                    (Some(old), Some(new)) => {
                        let r = cm_ai::reapply::reapply_edits(&old, &stored_lines, &new);
                        if r.reapplied > 0 {
                            (json!(r.lines), r.reapplied, r.dropped)
                        } else {
                            (json!([]), 0, r.dropped)
                        }
                    }
                    // No way to compare: nothing can be kept.
                    _ => (json!([]), 0, stored_lines.len()),
                }
            });
            let p = write_atomic(&write_dir, TRANSCRIPT_FILE, &bytes)?;
            let mut lib = lock_library(&library);
            let (revision, reapplied, dropped) = match kept {
                Some((lines, reapplied, dropped)) => {
                    let out = cm_library::rec_edit::apply_captions(
                        &write_dir,
                        lines,
                        by,
                        time::OffsetDateTime::now_utc(),
                    )?;
                    (Some(out.revision), reapplied, dropped)
                }
                None => (None, 0, 0),
            };
            // The recording's FIRST transcript switches the preview captions on (export stays off),
            // unless the user already decided: see `default_captions_on_first_transcript`. Not a
            // revision, so there is no extra undo step.
            let shown = if replaced {
                false
            } else {
                cm_library::rec_edit::default_captions_on_first_transcript(&write_dir)?
            };
            // The bundle changed: refresh the index row.
            lib.index_dir(&write_dir)?;
            Ok((p, replaced, revision, shown, reapplied, dropped))
        })
        .await
        .map_err(|e| match e {
            DaemonError::Cm(c) => c,
            other => CmError::internal(other.to_string()),
        })?;
        if let Some(rev) = &revision {
            self.events.publish(
                events::CAPTURE_REVISED,
                json!({ "capture_id": capture_id, "revision": rev.rev,
                        "action": rev.action, "by": rev.by }),
            );
        }
        self.events.publish(
            events::TRANSCRIPT_READY,
            json!({ "capture_id": capture_id, "model": transcript.model,
                    "language": transcript.language, "words": transcript.words.len(),
                    "replaced": replaced, "captions_cleared": revision.is_some(),
                    "captions_shown": shown, "reapplied": reapplied, "dropped": dropped,
                    "glossary_replaced": glossary_replaced }),
        );
        Ok(json!({ "capture_id": capture_id, "model": transcript.model,
                   "language": transcript.language, "words": transcript.words.len(),
                   "path": path, "replaced": replaced,
                   "captions_cleared": revision.is_some(),
                   "captions_shown": shown, "reapplied": reapplied, "dropped": dropped,
                   "glossary_replaced": glossary_replaced }))
    }
}

/// The recording's mic or system track is what a transcript is made from.
fn has_speech_track(manifest: &cm_library::manifest::RecManifest) -> bool {
    ["mic", "system"].iter().any(|k| {
        manifest
            .tracks
            .get(*k)
            .is_some_and(|t| t.get("missing") != Some(&json!(true)))
    })
}

/// Enqueues a transcription unless an equal one is queued, running or paused (then that job is
/// returned, `true` = reused).
fn enqueue_transcribe(
    queue: &JobQueue,
    capture_id: &str,
    model: &str,
    language: Option<&str>,
    by: &str,
    auto: bool,
) -> Result<(Job, bool), CmError> {
    static ENQUEUE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _one_at_a_time = ENQUEUE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let active = queue.list(&JobFilter {
        job_type: Some(JOB_TRANSCRIBE.to_owned()),
        states: Some(vec![JobState::Queued, JobState::Running, JobState::Paused]),
        limit: Some(200),
    })?;
    if let Some(j) = active.into_iter().find(|j| {
        j.inputs.get("capture_id").and_then(Value::as_str) == Some(capture_id)
            && j.inputs.get("model").and_then(Value::as_str) == Some(model)
            && j.inputs.get("language").and_then(Value::as_str) == language
    }) {
        return Ok((j, true));
    }
    let job = queue.enqueue(NewJob::new(
        JOB_TRANSCRIBE,
        json!({ "capture_id": capture_id, "model": model, "language": language,
                "by": by, "auto": auto }),
    ))?;
    Ok((job, false))
}

// ---- the daemon side -----------------------------------------------------------------------

impl Daemon {
    /// Registers the executors of the `model_download` and `transcribe` job types with the
    /// engines of [`crate::DaemonBuilder::ai`]. Call it next to
    /// [`Daemon::register_export_handler`], before [`Daemon::start_jobs`], in every process that
    /// owns a daemon. Errors with `not_implemented` without a job queue.
    pub fn register_ai_handlers(&self) -> Result<(), DaemonError> {
        let inner = &self.inner;
        self.register_job_handler(
            JOB_MODEL_DOWNLOAD,
            Arc::new(ModelDownloadHandler {
                events: inner.events.clone(),
                service: inner.ai.models.clone(),
            }),
        )?;
        self.register_job_handler(
            JOB_TRANSCRIBE,
            Arc::new(TranscribeHandler {
                library: inner.library.clone(),
                events: inner.events.clone(),
                service: inner.ai.transcribe.clone(),
                captions: inner.ai.captions.clone(),
                settings: inner.settings.clone(),
            }),
        )
    }

    /// Starts the automatic transcription after a recording (setting `ai.auto_transcribe`).
    /// When a recording stops normally the daemon enqueues a `transcribe` job, but only if the
    /// setting is on (default: on with 16 GB of RAM or more), a speech model is installed, a
    /// transcriber is registered and the recording has a microphone or system-audio track.
    /// Without an installed model nothing happens, silently: the app shows its gentle notice
    /// instead, and nothing is downloaded. Needs a tokio runtime; the task ends with the daemon.
    pub fn start_ai_auto_transcribe(&self) -> tokio::task::JoinHandle<()> {
        let mut rx = self.subscribe();
        let weak = Arc::downgrade(&self.inner);
        tokio::spawn(async move {
            while let Ok(ev) = rx.recv().await {
                if ev.name != events::RECORDING_STOPPED {
                    continue;
                }
                let Some(capture_id) = ev.data.get("capture_id").and_then(Value::as_str) else {
                    continue;
                };
                let Some(inner) = weak.upgrade() else { break };
                let capture_id = capture_id.to_owned();
                let _ = inner
                    .auto_transcribe(&capture_id, ev.data.get("tracks"))
                    .await;
            }
        })
    }
}

impl Inner {
    /// The installed models, from the service, off the async threads.
    async fn model_list(&self) -> Result<ModelList, DaemonError> {
        let svc = self.ai.models.clone();
        blocking(move || svc.list()).await
    }

    /// The model a transcription uses: the one asked for, else the active one, else the only
    /// or first installed one. `no_model` when nothing is installed.
    fn pick_model(list: &ModelList, wanted: Option<&str>) -> Result<String, DaemonError> {
        let installed: Vec<&ModelInfo> = list.models.iter().filter(|m| m.installed).collect();
        let ids = || installed.iter().map(|m| m.id.clone()).collect::<Vec<_>>();
        if let Some(w) = wanted {
            return if installed.iter().any(|m| m.id == w) {
                Ok(w.to_owned())
            } else if installed.is_empty() {
                Err(no_model_error())
            } else {
                Err(model_not_installed(w, &ids()))
            };
        }
        installed
            .iter()
            .find(|m| m.active)
            .or_else(|| installed.first())
            .map(|m| m.id.clone())
            .ok_or_else(no_model_error)
    }

    /// The dialog text and details of a Confirm-tier AI action, from the live model list, so the
    /// dialog says exactly what will happen (AGT-2). Refuses a request that cannot run now.
    pub(crate) async fn plan_ai(
        &self,
        p: &AiParams,
    ) -> Result<(String, Value, Value), DaemonError> {
        let list = self.model_list().await?;
        let (model, download) = match &p.op {
            AiOp::DownloadModel { model } => (model, true),
            AiOp::RemoveModel { model } => (model, false),
            _ => return Ok((summary(p), p.to_json(), confirm_params(p))),
        };
        let info = list.models.iter().find(|m| &m.id == model).ok_or_else(|| {
            CmError::invalid_field(
                "model",
                format!(
                    "unknown model `{model}`; use one of: {}",
                    list.models
                        .iter()
                        .map(|m| m.id.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            )
        })?;
        if download && info.installed {
            return Err(
                CmError::invalid_field("model", format!("`{model}` is already installed")).into(),
            );
        }
        if !download && !info.installed {
            return Err(CmError::not_found(format!("`{model}` is not installed")).into());
        }
        let mut details = Map::new();
        details.insert("model".into(), json!(info.id));
        details.insert("name".into(), json!(info.label));
        details.insert("size_mb".into(), json!(info.size_mb));
        if download {
            details.insert("destination".into(), json!(list.dir));
            details.insert(
                "network".into(),
                json!("Downloads the model from the internet"),
            );
            let params = json!({ "model": info.id, "name": info.label, "size_mb": info.size_mb,
                                 "destination": list.dir });
            Ok((
                format!(
                    "Download the speech model \"{}\" (about {} MB) from the internet",
                    info.label, info.size_mb
                ),
                Value::Object(details),
                params,
            ))
        } else {
            if let Some(path) = &info.path {
                details.insert("file".into(), json!(path));
            }
            let params = json!({ "model": info.id, "name": info.label, "size_mb": info.size_mb,
                                 "file": info.path });
            Ok((
                format!(
                    "Delete the speech model \"{}\" ({} MB) from this computer",
                    info.label, info.size_mb
                ),
                Value::Object(details),
                params,
            ))
        }
    }

    /// Runs one AI action.
    pub(crate) async fn ai_op(&self, actor: &Actor, p: AiParams) -> Result<Value, DaemonError> {
        match p.op {
            AiOp::ListModels => Ok(serde_json::to_value(self.model_list().await?)
                .map_err(|e| DaemonError::Internal(e.to_string()))?),
            AiOp::DownloadModel { model } => self.download_model(actor, model).await,
            AiOp::ImportModel { model, path } => {
                let svc = self.ai.models.clone();
                let id = model.clone();
                let file = PathBuf::from(&path);
                blocking(move || {
                    if !file.is_file() {
                        return Err(CmError::invalid_field(
                            "path",
                            format!("`{}` is not a file", file.display()),
                        ));
                    }
                    svc.import(&id, &file)
                })
                .await?;
                self.events.publish(
                    events::MODEL_CHANGED,
                    json!({ "model": model, "change": "imported" }),
                );
                Ok(json!({ "model": model, "installed": true }))
            }
            AiOp::SelectModel { model } => {
                let svc = self.ai.models.clone();
                let id = model.clone();
                blocking(move || {
                    let installed = svc.list()?.models.iter().any(|m| m.id == id && m.installed);
                    if !installed {
                        return Err(CmError::invalid_field(
                            "model",
                            format!("`{id}` is not installed; download or import it first"),
                        ));
                    }
                    svc.select(&id)
                })
                .await?;
                self.events.publish(
                    events::MODEL_CHANGED,
                    json!({ "model": model, "change": "selected" }),
                );
                Ok(json!({ "active": model }))
            }
            AiOp::RemoveModel { model } => self.remove_model(model).await,
            AiOp::Transcribe {
                capture_id,
                language,
                model,
            } => self.transcribe(actor, capture_id, language, model).await,
            AiOp::GetTranscript { capture_id } => self.get_transcript(&capture_id).await,
            AiOp::GetCaptions { capture_id } => self.get_captions(&capture_id).await,
            AiOp::SetCaptions { capture_id, lines } => {
                self.set_captions(actor, capture_id, lines).await
            }
            AiOp::ReplaceInCaptions {
                capture_id,
                replacements,
            } => {
                self.replace_in_captions(actor, capture_id, replacements)
                    .await
            }
            AiOp::ExportCaptions { capture_id, format } => {
                self.export_captions(&capture_id, format).await
            }
            AiOp::SuggestZooms { capture_id } => self.suggest(&capture_id, true).await,
            AiOp::SuggestCuts { capture_id } => self.suggest(&capture_id, false).await,
        }
    }

    async fn download_model(&self, actor: &Actor, model: String) -> Result<Value, DaemonError> {
        let list = self.model_list().await?;
        match list.models.iter().find(|m| m.id == model) {
            None => {
                return Err(
                    CmError::invalid_field("model", format!("unknown model `{model}`")).into(),
                )
            }
            Some(m) if m.installed => {
                return Err(CmError::invalid_field(
                    "model",
                    format!("`{model}` is already installed"),
                )
                .into())
            }
            Some(_) => {}
        }
        let queue = self.job_queue()?.queue.clone();
        if !queue.has_handler(JOB_MODEL_DOWNLOAD) {
            return Err(DaemonError::NotImplemented(
                "model downloads are not available: this daemon has no model downloader registered"
                    .to_owned(),
            ));
        }
        let by = actor.to_string();
        let id = model.clone();
        let (job, reused) = blocking(move || {
            let active = queue.list(&JobFilter {
                job_type: Some(JOB_MODEL_DOWNLOAD.to_owned()),
                states: Some(vec![JobState::Queued, JobState::Running, JobState::Paused]),
                limit: Some(50),
            })?;
            if let Some(j) = active
                .into_iter()
                .find(|j| j.inputs.get("model").and_then(Value::as_str) == Some(id.as_str()))
            {
                return Ok((j, true));
            }
            let job = queue.enqueue(NewJob::new(
                JOB_MODEL_DOWNLOAD,
                json!({ "model": id, "by": by }),
            ))?;
            Ok((job, false))
        })
        .await?;
        Ok(
            json!({ "job_id": job.id, "model": model, "state": job.state.as_str(), "reused": reused }),
        )
    }

    async fn remove_model(&self, model: String) -> Result<Value, DaemonError> {
        // A model in use cannot be pulled away: a queued or running transcription with it, or
        // its own download, is `busy`.
        if let Some(rt) = &self.jobs {
            let q = rt.queue.clone();
            let id = model.clone();
            blocking(move || {
                for (ty, what) in [
                    (JOB_TRANSCRIBE, "is being used by a transcription"),
                    (JOB_MODEL_DOWNLOAD, "is being downloaded"),
                ] {
                    let active = q.list(&JobFilter {
                        job_type: Some(ty.to_owned()),
                        states: Some(vec![JobState::Queued, JobState::Running, JobState::Paused]),
                        limit: Some(200),
                    })?;
                    if active
                        .iter()
                        .any(|j| j.inputs.get("model").and_then(Value::as_str) == Some(id.as_str()))
                    {
                        return Err(CmError::busy(format!(
                            "The model `{id}` {what}; wait for it to finish or cancel the job first"
                        )));
                    }
                }
                Ok(())
            })
            .await?;
        }
        let svc = self.ai.models.clone();
        let id = model.clone();
        blocking(move || svc.remove(&id)).await?;
        self.events.publish(
            events::MODEL_CHANGED,
            json!({ "model": model, "change": "removed" }),
        );
        Ok(json!({ "model": model, "removed": true }))
    }

    async fn transcribe(
        &self,
        actor: &Actor,
        capture_id: String,
        language: Option<String>,
        model: Option<String>,
    ) -> Result<Value, DaemonError> {
        let _serial = self.capture_locks.lock(&capture_id).await;
        let list = self.model_list().await?;
        let model = Self::pick_model(&list, model.as_deref())?;
        let queue = self.job_queue()?.queue.clone();
        if !queue.has_handler(JOB_TRANSCRIBE) {
            return Err(DaemonError::NotImplemented(
                "transcription is not available: this daemon has no transcriber registered"
                    .to_owned(),
            ));
        }
        let (library, this, id) = (
            self.library.clone(),
            self.library_view(),
            capture_id.clone(),
        );
        blocking(move || {
            let lib = lock_library(&library);
            let rec = lib.get(&id)?;
            if rec.kind != CaptureKind::Recording {
                return Err(CmError::invalid_input(format!(
                    "`{id}` is a screenshot; only recordings have speech to transcribe"
                )));
            }
            let dir = lib.bundle_path(&rec);
            this.ensure_idle(&rec, &dir, "transcribe")?;
            if !has_speech_track(&cm_library::read_manifest(&dir)?) {
                return Err(CmError::invalid_input(format!(
                    "`{id}` has no microphone or system-audio track to transcribe"
                )));
            }
            Ok(())
        })
        .await?;
        let by = actor.to_string();
        let (id, m, l) = (capture_id.clone(), model.clone(), language.clone());
        let (job, reused) =
            blocking(move || enqueue_transcribe(&queue, &id, &m, l.as_deref(), &by, false)).await?;
        Ok(json!({
            "job_id": job.id,
            "capture_id": capture_id,
            "model": model,
            "language": language,
            "state": job.state.as_str(),
            "reused": reused,
        }))
    }

    /// The automatic transcription of a recording that just stopped (see
    /// [`Daemon::start_ai_auto_transcribe`]). Quietly does nothing unless everything is in place.
    async fn auto_transcribe(
        &self,
        capture_id: &str,
        tracks: Option<&Value>,
    ) -> Result<(), DaemonError> {
        let Some(rt) = &self.jobs else { return Ok(()) };
        let speech = tracks.and_then(Value::as_array).is_some_and(|t| {
            t.iter()
                .any(|k| matches!(k.as_str(), Some("mic" | "system")))
        });
        if !speech || !rt.queue.has_handler(JOB_TRANSCRIBE) {
            return Ok(());
        }
        let settings = self.settings.all().unwrap_or_default();
        if !auto_transcribe_enabled(&settings, system_ram_gb()) {
            return Ok(());
        }
        // No model, no job, no notice here: the AI page says so, and nothing is downloaded.
        let Ok(list) = self.model_list().await else {
            return Ok(());
        };
        let Ok(model) = Self::pick_model(&list, None) else {
            return Ok(());
        };
        let (queue, id) = (rt.queue.clone(), capture_id.to_owned());
        blocking(move || {
            enqueue_transcribe(&queue, &id, &model, None, "user", true)?;
            Ok(())
        })
        .await
    }

    async fn get_transcript(&self, capture_id: &str) -> Result<Value, DaemonError> {
        let (library, id, captions) = (
            self.library.clone(),
            capture_id.to_owned(),
            self.ai.captions.clone(),
        );
        blocking(move || {
            let (dir, manifest) = recording_bundle(&library, &id)?;
            let path = dir.join(TRANSCRIPT_FILE);
            let text = std::fs::read_to_string(&path).map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    CmError::not_found(format!(
                        "`{id}` has no transcript yet; run `transcribe` first"
                    ))
                } else {
                    CmError::internal(format!("cannot read `{}`: {e}", path.display()))
                }
            })?;
            let transcript: Transcript = serde_json::from_str(&text).map_err(|e| {
                CmError::internal(format!("`{}` is not a transcript: {e}", path.display()))
            })?;
            // When the transcript was written (Unix seconds), for the details page.
            let made_at = std::fs::metadata(&path)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs());
            let stored = cm_library::rec_edit::captions_of(&manifest).cloned();
            let (lines, source) = match stored {
                Some(l) => (l, Some("edited")),
                None => match captions.lines(&transcript) {
                    Ok(l) => (json!(l), Some("generated")),
                    Err(_) => (Value::Null, None),
                },
            };
            let options = cm_library::rec_edit::caption_options(&manifest);
            Ok(json!({
                "capture_id": id,
                "transcript": transcript,
                "made_at": made_at,
                "captions": lines,
                "captions_source": source,
                "caption_options": {"enabled": options.enabled, "burn_in": options.burn_in},
            }))
        })
        .await
    }

    /// The caption lines of a recording: the edited lines when `set_captions` stored some,
    /// otherwise lines built from the transcript. `not_found` when there is neither.
    async fn get_captions(&self, capture_id: &str) -> Result<Value, DaemonError> {
        let (library, id, captions) = (
            self.library.clone(),
            capture_id.to_owned(),
            self.ai.captions.clone(),
        );
        blocking(move || {
            let (dir, manifest) = recording_bundle(&library, &id)?;
            if let Some(stored) = cm_library::rec_edit::captions_of(&manifest).cloned() {
                return Ok(json!({"capture_id": id, "lines": stored, "source": "edited"}));
            }
            let path = dir.join(TRANSCRIPT_FILE);
            let text = std::fs::read_to_string(&path).map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    CmError::not_found(format!(
                        "`{id}` has no captions yet; run `transcribe` first"
                    ))
                } else {
                    CmError::internal(format!("cannot read `{}`: {e}", path.display()))
                }
            })?;
            let transcript: Transcript = serde_json::from_str(&text).map_err(|e| {
                CmError::internal(format!("`{}` is not a transcript: {e}", path.display()))
            })?;
            let lines = captions.lines(&transcript)?;
            Ok(json!({"capture_id": id, "lines": lines, "source": "generated"}))
        })
        .await
    }

    async fn set_captions(
        &self,
        actor: &Actor,
        capture_id: String,
        lines: Vec<CaptionLine>,
    ) -> Result<Value, DaemonError> {
        let _serial = self.capture_locks.lock(&capture_id).await;
        let (library, this, now, by) = (
            self.library.clone(),
            self.library_view(),
            self.clock.now(),
            actor.clone(),
        );
        let (id, count) = (capture_id.clone(), lines.len());
        let out = blocking(move || {
            let mut lib = lock_library(&library);
            let rec = lib.get(&id)?;
            if rec.kind != CaptureKind::Recording {
                return Err(CmError::invalid_input(format!(
                    "`{id}` is a screenshot; captions belong to recordings"
                )));
            }
            let dir = lib.bundle_path(&rec);
            this.ensure_idle(&rec, &dir, "caption")?;
            let json_lines = serde_json::to_value(&lines)
                .map_err(|e| CmError::internal(format!("captions: {e}")))?;
            let out = cm_library::rec_edit::apply_captions(&dir, json_lines, by, now)?;
            lib.index_dir(&dir)?;
            Ok(out)
        })
        .await?;
        self.events.publish(
            events::CAPTURE_REVISED,
            json!({
                "capture_id": capture_id,
                "revision": out.revision.rev,
                "action": out.revision.action,
                "by": out.revision.by,
            }),
        );
        Ok(json!({
            "capture_id": capture_id,
            "revision": out.revision.rev,
            "action": out.revision.action,
            "by": out.revision.by,
            "lines": count,
            "can_undo": out.can_undo,
            "can_redo": out.can_redo,
        }))
    }

    /// `replace_in_captions`: find and replace over every caption line (the edited lines, else
    /// the lines built from the transcript), matched like the editor's find (NFC, Vietnamese
    /// exact), the pairs applied in order. One revision; nothing is written when nothing matched.
    /// `transcript.json` is not touched: the lines are the corrected text layer.
    async fn replace_in_captions(
        &self,
        actor: &Actor,
        capture_id: String,
        pairs: Vec<Replacement>,
    ) -> Result<Value, DaemonError> {
        let _serial = self.capture_locks.lock(&capture_id).await;
        let (library, this, now, by, captions) = (
            self.library.clone(),
            self.library_view(),
            self.clock.now(),
            actor.clone(),
            self.ai.captions.clone(),
        );
        let id = capture_id.clone();
        let out = blocking(move || {
            let mut lib = lock_library(&library);
            let rec = lib.get(&id)?;
            if rec.kind != CaptureKind::Recording {
                return Err(CmError::invalid_input(format!(
                    "`{id}` is a screenshot; captions belong to recordings"
                )));
            }
            let dir = lib.bundle_path(&rec);
            this.ensure_idle(&rec, &dir, "caption")?;
            let manifest = cm_library::read_manifest(&dir)?;
            let mut lines: Vec<CaptionLine> = match cm_library::rec_edit::captions_of(&manifest) {
                Some(v) => serde_json::from_value(v.clone())
                    .map_err(|e| CmError::internal(format!("stored captions: {e}")))?,
                None => {
                    let text = std::fs::read_to_string(dir.join(TRANSCRIPT_FILE)).map_err(|_| {
                        CmError::not_found(format!(
                            "`{id}` has no captions yet; run `transcribe` first"
                        ))
                    })?;
                    let t: Transcript = serde_json::from_str(&text)
                        .map_err(|e| CmError::internal(format!("transcript: {e}")))?;
                    captions.lines(&t)?
                }
            };
            let (mut total, mut touched) = (0usize, vec![false; lines.len()]);
            for p in &pairs {
                for (i, l) in lines.iter_mut().enumerate() {
                    if let Some((text, n)) = cm_ai::textmatch::replace_in_line(
                        &l.text,
                        &p.find,
                        &p.replace,
                        p.match_case,
                        p.whole_word,
                    ) {
                        l.text = text;
                        total += n;
                        touched[i] = true;
                    }
                }
            }
            let lines_touched = touched.iter().filter(|t| **t).count();
            if total == 0 {
                return Ok(json!({
                    "capture_id": id, "replacements": 0, "lines_touched": 0,
                    "revision": null, "changed": false,
                }));
            }
            // The same limits as `set_captions`: a replacement may not make a line too long.
            if let Some((i, _)) = lines
                .iter()
                .enumerate()
                .find(|(i, l)| touched[*i] && !cm_ai::textmatch::rows_ok(&l.text))
            {
                return Err(CmError::invalid_input(format!(
                    "the replacement would make caption line {} longer than {MAX_CAPTION_ROWS} rows of {MAX_CAPTION_ROW_CHARS} characters; nothing was changed",
                    i + 1
                )));
            }
            let json_lines = serde_json::to_value(&lines)
                .map_err(|e| CmError::internal(format!("captions: {e}")))?;
            let out = cm_library::rec_edit::apply_captions(&dir, json_lines, by, now)?;
            lib.index_dir(&dir)?;
            Ok(json!({
                "capture_id": id, "replacements": total, "lines_touched": lines_touched,
                "revision": out.revision.rev, "action": out.revision.action,
                "by": out.revision.by, "changed": true,
                "can_undo": out.can_undo, "can_redo": out.can_redo,
            }))
        })
        .await?;
        if out["changed"] == json!(true) {
            self.events.publish(
                events::CAPTURE_REVISED,
                json!({ "capture_id": capture_id, "revision": out["revision"],
                        "action": out["action"], "by": out["by"] }),
            );
        }
        Ok(out)
    }

    pub(crate) async fn export_captions(
        &self,
        capture_id: &str,
        format: CaptionFormat,
    ) -> Result<Value, DaemonError> {
        let _serial = self.capture_locks.lock(capture_id).await;
        let (library, id, captions) = (
            self.library.clone(),
            capture_id.to_owned(),
            self.ai.captions.clone(),
        );
        blocking(move || {
            let (dir, manifest) = recording_bundle(&library, &id)?;
            let title = lock_library(&library).get(&id)?.title;
            let lines: Vec<CaptionLine> = match cm_library::rec_edit::captions_of(&manifest) {
                Some(l) => serde_json::from_value(l.clone())
                    .map_err(|e| CmError::internal(format!("stored captions: {e}")))?,
                None => {
                    let text =
                        std::fs::read_to_string(dir.join(TRANSCRIPT_FILE)).map_err(|_| {
                            CmError::not_found(format!(
                            "`{id}` has no captions yet; run `transcribe` or `set_captions` first"
                        ))
                        })?;
                    let t: Transcript = serde_json::from_str(&text)
                        .map_err(|e| CmError::internal(format!("transcript: {e}")))?;
                    captions.lines(&t)?
                }
            };
            let edits = cm_library::rec_edit::load_edits(&manifest);
            let text = captions.render(&lines, format, &edits)?;
            let out_dir = dir.join("exports");
            std::fs::create_dir_all(&out_dir).map_err(|e| {
                CmError::internal(format!("cannot create `{}`: {e}", out_dir.display()))
            })?;
            let safe_title = title.replace(['/', '\\', ':'], "-");
            // The plain-text transcript is a document in its own right: `<title>.txt`; the
            // subtitle files say what they are.
            let name = if format.is_text() {
                format!("{safe_title}.txt")
            } else {
                format!("{safe_title} - captions.{}", format.ext())
            };
            let path = write_atomic(&out_dir, &name, text.as_bytes())?;
            lock_library(&library).index_dir(&dir)?;
            Ok(json!({
                "capture_id": id,
                "format": format.ext(),
                "path": path,
                "lines": lines.len(),
            }))
        })
        .await
    }

    async fn suggest(&self, capture_id: &str, zooms: bool) -> Result<Value, DaemonError> {
        let (library, id, svc) = (
            self.library.clone(),
            capture_id.to_owned(),
            self.ai.suggest.clone(),
        );
        blocking(move || {
            let (dir, _) = recording_bundle(&library, &id)?;
            let to_s = |t: cm_core::MediaTime| secs(t);
            let suggestions: Vec<Value> = if zooms {
                svc.zooms(&dir)?
                    .iter()
                    .map(|z| {
                        json!({
                            "start": z.start, "end": z.end,
                            "start_s": to_s(z.start), "end_s": to_s(z.end),
                            "focus": [z.focus.0, z.focus.1], "scale": z.scale,
                            "reason": z.reason,
                        })
                    })
                    .collect()
            } else {
                let transcript = std::fs::read_to_string(dir.join(TRANSCRIPT_FILE))
                    .ok()
                    .and_then(|t| serde_json::from_str::<Transcript>(&t).ok());
                svc.cuts(&dir, transcript.as_ref())?
                    .iter()
                    .map(|c| {
                        json!({
                            "start": c.start, "end": c.end,
                            "start_s": to_s(c.start), "end_s": to_s(c.end),
                            "kind": c.kind, "detail": c.detail,
                        })
                    })
                    .collect()
            };
            Ok(json!({
                "capture_id": id,
                "count": suggestions.len(),
                "suggestions": suggestions,
                "apply_with": if zooms { "set_zooms" } else { "cut_segments" },
            }))
        })
        .await
    }
}

/// The recording's bundle folder and manifest; `invalid_input` for a screenshot.
pub(crate) fn recording_bundle(
    library: &Arc<std::sync::Mutex<cm_library::Library>>,
    id: &str,
) -> Result<(PathBuf, cm_library::manifest::RecManifest), CmError> {
    let (rec, dir) = {
        let lib = lock_library(library);
        let r = lib.get(id)?;
        let d = lib.bundle_path(&r);
        (r, d)
    };
    if rec.kind != CaptureKind::Recording {
        return Err(CmError::invalid_input(format!(
            "`{id}` is a screenshot; this works on recordings"
        )));
    }
    if !dir.is_dir() {
        return Err(CmError::not_found(format!(
            "The recording folder `{}` was moved or deleted",
            dir.display()
        )));
    }
    let manifest = cm_library::read_manifest(&dir)?;
    Ok((dir, manifest))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn parse_ok(method: &str, params: Value) -> AiParams {
        match Action::parse(method, &params).unwrap() {
            Action::Ai(p) => p,
            other => panic!("not an AI action: {other:?}"),
        }
    }

    fn parse_err(method: &str, params: Value) -> String {
        Action::parse(method, &params).unwrap_err().to_string()
    }

    /// Every method name parses to an AI action with the same method name.
    #[test]
    fn ai_methods_parse_to_themselves() {
        for (m, p) in [
            ("list_models", json!({})),
            (
                "download_model",
                json!({"model": "small", "request_id": "r"}),
            ),
            (
                "import_model",
                json!({"model": "small", "path": "/tmp/m.bin"}),
            ),
            ("select_model", json!({"model": "base"})),
            ("remove_model", json!({"model": "base"})),
            (
                "transcribe",
                json!({"capture_id": "rec_1", "language": "auto"}),
            ),
            ("get_transcript", json!({"capture_id": "rec_1"})),
            ("get_captions", json!({"capture_id": "rec_1"})),
            (
                "set_captions",
                json!({"capture_id": "rec_1", "lines": [{"start": 1, "end": 2.5, "text": "Hi"}]}),
            ),
            (
                "replace_in_captions",
                json!({"capture_id": "rec_1", "replacements": [{"find": "a", "replace": "b"}]}),
            ),
            (
                "export_captions",
                json!({"capture_id": "rec_1", "format": "srt"}),
            ),
            ("suggest_zooms", json!({"capture_id": "rec_1"})),
            ("suggest_cuts", json!({"capture_id": "rec_1"})),
        ] {
            assert_eq!(parse_ok(m, p).op.method(), m);
        }
        assert_eq!(AI_METHODS.len(), 13);
    }

    #[test]
    fn language_and_model_ids_are_checked() {
        let p = parse_ok("transcribe", json!({"capture_id": "r", "language": "auto"}));
        assert!(matches!(p.op, AiOp::Transcribe { language: None, .. }));
        let p = parse_ok("transcribe", json!({"capture_id": "r", "language": "VI"}));
        assert!(matches!(p.op, AiOp::Transcribe { language: Some(l), .. } if l == "vi"));
        assert!(parse_err(
            "transcribe",
            json!({"capture_id": "r", "language": "english!"})
        )
        .starts_with("language"));
        assert!(parse_err("download_model", json!({"model": "../x"})).starts_with("model"));
        assert!(parse_err("download_model", json!({})).starts_with("model"));
        assert!(parse_err("list_models", json!({"x": 1})).starts_with('x'));
    }

    /// AI-1: caption lines keep to 2 rows of 42 characters and 7 s, in order, without overlap.
    #[test]
    fn ai_1_caption_lines_are_validated() {
        let ok = parse_ok(
            "set_captions",
            json!({"capture_id": "r", "lines": [
                {"start": 1, "end": 3, "text": "First line"},
                {"start": "t:480000", "end": 5.5, "text": "Two\nrows"}]}),
        );
        let AiOp::SetCaptions { lines, .. } = &ok.op else {
            panic!()
        };
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[1].start.ticks(), 480_000);
        let long = "x".repeat(43);
        for (lines, field) in [
            (json!([{"start": 2, "end": 1, "text": "a"}]), "lines[0].end"),
            (json!([{"start": 0, "end": 9, "text": "a"}]), "lines[0].end"),
            (json!([{"start": 0, "end": 1, "text": ""}]), "lines[0].text"),
            (
                json!([{"start": 0, "end": 1, "text": long}]),
                "lines[0].text",
            ),
            (
                json!([{"start": 0, "end": 1, "text": "a\nb\nc"}]),
                "lines[0].text",
            ),
            (
                json!([{"start": 0, "end": 2, "text": "a"}, {"start": 1, "end": 3, "text": "b"}]),
                "lines[1].start",
            ),
            (
                json!([{"start": 0, "end": 1, "text": "a", "x": 1}]),
                "lines[0].x",
            ),
            (json!([{"end": 1, "text": "a"}]), "lines[0].start"),
        ] {
            let e = parse_err("set_captions", json!({"capture_id": "r", "lines": lines}));
            assert!(e.starts_with(field), "{field}: {e}");
        }
        // An empty list clears the captions.
        assert!(matches!(
            parse_ok("set_captions", json!({"capture_id": "r", "lines": []})).op,
            AiOp::SetCaptions { lines, .. } if lines.is_empty()
        ));
    }

    /// The automatic transcription defaults on only with enough RAM; the user's choice wins.
    #[test]
    fn ai_1_auto_transcribe_default_depends_on_ram() {
        let none = Map::new();
        assert!(auto_transcribe_enabled(&none, Some(16)));
        assert!(auto_transcribe_enabled(&none, Some(64)));
        assert!(!auto_transcribe_enabled(&none, Some(8)));
        assert!(!auto_transcribe_enabled(&none, None));
        let mut on = Map::new();
        on.insert(SETTING_AUTO_TRANSCRIBE.into(), json!(true));
        assert!(auto_transcribe_enabled(&on, Some(8)));
        let mut off = Map::new();
        off.insert(SETTING_AUTO_TRANSCRIBE.into(), json!(false));
        assert!(!auto_transcribe_enabled(&off, Some(64)));
    }

    fn list(models: &[(&str, bool, bool)]) -> ModelList {
        ModelList {
            models: models
                .iter()
                .map(|(id, installed, active)| ModelInfo {
                    id: (*id).to_owned(),
                    label: (*id).to_owned(),
                    size_mb: 100,
                    ram_mb: 0,
                    hint: String::new(),
                    installed: *installed,
                    active: *active,
                    path: None,
                })
                .collect(),
            active: None,
            dir: "/models".to_owned(),
        }
    }

    #[test]
    fn model_choice_prefers_the_active_one_and_says_no_model() {
        let l = list(&[
            ("base", true, false),
            ("small", true, true),
            ("medium", false, false),
        ]);
        assert_eq!(Inner::pick_model(&l, None).unwrap(), "small");
        assert_eq!(Inner::pick_model(&l, Some("base")).unwrap(), "base");
        let e = Inner::pick_model(&l, Some("medium")).unwrap_err();
        assert!(
            matches!(&e, DaemonError::WithData { data, .. } if data["reason"] == "model_not_installed")
        );
        let none = list(&[("base", false, false)]);
        let e = Inner::pick_model(&none, None).unwrap_err();
        assert!(matches!(&e, DaemonError::WithData { data, .. } if data["reason"] == "no_model"));
        let e = Inner::pick_model(&none, Some("base")).unwrap_err();
        assert!(matches!(&e, DaemonError::WithData { data, .. } if data["reason"] == "no_model"));
    }
}
