//! GNOME 46 on Wayland: the picture the desktop's own screenshot picker saved (CAP-1.AC4, window pick).
//!
//! `Screenshot` with `interactive: true` shows GNOME's screenshot UI. The shell takes the picture
//! and saves it as `~/Pictures/Screenshots/Screenshot from <date>.png`, but `xdg-desktop-portal-gnome`
//! then logs `InteractiveScreenshot didn't return a file` and answers with response code 2, so the
//! app gets nothing (measured on a ThinkPad T490, Ubuntu 24.04, Nov 8, 2026:
//! `verification/L1-linux-0.3.0-desktop-wayland-window-experiment-wayland-2026-11-08-claude.md`).
//! Esc in the same UI ends the same way (code 2) but leaves no file. So after a failed interactive
//! request, a PNG that appeared in the shell's Screenshots folder since the request started is the
//! person's choice, and no new file means the picker was cancelled.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use cm_core::{CmError, ErrorCode};

use super::portal::{frame_from_png, ScreenshotPortal};
use crate::Frame;

/// How long after a failed request the shell may still be writing the file.
const GRACE: Duration = Duration::from_millis(2500);
/// Looking again this often during the grace time.
const POLL: Duration = Duration::from_millis(150);

/// The folders the shell saves its screenshots in: `<Pictures>/Screenshots` for the Pictures folder
/// named by `user-dirs.dirs` (a localised desktop may call it something else), then `~/Pictures/Screenshots`.
pub fn screenshot_dirs(home: &Path, user_dirs_text: Option<&str>) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(p) = user_dirs_text.and_then(|t| pictures_dir(t, home)) {
        out.push(p.join("Screenshots"));
    }
    let fallback = home.join("Pictures").join("Screenshots");
    if !out.contains(&fallback) {
        out.push(fallback);
    }
    out
}

/// `XDG_PICTURES_DIR="$HOME/Bilder"` of a `user-dirs.dirs` text (last assignment wins).
fn pictures_dir(text: &str, home: &Path) -> Option<PathBuf> {
    let mut found = None;
    for line in text.lines() {
        let Some(v) = line.trim().strip_prefix("XDG_PICTURES_DIR=") else {
            continue;
        };
        found = Some(v.trim().trim_matches('"').to_owned());
    }
    let raw = found?;
    let path = match raw.strip_prefix("$HOME/") {
        Some(rest) => home.join(rest),
        None => PathBuf::from(&raw),
    };
    (!raw.is_empty() && path.is_absolute() && path != home).then_some(path)
}

/// The newest `.png` in `dirs` modified at or after `since`, if any.
pub fn newest_png_since(dirs: &[PathBuf], since: SystemTime) -> Option<PathBuf> {
    let mut best: Option<(SystemTime, PathBuf)> = None;
    for d in dirs {
        let Ok(rd) = std::fs::read_dir(d) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().is_none_or(|x| !x.eq_ignore_ascii_case("png")) {
                continue;
            }
            let Ok(m) = e.metadata() else { continue };
            let Ok(t) = m.modified() else { continue };
            if m.len() > 0 && t >= since && best.as_ref().is_none_or(|(b, _)| t > *b) {
                best = Some((t, p));
            }
        }
    }
    best.map(|(_, p)| p)
}

/// Wraps the system portal: an interactive request that fails with "no image" (the portal's
/// code 2 or a cancel) is answered with the file the shell saved meanwhile, when there is one.
pub struct ShellSavedFallback {
    inner: Box<dyn ScreenshotPortal>,
    dirs: Vec<PathBuf>,
    grace: Duration,
}

impl ShellSavedFallback {
    pub fn new(inner: Box<dyn ScreenshotPortal>) -> Self {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let text = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| home.as_ref().map(|h| h.join(".config")))
            .and_then(|c| std::fs::read_to_string(c.join("user-dirs.dirs")).ok());
        let dirs = home
            .map(|h| screenshot_dirs(&h, text.as_deref()))
            .unwrap_or_default();
        Self::with_dirs(inner, dirs, GRACE)
    }

    /// Another folder list and grace time (tests).
    pub fn with_dirs(
        inner: Box<dyn ScreenshotPortal>,
        dirs: Vec<PathBuf>,
        grace: Duration,
    ) -> Self {
        ShellSavedFallback { inner, dirs, grace }
    }

    fn wait_for_file(&self, since: SystemTime) -> Option<PathBuf> {
        let start = Instant::now();
        loop {
            if let Some(p) = newest_png_since(&self.dirs, since) {
                return Some(p);
            }
            if start.elapsed() >= self.grace {
                return None;
            }
            std::thread::sleep(POLL);
        }
    }
}

impl ScreenshotPortal for ShellSavedFallback {
    fn take(&self, interactive: bool) -> Result<Frame, CmError> {
        if !interactive {
            return self.inner.take(false);
        }
        // A second of slack: file times and the clock of this process can differ by a little.
        let since = SystemTime::now() - Duration::from_secs(1);
        match self.inner.take(true) {
            Ok(f) => Ok(f),
            // Nobody answered in time: not a picture the person chose.
            Err(e) if e.code() == ErrorCode::ConfirmationTimeout => Err(e),
            Err(e) => match self.wait_for_file(since) {
                Some(path) => {
                    log::info!(
                        "the portal gave no file; using the picture the shell saved: {}",
                        path.display()
                    );
                    let bytes = std::fs::read(&path).map_err(|err| {
                        CmError::internal(format!(
                            "cannot read the screenshot {}: {err}",
                            path.display()
                        ))
                    })?;
                    frame_from_png(&bytes)
                }
                None => Err(e),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// A portal whose interactive answer is scripted.
    struct Scripted(Mutex<Option<Result<Frame, CmError>>>);

    impl ScreenshotPortal for Scripted {
        fn take(&self, interactive: bool) -> Result<Frame, CmError> {
            assert!(interactive);
            self.0.lock().unwrap().take().unwrap()
        }
    }

    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut out, w, h);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            let mut wr = enc.write_header().unwrap();
            wr.write_image_data(&vec![200; (w * h * 4) as usize])
                .unwrap();
        }
        out
    }

    fn declined() -> CmError {
        CmError::user_declined("ended without an image")
    }

    /// CAP-1.AC4: the portal fails (GNOME's code 2) but the shell saved the person's pick: use it.
    #[test]
    fn cap1_ac4_a_file_the_shell_saved_is_the_pick() {
        let dir = tempfile::tempdir().unwrap();
        let shots = dir.path().join("Screenshots");
        std::fs::create_dir(&shots).unwrap();
        std::fs::write(
            shots.join("Screenshot from 2026-11-08 21-12-14.png"),
            png(3, 2),
        )
        .unwrap();
        let f = ShellSavedFallback::with_dirs(
            Box::new(Scripted(Mutex::new(Some(Err(declined()))))),
            vec![shots],
            Duration::from_millis(100),
        );
        let frame = f.take(true).unwrap();
        assert_eq!((frame.size().width, frame.size().height), (3, 2));
    }

    /// Esc leaves no file: the original error stands (nothing is invented).
    #[test]
    fn cap1_ac4_no_new_file_means_cancelled() {
        let dir = tempfile::tempdir().unwrap();
        let shots = dir.path().join("Screenshots");
        std::fs::create_dir(&shots).unwrap();
        let old = shots.join("old.png");
        std::fs::write(&old, png(2, 2)).unwrap();
        let long_ago = SystemTime::now() - Duration::from_secs(3600);
        std::fs::File::options()
            .write(true)
            .open(&old)
            .unwrap()
            .set_modified(long_ago)
            .unwrap();
        let f = ShellSavedFallback::with_dirs(
            Box::new(Scripted(Mutex::new(Some(Err(declined()))))),
            vec![shots],
            Duration::from_millis(100),
        );
        let e = f.take(true).unwrap_err();
        assert_eq!(e.code(), ErrorCode::UserDeclined);
    }

    /// A request nobody answered is a timeout, never "the newest file".
    #[test]
    fn a_timeout_is_not_rescued_by_a_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.png"), png(2, 2)).unwrap();
        let f = ShellSavedFallback::with_dirs(
            Box::new(Scripted(Mutex::new(Some(Err(
                CmError::confirmation_timeout("t"),
            ))))),
            vec![dir.path().to_path_buf()],
            Duration::from_millis(100),
        );
        assert_eq!(
            f.take(true).unwrap_err().code(),
            ErrorCode::ConfirmationTimeout
        );
    }

    // The fixture uses Unix paths; "/home/anna" is not an absolute path on Windows.
    #[cfg(unix)]
    #[test]
    fn the_folders_follow_the_pictures_dir_of_a_localised_desktop() {
        let home = Path::new("/home/anna");
        let d = screenshot_dirs(home, Some("XDG_PICTURES_DIR=\"$HOME/Bilder\"\n"));
        assert_eq!(
            d,
            vec![
                PathBuf::from("/home/anna/Bilder/Screenshots"),
                PathBuf::from("/home/anna/Pictures/Screenshots")
            ]
        );
        assert_eq!(
            screenshot_dirs(home, None),
            vec![PathBuf::from("/home/anna/Pictures/Screenshots")]
        );
    }
}
