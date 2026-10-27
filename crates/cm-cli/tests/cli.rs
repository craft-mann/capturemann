//! Integration tests: the CLI library entry point against an in-process `cm-daemon` on a
//! temp socket with the fake capture backend. Cites CAP-*, ANN-*, AGT-* acceptance criteria.

#![allow(clippy::unwrap_used)]

use std::io::Cursor;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use cm_capture::fake::FakeBackend;
use cm_capture::Capturer;
use cm_cli::{run, Env, Launcher, SharedBuf, CLI_COMMANDS};
use cm_daemon::{is_internal, Daemon, Endpoint, IpcServer, ACTIONS, INTERNAL_ACTIONS};
use cm_library::Library;
use serde_json::{json, Value};

struct Rig {
    dir: tempfile::TempDir,
    endpoint: Endpoint,
    daemon: Daemon,
}

fn desktop() -> FakeBackend {
    FakeBackend::new()
        .with_display(1, 0, 0, 400, 300, 2.0)
        .with_display(2, 400, 0, 200, 100, 1.0)
        .with_window(10, "Editor", "Code", 100, 1, (20, 10, 100, 80), false)
        .with_window(11, "Terminal", "Shell", 101, 1, (0, 0, 50, 50), false)
}

fn make_daemon(backend: FakeBackend, dir: &std::path::Path) -> Daemon {
    let lib = Library::open_or_create(&dir.join("lib")).unwrap();
    Daemon::builder(Capturer::new(backend).with_own_pid(None), lib).build()
}

fn serve(daemon: &Daemon, endpoint: &Endpoint) {
    let server = IpcServer::bind(endpoint).unwrap();
    let d = daemon.clone();
    tokio::spawn(async move {
        let _ = server.run(d).await;
    });
}

/// A daemon already listening.
fn rig_with(backend: FakeBackend) -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let endpoint = Endpoint::Path(dir.path().join("d.sock"));
    let daemon = make_daemon(backend, dir.path());
    serve(&daemon, &endpoint);
    Rig {
        dir,
        endpoint,
        daemon,
    }
}

fn rig() -> Rig {
    rig_with(desktop())
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

fn env_for(
    endpoint: &Endpoint,
    launcher: Option<Launcher>,
    stdin: &str,
) -> (Env, SharedBuf, SharedBuf) {
    let (o, e) = (SharedBuf::default(), SharedBuf::default());
    let env = Env {
        endpoint: endpoint.clone(),
        launcher,
        stdin: Box::new(Cursor::new(stdin.to_owned().into_bytes())),
        out: Box::new(o.clone()),
        err: Box::new(e.clone()),
    };
    (env, o, e)
}

async fn cli_stdin(r: &Rig, stdin: &str, args: &[&str]) -> Out {
    let (mut env, o, e) = env_for(&r.endpoint, None, stdin);
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

fn png_size(bytes: &[u8]) -> (u32, u32) {
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    let w = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
    let h = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
    (w, h)
}

const OPS: &str =
    r##"[{"id":"op_1","op":"rect","rect":[10,10,50,30],"color":"#FF3B3B","width":4}]"##;

#[tokio::test]
async fn targets_lists_displays_and_windows_human_and_json() {
    let r = rig();
    let o = cli(&r, &["targets"]).await;
    assert_eq!(o.code, 0, "{}", o.err);
    assert!(
        o.out.contains("Displays") && o.out.contains("(primary)"),
        "{}",
        o.out
    );
    assert!(o.out.contains("Code - Editor"), "{}", o.out);
    // CAP-3: sizes print as `WxH` (the daemon serializes a physical size as `[w, h]`).
    assert!(o.out.contains("400x300 at (0, 0)"), "{}", o.out);
    assert!(o.out.contains("100x80 at (20, 10)"), "{}", o.out);
    assert!(!o.out.contains("null"), "{}", o.out);
    let j = cli(&r, &["targets", "list", "--json"]).await.json();
    assert_eq!(j["displays"].as_array().unwrap().len(), 2);
    assert_eq!(j["windows"][0]["title"], "Editor");
    // CAP-1.AC1: `--displays-only` asks for `windows: false`.
    let d = cli(&r, &["targets", "--displays-only", "--json"])
        .await
        .json();
    assert_eq!(d["displays"].as_array().unwrap().len(), 2);
    assert_eq!(d["windows"].as_array().unwrap().len(), 0);
}

/// CAP-1.AC2: a region is physical px relative to the display.
#[tokio::test]
async fn screenshot_region_is_physical_px_and_out_writes_png() {
    let r = rig();
    let png = r.dir.path().join("r.png");
    let o = cli(
        &r,
        &[
            "capture",
            "screenshot",
            "--display",
            "1",
            "--region",
            "10,20,30,40",
            "--out",
            png.to_str().unwrap(),
            "--json",
        ],
    )
    .await;
    assert_eq!(o.code, 0, "{}", o.err);
    let j = o.json();
    assert_eq!(
        (j["width"].clone(), j["height"].clone()),
        (json!(30), json!(40))
    );
    assert_eq!(png_size(&std::fs::read(&png).unwrap()), (30, 40));
    // The base64 is never printed.
    assert!(j["image"]["data"].is_null() && j["image"]["bytes"].as_u64().unwrap() > 0);
    // The capture is in the library.
    let list = cli(&r, &["library", "list", "--json"]).await.json();
    assert_eq!(list["captures"][0]["id"], j["capture_id"]);
}

/// CAP-1: `--region` alone uses the primary display; display capture is full size.
#[tokio::test]
async fn screenshot_display_and_default_display_for_region() {
    let r = rig();
    let o = cli(&r, &["capture", "screenshot", "--display", "2", "--json"]).await;
    assert_eq!(o.code, 0, "{}", o.err);
    assert_eq!(
        (o.json()["width"].clone(), o.json()["height"].clone()),
        (json!(200), json!(100))
    );
    let o = cli(
        &r,
        &["capture", "screenshot", "--region", "0,0,16,8", "--json"],
    )
    .await;
    assert_eq!(o.code, 0, "{}", o.err);
    assert_eq!(o.json()["width"], 16);
}

/// CAP-1.AC4: window by id or by title substring (case-insensitive).
#[tokio::test]
async fn screenshot_window_by_title_or_id() {
    let r = rig();
    let by_title = cli(&r, &["capture", "screenshot", "--window", "edit", "--json"]).await;
    assert_eq!(by_title.code, 0, "{}", by_title.err);
    let j = by_title.json();
    assert_eq!(
        (j["kind"].clone(), j["width"].clone(), j["height"].clone()),
        (json!("window"), json!(100), json!(80))
    );
    let by_id = cli(&r, &["capture", "screenshot", "--window", "11", "--json"])
        .await
        .json();
    assert_eq!(by_id["width"], 50);
    let human = cli(&r, &["capture", "screenshot", "--window", "Terminal"]).await;
    assert!(human.out.starts_with("Captured "), "{}", human.out);
}

#[tokio::test]
async fn screenshot_unknown_window_is_not_found_exit_2() {
    let r = rig();
    let o = cli(&r, &["capture", "screenshot", "--window", "nonexistent"]).await;
    assert_eq!(o.code, 2);
    assert!(o.err.starts_with("error[not_found]"), "{}", o.err);
}

/// CAP-5: delay outside 0..=30 s is invalid_input (exit 2); a valid delay works.
#[tokio::test]
async fn screenshot_delay_is_validated_by_the_daemon() {
    let r = rig();
    let bad = cli(
        &r,
        &["capture", "screenshot", "--display", "1", "--delay", "31"],
    )
    .await;
    assert_eq!(bad.code, 2);
    assert!(bad.err.contains("invalid_input"), "{}", bad.err);
    let ok = cli(
        &r,
        &["capture", "screenshot", "--display", "1", "--delay", "0.05"],
    )
    .await;
    assert_eq!(ok.code, 0, "{}", ok.err);
}

/// CAP-4 (agent): `--copy` asks the daemon to copy; without a clipboard engine it is a warning.
#[tokio::test]
async fn screenshot_copy_flag_reaches_the_daemon() {
    let r = rig();
    let o = cli(
        &r,
        &[
            "capture",
            "screenshot",
            "--display",
            "1",
            "--copy",
            "--json",
        ],
    )
    .await;
    assert_eq!(o.code, 0, "{}", o.err);
    let j = o.json();
    assert_eq!(j["copied_to_clipboard"], false);
    assert!(!j["warnings"].as_array().unwrap().is_empty());
}

/// CAP-1.AC1 error path: missing screen permission is exit 4 with the permission named.
#[tokio::test]
async fn permission_missing_is_exit_4() {
    let r = rig_with(desktop().without_permission());
    let o = cli(&r, &["capture", "screenshot", "--display", "1"]).await;
    assert_eq!(o.code, 4, "{}", o.err);
    assert!(
        o.err.contains("error[permission_missing]") && o.err.contains("screen"),
        "{}",
        o.err
    );
    let j = cli(&r, &["capture", "screenshot", "--display", "1", "--json"]).await;
    assert_eq!(j.code, 4);
    let e: Value = serde_json::from_str(&j.err).unwrap();
    assert_eq!(e["error"]["code"], "permission_missing");
    assert_eq!(e["error"]["data"]["permission"], "screen");
}

#[tokio::test]
async fn bad_region_and_usage_errors_exit_2() {
    let r = rig();
    for args in [
        vec![
            "capture",
            "screenshot",
            "--display",
            "1",
            "--region",
            "1,2,3",
        ],
        vec![
            "capture",
            "screenshot",
            "--display",
            "1",
            "--region",
            "0,0,0,5",
        ],
        vec!["capture", "screenshot"],
        vec!["capture", "screenshot", "--window", "x", "--display", "1"],
        vec!["bogus"],
        vec![],
    ] {
        let o = cli(&r, &args).await;
        assert_eq!(o.code, 2, "{args:?}: {}", o.err);
        assert!(o.out.is_empty());
    }
    assert_eq!(cli(&r, &["--help"]).await.code, 0);
    assert_eq!(cli(&r, &["--version"]).await.code, 0);
}

/// ANN-1/ANN-3: annotate (inline, file, stdin), get, undo, redo; each is one revision.
#[tokio::test]
async fn annotate_get_undo_redo_roundtrip() {
    let r = rig();
    let id = cli(&r, &["capture", "screenshot", "--display", "1", "--json"])
        .await
        .json()["capture_id"]
        .as_str()
        .unwrap()
        .to_owned();

    let out_png = r.dir.path().join("a.png");
    let a = cli(
        &r,
        &[
            "annotate",
            &id,
            "--ops",
            OPS,
            "--out",
            out_png.to_str().unwrap(),
        ],
    )
    .await;
    assert_eq!(a.code, 0, "{}", a.err);
    assert!(a.out.contains("by agent:capturemann-cli"), "{}", a.out);
    assert_eq!(png_size(&std::fs::read(&out_png).unwrap()), (400, 300));

    let file = r.dir.path().join("ops.json");
    std::fs::write(
        &file,
        r##"{"ops":[{"id":"op_2","op":"ellipse","rect":[5,5,40,40],"color":"#2E8BFF","width":3}]}"##,
    )
    .unwrap();
    assert_eq!(
        cli(&r, &["annotate", &id, "--ops", file.to_str().unwrap()])
            .await
            .code,
        0
    );

    let stdin_ops =
        r##"[{"id":"op_3","op":"line","from":[0,0],"to":[50,50],"color":"#111111","width":2}]"##;
    let s = cli_stdin(&r, stdin_ops, &["annotate", &id, "--ops", "-", "--json"]).await;
    assert_eq!(s.code, 0, "{}", s.err);
    assert_eq!(s.json()["revision"], 3);

    let g = cli(&r, &["capture", "get", &id, "--json"]).await.json();
    assert_eq!(g["ops"].as_array().unwrap().len(), 3);
    assert_eq!(g["can_undo"], true);

    let u = cli(&r, &["undo", &id, "--json"]).await.json();
    assert_eq!(u["changed"], true);
    assert_eq!(
        cli(&r, &["capture", "get", &id, "--json"]).await.json()["ops"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let human = cli(&r, &["redo", &id]).await;
    assert!(human.out.starts_with("Redid "), "{}", human.out);
    assert_eq!(
        cli(&r, &["capture", "get", &id, "--json"]).await.json()["ops"]
            .as_array()
            .unwrap()
            .len(),
        3
    );

    // `get --out` writes the annotated image.
    let got = r.dir.path().join("g.png");
    assert_eq!(
        cli(&r, &["capture", "get", &id, "--out", got.to_str().unwrap()])
            .await
            .code,
        0
    );
    assert_eq!(png_size(&std::fs::read(&got).unwrap()), (400, 300));
}

#[tokio::test]
async fn undo_with_nothing_to_undo_is_not_an_error() {
    let r = rig();
    let id = cli(&r, &["capture", "screenshot", "--display", "1", "--json"])
        .await
        .json()["capture_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let o = cli(&r, &["undo", &id]).await;
    assert_eq!(o.code, 0, "{}", o.err);
    assert!(o.out.starts_with("Nothing to undo"), "{}", o.out);
}

/// ANN-2.AC4: `annotate --suggest-redactions` / `--redact-emails` propose and apply nothing.
#[tokio::test]
async fn ann_2_ac4_annotate_suggest_redactions_is_read_only() {
    use cm_daemon::testing::{ScriptedRecognizer, TextLine};
    let dir = tempfile::tempdir().unwrap();
    let endpoint = Endpoint::Path(dir.path().join("d.sock"));
    let lib = Library::open_or_create(&dir.path().join("lib")).unwrap();
    let rec = ScriptedRecognizer::new(vec![
        TextLine::laid_out("Owner: jane.doe@example.com", 50.0, 60.0, 10.0, 20.0),
        TextLine::laid_out("Billing: ap@vendor.example.org", 50.0, 140.0, 10.0, 20.0),
        TextLine::laid_out(
            "key sk-proj-A1b2C3d4E5f6G7h8I9j0K1l2",
            50.0,
            220.0,
            10.0,
            20.0,
        ),
    ]);
    let daemon = Daemon::builder(Capturer::new(desktop()).with_own_pid(None), lib)
        .text_recognizer(rec)
        .build();
    serve(&daemon, &endpoint);
    let r = Rig {
        dir,
        endpoint,
        daemon,
    };
    let id = cli(&r, &["capture", "screenshot", "--display", "1", "--json"])
        .await
        .json()["capture_id"]
        .as_str()
        .unwrap()
        .to_owned();

    let all = cli(&r, &["annotate", &id, "--suggest-redactions", "--json"]).await;
    assert_eq!(all.code, 0, "{}", all.err);
    assert_eq!(all.json()["rects"].as_array().unwrap().len(), 3);
    assert_eq!(all.json()["applied"], false);
    let emails = cli(&r, &["annotate", &id, "--redact-emails", "--json"]).await;
    assert_eq!(emails.code, 0, "{}", emails.err);
    assert_eq!(emails.json()["rects"].as_array().unwrap().len(), 2);
    let human = cli(&r, &["annotate", &id, "--redact-emails"]).await;
    assert!(
        human.out.contains("2 proposed redaction(s)"),
        "{}",
        human.out
    );
    assert!(human.out.contains("nothing applied"), "{}", human.out);
    assert!(!human.out.contains("jane.doe"), "{}", human.out);

    // Nothing was applied.
    let got = cli(&r, &["capture", "get", &id, "--json"]).await;
    assert_eq!(got.json()["ops"], json!([]));
    // `--ops` stays required without the flags, and the flags refuse write options.
    let none = cli(&r, &["annotate", &id]).await;
    assert_eq!(none.code, 2, "{}", none.err);
    let both = cli(&r, &["annotate", &id, "--suggest-redactions", "--ops", OPS]).await;
    assert_eq!(both.code, 2, "{}", both.err);
}

/// ANN-1 errors: invalid op -> invalid_input (2); unknown capture -> not_found (2); bad
/// local ops input is rejected before any daemon call.
#[tokio::test]
async fn annotate_errors_exit_2() {
    let r = rig();
    let id = cli(&r, &["capture", "screenshot", "--display", "1", "--json"])
        .await
        .json()["capture_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let bad_op = cli(&r, &["annotate", &id, "--ops", r#"[{"op":"zigzag"}]"#]).await;
    assert_eq!(bad_op.code, 2, "{}", bad_op.err);
    assert!(bad_op.err.contains("invalid_input"));
    let missing = cli(&r, &["annotate", "cap_nope", "--ops", OPS]).await;
    assert_eq!(missing.code, 2, "{}", missing.err);
    assert!(missing.err.contains("not_found"), "{}", missing.err);
    for ops in ["/no/such/file.json", "{not json", r#"{"x":1}"#, "[]"] {
        let o = cli(&r, &["annotate", &id, "--ops", ops]).await;
        assert_eq!(o.code, 2, "{ops}: {}", o.err);
    }
}

/// `capture screenshot --annotate` (spec example): one command captures and annotates.
#[tokio::test]
async fn screenshot_with_annotate_applies_ops() {
    let r = rig();
    let file = r.dir.path().join("ops.json");
    std::fs::write(&file, OPS).unwrap();
    let png = r.dir.path().join("s.png");
    let o = cli(
        &r,
        &[
            "capture",
            "screenshot",
            "--display",
            "1",
            "--annotate",
            file.to_str().unwrap(),
            "--out",
            png.to_str().unwrap(),
            "--json",
        ],
    )
    .await;
    assert_eq!(o.code, 0, "{}", o.err);
    let j = o.json();
    assert_eq!(j["revision"], 1);
    let id = j["capture_id"].as_str().unwrap();
    let g = cli(&r, &["capture", "get", id, "--json"]).await.json();
    assert_eq!(g["ops"].as_array().unwrap().len(), 1);
    assert!(png.exists());
    // Invalid ops fail before capturing anything.
    let before = cli(&r, &["library", "list", "--json"]).await.json()["captures"]
        .as_array()
        .unwrap()
        .len();
    let bad = cli(
        &r,
        &[
            "capture",
            "screenshot",
            "--display",
            "1",
            "--annotate",
            "{nope",
        ],
    )
    .await;
    assert_eq!(bad.code, 2);
    let after = cli(&r, &["library", "list", "--json"]).await.json()["captures"]
        .as_array()
        .unwrap()
        .len();
    assert_eq!(before, after);
}

/// AGT-3.AC4: a repeated `--request-id` replays the first result, no second capture.
#[tokio::test]
async fn request_id_is_idempotent() {
    let r = rig();
    let a = cli(
        &r,
        &[
            "--request-id",
            "k1",
            "capture",
            "screenshot",
            "--display",
            "1",
            "--json",
        ],
    )
    .await;
    let b = cli(
        &r,
        &[
            "--request-id",
            "k1",
            "capture",
            "screenshot",
            "--display",
            "1",
            "--json",
        ],
    )
    .await;
    assert_eq!((a.code, b.code), (0, 0), "{} {}", a.err, b.err);
    assert_eq!(a.json()["capture_id"], b.json()["capture_id"]);
    let list = cli(&r, &["library", "list", "--json"]).await.json();
    assert_eq!(list["captures"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn library_list_search_kind_and_paging() {
    let r = rig();
    for t in ["alpha one", "beta two", "alpha three"] {
        assert_eq!(
            cli(
                &r,
                &["capture", "screenshot", "--display", "2", "--title", t]
            )
            .await
            .code,
            0
        );
    }
    let all = cli(&r, &["library", "list"]).await;
    assert_eq!(all.out.lines().count(), 3, "{}", all.out);
    let q = cli(&r, &["library", "search", "alpha", "--json"])
        .await
        .json();
    assert_eq!(q["captures"].as_array().unwrap().len(), 2);
    let page = cli(&r, &["library", "list", "--limit", "2", "--json"])
        .await
        .json();
    assert_eq!(page["captures"].as_array().unwrap().len(), 2);
    let cursor = page["next_cursor"].as_str().unwrap();
    let next = cli(
        &r,
        &[
            "library", "list", "--limit", "2", "--cursor", cursor, "--json",
        ],
    )
    .await
    .json();
    assert_eq!(next["captures"].as_array().unwrap().len(), 1);
    let none = cli(&r, &["library", "list", "--kind", "recording"]).await;
    assert_eq!(none.out.trim(), "No captures.");
    assert_eq!(cli(&r, &["library", "list", "--limit", "0"]).await.code, 2);
}

#[tokio::test]
async fn settings_set_and_get() {
    let r = rig();
    let set = cli(
        &r,
        &[
            "settings",
            "set",
            "recording.size",
            "source",
            "recording.fps",
            "60",
            "recording.codec",
            "hevc",
            "appearance.theme",
            "true",
            "--json",
        ],
    )
    .await;
    assert_eq!(set.code, 0, "{}", set.err);
    let s = set.json();
    assert_eq!(s["settings"]["recording.fps"], 60);
    assert_eq!(s["settings"]["recording.size"], "source");
    assert_eq!(s["settings"]["appearance.theme"], true);
    let get = cli(&r, &["settings", "get", "recording.fps", "recording.codec"]).await;
    assert_eq!(get.out, "recording.codec = hevc\nrecording.fps = 60\n");
    let all = cli(&r, &["settings", "get", "--json"]).await.json();
    assert_eq!(all["settings"]["recording.size"], "source");
    // Errors: odd pair count, unknown key, secret key.
    assert_eq!(cli(&r, &["settings", "set", "a.b"]).await.code, 2);
    assert_eq!(cli(&r, &["settings", "get", "never.set"]).await.code, 2);
    assert_eq!(
        cli(&r, &["settings", "set", "auth.token", "x"]).await.code,
        2
    );
}

/// AGT-1.AC2: with no daemon running, the CLI starts one and the command completes.
#[tokio::test]
async fn starts_a_daemon_when_none_is_running() {
    let dir = tempfile::tempdir().unwrap();
    let endpoint = Endpoint::Path(dir.path().join("late.sock"));
    let starts = Arc::new(AtomicUsize::new(0));
    let launcher: Launcher = {
        let (starts, endpoint, base) = (starts.clone(), endpoint.clone(), dir.path().to_owned());
        Arc::new(move |_| {
            starts.fetch_add(1, Ordering::SeqCst);
            let daemon = make_daemon(desktop(), &base);
            let server = IpcServer::bind(&endpoint)?;
            tokio::spawn(async move {
                let _ = server.run(daemon).await;
            });
            Ok(())
        })
    };
    let (mut env, o, e) = env_for(&endpoint, Some(launcher.clone()), "");
    let code = run(
        [
            "capturemann",
            "capture",
            "screenshot",
            "--display",
            "1",
            "--json",
        ],
        &mut env,
    )
    .await;
    assert_eq!(code, 0, "{}", e.contents());
    assert_eq!(
        serde_json::from_str::<Value>(&o.contents()).unwrap()["width"],
        400
    );
    assert_eq!(starts.load(Ordering::SeqCst), 1);
    // A second command reuses the running daemon.
    let (mut env, _, _) = env_for(&endpoint, Some(launcher), "");
    assert_eq!(run(["capturemann", "targets"], &mut env).await, 0);
    assert_eq!(starts.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn no_daemon_gives_a_clear_error() {
    let dir = tempfile::tempdir().unwrap();
    let endpoint = Endpoint::Path(dir.path().join("none.sock"));
    // No launcher.
    let (mut env, o, e) = env_for(&endpoint, None, "");
    assert_eq!(run(["capturemann", "targets"], &mut env).await, 5);
    assert!(o.contents().is_empty());
    assert!(
        e.contents().contains("error[daemon_unavailable]") && e.contents().contains("daemon start"),
        "{}",
        e.contents()
    );
    // --no-start beats a launcher.
    let launcher: Launcher = Arc::new(|_| panic!("must not start a daemon"));
    let (mut env, _, e) = env_for(&endpoint, Some(launcher), "");
    assert_eq!(
        run(["capturemann", "--no-start", "targets"], &mut env).await,
        5
    );
    assert!(e.contents().contains("daemon_unavailable"));
    // A launcher that fails.
    let failing: Launcher = Arc::new(|_| Err(std::io::Error::other("no cm-daemon binary")));
    let (mut env, _, e) = env_for(&endpoint, Some(failing), "");
    assert_eq!(run(["capturemann", "targets"], &mut env).await, 5);
    assert!(
        e.contents().contains("no cm-daemon binary"),
        "{}",
        e.contents()
    );
}

#[tokio::test]
async fn daemon_status_and_start() {
    let r = rig();
    let st = cli(&r, &["daemon", "status", "--json"]).await;
    assert_eq!(st.code, 0, "{}", st.err);
    assert_eq!(st.json()["actions"], ACTIONS.len());
    let start = cli(&r, &["daemon", "start", "--json"]).await.json();
    assert_eq!(
        (start["running"].clone(), start["started"].clone()),
        (json!(true), json!(false))
    );

    let dir = tempfile::tempdir().unwrap();
    let down = Rig {
        endpoint: Endpoint::Path(dir.path().join("x.sock")),
        daemon: r.daemon.clone(),
        dir,
    };
    assert_eq!(cli(&down, &["daemon", "status"]).await.code, 5);
}

/// `--socket` overrides the environment's endpoint.
#[tokio::test]
async fn socket_flag_overrides_endpoint() {
    let r = rig();
    let dir = tempfile::tempdir().unwrap();
    let wrong = Rig {
        endpoint: Endpoint::Path(dir.path().join("wrong.sock")),
        daemon: r.daemon.clone(),
        dir,
    };
    let Endpoint::Path(p) = &r.endpoint else {
        unreachable!()
    };
    let o = cli(
        &wrong,
        &["--socket", p.to_str().unwrap(), "targets", "--json"],
    )
    .await;
    assert_eq!(o.code, 0, "{}", o.err);
}

/// Generic event streaming with a non-job topic (job topics are covered below).
#[tokio::test]
async fn jobs_watch_streams_events() {
    let r = rig();
    let (mut env, o, e) = env_for(&r.endpoint, None, "");
    let watch = tokio::spawn(async move {
        let code = run(
            [
                "capturemann",
                "--json",
                "jobs",
                "watch",
                "--topic",
                "capture.*",
                "--max-events",
                "1",
            ],
            &mut env,
        )
        .await;
        (code, o.contents(), e.contents())
    });
    // Keep capturing until the subscription is live and the watcher has seen one event.
    let mut watch = watch;
    let (code, out, err) = loop {
        tokio::select! {
            res = &mut watch => break res.unwrap(),
            () = tokio::time::sleep(Duration::from_millis(50)) => {
                assert_eq!(cli(&r, &["capture", "screenshot", "--display", "2"]).await.code, 0);
            }
        }
    };
    assert_eq!(code, 0, "{err}");
    let ev: Value = serde_json::from_str(out.lines().next().unwrap()).unwrap();
    assert_eq!(ev["event"], "capture.created");
}

/// A rig whose daemon has a job queue with two test handlers: `hold` reports 50% and then
/// waits to be cancelled; `quick` reports 50% and finishes.
fn jobs_rig() -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let endpoint = Endpoint::Path(dir.path().join("d.sock"));
    let lib = Library::open_or_create(&dir.path().join("lib")).unwrap();
    let queue = cm_daemon::library_job_queue(lib.root()).unwrap();
    let daemon = Daemon::builder(Capturer::new(desktop()).with_own_pid(None), lib)
        .jobs(queue)
        .build();
    daemon
        .register_job_handler(
            "hold",
            cm_jobs::handler_fn(|mut ctx| async move {
                ctx.reporter.report(0.5, Some("halfway".to_owned()));
                ctx.cancel.interrupted().await;
                Err(cm_core::CmError::internal("stopped"))
            }),
        )
        .unwrap();
    daemon
        .register_job_handler(
            "quick",
            cm_jobs::handler_fn(|ctx| async move {
                ctx.reporter.report(0.5, Some("working".to_owned()));
                Ok(json!({"path": "/tmp/out.mp4"}))
            }),
        )
        .unwrap();
    daemon.start_jobs().unwrap();
    serve(&daemon, &endpoint);
    Rig {
        dir,
        endpoint,
        daemon,
    }
}

fn enqueue(r: &Rig, job_type: &str) -> String {
    r.daemon
        .job_queue()
        .unwrap()
        .enqueue(cm_jobs::NewJob::new(job_type, json!({})))
        .unwrap()
        .id
}

async fn wait_state(r: &Rig, id: &str, state: &str) -> Value {
    for _ in 0..500 {
        let o = cli(r, &["--json", "jobs", "get", id]).await;
        assert_eq!(o.code, 0, "{}", o.err);
        let j = o.json();
        if j["state"] == state {
            return j;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("job {id} never became {state}");
}

/// EXP-4 / STO-3 / AGT-1: `jobs list|get|cancel` against an in-process daemon.
#[tokio::test]
async fn jobs_list_get_cancel() {
    let r = jobs_rig();
    let hold = enqueue(&r, "hold");
    let quick = enqueue(&r, "quick");
    let j = wait_state(&r, &quick, "done").await;
    assert_eq!(j["result"]["path"], "/tmp/out.mp4");
    wait_state(&r, &hold, "running").await;

    let all = cli(&r, &["--json", "jobs", "list"]).await.json();
    assert_eq!(all["jobs"].as_array().unwrap().len(), 2);
    let done = cli(&r, &["--json", "jobs", "list", "--state", "done"])
        .await
        .json();
    assert_eq!(done["jobs"][0]["job_id"], quick);
    assert_eq!(done["jobs"].as_array().unwrap().len(), 1);
    let typed = cli(
        &r,
        &["--json", "jobs", "list", "--type", "hold", "--limit", "1"],
    )
    .await
    .json();
    assert_eq!(typed["jobs"][0]["job_id"], hold);

    // Human output.
    let o = cli(&r, &["jobs", "get", &hold]).await;
    assert_eq!(o.code, 0, "{}", o.err);
    assert!(o.out.contains(&hold) && o.out.contains("running") && o.out.contains("50%"));
    assert!(o.out.contains("halfway"), "{}", o.out);
    let o = cli(&r, &["jobs", "list"]).await;
    assert!(
        o.out.contains("quick") && o.out.contains("done"),
        "{}",
        o.out
    );

    // Cancel is idempotent; unknown ids are not_found (exit 2); finished jobs are refused.
    let o = cli(&r, &["--json", "jobs", "cancel", &hold]).await;
    assert_eq!(o.code, 0, "{}", o.err);
    wait_state(&r, &hold, "cancelled").await;
    let o = cli(
        &r,
        &["--json", "--request-id", "c-1", "jobs", "cancel", &hold],
    )
    .await;
    assert_eq!(o.code, 0, "{}", o.err);
    assert_eq!(o.json()["state"], "cancelled");
    let o = cli(&r, &["jobs", "cancel", &hold]).await;
    assert_eq!(o.code, 0);
    assert!(o.out.contains("cancelled"), "{}", o.out);
    assert_eq!(cli(&r, &["jobs", "get", "nope"]).await.code, 2);
    assert_eq!(cli(&r, &["jobs", "cancel", &quick]).await.code, 2);
    assert_eq!(cli(&r, &["jobs", "list", "--state", "stuck"]).await.code, 2);
}

/// Without a job queue the daemon says so; the CLI does not print an empty list.
#[tokio::test]
async fn jobs_without_a_queue_is_an_error() {
    let r = rig();
    let o = cli(&r, &["jobs", "list"]).await;
    assert_ne!(o.code, 0);
    assert!(o.err.contains("job queue"), "{}", o.err);
}

/// `jobs watch`: a snapshot of the active jobs, then real `job.progress` / `job.done` events.
#[tokio::test]
async fn jobs_watch_snapshot_then_job_events() {
    let r = jobs_rig();
    let hold = enqueue(&r, "hold");
    wait_state(&r, &hold, "running").await;

    let (mut env, o, e) = env_for(&r.endpoint, None, "");
    let mut watch = tokio::spawn(async move {
        let code = run(
            [
                "capturemann",
                "--json",
                "jobs",
                "watch",
                "--max-events",
                "2",
            ],
            &mut env,
        )
        .await;
        (code, e.contents())
    });
    // The snapshot is printed once the subscription is live.
    for _ in 0..500 {
        if o.contents().contains("job.snapshot") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let quick = enqueue(&r, "quick");
    let (code, err) = tokio::time::timeout(Duration::from_secs(5), &mut watch)
        .await
        .expect("watch did not finish")
        .unwrap();
    assert_eq!(code, 0, "{err}");
    let lines: Vec<Value> = o
        .contents()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines[0]["event"], "job.snapshot");
    assert_eq!(lines[0]["data"]["job_id"], hold);
    let rest: Vec<&str> = lines[1..]
        .iter()
        .map(|l| l["event"].as_str().unwrap())
        .collect();
    assert_eq!(rest, ["job.progress", "job.done"]);
    assert!(lines[1..].iter().all(|l| l["data"]["job_id"] == quick));
}

/// EXP-1 / EXP-4 / REC-1: `capturemann export <id> [--preset quick] [--wait]` calls
/// `export_video`; `--wait` prints the file when the job is done and maps a failed job to an exit
/// code (a missing FFmpeg stays `not_found`, exit 2); other presets are `invalid_input`.
#[tokio::test]
async fn exp1_export_command() {
    use std::sync::atomic::AtomicBool;
    let dir = tempfile::tempdir().unwrap();
    let endpoint = Endpoint::Path(dir.path().join("d.sock"));
    let lib = Library::open_or_create(&dir.path().join("lib")).unwrap();
    let queue = cm_daemon::library_job_queue(lib.root()).unwrap();
    let daemon = Daemon::builder(Capturer::new(desktop()).with_own_pid(None), lib)
        .jobs(queue)
        .recording_sources(cm_daemon::testing::FakeSources::new())
        .recording_options(cm_daemon::EngineOptions {
            tick_every: Duration::from_millis(20),
            countdown_step: Duration::from_millis(20),
            host: Arc::new(cm_core::ManualHostClock::new(1_000_000_000)),
            free_space: Arc::new(cm_capture::disk::FakeFreeSpace::new(500_000_000_000)),
            disk_check_every: Duration::ZERO,
            own_pid: None,
            ..cm_daemon::EngineOptions::default()
        })
        .build();
    let fail = Arc::new(AtomicBool::new(false));
    let f2 = fail.clone();
    daemon
        .register_job_handler(
            "export",
            cm_jobs::handler_fn(move |ctx| {
                let fail = f2.clone();
                async move {
                    if fail.load(Ordering::SeqCst) {
                        return Err(cm_core::CmError::not_found(
                            "FFmpeg was not found: install it with Homebrew (brew install ffmpeg)",
                        ));
                    }
                    ctx.reporter.report(0.5, None);
                    Ok(json!({"capture_id": "x", "path": "/lib/Take.cmrec/exports/Take - quick.mp4",
                              "size_bytes": 1234, "duration_s": 2.5, "encoder": "libx264"}))
                }
            }),
        )
        .unwrap();
    daemon.start_jobs().unwrap();
    serve(&daemon, &endpoint);
    let r = Rig {
        dir,
        endpoint,
        daemon: daemon.clone(),
    };
    // One recording to export (auto-export off, this test drives it by hand).
    daemon
        .execute_method(
            cm_core::Actor::User,
            0,
            "set_settings",
            &json!({"values": {"export.auto_on_stop": false}}),
        )
        .await
        .unwrap();
    let st = daemon
        .execute_method(
            cm_core::Actor::User,
            0,
            "start_recording",
            &json!({"target": {"display_id": 1}, "countdown_s": 0}),
        )
        .await
        .unwrap();
    let id = st["capture_id"].as_str().unwrap().to_owned();
    daemon
        .execute_method(cm_core::Actor::User, 0, "stop_recording", &json!({}))
        .await
        .unwrap();

    // Without --wait: the job id and a hint.
    let o = cli(&r, &["--json", "export", &id]).await;
    assert_eq!(o.code, 0, "{}", o.err);
    let j = o.json();
    assert_eq!(j["preset"], "quick");
    let job = j["job_id"].as_str().unwrap().to_owned();
    wait_state(&r, &job, "done").await;
    let o = cli(&r, &["export", &id, "--preset", "quick"]).await;
    assert_eq!(o.code, 0, "{}", o.err);
    assert!(o.out.contains("jobs get"), "{}", o.out);

    // AUD-4: --loudness is sent as loudness_lufs (a different target is its own job); a value
    // outside -40..-5 is a usage error.
    let o = cli(&r, &["--json", "export", &id, "--loudness", "-14"]).await;
    assert_eq!(o.code, 0, "{}", o.err);
    assert_eq!(o.json()["loudness_lufs"], -14.0);
    assert_eq!(cli(&r, &["export", &id, "--loudness", "-3"]).await.code, 2);
    assert_eq!(
        cli(&r, &["export", &id, "--loudness", "loud"]).await.code,
        2
    );

    // --wait: waits for the file.
    let o = cli(&r, &["export", &id, "--wait"]).await;
    assert_eq!(o.code, 0, "{}", o.err);
    assert!(
        o.out.contains("Take - quick.mp4") && o.out.contains("1234"),
        "{}",
        o.out
    );
    let o = cli(&r, &["--json", "export", &id, "--wait"]).await;
    assert_eq!(o.json()["result"]["encoder"], "libx264");

    // A missing FFmpeg: the job fails, the command exits 2 (not_found) with the hint.
    fail.store(true, Ordering::SeqCst);
    let o = cli(&r, &["export", &id, "--wait"]).await;
    assert_eq!(o.code, 2, "{}", o.err);
    assert!(o.err.contains("brew install ffmpeg"), "{}", o.err);

    // Unknown presets, options that do not belong to the preset, and unknown captures.
    let o = cli(&r, &["export", &id, "--preset", "tiktok"]).await;
    assert_eq!(o.code, 2);
    assert!(o.err.contains("unknown preset"), "{}", o.err);
    let o = cli(&r, &["export", &id, "--width", "640"]).await;
    assert_eq!(o.code, 2);
    assert!(o.err.contains("gif"), "{}", o.err);
    assert_eq!(cli(&r, &["export", "rec_nope"]).await.code, 2);
    // EXP-3: `--format gif` and the size options are sent as the gif preset with its options.
    // (The recording of this test has no screen size, so the request is refused after parsing.)
    let o = cli(
        &r,
        &[
            "--json", "export", &id, "--format", "gif", "--width", "640", "--fps", "10",
            "--max-mb", "5",
        ],
    )
    .await;
    assert_eq!(o.code, 2, "{}", o.err);
    assert!(o.err.contains("screen track"), "{}", o.err);
    assert_eq!(
        cli(&r, &["export", &id, "--format", "avi"]).await.code,
        2,
        "an unknown format is a usage error"
    );

    // VOC-2: `--audio` calls `export_audio`. This recording has no audio file, so the daemon
    // refuses it by name; the flags that belong to a video (or only to --audio) are usage errors.
    let o = cli(&r, &["export", &id, "--audio"]).await;
    assert_eq!(o.code, 2, "{}", o.err);
    assert!(o.err.contains("no audio track"), "{}", o.err);
    let o = cli(&r, &["export", &id, "--format", "wav", "--tracks", "mic"]).await;
    assert_eq!(o.code, 2, "{}", o.err);
    assert!(o.err.contains("no microphone track"), "{}", o.err);
    for args in [
        vec!["export", &id, "--audio", "--preset", "quick"],
        vec!["export", &id, "--audio", "--format", "gif"],
        vec!["export", &id, "--tracks", "mic"],
        vec!["export", &id, "--audio", "--tracks", "both"],
        vec!["export", &id, "--loudness", "off"],
        vec!["export", &id, "--audio", "--loudness", "-3"],
    ] {
        assert_eq!(cli(&r, &args).await.code, 2, "{args:?}");
    }
}

/// R2-14: `export --wait --timeout` gives up on a held export (a recording holds exports),
/// says it is held, and leaves the job running (exit 6).
#[tokio::test]
async fn r2_14_export_wait_timeout_reports_a_held_job() {
    let dir = tempfile::tempdir().unwrap();
    let endpoint = Endpoint::Path(dir.path().join("d.sock"));
    let lib = Library::open_or_create(&dir.path().join("lib")).unwrap();
    let queue = cm_daemon::library_job_queue(lib.root()).unwrap();
    let daemon = Daemon::builder(Capturer::new(desktop()).with_own_pid(None), lib)
        .jobs(queue)
        .recording_sources(cm_daemon::testing::FakeSources::new())
        .recording_options(cm_daemon::EngineOptions {
            tick_every: Duration::from_millis(20),
            countdown_step: Duration::from_millis(20),
            host: Arc::new(cm_core::ManualHostClock::new(1_000_000_000)),
            free_space: Arc::new(cm_capture::disk::FakeFreeSpace::new(500_000_000_000)),
            disk_check_every: Duration::ZERO,
            own_pid: None,
            ..cm_daemon::EngineOptions::default()
        })
        .build();
    daemon
        .register_job_handler(
            "export",
            cm_jobs::handler_fn(|_ctx| async move { Ok(json!({"path": "/x.mp4"})) }),
        )
        .unwrap();
    daemon.start_jobs().unwrap();
    serve(&daemon, &endpoint);
    let r = Rig {
        dir,
        endpoint,
        daemon: daemon.clone(),
    };
    let st = daemon
        .execute_method(
            cm_core::Actor::User,
            0,
            "start_recording",
            &json!({"target": {"display_id": 1}, "countdown_s": 0}),
        )
        .await
        .unwrap();
    let id = st["capture_id"].as_str().unwrap().to_owned();
    daemon
        .execute_method(cm_core::Actor::User, 0, "stop_recording", &json!({}))
        .await
        .unwrap();
    // Let the auto-export finish, then hold exports (a second recording would do the same).
    tokio::time::sleep(Duration::from_millis(300)).await;
    daemon.hold_job_type("export");
    daemon
        .job_queue()
        .unwrap()
        .enqueue(cm_jobs::NewJob::new(
            "export",
            json!({"capture_id": id, "preset": "quick", "by": "user", "auto": false}),
        ))
        .unwrap();
    // `export_video` reuses the queued (held) job; the CLI waits, says so, then times out.
    let o = cli(&r, &["export", &id, "--wait", "--timeout", "0.6"]).await;
    assert_eq!(o.code, 6, "{}\n{}", o.out, o.err);
    assert!(o.err.contains("held"), "{}", o.err);
    assert!(o.err.contains("still held"), "{}", o.err);
    // The job was left alone: releasing the hold lets it finish.
    daemon.resume_job_type("export");
    let o = cli(&r, &["export", &id, "--wait", "--timeout", "5"]).await;
    assert_eq!(o.code, 0, "{}", o.err);
    // `--timeout` needs `--wait`, and must be positive.
    assert_eq!(cli(&r, &["export", &id, "--timeout", "5"]).await.code, 2);
    assert_eq!(
        cli(&r, &["export", &id, "--wait", "--timeout", "0"])
            .await
            .code,
        2
    );
}

/// R1-1 regression: `capture screenshot --copy --annotate` puts the ANNOTATED image on the
/// clipboard (never the unredacted original), and says so truthfully.
#[tokio::test]
async fn r1_1_screenshot_copy_with_annotate_copies_the_rendered_image() {
    use cm_daemon::testing::RecordingClipboard;
    let dir = tempfile::tempdir().unwrap();
    let endpoint = Endpoint::Path(dir.path().join("d.sock"));
    let lib = Library::open_or_create(&dir.path().join("lib")).unwrap();
    let clip = Arc::new(RecordingClipboard::default());
    let daemon = Daemon::builder(Capturer::new(desktop()).with_own_pid(None), lib)
        .clipboard(clip.clone())
        .build();
    serve(&daemon, &endpoint);
    let r = Rig {
        dir,
        endpoint,
        daemon,
    };
    // Plain `--copy`: the capture itself.
    let plain = cli(
        &r,
        &[
            "capture",
            "screenshot",
            "--display",
            "1",
            "--copy",
            "--json",
        ],
    )
    .await;
    assert_eq!(plain.code, 0, "{}", plain.err);
    assert_eq!(plain.json()["copied_to_clipboard"], true);
    let original = clip.last_png().unwrap();

    // `--copy --annotate`: the rendered image, and only that.
    let blur = r#"[{"op":"pixelate","rect":[0,0,200,150]}]"#;
    let out = r.dir.path().join("out.png");
    let o = cli(
        &r,
        &[
            "capture",
            "screenshot",
            "--display",
            "1",
            "--copy",
            "--annotate",
            blur,
            "--out",
            out.to_str().unwrap(),
            "--json",
        ],
    )
    .await;
    assert_eq!(o.code, 0, "{}", o.err);
    assert_eq!(o.json()["copied_to_clipboard"], true);
    let copied = clip.last_png().unwrap();
    assert_ne!(
        copied, original,
        "the redacted image was copied, not the original"
    );
    assert_eq!(
        copied,
        std::fs::read(&out).unwrap(),
        "clipboard == the saved rendered PNG"
    );

    // `annotate --copy` copies the new revision too.
    let id = o.json()["capture_id"].as_str().unwrap().to_owned();
    let rect = r##"[{"op":"rect","rect":[10,10,50,30],"color":"#FF3B3B","width":4}]"##;
    let a = cli(&r, &["annotate", &id, "--ops", rect, "--copy", "--json"]).await;
    assert_eq!(a.code, 0, "{}", a.err);
    assert_eq!(a.json()["copied_to_clipboard"], true);
    assert_ne!(clip.last_png().unwrap(), copied);
}

/// `library rename`, `library rm` (Confirm: the folder goes to the Trash, here a fake) and
/// `annotate --no-image`.
#[tokio::test]
async fn library_rename_rm_and_annotate_no_image() {
    use cm_daemon::testing::{FakeTrasher, FixedConfirmer};
    use cm_daemon::Decision;
    let dir = tempfile::tempdir().unwrap();
    let endpoint = Endpoint::Path(dir.path().join("d.sock"));
    let lib = Library::open_or_create(&dir.path().join("lib")).unwrap();
    let confirmer = Arc::new(FixedConfirmer::new(Decision::Approve));
    let trasher = Arc::new(FakeTrasher::new());
    let daemon = Daemon::builder(Capturer::new(desktop()).with_own_pid(None), lib)
        .confirmer(confirmer.clone())
        .trasher(trasher.clone())
        .build();
    serve(&daemon, &endpoint);
    let r = Rig {
        dir,
        endpoint,
        daemon,
    };
    let shot = cli(
        &r,
        &[
            "capture",
            "screenshot",
            "--display",
            "1",
            "--title",
            "Before",
            "--json",
        ],
    )
    .await
    .json();
    let id = shot["capture_id"].as_str().unwrap().to_owned();

    let o = cli(
        &r,
        &["library", "rename", &id, "After the rename", "--json"],
    )
    .await;
    assert_eq!(o.code, 0, "{}", o.err);
    assert_eq!(o.json()["title"], "After the rename");
    let human = cli(&r, &["library", "rename", &id, "Third"]).await;
    assert!(
        human.out.contains("Renamed") && human.out.contains("Third"),
        "{}",
        human.out
    );
    assert!(confirmer.shown().is_empty());

    let a = cli(&r, &["annotate", &id, "--ops", OPS, "--no-image", "--json"]).await;
    assert_eq!(a.code, 0, "{}", a.err);
    assert!(a.json()["image"].is_null(), "{}", a.out);
    let both = cli(
        &r,
        &[
            "annotate",
            &id,
            "--ops",
            OPS,
            "--no-image",
            "--out",
            "x.png",
        ],
    )
    .await;
    assert_eq!(both.code, 2, "usage error");

    // `--also-remote` on a capture with no remote copy: the dialog says so, then it is a plain
    // delete.
    let rm = cli(&r, &["library", "rm", &id, "--also-remote"]).await;
    assert_eq!(rm.code, 0, "{}", rm.err);
    assert!(rm.out.contains("Trash"), "{}", rm.out);
    assert!(!rm.out.contains("Deleted remotely"), "{}", rm.out);
    assert!(
        confirmer.shown()[0].details["remote"]
            .as_str()
            .is_some_and(|t| t.contains("No remote copy")),
        "{}",
        confirmer.shown()[0].details
    );
    assert_eq!(confirmer.shown().len(), 1);
    assert_eq!(trasher.trashed().len(), 1);
    let list = cli(&r, &["library", "list", "--json"]).await.json();
    assert!(list["captures"].as_array().unwrap().is_empty());
    let again = cli(&r, &["library", "rm", &id]).await;
    assert_eq!(again.code, cm_cli::exit_code_for("not_found"));
}

/// LIB-2.AC5, AC6: `capturemann library move` asks on screen (Confirm), copies in a job, waits,
/// and the fresh and use-existing modes only switch.
#[tokio::test]
async fn library_move_asks_copies_and_waits() {
    use cm_daemon::testing::FixedConfirmer;
    use cm_daemon::Decision;
    let dir = tempfile::tempdir().unwrap();
    let endpoint = Endpoint::Path(dir.path().join("d.sock"));
    let lib_dir = dir.path().join("lib");
    let lib = Library::open_or_create(&lib_dir).unwrap();
    let confirmer = Arc::new(FixedConfirmer::new(Decision::Approve));
    let daemon = Daemon::builder(Capturer::new(desktop()).with_own_pid(None), lib)
        .confirmer(confirmer.clone())
        .jobs(cm_daemon::library_job_queue(&lib_dir).unwrap())
        .build();
    daemon.register_library_move_handler().unwrap();
    daemon.start_jobs().unwrap();
    serve(&daemon, &endpoint);
    let r = Rig {
        dir,
        endpoint,
        daemon,
    };
    let shot = cli(&r, &["capture", "screenshot", "--display", "1", "--json"]).await;
    assert_eq!(shot.code, 0, "{}", shot.err);
    let to = r.dir.path().join("elsewhere");
    let to_s = to.to_str().unwrap();
    let o = cli(&r, &["library", "move", to_s, "--wait", "--json"]).await;
    assert_eq!(o.code, 0, "{} {}", o.out, o.err);
    assert_eq!(o.json()["result"]["bundles"], 1);
    assert_eq!(o.json()["result"]["restart_required"], true);
    assert_eq!(confirmer.shown().len(), 1);
    assert_eq!(confirmer.shown()[0].kind, "move_library");
    assert!(std::fs::read_dir(&to)
        .unwrap()
        .flatten()
        .any(|e| e.file_name().to_string_lossy().ends_with(".cmshot")));
    // The folder now holds a library: a plain move is refused with `invalid_input` (exit 2)...
    let again = cli(&r, &["library", "move", to_s]).await;
    assert_eq!(
        again.code,
        cm_cli::exit_code_for("invalid_input"),
        "{}",
        again.err
    );
    assert!(again.err.contains("use_existing"), "{}", again.err);
    // ...and `--use-existing` switches without a job; `--fresh` conflicts with it.
    let used = cli(&r, &["library", "move", to_s, "--use-existing"]).await;
    assert_eq!(used.code, 0, "{}", used.err);
    assert!(used.out.contains("Restart CaptureMann"), "{}", used.out);
    let clash = cli(&r, &["library", "move", to_s, "--fresh", "--use-existing"]).await;
    assert_eq!(clash.code, 2);
    // Fresh in a new folder.
    let fresh = r.dir.path().join("fresh");
    let f = cli(
        &r,
        &[
            "library",
            "move",
            fresh.to_str().unwrap(),
            "--fresh",
            "--json",
        ],
    )
    .await;
    assert_eq!(f.code, 0, "{}", f.err);
    assert_eq!(f.json()["mode"], "fresh");
    assert!(f.json()["job_id"].is_null());
}

/// AGT-1.AC4: every daemon action has a CLI mapping entry (or is a known gap).
#[test]
fn cli_parity_table_covers_every_action() {
    // Internal actions are explicitly not CLI commands (`cm_daemon::INTERNAL_ACTIONS`).
    let names: Vec<&str> = ACTIONS
        .iter()
        .map(|a| a.name)
        .filter(|n| !is_internal(n))
        .collect();
    for n in INTERNAL_ACTIONS {
        assert!(
            !CLI_COMMANDS.iter().any(|(m, _)| m == n),
            "internal action `{n}` must not have a CLI command"
        );
    }
    for n in &names {
        assert!(
            CLI_COMMANDS.iter().any(|(m, _)| m == n),
            "action `{n}` has no CLI mapping entry"
        );
    }
    for (m, cmd) in CLI_COMMANDS {
        assert!(names.contains(m), "stale mapping `{m}`");
        if let Some(cmd) = cmd {
            // The mapped command parses: its first word is a real subcommand.
            let o = std::process::Command::new(env!("CARGO_BIN_EXE_capturemann"))
                .args(cmd.split(' '))
                .arg("--help")
                .output()
                .unwrap();
            assert!(
                o.status.success(),
                "`capturemann {cmd} --help` failed for {m}"
            );
        }
    }
}

// ---- recording ---------------------------------------------------------------------------

mod recording {
    use super::*;
    use cm_core::{ManualHostClock, Permission};
    use cm_daemon::testing::{FakeSources, FixedConfirmer, NeverAnswers};
    use cm_daemon::{Decision, EngineOptions};

    pub struct RecRig {
        pub rig: Rig,
        pub confirmer: Arc<FixedConfirmer>,
    }

    pub fn options() -> EngineOptions {
        EngineOptions {
            tick_every: Duration::from_millis(20),
            countdown_step: Duration::from_millis(20),
            host: Arc::new(ManualHostClock::new(1_000_000_000)),
            free_space: Arc::new(cm_capture::disk::FakeFreeSpace::new(500_000_000_000)),
            disk_check_every: Duration::ZERO,
            own_pid: None,
            selftest_duration: Duration::from_millis(300),
            ..EngineOptions::default()
        }
    }

    pub fn rec_rig(decision: Decision, sources: FakeSources) -> RecRig {
        let dir = tempfile::tempdir().unwrap();
        let endpoint = Endpoint::Path(dir.path().join("d.sock"));
        let lib = Library::open_or_create(&dir.path().join("lib")).unwrap();
        let confirmer = Arc::new(FixedConfirmer::new(decision));
        let daemon = Daemon::builder(Capturer::new(desktop()).with_own_pid(None), lib)
            .recording_sources(sources.clone())
            .recording_options(options())
            .confirmer(confirmer.clone())
            .build();
        serve(&daemon, &endpoint);
        RecRig {
            rig: Rig {
                dir,
                endpoint,
                daemon,
            },
            confirmer,
        }
    }

    pub fn timeout_rig(sources: FakeSources) -> Rig {
        let dir = tempfile::tempdir().unwrap();
        let endpoint = Endpoint::Path(dir.path().join("d.sock"));
        let lib = Library::open_or_create(&dir.path().join("lib")).unwrap();
        let daemon = Daemon::builder(Capturer::new(desktop()).with_own_pid(None), lib)
            .recording_sources(sources)
            .recording_options(options())
            .confirmer(NeverAnswers)
            .confirm_timeout(Duration::from_millis(150))
            .build();
        serve(&daemon, &endpoint);
        Rig {
            dir,
            endpoint,
            daemon,
        }
    }

    pub fn permission(p: Permission) -> FakeSources {
        FakeSources::new().with_missing_permission(p)
    }
}

/// REC-3: `record selftest` runs the 60 fps self-test (human and JSON output); a pass opens
/// `record start --fps 60`, which is refused with exit 2 and a suggestion before it.
#[tokio::test]
async fn record_selftest_unlocks_60_fps() {
    let r = recording::rec_rig(
        cm_daemon::Decision::Approve,
        cm_daemon::testing::FakeSources::new(),
    );
    let o = cli(
        &r.rig,
        &["record", "start", "--fps", "60", "--countdown", "0"],
    )
    .await;
    assert_eq!(o.code, 2, "{}", o.err);
    assert!(o.err.contains("error[invalid_input]"), "{}", o.err);
    assert!(
        o.err.contains("suggestion: record at 30 fps")
            && o.err.contains("capturemann record selftest"),
        "{}",
        o.err
    );
    let j = cli(
        &r.rig,
        &[
            "--json",
            "record",
            "start",
            "--fps",
            "60",
            "--countdown",
            "0",
        ],
    )
    .await;
    assert_eq!(j.code, 2);
    let e: Value = serde_json::from_str(j.err.trim().lines().last().unwrap()).unwrap();
    assert_eq!(e["error"]["data"]["suggestion"]["fps"], 30, "{e}");
    assert!(r.confirmer.shown().is_empty(), "refused before any dialog");

    let t = cli(&r.rig, &["record", "selftest"]).await;
    assert_eq!(t.code, 0, "{}", t.err);
    assert!(t.err.contains("60 fps self-test"), "{}", t.err);
    assert!(t.out.contains("60 fps self-test passed"), "{}", t.out);
    assert!(t.out.contains("late frames: 0.00%"), "{}", t.out);
    assert!(t.out.contains("record start --fps 60"), "{}", t.out);
    let t = cli(&r.rig, &["--json", "record", "selftest"]).await;
    assert_eq!(t.code, 0, "{}", t.err);
    let v = t.json();
    assert_eq!(v["state"], "passed");
    assert_eq!(v["fps"], 60);
    assert_eq!(v["stored"], true);

    let o = cli(
        &r.rig,
        &[
            "--json",
            "record",
            "start",
            "--fps",
            "60",
            "--countdown",
            "0",
        ],
    )
    .await;
    assert_eq!(o.code, 0, "{}", o.err);
    assert_eq!(o.json()["indicator"]["output"]["fps"], 60);
    let c = cli(&r.rig, &["record", "stop"]).await;
    assert_eq!(c.code, 0, "{}", c.err);
}

/// A failed self-test says so and what to try; the exit code is 0 (the command worked).
#[tokio::test]
async fn record_selftest_reports_a_fail_with_a_suggestion() {
    let r = recording::rec_rig(
        cm_daemon::Decision::Approve,
        cm_daemon::testing::FakeSources::new().with_screen_quality(3.5, 0),
    );
    let o = cli(&r.rig, &["record", "selftest"]).await;
    assert_eq!(o.code, 0, "{}", o.err);
    assert!(o.out.contains("60 fps self-test failed"), "{}", o.out);
    assert!(o.out.contains("late frames: 3.50%"), "{}", o.out);
    assert!(o.out.contains("isn't stable on this machine"), "{}", o.out);
    let s = cli(
        &r.rig,
        &["record", "start", "--fps", "60", "--countdown", "0"],
    )
    .await;
    assert_eq!(s.code, 2);
    assert!(s.err.contains("isn't stable"), "{}", s.err);
}

/// `record stop` cancels a running self-test and nothing is kept.
#[tokio::test]
async fn record_stop_cancels_a_running_selftest() {
    let mut opts = recording::options();
    opts.selftest_duration = Duration::from_secs(30);
    let dir = tempfile::tempdir().unwrap();
    let endpoint = Endpoint::Path(dir.path().join("d.sock"));
    let lib = Library::open_or_create(&dir.path().join("lib")).unwrap();
    let daemon = Daemon::builder(Capturer::new(desktop()).with_own_pid(None), lib)
        .recording_sources(cm_daemon::testing::FakeSources::new())
        .recording_options(opts)
        .confirmer(cm_daemon::testing::FixedConfirmer::new(
            cm_daemon::Decision::Approve,
        ))
        .build();
    serve(&daemon, &endpoint);
    let r = Rig {
        dir,
        endpoint,
        daemon,
    };
    let rig2 = Rig {
        dir: tempfile::tempdir().unwrap(),
        endpoint: r.endpoint.clone(),
        daemon: r.daemon.clone(),
    };
    let t = tokio::spawn(async move { cli(&rig2, &["record", "selftest"]).await });
    for _ in 0..500 {
        if r.daemon.selftest_active() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(r.daemon.selftest_active());
    let c = cli(&r, &["record", "stop"]).await;
    assert_eq!(c.code, 0, "{}", c.err);
    assert!(c.out.contains("self-test was cancelled"), "{}", c.out);
    let t = t.await.unwrap();
    assert_eq!(t.code, 3, "{}{}", t.out, t.err);
    assert!(!r.daemon.selftest_active());
}

/// REC-1, REC-4, AGT-1: `record start` waits for the confirmation (and says so on stderr), then
/// prints what is recording; status, pause, resume and stop follow, in human and JSON form.
#[tokio::test]
async fn record_start_status_pause_resume_stop() {
    let r = recording::rec_rig(
        cm_daemon::Decision::Approve,
        cm_daemon::testing::FakeSources::new(),
    );
    let o = cli(&r.rig, &["record", "start", "--countdown", "0", "--mic"]).await;
    assert_eq!(o.code, 0, "{}", o.err);
    assert!(
        o.err
            .contains("Waiting for confirmation in the CaptureMann window"),
        "{}",
        o.err
    );
    assert!(o.out.contains("Recording started: rec_"), "{}", o.out);
    assert!(o.out.contains("tracks: screen, cursor, mic"), "{}", o.out);
    assert!(o.out.contains("display 1"), "{}", o.out);
    assert!(o.out.contains("capturemann record stop"), "{}", o.out);
    // The dialog the user saw says the CLI asked and what it asked for.
    let shown = r.confirmer.shown();
    assert_eq!(shown[0].client, "capturemann-cli");
    assert_eq!(shown[0].tool, "start_recording");
    assert!(shown[0].details["microphone"]
        .as_str()
        .unwrap()
        .starts_with("On:"));

    let st = cli(&r.rig, &["record", "status"]).await;
    assert_eq!(st.code, 0, "{}", st.err);
    assert!(st.out.contains("recording"), "{}", st.out);
    assert!(st.out.contains("tracks: screen (running)"), "{}", st.out);
    let st = cli(&r.rig, &["--json", "record", "status"]).await.json();
    assert_eq!(st["state"], "recording");
    assert_eq!(st["indicator"]["started_by"]["client"], "capturemann-cli");

    let p = cli(&r.rig, &["record", "pause"]).await;
    assert_eq!(p.code, 0, "{}", p.err);
    assert!(p.out.starts_with("Paused at 00:00"), "{}", p.out);
    let again = cli(&r.rig, &["record", "pause"]).await;
    assert!(again.out.contains("already paused"), "{}", again.out);
    let p = cli(&r.rig, &["--json", "record", "resume"]).await.json();
    assert_eq!(p["state"], "recording");

    let s = cli(&r.rig, &["record", "stop"]).await;
    assert_eq!(s.code, 0, "{}", s.err);
    assert!(s.out.contains("Stopped rec_"), "{}", s.out);
    assert!(s.out.contains("duration: 00:00"), "{}", s.out);
    assert!(
        s.out.contains("saved:") && s.out.contains(".cmrec"),
        "{}",
        s.out
    );
    assert!(!r.rig.daemon.recording_active());
    // Nothing is running: status says so; stop is not_found (exit 2).
    let idle = cli(&r.rig, &["record", "status"]).await;
    assert!(idle.out.contains("No recording is running"), "{}", idle.out);
    let e = cli(&r.rig, &["record", "stop"]).await;
    assert_eq!(e.code, 2, "{}", e.err);
    assert!(e.err.contains("error[not_found]"));
}

/// Every flag reaches the daemon: target, size, fps, codec, camera and its bubble, mic,
/// system audio, countdown, max duration, title, exclusions (the dialog rows prove it).
#[tokio::test]
async fn record_start_flags_map_to_the_tool_inputs() {
    let r = recording::rec_rig(
        cm_daemon::Decision::Approve,
        cm_daemon::testing::FakeSources::new(),
    );
    // 60 fps needs a passing self-test: a pass at the whole display covers the region on it.
    let t = cli(
        &r.rig,
        &["record", "selftest", "--display", "2", "--codec", "h264"],
    )
    .await;
    assert_eq!(t.code, 0, "{}", t.err);
    let o = cli(
        &r.rig,
        &[
            "--json",
            "--request-id",
            "r-1",
            "record",
            "start",
            "--display",
            "2",
            "--region",
            "10,10,50,40",
            "--size",
            "720p",
            "--fps",
            "60",
            "--codec",
            "h264",
            "--quality",
            "balanced",
            "--camera",
            "--camera-resolution",
            "720p",
            "--camera-shape",
            "rounded",
            "--camera-size",
            "l",
            "--camera-pos",
            "top-left",
            "--mic",
            "fake-mic",
            "--no-denoise",
            "--system-audio",
            "--no-cursor",
            "--countdown",
            "1",
            "--max-duration",
            "90",
            "--title",
            "Flags",
            "--exclude-app",
            "com.example.secret",
            "--exclude-window",
            "77",
        ],
    )
    .await;
    assert_eq!(o.code, 0, "{}", o.err);
    let v = o.json();
    assert_eq!(v["title"], "Flags");
    assert_eq!(v["max_duration_s"], 90.0);
    assert_eq!(v["countdown_s"], 1);
    assert_eq!(
        v["tracks"],
        serde_json::json!(["screen", "mic", "system", "camera"])
    );
    assert_eq!(v["indicator"]["area"]["kind"], "region");
    assert_eq!(v["indicator"]["area"]["display_id"], 2);
    assert_eq!(
        v["indicator"]["area"]["bounds"],
        serde_json::json!([410, 10, 50, 40])
    );
    assert_eq!(v["indicator"]["output"]["fps"], 60);
    assert_eq!(v["indicator"]["camera"]["shape"], "rounded");
    assert_eq!(v["indicator"]["camera"]["size"], "l");
    assert_eq!(v["indicator"]["camera"]["position"], "top-left");
    let shown = r.confirmer.shown();
    let d = &shown.last().unwrap().details;
    assert!(d["output"].as_str().unwrap().contains("60 fps"));
    assert!(d["output"].as_str().unwrap().contains("H.264"));
    assert!(d["microphone"]
        .as_str()
        .unwrap()
        .contains("Fake Microphone"));
    assert!(!d["microphone"]
        .as_str()
        .unwrap()
        .contains("noise suppression"));
    assert_eq!(d["system_audio"], "On");
    assert_eq!(d["max_duration"], "1 min 30 s");
    assert!(d["camera"].as_str().unwrap().contains("720p"));

    // The same request id replays the first result (AGT-3.AC4): no second dialog.
    let o2 = cli(
        &r.rig,
        &[
            "--json",
            "--request-id",
            "r-1",
            "record",
            "start",
            "--display",
            "2",
            "--region",
            "10,10,50,40",
            "--size",
            "720p",
            "--fps",
            "60",
            "--codec",
            "h264",
            "--quality",
            "balanced",
            "--camera",
            "--camera-resolution",
            "720p",
            "--camera-shape",
            "rounded",
            "--camera-size",
            "l",
            "--camera-pos",
            "top-left",
            "--mic",
            "fake-mic",
            "--no-denoise",
            "--system-audio",
            "--no-cursor",
            "--countdown",
            "1",
            "--max-duration",
            "90",
            "--title",
            "Flags",
            "--exclude-app",
            "com.example.secret",
            "--exclude-window",
            "77",
        ],
    )
    .await;
    assert_eq!(o2.code, 0, "{}", o2.err);
    assert_eq!(o2.json(), v);
    // The self-test's dialog and the start's: the replayed start asked nobody again.
    assert_eq!(r.confirmer.shown().len(), 2);
    assert_eq!(cli(&r.rig, &["record", "stop"]).await.code, 0);

    // A window by title, and a camera by id with a position as fractions.
    let o = cli(
        &r.rig,
        &[
            "--json",
            "record",
            "start",
            "--window",
            "editor",
            "--countdown",
            "0",
            "--camera",
            "fake-cam",
            "--camera-pos",
            "0.25,0.75",
            "--mic",
            "off",
        ],
    )
    .await;
    assert_eq!(o.code, 0, "{}", o.err);
    let v = o.json();
    assert_eq!(v["indicator"]["area"]["kind"], "window");
    assert_eq!(v["indicator"]["area"]["window_id"], 10);
    assert_eq!(
        v["indicator"]["camera"]["position"],
        serde_json::json!([0.25, 0.75])
    );
    cli(&r.rig, &["record", "stop"]).await;
}

/// Bad local flags exit 2 before the daemon is asked anything.
#[tokio::test]
async fn record_start_rejects_bad_flags_locally() {
    let r = recording::rec_rig(
        cm_daemon::Decision::Approve,
        cm_daemon::testing::FakeSources::new(),
    );
    for args in [
        vec!["record", "start", "--region", "1,2,3"],
        vec![
            "record",
            "start",
            "--camera",
            "off",
            "--camera-shape",
            "circle",
        ],
        vec!["record", "start", "--no-denoise"],
        vec!["record", "start", "--mic", "off", "--no-denoise"],
        vec!["record", "start", "--fps", "24"],
        vec!["record", "start", "--display", "1", "--window", "x"],
        vec!["record", "start", "--camera-pos", "a,b"],
        // EDT-4: a separate cursor needs the telemetry.
        vec!["record", "start", "--no-cursor", "--cursor-separate"],
    ] {
        let o = cli(&r.rig, &args).await;
        assert_eq!(o.code, 2, "{args:?}: {}", o.err);
    }
    assert!(r.confirmer.shown().is_empty());
    // The daemon's own validation also exits 2.
    let o = cli(&r.rig, &["record", "start", "--camera-pos", "middle"]).await;
    assert_eq!(o.code, 2, "{}", o.err);
    assert!(o.err.contains("camera.position"), "{}", o.err);
    let o = cli(&r.rig, &["record", "start", "--window", "no such window"]).await;
    assert_eq!(o.code, 2, "{}", o.err);
}

/// AGT-2: declined -> exit 3 `user_declined`; unanswered -> exit 3 `confirmation_timeout`.
#[tokio::test]
async fn record_start_declined_and_timeout_exit_3() {
    let r = recording::rec_rig(
        cm_daemon::Decision::Decline,
        cm_daemon::testing::FakeSources::new(),
    );
    let o = cli(&r.rig, &["record", "start", "--countdown", "0"]).await;
    assert_eq!(o.code, 3, "{}", o.err);
    assert!(o.err.contains("error[user_declined]"), "{}", o.err);
    assert!(o.err.contains("Waiting for confirmation"), "{}", o.err);
    assert!(!r.rig.daemon.recording_active());
    let o = cli(&r.rig, &["--json", "record", "start", "--countdown", "0"]).await;
    assert_eq!(o.code, 3);
    let e: Value = serde_json::from_str(o.err.lines().last().unwrap()).unwrap();
    assert_eq!(e["error"]["code"], "user_declined");
    assert_eq!(e["error"]["exit_code"], 3);

    let t = recording::timeout_rig(cm_daemon::testing::FakeSources::new());
    let o = cli(&t, &["record", "start", "--countdown", "0"]).await;
    assert_eq!(o.code, 3, "{}", o.err);
    assert!(o.err.contains("error[confirmation_timeout]"), "{}", o.err);
    assert!(!t.daemon.recording_active());
}

/// REC-1: missing permission exits 4 with guidance; a second start is `busy` (exit 6).
#[tokio::test]
async fn record_start_permission_missing_and_busy() {
    let r = recording::rec_rig(
        cm_daemon::Decision::Approve,
        recording::permission(cm_core::Permission::Screen),
    );
    let o = cli(&r.rig, &["record", "start", "--countdown", "0"]).await;
    assert_eq!(o.code, 4, "{}", o.err);
    assert!(o.err.contains("error[permission_missing]"), "{}", o.err);
    assert!(o.err.contains("grant the screen permission"), "{}", o.err);
    assert!(
        r.confirmer.shown().is_empty(),
        "no dialog for a start that cannot happen"
    );

    let r = recording::rec_rig(
        cm_daemon::Decision::Approve,
        cm_daemon::testing::FakeSources::new(),
    );
    assert_eq!(
        cli(&r.rig, &["record", "start", "--countdown", "0"])
            .await
            .code,
        0
    );
    let o = cli(&r.rig, &["record", "start", "--countdown", "0"]).await;
    assert_eq!(o.code, 6, "{}", o.err);
    assert!(
        o.err.contains("A recording is already running"),
        "{}",
        o.err
    );
    assert_eq!(cli(&r.rig, &["record", "stop"]).await.code, 0);
}

/// `record stop` during a countdown cancels it; the start command exits 3 (declined).
#[tokio::test]
async fn record_stop_cancels_a_countdown() {
    let r = recording::rec_rig(
        cm_daemon::Decision::Approve,
        cm_daemon::testing::FakeSources::new(),
    );
    let (mut env, _o, e) = env_for(&r.rig.endpoint, None, "");
    let starting = tokio::spawn(async move {
        let code = run(
            ["capturemann", "record", "start", "--countdown", "10"],
            &mut env,
        )
        .await;
        (code, e.contents())
    });
    for _ in 0..500 {
        let st = cli(&r.rig, &["--json", "record", "status"]).await;
        if st.out.contains("countdown") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let o = cli(&r.rig, &["record", "stop"]).await;
    assert_eq!(o.code, 0, "{}", o.err);
    assert!(o.out.contains("countdown was cancelled"), "{}", o.out);
    let (code, err) = starting.await.unwrap();
    assert_eq!(code, 3, "{err}");
    assert!(err.contains("cancelled"), "{err}");
}

/// `record watch` streams the `recording.*` events (JSON lines with --json).
#[tokio::test]
async fn record_watch_streams_recording_events() {
    let r = recording::rec_rig(
        cm_daemon::Decision::Approve,
        cm_daemon::testing::FakeSources::new(),
    );
    let (mut env, o, e) = env_for(&r.rig.endpoint, None, "");
    let watch = tokio::spawn(async move {
        let code = run(
            [
                "capturemann",
                "--json",
                "record",
                "watch",
                "--max-events",
                "3",
            ],
            &mut env,
        )
        .await;
        (code, e.contents())
    });
    // Wait for the subscription: events published before it would be missed.
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        cli(&r.rig, &["record", "start", "--countdown", "0"])
            .await
            .code,
        0
    );
    tokio::time::sleep(Duration::from_millis(150)).await; // a few stats ticks
    assert_eq!(cli(&r.rig, &["record", "stop"]).await.code, 0);
    let (code, err) = tokio::time::timeout(Duration::from_secs(5), watch)
        .await
        .expect("watch did not finish")
        .unwrap();
    assert_eq!(code, 0, "{err}");
    let names: Vec<String> = o
        .contents()
        .lines()
        .map(|l| {
            serde_json::from_str::<Value>(l).unwrap()["event"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect();
    // `recording.starting` (when the engine announces it) may precede `recording.started`.
    assert!(
        names[0] == "recording.started"
            || (names[0] == "recording.starting"
                && names.contains(&"recording.started".to_owned())),
        "{names:?}"
    );
    assert!(
        names.iter().all(|n| n.starts_with("recording.")),
        "{names:?}"
    );
}

/// CAM-4, AUD-1, AUD-2: `devices` lists cameras, microphones and system audio.
#[tokio::test]
async fn devices_lists_cameras_mics_and_system_audio() {
    let r = recording::rec_rig(
        cm_daemon::Decision::Approve,
        cm_daemon::testing::FakeSources::new(),
    );
    let o = cli(&r.rig, &["devices"]).await;
    assert_eq!(o.code, 0, "{}", o.err);
    assert!(
        o.out.contains("Cameras") && o.out.contains("fake-cam  Fake Camera"),
        "{}",
        o.out
    );
    assert!(
        o.out.contains("Microphones") && o.out.contains("Fake Microphone"),
        "{}",
        o.out
    );
    assert!(o.out.contains("(default)"), "{}", o.out);
    assert!(
        o.out.contains("System audio\n  supported (fake)"),
        "{}",
        o.out
    );
    let j = cli(&r.rig, &["--json", "devices"]).await.json();
    assert_eq!(j["mics"][0]["id"], "fake-mic");
    assert_eq!(j["system_audio"]["supported"], true);
}

/// Without a recording engine the commands say `not_implemented` (exit 1), as the daemon does.
#[tokio::test]
async fn record_without_an_engine_is_not_implemented() {
    let r = rig();
    let o = cli(&r, &["record", "start", "--countdown", "0"]).await;
    assert_eq!(o.code, 1, "{}", o.err);
    assert!(o.err.contains("not_implemented"), "{}", o.err);
    let o = cli(&r, &["record", "status"]).await;
    assert_eq!(o.code, 1, "{}", o.err);
}

// ---- mcp subcommand ------------------------------------------------------------------------

/// `capturemann mcp` runs the `cm-mcp` binary with the flags forwarded (AGT-1).
#[cfg(unix)]
#[test]
fn mcp_subcommand_runs_cm_mcp_with_forwarded_flags() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let fake = dir.path().join("cm-mcp");
    std::fs::write(&fake, "#!/bin/sh\necho \"$@\"\n").unwrap();
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
    let run = |args: &[&str]| {
        std::process::Command::new(env!("CARGO_BIN_EXE_capturemann"))
            .env("CAPTUREMANN_MCP", &fake)
            .args(args)
            .output()
            .unwrap()
    };
    let o = run(&["mcp"]);
    assert!(o.status.success());
    assert_eq!(String::from_utf8_lossy(&o.stdout).trim(), "");
    let o = run(&["--socket", "/tmp/x.sock", "mcp", "--http", "--port", "7001"]);
    assert!(o.status.success());
    assert_eq!(
        String::from_utf8_lossy(&o.stdout).trim(),
        "--http --port 7001 --socket /tmp/x.sock"
    );
    // --port without --http is a usage error (exit 2).
    assert_eq!(run(&["mcp", "--port", "7001"]).status.code(), Some(2));
}

// ---- edit commands ---------------------------------------------------------------------------

/// A finished 60 s recording in the rig's library, indexed.
async fn recording_in(r: &Rig) -> String {
    use cm_core::{MediaTime, TrackInfo, TrackKind, TrackStats};
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
    let info = TrackInfo {
        kind: TrackKind::Screen,
        file: "screen.mp4".into(),
        start: MediaTime::ZERO,
        end: MediaTime::from_secs(60),
        details: json!({}),
        stats: TrackStats::default(),
    };
    let m = b.finalize(&[info], now).unwrap();
    r.daemon
        .execute_method(cm_core::Actor::User, 0, "rescan_library", &json!({}))
        .await
        .unwrap();
    m.id
}

/// EDT-1: `edit trim|cut|restore|split|speed|background` call the matching tools; each prints the
/// new export length, `--json` prints the result, and `undo` walks the steps back.
#[tokio::test]
async fn edit_commands_edit_a_recording_and_undo_walks_back() {
    let r = rig();
    let id = recording_in(&r).await;
    let o = cli(&r, &["edit", "trim", &id, "--start", "2", "--end", "58"]).await;
    assert_eq!(o.code, 0, "{}", o.err);
    assert!(o.out.contains("56.00 s"), "{}", o.out);
    let o = cli(&r, &["--json", "edit", "cut", &id, "10-15", "40-42"]).await;
    assert_eq!(o.code, 0, "{}", o.err);
    assert_eq!(o.json()["duration_s"], 49.0);
    assert_eq!(o.json()["action"], "cut_segments");
    let o = cli(&r, &["edit", "restore", &id, "t:1200000-15"]).await;
    assert_eq!(o.code, 0, "{}", o.err);
    let o = cli(&r, &["edit", "split", &id, "--at", "30"]).await;
    assert_eq!(o.code, 0, "{}", o.err);
    let o = cli(&r, &["edit", "speed", &id, "20-40", "--x", "2"]).await;
    assert_eq!(o.code, 0, "{}", o.err);
    let o = cli(
        &r,
        &[
            "edit",
            "background",
            &id,
            "--kind",
            "gradient",
            "--value",
            "#000,#fff",
            "--padding",
            "8",
            "--radius",
            "12",
            "--shadow",
            "true",
        ],
    )
    .await;
    assert_eq!(o.code, 0, "{}", o.err);
    let o = cli(&r, &["--json", "undo", &id]).await;
    assert_eq!(o.code, 0, "{}", o.err);
    assert_eq!(o.json()["changed"], true);
    assert_eq!(o.json()["edits"]["background"]["kind"], "none");
}

/// AUD-4: `edit audio --system-gain -6` (a negative number) sets the mix; no option is a usage
/// error and an out-of-range gain exits 2.
#[tokio::test]
async fn edit_audio_sets_the_mix() {
    let r = rig();
    let id = recording_in(&r).await;
    let o = cli(
        &r,
        &[
            "--json",
            "edit",
            "audio",
            &id,
            "--system-gain",
            "-6",
            "--mic-mute",
            "true",
        ],
    )
    .await;
    assert_eq!(o.code, 0, "{}", o.err);
    assert_eq!(o.json()["action"], "set_audio_mix");
    assert_eq!(o.json()["edits"]["audio"]["system_gain_db"], -6.0);
    assert_eq!(o.json()["edits"]["audio"]["mic_mute"], true);
    for args in [
        vec!["edit", "audio", &id],
        vec!["edit", "audio", &id, "--mic-gain", "30"],
    ] {
        let o = cli(&r, &args).await;
        assert_eq!(o.code, 2, "{args:?}: {}{}", o.out, o.err);
    }
}

/// Bad times, ranges and daemon refusals exit 2 with the field named.
#[tokio::test]
async fn edit_commands_report_bad_input() {
    let r = rig();
    let id = recording_in(&r).await;
    for args in [
        vec!["edit", "cut", &id, "ten-fifteen"],
        vec!["edit", "cut", &id, "15"],
        vec!["edit", "trim", &id, "--start", "soon", "--end", "5"],
        vec!["edit", "trim", &id, "--start", "5", "--end", "2"],
        vec!["edit", "speed", &id, "1-2", "--x", "20"],
        vec!["edit", "cut", &id, "0.1-60"],
    ] {
        let o = cli(&r, &args).await;
        assert_eq!(o.code, 2, "{args:?}: {}{}", o.out, o.err);
    }
}

/// CAM-3: `edit camera` (alias `layout`) sets a range; camera layouts need a camera track, a bad
/// `--rect` is a usage error (exit 2).
#[tokio::test]
async fn edit_camera_sets_a_layout_range() {
    let r = rig();
    let id = recording_in(&r).await;
    let o = cli(
        &r,
        &[
            "--json", "edit", "camera", &id, "0-15", "--layout", "screen", "--smooth",
        ],
    )
    .await;
    assert_eq!(o.code, 0, "{}", o.err);
    assert_eq!(o.json()["action"], "set_camera_layout");
    let seg = &o.json()["edits"]["layout"]["segments"][0];
    assert_eq!(seg["mode"], "screen");
    assert_eq!(seg["transition"], "smooth");
    // The fixture recording has no camera track.
    let o = cli(&r, &["edit", "layout", &id, "0-15", "--layout", "camera"]).await;
    assert_eq!(o.code, 2, "{}", o.err);
    assert!(o.err.contains("no camera track"), "{}", o.err);
    for args in [
        vec![
            "edit", "camera", &id, "0-5", "--layout", "bubble", "--rect", "1,2",
        ],
        vec![
            "edit", "camera", &id, "0-5", "--layout", "bubble", "--ring", "thick",
        ],
        vec!["edit", "camera", &id, "0-5", "--layout", "wide"],
    ] {
        let o = cli(&r, &args).await;
        assert_eq!(o.code, 2, "{args:?}: {}{}", o.out, o.err);
    }
}

// ---- EDT-4 and EDT-5 commands -----------------------------------------------------------------

/// `edit zoom|cursor|reframe` call `set_zooms`, `set_cursor_style` and `set_reframe`; bad input
/// exits 2.
#[tokio::test]
async fn edit_zoom_cursor_and_reframe_commands() {
    let r = rig();
    let id = recording_in(&r).await;
    let o = cli(
        &r,
        &[
            "--json",
            "edit",
            "zoom",
            &id,
            "--at",
            "15",
            "--length",
            "3",
            "--focus",
            "0.5,0.4",
            "--scale",
            "2",
            "--follow-cursor",
        ],
    )
    .await;
    assert_eq!(o.code, 0, "{}", o.err);
    assert_eq!(o.json()["action"], "set_zooms");
    let z = &o.json()["edits"]["zooms"][0];
    assert_eq!(z["at"], 15 * 120_000);
    assert_eq!(z["scale"], 2.0);
    assert_eq!(z["follow_cursor"], true);
    assert_eq!(z["focus"], serde_json::json!([0.5, 0.4]));

    // A second one shortens the first; --replace swaps them; --clear removes every zoom.
    let o = cli(
        &r,
        &["--json", "edit", "zoom", &id, "--at", "17", "--length", "4"],
    )
    .await;
    assert_eq!(o.code, 0, "{}", o.err);
    assert_eq!(o.json()["edits"]["zooms"].as_array().unwrap().len(), 2);
    let o = cli(
        &r,
        &[
            "--json",
            "edit",
            "zoom",
            &id,
            "--at",
            "30",
            "--length",
            "2",
            "--replace",
        ],
    )
    .await;
    assert_eq!(o.json()["edits"]["zooms"].as_array().unwrap().len(), 1);
    let o = cli(&r, &["--json", "edit", "zoom", &id, "--clear"]).await;
    assert_eq!(o.code, 0, "{}", o.err);
    assert!(o.json()["edits"].get("zooms").is_none());

    let o = cli(
        &r,
        &[
            "--json",
            "edit",
            "cursor",
            &id,
            "--size",
            "2",
            "--smoothing",
            "strong",
            "--click-effect",
            "ring",
            "--hide-idle",
            "true",
        ],
    )
    .await;
    assert_eq!(o.code, 0, "{}", o.err);
    assert_eq!(o.json()["action"], "set_cursor_style");
    assert_eq!(o.json()["edits"]["cursor"]["smoothing"], "strong");
    assert_eq!(o.json()["edits"]["cursor"]["hide_idle"], true);

    let o = cli(
        &r,
        &[
            "--json", "edit", "reframe", &id, "--aspect", "9:16", "--mode", "fixed", "--focus",
            "0.3,0.5",
        ],
    )
    .await;
    assert_eq!(o.code, 0, "{}", o.err);
    assert_eq!(o.json()["action"], "set_reframe");
    assert_eq!(
        o.json()["edits"]["reframe"]["9:16"]["windows"][0]["focus"][0],
        0.3
    );

    for args in [
        vec!["edit", "zoom", &id],
        vec!["edit", "zoom", &id, "--at", "1", "--length", "0.2"],
        vec![
            "edit",
            "zoom",
            &id,
            "--at",
            "1",
            "--length",
            "3",
            "--focus",
            "half,half",
        ],
        vec![
            "edit", "zoom", &id, "--at", "1", "--length", "3", "--scale", "9",
        ],
        vec!["edit", "cursor", &id],
        vec!["edit", "cursor", &id, "--size", "9"],
        vec!["edit", "cursor", &id, "--smoothing", "wobbly"],
        vec!["edit", "reframe", &id, "--aspect", "9:16"],
        vec!["edit", "reframe", &id, "--aspect", "4:3", "--mode", "fit"],
        vec!["edit", "reframe", &id, "--aspect", "9:16", "--focus", "2,2"],
    ] {
        let o = cli(&r, &args).await;
        assert_eq!(o.code, 2, "{args:?}: {}{}", o.out, o.err);
    }
}

/// `destinations` and `upload` (STO-1, STO-3, STO-5): same behavior as the actions. Tested with
/// a local folder.
#[tokio::test]
async fn sto_1_destinations_and_upload_commands() {
    use cm_storage::{Destination, DestinationStore, Kind, What};
    let dir = tempfile::tempdir().unwrap();
    let endpoint = Endpoint::Path(dir.path().join("d.sock"));
    let nas = dir.path().join("nas");
    std::fs::create_dir(&nas).unwrap();
    let store = DestinationStore::new(dir.path().join("config"));
    store
        .save(&[Destination {
            id: "nas".into(),
            name: "NAS".into(),
            kind: Kind::Local,
            path: nas.clone(),
            account: None,
            folder_pattern: "{year}".into(),
            what: What::Original,
            auto_upload: cm_storage::AutoUpload::Off,
            default: true,
        }])
        .unwrap();
    let lib = Library::open_or_create(&dir.path().join("lib")).unwrap();
    let queue = cm_daemon::library_job_queue(lib.root()).unwrap();
    let daemon = Daemon::builder(Capturer::new(desktop()).with_own_pid(None), lib)
        .jobs(queue)
        .destinations(store)
        .build();
    daemon.register_upload_handler().unwrap();
    daemon.start_jobs().unwrap();
    serve(&daemon, &endpoint);
    let r = Rig {
        dir,
        endpoint,
        daemon,
    };

    let o = cli(&r, &["destinations", "--json"]).await;
    assert_eq!(o.code, 0, "{}", o.err);
    assert_eq!(o.json()["destinations"][0]["id"], "nas");
    let human = cli(&r, &["destinations"]).await;
    assert!(
        human.out.contains("nas") && human.out.contains("(default)"),
        "{}",
        human.out
    );

    let file = r.dir.path().join("clip.mp4");
    std::fs::write(&file, b"clip bytes").unwrap();
    let f = file.to_str().unwrap();
    let up = cli(&r, &["upload", "--file", f, "--to", "nas", "--json"]).await;
    assert_eq!(up.code, 0, "{}", up.err);
    let job = up.json()["job_id"].as_str().unwrap().to_owned();
    let mut state = String::new();
    for _ in 0..200 {
        state = cli(&r, &["jobs", "get", &job, "--json"]).await.json()["state"]
            .as_str()
            .unwrap()
            .to_owned();
        if state == "done" || state == "failed" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(state, "done");

    let unknown = cli(&r, &["upload", "--file", f, "--to", "nope"]).await;
    assert_ne!(unknown.code, 0);
    let neither = cli(&r, &["upload"]).await;
    assert_ne!(neither.code, 0, "clap rejects a missing target");
}

/// `publish accounts` and `publish` (PUB-1, PUB-2) through the CLI: the accounts list shows the
/// channel and the day's count; a publish of a capture that does not exist fails with
/// `not_found` (nothing is asked, nothing is sent). Tested with fakes.
#[tokio::test]
async fn pub_publish_accounts_and_publish_commands() {
    use cm_publish::{Privacy, PublishAccount, PublishAccountStore};
    let dir = tempfile::tempdir().unwrap();
    let endpoint = Endpoint::Path(dir.path().join("d.sock"));
    let store = PublishAccountStore::new(dir.path().join("config"));
    store
        .save(&[PublishAccount {
            id: "youtube-abc".into(),
            platform: "youtube".into(),
            name: "Teacher Channel".into(),
            account_id: "UC1".into(),
            default_privacy: Privacy::Unlisted,
        }])
        .unwrap();
    let lib = Library::open_or_create(&dir.path().join("lib")).unwrap();
    let queue = cm_daemon::library_job_queue(lib.root()).unwrap();
    let daemon = Daemon::builder(Capturer::new(desktop()).with_own_pid(None), lib)
        .jobs(queue)
        .publish_accounts(store)
        .build();
    daemon.register_publish_handler().unwrap();
    daemon.start_jobs().unwrap();
    serve(&daemon, &endpoint);
    let r = Rig {
        dir,
        endpoint,
        daemon,
    };

    let o = cli(&r, &["publish", "accounts", "--json"]).await;
    assert_eq!(o.code, 0, "{}", o.err);
    let a = &o.json()["accounts"][0];
    assert_eq!(a["id"], "youtube-abc");
    assert_eq!(a["name"], "Teacher Channel");
    assert_eq!(a["uploads_today"], 0);
    let human = cli(&r, &["publish", "accounts"]).await;
    assert!(
        human.out.contains("Teacher Channel") && human.out.contains("0 of 100"),
        "{}",
        human.out
    );

    let none = cli(&r, &["publish"]).await;
    assert_ne!(none.code, 0, "a capture id or `accounts` is needed");
    let missing = cli(
        &r,
        &[
            "publish",
            "rec_nope",
            "--platform",
            "youtube",
            "--title",
            "T",
            "--made-for-kids",
            "no",
            "--privacy",
            "private",
        ],
    )
    .await;
    assert_ne!(missing.code, 0);
    assert!(missing.err.contains("not_found"), "{}", missing.err);
}

/// AUD-6: `record mic --mute|--unmute` is `set_mic_muted`; it is idempotent, the status shows it
/// in both forms, `--json` returns the tool result, and the flags are validated by the parser.
#[tokio::test]
async fn record_mic_mutes_and_unmutes() {
    let r = recording::rec_rig(
        cm_daemon::Decision::Approve,
        cm_daemon::testing::FakeSources::new(),
    );
    // Nothing running: not_found (exit 2), like pause.
    let none = cli(&r.rig, &["record", "mic", "--mute"]).await;
    assert_eq!(none.code, 2, "{}", none.err);
    assert!(none.err.contains("error[not_found]"), "{}", none.err);

    let o = cli(&r.rig, &["record", "start", "--countdown", "0", "--mic"]).await;
    assert_eq!(o.code, 0, "{}", o.err);
    let shown = r.confirmer.shown().len();

    let m = cli(&r.rig, &["record", "mic", "--mute"]).await;
    assert_eq!(m.code, 0, "{}", m.err);
    assert!(m.out.starts_with("Microphone muted at 00:00"), "{}", m.out);
    let again = cli(&r.rig, &["record", "mic", "--mute"]).await;
    assert!(again.out.contains("already muted"), "{}", again.out);
    let st = cli(&r.rig, &["record", "status"]).await;
    assert!(st.out.contains("microphone: muted"), "{}", st.out);
    let st = cli(&r.rig, &["--json", "record", "status"]).await.json();
    assert_eq!(st["mic_muted"], true);
    let j = cli(&r.rig, &["--json", "record", "mic", "--unmute"])
        .await
        .json();
    assert_eq!(j["muted"], false);
    assert_eq!(j["changed"], true);
    assert!(j["recording_id"].as_str().unwrap().starts_with("rec_"));
    assert_eq!(r.confirmer.shown().len(), shown, "no dialog for a mute");

    // The parser wants exactly one of the two flags.
    for args in [
        vec!["record", "mic"],
        vec!["record", "mic", "--mute", "--unmute"],
    ] {
        let bad = cli(&r.rig, &args).await;
        assert_eq!(bad.code, 2, "{args:?}: {}{}", bad.out, bad.err);
    }
    cli(&r.rig, &["record", "stop"]).await;
}

/// VOC-1: `record start --audio` is `start_recording { mode: "audio" }`: no target lookup, the
/// dialog says "Record audio", the screen options are refused by the parser, and the library
/// lists the result under `library list --kind audio`.
#[tokio::test]
async fn record_start_audio_records_an_audio_bundle() {
    let r = recording::rec_rig(
        cm_daemon::Decision::Approve,
        cm_daemon::testing::FakeSources::new(),
    );
    let o = cli(
        &r.rig,
        &[
            "record",
            "start",
            "--audio",
            "--countdown",
            "0",
            "--preset",
            "online",
            "--title",
            "Standup",
        ],
    )
    .await;
    assert_eq!(o.code, 0, "{}", o.err);
    assert!(
        o.out.contains("audio only: microphone and system audio"),
        "{}",
        o.out
    );
    assert!(o.out.contains("tracks: mic, system"), "{}", o.out);
    let shown = r.confirmer.shown();
    assert_eq!(shown.len(), 1);
    assert!(
        shown[0].summary.starts_with("Record audio"),
        "{}",
        shown[0].summary
    );

    let m = cli(&r.rig, &["record", "marker", "--label", "Action item"]).await;
    assert_eq!(m.code, 0, "{}{}", m.out, m.err);
    assert!(
        m.out.starts_with("Marker \"Action item\" added at 00:00"),
        "{}",
        m.out
    );
    let j = cli(&r.rig, &["--json", "record", "marker"]).await.json();
    assert!(j["marker"]["by"].as_str().unwrap().starts_with("agent:"));
    assert_eq!(j["count"], 2);
    let stopped = cli(&r.rig, &["--json", "record", "stop"]).await.json();
    let id = stopped["capture_id"].as_str().unwrap().to_owned();

    let list = cli(&r.rig, &["--json", "library", "list", "--kind", "audio"])
        .await
        .json();
    assert_eq!(list["captures"][0]["id"], id.as_str());
    let none = cli(
        &r.rig,
        &["--json", "library", "list", "--kind", "recording"],
    )
    .await
    .json();
    assert!(none["captures"].as_array().unwrap().is_empty());

    // A marker on the finished recording, and the parser's rules for the two forms.
    let late = cli(
        &r.rig,
        &[
            "record",
            "marker",
            "--capture",
            &id,
            "--at",
            "0",
            "--label",
            "Start",
        ],
    )
    .await;
    assert_eq!(late.code, 0, "{}{}", late.out, late.err);
    assert!(late.out.contains(&format!("to {id}")), "{}", late.out);
    for args in [
        vec!["record", "marker", "--capture", id.as_str()],
        vec!["record", "marker", "--at", "3"],
        vec![
            "record",
            "marker",
            "--id",
            "rec_x",
            "--capture",
            id.as_str(),
            "--at",
            "1",
        ],
    ] {
        let bad = cli(&r.rig, &args).await;
        assert_eq!(bad.code, 2, "{args:?}: {}{}", bad.out, bad.err);
    }

    // Screen-only options conflict with --audio at the parser (exit 2), before anything runs.
    let before = r.confirmer.shown().len();
    for extra in [
        vec!["--display", "1"],
        vec!["--camera"],
        vec!["--fps", "60"],
        vec!["--no-cursor"],
        vec!["--size", "720p"],
    ] {
        let mut args = vec!["record", "start", "--audio"];
        args.extend(extra.iter().copied());
        let bad = cli(&r.rig, &args).await;
        assert_eq!(bad.code, 2, "{args:?}: {}{}", bad.out, bad.err);
    }
    let bad = cli(&r.rig, &["record", "start", "--preset", "room"]).await;
    assert_eq!(bad.code, 2, "--preset needs --audio");
    assert_eq!(r.confirmer.shown().len(), before);
}

/// VOC-1: `record start --audio --people N` sends `people` (1 to 8); it needs `--audio`.
#[tokio::test]
async fn record_start_people_is_sent_and_checked() {
    let r = recording::rec_rig(
        cm_daemon::Decision::Approve,
        cm_daemon::testing::FakeSources::new(),
    );
    for args in [
        vec!["record", "start", "--people", "3"],
        vec!["record", "start", "--audio", "--people", "0"],
        vec!["record", "start", "--audio", "--people", "9"],
    ] {
        let bad = cli(&r.rig, &args).await;
        assert_eq!(bad.code, 2, "{args:?}: {}{}", bad.out, bad.err);
    }
    let o = cli(
        &r.rig,
        &[
            "record",
            "start",
            "--audio",
            "--countdown",
            "0",
            "--preset",
            "room",
            "--people",
            "3",
        ],
    )
    .await;
    assert_eq!(o.code, 0, "{}", o.err);
    let shown = r.confirmer.shown();
    assert_eq!(shown.last().unwrap().params["people"], 3);
    cli(&r.rig, &["record", "stop"]).await;
}
