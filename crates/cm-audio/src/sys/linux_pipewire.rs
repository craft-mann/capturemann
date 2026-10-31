//! Linux audio capture over PipeWire (see `specs/plans/linux-recording-plan.md`, section 2):
//! the microphone (the default source) and system audio (the monitor of the default sink,
//! `stream.capture.sink = true`), as an [`AudioSource`] with the same contract as `CpalSource`.
//!
//! The pure helpers (timestamping, sample decoding) are tested on every OS; the PipeWire part only
//! exists on Linux, elsewhere the factories answer `not_implemented`.

use cm_core::CmError;

use crate::source::{AudioSource, DeviceSelector, SourceFactory};

/// Sample rate the stream asks PipeWire for (the graph resamples; same as the track rate).
pub const STREAM_RATE: u32 = 48_000;

/// Capture time of the first frame of a buffer that was delivered at `now_ns` and holds `frames`
/// frames: the buffer ends about now, so it began `frames / rate` earlier. The graph's own latency
/// (one quantum, typically 21 ms) is not subtracted; the recorder's `stamp_offset_ms` is the place
/// for a measured constant.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn chunk_start_ns(now_ns: u64, frames: usize, rate: u32) -> u64 {
    let dur = (frames as u128 * 1_000_000_000 / u128::from(rate.max(1))) as u64;
    now_ns.saturating_sub(dur)
}

/// Little-endian f32 bytes to samples (PipeWire hands over the mapped buffer as bytes).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn decode_f32le(bytes: &[u8], out: &mut Vec<f32>) {
    out.clear();
    let (words, _) = bytes.as_chunks::<4>();
    out.extend(words.iter().map(|b| f32::from_le_bytes(*b)));
}

/// Opens the microphone (default source) through PipeWire.
pub fn mic_factory() -> SourceFactory {
    Box::new(|sel: &DeviceSelector| open(sel, false))
}

/// Opens system audio: the monitor of the default sink.
pub fn monitor_factory() -> SourceFactory {
    Box::new(|sel: &DeviceSelector| open(sel, true))
}

/// Is PipeWire usable here? (Linux with a reachable daemon.)
pub fn available() -> bool {
    imp::available()
}

fn open(sel: &DeviceSelector, monitor: bool) -> Result<Box<dyn AudioSource>, CmError> {
    imp::open(sel, monitor)
}

#[cfg(not(target_os = "linux"))]
mod imp {
    use super::*;

    pub fn available() -> bool {
        false
    }

    pub fn open(_: &DeviceSelector, _: bool) -> Result<Box<dyn AudioSource>, CmError> {
        Err(CmError::not_implemented(
            "PipeWire audio capture exists on Linux only.",
        ))
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::{channel, RecvTimeoutError};
    use std::sync::Arc;
    use std::thread::JoinHandle;
    use std::time::Duration;

    use cm_core::{CmError, HostClock, SystemHostClock};
    use pipewire as pw;
    use pw::spa;

    use super::{chunk_start_ns, decode_f32le, STREAM_RATE};
    use crate::source::{AudioSource, DeviceSelector, SourceFormat, SourceSink};

    pub fn available() -> bool {
        // The daemon's socket decides; connecting is the only honest test.
        let dir = std::env::var_os("XDG_RUNTIME_DIR");
        dir.is_some_and(|d| std::path::Path::new(&d).join("pipewire-0").exists())
    }

    pub fn open(sel: &DeviceSelector, monitor: bool) -> Result<Box<dyn AudioSource>, CmError> {
        if !available() {
            return Err(CmError::not_found(
                "PipeWire is not running (no pipewire-0 socket in XDG_RUNTIME_DIR).",
            ));
        }
        let target = match sel {
            DeviceSelector::Default => None,
            DeviceSelector::Id(id) => Some(id.clone()),
        };
        let channels: u16 = if monitor { 2 } else { 1 };
        Ok(Box::new(PipeWireAudioSource {
            monitor,
            target,
            format: SourceFormat {
                sample_rate: STREAM_RATE,
                channels,
                device_name: if monitor {
                    "System audio (default output monitor)".into()
                } else {
                    "Default microphone".into()
                },
                device_id: if monitor {
                    "default-monitor"
                } else {
                    "default"
                }
                .into(),
            },
            stop: Arc::new(AtomicBool::new(false)),
            thread: None,
        }))
    }

    /// A PipeWire capture stream.
    pub struct PipeWireAudioSource {
        monitor: bool,
        target: Option<String>,
        format: SourceFormat,
        stop: Arc<AtomicBool>,
        thread: Option<JoinHandle<()>>,
    }

    fn run_loop(
        monitor: bool,
        target: Option<String>,
        channels: u16,
        mut sink: SourceSink,
        stop: Arc<AtomicBool>,
        ready: std::sync::mpsc::Sender<Result<(), CmError>>,
    ) {
        let fail = |m: String| CmError::internal(format!("PipeWire audio: {m}"));
        pw::init();
        let setup = || -> Result<_, CmError> {
            let mainloop = pw::main_loop::MainLoopRc::new(None).map_err(|e| fail(e.to_string()))?;
            let context =
                pw::context::ContextRc::new(&mainloop, None).map_err(|e| fail(e.to_string()))?;
            let core = context
                .connect_rc(None)
                .map_err(|e| fail(format!("cannot reach the PipeWire daemon: {e}")))?;
            Ok((mainloop, context, core))
        };
        let (mainloop, _context, core) = match setup() {
            Ok(v) => v,
            Err(e) => {
                let _ = ready.send(Err(e));
                return;
            }
        };
        let mut props = pw::properties::properties! {
            *pw::keys::MEDIA_TYPE => "Audio",
            *pw::keys::MEDIA_CATEGORY => "Capture",
            *pw::keys::MEDIA_ROLE => "Production",
        };
        if monitor {
            props.insert(*pw::keys::STREAM_CAPTURE_SINK, "true");
        }
        if let Some(t) = &target {
            props.insert("target.object", t.as_str());
        }
        let stream = match pw::stream::StreamBox::new(&core, "capturemann-audio", props) {
            Ok(s) => s,
            Err(e) => {
                let _ = ready.send(Err(fail(e.to_string())));
                return;
            }
        };
        let notifier = sink.notifier();
        let clock = SystemHostClock;
        let mut scratch: Vec<f32> = Vec::with_capacity(8192);
        let ml_state = mainloop.clone();
        let ready_state = ready.clone();
        let _listener = stream
            .add_local_listener_with_user_data(())
            .state_changed(move |_, _, _old, new| match new {
                pw::stream::StreamState::Streaming => {
                    let _ = ready_state.send(Ok(()));
                }
                pw::stream::StreamState::Error(m) => {
                    let _ = ready_state.send(Err(fail(m.clone())));
                    notifier.disconnected(m);
                    ml_state.quit();
                }
                _ => {}
            })
            .process(move |stream, _| {
                let Some(mut buffer) = stream.dequeue_buffer() else {
                    return;
                };
                let now = clock.now_ns();
                let Some(d) = buffer.datas_mut().first_mut() else {
                    return;
                };
                let size = d.chunk().size() as usize;
                let Some(bytes) = d.data() else { return };
                decode_f32le(&bytes[..size.min(bytes.len())], &mut scratch);
                let frames = scratch.len() / usize::from(channels.max(1));
                if frames > 0 {
                    sink.push(chunk_start_ns(now, frames, STREAM_RATE), &scratch);
                }
            })
            .register();
        let _listener = match _listener {
            Ok(l) => l,
            Err(e) => {
                let _ = ready.send(Err(fail(e.to_string())));
                return;
            }
        };
        let mut info = spa::param::audio::AudioInfoRaw::new();
        info.set_format(spa::param::audio::AudioFormat::F32LE);
        info.set_rate(STREAM_RATE);
        info.set_channels(u32::from(channels));
        let obj = spa::pod::Object {
            type_: spa::sys::SPA_TYPE_OBJECT_Format,
            id: spa::sys::SPA_PARAM_EnumFormat,
            properties: info.into(),
        };
        let bytes = match spa::pod::serialize::PodSerializer::serialize(
            std::io::Cursor::new(Vec::new()),
            &spa::pod::Value::Object(obj),
        ) {
            Ok((c, _)) => c.into_inner(),
            Err(e) => {
                let _ = ready.send(Err(fail(format!("{e:?}"))));
                return;
            }
        };
        let Some(pod) = spa::pod::Pod::from_bytes(&bytes) else {
            let _ = ready.send(Err(fail("format pod".into())));
            return;
        };
        if let Err(e) = stream.connect(
            spa::utils::Direction::Input,
            None,
            pw::stream::StreamFlags::AUTOCONNECT
                | pw::stream::StreamFlags::MAP_BUFFERS
                | pw::stream::StreamFlags::RT_PROCESS,
            &mut [pod],
        ) {
            let _ = ready.send(Err(fail(e.to_string())));
            return;
        }
        let (ml_t, stop_t) = (mainloop.clone(), Arc::clone(&stop));
        let timer = mainloop.loop_().add_timer(move |_| {
            if stop_t.load(Ordering::Relaxed) {
                ml_t.quit();
            }
        });
        timer.update_timer(
            Some(Duration::from_millis(50)),
            Some(Duration::from_millis(50)),
        );
        mainloop.run();
    }

    impl AudioSource for PipeWireAudioSource {
        fn format(&self) -> SourceFormat {
            self.format.clone()
        }

        fn start(&mut self, sink: SourceSink) -> Result<(), CmError> {
            self.stop.store(false, Ordering::Relaxed);
            let (tx, rx) = channel();
            let (monitor, target, channels) =
                (self.monitor, self.target.clone(), self.format.channels);
            let stop = Arc::clone(&self.stop);
            self.thread = Some(
                std::thread::Builder::new()
                    .name("cm-pw-audio".into())
                    .spawn(move || run_loop(monitor, target, channels, sink, stop, tx))
                    .map_err(|e| CmError::internal(format!("audio thread: {e}")))?,
            );
            match rx.recv_timeout(Duration::from_secs(10)) {
                Ok(Ok(())) => Ok(()),
                Ok(Err(e)) => {
                    self.stop();
                    Err(e)
                }
                Err(RecvTimeoutError::Timeout) => {
                    self.stop();
                    Err(CmError::internal(
                        "PipeWire did not start the audio stream in time.",
                    ))
                }
                Err(RecvTimeoutError::Disconnected) => {
                    self.stop();
                    Err(CmError::internal("the PipeWire audio thread ended early"))
                }
            }
        }

        fn stop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            if let Some(t) = self.thread.take() {
                let _ = t.join();
            }
        }
    }

    impl Drop for PipeWireAudioSource {
        fn drop(&mut self) {
            self.stop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_buffer_started_one_duration_before_it_was_delivered() {
        // 480 frames at 48 kHz are 10 ms.
        assert_eq!(chunk_start_ns(1_000_000_000, 480, 48_000), 990_000_000);
        assert_eq!(chunk_start_ns(5, 480, 48_000), 0);
        assert_eq!(chunk_start_ns(10, 0, 0), 10);
    }

    #[test]
    fn f32_bytes_decode_and_ragged_tails_are_ignored() {
        let mut out = vec![9.0];
        let mut b = Vec::new();
        for v in [0.5f32, -1.0] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.push(7);
        decode_f32le(&b, &mut out);
        assert_eq!(out, vec![0.5, -1.0]);
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn other_systems_answer_not_implemented() {
        assert!(!available());
        let e = (mic_factory())(&DeviceSelector::Default).err().unwrap();
        assert_eq!(e.code(), cm_core::ErrorCode::NotImplemented);
    }
}
