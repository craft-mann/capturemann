//! Audio output for the editor preview: plays a finished mix (`mix::MixedAudio`, on the output
//! timeline) through the default output device and reports how far it has played, which the
//! preview uses as its master clock (NFR-3: picture follows sound).
//!
//! [`fill`] is the pure resampling and channel-mapping step (unit tested). [`PreviewAudio`] is the
//! `cpal` glue: it opens the device when created and never touches the microphone.
//!
//! The reported position is the frames handed to the device; the device's own output latency
//! (about 10 to 20 ms on macOS built-in output) is not subtracted.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use cm_core::CmError;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, StreamConfig};

use crate::mix::MixedAudio;

/// Writes `out.len() / dev_channels` device frames from `mix` starting at `pos` (a position in
/// mix frames, advanced as frames are played). Linear interpolation resamples `mix_rate` to
/// `dev_rate`; a stereo mix goes to the first two device channels (a mono device gets the
/// average, further channels stay silent). Past the end of the mix the output is silence and
/// `pos` stops at the end.
pub fn fill(
    mix: &[f32],
    mix_channels: usize,
    mix_rate: u32,
    pos: &mut f64,
    out: &mut [f32],
    dev_channels: usize,
    dev_rate: u32,
) {
    let dev_channels = dev_channels.max(1);
    let mix_channels = mix_channels.max(1);
    let frames = mix.len() / mix_channels;
    let step = f64::from(mix_rate) / f64::from(dev_rate.max(1));
    let sample = |frame: usize, ch: usize| mix[frame * mix_channels + ch.min(mix_channels - 1)];
    for dev_frame in out.chunks_mut(dev_channels) {
        let i = pos.max(0.0).floor() as usize;
        if i >= frames {
            dev_frame.fill(0.0);
            *pos = frames as f64;
            continue;
        }
        let frac = (*pos - i as f64).max(0.0) as f32;
        let j = (i + 1).min(frames - 1);
        let lerp = |ch: usize| sample(i, ch) * (1.0 - frac) + sample(j, ch) * frac;
        let (l, r) = (lerp(0), lerp(1));
        match dev_frame.len() {
            1 => dev_frame[0] = 0.5 * (l + r),
            _ => {
                dev_frame.fill(0.0);
                dev_frame[0] = l;
                dev_frame[1] = r;
            }
        }
        *pos += step;
    }
}

struct Shared {
    mix: Mutex<Option<Arc<MixedAudio>>>,
    /// Play position in mix frames (the bits of an `f64`).
    pos: AtomicU64,
    rate: AtomicU32,
    playing: AtomicBool,
}

impl Shared {
    fn pos(&self) -> f64 {
        f64::from_bits(self.pos.load(Ordering::Relaxed))
    }

    fn set_pos(&self, p: f64) {
        self.pos.store(p.to_bits(), Ordering::Relaxed);
    }
}

/// The preview's audio output.
pub struct PreviewAudio {
    shared: Arc<Shared>,
    stream: cpal::Stream,
    device_name: String,
}

fn build<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    shared: Arc<Shared>,
) -> Result<cpal::Stream, cpal::Error>
where
    T: SizedSample + FromSample<f32> + Send + 'static,
{
    let channels = usize::from(config.channels);
    let dev_rate = config.sample_rate;
    let mut scratch: Vec<f32> = Vec::new();
    device.build_output_stream(
        *config,
        move |data: &mut [T], _info: &cpal::OutputCallbackInfo| {
            scratch.clear();
            scratch.resize(data.len(), 0.0);
            if shared.playing.load(Ordering::Relaxed) {
                // Never wait on the control thread: a missed lock is one buffer of silence.
                if let Ok(guard) = shared.mix.try_lock() {
                    if let Some(m) = guard.as_ref() {
                        let mut pos = shared.pos();
                        fill(
                            &m.samples,
                            usize::from(m.channels),
                            m.sample_rate,
                            &mut pos,
                            &mut scratch,
                            channels,
                            dev_rate,
                        );
                        shared.set_pos(pos);
                    }
                }
            }
            for (d, s) in data.iter_mut().zip(&scratch) {
                *d = T::from_sample(*s);
            }
        },
        |_e| {},
        None,
    )
}

impl PreviewAudio {
    /// Opens the default output device. Fails when there is none.
    pub fn open() -> Result<PreviewAudio, CmError> {
        let device = cpal::default_host()
            .default_output_device()
            .ok_or_else(|| CmError::not_found("No audio output device found."))?;
        let name = device
            .description()
            .map(|d| d.name().to_owned())
            .unwrap_or_else(|_| "the default output".to_owned());
        let config = device
            .default_output_config()
            .map_err(|e| crate::map_cpal_error(&e, &name))?;
        let shared = Arc::new(Shared {
            mix: Mutex::new(None),
            pos: AtomicU64::new(0f64.to_bits()),
            rate: AtomicU32::new(48_000),
            playing: AtomicBool::new(false),
        });
        let cfg = config.config();
        let stream = match config.sample_format() {
            SampleFormat::F32 => build::<f32>(&device, &cfg, shared.clone()),
            SampleFormat::I16 => build::<i16>(&device, &cfg, shared.clone()),
            SampleFormat::I32 => build::<i32>(&device, &cfg, shared.clone()),
            other => {
                return Err(CmError::invalid_input(format!(
                    "{name}: unsupported sample format {other:?}"
                )))
            }
        }
        .map_err(|e| crate::map_cpal_error(&e, &name))?;
        Ok(PreviewAudio {
            shared,
            stream,
            device_name: name,
        })
    }

    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    /// Replaces the mix (after an edit). The play position, in output seconds, is unchanged.
    pub fn set_mix(&self, mix: Arc<MixedAudio>) {
        self.shared.rate.store(mix.sample_rate, Ordering::Relaxed);
        *self
            .shared
            .mix
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some(mix);
    }

    pub fn has_mix(&self) -> bool {
        self.shared
            .mix
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_some()
    }

    fn rate(&self) -> f64 {
        f64::from(self.shared.rate.load(Ordering::Relaxed))
    }

    /// Jumps to `secs` of output time.
    pub fn seek(&self, secs: f64) {
        self.shared.set_pos((secs * self.rate()).max(0.0));
    }

    /// Plays from `secs` of output time.
    pub fn play_from(&self, secs: f64) {
        self.seek(secs);
        self.shared.playing.store(true, Ordering::Relaxed);
        let _ = self.stream.play();
    }

    pub fn pause(&self) {
        self.shared.playing.store(false, Ordering::Relaxed);
        let _ = self.stream.pause();
    }

    pub fn is_playing(&self) -> bool {
        self.shared.playing.load(Ordering::Relaxed)
    }

    /// Output seconds played so far; `None` until a mix is set.
    pub fn position(&self) -> Option<f64> {
        self.has_mix().then(|| self.shared.pos() / self.rate())
    }
}

impl Drop for PreviewAudio {
    fn drop(&mut self) {
        self.shared.playing.store(false, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Same rate, stereo to a stereo device: the samples are copied and the position advances.
    #[test]
    fn copies_at_the_same_rate() {
        let mix = [0.1, -0.1, 0.2, -0.2, 0.3, -0.3];
        let mut out = [9.0f32; 4];
        let mut pos = 0.0;
        fill(&mix, 2, 48_000, &mut pos, &mut out, 2, 48_000);
        assert_eq!(out, [0.1, -0.1, 0.2, -0.2]);
        assert_eq!(pos, 2.0);
    }

    /// A 24 kHz mix on a 48 kHz device is interpolated (half a frame per device frame).
    #[test]
    fn upsamples_with_linear_interpolation() {
        let mix = [0.0, 0.0, 1.0, 1.0, 0.0, 0.0];
        let mut out = [0.0f32; 6];
        let mut pos = 0.0;
        fill(&mix, 2, 24_000, &mut pos, &mut out, 2, 48_000);
        assert_eq!(out, [0.0, 0.0, 0.5, 0.5, 1.0, 1.0]);
        assert!((pos - 1.5).abs() < 1e-9);
    }

    /// A mono device gets the average; a surround device gets silence on the other channels.
    #[test]
    fn maps_channels() {
        let mix = [0.2, 0.6, 0.2, 0.6];
        let mut mono = [0.0f32; 2];
        fill(&mix, 2, 48_000, &mut 0.0, &mut mono, 1, 48_000);
        assert_eq!(mono, [0.4, 0.4]);
        let mut six = [1.0f32; 6];
        fill(&mix, 2, 48_000, &mut 0.0, &mut six, 6, 48_000);
        assert_eq!(six, [0.2, 0.6, 0.0, 0.0, 0.0, 0.0]);
        // A mono mix feeds both ears.
        let mut st = [0.0f32; 2];
        fill(&[0.5, 0.5], 1, 48_000, &mut 0.0, &mut st, 2, 48_000);
        assert_eq!(st, [0.5, 0.5]);
    }

    /// Past the end: silence, and the position stays at the end (the clock stops there).
    #[test]
    fn silence_after_the_end() {
        let mix = [0.5, 0.5, 0.5, 0.5];
        let mut out = [1.0f32; 8];
        let mut pos = 0.0;
        fill(&mix, 2, 48_000, &mut pos, &mut out, 2, 48_000);
        assert_eq!(out, [0.5, 0.5, 0.5, 0.5, 0.0, 0.0, 0.0, 0.0]);
        assert_eq!(pos, 2.0);
        fill(&mix, 2, 48_000, &mut pos, &mut out, 2, 48_000);
        assert_eq!(pos, 2.0);
    }

    /// A position before the start (a seek to a negative time) plays from the start.
    #[test]
    fn a_negative_position_is_the_start() {
        let mut out = [0.0f32; 2];
        let mut pos = -5.0;
        fill(&[0.7, 0.7], 2, 48_000, &mut pos, &mut out, 2, 48_000);
        assert_eq!(out, [0.7, 0.7]);
    }
}
