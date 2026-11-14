//! The Long Shot driver: Auto scroll and Hand scroll loops over the traits.

use cm_core::CmError;

use super::stitch::{Direction, EndReason, Limits, PushOutcome, Stitched, Stitcher};
use super::{wait_settled, FrameSource, Scroller, Settle, Sleeper};
use crate::Frame;

/// Settings of one Long Shot.
#[derive(Debug, Clone, Copy)]
pub struct LongShotConfig {
    pub direction: Direction,
    pub limits: Limits,
    /// Share of the scrollable content scrolled each time (CAP-6: about 80 %).
    pub step_fraction: f64,
    /// The first scroll happens before the fixed header and footer are known, so it is shorter
    /// (it must stay inside the content area).
    pub first_step_fraction: f64,
    pub settle: Settle,
}

impl Default for LongShotConfig {
    fn default() -> Self {
        LongShotConfig {
            direction: Direction::Vertical,
            limits: Limits::default(),
            step_fraction: 0.8,
            first_step_fraction: 0.5,
            settle: Settle::default(),
        }
    }
}

/// What the HUD and side preview show after each joined frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LongShotProgress {
    pub frames: u32,
    pub len_px: u32,
    pub max_len: u32,
    pub header_px: u32,
    pub footer_px: u32,
}

/// The end of a Long Shot: the image and why it ended.
#[derive(Debug, Clone)]
pub struct LongShotResult {
    pub image: Stitched,
    pub end: EndReason,
}

fn progress(s: &Stitcher) -> LongShotProgress {
    let (header_px, footer_px) = s.bands();
    LongShotProgress {
        frames: s.frames(),
        len_px: s.len_px(),
        max_len: s.max_len(),
        header_px,
        footer_px,
    }
}

fn extent(f: &Frame, d: Direction) -> u32 {
    match d {
        Direction::Vertical => f.height(),
        Direction::Horizontal => f.width(),
    }
}

/// Auto scroll: frame, scroll about 80 % of the area, wait for the content to settle, frame,
/// until the end, the limit, a failed join or `stop` returns true (Esc or Stop; what was
/// captured is kept).
pub fn run_auto(
    src: &mut dyn FrameSource,
    scroller: &mut dyn Scroller,
    sleeper: &mut dyn Sleeper,
    cfg: LongShotConfig,
    stop: &mut dyn FnMut() -> bool,
    on_progress: &mut dyn FnMut(LongShotProgress),
) -> Result<LongShotResult, CmError> {
    let mut stitcher = Stitcher::new(cfg.direction, cfg.limits);
    let first = wait_settled(src, sleeper, cfg.settle)?;
    stitcher.push(&first.frame, None);
    on_progress(progress(&stitcher));
    let size = f64::from(extent(&first.frame, cfg.direction));
    let mut step = ((size * cfg.first_step_fraction) as u32).max(1);
    let end = loop {
        if stop() {
            break EndReason::StoppedByUser;
        }
        scroller.scroll(cfg.direction, step)?;
        let next = wait_settled(src, sleeper, cfg.settle)?;
        match stitcher.push(&next.frame, Some(step)) {
            PushOutcome::Added { .. } => {
                on_progress(progress(&stitcher));
                // 80 % of what actually scrolls: the area without the fixed bands.
                let (h, f) = stitcher.bands();
                let content = (size - f64::from(h) - f64::from(f)).max(1.0);
                step = ((content * cfg.step_fraction) as u32).max(1);
            }
            PushOutcome::NoChange => break EndReason::ReachedEnd,
            PushOutcome::NoOverlap => break EndReason::NoOverlap,
            PushOutcome::LimitReached { .. } => {
                on_progress(progress(&stitcher));
                break EndReason::LimitReached;
            }
        }
    };
    Ok(LongShotResult {
        image: stitcher.finish()?,
        end,
    })
}

/// Hand scroll: the user scrolls. A frame is taken whenever the content has stopped moving
/// (two polls `settle.interval` apart are equal) and differs from the last frame taken.
/// Ends when `stop` returns true, at the limit, or when a frame does not line up.
pub fn run_hand(
    src: &mut dyn FrameSource,
    sleeper: &mut dyn Sleeper,
    cfg: LongShotConfig,
    stop: &mut dyn FnMut() -> bool,
    on_progress: &mut dyn FnMut(LongShotProgress),
) -> Result<LongShotResult, CmError> {
    let mut stitcher = Stitcher::new(cfg.direction, cfg.limits);
    let first = wait_settled(src, sleeper, cfg.settle)?;
    stitcher.push(&first.frame, None);
    on_progress(progress(&stitcher));
    let mut last_taken = first.frame;
    let mut prev_poll = last_taken.clone();
    let end = loop {
        if stop() {
            break EndReason::StoppedByUser;
        }
        sleeper.sleep(cfg.settle.interval);
        let g = src.grab()?;
        let settled = g == prev_poll;
        prev_poll = g.clone();
        if !settled || g == last_taken {
            continue;
        }
        match stitcher.push(&g, None) {
            PushOutcome::Added { .. } => {
                on_progress(progress(&stitcher));
                last_taken = g;
            }
            PushOutcome::NoChange => {}
            PushOutcome::NoOverlap => break EndReason::NoOverlap,
            PushOutcome::LimitReached { .. } => {
                on_progress(progress(&stitcher));
                break EndReason::LimitReached;
            }
        }
    };
    Ok(LongShotResult {
        image: stitcher.finish()?,
        end,
    })
}
