//! Where Click Guide gets its clicks from: a trait with the system implementation behind
//! [`DaemonBuilder::guide_system`](crate::DaemonBuilder::guide_system) and a scripted one for
//! tests (no real clicks anywhere in the test suite).

use std::sync::{Arc, Mutex, PoisonError};

use cm_capture::{CaptureTarget, Targets};
use cm_core::CmError;

/// One mouse-down, mapped to the followed target.
#[derive(Debug, Clone, PartialEq)]
pub struct LocalClick {
    /// Physical px inside the target's frame (may lie outside it: see `outside`).
    pub x: f64,
    pub y: f64,
    /// Where the OS says the pointer is, in its global space (for the HUD exclusion check).
    pub gx: f64,
    pub gy: f64,
    /// The click is not inside the target (GDE-1.AC2).
    pub outside: bool,
    /// `left`, `right` or `middle`.
    pub button: String,
    /// Event time in host nanoseconds.
    pub host_ns: u64,
}

/// Receives every mouse-down, in order, on a helper thread.
pub type ClickSink = Box<dyn FnMut(LocalClick) + Send>;

/// A source of mouse-downs. Never a source of keystrokes.
pub trait GuideClicks: Send + Sync {
    /// Starts delivering clicks on `target` into `sink`.
    ///
    /// # Errors
    /// `permission_missing(input_monitoring)` (GDE-1.AC5) or `not_implemented` with the reason
    /// (Wayland without input events, GDE-1.AC6).
    fn start(
        &self,
        target: &CaptureTarget,
        targets: &Targets,
        sink: ClickSink,
    ) -> Result<(), CmError>;

    /// Stops delivering; nothing reaches the sink afterwards.
    fn stop(&self);
}

/// The default of a daemon that was not given a click source.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoClicks;

impl GuideClicks for NoClicks {
    fn start(&self, _: &CaptureTarget, _: &Targets, _: ClickSink) -> Result<(), CmError> {
        Err(CmError::not_implemented(
            "Click Guide needs the CaptureMann app, which listens for clicks",
        ))
    }
    fn stop(&self) {}
}

/// A scripted click source: tests push clicks with [`ScriptedClicks::click`] (or
/// [`ScriptedClicks::click_at`]), or make the start fail with [`ScriptedClicks::failing`].
#[derive(Clone, Default)]
pub struct ScriptedClicks {
    inner: Arc<Mutex<ScriptState>>,
}

#[derive(Default)]
struct ScriptState {
    sink: Option<ClickSink>,
    fail: Option<CmError>,
    clock_ns: u64,
}

impl ScriptedClicks {
    pub fn new() -> Self {
        Self::default()
    }

    /// `start` answers `err` (a missing permission, a Wayland session).
    pub fn failing(err: CmError) -> Self {
        let s = Self::default();
        s.lock().fail = Some(err);
        s
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, ScriptState> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Whether a guide is listening.
    pub fn is_listening(&self) -> bool {
        self.lock().sink.is_some()
    }

    /// Delivers a left click at `(x, y)` inside the target (the pointer is where the click is).
    pub fn click(&self, x: f64, y: f64) -> bool {
        self.click_at(x, y, x, y, false, "left", 1_000_000_000)
    }

    /// Delivers a click with every field; `advance_ns` moves the scripted clock first.
    #[allow(clippy::too_many_arguments)]
    pub fn click_at(
        &self,
        x: f64,
        y: f64,
        gx: f64,
        gy: f64,
        outside: bool,
        button: &str,
        advance_ns: u64,
    ) -> bool {
        let mut g = self.lock();
        g.clock_ns += advance_ns;
        let host_ns = g.clock_ns;
        match g.sink.as_mut() {
            Some(sink) => {
                sink(LocalClick {
                    x,
                    y,
                    gx,
                    gy,
                    outside,
                    button: button.to_owned(),
                    host_ns,
                });
                true
            }
            None => false,
        }
    }
}

impl GuideClicks for ScriptedClicks {
    fn start(&self, _: &CaptureTarget, _: &Targets, sink: ClickSink) -> Result<(), CmError> {
        let mut g = self.lock();
        if let Some(e) = g.fail.clone() {
            return Err(e);
        }
        g.sink = Some(sink);
        Ok(())
    }
    fn stop(&self) {
        self.lock().sink = None;
    }
}

pub(crate) mod sys;
