//! System audio on Windows without CaptureMann's own sound (AUD-2): WASAPI *process loopback*
//! (`ActivateAudioInterfaceAsync` on `VAD\Process_Loopback`, Windows 10 build 19041 or later, officially
//! 20348), in the mode that captures everything the computer plays except the target process tree. The
//! target is this process, so the app's chimes, previews and its own playback stay out of the recording.
//! Where the activation is refused (an older Windows) the caller falls back to the plain loopback of the
//! default output (`cpal_source`), which cannot exclude anything.
//!
//! The capture runs on its own thread (COM multithreaded apartment, an event-driven shared-mode stream of
//! 48 kHz stereo float; the virtual device cannot report a mix format, so the format is asked for and
//! converted by the audio engine).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

use cm_core::CmError;
use windows::core::{implement, w, Interface, Ref, HRESULT};
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
use windows::Win32::Media::Audio::{
    ActivateAudioInterfaceAsync, IActivateAudioInterfaceAsyncOperation,
    IActivateAudioInterfaceCompletionHandler, IActivateAudioInterfaceCompletionHandler_Impl,
    IAudioCaptureClient, IAudioClient, AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED,
    AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM, AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
    AUDCLNT_STREAMFLAGS_LOOPBACK, AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
    AUDIOCLIENT_ACTIVATION_PARAMS, AUDIOCLIENT_ACTIVATION_PARAMS_0,
    AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK, AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS,
    PROCESS_LOOPBACK_MODE_EXCLUDE_TARGET_PROCESS_TREE, WAVEFORMATEX,
};
use windows::Win32::System::Com::StructuredStorage::{
    PROPVARIANT, PROPVARIANT_0, PROPVARIANT_0_0, PROPVARIANT_0_0_0,
};
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, BLOB, COINIT_MULTITHREADED};
use windows::Win32::System::Threading::{CreateEventW, SetEvent, WaitForSingleObject};
use windows::Win32::System::Variant::VT_BLOB;

use crate::source::{AudioSource, SinkNotifier, SourceFormat, SourceSink};

const RATE: u32 = 48_000;
const CHANNELS: u16 = 2;
/// `WAVE_FORMAT_IEEE_FLOAT` (mmreg.h).
const WAVE_FORMAT_IEEE_FLOAT: u16 = 3;

/// Signals an event when the activation of the audio interface has finished.
#[implement(IActivateAudioInterfaceCompletionHandler)]
struct Done(HANDLE);

impl IActivateAudioInterfaceCompletionHandler_Impl for Done_Impl {
    fn ActivateCompleted(
        &self,
        _operation: Ref<'_, IActivateAudioInterfaceAsyncOperation>,
    ) -> windows::core::Result<()> {
        // SAFETY: the event handle outlives the activation (the starting thread waits for it).
        unsafe { SetEvent(self.0) }
    }
}

fn err(what: &str, e: windows::core::Error) -> CmError {
    CmError::platform_rejected(format!("Windows process loopback: {what}: {e}"), None)
}

/// Whether this Windows accepts the process loopback (an older one refuses the activation): tried once on a
/// short-lived thread with its own COM apartment, the stream is stopped at once.
pub fn available() -> bool {
    std::thread::Builder::new()
        .name("process-loopback-probe".into())
        .spawn(|| {
            // SAFETY: plain COM start-up on this thread; undone before it ends.
            let com = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
            let ok = match open() {
                Ok(opened) => {
                    drop(opened);
                    true
                }
                Err(e) => {
                    log::info!("process loopback is not available: {e}");
                    false
                }
            };
            if com {
                // SAFETY: balances the CoInitializeEx above.
                unsafe { CoUninitialize() };
            }
            ok
        })
        .ok()
        .and_then(|h| h.join().ok())
        .unwrap_or(false)
}

/// System audio without this process's own sound.
pub struct ProcessLoopbackSource {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl ProcessLoopbackSource {
    pub fn new() -> Self {
        ProcessLoopbackSource {
            stop: Arc::new(AtomicBool::new(false)),
            thread: None,
        }
    }
}

impl Default for ProcessLoopbackSource {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioSource for ProcessLoopbackSource {
    fn format(&self) -> SourceFormat {
        SourceFormat {
            sample_rate: RATE,
            channels: CHANNELS,
            device_name: "System audio (without CaptureMann)".into(),
            device_id: "wasapi:process-loopback".into(),
        }
    }

    fn start(&mut self, sink: SourceSink) -> Result<(), CmError> {
        let stop = Arc::clone(&self.stop);
        stop.store(false, Ordering::SeqCst);
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(), CmError>>();
        let handle = std::thread::Builder::new()
            .name("process-loopback".into())
            .spawn(move || capture_thread(sink, stop, ready_tx))
            .map_err(|e| CmError::internal(format!("process loopback thread: {e}")))?;
        match ready_rx.recv_timeout(Duration::from_secs(10)) {
            Ok(Ok(())) => {
                self.thread = Some(handle);
                Ok(())
            }
            Ok(Err(e)) => {
                let _ = handle.join();
                Err(e)
            }
            Err(_) => {
                self.stop.store(true, Ordering::SeqCst);
                Err(CmError::platform_rejected(
                    "Windows process loopback: the activation did not finish",
                    None,
                ))
            }
        }
    }

    fn stop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }

    fn gap_reason(&self) -> &'static str {
        // A loopback sends nothing while nothing plays.
        "idle"
    }
}

impl Drop for ProcessLoopbackSource {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Everything the capture thread owns: released in order before the apartment ends.
struct Opened {
    client: IAudioClient,
    capture: IAudioCaptureClient,
    event: HANDLE,
}

impl Drop for Opened {
    fn drop(&mut self) {
        // SAFETY: the stream was started by `open` and is stopped once; the event was created there.
        unsafe {
            let _ = self.client.Stop();
            let _ = CloseHandle(self.event);
        }
    }
}

fn open() -> Result<Opened, CmError> {
    let params = AUDIOCLIENT_ACTIVATION_PARAMS {
        ActivationType: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
        Anonymous: AUDIOCLIENT_ACTIVATION_PARAMS_0 {
            ProcessLoopbackParams: AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS {
                TargetProcessId: std::process::id(),
                ProcessLoopbackMode: PROCESS_LOOPBACK_MODE_EXCLUDE_TARGET_PROCESS_TREE,
            },
        },
    };
    // Never dropped: `PROPVARIANT`'s drop would free the blob, which lives on this stack (heap corruption,
    // 0xC0000374, in the first version).
    let prop = std::mem::ManuallyDrop::new(PROPVARIANT {
        Anonymous: PROPVARIANT_0 {
            Anonymous: std::mem::ManuallyDrop::new(PROPVARIANT_0_0 {
                vt: VT_BLOB,
                wReserved1: 0,
                wReserved2: 0,
                wReserved3: 0,
                Anonymous: PROPVARIANT_0_0_0 {
                    blob: BLOB {
                        cbSize: std::mem::size_of::<AUDIOCLIENT_ACTIVATION_PARAMS>() as u32,
                        pBlobData: &params as *const _ as *mut u8,
                    },
                },
            }),
        },
    });
    // SAFETY: an auto-reset event handle owned by this function until `Opened` takes it.
    let done = unsafe { CreateEventW(None, false, false, None) }.map_err(|e| err("event", e))?;
    let handler: IActivateAudioInterfaceCompletionHandler = Done(done).into();
    // SAFETY: `prop` and `params` live until the call returns (the blob is copied by the activation); the
    // handler is an agile COM object that only sets the event.
    let op = unsafe {
        ActivateAudioInterfaceAsync(
            w!("VAD\\Process_Loopback"),
            &IAudioClient::IID,
            Some(&*prop),
            &handler,
        )
    }
    .map_err(|e| err("activate", e));
    let op = match op {
        Ok(op) => op,
        Err(e) => {
            // SAFETY: closes the event created above, once.
            unsafe {
                let _ = CloseHandle(done);
            }
            return Err(e);
        }
    };
    // SAFETY: waits on the event created above.
    let waited = unsafe { WaitForSingleObject(done, 5000) };
    if waited != WAIT_OBJECT_0 {
        // SAFETY: closes the event created above, once.
        unsafe {
            let _ = CloseHandle(done);
        }
        return Err(CmError::platform_rejected(
            "Windows process loopback: the activation timed out",
            None,
        ));
    }
    // SAFETY: closes the event created above, once; it is not used again.
    unsafe {
        let _ = CloseHandle(done);
    }
    let mut result = HRESULT(0);
    let mut unknown = None;
    // SAFETY: both out parameters are valid locals; the operation has completed.
    unsafe { op.GetActivateResult(&mut result, &mut unknown) }.map_err(|e| err("result", e))?;
    result.ok().map_err(|e| err("activation refused", e))?;
    let client: IAudioClient = unknown
        .ok_or_else(|| CmError::internal("Windows process loopback: no audio client"))?
        .cast()
        .map_err(|e| err("audio client", e))?;

    let bytes = u32::from(CHANNELS) * 4;
    let format = WAVEFORMATEX {
        wFormatTag: WAVE_FORMAT_IEEE_FLOAT,
        nChannels: CHANNELS,
        nSamplesPerSec: RATE,
        nAvgBytesPerSec: RATE * bytes,
        nBlockAlign: bytes as u16,
        wBitsPerSample: 32,
        cbSize: 0,
    };
    let flags = AUDCLNT_STREAMFLAGS_LOOPBACK
        | AUDCLNT_STREAMFLAGS_EVENTCALLBACK
        | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM
        | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY;
    // SAFETY: `format` lives for the call; shared mode, 20 ms buffer in 100 ns units.
    unsafe { client.Initialize(AUDCLNT_SHAREMODE_SHARED, flags, 200_000, 0, &format, None) }
        .map_err(|e| err("initialize", e))?;
    // SAFETY: a fresh auto-reset event handed to the stream.
    let event = unsafe { CreateEventW(None, false, false, None) }.map_err(|e| err("event", e))?;
    // SAFETY: the stream was initialized with the event-callback flag.
    if let Err(e) = unsafe { client.SetEventHandle(event) } {
        // SAFETY: closes the event created above, once.
        unsafe {
            let _ = CloseHandle(event);
        }
        return Err(err("event handle", e));
    }
    // SAFETY: the client is initialized; the service interface belongs to it.
    let capture: IAudioCaptureClient =
        unsafe { client.GetService() }.map_err(|e| err("capture client", e))?;
    // SAFETY: starts the initialized stream.
    unsafe { client.Start() }.map_err(|e| err("start", e))?;
    Ok(Opened {
        client,
        capture,
        event,
    })
}

fn capture_thread(
    mut sink: SourceSink,
    stop: Arc<AtomicBool>,
    ready: mpsc::Sender<Result<(), CmError>>,
) {
    // SAFETY: plain COM start-up on this thread; undone at the end.
    let com = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
    let notifier: SinkNotifier = sink.notifier();
    match open() {
        Err(e) => {
            let _ = ready.send(Err(e));
        }
        Ok(opened) => {
            let _ = ready.send(Ok(()));
            let mut scratch: Vec<f32> = Vec::with_capacity(4800 * usize::from(CHANNELS));
            while !stop.load(Ordering::SeqCst) {
                // SAFETY: waits on the stream's event.
                let _ = unsafe { WaitForSingleObject(opened.event, 100) };
                loop {
                    // SAFETY: the capture client belongs to the started stream.
                    let next = unsafe { opened.capture.GetNextPacketSize() };
                    let _frames = match next {
                        Ok(0) => break,
                        Ok(n) => n,
                        Err(e) => {
                            notifier.disconnected(e.to_string());
                            break;
                        }
                    };
                    let mut data = std::ptr::null_mut();
                    let mut got = 0u32;
                    let mut flags = 0u32;
                    let mut qpc = 0u64;
                    // SAFETY: the out parameters are valid locals; the buffer is released below.
                    let r = unsafe {
                        opened.capture.GetBuffer(
                            &mut data,
                            &mut got,
                            &mut flags,
                            None,
                            Some(&mut qpc),
                        )
                    };
                    if r.is_err() {
                        break;
                    }
                    let n = got as usize * usize::from(CHANNELS);
                    scratch.clear();
                    if flags & (AUDCLNT_BUFFERFLAGS_SILENT.0 as u32) != 0 || data.is_null() {
                        scratch.resize(n, 0.0);
                    } else {
                        // SAFETY: the buffer holds `got` frames of interleaved f32 until released.
                        let src = unsafe { std::slice::from_raw_parts(data as *const f32, n) };
                        scratch.extend_from_slice(src);
                    }
                    // SAFETY: releases the buffer taken above with the frame count it returned.
                    let _ = unsafe { opened.capture.ReleaseBuffer(got) };
                    // QPC position is in 100 ns units on the capture clock.
                    sink.push(qpc.saturating_mul(100), &scratch);
                }
            }
            drop(opened);
        }
    }
    if com {
        // SAFETY: balances the CoInitializeEx above.
        unsafe { CoUninitialize() };
    }
}
