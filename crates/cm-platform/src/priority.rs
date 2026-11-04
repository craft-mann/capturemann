//! Keeping long background work (transcription, speaker finding) from starving the app's own
//! windows: how many CPU threads a job may use, and a lower scheduling class for the thread that
//! runs it. Measured on macOS 26: a thread started with `pthread_create` (what `std::thread` and
//! the Whisper/ggml worker pool use) does NOT inherit the creator's class, it starts at the
//! default class (below the UI's user-interactive class). So the class lowers the job's own
//! thread (the decode loop, the GPU submissions); the CPU workers are held back by the thread
//! count ([`job_threads`]) only.
//!
//! The effect on how responsive the UI feels is not measured by the tests here; they check the
//! thread count and that the class is set and put back.

/// The most threads a background job uses.
pub const MAX_JOB_THREADS: usize = 8;
/// The fewest.
pub const MIN_JOB_THREADS: usize = 2;
/// Cores a background job leaves to the rest of the computer.
pub const CORES_LEFT_FREE: usize = 2;

/// CPU threads for a background job on a computer with `available` cores: all but
/// [`CORES_LEFT_FREE`], at least [`MIN_JOB_THREADS`], at most [`MAX_JOB_THREADS`].
pub fn job_threads_for(available: usize) -> i32 {
    available
        .saturating_sub(CORES_LEFT_FREE)
        .clamp(MIN_JOB_THREADS, MAX_JOB_THREADS) as i32
}

/// [`job_threads_for`] with this computer's core count.
pub fn job_threads() -> i32 {
    job_threads_for(std::thread::available_parallelism().map_or(4, |n| n.get()))
}

/// What sets the scheduling class of the calling thread (the OS here, a script in tests).
pub trait ThreadClass {
    /// Lowers the calling thread to background-utility work. `false` when the OS refused or has
    /// no such thing (nothing then needs putting back).
    fn lower(&self) -> bool;
    /// Puts the calling thread back to the normal class.
    fn restore(&self);
}

/// The operating system's thread classes (macOS QoS; elsewhere nothing happens yet).
pub struct OsThreadClass;

impl ThreadClass for OsThreadClass {
    fn lower(&self) -> bool {
        sys::lower()
    }
    fn restore(&self) {
        sys::restore();
    }
}

/// The calling thread runs at the lower class until this is dropped.
pub struct Lowered<'a> {
    host: &'a dyn ThreadClass,
    active: bool,
}

impl<'a> Lowered<'a> {
    /// Lowers the calling thread when `enabled`; a disabled or refused request does nothing.
    pub fn enter(host: &'a dyn ThreadClass, enabled: bool) -> Self {
        let active = enabled && host.lower();
        Lowered { host, active }
    }

    pub fn is_lowered(&self) -> bool {
        self.active
    }
}

impl Drop for Lowered<'_> {
    fn drop(&mut self) {
        if self.active {
            self.host.restore();
        }
    }
}

/// The calling thread at the lower class for as long as the result lives, when `enabled`.
pub fn lower_current_thread(enabled: bool) -> Lowered<'static> {
    static OS: OsThreadClass = OsThreadClass;
    Lowered::enter(&OS, enabled)
}

#[cfg(target_os = "macos")]
mod sys {
    use std::ffi::c_int;

    const QOS_CLASS_UTILITY: u32 = 0x11;
    const QOS_CLASS_DEFAULT: u32 = 0x15;

    extern "C" {
        fn pthread_set_qos_class_self_np(qos_class: u32, relative_priority: c_int) -> c_int;
    }

    pub fn lower() -> bool {
        // SAFETY: plain call on the calling thread with a valid class and a zero offset.
        unsafe { pthread_set_qos_class_self_np(QOS_CLASS_UTILITY, 0) == 0 }
    }

    pub fn restore() {
        // SAFETY: as above.
        let _ = unsafe { pthread_set_qos_class_self_np(QOS_CLASS_DEFAULT, 0) };
    }

    /// The class of the calling thread (tests).
    #[cfg(test)]
    pub fn current() -> u32 {
        extern "C" {
            fn pthread_self() -> usize;
            fn pthread_get_qos_class_np(
                thread: usize,
                qos_class: *mut u32,
                relative_priority: *mut c_int,
            ) -> c_int;
        }
        let (mut class, mut rel) = (0u32, 0 as c_int);
        // SAFETY: both out-pointers are valid for the call.
        unsafe { pthread_get_qos_class_np(pthread_self(), &mut class, &mut rel) };
        class
    }

    #[cfg(test)]
    pub const UTILITY: u32 = QOS_CLASS_UTILITY;
    #[cfg(test)]
    pub const DEFAULT: u32 = QOS_CLASS_DEFAULT;
}

#[cfg(not(target_os = "macos"))]
mod sys {
    pub fn lower() -> bool {
        false
    }
    pub fn restore() {}
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use std::cell::Cell;

    #[derive(Default)]
    struct Script {
        lowered: Cell<u32>,
        restored: Cell<u32>,
        refuse: bool,
    }

    impl ThreadClass for Script {
        fn lower(&self) -> bool {
            self.lowered.set(self.lowered.get() + 1);
            !self.refuse
        }
        fn restore(&self) {
            self.restored.set(self.restored.get() + 1);
        }
    }

    #[test]
    fn threads_leave_two_cores_free_between_two_and_eight() {
        let t = |n| job_threads_for(n);
        assert_eq!((t(1), t(2), t(4), t(6)), (2, 2, 2, 4));
        assert_eq!((t(10), t(12), t(16), t(64)), (8, 8, 8, 8));
        assert_eq!(t(0), 2);
    }

    #[test]
    fn the_class_is_lowered_for_the_scope_and_put_back() {
        let s = Script::default();
        {
            let g = Lowered::enter(&s, true);
            assert!(g.is_lowered());
            assert_eq!((s.lowered.get(), s.restored.get()), (1, 0));
        }
        assert_eq!((s.lowered.get(), s.restored.get()), (1, 1));
    }

    #[test]
    fn switched_off_or_refused_touches_nothing_more() {
        let s = Script::default();
        drop(Lowered::enter(&s, false));
        assert_eq!((s.lowered.get(), s.restored.get()), (0, 0));
        let r = Script {
            refuse: true,
            ..Script::default()
        };
        let g = Lowered::enter(&r, true);
        assert!(!g.is_lowered());
        drop(g);
        assert_eq!((r.lowered.get(), r.restored.get()), (1, 0));
    }

    /// The real call, on a thread of its own: utility inside the scope, default after it.
    #[cfg(target_os = "macos")]
    #[test]
    fn macos_sets_the_utility_class_and_puts_the_default_back() {
        std::thread::spawn(|| {
            {
                let g = lower_current_thread(true);
                assert!(g.is_lowered());
                assert_eq!(sys::current(), sys::UTILITY);
                // A thread started from here does not inherit it (see the module docs).
                let child = std::thread::spawn(sys::current).join().unwrap();
                assert_eq!(child, sys::DEFAULT);
            }
            assert_eq!(sys::current(), sys::DEFAULT);
        })
        .join()
        .unwrap();
    }
}
