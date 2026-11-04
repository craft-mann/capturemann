//! The camera track's file writer on Windows.

use std::path::Path;

use cm_core::CmError;

use crate::writer::{FragmentWriter, WriterFactory, WriterSpec};

pub struct MfWriterFactory;

impl WriterFactory for MfWriterFactory {
    fn create(&self, _path: &Path, _spec: &WriterSpec) -> Result<Box<dyn FragmentWriter>, CmError> {
        Err(CmError::not_implemented(
            "Recording the camera on Windows needs the Windows hardware encoder, which is not built yet",
        ))
    }
}
