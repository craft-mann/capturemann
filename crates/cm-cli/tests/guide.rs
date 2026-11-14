//! `capturemann guide ...` (GDE-4) against an in-process daemon with a scripted click source.

#![allow(clippy::unwrap_used)]

use std::io::Cursor;
use std::sync::Arc;

use cm_capture::fake::FakeBackend;
use cm_capture::Capturer;
use cm_cli::{run, Env, SharedBuf};
use cm_daemon::guide::ScriptedClicks;
use cm_daemon::testing::FixedConfirmer;
use cm_daemon::{Daemon, Decision, Endpoint, IpcServer};
use cm_library::Library;
use serde_json::Value;

struct Out {
    code: i32,
    out: String,
    err: String,
}

async fn cli(endpoint: &Endpoint, args: &[&str]) -> Out {
    let (o, e) = (SharedBuf::default(), SharedBuf::default());
    let mut env = Env {
        endpoint: endpoint.clone(),
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

#[tokio::test]
async fn gde_4_the_cli_runs_the_whole_flow() {
    let dir = tempfile::tempdir().unwrap();
    let endpoint = Endpoint::scratch(dir.path(), "d");
    let backend = FakeBackend::new().with_display(1, 0, 0, 400, 300, 2.0);
    let lib = Library::open_or_create(&dir.path().join("lib")).unwrap();
    let clicks = ScriptedClicks::new();
    let daemon = Daemon::builder(Capturer::new(backend).with_own_pid(None), lib)
        .guide_clicks(clicks.clone())
        .confirmer(Arc::new(FixedConfirmer::new(Decision::Approve)))
        .build();
    let server = IpcServer::bind(&endpoint).unwrap();
    let d = daemon.clone();
    tokio::spawn(async move {
        let _ = server.run(d).await;
    });

    let r = cli(
        &endpoint,
        &[
            "--json",
            "guide",
            "start",
            "--countdown",
            "0",
            "--title",
            "Demo",
        ],
    )
    .await;
    assert_eq!(r.code, 0, "{}", r.err);
    let id = serde_json::from_str::<Value>(&r.out).unwrap()["guide_id"]
        .as_str()
        .unwrap()
        .to_owned();
    for x in [30.0, 90.0, 150.0] {
        assert!(clicks.click(x, 40.0));
    }
    let r = cli(&endpoint, &["guide", "stop"]).await;
    assert_eq!(r.code, 0, "{}", r.err);
    assert!(
        r.out.contains("Demo") && r.out.contains("3 steps"),
        "{}",
        r.out
    );
    assert!(r.out.contains("Click here"));

    let g: Value =
        serde_json::from_str(&cli(&endpoint, &["--json", "guide", "get", &id]).await.out).unwrap();
    let ids: Vec<String> = g["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap().to_owned())
        .collect();
    let steps = format!(
        r#"[{{"id":"{}","title":"Open it"}},{{"id":"{}"}},{{"id":"{}","delete":true}}]"#,
        ids[2], ids[0], ids[1]
    );
    let r = cli(
        &endpoint,
        &["guide", "set", &id, "--steps", &steps, "--intro", "Hello"],
    )
    .await;
    assert_eq!(r.code, 0, "{}", r.err);
    assert!(
        r.out.contains("Open it") && r.out.contains("2 steps"),
        "{}",
        r.out
    );
    // One call, one undo step per setter; `undo` takes the guide id.
    let r = cli(&endpoint, &["undo", &id]).await;
    assert_eq!(r.code, 0, "{}", r.err);

    let dest = dir.path().join("out");
    let r = cli(
        &endpoint,
        &[
            "guide",
            "export",
            &id,
            "--format",
            "markdown",
            "--dest",
            dest.to_str().unwrap(),
        ],
    )
    .await;
    assert_eq!(r.code, 0, "{}", r.err);
    assert!(dest.join("guide.md").is_file());
    let r = cli(
        &endpoint,
        &[
            "guide",
            "export",
            &id,
            "--format",
            "pdf",
            "--dest",
            dest.to_str().unwrap(),
        ],
    )
    .await;
    assert_ne!(r.code, 0);
    assert!(r.err.contains("not_implemented"), "{}", r.err);
}
