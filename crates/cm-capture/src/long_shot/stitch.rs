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
    /// The window was moved or resized while the run went on (the frames are another size, or
    /// the window's rectangle is not where it was): what was captured before is kept.
    WindowChanged,
    /// The window was closed while the run went on: what was captured before is kept.
    WindowClosed,
}

impl EndReason {
    /// The English note shown with the saved image (the UI translates the same strings).
    pub fn note(self) -> Option<&'static str> {
        match self {
            EndReason::NoOverlap => Some("Stopped: the next part did not line up"),
            EndReason::LimitReached => Some("Reached the longest image CaptureMann can edit"),
            EndReason::NothingScrolled => {
                Some("Nothing scrolled: the area may be covered by another window or the page is not scrollable")
            }
            EndReason::WindowChanged => Some("The window was moved or resized"),
            EndReason::WindowClosed => Some("The window was closed"),
            EndReason::ReachedEnd | EndReason::StoppedByUser => None,
        }
    }

    /// [`EndReason::note`] that names the real limit when the run stopped at one: the caller's
    /// `max_px`, the 50 megapixel rule, or the longest image the editor handles. `len` is the
    /// image's length along the scroll direction.
    pub fn note_for(self, limits: Limits, len: u32) -> Option<String> {
        if self != EndReason::LimitReached {
            return self.note().map(str::to_owned);
        }
        Some(match limit_kind(limits, len) {
            LimitKind::MaxPx => format!("Reached the size limit of {} px", limits.max_len),
            LimitKind::Pixels => "Reached the size limit of 50 megapixels".to_owned(),
            LimitKind::Editor => "Reached the longest image CaptureMann can edit".to_owned(),
        })
    }
}

/// Which limit a run that ended with [`EndReason::LimitReached`] met.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitKind {
    /// The caller asked for less than the product limit (`max_px`).
    MaxPx,
    /// The 50 megapixel rule: the image is shorter than the length limit.
    Pixels,
    /// [`MAX_LONG_SIDE`].
    Editor,
}

/// Rows an image may be short of the length limit and still have stopped at it.
const LIMIT_SLACK_PX: u32 = 64;

/// The limit behind a stop at `len` px along the scroll direction.
pub fn limit_kind(limits: Limits, len: u32) -> LimitKind {
    // An image a few rows short of the length limit stopped at it (a real Edge run: 1048x16383
    // said "50 megapixels" for a 17 MP image).
    if len.saturating_add(LIMIT_SLACK_PX) < limits.max_len {
        LimitKind::Pixels
    } else if limits.max_len < MAX_LONG_SIDE {
        LimitKind::MaxPx
    } else {
        LimitKind::Editor
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
/// Rows in which more columns than both edge strips can hold (plus this many) differ do not count
/// for the edge strips.
const WIDER_THAN_EDGES: usize = 16;
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
/// Most compared lines of a lenient match that may differ at all (see `find_join_with`).
const LENIENT_INEXACT_SHARE: f64 = 0.15;

/// Where two frames join: exact lines first (the clean case, and the only one that never meets
/// an ambiguity from the tolerance); when that finds nothing, again with lines that differ in a
/// few pixels counted as the same.
fn find_join(
    a: &Lines,
    b: &Lines,
    hint: Option<usize>,
    bands: Option<(usize, usize)>,
) -> Result<Join, NoJoin> {
    find_join_opt(a, b, hint, bands, true)
}

/// [`find_join`]; `robust` false leaves out the last look (stretches of lines that differ), which
/// would take a moving scrollbar's thumb for such a stretch: the first pair asks for the edges
/// first (see `Stitcher::apply_trim`).
fn find_join_opt(
    a: &Lines,
    b: &Lines,
    hint: Option<usize>,
    bands: Option<(usize, usize)>,
    robust: bool,
) -> Result<Join, NoJoin> {
    match find_join_with(a, b, hint, bands, 0) {
        Err(NoJoin::NoOverlap(strict)) => {
            // Narrow lines have no room for a tolerance.
            if a.line_len / 4 < LENIENT_MIN_LINE {
                return Err(NoJoin::NoOverlap(strict));
            }
            let max_px = ((a.line_len / 4) as f64 * LENIENT_SHARE) as usize;
            match find_join_with(a, b, hint, bands, max_px.max(LENIENT_MIN_PX)) {
                Err(NoJoin::NoOverlap(lenient)) => {
                    // A translucent sticky header and a floating button change whole stretches
                    // of lines, which no per-line tolerance covers: a last look that lets such
                    // stretches differ when everything else matches exactly.
                    if !robust {
                        return Err(NoJoin::NoOverlap(format!(
                            "{strict}; with up to {} differing pixels per line allowed: {lenient}",
                            max_px.max(LENIENT_MIN_PX)
                        )));
                    }
                    if same_but_for_a_region(a, b) {
                        return Err(NoJoin::Same);
                    }
                    let robust_result =
                        match find_join_robust(a, b, hint, bands, ROBUST_MIN_INK, false) {
                            // A shift the step names, on fewer lines (see `ROBUST_HINTED_MIN_INK`).
                            Err(NoJoin::NoOverlap(why)) => match hint {
                                Some(_) => {
                                    find_join_robust(a, b, hint, bands, ROBUST_HINTED_MIN_INK, true)
                                        .map_err(|e| match e {
                                            NoJoin::NoOverlap(_) => NoJoin::NoOverlap(why),
                                            other => other,
                                        })
                                }
                                None => Err(NoJoin::NoOverlap(why)),
                            },
                            other => other,
                        };
                    match robust_result {
                        Err(NoJoin::NoOverlap(robust)) => {
                            // A column that scrolls by another amount than the text (a table
                            // beside it at a fraction of a pixel, a card the pointer opened):
                            // the last look is at a band of columns that holds the text.
                            if let Some(j) = find_join_banded(a, b, hint, bands) {
                                return Ok(j);
                            }
                            Err(NoJoin::NoOverlap(format!(
                                "{strict}; with up to {} differing pixels per line allowed: {lenient}; with stretches of lines left out: {robust}; with a band of columns: no shift matches",
                                max_px.max(LENIENT_MIN_PX)
                            )))
                        }
                        other => other,
                    }
                }
                other => other,
            }
        }
        other => other,
    }
}

/// Most of the compared lines of a robust match that may differ.
const ROBUST_BAD_SHARE: f64 = 0.5;
/// Most separate stretches of differing lines (a translucent header, a floating button, a
/// scrollbar thumb).
const ROBUST_MAX_RUNS: usize = 3;
/// Lines that carry ink (not one flat colour) that must match exactly, and at least this many
/// times as many as the ink lines that differ.
const ROBUST_MIN_INK: usize = 40;
/// Ink lines that must match when the step names the shift (the learned wheel distance times the
/// asked step): the shift is looked for only near it and must be the only one that fits. For a
/// pair with little left in common: a translucent header at the top, a card that reveals itself
/// as it scrolls into view at the bottom, a few lines of text between (a real page, Windows
/// Edge, carchargeos.com, Nov 16, 2026: 85 matching rows of 354).
const ROBUST_HINTED_MIN_INK: usize = 20;
/// Most inked lines that may differ per inked line that matches, when the step names the shift.
const ROBUST_HINTED_BAD_PER_OK: usize = 4;
const ROBUST_INK_RATIO: usize = 1;

/// Whether a line is of one colour only (a blank stretch of page: it matches at every shift and
/// proves nothing).
fn flat(l: &[u8]) -> bool {
    let (first, rest) = l.split_at(4.min(l.len()));
    rest.as_chunks::<4>().0.iter().all(|p| p == first)
}

/// The last look at two frames that did not join: a sticky header that is opaque in one frame and
/// frosted over the scrolled page in the next (its rows differ in every frame pair), and a floating
/// button (scroll to top, chat) at one corner, each change **whole stretches of lines** by more
/// than a few pixels, so no per-line tolerance holds them (a real page, https://www.carchargeos.com,
/// Linux, Nov 16, 2026: `no_overlap` after the first frame).
///
/// A shift is believed when the lines that differ are few (at most [`ROBUST_BAD_SHARE`] of the
/// compared ones, in at most [`ROBUST_MAX_RUNS`] stretches) and every other line matches exactly,
/// over at least [`ROBUST_MIN_INK`] lines that carry ink, and over at least twice as many of those
/// as the ones that differ. Lines of one colour prove nothing and do not count. The best shift
/// has to beat the next clearly (or be the one nearest the expected step), else it is refused:
/// frames that only look alike (a list whose rows differ by a few pixels, a page that jumped)
/// have almost no exact lines carrying ink.
fn find_join_robust(
    a: &Lines,
    b: &Lines,
    hint: Option<usize>,
    bands: Option<(usize, usize)>,
    min_ink: usize,
    near_hint: bool,
) -> Result<Join, NoJoin> {
    let n = a.count;
    if a.data == b.data {
        return Err(NoJoin::Same);
    }
    let (header, footer) = match bands {
        Some((h, f)) => {
            if (0..h).any(|i| !a.same_line_tol(b, i, 0))
                || (n - f..n).any(|i| !a.same_line_tol(b, i, 0))
            {
                return Err(NoJoin::NoOverlap("a fixed band changed".into()));
            }
            (h, f)
        }
        None => {
            let mut h = 0;
            while h < n && a.same_line_tol(b, h, 0) {
                h += 1;
            }
            let mut f = 0;
            while f < n - h && a.same_line_tol(b, n - 1 - f, 0) {
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
        return Err(NoJoin::NoOverlap(
            "too little content between the bands".into(),
        ));
    }
    // (shift, ink lines that match, ink lines that differ)
    let mut found: Vec<(usize, usize, usize)> = Vec::new();
    // Only the shifts the step names: a quarter of it either way, as the tie-break below does.
    let window = match (near_hint, hint) {
        (true, Some(h)) => {
            let wide = (cw / 20).max(2).max(h / 4);
            h.saturating_sub(wide).max(1)..=(h + wide).min(cw - min_ov)
        }
        _ => 1..=(cw - min_ov),
    };
    for s in window {
        let overlap = cw - s;
        let g = GUARD.min(overlap / 3);
        let compared = overlap - 2 * g;
        if compared < min_ink {
            break;
        }
        let allowed = (compared as f64 * ROBUST_BAD_SHARE) as usize;
        // Lines that differ before the first one that matches are the top of the next frame
        // (a sticky header drawn over the page): they are left out and not counted, as the
        // overlap of a long step may be little more than the header.
        let (mut bad, mut runs, mut prev_bad, mut lead) = (0usize, 0usize, false, true);
        let (mut ink_ok, mut ink_bad) = (0usize, 0usize);
        let mut ok = true;
        for i in g..overlap - g {
            let (ia, ib) = (header + s + i, header + i);
            let inked = !flat(a.line(ia)) || !flat(b.line(ib));
            if a.near(ia, b, ib) {
                // A flat line (blank page) matches at any shift and does not end the header.
                if inked {
                    lead = false;
                }
                // When the step names the shift, a blank line that matches between differing
                // ones does not end the stretch (a card still drawing: its text lines differ,
                // the gaps between them are blank in both).
                if inked || !near_hint {
                    prev_bad = false;
                }
                if inked {
                    ink_ok += 1;
                }
            } else if lead {
                prev_bad = true;
            } else {
                bad += 1;
                if !prev_bad {
                    runs += 1;
                }
                prev_bad = true;
                if inked {
                    ink_bad += 1;
                }
                if bad > allowed || runs > ROBUST_MAX_RUNS {
                    ok = false;
                    break;
                }
            }
        }
        // The step names the shift: what differs may outweigh what matches (a header and a
        // card still drawing are most of a short overlap), but not by more than 4 to 1.
        let outweighed = if near_hint {
            ink_ok * ROBUST_HINTED_BAD_PER_OK < ink_bad
        } else {
            ink_ok < ROBUST_INK_RATIO * ink_bad
        };
        if ok && ink_ok >= min_ink && !outweighed {
            found.push((s, ink_ok, ink_bad));
        }
    }
    if found.is_empty() {
        return Err(NoJoin::NoOverlap(
            "no shift matches even with stretches of lines left out".into(),
        ));
    }
    found.sort_by_key(|c| std::cmp::Reverse(c.1));
    let top = found[0];
    let clear = found.get(1).is_none_or(|next| top.1 >= 2 * next.1);
    let shift = if clear {
        top.0
    } else if let Some(h) = hint {
        // Several shifts fit about as well (repeating content): the expected step may choose,
        // when exactly one candidate lies within a quarter of it.
        let wide = (cw / 20).max(2).max(h / 4);
        let mut near = found.iter().filter(|c| c.0.abs_diff(h) <= wide);
        match (near.next(), near.next()) {
            (Some(c), None) => c.0,
            _ => {
                return Err(NoJoin::NoOverlap(
                    "several shifts match with stretches left out and the step names none".into(),
                ))
            }
        }
    } else {
        return Err(NoJoin::NoOverlap(
            "several shifts match with stretches left out and no step is known".into(),
        ));
    };
    Ok(Join {
        shift,
        guard: GUARD.min((cw - shift) / 3),
        header,
        footer,
    })
}

/// Bands of columns (as shares of the width) tried by [`find_join_banded`]: the left two thirds,
/// the right two thirds, the middle and the two halves.
const COLUMN_BANDS: [(f64, f64); 5] =
    [(0.0, 0.65), (0.35, 1.0), (0.2, 0.8), (0.0, 0.5), (0.5, 1.0)];
/// Ink lines that must match exactly in a band (more than for all columns: less is compared).
const BANDED_MIN_INK: usize = 80;
/// A band has to hold moving content: at least this share of the content lines differ in it
/// between the two frames when they are not shifted (a fixed column matches at every shift).
const BANDED_MIN_MOVING: f64 = 0.25;
/// Share of the ink lines of a band's overlap that must differ from each other.
const BANDED_MIN_DISTINCT: f64 = 0.8;
/// The narrowest frame for which a band of columns means anything, in px.
const BANDED_MIN_WIDTH: usize = 200;

/// The last look at two frames that did not join: the join is looked for in a band of columns
/// only, and believed when every line of it matches exactly (a few stretches excepted, as in
/// [`find_join_robust`]) over at least [`BANDED_MIN_INK`] lines that carry ink, and that one shift
/// beats the others clearly. For pages where a column scrolls by a slightly different amount than
/// the text beside it (a real page, the Wikipedia infobox on a Retina Chrome window, Nov 16, 2026:
/// the text matched exactly at a shift of 489 lines and the table's glyph edges did not), or where
/// a hover card, a thumbnail preview or a floating panel changes a third of the width.
///
/// A band must hold moving content ([`BANDED_MIN_MOVING`]): fixed columns match at any shift and
/// cannot tell one from another, which made a wrong join (doubled text) when the columns that
/// differ were left out after a shift had been picked from the rest.
fn find_join_banded(
    a: &Lines,
    b: &Lines,
    hint: Option<usize>,
    bands: Option<(usize, usize)>,
) -> Option<Join> {
    let cross = a.line_len / 4;
    if cross < BANDED_MIN_WIDTH || a.count != b.count || a.line_len != b.line_len {
        return None;
    }
    let (header, footer) = bands.unwrap_or((0, 0));
    let n = a.count;
    if header + footer >= n {
        return None;
    }
    for (f0, f1) in COLUMN_BANDS {
        let (c0, c1) = ((cross as f64 * f0) as usize, (cross as f64 * f1) as usize);
        let (ac, bc) = (a.cropped(c0, c1), b.cropped(c0, c1));
        let moving = (header..n - footer)
            .filter(|&i| !ac.near(i, &bc, i) && (!flat(ac.line(i)) || !flat(bc.line(i))))
            .count();
        if (moving as f64) < BANDED_MIN_MOVING * (n - header - footer) as f64 {
            continue;
        }
        if let Ok(j) = find_join_robust(&ac, &bc, hint, bands, BANDED_MIN_INK, false) {
            // The lines the shift was believed on have to differ from each other: rows that
            // repeat (a list of alike rows, tinted stripes) match at many shifts, and the band
            // of columns that leaves out what tells them apart cannot say which is the true one.
            let mut seen = std::collections::HashSet::new();
            let mut ink = 0usize;
            for i in header + j.shift..n - footer {
                if !flat(ac.line(i)) {
                    ink += 1;
                    seen.insert(ac.hashes[i]);
                }
            }
            if ink >= BANDED_MIN_INK && seen.len() as f64 >= BANDED_MIN_DISTINCT * ink as f64 {
                return Some(j);
            }
        }
    }
    None
}

/// Rows between two differing stretches that still count as one.
const RUN_GAP: usize = 8;
/// Most separate stretches of differing lines a floating region may make (a card, a status bar).
const MAX_REGION_RUNS: usize = 3;

/// How many separate stretches the `true` entries form, stretches less than [`RUN_GAP`] apart
/// counting as one. Floating elements make few; content that only looks alike (a list, numbered
/// lines) makes one per line or per cell.
fn stretches(flags: &[bool]) -> usize {
    let (mut runs, mut since) = (0usize, usize::MAX);
    for &f in flags {
        if f {
            if since == usize::MAX || since > RUN_GAP {
                runs += 1;
            }
            since = 0;
        } else if since != usize::MAX {
            since += 1;
        }
    }
    runs
}

/// A column that differs in at least this share of the lines belongs to the region.
const REGION_COLUMN_SHARE: f64 = 0.01;
/// Lines that may still differ outside those columns (the antialiased edge of a status text).
const REGION_LEFT_SHARE: f64 = 0.06;
/// Most of the lines that may differ for [`same_but_for_a_region`].
const REGION_MAX_LINES: f64 = 0.6;
/// Share of the lines that carry ink in either frame that must be equal, unshifted, for two frames
/// to be one position drawn differently in a region (see [`same_but_for_a_region`]).
const REGION_MIN_INKED_EQUAL: f64 = 0.5;

/// Two frames of the same scroll position that differ only in a region: a link preview or a
/// status bar that the pointer opened (Firefox shows the address of the link under the pointer
/// at the bottom left, Wikipedia a card over a third of the page). Not a scroll: the lines that
/// differ are at most [`REGION_MAX_LINES`] of them (a scrolled region differs in all), in at most
/// [`MASK_MAX_COLUMNS`] of the columns, and outside those columns every line is the same.
fn same_but_for_a_region(a: &Lines, b: &Lines) -> bool {
    let cross = a.line_len / 4;
    if cross < 256 || a.count != b.count {
        return false;
    }
    let differs = |ia: usize, ib: usize, x: usize| {
        a.line(ia)[x * 4..x * 4 + 4]
            .iter()
            .zip(&b.line(ib)[x * 4..x * 4 + 4])
            .any(|(u, v)| u.abs_diff(*v) > NOISE_TOLERANCE)
    };
    let mut bad = vec![0usize; cross];
    let mut rows_differing = 0usize;
    let flags: Vec<bool> = (0..a.count).map(|i| !a.near(i, b, i)).collect();
    if stretches(&flags) > MAX_REGION_RUNS {
        return false;
    }
    for (i, _) in flags.iter().enumerate().filter(|(_, &f)| f) {
        rows_differing += 1;
        for (x, c) in bad.iter_mut().enumerate() {
            if differs(i, i, x) {
                *c += 1;
            }
        }
    }
    if rows_differing as f64 > REGION_MAX_LINES * a.count as f64 {
        return false;
    }
    // A scroll moves the inked rows: few of them are the same unshifted. A card or a status bar
    // leaves the page as it was, so most of its inked rows are equal. (A narrow column of text
    // with wide blank gaps differs in few stretches and few columns too, and was called the same
    // frame after a scroll: paulgraham.com, Windows Edge, Nov 16, 2026, `reached_end` after 12
    // frames of a page of more than 16,000 px.)
    let (mut inked, mut inked_equal) = (0usize, 0usize);
    for i in 0..a.count {
        if flat(a.line(i)) && flat(b.line(i)) {
            continue;
        }
        inked += 1;
        if a.near(i, b, i) {
            inked_equal += 1;
        }
    }
    if (inked_equal as f64) < REGION_MIN_INKED_EQUAL * inked as f64 {
        return false;
    }
    let limit = ((a.count as f64 * REGION_COLUMN_SHARE) as usize).max(3);
    let masked: Vec<bool> = bad.iter().map(|&c| c > limit).collect();
    if masked.iter().filter(|&&m| m).count() as f64 > MASK_MAX_COLUMNS * cross as f64 {
        return false;
    }
    let left = (0..a.count)
        .filter(|&i| !a.near(i, b, i) && (0..cross).any(|x| !masked[x] && differs(i, i, x)))
        .count();
    left as f64 <= REGION_LEFT_SHARE * a.count as f64
}

/// At most this share of the columns may belong to the region (see [`same_but_for_a_region`]).
const MASK_MAX_COLUMNS: f64 = 0.7;

/// Most candidates whose overlaps are compared pixel by pixel (see [`best_by_difference`]).
const MAX_SCORED_CANDIDATES: usize = 4096;
/// The runner-up must differ by at least this share of the best (plus [`SCORE_MARGIN_PX`]).
const SCORE_MARGIN_SHARE: f64 = 0.1;
/// ... and by this many pixels per compared line.
const SCORE_MARGIN_PX: f64 = 0.5;

/// Among shifts that all match within the tolerance, the one whose overlap has the fewest
/// differing pixels per line, when the next one has clearly more. `rival` is a score (pixels per
/// line) the best one must also beat (the frames as they are, no scroll); infinity for none.
/// The first is `None` on a tie or when there are too many to compare; the second is the best
/// score found (infinity for none), which tells a tie of good shifts from no good shift.
fn best_by_difference(
    a: &Lines,
    b: &Lines,
    header: usize,
    cw: usize,
    candidates: &[usize],
    rival: f64,
) -> (Option<(usize, f64, usize)>, f64) {
    if candidates.len() > MAX_SCORED_CANDIDATES {
        return (None, f64::INFINITY);
    }
    // (shift, pixels per line, compared lines) of every candidate; a candidate that is already
    // beyond what could still tie with the best so far is dropped without its full count.
    let mut scores: Vec<(usize, f64, usize)> = Vec::with_capacity(candidates.len());
    let mut best = rival;
    for &s in candidates {
        let overlap = cw - s;
        let g = GUARD.min(overlap / 3);
        let lines = (overlap - 2 * g).max(1);
        let cap = (best + best * SCORE_MARGIN_SHARE + SCORE_MARGIN_PX) * lines as f64;
        let mut total = 0usize;
        for i in g..overlap - g {
            total += a.diff_px(header + s + i, b, header + i);
            if total as f64 > cap {
                break;
            }
        }
        let score = total as f64 / lines as f64;
        best = best.min(score);
        scores.push((s, score, lines));
    }
    let found = scores.iter().map(|c| c.1).fold(f64::INFINITY, f64::min);
    let margin = best * SCORE_MARGIN_SHARE + SCORE_MARGIN_PX;
    // Everything that cannot be told from the best (the frames as they are count too: no scroll).
    let tied: Vec<&(usize, f64, usize)> = scores.iter().filter(|c| c.1 < best + margin).collect();
    if !best.is_finite() || rival < best + margin {
        return (None, found);
    }
    let pick = match tied.as_slice() {
        [one] => Some(**one),
        // Several shifts fit equally well, but one of them over far more lines than any other:
        // an overlap of a few lines that happens to match is little evidence (a terminal's
        // numbered lines, nearly blank rows), a long overlap that matches is a lot. At least
        // twice the lines of the next, and enough of them to mean something.
        many => {
            let mut by_lines: Vec<&&(usize, f64, usize)> = many.iter().collect();
            by_lines.sort_by_key(|c| std::cmp::Reverse(c.2));
            let (top, next) = (by_lines[0], by_lines[1]);
            (top.2 >= LONG_OVERLAP_LINES && top.2 >= 2 * next.2).then_some(**top)
        }
    };
    (pick, found)
}

/// Compared lines a match needs before the expected step may choose it among several equal ones.
const MIN_TIE_LINES: usize = 12;
/// Compared lines a matching overlap needs before it outweighs shorter ones that match as well.
const LONG_OVERLAP_LINES: usize = 30;

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
    let (scored, best_score) = if (candidates.len() > 1 || rival.is_finite()) && max_px > 0 {
        best_by_difference(a, b, header, cw, &candidates, rival)
    } else {
        (None, f64::INFINITY)
    };
    // The best shift is far short of the expected one and its overlap is not an exact match: when
    // the wheel overshot, the true overlap is a few lines (too few to be a candidate) and the
    // shift that fits least badly is a line of text matched with another that differs in a digit.
    // Refused, so that the run scrolls back and looks again; never believed.
    if let (Some((s, score, lines)), Some(h)) = (scored, hint) {
        let wide = (cw / 20).max(2).max(h / 8);
        let exact = score <= SCORE_MARGIN_PX && lines >= LONG_OVERLAP_LINES;
        if s + wide < h && !exact {
            return Err(NoJoin::NoOverlap(format!(
                "the best shift ({s}) is far from the expected {h} and does not match exactly"
            )));
        }
    }
    let scored = scored.map(|c| c.0);
    if rival.is_finite() && scored.is_none() {
        // A blink and nothing else, unless several shifts explain the difference equally well
        // and clearly better than no shift does: the page scrolled and the overlap is too
        // little or too alike to tell where (a terminal's numbered lines). That is not the end
        // of the content, and it must not be believed to be.
        if best_score + best_score * SCORE_MARGIN_SHARE + SCORE_MARGIN_PX >= rival {
            return Err(NoJoin::Same);
        }
        return Err(NoJoin::NoOverlap(format!(
            "{} shifts explain the change equally well (a terminal's numbered lines, or too little overlap)",
            candidates.len()
        )));
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
            // Lines of the overlap that would be compared at a shift: a match over a handful of
            // lines is no evidence when several shifts match, so the step does not choose among
            // those (the run looks again, scrolled back by half a step).
            let compared = |s: usize| {
                let overlap = cw - s;
                overlap - 2 * GUARD.min(overlap / 3)
            };
            let mut by_dist: Vec<(usize, usize)> = candidates
                .iter()
                .filter(|&&s| compared(s) >= MIN_TIE_LINES)
                .map(|&s| (s.abs_diff(h), s))
                .collect();
            if by_dist.is_empty() {
                return Err(NoJoin::NoOverlap(
                    "several shifts match and only over a few lines each".into(),
                ));
            }
            by_dist.sort_unstable();
            let (d0, s0) = by_dist[0];
            let tol = (cw / 20).max(2);
            if d0 <= tol && by_dist.get(1).is_none_or(|b| b.0 > d0 + tol) {
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
    // A match that only holds because lines may differ in a few pixels is believed only when
    // those lines are few: a caret, a clock or a cursor changes a handful of lines, while
    // content that is merely alike (rows 28 apart in a list, whose bars differ by 4 px and
    // whose numbers by two digits) differs in every line that holds ink. Such a pair did not
    // overlap at all (a page that jumped) and was joined, silently leaving rows out (Linux QA,
    // Nov 16, 2026).
    if max_px > 0 {
        let overlap = cw - shift;
        let g = GUARD.min(overlap / 3);
        let compared = (overlap - 2 * g).max(1);
        let inexact = (g..overlap - g)
            .filter(|&i| !a.near(header + shift + i, b, header + i))
            .count();
        if inexact as f64 > compared as f64 * LENIENT_INEXACT_SHARE {
            return Err(NoJoin::NoOverlap(format!(
                "the match at shift {shift} needs the pixel tolerance on {inexact} of {compared} lines: the frames only look alike"
            )));
        }
    }
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
    // A row in which more columns differ than the two strips can hold is a stretch that changes as a whole (a translucent
    // sticky header that is frosted over the scrolled page, a floating banner), not a moving
    // edge: it would make every column look volatile and the strips as wide as they can be, so
    // it is left out of the tally.
    let rows: Vec<(usize, usize)> = rows
        .into_iter()
        .filter(|&(ia, ib)| {
            let (la, lb) = (a.line(ia), b.line(ib));
            let differing = (0..cross)
                .filter(|&x| {
                    la[x * 4..x * 4 + 4]
                        .iter()
                        .zip(&lb[x * 4..x * 4 + 4])
                        .any(|(u, v)| u.abs_diff(*v) > NOISE_TOLERANCE)
                })
                .count();
            differing <= max.0 + max.1 + WIDER_THAN_EDGES
        })
        .collect();
    let limit = (rows.len() as f64 * VOLATILE_SHARE) as usize;
    let mut bad = vec![0usize; cross];
    for (ia, ib) in rows {
        let (la, lb) = (a.line(ia), b.line(ib));
        for (x, c) in bad.iter_mut().enumerate() {
            if la[x * 4..x * 4 + 4]
                .iter()
                .zip(&lb[x * 4..x * 4 + 4])
                .any(|(u, v)| u.abs_diff(*v) > NOISE_TOLERANCE)
            {
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
    let bad = changed_columns(a, b);
    let limit = static_limit(a.count);
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

/// How many lines differ (beyond the noise) in each column between two frames of the same size.
fn changed_columns(a: &Lines, b: &Lines) -> Vec<usize> {
    let cross = a.line_len / 4;
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
    bad
}

fn static_limit(count: usize) -> usize {
    ((count as f64 * STATIC_SHARE) as usize).max(2)
}

/// The columns a join of two vertical frames has to leave out of the comparison when the page
/// has fixed side content (a sidebar, a panel, a margin), as candidate masks `(left, right)`,
/// the most likely first. A fixed bar matches at any shift, so no shift can be told from it.
///
/// What [`static_strips`] cannot see: a scrollbar (or a window border) at the very end of the
/// frame, outside the bar, moves while the page scrolls, so the unbroken run of unchanging
/// columns from that end is 0 and the bar behind it was not found (Windows, Edge, a page with
/// fixed bars on both sides: the left one was found and the right one made every join fail, QA
/// pass 6). Here each end may start with a stretch of moving columns (at most as wide as the
/// widest edge strip) before the run of unchanging columns, and the mask covers both. A bar
/// that casts a shadow over the scrolling page has columns next to it that change: the masks
/// are also tried a little wider (`SHADOW_STEPS`). At least [`MIN_COMPARED_COLS`] columns must
/// stay in the comparison, else there is no mask (everything is compared).
fn masked_sides(a: &Lines, b: &Lines) -> Vec<(usize, usize)> {
    let cross = a.line_len / 4;
    if cross < 2 * MIN_COMPARED_COLS || a.count != b.count {
        return Vec::new();
    }
    let bad = changed_columns(a, b);
    let limit = static_limit(a.count);
    let is_static = |x: usize| bad[x] <= limit;
    // The end's mask: moving columns up to `gap_max`, then a run of unchanging ones of at least
    // `MIN_BAR_COLS` (unless there is no gap, when any run counts). `None` when there is none.
    let side = |from_end: bool, gap_max: usize| -> Option<usize> {
        let at = |k: usize| if from_end { cross - 1 - k } else { k };
        let mut gap = 0;
        while gap < gap_max && gap < cross && !is_static(at(gap)) {
            gap += 1;
        }
        let run = (gap..cross).take_while(|&k| is_static(at(k))).count();
        let min = if gap == 0 { 1 } else { MIN_BAR_COLS };
        (run >= min && gap < gap_max).then_some(gap + run)
    };
    let (l, r) = (side(false, MAX_LEFT_STRIP), side(true, MAX_RIGHT_STRIP));
    let mut out = Vec::new();
    for ext in SHADOW_STEPS {
        let l = l.map(|l| l + ext).unwrap_or(0);
        let r = r.map(|r| r + ext).unwrap_or(0);
        if (l, r) == (0, 0) || cross.saturating_sub(l + r) < MIN_COMPARED_COLS {
            continue;
        }
        if !out.contains(&(l, r)) {
            out.push((l, r));
        }
    }
    out
}

/// Fewest columns that stay in the comparison when static side columns are masked: a join over
/// fewer is a guess (a few columns of text match many shifts).
const MIN_COMPARED_COLS: usize = 24;
/// A run of unchanging columns behind a stretch of moving ones must be at least this wide to
/// be a bar (not a few columns of a flat page that happen to stay).
const MIN_BAR_COLS: usize = 8;
/// Extra columns tried next to a bar for its shadow or border, which blend with what scrolls
/// under them.
const SHADOW_STEPS: [usize; 4] = [0, 4, 12, 32];

/// The share of the lines in which a row (column) of a static strip may differ between two
/// frames (a blinking caret, a loading spinner in the toolbar).
const STATIC_SHARE: f64 = 0.02;

/// The columns at each end of a vertical capture that are left out of the comparison when a pair
/// does not join and an overlay scrollbar may be drawn over them (macOS: a thumb that fades in
/// and out over the page's edge and over a fixed edge strip, 14 to 28 physical px at 2x).
const OVERLAY_EDGE_LEFT: usize = 16;
const OVERLAY_EDGE_RIGHT: usize = 32;

/// The outermost columns (per side) that are looked at for the window's rounded corners when the
/// static columns of a vertical capture are not part of the join (see [`FullSides`]).
const SIDE_CORNER_COLS: usize = 96;

/// A vertical capture whose first pair needed its static side columns left out of the comparison
/// (a fixed sidebar, a page's margin: they match at any shift, so no shift can be told from them).
/// The columns are only a **mask**: the join is found on the columns between them, but every row of
/// the image is composed from the whole frame that supplies it, side columns included. A fixed
/// sidebar then appears once per slice like anything that scrolls, which is what a scrolling
/// capture of such a page is. (Until Nov 15, 2026 the columns were cut and the first frame's were
/// put back, repeating its last row below it: the window's rounded corner at the bottom of that
/// row was smeared down both sides of the image, QA macOS pass 4, LS-1.)
struct FullSides {
    /// Static columns at the start and at the end of every row, left out of the comparison.
    l: usize,
    r: usize,
    /// The last accepted frame with all its columns.
    prev: Lines,
}

/// How many rows at the end of the last accepted frame (`prev`) differ, in the outermost columns
/// of the cut sides, from the same page row of `next` (`join.shift` rows on): the window's
/// rounded corners lie in the static columns, which the join does not compare, so the corner
/// depth of the joined columns (see [`corner_depth`]) cannot see them. A fixed element at the
/// edge also differs and only makes the replaced stretch longer, which is safe: the next frame
/// has those rows.
fn side_corner_depth(prev: &Lines, next: &Lines, join: &Join, l: usize, r: usize) -> usize {
    let cross = prev.line_len / 4;
    let n = prev.count;
    let overlap = n - join.header - join.footer - join.shift;
    let max = (overlap / 2).min(MAX_CORNER_DEPTH);
    let (wl, wr) = (l.min(SIDE_CORNER_COLS), r.min(SIDE_CORNER_COLS));
    let same = |p: usize| {
        let (a, b) = (prev.line(p), next.line(p - join.shift));
        let close = |x0: usize, x1: usize| {
            a[x0 * 4..x1 * 4]
                .iter()
                .zip(&b[x0 * 4..x1 * 4])
                .all(|(u, v)| u.abs_diff(*v) <= NOISE_TOLERANCE)
        };
        close(0, wl) && close(cross - wr, cross)
    };
    let (mut depth, mut run) = (0, 0);
    for d in 0..max {
        let p = n - join.footer - 1 - d;
        if same(p) {
            run += 1;
            if run >= CORNER_END_RUN {
                break;
            }
        } else {
            run = 0;
            depth = d + 1;
        }
    }
    depth
}

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
    /// A vertical capture whose first pair needed static columns left out of the comparison
    /// (a fixed sidebar, a page's margin): the image is composed from whole frames (see
    /// [`FullSides`]).
    full: Option<FullSides>,
    header: Vec<u8>,
    content: Vec<u8>,
    footer: Vec<u8>,
    frames: u32,
    /// Lines in the image so far (header + content + footer).
    len: usize,
    limit_hit: bool,
    /// Why the last frame was refused (see [`Stitcher::last_refusal`]).
    refusal: Option<String>,
    /// The rows the last joined frame put into the content: `(first row of the frame, one past
    /// the last)`, for [`Stitcher::refresh_last`].
    last_add: Option<(usize, usize)>,
}

#[cfg(test)]
impl Stitcher {
    /// Test only: the state a run is in after a first pair that joined on the whole width.
    pub(super) fn assume_whole_width(&mut self) {
        self.trim = Some((0, 0));
    }
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
            full: None,
            header: Vec::new(),
            content: Vec::new(),
            footer: Vec::new(),
            frames: 0,
            len: 0,
            limit_hit: false,
            refusal: None,
            last_add: None,
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

    /// Whether `frame` is not the size of the first one: the window was resized (or the capture
    /// changed what it covers), so no join can be asked of it.
    pub fn size_differs(&self, frame: &Frame) -> bool {
        self.size
            .is_some_and(|s| s != (frame.width(), frame.height()))
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

    /// Takes a later look at the position of the last joined frame in place of that frame: the
    /// page was still drawing when the frame was taken (a heading that reveals itself when it
    /// scrolls into view: QA macOS, carchargeos.com, the top half of a heading missing at a seam).
    /// `frame` must be the same size, at the same scroll position (it differs from the last frame
    /// in at most a few stretches of lines, not as a scroll does). The rows that frame put into
    /// the image are replaced by the same rows of `frame`. Returns whether anything changed.
    pub fn refresh_last(&mut self, frame: &Frame) -> bool {
        let Some(prev) = &self.prev else {
            return false;
        };
        if self.limit_hit || self.size != Some((frame.width(), frame.height())) {
            return false;
        }
        let whole = Lines::from_frame(frame, self.dir);
        let mut lines = whole.clone();
        if let Some((l, r)) = self.trim {
            if (l, r) != (0, 0) {
                let cross = lines.line_len / 4;
                lines = lines.cropped(l, cross - r);
            }
        }
        if lines.line_len != prev.line_len || lines.data == prev.data {
            return false;
        }
        // The same position: a few stretches differ, never a scroll (which changes most lines).
        let flags: Vec<bool> = (0..lines.count).map(|i| !prev.near(i, &lines, i)).collect();
        let differing = flags.iter().filter(|&&f| f).count();
        if differing == 0
            || differing as f64 > REGION_MAX_LINES * lines.count as f64
            || stretches(&flags) > MAX_REGION_RUNS
        {
            return false;
        }
        let src = match &self.full {
            Some(_) => &whole,
            None => &lines,
        };
        if self.frames >= 2 {
            let (Some((from, to)), line_len) = (self.last_add, src.line_len) else {
                return false;
            };
            let (header, footer) = self.bands.unwrap_or((0, 0));
            let have = self.content.len() / line_len;
            if to > src.count || to < from || to - from > have || footer > src.count - header {
                return false;
            }
            self.content.truncate((have - (to - from)) * line_len);
            self.content.extend_from_slice(src.range(from, to));
            self.footer = src.range(src.count - footer, src.count).to_vec();
        }
        if let Some(f) = self.full.as_mut() {
            f.prev = whole;
        }
        self.prev = Some(lines);
        true
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
        // The whole frame, for a vertical capture that may leave its static side columns out of
        // the comparison (the first pair has not been looked at yet, or it has).
        let whole = (self.dir == Direction::Vertical
            && (self.trim.is_none() || self.full.is_some()))
        .then(|| lines.clone());
        let (prev, lines) = self.apply_trim(prev, lines, hint);
        let mut full = self.full.take();
        let pair = full.as_ref().zip(whole.as_ref());
        let outcome = self.join(&prev, &lines, size, hint, pair);
        // A join that fails after the first pair: an overlay scrollbar that was not there in the
        // first pair (or that was narrower) draws a thumb over the outermost columns, which then
        // differ in every line it covers, in the content and in the fixed bands alike (QA macOS
        // pass 6, LS-9: "a fixed band changed", "no shift matches"). The outermost columns are
        // left out of the comparison and the image is composed from whole frames, as for static
        // side columns.
        if outcome == PushOutcome::NoOverlap {
            if let Some((o, l, r, n)) =
                self.join_masked(&prev, &lines, full.as_ref(), whole.as_ref(), size, hint)
            {
                self.trim = Some((l, r));
                self.full = Some(FullSides {
                    l,
                    r,
                    prev: whole.unwrap_or_else(|| lines.clone()),
                });
                self.prev = Some(n);
                return o;
            }
        }
        if outcome == PushOutcome::NoOverlap && super::debug::enabled() {
            // For the dump: which state of the run the pair needs. A fresh look that joins the
            // pair where the run did not names the culprit (hint, bands or the mask).
            let fresh = |hint: Option<usize>, bands: Option<(usize, usize)>| match find_join(
                &prev, &lines, hint, bands,
            ) {
                Ok(j) => format!(
                    "joins at shift {} (header {}, footer {})",
                    j.shift, j.header, j.footer
                ),
                Err(NoJoin::Same) => "same frame".to_string(),
                Err(NoJoin::NoOverlap(_)) => "no".to_string(),
            };
            let note = format!(
                "; run state: trim {:?}, full mask {:?}, bands {:?}, hint {:?}; same pair with the hint left out: {}; with the bands left out: {}; with both left out: {}",
                self.trim,
                self.full.as_ref().map(|f| (f.l, f.r)).or(full.as_ref().map(|f| (f.l, f.r))),
                self.bands,
                hint,
                fresh(None, self.bands),
                fresh(hint.map(|h| h as usize), None),
                fresh(None, None),
            );
            if let Some(r) = self.refusal.as_mut() {
                r.push_str(&note);
            }
        }
        let added = matches!(outcome, PushOutcome::Added { .. });
        if let (Some(f), Some(w)) = (full.as_mut(), whole) {
            if added {
                f.prev = w;
            }
        }
        self.full = full;
        // The newest accepted frame is the one the next frame is matched against.
        self.prev = Some(if added { lines } else { prev });
        outcome
    }

    /// [`Stitcher::push`]'s second look at a pair that did not join: with at least
    /// [`OVERLAY_EDGE_LEFT`] and [`OVERLAY_EDGE_RIGHT`] columns at the two ends left out of the
    /// comparison. Only for a vertical capture whose frames are whole (no trim yet, or a mask of
    /// static side columns that can be widened). Returns the outcome (when the pair joined) with
    /// the new mask and the compared lines of both frames.
    fn join_masked(
        &mut self,
        prev: &Lines,
        lines: &Lines,
        full: Option<&FullSides>,
        whole: Option<&Lines>,
        size: (u32, u32),
        hint: Option<u32>,
    ) -> Option<(PushOutcome, usize, usize, Lines)> {
        let cross = lines.line_len / 4;
        if self.dir != Direction::Vertical || self.limit_hit || self.size != Some(size) {
            return None;
        }
        let (l0, r0, whole_prev, whole_next) = match (full, whole) {
            (Some(f), Some(w)) => (f.l, f.r, f.prev.clone(), w.clone()),
            _ if self.trim == Some((0, 0)) => (0, 0, prev.clone(), lines.clone()),
            _ => return None,
        };
        if whole_prev.line_len != whole_next.line_len || cross < 4 * (OVERLAY_EDGE_LEFT + 16) {
            return None;
        }
        // The masks to try, the overlay scrollbar's edges first, then the side columns that stay
        // put in this pair (a sidebar, a panel, a contents list that the page pinned after the
        // first pair was looked at: a fresh stitcher finds them from the pair, a run that kept
        // the mask of its first pair did not: Linux, Firefox, Wikipedia and YouTube, Nov 16,
        // 2026, `no_overlap` mid-page). Never narrower than the mask the run has.
        let mut masks = vec![(l0.max(OVERLAY_EDGE_LEFT), r0.max(OVERLAY_EDGE_RIGHT))];
        // Only when no shift matched at all: a pair that was refused because several shifts match
        // or the overlap is too little or too alike (a terminal's numbered lines, a page that
        // overshot) is not made joinable by leaving columns out, which only makes it more alike.
        let ambiguous = self.refusal.as_deref().is_some_and(|r| {
            [
                "shifts match",
                "shifts explain",
                "several shifts",
                "far from the expected",
                "only look alike",
            ]
            .iter()
            .any(|k| r.contains(k))
        });
        let sides = if ambiguous {
            Vec::new()
        } else {
            masked_sides(&whole_prev, &whole_next)
        };
        for (sl, sr) in sides {
            let m = (l0.max(sl), r0.max(sr));
            if !masks.contains(&m) {
                masks.push(m);
            }
        }
        for (l, r) in masks {
            if (l, r) == (l0, r0) || cross.saturating_sub(l + r) < MIN_COMPARED_COLS {
                continue;
            }
            let pc = whole_prev.cropped(l, cross - r);
            let nc = whole_next.cropped(l, cross - r);
            let fs = FullSides {
                l,
                r,
                prev: whole_prev.clone(),
            };
            let o = self.join(&pc, &nc, size, hint, Some((&fs, &whole_next)));
            if matches!(
                o,
                PushOutcome::Added { .. } | PushOutcome::LimitReached { .. }
            ) {
                return Some((o, l, r, nc));
            }
        }
        None
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
        match find_join_opt(&prev, &lines, hint, None, false) {
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
        // A vertical capture masks its fixed side content (a scrollbar may stand outside it, a
        // shadow beside it: see `masked_sides`); a horizontal one cuts the static rows.
        let candidates = if self.dir == Direction::Vertical {
            masked_sides(&prev, &lines)
        } else {
            let (sl, sr) = static_strips(&prev, &lines);
            if (sl, sr) == (0, 0) {
                Vec::new()
            } else {
                vec![(sl, sr)]
            }
        };
        let mut found = None;
        for (sl, sr) in candidates {
            let (p0, n0) = (prev.cropped(sl, cross - sr), lines.cropped(sl, cross - sr));
            if find_join(&p0, &n0, hint, None).is_ok() {
                found = Some((sl, sr));
            } else if let Some((l, r)) = edge_trim(&p0, &n0, hint) {
                found = Some((sl + l, sr + r));
            }
            if found.is_some() {
                break;
            }
        }
        let Some((l, r)) = found else {
            // No edge or side column explains it: the whole width, with stretches of lines that
            // differ (a translucent header, a floating button) left out of the comparison.
            if find_join(&prev, &lines, hint, None).is_ok() {
                self.trim = Some((0, 0));
            }
            return (prev, lines);
        };
        self.trim = Some((l, r));
        // Columns left out only so that the frames could join are content of a vertical capture
        // (a sidebar that stays, the white margin of a page: Chrome, macOS, Nov 15, 2026, the
        // left 150 to 300 px of two captures were missing). They only mask the comparison; the
        // image is composed from whole frames (see [`FullSides`]).
        let cropped = (prev.cropped(l, cross - r), lines.cropped(l, cross - r));
        if self.dir == Direction::Vertical {
            self.full = Some(FullSides { l, r, prev });
        }
        cropped
    }

    fn join(
        &mut self,
        prev: &Lines,
        lines: &Lines,
        size: (u32, u32),
        hint: Option<u32>,
        full: Option<(&FullSides, &Lines)>,
    ) -> PushOutcome {
        // The rows the image is composed from: whole frames when static side columns were left
        // out of the comparison, else the compared lines themselves.
        let (src_prev, src) = match full {
            Some((f, w)) => (&f.prev, w),
            None => (prev, lines),
        };
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
        let line_len = src.line_len;
        let join = match find_join(prev, lines, hint, self.bands) {
            Ok(j) => j,
            Err(NoJoin::Same) => return PushOutcome::NoChange,
            Err(NoJoin::NoOverlap(why)) => {
                self.refusal = Some(why);
                // A band that went away (a sticky header that hides while the page scrolls
                // down): the bands are found again from this pair, and only narrower ones are
                // accepted. The rows that were band become content.
                let again = find_join(prev, lines, hint, None);
                if let (Some(_), Err(NoJoin::NoOverlap(why2))) = (self.bands, &again) {
                    if let Some(r) = self.refusal.as_mut() {
                        r.push_str(&format!("; with the bands found again: {why2}"));
                    }
                }
                let (Some((h, f)), Ok(mut j)) = (self.bands, again) else {
                    return PushOutcome::NoOverlap;
                };
                // Rows at the bottom that are equal in this pair only (blank page, the window's
                // edge) are not a footer the capture had: the join is looked for again with the
                // footer it had (docs.rs in Firefox, Nov 16, 2026: a header band of 133 rows that
                // reached into blank page rows, then a pair whose bottom 14 rows were equal).
                if j.header <= h && j.footer > f {
                    match find_join(prev, lines, hint, Some((j.header, f))) {
                        Ok(j2) => j = j2,
                        Err(_) => return PushOutcome::NoOverlap,
                    }
                }
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
            self.header = src_prev.range(0, join.header).to_vec();
            self.content = src_prev.range(join.header, n - join.footer).to_vec();
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
        let side = full.map_or(0, |(f, w)| side_corner_depth(&f.prev, w, &join, f.l, f.r));
        let guard = join
            .guard
            .max(corner_depth(prev, lines, &join))
            .max(side)
            .min(cw - join.shift)
            .min(self.content.len() / line_len);
        let join = Join { guard, ..join };
        let keep = self.content.len() - join.guard * line_len;
        if total <= room {
            self.content.truncate(keep);
            self.content
                .extend_from_slice(src.range(from - join.guard, from + join.shift));
            self.last_add = Some((from - join.guard, from + join.shift));
            self.footer = src.range(n - join.footer, n).to_vec();
            self.len = total;
            self.frames += 1;
            return PushOutcome::Added {
                new_len: join.shift as u32,
            };
        }
        // Over the limit: fill up to it, drop the footer (the content under it was cut off).
        self.last_add = None;
        let take = room.saturating_sub(have).min(join.shift);
        self.content.truncate(keep);
        self.content
            .extend_from_slice(src.range(from - join.guard, from + take));
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
        // The last accepted frame as it is stored: with its static side columns when the image is
        // composed from whole frames.
        let prev = self.full.as_ref().map(|f| &f.prev).or(self.prev.as_ref())?;
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
        let (Some(prev), Some(_)) = (
            self.full.as_ref().map(|f| &f.prev).or(self.prev.as_ref()),
            self.size,
        ) else {
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

#[cfg(test)]
mod region_tests {
    use super::*;

    /// Short paragraphs in a narrow column with wide blank gaps, the page at `pos`.
    fn sparse(pos: usize, status_bar: bool) -> Frame {
        let (w, h) = (600usize, 300usize);
        let mut data = Vec::with_capacity(w * h * 4);
        for i in 0..h {
            let y = pos + i;
            let mut line = vec![255u8; w * 4];
            if y % 200 < 60 {
                for x in 100..340 {
                    let v = ((y * 31 + x * 17) ^ (x >> 1)) % 251;
                    line[x * 4..x * 4 + 4].copy_from_slice(&[v as u8, (v / 2) as u8, 20, 255]);
                }
            }
            if status_bar && i >= 280 {
                for x in 0..200 {
                    line[x * 4..x * 4 + 4].copy_from_slice(&[10, 10, 10, 255]);
                }
            }
            data.extend_from_slice(&line);
        }
        Frame::new(w as u32, h as u32, data).unwrap()
    }

    /// Windows Edge, paulgraham.com (Nov 16, 2026): a scroll of 200 px on a sparse text page
    /// differs in two stretches of rows and in the columns of the text only, like a card; it is
    /// not "the same frame". A status bar over the same position still is.
    #[test]
    fn lsreal_a_scroll_of_a_sparse_page_is_not_one_position_with_a_region() {
        let at = |pos, bar| Lines::from_frame(&sparse(pos, bar), Direction::Vertical);
        assert!(!same_but_for_a_region(&at(0, false), &at(200, false)));
        assert!(same_but_for_a_region(&at(0, false), &at(0, true)));
    }
}
