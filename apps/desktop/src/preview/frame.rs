//! Synthetic 1080p NV12 frames for the preview test (no capture permissions, no decoder).
//!
//! Each frame carries a 32-bit frame counter as a row of black/white cells at the top left
//! ([`read_counter`] decodes it) and a white bar that moves 8 px per frame, so a dropped or
//! repeated frame is visible by eye and checkable in tests. The same source feeds the editor
//! tests that need a frame-counter fixture.

/// Full-range NV12 (`420f`): a luma plane and one interleaved CbCr plane at half height.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Nv12Frame {
    pub width: usize,
    pub height: usize,
    /// `width * height` bytes.
    pub y: Vec<u8>,
    /// `width * height / 2` bytes: Cb, Cr pairs, one pair per 2x2 luma block.
    pub uv: Vec<u8>,
}

const CELL: usize = 24;
const CELL_GAP: usize = 4;
const MARGIN: usize = 16;
const BAR_W: usize = 64;
const BAR_STEP: usize = 8;

/// Generates frames by index. The background is built once; each frame copies it and draws the
/// counter and the bar, which costs about what a decoder hand-off costs (a memcpy of 3 MB).
pub struct SyntheticSource {
    base: Nv12Frame,
}

impl SyntheticSource {
    pub fn new(width: usize, height: usize) -> SyntheticSource {
        assert!(
            width >= 2 * MARGIN + 32 * (CELL + CELL_GAP)
                && width.is_multiple_of(2)
                && height.is_multiple_of(2)
        );
        let mut y = vec![0u8; width * height];
        for (row, line) in y.chunks_exact_mut(width).enumerate() {
            for (x, px) in line.iter_mut().enumerate() {
                // Horizontal ramp with a darker stripe every 120 px and a lighter band every 90 rows.
                let ramp = 40 + (x * 150 / width) as u8;
                let stripe = if (x / 120) % 2 == 0 { 0 } else { 18 };
                let band = if (row / 90) % 2 == 0 { 0 } else { 10 };
                *px = ramp.saturating_sub(stripe).saturating_add(band);
            }
        }
        let mut uv = vec![128u8; width * height / 2];
        for line in uv.chunks_exact_mut(width) {
            for (i, pair) in line.as_chunks_mut::<2>().0.iter_mut().enumerate() {
                pair[0] = 96 + (i * 64 / (width / 2)) as u8; // Cb
                pair[1] = 160 - (i * 64 / (width / 2)) as u8; // Cr
            }
        }
        SyntheticSource {
            base: Nv12Frame {
                width,
                height,
                y,
                uv,
            },
        }
    }

    pub fn width(&self) -> usize {
        self.base.width
    }

    pub fn height(&self) -> usize {
        self.base.height
    }

    /// Frame `index`: counter cells (MSB first) and the moving bar.
    pub fn frame(&self, index: u32) -> Nv12Frame {
        let mut f = self.base.clone();
        let w = f.width;
        for bit in 0..32 {
            let on = (index >> (31 - bit)) & 1 == 1;
            let x0 = MARGIN + bit * (CELL + CELL_GAP);
            fill_luma(&mut f, x0, MARGIN, CELL, CELL, if on { 235 } else { 16 });
            fill_chroma_neutral(&mut f, x0, MARGIN, CELL, CELL);
        }
        let bar_x = (index as usize * BAR_STEP) % (w - BAR_W);
        let top = MARGIN + CELL + MARGIN;
        let h = f.height - top;
        fill_luma(&mut f, bar_x, top, BAR_W, h, 235);
        f
    }
}

fn fill_luma(f: &mut Nv12Frame, x0: usize, y0: usize, w: usize, h: usize, value: u8) {
    for row in y0..y0 + h {
        let start = row * f.width + x0;
        f.y[start..start + w].fill(value);
    }
}

fn fill_chroma_neutral(f: &mut Nv12Frame, x0: usize, y0: usize, w: usize, h: usize) {
    for row in y0 / 2..(y0 + h) / 2 {
        let start = row * f.width + x0;
        f.uv[start..start + w].fill(128);
    }
}

/// Decodes the frame counter drawn by [`SyntheticSource::frame`].
pub fn read_counter(f: &Nv12Frame) -> u32 {
    let mut v = 0u32;
    for bit in 0..32 {
        let x = MARGIN + bit * (CELL + CELL_GAP) + CELL / 2;
        let y = MARGIN + CELL / 2;
        v = (v << 1) | u32::from(f.y[y * f.width + x] > 128);
    }
    v
}

/// BT.601 full-range NV12 to packed BGRA (the byte order GPUI's atlas wants), for the CPU
/// upload path. Alpha is 255.
pub fn nv12_to_bgra(f: &Nv12Frame) -> Vec<u8> {
    let (w, h) = (f.width, f.height);
    let mut out = vec![255u8; w * h * 4];
    for row in 0..h {
        let uv_row = &f.uv[(row / 2) * w..(row / 2) * w + w];
        let y_row = &f.y[row * w..row * w + w];
        let dst = &mut out[row * w * 4..(row + 1) * w * 4];
        for x in 0..w {
            let yv = i32::from(y_row[x]);
            let cb = i32::from(uv_row[x & !1]) - 128;
            let cr = i32::from(uv_row[x | 1]) - 128;
            let r = yv + ((359 * cr) >> 8);
            let g = yv - ((88 * cb + 183 * cr) >> 8);
            let b = yv + ((454 * cb) >> 8);
            let px = &mut dst[x * 4..x * 4 + 4];
            px[0] = b.clamp(0, 255) as u8;
            px[1] = g.clamp(0, 255) as u8;
            px[2] = r.clamp(0, 255) as u8;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The counter round-trips for the first frames, a large index and the wrap value.
    #[test]
    fn counter_round_trips() {
        let s = SyntheticSource::new(1920, 1080);
        for i in [0u32, 1, 2, 59, 60, 1_000_003, u32::MAX] {
            assert_eq!(read_counter(&s.frame(i)), i);
        }
    }

    /// Consecutive frames differ (the bar moves), so a repeated frame is detectable.
    #[test]
    fn consecutive_frames_differ() {
        let s = SyntheticSource::new(1920, 1080);
        assert_ne!(s.frame(10), s.frame(11));
        assert_eq!(s.frame(10), s.frame(10));
    }

    #[test]
    fn plane_sizes_are_nv12() {
        let f = SyntheticSource::new(1920, 1080).frame(0);
        assert_eq!(f.y.len(), 1920 * 1080);
        assert_eq!(f.uv.len(), 1920 * 1080 / 2);
        assert_eq!(nv12_to_bgra(&f).len(), 1920 * 1080 * 4);
    }

    /// Neutral chroma gives grey: B, G and R agree with the luma.
    #[test]
    fn neutral_chroma_converts_to_grey() {
        let mut f = Nv12Frame {
            width: 2,
            height: 2,
            y: vec![200; 4],
            uv: vec![128; 2],
        };
        f.y[0] = 16;
        let px = nv12_to_bgra(&f);
        assert_eq!(&px[0..4], &[16, 16, 16, 255]);
        assert_eq!(&px[4..8], &[200, 200, 200, 255]);
    }
}
