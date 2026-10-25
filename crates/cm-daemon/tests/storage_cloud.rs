//! Google Drive, OneDrive and Dropbox destinations through the daemon (STO-2), against the in-process fake server:
//! the `upload` job picks the provider from the destination kind, returns the link, and fails
//! readably as `auth_expired` when the account is not connected or was revoked. Tested with fakes.

#![allow(clippy::unwrap_used)]

mod common;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use cm_core::Actor;
use cm_daemon::storage_cloud::{AccountTokens, CloudBackend};
use cm_daemon::Daemon;
use cm_storage::fake_drive::{FakeDrive, FakeTokens};
use cm_storage::fake_dropbox::FakeDropbox;
use cm_storage::fake_onedrive::FakeOneDrive;
use cm_storage::{
    CloudAccount, Destination, DestinationStore, Kind, LinkMode, TokenSource, What,
    DEFAULT_FOLDER_PATTERN,
};
use common::{builder, desktop};
use serde_json::{json, Value};

/// Hands out the scripted tokens, and checks the destination asked for its own account key.
struct Tokens(Arc<FakeTokens>, &'static str);

impl AccountTokens for Tokens {
    fn source(&self, account: &str) -> Arc<dyn TokenSource> {
        assert_eq!(account, self.1);
        Arc::new(Shared(self.0.clone()))
    }
}

struct Shared(Arc<FakeTokens>);

#[async_trait]
impl TokenSource for Shared {
    async fn access_token(&self, rejected: Option<&str>) -> Result<String, cm_core::CmError> {
        self.0.access_token(rejected).await
    }
}

fn drive_dest() -> Destination {
    cloud_dest("drive", "School Drive", Kind::GoogleDrive, "gdrive-1")
}

fn cloud_dest(id: &str, name: &str, kind: Kind, account: &str) -> Destination {
    Destination {
        id: id.into(),
        name: name.into(),
        kind,
        path: PathBuf::new(),
        account: Some(CloudAccount {
            account: account.into(),
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

async fn setup(backend: Option<CloudBackend>) -> (Daemon, tempfile::TempDir, tempfile::TempDir) {
    setup_with(backend, drive_dest()).await
}

async fn setup_with(
    backend: Option<CloudBackend>,
    dest: Destination,
) -> (Daemon, tempfile::TempDir, tempfile::TempDir) {
    let tmp = tempfile::tempdir().unwrap();
    let store = DestinationStore::new(tmp.path().join("config"));
    store.save(&[dest]).unwrap();
    let (mut b, lib) = builder(desktop());
    let queue = cm_daemon::library_job_queue(lib.path()).unwrap();
    b = b.jobs(queue).destinations(store);
    if let Some(c) = backend {
        b = b.cloud_backend(c);
    }
    let daemon = b.build();
    daemon
        .register_job_handler(
            cm_jobs::JOB_UPLOAD,
            daemon.upload_handler(Duration::from_millis(20)),
        )
        .unwrap();
    daemon.start_jobs().unwrap();
    (daemon, tmp, lib)
}

async fn run(daemon: &Daemon, file: &Path) -> Value {
    let v = daemon
        .execute_method(
            Actor::User,
            0,
            "upload",
            &json!({ "file": file, "folder": "CaptureMann/Lessons" }),
        )
        .await
        .unwrap();
    let id = v["job_id"].as_str().unwrap().to_owned();
    for _ in 0..500 {
        let j = daemon
            .execute_method(Actor::User, 0, "get_job", &json!({ "job_id": id }))
            .await
            .unwrap();
        // An expired sign-in does not fail the job: it waits, paused, for the account to be
        // reconnected (STO-2.AC5, STO-3).
        let waiting = j["state"] == "paused" && j["pause_reason"] == "auth_expired";
        if waiting || matches!(j["state"].as_str(), Some("done" | "failed" | "cancelled")) {
            return j;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("the upload job did not finish");
}

#[tokio::test]
async fn the_job_uploads_to_drive_and_returns_the_link() {
    let fake = Arc::new(FakeDrive::new());
    let tokens = Arc::new(FakeTokens::new("tok-1"));
    let backend = CloudBackend {
        tokens: Arc::new(Tokens(tokens, "gdrive-1")),
        http: fake.clone(),
    };
    let (daemon, tmp, _lib) = setup(Some(backend)).await;
    let src = tmp.path().join("lesson.mp4");
    std::fs::write(&src, vec![7u8; 700_000]).unwrap();
    let j = run(&daemon, &src).await;
    assert_eq!(j["state"], "done", "{j}");
    assert_eq!(j["result"]["bytes"], 700_000);
    assert!(j["result"]["share_link"]
        .as_str()
        .unwrap()
        .starts_with("https://drive.fake/"));
    let files = fake.files();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].data.len(), 700_000);
    assert_eq!(fake.path_of(&files[0].parent), "CaptureMann/Lessons");
}

#[tokio::test]
async fn without_a_connected_account_the_job_waits_as_auth_expired() {
    // The default backend has no account: a readable reason, not a silent failure.
    let (daemon, tmp, _lib) = setup(None).await;
    let src = tmp.path().join("lesson.mp4");
    std::fs::write(&src, b"x").unwrap();
    let j = run(&daemon, &src).await;
    assert_eq!(j["state"], "paused", "{j}");
    assert_eq!(j["pause_reason"], "auth_expired", "{j}");
    let text = j["error"].to_string();
    assert!(text.contains("auth_expired"), "{text}");
    assert!(text.contains("Reconnect"), "{text}");
    assert!(text.contains("School Drive"), "{text}");
}

#[tokio::test]
async fn a_revoked_token_pauses_the_job_readably() {
    // STO-2.AC5: the server says 401 and the grant cannot be renewed.
    let fake = Arc::new(FakeDrive::new());
    fake.set_valid_token("other");
    let tokens = Arc::new(FakeTokens::new("tok-1"));
    tokens.revoke_on_reject();
    let backend = CloudBackend {
        tokens: Arc::new(Tokens(tokens, "gdrive-1")),
        http: fake.clone(),
    };
    let (daemon, tmp, _lib) = setup(Some(backend)).await;
    let src = tmp.path().join("lesson.mp4");
    std::fs::write(&src, b"x").unwrap();
    let j = run(&daemon, &src).await;
    assert_eq!(j["state"], "paused", "{j}");
    assert_eq!(j["pause_reason"], "auth_expired", "{j}");
    assert!(j["error"].to_string().contains("auth_expired"), "{j}");
    assert!(fake.files().is_empty());
}

#[tokio::test]
async fn list_destinations_shows_the_account_label_and_nothing_secret() {
    let (daemon, _tmp, _lib) = setup(None).await;
    let v = daemon
        .execute_method(Actor::User, 0, "list_destinations", &json!({}))
        .await
        .unwrap();
    let d = &v["destinations"][0];
    assert_eq!(d["kind"], "google_drive");
    assert_eq!(d["account"]["label"], "teacher@example.org");
    assert_eq!(d["account"]["link_mode"], "anyone");
    let text = v.to_string();
    assert!(
        !text.contains("gdrive-1"),
        "the key name stays private: {text}"
    );
    assert!(!text.contains("token") && !text.contains("secret"));
}

// ---- OneDrive and Dropbox (STO-2) ------------------------------------------------------------

fn onedrive_dest() -> Destination {
    cloud_dest("od", "School OneDrive", Kind::OneDrive, "onedrive-1")
}

fn dropbox_dest(app_folder: bool) -> Destination {
    let mut d = cloud_dest("dbx", "School Dropbox", Kind::Dropbox, "dropbox-1");
    d.account.as_mut().unwrap().app_folder = app_folder;
    d
}

#[tokio::test]
async fn the_job_uploads_to_onedrive_and_returns_the_link() {
    let fake = Arc::new(FakeOneDrive::new());
    let backend = CloudBackend {
        tokens: Arc::new(Tokens(Arc::new(FakeTokens::new("tok-1")), "onedrive-1")),
        http: fake.clone(),
    };
    let (daemon, tmp, _lib) = setup_with(Some(backend), onedrive_dest()).await;
    // Bigger than the 4 MB simple PUT: an upload session with 10 MiB chunks.
    let src = tmp.path().join("lesson.mp4");
    std::fs::write(&src, vec![7u8; 5_000_000]).unwrap();
    let j = run(&daemon, &src).await;
    assert_eq!(j["state"], "done", "{j}");
    assert_eq!(j["result"]["bytes"], 5_000_000);
    assert!(j["result"]["share_link"]
        .as_str()
        .unwrap()
        .starts_with("https://1drv.fake/anonymous/"));
    let files = fake.files();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].data.len(), 5_000_000);
    assert_eq!(fake.path_of(&files[0].parent), "CaptureMann/Lessons");
    assert_eq!(fake.count(cm_storage::fake_onedrive::Call::Init), 1);
    assert!(fake.violations().is_empty());
}

#[tokio::test]
async fn the_job_uploads_to_dropbox_and_returns_the_link() {
    let fake = Arc::new(FakeDropbox::new());
    let backend = CloudBackend {
        tokens: Arc::new(Tokens(Arc::new(FakeTokens::new("tok-1")), "dropbox-1")),
        http: fake.clone(),
    };
    let (daemon, tmp, _lib) = setup_with(Some(backend), dropbox_dest(false)).await;
    let src = tmp.path().join("lesson.mp4");
    std::fs::write(&src, vec![7u8; 700_000]).unwrap();
    let j = run(&daemon, &src).await;
    assert_eq!(j["state"], "done", "{j}");
    assert_eq!(j["result"]["bytes"], 700_000);
    assert!(j["result"]["share_link"]
        .as_str()
        .unwrap()
        .starts_with("https://dbx.fake/"));
    let files = fake.files();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].data.len(), 700_000);
    assert_eq!(files[0].path, "/CaptureMann/Lessons/lesson.mp4");
}

#[tokio::test]
async fn an_app_folder_dropbox_destination_uses_the_app_folder_as_root() {
    let fake = Arc::new(FakeDropbox::new());
    let backend = CloudBackend {
        tokens: Arc::new(Tokens(Arc::new(FakeTokens::new("tok-1")), "dropbox-1")),
        http: fake.clone(),
    };
    let (daemon, tmp, _lib) = setup_with(Some(backend), dropbox_dest(true)).await;
    let src = tmp.path().join("lesson.mp4");
    std::fs::write(&src, b"hello").unwrap();
    let j = run(&daemon, &src).await;
    assert_eq!(j["state"], "done", "{j}");
    assert_eq!(fake.files()[0].path, "/Lessons/lesson.mp4");
}

#[tokio::test]
async fn revoked_onedrive_and_dropbox_tokens_pause_the_job_as_auth_expired() {
    // STO-2.AC5 for both providers.
    let od = Arc::new(FakeOneDrive::new());
    od.set_valid_token("other");
    let tokens = Arc::new(FakeTokens::new("tok-1"));
    tokens.revoke_on_reject();
    let backend = CloudBackend {
        tokens: Arc::new(Tokens(tokens, "onedrive-1")),
        http: od.clone(),
    };
    let (daemon, tmp, _lib) = setup_with(Some(backend), onedrive_dest()).await;
    let src = tmp.path().join("lesson.mp4");
    std::fs::write(&src, b"x").unwrap();
    let j = run(&daemon, &src).await;
    assert_eq!(j["state"], "paused", "{j}");
    assert_eq!(j["pause_reason"], "auth_expired", "{j}");
    assert!(od.files().is_empty());

    let dbx = Arc::new(FakeDropbox::new());
    dbx.set_valid_token("other");
    let tokens = Arc::new(FakeTokens::new("tok-1"));
    tokens.revoke_on_reject();
    let backend = CloudBackend {
        tokens: Arc::new(Tokens(tokens, "dropbox-1")),
        http: dbx.clone(),
    };
    let (daemon, tmp, _lib) = setup_with(Some(backend), dropbox_dest(false)).await;
    let src = tmp.path().join("lesson.mp4");
    std::fs::write(&src, b"x").unwrap();
    let j = run(&daemon, &src).await;
    assert_eq!(j["state"], "paused", "{j}");
    assert_eq!(j["pause_reason"], "auth_expired", "{j}");
    assert!(dbx.files().is_empty());
}

#[tokio::test]
async fn onedrive_and_dropbox_without_an_account_wait_as_auth_expired() {
    for dest in [onedrive_dest(), dropbox_dest(false)] {
        let (daemon, tmp, _lib) = setup_with(None, dest).await;
        let src = tmp.path().join("lesson.mp4");
        std::fs::write(&src, b"x").unwrap();
        let j = run(&daemon, &src).await;
        assert_eq!(j["state"], "paused", "{j}");
        assert_eq!(j["pause_reason"], "auth_expired", "{j}");
    }
}

#[tokio::test]
async fn list_destinations_shows_onedrive_and_dropbox_without_secrets() {
    let tmp = tempfile::tempdir().unwrap();
    let store = DestinationStore::new(tmp.path().join("config"));
    let mut d2 = dropbox_dest(true);
    d2.default = false;
    let mut d1 = onedrive_dest();
    d1.account.as_mut().unwrap().link_mode = LinkMode::Organization;
    d1.default = false;
    store.save(&[drive_dest(), d1, d2]).unwrap();
    let (b, _lib) = builder(desktop());
    let daemon = b.destinations(store).build();
    let v = daemon
        .execute_method(Actor::User, 0, "list_destinations", &json!({}))
        .await
        .unwrap();
    let list = v["destinations"].as_array().unwrap();
    let kinds: Vec<&str> = list.iter().map(|d| d["kind"].as_str().unwrap()).collect();
    assert_eq!(kinds, ["google_drive", "onedrive", "dropbox"]);
    assert_eq!(list[1]["account"]["link_mode"], "organization");
    assert_eq!(list[2]["account"]["app_folder"], true);
    assert_eq!(list[1]["account"]["label"], "teacher@example.org");
    assert!(list.iter().all(|d| d["online"] == true));
    let text = v.to_string();
    for key in ["gdrive-1", "onedrive-1", "dropbox-1", "token", "secret"] {
        assert!(!text.contains(key), "{key} leaked: {text}");
    }
}
