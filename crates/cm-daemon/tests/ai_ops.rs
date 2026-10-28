//! The local-AI actions through the daemon (M5, AI-1, AI-3, AI-4) with a scripted engine
//! ([`FakeAi`]): the no-model error, model downloads as jobs (progress, cancel), the Confirm gate,
//! transcripts, undoable captions, caption files and suggestions. No Whisper and no network.

#![allow(clippy::unwrap_used)]

mod common;

use std::sync::Arc;
use std::time::Duration;

use cm_core::{Actor, ErrorCode, MediaTime, TrackInfo, TrackKind, TrackStats};
use cm_daemon::testing::{FakeAi, FixedConfirmer};
use cm_daemon::{Daemon, DaemonError, Decision, Event};
use common::{builder, desktop};
use serde_json::{json, Map, Value};
use time::macros::datetime;
use tokio::sync::broadcast::Receiver;

fn agent(n: &str) -> Actor {
    Actor::agent(n).unwrap()
}

fn code(e: &DaemonError) -> ErrorCode {
    match e {
        DaemonError::Cm(c) => c.code(),
        DaemonError::WithData { error, .. } => error.code(),
        DaemonError::NotImplemented(_) => ErrorCode::NotImplemented,
        DaemonError::Declined { .. } => ErrorCode::UserDeclined,
        other => panic!("unexpected {other:?}"),
    }
}

fn reason(e: &DaemonError) -> String {
    match e {
        DaemonError::WithData { data, .. } => data["reason"].as_str().unwrap_or("").to_owned(),
        DaemonError::Declined { reason, .. } => (*reason).to_owned(),
        _ => String::new(),
    }
}

struct Rig {
    daemon: Daemon,
    ai: FakeAi,
    /// A finished 60 s recording with a mic track.
    id: String,
    dir: std::path::PathBuf,
    confirmer: Arc<FixedConfirmer>,
    tmp: tempfile::TempDir,
}

fn recording(tmp: &std::path::Path, title: &str, mic: bool) -> (String, std::path::PathBuf) {
    use cm_library::rec::{RecBundle, SessionInfo};
    let now = datetime!(2026-10-22 10:00:00 UTC);
    let session = SessionInfo {
        clock_origin_ns: 1,
        timebase: "uptime_raw_ns".into(),
        displays: vec![],
        devices: json!({}),
        app_version: "0.0.1".into(),
        extra: Map::new(),
    };
    let rec = RecBundle::create(tmp, title, now, json!({}), &session).unwrap();
    std::fs::write(rec.dir().join("screen.mp4"), b"master bytes").unwrap();
    let mut tracks = vec![TrackInfo {
        kind: TrackKind::Screen,
        file: "screen.mp4".into(),
        start: MediaTime::ZERO,
        end: MediaTime::from_secs(60),
        details: json!({}),
        stats: TrackStats::default(),
    }];
    if mic {
        std::fs::write(rec.dir().join("mic.wav"), b"audio").unwrap();
        tracks.push(TrackInfo {
            kind: TrackKind::Mic,
            file: "mic.wav".into(),
            start: MediaTime::ZERO,
            end: MediaTime::from_secs(60),
            details: json!({}),
            stats: TrackStats::default(),
        });
    }
    let dir = rec.dir().to_path_buf();
    let m = rec.finalize(&tracks, now).unwrap();
    (m.id, dir)
}

async fn rig(ai: FakeAi, decision: Decision) -> Rig {
    let (b, tmp) = builder(desktop());
    let queue = cm_daemon::library_job_queue(tmp.path()).unwrap();
    let confirmer = Arc::new(FixedConfirmer::new(decision));
    let daemon = b
        .jobs(queue)
        .confirmer(confirmer.clone())
        .ai(ai.services())
        .build();
    daemon.register_ai_handlers().unwrap();
    daemon.start_jobs().unwrap();
    let (id, dir) = recording(tmp.path(), "Lesson", true);
    daemon
        .execute_method(Actor::User, 0, "rescan_library", &json!({}))
        .await
        .unwrap();
    Rig {
        daemon,
        ai,
        id,
        dir,
        confirmer,
        tmp,
    }
}

async fn call(r: &Rig, who: Actor, method: &str, mut p: Value) -> Result<Value, DaemonError> {
    let on_a_capture = matches!(
        method,
        "transcribe"
            | "get_transcript"
            | "get_captions"
            | "set_captions"
            | "replace_in_captions"
            | "set_caption_options"
            | "export_captions"
            | "suggest_zooms"
            | "suggest_cuts"
            | "cut_segments"
            | "undo"
            | "redo"
    );
    if on_a_capture && p.get("capture_id").is_none() {
        p["capture_id"] = json!(r.id);
    }
    r.daemon.execute_method(who, 0, method, &p).await
}

async fn user(r: &Rig, method: &str, p: Value) -> Result<Value, DaemonError> {
    call(r, Actor::User, method, p).await
}

async fn until<T>(mut f: impl AsyncFnMut() -> Option<T>) -> T {
    for _ in 0..500 {
        if let Some(v) = f().await {
            return v;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("timed out");
}

async fn job_done(r: &Rig, job_id: &str) -> Value {
    until(async || {
        let j = user(r, "get_job", json!({"job_id": job_id})).await.unwrap();
        matches!(j["state"].as_str(), Some("done" | "failed" | "cancelled")).then_some(j)
    })
    .await
}

fn drain(rx: &mut Receiver<Event>) -> Vec<Event> {
    let mut v = vec![];
    while let Ok(e) = rx.try_recv() {
        v.push(e);
    }
    v
}

fn manifest(dir: &std::path::Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(dir.join("manifest.json")).unwrap()).unwrap()
}

/// AI-1: without an installed model `transcribe` fails with `invalid_input` and
/// `data.reason = "no_model"`, points to the AI page, and nothing is queued or downloaded.
#[tokio::test]
async fn ai_1_transcribe_without_a_model_says_no_model_and_downloads_nothing() {
    let r = rig(FakeAi::new(), Decision::Approve).await;
    let e = user(&r, "transcribe", json!({})).await.unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
    assert_eq!(reason(&e), "no_model");
    let msg = e.to_string();
    assert!(
        msg.contains("AI page") && msg.contains("capturemann ai models"),
        "{msg}"
    );
    let jobs = user(&r, "list_jobs", json!({})).await.unwrap();
    assert!(jobs["jobs"].as_array().unwrap().is_empty());
    assert!(
        r.ai.downloads_started().is_empty(),
        "nothing is downloaded silently"
    );
    // A model that is named but missing is a different, specific error.
    let r2 = rig(FakeAi::new().with_small_installed(), Decision::Approve).await;
    let e = user(&r2, "transcribe", json!({"model": "medium"}))
        .await
        .unwrap_err();
    assert_eq!(reason(&e), "model_not_installed");
}

/// AI-1: `transcribe` is a job; the transcript lands in the bundle and `get_transcript` returns
/// it with the generated caption lines; `transcript.ready` is published.
#[tokio::test]
async fn ai_1_transcribe_job_writes_transcript_json_and_publishes_ready() {
    let r = rig(FakeAi::new().with_small_installed(), Decision::Approve).await;
    let mut rx = r.daemon.subscribe();
    let started = user(&r, "transcribe", json!({"language": "auto"}))
        .await
        .unwrap();
    assert_eq!(started["model"], "small");
    let job = job_done(&r, started["job_id"].as_str().unwrap()).await;
    assert_eq!(job["state"], "done", "{job}");
    assert_eq!(job["result"]["words"], 6);
    assert!(r.dir.join("transcript.json").is_file());
    let calls = r.ai.transcribe_calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(
        (calls[0].model.as_str(), calls[0].language.clone()),
        ("small", None)
    );
    assert_eq!(calls[0].bundle_dir, r.dir);
    assert!(drain(&mut rx).iter().any(|e| e.name == "transcript.ready"
        && e.data["capture_id"] == json!(r.id)
        && e.data["words"] == 6));
    let t = user(&r, "get_transcript", json!({})).await.unwrap();
    assert_eq!(t["transcript"]["language"], "en");
    assert_eq!(t["transcript"]["words"].as_array().unwrap().len(), 6);
    assert_eq!(t["captions_source"], "generated");
    assert_eq!(t["captions"].as_array().unwrap().len(), 2);
    // No transcript yet on another recording: not_found, with the way forward.
    let (other, _) = recording(r.tmp.path(), "Other", true);
    r.daemon
        .execute_method(Actor::User, 0, "rescan_library", &json!({}))
        .await
        .unwrap();
    let e = user(&r, "get_transcript", json!({"capture_id": other}))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::NotFound);
    assert!(e.to_string().contains("transcribe"));
}

/// AI-1: the recording's FIRST transcript switches the preview captions on and leaves the export
/// default off, without an extra revision; `get_transcript` reports the switches.
#[tokio::test]
async fn ai_1_first_transcript_shows_captions_but_does_not_burn_them_in() {
    let r = rig(FakeAi::new().with_small_installed(), Decision::Approve).await;
    let revs_before = manifest(&r.dir)["revisions"].as_array().unwrap().len();
    let started = user(&r, "transcribe", json!({})).await.unwrap();
    let job = job_done(&r, started["job_id"].as_str().unwrap()).await;
    assert_eq!(job["result"]["captions_shown"], true, "{job}");
    let c = &manifest(&r.dir)["captions"];
    assert_eq!(c["enabled"], true);
    assert_eq!(c["burn_in"], false, "the export default stays off");
    assert_eq!(
        manifest(&r.dir)["revisions"].as_array().unwrap().len(),
        revs_before,
        "no extra undo step"
    );
    let t = user(&r, "get_transcript", json!({})).await.unwrap();
    assert_eq!(
        t["caption_options"],
        json!({"enabled": true, "burn_in": false})
    );

    // The user turns captions off; a re-generated transcript leaves that alone.
    user(&r, "set_caption_options", json!({"enabled": false}))
        .await
        .unwrap();
    let again = user(&r, "transcribe", json!({})).await.unwrap();
    let job = job_done(&r, again["job_id"].as_str().unwrap()).await;
    assert_eq!(job["result"]["captions_shown"], false);
    assert_eq!(manifest(&r.dir)["captions"]["enabled"], false);
}

/// A choice made before the first transcript (even "off", the value it already had) is kept.
#[tokio::test]
async fn ai_1_an_explicit_caption_choice_before_the_first_transcript_is_kept() {
    let r = rig(FakeAi::new().with_small_installed(), Decision::Approve).await;
    user(&r, "set_caption_options", json!({"enabled": false}))
        .await
        .unwrap();
    let started = user(&r, "transcribe", json!({})).await.unwrap();
    let job = job_done(&r, started["job_id"].as_str().unwrap()).await;
    assert_eq!(job["result"]["captions_shown"], false, "{job}");
    assert_eq!(manifest(&r.dir)["captions"]["enabled"], false);
}

/// A repeat with the same `request_id`, and an equal job already running, share one job.
#[tokio::test]
async fn ai_1_transcribe_is_idempotent_and_reuses_the_running_job() {
    let r = rig(
        FakeAi::new().with_small_installed().slow_downloads(60, 10),
        Decision::Approve,
    )
    .await;
    let p = json!({"request_id": "t1"});
    let a = user(&r, "transcribe", p.clone()).await.unwrap();
    let b = user(&r, "transcribe", p).await.unwrap();
    assert_eq!(a["job_id"], b["job_id"]);
    let c = user(&r, "transcribe", json!({})).await.unwrap();
    assert_eq!(c["job_id"], a["job_id"], "an equal job is reused");
    assert_eq!(c["reused"], true);
    // Cancel it so the test ends quickly.
    user(&r, "cancel_job", json!({"job_id": a["job_id"]}))
        .await
        .unwrap();
    assert_eq!(
        job_done(&r, a["job_id"].as_str().unwrap()).await["state"],
        "cancelled"
    );
}

/// Screenshots and recordings without a microphone or system track have nothing to transcribe.
#[tokio::test]
async fn ai_1_recordings_without_audio_and_screenshots_are_refused() {
    let r = rig(FakeAi::new().with_small_installed(), Decision::Approve).await;
    let (silent, _) = recording(r.tmp.path(), "Silent", false);
    r.daemon
        .execute_method(Actor::User, 0, "rescan_library", &json!({}))
        .await
        .unwrap();
    let e = user(&r, "transcribe", json!({"capture_id": silent}))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
    assert!(e.to_string().contains("no microphone"), "{e}");
    let shot = user(
        &r,
        "capture_screenshot",
        json!({"target": {"display_id": 1}}),
    )
    .await
    .unwrap();
    for m in [
        "transcribe",
        "get_transcript",
        "set_captions",
        "export_captions",
        "suggest_zooms",
    ] {
        let mut p = json!({"capture_id": shot["capture_id"]});
        if m == "set_captions" {
            p["lines"] = json!([]);
        }
        if m == "export_captions" {
            p["format"] = json!("srt");
        }
        let e = user(&r, m, p).await.unwrap_err();
        assert_eq!(code(&e), ErrorCode::InvalidInput, "{m}: {e}");
    }
}

/// AGT-2: `download_model` and `remove_model` are Confirm tier. The dialog says exactly what runs
/// (name, size, where it goes), a decline changes nothing, an approval does the job.
#[tokio::test]
async fn agt_2_download_and_remove_wait_for_the_user_and_show_the_plan() {
    let r = rig(FakeAi::new(), Decision::Decline).await;
    let e = call(
        &r,
        agent("claude"),
        "download_model",
        json!({"model": "small"}),
    )
    .await
    .unwrap_err();
    assert_eq!(code(&e), ErrorCode::UserDeclined);
    assert!(r.ai.downloads_started().is_empty());
    let shown = r.confirmer.shown();
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].tool, "download_model");
    // P3: the typed plan next to the English dialog text.
    assert_eq!(shown[0].kind, "download_model");
    assert_eq!(
        shown[0].params,
        json!({"model": "small", "name": "Small", "size_mb": 466, "destination": "/models"})
    );
    assert!(
        shown[0].summary.contains("Small") && shown[0].summary.contains("466 MB"),
        "{}",
        shown[0].summary
    );
    assert!(shown[0].summary.contains("internet"));
    assert_eq!(shown[0].details["destination"], "/models");
    assert_eq!(shown[0].details["size_mb"], 466);

    let ok = rig(FakeAi::new(), Decision::Approve).await;
    let started = call(
        &ok,
        agent("claude"),
        "download_model",
        json!({"model": "small"}),
    )
    .await
    .unwrap();
    assert_eq!(
        job_done(&ok, started["job_id"].as_str().unwrap()).await["state"],
        "done"
    );
    assert_eq!(ok.ai.installed(), vec!["small"]);
    // Removing asks too, and a decline keeps the file.
    let keep = rig(FakeAi::new().with_small_installed(), Decision::Decline).await;
    let e = call(
        &keep,
        agent("claude"),
        "remove_model",
        json!({"model": "small"}),
    )
    .await
    .unwrap_err();
    assert_eq!(code(&e), ErrorCode::UserDeclined);
    assert_eq!(keep.ai.installed(), vec!["small"]);
    assert!(keep.confirmer.shown()[0]
        .summary
        .contains("Delete the speech model"));
    let gone = rig(FakeAi::new().with_small_installed(), Decision::Approve).await;
    call(
        &gone,
        agent("claude"),
        "remove_model",
        json!({"model": "small"}),
    )
    .await
    .unwrap();
    assert!(gone.ai.installed().is_empty());
    // The user's own app skips the dialog (it asks itself).
    let own = rig(FakeAi::new(), Decision::Decline).await;
    let s = user(&own, "download_model", json!({"model": "base"}))
        .await
        .unwrap();
    assert_eq!(
        job_done(&own, s["job_id"].as_str().unwrap()).await["state"],
        "done"
    );
    assert!(own.confirmer.shown().is_empty());
}

/// Requests that cannot run are refused before any dialog: unknown model, already installed, not
/// installed.
#[tokio::test]
async fn agt_2_impossible_requests_fail_before_the_dialog() {
    let r = rig(FakeAi::new().with_small_installed(), Decision::Approve).await;
    let e = call(
        &r,
        agent("claude"),
        "download_model",
        json!({"model": "huge"}),
    )
    .await
    .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
    assert!(e.to_string().contains("base, small, medium"), "{e}");
    let e = call(
        &r,
        agent("claude"),
        "download_model",
        json!({"model": "small"}),
    )
    .await
    .unwrap_err();
    assert!(e.to_string().contains("already installed"));
    let e = call(
        &r,
        agent("claude"),
        "remove_model",
        json!({"model": "base"}),
    )
    .await
    .unwrap_err();
    assert_eq!(code(&e), ErrorCode::NotFound);
    assert!(
        r.confirmer.shown().is_empty(),
        "no dialog for a request that cannot run"
    );
}

/// A daemon without an AI engine answers `not_implemented` and never opens a dialog for the
/// Confirm-tier actions.
#[tokio::test]
async fn a_daemon_without_an_engine_refuses_before_any_dialog() {
    let (b, tmp) = builder(desktop());
    let confirmer = Arc::new(FixedConfirmer::new(Decision::Approve));
    let queue = cm_daemon::library_job_queue(tmp.path()).unwrap();
    let d = b.jobs(queue).confirmer(confirmer.clone()).build();
    for (m, p) in [
        ("download_model", json!({"model": "small"})),
        ("remove_model", json!({"model": "small"})),
    ] {
        let e = d
            .execute_method(agent("claude"), 0, m, &p)
            .await
            .unwrap_err();
        assert_eq!(code(&e), ErrorCode::NotImplemented, "{m}");
    }
    assert!(confirmer.shown().is_empty());
    let e = d
        .execute_method(Actor::User, 0, "list_models", &json!({}))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::NotImplemented);
    assert!(e.to_string().contains("not available in this build"));
    // The capability list marks them unavailable.
    let avail = |name: &str| {
        d.actions()
            .into_iter()
            .find(|(s, _)| s.name == name)
            .unwrap()
            .1
    };
    assert!(!avail("download_model"));
}

/// EXP-4 style: a download is a job with progress and `model.download.progress` events; cancel
/// stops it and nothing is installed; the same model is not downloaded twice at once.
#[tokio::test]
async fn model_download_reports_progress_and_can_be_cancelled() {
    let r = rig(FakeAi::new().slow_downloads(500, 10), Decision::Approve).await;
    let mut rx = r.daemon.subscribe();
    let a = user(&r, "download_model", json!({"model": "small"}))
        .await
        .unwrap();
    let b = user(&r, "download_model", json!({"model": "small"}))
        .await
        .unwrap();
    assert_eq!(a["job_id"], b["job_id"]);
    assert_eq!(b["reused"], true);
    let id = a["job_id"].as_str().unwrap().to_owned();
    until(async || {
        let j = user(&r, "get_job", json!({"job_id": id})).await.unwrap();
        (j["progress"].as_f64().unwrap_or(0.0) > 0.0).then_some(())
    })
    .await;
    // A model that is being downloaded cannot be removed.
    let e = user(&r, "remove_model", json!({"model": "small"}))
        .await
        .unwrap_err();
    assert_eq!(
        code(&e),
        ErrorCode::NotFound,
        "not installed yet: nothing to remove ({e})"
    );
    user(&r, "cancel_job", json!({"job_id": id})).await.unwrap();
    assert_eq!(job_done(&r, &id).await["state"], "cancelled");
    assert!(r.ai.installed().is_empty());
    let events = drain(&mut rx);
    assert!(events.iter().any(|e| e.name == "model.download.progress"
        && e.data["model"] == "small"
        && e.data["fraction"].as_f64().unwrap() > 0.0));
    assert!(!events.iter().any(|e| e.name == "model.changed"));
}

/// A finished download installs the model, makes it active when it is the first, and publishes
/// `model.changed`; `list_models` shows it.
#[tokio::test]
async fn model_download_installs_selects_and_lists() {
    let r = rig(FakeAi::new(), Decision::Approve).await;
    let mut rx = r.daemon.subscribe();
    let before = user(&r, "list_models", json!({})).await.unwrap();
    assert_eq!(before["models"].as_array().unwrap().len(), 3);
    assert!(before["models"]
        .as_array()
        .unwrap()
        .iter()
        .all(|m| m["installed"] == false));
    assert_eq!(before["dir"], "/models");
    let s = user(&r, "download_model", json!({"model": "base"}))
        .await
        .unwrap();
    assert_eq!(
        job_done(&r, s["job_id"].as_str().unwrap()).await["state"],
        "done"
    );
    let after = user(&r, "list_models", json!({})).await.unwrap();
    assert_eq!(after["active"], "base");
    assert!(drain(&mut rx)
        .iter()
        .any(|e| e.name == "model.changed" && e.data["change"] == "downloaded"));
    // Import and select.
    let file = cm_daemon::testing::fake_ai::some_file(r.tmp.path());
    user(&r, "import_model", json!({"model": "small", "path": file}))
        .await
        .unwrap();
    let e = user(
        &r,
        "import_model",
        json!({"model": "small", "path": "/nope/x.bin"}),
    )
    .await
    .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
    user(&r, "select_model", json!({"model": "small"}))
        .await
        .unwrap();
    assert_eq!(r.ai.active().as_deref(), Some("small"));
    let e = user(&r, "select_model", json!({"model": "medium"}))
        .await
        .unwrap_err();
    assert!(e.to_string().contains("not installed"));
}

/// A model that a queued or running transcription uses cannot be removed (`busy`).
#[tokio::test]
async fn removing_a_model_in_use_is_busy() {
    let r = rig(
        FakeAi::new().with_small_installed().slow_downloads(60, 10),
        Decision::Approve,
    )
    .await;
    let s = user(&r, "transcribe", json!({})).await.unwrap();
    let e = user(&r, "remove_model", json!({"model": "small"}))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::Busy, "{e}");
    user(&r, "cancel_job", json!({"job_id": s["job_id"]}))
        .await
        .unwrap();
    job_done(&r, s["job_id"].as_str().unwrap()).await;
    user(&r, "remove_model", json!({"model": "small"}))
        .await
        .unwrap();
}

/// AI-1: edited caption lines are one revision tagged with the caller and one undo step; undo
/// restores the lines before, redo the edited ones; the master and the edit list are untouched.
#[tokio::test]
async fn ai_1_set_captions_is_one_revision_and_one_undo_step() {
    let r = rig(FakeAi::new().with_small_installed(), Decision::Approve).await;
    // A transcript first: re-generating one later discards edited lines (see the regenerate test).
    let s = user(&r, "transcribe", json!({})).await.unwrap();
    job_done(&r, s["job_id"].as_str().unwrap()).await;
    let master = std::fs::read(r.dir.join("screen.mp4")).unwrap();
    // The first transcript switched the preview captions on; editing lines must not touch that.
    let enabled_before = manifest(&r.dir)["captions"]["enabled"].clone();
    assert_eq!(enabled_before, true);
    let lines = json!([
        {"start": 1.0, "end": 3.0, "text": "Hello and welcome"},
        {"start": "t:480000", "end": 6.5, "text": "to the demo"}]);
    let a = call(&r, agent("claude"), "set_captions", json!({"lines": lines}))
        .await
        .unwrap();
    assert_eq!(a["action"], "set_captions");
    assert_eq!(a["by"], "agent:claude");
    assert_eq!(a["lines"], 2);
    assert_eq!(a["can_undo"], true);
    let m = manifest(&r.dir);
    assert_eq!(m["captions"]["lines"].as_array().unwrap().len(), 2);
    assert_eq!(m["captions"]["lines"][1]["start"], 480_000);
    assert_eq!(
        m["captions"]["enabled"], enabled_before,
        "other caption fields stay"
    );
    assert_eq!(
        m["revisions"].as_array().unwrap().last().unwrap()["action"],
        "set_captions"
    );
    assert_eq!(std::fs::read(r.dir.join("screen.mp4")).unwrap(), master);

    // get_transcript says the lines were edited.
    let t = user(&r, "get_transcript", json!({})).await.unwrap();
    assert_eq!(t["captions_source"], "edited");
    assert_eq!(t["captions"][0]["text"], "Hello and welcome");

    // Undo / redo of the recording walk the caption step.
    let u = user(&r, "undo", json!({})).await.unwrap();
    assert_eq!(u["changed"], true);
    assert!(manifest(&r.dir)["captions"].get("lines").is_none());
    user(&r, "redo", json!({})).await.unwrap();
    assert_eq!(
        manifest(&r.dir)["captions"]["lines"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    // Bad lines are refused and change nothing.
    let e = call(
        &r,
        agent("claude"),
        "set_captions",
        json!({"lines": [{"start": 3, "end": 1, "text": "x"}]}),
    )
    .await
    .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
    assert_eq!(
        manifest(&r.dir)["captions"]["lines"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    // An empty list clears them, still undoable.
    user(&r, "set_captions", json!({"lines": []}))
        .await
        .unwrap();
    assert!(manifest(&r.dir)["captions"].get("lines").is_none());
    user(&r, "undo", json!({})).await.unwrap();
    assert!(manifest(&r.dir)["captions"].get("lines").is_some());
    // Idempotent by request_id.
    let p = json!({"lines": lines, "request_id": "c1"});
    let one = user(&r, "set_captions", p.clone()).await.unwrap();
    let two = user(&r, "set_captions", p).await.unwrap();
    assert_eq!(one["revision"], two["revision"]);
}

/// AI-1: `export_captions` writes SRT or VTT into `exports/`, from the edited lines when there are
/// some, else from the transcript, and hands the edit list to the renderer.
#[tokio::test]
async fn ai_1_export_captions_writes_srt_and_vtt() {
    let r = rig(FakeAi::new().with_small_installed(), Decision::Approve).await;
    // Nothing to export yet.
    let e = user(&r, "export_captions", json!({"format": "srt"}))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::NotFound);
    let s = user(&r, "transcribe", json!({})).await.unwrap();
    job_done(&r, s["job_id"].as_str().unwrap()).await;
    let srt = user(&r, "export_captions", json!({"format": "srt"}))
        .await
        .unwrap();
    assert_eq!(srt["format"], "srt");
    assert_eq!(srt["lines"], 2);
    let path = std::path::PathBuf::from(srt["path"].as_str().unwrap());
    assert_eq!(path.parent().unwrap(), r.dir.join("exports"));
    assert_eq!(path.extension().unwrap(), "srt");
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        text.starts_with("1\n00:00:01,000 --> 00:00:02,300\nHello and welcome"),
        "{text}"
    );
    let vtt = user(&r, "export_captions", json!({"format": "vtt"}))
        .await
        .unwrap();
    let text = std::fs::read_to_string(vtt["path"].as_str().unwrap()).unwrap();
    assert!(
        text.starts_with("WEBVTT\n\n00:00:01.000 --> 00:00:02.300"),
        "{text}"
    );
    // Edited lines win over the generated ones.
    user(
        &r,
        "set_captions",
        json!({"lines": [{"start": 5, "end": 6, "text": "Only line"}]}),
    )
    .await
    .unwrap();
    let srt = user(&r, "export_captions", json!({"format": "srt"}))
        .await
        .unwrap();
    assert_eq!(srt["lines"], 1);
    assert!(std::fs::read_to_string(srt["path"].as_str().unwrap())
        .unwrap()
        .contains("Only line"));
    let calls = r.ai.render_calls();
    assert_eq!(calls.last().unwrap().1, 1);
    assert_eq!(calls.len(), 3);
}

/// AI-3 and AI-4: suggestions are listed, never applied; cut suggestions add filler words when
/// there is a transcript; each says how to apply it.
#[tokio::test]
async fn ai_3_and_4_suggestions_are_listed_and_not_applied() {
    let r = rig(FakeAi::new().with_small_installed(), Decision::Approve).await;
    let edits_before = manifest(&r.dir)["edits"].clone();
    let z = user(&r, "suggest_zooms", json!({})).await.unwrap();
    assert_eq!(z["count"], 1);
    assert_eq!(z["apply_with"], "set_zooms");
    assert_eq!(z["suggestions"][0]["start_s"], 10.0);
    assert_eq!(z["suggestions"][0]["reason"], "click");
    assert_eq!(z["suggestions"][0]["focus"], json!([0.4f32, 0.6f32]));
    let c = user(&r, "suggest_cuts", json!({})).await.unwrap();
    assert_eq!(c["count"], 1, "silence only without a transcript");
    assert_eq!(c["apply_with"], "cut_segments");
    let s = user(&r, "transcribe", json!({})).await.unwrap();
    job_done(&r, s["job_id"].as_str().unwrap()).await;
    let c = user(&r, "suggest_cuts", json!({})).await.unwrap();
    assert_eq!(c["count"], 2);
    assert_eq!(c["suggestions"][1]["kind"], "filler");
    assert_eq!(
        manifest(&r.dir)["edits"],
        edits_before,
        "nothing was applied"
    );
    // Accepting is the existing action, with the suggested times.
    let cut = &c["suggestions"][0];
    user(
        &r,
        "cut_segments",
        json!({"ranges": [[cut["start_s"], cut["end_s"]]]}),
    )
    .await
    .unwrap();
}

/// AGT-2: the AI settings are app-only keys: an agent cannot turn auto-transcribe on or off.
#[tokio::test]
async fn ai_settings_are_app_only_for_agents() {
    let r = rig(FakeAi::new(), Decision::Approve).await;
    let e = r
        .daemon
        .execute_method(
            agent("claude"),
            0,
            "set_settings",
            &json!({"values": {"ai.auto_transcribe": true}}),
        )
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::UserDeclined);
    assert_eq!(reason(&e), "app_only_setting");
    // The app itself can.
    r.daemon
        .execute_method(
            Actor::User,
            0,
            "set_settings",
            &json!({"values": {"ai.auto_transcribe": true}}),
        )
        .await
        .unwrap();
}

/// AI-1: the automatic transcription after a recording: on by the setting, only with an
/// installed model, only for a recording with audio, never touching the network.
#[tokio::test]
async fn ai_1_auto_transcribe_needs_the_setting_a_model_and_audio() {
    let stopped = |r: &Rig, id: &str, tracks: Value| {
        r.daemon.event_bus().publish(
            "recording.stopped",
            json!({"capture_id": id, "tracks": tracks, "reason": "stopped"}),
        );
    };
    let jobs = async |r: &Rig| {
        user(r, "list_jobs", json!({"type": "transcribe"}))
            .await
            .unwrap()["jobs"]
            .as_array()
            .unwrap()
            .len()
    };
    let settle = || tokio::time::sleep(Duration::from_millis(150));
    let on = json!({"values": {"ai.auto_transcribe": true}});

    // Setting on, no model installed: nothing happens and nothing is downloaded.
    let r = rig(FakeAi::new(), Decision::Approve).await;
    user(&r, "set_settings", on.clone()).await.unwrap();
    r.daemon.start_ai_auto_transcribe();
    tokio::task::yield_now().await;
    stopped(&r, &r.id, json!(["screen", "mic"]));
    settle().await;
    assert_eq!(jobs(&r).await, 0);
    assert!(r.ai.downloads_started().is_empty());

    // Setting on, model installed, audio: one job (a repeat event reuses it).
    let r = rig(
        FakeAi::new().with_small_installed().slow_downloads(60, 10),
        Decision::Approve,
    )
    .await;
    user(&r, "set_settings", on.clone()).await.unwrap();
    let mut rx = r.daemon.subscribe();
    r.daemon.start_ai_auto_transcribe();
    tokio::task::yield_now().await;
    stopped(&r, &r.id, json!(["screen", "mic"]));
    stopped(&r, &r.id, json!(["screen", "mic"]));
    until(async || (jobs(&r).await == 1).then_some(())).await;
    settle().await;
    assert_eq!(jobs(&r).await, 1);
    // The app is told, once, so the editor's tabs show the run (a long recording takes minutes).
    let mut started = Vec::new();
    while let Ok(e) = rx.try_recv() {
        if e.name == "transcript.started" {
            started.push(e);
        }
    }
    assert_eq!(started.len(), 1, "one event for the one job: {started:?}");
    assert_eq!(started[0].data["capture_id"], json!(r.id));
    assert_eq!(started[0].data["automatic"], json!(true));
    assert!(started[0].data["job_id"]
        .as_str()
        .is_some_and(|j| !j.is_empty()));

    // No audio track in the event: nothing.
    let r = rig(FakeAi::new().with_small_installed(), Decision::Approve).await;
    user(&r, "set_settings", on).await.unwrap();
    r.daemon.start_ai_auto_transcribe();
    tokio::task::yield_now().await;
    stopped(&r, &r.id, json!(["screen"]));
    settle().await;
    assert_eq!(jobs(&r).await, 0);

    // The user turned it off: nothing, whatever the RAM.
    let r = rig(FakeAi::new().with_small_installed(), Decision::Approve).await;
    user(
        &r,
        "set_settings",
        json!({"values": {"ai.auto_transcribe": false}}),
    )
    .await
    .unwrap();
    r.daemon.start_ai_auto_transcribe();
    tokio::task::yield_now().await;
    stopped(&r, &r.id, json!(["screen", "mic"]));
    settle().await;
    assert_eq!(jobs(&r).await, 0);
}

/// A failing transcription fails the job with the engine's reason and writes no transcript.
#[tokio::test]
async fn ai_1_a_failed_transcription_leaves_no_transcript() {
    let ai = FakeAi::new().with_small_installed();
    ai.fail_transcribe_with(cm_core::CmError::invalid_input("The audio is silent"));
    let r = rig(ai, Decision::Approve).await;
    let s = user(&r, "transcribe", json!({})).await.unwrap();
    let job = job_done(&r, s["job_id"].as_str().unwrap()).await;
    assert_eq!(job["state"], "failed", "{job}");
    assert!(job["error"]["reason"].as_str().unwrap().contains("silent"));
    assert!(!r.dir.join("transcript.json").exists());
}

/// AI-1: `export_captions` with `txt` writes `<title>.txt`: one caption line per row, no times by
/// default, `[mm:ss]` prefixes with `timestamps`; edited lines win; `timestamps` only goes with
/// `txt`; with no transcript it is `not_found` like SRT.
#[tokio::test]
async fn ai_1_export_captions_txt_plain_and_with_timestamps() {
    let r = rig(FakeAi::new().with_small_installed(), Decision::Approve).await;
    let e = user(&r, "export_captions", json!({"format": "txt"}))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::NotFound);
    let s = user(&r, "transcribe", json!({})).await.unwrap();
    job_done(&r, s["job_id"].as_str().unwrap()).await;

    let plain = user(&r, "export_captions", json!({"format": "txt"}))
        .await
        .unwrap();
    assert_eq!(plain["format"], "txt");
    let path = std::path::PathBuf::from(plain["path"].as_str().unwrap());
    assert_eq!(path.parent().unwrap(), r.dir.join("exports"));
    assert_eq!(path.file_name().unwrap(), "Lesson.txt");
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(!text.contains("-->") && !text.contains('['), "{text}");
    assert_eq!(text.lines().count(), 2, "{text}");
    assert!(text.starts_with("Hello and welcome"), "{text}");
    assert!(text.ends_with('\n'));

    let timed = user(
        &r,
        "export_captions",
        json!({"format": "txt", "timestamps": true}),
    )
    .await
    .unwrap();
    let text = std::fs::read_to_string(timed["path"].as_str().unwrap()).unwrap();
    assert!(text.starts_with("[00:01] Hello and welcome"), "{text}");
    assert_eq!(text.lines().count(), 2, "{text}");

    // Edited lines win, exactly as for SRT.
    user(
        &r,
        "set_captions",
        json!({"lines": [{"start": 5, "end": 6, "text": "Only line"}]}),
    )
    .await
    .unwrap();
    let edited = user(&r, "export_captions", json!({"format": "txt"}))
        .await
        .unwrap();
    assert_eq!(edited["lines"], 1);
    assert_eq!(
        std::fs::read_to_string(edited["path"].as_str().unwrap()).unwrap(),
        "Only line\n"
    );

    // `timestamps` is for txt only; an unknown format names the choices.
    let e = user(
        &r,
        "export_captions",
        json!({"format": "srt", "timestamps": true}),
    )
    .await
    .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
    let e = user(&r, "export_captions", json!({"format": "docx"}))
        .await
        .unwrap_err();
    assert!(e.to_string().contains("txt"), "{e}");
}

/// Re-generating: a second `transcribe` replaces the transcript, and the caption lines the user
/// edited (they belong to the old transcript) are discarded as ONE undoable revision; undo brings
/// them back. `transcript.ready` says it replaced one and cleared the captions.
#[tokio::test]
async fn ai_1_regenerate_replaces_the_transcript_and_clears_edits_undoably() {
    let r = rig(FakeAi::new().with_small_installed(), Decision::Approve).await;
    let first = user(&r, "transcribe", json!({})).await.unwrap();
    let job = job_done(&r, first["job_id"].as_str().unwrap()).await;
    assert_eq!(job["result"]["replaced"], false);
    assert_eq!(job["result"]["captions_cleared"], false);
    user(
        &r,
        "set_captions",
        json!({"lines": [{"start": 5, "end": 6, "text": "My edit"}]}),
    )
    .await
    .unwrap();
    assert_eq!(
        user(&r, "get_transcript", json!({})).await.unwrap()["captions_source"],
        "edited"
    );
    let made_before = user(&r, "get_transcript", json!({})).await.unwrap()["made_at"].clone();
    assert!(made_before.as_u64().is_some(), "made_at is Unix seconds");

    let mut rx = r.daemon.subscribe();
    let again = user(&r, "transcribe", json!({"language": "en"}))
        .await
        .unwrap();
    assert_ne!(
        first["job_id"], again["job_id"],
        "a finished job is not reused"
    );
    let job = job_done(&r, again["job_id"].as_str().unwrap()).await;
    assert_eq!(job["state"], "done", "{job}");
    assert_eq!(job["result"]["replaced"], true);
    assert_eq!(job["result"]["captions_cleared"], true);
    let events = drain(&mut rx);
    assert!(events.iter().any(|e| e.name == "transcript.ready"
        && e.data["replaced"] == true
        && e.data["captions_cleared"] == true));
    assert!(events
        .iter()
        .any(|e| e.name == "capture.revised" && e.data["action"] == "set_captions"));
    // The edited lines are gone, the generated ones are back.
    assert!(manifest(&r.dir)["captions"].get("lines").is_none());
    let t = user(&r, "get_transcript", json!({})).await.unwrap();
    assert_eq!(t["captions_source"], "generated");
    assert_eq!(r.ai.transcribe_calls().len(), 2);
    assert_eq!(r.ai.transcribe_calls()[1].language.as_deref(), Some("en"));
    // One undo step brings the edited lines back.
    user(&r, "undo", json!({})).await.unwrap();
    assert_eq!(
        manifest(&r.dir)["captions"]["lines"][0]["text"],
        "My edit",
        "undo restores the discarded edit"
    );
    // A recording without edited lines is not given a pointless revision.
    user(&r, "set_captions", json!({"lines": []}))
        .await
        .unwrap();
    let revs = manifest(&r.dir)["revisions"].as_array().unwrap().len();
    let third = user(&r, "transcribe", json!({})).await.unwrap();
    let job = job_done(&r, third["job_id"].as_str().unwrap()).await;
    assert_eq!(job["result"]["captions_cleared"], false);
    assert_eq!(
        manifest(&r.dir)["revisions"].as_array().unwrap().len(),
        revs
    );
}

/// Re-generating can choose the model for the run; a failed re-generate leaves the previous
/// transcript, its caption edits and the file system exactly as they were (no temp files).
#[tokio::test]
async fn ai_1_a_failed_regenerate_keeps_the_previous_transcript_and_edits() {
    let ai = FakeAi::new().with_small_installed();
    let r = rig(ai, Decision::Approve).await;
    let first = user(&r, "transcribe", json!({})).await.unwrap();
    job_done(&r, first["job_id"].as_str().unwrap()).await;
    user(
        &r,
        "set_captions",
        json!({"lines": [{"start": 5, "end": 6, "text": "My edit"}]}),
    )
    .await
    .unwrap();
    let before = std::fs::read(r.dir.join("transcript.json")).unwrap();
    let manifest_before = manifest(&r.dir);

    r.ai.fail_transcribe_with(cm_core::CmError::invalid_input("The audio is silent"));
    let again = user(&r, "transcribe", json!({})).await.unwrap();
    let job = job_done(&r, again["job_id"].as_str().unwrap()).await;
    assert_eq!(job["state"], "failed", "{job}");
    assert_eq!(
        std::fs::read(r.dir.join("transcript.json")).unwrap(),
        before
    );
    assert_eq!(
        manifest(&r.dir),
        manifest_before,
        "edits and history untouched"
    );
    let leftovers: Vec<String> = std::fs::read_dir(&r.dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains("transcript") && n != "transcript.json")
        .collect();
    assert!(leftovers.is_empty(), "no temp files: {leftovers:?}");
}

/// A cancelled re-generate does the same: the previous transcript stays.
#[tokio::test]
async fn ai_1_a_cancelled_regenerate_keeps_the_previous_transcript() {
    let ai = FakeAi::new().with_small_installed();
    let r = rig(ai, Decision::Approve).await;
    let first = user(&r, "transcribe", json!({})).await.unwrap();
    job_done(&r, first["job_id"].as_str().unwrap()).await;
    let before = std::fs::read(r.dir.join("transcript.json")).unwrap();
    // The fake's runs become slow, so the second one can be cancelled part-way.
    r.ai.set_slow(500, 10);
    let again = user(&r, "transcribe", json!({})).await.unwrap();
    let id = again["job_id"].as_str().unwrap().to_owned();
    until(async || {
        let j = user(&r, "get_job", json!({"job_id": id})).await.unwrap();
        (j["progress"].as_f64().unwrap_or(0.0) > 0.0).then_some(())
    })
    .await;
    user(&r, "cancel_job", json!({"job_id": id})).await.unwrap();
    assert_eq!(job_done(&r, &id).await["state"], "cancelled");
    assert_eq!(
        std::fs::read(r.dir.join("transcript.json")).unwrap(),
        before
    );
}

fn word(text: &str, a: f64, b: f64, conf: f32) -> cm_ai::transcribe::Word {
    cm_ai::transcribe::Word {
        text: text.into(),
        start: MediaTime::from_secs_f64(a).unwrap(),
        end: MediaTime::from_secs_f64(b).unwrap(),
        confidence: conf,
    }
}

fn six_words(third: &str) -> Vec<cm_ai::transcribe::Word> {
    vec![
        word("Open", 1.0, 1.4, 0.9),
        word("Logo", 1.5, 1.7, 0.4),
        word(third, 1.8, 2.3, 0.9),
        word("then", 2.4, 2.5, 0.9),
        word("save", 2.6, 2.7, 0.9),
        word("it", 2.8, 3.4, 0.9),
    ]
}

/// AI-6a: the glossary is an app-only setting (agents can read it, not write it); the job passes
/// it to the transcriber (it primes Whisper) and replaces the heard-as spellings in the stored
/// words, with the multi-word variant merged into one word.
#[tokio::test]
async fn ai_6a_glossary_is_app_only_primes_the_run_and_fixes_the_words() {
    let r = rig(FakeAi::new().with_small_installed(), Decision::Approve).await;
    r.ai.set_words(six_words("Drive"));
    let g = json!([{"term": "Google Drive", "heard_as": ["logo drive"]}, {"term": "CaptureMann"}]);
    let e = r
        .daemon
        .execute_method(
            agent("claude"),
            0,
            "set_settings",
            &json!({"values": {"ai.glossary": g}}),
        )
        .await
        .unwrap_err();
    assert_eq!(reason(&e), "app_only_setting");
    r.daemon
        .execute_method(
            Actor::User,
            0,
            "set_settings",
            &json!({"values": {"ai.glossary": g}}),
        )
        .await
        .unwrap();
    // An agent can read it.
    let got = r
        .daemon
        .execute_method(agent("claude"), 0, "get_settings", &json!({}))
        .await
        .unwrap();
    assert_eq!(got["settings"]["ai.glossary"][0]["term"], "Google Drive");

    let t = user(&r, "transcribe", json!({})).await.unwrap();
    let job = job_done(&r, t["job_id"].as_str().unwrap()).await;
    assert_eq!(job["state"], "done", "{job}");
    assert_eq!(job["result"]["glossary_replaced"], 1);
    let call = &r.ai.transcribe_calls()[0];
    assert_eq!(call.glossary.len(), 2);
    assert_eq!(call.glossary[1].term, "CaptureMann");
    let words = user(&r, "get_transcript", json!({})).await.unwrap()["transcript"]["words"].clone();
    let texts: Vec<&str> = words
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["text"].as_str().unwrap())
        .collect();
    assert_eq!(texts, vec!["Open", "Google Drive", "then", "save", "it"]);
    // Without a glossary nothing changes.
    r.daemon
        .execute_method(
            Actor::User,
            0,
            "set_settings",
            &json!({"values": {"ai.glossary": []}}),
        )
        .await
        .unwrap();
    let t = user(&r, "transcribe", json!({})).await.unwrap();
    let job = job_done(&r, t["job_id"].as_str().unwrap()).await;
    assert_eq!(job["result"]["glossary_replaced"], 0);
    assert_eq!(
        user(&r, "get_transcript", json!({})).await.unwrap()["transcript"]["words"]
            .as_array()
            .unwrap()
            .len(),
        6
    );
}

/// AI-6a: a re-generate keeps the caption edits where the same words are heard again and counts
/// the ones it cannot place; one undo step brings back the previous captions.
#[tokio::test]
async fn ai_6a_regenerate_keeps_edits_on_the_same_words_and_counts_the_rest() {
    let r = rig(FakeAi::new().with_small_installed(), Decision::Approve).await;
    r.ai.set_words(six_words("Drive"));
    let first = user(&r, "transcribe", json!({})).await.unwrap();
    job_done(&r, first["job_id"].as_str().unwrap()).await;
    let lines = user(&r, "get_captions", json!({})).await.unwrap()["lines"].clone();
    assert_eq!(lines.as_array().unwrap().len(), 2);
    // Edit both lines.
    // (Times come back as ticks; `set_captions` takes seconds or "t:<ticks>".)
    let line = |i: usize, text: &str| {
        json!({"start": format!("t:{}", lines[i]["start"]), "end": format!("t:{}", lines[i]["end"]),
               "text": text})
    };
    let edited = json!([line(0, "Open Google Drive"), line(1, "Then save it!")]);
    user(&r, "set_captions", json!({"lines": edited}))
        .await
        .unwrap();
    // Heard again with line 2's words changed.
    r.ai.set_words(vec![
        word("Open", 1.0, 1.4, 0.9),
        word("Logo", 1.5, 1.7, 0.4),
        word("Drive", 1.8, 2.3, 0.9),
        word("then", 2.4, 2.5, 0.9),
        word("safe", 2.6, 2.7, 0.9),
        word("it", 2.8, 3.4, 0.9),
    ]);
    let revs = manifest(&r.dir)["revisions"].as_array().unwrap().len();
    let again = user(&r, "transcribe", json!({})).await.unwrap();
    let job = job_done(&r, again["job_id"].as_str().unwrap()).await;
    assert_eq!(job["result"]["reapplied"], 1);
    assert_eq!(job["result"]["dropped"], 1);
    assert_eq!(
        manifest(&r.dir)["revisions"].as_array().unwrap().len(),
        revs + 1,
        "one revision"
    );
    let now = user(&r, "get_captions", json!({})).await.unwrap();
    assert_eq!(now["source"], "edited");
    assert_eq!(now["lines"][0]["text"], "Open Google Drive");
    assert_eq!(now["lines"][1]["text"], "then safe it");
    // Undo brings the previous captions back in one step.
    user(&r, "undo", json!({})).await.unwrap();
    let back = user(&r, "get_captions", json!({})).await.unwrap();
    assert_eq!(back["lines"][1]["text"], "Then save it!");
}

/// AI-6a: `replace_in_captions` replaces across the lines (the generated ones before any edit),
/// counts replacements and lines, is one undo step, idempotent, Free for agents, leaves
/// `transcript.json` alone, and refuses a result that breaks the row rules.
#[tokio::test]
async fn ai_6a_replace_in_captions() {
    let r = rig(FakeAi::new().with_small_installed(), Decision::Approve).await;
    let e = user(
        &r,
        "replace_in_captions",
        json!({"replacements": [{"find": "a", "replace": "b"}]}),
    )
    .await
    .unwrap_err();
    assert_eq!(code(&e), ErrorCode::NotFound);
    r.ai.set_words(six_words("Drive"));
    let t = user(&r, "transcribe", json!({})).await.unwrap();
    job_done(&r, t["job_id"].as_str().unwrap()).await;
    let transcript_before = std::fs::read(r.dir.join("transcript.json")).unwrap();

    let p = json!({"replacements": [
        {"find": "logo drive", "replace": "Google Drive"},
        {"find": "SAVE", "replace": "keep", "match_case": true},
        {"find": "it", "replace": "IT", "whole_word": true},
    ], "request_id": "rep1"});
    let out = call(&r, agent("claude"), "replace_in_captions", p.clone())
        .await
        .unwrap();
    // "Open Logo Drive" -> 1; "save" does not match case; "it" is a whole word once.
    assert_eq!(out["replacements"], 2, "{out}");
    assert_eq!(out["lines_touched"], 2);
    assert_eq!(out["by"], "agent:claude");
    let lines = user(&r, "get_captions", json!({})).await.unwrap();
    assert_eq!(lines["source"], "edited");
    assert_eq!(lines["lines"][0]["text"], "Open Google Drive");
    assert_eq!(lines["lines"][1]["text"], "then save IT");
    // Same request id: no second revision.
    let revs = manifest(&r.dir)["revisions"].as_array().unwrap().len();
    let again = call(&r, agent("claude"), "replace_in_captions", p)
        .await
        .unwrap();
    assert_eq!(again["revision"], out["revision"]);
    assert_eq!(
        manifest(&r.dir)["revisions"].as_array().unwrap().len(),
        revs
    );
    // One undo step.
    user(&r, "undo", json!({})).await.unwrap();
    let lines = user(&r, "get_captions", json!({})).await.unwrap();
    assert_eq!(lines["source"], "generated");
    // Nothing found: no revision.
    let none = user(
        &r,
        "replace_in_captions",
        json!({"replacements": [{"find": "zzz", "replace": "y"}]}),
    )
    .await
    .unwrap();
    assert_eq!(none["replacements"], 0);
    assert_eq!(none["changed"], false);
    // A result over two rows of 42 characters is refused and changes nothing.
    let long = "word ".repeat(30);
    let e = user(
        &r,
        "replace_in_captions",
        json!({"replacements": [{"find": "save", "replace": long}]}),
    )
    .await
    .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
    assert_eq!(
        user(&r, "get_captions", json!({})).await.unwrap()["source"],
        "generated"
    );
    // Input checks.
    for bad in [
        json!({"replacements": []}),
        json!({"replacements": [{"find": "", "replace": "x"}]}),
        json!({"replacements": [{"find": "a"}]}),
        json!({"replacements": [{"find": "a", "replace": "b", "regex": true}]}),
    ] {
        let e = user(&r, "replace_in_captions", bad).await.unwrap_err();
        assert_eq!(code(&e), ErrorCode::InvalidInput);
    }
    assert_eq!(
        std::fs::read(r.dir.join("transcript.json")).unwrap(),
        transcript_before,
        "the original transcript is not modified"
    );
}
