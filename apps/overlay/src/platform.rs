//! The only module with platform calls and `unsafe`: raising the overlay above the menu bar
//! and hiding it from screen capture.
//!
//! * macOS: a borderless window at screen-saver level covering the screen's full frame
//!   (menu bar and notch strip included, not native fullscreen: SPIKES S2) with
//!   `NSWindow.sharingType = none` so ScreenCaptureKit does not see it (SPIKES S4/S6).
//! * Windows: borderless fullscreen is requested through winit; here the window gets
//!   `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)`. Compile-only: not built or run on
//!   Windows yet.

use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

/// What [`prepare_overlay_window`] did, for the trace log.
#[derive(Debug, Clone, PartialEq)]
pub struct Prepared {
    /// Frame the window was given in points (macOS), `None` elsewhere.
    pub frame_points: Option<(f64, f64, f64, f64)>,
    /// The window is excluded from screen capture.
    pub excluded_from_capture: bool,
}

/// Raises the window above everything, makes it cover its screen and hides it from capture.
pub fn prepare_overlay_window(window: &Window) -> Result<Prepared, String> {
    let handle = window.window_handle().map_err(|e| e.to_string())?;
    imp::prepare(handle.as_raw())
}

/// Blocks until the process `pid` has ended and returns `true`; `false` at once when it cannot be
/// watched (not Windows, or the process cannot be opened for another reason than being gone).
/// A process that is already gone counts as ended.
pub fn wait_for_process_exit(pid: u32) -> bool {
    imp::wait_for_process_exit(pid)
}

/// Asks Windows to make this window the foreground window with the keyboard focus, which a
/// process started by another process's hotkey is not given on its own (`SetForegroundWindow`
/// restrictions). Returns whether the window is the foreground window afterwards. A no-op
/// (`true`) on macOS and Linux, where the toolkit handles focus.
pub fn take_foreground(window: &Window) -> bool {
    match window.window_handle() {
        Ok(h) => imp::take_foreground(h.as_raw()),
        Err(_) => false,
    }
}

/// One line for the log: handle, rectangle, styles, display affinity and focus state (so a
/// hardware run can show what the picker windows look like without writing any screen content).
pub fn describe_window(window: &Window) -> String {
    match window.window_handle() {
        Ok(h) => imp::describe(h.as_raw()),
        Err(e) => format!("no window handle: {e}"),
    }
}

/// Whether Esc is down right now, whichever window has the keyboard (Windows; `false` elsewhere).
/// The picker's way out when Windows never gave it the keyboard focus.
pub fn escape_is_down() -> bool {
    imp::escape_is_down()
}

/// Ends the process now, without running exit handlers or waiting for other threads (Windows:
/// `TerminateProcess`; `ExitProcess` waits for the loader lock and can hang behind a thread that
/// is stuck in a driver). Elsewhere `std::process::exit`.
pub fn hard_exit(code: i32) -> ! {
    imp::hard_exit(code)
}

/// The names of the extended window styles that matter for a picker (pure; for the log).
pub fn describe_ex_style(ex: u32) -> String {
    const FLAGS: [(u32, &str); 8] = [
        (0x0000_0008, "TOPMOST"),
        (0x0000_0020, "TRANSPARENT"),
        (0x0000_0080, "TOOLWINDOW"),
        (0x0000_0100, "WINDOWEDGE"),
        (0x0004_0000, "APPWINDOW"),
        (0x0008_0000, "LAYERED"),
        (0x0800_0000, "NOACTIVATE"),
        (0x0020_0000, "NOREDIRECTIONBITMAP"),
    ];
    let names: Vec<&str> = FLAGS
        .iter()
        .filter(|(bit, _)| ex & bit != 0)
        .map(|(_, n)| *n)
        .collect();
    format!("0x{ex:08x} [{}]", names.join("|"))
}

/// The meaning of a display affinity value (pure; for the log).
pub fn describe_affinity(a: u32) -> &'static str {
    match a {
        0 => "NONE (visible to capture)",
        1 => "MONITOR (black in capture)",
        0x11 => "EXCLUDEFROMCAPTURE (absent from capture)",
        _ => "unknown",
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::{Prepared, RawWindowHandle};
    use objc2_app_kit::{NSView, NSWindowCollectionBehavior, NSWindowSharingType};

    /// NSScreenSaverWindowLevel: above the menu bar and the Dock.
    const SCREEN_SAVER_LEVEL: isize = 1000;

    pub fn wait_for_process_exit(_pid: u32) -> bool {
        false
    }

    pub fn take_foreground(_raw: RawWindowHandle) -> bool {
        true
    }

    pub fn describe(_raw: RawWindowHandle) -> String {
        "AppKit window".into()
    }

    pub fn escape_is_down() -> bool {
        false
    }

    pub fn hard_exit(code: i32) -> ! {
        std::process::exit(code)
    }

    pub fn prepare(raw: RawWindowHandle) -> Result<Prepared, String> {
        let RawWindowHandle::AppKit(h) = raw else {
            return Err("not an AppKit window".into());
        };
        // SAFETY: `ns_view` is the live NSView winit owns for the lifetime of the `Window`
        // that produced this handle; we only borrow it for this call, on the main thread
        // (winit calls us from its event loop callbacks).
        let view: &NSView = unsafe { h.ns_view.cast::<NSView>().as_ref() };
        let ns_window = view.window().ok_or("view has no NSWindow")?;
        let screen = ns_window
            .screen()
            .ok_or("the window is not on any screen")?;
        let frame = screen.frame();
        ns_window.setLevel(SCREEN_SAVER_LEVEL);
        ns_window.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::Stationary
                | NSWindowCollectionBehavior::IgnoresCycle
                | NSWindowCollectionBehavior::FullScreenAuxiliary,
        );
        ns_window.setHasShadow(false);
        // Not captured by ScreenCaptureKit, so the daemon's region capture and a recording
        // never contain the overlay.
        ns_window.setSharingType(NSWindowSharingType::None);
        ns_window.setFrame_display(frame, true);
        Ok(Prepared {
            frame_points: Some((
                frame.origin.x,
                frame.origin.y,
                frame.size.width,
                frame.size.height,
            )),
            excluded_from_capture: ns_window.sharingType() == NSWindowSharingType::None,
        })
    }
}

#[cfg(windows)]
mod imp {
    use super::{Prepared, RawWindowHandle};
    use windows_sys::Win32::Foundation::{HWND, RECT};
    use windows_sys::Win32::System::Threading::{
        AttachThreadInput, GetCurrentProcess, GetCurrentThreadId, TerminateProcess,
    };
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetActiveWindow, GetAsyncKeyState, GetFocus, SetFocus, VK_ESCAPE,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        BringWindowToTop, GetForegroundWindow, GetWindowDisplayAffinity, GetWindowLongW,
        GetWindowRect, GetWindowThreadProcessId, IsWindowVisible, SetForegroundWindow,
        SetWindowDisplayAffinity, GWL_EXSTYLE, GWL_STYLE, WDA_EXCLUDEFROMCAPTURE,
    };

    pub fn wait_for_process_exit(pid: u32) -> bool {
        use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, WAIT_OBJECT_0};
        use windows_sys::Win32::System::Threading::{
            OpenProcess, WaitForSingleObject, INFINITE, PROCESS_SYNCHRONIZE,
        };
        const ERROR_INVALID_PARAMETER: u32 = 87; // no process has this id any more
                                                 // SAFETY: `OpenProcess` takes plain values; a null result is an error, checked below.
        let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
        if handle.is_null() {
            // SAFETY: reads the calling thread's last error, right after the failed call.
            return unsafe { GetLastError() } == ERROR_INVALID_PARAMETER;
        }
        // SAFETY: `handle` is a live process handle that this function owns; it is waited on and
        // closed exactly once.
        let ended = unsafe { WaitForSingleObject(handle, INFINITE) } == WAIT_OBJECT_0;
        // SAFETY: as above.
        unsafe { CloseHandle(handle) };
        ended
    }

    fn hwnd_of(raw: RawWindowHandle) -> Option<HWND> {
        match raw {
            RawWindowHandle::Win32(h) => Some(h.hwnd.get() as HWND),
            _ => None,
        }
    }

    pub fn hard_exit(code: i32) -> ! {
        // SAFETY: ends this process; nothing runs afterwards.
        unsafe { TerminateProcess(GetCurrentProcess(), code as u32) };
        std::process::exit(code)
    }

    pub fn escape_is_down() -> bool {
        // SAFETY: reads the asynchronous key state; no pointers.
        let state = unsafe { GetAsyncKeyState(i32::from(VK_ESCAPE)) };
        state < 0
    }

    /// Foreground-lock workaround: join the input queue of the thread that owns the foreground,
    /// ask for the foreground, then leave that queue again.
    pub fn take_foreground(raw: RawWindowHandle) -> bool {
        let Some(hwnd) = hwnd_of(raw) else {
            return false;
        };
        // SAFETY: `hwnd` is the live window winit owns; the calls take plain values. The input
        // queue attachment is undone before returning.
        unsafe {
            let fg = GetForegroundWindow();
            if fg == hwnd {
                return true;
            }
            let this_thread = GetCurrentThreadId();
            let fg_thread = if fg.is_null() {
                0
            } else {
                GetWindowThreadProcessId(fg, std::ptr::null_mut())
            };
            let attached = fg_thread != 0
                && fg_thread != this_thread
                && AttachThreadInput(this_thread, fg_thread, 1) != 0;
            BringWindowToTop(hwnd);
            SetForegroundWindow(hwnd);
            SetFocus(hwnd);
            if attached {
                AttachThreadInput(this_thread, fg_thread, 0);
            }
            GetForegroundWindow() == hwnd
        }
    }

    pub fn describe(raw: RawWindowHandle) -> String {
        let Some(hwnd) = hwnd_of(raw) else {
            return "not a Win32 window".into();
        };
        // SAFETY: `hwnd` is the live window winit owns; the out-parameters are locals.
        unsafe {
            let mut rect: RECT = std::mem::zeroed();
            let got_rect = GetWindowRect(hwnd, &mut rect) != 0;
            let mut affinity = 0u32;
            let got_aff = GetWindowDisplayAffinity(hwnd, &mut affinity) != 0;
            let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
            let ex = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
            let fg = GetForegroundWindow();
            format!(
                "hwnd {:#x}, rect {}, visible {}, style 0x{style:08x}, ex-style {}, affinity {}, foreground {:#x} (this window: {}), focus {:#x}, active {:#x}",
                hwnd as usize,
                if got_rect {
                    format!(
                        "({},{}) {}x{}",
                        rect.left,
                        rect.top,
                        rect.right - rect.left,
                        rect.bottom - rect.top
                    )
                } else {
                    "unknown".into()
                },
                IsWindowVisible(hwnd) != 0,
                super::describe_ex_style(ex),
                if got_aff {
                    super::describe_affinity(affinity)
                } else {
                    "unreadable"
                },
                fg as usize,
                fg == hwnd,
                GetFocus() as usize,
                GetActiveWindow() as usize,
            )
        }
    }

    pub fn prepare(raw: RawWindowHandle) -> Result<Prepared, String> {
        let RawWindowHandle::Win32(h) = raw else {
            return Err("not a Win32 window".into());
        };
        let hwnd = h.hwnd.get() as windows_sys::Win32::Foundation::HWND;
        // SAFETY: `hwnd` is the live top-level window winit created and still owns; the call
        // only sets a display affinity flag on it. It fails (returns 0) before Windows 10
        // 2004, where the window then stays visible to capture; we report that.
        let ok = unsafe { SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE) } != 0;
        Ok(Prepared {
            frame_points: None,
            excluded_from_capture: ok,
        })
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
mod imp {
    use super::{Prepared, RawWindowHandle};

    pub fn wait_for_process_exit(_pid: u32) -> bool {
        false
    }

    pub fn take_foreground(_raw: RawWindowHandle) -> bool {
        true
    }

    pub fn describe(_raw: RawWindowHandle) -> String {
        "window".into()
    }

    pub fn escape_is_down() -> bool {
        false
    }

    pub fn hard_exit(code: i32) -> ! {
        std::process::exit(code)
    }

    pub fn prepare(_raw: RawWindowHandle) -> Result<Prepared, String> {
        Ok(Prepared {
            frame_points: None,
            excluded_from_capture: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_extended_styles_are_named() {
        assert_eq!(describe_ex_style(0), "0x00000000 []");
        let s = describe_ex_style(0x0800_0008 | 0x0008_0000);
        assert!(s.contains("TOPMOST") && s.contains("NOACTIVATE") && s.contains("LAYERED"));
        assert!(!s.contains("TRANSPARENT"));
    }

    #[test]
    fn the_affinity_values_are_named() {
        assert!(describe_affinity(0x11).starts_with("EXCLUDEFROMCAPTURE"));
        assert!(describe_affinity(0).starts_with("NONE"));
        assert_eq!(describe_affinity(7), "unknown");
    }
}
