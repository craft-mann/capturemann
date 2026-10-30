//! Keeping the computer awake while a long recording runs.

/// Holds the "prevent idle sleep" state of the OS until dropped.
pub struct SleepGuard {
    _held: sys::Held,
}

impl std::fmt::Debug for SleepGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SleepGuard")
    }
}

/// Starts preventing idle sleep (the display may still turn off). macOS: an `IOPMAssertion` of
/// type `PreventUserIdleSystemSleep`; Windows: a system-required power request; elsewhere
/// nothing happens yet and the guard is empty. `reason` is shown by the OS tools that list
/// power assertions.
pub fn prevent_idle_sleep(reason: &str) -> Result<SleepGuard, String> {
    sys::acquire(reason).map(|_held| SleepGuard { _held })
}

#[cfg(target_os = "macos")]
mod sys {
    use std::ffi::{c_char, c_void, CString};

    type CFStringRef = *const c_void;

    #[link(name = "IOKit", kind = "framework")]
    extern "C" {
        fn IOPMAssertionCreateWithName(
            assertion_type: CFStringRef,
            level: u32,
            name: CFStringRef,
            id: *mut u32,
        ) -> i32;
        fn IOPMAssertionRelease(id: u32) -> i32;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFStringCreateWithCString(
            alloc: *const c_void,
            c_str: *const c_char,
            encoding: u32,
        ) -> CFStringRef;
        fn CFRelease(cf: *const c_void);
    }

    const UTF8: u32 = 0x0800_0100;
    const LEVEL_ON: u32 = 255;

    fn cf_string(s: &str) -> Option<CFStringRef> {
        let c = CString::new(s).ok()?;
        // SAFETY: `c` is a valid NUL-terminated string for the duration of the call; a null
        // allocator selects the default one. The result is released by the caller.
        let r = unsafe { CFStringCreateWithCString(std::ptr::null(), c.as_ptr(), UTF8) };
        (!r.is_null()).then_some(r)
    }

    pub struct Held(u32);

    impl Drop for Held {
        fn drop(&mut self) {
            // SAFETY: `self.0` came from a successful `IOPMAssertionCreateWithName` and is
            // released exactly once, here.
            unsafe {
                IOPMAssertionRelease(self.0);
            }
        }
    }

    pub fn acquire(reason: &str) -> Result<Held, String> {
        let kind = cf_string("PreventUserIdleSystemSleep")
            .ok_or_else(|| "cannot build the power assertion type".to_owned())?;
        let name = cf_string(reason);
        let mut id = 0u32;
        // SAFETY: `kind` and `name` are valid CFStrings, `id` points to a live u32.
        let status = unsafe {
            IOPMAssertionCreateWithName(
                kind,
                LEVEL_ON,
                name.unwrap_or(kind),
                std::ptr::addr_of_mut!(id),
            )
        };
        // SAFETY: both strings were created above and are not used again.
        unsafe {
            CFRelease(kind);
            if let Some(n) = name {
                CFRelease(n);
            }
        }
        if status != 0 {
            return Err(format!("cannot prevent idle sleep (IOKit status {status})"));
        }
        Ok(Held(id))
    }
}

#[cfg(windows)]
mod sys {
    use windows::core::PWSTR;
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::System::Power::{
        PowerClearRequest, PowerCreateRequest, PowerRequestSystemRequired, PowerSetRequest,
    };
    use windows::Win32::System::Threading::{
        POWER_REQUEST_CONTEXT_SIMPLE_STRING, REASON_CONTEXT, REASON_CONTEXT_0,
    };

    /// A power request handle; unlike `SetThreadExecutionState` it is not tied to a thread, so
    /// the guard may be dropped anywhere.
    pub struct Held(HANDLE);

    // SAFETY: a power request handle is a kernel object handle, usable from any thread.
    unsafe impl Send for Held {}
    // SAFETY: as above; the guard has no interior state to race on.
    unsafe impl Sync for Held {}

    impl Drop for Held {
        fn drop(&mut self) {
            // SAFETY: the handle came from `PowerCreateRequest` and is released exactly once.
            unsafe {
                let _ = PowerClearRequest(self.0, PowerRequestSystemRequired);
                let _ = CloseHandle(self.0);
            }
        }
    }

    pub fn acquire(reason: &str) -> Result<Held, String> {
        let mut text: Vec<u16> = reason.encode_utf16().chain(std::iter::once(0)).collect();
        let context = REASON_CONTEXT {
            Version: 0, // POWER_REQUEST_CONTEXT_VERSION
            Flags: POWER_REQUEST_CONTEXT_SIMPLE_STRING,
            Reason: REASON_CONTEXT_0 {
                SimpleReasonString: PWSTR(text.as_mut_ptr()),
            },
        };
        // SAFETY: `context` and the string it points to live until the call returns; Windows
        // copies the text.
        let handle = unsafe { PowerCreateRequest(&context) }
            .map_err(|e| format!("cannot prevent idle sleep: {e}"))?;
        // SAFETY: `handle` is the valid request just created.
        if let Err(e) = unsafe { PowerSetRequest(handle, PowerRequestSystemRequired) } {
            // SAFETY: closing the handle we own, once.
            unsafe {
                let _ = CloseHandle(handle);
            }
            return Err(format!("cannot prevent idle sleep: {e}"));
        }
        Ok(Held(handle))
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
mod sys {
    pub struct Held;

    pub fn acquire(_reason: &str) -> Result<Held, String> {
        Ok(Held)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Takes the real OS assertion for a moment: nothing is recorded, nothing is shown.
    #[test]
    fn the_guard_is_taken_and_released() {
        let g = prevent_idle_sleep("CaptureMann test").expect("a power request");
        drop(g);
    }
}
