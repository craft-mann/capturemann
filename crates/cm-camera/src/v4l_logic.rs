//! Platform-neutral helpers of the Linux camera backend (Video4Linux2): pixel-format codes, which
//! device nodes are cameras, how a node becomes a stable id, camera classification, capture
//! stamps, folding the sizes and frame intervals a driver lists into capture modes, the Motion JPEG
//! decoder and the permission summary. Nothing here touches the OS, so it is tested on every
//! system; the ioctl calls live in `sys/`.

use std::path::Path;
use std::sync::Arc;

use zune_core::bytestream::ZCursor;
use zune_core::colorspace::ColorSpace;
use zune_core::options::DecoderOptions;
use zune_jpeg::JpegDecoder;

use crate::convert::{PixelFormat, RawFrame, RawMode, Subtype};
use crate::permission::CameraPermission;
use crate::types::CameraKind;

// ---- pixel formats -------------------------------------------------------------------------

/// A V4L2 four-character code (`v4l2_fourcc`: the bytes, first one lowest).
pub const fn fourcc(s: &[u8; 4]) -> u32 {
    (s[0] as u32) | ((s[1] as u32) << 8) | ((s[2] as u32) << 16) | ((s[3] as u32) << 24)
}

pub const FOURCC_YUYV: u32 = fourcc(b"YUYV");
pub const FOURCC_NV12: u32 = fourcc(b"NV12");
pub const FOURCC_MJPEG: u32 = fourcc(b"MJPG");
pub const FOURCC_JPEG: u32 = fourcc(b"JPEG");

/// The picture layouts the Linux source can use, as the mode picker sees them. `None` for
/// anything else (H.264 streams, Bayer, depth, infrared): such a format is not listed.
pub fn subtype_of(code: u32) -> Option<Subtype> {
    match code {
        FOURCC_NV12 => Some(Subtype::Nv12),
        FOURCC_YUYV => Some(Subtype::Yuy2),
        FOURCC_MJPEG | FOURCC_JPEG => Some(Subtype::Mjpg),
        _ => None,
    }
}

/// The code as text for messages (`YUYV`).
pub fn fourcc_text(code: u32) -> String {
    code.to_le_bytes()
        .iter()
        .map(|b| {
            if b.is_ascii_graphic() {
                *b as char
            } else {
                '?'
            }
        })
        .collect()
}

// ---- device nodes --------------------------------------------------------------------------

pub const CAP_VIDEO_CAPTURE: u32 = 0x0000_0001;
pub const CAP_STREAMING: u32 = 0x0400_0000;
pub const CAP_DEVICE_CAPS: u32 = 0x8000_0000;

/// Whether a node is a camera we can read single-plane frames from. `capabilities` is what the
/// driver as a whole offers, `device_caps` what this node does (valid when `CAP_DEVICE_CAPS` is
/// set in `capabilities`). Metadata nodes (a UVC camera has one next to its video node), radio and
/// output nodes fail this test.
pub fn is_capture_node(capabilities: u32, device_caps: u32) -> bool {
    let caps = if capabilities & CAP_DEVICE_CAPS != 0 {
        device_caps
    } else {
        capabilities
    };
    caps & CAP_VIDEO_CAPTURE != 0 && caps & CAP_STREAMING != 0
}

/// The number in `/dev/video7` (for ordering: `video2` before `video10`).
pub fn node_index(path: &Path) -> Option<u32> {
    path.file_name()?
        .to_str()?
        .strip_prefix("video")?
        .parse()
        .ok()
}

/// Picks the stable id of a node out of the symlinks udev keeps for it: `by-id` first (survives
/// replugging into another port), then `by-path`, else the node itself. `links` are full paths.
pub fn stable_id(node: &Path, links: &[String]) -> String {
    let pick = |dir: &str| {
        links
            .iter()
            .filter(|l| l.starts_with(dir))
            // `-video-index0` is the picture node; index1 is the metadata one.
            .find(|l| l.ends_with("-video-index0"))
            .or_else(|| links.iter().find(|l| l.starts_with(dir)))
            .cloned()
    };
    pick("/dev/v4l/by-id/")
        .or_else(|| pick("/dev/v4l/by-path/"))
        .unwrap_or_else(|| node.to_string_lossy().into_owned())
}

/// What kind of camera a node is. `device_path` is the canonical sysfs path of the node's device
/// (`/sys/devices/pci.../usb1/1-8/1-8:1.0`), `removable` the `removable` attribute of its USB
/// device (`fixed`, `removable` or `unknown`).
pub fn classify_linux(name: &str, device_path: &str, removable: Option<&str>) -> CameraKind {
    let n = name.to_ascii_lowercase();
    if device_path.contains("/devices/virtual/")
        || n.contains("v4l2loopback")
        || n.contains("virtual")
        || n.contains("dummy video")
    {
        return CameraKind::Virtual;
    }
    match removable.map(str::trim) {
        Some("fixed") => CameraKind::BuiltIn,
        Some("removable") => CameraKind::External,
        _ if n.contains("integrated") || n.contains("built-in") || n.contains("builtin") => {
            CameraKind::BuiltIn
        }
        _ => CameraKind::External,
    }
}

// ---- time ----------------------------------------------------------------------------------

pub const BUF_FLAG_TIMESTAMP_MASK: u32 = 0x0000_e000;
pub const BUF_FLAG_TIMESTAMP_MONOTONIC: u32 = 0x0000_2000;

/// How far a driver's frame stamp may be from the host clock at delivery before it is not
/// believed.
pub const MAX_STAMP_SKEW_NS: u64 = 5_000_000_000;

/// The capture time of a frame on the host clock (`CLOCK_MONOTONIC` ns). A V4L2 buffer carries a
/// `timeval`; it is on the monotonic clock only when the buffer says so
/// (`V4L2_BUF_FLAG_TIMESTAMP_MONOTONIC`, what UVC and other real drivers set). Any other clock,
/// an empty stamp, one in the future or further than [`MAX_STAMP_SKEW_NS`] behind falls back to
/// the time of delivery.
pub fn capture_stamp_ns(tv_sec: i64, tv_usec: i64, flags: u32, delivered_ns: u64) -> u64 {
    if flags & BUF_FLAG_TIMESTAMP_MASK != BUF_FLAG_TIMESTAMP_MONOTONIC {
        return delivered_ns;
    }
    let (Ok(s), Ok(us)) = (u64::try_from(tv_sec), u64::try_from(tv_usec)) else {
        return delivered_ns;
    };
    let ns = s
        .saturating_mul(1_000_000_000)
        .saturating_add(us.saturating_mul(1000));
    if ns == 0 || ns > delivered_ns || delivered_ns - ns > MAX_STAMP_SKEW_NS {
        delivered_ns
    } else {
        ns
    }
}

// ---- modes ---------------------------------------------------------------------------------

/// A frame interval (seconds per frame as `num/den`) as frames per second.
pub fn fps_of(num: u32, den: u32) -> Option<f64> {
    (num > 0 && den > 0).then(|| f64::from(den) / f64::from(num))
}

/// One capture mode of a node: [`RawMode`] plus the code to ask the driver for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodeMode {
    pub fourcc: u32,
    pub mode: RawMode,
}

/// A mode from the frame rates a driver lists for one size and format (empty list: a driver that
/// lists none, assumed 30 fps so the mode is not lost).
pub fn node_mode(fourcc: u32, width: u32, height: u32, fps_list: &[f64]) -> Option<NodeMode> {
    let subtype = subtype_of(fourcc)?;
    let (min_fps, max_fps) = if fps_list.is_empty() {
        (30.0, 30.0)
    } else {
        (
            fps_list.iter().copied().fold(f64::MAX, f64::min),
            fps_list.iter().copied().fold(0.0, f64::max),
        )
    };
    (width > 0 && height > 0).then_some(NodeMode {
        fourcc,
        mode: RawMode {
            width,
            height,
            min_fps,
            max_fps,
            subtype,
        },
    })
}

/// The sizes to offer for a stepwise or continuous range (no camera that matters has one, but a
/// virtual camera can): the usual video sizes inside the range.
pub fn stepwise_sizes(min: (u32, u32), max: (u32, u32), step: (u32, u32)) -> Vec<(u32, u32)> {
    const COMMON: [(u32, u32); 6] = [
        (320, 240),
        (640, 480),
        (800, 600),
        (1280, 720),
        (1920, 1080),
        (3840, 2160),
    ];
    let on_grid = |v: u32, lo: u32, st: u32| st <= 1 || (v >= lo && (v - lo).is_multiple_of(st));
    COMMON
        .iter()
        .copied()
        .filter(|&(w, h)| {
            w >= min.0
                && w <= max.0
                && h >= min.1
                && h <= max.1
                && on_grid(w, min.0, step.0)
                && on_grid(h, min.1, step.1)
        })
        .collect()
}

// ---- permission ----------------------------------------------------------------------------

/// What the permission to use cameras comes to on Linux. There is no prompt: a normal desktop
/// login gives the user access to video nodes (udev `uaccess`, or the `video` group). With no
/// camera at all there is nothing to deny; a camera that exists but cannot be opened for
/// permission reasons means `Denied`.
pub fn permission_summary(nodes: usize, opened: usize, denied: usize) -> CameraPermission {
    if opened > 0 || nodes == 0 || denied == 0 {
        CameraPermission::Authorized
    } else {
        CameraPermission::Denied
    }
}

// ---- Motion JPEG ---------------------------------------------------------------------------

/// Decodes one Motion JPEG frame to packed BGRA. `None` for a damaged picture (cameras do send
/// truncated frames now and then; the caller skips them).
pub fn decode_mjpeg(data: &[u8]) -> Option<RawFrame> {
    let patched;
    let data = if has_huffman_tables(data) {
        data
    } else {
        patched = insert_standard_tables(data)?;
        &patched[..]
    };
    let opts = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::BGRA);
    let mut dec = JpegDecoder::new_with_options(ZCursor::new(data), opts);
    dec.decode_headers().ok()?;
    let info = dec.info()?;
    let (w, h) = (u32::from(info.width), u32::from(info.height));
    if w == 0 || h == 0 {
        return None;
    }
    let mut out = vec![0u8; w as usize * h as usize * 4];
    dec.decode_into(&mut out).ok()?;
    Some(RawFrame {
        format: PixelFormat::Bgra,
        width: w,
        height: h,
        stride: w as usize * 4,
        data: Arc::new(out),
    })
}

/// Whether the stream carries a Huffman table segment (`FFC4`) before its scan.
fn has_huffman_tables(data: &[u8]) -> bool {
    segments(data).any(|(m, _)| m == 0xC4)
}

/// `(marker, offset of the 0xFF)` of each marker segment up to the start of scan.
fn segments(data: &[u8]) -> impl Iterator<Item = (u8, usize)> + '_ {
    let mut i = 2usize; // after SOI
    std::iter::from_fn(move || {
        if i + 4 > data.len() || data[i] != 0xFF {
            return None;
        }
        let marker = data[i + 1];
        let at = i;
        if marker == 0xDA {
            i = data.len();
            return Some((marker, at));
        }
        let len = usize::from(u16::from_be_bytes([data[i + 2], data[i + 3]]));
        i += 2 + len;
        Some((marker, at))
    })
}

/// Many UVC cameras leave out the Huffman tables (the JPEG standard's example tables are
/// implied, ITU T.81 Annex K.3). Inserts them before the start of scan; `None` if the stream has
/// no start of scan.
fn insert_standard_tables(data: &[u8]) -> Option<Vec<u8>> {
    let (_, sos) = segments(data).find(|(m, _)| *m == 0xDA)?;
    let mut out = Vec::with_capacity(data.len() + 420);
    out.extend_from_slice(&data[..sos]);
    out.extend_from_slice(std_dht());
    out.extend_from_slice(&data[sos..]);
    Some(out)
}

/// The four standard tables as one DHT segment (marker and length included).
fn std_dht() -> &'static [u8] {
    static SEGMENT: std::sync::OnceLock<[u8; 420]> = std::sync::OnceLock::new();
    SEGMENT.get_or_init(build_std_dht)
}

fn build_std_dht() -> [u8; 420] {
    const DC_LUM_BITS: [u8; 16] = [0, 1, 5, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0];
    const DC_CHR_BITS: [u8; 16] = [0, 3, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0];
    const DC_VALS: [u8; 12] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];
    const AC_LUM_BITS: [u8; 16] = [0, 2, 1, 3, 3, 2, 4, 3, 5, 5, 4, 4, 0, 0, 1, 0x7d];
    const AC_LUM_VALS: [u8; 162] = [
        0x01, 0x02, 0x03, 0x00, 0x04, 0x11, 0x05, 0x12, 0x21, 0x31, 0x41, 0x06, 0x13, 0x51, 0x61,
        0x07, 0x22, 0x71, 0x14, 0x32, 0x81, 0x91, 0xa1, 0x08, 0x23, 0x42, 0xb1, 0xc1, 0x15, 0x52,
        0xd1, 0xf0, 0x24, 0x33, 0x62, 0x72, 0x82, 0x09, 0x0a, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x25,
        0x26, 0x27, 0x28, 0x29, 0x2a, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a, 0x43, 0x44, 0x45,
        0x46, 0x47, 0x48, 0x49, 0x4a, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5a, 0x63, 0x64,
        0x65, 0x66, 0x67, 0x68, 0x69, 0x6a, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7a, 0x83,
        0x84, 0x85, 0x86, 0x87, 0x88, 0x89, 0x8a, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99,
        0x9a, 0xa2, 0xa3, 0xa4, 0xa5, 0xa6, 0xa7, 0xa8, 0xa9, 0xaa, 0xb2, 0xb3, 0xb4, 0xb5, 0xb6,
        0xb7, 0xb8, 0xb9, 0xba, 0xc2, 0xc3, 0xc4, 0xc5, 0xc6, 0xc7, 0xc8, 0xc9, 0xca, 0xd2, 0xd3,
        0xd4, 0xd5, 0xd6, 0xd7, 0xd8, 0xd9, 0xda, 0xe1, 0xe2, 0xe3, 0xe4, 0xe5, 0xe6, 0xe7, 0xe8,
        0xe9, 0xea, 0xf1, 0xf2, 0xf3, 0xf4, 0xf5, 0xf6, 0xf7, 0xf8, 0xf9, 0xfa,
    ];
    const AC_CHR_BITS: [u8; 16] = [0, 2, 1, 2, 4, 4, 3, 4, 7, 5, 4, 4, 0, 1, 2, 0x77];
    const AC_CHR_VALS: [u8; 162] = [
        0x00, 0x01, 0x02, 0x03, 0x11, 0x04, 0x05, 0x21, 0x31, 0x06, 0x12, 0x41, 0x51, 0x07, 0x61,
        0x71, 0x13, 0x22, 0x32, 0x81, 0x08, 0x14, 0x42, 0x91, 0xa1, 0xb1, 0xc1, 0x09, 0x23, 0x33,
        0x52, 0xf0, 0x15, 0x62, 0x72, 0xd1, 0x0a, 0x16, 0x24, 0x34, 0xe1, 0x25, 0xf1, 0x17, 0x18,
        0x19, 0x1a, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a, 0x43, 0x44,
        0x45, 0x46, 0x47, 0x48, 0x49, 0x4a, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5a, 0x63,
        0x64, 0x65, 0x66, 0x67, 0x68, 0x69, 0x6a, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7a,
        0x82, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89, 0x8a, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97,
        0x98, 0x99, 0x9a, 0xa2, 0xa3, 0xa4, 0xa5, 0xa6, 0xa7, 0xa8, 0xa9, 0xaa, 0xb2, 0xb3, 0xb4,
        0xb5, 0xb6, 0xb7, 0xb8, 0xb9, 0xba, 0xc2, 0xc3, 0xc4, 0xc5, 0xc6, 0xc7, 0xc8, 0xc9, 0xca,
        0xd2, 0xd3, 0xd4, 0xd5, 0xd6, 0xd7, 0xd8, 0xd9, 0xda, 0xe2, 0xe3, 0xe4, 0xe5, 0xe6, 0xe7,
        0xe8, 0xe9, 0xea, 0xf2, 0xf3, 0xf4, 0xf5, 0xf6, 0xf7, 0xf8, 0xf9, 0xfa,
    ];
    let mut out = [0u8; 420];
    out[0] = 0xFF;
    out[1] = 0xC4;
    out[2] = (418u16 >> 8) as u8;
    out[3] = (418u16 & 0xFF) as u8;
    let mut at = 4;
    // (class and id, code length counts, values)
    let mut put = |tc_th: u8, bits: &[u8; 16], vals: &[u8]| {
        out[at] = tc_th;
        at += 1;
        for b in bits.iter().chain(vals) {
            out[at] = *b;
            at += 1;
        }
    };
    put(0x00, &DC_LUM_BITS, &DC_VALS);
    put(0x10, &AC_LUM_BITS, &AC_LUM_VALS);
    put(0x01, &DC_CHR_BITS, &DC_VALS);
    put(0x11, &AC_CHR_BITS, &AC_CHR_VALS);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fourcc_codes_match_the_kernel_headers() {
        assert_eq!(FOURCC_YUYV, 0x5659_5559);
        assert_eq!(FOURCC_MJPEG, 0x4750_4A4D);
        assert_eq!(FOURCC_NV12, 0x3231_564E);
        assert_eq!(fourcc_text(FOURCC_YUYV), "YUYV");
        assert_eq!(subtype_of(FOURCC_MJPEG), Some(Subtype::Mjpg));
        assert_eq!(subtype_of(fourcc(b"H264")), None);
    }

    #[test]
    fn capture_nodes_are_told_from_metadata_nodes() {
        // A UVC camera: video node and metadata node share the driver's capabilities.
        let driver = 0x8400_0001 | 0x0080_0000;
        assert!(is_capture_node(driver, 0x0400_0001));
        assert!(!is_capture_node(driver, 0x0480_0000)); // META_CAPTURE | STREAMING | EXT_PIX
                                                        // No per-node caps: the driver's own decide.
        assert!(is_capture_node(0x0400_0001, 0));
        assert!(!is_capture_node(0x0000_0001, 0)); // cannot stream
    }

    #[test]
    fn nodes_sort_numerically() {
        let mut v = [
            Path::new("/dev/video10"),
            Path::new("/dev/video2"),
            Path::new("/dev/video0"),
        ];
        v.sort_by_key(|p| node_index(p));
        assert_eq!(v[2], Path::new("/dev/video10"));
        assert_eq!(node_index(Path::new("/dev/media0")), None);
    }

    #[test]
    fn stable_id_prefers_by_id_and_the_picture_node() {
        let node = Path::new("/dev/video0");
        let links = vec![
            "/dev/v4l/by-path/pci-0000:00:14.0-usb-0:8:1.0-video-index0".to_string(),
            "/dev/v4l/by-id/usb-Chicony_Integrated_Camera-video-index0".to_string(),
        ];
        assert_eq!(
            stable_id(node, &links),
            "/dev/v4l/by-id/usb-Chicony_Integrated_Camera-video-index0"
        );
        assert_eq!(
            stable_id(node, &links[..1]),
            "/dev/v4l/by-path/pci-0000:00:14.0-usb-0:8:1.0-video-index0"
        );
        assert_eq!(stable_id(node, &[]), "/dev/video0");
    }

    #[test]
    fn kinds_follow_sysfs_before_the_name() {
        use CameraKind::*;
        let usb = "/sys/devices/pci0000:00/0000:00:14.0/usb1/1-8/1-8:1.0";
        assert_eq!(
            classify_linux("Integrated Camera", usb, Some("fixed")),
            BuiltIn
        );
        assert_eq!(
            classify_linux("HD Pro Webcam C920", usb, Some("removable")),
            External
        );
        assert_eq!(classify_linux("Integrated Camera", usb, None), BuiltIn);
        assert_eq!(classify_linux("Some USB Cam", usb, None), External);
        assert_eq!(
            classify_linux(
                "Dummy video device (0x0000)",
                "/sys/devices/virtual/video4linux/video9",
                None
            ),
            Virtual
        );
    }

    #[test]
    fn stamps_are_believed_only_on_the_monotonic_clock() {
        let delivered = 100_000_000_000u64;
        let mono = BUF_FLAG_TIMESTAMP_MONOTONIC;
        // 99.97 s on the monotonic clock.
        assert_eq!(
            capture_stamp_ns(99, 970_000, mono, delivered),
            99_970_000_000
        );
        // Another clock (copy flag) or no flag: delivery time.
        assert_eq!(capture_stamp_ns(99, 970_000, 0x4000, delivered), delivered);
        assert_eq!(capture_stamp_ns(99, 970_000, 0, delivered), delivered);
        // Future, empty, negative, stale.
        assert_eq!(capture_stamp_ns(101, 0, mono, delivered), delivered);
        assert_eq!(capture_stamp_ns(0, 0, mono, delivered), delivered);
        assert_eq!(capture_stamp_ns(-1, 0, mono, delivered), delivered);
        assert_eq!(capture_stamp_ns(50, 0, mono, delivered), delivered);
    }

    #[test]
    fn intervals_become_rates_and_modes() {
        assert_eq!(fps_of(1, 30), Some(30.0));
        assert_eq!(fps_of(1, 0), None);
        let m = node_mode(FOURCC_YUYV, 1280, 720, &[10.0, 7.5, 5.0]).unwrap();
        assert_eq!((m.mode.min_fps, m.mode.max_fps), (5.0, 10.0));
        assert_eq!(m.mode.subtype, Subtype::Yuy2);
        let m = node_mode(FOURCC_MJPEG, 640, 480, &[]).unwrap();
        assert_eq!((m.mode.min_fps, m.mode.max_fps), (30.0, 30.0));
        assert!(node_mode(fourcc(b"H264"), 640, 480, &[30.0]).is_none());
        assert!(node_mode(FOURCC_YUYV, 0, 480, &[30.0]).is_none());
    }

    #[test]
    fn stepwise_ranges_offer_the_common_sizes_inside_them() {
        assert_eq!(
            stepwise_sizes((320, 240), (1920, 1080), (16, 8)),
            vec![
                (320, 240),
                (640, 480),
                (800, 600),
                (1280, 720),
                (1920, 1080)
            ]
        );
        assert!(stepwise_sizes((10, 10), (100, 100), (2, 2)).is_empty());
    }

    #[test]
    fn permission_is_denied_only_when_a_camera_exists_but_cannot_be_opened() {
        use CameraPermission::*;
        assert_eq!(permission_summary(0, 0, 0), Authorized);
        assert_eq!(permission_summary(2, 2, 0), Authorized);
        assert_eq!(permission_summary(2, 0, 2), Denied);
        assert_eq!(permission_summary(2, 1, 1), Authorized);
        // Busy or odd nodes that fail for another reason do not mean "denied".
        assert_eq!(permission_summary(2, 0, 0), Authorized);
    }

    #[test]
    fn the_standard_table_segment_is_well_formed() {
        assert_eq!(&std_dht()[..2], &[0xFF, 0xC4]);
        let len = usize::from(u16::from_be_bytes([std_dht()[2], std_dht()[3]]));
        assert_eq!(len + 2, std_dht().len());
        // Each table: class byte, 16 counts, then as many values as the counts add up to.
        let mut at = 4;
        let mut tables = 0;
        while at < std_dht().len() {
            let total: usize = std_dht()[at + 1..at + 17]
                .iter()
                .map(|&c| usize::from(c))
                .sum();
            at += 17 + total;
            tables += 1;
        }
        assert_eq!((at, tables), (std_dht().len(), 4));
    }

    #[test]
    fn a_stream_without_tables_gets_them_before_the_scan() {
        // SOI, a fake 2-byte APP0 segment (length 4), SOS marker.
        let jpeg = [
            0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0x00, 0x00, 0xFF, 0xDA, 0x00, 0x02,
        ];
        assert!(!has_huffman_tables(&jpeg));
        let patched = insert_standard_tables(&jpeg).unwrap();
        assert!(has_huffman_tables(&patched));
        assert_eq!(patched.len(), jpeg.len() + std_dht().len());
        assert_eq!(&patched[patched.len() - 4..], &jpeg[8..]);
        assert!(decode_mjpeg(&[0xFF, 0xD8]).is_none());
        assert!(decode_mjpeg(&[]).is_none());
    }

    /// A 32x16 test picture made with the standard tables, as a camera sends it.
    const SAMPLE: &[u8] = include_bytes!("../tests/data/mjpeg_32x16.jpg");

    fn without_tables(jpeg: &[u8]) -> Vec<u8> {
        let (_, at) = segments(jpeg).find(|(m, _)| *m == 0xC4).unwrap();
        let len = usize::from(u16::from_be_bytes([jpeg[at + 2], jpeg[at + 3]]));
        [&jpeg[..at], &jpeg[at + 2 + len..]].concat()
    }

    #[test]
    fn a_picture_decodes_with_and_without_its_tables() {
        let a = decode_mjpeg(SAMPLE).unwrap();
        assert_eq!((a.width, a.height, a.format), (32, 16, PixelFormat::Bgra));
        assert_eq!(a.data.len(), 32 * 16 * 4);
        let bare = without_tables(SAMPLE);
        assert!(!has_huffman_tables(&bare));
        let b = decode_mjpeg(&bare).unwrap();
        assert_eq!(
            a.data, b.data,
            "the standard tables must reproduce the picture"
        );
        // And it is not blank.
        assert!(a.data.iter().any(|&v| v != a.data[0]));
        // A cut-off frame is skipped, not a panic.
        assert!(decode_mjpeg(&SAMPLE[..SAMPLE.len() / 2]).is_none());
    }
}
