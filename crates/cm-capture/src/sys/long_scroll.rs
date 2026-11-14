//! Auto scroll on the real system (CAP-6). Per system:
//!
//! * macOS: `CGEventPost` of a pixel-unit scroll-wheel event; needs the Accessibility permission
//!   (optional row of the permissions page, PRM-1.AC1b). Built here, **not run** (spike S1 needs
//!   the maintainer's consent: it moves the pointer's scroll in a real app).
//! * Windows (`SendInput` with `MOUSEEVENTF_WHEEL` / `HWHEEL`, no permission) and Linux X11
//!   (XTest `fake_input`): not built yet, `NotBuilt`; both are spike S1 on the lab laptop.
//! * Wayland: no scroll injection exists, `NoInjection`; Hand scroll is the only mode.

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
    match imp::support() {
        AutoScrollSupport::Available => imp::scroller(),
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

    pub fn scroller() -> Result<Box<dyn Scroller + Send>, CmError> {
        Ok(Box::new(MacScroller))
    }
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
            // XTest is spike S1 (L1).
            _ => AutoScrollSupport::NotBuilt,
        }
    }

    pub fn scroller() -> Result<Box<dyn Scroller + Send>, CmError> {
        Err(CmError::not_implemented("Auto scroll on X11 (XTest)"))
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
mod imp {
    use super::*;

    pub fn trusted() -> bool {
        true
    }

    pub fn request_accessibility() {}

    pub fn support() -> AutoScrollSupport {
        // `SendInput` is spike S1 (W1).
        AutoScrollSupport::NotBuilt
    }

    pub fn scroller() -> Result<Box<dyn Scroller + Send>, CmError> {
        Err(CmError::not_implemented(
            "Auto scroll on Windows (SendInput)",
        ))
    }
}
