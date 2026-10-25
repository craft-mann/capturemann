//! The product's token source (`KeychainTokens`) under the `upload` job (STO-2.AC5, STO-3): tokens
//! and client credentials come from a secret store (in memory here, never the real keychain), the
//! sign-in server and Drive are fakes. Tested with fakes: no network, no account.

#![allow(clippy::unwrap_used)]

mod common;

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use cm_auth::credentials::save_credentials;
use cm_auth::{
    ClientCredentials, HttpClient, HttpError, HttpResponse, MemoryStore, ProviderConfig,
    SecretStore, TokenManager, TokenSet,
};
use cm_core::Actor;
use cm_daemon::storage_cloud::{CloudBackend, KeychainTokens};
use cm_daemon::Daemon;
use cm_storage::fake_drive::FakeDrive;
use cm_storage::{
    CloudAccount, Destination, DestinationStore, Kind, LinkMode, What, DEFAULT_FOLDER_PATTERN,
};
use common::{builder, desktop};
use serde_json::{json, Value};

const ACCOUNT: &str = "gdrive-1";

/// The sign-in server: answers the refresh requests in order and counts them.
struct SignIn {
    answers: Mutex<VecDeque<(u16, String)>>,
    calls: AtomicUsize,
}

impl SignIn {
    fn new(answers: &[(u16, &str)]) -> Arc<Self> {
        Arc::new(SignIn {
            answers: Mutex::new(answers.iter().map(|(s, b)| (*s, (*b).to_owned())).collect()),
            calls: AtomicUsize::new(0),
        })
    }

    fn push(&self, status: u16, body: &str) {
        self.answers
            .lock()
            .unwrap()
            .push_back((status, body.to_owned()));
    }
}

#[async_trait]
impl HttpClient for SignIn {
    async fn post_form(
        &self,
        _url: &str,
        _form: &[(String, String)],
    ) -> Result<HttpResponse, HttpError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let (status, body) = self
            .answers
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or((500, String::new()));
        Ok(HttpResponse { status, body })
    }
}

fn fresh(token: &str) -> String {
    format!(r#"{{"access_token":"{token}","expires_in":3600}}"#)
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

/// A connected account: tokens (valid for `valid_for` seconds, may be negative) and credentials.
fn connect(store: &Arc<MemoryStore>, access: &str, valid_for: i64) {
    let manager = TokenManager::new(
        ProviderConfig::google_drive(),
        ClientCredentials {
            client_id: "cid".into(),
            client_secret: Some("csecret".into()),
        },
        SignIn::new(&[]),
        store.clone(),
    );
    manager
        .save(
            ACCOUNT,
            &TokenSet {
                access_token: access.into(),
                refresh_token: Some("refresh-1".into()),
                expires_at: now() + valid_for,
                scopes: vec![],
                account_label: Some("teacher@example.org".into()),
            },
        )
        .unwrap();
    save_credentials(
        store.as_ref(),
        "google",
        ACCOUNT,
        &ClientCredentials {
            client_id: "cid".into(),
            client_secret: Some("csecret".into()),
        },
    )
    .unwrap();
}

fn drive_dest() -> Destination {
    Destination {
        id: "drive".into(),
        name: "School Drive".into(),
        kind: Kind::GoogleDrive,
        path: PathBuf::new(),
        account: Some(CloudAccount {
            account: ACCOUNT.into(),
            label: "teacher@example.org".into(),
            folder_id: None,
            folder_name: "CaptureMann".into(),
            link_mode: LinkMode::Anyone,
            app_folder: false,
        }),
        folder_pattern: DEFAULT_FOLDER_PATTERN.into(),
        what: What::Export,
        auto_upload: cm_storage::AutoUpload::Off,
        default: true,
    }
}

struct Rig {
    daemon: Daemon,
    drive: Arc<FakeDrive>,
    store: Arc<MemoryStore>,
    signin: Arc<SignIn>,
    tokens: Arc<KeychainTokens>,
    tmp: tempfile::TempDir,
    _lib: tempfile::TempDir,
}

async fn rig(signin: &[(u16, &str)]) -> Rig {
    let tmp = tempfile::tempdir().unwrap();
    let dests = DestinationStore::new(tmp.path().join("config"));
    dests.save(&[drive_dest()]).unwrap();
    let drive = Arc::new(FakeDrive::new());
    let store = Arc::new(MemoryStore::new());
    let signin = SignIn::new(signin);
    let tokens = KeychainTokens::new(store.clone(), signin.clone());
    let backend = CloudBackend {
        tokens: Arc::new(tokens.clone()),
        http: drive.clone(),
    };
    let (b, lib) = builder(desktop());
    let queue = cm_daemon::library_job_queue(lib.path()).unwrap();
    let daemon = b
        .jobs(queue)
        .destinations(dests)
        .cloud_backend(backend)
        .build();
    daemon
        .register_job_handler(
            cm_jobs::JOB_UPLOAD,
            daemon.upload_handler(Duration::from_millis(20)),
        )
        .unwrap();
    daemon.start_jobs().unwrap();
    Rig {
        daemon,
        drive,
        store,
        signin,
        tokens,
        tmp,
        _lib: lib,
    }
}

async fn start(daemon: &Daemon, file: &Path) -> String {
    let v = daemon
        .execute_method(
            Actor::User,
            0,
            "upload",
            &json!({ "file": file, "folder": "CaptureMann/Lessons" }),
        )
        .await
        .unwrap();
    v["job_id"].as_str().unwrap().to_owned()
}

/// Polls until the job is done, failed, cancelled or waiting for a sign-in.
async fn settle(daemon: &Daemon, id: &str) -> Value {
    for _ in 0..500 {
        let j = daemon
            .execute_method(Actor::User, 0, "get_job", &json!({ "job_id": id }))
            .await
            .unwrap();
        let waiting = j["state"] == "paused" && j["pause_reason"] == "auth_expired";
        if waiting || matches!(j["state"].as_str(), Some("done" | "failed" | "cancelled")) {
            return j;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("the upload job did not settle");
}

fn lesson(r: &Rig) -> PathBuf {
    let src = r.tmp.path().join("lesson.mp4");
    std::fs::write(&src, vec![3u8; 300_000]).unwrap();
    src
}

#[tokio::test]
async fn an_expired_token_is_renewed_and_stored_before_the_upload() {
    let r = rig(&[(200, &fresh("tok-2"))]).await;
    connect(&r.store, "tok-1", -10);
    r.drive.set_valid_token("tok-2");
    let id = start(&r.daemon, &lesson(&r)).await;
    let j = settle(&r.daemon, &id).await;
    assert_eq!(j["state"], "done", "{j}");
    assert_eq!(r.signin.calls.load(Ordering::SeqCst), 1);
    let stored = r.store.get("oauth.google.gdrive-1").unwrap().unwrap();
    assert!(stored.contains("tok-2"), "the renewed token is kept");
    assert_eq!(r.drive.files().len(), 1);
}

#[tokio::test]
async fn a_rejected_token_is_renewed_once_even_though_it_has_not_expired() {
    // The clock says the token is good for an hour; Drive says 401: one forced refresh.
    let r = rig(&[(200, &fresh("tok-2"))]).await;
    connect(&r.store, "tok-1", 3600);
    r.drive.set_valid_token("tok-2");
    let id = start(&r.daemon, &lesson(&r)).await;
    let j = settle(&r.daemon, &id).await;
    assert_eq!(j["state"], "done", "{j}");
    assert_eq!(r.signin.calls.load(Ordering::SeqCst), 1, "renewed once");
    assert_eq!(r.drive.files().len(), 1);
}

#[tokio::test]
async fn a_revoked_grant_waits_and_a_reconnect_continues_the_job() {
    // STO-2.AC5: the refresh is refused (`invalid_grant`), the job waits as auth_expired; when
    // the account is connected again the app resumes it and it completes.
    let r = rig(&[(400, r#"{"error":"invalid_grant"}"#)]).await;
    connect(&r.store, "tok-1", 3600);
    r.drive.set_valid_token("other");
    let id = start(&r.daemon, &lesson(&r)).await;
    let j = settle(&r.daemon, &id).await;
    assert_eq!(j["state"], "paused", "{j}");
    assert_eq!(j["pause_reason"], "auth_expired", "{j}");
    assert!(r.drive.files().is_empty());
    assert!(!j.to_string().contains("refresh-1"), "no secret in the job");

    // The user signs in again: new tokens and credentials, then the app resumes the jobs.
    connect(&r.store, "tok-3", 3600);
    r.tokens.invalidate(ACCOUNT);
    r.drive.set_valid_token("tok-3");
    assert_eq!(r.daemon.resume_auth_expired_jobs(), 1);
    let j = settle(&r.daemon, &id).await;
    assert_eq!(j["state"], "done", "{j}");
    assert_eq!(r.drive.files().len(), 1);
}

#[tokio::test]
async fn a_client_the_provider_no_longer_knows_is_auth_expired_not_a_failure() {
    let r = rig(&[(401, r#"{"error":"invalid_client"}"#)]).await;
    connect(&r.store, "tok-1", -10);
    let id = start(&r.daemon, &lesson(&r)).await;
    let j = settle(&r.daemon, &id).await;
    assert_eq!(j["state"], "paused", "{j}");
    assert_eq!(j["pause_reason"], "auth_expired", "{j}");
}

#[tokio::test]
async fn an_unreachable_sign_in_server_is_retried_not_paused() {
    let r = rig(&[(503, "down")]).await;
    connect(&r.store, "tok-1", -10);
    r.drive.set_valid_token("tok-2");
    let id = start(&r.daemon, &lesson(&r)).await;
    // The first attempt hits the 503 (a `network` error the queue retries later); the sign-in
    // server then answers again.
    r.signin.push(200, &fresh("tok-2"));
    for _ in 0..50 {
        let j = r
            .daemon
            .execute_method(Actor::User, 0, "get_job", &json!({ "job_id": id }))
            .await
            .unwrap();
        assert_ne!(j["pause_reason"], "auth_expired", "{j}");
        if j["state"] == "done" {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("the job did not finish after the sign-in server came back");
}

#[tokio::test]
async fn missing_credentials_or_tokens_are_auth_expired() {
    let r = rig(&[]).await;
    // Nothing stored at all.
    let id = start(&r.daemon, &lesson(&r)).await;
    let j = settle(&r.daemon, &id).await;
    assert_eq!(j["pause_reason"], "auth_expired", "{j}");
    // Tokens without the client credentials (a damaged keychain).
    connect(&r.store, "tok-1", 3600);
    r.store.delete("oauth.google.gdrive-1.client").unwrap();
    r.tokens.invalidate(ACCOUNT);
    let id = start(&r.daemon, &lesson(&r)).await;
    let j = settle(&r.daemon, &id).await;
    assert_eq!(j["pause_reason"], "auth_expired", "{j}");
}

// ---- the client ID behind an account (daily YouTube uploads are counted per project) ---------

#[test]
fn the_client_id_of_an_account_comes_from_the_secret_store_and_never_leaks_the_secret() {
    use cm_daemon::storage_cloud::AccountTokens;
    let store = Arc::new(MemoryStore::new());
    let tokens = KeychainTokens::new(store.clone(), SignIn::new(&[]));
    // Not connected: no client.
    assert_eq!(tokens.client_id("yt-a"), None);
    save_credentials(
        store.as_ref(),
        "google",
        "yt-b",
        &ClientCredentials {
            client_id: "123456789012-abc.apps.googleusercontent.com".into(),
            client_secret: Some("never-shown".into()),
        },
    )
    .unwrap();
    assert_eq!(
        tokens.client_id("yt-b").as_deref(),
        Some("123456789012-abc.apps.googleusercontent.com")
    );
    // Remembered until the account is invalidated (connected again, or removed).
    save_credentials(
        store.as_ref(),
        "google",
        "yt-b",
        &ClientCredentials {
            client_id: "999-new.apps.googleusercontent.com".into(),
            client_secret: None,
        },
    )
    .unwrap();
    assert_eq!(
        tokens.client_id("yt-b").as_deref(),
        Some("123456789012-abc.apps.googleusercontent.com")
    );
    tokens.invalidate("yt-b");
    assert_eq!(
        tokens.client_id("yt-b").as_deref(),
        Some("999-new.apps.googleusercontent.com")
    );
    // A miss is remembered too (no keychain read on every call) until invalidated.
    assert_eq!(tokens.client_id("yt-a"), None);
    save_credentials(
        store.as_ref(),
        "google",
        "yt-a",
        &ClientCredentials {
            client_id: "1-x.apps.googleusercontent.com".into(),
            client_secret: None,
        },
    )
    .unwrap();
    assert_eq!(tokens.client_id("yt-a"), None);
    tokens.invalidate("yt-a");
    assert!(tokens.client_id("yt-a").is_some());
}
