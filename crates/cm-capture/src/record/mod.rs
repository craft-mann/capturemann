//! The screen recording track (M3 slice 1): REC-1 to REC-4, REC-6 and the screen half of REC-8.
//!
//! * [`plan`]: target and settings to a [`CapturePlan`] (display, window or region; exclusions).
//! * [`timeline`]: pause gaps and the session-to-file time mapping.
//! * [`pipeline`]: frame retiming to constant frame rate, repeats, late-frame accounting.
//! * [`source`]: the OS side ([`ScreenSource`]); `sck` is ScreenCaptureKit on macOS.
//! * [`recorder`]: [`ScreenRecorder`], implementing `cm_core::TrackRecorder`.
//! * [`fake`]: a scriptable source for tests without a display.

pub mod exclude;
pub mod fake;
pub mod pipeline;
pub mod pipewire;
pub mod plan;
pub mod recorder;
#[cfg(target_os = "macos")]
pub mod sck;
pub mod source;
pub mod timeline;

pub use pipeline::{HaltReason, LateTracker, Pipeline, PipelineStats, HEALTH_WINDOW_SECS};
pub use plan::{plan_capture, CapturePlan, Exclusions, ScreenSettings, WindowProvider};
pub use recorder::{
    Latency, RecorderEvent, RecorderOptions, ScreenRecorder, ScreenStatus, SCREEN_FILE,
};
pub use source::{EventSink, ScreenSource, SourceEvent, StopReason};
pub use timeline::{PauseTimeline, MAX_PAUSES};

/// The ScreenCaptureKit + VideoToolbox screen recorder.
#[cfg(target_os = "macos")]
pub type SystemScreenRecorder = ScreenRecorder<sck::SckSource, cm_encode::HardwareEncoder>;

#[cfg(target_os = "macos")]
impl SystemScreenRecorder {
    /// A recorder for `plan` on the real display stack.
    pub fn system(plan: CapturePlan) -> Self {
        // No software fallback (REC-2): VideoToolbox rejecting the plan fails the start.
        ScreenRecorder::new(
            plan,
            sck::SckSource::new(),
            cm_encode::HardwareEncoder::create,
        )
    }
}

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::{SystemScreenRecorder, WgcSource};
