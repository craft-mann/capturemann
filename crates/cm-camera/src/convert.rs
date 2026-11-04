//! Platform-neutral helpers of the Windows camera backend (Media Foundation): pixel-format
//! conversion for the preview, choosing a capture mode, camera classification, the registry
//! switches of the camera privacy setting and the time conversions. Nothing here touches the OS, so
//! it is tested on every system; the Media Foundation calls live in `sys/`.

use std::sync::Arc;

use crate::permission::CameraPermission;
use crate::preview::{scaled_size, RgbaImage};
use crate::types::{CameraKind, CameraMode};

/// The pixel layouts the Windows source delivers (the source reader converts anything else, such as
/// Motion JPEG, to NV12 before the frame reaches us).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    /// Y plane, then interleaved U/V at half resolution.
    Nv12,
    /// Packed `Y0 U Y1 V`.
    Yuy2,
    /// Packed `B G R X` (Media Foundation's `RGB32`).
    Bgra,
}

/// One camera frame as bytes, top row first. Cloning shares the pixels.
#[derive(Debug, Clone)]
pub struct RawFrame {
    pub format: PixelFormat,
    pub width: u32,
    pub height: u32,
    /// Bytes from the start of one row to the next (at least the row's size).
    pub stride: usize,
    pub data: Arc<Vec<u8>>,
}

impl RawFrame {
    /// Bytes the layout needs for `height` rows at `stride`.
    pub fn needed_len(format: PixelFormat, stride: usize, height: u32) -> usize {
        let h = height as usize;
        match format {
            PixelFormat::Nv12 => stride * h + stride * h.div_ceil(2),
            PixelFormat::Yuy2 | PixelFormat::Bgra => stride * h,
        }
    }

    /// An RGBA picture at most `max_edge` on the long side (nearest neighbour), mirrored when
    /// asked. `None` for an empty frame or one whose buffer is too short for its stride.
    pub fn to_rgba(&self, max_edge: u32, mirror: bool) -> Option<RgbaImage> {
        let (sw, sh) = (self.width, self.height);
        if sw == 0 || sh == 0 || self.data.len() < Self::needed_len(self.format, self.stride, sh) {
            return None;
        }
        let row_bytes = match self.format {
            PixelFormat::Nv12 => sw as usize,
            PixelFormat::Yuy2 => (sw as usize).div_ceil(2) * 4,
            PixelFormat::Bgra => sw as usize * 4,
        };
        if self.stride < row_bytes {
            return None;
        }
        let hd = sh >= 720;
        let (tw, th) = scaled_size(sw, sh, max_edge);
        let mut out = Vec::with_capacity(tw as usize * th as usize * 4);
        for y in 0..th {
            let sy = (u64::from(y) * u64::from(sh) / u64::from(th)) as usize;
            for x in 0..tw {
                let dx = if mirror { tw - 1 - x } else { x };
                let sx = (u64::from(dx) * u64::from(sw) / u64::from(tw)) as usize;
                let [r, g, b] = self.pixel(sx, sy, hd);
                out.extend_from_slice(&[r, g, b, 255]);
            }
        }
        Some(RgbaImage {
            width: tw,
            height: th,
            data: out,
        })
    }

    fn pixel(&self, x: usize, y: usize, hd: bool) -> [u8; 3] {
        let d = &self.data;
        match self.format {
            PixelFormat::Bgra => {
                let i = y * self.stride + x * 4;
                [d[i + 2], d[i + 1], d[i]]
            }
            PixelFormat::Yuy2 => {
                let i = y * self.stride + (x / 2) * 4;
                let luma = if x.is_multiple_of(2) { d[i] } else { d[i + 2] };
                yuv_to_rgb(luma, d[i + 1], d[i + 3], hd)
            }
            PixelFormat::Nv12 => {
                let luma = d[y * self.stride + x];
                let uv = self.stride * self.height as usize + (y / 2) * self.stride + (x / 2) * 2;
                yuv_to_rgb(luma, d[uv], d[uv + 1], hd)
            }
        }
    }
}

/// Limited-range YUV to RGB: BT.709 for HD pictures, BT.601 below (the usual convention for
/// cameras that do not say).
pub fn yuv_to_rgb(y: u8, u: u8, v: u8, bt709: bool) -> [u8; 3] {
    let c = i32::from(y) - 16;
    let d = i32::from(u) - 128;
    let e = i32::from(v) - 128;
    let (r, g, b) = if bt709 {
        (
            298 * c + 459 * e,
            298 * c - 55 * d - 136 * e,
            298 * c + 541 * d,
        )
    } else {
        (
            298 * c + 409 * e,
            298 * c - 100 * d - 208 * e,
            298 * c + 516 * d,
        )
    };
    let clamp = |v: i32| ((v + 128) >> 8).clamp(0, 255) as u8;
    [clamp(r), clamp(g), clamp(b)]
}

// ---- time ---------------------------------------------------------------------------------

/// How far a camera's own capture time may be from the host clock at delivery before it is not
/// believed (a stuck or wrapped device clock).
pub const MAX_STAMP_SKEW_NS: u64 = 5_000_000_000;

/// The capture time of a frame on the host clock (QPC nanoseconds). Media Foundation stamps live
/// capture samples in 100 ns units on the system clock; a missing, negative or implausible stamp
/// (further than [`MAX_STAMP_SKEW_NS`] from `delivered_ns`, or in the future) falls back to the
/// time of delivery.
pub fn capture_host_ns(sample_time_100ns: i64, delivered_ns: u64) -> u64 {
    let Ok(t) = u64::try_from(sample_time_100ns) else {
        return delivered_ns;
    };
    let ns = t.saturating_mul(100);
    if ns == 0 || ns > delivered_ns || delivered_ns - ns > MAX_STAMP_SKEW_NS {
        delivered_ns
    } else {
        ns
    }
}

/// Whether a preview frame is due: frames are dropped to the preview rate, keeping a little slack
/// so a 30 fps camera gives a steady 15 or 30 fps preview and not a jittery one.
pub fn preview_due(last_ns: Option<u64>, now_ns: u64, fps: u32) -> bool {
    let Some(last) = last_ns else { return true };
    let interval = 1_000_000_000 / u64::from(fps.max(1));
    now_ns.saturating_sub(last) + interval / 10 >= interval
}

// ---- capture modes -------------------------------------------------------------------------

/// How a camera offers its pictures, in order of how little work the pipeline needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Subtype {
    Nv12,
    Yuy2,
    /// Motion JPEG: the reader decodes it.
    Mjpg,
    Rgb32,
    Other,
}

impl Subtype {
    fn rank(self) -> u8 {
        match self {
            Subtype::Nv12 => 0,
            Subtype::Yuy2 => 1,
            Subtype::Mjpg => 2,
            Subtype::Rgb32 => 3,
            Subtype::Other => 4,
        }
    }
}

/// One native media type of a camera.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RawMode {
    pub width: u32,
    pub height: u32,
    pub min_fps: f64,
    pub max_fps: f64,
    pub subtype: Subtype,
}

impl RawMode {
    fn supports_fps(&self, fps: u32) -> bool {
        self.min_fps - 0.5 <= f64::from(fps) && f64::from(fps) <= self.max_fps + 0.5
    }
}

/// The modes a person sees: one per size with the frame-rate range merged, largest first.
pub fn fold_modes(raw: &[RawMode]) -> Vec<CameraMode> {
    let mut out: Vec<CameraMode> = Vec::new();
    for m in raw.iter().filter(|m| m.width > 0 && m.height > 0) {
        match out
            .iter_mut()
            .find(|o| o.width == m.width && o.height == m.height)
        {
            Some(o) => {
                o.min_fps = o.min_fps.min(m.min_fps);
                o.max_fps = o.max_fps.max(m.max_fps);
            }
            None => out.push(CameraMode {
                width: m.width,
                height: m.height,
                min_fps: m.min_fps,
                max_fps: m.max_fps,
            }),
        }
    }
    out.sort_by_key(|m| std::cmp::Reverse(u64::from(m.width) * u64::from(m.height)));
    out
}

/// Chooses the native type to capture at: one that offers `fps`, the smallest picture that is at
/// least `want_height` tall (else the largest below it), then the layout that needs the least
/// conversion. If no type offers `fps`, the one that comes closest. `None` for no usable type.
pub fn pick_mode(raw: &[RawMode], want_height: u32, want_fps: u32) -> Option<usize> {
    let usable = || {
        raw.iter()
            .enumerate()
            .filter(|(_, m)| m.width > 0 && m.height > 0)
    };
    let with_fps: Vec<(usize, &RawMode)> =
        usable().filter(|(_, m)| m.supports_fps(want_fps)).collect();
    if with_fps.is_empty() {
        // Nearest top frame rate, then the same size rules.
        return usable()
            .min_by(|(_, a), (_, b)| {
                let da = (a.max_fps - f64::from(want_fps)).abs();
                let db = (b.max_fps - f64::from(want_fps)).abs();
                da.total_cmp(&db)
                    .then(size_key(a, want_height).cmp(&size_key(b, want_height)))
                    .then(a.subtype.rank().cmp(&b.subtype.rank()))
            })
            .map(|(i, _)| i);
    }
    with_fps
        .into_iter()
        .min_by_key(|(_, m)| (size_key(m, want_height), m.subtype.rank()))
        .map(|(i, _)| i)
}

/// Smaller sorts first: sizes at or above the wanted height by how much they exceed it, then
/// sizes below it by how much they fall short; wider first as the last tie-break.
fn size_key(m: &RawMode, want_height: u32) -> (u8, u32, std::cmp::Reverse<u32>) {
    if m.height >= want_height {
        (0, m.height - want_height, std::cmp::Reverse(m.width))
    } else {
        (1, want_height - m.height, std::cmp::Reverse(m.width))
    }
}

// ---- classification --------------------------------------------------------------------------

/// What kind of camera a device is. Windows has no flag for it, so the device path decides what it
/// can: a camera that is not a hardware device at all (a software or root-enumerated device, as
/// virtual cameras register) is `Virtual`; a name that says the camera is part of the computer
/// makes it `BuiltIn`; anything else on the bus is `External`.
pub fn classify_camera(name: &str, symbolic_link: &str) -> CameraKind {
    let link = symbolic_link.to_ascii_lowercase();
    let path = link.trim_start_matches(r"\\?\");
    let software = ["root#", "swd#", "sw#"].iter().any(|p| path.starts_with(p));
    let virtual_name = {
        let n = name.to_ascii_lowercase();
        n.contains("virtual")
    };
    if software || (virtual_name && !path.starts_with("usb#")) {
        return CameraKind::Virtual;
    }
    let n = name.to_ascii_lowercase();
    if n.contains("integrated") || n.contains("built-in") || n.contains("builtin") {
        CameraKind::BuiltIn
    } else {
        CameraKind::External
    }
}

// ---- privacy switches --------------------------------------------------------------------------

/// What the camera privacy switches in Settings > Privacy & security > Camera add up to: a group
/// policy that denies apps (`2`) is `Restricted`; `Deny` in the device, user or desktop-apps
/// switch is `Denied`; a policy that forces access on (`1`) or no switch is `Authorized`. Windows
/// never asks a desktop app, so there is no "not determined".
pub fn windows_camera_state(
    policy: Option<u32>,
    machine: Option<&str>,
    user: Option<&str>,
    desktop_apps: Option<&str>,
) -> CameraPermission {
    match policy {
        Some(2) => return CameraPermission::Restricted,
        Some(1) => return CameraPermission::Authorized,
        _ => {}
    }
    let denied = |v: Option<&str>| v.is_some_and(|v| v.trim().eq_ignore_ascii_case("deny"));
    if denied(machine) || denied(user) || denied(desktop_apps) {
        CameraPermission::Denied
    } else {
        CameraPermission::Authorized
    }
}

// ---- NV12 for the encoder ------------------------------------------------------------------

/// Where output sample `x` of `dst` starts and ends on an axis of `src` samples: the box an area
/// average covers (at least one sample).
fn box_span(x: u32, src: u32, dst: u32) -> (usize, usize) {
    let start = (u64::from(x) * u64::from(src) / u64::from(dst)) as u32;
    let end = ((u64::from(x) + 1) * u64::from(src)).div_ceil(u64::from(dst)) as u32;
    (
        start.min(src - 1) as usize,
        end.max(start + 1).min(src) as usize,
    )
}

impl RawFrame {
    fn luma_at(&self, x: usize, y: usize) -> u32 {
        let row = y * self.stride;
        match self.format {
            PixelFormat::Nv12 => u32::from(self.data[row + x]),
            // Y0 U Y1 V: luma sits at the even bytes.
            _ => u32::from(self.data[row + x * 2]),
        }
    }

    /// The U and V of chroma sample (`cx`, `cy`) of the 4:2:0 grid (`ceil(w/2)` x `ceil(h/2)`).
    fn chroma_at(&self, cx: usize, cy: usize) -> (u32, u32) {
        match self.format {
            PixelFormat::Nv12 => {
                let at = self.stride * self.height as usize + cy * self.stride + cx * 2;
                (u32::from(self.data[at]), u32::from(self.data[at + 1]))
            }
            _ => {
                // YUY2 has chroma on every row: average the two rows the 4:2:0 sample covers.
                let h = self.height as usize;
                let (r0, r1) = (cy * 2, (cy * 2 + 1).min(h - 1));
                let a = r0 * self.stride + cx * 4;
                let b = r1 * self.stride + cx * 4;
                (
                    (u32::from(self.data[a + 1]) + u32::from(self.data[b + 1])).div_ceil(2),
                    (u32::from(self.data[a + 3]) + u32::from(self.data[b + 3])).div_ceil(2),
                )
            }
        }
    }

    /// The frame as tightly packed NV12 of exactly `dst_w` x `dst_h` (both even and non-zero),
    /// scaled by area average when the size differs. Only NV12 and YUY2 sources (what the source
    /// reader delivers for the camera's own formats; Motion JPEG is decoded by the reader before a
    /// frame reaches us): `None` for packed RGB (the encoder converts and scales those itself), a
    /// bad size, or a buffer too short for its stride.
    pub fn to_nv12(&self, dst_w: u32, dst_h: u32) -> Option<Vec<u8>> {
        if !matches!(self.format, PixelFormat::Nv12 | PixelFormat::Yuy2)
            || self.width == 0
            || self.height == 0
            || dst_w == 0
            || dst_h == 0
            || !dst_w.is_multiple_of(2)
            || !dst_h.is_multiple_of(2)
        {
            return None;
        }
        let row_bytes = match self.format {
            PixelFormat::Nv12 => self.width as usize,
            _ => self.width.div_ceil(2) as usize * 4,
        };
        if self.stride < row_bytes
            || self.data.len() < Self::needed_len(self.format, self.stride, self.height)
        {
            return None;
        }
        let (sw, sh) = (self.width, self.height);
        let (cw, ch) = (sw.div_ceil(2), sh.div_ceil(2));
        let (dw, dh) = (dst_w as usize, dst_h as usize);
        let mut out = vec![0u8; dw * dh + dw * dh / 2];
        let (yp, uvp) = out.split_at_mut(dw * dh);
        let x_spans: Vec<(usize, usize)> = (0..dst_w).map(|x| box_span(x, sw, dst_w)).collect();
        for dy in 0..dh {
            let (y0, y1) = box_span(dy as u32, sh, dst_h);
            for (dx, &(x0, x1)) in x_spans.iter().enumerate() {
                let (mut sum, mut n) = (0u32, 0u32);
                for y in y0..y1 {
                    for x in x0..x1 {
                        sum += self.luma_at(x, y);
                        n += 1;
                    }
                }
                yp[dy * dw + dx] = ((sum + n / 2) / n) as u8;
            }
        }
        let cx_spans: Vec<(usize, usize)> =
            (0..dst_w / 2).map(|x| box_span(x, cw, dst_w / 2)).collect();
        for dy in 0..dh / 2 {
            let (y0, y1) = box_span(dy as u32, ch, dst_h / 2);
            for (dx, &(x0, x1)) in cx_spans.iter().enumerate() {
                let (mut su, mut sv, mut n) = (0u32, 0u32, 0u32);
                for y in y0..y1 {
                    for x in x0..x1 {
                        let (u, v) = self.chroma_at(x, y);
                        su += u;
                        sv += v;
                        n += 1;
                    }
                }
                uvp[dy * dw + dx * 2] = ((su + n / 2) / n) as u8;
                uvp[dy * dw + dx * 2 + 1] = ((sv + n / 2) / n) as u8;
            }
        }
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(w: u32, h: u32, lo: f64, hi: f64, s: Subtype) -> RawMode {
        RawMode {
            width: w,
            height: h,
            min_fps: lo,
            max_fps: hi,
            subtype: s,
        }
    }

    fn frame(format: PixelFormat, w: u32, h: u32, stride: usize, data: Vec<u8>) -> RawFrame {
        RawFrame {
            format,
            width: w,
            height: h,
            stride,
            data: Arc::new(data),
        }
    }

    #[test]
    fn yuv_primaries_in_both_matrices() {
        // Limited range: black (16,128,128), white (235,128,128).
        assert_eq!(yuv_to_rgb(16, 128, 128, false), [0, 0, 0]);
        assert_eq!(yuv_to_rgb(235, 128, 128, false), [255, 255, 255]);
        assert_eq!(yuv_to_rgb(235, 128, 128, true), [255, 255, 255]);
        // Strong red V pushes red up and blue down in both matrices.
        for hd in [false, true] {
            let [r, g, b] = yuv_to_rgb(81, 90, 240, hd);
            assert!(r > 200 && g < 60 && b < 60, "{hd}: {r} {g} {b}");
        }
        // Out-of-range input clamps instead of wrapping.
        assert_eq!(yuv_to_rgb(255, 255, 255, false)[0], 255);
        assert_eq!(yuv_to_rgb(0, 0, 0, true)[0], 0);
    }

    #[test]
    fn bgra_to_rgba_swaps_channels_and_mirrors() {
        // Two pixels: blue-ish then red-ish, stride padded to 12 bytes.
        let data = vec![200, 10, 5, 255, 5, 10, 200, 255, 0, 0, 0, 0];
        let f = frame(PixelFormat::Bgra, 2, 1, 12, data);
        let img = f.to_rgba(100, false).unwrap();
        assert_eq!((img.width, img.height), (2, 1));
        assert_eq!(img.data, vec![5, 10, 200, 255, 200, 10, 5, 255]);
        let mirrored = f.to_rgba(100, true).unwrap();
        assert_eq!(mirrored.data, vec![200, 10, 5, 255, 5, 10, 200, 255]);
    }

    #[test]
    fn nv12_and_yuy2_agree_on_the_same_picture() {
        // 2x2: luma 100, 120 / 140, 160; chroma neutral.
        let nv12 = frame(
            PixelFormat::Nv12,
            2,
            2,
            2,
            vec![100, 120, 140, 160, 128, 128],
        );
        let yuy2 = frame(
            PixelFormat::Yuy2,
            2,
            2,
            4,
            vec![100, 128, 120, 128, 140, 128, 160, 128],
        );
        let a = nv12.to_rgba(100, false).unwrap();
        let b = yuy2.to_rgba(100, false).unwrap();
        assert_eq!(a.data, b.data);
        // Neutral chroma gives grey: r == g == b, getting lighter with luma.
        let grey: Vec<u8> = a.data.chunks(4).map(|p| p[0]).collect();
        assert!(a
            .data
            .chunks(4)
            .all(|p| p[0] == p[1] && p[1] == p[2] && p[3] == 255));
        assert!(grey.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn downscale_keeps_the_aspect_and_short_buffers_are_refused() {
        let w = 64u32;
        let h = 32u32;
        let f = frame(
            PixelFormat::Nv12,
            w,
            h,
            w as usize,
            vec![128; (w * h * 3 / 2) as usize],
        );
        let img = f.to_rgba(16, false).unwrap();
        assert_eq!((img.width, img.height), (16, 8));
        assert_eq!(img.data.len(), 16 * 8 * 4);
        let short = frame(PixelFormat::Nv12, w, h, w as usize, vec![0; 100]);
        assert!(short.to_rgba(16, false).is_none());
        let thin = frame(PixelFormat::Bgra, 4, 1, 8, vec![0; 8]);
        assert!(
            thin.to_rgba(16, false).is_none(),
            "stride below the row size"
        );
        assert!(frame(PixelFormat::Bgra, 0, 0, 0, vec![])
            .to_rgba(16, false)
            .is_none());
    }

    #[test]
    fn capture_stamps_are_believed_only_when_plausible() {
        let now = 50_000_000_000u64;
        // 100 ns units: 49.9 s.
        assert_eq!(capture_host_ns(499_000_000, now), 49_900_000_000);
        assert_eq!(capture_host_ns(0, now), now);
        assert_eq!(capture_host_ns(-5, now), now);
        // In the future: not believed.
        assert_eq!(capture_host_ns(600_000_000, now), now);
        // More than 5 s old: a stuck device clock.
        assert_eq!(capture_host_ns(10_000_000, now), now);
    }

    #[test]
    fn preview_rate_drops_frames_with_slack() {
        assert!(preview_due(None, 0, 15));
        let step = 1_000_000_000 / 15;
        assert!(!preview_due(Some(0), step / 2, 15));
        // Two thirds of the way is still too early.
        assert!(!preview_due(Some(0), step * 2 / 3, 15));
        assert!(preview_due(Some(0), step - 1_000_000, 15)); // within 10 % slack
        assert!(preview_due(Some(0), step * 2, 15));
    }

    #[test]
    fn modes_fold_by_size_largest_first() {
        let modes = fold_modes(&[
            raw(640, 480, 5.0, 30.0, Subtype::Yuy2),
            raw(1920, 1080, 5.0, 30.0, Subtype::Mjpg),
            raw(1920, 1080, 5.0, 5.0, Subtype::Yuy2),
            raw(1280, 720, 10.0, 60.0, Subtype::Nv12),
            raw(0, 0, 0.0, 0.0, Subtype::Other),
        ]);
        let sizes: Vec<(u32, u32)> = modes.iter().map(|m| (m.width, m.height)).collect();
        assert_eq!(sizes, vec![(1920, 1080), (1280, 720), (640, 480)]);
        assert_eq!(modes[0].max_fps, 30.0);
        assert_eq!(modes[1].max_fps, 60.0);
    }

    #[test]
    fn picking_prefers_the_asked_rate_then_size_then_the_cheapest_layout() {
        let list = [
            raw(1920, 1080, 5.0, 5.0, Subtype::Yuy2), // 0: right size, only 5 fps
            raw(1920, 1080, 5.0, 30.0, Subtype::Mjpg), // 1
            raw(1280, 720, 5.0, 30.0, Subtype::Yuy2), // 2
            raw(1280, 720, 5.0, 30.0, Subtype::Nv12), // 3
            raw(640, 480, 5.0, 30.0, Subtype::Yuy2),  // 4
        ];
        assert_eq!(
            pick_mode(&list, 1080, 30),
            Some(1),
            "1080p30 needs MJPG here"
        );
        assert_eq!(
            pick_mode(&list, 720, 30),
            Some(3),
            "NV12 beats YUY2 at 720p"
        );
        assert_eq!(
            pick_mode(&list, 1080, 5),
            Some(0),
            "YUY2 beats MJPG at 5 fps"
        );
        // Wants more than the camera has: the largest below.
        assert_eq!(
            pick_mode(&list[2..], 1080, 30),
            Some(1),
            "720p NV12 of the slice"
        );
        // Nobody offers 60: the closest top rate, then the size rules.
        assert_eq!(pick_mode(&list, 720, 60), Some(3));
        assert_eq!(pick_mode(&[], 720, 30), None);
        assert_eq!(
            pick_mode(&[raw(0, 0, 0.0, 0.0, Subtype::Other)], 720, 30),
            None
        );
    }

    #[test]
    fn a_rate_nobody_offers_falls_back_to_the_nearest_top_rate() {
        let list = [
            raw(1280, 720, 5.0, 24.0, Subtype::Nv12),
            raw(1280, 720, 5.0, 30.0, Subtype::Yuy2),
        ];
        assert_eq!(pick_mode(&list, 720, 60), Some(1));
    }

    #[test]
    fn classification_uses_the_device_path_and_the_name() {
        use CameraKind::*;
        assert_eq!(
            classify_camera(
                "Integrated Camera",
                r"\\?\usb#vid_04f2&pid_b6d9&mi_00#6&abc#{e5323777}\global"
            ),
            BuiltIn
        );
        assert_eq!(
            classify_camera(
                "Logitech BRIO",
                r"\\?\usb#vid_046d&pid_085e&mi_00#7&1#{e5323777}\global"
            ),
            External
        );
        assert_eq!(
            classify_camera(
                "OBS Virtual Camera",
                r"\\?\root#image#0000#{e5323777}\global"
            ),
            Virtual
        );
        assert_eq!(
            classify_camera("Any Camera", r"\\?\swd#something#{e5323777}"),
            Virtual
        );
        assert_eq!(classify_camera("Some Virtual Camera", ""), Virtual);
        // A real USB device that has "virtual" in its name stays a hardware camera.
        assert_eq!(
            classify_camera("Virtual Reality Cam", r"\\?\usb#vid_1&pid_2#3#{e5323777}"),
            External
        );
    }

    #[test]
    fn camera_switches_add_up_like_the_microphone_ones() {
        let st = windows_camera_state;
        assert_eq!(st(None, None, None, None), CameraPermission::Authorized);
        assert_eq!(
            st(None, Some("Allow"), Some("Deny"), None),
            CameraPermission::Denied
        );
        assert_eq!(
            st(None, None, Some("Allow"), Some("Deny")),
            CameraPermission::Denied
        );
        assert_eq!(
            st(Some(2), Some("Allow"), None, None),
            CameraPermission::Restricted
        );
        assert_eq!(
            st(Some(1), Some("Deny"), Some("Deny"), None),
            CameraPermission::Authorized
        );
    }

    #[test]
    fn nv12_to_nv12_at_the_same_size_drops_the_row_padding() {
        // 4x2, stride 6: two Y rows then one UV row, each padded by 2 bytes.
        let mut d = vec![0u8; 6 * 2 + 6];
        d[..4].copy_from_slice(&[10, 11, 12, 13]);
        d[6..10].copy_from_slice(&[20, 21, 22, 23]);
        d[12..16].copy_from_slice(&[100, 200, 101, 201]);
        let out = frame(PixelFormat::Nv12, 4, 2, 6, d).to_nv12(4, 2).unwrap();
        assert_eq!(
            out,
            vec![10, 11, 12, 13, 20, 21, 22, 23, 100, 200, 101, 201]
        );
    }

    #[test]
    fn yuy2_becomes_nv12_with_chroma_averaged_over_two_rows() {
        // 4x2 YUY2: each row is (Y0 U Y1 V) twice.
        let d = vec![
            1, 100, 2, 200, 3, 102, 4, 202, // row 0
            5, 110, 6, 210, 7, 112, 8, 212, // row 1
        ];
        let out = frame(PixelFormat::Yuy2, 4, 2, 8, d).to_nv12(4, 2).unwrap();
        assert_eq!(&out[..8], &[1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(&out[8..], &[105, 205, 107, 207]);
    }

    #[test]
    fn nv12_scales_down_by_area_average() {
        // 4x4 to 2x2: each output luma is the mean of a 2x2 box; the one chroma pair of the
        // output is the mean of the four source pairs.
        let mut d = vec![0u8; 16 + 8];
        for (i, v) in d[..16].iter_mut().enumerate() {
            *v = (i as u8 / 4) * 40 + (i as u8 % 4) * 4;
        }
        d[16..].copy_from_slice(&[10, 20, 30, 40, 50, 60, 70, 80]);
        let out = frame(PixelFormat::Nv12, 4, 4, 4, d).to_nv12(2, 2).unwrap();
        assert_eq!(&out[..4], &[22, 30, 102, 110]);
        assert_eq!(&out[4..], &[40, 50]);
    }

    #[test]
    fn to_nv12_refuses_rgb_odd_sizes_and_short_buffers() {
        let rgb = frame(PixelFormat::Bgra, 2, 2, 8, vec![0; 16]);
        assert!(rgb.to_nv12(2, 2).is_none(), "RGB goes to the encoder as is");
        let nv = frame(PixelFormat::Nv12, 4, 2, 4, vec![0; 12]);
        assert!(nv.to_nv12(3, 2).is_none());
        assert!(nv.to_nv12(4, 0).is_none());
        assert!(frame(PixelFormat::Nv12, 4, 2, 4, vec![0; 5])
            .to_nv12(4, 2)
            .is_none());
        assert!(frame(PixelFormat::Yuy2, 4, 2, 4, vec![0; 16])
            .to_nv12(4, 2)
            .is_none());
    }

    #[test]
    fn nv12_upscales_without_reading_outside_the_frame() {
        let nv = frame(PixelFormat::Nv12, 2, 2, 2, vec![50, 60, 70, 80, 128, 129]);
        let out = nv.to_nv12(4, 4).unwrap();
        assert_eq!(out.len(), 4 * 4 + 4 * 2);
        assert!(out[..16].iter().all(|v| (50..=80).contains(v)));
        assert!(out[16..].chunks(2).all(|p| p == [128, 129]));
    }
}
