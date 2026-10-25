//! `MediaTime`: i64 ticks on the session clock, 120,000 per second.

use crate::error::CmError;
use serde::{Deserialize, Serialize};
use std::ops::{Add, AddAssign, Mul, Neg, Sub, SubAssign};

/// Ticks per second (exact for 24, 25, 30, 50 and 60 fps, and the NTSC rates).
pub const TICKS_PER_SECOND: i64 = 120_000;
const TICKS_PER_MS: i64 = TICKS_PER_SECOND / 1000;

/// A point or span of time in ticks. Serializes as a plain integer.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct MediaTime(i64);

impl MediaTime {
    pub const ZERO: MediaTime = MediaTime(0);

    pub const fn from_ticks(ticks: i64) -> Self {
        MediaTime(ticks)
    }

    pub const fn ticks(self) -> i64 {
        self.0
    }

    pub const fn from_secs(secs: i64) -> Self {
        MediaTime(secs * TICKS_PER_SECOND)
    }

    /// Exact: 1 ms is 120 ticks.
    pub const fn from_millis(ms: i64) -> Self {
        MediaTime(ms * TICKS_PER_MS)
    }

    /// Rounds to the nearest tick; rejects NaN, infinities and out-of-range values.
    pub fn from_secs_f64(secs: f64) -> Result<Self, CmError> {
        let ticks = (secs * TICKS_PER_SECOND as f64).round();
        // i64::MAX as f64 rounds up to exactly 2^63, so `>=` rejects every overflow.
        if !ticks.is_finite() || ticks >= i64::MAX as f64 || ticks < i64::MIN as f64 {
            return Err(CmError::invalid_input(format!(
                "time {secs} s is not representable"
            )));
        }
        Ok(MediaTime(ticks as i64))
    }

    pub fn as_secs_f64(self) -> f64 {
        self.0 as f64 / TICKS_PER_SECOND as f64
    }

    /// Whole milliseconds, rounded toward negative infinity.
    pub const fn as_millis_floor(self) -> i64 {
        self.0.div_euclid(TICKS_PER_MS)
    }

    /// Time of the start of frame `n`. Errors on overflow.
    pub fn from_frames(frames: i64, fps: Fps) -> Result<Self, CmError> {
        frames
            .checked_mul(fps.ticks_per_frame())
            .map(MediaTime)
            .ok_or_else(|| CmError::invalid_input("frame count overflows MediaTime"))
    }

    /// Index of the frame containing this time (floor).
    pub fn frames_floor(self, fps: Fps) -> i64 {
        self.0.div_euclid(fps.ticks_per_frame())
    }

    /// Index of the nearest frame boundary (ties round up).
    pub fn frames_round(self, fps: Fps) -> i64 {
        let t = fps.ticks_per_frame();
        (self.0 + t / 2).div_euclid(t)
    }

    pub fn is_frame_aligned(self, fps: Fps) -> bool {
        self.0.rem_euclid(fps.ticks_per_frame()) == 0
    }

    pub fn checked_add(self, rhs: Self) -> Option<Self> {
        self.0.checked_add(rhs.0).map(MediaTime)
    }

    pub fn checked_sub(self, rhs: Self) -> Option<Self> {
        self.0.checked_sub(rhs.0).map(MediaTime)
    }

    pub const fn abs(self) -> Self {
        MediaTime(self.0.saturating_abs())
    }
}

// The operators saturate at `i64::MIN`/`i64::MAX` ticks instead of wrapping in release builds
// (or panicking in debug ones): a corrupt or hostile manifest can hold huge tick counts, and a
// wrapped sum would turn into a negative or tiny time. Use `checked_add`/`checked_sub` where
// overflow must be an error (R1-15).
impl Add for MediaTime {
    type Output = MediaTime;
    fn add(self, rhs: Self) -> Self {
        MediaTime(self.0.saturating_add(rhs.0))
    }
}
impl Sub for MediaTime {
    type Output = MediaTime;
    fn sub(self, rhs: Self) -> Self {
        MediaTime(self.0.saturating_sub(rhs.0))
    }
}
impl Neg for MediaTime {
    type Output = MediaTime;
    fn neg(self) -> Self {
        MediaTime(self.0.saturating_neg())
    }
}
impl Mul<i64> for MediaTime {
    type Output = MediaTime;
    fn mul(self, rhs: i64) -> Self {
        MediaTime(self.0.saturating_mul(rhs))
    }
}
impl AddAssign for MediaTime {
    fn add_assign(&mut self, rhs: Self) {
        self.0 = self.0.saturating_add(rhs.0);
    }
}
impl SubAssign for MediaTime {
    fn sub_assign(&mut self, rhs: Self) {
        self.0 = self.0.saturating_sub(rhs.0);
    }
}

/// A frame rate whose frame duration is a whole number of ticks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Fps {
    num: u32,
    den: u32,
}

impl Fps {
    pub const FPS_24: Fps = Fps { num: 24, den: 1 };
    pub const FPS_25: Fps = Fps { num: 25, den: 1 };
    pub const FPS_30: Fps = Fps { num: 30, den: 1 };
    pub const FPS_50: Fps = Fps { num: 50, den: 1 };
    pub const FPS_60: Fps = Fps { num: 60, den: 1 };

    /// `num/den` frames per second; errors when a frame is not a whole number of ticks.
    pub fn new(num: u32, den: u32) -> Result<Self, CmError> {
        if num == 0 || den == 0 {
            return Err(CmError::invalid_field("fps", "must be positive"));
        }
        if (TICKS_PER_SECOND * i64::from(den)) % i64::from(num) != 0 {
            return Err(CmError::invalid_field(
                "fps",
                format!("{num}/{den} is not exact in 120,000 ticks/s"),
            ));
        }
        Ok(Fps { num, den })
    }

    pub fn ticks_per_frame(self) -> i64 {
        TICKS_PER_SECOND * i64::from(self.den) / i64::from(self.num)
    }

    pub fn as_f64(self) -> f64 {
        f64::from(self.num) / f64::from(self.den)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXACT: [Fps; 5] = [
        Fps::FPS_24,
        Fps::FPS_25,
        Fps::FPS_30,
        Fps::FPS_50,
        Fps::FPS_60,
    ];

    #[test]
    fn td_time_exact_for_24_25_30_50_60_fps() {
        for fps in EXACT {
            assert_eq!(TICKS_PER_SECOND % fps.ticks_per_frame(), 0);
            let one_sec = MediaTime::from_secs(1);
            assert_eq!(one_sec.frames_floor(fps), fps.as_f64() as i64);
        }
        assert_eq!(Fps::FPS_60.ticks_per_frame(), 2000);
        assert_eq!(Fps::FPS_24.ticks_per_frame(), 5000);
    }

    #[test]
    fn frames_round_trip_and_ntsc() {
        for fps in EXACT {
            for n in [0, 1, 59, 1_000_000, -7] {
                let t = MediaTime::from_frames(n, fps).unwrap();
                assert_eq!(t.frames_floor(fps), n);
                assert_eq!(t.frames_round(fps), n);
                assert!(t.is_frame_aligned(fps));
            }
        }
        let ntsc = Fps::new(30000, 1001).unwrap();
        assert_eq!(ntsc.ticks_per_frame(), 4004);
        assert!(Fps::new(7, 1).is_err());
        assert!(Fps::new(0, 1).is_err());
    }

    #[test]
    fn frame_rounding_between_boundaries() {
        let fps = Fps::FPS_60;
        let t = MediaTime::from_ticks(2999);
        assert_eq!(t.frames_floor(fps), 1);
        assert_eq!(t.frames_round(fps), 1);
        assert_eq!(MediaTime::from_ticks(3000).frames_round(fps), 2);
        assert_eq!(MediaTime::from_ticks(-1).frames_floor(fps), -1);
        assert!(!t.is_frame_aligned(fps));
    }

    #[test]
    fn seconds_and_millis_conversions() {
        assert_eq!(MediaTime::from_secs(2).ticks(), 240_000);
        assert_eq!(MediaTime::from_millis(1500).ticks(), 180_000);
        assert_eq!(MediaTime::from_millis(1500).as_millis_floor(), 1500);
        assert_eq!(MediaTime::from_ticks(-1).as_millis_floor(), -1);
        let t = MediaTime::from_secs_f64(0.5).unwrap();
        assert_eq!(t.ticks(), 60_000);
        assert!((t.as_secs_f64() - 0.5).abs() < 1e-12);
        assert!(MediaTime::from_secs_f64(f64::NAN).is_err());
        assert!(MediaTime::from_secs_f64(f64::INFINITY).is_err());
        assert!(MediaTime::from_secs_f64(1e30).is_err());
    }

    #[test]
    fn arithmetic() {
        let a = MediaTime::from_secs(3);
        let b = MediaTime::from_secs(1);
        assert_eq!(a + b, MediaTime::from_secs(4));
        assert_eq!(a - b, MediaTime::from_secs(2));
        assert_eq!(-b, MediaTime::from_secs(-1));
        assert_eq!(b * 5, MediaTime::from_secs(5));
        let mut c = a;
        c += b;
        c -= MediaTime::from_secs(4);
        assert_eq!(c, MediaTime::ZERO);
        assert!(MediaTime::from_ticks(i64::MAX).checked_add(b).is_none());
        assert!(MediaTime::from_frames(i64::MAX, Fps::FPS_60).is_err());
    }

    /// R1-15: operators saturate instead of wrapping.
    #[test]
    fn r1_15_operators_saturate() {
        let max = MediaTime::from_ticks(i64::MAX);
        let min = MediaTime::from_ticks(i64::MIN);
        let one = MediaTime::from_ticks(1);
        assert_eq!(max + one, max);
        assert_eq!(min - one, min);
        assert_eq!(max * 2, max);
        assert_eq!(-min, max);
        assert_eq!(min.abs(), max);
        let mut t = max;
        t += one;
        assert_eq!(t, max);
        let mut t = min;
        t -= one;
        assert_eq!(t, min);
    }

    #[test]
    fn serde_is_plain_integer() {
        let t = MediaTime::from_ticks(240_000);
        assert_eq!(serde_json::to_string(&t).unwrap(), "240000");
        assert_eq!(
            serde_json::from_str::<MediaTime>("7200000")
                .unwrap()
                .ticks(),
            7_200_000
        );
    }
}
