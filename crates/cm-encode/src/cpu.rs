//! A frame in CPU memory: what a software-path source (PipeWire memory buffers on Linux) delivers
//! and what [`crate::ffmpeg_vp9::FfmpegVp9Encoder`] consumes. Pure Rust, tested on every OS.

use std::borrow::Cow;
use std::sync::Arc;

use cm_core::CmError;

/// Pixel layouts a screen source can deliver in memory (the names are PipeWire/SPA's, the byte
/// order is memory order: `Bgrx` is B, G, R, unused).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PixelFormat {
    Bgrx,
    Bgra,
    Rgbx,
    Rgba,
    /// Y plane (`stride` bytes per row) followed by an interleaved UV plane at the same stride.
    Nv12,
}

impl PixelFormat {
    /// Bytes per pixel of a packed format; `None` for NV12.
    pub fn bytes_per_pixel(self) -> Option<usize> {
        match self {
            PixelFormat::Bgrx | PixelFormat::Bgra | PixelFormat::Rgbx | PixelFormat::Rgba => {
                Some(4)
            }
            PixelFormat::Nv12 => None,
        }
    }

    /// The FFmpeg `-pix_fmt` name for raw input in this layout.
    pub fn ffmpeg_name(self) -> &'static str {
        match self {
            PixelFormat::Bgrx => "bgr0",
            PixelFormat::Bgra => "bgra",
            PixelFormat::Rgbx => "rgb0",
            PixelFormat::Rgba => "rgba",
            PixelFormat::Nv12 => "nv12",
        }
    }

    /// Bytes of one tightly packed frame.
    pub fn packed_len(self, width: u32, height: u32) -> usize {
        let (w, h) = (width as usize, height as usize);
        match self.bytes_per_pixel() {
            Some(b) => w * h * b,
            None => w * h + w * h.div_ceil(2),
        }
    }
}

/// One frame. Cloning shares the pixels.
#[derive(Debug, Clone)]
pub struct CpuFrame {
    pub width: u32,
    pub height: u32,
    /// Bytes from the start of one row to the next (>= the row's pixel bytes).
    pub stride: usize,
    pub format: PixelFormat,
    pub data: Arc<Vec<u8>>,
}

impl CpuFrame {
    /// Checks that `data` really holds `height` rows of `stride` bytes.
    pub fn new(
        width: u32,
        height: u32,
        stride: usize,
        format: PixelFormat,
        data: Vec<u8>,
    ) -> Result<Self, CmError> {
        let f = CpuFrame {
            width,
            height,
            stride,
            format,
            data: Arc::new(data),
        };
        f.validate()?;
        Ok(f)
    }

    fn row_bytes(&self) -> usize {
        match self.format.bytes_per_pixel() {
            Some(b) => self.width as usize * b,
            None => self.width as usize,
        }
    }

    fn rows(&self) -> usize {
        match self.format {
            PixelFormat::Nv12 => self.height as usize + (self.height as usize).div_ceil(2),
            _ => self.height as usize,
        }
    }

    /// Size and stride are consistent with the data length.
    pub fn validate(&self) -> Result<(), CmError> {
        if self.width == 0 || self.height == 0 {
            return Err(CmError::invalid_input("frame has no pixels"));
        }
        if self.stride < self.row_bytes() {
            return Err(CmError::invalid_input(format!(
                "stride {} is smaller than a row ({} bytes)",
                self.stride,
                self.row_bytes()
            )));
        }
        // The last row needs only its pixel bytes, not the padding.
        let need = (self.rows() - 1) * self.stride + self.row_bytes();
        if self.data.len() < need {
            return Err(CmError::invalid_input(format!(
                "frame data is {} bytes, {}x{} {:?} with stride {} needs {need}",
                self.data.len(),
                self.width,
                self.height,
                self.format,
                self.stride
            )));
        }
        Ok(())
    }

    /// The pixels without row padding (borrowed when there is none).
    pub fn packed(&self) -> Cow<'_, [u8]> {
        let rb = self.row_bytes();
        let rows = self.rows();
        if self.stride == rb && self.data.len() == rb * rows {
            return Cow::Borrowed(&self.data[..]);
        }
        let mut out = Vec::with_capacity(rb * rows);
        for r in 0..rows {
            let s = r * self.stride;
            out.extend_from_slice(&self.data[s..s + rb]);
        }
        Cow::Owned(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed_drops_row_padding() {
        // 2x2 BGRx with stride 12 (4 bytes of padding after the first row).
        let mut d = vec![0u8; 12 + 8];
        d[..8].copy_from_slice(&[1; 8]);
        d[12..20].copy_from_slice(&[2; 8]);
        let f = CpuFrame::new(2, 2, 12, PixelFormat::Bgrx, d).unwrap();
        assert_eq!(
            &*f.packed(),
            &[1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 2, 2, 2]
        );
    }

    #[test]
    fn tight_frames_are_not_copied() {
        let f = CpuFrame::new(2, 1, 8, PixelFormat::Rgba, vec![7; 8]).unwrap();
        assert!(matches!(f.packed(), Cow::Borrowed(_)));
    }

    #[test]
    fn short_data_and_small_stride_are_rejected() {
        assert!(CpuFrame::new(2, 2, 8, PixelFormat::Bgra, vec![0; 15]).is_err());
        assert!(CpuFrame::new(2, 2, 4, PixelFormat::Bgra, vec![0; 64]).is_err());
        assert!(CpuFrame::new(0, 2, 8, PixelFormat::Bgra, vec![0; 64]).is_err());
    }

    #[test]
    fn nv12_has_a_chroma_plane() {
        assert_eq!(PixelFormat::Nv12.packed_len(4, 4), 24);
        let f = CpuFrame::new(4, 4, 4, PixelFormat::Nv12, vec![0; 24]).unwrap();
        assert_eq!(f.packed().len(), 24);
        assert!(CpuFrame::new(4, 4, 4, PixelFormat::Nv12, vec![0; 23]).is_err());
    }
}
