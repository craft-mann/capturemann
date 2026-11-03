//! M2f at the request level: the call the editor makes by itself when it opens (`suggest_cuts`
//! through `AiClient` and the daemon transport) against the real daemon, with a scripted
//! transcriber in place of whisper.cpp. Needs FFmpeg for the fixture (prints SKIPPED without it).

#![allow(clippy::unwrap_used)]

use std::sync::Arc;
use std::time::Duration;

use cm_ai::transcribe::ScriptedTranscriber;
use cm_capture::fake::FakeBackend;
use cm_capture::Capturer;
use cm_core::Actor;
use cm_daemon::ai_engine::services_with;
use cm_daemon::fixture::{make_fixture, FixtureSpec};
use serde_json::json;

use super::client::{is_cancellable, AiClient, DaemonTransport};

struct Rig {
    daemon: cm_daemon::Daemon,
    id: String,
    models: tempfile::TempDir,
    _lib: tempfile::TempDir,
}

async fn rig() -> Option<Rig> {
    let Ok(tools) = cm_media::tools::locate() else {
        eprintln!("SKIPPED: FFmpeg was not found");
        return None;
    };
    let models = tempfile::tempdir().unwrap();
    // "um" stands alone (more than 150 ms of quiet on each side) and falls on a beep of the
    // fixture, outside every pause (a filler inside a pause is already part of that cut).
    let mut engine = ScriptedTranscriber::from_secs(&[
        ("Hello", 1.0, 1.4),
        ("um", 4.3, 4.6),
        ("welcome", 6.0, 6.5),
    ]);
    engine.model = "small".into();
    let lib = tempfile::tempdir().unwrap();
    let library = cm_library::Library::open_or_create(lib.path()).unwrap();
    let queue = cm_daemon::library_job_queue(lib.path()).unwrap();
    let daemon = cm_daemon::Daemon::builder(
        Capturer::new(FakeBackend::new().with_display(1, 0, 0, 40, 30, 1.0)).with_own_pid(None),
        library,
    )
    .jobs(queue)
    .ai(services_with(
        models.path().to_path_buf(),
        Some(Arc::new(engine)),
    ))
    .build();
    daemon.register_ai_handlers().unwrap();
    daemon.start_jobs().unwrap();
    let spec = FixtureSpec {
        title: "Auto cuts".into(),
        secs: 12,
        pause: None,
        ..FixtureSpec::default()
    };
    let fx = make_fixture(&tools, lib.path(), &spec).unwrap();
    daemon
        .execute_method(Actor::User, 0, "rescan_library", &json!({}))
        .await
        .unwrap();
    Some(Rig {
        daemon,
        id: fx.id,
        models,
        _lib: lib,
    })
}

fn client(r: &Rig) -> AiClient {
    AiClient::new(Arc::new(DaemonTransport {
        daemon: r.daemon.clone(),
        rt: tokio::runtime::Handle::current(),
    }))
}

async fn call(r: &Rig, method: &str, p: serde_json::Value) -> serde_json::Value {
    r.daemon
        .execute_method(Actor::User, 0, method, &p)
        .await
        .unwrap()
}

/// The revision count of the recording's manifest (an applied cut would add one).
fn revisions_of(r: &Rig) -> usize {
    let dir = std::fs::read_dir(r._lib.path())
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.extension().is_some_and(|e| e == "cmrec"))
        .unwrap();
    cm_library::read_manifest(&dir).unwrap().revisions.len()
}

/// Without a transcript the automatic run finds pauses only and changes nothing; once a transcript
/// exists the same call also finds the filler word. Nothing is applied either way, so Accept is
/// still the editor's one `cut_segments`.
#[tokio::test]
async fn suggest_cuts_by_itself_finds_pauses_then_fillers_and_applies_nothing() {
    let Some(r) = rig().await else { return };
    let ai = client(&r);
    let before = revisions_of(&r);

    let first = ai.suggest_cuts(&r.id).await.unwrap();
    assert!(
        first.iter().all(|c| c.kind != "filler"),
        "no transcript, no fillers: {first:?}"
    );

    // A model and a transcript (scripted).
    let model = r.models.path().join("ggml-small.bin");
    let mut bytes = vec![0u8; 21_000_000];
    bytes[..4].copy_from_slice(&[0x6c, 0x6d, 0x67, 0x67]);
    std::fs::write(&model, bytes).unwrap();
    call(
        &r,
        "import_model",
        json!({"model": "small", "path": model.to_str().unwrap()}),
    )
    .await;
    let started = call(&r, "transcribe", json!({"capture_id": r.id})).await;
    let job_id = started["job_id"].as_str().unwrap().to_owned();
    for _ in 0..1500 {
        let j = call(&r, "get_job", json!({"job_id": job_id})).await;
        if j["state"] == "done" {
            break;
        }
        assert_ne!(j["state"], "failed", "{j}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let second = ai.suggest_cuts(&r.id).await.unwrap();
    assert!(
        second.iter().any(|c| c.kind == "filler"),
        "the transcript adds the filler: {second:?}"
    );
    assert_eq!(
        before,
        revisions_of(&r),
        "suggesting adds no revision and applies nothing"
    );
}

/// Only the read-only suggestion calls are abandoned when the editor goes away.
#[test]
fn only_the_suggestions_are_cancelled_when_the_caller_goes_away() {
    assert!(is_cancellable("suggest_cuts"));
    assert!(is_cancellable("suggest_zooms"));
    for m in [
        "cut_segments",
        "transcribe",
        "set_captions",
        "diarize",
        "undo",
    ] {
        assert!(!is_cancellable(m), "{m} may change something");
    }
}

/// Dropping the future of a cancellable call aborts its task on the runtime.
#[tokio::test]
async fn dropping_the_guard_aborts_the_task() {
    let flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let f = flag.clone();
    let task = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(300)).await;
        f.store(true, std::sync::atomic::Ordering::SeqCst);
    });
    drop(super::client::abort_on_drop(task.abort_handle()));
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(
        !flag.load(std::sync::atomic::Ordering::SeqCst),
        "it was aborted"
    );
    assert!(task.await.unwrap_err().is_cancelled());
}
