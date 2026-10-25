//! Writes a synthetic `.cmrec` recording (screen frame counter, flash and beep sync pattern,
//! camera, microphone, system audio, one pause, cursor) into a library folder, so the editor, the
//! `capturemann edit` commands and the exporter have something to work on. Nothing is captured
//! from your screen, camera or microphone, and your real library is never touched: `--library`
//! is required.
//!
//! ```text
//! cargo run -p cm-daemon --example make_fixture -- --library /tmp/cm-fixture-library
//! cargo run -p cm-daemon --example make_fixture -- --library DIR --secs 20 --no-camera
//! ```
//!
//! Then open it in the app with its own data folders (see tests/manual/M4-GATE.md):
//!
//! ```text
//! CAPTUREMANN_CONFIG_DIR=/tmp/cm-fixture-config CAPTUREMANN_LIBRARY=/tmp/cm-fixture-library \
//!     cargo run -p cm-desktop
//! ```
//!
//! Flags: `--library DIR` (required), `--title T`, `--secs N` (screen file length, 4 to 120,
//! default 12), `--size WxH` (default 1280x720), `--fps N` (default 30), `--no-camera`,
//! `--no-audio`, `--no-cursor`, `--no-pause`, `--pause-at S --pause-for S` (screen file time).
//! Needs FFmpeg with libx264 (`brew install ffmpeg`).

use std::path::PathBuf;
use std::process::ExitCode;

use cm_daemon::fixture::{make_fixture, FixturePause, FixtureSpec};

fn usage(msg: &str) -> ExitCode {
    eprintln!("make_fixture: {msg}");
    eprintln!(
        "usage: make_fixture --library DIR [--title T] [--secs N] [--size WxH] [--fps N] \
         [--no-camera] [--no-audio] [--no-cursor] [--no-pause] [--pause-at S --pause-for S]"
    );
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let mut spec = FixtureSpec::default();
    let mut library: Option<PathBuf> = None;
    let mut pause_at: Option<f64> = None;
    let mut pause_for: Option<f64> = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut value = |name: &str| args.next().ok_or_else(|| format!("{name} needs a value"));
        let r: Result<(), String> = (|| {
            match a.as_str() {
                "--library" => library = Some(PathBuf::from(value("--library")?)),
                "--title" => spec.title = value("--title")?,
                "--secs" => {
                    spec.secs = value("--secs")?
                        .parse::<u32>()
                        .ok()
                        .filter(|n| (4..=120).contains(n))
                        .ok_or("--secs is 4 to 120")?;
                }
                "--size" => {
                    let v = value("--size")?;
                    let (w, h) = v.split_once('x').ok_or("--size is WxH")?;
                    let size = (
                        w.parse::<u32>().map_err(|_| "--size is WxH")?,
                        h.parse::<u32>().map_err(|_| "--size is WxH")?,
                    );
                    if size.0 < 64 || size.1 < 64 || size.0 % 2 == 1 || size.1 % 2 == 1 {
                        return Err("--size must be even and at least 64x64".to_owned());
                    }
                    spec.size = size;
                }
                "--fps" => {
                    spec.fps = value("--fps")?
                        .parse::<u32>()
                        .ok()
                        .filter(|n| (1..=60).contains(n))
                        .ok_or("--fps is 1 to 60")?;
                }
                "--no-camera" => spec.camera = false,
                "--no-audio" => spec.audio = false,
                "--no-cursor" => spec.cursor = false,
                "--no-pause" => spec.pause = None,
                "--pause-at" => {
                    pause_at = Some(
                        value("--pause-at")?
                            .parse()
                            .map_err(|_| "--pause-at is seconds")?,
                    );
                }
                "--pause-for" => {
                    pause_for = Some(
                        value("--pause-for")?
                            .parse()
                            .map_err(|_| "--pause-for is seconds")?,
                    );
                }
                other => return Err(format!("unknown flag {other}")),
            }
            Ok(())
        })();
        if let Err(e) = r {
            return usage(&e);
        }
    }
    let Some(library) = library else {
        return usage("--library is required (the fixture is never written to your real library)");
    };
    if pause_at.is_some() || pause_for.is_some() {
        spec.pause = Some(FixturePause {
            at_s: pause_at.unwrap_or(5.5),
            len_s: pause_for.unwrap_or(1.0),
        });
    }
    if let Some(p) = spec.pause {
        if p.at_s < 1.0 || p.at_s + 1.0 > f64::from(spec.secs) || p.len_s <= 0.0 {
            return usage("the pause must start after 1 s and end before the last second");
        }
    }
    let tools = match cm_media::tools::locate() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("make_fixture: FFmpeg not found ({e}); brew install ffmpeg");
            return ExitCode::FAILURE;
        }
    };
    match make_fixture(&tools, &library, &spec) {
        Ok(f) => {
            println!("id:      {}", f.id);
            println!("bundle:  {}", f.dir.display());
            println!("library: {}", library.display());
            println!(
                "session: {:.2} s, flashes and beeps at screen file times {:?} s",
                f.session_end_s,
                spec.sync_times_s()
            );
            // Index it, so the CLI and the app find it without waiting for a rescan.
            match cm_library::Library::open_or_create(&library).and_then(|mut l| l.rescan()) {
                Ok(_) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("make_fixture: written, but indexing failed: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        Err(e) => {
            eprintln!("make_fixture: {e}");
            ExitCode::FAILURE
        }
    }
}
