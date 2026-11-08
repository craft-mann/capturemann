//! The editing copy, the decoded sound and the verify step (IMP-2).
//!
//! All of it runs the bundled FFmpeg. Nothing here writes outside the paths it is given, and a
//! cancelled run removes its partial output.

use std::path::Path;
use std::process::Stdio;

use cm_core::CmError;

use super::classify::MediaKind;
use super::probe::{probe, ProbeError, ProbedFile, VideoStream};
use super::{Problem, Reason};
use crate::run::{run_ffmpeg, Progress, RunError};
use crate::tools::{spawn_error, Tools};

/// The sample rate of the decoded sound.
pub const SOUND_RATE: u32 = 48_000;
/// A WAV file cannot hold more than 4 GiB; above this the sound is mixed down to mono.
const WAV_LIMIT_BYTES: f64 = 3.9e9;

/// Why a prepare or decode run did not produce its file.
#[derive(Debug)]
pub enum RunFailure {
    Cancelled,
    /// No usable encoder for the copy: no hardware H.264 or HEVC encoder on macOS and Windows (no
    /// software H.264 encoder ships, NFR-12), and on Linux neither VA-API H.264 nor `libvpx-vp9`
    /// in this FFmpeg (O10).
    NoHardwareEncoder,
    Failed(CmError),
}

impl From<RunError> for RunFailure {
    fn from(e: RunError) -> Self {
        match e {
            RunError::Cancelled => RunFailure::Cancelled,
            RunError::Failed(e) => RunFailure::Failed(e),
        }
    }
}

/// The constant frame rate of an editing copy, as an FFmpeg rational: the source's rate snapped to
/// a standard one when within 0.05% of it (29.97 and 30 stay apart), else rounded, between 15 and 60.
pub fn editing_fps(source: Option<f64>) -> String {
    let Some(f) = source.filter(|f| f.is_finite() && *f > 0.0) else {
        return "30".to_owned();
    };
    for (v, text) in [
        (24000.0 / 1001.0, "24000/1001"),
        (24.0, "24"),
        (25.0, "25"),
        (30000.0 / 1001.0, "30000/1001"),
        (30.0, "30"),
        (50.0, "50"),
        (60000.0 / 1001.0, "60000/1001"),
        (60.0, "60"),
    ] {
        if (f - v).abs() / v < 0.0005 {
            return text.to_owned();
        }
    }
    format!("{}", f.round().clamp(15.0, 60.0))
}

/// Bits per second of the editing copy: 0.15 bits per pixel per frame, between 2 and 80 Mbit/s.
pub fn editing_bitrate(width: u32, height: u32, fps: f64) -> u64 {
    let bps = f64::from(width) * f64::from(height) * fps.clamp(1.0, 120.0) * 0.15;
    bps.clamp(2.0e6, 80.0e6) as u64
}

/// Which family of encoders the editing copy may use. No software H.264 encoder ships (NFR-12), so
/// each family names only encoders that run on the machine's media hardware, except on Linux
/// where software VP9 is the labelled fallback (decision O10, Nov 8, 2026).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncoderFamily {
    /// Apple VideoToolbox (`h264_videotoolbox`, `hevc_videotoolbox`).
    VideoToolbox,
    /// Windows Media Foundation (`h264_mf`), forced onto a hardware transform (`-hw_encoding 1`):
    /// without that option FFmpeg falls back to Microsoft's software encoder when the machine has
    /// no hardware one.
    MediaFoundation,
    /// Linux VA-API (`h264_vaapi`): frames are uploaded to the GPU through a render node.
    Vaapi,
    /// Software VP9 (`libvpx-vp9`, BSD) in an MP4: the Linux fallback when VA-API H.264 is missing
    /// or refuses to start (O10). It is not H.264, and the import says so.
    SoftwareVp9,
}

impl EncoderFamily {
    /// The family of the machine this runs on.
    pub fn host() -> Self {
        Self::for_os(std::env::consts::OS)
    }

    /// The first family tried on `os` (`std::env::consts::OS`). Pure.
    pub fn for_os(os: &str) -> Self {
        match os {
            "windows" => EncoderFamily::MediaFoundation,
            "linux" => EncoderFamily::Vaapi,
            _ => EncoderFamily::VideoToolbox,
        }
    }

    /// The name of the family's main encoder (H.264, or VP9 for the software family).
    fn h264(self) -> &'static str {
        match self {
            EncoderFamily::VideoToolbox => "h264_videotoolbox",
            EncoderFamily::MediaFoundation => "h264_mf",
            EncoderFamily::Vaapi => "h264_vaapi",
            EncoderFamily::SoftwareVp9 => "libvpx-vp9",
        }
    }

    /// `None` when the family has no HEVC encoder that the copy uses.
    fn hevc(self) -> Option<&'static str> {
        match self {
            EncoderFamily::VideoToolbox => Some("hevc_videotoolbox"),
            EncoderFamily::MediaFoundation | EncoderFamily::Vaapi | EncoderFamily::SoftwareVp9 => {
                None
            }
        }
    }

    /// The 8-bit 4:2:0 layout the encoder is fed. Media Foundation takes NV12: Intel's Quick Sync
    /// transform refuses yuv420p with "format negotiation failed".
    fn pixel_format(self) -> &'static str {
        match self {
            EncoderFamily::VideoToolbox | EncoderFamily::SoftwareVp9 => "yuv420p",
            EncoderFamily::MediaFoundation => "nv12",
            // VA-API takes NV12 frames uploaded to the GPU.
            EncoderFamily::Vaapi => "nv12,hwupload",
        }
    }

    /// Arguments that must come with the encoder to keep it on the hardware (or, for VP9, to keep
    /// it fast: the copy is for scrubbing, not for delivery).
    fn extra_args(self) -> &'static [&'static str] {
        match self {
            EncoderFamily::VideoToolbox => &[],
            EncoderFamily::MediaFoundation => &["-hw_encoding", "1"],
            EncoderFamily::Vaapi => &["-profile:v", "high"],
            EncoderFamily::SoftwareVp9 => {
                &["-deadline", "realtime", "-cpu-used", "6", "-row-mt", "1"]
            }
        }
    }

    /// Whether an FFmpeg error message says the hardware encoder refused to start (no media
    /// engine or transform, a size it cannot do). Pure.
    fn refused(self, message: &str) -> bool {
        let m = message.to_lowercase();
        match self {
            EncoderFamily::VideoToolbox => {
                m.contains("videotoolbox") && (m.contains("error") || m.contains("cannot"))
            }
            EncoderFamily::MediaFoundation => {
                (m.contains("_mf") || m.contains("mediafoundation") || m.contains("mft"))
                    && (m.contains("error")
                        || m.contains("cannot")
                        || m.contains("could not")
                        || m.contains("not found")
                        || m.contains("failed"))
            }
            EncoderFamily::Vaapi => {
                ["vaapi", "libva", "hwupload", "/dev/dri", "render node"]
                    .iter()
                    .any(|k| m.contains(k))
                    && [
                        "error",
                        "cannot",
                        "could not",
                        "failed",
                        "not found",
                        "no usable",
                        "no such",
                        "no va display",
                    ]
                    .iter()
                    .any(|k| m.contains(k))
            }
            // Software: it starts, or the input is bad; never "no encoder on this computer".
            EncoderFamily::SoftwareVp9 => false,
        }
    }
}

/// One encoder the editing copy can be made with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CopyEncoder {
    pub family: EncoderFamily,
    /// FFmpeg's encoder name.
    pub name: &'static str,
}

impl CopyEncoder {
    /// `h264`, `hevc` or `vp9`.
    pub fn codec(&self) -> &'static str {
        if self.name.starts_with("hevc") {
            "hevc"
        } else if self.family == EncoderFamily::SoftwareVp9 {
            "vp9"
        } else {
            "h264"
        }
    }

    pub fn is_software(&self) -> bool {
        self.family == EncoderFamily::SoftwareVp9
    }
}

/// The encoders to try for the editing copy, best first. Pure: `listing` is the text of
/// `ffmpeg -encoders`, `vaapi_device` the render node when the computer has one.
///
/// macOS and Windows: the one hardware encoder of the platform (unchanged, NFR-12). Linux (O10):
/// `h264_vaapi` when the build has it and a render node exists, then software VP9 when the build
/// has `libvpx-vp9`. Never `libx264` or OpenH264.
pub fn copy_ladder(
    host: EncoderFamily,
    source_codec: &str,
    listing: &str,
    vaapi_device: Option<&str>,
) -> Vec<CopyEncoder> {
    if host == EncoderFamily::Vaapi {
        let mut out = Vec::new();
        if listing.contains("h264_vaapi") && vaapi_device.is_some() {
            out.push(CopyEncoder {
                family: EncoderFamily::Vaapi,
                name: "h264_vaapi",
            });
        }
        if listing.contains("libvpx-vp9") {
            out.push(CopyEncoder {
                family: EncoderFamily::SoftwareVp9,
                name: "libvpx-vp9",
            });
        }
        return out;
    }
    let (h264, hevc) = parse_hardware_encoders(host, listing);
    pick_encoder(host, source_codec, h264, hevc)
        .map(|name| CopyEncoder { family: host, name })
        .into_iter()
        .collect()
}

/// What the import records about the editing copy it made: which encoder, and why it was this one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditingCopy {
    pub encoder: &'static str,
    /// `h264`, `hevc` or `vp9`.
    pub codec: &'static str,
    pub software: bool,
    /// The sentence the CLI, the MCP result and the import sheet show when the copy is not
    /// hardware H.264 (O10).
    pub note: Option<String>,
}

impl EditingCopy {
    fn of(used: CopyEncoder, hardware_refused: bool) -> EditingCopy {
        let note = used.is_software().then(|| {
            if hardware_refused {
                "The editing copy is software VP9: the hardware H.264 encoder did not start on this computer.".to_owned()
            } else {
                "The editing copy is software VP9: this computer has no hardware H.264 encoder (VA-API).".to_owned()
            }
        });
        EditingCopy {
            encoder: used.name,
            codec: used.codec(),
            software: used.is_software(),
            note,
        }
    }
}

/// The encoders of `family` that appear in the text of `ffmpeg -encoders`: `(h264, hevc)`.
pub fn parse_hardware_encoders(family: EncoderFamily, listing: &str) -> (bool, bool) {
    (
        listing.contains(family.h264()),
        family.hevc().is_some_and(|h| listing.contains(h)),
    )
}

/// `ffmpeg -encoders` scanned for the hardware encoders of this machine: `(h264, hevc)`. A listed
/// Media Foundation encoder can still fail to start when the machine has no hardware transform;
/// [`make_editing_copy`] turns that failure into [`RunFailure::NoHardwareEncoder`].
pub fn hardware_encoders(tools: &Tools) -> Result<(bool, bool), CmError> {
    let out = cm_platform::process::command(&tools.ffmpeg)
        .args(["-hide_banner", "-encoders"])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| spawn_error(&tools.ffmpeg, &e))?;
    let text = String::from_utf8_lossy(&out.stdout);
    let host = EncoderFamily::host();
    if host == EncoderFamily::Vaapi
        && (!text.contains("h264_vaapi") || crate::tools::vaapi_probe(&tools.ffmpeg).is_none())
    {
        return Ok((false, false));
    }
    Ok(parse_hardware_encoders(host, &text))
}

/// The encoders the editing copy can be made with on this computer, best first, and the VA-API
/// render node (see [`copy_ladder`]). Empty: nothing can make the copy
/// ([`RunFailure::NoHardwareEncoder`]).
pub fn editing_copy_encoders(
    tools: &Tools,
    source_codec: &str,
) -> Result<(Vec<CopyEncoder>, Option<String>, bool), CmError> {
    let out = cm_platform::process::command(&tools.ffmpeg)
        .args(["-hide_banner", "-encoders"])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| spawn_error(&tools.ffmpeg, &e))?;
    let text = String::from_utf8_lossy(&out.stdout);
    // A render node counts only when a probe encode works on it (and says whether the driver
    // takes constant quality only).
    let probed = if EncoderFamily::host() == EncoderFamily::Vaapi && text.contains("h264_vaapi") {
        crate::tools::vaapi_probe(&tools.ffmpeg)
    } else {
        None
    };
    let cqp = probed.as_ref().is_some_and(|p| p.1);
    let device = probed.map(|p| p.0);
    let ladder = copy_ladder(
        EncoderFamily::host(),
        source_codec,
        &text,
        device.as_deref(),
    );
    Ok((ladder, device, cqp))
}

/// The encoder for the copy: HEVC for an HEVC source when the build has it, else H.264.
pub fn pick_encoder(
    family: EncoderFamily,
    source_codec: &str,
    h264: bool,
    hevc: bool,
) -> Option<&'static str> {
    let hevc_name = family.hevc().filter(|_| hevc);
    if source_codec == "hevc" && hevc_name.is_some() {
        hevc_name
    } else if h264 {
        Some(family.h264())
    } else {
        hevc_name
    }
}

/// The colour standard of an editing copy made by Media Foundation, which writes no colour
/// information of its own (VideoToolbox tags its output, so the macOS copy is left alone).
/// Standard-definition video is BT.601 (`smpte170m`), high definition BT.709: the guess every
/// player makes for an untagged stream, and the source's own tag when it has one. Anything else a
/// file can carry (BT.2020, ...) is left as it is rather than mislabelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Standard {
    Bt709,
    Smpte170m,
}

impl Standard {
    /// `None`: the source says something this copy does not re-tag.
    pub fn of_source(v: &VideoStream) -> Option<Standard> {
        match v.color_space.as_deref() {
            Some("bt709") => Some(Standard::Bt709),
            Some("smpte170m" | "bt470bg") => Some(Standard::Smpte170m),
            Some(_) => None,
            None if v.height >= 720 || v.width >= 1280 => Some(Standard::Bt709),
            None => Some(Standard::Smpte170m),
        }
    }

    /// FFmpeg's name (matrix, primaries and transfer all carry it).
    fn name(self) -> &'static str {
        match self {
            Standard::Bt709 => "bt709",
            Standard::Smpte170m => "smpte170m",
        }
    }

    /// The H.264 table code for primaries, transfer and matrix.
    fn code(self) -> u8 {
        match self {
            Standard::Bt709 => 1,
            Standard::Smpte170m => 6,
        }
    }
}

/// How the Media Foundation copy is tagged: the standard, and whether this FFmpeg has the
/// `h264_metadata` filter that puts it into the SPS (`Caps::h264_metadata`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CopyColour {
    pub standard: Standard,
    pub vui: bool,
}

/// The FFmpeg arguments of the editing copy. Pure. FFmpeg applies the rotation tag by itself
/// (autorotate), so the copy is upright with no tag.
#[allow(clippy::too_many_arguments)]
pub fn editing_copy_args(
    input: &Path,
    output: &Path,
    video_index: u32,
    family: EncoderFamily,
    encoder: &str,
    fps: &str,
    bitrate: u64,
    colour: Option<CopyColour>,
    vaapi_device: Option<&str>,
    vaapi_cqp: bool,
) -> Vec<String> {
    let rate: f64 = match fps.split_once('/') {
        Some((n, d)) => n.parse::<f64>().unwrap_or(30.0) / d.parse::<f64>().unwrap_or(1.0),
        None => fps.parse().unwrap_or(30.0),
    };
    let mut a: Vec<String> = [
        "-hide_banner",
        "-loglevel",
        "error",
        "-nostdin",
        "-y",
        "-progress",
        "pipe:1",
        "-nostats",
        "-i",
    ]
    .map(String::from)
    .to_vec();
    if family == EncoderFamily::Vaapi {
        // A global option: the render node the `hwupload` filter and the encoder share.
        a.splice(
            1..1,
            [
                "-vaapi_device".to_owned(),
                vaapi_device.unwrap_or("/dev/dri/renderD128").to_owned(),
            ],
        );
    }
    a.push(input.display().to_string());
    a.extend(["-map".to_owned(), format!("0:{video_index}")]);
    a.extend(["-an", "-sn", "-dn"].map(String::from));
    // The scale keeps the matrix the source has (or the one players assume for its size) on both
    // sides, so the pixels are not converted, and makes the range limited.
    let matrix = colour.map_or(String::new(), |c| {
        format!(
            ":in_color_matrix={m}:out_color_matrix={m}:out_range=tv",
            m = c.standard.name()
        )
    });
    a.extend([
        "-vf".to_owned(),
        format!(
            "fps={fps},scale=trunc(iw/2)*2:trunc(ih/2)*2{matrix},format={}",
            family.pixel_format()
        ),
        "-c:v".to_owned(),
        encoder.to_owned(),
    ]);
    if family == EncoderFamily::Vaapi && vaapi_cqp {
        // The driver has no bit-rate mode (Intel iHD): constant quality instead.
        a.extend(["-rc_mode", "CQP", "-qp", "23"].map(String::from));
    } else {
        a.extend(["-b:v".to_owned(), bitrate.to_string()]);
    }
    a.extend([
        "-g".to_owned(),
        format!("{}", rate.round().max(1.0)),
        "-fps_mode".to_owned(),
        "cfr".to_owned(),
    ]);
    a.extend(family.extra_args().iter().map(|x| (*x).to_owned()));
    if encoder.starts_with("hevc") {
        a.extend(["-tag:v", "hvc1"].map(String::from));
    }
    if let Some(c) = colour {
        let n = c.standard.name();
        a.extend(
            [
                "-colorspace",
                n,
                "-color_primaries",
                n,
                "-color_trc",
                n,
                "-color_range",
                "tv",
            ]
            .map(String::from),
        );
        if c.vui {
            a.extend([
                "-bsf:v".to_owned(),
                crate::graph::mf_vui_bsf(c.standard.code()),
            ]);
        }
    }
    // The `colr` atom for Media Foundation, whose stream carries no colour description itself.
    a.extend([
        "-movflags".to_owned(),
        crate::graph::mp4_movflags(colour.is_some(), colour.as_ref().is_some_and(|c| c.vui))
            .to_owned(),
    ]);
    a.push(output.display().to_string());
    a
}

/// Makes the editing copy of a video: constant frame rate, upright, 8-bit 4:2:0, no sound.
/// `progress` gets 0..=1. A cancel removes the partial file. Returns the encoder used: on Linux
/// it is hardware H.264 (VA-API) where that works and software VP9 otherwise (O10), and the
/// caller says which.
pub fn make_editing_copy(
    tools: &Tools,
    source: &ProbedFile,
    output: &Path,
    progress: &mut dyn FnMut(f64),
    cancel: &dyn Fn() -> bool,
) -> Result<EditingCopy, RunFailure> {
    let v = source
        .video
        .as_ref()
        .ok_or_else(|| RunFailure::Failed(CmError::invalid_input("the file has no video")))?;
    let (ladder, device, cqp) =
        editing_copy_encoders(tools, &v.codec).map_err(RunFailure::Failed)?;
    if ladder.is_empty() {
        return Err(RunFailure::NoHardwareEncoder);
    }
    let fps = editing_fps(v.fps);
    let rate = v.fps.unwrap_or(30.0).clamp(15.0, 60.0);
    let mut refused_before = false;
    for (n, enc) in ladder.iter().enumerate() {
        let family = enc.family;
        let colour = (family == EncoderFamily::MediaFoundation)
            .then(|| Standard::of_source(v))
            .flatten()
            .map(|standard| CopyColour {
                standard,
                vui: crate::tools::has_bitstream_filter(tools, "h264_metadata"),
            });
        let args = editing_copy_args(
            &source.path,
            output,
            v.index,
            family,
            enc.name,
            &fps,
            editing_bitrate(v.width, v.height, rate),
            colour,
            device.as_deref(),
            cqp,
        );
        let mut on = |p: Progress| progress(p.fraction);
        let r = run_ffmpeg(&tools.ffmpeg, &args, source.duration_s, &mut on, cancel);
        match r {
            Ok(()) => return Ok(EditingCopy::of(*enc, refused_before)),
            Err(e) => {
                let _ = std::fs::remove_file(output);
                // The encoder refused to start (no media engine, a size it cannot do): the next
                // one is tried, and when there is none the file cannot be prepared here.
                if let RunError::Failed(ce) = &e {
                    if family.refused(&ce.to_string()) {
                        if n + 1 < ladder.len() {
                            refused_before = true;
                            continue;
                        }
                        return Err(RunFailure::NoHardwareEncoder);
                    }
                }
                return Err(e.into());
            }
        }
    }
    Err(RunFailure::NoHardwareEncoder)
}

/// How many channels `mic.wav` gets: the source's, at most 2, and mono when stereo would not fit
/// in a WAV file.
pub fn sound_channels(source_channels: u32, duration_s: f64) -> u32 {
    let ch = source_channels.clamp(1, 2);
    if ch == 2 && duration_s * f64::from(SOUND_RATE) * 4.0 * 2.0 > WAV_LIMIT_BYTES {
        1
    } else {
        ch
    }
}

/// The FFmpeg arguments that decode one audio stream to 48 kHz 32-bit float WAV. The stream is
/// padded with silence at the front when it starts after time zero, so it stays in step with the
/// picture. Pure.
pub fn sound_args(input: &Path, output: &Path, stream_index: u32, channels: u32) -> Vec<String> {
    let mut a: Vec<String> = [
        "-hide_banner",
        "-loglevel",
        "error",
        "-nostdin",
        "-y",
        "-progress",
        "pipe:1",
        "-nostats",
        "-i",
    ]
    .map(String::from)
    .to_vec();
    a.push(input.display().to_string());
    a.extend(["-map".to_owned(), format!("0:{stream_index}")]);
    a.extend(["-vn", "-sn", "-dn"].map(String::from));
    a.extend([
        "-af".to_owned(),
        "aresample=async=1:first_pts=0".to_owned(),
        "-ac".to_owned(),
        channels.to_string(),
        "-ar".to_owned(),
        SOUND_RATE.to_string(),
        "-c:a".to_owned(),
        "pcm_f32le".to_owned(),
        "-f".to_owned(),
        "wav".to_owned(),
    ]);
    a.push(output.display().to_string());
    a
}

/// Decodes the chosen audio stream of `source` to `output` (`mic.wav`). Returns the channel
/// count written.
pub fn decode_sound(
    tools: &Tools,
    source: &ProbedFile,
    output: &Path,
    progress: &mut dyn FnMut(f64),
    cancel: &dyn Fn() -> bool,
) -> Result<u32, RunFailure> {
    let a = source
        .audio
        .as_ref()
        .ok_or_else(|| RunFailure::Failed(CmError::invalid_input("the file has no audio")))?;
    let channels = sound_channels(a.channels, source.duration_s);
    let args = sound_args(&source.path, output, a.index, channels);
    let mut on = |p: Progress| progress(p.fraction);
    match run_ffmpeg(&tools.ffmpeg, &args, source.duration_s, &mut on, cancel) {
        Ok(()) => Ok(channels),
        Err(e) => {
            let _ = std::fs::remove_file(output);
            Err(e.into())
        }
    }
}

/// One frame of `fps`, or 25 ms without a picture: how far two durations may differ.
pub fn duration_tolerance(kind: MediaKind, fps: Option<f64>) -> f64 {
    match (kind, fps) {
        (MediaKind::Video, Some(f)) if f > 0.0 => 1.0 / f,
        (MediaKind::Video, _) => 1.0 / 24.0,
        (MediaKind::Audio, _) => 0.025,
    }
}

fn unreadable(m: impl Into<String>) -> Problem {
    Problem::new(Reason::UnreadableFile, m)
}

/// Decodes `seconds` at the start (`head`) or the end of `file` and reports any decode error.
pub fn decode_check(tools: &Tools, file: &Path, head: bool, seconds: u32) -> Result<(), Problem> {
    let mut cmd = cm_platform::process::command(&tools.ffmpeg);
    cmd.args(["-hide_banner", "-loglevel", "error", "-nostdin"]);
    if head {
        cmd.args(["-t".to_owned(), seconds.to_string()]);
    } else {
        cmd.args(["-sseof".to_owned(), format!("-{seconds}")]);
    }
    cmd.arg("-i")
        .arg(file)
        .args(["-f", "null", "-"])
        .stdin(Stdio::null());
    let out = cmd
        .output()
        .map_err(|e| unreadable(format!("cannot run FFmpeg: {e}")))?;
    let err = String::from_utf8_lossy(&out.stderr);
    if !out.status.success() || !err.trim().is_empty() {
        return Err(unreadable(format!(
            "The copy of `{}` does not decode cleanly: {}",
            file.file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
            err.lines().next().unwrap_or("FFmpeg failed")
        )));
    }
    Ok(())
}

/// What [`verify`] checks.
pub struct VerifyInput<'a> {
    pub tools: &'a Tools,
    pub source: &'a ProbedFile,
    pub kind: MediaKind,
    /// The copy of the file inside the bundle (the master).
    pub master: &'a Path,
    /// `mic.wav`, when the file has sound.
    pub sound: Option<&'a Path>,
    /// The editing copy, when one was made.
    pub editing_copy: Option<&'a Path>,
}

fn probe_problem(e: ProbeError) -> Problem {
    match e {
        ProbeError::Problem(p) => p,
        ProbeError::Permission { message, .. } => unreadable(message),
        ProbeError::Other(e) => unreadable(e.to_string()),
    }
}

/// The verify step (IMP-2): the master has the source's size in bytes and reads back with the
/// source's duration (within one frame; 25 ms for audio), the first and last 2 s of the master and
/// of the editing copy decode without error, `mic.wav` is as long as the source's sound, and the
/// editing copy is as long as the source. Any failure is `unreadable_file`.
pub fn verify(i: &VerifyInput<'_>) -> Result<(), Problem> {
    let tol = duration_tolerance(i.kind, i.source.video.as_ref().and_then(|v| v.fps));
    let size = std::fs::metadata(i.master)
        .map_err(|e| unreadable(format!("The copy cannot be read: {e}")))?
        .len();
    if size != i.source.size_bytes {
        return Err(unreadable(format!(
            "The copy has {size} bytes but the original has {}.",
            i.source.size_bytes
        )));
    }
    let copy = probe(i.tools, i.master).map_err(probe_problem)?;
    if (copy.duration_s - i.source.duration_s).abs() > tol {
        return Err(unreadable(format!(
            "The copy lasts {:.2} s but the original lasts {:.2} s.",
            copy.duration_s, i.source.duration_s
        )));
    }
    decode_check(i.tools, i.master, true, 2)?;
    decode_check(i.tools, i.master, false, 2)?;
    if let Some(ec) = i.editing_copy {
        let p = probe(i.tools, ec).map_err(probe_problem)?;
        if (p.duration_s - i.source.duration_s).abs() > tol.max(0.05) {
            return Err(unreadable(format!(
                "The editing copy lasts {:.2} s but the original lasts {:.2} s.",
                p.duration_s, i.source.duration_s
            )));
        }
        decode_check(i.tools, ec, true, 2)?;
        decode_check(i.tools, ec, false, 2)?;
    }
    if let Some(s) = i.sound {
        let want = i
            .source
            .audio
            .as_ref()
            .and_then(|a| a.duration_s)
            .unwrap_or(i.source.duration_s);
        let got = wav_seconds(s).map_err(unreadable)?;
        if (got - want).abs() > 0.05_f64.max(tol) {
            return Err(unreadable(format!(
                "The decoded sound lasts {got:.2} s but the original's sound lasts {want:.2} s."
            )));
        }
    }
    Ok(())
}

/// Length of a WAV file in seconds, from its chunk headers (no dependency: the `fmt ` and `data`
/// chunks are all that is needed). A `data` size of 0 or `0xFFFFFFFF` (a writer that did not come
/// back to patch it) means "to the end of the file".
pub fn wav_seconds(path: &Path) -> Result<f64, String> {
    use std::io::{Read, Seek, SeekFrom};
    let bad = |m: &str| format!("The decoded sound can't be read: {m}");
    let mut f = std::fs::File::open(path).map_err(|e| bad(&e.to_string()))?;
    let total = f.metadata().map_err(|e| bad(&e.to_string()))?.len();
    let mut head = [0u8; 12];
    f.read_exact(&mut head).map_err(|e| bad(&e.to_string()))?;
    if &head[0..4] != b"RIFF" || &head[8..12] != b"WAVE" {
        return Err(bad("not a WAV file"));
    }
    let (mut rate, mut block) = (0u32, 0u32);
    let mut pos = 12u64;
    loop {
        f.seek(SeekFrom::Start(pos))
            .map_err(|e| bad(&e.to_string()))?;
        let mut ch = [0u8; 8];
        if f.read_exact(&mut ch).is_err() {
            return Err(bad("no data chunk"));
        }
        let size = u64::from(u32::from_le_bytes([ch[4], ch[5], ch[6], ch[7]]));
        match &ch[0..4] {
            b"fmt " => {
                let mut fmt = [0u8; 16];
                f.read_exact(&mut fmt).map_err(|e| bad(&e.to_string()))?;
                rate = u32::from_le_bytes([fmt[4], fmt[5], fmt[6], fmt[7]]);
                block = u32::from(u16::from_le_bytes([fmt[12], fmt[13]]));
            }
            b"data" => {
                if rate == 0 || block == 0 {
                    return Err(bad("data before format"));
                }
                let avail = total.saturating_sub(pos + 8);
                let bytes = if size == 0 || size == 0xFFFF_FFFF {
                    avail
                } else {
                    size.min(avail)
                };
                return Ok(bytes as f64 / f64::from(block) / f64::from(rate));
            }
            _ => {}
        }
        pos += 8 + size + (size & 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_frame_rate_snaps_to_a_standard_one() {
        assert_eq!(editing_fps(Some(29.97)), "30000/1001");
        assert_eq!(editing_fps(Some(30.0)), "30");
        assert_eq!(editing_fps(Some(23.976)), "24000/1001");
        assert_eq!(editing_fps(Some(59.94)), "60000/1001");
        assert_eq!(editing_fps(Some(29.4)), "29");
        assert_eq!(editing_fps(Some(120.0)), "60");
        assert_eq!(editing_fps(Some(5.0)), "15");
        assert_eq!(editing_fps(None), "30");
        assert_eq!(editing_fps(Some(f64::NAN)), "30");
    }

    #[test]
    fn the_bitrate_follows_the_size() {
        assert_eq!(editing_bitrate(1920, 1080, 30.0), 9_331_200);
        assert_eq!(editing_bitrate(320, 240, 30.0), 2_000_000);
        assert_eq!(editing_bitrate(7680, 4320, 60.0), 80_000_000);
    }

    #[test]
    fn the_encoder_follows_the_source_and_what_exists() {
        use EncoderFamily::{MediaFoundation as Mf, VideoToolbox as Vt};
        assert_eq!(
            pick_encoder(Vt, "hevc", true, true),
            Some("hevc_videotoolbox")
        );
        assert_eq!(
            pick_encoder(Vt, "h264", true, true),
            Some("h264_videotoolbox")
        );
        assert_eq!(
            pick_encoder(Vt, "hevc", true, false),
            Some("h264_videotoolbox")
        );
        assert_eq!(
            pick_encoder(Vt, "h264", false, true),
            Some("hevc_videotoolbox")
        );
        assert_eq!(pick_encoder(Vt, "h264", false, false), None);
        // Windows: H.264 for every source; no HEVC encoder is used there.
        assert_eq!(pick_encoder(Mf, "hevc", true, true), Some("h264_mf"));
        assert_eq!(pick_encoder(Mf, "h264", true, false), Some("h264_mf"));
        assert_eq!(pick_encoder(Mf, "hevc", false, true), None);
        assert_eq!(pick_encoder(Mf, "h264", false, false), None);
    }

    #[test]
    fn the_listing_decides_which_family_encoders_exist() {
        let listing = " V....D h264_mf Media Foundation\n V....D h264_videotoolbox VT\n";
        assert_eq!(
            parse_hardware_encoders(EncoderFamily::MediaFoundation, listing),
            (true, false)
        );
        assert_eq!(
            parse_hardware_encoders(EncoderFamily::VideoToolbox, listing),
            (true, false)
        );
        assert_eq!(
            parse_hardware_encoders(EncoderFamily::MediaFoundation, " A aac AAC\n"),
            (false, false)
        );
    }

    #[test]
    fn the_media_foundation_copy_is_forced_onto_hardware_and_never_tagged_hevc() {
        let a = editing_copy_args(
            Path::new("a.mp4"),
            Path::new("b.mp4"),
            0,
            EncoderFamily::MediaFoundation,
            "h264_mf",
            "30",
            5_000_000,
            None,
            None,
            false,
        );
        let s = a.join(" ");
        assert!(s.contains("-c:v h264_mf -b:v 5000000"), "{s}");
        assert!(s.contains("-hw_encoding 1"), "{s}");
        assert!(s.contains("format=nv12"), "{s}");
        assert!(!s.contains("hvc1"), "{s}");
        let v = editing_copy_args(
            Path::new("a"),
            Path::new("b"),
            0,
            EncoderFamily::VideoToolbox,
            "h264_videotoolbox",
            "30",
            1,
            None,
            None,
            false,
        );
        assert!(!v.join(" ").contains("hw_encoding"));
        assert!(v.join(" ").contains("format=yuv420p"));
    }

    fn stream(w: u32, h: u32, space: Option<&str>) -> VideoStream {
        VideoStream {
            index: 0,
            codec: "h264".into(),
            width: w,
            height: h,
            fps: Some(30.0),
            rotation: 0,
            pix_fmt: Some("yuv420p".into()),
            hdr: false,
            color_space: space.map(str::to_owned),
            variable_frame_rate: false,
        }
    }

    #[test]
    fn the_standard_follows_the_source_tag_else_its_size() {
        use Standard::*;
        assert_eq!(
            Standard::of_source(&stream(1920, 1080, Some("bt709"))),
            Some(Bt709)
        );
        assert_eq!(Standard::of_source(&stream(1920, 1080, None)), Some(Bt709));
        assert_eq!(Standard::of_source(&stream(1280, 720, None)), Some(Bt709));
        assert_eq!(
            Standard::of_source(&stream(720, 576, None)),
            Some(Smpte170m)
        );
        assert_eq!(
            Standard::of_source(&stream(320, 240, None)),
            Some(Smpte170m)
        );
        // A source tag wins over the size, and an unusual one is not re-tagged.
        assert_eq!(
            Standard::of_source(&stream(1920, 1080, Some("smpte170m"))),
            Some(Smpte170m)
        );
        assert_eq!(
            Standard::of_source(&stream(720, 576, Some("bt470bg"))),
            Some(Smpte170m)
        );
        assert_eq!(
            Standard::of_source(&stream(720, 576, Some("bt709"))),
            Some(Bt709)
        );
        assert_eq!(
            Standard::of_source(&stream(3840, 2160, Some("bt2020nc"))),
            None
        );
    }

    #[test]
    fn the_media_foundation_copy_keeps_the_matrix_and_is_tagged() {
        let run = |std: Standard, vui: bool| {
            editing_copy_args(
                Path::new("a.mp4"),
                Path::new("b.mp4"),
                0,
                EncoderFamily::MediaFoundation,
                "h264_mf",
                "30",
                5_000_000,
                Some(CopyColour { standard: std, vui }),
                None,
                false,
            )
            .join(" ")
        };
        let hd = run(Standard::Bt709, true);
        assert!(
            hd.contains("scale=trunc(iw/2)*2:trunc(ih/2)*2:in_color_matrix=bt709:out_color_matrix=bt709:out_range=tv,format=nv12"),
            "{hd}"
        );
        assert!(
            hd.contains(
                "-colorspace bt709 -color_primaries bt709 -color_trc bt709 -color_range tv"
            ),
            "{hd}"
        );
        assert!(hd.contains("-bsf:v h264_metadata=colour_primaries=1:transfer_characteristics=1:matrix_coefficients=1:video_full_range_flag=0"), "{hd}");
        assert!(
            hd.contains("-movflags +faststart") && !hd.contains("write_colr"),
            "{hd}"
        );
        let sd = run(Standard::Smpte170m, false);
        assert!(sd.contains("out_color_matrix=smpte170m"), "{sd}");
        assert!(
            sd.contains("-colorspace smpte170m -color_primaries smpte170m -color_trc smpte170m"),
            "{sd}"
        );
        assert!(
            !sd.contains("-bsf:v"),
            "no h264_metadata in this FFmpeg: {sd}"
        );
        assert!(sd.contains("-movflags +faststart+write_colr"), "{sd}");
        // The tags come before the output file and after the encoder.
        assert!(hd.ends_with("b.mp4"));
    }

    #[test]
    fn a_refused_hardware_encoder_is_recognised_per_family() {
        let mf = EncoderFamily::MediaFoundation;
        assert!(mf.refused("[h264_mf @ 0000] Error initializing the MFT"));
        assert!(mf.refused("[h264_mf @ 0000] could not find a hardware encoder"));
        assert!(!mf.refused("Invalid data found when processing input"));
        assert!(!mf.refused("[h264_videotoolbox @ 0x1] Error: cannot create"));
        let vt = EncoderFamily::VideoToolbox;
        assert!(vt.refused("[h264_videotoolbox @ 0x1] Error: cannot create"));
        assert!(!vt.refused("[h264_mf @ 0000] Error initializing the MFT"));
    }

    #[test]
    fn the_copy_arguments_make_a_constant_upright_picture_without_sound() {
        let a = editing_copy_args(
            Path::new("/in/a.mkv"),
            Path::new("/out/screen.edit.mp4"),
            2,
            EncoderFamily::VideoToolbox,
            "hevc_videotoolbox",
            "30000/1001",
            9_000_000,
            None,
            None,
            false,
        );
        let s = a.join(" ");
        assert!(s.contains("-map 0:2 -an -sn -dn"), "{s}");
        assert!(
            s.contains("fps=30000/1001,scale=trunc(iw/2)*2:trunc(ih/2)*2,format=yuv420p"),
            "{s}"
        );
        assert!(
            s.contains("-c:v hevc_videotoolbox -b:v 9000000 -g 30"),
            "{s}"
        );
        assert!(
            s.contains("-fps_mode cfr") && s.contains("-tag:v hvc1"),
            "{s}"
        );
        assert!(s.ends_with("/out/screen.edit.mp4"));
        let h = editing_copy_args(
            Path::new("a"),
            Path::new("b"),
            0,
            EncoderFamily::VideoToolbox,
            "h264_videotoolbox",
            "25",
            1,
            None,
            None,
            false,
        );
        assert!(!h.join(" ").contains("hvc1"));
    }

    #[test]
    fn the_sound_arguments_keep_step_with_the_picture() {
        let a = sound_args(Path::new("/in/a.mov"), Path::new("/out/mic.wav"), 1, 2);
        let s = a.join(" ");
        assert!(s.contains("-map 0:1 -vn -sn -dn"), "{s}");
        assert!(s.contains("aresample=async=1:first_pts=0"), "{s}");
        assert!(s.contains("-ac 2 -ar 48000 -c:a pcm_f32le -f wav"), "{s}");
    }

    #[test]
    fn long_stereo_sound_is_mixed_down_to_fit_a_wav_file() {
        assert_eq!(sound_channels(2, 600.0), 2);
        assert_eq!(sound_channels(1, 36_000.0), 1);
        assert_eq!(sound_channels(6, 60.0), 2);
        assert_eq!(sound_channels(2, 3.0 * 3600.0), 1);
        assert_eq!(sound_channels(0, 10.0), 1);
    }

    #[test]
    fn the_tolerance_is_one_frame() {
        assert!((duration_tolerance(MediaKind::Video, Some(25.0)) - 0.04).abs() < 1e-9);
        assert_eq!(duration_tolerance(MediaKind::Audio, Some(25.0)), 0.025);
        assert!(duration_tolerance(MediaKind::Video, None) > 0.0);
    }

    // IMP-2 on Linux (decision O10): VA-API H.264 where it works, software VP9 as the labelled
    // fallback, never libx264 or OpenH264. Tested with fakes: scripted encoder listings.
    const LINUX_FULL: &str =
        " V....D h264_vaapi H.264 (VAAPI)\n V....D libvpx-vp9 VP9\n A aac AAC\n";
    const LINUX_NO_VAAPI_BUILD: &str = " V....D libvpx-vp9 VP9\n A aac AAC\n";
    const LINUX_GPL_ONLY: &str = " V....D libx264 H.264\n V....D libopenh264 H.264\n";

    fn names(l: &[CopyEncoder]) -> Vec<&'static str> {
        l.iter().map(|e| e.name).collect()
    }

    #[test]
    fn linux_tries_vaapi_then_vp9_and_only_with_a_render_node() {
        let host = EncoderFamily::for_os("linux");
        assert_eq!(host, EncoderFamily::Vaapi);
        // Hardware present: VA-API first, VP9 stays as the retry.
        let l = copy_ladder(host, "h264", LINUX_FULL, Some("/dev/dri/renderD128"));
        assert_eq!(names(&l), ["h264_vaapi", "libvpx-vp9"]);
        assert!(!l[0].is_software() && l[1].is_software());
        assert_eq!((l[0].codec(), l[1].codec()), ("h264", "vp9"));
        // The build lists h264_vaapi but the computer has no render node (a VM): VP9 only.
        let l = copy_ladder(host, "hevc", LINUX_FULL, None);
        assert_eq!(names(&l), ["libvpx-vp9"]);
        // A build without VA-API: VP9 only, with or without a node.
        assert_eq!(
            names(&copy_ladder(
                host,
                "h264",
                LINUX_NO_VAAPI_BUILD,
                Some("/dev/dri/renderD128")
            )),
            ["libvpx-vp9"]
        );
        // Only GPL or patent-encumbered encoders listed: nothing, never libx264 or OpenH264.
        assert!(copy_ladder(host, "h264", LINUX_GPL_ONLY, Some("/dev/dri/renderD128")).is_empty());
    }

    #[test]
    fn macos_and_windows_ladders_are_unchanged() {
        let mac = EncoderFamily::for_os("macos");
        let l = copy_ladder(
            mac,
            "hevc",
            " V h264_videotoolbox x\n V hevc_videotoolbox y\n",
            None,
        );
        assert_eq!(names(&l), ["hevc_videotoolbox"]);
        // No software fallback on macOS, even when libvpx is there.
        assert!(copy_ladder(mac, "h264", " V libvpx-vp9 x\n", None).is_empty());
        let win = EncoderFamily::for_os("windows");
        assert_eq!(
            names(&copy_ladder(
                win,
                "hevc",
                " V h264_mf x\n V libvpx-vp9 y\n",
                None
            )),
            ["h264_mf"]
        );
        assert!(copy_ladder(
            win,
            "h264",
            " V libvpx-vp9 y\n",
            Some("/dev/dri/renderD128")
        )
        .is_empty());
    }

    #[test]
    fn the_vaapi_copy_uploads_nv12_frames_to_the_render_node() {
        let a = editing_copy_args(
            Path::new("a.mkv"),
            Path::new("b.mp4"),
            0,
            EncoderFamily::Vaapi,
            "h264_vaapi",
            "30",
            5_000_000,
            None,
            Some("/dev/dri/renderD129"),
            false,
        );
        let s = a.join(" ");
        assert!(
            s.starts_with("-hide_banner -vaapi_device /dev/dri/renderD129 "),
            "{s}"
        );
        assert!(s.contains("format=nv12,hwupload"), "{s}");
        assert!(s.contains("-c:v h264_vaapi -b:v 5000000 -g 30"), "{s}");
        assert!(!s.contains("libx264") && !s.contains("openh264"), "{s}");
    }

    #[test]
    fn the_vp9_copy_is_constant_rate_upright_fast_and_untagged_for_avc() {
        // A rotated source and a variable-rate source go through the same arguments: the fps
        // filter makes the rate constant, FFmpeg's autorotate makes the picture upright.
        let a = editing_copy_args(
            Path::new("rot.mkv"),
            Path::new("screen.edit.mp4"),
            1,
            EncoderFamily::SoftwareVp9,
            "libvpx-vp9",
            "30000/1001",
            4_000_000,
            None,
            Some("/dev/dri/renderD128"),
            false,
        );
        let s = a.join(" ");
        assert!(!s.contains("-vaapi_device"), "{s}");
        assert!(s.contains("-map 0:1 -an -sn -dn"), "{s}");
        assert!(
            s.contains("fps=30000/1001,scale=trunc(iw/2)*2:trunc(ih/2)*2,format=yuv420p"),
            "{s}"
        );
        assert!(s.contains("-c:v libvpx-vp9 -b:v 4000000 -g 30"), "{s}");
        assert!(s.contains("-deadline realtime"), "{s}");
        assert!(s.contains("-fps_mode cfr"), "{s}");
        assert!(
            !s.contains("hwupload") && !s.contains("hvc1") && !s.contains("avc1"),
            "{s}"
        );
    }

    #[test]
    fn a_refused_vaapi_encoder_is_recognised_and_vp9_is_never_refused() {
        let va = EncoderFamily::Vaapi;
        assert!(va.refused(
            "[AVHWDeviceContext @ 0x1] No VA display found for device /dev/dri/renderD128."
        ));
        assert!(va.refused("Failed to initialise VAAPI connection: -1 (unknown libva error)."));
        assert!(!va.refused("Invalid data found when processing input"));
        assert!(!va.refused("[h264_videotoolbox @ 0x1] Error: cannot create"));
        assert!(!EncoderFamily::SoftwareVp9.refused("libvpx error vaapi failed"));
    }

    #[test]
    fn the_copy_says_when_it_is_software_vp9() {
        let vp9 = CopyEncoder {
            family: EncoderFamily::SoftwareVp9,
            name: "libvpx-vp9",
        };
        let none = EditingCopy::of(vp9, false);
        assert!(none.software && none.codec == "vp9");
        assert!(none
            .note
            .as_deref()
            .unwrap()
            .contains("no hardware H.264 encoder"));
        let refused = EditingCopy::of(vp9, true);
        assert!(refused.note.as_deref().unwrap().contains("did not start"));
        let va = CopyEncoder {
            family: EncoderFamily::Vaapi,
            name: "h264_vaapi",
        };
        let hw = EditingCopy::of(va, false);
        assert!(!hw.software && hw.codec == "h264" && hw.note.is_none());
    }

    /// IMP-2 on the T490 (Intel iHD offers CQP only): the VA-API copy gets constant quality and
    /// no bit rate; a driver with a bit-rate mode keeps `-b:v`.
    #[test]
    fn the_vaapi_copy_uses_constant_quality_when_the_driver_has_no_bit_rate_mode() {
        let run = |cqp: bool| {
            editing_copy_args(
                Path::new("a.mkv"),
                Path::new("b.mp4"),
                0,
                EncoderFamily::Vaapi,
                "h264_vaapi",
                "30",
                5_000_000,
                None,
                Some("/dev/dri/renderD128"),
                cqp,
            )
            .join(" ")
        };
        let q = run(true);
        assert!(
            q.contains("-c:v h264_vaapi -rc_mode CQP -qp 23 -g 30"),
            "{q}"
        );
        assert!(!q.contains("-b:v"), "{q}");
        let b = run(false);
        assert!(b.contains("-c:v h264_vaapi -b:v 5000000 -g 30"), "{b}");
        assert!(!b.contains("CQP"), "{b}");
    }
}
