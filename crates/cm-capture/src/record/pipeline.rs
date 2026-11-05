//! The frame pipeline between a capture source and the encoder: the platform-independent core of
//! the screen track, tested with fake frames.
//!
//! What it does to every frame the OS delivers:
//!
//! 1. Drops frames stamped before the session origin, or inside a pause gap (REC-4).
//! 2. Maps session time to file time (continuous across gaps) and snaps the frame to a
//!    constant-frame-rate slot grid. Frames that land on an occupied slot are coalesced (the OS
//!    may deliver faster than the target fps), and empty slots are filled by repeating the last
//!    frame, so the file is constant frame rate (REC-3: "static moments repeat the last frame")
//!    with exactly `fps x seconds` frames (REC-3.AC3).
//! 3. Counts late frames the way S6 did (same spirit as S3): a frame is late if it reached us
//!    more than one frame interval behind the steady-state delivery lag (the running median of
//!    "now minus capture stamp"), or a delivery slot was missed, or the encoder was not ready or
//!    failed. NFR-2 reads `late / slots`.
//!
//! Stalls (R3-7): once the source has shown that it reports idle status samples for an unchanged
//! screen (SCK does), a gap between two frames with no idle sample in it is a stall, however
//! long: every missed slot counts as late and a watchdog ([`Pipeline::watchdog`]) reports a
//! delivery silence longer than [`STALL_TICKS`] (`stalls`, `stall_ms`). A source that never sent
//! an idle sample cannot be told apart from a static screen, so gaps of 250 ms or more stay
//! "static" for it, as before.
//!
//! A heartbeat (`heartbeat`) keeps filling slots during static content so fragments keep flowing
//! and a crash during an idle screen still leaves media up to the last 2 s (REC-8.AC1).

use std::collections::VecDeque;

use cm_core::{CmError, MediaTime, PauseGap, TrackStats, TICKS_PER_SECOND};
use cm_encode::{AppendOutcome, VideoEncoder, WriterSummary};
use serde::Serialize;

use super::load::{LoadMonitor, RateChange};
use super::timeline::PauseTimeline;

/// A silence between deliveries longer than this is treated as a static screen, not a stall.
const STATIC_GAP_TICKS: i64 = TICKS_PER_SECOND / 4;
/// No delivery of any kind for this long, from a source that reports idle samples, is a stall.
pub const STALL_TICKS: i64 = TICKS_PER_SECOND;
/// Histogram bin for the lag median: 0.5 ms.
const BIN_TICKS: i64 = TICKS_PER_SECOND / 2000;
const FINAL_SLOT_TRIES: u32 = 100;
const FINAL_SLOT_WAIT: std::time::Duration = std::time::Duration::from_millis(10);
const BINS: usize = 4000; // up to 2 s of lag
/// Window of the health monitor (REC-3: "late frames exceed 2% over the last 30 s").
pub const HEALTH_WINDOW_SECS: i64 = 30;
/// A rate over fewer frames than this is not a rate: a screen that changes once a second (a mostly
/// static desktop captured by Windows.Graphics.Capture, which sends frames only when the picture changes)
/// has 30 frames in the window, and two late ones would read as "7 % late". Three seconds of 30 fps.
const HEALTH_MIN_OBSERVED: u64 = 90;

/// Why a recording stopped by itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "reason", content = "message", rename_all = "snake_case")]
pub enum HaltReason {
    /// The window or display went away (REC-1 edge cases) or the stream failed.
    SourceStopped(String),
    /// Screen Recording access was withdrawn while recording (PRM-1).
    PermissionRevoked(String),
    /// Free space fell below the clean-stop threshold (NFR-7).
    DiskFull,
    /// The encoder or the file writer failed.
    EncoderFailed(String),
}

/// Counters, all cumulative.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct PipelineStats {
    /// Frames with an image delivered by the OS.
    pub delivered: u64,
    /// Status-only samples (screen unchanged) delivered by the OS.
    pub idle: u64,
    /// Samples appended to the file, repeats included. Every slot the pipeline offers the encoder
    /// ends as exactly one of `written` or [`PipelineStats::dropped`]; `delivered` counts the OS's
    /// frames, a different unit (the OS may deliver more or fewer than there are slots).
    pub written: u64,
    /// Of `written`, repeats of the previous frame (constant frame rate).
    pub repeated: u64,
    /// Frames that landed on an already occupied slot (OS faster than the target fps).
    pub coalesced: u64,
    /// Encoder not ready: the slot was not written (its time stays empty in the file).
    pub not_ready: u64,
    /// Encoder or disk failure while appending.
    pub append_failed: u64,
    /// Frames inside a pause gap (not recorded by design).
    pub paused_drops: u64,
    /// Frames stamped before the session origin.
    pub before_origin: u64,
    /// Delivery slots with no sample, counted from gaps shorter than a static screen.
    pub missed_slots: u64,
    /// Frames that arrived more than one interval behind the steady-state lag.
    pub lag_late: u64,
    /// Capture stalls: silences over [`STALL_TICKS`] from a source that reports idle samples.
    pub stalls: u64,
    /// Total length of those stalls, milliseconds (an open one counts up to its last update).
    pub stall_ms: u64,
    /// Slots not offered to the encoder on purpose because the encoder could not keep up and the
    /// rate was lowered (see [`LoadMonitor`]); not counted as dropped or late.
    pub rate_skipped: u64,
    /// How often the offered rate was lowered.
    pub rate_changes: u64,
}

impl PipelineStats {
    /// Late frames for NFR-2: frames that arrived more than one interval behind the steady lag,
    /// delivery slots with no sample (`missed_slots`), and slots the encoder refused (`dropped`).
    /// A count of events, not of distinct frames: it can exceed the slots in a file, so
    /// [`Pipeline::late_pct`] clamps the share to 100.
    pub fn late(&self) -> u64 {
        self.lag_late + self.missed_slots + self.dropped()
    }

    /// Slots the encoder did not take: the file has no sample for them (and the sample before
    /// them lasts through their time, so the file keeps its length).
    pub fn dropped(&self) -> u64 {
        self.not_ready + self.append_failed
    }

    /// Slots offered to the encoder: `written + dropped`.
    pub fn slots_offered(&self) -> u64 {
        self.written + self.dropped()
    }

    pub fn track_stats(&self) -> TrackStats {
        TrackStats {
            delivered: self.delivered,
            written: self.written,
            dropped: self.dropped(),
            late: self.late(),
        }
    }
}

/// Running median of the delivery lag, from a histogram, to flag late frames online.
#[derive(Debug, Clone)]
pub struct LateTracker {
    hist: Vec<u32>,
    n: u64,
    baseline: i64,
    interval: i64,
}

impl LateTracker {
    pub fn new(interval_ticks: i64) -> Self {
        LateTracker {
            hist: vec![0; BINS],
            n: 0,
            baseline: 0,
            interval: interval_ticks,
        }
    }

    /// Records the lag (now minus capture stamp) of one frame; true if the frame is late.
    pub fn record(&mut self, lag_ticks: i64) -> bool {
        let bin = usize::try_from(lag_ticks.max(0) / BIN_TICKS)
            .unwrap_or(BINS - 1)
            .min(BINS - 1);
        self.hist[bin] += 1;
        self.n += 1;
        // Refresh the median every sample at first, then every 30 samples.
        if self.n <= 30 || self.n.is_multiple_of(30) {
            self.baseline = self.median();
        }
        lag_ticks - self.baseline > self.interval
    }

    fn median(&self) -> i64 {
        let half = self.n.div_ceil(2);
        let mut acc = 0u64;
        for (i, c) in self.hist.iter().enumerate() {
            acc += u64::from(*c);
            if acc >= half {
                return i as i64 * BIN_TICKS;
            }
        }
        0
    }

    /// Steady-state lag in ticks.
    pub fn baseline(&self) -> i64 {
        self.baseline
    }
}

/// Late-frame rate over the last 30 s, in one-second buckets.
#[derive(Debug, Clone, Default)]
struct HealthWindow {
    /// (second index, frames observed, frames bad)
    buckets: VecDeque<(i64, u64, u64)>,
}

impl HealthWindow {
    fn add(&mut self, second: i64, observed: u64, bad: u64) {
        match self.buckets.back_mut() {
            Some(b) if b.0 == second => {
                b.1 += observed;
                b.2 += bad;
            }
            _ => self.buckets.push_back((second, observed, bad)),
        }
        while self
            .buckets
            .front()
            .is_some_and(|b| b.0 <= second - HEALTH_WINDOW_SECS)
        {
            self.buckets.pop_front();
        }
    }

    fn late_pct(&self, now_second: i64) -> f64 {
        let (mut obs, mut bad) = (0u64, 0u64);
        for b in &self.buckets {
            if b.0 > now_second - HEALTH_WINDOW_SECS {
                obs += b.1;
                bad += b.2;
            }
        }
        if obs < HEALTH_MIN_OBSERVED {
            0.0
        } else {
            100.0 * bad as f64 / obs as f64
        }
    }
}

/// What the pipeline did with a frame (for latency measurements).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameOutcome {
    /// First frame written to the file.
    FirstWritten,
    Handled,
}

pub struct Pipeline<E: VideoEncoder> {
    enc: E,
    interval: i64,
    timeline: PauseTimeline,
    held: Option<E::Frame>,
    first_slot: Option<i64>,
    /// The last slot written to the file.
    last_slot: Option<i64>,
    /// The last slot dealt with: written, dropped or skipped. Slots never come back, so a slot the
    /// encoder refused stays empty and the file never runs ahead of the recording (a refused
    /// slot used to be retried by the next one, which squeezed 73 s of recording into 33 s).
    cursor: Option<i64>,
    /// The last slot the encoder refused.
    last_dropped: Option<i64>,
    load: LoadMonitor,
    last_delivery: Option<MediaTime>,
    stats: PipelineStats,
    late: LateTracker,
    health: HealthWindow,
    halted: Option<HaltReason>,
    closed: bool,
    summary: Option<WriterSummary>,
    /// The source has delivered at least one idle status sample.
    reports_idle: bool,
    /// The source sends frames only when the picture changes (see `set_event_driven`).
    event_driven: bool,
    /// The last delivery was an idle sample (not a frame).
    last_was_idle: bool,
    /// A stall is being reported; ends with the next delivery.
    stall_open: bool,
    /// Milliseconds of stalls already closed.
    stall_closed_ms: u64,
}

fn ticks_ms(t: i64) -> u64 {
    u64::try_from(t / (TICKS_PER_SECOND / 1000)).unwrap_or(0)
}

fn round_div(a: i64, b: i64) -> i64 {
    (a + b / 2).div_euclid(b)
}

impl<E: VideoEncoder> Pipeline<E> {
    /// The source sends frames only on change: gaps between frames are not missed slots.
    pub fn set_event_driven(&mut self, on: bool) {
        self.event_driven = on;
    }

    pub fn new(enc: E, fps: u32) -> Self {
        let interval = TICKS_PER_SECOND / i64::from(fps.max(1));
        Pipeline {
            enc,
            interval,
            timeline: PauseTimeline::new(),
            held: None,
            first_slot: None,
            last_slot: None,
            cursor: None,
            last_dropped: None,
            load: LoadMonitor::new(fps),
            last_delivery: None,
            stats: PipelineStats::default(),
            late: LateTracker::new(interval),
            health: HealthWindow::default(),
            halted: None,
            closed: false,
            summary: None,
            reports_idle: false,
            event_driven: false,
            last_was_idle: false,
            stall_open: false,
            stall_closed_ms: 0,
        }
    }

    pub fn stats(&self) -> &PipelineStats {
        &self.stats
    }

    pub fn timeline(&self) -> &PauseTimeline {
        &self.timeline
    }

    pub fn interval(&self) -> MediaTime {
        MediaTime::from_ticks(self.interval)
    }

    pub fn halted(&self) -> Option<&HaltReason> {
        self.halted.as_ref()
    }

    pub fn encoder(&self) -> &E {
        &self.enc
    }

    pub fn fragments_written(&self) -> usize {
        self.enc.fragments_written()
    }

    /// Session time of the first frame in the file (after snapping), if any.
    pub fn first_frame_session_time(&self) -> Option<MediaTime> {
        let s = self.first_slot?;
        Some(
            self.timeline
                .unmap(MediaTime::from_ticks(s * self.interval)),
        )
    }

    /// File time covered so far: first slot start to end of the last slot.
    pub fn file_span(&self) -> Option<(MediaTime, MediaTime)> {
        let (a, b) = (self.first_slot?, self.last_slot?);
        Some((
            MediaTime::from_ticks(a * self.interval),
            MediaTime::from_ticks((b + 1) * self.interval),
        ))
    }

    /// Late percentage over the whole recording: late events over the slots from the first
    /// written one to the last dealt with, never above 100.
    pub fn late_pct(&self) -> f64 {
        let slots = match (self.first_slot, self.cursor) {
            (Some(a), Some(b)) => (b - a + 1).max(1) as u64,
            _ => return 0.0,
        };
        100.0 * self.stats.late().min(slots) as f64 / slots as f64
    }

    /// The frame rate offered to the encoder now (the asked rate unless the encoder could not
    /// keep up).
    pub fn effective_fps(&self) -> f64 {
        self.load.effective_fps()
    }

    /// Checks whether the encoder keeps up; call from the heartbeat with the session time. On an
    /// overload the offered rate is lowered and the decision returned (once per step).
    pub fn check_load(&mut self, now: MediaTime) -> Option<RateChange> {
        let c = self.load.evaluate(Self::second_of(now))?;
        self.stats.rate_changes += 1;
        Some(c)
    }

    /// Whether a slot is offered to the encoder at the current rate.
    fn due(&self, slot: i64) -> bool {
        slot.rem_euclid(i64::from(self.load.stride())) == 0
    }

    /// Late percentage over the last 30 s of session time (REC-3 health monitor).
    pub fn recent_late_pct(&self, now: MediaTime) -> f64 {
        self.health
            .late_pct(now.ticks().div_euclid(TICKS_PER_SECOND))
    }

    /// Stops accepting frames; the file stays valid and `finish` still works.
    pub fn halt(&mut self, reason: HaltReason) {
        if self.halted.is_none() {
            self.halted = Some(reason);
        }
    }

    fn accepting(&self) -> bool {
        self.halted.is_none() && !self.closed
    }

    fn second_of(t: MediaTime) -> i64 {
        t.ticks().div_euclid(TICKS_PER_SECOND)
    }

    /// Notes a delivery (frame or idle status sample) and counts missed slots.
    fn note_delivery(&mut self, pts: MediaTime, idle: bool) {
        if let Some(prev) = self.last_delivery {
            let gap = pts.ticks() - prev.ticks();
            if self.stall_open {
                // The silence ends: its whole length is the stall.
                let total = self.stall_closed_ms + ticks_ms(gap);
                self.stats.stall_ms = total;
                self.stall_closed_ms = total;
                self.stall_open = false;
            }
            // Only a frame after a frame can show a stall: an idle sample in between means the
            // source was alive and the screen unchanged.
            let counts = !idle
                && !self.event_driven
                && !self.last_was_idle
                && gap * 2 > self.interval * 3
                && (gap < STATIC_GAP_TICKS || self.reports_idle);
            if counts {
                let missed = u64::try_from(round_div(gap, self.interval) - 1).unwrap_or(0);
                self.stats.missed_slots += missed;
                self.health.add(Self::second_of(pts), missed, missed);
            }
        }
        self.last_delivery = Some(pts);
        self.last_was_idle = idle;
    }

    /// A status-only sample: the screen did not change.
    pub fn on_idle(&mut self, session_pts: MediaTime) {
        self.stats.idle += 1;
        self.reports_idle = true;
        if self.accepting() && session_pts.ticks() >= 0 && self.timeline.map(session_pts).is_some()
        {
            self.note_delivery(session_pts, true);
        }
    }

    /// The delivery watchdog: call with the current session time (the heartbeat does). Returns
    /// the silence in ticks the first time it passes [`STALL_TICKS`] (once per stall). Only for
    /// a source that reports idle samples, while recording (not paused).
    pub fn watchdog(&mut self, now: MediaTime) -> Option<MediaTime> {
        if !self.accepting() || !self.reports_idle || self.timeline.map(now).is_none() {
            return None;
        }
        let last = self.last_delivery?;
        let silence = now.ticks() - last.ticks();
        if silence <= STALL_TICKS {
            return None;
        }
        self.stats.stall_ms = self.stall_closed_ms + ticks_ms(silence);
        if self.stall_open {
            return None;
        }
        self.stall_open = true;
        self.stats.stalls += 1;
        Some(MediaTime::from_ticks(silence))
    }

    /// One frame stamped `session_pts` (session clock), delivered when the session clock read
    /// `now`.
    pub fn on_frame(
        &mut self,
        session_pts: MediaTime,
        now: MediaTime,
        frame: E::Frame,
    ) -> FrameOutcome {
        self.stats.delivered += 1;
        if !self.accepting() {
            return FrameOutcome::Handled;
        }
        if session_pts.ticks() < 0 {
            self.stats.before_origin += 1;
            return FrameOutcome::Handled;
        }
        let Some(file_t) = self.timeline.map(session_pts) else {
            self.stats.paused_drops += 1;
            return FrameOutcome::Handled;
        };
        self.note_delivery(session_pts, false);
        let was_late = self.late.record(now.ticks() - session_pts.ticks());
        if was_late {
            self.stats.lag_late += 1;
        }
        self.health
            .add(Self::second_of(session_pts), 1, u64::from(was_late));

        let slot = round_div(file_t.ticks(), self.interval);
        let first = self.first_slot.is_none();
        if let Some(c) = self.cursor {
            if slot <= c {
                self.stats.coalesced += 1;
                self.held = Some(frame);
                return FrameOutcome::Handled;
            }
            // Slots the OS did not fill: repeat the previous frame.
            self.fill_to(slot - 1);
            if !self.accepting() {
                return FrameOutcome::Handled;
            }
        }
        let ok = self.offer(&frame, slot, false);
        self.held = Some(frame);
        if ok && first {
            self.first_slot = Some(slot);
            FrameOutcome::FirstWritten
        } else {
            FrameOutcome::Handled
        }
    }

    /// Deals with `slot` once: skips it when the rate was lowered, else offers it to the encoder.
    /// Returns true when written.
    fn offer(&mut self, frame: &E::Frame, slot: i64, repeat: bool) -> bool {
        self.cursor = Some(self.cursor.map_or(slot, |c| c.max(slot)));
        if !self.due(slot) {
            self.stats.rate_skipped += 1;
            return false;
        }
        self.append(frame, slot, repeat)
    }

    /// Appends one sample at `slot`. Returns true when written.
    fn append(&mut self, frame: &E::Frame, slot: i64, repeat: bool) -> bool {
        let pts = MediaTime::from_ticks(slot * self.interval);
        match self
            .enc
            .append(frame, pts, MediaTime::from_ticks(self.interval))
        {
            Ok(AppendOutcome::Written) => {
                self.stats.written += 1;
                if repeat {
                    self.stats.repeated += 1;
                }
                self.last_slot = Some(slot);
                if self.first_slot.is_none() {
                    self.first_slot = Some(slot);
                }
                self.load
                    .record(Self::second_of(self.timeline.unmap(pts)), false);
                true
            }
            Ok(AppendOutcome::NotReady) => {
                self.stats.not_ready += 1;
                self.last_dropped = Some(slot);
                let second = Self::second_of(self.timeline.unmap(pts));
                self.health.add(second, 1, 1);
                self.load.record(second, true);
                false
            }
            Err(e) => {
                self.stats.append_failed += 1;
                let reason = match e {
                    CmError::DiskFull { .. } => HaltReason::DiskFull,
                    other => HaltReason::EncoderFailed(other.to_string()),
                };
                self.halt(reason);
                false
            }
        }
    }

    /// Writes the held frame at the final `slot`, waiting up to a second for an encoder that is
    /// behind, so the file ends where the recording does.
    fn append_last(&mut self, slot: i64) {
        let Some(held) = self.held.clone() else {
            return;
        };
        self.cursor = Some(self.cursor.map_or(slot, |c| c.max(slot)));
        let pts = MediaTime::from_ticks(slot * self.interval);
        for _ in 0..FINAL_SLOT_TRIES {
            match self
                .enc
                .append(&held, pts, MediaTime::from_ticks(self.interval))
            {
                Ok(AppendOutcome::Written) => {
                    self.stats.written += 1;
                    self.stats.repeated += 1;
                    if self.last_dropped == Some(slot) {
                        // This slot was counted as refused a moment ago: it is written now.
                        self.stats.not_ready = self.stats.not_ready.saturating_sub(1);
                    }
                    self.last_slot = Some(slot);
                    if self.first_slot.is_none() {
                        self.first_slot = Some(slot);
                    }
                    return;
                }
                Ok(AppendOutcome::NotReady) => std::thread::sleep(FINAL_SLOT_WAIT),
                Err(e) => {
                    self.stats.append_failed += 1;
                    let reason = match e {
                        CmError::DiskFull { .. } => HaltReason::DiskFull,
                        other => HaltReason::EncoderFailed(other.to_string()),
                    };
                    self.halt(reason);
                    return;
                }
            }
        }
        self.stats.not_ready += 1;
    }

    /// Repeats the held frame into every empty slot up to and including `upto`.
    fn fill_to(&mut self, upto: i64) {
        let Some(held) = self.held.clone() else {
            return;
        };
        // A refused or skipped slot stays empty and the next one is tried: time never stands still.
        while let Some(next) = self.cursor.map(|c| c + 1) {
            if next > upto || !self.accepting() {
                break;
            }
            self.offer(&held, next, true);
        }
    }

    /// Keeps constant frame rate through a static screen. Call about every half frame interval
    /// with the current session time. Only slots older than a safety margin are filled, so a real
    /// frame still in flight is never overtaken.
    pub fn heartbeat(&mut self, now: MediaTime) {
        if !self.accepting() || self.held.is_none() {
            return;
        }
        let Some(file_now) = self.timeline.map(now) else {
            return;
        };
        let margin = (3 * self.interval).max(TICKS_PER_SECOND / 16);
        let safe = (file_now.ticks() - margin).div_euclid(self.interval);
        if self.cursor.is_some_and(|c| safe > c) {
            self.fill_to(safe);
        }
    }

    /// File time -> last slot that ends at or before it (exclusive end semantics).
    fn last_slot_before(&self, file_end: MediaTime) -> i64 {
        round_div(file_end.ticks(), self.interval) - 1
    }

    pub fn pause(&mut self, at: MediaTime) -> Result<(), CmError> {
        if self.accepting() {
            if let Some(file_at) = self.timeline.map(at) {
                let upto = self.last_slot_before(file_at);
                self.fill_to(upto);
            }
        }
        self.timeline.pause(at)?;
        self.last_delivery = None;
        self.last_was_idle = false;
        if self.stall_open {
            // A stall that was open when the user paused ends there (the pause is not a stall).
            self.stall_closed_ms = self.stats.stall_ms;
            self.stall_open = false;
        }
        Ok(())
    }

    pub fn resume(&mut self, at: MediaTime) -> Result<(), CmError> {
        self.timeline.resume(at)?;
        self.last_delivery = None;
        self.last_was_idle = false;
        Ok(())
    }

    /// Pause gaps recorded so far (closed ones).
    pub fn gaps(&self) -> Vec<PauseGap> {
        self.timeline.gaps().to_vec()
    }

    /// Fills to the stop time and finishes the file. Idempotent.
    pub fn finish(&mut self, at: MediaTime) -> Result<WriterSummary, CmError> {
        if let Some(s) = &self.summary {
            return Ok(s.clone());
        }
        if self.accepting() && self.cursor.is_some() {
            let end = self.timeline.end_file_time(at);
            let upto = self.last_slot_before(end);
            if self.cursor.is_some_and(|c| upto > c) {
                self.fill_to(upto - 1);
            }
            // The last slot gives the file its end (a lowered rate or a refusal may have left it
            // empty): wait briefly for a busy encoder.
            if self.last_slot.is_none_or(|l| l < upto) {
                self.append_last(upto);
            }
        }
        self.closed = true;
        let s = self.enc.finish()?;
        self.summary = Some(s.clone());
        Ok(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cm_encode::synthetic::{Appended, SyntheticEncoder, SyntheticFrame};

    #[test]
    fn a_few_frames_in_the_window_are_not_a_late_rate() {
        // A mostly static screen sends about one frame a second: 2 late of 16 is not "12.5 % late".
        let mut h = HealthWindow::default();
        for s in 0..16 {
            h.add(s, 1, u64::from(s < 2));
        }
        assert_eq!(h.late_pct(16), 0.0);
        // The same ratio over 160 frames is a rate.
        let mut h = HealthWindow::default();
        for s in 0..16 {
            h.add(s, 10, u64::from(s < 2) * 10);
        }
        assert!(h.late_pct(16) > 12.0);
    }
    use cm_encode::{plan_video, EncoderConfig, VideoSettings};

    fn pipeline(
        fps: u32,
        dir: &std::path::Path,
    ) -> (Pipeline<SyntheticEncoder>, cm_encode::synthetic::AppendLog) {
        let plan = plan_video(
            1920,
            1080,
            &VideoSettings {
                fps,
                ..Default::default()
            },
        )
        .unwrap();
        let enc = SyntheticEncoder::create(&EncoderConfig {
            path: dir.join("screen.mp4"),
            plan,
        })
        .unwrap();
        let log = enc.log();
        (Pipeline::new(enc, fps), log)
    }

    fn at(ms: i64) -> MediaTime {
        MediaTime::from_millis(ms)
    }

    fn frame(p: &mut Pipeline<SyntheticEncoder>, ms: i64, id: u64) -> FrameOutcome {
        // Delivered 10 ms after the capture stamp.
        p.on_frame(at(ms), at(ms + 10), SyntheticFrame(id))
    }

    fn pts_of(log: &cm_encode::synthetic::AppendLog) -> Vec<i64> {
        log.lock().unwrap().iter().map(|a| a.pts.ticks()).collect()
    }

    #[test]
    fn rec3_ac3_steady_60fps_gives_exactly_fps_times_seconds_frames() {
        let dir = tempfile::tempdir().unwrap();
        let (mut p, log) = pipeline(60, dir.path());
        // OS delivers at ~66 Hz (SCK interval 1.1 x fps) with jitter for 3 s.
        let mut t = 0.0f64;
        let mut id = 0;
        while t < 3000.0 {
            frame(&mut p, t.round() as i64, id);
            id += 1;
            t += 1000.0 / 66.0;
        }
        p.finish(at(3000)).unwrap();
        let n = log.lock().unwrap().len();
        assert!((179..=181).contains(&n), "{n} frames for 3 s at 60 fps");
        let pts = pts_of(&log);
        assert!(
            pts.windows(2).all(|w| w[1] - w[0] == 2000),
            "constant frame rate"
        );
        assert!(p.stats().coalesced > 0, "66 Hz delivery coalesces");
        assert_eq!(p.stats().late(), 0);
    }

    #[test]
    fn static_screen_repeats_the_last_frame() {
        let dir = tempfile::tempdir().unwrap();
        let (mut p, log) = pipeline(30, dir.path());
        frame(&mut p, 0, 1);
        // Nothing for 1 s, then a new frame.
        frame(&mut p, 1000, 2);
        p.finish(at(1033)).unwrap();
        let l = log.lock().unwrap();
        assert_eq!(l.len(), 31, "slots 0..=30");
        assert!(l[1..30].iter().all(|a| a.frame == SyntheticFrame(1)));
        assert_eq!(l[30].frame, SyntheticFrame(2));
        assert_eq!(p.stats().repeated, 29);
        // A 1 s silence is a static screen, not 29 missed slots.
        assert_eq!(p.stats().missed_slots, 0);
    }

    #[test]
    fn heartbeat_keeps_slots_flowing_but_never_overtakes_a_frame_in_flight() {
        let dir = tempfile::tempdir().unwrap();
        let (mut p, log) = pipeline(30, dir.path());
        frame(&mut p, 0, 1);
        p.heartbeat(at(50)); // within the margin: nothing
        assert_eq!(log.lock().unwrap().len(), 1);
        p.heartbeat(at(1000));
        let n = log.lock().unwrap().len();
        // Filled up to now minus the 100 ms margin: slots 0..=26 (900 ms / 33.3 ms).
        assert!((26..=28).contains(&n), "{n}");
        // The next real frame continues the grid without a duplicate or a gap.
        frame(&mut p, 1010, 2);
        let pts = pts_of(&log);
        assert!(pts.windows(2).all(|w| w[1] - w[0] == 4000));
    }

    /// REC-4.AC1: 10 s recorded, 20 s paused, 10 s recorded = 20 s file (+- 1 frame).
    #[test]
    fn rec4_ac1_pause_gap_gives_a_continuous_file() {
        let dir = tempfile::tempdir().unwrap();
        let (mut p, log) = pipeline(30, dir.path());
        for i in 0..300 {
            frame(&mut p, i * 1000 / 30, i as u64);
        }
        p.pause(at(10_000)).unwrap();
        // Frames during the pause (the stream keeps running) are dropped.
        for i in 0..100 {
            frame(&mut p, 10_000 + i * 200, 1000 + i as u64);
        }
        p.resume(at(30_000)).unwrap();
        for i in 0..300 {
            frame(&mut p, 30_000 + i * 1000 / 30, 2000 + i as u64);
        }
        p.finish(at(40_000)).unwrap();
        let l = log.lock().unwrap();
        let first = l.first().unwrap().pts.ticks();
        let last = l.last().unwrap();
        let file_len = last.pts.ticks() + last.duration.ticks() - first;
        assert!(
            (file_len - 20 * 120_000).abs() <= 4000,
            "file is {} s",
            file_len as f64 / 120_000.0
        );
        // No hole at the join: pts is constant-rate straight through the gap.
        assert!(l
            .windows(2)
            .all(|w| w[1].pts.ticks() - w[0].pts.ticks() == 4000));
        // No frame captured during the pause was written.
        assert!(l.iter().all(|a| !(1000..2000).contains(&a.frame.0)));
        assert_eq!(p.stats().paused_drops, 100);
        assert_eq!(
            p.gaps(),
            vec![PauseGap {
                from: at(10_000),
                to: at(30_000)
            }]
        );
    }

    #[test]
    fn stop_while_paused_ends_at_the_pause_point() {
        let dir = tempfile::tempdir().unwrap();
        let (mut p, log) = pipeline(30, dir.path());
        for i in 0..30 {
            frame(&mut p, i * 1000 / 30, i as u64);
        }
        p.pause(at(1000)).unwrap();
        p.finish(at(9000)).unwrap();
        let n = log.lock().unwrap().len();
        assert!((29..=31).contains(&n), "{n}");
    }

    #[test]
    fn frames_before_the_origin_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let (mut p, log) = pipeline(30, dir.path());
        p.on_frame(at(-50), at(0), SyntheticFrame(1));
        frame(&mut p, 5, 2);
        p.finish(at(100)).unwrap();
        assert_eq!(p.stats().before_origin, 1);
        assert_eq!(log.lock().unwrap()[0].frame, SyntheticFrame(2));
    }

    #[test]
    fn encoder_backpressure_counts_as_dropped_and_late_and_is_filled() {
        let dir = tempfile::tempdir().unwrap();
        let plan = plan_video(1920, 1080, &VideoSettings::default()).unwrap();
        let mut enc = SyntheticEncoder::create(&EncoderConfig {
            path: dir.path().join("screen.mp4"),
            plan,
        })
        .unwrap();
        enc.not_ready_every = 5;
        let log = enc.log();
        let mut p = Pipeline::new(enc, 30);
        for i in 0..60 {
            frame(&mut p, i * 1000 / 30, i as u64);
        }
        p.finish(at(2000)).unwrap();
        // Every 5th append is refused, repeats included, so at least the 12 real ones.
        assert!(p.stats().not_ready >= 10);
        assert_eq!(p.stats().dropped(), p.stats().not_ready);
        assert!(p.stats().late() >= 10);
        // A refused slot stays empty (the previous frame lasts through it): every sample sits on
        // the slot grid at its own time, so the file keeps the length of the recording.
        let l: Vec<Appended> = log.lock().unwrap().clone();
        assert!(l.windows(2).all(|w| w[1].pts > w[0].pts));
        assert!(l.iter().all(|a| a.pts.ticks() % 4000 == 0));
        let last = l.last().unwrap().pts.ticks();
        assert!(last >= 2 * 120_000 - 3 * 4000, "file ends at {last}");
        assert_eq!(
            p.stats().slots_offered(),
            p.stats().written + p.stats().dropped()
        );
    }

    /// An encoder that accepts a number of slots per second (a budget the test refills), like a
    /// hardware encoder that cannot do 4K30.
    struct Limited {
        inner: SyntheticEncoder,
        budget: std::sync::Arc<std::sync::Mutex<f64>>,
    }

    impl VideoEncoder for Limited {
        type Frame = SyntheticFrame;
        fn append(
            &mut self,
            f: &SyntheticFrame,
            pts: MediaTime,
            d: MediaTime,
        ) -> Result<AppendOutcome, CmError> {
            let mut b = self.budget.lock().unwrap();
            if *b < 1.0 {
                return Ok(AppendOutcome::NotReady);
            }
            *b -= 1.0;
            drop(b);
            self.inner.append(f, pts, d)
        }
        fn check(&self) -> Result<(), CmError> {
            self.inner.check()
        }
        fn fragments_written(&self) -> usize {
            self.inner.fragments_written()
        }
        fn finish(&mut self) -> Result<WriterSummary, CmError> {
            self.inner.finish()
        }
    }

    /// Drives 30 fps delivery, the heartbeat and the load check over `secs` seconds against an
    /// encoder that takes `capacity` slots a second.
    fn overloaded(
        capacity: f64,
        secs: i64,
        dir: &std::path::Path,
    ) -> (
        Pipeline<Limited>,
        cm_encode::synthetic::AppendLog,
        Vec<RateChange>,
    ) {
        let plan = plan_video(3840, 2160, &VideoSettings::default()).unwrap();
        let inner = SyntheticEncoder::create(&EncoderConfig {
            path: dir.join("screen.mp4"),
            plan,
        })
        .unwrap();
        let log = inner.log();
        let budget = std::sync::Arc::new(std::sync::Mutex::new(2.0));
        let mut p = Pipeline::new(
            Limited {
                inner,
                budget: budget.clone(),
            },
            30,
        );
        let mut changes = Vec::new();
        let mut next_frame = 0.0f64;
        let mut id = 0;
        for ms in 0..secs * 1000 {
            {
                let mut b = budget.lock().unwrap();
                *b = (*b + capacity / 1000.0).min(4.0); // a short queue
            }
            if ms as f64 >= next_frame {
                p.on_frame(at(ms), at(ms + 5), SyntheticFrame(id));
                id += 1;
                next_frame += 1000.0 / 30.0;
            }
            if ms % 16 == 0 {
                p.heartbeat(at(ms));
            }
            if ms % 100 == 0 {
                changes.extend(p.check_load(at(ms)));
            }
        }
        // The encoder has caught up a little by the stop.
        *budget.lock().unwrap() = 6.0;
        p.finish(at(secs * 1000)).unwrap();
        (p, log, changes)
    }

    /// Finding F4 (Windows lab round 3): a 4K30 recording of 73.5 s on an encoder that managed
    /// 13 fps produced 981 frames = 32.7 s. A refused slot was retried by the next one, so the file
    /// ran at the encoder's rate on a squeezed timeline. Dropped slots must stay empty.
    #[test]
    fn f4_an_overloaded_encoder_never_shortens_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let (p, log, _) = overloaded(13.0, 40, dir.path());
        let l = log.lock().unwrap();
        let last = l.last().unwrap();
        let end = last.pts.ticks() + last.duration.ticks();
        assert!(
            (end - 40 * 120_000).abs() <= 4000,
            "the file is {:.2} s for a 40 s recording",
            end as f64 / 120_000.0
        );
        assert!(l.windows(2).all(|w| w[1].pts > w[0].pts));
        assert!(
            l.iter().all(|a| a.pts.ticks() % 4000 == 0),
            "on the slot grid"
        );
        let _ = p;
    }

    #[test]
    fn f4_the_rate_is_lowered_early_and_the_counters_agree() {
        let dir = tempfile::tempdir().unwrap();
        let (p, log, changes) = overloaded(13.0, 40, dir.path());
        let first = changes.first().expect("a rate change");
        assert!(first.attempted >= 20 && first.dropped > 0);
        assert_eq!(first.asked_fps, 30);
        assert!(p.effective_fps() < 30.0 && p.effective_fps() >= 5.0);
        let s = p.stats();
        // A slot is written or dropped, never both; counted once.
        assert_eq!(s.written, log.lock().unwrap().len() as u64);
        assert_eq!(s.slots_offered(), s.written + s.dropped());
        // After the step down the encoder keeps up: refusals stop growing.
        assert!(s.dropped() < 150, "{s:?}");
        assert!(s.rate_skipped > 0 && s.rate_changes >= 1);
        // A rate, never above 100 %, and far lower than the 920 % the lab saw.
        assert!(p.late_pct() <= 100.0, "{}", p.late_pct());
        assert!(s.late() >= s.dropped());
    }

    #[test]
    fn f4_a_healthy_encoder_keeps_the_asked_rate_and_drops_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let (p, log, changes) = overloaded(60.0, 20, dir.path());
        assert!(changes.is_empty());
        assert_eq!(p.effective_fps(), 30.0);
        assert_eq!(p.stats().dropped(), 0);
        assert_eq!(p.stats().rate_skipped, 0);
        assert!((599..=601).contains(&log.lock().unwrap().len()));
    }

    #[test]
    fn late_percentage_cannot_exceed_one_hundred() {
        let dir = tempfile::tempdir().unwrap();
        let (mut p, _log) = pipeline(30, dir.path());
        frame(&mut p, 0, 1);
        frame(&mut p, 33, 2);
        p.stats.lag_late = 500;
        p.stats.not_ready = 9000;
        assert_eq!(p.late_pct(), 100.0);
    }

    #[test]
    fn a_stall_is_counted_as_missed_slots_and_lag_late() {
        let dir = tempfile::tempdir().unwrap();
        let (mut p, _log) = pipeline(60, dir.path());
        for i in 0..120 {
            frame(&mut p, i * 1000 / 60, i as u64);
        }
        // 100 ms hiccup (6 slots), delivered late.
        let t = 2000 + 100;
        p.on_frame(at(t), at(t + 110), SyntheticFrame(500));
        for i in 1..60 {
            frame(&mut p, t + i * 1000 / 60, 500 + i as u64);
        }
        assert!(p.stats().missed_slots >= 5, "{:?}", p.stats());
        assert!(p.stats().lag_late >= 1);
        assert!(p.late_pct() > 0.0);
        assert!(p.recent_late_pct(at(3000)) > 2.0);
    }

    /// A source that sends frames only on change (Windows.Graphics.Capture): a picture that changes 16 times
    /// a second leaves gaps that are not missed delivery slots, and the slots are still filled by repeats.
    #[test]
    fn an_event_driven_source_has_no_missed_slots_for_a_slowly_changing_picture() {
        let dir = tempfile::tempdir().unwrap();
        let (mut p, _log) = pipeline(30, dir.path());
        p.set_event_driven(true);
        for i in 0..160 {
            frame(&mut p, i * 62, i as u64); // a frame every 62 ms (about 16 fps)
        }
        assert_eq!(p.stats().missed_slots, 0, "{:?}", p.stats());
        assert!(p.stats().repeated > 0, "{:?}", p.stats());
        assert!(p.late_pct() < 2.0, "{}", p.late_pct());
    }

    /// R3-7: once the source reports idle samples, a long silence with none in it is a stall:
    /// its slots count as missed (late), the watchdog reports it once, and its length is kept.
    #[test]
    fn r3_7_a_long_stall_counts_when_the_source_reports_idle() {
        let dir = tempfile::tempdir().unwrap();
        let (mut p, _log) = pipeline(60, dir.path());
        for i in 0..60 {
            frame(&mut p, i * 1000 / 60, i as u64);
        }
        p.on_idle(at(1010)); // the source says "unchanged": it reports idle
                             // 2 s with nothing at all (display sleep, a stuck SCK) ...
        assert_eq!(p.watchdog(at(1500)), None, "under a second is not a stall");
        let silence = p.watchdog(at(2300)).expect("stall");
        assert!(silence.ticks() >= STALL_TICKS);
        assert_eq!(p.watchdog(at(2900)), None, "reported once");
        assert_eq!(p.stats().stalls, 1);
        // ... then frames resume at 3.1 s.
        for i in 0..30 {
            frame(&mut p, 3100 + i * 1000 / 60, 100 + i as u64);
        }
        // idle -> frame is not a counted gap (the source was alive), the stall is in stall_ms.
        assert!(
            (2000..2300).contains(&p.stats().stall_ms),
            "{}",
            p.stats().stall_ms
        );
        // A frame-to-frame gap of 400 ms with no idle in it IS a stall for this source.
        frame(&mut p, 3100 + 29 * 1000 / 60 + 400, 999);
        assert!(p.stats().missed_slots >= 20, "{:?}", p.stats());
        assert!(p.recent_late_pct(at(3700)) > 2.0);
    }

    /// R3-7: a source that never reported idle keeps the old rule (a 1 s silence is a static
    /// screen); and a static screen WITH idle samples is never a stall.
    #[test]
    fn r3_7_static_screens_are_not_stalls() {
        let dir = tempfile::tempdir().unwrap();
        let (mut p, _log) = pipeline(30, dir.path());
        frame(&mut p, 0, 1);
        assert_eq!(p.watchdog(at(5000)), None, "no idle seen: cannot tell");
        frame(&mut p, 5000, 2);
        assert_eq!((p.stats().missed_slots, p.stats().stalls), (0, 0));
        // The same, from a source that reports idle: idle samples at 4 Hz keep it alive.
        let dir2 = tempfile::tempdir().unwrap();
        let (mut q, _l) = pipeline(30, dir2.path());
        frame(&mut q, 0, 1);
        for i in 1..=20 {
            q.on_idle(at(i * 250));
            assert_eq!(q.watchdog(at(i * 250 + 100)), None);
        }
        frame(&mut q, 5200, 2);
        assert_eq!((q.stats().missed_slots, q.stats().stalls), (0, 0));
    }

    #[test]
    fn late_tracker_baseline_is_the_median_lag() {
        let mut t = LateTracker::new(2000);
        for _ in 0..100 {
            assert!(!t.record(1500)); // ~12.5 ms steady lag
        }
        assert!((t.baseline() - 1500).abs() <= BIN_TICKS);
        assert!(t.record(1500 + 2500));
        assert!(!t.record(1500 + 1500));
    }

    #[test]
    fn disk_error_halts_with_disk_full() {
        // A failing encoder: use the synthetic one with an unwritable path replaced by halt().
        let dir = tempfile::tempdir().unwrap();
        let (mut p, _log) = pipeline(30, dir.path());
        frame(&mut p, 0, 1);
        p.halt(HaltReason::DiskFull);
        frame(&mut p, 100, 2);
        assert_eq!(p.stats().written, 1);
        assert_eq!(p.stats().delivered, 2);
        assert_eq!(p.halted(), Some(&HaltReason::DiskFull));
        // The file is still finished cleanly.
        assert!(p.finish(at(200)).is_ok());
    }
}
