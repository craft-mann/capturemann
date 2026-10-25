//! The seam between an audio engine (cpal, ScreenCaptureKit, WASAPI, a test fake) and the recorder.
//!
//! An [`AudioSource`] delivers [`AudioChunk`]s stamped with the OS capture time through a
//! [`SourceSink`]. The sink computes the live level meter on the audio thread (a few arithmetic
//! operations per sample, no locks) and hands the chunk to the recorder's worker thread over a
//! bounded queue. If the worker falls behind, chunks are dropped and counted; the resulting gap in
//! timestamps is filled with silence, so sync is kept.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{SyncSender, TrySendError};
use std::sync::Arc;
use std::time::Duration;

use cm_core::{CmError, MediaTime};

use crate::level::{LevelHandle, LevelMeter, DEFAULT_METER_HZ};

/// Capacity of the callback-to-worker queue (about 2.5 s of 10 ms buffers).
pub const QUEUE_CAPACITY: usize = 256;

/// Which input a recorder should open.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum DeviceSelector {
    /// The system default input (or default output for loopback).
    #[default]
    Default,
    /// A device id from [`crate::list_audio_devices`].
    Id(String),
}

impl DeviceSelector {
    /// `"default"` or a device id (the `mic` argument of `start_recording`).
    pub fn parse(s: &str) -> Self {
        if s.is_empty() || s.eq_ignore_ascii_case("default") {
            DeviceSelector::Default
        } else {
            DeviceSelector::Id(s.to_string())
        }
    }
}

/// What a source produces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFormat {
    pub sample_rate: u32,
    pub channels: u16,
    pub device_name: String,
    pub device_id: String,
}

/// Interleaved f32 audio and the OS capture time of its first frame.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioChunk {
    /// Host nanoseconds on the capture timebase (see `cm_core::HostClock`).
    pub host_ns: u64,
    pub channels: u16,
    pub data: Vec<f32>,
}

impl AudioChunk {
    pub fn frames(&self) -> usize {
        self.data.len() / usize::from(self.channels.max(1))
    }
}

/// Messages on the worker queue. Control messages share the queue with audio so they are ordered
/// against it.
pub(crate) enum Msg {
    Chunk(AudioChunk),
    /// (source generation, reason). A stale generation is ignored.
    Disconnected(u64, String),
    Pause(MediaTime),
    Resume(MediaTime),
    Stop(MediaTime),
    /// Replies once everything queued before it has been processed.
    Barrier(std::sync::mpsc::Sender<()>),
}

/// Counters shared between the audio thread, the worker and `TrackRecorder::stats`.
#[derive(Debug, Default)]
pub struct Counters {
    pub delivered: AtomicU64,
    pub written: AtomicU64,
    pub dropped: AtomicU64,
    pub late: AtomicU64,
}

impl Counters {
    pub fn snapshot(&self) -> cm_core::TrackStats {
        cm_core::TrackStats {
            delivered: self.delivered.load(Ordering::Relaxed),
            written: self.written.load(Ordering::Relaxed),
            dropped: self.dropped.load(Ordering::Relaxed),
            late: self.late.load(Ordering::Relaxed),
        }
    }
}

/// Reports stream errors from any thread.
#[derive(Clone)]
pub struct SinkNotifier {
    tx: Option<SyncSender<Msg>>,
    generation: u64,
}

impl SinkNotifier {
    /// The device went away (unplugged, stream invalidated). The recorder switches to the default
    /// device.
    pub fn disconnected(&self, reason: impl Into<String>) {
        if let Some(tx) = &self.tx {
            // Never block an OS callback: a full queue means the worker is busy and will see the
            // gap in timestamps anyway; try again once.
            let _ = tx.try_send(Msg::Disconnected(self.generation, reason.into()));
        }
    }
}

/// The audio-thread end of a source. Not `Clone`: the data callback owns it.
pub struct SourceSink {
    tx: Option<SyncSender<Msg>>,
    meter: LevelMeter,
    counters: Arc<Counters>,
    channels: u16,
    generation: u64,
}

impl SourceSink {
    /// A sink feeding the recorder queue (or only the meter when `tx` is `None`).
    pub(crate) fn new(
        format: &SourceFormat,
        tx: Option<SyncSender<Msg>>,
        counters: Arc<Counters>,
        meter: LevelMeter,
        generation: u64,
    ) -> Self {
        SourceSink {
            tx,
            meter,
            counters,
            channels: format.channels,
            generation,
        }
    }

    /// A sink that only drives a meter (the setup-panel monitor, tests). Returns the sink and the
    /// read handle.
    pub fn meter_only(format: &SourceFormat) -> (SourceSink, LevelHandle) {
        let (meter, handle) =
            LevelMeter::new(format.sample_rate, format.channels, DEFAULT_METER_HZ);
        (
            SourceSink::new(format, None, Arc::new(Counters::default()), meter, 0),
            handle,
        )
    }

    pub fn notifier(&self) -> SinkNotifier {
        SinkNotifier {
            tx: self.tx.clone(),
            generation: self.generation,
        }
    }

    /// Called from the audio callback with one buffer. Returns false if the chunk was dropped
    /// because the worker queue is full.
    pub fn push(&mut self, host_ns: u64, interleaved: &[f32]) -> bool {
        self.meter.push(interleaved);
        self.counters.delivered.fetch_add(1, Ordering::Relaxed);
        let Some(tx) = &self.tx else {
            return true;
        };
        let chunk = AudioChunk {
            host_ns,
            channels: self.channels,
            data: interleaved.to_vec(),
        };
        match tx.try_send(Msg::Chunk(chunk)) {
            Ok(()) => true,
            Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {
                self.counters.dropped.fetch_add(1, Ordering::Relaxed);
                false
            }
        }
    }

    /// For non-real-time sources (files, test fakes): waits for queue space instead of dropping.
    pub fn push_blocking(&mut self, host_ns: u64, interleaved: &[f32]) {
        self.meter.push(interleaved);
        self.counters.delivered.fetch_add(1, Ordering::Relaxed);
        let Some(tx) = &self.tx else { return };
        let chunk = AudioChunk {
            host_ns,
            channels: self.channels,
            data: interleaved.to_vec(),
        };
        let _ = tx.send(Msg::Chunk(chunk));
    }

    /// Shows silence on the meter.
    pub fn meter_silence(&mut self) {
        self.meter.silence();
    }
}

/// An audio engine. Created and driven on the recorder's worker thread (so it need not be `Send`).
pub trait AudioSource {
    /// The format chunks will have. Valid before `start`.
    fn format(&self) -> SourceFormat;
    /// Starts delivering audio into `sink`.
    fn start(&mut self, sink: SourceSink) -> Result<(), CmError>;
    /// Stops delivery; no more chunks after this returns.
    fn stop(&mut self);
    /// Polled by the worker about 4 times a second: true when the OS default device changed and the
    /// recorder should reopen the default (WASAPI loopback does not report this as an error).
    fn needs_reopen(&mut self) -> bool {
        false
    }
}

/// Opens sources. Called on the worker thread at start and again to follow a default-device change.
pub type SourceFactory =
    Box<dyn FnMut(&DeviceSelector) -> Result<Box<dyn AudioSource>, CmError> + Send>;

/// How long `start` waits for the worker to open the device before giving up.
pub(crate) const START_TIMEOUT: Duration = Duration::from_secs(15);

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::sync_channel;

    fn fmt() -> SourceFormat {
        SourceFormat {
            sample_rate: 48_000,
            channels: 1,
            device_name: "t".into(),
            device_id: "t".into(),
        }
    }

    #[test]
    fn selector_parses_default_and_ids() {
        assert_eq!(DeviceSelector::parse("default"), DeviceSelector::Default);
        assert_eq!(DeviceSelector::parse(""), DeviceSelector::Default);
        assert_eq!(
            DeviceSelector::parse("coreaudio:abc"),
            DeviceSelector::Id("coreaudio:abc".into())
        );
    }

    #[test]
    fn full_queue_drops_and_counts() {
        let (tx, _rx) = sync_channel(2);
        let (meter, _h) = LevelMeter::new(48_000, 1, 30.0);
        let counters = Arc::new(Counters::default());
        let mut sink = SourceSink::new(&fmt(), Some(tx), counters.clone(), meter, 0);
        assert!(sink.push(0, &[0.0; 10]));
        assert!(sink.push(1, &[0.0; 10]));
        assert!(!sink.push(2, &[0.0; 10]));
        let s = counters.snapshot();
        assert_eq!((s.delivered, s.dropped), (3, 1));
    }

    #[test]
    fn meter_only_sink_updates_the_meter() {
        let (mut sink, h) = SourceSink::meter_only(&fmt());
        sink.push(0, &vec![0.5; 1600]);
        assert!((h.read().peak_dbfs + 6.02).abs() < 0.05);
    }
}
