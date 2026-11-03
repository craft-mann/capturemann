//! Times `import_media` on generated fixtures (no screen, camera or microphone; the fixtures come
//! from the `ffmpeg` on `PATH`): `cargo run --release -p cm-daemon --example import_bench -- --dir <scratch>
//! [--big-secs 300] [--keep]`. Prints one Markdown row per case: file, size, duration, import seconds,
//! seconds per GB. The library and the files are made in `--dir` and removed unless `--keep`.
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use cm_capture::fake::FakeBackend;
use cm_capture::Capturer;
use cm_core::Actor;
use cm_library::Library;
use serde_json::json;

fn ff(args: &[&str]) {
    let ok = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y"])
        .args(args)
        .stdin(Stdio::null())
        .status()
        .unwrap()
        .success();
    assert!(ok, "ffmpeg {args:?}");
}

fn arg(name: &str) -> Option<String> {
    let a: Vec<String> = std::env::args().collect();
    a.iter()
        .position(|x| x == name)
        .and_then(|i| a.get(i + 1).cloned())
}

#[tokio::main]
async fn main() {
    let dir = PathBuf::from(arg("--dir").expect("--dir <scratch folder>"));
    let big: u32 = arg("--big-secs")
        .and_then(|s| s.parse().ok())
        .unwrap_or(300);
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("src");
    let libdir = dir.join("lib");
    std::fs::create_dir_all(&src).unwrap();
    let s = |p: &Path| p.to_string_lossy().into_owned();
    // Fixtures.
    let (mp4, vfr, m4a, wav) = (
        src.join("big.mp4"),
        src.join("vfr.mkv"),
        src.join("talk.m4a"),
        src.join("memo.wav"),
    );
    ff(&[
        "-f",
        "lavfi",
        "-i",
        &format!("testsrc2=size=1920x1080:rate=30:duration={big}"),
        "-f",
        "lavfi",
        "-i",
        &format!("sine=frequency=440:duration={big}"),
        "-c:v",
        "libx264",
        "-preset",
        "ultrafast",
        "-b:v",
        "30M",
        "-pix_fmt",
        "yuv420p",
        "-c:a",
        "aac",
        &s(&mp4),
    ]);
    ff(&[
        "-f",
        "lavfi",
        "-i",
        "testsrc2=size=1920x1080:rate=30:duration=60",
        "-vf",
        "select=not(eq(mod(n\\,4)\\,1))",
        "-fps_mode",
        "vfr",
        "-c:v",
        "libx264",
        "-preset",
        "ultrafast",
        "-pix_fmt",
        "yuv420p",
        &s(&vfr),
    ]);
    ff(&[
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=300:duration=3600",
        "-c:a",
        "aac",
        &s(&m4a),
    ]);
    ff(&[
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=440:duration=15",
        &s(&wav),
    ]);
    let lib = Library::open_or_create(&libdir).unwrap();
    let daemon =
        cm_daemon::Daemon::builder(Capturer::new(FakeBackend::new()).with_own_pid(None), lib)
            .jobs(cm_daemon::library_job_queue(&libdir).unwrap())
            .build();
    daemon.register_import_handler().unwrap();
    daemon.start_jobs().unwrap();
    println!("| File | Status | Size (MB) | Duration (s) | Import time (s) | Seconds per GB |");
    println!("|---|---|---|---|---|---|");
    for f in [&wav, &m4a, &mp4, &vfr] {
        let bytes = std::fs::metadata(f).unwrap().len();
        let rows = daemon
            .execute_method(
                Actor::User,
                0,
                "import_media",
                &json!({"paths": [s(f)], "dry_run": true}),
            )
            .await
            .unwrap();
        let (status, dur) = (
            rows["files"][0]["status"].as_str().unwrap().to_owned(),
            rows["files"][0]["duration_s"].as_f64().unwrap(),
        );
        let t = Instant::now();
        let out = daemon
            .execute_method(Actor::User, 0, "import_media", &json!({"paths": [s(f)]}))
            .await
            .unwrap();
        let id = out["job_id"].as_str().unwrap().to_owned();
        let state = loop {
            let j = daemon
                .execute_method(Actor::User, 0, "get_job", &json!({"job_id": id}))
                .await
                .unwrap();
            if matches!(j["state"].as_str(), Some("done" | "failed")) {
                break j;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        };
        let secs = t.elapsed().as_secs_f64();
        let note = if state["state"] == "done" {
            String::new()
        } else {
            format!(" ({})", state["error"]["reason"])
        };
        println!(
            "| {}{} | {} | {:.1} | {:.0} | {:.1} | {:.1} |",
            f.file_name().unwrap().to_string_lossy(),
            note,
            status,
            bytes as f64 / 1e6,
            dur,
            secs,
            secs / (bytes as f64 / 1e9)
        );
    }
    if !std::env::args().any(|a| a == "--keep") {
        let _ = std::fs::remove_dir_all(&dir);
    }
}
