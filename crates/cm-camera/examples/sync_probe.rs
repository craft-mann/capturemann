//! Camera sync measurement (CAM-2 AC2, NFR-3). See tests/manual/CAM-2.md.
//!
//! ```text
//! cargo run -p cm-camera --example sync_probe -- list
//! cargo run -p cm-camera --example sync_probe -- latency --yes [--secs 5] [--device ID] [--res 720|1080]
//! cargo run -p cm-camera --example sync_probe -- record  --yes --out DIR [--secs 8] [--comp-ms 0] [--mirror]
//! cargo run -p cm-camera --example sync_probe -- analyze --camera DIR/camera.mp4 --meta DIR/probe.json \
//!        --flashes flashes.json [--display-latency-ms 16]
//! cargo run -p cm-camera --example sync_probe -- compare --screen screen.mp4 --camera camera.mp4
//! ```
//!
//! `latency` and `record` TURN THE CAMERA ON (and `record` stores your image): they require `--yes`,
//! never prompt for permission (they refuse unless camera access is already granted) and are
//! capped at 10 s. Delete the output afterwards. `analyze`/`compare` only read files.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use cm_camera::flash::{
    detect_edges, expected_edges, luma_series, match_edges, summarize_offsets_ms, FlashEvent,
};
use cm_camera::{
    check_camera_permission, list_cameras, probe_delivery_latency, CameraConfig, CameraRecorder,
    CameraResolution,
};
use cm_core::{SessionClock, TrackContext, TrackRecorder};

struct Args(Vec<String>);

impl Args {
    fn flag(&self, name: &str) -> bool {
        self.0.iter().any(|a| a == name)
    }
    fn value(&self, name: &str) -> Option<&str> {
        self.0
            .iter()
            .position(|a| a == name)
            .and_then(|i| self.0.get(i + 1))
            .map(String::as_str)
    }
    fn num<T: std::str::FromStr>(&self, name: &str, default: T) -> T {
        self.value(name)
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    }
}

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let Some(cmd) = argv.first().cloned() else {
        eprintln!(
            "{}",
            include_str!("sync_probe.rs")
                .lines()
                .take(14)
                .collect::<Vec<_>>()
                .join("\n")
        );
        return ExitCode::from(2);
    };
    let args = Args(argv);
    let result = match cmd.as_str() {
        "list" => list(),
        "latency" => latency(&args),
        "record" => record(&args),
        "analyze" => analyze(&args),
        "compare" => compare(&args),
        other => Err(format!("unknown command {other}")),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(1)
        }
    }
}

fn list() -> Result<(), String> {
    println!(
        "camera permission: {:?} (this command never prompts)",
        check_camera_permission()
    );
    let cams = list_cameras().map_err(|e| e.to_string())?;
    println!(
        "{}",
        serde_json::to_string_pretty(&cams).map_err(|e| e.to_string())?
    );
    Ok(())
}

fn guard(args: &Args) -> Result<(), String> {
    if !args.flag("--yes") {
        return Err("this turns the camera on; pass --yes to confirm (max 10 s)".into());
    }
    check_camera_permission()
        .require()
        .map_err(|e| format!("{e} (this tool never prompts)"))
}

fn resolution(args: &Args) -> CameraResolution {
    if args.value("--res") == Some("720") {
        CameraResolution::P720
    } else {
        CameraResolution::P1080
    }
}

fn latency(args: &Args) -> Result<(), String> {
    guard(args)?;
    let secs = args.num("--secs", 5u64).clamp(1, 10);
    let r = probe_delivery_latency(args.value("--device"), resolution(args), secs)
        .map_err(|e| e.to_string())?;
    println!("device: {} ({})", r.device.device_name, r.device.device_id);
    println!(
        "{}",
        serde_json::to_string_pretty(&r).map_err(|e| e.to_string())?
    );
    println!(
        "delivery latency = host clock at the callback minus the sample PTS. If the PTS is the true\n\
         capture time this is pipeline delay only and needs no correction; use `record` + the flash\n\
         test to learn whether the PTS itself is late."
    );
    Ok(())
}

fn record(args: &Args) -> Result<(), String> {
    guard(args)?;
    let out = PathBuf::from(args.value("--out").ok_or("--out DIR is required")?);
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let secs = args.num("--secs", 8u64).clamp(2, 10);
    let comp = args.num("--comp-ms", 0.0f64);
    let cfg = CameraConfig {
        device_id: args.value("--device").map(str::to_string),
        resolution: resolution(args),
        mirror: args.flag("--mirror"),
        latency_compensation_ms: comp,
        ..CameraConfig::default()
    };
    let clock = SessionClock::start();
    let mut rec = CameraRecorder::system(cfg);
    rec.start(TrackContext {
        dir: out.clone(),
        clock: clock.clone(),
    })
    .map_err(|e| e.to_string())?;
    println!("recording {secs} s of camera into {} ...", out.display());
    std::thread::sleep(Duration::from_secs(secs));
    let info = rec.stop(clock.now()).map_err(|e| e.to_string())?;
    let meta = serde_json::json!({
        "origin_host_ns": clock.origin_ns(),
        "latency_compensation_ms": comp,
        "track": info,
    });
    std::fs::write(
        out.join("probe.json"),
        serde_json::to_string_pretty(&meta).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    println!(
        "{}",
        serde_json::to_string_pretty(&info.details).map_err(|e| e.to_string())?
    );
    println!("stats: {:?}", info.stats);
    if let Ok(rep) = cm_camera::mp4::inspect_file(&out.join("camera.mp4")) {
        println!(
            "mp4: moov_first={} timescale={} fragments={} lengths_s={:?}",
            rep.moov_first,
            rep.timescale,
            rep.fragment_starts_s.len(),
            rep.fragment_lengths_s()
        );
    }
    println!(
        "wrote {}/probe.json. Delete the folder when done: it contains your image.",
        out.display()
    );
    Ok(())
}

fn analyze(args: &Args) -> Result<(), String> {
    let camera = PathBuf::from(args.value("--camera").ok_or("--camera FILE required")?);
    let meta: serde_json::Value = serde_json::from_slice(
        &std::fs::read(args.value("--meta").ok_or("--meta probe.json required")?)
            .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let flashes: Vec<FlashEvent> = serde_json::from_slice(
        &std::fs::read(
            args.value("--flashes")
                .ok_or("--flashes flashes.json required")?,
        )
        .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let origin = meta["origin_host_ns"]
        .as_u64()
        .ok_or("probe.json has no origin_host_ns")?;
    let comp = meta["latency_compensation_ms"].as_f64().unwrap_or(0.0);
    let display_ms = args.num("--display-latency-ms", 16.0f64);

    let expected = expected_edges(&flashes, origin);
    let series = luma_series(&camera).map_err(|e| e.to_string())?;
    let detected = detect_edges(&series, 25.0)
        .ok_or("no flash visible in the camera clip (point the camera at the flashing window)")?;
    let offs = match_edges(&expected, &detected, 0.15, 0.6);
    let s = summarize_offsets_ms(&offs)
        .ok_or("no flash matched the log; check the window was in view")?;
    println!(
        "flashes logged: {}, detected in camera: {}, matched: {}",
        expected.len(),
        detected.len(),
        s.count
    );
    println!(
        "camera file time minus flash command time (ms): median {:.1}, p95 {:.1}, min {:.1}, max {:.1}",
        s.median_ms, s.p95_ms, s.min_ms, s.max_ms
    );
    println!(
        "less the display's own latency ({display_ms:.0} ms assumed): camera error as recorded = {:+.1} ms \
         (positive = camera late)",
        s.median_ms - display_ms
    );
    println!(
        "recorded with latency_compensation_ms = {comp}; to correct, set it to {:.0}",
        comp + s.median_ms - display_ms
    );
    println!("NFR-3 needs |error| <= 50 ms; rerun after changing the compensation to confirm.");
    Ok(())
}

fn compare(args: &Args) -> Result<(), String> {
    let load = |flag: &str| -> Result<Vec<_>, String> {
        let p = args.value(flag).ok_or(format!("{flag} FILE required"))?;
        let s = luma_series(Path::new(p)).map_err(|e| e.to_string())?;
        detect_edges(&s, 25.0).ok_or(format!("no flash visible in {p}"))
    };
    let screen = load("--screen")?;
    let camera = load("--camera")?;
    let offs = match_edges(&screen, &camera, 0.15, 0.6);
    let s = summarize_offsets_ms(&offs).ok_or("no flash edges matched between the tracks")?;
    println!(
        "screen edges {}, camera edges {}, matched {}",
        screen.len(),
        camera.len(),
        s.count
    );
    println!(
        "camera minus screen (ms): median {:.1}, p95 {:.1}, min {:.1}, max {:.1} (includes the camera\n\
         seeing the display after its own latency; compare runs, or use `analyze` for ground truth)",
        s.median_ms, s.p95_ms, s.min_ms, s.max_ms
    );
    Ok(())
}
