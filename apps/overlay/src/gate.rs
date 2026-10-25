//! Holds pointer input until the window list has arrived (CAP-1.AC5).
//!
//! The overlay opens as soon as the frozen frames are there; the window list is loaded in
//! parallel and normally lands before the first mouse event. If a click comes first, it must not
//! be taken for "no window under the pointer" (a region click instead of a window pick): the
//! pointer events are kept in order and replayed once the windows are set, or once the wait
//! gives up (the list failed or is too slow) and the pickers carry no windows.

use std::time::{Duration, Instant};

/// How long pointer input waits for the window list before it is replayed without windows.
pub const WINDOWS_WAIT: Duration = Duration::from_millis(1500);

/// A pointer event for one view, in physical px of its window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Pointer {
    Moved { x: f64, y: f64 },
    Pressed,
    Released,
}

/// A pointer event and the view it belongs to.
pub type Held = (usize, Pointer);

#[derive(Debug)]
pub struct Gate {
    open: bool,
    held: Vec<Held>,
    deadline: Instant,
}

impl Gate {
    /// Closed until [`Gate::open`]; with `needs_windows == false` (display mode) it starts open.
    pub fn new(needs_windows: bool, now: Instant) -> Gate {
        Gate {
            open: !needs_windows,
            held: Vec::new(),
            deadline: now + WINDOWS_WAIT,
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// `Some(event)` when it can be applied now; `None` when it was held for later.
    pub fn submit(&mut self, view: usize, ev: Pointer) -> Option<Held> {
        if self.open {
            Some((view, ev))
        } else {
            self.held.push((view, ev));
            None
        }
    }

    /// Opens the gate and returns the held events, oldest first.
    pub fn open(&mut self) -> Vec<Held> {
        self.open = true;
        std::mem::take(&mut self.held)
    }

    /// When the wait gives up, if the gate is still closed.
    pub fn deadline(&self) -> Option<Instant> {
        (!self.open).then_some(self.deadline)
    }

    /// True when the gate is closed and the wait is over.
    pub fn expired(&self, now: Instant) -> bool {
        !self.open && now >= self.deadline
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::picker::{Effect, Mode, Outcome, Picker};
    use crate::targets::PickableWindow;
    use cm_core::{PhysicalPoint, PhysicalRect, PhysicalSize};

    fn picker() -> Picker {
        Picker::new(
            Mode::Region,
            PhysicalSize {
                width: 400,
                height: 300,
            },
            2.0,
            Vec::new(),
            false,
        )
    }

    fn apply(p: &mut Picker, (_, ev): Held) -> Effect {
        match ev {
            Pointer::Moved { x, y } => p.pointer_moved(x, y),
            Pointer::Pressed => p.press(),
            Pointer::Released => p.release(),
        }
    }

    /// CAP-1.AC5: a click before the window list arrives is held, and once the windows are set
    /// it picks the window under the pointer instead of acting as a click on bare screen.
    #[test]
    fn cap1_ac5_click_before_the_window_list_picks_the_window_after_it() {
        let now = Instant::now();
        let mut gate = Gate::new(true, now);
        let mut p = picker();
        for ev in [
            Pointer::Moved { x: 100.0, y: 80.0 },
            Pointer::Pressed,
            Pointer::Released,
        ] {
            assert_eq!(gate.submit(0, ev), None, "held while the list is loading");
        }
        assert!(!gate.is_open());
        // The list arrives: set it on the picker, then replay in order.
        p.set_windows(vec![PickableWindow {
            id: 7,
            title: "t".into(),
            app: "a".into(),
            rect: PhysicalRect {
                origin: PhysicalPoint { x: 50, y: 40 },
                size: PhysicalSize {
                    width: 200,
                    height: 150,
                },
            },
        }]);
        let mut last = Effect::Nothing;
        for held in gate.open() {
            last = apply(&mut p, held);
        }
        assert_eq!(last, Effect::Done(Outcome::Window { window_id: 7 }));
        // Open now: events pass straight through.
        assert_eq!(
            gate.submit(1, Pointer::Pressed),
            Some((1, Pointer::Pressed))
        );
    }

    /// CAP-1.AC5: if the list never comes, the wait ends and the same click is a plain region
    /// click (no window under it), so the picker is never stuck.
    #[test]
    fn cap1_ac5_a_missing_window_list_stops_blocking_input() {
        let now = Instant::now();
        let mut gate = Gate::new(true, now);
        assert_eq!(gate.deadline(), Some(now + WINDOWS_WAIT));
        assert!(!gate.expired(now + WINDOWS_WAIT - Duration::from_millis(1)));
        assert!(gate.expired(now + WINDOWS_WAIT));
        gate.submit(0, Pointer::Pressed);
        assert_eq!(gate.open().len(), 1);
        assert_eq!(gate.deadline(), None);
        assert!(!gate.expired(now + WINDOWS_WAIT * 2));
    }

    /// Display mode never looks at windows, so it never waits for them.
    #[test]
    fn display_mode_is_never_gated() {
        let mut gate = Gate::new(false, Instant::now());
        assert!(gate.is_open());
        assert!(gate.submit(0, Pointer::Pressed).is_some());
    }
}
