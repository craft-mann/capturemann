//! The worker-thread recorder behind both [`crate::MicRecorder`] and [`crate::SystemAudioRecorder`].
//!
//! A worker thread owns the [`AudioSource`] (so OS stream objects never need to be `Send`) and the
//! [`Pipeline`]. The OS audio callback only meters and enqueues; control calls from the session
//! (`pause`, `resume`, `stop`) travel the same queue, so they are ordered against audio. If the
//! source reports that its device went away, the worker opens the system default device, tells the
//! pipeline (which fills the gap with silence on the timestamp axis and reports it), and records an
//! [`AudioEvent::DeviceChanged`] for the UI toast.

use std::sync::mpsc::{channel, sync_channel, Receiver, RecvTimeoutError, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use cm_core::{CmError, MediaTime, TrackContext, TrackInfo, TrackKind, TrackStats};
use serde::{Deserialize, Serialize};

use crate::level::{LevelHandle, LevelMeter, DEFAULT_METER_HZ};
use crate::mute::{MuteChange, MuteControl};
use crate::pipeline::{Pipeline, PipelineConfig};
use crate::source::{
    AudioSource, Counters, DeviceSelector, Msg, SourceFactory, SourceFormat, SourceSink,
    QUEUE_CAPACITY, START_TIMEOUT,
};

/// How often the worker retries opening the default device while none is available.
const RECONNECT_INTERVAL: Duration = Duration::from_millis(250);

/// Something the UI should know about (toasts, `session.json`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AudioEvent {
    /// The requested device was not available at start; the default is used instead.
    DeviceFallback { requested: String, using: String },
    /// The device went away mid-recording and capture continues on `to` (toast "Mic changed to <to>").
    DeviceChanged {
        at: MediaTime,
        from: String,
        to: String,
    },
    /// The device went away and no replacement is available yet; the track records silence.
    DeviceLost { at: MediaTime, name: String },
    /// The track can no longer be written (disk full, I/O error): it ends where its file ends
    /// and the rest of the recording continues (R3-1, R3-21).
    WriteFailed { at: MediaTime, message: String },
    /// The microphone has delivered only digital silence for a long while (muted, or its access
    /// was revoked while the stream stayed open). Recording continues (R3-21).
    DigitalSilence { at: MediaTime },
}

/// Settings common to the audio recorders.
#[derive(Debug, Clone)]
pub struct RecorderSettings {
    pub kind: TrackKind,
    pub device: DeviceSelector,
    pub raw_file: String,
    pub denoised_file: Option<String>,
    /// `None`: the device's channel count capped at 2.
    pub force_channels: Option<u16>,
    pub resync_ms: f64,
    pub stamp_offset_ms: f64,
}

struct Worker {
    tx: SyncSender<Msg>,
    join: JoinHandle<Result<TrackInfo, CmError>>,
}

/// See the module docs.
pub struct AudioRecorder {
    settings: RecorderSettings,
    factory: Option<SourceFactory>,
    counters: Arc<Counters>,
    level: LevelHandle,
    events: Arc<Mutex<Vec<AudioEvent>>>,
    format: Arc<Mutex<Option<SourceFormat>>>,
    worker: Option<Worker>,
    start_timeout: Duration,
    fault_after: Option<u64>,
    hint: Arc<Mutex<Option<serde_json::Value>>>,
    mute: Arc<MuteControl>,
    clock: Option<cm_core::SessionClock>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

impl AudioRecorder {
    pub fn new(settings: RecorderSettings, factory: SourceFactory) -> Self {
        AudioRecorder {
            settings,
            factory: Some(factory),
            counters: Arc::new(Counters::default()),
            level: LevelHandle::new_silent(),
            events: Arc::new(Mutex::new(Vec::new())),
            format: Arc::new(Mutex::new(None)),
            worker: None,
            start_timeout: START_TIMEOUT,
            fault_after: None,
            hint: Arc::new(Mutex::new(None)),
            mute: Arc::new(MuteControl::new()),
            clock: None,
        }
    }

    /// Mutes or unmutes (AUD-6). The track keeps running; frames captured while muted are
    /// written as digital silence to the raw and the denoised file (see [`crate::mute`] for the
    /// boundary rule). Idempotent: `changed` is `false` when the state already was `muted`.
    /// Needs a started recorder.
    pub fn set_muted(&self, muted: bool) -> Result<MuteChange, CmError> {
        self.worker()?;
        let clock = self
            .clock
            .as_ref()
            .ok_or_else(|| CmError::invalid_input("audio recorder is not running"))?;
        Ok(self.mute.set(muted, || {
            let at = clock.now();
            (at, i128::from(clock.to_host_ns(at)))
        }))
    }

    /// Whether the recorder is muted now.
    pub fn is_muted(&self) -> bool {
        self.mute.is_muted()
    }

    /// Test hook: the raw file fails (disk full) after `bytes` of audio.
    #[doc(hidden)]
    #[must_use]
    pub fn with_write_fault_after(mut self, bytes: u64) -> Self {
        self.fault_after = Some(bytes);
        self
    }

    /// How long `start` waits for the device to open (tests use a short one).
    #[must_use]
    pub fn with_start_timeout(mut self, t: Duration) -> Self {
        self.start_timeout = t;
        self
    }

    pub fn kind(&self) -> TrackKind {
        self.settings.kind
    }

    /// Lock-free meter handle; valid before `start` and across device changes.
    pub fn level_handle(&self) -> LevelHandle {
        self.level.clone()
    }

    /// Events since the last call.
    pub fn drain_events(&self) -> Vec<AudioEvent> {
        std::mem::take(&mut *lock(&self.events))
    }

    /// What crash recovery needs to place this track: first-sample session time, file names,
    /// channels and rate. `None` until the first sample was placed (R3-2).
    pub fn recovery_hint(&self) -> Option<serde_json::Value> {
        lock(&self.hint).clone()
    }

    /// Format of the device in use (after `start`).
    pub fn device_format(&self) -> Option<SourceFormat> {
        lock(&self.format).clone()
    }

    pub fn stats(&self) -> TrackStats {
        self.counters.snapshot()
    }

    /// Blocks until everything queued so far has been processed (tests, orderly shutdown).
    pub fn sync(&self) -> Result<(), CmError> {
        let w = self.worker()?;
        let (tx, rx) = channel();
        w.tx.send(Msg::Barrier(tx))
            .map_err(|_| CmError::internal("audio worker stopped"))?;
        rx.recv_timeout(Duration::from_secs(30))
            .map_err(|_| CmError::internal("audio worker did not respond"))
    }

    fn worker(&self) -> Result<&Worker, CmError> {
        self.worker
            .as_ref()
            .ok_or_else(|| CmError::invalid_input("audio recorder is not running"))
    }

    pub fn start(&mut self, ctx: TrackContext) -> Result<(), CmError> {
        if self.worker.is_some() {
            return Err(CmError::invalid_input("audio recorder already started"));
        }
        let factory = self
            .factory
            .take()
            .ok_or_else(|| CmError::invalid_input("audio recorder already used"))?;
        let (tx, rx) = sync_channel::<Msg>(QUEUE_CAPACITY);
        let (ready_tx, ready_rx) = channel::<Result<(), CmError>>();
        let leftovers: Vec<std::path::PathBuf> = std::iter::once(&self.settings.raw_file)
            .chain(self.settings.denoised_file.iter())
            .map(|f| ctx.dir.join(f))
            .collect();
        self.clock = Some(ctx.clock.clone());
        let args = WorkerArgs {
            rx,
            tx: tx.clone(),
            factory,
            settings: self.settings.clone(),
            ctx,
            counters: self.counters.clone(),
            level: self.level.clone(),
            events: self.events.clone(),
            format: self.format.clone(),
            fault_after: self.fault_after,
            hint: self.hint.clone(),
            mute: self.mute.clone(),
        };
        let join = std::thread::Builder::new()
            .name(format!("cm-audio-{}", self.settings.kind.as_str()))
            .spawn(move || run_worker(args, ready_tx))
            .map_err(|e| CmError::internal(format!("cannot start audio thread: {e}")))?;
        match ready_rx.recv_timeout(self.start_timeout) {
            Ok(Ok(())) => {
                self.worker = Some(Worker { tx, join });
                Ok(())
            }
            Ok(Err(e)) => {
                let _ = join.join();
                Err(e)
            }
            Err(_) => {
                // R3-13: the worker holds its own sender, so it would never exit: stop it, and
                // delete the partial files once it has (the device may open late). The
                // microphone must not stay open and keep writing after a failed start.
                std::thread::spawn(move || {
                    let _ = tx.send(Msg::Stop(MediaTime::from_ticks(i64::MAX / 4)));
                    let _ = join.join();
                    for f in leftovers {
                        let _ = std::fs::remove_file(f);
                    }
                });
                Err(CmError::internal("timed out opening the audio device"))
            }
        }
    }

    fn send(&self, msg: Msg) -> Result<(), CmError> {
        self.worker()?
            .tx
            .send(msg)
            .map_err(|_| CmError::internal("audio worker stopped"))
    }

    pub fn pause(&mut self, at: MediaTime) -> Result<(), CmError> {
        self.send(Msg::Pause(at))
    }

    pub fn resume(&mut self, at: MediaTime) -> Result<(), CmError> {
        self.send(Msg::Resume(at))
    }

    pub fn stop(&mut self, at: MediaTime) -> Result<TrackInfo, CmError> {
        let w = self
            .worker
            .take()
            .ok_or_else(|| CmError::invalid_input("audio recorder is not running"))?;
        // The worker may already have exited on an internal error; join reports it.
        let _ = w.tx.send(Msg::Stop(at));
        w.join
            .join()
            .map_err(|_| CmError::internal("audio worker panicked"))?
    }
}

impl Drop for AudioRecorder {
    fn drop(&mut self) {
        if let Some(w) = self.worker.take() {
            // Dropped without stop: finish the files so they stay readable.
            let _ = w.tx.send(Msg::Stop(MediaTime::from_ticks(i64::MAX / 4)));
            let _ = w.join.join();
        }
    }
}

struct WorkerArgs {
    rx: Receiver<Msg>,
    tx: SyncSender<Msg>,
    factory: SourceFactory,
    settings: RecorderSettings,
    ctx: TrackContext,
    counters: Arc<Counters>,
    level: LevelHandle,
    events: Arc<Mutex<Vec<AudioEvent>>>,
    format: Arc<Mutex<Option<SourceFormat>>>,
    fault_after: Option<u64>,
    hint: Arc<Mutex<Option<serde_json::Value>>>,
    mute: Arc<MuteControl>,
}

struct Live {
    source: Option<Box<dyn AudioSource>>,
    generation: u64,
}

fn open_and_start(
    factory: &mut SourceFactory,
    selector: &DeviceSelector,
    tx: &SyncSender<Msg>,
    counters: &Arc<Counters>,
    level: &LevelHandle,
    generation: u64,
) -> Result<(Box<dyn AudioSource>, SourceFormat), CmError> {
    let mut source = factory(selector)?;
    let format = source.format();
    let meter = LevelMeter::attached(level, format.sample_rate, format.channels, DEFAULT_METER_HZ);
    let sink = SourceSink::new(
        &format,
        Some(tx.clone()),
        counters.clone(),
        meter,
        generation,
    );
    source.start(sink)?;
    Ok((source, format))
}

fn run_worker(
    mut a: WorkerArgs,
    ready: std::sync::mpsc::Sender<Result<(), CmError>>,
) -> Result<TrackInfo, CmError> {
    let mut live = Live {
        source: None,
        generation: 1,
    };
    // Open the requested device; a missing specific device falls back to the default (AUD-1).
    let opened = match open_and_start(
        &mut a.factory,
        &a.settings.device,
        &a.tx,
        &a.counters,
        &a.level,
        live.generation,
    ) {
        Err(CmError::NotFound { .. }) if matches!(a.settings.device, DeviceSelector::Id(_)) => {
            open_and_start(
                &mut a.factory,
                &DeviceSelector::Default,
                &a.tx,
                &a.counters,
                &a.level,
                live.generation,
            )
            .inspect(|(_, f)| {
                if let DeviceSelector::Id(id) = &a.settings.device {
                    lock(&a.events).push(AudioEvent::DeviceFallback {
                        requested: id.clone(),
                        using: f.device_name.clone(),
                    });
                }
            })
        }
        other => other,
    };
    let (source, format) = match opened {
        Ok(v) => v,
        Err(e) => {
            let _ = ready.send(Err(e.clone()));
            return Err(e);
        }
    };
    live.source = Some(source);
    *lock(&a.format) = Some(format.clone());

    let out_channels = a
        .settings
        .force_channels
        .unwrap_or_else(|| format.channels.min(2));
    let mut pipeline = match Pipeline::new(
        PipelineConfig {
            dir: a.ctx.dir.clone(),
            clock: a.ctx.clock.clone(),
            kind: a.settings.kind,
            raw_file: a.settings.raw_file.clone(),
            denoised_file: a.settings.denoised_file.clone(),
            out_channels,
            resync_ms: a.settings.resync_ms,
            stamp_offset_ms: a.settings.stamp_offset_ms,
        },
        &format,
        a.counters.clone(),
    ) {
        Ok(p) => p,
        Err(e) => {
            if let Some(mut s) = live.source.take() {
                s.stop();
            }
            let _ = ready.send(Err(e.clone()));
            return Err(e);
        }
    };
    pipeline.set_hint_cell(a.hint.clone());
    pipeline.set_mute_control(a.mute.clone());
    if let Some(bytes) = a.fault_after {
        pipeline.inject_write_fault_after(bytes);
    }
    let _ = ready.send(Ok(()));

    let mut no_device = false;
    let mut stop_at = None;
    let mut failure_reported = false;
    loop {
        match a.rx.recv_timeout(RECONNECT_INTERVAL) {
            Ok(Msg::Chunk(c)) => {
                pipeline.push_chunk(c);
                if pipeline.take_silence_alert() {
                    lock(&a.events).push(AudioEvent::DigitalSilence {
                        at: a.ctx.clock.now(),
                    });
                }
                if !failure_reported {
                    if let Some(e) = pipeline.first_error() {
                        failure_reported = true;
                        lock(&a.events).push(AudioEvent::WriteFailed {
                            at: a.ctx.clock.now(),
                            message: e.to_string(),
                        });
                    }
                }
            }
            Ok(Msg::Pause(t)) => pipeline.pause(t),
            Ok(Msg::Resume(t)) => pipeline.resume(t),
            Ok(Msg::Barrier(ack)) => {
                let _ = ack.send(());
            }
            Ok(Msg::Stop(t)) => {
                stop_at = Some(t);
                break;
            }
            Ok(Msg::Disconnected(generation, _reason)) => {
                if generation != live.generation || no_device {
                    continue;
                }
                lose_and_reconnect(&mut a, &mut live, &mut pipeline, &mut no_device, true);
            }
            Err(RecvTimeoutError::Timeout) => {
                if no_device {
                    try_reconnect(&mut a, &mut live, &mut pipeline, &mut no_device);
                } else if live.source.as_mut().is_some_and(|s| s.needs_reopen()) {
                    lose_and_reconnect(&mut a, &mut live, &mut pipeline, &mut no_device, false);
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }

    let at = stop_at.unwrap_or_else(|| a.ctx.clock.now());
    pipeline.set_stop(at);
    if let Some(mut s) = live.source.take() {
        s.stop();
    }
    // Chunks that raced the stop message.
    while let Ok(m) = a.rx.try_recv() {
        if let Msg::Chunk(c) = m {
            pipeline.push_chunk(c);
        }
    }
    pipeline.finish(at)
}

/// Closes the current source and opens the default device. `announce_loss` adds a `DeviceLost`
/// event first (unplug); a deliberate switch to a new default does not.
fn lose_and_reconnect(
    a: &mut WorkerArgs,
    live: &mut Live,
    pipeline: &mut Pipeline,
    no_device: &mut bool,
    announce_loss: bool,
) {
    let name = lock(&a.format)
        .as_ref()
        .map(|f| f.device_name.clone())
        .unwrap_or_default();
    if let Some(mut s) = live.source.take() {
        s.stop();
    }
    *no_device = true;
    if announce_loss {
        lock(&a.events).push(AudioEvent::DeviceLost {
            at: a.ctx.clock.now(),
            name,
        });
    }
    try_reconnect(a, live, pipeline, no_device);
}

fn try_reconnect(
    a: &mut WorkerArgs,
    live: &mut Live,
    pipeline: &mut Pipeline,
    no_device: &mut bool,
) {
    let generation = live.generation + 1;
    let Ok((source, format)) = open_and_start(
        &mut a.factory,
        &DeviceSelector::Default,
        &a.tx,
        &a.counters,
        &a.level,
        generation,
    ) else {
        return;
    };
    let from = lock(&a.format)
        .as_ref()
        .map(|f| f.device_name.clone())
        .unwrap_or_default();
    if pipeline.set_input(&format).is_err() {
        let mut source = source;
        source.stop();
        return;
    }
    live.source = Some(source);
    live.generation = generation;
    *no_device = false;
    lock(&a.events).push(AudioEvent::DeviceChanged {
        at: a.ctx.clock.now(),
        from,
        to: format.device_name.clone(),
    });
    *lock(&a.format) = Some(format);
}
