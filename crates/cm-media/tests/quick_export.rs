//! FFmpeg-gated integration tests of the quick exporter (EXP-1.AC3, EXP-4.AC4, REC-1, REC-4,
//! AUD-4, CAM-1, CAM-2.AC3). Skipped (with a message) when no FFmpeg is installed.
//!
//! The fixtures are synthesized with FFmpeg's `lavfi` sources into temp folders shaped like a
//! `.cmrec` bundle: a 1280x720@30 fragmented `screen.mp4` whose first frame is 390 ms after the
//! session origin, a `camera.mp4` starting 190 ms later, a mono `mic.denoised.wav` starting 290 ms
//! *before* the screen, and a stereo `system.wav`. Nothing touches a camera, microphone or screen.
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};

use cm_media::tools::{locate, probe_file};
use cm_media::{export_quick, ExportError, QuickRequest, Tools};
use serde_json::{json, Value};

const MS: i64 = 120; // ticks per millisecond
const FPS: u32 = 30;

fn tools() -> Option<Tools> {
    match locate() {
        Ok(t) if t.ffprobe.is_some() => Some(t),
        _ => {
            eprintln!("SKIPPED: FFmpeg/ffprobe not found (brew install ffmpeg)");
            None
        }
    }
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

/// Fixture timing (seconds): the visual flash on screen, the camera's colour change and the beep
/// all happen at known times on the SESSION clock.
struct Fixture {
    dir: tempfile::TempDir,
    manifest: Value,
    /// Screen file length.
    secs: f64,
}

const SCREEN_START_MS: i64 = 390;
const CAMERA_START_MS: i64 = 580;
const MIC_START_MS: i64 = 100;
/// The flash is 1.0 s into the screen file = session 1.39 s.
const FLASH_FILE_S: f64 = 1.0;
/// The camera turns blue 1.0 s into the camera file.
const CAMERA_EVENT_FILE_S: f64 = 1.0;

impl Fixture {
    fn path(&self) -> &Path {
        self.dir.path()
    }
}

/// `screen_secs` long. With `with_camera`/`with_mic`/`with_system` the optional tracks exist.
fn fixture(
    t: &Tools,
    screen_secs: u32,
    size: &str,
    with_camera: bool,
    with_audio: bool,
) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    let s = |n: &str| d.join(n).to_string_lossy().into_owned();
    let secs = f64::from(screen_secs);
    // Screen: testsrc with a white flash from FLASH_FILE_S for 0.2 s; PTS starts at 0.39 s.
    let flash = format!(
        "drawbox=x=0:y=0:w=iw:h=ih:color=white:t=fill:enable='between(t,{FLASH_FILE_S},{})'",
        FLASH_FILE_S + 0.2
    );
    ff(
        t,
        &[
            "-f",
            "lavfi",
            "-i",
            &format!("testsrc=size={size}:rate={FPS}:duration={screen_secs}"),
            "-vf",
            &flash,
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-pix_fmt",
            "yuv420p",
            "-output_ts_offset",
            "0.39",
            "-movflags",
            "frag_keyframe+empty_moov",
            &s("screen.mp4"),
        ],
    );
    let cam_secs = screen_secs; // the camera file is as long as the screen
    if with_camera {
        // Left half red, right half green (an asymmetric image shows mirroring); blue from 1 s.
        let blue = format!(
            "drawbox=x=0:y=0:w=iw:h=ih:color=blue:t=fill:enable='gte(t,{CAMERA_EVENT_FILE_S})'"
        );
        ff(
            t,
            &[
                "-f",
                "lavfi",
                "-i",
                &format!("color=c=red:s=640x720:r={FPS}:d={cam_secs}"),
                "-f",
                "lavfi",
                "-i",
                &format!("color=c=green:s=640x720:r={FPS}:d={cam_secs}"),
                "-filter_complex",
                &format!("[0:v][1:v]hstack,{blue}"),
                "-c:v",
                "libx264",
                "-preset",
                "ultrafast",
                "-pix_fmt",
                "yuv420p",
                "-output_ts_offset",
                "0.58",
                "-movflags",
                "frag_keyframe+empty_moov",
                &s("camera.mp4"),
            ],
        );
    }
    if with_audio {
        // Mic: silence with a 1 kHz beep at session 1.39 s = mic file time 1.29 s (mic starts
        // at 0.1 s); 0.5 s longer than the screen like a real stop.
        let mic_len = secs + 0.5;
        let beep_file_ms = (SCREEN_START_MS - MIC_START_MS) as f64 + FLASH_FILE_S * 1000.0;
        ff(
            t,
            &[
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=1000:duration=0.2:sample_rate=48000",
                "-af",
                &format!(
                    "volume=6,adelay={}:all=1,apad=whole_dur={mic_len},aformat=sample_fmts=flt:channel_layouts=mono",
                    beep_file_ms as i64
                ),
                "-c:a",
                "pcm_f32le",
                &s("mic.denoised.wav"),
            ],
        );
        ff(
            t,
            &[
                "-f",
                "lavfi",
                "-i",
                &format!(
                    "sine=frequency=300:duration={}:sample_rate=48000",
                    secs + 0.5
                ),
                "-af",
                "volume=0.1,aformat=sample_fmts=flt:channel_layouts=stereo",
                "-c:a",
                "pcm_f32le",
                &s("system.wav"),
            ],
        );
    }
    let mut tracks = json!({
        "screen": {"file": "screen.mp4", "size": [1280, 720], "fps": FPS, "codec": "h264",
                   "source_size": [1280, 720], "scale": 1.0,
                   "start": SCREEN_START_MS * MS,
                   // A 1 s pause inside the recording: session span = file length + 1 s.
                   "end": SCREEN_START_MS * MS + (screen_secs as i64 + 1) * 1000 * MS},
    });
    if with_camera {
        tracks["camera"] = json!({"file": "camera.mp4", "size": [1280, 720], "fps": FPS,
            "mirror": true, "start": CAMERA_START_MS * MS, "end": (CAMERA_START_MS + 1000) * MS});
    }
    if with_audio {
        tracks["mic"] = json!({"file": "mic.denoised.wav", "rate": 48000, "channels": 1,
            "start": MIC_START_MS * MS, "end": 3000 * MS});
        tracks["system"] = json!({"file": "system.wav", "rate": 48000, "channels": 2,
            "start": SCREEN_START_MS * MS, "end": 3000 * MS});
    }
    let manifest = json!({
        "schema": 1, "id": "rec_FIXTURE", "title": "Fixture: 1/2", "created": "2026-10-21T09:00:00Z",
        "tracks": tracks,
        // The pause is after every start and after the flash; it only affects durations.
        "gaps": [{"from": (SCREEN_START_MS + 3000) * MS, "to": (SCREEN_START_MS + 4000) * MS}],
        "edits": [{"in": 0, "out": 1, "speed": 1.0, "layout": {"camera": {
            "visible": true, "shape": "circle", "rect": [0.70, 0.55, 0.20, 0.20], "mirror": true}}}],
        "settings": {}
    });
    Fixture {
        dir,
        manifest,
        secs,
    }
}

fn run_export(t: &Tools, f: &Fixture) -> cm_media::QuickOutcome {
    export_quick(
        t,
        &QuickRequest {
            captions: None,
            bundle_dir: f.path(),
            manifest: &f.manifest,
            prefer_encoder: None,
            path: cm_media::ExportPath::Auto,
            loudness_lufs: None,
            preset: None,
        },
        &mut |_| {},
        &|| false,
    )
    .unwrap_or_else(|e| panic!("export failed: {}", e.into_cm()))
}

/// Frame at `t_s` as RGB (decoded by FFmpeg to PNG, read with the `image` crate).
fn frame_at(t: &Tools, video: &Path, t_s: f64, tmp: &Path) -> image::RgbImage {
    let out = tmp.join(format!("frame-{t_s}.png"));
    ff(
        t,
        &[
            "-ss",
            &t_s.to_string(),
            "-i",
            &video.to_string_lossy(),
            "-frames:v",
            "1",
            &out.to_string_lossy(),
        ],
    );
    image::open(&out).unwrap().to_rgb8()
}

/// Per-frame mean luma of a region (`crop=w:h:x:y`) as bytes via a raw pipe.
fn region_means(t: &Tools, video: &Path, crop: &str) -> Vec<u8> {
    let out = Command::new(&t.ffmpeg)
        .args(["-hide_banner", "-loglevel", "error", "-i"])
        .arg(video)
        .args([
            "-vf",
            &format!("crop={crop},scale=1:1:flags=area,format=gray"),
            "-f",
            "rawvideo",
            "-",
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    out.stdout
}

/// Output PCM as mono f32 at 48 kHz.
#[allow(clippy::chunks_exact_to_as_chunks)]
fn pcm(t: &Tools, audio: &Path) -> Vec<f32> {
    let out = Command::new(&t.ffmpeg)
        .args(["-hide_banner", "-loglevel", "error", "-i"])
        .arg(audio)
        .args(["-vn", "-ac", "1", "-ar", "48000", "-f", "f32le", "-"])
        .output()
        .unwrap();
    assert!(out.status.success());
    out.stdout
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect()
}

fn near(p: &image::Rgb<u8>, rgb: [u8; 3], tol: i32) -> bool {
    p.0.iter()
        .zip(rgb)
        .all(|(a, b)| (i32::from(*a) - i32::from(b)).abs() <= tol)
}

fn ffprobe_json(t: &Tools, file: &Path) -> Value {
    let out = Command::new(t.ffprobe.as_ref().unwrap())
        .args([
            "-v",
            "error",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(file)
        .output()
        .unwrap();
    serde_json::from_slice(&out.stdout).unwrap()
}

/// Top-level MP4 box order, e.g. `["ftyp", "moov", "mdat"]`.
fn boxes(file: &Path) -> Vec<String> {
    let b = std::fs::read(file).unwrap();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i + 8 <= b.len() {
        let size = u32::from_be_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]) as usize;
        out.push(String::from_utf8_lossy(&b[i + 4..i + 8]).into_owned());
        if size < 8 {
            break;
        }
        i += size;
    }
    out
}

// ---- tests ------------------------------------------------------------------------------------

/// EXP-1.AC3: the output passes ffprobe checks: H.264 High, yuv420p, constant 30 fps, the screen
/// master's even size, AAC 48 kHz, fast start, duration = the screen file's (± 1 frame), written
/// as `exports/<title> - quick.mp4` with the entry fields the manifest needs.
#[test]
fn exp1_ac3_output_passes_ffprobe_checks() {
    let Some(t) = tools() else { return };
    let f = fixture(&t, 4, "1280x720", true, true);
    let o = run_export(&t, &f);
    assert_eq!(o.file, "exports/Fixture- 1-2 - quick.mp4");
    assert!(o.path.is_file() && o.size_bytes > 10_000);
    assert!(
        !f.path()
            .join("exports")
            .join("Fixture- 1-2 - quick.mp4.part")
            .exists(),
        "atomic rename: no temp file"
    );
    let work_left: Vec<_> = std::fs::read_dir(f.path().join("exports"))
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        work_left,
        ["Fixture- 1-2 - quick.mp4"],
        "work folder cleaned"
    );

    let j = ffprobe_json(&t, &o.path);
    let streams = j["streams"].as_array().unwrap();
    let v = streams.iter().find(|s| s["codec_type"] == "video").unwrap();
    let a = streams.iter().find(|s| s["codec_type"] == "audio").unwrap();
    assert_eq!(v["codec_name"], "h264");
    assert_eq!(v["profile"], "High");
    assert_eq!(v["pix_fmt"], "yuv420p");
    assert_eq!(
        (v["width"].as_u64(), v["height"].as_u64()),
        (Some(1280), Some(720))
    );
    assert_eq!(v["r_frame_rate"], "30/1");
    assert_eq!(v["avg_frame_rate"], "30/1");
    assert_eq!(a["codec_name"], "aac");
    assert_eq!(a["sample_rate"], "48000");
    assert_eq!(a["channels"], 2, "mic + stereo system audio mix to stereo");
    let dur: f64 = j["format"]["duration"].as_str().unwrap().parse().unwrap();
    assert!((dur - f.secs).abs() <= 1.0 / 30.0 + 0.02, "duration {dur}");
    assert!((o.duration_ms as f64 / 1000.0 - dur).abs() < 0.01);
    let order = boxes(&o.path);
    let (moov, mdat) = (
        order.iter().position(|b| b == "moov").unwrap(),
        order.iter().position(|b| b == "mdat").unwrap(),
    );
    assert!(moov < mdat, "fast start: {order:?}");
    // Frame count = duration * fps, constant frame rate.
    let frames = region_means(&t, &o.path, "16:16:0:0").len();
    assert!((frames as i64 - 120).abs() <= 1, "{frames} frames");
    // Entry fields and the encoder used.
    let e = o.to_entry("2026-10-21T10:00:00Z");
    assert_eq!(e["preset"], "quick");
    assert!(e["encoder"] == "h264_videotoolbox" || e["encoder"] == "libx264");
    assert_eq!(e["settings"]["video"]["profile"], "high");
    assert_eq!(e["settings"]["audio"]["channels"], 2);
    assert_eq!(e["settings"]["audio"]["loudness_normalization"], false);
}

/// REC-1 / AUD-4 / REC-4: tracks line up on the session clock. The screen flash (file time 1.0 s)
/// and the mic beep (session 1.39 s, in a file that started 290 ms before the screen) must both
/// land at output time 1.0 s, whatever offsets the tracks started with; the camera (which started
/// 190 ms after the screen) changes colour at output time 1.19 s. Measured offsets are printed.
#[test]
fn rec1_tracks_are_aligned_on_the_session_clock() {
    let Some(t) = tools() else { return };
    let f = fixture(&t, 4, "1280x720", true, true);
    let o = run_export(&t, &f);

    // Video: first frame whose top-left region is white.
    let means = region_means(&t, &o.path, "160:90:0:0");
    let flash = means.iter().position(|m| *m > 235).expect("flash found");
    let flash_t = flash as f64 / f64::from(FPS);
    // Audio: first sample above the system-tone level.
    let samples = pcm(&t, &o.path);
    let beep = samples
        .iter()
        .position(|s| s.abs() > 0.3)
        .expect("beep found");
    let beep_t = beep as f64 / 48_000.0;
    eprintln!(
        "alignment: flash at {flash_t:.3} s, beep at {beep_t:.3} s, A/V offset {:+.1} ms",
        (beep_t - flash_t) * 1000.0
    );
    assert!(
        (flash_t - FLASH_FILE_S).abs() <= 1.5 / f64::from(FPS),
        "flash {flash_t}"
    );
    assert!((beep_t - FLASH_FILE_S).abs() <= 0.030, "beep {beep_t}");
    assert!((beep_t - flash_t).abs() <= 0.050, "A/V offset");

    // Camera: the bubble centre turns blue at 0.19 + 1.0 s.
    // Bubble box: x 896..1152, y 396..652 (rect [0.70, 0.55, 0.20, 0.20] on 1280x720).
    let centre = region_means(&t, &o.path, "8:8:1020:520");
    // Mean luma of red/green is >> blue's; find the drop.
    let first = centre[10];
    let drop = centre
        .iter()
        .position(|m| i32::from(*m) < i32::from(first) - 25)
        .expect("camera colour change found");
    let drop_t = drop as f64 / f64::from(FPS);
    let expect = (CAMERA_START_MS - SCREEN_START_MS) as f64 / 1000.0 + CAMERA_EVENT_FILE_S;
    eprintln!("alignment: camera event at {drop_t:.3} s (expected {expect:.3} s)");
    assert!(
        (drop_t - expect).abs() <= 2.0 / f64::from(FPS),
        "camera {drop_t} vs {expect}"
    );
}

/// CAM-1 / DESIGN 5.4 / CAM-2.AC3: the bubble is a circle at the layout rect with a 4 px `rec`
/// ring; the camera pixels sit inside, the screen shows through outside the circle; with
/// `mirror: true` the stored (never flipped) camera is flipped for the export.
#[test]
fn cam1_bubble_circle_ring_and_mirror() {
    let Some(t) = tools() else { return };
    let f = fixture(&t, 4, "1280x720", true, false);
    let tmp = tempfile::tempdir().unwrap();
    let o = run_export(&t, &f);
    let frame = frame_at(&t, &o.path, 0.5, tmp.path());
    // Bubble box x 896..1152, y 396..652; centre (1024, 524); outer radius 128.
    // Mirror on: the camera's right half (green) is on the left.
    assert!(
        near(frame.get_pixel(960, 524), [0, 128, 0], 70),
        "left {:?}",
        frame.get_pixel(960, 524)
    );
    assert!(
        near(frame.get_pixel(1088, 524), [255, 0, 0], 70),
        "right {:?}",
        frame.get_pixel(1088, 524)
    );
    // Ring: the top of the circle, 2 px under the edge, is rec red (#FF4F3A), 4 px wide.
    let ring = frame.get_pixel(1024, 398);
    assert!(near(ring, [0xFF, 0x4F, 0x3A], 60), "ring {ring:?}");
    let inside = frame.get_pixel(1024, 406);
    assert!(
        !near(inside, [0xFF, 0x4F, 0x3A], 40),
        "camera starts after the ring: {inside:?}"
    );
    // Outside the circle but inside the bubble box: the screen (testsrc), not camera pixels.
    let no_cam = {
        let mut f2 = fixture(&t, 4, "1280x720", false, false);
        f2.manifest["tracks"]
            .as_object_mut()
            .unwrap()
            .remove("camera");
        let o2 = run_export(&t, &f2);
        frame_at(&t, &o2.path, 0.5, tmp.path())
    };
    for (x, y) in [(900, 400), (1148, 400), (900, 648), (1148, 648)] {
        let (a, b) = (frame.get_pixel(x, y), no_cam.get_pixel(x, y));
        assert!(
            a.0.iter()
                .zip(b.0)
                .all(|(p, q)| (i32::from(*p) - i32::from(q)).abs() <= 24),
            "corner ({x},{y}) shows the screen: {a:?} vs {b:?}"
        );
    }
    // The ring is a circle: on the diagonal at the outer radius (128 * 0.707 = 90) it is red too.
    let d = frame.get_pixel(1024 + 88, 524 - 88);
    assert!(
        near(d, [0xFF, 0x4F, 0x3A], 90)
            || near(frame.get_pixel(1024 + 86, 524 - 86), [0xFF, 0x4F, 0x3A], 90),
        "diagonal ring {d:?}"
    );

    // Mirror off flips it: left red, right green.
    let mut m = f.manifest.clone();
    m["edits"][0]["layout"]["camera"]["mirror"] = json!(false);
    let flipped = export_quick(
        &t,
        &QuickRequest {
            captions: None,
            bundle_dir: f.path(),
            manifest: &m,
            prefer_encoder: None,
            path: cm_media::ExportPath::Auto,
            loudness_lufs: None,
            preset: None,
        },
        &mut |_| {},
        &|| false,
    )
    .unwrap();
    let frame = frame_at(&t, &flipped.path, 0.5, tmp.path());
    assert!(
        near(frame.get_pixel(960, 524), [255, 0, 0], 70),
        "left {:?}",
        frame.get_pixel(960, 524)
    );
    assert!(
        near(frame.get_pixel(1088, 524), [0, 128, 0], 70),
        "right {:?}",
        frame.get_pixel(1088, 524)
    );
}

/// A rounded bubble keeps square-ish corners (4:3 box).
#[test]
fn cam1_rounded_bubble_is_a_rounded_rectangle() {
    let Some(t) = tools() else { return };
    let mut f = fixture(&t, 3, "1280x720", true, false);
    f.manifest["edits"][0]["layout"]["camera"]["shape"] = json!("rounded");
    let tmp = tempfile::tempdir().unwrap();
    let o = run_export(&t, &f);
    let frame = frame_at(&t, &o.path, 0.5, tmp.path());
    // 256 x 192 box at (896, 396): a point just inside the corner is camera (not the circle's bare
    // corner), and the ring runs along the straight top edge.
    assert!(
        near(frame.get_pixel(906, 406), [0, 128, 0], 90),
        "{:?}",
        frame.get_pixel(906, 406)
    );
    assert!(near(frame.get_pixel(1024, 398), [0xFF, 0x4F, 0x3A], 60));
}

/// Camera-less export (no camera track) works: video + audio, no bubble.
#[test]
fn camera_less_export_works() {
    let Some(t) = tools() else { return };
    let f = fixture(&t, 3, "1280x720", false, true);
    let o = run_export(&t, &f);
    let info = probe_file(&t, &o.path).unwrap();
    assert!(info.has_video && info.has_audio);
    assert!((info.duration_s.unwrap() - 3.0).abs() < 0.1);
    assert!(o.settings["camera"].is_null());
}

/// No audio tracks at all: video only.
#[test]
fn video_only_export_has_no_audio_stream() {
    let Some(t) = tools() else { return };
    let f = fixture(&t, 2, "1280x720", true, false);
    let o = run_export(&t, &f);
    let info = probe_file(&t, &o.path).unwrap();
    assert!(info.has_video && !info.has_audio);
}

/// A hidden bubble (`layout.camera.visible: false`) exports screen and audio only.
#[test]
fn hidden_bubble_exports_screen_only() {
    let Some(t) = tools() else { return };
    let mut f = fixture(&t, 2, "1280x720", true, true);
    f.manifest["edits"][0]["layout"]["camera"]["visible"] = json!(false);
    let o = run_export(&t, &f);
    assert!(o.settings["camera"].is_null());
    assert!(o.settings["notes"].to_string().contains("hidden"));
}

/// Odd and large masters: an odd size is made even and the export still plays.
#[test]
fn odd_master_size_becomes_even() {
    let Some(t) = tools() else { return };
    let mut f = fixture(&t, 2, "1280x720", false, false);
    f.manifest["tracks"]["screen"]["size"] = json!([1281, 721]);
    let o = run_export(&t, &f);
    let info = probe_file(&t, &o.path).unwrap();
    // The probed master (1280x720) wins over the manifest.
    assert_eq!((info.width, info.height), (Some(1280), Some(720)));
}

/// EXP-4.AC4: cancelling kills FFmpeg and deletes the partial output (and the work folder).
#[test]
fn exp4_ac4_cancel_deletes_partial_output() {
    let Some(t) = tools() else { return };
    // A 1080p, 60 s screen keeps the encoder busy for much longer than the cancel delay.
    let f = fixture(&t, 60, "1920x1080", true, true);
    let mut m = f.manifest.clone();
    m["tracks"]["screen"]["size"] = json!([1920, 1080]);
    m["tracks"]["screen"]["source_size"] = json!([1920, 1080]);
    let started = std::time::Instant::now();
    let flag = AtomicBool::new(false);
    let r = export_quick(
        &t,
        &QuickRequest {
            captions: None,
            bundle_dir: f.path(),
            manifest: &m,
            prefer_encoder: None,
            path: cm_media::ExportPath::Auto,
            loudness_lufs: None,
            preset: None,
        },
        &mut |_| {},
        &|| {
            if started.elapsed() > std::time::Duration::from_millis(700) {
                flag.store(true, Ordering::SeqCst);
            }
            flag.load(Ordering::SeqCst)
        },
    );
    assert!(
        matches!(r, Err(ExportError::Cancelled)),
        "{:?}",
        r.map(|o| o.file)
    );
    assert!(started.elapsed() < std::time::Duration::from_secs(20));
    let left: Vec<PathBuf> = std::fs::read_dir(f.path().join("exports"))
        .map(|d| d.filter_map(Result::ok).map(|e| e.path()).collect())
        .unwrap_or_default();
    assert!(left.is_empty(), "nothing left in exports/: {left:?}");
    assert!(f.path().join("screen.mp4").is_file(), "masters untouched");
}

/// A missing ffmpeg is `not_found` with the install hint, before anything is written.
#[test]
fn missing_ffmpeg_maps_to_not_found() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("screen.mp4"), b"x").unwrap();
    let manifest = json!({"schema": 1, "title": "T", "tracks": {"screen": {
        "file": "screen.mp4", "size": [1280, 720], "fps": 30, "start": 0, "end": 120000}}});
    let tools = Tools {
        ffmpeg: "/nonexistent/ffmpeg".into(),
        ffprobe: None,
    };
    let r = export_quick(
        &tools,
        &QuickRequest {
            captions: None,
            bundle_dir: dir.path(),
            manifest: &manifest,
            prefer_encoder: None,
            path: cm_media::ExportPath::Auto,
            loudness_lufs: None,
            preset: None,
        },
        &mut |_| {},
        &|| false,
    );
    let e = match r {
        Err(e) => e.into_cm(),
        Ok(_) => panic!("must fail"),
    };
    assert_eq!(e.code(), cm_core::ErrorCode::NotFound);
    assert!(e.to_string().contains("FFmpeg was not found"), "{e}");
    assert!(e.to_string().contains("brew install ffmpeg"), "{e}");
    assert!(!dir.path().join("exports").join("T - quick.mp4").exists());
}

/// A missing screen file is `not_found` too (the bundle was moved or damaged).
#[test]
fn missing_screen_file_is_not_found() {
    let Some(t) = tools() else { return };
    let dir = tempfile::tempdir().unwrap();
    let manifest = json!({"schema": 1, "title": "T", "tracks": {"screen": {
        "file": "screen.mp4", "size": [1280, 720], "fps": 30, "start": 0, "end": 120000}}});
    let r = export_quick(
        &t,
        &QuickRequest {
            captions: None,
            bundle_dir: dir.path(),
            manifest: &manifest,
            prefer_encoder: None,
            path: cm_media::ExportPath::Auto,
            loudness_lufs: None,
            preset: None,
        },
        &mut |_| {},
        &|| false,
    );
    let e = match r {
        Err(e) => e.into_cm(),
        Ok(_) => panic!("must fail"),
    };
    assert_eq!(e.code(), cm_core::ErrorCode::NotFound);
}

/// Progress reaches the callback as increasing fractions, ending at 1.0.
#[test]
fn progress_is_reported() {
    let Some(t) = tools() else { return };
    let f = fixture(&t, 4, "1280x720", true, true);
    let mut seen: Vec<f64> = Vec::new();
    let out = export_quick(
        &t,
        &QuickRequest {
            captions: None,
            bundle_dir: f.path(),
            manifest: &f.manifest,
            prefer_encoder: None,
            path: cm_media::ExportPath::Auto,
            loudness_lufs: None,
            preset: None,
        },
        &mut |p| seen.push(p),
        &|| false,
    )
    .unwrap();
    assert_eq!(seen.last().copied(), Some(1.0));
    // A fallback to another encoder restarts the bar from 0 on purpose (R5-4); within one attempt it only grows.
    if out.settings.get("encoder_fallback").is_none() {
        assert!(seen.windows(2).all(|w| w[1] >= w[0] - 1e-9), "{seen:?}");
    }
}

/// Timing of a 30 s 1080p export (the report's number). `cargo test -p cm-media --test
/// quick_export timing -- --ignored --nocapture`.
#[test]
#[ignore = "timing probe; prints the export time of a 30 s 1080p fixture"]
fn timing_of_a_30_s_1080p_fixture() {
    let Some(t) = tools() else { return };
    let mut f = fixture(&t, 30, "1920x1080", true, true);
    f.manifest["tracks"]["screen"]["size"] = json!([1920, 1080]);
    f.manifest["tracks"]["screen"]["source_size"] = json!([1920, 1080]);
    f.manifest["tracks"]["screen"]["scale"] = json!(1.0);
    for hw in [None, Some(cm_media::Encoder::Libx264)] {
        let o = export_quick(
            &t,
            &QuickRequest {
                captions: None,
                bundle_dir: f.path(),
                manifest: &f.manifest,
                prefer_encoder: hw,
                path: cm_media::ExportPath::Auto,
                loudness_lufs: None,
                preset: None,
            },
            &mut |_| {},
            &|| false,
        )
        .unwrap();
        eprintln!(
            "TIMING: 30 s 1080p with {} took {} ms ({:.1}x real time), {} bytes",
            o.encoder,
            o.elapsed_ms,
            30_000.0 / o.elapsed_ms as f64,
            o.size_bytes
        );
    }
}

/// Same fixture through the compositor path (a 10% purple frame, a bubble, a cut). Run with
/// `cargo test -p cm-media --test quick_export timing_compositor -- --ignored --nocapture`.
#[test]
#[ignore = "timing probe; prints the compositor export time of a 30 s 1080p fixture"]
fn timing_compositor_30_s_1080p_fixture() {
    let Some(t) = tools() else { return };
    if !gpu_available() {
        return;
    }
    let mut f = fixture(&t, 30, "1920x1080", true, true);
    f.manifest["tracks"]["screen"]["size"] = json!([1920, 1080]);
    f.manifest["tracks"]["screen"]["source_size"] = json!([1920, 1080]);
    f.manifest["tracks"]["screen"]["scale"] = json!(1.0);
    let m = with_edits(
        &f,
        json!({"cuts": [[5000 * MS, 6000 * MS]],
               "background": {"kind": "color", "value": "#7f007f", "padding_pct": 6.0,
                              "radius_px": 12, "shadow": true}}),
    );
    let o = run_edited(&t, &f, &m);
    let secs = o.duration_ms as f64 / 1000.0;
    eprintln!(
        "TIMING: compositor, {secs:.1} s of 1080p with {} took {} ms ({:.1}x real time), {} bytes",
        o.encoder,
        o.elapsed_ms,
        secs * 1000.0 / o.elapsed_ms as f64,
        o.size_bytes
    );
}

// ---- R5 robustness -------------------------------------------------------------------------

/// A stand-in `ffmpeg` that advertises VideoToolbox and libx264 and fails the encode with
/// `fail_msg` when the command asks for `h264_videotoolbox`; every other command (the libx264
/// encode) is forwarded to the real FFmpeg. Each encode appends a line to `calls.log`.
#[cfg(unix)]
fn stub_ffmpeg(real: &Tools, dir: &Path, fail_msg: &str) -> (Tools, PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let log = dir.join("calls.log");
    let script = format!(
        r#"#!/bin/sh
for a in "$@"; do
  case "$a" in
    -version) echo "ffmpeg version 7.1 Copyright (c) stub"; exit 0;;
    -encoders) printf ' V....D h264_videotoolbox VT\n V....D libx264 x264\n A....D aac AAC\n'; exit 0;;
  esac
done
echo "$@" | grep -o 'h264_videotoolbox\|libx264' | head -1 >> '{log}'
case "$*" in
  *h264_videotoolbox*) echo '{msg}' >&2; exit 1;;
esac
exec '{real}' "$@"
"#,
        log = log.display(),
        msg = fail_msg,
        real = real.ffmpeg.display()
    );
    let path = dir.join("ffmpeg-stub");
    std::fs::write(&path, script).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    (
        Tools {
            ffmpeg: path,
            ffprobe: real.ffprobe.clone(),
        },
        log,
    )
}

/// R5-4: when the hardware encoder cannot start, the export is redone with the next encoder and
/// says so; the encode is attempted once per encoder.
#[cfg(unix)]
#[test]
fn r5_4_videotoolbox_init_failure_falls_back_to_libx264() {
    let Some(t) = tools() else { return };
    let f = fixture(&t, 3, "1280x720", false, false);
    let stub_dir = tempfile::tempdir().unwrap();
    let (stub, log) = stub_ffmpeg(
        &t,
        stub_dir.path(),
        "Error while opening encoder for output stream #0:0 - maybe incorrect parameters",
    );
    let o = export_quick(
        &stub,
        &QuickRequest {
            captions: None,
            bundle_dir: f.path(),
            manifest: &f.manifest,
            prefer_encoder: None,
            path: cm_media::ExportPath::Auto,
            loudness_lufs: None,
            preset: None,
        },
        &mut |_| {},
        &|| false,
    )
    .unwrap_or_else(|e| panic!("{}", e.into_cm()));
    assert_eq!(o.encoder, "libx264");
    assert!(!o.lgpl_safe);
    let note = o.settings["encoder_fallback"].as_str().unwrap();
    assert!(
        note.contains("h264_videotoolbox failed, used libx264"),
        "{note}"
    );
    assert_eq!(
        std::fs::read_to_string(log)
            .unwrap()
            .lines()
            .collect::<Vec<_>>(),
        ["h264_videotoolbox", "libx264"]
    );
    assert!(o.path.is_file());
}

/// R5-4: a failure that is not an encoder start-up problem (bad input) is NOT retried, and a
/// forced encoder is not replaced.
#[cfg(unix)]
#[test]
fn r5_4_other_failures_are_not_retried() {
    let Some(t) = tools() else { return };
    let f = fixture(&t, 3, "1280x720", false, false);
    for (msg, prefer) in [
        ("screen.mp4: Invalid data found when processing input", None),
        (
            "Error while opening encoder for output stream #0:0",
            Some(cm_media::Encoder::VideoToolbox),
        ),
    ] {
        let stub_dir = tempfile::tempdir().unwrap();
        let (stub, log) = stub_ffmpeg(&t, stub_dir.path(), msg);
        let r = export_quick(
            &stub,
            &QuickRequest {
                captions: None,
                bundle_dir: f.path(),
                manifest: &f.manifest,
                prefer_encoder: prefer,
                path: cm_media::ExportPath::Auto,
                loudness_lufs: None,
                preset: None,
            },
            &mut |_| {},
            &|| false,
        );
        let e = match r {
            Err(e) => e.into_cm(),
            Ok(_) => panic!("must fail: {msg}"),
        };
        assert!(
            e.to_string().contains(msg.split(':').next().unwrap()),
            "{e}"
        );
        assert_eq!(
            std::fs::read_to_string(log).unwrap().lines().count(),
            1,
            "{msg}"
        );
        let left: Vec<_> = std::fs::read_dir(f.path().join("exports"))
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert!(left.is_empty(), "no partial output: {left:?}");
    }
}

/// R5-16: a title renamed between two exports leaves one file, not two; very long and CJK titles
/// export fine.
#[test]
fn r5_16_reexport_after_rename_replaces_the_old_file() {
    let Some(t) = tools() else { return };
    let mut f = fixture(&t, 2, "1280x720", false, false);
    f.manifest["title"] = json!("First name");
    let first = run_export(&t, &f);
    assert!(first.replaced.is_empty());
    // The manifest as the daemon stores it after the first export, then a rename.
    f.manifest["exports"] = json!([first.to_entry("2026-10-21T10:00:00Z")]);
    f.manifest["title"] = json!("字".repeat(300));
    let second = run_export(&t, &f);
    assert_eq!(second.replaced, ["exports/First name - quick.mp4"]);
    assert!(!first.path.exists());
    assert!(second.path.is_file());
    assert!(second.path.file_name().unwrap().len() <= 255);
    let names: Vec<_> = std::fs::read_dir(f.path().join("exports"))
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names.len(), 1, "{names:?}");
}

/// A camera file shorter than the screen: the export succeeds, takes the screen's length, and the
/// bubble keeps showing the camera's last frame (the overlay repeats it at EOF) instead of
/// vanishing or ending the video early.
#[test]
fn camera_shorter_than_the_screen_keeps_the_bubble_to_the_end() {
    let Some(t) = tools() else { return };
    let f = fixture(&t, 6, "1280x720", true, false);
    // Replace the 6 s camera by a 2 s one (all blue: the colour change happens at 1 s).
    ff(
        &t,
        &[
            "-f",
            "lavfi",
            "-i",
            &format!("color=c=blue:s=1280x720:r={FPS}:d=2"),
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-pix_fmt",
            "yuv420p",
            "-output_ts_offset",
            "0.58",
            "-movflags",
            "frag_keyframe+empty_moov",
            &f.path().join("camera.mp4").to_string_lossy(),
        ],
    );
    let o = run_export(&t, &f);
    let info = probe_file(&t, &o.path).unwrap();
    assert!(
        (info.duration_s.unwrap() - 6.0).abs() < 0.2,
        "duration {:?}",
        info.duration_s
    );
    let tmp = tempfile::tempdir().unwrap();
    let late = frame_at(&t, &o.path, 5.0, tmp.path());
    // Inside the bubble (centre 1024,524), well after the camera ended.
    assert!(
        near(late.get_pixel(1024, 524), [0, 0, 255], 80),
        "bubble at 5 s: {:?}",
        late.get_pixel(1024, 524)
    );
}

// ---- the edited export (EXP-1, EDT-1..3) -----------------------------------------------------------
//
// Same fixtures, a versioned edit list. They need a GPU adapter for the compositor and print
// SKIPPED without one (or without FFmpeg).

fn gpu_available() -> bool {
    match cm_render::Compositor::new() {
        Ok(_) => true,
        Err(e) => {
            eprintln!("SKIPPED: no GPU compositor ({e})");
            false
        }
    }
}

/// The fixture's manifest with `edits[0]` = a version 2 list over the whole screen span, patched
/// with `patch` (cuts, splits, speeds, background, in, out).
fn with_edits(f: &Fixture, patch: Value) -> Value {
    let mut m = f.manifest.clone();
    let screen_end = m["tracks"]["screen"]["end"].as_i64().unwrap();
    let mut e = json!({
        "version": 2, "in": SCREEN_START_MS * MS, "out": screen_end, "speed": 1.0,
        "layout": m["edits"][0]["layout"].clone(),
    });
    e.as_object_mut()
        .unwrap()
        .extend(patch.as_object().unwrap().clone());
    m["edits"] = json!([e]);
    m
}

fn run_edited(t: &Tools, f: &Fixture, manifest: &Value) -> cm_media::QuickOutcome {
    export_quick(
        t,
        &QuickRequest {
            captions: None,
            bundle_dir: f.path(),
            manifest,
            prefer_encoder: None,
            path: cm_media::ExportPath::Auto,
            loudness_lufs: None,
            preset: None,
        },
        &mut |_| {},
        &|| false,
    )
    .unwrap_or_else(|e| panic!("edited export failed: {}", e.into_cm()))
}

fn duration_of(t: &Tools, file: &Path) -> f64 {
    probe_file(t, file).unwrap().duration_s.unwrap()
}

/// First frame (index) whose mean luma is above 200: the white flash.
fn flash_frame(t: &Tools, video: &Path) -> usize {
    region_means(t, video, "iw:ih:0:0")
        .iter()
        .position(|l| *l > 200)
        .expect("no flash in the export")
}

/// Time of the first sample above 0.3: the mic beep (the system tone stays at 0.1).
fn beep_time(t: &Tools, file: &Path) -> f64 {
    let p = pcm(t, file);
    p.iter().position(|x| x.abs() > 0.3).expect("no beep") as f64 / 48_000.0
}

/// EDT-1.AC1: trimming the start and end shortens the export by exactly that (± 1 frame). The
/// fixture's 1 s pause is skipped as well: 0.89..4.89 s of session time minus the pause = 3.0 s.
#[test]
fn edt1_ac1_trim_sets_the_export_duration() {
    let Some(t) = tools() else { return };
    if !gpu_available() {
        return;
    }
    let f = fixture(&t, 4, "1280x720", false, true);
    let m = with_edits(&f, json!({"in": 890 * MS, "out": 4890 * MS}));
    let o = run_edited(&t, &f, &m);
    let d = duration_of(&t, &o.path);
    assert!((d - 3.0).abs() <= 1.0 / 30.0 + 0.02, "duration {d}");
    assert_eq!(o.settings["renderer"], "compositor");
}

/// EDT-1.AC2 / NFR-3: after a cut the picture and the sound stay together. The flash (session
/// 1.39 s) and the mic beep (the same instant) both move by the 0.5 s that was cut before them.
#[test]
fn edt1_ac2_flash_and_beep_stay_in_sync_after_a_cut() {
    let Some(t) = tools() else { return };
    if !gpu_available() {
        return;
    }
    let f = fixture(&t, 4, "1280x720", false, true);
    let m = with_edits(
        &f,
        json!({"cuts": [[500 * MS, 1000 * MS]], "splits": [500 * MS, 1000 * MS]}),
    );
    let o = run_edited(&t, &f, &m);
    // in = 0.39 s, so the flash is at (1.39 - 0.39 - 0.5) = 0.5 s of the output.
    let flash_s = flash_frame(&t, &o.path) as f64 / 30.0;
    let beep_s = beep_time(&t, &o.path);
    assert!((flash_s - 0.5).abs() <= 2.0 / 30.0, "flash at {flash_s}");
    assert!((beep_s - 0.5).abs() <= 0.04, "beep at {beep_s}");
    assert!((flash_s - beep_s).abs() <= 0.05, "{flash_s} vs {beep_s}");
}

/// EDT-2.AC1: a 2 s span at 2x lasts 1 s; the flash inside it lands at (1.39 - 0.39) / 2 = 0.5 s.
#[test]
fn edt2_ac1_speed_span_halves_and_moves_the_flash() {
    let Some(t) = tools() else { return };
    if !gpu_available() {
        return;
    }
    let f = fixture(&t, 4, "1280x720", false, true);
    let m = with_edits(
        &f,
        json!({"splits": [390 * MS, 2390 * MS],
               "speeds": [{"from": 390 * MS, "to": 2390 * MS, "factor": 2.0, "keep_audio": false}]}),
    );
    let o = run_edited(&t, &f, &m);
    // Whole export: 5.0 s of session minus the 1 s pause = 4.0 s, minus 1 s from the 2x span.
    let d = duration_of(&t, &o.path);
    assert!((d - 3.0).abs() <= 1.0 / 30.0 + 0.02, "duration {d}");
    let flash_s = flash_frame(&t, &o.path) as f64 / 30.0;
    assert!((flash_s - 0.5).abs() <= 3.0 / 30.0, "flash at {flash_s}");
}

/// EDT-3 in the export: a purple background with 10% padding shows red at the corners and the
/// screen in the middle; the output keeps the master's size.
#[test]
fn edt3_background_and_padding_reach_the_export() {
    let Some(t) = tools() else { return };
    if !gpu_available() {
        return;
    }
    let f = fixture(&t, 3, "1280x720", false, false);
    let m = with_edits(
        &f,
        json!({"background": {"kind": "color", "value": "#7f007f", "padding_pct": 10.0,
                              "radius_px": 12, "shadow": false}}),
    );
    let o = run_edited(&t, &f, &m);
    let tmp = tempfile::tempdir().unwrap();
    let img = frame_at(&t, &o.path, 0.5, tmp.path());
    assert_eq!(img.dimensions(), (1280, 720));
    assert!(
        near(img.get_pixel(4, 4), [127, 0, 127], 40),
        "corner is not the background"
    );
    assert!(
        !near(img.get_pixel(640, 360), [127, 0, 127], 60),
        "the screen is missing"
    );
    // Padding is 10% of the width (128 px) at the sides and 10% of the height (72 px) above and
    // below, so a screen with the frame's own aspect gets 1024 x 576. Scanning inward from the
    // border, the purple ends there (the test pattern has no purple at its edges).
    let end_of_bg = (0..640)
        .find(|x| !near(img.get_pixel(*x, 360), [127, 0, 127], 40))
        .unwrap();
    assert!(
        (125..=131).contains(&end_of_bg),
        "purple ends at x = {end_of_bg}"
    );
    let end_of_bg_y = (0..360)
        .find(|y| !near(img.get_pixel(640, *y), [127, 0, 127], 40))
        .unwrap();
    assert!(
        (69..=75).contains(&end_of_bg_y),
        "purple ends at y = {end_of_bg_y}"
    );
}

/// The camera bubble is composited by the GPU path too, at the stored layout, mirrored.
#[test]
fn camera_bubble_is_drawn_by_the_compositor() {
    let Some(t) = tools() else { return };
    if !gpu_available() {
        return;
    }
    let f = fixture(&t, 3, "1280x720", true, false);
    let m = with_edits(&f, json!({"cuts": [[2000 * MS, 2500 * MS]]}));
    let o = run_edited(&t, &f, &m);
    let tmp = tempfile::tempdir().unwrap();
    let img = frame_at(&t, &o.path, 0.5, tmp.path());
    // rect [0.70, 0.55, 0.20, 0.20] of 1280x720: x 896..1152, y 396..; the bubble is square.
    let (cx, cy) = (1024u32, 396 + 128);
    // Mirrored: the green half is on the left.
    let left = img.get_pixel(cx - 40, cy);
    let right = img.get_pixel(cx + 40, cy);
    assert!(
        left[1] > left[0] + 80,
        "left of the bubble is not green: {left:?}"
    );
    assert!(
        right[0] > right[1] + 80,
        "right of the bubble is not red: {right:?}"
    );
}

/// The fixture with a camera and the layout ranges `segments` over the recorder's bubble.
fn with_layout(f: &Fixture, segments: Value) -> Value {
    let layout = json!({"camera": f.manifest["edits"][0]["layout"]["camera"].clone(),
                        "segments": segments});
    with_edits(f, json!({ "layout": layout }))
}

fn is_green(p: &image::Rgb<u8>) -> bool {
    let [r, g, b] = p.0.map(i32::from);
    g > r + 80 && g > b + 80
}

fn is_red(p: &image::Rgb<u8>) -> bool {
    let [r, g, b] = p.0.map(i32::from);
    r > g + 80 && r > b + 80
}

/// CAM-3.AC2: a range set to camera full shows only the camera, edge to edge, in that range (the
/// camera is mirrored, so green is on the left), and the screen comes back after it.
#[test]
fn cam3_ac2_camera_full_range_shows_only_the_camera() {
    let Some(t) = tools() else { return };
    if !gpu_available() {
        return;
    }
    let f = fixture(&t, 3, "1280x720", true, false);
    let m = with_layout(
        &f,
        json!([{"from": 390 * MS, "to": 1390 * MS, "mode": "camera", "transition": "cut"}]),
    );
    let o = run_edited(&t, &f, &m);
    let tmp = tempfile::tempdir().unwrap();
    let inside = frame_at(&t, &o.path, 0.5, tmp.path());
    assert_eq!(inside.dimensions(), (1280, 720));
    for (x, y) in [(40, 40), (300, 360), (600, 680)] {
        assert!(
            is_green(inside.get_pixel(x, y)),
            "({x},{y}) not green: {:?}",
            inside.get_pixel(x, y)
        );
    }
    for (x, y) in [(700, 40), (960, 360), (1240, 680)] {
        assert!(
            is_red(inside.get_pixel(x, y)),
            "({x},{y}) not red: {:?}",
            inside.get_pixel(x, y)
        );
    }
    let after = frame_at(&t, &o.path, 1.7, tmp.path());
    let all_green = [(40, 40), (300, 360), (600, 680)]
        .iter()
        .all(|(x, y)| is_green(after.get_pixel(*x, *y)));
    assert!(!all_green, "the screen did not come back after the range");
}

/// CAM-3.AC1: a bubble moved to the top left for a range shows there, and not at the recorder's
/// bottom-right place, in that range.
#[test]
fn cam3_ac1_a_moved_bubble_shows_top_left_in_its_range() {
    let Some(t) = tools() else { return };
    if !gpu_available() {
        return;
    }
    let f = fixture(&t, 3, "1280x720", true, false);
    let m = with_layout(
        &f,
        json!([{"from": 390 * MS, "to": 2390 * MS, "mode": "bubble",
                "rect": [0.05, 0.05, 0.2, 0.2], "shape": "circle", "transition": "cut"}]),
    );
    let o = run_edited(&t, &f, &m);
    let tmp = tempfile::tempdir().unwrap();
    let img = frame_at(&t, &o.path, 0.5, tmp.path());
    // 0.2 * 1280 = 256 px wide at (64, 36): the centre is (192, 164); mirrored, green is left.
    assert!(
        is_green(img.get_pixel(192 - 40, 164)),
        "{:?}",
        img.get_pixel(152, 164)
    );
    assert!(
        is_red(img.get_pixel(192 + 40, 164)),
        "{:?}",
        img.get_pixel(232, 164)
    );
    // At the recorder's place (x 896.., y 396..) the test pattern is back: its pixels are not the
    // bubble's pure red and green on both sides of the centre.
    let (l, r) = (img.get_pixel(1024 - 40, 524), img.get_pixel(1024 + 40, 524));
    assert!(
        !(is_green(l) && is_red(r)),
        "the bubble is still at its old place"
    );
}

/// CAM-3.AC3: after a smooth change the camera is half grown part way through the 300 ms and
/// full frame at the end; just after the start it is still small.
#[test]
fn cam3_ac3_smooth_change_grows_the_camera_over_300_ms() {
    let Some(t) = tools() else { return };
    if !gpu_available() {
        return;
    }
    let f = fixture(&t, 3, "1280x720", true, false);
    let m = with_layout(
        &f,
        json!([
            {"from": 390 * MS, "to": 790 * MS, "mode": "screen", "transition": "cut"},
            {"from": 790 * MS, "to": 1390 * MS, "mode": "camera", "transition": "smooth"}
        ]),
    );
    let o = run_edited(&t, &f, &m);
    let tmp = tempfile::tempdir().unwrap();
    // The change starts 0.4 s into the export and ends at 0.7 s.
    let early = frame_at(&t, &o.path, 0.43, tmp.path());
    assert!(
        !is_green(early.get_pixel(400, 360)),
        "{:?}",
        early.get_pixel(400, 360)
    );
    let mid = frame_at(&t, &o.path, 0.55, tmp.path());
    // Half way (eased 0.5): a 640 x 360 camera tile around the centre; the screen around it.
    assert!(
        is_green(mid.get_pixel(400, 360)),
        "{:?}",
        mid.get_pixel(400, 360)
    );
    assert!(
        is_red(mid.get_pixel(880, 360)),
        "{:?}",
        mid.get_pixel(880, 360)
    );
    assert!(!is_green(mid.get_pixel(100, 100)) && !is_red(mid.get_pixel(100, 100)));
    let done = frame_at(&t, &o.path, 0.85, tmp.path());
    assert!(
        is_green(done.get_pixel(100, 100)),
        "{:?}",
        done.get_pixel(100, 100)
    );
}

/// EXP-4.AC4: cancelling an edited export leaves no partial file.
#[test]
fn exp4_cancelling_an_edited_export_cleans_up() {
    let Some(t) = tools() else { return };
    if !gpu_available() {
        return;
    }
    let f = fixture(&t, 3, "1280x720", false, true);
    let m = with_edits(&f, json!({"cuts": [[2000 * MS, 2500 * MS]]}));
    let flag = AtomicBool::new(false);
    let started = std::time::Instant::now();
    let stalled = AtomicBool::new(false);
    let r = export_quick(
        &t,
        &QuickRequest {
            captions: None,
            bundle_dir: f.path(),
            manifest: &m,
            prefer_encoder: None,
            path: cm_media::ExportPath::Auto,
            loudness_lufs: None,
            preset: None,
        },
        &mut |p| {
            if p > 0.05 {
                flag.store(true, Ordering::SeqCst);
            }
        },
        &|| {
            // The cancel is only asked for after 5 % progress. Once, in a busy full run, no frame
            // ever reached FFmpeg (it sat idle for an hour while the compositor produced nothing),
            // so this test waited forever: give up after two minutes and say why instead.
            if !flag.load(Ordering::SeqCst)
                && started.elapsed() > std::time::Duration::from_secs(120)
            {
                stalled.store(true, Ordering::SeqCst);
                return true;
            }
            flag.load(Ordering::SeqCst)
        },
    );
    assert!(
        !stalled.load(Ordering::SeqCst),
        "the export made no progress for two minutes (a stalled compositor?): {r:?}"
    );
    assert!(matches!(r, Err(ExportError::Cancelled)), "{r:?}");
    let left: Vec<_> = std::fs::read_dir(f.path().join("exports"))
        .map(|d| d.flatten().map(|e| e.file_name()).collect())
        .unwrap_or_default();
    assert!(left.is_empty(), "left behind: {left:?}");
}

// ---- the cm-audio mix in the export (AUD-4, EDT-2.AC2, REC-4 pauses) ----------------------------

/// `with_edits` plus an `audio` mix object and an optional loudness target.
fn run_mixed(
    t: &Tools,
    f: &Fixture,
    patch: Value,
    audio: Value,
    loudness: Option<f64>,
) -> cm_media::QuickOutcome {
    let mut p = patch;
    p.as_object_mut().unwrap().insert("audio".into(), audio);
    let m = with_edits(f, p);
    export_quick(
        t,
        &QuickRequest {
            captions: None,
            bundle_dir: f.path(),
            manifest: &m,
            prefer_encoder: None,
            path: cm_media::ExportPath::Auto,
            loudness_lufs: loudness,
            preset: None,
        },
        &mut |_| {},
        &|| false,
    )
    .unwrap_or_else(|e| panic!("edited export failed: {}", e.into_cm()))
}

/// Output PCM as interleaved stereo f32 at 48 kHz.
#[allow(clippy::chunks_exact_to_as_chunks)]
fn pcm_stereo(t: &Tools, audio: &Path) -> Vec<f32> {
    let out = Command::new(&t.ffmpeg)
        .args(["-hide_banner", "-loglevel", "error", "-i"])
        .arg(audio)
        .args(["-vn", "-ac", "2", "-ar", "48000", "-f", "f32le", "-"])
        .output()
        .unwrap();
    assert!(out.status.success());
    out.stdout
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect()
}

fn rms_db(x: &[f32]) -> f64 {
    let e: f64 = x.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>() / x.len().max(1) as f64;
    10.0 * e.max(1e-20).log10()
}

/// The part of a mono 48 kHz signal between two times.
fn window(x: &[f32], from_s: f64, to_s: f64) -> &[f32] {
    let a = (from_s * 48_000.0) as usize;
    let b = ((to_s * 48_000.0) as usize).min(x.len());
    &x[a.min(b)..b]
}

/// Frequency from the rising zero crossings of a clean tone.
fn tone_hz(x: &[f32], from_s: f64, to_s: f64) -> f64 {
    let w = window(x, from_s, to_s);
    let ups: Vec<usize> = (1..w.len())
        .filter(|i| w[i - 1] <= 0.0 && w[*i] > 0.0)
        .collect();
    let (first, last) = (*ups.first().unwrap(), *ups.last().unwrap());
    (ups.len() - 1) as f64 * 48_000.0 / (last - first) as f64
}

/// AUD-4.AC1: a +6 dB system gain raises the exported system tone by 6 dB (within 0.5 dB); the
/// microphone is muted in both runs so only the steady 300 Hz tone is measured.
#[test]
fn aud4_ac1_system_gain_is_applied_in_the_export() {
    let Some(t) = tools() else { return };
    if !gpu_available() {
        return;
    }
    let f = fixture(&t, 4, "1280x720", false, true);
    let level = |gain: f64| {
        let o = run_mixed(
            &t,
            &f,
            json!({}),
            json!({"mic_mute": true, "system_gain_db": gain}),
            None,
        );
        assert_eq!(o.settings["audio"]["mixer"], "cm-audio");
        let p = pcm(&t, &o.path);
        rms_db(window(&p, 0.5, 3.0))
    };
    let (base, boosted) = (level(0.0), level(6.0));
    assert!(
        (boosted - base - 6.0).abs() <= 0.5,
        "base {base:.2} dB, boosted {boosted:.2} dB"
    );
}

/// EXP-4.AC4: a finished edited export with mixed audio leaves nothing but the file itself: the
/// raw mixed-audio scratch file (about 23 MB per minute) is deleted too (M4 gate finding).
#[test]
fn exp4_a_finished_mixed_export_leaves_no_scratch_files() {
    let Some(t) = tools() else { return };
    if !gpu_available() {
        return;
    }
    let f = fixture(&t, 3, "1280x720", false, true);
    let o = run_mixed(&t, &f, json!({}), json!({"system_gain_db": -3.0}), None);
    assert_eq!(o.settings["audio"]["mixer"], "cm-audio");
    let names: Vec<String> = std::fs::read_dir(f.path().join("exports"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        names,
        vec![o.file.trim_start_matches("exports/").to_owned()],
        "{names:?}"
    );
}

/// AUD-4.AC2: `loudness_lufs: -14` lands the export at -14 LUFS (within 1 LU) under the -1 dBTP
/// ceiling; the result is recorded in the export settings.
#[test]
fn aud4_ac2_loudness_target_is_reached_in_the_export() {
    let Some(t) = tools() else { return };
    if !gpu_available() {
        return;
    }
    let f = fixture(&t, 4, "1280x720", false, true);
    let o = run_mixed(&t, &f, json!({}), json!({"mic_mute": true}), Some(-14.0));
    let s = pcm_stereo(&t, &o.path);
    let lufs = cm_audio::mix::integrated_lufs(&s, 2, 48_000).expect("audible output");
    assert!((lufs + 14.0).abs() <= 1.0, "measured {lufs:.2} LUFS");
    // The AAC encode can overshoot the limiter a little; the limiter itself is -1 dBTP.
    let peak = cm_audio::mix::true_peak_db(&s, 2);
    assert!(peak <= -0.5, "true peak {peak:.2} dBTP");
    let l = &o.settings["audio"]["loudness"];
    assert_eq!(l["target_lufs"], -14.0);
    assert!((l["integrated_lufs"].as_f64().unwrap() + 14.0).abs() <= 1.0);
}

/// AUD-4.AC4: a muted source contributes nothing. With the system track muted the steady tone is
/// gone but the mic beep is still there; with both muted the whole export is silent.
#[test]
fn aud4_ac4_muted_sources_contribute_nothing() {
    let Some(t) = tools() else { return };
    if !gpu_available() {
        return;
    }
    let f = fixture(&t, 4, "1280x720", false, true);
    let o = run_mixed(&t, &f, json!({}), json!({"system_mute": true}), None);
    let p = pcm(&t, &o.path);
    assert!(rms_db(window(&p, 2.0, 3.0)) < -80.0, "tone still audible");
    assert!(beep_time(&t, &o.path) > 0.0);
    let o = run_mixed(
        &t,
        &f,
        json!({}),
        json!({"mic_mute": true, "system_mute": true}),
        None,
    );
    let p = pcm(&t, &o.path);
    assert!(rms_db(&p) < -80.0, "muted export is not silent");
}

/// EDT-2.AC2 on the exported file: a 1.5x span keeps the 300 Hz tone's pitch (within 3%), and
/// the span is 1.5x shorter.
#[test]
fn edt2_ac2_exported_pitch_is_kept_at_1_5x() {
    let Some(t) = tools() else { return };
    if !gpu_available() {
        return;
    }
    let f = fixture(&t, 4, "1280x720", false, true);
    let o = run_mixed(
        &t,
        &f,
        json!({"splits": [390 * MS, 3390 * MS],
               "speeds": [{"from": 390 * MS, "to": 3390 * MS, "factor": 1.5, "keep_audio": false}]}),
        json!({"mic_mute": true}),
        None,
    );
    // 3 s of session at 1.5x = 2 s, then the 1 s pause is cut and 1 s at 1x remains: 3 s.
    let d = duration_of(&t, &o.path);
    assert!((d - 3.0).abs() <= 0.1, "duration {d}");
    let p = pcm(&t, &o.path);
    let hz = tone_hz(&p, 0.3, 1.7);
    assert!((hz - 300.0).abs() <= 9.0, "pitch {hz:.1} Hz");
}

/// REC-4 / EDT-1: a pause that comes before the flash and the beep is removed from the picture
/// and from the sound: both land 1.0 s into the export, and the two tracks end together.
#[test]
fn rec4_pause_is_removed_from_picture_and_sound_and_they_stay_in_sync() {
    let Some(t) = tools() else { return };
    if !gpu_available() {
        return;
    }
    let mut f = fixture(&t, 4, "1280x720", false, true);
    // A 1 s pause at session 0.6..1.6 s, before the flash (file 1.0 s) and the beep. Every
    // later session time is 1 s later than the file time suggests: flash and beep at 2.39 s.
    f.manifest["gaps"] = json!([{"from": 600 * MS, "to": 1600 * MS}]);
    let o = run_mixed(&t, &f, json!({}), json!({"system_mute": true}), None);
    // Session 0.39..5.39 s minus the 1 s pause = 4.0 s.
    let d = duration_of(&t, &o.path);
    assert!((d - 4.0).abs() <= 0.1, "duration {d}");
    let flash_s = flash_frame(&t, &o.path) as f64 / 30.0;
    let beep_s = beep_time(&t, &o.path);
    assert!((flash_s - 1.0).abs() <= 2.0 / 30.0, "flash at {flash_s}");
    assert!((beep_s - 1.0).abs() <= 0.04, "beep at {beep_s}");
    assert!((flash_s - beep_s).abs() <= 0.05, "{flash_s} vs {beep_s}");
    let video = probe_file(&t, &o.path).unwrap();
    let audio_len = pcm(&t, &o.path).len() as f64 / 48_000.0;
    assert!(
        (audio_len - video.duration_s.unwrap()).abs() <= 0.1,
        "audio {audio_len} vs video {:?}",
        video.duration_s
    );
}

// ---- presets (EXP-2) and GIF/WebM (EXP-3) --------------------------------------------------------
//
// The fixtures are 1280x720 at 30 fps, so every preset below is made by the compositor (a fixed
// frame, GIF, WebM) or by the FFmpeg graph (`source`). They need FFmpeg and a GPU adapter.

use cm_media::{ExportOptions, PresetRequest, Presets};

/// The fixture's manifest as a versioned edit list covering the whole recording (the fixture's
/// own entry is the recorder's untouched form, which the FFmpeg graph renders).
fn versioned(f: &Fixture) -> Value {
    with_edits(f, json!({}))
}

fn run_preset(
    t: &Tools,
    f: &Fixture,
    manifest: &Value,
    id: &str,
    options: ExportOptions,
) -> Result<cm_media::QuickOutcome, cm_core::CmError> {
    let presets = Presets::builtin();
    export_quick(
        t,
        &QuickRequest {
            captions: None,
            bundle_dir: f.path(),
            manifest,
            prefer_encoder: None,
            path: cm_media::ExportPath::Auto,
            loudness_lufs: None,
            preset: Some(PresetRequest {
                presets: &presets,
                id,
                options,
            }),
        },
        &mut |_| {},
        &|| false,
    )
    .map_err(ExportError::into_cm)
}

/// `(width, height, codec)` of the first video stream.
fn video_info(t: &Tools, file: &Path) -> (u64, u64, String) {
    let j = ffprobe_json(t, file);
    let v = j["streams"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["codec_type"] == "video")
        .unwrap()
        .clone();
    (
        v["width"].as_u64().unwrap(),
        v["height"].as_u64().unwrap(),
        v["codec_name"].as_str().unwrap().to_owned(),
    )
}

fn audio_codec(t: &Tools, file: &Path) -> Option<(String, u64)> {
    let j = ffprobe_json(t, file);
    j["streams"]
        .as_array()?
        .iter()
        .find(|s| s["codec_type"] == "audio")
        .map(|s| {
            (
                s["codec_name"].as_str().unwrap().to_owned(),
                s["sample_rate"].as_str().unwrap().parse().unwrap(),
            )
        })
}

/// Number of decoded video frames.
fn decoded_frames(t: &Tools, file: &Path) -> u64 {
    let out = Command::new(t.ffprobe.as_ref().unwrap())
        .args([
            "-v",
            "error",
            "-count_frames",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=nb_read_frames",
            "-of",
            "csv=p=0",
        ])
        .arg(file)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().parse().unwrap()
}

/// Bounding box `(min_x, min_y, max_x, max_y)` of the pixels near `rgb` in a frame.
fn bbox_of(img: &image::RgbImage, rgb: [u8; 3]) -> Option<(u32, u32, u32, u32)> {
    let mut b: Option<(u32, u32, u32, u32)> = None;
    for (x, y, p) in img.enumerate_pixels() {
        if near(p, rgb, 40) {
            b = Some(match b {
                None => (x, y, x, y),
                Some((a, c, d, e)) => (a.min(x), c.min(y), d.max(x), e.max(y)),
            });
        }
    }
    b
}

/// EXP-2.AC1 / vertical-1080: the output is 1080x1920 H.264/AAC 48 kHz with fast start, the
/// recording is fitted by width and centered (the background shows above and below), and the
/// camera bubble stays out of the bottom 20% and the right 12% of the frame.
#[test]
fn exp2_vertical_1080_is_1080x1920_with_the_bubble_in_the_safe_area() {
    let Some(t) = tools() else { return };
    if !gpu_available() {
        return;
    }
    let f = fixture(&t, 4, "1280x720", true, true);
    let m = with_edits(
        &f,
        json!({"background": {"kind": "color", "value": "#101030", "padding_pct": 0.0,
                               "radius_px": 0, "shadow": false},
               "audio": {"mic_mute": true}}),
    );
    let o = run_preset(&t, &f, &m, "vertical-1080", ExportOptions::default()).unwrap();
    assert_eq!(o.preset, "vertical-1080");
    assert!(o.file.ends_with(" - vertical-1080.mp4"), "{}", o.file);
    assert_eq!(video_info(&t, &o.path), (1080, 1920, "h264".into()));
    assert_eq!(audio_codec(&t, &o.path), Some(("aac".into(), 48_000)));
    assert_eq!(boxes(&o.path)[..2], ["ftyp", "moov"], "fast start");
    let d = duration_of(&t, &o.path);
    assert!((d - 4.0).abs() <= 1.0 / 30.0 + 0.02, "duration {d}");
    assert_eq!(o.settings["target"]["frame"], json!([1080, 1920]));
    assert_eq!(o.settings["video"]["bitrate_bps"], 12_000_000);
    // The picture is 1080x608 at y = 656; the background fills above and below it.
    let tmp = tempfile::tempdir().unwrap();
    let frame = frame_at(&t, &o.path, 2.0, tmp.path());
    assert!(near(frame.get_pixel(540, 100), [0x10, 0x10, 0x30], 12));
    assert!(near(frame.get_pixel(540, 1800), [0x10, 0x10, 0x30], 12));
    // From 1 s the camera is solid blue: the bubble's bounding box is in the safe area.
    let (_, min_y, max_x, max_y) = bbox_of(&frame, [0, 0, 255]).expect("the bubble is drawn");
    assert!(
        max_x <= (1080.0 * 0.88) as u32,
        "bubble reaches x = {max_x}"
    );
    assert!(
        max_y <= (1920.0 * 0.80) as u32,
        "bubble reaches y = {max_y}"
    );
    assert!(
        min_y >= 656,
        "the bubble is on the picture, not on the background"
    );
}

/// EXP-2: the other platform presets have their frames, bitrates and -14 LUFS audio.
#[test]
fn exp2_platform_presets_have_their_frames_bitrates_and_loudness() {
    let Some(t) = tools() else { return };
    if !gpu_available() {
        return;
    }
    let f = fixture(&t, 4, "1280x720", false, true);
    let m = with_edits(&f, json!({"audio": {"mic_mute": true}}));
    for (id, size, bps) in [
        ("youtube-1080p", (1920u64, 1080u64), 8_000_000u64),
        ("youtube-1440p", (2560, 1440), 16_000_000),
        ("square-1080", (1080, 1080), 8_000_000),
    ] {
        let o = run_preset(&t, &f, &m, id, ExportOptions::default()).unwrap();
        assert_eq!(
            video_info(&t, &o.path),
            (size.0, size.1, "h264".into()),
            "{id}"
        );
        assert_eq!(o.settings["video"]["bitrate_bps"], bps, "{id}");
        let d = duration_of(&t, &o.path);
        assert!((d - 4.0).abs() <= 1.0 / 30.0 + 0.02, "{id} duration {d}");
        // Platform presets normalise to -14 LUFS (within 1 LU) under a -0.5 dBTP ceiling.
        let s = pcm_stereo(&t, &o.path);
        let lufs = cm_audio::mix::integrated_lufs(&s, 2, 48_000).expect("audible output");
        assert!((lufs + 14.0).abs() <= 1.0, "{id}: {lufs:.2} LUFS");
        assert_eq!(
            o.settings["audio"]["loudness"]["target_lufs"], -14.0,
            "{id}"
        );
        assert!(cm_audio::mix::true_peak_db(&s, 2) <= -0.5, "{id}");
        // Stored as that preset's own manifest entry.
        let e = o.to_entry("2026-10-22T10:00:00Z");
        assert_eq!(e["preset"], id);
    }
}

/// EXP-2.AC3: a recording smaller than 2160p cannot be exported as `youtube-4k`, and the reason
/// names the preset to use instead. Nothing is written.
#[test]
fn exp2_ac3_youtube_4k_is_refused_for_a_smaller_recording() {
    let Some(t) = tools() else { return };
    let f = fixture(&t, 2, "1280x720", false, false);
    let e = run_preset(&t, &f, &f.manifest, "youtube-4k", ExportOptions::default()).unwrap_err();
    assert_eq!(e.code(), cm_core::ErrorCode::InvalidInput);
    assert!(e.to_string().contains("`source`"), "{e}");
    let exports = f.path().join("exports");
    assert!(!exports.exists() || std::fs::read_dir(exports).unwrap().next().is_none());
    // The same check without FFmpeg, as `export_video` does it before queuing a job.
    let presets = Presets::builtin();
    let e = cm_media::check_request(
        &f.manifest,
        &PresetRequest {
            presets: &presets,
            id: "youtube-4k",
            options: ExportOptions::default(),
        },
    )
    .unwrap_err();
    assert!(e.to_string().contains("enlarge"), "{e}");
}

/// EXP-1 `source`: the recording's own size and fps, no loudness change, named `- source.mp4`.
#[test]
fn exp2_source_keeps_the_master_size_and_the_levels() {
    let Some(t) = tools() else { return };
    let f = fixture(&t, 3, "1280x720", false, true);
    let o = run_preset(&t, &f, &f.manifest, "source", ExportOptions::default()).unwrap();
    assert_eq!(o.preset, "source");
    assert!(o.file.ends_with(" - source.mp4"), "{}", o.file);
    assert_eq!(video_info(&t, &o.path), (1280, 720, "h264".into()));
    assert!(o.settings["video"]["encoder"].is_string());
    let e = o.to_entry("2026-10-22T10:00:00Z");
    assert_eq!(e["preset"], "source");
}

/// EXP-3.AC1: a 10 s clip at 800 px and 15 fps is a looping GIF of 150 frames (± 1), 800x450.
#[test]
fn exp3_ac1_gif_has_150_frames_and_loops_forever() {
    let Some(t) = tools() else { return };
    if !gpu_available() {
        return;
    }
    let f = fixture(&t, 10, "1280x720", false, true);
    let o = run_preset(&t, &f, &versioned(&f), "gif", ExportOptions::default()).unwrap();
    assert!(o.file.ends_with(" - gif.gif"), "{}", o.file);
    assert_eq!(video_info(&t, &o.path), (800, 450, "gif".into()));
    assert_eq!(audio_codec(&t, &o.path), None, "a GIF has no audio");
    let frames = decoded_frames(&t, &o.path);
    assert!((149..=151).contains(&frames), "{frames} frames");
    // The NETSCAPE2.0 extension with loop count 0 = forever.
    let bytes = std::fs::read(&o.path).unwrap();
    let at = bytes
        .windows(11)
        .position(|w| w == b"NETSCAPE2.0")
        .expect("no loop extension");
    assert_eq!(&bytes[at + 11..at + 15], [3, 1, 0, 0], "loop count");
    assert_eq!(o.encoder, "gif");
    assert!(o.lgpl_safe);
    // No scratch files are left behind.
    let leftovers: Vec<_> = std::fs::read_dir(f.path().join("exports"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".part") || n.starts_with(".work"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

/// EXP-3.AC2: "fit under N MB" steps the GIF down until the file is under the limit.
#[test]
fn exp3_ac2_gif_fits_under_the_size_limit() {
    let Some(t) = tools() else { return };
    if !gpu_available() {
        return;
    }
    let f = fixture(&t, 4, "1280x720", false, false);
    let free = run_preset(&t, &f, &versioned(&f), "gif", ExportOptions::default()).unwrap();
    let limit_mb = free.size_bytes as f64 * 0.5 / 1e6;
    let o = run_preset(
        &t,
        &f,
        &versioned(&f),
        "gif",
        ExportOptions {
            max_mb: Some(limit_mb),
            ..ExportOptions::default()
        },
    )
    .unwrap();
    assert!(
        o.size_bytes as f64 <= limit_mb * 1e6,
        "{} bytes over {limit_mb} MB (unlimited: {})",
        o.size_bytes,
        free.size_bytes
    );
    let (w, _, _) = video_info(&t, &o.path);
    assert!(w < 800, "the width came down, got {w}");
    // A limit nothing can meet is an error that says what to do.
    let e = run_preset(
        &t,
        &f,
        &versioned(&f),
        "gif",
        ExportOptions {
            max_mb: Some(0.1),
            ..ExportOptions::default()
        },
    );
    if let Err(e) = e {
        assert!(e.to_string().contains("WebM"), "{e}");
    }
}

/// EXP-3: a GIF longer than 30 s is refused with the hint, before any work.
#[test]
fn exp3_gif_over_30_s_is_refused() {
    let Some(t) = tools() else { return };
    let f = fixture(&t, 32, "320x180", false, false);
    let e = run_preset(&t, &f, &versioned(&f), "gif", ExportOptions::default()).unwrap_err();
    assert_eq!(e.code(), cm_core::ErrorCode::InvalidInput);
    assert!(
        e.to_string()
            .contains("Use MP4 or WebM for clips longer than 30 s"),
        "{e}"
    );
    // Trimmed to 10 s it is fine to ask (the check alone, no render).
    let presets = Presets::builtin();
    let trimmed = with_edits(&f, json!({"in": 390 * MS, "out": 10_390 * MS}));
    assert!(cm_media::check_request(
        &trimmed,
        &PresetRequest {
            presets: &presets,
            id: "gif",
            options: ExportOptions::default()
        }
    )
    .is_ok());
}

/// EXP-3.AC3: WebM is VP9 + Opus in a WebM container, the asked width, the right duration.
#[test]
fn exp3_ac3_webm_is_vp9_and_opus_at_the_chosen_width() {
    let Some(t) = tools() else { return };
    if !gpu_available() {
        return;
    }
    let f = fixture(&t, 4, "1280x720", false, true);
    let m = with_edits(&f, json!({"audio": {"mic_mute": true}}));
    let o = run_preset(
        &t,
        &f,
        &m,
        "webm",
        ExportOptions {
            width: Some(640),
            fps: Some(24.0),
            ..ExportOptions::default()
        },
    )
    .unwrap();
    assert!(o.file.ends_with(" - webm.webm"), "{}", o.file);
    assert_eq!(video_info(&t, &o.path), (640, 360, "vp9".into()));
    assert_eq!(audio_codec(&t, &o.path), Some(("opus".into(), 48_000)));
    let j = ffprobe_json(&t, &o.path);
    assert!(j["format"]["format_name"]
        .as_str()
        .unwrap()
        .contains("webm"));
    let d = duration_of(&t, &o.path);
    assert!((d - 4.0).abs() <= 1.0 / 24.0 + 0.05, "duration {d}");
    assert_eq!(o.encoder, "libvpx-vp9");
    assert_eq!(o.settings["fps"], 24.0);
}

/// EXP-3.AC2 for WebM: "fit under N MB" re-encodes at a lower bitrate until it fits.
#[test]
fn exp3_ac2_webm_fits_under_the_size_limit() {
    let Some(t) = tools() else { return };
    if !gpu_available() {
        return;
    }
    let f = fixture(&t, 4, "1280x720", false, false);
    let free = run_preset(&t, &f, &versioned(&f), "webm", ExportOptions::default()).unwrap();
    let limit_mb = free.size_bytes as f64 * 0.5 / 1e6;
    let o = run_preset(
        &t,
        &f,
        &versioned(&f),
        "webm",
        ExportOptions {
            max_mb: Some(limit_mb),
            ..ExportOptions::default()
        },
    )
    .unwrap();
    assert!(
        o.size_bytes as f64 <= limit_mb * 1e6,
        "{} bytes over {limit_mb} MB (unlimited: {})",
        o.size_bytes,
        free.size_bytes
    );
    assert!(o.settings["video"]["bitrate_bps"].is_u64());
}

/// Options that do not belong to the preset are refused.
#[test]
fn width_on_an_mp4_preset_is_refused() {
    let Some(t) = tools() else { return };
    let f = fixture(&t, 2, "1280x720", false, false);
    let e = run_preset(
        &t,
        &f,
        &f.manifest,
        "youtube-1080p",
        ExportOptions {
            width: Some(640),
            ..ExportOptions::default()
        },
    )
    .unwrap_err();
    assert!(e.to_string().contains("width"), "{e}");
}

/// Timing of every preset on one 30 s 1080p fixture (screen, bubble, mic and system audio). Run with
/// `cargo test -p cm-media --test quick_export timing_presets -- --ignored --nocapture`.
#[test]
#[ignore = "timing probe; prints the export time of each preset for a 30 s 1080p fixture"]
fn timing_presets_30_s_1080p_fixture() {
    let Some(t) = tools() else { return };
    if !gpu_available() {
        return;
    }
    let mut f = fixture(&t, 30, "1920x1080", true, true);
    f.manifest["tracks"]["screen"]["size"] = json!([1920, 1080]);
    f.manifest["tracks"]["screen"]["source_size"] = json!([1920, 1080]);
    f.manifest["tracks"]["screen"]["scale"] = json!(1.0);
    let m = with_edits(&f, json!({}));
    for (id, opts) in [
        ("youtube-1080p", ExportOptions::default()),
        ("vertical-1080", ExportOptions::default()),
        ("square-1080", ExportOptions::default()),
        ("source", ExportOptions::default()),
        (
            "gif",
            ExportOptions {
                max_mb: Some(10.0),
                ..ExportOptions::default()
            },
        ),
        ("webm", ExportOptions::default()),
    ] {
        let o = run_preset(&t, &f, &m, id, opts).unwrap();
        let secs = o.duration_ms as f64 / 1000.0;
        eprintln!(
            "TIMING: {id}: {secs:.1} s via {} in {} ms ({:.1}x real time), {} bytes",
            o.encoder,
            o.elapsed_ms,
            secs * 1000.0 / o.elapsed_ms as f64,
            o.size_bytes
        );
    }
}

// ---- the export starts at `live_at` (REC-1 default in-point) -----------------------------------

fn with_live_at(f: &Fixture, live_at_ms: Option<i64>) -> Value {
    let mut m = f.manifest.clone();
    match live_at_ms {
        Some(ms) => m["live_at"] = json!(ms * MS),
        None => {
            m.as_object_mut().unwrap().remove("live_at");
        }
    }
    m
}

/// A copy of an export: the next export of the same bundle replaces the file in `exports/`.
fn keep(o: &cm_media::QuickOutcome, dir: &Path, name: &str) -> PathBuf {
    let to = dir.join(name);
    std::fs::copy(&o.path, &to).unwrap();
    to
}

fn export_manifest(
    t: &Tools,
    f: &Fixture,
    m: &Value,
    path: cm_media::ExportPath,
) -> cm_media::QuickOutcome {
    export_quick(
        t,
        &QuickRequest {
            captions: None,
            bundle_dir: f.path(),
            manifest: m,
            prefer_encoder: None,
            path,
            loudness_lufs: None,
            preset: None,
        },
        &mut |_| {},
        &|| false,
    )
    .unwrap_or_else(|e| panic!("export failed: {}", e.into_cm()))
}

/// The untouched export starts at the manifest's `live_at` (here the camera's first frame, 190 ms
/// after the screen's): it is that much shorter, and flash, beep and the camera event move by the
/// same amount, so the tracks stay in step (NFR-3).
#[test]
fn exp1_untouched_export_starts_at_live_at_and_stays_in_step() {
    let Some(t) = tools() else { return };
    let f = fixture(&t, 4, "1280x720", true, true);
    let keep_dir = tempfile::tempdir().unwrap();
    let before = export_manifest(
        &t,
        &f,
        &with_live_at(&f, None),
        cm_media::ExportPath::Ffmpeg,
    );
    let d0 = duration_of(&t, &keep(&before, keep_dir.path(), "before.mp4"));
    let m = with_live_at(&f, Some(CAMERA_START_MS));
    let o = export_manifest(&t, &f, &m, cm_media::ExportPath::Ffmpeg);
    let cut = (CAMERA_START_MS - SCREEN_START_MS) as f64 / 1000.0;
    let d1 = duration_of(&t, &o.path);
    assert!(
        (d0 - d1 - cut).abs() <= 1.5 / f64::from(FPS),
        "{d0} -> {d1}"
    );

    let flash_t = flash_frame(&t, &o.path) as f64 / f64::from(FPS);
    let beep_t = beep_time(&t, &o.path);
    assert!(
        (flash_t - (FLASH_FILE_S - cut)).abs() <= 1.5 / f64::from(FPS),
        "flash {flash_t}"
    );
    assert!(
        (beep_t - (FLASH_FILE_S - cut)).abs() <= 0.030,
        "beep {beep_t}"
    );
    assert!((beep_t - flash_t).abs() <= 0.050, "A/V offset");
    // The camera starts at the origin now: its colour change is 1.0 s into the output.
    let centre = region_means(&t, &o.path, "8:8:1020:520");
    let first = centre[10];
    let drop = centre
        .iter()
        .position(|m| i32::from(*m) < i32::from(first) - 25)
        .expect("camera colour change found");
    assert!(
        (drop as f64 / f64::from(FPS) - CAMERA_EVENT_FILE_S).abs() <= 2.0 / f64::from(FPS),
        "camera event at {}",
        drop as f64 / f64::from(FPS)
    );
}

/// Without a `live_at` (older recordings), or with one outside the screen track, the start is the
/// screen's first frame, as before: the same length and the flash at 1.0 s.
#[test]
fn exp1_without_a_usable_live_at_the_screen_start_is_kept() {
    let Some(t) = tools() else { return };
    let f = fixture(&t, 4, "1280x720", true, true);
    let base = export_manifest(
        &t,
        &f,
        &with_live_at(&f, None),
        cm_media::ExportPath::Ffmpeg,
    );
    let d0 = duration_of(&t, &base.path);
    for live in [Some(100), Some(SCREEN_START_MS), Some(60_000)] {
        let o = export_manifest(
            &t,
            &f,
            &with_live_at(&f, live),
            cm_media::ExportPath::Ffmpeg,
        );
        let d = duration_of(&t, &o.path);
        assert!(
            (d - d0).abs() <= 1.0 / f64::from(FPS),
            "{live:?}: {d} vs {d0}"
        );
        let flash_t = flash_frame(&t, &o.path) as f64 / f64::from(FPS);
        assert!(
            (flash_t - FLASH_FILE_S).abs() <= 1.5 / f64::from(FPS),
            "{live:?}: {flash_t}"
        );
    }
}

/// A slow machine: the camera only delivers 2.5 s after the screen, so `live_at` is 2.5 s in. The
/// export loses that pre-roll, the camera event is 1.0 s into the output, the beep (before the
/// origin) is gone, and the audio is as long as the video.
#[test]
fn exp1_a_2_5_s_pre_roll_is_cut_and_the_tracks_stay_aligned() {
    let Some(t) = tools() else { return };
    let mut f = fixture(&t, 4, "1280x720", true, true);
    let base = export_manifest(
        &t,
        &f,
        &with_live_at(&f, None),
        cm_media::ExportPath::Ffmpeg,
    );
    let d0 = duration_of(&t, &base.path);
    let cam_start = SCREEN_START_MS + 2500;
    f.manifest["tracks"]["camera"]["start"] = json!(cam_start * MS);
    f.manifest["tracks"]["camera"]["end"] = json!((cam_start + 4000) * MS);
    let m = with_live_at(&f, Some(cam_start));
    let o = export_manifest(&t, &f, &m, cm_media::ExportPath::Ffmpeg);
    let d = duration_of(&t, &o.path);
    assert!((d0 - d - 2.5).abs() <= 1.5 / f64::from(FPS), "{d0} -> {d}");
    let centre = region_means(&t, &o.path, "8:8:1020:520");
    let first = centre[10];
    let drop = centre
        .iter()
        .position(|m| i32::from(*m) < i32::from(first) - 25)
        .expect("camera colour change found");
    assert!(
        (drop as f64 / f64::from(FPS) - CAMERA_EVENT_FILE_S).abs() <= 2.0 / f64::from(FPS),
        "camera event at {}",
        drop as f64 / f64::from(FPS)
    );
    let samples = pcm(&t, &o.path);
    assert!(
        samples.iter().all(|s| s.abs() <= 0.3),
        "the beep before the origin is gone"
    );
    assert!(
        (samples.len() as f64 / 48_000.0 - d).abs() <= 0.05,
        "audio and video lengths"
    );
}

/// The two renderers agree: an edit list that starts at `live_at` through the compositor and the
/// untouched FFmpeg-graph export have the same length, and flash and beep land at the same
/// instants (the reported duration is the edit list's, so this is also reported = exported).
#[test]
fn exp1_untouched_and_compositor_exports_have_the_same_start_and_length() {
    let Some(t) = tools() else { return };
    if !gpu_available() {
        return;
    }
    let f = fixture(&t, 4, "1280x720", false, true);
    let m = with_live_at(&f, Some(CAMERA_START_MS));
    let keep_dir = tempfile::tempdir().unwrap();
    let quick = export_manifest(&t, &f, &m, cm_media::ExportPath::Ffmpeg);
    let quick_path = keep(&quick, keep_dir.path(), "quick.mp4");
    let mut edited = with_edits(&f, json!({"in": CAMERA_START_MS * MS}));
    edited["live_at"] = json!(CAMERA_START_MS * MS);
    let comp = export_manifest(&t, &f, &edited, cm_media::ExportPath::Compositor);
    assert_eq!(comp.settings["renderer"], "compositor");
    let (dq, dc) = (duration_of(&t, &quick_path), duration_of(&t, &comp.path));
    assert!(
        (dq - dc).abs() <= 1.5 / f64::from(FPS),
        "quick {dq} vs compositor {dc}"
    );
    let (fq, fc) = (flash_frame(&t, &quick_path), flash_frame(&t, &comp.path));
    assert!(fq.abs_diff(fc) <= 1, "flash frame {fq} vs {fc}");
    assert!((beep_time(&t, &quick_path) - beep_time(&t, &comp.path)).abs() <= 0.04);
}
