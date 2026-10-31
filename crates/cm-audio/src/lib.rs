//! Mic and system audio capture, level meters and denoise.
//!
//! * [`MicRecorder`] (AUD-1, AUD-3): `cpal` input -> `mic.wav` (raw) and `mic.denoised.wav`
//!   (RNNoise), 48 kHz 32-bit float, aligned to the session clock by OS capture timestamps.
//! * [`SystemAudioRecorder`] (AUD-2): ScreenCaptureKit (macOS) / WASAPI loopback (Windows) ->
//!   `system.wav`.
//! * [`list_audio_devices`], [`microphone_permission`], [`request_microphone_access`]: the audio half
//!   of `list_devices` and the permission flow. Checking never prompts.
//! * [`run_check`] (Settings system-audio check): plays a chime from `afplay` and reports whether
//!   system capture heard it.
//! * [`FakeAudioSource`]: a scripted source so sessions and CI can run without hardware.
//!
//! Both recorders implement `cm_core::TrackRecorder`. Pause drops audio inside the paused interval
//! and leaves no hole in the files (REC-4); the pauses, dropouts, device changes, measured drift and
//! first-sample time are returned in `TrackInfo::details`.

#![cfg_attr(test, allow(clippy::unwrap_used))]

mod chime;
mod cpal_source;
pub mod denoise;
mod devices;
mod fake;
pub mod level;
mod mic;
pub mod mix;
pub mod mute;
mod permission;
pub mod pipeline;
pub mod playback;
mod recorder;
pub mod resample;
mod source;
mod sys;
pub use sys::linux_pipewire::{
    available as pipewire_available, mic_factory as pipewire_mic_factory,
    monitor_factory as pipewire_monitor_factory, STREAM_RATE as PIPEWIRE_RATE,
};
mod system;
pub mod wav;

pub use chime::{
    chime_wav, run_check, AfplayPlayer, ChimeConfig, ChimeHandle, ChimeOutcome, ChimePlayer,
    ChimeProcess, FakeChimePlayer, FakeChimeScript,
};
pub use cpal_source::{map_cpal_error, CpalSource};
pub use devices::{
    list_audio_devices, system_audio_support, AudioDevices, MicDevice, RateRange,
    SystemAudioSupport,
};
pub use fake::{FakeAudioSource, FakeConfig, FakeHandle, FakeSignal};
pub use level::{to_dbfs, Level, LevelHandle, LevelMeter, FLOOR_DBFS};
pub use mic::{
    cpal_mic_factory, MicConfig, MicMonitor, MicRecorder, MIC_DENOISED_FILE, MIC_RAW_FILE,
};
pub use mute::{MuteChange, MuteControl, MuteSpan};
pub use permission::{
    microphone_permission, request_microphone_access, require_microphone,
    screen_recording_permission, PermissionState,
};
pub use pipeline::{DEFAULT_RESYNC_MS, TRACK_RATE};
pub use recorder::{AudioEvent, AudioRecorder, RecorderSettings};
pub use source::{
    AudioChunk, AudioSource, DeviceSelector, SinkNotifier, SourceFactory, SourceFormat, SourceSink,
};
pub use system::{system_audio_factory, SystemAudioConfig, SystemAudioRecorder, SYSTEM_FILE};
pub use wav::{read_info, repair_wav, WavInfo, WavWriter};
