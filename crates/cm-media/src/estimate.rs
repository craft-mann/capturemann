//! Export size choices and the file-size estimate (EXP-5, release 0.3.1 section 3).
//!
//! * **Size choices** ([`SizeChoice`], [`frame_for`], [`size_options`]): the recording's own size
//!   plus fixed height steps 1440, 1080, 720 and 480, never above the recording; the frame keeps
//!   the recording's shape and has even sides. A step equal to the recording's height is the
//!   recording size. No custom size.
//! * **The estimate** ([`estimate`]): an **upper bound** ("Up to 290 MB"), a formula only, no probe
//!   of the files: the length after cuts and In/Out, the chosen frame, the bit-rate ceiling the
//!   exporter sets for the codec (H.264: the rate it asks the encoder for; VP9: the cap it gives
//!   `libvpx` next to the CRF), the audio bit rate and a measured margin, rounded **up** to two
//!   significant figures. A quiet screen lands far under the bound; that is the accepted trade-off.
//! * **The typical size** ([`typical_video_bps`], `typical_bytes` of [`SizeEstimate`]): a best
//!   guess ("About 14 MB") next to the bound, from one cheap measure of the picture's activity
//!   that needs no decoding: the master screen track's bytes per second ([`MasterRate`], file size
//!   over length), scaled to the chosen frame and the export codec with constants calibrated on
//!   real recordings (see the constants' comments). Never above the bound; absent when there is no
//!   screen master to measure.
//! * **Refusals** with one text for the app, the CLI and MCP (`gif_too_long`,
//!   `size_larger_than_source`; `gif_cannot_fit` is raised by the exporter): the ids and the
//!   wording are templates in `cm_core::message`.
//!
//! The bit-rate tables in this file mirror what the exporter really asks FFmpeg for. If the code
//! they mirror changes, the stored comparison test (`tests/estimate_vs_real.rs`,
//! `verification/benchmarks/export-estimate-*.md`) must be run again.

use std::path::Path;

use cm_core::{CmError, TICKS_PER_SECOND};
use serde_json::Value;

use crate::edit_plan::{secs_of, with_pauses_cut};
use crate::edited::resolve_edits;
use crate::graph::{output_size, videotoolbox_bitrate, AUDIO_BPS_MONO, AUDIO_BPS_STEREO};
use crate::spec::QuickSpec;
use crate::tools::{encoder_candidates_for, probe_caps, Encoder, EncoderPolicy, Tools};

/// The fixed height steps of the size list, largest first.
pub const SIZE_STEPS: [u32; 4] = [1440, 1080, 720, 480];
/// The GIF widths of the chooser.
pub const GIF_WIDTHS: [u32; 4] = [480, 640, 800, 1024];
/// A GIF is at most this long (seconds), as the `gif` preset says.
pub const GIF_MAX_SECONDS: f64 = 30.0;
/// What the real file may exceed the nominal size (bit rate times length) by: container
/// overhead and the encoders' overshoot of their rate request, measured by
/// `tests/estimate_vs_real.rs` (the worst sample is in `verification/benchmarks/export-estimate-*.md`).
pub const BOUND_MARGIN: f64 = 1.05;
/// Below this many bytes (10^6) the label reads "Under 5 MB".
pub const UNDER_BYTES: f64 = 5_000_000.0;
/// WebM audio is Opus at these rates (`edited::OPUS_BPS_*`).
const OPUS_BPS_MONO: u32 = 64_000;
const OPUS_BPS_STEREO: u32 = 96_000;
/// The WebM preset's frame rate (`presets.toml`), capped at the recording's.
const WEBM_FPS: f64 = 30.0;

/// The output formats of the chooser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Mp4,
    WebM,
    Gif,
}

impl Format {
    pub fn parse(s: &str) -> Option<Format> {
        match s {
            "mp4" => Some(Format::Mp4),
            "webm" => Some(Format::WebM),
            "gif" => Some(Format::Gif),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Format::Mp4 => "mp4",
            Format::WebM => "webm",
            Format::Gif => "gif",
        }
    }
}

/// A size of the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizeChoice {
    /// The recording's own size (capped for H.264 level 5.2).
    Source,
    /// A step: this many pixels tall, one of [`SIZE_STEPS`].
    Height(u32),
}

impl SizeChoice {
    /// `source`, or a step as `720`, `720p`.
    pub fn parse(s: &str) -> Result<SizeChoice, CmError> {
        let t = s.trim().to_ascii_lowercase();
        if t == "source" {
            return Ok(SizeChoice::Source);
        }
        let n = t
            .strip_suffix('p')
            .unwrap_or(&t)
            .parse::<u32>()
            .ok()
            .filter(|n| SIZE_STEPS.contains(n));
        n.map(SizeChoice::Height).ok_or_else(|| {
            CmError::invalid_field("size", "must be `source`, 1440, 1080, 720 or 480")
        })
    }

    /// `source` or the step as a number; the form kept in a job's inputs and the MCP results.
    pub fn to_json(self) -> Value {
        match self {
            SizeChoice::Source => Value::String("source".to_owned()),
            SizeChoice::Height(h) => Value::from(h),
        }
    }

    /// Reads [`SizeChoice::to_json`] (a number or a string such as `"720p"`).
    pub fn from_json(v: &Value) -> Option<SizeChoice> {
        match v {
            Value::Number(n) => n
                .as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .filter(|n| SIZE_STEPS.contains(n))
                .map(SizeChoice::Height),
            Value::String(s) => SizeChoice::parse(s).ok(),
            _ => None,
        }
    }
}

fn even_nearest(x: f64) -> u32 {
    let n = (x / 2.0).round().max(1.0) as u32;
    n * 2
}

/// The frame of a size choice for a recording of `master` pixels: the recording size is
/// [`output_size`] (even, capped for H.264); a step keeps the shape. A step taller than the
/// recording is refused (`size_larger_than_source`); one equal to it is the recording size.
pub fn frame_for(choice: SizeChoice, master: (u32, u32)) -> Result<(u32, u32), CmError> {
    let src = output_size(master);
    let SizeChoice::Height(h) = choice else {
        return Ok(src);
    };
    if h > src.1 {
        return Err(larger_than_source(&format!("{h}p"), src));
    }
    if h == src.1 {
        return Ok(src);
    }
    let w = even_nearest(f64::from(h) * f64::from(src.0) / f64::from(src.1)).min(src.0);
    Ok((w, h))
}

/// `size_larger_than_source`.
pub fn larger_than_source(what: &str, src: (u32, u32)) -> CmError {
    CmError::invalid_input(format!(
        "{what} is larger than the recording ({} × {}). Choose the recording size or a smaller one.",
        src.0, src.1
    ))
}

/// One row of the size list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SizeOption {
    pub choice: SizeChoice,
    pub width: u32,
    pub height: u32,
}

/// The sizes to list for a recording of `master` pixels: the recording size first, then every
/// step smaller than it.
pub fn size_options(master: (u32, u32)) -> Vec<SizeOption> {
    let src = output_size(master);
    let mut v = vec![SizeOption {
        choice: SizeChoice::Source,
        width: src.0,
        height: src.1,
    }];
    for h in SIZE_STEPS {
        if h < src.1 {
            if let Ok((width, height)) = frame_for(SizeChoice::Height(h), master) {
                v.push(SizeOption {
                    choice: SizeChoice::Height(h),
                    width,
                    height,
                });
            }
        }
    }
    v
}

/// The GIF widths to list: those not above the recording's width.
pub fn gif_width_options(master: (u32, u32)) -> Vec<u32> {
    let src = output_size(master);
    let v: Vec<u32> = GIF_WIDTHS.iter().copied().filter(|w| *w <= src.0).collect();
    if v.is_empty() {
        vec![GIF_WIDTHS[0]]
    } else {
        v
    }
}

/// `3:12` (minutes and seconds, rounded to the second); `1:02:03` from an hour on.
pub fn clock_text(seconds: f64) -> String {
    let s = seconds.max(0.0).round() as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s % 3600 / 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

/// `gif_too_long`: "GIF is for clips of 30 s or less. This one is 3:12." The hint
/// (`cm_core::message::GIF_TOO_LONG_HINT`) is a separate sentence.
pub fn gif_too_long(max_s: f64, duration_s: f64) -> CmError {
    CmError::invalid_input(format!(
        "GIF is for clips of {} s or less. This one is {}.",
        max_s.round() as u64,
        clock_text(duration_s)
    ))
}

/// `gif_cannot_fit`: "Can't fit under 10 MB at 480 px and 10 fps."
pub fn gif_cannot_fit(max_bytes: u64, width: u32, fps: f64) -> CmError {
    let mb = max_bytes as f64 / 1e6;
    let mb = if (mb - mb.round()).abs() < 1e-9 {
        format!("{}", mb.round() as u64)
    } else {
        format!("{mb:.1}")
    };
    let fps = if (fps - fps.round()).abs() < 1e-9 {
        format!("{}", fps.round() as u64)
    } else {
        format!("{fps:.1}")
    };
    CmError::invalid_input(format!(
        "Can't fit under {mb} MB at {width} px and {fps} fps."
    ))
}

// ---- the bit-rate table --------------------------------------------------------------------

/// The video codec of an MP4 on this computer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Codec {
    H264,
    Vp9,
}

impl Codec {
    pub fn as_str(self) -> &'static str {
        match self {
            Codec::H264 => "h264",
            Codec::Vp9 => "vp9",
        }
    }

    /// `vp9` is VP9; anything else (including `None`) is H.264.
    pub fn parse_or_h264(s: Option<&str>) -> Codec {
        if s == Some("vp9") {
            Codec::Vp9
        } else {
            Codec::H264
        }
    }
}

/// What an MP4 is on this computer: VP9 on Linux without a hardware H.264 encoder (the exporter's
/// own choice, `encoder_candidates_for` under the host policy), H.264 elsewhere. Only Linux needs
/// FFmpeg to be probed; elsewhere `tools` may be `None`.
pub fn mp4_codec(tools: Option<&Tools>) -> Result<Codec, CmError> {
    let policy = EncoderPolicy::host();
    if !policy.linux_set {
        return Ok(Codec::H264);
    }
    let tools = tools.ok_or_else(|| CmError::not_found(crate::tools::not_found_message()))?;
    mp4_codec_for(&probe_caps(tools)?, policy)
}

/// [`mp4_codec`] for known FFmpeg capabilities and policy (pure, tested without a Linux machine):
/// the first encoder the exporter would try decides.
pub fn mp4_codec_for(caps: &crate::tools::Caps, policy: EncoderPolicy) -> Result<Codec, CmError> {
    mp4_plan_for(caps, policy).map(|p| p.codec)
}

/// How the MP4 video is encoded on this computer: the codec and whether the rate is held by a
/// cap (every encoder but one) or only by a constant quality (VA-API H.264 on a driver without a
/// bit-rate mode, `Caps::vaapi_cqp`, where the exporter cannot give the encoder a rate).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mp4Plan {
    pub codec: Codec,
    /// Constant quality, no rate cap: the bit-rate table is no ceiling, see [`CQP_WORST_CASE`].
    pub cqp: bool,
}

impl From<Codec> for Mp4Plan {
    /// A codec with a rate cap (what every platform but VA-API CQP has).
    fn from(codec: Codec) -> Self {
        Mp4Plan { codec, cqp: false }
    }
}

/// [`mp4_codec`] with the constant-quality fact; FFmpeg is probed only on Linux.
pub fn mp4_plan(tools: Option<&Tools>) -> Result<Mp4Plan, CmError> {
    let policy = EncoderPolicy::host();
    if !policy.linux_set {
        return Ok(Codec::H264.into());
    }
    let tools = tools.ok_or_else(|| CmError::not_found(crate::tools::not_found_message()))?;
    mp4_plan_for(&probe_caps(tools)?, policy)
}

/// [`mp4_plan`] for known capabilities and policy (pure): the first encoder the exporter would
/// try decides, and `cqp` is set when that is VA-API with the constant-quality fallback (the same
/// `caps.vaapi_cqp` that `graph::video_codec_for` is given).
pub fn mp4_plan_for(caps: &crate::tools::Caps, policy: EncoderPolicy) -> Result<Mp4Plan, CmError> {
    let first = encoder_candidates_for(caps, None, policy)?.first().copied();
    Ok(Mp4Plan {
        codec: if first == Some(Encoder::Vp9Mp4) {
            Codec::Vp9
        } else {
            Codec::H264
        },
        cqp: first == Some(Encoder::Vaapi) && caps.vaapi_cqp,
    })
}

/// The video bit-rate **ceiling** of `codec` for a `frame` at `fps`: what the exporter asks of
/// the encoder and never lets it exceed.
///
/// H.264 is `graph::videotoolbox_bitrate` (12 Mbps at 1080p30, scaled by area and by fps above
/// 30), the `-b:v` of the hardware encoders; VP9 is `graph::vp9_cap_bitrate` (6 Mbps at 1080p30,
/// the `-b:v` given to `libvpx-vp9` next to `-crf 32`, `edited::OutFormat::vp9` and
/// `graph::video_codec_for`). Both functions are the exporter's own, so the bound follows the
/// code.
pub fn video_bps(codec: Codec, frame: (u32, u32), fps: f64) -> f64 {
    match codec {
        Codec::H264 => videotoolbox_bitrate(frame, fps) as f64,
        Codec::Vp9 => crate::graph::vp9_cap_bitrate(frame, fps) as f64,
    }
}

/// The factor by which constant-quality (CQP) H.264 can exceed the bit-rate table, applied to the
/// video part of the bound when [`Mp4Plan::cqp`]. With `-rc_mode CQP -qp 23` and no `-b:v`
/// (`graph::VAAPI_CQP_QP`) the file follows the picture, not a rate: the Intel UHD 620 run of
/// Nov 10, 2026 (`verification/L1-linux-0.3.1-estimate-vs-real-x11-uhd620-vaapi-2026-11-10-claude.md`)
/// had 3 of 18 samples over the bound, all high-motion MP4, worst real / bound 3.6 (1080p: 117.97
/// MB against 33). 4.0 is that worst ratio rounded up with a margin; a noisier picture than the
/// test's synthetic motion could still pass it, which is the limit of any formula without
/// probing the files. Not used on any other encoder.
pub const CQP_WORST_CASE: f64 = 4.0;

/// The audio bit rate: AAC 128 kbps mono or 192 kbps stereo in MP4, Opus 64 or 96 kbps in WebM
/// (`edited::export_edited`), `0` without audio.
pub fn audio_bps(format: Format, channels: Option<u32>) -> f64 {
    let Some(ch) = channels else { return 0.0 };
    f64::from(match (format, ch >= 2) {
        (Format::WebM, true) => OPUS_BPS_STEREO,
        (Format::WebM, false) => OPUS_BPS_MONO,
        (_, true) => AUDIO_BPS_STEREO,
        (_, false) => AUDIO_BPS_MONO,
    })
}

// ---- the typical size ------------------------------------------------------------------------
//
// One rule, few constants, calibrated on real screen recordings of one user on one Mac (Apple
// M4 Pro, hardware H.264 through VideoToolbox, the FFmpeg that ships), Nov 9, 2026: 10 captures
// (8 recorded by CaptureMann, HEVC masters at the recorder's high quality, 1964p to 4K, 5 with a
// camera bubble; 2 imported H.264 files of 1080p and 1440p), 12 one-minute slices, 36 exports
// (WebM 480p and 1080p, MP4 at the recording size). Leave-one-recording-out: median real /
// typical 1.03, 86 % within a factor 1.5, worst 2.4 and 0.39. The table, the fit and the limits:
// `verification/benchmarks/export-typical-estimate-2026-11-09.md`. Not calibrated: Windows and
// Linux encoders, recorder masters at other quality settings or in H.264, 60 fps, 720p and 480p
// MP4, imports other than two H.264 files.

/// The export codec's typical video rate as a fraction of the master's rate at the master's own
/// frame: `typical = TYPICAL_K[codec] * budget * master_bps * (export area / master area) ^
/// TYPICAL_AREA_EXP`, where `budget` is 1 for the recorder's own master and
/// [`TYPICAL_IMPORT_FACTOR`] for any other file.
pub const TYPICAL_K_H264: f64 = 2.0;
pub const TYPICAL_K_VP9: f64 = 0.30;
/// How the typical rate follows the frame area: a little under 1, because a smaller picture keeps
/// more of its detail per pixel.
pub const TYPICAL_AREA_EXP: f64 = 0.9;
/// An imported file (no `quality` on its screen track) spends far fewer bytes on the same picture
/// than the recorder's master, which asks its encoder for 30 to 40 Mbps and gets 2 to 18: its rate
/// counts this many times. Measured on two imported H.264 files; a weak constant, see the report.
pub const TYPICAL_IMPORT_FACTOR: f64 = 3.8;
/// The typical video rate never drops under this many bits per pixel per frame (a flat screen
/// still costs something: the synthetic static sample of the bound test measured 0.004 to 0.02).
/// 0.003 at 1080p30 is 190 kbps.
pub const TYPICAL_FLOOR_BPP: f64 = 0.003;
/// What the real file adds to the video and audio rates: the container and the key frames.
pub const TYPICAL_MARGIN: f64 = 1.02;

/// The master screen track's size and length: the cheap activity measure of the typical estimate.
/// Read from the capture bundle by [`master_rate`]; tests build it by hand.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MasterRate {
    /// The screen master file's size in bytes.
    pub bytes: u64,
    /// The length the file covers (file time, without the pauses), in seconds.
    pub duration_s: f64,
    /// The master's frame in pixels.
    pub frame: (u32, u32),
    pub fps: f64,
    /// The recorder's own master (its screen track carries the recorder's `quality`), as against
    /// an imported file.
    pub recorded: bool,
}

impl MasterRate {
    /// The master's bits per second.
    pub fn bps(&self) -> f64 {
        self.bytes as f64 * 8.0 / self.duration_s.max(0.001)
    }

    /// The master's rate as a measure of the picture's activity: an imported file's bytes count
    /// [`TYPICAL_IMPORT_FACTOR`] times.
    pub fn activity_bps(&self) -> f64 {
        self.bps()
            * if self.recorded {
                1.0
            } else {
                TYPICAL_IMPORT_FACTOR
            }
    }
}

/// The master screen track's [`MasterRate`] from the bundle folder: the screen file's size and the
/// track's length (end minus start, minus the pauses inside it); `recorded` is whether the
/// manifest's screen track carries the recorder's `quality`. `None` when the recording has no
/// screen track, the file is missing or empty, or the length is not positive: the caller then
/// shows the bound alone. Never fails.
pub fn master_rate(bundle_dir: &Path, manifest: &Value, spec: &QuickSpec) -> Option<MasterRate> {
    let s = &spec.screen;
    if s.file.is_empty() {
        return None;
    }
    let bytes = std::fs::metadata(bundle_dir.join(&s.file)).ok()?.len();
    if bytes == 0 {
        return None;
    }
    let paused: i64 = spec
        .gaps
        .iter()
        .map(|&(from, to)| to.min(s.end).saturating_sub(from.max(s.start)).max(0))
        .sum();
    let ticks = (s.end - s.start - paused).max(0);
    let duration_s = ticks as f64 / TICKS_PER_SECOND as f64;
    if duration_s <= 0.0 || !duration_s.is_finite() {
        return None;
    }
    let recorded = manifest
        .pointer("/tracks/screen/quality")
        .is_some_and(Value::is_string);
    Some(MasterRate {
        bytes,
        duration_s,
        frame: s.size,
        fps: s.fps,
        recorded,
    })
}

/// The video bit rate a file of `codec` at `frame` typically has for a recording whose master is
/// `master`: the rule above, clamped between the floor and the codec's ceiling
/// ([`video_bps`]). The export's `fps` only sets the floor and the ceiling: the exporter keeps the
/// recording's frame rate (WebM at most 30), so the master's rate already counts the frames.
pub fn typical_video_bps(codec: Codec, frame: (u32, u32), fps: f64, master: &MasterRate) -> f64 {
    let area = f64::from(frame.0) * f64::from(frame.1);
    let master_area = (f64::from(master.frame.0) * f64::from(master.frame.1)).max(1.0);
    let k = match codec {
        Codec::H264 => TYPICAL_K_H264,
        Codec::Vp9 => TYPICAL_K_VP9,
    };
    let raw = k * master.activity_bps() * (area / master_area).powf(TYPICAL_AREA_EXP);
    let floor = TYPICAL_FLOOR_BPP * area * fps.max(1.0);
    raw.max(floor).min(video_bps(codec, frame, fps))
}

/// The typical size in bytes before the rounding: the typical video rate plus the audio, times
/// the length, times [`TYPICAL_MARGIN`].
pub fn typical_bytes_of(i: &Inputs, master: &MasterRate) -> f64 {
    let bps =
        typical_video_bps(i.codec, i.frame, i.fps, master) + audio_bps(i.format, i.audio_channels);
    bps * i.duration_s.max(0.0) / 8.0 * TYPICAL_MARGIN
}

/// The typical size as shown: rounded to the nearest two significant figures and never above
/// `max_bytes`.
pub fn typical_of(nominal: f64, max_bytes: u64) -> u64 {
    (two_sf_nearest(nominal).round() as u64).min(max_bytes)
}

/// The English label of the typical size: "About 14 MB", "About 1.2 GB", "Under 5 MB" at 5 MB and
/// below. The app builds the same sentence in its language from `typical_bytes`.
pub fn typical_label_of(typical_bytes: u64) -> String {
    if typical_bytes as f64 <= UNDER_BYTES {
        return "Under 5 MB".to_owned();
    }
    let (div, unit) = if typical_bytes >= 1_000_000_000 {
        (1e9, "GB")
    } else {
        (1e6, "MB")
    };
    format!("About {} {unit}", number_text(typical_bytes as f64 / div))
}

// ---- the estimate --------------------------------------------------------------------------

/// What the formula needs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Inputs {
    pub codec: Codec,
    pub format: Format,
    pub frame: (u32, u32),
    pub fps: f64,
    pub duration_s: f64,
    /// Output audio channels; `None` without audio.
    pub audio_channels: Option<u32>,
    /// The MP4 video is constant quality without a rate cap ([`Mp4Plan::cqp`]).
    pub cqp: bool,
}

/// The nominal size in bytes (bit rate times length), before the margin and the rounding. The
/// video rate of a constant-quality encode counts [`CQP_WORST_CASE`] times.
pub fn estimate_bytes(i: &Inputs) -> f64 {
    let scale = if i.cqp { CQP_WORST_CASE } else { 1.0 };
    let bps = video_bps(i.codec, i.frame, i.fps) * scale + audio_bps(i.format, i.audio_channels);
    bps * i.duration_s.max(0.0) / 8.0
}

/// Rounds `x` to two significant figures, down or up.
fn two_sf(x: f64, up: bool) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    let mag = 10f64.powi(x.log10().floor() as i32 - 1);
    let q = x / mag;
    // Guard against 1e-15 noise turning 24.0 into 25.
    let r = if up {
        (q - 1e-9).ceil()
    } else {
        (q + 1e-9).floor()
    };
    r * mag
}

/// Rounds `x` to the nearest two significant figures.
fn two_sf_nearest(x: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    let mag = 10f64.powi(x.log10().floor() as i32 - 1);
    (x / mag).round() * mag
}

/// The upper bound of an estimate: the nominal size times [`BOUND_MARGIN`], rounded **up** to
/// two significant figures, in bytes.
pub fn bound_of(nominal: f64) -> u64 {
    two_sf(nominal * BOUND_MARGIN, true).round() as u64
}

fn number_text(v: f64) -> String {
    if v < 10.0 {
        format!("{v:.1}")
    } else {
        format!("{}", v.round() as u64)
    }
}

/// The English label: "Up to 290 MB", "Up to 1.2 GB", "Under 5 MB". The app builds the same
/// sentence in its language from `max_bytes`.
pub fn label_of(max_bytes: u64) -> String {
    if max_bytes as f64 <= UNDER_BYTES {
        return "Under 5 MB".to_owned();
    }
    let (div, unit) = if max_bytes >= 1_000_000_000 {
        (1e9, "GB")
    } else {
        (1e6, "MB")
    };
    format!("Up to {} {unit}", number_text(max_bytes as f64 / div))
}

/// A size estimate of one choice.
#[derive(Debug, Clone, PartialEq)]
pub struct SizeEstimate {
    /// The file is not larger than this (two significant figures, rounded up).
    pub max_bytes: u64,
    /// "Up to 290 MB" or "Under 5 MB".
    pub label: String,
    /// The best guess (nearest two significant figures, never above `max_bytes`), from the master
    /// screen track's bytes per second; `None` when there is no screen master to measure.
    pub typical_bytes: Option<u64>,
    /// "About 14 MB" or "Under 5 MB"; `None` with `typical_bytes`.
    pub typical_label: Option<String>,
    /// `h264` or `vp9`: what the file's video will be (MP4 on a machine without a hardware
    /// H.264 encoder is `vp9`).
    pub codec: Codec,
    /// The video is constant quality without a rate cap (VA-API CQP): the bound is scaled by
    /// [`CQP_WORST_CASE`] and there is no typical size.
    pub constant_quality: bool,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    /// Length after the cuts and In/Out, in seconds.
    pub duration_s: f64,
}

/// The estimate of a choice, or that there is none.
#[derive(Debug, Clone, PartialEq)]
pub enum Estimated {
    Bound(SizeEstimate),
    /// GIF: the size is known only after the export.
    KnownAfterExport {
        /// Length after the cuts and In/Out, in seconds.
        duration_s: f64,
    },
}

/// The upper bound of the formula for `i`, with its label, and the typical size when `master`
/// is known.
pub fn estimate_bound(i: &Inputs, master: Option<&MasterRate>) -> SizeEstimate {
    let max_bytes = bound_of(estimate_bytes(i));
    // The typical rule is calibrated on rate-capped encoders only; for constant quality there is
    // no calibration, so the bound stands alone (the simpler honest choice).
    let typical_bytes = master
        .filter(|_| !i.cqp)
        .map(|m| typical_of(typical_bytes_of(i, m), max_bytes));
    SizeEstimate {
        max_bytes,
        label: label_of(max_bytes),
        typical_bytes,
        typical_label: typical_bytes.map(typical_label_of),
        codec: i.codec,
        constant_quality: i.cqp,
        width: i.frame.0,
        height: i.frame.1,
        fps: i.fps,
        duration_s: i.duration_s,
    }
}

/// A choice in the chooser's terms.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Choice {
    pub format: Format,
    /// MP4 and WebM; `None` is the recording size.
    pub size: Option<SizeChoice>,
    /// GIF only.
    pub width: Option<u32>,
    pub max_mb: Option<f64>,
}

/// Length after the cuts, In/Out and pauses, in seconds.
pub fn duration_after_edits(manifest: &Value, spec: &QuickSpec) -> f64 {
    let resolved = resolve_edits(manifest, spec);
    let list = with_pauses_cut(&resolved.list, &resolved.gaps);
    secs_of(list.timeline().duration())
}

/// Checks the combination of options of a choice (the same refusals for the estimate and the
/// export): `size` is not for GIF, `width` is for GIF only, a step above the recording, a GIF
/// over 30 s.
pub fn check_choice(manifest: &Value, c: &Choice) -> Result<(), CmError> {
    let spec = QuickSpec::from_manifest(manifest)?;
    check_choice_for(manifest, &spec, c).map(|_| ())
}

fn check_choice_for(manifest: &Value, spec: &QuickSpec, c: &Choice) -> Result<f64, CmError> {
    match c.format {
        Format::Gif => {
            if c.size.is_some() {
                return Err(CmError::invalid_field(
                    "size",
                    "GIF is sized by `width` (480, 640, 800 or 1024), not by `size`",
                ));
            }
            if let Some(w) = c.width {
                if !GIF_WIDTHS.contains(&w) {
                    return Err(CmError::invalid_field(
                        "width",
                        "a GIF is 480, 640, 800 or 1024 px wide",
                    ));
                }
            }
        }
        Format::Mp4 | Format::WebM => {
            if c.width.is_some() && c.size.is_some() {
                return Err(CmError::invalid_field(
                    "width",
                    "is for GIF only; choose the frame of an MP4 or WebM with `size`",
                ));
            }
            if c.width.is_some() && c.format == Format::Mp4 {
                return Err(CmError::invalid_field(
                    "width",
                    "is for GIF only; choose the frame of an MP4 with `size`",
                ));
            }
            // The WebM preset's own `width` (EXP-3) keeps working without `size`.
            if let Some(w) = c.width {
                if w < 16 || w % 2 != 0 {
                    return Err(CmError::invalid_field(
                        "width",
                        "must be an even width of at least 16 px",
                    ));
                }
            }
            if c.max_mb.is_some() && c.format == Format::Mp4 {
                return Err(CmError::invalid_field(
                    "max_mb",
                    "applies to GIF and WebM only",
                ));
            }
            if let Some(s) = c.size {
                frame_for(s, spec.screen.size)?;
            }
        }
    }
    let d = duration_after_edits(manifest, spec);
    if c.format == Format::Gif && d > GIF_MAX_SECONDS + 0.5 {
        return Err(gif_too_long(GIF_MAX_SECONDS, d));
    }
    Ok(d)
}

/// The estimate of `choice` for a recording (its manifest as JSON): the bound alone, without a
/// typical size. `mp4_codec` is [`mp4_codec`]. Refuses what [`check_choice`] refuses. GIF, and
/// nothing else, has no bound.
pub fn estimate(
    manifest: &Value,
    c: &Choice,
    mp4_codec: impl Into<Mp4Plan>,
) -> Result<Estimated, CmError> {
    estimate_with_master(manifest, c, mp4_codec, None)
}

/// [`estimate`] with the typical size too, when the master screen track's rate is known
/// ([`master_rate`] of the bundle folder; `None` gives the bound alone).
pub fn estimate_with_master(
    manifest: &Value,
    c: &Choice,
    mp4_codec: impl Into<Mp4Plan>,
    master: Option<&MasterRate>,
) -> Result<Estimated, CmError> {
    let mp4 = mp4_codec.into();
    let spec = QuickSpec::from_manifest(manifest)?;
    let duration_s = check_choice_for(manifest, &spec, c)?;
    if c.format == Format::Gif {
        return Ok(Estimated::KnownAfterExport { duration_s });
    }
    let frame = match (c.format, c.width) {
        // As `preset::resolve` sizes a WebM by width: never enlarged.
        (Format::WebM, Some(w)) => {
            let master = spec.screen.size;
            let w = w.min(crate::preset::even_up(f64::from(master.0)));
            crate::preset::sized_by_width(w, master)
        }
        _ => frame_for(c.size.unwrap_or(SizeChoice::Source), spec.screen.size)?,
    };
    let (codec, fps, cqp) = match c.format {
        Format::WebM => (Codec::Vp9, WEBM_FPS.min(spec.screen.fps.max(1.0)), false),
        _ => (
            mp4.codec,
            spec.screen.fps,
            mp4.cqp && mp4.codec == Codec::H264,
        ),
    };
    let channels = [spec.mic.as_ref(), spec.system.as_ref()]
        .into_iter()
        .flatten()
        .map(|a| a.channels)
        .max()
        .map(|c| c.clamp(1, 2));
    Ok(Estimated::Bound(estimate_bound(
        &Inputs {
            codec,
            format: c.format,
            frame,
            fps,
            duration_s,
            audio_channels: channels,
            cqp,
        },
        master,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_never_exceed_the_recording_and_keep_its_shape() {
        // The design's example: 2880 x 1800.
        let v = size_options((2880, 1800));
        let sizes: Vec<_> = v.iter().map(|o| (o.width, o.height)).collect();
        assert_eq!(
            sizes,
            [
                (2880, 1800),
                (2304, 1440),
                (1728, 1080),
                (1152, 720),
                (768, 480)
            ]
        );
        // A 1080p recording: 1080 merges into the recording size, 1440 is not offered.
        let v = size_options((1920, 1080));
        let sizes: Vec<_> = v.iter().map(|o| (o.width, o.height)).collect();
        assert_eq!(sizes, [(1920, 1080), (1280, 720), (854, 480)]);
        assert_eq!(
            frame_for(SizeChoice::Height(1080), (1920, 1080)).unwrap(),
            (1920, 1080)
        );
        let e = frame_for(SizeChoice::Height(1440), (1920, 1080)).unwrap_err();
        assert_eq!(e.code(), cm_core::ErrorCode::InvalidInput);
        assert_eq!(
            e.to_string(),
            "1440p is larger than the recording (1920 × 1080). Choose the recording size or a smaller one."
        );
        assert_eq!(
            cm_core::message::export_refusal(&e.to_string()).unwrap().id,
            "size_larger_than_source"
        );
        // Odd recordings: even sides.
        for m in [(1366, 768), (1281, 801), (5120, 1440)] {
            for o in size_options(m) {
                assert!(o.width % 2 == 0 && o.height % 2 == 0, "{o:?}");
                assert!(o.width <= m.0.max(2) && o.height <= m.1.max(2));
            }
        }
    }

    #[test]
    fn size_parses_in_the_forms_of_the_cli_and_mcp() {
        assert_eq!(SizeChoice::parse("720p").unwrap(), SizeChoice::Height(720));
        assert_eq!(SizeChoice::parse("720").unwrap(), SizeChoice::Height(720));
        assert_eq!(SizeChoice::parse("Source").unwrap(), SizeChoice::Source);
        for bad in ["900", "720px", "", "gif", "4k"] {
            assert!(SizeChoice::parse(bad).is_err(), "{bad}");
        }
        assert_eq!(
            SizeChoice::from_json(&serde_json::json!(480)),
            Some(SizeChoice::Height(480))
        );
        assert_eq!(
            SizeChoice::from_json(&serde_json::json!("source")),
            Some(SizeChoice::Source)
        );
        assert_eq!(SizeChoice::from_json(&serde_json::json!(500)), None);
    }

    #[test]
    fn refusal_texts_are_the_designs() {
        let e = gif_too_long(30.0, 192.0);
        assert_eq!(
            e.to_string(),
            "GIF is for clips of 30 s or less. This one is 3:12."
        );
        let p = cm_core::message::export_refusal(&e.to_string()).unwrap();
        assert_eq!(
            (p.id, p.get("duration"), p.get("max")),
            ("gif_too_long", "3:12", "30")
        );
        let d = e.data().unwrap();
        assert_eq!(d["reason"], "gif_too_long");
        assert_eq!(d["hint"], cm_core::message::GIF_TOO_LONG_HINT);
        assert_eq!(clock_text(3723.0), "1:02:03");
        let e = gif_cannot_fit(10_000_000, 480, 10.0);
        assert_eq!(e.to_string(), "Can't fit under 10 MB at 480 px and 10 fps.");
        assert_eq!(
            cm_core::message::export_refusal(&e.to_string()).unwrap().id,
            "gif_cannot_fit"
        );
        assert_eq!(
            gif_cannot_fit(2_500_000, 640, 15.0).to_string(),
            "Can't fit under 2.5 MB at 640 px and 15 fps."
        );
    }

    #[test]
    fn the_bound_is_rounded_up_to_two_figures_and_labelled_up_to() {
        // Never down: 243 MB nominal with the 5 % margin is 255.15 MB, shown as 260 MB.
        assert_eq!(bound_of(243e6), 260_000_000);
        assert_eq!(label_of(bound_of(243e6)), "Up to 260 MB");
        // Exact two-figure values stay (the margin applied first).
        assert_eq!(bound_of(200e6 / BOUND_MARGIN), 200_000_000);
        assert_eq!(label_of(1_200_000_000), "Up to 1.2 GB");
        assert_eq!(label_of(7_300_000), "Up to 7.3 MB");
        assert_eq!(label_of(4_800_000), "Under 5 MB");
        assert_eq!(label_of(5_000_000), "Under 5 MB");
        for x in [3e6, 7.7e6, 55e6, 640e6, 2.2e9] {
            let b = bound_of(x);
            assert!(b as f64 >= x * BOUND_MARGIN - 1.0, "{x} {b}");
            assert!(b as f64 <= x * BOUND_MARGIN * 1.1, "two figures: {x} {b}");
        }
    }

    #[test]
    fn mp4_is_vp9_on_linux_without_a_hardware_h264_encoder() {
        use crate::tools::caps_from_output;
        let linux = EncoderPolicy::for_os("linux");
        let both = " V h264_vaapi x\n V libvpx-vp9 y\n A aac z\n";
        let mut c = caps_from_output("ffmpeg version 7.1", both);
        // A build with VA-API but no render node (a VM): VP9.
        assert_eq!(mp4_codec_for(&c, linux).unwrap(), Codec::Vp9);
        c.vaapi_device = Some("/dev/dri/renderD128".into());
        assert_eq!(mp4_codec_for(&c, linux).unwrap(), Codec::H264);
        // A build without h264_vaapi.
        let nova = caps_from_output("ffmpeg version 7.1", " V libvpx-vp9 y\n A aac z\n");
        assert_eq!(mp4_codec_for(&nova, linux).unwrap(), Codec::Vp9);
        // macOS and Windows: the hardware H.264 encoder of the platform.
        let mac = caps_from_output("ffmpeg version 7.1", " V h264_videotoolbox x\n A aac z\n");
        assert_eq!(
            mp4_codec_for(&mac, EncoderPolicy::for_os("macos")).unwrap(),
            Codec::H264
        );
        // The estimate reports the codec and sizes the file with the VP9 rate.
        let i = |codec| Inputs {
            codec,
            format: Format::Mp4,
            frame: (1920, 1080),
            fps: 30.0,
            duration_s: 60.0,
            audio_channels: None,
            cqp: false,
        };
        let (h, v) = (
            estimate_bound(&i(Codec::H264), None),
            estimate_bound(&i(Codec::Vp9), None),
        );
        assert_eq!((h.codec.as_str(), v.codec.as_str()), ("h264", "vp9"));
        assert!(v.max_bytes < h.max_bytes);
    }

    #[test]
    fn h264_is_the_rate_the_hardware_encoder_is_asked_for() {
        // 1080p30: 12 Mbps video + 192 kbps stereo AAC for 60 s.
        let i = Inputs {
            codec: Codec::H264,
            format: Format::Mp4,
            frame: (1920, 1080),
            fps: 30.0,
            duration_s: 60.0,
            audio_channels: Some(2),
            cqp: false,
        };
        let b = estimate_bytes(&i);
        assert!(
            (b - (12_000_000.0 + 192_000.0) * 60.0 / 8.0).abs() < 1.0,
            "{b}"
        );
        // No audio: video only. WebM audio is Opus.
        assert_eq!(audio_bps(Format::Mp4, None), 0.0);
        assert_eq!(audio_bps(Format::WebM, Some(1)), 64_000.0);
        assert_eq!(audio_bps(Format::Mp4, Some(1)), 128_000.0);
    }

    /// An imported master of `kbps` at `frame` for `secs`, as the calibration table lists them.
    fn master(kbps: f64, frame: (u32, u32), secs: f64) -> MasterRate {
        MasterRate {
            bytes: (kbps * 1000.0 * secs / 8.0).round() as u64,
            duration_s: secs,
            frame,
            fps: 30.0,
            recorded: false,
        }
    }

    #[test]
    fn the_typical_size_follows_the_master_rate_and_never_exceeds_the_bound() {
        // The maintainer's capture of Nov 9, 2026 (R1 of the calibration report): 8:00 of
        // 2218 x 1440, an imported H.264 file at 705 kbps, exported as a 480p WebM, came out at
        // 14.2 MB where the bound said 69 MB. The rule says about 11 MB.
        let m = master(705.0, (2218, 1440), 480.0);
        let i = Inputs {
            codec: Codec::Vp9,
            format: Format::WebM,
            frame: (740, 480),
            fps: 30.0,
            duration_s: 480.0,
            audio_channels: Some(2),
            cqp: false,
        };
        let e = estimate_bound(&i, Some(&m));
        let t = e.typical_bytes.unwrap();
        assert!(t <= e.max_bytes);
        assert!(
            (9e6..=20e6).contains(&(t as f64)),
            "about 11 MB expected, got {t} ({})",
            e.typical_label.as_deref().unwrap_or("")
        );
        assert!(e.typical_label.as_deref().unwrap().starts_with("About "));
        // The recorder's own master at the same rate is a quieter picture: a smaller guess.
        let own = MasterRate {
            recorded: true,
            ..m
        };
        let o = estimate_bound(&i, Some(&own));
        assert!(o.typical_bytes.unwrap() < t, "{o:?}");
        // R10 of the report: a recorded HEVC master of 1648 kbps at 3024 x 1964 came out at
        // 92 kbps as a 480p WebM (64 kbps of it mono Opus) and 4538 kbps as an MP4 at the
        // recording size; the rule lands within a factor 1.5 of both.
        let r10 = MasterRate {
            recorded: true,
            ..master(1648.0, (3024, 1964), 24.0)
        };
        let webm = typical_video_bps(Codec::Vp9, (740, 480), 30.0, &r10) + 64_000.0;
        assert!((webm / 92_000.0 - 1.0).abs() < 0.5, "{webm}");
        let mp4 = typical_video_bps(Codec::H264, (3024, 1964), 30.0, &r10) + 128_000.0;
        assert!((mp4 / 4_538_000.0 - 1.0).abs() < 0.5, "{mp4}");
        // Without a master there is only the bound.
        let b = estimate_bound(&i, None);
        assert_eq!((b.typical_bytes, b.typical_label), (None, None));
        assert_eq!(b.max_bytes, e.max_bytes);
        // A busier master is a larger typical size; a bigger frame too.
        let busy = typical_video_bps(
            Codec::Vp9,
            (740, 480),
            30.0,
            &master(5000.0, (2218, 1440), 60.0),
        );
        let quiet = typical_video_bps(
            Codec::Vp9,
            (740, 480),
            30.0,
            &master(705.0, (2218, 1440), 60.0),
        );
        assert!(busy > quiet);
        let large = typical_video_bps(
            Codec::Vp9,
            (1664, 1080),
            30.0,
            &master(705.0, (2218, 1440), 60.0),
        );
        assert!(large > quiet);
        // Clamped: a nearly empty master still costs the floor; a frantic one stops at the ceiling.
        let still = master(1.0, (1920, 1080), 60.0);
        let fl = typical_video_bps(Codec::H264, (1920, 1080), 30.0, &still);
        assert!(
            (fl - TYPICAL_FLOOR_BPP * 1920.0 * 1080.0 * 30.0).abs() < 1.0,
            "{fl}"
        );
        let frantic = master(200_000.0, (1920, 1080), 60.0);
        let ceil = typical_video_bps(Codec::H264, (1920, 1080), 30.0, &frantic);
        assert_eq!(ceil, video_bps(Codec::H264, (1920, 1080), 30.0));
        // The shown value is the nearest two figures, capped at the bound; the labels.
        assert_eq!(typical_of(14_237_000.0, 69_000_000), 14_000_000);
        assert_eq!(typical_of(14_650_000.0, 69_000_000), 15_000_000);
        assert_eq!(typical_of(80_000_000.0, 69_000_000), 69_000_000);
        assert_eq!(typical_label_of(14_000_000), "About 14 MB");
        assert_eq!(typical_label_of(1_200_000_000), "About 1.2 GB");
        assert_eq!(typical_label_of(7_300_000), "About 7.3 MB");
        assert_eq!(typical_label_of(4_900_000), "Under 5 MB");
    }

    #[test]
    fn the_master_rate_is_read_from_the_bundle_or_absent() {
        let dir = tempfile::tempdir().unwrap();
        let mut manifest = serde_json::json!({
            "id": "rec_x", "title": "t", "tick_rate": 120000, "duration": 60 * 120000,
            "tracks": { "screen": { "file": "screen.mp4", "size": [1920, 1080], "fps": 30,
                                    "start": 0, "end": 60 * 120000 } },
            "gaps": [{ "from": 10 * 120000, "to": 20 * 120000 }],
        });
        let mut spec = QuickSpec::from_manifest(&manifest).unwrap();
        // No file: no typical size, no error.
        assert_eq!(master_rate(dir.path(), &manifest, &spec), None);
        // An empty file: the same.
        std::fs::write(dir.path().join("screen.mp4"), b"").unwrap();
        assert_eq!(master_rate(dir.path(), &manifest, &spec), None);
        // 5 MB over 60 s of track time with a 10 s pause inside it: 50 s of file. No `quality`
        // on the track: an imported file.
        std::fs::write(dir.path().join("screen.mp4"), vec![0u8; 5_000_000]).unwrap();
        let m = master_rate(dir.path(), &manifest, &spec).unwrap();
        assert_eq!(m.bytes, 5_000_000);
        assert!((m.duration_s - 50.0).abs() < 1e-6, "{}", m.duration_s);
        assert!((m.bps() - 800_000.0).abs() < 1.0);
        assert_eq!((m.frame, m.fps), ((1920, 1080), 30.0));
        assert!(!m.recorded);
        assert!((m.activity_bps() - 800_000.0 * TYPICAL_IMPORT_FACTOR).abs() < 1.0);
        // The recorder's master carries its quality setting.
        manifest["tracks"]["screen"]["quality"] = serde_json::json!("high");
        let m = master_rate(dir.path(), &manifest, &spec).unwrap();
        assert!(m.recorded);
        assert!((m.activity_bps() - 800_000.0).abs() < 1.0);
        // A track of no length: absent.
        spec.screen.end = spec.screen.start;
        assert_eq!(master_rate(dir.path(), &manifest, &spec), None);
    }

    #[test]
    fn vp9_cap_is_monotonic_and_scales_with_fps() {
        let mut last = 0.0;
        for h in [240u32, 480, 720, 1080, 1440, 2160, 2880] {
            let w = h * 16 / 9;
            let b = video_bps(Codec::Vp9, (w, h), 30.0);
            assert!(b > last, "{h}: {b}");
            last = b;
        }
        let a = video_bps(Codec::Vp9, (1920, 1080), 30.0);
        let b = video_bps(Codec::Vp9, (1920, 1080), 60.0);
        assert!((b / a - 2.0).abs() < 1e-9);
    }

    #[test]
    fn constant_quality_scales_the_video_part_of_the_bound_only() {
        use crate::tools::caps_from_output;
        let linux = EncoderPolicy::for_os("linux");
        let mut c = caps_from_output(
            "ffmpeg version 7.1",
            " V h264_vaapi x\n V libvpx-vp9 y\n A aac z\n",
        );
        c.vaapi_device = Some("/dev/dri/renderD128".into());
        // A driver with a bit-rate mode: capped, as before.
        assert_eq!(
            mp4_plan_for(&c, linux).unwrap(),
            Mp4Plan {
                codec: Codec::H264,
                cqp: false
            }
        );
        // The iHD driver of the UHD 620: constant quality.
        c.vaapi_cqp = true;
        assert_eq!(
            mp4_plan_for(&c, linux).unwrap(),
            Mp4Plan {
                codec: Codec::H264,
                cqp: true
            }
        );
        // No render node: VP9, which has a cap whatever the flag says.
        c.vaapi_device = None;
        assert_eq!(
            mp4_plan_for(&c, linux).unwrap(),
            Mp4Plan {
                codec: Codec::Vp9,
                cqp: false
            }
        );
        // macOS and Windows never set it.
        let mac = caps_from_output("ffmpeg version 7.1", " V h264_videotoolbox x\n A aac z\n");
        assert!(
            !mp4_plan_for(&mac, EncoderPolicy::for_os("macos"))
                .unwrap()
                .cqp
        );
        assert!(!Mp4Plan::from(Codec::H264).cqp);

        // Numbers: 1080p30, 20 s, no audio. Capped: (12 Mbps + 192 kbps) for 20 s is 30.5 MB,
        // times 1.05, rounded up to 33 MB. Constant quality: four times the video part.
        let i = |cqp| Inputs {
            codec: Codec::H264,
            format: Format::Mp4,
            frame: (1920, 1080),
            fps: 30.0,
            duration_s: 20.0,
            audio_channels: Some(2),
            cqp,
        };
        let capped = estimate_bound(&i(false), None);
        let cq = estimate_bound(&i(true), None);
        assert_eq!(capped.max_bytes, 33_000_000); // unchanged: the report's "bound 33"
        assert!(!capped.constant_quality && cq.constant_quality);
        let video = 12_000_000.0 * 20.0 / 8.0;
        let audio = 192_000.0 * 20.0 / 8.0;
        let want = (video * CQP_WORST_CASE + audio) * BOUND_MARGIN;
        assert!(cq.max_bytes as f64 >= want && (cq.max_bytes as f64) < want * 1.1);
        // The three samples that were over the bound on the UHD 620 (real MB, old bound MB, frame):
        // each ratio is under CQP_WORST_CASE, so each is inside the new bound.
        for (real, old, frame) in [
            (117.97, 33.0, (1920, 1080)),
            (22.52, 15.0, (1280, 720)),
            (6.82, 6.8, (854, 480)),
        ] {
            assert!(real / old <= CQP_WORST_CASE, "{frame:?}");
            let e = estimate_bound(&Inputs { frame, ..i(true) }, None);
            assert!(
                e.max_bytes as f64 >= real * 1e6,
                "{frame:?}: {}",
                e.max_bytes
            );
        }
        // The worst ratio of the report is under the constant with a margin.
        const { assert!(CQP_WORST_CASE >= 3.6 * 1.1) };
        // The typical size is not offered for constant quality; WebM and VP9 ignore the flag.
        let m = master(800.0, (1920, 1080), 20.0);
        assert!(estimate_bound(&i(false), Some(&m)).typical_bytes.is_some());
        assert_eq!(estimate_bound(&i(true), Some(&m)).typical_bytes, None);
        assert_eq!(estimate_bound(&i(true), Some(&m)).typical_label, None);
    }
}
