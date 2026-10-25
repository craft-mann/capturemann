//! Selection geometry: drawing, resizing, moving, nudging, click-vs-drag (CAP-1, CAP-3).
//!
//! Everything here is in **physical px relative to one display's top-left corner**, the same
//! space as the daemon's region `rect` and the frozen frame (CAP-1.AC2). Nothing here knows
//! about windows or the daemon, so it is unit tested without a display.
//!
//! Positions are grid lines: a selection from line 10 to line 430 is 420 px wide and covers
//! pixels 10..=429. Pointer positions from the OS are rounded to the nearest line.

use cm_core::{PhysicalPoint, PhysicalRect, PhysicalSize};

/// Movement below this many physical px (on both axes) between press and release is a click,
/// not a drag (CAP-1.AC5).
pub const CLICK_THRESHOLD_PX: i32 = 4;

/// Selections smaller than this on either axis are discarded (CAP-1 edge cases: "smaller than
/// 2x2 px is a click or cancelled").
pub const MIN_SELECTION_PX: u32 = 2;

/// Rounds an OS pointer position (physical px, fractional) to the nearest grid line and clamps
/// it to the display, so a selection can never leave its display (CAP-1.AC7).
pub fn pointer_to_line(x: f64, y: f64, display: PhysicalSize) -> PhysicalPoint {
    let c = |v: f64, max: u32| -> i32 {
        if v.is_nan() {
            return 0;
        }
        v.round().clamp(0.0, f64::from(max)) as i32
    };
    PhysicalPoint {
        x: c(x, display.width),
        y: c(y, display.height),
    }
}

/// The pixel under a pointer (floor, not round): what the magnifier centers on (CAP-3.AC1).
pub fn pointer_to_pixel(x: f64, y: f64, display: PhysicalSize) -> PhysicalPoint {
    let c = |v: f64, max: u32| -> i32 {
        if v.is_nan() {
            return 0;
        }
        let hi = f64::from(max.saturating_sub(1));
        v.floor().clamp(0.0, hi) as i32
    };
    PhysicalPoint {
        x: c(x, display.width),
        y: c(y, display.height),
    }
}

/// True when press and release are close enough to count as a click (CAP-1.AC5).
pub fn is_click(down: PhysicalPoint, up: PhysicalPoint) -> bool {
    (up.x - down.x).abs() < CLICK_THRESHOLD_PX && (up.y - down.y).abs() < CLICK_THRESHOLD_PX
}

/// Rectangle between two grid lines, normalized (either corner may come first).
pub fn rect_between(a: PhysicalPoint, b: PhysicalPoint) -> PhysicalRect {
    let (x0, x1) = (a.x.min(b.x), a.x.max(b.x));
    let (y0, y1) = (a.y.min(b.y), a.y.max(b.y));
    PhysicalRect {
        origin: PhysicalPoint { x: x0, y: y0 },
        size: PhysicalSize {
            width: (x1 - x0) as u32,
            height: (y1 - y0) as u32,
        },
    }
}

pub fn rect_right(r: PhysicalRect) -> i32 {
    r.origin.x + r.size.width as i32
}

pub fn rect_bottom(r: PhysicalRect) -> i32 {
    r.origin.y + r.size.height as i32
}

/// Pixel `p` lies inside `r` (right and bottom edges exclusive).
pub fn rect_contains(r: PhysicalRect, p: PhysicalPoint) -> bool {
    p.x >= r.origin.x && p.y >= r.origin.y && p.x < rect_right(r) && p.y < rect_bottom(r)
}

/// Overlap of two rects, `None` when empty.
pub fn rect_intersection(a: PhysicalRect, b: PhysicalRect) -> Option<PhysicalRect> {
    let (x0, y0) = (a.origin.x.max(b.origin.x), a.origin.y.max(b.origin.y));
    let (x1, y1) = (
        rect_right(a).min(rect_right(b)),
        rect_bottom(a).min(rect_bottom(b)),
    );
    (x1 > x0 && y1 > y0).then(|| {
        rect_between(
            PhysicalPoint { x: x0, y: y0 },
            PhysicalPoint { x: x1, y: y1 },
        )
    })
}

/// A grab handle on a selection: 4 corners and 4 edges.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Handle {
    North,
    NorthEast,
    East,
    SouthEast,
    South,
    SouthWest,
    West,
    NorthWest,
}

impl Handle {
    pub const ALL: [Handle; 8] = [
        Handle::NorthWest,
        Handle::North,
        Handle::NorthEast,
        Handle::East,
        Handle::SouthEast,
        Handle::South,
        Handle::SouthWest,
        Handle::West,
    ];

    fn moves_left(self) -> bool {
        matches!(self, Handle::West | Handle::NorthWest | Handle::SouthWest)
    }
    fn moves_right(self) -> bool {
        matches!(self, Handle::East | Handle::NorthEast | Handle::SouthEast)
    }
    fn moves_top(self) -> bool {
        matches!(self, Handle::North | Handle::NorthEast | Handle::NorthWest)
    }
    fn moves_bottom(self) -> bool {
        matches!(self, Handle::South | Handle::SouthEast | Handle::SouthWest)
    }
}

/// Where a handle sits on a rect (a corner, or the middle of an edge).
pub fn handle_position(r: PhysicalRect, h: Handle) -> PhysicalPoint {
    let (l, t, rt, b) = (r.origin.x, r.origin.y, rect_right(r), rect_bottom(r));
    let (cx, cy) = ((l + rt) / 2, (t + b) / 2);
    let x = if h.moves_left() {
        l
    } else if h.moves_right() {
        rt
    } else {
        cx
    };
    let y = if h.moves_top() {
        t
    } else if h.moves_bottom() {
        b
    } else {
        cy
    };
    PhysicalPoint { x, y }
}

/// The handle within `grab` px (Chebyshev) of `p`, nearest first. `grab` is the hit radius in
/// physical px (half the 8 logical px handle plus slack, scaled by the display).
pub fn hit_handle(r: PhysicalRect, p: PhysicalPoint, grab: i32) -> Option<Handle> {
    Handle::ALL
        .iter()
        .filter_map(|&h| {
            let hp = handle_position(r, h);
            let d = (hp.x - p.x).abs().max((hp.y - p.y).abs());
            (d <= grab).then_some((d, h))
        })
        .min_by_key(|&(d, _)| d)
        .map(|(_, h)| h)
}

/// One axis of an edit in progress. A moving axis has a fixed edge and a moving edge that may
/// cross it (the rectangle flips); a locked axis keeps its extent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Axis {
    Locked { lo: i32, hi: i32 },
    Moving { fixed: i32, moving: i32 },
}

impl Axis {
    fn span(self) -> (i32, i32) {
        match self {
            Axis::Locked { lo, hi } => (lo, hi),
            Axis::Moving { fixed, moving } => (fixed.min(moving), fixed.max(moving)),
        }
    }

    fn set(self, v: i32, max: i32) -> Axis {
        match self {
            Axis::Moving { fixed, .. } => Axis::Moving {
                fixed,
                moving: v.clamp(0, max),
            },
            locked => locked,
        }
    }

    fn nudged(self, d: i32, max: i32) -> Axis {
        match self {
            Axis::Moving { fixed, moving } => Axis::Moving {
                fixed,
                moving: (moving + d).clamp(0, max),
            },
            locked => locked,
        }
    }

    fn moving(self) -> Option<i32> {
        match self {
            Axis::Moving { moving, .. } => Some(moving),
            Axis::Locked { .. } => None,
        }
    }
}

/// A resize or draw in progress: the fixed anchors and the moving edges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Edit {
    x: Axis,
    y: Axis,
}

impl Edit {
    /// Drawing from `anchor`: both axes move with the pointer.
    pub fn draw(anchor: PhysicalPoint) -> Edit {
        Edit {
            x: Axis::Moving {
                fixed: anchor.x,
                moving: anchor.x,
            },
            y: Axis::Moving {
                fixed: anchor.y,
                moving: anchor.y,
            },
        }
    }

    /// Dragging `handle` of `r`: the opposite edges stay put.
    pub fn resize(r: PhysicalRect, h: Handle) -> Edit {
        let (l, t, rt, b) = (r.origin.x, r.origin.y, rect_right(r), rect_bottom(r));
        let x = if h.moves_left() {
            Axis::Moving {
                fixed: rt,
                moving: l,
            }
        } else if h.moves_right() {
            Axis::Moving {
                fixed: l,
                moving: rt,
            }
        } else {
            Axis::Locked { lo: l, hi: rt }
        };
        let y = if h.moves_top() {
            Axis::Moving {
                fixed: b,
                moving: t,
            }
        } else if h.moves_bottom() {
            Axis::Moving {
                fixed: t,
                moving: b,
            }
        } else {
            Axis::Locked { lo: t, hi: b }
        };
        Edit { x, y }
    }

    /// Moves the free edges to the pointer, clamped to the display.
    pub fn drag_to(self, p: PhysicalPoint, display: PhysicalSize) -> Edit {
        Edit {
            x: self.x.set(p.x, display.width as i32),
            y: self.y.set(p.y, display.height as i32),
        }
    }

    /// Moves the free edges by whole px (arrow keys, CAP-3.AC2).
    pub fn nudge(self, dx: i32, dy: i32, display: PhysicalSize) -> Edit {
        Edit {
            x: self.x.nudged(dx, display.width as i32),
            y: self.y.nudged(dy, display.height as i32),
        }
    }

    /// The rectangle this edit currently describes.
    pub fn rect(self) -> PhysicalRect {
        let (x0, x1) = self.x.span();
        let (y0, y1) = self.y.span();
        rect_between(
            PhysicalPoint { x: x0, y: y0 },
            PhysicalPoint { x: x1, y: y1 },
        )
    }

    /// The point being dragged (for the pointer-warp on nudge); `None` on a locked axis.
    pub fn moving_point(self) -> (Option<i32>, Option<i32>) {
        (self.x.moving(), self.y.moving())
    }
}

/// Moves `r` by `(dx, dy)` keeping its size and staying inside the display.
pub fn move_rect(r: PhysicalRect, dx: i32, dy: i32, display: PhysicalSize) -> PhysicalRect {
    let max_x = (display.width as i32 - r.size.width as i32).max(0);
    let max_y = (display.height as i32 - r.size.height as i32).max(0);
    PhysicalRect {
        origin: PhysicalPoint {
            x: (r.origin.x + dx).clamp(0, max_x),
            y: (r.origin.y + dy).clamp(0, max_y),
        },
        size: r.size,
    }
}

/// Clamps a rect to the display (used for window highlights that extend past the screen).
pub fn clamp_to_display(r: PhysicalRect, display: PhysicalSize) -> Option<PhysicalRect> {
    rect_intersection(
        r,
        PhysicalRect {
            origin: PhysicalPoint { x: 0, y: 0 },
            size: display,
        },
    )
}

/// True when `r` is a usable selection (at least 2x2, CAP-1 edge cases).
pub fn is_selection(r: PhysicalRect) -> bool {
    r.size.width >= MIN_SELECTION_PX && r.size.height >= MIN_SELECTION_PX
}

#[cfg(test)]
mod tests {
    use super::*;

    const DISPLAY: PhysicalSize = PhysicalSize {
        width: 3024,
        height: 1964,
    };

    fn p(x: i32, y: i32) -> PhysicalPoint {
        PhysicalPoint { x, y }
    }

    fn r(x: i32, y: i32, w: u32, h: u32) -> PhysicalRect {
        PhysicalRect {
            origin: p(x, y),
            size: PhysicalSize {
                width: w,
                height: h,
            },
        }
    }

    /// CAP-1.AC2: a drag from (2207, 669) by 420x649 is exactly that rect.
    #[test]
    fn cap1_ac2_drag_gives_the_exact_physical_rect() {
        let edit = Edit::draw(p(2207, 669)).drag_to(p(2207 + 420, 669 + 649), DISPLAY);
        assert_eq!(edit.rect(), r(2207, 669, 420, 649));
    }

    /// CAP-1.AC2: dragging up and to the left gives the same rect.
    #[test]
    fn cap1_ac2_reverse_drag_normalizes() {
        let edit = Edit::draw(p(2627, 1318)).drag_to(p(2207, 669), DISPLAY);
        assert_eq!(edit.rect(), r(2207, 669, 420, 649));
    }

    /// CAP-1.AC2: fractional OS pointer positions land on the nearest grid line.
    #[test]
    fn cap1_ac2_pointer_rounds_to_nearest_line() {
        assert_eq!(pointer_to_line(2206.6, 669.4, DISPLAY), p(2207, 669));
        assert_eq!(pointer_to_line(f64::NAN, 1.0, DISPLAY), p(0, 1));
    }

    /// CAP-1.AC7: the pointer (and so the selection) is clamped to its own display.
    #[test]
    fn cap1_ac7_selection_cannot_leave_the_display() {
        assert_eq!(pointer_to_line(-50.0, 99999.0, DISPLAY), p(0, 1964));
        let edit =
            Edit::draw(p(3000, 1900)).drag_to(pointer_to_line(4000.0, 3000.0, DISPLAY), DISPLAY);
        assert_eq!(edit.rect(), r(3000, 1900, 24, 64));
        assert_eq!(rect_right(edit.rect()), 3024);
    }

    /// CAP-1.AC5: under 4 px of movement is a click; 4 px is a drag.
    #[test]
    fn cap1_ac5_click_threshold_is_four_px() {
        assert!(is_click(p(100, 100), p(100, 100)));
        assert!(is_click(p(100, 100), p(103, 97)));
        assert!(!is_click(p(100, 100), p(104, 100)));
        assert!(!is_click(p(100, 100), p(100, 96)));
    }

    #[test]
    fn selections_under_two_px_are_not_selections() {
        assert!(!is_selection(r(10, 10, 1, 50)));
        assert!(!is_selection(r(10, 10, 50, 1)));
        assert!(is_selection(r(10, 10, 2, 2)));
    }

    #[test]
    fn handles_sit_on_corners_and_edge_midpoints() {
        let sel = r(100, 200, 400, 300);
        assert_eq!(handle_position(sel, Handle::NorthWest), p(100, 200));
        assert_eq!(handle_position(sel, Handle::SouthEast), p(500, 500));
        assert_eq!(handle_position(sel, Handle::North), p(300, 200));
        assert_eq!(handle_position(sel, Handle::West), p(100, 350));
    }

    #[test]
    fn hit_handle_picks_the_nearest_within_the_grab_radius() {
        let sel = r(100, 200, 400, 300);
        assert_eq!(hit_handle(sel, p(103, 198), 6), Some(Handle::NorthWest));
        assert_eq!(hit_handle(sel, p(300, 204), 6), Some(Handle::North));
        assert_eq!(hit_handle(sel, p(300, 350), 6), None);
        // A tiny selection: the nearest handle wins, not the first in the list.
        let tiny = r(100, 100, 4, 4);
        assert_eq!(hit_handle(tiny, p(104, 104), 6), Some(Handle::SouthEast));
    }

    #[test]
    fn dragging_an_edge_handle_moves_only_that_edge() {
        let sel = r(100, 200, 400, 300);
        let e = Edit::resize(sel, Handle::East).drag_to(p(650, 9999), DISPLAY);
        assert_eq!(e.rect(), r(100, 200, 550, 300));
        let e = Edit::resize(sel, Handle::North).drag_to(p(0, 150), DISPLAY);
        assert_eq!(e.rect(), r(100, 150, 400, 350));
    }

    #[test]
    fn dragging_a_corner_past_the_opposite_corner_flips() {
        let sel = r(100, 200, 400, 300);
        let e = Edit::resize(sel, Handle::SouthEast).drag_to(p(50, 100), DISPLAY);
        assert_eq!(e.rect(), r(50, 100, 50, 100));
    }

    #[test]
    fn resize_is_clamped_to_the_display() {
        let sel = r(100, 200, 400, 300);
        let e = Edit::resize(sel, Handle::SouthEast).drag_to(p(99999, 99999), DISPLAY);
        assert_eq!(e.rect(), r(100, 200, 2924, 1764));
    }

    /// CAP-3.AC2: five right-arrow presses during a drag move the right edge exactly 5 px.
    #[test]
    fn cap3_ac2_arrow_nudges_move_the_active_edge_by_whole_pixels() {
        let mut e = Edit::draw(p(100, 100)).drag_to(p(300, 250), DISPLAY);
        for _ in 0..5 {
            e = e.nudge(1, 0, DISPLAY);
        }
        assert_eq!(e.rect(), r(100, 100, 205, 150));
        // Shift+arrow is 10 px (the caller passes 10).
        e = e.nudge(0, 10, DISPLAY);
        assert_eq!(e.rect(), r(100, 100, 205, 160));
    }

    #[test]
    fn nudging_a_locked_axis_does_nothing() {
        let sel = r(100, 200, 400, 300);
        let e = Edit::resize(sel, Handle::East).nudge(0, 7, DISPLAY);
        assert_eq!(e.rect(), sel);
    }

    #[test]
    fn nudge_stops_at_the_display_edge() {
        let e = Edit::draw(p(3000, 10))
            .drag_to(p(3020, 50), DISPLAY)
            .nudge(100, 0, DISPLAY);
        assert_eq!(rect_right(e.rect()), 3024);
    }

    #[test]
    fn moving_keeps_size_and_stays_on_the_display() {
        let sel = r(100, 200, 400, 300);
        assert_eq!(move_rect(sel, 50, -20, DISPLAY), r(150, 180, 400, 300));
        assert_eq!(move_rect(sel, -500, -500, DISPLAY), r(0, 0, 400, 300));
        assert_eq!(
            move_rect(sel, 99999, 99999, DISPLAY),
            r(2624, 1664, 400, 300)
        );
    }

    #[test]
    fn pixel_under_pointer_is_floor_and_in_range() {
        assert_eq!(pointer_to_pixel(100.9, 200.1, DISPLAY), p(100, 200));
        assert_eq!(pointer_to_pixel(5000.0, 5000.0, DISPLAY), p(3023, 1963));
    }

    #[test]
    fn intersection_and_clamp() {
        let a = r(0, 0, 100, 100);
        assert_eq!(
            rect_intersection(a, r(50, 50, 100, 100)),
            Some(r(50, 50, 50, 50))
        );
        assert_eq!(rect_intersection(a, r(100, 0, 10, 10)), None);
        assert_eq!(
            clamp_to_display(
                r(-20, -20, 100, 100),
                PhysicalSize {
                    width: 50,
                    height: 50
                }
            ),
            Some(r(0, 0, 50, 50))
        );
    }
}
