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
//! | `list_text_models`, `select_text_model { model }` | Free | the built-in text models (AI-7) and which one runs |
//! | `download_text_model { model }`, `remove_text_model { model }` | **Confirm** | the same as `download_model` / `remove_model`, for the GGUF files of the built-in text engine; job type `text_model_download` |
//! | `transcribe { capture_id, language?, model? }` | Free | a `transcribe` job; `transcript.json` in the bundle |
//! | `get_transcript { capture_id }` | Free | the transcript and the caption lines |
//! | `set_captions { capture_id, lines }` | Free | edited caption lines: one revision, one undo step |
//! | `replace_in_captions { capture_id, replacements }` | Free | find and replace over the caption lines (AI-6a): one revision, one undo step |
//! | `export_captions { capture_id, format }` | Free | SRT or VTT in `exports/`, mapped through the edit list |
//! | `translate_captions { capture_id, target_language, engine? }` | Free | a `translate` job; `translations/<lang>.json` (AI-5); `get_captions`, `set_captions`, `replace_in_captions` and `export_captions` take an optional `language` |
//! | `list_report_templates`, `generate_report { capture_id, template, language?, context?, report_id? }`, `get_report`, `export_report`, `delete_report`, `edit_report`, `check_report_item` | Free | AI-9 reports (a `report` job; local text model only) |
//! | `suggest_zooms`, `suggest_cuts` | Free | suggestions only; accepting uses `set_zooms` / `cut_segments` |
//!
//! # Nothing is downloaded silently
//!
//! A model is fetched only by `download_model` (Confirm for agents; the app's AI page asks the user
//! itself). `transcribe` without an installed model fails with `invalid_input` and
//! `data.reason = "no_model"`, whose message points to the AI page and `capturemann ai models`.
//! The automatic transcription after a recording ([`Daemon::start_ai_auto_transcribe`], setting
//! `ai.auto_transcribe`, default on only with 16 GB of RAM or more) never starts without an
//! installed model, and never touches the network. The automatic speaker step after a transcript
//! ([`Daemon::start_ai_auto_speakers`], setting `ai.auto_speakers`, default ON) queues the
//! `diarize` job with the bundled speaker models and never overwrites an existing result.
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
//! captions_shown, reapplied, dropped, glossary_replaced }`), `translation.ready` (`{ capture_id,
//! language, engine, model, lines, untranslated, path, kept_edits, dropped_edits }`); jobs also emit the usual
//! `job.progress`, `job.done` and `job.failed`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use cm_ai::caption::CaptionLine;
use cm_ai::model::{DownloadProgress, ModelInfo, ModelList};
use cm_ai::suggest::{CutSuggestion, ZoomSuggestion};
use cm_ai::text_model::engine::{EngineSpec, EngineState, EngineStatus, TextEngine};
use cm_ai::text_model::store::{ResolvedTextModel, TextModelList};
use cm_ai::text_model::TextModelError;
use cm_ai::transcribe::Transcript;
use cm_core::edit::{parse_time, secs};
use cm_core::{Actor, CmError, EditList};
use cm_jobs::{Job, JobContext, JobFilter, JobHandler, JobQueue, JobState, NewJob, JOB_TRANSCRIBE};
use serde::{Serialize, Serializer};
use serde_json::{json, Map, Value};

use crate::action::{Action, Fields};
use crate::daemon::{blocking, lock_library, Daemon, Inner};
use crate::error::DaemonError;
use crate::events::{self, EventBus};

/// Job type of a user-started model download.
pub const JOB_MODEL_DOWNLOAD: &str = "model_download";
/// Job type of a user-started text-model download (AI-7, the built-in engine).
pub const JOB_TEXT_MODEL_DOWNLOAD: &str = "text_model_download";
/// The transcript in the bundle folder.
pub const TRANSCRIPT_FILE: &str = "transcript.json";
/// Setting: transcribe after every recording (default on only with at least [`AUTO_MIN_RAM_GB`]).
pub const SETTING_AUTO_TRANSCRIBE: &str = "ai.auto_transcribe";
/// Setting: find the speakers after each transcript (AI-8). Default ON (maintainer decision,
/// Oct 27, 2026); the daemon only starts it when the speaker models are usable.
pub const SETTING_AUTO_SPEAKERS: &str = "ai.auto_speakers";
/// Setting: the languages the user speaks (an array of codes, or `vi,en`). A transcript made
/// without a `language` chooses among them instead of among all of Whisper's languages: a short
/// or noisy recording is otherwise sometimes heard as a relative (Vietnamese as Indonesian).
pub const SETTING_LANGUAGES: &str = "ai.languages";
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

/// Job type of a speaker-labelling run (AI-8).
pub const JOB_DIARIZE: &str = "diarize";
/// Most renames, merges or reassignments one `set_speakers` call may carry.
pub const MAX_SPEAKER_CHANGES: usize = 500;

/// The action names of this module, as MCP tools and CLI commands.
pub const AI_METHODS: &[&str] = &[
    "list_models",
    "download_model",
    "import_model",
    "select_model",
    "remove_model",
    "list_text_models",
    "download_text_model",
    "select_text_model",
    "remove_text_model",
    "transcribe",
    "get_transcript",
    "get_captions",
    "set_captions",
    "replace_in_captions",
    "export_captions",
    "suggest_zooms",
    "suggest_cuts",
    "translate_captions",
    "diarize",
    "set_speakers",
    "list_report_templates",
    "generate_report",
    "get_report",
    "export_report",
    "delete_report",
    "edit_report",
    "check_report_item",
];

/// Whether `name` is one of [`AI_METHODS`].
pub fn is_ai_method(name: &str) -> bool {
    AI_METHODS.contains(&name)
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
    if !rec.kind.is_recording() {
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

mod actions;
mod captions;
mod dispatch;
mod jobs;
mod models;
mod reports;
mod services;
mod speakers;
mod suggest;
#[cfg(test)]
mod tests;
mod text_models;
mod transcripts;
mod translation;

pub use self::actions::*;
pub use self::jobs::*;
pub use self::reports::{ReportFormat, JOB_REPORT, MAX_REPORT_MARKDOWN_CHARS};
pub use self::services::*;
pub use self::translation::*;
