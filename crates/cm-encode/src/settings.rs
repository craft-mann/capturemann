//! Recording quality settings resolved to a concrete encode plan (REC-2, REC-3).
//!
//! Pure functions: no OS calls, so size, codec, fps and bitrate selection is unit tested on any
//! machine. Sources: specs/00-product-spec.md "Recording quality spec" and
//! specs/features/03-recording.md REC-2 / REC-3.

use cm_core::CmError;
use serde::{Deserialize, Serialize};

/// H.264 cannot encode frames wider than this (REC-2: "H.264 can't encode frames wider than
/// 4096 px").
pub const MAX_H264_WIDTH: u32 = 4096;

/// Fragment length of every recording file (NFR-6, REC-8).
pub const FRAGMENT_SECS: u32 = 2;

/// The codec actually used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Codec {
    Hevc,
    H264,
}

impl Codec {
    pub fn as_str(self) -> &'static str {
        match self {
            Codec::Hevc => "hevc",
            Codec::H264 => "h264",
        }
    }
}

/// The user's codec choice (Recording settings table: "HEVC when width > 1920, else H.264").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CodecChoice {
    #[default]
    Auto,
    Hevc,
    H264,
}

/// Quality preset. `High` is the master target from the spec table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Quality {
    #[default]
    High,
    Balanced,
    Small,
}

impl Quality {
    /// Bitrate multiplier against the High table. The spec table only gives High, so Balanced
    /// and Small are an implementation decision (see the M3 slice 1 report).
    pub fn factor(self) -> f64 {
        match self {
            Quality::High => 1.0,
            Quality::Balanced => 0.6,
            Quality::Small => 0.35,
        }
    }
}

/// Output size option (REC-2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum OutputSize {
    #[default]
    #[serde(rename = "source")]
    Source,
    #[serde(rename = "2160p")]
    P2160,
    #[serde(rename = "1440p")]
    P1440,
    #[serde(rename = "1080p")]
    P1080,
    #[serde(rename = "720p")]
    P720,
}

impl OutputSize {
    /// Target height for the preset, `None` for Source.
    pub fn height(self) -> Option<u32> {
        match self {
            OutputSize::Source => None,
            OutputSize::P2160 => Some(2160),
            OutputSize::P1440 => Some(1440),
            OutputSize::P1080 => Some(1080),
            OutputSize::P720 => Some(720),
        }
    }
}

/// What the user chose (the `start_recording` size / fps / codec fields plus the quality preset).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct VideoSettings {
    pub size: OutputSize,
    /// 30 or 60 (REC-3).
    pub fps: u32,
    pub codec: CodecChoice,
    pub quality: Quality,
}

impl Default for VideoSettings {
    fn default() -> Self {
        VideoSettings {
            size: OutputSize::Source,
            fps: 30,
            codec: CodecChoice::Auto,
            quality: Quality::High,
        }
    }
}

/// The concrete encode parameters for a recording.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct VideoPlan {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub codec: Codec,
    pub bitrate_bps: u64,
    pub quality: Quality,
}

impl VideoPlan {
    /// Estimated disk use per minute in MB (10^6 bytes), shown in the settings panel (REC-2).
    pub fn disk_mb_per_minute(&self) -> f64 {
        disk_mb_per_minute(self.bitrate_bps)
    }
}

/// Resolves settings against the source size in physical px (the display, window or region).
///
/// REC-2: Source keeps the native size; presets downscale to an exact height with the width
/// rounded to an even number, and never upscale. REC-3: 30 or 60 fps only.
pub fn plan_video(
    source_width: u32,
    source_height: u32,
    s: &VideoSettings,
) -> Result<VideoPlan, CmError> {
    if s.fps != 30 && s.fps != 60 {
        return Err(CmError::invalid_field("fps", "must be 30 or 60"));
    }
    let (width, height) = output_dimensions(source_width, source_height, s.size)?;
    let codec = resolve_codec(s.codec, width)?;
    Ok(VideoPlan {
        width,
        height,
        fps: s.fps,
        codec,
        bitrate_bps: bitrate_bps(codec, s.fps, s.quality, width, height),
        quality: s.quality,
    })
}

/// Output width and height for a source size (REC-2.AC1..AC3). Both are even.
pub fn output_dimensions(
    source_width: u32,
    source_height: u32,
    size: OutputSize,
) -> Result<(u32, u32), CmError> {
    let (sw, sh) = (source_width & !1, source_height & !1);
    if sw == 0 || sh == 0 {
        return Err(CmError::invalid_input(format!(
            "Source {source_width}x{source_height} is too small to record"
        )));
    }
    let Some(target_h) = size.height() else {
        return Ok((sw, sh));
    };
    if target_h >= sh {
        // Downscaling only: a 1080p display recorded at "2160p" stays 1080p.
        return Ok((sw, sh));
    }
    let w = (f64::from(sw) * f64::from(target_h) / f64::from(sh) / 2.0).round() * 2.0;
    let w = (w as u32).max(2);
    Ok((w, target_h))
}

/// Codec for an output width (REC-2): Auto is HEVC above 1920 px; H.264 is refused above 4096 px.
pub fn resolve_codec(choice: CodecChoice, output_width: u32) -> Result<Codec, CmError> {
    match choice {
        CodecChoice::Hevc => Ok(Codec::Hevc),
        CodecChoice::H264 if output_width > MAX_H264_WIDTH => Err(CmError::invalid_input(
            "H.264 can't encode frames wider than 4096 px; use HEVC",
        )),
        CodecChoice::H264 => Ok(Codec::H264),
        CodecChoice::Auto if output_width > 1920 => Ok(Codec::Hevc),
        CodecChoice::Auto => Ok(Codec::H264),
    }
}

/// The spec's High (master) targets in Mbps: `[H.264 30, H.264 60, HEVC 30, HEVC 60]`.
const TABLE: [(u64, [f64; 4]); 4] = [
    (1280 * 720, [8.0, 12.0, 5.0, 8.0]),
    (1920 * 1080, [16.0, 24.0, 10.0, 16.0]),
    (2560 * 1440, [30.0, 45.0, 20.0, 30.0]),
    (3840 * 2160, [60.0, 90.0, 40.0, 60.0]),
];

/// Average bitrate target in bits per second.
///
/// Exact table values at 720p, 1080p, 1440p and 2160p pixel counts; linear in pixel count
/// between rows, and proportional to the nearest row beyond either end (5K scales up from the
/// 2160p row). Quality multiplies the result.
pub fn bitrate_bps(codec: Codec, fps: u32, quality: Quality, width: u32, height: u32) -> u64 {
    let col = match (codec, fps > 30) {
        (Codec::H264, false) => 0,
        (Codec::H264, true) => 1,
        (Codec::Hevc, false) => 2,
        (Codec::Hevc, true) => 3,
    };
    let px = u64::from(width) * u64::from(height);
    let mbps = interpolate(px, col);
    ((mbps * quality.factor() * 1e6).round() as u64).max(250_000)
}

fn interpolate(px: u64, col: usize) -> f64 {
    let (first_px, first) = TABLE[0];
    let (last_px, last) = TABLE[TABLE.len() - 1];
    if px <= first_px {
        return first[col] * px as f64 / first_px as f64;
    }
    if px >= last_px {
        return last[col] * px as f64 / last_px as f64;
    }
    for pair in TABLE.windows(2) {
        let ((p0, r0), (p1, r1)) = (pair[0], pair[1]);
        if px <= p1 {
            let t = (px - p0) as f64 / (p1 - p0) as f64;
            return r0[col] + t * (r1[col] - r0[col]);
        }
    }
    last[col]
}

/// MB (10^6 bytes) per minute at a bitrate.
pub fn disk_mb_per_minute(bitrate_bps: u64) -> f64 {
    bitrate_bps as f64 * 60.0 / 8.0 / 1e6
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(w: u32, h: u32, size: OutputSize, fps: u32, codec: CodecChoice) -> VideoPlan {
        plan_video(
            w,
            h,
            &VideoSettings {
                size,
                fps,
                codec,
                quality: Quality::High,
            },
        )
        .unwrap()
    }

    #[test]
    fn rec2_ac1_source_keeps_native_size_and_picks_hevc() {
        let p = plan(3024, 1964, OutputSize::Source, 30, CodecChoice::Auto);
        assert_eq!((p.width, p.height, p.codec), (3024, 1964, Codec::Hevc));
    }

    #[test]
    fn rec2_ac2_5k_is_hevc_and_h264_is_refused() {
        let p = plan(5120, 2880, OutputSize::Source, 30, CodecChoice::Auto);
        assert_eq!((p.width, p.height, p.codec), (5120, 2880, Codec::Hevc));
        let e = plan_video(
            5120,
            2880,
            &VideoSettings {
                codec: CodecChoice::H264,
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(e.to_string().contains("4096"));
        // At exactly 4096 wide H.264 is still allowed.
        assert_eq!(resolve_codec(CodecChoice::H264, 4096).unwrap(), Codec::H264);
    }

    #[test]
    fn rec2_ac3_1080p_on_3024x1964_is_1662x1080() {
        let p = plan(3024, 1964, OutputSize::P1080, 30, CodecChoice::Auto);
        assert_eq!((p.width, p.height), (1662, 1080));
        // Width <= 1920 so Auto picks H.264.
        assert_eq!(p.codec, Codec::H264);
    }

    #[test]
    fn rec2_never_upscales_and_stays_even() {
        let p = plan(1920, 1080, OutputSize::P2160, 30, CodecChoice::Auto);
        assert_eq!((p.width, p.height), (1920, 1080));
        // Odd region sizes are cut to even.
        assert_eq!(
            output_dimensions(1281, 721, OutputSize::Source).unwrap(),
            (1280, 720)
        );
        assert!(output_dimensions(1, 1, OutputSize::Source).is_err());
        for h in [720, 1080, 1440, 2160] {
            let (w, hh) = output_dimensions(3456, 2234, size_for(h)).unwrap();
            assert_eq!(hh, h);
            assert_eq!(w % 2, 0);
        }
    }

    fn size_for(h: u32) -> OutputSize {
        match h {
            720 => OutputSize::P720,
            1080 => OutputSize::P1080,
            1440 => OutputSize::P1440,
            _ => OutputSize::P2160,
        }
    }

    #[test]
    fn rec2_auto_codec_threshold_is_width_above_1920() {
        assert_eq!(resolve_codec(CodecChoice::Auto, 1920).unwrap(), Codec::H264);
        assert_eq!(resolve_codec(CodecChoice::Auto, 1922).unwrap(), Codec::Hevc);
        assert_eq!(resolve_codec(CodecChoice::Hevc, 1280).unwrap(), Codec::Hevc);
    }

    #[test]
    fn rec2_bitrates_match_the_spec_table_exactly() {
        use Codec::*;
        let rows = [
            ((3840, 2160), [60, 90, 40, 60]),
            ((2560, 1440), [30, 45, 20, 30]),
            ((1920, 1080), [16, 24, 10, 16]),
            ((1280, 720), [8, 12, 5, 8]),
        ];
        for ((w, h), want) in rows {
            let got = [
                bitrate_bps(H264, 30, Quality::High, w, h),
                bitrate_bps(H264, 60, Quality::High, w, h),
                bitrate_bps(Hevc, 30, Quality::High, w, h),
                bitrate_bps(Hevc, 60, Quality::High, w, h),
            ];
            let want = want.map(|m| m * 1_000_000);
            assert_eq!(got, want, "{w}x{h}");
        }
    }

    #[test]
    fn rec2_ac4_disk_estimate_for_2160p30_hevc_is_300_mb_per_minute() {
        let p = plan(3840, 2160, OutputSize::Source, 30, CodecChoice::Auto);
        assert_eq!(p.codec, Codec::Hevc);
        assert!((p.disk_mb_per_minute() - 300.0).abs() < 0.5);
        let p60 = plan(3840, 2160, OutputSize::Source, 60, CodecChoice::Auto);
        assert!((p60.disk_mb_per_minute() - 450.0).abs() < 0.5);
    }

    #[test]
    fn bitrate_is_monotonic_in_pixels_and_scales_with_quality() {
        let mut last = 0;
        for (w, h) in [
            (640, 360),
            (1280, 720),
            (1662, 1080),
            (3024, 1964),
            (5120, 2880),
        ] {
            let b = bitrate_bps(Codec::Hevc, 30, Quality::High, w, h);
            assert!(b > last, "{w}x{h}");
            last = b;
        }
        let hi = bitrate_bps(Codec::Hevc, 30, Quality::High, 1920, 1080);
        let bal = bitrate_bps(Codec::Hevc, 30, Quality::Balanced, 1920, 1080);
        let small = bitrate_bps(Codec::Hevc, 30, Quality::Small, 1920, 1080);
        assert!(hi > bal && bal > small);
    }

    #[test]
    fn rec3_only_30_and_60_fps_are_accepted() {
        for fps in [0, 24, 25, 120] {
            let e = plan_video(
                1920,
                1080,
                &VideoSettings {
                    fps,
                    ..Default::default()
                },
            );
            assert!(e.is_err(), "{fps}");
        }
        assert!(plan_video(
            1920,
            1080,
            &VideoSettings {
                fps: 60,
                ..Default::default()
            }
        )
        .is_ok());
    }

    #[test]
    fn settings_serialize_with_spec_names() {
        let json = serde_json::to_string(&VideoSettings {
            size: OutputSize::P1440,
            fps: 60,
            codec: CodecChoice::Hevc,
            quality: Quality::Balanced,
        })
        .unwrap();
        assert!(json.contains("\"1440p\""));
        assert!(json.contains("\"hevc\""));
        let back: VideoSettings = serde_json::from_str("{\"size\":\"2160p\"}").unwrap();
        assert_eq!(back.size, OutputSize::P2160);
        assert_eq!(back.fps, 30);
    }
}
