//! Action bus, sessions, JSON-RPC server, event stream, confirmation gate.
//!
//! `cm-daemon` is the only crate that talks to the engines (`cm-capture`, `cm-library`,
//! `cm-render`, later recording, jobs and publishing); the desktop app, overlay, CLI and MCP
//! server are its clients (AGT-1). This crate is headless: a [`Daemon`] is built from injected
//! engines and can be driven in process ([`Daemon::execute`]) or over a local socket
//! ([`IpcServer`], [`serve_connection`]).
//!
//! # Map
//!
//! * [`action`]: the typed [`Action`] enum, the [`ACTIONS`] tier table, input validation.
//! * [`Daemon`] / [`DaemonBuilder`]: policy, idempotency, confirmation gate, per-capture
//!   serialization, engines, events.
//! * [`gate`]: [`Confirmer`] (the UI hook), [`ClientPolicy`], activity log, bounded confirm queue.
//! * [`config`]: where the app keeps `settings.json` and the library, and a file-backed
//!   [`config::FileSettings`] (used by the headless `cm-daemon`).
//! * [`engines`]: [`Clipboard`], [`SettingsStore`], [`ExternalActions`] (recording, publish,
//!   delete engines plug in here) and the `set_settings` policy ([`engines::APP_ONLY_SETTINGS`]).
//! * [`events`]: the event stream and the [`JobEvents`] bridge trait for `cm-jobs`.
//! * [`recording`]: [`recording::RecordingSession`] (the M3 track recorders wired together) and
//!   [`RecordingEngine`], the state machine behind `start_recording`, `pause_recording`,
//!   `resume_recording`, `stop_recording`, `get_recording_status`, `list_devices` and the HUD's
//!   internal `discard_recording`, `restart_recording`, `skip_countdown`. Turn it on with
//!   [`DaemonBuilder::recording_system`] (real sources) or [`DaemonBuilder::recording_sources`]
//!   (fakes). [`Daemon::recover_recordings`] recovers crashed recordings at startup.
//! * [`rpc`], [`ipc`], [`error`]: JSON-RPC protocol, transport and error-code mapping.
//! * [`testing`]: fakes used by this crate's tests and available to others.
//!
//! # Jobs (`cm-jobs`)
//!
//! [`DaemonBuilder::jobs`] gives the daemon a persistent queue; it serves `get_job`,
//! `list_jobs` and `cancel_job` and republishes `job.progress` / `job.done` / `job.failed` on the
//! event stream. Later crates plug in with [`Daemon::register_job_handler`]; the recording
//! engine uses [`Daemon::hold_job_type`]. See the `jobs` module source for the lifecycle and
//! the DB location. [`JobEvents`] / [`Daemon::bridge_job_events`] remain the generic bridge.

#![cfg_attr(test, allow(clippy::unwrap_used))]

pub mod action;
pub mod ai;
pub mod ai_engine;
pub mod auto_upload;
pub mod caption_options;
pub mod clock;
pub mod config;
mod daemon;
pub mod edit_ops;
pub mod engines;
pub mod error;
pub mod events;
pub mod export;
pub mod export_audio;
pub mod fixture;
mod frames;
pub mod gate;
mod handlers;
mod idem;
pub mod ipc;
mod jobs;
pub mod library_move;
mod library_ops;
pub mod publish_ops;
mod publish_records;
pub mod recording;
mod recovery;
mod remote_delete;
pub mod rpc;
pub mod storage_cloud;
pub mod storage_ops;
pub mod testing;
pub mod tool_policy;
mod upload_records;
pub mod watcher;

pub use action::{
    is_internal, spec_for, tier_of, Action, ActionSpec, Tier, ACTIONS, INTERNAL_ACTIONS, NEVER_TIER,
};
pub use ai::{
    AiServices, CaptionFormat, CaptionService, ModelService, SuggestService, TranscribeRequest,
    TranscribeService,
};
pub use clock::{Clock, ManualClock, SystemClock};
pub use daemon::{ActionCall, Daemon, DaemonBuilder};
pub use engines::{
    Clipboard, ExternalActions, MemorySettings, NoClipboard, NoExternalActions, SettingsStore,
};
pub use error::{rpc_code_for, DaemonError};
pub use events::{Event, EventBus, JobEvent, JobEventKind, JobEvents};
pub use frames::{DEFAULT_FRAME_TTL, MAX_FRAMES, MAX_FRAME_BYTES};
pub use gate::{
    ActivityDecision, ActivityEntry, Approval, ClientPolicies, ClientPolicy, ConfirmationRequest,
    Confirmer, Decision, DenyAll, DEFAULT_CONFIRM_TIMEOUT, FIRST_PARTY_CLIENTS,
};
pub use ipc::{connect, Endpoint, IpcServer};
pub use jobs::{library_job_queue, library_jobs_path, JOBS_RELATIVE_PATH};
pub use library_move::{MoveMode, MovePlan, MoveProblem, TrashReport};
pub use library_ops::{DeleteDescription, SystemTrasher, Trasher};
pub use recording::{EngineOptions, RecordingEngine, SourceFactory, SystemSources};
pub use recovery::{RecoveredRecording, RecoveredRecordings};
pub use rpc::serve_connection;
