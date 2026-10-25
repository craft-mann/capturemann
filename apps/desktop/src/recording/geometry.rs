//! Geometry of the recording overlays: the area indicator (REC-5), the HUD placement (REC-5.AC4)
//! and the webcam bubble (CAM-1). Pure arithmetic in logical points; the windows only paint what
//! this module computes.
//!
//! # Coordinates
//!
//! `recording.started` gives the recorded area as **physical px in the global desktop space**
//! (`bounds`) and the area's display scale. The engine builds that space by multiplying a display's
//! point coordinates by that display's own scale, so dividing by the same scale gives back global
//! points ([`area_points`]); GPUI window bounds are global points too. A region on a secondary
//! display therefore lands on the right pixels without knowing the other displays' scales.

use cm_i18n::{tr, tr_ctx};

use serde::{Deserialize, Serialize};

/// A rectangle in points, origin top-left.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub const fn new(x: f64, y: f64, w: f64, h: f64) -> Rect {
        Rect { x, y, w, h }
    }

    pub fn right(&self) -> f64 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f64 {
        self.y + self.h
    }

    pub fn center(&self) -> (f64, f64) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }

    pub fn contains(&self, px: f64, py: f64) -> bool {
        px >= self.x && px < self.right() && py >= self.y && py < self.bottom()
    }

    /// The rectangle moved so its origin is relative to `origin`'s origin.
    pub fn relative_to(&self, origin: &Rect) -> Rect {
        Rect::new(self.x - origin.x, self.y - origin.y, self.w, self.h)
    }
}

/// Physical `[x, y, w, h]` bounds and a display scale to points.
pub fn area_points(bounds: [i64; 4], scale: f64) -> Rect {
    let s = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    Rect::new(
        bounds[0] as f64 / s,
        bounds[1] as f64 / s,
        bounds[2] as f64 / s,
        bounds[3] as f64 / s,
    )
}

/// Index of the display (by its point bounds) the recorded area belongs to: the one holding the
/// area's center, else the one with the largest overlap, else the first.
pub fn pick_display(displays: &[Rect], area: &Rect) -> Option<usize> {
    let (cx, cy) = area.center();
    if let Some(i) = displays.iter().position(|d| d.contains(cx, cy)) {
        return Some(i);
    }
    let overlap = |d: &Rect| {
        let w = (d.right().min(area.right()) - d.x.max(area.x)).max(0.0);
        let h = (d.bottom().min(area.bottom()) - d.y.max(area.y)).max(0.0);
        w * h
    };
    displays
        .iter()
        .enumerate()
        .max_by(|a, b| overlap(a.1).total_cmp(&overlap(b.1)))
        .map(|(i, _)| i)
}

// ---- area indicator --------------------------------------------------------------------------

/// Crop-bracket arm length (kit `Recording`: 28 px).
pub const BRACKET_ARM: f64 = 28.0;
/// Bracket stroke (kit: 4 px white, radius 6).
pub const BRACKET_STROKE: f64 = 4.0;
/// Bracket corner radius.
pub const BRACKET_RADIUS: f64 = 6.0;
/// Outline stroke (REC-5: 2 px `rec` at 70% opacity; dashed while paused).
pub const OUTLINE_STROKE: f64 = 2.0;
/// Outline opacity, percent.
pub const OUTLINE_OPACITY_PERCENT: u32 = 70;
/// Scrim outside a region (kit: 55% black).
pub const SCRIM_OPACITY_PERCENT: u32 = 55;
/// Height of a size chip (mono 12 with 4 px vertical padding and a 1 px border).
pub const CHIP_HEIGHT: f64 = 26.0;
/// Gap between the chips and the area's top edge (kit: chips end 12 px above the corner).
pub const CHIP_GAP: f64 = 12.0;

/// What the recorded area is (`indicator.area.kind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AreaKind {
    Display,
    Window,
    Region,
}

impl AreaKind {
    pub fn parse(s: &str) -> AreaKind {
        match s {
            "window" => AreaKind::Window,
            "region" => AreaKind::Region,
            _ => AreaKind::Display,
        }
    }

    /// REC-5: the scrim outside the area is shown for a region (a display has no outside, and the
    /// window outline leaves the desktop undimmed so the user can keep working around it).
    pub fn dims_outside(self) -> bool {
        self == AreaKind::Region
    }
}

/// The four brackets as outer boxes (each `BRACKET_ARM` square) around `area`, 4 px outside the
/// selection like the kit (`left: 156` for a selection at 160), clamped to `display` so a display
/// or edge-aligned recording still shows them (REC-5.AC9). Order: top-left, top-right,
/// bottom-left, bottom-right.
pub fn brackets(area: &Rect, display: &Rect) -> [Rect; 4] {
    let a = BRACKET_ARM;
    let left = (area.x - BRACKET_STROKE).max(display.x);
    let top = (area.y - BRACKET_STROKE).max(display.y);
    let right = (area.right() + BRACKET_STROKE).min(display.right());
    let bottom = (area.bottom() + BRACKET_STROKE).min(display.bottom());
    [
        Rect::new(left, top, a, a),
        Rect::new(right - a, top, a, a),
        Rect::new(left, bottom - a, a, a),
        Rect::new(right - a, bottom - a, a, a),
    ]
}

/// The two chips of the kit: `1920 × 1080` and `2× · native`.
pub fn size_chips(source_size: [u32; 2], scale: f64, output: Option<[u32; 2]>) -> (String, String) {
    let first = format!("{} \u{00D7} {}", source_size[0], source_size[1]);
    let factor = if (scale - scale.round()).abs() < 0.005 {
        format!("{}\u{00D7}", scale.round() as i64)
    } else {
        format!("{scale:.1}\u{00D7}")
    };
    let second = match output {
        Some(o) if o != source_size => {
            tr!(
                "{factor} \u{00B7} {w} \u{00D7} {h} out",
                factor = factor,
                w = o[0],
                h = o[1]
            )
        }
        _ => tr!("{factor} \u{00B7} native", factor = factor),
    };
    (first, second)
}

/// Where the chips sit: above the top-left corner (`CHIP_GAP` over the selection), or just inside
/// the area when there is no room above it.
pub fn chips_origin(area: &Rect, display: &Rect) -> (f64, f64) {
    let above = area.y - CHIP_GAP - CHIP_HEIGHT;
    let x = area.x.max(display.x);
    if above >= display.y {
        (x, above)
    } else {
        (x + CHIP_GAP, area.y.max(display.y) + CHIP_GAP)
    }
}

/// The four rectangles that dim everything outside `area` inside `display` (top, bottom, left,
/// right strips); empty strips are dropped.
pub fn scrim_strips(area: &Rect, display: &Rect) -> Vec<Rect> {
    let top = (area.y - display.y).clamp(0.0, display.h);
    let bottom_y = area.bottom().clamp(display.y, display.bottom());
    let left = area.x.clamp(display.x, display.right());
    let right = area.right().clamp(display.x, display.right());
    let mid_top = display.y + top;
    let mid_h = (bottom_y - mid_top).max(0.0);
    [
        Rect::new(display.x, display.y, display.w, top),
        Rect::new(display.x, bottom_y, display.w, display.bottom() - bottom_y),
        Rect::new(display.x, mid_top, left - display.x, mid_h),
        Rect::new(right, mid_top, display.right() - right, mid_h),
    ]
    .into_iter()
    .filter(|r| r.w > 0.0 && r.h > 0.0)
    .collect()
}

// ---- HUD placement ---------------------------------------------------------------------------

/// Distance from the display's bottom edge to the HUD's bar in the default position (kit: 36).
pub const HUD_BOTTOM_MARGIN: f64 = 36.0;
/// How far from an edge a dropped HUD snaps to it (REC-5.AC4).
pub const HUD_SNAP_DISTANCE: f64 = 48.0;
/// Margin kept to a screen edge after snapping.
pub const HUD_EDGE_MARGIN: f64 = 16.0;

/// Default HUD position: centered horizontally, `HUD_BOTTOM_MARGIN` above the bottom edge.
pub fn hud_default(display: &Rect, size: (f64, f64)) -> Rect {
    Rect::new(
        display.x + (display.w - size.0) / 2.0,
        display.bottom() - HUD_BOTTOM_MARGIN - size.1,
        size.0,
        size.1,
    )
}

/// Keeps the HUD fully on `display` and snaps it to an edge it was dropped near.
pub fn snap_hud(hud: &Rect, display: &Rect) -> Rect {
    let max_x = (display.right() - hud.w - HUD_EDGE_MARGIN).max(display.x);
    let max_y = (display.bottom() - hud.h - HUD_EDGE_MARGIN).max(display.y);
    let min_x = display.x + HUD_EDGE_MARGIN;
    let min_y = display.y + HUD_EDGE_MARGIN;
    let snap = |v: f64, lo: f64, hi: f64| {
        let v = v.clamp(lo.min(hi), hi);
        if v - lo < HUD_SNAP_DISTANCE {
            lo.min(hi)
        } else if hi - v < HUD_SNAP_DISTANCE {
            hi
        } else {
            v
        }
    };
    Rect::new(
        snap(hud.x, min_x, max_x),
        snap(hud.y, min_y, max_y),
        hud.w,
        hud.h,
    )
}

/// The HUD's center as fractions of its display (what is remembered per display).
pub fn center_fraction(r: &Rect, within: &Rect) -> (f64, f64) {
    let (cx, cy) = r.center();
    (
        ((cx - within.x) / within.w.max(1.0)).clamp(0.0, 1.0),
        ((cy - within.y) / within.h.max(1.0)).clamp(0.0, 1.0),
    )
}

/// Inverse of [`center_fraction`] for a rectangle of `size`.
pub fn rect_from_center_fraction(f: (f64, f64), size: (f64, f64), within: &Rect) -> Rect {
    Rect::new(
        within.x + f.0 * within.w - size.0 / 2.0,
        within.y + f.1 * within.h - size.1 / 2.0,
        size.0,
        size.1,
    )
}

// ---- webcam bubble ---------------------------------------------------------------------------

/// The three presets of CAM-1 (OD-1): diameter or width in logical px.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BubbleSize {
    S,
    #[default]
    M,
    L,
}

impl BubbleSize {
    pub const ALL: [BubbleSize; 3] = [BubbleSize::S, BubbleSize::M, BubbleSize::L];

    pub fn px(self) -> f64 {
        match self {
            BubbleSize::S => 160.0,
            BubbleSize::M => 240.0,
            BubbleSize::L => 320.0,
        }
    }

    pub fn parse(s: &str) -> Option<BubbleSize> {
        match s {
            "s" => Some(BubbleSize::S),
            "m" => Some(BubbleSize::M),
            "l" => Some(BubbleSize::L),
            _ => None,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            BubbleSize::S => "s",
            BubbleSize::M => "m",
            BubbleSize::L => "l",
        }
    }

    pub fn label(self) -> String {
        match self {
            BubbleSize::S => tr_ctx!("bubble size", "Small"),
            BubbleSize::M => tr_ctx!("bubble size", "Medium"),
            BubbleSize::L => tr_ctx!("bubble size", "Large"),
        }
    }
}

/// Circle, or the 4:3 rounded rectangle with a 24 px radius.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BubbleShape {
    #[default]
    Circle,
    Rounded,
}

impl BubbleShape {
    pub fn parse(s: &str) -> Option<BubbleShape> {
        match s {
            "circle" => Some(BubbleShape::Circle),
            "rounded" => Some(BubbleShape::Rounded),
            _ => None,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            BubbleShape::Circle => "circle",
            BubbleShape::Rounded => "rounded",
        }
    }

    pub fn label(self) -> String {
        match self {
            BubbleShape::Circle => tr_ctx!("bubble shape", "Circle"),
            BubbleShape::Rounded => tr_ctx!("bubble shape", "Rounded"),
        }
    }

    pub fn toggled(self) -> BubbleShape {
        match self {
            BubbleShape::Circle => BubbleShape::Rounded,
            BubbleShape::Rounded => BubbleShape::Circle,
        }
    }

    /// Corner radius of the outer shape for a bubble of `w` x `h`.
    pub fn radius(self, w: f64, h: f64) -> f64 {
        match self {
            BubbleShape::Circle => w.min(h) / 2.0,
            BubbleShape::Rounded => 24.0,
        }
    }
}

/// Ring width (kit: 4 px `rec`, part of the outer size).
pub const BUBBLE_RING: f64 = 4.0;
/// Margin the bubble keeps to the recorded area's edge when it sits on an anchor (CAM-1).
pub const BUBBLE_MARGIN: f64 = 24.0;
/// Snap distance to an anchor (CAM-1: 32 px).
pub const BUBBLE_SNAP: f64 = 32.0;
/// Outer width and height of the bubble.
pub fn bubble_dims(size: BubbleSize, shape: BubbleShape) -> (f64, f64) {
    let w = size.px();
    match shape {
        BubbleShape::Circle => (w, w),
        BubbleShape::Rounded => (w, (w * 3.0 / 4.0).round()),
    }
}

/// A named anchor of `start_recording`'s `camera.position`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    TopLeft,
    Top,
    TopRight,
    Left,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

impl Anchor {
    pub const ALL: [Anchor; 8] = [
        Anchor::TopLeft,
        Anchor::Top,
        Anchor::TopRight,
        Anchor::Left,
        Anchor::Right,
        Anchor::BottomLeft,
        Anchor::Bottom,
        Anchor::BottomRight,
    ];

    pub fn parse(s: &str) -> Option<Anchor> {
        Some(match s {
            "top-left" => Anchor::TopLeft,
            "top" => Anchor::Top,
            "top-right" => Anchor::TopRight,
            "left" => Anchor::Left,
            "right" => Anchor::Right,
            "bottom-left" => Anchor::BottomLeft,
            "bottom" => Anchor::Bottom,
            "bottom-right" => Anchor::BottomRight,
            _ => return None,
        })
    }

    /// The bubble rectangle on this anchor, `BUBBLE_MARGIN` from the area's edges (centered on
    /// the edge midpoints).
    pub fn rect(self, area: &Rect, dims: (f64, f64)) -> Rect {
        let (w, h) = dims;
        let left = area.x + BUBBLE_MARGIN;
        let right = area.right() - BUBBLE_MARGIN - w;
        let top = area.y + BUBBLE_MARGIN;
        let bottom = area.bottom() - BUBBLE_MARGIN - h;
        let mid_x = area.x + (area.w - w) / 2.0;
        let mid_y = area.y + (area.h - h) / 2.0;
        let (x, y) = match self {
            Anchor::TopLeft => (left, top),
            Anchor::Top => (mid_x, top),
            Anchor::TopRight => (right, top),
            Anchor::Left => (left, mid_y),
            Anchor::Right => (right, mid_y),
            Anchor::BottomLeft => (left, bottom),
            Anchor::Bottom => (mid_x, bottom),
            Anchor::BottomRight => (right, bottom),
        };
        Rect::new(x, y, w, h)
    }
}

/// Where the bubble is: an anchor, or the center as fractions of the recorded area.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BubblePosition {
    Anchor(Anchor),
    /// Center of the bubble as `(x, y)` fractions of the recorded area (0..=1). Fractions keep
    /// the position meaningful when the area or display changes (CAM-1).
    Fraction(f64, f64),
}

impl Default for BubblePosition {
    fn default() -> Self {
        BubblePosition::Anchor(Anchor::BottomLeft)
    }
}

impl BubblePosition {
    /// Parses the engine's `camera.position` JSON (`"bottom-right"` or `[x, y]`).
    pub fn from_json(v: &serde_json::Value) -> Option<BubblePosition> {
        match v {
            serde_json::Value::String(s) => Anchor::parse(s).map(BubblePosition::Anchor),
            serde_json::Value::Array(a) if a.len() == 2 => {
                let x = a[0].as_f64()?;
                let y = a[1].as_f64()?;
                ((0.0..=1.0).contains(&x) && (0.0..=1.0).contains(&y))
                    .then_some(BubblePosition::Fraction(x, y))
            }
            _ => None,
        }
    }

    pub fn to_json(self) -> serde_json::Value {
        match self {
            BubblePosition::Anchor(a) => serde_json::json!(match a {
                Anchor::TopLeft => "top-left",
                Anchor::Top => "top",
                Anchor::TopRight => "top-right",
                Anchor::Left => "left",
                Anchor::Right => "right",
                Anchor::BottomLeft => "bottom-left",
                Anchor::Bottom => "bottom",
                Anchor::BottomRight => "bottom-right",
            }),
            BubblePosition::Fraction(x, y) => serde_json::json!([x, y]),
        }
    }

    /// The bubble rectangle inside `area`, kept fully inside it.
    pub fn rect(self, area: &Rect, dims: (f64, f64)) -> Rect {
        match self {
            BubblePosition::Anchor(a) => a.rect(area, dims),
            BubblePosition::Fraction(fx, fy) => {
                clamp_inside(rect_from_center_fraction((fx, fy), dims, area), area)
            }
        }
    }
}

/// Moves `r` so it lies inside `area` (it is made to fit by origin only; an oversized bubble is
/// left at the area's origin).
pub fn clamp_inside(r: Rect, area: &Rect) -> Rect {
    let max_x = (area.right() - r.w).max(area.x);
    let max_y = (area.bottom() - r.h).max(area.y);
    Rect::new(r.x.clamp(area.x, max_x), r.y.clamp(area.y, max_y), r.w, r.h)
}

/// CAM-1: after a drag the bubble snaps to the nearest of the 8 anchors (corners and edge
/// midpoints) when its origin is within `BUBBLE_SNAP` px of that anchor's origin.
pub fn snap_bubble(r: &Rect, area: &Rect) -> (Rect, Option<Anchor>) {
    let r = clamp_inside(*r, area);
    let best = Anchor::ALL
        .iter()
        .map(|a| {
            let c = a.rect(area, (r.w, r.h));
            let d = ((c.x - r.x).powi(2) + (c.y - r.y).powi(2)).sqrt();
            (*a, c, d)
        })
        .min_by(|a, b| a.2.total_cmp(&b.2));
    match best {
        Some((a, c, d)) if d <= BUBBLE_SNAP => (c, Some(a)),
        _ => (r, None),
    }
}

/// What to remember after a drag: the anchor if it snapped, else the center fractions.
pub fn position_after_drag(r: &Rect, area: &Rect) -> (Rect, BubblePosition) {
    let (snapped, anchor) = snap_bubble(r, area);
    match anchor {
        Some(a) => (snapped, BubblePosition::Anchor(a)),
        None => {
            let (fx, fy) = center_fraction(&snapped, area);
            (snapped, BubblePosition::Fraction(fx, fy))
        }
    }
}

/// One toolbar button (R18 board: 30 px tiles).
pub const TOOLBAR_BUTTON: f64 = 30.0;
/// Buttons on the toolbar: S, M, L, circle, rounded, mirror, camera off.
pub const TOOLBAR_BUTTONS: usize = 7;
/// Gap between the toolbar's items (the buttons and the one divider).
pub const TOOLBAR_ITEM_GAP: f64 = 2.0;
/// The divider between the size group and the rest (1 px line).
pub const TOOLBAR_DIVIDER: f64 = 1.0;
/// Padding plus border of the pill, per side.
pub const TOOLBAR_INSET: f64 = 5.0;
/// Width of the hover toolbar pill: 7 buttons of 30 px, the divider, 7 gaps of 2 px, and the
/// padding and border (235 px, wider than the S bubble's 160 px).
pub const TOOLBAR_W: f64 = TOOLBAR_BUTTON * TOOLBAR_BUTTONS as f64
    + TOOLBAR_DIVIDER
    + TOOLBAR_ITEM_GAP * TOOLBAR_BUTTONS as f64
    + 2.0 * TOOLBAR_INSET;
/// Height of the hover toolbar pill.
pub const TOOLBAR_H: f64 = 40.0;
/// Space between the pill and the bubble's outer edge.
pub const TOOLBAR_GAP: f64 = 6.0;
/// Extra window height for the toolbar, on the bubble's top (or bottom when flipped).
pub const TOOLBAR_BAND: f64 = TOOLBAR_H + TOOLBAR_GAP;

/// Where everything sits in the bubble window: the bubble plus a toolbar band above it (below
/// when there is no room above on the display). All rectangles but `window` are relative to the
/// window's top-left corner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BubbleLayout {
    /// The window, global points.
    pub window: Rect,
    /// The bubble's outer box inside the window.
    pub bubble: Rect,
    /// The toolbar pill inside the window.
    pub toolbar: Rect,
    /// The toolbar sits below the bubble.
    pub flipped: bool,
}

/// Lays the bubble window out for `bubble` (global points) on `display`. The window is as wide as
/// the toolbar (or the bubble, if wider) centred on the bubble and kept on the display; the
/// toolbar is centred on the bubble as far as the window allows.
pub fn bubble_layout(bubble: &Rect, display: &Rect) -> BubbleLayout {
    let ww = bubble.w.max(TOOLBAR_W);
    let (cx, _) = bubble.center();
    let max_x = (display.right() - ww).max(display.x);
    let wx = (cx - ww / 2.0).clamp(display.x, max_x);
    let flipped = bubble.y - TOOLBAR_BAND < display.y;
    let wy = if flipped {
        bubble.y
    } else {
        bubble.y - TOOLBAR_BAND
    };
    let bubble_rel = Rect::new(
        bubble.x - wx,
        if flipped { 0.0 } else { TOOLBAR_BAND },
        bubble.w,
        bubble.h,
    );
    let tx = (cx - wx - TOOLBAR_W / 2.0).clamp(0.0, (ww - TOOLBAR_W).max(0.0));
    let ty = if flipped { bubble.h + TOOLBAR_GAP } else { 0.0 };
    BubbleLayout {
        window: Rect::new(wx, wy, ww, bubble.h + TOOLBAR_BAND),
        bubble: bubble_rel,
        toolbar: Rect::new(tx, ty, TOOLBAR_W, TOOLBAR_H),
        flipped,
    }
}

impl BubbleLayout {
    /// The bubble's outer box in global points.
    pub fn bubble_global(&self) -> Rect {
        self.bubble_at(self.window.x, self.window.y)
    }

    /// The bubble's box for a window whose top-left corner is at `(wx, wy)` (the window may have
    /// been dragged since the layout was made).
    pub fn bubble_at(&self, wx: f64, wy: f64) -> Rect {
        Rect::new(
            wx + self.bubble.x,
            wy + self.bubble.y,
            self.bubble.w,
            self.bubble.h,
        )
    }

    /// The toolbar and the gap between it and the bubble, in window coordinates: moving the
    /// pointer from the bubble to a button must not cross a dead strip.
    pub fn toolbar_zone(&self) -> Rect {
        if self.flipped {
            let top = self.bubble.bottom() - BUBBLE_RING;
            Rect::new(
                self.toolbar.x,
                top,
                self.toolbar.w,
                self.toolbar.bottom() - top,
            )
        } else {
            let bottom = self.bubble.y + BUBBLE_RING;
            Rect::new(
                self.toolbar.x,
                self.toolbar.y,
                self.toolbar.w,
                bottom - self.toolbar.y,
            )
        }
    }
}

/// Whether the global point `(px, py)` is a hit in the bubble window whose top-left corner is at
/// `(wx, wy)`: on the visible bubble always, on the toolbar (and its gap) only while it is shown.
/// Everything else in the window is transparent and must let the mouse through.
pub fn layout_hit(
    layout: &BubbleLayout,
    (wx, wy): (f64, f64),
    radius: f64,
    toolbar_shown: bool,
    px: f64,
    py: f64,
) -> bool {
    if bubble_hit(&layout.bubble_at(wx, wy), radius, px, py) {
        return true;
    }
    toolbar_shown && layout.toolbar_zone().contains(px - wx, py - wy)
}

/// Whether the global point `(px, py)` hits the visible bubble: inside the bubble's box and,
/// for the circle and the rounded shape, inside its rounded corners. Everything else in the
/// window is transparent and must let the mouse through.
pub fn bubble_hit(bubble: &Rect, radius: f64, px: f64, py: f64) -> bool {
    if px < bubble.x || py < bubble.y || px >= bubble.right() || py >= bubble.bottom() {
        return false;
    }
    let r = radius.min(bubble.w / 2.0).min(bubble.h / 2.0);
    let cx = px.clamp(bubble.x + r, bubble.right() - r);
    let cy = py.clamp(bubble.y + r, bubble.bottom() - r);
    (px - cx).powi(2) + (py - cy).powi(2) <= r * r
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const DISPLAY: Rect = Rect::new(0.0, 0.0, 1440.0, 900.0);

    /// The kit's frame: a 960 x 540 selection at (160, 110) in a 1280 x 800 screen has its
    /// brackets at (156, 106) and (1096, 106): 4 px outside, 28 px arms.
    #[test]
    fn rec5_ac9_brackets_follow_the_kit() {
        let screen = Rect::new(0.0, 0.0, 1280.0, 800.0);
        let sel = Rect::new(160.0, 110.0, 960.0, 540.0);
        let b = brackets(&sel, &screen);
        assert_eq!((b[0].x, b[0].y), (156.0, 106.0));
        assert_eq!((b[1].x, b[1].y), (1096.0, 106.0));
        assert_eq!((b[2].x, b[2].y), (156.0, 626.0));
        assert_eq!((b[3].x, b[3].y), (1096.0, 626.0));
        assert!(b.iter().all(|r| r.w == BRACKET_ARM && r.h == BRACKET_ARM));
    }

    /// REC-5.AC9: a display recording still shows four brackets, inside the display.
    #[test]
    fn rec5_ac9_display_brackets_stay_on_screen() {
        let b = brackets(&DISPLAY, &DISPLAY);
        assert_eq!((b[0].x, b[0].y), (0.0, 0.0));
        assert_eq!((b[3].right(), b[3].bottom()), (1440.0, 900.0));
        for r in b {
            assert!(r.x >= 0.0 && r.y >= 0.0 && r.right() <= 1440.0 && r.bottom() <= 900.0);
        }
    }

    #[test]
    fn bounds_become_points_with_the_areas_scale() {
        // A region at physical (2560, 400) 1920 x 1080 on a 2x display.
        let r = area_points([2560, 400, 1920, 1080], 2.0);
        assert_eq!(r, Rect::new(1280.0, 200.0, 960.0, 540.0));
        // A bad scale never divides by zero.
        assert_eq!(
            area_points([10, 10, 20, 20], 0.0),
            Rect::new(10.0, 10.0, 20.0, 20.0)
        );
    }

    #[test]
    fn the_display_is_found_by_the_areas_center() {
        let d = [
            Rect::new(0.0, 0.0, 1440.0, 900.0),
            Rect::new(1440.0, 0.0, 1920.0, 1080.0),
        ];
        assert_eq!(
            pick_display(&d, &Rect::new(1500.0, 100.0, 400.0, 300.0)),
            Some(1)
        );
        assert_eq!(
            pick_display(&d, &Rect::new(100.0, 100.0, 400.0, 300.0)),
            Some(0)
        );
        // Straddling: the center decides.
        assert_eq!(
            pick_display(&d, &Rect::new(1200.0, 100.0, 600.0, 300.0)),
            Some(1)
        );
        assert_eq!(pick_display(&[], &DISPLAY), None);
    }

    /// Kit chips: `1920 × 1080` and `2× · native`.
    #[test]
    fn rec5_size_chips() {
        assert_eq!(
            size_chips([1920, 1080], 2.0, Some([1920, 1080])),
            (
                "1920 \u{00D7} 1080".to_owned(),
                "2\u{00D7} \u{00B7} native".to_owned()
            )
        );
        assert_eq!(
            size_chips([3024, 1964], 1.5, None).1,
            "1.5\u{00D7} \u{00B7} native"
        );
        assert_eq!(
            size_chips([3024, 1964], 2.0, Some([1920, 1080])).1,
            "2\u{00D7} \u{00B7} 1920 \u{00D7} 1080 out"
        );
    }

    #[test]
    fn chips_sit_above_the_corner_or_inside_when_there_is_no_room() {
        let (x, y) = chips_origin(&Rect::new(160.0, 110.0, 960.0, 540.0), &DISPLAY);
        assert_eq!((x, y), (160.0, 110.0 - CHIP_GAP - CHIP_HEIGHT));
        let (x, y) = chips_origin(&DISPLAY, &DISPLAY);
        assert_eq!((x, y), (CHIP_GAP, CHIP_GAP));
    }

    /// REC-5: the scrim covers everything outside a region and nothing inside it.
    #[test]
    fn rec5_scrim_surrounds_the_region() {
        let area = Rect::new(160.0, 110.0, 960.0, 540.0);
        let strips = scrim_strips(&area, &Rect::new(0.0, 0.0, 1280.0, 800.0));
        assert_eq!(strips.len(), 4);
        let covered: f64 = strips.iter().map(|r| r.w * r.h).sum();
        assert_eq!(covered, 1280.0 * 800.0 - 960.0 * 540.0);
        assert!(strips.iter().all(|s| s.right() <= area.x
            || s.x >= area.right()
            || s.bottom() <= area.y
            || s.y >= area.bottom()));
        // A full-display area has nothing to dim.
        assert!(scrim_strips(&DISPLAY, &DISPLAY).is_empty());
        assert!(AreaKind::Region.dims_outside() && !AreaKind::Display.dims_outside());
        assert!(!AreaKind::Window.dims_outside());
    }

    /// REC-5 constants from the kit and the product owner decision OD-3.
    #[test]
    fn rec5_indicator_constants() {
        assert_eq!(
            (BRACKET_STROKE, BRACKET_ARM, BRACKET_RADIUS),
            (4.0, 28.0, 6.0)
        );
        assert_eq!((OUTLINE_STROKE, OUTLINE_OPACITY_PERCENT), (2.0, 70));
        assert_eq!(SCRIM_OPACITY_PERCENT, 55);
    }

    /// REC-5.AC4: dropped near an edge, the HUD snaps to it; elsewhere it stays.
    #[test]
    fn rec5_ac4_hud_snaps_to_edges() {
        // The HUD bar of the redesign (Stop carries its label): 506 x 58; with an agent chip it
        // is wider and still centred, snapped and remembered the same way.
        let hud = crate::ui::recording::HUD_BAR;
        assert_eq!(hud, (506.0, 58.0));
        let d = hud_default(&DISPLAY, hud);
        assert_eq!(d.x, (1440.0 - 506.0) / 2.0);
        let wide = crate::recording::hud_layout::bar_size(Some("Claude Code"));
        assert!(wide.0 > hud.0);
        let dw = hud_default(&DISPLAY, wide);
        assert_eq!(dw.x, (1440.0 - wide.0) / 2.0);
        assert_eq!(dw.bottom(), 900.0 - HUD_BOTTOM_MARGIN);
        let sw = snap_hud(&Rect::new(1300.0, 400.0, wide.0, wide.1), &DISPLAY);
        assert_eq!(sw.right(), 1440.0 - HUD_EDGE_MARGIN);
        assert_eq!(d.bottom(), 900.0 - HUD_BOTTOM_MARGIN);
        // Dropped 20 px from the left and 30 px from the top: snaps to both margins.
        let s = snap_hud(&Rect::new(20.0, 30.0, hud.0, hud.1), &DISPLAY);
        assert_eq!((s.x, s.y), (HUD_EDGE_MARGIN, HUD_EDGE_MARGIN));
        // Dropped past the right edge: pulled back on screen and snapped to the margin.
        let s = snap_hud(&Rect::new(1300.0, 400.0, hud.0, hud.1), &DISPLAY);
        assert_eq!(s.right(), 1440.0 - HUD_EDGE_MARGIN);
        assert_eq!(s.y, 400.0);
        // The middle of the screen is left alone.
        let s = snap_hud(&Rect::new(500.0, 400.0, hud.0, hud.1), &DISPLAY);
        assert_eq!((s.x, s.y), (500.0, 400.0));
    }

    #[test]
    fn the_hud_position_round_trips_through_fractions() {
        let r = Rect::new(300.0, 700.0, 506.0, 58.0);
        let f = center_fraction(&r, &DISPLAY);
        let back = rect_from_center_fraction(f, (506.0, 58.0), &DISPLAY);
        assert!((back.x - r.x).abs() < 1e-9 && (back.y - r.y).abs() < 1e-9);
    }

    /// CAM-1 / DESIGN 5.4 (OD-1): S 160, M 240 (default), L 320; rounded is 4:3 with a 24 px radius.
    #[test]
    fn cam1_presets() {
        assert_eq!(BubbleSize::ALL.map(BubbleSize::px), [160.0, 240.0, 320.0]);
        assert_eq!(BubbleSize::default(), BubbleSize::M);
        assert_eq!(
            bubble_dims(BubbleSize::M, BubbleShape::Circle),
            (240.0, 240.0)
        );
        assert_eq!(
            bubble_dims(BubbleSize::L, BubbleShape::Rounded),
            (320.0, 240.0)
        );
        assert_eq!(BubbleShape::Rounded.radius(320.0, 240.0), 24.0);
        assert_eq!(BubbleShape::Circle.radius(240.0, 240.0), 120.0);
        assert_eq!(BUBBLE_RING, 4.0);
    }

    /// CAM-1: default bottom-left with a 24 px margin, as center fractions of the area.
    #[test]
    fn cam1_default_position_is_bottom_left() {
        let area = Rect::new(0.0, 0.0, 1440.0, 900.0);
        let r = BubblePosition::default().rect(&area, (240.0, 240.0));
        assert_eq!((r.x, r.bottom()), (24.0, 900.0 - 24.0));
        let (fx, fy) = center_fraction(&r, &area);
        assert!((fx - 144.0 / 1440.0).abs() < 1e-9 && (fy - 756.0 / 900.0).abs() < 1e-9);
        assert_eq!(
            BubblePosition::default().to_json(),
            serde_json::json!("bottom-left")
        );
        let r = Anchor::Top.rect(&area, (240.0, 240.0));
        assert_eq!((r.x, r.y), (600.0, 24.0));
    }

    /// CAM-1.AC6: dragged near the bottom-right corner, the bubble snaps to the 24 px margins;
    /// far from every anchor it stays where it was dropped and is remembered as fractions.
    #[test]
    fn cam1_ac6_snap_to_anchors_within_32_px() {
        let area = Rect::new(0.0, 0.0, 1440.0, 900.0);
        let near = Rect::new(
            1440.0 - 24.0 - 240.0 - 20.0,
            900.0 - 24.0 - 240.0 + 15.0,
            240.0,
            240.0,
        );
        let (r, a) = snap_bubble(&near, &area);
        assert_eq!(a, Some(Anchor::BottomRight));
        assert_eq!((r.right(), r.bottom()), (1416.0, 876.0));
        let far = Rect::new(500.0, 300.0, 240.0, 240.0);
        let (r, a) = snap_bubble(&far, &area);
        assert_eq!((a, r), (None, far));
        let (_, pos) = position_after_drag(&far, &area);
        let BubblePosition::Fraction(fx, fy) = pos else {
            panic!("expected fractions");
        };
        assert!((fx - 620.0 / 1440.0).abs() < 1e-9 && (fy - 420.0 / 900.0).abs() < 1e-9);
        // Back from the fractions to the same rectangle.
        assert_eq!(pos.rect(&area, (240.0, 240.0)), far);
    }

    #[test]
    fn the_bubble_never_leaves_the_area() {
        let area = Rect::new(100.0, 100.0, 800.0, 600.0);
        let r = clamp_inside(Rect::new(-50.0, 900.0, 240.0, 240.0), &area);
        assert_eq!((r.x, r.y), (100.0, 460.0));
        let p = BubblePosition::Fraction(1.0, 1.0).rect(&area, (240.0, 240.0));
        assert_eq!((p.right(), p.bottom()), (900.0, 700.0));
    }

    #[test]
    fn engine_positions_parse() {
        assert_eq!(
            BubblePosition::from_json(&json!("top-left")),
            Some(BubblePosition::Anchor(Anchor::TopLeft))
        );
        assert_eq!(
            BubblePosition::from_json(&json!([0.25, 0.75])),
            Some(BubblePosition::Fraction(0.25, 0.75))
        );
        assert_eq!(BubblePosition::from_json(&json!([2.0, 0.5])), None);
        assert_eq!(BubblePosition::from_json(&json!("middle")), None);
        for a in Anchor::ALL {
            let j = BubblePosition::Anchor(a).to_json();
            assert_eq!(
                BubblePosition::from_json(&j),
                Some(BubblePosition::Anchor(a))
            );
        }
    }

    const SCREEN: Rect = Rect::new(0.0, 0.0, 1440.0, 900.0);

    #[test]
    fn the_toolbar_is_wider_than_the_small_bubble_and_pinned() {
        assert_eq!(TOOLBAR_W, 235.0);
        assert_eq!(TOOLBAR_BAND, 46.0);
        assert!(TOOLBAR_W > BubbleSize::S.px());
        assert!(TOOLBAR_W < BubbleSize::L.px());
    }

    #[test]
    fn the_toolbar_sits_above_the_bubble_when_there_is_room() {
        for size in BubbleSize::ALL {
            let (w, h) = bubble_dims(size, BubbleShape::Circle);
            let b = Rect::new(600.0, 400.0, w, h);
            let l = bubble_layout(&b, &SCREEN);
            assert!(!l.flipped);
            assert_eq!(l.window.h, h + TOOLBAR_BAND);
            assert_eq!(l.window.w, w.max(TOOLBAR_W));
            assert_eq!(l.bubble_global(), b);
            assert_eq!(l.toolbar.y, 0.0);
            assert!(l.toolbar.bottom() + TOOLBAR_GAP <= l.bubble.y);
            // Centred on the bubble.
            let tc = l.window.x + l.toolbar.x + l.toolbar.w / 2.0;
            assert!((tc - b.center().0).abs() < 0.5, "{size:?}");
            // The pill stays inside the window.
            assert!(l.toolbar.x >= 0.0 && l.toolbar.right() <= l.window.w);
        }
    }

    #[test]
    fn the_toolbar_flips_below_when_there_is_no_room_above() {
        let (w, h) = bubble_dims(BubbleSize::M, BubbleShape::Circle);
        // On the anchor with the 24 px margin there is no room for the 46 px band.
        let b = Rect::new(600.0, BUBBLE_MARGIN, w, h);
        let l = bubble_layout(&b, &SCREEN);
        assert!(l.flipped);
        assert_eq!(l.window.y, b.y);
        assert_eq!(l.bubble.y, 0.0);
        assert_eq!(l.toolbar.y, h + TOOLBAR_GAP);
        assert_eq!(l.bubble_global(), b);
        // Exactly enough room above: not flipped.
        let b = Rect::new(600.0, TOOLBAR_BAND, w, h);
        assert!(!bubble_layout(&b, &SCREEN).flipped);
        let b = Rect::new(600.0, TOOLBAR_BAND - 1.0, w, h);
        assert!(bubble_layout(&b, &SCREEN).flipped);
    }

    #[test]
    fn the_window_stays_on_the_display_at_the_side_edges() {
        let (w, h) = bubble_dims(BubbleSize::S, BubbleShape::Circle);
        let left = Rect::new(BUBBLE_MARGIN, 400.0, w, h);
        let l = bubble_layout(&left, &SCREEN);
        assert_eq!(l.window.x, 0.0);
        assert_eq!(l.bubble_global(), left);
        assert_eq!(l.toolbar.x, 0.0);
        let right = Rect::new(SCREEN.right() - BUBBLE_MARGIN - w, 400.0, w, h);
        let l = bubble_layout(&right, &SCREEN);
        assert_eq!(l.window.right(), SCREEN.right());
        assert_eq!(l.bubble_global(), right);
        assert_eq!(l.toolbar.right(), l.window.w);
        // A second display to the right: the window stays on it.
        let d2 = Rect::new(1440.0, 0.0, 1920.0, 1080.0);
        let b = Rect::new(d2.x + BUBBLE_MARGIN, 500.0, w, h);
        let l = bubble_layout(&b, &d2);
        assert!(l.window.x >= d2.x);
        assert_eq!(l.bubble_global(), b);
    }

    #[test]
    fn snapping_uses_the_bubble_not_the_larger_window() {
        let area = Rect::new(0.0, 0.0, 1440.0, 900.0);
        let (w, h) = bubble_dims(BubbleSize::M, BubbleShape::Circle);
        let layout = bubble_layout(&Rect::new(500.0, 300.0, w, h), &SCREEN);
        // The user drags the window so that the bubble is near the top-left corner.
        let b = layout.bubble_at(30.0 - layout.bubble.x, 40.0 - layout.bubble.y);
        assert_eq!((b.x, b.y), (30.0, 40.0));
        let (snapped, anchor) = snap_bubble(&b, &area);
        assert_eq!(anchor, Some(Anchor::TopLeft));
        assert_eq!((snapped.x, snapped.y), (BUBBLE_MARGIN, BUBBLE_MARGIN));
        // After snapping, the toolbar flips and the window follows the bubble.
        let l = bubble_layout(&snapped, &SCREEN);
        assert!(l.flipped);
        assert_eq!(l.bubble_global(), snapped);
    }

    #[test]
    fn clicks_on_empty_window_area_pass_through() {
        let (w, h) = bubble_dims(BubbleSize::S, BubbleShape::Circle);
        let b = Rect::new(600.0, 400.0, w, h);
        let l = bubble_layout(&b, &SCREEN);
        let o = (l.window.x, l.window.y);
        let r = BubbleShape::Circle.radius(w, h);
        let (cx, cy) = b.center();
        assert!(layout_hit(&l, o, r, false, cx, cy));
        // Corner of the window beside the bubble, and the band above it.
        assert!(!layout_hit(
            &l,
            o,
            r,
            false,
            l.window.x + 1.0,
            l.window.y + 1.0
        ));
        assert!(!layout_hit(&l, o, r, false, l.window.x + 1.0, cy));
        // The hidden toolbar swallows nothing; the shown one is hit, with the gap down to the ring.
        let (tx, ty) = (l.window.x + l.toolbar.x + 10.0, l.window.y + 5.0);
        assert!(!layout_hit(&l, o, r, false, tx, ty));
        assert!(layout_hit(&l, o, r, true, tx, ty));
        let gap_y = l.window.y + l.toolbar.bottom() + TOOLBAR_GAP / 2.0;
        assert!(layout_hit(&l, o, r, true, cx, gap_y));
        // Outside the pill's width stays transparent even when it is shown.
        assert!(!layout_hit(
            &l,
            o,
            r,
            true,
            l.window.x + l.toolbar.x - 2.0,
            ty
        ));
    }

    #[test]
    fn the_toolbar_zone_reaches_the_bubble_in_both_orientations() {
        let (w, h) = bubble_dims(BubbleSize::M, BubbleShape::Rounded);
        for y in [500.0, 10.0] {
            let l = bubble_layout(&Rect::new(600.0, y, w, h), &SCREEN);
            let z = l.toolbar_zone();
            if l.flipped {
                assert!(z.y < l.bubble.bottom());
                assert_eq!(z.bottom(), l.toolbar.bottom());
            } else {
                assert!(z.bottom() > l.bubble.y);
                assert_eq!(z.y, l.toolbar.y);
            }
        }
    }

    #[test]
    fn only_the_circle_and_ring_are_hit() {
        let (w, h) = bubble_dims(BubbleSize::M, BubbleShape::Circle);
        let b = Rect::new(100.0, 100.0, w, h);
        let r = BubbleShape::Circle.radius(w, h);
        assert_eq!(r, 120.0);
        // Centre and the ring's outer edge hit; the window's corners and just outside miss.
        assert!(bubble_hit(&b, r, 220.0, 220.0));
        assert!(bubble_hit(&b, r, 100.5, 220.0));
        assert!(!bubble_hit(&b, r, 101.0, 101.0));
        assert!(!bubble_hit(&b, r, 339.0, 339.0));
        assert!(!bubble_hit(&b, r, 99.0, 220.0));
        // Visible diameter = outer size; the camera fills the inside of the 4 px ring.
        assert_eq!(w - 2.0 * BUBBLE_RING, 232.0);
        // Rounded: corners cut by the 24 px radius, the rest of the box hits.
        let (w, h) = bubble_dims(BubbleSize::M, BubbleShape::Rounded);
        let b = Rect::new(0.0, 0.0, w, h);
        let r = BubbleShape::Rounded.radius(w, h);
        assert!(!bubble_hit(&b, r, 1.0, 1.0));
        assert!(bubble_hit(&b, r, 24.0, 1.0));
        assert!(bubble_hit(&b, r, w / 2.0, h / 2.0));
    }
}
