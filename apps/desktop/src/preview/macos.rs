//! The zero-extra-copy macOS path: an IOSurface-backed NV12 `CVPixelBuffer` that GPUI's
//! `surface()` element draws through a CoreVideo Metal texture cache.
//!
//! GPUI's Metal renderer asserts the buffer is `420YpCbCr8BiPlanarFullRange` (NV12, full
//! range), so that is the format a decoder (VideoToolbox) or ScreenCaptureKit hands over, and the
//! format the compositor output would have to be converted to before it can reach `surface()`.

use std::ptr::NonNull;

use objc2_core_foundation::{CFDictionary, CFRetained, CFString, CFType};
use objc2_core_video::{
    kCVPixelBufferIOSurfacePropertiesKey, kCVPixelFormatType_420YpCbCr8BiPlanarFullRange,
    kCVReturnSuccess, CVPixelBuffer, CVPixelBufferCreate, CVPixelBufferGetBaseAddressOfPlane,
    CVPixelBufferGetBytesPerRowOfPlane, CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags,
    CVPixelBufferUnlockBaseAddress,
};

use super::frame::Nv12Frame;

/// A pixel buffer that may cross threads. CoreVideo pixel buffers are reference counted and
/// immutable once unlocked, which is how decoders hand them between threads.
pub struct SendBuffer(pub CFRetained<CVPixelBuffer>);

// SAFETY: see the type docs; the buffer is only read after it is filled and unlocked.
unsafe impl Send for SendBuffer {}

/// Creates an IOSurface-backed NV12 buffer and copies `frame` into it.
pub fn pixel_buffer(frame: &Nv12Frame) -> Result<SendBuffer, String> {
    let empty = CFDictionary::<CFString, CFType>::from_slices(&[], &[]);
    let attrs = {
        // SAFETY: a static CFString key owned by CoreVideo.
        let key = unsafe { kCVPixelBufferIOSurfacePropertiesKey };
        CFDictionary::<CFString, CFDictionary>::from_slices(&[key], &[empty.as_opaque()])
    };
    let mut raw: *mut CVPixelBuffer = std::ptr::null_mut();
    // SAFETY: valid size and format; `raw` is a valid out pointer.
    let status = unsafe {
        CVPixelBufferCreate(
            None,
            frame.width,
            frame.height,
            kCVPixelFormatType_420YpCbCr8BiPlanarFullRange,
            Some(attrs.as_opaque()),
            NonNull::from(&mut raw),
        )
    };
    if status != kCVReturnSuccess {
        // i18n-ignore: technical detail for the log
        return Err(format!("CVPixelBufferCreate failed: {status}"));
    }
    let raw = NonNull::new(raw).ok_or("CVPixelBufferCreate returned null")?;
    // SAFETY: Create follows the CF create rule: we own one retain.
    let buf = unsafe { CFRetained::from_raw(raw) };

    let flags = CVPixelBufferLockFlags(0);
    // SAFETY: the buffer is locked for the copy and unlocked after it.
    unsafe {
        let status = CVPixelBufferLockBaseAddress(&buf, flags);
        if status != kCVReturnSuccess {
            // i18n-ignore: technical detail for the log
            return Err(format!("CVPixelBufferLockBaseAddress failed: {status}"));
        }
        copy_plane(&buf, 0, &frame.y, frame.width, frame.height);
        copy_plane(&buf, 1, &frame.uv, frame.width, frame.height / 2);
        CVPixelBufferUnlockBaseAddress(&buf, flags);
    }
    Ok(SendBuffer(buf))
}

/// Copies `rows` rows of `row_bytes` into a locked plane, honouring the plane's stride.
///
/// # Safety
/// The buffer must be locked and `plane` valid for it.
unsafe fn copy_plane(buf: &CVPixelBuffer, plane: usize, src: &[u8], row_bytes: usize, rows: usize) {
    let base = CVPixelBufferGetBaseAddressOfPlane(buf, plane).cast::<u8>();
    let stride = CVPixelBufferGetBytesPerRowOfPlane(buf, plane);
    debug_assert!(!base.is_null() && stride >= row_bytes);
    for row in 0..rows {
        // SAFETY: the caller locked the buffer; `row` stays inside the plane and the source slice.
        unsafe {
            std::ptr::copy_nonoverlapping(
                src.as_ptr().add(row * row_bytes),
                base.add(row * stride),
                row_bytes,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preview::frame::SyntheticSource;
    use objc2_core_video::{
        CVPixelBufferGetHeight, CVPixelBufferGetPixelFormatType, CVPixelBufferGetWidth,
    };

    /// A 1080p frame becomes an NV12 full-range buffer of the same size (what GPUI asserts).
    #[test]
    fn pixel_buffer_is_nv12_full_range_at_the_frame_size() {
        let f = SyntheticSource::new(1920, 1080).frame(7);
        let b = pixel_buffer(&f).unwrap();
        assert_eq!(CVPixelBufferGetWidth(&b.0), 1920);
        assert_eq!(CVPixelBufferGetHeight(&b.0), 1080);
        assert_eq!(
            CVPixelBufferGetPixelFormatType(&b.0),
            kCVPixelFormatType_420YpCbCr8BiPlanarFullRange
        );
    }
}
