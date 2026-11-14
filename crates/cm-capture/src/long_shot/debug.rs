//! Opt-in diagnosis of a refused frame (CAP-6): with `CM_LONG_SHOT_DEBUG_DIR=<dir>` set, every
//! frame the stitcher refuses is written there as two PNG files (the last accepted frame and the
//! refused one) and a text file with the reason. Off by default; nothing is ever uploaded; the
//! files hold what was on the screen, so look at them before sending them to anyone. At most
//! [`MAX_DUMPS`] sets are written per process.

use std::sync::atomic::{AtomicU32, Ordering};

use crate::Frame;

/// The environment variable that turns the dump on.
pub const ENV: &str = "CM_LONG_SHOT_DEBUG_DIR";
const MAX_DUMPS: u32 = 12;

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
