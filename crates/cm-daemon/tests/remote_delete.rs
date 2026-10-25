//! `delete_capture { also_delete_remote: true }` through the daemon (LIB-1, STO-4, PUB-2): the
//! dialog lists every remote copy, the copies are deleted (Google Drive files and folders, local
//! copies, YouTube videos) and then the capture goes to the Trash; a failed step keeps the
//! capture and says what was and was not deleted. Also: a second Google Drive bundle copy gets a
//! fresh folder (STO-5), and a restarted bundle upload continues in the folder it claimed.
//! Everything runs against in-process fakes: no network, no account, no keychain, no real Trash.
//! Tested with fakes.

#![allow(clippy::unwrap_used)]

mod common;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use cm_core::{Actor, CmError, ErrorCode, MediaTime, TrackInfo, TrackKind, TrackStats};
use cm_daemon::storage_cloud::{AccountTokens, CloudBackend};
use cm_daemon::testing::{FakeTrasher, FixedConfirmer};
use cm_daemon::{ConfirmationRequest, Confirmer, Daemon, DaemonError, Decision};
use cm_library::rec::{RecBundle, SessionInfo};
use cm_publish::fake_youtube::{Call as YtCall, FakeYouTube, Fault as YtFault};
use cm_publish::{
    Privacy, Progress, PublishAccount, PublishAccountStore, PublishMeta, PublishProvider,
    YouTubeConfig, YouTubeProvider,
};
use cm_storage::fake_drive::{Call as DriveCall, FakeDrive, FakeTokens, Fault as DriveFault};
use cm_storage::http::{HttpReply, HttpRequest, StorageHttp, TransportError};
use cm_storage::{
    AutoUpload, CloudAccount, Destination, DestinationStore, Kind, LinkMode, TokenSource, What,
    DEFAULT_FOLDER_PATTERN,
};
use common::{builder, desktop};
use serde_json::{json, Map, Value};
use time::macros::datetime;

const GDRIVE: &str = "gdrive-1";
const YT: &str = "yt1";

/// Sends Google Drive calls to the fake Drive and YouTube calls to the fake YouTube.
struct Router {
    drive: Arc<FakeDrive>,
    yt: Arc<FakeYouTube>,
}

#[async_trait]
impl StorageHttp for Router {
    async fn send(&self, req: HttpRequest) -> Result<HttpReply, TransportError> {
        if req.url.contains("/youtube/v3") {
            self.yt.send(req).await
        } else {
            self.drive.send(req).await
        }
    }
}

struct Shared(Arc<FakeTokens>);

#[async_trait]
impl TokenSource for Shared {
    async fn access_token(&self, rejected: Option<&str>) -> Result<String, CmError> {
        self.0.access_token(rejected).await
    }
}

struct Tokens {
    drive: Arc<FakeTokens>,
    yt: Arc<FakeTokens>,
}

impl AccountTokens for Tokens {
    fn source(&self, account: &str) -> Arc<dyn TokenSource> {
        match account {
            GDRIVE => Arc::new(Shared(self.drive.clone())),
            YT => Arc::new(Shared(self.yt.clone())),
            other => panic!("unexpected account key {other}"),
        }
    }
}

/// Runs a closure when the dialog opens (a change of the world while the user is deciding), then
/// answers like a [`FixedConfirmer`].
struct Hook {
    inner: Arc<FixedConfirmer>,
    on_dialog: Mutex<Option<Box<dyn FnOnce() + Send>>>,
}

#[async_trait]
impl Confirmer for Hook {
    async fn confirm(&self, request: &ConfirmationRequest) -> Decision {
        let f = self.on_dialog.lock().unwrap().take();
        if let Some(f) = f {
            f();
        }
        self.inner.confirm(request).await
    }
}

struct Rig {
    daemon: Daemon,
    confirmer: Arc<FixedConfirmer>,
    trasher: Arc<FakeTrasher>,
    drive: Arc<FakeDrive>,
    yt: Arc<FakeYouTube>,
    drive_tokens: Arc<FakeTokens>,
    yt_tokens: Arc<FakeTokens>,
    dests: DestinationStore,
    nas: PathBuf,
    id: String,
    dir: PathBuf,
    export: PathBuf,
    cfg: tempfile::TempDir,
    _lib: tempfile::TempDir,
    _trash: tempfile::TempDir,
}

fn drive_dest() -> Destination {
    Destination {
        id: "drive".into(),
        name: "School Drive".into(),
        kind: Kind::GoogleDrive,
        path: PathBuf::new(),
        account: Some(CloudAccount {
            account: GDRIVE.into(),
            label: "teacher@example.org".into(),
            folder_id: None,
            folder_name: "CaptureMann".into(),
            link_mode: LinkMode::None,
            app_folder: false,
        }),
        folder_pattern: DEFAULT_FOLDER_PATTERN.into(),
        what: What::Both,
        auto_upload: AutoUpload::Off,
        default: false,
    }
}

fn nas_dest(path: &Path) -> Destination {
    Destination {
        id: "nas".into(),
        name: "Office NAS".into(),
        kind: Kind::Local,
        path: path.to_path_buf(),
        account: None,
        folder_pattern: DEFAULT_FOLDER_PATTERN.into(),
        what: What::Both,
        auto_upload: AutoUpload::Off,
        default: true,
    }
}

fn session() -> SessionInfo {
    SessionInfo {
        clock_origin_ns: 1,
        timebase: "uptime_raw_ns".into(),
        displays: vec![],
        devices: json!({}),
        app_version: "0.0.1".into(),
        extra: Map::new(),
    }
}

fn agent() -> Actor {
    Actor::agent("claude").unwrap()
}

fn code(e: &DaemonError) -> ErrorCode {
    e.code().unwrap()
}

fn yt_config() -> YouTubeConfig {
    YouTubeConfig {
        max_retries: 0,
        backoff_base: Duration::from_millis(5),
        ..YouTubeConfig::default()
    }
}

/// A rig with a recording that has an export, a local NAS and a Google Drive destination, one
/// YouTube channel, and nothing sent anywhere yet.
async fn rig() -> Rig {
    rig_with(|b| b, None).await
}

async fn rig_with(
    tweak: impl FnOnce(cm_daemon::DaemonBuilder) -> cm_daemon::DaemonBuilder,
    hook: Option<Box<dyn FnOnce() + Send>>,
) -> Rig {
    let cfg = tempfile::tempdir().unwrap();
    let nas = cfg.path().join("nas");
    std::fs::create_dir(&nas).unwrap();
    let dests = DestinationStore::new(cfg.path().join("config"));
    dests.save(&[nas_dest(&nas), drive_dest()]).unwrap();
    let accounts = PublishAccountStore::new(cfg.path().join("config"));
    accounts
        .save(&[PublishAccount {
            id: YT.into(),
            platform: "youtube".into(),
            name: "Teacher Channel".into(),
            account_id: "UC-channel".into(),
            default_privacy: Privacy::Unlisted,
        }])
        .unwrap();

    let drive = Arc::new(FakeDrive::new());
    let yt = Arc::new(FakeYouTube::new());
    let drive_tokens = Arc::new(FakeTokens::new("tok-1"));
    let yt_tokens = Arc::new(FakeTokens::new("tok-1"));
    let confirmer = Arc::new(FixedConfirmer::new(Decision::Approve));
    let trash = tempfile::tempdir().unwrap();
    let trasher = Arc::new(FakeTrasher::into_dir(trash.path()));

    let (b, lib) = builder(desktop());
    let queue = cm_daemon::library_job_queue(lib.path()).unwrap();
    let mut b = b
        .trasher(trasher.clone())
        .jobs(queue)
        .destinations(dests.clone())
        .publish_accounts(accounts.clone())
        .cloud_backend(CloudBackend {
            tokens: Arc::new(Tokens {
                drive: drive_tokens.clone(),
                yt: yt_tokens.clone(),
            }),
            http: Arc::new(Router {
                drive: drive.clone(),
                yt: yt.clone(),
            }),
        })
        .youtube_config(yt_config());
    b = match hook {
        Some(f) => b.confirmer(Hook {
            inner: confirmer.clone(),
            on_dialog: Mutex::new(Some(f)),
        }),
        None => b.confirmer(confirmer.clone()),
    };
    let daemon = tweak(b).build();
    daemon
        .register_job_handler(
            cm_jobs::JOB_UPLOAD,
            daemon.upload_handler(Duration::from_millis(20)),
        )
        .unwrap();
    daemon.start_jobs().unwrap();

    let now = datetime!(2026-10-22 10:00:00 UTC);
    let rec = RecBundle::create(lib.path(), "Lesson", now, json!({}), &session()).unwrap();
    std::fs::write(rec.dir().join("screen.mp4"), b"master bytes").unwrap();
    let dir = rec.dir().to_path_buf();
    let info = TrackInfo {
        kind: TrackKind::Screen,
        file: "screen.mp4".into(),
        start: MediaTime::ZERO,
        end: MediaTime::from_secs(60),
        details: json!({}),
        stats: TrackStats::default(),
    };
    let m = rec.finalize(&[info], now).unwrap();
    std::fs::create_dir_all(dir.join("exports")).unwrap();
    let export = dir.join("exports/Lesson - quick.mp4");
    std::fs::write(&export, vec![9u8; 300_000]).unwrap();
    cm_library::record_export(
        &dir,
        json!({ "file": "exports/Lesson - quick.mp4", "preset": "quick" }),
        Actor::User,
        datetime!(2026-10-22 11:00:00 UTC),
    )
    .unwrap();
    daemon
        .execute_method(Actor::User, 0, "rescan_library", &json!({}))
        .await
        .unwrap();
    Rig {
        daemon,
        confirmer,
        trasher,
        drive,
        yt,
        drive_tokens,
        yt_tokens,
        dests,
        nas,
        id: m.id,
        dir,
        export,
        cfg,
        _lib: lib,
        _trash: trash,
    }
}

async fn call(r: &Rig, who: Actor, method: &str, p: Value) -> Result<Value, DaemonError> {
    r.daemon.execute_method(who, 0, method, &p).await
}

async fn wait_done(r: &Rig, job_id: &str) -> Value {
    for _ in 0..500 {
        let j = call(r, Actor::User, "get_job", json!({ "job_id": job_id }))
            .await
            .unwrap();
        if matches!(j["state"].as_str(), Some("done" | "failed" | "cancelled")) {
            return j;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("the upload job did not finish");
}

/// Uploads the export and the bundle (`what: both`) and waits for the job.
async fn upload(r: &Rig, destination: &str) -> Value {
    let v = call(
        r,
        Actor::User,
        "upload",
        json!({ "capture_id": r.id, "destination_id": destination, "what": "both" }),
    )
    .await
    .unwrap();
    let j = wait_done(r, v["job_id"].as_str().unwrap()).await;
    assert_eq!(j["state"], "done", "{j}");
    j
}

/// Publishes the export to the fake YouTube and records it like the publish job does.
async fn publish(r: &Rig, title: &str) -> String {
    let provider = YouTubeProvider::new(
        r.yt.clone(),
        Arc::new(Shared(r.yt_tokens.clone())),
        yt_config(),
    );
    let meta = PublishMeta {
        title: title.into(),
        made_for_kids: Some(false),
        privacy: Some(Privacy::Unlisted),
        ..PublishMeta::default()
    };
    let done = provider
        .publish(&r.export, &meta, None, Progress::none())
        .await
        .unwrap();
    cm_library::record_publish(
        &r.dir,
        json!({
            "platform": "youtube", "account_id": YT, "account": "Teacher Channel",
            "remote_id": done.remote_id, "url": done.url, "title": title,
            "privacy": "unlisted", "status": "processing",
        }),
    )
    .unwrap();
    done.remote_id
}

/// A rig with everything sent: the export and the bundle to Drive and to the NAS, and a video.
async fn full_rig() -> (Rig, String) {
    let r = rig().await;
    upload(&r, "drive").await;
    upload(&r, "nas").await;
    let video = publish(&r, "How the editor works").await;
    (r, video)
}

fn all_text(details: &Value) -> String {
    details
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| format!("{k}: {}", v.as_str().unwrap_or("")))
        .collect::<Vec<_>>()
        .join("\n")
}

fn delete_params(r: &Rig) -> Value {
    json!({ "capture_id": r.id, "also_delete_remote": true })
}

fn local_files(root: &Path) -> Vec<String> {
    fn rec(dir: &Path, root: &Path, out: &mut Vec<String>) {
        for e in std::fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                rec(&p, root, out);
            } else {
                out.push(p.strip_prefix(root).unwrap().display().to_string());
            }
        }
    }
    let mut out = Vec::new();
    rec(root, root, &mut out);
    out.sort();
    out
}

fn drive_paths(r: &Rig) -> Vec<String> {
    let mut v: Vec<String> = r
        .drive
        .files()
        .iter()
        .map(|f| r.drive.path_of(&f.id))
        .collect();
    v.sort();
    v
}

// ---- STO-5: a second Google Drive bundle copy gets its own folder -----------------------------

#[tokio::test]
async fn a_second_bundle_copy_on_drive_claims_a_new_folder_and_both_are_recorded() {
    let r = rig().await;
    let first = upload(&r, "drive").await;
    let second = upload(&r, "drive").await;
    assert_eq!(first["result"]["capture_id"], json!(r.id));

    let paths = drive_paths(&r);
    let copy1: Vec<&String> = paths
        .iter()
        .filter(|p| p.contains("/Lesson.cmrec/"))
        .collect();
    let copy2: Vec<&String> = paths
        .iter()
        .filter(|p| p.contains("/Lesson (2).cmrec/"))
        .collect();
    assert!(!copy1.is_empty() && copy1.len() == copy2.len(), "{paths:?}");
    // No file of the second copy is in the first copy's folder.
    assert!(
        paths.iter().all(|p| p.contains("/Lesson.cmrec/")
            || p.contains("/Lesson (2).cmrec/")
            || p.ends_with("Lesson - quick.mp4")
            || p.ends_with("Lesson - quick (2).mp4")),
        "{paths:?}"
    );
    let originals: Vec<Value> = cm_library::uploads_of(&r.dir)
        .into_iter()
        .filter(|u| u["what"] == "original")
        .collect();
    assert_eq!(originals.len(), 2, "{originals:?}");
    let ids: Vec<&str> = originals
        .iter()
        .map(|o| o["folder_id"].as_str().unwrap())
        .collect();
    assert_ne!(ids[0], ids[1], "each copy records its own Drive folder id");
    assert_eq!(
        originals[0]["path"],
        "CaptureMann/2026/10/Lesson/Lesson.cmrec"
    );
    assert_eq!(
        originals[1]["path"],
        "CaptureMann/2026/10/Lesson/Lesson (2).cmrec"
    );
    // The folder is the one the files are in.
    assert_eq!(
        r.drive.path_of(ids[1]),
        "CaptureMann/2026/10/Lesson/Lesson (2).cmrec"
    );
    let _ = second;
}

#[tokio::test]
async fn a_retried_bundle_upload_goes_on_in_the_folder_it_claimed() {
    let r = rig().await;
    // The first attempt claims the folder, then Drive refuses the first file for good.
    r.drive.inject(
        DriveCall::Init,
        0,
        DriveFault::Status {
            status: 403,
            retry_after: None,
            reason: "storageQuotaExceeded",
            message: "The user's Drive storage quota has been exceeded.",
        },
        1,
    );
    let v = call(
        &r,
        Actor::User,
        "upload",
        json!({ "capture_id": r.id, "destination_id": "drive", "what": "original" }),
    )
    .await
    .unwrap();
    let job = v["job_id"].as_str().unwrap().to_owned();
    let failed = wait_done(&r, &job).await;
    assert_eq!(failed["state"], "failed", "{failed}");
    let claimed = r
        .drive
        .folders()
        .iter()
        .filter(|f| f.name == "Lesson.cmrec")
        .count();
    assert_eq!(claimed, 1);

    // Retry: the same folder, not "Lesson (2).cmrec".
    call(&r, Actor::User, "retry_job", json!({ "job_id": job }))
        .await
        .unwrap();
    let ok = wait_done(&r, &job).await;
    assert_eq!(ok["state"], "done", "{ok}");
    let names: Vec<String> = r
        .drive
        .folders()
        .iter()
        .filter(|f| f.name.contains(".cmrec"))
        .map(|f| f.name.clone())
        .collect();
    assert_eq!(names, ["Lesson.cmrec"], "{names:?}");
    assert!(!r.drive.files().is_empty());
    assert!(drive_paths(&r)
        .iter()
        .all(|p| p.starts_with("CaptureMann/2026/10/Lesson/Lesson.cmrec/")));
    let rec = &cm_library::uploads_of(&r.dir)[0];
    assert_eq!(rec["what"], "original");
    assert!(rec["folder_id"].is_string());
}

// ---- the dialog ---------------------------------------------------------------------------------

#[tokio::test]
async fn the_dialog_lists_every_remote_copy_and_says_it_is_permanent() {
    let (r, video) = full_rig().await;
    let out = call(&r, agent(), "delete_capture", delete_params(&r))
        .await
        .unwrap();
    assert_eq!(out["moved_to_trash"], true);
    let shown = r.confirmer.shown();
    assert_eq!(shown.len(), 1);
    let text = all_text(&shown[0].details);
    // Each Google Drive upload: its path and its destination.
    assert!(
        text.contains("Google Drive: the file CaptureMann/2026/10/Lesson/Lesson - quick.mp4")
            && text.contains("teacher@example.org")
            && text.contains("School Drive"),
        "{text}"
    );
    assert!(
        text.contains("the folder CaptureMann/2026/10/Lesson/Lesson.cmrec and everything in it"),
        "{text}"
    );
    // The local copies.
    assert!(text.contains("Folder: the file"), "{text}");
    assert!(text.contains("copied files of"), "{text}");
    // The YouTube video: title, URL and channel.
    assert!(
        text.contains("How the editor works")
            && text.contains(&format!("https://www.youtube.com/watch?v={video}"))
            && text.contains("Teacher Channel"),
        "{text}"
    );
    assert!(
        text.contains("permanent") && text.contains("cannot be undone"),
        "{text}"
    );
    assert!(
        shown[0]
            .summary
            .contains("permanently delete 5 remote copies"),
        "{}",
        shown[0].summary
    );
    assert!(text.contains("remote_delete_5"), "{text}");
    assert!(!text.contains("remote_delete_6"), "{text}");
    // The same copies as structured data in the typed params (the app words them): numbered like
    // the rows, with the form, the path or title and the place.
    let items = shown[0].params["remote_items"].as_array().unwrap();
    assert_eq!(items.len(), 5, "{items:?}");
    assert!(items.iter().all(|i| i["state"] == "delete"));
    let ns: Vec<u64> = items.iter().map(|i| i["n"].as_u64().unwrap()).collect();
    assert_eq!(ns, [1, 2, 3, 4, 5]);
    let forms: Vec<&str> = items.iter().map(|i| i["form"].as_str().unwrap()).collect();
    for f in [
        "drive_file",
        "drive_folder",
        "local_file",
        "local_bundle",
        "youtube",
    ] {
        assert!(forms.contains(&f), "{f} in {forms:?}");
    }
    let yt = items.iter().find(|i| i["form"] == "youtube").unwrap();
    assert_eq!(yt["title"], "How the editor works");
    assert_eq!(yt["channel"], "Teacher Channel");
    let drive = items.iter().find(|i| i["form"] == "drive_file").unwrap();
    assert_eq!(drive["place"]["service"], "Google Drive");
    assert_eq!(drive["place"]["account"], "teacher@example.org");
    assert_eq!(drive["place"]["name"], "School Drive");
    let bundle = items.iter().find(|i| i["form"] == "local_bundle").unwrap();
    assert!(bundle["files"].as_u64().unwrap() > 0);
    assert_eq!(bundle["place"]["type"], "folder");
}

#[tokio::test]
async fn describe_delete_gives_the_same_rows_as_the_dialog() {
    let (r, _video) = full_rig().await;
    let d = r.daemon.describe_delete(&r.id, true).await.unwrap();
    call(&r, agent(), "delete_capture", delete_params(&r))
        .await
        .unwrap();
    let shown = r.confirmer.shown();
    assert_eq!(d.details, shown[0].details);
    assert_eq!(d.summary, shown[0].summary);
    assert_eq!(d.remote_items, shown[0].params["remote_items"]);
    // Without the flag: no remote rows at all.
    let r2 = rig().await;
    let plain = r2.daemon.describe_delete(&r2.id, false).await.unwrap();
    assert!(plain.details.get("remote").is_none());
    assert_eq!(plain.details["result"], "Moved to the Trash");
}

#[tokio::test]
async fn a_declined_dialog_deletes_nothing_anywhere() {
    let r = rig_declining().await;
    let e = call(&r, agent(), "delete_capture", delete_params(&r))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::UserDeclined);
    assert!(r.trasher.trashed().is_empty());
    assert_eq!(r.yt.videos().len(), 1);
    assert!(r.drive.files().len() >= 2);
    assert_eq!(r.yt.count(YtCall::Delete), 0);
    assert_eq!(r.drive.count(DriveCall::Delete), 0);
    assert_eq!(cm_library::uploads_of(&r.dir).len(), 4);
}

/// A rig like [`full_rig`] whose user declines every dialog.
async fn rig_declining() -> Rig {
    let r = rig_with(
        |b| b.confirmer(FixedConfirmer::new(Decision::Decline)),
        None,
    )
    .await;
    upload(&r, "drive").await;
    upload(&r, "nas").await;
    publish(&r, "How the editor works").await;
    r
}

// ---- deleting ---------------------------------------------------------------------------------

#[tokio::test]
async fn approving_deletes_every_remote_copy_and_then_trashes_the_capture() {
    let (r, video) = full_rig().await;
    assert!(
        r.drive.files().len() >= 2,
        "the export and the bundle's files are there"
    );
    assert!(!local_files(&r.nas).is_empty());

    let out = call(&r, agent(), "delete_capture", delete_params(&r))
        .await
        .unwrap();
    assert_eq!(out["moved_to_trash"], true);
    assert_eq!(out["deleted_remote"], true);
    assert_eq!(out["remote"]["deleted"].as_array().unwrap().len(), 5);
    assert!(out["remote"]["failed"].as_array().unwrap().is_empty());

    // Google Drive: the export file and the bundle folder with its content are gone; the folders
    // above stay.
    assert!(r.drive.files().is_empty(), "{:?}", drive_paths(&r));
    assert!(r.drive.folders().iter().any(|f| f.name == "Lesson"));
    assert!(!r.drive.folders().iter().any(|f| f.name == "Lesson.cmrec"));
    // The NAS: the copied files are gone and so are the copy's folders; the user's own folders
    // above stay.
    assert!(local_files(&r.nas).is_empty(), "{:?}", local_files(&r.nas));
    assert!(r.nas.join("CaptureMann/2026/10/Lesson").is_dir());
    assert!(!r
        .nas
        .join("CaptureMann/2026/10/Lesson/Lesson.cmrec")
        .exists());
    // YouTube: the video is gone.
    assert!(r.yt.videos().iter().all(|v| v.id != video));
    assert_eq!(r.yt.count(YtCall::Delete), 1);
    // The capture is in the Trash and out of the library.
    assert_eq!(r.trasher.trashed().len(), 1);
    assert!(!r.dir.exists(), "the folder left the library");
    // Nothing but deletes was sent to YouTube: the privacy of no video was touched.
    assert!(r.yt.unexpected().is_empty());
}

#[tokio::test]
async fn the_in_process_user_gets_the_same_deletion_without_a_dialog() {
    let (r, _video) = full_rig().await;
    let out = call(&r, Actor::User, "delete_capture", delete_params(&r))
        .await
        .unwrap();
    assert_eq!(out["remote"]["deleted"].as_array().unwrap().len(), 5);
    assert!(r.confirmer.shown().is_empty());
    assert!(r.drive.files().is_empty() && r.yt.videos().is_empty());
}

#[tokio::test]
async fn without_the_flag_no_remote_copy_is_touched() {
    let (r, _video) = full_rig().await;
    let before = (
        r.drive.files().len(),
        local_files(&r.nas),
        r.yt.videos().len(),
    );
    let out = call(
        &r,
        agent(),
        "delete_capture",
        json!({ "capture_id": r.id, "also_delete_remote": false }),
    )
    .await
    .unwrap();
    assert_eq!(out["deleted_remote"], false);
    assert!(out.get("remote").is_none());
    assert_eq!(
        before,
        (
            r.drive.files().len(),
            local_files(&r.nas),
            r.yt.videos().len()
        )
    );
    let text = all_text(&r.confirmer.shown()[0].details);
    assert!(!text.contains("remote_delete"), "{text}");
}

#[tokio::test]
async fn copies_that_are_gone_already_count_as_deleted() {
    let (r, video) = full_rig().await;
    // The user already removed the NAS export, and the video was deleted in YouTube Studio.
    let uploads = cm_library::uploads_of(&r.dir);
    let nas_export = uploads
        .iter()
        .find(|u| u["destination_id"] == "nas" && u["what"] == "export")
        .unwrap();
    std::fs::remove_file(nas_export["remote_id"].as_str().unwrap()).unwrap();
    let provider = YouTubeProvider::new(
        r.yt.clone(),
        Arc::new(Shared(r.yt_tokens.clone())),
        yt_config(),
    );
    provider.delete_video(&video).await.unwrap();

    let out = call(&r, agent(), "delete_capture", delete_params(&r))
        .await
        .unwrap();
    assert_eq!(out["moved_to_trash"], true);
    // The NAS file is known to be gone from the plan; a video or Drive file that is gone is
    // answered by the platform (404) and counts as deleted.
    assert_eq!(out["remote"]["already_gone"].as_array().unwrap().len(), 1);
    assert_eq!(out["remote"]["deleted"].as_array().unwrap().len(), 4);
    let text = all_text(&r.confirmer.shown()[0].details);
    assert!(text.contains("already gone"), "{text}");
}

#[tokio::test]
async fn a_screenshot_with_a_local_copy_is_deleted_remotely_too() {
    let r = rig().await;
    let shot = call(
        &r,
        Actor::User,
        "capture_screenshot",
        json!({"target": {"display_id": 1}, "title": "Slide"}),
    )
    .await
    .unwrap();
    let sid = shot["capture_id"].as_str().unwrap().to_owned();
    let v = call(
        &r,
        Actor::User,
        "upload",
        json!({ "capture_id": sid, "destination_id": "nas", "what": "original" }),
    )
    .await
    .unwrap();
    wait_done(&r, v["job_id"].as_str().unwrap()).await;
    assert!(!local_files(&r.nas).is_empty());
    let out = call(
        &r,
        agent(),
        "delete_capture",
        json!({ "capture_id": sid, "also_delete_remote": true }),
    )
    .await
    .unwrap();
    assert_eq!(out["remote"]["deleted"].as_array().unwrap().len(), 1);
    assert!(local_files(&r.nas).is_empty());
}

// ---- failures ---------------------------------------------------------------------------------

#[tokio::test]
async fn an_expired_google_sign_in_keeps_the_capture_and_says_what_was_and_was_not_deleted() {
    let (r, video) = full_rig().await;
    // Google Drive no longer accepts the account.
    r.drive.set_valid_token("other");
    r.drive_tokens.revoke_on_reject();

    let e = call(&r, agent(), "delete_capture", delete_params(&r))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::AuthExpired, "{e:?}");
    let msg = e.to_string();
    assert!(msg.contains("was kept"), "{msg}");
    assert!(
        msg.contains("Reconnect needed for Google Drive account"),
        "{msg}"
    );
    assert!(
        msg.contains("Deleted:") && msg.contains("Not deleted:"),
        "{msg}"
    );
    // The capture stays.
    assert!(r.trasher.trashed().is_empty());
    assert!(r.dir.is_dir(), "the capture is still where it was");
    // The steps that could run did: the NAS and YouTube are cleaned, Drive is not.
    assert!(local_files(&r.nas).is_empty());
    assert!(r.yt.videos().iter().all(|v| v.id != video));
    assert!(r.drive.files().len() >= 2);
    // The error carries the same information as data.
    let data = match &e {
        DaemonError::WithData { data, .. } => data.clone(),
        other => panic!("expected data, got {other:?}"),
    };
    assert_eq!(data["kept_local"], true);
    assert_eq!(data["remote"]["deleted"].as_array().unwrap().len(), 3);
    let failed = data["remote"]["failed"].as_array().unwrap();
    assert_eq!(failed.len(), 2);
    assert!(failed.iter().all(|f| f["error"] == "auth_expired"));

    // The manifest keeps the outcome and only the records that are still out there.
    let left = cm_library::uploads_of(&r.dir);
    assert_eq!(left.len(), 2, "{left:?}");
    assert!(left.iter().all(|u| u["destination_id"] == "drive"));
    assert!(cm_library::publishes_of(&r.dir).is_empty());
    let log = cm_library::remote_deletions_of(&r.dir);
    assert_eq!(log.len(), 5, "{log:?}");
    assert_eq!(log.iter().filter(|l| l["outcome"] == "deleted").count(), 3);
    assert_eq!(log.iter().filter(|l| l["outcome"] == "failed").count(), 2);
    assert!(log.iter().all(|l| l["at"].is_string()));

    // LIB-1: the library filters follow the records: two uploads are left, no video.
    let listed = |status: &str| {
        let r = &r;
        let status = status.to_owned();
        async move {
            call(r, Actor::User, "list_library", json!({ "status": status }))
                .await
                .unwrap()["captures"]
                .as_array()
                .unwrap()
                .len()
        }
    };
    assert_eq!(listed("uploaded").await, 1);
    assert_eq!(listed("published").await, 0);
    assert_eq!(listed("not_published").await, 1);

    // Signed in again: a second delete meets only what is left, and then trashes the capture.
    r.drive.set_valid_token("tok-2");
    // Each delete step builds its own provider, which asks for a token once: two steps.
    r.drive_tokens.refresh_to("tok-2");
    r.drive_tokens.refresh_to("tok-2");
    let out = call(&r, agent(), "delete_capture", delete_params(&r))
        .await
        .unwrap();
    assert_eq!(out["moved_to_trash"], true);
    assert_eq!(out["remote"]["deleted"].as_array().unwrap().len(), 2);
    assert!(r.drive.files().is_empty());
    assert_eq!(
        r.yt.count(YtCall::Delete),
        1,
        "the video is not deleted twice"
    );
    let text = all_text(&r.confirmer.shown()[1].details);
    assert!(
        !text.contains("How the editor works"),
        "the second dialog lists only what is left: {text}"
    );
}

#[tokio::test]
async fn an_expired_youtube_sign_in_is_reported_for_the_video_only() {
    let (r, video) = full_rig().await;
    r.yt.set_valid_token("other");
    r.yt_tokens.revoke_on_reject();
    let e = call(&r, agent(), "delete_capture", delete_params(&r))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::AuthExpired, "{e:?}");
    assert!(e
        .to_string()
        .contains("Reconnect needed for YouTube account \"Teacher Channel\""));
    assert!(r.drive.files().is_empty() && local_files(&r.nas).is_empty());
    assert!(r.yt.videos().iter().any(|v| v.id == video));
    assert!(r.trasher.trashed().is_empty());
    let left = cm_library::publishes_of(&r.dir);
    assert_eq!(left.len(), 1);
    assert_eq!(left[0]["remote_id"], json!(video));
}

#[tokio::test]
async fn a_network_failure_stops_the_remaining_steps_and_keeps_the_capture() {
    let (r, _video) = full_rig().await;
    // Google Drive asks to wait two minutes: that is a `network` error at once, and the steps
    // after it are not tried (they would wait on the same outage).
    r.drive.inject(
        DriveCall::Delete,
        0,
        DriveFault::Status {
            status: 429,
            retry_after: Some(120),
            reason: "rateLimitExceeded",
            message: "slow down",
        },
        1,
    );
    let e = call(&r, agent(), "delete_capture", delete_params(&r))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::Network, "{e:?}");
    let data = match &e {
        DaemonError::WithData { data, .. } => data.clone(),
        other => panic!("expected data, got {other:?}"),
    };
    assert_eq!(data["remote"]["failed"].as_array().unwrap().len(), 1);
    assert_eq!(data["remote"]["not_tried"].as_array().unwrap().len(), 4);
    assert_eq!(data["remote"]["deleted"].as_array().unwrap().len(), 0);
    assert!(r.trasher.trashed().is_empty());
    assert_eq!(r.yt.count(YtCall::Delete), 0);
    assert!(!local_files(&r.nas).is_empty());
    // Everything is still recorded, so a later run can finish.
    assert_eq!(cm_library::uploads_of(&r.dir).len(), 4);
    assert_eq!(cm_library::publishes_of(&r.dir).len(), 1);
    let out = call(&r, agent(), "delete_capture", delete_params(&r))
        .await
        .unwrap();
    assert_eq!(out["remote"]["deleted"].as_array().unwrap().len(), 5);
}

#[tokio::test]
async fn a_refusal_by_a_platform_is_platform_rejected_and_the_rest_still_runs() {
    let (r, video) = full_rig().await;
    r.yt.inject(
        YtCall::Delete,
        0,
        YtFault::Status {
            status: 403,
            retry_after: None,
            reason: "forbidden",
            message: "The video cannot be deleted by this account.",
        },
        1,
    );
    let e = call(&r, agent(), "delete_capture", delete_params(&r))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::PlatformRejected, "{e:?}");
    assert!(e.to_string().contains("cannot be deleted"), "{e}");
    assert!(r.drive.files().is_empty() && local_files(&r.nas).is_empty());
    assert!(r.yt.videos().iter().any(|v| v.id == video));
    assert!(r.trasher.trashed().is_empty());
}

#[tokio::test]
async fn an_offline_local_destination_keeps_the_capture() {
    let (r, _video) = full_rig().await;
    let away = r.cfg.path().join("nas-away");
    std::fs::rename(&r.nas, &away).unwrap();
    let e = call(&r, agent(), "delete_capture", delete_params(&r))
        .await
        .unwrap_err();
    // The plan (made while the volume is away) cannot see the files, so the local steps are
    // "already gone"? No: a missing root is `network`, never a silent success.
    assert!(
        matches!(code(&e), ErrorCode::Network),
        "an unreachable volume must not look like success: {e:?}"
    );
    assert!(r.trasher.trashed().is_empty());
    std::fs::rename(&away, &r.nas).unwrap();
    let files_before = local_files(&r.nas);
    assert!(
        !files_before.is_empty(),
        "nothing on the volume was touched"
    );
}

// ---- what is left in place, and safety ------------------------------------------------------

fn add_upload_record(r: &Rig, record: Value) {
    cm_library::record_upload(&r.dir, record).unwrap();
}

#[tokio::test]
async fn copies_that_cannot_be_deleted_here_are_listed_as_left_in_place() {
    let r = rig().await;
    let mut dests = r.dests.list().unwrap();
    let mut onedrive = drive_dest();
    onedrive.id = "od".into();
    onedrive.name = "OneDrive".into();
    onedrive.kind = Kind::OneDrive;
    dests.push(onedrive);
    r.dests.save(&dests).unwrap();
    add_upload_record(
        &r,
        json!({"destination_id": "od", "destination": "OneDrive", "what": "export",
               "remote_id": "od-file", "path": "CaptureMann/a.mp4"}),
    );
    // A destination that was removed since.
    add_upload_record(
        &r,
        json!({"destination_id": "gone", "destination": "Old disk", "what": "export",
               "remote_id": "/old/a.mp4", "path": "a.mp4"}),
    );
    // A Drive bundle copy from before folder ids were recorded.
    add_upload_record(
        &r,
        json!({"destination_id": "drive", "destination": "School Drive", "what": "original",
               "remote_id": "CaptureMann/2026/10/Lesson/Lesson.cmrec",
               "path": "CaptureMann/2026/10/Lesson/Lesson.cmrec"}),
    );
    // A YouTube channel that is not connected any more.
    cm_library::record_publish(
        &r.dir,
        json!({"platform": "youtube", "account_id": "yt-old", "account": "Old channel",
               "remote_id": "v-old", "url": "https://youtu.be/v-old", "title": "Old"}),
    )
    .unwrap();
    // And a platform this version cannot delete from.
    cm_library::record_publish(
        &r.dir,
        json!({"platform": "tiktok", "account_id": "tt", "account": "me",
               "remote_id": "t1", "url": "https://tiktok/t1", "title": "T"}),
    )
    .unwrap();

    let out = call(&r, agent(), "delete_capture", delete_params(&r))
        .await
        .unwrap();
    assert_eq!(out["moved_to_trash"], true);
    assert_eq!(out["deleted_remote"], false);
    let left = out["remote"]["left_in_place"].as_array().unwrap();
    assert_eq!(left.len(), 5, "{left:?}");
    let text = all_text(&r.confirmer.shown()[0].details);
    assert!(text.contains("Left in place"), "{text}");
    assert!(
        text.contains("deleting on OneDrive is not available yet"),
        "{text}"
    );
    assert!(text.contains("that destination was removed"), "{text}");
    assert!(text.contains("before folder ids were recorded"), "{text}");
    assert!(text.contains("not connected any more"), "{text}");
    assert!(
        !text.contains("permanent"),
        "nothing is deleted remotely, so no warning: {text}"
    );
    // Every copy that stays has a stable reason id next to the English text, in the typed
    // params and in the result.
    let ids = |v: &Value| -> Vec<String> {
        let mut ids: Vec<String> = v
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["reason"]["id"].as_str().unwrap().to_owned())
            .collect();
        ids.sort();
        ids
    };
    let planned = &r.confirmer.shown()[0].params["remote_items"];
    assert!(planned
        .as_array()
        .unwrap()
        .iter()
        .all(|i| i["state"] == "left"));
    let expected = ids(planned);
    assert_eq!(expected.len(), 5, "{expected:?}");
    for id in [
        "destination_removed",
        "drive_no_folder_id",
        "platform_unsupported",
        "provider_unavailable",
    ] {
        assert!(expected.iter().any(|e| e == id), "{id} in {expected:?}");
    }
    let outcome = &out["remote"]["items"];
    assert!(outcome
        .as_array()
        .unwrap()
        .iter()
        .all(|i| i["outcome"] == "left"));
    assert_eq!(ids(outcome), expected);
    // Nothing was sent anywhere.
    assert_eq!(r.drive.count(DriveCall::Delete), 0);
    assert_eq!(r.yt.count(YtCall::Delete), 0);
}

#[tokio::test]
async fn a_recorded_path_outside_the_destination_root_is_never_deleted() {
    let r = rig().await;
    let precious = r.cfg.path().join("precious.txt");
    std::fs::write(&precious, "keep").unwrap();
    for bad in [
        precious.display().to_string(),
        format!("{}/../precious.txt", r.nas.display()),
        "precious.txt".to_owned(),
    ] {
        add_upload_record(
            &r,
            json!({"destination_id": "nas", "destination": "Office NAS", "what": "export",
                   "remote_id": bad, "path": "x"}),
        );
    }
    // And a bundle record whose folder is absolute or climbs out.
    for bad in ["/etc", "../.."] {
        add_upload_record(
            &r,
            json!({"destination_id": "nas", "destination": "Office NAS", "what": "original",
                   "remote_id": bad, "path": bad}),
        );
    }
    let out = call(&r, agent(), "delete_capture", delete_params(&r)).await;
    // Either the entries are left in place, or (the `..` folder) refused; the file is intact.
    assert!(precious.exists());
    assert!(r.nas.exists());
    match out {
        Ok(v) => assert_eq!(v["remote"]["deleted"].as_array().unwrap().len(), 0, "{v}"),
        Err(e) => assert!(
            matches!(code(&e), ErrorCode::InvalidInput | ErrorCode::NotFound),
            "{e:?}"
        ),
    }
}

#[cfg(unix)]
#[tokio::test]
async fn a_symbolic_link_inside_the_destination_does_not_lead_the_delete_out() {
    let r = rig().await;
    let precious = r.cfg.path().join("precious.txt");
    std::fs::write(&precious, "keep").unwrap();
    let link = r.nas.join("link.mp4");
    std::os::unix::fs::symlink(&precious, &link).unwrap();
    add_upload_record(
        &r,
        json!({"destination_id": "nas", "destination": "Office NAS", "what": "export",
               "remote_id": link.display().to_string(), "path": "link.mp4"}),
    );
    let e = call(&r, agent(), "delete_capture", delete_params(&r))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput, "{e:?}");
    assert!(precious.exists(), "the file behind the link is untouched");
    assert!(r.trasher.trashed().is_empty());
}

#[tokio::test]
async fn files_the_user_added_to_a_local_bundle_copy_stay() {
    let r = rig().await;
    upload(&r, "nas").await;
    let copy = r.nas.join("CaptureMann/2026/10/Lesson/Lesson.cmrec");
    assert!(copy.is_dir());
    std::fs::write(copy.join("my notes.txt"), "mine").unwrap();
    let out = call(&r, agent(), "delete_capture", delete_params(&r))
        .await
        .unwrap();
    assert_eq!(out["moved_to_trash"], true);
    assert_eq!(local_files(&copy), ["my notes.txt"]);
}

// ---- what runs is what the dialog showed ------------------------------------------------------

#[tokio::test]
async fn only_the_copies_the_dialog_listed_are_deleted() {
    // A dialog hook adds one more NAS copy of the export while the user decides.
    let late = Arc::new(Mutex::new(None::<(PathBuf, PathBuf)>));
    let late2 = late.clone();
    let (cell_dir, cell_nas) = (
        Arc::new(Mutex::new(None::<PathBuf>)),
        Arc::new(Mutex::new(None::<PathBuf>)),
    );
    let (cd, cn) = (cell_dir.clone(), cell_nas.clone());
    let hook: Box<dyn FnOnce() + Send> = Box::new(move || {
        let dir = cd.lock().unwrap().clone().unwrap();
        let nas = cn.lock().unwrap().clone().unwrap();
        let extra = nas.join("late-copy.mp4");
        std::fs::write(&extra, "late").unwrap();
        cm_library::record_upload(
            &dir,
            json!({"destination_id": "nas", "destination": "Office NAS", "what": "export",
                   "remote_id": extra.display().to_string(), "path": "late-copy.mp4"}),
        )
        .unwrap();
        *late2.lock().unwrap() = Some((extra, dir));
    });
    let r = rig_with(|b| b, Some(hook)).await;
    *cell_dir.lock().unwrap() = Some(r.dir.clone());
    *cell_nas.lock().unwrap() = Some(r.nas.clone());
    upload(&r, "nas").await;
    let out = call(&r, agent(), "delete_capture", delete_params(&r))
        .await
        .unwrap();
    assert_eq!(out["moved_to_trash"], true);
    // The two copies of the dialog are gone; the one added after it is still there.
    assert_eq!(out["remote"]["deleted"].as_array().unwrap().len(), 2);
    let (extra, _) = late.lock().unwrap().clone().unwrap();
    assert!(
        extra.exists(),
        "a copy the dialog did not list is not deleted"
    );
}

#[tokio::test]
async fn a_destination_that_changed_kind_after_the_dialog_deletes_nothing_there() {
    let dests_cell: Arc<Mutex<Option<DestinationStore>>> = Arc::new(Mutex::new(None));
    let cell = dests_cell.clone();
    let hook: Box<dyn FnOnce() + Send> = Box::new(move || {
        let store = cell.lock().unwrap().clone().unwrap();
        let mut list = store.list().unwrap();
        // The folder destination "nas" becomes a Google Drive destination.
        let mut d = drive_dest();
        d.id = "nas".into();
        list.retain(|x| x.id != "nas");
        list.push(d);
        store.save(&list).unwrap();
    });
    let r = rig_with(|b| b, Some(hook)).await;
    *dests_cell.lock().unwrap() = Some(r.dests.clone());
    upload(&r, "nas").await;
    let before = local_files(&r.nas);
    let e = call(&r, agent(), "delete_capture", delete_params(&r))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput, "{e:?}");
    assert_eq!(before, local_files(&r.nas), "nothing was deleted");
    assert!(r.trasher.trashed().is_empty());
}

// ---- screenshot manifest race (record vs editor) -------------------------------------------

#[tokio::test]
async fn an_upload_record_and_an_annotation_both_survive_on_a_screenshot() {
    let r = rig().await;
    let shot = call(
        &r,
        Actor::User,
        "capture_screenshot",
        json!({"target": {"display_id": 1}, "title": "Race"}),
    )
    .await
    .unwrap();
    let sid = shot["capture_id"].as_str().unwrap().to_owned();
    let ops = json!([{"op":"rect","rect":[5,5,20,20],"color":"#2E8BFF","width":2}]);
    // Many annotations and many uploads at the same time, all through the daemon.
    let annotate = async {
        for _ in 0..6 {
            call(
                &r,
                Actor::User,
                "annotate",
                json!({"capture_id": sid, "ops": ops}),
            )
            .await
            .unwrap();
        }
    };
    let uploads = async {
        let mut jobs = Vec::new();
        for _ in 0..4 {
            let v = call(
                &r,
                Actor::User,
                "upload",
                json!({ "capture_id": sid, "destination_id": "nas", "what": "original" }),
            )
            .await
            .unwrap();
            jobs.push(v["job_id"].as_str().unwrap().to_owned());
        }
        for j in jobs {
            let done = wait_done(&r, &j).await;
            assert_eq!(done["state"], "done", "{done}");
        }
    };
    tokio::join!(annotate, uploads);

    let got = call(&r, Actor::User, "get_capture", json!({ "capture_id": sid }))
        .await
        .unwrap();
    // Every annotation revision is there, and so is every upload record.
    assert_eq!(got["revision"], 6, "{got}");
    let dir = std::fs::read_dir(r._lib.path())
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.extension().is_some_and(|e| e == "cmshot"))
        .unwrap();
    let records = cm_library::uploads_of(&dir);
    assert_eq!(records.len(), 4, "{records:?}");
    let undone = call(&r, Actor::User, "undo", json!({ "capture_id": sid }))
        .await
        .unwrap();
    assert_eq!(undone["changed"], true, "{undone}");
    assert_eq!(
        cm_library::uploads_of(&dir).len(),
        4,
        "undo keeps the upload records"
    );
}
