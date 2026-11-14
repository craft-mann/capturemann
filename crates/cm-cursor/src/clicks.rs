//! Mouse-down listening for Click Guide (GDE-1): the REC-7 source with nothing but button-down
//! events kept.
//!
//! The platform source is the same one the cursor recorder uses (event tap, hook, X11 record);
//! its mask has no key events, so no keystroke can reach this module (NFR-9). Moves, scrolls and
//! cursor images are dropped here, and so are button-up events: a click is its mouse-down.

use std::sync::mpsc::RecvTimeoutError;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread::JoinHandle;
use std::time::Duration;

use cm_core::{CmError, Rect};

use crate::pipeline::Msg;
use crate::recorder::CursorConfig;
use crate::schema::{Availability, Button, CursorStatus};
use crate::source::{CursorSource, RawKind, RawSender};
use crate::target::CursorTarget;

/// One mouse-down, in the OS's global space (see [`CursorTarget`]).
#[derive(Debug, Clone, PartialEq)]
pub struct ClickEvent {
    /// OS event time, host nanoseconds.
    pub host_ns: u64,
    pub gx: f64,
    pub gy: f64,
    pub button: Button,
}

/// A running click listener. Dropping it stops it.
pub struct ClickListener {
    source: Box<dyn CursorSource>,
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
    status: CursorStatus,
}

impl std::fmt::Debug for ClickListener {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClickListener").finish_non_exhaustive()
    }
}

impl ClickListener {
    /// Listens on this system's real source. `on_click` runs on a helper thread, in order.
    ///
    /// # Errors
    /// `permission_missing(input_monitoring)` when the OS delivers no clicks (macOS without
    /// Input Monitoring; GDE-1.AC5), `not_implemented` when the session has no click events
    /// (Wayland; GDE-1.AC6).
    pub fn start(on_click: impl FnMut(ClickEvent) + Send + 'static) -> Result<Self, CmError> {
        let target = CursorTarget::display(0, Rect::new(0.0, 0.0, 1.0, 1.0), 1.0);
        let config = CursorConfig::new(target);
        let source = crate::platform::default_source(&config);
        Self::start_with_source(source, on_click)
    }

    /// [`ClickListener::start`] on an injected source (tests).
    pub fn start_with_source(
        mut source: Box<dyn CursorSource>,
        mut on_click: impl FnMut(ClickEvent) + Send + 'static,
    ) -> Result<Self, CmError> {
        let (tx, rx) = RawSender::channel();
        let status = source.start(tx)?;
        if status.clicks != Availability::Available {
            source.stop();
            let w = status.warnings.first();
            return Err(match w.and_then(|w| w.permission) {
                Some(p) => CmError::permission_missing(
                    p,
                    w.map(|w| w.message.clone())
                        .unwrap_or_else(|| "Click events are not available".to_owned()),
                ),
                None => CmError::not_implemented(
                    w.map(|w| w.message.clone())
                        .unwrap_or_else(|| "this session delivers no click events".to_owned()),
                ),
            });
        }
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let join = std::thread::spawn(move || {
            while !flag.load(Ordering::Relaxed) {
                match rx.recv_timeout(Duration::from_millis(50)) {
                    Ok(Msg::Raw(ev)) => {
                        if let RawKind::Button {
                            gx,
                            gy,
                            button,
                            down: true,
                            ..
                        } = ev.kind
                        {
                            on_click(ClickEvent {
                                host_ns: ev.host_ns,
                                gx,
                                gy,
                                button,
                            });
                        }
                    }
                    Ok(_) | Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            }
        });
        Ok(ClickListener {
            source,
            stop,
            join: Some(join),
            status,
        })
    }

    pub fn status(&self) -> &CursorStatus {
        &self.status
    }

    /// Stops the source and joins the helper thread; nothing is delivered afterwards.
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.source.stop();
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

impl Drop for ClickListener {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn only_mouse_downs_are_delivered() {
        let (src, handle) = crate::source::FakeCursorSource::full();
        let got = Arc::new(Mutex::new(Vec::new()));
        let g = got.clone();
        let mut l = ClickListener::start_with_source(Box::new(src), move |c| {
            g.lock().unwrap().push(c);
        })
        .unwrap();
        handle.mv(1, 1.0, 1.0);
        handle.click(2, 10.0, 20.0, Button::Left, true);
        handle.click(3, 10.0, 20.0, Button::Left, false);
        std::thread::sleep(Duration::from_millis(250));
        l.stop();
        let got = got.lock().unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!((got[0].host_ns, got[0].gx, got[0].gy), (2, 10.0, 20.0));
    }

    #[test]
    fn missing_permission_is_an_error_gde_1_ac5() {
        let status = CursorStatus::permission_fallback(cm_core::Permission::InputMonitoring, "off");
        let (src, _h) = crate::source::FakeCursorSource::new(status);
        let e = ClickListener::start_with_source(Box::new(src), |_| {}).unwrap_err();
        assert_eq!(e.code(), cm_core::ErrorCode::PermissionMissing);
    }
}
