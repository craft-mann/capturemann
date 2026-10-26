//! `capturemann ai translate` and the `--language` flags (AI-5) against an in-process daemon with
//! a scripted AI engine. Cites the acceptance criteria of `specs/features/07-ai.md`.

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
}

fn rig(ai: FakeAi) -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let endpoint = Endpoint::Path(dir.path().join("d.sock"));
    let lib_dir = dir.path().join("lib");
    let lib = Library::open_or_create(&lib_dir).unwrap();
    let queue = cm_daemon::library_job_queue(&lib_dir).unwrap();
    let backend = FakeBackend::new().with_display(1, 0, 0, 400, 300, 2.0);
    let daemon = Daemon::builder(Capturer::new(backend).with_own_pid(None), lib)
        .jobs(queue)
        .confirmer(Arc::new(FixedConfirmer::new(Decision::Decline)))
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

async fn cli(r: &Rig, args: &[&str]) -> Out {
    let (o, e) = (SharedBuf::default(), SharedBuf::default());
    let mut env = Env {
        endpoint: r.endpoint.clone(),
        launcher: None,
        stdin: Box::new(Cursor::new(Vec::new())),
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

/// A finished recording with a microphone track and a Vietnamese transcript, indexed.
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
    let dir = b.dir().to_path_buf();
    let m = b
        .finalize(
            &[
                track(TrackKind::Screen, "screen.mp4"),
                track(TrackKind::Mic, "mic.wav"),
            ],
            now,
        )
        .unwrap();
    // Ticks of 1/120000 s.
    let w = |text: &str, a: i64, b: i64| json!({ "text": text, "start": a * 12_000, "end": b * 12_000, "confidence": 0.9 });
    let transcript = json!({
        "version": 1, "language": "vi", "model": "small", "note": null,
        "words": [w("Xin", 10, 12), w("chào", 13, 16), w("mọi", 17, 20)],
    });
    std::fs::write(
        dir.join("transcript.json"),
        serde_json::to_vec(&transcript).unwrap(),
    )
    .unwrap();
    r.daemon
        .execute_method(cm_core::Actor::User, 0, "rescan_library", &json!({}))
        .await
        .unwrap();
    m.id
}

/// AI-5 from the command line: translate with Whisper, read and export it per language, and the
/// two errors a person can act on.
#[tokio::test]
async fn ai_5_translate_get_and_export_per_language() {
    let r = rig(FakeAi::new().with_installed(
        &["small", "large", "large-v3-turbo"],
        Some("large-v3-turbo"),
    ));
    let id = recording_in(&r).await;

    let o = cli(&r, &["ai", "translate", &id, "--to", "en", "--wait"]).await;
    assert_eq!(o.code, 0, "{}{}", o.out, o.err);
    assert!(
        o.out.contains("Translated") && o.out.contains("`large`"),
        "{}",
        o.out
    );
    // The turbo model is active; the job used large-v3 (the model list calls it `large`).
    assert_eq!(r.ai.translate_calls()[0].model, "large");

    let g = cli(&r, &["ai", "captions", "get", &id, "--language", "en"]).await;
    assert_eq!(g.code, 0, "{}", g.err);
    assert!(
        g.out.contains("translation to en by whisper `large`")
            && g.out.contains("Hello and welcome"),
        "{}",
        g.out
    );
    let j = cli(
        &r,
        &["--json", "ai", "captions", "get", &id, "--language", "en"],
    )
    .await;
    assert_eq!(j.json()["source"], "translation");

    let e = cli(
        &r,
        &["export", &id, "--captions", "srt", "--language", "en"],
    )
    .await;
    assert_eq!(e.code, 0, "{}{}", e.out, e.err);
    assert!(e.out.contains("captions.en.srt"), "{}", e.out);

    let rep = cli(
        &r,
        &[
            "ai",
            "captions",
            "replace",
            &id,
            "--language",
            "en",
            "--find",
            "welcome",
            "--replace",
            "greetings",
        ],
    )
    .await;
    assert_eq!(rep.code, 0, "{}{}", rep.out, rep.err);
    assert!(rep.out.contains("`en` translation"), "{}", rep.out);

    // Not a translation yet: not_found (exit 2 family), with the way to make one.
    let missing = cli(&r, &["ai", "captions", "get", &id, "--language", "es"]).await;
    assert_ne!(missing.code, 0);
    assert!(missing.err.contains("translate"), "{}", missing.err);

    // No text model: the reason is in the error data.
    let t = cli(&r, &["--json", "ai", "translate", &id, "--to", "es"]).await;
    assert_eq!(t.code, 2, "{}{}", t.out, t.err);
    assert!(
        t.out.contains("no_text_model") || t.err.contains("no_text_model"),
        "{}{}",
        t.out,
        t.err
    );
    // --language needs --captions.
    let bad = cli(&r, &["export", &id, "--language", "en"]).await;
    assert_ne!(bad.code, 0);
}
