//! The 60 fps self-test on the REAL screen and encoder (REC-3). Ignored: it records this
//! machine's screen for ~10 s (the test video is deleted at once) and needs Screen Recording
//! permission. Run it by hand:
//!
//! ```text
//! cargo test -p cm-daemon --test real_selftest -- --ignored --nocapture
//! ```
//!
//! It prints the verdict (late %, dropped, frames). A fail is a valid outcome on a slow machine;
//! the test only checks that the run completes, reports numbers, stores the result and leaves no
//! file behind.

#![cfg(target_os = "macos")]
#![allow(clippy::unwrap_used)]

use cm_capture::Capturer;
use cm_core::Actor;
use cm_daemon::Daemon;
use cm_library::Library;
use serde_json::json;

#[tokio::test(flavor = "multi_thread")]
#[ignore = "records the real screen for 10 s; run by hand"]
async fn real_fps_selftest_runs_reports_and_cleans_up() {
    let dir = tempfile::tempdir().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let lib = Library::open_or_create(dir.path()).unwrap();
    let daemon = Daemon::builder(
        Capturer::system().with_own_pid(Some(std::process::id())),
        lib,
    )
    .recording_system()
    .recording_options(cm_daemon::EngineOptions {
        selftest_dir: Some(tmp.path().to_owned()),
        ..cm_daemon::EngineOptions::default()
    })
    .build();
    let v = daemon
        .execute_method(Actor::User, 0, "run_fps_selftest", &json!({}))
        .await
        .unwrap();
    println!("{}", serde_json::to_string_pretty(&v).unwrap());
    assert!(
        matches!(v["state"].as_str(), Some("passed" | "failed")),
        "{v}"
    );
    assert!(v["frames"].as_u64().unwrap() > 0, "{v}");
    assert!(v["late_pct"].as_f64().is_some(), "{v}");
    assert_eq!(v["stored"], true);
    assert_eq!(
        std::fs::read_dir(tmp.path()).unwrap().count(),
        0,
        "temp folder is empty"
    );
    let leftovers: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().ends_with(".cmrec"))
        .collect();
    assert!(leftovers.is_empty(), "no bundle in the library");
}
