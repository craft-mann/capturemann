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

/// Brings the window with this id to the front, so that Auto scroll's wheel events reach it and
/// not a window that covers it (QA Windows pass 3: `nothing_scrolled` with a console window over
/// the target). Built on Windows only (`SetForegroundWindow`); on macOS, X11 and Wayland there is
/// no helper yet and this does nothing and returns `false`. Never run in tests.
pub fn raise_window(window_id: u32) -> bool {
    #[cfg(windows)]
    {
        super::win_window::raise(window_id)
    }
    #[cfg(not(windows))]
    {
        let _ = window_id;
        false
    }
}

/// Window classes of a Windows console: the classic console host (cmd.exe, PowerShell, WSL in
/// their own window) and Windows Terminal. They are the targets that get the per-target
/// fallbacks of [`Scroller::last_resort`] (a wheel message, then Page Down).
#[cfg_attr(not(windows), allow(dead_code))]
pub fn is_console_class(class: &str) -> bool {
    matches!(
        class,
        "ConsoleWindowClass" | "CASCADIA_HOSTING_WINDOW_CLASS"
    )
}

/// The scroller of this system, or why there is none.
pub fn platform_scroller() -> Result<Box<dyn Scroller + Send>, CmError> {
    platform_scroller_at(None)
}

/// Like [`platform_scroller`]; `anchor` is a point of the area in global physical px, where the
/// pointer is put before each scroll (all systems; macOS converts it to points).
pub fn platform_scroller_at(
    anchor: Option<(i32, i32)>,
) -> Result<Box<dyn Scroller + Send>, CmError> {
    platform_scroller_for(anchor, None)
}

/// Like [`platform_scroller_at`] for a window target (`window_id`): on Windows a console window
/// gets the fallbacks of [`Scroller::last_resort`]; every other target, and every other system,
/// stays on the wheel alone.
pub fn platform_scroller_for(
    anchor: Option<(i32, i32)>,
    window_id: Option<u32>,
) -> Result<Box<dyn Scroller + Send>, CmError> {
    match imp::support() {
        AutoScrollSupport::Available => imp::scroller(anchor, window_id),
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

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub(super) struct CGPoint {
        x: f64,
        y: f64,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct CGRect {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
    }
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGGetActiveDisplayList(max: u32, ids: *mut u32, count: *mut u32) -> i32;
        fn CGDisplayBounds(id: u32) -> CGRect;
        fn CGDisplayCopyDisplayMode(id: u32) -> *const c_void;
        fn CGDisplayModeGetPixelWidth(mode: *const c_void) -> usize;
        fn CGDisplayModeRelease(mode: *const c_void);
        fn CGWarpMouseCursorPosition(p: CGPoint) -> i32;
        fn CGAssociateMouseAndMouseCursorPosition(connected: bool) -> i32;
        fn CGEventSetLocation(ev: *mut c_void, p: CGPoint);
    }

    /// The anchor (global physical px) as a point in macOS's global point space: the display
    /// whose point bounds, scaled by its own backing scale, contain it decides the scale.
    pub(super) fn anchor_to_points(anchor: (i32, i32)) -> (CGPoint, f64) {
        let (ax, ay) = (f64::from(anchor.0), f64::from(anchor.1));
        let mut ids = [0u32; 16];
        let mut n = 0u32;
        // SAFETY: the buffer holds 16 ids; `n` receives how many were written.
        let ok = unsafe { CGGetActiveDisplayList(16, ids.as_mut_ptr(), &mut n) } == 0;
        let mut best: Option<(CGPoint, f64)> = None;
        let mut main_scale = 1.0;
        if ok {
            for (i, id) in ids.iter().take(n as usize).enumerate() {
                // SAFETY: `id` is an active display; the mode is released after use.
                let (b, scale) = unsafe {
                    let b = CGDisplayBounds(*id);
                    let mode = CGDisplayCopyDisplayMode(*id);
                    let px = if mode.is_null() {
                        0
                    } else {
                        let w = CGDisplayModeGetPixelWidth(mode);
                        CGDisplayModeRelease(mode);
                        w
                    };
                    let scale = if px > 0 && b.w > 0.0 {
                        px as f64 / b.w
                    } else {
                        1.0
                    };
                    (b, scale)
                };
                if i == 0 {
                    main_scale = scale;
                }
                let (x, y) = (ax / scale, ay / scale);
                if best.is_none() && x >= b.x && x < b.x + b.w && y >= b.y && y < b.y + b.h {
                    best = Some((CGPoint { x, y }, scale));
                }
            }
        }
        best.unwrap_or((
            CGPoint {
                x: ax / main_scale,
                y: ay / main_scale,
            },
            main_scale,
        ))
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

    struct MacScroller {
        /// Where the pointer is put before scrolling (points); the wheel goes to the window
        /// under it, so without this the content under the pointer's last place scrolls.
        anchor: Option<CGPoint>,
        /// Physical px per point of the display under the anchor: a pixel-unit wheel event
        /// counts points, `amount_px` is physical.
        scale: f64,
    }

    impl Scroller for MacScroller {
        fn scroll(&mut self, direction: Direction, amount_px: u32) -> Result<(), CmError> {
            self.wheel(direction, amount_px, true)
        }

        fn scroll_back(&mut self, direction: Direction, amount_px: u32) -> Result<bool, CmError> {
            self.wheel(direction, amount_px, false)?;
            Ok(true)
        }
    }

    impl MacScroller {
        /// `forward`: the content moves down or to the right; otherwise up or left.
        fn wheel(
            &mut self,
            direction: Direction,
            amount_px: u32,
            forward: bool,
        ) -> Result<(), CmError> {
            let sign = if forward { -1 } else { 1 };
            if let Some(p) = self.anchor {
                // SAFETY: plain calls with a value point; no pointers.
                unsafe {
                    CGAssociateMouseAndMouseCursorPosition(true);
                    CGWarpMouseCursorPosition(p);
                }
                std::thread::sleep(std::time::Duration::from_millis(60));
            }
            let mut left = ((f64::from(amount_px) / self.scale).round() as i32).max(1);
            while left > 0 {
                let part = left.min(MAX_EVENT_PX);
                left -= part;
                // Wheel 1 is vertical, wheel 2 horizontal. Positive is up / left, so the
                // content moves forward (down / right) with a negative value.
                let (w1, w2) = match direction {
                    Direction::Vertical => (sign * part, 0),
                    Direction::Horizontal => (0, sign * part),
                };
                // SAFETY: a null source is allowed; the event is released after posting.
                unsafe {
                    let ev =
                        CGEventCreateScrollWheelEvent2(std::ptr::null(), UNIT_PIXEL, 2, w1, w2, 0);
                    if ev.is_null() {
                        return Err(CmError::internal("could not create a scroll event"));
                    }
                    if let Some(p) = self.anchor {
                        CGEventSetLocation(ev, p);
                    }
                    CGEventPost(TAP_HID, ev);
                    CFRelease(ev);
                }
                if left > 0 {
                    std::thread::sleep(std::time::Duration::from_millis(8));
                }
            }
            Ok(())
        }
    }

    pub fn scroller(
        anchor: Option<(i32, i32)>,
        _window_id: Option<u32>,
    ) -> Result<Box<dyn Scroller + Send>, CmError> {
        let (anchor, scale) = match anchor.map(anchor_to_points) {
            Some((p, s)) => (Some(p), s),
            None => (None, 1.0),
        };
        Ok(Box::new(MacScroller { anchor, scale }))
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

/// `SendInput` wheel deltas for `amount_px`: 120 units are one notch (`WHEEL_DELTA`). Always
/// whole notches, at least one for any amount: a classic console window (conhost) scrolls three
/// lines per notch and ignores a part of a notch, so the 96 units that were the end of a 480 px
/// request did nothing there, and a first step of a few px did nothing at all (Windows QA pass
/// 6: `nothing_scrolled`). The value is split into events of at most `max_event` units (rounded
/// down to whole notches, at least one). The driver measures the distance that really moved
/// from the joins, so rounding the request costs nothing.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn wheel_deltas(amount_px: u32, px_per_notch: u32, max_event: u32) -> Vec<u32> {
    const NOTCH: u32 = 120;
    if amount_px == 0 {
        return Vec::new();
    }
    let notches = ((f64::from(amount_px) / f64::from(px_per_notch.max(1))).round() as u32).max(1);
    let per_event = (max_event / NOTCH).max(1);
    let mut out = Vec::new();
    let mut left = notches;
    while left > 0 {
        let part = left.min(per_event);
        out.push(part * NOTCH);
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

    pub fn scroller(
        anchor: Option<(i32, i32)>,
        _window_id: Option<u32>,
    ) -> Result<Box<dyn Scroller + Send>, CmError> {
        super::super::x11_scroll::scroller(anchor)
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::mem::size_of;
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
        KEYEVENTF_KEYUP, MOUSEEVENTF_HWHEEL, MOUSEEVENTF_WHEEL, MOUSEINPUT, VIRTUAL_KEY, VK_NEXT,
        VK_PRIOR,
    };
    use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, SetCursorPos, WM_MOUSEWHEEL};

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

    /// How the scroller moves the content. The wheel (`SendInput`) is the default; a console
    /// window that did not move for it is tried with `WM_MOUSEWHEEL` posted to the window itself
    /// (it needs no pointer over it), then with the Page Down and Page Up keys.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Way {
        Wheel,
        Message,
        PageKeys,
    }

    struct WinScroller {
        anchor: Option<(i32, i32)>,
        /// The window target when it is a console window (see [`is_console_class`]): the only
        /// target with fallbacks.
        console: Option<u32>,
        way: Way,
    }

    impl Scroller for WinScroller {
        fn scroll(&mut self, direction: Direction, amount_px: u32) -> Result<(), CmError> {
            self.go(direction, amount_px, true)
        }

        fn scroll_back(&mut self, direction: Direction, amount_px: u32) -> Result<bool, CmError> {
            self.go(direction, amount_px, false)?;
            Ok(true)
        }

        fn last_resort(&mut self, direction: Direction, amount_px: u32) -> Result<bool, CmError> {
            let Some(id) = self.console else {
                return Ok(false);
            };
            self.way = match self.way {
                Way::Wheel => Way::Message,
                Way::Message if direction == Direction::Vertical => Way::PageKeys,
                _ => return Ok(false),
            };
            if self.way == Way::PageKeys {
                // The keys go to the window in front.
                let _ = super::super::win_window::raise(id);
            }
            self.go(direction, amount_px, true)?;
            Ok(true)
        }
    }

    impl WinScroller {
        fn go(
            &mut self,
            direction: Direction,
            amount_px: u32,
            forward: bool,
        ) -> Result<(), CmError> {
            match self.way {
                Way::Wheel => self.wheel(direction, amount_px, forward),
                Way::Message => self.message(direction, amount_px, forward),
                Way::PageKeys => self.keys(forward),
            }
        }

        /// `forward`: the content moves down or to the right; otherwise up or left.
        fn wheel(
            &mut self,
            direction: Direction,
            amount_px: u32,
            forward: bool,
        ) -> Result<(), CmError> {
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
                let input = INPUT {
                    r#type: INPUT_MOUSE,
                    Anonymous: INPUT_0 {
                        mi: MOUSEINPUT {
                            dx: 0,
                            dy: 0,
                            mouseData: signed(direction, forward, delta) as u32,
                            dwFlags: flag,
                            time: 0,
                            dwExtraInfo: 0,
                        },
                    },
                };
                send(&[input])?;
                std::thread::sleep(EVENT_GAP);
            }
            Ok(())
        }

        /// `WM_MOUSEWHEEL` posted to the console window (vertical only; the console has no
        /// sideways scrolling).
        fn message(
            &mut self,
            direction: Direction,
            amount_px: u32,
            forward: bool,
        ) -> Result<(), CmError> {
            let (Some(id), Direction::Vertical) = (self.console, direction) else {
                return Ok(());
            };
            let hwnd = HWND(id as usize as *mut std::ffi::c_void);
            let (x, y) = self.anchor.unwrap_or((0, 0));
            // The low word of `lparam` is x and the high word y, in screen px; the high word of
            // `wparam` is the wheel delta (a signed 16 bit value), the low word the key state.
            let lparam = ((y as u16 as usize) << 16) | (x as u16 as usize);
            for delta in wheel_deltas(amount_px, PX_PER_NOTCH, MAX_EVENT) {
                let d = signed(direction, forward, delta) as i16 as u16 as usize;
                // SAFETY: a plain message post with integer arguments; a stale window handle
                // makes it fail, which is reported.
                unsafe {
                    PostMessageW(
                        Some(hwnd),
                        WM_MOUSEWHEEL,
                        WPARAM(d << 16),
                        LPARAM(lparam as isize),
                    )
                }
                .map_err(|_| CmError::internal("the console window did not take the scroll"))?;
                std::thread::sleep(EVENT_GAP);
            }
            Ok(())
        }

        /// Page Down (forward) or Page Up to the window in front, one key press per request.
        fn keys(&mut self, forward: bool) -> Result<(), CmError> {
            let vk = if forward { VK_NEXT } else { VK_PRIOR };
            send(&[key(vk, false), key(vk, true)])?;
            std::thread::sleep(EVENT_GAP);
            Ok(())
        }
    }

    /// Vertical: a negative delta scrolls the content down (forward). Horizontal: a positive
    /// delta scrolls it to the right (forward).
    fn signed(direction: Direction, forward: bool, delta: u32) -> i32 {
        match (direction, forward) {
            (Direction::Vertical, true) | (Direction::Horizontal, false) => -(delta as i32),
            (Direction::Vertical, false) | (Direction::Horizontal, true) => delta as i32,
        }
    }

    fn key(vk: VIRTUAL_KEY, up: bool) -> INPUT {
        // Page Up and Page Down are extended keys (the grey ones next to the arrows).
        let mut flags = KEYEVENTF_EXTENDEDKEY;
        if up {
            flags |= KEYEVENTF_KEYUP;
        }
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    fn send(inputs: &[INPUT]) -> Result<(), CmError> {
        // SAFETY: fully initialised INPUTs and their true size.
        let sent = unsafe { SendInput(inputs, size_of::<INPUT>() as i32) };
        if sent as usize != inputs.len() {
            return Err(CmError::internal(
                "Windows did not accept the scroll (the screen may be locked or a program with higher rights is in front)",
            ));
        }
        Ok(())
    }

    pub fn scroller(
        anchor: Option<(i32, i32)>,
        window_id: Option<u32>,
    ) -> Result<Box<dyn Scroller + Send>, CmError> {
        let console = window_id.filter(|&id| {
            super::super::win_window::class_name(id).is_some_and(|c| is_console_class(&c))
        });
        Ok(Box::new(WinScroller {
            anchor,
            console,
            way: Way::Wheel,
        }))
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

    pub fn scroller(
        _anchor: Option<(i32, i32)>,
        _window_id: Option<u32>,
    ) -> Result<Box<dyn Scroller + Send>, CmError> {
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
    fn wheel_deltas_are_whole_notches_in_small_events() {
        // 480 px at 100 px per notch is 4.8 notches: 5 notches, in events of at most 240 units.
        let d = wheel_deltas(480, 100, 240);
        assert_eq!(d, vec![240, 240, 120]);
        assert!(wheel_deltas(0, 100, 240).is_empty());
    }

    /// A classic console window ignores a wheel event of less than one notch: no event is ever
    /// smaller than 120 units or a part of a notch, however small the request.
    #[test]
    fn wheel_deltas_never_send_a_part_of_a_notch() {
        for amount in [1u32, 5, 49, 50, 99, 100, 130, 224, 480, 777, 1000, 12_345] {
            for max_event in [1u32, 120, 200, 240, 500] {
                let d = wheel_deltas(amount, 100, max_event);
                assert!(!d.is_empty(), "{amount} px");
                assert!(
                    d.iter().all(|u| u % 120 == 0 && *u >= 120),
                    "{amount}: {d:?}"
                );
                assert!(
                    d.iter().all(|u| *u <= max_event.max(120)),
                    "{amount}: {d:?}"
                );
            }
        }
        assert_eq!(wheel_deltas(5, 100, 240), vec![120]);
    }

    /// A wheel that ignores a delta below one notch (the console host) still moves for every
    /// request the scroller makes.
    #[test]
    fn a_scroller_that_ignores_parts_of_a_notch_still_moves() {
        for amount in 1u32..400 {
            let moved: u32 = wheel_deltas(amount, 100, 240)
                .into_iter()
                .filter(|u| *u >= 120)
                .sum();
            assert!(moved > 0, "{amount} px moved nothing");
        }
    }

    #[test]
    fn console_classes_are_the_only_ones_with_fallbacks() {
        assert!(is_console_class("ConsoleWindowClass"));
        assert!(is_console_class("CASCADIA_HOSTING_WINDOW_CLASS"));
        assert!(!is_console_class("Chrome_WidgetWin_1"));
        assert!(!is_console_class("Notepad"));
        assert!(!is_console_class(""));
    }
}
