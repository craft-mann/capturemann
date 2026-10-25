//! Tests of the OneDrive provider against the in-process fake Graph server (STO-2.AC2, AC3, AC5,
//! AC6, STO-4). "Tested with fakes": no network, no account.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::*;
use crate::fake_drive::{FakeTokens, Fault};
use crate::fake_onedrive::{Call, FakeOneDrive};

const KIB: u64 = 1024;
/// Two units of 320 KiB.
const CHUNK: u64 = 2 * CHUNK_UNIT;

struct Rig {
    graph: Arc<FakeOneDrive>,
    tokens: Arc<FakeTokens>,
    provider: OneDriveProvider,
    tmp: tempfile::TempDir,
}

fn rig_with(cfg: impl FnOnce(&mut OneDriveConfig)) -> Rig {
    let graph = Arc::new(FakeOneDrive::new());
    let tokens = Arc::new(FakeTokens::new("tok-1"));
    let mut c = OneDriveConfig {
        chunk_size: CHUNK,
        // Every file goes through an upload session unless a test says otherwise.
        simple_put_max: 0,
        retry: Retry {
            backoff_base: Duration::from_millis(10),
            ..Retry::default()
        },
        ..OneDriveConfig::default()
    };
    cfg(&mut c);
    let provider = OneDriveProvider::new(graph.clone(), tokens.clone(), c);
    Rig {
        graph,
        tokens,
        provider,
        tmp: tempfile::tempdir().unwrap(),
    }
}

fn rig() -> Rig {
    rig_with(|_| {})
}

fn no_retries(c: &mut OneDriveConfig) {
    c.retry.max_retries = 0;
}

fn make_file(r: &Rig, name: &str, size: usize) -> PathBuf {
    let path = r.tmp.path().join(name);
    let data: Vec<u8> = (0..size).map(|i| (i * 31 % 251) as u8).collect();
    std::fs::write(&path, data).unwrap();
    path
}

fn target() -> Target {
    Target {
        folder: "CaptureMann/2026/10/Lesson".into(),
        file_name: "Lesson.mp4".into(),
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

fn starts(r: &Rig) -> Vec<u64> {
    r.graph.chunks().iter().map(|c| c.0).collect()
}

const SIZE: usize = 5 * 2 * 320 * 1024 + 1234; // five full chunks and a short last one

fn status_fault(status: u16, retry_after: Option<u64>, reason: &'static str) -> Fault {
    Fault::Status {
        status,
        retry_after,
        reason,
        message: "scripted failure",
    }
}

#[tokio::test(start_paused = true)]
async fn uploads_in_chunks_and_makes_an_anonymous_link() {
    let r = rig();
    let file = make_file(&r, "a.mp4", SIZE);
    let (p, saved, last) = tracked();
    let up = r.provider.upload(&file, &target(), None, p).await.unwrap();

    // AC2: the remote file is the local file, byte for byte, in the right folder.
    let files = r.graph.files();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].data, std::fs::read(&file).unwrap());
    assert_eq!(up.bytes, SIZE as u64);
    assert_eq!(
        r.graph.path_of(&files[0].parent),
        "CaptureMann/2026/10/Lesson"
    );
    assert_eq!(last.load(Ordering::Relaxed), SIZE as u64);

    // AC6: every chunk but the last is a multiple of 327,680 bytes.
    let chunks = r.graph.chunks();
    assert_eq!(chunks.len(), 6);
    assert!(chunks[..5].iter().all(|c| c.1 % 327_680 == 0));
    assert_eq!(chunks[5].1, 1234);
    assert!(
        r.graph.violations().is_empty(),
        "{:?}",
        r.graph.violations()
    );

    // STO-4: createLink view/anonymous.
    assert_eq!(
        r.graph.links(),
        vec![(files[0].id.clone(), "anonymous".into())]
    );
    assert_eq!(
        up.share_link.as_deref(),
        Some(format!("https://1drv.fake/anonymous/{}", files[0].id).as_str())
    );
    assert!(up.note.is_none());
    assert_eq!(up.remote_id, files[0].id);

    // The resume state was saved after every chunk and holds the last confirmed offset.
    let st: OneDriveResume =
        serde_json::from_value(saved.lock().unwrap().clone().unwrap().0).unwrap();
    assert_eq!(st.offset, 5 * CHUNK);
    assert_eq!(st.size, SIZE as u64);
}

#[tokio::test(start_paused = true)]
async fn the_default_chunk_is_10_mib_and_a_multiple_of_320_kib() {
    // AC6 with the real default.
    assert_eq!(OneDriveConfig::default().chunk_size, 10 * 1024 * KIB);
    let r = rig_with(|c| c.chunk_size = OneDriveConfig::default().chunk_size);
    let size = 21 * 1024 * 1024;
    let file = make_file(&r, "big.mp4", size);
    r.provider
        .upload(&file, &target(), None, Progress::none())
        .await
        .unwrap();
    let lens: Vec<u64> = r.graph.chunks().iter().map(|c| c.1).collect();
    assert_eq!(lens, vec![10 << 20, 10 << 20, 1 << 20]);
    assert!(lens[..2].iter().all(|l| l % 327_680 == 0));
    assert!(r.graph.violations().is_empty());
    assert_eq!(r.graph.files()[0].data.len(), size);
}

#[test]
fn chunk_size_is_rounded_to_320_kib_and_kept_under_60_mib() {
    let p = |chunk_size| {
        OneDriveProvider::new(
            Arc::new(FakeOneDrive::new()),
            Arc::new(FakeTokens::new("t")),
            OneDriveConfig {
                chunk_size,
                ..OneDriveConfig::default()
            },
        )
        .cfg
        .chunk_size
    };
    assert_eq!(p(1_000_000), 3 * CHUNK_UNIT);
    assert_eq!(p(1), CHUNK_UNIT);
    let big = p(500 * 1024 * KIB);
    assert_eq!(big % CHUNK_UNIT, 0);
    assert!(big < 60 * 1024 * KIB);
    assert_eq!(big, 191 * CHUNK_UNIT);
}

#[tokio::test(start_paused = true)]
async fn small_files_use_the_simple_put() {
    let r = rig_with(|c| c.simple_put_max = SIMPLE_PUT_MAX);
    let file = make_file(&r, "s.png", 100_000);
    let up = r
        .provider
        .upload(&file, &target(), None, Progress::none())
        .await
        .unwrap();
    assert_eq!(r.graph.count(Call::SimplePut), 1);
    assert_eq!(r.graph.count(Call::Init), 0);
    assert_eq!(r.graph.files()[0].data, std::fs::read(&file).unwrap());
    assert!(up.share_link.is_some());

    // An empty file works too, and a file above the limit uses a session.
    let empty = make_file(&r, "e.txt", 0);
    let up = r
        .provider
        .upload(&empty, &target(), None, Progress::none())
        .await
        .unwrap();
    assert_eq!(up.bytes, 0);
    let big = make_file(&r, "b.mp4", 4_000_001);
    r.provider
        .upload(&big, &target(), None, Progress::none())
        .await
        .unwrap();
    assert_eq!(r.graph.count(Call::Init), 1);
    assert!(r.graph.violations().is_empty());
}

#[tokio::test(start_paused = true)]
async fn folders_are_cached_and_names_with_odd_characters_work() {
    let r = rig();
    let file = make_file(&r, "a.mp4", 1000);
    let t = Target {
        folder: "CaptureMann/Mr O'Brien's class #1 & más".into(),
        file_name: "a b+c.mp4".into(),
    };
    r.provider
        .upload(&file, &t, None, Progress::none())
        .await
        .unwrap();
    let lookups = r.graph.count(Call::FolderGet);
    r.provider
        .upload(&file, &t, None, Progress::none())
        .await
        .unwrap();
    assert_eq!(r.graph.count(Call::FolderGet), lookups, "ids are cached");
    assert_eq!(
        r.graph.folders().len(),
        2,
        "CaptureMann and the class folder"
    );
    let files = r.graph.files();
    assert_eq!(files.len(), 2);
    assert_eq!(files[0].name, "a b+c.mp4");
    assert_eq!(files[1].name, "a b+c 1.mp4", "a name clash is renamed");
    assert_eq!(
        r.graph.path_of(&files[0].parent),
        "CaptureMann/Mr O'Brien's class #1 & más"
    );
}

#[tokio::test(start_paused = true)]
async fn an_existing_folder_is_found_not_duplicated() {
    let r = rig();
    let file = make_file(&r, "a.mp4", 1000);
    r.provider
        .upload(&file, &target(), None, Progress::none())
        .await
        .unwrap();
    // A second provider (a new run of the app) has no cache.
    let again = OneDriveProvider::new(r.graph.clone(), r.tokens.clone(), OneDriveConfig::default());
    again
        .upload(&file, &target(), None, Progress::none())
        .await
        .unwrap();
    assert_eq!(r.graph.folders().len(), 4);
    assert_eq!(again.root_folder_id(), r.provider.root_folder_id());
}

#[tokio::test(start_paused = true)]
async fn a_folder_create_that_keeps_failing_is_an_error_not_a_loop() {
    // The create answers 409: somebody else made the folder after our lookup.
    let r = rig();
    r.graph.inject(
        Call::FolderCreate,
        0,
        status_fault(409, None, "nameAlreadyExists"),
        1,
    );
    r.graph.inject(
        Call::FolderGet,
        0,
        status_fault(404, None, "itemNotFound"),
        1,
    );
    let file = make_file(&r, "a.mp4", 1000);
    // The injected 404 hides nothing here (no folder yet), so the create is attempted, hits the
    // injected 409, and the second lookup finds nothing: that is an error, not a loop.
    let err = r
        .provider
        .upload(&file, &target(), None, Progress::none())
        .await
        .unwrap_err();
    assert!(matches!(err, CmError::PlatformRejected { .. }), "{err:?}");
}

#[tokio::test(start_paused = true)]
async fn network_drop_at_40_percent_resumes_from_the_last_confirmed_chunk() {
    // AC3. No in-place retries: the first call fails, the second continues from the state.
    let r = rig_with(no_retries);
    let file = make_file(&r, "a.mp4", SIZE);
    let at = 2 * CHUNK; // 40% of 3.1 MiB is inside the third chunk
    r.graph.inject(Call::Chunk, at, Fault::DropBefore, 1);
    let (p, saved, _) = tracked();
    let err = r
        .provider
        .upload(&file, &target(), None, p)
        .await
        .unwrap_err();
    assert!(err.is_retryable(), "{err:?}");
    let resume = saved.lock().unwrap().clone().unwrap();
    let st: OneDriveResume = serde_json::from_value(resume.0.clone()).unwrap();
    assert_eq!(st.offset, at, "two chunks were confirmed");

    let up = r
        .provider
        .upload(&file, &target(), Some(resume), Progress::none())
        .await
        .unwrap();
    assert_eq!(up.bytes, SIZE as u64);
    assert_eq!(r.graph.sessions_started(), 1, "the session was reused");
    assert_eq!(
        starts(&r),
        vec![0, CHUNK, 2 * CHUNK, 3 * CHUNK, 4 * CHUNK, 5 * CHUNK]
    );
    assert_eq!(r.graph.files()[0].data, std::fs::read(&file).unwrap());
    assert!(r.graph.violations().is_empty());
}

#[tokio::test(start_paused = true)]
async fn a_lost_answer_does_not_resend_a_chunk_that_landed() {
    // Graph got the chunk, the answer was lost: the status query (nextExpectedRanges) finds it.
    let r = rig_with(no_retries);
    let file = make_file(&r, "a.mp4", SIZE);
    r.graph.inject(Call::Chunk, 2 * CHUNK, Fault::DropAfter, 1);
    let (p, saved, _) = tracked();
    assert!(r.provider.upload(&file, &target(), None, p).await.is_err());
    let resume = saved.lock().unwrap().clone().unwrap();
    r.provider
        .upload(&file, &target(), Some(resume), Progress::none())
        .await
        .unwrap();
    assert_eq!(
        starts(&r),
        vec![0, CHUNK, 2 * CHUNK, 3 * CHUNK, 4 * CHUNK, 5 * CHUNK]
    );
    assert_eq!(r.graph.files()[0].data, std::fs::read(&file).unwrap());
    assert!(r.graph.count(Call::StatusQuery) >= 1);
}

#[tokio::test(start_paused = true)]
async fn a_416_asks_where_the_session_is() {
    let r = rig();
    let file = make_file(&r, "a.mp4", SIZE);
    r.graph.inject(
        Call::Chunk,
        CHUNK,
        status_fault(416, None, "invalidRange"),
        1,
    );
    r.provider
        .upload(&file, &target(), None, Progress::none())
        .await
        .unwrap();
    assert_eq!(r.graph.files()[0].data, std::fs::read(&file).unwrap());
    assert_eq!(r.graph.count(Call::StatusQuery), 1);
}

#[tokio::test(start_paused = true)]
async fn transient_failures_recover_inside_one_call() {
    let r = rig();
    let file = make_file(&r, "a.mp4", SIZE);
    r.graph.inject(Call::Chunk, CHUNK, Fault::DropAfter, 1);
    r.graph.inject(
        Call::Chunk,
        3 * CHUNK,
        status_fault(503, None, "serviceNotAvailable"),
        2,
    );
    r.graph.inject(Call::Init, 0, Fault::DropBefore, 1);
    r.provider
        .upload(&file, &target(), None, Progress::none())
        .await
        .unwrap();
    assert_eq!(r.graph.files()[0].data, std::fs::read(&file).unwrap());
    assert_eq!(
        r.graph.sessions_started(),
        2,
        "the dropped init was retried"
    );
    assert_eq!(
        starts(&r),
        vec![0, CHUNK, 2 * CHUNK, 3 * CHUNK, 4 * CHUNK, 5 * CHUNK]
    );
}

#[tokio::test(start_paused = true)]
async fn five_hundreds_that_never_stop_become_a_retryable_network_error() {
    let r = rig_with(|c| c.retry.max_retries = 2);
    let file = make_file(&r, "a.mp4", 1000);
    r.graph.inject(
        Call::Init,
        0,
        status_fault(500, None, "generalException"),
        100,
    );
    let err = r
        .provider
        .upload(&file, &target(), None, Progress::none())
        .await
        .unwrap_err();
    assert!(matches!(err, CmError::Network { .. }), "{err:?}");
    assert!(err.is_retryable());
    assert_eq!(r.graph.count(Call::Init), 3, "one try and two retries");
}

#[tokio::test(start_paused = true)]
async fn retry_after_is_obeyed_and_a_long_wait_is_left_to_the_queue() {
    let r = rig();
    let file = make_file(&r, "a.mp4", 1000);
    r.graph.inject(
        Call::Init,
        0,
        status_fault(429, Some(7), "activityLimitReached"),
        1,
    );
    let t0 = tokio::time::Instant::now();
    r.provider
        .upload(&file, &target(), None, Progress::none())
        .await
        .unwrap();
    assert!(
        t0.elapsed() >= Duration::from_secs(7),
        "waited per Retry-After"
    );
    assert!(t0.elapsed() < Duration::from_secs(8));

    r.graph.inject(
        Call::Init,
        0,
        status_fault(429, Some(600), "activityLimitReached"),
        1,
    );
    let t0 = tokio::time::Instant::now();
    let err = r
        .provider
        .upload(
            &make_file(&r, "b.mp4", 10),
            &target(),
            None,
            Progress::none(),
        )
        .await
        .unwrap_err();
    assert!(
        matches!(&err, CmError::Network { message } if message.contains("600")),
        "{err:?}"
    );
    assert!(
        t0.elapsed() < Duration::from_secs(1),
        "did not wait in place"
    );
}

#[tokio::test(start_paused = true)]
async fn a_401_asks_for_a_new_token_once() {
    let r = rig();
    r.graph.set_valid_token("tok-2");
    r.tokens.refresh_to("tok-2");
    let file = make_file(&r, "a.mp4", SIZE);
    r.provider
        .upload(&file, &target(), None, Progress::none())
        .await
        .unwrap();
    assert_eq!(r.graph.files()[0].data, std::fs::read(&file).unwrap());
    // Once at the start, once after the first 401; later requests reuse the new token.
    assert_eq!(r.tokens.calls(), 2);
}

#[tokio::test(start_paused = true)]
async fn a_revoked_token_is_auth_expired() {
    // AC5: the source cannot renew.
    let r = rig();
    r.graph.set_valid_token("tok-2");
    r.tokens.revoke_on_reject();
    let err = r
        .provider
        .upload(
            &make_file(&r, "a.mp4", 1000),
            &target(),
            None,
            Progress::none(),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, CmError::AuthExpired { .. }), "{err:?}");
    assert!(r.graph.files().is_empty());

    // The source answers with the same token again: the server still says 401.
    let r = rig();
    r.graph.set_valid_token("tok-2");
    let err = r
        .provider
        .upload(
            &make_file(&r, "a.mp4", 10),
            &target(),
            None,
            Progress::none(),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, CmError::AuthExpired { .. }), "{err:?}");
    assert!(!err.to_string().contains("tok-"), "no token in the message");

    // The account is gone entirely.
    let p = OneDriveProvider::new(
        Arc::new(FakeOneDrive::new()),
        Arc::new(FakeTokens::revoked()),
        OneDriveConfig::default(),
    );
    let err = p
        .upload(
            &make_file(&r, "c.mp4", 10),
            &target(),
            None,
            Progress::none(),
        )
        .await
        .unwrap_err();
    assert!(matches!(err, CmError::AuthExpired { .. }), "{err:?}");
}

#[tokio::test(start_paused = true)]
async fn quota_exceeded_is_platform_rejected_and_not_retried() {
    let r = rig();
    r.graph.set_quota(100);
    let err = r
        .provider
        .upload(
            &make_file(&r, "a.mp4", 5000),
            &target(),
            None,
            Progress::none(),
        )
        .await
        .unwrap_err();
    match err {
        CmError::PlatformRejected {
            message,
            platform_code,
        } => {
            assert!(message.contains("quota"), "{message}");
            assert_eq!(platform_code.as_deref(), Some("quotaLimitReached"));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        r.graph.count(Call::Init),
        1,
        "507 is not a transient failure"
    );
}

#[tokio::test(start_paused = true)]
async fn a_403_on_upload_is_platform_rejected() {
    let r = rig();
    r.graph
        .inject(Call::Init, 0, status_fault(403, None, "accessDenied"), 1);
    let err = r
        .provider
        .upload(
            &make_file(&r, "a.mp4", 5000),
            &target(),
            None,
            Progress::none(),
        )
        .await
        .unwrap_err();
    assert!(
        matches!(&err, CmError::PlatformRejected { platform_code, .. }
            if platform_code.as_deref() == Some("accessDenied")),
        "{err:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn a_size_mismatch_is_an_error() {
    let r = rig();
    r.graph.skew_final_size(-1);
    let err = r
        .provider
        .upload(
            &make_file(&r, "a.mp4", 5000),
            &target(),
            None,
            Progress::none(),
        )
        .await
        .unwrap_err();
    assert!(err.to_string().contains("not complete"), "{err}");
    assert!(r.graph.links().is_empty(), "no link for an incomplete file");
}

#[tokio::test(start_paused = true)]
async fn forbidden_anonymous_links_fall_back_to_the_organisation() {
    let r = rig();
    r.graph.deny_anonymous_links();
    let up = r
        .provider
        .upload(
            &make_file(&r, "a.mp4", 5000),
            &target(),
            None,
            Progress::none(),
        )
        .await
        .unwrap();
    let id = r.graph.files()[0].id.clone();
    assert_eq!(
        up.share_link.as_deref(),
        Some(format!("https://1drv.fake/organization/{id}").as_str())
    );
    assert!(up.note.unwrap().contains("organisation"));
    assert_eq!(r.graph.links(), vec![(id, "organization".into())]);
}

#[tokio::test(start_paused = true)]
async fn when_no_link_is_allowed_the_upload_still_finishes() {
    let r = rig();
    r.graph.deny_all_links();
    let up = r
        .provider
        .upload(
            &make_file(&r, "a.mp4", 5000),
            &target(),
            None,
            Progress::none(),
        )
        .await
        .unwrap();
    assert!(up.share_link.is_none());
    assert!(up.note.unwrap().contains("no share link"));
    assert_eq!(r.graph.files().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn link_modes_organization_and_none() {
    let r = rig_with(|c| c.link_mode = LinkMode::Organization);
    let up = r
        .provider
        .upload(
            &make_file(&r, "a.mp4", 5000),
            &target(),
            None,
            Progress::none(),
        )
        .await
        .unwrap();
    assert!(up.share_link.unwrap().contains("/organization/"));
    assert_eq!(r.graph.links()[0].1, "organization");

    // STO-4.AC2: no link, no permission change.
    let r = rig_with(|c| c.link_mode = LinkMode::None);
    let up = r
        .provider
        .upload(
            &make_file(&r, "a.mp4", 5000),
            &target(),
            None,
            Progress::none(),
        )
        .await
        .unwrap();
    assert!(up.share_link.is_none() && up.note.is_none());
    assert_eq!(r.graph.count(Call::CreateLink), 0);
}

#[tokio::test(start_paused = true)]
async fn a_changed_file_starts_a_new_session() {
    let r = rig_with(no_retries);
    let file = make_file(&r, "a.mp4", SIZE);
    r.graph.inject(Call::Chunk, 2 * CHUNK, Fault::DropBefore, 1);
    let (p, saved, _) = tracked();
    assert!(r.provider.upload(&file, &target(), None, p).await.is_err());
    let resume = saved.lock().unwrap().clone().unwrap();

    // Same size, different content.
    let mut data = std::fs::read(&file).unwrap();
    data[10] ^= 0xff;
    std::fs::write(&file, &data).unwrap();
    r.provider
        .upload(&file, &target(), Some(resume), Progress::none())
        .await
        .unwrap();
    assert_eq!(r.graph.sessions_started(), 2);
    assert_eq!(r.graph.files().pop().unwrap().data, data);
}

#[tokio::test(start_paused = true)]
async fn an_expired_session_restarts_from_zero() {
    let r = rig_with(no_retries);
    let file = make_file(&r, "a.mp4", SIZE);
    r.graph.inject(Call::Chunk, 2 * CHUNK, Fault::DropBefore, 1);
    let (p, saved, _) = tracked();
    assert!(r.provider.upload(&file, &target(), None, p).await.is_err());
    let resume = saved.lock().unwrap().clone().unwrap();
    r.graph.expire_sessions();
    r.provider
        .upload(&file, &target(), Some(resume), Progress::none())
        .await
        .unwrap();
    assert_eq!(r.graph.sessions_started(), 2);
    assert_eq!(r.graph.files()[0].data, std::fs::read(&file).unwrap());
}

#[tokio::test(start_paused = true)]
async fn cancel_stops_between_chunks() {
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
    let err = r
        .provider
        .upload(&file, &target(), None, p)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("cancelled"));
    assert!(r.graph.chunks().len() < 6);
}

#[tokio::test(start_paused = true)]
async fn the_account_is_named_by_mail_or_principal_name_and_the_kind_is_known() {
    let r = rig();
    let a = r.provider.account().await.unwrap();
    assert_eq!(
        (a.label.as_str(), a.work),
        ("teacher@example.org", Some(true))
    );
    r.graph.set_account(None, "t.user@school.onmicrosoft.com");
    assert_eq!(
        r.provider.account().await.unwrap().label,
        "t.user@school.onmicrosoft.com"
    );
    r.graph.set_drive_type("personal");
    assert_eq!(r.provider.account().await.unwrap().work, Some(false));
    // Graph refuses the profile: the drive's owner names the account.
    r.graph.set_account(Some("me@outlook.example"), "ignored");
    r.graph.deny_profile();
    assert_eq!(
        r.provider.account().await.unwrap().label,
        "me@outlook.example"
    );
    r.graph.set_account(None, "ignored");
    assert_eq!(r.provider.account().await.unwrap().label, "Fake User");

    let id = r.provider.ensure_root_folder().await.unwrap();
    assert_eq!(r.provider.root_folder_id(), Some(id));
    assert_eq!(r.graph.folders().len(), 1);

    r.graph.set_valid_token("other");
    r.tokens.revoke_on_reject();
    assert!(matches!(
        r.provider.account().await,
        Err(CmError::AuthExpired { .. })
    ));
}

#[tokio::test(start_paused = true)]
async fn test_connection_uploads_and_deletes_a_file() {
    let r = rig_with(|c| c.simple_put_max = SIMPLE_PUT_MAX);
    r.provider.test_connection().await.unwrap();
    assert!(r.graph.files().is_empty(), "the test file is gone");
    assert_eq!(r.graph.count(Call::Delete), 1);
    assert!(r.graph.links().is_empty(), "no link for the test file");
    assert_eq!(r.graph.folders().len(), 1, "CaptureMann exists");

    r.graph.set_valid_token("other");
    r.tokens.revoke_on_reject();
    assert!(matches!(
        r.provider.test_connection().await,
        Err(CmError::AuthExpired { .. })
    ));
}
