//! Camera enumeration for `list_devices`: names, kinds and capture modes. Reading the modes opens
//! each camera briefly; a camera that cannot be opened is still listed, without modes.

use std::time::Duration;

use windows::Win32::Media::MediaFoundation::{IMFMediaSource, MFCreateSourceReaderFromMediaSource};

use cm_core::CmError;

use super::mf::{enumerate_devices, native_types, Device, MfRuntime};
use crate::convert::{classify_camera, fold_modes};
use crate::permission::CameraPermission;
use crate::types::{CameraInfo, CameraMode};

pub use super::consent::permission_status;

/// Windows shows no prompt to a desktop app: the answer is the current state of the switches.
pub fn request_access(_timeout: Duration) -> CameraPermission {
    permission_status()
}

fn modes_of(device: &Device) -> Vec<CameraMode> {
    // SAFETY: the activation object is live; the source is shut down before it is dropped.
    let Ok(source) = (unsafe { device.activate.ActivateObject::<IMFMediaSource>() }) else {
        return Vec::new();
    };
    // SAFETY: `source` is a live media source; no attributes are passed.
    let modes = unsafe { MFCreateSourceReaderFromMediaSource(&source, None) }
        .map(|reader| {
            let raw: Vec<_> = native_types(&reader).into_iter().map(|(m, _)| m).collect();
            fold_modes(&raw)
        })
        .unwrap_or_default();
    // SAFETY: releases the camera; both calls are safe to repeat.
    unsafe {
        let _ = source.Shutdown();
        let _ = device.activate.ShutdownObject();
    }
    modes
}

pub fn list_cameras() -> Result<Vec<CameraInfo>, CmError> {
    let _rt = MfRuntime::start()?;
    let devices = enumerate_devices()?;
    Ok(devices
        .iter()
        .enumerate()
        .map(|(i, d)| {
            let modes = modes_of(d);
            let top = modes.first().copied();
            CameraInfo {
                id: d.link.clone(),
                name: d.name.clone(),
                kind: classify_camera(&d.name, &d.link),
                is_default: i == 0,
                max_size: top.map_or([0, 0], |m| [m.width, m.height]),
                max_fps: modes.iter().map(|m| m.max_fps).fold(0.0, f64::max),
                modes,
                in_use_elsewhere: false,
            }
        })
        .collect())
}
