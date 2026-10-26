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
    "transcribe",
    "get_transcript",
    "get_captions",
    "set_captions",
    "replace_in_captions",
    "export_captions",
    "suggest_zooms",
    "suggest_cuts",
    "diarize",
    "set_speakers",
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

mod actions;
mod captions;
mod dispatch;
mod jobs;
mod models;
mod services;
mod speakers;
mod suggest;
#[cfg(test)]
mod tests;
mod transcripts;

pub use self::actions::*;
pub use self::jobs::*;
pub use self::services::*;
