//! A scripted audio source for CI and for other crates' recording-session tests (no hardware, no
//! permissions, no real time).
//!
//! The test thread plays the role of the OS audio callback through a [`FakeHandle`]: it advances
//! the device clock, delivers chunks stamped on the host timeline, and can inject the faults the
//! recorder must survive (dropouts, a device clock that runs fast or slow, stamp jumps, a
//! disconnect). Because chunks and the recorder's control messages travel one queue from one
//! thread, runs are fully deterministic.

use std::sync::{Arc, Mutex, MutexGuard};

use cm_core::CmError;

use crate::source::{AudioSource, SinkNotifier, SourceFormat, SourceSink};

/// What the fake device "hears".
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FakeSignal {
    Silence,
    Sine {
        freq_hz: f64,
        amp: f32,
    },
    /// Deterministic white noise in `[-amp, amp]`.
    Noise {
        amp: f32,
    },
    /// A constant level (handy for meter tests).
    Constant(f32),
    /// Silence with a `len_s` sine beep starting every `period_s` of device time (sync tests).
    Beeps {
        period_s: f64,
        len_s: f64,
        freq_hz: f64,
        amp: f32,
    },
}

/// Configuration of a fake device.
#[derive(Debug, Clone)]
pub struct FakeConfig {
    pub sample_rate: u32,
    pub channels: u16,
    pub device_name: String,
    pub device_id: String,
    /// Frames per delivered chunk (10 ms at 48 kHz by default).
    pub chunk_frames: usize,
    /// Host time of the device's first sample.
    pub host_start_ns: u64,
    /// Device clock error: positive means the device clock is slow (it delivers fewer samples per
    /// host second), which makes sample-count time lag behind the OS timestamps.
    pub drift_ppm: f64,
    pub signal: FakeSignal,
}

impl Default for FakeConfig {
    fn default() -> Self {
        FakeConfig {
            sample_rate: 48_000,
            channels: 1,
            device_name: "Fake Mic".into(),
            device_id: "fake:mic".into(),
            chunk_frames: 480,
            host_start_ns: 0,
            drift_ppm: 0.0,
            signal: FakeSignal::Sine {
                freq_hz: 1000.0,
                amp: 0.5,
            },
        }
    }
}

struct State {
    cfg: FakeConfig,
    sink: Option<SourceSink>,
    notifier: Option<SinkNotifier>,
    /// Device samples produced so far (delivered or skipped).
    next_sample: u64,
    /// Added to every stamp (stamp jumps).
    host_shift_ns: i64,
    noise_state: u32,
    started: bool,
    fail_start: Option<CmError>,
    reopen: bool,
}

/// The source half, handed to the recorder.
pub struct FakeAudioSource {
    shared: Arc<Mutex<State>>,
    format: SourceFormat,
}

/// The "OS" half, kept by the test.
#[derive(Clone)]
pub struct FakeHandle {
    shared: Arc<Mutex<State>>,
}

impl FakeAudioSource {
    pub fn new(cfg: FakeConfig) -> (FakeAudioSource, FakeHandle) {
        let format = SourceFormat {
            sample_rate: cfg.sample_rate,
            channels: cfg.channels,
            device_name: cfg.device_name.clone(),
            device_id: cfg.device_id.clone(),
        };
        let shared = Arc::new(Mutex::new(State {
            cfg,
            sink: None,
            notifier: None,
            next_sample: 0,
            host_shift_ns: 0,
            noise_state: 0x1234_5678,
            started: false,
            fail_start: None,
            reopen: false,
        }));
        (
            FakeAudioSource {
                shared: shared.clone(),
                format,
            },
            FakeHandle { shared },
        )
    }
}

fn lock(m: &Mutex<State>) -> MutexGuard<'_, State> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

impl AudioSource for FakeAudioSource {
    fn format(&self) -> SourceFormat {
        self.format.clone()
    }

    fn start(&mut self, sink: SourceSink) -> Result<(), CmError> {
        let mut s = lock(&self.shared);
        if let Some(e) = s.fail_start.take() {
            return Err(e);
        }
        s.notifier = Some(sink.notifier());
        s.sink = Some(sink);
        s.started = true;
        Ok(())
    }

    fn stop(&mut self) {
        let mut s = lock(&self.shared);
        s.sink = None;
        s.started = false;
    }

    fn needs_reopen(&mut self) -> bool {
        std::mem::take(&mut lock(&self.shared).reopen)
    }
}

impl State {
    fn host_ns_of(&self, sample: u64) -> u64 {
        let ns = sample as f64 * 1e9 / f64::from(self.cfg.sample_rate)
            * (1.0 + self.cfg.drift_ppm * 1e-6);
        let t =
            self.cfg.host_start_ns as i128 + ns.round() as i128 + i128::from(self.host_shift_ns);
        u64::try_from(t.max(0)).unwrap_or(0)
    }

    fn sample_value(&mut self, index: u64) -> f32 {
        match self.cfg.signal {
            FakeSignal::Silence => 0.0,
            FakeSignal::Constant(v) => v,
            FakeSignal::Sine { freq_hz, amp } => {
                let t = index as f64 / f64::from(self.cfg.sample_rate);
                (amp as f64 * (2.0 * std::f64::consts::PI * freq_hz * t).sin()) as f32
            }
            FakeSignal::Beeps {
                period_s,
                len_s,
                freq_hz,
                amp,
            } => {
                let t = index as f64 / f64::from(self.cfg.sample_rate);
                if t % period_s < len_s {
                    (amp as f64 * (2.0 * std::f64::consts::PI * freq_hz * t).sin()) as f32
                } else {
                    0.0
                }
            }
            FakeSignal::Noise { amp } => {
                let mut s = self.noise_state;
                s ^= s << 13;
                s ^= s >> 17;
                s ^= s << 5;
                self.noise_state = s;
                (s as f32 / u32::MAX as f32 * 2.0 - 1.0) * amp
            }
        }
    }
}

impl FakeHandle {
    /// Delivers `secs` of device time as chunks, stamped on the host timeline.
    pub fn advance(&self, secs: f64) {
        let mut s = lock(&self.shared);
        let total = (secs * f64::from(s.cfg.sample_rate)).round() as u64;
        let ch = usize::from(s.cfg.channels);
        let mut done = 0u64;
        while done < total {
            let n = (s.cfg.chunk_frames as u64).min(total - done);
            let start = s.next_sample;
            let stamp = s.host_ns_of(start);
            let mut data = Vec::with_capacity(n as usize * ch);
            for i in 0..n {
                let v = s.sample_value(start + i);
                for _ in 0..ch {
                    data.push(v);
                }
            }
            s.next_sample += n;
            done += n;
            if let Some(sink) = s.sink.as_mut() {
                sink.push_blocking(stamp, &data);
            }
        }
    }

    /// Device time passes without any delivery (dropout).
    pub fn skip(&self, secs: f64) {
        let mut s = lock(&self.shared);
        s.next_sample += (secs * f64::from(s.cfg.sample_rate)).round() as u64;
    }

    /// Shifts every later stamp by `ns` (positive: later) without touching the audio.
    pub fn shift_host_ns(&self, ns: i64) {
        lock(&self.shared).host_shift_ns += ns;
    }

    /// Changes what the device hears from the next delivered sample on.
    pub fn set_signal(&self, signal: FakeSignal) {
        lock(&self.shared).cfg.signal = signal;
    }

    /// Host time of the next undelivered sample.
    pub fn next_host_ns(&self) -> u64 {
        let s = lock(&self.shared);
        s.host_ns_of(s.next_sample)
    }

    /// Reports a device error as the OS would on unplug.
    pub fn disconnect(&self, reason: &str) {
        let n = lock(&self.shared).notifier.clone();
        if let Some(n) = n {
            n.disconnected(reason);
        }
    }

    /// Makes the next `start` fail with `err` (device busy, permission).
    pub fn fail_next_start(&self, err: CmError) {
        lock(&self.shared).fail_start = Some(err);
    }

    /// Simulates the OS default device changing while this one keeps working.
    pub fn request_reopen(&self) {
        lock(&self.shared).reopen = true;
    }

    pub fn is_started(&self) -> bool {
        lock(&self.shared).started
    }
}
