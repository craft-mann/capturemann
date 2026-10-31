//! `capturemann ai ...` and `export --captions` against an in-process daemon with a scripted AI
//! engine (M5, AI-1, AI-3, AI-4). Cites the acceptance criteria of `specs/features/07-ai.md`.

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
    confirmer: Arc<FixedConfirmer>,
}

fn rig(ai: FakeAi, decision: Decision) -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let endpoint = Endpoint::scratch(dir.path(), "d");
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

/// AI-1: with no model installed `ai models` says how to get one and that nothing is downloaded on
/// its own, and `ai transcribe` fails with exit 2 pointing to the same place.
#[tokio::test]
async fn ai_1_no_model_is_explained_and_nothing_is_downloaded() {
    let r = rig(FakeAi::new(), Decision::Approve);
    let id = recording_in(&r).await;
    let o = cli(&r, &["ai", "models"]).await;
    assert_eq!(o.code, 0, "{}", o.err);
    assert!(
        o.out.contains("Speech models (folder: /models)"),
        "{}",
        o.out
    );
    assert!(o.out.contains("not installed"), "{}", o.out);
    assert!(
        o.out.contains("capturemann ai models download small"),
        "{}",
        o.out
    );
    assert!(
        o.out.contains("nothing is downloaded unless you run it"),
        "{}",
        o.out
    );
    let j = cli(&r, &["ai", "models", "--json"]).await.json();
    assert_eq!(j["models"].as_array().unwrap().len(), 3);

    let o = cli(&r, &["ai", "transcribe", &id]).await;
    assert_eq!(o.code, 2, "{}", o.err);
    assert!(
        o.err.contains("AI page") && o.err.contains("capturemann ai models"),
        "{}",
        o.err
    );
    assert!(r.ai.downloads_started().is_empty());
    assert!(r.confirmer.shown().is_empty());
}

/// A download asks for confirmation, runs as a job and installs the model; `--wait` prints the
/// result; a decline exits 3 and installs nothing.
#[tokio::test]
async fn models_download_use_and_rm_ask_first() {
    let r = rig(FakeAi::new(), Decision::Approve);
    let o = cli(&r, &["ai", "models", "download", "small", "--wait"]).await;
    assert_eq!(o.code, 0, "{}", o.err);
    assert!(o.out.contains("Installed the model `small`"), "{}", o.out);
    assert_eq!(r.confirmer.shown().len(), 1);
    assert!(r.confirmer.shown()[0].summary.contains("Small"));
    let t = cli(&r, &["ai", "models"]).await;
    assert!(
        t.out.contains("* small") && t.out.contains("installed, used by default"),
        "{}",
        t.out
    );
    assert!(!t.out.contains("No model is installed"));
    // Without --wait it prints the job.
    let o = cli(&r, &["ai", "models", "download", "base"]).await;
    assert_eq!(o.code, 0, "{}", o.err);
    assert!(o.out.contains("Downloading `base`: job"), "{}", o.out);
    assert!(o.out.contains("capturemann jobs get"), "{}", o.out);
    // Already installed is a usage error.
    let o = cli(&r, &["ai", "models", "download", "small"]).await;
    assert_eq!(o.code, 2);
    assert!(o.err.contains("already installed"), "{}", o.err);
    // Use, and a model that is not installed.
    assert_eq!(cli(&r, &["ai", "models", "use", "medium"]).await.code, 2);
    let imported = cm_daemon::testing::fake_ai::some_file(r.dir.path());
    let o = cli(
        &r,
        &[
            "ai",
            "models",
            "import",
            "medium",
            imported.to_str().unwrap(),
        ],
    )
    .await;
    assert_eq!(o.code, 0, "{}", o.err);
    let o = cli(&r, &["ai", "models", "use", "medium"]).await;
    assert!(
        o.out.contains("`medium` is now the default model"),
        "{}",
        o.out
    );
    // Removing asks; a decline exits 3 and keeps the file.
    let o = cli(&r, &["ai", "models", "rm", "medium"]).await;
    assert_eq!(o.code, 0, "{}", o.err);
    assert!(o.out.contains("Deleted the model `medium`"), "{}", o.out);

    let declined = rig(FakeAi::new().with_small_installed(), Decision::Decline);
    let o = cli(&declined, &["ai", "models", "rm", "small"]).await;
    assert_eq!(o.code, 3, "{}", o.err);
    assert_eq!(declined.ai.installed(), vec!["small"]);
    let o = cli(&declined, &["ai", "models", "download", "base"]).await;
    assert_eq!(o.code, 3, "{}", o.err);
    assert!(declined.ai.downloads_started().is_empty());
}

/// AI-1: transcribe, read the transcript, edit captions, export SRT and VTT with the CLI.
#[tokio::test]
async fn ai_1_transcribe_transcript_captions_and_export() {
    let r = rig(FakeAi::new().with_small_installed(), Decision::Approve);
    let id = recording_in(&r).await;
    let o = cli(&r, &["ai", "transcribe", &id, "--wait", "--language", "en"]).await;
    assert_eq!(o.code, 0, "{}{}", o.out, o.err);
    assert!(
        o.out.contains("6 words, language en, model `small`"),
        "{}",
        o.out
    );
    assert_eq!(r.ai.transcribe_calls()[0].language.as_deref(), Some("en"));

    let t = cli(&r, &["ai", "transcript", &id]).await;
    assert_eq!(t.code, 0, "{}", t.err);
    assert!(
        t.out.contains("Transcript of") && t.out.contains("6 words"),
        "{}",
        t.out
    );
    assert!(t.out.contains("Caption lines (generated):"), "{}", t.out);
    assert!(
        t.out.contains("00:01.000 - 00:02.300  Hello and welcome"),
        "{}",
        t.out
    );
    let w = cli(&r, &["ai", "transcript", &id, "--words"]).await;
    assert!(
        w.out.contains("00:01.000 - 00:01.400  Hello  (90%)"),
        "{}",
        w.out
    );
    assert!(!w.out.contains("Caption lines"));

    // Edit the lines from stdin.
    let lines = r#"[{"start": 1, "end": 3, "text": "Hello there"}, {"start": 4, "end": 5.5, "text": "Second line"}]"#;
    let o = cli_stdin(&r, lines, &["ai", "captions", "set", &id, "--file", "-"]).await;
    assert_eq!(o.code, 0, "{}", o.err);
    assert!(
        o.out.contains("Set 2 caption lines") && o.out.contains("capturemann undo"),
        "{}",
        o.out
    );
    let t = cli(&r, &["ai", "transcript", &id]).await;
    assert!(
        t.out.contains("Caption lines (edited):") && t.out.contains("Hello there"),
        "{}",
        t.out
    );
    // Bad lines: exit 2, nothing changed.
    let bad = cli_stdin(
        &r,
        r#"[{"start": 3, "end": 1, "text": "x"}]"#,
        &["ai", "captions", "set", &id, "--file", "-"],
    )
    .await;
    assert_eq!(bad.code, 2, "{}", bad.err);
    assert_eq!(
        cli_stdin(&r, "{}", &["ai", "captions", "set", &id, "--file", "-"])
            .await
            .code,
        2
    );
    // Export the files.
    let srt = cli(&r, &["export", &id, "--captions", "srt"]).await;
    assert_eq!(srt.code, 0, "{}", srt.err);
    assert!(
        srt.out.contains("Wrote 2 caption lines") && srt.out.contains("SRT"),
        "{}",
        srt.out
    );
    let path = srt
        .out
        .lines()
        .find_map(|l| l.trim().strip_prefix("file: "))
        .unwrap();
    assert!(std::fs::read_to_string(path)
        .unwrap()
        .contains("Hello there"));
    let vtt = cli(&r, &["export", &id, "--captions", "vtt", "--json"])
        .await
        .json();
    assert_eq!(vtt["format"], "vtt");
    assert!(std::fs::read_to_string(vtt["path"].as_str().unwrap())
        .unwrap()
        .starts_with("WEBVTT"));
    // Plain text: `--captions txt`, with `--timestamps` for `[mm:ss]` prefixes.
    let txt = cli(&r, &["export", &id, "--captions", "txt", "--json"])
        .await
        .json();
    assert_eq!(txt["format"], "txt");
    let plain = std::fs::read_to_string(txt["path"].as_str().unwrap()).unwrap();
    assert!(
        plain.contains("Hello there") && !plain.contains("-->"),
        "{plain}"
    );
    let timed = cli(
        &r,
        &["export", &id, "--captions", "txt", "--timestamps", "--json"],
    )
    .await
    .json();
    let stamped = std::fs::read_to_string(timed["path"].as_str().unwrap()).unwrap();
    assert!(stamped.starts_with('['), "{stamped}");
    // `--timestamps` needs `--captions txt` (clap: requires --captions; the CLI: only for txt).
    assert_eq!(cli(&r, &["export", &id, "--timestamps"]).await.code, 2);
    assert_eq!(
        cli(&r, &["export", &id, "--captions", "srt", "--timestamps"])
            .await
            .code,
        2
    );
    // `--captions` replaces the video export: it cannot be combined with its options.
    for extra in [["--preset", "gif"], ["--loudness", "-14"]] {
        let o = cli(
            &r,
            &["export", &id, "--captions", "srt", extra[0], extra[1]],
        )
        .await;
        assert_eq!(o.code, 2, "{extra:?}: {}", o.err);
    }
    // Clear, then undo the clear.
    let o = cli(&r, &["ai", "captions", "set", &id, "--clear"]).await;
    assert_eq!(o.code, 0, "{}", o.err);
    assert!(cli(&r, &["ai", "transcript", &id])
        .await
        .out
        .contains("(generated)"));
    assert_eq!(cli(&r, &["undo", &id]).await.code, 0);
    assert!(cli(&r, &["ai", "transcript", &id])
        .await
        .out
        .contains("(edited)"));
    // Usage: set needs a source.
    assert_eq!(cli(&r, &["ai", "captions", "set", &id]).await.code, 2);
}

/// AI-3 and AI-4: suggestions are printed with how to accept them; nothing is applied.
#[tokio::test]
async fn ai_3_and_4_suggest_zooms_and_cuts_apply_nothing() {
    let r = rig(FakeAi::new().with_small_installed(), Decision::Approve);
    let id = recording_in(&r).await;
    let z = cli(&r, &["ai", "suggest", "zooms", &id]).await;
    assert_eq!(z.code, 0, "{}", z.err);
    assert!(
        z.out
            .contains("1. 00:10.000 - 00:14.000  zoom 2.0x at (0.40, 0.60)  click"),
        "{}",
        z.out
    );
    assert!(
        z.out.contains("Nothing was applied") && z.out.contains("capturemann edit zoom"),
        "{}",
        z.out
    );
    let c = cli(&r, &["ai", "suggest", "cuts", &id]).await;
    assert!(
        c.out.contains("silence") && c.out.contains("capturemann edit cut"),
        "{}",
        c.out
    );
    let j = cli(&r, &["ai", "suggest", "cuts", &id, "--json"])
        .await
        .json();
    assert_eq!(j["count"], 1);
    assert_eq!(j["apply_with"], "cut_segments");
    // A suggestion is accepted with the existing command.
    let o = cli(&r, &["edit", "cut", &id, "20-23"]).await;
    assert_eq!(o.code, 0, "{}", o.err);
}

/// A daemon without an engine: the model list says so (exit 1, `not_implemented`).
#[tokio::test]
async fn without_an_engine_the_ai_commands_say_not_implemented() {
    let dir = tempfile::tempdir().unwrap();
    let endpoint = Endpoint::scratch(dir.path(), "d");
    let lib = Library::open_or_create(&dir.path().join("lib")).unwrap();
    let backend = FakeBackend::new().with_display(1, 0, 0, 400, 300, 2.0);
    let daemon = Daemon::builder(Capturer::new(backend).with_own_pid(None), lib).build();
    let server = IpcServer::bind(&endpoint).unwrap();
    let d = daemon.clone();
    tokio::spawn(async move {
        let _ = server.run(d).await;
    });
    let r = Rig {
        dir,
        endpoint,
        daemon,
        ai: FakeAi::new(),
        confirmer: Arc::new(FixedConfirmer::new(Decision::Approve)),
    };
    let o = cli(&r, &["ai", "models"]).await;
    assert_eq!(o.code, 1, "{}{}", o.out, o.err);
    assert!(
        o.err.contains("not_implemented") && o.err.contains("not available in this build"),
        "{}",
        o.err
    );
    let o = cli(&r, &["ai", "models", "download", "small"]).await;
    assert_eq!(o.code, 1, "{}", o.err);
}

/// AI-1: `ai captions options` sets the show and burn-in switches (one undo step), says when
/// nothing changed, and rejects a call without an option or with contradicting ones.
#[tokio::test]
async fn ai_1_captions_options_set_the_switches() {
    let r = rig(FakeAi::new().with_small_installed(), Decision::Approve);
    let id = recording_in(&r).await;
    let o = cli(&r, &["ai", "captions", "options", &id, "--show", "--json"]).await;
    assert_eq!(o.code, 0, "{}{}", o.out, o.err);
    let v = o.json();
    assert_eq!(
        (v["enabled"].clone(), v["burn_in"].clone()),
        (json!(true), json!(false))
    );
    assert_eq!(v["changed"], true);
    let o = cli(&r, &["ai", "captions", "options", &id, "--burn"]).await;
    assert_eq!(o.code, 0, "{}", o.err);
    assert!(
        o.out.contains("shown on")
            && o.out.contains("burned into exports on")
            && o.out.contains("undo with"),
        "{}",
        o.out
    );
    let again = cli(&r, &["ai", "captions", "options", &id, "--burn"]).await;
    assert!(again.out.contains("nothing changed"), "{}", again.out);
    let o = cli(
        &r,
        &["ai", "captions", "options", &id, "--hide", "--no-burn"],
    )
    .await;
    assert_eq!(o.code, 0, "{}", o.err);
    assert!(
        o.out.contains("shown off") && o.out.contains("exports off"),
        "{}",
        o.out
    );
    // No option, or contradicting options, is a usage error (exit 2).
    assert_eq!(cli(&r, &["ai", "captions", "options", &id]).await.code, 2);
    for bad in [["--show", "--hide"], ["--burn", "--no-burn"]] {
        let o = cli(&r, &["ai", "captions", "options", &id, bad[0], bad[1]]).await;
        assert_eq!(o.code, 2, "{bad:?}");
    }
}

/// AI-6a: `ai captions replace` runs `replace_in_captions` (flags or a JSON file), says what it
/// did, says when nothing matched, and rejects a call without a pair (exit 2).
#[tokio::test]
async fn ai_6a_captions_replace_with_flags_and_a_file() {
    let r = rig(FakeAi::new().with_small_installed(), Decision::Approve);
    let id = recording_in(&r).await;
    let o = cli(&r, &["ai", "transcribe", &id, "--wait"]).await;
    assert_eq!(o.code, 0, "{}{}", o.out, o.err);

    let o = cli(
        &r,
        &[
            "ai",
            "captions",
            "replace",
            &id,
            "--find",
            "WELCOME",
            "--replace",
            "Welcome",
            "--match-case",
        ],
    )
    .await;
    assert_eq!(o.code, 0, "{}{}", o.out, o.err);
    assert!(o.out.contains("Nothing found"), "{}", o.out);

    let o = cli(
        &r,
        &[
            "ai",
            "captions",
            "replace",
            &id,
            "--find",
            "WELCOME",
            "--replace",
            "Welcome",
            "--json",
        ],
    )
    .await;
    assert_eq!(o.code, 0, "{}{}", o.out, o.err);
    let v = o.json();
    assert_eq!(
        (v["replacements"].clone(), v["lines_touched"].clone()),
        (json!(1), json!(1))
    );
    let o = cli(
        &r,
        &[
            "ai",
            "captions",
            "replace",
            &id,
            "--find",
            "demo",
            "--replace",
            "product",
        ],
    )
    .await;
    assert!(
        o.out.contains("Replaced 1 matches in 1 caption lines")
            && o.out.contains("capturemann undo"),
        "{}",
        o.out
    );

    let pairs =
        r#"[{"find": "the", "replace": "a", "whole_word": true}, {"find": "to", "replace": ""}]"#;
    let o = cli_stdin(
        &r,
        pairs,
        &["ai", "captions", "replace", &id, "--file", "-"],
    )
    .await;
    assert_eq!(o.code, 0, "{}{}", o.out, o.err);
    let t = cli(&r, &["ai", "transcript", &id]).await;
    assert!(
        t.out.contains("Hello and Welcome") && t.out.contains("a product"),
        "{}",
        t.out
    );

    // Usage errors.
    assert_eq!(cli(&r, &["ai", "captions", "replace", &id]).await.code, 2);
    assert_eq!(
        cli(&r, &["ai", "captions", "replace", &id, "--find", "x"])
            .await
            .code,
        2
    );
    assert_eq!(
        cli_stdin(&r, "{}", &["ai", "captions", "replace", &id, "--file", "-"])
            .await
            .code,
        2
    );
}
