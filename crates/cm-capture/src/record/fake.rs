//! A scriptable [`ScreenSource`] for tests that need no display.

use std::sync::{Arc, Mutex};

use cm_core::{CmError, Permission};
use cm_encode::synthetic::SyntheticFrame;

use super::plan::CapturePlan;
use super::source::{EventSink, ScreenSource, SourceEvent, StopReason};

#[derive(Default)]
struct Inner {
    sink: Mutex<Option<EventSink<SyntheticFrame>>>,
    plan: Mutex<Option<CapturePlan>>,
}

/// The test's side: pushes events as if the OS delivered them.
#[derive(Clone, Default)]
pub struct FakeScreenHandle(Arc<Inner>);

impl FakeScreenHandle {
    fn emit(&self, ev: SourceEvent<SyntheticFrame>) {
        if let Ok(mut s) = self.0.sink.lock() {
            if let Some(sink) = s.as_mut() {
                sink(ev);
            }
        }
    }

    /// Delivers a frame stamped `host_ns`.
    pub fn frame(&self, host_ns: u64, id: u64) {
        self.emit(SourceEvent::Frame {
            host_ns,
            frame: SyntheticFrame(id),
        });
    }

    /// Delivers a "screen unchanged" status sample.
    pub fn idle(&self, host_ns: u64) {
        self.emit(SourceEvent::Idle { host_ns });
    }

    /// The source ends by itself (window closed).
    pub fn close_target(&self, what: &str) {
        self.emit(SourceEvent::Stopped(StopReason::TargetClosed(what.into())));
    }

    /// The stream fails (for example Screen Recording access was revoked mid-recording: SCK's
    /// `didStopWithError`).
    pub fn stop_with_error(&self, message: &str) {
        self.emit(SourceEvent::Stopped(StopReason::Error(message.into())));
    }

    /// Screen Recording access is withdrawn while recording (SCK -3801 "user declined TCCs").
    pub fn revoke_permission(&self) {
        self.emit(SourceEvent::Stopped(StopReason::PermissionRevoked(
            "The user declined TCCs for application, window, display capture".into(),
        )));
    }

    /// The plan the recorder started the source with.
    pub fn plan(&self) -> Option<CapturePlan> {
        self.0.plan.lock().ok().and_then(|p| p.clone())
    }

    pub fn is_running(&self) -> bool {
        self.0.sink.lock().is_ok_and(|s| s.is_some())
    }
}

pub struct FakeScreenSource {
    inner: Arc<Inner>,
    permission: bool,
}

impl FakeScreenSource {
    pub fn new() -> (Self, FakeScreenHandle) {
        let inner = Arc::new(Inner::default());
        (
            FakeScreenSource {
                inner: Arc::clone(&inner),
                permission: true,
            },
            FakeScreenHandle(inner),
        )
    }

    /// A source whose permission check fails like macOS without Screen Recording access.
    pub fn without_permission() -> (Self, FakeScreenHandle) {
        let (mut s, h) = Self::new();
        s.permission = false;
        (s, h)
    }
}

impl ScreenSource for FakeScreenSource {
    type Frame = SyntheticFrame;

    fn check_permission(&self) -> Result<(), CmError> {
        if self.permission {
            Ok(())
        } else {
            Err(CmError::permission_missing(
                Permission::Screen,
                "Screen Recording permission is not granted",
            ))
        }
    }

    fn start(
        &mut self,
        plan: &CapturePlan,
        sink: EventSink<SyntheticFrame>,
    ) -> Result<(), CmError> {
        if let Ok(mut p) = self.inner.plan.lock() {
            *p = Some(plan.clone());
        }
        if let Ok(mut s) = self.inner.sink.lock() {
            *s = Some(sink);
        }
        Ok(())
    }

    fn stop(&mut self) {
        if let Ok(mut s) = self.inner.sink.lock() {
            *s = None;
        }
    }
}
