//! Opt-in diagnosis of a refused frame (CAP-6): with `CM_LONG_SHOT_DEBUG_DIR=<dir>` set, every
//! frame the stitcher refuses is written there as two PNG files (the last accepted frame and the
//! refused one) and a text file with the reason. Off by default; nothing is ever uploaded; the
//! files hold what was on the screen, so look at them before sending them to anyone. At most
//! [`MAX_DUMPS`] sets are written per process.

use std::sync::atomic::{AtomicU32, Ordering};

use crate::Frame;

/// The environment variable that turns the dump on.
pub const ENV: &str = "CM_LONG_SHOT_DEBUG_DIR";
const MAX_DUMPS: u32 = 60;

static DUMPS: AtomicU32 = AtomicU32::new(0);

fn dir() -> Option<std::path::PathBuf> {
    std::env::var_os(ENV)
        .filter(|v| !v.is_empty())
        .map(std::path::PathBuf::from)
}

/// Whether the dump is on.
pub fn enabled() -> bool {
    dir().is_some()
}

/// Writes `refused-NN-last.png`, `refused-NN-refused.png` and `refused-NN.txt`. Errors are
/// ignored (a diagnosis never stops a capture).
pub fn dump_refused(last: &Frame, refused: &Frame, reason: Option<&str>, note: &str) {
    let Some(dir) = dir() else { return };
    let n = DUMPS.fetch_add(1, Ordering::SeqCst) + 1;
    if n > MAX_DUMPS || std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    for (name, f) in [("last", last), ("refused", refused)] {
        if let Ok(png) = f.to_png() {
            let _ = std::fs::write(dir.join(format!("refused-{n:02}-{name}.png")), png);
        }
    }
    let text = format!(
        "frame size {}x{} px (last {}x{} px)\nreason: {}\n{note}\n",
        refused.width(),
        refused.height(),
        last.width(),
        last.height(),
        reason.unwrap_or("not given"),
    );
    let _ = std::fs::write(dir.join(format!("refused-{n:02}.txt")), text);
}

/// The environment variable that turns the frame trace on (a folder).
pub const TRACE_ENV: &str = "CM_LONG_SHOT_TRACE_DIR";
const MAX_TRACE_FRAMES: u32 = 400;

static TRACED: AtomicU32 = AtomicU32::new(0);

/// Auto scroll only. With `CM_LONG_SHOT_TRACE_DIR=<dir>` set, every frame the run looks at is
/// written as `trace-NNN.png`, and one line per frame (what was asked, what the stitcher said) is
/// appended to `trace.txt`: a real run can then be replayed offline, frame by frame, with the
/// stitcher (`cargo run -p cm-capture --example long_shot_stitch`). Off by default; the files hold
/// what was on the screen. At most [`MAX_TRACE_FRAMES`] per process.
pub fn trace(frame: &Frame, what: &str) {
    let Some(dir) = std::env::var_os(TRACE_ENV)
        .filter(|v| !v.is_empty())
        .map(std::path::PathBuf::from)
    else {
        return;
    };
    let n = TRACED.fetch_add(1, Ordering::SeqCst) + 1;
    if n > MAX_TRACE_FRAMES || std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    if let Ok(png) = frame.to_png() {
        let _ = std::fs::write(dir.join(format!("trace-{n:03}.png")), png);
    }
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("trace.txt"))
    {
        let _ = writeln!(f, "{n:03} {}x{} {what}", frame.width(), frame.height());
    }
}
