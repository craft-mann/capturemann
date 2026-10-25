//! Camera hot-plug watching: tells the daemon when a camera appears or goes away (CAM-1), also
//! while nothing records, so the Record sheet's camera list can refresh.
//!
//! **Choice (macOS): a low-cost polling diff of the device list, not
//! `AVCaptureDeviceWasConnectedNotification`.** The notifications are posted on the thread that
//! owns the device discovery run loop, and the headless daemon has no main run loop; a polling
//! thread works in every host (app, headless daemon, tests) and a camera appearing a second or
//! two late is fine for a picker. Enumeration never prompts and does not light the camera. The
//! recorder's own `AVCaptureDeviceWasDisconnectedNotification` observer stays (it is what puts a
//! gap in the track); this module only reports the device list changing.
//!
//! The platform part is the [`CameraWatcher`] trait; [`PollingWatcher`] is the real
//! implementation over any list function and [`FakeCameraWatcher`] is the scripted fake.

use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::Duration;

use cm_core::CmError;

use crate::types::CameraInfo;

/// Default polling period of the system watcher.
pub const POLL_INTERVAL: Duration = Duration::from_millis(2000);

/// A camera appeared or went away.
#[derive(Debug, Clone, PartialEq)]
pub enum DeviceChange {
    Connected(CameraInfo),
    /// Carries the last known description of the device.
    Disconnected(CameraInfo),
}

impl DeviceChange {
    pub fn camera(&self) -> &CameraInfo {
        match self {
            DeviceChange::Connected(c) | DeviceChange::Disconnected(c) => c,
        }
    }
}

/// Receives changes on the watcher's own thread.
pub type ChangeSink = Arc<dyn Fn(DeviceChange) + Send + Sync>;

/// Watches the camera list. `start` takes the baseline (no events for cameras already there);
/// `stop` ends the watching and returns once no more changes will be delivered.
pub trait CameraWatcher: Send {
    fn start(&mut self, sink: ChangeSink) -> Result<(), CmError>;
    fn stop(&mut self);
}

/// The changes from `prev` to `now`, by device id: disconnections first, then connections, each
/// in list order. A device whose other fields changed (default flag, busy flag) is not a change.
pub fn diff_devices(prev: &[CameraInfo], now: &[CameraInfo]) -> Vec<DeviceChange> {
    let mut out = Vec::new();
    for p in prev {
        if !now.iter().any(|n| n.id == p.id) {
            out.push(DeviceChange::Disconnected(p.clone()));
        }
    }
    for n in now {
        if !prev.iter().any(|p| p.id == n.id) {
            out.push(DeviceChange::Connected(n.clone()));
        }
    }
    out
}

/// The JSON payload of `camera.connected` / `camera.disconnected`.
pub fn change_payload(c: &DeviceChange) -> serde_json::Value {
    let cam = c.camera();
    serde_json::json!({
        "device_id": cam.id,
        "name": cam.name,
        "kind": cam.kind,
        "is_default": cam.is_default,
    })
}

/// The event name of a change.
pub fn change_event_name(c: &DeviceChange) -> &'static str {
    match c {
        DeviceChange::Connected(_) => "camera.connected",
        DeviceChange::Disconnected(_) => "camera.disconnected",
    }
}

type ListFn = Arc<dyn Fn() -> Result<Vec<CameraInfo>, CmError> + Send + Sync>;

/// Polls a list function on a thread and reports the diffs.
pub struct PollingWatcher {
    list: ListFn,
    interval: Duration,
    stop_tx: Option<Sender<()>>,
    join: Option<JoinHandle<()>>,
}

impl PollingWatcher {
    pub fn new(
        interval: Duration,
        list: impl Fn() -> Result<Vec<CameraInfo>, CmError> + Send + Sync + 'static,
    ) -> Self {
        PollingWatcher {
            list: Arc::new(list),
            interval,
            stop_tx: None,
            join: None,
        }
    }
}

impl CameraWatcher for PollingWatcher {
    fn start(&mut self, sink: ChangeSink) -> Result<(), CmError> {
        if self.join.is_some() {
            return Ok(());
        }
        // A failing enumeration (no camera support on this platform) starts as an empty list.
        let mut known = (self.list)().unwrap_or_default();
        let (tx, rx) = mpsc::channel::<()>();
        let list = Arc::clone(&self.list);
        let interval = self.interval;
        let join = std::thread::Builder::new()
            .name("cm-camera-watch".into())
            .spawn(move || loop {
                match rx.recv_timeout(interval) {
                    Err(RecvTimeoutError::Timeout) => {}
                    // Stop requested, or the watcher was dropped.
                    Ok(()) | Err(RecvTimeoutError::Disconnected) => return,
                }
                // A failed poll keeps the last known list: no phantom disconnects.
                let Ok(now) = list() else { continue };
                for change in diff_devices(&known, &now) {
                    sink(change);
                }
                known = now;
            })
            .map_err(|e| CmError::internal(format!("could not start the camera watcher: {e}")))?;
        self.stop_tx = Some(tx);
        self.join = Some(join);
        Ok(())
    }

    fn stop(&mut self) {
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(());
        }
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

impl Drop for PollingWatcher {
    fn drop(&mut self) {
        self.stop();
    }
}

/// The watcher for this OS: polls [`crate::list_cameras`] every [`POLL_INTERVAL`].
pub fn system_watcher() -> Box<dyn CameraWatcher> {
    Box::new(PollingWatcher::new(POLL_INTERVAL, crate::list_cameras))
}

// ---- the scripted fake ---------------------------------------------------------------------

#[derive(Default)]
struct FakeWatchState {
    sink: Option<ChangeSink>,
    cameras: Vec<CameraInfo>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

/// A watcher a test drives by hand through its [`FakeWatcherHandle`]; changes are delivered
/// synchronously on the caller's thread, and only while the watcher is started.
pub struct FakeCameraWatcher {
    state: Arc<Mutex<FakeWatchState>>,
}

/// Plugs and unplugs cameras of a [`FakeCameraWatcher`].
#[derive(Clone)]
pub struct FakeWatcherHandle {
    state: Arc<Mutex<FakeWatchState>>,
}

impl FakeCameraWatcher {
    pub fn new() -> (FakeCameraWatcher, FakeWatcherHandle) {
        let state = Arc::new(Mutex::new(FakeWatchState::default()));
        (
            FakeCameraWatcher {
                state: Arc::clone(&state),
            },
            FakeWatcherHandle { state },
        )
    }
}

impl CameraWatcher for FakeCameraWatcher {
    fn start(&mut self, sink: ChangeSink) -> Result<(), CmError> {
        lock(&self.state).sink = Some(sink);
        Ok(())
    }
    fn stop(&mut self) {
        lock(&self.state).sink = None;
    }
}

impl FakeWatcherHandle {
    /// Plugs a camera in. Returns false when it was already there.
    pub fn connect(&self, cam: CameraInfo) -> bool {
        let sink = {
            let mut s = lock(&self.state);
            if s.cameras.iter().any(|c| c.id == cam.id) {
                return false;
            }
            s.cameras.push(cam.clone());
            s.sink.clone()
        };
        if let Some(sink) = sink {
            sink(DeviceChange::Connected(cam));
        }
        true
    }

    /// Unplugs a camera. Returns false when it was not there.
    pub fn disconnect(&self, id: &str) -> bool {
        let (cam, sink) = {
            let mut s = lock(&self.state);
            let Some(i) = s.cameras.iter().position(|c| c.id == id) else {
                return false;
            };
            (s.cameras.remove(i), s.sink.clone())
        };
        if let Some(sink) = sink {
            sink(DeviceChange::Disconnected(cam));
        }
        true
    }

    /// Whether the daemon (or test) has started the watcher and not stopped it.
    pub fn is_running(&self) -> bool {
        lock(&self.state).sink.is_some()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use crate::types::CameraKind;

    use super::*;

    pub(crate) fn cam(id: &str) -> CameraInfo {
        CameraInfo {
            id: id.into(),
            name: format!("Cam {id}"),
            kind: CameraKind::External,
            is_default: false,
            max_size: [1920, 1080],
            max_fps: 30.0,
            modes: Vec::new(),
            in_use_elsewhere: false,
        }
    }

    #[test]
    fn diff_reports_disconnects_then_connects_by_id() {
        let a = cam("a");
        let mut a_default = cam("a");
        a_default.is_default = true;
        assert!(diff_devices(std::slice::from_ref(&a), &[a_default]).is_empty());
        let d = diff_devices(&[a.clone(), cam("b")], &[cam("b"), cam("c")]);
        assert_eq!(
            d,
            vec![
                DeviceChange::Disconnected(a),
                DeviceChange::Connected(cam("c"))
            ]
        );
    }

    #[test]
    fn payload_has_id_name_kind() {
        let p = change_payload(&DeviceChange::Connected(cam("x")));
        assert_eq!(p["device_id"], "x");
        assert_eq!(p["name"], "Cam x");
        assert_eq!(p["kind"], "external");
        assert_eq!(
            change_event_name(&DeviceChange::Disconnected(cam("x"))),
            "camera.disconnected"
        );
    }

    #[test]
    fn fake_delivers_only_while_started() {
        let (mut w, h) = FakeCameraWatcher::new();
        let seen = Arc::new(Mutex::new(Vec::new()));
        assert!(h.connect(cam("before")));
        let s = Arc::clone(&seen);
        w.start(Arc::new(move |c| lock(&s).push(c))).unwrap();
        assert!(h.is_running());
        h.connect(cam("a"));
        assert!(!h.connect(cam("a")));
        assert!(h.disconnect("a"));
        assert!(!h.disconnect("a"));
        w.stop();
        h.connect(cam("late"));
        let seen = lock(&seen);
        assert_eq!(seen.len(), 2);
        assert!(matches!(&seen[0], DeviceChange::Connected(c) if c.id == "a"));
        assert!(matches!(&seen[1], DeviceChange::Disconnected(c) if c.id == "a"));
    }

    #[test]
    fn polling_watcher_reports_diffs_and_survives_list_errors() {
        let list: Arc<Mutex<Result<Vec<CameraInfo>, CmError>>> =
            Arc::new(Mutex::new(Ok(vec![cam("a")])));
        let l = Arc::clone(&list);
        let mut w = PollingWatcher::new(Duration::from_millis(5), move || lock(&l).clone());
        let seen = Arc::new(Mutex::new(Vec::<DeviceChange>::new()));
        let count = Arc::new(AtomicUsize::new(0));
        let (s, c) = (Arc::clone(&seen), Arc::clone(&count));
        w.start(Arc::new(move |ch| {
            lock(&s).push(ch);
            c.fetch_add(1, Ordering::SeqCst);
        }))
        .unwrap();
        let wait = |n: usize| {
            for _ in 0..400 {
                if count.load(Ordering::SeqCst) >= n {
                    return;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            panic!("timed out waiting for {n} changes");
        };
        // The baseline is silent; an error is not a disconnect.
        *lock(&list) = Err(CmError::internal("enumeration failed"));
        std::thread::sleep(Duration::from_millis(30));
        assert_eq!(count.load(Ordering::SeqCst), 0);
        *lock(&list) = Ok(vec![cam("a"), cam("b")]);
        wait(1);
        *lock(&list) = Ok(vec![cam("b")]);
        wait(2);
        w.stop();
        let seen = lock(&seen);
        assert!(matches!(&seen[0], DeviceChange::Connected(c) if c.id == "b"));
        assert!(matches!(&seen[1], DeviceChange::Disconnected(c) if c.id == "a"));
    }
}
