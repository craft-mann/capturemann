//! A real Long Shot of one window without the daemon: the spike S1 probe for the lab laptop
//! (CAP-6). It captures the window through `Capturer`, scrolls it with the system's scroller
//! (`SendInput` on Windows, XTest on X11, `CGEventPost` on macOS) and writes the stitched image.
//!
//! It scrolls a real window and captures the screen: run it only on a machine set up for that.
//!
//! ```text
//! long_shot_probe --window "Long Shot test page" --out result.png [--max-px 16384] [--horizontal] [--hand-secs 30] [--dump dir]
//! ```
//!
//! `--hand-secs N`: Hand scroll for N seconds (the user, or the page's own script, scrolls).
//! Prints one JSON line: frames, end reason, size, bands, scrolls asked, time.

use std::time::{Duration, Instant};

use cm_capture::long_shot::{
    self, Direction, FrameSource, Limits, LongShotConfig, ThreadSleeper, MAX_LONG_SIDE,
};
use cm_capture::{CaptureRequest, CaptureTarget, Capturer, Frame};
use cm_core::CmError;

struct Frames {
    capturer: Capturer,
    target: CaptureTarget,
    /// `--dump <dir>`: every frame taken is written there (diagnosis).
    dump: Option<(std::path::PathBuf, u32)>,
}

impl FrameSource for Frames {
    fn grab(&mut self) -> Result<Frame, CmError> {
        let frame = self
            .capturer
            .capture(CaptureRequest::new(self.target), None)?
            .map(|c| c.frame)
            .ok_or_else(|| CmError::invalid_input("capture was cancelled"))?;
        if let Some((dir, n)) = &mut self.dump {
            *n += 1;
            let _ = std::fs::write(dir.join(format!("f{n:03}.png")), frame.to_png()?);
        }
        Ok(frame)
    }
}

fn main() {
    if let Err(e) = run() {
        eprintln!("{{\"error\": {:?}}}", e.to_string());
        std::process::exit(1);
    }
}

fn run() -> Result<(), CmError> {
    cm_capture::ensure_dpi_aware();
    let mut args = std::env::args().skip(1);
    let (mut window, mut out, mut dump) = (None, None, None);
    let (mut max_px, mut hand_secs, mut horizontal) = (MAX_LONG_SIDE, 0u64, false);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--window" => window = args.next(),
            "--out" => out = args.next(),
            "--max-px" => max_px = args.next().and_then(|v| v.parse().ok()).unwrap_or(max_px),
            "--hand-secs" => hand_secs = args.next().and_then(|v| v.parse().ok()).unwrap_or(0),
            "--horizontal" => horizontal = true,
            "--dump" => dump = args.next().map(|d| (std::path::PathBuf::from(d), 0)),
            other => return Err(CmError::invalid_input(format!("unknown argument {other}"))),
        }
    }
    let window = window.ok_or_else(|| CmError::invalid_input("--window <title part> needed"))?;
    let out = out.ok_or_else(|| CmError::invalid_input("--out <png> needed"))?;
    let capturer = Capturer::system();
    let w = capturer.find_window(&window)?;
    let centre = (
        w.bounds.origin.x + (w.bounds.size.width / 2) as i32,
        w.bounds.origin.y + (w.bounds.size.height / 2) as i32,
    );
    let mut frames = Frames {
        capturer,
        target: CaptureTarget::Window { window_id: w.id },
        dump,
    };
    let direction = if horizontal {
        Direction::Horizontal
    } else {
        Direction::Vertical
    };
    let cfg = LongShotConfig {
        direction,
        limits: Limits::with_max_px(Some(max_px)),
        ..Default::default()
    };
    let started = Instant::now();
    let mut progress_lines = 0u32;
    let mut on_progress = |p: long_shot::LongShotProgress| {
        progress_lines += 1;
        eprintln!(
            "frame {} len {} / {} header {} footer {}",
            p.frames, p.len_px, p.max_len, p.header_px, p.footer_px
        );
    };
    let result = if hand_secs > 0 {
        let end = started + Duration::from_secs(hand_secs);
        long_shot::run_hand(
            &mut frames,
            &mut ThreadSleeper,
            cfg,
            &mut || Instant::now() >= end,
            &mut on_progress,
        )?
    } else {
        let mut scroller = long_shot::platform_scroller_at(Some(centre))?;
        long_shot::run_auto(
            &mut frames,
            scroller.as_mut(),
            &mut ThreadSleeper,
            cfg,
            &mut || false,
            &mut on_progress,
        )?
    };
    std::fs::write(&out, result.image.frame.to_png()?)
        .map_err(|e| CmError::internal(format!("cannot write {out}: {e}")))?;
    println!(
        "{}",
        serde_json::json!({
            "window": w.title,
            "window_px": [w.bounds.size.width, w.bounds.size.height],
            "frames": result.image.frames,
            "end": result.end,
            "note": result.end.note(),
            "image_px": [result.image.frame.width(), result.image.frame.height()],
            "header_px": result.image.header_len,
            "footer_px": result.image.footer_len,
            "secs": started.elapsed().as_secs_f64(),
        })
    );
    Ok(())
}
