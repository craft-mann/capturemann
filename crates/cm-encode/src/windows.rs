//! Windows hardware encoder: Media Foundation (NVENC / Quick Sync / AMF behind it) is a later
//! slice. This stub keeps the crate compiling on Windows and fails with a clear error.

use cm_core::{CmError, MediaTime};

use crate::encoder::{AppendOutcome, EncoderConfig, VideoEncoder};
use crate::writer::WriterSummary;

/// The error the Windows encoder returns today: the shared `not_implemented` code.
pub fn not_implemented() -> CmError {
    CmError::not_implemented("the Windows Media Foundation hardware encoder is not built yet")
}

/// Placeholder for the Media Foundation encoder.
pub struct HardwareEncoder;

impl HardwareEncoder {
    pub fn create(_cfg: &EncoderConfig) -> Result<Self, CmError> {
        Err(not_implemented())
    }
}

impl VideoEncoder for HardwareEncoder {
    type Frame = ();

    fn append(&mut self, _: &(), _: MediaTime, _: MediaTime) -> Result<AppendOutcome, CmError> {
        Err(not_implemented())
    }
    fn check(&self) -> Result<(), CmError> {
        Err(not_implemented())
    }
    fn fragments_written(&self) -> usize {
        0
    }
    fn finish(&mut self) -> Result<WriterSummary, CmError> {
        Err(not_implemented())
    }
}
