//! macOS backend: AVFoundation camera capture and the AVAssetWriter CMAF writer.
//!
//! All `unsafe` in this crate lives in this module tree. The unsafe calls are Objective-C /
//! CoreMedia calls through the `objc2-*` bindings; each block carries a SAFETY note naming the
//! invariant. No Swift bridge, so no libswift link requirements (docs/SPIKES.md S6).

mod devices;
mod sample;
mod session;
mod writer;

use std::sync::Arc;

use cm_core::{CmError, SystemHostClock};

use crate::types::{CameraInfo, CameraSource};
use crate::writer::WriterFactory;

pub use devices::{permission_status, request_access};
pub use sample::NativeSample;

pub fn list_cameras() -> Result<Vec<CameraInfo>, CmError> {
    devices::list()
}

pub fn system_source() -> Box<dyn CameraSource> {
    Box::new(session::HubSource::default())
}

pub fn system_writer_factory() -> Arc<dyn WriterFactory> {
    Arc::new(writer::AvWriterFactory)
}

/// Host clock in ns (`CLOCK_UPTIME_RAW`), the timebase AVFoundation stamps camera frames on.
pub(crate) fn host_ns() -> u64 {
    use cm_core::HostClock;
    SystemHostClock.now_ns()
}

/// CMTime to nanoseconds; 0 for invalid times.
pub(crate) fn cm_ns(t: objc2_core_media::CMTime) -> i64 {
    if t.timescale == 0 {
        0
    } else {
        (i128::from(t.value) * 1_000_000_000 / i128::from(t.timescale)) as i64
    }
}

/// Wrapper to move Objective-C objects across threads where Apple documents them as thread safe
/// (AVCaptureSession, AVAssetWriter, CMSampleBuffer). Access is additionally serialized by the
/// owner's mutex.
pub(crate) struct SendBox<T>(pub T);
// SAFETY: only used for the Apple-documented thread-safe classes listed above, behind a mutex.
unsafe impl<T> Send for SendBox<T> {}
// SAFETY: as above; shared access never mutates without the owner's lock.
unsafe impl<T> Sync for SendBox<T> {}
