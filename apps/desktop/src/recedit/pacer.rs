//! Playback pacing, free of any display and any audio device: which **output time** (seconds on
//! the exported timeline) the preview should show right now.
//!
//! The audio device is the master clock while it runs, so picture and sound cannot drift
//! (NFR-3). When there is no audio (a recording without microphone or system audio, no device,
//! the mix still being rendered) the wall clock leads. When the two disagree by more than
//! [`DRIFT_MAX`] (a stalled or restarted device) the wall clock wins and the caller is told to
//! restart the audio at the new time ([`Tick::Play::resync`]).
//!
//! Output time already contains cuts and speed (`cm_core::edit::Timeline`), so a cut is skipped
//! and a 2x segment advances twice as fast in session time without any special case here.

use std::time::Instant;

/// Audio and wall clock may differ by this much (seconds) before the audio is restarted.
pub const DRIFT_MAX: f64 = 0.25;

/// Within this many seconds of the end counts as the end (half a 60 fps frame).
const END_EPS: f64 = 1.0 / 120.0;

/// What a tick decided.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tick {
    /// Show this output time. `resync`: restart the audio here, it fell out of step.
    Play { out: f64, resync: bool },
    /// Playback reached the end and stopped.
    Ended,
    /// Playback reached the end and starts again at 0.
    Looped,
}

#[derive(Debug, Clone, Copy)]
struct Anchor {
    out: f64,
    at: Instant,
}

/// The playback clock.
#[derive(Debug, Default)]
pub struct Pacer {
    anchor: Option<Anchor>,
}

impl Pacer {
    pub fn is_running(&self) -> bool {
        self.anchor.is_some()
    }

    /// Playback starts (or restarts after a seek) at output time `out`.
    pub fn start(&mut self, out: f64, now: Instant) {
        self.anchor = Some(Anchor { out, at: now });
    }

    pub fn stop(&mut self) {
        self.anchor = None;
    }

    /// The output time at `now`. `audio` is the device's position in output seconds, when audio
    /// is playing; `duration` is the length of the export.
    pub fn tick(&mut self, now: Instant, audio: Option<f64>, duration: f64, looping: bool) -> Tick {
        let Some(a) = self.anchor else {
            return Tick::Ended;
        };
        let wall = a.out + now.saturating_duration_since(a.at).as_secs_f64();
        let (out, resync) = match audio {
            Some(p) if (p - wall).abs() <= DRIFT_MAX => (p, false),
            Some(_) => (wall, true),
            None => (wall, false),
        };
        if out >= duration - END_EPS {
            if looping && duration > 0.0 {
                self.anchor = Some(Anchor { out: 0.0, at: now });
                return Tick::Looped;
            }
            self.anchor = None;
            return Tick::Ended;
        }
        // Re-anchor on what was shown, so the wall estimate never drifts from the audio.
        self.anchor = Some(Anchor { out, at: now });
        Tick::Play { out, resync }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn at(base: Instant, ms: u64) -> Instant {
        base + Duration::from_millis(ms)
    }

    /// Without audio the wall clock leads, one second per second.
    #[test]
    fn wall_clock_without_audio() {
        let t0 = Instant::now();
        let mut p = Pacer::default();
        p.start(2.0, t0);
        assert_eq!(
            p.tick(at(t0, 500), None, 10.0, false),
            Tick::Play {
                out: 2.5,
                resync: false
            }
        );
        assert_eq!(
            p.tick(at(t0, 1_000), None, 10.0, false),
            Tick::Play {
                out: 3.0,
                resync: false
            }
        );
    }

    /// With a healthy audio clock the picture follows the sound exactly (NFR-3).
    #[test]
    fn audio_is_the_master_clock() {
        let t0 = Instant::now();
        let mut p = Pacer::default();
        p.start(0.0, t0);
        // The device reports slightly less than the wall clock (output latency).
        assert_eq!(
            p.tick(at(t0, 1_000), Some(0.93), 10.0, false),
            Tick::Play {
                out: 0.93,
                resync: false
            }
        );
        // The wall estimate now continues from the audio, not from the start.
        match p.tick(at(t0, 1_500), None, 10.0, false) {
            Tick::Play { out, resync } => {
                assert!((out - 1.43).abs() < 1e-9 && !resync, "{out}");
            }
            other => panic!("{other:?}"),
        }
    }

    /// A stalled device (position stuck) hands the lead to the wall clock and asks for a restart.
    #[test]
    fn a_stalled_audio_clock_is_resynced() {
        let t0 = Instant::now();
        let mut p = Pacer::default();
        p.start(0.0, t0);
        assert!(matches!(
            p.tick(at(t0, 100), Some(0.1), 10.0, false),
            Tick::Play { resync: false, .. }
        ));
        // 600 ms later the device still says 0.1 s.
        match p.tick(at(t0, 700), Some(0.1), 10.0, false) {
            Tick::Play { out, resync } => {
                assert!(resync);
                assert!((out - 0.7).abs() < 1e-9, "{out}");
            }
            other => panic!("{other:?}"),
        }
    }

    /// The end stops playback, or loops it.
    #[test]
    fn the_end_stops_or_loops() {
        let t0 = Instant::now();
        let mut p = Pacer::default();
        p.start(9.5, t0);
        assert_eq!(p.tick(at(t0, 600), None, 10.0, false), Tick::Ended);
        assert!(!p.is_running());
        p.start(9.5, t0);
        assert_eq!(p.tick(at(t0, 600), None, 10.0, true), Tick::Looped);
        assert!(p.is_running());
        assert_eq!(
            p.tick(at(t0, 1_100), None, 10.0, true),
            Tick::Play {
                out: 0.5,
                resync: false
            }
        );
    }

    /// A stopped pacer reports the end, and an empty export never loops forever.
    #[test]
    fn stopped_and_empty() {
        let t0 = Instant::now();
        let mut p = Pacer::default();
        assert_eq!(p.tick(t0, None, 10.0, false), Tick::Ended);
        p.start(0.0, t0);
        assert_eq!(p.tick(t0, None, 0.0, true), Tick::Ended);
    }
}
