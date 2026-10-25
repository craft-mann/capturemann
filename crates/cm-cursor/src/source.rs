//! The platform seam: raw OS events, the [`CursorSource`] trait, and the shared pollers.
//!
//! A source only stamps and forwards (no disk, no allocation beyond the event): the OS callback
//! sends a [`RawEvent`] carrying the OS event time in host nanoseconds; conversion to ticks, the
//! coordinate mapping, rate limiting and file writing all happen on the recorder's writer thread.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use cm_core::{CmError, HostClock};

use crate::image::CursorImage;
use crate::pipeline::Msg;
use crate::schema::{Button, CursorStatus, Modifier, MoveSource};

/// An event as the OS reports it, before mapping to the recorded area.
#[derive(Debug, Clone, PartialEq)]
pub struct RawEvent {
    /// OS event time in host nanoseconds (the `HostClock` timebase), not receipt time.
    pub host_ns: u64,
    pub kind: RawKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RawKind {
    /// Pointer position in the OS's global space (see [`crate::CursorTarget`]).
    Move { gx: f64, gy: f64 },
    Button {
        gx: f64,
        gy: f64,
        button: Button,
        down: bool,
        mods: Vec<Modifier>,
    },
    /// Direction signs only.
    Scroll { dx: i8, dy: i8 },
    /// The cursor image changed.
    Image(Box<CursorImage>),
}

/// Channel end sources send into. Cheap to clone; usable from OS callbacks (never blocks).
#[derive(Debug, Clone)]
pub struct RawSender(mpsc::Sender<Msg>);

impl RawSender {
    pub(crate) fn channel() -> (RawSender, mpsc::Receiver<Msg>) {
        let (tx, rx) = mpsc::channel();
        (RawSender(tx), rx)
    }

    pub(crate) fn control(&self, msg: Msg) -> bool {
        self.0.send(msg).is_ok()
    }

    /// Queues an event; false once the recorder has stopped listening.
    pub fn send(&self, ev: RawEvent) -> bool {
        self.0.send(Msg::Raw(ev)).is_ok()
    }
}

/// A source of cursor events. `start` is called once; `stop` joins all threads the source made,
/// after which nothing more is sent.
pub trait CursorSource: Send {
    /// Starts delivering into `tx` and reports what could be recorded. Permission problems are a
    /// degraded `Ok(status)` (positions only), not an `Err`.
    fn start(&mut self, tx: RawSender) -> Result<CursorStatus, CmError>;
    fn stop(&mut self);
}

/// Last time an input event was seen (host ns); lets the image poller slow down when idle.
#[derive(Debug, Default)]
pub struct Activity(AtomicU64);

impl Activity {
    pub fn touch(&self, host_ns: u64) {
        self.0.store(host_ns, Ordering::Relaxed);
    }
    pub fn last(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }
}

/// Image poll period while the mouse is active (30 Hz) and idle (4 Hz). The image is stamped at
/// poll time, so a shape change can be up to one active period (33 ms) late.
pub const IMAGE_POLL_ACTIVE: Duration = Duration::from_millis(33);
pub const IMAGE_POLL_IDLE: Duration = Duration::from_millis(250);
/// Mouse counts as idle after this long without events.
pub const IDLE_AFTER_NS: u64 = 300_000_000;
/// Position polling rate of the permission fallback.
pub const POLL_HZ: u32 = 120;

/// Sleeps `d` in short slices so a stop request is honored within ~10 ms.
pub(crate) fn sleep_unless_stopped(stop: &AtomicBool, d: Duration) {
    let slice = Duration::from_millis(10);
    let mut left = d;
    while !left.is_zero() && !stop.load(Ordering::Relaxed) {
        let s = left.min(slice);
        thread::sleep(s);
        left -= s;
    }
}

/// Polls the pointer position at `hz` and sends a [`RawKind::Move`] on change, stamped with the
/// host clock at poll time. Used by the permission fallback.
pub fn spawn_position_poller(
    mut get: impl FnMut() -> Option<(f64, f64)> + Send + 'static,
    hz: u32,
    clock: Arc<dyn HostClock>,
    tx: RawSender,
    stop: Arc<AtomicBool>,
    activity: Arc<Activity>,
) -> JoinHandle<()> {
    let period = Duration::from_secs_f64(1.0 / f64::from(hz.max(1)));
    thread::spawn(move || {
        let mut last: Option<(f64, f64)> = None;
        while !stop.load(Ordering::Relaxed) {
            if let Some(p) = get() {
                if last != Some(p) {
                    last = Some(p);
                    let host_ns = clock.now_ns();
                    activity.touch(host_ns);
                    if !tx.send(RawEvent {
                        host_ns,
                        kind: RawKind::Move { gx: p.0, gy: p.1 },
                    }) {
                        break;
                    }
                }
            }
            sleep_unless_stopped(&stop, period);
        }
    })
}

/// Gives the current cursor image when it changed since the last call.
pub trait ImageProvider: Send {
    fn poll(&mut self) -> Option<CursorImage>;
}

/// Runs `provider` at 30 Hz while the mouse is active and 4 Hz when idle.
pub fn spawn_image_poller(
    mut provider: Box<dyn ImageProvider>,
    clock: Arc<dyn HostClock>,
    tx: RawSender,
    stop: Arc<AtomicBool>,
    activity: Arc<Activity>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        while !stop.load(Ordering::Relaxed) {
            let t0 = clock.now_ns();
            if let Some(img) = provider.poll() {
                if !tx.send(RawEvent {
                    host_ns: t0,
                    kind: RawKind::Image(Box::new(img)),
                }) {
                    break;
                }
            }
            let idle = clock.now_ns().saturating_sub(activity.last()) > IDLE_AFTER_NS;
            sleep_unless_stopped(
                &stop,
                if idle {
                    IMAGE_POLL_IDLE
                } else {
                    IMAGE_POLL_ACTIVE
                },
            );
        }
    })
}

/// A scripted source for CI: the test pushes [`RawEvent`]s through a [`FakeHandle`] and picks the
/// [`CursorStatus`] the source reports (use [`CursorStatus::permission_fallback`] to exercise the
/// denied-permission path).
#[derive(Debug)]
pub struct FakeCursorSource {
    status: CursorStatus,
    shared: Arc<Mutex<Option<RawSender>>>,
}

/// Test-side end of a [`FakeCursorSource`]; clone freely.
#[derive(Debug, Clone)]
pub struct FakeHandle {
    shared: Arc<Mutex<Option<RawSender>>>,
}

impl FakeCursorSource {
    pub fn new(status: CursorStatus) -> (Self, FakeHandle) {
        let shared = Arc::new(Mutex::new(None));
        (
            FakeCursorSource {
                status,
                shared: shared.clone(),
            },
            FakeHandle { shared },
        )
    }

    /// A fully working fake.
    pub fn full() -> (Self, FakeHandle) {
        Self::new(CursorStatus::full(MoveSource::Fake))
    }
}

impl CursorSource for FakeCursorSource {
    fn start(&mut self, tx: RawSender) -> Result<CursorStatus, CmError> {
        if let Ok(mut g) = self.shared.lock() {
            *g = Some(tx);
        }
        Ok(self.status.clone())
    }

    fn stop(&mut self) {
        if let Ok(mut g) = self.shared.lock() {
            *g = None;
        }
    }
}

impl FakeHandle {
    /// Sends one event; returns false if the source is not started (or already stopped).
    pub fn send(&self, host_ns: u64, kind: RawKind) -> bool {
        match self.shared.lock() {
            Ok(g) => g
                .as_ref()
                .is_some_and(|tx| tx.send(RawEvent { host_ns, kind })),
            Err(_) => false,
        }
    }

    pub fn mv(&self, host_ns: u64, gx: f64, gy: f64) -> bool {
        self.send(host_ns, RawKind::Move { gx, gy })
    }

    pub fn click(&self, host_ns: u64, gx: f64, gy: f64, button: Button, down: bool) -> bool {
        self.send(
            host_ns,
            RawKind::Button {
                gx,
                gy,
                button,
                down,
                mods: vec![],
            },
        )
    }

    pub fn image(&self, host_ns: u64, img: CursorImage) -> bool {
        self.send(host_ns, RawKind::Image(Box::new(img)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cm_core::ManualHostClock;

    #[test]
    fn position_poller_sends_only_changes() {
        let (tx, rx) = RawSender::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let clock = Arc::new(ManualHostClock::new(1_000));
        let positions = Arc::new(Mutex::new(vec![(1.0, 1.0), (1.0, 1.0), (2.0, 1.0)]));
        let p = positions.clone();
        let h = spawn_position_poller(
            move || {
                let mut g = p.lock().ok()?;
                if g.len() > 1 {
                    Some(g.remove(0))
                } else {
                    g.first().copied()
                }
            },
            1000,
            clock,
            tx,
            stop.clone(),
            Arc::new(Activity::default()),
        );
        thread::sleep(Duration::from_millis(150));
        stop.store(true, Ordering::Relaxed);
        let _ = h.join();
        let got: Vec<RawEvent> = rx
            .try_iter()
            .filter_map(|m| match m {
                Msg::Raw(e) => Some(e),
                _ => None,
            })
            .collect();
        assert_eq!(got.len(), 2, "{got:?}");
        assert_eq!(got[1].kind, RawKind::Move { gx: 2.0, gy: 1.0 });
    }
}
