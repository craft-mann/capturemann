//! The OS side of the screen track: something that delivers frames.

use cm_core::CmError;

use super::plan::CapturePlan;

/// Why a source ended by itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopReason {
    /// The captured window or display went away (REC-1 edge cases).
    TargetClosed(String),
    /// The stream failed.
    Error(String),
    /// The OS withdrew Screen Recording access while the stream ran (ScreenCaptureKit
    /// `SCStreamErrorUserDeclined`, -3801). Typed so the owner can say why (PRM-1).
    PermissionRevoked(String),
}

/// What a source reports. `host_ns` is the OS capture timestamp on the host clock timebase.
pub enum SourceEvent<F> {
    /// A frame with pixels.
    Frame { host_ns: u64, frame: F },
    /// A status-only sample: the screen has not changed since the last frame.
    Idle { host_ns: u64 },
    /// The source ended on its own.
    Stopped(StopReason),
}

/// Receives events on the source's own thread or queue.
pub type EventSink<F> = Box<dyn FnMut(SourceEvent<F>) + Send>;

/// A screen capture backend: ScreenCaptureKit on macOS, Windows.Graphics.Capture later, a fake in
/// tests.
pub trait ScreenSource: Send + 'static {
    /// The GPU-surface handle delivered to the encoder.
    type Frame: Clone + Send + 'static;

    /// `Err(permission_missing(screen))` when the OS denies screen capture. Must not prompt.
    fn check_permission(&self) -> Result<(), CmError>;

    /// Starts delivering frames for `plan` to `sink`. Returns once the stream is running.
    fn start(&mut self, plan: &CapturePlan, sink: EventSink<Self::Frame>) -> Result<(), CmError>;

    /// Stops delivering. After this returns no more events reach the sink.
    fn stop(&mut self);
}
