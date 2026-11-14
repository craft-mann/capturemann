//! The blinking caret of text on the canvas (ANN-1.AC12), as pure functions of the time since
//! the last key, press or input-method event.

use std::time::Duration;

/// The caret stays solid this long after the last input.
pub const SOLID_MS: u64 = 500;
/// After that it toggles every 530 ms (the system's usual blink rate).
pub const BLINK_MS: u64 = 530;

/// Whether the caret is drawn `idle_ms` after the last input. `window_active` is false while
/// another window has the focus: there is no caret then (the selection turns grey instead).
pub fn visible(idle_ms: u64, window_active: bool) -> bool {
    if !window_active {
        return false;
    }
    if idle_ms < SOLID_MS {
        return true;
    }
    // The first toggle (at 500 ms) hides it, the next shows it again, and so on.
    ((idle_ms - SOLID_MS) / BLINK_MS) % 2 == 1
}

/// How long to wait before the caret may change again, so the view repaints only when needed.
pub fn next_change(idle_ms: u64) -> Duration {
    let ms = if idle_ms < SOLID_MS {
        SOLID_MS - idle_ms
    } else {
        BLINK_MS - (idle_ms - SOLID_MS) % BLINK_MS
    };
    Duration::from_millis(ms.max(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ANN-1.AC12: solid for 500 ms after a key, then it toggles every 530 ms; with the window
    /// inactive there is no caret.
    #[test]
    fn the_caret_is_solid_then_blinks_every_530_ms() {
        assert!(visible(0, true));
        assert!(visible(499, true));
        assert!(!visible(500, true), "first toggle: hidden");
        assert!(!visible(1029, true));
        assert!(visible(1030, true), "second toggle: shown");
        assert!(visible(1559, true));
        assert!(!visible(1560, true));
        for t in [0, 400, 600, 5000] {
            assert!(!visible(t, false), "no caret in an inactive window");
        }
    }

    #[test]
    fn the_next_repaint_is_at_the_next_change() {
        assert_eq!(next_change(100), Duration::from_millis(400));
        assert_eq!(next_change(500), Duration::from_millis(530));
        assert_eq!(next_change(600), Duration::from_millis(430));
        assert_eq!(next_change(1030), Duration::from_millis(530));
    }
}
