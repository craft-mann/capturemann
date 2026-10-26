//! The recording UI's display-free logic (REC-1, REC-4, REC-5, REC-6, CAM-1, CAM-4).
//!
//! The windows (HUD, countdown, area indicator, webcam bubble, start sheet) live in
//! `ui::recording`; everything they show is decided here, so it is unit tested without a
//! display:
//!
//! * [`model`]: [`model::HudState`], the state machine fed by the daemon's `recording.*` events
//!   and `get_recording_status`, with the timer math across pauses.
//! * [`geometry`]: the area indicator (brackets, outline, size chips, scrim), the HUD placement
//!   and the webcam bubble presets, snapping and placement.
//! * [`hud_layout`]: the HUD's control order, icons and bar widths.
//! * [`start`]: the start sheet's options turned into `start_recording` parameters, and the
//!   remembered defaults (`recording.*`, `camera.*`, `audio.*` settings).
//! * [`countdown`]: the countdown ring.
//! * [`drag`]: whether a window drag is live (HUD and bubble).
//! * [`attention`]: the tray's Attention rule for failed publishes and an offline library.
//! * [`tray`]: the tray state machine and the hotkey decisions.
//! * [`preview`]: live camera frames for the bubble (shared session with the recorder).
//! * [`sheet`]: the start sheet's model (displays, devices, permissions, choices).
//! * [`files`]: finding a `.cmrec` bundle on disk, its details card and its poster.
//! * [`exports`]: the quick export's jobs and toasts ("Recording saved - exporting...").

pub mod attention;
pub mod audio_sheet;
pub mod countdown;
pub mod demo;
pub mod drag;
pub mod exports;
pub mod files;
pub mod geometry;
pub mod hud_layout;
pub mod model;
pub mod preview;
pub mod sheet;
pub mod start;
pub mod toasts;
pub mod tray;
pub mod upload_progress;
pub mod uploads;
pub mod window_rows;
