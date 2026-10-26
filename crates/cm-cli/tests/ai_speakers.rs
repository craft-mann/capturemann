//! `capturemann ai speakers ...` (AI-8) against an in-process daemon with a scripted speaker
//! model: `diarize`, rename, merge, assign, list, undo, the named exports and the
//! `no_speaker_model` error.
#![allow(clippy::unwrap_used)]

use std::io::Cursor;
use std::sync::Arc;

use cm_capture::fake::FakeBackend;
use cm_capture::Capturer;
use cm_cli::{run, Env, SharedBuf};
use cm_core::{MediaTime, TrackInfo, TrackKind, TrackStats};
use cm_daemon::testing::{FakeAi, FixedConfirmer};
use cm_daemon::{Daemon, Decision, Endpoint, IpcServer};
use cm_library::Library;
use serde_json::{json, Value};

struct Rig {
    dir: tempfile::TempDir,
    endpoint: Endpoint,
    daemon: Daemon,
    ai: FakeAi,
    #[allow(dead_code)]
    confirmer: Arc<FixedConfirmer>,
}

fn rig(ai: FakeAi, decision: Decision) -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let endpoint = Endpoint::Path(dir.path().join("d.sock"));
    let lib_dir = dir.path().join("lib");
    let lib = Library::open_or_create(&lib_dir).unwrap();
    let queue = cm_daemon::library_job_queue(&lib_dir).unwrap();
    let confirmer = Arc::new(FixedConfirmer::new(decision));
    let backend = FakeBackend::new().with_display(1, 0, 0, 400, 300, 2.0);
    let daemon = Daemon::builder(Capturer::new(backend).with_own_pid(None), lib)
        .jobs(queue)
        .confirmer(confirmer.clone())
        .ai(ai.services())
        .build();
    daemon.register_ai_handlers().unwrap();
    daemon.start_jobs().unwrap();
    let server = IpcServer::bind(&endpoint).unwrap();
    let d = daemon.clone();
    tokio::spawn(async move {
        let _ = server.run(d).await;
    });
    Rig {
        dir,
        endpoint,
        daemon,
        ai,
        confirmer,
    }
}

struct Out {
    code: i32,
    out: String,
    err: String,
}

impl Out {
    fn json(&self) -> Value {
        serde_json::from_str(&self.out).unwrap_or_else(|e| panic!("{e}: {}", self.out))
    }
}

async fn cli_stdin(r: &Rig, stdin: &str, args: &[&str]) -> Out {
    let (o, e) = (SharedBuf::default(), SharedBuf::default());
    let mut env = Env {
        endpoint: r.endpoint.clone(),
        launcher: None,
        stdin: Box::new(Cursor::new(stdin.to_owned().into_bytes())),
        out: Box::new(o.clone()),
        err: Box::new(e.clone()),
    };
    let argv: Vec<String> = std::iter::once("capturemann")
        .chain(args.iter().copied())
        .map(str::to_owned)
        .collect();
    let code = run(argv, &mut env).await;
    Out {
        code,
        out: o.contents(),
        err: e.contents(),
    }
}

async fn cli(r: &Rig, args: &[&str]) -> Out {
    cli_stdin(r, "", args).await
}

/// A finished 60 s recording with a microphone track, indexed.
async fn recording_in(r: &Rig) -> String {
    use cm_library::rec::{RecBundle, SessionInfo};
    let session = SessionInfo {
        clock_origin_ns: 1,
        timebase: "uptime_raw_ns".into(),
        displays: vec![],
        devices: json!({}),
        app_version: "0.0.1".into(),
        extra: Default::default(),
    };
    let now = time::OffsetDateTime::UNIX_EPOCH;
    let b = RecBundle::create(
        &r.dir.path().join("lib"),
        "Lesson",
        now,
        json!({}),
        &session,
    )
    .unwrap();
    std::fs::write(b.dir().join("screen.mp4"), b"master").unwrap();
    std::fs::write(b.dir().join("mic.wav"), b"audio").unwrap();
    let track = |kind, file: &str| TrackInfo {
        kind,
        file: file.into(),
        start: MediaTime::ZERO,
        end: MediaTime::from_secs(60),
        details: json!({}),
        stats: TrackStats::default(),
    };
    let m = b
        .finalize(
            &[
                track(TrackKind::Screen, "screen.mp4"),
                track(TrackKind::Mic, "mic.wav"),
            ],
            now,
        )
        .unwrap();
    r.daemon
        .execute_method(cm_core::Actor::User, 0, "rescan_library", &json!({}))
        .await
        .unwrap();
    m.id
}

const SCRIPT: [(f64, f64, u32, f32); 2] = [(0.0, 2.0, 5, 0.2), (2.0, 30.0, 9, 0.2)];

async fn transcribed(r: &Rig) -> String {
    let id = recording_in(r).await;
    let o = cli(r, &["ai", "transcribe", &id, "--wait"]).await;
    assert_eq!(o.code, 0, "{}", o.err);
    id
}

/// AI-8: no speaker model is exit 2 with the way forward, and nothing is queued.
#[tokio::test]
async fn ai_8_no_speaker_model_is_explained() {
    let r = rig(FakeAi::new().with_small_installed(), Decision::Approve);
    let id = transcribed(&r).await;
    let o = cli(&r, &["ai", "speakers", &id]).await;
    assert_eq!(o.code, 2, "{}", o.err);
    assert!(o.err.contains("No speaker model is installed"), "{}", o.err);
    let jobs = cli(&r, &["jobs", "list", "--json"]).await.json();
    assert!(!jobs.to_string().contains("diarize"), "{jobs}");
}

/// AI-8.AC3/AC4/AC5: `ai speakers <id> --num 2 --wait` finds two speakers (the hint reaches the
/// model), `rename`, `merge` and `assign` are one undo step each, `list` and the exports show
/// the names.
#[tokio::test]
async fn ai_8_speakers_commands_end_to_end() {
    let r = rig(
        FakeAi::new()
            .with_small_installed()
            .with_speaker_script(&SCRIPT),
        Decision::Approve,
    );
    let id = transcribed(&r).await;

    // Without --wait it prints the job.
    let o = cli(&r, &["ai", "speakers", &id, "--num", "2"]).await;
    assert_eq!(o.code, 0, "{}", o.err);
    assert!(o.out.contains("Finding the speakers of"), "{}", o.out);
    let o = cli(&r, &["ai", "speakers", &id, "--num", "2", "--wait"]).await;
    assert_eq!(o.code, 0, "{}", o.err);
    assert!(
        o.out.contains("Found 2 speakers") && o.out.contains("Speaker 2"),
        "{}",
        o.out
    );
    assert_eq!(
        r.ai.diarize_calls().last().unwrap().hints.num_speakers,
        Some(2)
    );

    let o = cli(&r, &["ai", "speakers", "rename", &id, "2", "Linh"]).await;
    assert_eq!(o.code, 0, "{}", o.err);
    assert!(
        o.out.contains("Linh") && o.out.contains("capturemann undo"),
        "{}",
        o.out
    );
    let l = cli(&r, &["ai", "speakers", "list", &id, "--json"])
        .await
        .json();
    assert_eq!(l["speakers"][1]["name"], "Linh");

    // The exports name the speakers.
    let o = cli(&r, &["export", &id, "--captions", "txt", "--timestamps"]).await;
    assert_eq!(o.code, 0, "{}", o.err);
    let path = o
        .out
        .lines()
        .find_map(|l| l.trim().strip_prefix("file: "))
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        "[00:01] Speaker 1: Hello and welcome\n[00:02] Linh: to the demo\n"
    );
    let o = cli(&r, &["export", &id, "--captions", "srt", "--speaker-names"]).await;
    assert_eq!(o.code, 0, "{}", o.err);
    let path = o
        .out
        .lines()
        .find_map(|l| l.trim().strip_prefix("file: "))
        .unwrap();
    assert!(std::fs::read_to_string(path)
        .unwrap()
        .contains("LINH: to the demo"));
    let o = cli(&r, &["export", &id, "--captions", "vtt", "--speaker-names"]).await;
    assert_eq!(o.code, 2, "{}", o.err);

    // Merge, assign, and one undo each.
    let o = cli(&r, &["ai", "speakers", "merge", &id, "2", "1"]).await;
    assert_eq!(o.code, 0, "{}", o.err);
    let l = cli(&r, &["ai", "speakers", "list", &id, "--json"])
        .await
        .json();
    assert_eq!(l["speakers"].as_array().unwrap().len(), 1);
    assert_eq!(cli(&r, &["undo", &id]).await.code, 0);
    let l = cli(&r, &["ai", "speakers", "list", &id, "--json"])
        .await
        .json();
    assert_eq!(l["speakers"].as_array().unwrap().len(), 2);
    let o = cli(
        &r,
        &["ai", "speakers", "assign", &id, "1", "2.35", "t:330000"],
    )
    .await;
    assert_eq!(o.code, 0, "{}", o.err);
    // Bad input is exit 2 and changes nothing.
    for bad in [
        vec!["ai", "speakers", "rename", &id, "9", "X"],
        vec!["ai", "speakers", "merge", &id, "1", "1"],
        vec!["ai", "speakers", "assign", &id, "1", "5", "4"],
        vec!["ai", "speakers", "assign", &id, "1", "soon", "4"],
    ] {
        assert_eq!(cli(&r, &bad).await.code, 2, "{bad:?}");
    }
}
