//! Permission status and device discovery.

use std::collections::HashSet;
use std::sync::mpsc;
use std::time::Duration;

use block2::RcBlock;
use cm_core::CmError;
use objc2::rc::Retained;
use objc2_av_foundation::{
    AVCaptureDevice, AVCaptureDeviceDiscoverySession, AVCaptureDevicePosition, AVMediaTypeVideo,
};
use objc2_core_media::CMVideoFormatDescriptionGetDimensions;
use objc2_foundation::{NSArray, NSString};

use crate::permission::CameraPermission;
use crate::types::{CameraInfo, CameraKind, CameraMode};

/// `AVCaptureDeviceType*` constants by value. The symbols exist only on newer macOS versions
/// (`External`, `ContinuityCamera` need 14/13), so they are looked up by string, which AVFoundation
/// ignores when unknown instead of failing to load the binary on older systems.
const DEVICE_TYPES: [&str; 5] = [
    "AVCaptureDeviceTypeBuiltInWideAngleCamera",
    "AVCaptureDeviceTypeExternal",
    "AVCaptureDeviceTypeExternalUnknown",
    "AVCaptureDeviceTypeContinuityCamera",
    "AVCaptureDeviceTypeDeskViewCamera",
];

const TRANSPORT_BUILT_IN: i32 = 0x626C_746E; // 'bltn'
const TRANSPORT_VIRTUAL: i32 = 0x7669_7274; // 'virt'

pub(super) fn video_media_type() -> Option<&'static objc2_av_foundation::AVMediaType> {
    // SAFETY: reading an immutable framework string constant.
    unsafe { AVMediaTypeVideo }
}

/// Current permission, never prompting.
pub fn permission_status() -> CameraPermission {
    let Some(mt) = video_media_type() else {
        return CameraPermission::NotDetermined;
    };
    // SAFETY: class method with a valid media type; no side effects.
    let raw = unsafe { AVCaptureDevice::authorizationStatusForMediaType(mt) };
    CameraPermission::from_av_status(raw.0 as i64)
}

/// Shows the system prompt when not determined, waits for the answer.
pub fn request_access(timeout: Duration) -> CameraPermission {
    let current = permission_status();
    if current != CameraPermission::NotDetermined {
        return current;
    }
    let Some(mt) = video_media_type() else {
        return current;
    };
    let (tx, rx) = mpsc::channel();
    let block = RcBlock::new(move |granted: objc2::runtime::Bool| {
        let _ = tx.send(granted.as_bool());
    });
    // SAFETY: the block is copied by the framework and only sends on a channel.
    unsafe { AVCaptureDevice::requestAccessForMediaType_completionHandler(mt, &block) };
    let _ = rx.recv_timeout(timeout);
    permission_status()
}

fn ns(s: &str) -> Retained<NSString> {
    NSString::from_str(s)
}

/// All video capture devices (discovery session plus the legacy list, which also contains
/// CoreMediaIO DAL virtual cameras), deduplicated by unique id.
pub(super) fn all_devices() -> Vec<Retained<AVCaptureDevice>> {
    let types: Vec<Retained<NSString>> = DEVICE_TYPES.iter().map(|t| ns(t)).collect();
    let refs: Vec<&NSString> = types.iter().map(|t| &**t).collect();
    let array = NSArray::from_slice(&refs);
    let mut out: Vec<Retained<AVCaptureDevice>> = Vec::new();
    let mut seen = HashSet::new();
    // SAFETY: plain Objective-C calls on valid objects; results are retained arrays.
    unsafe {
        let discovery =
            AVCaptureDeviceDiscoverySession::discoverySessionWithDeviceTypes_mediaType_position(
                &array,
                video_media_type(),
                AVCaptureDevicePosition::Unspecified,
            );
        for d in discovery.devices().iter() {
            if seen.insert(d.uniqueID().to_string()) {
                out.push(d);
            }
        }
        #[allow(deprecated)]
        if let Some(mt) = video_media_type() {
            for d in AVCaptureDevice::devicesWithMediaType(mt).iter() {
                if seen.insert(d.uniqueID().to_string()) {
                    out.push(d);
                }
            }
        }
    }
    out
}

/// Classifies from device type, transport and model (heuristics for the last two, see
/// `list_cameras` docs).
fn classify(device_type: &str, transport: i32, model: &str, name: &str) -> CameraKind {
    let lower = name.to_lowercase();
    if device_type == "AVCaptureDeviceTypeContinuityCamera"
        || model.starts_with("iPhone")
        || model.starts_with("iPad")
    {
        CameraKind::Continuity
    } else if transport == TRANSPORT_VIRTUAL || lower.contains("virtual") || lower.contains("obs ")
    {
        CameraKind::Virtual
    } else if transport == TRANSPORT_BUILT_IN {
        CameraKind::BuiltIn
    } else {
        CameraKind::External
    }
}

fn modes_of(d: &AVCaptureDevice) -> Vec<CameraMode> {
    let mut modes: Vec<CameraMode> = Vec::new();
    // SAFETY: formats and their descriptions are retained objects owned by the device.
    unsafe {
        for f in d.formats().iter() {
            let desc = f.formatDescription();
            let dim = CMVideoFormatDescriptionGetDimensions(&desc);
            let (Ok(width), Ok(height)) = (u32::try_from(dim.width), u32::try_from(dim.height))
            else {
                continue;
            };
            for r in f.videoSupportedFrameRateRanges().iter() {
                let (lo, hi) = (r.minFrameRate(), r.maxFrameRate());
                match modes
                    .iter_mut()
                    .find(|m| m.width == width && m.height == height)
                {
                    Some(m) => {
                        m.min_fps = m.min_fps.min(lo);
                        m.max_fps = m.max_fps.max(hi);
                    }
                    None => modes.push(CameraMode {
                        width,
                        height,
                        min_fps: lo,
                        max_fps: hi,
                    }),
                }
            }
        }
    }
    modes.sort_by_key(|m| std::cmp::Reverse(u64::from(m.width) * u64::from(m.height)));
    modes
}

pub fn list() -> Result<Vec<CameraInfo>, CmError> {
    let default_id = video_media_type().and_then(|mt| {
        // SAFETY: class method returning an optional retained device.
        unsafe { AVCaptureDevice::defaultDeviceWithMediaType(mt) }
            .map(|d| unsafe { d.uniqueID() }.to_string())
    });
    let mut cams: Vec<CameraInfo> = all_devices()
        .iter()
        .map(|d| {
            // SAFETY: property reads on a valid device.
            let (id, name, model, dtype, transport, busy) = unsafe {
                (
                    d.uniqueID().to_string(),
                    d.localizedName().to_string(),
                    d.modelID().to_string(),
                    d.deviceType().to_string(),
                    d.transportType(),
                    d.isInUseByAnotherApplication(),
                )
            };
            let modes = modes_of(d);
            let max = modes.first().map_or([0, 0], |m| [m.width, m.height]);
            CameraInfo {
                is_default: default_id.as_deref() == Some(id.as_str()),
                kind: classify(&dtype, transport, &model, &name),
                max_fps: modes.iter().map(|m| m.max_fps).fold(0.0, f64::max),
                max_size: max,
                modes,
                in_use_elsewhere: busy,
                id,
                name,
            }
        })
        .collect();
    // Default first, then built-in, then by name: a stable order for pickers.
    cams.sort_by(|a, b| {
        b.is_default
            .cmp(&a.is_default)
            .then((a.kind as u8).cmp(&(b.kind as u8)))
            .then(a.name.cmp(&b.name))
    });
    Ok(cams)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classification() {
        assert_eq!(
            classify("x", TRANSPORT_BUILT_IN, "Mac14,5", "FaceTime HD Camera"),
            CameraKind::BuiltIn
        );
        assert_eq!(
            classify("x", 0x7573_6220, "UVC", "Logitech C920"),
            CameraKind::External
        );
        assert_eq!(
            classify("x", TRANSPORT_VIRTUAL, "", "OBS Virtual Camera"),
            CameraKind::Virtual
        );
        assert_eq!(
            classify(
                "AVCaptureDeviceTypeContinuityCamera",
                0,
                "iPhone15,2",
                "Trung's iPhone Camera"
            ),
            CameraKind::Continuity
        );
        assert_eq!(
            classify("x", TRANSPORT_BUILT_IN, "iPhone15,2", "iPhone Camera"),
            CameraKind::Continuity
        );
    }
}
