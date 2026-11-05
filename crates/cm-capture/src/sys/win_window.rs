//! Capture of one window on Windows. xcap's own window capture takes the whole window rectangle (with the
//! invisible resize border), scales it to the size of the visible frame and then crops at the border's offset, so
//! its picture was a slightly shrunk, shifted copy and 7 px narrower than the listed bounds (listed 754x512,
//! image 747x510 on a runner). This one asks the window to paint itself (`PrintWindow`) and cuts out exactly the
//! visible frame (`DWMWA_EXTENDED_FRAME_BOUNDS`), which is also what xcap lists as the window's bounds, so the
//! picture and the bounds agree in size and origin.

use std::ffi::c_void;

use windows::core::{BOOL, HSTRING, PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, RECT};
use windows::Win32::Graphics::Dwm::DWMWA_CLOAKED;
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetWindowDC,
    ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP, HDC,
};
use windows::Win32::Storage::FileSystem::{
    GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
};
use windows::Win32::Storage::Xps::{PrintWindow, PRINT_WINDOW_FLAGS};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumChildWindows, GetWindowLongW, GetWindowRect, GetWindowThreadProcessId, GWL_EXSTYLE,
};

use super::win_names::{choose_app_name, crop_box, is_frame_host, visible_xywh};

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

/// Makes this process per-monitor DPI aware (once). A process without a DPI manifest (the headless daemon,
/// the CLI, a test program) gets window rectangles scaled down by the display scale from `GetWindowRect`
/// and a `PrintWindow` bitmap of that smaller size, while `DWMWA_EXTENDED_FRAME_BOUNDS` is always physical:
/// on the lab laptop (150 %) a Notepad window listed as 1426x721 was captured as 960x485. The desktop app calls it first thing in `main` (it has no
/// manifest); when the awareness is already set the call fails and changes nothing.
pub fn ensure_dpi_aware() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        // SAFETY: a plain process-wide setting; it returns an error when the awareness is already set.
        unsafe {
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        }
    });
}

/// The visible frame of the window as `(x, y, width, height)` in physical px. This is the rectangle `capture`
/// cuts out of the painted window, so the size the picker lists is the size the saved picture has (the 0.2.6
/// picker listed 1426x720 for a window that was saved as 1426x721, and the two numbers came from two code
/// paths).
pub fn frame_bounds(id: u32) -> Option<(i32, i32, u32, u32)> {
    ensure_dpi_aware();
    let hwnd = HWND(id as usize as *mut c_void);
    let (mut outer, mut frame) = (RECT::default(), RECT::default());
    // SAFETY: both calls only write into the RECTs, whose sizes are passed; a stale handle makes them return an error.
    unsafe {
        GetWindowRect(hwnd, &mut outer).ok()?;
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut frame as *mut RECT as *mut c_void,
            std::mem::size_of::<RECT>() as u32,
        )
        .ok()?;
    }
    // The same rule `capture` cuts the picture with, so the listed size is the saved size even when the frame
    // is not inside the rectangle (a rounding difference at a fractional scale, a window between two monitors).
    visible_xywh(
        (outer.left, outer.top, outer.right, outer.bottom),
        (frame.left, frame.top, frame.right, frame.bottom),
    )
}

/// The full path of the program a process runs, when the process lets us ask.
fn exe_path_of(pid: u32) -> Option<String> {
    // SAFETY: the handle is opened and closed here; the buffer length is passed and updated by the call.
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let asked = QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut len,
        );
        let _ = CloseHandle(process);
        asked.ok()?;
        Some(String::from_utf16_lossy(&buf[..len as usize]))
    }
}

/// `FileDescription` and `ProductName` from the version resource of a program, in the order of its languages.
fn version_descriptions(path: &str) -> Vec<String> {
    let mut out = Vec::new();
    let wide = HSTRING::from(path);
    // SAFETY: the buffer is as large as `GetFileVersionInfoSizeW` says; `VerQueryValueW` returns pointers into
    // it that are read before it is dropped, with the lengths it reports.
    unsafe {
        let size = GetFileVersionInfoSizeW(PCWSTR(wide.as_ptr()), None);
        if size == 0 {
            return out;
        }
        let mut block = vec![0u8; size as usize];
        if GetFileVersionInfoW(PCWSTR(wide.as_ptr()), None, size, block.as_mut_ptr().cast())
            .is_err()
        {
            return out;
        }
        let (mut ptr, mut len) = (std::ptr::null_mut::<c_void>(), 0u32);
        let translation = HSTRING::from("\\VarFileInfo\\Translation");
        if !VerQueryValueW(block.as_ptr().cast(), &translation, &mut ptr, &mut len).as_bool()
            || ptr.is_null()
        {
            return out;
        }
        let pairs = std::slice::from_raw_parts(ptr as *const [u16; 2], len as usize / 4);
        for key in ["FileDescription", "ProductName"] {
            for [lang, page] in pairs {
                let query = HSTRING::from(format!("\\StringFileInfo\\{lang:04x}{page:04x}\\{key}"));
                let (mut text, mut chars) = (std::ptr::null_mut::<c_void>(), 0u32);
                if VerQueryValueW(block.as_ptr().cast(), &query, &mut text, &mut chars).as_bool()
                    && !text.is_null()
                    && chars > 0
                {
                    let units = std::slice::from_raw_parts(text as *const u16, chars as usize);
                    let end = units.iter().position(|&c| c == 0).unwrap_or(units.len());
                    out.push(String::from_utf16_lossy(&units[..end]));
                }
            }
        }
    }
    out
}

/// State of the child-window search in [`app_pid_of_frame`]: the frame host's pid, and the pid found.
struct ChildSearch {
    host: u32,
    found: u32,
}

unsafe extern "system" fn first_foreign_child(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: `lparam` is the `ChildSearch` that `app_pid_of_frame` passed and keeps alive during the call.
    let search = unsafe { &mut *(lparam.0 as *mut ChildSearch) };
    let mut pid = 0u32;
    // SAFETY: writes one u32.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if pid != 0 && pid != search.host {
        search.found = pid;
        return BOOL(0);
    }
    BOOL(1)
}

/// The process of the app that a packaged window shows: Windows runs every such window in
/// `ApplicationFrameHost.exe`, and the app's own process owns a child window inside it.
fn app_pid_of_frame(hwnd: HWND, host: u32) -> u32 {
    let mut search = ChildSearch { host, found: 0 };
    // SAFETY: the callback only reads and writes `search`, which outlives the call.
    unsafe {
        let _ = EnumChildWindows(
            Some(hwnd),
            Some(first_foreign_child),
            LPARAM(&mut search as *mut ChildSearch as isize),
        );
    }
    if search.found != 0 {
        search.found
    } else {
        host
    }
}

/// The name the picker shows for the app that owns a window: the description in the program's version
/// resource, else the program's file name, else the window title (see `win_names::choose_app_name`). For a
/// packaged app the program is the app's own, not `ApplicationFrameHost.exe`.
pub fn app_name(id: u32, title: &str) -> String {
    let hwnd = HWND(id as usize as *mut c_void);
    let mut pid = 0u32;
    // SAFETY: writes one u32; a stale handle leaves 0.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    let mut exe = exe_path_of(pid).unwrap_or_default();
    if is_frame_host(&exe) {
        let app = app_pid_of_frame(hwnd, pid);
        if app != pid {
            exe = exe_path_of(app).unwrap_or_default();
        } else {
            // The app's process is not reachable: the host's own name says nothing, the title does.
            exe.clear();
        }
    }
    let descriptions = if exe.is_empty() {
        Vec::new()
    } else {
        version_descriptions(&exe)
    };
    let refs: Vec<&str> = descriptions.iter().map(String::as_str).collect();
    choose_app_name(&refs, &exe, title)
}

/// An RGBA8 picture of the visible frame of the window: `(width, height, pixels)`.
pub fn capture(id: u32) -> Result<(u32, u32, Vec<u8>), String> {
    ensure_dpi_aware();
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
    use super::is_pickable_style;

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
