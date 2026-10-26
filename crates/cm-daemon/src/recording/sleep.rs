//! Keeping the computer awake while an audio recording runs (VOC-1, "Long sessions").
//!
//! A meeting recorded with the laptop on the table must not stop because the Mac decided it is
//! idle. The engine asks an [`IdleSleepInhibitor`] for a guard when an audio recording starts and
//! drops it when the recording ends, however it ends. Tests inject [`FakeIdleSleep`]; the real
//! adapter ([`SystemIdleSleep`], macOS `IOPMAssertion`) is wired in one place,
//! `DaemonBuilder::recording_system`, and is not exercised by any test: it takes a power
//! assertion, nothing else.
//!
//! Closing the lid of a laptop without an external display still sleeps the Mac; the engine then
//! stops the recording with `reason: "system_sleep"` and keeps what was captured.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use cm_core::CmError;

/// Something that can hold the "prevent idle sleep" state of the OS.
pub trait IdleSleepInhibitor: Send + Sync {
    /// Starts preventing idle sleep. The returned guard releases it when dropped.
    fn prevent(&self, reason: &str) -> Result<Box<dyn Send + Sync>, CmError>;
}

/// A scripted inhibitor that counts what it was asked (tests).
#[derive(Debug, Clone, Default)]
pub struct FakeIdleSleep {
    acquired: Arc<AtomicUsize>,
    active: Arc<AtomicUsize>,
}

impl FakeIdleSleep {
    pub fn new() -> Self {
        Self::default()
    }

    /// How many guards were handed out.
    pub fn acquired(&self) -> usize {
        self.acquired.load(Ordering::SeqCst)
    }

    /// How many guards are alive now.
    pub fn active(&self) -> usize {
        self.active.load(Ordering::SeqCst)
    }
}

struct FakeGuard(Arc<AtomicUsize>);

impl Drop for FakeGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

impl IdleSleepInhibitor for FakeIdleSleep {
    fn prevent(&self, _reason: &str) -> Result<Box<dyn Send + Sync>, CmError> {
        self.acquired.fetch_add(1, Ordering::SeqCst);
        self.active.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(FakeGuard(self.active.clone())))
    }
}

/// The real inhibitor of this OS: an `IOPMAssertion` of type `PreventUserIdleSystemSleep` on
/// macOS; a no-op elsewhere (Windows `SetThreadExecutionState` comes with the Windows port).
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemIdleSleep;

#[cfg(target_os = "macos")]
mod mac {
    use super::*;
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

    struct Assertion(u32);

    impl Drop for Assertion {
        fn drop(&mut self) {
            // SAFETY: `self.0` came from a successful `IOPMAssertionCreateWithName` and is
            // released exactly once, here.
            unsafe {
                IOPMAssertionRelease(self.0);
            }
        }
    }

    impl IdleSleepInhibitor for SystemIdleSleep {
        fn prevent(&self, reason: &str) -> Result<Box<dyn Send + Sync>, CmError> {
            let kind = cf_string("PreventUserIdleSystemSleep")
                .ok_or_else(|| CmError::internal("cannot build the power assertion type"))?;
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
                return Err(CmError::internal(format!(
                    "cannot prevent idle sleep (IOKit status {status})"
                )));
            }
            Ok(Box::new(Assertion(id)))
        }
    }
}

#[cfg(not(target_os = "macos"))]
impl IdleSleepInhibitor for SystemIdleSleep {
    fn prevent(&self, _reason: &str) -> Result<Box<dyn Send + Sync>, CmError> {
        Ok(Box::new(()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fake_counts_guards_and_releases_them_on_drop() {
        let f = FakeIdleSleep::new();
        let g = f.prevent("test").unwrap();
        assert_eq!((f.acquired(), f.active()), (1, 1));
        drop(g);
        assert_eq!((f.acquired(), f.active()), (1, 0));
    }
}
