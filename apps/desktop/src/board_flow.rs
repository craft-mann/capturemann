//! Boards (BRD-1, design board S10): the state and the words of the "Make a board" sheet. Pure:
//! no windows here, so it is unit tested. The window is `ui::board_sheet`; **Finish board** calls
//! `create_board` as the user, the same tool an agent uses.
//!
//! Until Finish nothing exists in the library: the layout is this state only, so it stays
//! editable. The canvas is drawn from [`cm_render::compose_board::plan`] with the sources' pixel
//! sizes; a source with a `crop` op counts with its cropped size ([`cropped_size`]), which is the
//! size the board's picture really has.
//!
//! **Free layout** (BRD-1, "place and size each image yourself") has a drag canvas: the slots live
//! in [`BoardSheet::free`] and a drag ([`FreeDrag`]) moves a slot or resizes it by a corner (the
//! picture keeps its proportions), snapping to the margin, to the edges and centres of the other
//! pictures and to the board's gap ([`snap_move`], [`snap_resize`]). The snap guides and the gap
//! measure are pink on the canvas (design board S10); everything here is pure geometry in board
//! pixels, so it is unit tested without a window.

use cm_i18n::{tr, trn};
use cm_render::compose_board::{
    self, Arrangement, Background, BoardLayout, BoardOptions, BoardPlan, LayoutKind, Slot,
    MAX_SOURCES, MIN_SOURCES,
};
use serde_json::{json, Value};

/// One screenshot of the board.
#[derive(Debug, Clone, PartialEq)]
pub struct BoardSource {
    pub id: String,
    pub title: String,
    pub width: u32,
    pub height: u32,
}

/// The background choices (BRD-1: transparent, white, light grey, dark or a tint colour).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BgChoice {
    Transparent,
    White,
    LightGrey,
    Dark,
    Tint,
}

impl BgChoice {
    pub const ALL: [BgChoice; 5] = [
        BgChoice::Transparent,
        BgChoice::White,
        BgChoice::LightGrey,
        BgChoice::Dark,
        BgChoice::Tint,
    ];

    pub fn label(self) -> String {
        match self {
            BgChoice::Transparent => tr!("Transparent"),
            BgChoice::White => tr!("White"),
            BgChoice::LightGrey => tr!("Light grey"),
            BgChoice::Dark => tr!("Dark"),
            BgChoice::Tint => tr!("Tint"),
        }
    }
}

/// The layouts the sheet offers. Free is placed by dragging on the canvas.
pub const LAYOUTS: [LayoutKind; 4] = [
    LayoutKind::Pair,
    LayoutKind::Grid,
    LayoutKind::Strip,
    LayoutKind::Free,
];

/// The size of a source for the board: the visible rect of its last `crop` op when it has one (in
/// the `ops` array of `get_capture`), else `fallback`. Crop rects are in original pixels and the
/// renderer rounds them outward to whole pixels, which `ceil` mirrors for the size.
pub fn cropped_size(ops: &Value, fallback: (u32, u32)) -> (u32, u32) {
    let crop = ops.as_array().and_then(|a| {
        a.iter()
            .rev()
            .find(|o| o["op"] == "crop")
            .and_then(|o| o["rect"].as_array())
    });
    let Some(r) = crop.filter(|r| r.len() == 4) else {
        return fallback;
    };
    let n = |i: usize| r[i].as_f64().unwrap_or(0.0);
    let (w, h) = (n(2).abs().ceil(), n(3).abs().ceil());
    if w >= 1.0 && h >= 1.0 && w < 1.0e6 && h < 1.0e6 {
        (w as u32, h as u32)
    } else {
        fallback
    }
}

/// Smallest side of a Free slot in board pixels.
pub const MIN_SLOT: u32 = 48;

/// A pink guide on the canvas while dragging: a vertical line at x or a horizontal line at y.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Guide {
    pub vertical: bool,
    pub at: i32,
}

/// The pink measure of the gap between the dragged picture and its nearest neighbour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GapMeasure {
    /// The two ends of the measure line, in board pixels.
    pub from: (i32, i32),
    pub to: (i32, i32),
    /// The gap in board pixels (the tag reads `24 px`).
    pub px: i32,
}

/// Which part of a Free picture is held.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grip {
    Move,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

/// One drag in progress on the Free canvas: where it began (window pixels) and the slot then.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FreeDrag {
    pub ix: usize,
    pub grip: Grip,
    pub origin: (f32, f32),
    pub start: Slot,
}

/// The lines (x or y) a dragged picture snaps to: the margin, then edges, centres and the gap
/// distance of every other picture. `vertical` picks x lines.
fn snap_lines(slots: &[Slot], skip: usize, margin: i32, gap: i32, vertical: bool) -> Vec<i32> {
    let mut v = vec![margin];
    for (i, o) in slots.iter().enumerate().filter(|(i, _)| *i != skip) {
        let _ = i;
        let (a, len) = if vertical { (o.x, o.w) } else { (o.y, o.h) };
        let len = len as i32;
        v.extend([a, a + len / 2, a + len, a + len + gap, a - gap]);
    }
    v
}

/// The best snap of the moving points `pts` (each with an offset from the box start) to `lines`
/// within `thr`: the shift to add, and the line it lands on.
fn best_snap(pts: &[i32], lines: &[i32], thr: i32) -> Option<(i32, i32)> {
    let mut best: Option<(i32, i32)> = None;
    for &p in pts {
        for &l in lines {
            let d = l - p;
            if d.abs() <= thr && best.is_none_or(|(b, _)| d.abs() < b.abs()) {
                best = Some((d, l));
            }
        }
    }
    best
}

/// Moves a picture by `(dx, dy)` board pixels from `start`, snapping each axis to the other
/// pictures and the margin within `thr` board pixels (`None`: no snapping, Alt held). Shift locks
/// the axis that moved less. The picture stays at x, y >= 0. Returns the slot and the guides.
pub fn snap_move(
    slots: &[Slot],
    ix: usize,
    start: Slot,
    (dx, dy): (f64, f64),
    opts: (i32, i32),
    thr: Option<i32>,
    lock_axis: bool,
) -> (Slot, Vec<Guide>) {
    let (margin, gap) = opts;
    let (mut dx, mut dy) = (dx.round() as i32, dy.round() as i32);
    if lock_axis {
        if dx.abs() >= dy.abs() {
            dy = 0;
        } else {
            dx = 0;
        }
    }
    let mut s = Slot {
        x: (start.x + dx).max(0),
        y: (start.y + dy).max(0),
        ..start
    };
    let mut guides = Vec::new();
    if let Some(thr) = thr {
        if !(lock_axis && dx == 0) {
            let lines = snap_lines(slots, ix, margin, gap, true);
            let pts = [s.x, s.x + s.w as i32 / 2, s.x + s.w as i32];
            if let Some((d, at)) = best_snap(&pts, &lines, thr) {
                s.x = (s.x + d).max(0);
                guides.push(Guide { vertical: true, at });
            }
        }
        if !(lock_axis && dy == 0) {
            let lines = snap_lines(slots, ix, margin, gap, false);
            let pts = [s.y, s.y + s.h as i32 / 2, s.y + s.h as i32];
            if let Some((d, at)) = best_snap(&pts, &lines, thr) {
                s.y = (s.y + d).max(0);
                guides.push(Guide {
                    vertical: false,
                    at,
                });
            }
        }
    }
    (s, guides)
}

/// Resizes a picture by a corner: the opposite corner stays, the picture keeps the proportions
/// of `ratio` (height over width) and the moving corner snaps to the same lines as a move.
pub fn snap_resize(
    slots: &[Slot],
    ix: usize,
    start: Slot,
    grip: Grip,
    (dx, dy): (f64, f64),
    opts: (i32, i32),
    thr: Option<i32>,
) -> (Slot, Vec<Guide>) {
    let (margin, gap) = opts;
    let ratio = f64::from(start.h) / f64::from(start.w.max(1));
    let left = matches!(grip, Grip::TopLeft | Grip::BottomLeft);
    let top = matches!(grip, Grip::TopLeft | Grip::TopRight);
    // The anchor is the corner that does not move.
    let (ax, ay) = (
        if left {
            start.x + start.w as i32
        } else {
            start.x
        },
        if top {
            start.y + start.h as i32
        } else {
            start.y
        },
    );
    let sx = if left { -1.0 } else { 1.0 };
    let sy = if top { -1.0 } else { 1.0 };
    // The width that follows the pointer: the larger of what x and y ask for (the corner can be
    // dragged diagonally), never below the minimum, never past the board's top-left edge.
    let w_x = f64::from(start.w) + dx * sx;
    let w_y = (f64::from(start.h) + dy * sy) / ratio;
    let mut w = w_x.max(w_y).max(f64::from(MIN_SLOT));
    let max_w = if left { f64::from(ax) } else { f64::INFINITY };
    let max_w = max_w.min(if top {
        f64::from(ay) / ratio
    } else {
        f64::INFINITY
    });
    w = w.min(max_w.max(f64::from(MIN_SLOT)));
    let mut guides = Vec::new();
    if let Some(thr) = thr {
        // The moving corner's x, then y; take the nearer snap and rebuild the other side.
        let ex = if left {
            ax - w.round() as i32
        } else {
            ax + w.round() as i32
        };
        let ey_h = (w * ratio).round() as i32;
        let ey = if top { ay - ey_h } else { ay + ey_h };
        let xs = best_snap(&[ex], &snap_lines(slots, ix, margin, gap, true), thr);
        let ys = best_snap(&[ey], &snap_lines(slots, ix, margin, gap, false), thr);
        let pick = match (xs, ys) {
            (Some(a), Some(b)) => Some(if a.0.abs() <= b.0.abs() {
                (a, true)
            } else {
                (b, false)
            }),
            (Some(a), None) => Some((a, true)),
            (None, Some(b)) => Some((b, false)),
            (None, None) => None,
        };
        if let Some(((_, at), is_x)) = pick {
            let nw = if is_x {
                f64::from(if left { ax - at } else { at - ax })
            } else {
                f64::from(if top { ay - at } else { at - ay }) / ratio
            };
            if nw >= f64::from(MIN_SLOT) && nw <= max_w {
                w = nw;
                guides.push(Guide { vertical: is_x, at });
            }
        }
    }
    let w = w.round().max(f64::from(MIN_SLOT)) as u32;
    let h = ((f64::from(w) * ratio).round() as u32).max(1);
    let slot = Slot {
        x: if left { ax - w as i32 } else { ax },
        y: if top { ay - h as i32 } else { ay },
        w,
        h,
    };
    (slot, guides)
}

/// The gap between slot `ix` and its nearest neighbour that overlaps it on the other axis, as the
/// pink measure drawn while dragging; `None` when nothing is beside it.
pub fn gap_measure(slots: &[Slot], ix: usize) -> Option<GapMeasure> {
    let a = *slots.get(ix)?;
    let (ar, ab) = (a.x + a.w as i32, a.y + a.h as i32);
    let mut best: Option<GapMeasure> = None;
    let mut keep = |m: GapMeasure| {
        if m.px >= 0 && best.is_none_or(|b| m.px < b.px) {
            best = Some(m);
        }
    };
    for (i, o) in slots.iter().enumerate().filter(|(i, _)| *i != ix) {
        let _ = i;
        let (or, ob) = (o.x + o.w as i32, o.y + o.h as i32);
        let (y0, y1) = (a.y.max(o.y), ab.min(ob));
        if y1 > y0 {
            let mid = (y0 + y1) / 2;
            if o.x >= ar {
                keep(GapMeasure {
                    from: (ar, mid),
                    to: (o.x, mid),
                    px: o.x - ar,
                });
            } else if or <= a.x {
                keep(GapMeasure {
                    from: (or, mid),
                    to: (a.x, mid),
                    px: a.x - or,
                });
            }
        }
        let (x0, x1) = (a.x.max(o.x), ar.min(or));
        if x1 > x0 {
            let mid = (x0 + x1) / 2;
            if o.y >= ab {
                keep(GapMeasure {
                    from: (mid, ab),
                    to: (mid, o.y),
                    px: o.y - ab,
                });
            } else if ob <= a.y {
                keep(GapMeasure {
                    from: (mid, ob),
                    to: (mid, a.y),
                    px: a.y - ob,
                });
            }
        }
    }
    best
}

pub fn layout_label(k: LayoutKind) -> String {
    match k {
        LayoutKind::Pair => tr!("Pair"),
        LayoutKind::Grid => tr!("Grid"),
        LayoutKind::Strip => tr!("Strip"),
        LayoutKind::Free => tr!("Free"),
    }
}

/// The state of the sheet.
#[derive(Debug, Clone, PartialEq)]
pub struct BoardSheet {
    pub sources: Vec<BoardSource>,
    pub layout: LayoutKind,
    /// Pair and Strip: stacked in a column.
    pub stacked: bool,
    /// Grid columns; `None` is the default (2 up to four pictures, else 3).
    pub columns: Option<u32>,
    pub gap: u32,
    pub margin: u32,
    pub background: BgChoice,
    /// `#RRGGBB` typed for the tint.
    pub tint: String,
    pub radius: u32,
    pub shadow: bool,
    /// A caption under each picture (its title).
    pub captions: bool,
    pub title: String,
    pub before_after: bool,
    /// Free layout: one rectangle per source (same order), in board pixels. Empty until Free is
    /// chosen; then kept in step with `sources`.
    pub free: Vec<Slot>,
    /// Free layout: the drag in progress, if any, and what the canvas shows for it.
    pub drag: Option<FreeDrag>,
    pub guides: Vec<Guide>,
    pub measure: Option<GapMeasure>,
    pub busy: bool,
    pub error: Option<String>,
}

impl BoardSheet {
    pub fn new(sources: Vec<BoardSource>) -> Self {
        let defaults = BoardOptions::default();
        let layout = if sources.len() == 2 {
            LayoutKind::Pair
        } else {
            LayoutKind::Grid
        };
        BoardSheet {
            before_after: layout == LayoutKind::Pair,
            sources,
            layout,
            stacked: false,
            columns: None,
            gap: defaults.gap,
            margin: defaults.margin,
            background: BgChoice::LightGrey,
            tint: "#DCE6F5".to_owned(),
            radius: defaults.radius,
            shadow: defaults.shadow,
            captions: false,
            title: String::new(),
            free: Vec::new(),
            drag: None,
            guides: Vec::new(),
            measure: None,
            busy: false,
            error: None,
        }
    }

    /// Whether a layout can be chosen for this many pictures (Pair is exactly two).
    pub fn layout_enabled(&self, k: LayoutKind) -> bool {
        k != LayoutKind::Pair || self.sources.len() == 2
    }

    pub fn set_layout(&mut self, k: LayoutKind) {
        if self.layout_enabled(k) {
            if k == LayoutKind::Free && self.layout != LayoutKind::Free {
                // Free starts from where the pictures are now, so nothing jumps.
                self.free = self.seed_free();
            }
            self.layout = k;
            self.drag = None;
            self.guides.clear();
            self.measure = None;
        }
    }

    /// The rectangles of the layout that is showing, as the start of a Free layout.
    fn seed_free(&self) -> Vec<Slot> {
        if let Ok(p) = self.plan() {
            if p.scale >= 1.0 {
                return p
                    .cells
                    .iter()
                    .map(|c| Slot {
                        x: c.x,
                        y: c.y,
                        w: c.w,
                        h: c.h,
                    })
                    .collect();
            }
        }
        // A board too large to show at full size: pictures in a row at their own sizes.
        let mut x = self.margin as i32;
        self.sources
            .iter()
            .map(|s| {
                let slot = Slot {
                    x,
                    y: self.margin as i32,
                    w: s.width.max(1),
                    h: s.height.max(1),
                };
                x += slot.w as i32 + self.gap as i32;
                slot
            })
            .collect()
    }

    fn snap_opts(&self) -> (i32, i32) {
        (self.margin as i32, self.gap as i32)
    }

    /// Starts holding picture `ix` by `grip` at window position `at` (Free only).
    pub fn begin_drag(&mut self, ix: usize, grip: Grip, at: (f32, f32)) {
        if self.layout == LayoutKind::Free && ix < self.free.len() {
            self.drag = Some(FreeDrag {
                ix,
                grip,
                origin: at,
                start: self.free[ix],
            });
        }
    }

    /// Continues the drag: `at` is the pointer in window pixels, `k` the canvas scale (window
    /// pixels per board pixel), `alt` switches snapping off, `shift` locks the axis (Move).
    pub fn drag_to(&mut self, at: (f32, f32), k: f32, alt: bool, shift: bool) {
        let Some(d) = self.drag else { return };
        if k <= 0.0 || d.ix >= self.free.len() {
            return;
        }
        let delta = (
            f64::from((at.0 - d.origin.0) / k),
            f64::from((at.1 - d.origin.1) / k),
        );
        // About 8 screen pixels, as board pixels.
        let thr = (!alt).then(|| ((8.0 / k).round() as i32).max(1));
        let opts = self.snap_opts();
        let (slot, guides) = match d.grip {
            Grip::Move => snap_move(&self.free, d.ix, d.start, delta, opts, thr, shift),
            g => snap_resize(&self.free, d.ix, d.start, g, delta, opts, thr),
        };
        self.free[d.ix] = slot;
        self.guides = guides;
        self.measure = gap_measure(&self.free, d.ix);
    }

    /// Lets go: the guides and the measure go away.
    pub fn end_drag(&mut self) {
        self.drag = None;
        self.guides.clear();
        self.measure = None;
    }

    /// Brings picture `ix` to the front (the last source is drawn on top).
    pub fn raise(&mut self, ix: usize) -> usize {
        if self.layout != LayoutKind::Free || ix + 1 >= self.sources.len() {
            return ix;
        }
        let src = self.sources.remove(ix);
        self.sources.push(src);
        if ix < self.free.len() {
            let slot = self.free.remove(ix);
            self.free.push(slot);
        }
        self.sources.len() - 1
    }

    /// Moves picture `ix` by `delta` places (the arrows of the source list).
    pub fn move_source(&mut self, ix: usize, delta: isize) {
        let to = ix as isize + delta;
        if ix < self.sources.len() && to >= 0 && (to as usize) < self.sources.len() {
            self.sources.swap(ix, to as usize);
            // The pictures keep their places; only the list order (the stacking) changes.
            if self.free.len() == self.sources.len() {
                self.free.swap(ix, to as usize);
            }
        }
    }

    /// Takes a picture out; Pair needs two, so the layout falls back to Grid.
    pub fn remove_source(&mut self, ix: usize) {
        if ix < self.sources.len() {
            self.sources.remove(ix);
            if ix < self.free.len() {
                self.free.remove(ix);
            }
            if self.layout == LayoutKind::Pair && self.sources.len() != 2 {
                self.layout = LayoutKind::Grid;
            }
        }
    }

    /// Adds a picture (a drop or a pick); at most 12.
    pub fn add_source(&mut self, s: BoardSource) -> bool {
        if self.sources.len() >= MAX_SOURCES || self.sources.iter().any(|x| x.id == s.id) {
            return false;
        }
        // A Free picture comes in at the right of the others.
        if self.layout == LayoutKind::Free || !self.free.is_empty() {
            let right = self
                .free
                .iter()
                .map(|f| f.x + f.w as i32 + self.gap as i32)
                .max()
                .unwrap_or(self.margin as i32);
            let top = self
                .free
                .iter()
                .map(|f| f.y)
                .min()
                .unwrap_or(self.margin as i32);
            self.free.push(Slot {
                x: right,
                y: top,
                w: s.width.max(1),
                h: s.height.max(1),
            });
        }
        self.sources.push(s);
        true
    }

    fn bg(&self) -> Result<Background, String> {
        Ok(match self.background {
            BgChoice::Transparent => Background::Transparent,
            BgChoice::White => Background::White,
            BgChoice::LightGrey => Background::LightGrey,
            BgChoice::Dark => Background::Dark,
            BgChoice::Tint => Background::parse(self.tint.trim())
                .map_err(|_| tr!("The tint must be a colour like #DCE6F5."))?,
        })
    }

    pub fn layout_json(&self) -> BoardLayout {
        let mut l = BoardLayout::new(self.layout);
        if self.stacked && matches!(self.layout, LayoutKind::Pair | LayoutKind::Strip) {
            l.arrangement = Arrangement::Stacked;
        }
        if self.layout == LayoutKind::Grid {
            l.columns = self.columns;
        }
        if self.layout == LayoutKind::Free && self.free.len() == self.sources.len() {
            l.slots = self.free.clone();
        }
        l
    }

    pub fn options(&self) -> Result<BoardOptions, String> {
        Ok(BoardOptions {
            gap: self.gap,
            margin: self.margin,
            background: self.bg()?,
            radius: self.radius,
            shadow: self.shadow,
            captions: if self.captions {
                self.sources.iter().map(|s| s.title.clone()).collect()
            } else {
                Vec::new()
            },
            title: Some(self.title.trim().to_owned()).filter(|t| !t.is_empty()),
            before_after: self.layout == LayoutKind::Pair && self.before_after,
            label_words: Some([tr!("BEFORE").to_uppercase(), tr!("AFTER").to_uppercase()]),
        })
    }

    /// The geometry the canvas draws.
    pub fn plan(&self) -> Result<BoardPlan, String> {
        let sizes: Vec<(u32, u32)> = self
            .sources
            .iter()
            .map(|s| (s.width.max(1), s.height.max(1)))
            .collect();
        compose_board::plan(&sizes, &self.layout_json(), &self.options()?)
            .map_err(|e| e.to_string())
    }

    /// The size chip: "2400 x 1200 px".
    pub fn size_chip(&self) -> String {
        match self.plan() {
            Ok(p) => format!("{} \u{00D7} {} px", p.width, p.height),
            Err(_) => String::new(),
        }
    }

    pub fn title_text(&self) -> String {
        trn!(
            "Board of {n} screenshot",
            "Board of {n} screenshots",
            self.sources.len()
        )
    }

    pub fn can_finish(&self) -> bool {
        let n = self.sources.len();
        !self.busy
            && (MIN_SOURCES..=MAX_SOURCES).contains(&n)
            && (self.layout != LayoutKind::Pair || n == 2)
            && self.plan().is_ok()
    }

    /// The `create_board` input.
    pub fn request(&self) -> Result<Value, String> {
        let n = self.sources.len();
        if !(MIN_SOURCES..=MAX_SOURCES).contains(&n) {
            return Err(tr!("A board needs 2 to 12 screenshots."));
        }
        let options = self.options()?;
        let mut p = json!({
            "capture_ids": self.sources.iter().map(|s| &s.id).collect::<Vec<_>>(),
            "layout": self.layout_json(),
            "options": options,
        });
        if !self.title.trim().is_empty() {
            p["title"] = json!(self.title.trim());
        }
        Ok(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn src(id: &str, w: u32, h: u32) -> BoardSource {
        BoardSource {
            id: id.into(),
            title: id.to_uppercase(),
            width: w,
            height: h,
        }
    }

    #[test]
    fn a_new_board_starts_with_the_designed_look() {
        let s = BoardSheet::new(vec![src("a", 400, 200), src("b", 300, 300)]);
        assert_eq!((s.margin, s.gap, s.radius), (40, 24, 12));
        assert!(s.shadow);
        assert_eq!(s.background, BgChoice::LightGrey);
    }

    #[test]
    fn two_pictures_start_as_a_pair_with_labels() {
        let s = BoardSheet::new(vec![src("a", 400, 200), src("b", 300, 300)]);
        assert_eq!(s.layout, LayoutKind::Pair);
        assert!(s.before_after);
        let p = s.plan().unwrap();
        assert_eq!(p.cells[0].h, p.cells[1].h);
        assert_eq!(
            s.size_chip(),
            format!("{} \u{00D7} {} px", p.width, p.height)
        );
        let r = s.request().unwrap();
        assert_eq!(r["layout"]["kind"], "pair");
        assert_eq!(r["options"]["before_after"], true);
        assert_eq!(r["options"]["background"], "light_grey");
        assert_eq!(r["capture_ids"], json!(["a", "b"]));
    }

    #[test]
    fn pair_needs_two_and_falls_back_to_a_grid() {
        let mut s = BoardSheet::new(vec![src("a", 10, 10), src("b", 10, 10), src("c", 10, 10)]);
        assert_eq!(s.layout, LayoutKind::Grid);
        assert!(!s.layout_enabled(LayoutKind::Pair));
        s.set_layout(LayoutKind::Pair);
        assert_eq!(s.layout, LayoutKind::Grid);
        s.set_layout(LayoutKind::Strip);
        s.stacked = true;
        assert_eq!(s.request().unwrap()["layout"]["arrangement"], "stacked");
        s.remove_source(0);
        s.set_layout(LayoutKind::Pair);
        assert_eq!(s.layout, LayoutKind::Pair);
        s.remove_source(0);
        assert!(!s.can_finish());
        assert!(s.request().is_err());
    }

    #[test]
    fn sources_move_and_the_count_is_capped() {
        let mut s = BoardSheet::new(vec![src("a", 10, 10), src("b", 10, 10)]);
        s.move_source(0, 1);
        assert_eq!(s.sources[0].id, "b");
        s.move_source(0, -1);
        assert_eq!(s.sources[0].id, "b");
        for i in 0..20 {
            s.add_source(src(&format!("x{i}"), 10, 10));
        }
        assert_eq!(s.sources.len(), 12);
        assert!(!s.add_source(src("b", 10, 10)));
    }

    #[test]
    fn captions_and_a_tint_go_into_the_options() {
        let mut s = BoardSheet::new(vec![src("a", 10, 10), src("b", 10, 10)]);
        s.captions = true;
        s.background = BgChoice::Tint;
        s.tint = "#112233".into();
        s.title = " Release ".into();
        let r = s.request().unwrap();
        assert_eq!(r["options"]["captions"], json!(["A", "B"]));
        assert_eq!(r["options"]["background"], "#112233");
        assert_eq!(r["options"]["title"], "Release");
        assert_eq!(r["title"], "Release");
        s.tint = "blue".into();
        assert!(s.request().is_err());
        assert!(!s.can_finish());
    }

    fn slot(x: i32, y: i32, w: u32, h: u32) -> Slot {
        Slot { x, y, w, h }
    }

    #[test]
    fn a_crop_op_gives_the_boards_source_size() {
        let ops = json!([
            {"op": "arrow"},
            {"op": "crop", "rect": [0, 0, 500, 300]},
            {"op": "crop", "rect": [10, 20, 400.2, 250]},
        ]);
        // The last crop wins; fractions round outward.
        assert_eq!(cropped_size(&ops, (1000, 800)), (401, 250));
        assert_eq!(cropped_size(&json!([]), (1000, 800)), (1000, 800));
        assert_eq!(cropped_size(&json!(null), (7, 8)), (7, 8));
        assert_eq!(
            cropped_size(&json!([{"op": "crop", "rect": [0, 0, 0, 5]}]), (7, 8)),
            (7, 8)
        );
        // The board plan uses it.
        let mut s = BoardSheet::new(vec![src("a", 400, 200), src("b", 400, 200)]);
        s.sources[0].width = 200;
        s.sources[0].height = 100;
        let p = s.plan().unwrap();
        assert_eq!(p.cells[0].h, p.cells[1].h);
    }

    #[test]
    fn free_starts_where_the_pictures_are_and_goes_into_the_request() {
        let mut s = BoardSheet::new(vec![
            src("a", 400, 200),
            src("b", 300, 300),
            src("c", 100, 100),
        ]);
        let grid = s.plan().unwrap();
        s.set_layout(LayoutKind::Free);
        assert_eq!(s.free.len(), 3);
        assert_eq!(
            (s.free[1].x, s.free[1].y, s.free[1].w, s.free[1].h),
            (
                grid.cells[1].x,
                grid.cells[1].y,
                grid.cells[1].w,
                grid.cells[1].h
            )
        );
        // Same canvas as the grid it started from.
        let p = s.plan().unwrap();
        assert_eq!((p.width, p.height), (grid.width, grid.height));
        let r = s.request().unwrap();
        assert_eq!(r["layout"]["kind"], "free");
        assert_eq!(r["layout"]["slots"].as_array().unwrap().len(), 3);
        assert!(s.can_finish());
        // Slots follow the pictures: removing, adding and reordering keep them in step.
        s.remove_source(0);
        assert_eq!(s.free.len(), 2);
        let (b0, c0) = (s.free[0], s.free[1]);
        s.move_source(0, 1);
        assert_eq!((s.free[0], s.free[1]), (c0, b0));
        assert!(s.add_source(src("d", 50, 80)));
        assert_eq!(s.free.len(), 3);
        assert!(s.plan().is_ok());
    }

    #[test]
    fn a_move_snaps_to_the_gap_beside_another_picture_and_shows_a_guide() {
        let slots = [slot(24, 24, 300, 200), slot(400, 30, 300, 200)];
        // Drag picture 1 left until it is 10 px past the 24 px gap: it snaps to x = 324 + 24.
        let (s, g) = snap_move(&slots, 1, slots[1], (-50.0, 0.0), (24, 24), Some(8), false);
        assert_eq!(s.x, 348);
        assert!(g.contains(&Guide {
            vertical: true,
            at: 348
        }));
        // Alt: no snapping.
        let (s, g) = snap_move(&slots, 1, slots[1], (-50.0, 0.0), (24, 24), None, false);
        assert_eq!((s.x, g.len()), (350, 0));
        // Top edges line up (30 vs 24): a horizontal guide.
        let (s, g) = snap_move(&slots, 1, slots[1], (0.0, -4.0), (24, 24), Some(8), false);
        assert_eq!(s.y, 24);
        assert!(g.iter().any(|g| !g.vertical && g.at == 24));
        // Never off the top-left of the board.
        let (s, _) = snap_move(&slots, 1, slots[1], (-900.0, -900.0), (24, 24), None, false);
        assert_eq!((s.x, s.y), (0, 0));
    }

    #[test]
    fn shift_locks_the_axis_that_moved_less() {
        let slots = [slot(100, 100, 200, 100), slot(500, 500, 100, 100)];
        let (s, _) = snap_move(&slots, 0, slots[0], (80.0, 10.0), (24, 24), None, true);
        assert_eq!((s.x, s.y), (180, 100));
        let (s, _) = snap_move(&slots, 0, slots[0], (10.0, 80.0), (24, 24), None, true);
        assert_eq!((s.x, s.y), (100, 180));
    }

    #[test]
    fn a_corner_resize_keeps_the_proportions_and_the_opposite_corner() {
        let a = slot(100, 100, 400, 200);
        let slots = [a, slot(900, 900, 50, 50)];
        let (s, _) = snap_resize(
            &slots,
            0,
            a,
            Grip::BottomRight,
            (100.0, 0.0),
            (24, 24),
            None,
        );
        assert_eq!((s.x, s.y, s.w, s.h), (100, 100, 500, 250));
        // The top-left corner pulled in keeps the bottom-right corner fixed.
        let (s, _) = snap_resize(&slots, 0, a, Grip::TopLeft, (100.0, 0.0), (24, 24), None);
        assert_eq!((s.x + s.w as i32, s.y + s.h as i32), (500, 300));
        assert_eq!(f64::from(s.h) / f64::from(s.w), 0.5);
        // Never smaller than the minimum, never past the board's edge.
        let (s, _) = snap_resize(
            &slots,
            0,
            a,
            Grip::BottomRight,
            (-900.0, -900.0),
            (24, 24),
            None,
        );
        assert_eq!(s.w, MIN_SLOT);
        let (s, _) = snap_resize(
            &slots,
            0,
            a,
            Grip::TopLeft,
            (-900.0, -900.0),
            (24, 24),
            None,
        );
        assert!(s.x >= 0 && s.y >= 0);
        // A snap: the right edge lands on the other picture's left edge minus the gap.
        let b = [a, slot(560, 100, 100, 100)];
        let (s, g) = snap_resize(&b, 0, a, Grip::BottomRight, (30.0, 0.0), (24, 24), Some(8));
        assert_eq!(s.x + s.w as i32, 536);
        assert!(g.iter().any(|g| g.vertical && g.at == 536));
    }

    #[test]
    fn the_gap_measure_names_the_nearest_neighbour() {
        let slots = [
            slot(0, 0, 100, 100),
            slot(130, 20, 100, 60),
            slot(0, 400, 50, 50),
        ];
        let m = gap_measure(&slots, 0).unwrap();
        assert_eq!(m.px, 30);
        assert_eq!((m.from.0, m.to.0), (100, 130));
        // Vertical neighbour: 300 px below picture 0.
        let only = [slots[0], slots[2]];
        assert_eq!(gap_measure(&only, 0).unwrap().px, 300);
        assert!(gap_measure(&[slots[0]], 0).is_none());
        // Overlapping pictures have no measure on that axis.
        let over = [slot(0, 0, 100, 100), slot(50, 50, 100, 100)];
        assert!(gap_measure(&over, 0).is_none());
    }

    #[test]
    fn a_drag_in_the_sheet_follows_the_pointer_and_lets_go_cleanly() {
        let mut s = BoardSheet::new(vec![src("a", 400, 200), src("b", 400, 200)]);
        s.set_layout(LayoutKind::Free);
        let before = s.free[1];
        // Canvas scale 0.5: 40 window px are 80 board px.
        s.begin_drag(1, Grip::Move, (10.0, 10.0));
        s.drag_to((50.0, 10.0), 0.5, true, false);
        assert_eq!(s.free[1].x, before.x + 80);
        assert!(s.guides.is_empty());
        s.end_drag();
        assert!(s.drag.is_none() && s.measure.is_none());
        // Moving without a drag does nothing.
        s.drag_to((500.0, 500.0), 0.5, true, false);
        assert_eq!(s.free[1].x, before.x + 80);
        // Pair and Grid ignore drags.
        s.set_layout(LayoutKind::Grid);
        s.begin_drag(0, Grip::Move, (0.0, 0.0));
        assert!(s.drag.is_none());
        // Raise brings a picture to the front of the stacking.
        s.set_layout(LayoutKind::Free);
        assert_eq!(s.raise(0), 1);
        assert_eq!(s.sources[1].id, "a");
    }
}
