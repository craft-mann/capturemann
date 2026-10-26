//! `export_audio` (VOC-2): validation without FFmpeg (stub bundles), and the whole path with the
//! real exporter on a synthesized bundle when FFmpeg has what it needs (otherwise SKIPPED). No
//! recording is made and no microphone, camera or screen is used.

#![allow(clippy::unwrap_used)]

mod common;

use std::process::{Command, Stdio};
use std::time::Duration;

use cm_capture::Capturer;
use cm_core::{Actor, ErrorCode, MediaTime, TrackInfo, TrackKind, TrackStats};
use cm_daemon::{Daemon, DaemonError};
use cm_library::rec::SessionInfo;
use cm_library::Library;
use cm_media::audio_export::{check_components, AudioOptions};
use cm_media::tools::{locate, Tools};
use serde_json::{json, Value};
use time::macros::datetime;

const MS: i64 = 120;

fn session() -> SessionInfo {
    SessionInfo {
        clock_origin_ns: 1,
        timebase: "uptime_raw_ns".into(),
        displays: vec![],
        devices: json!({}),
        app_version: "0.0.1".into(),
        extra: Default::default(),
    }
}

fn track(kind: TrackKind, file: &str, start_ms: i64, secs: f64, details: Value) -> TrackInfo {
    TrackInfo {
        kind,
        file: file.into(),
        start: MediaTime::from_ticks(start_ms * MS),
        end: MediaTime::from_ticks(start_ms * MS + (secs * 1000.0) as i64 * MS),
        details,
        stats: TrackStats::default(),
    }
}

/// A library with one recording of a screen track (never read by the audio export) plus the
/// given audio tracks; `write(path, name)` creates each audio file.
fn library(
    title: &str,
    audio: &[(&str, TrackKind)],
    write: &dyn Fn(&std::path::Path, &str),
) -> (tempfile::TempDir, Library, String) {
    let root = tempfile::tempdir().unwrap();
    let mut lib = Library::open_or_create(root.path()).unwrap();
    let now = datetime!(2026-10-26 10:00:00 UTC);
    let bundle = lib
        .create_recording(Some(title), now, json!({}), &session())
        .unwrap();
    let d = bundle.dir().to_owned();
    std::fs::write(d.join("screen.mp4"), b"not really a video").unwrap();
    let mut tracks = vec![track(
        TrackKind::Screen,
        "screen.mp4",
        0,
        10.0,
        json!({"size": [640, 360], "fps": 30, "source_size": [640, 360], "scale": 1.0}),
    )];
    for (file, kind) in audio {
        write(&d, file);
        let channels = if *kind == TrackKind::Mic { 1 } else { 2 };
        tracks.push(track(
            *kind,
            file,
            0,
            10.0,
            json!({"rate": 48000, "channels": channels}),
        ));
    }
    let rec = lib.finish_recording(bundle, &tracks, now).unwrap();
    (root, lib, rec.id)
}

fn daemon_for(lib: Library) -> Daemon {
    let queue = cm_daemon::library_job_queue(lib.root()).unwrap();
    let d = Daemon::builder(Capturer::new(common::desktop()).with_own_pid(None), lib)
        .jobs(queue)
        .build();
    d.register_job_handler(cm_jobs::JOB_EXPORT, d.export_handler())
        .unwrap();
    d.start_jobs().unwrap();
    d
}

async fn call(d: &Daemon, m: &str, p: Value) -> Result<Value, DaemonError> {
    d.execute_method(Actor::User, 0, m, &p).await
}

fn code(e: &DaemonError) -> ErrorCode {
    match e {
        DaemonError::Cm(c) => c.code(),
        DaemonError::WithData { error, .. } => error.code(),
        DaemonError::NotImplemented(_) => ErrorCode::NotImplemented,
        other => panic!("unexpected {other:?}"),
    }
}

async fn wait_state(d: &Daemon, id: &str, state: &str) -> Value {
    for _ in 0..1500 {
        let j = call(d, "get_job", json!({ "job_id": id })).await.unwrap();
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

/// The validation does not need FFmpeg: the plan is pure.
#[tokio::test]
async fn export_audio_validates_before_queueing_and_reuses_equal_jobs() {
    let dummy = |d: &std::path::Path, n: &str| std::fs::write(d.join(n), b"x").unwrap();
    let (_root, lib, id) = library("Meet", &[("mic.wav", TrackKind::Mic)], &dummy);
    let d = daemon_for(lib);

    // A hold keeps the first job queued so the second finds it.
    let a = call(
        &d,
        "export_audio",
        json!({ "capture_id": id, "format": "wav" }),
    )
    .await
    .unwrap();
    assert_eq!(a["kind"], "audio");
    assert_eq!(a["format"], "wav");
    assert_eq!(a["tracks"], "mix");
    assert!(
        a["loudness_lufs"].is_null(),
        "WAV has no loudness by default: {a}"
    );

    let m4a = call(&d, "export_audio", json!({ "capture_id": id }))
        .await
        .unwrap();
    assert_eq!(m4a["loudness_lufs"], -16.0);

    // No system track: System only has nothing to export.
    let e = call(
        &d,
        "export_audio",
        json!({ "capture_id": id, "tracks": "system" }),
    )
    .await
    .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
    assert!(e.to_string().contains("system audio"), "{e}");

    for (p, field) in [
        (json!({ "capture_id": id, "format": "mp3" }), "format"),
        (json!({ "capture_id": id, "tracks": "x" }), "tracks"),
        (
            json!({ "capture_id": id, "loudness_lufs": 3 }),
            "loudness_lufs",
        ),
        (json!({ "capture_id": id, "out_dir": "rel" }), "out_dir"),
        (
            json!({ "capture_id": id, "out_dir": "/definitely/not/here" }),
            "out_dir",
        ),
    ] {
        let e = call(&d, "export_audio", p).await.unwrap_err();
        assert_eq!(code(&e), ErrorCode::InvalidInput);
        assert!(e.to_string().contains(field), "{e}");
    }
    let e = call(&d, "export_audio", json!({ "capture_id": "rec_nope" }))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::NotFound);
}

fn ffmpeg_for_audio() -> Option<Tools> {
    let t = match locate() {
        Ok(t) if t.ffprobe.is_some() => t,
        _ => {
            eprintln!("SKIPPED: FFmpeg/ffprobe not found (brew install ffmpeg)");
            return None;
        }
    };
    match check_components(&t, &AudioOptions::default()) {
        Ok(()) => Some(t),
        Err(e) => {
            eprintln!("SKIPPED: {e}");
            None
        }
    }
}

fn lavfi_wav(t: &Tools, path: &std::path::Path, src: &str, channels: &str) {
    let o = Command::new(&t.ffmpeg)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            src,
        ])
        .args(["-ac", channels, "-c:a", "pcm_f32le"])
        .arg(path)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
}

/// The whole path: a cut shortens the file, the entry lands in the manifest as `kind: "audio"`,
/// the copy goes to `out_dir` without overwriting, the events arrive.
#[tokio::test]
async fn export_audio_runs_records_copies_and_announces() {
    let Some(t) = ffmpeg_for_audio() else { return };
    let tc = t.clone();
    let write = move |d: &std::path::Path, n: &str| {
        let (src, ch) = if n.starts_with("mic") {
            ("anoisesrc=d=10:c=pink:r=48000:a=0.2:seed=7", "1")
        } else {
            ("sine=frequency=1013:duration=10:sample_rate=48000", "2")
        };
        lavfi_wav(&tc, &d.join(n), src, ch);
    };
    let (root, lib, id) = library(
        "Call",
        &[
            ("mic.wav", TrackKind::Mic),
            ("system.wav", TrackKind::System),
        ],
        &write,
    );
    let d = daemon_for(lib);
    let mut rx = d.subscribe();
    // Cut 2 s..4 s: 8 s remain.
    call(
        &d,
        "cut_segments",
        json!({ "capture_id": id, "ranges": [[2, 4]] }),
    )
    .await
    .unwrap();

    let out_dir = tempfile::tempdir().unwrap();
    let r = call(
        &d,
        "export_audio",
        json!({ "capture_id": id, "tracks": "mic", "out_dir": out_dir.path() }),
    )
    .await
    .unwrap();
    let job = r["job_id"].as_str().unwrap().to_owned();
    let done = wait_state(&d, &job, "done").await;
    let res = &done["result"];
    assert_eq!(res["format"], "m4a");
    assert_eq!(res["tracks"], "mic");
    assert_eq!(res["file"], "exports/Call - audio-mic.m4a");
    assert!(
        (res["duration_s"].as_f64().unwrap() - 8.0).abs() < 0.05,
        "{res}"
    );
    assert!(res["size_bytes"].as_u64().unwrap() > 1000);
    let path = root.path().join("Call.cmrec/exports/Call - audio-mic.m4a");
    assert!(path.is_file());
    let copy = out_dir.path().join("Call - audio-mic.m4a");
    assert!(copy.is_file());
    assert_eq!(res["copy_path"], copy.to_string_lossy().as_ref());

    // A second export copies next to the first one, never over it.
    let r2 = call(
        &d,
        "export_audio",
        json!({ "capture_id": id, "tracks": "mic", "out_dir": out_dir.path() }),
    )
    .await
    .unwrap();
    wait_state(&d, r2["job_id"].as_str().unwrap(), "done").await;
    assert!(out_dir.path().join("Call - audio-mic (2).m4a").is_file());

    let m: Value = serde_json::from_str(
        &std::fs::read_to_string(root.path().join("Call.cmrec/manifest.json")).unwrap(),
    )
    .unwrap();
    let exports = m["exports"].as_array().unwrap();
    assert_eq!(
        exports.len(),
        1,
        "same tracks replace the entry: {exports:?}"
    );
    assert_eq!(exports[0]["kind"], "audio");
    assert_eq!(exports[0]["preset"], "audio-mic");
    assert_eq!(exports[0]["settings"]["loudness_mode"], "two_pass");

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
}
