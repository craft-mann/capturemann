//! The recording session clock (specs/01-technical-design.md, "One clock").
//!
//! Every frame, audio buffer and cursor event is stamped with the OS capture time (mach time on
//! macOS, QPC on Windows) and converted to [`MediaTime`] against one session origin. Tracks are
//! aligned by timestamp, never by sample count. Pause gaps are the session's concern, not the clock's.

use std::sync::Arc;

use crate::media_time::MediaTime;

/// A monotonic host clock in nanoseconds on the same timebase the capture APIs stamp with.
pub trait HostClock: Send + Sync + std::fmt::Debug {
    /// Nanoseconds on the OS capture timebase (macOS: `CLOCK_UPTIME_RAW`, which is
    /// `mach_absolute_time` in nanoseconds; Windows: QPC converted to nanoseconds).
    fn now_ns(&self) -> u64;
}

/// The real OS clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemHostClock;

#[cfg(target_os = "macos")]
impl HostClock for SystemHostClock {
    fn now_ns(&self) -> u64 {
        let mut ts = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        // SAFETY: `ts` is a valid, writable `timespec` for the duration of the call.
        let rc = unsafe { libc::clock_gettime(libc::CLOCK_UPTIME_RAW, &mut ts) };
        if rc != 0 {
            return 0;
        }
        u64::try_from(ts.tv_sec)
            .unwrap_or(0)
            .saturating_mul(1_000_000_000)
            .saturating_add(u64::try_from(ts.tv_nsec).unwrap_or(0))
    }
}

#[cfg(not(target_os = "macos"))]
impl HostClock for SystemHostClock {
    /// Placeholder timebase until the Windows QPC path lands with the Windows capture backend:
    /// monotonic time since the first call in this process.
    fn now_ns(&self) -> u64 {
        use std::sync::OnceLock;
        use std::time::Instant;
        static START: OnceLock<Instant> = OnceLock::new();
        u64::try_from(START.get_or_init(Instant::now).elapsed().as_nanos()).unwrap_or(u64::MAX)
    }
}

/// A clock a test advances by hand.
#[derive(Debug, Default)]
pub struct ManualHostClock(std::sync::atomic::AtomicU64);

impl ManualHostClock {
    pub fn new(ns: u64) -> Self {
        ManualHostClock(std::sync::atomic::AtomicU64::new(ns))
    }
    pub fn set_ns(&self, ns: u64) {
        self.0.store(ns, std::sync::atomic::Ordering::SeqCst);
    }
    pub fn advance_ns(&self, ns: u64) {
        self.0.fetch_add(ns, std::sync::atomic::Ordering::SeqCst);
    }
}

impl HostClock for ManualHostClock {
    fn now_ns(&self) -> u64 {
        self.0.load(std::sync::atomic::Ordering::SeqCst)
    }
}

/// Session origin plus the host clock. Cheap to clone; share one per recording.
#[derive(Debug, Clone)]
pub struct SessionClock {
    host: Arc<dyn HostClock>,
    origin_ns: u64,
}

impl SessionClock {
    /// Starts a session now on the real OS clock.
    pub fn start() -> Self {
        Self::with_host(Arc::new(SystemHostClock))
    }

    /// Starts a session now on the given clock (tests use [`ManualHostClock`]).
    pub fn with_host(host: Arc<dyn HostClock>) -> Self {
        let origin_ns = host.now_ns();
        SessionClock { host, origin_ns }
    }

    /// The session origin in host nanoseconds.
    pub fn origin_ns(&self) -> u64 {
        self.origin_ns
    }

    /// Current session time.
    pub fn now(&self) -> MediaTime {
        self.from_host_ns(self.host.now_ns())
    }

    /// Converts an OS capture timestamp (host nanoseconds) to session time. Stamps before the
    /// origin come out negative, which callers may drop or clamp.
    pub fn from_host_ns(&self, host_ns: u64) -> MediaTime {
        let delta = i128::from(host_ns) - i128::from(self.origin_ns);
        // ticks = ns * 120_000 / 1e9, rounded to nearest (ties away from zero).
        let scaled = delta * 120_000;
        let half = 500_000_000i128;
        let ticks = if scaled >= 0 {
            (scaled + half) / 1_000_000_000
        } else {
            (scaled - half) / 1_000_000_000
        };
        MediaTime::from_ticks(i64::try_from(ticks).unwrap_or(if ticks < 0 {
            i64::MIN
        } else {
            i64::MAX
        }))
    }

    /// Converts session time back to host nanoseconds (for APIs that take host times).
    pub fn to_host_ns(&self, t: MediaTime) -> u64 {
        let ns = i128::from(t.ticks()) * 1_000_000_000 / 120_000;
        u64::try_from((i128::from(self.origin_ns) + ns).max(0)).unwrap_or(u64::MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clock_at(origin_ns: u64) -> (SessionClock, Arc<ManualHostClock>) {
        let host = Arc::new(ManualHostClock::new(origin_ns));
        (SessionClock::with_host(host.clone()), host)
    }

    #[test]
    fn origin_maps_to_zero_and_one_second_to_120k_ticks() {
        let (c, host) = clock_at(5_000_000_000);
        assert_eq!(c.now(), MediaTime::ZERO);
        host.advance_ns(1_000_000_000);
        assert_eq!(c.now().ticks(), 120_000);
    }

    #[test]
    fn frame_times_are_exact_at_common_rates() {
        let (c, _) = clock_at(0);
        // 1/60 s = 16_666_667 ns (rounded) is 2000 ticks; 1/30 s is 4000.
        assert_eq!(c.from_host_ns(16_666_667).ticks(), 2000);
        assert_eq!(c.from_host_ns(33_333_333).ticks(), 4000);
    }

    #[test]
    fn stamps_before_the_origin_are_negative() {
        let (c, _) = clock_at(1_000_000_000);
        assert!(c.from_host_ns(500_000_000).ticks() < 0);
        assert_eq!(c.from_host_ns(500_000_000).ticks(), -60_000);
    }

    #[test]
    fn round_trips_through_host_ns() {
        let (c, _) = clock_at(7_000_000_000);
        let t = MediaTime::from_ticks(123_456);
        let back = c.from_host_ns(c.to_host_ns(t));
        assert!((back.ticks() - t.ticks()).abs() <= 1);
    }

    #[test]
    fn system_clock_is_monotonic() {
        let a = SystemHostClock.now_ns();
        let b = SystemHostClock.now_ns();
        assert!(b >= a);
    }
}
