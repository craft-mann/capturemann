//! Spike for the window picker (R19, R20): can every listed window be captured for a thumbnail,
//! how long does it take, and what happens to minimized windows and windows on another display?
//! Run by hand: `cargo test -p cm-capture --test real_window_thumbs -- --ignored --nocapture`.
//! It captures your windows into memory (nothing is written to disk) and prints a table.
#![cfg(any(target_os = "macos", target_os = "windows"))]
#![allow(clippy::unwrap_used, clippy::chunks_exact_to_as_chunks)]

use std::time::Instant;

use cm_capture::{CaptureRequest, CaptureTarget, Capturer};
use cm_core::CmError;

#[test]
#[ignore = "captures every window on this machine into memory; run by hand"]
fn every_listed_window_can_be_captured_for_a_thumbnail() {
    let c = Capturer::system();
    let t = match c.list_targets() {
        Ok(t) => t,
        Err(CmError::PermissionMissing { .. }) => {
            eprintln!("Screen Recording permission missing: grant it and rerun");
            return;
        }
        Err(e) => panic!("{e}"),
    };
    println!("{} displays, {} windows", t.displays.len(), t.windows.len());
    let (mut ok, mut failed, mut blank) = (0, 0, 0);
    let mut times = Vec::new();
    for w in &t.windows {
        let started = Instant::now();
        let r = c.capture(
            CaptureRequest::new(CaptureTarget::Window { window_id: w.id }),
            None,
        );
        let ms = started.elapsed().as_millis();
        let (verdict, size) = match r {
            Ok(Some(cap)) => {
                let f = &cap.frame;
                // A frame that is one flat colour is "blank": the OS returned nothing useful.
                let px = f.rgba();
                let first = &px[..4.min(px.len())];
                let flat = px.chunks_exact(4).step_by(97).all(|p| p == first);
                if flat {
                    blank += 1;
                } else {
                    ok += 1;
                }
                (
                    if flat { "BLANK" } else { "ok" },
                    format!("{}x{}", f.width(), f.height()),
                )
            }
            Ok(None) => ("cancelled", String::new()),
            Err(e) => {
                failed += 1;
                ("ERR", e.to_string())
            }
        };
        if verdict == "ok" {
            times.push(ms);
        }
        println!(
            "{verdict:>5} {ms:>5} ms  {:<24} {:<32} display {:?} minimized {} {size}",
            w.app.chars().take(24).collect::<String>(),
            w.title.chars().take(32).collect::<String>(),
            w.display_id,
            w.minimized,
        );
    }
    times.sort_unstable();
    let median = times.get(times.len() / 2).copied().unwrap_or(0);
    println!(
        "ok {ok}, blank {blank}, failed {failed}; median {median} ms, slowest {} ms",
        times.last().copied().unwrap_or(0)
    );
}

#[test]
#[ignore = "captures the first three windows on this machine into memory; run by hand"]
fn window_thumbnail_of_the_first_three_windows() {
    let c = Capturer::system();
    let t = match c.list_targets() {
        Ok(t) => t,
        Err(CmError::PermissionMissing { .. }) => {
            eprintln!("Screen Recording permission missing: grant it and rerun");
            return;
        }
        Err(e) => panic!("{e}"),
    };
    for d in &t.displays {
        println!("display {} {:?} built-in {}", d.id, d.name, d.is_builtin);
    }
    for w in t.windows.iter().take(3) {
        let started = Instant::now();
        let r = c.window_thumbnail(w.id, 360);
        let ms = started.elapsed().as_millis();
        match r {
            Ok(f) => {
                assert!(f.width().max(f.height()) <= 360);
                let png = f.to_png().unwrap();
                println!(
                    "{:<24} {}x{} -> {}x{} png {} bytes, {ms} ms",
                    w.app.chars().take(24).collect::<String>(),
                    w.bounds.size.width,
                    w.bounds.size.height,
                    f.width(),
                    f.height(),
                    png.len()
                );
            }
            Err(e) => println!("{:<24} ERR {e} ({ms} ms)", w.app),
        }
    }
}
