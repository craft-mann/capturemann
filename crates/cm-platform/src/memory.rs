//! Process memory and core-layout probes.

/// The peak resident memory of this process so far, in bytes (`None` where it cannot be read).
pub fn peak_rss_bytes() -> Option<u64> {
    sys::peak_rss_bytes()
}

/// How many performance cores an Apple Silicon Mac has (its efficiency cores slow a run that
/// waits for its slowest thread). `None` on any other computer, where nothing is known about the
/// cores beyond their count.
pub fn performance_cores() -> Option<usize> {
    sys::performance_cores()
}

#[cfg(unix)]
mod sys {
    pub fn peak_rss_bytes() -> Option<u64> {
        // SAFETY: `getrusage` fills the zeroed struct we hand it and nothing else.
        let ru = unsafe {
            let mut ru: libc::rusage = std::mem::zeroed();
            if libc::getrusage(libc::RUSAGE_SELF, &mut ru) != 0 {
                return None;
            }
            ru
        };
        let max = u64::try_from(ru.ru_maxrss).ok()?;
        // macOS reports bytes, Linux kilobytes.
        Some(if cfg!(target_os = "macos") {
            max
        } else {
            max * 1024
        })
    }

    #[cfg(target_os = "macos")]
    pub fn performance_cores() -> Option<usize> {
        let mut n: libc::c_int = 0;
        let mut len = std::mem::size_of::<libc::c_int>();
        // SAFETY: the name is a NUL-terminated literal and `n`/`len` are valid for `len` bytes.
        let ok = unsafe {
            libc::sysctlbyname(
                c"hw.perflevel0.logicalcpu".as_ptr(),
                (&raw mut n).cast(),
                &raw mut len,
                std::ptr::null_mut(),
                0,
            ) == 0
        };
        if ok && n > 0 {
            return usize::try_from(n).ok();
        }
        None
    }

    #[cfg(not(target_os = "macos"))]
    pub fn performance_cores() -> Option<usize> {
        None
    }
}

#[cfg(windows)]
mod sys {
    use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    use windows::Win32::System::Threading::GetCurrentProcess;

    pub fn peak_rss_bytes() -> Option<u64> {
        let mut c = PROCESS_MEMORY_COUNTERS {
            cb: u32::try_from(std::mem::size_of::<PROCESS_MEMORY_COUNTERS>()).ok()?,
            ..Default::default()
        };
        // SAFETY: the pseudo handle of the current process needs no closing and `c` is a valid
        // struct whose `cb` holds its size.
        unsafe { GetProcessMemoryInfo(GetCurrentProcess(), &mut c, c.cb) }.ok()?;
        u64::try_from(c.PeakWorkingSetSize).ok()
    }

    pub fn performance_cores() -> Option<usize> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn performance_cores_never_exceeds_the_logical_cores() {
        if let Some(n) = performance_cores() {
            assert!(n >= 1 && n <= std::thread::available_parallelism().expect("cores").get());
        }
    }

    #[test]
    fn the_peak_is_known_and_plausible() {
        let peak = peak_rss_bytes().expect("peak memory is readable on this OS");
        assert!(peak > 1_000_000, "{peak}");
    }
}
