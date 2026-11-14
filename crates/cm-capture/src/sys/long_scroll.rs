//! Auto scroll on the real system (CAP-6). Per system:
//!
//! * macOS: `CGEventPost` of a pixel-unit scroll-wheel event; needs the Accessibility permission
//!   (optional row of the permissions page, PRM-1.AC1b). Built here, **not run** (spike S1 needs
//!   the maintainer's consent: it moves the pointer's scroll in a real app).
//! * Windows: `SendInput` with `MOUSEEVENTF_WHEEL` / `MOUSEEVENTF_HWHEEL` (no permission). Run
//!   on the lab laptop (verification report of Nov 14, 2026), not on other Windows machines.
//! * Linux X11: XTest `fake_input` (buttons 4 to 7). Built, **not run** (needs the lab's Xorg
//!   session); `support()` is `Available` only when the XTEST extension answers.
//! * Wayland: no scroll injection exists, `NoInjection`; Hand scroll is the only mode.
//!
//! A wheel event goes to the window under the pointer, so the Windows and X11 scrollers move the
//! pointer to the `anchor` (global physical px, the centre of the area) before every scroll when
//! one is given.

use cm_core::CmError;

use crate::long_shot::{Direction, Scroller};

/// Whether Auto scroll can be offered on this system right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AutoScrollSupport {
    Available,
    /// macOS without Accessibility: shown disabled with "Allow Accessibility to scroll for you".
    NeedsAccessibility,
    /// Wayland: the system offers no way to inject scrolling; Auto scroll is hidden.
    NoInjection,
    /// Not built on this system yet.
    NotBuilt,
}

/// Whether this process is trusted for Accessibility (macOS; `true` elsewhere, where no such
/// permission exists). Asks nothing.
pub fn accessibility_trusted() -> bool {
    imp::trusted()
}

/// Asks macOS to show its Accessibility prompt (nothing elsewhere). Not run in tests.
pub fn request_accessibility() {
    imp::request_accessibility();
}

pub fn auto_scroll_support() -> AutoScrollSupport {
    imp::support()
}

/// The scroller of this system, or why there is none.
pub fn platform_scroller() -> Result<Box<dyn Scroller + Send>, CmError> {
    platform_scroller_at(None)
}

/// Like [`platform_scroller`]; `anchor` is a point of the area in global physical px, where the
/// pointer is put before each scroll (Windows, X11; macOS posts at the pointer's place).
pub fn platform_scroller_at(
    anchor: Option<(i32, i32)>,
) -> Result<Box<dyn Scroller + Send>, CmError> {
    match imp::support() {
        AutoScrollSupport::Available => imp::scroller(anchor),
        AutoScrollSupport::NeedsAccessibility => Err(CmError::permission_missing(
            cm_core::Permission::Accessibility,
            "Allow Accessibility to scroll for you",
        )),
        AutoScrollSupport::NoInjection => Err(CmError::not_implemented(
            "Auto scroll on Wayland (use Hand scroll)",
        )),
        AutoScrollSupport::NotBuilt => Err(CmError::not_implemented("Auto scroll on this system")),
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::*;
    use std::ffi::c_void;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventCreateScrollWheelEvent2(
            source: *const c_void,
            units: u32,
            wheel_count: u32,
            w1: i32,
            w2: i32,
            w3: i32,
        ) -> *mut c_void;
        fn CGEventPost(tap: u32, event: *mut c_void);
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFRelease(cf: *const c_void);
    }
    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrusted() -> bool;
    }

    const UNIT_PIXEL: u32 = 0;
    const TAP_HID: u32 = 0;
    /// One event moves at most this many px; larger steps are sent as several.
    const MAX_EVENT_PX: i32 = 400;

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        static kAXTrustedCheckOptionPrompt: *const c_void;
        fn AXIsProcessTrustedWithOptions(options: *const c_void) -> bool;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        static kCFBooleanTrue: *const c_void;
        fn CFDictionaryCreate(
            allocator: *const c_void,
            keys: *const *const c_void,
            values: *const *const c_void,
            count: isize,
            key_callbacks: *const c_void,
            value_callbacks: *const c_void,
        ) -> *const c_void;
    }

    /// Asks macOS to show its Accessibility prompt (it adds the app to the list and offers to
    /// open System Settings). Not run in tests.
    pub fn request_accessibility() {
        // SAFETY: both keys and values are CoreFoundation constants that outlive the dictionary
        // (null callbacks: it does not retain them); the dictionary is released after the call.
        unsafe {
            let keys = [kAXTrustedCheckOptionPrompt];
            let values = [kCFBooleanTrue];
            let dict = CFDictionaryCreate(
                std::ptr::null(),
                keys.as_ptr(),
                values.as_ptr(),
                1,
                std::ptr::null(),
                std::ptr::null(),
            );
            if !dict.is_null() {
                let _ = AXIsProcessTrustedWithOptions(dict);
                CFRelease(dict);
            }
        }
    }

    pub fn trusted() -> bool {
        // SAFETY: argument-less query, no preconditions.
        unsafe { AXIsProcessTrusted() }
    }

    pub fn support() -> AutoScrollSupport {
        if trusted() {
            AutoScrollSupport::Available
        } else {
            AutoScrollSupport::NeedsAccessibility
        }
    }

    struct MacScroller;

    impl Scroller for MacScroller {
        fn scroll(&mut self, direction: Direction, amount_px: u32) -> Result<(), CmError> {
            let mut left = i32::try_from(amount_px).unwrap_or(i32::MAX);
            while left > 0 {
                let part = left.min(MAX_EVENT_PX);
                left -= part;
                // Wheel 1 is vertical, wheel 2 horizontal. Positive is up / left, so the
                // content moves forward (down / right) with a negative value.
                let (w1, w2) = match direction {
                    Direction::Vertical => (-part, 0),
                    Direction::Horizontal => (0, -part),
                };
                // SAFETY: a null source is allowed; the event is released after posting.
                unsafe {
                    let ev =
                        CGEventCreateScrollWheelEvent2(std::ptr::null(), UNIT_PIXEL, 2, w1, w2, 0);
                    if ev.is_null() {
                        return Err(CmError::internal("could not create a scroll event"));
                    }
                    CGEventPost(TAP_HID, ev);
                    CFRelease(ev);
                }
            }
            Ok(())
        }
    }

    pub fn scroller(_anchor: Option<(i32, i32)>) -> Result<Box<dyn Scroller + Send>, CmError> {
        Ok(Box::new(MacScroller))
    }
}

/// Wheel "notches" for `amount_px`: the number of whole notches (at least 1 for a non-zero
/// amount) and the pixels one notch is taken to move. The real distance differs per app and is
/// learned by the driver from the joins (`run_auto` scales its next request).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn notches_for(amount_px: u32, px_per_notch: u32) -> u32 {
    if amount_px == 0 {
        return 0;
    }
    ((f64::from(amount_px) / f64::from(px_per_notch.max(1))).round() as u32).max(1)
}

/// `SendInput` wheel deltas for `amount_px`: 120 units are one notch (`WHEEL_DELTA`); the value
/// is split into events of at most `max_event` units, and a part of a notch is sent as such
/// (smooth-scrolling apps take it, the others round it).
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn wheel_deltas(amount_px: u32, px_per_notch: u32, max_event: u32) -> Vec<u32> {
    let total = (f64::from(amount_px) * 120.0 / f64::from(px_per_notch.max(1))).round() as u32;
    let max_event = max_event.max(1);
    let mut out = Vec::new();
    let mut left = total;
    while left > 0 {
        let part = left.min(max_event);
        out.push(part);
        left -= part;
    }
    out
}

#[cfg(target_os = "linux")]
mod imp {
    use super::session::{self, DisplayServer};
    use super::*;

    pub fn trusted() -> bool {
        true
    }

    pub fn request_accessibility() {}

    pub fn support() -> AutoScrollSupport {
        match session::from_env() {
            DisplayServer::Wayland => AutoScrollSupport::NoInjection,
            DisplayServer::X11 if super::super::x11_scroll::xtest_available() => {
                AutoScrollSupport::Available
            }
            _ => AutoScrollSupport::NotBuilt,
        }
    }

    pub fn scroller(anchor: Option<(i32, i32)>) -> Result<Box<dyn Scroller + Send>, CmError> {
        super::super::x11_scroll::scroller(anchor)
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::mem::size_of;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_HWHEEL, MOUSEEVENTF_WHEEL, MOUSEINPUT,
    };
    use windows::Win32::UI::WindowsAndMessaging::SetCursorPos;

    /// What one notch (120 units) is taken to move, in physical px. Chrome moves about 100 px
    /// at 100 % scale, classic apps three lines (about 48 px): the driver measures the real
    /// distance from the joins, so this only sets the first guess.
    const PX_PER_NOTCH: u32 = 100;
    /// Units per event: two notches, so a smooth-scrolling app sees a steady stream.
    const MAX_EVENT: u32 = 240;
    const EVENT_GAP: std::time::Duration = std::time::Duration::from_millis(4);

    pub fn trusted() -> bool {
        true
    }

    pub fn request_accessibility() {}

    pub fn support() -> AutoScrollSupport {
        AutoScrollSupport::Available
    }

    struct WinScroller {
        anchor: Option<(i32, i32)>,
    }

    impl Scroller for WinScroller {
        fn scroll(&mut self, direction: Direction, amount_px: u32) -> Result<(), CmError> {
            if let Some((x, y)) = self.anchor {
                // SAFETY: plain call with two integers; fails (and is ignored) only when the
                // desktop is locked, where the wheel event below fails as well.
                let _ = unsafe { SetCursorPos(x, y) };
            }
            let flag = match direction {
                Direction::Vertical => MOUSEEVENTF_WHEEL,
                Direction::Horizontal => MOUSEEVENTF_HWHEEL,
            };
            for delta in wheel_deltas(amount_px, PX_PER_NOTCH, MAX_EVENT) {
                // Vertical: a negative delta scrolls the content down (forward). Horizontal: a
                // positive delta scrolls it to the right (forward).
                let signed = match direction {
                    Direction::Vertical => -(delta as i32),
                    Direction::Horizontal => delta as i32,
                };
                let input = INPUT {
                    r#type: INPUT_MOUSE,
                    Anonymous: INPUT_0 {
                        mi: MOUSEINPUT {
                            dx: 0,
                            dy: 0,
                            mouseData: signed as u32,
                            dwFlags: flag,
                            time: 0,
                            dwExtraInfo: 0,
                        },
                    },
                };
                // SAFETY: one fully initialised INPUT and its true size.
                let sent = unsafe { SendInput(&[input], size_of::<INPUT>() as i32) };
                if sent != 1 {
                    return Err(CmError::internal(
                        "Windows did not accept the scroll (the screen may be locked or a program with higher rights is in front)",
                    ));
                }
                std::thread::sleep(EVENT_GAP);
            }
            Ok(())
        }
    }

    pub fn scroller(anchor: Option<(i32, i32)>) -> Result<Box<dyn Scroller + Send>, CmError> {
        Ok(Box::new(WinScroller { anchor }))
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
mod imp {
    use super::*;

    pub fn trusted() -> bool {
        true
    }

    pub fn request_accessibility() {}

    pub fn support() -> AutoScrollSupport {
        AutoScrollSupport::NotBuilt
    }

    pub fn scroller(_anchor: Option<(i32, i32)>) -> Result<Box<dyn Scroller + Send>, CmError> {
        Err(CmError::not_implemented("Auto scroll on this system"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notches_round_and_never_drop_to_zero() {
        assert_eq!(notches_for(0, 100), 0);
        assert_eq!(notches_for(10, 100), 1);
        assert_eq!(notches_for(480, 100), 5);
        assert_eq!(notches_for(480, 0), 480);
    }

    #[test]
    fn wheel_deltas_sum_to_the_request_in_small_events() {
        // 480 px at 100 px per notch is 4.8 notches = 576 units, in events of at most 240.
        let d = wheel_deltas(480, 100, 240);
        assert_eq!(d, vec![240, 240, 96]);
        assert_eq!(d.iter().sum::<u32>(), 576);
        assert!(wheel_deltas(0, 100, 240).is_empty());
    }
}
