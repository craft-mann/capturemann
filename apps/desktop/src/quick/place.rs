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

// ---- the Quick Capture bar (QC-1 "Size") ---------------------------------------------------------

/// The pieces the bar is drawn with (px). The window sizes come from them, so the drawing and
/// the placement cannot drift apart.
pub mod bar {
    /// A button: 44 wide, 48 high, with an 18 px icon and the key in 10 px mono under it.
    pub const CELL_W: f64 = 44.0;
    pub const CELL_H: f64 = 48.0;
    /// The gap between buttons of one group, and a recessed track's padding and border.
    pub const CELL_GAP: f64 = 2.0;
    pub const TRACK_PAD: f64 = 2.0;
    pub const TRACK_BORDER: f64 = 1.0;
    /// A divider: 1 px and 4 px each side.
    pub const SEP_W: f64 = 9.0;
    /// The logo with the Preview tag under it.
    pub const LOGO_W: f64 = 52.0;
    /// The action button, and the margin before it.
    pub const ACTION_W: f64 = 140.0;
    pub const ACTION_ML: f64 = 4.0;
    /// The expand/collapse toggle.
    pub const ICON_BTN_W: f64 = 38.0;
    /// The bar's padding (sides; top and bottom is `PAD_Y`), its border and the gap between its
    /// items.
    pub const PAD_X: f64 = 6.0;
    pub const PAD_Y: f64 = 2.0;
    pub const BORDER: f64 = 1.0;
    pub const GAP: f64 = 4.0;
    pub const TRACK_H: f64 = CELL_H + 2.0 * TRACK_PAD + 2.0 * TRACK_BORDER;

    /// A recessed track holding `n` buttons.
    pub const fn track_w(n: usize) -> f64 {
        n as f64 * CELL_W + (n - 1) as f64 * CELL_GAP + 2.0 * TRACK_PAD + 2.0 * TRACK_BORDER
    }
    /// A group of `n` buttons with no track.
    pub const fn group_w(n: usize) -> f64 {
        n as f64 * CELL_W + (n - 1) as f64 * CELL_GAP
    }
    const SIDES: f64 = 2.0 * (PAD_X + BORDER);
    /// Row one of the two-row bar: logo, kind, target.
    pub const ROW1_W: f64 = SIDES + LOGO_W + track_w(3) + SEP_W + track_w(4) + 3.0 * GAP;
    /// Row two: options, preset, action, toggle.
    pub const ROW2_W: f64 =
        SIDES + group_w(4) + SEP_W + CELL_W + ACTION_ML + ACTION_W + ICON_BTN_W + 4.0 * GAP;
    /// One row: everything, with a divider after the targets and the options and one before
    /// the preset; nine gaps.
    pub const ONE_ROW_W: f64 = SIDES
        + LOGO_W
        + track_w(3)
        + SEP_W
        + track_w(4)
        + SEP_W
        + group_w(4)
        + SEP_W
        + CELL_W
        + ACTION_ML
        + ACTION_W
        + ICON_BTN_W
        + 9.0 * GAP;
    /// The collapsed bar: logo, the kind track, a divider, the preset, the action and the
    /// toggle; five gaps. The same for every kind.
    pub const COLLAPSED_W: f64 = SIDES
        + LOGO_W
        + track_w(3)
        + SEP_W
        + CELL_W
        + ACTION_ML
        + ACTION_W
        + ICON_BTN_W
        + 5.0 * GAP;
}

/// The bar's width in one row. The same for Screenshot, Video and Audio, so switching the kind
/// never moves anything.
pub const BAR_W: f64 = bar::ONE_ROW_W;
/// The bar's height in one row.
pub const BAR_H: f64 = bar::TRACK_H + 2.0 * bar::PAD_Y + 2.0 * bar::BORDER;
/// The collapsed bar's width (one row, whatever the display), QC-1.AC21.
pub const COLLAPSED_W: f64 = bar::COLLAPSED_W;
/// The two-row bar: kind and target, then options and action.
pub const TWO_ROW_W: f64 = bar::ROW2_W;
pub const TWO_ROW_H: f64 = 2.0 * bar::TRACK_H + bar::GAP + 2.0 * bar::PAD_Y + 2.0 * bar::BORDER;
/// Room under the bar for the tooltip. The window always has it, so a tooltip is never cut at the
/// window's edge.
pub const TIP_ROOM: f64 = 40.0;
/// Gap between the bar and the popover (picker, value menu, name screen).
pub const POP_GAP: f64 = 6.0;
/// The popover's width.
pub const POP_W: f64 = 360.0;
/// The least display width, in points, that holds the bar in one row.
pub const ONE_ROW_MIN: f64 = 900.0;

/// Whether the bar splits into two rows on a display `display_w` points wide: below 900 points,
/// and wherever the one-row bar would not fit with its edge gaps.
pub fn two_rows(display_w: f64) -> bool {
    display_w < ONE_ROW_MIN.max(BAR_W + 2.0 * EDGE)
}

/// The bar itself (without the tooltip room) on a display `display_w` wide.
pub fn bar_size(display_w: f64) -> (f64, f64) {
    if two_rows(display_w) {
        (TWO_ROW_W, TWO_ROW_H)
    } else {
        (BAR_W, BAR_H)
    }
}

/// The window at rest: the bar and the tooltip room under it.
pub fn window_size(display_w: f64) -> (f64, f64) {
    window_size_view(display_w, false)
}

/// [`bar_size`] of the bar as it is shown: the collapsed bar is one row of a fixed width on
/// every display (the two rows are for the expanded bar only).
pub fn bar_size_view(display_w: f64, collapsed: bool) -> (f64, f64) {
    if collapsed {
        (COLLAPSED_W, BAR_H)
    } else {
        bar_size(display_w)
    }
}

/// Whether the pointer, at `(x, y)` in the window's own points, is over the empty strip under the
/// bar at rest: inside the window, below the bar (`bar_h` high, at the top of the window) and
/// with no popover open (the popover fills that room). Over the strip the window lets the mouse
/// through, so a click meant for the window below reaches it (release 0.3.7, macOS K5).
pub fn over_strip(pointer: (f64, f64), window: (f64, f64), bar_h: f64, popover_open: bool) -> bool {
    let (x, y) = pointer;
    !popover_open && x >= 0.0 && x < window.0 && y > bar_h && y < window.1
}

/// How often the hidden bar looks whether the run it started is over, in milliseconds.
pub const RETURN_POLL_MS: u64 = 100;
/// Quiet polls in a row (0.2 s) before the bar comes back after a screenshot: it is back as soon
/// as the capture is over.
pub const RETURN_CALM_SHOT: u32 = 2;
/// Quiet polls in a row (1.5 s) after a recording starts from the bar: the gap between the pick
/// and the start of the recording is shorter than that, and the bar must not flash in it.
pub const RETURN_CALM_RUN: u32 = 15;
/// Give up after about eight hours (a long recording); the bar then stays closed.
pub const RETURN_MAX_POLLS: u32 = 8 * 3600 * 10;

/// The count of quiet polls after one more look: any busy look starts it again.
pub fn calm_step(calm: u32, busy: bool) -> u32 {
    if busy {
        0
    } else {
        calm + 1
    }
}

/// [`window_size`] of the bar as it is shown, expanded or collapsed.
pub fn window_size_view(display_w: f64, collapsed: bool) -> (f64, f64) {
    let (w, h) = bar_size_view(display_w, collapsed);
    (w, h + TIP_ROOM)
}

/// The window at rest after the bar is collapsed or expanded: `size` is the new
/// [`window_size_view`]. The left and the top edge stay where they are; the window grows or
/// shrinks to the right and down, and where that would leave the display it is moved just far
/// enough to be fully on it (`movable`: the window can be moved; else it keeps its corner).
pub fn toggled_home(home: &Rect, size: (f64, f64), display: &Rect, movable: bool) -> Rect {
    let (w, h) = size;
    if !movable {
        return Rect::new(home.x, home.y, w, h);
    }
    let fit = |pos: f64, len: f64, lo: f64, span: f64| {
        let hi = lo + span - len - EDGE;
        pos.min(hi).max(lo + EDGE)
    };
    Rect::new(
        fit(home.x, w, display.x, display.w),
        fit(home.y, h, display.y, display.h),
        w,
        h,
    )
}

/// The height of the popover while the preset picker is open (QC-1 "Preset picker"): the fixed
/// parts (padding, the rule, the two links under the list) plus the rows, never more than `cap`
/// (the room the display leaves); beyond that the list scrolls.
///
/// `presets` is the number of presets; the list also has the "No preset" row. Row heights come
/// from the same constants the picker draws with ([`picker_list_height`]), not from a guess.
pub fn popover_height(presets: usize, cap: f64) -> f64 {
    (POPOVER_FIXED + picker_list_height(presets)).min(cap.max(POPOVER_MIN))
}

/// The least a popover is drawn with: the value menu and the name screen fit in it.
pub const POPOVER_MIN: f64 = 200.0;
/// The value menu's height and the name screen's height (with the room for one error line).
pub const MENU_H: f64 = 112.0;
pub const NAME_H: f64 = 214.0;

/// Everything in the picker's popover that is not the list: padding and border, the rule and
/// the two links under the list (8 + 8 padding, 2 border, 6 + 1 + 6 + 40 + 6 + 40).
pub const POPOVER_FIXED: f64 = 117.0;

/// Where the window goes while a popover of height `pop_h` is open: the bar stays where it is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PopoverPlace {
    /// The window's frame (global points).
    pub frame: Rect,
    /// The popover is above the bar (the display has no room under it).
    pub upward: bool,
    /// The popover's height: `pop_h`, or less when neither side has the room (it then scrolls).
    pub pop_h: f64,
}

/// The popover hangs under the bar. When the display has no room there it stands above the bar
/// instead (the window grows upward, so the bar does not move) if `movable` (the window can be
/// moved: macOS and Windows); else it keeps under the bar and takes the room there is. `home` is
/// the window at rest (the bar and the tooltip room), `bar_h` the bar's height.
pub fn popover_place(
    home: &Rect,
    bar_h: f64,
    pop_h: f64,
    display: &Rect,
    movable: bool,
) -> PopoverPlace {
    let below = display.y + display.h - EDGE - (home.y + bar_h + POP_GAP);
    let above = home.y - EDGE - POP_GAP;
    let down = |h: f64| PopoverPlace {
        frame: Rect::new(home.x, home.y, home.w, bar_h + POP_GAP + h.max(TIP_ROOM)),
        upward: false,
        pop_h: h,
    };
    let up = |h: f64| PopoverPlace {
        frame: Rect::new(home.x, home.y - POP_GAP - h, home.w, h + POP_GAP + home.h),
        upward: true,
        pop_h: h,
    };
    if pop_h <= below {
        down(pop_h)
    } else if movable && pop_h <= above {
        up(pop_h)
    } else if movable && above > below {
        up(above.max(0.0))
    } else {
        down(below.max(TIP_ROOM))
    }
}

/// Vertical padding of a picker row (top and bottom each), its border (each side) and the least
/// height of a row.
pub const ROW_PAD_Y: f64 = 6.0;
pub const ROW_BORDER: f64 = 1.0;
pub const ROW_MIN_H: f64 = 44.0;
/// Font sizes of a row's name and summary, and the line height GPUI gives text that sets none
/// (the golden ratio times the size).
pub const ROW_NAME_PX: f64 = 14.0;
pub const ROW_SUMMARY_PX: f64 = 11.0;
#[allow(clippy::approx_constant)]
pub const LINE_RATIO: f64 = 1.618_034;

/// The height of one picker row: its text lines plus padding and border, never under the least
/// height. A preset row has a summary line under the name, "No preset" has none.
pub fn picker_row_height(with_summary: bool) -> f64 {
    let lines = (ROW_NAME_PX + if with_summary { ROW_SUMMARY_PX } else { 0.0 }) * LINE_RATIO;
    (lines + 2.0 * ROW_PAD_Y + 2.0 * ROW_BORDER).max(ROW_MIN_H)
}

/// The list: "No preset" and `presets` rows with a summary.
pub fn picker_list_height(presets: usize) -> f64 {
    picker_row_height(false) + presets as f64 * picker_row_height(true)
}

/// X11: a pointer position in root-window pixels as GPUI points. GPUI sizes the X screen as
/// `root pixels / scale`, so the scale is the root width over the display width in points.
pub fn x11_pixels_to_points(p: (f64, f64), root_w: f64, display_w: f64) -> (f64, f64) {
    if root_w <= 0.0 || display_w <= 0.0 {
        return p;
    }
    let scale = root_w / display_w;
    (p.0 / scale, p.1 / scale)
}

/// QC-1 "Opening": the bar's default place, the middle of the top of the display (on its visible
/// part, under the menu bar), whatever the pointer does.
pub fn top_centre(w: f64, h: f64, display: &Rect) -> Rect {
    Rect::new(display.x + (display.w - w) / 2.0, display.y + EDGE, w, h)
}

/// QC-1 "Opening": where the panel's top edge centre goes. The pointer where the system tells it;
/// else (Wayland gives no position to a client) the panel is centred on the display.
pub fn open_point(pointer: Option<(f64, f64)>, panel_h: f64, display: &Rect) -> (f64, f64) {
    pointer.unwrap_or((
        display.x + display.w / 2.0,
        display.y + (display.h - panel_h) / 2.0,
    ))
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

/// QC-1 "Opening", from the tray: the panel is centred on the tray icon `(x, y, w, h)`, hangs
/// below it when the icon is in the upper half of the display (the menu bar) and stands above it
/// otherwise (the taskbar); always fully on the display.
pub fn tray_rect(icon: (f64, f64, f64, f64), w: f64, h: f64, display: &Rect) -> Rect {
    let (ix, iy, iw, ih) = icon;
    let x = (ix + iw / 2.0 - w / 2.0)
        .max(display.x + EDGE)
        .min(display.x + display.w - w - EDGE);
    let y = if iy + ih / 2.0 < display.y + display.h / 2.0 {
        iy + ih + BAR_GAP
    } else {
        iy - BAR_GAP - h
    }
    .max(display.y + EDGE)
    .min(display.y + display.h - h - EDGE);
    Rect::new(x, y, w, h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_place_is_the_middle_of_the_top() {
        let d = Rect::new(0.0, 25.0, 1440.0, 875.0);
        let r = top_centre(867.0, 100.0, &d);
        assert_eq!((r.x, r.y), (286.5, 25.0 + EDGE));
        assert_eq!((r.w, r.h), (867.0, 100.0));
    }

    #[test]
    fn the_bar_returns_only_after_a_quiet_stretch() {
        let mut calm = 0;
        for busy in [false, false, true, false, false, false, false, false, false] {
            calm = calm_step(calm, busy);
        }
        assert_eq!(calm, 6, "a busy look starts the count again");
        assert!(calm >= RETURN_CALM_SHOT && RETURN_CALM_SHOT < RETURN_CALM_RUN);
        assert_eq!(calm_step(5, true), 0);
    }

    #[test]
    fn only_the_empty_strip_under_the_bar_lets_clicks_through() {
        let win = (882.0, 101.0);
        // On the bar, on the strip, outside the window, with a popover open.
        assert!(!over_strip((100.0, 30.0), win, 60.0, false));
        assert!(over_strip((100.0, 80.0), win, 60.0, false));
        assert!(!over_strip((100.0, 120.0), win, 60.0, false));
        assert!(!over_strip((-3.0, 80.0), win, 60.0, false));
        assert!(!over_strip((900.0, 80.0), win, 60.0, false));
        assert!(!over_strip((100.0, 80.0), win, 60.0, true));
    }

    fn display() -> Rect {
        Rect::new(0.0, 0.0, 1440.0, 900.0)
    }

    /// QC-4.AC8: a row with its summary is about 55 px (measured on the real picker), not 48, so
    /// four starters and a "No preset" row all fit.
    #[test]
    fn a_picker_row_is_as_tall_as_its_two_lines() {
        let row = picker_row_height(true);
        assert!((54.0..=56.0).contains(&row), "{row}");
        assert_eq!(picker_row_height(false), ROW_MIN_H);
        // The four starters: every row is inside the popover the picker asks for.
        let h = popover_height(4, 876.0);
        assert!(h - POPOVER_FIXED >= picker_list_height(4));
    }

    /// QC-1.AC1: the bar is 60 px high and one width for every kind; the window at rest adds
    /// the tooltip room.
    #[test]
    fn qc1_ac1_the_bar_is_one_slim_row() {
        assert_eq!(BAR_H, 60.0);
        assert_eq!(BAR_W, 867.0);
        assert_eq!(bar::track_w(3), 142.0);
        assert_eq!(bar::track_w(4), 188.0);
        assert_eq!(window_size(1920.0), (BAR_W, BAR_H + TIP_ROOM));
        // The same size whatever the kind: the window size has no kind in it.
        assert_eq!(bar_size(1440.0), (BAR_W, BAR_H));
    }

    /// QC-1.AC1: below 900 points the bar splits into two rows (and wherever the one row would
    /// not fit with its edge gaps); the two rows are narrower and taller.
    #[test]
    fn qc1_ac1_two_rows_on_a_narrow_display() {
        assert!(two_rows(899.0));
        assert!(two_rows(800.0));
        assert!(
            two_rows(BAR_W + 2.0 * EDGE - 1.0),
            "one row must not touch the edge"
        );
        assert!(!two_rows(ONE_ROW_MIN.max(BAR_W + 2.0 * EDGE)));
        assert!(!two_rows(1440.0));
        let (w1, h1) = bar_size(1440.0);
        let (w2, h2) = bar_size(800.0);
        assert!(w2 < w1 && h2 > h1);
        assert!(bar::ROW1_W <= w2, "row one fits the two-row width");
        assert!(
            w2 + 2.0 * EDGE <= 800.0,
            "the two rows fit a narrow display"
        );
    }

    /// QC-1.AC21: the collapsed bar is one row of one fixed width on every display, narrower than
    /// the expanded bar, as high as the one-row bar and never two rows.
    #[test]
    fn qc1_ac21_the_collapsed_bar_is_one_fixed_row() {
        assert_eq!(COLLAPSED_W, 463.0);
        for w in [600.0, 800.0, 899.0, 1440.0, 3000.0] {
            assert_eq!(bar_size_view(w, true), (COLLAPSED_W, BAR_H), "{w}");
            assert_eq!(window_size_view(w, true), (COLLAPSED_W, BAR_H + TIP_ROOM));
            assert_eq!(window_size_view(w, false), window_size(w));
        }
        const {
            assert!(COLLAPSED_W < BAR_W);
            // The popovers are as wide as the window, and it fits a narrow display.
            assert!(COLLAPSED_W >= POP_W);
            assert!(COLLAPSED_W + 2.0 * EDGE <= 800.0);
        }
    }

    /// QC-1.AC21: toggling keeps the left edge (and the top) where they are and the window fully
    /// on the display; where the window cannot be moved it keeps its corner.
    #[test]
    fn qc1_ac21_toggling_keeps_the_bar_on_the_display() {
        let d = display();
        let (cw, ch) = window_size_view(d.w, true);
        let (ew, eh) = window_size_view(d.w, false);
        // In the middle: the left edge stays, the window grows to the right.
        let small = Rect::new(300.0, 200.0, cw, ch);
        let big = toggled_home(&small, (ew, eh), &d, true);
        assert_eq!((big.x, big.y), (300.0, 200.0));
        assert_eq!((big.w, big.h), (ew, eh));
        // Near the right edge: moved left just enough.
        let small = Rect::new(1100.0, 200.0, cw, ch);
        let big = toggled_home(&small, (ew, eh), &d, true);
        assert_eq!(big.x + big.w, d.x + d.w - EDGE);
        assert!(big.x < small.x);
        // Collapsing never needs a move.
        let back = toggled_home(&big, (cw, ch), &d, true);
        assert_eq!((back.x, back.y), (big.x, big.y));
        // The taller two-row bar near the bottom of a narrow display: moved up to stay on it.
        let narrow = Rect::new(0.0, 0.0, 800.0, 600.0);
        let (tw, th) = window_size(narrow.w);
        let low = Rect::new(100.0, 600.0 - EDGE - ch, cw, ch);
        let tall = toggled_home(&low, (tw, th), &narrow, true);
        assert!(tall.y + tall.h <= narrow.y + narrow.h - EDGE + 0.001);
        assert!(tall.x + tall.w <= narrow.x + narrow.w - EDGE + 0.001);
        // Not movable (Linux): the corner stays.
        let big = toggled_home(&Rect::new(1100.0, 200.0, cw, ch), (ew, eh), &d, false);
        assert_eq!((big.x, big.y), (1100.0, 200.0));
        // Always fully on the display for any start.
        for x in [0.0, 50.0, 700.0, 1400.0] {
            for y in [0.0, 400.0, 880.0] {
                let r = toggled_home(&Rect::new(x, y, cw, ch), (ew, eh), &d, true);
                assert!(r.x >= d.x && r.x + r.w <= d.x + d.w, "{r:?}");
                assert!(r.y >= d.y && r.y + r.h <= d.y + d.h, "{r:?}");
            }
        }
    }

    #[test]
    fn the_pointer_on_x11_becomes_points() {
        assert_eq!(
            x11_pixels_to_points((300.0, 200.0), 1920.0, 1920.0),
            (300.0, 200.0)
        );
        assert_eq!(
            x11_pixels_to_points((300.0, 200.0), 3840.0, 1920.0),
            (150.0, 100.0)
        );
        assert_eq!(x11_pixels_to_points((5.0, 6.0), 0.0, 1920.0), (5.0, 6.0));
    }

    /// QC-1 "Opening": no pointer (Wayland) centres the bar; a pointer is used as it is.
    #[test]
    fn without_a_pointer_the_panel_is_centred() {
        let d = display();
        let (w, h) = window_size(d.w);
        let (x, y) = open_point(None, h, &d);
        let r = panel_rect((x, y), w, h, &d);
        assert_eq!(r.x + r.w / 2.0, 720.0);
        assert_eq!(r.y + r.h / 2.0, 450.0);
        assert_eq!(open_point(Some((300.0, 300.0)), h, &d), (300.0, 300.0));
    }

    #[test]
    fn the_picker_grows_to_its_rows_and_stops_at_the_room() {
        assert_eq!(
            popover_height(0, 876.0),
            POPOVER_FIXED + picker_list_height(0)
        );
        let six = POPOVER_FIXED + picker_list_height(6);
        assert_eq!(popover_height(6, 876.0), six);
        assert_eq!(popover_height(40, 500.0), 500.0, "capped, the list scrolls");
        assert_eq!(
            popover_height(40, 100.0),
            POPOVER_MIN,
            "a tiny room keeps the least"
        );
    }

    /// QC-1 "Preset picker": the popover hangs under the bar; near the bottom it stands above the
    /// bar (the bar does not move) where the window can be moved, else it takes the room under.
    #[test]
    fn the_popover_fits_the_display() {
        let d = display();
        let (w, h) = window_size(d.w);
        let home = Rect::new(200.0, 100.0, w, h);
        let p = popover_place(&home, BAR_H, 300.0, &d, true);
        assert!(!p.upward);
        assert_eq!((p.frame.x, p.frame.y), (home.x, home.y), "the bar stays");
        assert_eq!(p.frame.h, BAR_H + POP_GAP + 300.0);
        assert!(p.frame.y + p.frame.h <= d.h - EDGE);
        // Near the bottom: above the bar, the bar's own screen position unchanged.
        let low = Rect::new(200.0, 760.0, w, h);
        let p = popover_place(&low, BAR_H, 300.0, &d, true);
        assert!(p.upward);
        assert_eq!(p.frame.y + p.pop_h + POP_GAP, low.y, "the bar stays");
        assert_eq!(p.frame.y + p.frame.h, low.y + low.h);
        assert!(p.frame.y >= d.y + EDGE);
        // Not movable (Linux): under the bar, the list takes the room there is.
        let p = popover_place(&low, BAR_H, 300.0, &d, false);
        assert!(!p.upward);
        assert!(p.pop_h < 300.0);
        assert!(p.frame.y + p.frame.h <= d.h - EDGE + TIP_ROOM, "{p:?}");
        // A long list on a short display is capped on the larger side.
        let p = popover_place(&home, BAR_H, 2000.0, &d, true);
        assert!(p.frame.y >= d.y && p.frame.y + p.frame.h <= d.h);
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

    /// QC-1: the bar opens at the point, clamped to the display.
    #[test]
    fn the_panel_is_clamped_to_the_display() {
        let d = display();
        let (w, h) = window_size(d.w);
        let p = panel_rect((1430.0, 880.0), w, h, &d);
        assert!(p.x + p.w <= d.w && p.y + p.h <= d.h);
        let q = panel_rect((720.0, 4.0), w, h, &d);
        assert_eq!(q.y, EDGE);
        assert_eq!(q.x, 720.0 - w / 2.0);
    }

    /// QC-1: from the tray the bar is on the icon: under it in a menu bar, above it over a
    /// taskbar, and kept on the display at the corner.
    #[test]
    fn the_tray_panel_sits_at_the_icon() {
        let d = display();
        let (w, h) = window_size(d.w);
        let top = tray_rect((1200.0, 0.0, 22.0, 24.0), w, h, &d);
        assert_eq!(top.y, 24.0 + BAR_GAP);
        assert!(top.x + top.w <= d.w - EDGE + 0.1);
        let bottom = tray_rect((1300.0, 870.0, 20.0, 30.0), w, h, &d);
        assert_eq!(bottom.y, 870.0 - BAR_GAP - h);
        assert!(bottom.y + bottom.h <= d.h);
        let mid = tray_rect((700.0, 0.0, 20.0, 24.0), w, h, &d);
        assert_eq!(mid.x + mid.w / 2.0, 710.0);
    }
}
