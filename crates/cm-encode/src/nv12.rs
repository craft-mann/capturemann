//! Packed RGB frames to NV12 (BT.709, limited range), with an optional crop and an area-average
//! downscale, for encoders that take system-memory NV12 (the Media Foundation H.264 transform on
//! Windows). Pure Rust, tested on every OS; large frames are converted in horizontal bands on
//! scoped threads.

use cm_core::CmError;

use crate::cpu::{CpuFrame, PixelFormat};

/// A rectangle of the source frame, in px.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SrcRect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// Frames with more pixels than this are converted on several threads.
const PARALLEL_FROM_PIXELS: u64 = 640 * 480;
const MAX_THREADS: usize = 4;

/// BT.709 limited range from 8-bit R, G, B.
#[inline]
fn yuv(r: i32, g: i32, b: i32) -> (u8, u8, u8) {
    let y = ((47 * r + 157 * g + 16 * b + 128) >> 8) + 16;
    let u = ((-26 * r - 86 * g + 112 * b + 128) >> 8) + 128;
    let v = ((112 * r - 102 * g - 10 * b + 128) >> 8) + 128;
    (
        y.clamp(16, 235) as u8,
        u.clamp(16, 240) as u8,
        v.clamp(16, 240) as u8,
    )
}

/// Byte offsets of R, G, B inside a four-byte pixel.
fn channel_order(f: PixelFormat) -> Option<[usize; 3]> {
    match f {
        PixelFormat::Bgrx | PixelFormat::Bgra => Some([2, 1, 0]),
        PixelFormat::Rgbx | PixelFormat::Rgba => Some([0, 1, 2]),
        PixelFormat::Nv12 => None,
    }
}

/// Bytes of an NV12 frame of `w` x `h` (both even).
pub fn nv12_len(w: u32, h: u32) -> usize {
    w as usize * h as usize * 3 / 2
}

/// The source column or row range that output index `i` covers when `src` px map onto `dst` px.
fn span(i: u32, src: u32, dst: u32) -> (usize, usize) {
    let a = (u64::from(i) * u64::from(src) / u64::from(dst)) as usize;
    let b = ((u64::from(i) + 1) * u64::from(src) / u64::from(dst)) as usize;
    (a, b.max(a + 1))
}

/// One output row of RGB triples: the average of the source pixels the row covers.
struct RowSampler<'a> {
    f: &'a CpuFrame,
    order: [usize; 3],
    rect: SrcRect,
    dst_w: u32,
    dst_h: u32,
    /// Source column range of every output column.
    cols: Vec<(usize, usize)>,
}

impl RowSampler<'_> {
    fn row(&self, dy: u32, out: &mut [u8]) {
        let (r0, r1) = span(dy, self.rect.h, self.dst_h);
        let stride = self.f.stride;
        let data = &self.f.data[..];
        let [ro, go, bo] = self.order;
        let (x0, y0) = (self.rect.x as usize, self.rect.y as usize);
        if self.rect.w == self.dst_w && r1 - r0 == 1 {
            // One to one: a swizzle.
            let base = (y0 + r0) * stride + x0 * 4;
            let src = &data[base..base + self.dst_w as usize * 4];
            for (o, p) in out
                .as_chunks_mut::<3>()
                .0
                .iter_mut()
                .zip(src.as_chunks::<4>().0)
            {
                o[0] = p[ro];
                o[1] = p[go];
                o[2] = p[bo];
            }
            return;
        }
        for (o, &(c0, c1)) in out.as_chunks_mut::<3>().0.iter_mut().zip(&self.cols) {
            let (mut r, mut g, mut b) = (0u32, 0u32, 0u32);
            for sy in r0..r1 {
                let base = (y0 + sy) * stride + (x0 + c0) * 4;
                for p in data[base..base + (c1 - c0) * 4].as_chunks::<4>().0 {
                    r += u32::from(p[ro]);
                    g += u32::from(p[go]);
                    b += u32::from(p[bo]);
                }
            }
            let n = ((r1 - r0) * (c1 - c0)) as u32;
            o[0] = ((r + n / 2) / n) as u8;
            o[1] = ((g + n / 2) / n) as u8;
            o[2] = ((b + n / 2) / n) as u8;
        }
    }
}

/// Converts the pair of output rows `2k` and `2k + 1` for every `k` in the band: luma into `y`
/// (rows of `w` bytes) and interleaved chroma into `uv`.
fn convert_band(s: &RowSampler<'_>, first_pair: u32, y: &mut [u8], uv: &mut [u8]) {
    let w = s.dst_w as usize;
    let mut a = vec![0u8; w * 3];
    let mut b = vec![0u8; w * 3];
    for (k, (yy, cc)) in y
        .chunks_exact_mut(w * 2)
        .zip(uv.chunks_exact_mut(w))
        .enumerate()
    {
        let row = (first_pair + k as u32) * 2;
        s.row(row, &mut a);
        s.row(row + 1, &mut b);
        let (y0, y1) = yy.split_at_mut(w);
        for x in 0..w {
            y0[x] = yuv(
                i32::from(a[x * 3]),
                i32::from(a[x * 3 + 1]),
                i32::from(a[x * 3 + 2]),
            )
            .0;
            y1[x] = yuv(
                i32::from(b[x * 3]),
                i32::from(b[x * 3 + 1]),
                i32::from(b[x * 3 + 2]),
            )
            .0;
        }
        for x in (0..w).step_by(2) {
            let avg = |c: usize| {
                (i32::from(a[x * 3 + c])
                    + i32::from(a[x * 3 + 3 + c])
                    + i32::from(b[x * 3 + c])
                    + i32::from(b[x * 3 + 3 + c])
                    + 2)
                    >> 2
            };
            let (_, u, v) = yuv(avg(0), avg(1), avg(2));
            cc[x] = u;
            cc[x + 1] = v;
        }
    }
}

/// Converts `rect` of `f` (the whole frame when `None`) to NV12 of `dst_w` x `dst_h`, scaling by
/// area average when the sizes differ. `dst_w` and `dst_h` must be even and non-zero.
pub fn frame_to_nv12(
    f: &CpuFrame,
    rect: Option<SrcRect>,
    dst_w: u32,
    dst_h: u32,
) -> Result<Vec<u8>, CmError> {
    f.validate()?;
    if dst_w == 0 || dst_h == 0 || !dst_w.is_multiple_of(2) || !dst_h.is_multiple_of(2) {
        return Err(CmError::invalid_input(format!(
            "NV12 output {dst_w}x{dst_h} must be even and non-zero"
        )));
    }
    let rect = rect.unwrap_or(SrcRect {
        x: 0,
        y: 0,
        w: f.width,
        h: f.height,
    });
    if rect.w == 0
        || rect.h == 0
        || u64::from(rect.x) + u64::from(rect.w) > u64::from(f.width)
        || u64::from(rect.y) + u64::from(rect.h) > u64::from(f.height)
    {
        return Err(CmError::invalid_input(format!(
            "crop {}x{} at {},{} is outside the {}x{} frame",
            rect.w, rect.h, rect.x, rect.y, f.width, f.height
        )));
    }
    let Some(order) = channel_order(f.format) else {
        // NV12 in: only a straight copy of a frame of the right size.
        if rect.w == dst_w
            && rect.h == dst_h
            && rect.x == 0
            && rect.y == 0
            && f.stride == f.width as usize
        {
            return Ok(f.packed().into_owned());
        }
        return Err(CmError::invalid_input("cannot scale or crop an NV12 frame"));
    };
    let sampler = RowSampler {
        f,
        order,
        rect,
        dst_w,
        dst_h,
        cols: (0..dst_w).map(|x| span(x, rect.w, dst_w)).collect(),
    };
    let mut out = vec![0u8; nv12_len(dst_w, dst_h)];
    let (yp, uvp) = out.split_at_mut(dst_w as usize * dst_h as usize);
    let pairs = (dst_h / 2) as usize;
    let threads = if u64::from(dst_w) * u64::from(dst_h) >= PARALLEL_FROM_PIXELS {
        std::thread::available_parallelism()
            .map_or(1, |n| n.get())
            .min(MAX_THREADS)
            .min(pairs)
    } else {
        1
    };
    if threads <= 1 {
        convert_band(&sampler, 0, yp, uvp);
        return Ok(out);
    }
    let per = pairs.div_ceil(threads);
    let w = dst_w as usize;
    std::thread::scope(|sc| {
        for (i, (y, uv)) in yp
            .chunks_mut(w * 2 * per)
            .zip(uvp.chunks_mut(w * per))
            .enumerate()
        {
            let sampler = &sampler;
            sc.spawn(move || convert_band(sampler, (i * per) as u32, y, uv));
        }
    });
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32, px: [u8; 4], format: PixelFormat) -> CpuFrame {
        let data: Vec<u8> = (0..w * h).flat_map(|_| px).collect();
        CpuFrame::new(w, h, w as usize * 4, format, data).unwrap()
    }

    fn planes(nv: &[u8], w: u32, h: u32) -> (&[u8], &[u8]) {
        nv.split_at(w as usize * h as usize)
    }

    #[test]
    fn primary_colours_match_bt709_limited_range() {
        // (R, G, B) -> (Y, U, V)
        let cases = [
            ([255, 255, 255], (235, 128, 128)),
            ([0, 0, 0], (16, 128, 128)),
            ([255, 0, 0], (63, 102, 240)),
            ([0, 255, 0], (173, 42, 26)),
            ([0, 0, 255], (32, 240, 118)),
        ];
        for (rgb, (y, u, v)) in cases {
            let f = solid(4, 2, [rgb[2], rgb[1], rgb[0], 255], PixelFormat::Bgra);
            let nv = frame_to_nv12(&f, None, 4, 2).unwrap();
            let (yp, uvp) = planes(&nv, 4, 2);
            let close = |a: u8, b: u8| a.abs_diff(b) <= 1;
            assert!(yp.iter().all(|&p| close(p, y)), "{rgb:?} Y {yp:?} want {y}");
            assert!(
                close(uvp[0], u) && close(uvp[1], v),
                "{rgb:?} UV {uvp:?} want {u},{v}"
            );
        }
    }

    #[test]
    fn rgba_and_bgra_give_the_same_picture() {
        let a = solid(8, 4, [10, 120, 250, 255], PixelFormat::Rgba);
        let b = solid(8, 4, [250, 120, 10, 255], PixelFormat::Bgrx);
        assert_eq!(
            frame_to_nv12(&a, None, 8, 4).unwrap(),
            frame_to_nv12(&b, None, 8, 4).unwrap()
        );
    }

    #[test]
    fn output_length_and_validation() {
        let f = solid(6, 4, [1, 2, 3, 255], PixelFormat::Bgra);
        assert_eq!(frame_to_nv12(&f, None, 6, 4).unwrap().len(), 36);
        assert!(frame_to_nv12(&f, None, 5, 4).is_err()); // odd
        assert!(frame_to_nv12(&f, None, 0, 4).is_err());
        let bad = SrcRect {
            x: 4,
            y: 0,
            w: 4,
            h: 4,
        };
        assert!(frame_to_nv12(&f, Some(bad), 4, 4).is_err());
        let empty = SrcRect {
            x: 0,
            y: 0,
            w: 0,
            h: 4,
        };
        assert!(frame_to_nv12(&f, Some(empty), 4, 4).is_err());
    }

    #[test]
    fn row_padding_is_skipped() {
        // 2x2 with a stride of 12: the padding bytes are 0xFF and must not reach the picture.
        let mut d = vec![0xFFu8; 12 + 8];
        d[..8].copy_from_slice(&[0, 0, 0, 255, 0, 0, 0, 255]);
        d[12..20].copy_from_slice(&[0, 0, 0, 255, 0, 0, 0, 255]);
        let f = CpuFrame::new(2, 2, 12, PixelFormat::Bgra, d).unwrap();
        let nv = frame_to_nv12(&f, None, 2, 2).unwrap();
        assert_eq!(nv, [16, 16, 16, 16, 128, 128]);
    }

    #[test]
    fn crop_takes_the_asked_rectangle() {
        // Left half black, right half white; crop the right half.
        let (w, h) = (8u32, 4u32);
        let mut data = Vec::new();
        for _ in 0..h {
            for x in 0..w {
                let v = if x < 4 { 0 } else { 255 };
                data.extend_from_slice(&[v, v, v, 255]);
            }
        }
        let f = CpuFrame::new(w, h, 32, PixelFormat::Bgra, data).unwrap();
        let r = SrcRect {
            x: 4,
            y: 0,
            w: 4,
            h: 4,
        };
        let nv = frame_to_nv12(&f, Some(r), 4, 4).unwrap();
        assert!(nv[..16].iter().all(|&p| p == 235));
        let left = SrcRect {
            x: 0,
            y: 0,
            w: 4,
            h: 4,
        };
        let nv = frame_to_nv12(&f, Some(left), 4, 4).unwrap();
        assert!(nv[..16].iter().all(|&p| p == 16));
    }

    #[test]
    fn downscale_averages_the_footprint() {
        // 4x4 checker of black and white 2x2 blocks halved to 2x2: each output px is the mean of
        // a 2x2 block, so a block-aligned checker stays pure black or white...
        let mut data = Vec::new();
        for y in 0..4 {
            for x in 0..4 {
                let v = if (x / 2 + y / 2) % 2 == 0 { 0 } else { 255 };
                data.extend_from_slice(&[v, v, v, 255]);
            }
        }
        let f = CpuFrame::new(4, 4, 16, PixelFormat::Bgra, data.clone()).unwrap();
        let nv = frame_to_nv12(&f, None, 2, 2).unwrap();
        assert_eq!(&nv[..4], &[16, 235, 235, 16]);
        // ...and a pixel-level checker averages to mid grey.
        let mut data = Vec::new();
        for y in 0..4 {
            for x in 0..4 {
                let v = if (x + y) % 2 == 0 { 0 } else { 255 };
                data.extend_from_slice(&[v, v, v, 255]);
            }
        }
        let f = CpuFrame::new(4, 4, 16, PixelFormat::Bgra, data).unwrap();
        let nv = frame_to_nv12(&f, None, 2, 2).unwrap();
        let grey = yuv(128, 128, 128).0;
        assert!(nv[..4].iter().all(|&p| p == grey), "{nv:?}");
    }

    #[test]
    fn uneven_scale_factors_cover_every_source_pixel() {
        // 5x5 to 2x2: spans (0,2) and (2,5); the last column and row are not dropped.
        assert_eq!(span(0, 5, 2), (0, 2));
        assert_eq!(span(1, 5, 2), (2, 5));
        assert_eq!(span(1, 1, 2), (0, 1)); // upscale: never an empty span
        let mut data = vec![0u8; 5 * 5 * 4];
        for px in data.as_chunks_mut::<4>().0.iter_mut() {
            px[3] = 255;
        }
        // Only the bottom-right source pixel is white.
        let last = (4 * 5 + 4) * 4;
        data[last..last + 3].copy_from_slice(&[255, 255, 255]);
        let f = CpuFrame::new(5, 5, 20, PixelFormat::Bgra, data).unwrap();
        let nv = frame_to_nv12(&f, None, 2, 2).unwrap();
        assert!(nv[3] > 16, "the corner pixel reached the output: {nv:?}");
    }

    #[test]
    fn threaded_and_single_threaded_conversion_agree() {
        // 1280x720 is above the threshold; compare against a one-band run.
        let (w, h) = (1280u32, 720u32);
        let mut data = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                data.extend_from_slice(&[
                    (x % 251) as u8,
                    (y % 241) as u8,
                    ((x + y) % 239) as u8,
                    255,
                ]);
            }
        }
        let f = CpuFrame::new(w, h, (w * 4) as usize, PixelFormat::Bgra, data).unwrap();
        let fast = frame_to_nv12(&f, None, w, h).unwrap();
        let sampler = RowSampler {
            f: &f,
            order: [2, 1, 0],
            rect: SrcRect { x: 0, y: 0, w, h },
            dst_w: w,
            dst_h: h,
            cols: (0..w).map(|x| span(x, w, w)).collect(),
        };
        let mut slow = vec![0u8; nv12_len(w, h)];
        let (y, uv) = slow.split_at_mut((w * h) as usize);
        convert_band(&sampler, 0, y, uv);
        assert!(fast == slow);
    }

    #[test]
    fn an_nv12_frame_of_the_right_size_is_copied_and_others_are_refused() {
        let f = CpuFrame::new(4, 2, 4, PixelFormat::Nv12, vec![7; 12]).unwrap();
        assert_eq!(frame_to_nv12(&f, None, 4, 2).unwrap(), vec![7; 12]);
        assert!(frame_to_nv12(&f, None, 2, 2).is_err());
    }
}
