//! Long Shot (CAP-6): a scrolling capture of a window or an area as one tall image.
//!
//! * [`stitch`]: the headless stitching engine (golden-tested on synthetic frame sequences).
//! * [`FrameSource`], [`Scroller`], [`Sleeper`]: what touches the screen, behind traits so the
//!   driver in [`run`] is tested with the scripted [`fake`] page and no display.
//! * [`run::run_auto`] (CaptureMann scrolls, about 80 % of the area per step, waiting until two
//!   frames 50 ms apart are the same, 150 ms at most) and [`run::run_hand`] (the user scrolls;
//!   a frame is taken whenever the content settles).
//! * [`platform_scroller`]: `CGEventPost` on macOS (needs Accessibility), `SendInput` on Windows,
//!   XTest on X11; Wayland cannot inject scrolling at all.

pub mod debug;
pub mod fake;
pub use cm_core::long_shot_hud as hud;
pub mod run;

pub mod stitch;

use std::time::Duration;

use cm_core::CmError;

pub use crate::sys::long_scroll::{
    accessibility_trusted, auto_scroll_support, platform_scroller, platform_scroller_at,
    raise_window, request_accessibility, AutoScrollSupport,
};
pub use run::{
    run_auto, run_auto_ex, run_hand, run_hand_ex, LongShotConfig, LongShotProgress, LongShotResult,
};
pub use stitch::{
    limit_kind, Direction, EndReason, LimitKind, Limits, PushOutcome, Stitched, Stitcher,
    MAX_LONG_SIDE, MAX_PIXELS,
};

use crate::Frame;

/// Takes frames of the scrollable area (physical px, the same size every time).
pub trait FrameSource {
    fn grab(&mut self) -> Result<Frame, CmError>;

    /// Whether the window the frames come from is no longer where, or as big as, it was when the
    /// run began (asked only when a join failed, so it may cost a listing of the windows).
    /// `false` when the source cannot tell.
    fn target_changed(&mut self) -> bool {
        false
    }
}

/// Scrolls the content under the area. `amount_px` is physical px along `direction`; the
/// content moves forward (down or to the right) by about that much.
pub trait Scroller {
    fn scroll(&mut self, direction: Direction, amount_px: u32) -> Result<(), CmError>;

    /// Moves to another point of the area for the following scrolls (the first scroll moved
    /// nothing: the wheel may have gone to another window). `false` when there is none.
    fn next_point(&mut self) -> bool {
        false
    }

    /// Scrolls the content back (up or left) by about `amount_px`; `Ok(false)` when this
    /// scroller cannot (nothing was done). Used once after a join failed: the content may have
    /// moved past the overlap, and half a step back gives the stitcher another try.
    fn scroll_back(&mut self, _direction: Direction, _amount_px: u32) -> Result<bool, CmError> {
        Ok(false)
    }
}

/// Waiting, so tests run without real time.
pub trait Sleeper {
    fn sleep(&mut self, d: Duration);
}

/// Sleeps the thread.
#[derive(Debug, Default, Clone, Copy)]
pub struct ThreadSleeper;

impl Sleeper for ThreadSleeper {
    fn sleep(&mut self, d: Duration) {
        std::thread::sleep(d);
    }
}

/// Settle detection: two frames `interval` apart are the same, within `limit`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settle {
    pub interval: Duration,
    pub limit: Duration,
}

impl Default for Settle {
    fn default() -> Self {
        Settle {
            interval: Duration::from_millis(50),
            limit: Duration::from_millis(150),
        }
    }
}

/// The frame after the content settled; `settled` is false when the limit passed first (an
/// animation): the last frame is returned anyway and the stitcher decides.
#[derive(Debug, Clone)]
pub struct Settled {
    pub frame: Frame,
    pub settled: bool,
}

/// Grabs a frame, waits, grabs again, and repeats until two consecutive frames are equal or
/// `settle.limit` has passed.
pub fn wait_settled(
    src: &mut dyn FrameSource,
    sleeper: &mut dyn Sleeper,
    settle: Settle,
) -> Result<Settled, CmError> {
    let mut prev = src.grab()?;
    let mut waited = Duration::ZERO;
    loop {
        sleeper.sleep(settle.interval);
        waited += settle.interval;
        let next = src.grab()?;
        if next == prev {
            return Ok(Settled {
                frame: next,
                settled: true,
            });
        }
        prev = next;
        if waited >= settle.limit {
            return Ok(Settled {
                frame: prev,
                settled: false,
            });
        }
    }
}

#[cfg(test)]
mod tests;
