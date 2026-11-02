//! Capture of one window on Windows. xcap's own window capture takes the whole window rectangle (with the
//! invisible resize border), scales it to the size of the visible frame and then crops at the border's offset, so
//! its picture was a slightly shrunk, shifted copy and 7 px narrower than the listed bounds (listed 754x512,
//! image 747x510 on a runner). This one asks the window to paint itself (`PrintWindow`) and cuts out exactly the
//! visible frame (`DWMWA_EXTENDED_FRAME_BOUNDS`), which is also what xcap lists as the window's bounds, so the
//! picture and the bounds agree in size and origin.

use std::ffi::c_void;

use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Dwm::DWMWA_CLOAKED;
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetWindowDC,
    ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP, HDC,
};
use windows::Win32::Storage::Xps::{PrintWindow, PRINT_WINDOW_FLAGS};
use windows::Win32::UI::WindowsAndMessaging::{GetWindowLongW, GetWindowRect, GWL_EXSTYLE};

/// `PW_RENDERFULLCONTENT`: also paints DirectX and XAML content.
const PW_RENDERFULLCONTENT: u32 = 2;

const WS_EX_TOOLWINDOW: u32 = 0x0000_0080;
const WS_EX_APPWINDOW: u32 = 0x0004_0000;
const WS_EX_NOACTIVATE: u32 = 0x0800_0000;

/// Whether a window is one a person would pick from a list of windows: the rule of Windows' own
/// Alt-Tab list. The taskbar, the desktop and the tray pop-ups are tool windows (`WS_EX_TOOLWINDOW`)
/// that do not ask to be listed (`WS_EX_APPWINDOW`); windows that never take the focus
/// (`WS_EX_NOACTIVATE`) and cloaked ones (on another virtual desktop, or hidden by the shell) are
/// not windows to capture either. xcap lists them all (the picker offered the 1920x72 taskbar as
/// "Windows Explorer").
pub fn is_pickable_style(ex_style: u32, cloaked: u32) -> bool {
    let app = ex_style & WS_EX_APPWINDOW != 0;
    cloaked == 0
        && (app || ex_style & WS_EX_TOOLWINDOW == 0)
        && (app || ex_style & WS_EX_NOACTIVATE == 0)
}

/// [`is_pickable_style`] for the window with this handle; a window that cannot be asked is kept.
pub fn is_pickable(id: u32) -> bool {
    let hwnd = HWND(id as usize as *mut c_void);
    let mut cloaked = 0u32;
    // SAFETY: the call only reads the window's style; a stale handle gives 0.
    let ex_style = unsafe { GetWindowLongW(hwnd, GWL_EXSTYLE) } as u32;
    // SAFETY: the call writes one u32 into `cloaked`, whose size is passed; a stale handle makes
    // it return an error that is ignored (the window then counts as not cloaked).
    let _ = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            &mut cloaked as *mut u32 as *mut c_void,
            std::mem::size_of::<u32>() as u32,
        )
    };
    is_pickable_style(ex_style, cloaked)
}

/// An RGBA8 picture of the visible frame of the window: `(width, height, pixels)`.
pub fn capture(id: u32) -> Result<(u32, u32, Vec<u8>), String> {
    let hwnd = HWND(id as usize as *mut c_void);
    let (mut outer, mut frame) = (RECT::default(), RECT::default());
    // SAFETY: both calls only write into the RECTs; a stale handle makes them return an error.
    unsafe {
        GetWindowRect(hwnd, &mut outer).map_err(|e| format!("GetWindowRect: {e}"))?;
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut frame as *mut RECT as *mut c_void,
            std::mem::size_of::<RECT>() as u32,
        )
        .map_err(|e| format!("DwmGetWindowAttribute: {e}"))?;
    }
    let (ow, oh) = (outer.right - outer.left, outer.bottom - outer.top);
    if ow <= 0 || oh <= 0 {
        return Err("the window has no size".into());
    }
    let full = print_window(hwnd, ow, oh)?;
    let (x0, y0, w, h) = crop_box(
        (outer.left, outer.top, outer.right, outer.bottom),
        (frame.left, frame.top, frame.right, frame.bottom),
    );
    let mut rgba = Vec::with_capacity(w as usize * h as usize * 4);
    for y in y0..y0 + h {
        let row = (y as usize * ow as usize + x0 as usize) * 4;
        for px in full[row..row + w as usize * 4].as_chunks::<4>().0 {
            // GDI gives B, G, R and an alpha it does not set; the picture is opaque.
            rgba.extend_from_slice(&[px[2], px[1], px[0], 255]);
        }
    }
    Ok((w as u32, h as u32, rgba))
}

/// Where the visible frame sits inside the window rectangle: `(x, y, width, height)` in the rectangle's own
/// pixels. A frame that is empty or not inside the rectangle gives the whole rectangle.
pub fn crop_box(outer: (i32, i32, i32, i32), frame: (i32, i32, i32, i32)) -> (i32, i32, i32, i32) {
    let inside = frame.2 > frame.0
        && frame.3 > frame.1
        && frame.0 >= outer.0
        && frame.1 >= outer.1
        && frame.2 <= outer.2
        && frame.3 <= outer.3;
    if inside {
        (
            frame.0 - outer.0,
            frame.1 - outer.1,
            frame.2 - frame.0,
            frame.3 - frame.1,
        )
    } else {
        (0, 0, outer.2 - outer.0, outer.3 - outer.1)
    }
}

/// Frees the GDI objects of one capture, in the right order.
struct Gdi {
    hwnd: HWND,
    window_dc: HDC,
    mem_dc: HDC,
    bitmap: HBITMAP,
}

impl Drop for Gdi {
    fn drop(&mut self) {
        // SAFETY: the handles were made by `print_window` and are freed once, here.
        unsafe {
            let _ = DeleteObject(self.bitmap.into());
            let _ = DeleteDC(self.mem_dc);
            ReleaseDC(Some(self.hwnd), self.window_dc);
        }
    }
}

/// The window painted into a `width` x `height` bitmap, as BGRA rows, top row first.
fn print_window(hwnd: HWND, width: i32, height: i32) -> Result<Vec<u8>, String> {
    // SAFETY: plain GDI calls on handles made here; the buffer is `width * height * 4` bytes, which is what
    // `GetDIBits` writes for a 32-bit top-down bitmap of that size.
    unsafe {
        let window_dc = GetWindowDC(Some(hwnd));
        if window_dc.is_invalid() {
            return Err("GetWindowDC failed".into());
        }
        let mem_dc = CreateCompatibleDC(Some(window_dc));
        let bitmap = CreateCompatibleBitmap(window_dc, width, height);
        let gdi = Gdi {
            hwnd,
            window_dc,
            mem_dc,
            bitmap,
        };
        if gdi.mem_dc.is_invalid() || gdi.bitmap.is_invalid() {
            return Err("cannot make a bitmap for the window".into());
        }
        let previous = SelectObject(gdi.mem_dc, gdi.bitmap.into());
        let painted =
            PrintWindow(hwnd, gdi.mem_dc, PRINT_WINDOW_FLAGS(PW_RENDERFULLCONTENT)).as_bool();
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut pixels = vec![0u8; width as usize * height as usize * 4];
        // The bitmap must not be selected into a DC while `GetDIBits` reads it.
        SelectObject(gdi.mem_dc, previous);
        let lines = GetDIBits(
            gdi.mem_dc,
            gdi.bitmap,
            0,
            height as u32,
            Some(pixels.as_mut_ptr().cast()),
            &mut info,
            DIB_RGB_COLORS,
        );
        if !painted {
            return Err("PrintWindow did not paint the window".into());
        }
        if lines != height {
            return Err(format!("GetDIBits returned {lines} of {height} rows"));
        }
        Ok(pixels)
    }
}

#[cfg(test)]
mod tests {
    use super::{crop_box, is_pickable_style};

    #[test]
    fn the_crop_is_the_visible_frame_inside_the_window_rectangle() {
        // The runner's Notepad: rectangle (104,104)-(872,623), frame (111,104)-(865,616).
        assert_eq!(
            crop_box((104, 104, 872, 623), (111, 104, 865, 616)),
            (7, 0, 754, 512)
        );
    }

    #[test]
    fn a_frame_that_does_not_fit_gives_the_whole_rectangle() {
        assert_eq!(crop_box((10, 10, 110, 60), (0, 0, 0, 0)), (0, 0, 100, 50));
        assert_eq!(
            crop_box((10, 10, 110, 60), (5, 10, 110, 60)),
            (0, 0, 100, 50)
        );
    }

    #[test]
    fn the_taskbar_and_the_desktop_are_not_pickable_but_app_windows_are() {
        // Shell_TrayWnd (taskbar) and Progman (desktop): tool windows, as read on Windows 11.
        assert!(!is_pickable_style(0x0000_0088, 0));
        assert!(!is_pickable_style(0x0020_0080, 0));
        // An ordinary window, and the CaptureMann window (WS_EX_APPWINDOW).
        assert!(is_pickable_style(0x0020_0100, 0));
        assert!(is_pickable_style(0x0024_0100, 0));
        // A tool window that asks to be listed stays; a cloaked or never-active window goes.
        assert!(is_pickable_style(0x0004_0080, 0));
        assert!(!is_pickable_style(0x0020_0100, 1));
        assert!(!is_pickable_style(0x0820_0080, 0));
        assert!(!is_pickable_style(0x0800_0000, 0));
    }
}
