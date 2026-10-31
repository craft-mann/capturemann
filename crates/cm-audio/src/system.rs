//! System audio track (AUD-2): `system.wav`, 48 kHz stereo float on the session clock.
//!
//! macOS: ScreenCaptureKit (see `sys::mac_sck` for why it runs its own audio-only stream).
//! Windows: WASAPI loopback of the default output through cpal; follows a default-device change
//! within ~250 ms and reports the gap (AUD-2.AC3). Other OSes: `not_implemented`.

use cm_core::{CmError, MediaTime, TrackContext, TrackInfo, TrackKind, TrackRecorder, TrackStats};

use crate::level::LevelHandle;
use crate::pipeline::DEFAULT_RESYNC_MS;
use crate::recorder::{AudioEvent, AudioRecorder, RecorderSettings};
#[allow(unused_imports)] // unused on Linux, where the PipeWire factory builds the source
use crate::source::{AudioSource, DeviceSelector, SourceFactory, SourceFormat};

/// System audio file name in the bundle.
pub const SYSTEM_FILE: &str = "system.wav";

/// System-audio options.
#[derive(Debug, Clone)]
pub struct SystemAudioConfig {
    /// macOS: bundle ids whose audio is left out (CaptureMann's own helper processes, for example).
    /// The current process's audio is always excluded.
    pub exclude_bundle_ids: Vec<String>,
    pub resync_ms: f64,
    /// Calibration offset added to capture stamps (ms).
    pub stamp_offset_ms: f64,
}

impl Default for SystemAudioConfig {
    fn default() -> Self {
        SystemAudioConfig {
            exclude_bundle_ids: Vec::new(),
            resync_ms: DEFAULT_RESYNC_MS,
            stamp_offset_ms: 0.0,
        }
    }
}

/// The real source factory for this OS.
pub fn system_audio_factory(exclude_bundle_ids: Vec<String>) -> SourceFactory {
    #[cfg(target_os = "macos")]
    {
        Box::new(move |_sel: &DeviceSelector| {
            Ok(Box::new(crate::sys::mac_sck::SckAudioSource::open(
                &exclude_bundle_ids,
            )?) as Box<dyn AudioSource>)
        })
    }
    #[cfg(target_os = "windows")]
    {
        let _ = &exclude_bundle_ids;
        Box::new(|_sel: &DeviceSelector| {
            Ok(
                Box::new(crate::cpal_source::CpalSource::open_default_output_loopback()?)
                    as Box<dyn AudioSource>,
            )
        })
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        // Linux: the monitor of the default sink through PipeWire (answers `not_implemented` on
        // any other system). Our own audio cannot be excluded.
        let _ = &exclude_bundle_ids;
        crate::sys::linux_pipewire::monitor_factory()
    }
}

/// Records what the computer plays.
pub struct SystemAudioRecorder {
    inner: AudioRecorder,
}

impl SystemAudioRecorder {
    pub fn new(cfg: SystemAudioConfig) -> Self {
        let factory = system_audio_factory(cfg.exclude_bundle_ids.clone());
        Self::with_factory(cfg, factory)
    }

    /// Over any source factory (tests use [`crate::FakeAudioSource`]).
    pub fn with_factory(cfg: SystemAudioConfig, factory: SourceFactory) -> Self {
        SystemAudioRecorder {
            inner: AudioRecorder::new(
                RecorderSettings {
                    kind: TrackKind::System,
                    device: DeviceSelector::Default,
                    raw_file: SYSTEM_FILE.into(),
                    denoised_file: None,
                    force_channels: Some(2),
                    resync_ms: cfg.resync_ms,
                    stamp_offset_ms: cfg.stamp_offset_ms,
                },
                factory,
            ),
        }
    }

    /// First-sample facts for crash recovery (R3-2).
    pub fn recovery_hint(&self) -> Option<serde_json::Value> {
        self.inner.recovery_hint()
    }

    /// Live level for the HUD's system-audio meter.
    pub fn level_handle(&self) -> LevelHandle {
        self.inner.level_handle()
    }

    pub fn drain_events(&self) -> Vec<AudioEvent> {
        self.inner.drain_events()
    }

    pub fn device_format(&self) -> Option<SourceFormat> {
        self.inner.device_format()
    }

    pub fn sync(&self) -> Result<(), CmError> {
        self.inner.sync()
    }
}

impl TrackRecorder for SystemAudioRecorder {
    fn kind(&self) -> TrackKind {
        TrackKind::System
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
