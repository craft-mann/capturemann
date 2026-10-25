//! [`AudioSource`] over `cpal`: microphones on macOS (CoreAudio) and Windows (WASAPI), and WASAPI
//! loopback of the default output device on Windows.

use std::str::FromStr;

use cm_core::{CmError, Permission};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{ErrorKind, FromSample, Sample, SampleFormat, SizedSample, StreamConfig};

use crate::source::{AudioSource, DeviceSelector, SinkNotifier, SourceFormat, SourceSink};

/// Maps a cpal error to the shared error codes (`device_busy`, `permission_missing`, `not_found`).
pub fn map_cpal_error(e: &cpal::Error, what: &str) -> CmError {
    match e.kind() {
        ErrorKind::DeviceBusy => CmError::device_busy(format!(
            "{what} is in use by another application. Close it and try again."
        )),
        ErrorKind::PermissionDenied => CmError::permission_missing(
            Permission::Microphone,
            "Microphone access is off. Turn it on in System Settings > Privacy & Security > Microphone.",
        ),
        ErrorKind::DeviceNotAvailable => {
            CmError::not_found(format!("{what} is not available (unplugged?)"))
        }
        ErrorKind::UnsupportedConfig | ErrorKind::InvalidInput => {
            CmError::invalid_input(format!("{what}: {e}"))
        }
        _ => CmError::internal(format!("{what}: {e}")),
    }
}

fn device_name(d: &cpal::Device) -> String {
    d.description()
        .map(|d| d.name().to_string())
        .unwrap_or_else(|_| "Unknown device".into())
}

fn device_id(d: &cpal::Device) -> String {
    d.id().map(|i| i.to_string()).unwrap_or_default()
}

/// A cpal input (or output-loopback) stream.
pub struct CpalSource {
    loopback: bool,
    device: cpal::Device,
    config: cpal::SupportedStreamConfig,
    format: SourceFormat,
    stream: Option<cpal::Stream>,
}

impl CpalSource {
    /// Opens a microphone. Does not touch permissions: callers check
    /// [`crate::require_microphone`] first so that no prompt appears unexpectedly.
    pub fn open_input(selector: &DeviceSelector) -> Result<Self, CmError> {
        let host = cpal::default_host();
        let device = match selector {
            DeviceSelector::Default => host
                .default_input_device()
                .ok_or_else(|| CmError::not_found("No microphone found."))?,
            DeviceSelector::Id(id) => {
                let parsed = cpal::DeviceId::from_str(id)
                    .map_err(|e| CmError::invalid_field("mic", format!("bad device id: {e}")))?;
                host.device_by_id(&parsed)
                    .filter(|d| d.supports_input())
                    .ok_or_else(|| CmError::not_found(format!("Microphone {id} not found.")))?
            }
        };
        let name = device_name(&device);
        let config = device
            .default_input_config()
            .map_err(|e| map_cpal_error(&e, &name))?;
        Ok(Self::new(device, config, name, false))
    }

    /// Opens the default output device for loopback capture (WASAPI: an input stream built on an
    /// output device records what it plays).
    pub fn open_default_output_loopback() -> Result<Self, CmError> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| CmError::not_found("No audio output device found."))?;
        let name = device_name(&device);
        let config = device
            .default_output_config()
            .map_err(|e| map_cpal_error(&e, &name))?;
        Ok(Self::new(device, config, name, true))
    }

    fn new(
        device: cpal::Device,
        config: cpal::SupportedStreamConfig,
        name: String,
        loopback: bool,
    ) -> Self {
        let format = SourceFormat {
            sample_rate: config.sample_rate(),
            channels: config.channels(),
            device_name: name,
            device_id: device_id(&device),
        };
        CpalSource {
            loopback,
            device,
            config,
            format,
            stream: None,
        }
    }
}

fn on_error(e: cpal::Error, notifier: &SinkNotifier) {
    match e.kind() {
        ErrorKind::DeviceNotAvailable
        | ErrorKind::StreamInvalidated
        | ErrorKind::HostUnavailable => {
            notifier.disconnected(e.to_string());
        }
        // Xruns and route notices do not end the stream; timestamps absorb any glitch.
        _ => {}
    }
}

fn build<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    mut sink: SourceSink,
) -> Result<cpal::Stream, cpal::Error>
where
    T: SizedSample + Send + 'static,
    f32: FromSample<T>,
{
    let notifier = sink.notifier();
    let mut scratch: Vec<f32> = Vec::with_capacity(8192);
    device.build_input_stream(
        *config,
        move |data: &[T], info: &cpal::InputCallbackInfo| {
            scratch.clear();
            scratch.extend(data.iter().map(|&s| f32::from_sample(s)));
            let ns = u64::try_from(info.timestamp().capture.as_nanos()).unwrap_or(0);
            sink.push(ns, &scratch);
        },
        move |e| on_error(e, &notifier),
        None,
    )
}

impl AudioSource for CpalSource {
    fn format(&self) -> SourceFormat {
        self.format.clone()
    }

    fn start(&mut self, sink: SourceSink) -> Result<(), CmError> {
        let cfg = self.config.config();
        let name = self.format.device_name.clone();
        let stream = match self.config.sample_format() {
            SampleFormat::F32 => build::<f32>(&self.device, &cfg, sink),
            SampleFormat::I16 => build::<i16>(&self.device, &cfg, sink),
            SampleFormat::I32 => build::<i32>(&self.device, &cfg, sink),
            SampleFormat::U8 => build::<u8>(&self.device, &cfg, sink),
            other => {
                return Err(CmError::invalid_input(format!(
                    "{name}: unsupported sample format {other:?}"
                )))
            }
        }
        .map_err(|e| map_cpal_error(&e, &name))?;
        stream.play().map_err(|e| map_cpal_error(&e, &name))?;
        self.stream = Some(stream);
        Ok(())
    }

    fn stop(&mut self) {
        // Dropping the stream stops the callbacks.
        self.stream = None;
    }

    fn needs_reopen(&mut self) -> bool {
        // WASAPI does not report a default-output change on a loopback stream: compare ids.
        if !self.loopback {
            return false;
        }
        let current = cpal::default_host()
            .default_output_device()
            .and_then(|d| d.id().ok())
            .map(|i| i.to_string());
        current.is_some_and(|id| id != self.format.device_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cm_core::ErrorCode;

    #[test]
    fn cpal_errors_map_to_shared_codes() {
        let e = |k| map_cpal_error(&cpal::Error::new(k), "USB Mic").code();
        assert_eq!(e(ErrorKind::DeviceBusy), ErrorCode::DeviceBusy);
        assert_eq!(e(ErrorKind::PermissionDenied), ErrorCode::PermissionMissing);
        assert_eq!(e(ErrorKind::DeviceNotAvailable), ErrorCode::NotFound);
        assert_eq!(e(ErrorKind::UnsupportedConfig), ErrorCode::InvalidInput);
        assert_eq!(e(ErrorKind::BackendError), ErrorCode::Internal);
    }

    #[test]
    fn busy_message_names_the_device() {
        let m = map_cpal_error(&cpal::Error::new(ErrorKind::DeviceBusy), "USB Mic").to_string();
        assert!(m.contains("USB Mic") && m.contains("in use"));
    }

    #[test]
    fn a_bad_device_id_is_invalid_input_and_an_unknown_one_not_found() {
        let bad = CpalSource::open_input(&DeviceSelector::Id("nonsense".into()));
        assert!(matches!(bad, Err(CmError::InvalidInput { .. })));
        // Opening by id never starts a stream, so this cannot trigger a permission prompt.
        let id = format!("{}:does-not-exist", cpal::default_host().id());
        let missing = CpalSource::open_input(&DeviceSelector::Id(id));
        assert!(
            matches!(missing, Err(CmError::NotFound { .. })),
            "{:?}",
            missing.err()
        );
    }
}
