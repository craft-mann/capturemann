//! Mapping capture timestamps to track timestamps, pause gaps included. Pure logic.
//!
//! # File time (REC-4)
//!
//! The camera file has continuous timestamps, like the screen and audio files: a pause removes
//! its span from the file. A frame is first placed on the session timeline and then shifted
//! earlier by the total length of the gaps that ended at or before it, the same mapping as
//! `PauseTimeline::map` in `cm-capture`:
//!
//! `file(t) = t - sum of the lengths of all gaps that ended at or before t`
//!
//! The gap list (session time) stays in `details.pauses`, so the file time can be mapped back.
//!
//! # Which timestamp
//!
//! A frame is placed on the session timeline by its **capture presentation time** (the
//! `CMSampleBuffer` PTS, which AVFoundation stamps on the host clock, the same `CLOCK_UPTIME_RAW`
//! timebase as [`SessionClock`]), never by the time the callback ran. The PTS is the camera
//! stack's statement of when the image was captured and is free of the queueing jitter that
//! delivery time carries (S6 measured the delivery about 75 ms behind the PTS, with spread). The
//! delivery time is used only for statistics.
//!
//! The PTS can itself be late relative to the optical event (USB webcams often stamp when the
//! frame arrives, after exposure and transfer). [`CameraConfig::latency_compensation_ms`]
//! shifts frames earlier by the measured amount; see tests/manual/CAM-2.md.
//!
//! [`CameraConfig::latency_compensation_ms`]: crate::CameraConfig::latency_compensation_ms

use cm_core::{MediaTime, PauseGap, SessionClock, TICKS_PER_SECOND};

/// What to do with a frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Retimed {
    /// Write it at `pts` (file time: session time minus the pause gaps before it) for
    /// `duration`. `session_pts` is the same instant on the session timeline. `missed_slots`
    /// frames were skipped since the previous written frame (not counting pause gaps).
    Write {
        pts: MediaTime,
        session_pts: MediaTime,
        duration: MediaTime,
        missed_slots: u32,
    },
    /// Captured before the session origin (after compensation).
    BeforeOrigin,
    /// Captured inside a pause gap.
    Paused,
    /// Not later than the previous written frame (writers need strictly increasing times).
    NotMonotonic,
}

#[derive(Debug)]
pub struct Retimer {
    clock: SessionClock,
    interval: i64,
    comp_ticks: i64,
    last_pts: Option<i64>,
    gaps: Vec<PauseGap>,
    open_pause: Option<MediaTime>,
}

impl Retimer {
    pub fn new(clock: SessionClock, fps: u32, latency_compensation_ms: f64) -> Self {
        let fps = i64::from(fps.max(1));
        Retimer {
            clock,
            interval: (TICKS_PER_SECOND + fps / 2) / fps,
            comp_ticks: (latency_compensation_ms * TICKS_PER_SECOND as f64 / 1000.0).round() as i64,
            last_pts: None,
            gaps: Vec::new(),
            open_pause: None,
        }
    }

    /// Nominal frame duration.
    pub fn interval(&self) -> MediaTime {
        MediaTime::from_ticks(self.interval)
    }

    pub fn pause(&mut self, at: MediaTime) {
        if self.open_pause.is_none() {
            self.open_pause = Some(at);
        }
    }

    pub fn resume(&mut self, at: MediaTime) {
        if let Some(from) = self.open_pause.take() {
            self.gaps.push(PauseGap {
                from,
                to: at.max(from),
            });
        }
    }

    /// Closes an open pause at `at` and returns all gaps.
    pub fn finish(&mut self, at: MediaTime) -> Vec<PauseGap> {
        self.resume(at);
        self.gaps.clone()
    }

    pub fn gaps(&self) -> &[PauseGap] {
        &self.gaps
    }

    /// The session time of a capture stamp after compensation.
    pub fn session_time(&self, capture_host_ns: u64) -> MediaTime {
        MediaTime::from_ticks(
            self.clock
                .from_host_ns(capture_host_ns)
                .ticks()
                .saturating_sub(self.comp_ticks),
        )
    }

    /// File time of a session time outside every gap: the closed gaps that ended at or before it
    /// are removed.
    pub fn file_time(&self, t: MediaTime) -> MediaTime {
        let removed: i64 = self
            .gaps
            .iter()
            .filter(|g| g.to <= t)
            .map(|g| g.to.ticks() - g.from.ticks())
            .sum();
        MediaTime::from_ticks(t.ticks() - removed)
    }

    fn in_pause(&self, t: MediaTime) -> bool {
        if matches!(self.open_pause, Some(from) if t >= from) {
            return true;
        }
        self.gaps.iter().any(|g| t >= g.from && t < g.to)
    }

    fn pause_between(&self, a: i64, b: i64) -> bool {
        self.open_pause.is_some()
            || self
                .gaps
                .iter()
                .any(|g| g.to.ticks() >= a && g.from.ticks() <= b)
    }

    /// Decides what to do with a frame captured at `capture_host_ns`.
    pub fn map(&mut self, capture_host_ns: u64) -> Retimed {
        let t = self.session_time(capture_host_ns);
        if t.ticks() < 0 {
            return Retimed::BeforeOrigin;
        }
        if self.in_pause(t) {
            return Retimed::Paused;
        }
        let mut missed = 0;
        if let Some(last) = self.last_pts {
            if t.ticks() <= last {
                return Retimed::NotMonotonic;
            }
            let gap = t.ticks() - last;
            if gap * 2 > self.interval * 3 && !self.pause_between(last, t.ticks()) {
                missed =
                    u32::try_from(((gap + self.interval / 2) / self.interval) - 1).unwrap_or(0);
            }
        }
        self.last_pts = Some(t.ticks());
        Retimed::Write {
            pts: self.file_time(t),
            session_pts: t,
            duration: MediaTime::from_ticks(self.interval),
            missed_slots: missed,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use cm_core::ManualHostClock;

    use super::*;

    const MS: u64 = 1_000_000;

    fn retimer(comp_ms: f64) -> (Retimer, u64) {
        let origin = 10_000 * MS;
        let host = Arc::new(ManualHostClock::new(origin));
        (
            Retimer::new(SessionClock::with_host(host), 30, comp_ms),
            origin,
        )
    }

    #[test]
    fn uses_capture_time_on_the_session_clock() {
        let (mut r, o) = retimer(0.0);
        match r.map(o + 100 * MS) {
            Retimed::Write { pts, duration, .. } => {
                assert_eq!(pts.ticks(), 12_000); // 100 ms
                assert_eq!(duration.ticks(), 4000); // 1/30 s
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn compensation_shifts_frames_earlier() {
        let (mut r, o) = retimer(50.0);
        let Retimed::Write { pts, .. } = r.map(o + 100 * MS) else {
            panic!()
        };
        assert_eq!(pts.ticks(), 6000); // 50 ms
        let (mut r, o) = retimer(-20.0);
        let Retimed::Write { pts, .. } = r.map(o + 100 * MS) else {
            panic!()
        };
        assert_eq!(pts.ticks(), 14_400);
    }

    #[test]
    fn drops_before_origin_and_non_monotonic() {
        let (mut r, o) = retimer(0.0);
        assert_eq!(r.map(o - 5 * MS), Retimed::BeforeOrigin);
        assert!(matches!(r.map(o + 40 * MS), Retimed::Write { .. }));
        assert_eq!(r.map(o + 40 * MS), Retimed::NotMonotonic);
        assert_eq!(r.map(o + 30 * MS), Retimed::NotMonotonic);
        assert!(matches!(r.map(o + 73 * MS), Retimed::Write { .. }));
    }

    #[test]
    fn counts_missed_slots_but_not_across_pauses() {
        let (mut r, o) = retimer(0.0);
        r.map(o + 33 * MS);
        // 100 ms later: two frames missed.
        let Retimed::Write { missed_slots, .. } = r.map(o + 133 * MS) else {
            panic!()
        };
        assert_eq!(missed_slots, 2);
        r.pause(MediaTime::from_millis(150));
        r.resume(MediaTime::from_millis(1000));
        let Retimed::Write { missed_slots, .. } = r.map(o + 1033 * MS) else {
            panic!()
        };
        assert_eq!(missed_slots, 0);
    }

    #[test]
    fn pause_gap_is_decided_by_capture_time_not_delivery() {
        let (mut r, o) = retimer(0.0);
        assert!(matches!(r.map(o + 33 * MS), Retimed::Write { .. }));
        r.pause(MediaTime::from_millis(100));
        // A frame captured before the pause but delivered after it is still kept.
        assert!(matches!(r.map(o + 99 * MS), Retimed::Write { .. }));
        assert_eq!(r.map(o + 133 * MS), Retimed::Paused);
        r.resume(MediaTime::from_millis(500));
        assert_eq!(r.map(o + 400 * MS), Retimed::Paused);
        assert!(matches!(r.map(o + 500 * MS), Retimed::Write { .. }));
        assert_eq!(
            r.gaps(),
            &[PauseGap {
                from: MediaTime::from_millis(100),
                to: MediaTime::from_millis(500)
            }]
        );
    }

    #[test]
    fn frames_after_a_gap_are_shifted_by_the_accumulated_gaps() {
        let (mut r, o) = retimer(0.0);
        let Retimed::Write {
            pts, session_pts, ..
        } = r.map(o + 100 * MS)
        else {
            panic!()
        };
        assert_eq!((pts.ticks(), session_pts.ticks()), (12_000, 12_000));
        r.pause(MediaTime::from_millis(133));
        r.resume(MediaTime::from_millis(1133)); // 1 s gap
                                                // The first frame at the resume lands one interval after the last frame before it.
        let Retimed::Write {
            pts, session_pts, ..
        } = r.map(o + 1133 * MS)
        else {
            panic!()
        };
        assert_eq!(session_pts, MediaTime::from_millis(1133));
        assert_eq!(pts, MediaTime::from_millis(133));
        r.pause(MediaTime::from_millis(2000));
        r.resume(MediaTime::from_millis(2500)); // 0.5 s more
        let Retimed::Write {
            pts, missed_slots, ..
        } = r.map(o + 2533 * MS)
        else {
            panic!()
        };
        assert_eq!(pts, MediaTime::from_millis(2533 - 1500));
        assert_eq!(missed_slots, 0);
        assert_eq!(
            r.file_time(MediaTime::from_millis(50)),
            MediaTime::from_millis(50)
        );
    }

    #[test]
    fn open_pause_is_closed_by_finish() {
        let (mut r, _) = retimer(0.0);
        r.pause(MediaTime::from_millis(10));
        let gaps = r.finish(MediaTime::from_millis(90));
        assert_eq!(gaps.len(), 1);
        assert_eq!(gaps[0].to, MediaTime::from_millis(90));
    }
}
