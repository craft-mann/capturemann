//! The OS capture clock.

/// Nanoseconds on the timebase the capture APIs stamp with: macOS `CLOCK_UPTIME_RAW` (the same
/// as `mach_absolute_time`, in nanoseconds), Windows `QueryPerformanceCounter`, other Unix
/// `CLOCK_MONOTONIC`. Monotonic; the origin is unspecified (compare differences only).
pub fn monotonic_ns() -> u64 {
    sys::monotonic_ns()
}

#[cfg(target_os = "macos")]
mod sys {
    pub fn monotonic_ns() -> u64 {
        super::clock_gettime_ns(libc::CLOCK_UPTIME_RAW)
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod sys {
    pub fn monotonic_ns() -> u64 {
        super::clock_gettime_ns(libc::CLOCK_MONOTONIC)
    }
}

#[cfg(unix)]
fn clock_gettime_ns(clock: libc::clockid_t) -> u64 {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `ts` is a valid, writable `timespec` for the duration of the call.
    let rc = unsafe { libc::clock_gettime(clock, &mut ts) };
    if rc != 0 {
        return 0;
    }
    u64::try_from(ts.tv_sec)
        .unwrap_or(0)
        .saturating_mul(1_000_000_000)
        .saturating_add(u64::try_from(ts.tv_nsec).unwrap_or(0))
}

#[cfg(windows)]
mod sys {
    use std::sync::OnceLock;
    use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};

    pub fn monotonic_ns() -> u64 {
        static FREQ: OnceLock<u64> = OnceLock::new();
        let freq = *FREQ.get_or_init(|| {
            let mut f = 0i64;
            // SAFETY: `f` is a valid, writable i64 for the duration of the call.
            let ok = unsafe { QueryPerformanceFrequency(&mut f) }.is_ok();
            u64::try_from(f).ok().filter(|&f| ok && f > 0).unwrap_or(1)
        });
        let mut c = 0i64;
        // SAFETY: `c` is a valid, writable i64 for the duration of the call.
        if unsafe { QueryPerformanceCounter(&mut c) }.is_err() {
            return 0;
        }
        let ticks = u128::from(u64::try_from(c).unwrap_or(0));
        u64::try_from(ticks * 1_000_000_000 / u128::from(freq)).unwrap_or(u64::MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_never_goes_backwards_and_moves() {
        let a = monotonic_ns();
        std::thread::sleep(std::time::Duration::from_millis(20));
        let b = monotonic_ns();
        assert!(b >= a + 10_000_000, "{a} -> {b}");
    }
}
