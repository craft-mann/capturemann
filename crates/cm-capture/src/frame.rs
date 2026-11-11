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

    /// A copy whose longer edge is at most `max_edge` px (aspect kept, each side at least 1 px),
    /// averaged over the exact source area of every output pixel (a box filter with fractional
    /// edge weights, alpha-weighted). A frame already within `max_edge` is returned unchanged.
    /// `max_edge` 0 is `invalid_input`.
    pub fn downscaled(&self, max_edge: u32) -> Result<Frame, CmError> {
        if max_edge == 0 {
            return Err(CmError::invalid_input("max_edge must be at least 1"));
        }
        let longer = self.width.max(self.height);
        if longer <= max_edge {
            return Ok(self.clone());
        }
        let scale = f64::from(max_edge) / f64::from(longer);
        let dw = ((f64::from(self.width) * scale).round() as u32).clamp(1, max_edge);
        let dh = ((f64::from(self.height) * scale).round() as u32).clamp(1, max_edge);
        let xs = axis_spans(self.width, dw);
        let ys = axis_spans(self.height, dh);
        let stride = self.width as usize * 4;
        let mut out = Vec::with_capacity(dw as usize * dh as usize * 4);
        for yspan in &ys {
            for xspan in &xs {
                let (mut r, mut g, mut b, mut a, mut wsum) =
                    (0.0f64, 0.0f64, 0.0f64, 0.0f64, 0.0f64);
                for &(sy, wy) in yspan {
                    let row = sy as usize * stride;
                    for &(sx, wx) in xspan {
                        let i = row + sx as usize * 4;
                        let w = wx * wy;
                        let pa = f64::from(self.rgba[i + 3]);
                        r += f64::from(self.rgba[i]) * pa * w;
                        g += f64::from(self.rgba[i + 1]) * pa * w;
                        b += f64::from(self.rgba[i + 2]) * pa * w;
                        a += pa * w;
                        wsum += w;
                    }
                }
                if a <= 0.0 || wsum <= 0.0 {
                    out.extend_from_slice(&[0, 0, 0, 0]);
                } else {
                    out.push((r / a).round().clamp(0.0, 255.0) as u8);
                    out.push((g / a).round().clamp(0.0, 255.0) as u8);
                    out.push((b / a).round().clamp(0.0, 255.0) as u8);
                    out.push((a / wsum).round().clamp(0.0, 255.0) as u8);
                }
            }
        }
        Frame::new(dw, dh, out)
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

/// For each of `dst` output pixels along one axis, the source pixels it covers with their
/// overlap lengths (fractional at the two ends).
fn axis_spans(src: u32, dst: u32) -> Vec<Vec<(u32, f64)>> {
    let ratio = f64::from(src) / f64::from(dst);
    (0..dst)
        .map(|d| {
            let (a, b) = (f64::from(d) * ratio, f64::from(d + 1) * ratio);
            let first = a.floor() as u32;
            let last = (b.ceil() as u32).min(src);
            (first..last)
                .filter_map(|s| {
                    let w = (b.min(f64::from(s) + 1.0) - a.max(f64::from(s))).max(0.0);
                    (w > 0.0).then_some((s, w))
                })
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use cm_core::PhysicalPoint;

    fn solid(w: u32, h: u32, px: [u8; 4]) -> Frame {
        Frame::new(w, h, px.repeat(w as usize * h as usize)).unwrap()
    }

    #[test]
    fn downscaled_keeps_a_small_frame_as_is() {
        let f = gradient(40, 30);
        assert_eq!(f.downscaled(40).unwrap(), f);
        assert_eq!(f.downscaled(360).unwrap(), f);
        assert!(f.downscaled(0).is_err());
    }

    #[test]
    fn downscaled_keeps_aspect_and_limits_the_longer_edge() {
        let d = solid(300, 100, [9, 8, 7, 255]).downscaled(60).unwrap();
        assert_eq!((d.width(), d.height()), (60, 20));
        let d = solid(100, 300, [9, 8, 7, 255]).downscaled(60).unwrap();
        assert_eq!((d.width(), d.height()), (20, 60));
        // Odd sizes: 101x51 -> 50 wide, 25.25 rounds to 25 high; never above the limit.
        let d = solid(101, 51, [1, 2, 3, 255]).downscaled(50).unwrap();
        assert_eq!((d.width(), d.height()), (50, 25));
        assert!(d
            .rgba()
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| *p == [1, 2, 3, 255]));
    }

    #[test]
    fn downscaled_never_makes_a_side_zero() {
        let d = solid(1000, 1, [5, 5, 5, 255]).downscaled(10).unwrap();
        assert_eq!((d.width(), d.height()), (10, 1));
        let d = solid(1, 1, [5, 5, 5, 255]).downscaled(1).unwrap();
        assert_eq!((d.width(), d.height()), (1, 1));
        let d = solid(7, 5, [5, 5, 5, 255]).downscaled(1).unwrap();
        assert_eq!((d.width(), d.height()), (1, 1));
    }

    #[test]
    fn downscaled_averages_the_covered_pixels() {
        // Left half black, right half white, halved to 2x1: exact halves.
        let mut v = Vec::new();
        for _ in 0..2 {
            for x in 0..4 {
                let c = if x < 2 { 0 } else { 255 };
                v.extend_from_slice(&[c, c, c, 255]);
            }
        }
        let d = Frame::new(4, 2, v).unwrap().downscaled(2).unwrap();
        assert_eq!((d.width(), d.height()), (2, 1));
        assert_eq!(d.pixel(0, 0), Some([0, 0, 0, 255]));
        assert_eq!(d.pixel(1, 0), Some([255, 255, 255, 255]));
        // A 3 -> 2 split blends the middle pixel: (0 * 1 + 255 * 0.5) / 1.5 = 85 in the left.
        let v: Vec<u8> = [0u8, 255, 255]
            .iter()
            .flat_map(|&c| [c, c, c, 255])
            .collect();
        let d = Frame::new(3, 1, v).unwrap().downscaled(2).unwrap();
        assert_eq!(d.pixel(0, 0), Some([85, 85, 85, 255]));
        assert_eq!(d.pixel(1, 0), Some([255, 255, 255, 255]));
    }

    #[test]
    fn downscaled_ignores_transparent_pixels_in_the_colour() {
        let v = vec![255, 0, 0, 255, 0, 255, 0, 0];
        let d = Frame::new(2, 1, v).unwrap().downscaled(1).unwrap();
        assert_eq!(d.pixel(0, 0), Some([255, 0, 0, 128]));
    }

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
