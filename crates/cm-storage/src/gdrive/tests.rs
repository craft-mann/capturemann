//! Tests of the Drive provider against the in-process fake server (STO-2.AC2, AC3, AC5, STO-4).
//! "Tested with fakes": no network, no account.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::*;
use crate::fake_drive::{Call, FakeDrive, FakeTokens, Fault};

mod replace;

const KIB: u64 = 1024;
const CHUNK: u64 = 512 * KIB;

struct Rig {
    drive: Arc<FakeDrive>,
    tokens: Arc<FakeTokens>,
    provider: GoogleDriveProvider,
    tmp: tempfile::TempDir,
}

fn rig_with(cfg: impl FnOnce(&mut DriveConfig)) -> Rig {
    let drive = Arc::new(FakeDrive::new());
    let tokens = Arc::new(FakeTokens::new("tok-1"));
    let mut c = DriveConfig {
        chunk_size: CHUNK,
        backoff_base: Duration::from_millis(10),
        ..DriveConfig::default()
    };
    cfg(&mut c);
    let provider = GoogleDriveProvider::new(drive.clone(), tokens.clone(), c);
    Rig {
        drive,
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

const SIZE: usize = 5 * 512 * 1024 + 1234; // five full chunks and a short last one

#[tokio::test(start_paused = true)]
async fn uploads_in_chunks_and_makes_a_link() {
    let r = rig();
    let file = make_file(&r, "a.mp4", SIZE);
    let (p, saved, last) = tracked();
    let up = r.provider.upload(&file, &target(), None, p).await.unwrap();

    // AC2: the remote file is the local file, byte for byte, in the right folder.
    let files = r.drive.files();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].data, std::fs::read(&file).unwrap());
    assert_eq!(up.bytes, SIZE as u64);
    assert_eq!(
        r.drive.path_of(&files[0].parent),
        "CaptureMann/2026/10/Lesson"
    );
    assert_eq!(last.load(Ordering::Relaxed), SIZE as u64);

    // Every chunk but the last is a multiple of 256 KiB.
    let chunks = r.drive.chunks();
    assert_eq!(chunks.len(), 6);
    assert!(chunks[..5].iter().all(|c| c.1 % (256 * KIB) == 0));
    assert!(r.drive.violations().is_empty());

    // STO-4: reader/anyone permission, and the webViewLink.
    assert_eq!(
        r.drive.permissions(),
        vec![(files[0].id.clone(), "reader".into(), "anyone".into())]
    );
    assert_eq!(
        up.share_link.as_deref(),
        Some(format!("https://drive.fake/file/{}/view", files[0].id).as_str())
    );
    assert!(up.note.is_none());

    // The resume state was saved after every chunk and ends at the file size.
    let st: DriveResume = serde_json::from_value(saved.lock().unwrap().clone().unwrap().0).unwrap();
    assert_eq!(st.offset, 5 * CHUNK, "the last chunk ended the session");
    assert_eq!(st.size, SIZE as u64);
}

#[tokio::test(start_paused = true)]
async fn folders_are_cached_and_names_with_quotes_work() {
    let r = rig();
    let file = make_file(&r, "a.mp4", 1000);
    let t = Target {
        folder: "CaptureMann/Mr O'Brien's class".into(),
        file_name: "a.mp4".into(),
    };
    r.provider
        .upload(&file, &t, None, Progress::none())
        .await
        .unwrap();
    let searches = r.drive.count(Call::FolderSearch);
    r.provider
        .upload(&file, &t, None, Progress::none())
        .await
        .unwrap();
    assert_eq!(
        r.drive.count(Call::FolderSearch),
        searches,
        "ids are cached"
    );
    assert_eq!(
        r.drive.folders().len(),
        2,
        "CaptureMann and the class folder"
    );
    assert_eq!(r.drive.files().len(), 2);
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
    let again = GoogleDriveProvider::new(r.drive.clone(), r.tokens.clone(), DriveConfig::default());
    again
        .upload(&file, &target(), None, Progress::none())
        .await
        .unwrap();
    assert_eq!(r.drive.folders().len(), 4);
}

#[tokio::test(start_paused = true)]
async fn network_drop_at_40_percent_resumes_from_the_last_confirmed_chunk() {
    // AC3. No in-place retries: the first call fails, the second continues from the state.
    let r = rig_with(|c| c.max_retries = 0);
    let file = make_file(&r, "a.mp4", SIZE);
    let at = 2 * CHUNK; // 40% of 2.6 MiB is inside the third chunk
    r.drive.inject(Call::Chunk, at, Fault::DropBefore, 1);
    let (p, saved, _) = tracked();
    let err = r
        .provider
        .upload(&file, &target(), None, p)
        .await
        .unwrap_err();
    assert!(err.is_retryable(), "{err:?}");
    let resume = saved.lock().unwrap().clone().unwrap();
    let st: DriveResume = serde_json::from_value(resume.0.clone()).unwrap();
    assert_eq!(st.offset, at, "two chunks were confirmed");

    let up = r
        .provider
        .upload(&file, &target(), Some(resume), Progress::none())
        .await
        .unwrap();
    assert_eq!(up.bytes, SIZE as u64);
    assert_eq!(r.drive.sessions_started(), 1, "the session was reused");
    let starts: Vec<u64> = r.drive.chunks().iter().map(|c| c.0).collect();
    assert_eq!(
        starts,
        vec![0, CHUNK, 2 * CHUNK, 3 * CHUNK, 4 * CHUNK, 5 * CHUNK]
    );
    assert_eq!(r.drive.files()[0].data, std::fs::read(&file).unwrap());
}

#[tokio::test(start_paused = true)]
async fn a_lost_answer_does_not_resend_a_chunk_that_landed() {
    // The server got the chunk, the answer was lost: the status query finds it.
    let r = rig_with(|c| c.max_retries = 0);
    let file = make_file(&r, "a.mp4", SIZE);
    r.drive.inject(Call::Chunk, 2 * CHUNK, Fault::DropAfter, 1);
    let (p, saved, _) = tracked();
    assert!(r.provider.upload(&file, &target(), None, p).await.is_err());
    let resume = saved.lock().unwrap().clone().unwrap();
    r.provider
        .upload(&file, &target(), Some(resume), Progress::none())
        .await
        .unwrap();
    let starts: Vec<u64> = r.drive.chunks().iter().map(|c| c.0).collect();
    assert_eq!(
        starts,
        vec![0, CHUNK, 2 * CHUNK, 3 * CHUNK, 4 * CHUNK, 5 * CHUNK]
    );
    assert_eq!(r.drive.files()[0].data, std::fs::read(&file).unwrap());
}

#[tokio::test(start_paused = true)]
async fn transient_failures_recover_inside_one_call() {
    let r = rig();
    let file = make_file(&r, "a.mp4", SIZE);
    r.drive.inject(Call::Chunk, CHUNK, Fault::DropAfter, 1);
    r.drive.inject(
        Call::Chunk,
        3 * CHUNK,
        Fault::Status {
            status: 503,
            retry_after: None,
            reason: "backendError",
            message: "Backend Error",
        },
        2,
    );
    r.drive.inject(Call::Init, 0, Fault::DropBefore, 1);
    r.provider
        .upload(&file, &target(), None, Progress::none())
        .await
        .unwrap();
    assert_eq!(r.drive.files()[0].data, std::fs::read(&file).unwrap());
    assert_eq!(
        r.drive.sessions_started(),
        2,
        "the dropped init was retried"
    );
    let starts: Vec<u64> = r.drive.chunks().iter().map(|c| c.0).collect();
    assert_eq!(
        starts,
        vec![0, CHUNK, 2 * CHUNK, 3 * CHUNK, 4 * CHUNK, 5 * CHUNK]
    );
}

#[tokio::test(start_paused = true)]
async fn five_hundreds_that_never_stop_become_a_retryable_network_error() {
    let r = rig_with(|c| c.max_retries = 2);
    let file = make_file(&r, "a.mp4", 1000);
    r.drive.inject(
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
    let err = r
        .provider
        .upload(&file, &target(), None, Progress::none())
        .await
        .unwrap_err();
    assert!(matches!(err, CmError::Network { .. }), "{err:?}");
    assert_eq!(r.drive.count(Call::Init), 3, "one try and two retries");
}

#[tokio::test(start_paused = true)]
async fn rate_limits_wait_for_retry_after() {
    let r = rig();
    let file = make_file(&r, "a.mp4", 1000);
    r.drive.inject(
        Call::Init,
        0,
        Fault::Status {
            status: 429,
            retry_after: Some(7),
            reason: "rateLimitExceeded",
            message: "Too many requests",
        },
        1,
    );
    let t0 = tokio::time::Instant::now();
    r.provider
        .upload(&file, &target(), None, Progress::none())
        .await
        .unwrap();
    assert!(t0.elapsed() >= Duration::from_secs(7), "{:?}", t0.elapsed());
    assert!(t0.elapsed() < Duration::from_secs(10));
}

#[tokio::test(start_paused = true)]
async fn a_403_rate_limit_is_retried_but_a_long_wait_is_left_to_the_job_queue() {
    let r = rig();
    let file = make_file(&r, "a.mp4", 1000);
    r.drive.inject(
        Call::Init,
        0,
        Fault::Status {
            status: 403,
            retry_after: None,
            reason: "userRateLimitExceeded",
            message: "Rate Limit Exceeded",
        },
        1,
    );
    r.provider
        .upload(&file, &target(), None, Progress::none())
        .await
        .unwrap();

    let r = rig();
    r.drive.inject(
        Call::Init,
        0,
        Fault::Status {
            status: 429,
            retry_after: Some(600),
            reason: "rateLimitExceeded",
            message: "Too many requests",
        },
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
    r.drive.set_valid_token("tok-2");
    r.tokens.refresh_to("tok-2");
    let file = make_file(&r, "a.mp4", SIZE);
    r.provider
        .upload(&file, &target(), None, Progress::none())
        .await
        .unwrap();
    assert_eq!(r.drive.files()[0].data, std::fs::read(&file).unwrap());
    // Once at the start, once after the first 401; later requests reuse the new token.
    assert_eq!(r.tokens.calls(), 2);
}

#[tokio::test(start_paused = true)]
async fn a_revoked_token_is_auth_expired() {
    // AC5: the source cannot renew.
    let r = rig();
    r.drive.set_valid_token("tok-2");
    r.tokens.revoke_on_reject();
    let file = make_file(&r, "a.mp4", 1000);
    let err = r
        .provider
        .upload(&file, &target(), None, Progress::none())
        .await
        .unwrap_err();
    assert!(matches!(err, CmError::AuthExpired { .. }), "{err:?}");
    assert!(r.drive.files().is_empty());

    // The source answers with the same token again: the server still says 401.
    let r = rig();
    r.drive.set_valid_token("tok-2");
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
    let drive = Arc::new(FakeDrive::new());
    let p = GoogleDriveProvider::new(
        drive,
        Arc::new(FakeTokens::revoked()),
        DriveConfig::default(),
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
async fn quota_exceeded_is_platform_rejected_with_the_providers_message() {
    let r = rig();
    r.drive.set_quota(100);
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
            assert!(message.contains("storage quota"), "{message}");
            assert_eq!(platform_code.as_deref(), Some("storageQuotaExceeded"));
        }
        other => panic!("{other:?}"),
    }
}

#[tokio::test(start_paused = true)]
async fn a_size_mismatch_is_an_error() {
    let r = rig();
    r.drive.skew_final_size(-1);
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
}

#[tokio::test(start_paused = true)]
async fn an_organisation_policy_gives_no_link_but_a_finished_upload() {
    let r = rig();
    r.drive.deny_public_links();
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
    assert!(up.note.unwrap().contains("organisation"));
    assert_eq!(r.drive.files().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn link_mode_none_changes_no_permission() {
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
    assert!(r.drive.permissions().is_empty());
    assert_eq!(r.drive.count(Call::Permission), 0);
}

#[tokio::test(start_paused = true)]
async fn a_changed_file_starts_a_new_session() {
    let r = rig_with(|c| c.max_retries = 0);
    let file = make_file(&r, "a.mp4", SIZE);
    r.drive.inject(Call::Chunk, 2 * CHUNK, Fault::DropBefore, 1);
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
    assert_eq!(r.drive.sessions_started(), 2);
    let newest = r.drive.files().pop().unwrap();
    assert_eq!(newest.data, data);
}

#[tokio::test(start_paused = true)]
async fn an_expired_session_restarts_from_zero() {
    let r = rig_with(|c| c.max_retries = 0);
    let file = make_file(&r, "a.mp4", SIZE);
    r.drive.inject(Call::Chunk, 2 * CHUNK, Fault::DropBefore, 1);
    let (p, saved, _) = tracked();
    assert!(r.provider.upload(&file, &target(), None, p).await.is_err());
    let resume = saved.lock().unwrap().clone().unwrap();
    r.drive.expire_sessions();
    r.provider
        .upload(&file, &target(), Some(resume), Progress::none())
        .await
        .unwrap();
    assert_eq!(r.drive.sessions_started(), 2);
    assert_eq!(r.drive.files()[0].data, std::fs::read(&file).unwrap());
}

#[tokio::test(start_paused = true)]
async fn an_empty_file_uploads() {
    let r = rig();
    let up = r
        .provider
        .upload(
            &make_file(&r, "e.txt", 0),
            &target(),
            None,
            Progress::none(),
        )
        .await
        .unwrap();
    assert_eq!(up.bytes, 0);
    assert_eq!(r.drive.files()[0].data.len(), 0);
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
    assert!(r.drive.chunks().len() < 6);
}

#[tokio::test(start_paused = true)]
async fn test_connection_uploads_and_deletes_a_file() {
    let r = rig();
    r.provider.test_connection().await.unwrap();
    assert!(r.drive.files().is_empty(), "the test file is gone");
    assert_eq!(r.drive.count(Call::Delete), 1);
    assert!(
        r.drive.permissions().is_empty(),
        "no link for the test file"
    );
    assert_eq!(r.drive.folders().len(), 1, "CaptureMann exists");

    r.drive.set_valid_token("other");
    r.tokens.revoke_on_reject();
    assert!(matches!(
        r.provider.test_connection().await,
        Err(CmError::AuthExpired { .. })
    ));
}

#[test]
fn chunk_size_is_a_multiple_of_256_kib() {
    let drive = Arc::new(FakeDrive::new());
    let p = GoogleDriveProvider::new(
        drive,
        Arc::new(FakeTokens::new("t")),
        DriveConfig {
            chunk_size: 1_000_000,
            ..DriveConfig::default()
        },
    );
    assert_eq!(p.cfg.chunk_size, 3 * 256 * KIB);
    assert_eq!(DriveConfig::default().chunk_size, 8 * 1024 * KIB);
}

/// STO-2.AC1 (the parts the provider owns): the account email and the "CaptureMann" folder.
#[tokio::test(start_paused = true)]
async fn reports_the_account_email_and_creates_the_root_folder_once() {
    let r = rig();
    r.drive.set_account_email("teacher@school.example");
    assert_eq!(
        r.provider.account_email().await.unwrap(),
        "teacher@school.example"
    );
    let id = r.provider.ensure_root_folder().await.unwrap();
    assert_eq!(r.provider.ensure_root_folder().await.unwrap(), id);
    let folders = r.drive.folders();
    assert_eq!(folders.len(), 1);
    assert_eq!(folders[0].name, "CaptureMann");
    assert_eq!(folders[0].id, id);
    assert_eq!(r.provider.root_folder_id().as_deref(), Some(id.as_str()));
}

#[tokio::test(start_paused = true)]
async fn email_and_folder_calls_need_a_valid_sign_in() {
    let r = rig();
    r.drive.set_valid_token("other");
    r.tokens.revoke_on_reject();
    assert!(matches!(
        r.provider.account_email().await,
        Err(CmError::AuthExpired { .. })
    ));
    assert!(matches!(
        r.provider.ensure_root_folder().await,
        Err(CmError::AuthExpired { .. })
    ));
    assert!(r.drive.folders().is_empty());
}

// ---- bundle folders and deleting (STO-5, library delete) ----------------------------------

const BASE: &str = "CaptureMann/2026/10/Lesson";

/// A second provider over the same fake server (another job, another process).
fn rig_sharing(r: &Rig) -> Rig {
    let tokens = Arc::new(FakeTokens::new("tok-1"));
    let provider = GoogleDriveProvider::new(
        r.drive.clone(),
        tokens.clone(),
        DriveConfig {
            chunk_size: CHUNK,
            backoff_base: Duration::from_millis(10),
            ..DriveConfig::default()
        },
    );
    Rig {
        drive: r.drive.clone(),
        tokens,
        provider,
        tmp: tempfile::tempdir().unwrap(),
    }
}

#[tokio::test(start_paused = true)]
async fn a_second_bundle_copy_claims_a_new_folder_name() {
    let r = rig();
    let p = &r.provider;
    let first = p.claim_bundle_folder(BASE, "Lesson.cmrec").await.unwrap();
    assert_eq!(first.name, "Lesson.cmrec");
    let second = p.claim_bundle_folder(BASE, "Lesson.cmrec").await.unwrap();
    assert_eq!(second.name, "Lesson (2).cmrec");
    let third = p.claim_bundle_folder(BASE, "Lesson.cmrec").await.unwrap();
    assert_eq!(third.name, "Lesson (3).cmrec");
    assert_ne!(first.id, second.id);
    let path = r.drive.path_of(second.id.as_deref().unwrap());
    assert_eq!(path, "CaptureMann/2026/10/Lesson/Lesson (2).cmrec");
}

#[tokio::test(start_paused = true)]
async fn files_go_into_the_claimed_folder_not_the_older_copy() {
    let r = rig();
    r.provider
        .claim_bundle_folder(BASE, "Lesson.cmrec")
        .await
        .unwrap();
    let f = make_file(&r, "manifest.json", 100);
    let t = |name: &str| Target {
        folder: format!("{BASE}/{name}"),
        file_name: "manifest.json".into(),
    };
    r.provider
        .upload(&f, &t("Lesson.cmrec"), None, Progress::none())
        .await
        .unwrap();
    // A new provider (a new job) claims the next name; its files stay apart from the first copy.
    let again = rig_sharing(&r);
    let new = again
        .provider
        .claim_bundle_folder(BASE, "Lesson.cmrec")
        .await
        .unwrap();
    assert_eq!(new.name, "Lesson (2).cmrec");
    again
        .provider
        .upload(&f, &t(&new.name), None, Progress::none())
        .await
        .unwrap();
    let mut paths: Vec<String> = r
        .drive
        .files()
        .iter()
        .map(|f| r.drive.path_of(&f.id))
        .collect();
    paths.sort();
    assert_eq!(
        paths,
        [
            "CaptureMann/2026/10/Lesson/Lesson (2).cmrec/manifest.json",
            "CaptureMann/2026/10/Lesson/Lesson.cmrec/manifest.json"
        ]
    );
}

#[tokio::test(start_paused = true)]
async fn a_rival_that_created_the_same_name_first_wins_and_we_take_the_next() {
    let r = rig();
    // Another machine creates "Lesson.cmrec" between our check and our create.
    r.drive.race_folder_create("Lesson.cmrec");
    let claim = r
        .provider
        .claim_bundle_folder(BASE, "Lesson.cmrec")
        .await
        .unwrap();
    assert_eq!(claim.name, "Lesson (2).cmrec");
    let names: Vec<String> = r
        .drive
        .folders()
        .iter()
        .filter(|f| f.name.contains(".cmrec"))
        .map(|f| f.name.clone())
        .collect();
    // The rival's folder and ours; the losing attempt was removed again.
    assert_eq!(names.len(), 2, "{names:?}");
    assert!(names.contains(&"Lesson.cmrec".to_owned()));
    assert!(names.contains(&"Lesson (2).cmrec".to_owned()));
}

#[tokio::test(start_paused = true)]
async fn a_trashed_folder_of_the_same_name_does_not_count() {
    let r = rig();
    let first = r
        .provider
        .claim_bundle_folder(BASE, "Lesson.cmrec")
        .await
        .unwrap();
    r.drive.trash(first.id.as_deref().unwrap());
    let again = rig_sharing(&r);
    let next = again
        .provider
        .claim_bundle_folder(BASE, "Lesson.cmrec")
        .await
        .unwrap();
    assert_eq!(next.name, "Lesson.cmrec");
}

#[tokio::test(start_paused = true)]
async fn a_restart_continues_in_the_remembered_folder() {
    let r = rig();
    let claim = r
        .provider
        .claim_bundle_folder(BASE, "Lesson.cmrec")
        .await
        .unwrap();
    // The job restarts: a new provider with an empty cache.
    let restarted = rig_sharing(&r);
    let kept = restarted
        .provider
        .reuse_bundle_folder(BASE, &claim)
        .await
        .unwrap();
    assert_eq!(kept, claim);
    let f = make_file(&r, "a.json", 10);
    let t = Target {
        folder: format!("{BASE}/Lesson.cmrec"),
        file_name: "a.json".into(),
    };
    restarted
        .provider
        .upload(&f, &t, None, Progress::none())
        .await
        .unwrap();
    let file = &r.drive.files()[0];
    assert_eq!(Some(file.parent.clone()), claim.id);
    assert_eq!(
        r.drive
            .folders()
            .iter()
            .filter(|f| f.name == "Lesson.cmrec")
            .count(),
        1
    );
}

#[tokio::test(start_paused = true)]
async fn a_remembered_folder_that_was_deleted_is_made_again_under_the_same_name() {
    let r = rig();
    let claim = r
        .provider
        .claim_bundle_folder(BASE, "Lesson.cmrec")
        .await
        .unwrap();
    r.provider
        .delete(claim.id.as_deref().unwrap())
        .await
        .unwrap();
    let restarted = rig_sharing(&r);
    let kept = restarted
        .provider
        .reuse_bundle_folder(BASE, &claim)
        .await
        .unwrap();
    assert_eq!(kept.name, "Lesson.cmrec");
    assert_ne!(kept.id, claim.id);
    assert!(kept.id.is_some());
}

#[tokio::test(start_paused = true)]
async fn a_remembered_folder_that_was_moved_away_is_not_used() {
    let r = rig();
    let claim = r
        .provider
        .claim_bundle_folder(BASE, "Lesson.cmrec")
        .await
        .unwrap();
    r.drive.move_to(claim.id.as_deref().unwrap(), "elsewhere");
    let restarted = rig_sharing(&r);
    let kept = restarted
        .provider
        .reuse_bundle_folder(BASE, &claim)
        .await
        .unwrap();
    assert_ne!(kept.id, claim.id);
}

#[tokio::test(start_paused = true)]
async fn claiming_needs_a_valid_sign_in_and_creates_nothing_without_one() {
    let r = rig();
    r.drive.set_valid_token("other");
    r.tokens.revoke_on_reject();
    let e = r
        .provider
        .claim_bundle_folder(BASE, "Lesson.cmrec")
        .await
        .unwrap_err();
    assert!(matches!(e, CmError::AuthExpired { .. }), "{e:?}");
    assert!(r.drive.folders().is_empty());
}

#[tokio::test(start_paused = true)]
async fn delete_removes_a_file_and_a_folder_with_its_content() {
    let r = rig();
    let f = make_file(&r, "a.mp4", 1000);
    let up = r
        .provider
        .upload(&f, &target(), None, Progress::none())
        .await
        .unwrap();
    r.provider.delete(&up.remote_id).await.unwrap();
    assert!(r.drive.files().is_empty());

    let claim = r
        .provider
        .claim_bundle_folder(BASE, "Lesson.cmrec")
        .await
        .unwrap();
    let t = Target {
        folder: format!("{BASE}/Lesson.cmrec/tracks"),
        file_name: "a.json".into(),
    };
    r.provider
        .upload(&f, &t, None, Progress::none())
        .await
        .unwrap();
    assert_eq!(r.drive.files().len(), 1);
    r.provider
        .delete(claim.id.as_deref().unwrap())
        .await
        .unwrap();
    assert!(
        r.drive.files().is_empty(),
        "the folder's content goes with it"
    );
    // The folders above the copy stay.
    assert!(r.drive.folders().iter().any(|f| f.name == "Lesson"));
}

#[tokio::test(start_paused = true)]
async fn deleting_what_is_already_gone_is_success() {
    let r = rig();
    r.provider.delete("no-such-id").await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn delete_errors_keep_their_codes() {
    let r = rig();
    let f = make_file(&r, "a.mp4", 100);
    let up = r
        .provider
        .upload(&f, &target(), None, Progress::none())
        .await
        .unwrap();

    // 401 and a token that cannot be renewed: auth_expired, the file stays.
    r.drive.set_valid_token("other");
    r.tokens.revoke_on_reject();
    let e = r.provider.delete(&up.remote_id).await.unwrap_err();
    assert!(matches!(e, CmError::AuthExpired { .. }), "{e:?}");
    assert_eq!(r.drive.files().len(), 1);

    // The network keeps failing: network, the file stays.
    let r = rig_with(|c| c.max_retries = 1);
    let f = make_file(&r, "a.mp4", 100);
    let up = r
        .provider
        .upload(&f, &target(), None, Progress::none())
        .await
        .unwrap();
    r.drive.inject(Call::Delete, 0, Fault::DropBefore, 5);
    let e = r.provider.delete(&up.remote_id).await.unwrap_err();
    assert!(matches!(e, CmError::Network { .. }), "{e:?}");
    assert_eq!(r.drive.files().len(), 1);

    // A refusal (no permission) is platform_rejected with Drive's words.
    let r = rig();
    r.drive.inject(
        Call::Delete,
        0,
        Fault::Status {
            status: 403,
            retry_after: None,
            reason: "insufficientFilePermissions",
            message: "The user does not have sufficient permissions for this file.",
        },
        1,
    );
    let e = r.provider.delete("x").await.unwrap_err();
    assert!(
        matches!(&e, CmError::PlatformRejected { message, .. } if message.contains("sufficient permissions")),
        "{e:?}"
    );
}
