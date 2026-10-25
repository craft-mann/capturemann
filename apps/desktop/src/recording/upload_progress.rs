//! The running-upload toast's display-free logic (redesign R17): throughput and ETA from the
//! `job.progress` events, the warm-up rule, the stall rule and the repaint throttle.
//!
//! `job.progress` of an upload carries `progress` (0..=1) and an optional `message`; it does not
//! carry bytes, so the ETA comes from the fraction alone. Nothing here touches a window or a clock:
//! every function takes `now`.

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

/// Events the rolling average looks at.
pub const WINDOW: usize = 6;
/// No ETA before this many events...
pub const MIN_SAMPLES: usize = 3;
/// ...and this much time between the first and the last of them.
pub const MIN_SPAN: Duration = Duration::from_secs(3);
/// No progress for this long: the toast says "Waiting...".
pub const STALL_AFTER: Duration = Duration::from_secs(10);
/// The toast is repainted at most this often.
pub const REPAINT_EVERY: Duration = Duration::from_millis(500);
/// An ETA beyond this is not shown (a stalled link, not an estimate).
pub const MAX_ETA: Duration = Duration::from_secs(24 * 3600);

/// How the time left is worded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Eta {
    /// Not enough data yet (or an absurd estimate).
    Unknown,
    /// No progress for [`STALL_AFTER`].
    Waiting,
    LessThanMinute,
    /// Rounded to 5 s, 5 to 45.
    Seconds(u32),
    /// Rounded to the nearest minute, at least 1.
    Minutes(u32),
}

/// What the toast shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProgressView {
    /// 0..=99 while running (100 is the Done toast's).
    pub percent: u8,
    pub eta: Eta,
}

/// Words a time left: seconds under a minute, minutes after, "less than a minute" at the very end.
pub fn eta_from_remaining(left: Duration) -> Eta {
    if left > MAX_ETA {
        return Eta::Unknown;
    }
    let s = left.as_secs_f64();
    if s < 5.0 {
        Eta::LessThanMinute
    } else if s < 45.0 {
        Eta::Seconds(((s / 5.0).round() as u32 * 5).max(5))
    } else {
        Eta::Minutes(((s / 60.0).round() as u32).max(1))
    }
}

/// Throughput of one upload from its recent events.
#[derive(Debug, Clone, Default)]
pub struct Throughput {
    first_at: Option<Instant>,
    samples: VecDeque<(Instant, f64)>,
    count: usize,
}

impl Throughput {
    pub fn push(&mut self, now: Instant, progress: f64) {
        let progress = progress.clamp(0.0, 1.0);
        // Progress that goes back (a restarted file) starts the estimate again.
        if self.samples.back().is_some_and(|&(_, p)| progress < p) {
            *self = Throughput::default();
        }
        self.first_at.get_or_insert(now);
        self.count += 1;
        self.samples.push_back((now, progress));
        while self.samples.len() > WINDOW {
            self.samples.pop_front();
        }
    }

    /// Time left, from the rolling average of the last [`WINDOW`] events. `Unknown` before
    /// [`MIN_SAMPLES`] events and [`MIN_SPAN`] of data, and when the rate is not positive.
    pub fn eta(&self, now: Instant) -> Eta {
        let (Some(&(t0, p0)), Some(&(t1, p1)), Some(first)) =
            (self.samples.front(), self.samples.back(), self.first_at)
        else {
            return Eta::Unknown;
        };
        if self.count < MIN_SAMPLES || t1.saturating_duration_since(first) < MIN_SPAN {
            return Eta::Unknown;
        }
        let dt = t1.saturating_duration_since(t0).as_secs_f64();
        let dp = p1 - p0;
        if dt <= 0.0 || dp <= 0.0 {
            return Eta::Unknown;
        }
        // The time since the last event counts too: a stall stretches the estimate.
        let quiet = now.saturating_duration_since(t1).as_secs_f64();
        let left = (1.0 - p1).max(0.0) / (dp / dt) - quiet;
        if !left.is_finite() {
            return Eta::Unknown;
        }
        if left <= 0.0 {
            return Eta::LessThanMinute;
        }
        eta_from_remaining(Duration::from_secs_f64(left.min(1.0e9)))
    }

    pub fn last_event(&self) -> Option<Instant> {
        self.samples.back().map(|&(t, _)| t)
    }
}

#[derive(Debug, Clone)]
struct Job {
    through: Throughput,
    progress: f64,
    last_paint: Option<Instant>,
    /// The toast already says "Waiting...".
    waiting: bool,
    /// The user closed the toast: no new one until the job ends.
    suppressed: bool,
    /// A toast was shown for this job (so a stall has something to update).
    shown: bool,
}

/// The progress of every running upload and which repaints they earn.
#[derive(Debug, Default)]
pub struct ProgressTracker {
    jobs: HashMap<String, Job>,
}

fn percent_of(p: f64) -> u8 {
    ((p.clamp(0.0, 1.0) * 100.0).floor() as u8).min(99)
}

impl ProgressTracker {
    /// One `job.progress`. Returns the view to paint, or `None` when it is too soon since the
    /// last paint (and nothing visible changed enough to matter) or the toast was closed.
    pub fn on_progress(
        &mut self,
        job_id: &str,
        progress: f64,
        now: Instant,
    ) -> Option<ProgressView> {
        let j = self.jobs.entry(job_id.to_owned()).or_insert_with(|| Job {
            through: Throughput::default(),
            progress: 0.0,
            last_paint: None,
            waiting: false,
            suppressed: false,
            shown: false,
        });
        j.through.push(now, progress);
        j.progress = progress.clamp(0.0, 1.0);
        j.waiting = false;
        if j.suppressed {
            return None;
        }
        if j.last_paint
            .is_some_and(|t| now.saturating_duration_since(t) < REPAINT_EVERY)
        {
            return None;
        }
        j.last_paint = Some(now);
        j.shown = true;
        Some(ProgressView {
            percent: percent_of(j.progress),
            eta: j.through.eta(now),
        })
    }

    /// The jobs that went quiet for [`STALL_AFTER`] since their last event, once each, as
    /// "Waiting..." views. Call it from a timer while an upload toast is on screen.
    pub fn stalled(&mut self, now: Instant) -> Vec<(String, ProgressView)> {
        let mut out = Vec::new();
        for (id, j) in &mut self.jobs {
            if j.waiting || j.suppressed || !j.shown {
                continue;
            }
            let quiet = j
                .through
                .last_event()
                .is_some_and(|t| now.saturating_duration_since(t) >= STALL_AFTER);
            if quiet {
                j.waiting = true;
                j.last_paint = Some(now);
                out.push((
                    id.clone(),
                    ProgressView {
                        percent: percent_of(j.progress),
                        eta: Eta::Waiting,
                    },
                ));
            }
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    /// The job ended (done, failed) or paused: forget it, a retry starts from scratch.
    pub fn finish(&mut self, job_id: &str) {
        self.jobs.remove(job_id);
    }

    /// The user closed the running toast of this job.
    pub fn dismissed(&mut self, job_id: &str) {
        if let Some(j) = self.jobs.get_mut(job_id) {
            j.suppressed = true;
        }
    }

    /// Whether some running upload still has a toast that a timer must watch.
    pub fn watching(&self) -> bool {
        self.jobs.values().any(|j| j.shown && !j.suppressed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(t0: Instant, ms: u64) -> Instant {
        t0 + Duration::from_millis(ms)
    }

    /// Steady 1 %/s: after the warm-up the ETA is the time to 100 %.
    #[test]
    fn eta_from_steady_throughput_after_warm_up() {
        let t0 = Instant::now();
        let mut th = Throughput::default();
        for s in 0..=5u64 {
            th.push(at(t0, s * 1000), s as f64 / 100.0);
        }
        // 5 % done at 5 s, 1 %/s: 95 s left, "about 2 min" rounded.
        assert_eq!(th.eta(at(t0, 5000)), Eta::Minutes(2));
    }

    #[test]
    fn no_eta_before_three_samples_and_three_seconds() {
        let t0 = Instant::now();
        let mut th = Throughput::default();
        th.push(t0, 0.0);
        th.push(at(t0, 4000), 0.04);
        assert_eq!(th.eta(at(t0, 4000)), Eta::Unknown, "two samples");
        th.push(at(t0, 4500), 0.045);
        assert_ne!(th.eta(at(t0, 4500)), Eta::Unknown, "three samples, 4.5 s");
        let mut th = Throughput::default();
        for ms in [0, 1000, 2000] {
            th.push(at(t0, ms), ms as f64 / 100_000.0);
        }
        assert_eq!(th.eta(at(t0, 2000)), Eta::Unknown, "three samples, 2 s");
    }

    /// The rolling window follows a speed change instead of averaging the whole upload.
    #[test]
    fn the_average_follows_recent_events() {
        let t0 = Instant::now();
        let mut th = Throughput::default();
        // Slow: 1 % per 10 s for 30 s, then fast: 5 % per second.
        for i in 0..=3u64 {
            th.push(at(t0, i * 10_000), i as f64 / 100.0);
        }
        for i in 1..=6u64 {
            th.push(at(t0, 30_000 + i * 1000), 0.03 + i as f64 * 0.05);
        }
        // The last 6 events are all fast: 67 % left at 5 %/s, about 13 s.
        let Eta::Seconds(s) = th.eta(at(t0, 36_000)) else {
            panic!("{:?}", th.eta(at(t0, 36_000)))
        };
        assert!((10..=20).contains(&s), "{s}");
    }

    #[test]
    fn never_negative_or_absurd() {
        let t0 = Instant::now();
        let mut flat = Throughput::default();
        for s in 0..6u64 {
            flat.push(at(t0, s * 1000), 0.5);
        }
        assert_eq!(flat.eta(at(t0, 5000)), Eta::Unknown, "no progress at all");
        // A crawl: 0.0000001 per second would take years.
        let mut crawl = Throughput::default();
        for s in 0..6u64 {
            crawl.push(at(t0, s * 1000), s as f64 * 1e-7);
        }
        assert_eq!(crawl.eta(at(t0, 5000)), Eta::Unknown);
        // Done: nothing left is "less than a minute", never below zero.
        let mut done = Throughput::default();
        for s in 0..6u64 {
            done.push(at(t0, s * 1000), 0.9 + s as f64 * 0.02);
        }
        assert_eq!(done.eta(at(t0, 5000)), Eta::LessThanMinute);
        assert_eq!(done.eta(at(t0, 500_000)), Eta::LessThanMinute);
        // Progress that goes back starts again.
        let mut back = Throughput::default();
        for s in 0..6u64 {
            back.push(at(t0, s * 1000), 0.5 + s as f64 * 0.05);
        }
        back.push(at(t0, 6000), 0.1);
        assert_eq!(back.eta(at(t0, 6000)), Eta::Unknown);
    }

    #[test]
    fn rounding() {
        let s = Duration::from_secs;
        assert_eq!(eta_from_remaining(s(2)), Eta::LessThanMinute);
        assert_eq!(eta_from_remaining(s(7)), Eta::Seconds(5));
        assert_eq!(eta_from_remaining(s(23)), Eta::Seconds(25));
        assert_eq!(eta_from_remaining(s(44)), Eta::Seconds(45));
        assert_eq!(eta_from_remaining(s(50)), Eta::Minutes(1));
        assert_eq!(eta_from_remaining(s(95)), Eta::Minutes(2));
        assert_eq!(eta_from_remaining(s(3600)), Eta::Minutes(60));
        assert_eq!(eta_from_remaining(s(48 * 3600)), Eta::Unknown);
    }

    /// At most one repaint per 500 ms, the first event always paints.
    #[test]
    fn repaints_are_throttled() {
        let t0 = Instant::now();
        let mut tr = ProgressTracker::default();
        assert!(tr.on_progress("j", 0.01, t0).is_some());
        assert!(tr.on_progress("j", 0.02, at(t0, 100)).is_none());
        assert!(tr.on_progress("j", 0.03, at(t0, 499)).is_none());
        let v = tr.on_progress("j", 0.04, at(t0, 500)).unwrap();
        assert_eq!(v.percent, 4);
    }

    /// 0 % is a valid first view; 100 % is shown as 99 (Done replaces the toast).
    #[test]
    fn percent_is_floored_and_capped() {
        let t0 = Instant::now();
        let mut tr = ProgressTracker::default();
        assert_eq!(tr.on_progress("j", 0.0, t0).unwrap().percent, 0);
        assert_eq!(tr.on_progress("j", 0.999, at(t0, 600)).unwrap().percent, 99);
        assert_eq!(tr.on_progress("j", 1.0, at(t0, 1200)).unwrap().percent, 99);
    }

    #[test]
    fn a_stalled_upload_says_waiting_once_after_ten_seconds() {
        let t0 = Instant::now();
        let mut tr = ProgressTracker::default();
        tr.on_progress("j", 0.3, t0);
        assert!(tr.stalled(at(t0, 9_999)).is_empty());
        let s = tr.stalled(at(t0, 10_000));
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].1.eta, Eta::Waiting);
        assert_eq!(s[0].1.percent, 30);
        assert!(tr.stalled(at(t0, 20_000)).is_empty(), "only once");
        // Progress again clears it, and a later stall is news again.
        assert_ne!(
            tr.on_progress("j", 0.31, at(t0, 21_000)).unwrap().eta,
            Eta::Waiting
        );
        assert_eq!(tr.stalled(at(t0, 31_000)).len(), 1);
    }

    #[test]
    fn two_uploads_do_not_disturb_each_other() {
        let t0 = Instant::now();
        let mut tr = ProgressTracker::default();
        tr.on_progress("a", 0.1, t0);
        tr.on_progress("b", 0.5, at(t0, 100));
        // b is throttled by its own clock, not by a's.
        assert!(tr.on_progress("a", 0.2, at(t0, 600)).is_some());
        assert!(tr.on_progress("b", 0.6, at(t0, 650)).is_some());
        tr.on_progress("b", 0.7, at(t0, 9_000));
        let s = tr.stalled(at(t0, 12_000));
        assert_eq!(s.len(), 1, "a is quiet since 0.6 s, b since 9 s");
        assert_eq!(s[0].0, "a");
    }

    /// A closed toast stays closed while the job runs; a retry after a failure starts fresh.
    #[test]
    fn dismissed_stays_dismissed_until_the_job_ends_and_retry_starts_over() {
        let t0 = Instant::now();
        let mut tr = ProgressTracker::default();
        tr.on_progress("j", 0.1, t0);
        tr.dismissed("j");
        assert!(tr.on_progress("j", 0.2, at(t0, 1000)).is_none());
        assert!(tr.stalled(at(t0, 60_000)).is_empty());
        assert!(!tr.watching());
        tr.finish("j");
        let v = tr.on_progress("j", 0.05, at(t0, 70_000)).unwrap();
        assert_eq!((v.percent, v.eta), (5, Eta::Unknown), "fresh estimate");
        assert!(tr.watching());
    }

    #[test]
    fn finish_forgets_the_job() {
        let t0 = Instant::now();
        let mut tr = ProgressTracker::default();
        tr.on_progress("j", 0.1, t0);
        tr.finish("j");
        assert!(tr.stalled(at(t0, 60_000)).is_empty());
        assert!(!tr.watching());
    }
}
