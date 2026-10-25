use cm_core::{CmError, PhysicalRect, PhysicalSize};

/// An RGBA8 (non-premultiplied, row-major, no padding) image at physical size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

impl Frame {
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Result<Self, CmError> {
        let expected = (width as usize)
            .checked_mul(height as usize)
            .and_then(|n| n.checked_mul(4));
        if expected != Some(rgba.len()) {
            return Err(CmError::invalid_input(format!(
                "pixel buffer has {} bytes, expected {width}x{height}x4",
                rgba.len()
            )));
        }
        Ok(Frame {
            width,
            height,
            rgba,
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn size(&self) -> PhysicalSize {
        PhysicalSize {
            width: self.width,
            height: self.height,
        }
    }

    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }

    pub fn into_rgba(self) -> Vec<u8> {
        self.rgba
    }

    /// The pixel at (x, y) as `[r, g, b, a]`, or `None` outside the image.
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let i = (y as usize * self.width as usize + x as usize) * 4;
        let p = self.rgba.get(i..i + 4)?;
        Some([p[0], p[1], p[2], p[3]])
    }

    /// Copies out `rect` (physical px, top-left origin). Pixel-exact; no resampling.
    pub fn crop(&self, rect: PhysicalRect) -> Result<Frame, CmError> {
        let (x, y) = (i64::from(rect.origin.x), i64::from(rect.origin.y));
        let (w, h) = (i64::from(rect.size.width), i64::from(rect.size.height));
        if w == 0 || h == 0 {
            return Err(CmError::invalid_input("Region is empty"));
        }
        if x < 0 || y < 0 || x + w > i64::from(self.width) || y + h > i64::from(self.height) {
            return Err(CmError::invalid_input(format!(
                "Region {w}x{h} at ({x}, {y}) is outside the {}x{} image",
                self.width, self.height
            )));
        }
        let (x, w, h) = (x as usize, w as usize, h as usize);
        let stride = self.width as usize * 4;
        let mut out = Vec::with_capacity(w * h * 4);
        for row in y as usize..y as usize + h {
            let start = row * stride + x * 4;
            out.extend_from_slice(&self.rgba[start..start + w * 4]);
        }
        Frame::new(rect.size.width, rect.size.height, out)
    }

    /// Encodes as an 8-bit RGBA PNG.
    pub fn to_png(&self) -> Result<Vec<u8>, CmError> {
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, self.width, self.height);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc
            .write_header()
            .map_err(|e| CmError::internal(format!("PNG encoding failed: {e}")))?;
        writer
            .write_image_data(&self.rgba)
            .map_err(|e| CmError::internal(format!("PNG encoding failed: {e}")))?;
        writer
            .finish()
            .map_err(|e| CmError::internal(format!("PNG encoding failed: {e}")))?;
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cm_core::PhysicalPoint;

    fn rect(x: i32, y: i32, w: u32, h: u32) -> PhysicalRect {
        PhysicalRect {
            origin: PhysicalPoint { x, y },
            size: PhysicalSize {
                width: w,
                height: h,
            },
        }
    }

    fn gradient(w: u32, h: u32) -> Frame {
        let mut v = Vec::new();
        for y in 0..h {
            for x in 0..w {
                v.extend_from_slice(&[x as u8, y as u8, 7, 255]);
            }
        }
        Frame::new(w, h, v).unwrap()
    }

    #[test]
    fn new_rejects_wrong_length() {
        assert!(Frame::new(2, 2, vec![0; 15]).is_err());
    }

    #[test]
    fn crop_is_pixel_exact() {
        let f = gradient(10, 8);
        let c = f.crop(rect(3, 2, 4, 5)).unwrap();
        assert_eq!((c.width(), c.height()), (4, 5));
        assert_eq!(c.pixel(0, 0), Some([3, 2, 7, 255]));
        assert_eq!(c.pixel(3, 4), Some([6, 6, 7, 255]));
    }

    #[test]
    fn crop_rejects_out_of_bounds_and_empty() {
        let f = gradient(10, 8);
        assert!(f.crop(rect(8, 0, 3, 1)).is_err());
        assert!(f.crop(rect(-1, 0, 3, 1)).is_err());
        assert!(f.crop(rect(0, 0, 0, 1)).is_err());
        assert!(f.crop(rect(7, 8, 1, 1)).is_err());
        assert!(f.crop(rect(0, 0, 10, 8)).is_ok());
    }

    #[test]
    fn png_round_trips() {
        let f = gradient(5, 3);
        let bytes = f.to_png().unwrap();
        let dec = png::Decoder::new(std::io::Cursor::new(bytes));
        let mut reader = dec.read_info().unwrap();
        let mut buf = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut buf).unwrap();
        assert_eq!((info.width, info.height), (5, 3));
        assert_eq!(&buf[..info.buffer_size()], f.rgba());
    }
}
