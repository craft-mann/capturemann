//! Where the Quick Capture windows go (QC-1 opening, QC-3 place). Pure geometry in logical
//! points; the callers pass the visible part of the display.

use crate::recording::geometry::Rect;

/// Gap between the captured area and the after-capture bar.
pub const BAR_GAP: f64 = 10.0;
/// Space kept between a window and the edge of the display.
pub const EDGE: f64 = 12.0;

/// QC-3 "Place": under the captured area, or above it when the bar would not fit fully on the
/// display below it (a capture near the bottom edge); when neither side has room it sits inside
/// the display's lower edge. Always fully on the display, horizontally centred on the area.
pub fn bar_rect(area: &Rect, bar_w: f64, bar_h: f64, display: &Rect) -> Rect {
    let w = bar_w.min(display.w - 2.0 * EDGE).max(1.0);
    let x = (area.x + area.w / 2.0 - w / 2.0)
        .max(display.x + EDGE)
        .min(display.x + display.w - w - EDGE);
    let below = area.y + area.h + BAR_GAP;
    let above = area.y - BAR_GAP - bar_h;
    let bottom_limit = display.y + display.h - EDGE;
    let y = if below + bar_h <= bottom_limit {
        below
    } else if above >= display.y + EDGE {
        above
    } else {
        // The area fills the display: inside it, at the lower edge.
        bottom_limit - bar_h
    };
    Rect::new(x, y, w, bar_h)
}

/// QC-1 "Opening": the panel opens under a point (the tray icon, or the pointer), kept on the
/// display. Near the top edge it hangs below the point; elsewhere it opens centred on it with the
/// top at the point.
pub fn panel_rect(point: (f64, f64), w: f64, h: f64, display: &Rect) -> Rect {
    let x = (point.0 - w / 2.0)
        .max(display.x + EDGE)
        .min(display.x + display.w - w - EDGE);
    let y = point
        .1
        .max(display.y + EDGE)
        .min(display.y + display.h - h - EDGE);
    Rect::new(x, y, w, h)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn display() -> Rect {
        Rect::new(0.0, 0.0, 1440.0, 900.0)
    }

    /// QC-3.AC1: under the area.
    #[test]
    fn qc3_ac1_bar_is_under_the_area() {
        let a = Rect::new(400.0, 200.0, 300.0, 200.0);
        let b = bar_rect(&a, 420.0, 52.0, &display());
        assert_eq!(b.y, 410.0);
        assert_eq!(b.x + b.w / 2.0, 550.0);
    }

    /// QC-3.AC7: near the bottom edge the bar is above the area and fully on the display.
    #[test]
    fn qc3_ac7_bar_is_above_near_the_bottom_edge() {
        let a = Rect::new(400.0, 700.0, 300.0, 180.0);
        let b = bar_rect(&a, 420.0, 52.0, &display());
        assert_eq!(b.y, 700.0 - BAR_GAP - 52.0);
        assert!(b.y >= 0.0 && b.y + b.h <= 900.0);
    }

    #[test]
    fn the_bar_stays_on_the_display_at_every_edge() {
        let d = display();
        for a in [
            Rect::new(0.0, 0.0, 1440.0, 900.0),
            Rect::new(1300.0, 100.0, 140.0, 800.0),
            Rect::new(-0.0, 850.0, 60.0, 50.0),
            Rect::new(1380.0, 0.0, 60.0, 40.0),
        ] {
            let b = bar_rect(&a, 420.0, 52.0, &d);
            assert!(b.x >= d.x && b.x + b.w <= d.x + d.w, "{a:?} {b:?}");
            assert!(b.y >= d.y && b.y + b.h <= d.y + d.h, "{a:?} {b:?}");
        }
    }

    /// QC-1: the panel opens at the point, clamped to the display.
    #[test]
    fn the_panel_is_clamped_to_the_display() {
        let d = display();
        let p = panel_rect((1430.0, 880.0), 352.0, 420.0, &d);
        assert!(p.x + p.w <= d.w && p.y + p.h <= d.h);
        let q = panel_rect((720.0, 4.0), 352.0, 420.0, &d);
        assert_eq!(q.y, EDGE);
        assert_eq!(q.x, 720.0 - 176.0);
    }
}
