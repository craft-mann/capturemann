//! Tests of the Dropbox provider against the in-process fake server (STO-2.AC2, AC3, AC5,
//! STO-4). "Tested with fakes": no network, no account.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::*;
use crate::fake_drive::{FakeTokens, Fault};
use crate::fake_dropbox::{Call, FakeDropbox};

const KIB: u64 = 1024;
const CHUNK: u64 = 512 * KIB;
const SIZE: usize = 5 * 512 * 1024 + 1234; // five appends and a short finish

struct Rig {
    dbx: Arc<FakeDropbox>,
    tokens: Arc<FakeTokens>,
    provider: DropboxProvider,
    tmp: tempfile::TempDir,
}

fn rig_with(cfg: impl FnOnce(&mut DropboxConfig)) -> Rig {
    let dbx = Arc::new(FakeDropbox::new());
    let tokens = Arc::new(FakeTokens::new("tok-1"));
    let mut c = DropboxConfig {
        chunk_size: CHUNK,
        retry: Retry {
            backoff_base: Duration::from_millis(10),
            ..Retry::default()
        },
        ..DropboxConfig::default()
    };
    cfg(&mut c);
    let provider = DropboxProvider::new(dbx.clone(), tokens.clone(), c);
    Rig {
        dbx,
        tokens,
        provider,
        tmp: tempfile::tempdir().unwrap(),
    }
}

fn rig() -> Rig {
    rig_with(|_| {})
}

fn no_retries(c: &mut DropboxConfig) {
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
    r.dbx.chunks().iter().map(|c| c.0).collect()
}

fn status_fault(status: u16, retry_after: Option<u64>, reason: &'static str) -> Fault {
    Fault::Status {
        status,
        retry_after,
        reason,
        message: "scripted failure",
    }
}

#[tokio::test(start_paused = true)]
async fn uploads_in_chunks_and_makes_a_public_link() {
    let r = rig();
    let file = make_file(&r, "a.mp4", SIZE);
    let (p, saved, last) = tracked();
    let up = r.provider.upload(&file, &target(), None, p).await.unwrap();

    // AC2: the remote file is the local file, byte for byte, at the right path.
    let files = r.dbx.files();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].data, std::fs::read(&file).unwrap());
    assert_eq!(files[0].path, "/CaptureMann/2026/10/Lesson/Lesson.mp4");
    assert_eq!(up.bytes, SIZE as u64);
    assert_eq!(up.remote_id, files[0].id);
    assert_eq!(last.load(Ordering::Relaxed), SIZE as u64);

    assert_eq!(
        starts(&r),
        vec![0, CHUNK, 2 * CHUNK, 3 * CHUNK, 4 * CHUNK, 5 * CHUNK]
    );
    assert_eq!(r.dbx.count(Call::Append), 5);
    assert_eq!(r.dbx.count(Call::Finish), 1);
    assert!(r.dbx.violations().is_empty(), "{:?}", r.dbx.violations());

    // STO-4: a public viewer link.
    assert_eq!(r.dbx.links(), vec![(files[0].id.clone(), "public".into())]);
    assert_eq!(
        up.share_link.as_deref(),
        Some(format!("https://dbx.fake/s/{}", files[0].id).as_str())
    );
    assert!(up.note.is_none());

    // The resume state was saved after every chunk.
    let st: DropboxResume =
        serde_json::from_value(saved.lock().unwrap().clone().unwrap().0).unwrap();
    assert_eq!(st.offset, 5 * CHUNK);
    assert_eq!(st.size, SIZE as u64);
    assert_eq!(st.path, files[0].path);
}

#[test]
fn defaults_and_limits() {
    assert_eq!(DropboxConfig::default().chunk_size, 16 * 1024 * KIB);
    let size_of = |chunk_size| {
        DropboxProvider::new(
            Arc::new(FakeDropbox::new()),
            Arc::new(FakeTokens::new("t")),
            DropboxConfig {
                chunk_size,
                ..DropboxConfig::default()
            },
        )
        .cfg
        .chunk_size
    };
    assert_eq!(size_of(u64::MAX), 150_000_000, "at most 150 MB");
    assert_eq!(size_of(0), 1);
}

#[tokio::test(start_paused = true)]
async fn the_app_folder_is_the_root_for_app_folder_apps() {
    let r = rig_with(|c| c.app_folder = true);
    r.provider
        .upload(
            &make_file(&r, "a.mp4", 1000),
            &target(),
            None,
            Progress::none(),
        )
        .await
        .unwrap();
    assert_eq!(r.dbx.files()[0].path, "/2026/10/Lesson/Lesson.mp4");

    // A folder pattern that does not start with the root name keeps all its segments.
    let r = rig();
    let t = Target {
        folder: "Lessons/Week 1".into(),
        file_name: "a.mp4".into(),
    };
    r.provider
        .upload(&make_file(&r, "a.mp4", 10), &t, None, Progress::none())
        .await
        .unwrap();
    assert_eq!(r.dbx.files()[0].path, "/CaptureMann/Lessons/Week 1/a.mp4");
}

#[tokio::test(start_paused = true)]
async fn non_ascii_names_travel_in_an_ascii_header() {
    let r = rig();
    let t = Target {
        folder: "CaptureMann/Lección 日本 😀".into(),
        file_name: "clase \"1\".mp4".into(),
    };
    r.provider
        .upload(&make_file(&r, "a.mp4", 10), &t, None, Progress::none())
        .await
        .unwrap();
    assert!(r.dbx.violations().is_empty(), "{:?}", r.dbx.violations());
    assert_eq!(
        r.dbx.files()[0].path,
        "/CaptureMann/Lección 日本 😀/clase \"1\".mp4"
    );
}

#[test]
fn ascii_json_escapes_what_headers_cannot_carry() {
    let v = json!({ "path": "/ü/日/😀" });
    let s = ascii_json(&v);
    assert!(s.is_ascii());
    assert_eq!(serde_json::from_str::<Value>(&s).unwrap(), v);
}

#[tokio::test(start_paused = true)]
async fn network_drop_at_40_percent_resumes_from_the_last_confirmed_chunk() {
    // AC3. No in-place retries: the first call fails, the second continues from the state.
    let r = rig_with(no_retries);
    let file = make_file(&r, "a.mp4", SIZE);
    let at = 2 * CHUNK;
    r.dbx.inject(Call::Append, at, Fault::DropBefore, 1);
    let (p, saved, _) = tracked();
    let err = r
        .provider
        .upload(&file, &target(), None, p)
        .await
        .unwrap_err();
    assert!(err.is_retryable(), "{err:?}");
    let resume = saved.lock().unwrap().clone().unwrap();
    let st: DropboxResume = serde_json::from_value(resume.0.clone()).unwrap();
    assert_eq!(st.offset, at, "two chunks were confirmed");

    let up = r
        .provider
        .upload(&file, &target(), Some(resume), Progress::none())
        .await
        .unwrap();
    assert_eq!(up.bytes, SIZE as u64);
    assert_eq!(r.dbx.sessions_started(), 1, "the session was reused");
    assert_eq!(
        starts(&r),
        vec![0, CHUNK, 2 * CHUNK, 3 * CHUNK, 4 * CHUNK, 5 * CHUNK]
    );
    assert_eq!(r.dbx.files()[0].data, std::fs::read(&file).unwrap());
}

#[tokio::test(start_paused = true)]
async fn a_lost_answer_is_fixed_by_incorrect_offset_recovery() {
    // Dropbox got the chunk, the answer was lost: the resend at the old offset is answered with
    // `incorrect_offset` and the offset to continue from, so no byte is stored twice.
    let r = rig_with(no_retries);
    let file = make_file(&r, "a.mp4", SIZE);
    r.dbx.inject(Call::Append, 2 * CHUNK, Fault::DropAfter, 1);
    let (p, saved, _) = tracked();
    assert!(r.provider.upload(&file, &target(), None, p).await.is_err());
    let resume = saved.lock().unwrap().clone().unwrap();
    let st: DropboxResume = serde_json::from_value(resume.0.clone()).unwrap();
    assert_eq!(st.offset, 2 * CHUNK, "the client does not know it landed");
    r.provider
        .upload(&file, &target(), Some(resume), Progress::none())
        .await
        .unwrap();
    assert_eq!(
        starts(&r),
        vec![0, CHUNK, 2 * CHUNK, 3 * CHUNK, 4 * CHUNK, 5 * CHUNK],
        "the landed chunk was not stored twice"
    );
    assert_eq!(r.dbx.files()[0].data, std::fs::read(&file).unwrap());
}

#[tokio::test(start_paused = true)]
async fn a_lost_answer_inside_one_call_recovers_too() {
    let r = rig();
    let file = make_file(&r, "a.mp4", SIZE);
    r.dbx.inject(Call::Append, CHUNK, Fault::DropAfter, 1);
    r.dbx.inject(Call::Start, 0, Fault::DropBefore, 1);
    r.provider
        .upload(&file, &target(), None, Progress::none())
        .await
        .unwrap();
    assert_eq!(r.dbx.files()[0].data, std::fs::read(&file).unwrap());
    assert_eq!(r.dbx.sessions_started(), 2, "the dropped start was retried");
    assert_eq!(
        starts(&r),
        vec![0, CHUNK, 2 * CHUNK, 3 * CHUNK, 4 * CHUNK, 5 * CHUNK]
    );
}

#[tokio::test(start_paused = true)]
async fn a_finish_whose_answer_was_lost_is_not_stored_twice() {
    let r = rig();
    let file = make_file(&r, "a.mp4", SIZE);
    r.dbx.inject(Call::Finish, 0, Fault::DropAfter, 1);
    let up = r
        .provider
        .upload(&file, &target(), None, Progress::none())
        .await
        .unwrap();
    assert_eq!(r.dbx.files().len(), 1, "no second copy");
    assert_eq!(up.remote_id, r.dbx.files()[0].id);
    assert_eq!(up.bytes, SIZE as u64);
    assert_eq!(r.dbx.sessions_started(), 1);
}

#[tokio::test(start_paused = true)]
async fn five_hundreds_that_never_stop_become_a_retryable_network_error() {
    let r = rig_with(|c| c.retry.max_retries = 2);
    let file = make_file(&r, "a.mp4", 1000);
    r.dbx
        .inject(Call::Start, 0, status_fault(503, None, "server_error"), 100);
    let err = r
        .provider
        .upload(&file, &target(), None, Progress::none())
        .await
        .unwrap_err();
    assert!(matches!(err, CmError::Network { .. }), "{err:?}");
    assert!(err.is_retryable());
    assert_eq!(r.dbx.count(Call::Start), 3, "one try and two retries");
}

#[tokio::test(start_paused = true)]
async fn too_many_write_operations_waits_for_retry_after() {
    let r = rig();
    let file = make_file(&r, "a.mp4", SIZE);
    r.dbx.inject(
        Call::Append,
        CHUNK,
        status_fault(429, Some(7), "too_many_write_operations"),
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
    assert_eq!(r.dbx.files()[0].data, std::fs::read(&file).unwrap());

    // A long wait is left to the job queue.
    r.dbx.inject(
        Call::Start,
        0,
        status_fault(429, Some(600), "too_many_write_operations"),
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
    r.dbx.set_valid_token("tok-2");
    r.tokens.refresh_to("tok-2");
    let file = make_file(&r, "a.mp4", SIZE);
    r.provider
        .upload(&file, &target(), None, Progress::none())
        .await
        .unwrap();
    assert_eq!(r.dbx.files()[0].data, std::fs::read(&file).unwrap());
    assert_eq!(r.tokens.calls(), 2);
}

#[tokio::test(start_paused = true)]
async fn a_revoked_token_is_auth_expired() {
    // AC5.
    let r = rig();
    r.dbx.set_valid_token("tok-2");
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
    assert!(r.dbx.files().is_empty());

    let r = rig();
    r.dbx.set_valid_token("tok-2");
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

    let p = DropboxProvider::new(
        Arc::new(FakeDropbox::new()),
        Arc::new(FakeTokens::revoked()),
        DropboxConfig::default(),
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
async fn insufficient_space_is_platform_rejected() {
    let r = rig();
    r.dbx.set_quota(100);
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
            assert!(message.contains("full"), "{message}");
            assert!(platform_code.unwrap().contains("insufficient_space"));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(r.dbx.count(Call::Finish), 1, "not retried");
    assert!(r.dbx.files().is_empty());
}

#[tokio::test(start_paused = true)]
async fn a_size_mismatch_is_an_error() {
    let r = rig();
    r.dbx.skew_final_size(-1);
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
    assert!(r.dbx.links().is_empty(), "no link for an incomplete file");
}

#[tokio::test(start_paused = true)]
async fn an_existing_shared_link_is_returned() {
    let r = rig();
    r.dbx.link_exists_for_new_files();
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
    let id = r.dbx.files()[0].id.clone();
    assert_eq!(
        up.share_link.as_deref(),
        Some(format!("https://dbx.fake/s/{id}").as_str())
    );
    assert!(up.note.is_none());
    assert_eq!(r.dbx.count(Call::CreateLink), 1);
    assert_eq!(r.dbx.count(Call::ListLinks), 1);
}

#[tokio::test(start_paused = true)]
async fn forbidden_public_links_fall_back_to_the_team() {
    let r = rig();
    r.dbx.deny_public_links();
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
    let id = r.dbx.files()[0].id.clone();
    assert!(up.share_link.is_some());
    assert!(up.note.unwrap().contains("team"));
    assert_eq!(r.dbx.links(), vec![(id, "team_only".into())]);
}

#[tokio::test(start_paused = true)]
async fn when_no_link_is_allowed_the_upload_still_finishes() {
    let r = rig();
    r.dbx.deny_all_links();
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
    assert_eq!(r.dbx.files().len(), 1);
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
    assert!(up.share_link.is_some() && up.note.is_none());
    assert_eq!(r.dbx.links()[0].1, "team_only");

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
    assert_eq!(r.dbx.count(Call::CreateLink), 0);
}

#[tokio::test(start_paused = true)]
async fn a_changed_file_starts_a_new_session() {
    let r = rig_with(no_retries);
    let file = make_file(&r, "a.mp4", SIZE);
    r.dbx.inject(Call::Append, 2 * CHUNK, Fault::DropBefore, 1);
    let (p, saved, _) = tracked();
    assert!(r.provider.upload(&file, &target(), None, p).await.is_err());
    let resume = saved.lock().unwrap().clone().unwrap();

    let mut data = std::fs::read(&file).unwrap();
    data[10] ^= 0xff;
    std::fs::write(&file, &data).unwrap();
    r.provider
        .upload(&file, &target(), Some(resume), Progress::none())
        .await
        .unwrap();
    assert_eq!(r.dbx.sessions_started(), 2);
    assert_eq!(r.dbx.files().pop().unwrap().data, data);
}

#[tokio::test(start_paused = true)]
async fn an_expired_session_restarts_from_zero() {
    let r = rig_with(no_retries);
    let file = make_file(&r, "a.mp4", SIZE);
    r.dbx.inject(Call::Append, 2 * CHUNK, Fault::DropBefore, 1);
    let (p, saved, _) = tracked();
    assert!(r.provider.upload(&file, &target(), None, p).await.is_err());
    let resume = saved.lock().unwrap().clone().unwrap();
    r.dbx.expire_sessions();
    r.provider
        .upload(&file, &target(), Some(resume), Progress::none())
        .await
        .unwrap();
    assert_eq!(r.dbx.sessions_started(), 2);
    assert_eq!(r.dbx.files()[0].data, std::fs::read(&file).unwrap());
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
    assert_eq!(r.dbx.files()[0].data.len(), 0);
}

#[tokio::test(start_paused = true)]
async fn a_file_that_is_an_exact_multiple_of_the_chunk_finishes_with_a_full_chunk() {
    let r = rig();
    let file = make_file(&r, "a.mp4", 2 * CHUNK as usize);
    r.provider
        .upload(&file, &target(), None, Progress::none())
        .await
        .unwrap();
    assert_eq!(r.dbx.chunks(), vec![(0, CHUNK), (CHUNK, CHUNK)]);
    assert_eq!(r.dbx.count(Call::Append), 1);
    assert_eq!(r.dbx.files()[0].data, std::fs::read(&file).unwrap());
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
    assert!(r.dbx.chunks().len() < 6);
    assert!(r.dbx.files().is_empty());
}

#[tokio::test(start_paused = true)]
async fn the_account_is_named_by_its_email_with_a_body_less_request() {
    let r = rig();
    assert_eq!(
        r.provider.account_email().await.unwrap(),
        "teacher@example.org"
    );
    r.dbx.set_account_email("other@example.org");
    assert_eq!(
        r.provider.account_email().await.unwrap(),
        "other@example.org"
    );
    assert!(r.dbx.violations().is_empty(), "{:?}", r.dbx.violations());

    r.dbx.set_valid_token("other");
    r.tokens.revoke_on_reject();
    assert!(matches!(
        r.provider.account_email().await,
        Err(CmError::AuthExpired { .. })
    ));
}

#[tokio::test(start_paused = true)]
async fn test_connection_uploads_and_deletes_a_file() {
    let r = rig();
    r.provider.test_connection().await.unwrap();
    assert!(r.dbx.files().is_empty(), "the test file is gone");
    assert_eq!(r.dbx.count(Call::Delete), 1);
    assert!(r.dbx.links().is_empty(), "no link for the test file");

    r.dbx.set_valid_token("other");
    r.tokens.revoke_on_reject();
    assert!(matches!(
        r.provider.test_connection().await,
        Err(CmError::AuthExpired { .. })
    ));
}
