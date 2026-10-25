//! Finding and probing the FFmpeg sidecar.
//!
//! Search order for `ffmpeg` (first hit wins; the first existing candidate is used, there is no
//! fallthrough from a bad `CM_FFMPEG`, so a typo is reported instead of silently using another
//! build):
//!
//! 1. `CM_FFMPEG` (a path to the executable, or to a folder holding `ffmpeg`)
//! 2. next to the running executable (`CaptureMann.app/Contents/MacOS/ffmpeg`, the LGPL build
//!    that ships in the app; it comes before `PATH`, so a Homebrew FFmpeg never changes how an
//!    installed app behaves)
//! 3. `PATH`
//! 4. `/opt/homebrew/bin` and `/usr/local/bin` (a GUI app started from Finder has a short `PATH`)
//!
//! `ffprobe` is looked up the same way (beside the chosen `ffmpeg` first). It is optional: the
//! exporter falls back to the manifest for duration, size and fps.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use cm_core::CmError;

/// Environment variable that overrides the lookup.
pub const ENV_FFMPEG: &str = "CM_FFMPEG";

/// Every "FFmpeg is missing" message starts with this. It is the one contract the app reads a
/// failed job's English reason by ([`is_not_found_message`]); the tests below pin that all the
/// messages of this module keep it.
pub const NOT_FOUND_PREFIX: &str = "FFmpeg was not found";

/// Whether a job's failure reason says FFmpeg is missing (no copy found, or the bundled copy is
/// gone or cannot run).
pub fn is_not_found_message(reason: &str) -> bool {
    reason.contains(NOT_FOUND_PREFIX)
}

/// The message users see when no FFmpeg is found (also the `job.failed` reason).
pub const NOT_FOUND_MESSAGE: &str = "FFmpeg was not found: install it with Homebrew (brew install ffmpeg) or set CM_FFMPEG to the ffmpeg executable. CaptureMann releases include their own copy.";

/// The message when the app is running from a `.app` bundle and the FFmpeg that ships inside it
/// is gone. It keeps the `FFmpeg was not found` prefix, which the app and the job reasons match on.
pub const BUNDLE_MISSING_MESSAGE: &str = "FFmpeg was not found: the copy that ships inside CaptureMann is missing, so the app is damaged or incomplete. Reinstall CaptureMann. (Advanced: set CM_FFMPEG to an ffmpeg executable.)";

/// Is `dir` the `Contents/MacOS` folder of a `.app` bundle?
pub fn in_app_bundle(dir: &Path) -> bool {
    let is = |p: &Path, name: &str| p.file_name().is_some_and(|n| n == name);
    dir.parent().is_some_and(|contents| {
        is(dir, "MacOS")
            && is(contents, "Contents")
            && contents
                .parent()
                .and_then(Path::extension)
                .is_some_and(|e| e == "app")
    })
}

/// The located executables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tools {
    pub ffmpeg: PathBuf,
    pub ffprobe: Option<PathBuf>,
}

impl Tools {
    /// Is this the FFmpeg that ships inside the CaptureMann app?
    pub fn is_bundled(&self) -> bool {
        self.ffmpeg.parent().is_some_and(in_app_bundle)
    }
}

fn exe_name(base: &str) -> String {
    if cfg!(windows) {
        format!("{base}.exe")
    } else {
        base.to_owned()
    }
}

fn is_file(p: &Path) -> bool {
    p.is_file()
}

/// Where each lookup step looks; injectable so the order is unit tested.
#[derive(Debug, Clone, Default)]
pub struct SearchPaths {
    /// Value of `CM_FFMPEG`.
    pub env: Option<OsString>,
    /// Folder of the running executable.
    pub exe_dir: Option<PathBuf>,
    /// Value of `PATH`.
    pub path_var: Option<OsString>,
    /// Fixed fallback folders.
    pub extra_dirs: Vec<PathBuf>,
}

impl SearchPaths {
    /// The real process environment.
    pub fn from_env() -> SearchPaths {
        SearchPaths {
            env: std::env::var_os(ENV_FFMPEG).filter(|v| !v.is_empty()),
            exe_dir: std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(Path::to_path_buf)),
            path_var: std::env::var_os("PATH"),
            extra_dirs: vec![
                PathBuf::from("/opt/homebrew/bin"),
                PathBuf::from("/usr/local/bin"),
            ],
        }
    }

    fn dirs(&self) -> Vec<PathBuf> {
        let mut dirs = Vec::new();
        if let Some(d) = &self.exe_dir {
            dirs.push(d.clone());
        }
        if let Some(p) = &self.path_var {
            dirs.extend(std::env::split_paths(p));
        }
        dirs.extend(self.extra_dirs.iter().cloned());
        dirs
    }

    fn find(&self, base: &str) -> Option<PathBuf> {
        let name = exe_name(base);
        self.dirs()
            .into_iter()
            .map(|d| d.join(&name))
            .find(|p| is_file(p))
    }
}

/// Locates FFmpeg for this process. Errors with `not_found` and [`NOT_FOUND_MESSAGE`] (or a
/// message naming a bad `CM_FFMPEG`).
pub fn locate() -> Result<Tools, CmError> {
    locate_in(&SearchPaths::from_env())
}

/// [`locate`] with explicit search paths.
pub fn locate_in(paths: &SearchPaths) -> Result<Tools, CmError> {
    let ffmpeg = match &paths.env {
        Some(v) => {
            let p = PathBuf::from(v);
            let candidate = if p.is_dir() {
                p.join(exe_name("ffmpeg"))
            } else {
                p
            };
            if !is_file(&candidate) {
                return Err(CmError::not_found(format!(
                    "FFmpeg was not found: {ENV_FFMPEG} points to `{}`, which is not a file. Fix or unset {ENV_FFMPEG}.",
                    candidate.display()
                )));
            }
            candidate
        }
        None => paths.find("ffmpeg").ok_or_else(|| {
            // A packaged app that lost its FFmpeg is damaged: say so instead of sending the
            // user to Homebrew. A developer build keeps the install hint.
            if paths.exe_dir.as_deref().is_some_and(in_app_bundle) {
                CmError::not_found(BUNDLE_MISSING_MESSAGE)
            } else {
                CmError::not_found(NOT_FOUND_MESSAGE)
            }
        })?,
    };
    let beside = ffmpeg.parent().map(|d| d.join(exe_name("ffprobe")));
    let ffprobe = beside
        .filter(|p| is_file(p))
        .or_else(|| paths.find("ffprobe"));
    Ok(Tools { ffmpeg, ffprobe })
}

/// What the located FFmpeg can do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Caps {
    /// First line of `-version`, e.g. `ffmpeg version 9.0.2 Copyright ...`.
    pub version_line: String,
    /// `(major, minor)`; a git build (`N-12345-g...`) counts as very new.
    pub version: (u32, u32),
    pub videotoolbox: bool,
    /// Windows Media Foundation (`h264_mf`).
    pub h264_mf: bool,
    /// Cisco OpenH264 (`libopenh264`), an optional encoder of a developer's FFmpeg (the app does not
    /// ship it and has no OpenH264 of its own).
    pub libopenh264: bool,
    pub nvenc: bool,
    pub qsv: bool,
    pub amf: bool,
    pub libx264: bool,
    pub aac: bool,
    /// VP9 (`libvpx-vp9`, BSD) and Opus (`libopus`, BSD) for WebM (EXP-3).
    pub libvpx_vp9: bool,
    pub libopus: bool,
    /// The GIF encoder and the lossless FFV1 used for the GIF's first stage (both LGPL, native).
    pub gif: bool,
    pub ffv1: bool,
    /// Built with `--enable-gpl`. A GPL FFmpeg is fine on a developer machine and must never be
    /// bundled (NFR-12).
    pub gpl: bool,
}

impl Caps {
    /// `amix=normalize=0` exists from FFmpeg 4.4.
    pub fn amix_normalize(&self) -> bool {
        self.version >= (4, 4)
    }

    /// `-fps_mode` replaces `-vsync` from FFmpeg 5.1.
    pub fn fps_mode(&self) -> bool {
        self.version >= (5, 1)
    }
}

/// Parses `ffmpeg version 6.1.1-static ...`, `ffmpeg version n7.0`, `ffmpeg version N-117-gabc`.
pub fn parse_version(line: &str) -> (u32, u32) {
    let rest = line
        .split_once("version")
        .map_or(line, |(_, r)| r)
        .trim_start();
    let rest = rest.trim_start_matches(['n', 'N', 'v']);
    if rest.starts_with('-') || !rest.starts_with(|c: char| c.is_ascii_digit()) {
        // A git build ("N-12345-g..." / "git-2024-..."): newer than anything we gate on.
        return (99, 0);
    }
    let mut nums = rest
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .map(|s| s.parse::<u32>().unwrap_or(0));
    (nums.next().unwrap_or(0), nums.next().unwrap_or(0))
}

/// Pure part of the probe: reads `-version` and `-encoders` output.
pub fn caps_from_output(version_out: &str, encoders_out: &str) -> Caps {
    let version_line = version_out.lines().next().unwrap_or("").trim().to_owned();
    let has = |name: &str| {
        encoders_out
            .lines()
            .any(|l| l.split_whitespace().nth(1) == Some(name))
    };
    Caps {
        version: parse_version(&version_line),
        videotoolbox: has("h264_videotoolbox"),
        h264_mf: has("h264_mf"),
        libopenh264: has("libopenh264"),
        nvenc: has("h264_nvenc"),
        qsv: has("h264_qsv"),
        amf: has("h264_amf"),
        libx264: has("libx264"),
        aac: has("aac"),
        libvpx_vp9: has("libvpx-vp9"),
        libopus: has("libopus"),
        gif: has("gif"),
        ffv1: has("ffv1"),
        gpl: version_out.contains("--enable-gpl"),
        version_line,
    }
}

fn run_capture(exe: &Path, args: &[&str]) -> Result<String, CmError> {
    let out = Command::new(exe)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| spawn_error(exe, &e))?;
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    Ok(text)
}

/// Maps a spawn failure: a missing or non-executable file is `not_found` with the install hint.
pub fn spawn_error(exe: &Path, e: &std::io::Error) -> CmError {
    use std::io::ErrorKind::{NotFound, PermissionDenied};
    // The copy inside the app exists on disk but cannot start: damaged, or macOS blocked it.
    if exe.parent().is_some_and(in_app_bundle) && matches!(e.kind(), NotFound | PermissionDenied) {
        return CmError::not_found(format!(
            "FFmpeg was not found: the copy inside CaptureMann (`{}`) cannot be run ({e}). The app is damaged or macOS blocked it. Reinstall CaptureMann. (Advanced: set CM_FFMPEG to an ffmpeg executable.)",
            exe.display()
        ));
    }
    match e.kind() {
        NotFound => CmError::not_found(NOT_FOUND_MESSAGE),
        PermissionDenied => CmError::not_found(format!(
            "FFmpeg at `{}` cannot be run (permission denied). {NOT_FOUND_MESSAGE}",
            exe.display()
        )),
        _ => CmError::internal(format!("cannot run FFmpeg at `{}`: {e}", exe.display())),
    }
}

/// Runs `ffmpeg -version` and `-encoders`.
pub fn probe_caps(tools: &Tools) -> Result<Caps, CmError> {
    let version = run_capture(&tools.ffmpeg, &["-hide_banner", "-version"])?;
    if !version.contains("ffmpeg version") {
        return Err(CmError::internal(format!(
            "`{}` does not look like FFmpeg (no version line)",
            tools.ffmpeg.display()
        )));
    }
    let encoders = run_capture(&tools.ffmpeg, &["-hide_banner", "-encoders"])?;
    Ok(caps_from_output(&version, &encoders))
}

/// The H.264 encoder used for the quick export.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoder {
    /// Hardware (macOS). Fast; the default there.
    VideoToolbox,
    /// Windows Media Foundation (hardware where the driver has an MFT, else Microsoft's).
    H264Mf,
    /// Cisco OpenH264 (software, BSD licensed): used only when the FFmpeg on hand has it; the app
    /// ships no software H.264 encoder (the bundled LGPL FFmpeg has none).
    LibOpenH264,
    /// NVIDIA NVENC.
    Nvenc,
    /// Intel Quick Sync.
    Qsv,
    /// AMD AMF.
    Amf,
    /// Software; developer machines with a GPL FFmpeg only (see the crate docs).
    Libx264,
}

impl Encoder {
    /// The default preference order: hardware of the platform first, the BSD software encoder
    /// next, GPL `libx264` last (R5-4).
    pub const PREFERENCE: [Encoder; 7] = [
        Encoder::VideoToolbox,
        Encoder::H264Mf,
        Encoder::LibOpenH264,
        Encoder::Nvenc,
        Encoder::Qsv,
        Encoder::Amf,
        Encoder::Libx264,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Encoder::VideoToolbox => "h264_videotoolbox",
            Encoder::H264Mf => "h264_mf",
            Encoder::LibOpenH264 => "libopenh264",
            Encoder::Nvenc => "h264_nvenc",
            Encoder::Qsv => "h264_qsv",
            Encoder::Amf => "h264_amf",
            Encoder::Libx264 => "libx264",
        }
    }

    /// False for encoders that pull GPL code into the FFmpeg build (`libx264`): a shipped
    /// binary must not use them (NFR-12). The app can refuse to ship, or to export, with one.
    pub fn lgpl_safe(self) -> bool {
        self != Encoder::Libx264
    }

    /// Uses a GPU or the OS media stack rather than the CPU.
    pub fn is_hardware(self) -> bool {
        !matches!(self, Encoder::LibOpenH264 | Encoder::Libx264)
    }

    /// The `-pix_fmt` the encoder takes (Quick Sync wants NV12; the rest take yuv420p).
    pub fn pix_fmt(self) -> &'static str {
        match self {
            Encoder::Qsv => "nv12",
            _ => "yuv420p",
        }
    }
}

impl Caps {
    /// True when this FFmpeg build has `e`.
    pub fn has(&self, e: Encoder) -> bool {
        match e {
            Encoder::VideoToolbox => self.videotoolbox,
            Encoder::H264Mf => self.h264_mf,
            Encoder::LibOpenH264 => self.libopenh264,
            Encoder::Nvenc => self.nvenc,
            Encoder::Qsv => self.qsv,
            Encoder::Amf => self.amf,
            Encoder::Libx264 => self.libx264,
        }
    }
}

/// The encoders to try, best first: `prefer` (when the build has it), then the rest in
/// [`Encoder::PREFERENCE`] order. `lgpl_only` leaves out GPL encoders. Errors when the build
/// has no AAC encoder or no usable H.264 encoder.
pub fn encoder_candidates(
    caps: &Caps,
    prefer: Option<Encoder>,
    lgpl_only: bool,
) -> Result<Vec<Encoder>, CmError> {
    if !caps.aac {
        return Err(CmError::internal(
            "This FFmpeg has no AAC encoder, so the MP4 cannot be written. Install a full FFmpeg (brew install ffmpeg).",
        ));
    }
    let ok = |e: &Encoder| caps.has(*e) && (!lgpl_only || e.lgpl_safe());
    let mut out: Vec<Encoder> = prefer.into_iter().filter(|e| ok(e)).collect();
    for e in Encoder::PREFERENCE {
        if ok(&e) && !out.contains(&e) {
            out.push(e);
        }
    }
    if out.is_empty() {
        return Err(CmError::internal(if lgpl_only {
            "This FFmpeg has no LGPL-compatible H.264 encoder (h264_videotoolbox, h264_mf, libopenh264, h264_nvenc, h264_qsv or h264_amf)."
        } else {
            "This FFmpeg has no H.264 encoder (none of h264_videotoolbox, h264_mf, libopenh264, h264_nvenc, h264_qsv, h264_amf or libx264). Install one with Homebrew (brew install ffmpeg)."
        }));
    }
    Ok(out)
}

/// Picks the encoder: the first of [`encoder_candidates`]. `prefer` forces one if the build has
/// it (tests; a user override later).
pub fn choose_encoder(caps: &Caps, prefer: Option<Encoder>) -> Result<Encoder, CmError> {
    encoder_candidates(caps, prefer, false).map(|c| c[0])
}

/// Does FFmpeg's stderr say the *encoder* could not start (a size, profile, driver or device it
/// refuses), as opposed to bad input, a full disk or a kill? Only those failures justify
/// re-running the export with another encoder (R5-4).
pub fn is_encoder_init_failure(stderr: &str, encoder: Encoder) -> bool {
    let lower = stderr.to_lowercase();
    if lower.contains("no space left on device") || lower.contains("disk full") {
        return false;
    }
    const PHRASES: [&str; 12] = [
        "error while opening encoder",
        "could not open encoder",
        "error initializing output stream",
        "unknown encoder",
        "encoder not found",
        "no capable devices found",
        "cannot load nvcuda",
        "failed to initialize encoder",
        "failed to create hardware device",
        "hardware device setup failed",
        "error creating a mfx session",
        "function not implemented",
    ];
    if PHRASES.iter().any(|p| lower.contains(p)) {
        return true;
    }
    // The encoder's own log line ("[h264_videotoolbox @ 0x...] Error ...").
    let tag = format!("[{} @", encoder.name());
    lower
        .lines()
        .any(|l| l.contains(&tag) && (l.contains("error") || l.contains("failed")))
}

/// Subset of `ffprobe` output the exporter uses.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MediaInfo {
    pub duration_s: Option<f64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fps: Option<f64>,
    pub audio_channels: Option<u32>,
    pub audio_rate: Option<u32>,
    pub has_video: bool,
    pub has_audio: bool,
    pub video_codec: Option<String>,
}

fn parse_rate(s: &str) -> Option<f64> {
    let (n, d) = s.split_once('/')?;
    let (n, d): (f64, f64) = (n.parse().ok()?, d.parse().ok()?);
    (d > 0.0 && n > 0.0).then(|| n / d)
}

/// Parses `ffprobe -print_format json -show_format -show_streams` output.
pub fn parse_probe(json: &str) -> Result<MediaInfo, CmError> {
    let v: serde_json::Value = serde_json::from_str(json)
        .map_err(|e| CmError::internal(format!("ffprobe returned invalid JSON: {e}")))?;
    let num = |x: &serde_json::Value| {
        x.as_f64()
            .or_else(|| x.as_str().and_then(|s| s.parse().ok()))
    };
    let mut info = MediaInfo {
        duration_s: v["format"].get("duration").and_then(num),
        ..Default::default()
    };
    for s in v["streams"].as_array().map_or(&[][..], Vec::as_slice) {
        match s["codec_type"].as_str() {
            Some("video") if !info.has_video => {
                info.has_video = true;
                info.width = s["width"].as_u64().and_then(|n| u32::try_from(n).ok());
                info.height = s["height"].as_u64().and_then(|n| u32::try_from(n).ok());
                info.fps = s["avg_frame_rate"]
                    .as_str()
                    .and_then(parse_rate)
                    .or_else(|| s["r_frame_rate"].as_str().and_then(parse_rate));
                info.video_codec = s["codec_name"].as_str().map(str::to_owned);
                if info.duration_s.is_none() {
                    info.duration_s = s.get("duration").and_then(num);
                }
            }
            Some("audio") if !info.has_audio => {
                info.has_audio = true;
                info.audio_channels = s["channels"].as_u64().and_then(|n| u32::try_from(n).ok());
                info.audio_rate = s["sample_rate"]
                    .as_str()
                    .and_then(|r| r.parse().ok())
                    .or_else(|| {
                        s["sample_rate"]
                            .as_u64()
                            .and_then(|n| u32::try_from(n).ok())
                    });
                if info.duration_s.is_none() {
                    info.duration_s = s.get("duration").and_then(num);
                }
            }
            _ => {}
        }
    }
    Ok(info)
}

/// Runs `ffprobe` on a file.
pub fn probe_file(tools: &Tools, file: &Path) -> Result<MediaInfo, CmError> {
    let ffprobe = tools.ffprobe.as_ref().ok_or_else(|| {
        CmError::not_found("ffprobe was not found next to ffmpeg (install FFmpeg completely)")
    })?;
    let out = Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(file)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| spawn_error(ffprobe, &e))?;
    if !out.status.success() {
        return Err(CmError::internal(format!(
            "ffprobe could not read `{}`: {}",
            file.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    parse_probe(&String::from_utf8_lossy(&out.stdout))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The app recognises a missing FFmpeg in a job's English reason by this prefix.
    /// Every missing-FFmpeg message has a stable id in `cm_core::message` (the typed
    /// "FFmpeg not found" code the app translates), with its parameters.
    #[test]
    fn every_missing_message_has_a_stable_id() {
        use cm_core::message::ffmpeg_missing_id;
        let id = |m: &str| ffmpeg_missing_id(m);
        assert_eq!(id(NOT_FOUND_MESSAGE), Some("ffmpeg_not_found"));
        assert_eq!(id(BUNDLE_MISSING_MESSAGE), Some("ffmpeg_bundle_missing"));
        let notfound = std::io::Error::from(std::io::ErrorKind::NotFound);
        let e = spawn_error(Path::new("/usr/bin/ffmpeg"), &notfound);
        assert_eq!(id(&e.to_string()), Some("ffmpeg_not_found"));
        let denied = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
        let e = spawn_error(Path::new("/opt/x/ffmpeg"), &denied);
        assert_eq!(id(&e.to_string()), Some("ffmpeg_permission_denied"));
        let e = spawn_error(
            Path::new("/Applications/CaptureMann.app/Contents/MacOS/ffmpeg"),
            &notfound,
        );
        let parts = cm_core::message::describe(e.code(), &e.to_string()).unwrap();
        assert_eq!(parts.id, "ffmpeg_bundle_cannot_run");
        assert_eq!(
            parts.get("path"),
            "/Applications/CaptureMann.app/Contents/MacOS/ffmpeg"
        );
        let paths = SearchPaths {
            env: Some("/no/such/ffmpeg".into()),
            ..Default::default()
        };
        let e = locate_in(&paths).unwrap_err();
        let parts = cm_core::message::describe(e.code(), &e.to_string()).unwrap();
        assert_eq!(parts.id, "ffmpeg_env_invalid");
        assert_eq!(parts.get("path"), "/no/such/ffmpeg");
        assert_eq!(id("The export failed."), None);
    }

    #[test]
    fn the_missing_messages_keep_the_prefix_the_app_reads() {
        assert!(is_not_found_message(NOT_FOUND_MESSAGE));
        assert!(is_not_found_message(BUNDLE_MISSING_MESSAGE));
        assert!(NOT_FOUND_MESSAGE.starts_with(NOT_FOUND_PREFIX));
        assert!(BUNDLE_MISSING_MESSAGE.starts_with(NOT_FOUND_PREFIX));
        assert!(!is_not_found_message("The export failed."));
    }

    #[test]
    fn versions_parse() {
        assert_eq!(parse_version("ffmpeg version 9.0.2 Copyright (c)"), (9, 0));
        assert_eq!(parse_version("ffmpeg version n6.1.1-3-gabc"), (6, 1));
        assert_eq!(parse_version("ffmpeg version 4.4.2-0ubuntu0"), (4, 4));
        assert_eq!(parse_version("ffmpeg version N-117123-gdeadbeef"), (99, 0));
        assert_eq!(parse_version("ffmpeg version git-2026-10-01"), (99, 0));
    }

    #[test]
    fn capabilities_come_from_the_encoder_list() {
        let enc = " V....D libx264              libx264 H.264\n V....D h264_videotoolbox    VideoToolbox H.264 Encoder (codec h264)\n A....D aac                  AAC\n A..... aac_at               aac (AudioToolbox)\n";
        let c = caps_from_output(
            "ffmpeg version 7.1 Copyright\nconfiguration: --enable-gpl --enable-libx264\n",
            enc,
        );
        assert!(c.videotoolbox && c.libx264 && c.aac && c.gpl);
        assert!(c.amix_normalize() && c.fps_mode());
        let lgpl = caps_from_output(
            "ffmpeg version 4.3 Copyright\nconfiguration: --enable-videotoolbox\n",
            " V....D h264_videotoolbox VT\n A....D aac AAC\n",
        );
        assert!(lgpl.videotoolbox && !lgpl.libx264 && !lgpl.gpl);
        assert!(!lgpl.amix_normalize() && !lgpl.fps_mode());
    }

    #[test]
    fn encoder_choice_prefers_hardware_and_explains_a_missing_encoder() {
        let mut c = caps_from_output(
            "ffmpeg version 7.1",
            " V h264_videotoolbox x\n V libx264 y\n A aac z\n",
        );
        assert_eq!(choose_encoder(&c, None).unwrap(), Encoder::VideoToolbox);
        assert_eq!(
            choose_encoder(&c, Some(Encoder::Libx264)).unwrap(),
            Encoder::Libx264
        );
        c.videotoolbox = false;
        assert_eq!(
            choose_encoder(&c, Some(Encoder::VideoToolbox)).unwrap(),
            Encoder::Libx264
        );
        c.libx264 = false;
        let e = choose_encoder(&c, None).unwrap_err();
        assert!(e.to_string().contains("no H.264 encoder"), "{e}");
        c.aac = false;
        assert!(choose_encoder(&c, None)
            .unwrap_err()
            .to_string()
            .contains("AAC"));
    }

    /// R5-4: the order, the LGPL filter and the per-encoder flags.
    #[test]
    fn r5_4_encoder_order_and_lgpl_flags() {
        let all = " V h264_videotoolbox x\n V h264_mf x\n V libopenh264 x\n V h264_nvenc x\n V h264_qsv x\n V h264_amf x\n V libx264 y\n A aac z\n";
        let c = caps_from_output("ffmpeg version 7.1", all);
        assert_eq!(
            encoder_candidates(&c, None, false).unwrap(),
            Encoder::PREFERENCE
        );
        // libx264 is last and the only one that is not LGPL safe.
        assert_eq!(*Encoder::PREFERENCE.last().unwrap(), Encoder::Libx264);
        assert!(Encoder::PREFERENCE
            .iter()
            .all(|e| e.lgpl_safe() == (*e != Encoder::Libx264)));
        let lgpl = encoder_candidates(&c, Some(Encoder::Libx264), true).unwrap();
        assert!(!lgpl.contains(&Encoder::Libx264) && lgpl.len() == 6);
        // A forced encoder goes first, the rest follow without a duplicate.
        let forced = encoder_candidates(&c, Some(Encoder::Amf), false).unwrap();
        assert_eq!(forced[0], Encoder::Amf);
        assert_eq!(forced.len(), 7);
        // Windows without hardware: h264_mf, then OpenH264.
        let win = caps_from_output(
            "ffmpeg version 7.1",
            " V h264_mf x\n V libopenh264 y\n A aac z\n",
        );
        assert_eq!(choose_encoder(&win, None).unwrap(), Encoder::H264Mf);
        let only_x264 = caps_from_output("ffmpeg version 7.1", " V libx264 y\n A aac z\n");
        assert!(encoder_candidates(&only_x264, None, true).is_err());
        assert_eq!(choose_encoder(&only_x264, None).unwrap(), Encoder::Libx264);
        assert_eq!(Encoder::Qsv.pix_fmt(), "nv12");
        assert_eq!(Encoder::VideoToolbox.pix_fmt(), "yuv420p");
    }

    /// R5-4: only encoder start-up failures allow a retry with another encoder.
    #[test]
    fn r5_4_retry_only_on_encoder_init_failures() {
        let vt = Encoder::VideoToolbox;
        assert!(is_encoder_init_failure(
            "Error while opening encoder for output stream #0:0 - maybe incorrect parameters such as bit_rate, rate, width or height",
            vt
        ));
        assert!(is_encoder_init_failure(
            "[h264_videotoolbox @ 0x7f] Error encoding frame: -12902",
            vt
        ));
        // Bad input, a full disk, a missing file: no retry.
        assert!(!is_encoder_init_failure(
            "screen.mp4: Invalid data found when processing input",
            vt
        ));
        assert!(!is_encoder_init_failure(
            "av_interleaved_write_frame(): No space left on device",
            vt
        ));
        assert!(!is_encoder_init_failure(
            "[h264_videotoolbox @ 0x7f] Error: No space left on device",
            vt
        ));
        assert!(!is_encoder_init_failure("", vt));
        // Another encoder's tag does not count for this one.
        assert!(!is_encoder_init_failure("[libx264 @ 0x1] error: foo", vt));
    }

    fn touch(dir: &Path, name: &str) -> PathBuf {
        let p = dir.join(exe_name(name));
        std::fs::write(&p, b"#!/bin/sh\n").unwrap();
        p
    }

    #[test]
    fn lookup_order_is_env_then_exe_dir_then_path_then_fixed() {
        let t = tempfile::tempdir().unwrap();
        let mk = |n: &str| {
            let d = t.path().join(n);
            std::fs::create_dir_all(&d).unwrap();
            d
        };
        let (env_dir, exe_dir, path_dir, fixed) = (mk("env"), mk("exe"), mk("path"), mk("fixed"));
        let env_ff = touch(&env_dir, "ffmpeg");
        touch(&env_dir, "ffprobe");
        let exe_ff = touch(&exe_dir, "ffmpeg");
        let path_ff = touch(&path_dir, "ffmpeg");
        let fixed_ff = touch(&fixed, "ffmpeg");
        let base = SearchPaths {
            env: None,
            exe_dir: Some(exe_dir.clone()),
            path_var: Some(std::env::join_paths([&path_dir]).unwrap()),
            extra_dirs: vec![fixed.clone()],
        };
        // CM_FFMPEG as a file, then as a folder.
        let mut p = base.clone();
        p.env = Some(env_ff.clone().into_os_string());
        assert_eq!(locate_in(&p).unwrap().ffmpeg, env_ff);
        assert!(locate_in(&p).unwrap().ffprobe.is_some());
        p.env = Some(env_dir.into_os_string());
        assert_eq!(locate_in(&p).unwrap().ffmpeg, env_ff);
        // Next to the executable beats PATH.
        assert_eq!(locate_in(&base).unwrap().ffmpeg, exe_ff);
        std::fs::remove_file(&exe_ff).unwrap();
        assert_eq!(locate_in(&base).unwrap().ffmpeg, path_ff);
        std::fs::remove_file(&path_ff).unwrap();
        assert_eq!(locate_in(&base).unwrap().ffmpeg, fixed_ff);
        // ffprobe is optional.
        assert_eq!(locate_in(&base).unwrap().ffprobe, None);
    }

    /// The missing-FFmpeg error is `not_found` with the install hint, and a bad `CM_FFMPEG` is
    /// reported (not silently replaced by another build).
    #[test]
    fn missing_ffmpeg_is_not_found_with_the_install_hint() {
        let none = SearchPaths::default();
        let e = locate_in(&none).unwrap_err();
        assert_eq!(e.code(), cm_core::ErrorCode::NotFound);
        assert!(e.to_string().contains("brew install ffmpeg"), "{e}");
        let bad = SearchPaths {
            env: Some("/nonexistent/ffmpeg".into()),
            ..Default::default()
        };
        let e = locate_in(&bad).unwrap_err();
        assert_eq!(e.code(), cm_core::ErrorCode::NotFound);
        assert!(e.to_string().contains("CM_FFMPEG"), "{e}");
        let io = std::io::Error::from(std::io::ErrorKind::NotFound);
        assert_eq!(
            spawn_error(Path::new("/x"), &io).code(),
            cm_core::ErrorCode::NotFound
        );
    }

    /// Inside a `.app` bundle the shipped FFmpeg wins over `PATH`, and its absence is reported as a
    /// damaged app (still starting with "FFmpeg was not found"); a developer build keeps the hint.
    #[test]
    fn bundle_ffmpeg_wins_and_its_loss_says_damaged() {
        let t = tempfile::tempdir().unwrap();
        let macos = t.path().join("CaptureMann.app/Contents/MacOS");
        let path_dir = t.path().join("brew");
        std::fs::create_dir_all(&macos).unwrap();
        std::fs::create_dir_all(&path_dir).unwrap();
        let path_ff = touch(&path_dir, "ffmpeg");
        let paths = SearchPaths {
            env: None,
            exe_dir: Some(macos.clone()),
            path_var: Some(std::env::join_paths([&path_dir]).unwrap()),
            extra_dirs: Vec::new(),
        };
        assert!(in_app_bundle(&macos));
        assert!(!in_app_bundle(&path_dir));
        // No bundled copy: a developer bundle falls back to PATH.
        assert_eq!(locate_in(&paths).unwrap().ffmpeg, path_ff);
        // The bundled copy beats PATH and is reported as bundled.
        let bundled = touch(&macos, "ffmpeg");
        let tools = locate_in(&paths).unwrap();
        assert_eq!(tools.ffmpeg, bundled);
        assert!(tools.is_bundled());
        assert!(!Tools {
            ffmpeg: path_ff,
            ffprobe: None
        }
        .is_bundled());
        // CM_FFMPEG still overrides it.
        let mut over = paths.clone();
        over.env = Some(path_dir.clone().into_os_string());
        assert!(!locate_in(&over).unwrap().is_bundled());
        // Nothing anywhere: damaged-app wording inside a bundle, install hint outside.
        std::fs::remove_file(&bundled).unwrap();
        let none = SearchPaths {
            exe_dir: Some(macos.clone()),
            ..Default::default()
        };
        let e = locate_in(&none).unwrap_err().to_string();
        assert!(e.starts_with("FFmpeg was not found"), "{e}");
        assert!(is_not_found_message(&e), "{e}");
        assert!(
            e.contains("Reinstall CaptureMann") && !e.contains("brew"),
            "{e}"
        );
        let dev_dir = t.path().join("target-debug");
        std::fs::create_dir_all(&dev_dir).unwrap();
        let dev = SearchPaths {
            exe_dir: Some(dev_dir),
            ..Default::default()
        };
        assert!(locate_in(&dev)
            .unwrap_err()
            .to_string()
            .contains("brew install ffmpeg"));
        // A bundled copy that cannot start.
        let io = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
        let e = spawn_error(&macos.join("ffmpeg"), &io).to_string();
        assert!(
            e.contains("Reinstall CaptureMann") && !e.contains("brew"),
            "{e}"
        );
    }

    #[test]
    fn probe_json_parses() {
        let j = r#"{"streams":[{"codec_type":"video","codec_name":"h264","width":1280,"height":720,"avg_frame_rate":"30/1","r_frame_rate":"30/1"},
            {"codec_type":"audio","channels":2,"sample_rate":"48000"}],"format":{"duration":"12.500000"}}"#;
        let i = parse_probe(j).unwrap();
        assert_eq!(
            (i.width, i.height, i.fps),
            (Some(1280), Some(720), Some(30.0))
        );
        assert_eq!((i.audio_channels, i.audio_rate), (Some(2), Some(48000)));
        assert_eq!(i.duration_s, Some(12.5));
        assert!(i.has_video && i.has_audio);
        assert!(parse_probe("not json").is_err());
    }
}
