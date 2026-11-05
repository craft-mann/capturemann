//! The editing copy, the decoded sound and the verify step (IMP-2).
//!
//! All of it runs the bundled FFmpeg. Nothing here writes outside the paths it is given, and a
//! cancelled run removes its partial output.

use std::path::Path;
use std::process::Stdio;

use cm_core::CmError;

use super::classify::MediaKind;
use super::probe::{probe, ProbeError, ProbedFile};
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
    /// No hardware H.264 or HEVC encoder in this FFmpeg or on this machine (no software encoder
    /// ships, NFR-12).
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

/// Which family of hardware encoders the editing copy may use. No software encoder ships
/// (NFR-12), so each family names only encoders that run on the machine's media hardware.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncoderFamily {
    /// Apple VideoToolbox (`h264_videotoolbox`, `hevc_videotoolbox`).
    VideoToolbox,
    /// Windows Media Foundation (`h264_mf`), forced onto a hardware transform (`-hw_encoding 1`):
    /// without that option FFmpeg falls back to Microsoft's software encoder when the machine has
    /// no hardware one.
    MediaFoundation,
}

impl EncoderFamily {
    /// The family of the machine this runs on.
    pub fn host() -> Self {
        if cfg!(windows) {
            EncoderFamily::MediaFoundation
        } else {
            EncoderFamily::VideoToolbox
        }
    }

    fn h264(self) -> &'static str {
        match self {
            EncoderFamily::VideoToolbox => "h264_videotoolbox",
            EncoderFamily::MediaFoundation => "h264_mf",
        }
    }

    /// `None` when the family has no HEVC encoder that the copy uses.
    fn hevc(self) -> Option<&'static str> {
        match self {
            EncoderFamily::VideoToolbox => Some("hevc_videotoolbox"),
            EncoderFamily::MediaFoundation => None,
        }
    }

    /// The 8-bit 4:2:0 layout the encoder is fed. Media Foundation takes NV12: Intel's Quick Sync
    /// transform refuses yuv420p with "format negotiation failed".
    fn pixel_format(self) -> &'static str {
        match self {
            EncoderFamily::VideoToolbox => "yuv420p",
            EncoderFamily::MediaFoundation => "nv12",
        }
    }

    /// Arguments that must come with the encoder to keep it on the hardware.
    fn extra_args(self) -> &'static [&'static str] {
        match self {
            EncoderFamily::VideoToolbox => &[],
            EncoderFamily::MediaFoundation => &["-hw_encoding", "1"],
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
    Ok(parse_hardware_encoders(EncoderFamily::host(), &text))
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

/// The FFmpeg arguments of the editing copy. Pure. FFmpeg applies the rotation tag by itself
/// (autorotate), so the copy is upright with no tag.
pub fn editing_copy_args(
    input: &Path,
    output: &Path,
    video_index: u32,
    family: EncoderFamily,
    encoder: &str,
    fps: &str,
    bitrate: u64,
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
    a.push(input.display().to_string());
    a.extend(["-map".to_owned(), format!("0:{video_index}")]);
    a.extend(["-an", "-sn", "-dn"].map(String::from));
    a.extend([
        "-vf".to_owned(),
        format!(
            "fps={fps},scale=trunc(iw/2)*2:trunc(ih/2)*2,format={}",
            family.pixel_format()
        ),
        "-c:v".to_owned(),
        encoder.to_owned(),
        "-b:v".to_owned(),
        bitrate.to_string(),
        "-g".to_owned(),
        format!("{}", rate.round().max(1.0)),
        "-fps_mode".to_owned(),
        "cfr".to_owned(),
    ]);
    a.extend(family.extra_args().iter().map(|x| (*x).to_owned()));
    if encoder.starts_with("hevc") {
        a.extend(["-tag:v", "hvc1"].map(String::from));
    }
    a.extend(["-movflags", "+faststart"].map(String::from));
    a.push(output.display().to_string());
    a
}

/// Makes the editing copy of a video: constant frame rate, upright, 8-bit 4:2:0, no sound.
/// `progress` gets 0..=1. A cancel removes the partial file.
pub fn make_editing_copy(
    tools: &Tools,
    source: &ProbedFile,
    output: &Path,
    progress: &mut dyn FnMut(f64),
    cancel: &dyn Fn() -> bool,
) -> Result<(), RunFailure> {
    let v = source
        .video
        .as_ref()
        .ok_or_else(|| RunFailure::Failed(CmError::invalid_input("the file has no video")))?;
    let family = EncoderFamily::host();
    let (h264, hevc) = hardware_encoders(tools).map_err(RunFailure::Failed)?;
    let encoder =
        pick_encoder(family, &v.codec, h264, hevc).ok_or(RunFailure::NoHardwareEncoder)?;
    let fps = editing_fps(v.fps);
    let rate = v.fps.unwrap_or(30.0).clamp(15.0, 60.0);
    let args = editing_copy_args(
        &source.path,
        output,
        v.index,
        family,
        encoder,
        &fps,
        editing_bitrate(v.width, v.height, rate),
    );
    let mut on = |p: Progress| progress(p.fraction);
    let r = run_ffmpeg(&tools.ffmpeg, &args, source.duration_s, &mut on, cancel);
    match r {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(output);
            // The hardware encoder refused to start (no media engine, a size it cannot do).
            if let RunError::Failed(ce) = &e {
                if family.refused(&ce.to_string()) {
                    return Err(RunFailure::NoHardwareEncoder);
                }
            }
            Err(e.into())
        }
    }
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
        );
        assert!(!v.join(" ").contains("hw_encoding"));
        assert!(v.join(" ").contains("format=yuv420p"));
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
}
