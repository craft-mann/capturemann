//! The editor's video preview surface and the 1080p60 test that decides GPUI vs iced
//! (docs/SPIKES.md S5, docs/PLAN.md M4). `frame` and `stats` are display-free and unit tested; `view` is
//! the GPUI element; `macos` is the `CVPixelBuffer` path into GPUI's `surface()`.
//!
//! Measured by `cargo run --release -p cm-desktop --example preview_bench`.

pub mod convert;
pub mod frame;
#[cfg(target_os = "macos")]
pub mod macos;
pub mod present;
pub mod stats;
pub mod view;

pub use convert::{rgba_to_bgra, rgba_to_nv12};
pub use frame::{nv12_to_bgra, read_counter, Nv12Frame, SyntheticSource};
pub use present::{default_path, Presenter};
pub use stats::{PresentStats, PresentSummary};
pub use view::{PreviewPath, PreviewSurface, WorkerStats};
