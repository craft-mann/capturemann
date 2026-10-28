//! What the recording details page relies on, against the real in-process daemon with a scripted
//! speech engine: the summary of a stored transcript (read again after the page is closed and
//! opened), the "edited captions" flag, the TXT download and the re-generate that replaces the
//! transcript. The client's answers are parsed from the daemon's real JSON, so a wrapper the
//! scripted transports do not have cannot hide a bug again.

use std::sync::Arc;

use cm_capture::fake::FakeBackend;
use cm_capture::Capturer;
use cm_core::{Actor, MediaTime, TrackInfo, TrackKind, TrackStats};
use cm_daemon::testing::{FakeAi, FixedConfirmer};
use cm_daemon::{Daemon, Decision};
use cm_library::rec::{RecBundle, SessionInfo};
use cm_library::Library;
use serde_json::{json, Value};

use super::client::{AiClient, CaptionFormat, DaemonTransport};
use cm_ai::caption::CaptionLine;

struct Rig {
    daemon: Daemon,
    client: AiClient,
    id: String,
    dir: std::path::PathBuf,
    _tmp: tempfile::TempDir,
}

async fn rig() -> Rig {
    let tmp = tempfile::tempdir().unwrap();
    let lib_dir = tmp.path().join("lib");
    let lib = Library::open_or_create(&lib_dir).unwrap();
    let queue = cm_daemon::library_job_queue(&lib_dir).unwrap();
    let daemon = Daemon::builder(
        Capturer::new(FakeBackend::new().with_display(1, 0, 0, 40, 30, 1.0)).with_own_pid(None),
        lib,
    )
    .jobs(queue)
    .confirmer(Arc::new(FixedConfirmer::new(Decision::Approve)))
    .ai(FakeAi::new().with_small_installed().services())
    .build();
    daemon.register_ai_handlers().unwrap();
    daemon.start_jobs().unwrap();
    let session = SessionInfo {
        clock_origin_ns: 1,
        timebase: "uptime_raw_ns".into(),
        displays: vec![],
        devices: json!({}),
        app_version: "0.0.1".into(),
        extra: Default::default(),
    };
    let now = time::OffsetDateTime::UNIX_EPOCH;
    let b = RecBundle::create(&lib_dir, "Lesson", now, json!({}), &session).unwrap();
    let dir = b.dir().to_path_buf();
    std::fs::write(dir.join("screen.mp4"), b"master").unwrap();
    std::fs::write(dir.join("mic.wav"), b"audio").unwrap();
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
    daemon
        .execute_method(Actor::User, 0, "rescan_library", &json!({}))
        .await
        .unwrap();
    let client = AiClient::new(Arc::new(DaemonTransport {
        daemon: daemon.clone(),
        rt: tokio::runtime::Handle::current(),
    }));
    Rig {
        daemon,
        client,
        id: m.id,
        dir,
        _tmp: tmp,
    }
}

impl Rig {
    /// Starts a transcription like the page does and waits for the job.
    async fn transcribe(&self, language: Option<&str>, model: Option<&str>) -> Value {
        let job = self
            .client
            .transcribe(&self.id, language, model, false)
            .await
            .unwrap();
        for _ in 0..400 {
            let j = self
                .daemon
                .execute_method(Actor::User, 0, "get_job", &json!({"job_id": job}))
                .await
                .unwrap();
            if matches!(j["state"].as_str(), Some("done" | "failed" | "cancelled")) {
                return j;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        panic!("the transcription job did not finish");
    }
}

/// The reported bug: a recording that has a transcript shows it again whenever the page asks,
/// with what the page prints (language, model, words, when) and whether captions were edited.
#[tokio::test]
async fn the_details_summary_comes_from_the_real_daemon_answer() {
    let r = rig().await;
    assert_eq!(r.client.transcript_info(&r.id).await.unwrap(), None);
    let job = r.transcribe(None, None).await;
    assert_eq!(job["state"], "done", "{job}");
    let info = r.client.transcript_info(&r.id).await.unwrap().unwrap();
    assert_eq!(info.language, "en");
    assert_eq!(info.model, "small");
    assert_eq!(info.words, 6);
    assert!(info.made_at.is_some_and(|t| t > 1_700_000_000), "{info:?}");
    assert!(!info.edited);
    // Asking again (the page was closed and opened) gives the same answer.
    assert_eq!(
        r.client.transcript_info(&r.id).await.unwrap().unwrap(),
        info
    );
    // The summary line the page prints.
    let line = crate::ai::summary::summary_line(&info, info.made_at.unwrap() + 90);
    assert_eq!(
        line,
        "English \u{00B7} small \u{00B7} 6 words \u{00B7} 2 min ago"
    );
}

/// Editing a caption line marks the transcript as edited (the Re-generate warning), TXT is
/// downloadable plain or with timestamps, and re-generating replaces the transcript and clears the
/// edit flag.
#[tokio::test]
async fn edit_flag_txt_download_and_regenerate_through_the_client() {
    let r = rig().await;
    r.transcribe(None, None).await;
    // TXT before any edit: generated lines, plain.
    let path = r
        .client
        .export_captions(&r.id, CaptionFormat::Txt, false)
        .await
        .unwrap();
    assert_eq!(path.file_name().unwrap(), "Lesson.txt");
    let plain = std::fs::read_to_string(&path).unwrap();
    assert!(plain.starts_with("Hello and welcome"), "{plain}");
    assert!(!plain.contains('['));
    let timed = r
        .client
        .export_captions(&r.id, CaptionFormat::Txt, true)
        .await
        .unwrap();
    assert!(std::fs::read_to_string(timed)
        .unwrap()
        .starts_with("[00:01] Hello and welcome"));
    // An edited line: the flag turns on and the TXT uses it.
    r.client
        .set_captions(
            &r.id,
            &[CaptionLine {
                start: MediaTime::from_secs(5),
                end: MediaTime::from_secs(6),
                text: "My edit".into(),
            }],
        )
        .await
        .unwrap();
    assert!(
        r.client
            .transcript_info(&r.id)
            .await
            .unwrap()
            .unwrap()
            .edited
    );
    let edited = r
        .client
        .export_captions(&r.id, CaptionFormat::Txt, false)
        .await
        .unwrap();
    assert_eq!(std::fs::read_to_string(edited).unwrap(), "My edit\n");
    // Re-generate with a chosen model and language: replaced, edits discarded.
    let job = r.transcribe(Some("en"), Some("small")).await;
    assert_eq!(job["state"], "done", "{job}");
    assert_eq!(job["result"]["replaced"], true);
    assert_eq!(job["result"]["captions_cleared"], true);
    assert!(
        !r.client
            .transcript_info(&r.id)
            .await
            .unwrap()
            .unwrap()
            .edited
    );
    assert!(r.dir.join("transcript.json").is_file());
}
