//! Windows screen source: Windows.Graphics.Capture (`sys/win_wgc.rs`) frames, stamped on the host
//! clock, delivered as CPU frames to the Media Foundation hardware encoder. A display, a region of
//! a display or one window; the cursor is drawn by WGC when the plan asks for it.
//!
//! Excluding CaptureMann's own windows from the picture (REC-6) is not done here: WGC captures
//! what the compositor shows, so the app's recorder windows must opt out themselves with
//! `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)`; `plan.exclusions` is not read on Windows.

use cm_core::{CmError, HostClock, SystemHostClock};
use cm_encode::CpuFrame;

use super::plan::CapturePlan;
use super::recorder::ScreenRecorder;
use super::source::{EventSink, ScreenSource, SourceEvent, StopReason};
use super::wgc_logic::{stamp_to_ns, StampMapper};
use crate::sys::win_wgc::{WgcCapture, WgcEvent, WgcTarget};
use crate::CaptureTarget;

/// The Windows.Graphics.Capture screen source.
#[derive(Default)]
pub struct WgcSource {
    capture: Option<WgcCapture>,
}

impl std::fmt::Debug for WgcSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "WgcSource(running: {})", self.capture.is_some())
    }
}

/// Which WGC target a plan means: displays and regions capture the display, windows the window.
fn target_of(plan: &CapturePlan) -> Result<WgcTarget, CmError> {
    match plan.target {
        CaptureTarget::Window { window_id } => Ok(WgcTarget::Window(window_id)),
        CaptureTarget::Display { display_id } | CaptureTarget::Region { display_id, .. } => {
            Ok(WgcTarget::Monitor(plan.display_id.unwrap_or(display_id)))
        }
    }
}

impl ScreenSource for WgcSource {
    type Frame = CpuFrame;

    fn check_permission(&self) -> Result<(), CmError> {
        Ok(()) // Windows has no screen-capture permission prompt.
    }

    fn start(&mut self, plan: &CapturePlan, sink: EventSink<CpuFrame>) -> Result<(), CmError> {
        let target = target_of(plan)?;
        let mut sink = sink;
        let mut mapper = StampMapper::default();
        let mut warned = false;
        let wgc_sink = Box::new(move |ev: WgcEvent| match ev {
            WgcEvent::Frame { stamp_100ns, frame } => {
                let now = SystemHostClock.now_ns();
                let host_ns = mapper.host_ns(stamp_to_ns(stamp_100ns), now);
                if mapper.trusted() == Some(false) && !warned {
                    warned = true;
                    log::warn!(
                        "capture frame times are not on the host clock; using arrival times"
                    );
                }
                sink(SourceEvent::Frame { host_ns, frame });
            }
            WgcEvent::Closed => sink(SourceEvent::Stopped(StopReason::TargetClosed(
                "The captured window or display was closed".into(),
            ))),
            WgcEvent::Failed(m) => sink(SourceEvent::Stopped(StopReason::Error(m))),
        });
        self.capture = Some(WgcCapture::start(
            target,
            plan.show_cursor,
            plan.crop,
            wgc_sink,
        )?);
        Ok(())
    }

    fn stop(&mut self) {
        if let Some(mut c) = self.capture.take() {
            c.stop();
        }
    }
}

pub type SystemScreenRecorder = ScreenRecorder<WgcSource, cm_encode::HardwareEncoder>;

impl SystemScreenRecorder {
    /// A recorder for `plan` on the real display stack. No software fallback (REC-2): a computer
    /// without a hardware H.264 encoder fails the start with `hardware_encoder_unavailable`.
    pub fn system(plan: CapturePlan) -> Self {
        ScreenRecorder::new(plan, WgcSource::default(), |cfg| {
            cm_encode::HardwareEncoder::create(cfg)
        })
    }
}
