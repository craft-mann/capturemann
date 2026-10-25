//! Burned-in captions in the exported video (AI-1). They need FFmpeg and a GPU adapter and print
//! SKIPPED without them. The screen is a solid red clip made with FFmpeg's `lavfi` source in a
//! temp `.cmrec`-shaped folder, so any other pixel in a frame is the caption.
#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;

use cm_core::{MediaTime, Timeline};
use cm_media::tools::locate;
use cm_media::{
    export_quick, CaptionCue, CaptionSource, ExportError, ExportOptions, ExportPath, PresetRequest,
    Presets, QuickRequest, Tools,
};
use image::RgbImage;
use serde_json::{json, Value};

const MS: i64 = 120;
const FPS: u32 = 30;
const SECS: u32 = 3;
const RED: [u8; 3] = [0xdc, 0x1e, 0x1e];

fn tools() -> Option<Tools> {
    match locate() {
        Ok(t) if t.ffprobe.is_some() => {}
        _ => {
            eprintln!("SKIPPED: FFmpeg/ffprobe not found (brew install ffmpeg)");
            return None;
        }
    }
    if let Err(e) = cm_render::Compositor::new() {
        eprintln!("SKIPPED: no GPU compositor ({e})");
        return None;
    }
    locate().ok()
}

fn ff(t: &Tools, args: &[&str]) {
    let out = Command::new(&t.ffmpeg)
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .args(args)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "ffmpeg failed: {}\n{}",
        args.join(" "),
        String::from_utf8_lossy(&out.stderr)
    );
}

struct Fixture {
    dir: tempfile::TempDir,
    manifest: Value,
}

/// A 3 s, 1280x720 solid red screen, no audio, with the given extra `edits[0]` fields.
fn fixture(t: &Tools, edits: Value) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("screen.mp4");
    ff(
        t,
        &[
            "-f",
            "lavfi",
            "-i",
            &format!("color=c=0xdc1e1e:s=1280x720,fps={FPS},trim=duration={SECS}"),
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-pix_fmt",
            "yuv420p",
            "-movflags",
            "frag_keyframe+empty_moov",
            out.to_str().unwrap(),
        ],
    );
    let end = i64::from(SECS) * 1000 * MS;
    let screen = json!({"file": "screen.mp4", "size": [1280, 720], "fps": FPS, "codec": "h264",
        "source_size": [1280, 720], "scale": 1.0, "start": 0, "end": end});
    let mut e = json!({"version": 2, "in": 0, "out": end, "speed": 1.0});
    e.as_object_mut()
        .unwrap()
        .extend(edits.as_object().unwrap().clone());
    let manifest = json!({
        "schema": 1, "id": "rec_CAP", "title": "Caption fixture", "created": "2026-10-22T09:00:00Z",
        "tracks": {"screen": screen}, "gaps": [], "edits": [e], "settings": {}
    });
    Fixture { dir, manifest }
}

fn cue(from_s: f64, to_s: f64, text: &str) -> CaptionCue {
    CaptionCue {
        start: MediaTime::from_ticks((from_s * 120_000.0) as i64),
        end: MediaTime::from_ticks((to_s * 120_000.0) as i64),
        text: text.to_owned(),
    }
}

fn source(cues: Vec<CaptionCue>) -> CaptionSource {
    Arc::new(move |_tl: &Timeline| Ok(cues.clone()))
}

fn export(
    t: &Tools,
    f: &Fixture,
    preset: Option<&str>,
    captions: Option<CaptionSource>,
) -> cm_media::QuickOutcome {
    let presets = Presets::builtin();
    export_quick(
        t,
        &QuickRequest {
            bundle_dir: f.dir.path(),
            manifest: &f.manifest,
            prefer_encoder: None,
            path: ExportPath::Auto,
            loudness_lufs: None,
            preset: preset.map(|id| PresetRequest {
                presets: &presets,
                id,
                options: ExportOptions::default(),
            }),
            captions,
        },
        &mut |_| {},
        &|| false,
    )
    .unwrap_or_else(|e: ExportError| panic!("export failed: {}", e.into_cm()))
}

fn frame_at(t: &Tools, video: &Path, at_s: f64, tmp: &Path, tag: &str) -> RgbImage {
    let png = tmp.join(format!("{tag}-{}.png", (at_s * 1000.0) as i64));
    ff(
        t,
        &[
            "-ss",
            &format!("{at_s}"),
            "-i",
            video.to_str().unwrap(),
            "-frames:v",
            "1",
            png.to_str().unwrap(),
        ],
    );
    image::open(&png).unwrap().to_rgb8()
}

/// Pixels that are not the plain red picture (the caption box and its text) in a region.
fn not_red(img: &RgbImage, region: (u32, u32, u32, u32)) -> usize {
    let mut n = 0;
    for y in region.1..region.3.min(img.height()) {
        for x in region.0..region.2.min(img.width()) {
            let p = img.get_pixel(x, y).0;
            if p.iter().zip(RED).any(|(a, b)| a.abs_diff(b) > 40) {
                n += 1;
            }
        }
    }
    n
}

/// AI-1: a caption is burned in while its cue lasts and nowhere else; without a source the export
/// is the plain picture.
#[test]
fn ai1_burned_in_caption_shows_only_during_its_cue() {
    let Some(t) = tools() else { return };
    let f = fixture(&t, json!({}));
    let with = export(
        &t,
        &f,
        None,
        Some(source(vec![cue(0.5, 1.5, "Hello from the video")])),
    );
    let tmp = tempfile::tempdir().unwrap();
    let (w, h) = (1280u32, 720u32);
    // The frames are read before the second export: both write `exports/<title> - quick.mp4`.
    let during = frame_at(&t, &with.path, 1.0, tmp.path(), "with");
    let before = frame_at(&t, &with.path, 0.2, tmp.path(), "with");
    let after = frame_at(&t, &with.path, 2.0, tmp.path(), "with");
    let plain = export(&t, &f, None, None);
    let control = frame_at(&t, &plain.path, 1.0, tmp.path(), "plain");
    assert_eq!(
        not_red(&control, (0, 0, w, h)),
        0,
        "the plain export is red"
    );
    assert!(
        not_red(&during, (0, h * 3 / 4, w, h)) > 500,
        "the caption is in the lower part while its cue lasts"
    );
    assert_eq!(
        not_red(&during, (0, 0, w, h * 3 / 4)),
        0,
        "nothing above the lower quarter"
    );
    assert_eq!(
        not_red(&before, (0, 0, w, h)),
        0,
        "no caption before the cue"
    );
    assert_eq!(not_red(&after, (0, 0, w, h)), 0, "no caption after the cue");
}

/// The source is asked for the cues of the OUTPUT timeline: with a 1 s cut the clip is 2 s long,
/// so a cue made from that duration lands in the last second of the output, not the source.
#[test]
fn ai1_the_cues_are_made_for_the_output_timeline_after_a_cut() {
    let Some(t) = tools() else { return };
    // Cut the second second of the 3 s clip: ticks of 1/120000 s.
    let f = fixture(&t, json!({"cuts": [[120_000, 240_000]]}));
    let src: CaptionSource = Arc::new(|tl: &Timeline| {
        let end = tl.duration();
        assert!(
            (end.as_secs_f64() - 2.0).abs() < 0.05,
            "the timeline is the output's (cut applied): {} s",
            end.as_secs_f64()
        );
        Ok(vec![CaptionCue {
            start: end - MediaTime::from_ticks(120_000),
            end,
            text: "Last second".to_owned(),
        }])
    });
    let out = export(&t, &f, None, Some(src));
    assert!(
        (out.duration_ms as f64 - 2000.0).abs() < 100.0,
        "{} ms",
        out.duration_ms
    );
    let tmp = tempfile::tempdir().unwrap();
    let early = frame_at(&t, &out.path, 0.3, tmp.path(), "cut");
    let late = frame_at(&t, &out.path, 1.5, tmp.path(), "cut");
    assert_eq!(
        not_red(&early, (0, 0, 1280, 720)),
        0,
        "none in the first second"
    );
    assert!(
        not_red(&late, (0, 600, 1280, 720)) > 300,
        "shown in the last second"
    );
}

/// AI-1.AC5: on a 9:16 export the caption keeps out of the bottom 20% and the right 12%.
#[test]
fn ai1_ac5_vertical_export_keeps_the_caption_in_the_safe_area() {
    let Some(t) = tools() else { return };
    let f = fixture(&t, json!({}));
    let text = "A long caption for a vertical video that must stay inside the platform safe area";
    let out = export(
        &t,
        &f,
        Some("vertical-1080"),
        Some(source(vec![cue(0.2, 2.8, text)])),
    );
    let tmp = tempfile::tempdir().unwrap();
    // Read the captioned frame before the plain export overwrites the same file name.
    let with = frame_at(&t, &out.path, 1.0, tmp.path(), "v");
    let plain = export(&t, &f, Some("vertical-1080"), None);
    let base = frame_at(&t, &plain.path, 1.0, tmp.path(), "vplain");
    assert_eq!((with.width(), with.height()), (1080, 1920));
    // Pixels that differ from the same frame without the caption are the caption.
    let changed = |region: (u32, u32, u32, u32)| {
        let mut n = 0;
        for y in region.1..region.3 {
            for x in region.0..region.2 {
                let (a, b) = (with.get_pixel(x, y).0, base.get_pixel(x, y).0);
                if a.iter().zip(b).any(|(p, q)| p.abs_diff(q) > 40) {
                    n += 1;
                }
            }
        }
        n
    };
    assert!(changed((0, 0, 1080, 1920)) > 1000, "the caption is drawn");
    let bottom_20 = 1920 * 80 / 100 + 2;
    let right_12 = 1080 * 88 / 100 + 2;
    assert_eq!(
        changed((0, bottom_20, 1080, 1920)),
        0,
        "the bottom 20% is clear"
    );
    assert_eq!(
        changed((right_12, 0, 1080, 1920)),
        0,
        "the right 12% is clear"
    );
}

/// GIF and WebM presets burn captions too (they share the compositor path).
#[test]
fn ai1_gif_and_webm_get_captions_too() {
    let Some(t) = tools() else { return };
    let f = fixture(&t, json!({}));
    for preset in ["gif", "webm"] {
        let out = export(
            &t,
            &f,
            Some(preset),
            Some(source(vec![cue(0.0, 3.0, "On every frame")])),
        );
        let tmp = tempfile::tempdir().unwrap();
        let frame = frame_at(&t, &out.path, 1.0, tmp.path(), preset);
        let (w, h) = (frame.width(), frame.height());
        assert!(
            not_red(&frame, (0, h * 3 / 4, w, h)) > 50,
            "{preset}: the caption is in the lower part"
        );
    }
}
