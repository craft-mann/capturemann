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
}

impl EndReason {
    /// The English note shown with the saved image (the UI translates the same strings).
    pub fn note(self) -> Option<&'static str> {
        match self {
            EndReason::NoOverlap => Some("Stopped: the next part did not line up"),
            EndReason::LimitReached => Some("Reached the longest image CaptureMann can edit"),
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

    fn same_line(&self, other: &Lines, i: usize) -> bool {
        self.hashes[i] == other.hashes[i] && self.line(i) == other.line(i)
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
    NoOverlap,
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

fn find_join(
    a: &Lines,
    b: &Lines,
    hint: Option<usize>,
    bands: Option<(usize, usize)>,
) -> Result<Join, NoJoin> {
    let n = a.count;
    if a.data == b.data {
        return Err(NoJoin::Same);
    }
    let (header, footer) = match bands {
        Some((h, f)) => {
            // A header or footer that changed since the first pair: do not trust the join.
            if (0..h).any(|i| !a.same_line(b, i)) || (n - f..n).any(|i| !a.same_line(b, i)) {
                return Err(NoJoin::NoOverlap);
            }
            (h, f)
        }
        None => {
            let mut h = 0;
            while h < n && a.same_line(b, h) {
                h += 1;
            }
            let mut f = 0;
            while f < n - h && a.same_line(b, n - 1 - f) {
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
        return Err(NoJoin::NoOverlap);
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
            if a.hashes[ia] != b.hashes[ib] || a.line(ia) != b.line(ib) {
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
    let shift = match (candidates.len(), hint) {
        (0, _) => return Err(NoJoin::NoOverlap),
        (1, _) => candidates[0],
        (_, None) => return Err(NoJoin::NoOverlap),
        (_, Some(h)) => {
            // Several equally good overlaps (repeating or blank content): the expected step may
            // choose, but only when it names one candidate clearly.
            let mut by_dist: Vec<(usize, usize)> =
                candidates.iter().map(|&s| (s.abs_diff(h), s)).collect();
            by_dist.sort_unstable();
            let (d0, s0) = by_dist[0];
            let tol = (cw / 20).max(2);
            if d0 > tol || by_dist[1].0 <= d0 + tol {
                return Err(NoJoin::NoOverlap);
            }
            s0
        }
    };
    Ok(Join {
        shift,
        guard: GUARD.min((cw - shift) / 3),
        header,
        footer,
    })
}

/// The widths of the strips at the start and the end of every line that hold columns which
/// differ between the two frames once they are aligned by `core` (a join found on the middle of
/// the lines). `None` when no column differs (the difference is not at the edges).
fn edge_strips(a: &Lines, b: &Lines, core: &Join, max: (usize, usize)) -> Option<(usize, usize)> {
    let n = a.count;
    let cross = a.line_len / 4;
    let overlap = n - core.header - core.footer - core.shift;
    let limit = (overlap as f64 * VOLATILE_SHARE) as usize;
    let mut bad = vec![0usize; cross];
    for i in core.guard..overlap - core.guard {
        let (la, lb) = (
            a.line(core.header + core.shift + i),
            b.line(core.header + i),
        );
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
        }
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
            Err(NoJoin::NoOverlap) => {}
        }
        // Too narrow to have edges to cut.
        if cross < 64 {
            return (prev, lines);
        }
        // The join is first looked for without the widest strips an edge can be (a scrollbar and
        // a window border), which holds the content that matters and none of the moving edges.
        let (ml, mr) = (
            (cross / 8).min(MAX_LEFT_STRIP),
            (cross / 8).min(MAX_RIGHT_STRIP),
        );
        let (c0, c1) = (ml, cross - mr);
        let core = find_join(&prev.cropped(c0, c1), &lines.cropped(c0, c1), hint, None);
        let Ok(core) = core else {
            return (prev, lines);
        };
        let Some((l, r)) = edge_strips(&prev, &lines, &core, (ml, mr)) else {
            return (prev, lines);
        };
        let (p, n) = (prev.cropped(l, cross - r), lines.cropped(l, cross - r));
        if find_join(&p, &n, hint, None).is_ok() {
            self.trim = Some((l, r));
            (p, n)
        } else {
            (prev, lines)
        }
    }

    fn join(
        &mut self,
        prev: &Lines,
        lines: &Lines,
        size: (u32, u32),
        hint: Option<u32>,
    ) -> PushOutcome {
        if self.limit_hit || self.size != Some(size) {
            return PushOutcome::NoOverlap;
        }
        let join = match find_join(prev, lines, hint.map(|h| h as usize), self.bands) {
            Ok(j) => j,
            Err(NoJoin::Same) => return PushOutcome::NoChange,
            Err(NoJoin::NoOverlap) => return PushOutcome::NoOverlap,
        };
        let n = lines.count;
        let line_len = lines.line_len;
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
