//! Irreversible redaction (ANN-2): Gaussian blur and pixelate.
//!
//! Both replace every pixel of the target region in the canvas buffer; nothing of the previous
//! content is kept anywhere (no layers, no side buffers escape these functions).

use tiny_skia::Pixmap;

/// Integer pixel region `[x0, x1) x [y0, y1)`, already clipped to the canvas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Region {
    pub x0: u32,
    pub y0: u32,
    pub x1: u32,
    pub y1: u32,
}

impl Region {
    /// Rounds `[x, y, w, h]` **outward** to whole pixels (so no partially covered source pixel is
    /// left visible) and clips to a `cw x ch` canvas. `None` if the result is empty.
    pub fn outward(rect: [f64; 4], cw: u32, ch: u32) -> Option<Region> {
        let (x0, x1) = ordered(rect[0], rect[0] + rect[2]);
        let (y0, y1) = ordered(rect[1], rect[1] + rect[3]);
        let clip = |v: f64, max: u32| v.clamp(0.0, f64::from(max));
        let (x0, x1) = (clip(x0.floor(), cw), clip(x1.ceil(), cw));
        let (y0, y1) = (clip(y0.floor(), ch), clip(y1.ceil(), ch));
        if x1 <= x0 || y1 <= y0 {
            return None;
        }
        Some(Region {
            x0: x0 as u32,
            y0: y0 as u32,
            x1: x1 as u32,
            y1: y1 as u32,
        })
    }

    fn w(self) -> usize {
        (self.x1 - self.x0) as usize
    }
    fn h(self) -> usize {
        (self.y1 - self.y0) as usize
    }
}

pub(crate) fn ordered(a: f64, b: f64) -> (f64, f64) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

/// Gaussian sigma for a `strength` (documented decision: `sigma = strength / 2`).
pub(crate) fn sigma_for(strength: u32) -> f32 {
    strength as f32 / 2.0
}

/// Radii of three successive box blurs whose combined variance approximates a Gaussian of
/// `sigma` ("boxes for Gauss", n = 3): box widths `wl` or `wl + 2` (odd), mixed so the sum of
/// their variances matches `sigma^2`.
fn box_radii(sigma: f32) -> [usize; 3] {
    let s2 = f64::from(sigma) * f64::from(sigma);
    let ideal = (12.0 * s2 / 3.0 + 1.0).sqrt();
    let mut wl = ideal.floor() as i64;
    if wl % 2 == 0 {
        wl -= 1;
    }
    let wl = wl.max(1);
    let m = ((12.0 * s2 - 3.0 * (wl * wl) as f64 - 12.0 * wl as f64 - 9.0)
        / (-4.0 * wl as f64 - 4.0))
        .round()
        .clamp(0.0, 3.0) as usize;
    std::array::from_fn(|i| {
        let width = if i < m { wl } else { wl + 2 };
        ((width - 1) / 2) as usize
    })
}

/// Worker threads for a region of `pixels` pixels: small regions stay on the caller's thread.
fn workers(pixels: usize) -> usize {
    if pixels < 200_000 {
        return 1;
    }
    std::thread::available_parallelism().map_or(1, |n| n.get().min(8))
}

#[inline]
fn avg(sum: u32, div: u32) -> u8 {
    ((sum + div / 2) / div) as u8
}

/// One horizontal box pass with edge clamping: `dst[x] = mean(src[x-r ..= x+r])`.
fn box_pass_h(src: &[u8], dst: &mut [u8], w: usize, h: usize, r: usize) {
    let div = (2 * r + 1) as u32;
    let row_job = |s: &[u8], d: &mut [u8]| {
        let px = |x: isize| -> usize { x.clamp(0, w as isize - 1) as usize * 4 };
        let mut sum = [0u32; 4];
        for i in -(r as isize)..=(r as isize) {
            let o = px(i);
            for c in 0..4 {
                sum[c] += u32::from(s[o + c]);
            }
        }
        for x in 0..w {
            for c in 0..4 {
                d[x * 4 + c] = avg(sum[c], div);
            }
            let add = px(x as isize + r as isize + 1);
            let sub = px(x as isize - r as isize);
            for c in 0..4 {
                sum[c] = sum[c] + u32::from(s[add + c]) - u32::from(s[sub + c]);
            }
        }
    };
    let row_len = w * 4;
    let threads = workers(w * h);
    let rows_per = h.div_ceil(threads).max(1);
    std::thread::scope(|sc| {
        for (si, di) in src
            .chunks(rows_per * row_len)
            .zip(dst.chunks_mut(rows_per * row_len))
        {
            if threads == 1 {
                for (s, d) in si.chunks(row_len).zip(di.chunks_mut(row_len)) {
                    row_job(s, d);
                }
            } else {
                sc.spawn(move || {
                    for (s, d) in si.chunks(row_len).zip(di.chunks_mut(row_len)) {
                        row_job(s, d);
                    }
                });
            }
        }
    });
}

/// One vertical box pass with edge clamping, swept row by row over all columns at once (cache
/// friendly). Threads take contiguous bands of output rows and start their window there.
fn box_pass_v(src: &[u8], dst: &mut [u8], w: usize, h: usize, r: usize) {
    let div = (2 * r + 1) as u32;
    let row_len = w * 4;
    let band = |first: usize, d: &mut [u8]| {
        let row = |y: isize| -> &[u8] {
            let y = y.clamp(0, h as isize - 1) as usize;
            &src[y * row_len..(y + 1) * row_len]
        };
        let mut sum = vec![0u32; row_len];
        for i in -(r as isize)..=(r as isize) {
            for (s, v) in sum.iter_mut().zip(row(first as isize + i)) {
                *s += u32::from(*v);
            }
        }
        for (k, out) in d.chunks_mut(row_len).enumerate() {
            let y = (first + k) as isize;
            for (o, s) in out.iter_mut().zip(&sum) {
                *o = avg(*s, div);
            }
            let (add, sub) = (row(y + r as isize + 1), row(y - r as isize));
            for ((s, a), b) in sum.iter_mut().zip(add).zip(sub) {
                *s = *s + u32::from(*a) - u32::from(*b);
            }
        }
    };
    let threads = workers(w * h);
    let rows_per = h.div_ceil(threads).max(1);
    std::thread::scope(|sc| {
        for (i, d) in dst.chunks_mut(rows_per * row_len).enumerate() {
            if threads == 1 {
                band(i * rows_per, d);
            } else {
                let band = &band;
                sc.spawn(move || band(i * rows_per, d));
            }
        }
    });
}

/// Gaussian-equivalent blur of the region: three box blurs per axis (O(1) per pixel whatever
/// the strength) over `u8`, rows and row bands in parallel. Samples are taken only from inside
/// the region with edge clamping, so pixels outside the region neither change nor bleed in.
/// Every pixel of the region is overwritten from the blurred copy (nothing of the source is
/// kept), and the extra memory is two region-sized `u8` buffers.
pub(crate) fn blur(pm: &mut Pixmap, r: Region, strength: u32) {
    let (w, h) = (r.w(), r.h());
    let stride = pm.width() as usize * 4;
    let data = pm.data_mut();
    let mut a = vec![0u8; w * h * 4];
    let mut b = vec![0u8; w * h * 4];
    for y in 0..h {
        let row = (r.y0 as usize + y) * stride + r.x0 as usize * 4;
        a[y * w * 4..(y + 1) * w * 4].copy_from_slice(&data[row..row + w * 4]);
    }
    for rad in box_radii(sigma_for(strength)) {
        if rad == 0 {
            continue;
        }
        box_pass_h(&a, &mut b, w, h, rad);
        box_pass_v(&b, &mut a, w, h, rad);
    }
    for y in 0..h {
        let row = (r.y0 as usize + y) * stride + r.x0 as usize * 4;
        data[row..row + w * 4].copy_from_slice(&a[y * w * 4..(y + 1) * w * 4]);
    }
    fix_premultiplied(data, stride, r);
}

/// Pixelate: blocks aligned to the region's top-left, each filled with its mean color.
pub(crate) fn pixelate(pm: &mut Pixmap, r: Region, block: u32) {
    let stride = pm.width() as usize * 4;
    let data = pm.data_mut();
    let b = block.max(1);
    let mut by = r.y0;
    while by < r.y1 {
        let ey = (by + b).min(r.y1);
        let mut bx = r.x0;
        while bx < r.x1 {
            let ex = (bx + b).min(r.x1);
            let mut sum = [0u32; 4];
            for y in by..ey {
                for x in bx..ex {
                    let o = y as usize * stride + x as usize * 4;
                    for c in 0..4 {
                        sum[c] += u32::from(data[o + c]);
                    }
                }
            }
            let n = (ex - bx) * (ey - by);
            let avg: [u8; 4] = std::array::from_fn(|c| ((sum[c] + n / 2) / n) as u8);
            for y in by..ey {
                for x in bx..ex {
                    let o = y as usize * stride + x as usize * 4;
                    data[o..o + 4].copy_from_slice(&avg);
                }
            }
            bx = ex;
        }
        by = ey;
    }
    fix_premultiplied(data, stride, r);
}

/// Rounding can push a color channel one above alpha; keep the buffer a valid premultiplied one.
fn fix_premultiplied(data: &mut [u8], stride: usize, r: Region) {
    for y in r.y0 as usize..r.y1 as usize {
        for x in r.x0 as usize..r.x1 as usize {
            let o = y * stride + x * 4;
            let a = data[o + 3];
            for c in 0..3 {
                data[o + c] = data[o + c].min(a);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kernel(sigma: f32) -> Vec<f32> {
        let radius = (sigma * 3.0).ceil().max(1.0) as i32;
        let mut k: Vec<f32> = (-radius..=radius)
            .map(|i| (-((i * i) as f32) / (2.0 * sigma * sigma)).exp())
            .collect();
        let sum: f32 = k.iter().sum();
        for v in &mut k {
            *v /= sum;
        }
        k
    }

    /// The previous implementation (true separable Gaussian, f32), kept as the reference.
    fn gaussian_reference(src: &[u8], w: usize, h: usize, strength: u32) -> Vec<u8> {
        let k = kernel(sigma_for(strength));
        let rad = (k.len() / 2) as isize;
        let mut tmp = vec![0f32; src.len()];
        for y in 0..h {
            for x in 0..w {
                for c in 0..4 {
                    let mut acc = 0f32;
                    for (ki, kv) in k.iter().enumerate() {
                        let sx = (x as isize + ki as isize - rad).clamp(0, w as isize - 1) as usize;
                        acc += kv * f32::from(src[(y * w + sx) * 4 + c]);
                    }
                    tmp[(y * w + x) * 4 + c] = acc;
                }
            }
        }
        let mut out = vec![0u8; src.len()];
        for y in 0..h {
            for x in 0..w {
                for c in 0..4 {
                    let mut acc = 0f32;
                    for (ki, kv) in k.iter().enumerate() {
                        let sy = (y as isize + ki as isize - rad).clamp(0, h as isize - 1) as usize;
                        acc += kv * tmp[(sy * w + x) * 4 + c];
                    }
                    out[(y * w + x) * 4 + c] = acc.round().clamp(0.0, 255.0) as u8;
                }
            }
        }
        out
    }

    fn pattern(w: u32, h: u32) -> Pixmap {
        let mut pm = Pixmap::new(w, h).unwrap();
        for (i, px) in pm.data_mut().as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let (x, y) = (i as u32 % w, i as u32 / w);
            // Hard edges plus texture.
            let v = if (x / 7 + y / 5) % 2 == 0 { 230 } else { 20 };
            let n = ((x * 31 + y * 17) % 23) as u8;
            px.copy_from_slice(&[v, v / 2 + n, 255 - v, 255]);
        }
        pm
    }

    fn diff(a: &[u8], b: &[u8]) -> (f64, u8) {
        let sum: u64 = a
            .iter()
            .zip(b)
            .map(|(x, y)| u64::from(x.abs_diff(*y)))
            .sum();
        let max = a
            .iter()
            .zip(b)
            .map(|(x, y)| x.abs_diff(*y))
            .max()
            .unwrap_or(0);
        (sum as f64 / a.len() as f64, max)
    }

    /// R1-8: the box approximation stays close to the Gaussian it replaces, for small and
    /// large strengths, and for a region big enough to take the multi-threaded path.
    #[test]
    fn r1_8_box_blur_matches_the_gaussian_reference() {
        for (w, h, strength) in [(120u32, 90u32, 4u32), (120, 90, 32), (600, 400, 12)] {
            let mut pm = pattern(w, h);
            let want = gaussian_reference(pm.data(), w as usize, h as usize, strength);
            let r = Region {
                x0: 0,
                y0: 0,
                x1: w,
                y1: h,
            };
            blur(&mut pm, r, strength);
            let (mean, _) = diff(pm.data(), &want);
            assert!(mean < 3.0, "{w}x{h} strength {strength}: mean diff {mean}");
        }
    }

    /// Pixels outside the region are untouched, and a region smaller than the radius is fine.
    #[test]
    fn r1_8_blur_stays_inside_the_region_and_handles_tiny_regions() {
        let mut pm = pattern(50, 40);
        let before = pm.data().to_vec();
        let r = Region {
            x0: 10,
            y0: 10,
            x1: 13,
            y1: 12,
        };
        blur(&mut pm, r, 64);
        for y in 0..40usize {
            for x in 0..50usize {
                let inside = (10..13).contains(&x) && (10..12).contains(&y);
                let o = (y * 50 + x) * 4;
                if !inside {
                    assert_eq!(pm.data()[o..o + 4], before[o..o + 4]);
                }
            }
        }
    }

    #[test]
    fn r1_8_box_radii_variance_matches_sigma() {
        for strength in [4u32, 8, 32, 64, 100] {
            let sigma = f64::from(sigma_for(strength));
            let var: f64 = box_radii(sigma_for(strength))
                .iter()
                .map(|&r| {
                    let n = (2 * r + 1) as f64;
                    (n * n - 1.0) / 12.0
                })
                .sum();
            assert!(
                (var.sqrt() - sigma).abs() / sigma < 0.1,
                "strength {strength}: {} vs {sigma}",
                var.sqrt()
            );
        }
    }

    /// R1-8 budget: a full-frame 5K blur at strength 32 took 3.0 s and 751 MB before. Run with
    /// `cargo test -p cm-render --release -- --ignored r1_8`.
    #[test]
    #[ignore = "timing; release build only"]
    fn r1_8_five_k_full_frame_blur_is_under_the_budget() {
        let mut pm = pattern(5120, 2880);
        let r = Region {
            x0: 0,
            y0: 0,
            x1: 5120,
            y1: 2880,
        };
        let t = std::time::Instant::now();
        blur(&mut pm, r, 32);
        let ms = t.elapsed().as_millis();
        println!("5K blur: {ms} ms");
        assert!(ms < 400, "{ms} ms");
    }
}
