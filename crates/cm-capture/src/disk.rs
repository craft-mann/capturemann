//! Disk-space guard for recording (NFR-7, REC-8): warn when free space is under 10 minutes of
//! recording at the current bitrate; stop cleanly at 1 GB free.
//!
//! [`assess_disk`] is a pure function; [`FreeSpace`] is the runtime hook the recorder calls every
//! few seconds.

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

/// Recording stops cleanly at or below this much free space (NFR-7: 1 GB, MB = 10^6 bytes).
pub const STOP_FREE_BYTES: u64 = 1_000_000_000;
/// The setup panel and the recorder warn below this many minutes of space (NFR-7).
pub const WARN_MINUTES: f64 = 10.0;

/// Result of a disk check.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DiskStatus {
    /// Plenty of space.
    Ok { minutes_left: f64 },
    /// Under [`WARN_MINUTES`] of recording left (REC-8.AC4): warn, keep recording.
    Low { minutes_left: f64 },
    /// At or below [`STOP_FREE_BYTES`] (REC-8.AC3): stop cleanly.
    Full { free_bytes: u64 },
}

impl DiskStatus {
    pub fn minutes_left(&self) -> Option<f64> {
        match *self {
            DiskStatus::Ok { minutes_left } | DiskStatus::Low { minutes_left } => {
                Some(minutes_left)
            }
            DiskStatus::Full { .. } => None,
        }
    }
}

/// The guard assumes the real output is this much larger than the encoder's target bitrate
/// (R3-19). The hardware HEVC encoder wrote about 48% of the target for synthetic content, but
/// busy content (video playback, scrolling) can exceed it, so the estimate errs on the safe side.
pub const BITRATE_SAFETY: f64 = 1.25;

/// Minutes of recording that fit in `free_bytes` at `bitrate_bps` (`disk_left_minutes` in
/// `get_recording_status`). The bitrate is the encoder's average target, taken
/// [`BITRATE_SAFETY`] times higher; it is an estimate, not a promise.
pub fn minutes_of_space(free_bytes: u64, bitrate_bps: u64) -> f64 {
    let per_minute = bitrate_bps as f64 * BITRATE_SAFETY / 8.0 * 60.0;
    if per_minute <= 0.0 {
        return f64::INFINITY;
    }
    free_bytes as f64 / per_minute
}

/// Classifies free space for a recording at `bitrate_bps` (video plus audio).
pub fn assess_disk(free_bytes: u64, bitrate_bps: u64) -> DiskStatus {
    if free_bytes <= STOP_FREE_BYTES {
        return DiskStatus::Full { free_bytes };
    }
    let minutes_left = minutes_of_space(free_bytes, bitrate_bps);
    if minutes_left < WARN_MINUTES {
        DiskStatus::Low { minutes_left }
    } else {
        DiskStatus::Ok { minutes_left }
    }
}

/// Free bytes on the volume holding a path. `None` when unknown (the guard is then skipped).
pub trait FreeSpace: Send + Sync {
    fn free_bytes(&self, path: &Path) -> Option<u64>;
}

/// The real volume, through `statvfs` (Unix) or `GetDiskFreeSpaceExW` (Windows).
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemFreeSpace;

#[cfg(unix)]
impl FreeSpace for SystemFreeSpace {
    fn free_bytes(&self, path: &Path) -> Option<u64> {
        use std::os::unix::ffi::OsStrExt;
        let c = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
        let mut st = std::mem::MaybeUninit::<libc::statvfs>::uninit();
        // SAFETY: `c` is a valid NUL-terminated path and `st` is a valid out-pointer for one
        // `statvfs`; the struct is only read after the call returned 0 (initialized).
        let st = unsafe {
            if libc::statvfs(c.as_ptr(), st.as_mut_ptr()) != 0 {
                return None;
            }
            st.assume_init()
        };
        Some((st.f_bavail as u64).saturating_mul(st.f_frsize as u64))
    }
}

#[cfg(windows)]
impl FreeSpace for SystemFreeSpace {
    fn free_bytes(&self, path: &Path) -> Option<u64> {
        use std::os::windows::ffi::OsStrExt;
        use windows::core::PCWSTR;
        use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
        let mut available = 0u64;
        // SAFETY: `wide` is a NUL-terminated UTF-16 path that outlives the call, and
        // `available` is a valid out-pointer for the one value requested (the bytes free for
        // this user, which honours quotas, like `f_bavail` above).
        unsafe {
            GetDiskFreeSpaceExW(PCWSTR(wide.as_ptr()), Some(&mut available), None, None).ok()?;
        }
        Some(available)
    }
}

#[cfg(not(any(unix, windows)))]
impl FreeSpace for SystemFreeSpace {
    fn free_bytes(&self, _path: &Path) -> Option<u64> {
        None
    }
}

/// A free-space reading a test sets by hand.
#[derive(Debug, Default)]
pub struct FakeFreeSpace(AtomicU64);

impl FakeFreeSpace {
    pub fn new(bytes: u64) -> Self {
        FakeFreeSpace(AtomicU64::new(bytes))
    }
    pub fn set(&self, bytes: u64) {
        self.0.store(bytes, Ordering::SeqCst);
    }
}

impl FreeSpace for FakeFreeSpace {
    fn free_bytes(&self, _path: &Path) -> Option<u64> {
        Some(self.0.load(Ordering::SeqCst))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1_000_000_000;

    /// 40 Mbps HEVC 2160p30 is 300 MB per minute.
    const BPS: u64 = 40_000_000;

    #[test]
    fn nfr7_warns_below_ten_minutes_of_space() {
        // 10 minutes at 40 Mbps = 3 GB, 3.75 GB with the 1.25x safety factor (R3-19).
        assert!(matches!(
            assess_disk(3 * GB + GB * 3 / 4 + 1, BPS),
            DiskStatus::Ok { .. }
        ));
        assert!(matches!(
            assess_disk(3 * GB + GB * 3 / 4 - 1, BPS),
            DiskStatus::Low { .. }
        ));
        let m = assess_disk(2 * GB, BPS).minutes_left().unwrap();
        assert!((m - 5.33).abs() < 0.01, "{m}");
    }

    #[test]
    fn nfr7_stops_cleanly_at_one_gigabyte_free() {
        assert_eq!(
            assess_disk(STOP_FREE_BYTES, BPS),
            DiskStatus::Full {
                free_bytes: STOP_FREE_BYTES
            }
        );
        assert!(matches!(
            assess_disk(STOP_FREE_BYTES + 1, BPS),
            DiskStatus::Low { .. }
        ));
        assert!(matches!(assess_disk(0, BPS), DiskStatus::Full { .. }));
    }

    #[test]
    fn small_bitrates_never_warn_on_a_big_disk() {
        // 720p Small at ~1.75 Mbps: 100 GB is hours.
        assert!(matches!(
            assess_disk(100 * GB, 1_750_000),
            DiskStatus::Ok { .. }
        ));
        assert_eq!(minutes_of_space(1, 0), f64::INFINITY);
    }

    #[cfg(unix)]
    #[test]
    fn system_free_space_reads_the_temp_volume() {
        let free = SystemFreeSpace.free_bytes(&std::env::temp_dir());
        assert!(free.is_some_and(|f| f > 0));
        assert!(SystemFreeSpace
            .free_bytes(Path::new("/definitely/not/a/path"))
            .is_none());
    }
}
