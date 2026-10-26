//! Settings -> Webcam & audio, board R13b: the rules of the Audio check card. Everything here is
//! pure (no window, no device): the 32-segment microphone meter with its good-range band, the
//! peak hold, the verdict from the peaks of the last 2 s, Clipping and Silent, the states of the
//! microphone sub-section, and the system audio check (a 16-segment meter, a status, and a small
//! trait the real chime check plugs into). `audio_check.rs` draws it and runs the devices.
//!
//! All levels are dBFS. Every threshold is a constant here so they are easy to tune on hardware.

use std::collections::VecDeque;
use std::sync::Arc;

use super::webcam_model::{CheckSession, Facts};

// ---- tuning ----------------------------------------------------------------------------------

/// Segments of the microphone meter, and of the system audio meter.
pub const SEGMENTS: usize = 32;
pub const SYSTEM_SEGMENTS: usize = 16;
/// The meter spans this level (empty) to 0 dBFS (full).
pub const METER_FLOOR_DB: f32 = -60.0;
/// The good range of the microphone: below it is too quiet, above it too loud.
pub const GOOD_LOW_DB: f32 = -30.0;
pub const GOOD_HIGH_DB: f32 = -8.0;
/// Peaks below this for [`SILENT_MS`] mean no sound comes in.
pub const SILENT_DB: f32 = -60.0;
pub const SILENT_MS: u64 = 2000;
/// The verdict looks at the peaks of this long, once at least [`VERDICT_WARMUP_MS`] of them exist.
pub const VERDICT_WINDOW_MS: u64 = 2000;
pub const VERDICT_WARMUP_MS: u64 = 1000;
/// A clipping event keeps the Clipping note this long.
pub const CLIP_HOLD_MS: u64 = 3000;
/// The peak marker stays this long, then falls at [`PEAK_DECAY_DB_PER_S`].
pub const PEAK_HOLD_MS: u64 = 1000;
pub const PEAK_DECAY_DB_PER_S: f32 = 30.0;
/// The bar itself rises at once and falls at this speed.
pub const BAR_RELEASE_DB_PER_S: f32 = 60.0;

// ---- segments --------------------------------------------------------------------------------

/// How one segment is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Seg {
    Unlit,
    /// Lit below the good range.
    Quiet,
    /// Lit inside the good range.
    Good,
    /// Lit above the good range.
    Loud,
}

/// Where a level sits on the meter, 0.0 (floor) to 1.0 (0 dBFS).
pub fn fraction(db: f32) -> f32 {
    if !db.is_finite() {
        return 0.0;
    }
    ((db - METER_FLOOR_DB) / -METER_FLOOR_DB).clamp(0.0, 1.0)
}

/// How many of `n` segments are lit for `db`.
pub fn lit_segments(db: f32, n: usize) -> usize {
    (fraction(db) * n as f32).round() as usize
}

/// The class of segment `i` of `n` by where it sits against the good range.
fn class_of(i: usize, n: usize) -> Seg {
    let db = METER_FLOOR_DB + (i as f32 + 0.5) / n as f32 * -METER_FLOOR_DB;
    if db < GOOD_LOW_DB {
        Seg::Quiet
    } else if db <= GOOD_HIGH_DB {
        Seg::Good
    } else {
        Seg::Loud
    }
}

/// The `n` segments for a bar at `level_db` and a peak marker at `peak_db`.
pub fn segments(n: usize, level_db: f32, peak_db: Option<f32>) -> Vec<Seg> {
    let lit = lit_segments(level_db, n);
    let peak = peak_db
        .filter(|p| fraction(*p) > 0.0)
        .map(|p| lit_segments(p, n).clamp(1, n) - 1);
    (0..n)
        .map(|i| {
            if i < lit || peak == Some(i) {
                class_of(i, n)
            } else {
                Seg::Unlit
            }
        })
        .collect()
}

/// The good-range band over the meter: left edge and width as fractions of its width.
pub fn band() -> (f32, f32) {
    let (a, b) = (fraction(GOOD_LOW_DB), fraction(GOOD_HIGH_DB));
    (a, b - a)
}

/// "-14 dB" with a real minus sign; below the meter's floor "-inf dB".
pub fn format_db(db: f32) -> String {
    if !db.is_finite() || db <= METER_FLOOR_DB {
        return "\u{2212}\u{221E} dB".to_owned();
    }
    let n = db.round() as i32;
    if n >= 0 {
        "0 dB".to_owned()
    } else {
        format!("\u{2212}{} dB", -n)
    }
}

// ---- peak hold -------------------------------------------------------------------------------

/// A marker that follows peaks up at once, stays [`PEAK_HOLD_MS`], then falls.
#[derive(Debug, Clone, Default)]
pub struct PeakHold {
    value: Option<f32>,
    since_ms: u64,
    last_ms: u64,
}

impl PeakHold {
    pub fn update(&mut self, level_db: f32, now_ms: u64) -> f32 {
        match self.value {
            Some(v) if level_db < v => {
                if now_ms.saturating_sub(self.since_ms) > PEAK_HOLD_MS {
                    let dt = now_ms.saturating_sub(self.last_ms) as f32 / 1000.0;
                    self.value = Some((v - PEAK_DECAY_DB_PER_S * dt).max(level_db));
                }
            }
            _ => {
                self.value = Some(level_db);
                self.since_ms = now_ms;
            }
        }
        self.last_ms = now_ms;
        self.value.unwrap_or(level_db)
    }

    pub fn value(&self) -> Option<f32> {
        self.value
    }
}

// ---- the microphone meter --------------------------------------------------------------------

/// The one-word verdict on the level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    TooQuiet,
    Good,
    TooLoud,
}

/// What the microphone sub-section shows right now.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MicReading {
    /// The bar.
    pub level_db: f32,
    /// The peak marker and the large readout.
    pub peak_db: f32,
    /// `None` until a second of peaks exists.
    pub verdict: Option<Verdict>,
    /// A clipping event happened in the last [`CLIP_HOLD_MS`].
    pub clipping: bool,
    /// Peaks have been below [`SILENT_DB`] for [`SILENT_MS`].
    pub silent: bool,
}

#[derive(Debug, Clone, Default)]
struct PeakWindow {
    samples: VecDeque<(u64, f32)>,
    first_ms: Option<u64>,
}

impl PeakWindow {
    fn push(&mut self, now_ms: u64, peak_db: f32) {
        self.first_ms.get_or_insert(now_ms);
        self.samples.push_back((now_ms, peak_db));
        while self
            .samples
            .front()
            .is_some_and(|(t, _)| now_ms.saturating_sub(*t) > VERDICT_WINDOW_MS.max(SILENT_MS))
        {
            self.samples.pop_front();
        }
    }

    fn history_ms(&self, now_ms: u64) -> u64 {
        self.first_ms.map_or(0, |f| now_ms.saturating_sub(f))
    }

    fn max_within(&self, now_ms: u64, window_ms: u64) -> Option<f32> {
        self.samples
            .iter()
            .filter(|(t, _)| now_ms.saturating_sub(*t) <= window_ms)
            .map(|(_, p)| *p)
            .reduce(f32::max)
    }
}

/// Turns the readings of a running microphone (peak dBFS and the clip counter, about 30 a second)
/// into what the card shows.
#[derive(Debug, Clone, Default)]
pub struct MicMeter {
    bar_db: Option<f32>,
    last_ms: u64,
    hold: PeakHold,
    window: PeakWindow,
    clips_seen: Option<u64>,
    clip_at_ms: Option<u64>,
}

impl MicMeter {
    pub fn push(&mut self, now_ms: u64, peak_db: f32, clip_events: u64) {
        let peak_db = if peak_db.is_finite() {
            peak_db
        } else {
            METER_FLOOR_DB
        };
        self.bar_db = Some(match self.bar_db {
            Some(bar) if peak_db < bar => {
                let dt = now_ms.saturating_sub(self.last_ms) as f32 / 1000.0;
                (bar - BAR_RELEASE_DB_PER_S * dt).max(peak_db)
            }
            _ => peak_db,
        });
        self.last_ms = now_ms;
        self.hold.update(peak_db, now_ms);
        self.window.push(now_ms, peak_db);
        match self.clips_seen {
            // The first reading is the baseline: clips before the check began do not count.
            None => self.clips_seen = Some(clip_events),
            Some(seen) if clip_events > seen => {
                self.clips_seen = Some(clip_events);
                self.clip_at_ms = Some(now_ms);
            }
            Some(_) => {}
        }
    }

    pub fn has_data(&self) -> bool {
        self.bar_db.is_some()
    }

    pub fn reading(&self, now_ms: u64) -> MicReading {
        let history = self.window.history_ms(now_ms);
        let recent = self.window.max_within(now_ms, VERDICT_WINDOW_MS);
        let verdict = match recent {
            Some(p) if history >= VERDICT_WARMUP_MS => Some(if p < GOOD_LOW_DB {
                Verdict::TooQuiet
            } else if p > GOOD_HIGH_DB {
                Verdict::TooLoud
            } else {
                Verdict::Good
            }),
            _ => None,
        };
        let silent = history >= SILENT_MS
            && self
                .window
                .max_within(now_ms, SILENT_MS)
                .is_some_and(|p| p < SILENT_DB);
        MicReading {
            level_db: self.bar_db.unwrap_or(METER_FLOOR_DB),
            peak_db: self.hold.value().unwrap_or(METER_FLOOR_DB),
            verdict,
            clipping: self
                .clip_at_ms
                .is_some_and(|t| now_ms.saturating_sub(t) <= CLIP_HOLD_MS),
            silent,
        }
    }

    pub fn reset(&mut self) {
        *self = MicMeter::default();
    }
}

/// A verdict that comes with a note and one button (the spec's "Too quiet" and "Clipping").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MicTip {
    TooQuiet,
    Clipping,
}

pub fn mic_tip(r: &MicReading) -> Option<MicTip> {
    if r.clipping {
        Some(MicTip::Clipping)
    } else if r.verdict == Some(Verdict::TooQuiet) && !r.silent {
        Some(MicTip::TooQuiet)
    } else {
        None
    }
}

/// What the microphone sub-section shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MicCard {
    /// The default: "Microphone is off", Test microphone.
    Off,
    /// Asked, no reading yet.
    Starting,
    /// The meter, with the verdict chip.
    Running,
    /// A recording holds the microphone: a plain note.
    Paused,
    NoPermission,
    /// Selected but silent: replaces the meter.
    Silent,
    /// The microphone could not be opened.
    Failed,
}

pub fn mic_card(
    session: &CheckSession,
    f: Facts,
    failed: bool,
    meter: &MicMeter,
    now_ms: u64,
) -> MicCard {
    if session.shows_no_permission() {
        return MicCard::NoPermission;
    }
    if failed {
        return MicCard::Failed;
    }
    if session.paused_by_recording(f) {
        return MicCard::Paused;
    }
    if !session.requested() {
        return MicCard::Off;
    }
    if !meter.has_data() {
        MicCard::Starting
    } else if meter.reading(now_ms).silent {
        MicCard::Silent
    } else {
        MicCard::Running
    }
}

// ---- the system audio check ------------------------------------------------------------------

/// How a system audio check ended.
/// Heard, NotHeard and NoPermission are built by the real runner (not wired yet).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SystemOutcome {
    /// The chime came back through system capture.
    Heard,
    /// Capture worked and the chime was not in it.
    NotHeard,
    /// Screen Recording is not granted.
    NoPermission,
    /// The check could not run (the reason, for the log).
    Failed(String),
}

/// Plays the test chime in another process and listens to system capture. The page runs it off the
/// UI thread; `progress` gets the current peak (dBFS) of the captured audio as it goes. It must
/// return after about 2 s, and touch nothing when it returns.
pub trait SystemCheckRunner: Send + Sync {
    fn run(&self, progress: &(dyn Fn(f32) + Send + Sync)) -> SystemOutcome;
}

/// The runner the page uses: the chime played by `afplay` (another process, because this
/// process's own audio is never captured) and heard through the system audio capture.
pub fn system_runner() -> Arc<dyn SystemCheckRunner> {
    Arc::new(ChimeRunner)
}

struct ChimeRunner;

impl SystemCheckRunner for ChimeRunner {
    fn run(&self, progress: &(dyn Fn(f32) + Send + Sync)) -> SystemOutcome {
        let outcome = cm_audio::run_check(
            &cm_audio::AfplayPlayer,
            cm_audio::system_audio_factory(Vec::new()),
            &cm_audio::ChimeConfig::default(),
            &mut |peak| progress(peak),
        );
        match outcome {
            cm_audio::ChimeOutcome::Heard { .. } => SystemOutcome::Heard,
            cm_audio::ChimeOutcome::NotHeard => SystemOutcome::NotHeard,
            cm_audio::ChimeOutcome::NoPermission => SystemOutcome::NoPermission,
            cm_audio::ChimeOutcome::Failed(why) => SystemOutcome::Failed(why),
        }
    }
}

/// The status of the system audio sub-section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SystemStatus {
    /// Nothing happened yet: the meter is empty, no chip.
    Idle,
    /// The chime plays, the meter moves.
    Running,
    /// The green "Heard" chip.
    Heard,
    /// The "Chime not heard" block.
    NotHeard,
    NoPermission,
    Failed(String),
}

/// The system audio sub-section.
#[derive(Debug, Clone)]
pub struct SystemCheck {
    status: SystemStatus,
    level_db: f32,
    max_db: f32,
}

impl Default for SystemCheck {
    fn default() -> Self {
        SystemCheck {
            status: SystemStatus::Idle,
            level_db: METER_FLOOR_DB,
            max_db: METER_FLOOR_DB,
        }
    }
}

impl SystemCheck {
    pub fn status(&self) -> &SystemStatus {
        &self.status
    }

    pub fn is_running(&self) -> bool {
        self.status == SystemStatus::Running
    }

    /// "Play test sound". Returns whether the check should run now. Without the Screen
    /// Recording permission nothing runs and the "No permission" block shows; while a recording
    /// is going, or a check is running, nothing happens.
    pub fn press_play(&mut self, permission: bool, recording: bool) -> bool {
        if recording || self.is_running() {
            return false;
        }
        if !permission {
            *self = SystemCheck {
                status: SystemStatus::NoPermission,
                ..SystemCheck::default()
            };
            return false;
        }
        *self = SystemCheck {
            status: SystemStatus::Running,
            ..SystemCheck::default()
        };
        true
    }

    pub fn progress(&mut self, peak_db: f32) {
        if self.is_running() {
            self.level_db = if peak_db.is_finite() {
                peak_db
            } else {
                METER_FLOOR_DB
            };
            self.max_db = self.max_db.max(self.level_db);
        }
    }

    pub fn finish(&mut self, outcome: SystemOutcome) {
        if !self.is_running() {
            return;
        }
        self.level_db = self.max_db;
        self.status = match outcome {
            SystemOutcome::Heard => SystemStatus::Heard,
            SystemOutcome::NotHeard => SystemStatus::NotHeard,
            SystemOutcome::NoPermission => SystemStatus::NoPermission,
            SystemOutcome::Failed(why) => SystemStatus::Failed(why),
        };
    }

    /// The page was left (or the window went away): back to idle, the result is forgotten.
    pub fn reset(&mut self) {
        *self = SystemCheck::default();
    }

    pub fn segments(&self) -> Vec<Seg> {
        segments(SYSTEM_SEGMENTS, self.level_db, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    const OK: Facts = Facts {
        permission: true,
        on_page: true,
        window_active: true,
        recording: false,
    };

    fn count(segs: &[Seg], s: Seg) -> usize {
        segs.iter().filter(|x| **x == s).count()
    }

    // ---- segments ----

    #[test]
    fn the_level_maps_onto_the_segments() {
        assert_eq!(lit_segments(-60.0, SEGMENTS), 0);
        assert_eq!(lit_segments(-120.0, SEGMENTS), 0);
        assert_eq!(lit_segments(0.0, SEGMENTS), 32);
        assert_eq!(lit_segments(6.0, SEGMENTS), 32, "clamped");
        assert_eq!(lit_segments(-30.0, SEGMENTS), 16);
        assert_eq!(lit_segments(-30.0, SYSTEM_SEGMENTS), 8);
        assert_eq!(lit_segments(f32::NAN, SEGMENTS), 0);
        assert_eq!(lit_segments(f32::NEG_INFINITY, SEGMENTS), 0);
    }

    #[test]
    fn lit_segments_are_coloured_by_the_good_range() {
        let s = segments(SEGMENTS, 0.0, None);
        assert_eq!(s.len(), 32);
        // -60..-30 dB is the first 16 segments, -30..-8 the next 12, the rest is loud.
        assert!(s[..16].iter().all(|x| *x == Seg::Quiet));
        assert!(s[16..28].iter().all(|x| *x == Seg::Good));
        assert!(s[28..].iter().all(|x| *x == Seg::Loud));
        assert_eq!(count(&s, Seg::Unlit), 0);
        let half = segments(SEGMENTS, -30.0, None);
        assert_eq!(count(&half, Seg::Quiet), 16);
        assert_eq!(count(&half, Seg::Unlit), 16);
        assert_eq!(count(&segments(SEGMENTS, -90.0, None), Seg::Unlit), 32);
    }

    #[test]
    fn the_peak_marker_lights_one_segment_beyond_the_bar() {
        let s = segments(SEGMENTS, -45.0, Some(-20.0));
        assert_eq!(s[lit_segments(-20.0, SEGMENTS) - 1], Seg::Good);
        assert_eq!(
            count(&s, Seg::Unlit),
            32 - lit_segments(-45.0, SEGMENTS) - 1
        );
        // A marker at the floor draws nothing.
        assert_eq!(
            count(&segments(SEGMENTS, -90.0, Some(-90.0)), Seg::Unlit),
            32
        );
        // A marker at full scale is the last segment.
        assert_eq!(segments(SEGMENTS, -90.0, Some(0.0))[31], Seg::Loud);
    }

    #[test]
    fn the_band_covers_the_good_range() {
        let (left, width) = band();
        assert!((left - 0.5).abs() < 1e-6);
        assert!((left + width - fraction(GOOD_HIGH_DB)).abs() < 1e-6);
        assert!(width > 0.3 && width < 0.4);
    }

    #[test]
    fn the_readout_uses_a_real_minus_sign() {
        assert_eq!(format_db(-14.2), "\u{2212}14 dB");
        assert_eq!(format_db(-0.2), "0 dB");
        assert_eq!(format_db(-60.0), "\u{2212}\u{221E} dB");
        assert_eq!(format_db(-120.0), "\u{2212}\u{221E} dB");
        assert_eq!(format_db(f32::NAN), "\u{2212}\u{221E} dB");
    }

    // ---- peak hold ----

    #[test]
    fn the_peak_holds_then_decays() {
        let mut h = PeakHold::default();
        assert_eq!(h.update(-20.0, 0), -20.0);
        assert_eq!(h.update(-40.0, 500), -20.0, "held");
        assert_eq!(h.update(-40.0, 1000), -20.0, "still held at the limit");
        // 1500 ms: past the hold; falls 30 dB/s over the 500 ms since the last update.
        let v = h.update(-60.0, 1500);
        assert!((v - -35.0).abs() < 1e-3, "{v}");
        // Never below the live level, and a louder peak takes over at once.
        assert_eq!(h.update(-34.0, 1530).max(-34.0), h.value().unwrap());
        assert_eq!(h.update(-10.0, 1600), -10.0);
    }

    #[test]
    fn the_peak_stops_falling_at_the_live_level() {
        let mut h = PeakHold::default();
        h.update(-20.0, 0);
        h.update(-50.0, 1100);
        let v = h.update(-50.0, 10_000);
        assert_eq!(v, -50.0);
    }

    // ---- the meter ----

    /// Feeds `peak` every 33 ms from `from` to `to` (ms).
    fn feed(m: &mut MicMeter, from: u64, to: u64, peak: f32, clips: u64) {
        let mut t = from;
        while t <= to {
            m.push(t, peak, clips);
            t += 33;
        }
    }

    #[test]
    fn no_verdict_before_a_second_of_peaks() {
        let mut m = MicMeter::default();
        feed(&mut m, 0, 900, -20.0, 0);
        assert_eq!(m.reading(900).verdict, None);
        feed(&mut m, 933, 1100, -20.0, 0);
        assert_eq!(m.reading(1100).verdict, Some(Verdict::Good));
    }

    #[test]
    fn verdict_thresholds() {
        for (peak, want) in [
            (-45.0, Verdict::TooQuiet),
            (-30.5, Verdict::TooQuiet),
            (-30.0, Verdict::Good),
            (-20.0, Verdict::Good),
            (-8.0, Verdict::Good),
            (-7.5, Verdict::TooLoud),
            (-1.0, Verdict::TooLoud),
        ] {
            let mut m = MicMeter::default();
            feed(&mut m, 0, 1500, peak, 0);
            assert_eq!(m.reading(1500).verdict, Some(want), "{peak}");
        }
    }

    /// The verdict follows the highest peak of the last 2 s: one loud word is enough, and it
    /// ages out.
    #[test]
    fn the_verdict_looks_at_the_last_two_seconds() {
        let mut m = MicMeter::default();
        feed(&mut m, 0, 1000, -45.0, 0);
        m.push(1010, -18.0, 0);
        feed(&mut m, 1043, 1500, -45.0, 0);
        assert_eq!(m.reading(1500).verdict, Some(Verdict::Good));
        feed(&mut m, 1533, 3100, -45.0, 0);
        assert_eq!(m.reading(3100).verdict, Some(Verdict::TooQuiet));
    }

    #[test]
    fn silence_for_two_seconds_is_silent() {
        let mut m = MicMeter::default();
        feed(&mut m, 0, 1900, -90.0, 0);
        assert!(!m.reading(1900).silent, "not yet");
        feed(&mut m, 1933, 2100, -90.0, 0);
        assert!(m.reading(2100).silent);
        // A sound brings it back at once.
        m.push(2133, -40.0, 0);
        assert!(!m.reading(2133).silent);
        // Quiet room noise above -60 is not silence.
        let mut n = MicMeter::default();
        feed(&mut n, 0, 3000, -55.0, 0);
        assert!(!n.reading(3000).silent);
        assert_eq!(n.reading(3000).verdict, Some(Verdict::TooQuiet));
    }

    #[test]
    fn a_clip_event_shows_clipping_for_a_while() {
        let mut m = MicMeter::default();
        // The counter was 5 when the check began: that is the baseline, not a clip.
        feed(&mut m, 0, 500, -20.0, 5);
        assert!(!m.reading(500).clipping);
        m.push(533, -0.1, 6);
        assert!(m.reading(533).clipping);
        feed(&mut m, 566, 3500, -20.0, 6);
        assert!(m.reading(3500).clipping, "3 s after the event at 533");
        m.push(3600, -20.0, 6);
        assert!(!m.reading(3600).clipping);
        assert_eq!(mic_tip(&m.reading(3600)), None);
    }

    #[test]
    fn tips_for_too_quiet_and_clipping() {
        let mut quiet = MicMeter::default();
        feed(&mut quiet, 0, 1500, -45.0, 0);
        assert_eq!(mic_tip(&quiet.reading(1500)), Some(MicTip::TooQuiet));
        let mut clip = MicMeter::default();
        feed(&mut clip, 0, 1500, -2.0, 0);
        clip.push(1533, 0.0, 1);
        assert_eq!(mic_tip(&clip.reading(1533)), Some(MicTip::Clipping));
        let mut ok = MicMeter::default();
        feed(&mut ok, 0, 1500, -20.0, 0);
        assert_eq!(mic_tip(&ok.reading(1500)), None);
        // Silent is not "too quiet": the sub-section is replaced instead.
        let mut silent = MicMeter::default();
        feed(&mut silent, 0, 2500, -100.0, 0);
        assert_eq!(mic_tip(&silent.reading(2500)), None);
    }

    #[test]
    fn the_bar_rises_at_once_and_falls_slowly() {
        let mut m = MicMeter::default();
        m.push(0, -50.0, 0);
        m.push(33, -10.0, 0);
        assert_eq!(m.reading(33).level_db, -10.0);
        m.push(533, -60.0, 0);
        let bar = m.reading(533).level_db;
        assert!((bar - -40.0).abs() < 1e-3, "{bar}: 60 dB/s over 0.5 s");
        // The marker is higher than the bar.
        assert_eq!(m.reading(533).peak_db, -10.0);
        m.reset();
        assert!(!m.has_data());
        assert_eq!(m.reading(0).level_db, METER_FLOOR_DB);
    }

    // ---- the microphone sub-section ----

    #[test]
    fn the_microphone_sub_section_follows_the_session() {
        let idle = CheckSession::default();
        let meter = MicMeter::default();
        assert_eq!(mic_card(&idle, OK, false, &meter, 0), MicCard::Off);

        let mut on = CheckSession::default();
        on.press_start(OK);
        assert_eq!(mic_card(&on, OK, false, &meter, 0), MicCard::Starting);
        let mut live = MicMeter::default();
        feed(&mut live, 0, 500, -20.0, 0);
        assert_eq!(mic_card(&on, OK, false, &live, 500), MicCard::Running);
        let rec = Facts {
            recording: true,
            ..OK
        };
        assert_eq!(mic_card(&on, rec, false, &live, 500), MicCard::Paused);

        let mut quiet = MicMeter::default();
        feed(&mut quiet, 0, 2500, -100.0, 0);
        assert_eq!(mic_card(&on, OK, false, &quiet, 2500), MicCard::Silent);
        assert_eq!(mic_card(&on, OK, true, &quiet, 2500), MicCard::Failed);

        let no_perm = Facts {
            permission: false,
            ..OK
        };
        let mut denied = CheckSession::default();
        denied.press_start(no_perm);
        assert_eq!(
            mic_card(&denied, no_perm, false, &meter, 0),
            MicCard::NoPermission
        );
    }

    // ---- the system audio check ----

    /// A scripted runner: reports the given peaks, then the outcome.
    struct Scripted {
        peaks: Vec<f32>,
        outcome: SystemOutcome,
    }

    impl SystemCheckRunner for Scripted {
        fn run(&self, progress: &(dyn Fn(f32) + Send + Sync)) -> SystemOutcome {
            for p in &self.peaks {
                progress(*p);
            }
            self.outcome.clone()
        }
    }

    /// Runs `runner` the way the page does and feeds the model.
    fn drive(model: &mut SystemCheck, runner: &dyn SystemCheckRunner) {
        let seen = Mutex::new(Vec::new());
        let outcome = runner.run(&|p| seen.lock().unwrap().push(p));
        for p in seen.into_inner().unwrap() {
            model.progress(p);
        }
        model.finish(outcome);
    }

    #[test]
    fn the_system_check_is_idle_until_play_is_pressed() {
        let s = SystemCheck::default();
        assert_eq!(s.status(), &SystemStatus::Idle);
        assert_eq!(count(&s.segments(), Seg::Unlit), 16);
    }

    #[test]
    fn a_heard_chime_fills_the_meter_and_shows_heard() {
        let mut s = SystemCheck::default();
        assert!(s.press_play(true, false));
        assert!(s.is_running());
        drive(
            &mut s,
            &Scripted {
                peaks: vec![-80.0, -30.0, -12.0, -40.0],
                outcome: SystemOutcome::Heard,
            },
        );
        assert_eq!(s.status(), &SystemStatus::Heard);
        let segs = s.segments();
        assert_eq!(count(&segs, Seg::Unlit), 16 - lit_segments(-12.0, 16));
        assert_eq!(segs[0], Seg::Quiet);
    }

    #[test]
    fn every_outcome_has_a_status() {
        for (outcome, want) in [
            (SystemOutcome::NotHeard, SystemStatus::NotHeard),
            (SystemOutcome::NoPermission, SystemStatus::NoPermission),
            (
                SystemOutcome::Failed("x".into()),
                SystemStatus::Failed("x".into()),
            ),
        ] {
            let mut s = SystemCheck::default();
            assert!(s.press_play(true, false));
            drive(
                &mut s,
                &Scripted {
                    peaks: vec![-70.0],
                    outcome,
                },
            );
            assert_eq!(s.status(), &want);
        }
    }

    /// Without Screen Recording nothing runs: the block shows, the runner is not called.
    #[test]
    fn play_without_permission_runs_nothing() {
        let mut s = SystemCheck::default();
        assert!(!s.press_play(false, false));
        assert_eq!(s.status(), &SystemStatus::NoPermission);
        // During a recording the button does nothing.
        let mut r = SystemCheck::default();
        assert!(!r.press_play(true, true));
        assert_eq!(r.status(), &SystemStatus::Idle);
    }

    #[test]
    fn play_again_restarts_and_leaving_forgets() {
        let mut s = SystemCheck::default();
        s.press_play(true, false);
        s.finish(SystemOutcome::Heard);
        assert!(s.press_play(true, false), "again");
        assert_eq!(s.status(), &SystemStatus::Running);
        assert!(!s.press_play(true, false), "not while it runs");
        s.reset();
        assert_eq!(s.status(), &SystemStatus::Idle);
        // A late result after leaving is ignored.
        s.finish(SystemOutcome::Heard);
        assert_eq!(s.status(), &SystemStatus::Idle);
    }
}
