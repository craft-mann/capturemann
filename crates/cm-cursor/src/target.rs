//! The recorded area and the mapping from the OS's global cursor space to its pixels.

use cm_core::{PhysicalSize, Point, Rect};
use serde::{Deserialize, Serialize};

/// What kind of area is recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetKind {
    Display,
    Region,
    Window,
}

/// The recorded area: `px = (global - origin) * scale`, relative to the area's top-left.
///
/// `origin` and the cursor positions the OS reports share one global space:
/// - macOS: logical points, origin at the top-left of the main display (y down, secondary displays
///   at their `CGDisplayBounds`), `scale` = the recorded display's backing scale (2.0 on Retina);
/// - Windows: virtual-screen physical px (per-monitor DPI aware process), `scale` = 1.0.
///
/// A cursor on a display with a different scale is mapped with the *target's* scale, so the result
/// stays a linear function of the global position (and is `outside` anyway).
///
/// A window target is fixed to the window's rectangle when recording starts. Window moves during
/// the recording are not tracked in v0.1 (the screen track is the window's own pixels, so the
/// cursor path would need the window rectangle over time; see the report).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CursorTarget {
    pub kind: TargetKind,
    /// Display the area is on (CGDirectDisplayID / monitor index), if known.
    pub display_id: Option<u32>,
    /// Top-left of the area in the global space, `[x, y]`.
    pub origin: [f64; 2],
    /// Size of the recorded area in physical px (the screen track size).
    pub size: PhysicalSize,
    /// Physical px per global unit.
    pub scale: f64,
}

impl CursorTarget {
    /// A whole display whose global bounds are `bounds` (logical on macOS, physical on Windows).
    pub fn display(display_id: u32, bounds: Rect, scale: f64) -> Self {
        Self::from_global_rect(TargetKind::Display, Some(display_id), bounds, scale)
    }

    /// A window or region given by its rectangle in global space.
    pub fn from_global_rect(
        kind: TargetKind,
        display_id: Option<u32>,
        rect: Rect,
        scale: f64,
    ) -> Self {
        CursorTarget {
            kind,
            display_id,
            origin: [rect.origin.x, rect.origin.y],
            size: PhysicalSize {
                width: (rect.size.width * scale).round().max(0.0) as u32,
                height: (rect.size.height * scale).round().max(0.0) as u32,
            },
            scale,
        }
    }

    /// A region given in coordinates local to its display (logical, top-left of the display),
    /// as the picker reports it (CAP-1).
    pub fn region_on_display(
        display_id: u32,
        display_bounds: Rect,
        local: Rect,
        scale: f64,
    ) -> Self {
        let global = local.translated(display_bounds.origin.x, display_bounds.origin.y);
        Self::from_global_rect(TargetKind::Region, Some(display_id), global, scale)
    }

    /// Maps a global position to `(x, y, outside)` in physical px of the recorded area.
    pub fn map(&self, gx: f64, gy: f64) -> (i32, i32, bool) {
        let px = (gx - self.origin[0]) * self.scale;
        let py = (gy - self.origin[1]) * self.scale;
        let clamp = |v: f64| v.round().clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32;
        let (x, y) = (clamp(px), clamp(py));
        let outside = x < 0
            || y < 0
            || i64::from(x) >= i64::from(self.size.width)
            || i64::from(y) >= i64::from(self.size.height);
        (x, y, outside)
    }

    /// The area's global rectangle.
    pub fn global_rect(&self) -> Rect {
        Rect {
            origin: Point::new(self.origin[0], self.origin[1]),
            size: cm_core::Size::new(
                f64::from(self.size.width) / self.scale,
                f64::from(self.size.height) / self.scale,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_maps_points_to_physical_px() {
        let t = CursorTarget::display(1, Rect::new(0.0, 0.0, 1512.0, 982.0), 2.0);
        assert_eq!(t.size.width, 3024);
        assert_eq!(t.map(100.0, 50.0), (200, 100, false));
        assert_eq!(t.map(1511.75, 981.0), (3024, 1962, true)); // rounds onto the edge: outside
        assert_eq!(t.map(1511.0, 981.0), (3022, 1962, false));
    }

    #[test]
    fn region_is_relative_to_the_region_not_the_display() {
        let display = Rect::new(0.0, 0.0, 1512.0, 982.0);
        let t =
            CursorTarget::region_on_display(1, display, Rect::new(100.0, 80.0, 420.0, 300.0), 2.0);
        assert_eq!((t.size.width, t.size.height), (840, 600));
        assert_eq!(t.map(100.0, 80.0), (0, 0, false));
        assert_eq!(t.map(110.0, 90.0), (20, 20, false));
        assert_eq!(t.map(99.0, 80.0), (-2, 0, true));
        assert_eq!(t.map(520.0, 100.0), (840, 40, true));
    }

    #[test]
    fn secondary_display_and_window_use_global_origin() {
        // Secondary display to the left of main (negative global x), 1x.
        let left = Rect::new(-1920.0, 0.0, 1920.0, 1080.0);
        let t = CursorTarget::display(2, left, 1.0);
        assert_eq!(t.map(-1920.0, 0.0), (0, 0, false));
        assert_eq!(t.map(-1.0, 10.0), (1919, 10, false));
        assert_eq!(t.map(0.0, 10.0), (1920, 10, true)); // moved onto the main display

        let win = CursorTarget::from_global_rect(
            TargetKind::Window,
            Some(1),
            Rect::new(300.0, 200.0, 800.0, 600.0),
            2.0,
        );
        assert_eq!(win.map(300.5, 200.0), (1, 0, false));
        assert_eq!(win.map(250.0, 150.0), (-100, -100, true));
    }

    #[test]
    fn target_round_trips_as_json() {
        let t = CursorTarget::display(7, Rect::new(0.0, 0.0, 100.0, 50.0), 2.0);
        let s = serde_json::to_string(&t).unwrap();
        assert!(s.contains("\"size\":[200,100]"));
        assert_eq!(serde_json::from_str::<CursorTarget>(&s).unwrap(), t);
    }
}
