//! `cm-media`: the FFmpeg *sidecar* side of CaptureMann (technical design, "Export and render").
//!
//! This crate holds the **interim exporter** ("quick" preset): after a recording stops, one
//! combined MP4 (screen + round camera bubble with the 4 px rec ring + microphone and system
//! audio) is rendered with an FFmpeg child process into the bundle's `exports/` folder. The masters
//! are never touched. With an edit list that changes the picture the GPU compositor (`cm-render`) renders it; the
//! interface above this crate (job type `export`, action `export_video`, the manifest `exports`
//! entry, preset names) is the one the spec defines and does not change.
//!
//! # Layers
//!
//! * [`tools`]: locate `ffmpeg`/`ffprobe` (env `CM_FFMPEG`, next to the executable, `PATH`, the
//!   Homebrew prefixes), probe the version and encoders, choose the H.264 encoder.
//! * [`spec`]: reads the recording manifest (tracks, gaps, `edits[0].layout.camera`, settings) into
//!   a [`spec::QuickSpec`].
//! * [`timeline`]: the pure offset math between tracks on the session clock (pause gaps removed).
//! * [`layout`]: the camera bubble as fractions of the recorded area, and its pixel geometry.
//! * [`preset`]: the export presets (EXP-2 platform presets, EXP-3 GIF and WebM) as data, and how one
//!   becomes a concrete [`preset::Target`] for a recording.
//! * [`graph`]: the pure FFmpeg argument and filter-graph builder (unit tested without FFmpeg).
//! * [`bubble`]: renders the bubble mask and ring PNGs the graph reads.
//! * [`run`]: runs FFmpeg with `-progress pipe:1` parsing, cancellation and a stderr tail.
//! * [`quick`]: the end-to-end quick export, `export_quick`.
//! * [`decode`]: the continuous preview decoder (one long-lived FFmpeg process per track, a frame
//!   cache, forward reads while playing) and the audio bank the editor plays.
//! * [`edited`] and [`edit_plan`]: the edit-list export (EXP-1): FFmpeg decode, the `cm-render`
//!   compositor, FFmpeg encode; `export_quick` uses it when the edit list changes the picture.
//!
//! # LGPL and the bundled FFmpeg (NFR-12)
//!
//! The product must not ship GPL code. This crate uses whatever `ffmpeg` it finds and records the
//! encoder it used in the export entry. The FFmpeg we *bundle* later (`Contents/MacOS/ffmpeg`)
//! must be an **LGPL build without `libx264`** (VideoToolbox on macOS, Media Foundation on
//! Windows; no software H.264 encoder ships). `libx264` is only a fallback for developer machines that have a GPL build: it is
//! last in [`Encoder::PREFERENCE`], flagged by `Encoder::lgpl_safe`, written to the export entry
//! (`settings.video.lgpl_safe`, `QuickOutcome::lgpl_safe`) and refused by
//! `tools::encoder_candidates(.., lgpl_only = true)`, so a shipping build can require LGPL.

#![cfg_attr(test, allow(clippy::unwrap_used))]

pub mod bubble;
pub mod captions;
pub mod decode;
pub mod dsp_audio;
pub mod edit_plan;
pub mod edit_view;
pub mod edited;
pub mod graph;
pub mod layout;
pub mod layout_plan;
pub mod preset;
pub mod quick;
pub mod run;
pub mod spec;
pub mod timeline;
pub mod tools;

pub use captions::{caption_layer_at, cue_at, CaptionCue, CaptionSource, CaptionTrack};
pub use edited::{check_request, ExportPath};
pub use layout::{CameraLayout, Shape};
pub use preset::{ExportOptions, Kind, Presets, Target};
pub use quick::{
    export_quick, ExportError, PresetRequest, QuickOutcome, QuickRequest, PRESET_QUICK,
};
pub use spec::QuickSpec;
pub use tools::{encoder_candidates, is_encoder_init_failure, Caps, Encoder, Tools};
