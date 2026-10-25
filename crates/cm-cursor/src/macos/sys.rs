//! The only `unsafe` FFI of the macOS backend: CoreGraphics event taps and a few CoreFoundation
//! calls, wrapped so the rest of the crate is safe code.

use std::ffi::c_void;

pub type Ptr = *mut c_void;
pub type TapCallback = extern "C" fn(Ptr, u32, Ptr, Ptr) -> Ptr;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CGPoint {
    pub x: f64,
    pub y: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct CGSize {
    pub w: f64,
    pub h: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct CGRect {
    pub origin: CGPoint,
    pub size: CGSize,
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventTapCreate(
        tap: u32,
        place: u32,
        options: u32,
        events_of_interest: u64,
        callback: TapCallback,
        user_info: Ptr,
    ) -> Ptr;
    fn CGEventTapEnable(tap: Ptr, enable: bool);
    fn CGEventGetLocation(ev: Ptr) -> CGPoint;
    fn CGEventGetTimestamp(ev: Ptr) -> u64;
    fn CGEventGetFlags(ev: Ptr) -> u64;
    fn CGEventGetIntegerValueField(ev: Ptr, field: u32) -> i64;
    fn CGEventCreate(src: Ptr) -> Ptr;
    fn CGPreflightListenEventAccess() -> bool;
    fn CGRequestListenEventAccess() -> bool;
    #[cfg(test)]
    fn CGPreflightPostEventAccess() -> bool;
    fn CGMainDisplayID() -> u32;
    fn CGDisplayBounds(d: u32) -> CGRect;
    fn CGDisplayCopyDisplayMode(d: u32) -> Ptr;
    fn CGDisplayModeGetPixelWidth(m: Ptr) -> usize;
    fn CGDisplayModeGetWidth(m: Ptr) -> usize;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFMachPortCreateRunLoopSource(alloc: Ptr, port: Ptr, order: isize) -> Ptr;
    fn CFMachPortInvalidate(port: Ptr);
    fn CFRunLoopGetCurrent() -> Ptr;
    fn CFRunLoopAddSource(rl: Ptr, src: Ptr, mode: Ptr);
    fn CFRunLoopRunInMode(mode: Ptr, seconds: f64, return_after_source: bool) -> i32;
    fn CFRelease(p: Ptr);
    static kCFRunLoopCommonModes: Ptr;
    static kCFRunLoopDefaultMode: Ptr;
}

extern "C" {
    fn mach_absolute_time() -> u64;
    fn mach_timebase_info(info: *mut [u32; 2]) -> i32;
}

pub const MOVED: u32 = 5;
pub const SCROLL: u32 = 22;
pub const DISABLED_BY_TIMEOUT: u32 = 0xFFFF_FFFE;
pub const DISABLED_BY_USER: u32 = 0xFFFF_FFFF;
/// `kCGMouseEventButtonNumber`.
pub const FIELD_BUTTON_NUMBER: u32 = 3;
/// `kCGScrollWheelEventPointDeltaAxis1` (vertical) and `...Axis2` (horizontal).
pub const FIELD_SCROLL_DY: u32 = 96;
pub const FIELD_SCROLL_DX: u32 = 97;

/// Event types we listen to: moves, drags, left/right/other button down and up, scroll. Key
/// events (and `flagsChanged`) are deliberately absent, so keystrokes are never delivered to us.
pub fn event_mask() -> u64 {
    // left down/up 1,2; right down/up 3,4; moved 5; dragged 6,7; scroll 22; other down/up/drag 25,26,27.
    [1u32, 2, 3, 4, 5, 6, 7, 22, 25, 26, 27]
        .iter()
        .fold(0, |m, t| m | (1u64 << t))
}

/// Input Monitoring (needed by a listen-only tap).
pub fn listen_access() -> bool {
    // SAFETY: no arguments; reads the TCC state.
    unsafe { CGPreflightListenEventAccess() }
}

/// Shows the system Input Monitoring prompt (asynchronous; returns the current state).
pub fn request_listen_access() -> bool {
    // SAFETY: no arguments; may present a system dialog.
    unsafe { CGRequestListenEventAccess() }
}

/// Accessibility-class access to post synthetic events (used only by gated hardware tests).
#[cfg(test)]
pub fn post_access() -> bool {
    // SAFETY: no arguments.
    unsafe { CGPreflightPostEventAccess() }
}

/// Current pointer position in global points, or `None` if the event could not be created.
pub fn pointer_location() -> Option<(f64, f64)> {
    // SAFETY: CGEventCreate(NULL) returns an owned event or NULL; we read it and release it once.
    unsafe {
        let ev = CGEventCreate(std::ptr::null_mut());
        if ev.is_null() {
            return None;
        }
        let p = CGEventGetLocation(ev);
        CFRelease(ev);
        Some((p.x, p.y))
    }
}

pub fn main_display_id() -> u32 {
    // SAFETY: no arguments.
    unsafe { CGMainDisplayID() }
}

pub fn display_bounds(id: u32) -> CGRect {
    // SAFETY: plain value call.
    unsafe { CGDisplayBounds(id) }
}

/// Backing scale (pixels per point) of a display, from its current mode.
pub fn display_scale(id: u32) -> f64 {
    // SAFETY: the mode is owned (Copy rule), only read while alive, and released once.
    unsafe {
        let m = CGDisplayCopyDisplayMode(id);
        if m.is_null() {
            return 1.0;
        }
        let (pw, w) = (CGDisplayModeGetPixelWidth(m), CGDisplayModeGetWidth(m));
        CFRelease(m);
        if w == 0 {
            1.0
        } else {
            pw as f64 / w as f64
        }
    }
}

/// `mach_absolute_time` ticks now.
pub fn mach_now() -> u64 {
    // SAFETY: no arguments.
    unsafe { mach_absolute_time() }
}

/// `(numer, denom)` converting mach ticks to ns.
pub fn timebase() -> (u64, u64) {
    let mut tb = [0u32; 2];
    // SAFETY: `tb` is a valid `mach_timebase_info` out-struct (two u32: numer, denom).
    let rc = unsafe { mach_timebase_info(&mut tb) };
    if rc != 0 || tb[1] == 0 {
        (1, 1)
    } else {
        (u64::from(tb[0]), u64::from(tb[1]))
    }
}

/// Fields of one tap event, read inside the callback.
pub struct EventView {
    ev: Ptr,
}

impl EventView {
    /// # Safety
    /// `ev` must be the live event handed to the tap callback, and the view must not outlive
    /// that callback invocation.
    pub unsafe fn new(ev: Ptr) -> Self {
        EventView { ev }
    }
    pub fn timestamp(&self) -> u64 {
        // SAFETY: `ev` is live per `new`'s contract.
        unsafe { CGEventGetTimestamp(self.ev) }
    }
    pub fn location(&self) -> (f64, f64) {
        // SAFETY: as above.
        let p = unsafe { CGEventGetLocation(self.ev) };
        (p.x, p.y)
    }
    pub fn flags(&self) -> u64 {
        // SAFETY: as above.
        unsafe { CGEventGetFlags(self.ev) }
    }
    pub fn field(&self, f: u32) -> i64 {
        // SAFETY: as above.
        unsafe { CGEventGetIntegerValueField(self.ev, f) }
    }
}

/// Re-enables a tap the system disabled (timeout or user input).
pub fn tap_enable(tap: usize) {
    if tap != 0 {
        // SAFETY: `tap` is the live CFMachPort created by `EventTap::create`, kept alive until
        // the owning thread tears it down after the callback can no longer run.
        unsafe { CGEventTapEnable(tap as Ptr, true) }
    }
}

/// A listen-only session event tap attached to the current thread's run loop.
pub struct EventTap {
    tap: Ptr,
    source: Ptr,
}

impl EventTap {
    /// Creates and enables the tap on the calling thread's run loop. `None` when the system
    /// refuses (missing permission).
    ///
    /// # Safety
    /// `user` must stay valid until [`EventTap::destroy`] has returned, and `cb` must treat it as
    /// the same type on every call.
    pub unsafe fn create(cb: TapCallback, user: Ptr) -> Option<Self> {
        // SAFETY: arguments per the contract above; session tap (1), head insert (0),
        // listen-only (1).
        unsafe {
            let tap = CGEventTapCreate(1, 0, 1, event_mask(), cb, user);
            if tap.is_null() {
                return None;
            }
            let source = CFMachPortCreateRunLoopSource(std::ptr::null_mut(), tap, 0);
            if source.is_null() {
                CFMachPortInvalidate(tap);
                CFRelease(tap);
                return None;
            }
            CFRunLoopAddSource(CFRunLoopGetCurrent(), source, kCFRunLoopCommonModes);
            CGEventTapEnable(tap, true);
            Some(EventTap { tap, source })
        }
    }

    pub fn raw(&self) -> usize {
        self.tap as usize
    }

    /// Runs the calling thread's run loop for up to `secs`.
    pub fn run_for(secs: f64) {
        // SAFETY: plain run-loop call on the current thread.
        unsafe {
            CFRunLoopRunInMode(kCFRunLoopDefaultMode, secs, false);
        }
    }

    /// Detaches and frees the tap. After this the callback never runs again.
    pub fn destroy(self) {
        // SAFETY: both objects were created by `create` on this thread and are released once.
        unsafe {
            CGEventTapEnable(self.tap, false);
            CFMachPortInvalidate(self.tap);
            CFRelease(self.source);
            CFRelease(self.tap);
        }
    }
}

#[cfg(test)]
pub mod inject {
    //! Synthetic events for the gated hardware tests. Never used unless CM_ALLOW_INPUT_INJECTION=1.
    use super::*;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventCreateMouseEvent(src: Ptr, ty: u32, pt: CGPoint, button: u32) -> Ptr;
        fn CGEventPost(tap: u32, ev: Ptr);
    }

    #[link(name = "c")]
    extern "C" {
        fn getrusage(who: i32, ru: *mut [i64; 18]) -> i32;
    }

    /// User + system CPU seconds of this process.
    pub fn cpu_seconds() -> f64 {
        let mut ru = [0i64; 18];
        // SAFETY: `ru` is at least as large as `struct rusage` on 64-bit macOS (144 bytes).
        unsafe { getrusage(0, &mut ru) };
        // ru_utime {sec: i64, usec: i32 + pad}, ru_stime likewise.
        (ru[0] + ru[2]) as f64 + ((ru[1] as i32) as f64 + (ru[3] as i32) as f64) / 1e6
    }

    pub fn post(ty: u32, x: f64, y: f64, button: u32) {
        // SAFETY: creates an owned event, posts it (copy semantics) and releases it once.
        unsafe {
            let e = CGEventCreateMouseEvent(std::ptr::null_mut(), ty, CGPoint { x, y }, button);
            if !e.is_null() {
                CGEventPost(0, e);
                CFRelease(e);
            }
        }
    }
}
