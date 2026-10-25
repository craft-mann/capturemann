//! Tests of the YouTube provider against the in-process fake server (PUB-2.AC1 to AC4 as far as
//! a provider can meet them). "Tested with fakes": no network, no account.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::*;
use crate::fake_youtube::{Call, FakeTokens, FakeYouTube, Fault};

const KIB: u64 = 1024;
const CHUNK: u64 = 512 * KIB;
const SIZE: usize = 5 * 512 * 1024 + 1234; // five full chunks and a short last one

struct Rig {
    yt: Arc<FakeYouTube>,
    tokens: Arc<FakeTokens>,
    provider: YouTubeProvider,
    tmp: tempfile::TempDir,
}

fn rig_with(cfg: impl FnOnce(&mut YouTubeConfig)) -> Rig {
    let yt = Arc::new(FakeYouTube::new());
    let tokens = Arc::new(FakeTokens::new("tok-1"));
    rig_from(yt, tokens, cfg)
}

fn rig_from(
    yt: Arc<FakeYouTube>,
    tokens: Arc<FakeTokens>,
    cfg: impl FnOnce(&mut YouTubeConfig),
) -> Rig {
    let mut c = YouTubeConfig {
        chunk_size: CHUNK,
        backoff_base: Duration::from_millis(10),
        ..YouTubeConfig::default()
    };
    cfg(&mut c);
    let provider = YouTubeProvider::new(yt.clone(), tokens.clone(), c);
    Rig {
        yt,
        tokens,
        provider,
        tmp: tempfile::tempdir().unwrap(),
    }
}

fn rig() -> Rig {
    rig_with(|_| {})
}

fn make_file(r: &Rig, name: &str, size: usize) -> PathBuf {
    let path = r.tmp.path().join(name);
    let data: Vec<u8> = (0..size).map(|i| (i * 31 % 251) as u8).collect();
    std::fs::write(&path, data).unwrap();
    path
}

fn meta() -> PublishMeta {
    PublishMeta {
        title: "How the editor works".into(),
        description: "00:00 Intro\n01:00 Cuts".into(),
        tags: vec!["screen recording".into(), "tutorial".into()],
        made_for_kids: Some(false),
        ..PublishMeta::default()
    }
}

/// A progress sink that remembers the latest resume state and the byte counts.
fn tracked() -> (Progress, Arc<Mutex<Option<ResumeState>>>, Arc<AtomicU64>) {
    let saved = Arc::new(Mutex::new(None));
    let last = Arc::new(AtomicU64::new(0));
    let (s2, l2) = (saved.clone(), last.clone());
    let p = Progress::new(
        move |done, _| l2.store(done, Ordering::Relaxed),
        Arc::new(AtomicBool::new(false)),
    )
    .with_resume_saver(move |st| *s2.lock().unwrap() = Some(st.clone()));
    (p, saved, last)
}

fn code(e: &CmError) -> Option<&str> {
    match e {
        CmError::PlatformRejected { platform_code, .. } => platform_code.as_deref(),
        _ => None,
    }
}

// ---- PUB-2.AC1 -------------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn publishes_unlisted_with_the_sheet_fields() {
    let r = rig();
    let file = make_file(&r, "a.mp4", SIZE);
    let (p, saved, last) = tracked();
    let mut m = meta();
    m.privacy = Some(Privacy::Unlisted);
    let done = r.provider.publish(&file, &m, None, p).await.unwrap();

    let videos = r.yt.videos();
    assert_eq!(videos.len(), 1);
    let v = &videos[0];
    assert_eq!(v.data, std::fs::read(&file).unwrap());
    assert_eq!(v.title, "How the editor works");
    assert_eq!(v.description, "00:00 Intro\n01:00 Cuts");
    assert_eq!(v.tags, vec!["screen recording", "tutorial"]);
    assert_eq!(v.privacy, "unlisted");
    assert_eq!(v.category, "28", "the default category");
    assert_eq!(v.made_for_kids, Some(false));
    assert_eq!(v.publish_at, None);

    assert_eq!(done.remote_id, v.id);
    assert_eq!(
        done.url,
        format!("https://www.youtube.com/watch?v={}", v.id)
    );
    assert_eq!(done.privacy, Privacy::Unlisted);
    assert_eq!(done.status, PublishStatus::Processing);
    assert!(done.warnings.is_empty());
    assert_eq!(last.load(Ordering::Relaxed), SIZE as u64);

    // part=snippet,status, resumable, chunks in 256 KiB multiples except the last.
    assert_eq!(
        r.yt.init_params(),
        vec!["uploadType=resumable&part=snippet,status".to_owned()]
    );
    let chunks = r.yt.chunks();
    assert_eq!(chunks.len(), 6);
    assert!(chunks[..5].iter().all(|c| c.1 % (256 * KIB) == 0));
    assert!(r.yt.violations().is_empty());
    assert!(r.yt.unexpected().is_empty());

    // The state was saved after every chunk and ends with the video.
    let st: YtResume = serde_json::from_value(saved.lock().unwrap().clone().unwrap().0).unwrap();
    assert_eq!(st.video.unwrap().id, v.id);
    assert_eq!(st.offset, SIZE as u64);
}

#[tokio::test(start_paused = true)]
async fn the_platform_default_privacy_is_unlisted() {
    let r = rig();
    let file = make_file(&r, "a.mp4", 1000);
    let done = r
        .provider
        .publish(&file, &meta(), None, Progress::none())
        .await
        .unwrap();
    assert_eq!(r.yt.videos()[0].privacy, "unlisted");
    assert_eq!(done.privacy, Privacy::Unlisted);
    assert_eq!(r.provider.id(), "youtube");
    assert_eq!(r.provider.requirements().title_max_chars, 100);
}

#[tokio::test(start_paused = true)]
async fn a_scheduled_video_goes_up_private_with_publish_at() {
    let r = rig();
    let file = make_file(&r, "a.mp4", 1000);
    let mut m = meta();
    m.privacy = Some(Privacy::Private);
    m.scheduled_at_unix = Some(4_102_444_800); // 2100-01-01
    let done = r
        .provider
        .publish(&file, &m, None, Progress::none())
        .await
        .unwrap();
    assert_eq!(
        r.yt.videos()[0].publish_at.as_deref(),
        Some("2100-01-01T00:00:00Z")
    );
    assert_eq!(done.privacy, Privacy::Private);
    assert!(done.warnings.is_empty());
}

// ---- validation before the network (PUB-2.AC4) -----------------------------------------------

#[tokio::test(start_paused = true)]
async fn invalid_metadata_never_reaches_the_network() {
    let r = rig();
    let file = make_file(&r, "a.mp4", 1000);
    let mut m = meta();
    m.made_for_kids = None;
    let e = r
        .provider
        .publish(&file, &m, None, Progress::none())
        .await
        .unwrap_err();
    assert!(
        matches!(&e, CmError::InvalidInput { message } if message.starts_with("made_for_kids:")),
        "{e:?}"
    );
    let mut m = meta();
    m.title = "x".repeat(101);
    let e = r
        .provider
        .publish(&file, &m, None, Progress::none())
        .await
        .unwrap_err();
    assert!(matches!(&e, CmError::InvalidInput { message } if message.starts_with("title:")));
    assert!(r.yt.log().is_empty(), "no request was sent");
    assert_eq!(r.tokens.calls(), 0, "no token was asked for");
}

#[tokio::test(start_paused = true)]
async fn the_server_refusing_metadata_is_platform_rejected() {
    // Rules loosened on purpose so the client check lets it through.
    let mut loose = PlatformRules::youtube();
    loose.title_max_chars = 1000;
    let r = rig();
    let provider = YouTubeProvider::new(r.yt.clone(), r.tokens.clone(), YouTubeConfig::default())
        .with_rules(loose);
    let file = make_file(&r, "a.mp4", 1000);
    let mut m = meta();
    m.title = "x".repeat(150);
    let e = provider
        .publish(&file, &m, None, Progress::none())
        .await
        .unwrap_err();
    assert_eq!(code(&e), Some("invalidTitle"), "{e:?}");
    assert_eq!(r.yt.count(Call::Init), 1, "not retried");
}

// ---- PUB-2.AC3: resume -----------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn resumes_from_the_confirmed_byte_after_a_drop_at_40_percent() {
    let r = rig_with(|c| c.max_retries = 0);
    let file = make_file(&r, "a.mp4", SIZE);
    // 40 %: the chunk starting at 1 MiB never arrives (the connection is gone for good).
    r.yt.inject(Call::Chunk, 2 * CHUNK, Fault::DropBefore, 1);
    let (p, saved, _) = tracked();
    let e = r
        .provider
        .publish(&file, &meta(), None, p)
        .await
        .unwrap_err();
    assert!(matches!(e, CmError::Network { .. }), "{e:?}");
    assert!(r.yt.videos().is_empty());
    let starts: Vec<u64> = r.yt.chunks().iter().map(|c| c.0).collect();
    assert_eq!(starts, vec![0, CHUNK]);

    // The saved state says where it stopped.
    let state = saved.lock().unwrap().clone().unwrap();
    let st: YtResume = serde_json::from_value(state.0.clone()).unwrap();
    assert_eq!(st.offset, 2 * CHUNK);

    // The retry (a new provider call, as the job queue does) continues at that byte.
    let (p, _, last) = tracked();
    let done = r
        .provider
        .publish(&file, &meta(), Some(state), p)
        .await
        .unwrap();
    let starts: Vec<u64> = r.yt.chunks().iter().map(|c| c.0).collect();
    assert_eq!(
        starts,
        vec![0, CHUNK, 2 * CHUNK, 3 * CHUNK, 4 * CHUNK, 5 * CHUNK]
    );
    assert_eq!(r.yt.sessions_started(), 1, "same session");
    assert_eq!(r.yt.videos().len(), 1);
    assert_eq!(r.yt.videos()[0].data, std::fs::read(&file).unwrap());
    assert_eq!(done.remote_id, r.yt.videos()[0].id);
    assert_eq!(last.load(Ordering::Relaxed), SIZE as u64);
    assert!(r.yt.violations().is_empty());
}

#[tokio::test(start_paused = true)]
async fn a_lost_answer_is_resolved_by_a_status_query_not_a_resend() {
    let r = rig();
    let file = make_file(&r, "a.mp4", SIZE);
    // The chunk at 40 % lands but the answer is lost.
    r.yt.inject(Call::Chunk, 2 * CHUNK, Fault::DropAfter, 1);
    let done = r
        .provider
        .publish(&file, &meta(), None, Progress::none())
        .await
        .unwrap();
    let starts: Vec<u64> = r.yt.chunks().iter().map(|c| c.0).collect();
    assert_eq!(
        starts,
        vec![0, CHUNK, 2 * CHUNK, 3 * CHUNK, 4 * CHUNK, 5 * CHUNK]
    );
    assert!(r.yt.count(Call::StatusQuery) >= 1);
    assert_eq!(r.yt.videos().len(), 1);
    assert_eq!(done.remote_id, r.yt.videos()[0].id);
    assert!(r.yt.violations().is_empty());
}

#[tokio::test(start_paused = true)]
async fn an_expired_session_starts_a_new_one() {
    let r = rig_with(|c| c.max_retries = 0);
    let file = make_file(&r, "a.mp4", SIZE);
    r.yt.inject(Call::Chunk, 2 * CHUNK, Fault::DropBefore, 1);
    let (p, saved, _) = tracked();
    r.provider
        .publish(&file, &meta(), None, p)
        .await
        .unwrap_err();
    let state = saved.lock().unwrap().clone();
    r.yt.expire_sessions();
    let done = r
        .provider
        .publish(&file, &meta(), state, Progress::none())
        .await
        .unwrap();
    assert_eq!(r.yt.sessions_started(), 2);
    assert_eq!(r.yt.videos().len(), 1);
    assert_eq!(r.yt.videos()[0].data, std::fs::read(&file).unwrap());
    assert_eq!(done.remote_id, r.yt.videos()[0].id);
}

#[tokio::test(start_paused = true)]
async fn a_changed_file_or_title_does_not_resume() {
    let r = rig_with(|c| c.max_retries = 0);
    let file = make_file(&r, "a.mp4", SIZE);
    r.yt.inject(Call::Chunk, 2 * CHUNK, Fault::DropBefore, 1);
    let (p, saved, _) = tracked();
    r.provider
        .publish(&file, &meta(), None, p)
        .await
        .unwrap_err();
    let state = saved.lock().unwrap().clone();

    let mut m = meta();
    m.title = "Another title".into();
    r.provider
        .publish(&file, &m, state, Progress::none())
        .await
        .unwrap();
    assert_eq!(r.yt.sessions_started(), 2);
    assert_eq!(r.yt.videos()[0].title, "Another title");
}

#[tokio::test(start_paused = true)]
async fn a_finished_upload_in_the_state_is_not_uploaded_again() {
    let r = rig();
    let file = make_file(&r, "a.mp4", 3000);
    let (p, saved, _) = tracked();
    let first = r.provider.publish(&file, &meta(), None, p).await.unwrap();
    let state = saved.lock().unwrap().clone();
    let log_len = r.yt.log().len();
    let again = r
        .provider
        .publish(&file, &meta(), state, Progress::none())
        .await
        .unwrap();
    assert_eq!(again.remote_id, first.remote_id);
    assert_eq!(r.yt.videos().len(), 1);
    assert_eq!(r.yt.log().len(), log_len, "no request at all");
}

// ---- transient failures ----------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn server_errors_and_429_are_retried() {
    let r = rig();
    let file = make_file(&r, "a.mp4", SIZE);
    let five = Fault::Status {
        status: 503,
        retry_after: None,
        reason: "backendError",
        message: "Backend Error",
    };
    r.yt.inject(
        Call::Init,
        0,
        Fault::Status {
            status: 429,
            retry_after: Some(2),
            reason: "rateLimitExceeded",
            message: "slow down",
        },
        1,
    );
    r.yt.inject(Call::Chunk, CHUNK, five, 2);
    r.provider
        .publish(&file, &meta(), None, Progress::none())
        .await
        .unwrap();
    assert_eq!(r.yt.count(Call::Init), 2);
    assert_eq!(r.yt.videos().len(), 1);
    assert_eq!(r.yt.videos()[0].data, std::fs::read(&file).unwrap());
    assert!(r.yt.violations().is_empty());
}

#[tokio::test(start_paused = true)]
async fn a_long_retry_after_is_a_network_error_with_the_wait() {
    let r = rig();
    let file = make_file(&r, "a.mp4", 1000);
    r.yt.inject(
        Call::Init,
        0,
        Fault::Status {
            status: 429,
            retry_after: Some(3600),
            reason: "rateLimitExceeded",
            message: "slow down",
        },
        1,
    );
    let e = r
        .provider
        .publish(&file, &meta(), None, Progress::none())
        .await
        .unwrap_err();
    assert!(
        matches!(&e, CmError::Network { message } if message.contains("3600")),
        "{e:?}"
    );
    assert_eq!(r.yt.count(Call::Init), 1);
}

#[tokio::test(start_paused = true)]
async fn retries_run_out_as_network() {
    let r = rig_with(|c| c.max_retries = 2);
    let file = make_file(&r, "a.mp4", 1000);
    r.yt.inject(
        Call::Init,
        0,
        Fault::Status {
            status: 500,
            retry_after: None,
            reason: "backendError",
            message: "Backend Error",
        },
        100,
    );
    let e = r
        .provider
        .publish(&file, &meta(), None, Progress::none())
        .await
        .unwrap_err();
    assert!(matches!(e, CmError::Network { .. }));
    assert_eq!(r.yt.count(Call::Init), 3);
}

// ---- authentication --------------------------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn a_401_asks_for_a_refreshed_token_and_goes_on() {
    let yt = Arc::new(FakeYouTube::new());
    yt.set_valid_token("tok-2");
    let tokens = Arc::new(FakeTokens::new("tok-1"));
    tokens.refresh_to("tok-2");
    let r = rig_from(yt, tokens, |_| {});
    let file = make_file(&r, "a.mp4", SIZE);
    r.provider
        .publish(&file, &meta(), None, Progress::none())
        .await
        .unwrap();
    assert_eq!(r.yt.videos().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn a_revoked_token_is_auth_expired() {
    // The server rejects the token and the refresh fails.
    let yt = Arc::new(FakeYouTube::new());
    yt.set_valid_token("other");
    let tokens = Arc::new(FakeTokens::new("tok-1"));
    tokens.revoke_on_reject();
    let r = rig_from(yt, tokens, |_| {});
    let file = make_file(&r, "a.mp4", 1000);
    let e = r
        .provider
        .publish(&file, &meta(), None, Progress::none())
        .await
        .unwrap_err();
    assert!(matches!(e, CmError::AuthExpired { .. }), "{e:?}");

    // The source is revoked from the start.
    let r2 = rig_from(
        Arc::new(FakeYouTube::new()),
        Arc::new(FakeTokens::revoked()),
        |_| {},
    );
    let e = r2
        .provider
        .publish(&file, &meta(), None, Progress::none())
        .await
        .unwrap_err();
    assert!(matches!(e, CmError::AuthExpired { .. }));
    assert!(r2.yt.log().is_empty());

    // The source cannot produce another token.
    let yt = Arc::new(FakeYouTube::new());
    yt.set_valid_token("other");
    let r3 = rig_from(yt, Arc::new(FakeTokens::new("tok-1")), |_| {});
    let e = r3
        .provider
        .publish(&file, &meta(), None, Progress::none())
        .await
        .unwrap_err();
    assert!(matches!(e, CmError::AuthExpired { .. }));
}

// ---- quota and the unverified project --------------------------------------------------------

#[tokio::test(start_paused = true)]
async fn quota_exhausted_is_platform_rejected_with_the_code() {
    for reason in ["quotaExceeded", "uploadLimitExceeded"] {
        let r = rig();
        let file = make_file(&r, "a.mp4", 1000);
        r.yt.inject(
            Call::Init,
            0,
            Fault::Status {
                status: 403,
                retry_after: None,
                reason,
                message: "The request cannot be completed because you have exceeded your quota.",
            },
            10,
        );
        let e = r
            .provider
            .publish(&file, &meta(), None, Progress::none())
            .await
            .unwrap_err();
        assert_eq!(code(&e), Some(reason), "{e:?}");
        assert!(e.to_string().contains("Upload limit reached for today"));
        assert_eq!(r.yt.count(Call::Init), 1, "a quota error is not retried");
    }
}

#[tokio::test(start_paused = true)]
async fn an_unverified_project_locks_to_private_and_is_reported_not_failed() {
    let r = rig();
    r.yt.lock_to_private();
    let file = make_file(&r, "a.mp4", 5000);
    let mut m = meta();
    m.privacy = Some(Privacy::Public);
    let done = r
        .provider
        .publish(&file, &m, None, Progress::none())
        .await
        .unwrap();
    assert_eq!(done.status, PublishStatus::PrivacyLocked);
    assert_eq!(done.privacy, Privacy::Private);
    assert!(done
        .warnings
        .iter()
        .any(|w| w.contains("kept this video private")));
    let v = &r.yt.videos()[0];
    assert_eq!(
        v.requested_privacy, "public",
        "the provider asked for what was passed"
    );
    assert_eq!(v.privacy, "private");
    // It did not try to fix it: no call other than the upload ones.
    assert!(r.yt.unexpected().is_empty());
    assert_eq!(r.yt.count(Call::Init) + r.yt.count(Call::Chunk), 2);

    // Asking for private under the lock is not a lock.
    let r = rig();
    r.yt.lock_to_private();
    let mut m = meta();
    m.privacy = Some(Privacy::Private);
    let done = r
        .provider
        .publish(&file, &m, None, Progress::none())
        .await
        .unwrap();
    assert_eq!(done.status, PublishStatus::Processing);
    assert!(done.warnings.is_empty());
}

#[tokio::test(start_paused = true)]
async fn the_provider_never_calls_anything_that_changes_privacy() {
    // Everything the provider can do in one run, with every extra step: the only routes it uses
    // are the upload, thumbnail, playlist and caption ones. There is no `videos.update`.
    let r = rig();
    let file = make_file(&r, "a.mp4", 5000);
    let thumb = r.tmp.path().join("t.png");
    std::fs::write(&thumb, b"png").unwrap();
    let srt = r.tmp.path().join("c.srt");
    std::fs::write(&srt, "1\n00:00:00,000 --> 00:00:01,000\nHi\n").unwrap();
    let mut m = meta();
    m.privacy = Some(Privacy::Unlisted);
    m.thumbnail = Some(thumb);
    m.captions = Some(srt);
    m.playlist_id = Some("PL123".into());
    r.provider
        .publish(&file, &m, None, Progress::none())
        .await
        .unwrap();
    let log = r.yt.log();
    for line in &log {
        assert!(
            matches!(
                line.as_str(),
                "POST /upload/youtube/v3/videos"
                    | "POST /upload/youtube/v3/thumbnails/set"
                    | "POST /upload/youtube/v3/captions"
                    | "POST /youtube/v3/playlistItems"
            ) || line.starts_with("PUT /session/"),
            "unexpected call {line}"
        );
    }
    assert!(r.yt.unexpected().is_empty());
    assert_eq!(r.yt.videos()[0].privacy, "unlisted");
}

// ---- thumbnail, playlist, captions (PUB-2.AC2) -----------------------------------------------

#[tokio::test(start_paused = true)]
async fn sets_the_thumbnail_playlist_and_captions() {
    let r = rig();
    let file = make_file(&r, "a.mp4", 5000);
    let thumb = r.tmp.path().join("t.jpg");
    std::fs::write(&thumb, b"jpegdata").unwrap();
    let srt = r.tmp.path().join("c.srt");
    std::fs::write(&srt, "1\n00:00:00,000 --> 00:00:01,000\nHello\n").unwrap();
    let mut m = meta();
    m.thumbnail = Some(thumb);
    m.captions = Some(srt.clone());
    m.captions_language = Some("pt-BR".into());
    m.playlist_id = Some("PLabc_1".into());
    let done = r
        .provider
        .publish(&file, &m, None, Progress::none())
        .await
        .unwrap();
    assert!(done.warnings.is_empty());
    let v = &r.yt.videos()[0];
    assert_eq!(v.thumbnail.as_deref(), Some(&b"jpegdata"[..]));
    assert_eq!(v.playlists, vec!["PLabc_1"]);
    assert_eq!(
        v.captions,
        vec![("pt-BR".to_owned(), std::fs::read(&srt).unwrap())]
    );
}

#[tokio::test(start_paused = true)]
async fn a_thumbnail_on_an_unverified_channel_is_a_warning_not_a_failure() {
    let r = rig();
    r.yt.thumbnails_need_verified_account();
    let file = make_file(&r, "a.mp4", 5000);
    let thumb = r.tmp.path().join("t.png");
    std::fs::write(&thumb, b"png").unwrap();
    let mut m = meta();
    m.thumbnail = Some(thumb);
    let done = r
        .provider
        .publish(&file, &m, None, Progress::none())
        .await
        .unwrap();
    assert_eq!(
        done.warnings,
        vec!["Thumbnail needs a verified YouTube account"]
    );
    assert_eq!(r.yt.videos().len(), 1);
    assert_eq!(
        done.url,
        format!("https://www.youtube.com/watch?v={}", done.remote_id)
    );
}

#[tokio::test(start_paused = true)]
async fn caption_and_playlist_refusals_are_warnings() {
    let r = rig();
    let file = make_file(&r, "a.mp4", 5000);
    let srt = r.tmp.path().join("c.srt");
    std::fs::write(&srt, "x").unwrap();
    r.yt.inject(
        Call::Captions,
        0,
        Fault::Status {
            status: 400,
            retry_after: None,
            reason: "invalidMetadata",
            message: "bad captions",
        },
        1,
    );
    r.yt.inject(
        Call::Playlist,
        0,
        Fault::Status {
            status: 404,
            retry_after: None,
            reason: "playlistNotFound",
            message: "no such playlist",
        },
        1,
    );
    let mut m = meta();
    m.captions = Some(srt);
    m.playlist_id = Some("PLgone".into());
    let done = r
        .provider
        .publish(&file, &m, None, Progress::none())
        .await
        .unwrap();
    assert_eq!(done.warnings.len(), 2, "{:?}", done.warnings);
    assert!(done.warnings[0].contains("playlist"));
    assert!(done.warnings[1].contains("captions"));
}

#[tokio::test(start_paused = true)]
async fn a_transient_failure_after_the_upload_resumes_without_a_second_video() {
    let r = rig_with(|c| c.max_retries = 1);
    let file = make_file(&r, "a.mp4", 5000);
    let thumb = r.tmp.path().join("t.jpg");
    std::fs::write(&thumb, b"jpeg").unwrap();
    r.yt.inject(
        Call::Thumbnail,
        0,
        Fault::Status {
            status: 503,
            retry_after: None,
            reason: "backendError",
            message: "down",
        },
        2,
    );
    let mut m = meta();
    m.thumbnail = Some(thumb);
    let (p, saved, _) = tracked();
    let e = r.provider.publish(&file, &m, None, p).await.unwrap_err();
    assert!(matches!(e, CmError::Network { .. }));
    assert_eq!(r.yt.videos().len(), 1, "the video is up");
    let state = saved.lock().unwrap().clone();

    let done = r
        .provider
        .publish(&file, &m, state, Progress::none())
        .await
        .unwrap();
    assert_eq!(r.yt.videos().len(), 1, "no second video");
    assert_eq!(r.yt.sessions_started(), 1);
    assert_eq!(r.yt.videos()[0].thumbnail.as_deref(), Some(&b"jpeg"[..]));
    assert_eq!(done.remote_id, r.yt.videos()[0].id);
}

#[tokio::test(start_paused = true)]
async fn cancelling_stops_the_upload() {
    let r = rig();
    let file = make_file(&r, "a.mp4", SIZE);
    let flag = Arc::new(AtomicBool::new(false));
    let f2 = flag.clone();
    let p = Progress::new(
        move |done, _| {
            if done >= CHUNK {
                f2.store(true, Ordering::Relaxed);
            }
        },
        flag,
    );
    let e = r
        .provider
        .publish(&file, &meta(), None, p)
        .await
        .unwrap_err();
    assert!(e.to_string().contains("cancelled"));
    assert!(r.yt.videos().is_empty());
    assert!(r.yt.chunks().len() < 6);
}

// ---- deleting a video (library delete with remote copies) ---------------------------------

async fn publish_one(r: &Rig, name: &str, privacy: Privacy) -> Published {
    let file = make_file(r, name, 100 * 1024);
    let mut m = meta();
    m.privacy = Some(privacy);
    r.provider
        .publish(&file, &m, None, Progress::none())
        .await
        .unwrap()
}

#[tokio::test(start_paused = true)]
async fn delete_video_removes_only_that_video_and_changes_nothing_else() {
    let r = rig();
    let a = publish_one(&r, "a.mp4", Privacy::Unlisted).await;
    let b = publish_one(&r, "b.mp4", Privacy::Private).await;
    r.provider.delete_video(&a.remote_id).await.unwrap();
    let left = r.yt.videos();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].id, b.remote_id);
    // The other video keeps the privacy it had: nothing else was sent.
    assert_eq!(left[0].privacy, "private");
    let log = r.yt.log();
    let after_uploads: Vec<&String> = log.iter().filter(|l| l.starts_with("DELETE")).collect();
    assert_eq!(after_uploads, ["DELETE /youtube/v3/videos"]);
    assert_eq!(r.yt.count(Call::Delete), 1);
    assert!(r.yt.unexpected().is_empty());
}

#[tokio::test(start_paused = true)]
async fn a_video_that_is_already_gone_counts_as_deleted() {
    let r = rig();
    let a = publish_one(&r, "a.mp4", Privacy::Unlisted).await;
    r.provider.delete_video(&a.remote_id).await.unwrap();
    r.provider.delete_video(&a.remote_id).await.unwrap();
    assert!(r.yt.videos().is_empty());
}

#[tokio::test(start_paused = true)]
async fn an_empty_id_is_refused_before_any_request() {
    let r = rig();
    let e = r.provider.delete_video("").await.unwrap_err();
    assert!(matches!(e, CmError::InvalidInput { .. }), "{e:?}");
    assert_eq!(r.yt.count(Call::Delete), 0);
}

#[tokio::test(start_paused = true)]
async fn delete_errors_keep_their_codes_and_the_video_stays() {
    let r = rig();
    let a = publish_one(&r, "a.mp4", Privacy::Unlisted).await;

    // The sign-in is revoked: auth_expired.
    r.yt.set_valid_token("other");
    r.tokens.revoke_on_reject();
    let e = r.provider.delete_video(&a.remote_id).await.unwrap_err();
    assert!(matches!(e, CmError::AuthExpired { .. }), "{e:?}");
    assert_eq!(r.yt.videos().len(), 1);

    // The network keeps failing: network.
    let r2 = rig_with(|c| c.max_retries = 1);
    let b = publish_one(&r2, "b.mp4", Privacy::Unlisted).await;
    r2.yt.inject(Call::Delete, 0, Fault::DropBefore, 5);
    let e = r2.provider.delete_video(&b.remote_id).await.unwrap_err();
    assert!(matches!(e, CmError::Network { .. }), "{e:?}");
    assert_eq!(r2.yt.videos().len(), 1);

    // YouTube refuses (not the owner): platform_rejected with its reason.
    let r3 = rig();
    let c = publish_one(&r3, "c.mp4", Privacy::Unlisted).await;
    r3.yt.inject(
        Call::Delete,
        0,
        Fault::Status {
            status: 403,
            retry_after: None,
            reason: "forbidden",
            message: "The video cannot be deleted by this account.",
        },
        1,
    );
    let e = r3.provider.delete_video(&c.remote_id).await.unwrap_err();
    assert_eq!(code(&e), Some("forbidden"), "{e:?}");
    assert_eq!(r3.yt.videos().len(), 1);
}
