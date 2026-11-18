//! Quick Capture presets through the CLI (QC-5.AC2, AC5, AC6): `presets list` and `--preset`,
//! against an in-process daemon with a `presets.json` and the fake capture backend.

#![allow(clippy::unwrap_used)]

use std::io::Cursor;

use cm_capture::fake::FakeBackend;
use cm_capture::Capturer;
use cm_cli::{run, Env, SharedBuf};
use cm_daemon::preset_ops::PresetStore;
use cm_daemon::{Daemon, Endpoint, IpcServer};
use cm_library::Library;
use serde_json::Value;

struct Rig {
    endpoint: Endpoint,
    _dir: tempfile::TempDir,
    _cfg: tempfile::TempDir,
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

fn rig() -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let cfg = tempfile::tempdir().unwrap();
    let endpoint = Endpoint::scratch(dir.path(), "d");
    let lib = Library::open_or_create(&dir.path().join("lib")).unwrap();
    let backend = FakeBackend::new()
        .with_display(1, 0, 0, 400, 300, 2.0)
        .with_display(2, 400, 0, 200, 100, 1.0)
        .with_window(11, "Terminal", "Shell", 101, 1, (0, 0, 50, 50), false);
    let daemon: Daemon = Daemon::builder(Capturer::new(backend).with_own_pid(None), lib)
        .presets(PresetStore::in_dir(cfg.path()))
        .clipboard(std::sync::Arc::new(
            cm_daemon::testing::RecordingClipboard::default(),
        ))
        .build();
    let server = IpcServer::bind(&endpoint).unwrap();
    tokio::spawn(async move {
        let _ = server.run(daemon).await;
    });
    Rig {
        endpoint,
        _dir: dir,
        _cfg: cfg,
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

/// QC-5.AC2: `presets list` prints the presets with name, kind and target and changes nothing.
#[tokio::test]
async fn qc5_ac2_presets_list() {
    let r = rig();
    let o = cli(&r, &["presets", "list"]).await;
    assert_eq!(o.code, 0, "{}", o.err);
    for name in ["Bug report", "Docs shot", "Demo video", "Meeting"] {
        assert!(o.out.contains(name), "{}", o.out);
    }
    assert!(o.out.contains("Screenshot · Region"), "{}", o.out);
    let j = cli(&r, &["presets", "list", "--json"]).await.json();
    assert_eq!(j["presets"].as_array().unwrap().len(), 4);
    assert_eq!(j["presets"][2]["kind"], "video");
    assert_eq!(j["presets"][2]["target"], "display");
    // Marked as a Preview feature in the help.
    let help = cli(&r, &["--help"]).await;
    let line = help.out.lines().find(|l| l.contains("presets")).unwrap();
    assert!(line.contains("[preview]"), "{line}");
}

/// QC-5.AC6 and AC5: `--preset` behaves like the API call with a preset, and an unknown name is
/// an input error that names it.
#[tokio::test]
async fn qc5_ac6_screenshot_with_a_preset() {
    let r = rig();
    let with = cli(
        &r,
        &[
            "capture",
            "screenshot",
            "--display",
            "2",
            "--preset",
            "Bug report",
            "--json",
        ],
    )
    .await;
    assert_eq!(with.code, 0, "{}", with.err);
    // The starter copies the picture; the call without a preset does not.
    assert_eq!(with.json()["copied_to_clipboard"], true, "{}", with.out);
    let without = cli(&r, &["capture", "screenshot", "--display", "2", "--json"]).await;
    assert_eq!(without.json()["copied_to_clipboard"], false);
    assert_eq!(with.json()["width"], without.json()["width"]);
    // A preset alone captures the primary display.
    let alone = cli(
        &r,
        &["capture", "screenshot", "--preset", "bug-report", "--json"],
    )
    .await;
    assert_eq!(alone.code, 0, "{}", alone.err);
    // A flag in the same command wins over the preset.
    let timed = cli(
        &r,
        &[
            "capture",
            "screenshot",
            "--preset",
            "Bug report",
            "--delay",
            "0",
            "--json",
        ],
    )
    .await;
    assert_eq!(timed.code, 0, "{}", timed.err);
    // A Window preset needs a window.
    let win = cli(&r, &["capture", "screenshot", "--preset", "Docs shot"]).await;
    assert_eq!(win.code, 2, "{}", win.err);
    assert!(win.err.contains("--window"), "{}", win.err);
    let ok = cli(
        &r,
        &[
            "capture",
            "screenshot",
            "--preset",
            "Docs shot",
            "--window",
            "Terminal",
            "--json",
        ],
    )
    .await;
    assert_eq!(ok.code, 0, "{}", ok.err);
    // Unknown.
    let bad = cli(
        &r,
        &[
            "capture",
            "screenshot",
            "--preset",
            "nope",
            "--display",
            "1",
        ],
    )
    .await;
    assert_eq!(bad.code, 2, "{}", bad.err);
    assert!(bad.err.contains("\"nope\""), "{}", bad.err);
    // Still an error without any target and without a preset.
    assert_eq!(cli(&r, &["capture", "screenshot"]).await.code, 2);
}

/// QC-5.AC7: `record start --preset <screenshot preset>` is `invalid_input` at once: it
/// does not print "Waiting for confirmation" and nothing is asked of the user.
#[tokio::test]
async fn a_screenshot_preset_is_refused_before_any_confirmation() {
    let r = rig();
    for args in [
        vec!["record", "start", "--preset", "Docs shot"],
        vec!["record", "start", "--preset", "docs shot", "--display", "1"],
    ] {
        let o = cli(&r, &args).await;
        assert_eq!(o.code, 2, "{args:?}: {}", o.err);
        assert!(o.err.contains("error[invalid_input]"), "{}", o.err);
        assert!(o.err.contains("makes screenshots"), "{}", o.err);
        assert!(
            !o.err.contains("Waiting for confirmation"),
            "validated first: {}",
            o.err
        );
    }
}
