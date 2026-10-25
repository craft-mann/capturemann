//! Whether a window drag is in progress (the HUD and the webcam bubble start a native window move
//! on mouse-down). A plain click never moves the window, so no bounds change ever follows it and
//! "dragging" must not stay set forever (R4-8): it would freeze the bubble's hover and click-through
//! logic. A drag is live while the primary button is held, or for [`GRACE`] after the last press or
//! move (the native move loop swallows the mouse-up, so the button state is asked of the OS).

use std::time::{Duration, Instant};

/// How long after the last press or move a drag is still considered live once the button is up.
pub const GRACE: Duration = Duration::from_millis(300);

#[derive(Debug, Clone, Copy, Default)]
pub struct DragState {
    since: Option<Instant>,
}

impl DragState {
    /// Mouse-down on the grip: a drag may start.
    pub fn press(&mut self, now: Instant) {
        self.since = Some(now);
    }

    /// The window moved while dragging.
    pub fn moved(&mut self, now: Instant) {
        if self.since.is_some() {
            self.since = Some(now);
        }
    }

    /// Mouse-up or the drag settled.
    pub fn clear(&mut self) {
        self.since = None;
    }

    /// Whether a drag is live at `now`. `button_down`: the primary button is held. A drag whose
    /// button is up and whose last activity is older than [`GRACE`] was a plain click: it is
    /// forgotten here.
    pub fn is_live(&mut self, now: Instant, button_down: bool) -> bool {
        let Some(since) = self.since else {
            return false;
        };
        if button_down || now.saturating_duration_since(since) < GRACE {
            return true;
        }
        self.since = None;
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    /// R4-8: a click without a drag clears itself, so hover and click-through resume.
    #[test]
    fn r4_8_a_plain_click_does_not_leave_the_drag_flag_set() {
        let t = Instant::now();
        let mut d = DragState::default();
        assert!(!d.is_live(t, false));
        d.press(t);
        assert!(d.is_live(t + ms(50), false), "just pressed");
        assert!(!d.is_live(t + ms(400), false), "a click, no bounds change");
        assert!(!d.is_live(t + ms(401), true), "and it stays cleared");
    }

    /// A real drag stays live while the button is held, however still the pointer is, and for the
    /// grace period after the last move.
    #[test]
    fn a_held_button_keeps_the_drag_live() {
        let t = Instant::now();
        let mut d = DragState::default();
        d.press(t);
        assert!(d.is_live(t + ms(5_000), true));
        d.moved(t + ms(5_000));
        assert!(d.is_live(t + ms(5_200), false));
        assert!(!d.is_live(t + ms(5_400), false));
        // Moves without a press are not drags.
        d.moved(t + ms(6_000));
        assert!(!d.is_live(t + ms(6_001), false));
        d.press(t);
        d.clear();
        assert!(!d.is_live(t, true));
    }
}
