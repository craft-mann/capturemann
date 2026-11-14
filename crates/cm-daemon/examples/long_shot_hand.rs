//! A real Hand scroll Long Shot through the daemon action, as the desktop app runs it
//! (`capture_screenshot { mode: "long", scroll: "hand" }` as the user, with a `request_id`), in a
//! throwaway library. The scrolling is done by you or by another program; this one stops the run
//! after `--secs` the way the HUD's Stop button does and prints what came back.
//!
//! It captures your screen: run it only on a machine set up for that, and delete the output.
//!
//! ```text
//! CM_LONG_SHOT_DEBUG_DIR=/tmp/ls-dbg long_shot_hand --window "<title part>" --secs 20 --out /tmp/ls.png [--area] [--max-px N]
//! ```
//!
//! `--area` picks the window's rectangle on its display (the overlay's region drag) instead of
//! the window itself. With `CM_LONG_SHOT_DEBUG_DIR` set, every refused frame is written there
//! (see `docs/development/long-shot-debug.md`).

use std::sync::Arc;
use std::time::Duration;

use cm_capture::Capturer;
use cm_core::Actor;
use cm_daemon::Daemon;
use cm_library::Library;
use serde_json::json;

#[tokio::main]
async fn main() {
    cm_capture::ensure_dpi_aware();
    let mut args = std::env::args().skip(1);
    let (mut window, mut out, mut secs, mut area, mut max_px) = (None, None, 20u64, false, None);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--window" => window = args.next(),
            "--out" => out = args.next(),
            "--secs" => secs = args.next().and_then(|v| v.parse().ok()).unwrap_or(secs),
            "--max-px" => max_px = args.next().and_then(|v| v.parse::<u32>().ok()),
            "--area" => area = true,
            other => panic!("unknown argument {other}"),
        }
    }
    let window = window.expect("--window <title part>");
    let dir = std::env::temp_dir().join(format!("ls-hand-{}", std::process::id()));
    let lib = Library::open_or_create(&dir).expect("library");
    let capturer = Capturer::system();
    let w = capturer.find_window(&window).expect("window");
    let targets = capturer.list_targets().expect("targets");
    let target = if area {
        let d = targets
            .displays
            .iter()
            .find(|d| Some(d.id) == w.display_id)
            .expect("display of the window");
        json!({ "display_id": d.id, "rect": [
            w.bounds.origin.x - d.bounds.origin.x,
            w.bounds.origin.y - d.bounds.origin.y,
            w.bounds.size.width, w.bounds.size.height ] })
    } else {
        json!({ "window_id": w.id })
    };
    eprintln!(
        "window {}x{} px at scale {}, target {target}",
        w.bounds.size.width,
        w.bounds.size.height,
        w.scale.get()
    );
    let daemon = Arc::new(Daemon::builder(capturer, lib).build());
    let rid = "long-shot-hand-probe";
    let stopper = daemon.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(secs)).await;
        // The HUD's Stop button: cancels the running call with this request id.
        let ok = stopper.cancel_delayed_capture(rid);
        eprintln!("stop sent: {ok}");
    });
    let mut params = json!({ "mode": "long", "target": target, "direction": "vertical",
        "scroll": "hand", "request_id": rid });
    if let Some(m) = max_px {
        params["max_px"] = json!(m);
    }
    let r = daemon
        .execute_method(Actor::User, 0, "capture_screenshot", &params)
        .await;
    match r {
        Ok(v) => {
            println!(
                "{}",
                serde_json::to_string(&v["long_shot"]).unwrap_or_default()
            );
            println!(
                "{}x{} px, capture {}",
                v["width"], v["height"], v["capture_id"]
            );
            if let Some(out) = out {
                // The stored image: the biggest PNG in the throwaway library.
                let mut best: Option<(u64, std::path::PathBuf)> = None;
                let mut todo = vec![dir.clone()];
                while let Some(d) = todo.pop() {
                    for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
                        let p = e.path();
                        if p.is_dir() {
                            todo.push(p);
                        } else if p.extension().is_some_and(|x| x == "png") {
                            let len = e.metadata().map_or(0, |m| m.len());
                            if best.as_ref().is_none_or(|(l, _)| len > *l) {
                                best = Some((len, p));
                            }
                        }
                    }
                }
                if let Some((_, p)) = best {
                    let _ = std::fs::copy(p, out);
                }
            }
        }
        Err(e) => println!("error: {e}"),
    }
    let _ = std::fs::remove_dir_all(dir);
}
