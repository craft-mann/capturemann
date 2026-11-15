//! The Long Shot stitching engine (CAP-6). Headless and pure: frames in, one tall image out.
//!
//! Rules (decisions of Nov 14, 2026, specs/features/01-screenshot.md CAP-6):
//!
//! * Consecutive frames are matched by their overlapping lines: a 64-bit hash per line (exact,
//!   with a small allowance of changed lines for a blinking caret), then a byte-for-byte check of
//!   every line the hash called equal.
//! * Fixed headers and footers are the bands that stay identical between the first two frames.
//!   The header is kept once at the top and the footer once at the bottom.
//! * **Never guess a join.** No overlap, or several equally good overlaps that the expected step
//!   cannot tell apart, ends the capture there with [`EndReason::NoOverlap`]; what was captured
//!   before is kept.
//! * The result stops at 16,384 px on its long side and 50 megapixels ([`Limits`]).
//! * Horizontal capture is the same method on columns (the frames are transposed in and out).
//! * **The edges of a window do not hold still**: a scrollbar's thumb moves with the content and
//!   the window's border and rounded corners blend with it, so the lines of a real window never
//!   match exactly (found on a real Windows window, Nov 14, 2026). When the first two frames do
//!   not join as they are, the engine finds the join on the middle half of every line, then looks
//!   for columns that differ in the overlap and cuts the strips at the edges that hold them
//!   (`edge_strip`). The strips are cut from every frame of that capture, so the image is that
//!   many pixels narrower and has no scrollbar.

use cm_core::CmError;

use crate::Frame;

/// The longest side the editor and renderer handle (the same limit as an imported picture).
pub const MAX_LONG_SIDE: u32 = 16_384;
/// The pixel limit of the stitched image (the same limit as an imported picture).
pub const MAX_PIXELS: u64 = 50_000_000;

/// Which way the content scrolls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    #[default]
    Vertical,
    Horizontal,
}

/// Size limits of the stitched image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Longest side, px. Never above [`MAX_LONG_SIDE`].
    pub max_len: u32,
    pub max_pixels: u64,
}

impl Limits {
    /// `max_px` from the caller (an agent's `max_px`), clamped to the product limit.
    pub fn with_max_px(max_px: Option<u32>) -> Limits {
        Limits {
            max_len: max_px.unwrap_or(MAX_LONG_SIDE).clamp(1, MAX_LONG_SIDE),
            max_pixels: MAX_PIXELS,
        }
    }
}

impl Default for Limits {
    fn default() -> Self {
        Limits::with_max_px(None)
    }
}

/// Why a Long Shot ended. Every ending keeps the image.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndReason {
    /// A scroll changed nothing: the end of the content.
    ReachedEnd,
    /// "Stopped: the next part did not line up".
    NoOverlap,
    /// "Reached the longest image CaptureMann can edit".
    LimitReached,
    StoppedByUser,
    /// The first scroll changed nothing, twice: the window did not scroll (the wheel went to
    /// another window, or the content does not scroll). Never reported as the end of content.
    NothingScrolled,
}

impl EndReason {
    /// The English note shown with the saved image (the UI translates the same strings).
    pub fn note(self) -> Option<&'static str> {
        match self {
            EndReason::NoOverlap => Some("Stopped: the next part did not line up"),
            EndReason::LimitReached => Some("Reached the longest image CaptureMann can edit"),
            EndReason::NothingScrolled => {
                Some("Nothing scrolled: check that the window can scroll")
            }
            EndReason::ReachedEnd | EndReason::StoppedByUser => None,
        }
    }
}

/// What one [`Stitcher::push`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushOutcome {
    /// The frame joined; `new_len` lines were added.
    Added { new_len: u32 },
    /// The frame equals the previous one: the end of the content (or the user did not scroll).
    NoChange,
    /// No join was found (or the frame is a different size). Nothing was added.
    NoOverlap,
    /// The limit was reached; `new_len` lines were added to fill the image up to it.
    LimitReached { new_len: u32 },
}

/// The finished image.
#[derive(Debug, Clone)]
pub struct Stitched {
    pub frame: Frame,
    /// Frames that joined (including the first).
    pub frames: u32,
    /// Fixed header and footer found, in px along the scroll direction.
    pub header_len: u32,
    pub footer_len: u32,
}

/// Lines of a frame along the scroll direction: rows (vertical) or columns (horizontal).
#[derive(Debug, Clone)]
struct Lines {
    /// Bytes per line.
    line_len: usize,
    count: usize,
    data: Vec<u8>,
    hashes: Vec<u64>,
}

impl Lines {
    fn from_frame(f: &Frame, dir: Direction) -> Lines {
        let (w, h) = (f.width() as usize, f.height() as usize);
        let data = match dir {
            Direction::Vertical => f.rgba().to_vec(),
            Direction::Horizontal => {
                let mut out = vec![0u8; w * h * 4];
                let src = f.rgba();
                for x in 0..w {
                    for y in 0..h {
                        let s = (y * w + x) * 4;
                        let d = (x * h + y) * 4;
                        out[d..d + 4].copy_from_slice(&src[s..s + 4]);
                    }
                }
                out
            }
        };
        let (line_len, count) = match dir {
            Direction::Vertical => (w * 4, h),
            Direction::Horizontal => (h * 4, w),
        };
        let hashes = (0..count)
            .map(|i| fnv(&data[i * line_len..(i + 1) * line_len]))
            .collect();
        Lines {
            line_len,
            count,
            data,
            hashes,
        }
    }

    /// The same lines keeping only pixels `from..to` across (the edge strips cut off).
    fn cropped(&self, from: usize, to: usize) -> Lines {
        let (b0, b1) = (from * 4, to * 4);
        let mut data = Vec::with_capacity(self.count * (b1 - b0));
        for i in 0..self.count {
            data.extend_from_slice(&self.line(i)[b0..b1]);
        }
        let keep = b1 - b0;
        let hashes = (0..self.count)
            .map(|i| fnv(&data[i * keep..(i + 1) * keep]))
            .collect();
        Lines {
            line_len: keep,
            count: self.count,
            data,
            hashes,
        }
    }

    fn line(&self, i: usize) -> &[u8] {
        &self.data[i * self.line_len..(i + 1) * self.line_len]
    }

    fn range(&self, from: usize, to: usize) -> &[u8] {
        &self.data[from * self.line_len..to * self.line_len]
    }

    /// Whether line `ia` of `self` and line `ib` of `other` are the same line: equal, or within
    /// [`NOISE_TOLERANCE`] per channel everywhere. Two captures of the same pixels of a Chrome
    /// window differ by one colour step in scattered pixels, in the toolbar and in the page
    /// alike (Retina Mac, Nov 14, 2026): an exact comparison found no header, rejected every
    /// join, and called the end of the page "did not line up".
    fn near(&self, ia: usize, other: &Lines, ib: usize) -> bool {
        if self.hashes[ia] == other.hashes[ib] && self.line(ia) == other.line(ib) {
            return true;
        }
        self.line(ia)
            .iter()
            .zip(other.line(ib))
            .all(|(a, b)| a.abs_diff(*b) <= NOISE_TOLERANCE)
    }

    /// [`Lines::near`], or, when `max_px` is more than 0, a line in which at most `max_px`
    /// pixels differ beyond the tolerance (a blinking caret, a clock, a terminal's cursor).
    fn near_tol(&self, ia: usize, other: &Lines, ib: usize, max_px: usize) -> bool {
        if self.near(ia, other, ib) {
            return true;
        }
        if max_px == 0 {
            return false;
        }
        let mut bad = 0;
        for (p, q) in self
            .line(ia)
            .as_chunks::<4>()
            .0
            .iter()
            .zip(other.line(ib).as_chunks::<4>().0.iter())
        {
            if p.iter()
                .zip(q)
                .any(|(a, b)| a.abs_diff(*b) > NOISE_TOLERANCE)
            {
                bad += 1;
                if bad > max_px {
                    return false;
                }
            }
        }
        true
    }

    /// Pixels of line `ia` that differ from line `ib` of `other` by more than the tolerance.
    fn diff_px(&self, ia: usize, other: &Lines, ib: usize) -> usize {
        self.line(ia)
            .as_chunks::<4>()
            .0
            .iter()
            .zip(other.line(ib).as_chunks::<4>().0.iter())
            .filter(|(p, q)| {
                p.iter()
                    .zip(q.iter())
                    .any(|(a, b)| a.abs_diff(*b) > NOISE_TOLERANCE)
            })
            .count()
    }

    fn same_line_tol(&self, other: &Lines, i: usize, max_px: usize) -> bool {
        self.near_tol(i, other, i, max_px)
    }
}

fn fnv(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for chunk in bytes.chunks(8) {
        let mut v = 0u64;
        for (i, b) in chunk.iter().enumerate() {
            v |= u64::from(*b) << (8 * i);
        }
        h = (h ^ v).wrapping_mul(0x0000_0100_0000_01b3);
        h ^= h >> 29;
    }
    h
}

/// The shift of a join as a share of the content area it was found in.
fn share(j: &Join, n: usize) -> f64 {
    j.shift as f64 / (n - j.header - j.footer).max(1) as f64
}

/// Where two consecutive frames join.
struct Join {
    /// Lines the content moved between the two frames.
    shift: usize,
    /// Lines at each end of the overlap that were not compared (see [`GUARD`]).
    guard: usize,
    header: usize,
    footer: usize,
}

/// Why two frames do not join.
enum NoJoin {
    Same,
    /// Why (for the debug dump and the tests, never shown to the user).
    NoOverlap(String),
}

/// Columns that differ in at least this share of the overlapping lines (and at least 3 lines)
/// belong to a moving edge (scrollbar, border).
const VOLATILE_SHARE: f64 = 0.01;
/// The widest strips cut from the start and the end of every line: a window's border and a
/// scrollbar with its edge (34 px at 200 % scale, and more with a border) at most.
const MAX_LEFT_STRIP: usize = 48;
const MAX_RIGHT_STRIP: usize = 64;
/// A strip of moving columns may have gaps of this many columns (a scrollbar's thumb, its arrows
/// and its border are not one block).
const STRIP_GAP: usize = 16;

/// Lines at each end of the overlap that are not compared when two frames are matched. They lie
/// next to the fixed bands, where a window draws its rounded corners and its edge line over
/// whatever scrolls under them: the pixels there are different in every frame (a real Edge window
/// at 150 %, Nov 14, 2026). Those lines are not trusted: the next frame's lines replace them.
const GUARD: usize = 8;

/// Per-channel difference that still counts as "the same" pixel between two captures.
const NOISE_TOLERANCE: u8 = 2;

/// Fixed bands may not take more than this share of a frame: more means the detection is not to
/// be trusted.
const MAX_BAND_SHARE: f64 = 0.6;
/// The smallest overlap that counts as a match, in lines (and as a share of the content).
const MIN_OVERLAP_LINES: usize = 8;
const MIN_OVERLAP_SHARE: f64 = 0.05;
/// Share of overlapping lines that may differ (a blinking caret, a clock) once the overlap has
/// at least [`NOISE_MIN_LINES`] lines.
const NOISE_SHARE: f64 = 0.02;
const NOISE_MIN_LINES: usize = 50;

/// For the debug dump: the shift whose overlap has the fewest differing lines, as text.
fn closest_shift(a: &Lines, b: &Lines, header: usize, cw: usize, min_ov: usize) -> String {
    let mut best: Option<(usize, usize, usize)> = None;
    for s in 1..=(cw - min_ov) {
        let overlap = cw - s;
        let g = GUARD.min(overlap / 3);
        let bad = (g..overlap - g)
            .filter(|&i| !a.near(header + s + i, b, header + i))
            .count();
        if best.is_none_or(|(_, bb, _)| bad < bb) {
            best = Some((s, bad, overlap - 2 * g));
        }
    }
    best.map_or_else(String::new, |(s, bad, of)| {
        format!("; closest: shift {s} with {bad} of {of} compared lines differing")
    })
}

/// The share of a line's pixels that may differ in a second, more tolerant look at two frames
/// that did not join: a blinking caret, a clock or a spinner in a fixed bar, a terminal's block
/// cursor (a real Chrome page with a sticky header, a Terminal window, Nov 14, 2026: such a
/// change made every frame "not line up"). At least [`LENIENT_MIN_PX`] pixels.
const LENIENT_SHARE: f64 = 0.03;
const LENIENT_MIN_PX: usize = 8;
/// Lines narrower than this (in pixels) are only ever compared exactly.
const LENIENT_MIN_LINE: usize = 256;

/// Where two frames join: exact lines first (the clean case, and the only one that never meets
/// an ambiguity from the tolerance); when that finds nothing, again with lines that differ in a
/// few pixels counted as the same.
fn find_join(
    a: &Lines,
    b: &Lines,
    hint: Option<usize>,
    bands: Option<(usize, usize)>,
) -> Result<Join, NoJoin> {
    match find_join_with(a, b, hint, bands, 0) {
        Err(NoJoin::NoOverlap(strict)) => {
            // Narrow lines have no room for a tolerance.
            if a.line_len / 4 < LENIENT_MIN_LINE {
                return Err(NoJoin::NoOverlap(strict));
            }
            let max_px = ((a.line_len / 4) as f64 * LENIENT_SHARE) as usize;
            match find_join_with(a, b, hint, bands, max_px.max(LENIENT_MIN_PX)) {
                Err(NoJoin::NoOverlap(lenient)) => Err(NoJoin::NoOverlap(format!(
                    "{strict}; with up to {} differing pixels per line allowed: {lenient}",
                    max_px.max(LENIENT_MIN_PX)
                ))),
                other => other,
            }
        }
        other => other,
    }
}

/// Most candidates whose overlaps are compared pixel by pixel (see [`best_by_difference`]).
const MAX_SCORED_CANDIDATES: usize = 4096;
/// The runner-up must differ by at least this share of the best (plus [`SCORE_MARGIN_PX`]).
const SCORE_MARGIN_SHARE: f64 = 0.1;
/// ... and by this many pixels per compared line.
const SCORE_MARGIN_PX: f64 = 0.5;

/// Among shifts that all match within the tolerance, the one whose overlap has the fewest
/// differing pixels per line, when the next one has clearly more. `rival` is a score (pixels per
/// line) the best one must also beat (the frames as they are, no scroll); infinity for none.
/// `None` on a tie or when there are too many to compare.
fn best_by_difference(
    a: &Lines,
    b: &Lines,
    header: usize,
    cw: usize,
    candidates: &[usize],
    rival: f64,
) -> Option<usize> {
    if candidates.len() > MAX_SCORED_CANDIDATES {
        return None;
    }
    let (mut best, mut best_s, mut second) = (f64::INFINITY, 0, rival);
    for &s in candidates {
        let overlap = cw - s;
        let g = GUARD.min(overlap / 3);
        let lines = (overlap - 2 * g).max(1) as f64;
        // Beyond this many differing pixels the shift is neither the best nor the runner-up.
        let cap = second * lines;
        let mut total = 0usize;
        for i in g..overlap - g {
            total += a.diff_px(header + s + i, b, header + i);
            if total as f64 > cap {
                break;
            }
        }
        let score = total as f64 / lines;
        if score < best {
            second = second.min(best);
            (best, best_s) = (score, s);
        } else if score < second {
            second = score;
        }
    }
    let margin = best * SCORE_MARGIN_SHARE + SCORE_MARGIN_PX;
    (best.is_finite() && second.is_finite() && second >= best + margin).then_some(best_s)
}

fn find_join_with(
    a: &Lines,
    b: &Lines,
    hint: Option<usize>,
    bands: Option<(usize, usize)>,
    max_px: usize,
) -> Result<Join, NoJoin> {
    let n = a.count;
    if a.data == b.data {
        return Err(NoJoin::Same);
    }
    // Every line equal within the tolerance: a blink (the exact look says the same), or, with the
    // tolerant look, lines that differ in a few pixels each (a terminal's numbered lines, the
    // text of a page) that scrolled. It is a scroll when a shift explains the difference better
    // than no shift does.
    let mut rival = f64::INFINITY;
    if (0..n).all(|i| a.same_line_tol(b, i, max_px)) {
        if max_px == 0 {
            return Err(NoJoin::Same);
        }
        rival = (0..n).map(|i| a.diff_px(i, b, i)).sum::<usize>() as f64 / n as f64;
    }
    let (header, footer) = match bands {
        Some((h, f)) => {
            // A header or footer that changed since the first pair: do not trust the join.
            if (0..h).any(|i| !a.same_line_tol(b, i, max_px))
                || (n - f..n).any(|i| !a.same_line_tol(b, i, max_px))
            {
                return Err(NoJoin::NoOverlap(format!(
                    "a fixed band changed since the first pair (header {h}, footer {f} lines)"
                )));
            }
            (h, f)
        }
        None => {
            let mut h = 0;
            while h < n && a.same_line_tol(b, h, max_px) {
                h += 1;
            }
            let mut f = 0;
            while f < n - h && a.same_line_tol(b, n - 1 - f, max_px) {
                f += 1;
            }
            if (h + f) as f64 > MAX_BAND_SHARE * n as f64 {
                (0, 0)
            } else {
                (h, f)
            }
        }
    };
    let cw = n - header - footer;
    let min_ov = MIN_OVERLAP_LINES.max((cw as f64 * MIN_OVERLAP_SHARE) as usize);
    if cw <= min_ov {
        return Err(NoJoin::NoOverlap(format!(
            "too little content between the bands ({cw} lines, header {header}, footer {footer})"
        )));
    }
    let mut candidates = Vec::new();
    for s in 1..=(cw - min_ov) {
        let overlap = cw - s;
        let allowed = if overlap >= NOISE_MIN_LINES {
            (overlap as f64 * NOISE_SHARE) as usize
        } else {
            0
        };
        // Row hashes first; then the pixel check of every line the hash called equal.
        let mut bad = 0;
        let g = GUARD.min(overlap / 3);
        for i in g..overlap - g {
            let (ia, ib) = (header + s + i, header + i);
            if !a.near_tol(ia, b, ib, max_px) {
                bad += 1;
                if bad > allowed {
                    break;
                }
            }
        }
        if bad <= allowed {
            candidates.push(s);
        }
    }
    // The tolerant look can call many shifts equal (text lines that differ in a few pixels each,
    // a terminal's numbered lines): the shift whose overlap differs least is the join, when it
    // clearly is the least. Never a guess: a tie is left to the expected step as before.
    let scored = if (candidates.len() > 1 || rival.is_finite()) && max_px > 0 {
        best_by_difference(a, b, header, cw, &candidates, rival)
    } else {
        None
    };
    if rival.is_finite() && scored.is_none() {
        return Err(NoJoin::Same);
    }
    let shift = match (candidates.len(), hint) {
        _ if scored.is_some() => scored.unwrap_or(0),
        (0, _) => {
            let near_miss = if super::debug::enabled() {
                closest_shift(a, b, header, cw, min_ov)
            } else {
                String::new()
            };
            return Err(NoJoin::NoOverlap(format!(
                "no shift matches (header {header}, footer {footer}, {n} lines){near_miss}"
            )));
        }
        (1, _) => candidates[0],
        (k, None) => {
            return Err(NoJoin::NoOverlap(format!(
                "{k} shifts match equally well (blank or repeating content) and no step is known"
            )))
        }
        (_, Some(h)) => {
            // Several equally good overlaps (repeating or blank content): the expected step may
            // choose, but only when it names one candidate clearly.
            // The expected step names one candidate clearly (the nearest is within a twentieth of
            // the content and the next one is further than that from it) ...
            let mut by_dist: Vec<(usize, usize)> =
                candidates.iter().map(|&s| (s.abs_diff(h), s)).collect();
            by_dist.sort_unstable();
            let (d0, s0) = by_dist[0];
            let tol = (cw / 20).max(2);
            if d0 <= tol && by_dist[1].0 > d0 + tol {
                s0
            } else {
                // ... or the wheel moved more or less than asked (a real one up to a few times
                // what is meant): the window around the expected step is a quarter of it, and
                // exactly one candidate inside it is the join. None, or two that cannot be told
                // apart, is refused.
                let wide = tol.max(h / 4);
                let mut near = candidates.iter().filter(|&&s| s.abs_diff(h) <= wide);
                match (near.next(), near.next()) {
                    (Some(&s0), None) => s0,
                    _ => {
                        return Err(NoJoin::NoOverlap(
                            "several shifts match and the expected step names none of them".into(),
                        ))
                    }
                }
            }
        }
    };
    Ok(Join {
        shift,
        guard: GUARD.min((cw - shift) / 3),
        header,
        footer,
    })
}

/// The most lines of a frame's end that are looked at for corners and shadow.
const MAX_CORNER_DEPTH: usize = 96;
/// Lines that are the same again in a row end the corner.
const CORNER_END_RUN: usize = 3;

/// How many lines at the end of the content of `prev` look different from the same content in
/// `lines` (the next frame, `join.shift` lines on): the window's rounded corners and shadow are
/// drawn over what scrolls at the edge of the capture, so the bottom lines of every frame have
/// pixels of the window instead of the page. They are not the page; the next frame's lines are
/// used in their place. The first frame's top and the last frame's bottom are never asked about,
/// so those stay as captured. Counted from the end of the content upwards, until three lines in
/// a row are the same.
fn corner_depth(prev: &Lines, lines: &Lines, join: &Join) -> usize {
    let n = prev.count;
    let overlap = n - join.header - join.footer - join.shift;
    let max = (overlap / 2).min(MAX_CORNER_DEPTH);
    let (mut depth, mut same) = (0, 0);
    for d in 0..max {
        let p = n - join.footer - 1 - d;
        if prev.near(p, lines, p - join.shift) {
            same += 1;
            if same >= CORNER_END_RUN {
                break;
            }
        } else {
            same = 0;
            depth = d + 1;
        }
    }
    depth
}

/// The widths of the strips at the start and the end of every line that hold columns which
/// differ between the two frames once they are aligned by `core` (a join found on the middle of
/// the lines). `None` when no column differs (the difference is not at the edges).
fn edge_strips(a: &Lines, b: &Lines, core: &Join, max: (usize, usize)) -> Option<(usize, usize)> {
    let n = a.count;
    let cross = a.line_len / 4;
    let overlap = n - core.header - core.footer - core.shift;
    // The rows compared: the overlap (aligned by the shift) and the fixed bands (aligned as they
    // are). A moving edge can lie in the bands alone: an overlay scrollbar's thumb sits at the top
    // of the page, next to a sticky header, in the first pair (Chrome, Nov 14, 2026).
    let band_rows = (0..core.header).chain(n - core.footer..n);
    let overlap_rows =
        (core.guard..overlap - core.guard).map(|i| (core.header + core.shift + i, core.header + i));
    let rows: Vec<(usize, usize)> = band_rows.map(|i| (i, i)).chain(overlap_rows).collect();
    let limit = (rows.len() as f64 * VOLATILE_SHARE) as usize;
    let mut bad = vec![0usize; cross];
    for (ia, ib) in rows {
        let (la, lb) = (a.line(ia), b.line(ib));
        for (x, c) in bad.iter_mut().enumerate() {
            if la[x * 4..x * 4 + 4] != lb[x * 4..x * 4 + 4] {
                *c += 1;
            }
        }
    }
    let volatile: Vec<bool> = bad.iter().map(|&c| c >= limit.max(3)).collect();
    let strip = |from_end: bool| {
        let max = if from_end { max.1 } else { max.0 };
        let (mut strip, mut gap) = (0, 0);
        for k in 0..max {
            let x = if from_end { cross - 1 - k } else { k };
            if volatile[x] {
                strip = k + 1;
                gap = 0;
            } else {
                gap += 1;
                if gap > STRIP_GAP {
                    break;
                }
            }
        }
        strip
    };
    let (l, r) = (strip(false), strip(true));
    (l + r > 0).then_some((l, r))
}

/// The widths of the moving edges (see [`edge_strips`]) that, cut from both frames, let them
/// join. `None` when no such cut makes them join.
fn edge_trim(prev: &Lines, lines: &Lines, hint: Option<usize>) -> Option<(usize, usize)> {
    let cross = lines.line_len / 4;
    // Too narrow to have edges to cut.
    if cross < 64 {
        return None;
    }
    // The join is first looked for without the widest strips an edge can be (a scrollbar and
    // a window border), which holds the content that matters and none of the moving edges.
    let (ml, mr) = (
        (cross / 8).min(MAX_LEFT_STRIP),
        (cross / 8).min(MAX_RIGHT_STRIP),
    );
    let (c0, c1) = (ml, cross - mr);
    let core = find_join(&prev.cropped(c0, c1), &lines.cropped(c0, c1), hint, None).ok()?;
    let (l, r) = edge_strips(prev, lines, &core, (ml, mr))?;
    let (p, n) = (prev.cropped(l, cross - r), lines.cropped(l, cross - r));
    find_join(&p, &n, hint, None).is_ok().then_some((l, r))
}

/// Rows (columns) that stay put across the whole scroll: a share of the lines at most
/// [`STATIC_SHARE`] differ in them. Only the unbroken run from each end counts: a browser's tab
/// strip and toolbar above a page that scrolls sideways, a status bar below it. No line of the
/// frame then matches the shifted line of the next frame, since the toolbar's icons do not move
/// with the page (Edge and Chrome on Windows, whole window, Nov 15, 2026). Returns the widths of
/// the two runs; the capture is cut to the part between them.
fn static_strips(a: &Lines, b: &Lines) -> (usize, usize) {
    let cross = a.line_len / 4;
    if cross < 32 || a.count != b.count {
        return (0, 0);
    }
    let limit = ((a.count as f64 * STATIC_SHARE) as usize).max(2);
    let mut bad = vec![0usize; cross];
    for i in 0..a.count {
        let (la, lb) = (a.line(i), b.line(i));
        for (x, c) in bad.iter_mut().enumerate() {
            let (p, q) = (&la[x * 4..x * 4 + 4], &lb[x * 4..x * 4 + 4]);
            if p.iter()
                .zip(q)
                .any(|(u, v)| u.abs_diff(*v) > NOISE_TOLERANCE)
            {
                *c += 1;
            }
        }
    }
    let is_static = |x: usize| bad[x] <= limit;
    let l = (0..cross).take_while(|&x| is_static(x)).count();
    if l == cross {
        return (0, 0);
    }
    let r = (0..cross - l)
        .take_while(|&k| is_static(cross - 1 - k))
        .count();
    // What is left must be able to hold a join.
    if cross - l - r < 16 {
        return (0, 0);
    }
    (l, r)
}

/// The share of the lines in which a row (column) of a static strip may differ between two
/// frames (a blinking caret, a loading spinner in the toolbar).
const STATIC_SHARE: f64 = 0.02;

/// Collects frames and builds the long image.
pub struct Stitcher {
    dir: Direction,
    limits: Limits,
    prev: Option<Lines>,
    /// Size of the frames (all must match).
    size: Option<(u32, u32)>,
    bands: Option<(usize, usize)>,
    /// Pixels cut from the start and the end of every line, once the first pair of frames has
    /// joined.
    trim: Option<(usize, usize)>,
    header: Vec<u8>,
    content: Vec<u8>,
    footer: Vec<u8>,
    frames: u32,
    /// Lines in the image so far (header + content + footer).
    len: usize,
    limit_hit: bool,
    /// Why the last frame was refused (see [`Stitcher::last_refusal`]).
    refusal: Option<String>,
}

impl Stitcher {
    pub fn new(dir: Direction, limits: Limits) -> Self {
        Stitcher {
            dir,
            limits,
            prev: None,
            size: None,
            bands: None,
            trim: None,
            header: Vec::new(),
            content: Vec::new(),
            footer: Vec::new(),
            frames: 0,
            len: 0,
            limit_hit: false,
            refusal: None,
        }
    }

    /// The lines `frame` is on from the last accepted one when it would join it (and is not the
    /// same), as a share of the content area; nothing is changed. Hand scroll asks it of frames
    /// taken while the content still moves.
    pub fn join_share(&self, frame: &Frame) -> Option<f64> {
        let Some(prev) = &self.prev else {
            return None;
        };
        if self.limit_hit || self.size != Some((frame.width(), frame.height())) {
            return None;
        }
        let mut lines = Lines::from_frame(frame, self.dir);
        if let Some((l, r)) = self.trim {
            if (l, r) != (0, 0) {
                let cross = lines.line_len / 4;
                lines = lines.cropped(l, cross - r);
            }
        }
        if lines.line_len != prev.line_len {
            return None;
        }
        match find_join(prev, &lines, None, self.bands) {
            Ok(j) => Some(share(&j, lines.count)),
            Err(NoJoin::Same) => None,
            Err(NoJoin::NoOverlap(_)) => match (self.bands, find_join(prev, &lines, None, None)) {
                (Some((h, f)), Ok(j)) if j.header <= h && j.footer <= f => {
                    Some(share(&j, lines.count))
                }
                _ => None,
            },
        }
    }

    /// Why the last refused frame did not join, as text for the debug dump.
    pub fn last_refusal(&self) -> Option<&str> {
        self.refusal.as_deref()
    }

    /// Frames that joined so far.
    pub fn frames(&self) -> u32 {
        self.frames
    }

    /// Length of the image so far along the scroll direction, px.
    pub fn len_px(&self) -> u32 {
        self.len as u32
    }

    /// The limit this capture stops at, px along the scroll direction.
    pub fn max_len(&self) -> u32 {
        self.limits.max_len
    }

    /// The fixed bands found so far, in px (header, footer).
    pub fn bands(&self) -> (u32, u32) {
        self.bands.map_or((0, 0), |(h, f)| (h as u32, f as u32))
    }

    fn room(&self, cross_px: usize) -> usize {
        let by_px = (self.limits.max_pixels / cross_px.max(1) as u64) as usize;
        (self.limits.max_len as usize).min(by_px)
    }

    /// Adds the next frame. `hint` is the step the scroller meant to scroll, in px; it only
    /// breaks ties between equally good overlaps.
    pub fn push(&mut self, frame: &Frame, hint: Option<u32>) -> PushOutcome {
        let lines = Lines::from_frame(frame, self.dir);
        let size = (frame.width(), frame.height());
        let Some(prev) = self.prev.take() else {
            self.len = lines.count;
            self.size = Some(size);
            self.prev = Some(lines);
            self.frames = 1;
            return PushOutcome::Added {
                new_len: self.len as u32,
            };
        };
        let (prev, lines) = self.apply_trim(prev, lines, hint);
        let outcome = self.join(&prev, &lines, size, hint);
        // The newest accepted frame is the one the next frame is matched against.
        self.prev = Some(if matches!(outcome, PushOutcome::Added { .. }) {
            lines
        } else {
            prev
        });
        outcome
    }

    /// Cuts the edge strips from both frames' lines: with the trim chosen so far, or (the first
    /// pair) the one the two frames need to join.
    fn apply_trim(&mut self, prev: Lines, lines: Lines, hint: Option<u32>) -> (Lines, Lines) {
        let cross = lines.line_len / 4;
        if let Some((l, r)) = self.trim {
            if l == 0 && r == 0 {
                return (prev, lines);
            }
            // `prev` was cropped when it was accepted; a frame kept uncropped is cropped now.
            let prev = if prev.line_len / 4 == cross {
                prev.cropped(l, cross - r)
            } else {
                prev
            };
            return (prev, lines.cropped(l, cross - r));
        }
        if prev.line_len != lines.line_len {
            return (prev, lines);
        }
        let hint = hint.map(|h| h as usize);
        match find_join(&prev, &lines, hint, None) {
            Ok(_) => {
                self.trim = Some((0, 0));
                return (prev, lines);
            }
            // Identical frames say nothing about the edges.
            Err(NoJoin::Same) => return (prev, lines),
            Err(NoJoin::NoOverlap(_)) => {}
        }
        // Moving edges (a scrollbar, a border) first, then static rows or columns across the
        // scroll direction (a browser's toolbar when a whole window scrolls sideways).
        if let Some((l, r)) = edge_trim(&prev, &lines, hint) {
            self.trim = Some((l, r));
            return (prev.cropped(l, cross - r), lines.cropped(l, cross - r));
        }
        let (sl, sr) = static_strips(&prev, &lines);
        if (sl, sr) == (0, 0) {
            return (prev, lines);
        }
        let (p0, n0) = (prev.cropped(sl, cross - sr), lines.cropped(sl, cross - sr));
        let (l, r) = if find_join(&p0, &n0, hint, None).is_ok() {
            (sl, sr)
        } else if let Some((l, r)) = edge_trim(&p0, &n0, hint) {
            (sl + l, sr + r)
        } else {
            return (prev, lines);
        };
        self.trim = Some((l, r));
        (prev.cropped(l, cross - r), lines.cropped(l, cross - r))
    }

    fn join(
        &mut self,
        prev: &Lines,
        lines: &Lines,
        size: (u32, u32),
        hint: Option<u32>,
    ) -> PushOutcome {
        if self.limit_hit || self.size != Some(size) {
            self.refusal = Some(format!(
                "the frame is {}x{} px, the first was {:?}{}",
                size.0,
                size.1,
                self.size,
                if self.limit_hit {
                    " (limit reached)"
                } else {
                    ""
                }
            ));
            return PushOutcome::NoOverlap;
        }
        let hint = hint.map(|h| h as usize);
        let n = lines.count;
        let line_len = lines.line_len;
        let join = match find_join(prev, lines, hint, self.bands) {
            Ok(j) => j,
            Err(NoJoin::Same) => return PushOutcome::NoChange,
            Err(NoJoin::NoOverlap(why)) => {
                self.refusal = Some(why);
                // A band that went away (a sticky header that hides while the page scrolls
                // down): the bands are found again from this pair, and only narrower ones are
                // accepted. The rows that were band become content.
                let (Some((h, f)), Ok(j)) = (self.bands, find_join(prev, lines, hint, None)) else {
                    return PushOutcome::NoOverlap;
                };
                if j.header > h || j.footer > f || (j.header == h && j.footer == f) {
                    return PushOutcome::NoOverlap;
                }
                let foot_cut = (f - j.footer) * line_len;
                let mut content = self.header.split_off(j.header * line_len);
                content.extend_from_slice(&self.content);
                content.extend_from_slice(&self.footer[..foot_cut]);
                self.footer.drain(..foot_cut);
                self.content = content;
                self.bands = Some((j.header, j.footer));
                j
            }
        };
        if self.bands.is_none() {
            // The second frame fixes the bands; the content starts with the first frame's.
            self.header = prev.range(0, join.header).to_vec();
            self.content = prev.range(join.header, n - join.footer).to_vec();
            self.bands = Some((join.header, join.footer));
        }
        let cw = n - join.header - join.footer;
        let have = join.header + self.content.len() / line_len;
        let total = have + join.shift + join.footer;
        let room = self.room(line_len / 4);
        // The new content is the bottom `shift` lines of this frame's content area.
        let from = join.header + cw - join.shift;
        // The last `guard` lines of the content were not compared: this frame's lines replace them.
        // So do the lines above them that this frame shows differently (the window's rounded
        // corners and shadow at the bottom of the previous frame: grey notches at every seam).
        let guard = join
            .guard
            .max(corner_depth(prev, lines, &join))
            .min(cw - join.shift)
            .min(self.content.len() / line_len);
        let join = Join { guard, ..join };
        let keep = self.content.len() - join.guard * line_len;
        if total <= room {
            self.content.truncate(keep);
            self.content
                .extend_from_slice(lines.range(from - join.guard, from + join.shift));
            self.footer = lines.range(n - join.footer, n).to_vec();
            self.len = total;
            self.frames += 1;
            return PushOutcome::Added {
                new_len: join.shift as u32,
            };
        }
        // Over the limit: fill up to it, drop the footer (the content under it was cut off).
        let take = room.saturating_sub(have).min(join.shift);
        self.content.truncate(keep);
        self.content
            .extend_from_slice(lines.range(from - join.guard, from + take));
        self.footer.clear();
        self.len = have + take;
        self.limit_hit = true;
        self.frames += 1;
        PushOutcome::LimitReached {
            new_len: take as u32,
        }
    }

    /// A small picture of the image so far for the side preview (S2): scaled to at most
    /// `max_w` px wide (never enlarged); when it would be taller than `max_h`, the newest
    /// `max_h` rows are kept (the preview follows the growing end). Horizontal captures are
    /// fitted whole into `max_w` x `max_h`. `None` before the first frame.
    pub fn thumbnail(&self, max_w: u32, max_h: u32) -> Option<Frame> {
        self.size?;
        let (max_w, max_h) = (max_w.max(1), max_h.max(1));
        if self.dir == Direction::Horizontal {
            let whole = self.finish().ok()?.frame;
            let scale = (f64::from(max_w) / f64::from(whole.width()))
                .min(f64::from(max_h) / f64::from(whole.height()))
                .min(1.0);
            return box_scale(
                whole.rgba(),
                whole.width() as usize,
                whole.height() as usize,
                scale,
                usize::MAX,
            );
        }
        let prev = self.prev.as_ref()?;
        let line_len = prev.line_len;
        // The lines were trimmed (a scrollbar, a border): the picture is as wide as they are.
        let w = (line_len / 4) as u32;
        let (header, content, footer): (&[u8], &[u8], &[u8]) = if self.bands.is_some() {
            (&self.header, &self.content, &self.footer)
        } else {
            (&prev.data, &[], &[])
        };
        let (hn, cn, fnn) = (
            header.len() / line_len,
            content.len() / line_len,
            footer.len() / line_len,
        );
        let row = |i: usize| -> &[u8] {
            let (src, at) = if i < hn {
                (header, i)
            } else if i < hn + cn {
                (content, i - hn)
            } else {
                (footer, i - hn - cn)
            };
            &src[at * line_len..(at + 1) * line_len]
        };
        let total = hn + cn + fnn;
        let scale = (f64::from(max_w) / f64::from(w)).min(1.0);
        box_rows(&row, total, w as usize, scale, max_h as usize)
    }

    /// The image so far. Errors when no frame was pushed.
    pub fn finish(&self) -> Result<Stitched, CmError> {
        let (Some(prev), Some(_)) = (&self.prev, self.size) else {
            return Err(CmError::invalid_input("Long Shot has no frames"));
        };
        let line_len = prev.line_len;
        // Pixels across the scroll direction after the trim.
        let cross = (line_len / 4) as u32;
        let (lines, header_len, footer_len) = if self.bands.is_none() {
            (prev.data.clone(), 0, 0)
        } else {
            let mut v =
                Vec::with_capacity(self.header.len() + self.content.len() + self.footer.len());
            v.extend_from_slice(&self.header);
            v.extend_from_slice(&self.content);
            v.extend_from_slice(&self.footer);
            (
                v,
                self.header.len() / line_len,
                self.footer.len() / line_len,
            )
        };
        let count = lines.len() / line_len;
        let frame = match self.dir {
            Direction::Vertical => Frame::new(cross, count as u32, lines)?,
            Direction::Horizontal => {
                // Lines are columns of `h` pixels: transpose back.
                let hh = cross as usize;
                let mut out = vec![0u8; lines.len()];
                for x in 0..count {
                    for y in 0..hh {
                        let s = (x * hh + y) * 4;
                        let d = (y * count + x) * 4;
                        out[d..d + 4].copy_from_slice(&lines[s..s + 4]);
                    }
                }
                Frame::new(count as u32, cross, out)?
            }
        };
        Ok(Stitched {
            frame,
            frames: self.frames,
            header_len: header_len as u32,
            footer_len: footer_len as u32,
        })
    }
}

/// Averages `row(i)` (rows of `w` RGBA pixels, `total` rows) down by `scale`, keeping the last
/// `max_h` output rows.
fn box_rows<'a>(
    row: &dyn Fn(usize) -> &'a [u8],
    total: usize,
    w: usize,
    scale: f64,
    max_h: usize,
) -> Option<Frame> {
    let out_w = ((w as f64 * scale).round() as usize).max(1);
    let out_total = ((total as f64 * scale).round() as usize).max(1);
    let out_h = out_total.min(max_h.max(1));
    let first = out_total - out_h;
    let mut out = vec![0u8; out_w * out_h * 4];
    for oy in 0..out_h {
        let gy = first + oy;
        let y0 = (gy as f64 / scale).floor() as usize;
        let y1 = (((gy + 1) as f64 / scale).ceil() as usize)
            .clamp(y0 + 1, total.max(y0 + 1))
            .min(total);
        let y0 = y0.min(total.saturating_sub(1));
        for ox in 0..out_w {
            let x0 = (ox as f64 / scale).floor() as usize;
            let x1 = (((ox + 1) as f64 / scale).ceil() as usize)
                .clamp(x0 + 1, w.max(x0 + 1))
                .min(w);
            let x0 = x0.min(w.saturating_sub(1));
            let mut acc = [0u32; 4];
            let mut n = 0u32;
            for y in y0..y1.max(y0 + 1) {
                let r = row(y);
                for x in x0..x1.max(x0 + 1) {
                    let p = &r[x * 4..x * 4 + 4];
                    for c in 0..4 {
                        acc[c] += u32::from(p[c]);
                    }
                    n += 1;
                }
            }
            let d = (oy * out_w + ox) * 4;
            for c in 0..4 {
                out[d + c] = (acc[c] / n.max(1)) as u8;
            }
        }
    }
    Frame::new(out_w as u32, out_h as u32, out).ok()
}

/// `box_rows` over one contiguous RGBA buffer.
fn box_scale(rgba: &[u8], w: usize, h: usize, scale: f64, max_h: usize) -> Option<Frame> {
    box_rows(&|i| &rgba[i * w * 4..(i + 1) * w * 4], h, w, scale, max_h)
}
