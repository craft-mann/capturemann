//! The invisible resize border of a Windows window. `GetWindowRect` (what xcap reports as the window's
//! position and size) includes a transparent border of a few pixels on the left, right and bottom, but the
//! captured image holds only the visible frame, so listed bounds were 7 px wider than the picture.
//! `DWMWA_EXTENDED_FRAME_BOUNDS` is the visible frame; the difference is the border. Both rectangles come
//! from the OS in the same units, so the border is applied to xcap's numbers as a difference.

/// How far the visible frame sits inside the window rectangle, in the OS's own pixels (never negative).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Insets {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

/// `outer` and `visible` as `(left, top, right, bottom)`. A visible frame that is not inside the window
/// rectangle (a window that is minimised, or a value the OS did not fill in) gives no insets.
pub fn insets_between(outer: (i32, i32, i32, i32), visible: (i32, i32, i32, i32)) -> Insets {
    let i = Insets {
        left: visible.0 - outer.0,
        top: visible.1 - outer.1,
        right: outer.2 - visible.2,
        bottom: outer.3 - visible.3,
    };
    if i.left < 0
        || i.top < 0
        || i.right < 0
        || i.bottom < 0
        || visible.2 <= visible.0
        || visible.3 <= visible.1
    {
        Insets::default()
    } else {
        i
    }
}

/// Position and size with the border taken off: `(x, y, width, height)`.
pub fn apply(i: Insets, x: i32, y: i32, w: u32, h: u32) -> (i32, i32, u32, u32) {
    let nw = (w as i64 - (i.left + i.right) as i64).max(1) as u32;
    let nh = (h as i64 - (i.top + i.bottom) as i64).max(1) as u32;
    (x + i.left, y + i.top, nw, nh)
}

/// The border of the window with this id (an `HWND`), or none off Windows or when the OS cannot say.
#[cfg(windows)]
pub fn of_window(id: u32) -> Option<Insets> {
    use std::ffi::c_void;
    use windows::Win32::Foundation::{HWND, RECT};
    use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
    use windows::Win32::UI::WindowsAndMessaging::GetWindowRect;

    let hwnd = HWND(id as usize as *mut c_void);
    let mut outer = RECT::default();
    let mut visible = RECT::default();
    // SAFETY: both calls only write into the RECTs above; a stale handle makes them return an error.
    unsafe {
        GetWindowRect(hwnd, &mut outer).ok()?;
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut visible as *mut RECT as *mut c_void,
            std::mem::size_of::<RECT>() as u32,
        )
        .ok()?;
    }
    Some(insets_between(
        (outer.left, outer.top, outer.right, outer.bottom),
        (visible.left, visible.top, visible.right, visible.bottom),
    ))
}

#[cfg(all(not(windows), not(target_os = "macos")))]
pub fn of_window(_id: u32) -> Option<Insets> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_invisible_border_is_taken_off_the_listed_bounds() {
        // A Windows 11 window: 7 px of border on the left, right and bottom, none on top.
        let i = insets_between((100, 100, 1130, 728), (107, 100, 1123, 721));
        assert_eq!(
            i,
            Insets {
                left: 7,
                top: 0,
                right: 7,
                bottom: 7
            }
        );
        assert_eq!(apply(i, 100, 100, 1030, 628), (107, 100, 1016, 621));
    }

    #[test]
    fn a_frame_outside_the_window_rectangle_gives_no_change() {
        let i = insets_between((100, 100, 200, 200), (90, 100, 200, 200));
        assert_eq!(i, Insets::default());
        assert_eq!(apply(i, 5, 6, 70, 80), (5, 6, 70, 80));
        // An empty visible frame (a minimised window) is ignored as well.
        assert_eq!(
            insets_between((0, 0, 100, 100), (0, 0, 0, 0)),
            Insets::default()
        );
    }

    #[test]
    fn a_border_larger_than_the_window_keeps_a_one_pixel_size() {
        let i = Insets {
            left: 6,
            top: 0,
            right: 6,
            bottom: 0,
        };
        assert_eq!(apply(i, 0, 0, 10, 10), (6, 0, 1, 10));
    }
}
