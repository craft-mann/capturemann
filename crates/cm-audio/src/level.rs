//! Live level meter: peak and RMS in dBFS, published ~30 Hz through atomics.
//!
//! The audio callback owns a [`LevelMeter`] and calls [`LevelMeter::push`]; the HUD reads the
//! cloneable [`LevelHandle`] from any thread without locks or allocation.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

/// Reported for silence (and before any audio): the meter floor.
pub const FLOOR_DBFS: f32 = -120.0;
/// Default publish rate (specs/features/05-audio.md updates at 20 Hz; the HUD can poll faster).
pub const DEFAULT_METER_HZ: f32 = 30.0;
/// A sample at or above this magnitude counts as clipping (about -0.004 dBFS).
pub const CLIP_THRESHOLD: f32 = 0.9995;

/// Linear amplitude (0..=1) to dBFS, clamped to [`FLOOR_DBFS`].
pub fn to_dbfs(linear: f32) -> f32 {
    if linear <= 0.0 || !linear.is_finite() {
        return FLOOR_DBFS;
    }
    (20.0 * linear.log10()).max(FLOOR_DBFS)
}

/// One published reading.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Level {
    /// Highest absolute sample in the last window.
    pub peak_dbfs: f32,
    /// RMS over the last window (all channels).
    pub rms_dbfs: f32,
    /// Clipping events since the meter was created (a run of clipped samples counts once).
    pub clip_events: u64,
}

impl Level {
    pub const SILENT: Level = Level {
        peak_dbfs: FLOOR_DBFS,
        rms_dbfs: FLOOR_DBFS,
        clip_events: 0,
    };
}

#[derive(Debug)]
struct Shared {
    peak: AtomicU32,
    rms: AtomicU32,
    clips: AtomicU64,
}

/// Read side of the meter. Cheap to clone.
#[derive(Debug, Clone)]
pub struct LevelHandle(Arc<Shared>);

impl LevelHandle {
    /// A handle showing silence, to be attached to meters later (it survives device changes).
    pub fn new_silent() -> LevelHandle {
        LevelHandle(Arc::new(Shared {
            peak: AtomicU32::new(FLOOR_DBFS.to_bits()),
            rms: AtomicU32::new(FLOOR_DBFS.to_bits()),
            clips: AtomicU64::new(0),
        }))
    }

    pub fn read(&self) -> Level {
        Level {
            peak_dbfs: f32::from_bits(self.0.peak.load(Ordering::Relaxed)),
            rms_dbfs: f32::from_bits(self.0.rms.load(Ordering::Relaxed)),
            clip_events: self.0.clips.load(Ordering::Relaxed),
        }
    }
}

/// Write side of the meter, driven from the audio callback.
#[derive(Debug)]
pub struct LevelMeter {
    shared: Arc<Shared>,
    window_frames: u64,
    channels: usize,
    frames_in_window: u64,
    sum_sq: f64,
    samples_in_window: u64,
    peak: f32,
    in_clip: bool,
}

impl LevelMeter {
    /// A meter for audio at `sample_rate` Hz publishing `hz` times per second.
    pub fn new(sample_rate: u32, channels: u16, hz: f32) -> (LevelMeter, LevelHandle) {
        let shared = Arc::new(Shared {
            peak: AtomicU32::new(FLOOR_DBFS.to_bits()),
            rms: AtomicU32::new(FLOOR_DBFS.to_bits()),
            clips: AtomicU64::new(0),
        });
        let window_frames = (f64::from(sample_rate) / f64::from(hz.max(1.0))).round() as u64;
        (
            LevelMeter {
                shared: shared.clone(),
                window_frames: window_frames.max(1),
                channels: usize::from(channels.max(1)),
                frames_in_window: 0,
                sum_sq: 0.0,
                samples_in_window: 0,
                peak: 0.0,
                in_clip: false,
            },
            LevelHandle(shared),
        )
    }

    /// A meter publishing into an existing handle (the handle outlives device changes).
    pub fn attached(handle: &LevelHandle, sample_rate: u32, channels: u16, hz: f32) -> LevelMeter {
        let (mut m, _) = LevelMeter::new(sample_rate, channels, hz);
        m.shared = handle.0.clone();
        m
    }

    /// Feeds interleaved samples; publishes whenever a window completes.
    pub fn push(&mut self, interleaved: &[f32]) {
        for frame in interleaved.chunks(self.channels) {
            for &s in frame {
                let a = s.abs();
                if a > self.peak {
                    self.peak = a;
                }
                self.sum_sq += f64::from(s) * f64::from(s);
                if a >= CLIP_THRESHOLD {
                    if !self.in_clip {
                        self.shared.clips.fetch_add(1, Ordering::Relaxed);
                        self.in_clip = true;
                    }
                } else {
                    self.in_clip = false;
                }
            }
            self.samples_in_window += frame.len() as u64;
            self.frames_in_window += 1;
            if self.frames_in_window >= self.window_frames {
                self.publish();
            }
        }
    }

    /// Publishes the partial window now (for streams that stop).
    pub fn flush(&mut self) {
        if self.frames_in_window > 0 {
            self.publish();
        }
    }

    /// Shows silence (device gone, stream stopped).
    pub fn silence(&mut self) {
        self.frames_in_window = 0;
        self.samples_in_window = 0;
        self.sum_sq = 0.0;
        self.peak = 0.0;
        self.shared
            .peak
            .store(FLOOR_DBFS.to_bits(), Ordering::Relaxed);
        self.shared
            .rms
            .store(FLOOR_DBFS.to_bits(), Ordering::Relaxed);
    }

    fn publish(&mut self) {
        let rms = if self.samples_in_window > 0 {
            (self.sum_sq / self.samples_in_window as f64).sqrt() as f32
        } else {
            0.0
        };
        self.shared
            .peak
            .store(to_dbfs(self.peak).to_bits(), Ordering::Relaxed);
        self.shared
            .rms
            .store(to_dbfs(rms).to_bits(), Ordering::Relaxed);
        self.frames_in_window = 0;
        self.samples_in_window = 0;
        self.sum_sq = 0.0;
        self.peak = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(amp: f32, n: usize, rate: f32) -> Vec<f32> {
        (0..n)
            .map(|i| amp * (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / rate).sin())
            .collect()
    }

    #[test]
    fn dbfs_math() {
        assert!((to_dbfs(1.0)).abs() < 1e-6);
        assert!((to_dbfs(0.5) + 6.0206).abs() < 1e-3);
        assert_eq!(to_dbfs(0.0), FLOOR_DBFS);
        assert_eq!(to_dbfs(f32::NAN), FLOOR_DBFS);
        assert_eq!(to_dbfs(1e-9), FLOOR_DBFS);
    }

    #[test]
    fn sine_reads_peak_and_rms() {
        let (mut m, h) = LevelMeter::new(48_000, 1, 30.0);
        assert_eq!(h.read().peak_dbfs, FLOOR_DBFS);
        // 1600 frames per window at 30 Hz; 1 kHz divides it exactly (33.3 periods is not whole,
        // so feed 10 windows for a stable RMS).
        m.push(&sine(0.5, 16_000, 48_000.0));
        let l = h.read();
        assert!((l.peak_dbfs + 6.02).abs() < 0.1, "{l:?}");
        // RMS of a sine is peak / sqrt(2): -9.03 dBFS.
        assert!((l.rms_dbfs + 9.03).abs() < 0.2, "{l:?}");
        assert_eq!(l.clip_events, 0);
    }

    #[test]
    fn publishes_about_30_times_a_second() {
        let (mut m, h) = LevelMeter::new(48_000, 1, 30.0);
        m.push(&vec![0.1; 1599]);
        assert_eq!(h.read().peak_dbfs, FLOOR_DBFS, "no window completed yet");
        m.push(&[0.1]);
        assert!((h.read().peak_dbfs + 20.0).abs() < 0.01);
    }

    #[test]
    fn stereo_peak_is_the_louder_channel_and_rms_covers_both() {
        let (mut m, h) = LevelMeter::new(48_000, 2, 30.0);
        let mut v = Vec::new();
        for _ in 0..1600 {
            v.push(0.8);
            v.push(0.0);
        }
        m.push(&v);
        let l = h.read();
        assert!((l.peak_dbfs - to_dbfs(0.8)).abs() < 1e-3);
        // Mean square = 0.32, rms = 0.5657 -> -4.95 dBFS.
        assert!((l.rms_dbfs + 4.95).abs() < 0.05, "{l:?}");
    }

    #[test]
    fn clipping_runs_count_once() {
        let (mut m, h) = LevelMeter::new(48_000, 1, 30.0);
        let mut v = vec![0.0f32; 3200];
        for s in &mut v[10..20] {
            *s = 1.0;
        }
        for s in &mut v[100..101] {
            *s = -1.0;
        }
        v[2000] = 0.99; // below the threshold
        m.push(&v);
        assert_eq!(h.read().clip_events, 2);
    }

    #[test]
    fn silence_resets_the_display() {
        let (mut m, h) = LevelMeter::new(48_000, 1, 30.0);
        m.push(&vec![0.7; 1600]);
        assert!(h.read().peak_dbfs > -10.0);
        m.silence();
        assert_eq!(h.read().peak_dbfs, FLOOR_DBFS);
        assert_eq!(h.read().rms_dbfs, FLOOR_DBFS);
    }
}
