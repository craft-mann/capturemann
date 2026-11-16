//! A real Long Shot of one window without the daemon: the spike S1 probe for the lab laptop
//! (CAP-6). It captures the window through `Capturer`, scrolls it with the system's scroller
//! (`SendInput` on Windows, XTest on X11, `CGEventPost` on macOS) and writes the stitched image.
//!
//! It scrolls a real window and captures the screen: run it only on a machine set up for that.
//!
//! ```text
//! long_shot_probe (--window "Long Shot test page" | --region x,y,w,h) --out result.png [--max-px 16384] [--horizontal] [--hand-secs 30] [--dump dir] [--park x,y]
//! ```
//!
//! `--park x,y`: where the pointer rests after each wheel step (global physical px; the
//! daemon picks a point beside the area by itself). Progress and the asked distances go to
//! stderr.
//!
//! `--hand-secs N`: Hand scroll for N seconds (the user, or the page's own script, scrolls).
//! Prints one JSON line: frames, end reason, size, bands, scrolls asked, time.

use std::time::{Duration, Instant};

use cm_capture::long_shot::Scroller as _;
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
    /// The frames are kept in memory and written when the run ends, so that writing PNGs does
    /// not slow the run down (the first 4 and the last 8 different ones are kept).
    kept: std::collections::VecDeque<(u32, Frame)>,
}

impl Frames {
    fn flush(&mut self) {
        if let Some((dir, _)) = &self.dump {
            for (n, f) in self.kept.drain(..) {
                if let Ok(png) = f.to_png() {
                    let _ = std::fs::write(dir.join(format!("f{n:03}.png")), png);
                }
            }
        }
    }
}

impl FrameSource for Frames {
    fn grab(&mut self) -> Result<Frame, CmError> {
        let frame = self
            .capturer
            .capture(CaptureRequest::new(self.target), None)?
            .map(|c| c.frame)
            .ok_or_else(|| CmError::invalid_input("capture was cancelled"))?;
        if let Some((_, n)) = &mut self.dump {
            *n += 1;
            // A frame equal to the one kept before it adds nothing to a diagnosis.
            if self.kept.back().is_some_and(|(_, last)| *last == frame) {
                return Ok(frame);
            }
            self.kept.push_back((*n, frame.clone()));
            // The first 4 frames stay (how a capture began is what a diagnosis needs), then the last 8.
            if self.kept.len() > 12 {
                self.kept.remove(4);
            }
        }
        Ok(frame)
    }
}

/// Prints what the driver asks the scroller for (`--trace`).
struct Trace(Box<dyn long_shot::Scroller + Send>);

impl long_shot::Scroller for Trace {
    fn scroll(&mut self, d: Direction, amount_px: u32) -> Result<(), CmError> {
        eprintln!("scroll {amount_px}");
        self.0.scroll(d, amount_px)
    }
    fn next_point(&mut self) -> bool {
        self.0.next_point()
    }
    fn park_at(&mut self, point: Option<(i32, i32)>) {
        self.0.park_at(point);
    }
    fn scroll_back(&mut self, d: Direction, amount_px: u32) -> Result<bool, CmError> {
        eprintln!("scroll back {amount_px}");
        self.0.scroll_back(d, amount_px)
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
    let mut park: Option<(i32, i32)> = None;
    let mut region: Option<String> = None;
    let (mut max_px, mut hand_secs, mut horizontal) = (MAX_LONG_SIDE, 0u64, false);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--window" => window = args.next(),
            "--out" => out = args.next(),
            "--region" => region = args.next(),
            "--max-px" => max_px = args.next().and_then(|v| v.parse().ok()).unwrap_or(max_px),
            "--hand-secs" => hand_secs = args.next().and_then(|v| v.parse().ok()).unwrap_or(0),
            "--horizontal" => horizontal = true,
            "--park" => {
                let v: Vec<i32> = args
                    .next()
                    .unwrap_or_default()
                    .split(',')
                    .filter_map(|p| p.trim().parse().ok())
                    .collect();
                if let [x, y] = v[..] {
                    park = Some((x, y));
                }
            }
            "--dump" => dump = args.next().map(|d| (std::path::PathBuf::from(d), 0)),
            other => return Err(CmError::invalid_input(format!("unknown argument {other}"))),
        }
    }
    let out = out.ok_or_else(|| CmError::invalid_input("--out <png> needed"))?;
    let capturer = Capturer::system();
    let (target, centre, label) = if let Some(r) = region {
        // `--region x,y,w,h` in physical px on the primary display: the scrollable area only,
        // as the overlay's area pick gives it (a window's own toolbar is not part of it).
        let v: Vec<i32> = r.split(',').filter_map(|p| p.trim().parse().ok()).collect();
        let [x, y, w, h] = v[..] else {
            return Err(CmError::invalid_input("--region needs x,y,w,h"));
        };
        let d = capturer
            .list_displays()?
            .displays
            .into_iter()
            .find(|d| d.is_primary)
            .ok_or_else(|| CmError::invalid_input("no primary display"))?;
        let rect = cm_core::PhysicalRect {
            origin: cm_core::PhysicalPoint { x, y },
            size: cm_core::PhysicalSize {
                width: w.max(1) as u32,
                height: h.max(1) as u32,
            },
        };
        (
            CaptureTarget::Region {
                display_id: d.id,
                rect: cm_capture::RegionRect::Physical(rect),
            },
            (d.bounds.origin.x + x + w / 2, d.bounds.origin.y + y + h / 2),
            format!("region {r}"),
        )
    } else {
        let window = window
            .ok_or_else(|| CmError::invalid_input("--window <title part> or --region needed"))?;
        let w = capturer.find_window(&window)?;
        (
            CaptureTarget::Window { window_id: w.id },
            (
                w.bounds.origin.x + (w.bounds.size.width / 2) as i32,
                w.bounds.origin.y + (w.bounds.size.height / 2) as i32,
            ),
            w.title.clone(),
        )
    };
    let mut frames = Frames {
        capturer,
        target,
        dump,
        kept: Default::default(),
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
        let mut scroller = Trace(long_shot::platform_scroller_at(Some(centre))?);
        scroller.park_at(park);
        long_shot::run_auto(
            &mut frames,
            &mut scroller,
            &mut ThreadSleeper,
            cfg,
            &mut || false,
            &mut on_progress,
        )?
    };
    frames.flush();
    std::fs::write(&out, result.image.frame.to_png()?)
        .map_err(|e| CmError::internal(format!("cannot write {out}: {e}")))?;
    println!(
        "{}",
        serde_json::json!({
            "window": label,
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
