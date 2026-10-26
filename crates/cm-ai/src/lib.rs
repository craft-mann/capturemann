//! Local AI (M5): transcripts and captions (AI-1), auto-zoom suggestions (AI-3) and silence and
//! filler-word cut suggestions (AI-4). Everything runs on the user's machine; nothing is
//! downloaded without an explicit user action (specs/features/07-ai.md).
//!
//! This file is the **contract** the parallel M5 streams build against. Module owners:
//!
//! | Module | Owner stream | Content |
//! | --- | --- | --- |
//! | [`model`] | engine | model catalog, store, resumable download with consent, import, selection |
//! | [`transcribe`] | engine | `Transcriber` trait, `Transcript`, the scripted fake, `word_error_rate` |
//! | [`whisper`] | engine | the whisper.cpp implementation of `Transcriber` (Metal on macOS) |
//! | [`audio`] | engine | decode a track to mono 16 kHz `f32` with the FFmpeg sidecar |
//! | [`recording`] | engine | transcribe a bundle (tracks, pause gaps, session time), `transcript.json` |
//! | [`caption`] | engine | caption line building (42 chars, 2 lines, 1 to 7 s), SRT and VTT, cut and speed mapping |
//! | [`suggest`] | suggestions | auto-zoom (AI-3) and silence and filler cuts (AI-4); deterministic |
//!
//! # Actions the daemon, MCP and CLI expose (names are final; the daemon stream implements them)
//!
//! | Action (MCP tool) | CLI | Tier | Result |
//! | --- | --- | --- | --- |
//! | `list_models` | `capturemann ai models` | Free | `{ models: [{ id, label, size_mb, installed, active, path? }], active, dir }` |
//! | `download_model { model }` | `capturemann ai models download <id>` | **Confirm** (network and disk) | `{ job_id }`; progress as `job.progress` |
//! | `import_model { model, path }` | `capturemann ai models import <id> <file>` | Free | `{ model }` (checks the file) |
//! | `select_model { model }` | `capturemann ai models use <id>` | Free | `{ active }` |
//! | `remove_model { model }` | `capturemann ai models rm <id>` | Confirm | `{ removed }` |
//! | `transcribe { capture_id, language?, model? }` | `capturemann ai transcribe <id>` | Free | `{ job_id }`; without an installed model: error `invalid_input` with `data.reason = "no_model"` |
//! | `get_transcript { capture_id }` | `capturemann ai transcript <id>` | Free | the [`transcribe::Transcript`] |
//! | `set_captions { capture_id, lines }` | `capturemann ai captions set` | Free, one revision | `{ revision }` |
//! | `export_captions { capture_id, format }` | `capturemann export <id> --captions srt\|vtt` | Free | `{ path }` |
//! | `suggest_zooms { capture_id }` | `capturemann ai suggest zooms <id>` | Free | `{ suggestions: [...] }` (nothing is applied) |
//! | `suggest_cuts { capture_id }` | `capturemann ai suggest cuts <id>` | Free | `{ suggestions: [...] }` (nothing is applied) |
//!
//! `no_model` is `invalid_input` built by [`model::no_model`]; the shared error type carries no
//! free-form data, so the daemon adds `{"reason": "no_model"}` with [`model::error_data`].
//! A cancelled transcription is [`transcribe::cancelled`] (`user_declined`).
//!
//! Accepting a suggestion uses the existing `set_zooms` and `cut_segments` actions.
//! Events: `model.download.progress`, `model.changed`, `transcript.ready` (`{ capture_id }`).
//! The UI shows a gentle notice (never a silent download) while no model is installed and the
//! user has not dismissed it; the AI page in the app is the model manager (list, download with
//! progress, import from a file, select, remove).

#![cfg_attr(test, allow(clippy::unwrap_used))]

pub mod audio;
pub mod caption;
pub mod doubtful;
pub mod glossary;
pub mod model;
pub mod reapply;
pub mod recording;
pub mod suggest;
pub mod textmatch;
pub mod transcribe;
pub mod vad;
pub mod whisper;
