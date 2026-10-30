//! Zoom regions, the reframe window and the telemetry cursor in the edited export (EDT-4,
//! EDT-5). They need FFmpeg and a GPU adapter and print SKIPPED without them. The fixtures are
//! synthesized with FFmpeg's `lavfi` sources into a temp `.cmrec`-shaped folder; nothing touches
//! a camera, a microphone or the screen.
#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::process::{Command, Stdio};

use cm_media::tools::locate;
use cm_media::{export_quick, ExportError, ExportOptions, PresetRequest, Presets, QuickRequest};
use cm_media::{ExportPath, Tools};
use serde_json::{json, Value};

const MS: i64 = 120;
const FPS: u32 = 30;
const SECS: u32 = 3;
const HEADER: &str = r#"{"type":"session","schema":1,"tick_rate":120000,"clock":"fake","platform":"macos","target":{"kind":"display","display_id":1,"origin":[0.0,0.0],"size":[1280,720],"scale":1.0},"clicks":"available","scroll":"available","move_source":"fake","permission_missing":null}"#;

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
        "fixture ffmpeg failed: {}\n{}",
        args.join(" "),
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Left third red, middle third green, right third blue.
const TRICOLOR: &str = "color=c=0xdc1e1e:s=427x720[a];color=c=0x1ec81e:s=426x720[b];color=c=0x1e1edc:s=427x720[c];[a][b][c]hstack=inputs=3";

struct Fixture {
    dir: tempfile::TempDir,
    manifest: Value,
}

/// A 3 s, 1280x720 screen of `source` (an FFmpeg filter graph) with no pause and no audio.
/// `cursor_separate` marks the cursor as not baked in (EDT-4) and writes `cursor.jsonl`.
fn fixture(t: &Tools, source: &str, cursor_events: Option<&[&str]>, edits: Value) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    let out = d.join("screen.mp4");
    ff(
        t,
        &[
            "-f",
            "lavfi",
            "-i",
            &format!("{source},fps={FPS},trim=duration={SECS}"),
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
    let mut screen = json!({"file": "screen.mp4", "size": [1280, 720], "fps": FPS, "codec": "h264",
        "source_size": [1280, 720], "scale": 1.0, "start": 0, "end": end});
    if let Some(events) = cursor_events {
        screen["cursor"] = json!(false);
        let mut text = String::from(HEADER);
        for e in events {
            text.push('\n');
            text.push_str(e);
        }
        text.push('\n');
        std::fs::write(d.join("cursor.jsonl"), text).unwrap();
    }
    let mut e = json!({"version": 2, "in": 0, "out": end, "speed": 1.0});
    e.as_object_mut()
        .unwrap()
        .extend(edits.as_object().unwrap().clone());
    let manifest = json!({
        "schema": 1, "id": "rec_VIEW", "title": "View fixture", "created": "2026-10-22T09:00:00Z",
        "tracks": {"screen": screen}, "gaps": [], "edits": [e], "settings": {}
    });
    Fixture { dir, manifest }
}

fn export(t: &Tools, f: &Fixture, preset: Option<&str>) -> cm_media::QuickOutcome {
    let presets = Presets::builtin();
    export_quick(
        t,
        &QuickRequest {
            captions: None,
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
        },
        &mut |_| {},
        &|| false,
    )
    .unwrap_or_else(|e: ExportError| panic!("export failed: {}", e.into_cm()))
}

fn frame_at(t: &Tools, video: &Path, at_s: f64, tmp: &Path) -> image::RgbImage {
    let png = tmp.join(format!("f{}.png", (at_s * 1000.0) as i64));
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
    image::open(png).unwrap().to_rgb8()
}

fn near(p: &image::Rgb<u8>, rgb: [u8; 3], tol: i32) -> bool {
    p.0.iter()
        .zip(rgb)
        .all(|(a, b)| (i32::from(*a) - i32::from(b)).abs() <= tol)
}

const RED: [u8; 3] = [0xdc, 0x1e, 0x1e];
const GREEN: [u8; 3] = [0x1e, 0xc8, 0x1e];
const BLUE: [u8; 3] = [0x1e, 0x1e, 0xdc];

fn secs_ticks(s: f64) -> i64 {
    (s * 120_000.0) as i64
}

/// EDT-4.AC2 in the export: a zoom at 0.5 s for 2 s at 2x on the top left shows the left half of
/// the screen, filling the frame; before and after it the whole screen is shown.
#[test]
fn edt4_ac2_a_zoom_region_crops_the_export_and_ends_with_the_ease() {
    let Some(t) = tools() else { return };
    let zoom = json!({"at": secs_ticks(0.5), "length": secs_ticks(2.0), "focus": [0.25, 0.25],
                      "scale": 2.0, "follow_cursor": false});
    let f = fixture(&t, TRICOLOR, None, json!({"zooms": [zoom]}));
    let o = export(&t, &f, None);
    let tmp = tempfile::tempdir().unwrap();
    let at = |s: f64| frame_at(&t, &o.path, s, tmp.path());

    let before = at(0.2);
    assert_eq!(before.dimensions(), (1280, 720));
    assert!(
        near(before.get_pixel(640, 360), GREEN, 40),
        "whole screen: the middle is green"
    );
    assert!(near(before.get_pixel(10, 10), RED, 40));
    assert!(near(before.get_pixel(1270, 10), BLUE, 40));

    // 1.5 s is inside the hold (eases end at 0.9 s and start again at 2.1 s).
    let zoomed = at(1.5);
    assert!(
        near(zoomed.get_pixel(640, 360), RED, 40),
        "the centre is now the red third"
    );
    assert!(near(zoomed.get_pixel(10, 10), RED, 40));
    assert!(
        near(zoomed.get_pixel(1270, 700), GREEN, 40),
        "the right edge is the green third"
    );
    let blue_at = (0..1280).find(|&x| near(zoomed.get_pixel(x, 360), BLUE, 40));
    assert!(
        blue_at.is_none(),
        "the blue third is out of view; blue-ish pixel at x={blue_at:?}: {:?}; encoder {}; row: {:?}",
        blue_at.map(|x| zoomed.get_pixel(x, 360).0),
        o.encoder,
        (0..1280)
            .step_by(80)
            .map(|x| (x, zoomed.get_pixel(x, 360).0))
            .collect::<Vec<_>>()
    );

    let after = at(2.8);
    assert!(
        near(after.get_pixel(640, 360), GREEN, 40),
        "back to the whole screen"
    );
    assert!(near(after.get_pixel(1270, 10), BLUE, 40));
}

/// EDT-5.AC1: a Fixed 9:16 reframe on the vertical preset shows the chosen window at full height,
/// filling the 1080x1920 frame; AC3: the 16:9 export of the same list is unchanged.
#[test]
fn edt5_ac1_ac3_a_fixed_reframe_fills_the_portrait_frame_and_leaves_16_9_alone() {
    let Some(t) = tools() else { return };
    let edits = |focus: f64| {
        json!({"reframe": {"9:16": {"mode": "fixed", "windows": [{"from": 0, "focus": [focus, 0.5]}]}},
               // Padding would show the background; the reframe takes priority over it.
               "background": {"kind": "color", "value": "#7f007f", "padding_pct": 10.0,
                              "radius_px": 12, "shadow": false}})
    };
    let tmp = tempfile::tempdir().unwrap();

    // The window is the middle 405 px of 1280: all green when centered.
    let f = fixture(&t, TRICOLOR, None, edits(0.5));
    let o = export(&t, &f, Some("vertical-1080"));
    let img = frame_at(&t, &o.path, 1.0, tmp.path());
    assert_eq!(img.dimensions(), (1080, 1920));
    for (x, y) in [(2, 2), (1077, 2), (2, 1917), (1077, 1917), (540, 960)] {
        assert!(
            near(img.get_pixel(x, y), GREEN, 40),
            "({x},{y}) is green, no padding or bars"
        );
    }

    // The same window pushed to the left edge shows the red third.
    let f = fixture(&t, TRICOLOR, None, edits(0.0));
    let o = export(&t, &f, Some("vertical-1080"));
    let img = frame_at(&t, &o.path, 1.0, tmp.path());
    assert!(
        near(img.get_pixel(540, 960), RED, 40),
        "the window sits at the left edge"
    );

    // AC3: the 16:9 export of the same edit list shows the whole screen with the padding.
    let o = export(&t, &f, None);
    let img = frame_at(&t, &o.path, 1.0, tmp.path());
    assert_eq!(img.dimensions(), (1280, 720));
    assert!(
        near(img.get_pixel(4, 4), [127, 0, 127], 40),
        "16:9 keeps its background"
    );
}

/// Fit (the default) still letterboxes the screen on the background.
#[test]
fn edt5_fit_keeps_the_letterbox() {
    let Some(t) = tools() else { return };
    let f = fixture(
        &t,
        TRICOLOR,
        None,
        json!({"reframe": {"9:16": {"mode": "fit", "windows": []}},
               "background": {"kind": "color", "value": "#7f007f", "padding_pct": 0.0,
                              "radius_px": 0, "shadow": false}}),
    );
    let o = export(&t, &f, Some("vertical-1080"));
    let tmp = tempfile::tempdir().unwrap();
    let img = frame_at(&t, &o.path, 1.0, tmp.path());
    assert_eq!(img.dimensions(), (1080, 1920));
    assert!(
        near(img.get_pixel(540, 30), [127, 0, 127], 40),
        "bars above the picture"
    );
    assert!(
        near(img.get_pixel(30, 960), RED, 40),
        "the whole screen is there"
    );
}

/// EDT-4.AC1 in the export: with "Record cursor separately" the telemetry cursor is drawn at its
/// position, and twice as big at size 2x.
#[test]
fn edt4_ac1_the_separate_cursor_is_drawn_at_its_telemetry_position() {
    let Some(t) = tools() else { return };
    let events = [
        r#"{"type":"move","t":0,"x":640,"y":360}"#,
        r#"{"type":"move","t":360000,"x":640,"y":360}"#,
    ];
    let gray = "color=c=gray:s=1280x720";
    let tmp = tempfile::tempdir().unwrap();
    let arrow_pixel = |size: f64| {
        let f = fixture(&t, gray, Some(&events), json!({"cursor": {"size": size}}));
        let o = export(&t, &f, None);
        let img = frame_at(&t, &o.path, 1.0, tmp.path());
        // The fallback arrow is white inside; probe a point inside its shaft and one outside.
        let inside = *img.get_pixel(640 + (3.0 * size) as u32, 360 + (8.0 * size) as u32);
        let outside = *img.get_pixel(640 + (40.0 * size) as u32, 360 + (8.0 * size) as u32);
        (inside, outside)
    };
    for size in [1.0, 2.0] {
        let (inside, outside) = arrow_pixel(size);
        assert!(
            near(&inside, [255, 255, 255], 45),
            "{size}x: the arrow is white, got {inside:?}"
        );
        assert!(
            near(&outside, [128, 128, 128], 12),
            "{size}x: gray around it, got {outside:?}"
        );
    }
}

/// A recording whose cursor is baked into the picture gets no second cursor.
#[test]
fn edt4_a_baked_cursor_is_left_alone() {
    let Some(t) = tools() else { return };
    let gray = "color=c=gray:s=1280x720";
    // Telemetry exists, but the screen track does not say the cursor is separate.
    let f = fixture(&t, gray, None, json!({"cursor": {"size": 3.0}}));
    std::fs::write(
        f.dir.path().join("cursor.jsonl"),
        format!("{HEADER}\n{}\n", r#"{"type":"move","t":0,"x":640,"y":360}"#),
    )
    .unwrap();
    let o = export(&t, &f, None);
    let tmp = tempfile::tempdir().unwrap();
    let img = frame_at(&t, &o.path, 1.0, tmp.path());
    assert!(
        near(img.get_pixel(640 + 9, 360 + 24), [128, 128, 128], 12),
        "nothing drawn"
    );
}
