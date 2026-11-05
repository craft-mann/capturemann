//! Camera hot-plug while recording: a [`CameraSource`] wrapper that, when the device it opened
//! goes away, reopens the SAME device (by the stable id the first open reported) with back-off and
//! keeps delivering into the same sink.
//!
//! This is the platform-neutral state machine of the Windows backend (CAM-1; macOS lets
//! AVFoundation resume the session by itself and does not need it). The wrapped source only has to
//! do what every [`CameraSource`] does: `open` blocks until the first frame, and a lost device is
//! reported as [`CameraEvent::Disconnected`] or [`CameraEvent::Error`].
//!
//! ```text
//! Streaming --Disconnected/Error--> Lost --(close dead reader)--> Reconnecting --open ok--> Streaming
//!                                                  ^------ wait (back-off), open fails ------'
//! ```
//!
//! What the recorder sees (nothing new for it): `Disconnected` is forwarded at once (the recorder
//! flushes and stamps the gap start on the session clock), frames then resume through the same
//! sink and the recorder places them by their capture time, so the gap is exactly the missing time
//! on the `MediaTime` clock (also across a pause). `Reconnected` is forwarded after the reopened
//! source delivered its first frame; the recorder reports the restoration with that frame.
//! `PermissionRevoked` is never retried (the owner decides, PRM-1).
//!
//! Only recording is supervised: a preview source is passed through (the bubble shows its
//! placeholder and the picker reopens it).

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use cm_core::CmError;

use crate::types::{
    CameraEvent, CameraFrame, CameraSource, FrameSink, OpenRequest, SourceInfo, SourceRole,
};

/// Waits between reopen attempts: grows by doubling from `first` up to `max`, then stays there
/// (a camera can come back after minutes; each attempt is cheap).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Backoff {
    pub first: Duration,
    pub max: Duration,
}

impl Default for Backoff {
    fn default() -> Self {
        Backoff {
            first: Duration::from_millis(250),
            max: Duration::from_secs(2),
        }
    }
}

impl Backoff {
    /// The wait before attempt number `attempt` (0 based).
    pub fn delay(&self, attempt: u32) -> Duration {
        let factor = 1u32.checked_shl(attempt.min(16)).unwrap_or(u32::MAX);
        self.first.saturating_mul(factor).min(self.max)
    }
}

enum Msg {
    Lost,
    Stop,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

/// Forwards to the real sink and tells the supervisor when the device is lost.
struct Proxy {
    down: Arc<dyn FrameSink>,
    tx: Mutex<Sender<Msg>>,
}

impl FrameSink for Proxy {
    fn on_frame(&self, frame: &CameraFrame) {
        self.down.on_frame(frame);
    }
    fn on_event(&self, event: CameraEvent) {
        let lost = matches!(event, CameraEvent::Disconnected | CameraEvent::Error(_));
        self.down.on_event(event);
        if lost {
            let _ = lock(&self.tx).send(Msg::Lost);
        }
    }
}

struct Supervisor {
    tx: Sender<Msg>,
    join: Option<JoinHandle<()>>,
}

/// Wraps a source so a recording survives the camera being unplugged and plugged in again.
pub struct ReconnectingSource {
    inner: Arc<Mutex<Box<dyn CameraSource>>>,
    backoff: Backoff,
    supervisor: Option<Supervisor>,
}

impl ReconnectingSource {
    pub fn new(inner: Box<dyn CameraSource>, backoff: Backoff) -> Self {
        ReconnectingSource {
            inner: Arc::new(Mutex::new(inner)),
            backoff,
            supervisor: None,
        }
    }
}

impl CameraSource for ReconnectingSource {
    fn open(&mut self, req: &OpenRequest, sink: Arc<dyn FrameSink>) -> Result<SourceInfo, CmError> {
        self.close();
        if req.role != SourceRole::Record {
            return lock(&self.inner).open(req, sink);
        }
        let (tx, rx) = mpsc::channel::<Msg>();
        let proxy = Arc::new(Proxy {
            down: sink.clone(),
            tx: Mutex::new(tx.clone()),
        });
        let info = lock(&self.inner).open(req, proxy)?;
        // The device the first open settled on (the default camera when none was asked for): the
        // same one is reopened, never another.
        let mut pinned = req.clone();
        pinned.device_id = Some(info.device_id.clone());
        let inner = self.inner.clone();
        let backoff = self.backoff.clone();
        let stop_tx = tx.clone();
        let join = std::thread::Builder::new()
            .name("cm-camera-reconnect".into())
            .spawn(move || supervise(&inner, &pinned, &backoff, &sink, &tx, &rx))
            .map_err(|e| CmError::internal(format!("camera reconnect thread: {e}")))?;
        self.supervisor = Some(Supervisor {
            tx: stop_tx,
            join: Some(join),
        });
        Ok(info)
    }

    fn close(&mut self) {
        if let Some(mut s) = self.supervisor.take() {
            let _ = s.tx.send(Msg::Stop);
            if let Some(j) = s.join.take() {
                let _ = j.join();
            }
        }
        lock(&self.inner).close();
    }
}

impl Drop for ReconnectingSource {
    fn drop(&mut self) {
        self.close();
    }
}

fn supervise(
    inner: &Mutex<Box<dyn CameraSource>>,
    pinned: &OpenRequest,
    backoff: &Backoff,
    down: &Arc<dyn FrameSink>,
    tx: &Sender<Msg>,
    rx: &Receiver<Msg>,
) {
    loop {
        match rx.recv() {
            Ok(Msg::Lost) => {}
            Ok(Msg::Stop) | Err(_) => return,
        }
        // Release the dead reader before trying again.
        lock(inner).close();
        let mut attempt = 0u32;
        loop {
            if !wait(rx, backoff.delay(attempt)) {
                return;
            }
            attempt += 1;
            let proxy = Arc::new(Proxy {
                down: down.clone(),
                tx: Mutex::new(tx.clone()),
            });
            match lock(inner).open(pinned, proxy) {
                Ok(_) => {
                    down.on_event(CameraEvent::Reconnected);
                    break;
                }
                Err(e) if e.code() == cm_core::ErrorCode::PermissionMissing => {
                    down.on_event(CameraEvent::PermissionRevoked);
                    return;
                }
                // Not there yet, busy, still starting: try again.
                Err(_) => {}
            }
        }
    }
}

/// Waits `d`; false when a stop arrived meanwhile. A loss reported meanwhile is ignored (it is
/// the one being handled).
fn wait(rx: &Receiver<Msg>, d: Duration) -> bool {
    let end = Instant::now() + d;
    loop {
        let left = end.saturating_duration_since(Instant::now());
        match rx.recv_timeout(left) {
            Ok(Msg::Stop) | Err(RecvTimeoutError::Disconnected) => return false,
            Ok(Msg::Lost) => {}
            Err(RecvTimeoutError::Timeout) => return true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_doubles_to_the_cap() {
        let b = Backoff::default();
        let ms: Vec<_> = (0..6).map(|i| b.delay(i).as_millis()).collect();
        assert_eq!(ms, [250, 500, 1000, 2000, 2000, 2000]);
        assert_eq!(b.delay(u32::MAX), Duration::from_secs(2));
    }
}
