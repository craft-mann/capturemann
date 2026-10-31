//! A recording whose screen track is VP9 in a fragmented MP4 (what the Linux recorder writes when no
//! hardware encoder is usable, D-R2) must be probed, decoded and exported. FFmpeg-gated: prints
//! SKIPPED without an FFmpeg that has `libvpx-vp9` (to make the fixture), or without a GPU adapter
//! for the compositor (software adapters such as llvmpipe count).
//!
//! The export goes through the host's encoder policy: on Linux it must be `h264_vaapi` (only with a
//! render node) or `libvpx-vp9` in MP4, never `libx264`; elsewhere the usual H.264 choice.
#![allow(clippy::unwrap_used)]

use std::process::{Command, Stdio};

use cm_media::tools::{locate, probe_caps, probe_file};
use cm_media::{export_quick, ExportPath, QuickRequest, Tools};
use serde_json::json;

const MS: i64 = 120;
const FPS: u32 = 30;

fn tools() -> Option<Tools> {
    let t = locate().ok().filter(|t| t.ffprobe.is_some());
    let Some(t) = t else {
        eprintln!("SKIPPED: FFmpeg/ffprobe not found");
        return None;
    };
    match probe_caps(&t) {
        Ok(c) if c.libvpx_vp9 && c.aac => Some(t),
        _ => {
            eprintln!("SKIPPED: this FFmpeg has no libvpx-vp9 or aac encoder (fixture needs both)");
            None
        }
    }
}

fn ff(t: &Tools, args: &[&str]) {
    // The bundled FFmpeg has no lavfi: the fixture comes from CM_FIXTURE_FFMPEG or the one on PATH.
    let maker = match std::env::var_os("CM_FIXTURE_FFMPEG") {
        Some(p) => std::path::PathBuf::from(p),
        None if has_path_ffmpeg() => "ffmpeg".into(),
        None => t.ffmpeg.clone(),
    };
    let out = Command::new(maker)
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .args(args)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "fixture ffmpeg failed: {}\n{}",
        args.join(" "),
        String::from_utf8_lossy(&out.stderr)
    );
}

fn has_path_ffmpeg() -> bool {
    Command::new("ffmpeg")
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

#[test]
fn a_vp9_mp4_recording_exports_with_an_encoder_the_host_may_use() {
    let Some(t) = tools() else { return };
    if let Err(e) = cm_render::Compositor::new() {
        eprintln!("SKIPPED: no GPU compositor ({e})");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let screen = dir.path().join("screen.mp4");
    let secs = 3;
    ff(
        &t,
        &[
            "-f",
            "lavfi",
            "-i",
            &format!("testsrc=size=640x360:rate={FPS}:duration={secs}"),
            "-c:v",
            "libvpx-vp9",
            "-deadline",
            "realtime",
            "-cpu-used",
            "8",
            "-pix_fmt",
            "yuv420p",
            "-movflags",
            "frag_keyframe+empty_moov",
            &screen.to_string_lossy(),
        ],
    );
    let info = probe_file(&t, &screen).unwrap();
    assert_eq!(info.video_codec.as_deref(), Some("vp9"));

    let manifest = json!({
        "schema": 1, "id": "rec_VP9", "title": "VP9 fixture", "created": "2026-10-31T09:00:00Z",
        "tracks": {"screen": {"file": "screen.mp4", "size": [640, 360], "fps": FPS,
            "codec": "vp9", "encoder": "libvpx-vp9", "software": true, "cursor_source": "none",
            "source_size": [640, 360], "scale": 1.0, "start": 0, "end": i64::from(secs) * 1000 * MS}},
        "edits": [], "settings": {}
    });
    let out = export_quick(
        &t,
        &QuickRequest {
            captions: None,
            bundle_dir: dir.path(),
            manifest: &manifest,
            prefer_encoder: None,
            path: ExportPath::Auto,
            loudness_lufs: None,
            preset: None,
        },
        &mut |_| {},
        &|| false,
    )
    .unwrap_or_else(|e| panic!("export failed: {}", e.into_cm()));

    // The job says which encoder wrote the file, and the file agrees.
    let probed = probe_file(&t, &out.path).unwrap();
    assert!(
        (probed.duration_s.unwrap() - f64::from(secs)).abs() < 0.3,
        "{probed:?}"
    );
    assert_eq!(out.settings["video"]["encoder"], out.encoder.as_str());
    let developer_override = std::env::var_os("CM_EXPORT_ALLOW_GPL").is_some_and(|v| v == "1");
    if std::env::consts::OS == "linux" && !developer_override {
        assert!(out.lgpl_safe);
        assert_ne!(out.encoder, "libx264");
        match out.encoder.as_str() {
            "h264_vaapi" => assert_eq!(probed.video_codec.as_deref(), Some("h264")),
            "libvpx-vp9" => {
                assert_eq!(probed.video_codec.as_deref(), Some("vp9"));
                assert_eq!(out.settings["video"]["codec"], "vp9");
                assert_eq!(out.settings["video"]["software"], true);
            }
            other => panic!("unexpected encoder {other}"),
        }
    }
    eprintln!(
        "vp9 export: encoder {} codec {:?} {} bytes",
        out.encoder, probed.video_codec, out.size_bytes
    );
}
