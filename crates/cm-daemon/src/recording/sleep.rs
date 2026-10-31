//! Keeping the computer awake while an audio recording runs (VOC-1, "Long sessions").
//!
//! A meeting recorded with the laptop on the table must not stop because the Mac decided it is
//! idle. The engine asks an [`IdleSleepInhibitor`] for a guard when an audio recording starts and
//! drops it when the recording ends, however it ends. Tests inject [`FakeIdleSleep`]; the real
//! adapter ([`SystemIdleSleep`], a power assertion or request) is wired in one place,
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

/// The real inhibitor of this OS (`cm_platform::sleep`): a power assertion on macOS, a power
/// request on Windows, nothing elsewhere yet.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemIdleSleep;

impl IdleSleepInhibitor for SystemIdleSleep {
    fn prevent(&self, reason: &str) -> Result<Box<dyn Send + Sync>, CmError> {
        let guard = cm_platform::sleep::prevent_idle_sleep(reason).map_err(CmError::internal)?;
        Ok(Box::new(guard))
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
