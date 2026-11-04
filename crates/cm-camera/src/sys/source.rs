//! The Windows [`CameraSource`]: one thread per open camera reads an `IMFSourceReader`
//! synchronously and hands each frame (copied to system memory, top row first) to the sink with
//! the capture time Media Foundation stamped, converted to the host clock.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use windows::core::Interface;
use windows::Win32::Media::MediaFoundation::{
    IMF2DBuffer, IMFMediaSource, IMFMediaType, IMFSample, IMFSourceReader, MFCreateAttributes,
    MFCreateMediaType, MFCreateSourceReaderFromMediaSource, MFMediaType_Video, MFVideoFormat_NV12,
    MFVideoFormat_RGB32, MFVideoFormat_YUY2, MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE, MF_MT_MAJOR_TYPE,
    MF_MT_SUBTYPE, MF_SOURCE_READERF_ENDOFSTREAM, MF_SOURCE_READERF_ERROR,
    MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING,
};

use cm_core::{CmError, HostClock, SystemHostClock};

use super::mf::{
    enumerate_devices, map_error, native_types, Failure, MfRuntime, FIRST_VIDEO_STREAM,
};
use crate::convert::{capture_host_ns, pick_mode, preview_due, PixelFormat, RawFrame};
use crate::types::{
    CameraEvent, CameraFrame, CameraSource, FrameSink, OpenRequest, SourceInfo, SourceRole,
};
use crate::NativeSample;

/// Time `open` allows for finding the camera and starting it, on top of the wait for the first
/// frame.
const OPEN_GRACE: Duration = Duration::from_secs(10);

#[derive(Default)]
pub struct MfSource {
    worker: Option<Worker>,
}

struct Worker {
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl Worker {
    fn stop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

impl CameraSource for MfSource {
    fn open(&mut self, req: &OpenRequest, sink: Arc<dyn FrameSink>) -> Result<SourceInfo, CmError> {
        self.close();
        let (tx, rx) = mpsc::channel::<Result<SourceInfo, CmError>>();
        let stop = Arc::new(AtomicBool::new(false));
        let join = {
            let (req, stop) = (req.clone(), stop.clone());
            std::thread::Builder::new()
                .name("cm-camera-mf".into())
                .spawn(move || run(&req, sink, &stop, &tx))
                .map_err(|e| CmError::internal(format!("camera thread: {e}")))?
        };
        let mut worker = Worker {
            stop,
            join: Some(join),
        };
        match rx.recv_timeout(req.first_frame_timeout + OPEN_GRACE) {
            Ok(Ok(info)) => {
                self.worker = Some(worker);
                Ok(info)
            }
            Ok(Err(e)) => {
                worker.stop();
                Err(e)
            }
            Err(_) => {
                worker.stop();
                Err(CmError::device_busy(
                    "The camera did not deliver a picture. It may be in use by another application.",
                ))
            }
        }
    }

    fn close(&mut self) {
        if let Some(mut w) = self.worker.take() {
            w.stop();
        }
    }
}

impl Drop for MfSource {
    fn drop(&mut self) {
        self.close();
    }
}

/// Two u32s in the high and low half of one u64, how Media Foundation stores sizes and rates.
fn pack(hi: u32, lo: u32) -> u64 {
    (u64::from(hi) << 32) | u64::from(lo)
}

/// An output layout the reader can convert to, best first.
fn output_type(format: PixelFormat, size: (u32, u32), fps: (u32, u32)) -> Option<IMFMediaType> {
    // SAFETY: plain construction calls; the keys and GUIDs are constants.
    unsafe {
        let mt = MFCreateMediaType().ok()?;
        mt.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).ok()?;
        let sub = match format {
            PixelFormat::Nv12 => MFVideoFormat_NV12,
            PixelFormat::Yuy2 => MFVideoFormat_YUY2,
            PixelFormat::Bgra => MFVideoFormat_RGB32,
        };
        mt.SetGUID(&MF_MT_SUBTYPE, &sub).ok()?;
        mt.SetUINT64(&MF_MT_FRAME_SIZE, pack(size.0, size.1)).ok()?;
        mt.SetUINT64(&MF_MT_FRAME_RATE, pack(fps.0, fps.1)).ok()?;
        Some(mt)
    }
}

/// An opened camera: the reader and what its frames look like.
struct Started {
    reader: IMFSourceReader,
    source: IMFMediaSource,
    info: SourceInfo,
    format: PixelFormat,
    size: (u32, u32),
}

/// Opens the camera, chooses and sets a type, and returns the reader with the layout and size
/// frames will have.
fn start_reader(req: &OpenRequest) -> Result<Started, CmError> {
    let devices = enumerate_devices()?;
    let device = match &req.device_id {
        Some(id) => devices
            .iter()
            .find(|d| &d.link == id)
            .ok_or_else(|| CmError::not_found(format!("Camera {id} not found.")))?,
        None => devices
            .first()
            .ok_or_else(|| CmError::not_found("No camera found."))?,
    };
    let what = device.name.clone();
    // SAFETY: the activation object is live.
    let source = unsafe { device.activate.ActivateObject::<IMFMediaSource>() }
        .map_err(|e| map_error(&e, &what))?;
    let mut attrs = None;
    // SAFETY: `attrs` is a valid out slot; the key is a constant.
    unsafe {
        MFCreateAttributes(&mut attrs, 1).map_err(|e| map_error(&e, &what))?;
        if let Some(a) = &attrs {
            // Lets the reader decode Motion JPEG and convert to NV12/YUY2/RGB32.
            a.SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1)
                .map_err(|e| map_error(&e, &what))?;
        }
    }
    // SAFETY: `source` is live; `attrs` is a valid attribute store.
    let reader = unsafe { MFCreateSourceReaderFromMediaSource(&source, attrs.as_ref()) }
        .map_err(|e| map_error(&e, &what))?;
    let types = native_types(&reader);
    let raw: Vec<_> = types.iter().map(|(m, _)| *m).collect();
    let idx = pick_mode(&raw, req.resolution.height(), req.fps)
        .ok_or_else(|| CmError::invalid_input(format!("{what} offers no video mode.")))?;
    let (mode, native) = &types[idx];
    // SAFETY: both media types are live.
    unsafe { reader.SetCurrentMediaType(FIRST_VIDEO_STREAM, None, native) }
        .map_err(|e| map_error(&e, &what))?;
    let fps = (req.fps.max(1), 1);
    let mut chosen = None;
    for format in [PixelFormat::Nv12, PixelFormat::Yuy2, PixelFormat::Bgra] {
        let Some(mt) = output_type(format, (mode.width, mode.height), fps) else {
            continue;
        };
        // SAFETY: the reader and the type are live.
        if unsafe { reader.SetCurrentMediaType(FIRST_VIDEO_STREAM, None, &mt) }.is_ok() {
            chosen = Some(format);
            break;
        }
    }
    let format = chosen.ok_or_else(|| {
        CmError::invalid_input(format!(
            "{what} gives no picture layout CaptureMann can use."
        ))
    })?;
    let info = SourceInfo {
        device_id: device.link.clone(),
        device_name: device.name.clone(),
    };
    Ok(Started {
        reader,
        source,
        info,
        format,
        size: (mode.width, mode.height),
    })
}

/// Copies one sample to tight, top-down bytes.
fn copy_frame(sample: &IMFSample, format: PixelFormat, size: (u32, u32)) -> Option<RawFrame> {
    let (w, h) = size;
    let row = match format {
        PixelFormat::Nv12 => w as usize,
        PixelFormat::Yuy2 => (w as usize).div_ceil(2) * 4,
        PixelFormat::Bgra => w as usize * 4,
    };
    let rows = match format {
        PixelFormat::Nv12 => h as usize + (h as usize).div_ceil(2),
        _ => h as usize,
    };
    // SAFETY: the sample is live.
    let buffer = unsafe { sample.ConvertToContiguousBuffer() }.ok()?;
    let mut data = vec![0u8; row * rows];
    if let Ok(b2d) = buffer.cast::<IMF2DBuffer>() {
        let mut base: *mut u8 = std::ptr::null_mut();
        let mut pitch = 0i32;
        // SAFETY: Lock2D fills `base`/`pitch`; the rows are read inside the locked buffer, each
        // `row` bytes at `pitch` apart (negative pitch walks a bottom-up image top-down), and the
        // buffer is unlocked before returning.
        unsafe {
            b2d.Lock2D(&mut base, &mut pitch).ok()?;
            if base.is_null() {
                let _ = b2d.Unlock2D();
                return None;
            }
            for r in 0..rows {
                let src = base.offset(r as isize * pitch as isize);
                std::ptr::copy_nonoverlapping(src, data.as_mut_ptr().add(r * row), row);
            }
            let _ = b2d.Unlock2D();
        }
    } else {
        let mut base: *mut u8 = std::ptr::null_mut();
        let mut len = 0u32;
        // SAFETY: Lock fills `base` and the current length; at most that many bytes are copied
        // before Unlock.
        unsafe {
            buffer.Lock(&mut base, None, Some(&mut len)).ok()?;
            let n = data.len().min(len as usize);
            if !base.is_null() {
                std::ptr::copy_nonoverlapping(base, data.as_mut_ptr(), n);
            }
            let _ = buffer.Unlock();
        }
    }
    Some(RawFrame {
        format,
        width: w,
        height: h,
        stride: row,
        data: Arc::new(data),
    })
}

fn run(
    req: &OpenRequest,
    sink: Arc<dyn FrameSink>,
    stop: &AtomicBool,
    ready: &mpsc::Sender<Result<SourceInfo, CmError>>,
) {
    let _rt = match MfRuntime::start() {
        Ok(rt) => rt,
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };
    let Started {
        reader,
        source,
        info,
        format,
        size,
    } = match start_reader(req) {
        Ok(x) => x,
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };
    let clock = SystemHostClock;
    let mut announced = false;
    let mut last_preview: Option<u64> = None;
    while !stop.load(Ordering::SeqCst) {
        let mut flags = 0u32;
        let mut stamp = 0i64;
        let mut sample: Option<IMFSample> = None;
        // SAFETY: the out parameters are valid for writes for the duration of the call.
        let read = unsafe {
            reader.ReadSample(
                FIRST_VIDEO_STREAM,
                0,
                None,
                Some(&mut flags),
                Some(&mut stamp),
                Some(&mut sample),
            )
        };
        let delivered = clock.now_ns();
        if let Err(e) = read {
            let event = match super::mf::classify(&e) {
                Failure::Denied => CameraEvent::PermissionRevoked,
                Failure::Gone => CameraEvent::Disconnected,
                _ => CameraEvent::Error(e.to_string()),
            };
            if announced {
                sink.on_event(event);
            } else {
                let _ = ready.send(Err(map_error(&e, &info.device_name)));
            }
            break;
        }
        #[allow(clippy::unnecessary_cast)]
        if flags & (MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 | MF_SOURCE_READERF_ERROR.0 as u32) != 0
        {
            if announced {
                sink.on_event(CameraEvent::Disconnected);
            } else {
                let _ = ready.send(Err(CmError::not_found(format!(
                    "{} stopped before its first picture.",
                    info.device_name
                ))));
            }
            break;
        }
        let Some(sample) = sample else { continue };
        if let SourceRole::Preview { fps } = req.role {
            if !preview_due(last_preview, delivered, fps) {
                continue;
            }
            last_preview = Some(delivered);
        }
        let Some(raw) = copy_frame(&sample, format, size) else {
            continue;
        };
        let frame = CameraFrame {
            capture_host_ns: capture_host_ns(stamp, delivered),
            delivered_host_ns: delivered,
            width: size.0,
            height: size.1,
            native: Some(NativeSample(raw)),
        };
        sink.on_frame(&frame);
        if !announced {
            announced = true;
            let _ = ready.send(Ok(info.clone()));
        }
    }
    // SAFETY: releases the camera; safe to call after an error as well.
    unsafe {
        let _ = source.Shutdown();
    }
}
