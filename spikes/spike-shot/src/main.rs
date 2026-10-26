//! S1 spike: does `xcap` capture every display and window correctly and fast?
//!
//! Usage: spike-shot [OUT_DIR] [--windows N]
//!
//! For every display it prints logical size, scale factor, captured image size
//! and capture time, then checks image size == logical size x scale (HiDPI).
//! Then it captures up to N visible windows (default 5).
//! Pass criteria (see docs/project/spikes.md): physical-size images, < 150 ms per 4K display.

use std::{path::PathBuf, time::Instant};

use anyhow::{Context, Result};
use xcap::{Monitor, Window};

const BUDGET_MS: f64 = 150.0;

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let mut out_dir = PathBuf::from("spike-out/shot");
    let mut max_windows = 5usize;
    while let Some(a) = args.next() {
        match a.as_str() {
            "--windows" => max_windows = args.next().context("--windows N")?.parse()?,
            other => out_dir = PathBuf::from(other),
        }
    }
    std::fs::create_dir_all(&out_dir)?;
    println!("CaptureMann S1 spike-shot  ->  {}", out_dir.display());
    println!("os: {} {}\n", std::env::consts::OS, std::env::consts::ARCH);

    let mut all_pass = true;

    // ---- Displays -------------------------------------------------------
    let t = Instant::now();
    let monitors =
        Monitor::all().context("Monitor::all failed (missing screen-recording permission?)")?;
    println!(
        "enumerated {} display(s) in {:.1} ms",
        monitors.len(),
        ms(t)
    );
    println!(
        "{:<3} {:<22} {:>11} {:>6} {:>11} {:>9} {:>9}  check",
        "#", "name", "logical", "scale", "image", "cold ms", "warm ms"
    );

    for (i, m) in monitors.iter().enumerate() {
        let name = m
            .friendly_name()
            .or_else(|_| m.name())
            .unwrap_or_else(|_| "?".into());
        let (w, h) = (m.width()?, m.height()?);
        let scale = m.scale_factor().unwrap_or(1.0);

        // Cold capture (first call may set up OS sessions), then 5 warm ones.
        let t = Instant::now();
        let img = m
            .capture_image()
            .with_context(|| format!("capture display {i}"))?;
        let cold = ms(t);
        let mut warm = Vec::new();
        for _ in 0..5 {
            let t = Instant::now();
            let _ = m.capture_image()?;
            warm.push(ms(t));
        }
        warm.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let warm_median = warm[warm.len() / 2];

        // xcap reports points (logical) on macOS, so the image must be
        // width x scale there. On Windows the reported size should already be
        // physical pixels; record what you see in docs/project/spikes.md either way.
        let got = (img.width(), img.height());
        let expect_phys = (
            (w as f32 * scale).round() as u32,
            (h as f32 * scale).round() as u32,
        );
        let size_ok = if cfg!(target_os = "macos") {
            got == expect_phys
        } else {
            got == (w, h) || got == expect_phys
        };
        let fast = warm_median <= BUDGET_MS;
        all_pass &= size_ok && fast;

        println!(
            "{:<3} {:<22} {:>11} {:>6.2} {:>11} {:>9.1} {:>9.1}  {}{}",
            i,
            truncate(&name, 22),
            format!("{w}x{h}"),
            scale,
            format!("{}x{}", img.width(), img.height()),
            cold,
            warm_median,
            if size_ok { "size ok" } else { "SIZE MISMATCH" },
            if fast { "" } else { ", TOO SLOW" },
        );
        img.save(out_dir.join(format!("display-{i}.png")))?;
    }

    // ---- Region capture (the overlay path crops a full capture instead, but
    //      region capture is useful for the CLI `--region`). -----------------
    if let Some(m) = monitors.first() {
        let t = Instant::now();
        match m.capture_region(0, 0, 400, 300) {
            Ok(img) => {
                println!(
                    "\nregion 400x300 on display 0 -> {}x{} in {:.1} ms",
                    img.width(),
                    img.height(),
                    ms(t)
                );
                img.save(out_dir.join("region-400x300.png"))?;
            }
            Err(e) => println!("\nregion capture failed: {e}"),
        }
    }

    // ---- Windows --------------------------------------------------------
    let t = Instant::now();
    let windows = Window::all().context("Window::all failed")?;
    println!(
        "\nenumerated {} window(s) in {:.1} ms",
        windows.len(),
        ms(t)
    );
    let mut done = 0;
    for win in &windows {
        if done >= max_windows {
            break;
        }
        if win.is_minimized().unwrap_or(true) {
            continue;
        }
        let title = win.title().unwrap_or_default();
        let app = win.app_name().unwrap_or_default();
        if title.trim().is_empty()
            || win.width().unwrap_or(0) < 50
            || win.height().unwrap_or(0) < 50
        {
            continue; // skip invisible helper windows
        }
        let t = Instant::now();
        match win.capture_image() {
            Ok(img) => {
                println!(
                    "  [{}] {:<20} {:<40} {:>10} in {:>6.1} ms",
                    win.id().unwrap_or(0),
                    truncate(&app, 20),
                    truncate(&title, 40),
                    format!("{}x{}", img.width(), img.height()),
                    ms(t)
                );
                img.save(out_dir.join(format!("window-{done}.png")))?;
                done += 1;
            }
            Err(e) => println!(
                "  {:<20} {:<40} FAILED: {e}",
                truncate(&app, 20),
                truncate(&title, 40)
            ),
        }
    }
    if done == 0 {
        all_pass = false;
        println!("  no window could be captured");
    }

    println!(
        "\nS1 result: {}  (open {} and eyeball the PNGs: right content, sharp on HiDPI)",
        if all_pass {
            "PASS"
        } else {
            "FAIL - see rows above"
        },
        out_dir.display()
    );
    Ok(())
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n - 1).collect::<String>() + "…"
    }
}
