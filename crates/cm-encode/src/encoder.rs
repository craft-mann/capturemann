//! The encoder contract between a recording source (cm-capture) and a platform encoder.

use std::path::PathBuf;

use cm_core::{CmError, MediaTime};

use crate::settings::VideoPlan;
use crate::writer::WriterSummary;

/// Where and how to encode.
#[derive(Debug, Clone)]
pub struct EncoderConfig {
    /// The `.mp4` file to create (inside the bundle directory).
    pub path: PathBuf,
    pub plan: VideoPlan,
}

/// What happened to one appended frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppendOutcome {
    Written,
    /// The encoder had no room (`isReadyForMoreMediaData` was false); the frame was not written.
    NotReady,
}

/// The message of the error an encoder returns when `finish` finds that no frame was ever
/// written.
pub const NO_FRAMES_MESSAGE: &str = "No frames were recorded";

/// The error for a recording that ended before its first frame (R3-5): there is nothing to
/// keep, so the owner deletes the empty bundle and says so, instead of leaving it for recovery.
pub fn no_frames_error() -> CmError {
    CmError::invalid_input(NO_FRAMES_MESSAGE)
}

/// Is `e` the "no frames were recorded" error?
pub fn is_no_frames(e: &CmError) -> bool {
    matches!(e, CmError::InvalidInput { message } if message == NO_FRAMES_MESSAGE)
}

/// The stable reason (`platform_code`) of the error returned when the platform's hardware encoder
/// cannot be used for the plan (REC-2): there is no software fallback in v0.1.
pub const HARDWARE_ENCODER_UNAVAILABLE: &str = "hardware_encoder_unavailable";

/// The English message of that error. It has no parameters, so the app can translate it by id
/// (`hardware_encoder_unavailable` in `cm_core::message`).
pub const HARDWARE_ENCODER_UNAVAILABLE_MESSAGE: &str = "This Mac has no usable hardware H.264 encoder for this recording. Try a lower resolution, update to macOS 13 or later, or use a Mac from about 2016 or later.";

/// The error for a platform encoder that rejects or cannot start the plan: `platform_rejected`
/// with `platform_code` `hardware_encoder_unavailable`.
pub fn hardware_encoder_unavailable() -> CmError {
    CmError::platform_rejected(
        HARDWARE_ENCODER_UNAVAILABLE_MESSAGE,
        Some(HARDWARE_ENCODER_UNAVAILABLE.into()),
    )
}

/// Is `e` the "no usable hardware encoder" error?
pub fn is_hardware_encoder_unavailable(e: &CmError) -> bool {
    matches!(e, CmError::PlatformRejected { platform_code: Some(c), .. }
        if c == HARDWARE_ENCODER_UNAVAILABLE)
}

/// A hardware (or synthetic) video encoder writing a fragmented MP4 file.
///
/// `Frame` is the platform's GPU-surface handle (a retained `CMSampleBuffer` on macOS); cloning
/// it must be cheap (a reference count), because the recorder keeps the last frame to repeat it
/// through static moments (constant frame rate, REC-3).
pub trait VideoEncoder: Send + 'static {
    type Frame: Clone + Send + 'static;

    /// Appends one frame. `pts` is the file time (session time minus pause gaps) on the
    /// 120,000 ticks/s timebase and must increase strictly.
    fn append(
        &mut self,
        frame: &Self::Frame,
        pts: MediaTime,
        duration: MediaTime,
    ) -> Result<AppendOutcome, CmError>;

    /// The plan the encoder really runs with when it had to use a simpler one than asked for
    /// (another codec, say); `None` when it is the plan it was given.
    fn used_plan(&self) -> Option<crate::settings::VideoPlan> {
        None
    }

    /// First disk error seen by the file writer (`disk_full`), if any.
    fn check(&self) -> Result<(), CmError>;

    /// Fragments the file has received so far.
    fn fragments_written(&self) -> usize;

    /// Finishes the file: flushes the last fragment and closes it. Idempotent.
    fn finish(&mut self) -> Result<WriterSummary, CmError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use cm_core::ErrorCode;

    /// REC-2: no software fallback; a rejected hardware encoder is `platform_rejected` with a
    /// stable reason the app can translate, and the message has an id in `cm_core::message`.
    #[test]
    fn rec2_the_unavailable_error_has_a_stable_reason_and_a_message_id() {
        let e = hardware_encoder_unavailable();
        assert_eq!(e.code(), ErrorCode::PlatformRejected);
        assert!(is_hardware_encoder_unavailable(&e));
        assert_eq!(
            e.data(),
            Some(serde_json::json!({ "platform_code": "hardware_encoder_unavailable" }))
        );
        let words = e.to_string();
        assert!(
            words.contains("no usable hardware H.264 encoder"),
            "{words}"
        );
        assert!(words.contains("lower resolution"), "{words}");
        assert!(words.contains("macOS 13"), "{words}");
        assert!(words.contains("2016"), "{words}");
        assert_eq!(
            e.message_parts().map(|p| p.id),
            Some("hardware_encoder_unavailable")
        );
        assert!(!is_hardware_encoder_unavailable(
            &CmError::platform_rejected("x", None)
        ));
    }
}
