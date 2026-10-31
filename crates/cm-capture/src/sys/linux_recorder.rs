//! The Linux screen recorder: the portal ScreenCast + PipeWire source and the software VP9 encoder
//! (an FFmpeg child process) behind the same [`ScreenRecorder`] the other systems use.
//!
//! It lives here, and compiles on every system, so the daemon can build it without `target_os`
//! lines of its own; only the portal backend inside [`PortalPipeWire`] is Linux code (elsewhere it
//! answers `not_implemented`). See `specs/plans/linux-recording-plan.md`, section 7 (D-R2, D-R3).

use std::path::PathBuf;

use cm_encode::FfmpegVp9Encoder;

use crate::record::pipewire::{FileTokenStore, PipeWireScreenSource};
use crate::record::{CapturePlan, ScreenRecorder};
use crate::sys::pipewire_linux::PortalPipeWire;

/// The screen recorder of a Linux desktop.
pub type LinuxScreenRecorder =
    ScreenRecorder<PipeWireScreenSource<PortalPipeWire, FileTokenStore>, FfmpegVp9Encoder>;

/// A recorder for `plan` on the portal. `token_file` keeps the portal's restore token (the caller
/// picks the folder, normally the app's config folder), so the compositor's dialog only appears
/// the first time.
pub fn linux_screen_recorder(plan: CapturePlan, token_file: PathBuf) -> LinuxScreenRecorder {
    ScreenRecorder::new(
        plan,
        PipeWireScreenSource::new(PortalPipeWire::new(), FileTokenStore(token_file)),
        FfmpegVp9Encoder::create,
    )
}
