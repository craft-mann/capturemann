//! M5 integration: the AI actions through the daemon with the REAL adapters over `cm-ai`
//! (model store, captions, suggestions) and a scripted transcriber in place of whisper.cpp, on a
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

/// `HH:MM:SS,mmm` of an SRT cue to seconds.
fn srt_secs(t: &str) -> f64 {
    let (hms, ms) = t.split_once(',').unwrap();
    let mut p = hms.split(':').map(|x| x.parse::<f64>().unwrap());
    let (h, m, s) = (p.next().unwrap(), p.next().unwrap(), p.next().unwrap());
    h * 3600.0 + m * 60.0 + s + ms.parse::<f64>().unwrap() / 1000.0
}

/// The cue start times of an SRT text.
fn cue_starts(srt: &str) -> Vec<f64> {
    srt.lines()
        .filter_map(|l| l.split_once(" --> "))
        .map(|(a, _)| srt_secs(a))
        .collect()
}

struct Rig {
    daemon: cm_daemon::Daemon,
    id: String,
    models: tempfile::TempDir,
    /// The prompt of each run the scripted transcriber served.
    prompts: Arc<std::sync::Mutex<Vec<Option<String>>>>,
    _lib: tempfile::TempDir,
}

async fn rig() -> Option<Rig> {
    let Ok(tools) = cm_media::tools::locate() else {
        eprintln!("SKIPPED: FFmpeg was not found");
        return None;
    };
    let models = tempfile::tempdir().unwrap();
    let mut engine = ScriptedTranscriber::from_secs(&[
        ("Hello", 1.0, 1.4),
        ("and", 1.5, 1.7),
        ("welcome", 1.8, 2.3),
        ("to", 8.0, 8.2),
        ("the", 8.3, 8.5),
        ("demo", 8.6, 9.0),
    ]);
    // The real engine reports the id of the model it loaded.
    engine.model = "small".into();
    let prompts = engine.prompts.clone();
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
        title: "AI fixture".into(),
        secs: 20,
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
        prompts,
        _lib: lib,
    })
}

async fn call(r: &Rig, method: &str, mut p: Value) -> Result<Value, DaemonError> {
    if p.get("capture_id").is_none()
        && matches!(
            method,
            "transcribe"
                | "get_transcript"
                | "get_captions"
                | "export_captions"
                | "suggest_zooms"
                | "suggest_cuts"
                | "cut_segments"
        )
    {
        p["capture_id"] = json!(r.id);
    }
    r.daemon.execute_method(Actor::User, 0, method, &p).await
}

async fn job_done(r: &Rig, job_id: &str) -> Value {
    for _ in 0..1500 {
        let j = call(r, "get_job", json!({"job_id": job_id})).await.unwrap();
        if matches!(j["state"].as_str(), Some("done" | "failed" | "cancelled")) {
            return j;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("the job did not finish");
}

/// A file with the ggml magic and a plausible size, which the store accepts as a model.
fn fake_model(dir: &std::path::Path) -> std::path::PathBuf {
    let p = dir.join("ggml-small.bin");
    let mut bytes = vec![0u8; 21_000_000];
    bytes[..4].copy_from_slice(&[0x6c, 0x6d, 0x67, 0x67]);
    std::fs::write(&p, bytes).unwrap();
    p
}

/// AI-8: the speaker models are rows of `list_models` (kind, licence, import-only flag) managed by
/// the same actions as the speech models; the import checks the pinned file, a download of the
/// import-only row is refused, they are never the speech model, and `diarize` without them says
/// `no_speaker_model`.
#[tokio::test]
async fn ai_8_speaker_models_are_listed_pinned_on_import_and_needed_by_diarize() {
    let Some(r) = rig().await else { return };
    let list = call(&r, "list_models", json!({})).await.unwrap();
    let rows = list["models"].as_array().unwrap();
    let speaker: Vec<&Value> = rows.iter().filter(|m| m["kind"] == "speaker").collect();
    let ids: Vec<&str> = speaker.iter().map(|m| m["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["speaker-segmentation", "speaker-embedding"]);
    assert_eq!(speaker[0]["license"], "MIT");
    assert_eq!(speaker[0]["import_only"], true);
    assert_eq!(speaker[1]["license"], "Apache-2.0");
    assert!(speaker[1].get("import_only").is_none());
    assert!(
        rows.iter()
            .filter(|m| m["kind"] != "speaker")
            .all(|m| m.get("license").is_none()),
        "speech rows are unchanged"
    );

    // An import-only row cannot be downloaded; the error names import_model.
    let e = call(
        &r,
        "download_model",
        json!({"model": "speaker-segmentation"}),
    )
    .await
    .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
    assert!(format!("{e:?}").contains("import_model"), "{e:?}");

    // A file that is not the pinned one is refused and nothing is installed.
    let wrong = r.models.path().join("model.onnx");
    std::fs::write(&wrong, vec![1u8; 5_992_913]).unwrap();
    let e = call(
        &r,
        "import_model",
        json!({"model": "speaker-segmentation", "path": wrong.to_str().unwrap()}),
    )
    .await
    .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
    let list = call(&r, "list_models", json!({})).await.unwrap();
    assert!(list["models"]
        .as_array()
        .unwrap()
        .iter()
        .all(|m| m["installed"] == false));

    // A speaker model is not a speech model.
    let e = call(&r, "transcribe", json!({"model": "speaker-embedding"}))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
    let e = call(&r, "select_model", json!({"model": "speaker-embedding"}))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);

    // Without the models `diarize` says so, and nothing is queued.
    let e = call(
        &r,
        "diarize",
        json!({"capture_id": r.id, "num_speakers": 2}),
    )
    .await
    .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
    assert_eq!(reason(&e), "no_speaker_model");
}

/// AI-1, AI-3, AI-4 end to end: no model, import one, transcribe a fixture recording, read the
/// transcript and caption lines, export SRT through a cut (AC4), and list suggestions.
#[tokio::test]
async fn m5_ai_path_from_no_model_to_captions_and_suggestions() {
    let Some(r) = rig().await else { return };

    // 1. No model installed: the error carries the reason the UI keys on, nothing is queued.
    let e = call(&r, "transcribe", json!({})).await.unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
    assert_eq!(reason(&e), "no_model");
    let list = call(&r, "list_models", json!({})).await.unwrap();
    assert!(list["models"]
        .as_array()
        .unwrap()
        .iter()
        .all(|m| m["installed"] == false));

    // 2. Import a model (no network); the first one becomes the active model.
    let file = fake_model(r.models.path());
    call(
        &r,
        "import_model",
        json!({"model": "small", "path": file.to_str().unwrap()}),
    )
    .await
    .unwrap();
    let list = call(&r, "list_models", json!({})).await.unwrap();
    assert_eq!(list["active"], "small");

    // 3. Transcribe: a job; transcript.json lands in the bundle.
    let started = call(&r, "transcribe", json!({})).await.unwrap();
    let job = job_done(&r, started["job_id"].as_str().unwrap()).await;
    assert_eq!(job["state"], "done", "{job}");
    let t = call(&r, "get_transcript", json!({})).await.unwrap();
    assert_eq!(t["transcript"]["words"].as_array().unwrap().len(), 6);
    assert_eq!(t["transcript"]["model"], "small");

    // 4. get_captions: generated from the words (two lines: a long pause separates them).
    let c = call(&r, "get_captions", json!({})).await.unwrap();
    assert_eq!(c["source"], "generated");
    assert_eq!(c["lines"].as_array().unwrap().len(), 2);

    // 5. SRT without edits, then with a cut between the two lines (AC4: later cues shift by the
    //    cut's length) and with a cut over the first line (the line disappears).
    let whole = call(&r, "export_captions", json!({"format": "srt"}))
        .await
        .unwrap();
    let before = cue_starts(&std::fs::read_to_string(whole["path"].as_str().unwrap()).unwrap());
    assert_eq!(before.len(), 2);
    let cut_start = before[0] + 2.5; // after the first line, before the second
    call(
        &r,
        "cut_segments",
        json!({"ranges": [[cut_start, cut_start + 2.0]]}),
    )
    .await
    .unwrap();
    let cut = call(&r, "export_captions", json!({"format": "srt"}))
        .await
        .unwrap();
    let after = cue_starts(&std::fs::read_to_string(cut["path"].as_str().unwrap()).unwrap());
    assert_eq!(after.len(), 2, "no line was inside the cut");
    assert!((after[0] - before[0]).abs() < 0.002);
    assert!(
        ((before[1] - after[1]) - 2.0).abs() < 0.002,
        "the second cue moves 2 s earlier: {before:?} -> {after:?}"
    );
    call(
        &r,
        "cut_segments",
        json!({"ranges": [[before[0] - 0.05, before[0] + 2.4]]}),
    )
    .await
    .unwrap();
    let cut2 = call(&r, "export_captions", json!({"format": "srt"}))
        .await
        .unwrap();
    let text = std::fs::read_to_string(cut2["path"].as_str().unwrap()).unwrap();
    assert_eq!(cue_starts(&text).len(), 1, "the cut line is gone: {text}");
    assert!(
        !text.contains("Hello") && !text.contains("welcome"),
        "{text}"
    );

    // 5b. The plain-text export follows the same cuts: the cut line is absent, one row per line,
    //     and `timestamps` prefixes `[mm:ss]` shifted like the SRT cue.
    let txt = call(&r, "export_captions", json!({"format": "txt"}))
        .await
        .unwrap();
    let text = std::fs::read_to_string(txt["path"].as_str().unwrap()).unwrap();
    assert_eq!(text.lines().count(), 1, "the cut line is gone: {text}");
    assert!(
        !text.contains("Hello") && !text.contains("welcome") && !text.contains('['),
        "{text}"
    );
    let timed = call(
        &r,
        "export_captions",
        json!({"format": "txt", "timestamps": true}),
    )
    .await
    .unwrap();
    let text = std::fs::read_to_string(timed["path"].as_str().unwrap()).unwrap();
    assert!(text.starts_with('['), "{text}");

    // 6. Edited lines win over the generated ones.
    call(
        &r,
        "set_captions",
        json!({"capture_id": r.id, "lines": [{"start": 4, "end": 5, "text": "Edited"}]}),
    )
    .await
    .unwrap();
    let c = call(&r, "get_captions", json!({})).await.unwrap();
    assert_eq!(c["source"], "edited");
    assert_eq!(c["lines"].as_array().unwrap().len(), 1);

    // 7. Suggestions are listed and never applied.
    let before_edits = call(&r, "get_capture", json!({"capture_id": r.id}))
        .await
        .ok();
    let cuts = call(&r, "suggest_cuts", json!({})).await.unwrap();
    assert!(cuts["suggestions"].is_array(), "{cuts}");
    let zooms = call(&r, "suggest_zooms", json!({})).await.unwrap();
    assert!(zooms["suggestions"].is_array(), "{zooms}");
    let after_edits = call(&r, "get_capture", json!({"capture_id": r.id}))
        .await
        .ok();
    assert_eq!(before_edits, after_edits, "suggesting changes nothing");
}

/// A recording whose transcript does not exist yet has no captions, with the way forward.
#[tokio::test]
async fn m5_get_captions_before_any_transcript_is_not_found() {
    let Some(r) = rig().await else { return };
    let e = call(&r, "get_captions", json!({})).await.unwrap_err();
    assert_eq!(code(&e), ErrorCode::NotFound);
    assert!(e.to_string().contains("transcribe"));
}

/// AI-6a, AC4 of the glossary: with the real adapters, the glossary primes the transcriber with
/// its terms and a heard-as spelling that spans several words is written as the term in the
/// stored transcript (and so in the caption lines).
#[tokio::test]
async fn ai_6a_glossary_primes_the_scripted_transcriber_and_fixes_the_stored_words() {
    let Some(r) = rig().await else { return };
    let file = fake_model(r.models.path());
    call(
        &r,
        "import_model",
        json!({"model": "small", "path": file.to_str().unwrap()}),
    )
    .await
    .unwrap();
    call(
        &r,
        "set_settings",
        json!({"values": {"ai.glossary": [
            {"term": "Demo Day", "heard_as": ["to the demo"]},
            {"term": "Đê Điều", "heard_as": ["đê điều"]},
            {"term": "Hello", "heard_as": ["xin"]}
        ]}}),
    )
    .await
    .unwrap();
    let started = call(&r, "transcribe", json!({})).await.unwrap();
    let job = job_done(&r, started["job_id"].as_str().unwrap()).await;
    assert_eq!(job["state"], "done", "{job}");
    assert_eq!(job["result"]["glossary_replaced"], 1);
    let prompts = r.prompts.lock().unwrap().clone();
    assert_eq!(prompts, vec![Some("Demo Day, Đê Điều, Hello".to_owned())]);
    let t = call(&r, "get_transcript", json!({})).await.unwrap();
    let words: Vec<&str> = t["transcript"]["words"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["text"].as_str().unwrap())
        .collect();
    assert_eq!(words, vec!["Hello", "and", "welcome", "Demo Day"]);
    let lines = call(&r, "get_captions", json!({})).await.unwrap();
    assert_eq!(lines["lines"][1]["text"], "Demo Day");

    // No glossary: no prompt, the words come back as spoken.
    call(&r, "set_settings", json!({"values": {"ai.glossary": []}}))
        .await
        .unwrap();
    let started = call(&r, "transcribe", json!({})).await.unwrap();
    job_done(&r, started["job_id"].as_str().unwrap()).await;
    assert_eq!(r.prompts.lock().unwrap().last().unwrap(), &None);
    let t = call(&r, "get_transcript", json!({})).await.unwrap();
    assert_eq!(t["transcript"]["words"].as_array().unwrap().len(), 6);
}
