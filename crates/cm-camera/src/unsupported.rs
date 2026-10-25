//! Non-macOS placeholder: the Windows camera backend (Media Foundation) is a later slice.

use std::sync::Arc;
use std::time::Duration;

use cm_core::CmError;

use crate::permission::CameraPermission;
use crate::preview::RgbaImage;
use crate::types::{CameraInfo, CameraSource, FrameSink, OpenRequest, SourceInfo};
use crate::writer::{FragmentWriter, WriterFactory, WriterSpec};

#[derive(Debug, Clone)]
pub struct NativeSample;

impl NativeSample {
    pub fn to_rgba(&self, _max_edge: u32, _mirror: bool) -> Option<RgbaImage> {
        None
    }
    pub fn io_surface_id(&self) -> Option<u32> {
        None
    }
}

fn unsupported() -> CmError {
    CmError::not_implemented("camera capture is not implemented on this platform yet")
}

pub fn permission_status() -> CameraPermission {
    CameraPermission::NotDetermined
}

pub fn request_access(_timeout: Duration) -> CameraPermission {
    CameraPermission::NotDetermined
}

pub fn list_cameras() -> Result<Vec<CameraInfo>, CmError> {
    Err(unsupported())
}

struct NoSource;

impl CameraSource for NoSource {
    fn open(&mut self, _r: &OpenRequest, _s: Arc<dyn FrameSink>) -> Result<SourceInfo, CmError> {
        Err(unsupported())
    }
    fn close(&mut self) {}
}

pub fn system_source() -> Box<dyn CameraSource> {
    Box::new(NoSource)
}

struct NoWriters;

impl WriterFactory for NoWriters {
    fn create(
        &self,
        _p: &std::path::Path,
        _s: &WriterSpec,
    ) -> Result<Box<dyn FragmentWriter>, CmError> {
        Err(unsupported())
    }
}

pub fn system_writer_factory() -> Arc<dyn WriterFactory> {
    Arc::new(NoWriters)
}
