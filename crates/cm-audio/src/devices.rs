//! Device enumeration for the `list_devices` tool (specs/02-agent-interface.md) and the setup panel.

use cpal::traits::{DeviceTrait, HostTrait};
use serde::{Deserialize, Serialize};

use crate::permission::{microphone_permission, screen_recording_permission, PermissionState};

/// A sample-rate range a device supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RateRange {
    pub min: u32,
    pub max: u32,
}

/// One microphone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MicDevice {
    /// Stable id to pass as `mic` to `start_recording` (e.g. `coreaudio:<uid>`).
    pub id: String,
    pub name: String,
    /// The system default input.
    pub default: bool,
    /// Channels and sample rate of the device's current configuration. Recording resamples to
    /// 48 kHz when `sample_rate` differs.
    pub channels: u16,
    pub sample_rate: u32,
    pub max_channels: u16,
    pub sample_rate_ranges: Vec<RateRange>,
}

/// Whether and how system audio can be recorded.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SystemAudioSupport {
    pub supported: bool,
    /// `screencapturekit`, `wasapi_loopback` or `none`.
    pub backend: String,
    /// Why it is unsupported (shown instead of hiding the option on unsupported OSes).
    pub reason: Option<String>,
    /// ScreenCaptureKit needs Screen Recording access.
    pub permission: PermissionState,
    /// CaptureMann's own sounds (countdown) are excluded from the track.
    pub excludes_own_audio: bool,
}

/// The audio half of `list_devices`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioDevices {
    pub mics: Vec<MicDevice>,
    pub system_audio: SystemAudioSupport,
    pub microphone_permission: PermissionState,
    /// Setup-panel text when the microphone is blocked.
    pub microphone_permission_message: Option<String>,
}

/// Lists microphones and system-audio support. Never prompts for permission and never opens a
/// stream; the permission state is reported alongside.
pub fn list_audio_devices() -> AudioDevices {
    let host = cpal::default_host();
    let default_id = host
        .default_input_device()
        .and_then(|d| d.id().ok())
        .map(|i| i.to_string());
    let mut mics = Vec::new();
    if let Ok(devices) = host.input_devices() {
        for d in devices {
            let Ok(id) = d.id().map(|i| i.to_string()) else {
                continue;
            };
            let Ok(cfg) = d.default_input_config() else {
                continue;
            };
            let mut ranges: Vec<RateRange> = Vec::new();
            let mut max_channels = cfg.channels();
            if let Ok(all) = d.supported_input_configs() {
                for c in all {
                    max_channels = max_channels.max(c.channels());
                    let r = RateRange {
                        min: c.min_sample_rate(),
                        max: c.max_sample_rate(),
                    };
                    if !ranges.contains(&r) {
                        ranges.push(r);
                    }
                }
            }
            mics.push(MicDevice {
                default: default_id.as_deref() == Some(id.as_str()),
                name: d
                    .description()
                    .map(|x| x.name().to_string())
                    .unwrap_or_else(|_| id.clone()),
                id,
                channels: cfg.channels(),
                sample_rate: cfg.sample_rate(),
                max_channels,
                sample_rate_ranges: ranges,
            });
        }
    }
    // Where PipeWire serves the audio (Linux) the recording uses it, not ALSA: list what it will
    // record, not what the ALSA plug-in happens to expose.
    if crate::sys::linux_pipewire::available() {
        mics = vec![crate::sys::linux_pipewire::default_mic_device()];
    }
    // Default first, then by name, so the list is stable.
    mics.sort_by(|a, b| b.default.cmp(&a.default).then_with(|| a.name.cmp(&b.name)));
    let permission = microphone_permission();
    AudioDevices {
        mics,
        system_audio: system_audio_support(),
        microphone_permission: permission,
        microphone_permission_message: permission.message().map(str::to_string),
    }
}

/// System-audio capability of this OS.
pub fn system_audio_support() -> SystemAudioSupport {
    #[cfg(target_os = "macos")]
    {
        let major = crate::sys::mac_sck::macos_major_version();
        let supported = major >= 13;
        SystemAudioSupport {
            supported,
            backend: if supported {
                "screencapturekit"
            } else {
                "none"
            }
            .into(),
            reason: (!supported).then(|| "System audio needs macOS 13 or later.".to_string()),
            permission: screen_recording_permission(),
            excludes_own_audio: true,
        }
    }
    #[cfg(target_os = "windows")]
    {
        SystemAudioSupport {
            supported: true,
            backend: "wasapi_loopback".into(),
            reason: None,
            permission: screen_recording_permission(),
            excludes_own_audio: false,
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let pw = crate::sys::linux_pipewire::available();
        SystemAudioSupport {
            supported: pw,
            backend: if pw { "pipewire_monitor" } else { "none" }.into(),
            reason: (!pw).then(|| "System audio needs PipeWire (not running here).".to_string()),
            permission: screen_recording_permission(),
            excludes_own_audio: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enumeration_has_the_list_devices_shape() {
        // Works with zero devices (CI) and with real ones; never prompts.
        let d = list_audio_devices();
        assert!(d.mics.iter().filter(|m| m.default).count() <= 1);
        for m in &d.mics {
            assert!(!m.id.is_empty());
            assert!(m.channels >= 1 && m.sample_rate >= 8000);
            assert!(m.max_channels >= m.channels);
        }
        if let Some(first) = d.mics.first() {
            // The default (if any) sorts first.
            assert!(first.default || d.mics.iter().all(|m| !m.default));
        }
        let v = serde_json::to_value(&d).unwrap();
        for key in ["mics", "system_audio", "microphone_permission"] {
            assert!(v.get(key).is_some(), "missing {key}");
        }
        assert!(v["system_audio"]["backend"].is_string());
        assert!(v["system_audio"]["supported"].is_boolean());
    }

    #[test]
    fn permission_message_follows_state() {
        let d = list_audio_devices();
        assert_eq!(
            d.microphone_permission_message.is_none(),
            d.microphone_permission == PermissionState::Authorized
        );
    }

    #[test]
    fn json_round_trips() {
        let d = AudioDevices {
            mics: vec![MicDevice {
                id: "coreaudio:abc".into(),
                name: "MacBook Pro Microphone".into(),
                default: true,
                channels: 1,
                sample_rate: 48_000,
                max_channels: 1,
                sample_rate_ranges: vec![RateRange {
                    min: 48_000,
                    max: 48_000,
                }],
            }],
            system_audio: SystemAudioSupport {
                supported: true,
                backend: "screencapturekit".into(),
                reason: None,
                permission: PermissionState::Authorized,
                excludes_own_audio: true,
            },
            microphone_permission: PermissionState::Denied,
            microphone_permission_message: Some("off".into()),
        };
        let back: AudioDevices = serde_json::from_str(&serde_json::to_string(&d).unwrap()).unwrap();
        assert_eq!(back, d);
    }
}
