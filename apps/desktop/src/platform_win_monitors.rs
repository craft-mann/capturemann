//! Windows monitors and window frames through the Win32 monitor APIs (`EnumDisplayMonitors`,
//! `GetMonitorInfoW`, `GetDpiForMonitor`, `GetCursorPos`, `SetWindowPos`), behind the functions
//! of `platform_window`. Every decision is made by the pure code in `recording::monitors`; this
//! file only reads the OS and moves windows.
//!
//! GPUI 0.3.9 on Windows names a display with its `HMONITOR` and puts it at `physical origin /
//! its own scale` in points, with window bounds absolute in those points. The process is
//! per-monitor DPI aware (GPUI sets it), so every rectangle read here is in physical px.

use gpui::Window;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::core::BOOL;
use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    ClientToScreen, EnumDisplayMonitors, GetMonitorInfoW, MonitorFromWindow, HDC, HMONITOR,
    MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClientRect, GetCursorPos, GetTopWindow, GetWindow, GetWindowLongPtrW, GetWindowRect,
    GetWindowThreadProcessId, IsWindow, SetLayeredWindowAttributes, SetWindowLongPtrW,
    SetWindowPos, GWL_EXSTYLE, GW_HWNDNEXT, HWND_TOPMOST, LWA_ALPHA, SWP_FRAMECHANGED,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
};

use crate::recording::geometry::Rect;
use crate::recording::monitors::{self, Monitor};

fn rect_px(r: &RECT) -> Rect {
    Rect::new(
        f64::from(r.left),
        f64::from(r.top),
        f64::from(r.right - r.left),
        f64::from(r.bottom - r.top),
    )
}

fn scale_of(h: HMONITOR) -> f64 {
    let (mut x, mut y) = (0u32, 0u32);
    // SAFETY: plain out-parameters on this stack frame.
    match unsafe { GetDpiForMonitor(h, MDT_EFFECTIVE_DPI, &mut x, &mut y) } {
        Ok(()) if x > 0 => f64::from(x) / 96.0,
        _ => 1.0,
    }
}

fn read(h: HMONITOR) -> Option<Monitor> {
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: `info` is initialised with its size, as the call requires.
    if !unsafe { GetMonitorInfoW(h, &mut info) }.as_bool() {
        return None;
    }
    Some(Monitor {
        id: u32::try_from(h.0 as usize as u64).ok()?,
        frame_px: rect_px(&info.rcMonitor),
        work_px: rect_px(&info.rcWork),
        scale: scale_of(h),
    })
}

unsafe extern "system" fn collect(h: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
    // SAFETY: `data` is the `Vec<Monitor>` `monitors()` passes and keeps alive for the call.
    let out = unsafe { &mut *(data.0 as *mut Vec<Monitor>) };
    if let Some(m) = read(h) {
        out.push(m);
    }
    true.into()
}

/// All monitors, in the order the system lists them.
pub fn monitors() -> Vec<Monitor> {
    let mut out: Vec<Monitor> = Vec::new();
    // SAFETY: the callback only runs during this call and `out` outlives it.
    let _ = unsafe {
        EnumDisplayMonitors(
            None,
            None,
            Some(collect),
            LPARAM(&mut out as *mut Vec<Monitor> as isize),
        )
    };
    out
}

fn by_id(id: u32) -> Option<Monitor> {
    read(HMONITOR(id as usize as *mut _))
}

/// The display's frame in GPUI points (`id` is GPUI's display id: the monitor handle).
pub fn display_frame(id: u32) -> Option<(f64, f64, f64, f64)> {
    let f = by_id(id)?.frame_points();
    Some((f.x, f.y, f.w, f.h))
}

/// The work area (without the taskbar) of the display whose point frame is centered on
/// `(x, y)`, else of the display holding that point, in GPUI points.
pub fn visible_frame_at(x: f64, y: f64) -> Option<(f64, f64, f64, f64)> {
    let ms = monitors();
    let same = |a: f64, b: f64| (a - b).abs() < 0.5;
    let i = ms
        .iter()
        .position(|m| {
            let c = m.frame_points().center();
            same(c.0, x) && same(c.1, y)
        })
        .or_else(|| monitors::index_for_point(&ms, x, y, None))?;
    let w = ms[i].work_points();
    Some((w.x, w.y, w.w, w.h))
}

/// The handle of the monitor holding the pointer (`GetCursorPos` is physical px).
pub fn pointer_display_id() -> Option<u32> {
    let mut p = POINT::default();
    // SAFETY: out-parameter on this stack frame.
    unsafe { GetCursorPos(&mut p) }.ok()?;
    let ms = monitors();
    let i = monitors::index_at_px(&ms, f64::from(p.x), f64::from(p.y))?;
    Some(ms[i].id)
}

/// The pointer in GPUI points.
pub fn mouse_location() -> Option<(f64, f64)> {
    let mut p = POINT::default();
    // SAFETY: out-parameter on this stack frame.
    unsafe { GetCursorPos(&mut p) }.ok()?;
    let ms = monitors();
    let i = monitors::index_at_px(&ms, f64::from(p.x), f64::from(p.y))?;
    Some(monitors::px_to_point(
        &ms[i],
        f64::from(p.x),
        f64::from(p.y),
    ))
}

pub fn left_button_down() -> bool {
    // SAFETY: reads the asynchronous key state; no pointers.
    let state = unsafe { GetAsyncKeyState(i32::from(VK_LBUTTON.0)) };
    state < 0
}

/// Every monitor by its handle (GPUI's display id) with its frame in physical px.
pub fn gpui_monitor_rects_px() -> Option<Vec<(u32, [i64; 4])>> {
    Some(
        monitors()
            .iter()
            .map(|m| {
                let r = m.frame_px;
                (m.id, [r.x as i64, r.y as i64, r.w as i64, r.h as i64])
            })
            .collect(),
    )
}

/// The handle of the monitor holding a recorded area (`[x, y, w, h]` physical px).
pub fn display_id_for_area(bounds: [i64; 4]) -> Option<u32> {
    let ms = monitors();
    monitors::index_for_area_px(&ms, bounds).map(|i| ms[i].id)
}

pub fn hwnd(window: &Window) -> Option<HWND> {
    let handle = HasWindowHandle::window_handle(window).ok()?;
    match handle.as_raw() {
        RawWindowHandle::Win32(h) => Some(HWND(h.hwnd.get() as *mut std::ffi::c_void)),
        _ => None,
    }
}

/// The monitor a window is on, by handle.
fn monitor_of(h: HWND) -> Option<Monitor> {
    // SAFETY: `h` is the live top-level window GPUI owns.
    let m = unsafe { MonitorFromWindow(h, MONITOR_DEFAULTTONEAREST) };
    read(m)
}

/// The client area's origin and the difference to the outer window rect, in physical px.
/// True when `h` is still a window of this thread. A deferred call (a task that runs tens of
/// milliseconds after the window opened) may run after the window closed; the handle is then
/// dead, or in the worst case reused by a window of someone else, and must not be touched.
/// The skip is logged at DEBUG with the reason.
fn usable(h: HWND, what: &str) -> bool {
    use crate::recording::zorder::{handle_state, HandleState};
    // SAFETY: `IsWindow` and `GetWindowThreadProcessId` accept any handle value.
    let (is_window, owner, me) = unsafe {
        (
            IsWindow(Some(h)).as_bool(),
            GetWindowThreadProcessId(h, None),
            GetCurrentThreadId(),
        )
    };
    let state = handle_state(is_window, owner, me);
    if state != HandleState::Live {
        log::debug!("{what}: skipped, window {:?} is {state:?}", h.0);
    }
    state == HandleState::Live
}

fn client_origin(h: HWND) -> Option<(RECT, RECT)> {
    let (mut outer, mut client) = (RECT::default(), RECT::default());
    let mut o = POINT::default();
    // SAFETY: out-parameters on this stack frame, `h` a live window.
    unsafe {
        GetWindowRect(h, &mut outer).ok()?;
        GetClientRect(h, &mut client).ok()?;
        if !ClientToScreen(h, &mut o).as_bool() {
            return None;
        }
    }
    let at = RECT {
        left: o.x,
        top: o.y,
        right: o.x + (client.right - client.left),
        bottom: o.y + (client.bottom - client.top),
    };
    Some((outer, at))
}

/// The window's client area as `(x, y, w, h)` in GPUI points: what GPUI calls its bounds.
pub fn frame(window: &Window) -> Option<(f64, f64, f64, f64)> {
    let h = hwnd(window)?;
    let (_, client) = client_origin(h)?;
    let m = monitor_of(h)?;
    let r = rect_px(&client);
    let s = m.scale.max(0.01);
    Some((r.x / s, r.y / s, r.w / s, r.h / s))
}

/// Moves the window so its client area's top-left is at `(x, y)` points.
pub fn move_to(window: &Window, x: f64, y: f64) {
    let Some(h) = hwnd(window) else { return };
    let Some((outer, client)) = client_origin(h) else {
        return;
    };
    let ms = monitors();
    let current = monitor_of(h).and_then(|m| ms.iter().position(|o| o.id == m.id));
    let Some(i) = monitors::index_for_point(&ms, x, y, current) else {
        return;
    };
    let (px, py) = (x * ms[i].scale, y * ms[i].scale);
    // The outer rect starts left/above the client area by the invisible frame.
    let (dx, dy) = (client.left - outer.left, client.top - outer.top);
    // SAFETY: `h` is a live window; the call only repositions it.
    let _ = unsafe {
        SetWindowPos(
            h,
            None,
            px.round() as i32 - dx,
            py.round() as i32 - dy,
            0,
            0,
            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        )
    };
}

/// Sets the client area to `(x, y, w, h)` points.
pub fn set_frame(window: &Window, x: f64, y: f64, w: f64, h_pt: f64) {
    let Some(h) = hwnd(window) else { return };
    set_frame_hwnd(h, x, y, w, h_pt);
}

/// [`set_frame`] for a window known by its handle, so it can be called later from a task that
/// holds no window borrow (`NativeWindow`). A handle that is gone makes the call fail quietly.
pub fn set_frame_hwnd(h: HWND, x: f64, y: f64, w: f64, h_pt: f64) {
    if !usable(h, "set_frame") {
        return;
    }
    let Some((outer, client)) = client_origin(h) else {
        return;
    };
    let ms = monitors();
    let current = monitor_of(h).and_then(|m| ms.iter().position(|o| o.id == m.id));
    let Some(i) = monitors::index_for_point(&ms, x + w / 2.0, y + h_pt / 2.0, current) else {
        return;
    };
    let s = ms[i].scale;
    let (dx, dy) = (client.left - outer.left, client.top - outer.top);
    let (extra_w, extra_h) = (
        (outer.right - outer.left) - (client.right - client.left),
        (outer.bottom - outer.top) - (client.bottom - client.top),
    );
    // SAFETY: `h` is a live window.
    let _ = unsafe {
        SetWindowPos(
            h,
            None,
            (x * s).round() as i32 - dx,
            (y * s).round() as i32 - dy,
            (w * s).round() as i32 + extra_w,
            (h_pt * s).round() as i32 + extra_h,
            SWP_NOZORDER | SWP_NOACTIVATE,
        )
    };
}

/// Makes a window ignore the mouse for good: `WS_EX_LAYERED | WS_EX_TRANSPARENT` (clicks fall
/// through to whatever is below, in any process), `WS_EX_NOACTIVATE`, `WS_EX_TOOLWINDOW`,
/// topmost; `SetLayeredWindowAttributes(255, LWA_ALPHA)` makes the layered window valid (a
/// layered window without it is not drawn at all). `UpdateLayeredWindow` is never used, so GPUI's
/// DirectComposition swap chain (`WS_EX_NOREDIRECTIONBITMAP`) keeps presenting: layered
/// top-level windows may be composed through DirectComposition from Windows 8 on.
pub fn make_click_through(h: HWND) -> bool {
    // SAFETY: `h` is a live top-level window of this process, used from its own thread.
    unsafe {
        let old = GetWindowLongPtrW(h, GWL_EXSTYLE) as u32;
        let new = crate::recording::zorder::area_ex_style(old);
        SetWindowLongPtrW(h, GWL_EXSTYLE, new as isize);
        let alpha = SetLayeredWindowAttributes(h, Default::default(), 255, LWA_ALPHA).is_ok();
        let _ = SetWindowPos(
            h,
            None,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
        let now = GetWindowLongPtrW(h, GWL_EXSTYLE) as u32;
        alpha && now & crate::recording::zorder::WS_EX_TRANSPARENT != 0
    }
}

/// Puts a window on top of the topmost band without activating, moving or resizing it.
pub fn raise_topmost(h: HWND) {
    if !usable(h, "raise_topmost") {
        return;
    }
    // SAFETY: `h` is a live window; the flags leave position, size and activation alone.
    let _ = unsafe {
        SetWindowPos(
            h,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        )
    };
}

/// One line per window: handle, rectangle (physical px), extended style and stack position
/// (0 = highest top-level window of the desktop).
pub fn describe_windows(wins: &[(&str, isize)]) -> Vec<String> {
    let mut stack: Vec<isize> = Vec::new();
    // SAFETY: plain window-list walk; handles may go stale, the calls then just fail.
    unsafe {
        let mut cur = GetTopWindow(None).ok();
        while let Some(c) = cur {
            stack.push(c.0 as isize);
            cur = GetWindow(c, GW_HWNDNEXT).ok();
            if stack.len() > 5000 {
                break;
            }
        }
    }
    let ids: Vec<isize> = wins.iter().map(|(_, h)| *h).collect();
    let z = crate::recording::zorder::z_indices(&stack, &ids);
    wins.iter()
        .zip(z)
        .map(|((name, id), z)| {
            let h = HWND(*id as *mut std::ffi::c_void);
            let mut r = RECT::default();
            // SAFETY: out-parameter on this frame.
            let ok = unsafe { GetWindowRect(h, &mut r) }.is_ok();
            // SAFETY: reading the style of a window handle.
            let ex = unsafe { GetWindowLongPtrW(h, GWL_EXSTYLE) } as u32;
            format!(
                "{name}: hwnd=0x{id:x} rect={}{} ex={} z={}",
                if ok {
                    format!(
                        "({},{} {}x{})",
                        r.left,
                        r.top,
                        r.right - r.left,
                        r.bottom - r.top
                    )
                } else {
                    "?".to_owned()
                },
                "px",
                crate::recording::zorder::describe_ex(ex),
                z.map_or("not found".to_owned(), |i| i.to_string()),
            )
        })
        .collect()
}
