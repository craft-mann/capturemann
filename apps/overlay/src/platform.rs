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

#[cfg(target_os = "macos")]
mod imp {
    use super::{Prepared, RawWindowHandle};
    use objc2_app_kit::{NSView, NSWindowCollectionBehavior, NSWindowSharingType};

    /// NSScreenSaverWindowLevel: above the menu bar and the Dock.
    const SCREEN_SAVER_LEVEL: isize = 1000;

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
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE,
    };

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

    pub fn prepare(_raw: RawWindowHandle) -> Result<Prepared, String> {
        Ok(Prepared {
            frame_points: None,
            excluded_from_capture: false,
        })
    }
}
