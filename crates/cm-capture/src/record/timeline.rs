//! Pause gaps and the session-time to file-time mapping (REC-4).
//!
//! The screen, camera, mic and system audio files have continuous timestamps: a pause removes its
//! span from the file, so a frame captured after the gap lands right after the last frame before it.
//! All of them carry the same gap list (`details.pauses`, `manifest.gaps`), which maps file time back to
//! session time. Only the cursor log keeps session time (its events are sparse and stamped by event
//! time). The mapping is
//!
//! `file(t) = t - sum of the lengths of all gaps that ended at or before t`
//!
//! and is `None` for times inside a gap (those frames are dropped).

use cm_core::{CmError, MediaTime, PauseGap};

/// Maximum pauses per recording (REC-4 edge case).
pub const MAX_PAUSES: usize = 500;

#[derive(Debug, Clone, Default)]
pub struct PauseTimeline {
    gaps: Vec<PauseGap>,
    /// Start of a pause that has not been resumed yet.
    open: Option<MediaTime>,
}

impl PauseTimeline {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_paused(&self) -> bool {
        self.open.is_some()
    }

    /// Closed gaps in order.
    pub fn gaps(&self) -> &[PauseGap] {
        &self.gaps
    }

    /// Total paused time in closed gaps.
    pub fn total_gap(&self) -> MediaTime {
        MediaTime::from_ticks(
            self.gaps
                .iter()
                .map(|g| g.to.ticks() - g.from.ticks())
                .sum(),
        )
    }

    pub fn pause(&mut self, at: MediaTime) -> Result<(), CmError> {
        if self.open.is_some() {
            return Err(CmError::invalid_input("The recording is already paused"));
        }
        if self.gaps.len() >= MAX_PAUSES {
            return Err(CmError::invalid_input(
                "Too many pauses — stop and start a new recording",
            ));
        }
        if self.gaps.last().is_some_and(|g| at < g.to) {
            return Err(CmError::invalid_input(
                "Pause time is before the last resume",
            ));
        }
        self.open = Some(at);
        Ok(())
    }

    pub fn resume(&mut self, at: MediaTime) -> Result<(), CmError> {
        let Some(from) = self.open else {
            return Err(CmError::invalid_input("The recording is not paused"));
        };
        let to = at.max(from);
        self.open = None;
        self.gaps.push(PauseGap { from, to });
        Ok(())
    }

    /// File time for a session time; `None` inside a gap or after an unresumed pause began.
    pub fn map(&self, t: MediaTime) -> Option<MediaTime> {
        if self.open.is_some_and(|from| t >= from) {
            return None;
        }
        let mut removed = 0i64;
        for g in &self.gaps {
            if t >= g.to {
                removed += g.to.ticks() - g.from.ticks();
            } else if t >= g.from {
                return None;
            } else {
                break;
            }
        }
        Some(MediaTime::from_ticks(t.ticks() - removed))
    }

    /// File time at which a recording stopped at session time `at` ends: the pause point when
    /// the stop came while paused.
    pub fn end_file_time(&self, at: MediaTime) -> MediaTime {
        match self.open {
            Some(from) => MediaTime::from_ticks(from.ticks() - self.total_gap().ticks()),
            None => self.map(at).unwrap_or(at),
        }
    }

    /// Session time for a file time (the inverse of [`PauseTimeline::map`] for recorded time).
    pub fn unmap(&self, file: MediaTime) -> MediaTime {
        let mut t = file.ticks();
        for g in &self.gaps {
            if g.from.ticks() <= t {
                t += g.to.ticks() - g.from.ticks();
            }
        }
        MediaTime::from_ticks(t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(secs: i64) -> MediaTime {
        MediaTime::from_secs(secs)
    }

    /// REC-4.AC1: 10 s recorded, 20 s paused, 10 s recorded is a 20 s file.
    #[test]
    fn rec4_ac1_gap_is_removed_from_the_file_timeline() {
        let mut t = PauseTimeline::new();
        t.pause(s(10)).unwrap();
        assert_eq!(t.map(s(9)), Some(s(9)));
        assert_eq!(t.map(s(10)), None);
        assert_eq!(t.map(s(25)), None);
        t.resume(s(30)).unwrap();
        assert_eq!(t.map(s(29)), None);
        assert_eq!(t.map(s(30)), Some(s(10)));
        assert_eq!(t.map(s(40)), Some(s(20)));
        assert_eq!(t.total_gap(), s(20));
    }

    #[test]
    fn several_gaps_accumulate() {
        let mut t = PauseTimeline::new();
        t.pause(s(2)).unwrap();
        t.resume(s(5)).unwrap();
        t.pause(s(8)).unwrap();
        t.resume(s(9)).unwrap();
        assert_eq!(t.map(s(6)), Some(s(3)));
        assert_eq!(t.map(s(10)), Some(s(6)));
        assert_eq!(t.unmap(s(3)), s(6));
        assert_eq!(t.unmap(s(6)), s(10));
        assert_eq!(t.unmap(s(1)), s(1));
    }

    #[test]
    fn map_is_monotonic_and_unmap_inverts_it() {
        let mut t = PauseTimeline::new();
        t.pause(MediaTime::from_ticks(1234)).unwrap();
        t.resume(MediaTime::from_ticks(98_765)).unwrap();
        t.pause(MediaTime::from_ticks(200_000)).unwrap();
        t.resume(MediaTime::from_ticks(250_001)).unwrap();
        let mut last = None;
        for ticks in (0..400_000).step_by(997) {
            let st = MediaTime::from_ticks(ticks);
            if let Some(f) = t.map(st) {
                if let Some(l) = last {
                    assert!(f > l);
                }
                last = Some(f);
                assert_eq!(t.unmap(f), st);
            }
        }
    }

    #[test]
    fn misuse_is_rejected() {
        let mut t = PauseTimeline::new();
        assert!(t.resume(s(1)).is_err());
        t.pause(s(1)).unwrap();
        assert!(t.pause(s(2)).is_err());
        t.resume(s(3)).unwrap();
        assert!(t.pause(s(2)).is_err());
    }

    #[test]
    fn rec4_edge_at_most_500_pauses() {
        let mut t = PauseTimeline::new();
        for i in 0..MAX_PAUSES as i64 {
            t.pause(s(i * 2)).unwrap();
            t.resume(s(i * 2 + 1)).unwrap();
        }
        let e = t.pause(s(2000)).unwrap_err();
        assert!(e.to_string().contains("Too many pauses"));
    }
}
