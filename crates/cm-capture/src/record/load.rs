//! The encoder load monitor: decides, from how many slot writes the encoder refused over the last
//! few seconds, whether it cannot keep up with the capture rate (a 4K30 recording on an integrated
//! GPU) and what rate it can sustain. Pure Rust, tested on every OS.
//!
//! The answer is a *stride*: the pipeline then offers the encoder only every `stride`-th slot of
//! its constant frame grid (30 fps with stride 2 is 15 fps). The slot grid, and with it every
//! sample time, is unchanged, so the file keeps the length of the recording and stays in sync with
//! the audio; only the number of distinct frames in it falls (variable frame rate).

use std::collections::VecDeque;

/// Seconds of attempts the decision is based on (and the least time between two decisions).
pub const LOAD_WINDOW_SECS: i64 = 3;
/// More than this share of refused attempts in the window is an overload.
pub const LOAD_DROP_SHARE: f64 = 0.10;
/// Fewer attempts than this in the window are not a rate.
pub const LOAD_MIN_ATTEMPTS: u64 = 20;
/// The lowest rate offered: a fraction `1 / MAX_STRIDE` of the asked rate.
pub const MAX_STRIDE: u32 = 6;
/// The sustainable rate is taken as this share of what the encoder was seen to accept.
const HEADROOM: f64 = 0.9;

/// A decision to lower the rate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RateChange {
    pub from_stride: u32,
    pub to_stride: u32,
    /// Attempts and refusals in the window the decision rests on.
    pub attempted: u64,
    pub dropped: u64,
    pub window_secs: i64,
    /// The rates before and after, frames per second.
    pub from_fps: f64,
    pub to_fps: f64,
    /// The rate asked for.
    pub asked_fps: u32,
}

#[derive(Debug, Clone)]
pub struct LoadMonitor {
    fps: u32,
    stride: u32,
    /// (second, attempts, refused)
    buckets: VecDeque<(i64, u64, u64)>,
    since: Option<i64>,
}

impl LoadMonitor {
    pub fn new(fps: u32) -> Self {
        LoadMonitor {
            fps: fps.max(1),
            stride: 1,
            buckets: VecDeque::new(),
            since: None,
        }
    }

    pub fn stride(&self) -> u32 {
        self.stride
    }

    /// The rate offered to the encoder now.
    pub fn effective_fps(&self) -> f64 {
        f64::from(self.fps) / f64::from(self.stride)
    }

    /// One slot offered to the encoder at session second `second`; `refused` when it said no.
    pub fn record(&mut self, second: i64, refused: bool) {
        self.since.get_or_insert(second);
        match self.buckets.back_mut() {
            Some(b) if b.0 == second => {
                b.1 += 1;
                b.2 += u64::from(refused);
            }
            _ => self.buckets.push_back((second, 1, u64::from(refused))),
        }
        while self
            .buckets
            .front()
            .is_some_and(|b| b.0 <= second - LOAD_WINDOW_SECS)
        {
            self.buckets.pop_front();
        }
    }

    /// Looks at the window ending at `now_second`; on an overload lowers the stride and says so.
    /// After a decision the next one waits for a fresh window at the new rate.
    pub fn evaluate(&mut self, now_second: i64) -> Option<RateChange> {
        let since = self.since?;
        if now_second - since < LOAD_WINDOW_SECS || self.stride >= MAX_STRIDE {
            return None;
        }
        let (mut attempted, mut dropped) = (0u64, 0u64);
        for b in &self.buckets {
            if b.0 > now_second - LOAD_WINDOW_SECS {
                attempted += b.1;
                dropped += b.2;
            }
        }
        if attempted < LOAD_MIN_ATTEMPTS || (dropped as f64) <= LOAD_DROP_SHARE * attempted as f64 {
            return None;
        }
        let accepted_per_s = (attempted - dropped) as f64 / LOAD_WINDOW_SECS as f64;
        let sustainable = accepted_per_s * HEADROOM;
        let wanted = if sustainable > 0.0 {
            (f64::from(self.fps) / sustainable).ceil() as u32
        } else {
            MAX_STRIDE
        };
        let to = wanted.clamp(self.stride + 1, MAX_STRIDE);
        let change = RateChange {
            from_stride: self.stride,
            to_stride: to,
            attempted,
            dropped,
            window_secs: LOAD_WINDOW_SECS,
            from_fps: self.effective_fps(),
            to_fps: f64::from(self.fps) / f64::from(to),
            asked_fps: self.fps,
        };
        self.stride = to;
        self.buckets.clear();
        self.since = Some(now_second);
        Some(change)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scripted rate-limited encoder: accepts `capacity` slots a second, in bursts at the start
    /// of each second.
    fn run(fps: u32, capacity: u64, secs: i64) -> (LoadMonitor, Vec<RateChange>) {
        let mut m = LoadMonitor::new(fps);
        let mut changes = Vec::new();
        for s in 0..secs {
            let per_s = (u64::from(fps) / u64::from(m.stride())).max(1);
            for i in 0..per_s {
                m.record(s, i >= capacity);
            }
            changes.extend(m.evaluate(s));
        }
        (m, changes)
    }

    #[test]
    fn a_healthy_encoder_never_changes_the_rate() {
        let (m, c) = run(30, 30, 30);
        assert!(c.is_empty());
        assert_eq!(m.stride(), 1);
    }

    #[test]
    fn an_encoder_that_manages_13_of_30_fps_is_brought_down_within_four_seconds() {
        let mut m = LoadMonitor::new(30);
        let mut first = None;
        for s in 0..10 {
            for i in 0..30 {
                m.record(s, i >= 13);
            }
            if let Some(c) = m.evaluate(s) {
                first.get_or_insert((s, c));
            }
        }
        let (at, c) = first.unwrap();
        assert!(at <= 3, "decided at second {at}");
        // 13 fps seen, 90 % of it is 11.7: 30 / 11.7 = 2.6, so every third slot (10 fps).
        assert_eq!((c.from_stride, c.to_stride), (1, 3));
        assert!(c.dropped > 0 && c.dropped <= c.attempted);
        assert_eq!(c.to_fps, 10.0);
        assert_eq!(c.asked_fps, 30);
    }

    #[test]
    fn after_the_step_the_new_rate_is_sustained_and_stays() {
        let (m, c) = run(30, 13, 30);
        assert_eq!(c.len(), 1, "{c:?}");
        assert_eq!(m.stride(), 3);
    }

    #[test]
    fn a_still_overloaded_rate_steps_down_again_but_never_below_the_floor() {
        let (m, c) = run(30, 2, 60);
        assert!(!c.is_empty());
        assert_eq!(m.stride(), MAX_STRIDE);
        // Refusing a little less takes two steps: 6 of 30 fps accepted is 5.4 sustainable, 6.
        assert!(c.iter().all(|c| c.to_stride > c.from_stride));
        // At the floor the monitor stops deciding.
        assert!(m.clone().evaluate(1000).is_none());
    }

    #[test]
    fn a_short_burst_of_refusals_is_not_an_overload() {
        let mut m = LoadMonitor::new(30);
        for s in 0..10 {
            for i in 0..30 {
                // 2 refusals a second = 6.7 %.
                m.record(s, i < 2);
            }
            assert!(m.evaluate(s).is_none());
        }
    }

    #[test]
    fn too_few_attempts_are_not_a_rate() {
        let mut m = LoadMonitor::new(30);
        for s in 0..10 {
            m.record(s, true);
            m.record(s, true);
            assert!(m.evaluate(s).is_none());
        }
    }
}
