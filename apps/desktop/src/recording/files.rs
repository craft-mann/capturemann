//! Recordings in the library (LIB-1): finding a `.cmrec` bundle, the read-only details card and
//! the poster.
//!
//! The daemon lists recordings (`list_library`) but its `get_capture` only opens screenshots, and
//! the list rows carry no folder, so the desktop locates a bundle by reading each top-level
//! `*.cmrec/manifest.json` of the library root for its `id` (read only). Posters of recordings do
//! not exist in the bundle; they are made here, once, and cached under
//! `<library>/.capturemann/posters/<id>.jpg` (a derived file the library may delete). The first
//! frame comes from FFmpeg when it is installed, else from macOS Quick Look (`qlmanage`), else the
//! card shows a styled placeholder. There is no in-process video decoder yet (M4 editor).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
#[cfg(any(target_os = "macos", test))]
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant};

use cm_i18n::tr;
use cm_library::manifest::RecManifest;

/// Why a helper tool (FFmpeg, Quick Look) did not finish. Worded when shown (`Display`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunError {
    /// Killed after this many seconds.
    TimedOut(u64),
    /// The OS text (not started, wait failed).
    Os(String),
}

impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&match self {
            RunError::TimedOut(secs) => tr!("timed out after {secs} s", secs = secs),
            RunError::Os(e) => e.clone(),
        })
    }
}

/// Why a poster (thumbnail) could not be made or shown. The words are made when drawn
/// (`Display`); `Os` and the tool details are the OS or tool text as it came.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PosterError {
    NoScreenTrack,
    /// The screen track could not be opened (the OS reason).
    TrackUnreadable(String),
    NotMp4,
    FfmpegMissing,
    FfmpegNoFrame,
    Ffmpeg(RunError),
    QuickLook(RunError),
    QuickLookNoThumbnail,
    NoThumbnailTool,
    /// FFmpeg failed, then Quick Look failed too.
    Both(Box<PosterError>, Box<PosterError>),
    /// The poster file could not be decoded (the image library's text).
    Decode(String),
    /// An OS error (folder, file, image conversion).
    Os(String),
}

impl std::fmt::Display for PosterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&match self {
            PosterError::NoScreenTrack => tr!("the recording has no screen track"),
            PosterError::TrackUnreadable(e) => {
                tr!("cannot read the screen track: {error}", error = e)
            }
            PosterError::NotMp4 => tr!("the screen track is not an MP4 file"),
            PosterError::FfmpegMissing => tr!("ffmpeg is not installed"),
            PosterError::FfmpegNoFrame => tr!("ffmpeg could not read a frame"),
            PosterError::Ffmpeg(e) => tr!("ffmpeg: {error}", error = e.to_string()),
            PosterError::QuickLook(e) => tr!("Quick Look: {error}", error = e.to_string()),
            PosterError::QuickLookNoThumbnail => tr!("Quick Look made no thumbnail"),
            PosterError::NoThumbnailTool => tr!("no thumbnail tool on this OS"),
            PosterError::Both(a, b) => tr!(
                "{first}; {second}",
                first = a.to_string(),
                second = b.to_string()
            ),
            PosterError::Decode(e) => tr!("cannot decode poster: {error}", error = e),
            PosterError::Os(e) => e.clone(),
        })
    }
}

/// Why a file action or a load of a recording failed. The words are made when the error is shown
/// (`Display`), so a note that stays on screen follows a language switch; `Detail` is text from
/// the OS or a tool, shown as it came.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileError {
    NoLibrary,
    /// The folder of the shown recording is not in the library.
    NoFolder,
    /// No bundle with this id.
    RecordingNotFound(String),
    NoScreenTrack,
    NoExportYet,
    FileGone,
    NotWebLink,
    Unsupported,
    NoLength,
    /// The manifest could not be read (the OS reason).
    ManifestUnreadable(String),
    ManifestDamaged,
    Detail(String),
}

impl std::fmt::Display for FileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&match self {
            FileError::NoLibrary => tr!("The library isn\u{2019}t available."),
            FileError::NoFolder => {
                tr!("This recording\u{2019}s folder couldn\u{2019}t be found in the library.")
            }
            FileError::RecordingNotFound(id) => tr!("Recording {id} was not found.", id = id),
            FileError::NoScreenTrack => tr!("the recording has no screen track"),
            FileError::NoExportYet => tr!("this recording has no exported MP4 yet"),
            FileError::FileGone => tr!("that file is no longer there"),
            FileError::NotWebLink => tr!("that is not a web link"),
            FileError::Unsupported => tr!("not supported on this OS"),
            FileError::NoLength => tr!("This recording has no length to edit."),
            FileError::ManifestUnreadable(e) => tr!("cannot read the manifest: {error}", error = e),
            FileError::ManifestDamaged => tr!("the manifest is damaged"),
            FileError::Detail(e) => e.clone(),
        })
    }
}

impl From<std::io::Error> for FileError {
    fn from(e: std::io::Error) -> FileError {
        FileError::Detail(e.to_string())
    }
}

/// The bundle folder of recording `id` under the library `root`.
pub fn find_bundle(root: &Path, id: &str) -> Option<PathBuf> {
    let entries = std::fs::read_dir(root).ok()?;
    for e in entries.flatten() {
        let path = e.path();
        if path.extension().and_then(|x| x.to_str()) != Some("cmrec") || !path.is_dir() {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(path.join("manifest.json")) else {
            continue;
        };
        if RecManifest::from_json(&text).is_ok_and(|m| m.id == id) {
            return Some(path);
        }
    }
    None
}

/// One track of a recording.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackLine {
    pub name: String,
    pub file: String,
    pub bytes: u64,
    pub missing: bool,
}

/// One rendered export of a recording (`manifest.exports[]`, the `exports/` folder).
#[derive(Debug, Clone, PartialEq)]
pub struct ExportLine {
    /// Relative to the bundle, `exports/<title> - quick.mp4`.
    pub file: String,
    pub preset: String,
    pub bytes: u64,
    /// The file is still on disk (the user may have deleted it).
    pub exists: bool,
    pub encoder: Option<String>,
    pub duration_ms: Option<i64>,
}

/// What the read-only details card shows.
#[derive(Debug, Clone, PartialEq)]
pub struct Details {
    pub id: String,
    pub title: String,
    pub created: time::OffsetDateTime,
    pub duration_s: Option<f64>,
    pub size_bytes: u64,
    pub tracks: Vec<TrackLine>,
    /// Rendered MP4s, oldest first (the quick export is the one played by default).
    pub exports: Vec<ExportLine>,
    /// `3024 × 1964 · 30 fps · hevc`.
    pub video: Option<String>,
    pub pauses: usize,
    /// Copies at destinations (`uploads[]`), oldest first.
    pub uploads: Vec<crate::published::UploadRecord>,
    /// The recording has captions (lines in the manifest, or a transcript to make them from);
    /// the publish sheet offers to upload them.
    pub has_captions: bool,
    /// An audio recording (`kind: "audio"`): it has no details page, it opens its editor.
    pub audio: bool,
    pub dir: PathBuf,
}

fn dir_size(dir: &Path) -> u64 {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| match e.metadata() {
            Ok(m) if m.is_dir() => dir_size(&e.path()),
            Ok(m) => m.len(),
            Err(_) => 0,
        })
        .sum()
}

/// Reads the manifest of a bundle folder.
pub fn read_details(dir: &Path) -> Result<Details, FileError> {
    let text = std::fs::read_to_string(dir.join("manifest.json"))
        .map_err(|e| FileError::ManifestUnreadable(e.to_string()))?;
    let m = RecManifest::from_json(&text).map_err(|_| FileError::ManifestDamaged)?;
    let mut tracks = Vec::new();
    for (name, t) in &m.tracks {
        let file = t
            .get("file")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let bytes = std::fs::metadata(dir.join(&file)).map_or(0, |md| md.len());
        tracks.push(TrackLine {
            name: name.clone(),
            file,
            bytes,
            missing: t.get("missing").and_then(serde_json::Value::as_bool) == Some(true),
        });
    }
    let video = m.tracks.get("screen").map(|s| {
        let mut parts = Vec::new();
        if let Some(a) = s.get("size").and_then(serde_json::Value::as_array) {
            if let (Some(w), Some(h)) = (
                a.first().and_then(serde_json::Value::as_u64),
                a.get(1).and_then(serde_json::Value::as_u64),
            ) {
                parts.push(format!("{w} \u{00D7} {h}"));
            }
        }
        if let Some(f) = s.get("fps").and_then(serde_json::Value::as_f64) {
            parts.push(format!("{} fps", f.round()));
        }
        if let Some(c) = s.get("codec").and_then(serde_json::Value::as_str) {
            parts.push(c.to_owned());
        }
        parts.join(" \u{00B7} ")
    });
    let exports = cm_library::exports_of(&m)
        .iter()
        .filter_map(|e| {
            let file = e.get("file")?.as_str()?.to_owned();
            let on_disk = dir.join(&file);
            Some(ExportLine {
                preset: e
                    .get("preset")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("quick")
                    .to_owned(),
                bytes: std::fs::metadata(&on_disk).map_or(0, |md| md.len()),
                exists: on_disk.is_file(),
                encoder: e
                    .get("encoder")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned),
                duration_ms: e.get("duration_ms").and_then(serde_json::Value::as_i64),
                file,
            })
        })
        .collect();
    Ok(Details {
        id: m.id.clone(),
        title: m.title.clone(),
        created: m.created,
        duration_s: m.duration.map(|d| d.as_secs_f64()),
        size_bytes: dir_size(dir),
        tracks,
        exports,
        video: video.filter(|v| !v.is_empty()),
        pauses: m.gaps.len(),
        uploads: crate::published::parse_uploads(Some(&serde_json::Value::Array(
            cm_library::uploads_of(dir),
        ))),
        has_captions: cm_library::rec_edit::captions_of(&m)
            .and_then(serde_json::Value::as_array)
            .is_some_and(|lines| !lines.is_empty())
            || dir.join("transcript.json").is_file(),
        audio: serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|v| {
                v.get("kind")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
            })
            .as_deref()
            == Some("audio"),
        dir: dir.to_owned(),
    })
}

/// Where the cached poster of recording `id` lives.
pub fn poster_path(root: &Path, id: &str) -> PathBuf {
    let safe: String = id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    root.join(".capturemann")
        .join("posters")
        .join(format!("{safe}.jpg"))
}

/// The longest edge of a generated poster (the library writes screenshot posters at 512).
pub const POSTER_EDGE: u32 = 512;

/// How long a poster tool may run before it is killed (R4-19): a hung `ffmpeg` or `qlmanage` must
/// not stall every thumbnail behind it.
pub const POSTER_TIMEOUT: Duration = Duration::from_secs(10);

/// At most this many poster tools run at once.
const POSTER_PARALLEL: usize = 2;

/// FFmpeg, found the way the exporter finds it (`CM_FFMPEG`, the bundle, `PATH`, the usual
/// folders), not by a second locator of our own (R5-9).
fn ffmpeg() -> Option<PathBuf> {
    cm_media::tools::locate().ok().map(|t| t.ffmpeg)
}

/// Runs `cmd` to its end, or kills it after `timeout` (R4-19). `Ok(true)` when it exited with
/// success.
pub fn run_with_timeout(mut cmd: Command, timeout: Duration) -> Result<bool, RunError> {
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| RunError::Os(e.to_string()))?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status.success()),
            Ok(None) if start.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(RunError::TimedOut(timeout.as_secs()));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(25)),
            Err(e) => return Err(RunError::Os(e.to_string())),
        }
    }
}

/// A counting gate: [`POSTER_PARALLEL`] posters are made at once, the rest wait their turn.
struct Gate {
    free: Mutex<usize>,
    cv: Condvar,
}

static POSTER_GATE: Gate = Gate {
    free: Mutex::new(POSTER_PARALLEL),
    cv: Condvar::new(),
};

struct Permit(&'static Gate);

impl Gate {
    fn acquire(&'static self) -> Permit {
        let mut n = self.free.lock().unwrap_or_else(PoisonError::into_inner);
        while *n == 0 {
            n = self.cv.wait(n).unwrap_or_else(PoisonError::into_inner);
        }
        *n -= 1;
        Permit(self)
    }
}

impl Drop for Permit {
    fn drop(&mut self) {
        *self.0.free.lock().unwrap_or_else(PoisonError::into_inner) += 1;
        self.0.cv.notify_one();
    }
}

fn poster_with_ffmpeg(bin: &Path, video: &Path, out: &Path) -> Result<(), PosterError> {
    let mut cmd = Command::new(bin);
    cmd.args(["-nostdin", "-y", "-v", "error", "-i"])
        .arg(video)
        .args(["-frames:v", "1", "-vf"])
        .arg(format!("scale={POSTER_EDGE}:-2"))
        .args(["-q:v", "4"])
        .arg(out);
    match run_with_timeout(cmd, POSTER_TIMEOUT) {
        Ok(true) if out.is_file() => Ok(()),
        Ok(_) => Err(PosterError::FfmpegNoFrame),
        Err(e) => Err(PosterError::Ffmpeg(e)),
    }
}

/// A scratch folder no other call shares (the process id alone is shared by every thumbnail).
#[cfg(any(target_os = "macos", test))]
fn unique_temp_dir(prefix: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    std::env::temp_dir().join(format!(
        "{prefix}-{}-{}-{nanos}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ))
}

#[cfg(target_os = "macos")]
fn poster_with_quicklook(video: &Path, out: &Path) -> Result<(), PosterError> {
    let tmp = unique_temp_dir("cm-ql");
    std::fs::create_dir_all(&tmp).map_err(|e| PosterError::Os(e.to_string()))?;
    let result = (|| {
        let mut cmd = Command::new("/usr/bin/qlmanage");
        cmd.args(["-t", "-s", &POSTER_EDGE.to_string(), "-o"])
            .arg(&tmp)
            .arg(video);
        let ok = run_with_timeout(cmd, POSTER_TIMEOUT).map_err(PosterError::QuickLook)?;
        let png = tmp.join(format!(
            "{}.png",
            video
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("video")
        ));
        if !ok || !png.is_file() {
            return Err(PosterError::QuickLookNoThumbnail);
        }
        let img = image::open(&png)
            .map_err(|e| PosterError::Os(e.to_string()))?
            .to_rgb8();
        img.save_with_format(out, image::ImageFormat::Jpeg)
            .map_err(|e| PosterError::Os(e.to_string()))
    })();
    let _ = std::fs::remove_dir_all(&tmp);
    result
}

#[cfg(not(target_os = "macos"))]
fn poster_with_quicklook(_video: &Path, _out: &Path) -> Result<(), PosterError> {
    Err(PosterError::NoThumbnailTool)
}

/// Makes the poster of the recording in `bundle` at `out` (blocking; run it off the UI thread).
/// Order: FFmpeg, then Quick Look. Returns why when neither works.
pub fn generate_poster(bundle: &Path, out: &Path) -> Result<(), PosterError> {
    let video = bundle.join("screen.mp4");
    if !video.is_file() {
        return Err(PosterError::NoScreenTrack);
    }
    // Only a real MP4 (an `ftyp` box first) goes to the tools: Quick Look answers an unreadable
    // file with a generic icon, which must not become a poster.
    let mut head = [0u8; 12];
    std::fs::File::open(&video)
        .and_then(|mut f| std::io::Read::read_exact(&mut f, &mut head))
        .map_err(|e| PosterError::TrackUnreadable(e.to_string()))?;
    if &head[4..8] != b"ftyp" {
        return Err(PosterError::NotMp4);
    }
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).map_err(|e| PosterError::Os(e.to_string()))?;
    }
    // A few at a time, never one global lock: a slow tool delays its own thumbnail only.
    let _turn = POSTER_GATE.acquire();
    let first = match ffmpeg() {
        Some(bin) => poster_with_ffmpeg(&bin, &video, out),
        None => Err(PosterError::FfmpegMissing),
    };
    match first {
        Ok(()) => Ok(()),
        Err(e) => poster_with_quicklook(&video, out)
            .map_err(|q| PosterError::Both(Box::new(e), Box::new(q))),
    }
}

/// The combined MP4 to play and reveal: the newest export that is still on disk, the `quick`
/// preset first (it is the only one for now).
pub fn export_file(bundle: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(bundle.join("manifest.json")).ok()?;
    let m = RecManifest::from_json(&text).ok()?;
    let files: Vec<(String, PathBuf)> = cm_library::exports_of(&m)
        .iter()
        .filter_map(|e| {
            // An audio export (VOC-2) is not the video to play.
            if e.get("kind").and_then(serde_json::Value::as_str) == Some("audio") {
                return None;
            }
            let file = e.get("file")?.as_str()?;
            let preset = e.get("preset").and_then(serde_json::Value::as_str)?;
            Some((preset.to_owned(), bundle.join(file)))
        })
        .filter(|(_, p)| p.is_file())
        .collect();
    files
        .iter()
        .find(|(preset, _)| preset == "quick")
        .or_else(|| files.last())
        .map(|(_, p)| p.clone())
}

/// Opens the recording in the system's default player (`open` / `start`): the combined MP4 when
/// there is one, else the screen master.
pub fn play_in_system_player(bundle: &Path) -> Result<(), FileError> {
    match export_file(bundle) {
        Some(p) => open_path(&p, false),
        None => play_master(bundle),
    }
}

/// Opens the screen master (not the export).
pub fn play_master(bundle: &Path) -> Result<(), FileError> {
    let video = bundle.join("screen.mp4");
    if !video.is_file() {
        return Err(FileError::NoScreenTrack);
    }
    open_path(&video, false)
}

/// Shows the combined MP4 in Finder / Explorer.
pub fn reveal_export(bundle: &Path) -> Result<(), FileError> {
    match export_file(bundle) {
        Some(p) => open_path(&p, true),
        None => Err(FileError::NoExportYet),
    }
}

/// Shows any file or folder in Finder / Explorer (a file a local destination received).
pub fn reveal_path(path: &Path) -> Result<(), FileError> {
    if !path.exists() {
        return Err(FileError::FileGone);
    }
    open_path(path, true)
}

/// Whether a share link is safe to hand to the system: `http` or `https`, one line, no spaces.
pub fn is_openable_link(url: &str) -> bool {
    (url.starts_with("https://") || url.starts_with("http://"))
        && !url.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// Opens a share link in the default browser.
pub fn open_link(url: &str) -> Result<(), FileError> {
    if !is_openable_link(url) {
        return Err(FileError::NotWebLink);
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(url)
            .spawn()
            .map(|_| ())
            .map_err(FileError::from)
    }
    #[cfg(target_os = "windows")]
    {
        Command::new("rundll32")
            .args(["url.dll,FileProtocolHandler", url])
            .spawn()
            .map(|_| ())
            .map_err(FileError::from)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        xdg_open(std::ffi::OsStr::new(url))
    }
}

/// Linux: hands a link or a path to the desktop's opener (`xdg-open`, from xdg-utils). Without it
/// the error says what to install instead of failing silently.
#[allow(dead_code)]
fn xdg_open(target: &std::ffi::OsStr) -> Result<(), FileError> {
    match Command::new("xdg-open")
        .arg(target)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(mut child) => {
            // Reap it so no zombie stays behind; xdg-open itself returns quickly.
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(FileError::Detail(tr!(
            "xdg-open is not installed; install xdg-utils to open links and folders"
        ))),
        Err(e) => Err(e.into()),
    }
}

/// The path `xdg-open` is given to "show" `path` in the file manager: the folder that holds it
/// (the desktop's opener cannot select a file), or the folder itself.
#[allow(dead_code)]
fn folder_to_show(path: &Path) -> PathBuf {
    if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent().unwrap_or(path).to_path_buf()
    }
}

/// Shows the bundle in Finder / Explorer.
pub fn reveal(bundle: &Path) -> Result<(), FileError> {
    open_path(bundle, true)
}

fn open_path(path: &Path, reveal: bool) -> Result<(), FileError> {
    #[cfg(target_os = "macos")]
    {
        let mut c = Command::new("open");
        if reveal {
            c.arg("-R");
        }
        c.arg(path).spawn().map(|_| ()).map_err(FileError::from)
    }
    #[cfg(target_os = "windows")]
    {
        let mut c = if reveal {
            let mut c = Command::new("explorer");
            c.arg("/select,").arg(path);
            c
        } else {
            let mut c = Command::new("cmd");
            c.args(["/C", "start", ""]).arg(path);
            c
        };
        c.spawn().map(|_| ()).map_err(FileError::from)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        if reveal {
            xdg_open(folder_to_show(path).as_os_str())
        } else {
            xdg_open(path.as_os_str())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Linux "Show in folder": a file shows its folder, a bundle folder shows itself.
    #[test]
    fn the_folder_to_show_is_the_file_s_parent_or_the_folder() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.png");
        std::fs::write(&file, "x").unwrap();
        assert_eq!(folder_to_show(&file), dir.path());
        assert_eq!(folder_to_show(dir.path()), dir.path());
    }

    #[test]
    fn only_web_links_are_opened() {
        assert!(is_openable_link("https://drive.google.com/file/d/1/view"));
        assert!(is_openable_link("http://127.0.0.1:8080/x"));
        for bad in [
            "",
            "file:///etc/passwd",
            "javascript:alert(1)",
            "https://a b",
            "https://a\nb",
            "ftp://x",
            "/Applications/Calculator.app",
        ] {
            assert!(!is_openable_link(bad), "{bad:?}");
        }
        assert!(open_link("file:///etc/passwd").is_err());
        assert!(reveal_path(Path::new("/definitely/not/here/xyz")).is_err());
    }

    fn write_bundle(root: &Path, folder: &str, id: &str, title: &str) -> PathBuf {
        let dir = root.join(folder);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("screen.mp4"), vec![0u8; 2048]).unwrap();
        std::fs::write(dir.join("mic.m4a"), vec![0u8; 100]).unwrap();
        let manifest = json!({
            "schema": 1, "id": id, "title": title, "created": "2026-10-21T10:30:00Z",
            "tick_rate": 120_000, "duration": 5_100_000,
            "tracks": {
                "screen": {"file": "screen.mp4", "size": [3024, 1964], "fps": 30, "codec": "hevc"},
                "mic": {"file": "mic.m4a"},
                "camera": {"file": "camera.mp4", "missing": true},
            },
            "gaps": [{"from": 10, "to": 20}],
        });
        std::fs::write(dir.join("manifest.json"), manifest.to_string()).unwrap();
        dir
    }

    /// An audio recording says so, so it opens its editor and not the details page.
    #[test]
    fn an_audio_recording_is_flagged_in_its_details() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = write_bundle(tmp.path(), "Take.cmrec", "rec_a", "Take");
        assert!(!read_details(&dir).unwrap().audio);
        let mut m: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("manifest.json")).unwrap())
                .unwrap();
        m["kind"] = json!("audio");
        std::fs::write(dir.join("manifest.json"), m.to_string()).unwrap();
        assert!(read_details(&dir).unwrap().audio);
    }

    /// LIB-1: a recording's bundle is found by its id, whatever the folder is called.
    #[test]
    fn finds_a_bundle_by_id() {
        let tmp = tempfile::tempdir().unwrap();
        write_bundle(tmp.path(), "Demo.cmrec", "rec_a", "Demo");
        let b = write_bundle(tmp.path(), "Other take.cmrec", "rec_b", "Other");
        std::fs::create_dir_all(tmp.path().join("Shot.cmshot")).unwrap();
        std::fs::write(tmp.path().join("notes.txt"), "x").unwrap();
        assert_eq!(find_bundle(tmp.path(), "rec_b"), Some(b));
        assert_eq!(find_bundle(tmp.path(), "rec_zzz"), None);
        assert_eq!(find_bundle(&tmp.path().join("missing"), "rec_a"), None);
    }

    #[test]
    fn details_card_reads_the_manifest() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = write_bundle(tmp.path(), "Demo.cmrec", "rec_a", "Demo");
        let d = read_details(&dir).unwrap();
        assert_eq!((d.id.as_str(), d.title.as_str()), ("rec_a", "Demo"));
        assert_eq!(
            d.video.as_deref(),
            Some("3024 \u{00D7} 1964 \u{00B7} 30 fps \u{00B7} hevc")
        );
        assert_eq!(d.duration_s, Some(42.5));
        assert_eq!(d.pauses, 1);
        assert!(d.size_bytes >= 2148);
        let camera = d.tracks.iter().find(|t| t.name == "camera").unwrap();
        assert!(camera.missing);
        let screen = d.tracks.iter().find(|t| t.name == "screen").unwrap();
        assert_eq!((screen.file.as_str(), screen.bytes), ("screen.mp4", 2048));
        assert!(read_details(&tmp.path().join("nope")).is_err());
    }

    #[test]
    fn poster_cache_path_is_inside_the_library_metadata_folder() {
        let p = poster_path(Path::new("/lib"), "rec_ab/../x");
        assert_eq!(p, Path::new("/lib/.capturemann/posters/rec_ab----x.jpg"));
    }

    /// EXP-1: the details card lists the exports and the default player target is the quick
    /// export when it exists on disk, else the master.
    #[test]
    fn exports_are_listed_and_played_by_default() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = write_bundle(tmp.path(), "Demo.cmrec", "rec_a", "Demo");
        assert!(read_details(&dir).unwrap().exports.is_empty());
        assert_eq!(export_file(&dir), None);
        std::fs::create_dir_all(dir.join("exports")).unwrap();
        std::fs::write(dir.join("exports/Demo - quick.mp4"), vec![0u8; 500]).unwrap();
        let mut m: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("manifest.json")).unwrap())
                .unwrap();
        m["exports"] = json!([
            {"file": "exports/Demo - quick.mp4", "preset": "quick", "created": "2026-10-21T11:00:00Z",
             "duration_ms": 42500, "size_bytes": 500, "encoder": "h264_videotoolbox", "settings": {}},
            {"file": "exports/Gone - youtube.mp4", "preset": "youtube-1080p", "duration_ms": 1}
        ]);
        std::fs::write(dir.join("manifest.json"), m.to_string()).unwrap();
        let d = read_details(&dir).unwrap();
        assert_eq!(d.exports.len(), 2);
        assert_eq!(d.exports[0].bytes, 500);
        assert!(
            d.exports[0].exists && !d.exports[1].exists,
            "a deleted file is flagged"
        );
        assert_eq!(d.exports[0].encoder.as_deref(), Some("h264_videotoolbox"));
        assert_eq!(d.exports[0].duration_ms, Some(42500));
        assert_eq!(
            export_file(&dir),
            Some(dir.join("exports/Demo - quick.mp4"))
        );
        // Masters stay in the Tracks list, untouched.
        assert!(d.tracks.iter().any(|t| t.name == "screen"));
        // No export on disk: Reveal says so.
        std::fs::remove_file(dir.join("exports/Demo - quick.mp4")).unwrap();
        assert!(reveal_export(&dir).is_err());
    }

    /// R4-19: a tool that hangs is killed after the timeout instead of stalling the thumbnails.
    #[cfg(unix)]
    #[test]
    fn r4_19_a_hung_tool_is_killed() {
        let mut slow = Command::new("sleep");
        slow.arg("30");
        let t = Instant::now();
        let r = run_with_timeout(slow, Duration::from_millis(200));
        assert!(r.is_err_and(|e| e.to_string().contains("timed out")));
        assert!(t.elapsed() < Duration::from_secs(5));
        assert_eq!(
            run_with_timeout(Command::new("true"), POSTER_TIMEOUT),
            Ok(true)
        );
        assert_eq!(
            run_with_timeout(Command::new("false"), POSTER_TIMEOUT),
            Ok(false)
        );
        assert!(run_with_timeout(Command::new("/nonexistent/tool"), POSTER_TIMEOUT).is_err());
    }

    /// R4-19: scratch folders are never shared between two thumbnails of one process.
    #[test]
    fn r4_19_scratch_folders_are_unique() {
        let a = unique_temp_dir("cm-ql");
        let b = unique_temp_dir("cm-ql");
        assert_ne!(a, b);
    }

    /// R4-19: the gate lets two posters through at once and frees a place when one finishes.
    #[test]
    fn r4_19_the_gate_bounds_parallel_posters() {
        static G: Gate = Gate {
            free: Mutex::new(2),
            cv: Condvar::new(),
        };
        let (a, b) = (G.acquire(), G.acquire());
        assert_eq!(*G.free.lock().unwrap(), 0);
        drop(a);
        let c = G.acquire();
        assert_eq!(*G.free.lock().unwrap(), 0);
        drop((b, c));
        assert_eq!(*G.free.lock().unwrap(), 2);
    }

    /// A bundle with no usable video yields an error (the card shows its placeholder), never a
    /// panic.
    #[test]
    fn poster_of_garbage_is_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = write_bundle(tmp.path(), "Demo.cmrec", "rec_a", "Demo");
        let out = tmp.path().join("p").join("rec_a.jpg");
        assert!(generate_poster(&dir, &out).is_err());
        assert!(generate_poster(&tmp.path().join("none"), &out).is_err());
        assert!(play_in_system_player(&tmp.path().join("none")).is_err());
    }
}
