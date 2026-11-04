//! Windows hardware encoder: the Media Foundation H.264 transform (NVENC, Quick Sync or AMF behind
//! it) fed system-memory NV12 frames, its Annex B output muxed into the shared 2 s fragmented MP4
//! writer.
//!
//! * Only transforms flagged `MFT_ENUM_FLAG_HARDWARE` are used. There is no software fallback
//!   (REC-2, CLAUDE.md): no hardware transform, or none that accepts the plan, is
//!   `hardware_encoder_unavailable`, and the recorder then tries smaller sizes.
//! * Hardware transforms are asynchronous (`MF_TRANSFORM_ASYNC`): input is only accepted after a
//!   `METransformNeedInput` event and output arrives with `METransformHaveOutput`. A transform
//!   that turns out to be synchronous is driven with the plain input / output loop.
//! * All Media Foundation objects live on one worker thread. `append` converts nothing and
//!   blocks on nothing: it hands the frame to a short queue and answers `NotReady` when the
//!   encoder is behind (the frame then counts as late, REC-3). The worker converts the frame to
//!   NV12 ([`crate::nv12`]), asks for a key frame every 2 s ([`KeyframeClock`]), and passes each
//!   encoded frame to [`H264Muxer`] logic ([`crate::h264_mux`]).
//! * B-frames are off, so output order is input order and a sample's time is its file time.
//!
//! This is the only module in the crate with Windows `unsafe`. Each block says why it is sound:
//! the COM objects are created and used on the worker thread only, buffers are locked for the
//! length of one copy, and every pointer passed to Media Foundation points to a live local.
//!
//! The hardware path cannot run on the development Mac; its steps are in
//! `tests/manual/REC-WIN.md` and `crates/cm-capture/tests/record_real.rs`.

use std::ffi::c_void;
use std::sync::mpsc::{channel, sync_channel, Receiver, SyncSender, TryRecvError, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use cm_core::{CmError, MediaTime};
use windows::core::Interface;
use windows::Win32::Media::MediaFoundation::{
    eAVEncCommonRateControlMode_PeakConstrainedVBR, eAVEncH264VProfile_High,
    CODECAPI_AVEncCommonMaxBitRate, CODECAPI_AVEncCommonMeanBitRate,
    CODECAPI_AVEncCommonRateControlMode, CODECAPI_AVEncMPVDefaultBPictureCount,
    CODECAPI_AVEncMPVGOPSize, CODECAPI_AVEncVideoForceKeyFrame, CODECAPI_AVLowLatencyMode,
    ICodecAPI, IMFActivate, IMFMediaBuffer, IMFMediaEventGenerator, IMFMediaType, IMFSample,
    IMFTransform, METransformDrainComplete, METransformHaveOutput, METransformNeedInput,
    MFCreateMediaType, MFCreateMemoryBuffer, MFCreateSample, MFMediaType_Video,
    MFNominalRange_16_235, MFShutdown, MFStartup, MFTEnumEx, MFVideoFormat_H264,
    MFVideoFormat_NV12, MFVideoInterlace_Progressive, MFVideoPrimaries_BT709, MFVideoTransFunc_709,
    MFVideoTransferMatrix_BT709, MFSTARTUP_FULL, MFT_CATEGORY_VIDEO_ENCODER,
    MFT_ENUM_FLAG_HARDWARE, MFT_ENUM_FLAG_SORTANDFILTER, MFT_MESSAGE_COMMAND_DRAIN,
    MFT_MESSAGE_COMMAND_FLUSH, MFT_MESSAGE_NOTIFY_BEGIN_STREAMING,
    MFT_MESSAGE_NOTIFY_END_OF_STREAM, MFT_MESSAGE_NOTIFY_START_OF_STREAM, MFT_OUTPUT_DATA_BUFFER,
    MFT_OUTPUT_STREAM_PROVIDES_SAMPLES, MFT_REGISTER_TYPE_INFO, MF_EVENT_FLAG_NO_WAIT,
    MF_E_NO_EVENTS_AVAILABLE, MF_E_TRANSFORM_NEED_MORE_INPUT, MF_E_TRANSFORM_STREAM_CHANGE,
    MF_LOW_LATENCY, MF_MT_AVG_BITRATE, MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE, MF_MT_INTERLACE_MODE,
    MF_MT_MAJOR_TYPE, MF_MT_MPEG2_PROFILE, MF_MT_MPEG_SEQUENCE_HEADER, MF_MT_PIXEL_ASPECT_RATIO,
    MF_MT_SUBTYPE, MF_MT_TRANSFER_FUNCTION, MF_MT_VIDEO_NOMINAL_RANGE, MF_MT_VIDEO_PRIMARIES,
    MF_MT_YUV_MATRIX, MF_TRANSFORM_ASYNC, MF_TRANSFORM_ASYNC_UNLOCK, MF_VERSION,
};
use windows::Win32::System::Com::{
    CoInitializeEx, CoTaskMemFree, CoUninitialize, COINIT_MULTITHREADED,
};
use windows::Win32::System::Variant::{VARIANT, VT_UI4};

use crate::cpu::CpuFrame;
use crate::encoder::{
    hardware_encoder_unavailable, no_frames_error, AppendOutcome, EncoderConfig, VideoEncoder,
};
use crate::h264::{self, AvccSample};
use crate::h264_mux::{hns_to_ticks, init_segment, ticks_to_hns, FragmentCutter, MuxSample};
use crate::mf_logic::{h264_plan_for, pack_pair, KeyframeClock};
use crate::nv12::{frame_to_nv12, nv12_len};
use crate::settings::VideoPlan;
use crate::writer::{FragmentedMp4Writer, WriterSummary};

/// Frames queued for the worker before `append` answers `NotReady` (about 130 ms at 30 fps).
pub const QUEUE_FRAMES: usize = 4;
/// How long the worker waits for the transform to finish draining at the end.
const DRAIN_TIMEOUT: Duration = Duration::from_secs(10);
/// How long a transform may stay silent while it is not asking for input before it counts as hung.
const STALL_TIMEOUT: Duration = Duration::from_secs(10);
/// How long `create` waits for the worker to set the transform up.
const SETUP_TIMEOUT: Duration = Duration::from_secs(20);

/// The error the Windows encoder returns for things it does not do.
pub fn not_implemented() -> CmError {
    CmError::not_implemented("the Windows Media Foundation hardware encoder cannot do this")
}

fn mf(what: &str, e: windows::core::Error) -> CmError {
    CmError::internal(format!("Media Foundation: {what}: {e}"))
}

enum Cmd {
    Frame(CpuFrame, i64),
    Finish,
}

#[derive(Default)]
struct Shared {
    /// The first failure on the worker; `append` and `check` report it.
    error: Mutex<Option<CmError>>,
}

impl Shared {
    fn set(&self, e: CmError) {
        if let Ok(mut s) = self.error.lock() {
            s.get_or_insert(e);
        }
    }

    fn get(&self) -> Option<CmError> {
        self.error.lock().ok().and_then(|e| e.clone())
    }
}

/// The Media Foundation H.264 hardware encoder.
pub struct HardwareEncoder {
    tx: Option<SyncSender<Cmd>>,
    worker: Option<JoinHandle<()>>,
    shared: Arc<Shared>,
    writer: FragmentedMp4Writer,
    asked: VideoPlan,
    used: VideoPlan,
    summary: Option<WriterSummary>,
}

impl HardwareEncoder {
    /// Starts the transform for `cfg`. `hardware_encoder_unavailable` when this computer has no
    /// hardware H.264 transform that takes the plan (the recorder then tries a smaller size).
    pub fn create(cfg: &EncoderConfig) -> Result<Self, CmError> {
        let used = h264_plan_for(&cfg.plan)?;
        let writer = FragmentedMp4Writer::create(&cfg.path)?;
        let shared = Arc::new(Shared::default());
        let (tx, rx) = sync_channel::<Cmd>(QUEUE_FRAMES);
        let (ready_tx, ready_rx) = channel::<Result<(), CmError>>();
        let (worker_writer, worker_shared) = (writer.clone(), Arc::clone(&shared));
        let worker = std::thread::Builder::new()
            .name("cm-mf-encoder".into())
            .spawn(move || worker_main(used, worker_writer, worker_shared, rx, ready_tx))
            .map_err(|e| CmError::internal(format!("cannot start the encoder thread: {e}")))?;
        let ready = ready_rx
            .recv_timeout(SETUP_TIMEOUT)
            .unwrap_or_else(|_| Err(hardware_encoder_unavailable()));
        if let Err(e) = ready {
            let _ = worker.join();
            let _ = writer.finish();
            let _ = std::fs::remove_file(&cfg.path);
            return Err(e);
        }
        Ok(HardwareEncoder {
            tx: Some(tx),
            worker: Some(worker),
            shared,
            writer,
            asked: cfg.plan,
            used,
            summary: None,
        })
    }
}

impl VideoEncoder for HardwareEncoder {
    type Frame = CpuFrame;

    fn append(
        &mut self,
        frame: &CpuFrame,
        pts: MediaTime,
        _duration: MediaTime,
    ) -> Result<AppendOutcome, CmError> {
        if let Some(e) = self.shared.get() {
            return Err(e);
        }
        let Some(tx) = &self.tx else {
            return Err(CmError::internal("the encoder is already finished"));
        };
        match tx.try_send(Cmd::Frame(frame.clone(), pts.ticks())) {
            Ok(()) => Ok(AppendOutcome::Written),
            Err(TrySendError::Full(_)) => Ok(AppendOutcome::NotReady),
            Err(TrySendError::Disconnected(_)) => Err(self
                .shared
                .get()
                .unwrap_or_else(|| CmError::internal("the encoder thread has stopped"))),
        }
    }

    fn used_plan(&self) -> Option<VideoPlan> {
        (self.used != self.asked).then_some(self.used)
    }

    fn check(&self) -> Result<(), CmError> {
        if let Some(e) = self.shared.get() {
            return Err(e);
        }
        self.writer.check()
    }

    fn fragments_written(&self) -> usize {
        self.writer.fragments_written()
    }

    fn finish(&mut self) -> Result<WriterSummary, CmError> {
        if let Some(s) = &self.summary {
            return Ok(s.clone());
        }
        if let Some(tx) = self.tx.take() {
            // The queue may hold frames; the worker takes them first, then the finish.
            log::info!("encoder finish: sending Finish");
            let _ = tx.send(Cmd::Finish);
        }
        if let Some(w) = self.worker.take() {
            let _ = w.join();
            log::info!("encoder finish: the worker has ended");
        }
        let summary = self.writer.finish()?;
        log::info!("encoder finish: the writer is finished");
        if summary.fragments.is_empty() {
            return Err(self.shared.get().unwrap_or_else(no_frames_error));
        }
        self.summary = Some(summary.clone());
        Ok(summary)
    }
}

impl Drop for HardwareEncoder {
    fn drop(&mut self) {
        if self.summary.is_none() {
            let _ = self.finish();
        }
    }
}

// --- the worker thread -------------------------------------------------------------------------

fn worker_main(
    plan: VideoPlan,
    writer: FragmentedMp4Writer,
    shared: Arc<Shared>,
    rx: Receiver<Cmd>,
    ready: std::sync::mpsc::Sender<Result<(), CmError>>,
) {
    // SAFETY: plain COM and Media Foundation start-up on this thread; undone below in reverse.
    let com = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
    // SAFETY: as above.
    let started = unsafe { MFStartup(MF_VERSION, MFSTARTUP_FULL) }.is_ok();
    let session = if started {
        Session::open(plan, writer)
    } else {
        Err(hardware_encoder_unavailable())
    };
    match session {
        Ok(mut s) => {
            let _ = ready.send(Ok(()));
            if let Err(e) = s.run(&rx) {
                log::warn!("the Media Foundation encoder failed: {e}");
                shared.set(e);
            }
            // Dropping the session releases every COM object before the shutdown below.
            log::info!("encoder worker: run ended, releasing the session");
            drop(s);
            log::info!("encoder worker: session released");
        }
        Err(e) => {
            let _ = ready.send(Err(e));
        }
    }
    if started {
        // SAFETY: balances the successful MFStartup above; all objects are released.
        let _ = unsafe { MFShutdown() };
        log::info!("encoder worker: MFShutdown done");
    }
    if com {
        // SAFETY: balances the successful CoInitializeEx above.
        unsafe { CoUninitialize() };
    }
}

/// A source frame's pixels and the NV12 made from them.
type ConvertedFrame = (Arc<Vec<u8>>, Arc<Vec<u8>>);

struct Session {
    transform: IMFTransform,
    events: Option<IMFMediaEventGenerator>,
    codec_api: Option<ICodecAPI>,
    provides_samples: bool,
    out_size: u32,
    plan: VideoPlan,
    writer: FragmentedMp4Writer,
    cutter: FragmentCutter,
    keyframes: KeyframeClock,
    /// Parameter sets from the transform's output type, used when a key frame carries none.
    header: Option<(Vec<u8>, Vec<u8>)>,
    init_written: bool,
    need_input: u32,
    drained: bool,
    /// NV12 of the last frame converted, for repeated frames (a static screen).
    last_nv12: Option<ConvertedFrame>,
}

fn input_type(plan: &VideoPlan, subtype: &windows::core::GUID) -> Result<IMFMediaType, CmError> {
    // SAFETY: creating a media type and setting attributes on it; the arguments are live locals.
    unsafe {
        let t = MFCreateMediaType().map_err(|e| mf("MFCreateMediaType", e))?;
        let set = |r: windows::core::Result<()>| r.map_err(|e| mf("set attribute", e));
        set(t.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video))?;
        set(t.SetGUID(&MF_MT_SUBTYPE, subtype))?;
        set(t.SetUINT64(&MF_MT_FRAME_SIZE, pack_pair(plan.width, plan.height)))?;
        set(t.SetUINT64(&MF_MT_FRAME_RATE, pack_pair(plan.fps, 1)))?;
        set(t.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack_pair(1, 1)))?;
        set(t.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32))?;
        Ok(t)
    }
}

/// A `VT_UI4` variant, which is what the codec properties used here take.
fn variant_u32(value: u32) -> VARIANT {
    let mut v = VARIANT::default();
    // SAFETY: writes the type tag and the 32-bit member of a zeroed variant that owns nothing.
    unsafe {
        let inner = &mut *v.Anonymous.Anonymous;
        inner.vt = VT_UI4;
        inner.Anonymous.ulVal = value;
    }
    v
}

fn codec_value(api: &ICodecAPI, key: &windows::core::GUID, value: u32, name: &str) {
    let v = variant_u32(value);
    // SAFETY: `key` and `v` are live for the call; a transform that lacks the property answers
    // with an error, which only means the default stays.
    if let Err(e) = unsafe { api.SetValue(key, &v) } {
        log::debug!("encoder property {name} not set: {e}");
    }
}

impl Session {
    fn open(plan: VideoPlan, writer: FragmentedMp4Writer) -> Result<Self, CmError> {
        let input = MFT_REGISTER_TYPE_INFO {
            guidMajorType: MFMediaType_Video,
            guidSubtype: MFVideoFormat_NV12,
        };
        let output = MFT_REGISTER_TYPE_INFO {
            guidMajorType: MFMediaType_Video,
            guidSubtype: MFVideoFormat_H264,
        };
        let mut list: *mut Option<IMFActivate> = std::ptr::null_mut();
        let mut count = 0u32;
        // SAFETY: the out parameters are live locals; on success `list` points to `count`
        // activation objects allocated by Media Foundation, taken below and then freed.
        let found = unsafe {
            MFTEnumEx(
                MFT_CATEGORY_VIDEO_ENCODER,
                MFT_ENUM_FLAG_HARDWARE | MFT_ENUM_FLAG_SORTANDFILTER,
                Some(&input),
                Some(&output),
                &mut list,
                &mut count,
            )
        };
        if found.is_err() || list.is_null() || count == 0 {
            return Err(hardware_encoder_unavailable());
        }
        let mut activates = Vec::with_capacity(count as usize);
        for i in 0..count as usize {
            // SAFETY: `i < count`, so the slot is inside the array; `take` leaves None behind and
            // moves the COM reference out, so each reference is released exactly once.
            if let Some(a) = unsafe { (*list.add(i)).take() } {
                activates.push(a);
            }
        }
        // SAFETY: `list` came from MFTEnumEx, which documents CoTaskMemFree for it.
        unsafe { CoTaskMemFree(Some(list as *const c_void)) };

        let mut last = hardware_encoder_unavailable();
        for activate in activates {
            // SAFETY: activating a transform from its activation object.
            let transform: IMFTransform = match unsafe { activate.ActivateObject() } {
                Ok(t) => t,
                Err(e) => {
                    log::debug!("hardware encoder not activated: {e}");
                    continue;
                }
            };
            match Self::configure(transform, plan, writer.clone()) {
                Ok(s) => return Ok(s),
                Err(e) => {
                    log::debug!("hardware encoder rejected the plan: {e}");
                    last = hardware_encoder_unavailable();
                }
            }
        }
        Err(last)
    }

    fn configure(
        transform: IMFTransform,
        plan: VideoPlan,
        writer: FragmentedMp4Writer,
    ) -> Result<Self, CmError> {
        // SAFETY: every call below passes live locals; the transform is used on this thread only.
        unsafe {
            let attrs = transform
                .GetAttributes()
                .map_err(|e| mf("GetAttributes", e))?;
            let is_async = attrs.GetUINT32(&MF_TRANSFORM_ASYNC).unwrap_or(0) != 0;
            if is_async {
                attrs
                    .SetUINT32(&MF_TRANSFORM_ASYNC_UNLOCK, 1)
                    .map_err(|e| mf("unlock the asynchronous transform", e))?;
            }
            let _ = attrs.SetUINT32(&MF_LOW_LATENCY, 1);

            // Rate control and key frames first, then the media types (the order encoders like).
            let codec_api = transform.cast::<ICodecAPI>().ok();
            if let Some(api) = &codec_api {
                let bps = u32::try_from(plan.bitrate_bps).unwrap_or(u32::MAX);
                codec_value(
                    api,
                    &CODECAPI_AVEncCommonRateControlMode,
                    eAVEncCommonRateControlMode_PeakConstrainedVBR.0 as u32,
                    "rate control",
                );
                codec_value(api, &CODECAPI_AVEncCommonMeanBitRate, bps, "mean bit rate");
                codec_value(
                    api,
                    &CODECAPI_AVEncCommonMaxBitRate,
                    bps.saturating_add(bps / 2),
                    "max bit rate",
                );
                codec_value(api, &CODECAPI_AVEncMPVGOPSize, plan.fps * 2, "GOP size");
                codec_value(api, &CODECAPI_AVEncMPVDefaultBPictureCount, 0, "B pictures");
                codec_value(api, &CODECAPI_AVLowLatencyMode, 1, "low latency");
            }

            let out = input_type(&plan, &MFVideoFormat_H264)?;
            let set = |r: windows::core::Result<()>| r.map_err(|e| mf("set attribute", e));
            set(out.SetUINT32(
                &MF_MT_AVG_BITRATE,
                u32::try_from(plan.bitrate_bps).unwrap_or(u32::MAX),
            ))?;
            set(out.SetUINT32(&MF_MT_MPEG2_PROFILE, eAVEncH264VProfile_High.0 as u32))?;
            transform
                .SetOutputType(0, &out, 0)
                .map_err(|e| mf("SetOutputType", e))?;

            let inp = input_type(&plan, &MFVideoFormat_NV12)?;
            set(inp.SetUINT32(&MF_MT_VIDEO_PRIMARIES, MFVideoPrimaries_BT709.0 as u32))?;
            set(inp.SetUINT32(&MF_MT_TRANSFER_FUNCTION, MFVideoTransFunc_709.0 as u32))?;
            set(inp.SetUINT32(&MF_MT_YUV_MATRIX, MFVideoTransferMatrix_BT709.0 as u32))?;
            set(inp.SetUINT32(&MF_MT_VIDEO_NOMINAL_RANGE, MFNominalRange_16_235.0 as u32))?;
            transform
                .SetInputType(0, &inp, 0)
                .map_err(|e| mf("SetInputType", e))?;

            let info = transform
                .GetOutputStreamInfo(0)
                .map_err(|e| mf("GetOutputStreamInfo", e))?;
            let header = sequence_header(&transform);

            let _ = transform.ProcessMessage(MFT_MESSAGE_COMMAND_FLUSH, 0);
            transform
                .ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0)
                .map_err(|e| mf("begin streaming", e))?;
            transform
                .ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0)
                .map_err(|e| mf("start of stream", e))?;

            let frame_ticks = cm_core::TICKS_PER_SECOND / i64::from(plan.fps.max(1));
            Ok(Session {
                events: if is_async {
                    transform.cast::<IMFMediaEventGenerator>().ok()
                } else {
                    None
                },
                codec_api,
                provides_samples: info.dwFlags & MFT_OUTPUT_STREAM_PROVIDES_SAMPLES.0 as u32 != 0,
                out_size: info.cbSize.max(1 << 20),
                transform,
                plan,
                writer,
                cutter: FragmentCutter::new(frame_ticks),
                keyframes: KeyframeClock::default(),
                header,
                init_written: false,
                need_input: 0,
                drained: false,
                last_nv12: None,
            })
        }
    }

    fn run(&mut self, rx: &Receiver<Cmd>) -> Result<(), CmError> {
        let mut finishing = false;
        let mut last_progress = Instant::now();
        while !finishing {
            let mut busy = self.pump_events()?;
            while self.can_accept() {
                match rx.try_recv() {
                    Ok(Cmd::Frame(f, pts)) => {
                        self.encode(&f, pts)?;
                        busy = true;
                        busy |= self.pump_events()?;
                    }
                    Ok(Cmd::Finish) | Err(TryRecvError::Disconnected) => {
                        finishing = true;
                        break;
                    }
                    Err(TryRecvError::Empty) => break,
                }
            }
            if busy {
                last_progress = Instant::now();
            } else if !finishing {
                // A transform that neither asks for input nor produces output for this long has
                // hung (a driver reset, say): fail the recording instead of blocking the stop.
                if self.need_input == 0 && last_progress.elapsed() > STALL_TIMEOUT {
                    return Err(CmError::internal(
                        "the hardware H.264 encoder stopped responding",
                    ));
                }
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        self.drain()?;
        if let Some(last) = self.cutter.finish() {
            self.writer.push_segment(last);
        }
        Ok(())
    }

    /// An asynchronous transform wants input only after it said so; a synchronous one any time.
    fn can_accept(&self) -> bool {
        self.events.is_none() || self.need_input > 0
    }

    /// Handles every queued transform event. True when there was one.
    fn pump_events(&mut self) -> Result<bool, CmError> {
        let Some(gen) = self.events.clone() else {
            return Ok(false);
        };
        let mut any = false;
        loop {
            // SAFETY: GetEvent on a live event generator; NO_WAIT never blocks.
            let ev = match unsafe { gen.GetEvent(MF_EVENT_FLAG_NO_WAIT) } {
                Ok(ev) => ev,
                Err(e) if e.code() == MF_E_NO_EVENTS_AVAILABLE => return Ok(any),
                Err(e) => return Err(mf("GetEvent", e)),
            };
            any = true;
            // SAFETY: reading the type of an event we own.
            let kind = unsafe { ev.GetType() }.map_err(|e| mf("event type", e))?;
            if kind == METransformNeedInput.0 as u32 {
                self.need_input += 1;
            } else if kind == METransformHaveOutput.0 as u32 {
                self.take_output()?;
            } else if kind == METransformDrainComplete.0 as u32 {
                self.drained = true;
            }
        }
    }

    fn encode(&mut self, frame: &CpuFrame, pts: i64) -> Result<(), CmError> {
        let nv12 = match &self.last_nv12 {
            Some((src, nv)) if Arc::ptr_eq(src, &frame.data) => Arc::clone(nv),
            _ => {
                let nv = Arc::new(frame_to_nv12(
                    frame,
                    None,
                    self.plan.width,
                    self.plan.height,
                )?);
                self.last_nv12 = Some((Arc::clone(&frame.data), Arc::clone(&nv)));
                nv
            }
        };
        debug_assert_eq!(nv12.len(), nv12_len(self.plan.width, self.plan.height));
        if self.keyframes.should_force(pts) {
            if let Some(api) = &self.codec_api {
                codec_value(api, &CODECAPI_AVEncVideoForceKeyFrame, 1, "force key frame");
            }
        }
        let interval = cm_core::TICKS_PER_SECOND / i64::from(self.plan.fps.max(1));
        let sample = make_input_sample(&nv12, ticks_to_hns(pts), ticks_to_hns(interval))?;
        // SAFETY: the sample is live; one input stream (id 0).
        unsafe { self.transform.ProcessInput(0, &sample, 0) }.map_err(|e| mf("ProcessInput", e))?;
        if self.events.is_some() {
            self.need_input = self.need_input.saturating_sub(1);
        } else {
            self.take_sync_outputs()?;
        }
        Ok(())
    }

    /// A synchronous transform: pull every output the input made.
    fn take_sync_outputs(&mut self) -> Result<(), CmError> {
        while self.take_output()? {}
        Ok(())
    }

    /// One `ProcessOutput`. False when the transform has nothing more without new input.
    fn take_output(&mut self) -> Result<bool, CmError> {
        let provided = if self.provides_samples {
            None
        } else {
            Some(alloc_output_sample(self.out_size)?)
        };
        let mut buf = [MFT_OUTPUT_DATA_BUFFER {
            dwStreamID: 0,
            pSample: std::mem::ManuallyDrop::new(provided),
            dwStatus: 0,
            pEvents: std::mem::ManuallyDrop::new(None),
        }];
        let mut status = 0u32;
        // SAFETY: `buf` and `status` are live locals; one output stream (id 0).
        let res = unsafe { self.transform.ProcessOutput(0, &mut buf, &mut status) };
        // SAFETY: the buffer's fields were initialised above and are taken exactly once, so the
        // sample and the event collection are released when these locals drop.
        let (sample, _events) = unsafe {
            (
                std::mem::ManuallyDrop::take(&mut buf[0].pSample),
                std::mem::ManuallyDrop::take(&mut buf[0].pEvents),
            )
        };
        match res {
            Ok(()) => {
                if let Some(s) = sample {
                    self.handle_sample(&s)?;
                }
                Ok(true)
            }
            Err(e) if e.code() == MF_E_TRANSFORM_NEED_MORE_INPUT => Ok(false),
            Err(e) if e.code() == MF_E_TRANSFORM_STREAM_CHANGE => {
                // The output type changed (a new sequence header): accept the transform's own.
                // SAFETY: querying and setting the transform's output type.
                unsafe {
                    let t = self
                        .transform
                        .GetOutputAvailableType(0, 0)
                        .map_err(|e| mf("GetOutputAvailableType", e))?;
                    self.transform
                        .SetOutputType(0, &t, 0)
                        .map_err(|e| mf("SetOutputType", e))?;
                }
                self.header = sequence_header(&self.transform);
                Ok(true)
            }
            Err(e) => Err(mf("ProcessOutput", e)),
        }
    }

    fn handle_sample(&mut self, s: &IMFSample) -> Result<(), CmError> {
        // SAFETY: reading time and bytes of a sample the transform just returned.
        let (hns, data) = unsafe {
            let hns = s.GetSampleTime().map_err(|e| mf("GetSampleTime", e))?;
            let buf = s
                .ConvertToContiguousBuffer()
                .map_err(|e| mf("ConvertToContiguousBuffer", e))?;
            (hns, read_buffer(&buf)?)
        };
        let avcc = match h264::annexb_to_avcc(&data) {
            Ok(a) => a,
            Err(e) => {
                log::warn!("skipping an encoder output: {e}");
                return Ok(());
            }
        };
        self.mux(hns_to_ticks(hns), avcc)
    }

    fn mux(&mut self, pts: i64, s: AvccSample) -> Result<(), CmError> {
        if !self.init_written {
            // The init segment needs the parameter sets: the first key frame has them, or the
            // transform's sequence header does.
            let (sps, pps) = match (&s.sps, &s.pps, &self.header) {
                (Some(a), Some(b), _) => (a.clone(), b.clone()),
                (_, _, Some((a, b))) => (a.clone(), b.clone()),
                _ if !s.sync => return Ok(()), // wait for a key frame
                _ => {
                    return Err(CmError::internal(
                        "the H.264 encoder gave no sequence and picture parameter sets",
                    ))
                }
            };
            if !s.sync {
                return Ok(());
            }
            self.writer
                .push_init(init_segment(self.plan.width, self.plan.height, &sps, &pps)?);
            self.init_written = true;
        }
        if let Some(segment) = self.cutter.push(MuxSample::from((pts, s))) {
            self.writer.push_segment(segment);
        }
        Ok(())
    }

    /// End of stream: flush what the transform still holds.
    fn drain(&mut self) -> Result<(), CmError> {
        // SAFETY: stream-end messages to a live transform.
        unsafe {
            let _ = self
                .transform
                .ProcessMessage(MFT_MESSAGE_NOTIFY_END_OF_STREAM, 0);
            self.transform
                .ProcessMessage(MFT_MESSAGE_COMMAND_DRAIN, 0)
                .map_err(|e| mf("drain", e))?;
        }
        if self.events.is_none() {
            return self.take_sync_outputs();
        }
        let started = Instant::now();
        while !self.drained {
            if !self.pump_events()? {
                if started.elapsed() > DRAIN_TIMEOUT {
                    log::warn!("the encoder did not finish draining; closing the file as it is");
                    break;
                }
                std::thread::sleep(Duration::from_millis(2));
            }
        }
        Ok(())
    }
}

/// The SPS and PPS the transform publishes on its output type, if it does.
fn sequence_header(t: &IMFTransform) -> Option<(Vec<u8>, Vec<u8>)> {
    // SAFETY: reading a blob attribute of the current output type into a buffer of its size.
    unsafe {
        let ty = t.GetOutputCurrentType(0).ok()?;
        let n = ty.GetBlobSize(&MF_MT_MPEG_SEQUENCE_HEADER).ok()? as usize;
        let mut blob = vec![0u8; n];
        ty.GetBlob(&MF_MT_MPEG_SEQUENCE_HEADER, &mut blob, None)
            .ok()?;
        let (sps, pps) = h264::parameter_sets(&blob);
        Some((sps?, pps?))
    }
}

fn make_input_sample(nv12: &[u8], time_hns: i64, duration_hns: i64) -> Result<IMFSample, CmError> {
    let len = u32::try_from(nv12.len()).map_err(|_| CmError::internal("frame too large"))?;
    // SAFETY: the buffer is locked for exactly one copy of `len` bytes into memory it owns.
    unsafe {
        let buf = MFCreateMemoryBuffer(len).map_err(|e| mf("MFCreateMemoryBuffer", e))?;
        let mut p: *mut u8 = std::ptr::null_mut();
        buf.Lock(&mut p, None, None)
            .map_err(|e| mf("buffer lock", e))?;
        std::ptr::copy_nonoverlapping(nv12.as_ptr(), p, nv12.len());
        buf.Unlock().map_err(|e| mf("buffer unlock", e))?;
        buf.SetCurrentLength(len)
            .map_err(|e| mf("SetCurrentLength", e))?;
        let sample = MFCreateSample().map_err(|e| mf("MFCreateSample", e))?;
        sample.AddBuffer(&buf).map_err(|e| mf("AddBuffer", e))?;
        sample
            .SetSampleTime(time_hns)
            .map_err(|e| mf("SetSampleTime", e))?;
        sample
            .SetSampleDuration(duration_hns)
            .map_err(|e| mf("SetSampleDuration", e))?;
        Ok(sample)
    }
}

fn alloc_output_sample(size: u32) -> Result<IMFSample, CmError> {
    // SAFETY: creating an empty sample with one buffer of `size` bytes for the transform to fill.
    unsafe {
        let buf = MFCreateMemoryBuffer(size).map_err(|e| mf("MFCreateMemoryBuffer", e))?;
        let sample = MFCreateSample().map_err(|e| mf("MFCreateSample", e))?;
        sample.AddBuffer(&buf).map_err(|e| mf("AddBuffer", e))?;
        Ok(sample)
    }
}

/// Copies the bytes of a media buffer.
///
/// # Safety
/// `buf` must be a live buffer; it is locked only for the copy.
unsafe fn read_buffer(buf: &IMFMediaBuffer) -> Result<Vec<u8>, CmError> {
    let mut p: *mut u8 = std::ptr::null_mut();
    let mut len = 0u32;
    // SAFETY: the out parameters are live locals; the lock is released below.
    unsafe { buf.Lock(&mut p, None, Some(&mut len)) }.map_err(|e| mf("buffer lock", e))?;
    // SAFETY: while locked, `p` points to `len` readable bytes.
    let data = unsafe { std::slice::from_raw_parts(p, len as usize) }.to_vec();
    // SAFETY: balances the lock above.
    unsafe { buf.Unlock() }.map_err(|e| mf("buffer unlock", e))?;
    Ok(data)
}
