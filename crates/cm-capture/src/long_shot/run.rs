//! The Long Shot driver: Auto scroll and Hand scroll loops over the traits.

use cm_core::CmError;

use super::stitch::{Direction, EndReason, Limits, PushOutcome, Stitched, Stitcher};
use super::{wait_settled, FrameSource, Scroller, Settle, Sleeper};
use crate::Frame;

/// How often, and after what wait, an unchanged frame after a scroll is looked at again.
const LATE_SCROLL_POLLS: u32 = 3;
const LATE_SCROLL_WAIT: std::time::Duration = std::time::Duration::from_millis(150);
/// A join that moved less than this share of the asked distance is looked at again (see
/// `run_auto_ex`): real wheel steps move about as much as asked or more.
const MIN_MOVED_PER_ASKED: f64 = 0.5;

/// The first frame is taken again after this wait, and the later one is the first: a window that
/// has just been activated (its title bar changes colour), an overlay or a dialog that is still
/// fading out, a scroller that is not ready. A first frame that is not what stays on screen
/// refuses every join after it (Windows, Nov 15, 2026: one frame, `no_overlap`).
const FIRST_FRAME_WARMUP: std::time::Duration = std::time::Duration::from_millis(250);
/// When the first scroll moved nothing, the step is sent again this many times (the first after
/// [`FIRST_SCROLL_RETRY_WAIT`] at the same point, the others at the scroller's next point)
/// before the run ends with `nothing_scrolled`.
const FIRST_SCROLL_RETRIES: u32 = 3;
const FIRST_SCROLL_RETRY_WAIT: std::time::Duration = std::time::Duration::from_millis(200);
/// A first pair that does not join is looked at again this many times (waiting longer each
/// time) before the run ends with `no_overlap`.
const FIRST_PAIR_RETRIES: u32 = 2;
/// A later pair that does not join (the content moved past the overlap, or the overlap is too
/// little or too alike to tell where it is: a terminal's numbered lines) is tried again this
/// many times, each time after scrolling back by [`BACK_SHARE`] of the asked step, before the
/// run ends with `no_overlap`.
const NO_OVERLAP_RETRIES: u32 = 2;
const BACK_SHARE: f64 = 0.5;

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
    /// `false` when the newest settled frame did not line up with the last one taken and was
    /// left out (Hand scroll; the HUD says "Scroll back up a little" until a frame joins again).
    pub last_join_ok: bool,
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
        last_join_ok: true,
    }
}

/// The shift expected from asking for `step`, given what a step has moved so far.
fn expected(step: u32, gain: f64) -> u32 {
    ((f64::from(step) * gain).round() as u32).max(1)
}

fn extent(f: &Frame, d: Direction) -> u32 {
    match d {
        Direction::Vertical => f.height(),
        Direction::Horizontal => f.width(),
    }
}

/// The end of a run whose last frame did not give a clean result: when Stop or Esc was asked in
/// the meantime, it was the user who ended it (the frame taken while the HUD was closing, or the
/// frame the stop interrupted, is not a failed join). QA Windows pass 3: Stop stored
/// `no_overlap` in 2 of 3 runs.
fn stopped_or(stop: &mut dyn FnMut() -> bool, end: EndReason) -> EndReason {
    if stop() {
        EndReason::StoppedByUser
    } else {
        end
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
    wait_settled(src, sleeper, cfg.settle)?;
    sleeper.sleep(FIRST_FRAME_WARMUP);
    let first = wait_settled(src, sleeper, cfg.settle)?;
    stitcher.push(&first.frame, None);
    {
        on_progress(progress(&stitcher));
        on_stitch(&stitcher);
    }
    let size = f64::from(extent(&first.frame, cfg.direction));
    let mut last_ok = first.frame.clone();
    let mut step = ((size * cfg.first_step_fraction) as u32).max(1);
    // Pixels the content moved per pixel asked, once a join has taught it (1 before): the
    // expected shift of a step, which breaks ties between equally good overlaps.
    let mut gain = 1.0f64;
    let mut learned = 0u32;
    let end = loop {
        if stop() {
            break EndReason::StoppedByUser;
        }
        scroller.scroll(cfg.direction, step)?;
        let mut next = wait_settled(src, sleeper, cfg.settle)?;
        let mut outcome = stitcher.push(&next.frame, Some(expected(step, gain)));
        // The first pair that does not join: the screen may still be changing for another
        // reason than the scroll (a window coming to the front, a dialog fading out). Looked at
        // again, waiting longer each time, before the run is given up.
        let mut retry = 0;
        while outcome == PushOutcome::NoOverlap
            && stitcher.frames() == 1
            && retry < FIRST_PAIR_RETRIES
        {
            retry += 1;
            sleeper.sleep(LATE_SCROLL_WAIT * (2 * retry));
            next = wait_settled(src, sleeper, cfg.settle)?;
            outcome = stitcher.push(&next.frame, Some(expected(step, gain)));
        }
        // A later pair that does not join: scroll back by half a step and look again before the
        // run is given up (the scroller moved more than asked, or the overlap that is left is
        // too alike to tell where it is). What the retries scroll back is not part of the
        // distance the join teaches, which is counted from `asked`.
        let mut asked = step;
        let mut back_total = 0u32;
        let mut retry = 0;
        while outcome == PushOutcome::NoOverlap
            && stitcher.frames() > 1
            && retry < NO_OVERLAP_RETRIES
            && !stop()
        {
            retry += 1;
            let back = ((f64::from(step) * BACK_SHARE) as u32).max(1);
            if !scroller.scroll_back(cfg.direction, back)? {
                break;
            }
            back_total += back;
            asked = step.saturating_sub(back_total).max(1);
            sleeper.sleep(LATE_SCROLL_WAIT);
            next = wait_settled(src, sleeper, cfg.settle)?;
            // No expected shift: how far the scroll-back moved is no better known than how far
            // the step did, and a wrong prior would pick one of several alike overlaps.
            outcome = stitcher.push(&next.frame, None);
            // Back where the last frame was taken is not the end of the content.
            if outcome == PushOutcome::NoChange {
                outcome = PushOutcome::NoOverlap;
            }
        }
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
            outcome = stitcher.push(&next.frame, Some(expected(step, gain)));
        }
        if outcome == PushOutcome::NoChange && stitcher.frames() == 1 {
            // The first scroll moved nothing. A fresh window can ignore the wheel for a moment
            // (the first retry waits and scrolls at the same point), and a wheel that went to
            // another window (this app's own, on top of the area) needs another point (the
            // later retries ask the scroller for one) before nothing scrolled is believed.
            for attempt in 0..FIRST_SCROLL_RETRIES {
                sleeper.sleep(FIRST_SCROLL_RETRY_WAIT);
                if attempt > 0 {
                    scroller.next_point();
                }
                scroller.scroll(cfg.direction, step)?;
                sleeper.sleep(LATE_SCROLL_WAIT);
                next = wait_settled(src, sleeper, cfg.settle)?;
                outcome = stitcher.push(&next.frame, Some(expected(step, gain)));
                if outcome != PushOutcome::NoChange {
                    break;
                }
            }
            if outcome == PushOutcome::NoChange {
                break stopped_or(stop, EndReason::NothingScrolled);
            }
        }
        match outcome {
            PushOutcome::Added { new_len } => {
                last_ok = next.frame.clone();
                // A browser's smooth scrolling can pause part-way (seen on Edge, Windows: a frame
                // at a tenth of the asked distance that looked settled). Believing it taught a
                // distance far too short, the next step was huge and the join was lost. When much
                // less than asked arrived, the screen is looked at again; what moves later is
                // joined on top, and only the total teaches the distance.
                let mut new_len = new_len;
                let mut ended = None;
                let mut tail = 0;
                while f64::from(new_len) < f64::from(asked) * MIN_MOVED_PER_ASKED
                    && tail < LATE_SCROLL_POLLS
                {
                    tail += 1;
                    sleeper.sleep(LATE_SCROLL_WAIT);
                    let more = wait_settled(src, sleeper, cfg.settle)?;
                    match stitcher.push(&more.frame, Some(expected(asked, gain))) {
                        PushOutcome::Added { new_len: n } => {
                            new_len += n;
                            last_ok = more.frame.clone();
                        }
                        PushOutcome::NoChange => break,
                        PushOutcome::NoOverlap => {
                            ended = Some(EndReason::NoOverlap);
                            break;
                        }
                        PushOutcome::LimitReached { .. } => {
                            ended = Some(EndReason::LimitReached);
                            break;
                        }
                    }
                }
                on_progress(progress(&stitcher));
                on_stitch(&stitcher);
                if let Some(e) = ended {
                    break stopped_or(stop, e);
                }
                // Pixels the content really moved per pixel asked (a wheel step differs per app
                // and per display scale): learned from each join, so the next request lands on
                // about 80 %.
                // The first join replaces the guess; later ones move it halfway, so that one step
                // that a wheel rounded the other way (a notch more or less) does not make the
                // next one overshoot the overlap. A join found after scrolling back teaches
                // nothing: how far the scroll-back moved is only about known.
                if back_total == 0 {
                    let seen = (f64::from(new_len) / f64::from(asked)).clamp(0.25, 4.0);
                    gain = if learned == 0 {
                        seen
                    } else {
                        (gain + seen) / 2.0
                    };
                    learned += 1;
                }
                let moved_per_asked = gain;
                // 80 % of what actually scrolls: the area without the fixed bands.
                let (h, f) = stitcher.bands();
                let content = (size - f64::from(h) - f64::from(f)).max(1.0);
                step = ((content * cfg.step_fraction / moved_per_asked) as u32).max(1);
            }
            PushOutcome::NoChange => break stopped_or(stop, EndReason::ReachedEnd),
            PushOutcome::NoOverlap => {
                super::debug::dump_refused(
                    &last_ok,
                    &next.frame,
                    stitcher.last_refusal(),
                    "auto scroll",
                );
                break stopped_or(stop, EndReason::NoOverlap);
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

/// Equal polls in a row (the content holds still for this many poll intervals, about 100 to
/// 150 ms) before Hand scroll takes the settled frame. Decision of Nov 15, 2026.
const HAND_STILL_POLLS: u32 = 2;

/// A frame seen while the content moves is taken when it is this share of the content area on
/// from the last one taken (the overlap kept is the rest).
/// Decision of Nov 15, 2026 (maintainer away, may change it): a third of the area, so that slow
/// continuous scrolling (small wheel steps one after the other, which never holds still) gives a
/// frame every third of a screen. The other trigger is the content holding still
/// ([`HAND_STILL_POLLS`]).
const HAND_TAKE_SHARE: f64 = 1.0 / 3.0;

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
    // No warm-up here: the person scrolls when they like, and the rebase below covers a first
    // frame that was not what stayed on screen.
    let first = wait_settled(src, sleeper, cfg.settle)?;
    stitcher.push(&first.frame, None);
    {
        on_progress(progress(&stitcher));
        on_stitch(&stitcher);
    }
    // The last settled frame that did not join the first one: when the next one joins it but
    // neither joins the first, the first frame was not what stayed on screen (a window coming
    // to the front, a dialog fading out) and the capture goes on from these two.
    let mut first_refused: Option<Frame> = None;
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
    let mut still_polls = 0u32;
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
        still_polls = if g == prev_poll { still_polls + 1 } else { 0 };
        let settled = still_polls >= HAND_STILL_POLLS || moving_polls >= HAND_MAX_MOVING_POLLS;
        prev_poll = g.clone();
        if g == last_taken {
            // Back at the last good frame: nothing is left out any more.
            if refused.take().is_some() {
                on_progress(progress(&stitcher));
            }
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
            PushOutcome::NoChange => {
                if refused.take().is_some() {
                    on_progress(progress(&stitcher));
                }
            }
            PushOutcome::NoOverlap => {
                super::debug::dump_refused(&last_taken, &g, stitcher.last_refusal(), "hand scroll");
                if stitcher.frames() == 1 {
                    let earlier = first_refused.take().filter(|e| *e != g);
                    let mut fresh = Stitcher::new(cfg.direction, cfg.limits);
                    let rebased = earlier.is_some_and(|e| {
                        fresh.push(&e, None);
                        matches!(fresh.push(&g, None), PushOutcome::Added { .. })
                    });
                    if rebased {
                        stitcher = fresh;
                        {
                            on_progress(progress(&stitcher));
                            on_stitch(&stitcher);
                        }
                        last_taken = g;
                        refused = None;
                        continue;
                    }
                    first_refused = Some(g.clone());
                }
                refused = Some(g);
                // Only a notice for the HUD: nothing was added.
                on_progress(LongShotProgress {
                    last_join_ok: false,
                    ..progress(&stitcher)
                });
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
