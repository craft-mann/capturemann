//! Microphone track (AUD-1, AUD-3): `mic.wav` raw and `mic.denoised.wav` beside it.

use std::sync::mpsc::{channel, Sender};
use std::thread::JoinHandle;
use std::time::Duration;

use cm_core::{CmError, MediaTime, TrackContext, TrackInfo, TrackKind, TrackRecorder, TrackStats};

use crate::cpal_source::CpalSource;
use crate::level::{LevelHandle, LevelMeter, DEFAULT_METER_HZ};
use crate::permission::require_microphone;
use crate::pipeline::DEFAULT_RESYNC_MS;
use crate::recorder::{AudioEvent, AudioRecorder, RecorderSettings};
use crate::source::{
    AudioSource, Counters, DeviceSelector, SourceFactory, SourceFormat, SourceSink, START_TIMEOUT,
};

/// Raw mic file name in the bundle.
pub const MIC_RAW_FILE: &str = "mic.wav";
/// Denoised mic file name in the bundle.
pub const MIC_DENOISED_FILE: &str = "mic.denoised.wav";

/// Mic recording options.
#[derive(Debug, Clone)]
pub struct MicConfig {
    /// `Default` or a device id from `list_audio_devices`.
    pub device: DeviceSelector,
    /// Write `mic.denoised.wav` (RNNoise). On by default (AUD-3).
    pub denoise: bool,
    /// Tolerance against the OS timestamps before silence is inserted or frames trimmed.
    pub resync_ms: f64,
    /// Calibration offset added to capture stamps (ms). 0 until measured on hardware
    /// (tests/manual/AUD-1.md, step 4).
    pub stamp_offset_ms: f64,
}

impl Default for MicConfig {
    fn default() -> Self {
        MicConfig {
            device: DeviceSelector::Default,
            denoise: true,
            resync_ms: DEFAULT_RESYNC_MS,
            stamp_offset_ms: 0.0,
        }
    }
}

/// The real cpal factory. Fails with `permission_missing(microphone)` instead of letting the OS
/// prompt unexpectedly: the app requests access explicitly via
/// [`crate::request_microphone_access`].
pub fn cpal_mic_factory() -> SourceFactory {
    Box::new(|sel: &DeviceSelector| {
        require_microphone()?;
        Ok(Box::new(CpalSource::open_input(sel)?) as Box<dyn AudioSource>)
    })
}

/// Records the microphone on the session clock.
pub struct MicRecorder {
    inner: AudioRecorder,
}

impl MicRecorder {
    /// A recorder on the real microphone.
    pub fn new(cfg: MicConfig) -> Self {
        Self::with_factory(cfg, cpal_mic_factory())
    }

    /// A recorder over any source factory (tests use [`crate::FakeAudioSource`]).
    pub fn with_factory(cfg: MicConfig, factory: SourceFactory) -> Self {
        MicRecorder {
            inner: AudioRecorder::new(
                RecorderSettings {
                    kind: TrackKind::Mic,
                    device: cfg.device,
                    raw_file: MIC_RAW_FILE.into(),
                    denoised_file: cfg.denoise.then(|| MIC_DENOISED_FILE.into()),
                    force_channels: None,
                    resync_ms: cfg.resync_ms,
                    stamp_offset_ms: cfg.stamp_offset_ms,
                },
                factory,
            ),
        }
    }

    /// Live level (peak/RMS dBFS, ~30 Hz) for the HUD; valid before `start`.
    pub fn level_handle(&self) -> LevelHandle {
        self.inner.level_handle()
    }

    /// Device changes and losses since the last call (toasts, `session.json`).
    pub fn drain_events(&self) -> Vec<AudioEvent> {
        self.inner.drain_events()
    }

    /// First-sample facts for crash recovery (R3-2).
    pub fn recovery_hint(&self) -> Option<serde_json::Value> {
        self.inner.recovery_hint()
    }

    /// The device in use after `start`.
    pub fn device_format(&self) -> Option<SourceFormat> {
        self.inner.device_format()
    }

    /// Mutes or unmutes the track without stopping it (AUD-6): see
    /// [`AudioRecorder::set_muted`].
    pub fn set_muted(&self, muted: bool) -> Result<crate::MuteChange, CmError> {
        self.inner.set_muted(muted)
    }

    /// Whether the microphone is muted now.
    pub fn is_muted(&self) -> bool {
        self.inner.is_muted()
    }

    /// Test hook: the raw file fails (disk full) after `bytes` of audio.
    #[doc(hidden)]
    #[must_use]
    pub fn with_write_fault_after(mut self, bytes: u64) -> Self {
        self.inner = self.inner.with_write_fault_after(bytes);
        self
    }

    /// Test hook: how long `start` waits for the device.
    #[doc(hidden)]
    #[must_use]
    pub fn with_start_timeout(mut self, t: Duration) -> Self {
        self.inner = self.inner.with_start_timeout(t);
        self
    }

    /// Waits until everything captured so far has been processed.
    pub fn sync(&self) -> Result<(), CmError> {
        self.inner.sync()
    }
}

impl TrackRecorder for MicRecorder {
    fn kind(&self) -> TrackKind {
        TrackKind::Mic
    }
    fn start(&mut self, ctx: TrackContext) -> Result<(), CmError> {
        self.inner.start(ctx)
    }
    fn pause(&mut self, at: MediaTime) -> Result<(), CmError> {
        self.inner.pause(at)
    }
    fn resume(&mut self, at: MediaTime) -> Result<(), CmError> {
        self.inner.resume(at)
    }
    fn stop(&mut self, at: MediaTime) -> Result<TrackInfo, CmError> {
        self.inner.stop(at)
    }
    fn stats(&self) -> TrackStats {
        self.inner.stats()
    }
}

/// Meter-only microphone stream for the setup panel (no files). Dropping it closes the device.
/// Switching devices is `drop` + `start`; the first reading arrives within one 33 ms window of the
/// first buffer (AUD-1.AC1: within 500 ms).
pub struct MicMonitor {
    level: LevelHandle,
    format: SourceFormat,
    stop: Option<Sender<()>>,
    join: Option<JoinHandle<()>>,
}

impl MicMonitor {
    pub fn start(device: &DeviceSelector) -> Result<Self, CmError> {
        Self::start_with(device.clone(), cpal_mic_factory())
    }

    pub fn start_with(device: DeviceSelector, mut factory: SourceFactory) -> Result<Self, CmError> {
        let level = LevelHandle::new_silent();
        let (ready_tx, ready_rx) = channel::<Result<SourceFormat, CmError>>();
        let (stop_tx, stop_rx) = channel::<()>();
        let lv = level.clone();
        let join = std::thread::Builder::new()
            .name("cm-audio-monitor".into())
            .spawn(move || {
                let mut source = match factory(&device) {
                    Ok(s) => s,
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                let format = source.format();
                let meter = LevelMeter::attached(
                    &lv,
                    format.sample_rate,
                    format.channels,
                    DEFAULT_METER_HZ,
                );
                let sink = SourceSink::new(
                    &format,
                    None,
                    std::sync::Arc::new(Counters::default()),
                    meter,
                    0,
                );
                if let Err(e) = source.start(sink) {
                    let _ = ready_tx.send(Err(e));
                    return;
                }
                let _ = ready_tx.send(Ok(format));
                let _ = stop_rx.recv();
                source.stop();
            })
            .map_err(|e| CmError::internal(format!("cannot start monitor thread: {e}")))?;
        match ready_rx.recv_timeout(START_TIMEOUT.min(Duration::from_secs(10))) {
            Ok(Ok(format)) => Ok(MicMonitor {
                level,
                format,
                stop: Some(stop_tx),
                join: Some(join),
            }),
            Ok(Err(e)) => {
                let _ = join.join();
                Err(e)
            }
            Err(_) => Err(CmError::internal("timed out opening the microphone")),
        }
    }

    pub fn level_handle(&self) -> LevelHandle {
        self.level.clone()
    }

    pub fn format(&self) -> &SourceFormat {
        &self.format
    }
}

impl Drop for MicMonitor {
    fn drop(&mut self) {
        if let Some(tx) = self.stop.take() {
            let _ = tx.send(());
        }
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}
