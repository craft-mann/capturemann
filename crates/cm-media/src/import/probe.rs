//! Reading a media file with `ffprobe` for the import (IMP-1).
//!
//! The probe never writes. It runs `ffprobe` twice at most: once for the streams and the format,
//! once for the timestamps of the first [`PACKET_SAMPLE`] video packets (variable frame rate shows
//! in those, not in the container's average rate). A file the bundled `ffprobe` cannot open at all
//! (AVI, MP3, AIFF, FLAC, OGG and bare AAC have no demuxer in the bundled build) is named from its
//! first bytes by [`sniff_container`].

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use cm_core::CmError;
use serde_json::Value;

use super::{Problem, Reason};
use crate::tools::{spawn_error, Tools};

/// How many video packets the variable-frame-rate check reads.
pub const PACKET_SAMPLE: usize = 300;
/// A probe that takes longer than this is `unreadable_file`.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(30);

/// The chosen video stream.
#[derive(Debug, Clone, PartialEq)]
pub struct VideoStream {
    /// `ffprobe` stream index.
    pub index: u32,
    pub codec: String,
    /// Displayed size in pixels: width and height swapped for a 90 or 270 degree rotation.
    pub width: u32,
    pub height: u32,
    /// Frame rate (the average, else the nominal one); `None` when the file says neither.
    pub fps: Option<f64>,
    /// 0, 90, 180 or 270 (degrees the picture is turned to display it).
    pub rotation: u16,
    pub pix_fmt: Option<String>,
    /// PQ or HLG transfer.
    pub hdr: bool,
    /// Frame times are not evenly spaced (see [`vfr_from_pts`]).
    pub variable_frame_rate: bool,
}

/// The chosen audio stream.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioStream {
    pub index: u32,
    /// Position among the audio streams (`0:a:N`).
    pub audio_index: u32,
    pub codec: String,
    pub sample_rate: u32,
    pub channels: u32,
    /// Length of this stream, when the container says (else the file's).
    pub duration_s: Option<f64>,
}

/// What `ffprobe` says about a file, reduced to what the import decides on.
#[derive(Debug, Clone, PartialEq)]
pub struct ProbedFile {
    pub path: PathBuf,
    pub size_bytes: u64,
    /// `format_name`, for example `mov,mp4,m4a,3gp,3g2,mj2`, `matroska,webm` or `wav`.
    pub container: String,
    pub duration_s: f64,
    pub video: Option<VideoStream>,
    pub audio: Option<AudioStream>,
    /// Audio streams besides the chosen one (not imported).
    pub other_audio_streams: usize,
    /// A stream is protected (DRM or encryption).
    pub protected: bool,
}

/// Why a probe produced no [`ProbedFile`].
#[derive(Debug, Clone, PartialEq)]
pub enum ProbeError {
    /// The file can be named but not imported.
    Problem(Problem),
    /// The OS refused to read it ("Operation not permitted"): `permission_missing`, `files`.
    Permission { path: PathBuf, message: String },
    /// Anything else (`not_found`, FFmpeg missing, ...).
    Other(CmError),
}

impl From<CmError> for ProbeError {
    fn from(e: CmError) -> Self {
        ProbeError::Other(e)
    }
}

/// Container families the import reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    /// MP4, MOV, M4V, M4A (and 3GP).
    Mov,
    Matroska,
    Wav,
    Other,
}

impl ProbedFile {
    pub fn family(&self) -> Family {
        family_of(&self.container)
    }
}

/// The family of a `format_name`.
pub fn family_of(container: &str) -> Family {
    let first = container.split(',').next().unwrap_or("");
    match first {
        "mov" | "mp4" | "m4a" => Family::Mov,
        "matroska" | "webm" => Family::Matroska,
        "wav" => Family::Wav,
        _ => Family::Other,
    }
}

/// Windows "sharing violation" and "lock violation": another program has the file open without
/// allowing reads (a recording still being written, a sync client). Not a permission problem.
const WIN_SHARING_VIOLATION: i32 = 32;
const WIN_LOCK_VIOLATION: i32 = 33;

/// Maps an I/O error from opening or reading the file. EPERM (1) is what macOS answers when the
/// app has no access to a protected folder or an external drive; on Windows the same situation is
/// "access denied" (`PermissionDenied`: file permissions, Controlled folder access, an antivirus).
/// Both are `permission_missing` with `data.permission: "files"`. A missing file is `not_found`;
/// a Windows file open in another program is `unreadable_file` with that reason.
pub fn map_open_error(path: &Path, e: &std::io::Error) -> ProbeError {
    map_open_error_on(cfg!(windows), path, e)
}

/// [`map_open_error`] for a chosen system (`windows`), so both wordings are tested anywhere.
pub fn map_open_error_on(windows: bool, path: &Path, e: &std::io::Error) -> ProbeError {
    use std::io::ErrorKind::{NotFound, PermissionDenied};
    if windows
        && matches!(
            e.raw_os_error(),
            Some(WIN_SHARING_VIOLATION | WIN_LOCK_VIOLATION)
        )
    {
        return ProbeError::Problem(Problem::new(
            Reason::UnreadableFile,
            format!(
                "`{}` is open in another program. Close it there (or wait for it to finish saving) and try again.",
                path.display()
            ),
        ));
    }
    let denied = if windows {
        e.kind() == PermissionDenied
    } else {
        e.raw_os_error() == Some(1)
    };
    if denied {
        let how = if windows {
            "Check the file's permissions, allow CaptureMann in Windows Security > Ransomware protection > Controlled folder access, or choose the file in the file picker."
        } else {
            "Allow access to its folder in System Settings > Privacy & Security > Files and Folders, or choose the file in the file picker."
        };
        return ProbeError::Permission {
            path: path.to_path_buf(),
            message: format!(
                "CaptureMann is not allowed to read `{}`. {how}",
                path.display()
            ),
        };
    }
    match e.kind() {
        NotFound => ProbeError::Other(CmError::not_found(format!(
            "`{}` does not exist",
            path.display()
        ))),
        PermissionDenied => ProbeError::Problem(Problem::new(
            Reason::UnreadableFile,
            format!("`{}` cannot be read (permission denied).", path.display()),
        )),
        _ => ProbeError::Problem(Problem::new(
            Reason::UnreadableFile,
            format!("`{}` cannot be read: {e}.", path.display()),
        )),
    }
}

/// Opens the file for reading and returns its first bytes (up to `n`) and its size. This is the
/// access check the daemon runs on every file before it queues a job.
pub fn read_head(path: &Path, n: usize) -> Result<(Vec<u8>, u64), ProbeError> {
    let meta = std::fs::metadata(path).map_err(|e| map_open_error(path, &e))?;
    if meta.is_dir() {
        return Err(ProbeError::Other(CmError::invalid_input(format!(
            "`{}` is a folder. Folders can't be imported yet: open the folder and select the files inside.",
            path.display()
        ))));
    }
    let mut f = std::fs::File::open(path).map_err(|e| map_open_error(path, &e))?;
    let mut buf = vec![0u8; n];
    let mut got = 0;
    while got < n {
        match f.read(&mut buf[got..]) {
            Ok(0) => break,
            Ok(k) => got += k,
            Err(e) => return Err(map_open_error(path, &e)),
        }
    }
    buf.truncate(got);
    Ok((buf, meta.len()))
}

/// Names a media container from its first bytes when `ffprobe` cannot open it: the formats the
/// bundled FFmpeg has no demuxer for. `None` for a signature this does not know (and for the
/// formats the import does read).
pub fn sniff_container(head: &[u8]) -> Option<&'static str> {
    let at = |i: usize, s: &[u8]| head.get(i..i + s.len()) == Some(s);
    if at(0, b"RIFF") && at(8, b"AVI ") {
        return Some("avi");
    }
    if at(0, b"fLaC") {
        return Some("flac");
    }
    if at(0, b"OggS") {
        return Some("ogg");
    }
    if at(0, b"FORM") && (at(8, b"AIFF") || at(8, b"AIFC")) {
        return Some("aiff");
    }
    if at(0, b"ID3") {
        return Some("mp3");
    }
    if at(0, b"FLV\x01") {
        return Some("flv");
    }
    if at(0, &[0x30, 0x26, 0xB2, 0x75, 0x8E, 0x66, 0xCF, 0x11]) {
        return Some("asf");
    }
    if at(0, &[0x00, 0x00, 0x01, 0xBA]) {
        return Some("mpeg");
    }
    if let (Some(&b0), Some(&b1)) = (head.first(), head.get(1)) {
        // ADTS (bare AAC): sync 0xFFF, layer bits 00. MPEG audio (MP3): sync 0xFFE, layer != 00.
        if b0 == 0xFF && (b1 & 0xF6) == 0xF0 {
            return Some("aac");
        }
        if b0 == 0xFF && (b1 & 0xE0) == 0xE0 && (b1 >> 1) & 3 != 0 {
            return Some("mp3");
        }
    }
    None
}

fn num(v: &Value) -> Option<f64> {
    v.as_f64()
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
}

fn rate(s: &Value) -> Option<f64> {
    let s = s.as_str()?;
    let (n, d) = s.split_once('/')?;
    let (n, d): (f64, f64) = (n.parse().ok()?, d.parse().ok()?);
    (n > 0.0 && d > 0.0).then(|| n / d)
}

/// Sample entries that mean the stream is encrypted: FairPlay (`drms`, `drmi`) and common
/// encryption (`encv`, `enca`, `encs`, `enct`, `encm`).
const PROTECTED_TAGS: [&str; 7] = ["drms", "drmi", "encv", "enca", "encs", "enct", "encm"];

fn stream_is_protected(s: &Value) -> bool {
    let tag = s["codec_tag_string"].as_str().unwrap_or("").to_lowercase();
    if PROTECTED_TAGS.contains(&tag.as_str()) {
        return true;
    }
    s["side_data_list"].as_array().is_some_and(|l| {
        l.iter().any(|d| {
            d["side_data_type"]
                .as_str()
                .is_some_and(|t| t.to_lowercase().contains("encryption"))
        })
    })
}

/// Whether `ffprobe`'s error text says the file is protected.
pub fn stderr_says_protected(text: &str) -> bool {
    let t = text.to_lowercase();
    t.contains("encrypted") || t.contains("drm") || t.contains("cenc") || t.contains("fairplay")
}

/// Rotation in degrees, 0/90/180/270, from the display matrix side data or the old `rotate` tag.
fn rotation_of(s: &Value) -> u16 {
    let from_side = s["side_data_list"]
        .as_array()
        .and_then(|l| l.iter().find_map(|d| d.get("rotation").and_then(num)));
    let degrees = from_side
        .or_else(|| s["tags"]["rotate"].as_str().and_then(|r| r.parse().ok()))
        .unwrap_or(0.0);
    // The display matrix is counter-clockwise: -90 turns the picture clockwise by 90.
    let d = ((-degrees).rem_euclid(360.0) / 90.0).round() as i64 % 4;
    (d * 90) as u16
}

/// The parts of the `ffprobe` JSON the import decides on. Pure; unit tested on JSON text.
pub fn parse_ffprobe(json: &str, path: &Path, size_bytes: u64) -> Result<ProbedFile, CmError> {
    let v: Value = serde_json::from_str(json)
        .map_err(|e| CmError::internal(format!("ffprobe returned invalid JSON: {e}")))?;
    let container = v["format"]["format_name"].as_str().unwrap_or("").to_owned();
    let streams = v["streams"].as_array().map_or(&[][..], Vec::as_slice);
    let mut protected = streams.iter().any(stream_is_protected);
    let mut video: Option<VideoStream> = None;
    let mut audio_streams: Vec<&Value> = Vec::new();
    for s in streams {
        match s["codec_type"].as_str() {
            Some("video") if s["disposition"]["attached_pic"].as_i64() != Some(1) => {
                if video.is_none() {
                    let rot = rotation_of(s);
                    let (w, h) = (
                        s["width"].as_u64().unwrap_or(0) as u32,
                        s["height"].as_u64().unwrap_or(0) as u32,
                    );
                    let transfer = s["color_transfer"].as_str().unwrap_or("");
                    let avg = rate(&s["avg_frame_rate"]);
                    let nominal = rate(&s["r_frame_rate"]);
                    video = Some(VideoStream {
                        index: s["index"].as_u64().unwrap_or(0) as u32,
                        codec: s["codec_name"].as_str().unwrap_or("unknown").to_owned(),
                        width: if rot % 180 == 90 { h } else { w },
                        height: if rot % 180 == 90 { w } else { h },
                        fps: avg.or(nominal),
                        rotation: rot,
                        pix_fmt: s["pix_fmt"].as_str().map(str::to_owned),
                        hdr: matches!(transfer, "smpte2084" | "arib-std-b67"),
                        variable_frame_rate: vfr_from_rates(avg, nominal),
                    });
                }
            }
            Some("audio") => audio_streams.push(s),
            _ => {}
        }
    }
    // The default-disposition audio stream, else the first.
    let chosen = audio_streams
        .iter()
        .position(|s| s["disposition"]["default"].as_i64() == Some(1))
        .unwrap_or(0);
    let audio = audio_streams.get(chosen).map(|s| AudioStream {
        index: s["index"].as_u64().unwrap_or(0) as u32,
        audio_index: chosen as u32,
        codec: s["codec_name"].as_str().unwrap_or("unknown").to_owned(),
        sample_rate: s["sample_rate"]
            .as_str()
            .and_then(|r| r.parse().ok())
            .or_else(|| s["sample_rate"].as_u64().map(|r| r as u32))
            .unwrap_or(0),
        channels: s["channels"].as_u64().unwrap_or(0) as u32,
        duration_s: num(&s["duration"]).filter(|d| *d > 0.0),
    });
    let duration_s = num(&v["format"]["duration"])
        .or_else(|| streams.iter().find_map(|s| num(&s["duration"])))
        .unwrap_or(0.0);
    if let Some(e) = v.get("error") {
        if stderr_says_protected(&e.to_string()) {
            protected = true;
        }
    }
    Ok(ProbedFile {
        path: path.to_path_buf(),
        size_bytes,
        container,
        duration_s,
        other_audio_streams: audio_streams.len().saturating_sub(1),
        video,
        audio,
        protected,
    })
}

/// Variable frame rate from the container's two rates: the average and the nominal one differ by
/// more than 5%. (A file with neither rate is treated as variable: nothing says it is constant.)
pub fn vfr_from_rates(avg: Option<f64>, nominal: Option<f64>) -> bool {
    match (avg, nominal) {
        (Some(a), Some(r)) => (a - r).abs() / r > 0.05,
        (None, None) => true,
        _ => false,
    }
}

/// Variable frame rate from presentation times: after sorting (B-frames arrive out of order), more
/// than 2% of the gaps differ from the median gap by over 25%. A constant-rate file whose times
/// are rounded to a coarse time base (33, 33, 34 ms) stays well under that.
pub fn vfr_from_pts(pts: &[f64]) -> bool {
    if pts.len() < 8 {
        return false;
    }
    let mut t: Vec<f64> = pts.to_vec();
    t.sort_by(f64::total_cmp);
    let gaps: Vec<f64> = t.windows(2).map(|w| w[1] - w[0]).collect();
    let mut sorted = gaps.clone();
    sorted.sort_by(f64::total_cmp);
    let median = sorted[sorted.len() / 2];
    if median <= 0.0 {
        return false;
    }
    let off = gaps
        .iter()
        .filter(|g| (**g - median).abs() / median > 0.25 && (**g - median).abs() > 0.002)
        .count();
    off as f64 / gaps.len() as f64 > 0.02
}

/// Runs `cmd` and collects stdout and stderr, killing it after `timeout`.
fn run_timeout(
    mut cmd: Command,
    exe: &Path,
    timeout: Duration,
) -> Result<(bool, String, String), CmError> {
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| spawn_error(exe, &e))?;
    let mut out = child.stdout.take();
    let mut err = child.stderr.take();
    let t_out = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(o) = out.as_mut() {
            let mut b = Vec::new();
            let _ = o.read_to_end(&mut b);
            s = String::from_utf8_lossy(&b).into_owned();
        }
        s
    });
    let t_err = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(o) = err.as_mut() {
            let mut b = Vec::new();
            let _ = o.read_to_end(&mut b);
            s = String::from_utf8_lossy(&b).into_owned();
        }
        s
    });
    let start = Instant::now();
    let ok = loop {
        match child.try_wait() {
            Ok(Some(st)) => break Ok(st.success()),
            Ok(None) if start.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(e) => {
                return Err(CmError::internal(format!(
                    "waiting for ffprobe failed: {e}"
                )))
            }
        }
    };
    let (o, e) = (
        t_out.join().unwrap_or_default(),
        t_err.join().unwrap_or_default(),
    );
    match ok {
        Ok(success) => Ok((success, o, e)),
        Err(()) => Err(CmError::internal("ffprobe timed out")),
    }
}

/// Probes `path`. `Ok` for any file `ffprobe` can open; classification (supported or not) is
/// [`super::classify`]. An unopenable file is named from its first bytes when possible.
pub fn probe(tools: &Tools, path: &Path) -> Result<ProbedFile, ProbeError> {
    let (head, size) = read_head(path, 64)?;
    if size == 0 {
        return Err(ProbeError::Problem(Problem::new(
            Reason::UnreadableFile,
            "The file is empty.",
        )));
    }
    let ffprobe = tools.ffprobe.as_ref().ok_or_else(|| {
        CmError::not_found("ffprobe was not found next to ffmpeg (install FFmpeg completely)")
    })?;
    let mut cmd = cm_platform::process::command(ffprobe);
    cmd.args([
        "-v",
        "error",
        "-print_format",
        "json",
        "-show_format",
        "-show_streams",
    ])
    .arg(path);
    let (ok, stdout, stderr) = match run_timeout(cmd, ffprobe, PROBE_TIMEOUT) {
        Ok(r) => r,
        Err(e) if e.to_string().contains("timed out") => {
            return Err(ProbeError::Problem(Problem::new(
                Reason::UnreadableFile,
                "Reading the file took too long.",
            )))
        }
        Err(e) => return Err(e.into()),
    };
    if !ok {
        if stderr_says_protected(&stderr) {
            return Err(ProbeError::Problem(Problem::new(
                Reason::ProtectedMedia,
                "This file is protected (DRM) and can't be imported.",
            )));
        }
        if let Some(fmt) = sniff_container(&head) {
            return Err(ProbeError::Problem(Problem::unsupported_format(fmt)));
        }
        return Err(ProbeError::Problem(Problem::new(
            Reason::UnreadableFile,
            "The file can't be read. It may be damaged, cut short or not a media file.",
        )));
    }
    let mut file = parse_ffprobe(&stdout, path, size).map_err(ProbeError::Other)?;
    if let Some(v) = file.video.as_mut() {
        let pts = sample_video_times(ffprobe, path, v.index);
        v.variable_frame_rate = match pts {
            Some(t) if t.len() >= 8 => vfr_from_pts(&t),
            _ => v.variable_frame_rate,
        };
    }
    Ok(file)
}

/// Presentation times of the first video packets of stream `index`, in seconds.
fn sample_video_times(ffprobe: &Path, path: &Path, index: u32) -> Option<Vec<f64>> {
    let mut cmd = cm_platform::process::command(ffprobe);
    cmd.args(["-v", "error", "-select_streams"])
        .arg(index.to_string())
        .args(["-read_intervals"])
        .arg(format!("%+#{PACKET_SAMPLE}"))
        .args([
            "-show_entries",
            "packet=pts_time,dts_time",
            "-print_format",
            "json",
        ])
        .arg(path);
    let (ok, out, _) = run_timeout(cmd, ffprobe, PROBE_TIMEOUT).ok()?;
    if !ok {
        return None;
    }
    let v: Value = serde_json::from_str(&out).ok()?;
    let t: Vec<f64> = v["packets"]
        .as_array()?
        .iter()
        .filter_map(|p| num(&p["pts_time"]).or_else(|| num(&p["dts_time"])))
        .collect();
    Some(t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn parse(v: Value) -> ProbedFile {
        parse_ffprobe(&v.to_string(), Path::new("/x/a.mov"), 1000).unwrap()
    }

    fn video(extra: Value) -> Value {
        let mut s = json!({"index": 0, "codec_type": "video", "codec_name": "h264",
            "width": 1920, "height": 1080, "pix_fmt": "yuv420p",
            "avg_frame_rate": "30/1", "r_frame_rate": "30/1", "codec_tag_string": "avc1"});
        for (k, x) in extra.as_object().unwrap() {
            s[k] = x.clone();
        }
        s
    }

    fn audio(index: u32, extra: Value) -> Value {
        let mut s = json!({"index": index, "codec_type": "audio", "codec_name": "aac",
            "sample_rate": "48000", "channels": 2, "codec_tag_string": "mp4a"});
        for (k, x) in extra.as_object().unwrap() {
            s[k] = x.clone();
        }
        s
    }

    fn file(streams: Vec<Value>) -> Value {
        json!({"format": {"format_name": "mov,mp4,m4a,3gp,3g2,mj2", "duration": "12.5"},
               "streams": streams})
    }

    #[test]
    fn a_plain_video_reads_streams_and_duration() {
        let p = parse(file(vec![video(json!({})), audio(1, json!({}))]));
        let v = p.video.clone().unwrap();
        assert_eq!((v.width, v.height, v.rotation), (1920, 1080, 0));
        assert_eq!(v.fps, Some(30.0));
        assert!(!v.variable_frame_rate && !v.hdr);
        assert_eq!(p.audio.as_ref().unwrap().codec, "aac");
        assert_eq!(p.duration_s, 12.5);
        assert_eq!(p.family(), Family::Mov);
        assert!(!p.protected);
    }

    #[test]
    fn rotation_swaps_the_displayed_size() {
        // A phone clip: the display matrix says -90 (turn clockwise by 90).
        let p = parse(file(vec![video(json!({
            "width": 1920, "height": 1080,
            "side_data_list": [{"side_data_type": "Display Matrix", "rotation": -90}]
        }))]));
        let v = p.video.unwrap();
        assert_eq!((v.width, v.height, v.rotation), (1080, 1920, 90));
        let p = parse(file(vec![video(json!({"tags": {"rotate": "180"}}))]));
        assert_eq!(p.video.unwrap().rotation, 180);
        let p = parse(file(vec![video(json!({
            "side_data_list": [{"rotation": 90}]
        }))]));
        assert_eq!(p.video.as_ref().unwrap().rotation, 270);
    }

    #[test]
    fn the_default_audio_stream_is_chosen_and_the_rest_counted() {
        let p = parse(file(vec![
            video(json!({})),
            audio(1, json!({"codec_name": "ac3"})),
            audio(2, json!({"disposition": {"default": 1}})),
            audio(3, json!({})),
        ]));
        let a = p.audio.unwrap();
        assert_eq!((a.index, a.audio_index, a.codec.as_str()), (2, 1, "aac"));
        assert_eq!(p.other_audio_streams, 2);
        let p = parse(file(vec![audio(0, json!({})), audio(1, json!({}))]));
        assert_eq!(p.audio.unwrap().index, 0);
    }

    #[test]
    fn cover_art_is_not_video() {
        let art = video(json!({"codec_name": "mjpeg", "disposition": {"attached_pic": 1}}));
        let p = parse(file(vec![audio(0, json!({})), art]));
        assert!(p.video.is_none());
        assert!(p.audio.is_some());
    }

    #[test]
    fn hdr_and_protection_are_flagged() {
        let p = parse(file(vec![video(json!({"color_transfer": "smpte2084"}))]));
        assert!(p.video.unwrap().hdr);
        let p = parse(file(vec![video(json!({"color_transfer": "arib-std-b67"}))]));
        assert!(p.video.unwrap().hdr);
        for tag in ["drmi", "encv", "ENCA"] {
            let p = parse(file(vec![video(json!({"codec_tag_string": tag}))]));
            assert!(p.protected, "{tag}");
        }
        let p = parse(file(vec![audio(
            0,
            json!({"side_data_list": [{"side_data_type": "Encryption initialization info"}]}),
        )]));
        assert!(p.protected);
        assert!(stderr_says_protected("Error: the stream is encrypted"));
        assert!(!stderr_says_protected(
            "Invalid data found when processing input"
        ));
    }

    #[test]
    fn variable_frame_rate_from_rates_and_times() {
        assert!(!vfr_from_rates(Some(29.97), Some(29.97)));
        assert!(vfr_from_rates(Some(24.0), Some(30.0)));
        assert!(vfr_from_rates(None, None));
        // 60 packets at 30 fps with the times rounded to milliseconds: constant.
        let cfr: Vec<f64> = (0..60)
            .map(|i| (f64::from(i) / 30.0 * 1000.0).round() / 1000.0)
            .collect();
        assert!(!vfr_from_pts(&cfr));
        // B-frame order does not matter.
        let mut shuffled = cfr.clone();
        shuffled.swap(3, 4);
        shuffled.swap(10, 12);
        assert!(!vfr_from_pts(&shuffled));
        // Every fifth frame dropped: gaps of 1/30 and 2/30.
        let vfr: Vec<f64> = (0..60)
            .filter(|i| i % 5 != 0 || *i == 0)
            .map(|i| f64::from(i) / 30.0)
            .collect();
        assert!(vfr_from_pts(&vfr));
        assert!(!vfr_from_pts(&[0.0, 0.1, 0.5]));
    }

    #[test]
    fn signatures_name_the_unsupported_containers() {
        let avi = b"RIFF\x00\x00\x00\x00AVI LIST";
        assert_eq!(sniff_container(avi), Some("avi"));
        assert_eq!(sniff_container(b"fLaC\0\0\0\""), Some("flac"));
        assert_eq!(sniff_container(b"OggS\0\x02"), Some("ogg"));
        assert_eq!(sniff_container(b"FORM\0\0\0\0AIFFCOMM"), Some("aiff"));
        assert_eq!(sniff_container(b"ID3\x04\0"), Some("mp3"));
        assert_eq!(sniff_container(&[0xFF, 0xFB, 0x90, 0x00]), Some("mp3"));
        assert_eq!(sniff_container(&[0xFF, 0xF1, 0x50, 0x80]), Some("aac"));
        assert_eq!(sniff_container(&[0xFF, 0xF9, 0x50, 0x80]), Some("aac"));
        // WAV and MP4 are read, not named here.
        assert_eq!(sniff_container(b"RIFF\0\0\0\0WAVEfmt "), None);
        assert_eq!(sniff_container(b"\0\0\0\x18ftypmp42"), None);
        assert_eq!(sniff_container(b""), None);
    }

    #[test]
    fn open_errors_map_to_the_shared_codes() {
        let eperm = std::io::Error::from_raw_os_error(1);
        match map_open_error_on(false, Path::new("/Users/x/Desktop/a.mov"), &eperm) {
            ProbeError::Permission { path, message } => {
                assert!(path.ends_with("a.mov"));
                assert!(message.contains("Files and Folders"), "{message}");
            }
            other => panic!("{other:?}"),
        }
        // Windows: access denied is the permission error; a sharing violation is not.
        let denied = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
        match map_open_error_on(true, Path::new("C:\\Users\\x\\a.mov"), &denied) {
            ProbeError::Permission { message, .. } => {
                assert!(message.contains("Controlled folder access"), "{message}");
                assert!(!message.contains("System Settings"), "{message}");
            }
            other => panic!("{other:?}"),
        }
        let sharing = std::io::Error::from_raw_os_error(32);
        match map_open_error_on(true, Path::new("a.mov"), &sharing) {
            ProbeError::Problem(p) => assert!(p.message.contains("open in another program")),
            other => panic!("{other:?}"),
        }
        // On macOS and Linux a plain PermissionDenied (EACCES) is still just unreadable.
        assert!(matches!(
            map_open_error_on(false, Path::new("a"), &denied),
            ProbeError::Problem(_)
        ));
        let nf = std::io::Error::from(std::io::ErrorKind::NotFound);
        assert!(matches!(
            map_open_error(Path::new("/x"), &nf),
            ProbeError::Other(CmError::NotFound { .. })
        ));
        let dir = tempfile::tempdir().unwrap();
        match read_head(dir.path(), 8) {
            Err(ProbeError::Other(e)) => {
                assert!(e.to_string().contains("Folders can't be imported yet"))
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            read_head(&dir.path().join("none"), 8),
            Err(ProbeError::Other(CmError::NotFound { .. }))
        ));
    }
}
