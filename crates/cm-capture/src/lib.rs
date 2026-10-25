//! Screen capture backends: screenshots ([`Capturer`]) and the screen recording track
//! ([`record::ScreenRecorder`], a `cm_core::TrackRecorder`; ScreenCaptureKit + VideoToolbox on
//! macOS, a stub on Windows). [`disk`] holds the NFR-7 disk-space guard.
//!
//! Headless API, no UI:
//!
//! * [`Capturer::list_targets`] returns displays and windows (the `list_targets` MCP tool shape).
//! * [`Capturer::capture`] captures a display, a window or a region of a display at physical
//!   size, optionally after a delay (CAP-1, CAP-5).
//! * [`CaptureBackend`] abstracts the OS. `XcapBackend` is the real one (macOS and Windows);
//!   [`fake::FakeBackend`] is a deterministic in-memory one so the crop and scale math is
//!   tested without a display.
//!
//! Units: bounds returned by this crate are **physical px**; logical px only appear in
//! [`RegionRect::Logical`] input.

#![cfg_attr(test, allow(clippy::unwrap_used))]

mod backend;
mod capturer;
pub mod disk;
pub mod fake;
mod frame;
pub mod record;
mod region;
#[cfg(target_os = "macos")]
pub mod sck_shot;
mod types;
#[cfg(any(target_os = "macos", target_os = "windows"))]
mod xcap_backend;

pub use backend::CaptureBackend;
pub use capturer::{
    crop_display_frame, delay_from_secs, is_shell_window, CancelToken, Capture, CaptureRequest,
    Capturer, MAX_DELAY_SECS, SHELL_APPS,
};
pub use frame::Frame;
pub use record::WindowProvider;
pub use region::resolve_region;
pub use types::{CaptureKind, CaptureTarget, DisplayInfo, RegionRect, Targets, WindowInfo};
#[cfg(any(target_os = "macos", target_os = "windows"))]
pub use xcap_backend::XcapBackend;
