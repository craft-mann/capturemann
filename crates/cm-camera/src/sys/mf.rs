//! Shared Media Foundation plumbing: COM and platform start-up for one thread, device activation
//! objects, media-type reading and error mapping.

use windows::core::{Error, GUID, PWSTR};
use windows::Win32::Foundation::E_ACCESSDENIED;
use windows::Win32::Media::MediaFoundation::{
    IMFActivate, IMFAttributes, IMFMediaType, IMFSourceReader, MFCreateAttributes,
    MFEnumDeviceSources, MFShutdown, MFStartup, MFVideoFormat_MJPG, MFVideoFormat_NV12,
    MFVideoFormat_RGB32, MFVideoFormat_YUY2, MFSTARTUP_FULL, MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME,
    MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE, MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID,
    MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK, MF_E_HW_MFT_FAILED_START_STREAMING,
    MF_E_NO_MORE_TYPES, MF_E_VIDEO_RECORDING_DEVICE_INVALIDATED,
    MF_E_VIDEO_RECORDING_DEVICE_PREEMPTED, MF_MT_FRAME_RATE, MF_MT_FRAME_RATE_RANGE_MAX,
    MF_MT_FRAME_RATE_RANGE_MIN, MF_MT_FRAME_SIZE, MF_MT_SUBTYPE, MF_VERSION,
};
use windows::Win32::System::Com::{
    CoInitializeEx, CoTaskMemFree, CoUninitialize, COINIT_MULTITHREADED,
};

use cm_core::{CmError, Permission};

use crate::convert::{RawMode, Subtype};

/// `MF_SOURCE_READER_FIRST_VIDEO_STREAM`.
pub const FIRST_VIDEO_STREAM: u32 = 0xFFFF_FFFC;

/// COM and Media Foundation started for the life of this value, on the thread that made it.
pub struct MfRuntime {
    com: bool,
}

impl MfRuntime {
    pub fn start() -> Result<Self, CmError> {
        // SAFETY: plain start-up calls with valid arguments. A thread that already runs COM in
        // another mode answers `RPC_E_CHANGED_MODE`; that is fine, only the uninitialize is skipped.
        let com = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
        // SAFETY: as above; MFStartup is reference counted.
        if let Err(e) = unsafe { MFStartup(MF_VERSION, MFSTARTUP_FULL) } {
            if com {
                // SAFETY: balances the successful CoInitializeEx above.
                unsafe { CoUninitialize() };
            }
            return Err(CmError::internal(format!(
                "Media Foundation did not start: {e}"
            )));
        }
        Ok(MfRuntime { com })
    }
}

impl Drop for MfRuntime {
    fn drop(&mut self) {
        // SAFETY: balances the successful MFStartup and CoInitializeEx of `start`.
        unsafe {
            let _ = MFShutdown();
            if self.com {
                CoUninitialize();
            }
        }
    }
}

/// A camera as Media Foundation lists it.
pub struct Device {
    pub activate: IMFActivate,
    pub name: String,
    pub link: String,
}

fn string_attr(a: &IMFActivate, key: &GUID) -> Option<String> {
    let mut p = PWSTR::null();
    let mut len = 0u32;
    // SAFETY: the out parameters are valid for writes; the allocated string is freed below.
    unsafe { a.GetAllocatedString(key, &mut p, &mut len) }.ok()?;
    // SAFETY: on success `p` points at `len` UTF-16 units that CoTaskMem owns.
    let s = unsafe { p.to_string() }.ok();
    // SAFETY: `p` was allocated by GetAllocatedString with CoTaskMemAlloc.
    unsafe { CoTaskMemFree(Some(p.0.cast())) };
    s
}

/// The cameras, in the order Windows lists them (the first is the one apps use by default).
pub fn enumerate_devices() -> Result<Vec<Device>, CmError> {
    let mut attrs: Option<IMFAttributes> = None;
    // SAFETY: `attrs` is a valid out slot.
    unsafe { MFCreateAttributes(&mut attrs, 1) }.map_err(|e| map_error(&e, "Camera list"))?;
    let attrs = attrs.ok_or_else(|| CmError::internal("Camera list: no attributes"))?;
    // SAFETY: both GUID references are valid for the call.
    unsafe {
        attrs.SetGUID(
            &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
            &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID,
        )
    }
    .map_err(|e| map_error(&e, "Camera list"))?;
    let mut list: *mut Option<IMFActivate> = std::ptr::null_mut();
    let mut count = 0u32;
    // SAFETY: the out parameters are valid; on success `list` points at `count` activation
    // objects in a CoTaskMem block that this function frees.
    unsafe { MFEnumDeviceSources(&attrs, &mut list, &mut count) }
        .map_err(|e| map_error(&e, "Camera list"))?;
    let mut out = Vec::new();
    for i in 0..count as usize {
        // SAFETY: `i < count`, so the slot is initialized; `read` takes ownership of the reference
        // and the block is freed below without dropping its slots.
        let slot = unsafe { list.add(i).read() };
        if let Some(activate) = slot {
            let name = string_attr(&activate, &MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME)
                .unwrap_or_else(|| "Camera".to_string());
            let link = string_attr(
                &activate,
                &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK,
            )
            .unwrap_or_default();
            out.push(Device {
                activate,
                name,
                link,
            });
        }
    }
    if !list.is_null() {
        // SAFETY: the block came from MFEnumDeviceSources (CoTaskMemAlloc).
        unsafe { CoTaskMemFree(Some(list.cast())) };
    }
    Ok(out)
}

fn subtype_of(guid: &GUID) -> Subtype {
    {
        if *guid == MFVideoFormat_NV12 {
            Subtype::Nv12
        } else if *guid == MFVideoFormat_YUY2 {
            Subtype::Yuy2
        } else if *guid == MFVideoFormat_MJPG {
            Subtype::Mjpg
        } else if *guid == MFVideoFormat_RGB32 {
            Subtype::Rgb32
        } else {
            Subtype::Other
        }
    }
}

fn unpack_ratio(v: u64) -> f64 {
    let (num, den) = ((v >> 32) as u32, v as u32);
    if den == 0 {
        0.0
    } else {
        f64::from(num) / f64::from(den)
    }
}

fn mode_of(mt: &IMFMediaType) -> RawMode {
    // SAFETY: the keys are constants and `mt` is a live media type.
    let (size, rate, lo, hi, sub) = unsafe {
        (
            mt.GetUINT64(&MF_MT_FRAME_SIZE).unwrap_or(0),
            mt.GetUINT64(&MF_MT_FRAME_RATE).unwrap_or(0),
            mt.GetUINT64(&MF_MT_FRAME_RATE_RANGE_MIN).unwrap_or(0),
            mt.GetUINT64(&MF_MT_FRAME_RATE_RANGE_MAX).unwrap_or(0),
            mt.GetGUID(&MF_MT_SUBTYPE).ok(),
        )
    };
    let fps = unpack_ratio(rate);
    let (min_fps, max_fps) = (unpack_ratio(lo), unpack_ratio(hi));
    RawMode {
        width: (size >> 32) as u32,
        height: size as u32,
        // A type without a range offers exactly its rate.
        min_fps: if min_fps > 0.0 { min_fps } else { fps },
        max_fps: if max_fps > 0.0 { max_fps } else { fps },
        subtype: sub.as_ref().map_or(Subtype::Other, subtype_of),
    }
}

/// The native media types of the first video stream, with their `IMFMediaType`s (index-aligned).
pub fn native_types(reader: &IMFSourceReader) -> Vec<(RawMode, IMFMediaType)> {
    let mut out = Vec::new();
    for i in 0..512u32 {
        // SAFETY: the stream index constant and the type index are plain integers.
        match unsafe { reader.GetNativeMediaType(FIRST_VIDEO_STREAM, i) } {
            Ok(mt) => out.push((mode_of(&mt), mt)),
            Err(e) if e.code() == MF_E_NO_MORE_TYPES => break,
            Err(_) => break,
        }
    }
    out
}

/// What a Media Foundation error means for the caller.
pub enum Failure {
    Denied,
    Busy,
    Gone,
    Other,
}

pub fn classify(e: &Error) -> Failure {
    let c = e.code();
    if c == E_ACCESSDENIED {
        Failure::Denied
    } else if c == MF_E_HW_MFT_FAILED_START_STREAMING || c == MF_E_VIDEO_RECORDING_DEVICE_PREEMPTED
    {
        Failure::Busy
    } else if c == MF_E_VIDEO_RECORDING_DEVICE_INVALIDATED {
        Failure::Gone
    } else {
        Failure::Other
    }
}

/// Maps a Media Foundation error to the shared error codes.
pub fn map_error(e: &Error, what: &str) -> CmError {
    match classify(e) {
        Failure::Denied => CmError::permission_missing(
            Permission::Camera,
            "Camera access is denied. Turn it on in Settings > Privacy & security > Camera, including Let desktop apps access your camera.",
        ),
        Failure::Busy => CmError::device_busy(format!(
            "{what} is in use by another application. Close it and try again."
        )),
        Failure::Gone => CmError::not_found(format!("{what} is not available (unplugged?)")),
        Failure::Other => CmError::internal(format!("{what}: {e}")),
    }
}
