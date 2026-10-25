//! Cursor telemetry capture and cursor image cache (REC-7, feeds EDT-4).
//!
//! [`CursorRecorder`] implements [`cm_core::TrackRecorder`]: it writes `cursor.jsonl` (see
//! [`schema`] for the line format and the coordinate convention) and `cursors/` (PNG per distinct
//! cursor image plus `index.json`) into the bundle directory. [`CursorLog::read`] reads the track
//! back. No keystroke content is ever captured: the event mask contains no key events, modifiers
//! are flags on button events, scroll is direction only (NFR-9).

#![cfg_attr(test, allow(clippy::unwrap_used))]

pub mod image;
mod pipeline;
mod platform;
mod reader;
mod recorder;
pub mod schema;
mod source;
mod target;
pub mod view;

pub use image::{CursorImage, ImageEntry, ImageIndex};
pub use platform::preflight;
pub use reader::CursorLog;
pub use recorder::{CursorConfig, CursorRecorder};
pub use schema::{
    Availability, Button, CursorEvent, CursorHeader, CursorStatus, CursorWarning, Modifier,
    MoveSource, CURSORS_DIR, CURSOR_FILE, SCHEMA_VERSION,
};
pub use source::{
    spawn_image_poller, spawn_position_poller, Activity, CursorSource, FakeCursorSource,
    FakeHandle, ImageProvider, RawEvent, RawKind, RawSender, POLL_HZ,
};
pub use target::{CursorTarget, TargetKind};

pub(crate) use recorder::io_err;

#[cfg(target_os = "macos")]
pub use platform::main_display_target;

#[cfg(test)]
mod tests;
