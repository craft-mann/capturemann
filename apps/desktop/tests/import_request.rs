//! IMP-5 at the request level: the sheet's rows come from the real daemon's `dry_run`, its request
//! is accepted by the real daemon, and the progress cards follow the job's result. The windows
//! are not drawn here (no display). Fixtures come from the `ffmpeg` on `PATH` (SKIPPED without).
#![allow(clippy::unwrap_used)]

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

use cm_capture::fake::FakeBackend;
use cm_capture::Capturer;
use cm_core::Actor;
use cm_desktop::import_flow::{CardState, ImportRun, Sheet, Status};
use cm_library::Library;
use serde_json::json;

fn ff(args: &[&str]) -> bool {
    Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y"])
        .args(args)
        .stdin(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

#[tokio::test]
async fn the_sheet_checks_imports_and_follows_the_job() {
    if Command::new("ffprobe").arg("-version").output().is_err()
        || Command::new("ffmpeg").arg("-version").output().is_err()
    {
        eprintln!("SKIPPED: ffmpeg and ffprobe not found (brew install ffmpeg)");
        return;
    }
    let lib_dir = tempfile::tempdir().unwrap();
    let src = tempfile::tempdir().unwrap();
    let lib = Library::open_or_create(lib_dir.path()).unwrap();
    let daemon =
        cm_daemon::Daemon::builder(Capturer::new(FakeBackend::new()).with_own_pid(None), lib)
            .jobs(cm_daemon::library_job_queue(lib_dir.path()).unwrap())
            .build();
    daemon.register_import_handler().unwrap();
    daemon.start_jobs().unwrap();
    let wav = src.path().join("Interview.wav");
    let flac = src.path().join("old.flac");
    assert!(ff(&[
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=440:duration=2",
        wav.to_str().unwrap()
    ]));
    assert!(ff(&[
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=440:duration=1",
        flac.to_str().unwrap()
    ]));
    let dir = src.path().to_path_buf();
    // The check, as the app sends it (a folder is not sent: the drop removes it first).
    let paths: Vec<PathBuf> = vec![wav.clone(), flac.clone()];
    let rows = daemon
        .execute_method(
            Actor::User,
            0,
            "import_media",
            &json!({"paths": paths.iter().map(|p| p.to_string_lossy()).collect::<Vec<_>>(), "dry_run": true}),
        )
        .await
        .unwrap();
    let mut sheet = Sheet::default();
    sheet.add_rows(rows["files"].as_array().unwrap());
    assert_eq!(sheet.rows.len(), 2);
    assert_eq!(sheet.rows[0].status, Status::Ready);
    assert!(matches!(&sheet.rows[1].status, Status::Cant(r, _) if r == "unsupported_format"));
    assert_eq!(sheet.importable(), 1);
    assert_eq!(sheet.button_label(), "Import 1 file");
    sheet.set_title(0, "My interview");
    sheet.transcribe = true; // no model in this daemon: skipped, never an error
                             // The start, as the user.
    let started = daemon
        .execute_method(Actor::User, 0, "import_media", &sheet.request())
        .await
        .unwrap();
    let sizes: Vec<(PathBuf, u64)> = sheet
        .rows
        .iter()
        .map(|r| (r.path.clone(), r.size_bytes))
        .collect();
    let mut run = ImportRun::from_started(&started, &sizes).unwrap();
    assert_eq!(run.cards.len(), 1, "only the importable file was sent");
    let job_id = started["job_id"].as_str().unwrap();
    let mut result = None;
    for _ in 0..500 {
        let j = daemon
            .execute_method(Actor::User, 0, "get_job", &json!({"job_id": job_id}))
            .await
            .unwrap();
        if j["state"] == "done" {
            result = Some(j["result"].clone());
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    run.finish(&result.expect("the job finished"));
    assert_eq!(run.cards[0].state, CardState::Done);
    // The library has it, and it says where it came from (the Imported chip reads `source`).
    let list = daemon
        .execute_method(Actor::User, 0, "list_library", &json!({}))
        .await
        .unwrap();
    let c = &list["captures"][0];
    assert_eq!(c["title"], "My interview");
    assert_eq!(c["source"]["original_name"], "Interview.wav");
    let item = cm_desktop::library::CaptureItem::from_json(c).unwrap();
    assert!(item.imported_from.is_some());
    // A folder in the call is refused by the daemon, the app never sends one.
    let e = daemon
        .execute_method(
            Actor::User,
            0,
            "import_media",
            &json!({"paths": [dir.to_string_lossy()]}),
        )
        .await
        .unwrap_err();
    assert!(
        e.to_string().contains("Folders can't be imported yet"),
        "{e}"
    );
}
