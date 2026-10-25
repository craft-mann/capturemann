//! The real exporter behind the daemon, on a bundle synthesized with FFmpeg (skipped when FFmpeg
//! is not installed): `export_video` -> job -> `exports/<title> - quick.mp4`, the manifest
//! `exports[]` entry, the events, the library size, and the missing-FFmpeg failure (EXP-1, EXP-4,
//! REC-1). No recording is made and no camera, microphone or screen is used.

#![allow(clippy::unwrap_used)]

mod common;

use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use cm_capture::Capturer;
use cm_core::{Actor, CmError, MediaTime, TrackInfo, TrackKind, TrackStats};
use cm_daemon::Daemon;
use cm_library::rec::SessionInfo;
use cm_library::Library;
use cm_media::tools::{locate, locate_in, SearchPaths, Tools};
use serde_json::{json, Value};
use time::macros::datetime;

fn ffmpeg() -> Option<Tools> {
    match locate() {
        Ok(t) => Some(t),
        Err(_) => {
            eprintln!("SKIPPED: FFmpeg not found (brew install ffmpeg)");
            None
        }
    }
}

fn run(t: &Tools, args: &[&str]) {
    let o = Command::new(&t.ffmpeg)
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .args(args)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
}

const MS: i64 = 120;

/// A finished library with one recording: 3 s screen, camera, mic. Returns the daemon library
/// root and the capture id.
fn library_with_recording(t: &Tools) -> (tempfile::TempDir, Library, String) {
    library_with_live_at(t, None)
}

/// The same recording with a manifest `live_at` (the moment every track was delivering).
fn library_with_live_at(
    t: &Tools,
    live_at_ms: Option<i64>,
) -> (tempfile::TempDir, Library, String) {
    let root = tempfile::tempdir().unwrap();
    let mut lib = Library::open_or_create(root.path()).unwrap();
    let now = datetime!(2026-10-21 10:00:00 UTC);
    let session = SessionInfo {
        clock_origin_ns: 1,
        timebase: "uptime_raw_ns".into(),
        displays: vec![],
        devices: json!({}),
        app_version: "0.0.1".into(),
        extra: Default::default(),
    };
    let settings = json!({"camera": {"shape": "circle", "size": "m", "position": "bottom-right", "mirror": true}});
    let bundle = lib
        .create_recording(Some("Real export"), now, settings, &session)
        .unwrap();
    let mut bundle = bundle;
    if let Some(ms) = live_at_ms {
        bundle.set_extra("live_at", json!(ms * MS)).unwrap();
    }
    let d = bundle.dir().to_owned();
    let p = |n: &str| d.join(n).to_string_lossy().into_owned();
    run(
        t,
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=640x360:rate=30:duration=3",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-pix_fmt",
            "yuv420p",
            "-movflags",
            "frag_keyframe+empty_moov",
            &p("screen.mp4"),
        ],
    );
    run(
        t,
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=640x360:rate=30:duration=3",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-pix_fmt",
            "yuv420p",
            "-movflags",
            "frag_keyframe+empty_moov",
            &p("camera.mp4"),
        ],
    );
    run(
        t,
        &[
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=3.2:sample_rate=48000",
            "-ac",
            "1",
            "-c:a",
            "pcm_f32le",
            &p("mic.denoised.wav"),
        ],
    );
    let track = |kind, file: &str, start_ms: i64, secs: f64, details: Value| TrackInfo {
        kind,
        file: file.into(),
        start: MediaTime::from_ticks(start_ms * MS),
        end: MediaTime::from_ticks(start_ms * MS + (secs * 1000.0) as i64 * MS),
        details,
        stats: TrackStats::default(),
    };
    let tracks = vec![
        track(
            TrackKind::Screen,
            "screen.mp4",
            300,
            3.0,
            json!({"size": [640, 360], "fps": 30, "source_size": [640, 360], "scale": 1.0, "codec": "h264"}),
        ),
        track(
            TrackKind::Camera,
            "camera.mp4",
            450,
            3.0,
            json!({"size": [640, 360], "mirror": true}),
        ),
        track(
            TrackKind::Mic,
            "mic.denoised.wav",
            100,
            3.2,
            json!({"rate": 48000, "channels": 1}),
        ),
    ];
    let rec = lib.finish_recording(bundle, &tracks, now).unwrap();
    (root, lib, rec.id)
}

fn daemon_for(lib: Library, tools: cm_daemon::export::ToolsFn) -> Daemon {
    let queue = cm_daemon::library_job_queue(lib.root()).unwrap();
    let d = Daemon::builder(Capturer::new(common::desktop()).with_own_pid(None), lib)
        .jobs(queue)
        .build();
    d.register_job_handler(cm_jobs::JOB_EXPORT, d.export_handler_with(tools))
        .unwrap();
    d.start_jobs().unwrap();
    d
}

async fn call(d: &Daemon, m: &str, p: Value) -> Value {
    d.execute_method(Actor::User, 0, m, &p).await.unwrap()
}

async fn wait_state(d: &Daemon, id: &str, state: &str) -> Value {
    for _ in 0..1500 {
        let j = call(d, "get_job", json!({ "job_id": id })).await;
        if j["state"] == state {
            return j;
        }
        if j["state"] == "failed" && state != "failed" {
            panic!("job failed: {j}");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("job did not reach {state}");
}

/// EXP-1.AC3 / EXP-4: the whole path with the real exporter. The manifest lists the export, the
/// masters are untouched, the library size grows, progress and events arrive.
#[tokio::test]
async fn export_video_renders_records_and_announces() {
    let Some(t) = ffmpeg() else { return };
    let (root, lib, id) = library_with_recording(&t);
    let screen = root.path().join("Real export.cmrec/screen.mp4");
    let before = std::fs::read(&screen).unwrap();
    let size_before = lib.get(&id).unwrap().size_bytes;
    let d = daemon_for(lib, Arc::new(locate));
    let mut rx = d.subscribe();

    let r = call(&d, "export_video", json!({ "capture_id": id })).await;
    let job = r["job_id"].as_str().unwrap().to_owned();
    let done = wait_state(&d, &job, "done").await;
    let res = &done["result"];
    assert_eq!(res["preset"], "quick");
    assert_eq!(res["file"], "exports/Real export - quick.mp4");
    let out = root
        .path()
        .join("Real export.cmrec/exports/Real export - quick.mp4");
    assert!(out.is_file());
    assert_eq!(res["path"], out.to_string_lossy().as_ref());
    assert!(
        (res["duration_s"].as_f64().unwrap() - 3.0).abs() < 0.2,
        "{res}"
    );
    assert!(res["size_bytes"].as_u64().unwrap() > 1000);

    // Masters untouched; manifest entry; revision; index size.
    assert_eq!(std::fs::read(&screen).unwrap(), before);
    let m: Value = serde_json::from_str(
        &std::fs::read_to_string(root.path().join("Real export.cmrec/manifest.json")).unwrap(),
    )
    .unwrap();
    let e = &m["exports"][0];
    assert_eq!(e["file"], "exports/Real export - quick.mp4");
    assert_eq!(e["preset"], "quick");
    assert!(e["encoder"] == "h264_videotoolbox" || e["encoder"] == "libx264");
    assert!(e["created"].as_str().unwrap().contains('T'));
    assert_eq!(e["settings"]["size"], json!([640, 360]));
    assert_eq!(
        m["revisions"].as_array().unwrap().last().unwrap()["action"],
        "export"
    );
    let lib_row = call(&d, "list_library", json!({ "kind": "recording" })).await;
    let row = &lib_row["captures"][0];
    assert!(row["size_bytes"].as_u64().unwrap() > size_before, "{row}");

    // Events: export.queued, job.progress (maybe), export.done, capture.revised, job.done.
    let mut names = Vec::new();
    while let Ok(ev) = rx.try_recv() {
        names.push(ev.name);
    }
    for n in [
        "export.queued",
        "export.done",
        "capture.revised",
        "job.done",
    ] {
        assert!(names.iter().any(|x| x == n), "{n} in {names:?}");
    }

    // A second export of the same preset replaces the entry, no duplicate.
    let r2 = call(&d, "export_video", json!({ "capture_id": id })).await;
    wait_state(&d, r2["job_id"].as_str().unwrap(), "done").await;
    let m: Value = serde_json::from_str(
        &std::fs::read_to_string(root.path().join("Real export.cmrec/manifest.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(m["exports"].as_array().unwrap().len(), 1);
}

/// FFmpeg missing: the job fails once (no retry) with the install hint, the recording is unaffected.
#[tokio::test]
async fn missing_ffmpeg_fails_the_job_once_with_the_hint() {
    // No FFmpeg needed: the lookup is replaced by one that finds nothing.
    let root = tempfile::tempdir().unwrap();
    let mut lib = Library::open_or_create(root.path()).unwrap();
    let now = datetime!(2026-10-21 10:00:00 UTC);
    let session = SessionInfo {
        clock_origin_ns: 1,
        timebase: "uptime_raw_ns".into(),
        displays: vec![],
        devices: json!({}),
        app_version: "0.0.1".into(),
        extra: Default::default(),
    };
    let bundle = lib
        .create_recording(Some("NoFF"), now, json!({}), &session)
        .unwrap();
    std::fs::write(bundle.dir().join("screen.mp4"), b"not really a video").unwrap();
    let tracks = vec![TrackInfo {
        kind: TrackKind::Screen,
        file: "screen.mp4".into(),
        start: MediaTime::from_ticks(1000),
        end: MediaTime::from_ticks(361_000),
        details: json!({"size": [640, 360], "fps": 30}),
        stats: TrackStats::default(),
    }];
    let rec = lib.finish_recording(bundle, &tracks, now).unwrap();
    let lookup: cm_daemon::export::ToolsFn = Arc::new(|| -> Result<Tools, CmError> {
        locate_in(&SearchPaths {
            env: Some("/nonexistent/ffmpeg".into()),
            ..Default::default()
        })
    });
    let d = daemon_for(lib, lookup);
    let r = call(&d, "export_video", json!({ "capture_id": rec.id })).await;
    let job = wait_state(&d, r["job_id"].as_str().unwrap(), "failed").await;
    assert_eq!(job["attempts"], 1, "not retried");
    assert_eq!(job["error"]["code"], "not_found");
    let reason = job["error"]["reason"].as_str().unwrap();
    assert!(reason.contains("FFmpeg was not found"), "{reason}");
    assert!(
        root.path().join("NoFF.cmrec/screen.mp4").is_file(),
        "recording unaffected"
    );
    assert!(
        !root.path().join("NoFF.cmrec/exports").exists()
            || std::fs::read_dir(root.path().join("NoFF.cmrec/exports"))
                .unwrap()
                .next()
                .is_none()
    );
}

/// EDT-1 through the daemon: the `trim` action writes the edit list and the next `export_video`
/// honours it with the compositor (skipped without FFmpeg or a GPU adapter).
#[tokio::test]
async fn trim_then_export_video_renders_the_trimmed_range() {
    let Some(t) = ffmpeg() else { return };
    if let Err(e) = cm_render::Compositor::new() {
        eprintln!("SKIPPED: no GPU compositor ({e})");
        return;
    }
    let (root, lib, id) = library_with_recording(&t);
    let d = daemon_for(lib, Arc::new(locate));
    call(
        &d,
        "trim",
        json!({ "capture_id": id, "start": 1.0, "end": 2.5 }),
    )
    .await;
    let r = call(&d, "export_video", json!({ "capture_id": id })).await;
    let done = wait_state(&d, r["job_id"].as_str().unwrap(), "done").await;
    let secs = done["result"]["duration_s"].as_f64().unwrap();
    assert!((secs - 1.5).abs() < 0.1, "{done}");
    let m: Value = serde_json::from_str(
        &std::fs::read_to_string(root.path().join("Real export.cmrec/manifest.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(m["exports"][0]["settings"]["renderer"], "compositor");
}

/// The export starts at the manifest's `live_at` on both renderers, and the duration the edit
/// actions report is the length of the exported file (REC-1: default in-point is `live_at`).
#[tokio::test]
async fn export_starts_at_live_at_and_matches_the_reported_duration() {
    let Some(t) = ffmpeg() else { return };
    if let Err(e) = cm_render::Compositor::new() {
        eprintln!("SKIPPED: no GPU compositor ({e})");
        return;
    }
    // Screen 0.30..3.30 s, camera 0.45 s: live_at = 0.45 s, so 2.85 s of the 3.0 s file remain.
    let (root, lib, id) = library_with_live_at(&t, Some(450));
    let renderer = || -> Value {
        let m: Value = serde_json::from_str(
            &std::fs::read_to_string(root.path().join("Real export.cmrec/manifest.json")).unwrap(),
        )
        .unwrap();
        m["exports"][0]["settings"]["renderer"].clone()
    };
    let d = daemon_for(lib, Arc::new(locate));
    let exported = |d: &Daemon, id: String| {
        let d = d.clone();
        async move {
            let r = call(&d, "export_video", json!({ "capture_id": id })).await;
            let done = wait_state(&d, r["job_id"].as_str().unwrap(), "done").await;
            done["result"].clone()
        }
    };

    // Untouched: the FFmpeg-graph path starts at live_at and the library shows the same length.
    let out = exported(&d, id.clone()).await;
    let secs = out["duration_s"].as_f64().unwrap();
    assert!((secs - 2.85).abs() <= 1.5 / 30.0, "exported {secs}");
    assert_eq!(renderer(), Value::Null, "{out}");
    let rows = call(&d, "list_library", json!({ "kind": "recording" })).await;
    if let Some(shown) = rows["captures"][0]["duration_s"].as_f64() {
        assert!(
            (shown - secs).abs() <= 1.5 / 30.0,
            "library {shown} vs exported {secs}"
        );
    }

    // A split stores an edit list: the compositor path, the same start and the reported length.
    let edit = call(&d, "split", json!({ "capture_id": id, "at": 1.5 })).await;
    let reported = edit["duration_s"].as_f64().unwrap();
    let out = exported(&d, id.clone()).await;
    let secs = out["duration_s"].as_f64().unwrap();
    assert!((reported - 2.85).abs() < 0.02, "reported {reported}");
    assert!(
        (secs - reported).abs() <= 1.5 / 30.0,
        "exported {secs} vs reported {reported}"
    );
    assert_eq!(renderer(), "compositor", "{out}");

    // A background on top of it: still the compositor path, still the same start and length.
    let edit = call(
        &d,
        "set_background",
        json!({ "capture_id": id, "kind": "color", "value": "#101820", "padding_pct": 5 }),
    )
    .await;
    let reported = edit["duration_s"].as_f64().unwrap();
    let out = exported(&d, id.clone()).await;
    let secs = out["duration_s"].as_f64().unwrap();
    assert!((reported - 2.85).abs() < 0.02, "reported {reported}");
    assert!(
        (secs - reported).abs() <= 1.5 / 30.0,
        "exported {secs} vs reported {reported}"
    );
    assert_eq!(renderer(), "compositor", "{out}");
}

// ---- burned-in captions (AI-1) ---------------------------------------------------------------

/// Pixels that differ between two frames in a region (a caption box and text are the difference).
fn diff_in(a: &image::RgbImage, b: &image::RgbImage, region: (u32, u32, u32, u32)) -> usize {
    let mut n = 0;
    for y in region.1..region.3.min(a.height()) {
        for x in region.0..region.2.min(a.width()) {
            let (p, q) = (a.get_pixel(x, y).0, b.get_pixel(x, y).0);
            if p.iter().zip(q).any(|(u, v)| u.abs_diff(v) > 40) {
                n += 1;
            }
        }
    }
    n
}

fn png_frame(
    t: &Tools,
    video: &str,
    at_s: f64,
    tmp: &std::path::Path,
    tag: &str,
) -> image::RgbImage {
    let png = tmp.join(format!("{tag}.png"));
    run(
        t,
        &[
            "-ss",
            &format!("{at_s}"),
            "-i",
            video,
            "-frames:v",
            "1",
            png.to_str().unwrap(),
        ],
    );
    image::open(&png).unwrap().to_rgb8()
}

/// A transcript of three words spoken at session 1.0 to 1.9 s, written into the bundle.
fn write_transcript(root: &std::path::Path) {
    let word = |text: &str, from: f64| {
        json!({"text": text, "start": (from * 120_000.0) as i64,
               "end": ((from + 0.3) * 120_000.0) as i64, "confidence": 0.9})
    };
    let t = json!({"version": 1, "language": "en", "model": "small", "note": null,
        "words": [word("Hello", 1.0), word("there", 1.3), word("friend", 1.6)]});
    std::fs::write(
        root.join("Real export.cmrec/transcript.json"),
        serde_json::to_vec(&t).unwrap(),
    )
    .unwrap();
}

/// AI-1: asking for `burn` by name without a transcript fails when the request is made, with the
/// way to get one; a bad value is refused; the recording's own `burn_in` default never blocks an
/// export, and `none` leaves the captions out on purpose.
#[tokio::test]
async fn ai1_burn_needs_a_transcript_but_the_default_never_blocks() {
    let Some(t) = ffmpeg() else { return };
    let (_root, lib, id) = library_with_recording(&t);
    let d = daemon_for(lib, Arc::new(locate));
    let err = d
        .execute_method(
            Actor::User,
            0,
            "export_video",
            &json!({ "capture_id": id, "captions": "burn" }),
        )
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("transcribe"),
        "the error says how to get captions: {err}"
    );
    let bad = d
        .execute_method(
            Actor::User,
            0,
            "export_video",
            &json!({ "capture_id": id, "captions": "yes" }),
        )
        .await
        .unwrap_err();
    assert!(bad.to_string().contains("captions"), "{bad}");

    // The recording is set to burn captions in, but has none: the export goes ahead without them.
    call(
        &d,
        "set_caption_options",
        json!({ "capture_id": id, "burn_in": true }),
    )
    .await;
    let r = call(&d, "export_video", json!({ "capture_id": id })).await;
    let done = wait_state(&d, r["job_id"].as_str().unwrap(), "done").await;
    assert_eq!(
        done["result"]["captions"], "skipped_no_transcript",
        "{done}"
    );
    let r = call(
        &d,
        "export_video",
        json!({ "capture_id": id, "captions": "none" }),
    )
    .await;
    let done = wait_state(&d, r["job_id"].as_str().unwrap(), "done").await;
    assert_eq!(done["result"]["captions"], "off", "{done}");
}

/// AI-1: with a transcript, `burn` draws the caption into the exported picture at the right
/// output time after a trim, `none` does not, and the two are different jobs.
#[tokio::test]
async fn ai1_burned_in_captions_follow_the_trim_and_the_choice() {
    let Some(t) = ffmpeg() else { return };
    if let Err(e) = cm_render::Compositor::new() {
        eprintln!("SKIPPED: no GPU compositor ({e})");
        return;
    }
    let (root, lib, id) = library_with_recording(&t);
    write_transcript(root.path());
    let d = daemon_for(lib, Arc::new(locate));
    // Keep session 0.5 to 2.5 s: the words (1.0 to 1.9 s) land at output 0.5 to 1.4 s.
    call(
        &d,
        "trim",
        json!({ "capture_id": id, "start": 0.5, "end": 2.5 }),
    )
    .await;
    let tmp = tempfile::tempdir().unwrap();
    let r = call(
        &d,
        "export_video",
        json!({ "capture_id": id, "captions": "burn" }),
    )
    .await;
    assert_eq!(r["captions"], "burn");
    let done = wait_state(&d, r["job_id"].as_str().unwrap(), "done").await;
    assert_eq!(done["result"]["captions"], "burned", "{done}");
    // Frames are read before the second export, which writes the same file name.
    let file = done["result"]["path"].as_str().unwrap().to_owned();
    let with = png_frame(&t, &file, 0.9, tmp.path(), "with");
    let late = png_frame(&t, &file, 1.8, tmp.path(), "late");

    // The same export without captions is another job (the choice is part of its identity).
    let r2 = call(
        &d,
        "export_video",
        json!({ "capture_id": id, "captions": "none" }),
    )
    .await;
    assert_ne!(r2["job_id"], r["job_id"]);
    let done2 = wait_state(&d, r2["job_id"].as_str().unwrap(), "done").await;
    let plain_file = done2["result"]["path"].as_str().unwrap().to_owned();
    let plain = png_frame(&t, &plain_file, 0.9, tmp.path(), "plain");
    let plain_late = png_frame(&t, &plain_file, 1.8, tmp.path(), "plain-late");
    let (w, h) = (with.width(), with.height());
    assert!(
        diff_in(&with, &plain, (0, h * 2 / 3, w, h)) > 300,
        "the caption is burned in while the words are spoken"
    );
    assert_eq!(
        diff_in(&with, &plain, (0, 0, w, h * 2 / 3)),
        0,
        "nothing above the lower third"
    );
    assert_eq!(
        diff_in(&late, &plain_late, (0, 0, w, h)),
        0,
        "no caption after the line has ended"
    );
}
