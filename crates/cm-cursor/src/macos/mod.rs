//! macOS backend: listen-only `CGEventTap` (needs Input Monitoring) for moves, buttons and
//! scroll stamped with the OS event timestamp, plus an `NSCursor` image poller. Without Input
//! Monitoring: 120 Hz position polling, clicks marked unavailable (REC-7.AC4).

mod image;
pub(crate) mod sys;

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use cm_core::{CmError, HostClock, Permission, SystemHostClock};

use crate::recorder::CursorConfig;
use crate::schema::{Button, CursorStatus, Modifier, MoveSource};
use crate::source::{
    spawn_image_poller, spawn_position_poller, Activity, CursorSource, RawEvent, RawKind,
    RawSender, POLL_HZ,
};
use crate::target::CursorTarget;

pub const CLOCK_NAME: &str = "CLOCK_UPTIME_RAW ns (mach_absolute_time)";
pub const PLATFORM: &str = "macos";

const MSG_DENIED: &str = "Input Monitoring is not granted: recording cursor positions only; \
clicks and scroll are unavailable (System Settings > Privacy & Security > Input Monitoring)";
const MSG_TAP_FAILED: &str =
    "the event tap could not be created: recording cursor positions only; clicks and scroll are unavailable";

/// Predicted status without starting a recording (for the HUD / onboarding).
pub fn preflight() -> CursorStatus {
    if sys::listen_access() {
        CursorStatus::full(MoveSource::EventTap)
    } else {
        CursorStatus::permission_fallback(Permission::InputMonitoring, MSG_DENIED)
    }
}

/// The target for a whole display (`None` = main display): global bounds in points and the
/// backing scale from its current mode.
pub fn main_display_target(display_id: Option<u32>) -> CursorTarget {
    let id = display_id.unwrap_or_else(sys::main_display_id);
    let b = sys::display_bounds(id);
    CursorTarget::display(
        id,
        cm_core::Rect::new(b.origin.x, b.origin.y, b.size.w, b.size.h),
        sys::display_scale(id),
    )
}

pub fn default_source(config: &CursorConfig) -> Box<dyn CursorSource> {
    Box::new(MacSource::new(config))
}

/// CGEventGetTimestamp is documented as ns but arrives as mach ticks on some systems (Apple
/// silicon, timebase 125/3): pick the interpretation closer to `now`.
pub(crate) fn event_ns(ts: u64, now_ticks: u64, tb: (u64, u64)) -> u64 {
    let to_ns = |t: u64| (u128::from(t) * u128::from(tb.0) / u128::from(tb.1.max(1))) as u64;
    let now_ns = to_ns(now_ticks);
    let as_ticks = to_ns(ts);
    if ts.abs_diff(now_ns) <= as_ticks.abs_diff(now_ns) {
        ts
    } else {
        as_ticks
    }
}

fn mods_from_flags(f: u64) -> Vec<Modifier> {
    let mut m = Vec::new();
    if f & 0x0002_0000 != 0 {
        m.push(Modifier::Shift);
    }
    if f & 0x0004_0000 != 0 {
        m.push(Modifier::Ctrl);
    }
    if f & 0x0008_0000 != 0 {
        m.push(Modifier::Alt);
    }
    if f & 0x0010_0000 != 0 {
        m.push(Modifier::Cmd);
    }
    m
}

fn other_button(n: i64) -> Button {
    match n {
        2 => Button::Middle,
        3 => Button::Back,
        4 => Button::Forward,
        _ => Button::Other,
    }
}

struct TapCtx {
    tx: RawSender,
    tap: AtomicUsize,
    activity: Arc<Activity>,
    tb: (u64, u64),
}

extern "C" fn tap_cb(_proxy: sys::Ptr, ty: u32, ev: sys::Ptr, user: sys::Ptr) -> sys::Ptr {
    if user.is_null() || ev.is_null() {
        return ev;
    }
    // SAFETY: `user` is the `TapCtx` owned by the tap thread, which destroys the tap (so this
    // callback can no longer run) before dropping it.
    let ctx = unsafe { &*(user as *const TapCtx) };
    if ty == sys::DISABLED_BY_TIMEOUT || ty == sys::DISABLED_BY_USER {
        sys::tap_enable(ctx.tap.load(Ordering::Relaxed));
        return ev;
    }
    // SAFETY: `ev` is live for the duration of this callback and the view does not escape it.
    let v = unsafe { sys::EventView::new(ev) };
    let host_ns = event_ns(v.timestamp(), sys::mach_now(), ctx.tb);
    let (gx, gy) = v.location();
    let kind = match ty {
        1 | 2 | 3 | 4 | 25 | 26 => {
            let down = matches!(ty, 1 | 3 | 25);
            let button = match ty {
                1 | 2 => Button::Left,
                3 | 4 => Button::Right,
                _ => other_button(v.field(sys::FIELD_BUTTON_NUMBER)),
            };
            RawKind::Button {
                gx,
                gy,
                button,
                down,
                mods: mods_from_flags(v.flags()),
            }
        }
        sys::MOVED | 6 | 7 | 27 => RawKind::Move { gx, gy },
        sys::SCROLL => RawKind::Scroll {
            dx: v.field(sys::FIELD_SCROLL_DX).signum() as i8,
            dy: v.field(sys::FIELD_SCROLL_DY).signum() as i8,
        },
        _ => return ev,
    };
    ctx.activity.touch(host_ns);
    ctx.tx.send(RawEvent { host_ns, kind });
    ev
}

/// The tap thread: creates the tap on its own run loop and pumps it until told to stop.
fn tap_thread(
    tx: RawSender,
    stop: Arc<AtomicBool>,
    activity: Arc<Activity>,
    ready: mpsc::Sender<bool>,
) {
    let ctx = Box::new(TapCtx {
        tx,
        tap: AtomicUsize::new(0),
        activity,
        tb: sys::timebase(),
    });
    let user = (&*ctx as *const TapCtx)
        .cast_mut()
        .cast::<std::ffi::c_void>();
    // SAFETY: `ctx` outlives the tap: it is dropped at the end of this function, after
    // `destroy`; `tap_cb` interprets `user` as `TapCtx` only.
    let Some(tap) = (unsafe { sys::EventTap::create(tap_cb, user) }) else {
        let _ = ready.send(false);
        return;
    };
    ctx.tap.store(tap.raw(), Ordering::Relaxed);
    let _ = ready.send(true);
    while !stop.load(Ordering::Relaxed) {
        sys::EventTap::run_for(0.1);
    }
    tap.destroy();
    drop(ctx);
}

/// The real macOS source.
pub struct MacSource {
    request_permission: bool,
    display_scale: f64,
    access: Box<dyn Fn() -> bool + Send>,
    stop: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
}

impl MacSource {
    pub fn new(config: &CursorConfig) -> Self {
        Self::with_access_probe(config, Box::new(sys::listen_access))
    }

    /// Injects the Input Monitoring check (tests use `|| false` to take the fallback path).
    pub fn with_access_probe(config: &CursorConfig, access: Box<dyn Fn() -> bool + Send>) -> Self {
        MacSource {
            request_permission: config.request_permission,
            display_scale: config.target.scale,
            access,
            stop: Arc::new(AtomicBool::new(false)),
            threads: Vec::new(),
        }
    }
}

impl CursorSource for MacSource {
    fn start(&mut self, tx: RawSender) -> Result<CursorStatus, CmError> {
        let clock: Arc<dyn HostClock> = Arc::new(SystemHostClock);
        self.stop = Arc::new(AtomicBool::new(false));
        let activity = Arc::new(Activity::default());

        let granted = (self.access)();
        if !granted && self.request_permission {
            sys::request_listen_access();
        }
        let mut status = None;
        if granted {
            let (rtx, rrx) = mpsc::channel();
            let (t, s, a) = (tx.clone(), self.stop.clone(), activity.clone());
            self.threads.push(
                thread::Builder::new()
                    .name("cm-cursor-tap".into())
                    .spawn(move || tap_thread(t, s, a, rtx))
                    .map_err(|e| CmError::internal(format!("tap thread: {e}")))?,
            );
            if rrx.recv_timeout(Duration::from_secs(2)).unwrap_or(false) {
                status = Some(CursorStatus::full(MoveSource::EventTap));
            }
        }
        let status = match status {
            Some(s) => s,
            None => {
                let msg = if granted { MSG_TAP_FAILED } else { MSG_DENIED };
                self.threads.push(spawn_position_poller(
                    sys::pointer_location,
                    POLL_HZ,
                    clock.clone(),
                    tx.clone(),
                    self.stop.clone(),
                    activity.clone(),
                ));
                CursorStatus::permission_fallback(Permission::InputMonitoring, msg)
            }
        };
        // Anchor the starting position (the tap only reports changes).
        if let Some((gx, gy)) = sys::pointer_location() {
            let host_ns = clock.now_ns();
            activity.touch(host_ns);
            tx.send(RawEvent {
                host_ns,
                kind: RawKind::Move { gx, gy },
            });
        }
        self.threads.push(spawn_image_poller(
            Box::new(image::MacImageProvider::new(self.display_scale)),
            clock,
            tx,
            self.stop.clone(),
            activity,
        ));
        Ok(status)
    }

    fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        for h in self.threads.drain(..) {
            let _ = h.join();
        }
    }
}

impl Drop for MacSource {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_timestamp_unit_is_detected() {
        let tb = (125, 3); // Apple silicon: 24 MHz
        let now_ticks = 24_000_000_000u64; // 1000 s uptime
        let now_ns = 1_000_000_000_000u64;
        // An event 5 ms ago, expressed as ticks or as ns, converts to the same ns.
        let ev_ns = now_ns - 5_000_000;
        let ev_ticks = ev_ns * 3 / 125;
        let a = event_ns(ev_ns, now_ticks, tb);
        let b = event_ns(ev_ticks, now_ticks, tb);
        assert!(
            a.abs_diff(ev_ns) < 100 && b.abs_diff(ev_ns) < 100,
            "{a} {b}"
        );
        // Intel (1/1): identical either way.
        assert_eq!(event_ns(777, 800, (1, 1)), 777);
    }

    #[test]
    fn modifiers_are_flags_only() {
        assert_eq!(mods_from_flags(0), vec![]);
        assert_eq!(
            mods_from_flags(0x0002_0000 | 0x0010_0000),
            vec![Modifier::Shift, Modifier::Cmd]
        );
        // Caps lock / fn / unrelated bits are ignored.
        assert_eq!(mods_from_flags(0x0001_0000 | 0x0080_0000), vec![]);
    }

    #[test]
    fn mask_has_no_key_events() {
        let m = sys::event_mask();
        for key_ty in [10u32, 11, 12] {
            // keyDown, keyUp, flagsChanged
            assert_eq!(m & (1 << key_ty), 0);
        }
        assert_ne!(m & (1 << sys::MOVED), 0);
    }
}

#[cfg(test)]
mod hardware_tests;
