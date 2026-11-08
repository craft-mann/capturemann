//! Systems without AVFoundation: Windows (Media Foundation) and Linux (Video4Linux2) have a camera
//! backend in `sys/`; any other system answers `not_implemented` from `sys/none.rs`.

use crate::convert::RawFrame;
use crate::preview::RgbaImage;

/// A camera picture kept as bytes (Media Foundation and Video4Linux2 deliver system-memory
/// frames). Cloning shares the pixels.
#[derive(Debug, Clone)]
pub struct NativeSample(pub(crate) RawFrame);

impl NativeSample {
    pub fn to_rgba(&self, max_edge: u32, mirror: bool) -> Option<RgbaImage> {
        self.0.to_rgba(max_edge, mirror)
    }
    pub fn io_surface_id(&self) -> Option<u32> {
        None
    }
}

pub use crate::sys::{
    list_cameras, permission_status, request_access, system_source, system_writer_factory,
};
