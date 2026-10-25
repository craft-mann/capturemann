//! Windows screen source: Windows.Graphics.Capture is a later slice (it cannot be built or run on
//! the macOS development machine). This stub compiles and fails with a clear error.

use cm_core::CmError;

use super::plan::CapturePlan;
use super::recorder::ScreenRecorder;
use super::source::{EventSink, ScreenSource};

/// Placeholder for the Windows.Graphics.Capture source.
#[derive(Debug, Default)]
pub struct WgcSource;

impl ScreenSource for WgcSource {
    type Frame = ();

    fn check_permission(&self) -> Result<(), CmError> {
        Ok(()) // Windows has no screen-capture permission prompt.
    }

    fn start(&mut self, _plan: &CapturePlan, _sink: EventSink<()>) -> Result<(), CmError> {
        Err(cm_encode::windows::not_implemented())
    }

    fn stop(&mut self) {}
}

pub type SystemScreenRecorder = ScreenRecorder<WgcSource, cm_encode::HardwareEncoder>;

impl SystemScreenRecorder {
    pub fn system(plan: CapturePlan) -> Self {
        ScreenRecorder::new(plan, WgcSource, |cfg| {
            cm_encode::HardwareEncoder::create(cfg)
        })
    }
}
