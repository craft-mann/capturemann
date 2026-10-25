//! Delayed capture (CAP-5): the delay choices, the countdown pill's geometry and timing, and what
//! a key does while it is on screen. Display-free; the window is `ui::delay_pill`.

use std::time::Duration;

use cm_i18n::{tr, trn};

use crate::recording::geometry::Rect;

/// Pill diameter (CAP-5: 48 px, DESIGN 5.3 styling).
pub const PILL: f64 = 48.0;
/// Ring stroke (the 160 px countdown uses 4).
pub const RING: f64 = 2.0;
/// Transparent room around the pill in its window.
pub const PAD: f64 = 4.0;
/// Distance from the display corner.
pub const MARGIN: f64 = 16.0;
/// Digit size.
pub const DIGIT_SIZE: f32 = 20.0;

/// The delays the tray offers (CAP-5: 3, 5 or 10 s), in menu order.
pub const CHOICES: [u32; 4] = [0, 3, 5, 10];

/// Setting key of the remembered delay (`capture.delay_s`, agent-writable).
pub const KEY: &str = "capture.delay_s";

/// The delay a stored setting value means: one of [`CHOICES`], else 0 (no delay). A hand-edited
/// or out-of-range value never makes a capture wait.
pub fn from_setting(value: Option<&serde_json::Value>) -> u32 {
    value
        .and_then(serde_json::Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
        .filter(|n| CHOICES.contains(n))
        .unwrap_or(0)
}

/// Short label of a choice on the capture toolbar (the tray uses [`choice_label`]).
pub fn toolbar_label(secs: u32) -> String {
    if secs == 0 {
        tr!("No delay")
    } else {
        format!("{secs} s") // i18n-ignore: number and unit symbol
    }
}

/// Tray menu id of a delay choice.
pub fn menu_id(secs: u32) -> String {
    format!("delay:{secs}")
}

/// The delay a tray menu id selects, `None` for any other id or a delay the menu does not offer.
pub fn from_menu_id(id: &str) -> Option<u32> {
    let secs: u32 = id.strip_prefix("delay:")?.parse().ok()?;
    CHOICES.contains(&secs).then_some(secs)
}

/// Tray label of a delay choice.
pub fn choice_label(secs: u32) -> String {
    if secs == 0 {
        tr!("No delay")
    } else {
        trn!("{n} second", "{n} seconds", secs)
    }
}

/// The pill's window: top right of the visible part of `display`, clear of the menu bar. Not
/// near the middle, so it stays out of a region or window the user is about to capture.
pub fn window_rect(visible: &Rect) -> Rect {
    let side = PILL + 2.0 * PAD;
    Rect::new(
        visible.right() - MARGIN - side,
        visible.y + MARGIN,
        side,
        side,
    )
}

/// The whole second the pill shows `elapsed` into a `total` delay: 5, 4, 3 ... 1, never 0 while
/// the delay runs. Past the end it stays at 1.
pub fn digit(elapsed: Duration, total: Duration) -> u32 {
    let left = total.saturating_sub(elapsed);
    let whole = left.as_secs() + u64::from(left.subsec_nanos() > 0);
    u32::try_from(whole.max(1)).unwrap_or(u32::MAX)
}

/// How much of the ring is left: 1 at the start draining to 0 at the end of the whole delay.
/// With `animate` off (reduced motion) the ring stays full and only the digit changes.
pub fn ring_fraction(elapsed: Duration, total: Duration, animate: bool) -> f64 {
    if !animate || total.is_zero() {
        return 1.0;
    }
    (1.0 - elapsed.as_secs_f64() / total.as_secs_f64()).clamp(0.0, 1.0)
}

/// Screen-reader text for the pill.
pub fn label(remaining: u32) -> String {
    tr!("Capturing in {remaining}", remaining = remaining)
}

/// `request_id` of the n-th delayed capture of this run: how Esc finds the daemon's waiting call
/// (`Daemon::cancel_delayed_capture`).
pub fn request_id(n: u64) -> String {
    format!("ui-delay-{n}")
}

/// What a key does while the pill has the keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAction {
    Cancel,
    None,
}

/// CAP-5.AC2: Esc cancels the delay.
pub fn key_action(key: &str) -> KeyAction {
    if key == "escape" {
        KeyAction::Cancel
    } else {
        KeyAction::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: fn(u64) -> Duration = Duration::from_secs;

    /// CAP-5: the tray offers 3, 5 and 10 s (and off); other ids are not delays.
    #[test]
    fn menu_ids_round_trip_and_reject_others() {
        for s in CHOICES {
            assert_eq!(from_menu_id(&menu_id(s)), Some(s));
        }
        assert_eq!(from_menu_id("delay:7"), None);
        assert_eq!(from_menu_id("delay:x"), None);
        assert_eq!(from_menu_id("capture_region"), None);
        assert_eq!(choice_label(0), "No delay");
        assert_eq!(choice_label(5), "5 seconds");
    }

    /// CAP-5: the remembered delay is one of the offered choices, else none.
    #[test]
    fn stored_delay_is_normalised() {
        use serde_json::json;
        assert_eq!(from_setting(None), 0);
        assert_eq!(from_setting(Some(&json!(5))), 5);
        assert_eq!(from_setting(Some(&json!(10))), 10);
        assert_eq!(from_setting(Some(&json!(7))), 0);
        assert_eq!(from_setting(Some(&json!(-3))), 0);
        assert_eq!(from_setting(Some(&json!("5"))), 0);
        assert_eq!(from_setting(Some(&json!(5_000_000_000_u64))), 0);
        assert_eq!(toolbar_label(0), "No delay");
        assert_eq!(toolbar_label(3), "3 s");
    }

    /// CAP-5: a 48 px pill in a screen corner, kept on the visible part of the display.
    #[test]
    fn pill_sits_in_the_top_right_corner() {
        let visible = Rect::new(0.0, 25.0, 1440.0, 800.0);
        let w = window_rect(&visible);
        assert_eq!(w.w, PILL + 2.0 * PAD);
        assert_eq!(w.right(), 1440.0 - MARGIN);
        assert_eq!(w.y, 25.0 + MARGIN);
        // A second display to the right keeps its own corner.
        let second = Rect::new(1440.0, 0.0, 1920.0, 1080.0);
        assert_eq!(window_rect(&second).right(), 3360.0 - MARGIN);
    }

    /// CAP-5.AC1: the digit counts 5 4 3 2 1 over the delay and never shows 0.
    #[test]
    fn digit_counts_down_and_stops_at_one() {
        let total = S(5);
        assert_eq!(digit(Duration::ZERO, total), 5);
        assert_eq!(digit(Duration::from_millis(999), total), 5);
        assert_eq!(digit(S(1), total), 4);
        assert_eq!(digit(Duration::from_millis(4500), total), 1);
        assert_eq!(digit(S(5), total), 1);
        assert_eq!(digit(S(9), total), 1);
    }

    #[test]
    fn ring_drains_over_the_whole_delay() {
        let total = S(10);
        assert_eq!(ring_fraction(Duration::ZERO, total, true), 1.0);
        assert!((ring_fraction(S(5), total, true) - 0.5).abs() < 1e-9);
        assert_eq!(ring_fraction(S(12), total, true), 0.0);
        assert_eq!(ring_fraction(S(5), total, false), 1.0);
        assert_eq!(ring_fraction(S(1), Duration::ZERO, true), 1.0);
    }

    /// CAP-5.AC2: Esc cancels; nothing else does.
    #[test]
    fn cap5_ac2_escape_cancels() {
        assert_eq!(key_action("escape"), KeyAction::Cancel);
        assert_eq!(key_action("space"), KeyAction::None);
    }

    #[test]
    fn labels_and_request_ids() {
        assert_eq!(label(3), "Capturing in 3");
        assert_eq!(request_id(2), "ui-delay-2");
    }
}
