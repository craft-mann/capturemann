//! Hiding the app's own pop-up windows around a capture, where the OS cannot exclude them.
//!
//! macOS hides a window from capture with `NSWindow.sharingType` and Windows has
//! `WDA_EXCLUDEFROMCAPTURE`; X11 and Wayland have nothing like it. On Linux the app therefore
//! takes the non-essential windows (today the toast stack) off the screen for the capture and
//! brings them back after it, with a short settle delay so the compositor has repainted before
//! the frame is taken. The library window is not touched: it is captured on purpose (REC-6.AC8).
//!
//! The capture runs on a daemon thread and the windows live on the main thread, so the two meet
//! in a small state machine:
//!
//! * the capture thread calls [`before_capture`] (the engine asks the kept-windows provider
//!   before every display capture and every target listing, `studio_window::kept_windows`): it
//!   takes out a lease of [`LEASE`] and waits until the main thread reports the windows hidden,
//!   then waits out [`SETTLE`];
//! * the main thread polls [`poll`] a few times a second: it hides while a lease is running
//!   ([`Step::Hide`], then [`mark_hidden`]) and restores when it ran out ([`Step::Restore`],
//!   then [`mark_restored`]);
//! * a lease that nobody renews runs out by itself, so a capture that fails or never ends
//!   cannot leave the windows hidden.
//!
//! Nothing happens until [`enable`] is called (Linux only, `platform_window::needs_capture_cloak`).

use std::sync::{Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// How long the windows stay hidden after the last request.
pub const LEASE: Duration = Duration::from_millis(1500);
/// How long the capture waits after the windows are gone, for the compositor to repaint.
pub const SETTLE: Duration = Duration::from_millis(180);
/// The longest the capture waits for the main thread to hide the windows.
pub const WAIT_HIDDEN: Duration = Duration::from_millis(800);

/// What the main thread should do now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Hide,
    Restore,
    Idle,
}

#[derive(Default)]
struct State {
    enabled: bool,
    lease_until: Option<Instant>,
    hidden_at: Option<Instant>,
}

impl State {
    fn poll(&mut self, now: Instant) -> Step {
        match (self.lease_until, self.hidden_at) {
            (Some(until), None) if now < until => Step::Hide,
            (Some(_), None) => {
                self.lease_until = None; // ran out before anything was hidden
                Step::Idle
            }
            (Some(until), Some(_)) if now >= until => Step::Restore,
            (None, Some(_)) => Step::Restore,
            _ => Step::Idle,
        }
    }
}

static STATE: Mutex<State> = Mutex::new(State {
    enabled: false,
    lease_until: None,
    hidden_at: None,
});
static HIDDEN: Condvar = Condvar::new();

fn lock() -> MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Turns the fallback on (the app calls this once at start where the OS has no exclusion).
pub fn enable() {
    lock().enabled = true;
}

/// Whether the windows are hidden right now (the toast code opens no window meanwhile).
pub fn is_hidden() -> bool {
    lock().hidden_at.is_some()
}

/// Called on the capture thread before a screenshot: asks for the windows to be hidden and
/// waits until they are (at most [`WAIT_HIDDEN`]), then for [`SETTLE`]. A no-op unless enabled.
pub fn before_capture() {
    let wait = {
        let mut st = lock();
        if !st.enabled {
            return;
        }
        let start = Instant::now();
        log::debug!("capture cloak: a capture asks for the pop-ups to be hidden");
        st.lease_until = Some(start + LEASE);
        while st.hidden_at.is_none() {
            let left = WAIT_HIDDEN.saturating_sub(start.elapsed());
            if left.is_zero() {
                break;
            }
            st = HIDDEN
                .wait_timeout(st, left)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
        st.hidden_at
            .map_or(Duration::ZERO, |at| settle_left(at, Instant::now()))
    };
    if !wait.is_zero() {
        std::thread::sleep(wait);
    }
}

/// How much of [`SETTLE`] is left when the windows were hidden at `hidden_at`.
pub fn settle_left(hidden_at: Instant, now: Instant) -> Duration {
    (hidden_at + settle()).saturating_duration_since(now)
}

/// [`SETTLE`], or `CAPTUREMANN_CLOAK_SETTLE_MS` when set (a slow compositor or software
/// renderer may need longer to repaint without the pop-ups).
pub fn settle() -> Duration {
    std::env::var("CAPTUREMANN_CLOAK_SETTLE_MS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .map_or(SETTLE, Duration::from_millis)
}

/// Main thread: what to do about the windows now.
pub fn poll(now: Instant) -> Step {
    lock().poll(now)
}

/// Main thread: the windows are off the screen.
pub fn mark_hidden(now: Instant) {
    lock().hidden_at = Some(now);
    HIDDEN.notify_all();
}

/// Main thread: the windows are back.
pub fn mark_restored() {
    let mut st = lock();
    st.hidden_at = None;
    st.lease_until = None;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(ms: u64, base: Instant) -> Instant {
        base + Duration::from_millis(ms)
    }

    #[test]
    fn a_lease_hides_then_restores_when_it_runs_out() {
        let base = Instant::now();
        let mut st = State {
            enabled: true,
            lease_until: Some(at(1500, base)),
            hidden_at: None,
        };
        assert_eq!(st.poll(at(10, base)), Step::Hide);
        st.hidden_at = Some(at(20, base));
        assert_eq!(st.poll(at(500, base)), Step::Idle, "hidden, lease running");
        // A second capture renews the lease.
        st.lease_until = Some(at(2400, base));
        assert_eq!(st.poll(at(1600, base)), Step::Idle);
        assert_eq!(st.poll(at(2400, base)), Step::Restore);
        st.hidden_at = None;
        st.lease_until = None;
        assert_eq!(st.poll(at(2500, base)), Step::Idle);
    }

    #[test]
    fn a_lease_that_ran_out_before_anything_was_hidden_is_dropped() {
        let base = Instant::now();
        let mut st = State {
            enabled: true,
            lease_until: Some(at(100, base)),
            hidden_at: None,
        };
        assert_eq!(st.poll(at(200, base)), Step::Idle);
        assert_eq!(st.lease_until, None);
    }

    #[test]
    fn nothing_to_do_without_a_lease() {
        let mut st = State::default();
        assert_eq!(st.poll(Instant::now()), Step::Idle);
    }

    #[test]
    fn the_capture_waits_out_the_rest_of_the_settle_time() {
        let base = Instant::now();
        assert_eq!(settle_left(base, at(0, base)), SETTLE);
        assert_eq!(
            settle_left(base, at(100, base)),
            SETTLE - Duration::from_millis(100)
        );
        assert_eq!(settle_left(base, at(500, base)), Duration::ZERO);
    }

    /// Disabled (macOS, Windows, tests): the capture thread never waits.
    #[test]
    fn before_capture_is_free_when_disabled() {
        let t = Instant::now();
        before_capture();
        assert!(t.elapsed() < Duration::from_millis(100));
    }
}
