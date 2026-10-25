//! M4 gate: the export matches the editor preview frame for frame.
//!
//! For each case (an edit list, written as data), the test builds a synthetic recording
//! (`cm_daemon::fixture`), exports it with the compositor path (`cm_media::export_quick`) and
//! renders the same output frames through the editor's preview (`recedit::source::open` +
//! `PreviewRenderer`, the code the editor window runs, without a window). It compares each sampled
//! frame and prints one table row per frame.
//!
//! Which frame is compared: output frame `n` of the export is output time `n / fps`; the preview
//! is asked for the session time the edit list's timeline (`without_pauses`, as the export and the
//! editor build it) maps that output time to. The export's decoder and the preview's decoder may
//! round a seek to neighbouring source frames, so the preview is also asked for the source frame
//! before and after, and the best of the three counts (the `shift` column; EDT-1/EDT-2 allow
//! "± 1 frame"). A shift that is not 0 is shown, never hidden.
//!
//! # Tolerance
//!
//! The export is H.264 (about 0.9 Mbit/s here) and the preview is the compositor's raw RGBA, so the
//! two are never equal. The no-edit **control** case measures that encode loss alone (mean error
//! about 0.6 of 255, PSNR about 36 dB on the `testsrc` pattern); every other case must stay within
//! [`CONTROL_MARGIN_MAE`] of it and under the absolute limits [`MAX_MAE`], [`MIN_PSNR_DB`] and
//! [`MAX_BAD_FRACTION`] (the share of pixels that differ by more than [`BAD_PIXEL`] in some
//! channel: encoder ringing stays below it, a wrong background, a wrong source frame or a wrong
//! crop does not). Measured values are in `tests/manual/M4-GATE.md` ("Parity numbers") and printed
//! by every run.
//!
//! The fixture's masters are tagged BT.709 like the real ones should be. With untagged masters
//! the preview (FFmpeg's RGBA decode, which assumes BT.601) and the export (NV12 into the
//! compositor, BT.709) disagree by about 7 of 255, which is why `tests/manual/M4-GATE.md` has a
//! step that checks a real recording's colour tags.
//!
//! # Colour
//!
//! Export frames are decoded as BT.709 limited range, which is what a player does with the
//! export's stream tags (the exporter writes them). An export without tags decodes differently in
//! players than in FFmpeg's own default, which is how the exporter's missing BT.709 handling was
//! found.
//!
//! # Camera cases
//!
//! The export and the preview both draw the bubble from the stored layout
//! (`edits[0].layout.camera` and the CAM-3 ranges), so camera cases are compared with nothing
//! masked in [`m4_gate_bubble_layout_matches`]; the main gate masks the bubble boxes only to keep
//! the numbers of the other cases independent of the camera encode.
//!
//! Adding a case is data only: a [`Case`] with the `edits[0]` JSON (times in session seconds, see
//! [`ticks`]) and whether the camera is on. Cursor, zoom, reframe and layout-segment cases are
//! added the same way when those features read their fields from `edits[0]`.
//!
//! Needs FFmpeg with libx264 and a GPU adapter; prints `SKIPPED` and passes without them. The
//! window is not needed, the test never opens one.
//! `CM_PARITY_KEEP=<dir>` keeps every export and every compared pair of pictures as PNG.
#![allow(clippy::unwrap_used, clippy::too_many_lines)]

use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use cm_core::{MediaTime, TICKS_PER_SECOND};
use cm_daemon::fixture::{make_fixture, FixtureSpec};
use cm_desktop::recedit::render::{PreviewRenderer, RenderRequest};
use cm_desktop::recedit::source;
use cm_desktop::recedit::BgSpec;
use cm_library::manifest::RecManifest;
use cm_media::layout::CameraLayout;
use cm_media::layout_plan::{FrameLayout, LayoutPlanner, Placement};
use cm_media::tools::locate;
use cm_media::{export_quick, CaptionTrack, ExportPath, QuickRequest, Tools};
use cm_render::Compositor;
use serde_json::{json, Value};

/// Mean absolute error (RGB, 0 to 255) at most. The control measures about 0.6.
const MAX_MAE: f64 = 3.0;
/// PSNR (dB) at least. The control measures about 36.
const MIN_PSNR_DB: f64 = 28.0;
/// Fraction of pixels at most that differ by more than [`BAD_PIXEL`] in some channel (the
/// shipped cases measure 0.4 to 1.0 %, all of it H.264 ringing on the pattern's hard edges).
const MAX_BAD_FRACTION: f64 = 0.03;
/// A pixel counts as different above this per-channel difference.
const BAD_PIXEL: i32 = 48;
/// A case may exceed the control's mean error by this much.
const CONTROL_MARGIN_MAE: f64 = 1.0;
/// Pixels around a masked bubble box that are left out too.
const MASK_MARGIN: i64 = 8;

fn ticks(secs: f64) -> i64 {
    (secs * TICKS_PER_SECOND as f64).round() as i64
}

/// One parity case: the stored edit list and which output frames to compare.
struct Case {
    name: &'static str,
    camera: bool,
    /// `edits[0]` without `layout` (the fixture's camera layout is added). Session seconds.
    edits: Value,
    /// Where in the output to sample, as fractions of its length.
    at: &'static [f64],
    /// Burned-in caption lines `(start, end, text)` in session seconds (AI-1); empty for none.
    /// They are stored as the recording's edited lines and read back through the daemon's own
    /// burn-in source, so the export and the preview draw what an export request would.
    captions: &'static [(f64, f64, &'static str)],
}

fn background(kind: &str, value: Value) -> Value {
    json!({"kind": kind, "value": value, "padding_pct": 8.0, "radius_px": 12, "shadow": true})
}

fn cases() -> Vec<Case> {
    let t = ticks;
    vec![
        Case {
            name: "control (no edit)",
            camera: false,
            edits: json!({"version": 2, "in": t(0.39), "out": t(12.0), "speed": 1.0}),
            at: &[0.1, 0.5, 0.9],
            captions: &[],
        },
        Case {
            name: "trim",
            camera: false,
            edits: json!({"version": 2, "in": t(2.4), "out": t(9.4), "speed": 1.0}),
            at: &[0.1, 0.5, 0.9],
            captions: &[],
        },
        Case {
            name: "cut+speed (across the pause)",
            camera: false,
            edits: json!({
                "version": 2, "in": t(0.39), "out": t(12.0), "speed": 1.0,
                "cuts": [[t(3.0), t(4.0)]],
                "speeds": [{"from": t(7.2), "to": t(9.0), "factor": 2.0, "keep_audio": false}],
            }),
            at: &[0.1, 0.3, 0.5, 0.7, 0.9],
            captions: &[],
        },
        Case {
            name: "background (gradient, padding, radius, shadow)",
            camera: false,
            edits: json!({
                "version": 2, "in": t(0.39), "out": t(12.0), "speed": 1.0,
                "background": background("gradient", json!(["#4b2bd6", "#9a6bff"])),
            }),
            at: &[0.1, 0.5, 0.9],
            captions: &[],
        },
        Case {
            name: "bubble",
            camera: true,
            edits: json!({"version": 2, "in": t(0.39), "out": t(12.0), "speed": 1.0}),
            at: &[0.15, 0.5, 0.85],
            captions: &[],
        },
        Case {
            name: "everything (trim, cut, speed, background, bubble)",
            camera: true,
            edits: json!({
                "version": 2, "in": t(1.0), "out": t(11.5), "speed": 1.0,
                "cuts": [[t(3.5), t(4.2)]],
                "speeds": [{"from": t(7.2), "to": t(9.0), "factor": 1.5, "keep_audio": false}],
                "background": background("color", json!("#e8e9ec")),
            }),
            at: &[0.1, 0.3, 0.5, 0.7, 0.9],
            captions: &[],
        },
        Case {
            name: "captions (cut, speed, background)",
            camera: false,
            edits: json!({
                "version": 2, "in": t(1.0), "out": t(11.5), "speed": 1.0,
                "cuts": [[t(3.5), t(4.2)]],
                "speeds": [{"from": t(7.2), "to": t(9.0), "factor": 1.5, "keep_audio": false}],
                "background": background("color", json!("#e8e9ec")),
            }),
            // One line before the cut, one that spans it, one inside the sped part, one after.
            at: &[0.08, 0.2, 0.35, 0.5, 0.65, 0.8, 0.92],
            captions: &[
                (1.2, 3.0, "Before the cut"),
                (3.0, 5.5, "A line that runs across the cut"),
                (7.4, 8.8, "Spoken while sped up"),
                (9.5, 11.0, "After everything"),
            ],
        },
    ]
}

/// Skips (returns `None`) without FFmpeg or a GPU.
fn tools() -> Option<Tools> {
    let t = match locate() {
        Ok(t) if t.ffprobe.is_some() => t,
        _ => {
            eprintln!("SKIPPED: FFmpeg/ffprobe not found (brew install ffmpeg)");
            return None;
        }
    };
    if Compositor::new().is_err() {
        eprintln!("SKIPPED: no GPU adapter for the compositor");
        return None;
    }
    Some(t)
}

/// Output frame `n` of `video` as RGB, decoded as a player decodes it (BT.709, limited range).
fn export_frame(t: &Tools, video: &Path, n: u64, tmp: &Path) -> image::RgbImage {
    let out = tmp.join(format!("export-{n}.png"));
    let status = Command::new(&t.ffmpeg)
        .args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
        .arg(video)
        .args([
            "-vf",
            &format!("select=eq(n\\,{n}),scale=in_color_matrix=bt709:in_range=tv"),
        ])
        .args(["-frames:v", "1"])
        .arg(&out)
        .stdin(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success(), "FFmpeg could not decode frame {n}");
    image::open(&out)
        .unwrap_or_else(|e| panic!("frame {n} was not written: {e}"))
        .to_rgb8()
}

/// The preview picture for `session`.
fn preview_frame(
    renderer: &PreviewRenderer,
    seq: u64,
    session: MediaTime,
    bg: &BgSpec,
    layout: FrameLayout,
    captions: Option<Arc<CaptionTrack>>,
) -> image::RgbImage {
    renderer.request(RenderRequest {
        seq,
        session,
        background: bg.clone(),
        layout,
        view: None,
        captions,
    });
    let t0 = Instant::now();
    loop {
        if let Some(r) = renderer.take() {
            if r.seq == seq {
                assert!(!r.plain, "the preview fell back to the plain picture");
                assert!(r.error.is_none(), "preview error: {:?}", r.error);
                return image::DynamicImage::ImageRgba8(r.image).to_rgb8();
            }
        }
        assert!(
            t0.elapsed() < Duration::from_secs(30),
            "the preview did not answer in 30 s"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// A pixel rectangle `[x0, y0, x1, y1)` left out of a comparison.
type Mask = [i64; 4];

fn masked(masks: &[Mask], x: u32, y: u32) -> bool {
    let (x, y) = (i64::from(x), i64::from(y));
    masks
        .iter()
        .any(|m| x >= m[0] && x < m[2] && y >= m[1] && y < m[3])
}

/// Difference between two same-sized pictures.
#[derive(Debug, Clone, Copy)]
struct Diff {
    mae: f64,
    psnr: f64,
    bad: f64,
}

fn diff(a: &image::RgbImage, b: &image::RgbImage, masks: &[Mask]) -> Diff {
    assert_eq!(a.dimensions(), b.dimensions());
    let (mut abs, mut sq, mut bad, mut px) = (0u64, 0u64, 0u64, 0u64);
    for (x, y, p) in a.enumerate_pixels() {
        if masked(masks, x, y) {
            continue;
        }
        let q = b.get_pixel(x, y);
        px += 1;
        let mut worst = 0;
        for c in 0..3 {
            let d = (i32::from(p[c]) - i32::from(q[c])).abs();
            abs += d as u64;
            sq += (d * d) as u64;
            worst = worst.max(d);
        }
        if worst > BAD_PIXEL {
            bad += 1;
        }
    }
    let samples = (px * 3).max(1) as f64;
    let mse = sq as f64 / samples;
    Diff {
        mae: abs as f64 / samples,
        psnr: if mse == 0.0 {
            99.0
        } else {
            10.0 * (255.0f64 * 255.0 / mse).log10()
        },
        bad: bad as f64 / px.max(1) as f64,
    }
}

/// The export's bubble box (the stored layout rect) and the preview's fixed bottom-left one.
fn bubble_masks(layout_rect: &Value, size: (u32, u32)) -> Vec<Mask> {
    let (w, h) = (f64::from(size.0), f64::from(size.1));
    let r: Vec<f64> = layout_rect
        .as_array()
        .map(|a| a.iter().filter_map(Value::as_f64).collect())
        .unwrap_or_default();
    let mut masks = Vec::new();
    if let [x, y, bw, bh] = r[..] {
        masks.push([
            (x * w) as i64 - MASK_MARGIN,
            (y * h) as i64 - MASK_MARGIN,
            ((x + bw) * w) as i64 + MASK_MARGIN,
            ((y + bh) * h) as i64 + MASK_MARGIN,
        ]);
    }
    // `Bubble::bottom_left()`: 22 % of the short side, centred on the bottom left corner.
    let side = (0.22 * w.min(h)) as i64 + 2 * MASK_MARGIN;
    masks.push([0, size.1 as i64 - side, side, size.1 as i64]);
    masks
}

/// One compared frame.
struct Row {
    label: String,
    shift: i32,
    diff: Diff,
}

/// Runs a case and returns one row per sampled frame; `strict_bubble` leaves the bubble boxes in.
fn run_case(t: &Tools, case: &Case, strict_bubble: bool) -> Vec<Row> {
    let lib = tempfile::tempdir().unwrap();
    let spec = FixtureSpec {
        secs: 12,
        camera: case.camera,
        audio: false,
        cursor: false,
        ..FixtureSpec::default()
    };
    let fixture = make_fixture(t, lib.path(), &spec).unwrap();
    let manifest_path = fixture.dir.join("manifest.json");
    let mut manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(&manifest_path).unwrap()).unwrap();
    let mut edits = case.edits.clone();
    edits["layout"] = manifest["edits"][0]["layout"].clone();
    let layout_rect = edits["layout"]["camera"]["rect"].clone();
    manifest["edits"] = json!([edits]);
    if !case.captions.is_empty() {
        let lines: Vec<Value> = case
            .captions
            .iter()
            .map(|(a, b, text)| json!({"start": ticks(*a), "end": ticks(*b), "text": text}))
            .collect();
        manifest["captions"] = json!({"enabled": true, "burn_in": true, "lines": lines});
    }
    std::fs::write(&manifest_path, manifest.to_string()).unwrap();
    // The captions both sides draw: the daemon's burn-in source (the stored lines mapped onto the
    // output timeline), as `export_video` would build it.
    let caption_source = if case.captions.is_empty() {
        None
    } else {
        cm_daemon::caption_options::burn_in_source("rec_PARITY", &fixture.dir, &manifest, true)
            .unwrap()
    };

    // The export (compositor path forced; the camera and background come from the edit list).
    let outcome = export_quick(
        t,
        &QuickRequest {
            captions: caption_source.clone(),
            bundle_dir: &fixture.dir,
            manifest: &manifest,
            prefer_encoder: None,
            path: ExportPath::Compositor,
            loudness_lufs: None,
            preset: None,
        },
        &mut |_| {},
        &|| false,
    )
    .unwrap_or_else(|e| panic!("{}: export failed: {}", case.name, e.into_cm()));

    let slug: String = case
        .name
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect();
    let keep = std::env::var_os("CM_PARITY_KEEP").map(std::path::PathBuf::from);
    if let Some(dir) = &keep {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::copy(&outcome.path, dir.join(format!("{slug}.mp4"))).unwrap();
    }

    // The preview's inputs: what the editor derives from the same stored manifest.
    let m = RecManifest::from_json(&manifest.to_string()).unwrap();
    let list = cm_library::rec_edit::load_edits(&m);
    let timeline = list.without_pauses(&m.gaps).timeline();
    let bg = BgSpec::of(&list.background);
    // The same captions on the same output timeline as the export's (pauses cut).
    let caption_track = caption_source
        .as_ref()
        .map(|s| Arc::new(CaptionTrack::new(s, timeline.clone()).unwrap()));
    let fps = f64::from(spec.fps);
    let frame_ticks = (TICKS_PER_SECOND as f64 / fps).round() as i64;
    let duration_s = timeline.duration().ticks() as f64 / TICKS_PER_SECOND as f64;

    let (src, why) = source::open(&fixture.dir, &m);
    assert!(why.is_none(), "{why:?}");
    let renderer = PreviewRenderer::spawn(src);
    // The camera layout the editor derives from the same stored manifest (CAM-3).
    let planner = LayoutPlanner::new(
        &list,
        list.rest
            .get("layout")
            .and_then(|l| l.get("camera"))
            .and_then(CameraLayout::from_json),
        renderer.size(),
        Placement::Whole,
    );

    let tmp = tempfile::tempdir().unwrap();
    let mut rows = Vec::new();
    let mut seq = 0;
    for f in case.at {
        let n = (f * duration_s * fps).floor() as u64;
        let out_t =
            MediaTime::from_ticks((n as f64 / fps * TICKS_PER_SECOND as f64).round() as i64);
        let sample = timeline
            .at_output(out_t)
            .unwrap_or_else(|| panic!("{}: output time {out_t:?} is past the end", case.name));
        let exported = export_frame(t, &outcome.path, n, tmp.path());
        let masks = if case.camera && !strict_bubble {
            bubble_masks(&layout_rect, exported.dimensions())
        } else {
            Vec::new()
        };
        let mut best: Option<(i32, Diff, image::RgbImage)> = None;
        for shift in [0i32, -1, 1] {
            seq += 1;
            let session = MediaTime::from_ticks(
                (sample.source.ticks() + i64::from(shift) * frame_ticks).max(0),
            );
            let mut previewed = preview_frame(
                &renderer,
                seq,
                session,
                &bg,
                planner.at(session),
                caption_track.clone(),
            );
            if previewed.dimensions() != exported.dimensions() {
                previewed = image::imageops::resize(
                    &previewed,
                    exported.width(),
                    exported.height(),
                    image::imageops::FilterType::Triangle,
                );
            }
            let d = diff(&exported, &previewed, &masks);
            if best.as_ref().is_none_or(|(_, b, _)| d.mae < b.mae) {
                best = Some((shift, d, previewed));
            }
            // An exact frame needs no neighbours.
            if shift == 0 && d.mae < 1.0 {
                break;
            }
        }
        let (shift, d, previewed) = best.unwrap();
        if let Some(dir) = &keep {
            exported
                .save(dir.join(format!("{slug}-{n}-export.png")))
                .unwrap();
            previewed
                .save(dir.join(format!("{slug}-{n}-preview.png")))
                .unwrap();
        }
        rows.push(Row {
            label: format!(
                "{:<50} frame {n:>4} (output {:>5.2} s, source {:>5.2} s)",
                case.name,
                n as f64 / fps,
                sample.source.ticks() as f64 / TICKS_PER_SECOND as f64
            ),
            shift,
            diff: d,
        });
    }
    rows
}

fn header() {
    eprintln!(
        "\n{:<104} {:>5} {:>7} {:>8} {:>8}\n",
        "case / frame", "shift", "MAE", "PSNR dB", "bad px"
    );
}

fn print(r: &Row) {
    eprintln!(
        "{:<104} {:>5} {:>7.2} {:>8.1} {:>7.2}%",
        r.label,
        r.shift,
        r.diff.mae,
        r.diff.psnr,
        r.diff.bad * 100.0
    );
}

/// The gate. One fixture per case; prints a table and fails if any frame is outside the
/// tolerance.
#[test]
fn m4_gate_export_matches_the_preview_frame_for_frame() {
    let Some(t) = tools() else { return };
    let mut failures = Vec::new();
    let mut control_mae: Option<f64> = None;
    header();
    for case in cases() {
        let rows = run_case(&t, &case, false);
        if control_mae.is_none() {
            // The first case is the control: encode loss alone.
            control_mae = Some(rows.iter().map(|r| r.diff.mae).fold(0.0, f64::max));
        }
        let limit = (control_mae.unwrap() + CONTROL_MARGIN_MAE).min(MAX_MAE);
        for r in &rows {
            print(r);
            let d = r.diff;
            if d.mae > limit
                || d.psnr < MIN_PSNR_DB
                || d.bad > MAX_BAD_FRACTION
                || r.shift.abs() > 1
            {
                failures.push(format!(
                    "{}: MAE {:.2} (max {limit:.2}), PSNR {:.1} (min {MIN_PSNR_DB}), bad {:.2}% (max {:.0}%), shift {}",
                    r.label,
                    d.mae,
                    d.psnr,
                    d.bad * 100.0,
                    MAX_BAD_FRACTION * 100.0,
                    r.shift
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "preview and export differ:\n{}",
        failures.join("\n")
    );
}

/// The camera cases with nothing masked: the preview must draw the bubble the export draws (the
/// stored layout: position, ring, mirror, crop). Fails today because the preview uses a fixed
/// bottom-left bubble; part of the M4 gate once CAM-3 lands.
#[test]
fn m4_gate_bubble_layout_matches() {
    let Some(t) = tools() else { return };
    let mut failures = Vec::new();
    header();
    for case in cases().into_iter().filter(|c| c.camera) {
        for r in run_case(&t, &case, true) {
            print(&r);
            let d = r.diff;
            if d.mae > MAX_MAE || d.psnr < MIN_PSNR_DB || d.bad > MAX_BAD_FRACTION {
                failures.push(format!(
                    "{}: MAE {:.2}, PSNR {:.1}, bad {:.2}%",
                    r.label,
                    d.mae,
                    d.psnr,
                    d.bad * 100.0
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "the bubble differs between preview and export:\n{}",
        failures.join("\n")
    );
}

/// The metric itself: identical pictures are exact, a misplaced block is caught, and a mask
/// leaves a region out.
#[test]
fn diff_metric_catches_a_misplaced_block_and_honours_masks() {
    let a = image::RgbImage::from_pixel(64, 64, image::Rgb([100, 100, 100]));
    let d0 = diff(&a, &a, &[]);
    assert_eq!((d0.mae, d0.bad), (0.0, 0.0));
    let mut b = a.clone();
    for y in 0..16 {
        for x in 0..16 {
            b.put_pixel(x, y, image::Rgb([255, 255, 255]));
        }
    }
    let d1 = diff(&a, &b, &[]);
    assert!(d1.bad > MAX_BAD_FRACTION, "{d1:?}");
    assert!(d1.mae > MAX_MAE / 4.0, "{d1:?}");
    let d2 = diff(&a, &b, &[[0, 0, 16, 16]]);
    assert_eq!((d2.mae, d2.bad), (0.0, 0.0));
}
