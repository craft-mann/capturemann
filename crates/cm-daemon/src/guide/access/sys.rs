//! The real accessibility lookups: macOS AX (`AXUIElementCopyElementAtPosition`) and Windows
//! UI Automation (`IUIAutomation::ElementFromPoint`). Linux gives none yet (AT-SPI needs a
//! D-Bus client the project does not have), so the screen text names the step there.
//!
//! macOS needs the Accessibility permission for this; without it the call fails and the answer
//! is `None` (it never prompts: CaptureMann asks for Input Monitoring only). Every call has a
//! short time limit, so a frozen application cannot hold up the click.

use super::AccessibleNames;

/// The lookup of this system.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemNames;

#[cfg(target_os = "macos")]
mod mac {
    use std::ffi::{c_char, c_void, CString};

    type CFTypeRef = *const c_void;
    type AXError = i32;

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXUIElementCreateSystemWide() -> CFTypeRef;
        fn AXUIElementCopyElementAtPosition(
            app: CFTypeRef,
            x: f32,
            y: f32,
            element: *mut CFTypeRef,
        ) -> AXError;
        fn AXUIElementCopyAttributeValue(
            element: CFTypeRef,
            attribute: CFTypeRef,
            value: *mut CFTypeRef,
        ) -> AXError;
        fn AXUIElementSetMessagingTimeout(element: CFTypeRef, seconds: f32) -> AXError;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFRelease(cf: CFTypeRef);
        fn CFGetTypeID(cf: CFTypeRef) -> usize;
        fn CFStringGetTypeID() -> usize;
        fn CFStringCreateWithCString(
            alloc: CFTypeRef,
            c: *const c_char,
            encoding: u32,
        ) -> CFTypeRef;
        fn CFStringGetLength(s: CFTypeRef) -> isize;
        fn CFStringGetMaximumSizeForEncoding(length: isize, encoding: u32) -> isize;
        fn CFStringGetCString(s: CFTypeRef, buf: *mut c_char, size: isize, encoding: u32) -> u8;
    }

    const UTF8: u32 = 0x0800_0100;
    /// A frozen application answers within this, or the click is named from the screen text.
    const TIMEOUT_S: f32 = 0.25;

    /// Owns one Core Foundation object and releases it.
    struct Cf(CFTypeRef);

    impl Drop for Cf {
        fn drop(&mut self) {
            if !self.0.is_null() {
                // SAFETY: the pointer came from a Copy/Create call that returned a +1 object.
                unsafe { CFRelease(self.0) };
            }
        }
    }

    fn cf_string(s: &str) -> Option<Cf> {
        let c = CString::new(s).ok()?;
        // SAFETY: `c` is a valid NUL-terminated string; a null allocator is the default one.
        let p = unsafe { CFStringCreateWithCString(std::ptr::null(), c.as_ptr(), UTF8) };
        (!p.is_null()).then_some(Cf(p))
    }

    fn to_string(s: &Cf) -> Option<String> {
        // SAFETY: `s` is a live CF object; the type is checked before it is read as a string.
        unsafe {
            if CFGetTypeID(s.0) != CFStringGetTypeID() {
                return None;
            }
            let len = CFStringGetLength(s.0);
            let cap = CFStringGetMaximumSizeForEncoding(len, UTF8) + 1;
            let mut buf = vec![0u8; usize::try_from(cap).ok()?];
            if CFStringGetCString(s.0, buf.as_mut_ptr().cast(), cap, UTF8) == 0 {
                return None;
            }
            let end = buf.iter().position(|b| *b == 0).unwrap_or(buf.len());
            String::from_utf8(buf[..end].to_vec()).ok()
        }
    }

    fn text(el: &Cf, name: &str) -> Option<String> {
        let key = cf_string(name)?;
        let mut v: CFTypeRef = std::ptr::null();
        // SAFETY: `el` and `key` are live; `v` receives a +1 object when the call succeeds.
        let err = unsafe { AXUIElementCopyAttributeValue(el.0, key.0, &mut v) };
        if err != 0 || v.is_null() {
            return None;
        }
        to_string(&Cf(v))
    }

    pub fn name_at(gx: f64, gy: f64) -> Option<String> {
        // SAFETY: plain calls into the system; every object returned is wrapped in `Cf`.
        let system = Cf(unsafe { AXUIElementCreateSystemWide() });
        if system.0.is_null() {
            return None;
        }
        // SAFETY: `system` is live.
        unsafe { AXUIElementSetMessagingTimeout(system.0, TIMEOUT_S) };
        let mut found: CFTypeRef = std::ptr::null();
        // SAFETY: `found` receives a +1 element when the call succeeds.
        let err =
            unsafe { AXUIElementCopyElementAtPosition(system.0, gx as f32, gy as f32, &mut found) };
        if err != 0 || found.is_null() {
            return None;
        }
        let el = Cf(found);
        // SAFETY: `el` is live.
        unsafe { AXUIElementSetMessagingTimeout(el.0, TIMEOUT_S) };
        let role = text(&el, "AXRole").unwrap_or_default();
        if !super::super::role_may_name(&role) {
            return None;
        }
        // The title, then the description (an icon button has only that). Never the value:
        // that is what the person typed or what a field shows.
        ["AXTitle", "AXDescription"]
            .iter()
            .filter_map(|a| text(&el, a))
            .find(|t| !t.trim().is_empty())
    }
}

#[cfg(windows)]
mod win {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
    };
    use windows::Win32::UI::Accessibility::{
        CUIAutomation, IUIAutomation, UIA_ButtonControlTypeId, UIA_CheckBoxControlTypeId,
        UIA_ComboBoxControlTypeId, UIA_EditControlTypeId, UIA_HyperlinkControlTypeId,
        UIA_MenuItemControlTypeId, UIA_RadioButtonControlTypeId, UIA_SplitButtonControlTypeId,
        UIA_TabItemControlTypeId,
    };

    pub fn name_at(gx: f64, gy: f64) -> Option<String> {
        // SAFETY: COM is initialised for this thread (a repeat, or another mode, is fine and
        // ignored); every interface is released when it drops.
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let uia: IUIAutomation =
                CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()?;
            let el = uia
                .ElementFromPoint(POINT {
                    x: gx.round() as i32,
                    y: gy.round() as i32,
                })
                .ok()?;
            // Only controls that are named for what they do; panes, documents, lists and text
            // carry content, which can be personal.
            let kind = el.CurrentControlType().ok()?;
            let named = [
                UIA_ButtonControlTypeId,
                UIA_CheckBoxControlTypeId,
                UIA_ComboBoxControlTypeId,
                UIA_EditControlTypeId,
                UIA_HyperlinkControlTypeId,
                UIA_MenuItemControlTypeId,
                UIA_RadioButtonControlTypeId,
                UIA_SplitButtonControlTypeId,
                UIA_TabItemControlTypeId,
            ];
            if !named.contains(&kind) {
                return None;
            }
            let name = el.CurrentName().ok()?.to_string();
            (!name.trim().is_empty()).then_some(name)
        }
    }
}

impl AccessibleNames for SystemNames {
    fn name_at(&self, gx: f64, gy: f64) -> Option<String> {
        #[cfg(target_os = "macos")]
        {
            mac::name_at(gx, gy)
        }
        #[cfg(windows)]
        {
            win::name_at(gx, gy)
        }
        #[cfg(not(any(target_os = "macos", windows)))]
        {
            let _ = (gx, gy);
            None
        }
    }
}
