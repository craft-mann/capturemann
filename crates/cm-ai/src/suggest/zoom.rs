//! AI-3 auto-zoom suggestions: a deterministic algorithm on the cursor telemetry (adapted from
//! OpenScreen, MIT, with attribution in `NOTICE`; this is an independent implementation of the
//! rules in `specs/features/07-ai.md`).
//!
//! 1. **Dwell**: the pointer stays inside a box of [`DWELL_RADIUS`] of the frame (both axes) for
//!    [`DWELL_MIN`] to [`DWELL_MAX`]. The candidate is centred, in time and in place, on the
//!    dwell, and lasts the dwell plus [`ZOOM_PAD`] each side (the ease in and out), so the view is
//!    fully zoomed while the pointer rests. A parked pointer longer than [`DWELL_MAX`] is *not* a
//!    dwell (nothing is being pointed at), so it makes no suggestion; its clicks still count.
//! 2. **Click**: a left button press that is not inside a dwell. The candidate is centred on the
//!    click and lasts [`CLICK_LENGTH`].
//! 3. **Merge**: candidates whose centres are closer than [`MERGE_GAP`] merge, keeping the
//!    stronger (a dwell beats a click, a longer dwell beats a shorter one, the earlier wins a
//!    tie); in total at most one suggestion per [`PER_SECONDS`] of video (the output duration
//!    after cuts, pauses and speed), strongest first.
//! 4. **Frame**: scale [`DEFAULT_SCALE`], focus clamped so the zoomed view stays inside the frame,
//!    and every region clipped so it never overlaps a cut or a pause (a region that no longer
//!    fits `ZOOM_MIN_LENGTH` is dropped).
//!
//! The telemetry stays in session time, so no track mapping is needed. Nothing here edits the
//! edit list (AI-3.AC3).

use cm_core::edit::Span;
use cm_core::{CmError, EditList, MediaTime, PauseGap, ZOOM_MIN_LENGTH, ZOOM_SCALE_RANGE};
use cm_cursor::{Button, CursorEvent, CursorLog};

use super::{subtract, ZoomSuggestion};

/// The pointer counts as resting while it stays inside a box of this fraction of the frame.
pub const DWELL_RADIUS: f64 = 0.02;
/// Shortest dwell.
pub const DWELL_MIN: MediaTime = MediaTime::from_millis(450);
/// Longest dwell (a longer rest is a parked pointer).
pub const DWELL_MAX: MediaTime = MediaTime::from_millis(2600);
/// Added to a dwell on each side for the ease in and out.
pub const ZOOM_PAD: MediaTime = MediaTime::from_millis(400);
/// Length of a click candidate.
pub const CLICK_LENGTH: MediaTime = MediaTime::from_millis(1500);
/// Candidates closer than this (centre to centre) merge.
pub const MERGE_GAP: MediaTime = MediaTime::from_millis(1800);
/// At most one suggestion per this many seconds of video.
pub const PER_SECONDS: f64 = 6.0;
/// Default scale of a suggestion.
pub const DEFAULT_SCALE: f32 = 1.8;

/// What the caller can tune.
#[derive(Debug, Clone)]
pub struct ZoomOptions {
    /// Zoom scale of every suggestion.
    pub scale: f32,
    /// At most this many suggestions (`max` of the agent interface).
    pub max: Option<usize>,
}

impl Default for ZoomOptions {
    fn default() -> Self {
        ZoomOptions {
            scale: DEFAULT_SCALE,
            max: None,
        }
    }
}

#[derive(Debug, Clone)]
struct Candidate {
    center: i64,
    from: i64,
    to: i64,
    fx: f32,
    fy: f32,
    strength: f64,
    reason: &'static str,
}

/// A pointer position in the telemetry, as fractions of the frame.
#[derive(Debug, Clone, Copy)]
struct Pos {
    t: i64,
    x: f64,
    y: f64,
}

/// Suggests zoom regions for a recording. `edits` and `gaps` say what is kept (cuts and pauses are
/// never zoomed over); the suggestions are in session time and not applied.
///
/// A recording without telemetry (the cursor was not recorded) is `invalid_input`.
pub fn suggest_zooms(
    log: &CursorLog,
    edits: &EditList,
    gaps: &[PauseGap],
    opts: &ZoomOptions,
) -> Result<Vec<ZoomSuggestion>, CmError> {
    let scale = f64::from(opts.scale);
    if !(opts.scale.is_finite() && scale >= ZOOM_SCALE_RANGE.0 && scale <= ZOOM_SCALE_RANGE.1) {
        return Err(CmError::invalid_field(
            "scale",
            format!(
                "must be between {} and {}",
                ZOOM_SCALE_RANGE.0, ZOOM_SCALE_RANGE.1
            ),
        ));
    }
    let size = log.header.target.size;
    let (w, h) = (f64::from(size.width), f64::from(size.height));
    let no_data = || CmError::invalid_input("This recording has no cursor data");
    if w <= 0.0 || h <= 0.0 {
        return Err(no_data());
    }
    let mut positions: Vec<Pos> = Vec::new();
    let mut clicks: Vec<Pos> = Vec::new();
    for e in &log.events {
        match e {
            CursorEvent::Move {
                t, x, y, outside, ..
            }
            | CursorEvent::Up {
                t, x, y, outside, ..
            } if !outside => positions.push(Pos {
                t: *t,
                x: f64::from(*x) / w,
                y: f64::from(*y) / h,
            }),
            CursorEvent::Down {
                t,
                x,
                y,
                button,
                outside,
                ..
            } if !outside => {
                let p = Pos {
                    t: *t,
                    x: f64::from(*x) / w,
                    y: f64::from(*y) / h,
                };
                positions.push(p);
                if *button == Button::Left {
                    clicks.push(p);
                }
            }
            _ => {}
        }
    }
    if positions.is_empty() {
        return Err(no_data());
    }

    let kept = edits.without_pauses(gaps);
    let (lo, hi) = (kept.start.ticks(), kept.end.ticks());
    let blocked = &kept.cuts;

    // Dwell candidates. A position holds from its event to the next one, so a still pointer
    // (the log only records changes) has a long hold.
    let mut dwells: Vec<(i64, i64)> = Vec::new();
    let mut cands: Vec<Candidate> = Vec::new();
    let mut i = 0;
    while i < positions.len() {
        let a = positions[i];
        let (mut min_x, mut max_x, mut min_y, mut max_y) = (a.x, a.x, a.y, a.y);
        let mut k = i;
        while k + 1 < positions.len() {
            let n = positions[k + 1];
            let (nx0, nx1) = (min_x.min(n.x), max_x.max(n.x));
            let (ny0, ny1) = (min_y.min(n.y), max_y.max(n.y));
            if nx1 - nx0 > DWELL_RADIUS || ny1 - ny0 > DWELL_RADIUS {
                break;
            }
            (min_x, max_x, min_y, max_y) = (nx0, nx1, ny0, ny1);
            k += 1;
        }
        // The pointer left at the next event; the last position holds until the recording ends.
        let leave = positions.get(k + 1).map_or(hi.max(a.t), |n| n.t);
        let hold = leave - a.t;
        if (DWELL_MIN.ticks()..=DWELL_MAX.ticks()).contains(&hold) && a.t >= lo && leave <= hi {
            let mid = a.t + hold / 2;
            let half = hold / 2 + ZOOM_PAD.ticks();
            dwells.push((a.t, leave));
            cands.push(Candidate {
                center: mid,
                from: mid - half,
                to: mid + half,
                fx: ((min_x + max_x) / 2.0) as f32,
                fy: ((min_y + max_y) / 2.0) as f32,
                strength: 2.0 + (hold as f64 / DWELL_MAX.ticks() as f64),
                reason: "dwell",
            });
        }
        i = k + 1;
    }

    // Click candidates, for clicks outside every dwell.
    for c in &clicks {
        if c.t < lo || c.t > hi || dwells.iter().any(|&(a, b)| c.t >= a && c.t <= b) {
            continue;
        }
        let half = CLICK_LENGTH.ticks() / 2;
        cands.push(Candidate {
            center: c.t,
            from: c.t - half,
            to: c.t + half,
            fx: c.x as f32,
            fy: c.y as f32,
            strength: 1.0,
            reason: "click",
        });
    }

    // Clip to what is kept: the piece around the centre, dropped when it is too short.
    let min_len = ZOOM_MIN_LENGTH.ticks();
    let mut valid: Vec<Candidate> = Vec::new();
    for mut c in cands {
        let span = Span::new(MediaTime::from_ticks(c.from), MediaTime::from_ticks(c.to));
        let pieces = subtract(span, blocked, kept.start, kept.end);
        let piece = pieces
            .iter()
            .find(|p| p.from.ticks() <= c.center && c.center < p.to.ticks())
            .or_else(|| pieces.iter().max_by_key(|p| p.len()));
        let Some(p) = piece else { continue };
        if p.len().ticks() < min_len {
            continue;
        }
        c.from = p.from.ticks();
        c.to = p.to.ticks();
        valid.push(c);
    }

    // Merge and cap: strongest first, then earliest.
    let video_secs = kept.timeline().duration().as_secs_f64();
    let cap = ((video_secs / PER_SECONDS).floor() as usize)
        .max(1)
        .min(opts.max.unwrap_or(usize::MAX));
    valid.sort_by(|a, b| {
        b.strength
            .total_cmp(&a.strength)
            .then(a.center.cmp(&b.center))
    });
    let mut chosen: Vec<Candidate> = Vec::new();
    for c in valid {
        if chosen.len() >= cap {
            break;
        }
        if chosen
            .iter()
            .all(|o| (o.center - c.center).abs() >= MERGE_GAP.ticks())
        {
            chosen.push(c);
        }
    }
    chosen.sort_by_key(|c| c.from);

    let half_view = (0.5 / scale) as f32;
    let clamp = |v: f32| v.clamp(half_view, 1.0 - half_view);
    Ok(chosen
        .into_iter()
        .map(|c| ZoomSuggestion {
            start: MediaTime::from_ticks(c.from),
            end: MediaTime::from_ticks(c.to),
            focus: (clamp(c.fx), clamp(c.fy)),
            scale: opts.scale,
            reason: c.reason.to_owned(),
        })
        .collect())
}
