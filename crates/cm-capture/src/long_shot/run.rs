//! The Long Shot driver: Auto scroll and Hand scroll loops over the traits.

use cm_core::CmError;

use super::stitch::{Direction, EndReason, Limits, PushOutcome, Stitched, Stitcher};
use super::{wait_settled, FrameSource, Scroller, Settle, Sleeper};
use crate::Frame;

/// How often, and after what wait, an unchanged frame after a scroll is looked at again.
const LATE_SCROLL_POLLS: u32 = 3;
const LATE_SCROLL_WAIT: std::time::Duration = std::time::Duration::from_millis(150);

/// Settings of one Long Shot.
#[derive(Debug, Clone, Copy)]
pub struct LongShotConfig {
    pub direction: Direction,
    pub limits: Limits,
    /// Share of the scrollable content scrolled each time (CAP-6: about 80 %).
    pub step_fraction: f64,
    /// The first scroll happens before the fixed header and footer are known, so it is shorter
    /// (it must stay inside the content area), and before the real distance of one wheel step in
    /// this app is known: a real wheel can move up to three times what was asked.
    pub first_step_fraction: f64,
    pub settle: Settle,
}

impl Default for LongShotConfig {
    fn default() -> Self {
        LongShotConfig {
            direction: Direction::Vertical,
            limits: Limits::default(),
            step_fraction: 0.8,
            first_step_fraction: 0.3,
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
    run_auto_ex(
        src,
        scroller,
        sleeper,
        cfg,
        stop,
        on_progress,
        &mut |_: &Stitcher| {},
    )
}

/// [`run_auto`] that also hands the stitcher to `on_stitch` after every joined frame (the daemon
/// renders the side preview from it).
#[allow(clippy::too_many_arguments)]
pub fn run_auto_ex(
    src: &mut dyn FrameSource,
    scroller: &mut dyn Scroller,
    sleeper: &mut dyn Sleeper,
    cfg: LongShotConfig,
    stop: &mut dyn FnMut() -> bool,
    on_progress: &mut dyn FnMut(LongShotProgress),
    on_stitch: &mut dyn FnMut(&Stitcher),
) -> Result<LongShotResult, CmError> {
    let mut stitcher = Stitcher::new(cfg.direction, cfg.limits);
    let first = wait_settled(src, sleeper, cfg.settle)?;
    stitcher.push(&first.frame, None);
    {
        on_progress(progress(&stitcher));
        on_stitch(&stitcher);
    }
    let size = f64::from(extent(&first.frame, cfg.direction));
    let mut last_ok = first.frame.clone();
    let mut step = ((size * cfg.first_step_fraction) as u32).max(1);
    let end = loop {
        if stop() {
            break EndReason::StoppedByUser;
        }
        scroller.scroll(cfg.direction, step)?;
        let mut next = wait_settled(src, sleeper, cfg.settle)?;
        let mut outcome = stitcher.push(&next.frame, Some(step));
        // An unchanged frame is the end of the content only when something has scrolled before.
        // A wheel event can arrive late (a browser's smooth scrolling starts after the settle
        // check), so the screen is looked at again a few times before "nothing changed" is
        // believed.
        for _ in 0..LATE_SCROLL_POLLS {
            if outcome != PushOutcome::NoChange {
                break;
            }
            sleeper.sleep(LATE_SCROLL_WAIT);
            next = wait_settled(src, sleeper, cfg.settle)?;
            outcome = stitcher.push(&next.frame, Some(step));
        }
        if outcome == PushOutcome::NoChange && stitcher.frames() == 1 {
            // The first scroll moved nothing: scroll once more (the scroller puts its pointer
            // back on the area first) before giving up.
            scroller.scroll(cfg.direction, step)?;
            sleeper.sleep(LATE_SCROLL_WAIT);
            next = wait_settled(src, sleeper, cfg.settle)?;
            outcome = stitcher.push(&next.frame, Some(step));
            if outcome == PushOutcome::NoChange {
                break EndReason::NothingScrolled;
            }
        }
        match outcome {
            PushOutcome::Added { new_len } => {
                last_ok = next.frame.clone();
                on_progress(progress(&stitcher));
                on_stitch(&stitcher);
                // Pixels the content really moved per pixel asked (a wheel step differs per app
                // and per display scale): learned from each join, so the next request lands on
                // about 80 %.
                let moved_per_asked = (f64::from(new_len) / f64::from(step)).clamp(0.25, 4.0);
                // 80 % of what actually scrolls: the area without the fixed bands.
                let (h, f) = stitcher.bands();
                let content = (size - f64::from(h) - f64::from(f)).max(1.0);
                step = ((content * cfg.step_fraction / moved_per_asked) as u32).max(1);
            }
            PushOutcome::NoChange => break EndReason::ReachedEnd,
            PushOutcome::NoOverlap => {
                super::debug::dump_refused(
                    &last_ok,
                    &next.frame,
                    stitcher.last_refusal(),
                    "auto scroll",
                );
                break EndReason::NoOverlap;
            }
            PushOutcome::LimitReached { .. } => {
                {
                    on_progress(progress(&stitcher));
                    on_stitch(&stitcher);
                }
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
/// (two polls `settle.interval` apart are equal) and differs from the last frame taken; a
/// frame from the middle of a long flick is taken too when waiting would lose the overlap.
/// Ends when `stop` returns true or at the limit. A frame that does not line up with the last
/// one taken does not end the capture (the user may scroll back, or less, or a bar on the page
/// may be fading): it is left out and the next frame that joins is taken. When the user stops
/// while the last frame seen was left out, the end is [`EndReason::NoOverlap`], so the note
/// "Stopped: the next part did not line up" is shown; the image so far is kept either way.
pub fn run_hand(
    src: &mut dyn FrameSource,
    sleeper: &mut dyn Sleeper,
    cfg: LongShotConfig,
    stop: &mut dyn FnMut() -> bool,
    on_progress: &mut dyn FnMut(LongShotProgress),
) -> Result<LongShotResult, CmError> {
    run_hand_ex(src, sleeper, cfg, stop, on_progress, &mut |_: &Stitcher| {})
}

/// Polls without two equal frames in a row after which Hand scroll takes a frame anyway.
const HAND_MAX_MOVING_POLLS: u32 = 20;

/// A frame seen while the content moves is taken when it is this share of the content area on
/// from the last one taken (the overlap kept is the rest).
const HAND_TAKE_SHARE: f64 = 0.7;

/// Pushes a frame that was checked to join (see [`Stitcher::joins`]) and reports it. `Some` is
/// the end of the capture (the limit was reached by it).
fn take_frame(
    stitcher: &mut Stitcher,
    frame: &Frame,
    on_progress: &mut dyn FnMut(LongShotProgress),
    on_stitch: &mut dyn FnMut(&Stitcher),
) -> Option<EndReason> {
    let outcome = stitcher.push(frame, None);
    if matches!(
        outcome,
        PushOutcome::Added { .. } | PushOutcome::LimitReached { .. }
    ) {
        on_progress(progress(stitcher));
        on_stitch(stitcher);
    }
    matches!(outcome, PushOutcome::LimitReached { .. }).then_some(EndReason::LimitReached)
}

/// [`run_hand`] with the `on_stitch` hook of [`run_auto_ex`].
pub fn run_hand_ex(
    src: &mut dyn FrameSource,
    sleeper: &mut dyn Sleeper,
    cfg: LongShotConfig,
    stop: &mut dyn FnMut() -> bool,
    on_progress: &mut dyn FnMut(LongShotProgress),
    on_stitch: &mut dyn FnMut(&Stitcher),
) -> Result<LongShotResult, CmError> {
    let mut stitcher = Stitcher::new(cfg.direction, cfg.limits);
    let first = wait_settled(src, sleeper, cfg.settle)?;
    stitcher.push(&first.frame, None);
    {
        on_progress(progress(&stitcher));
        on_stitch(&stitcher);
    }
    let mut last_taken = first.frame;
    let mut prev_poll = last_taken.clone();
    // The settled frame that did not join the last one taken (the stitcher's state has not
    // changed since, so the same frame would be refused again).
    let mut refused: Option<Frame> = None;
    // The newest frame seen while the content was still moving that joins the last one taken.
    // A flick (a trackpad, a wheel with momentum, Page Down in a terminal) can carry the content
    // further than one screen before it settles: the frame at the settled place then has no
    // overlap with the last one taken. This frame, the furthest one that still had, is taken
    // first so nothing between is lost. It is only ever taken for that: a frame in the middle of
    // an animation (an image fading in) is not what the person scrolled to.
    let mut bridge: Option<Frame> = None;
    let mut moving_polls = 0u32;
    let end = loop {
        if stop() {
            break if refused.is_some() {
                EndReason::NoOverlap
            } else {
                EndReason::StoppedByUser
            };
        }
        sleeper.sleep(cfg.settle.interval);
        let g = src.grab()?;
        // Content that never holds still (a video, a spinner) is taken after a second anyway.
        let settled = g == prev_poll || moving_polls >= HAND_MAX_MOVING_POLLS;
        prev_poll = g.clone();
        if g == last_taken {
            // Back at the last good frame: nothing is left out any more.
            refused = None;
            bridge = None;
            moving_polls = 0;
            continue;
        }
        if !settled {
            moving_polls += 1;
            let share = stitcher.join_share(&g);
            if share.is_some_and(|s| s >= HAND_TAKE_SHARE) {
                // Continuous scrolling never settles (small wheel steps one after the other):
                // a frame that is this far on is taken as it is, before the overlap is gone.
                if let Some(end) = take_frame(&mut stitcher, &g, on_progress, on_stitch) {
                    break end;
                }
                last_taken = g;
                bridge = None;
                moving_polls = 0;
            } else if share.is_some() {
                bridge = Some(g);
            } else if let Some(b) = bridge.take() {
                // This frame is past the overlap: take the furthest one that was not.
                if let Some(end) = take_frame(&mut stitcher, &b, on_progress, on_stitch) {
                    break end;
                }
                last_taken = b;
                if stitcher.join_share(&g).is_some() {
                    bridge = Some(g);
                }
            }
            continue;
        }
        moving_polls = 0;
        if refused.as_ref() == Some(&g) {
            continue;
        }
        let mut outcome = stitcher.push(&g, None);
        if outcome == PushOutcome::NoOverlap {
            if let Some(b) = bridge.take() {
                // The settled frame is past the overlap of the last one taken: bridge first.
                match take_frame(&mut stitcher, &b, on_progress, on_stitch) {
                    Some(end) => break end,
                    None => {
                        last_taken = b;
                        outcome = stitcher.push(&g, None);
                    }
                }
            }
        }
        bridge = None;
        match outcome {
            PushOutcome::Added { .. } => {
                refused = None;
                {
                    on_progress(progress(&stitcher));
                    on_stitch(&stitcher);
                }
                last_taken = g;
            }
            PushOutcome::NoChange => refused = None,
            PushOutcome::NoOverlap => {
                super::debug::dump_refused(&last_taken, &g, stitcher.last_refusal(), "hand scroll");
                refused = Some(g);
            }
            PushOutcome::LimitReached { .. } => {
                {
                    on_progress(progress(&stitcher));
                    on_stitch(&stitcher);
                }
                break EndReason::LimitReached;
            }
        }
    };
    Ok(LongShotResult {
        image: stitcher.finish()?,
        end,
    })
}
