//! The platform frame handle: a retained `CMSampleBuffer`.

use std::fmt;
use std::ptr::NonNull;

use objc2_core_foundation::CFRetained;
use objc2_core_media::CMSampleBuffer;
use objc2_core_video::{
    CVPixelBufferGetBaseAddress, CVPixelBufferGetBytesPerRow, CVPixelBufferGetHeight,
    CVPixelBufferGetIOSurface, CVPixelBufferGetPixelFormatType, CVPixelBufferGetWidth,
    CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags, CVPixelBufferUnlockBaseAddress,
};

use super::SendBox;
use crate::preview::{scaled_size, RgbaImage};

const FOURCC_BGRA: u32 = 0x4247_5241; // kCVPixelFormatType_32BGRA

/// A reference-counted camera sample. Cloning retains; no pixels are copied.
#[derive(Clone)]
pub struct NativeSample {
    pub(super) sb: std::sync::Arc<SendBox<CFRetained<CMSampleBuffer>>>,
}

impl fmt::Debug for NativeSample {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("NativeSample(CMSampleBuffer)")
    }
}

impl NativeSample {
    /// Retains `sb` for as long as the handle lives.
    pub(super) fn retain(sb: &CMSampleBuffer) -> Self {
        // SAFETY: `sb` is a valid CMSampleBuffer reference for this call; `retain` takes its own
        // +1 reference, so the buffer outlives the callback that handed it to us.
        let retained = unsafe { CFRetained::retain(NonNull::from(sb)) };
        NativeSample {
            sb: std::sync::Arc::new(SendBox(retained)),
        }
    }

    pub(super) fn buffer(&self) -> &CMSampleBuffer {
        &self.sb.0
    }

    /// BGRA pixel buffers only (the preview output asks for BGRA). Nearest-neighbour downscale.
    pub fn to_rgba(&self, max_edge: u32, mirror: bool) -> Option<RgbaImage> {
        // SAFETY: the sample buffer is alive (we hold a reference); `image_buffer` returns a
        // retained pixel buffer or None.
        let pb = unsafe { self.buffer().image_buffer() }?;
        if CVPixelBufferGetPixelFormatType(&pb) != FOURCC_BGRA {
            return None;
        }
        let (sw, sh) = (
            u32::try_from(CVPixelBufferGetWidth(&pb)).ok()?,
            u32::try_from(CVPixelBufferGetHeight(&pb)).ok()?,
        );
        let (tw, th) = scaled_size(sw, sh, max_edge);
        let stride = CVPixelBufferGetBytesPerRow(&pb);
        // SAFETY: lock/unlock are paired below; the base address is valid and
        // `stride * height` bytes long while locked, and we only read inside that range
        // (x < sw, y < sh, 4 bytes per pixel, stride >= 4 * sw for BGRA buffers).
        unsafe {
            if CVPixelBufferLockBaseAddress(&pb, CVPixelBufferLockFlags::ReadOnly) != 0 {
                return None;
            }
            let base = CVPixelBufferGetBaseAddress(&pb).cast::<u8>().cast_const();
            let mut out = Vec::with_capacity((tw * th * 4) as usize);
            if !base.is_null() && stride >= 4 * sw as usize {
                for y in 0..th {
                    let sy = (u64::from(y) * u64::from(sh) / u64::from(th)) as usize;
                    let row = std::slice::from_raw_parts(base.add(sy * stride), 4 * sw as usize);
                    for x in 0..tw {
                        let dx = if mirror { tw - 1 - x } else { x };
                        let sx = (u64::from(dx) * u64::from(sw) / u64::from(tw)) as usize * 4;
                        out.extend_from_slice(&[row[sx + 2], row[sx + 1], row[sx], 255]);
                    }
                }
            }
            CVPixelBufferUnlockBaseAddress(&pb, CVPixelBufferLockFlags::ReadOnly);
            (out.len() == (tw * th * 4) as usize).then_some(RgbaImage {
                width: tw,
                height: th,
                data: out,
            })
        }
    }

    pub fn io_surface_id(&self) -> Option<u32> {
        // SAFETY: as in `to_rgba`; the IOSurface is retained by the call.
        let pb = unsafe { self.buffer().image_buffer() }?;
        CVPixelBufferGetIOSurface(Some(&pb)).map(|s| s.id())
    }
}
