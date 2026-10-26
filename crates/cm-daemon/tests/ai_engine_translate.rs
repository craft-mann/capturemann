//! AI-5 with the REAL adapters over `cm-ai` (model store, captions, the audio decode and the time
//! mapping of `translate_recording`) and a scripted transcriber in place of whisper.cpp, on a
//! synthetic fixture recording. No Whisper model, no network, no hardware; needs FFmpeg for the
//! fixture (prints SKIPPED without it).

#![allow(clippy::unwrap_used)]

mod common;

use std::sync::Arc;
use std::time::Duration;

use cm_ai::transcribe::ScriptedTranscriber;
use cm_core::{Actor, ErrorCode};
use cm_daemon::ai_engine::services_with;
use cm_daemon::fixture::{make_fixture, FixtureSpec};
use cm_daemon::DaemonError;
use common::{builder, desktop};
use serde_json::{json, Value};

fn code(e: &DaemonError) -> ErrorCode {
    match e {
        DaemonError::Cm(c) => c.code(),
        DaemonError::WithData { error, .. } => error.code(),
        other => panic!("unexpected {other:?}"),
    }
}

fn reason(e: &DaemonError) -> String {
    match e {
        DaemonError::WithData { data, .. } => data["reason"].as_str().unwrap_or("").to_owned(),
        _ => String::new(),
    }
}

/// A file with the ggml magic and a plausible size, which the store accepts as a model.
fn fake_model(dir: &std::path::Path, name: &str) -> std::path::PathBuf {
    let p = dir.join(name);
    let mut bytes = vec![0u8; 21_000_000];
    bytes[..4].copy_from_slice(&[0x6c, 0x6d, 0x67, 0x67]);
    std::fs::write(&p, bytes).unwrap();
    p
}

#[tokio::test]
async fn ai_5_translate_through_the_real_adapters() {
    let Ok(tools) = cm_media::tools::locate() else {
        eprintln!("SKIPPED: FFmpeg was not found");
        return;
    };
    let models = tempfile::tempdir().unwrap();
    let mut engine = ScriptedTranscriber::from_secs(&[
        ("Xin", 1.0, 1.4),
        ("chào", 1.5, 1.7),
        ("mọi", 1.8, 2.3),
        ("người", 2.4, 2.8),
    ])
    .with_translation(&[("Hello", 1.0, 1.4), ("everyone.", 1.5, 2.8)]);
    engine.language = "vi".into();
    engine.model = "large".into();
    let runs = engine.translate_runs.clone();
    let (b, lib) = builder(desktop());
    let queue = cm_daemon::library_job_queue(lib.path()).unwrap();
    let daemon = b
        .jobs(queue)
        .ai(services_with(
            models.path().to_path_buf(),
            Some(Arc::new(engine)),
        ))
        .build();
    daemon.register_ai_handlers().unwrap();
    daemon.start_jobs().unwrap();
    let spec = FixtureSpec {
        title: "AI translate fixture".into(),
        secs: 10,
        pause: None,
        ..FixtureSpec::default()
    };
    let fx = make_fixture(&tools, lib.path(), &spec).unwrap();
    let id = fx.id;
    let run = |method: &'static str, mut p: Value| {
        let daemon = daemon.clone();
        let id = id.clone();
        async move {
            let on_capture = !matches!(method, "rescan_library" | "list_models" | "import_model");
            if on_capture && p.get("capture_id").is_none() {
                p["capture_id"] = json!(id);
            }
            daemon.execute_method(Actor::User, 0, method, &p).await
        }
    };
    run("rescan_library", json!({})).await.unwrap();
    let wait = |job_id: String| {
        let daemon = daemon.clone();
        async move {
            for _ in 0..1500 {
                let j = daemon
                    .execute_method(Actor::User, 0, "get_job", &json!({"job_id": job_id}))
                    .await
                    .unwrap();
                if matches!(j["state"].as_str(), Some("done" | "failed" | "cancelled")) {
                    return j;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            panic!("the job did not finish");
        }
    };

    // Only an unpinned turbo model is catalogued: it cannot be imported, listed or downloaded.
    let list = run("list_models", json!({})).await.unwrap();
    assert!(list["models"]
        .as_array()
        .unwrap()
        .iter()
        .all(|m| m["id"] != "large-v3-turbo"));
    let turbo = fake_model(models.path(), "ggml-large-v3-turbo.bin");
    let e = run(
        "import_model",
        json!({"model": "large-v3-turbo", "path": turbo.to_str().unwrap()}),
    )
    .await
    .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);

    // No model: translation into English cannot start, and says why.
    let e = run("translate_captions", json!({"target_language": "en"}))
        .await
        .unwrap_err();
    // (no transcript yet: the transcript comes first)
    assert_eq!(code(&e), ErrorCode::NotFound);

    // Import large-v3 (id `large`), transcribe, then translate.
    let file = fake_model(models.path(), "ggml-large-v3.bin");
    run(
        "import_model",
        json!({"model": "large-v3", "path": file.to_str().unwrap()}),
    )
    .await
    .unwrap();
    let started = run("transcribe", json!({})).await.unwrap();
    let job = wait(started["job_id"].as_str().unwrap().to_owned()).await;
    assert_eq!(job["state"], "done", "{job}");

    let e = run("translate_captions", json!({"target_language": "vi"}))
        .await
        .unwrap_err();
    assert_eq!(reason(&e), "same_language");
    let e = run("translate_captions", json!({"target_language": "es"}))
        .await
        .unwrap_err();
    assert_eq!(reason(&e), "no_text_model");

    let started = run("translate_captions", json!({"target_language": "en"}))
        .await
        .unwrap();
    assert_eq!(started["engine"], "whisper");
    assert_eq!(started["model"], "large");
    let job = wait(started["job_id"].as_str().unwrap().to_owned()).await;
    assert_eq!(job["state"], "done", "{job}");
    assert_eq!(job["result"]["model"], "large");
    assert_eq!(
        runs.lock().unwrap().as_slice(),
        [Some("vi".to_owned())],
        "Whisper got the spoken language as its hint"
    );
    let got = run("get_captions", json!({"language": "en"}))
        .await
        .unwrap();
    assert_eq!(got["source"], "translation");
    assert_eq!(got["lines"][0]["text"], "Hello everyone.");
    let srt = run(
        "export_captions",
        json!({"language": "en", "format": "srt"}),
    )
    .await
    .unwrap();
    let body = std::fs::read_to_string(srt["path"].as_str().unwrap()).unwrap();
    assert!(body.contains("Hello everyone."), "{body}");
    // The Vietnamese originals are untouched.
    let orig = run("get_captions", json!({})).await.unwrap();
    assert_eq!(orig["source"], "generated");
    assert!(orig["lines"][0]["text"].as_str().unwrap().contains("Xin"));
}
