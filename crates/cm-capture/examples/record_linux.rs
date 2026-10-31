//! Linux recording check (L1): records the desktop through the ScreenCast portal + PipeWire into
//! `screen.mp4` (software VP9 in fragmented MP4) and, with `--audio`, the microphone and system
//! audio into `mic.wav` / `system.wav`, all on one session clock. Prints one JSON line.
//!
//! ```text
//! cargo run --release -p cm-capture --example record_linux -- --out /tmp/rec --secs 10 \
//!     --token-file /tmp/token.txt [--fps 30] [--size 1280x720] [--audio]
//! ```
//! The portal shows its consent dialog unless the token file holds a valid restore token.

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("this example runs on Linux only");
}

#[cfg(target_os = "linux")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::time::Duration;

    use cm_audio::{
        pipewire_mic_factory, pipewire_monitor_factory, MicConfig, MicRecorder, SystemAudioConfig,
        SystemAudioRecorder,
    };
    use cm_capture::record::pipewire::{FileTokenStore, PipeWireScreenSource};
    use cm_capture::record::{plan_capture, ScreenRecorder, ScreenSettings};
    use cm_capture::{CaptureTarget, DisplayInfo, PortalPipeWire, Targets};
    use cm_core::{
        PhysicalPoint, PhysicalRect, PhysicalSize, ScaleFactor, SessionClock, TrackContext,
        TrackRecorder,
    };
    use cm_encode::{FfmpegVp9Encoder, VideoSettings};

    let v: Vec<String> = std::env::args().skip(1).collect();
    let get = |f: &str| {
        v.iter()
            .position(|a| a == f)
            .and_then(|i| v.get(i + 1))
            .cloned()
    };
    let secs: u64 = get("--secs").and_then(|s| s.parse().ok()).unwrap_or(10);
    let fps: u32 = get("--fps").and_then(|s| s.parse().ok()).unwrap_or(30);
    let out = std::path::PathBuf::from(get("--out").ok_or("--out <dir> is required")?);
    let audio = v.iter().any(|a| a == "--audio");
    let (w, h) = get("--size")
        .and_then(|s| {
            let (a, b) = s.split_once('x')?;
            Some((a.parse().ok()?, b.parse().ok()?))
        })
        .unwrap_or((1280u32, 720u32));
    std::fs::create_dir_all(&out)?;
    let token = get("--token-file").map_or_else(|| out.join("token.txt"), Into::into);

    // The compositor's dialog picks the real monitor; the plan only needs a size hint.
    let targets = Targets {
        displays: vec![DisplayInfo {
            id: 1,
            name: "portal".into(),
            bounds: PhysicalRect {
                origin: PhysicalPoint { x: 0, y: 0 },
                size: PhysicalSize {
                    width: w,
                    height: h,
                },
            },
            scale: ScaleFactor::new(1.0)?,
            is_primary: true,
            is_builtin: false,
        }],
        windows: vec![],
    };
    let mut settings = ScreenSettings::new(CaptureTarget::Display { display_id: 1 });
    settings.video = VideoSettings {
        fps,
        ..Default::default()
    };
    let plan = plan_capture(&settings, &targets)?;
    let source = PipeWireScreenSource::new(PortalPipeWire::new(), FileTokenStore(token));
    let mut screen = ScreenRecorder::new(plan, source, FfmpegVp9Encoder::create);

    let clock = SessionClock::start();
    let ctx = TrackContext {
        dir: out.clone(),
        clock: clock.clone(),
    };
    let mut tracks: Vec<Box<dyn TrackRecorder>> = Vec::new();
    if audio {
        tracks.push(Box::new(MicRecorder::with_factory(
            MicConfig::default(),
            pipewire_mic_factory(),
        )));
        tracks.push(Box::new(SystemAudioRecorder::with_factory(
            SystemAudioConfig::default(),
            pipewire_monitor_factory(),
        )));
    }
    screen.start(ctx.clone())?;
    for t in &mut tracks {
        t.start(ctx.clone())?;
    }
    eprintln!("recording {secs} s ...");
    std::thread::sleep(Duration::from_secs(secs));
    let at = clock.now();
    let mut infos = vec![screen.stop(at)?];
    for t in &mut tracks {
        infos.push(t.stop(at)?);
    }
    println!(
        "{}",
        serde_json::json!({ "origin_host_ns": clock.origin_ns(), "tracks": infos })
    );
    Ok(())
}
