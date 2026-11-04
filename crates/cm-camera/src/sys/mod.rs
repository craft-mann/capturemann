//! Windows backend: Media Foundation camera enumeration and capture (an `IMFSourceReader` per
//! open camera) and the camera privacy switches. All `unsafe` in the Windows build lives in this
//! folder, each block with a SAFETY comment; the logic that decides things (formats, modes, stamps)
//! is in `crate::convert` and tested without Windows.

mod consent;
mod devices;
mod mf;
mod source;
mod writer;

use std::sync::Arc;

use crate::types::CameraSource;
use crate::writer::WriterFactory;

pub use consent::permission_status;
pub use devices::describe_native_modes as devices_describe_native_modes;
pub use devices::{list_cameras, request_access};

pub fn system_source() -> Box<dyn CameraSource> {
    Box::new(source::MfSource::default())
}

pub fn system_writer_factory() -> Arc<dyn WriterFactory> {
    Arc::new(writer::MfWriterFactory)
}
