//! `list_publish_accounts`, `publish_video` and the `publish` job (PUB-1, PUB-2) through the
//! daemon: fake recording sources, a stub exporter, a scripted confirmer, the in-process fake
//! YouTube server and scripted tokens. No network, no account, no keychain. Tested with fakes.

#![allow(clippy::unwrap_used)]

mod common;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use cm_core::{Actor, CmError, ErrorCode, ManualHostClock};
use cm_daemon::storage_cloud::{AccountTokens, CloudBackend};
use cm_daemon::testing::{FakeSources, FixedConfirmer, NeverAnswers};
use cm_daemon::{Daemon, DaemonError, Decision, EngineOptions, Event};
use cm_jobs::{BackoffPolicy, JobContext, JobHandler, JobQueue, QueueConfig, JOB_EXPORT};
use cm_publish::fake_youtube::{Call, FakeTokens, FakeYouTube, Fault};
use cm_publish::{
    next_pacific_midnight, Privacy, PublishAccount, PublishAccountStore, YouTubeConfig,
    YOUTUBE_UPLOADS_PER_DAY,
};
use cm_storage::TokenSource;
use common::{builder, desktop};
use serde_json::{json, Value};
use tokio::sync::broadcast::Receiver;

const KIB: u64 = 1024;
/// The scripted channel's keychain key: the one string that must never reach a client.
const KEY: &str = "yt1";
const EXPORT_BYTES: usize = 700 * 1024;

/// Hands the scripted tokens to the provider, and checks that the keychain key (not the account
/// id) is what the provider asks for. `swap` is the user signing in again.
struct Tokens(Arc<Mutex<Arc<FakeTokens>>>);

impl AccountTokens for Tokens {
    fn source(&self, account: &str) -> Arc<dyn TokenSource> {
        assert_eq!(account, KEY, "the provider must use the keychain key");
        Arc::new(Delegate(self.0.clone()))
    }

    /// `yt1` and `yt2` were connected with clients of one Google Cloud project, `yt3` with
    /// another project's client; any other account has no known client.
    fn client_id(&self, account: &str) -> Option<String> {
        match account {
            "yt1" => Some("123456789012-aaaa.apps.googleusercontent.com".into()),
            "yt2" => Some("123456789012-bbbb.apps.googleusercontent.com".into()),
            "yt3" => Some("999000111222-cccc.apps.googleusercontent.com".into()),
            _ => None,
        }
    }
}

struct Delegate(Arc<Mutex<Arc<FakeTokens>>>);

#[async_trait]
impl TokenSource for Delegate {
    async fn access_token(&self, rejected: Option<&str>) -> Result<String, CmError> {
        let t = self.0.lock().unwrap().clone();
        t.access_token(rejected).await
    }
}

/// Writes `exports/Take - <preset>.mp4` and records it, like the real exporter.
struct StubExport {
    root: PathBuf,
}

#[async_trait]
impl JobHandler for StubExport {
    async fn run(&self, ctx: JobContext) -> Result<serde_json::Value, CmError> {
        let id = ctx.job.inputs["capture_id"].as_str().unwrap().to_owned();
        let preset = ctx.job.inputs["preset"].as_str().unwrap().to_owned();
        write_export(&self.root, &id, &preset);
        Ok(json!({ "path": "x" }))
    }
}

fn bundle_dir(root: &Path, id: &str) -> PathBuf {
    std::fs::read_dir(root)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "cmrec"))
        .find(|p| cm_library::read_manifest(p).is_ok_and(|m| m.id == id))
        .expect("bundle folder")
}

fn write_export(root: &Path, id: &str, preset: &str) -> String {
    let dir = bundle_dir(root, id);
    std::fs::create_dir_all(dir.join("exports")).unwrap();
    let rel = format!("exports/Take - {preset}.mp4");
    let data: Vec<u8> = (0..EXPORT_BYTES).map(|i| (i * 31 % 251) as u8).collect();
    std::fs::write(dir.join(&rel), data).unwrap();
    cm_library::record_export(
        &dir,
        json!({ "file": rel, "preset": preset, "duration_ms": 61_000, "size_bytes": EXPORT_BYTES }),
        Actor::User,
        time::OffsetDateTime::now_utc(),
    )
    .unwrap();
    rel
}

struct Opts {
    decision: Decision,
    max_retries: u32,
    default_privacy: Privacy,
    never_answers: bool,
}

impl Default for Opts {
    fn default() -> Self {
        Opts {
            decision: Decision::Approve,
            max_retries: 0,
            default_privacy: Privacy::Unlisted,
            never_answers: false,
        }
    }
}

struct Rig {
    daemon: Daemon,
    yt: Arc<FakeYouTube>,
    tokens: Arc<Mutex<Arc<FakeTokens>>>,
    store: PublishAccountStore,
    confirmer: Arc<FixedConfirmer>,
    root: PathBuf,
    _dirs: (tempfile::TempDir, tempfile::TempDir),
}

fn account(default_privacy: Privacy) -> PublishAccount {
    PublishAccount {
        id: "yt1".into(),
        platform: "youtube".into(),
        name: "Teacher Channel".into(),
        account_id: "UC-channel-id".into(),
        default_privacy,
    }
}

fn rig() -> Rig {
    rig_with(Opts::default())
}

fn rig_with(o: Opts) -> Rig {
    let (b, lib) = builder(desktop());
    let cfg = tempfile::tempdir().unwrap();
    let store = PublishAccountStore::new(cfg.path());
    store.save(&[account(o.default_privacy)]).unwrap();
    let yt = Arc::new(FakeYouTube::new());
    let tokens = Arc::new(Mutex::new(Arc::new(FakeTokens::new("tok-1"))));
    // A quick retry schedule, so a job that fails with `network` is tried again at once.
    let queue = JobQueue::open(
        cm_daemon::library_jobs_path(lib.path()),
        QueueConfig {
            backoff: BackoffPolicy {
                base: Duration::from_millis(20),
                max: Duration::from_millis(100),
                jitter_ratio: 0.0,
            },
            poll_interval: Duration::from_millis(20),
            ..QueueConfig::default()
        },
    )
    .or_else(|_| {
        std::fs::create_dir_all(lib.path().join(".capturemann")).unwrap();
        JobQueue::open(
            cm_daemon::library_jobs_path(lib.path()),
            QueueConfig::default(),
        )
    })
    .unwrap();
    let opts = EngineOptions {
        tick_every: Duration::from_millis(15),
        countdown_step: Duration::from_millis(30),
        host: Arc::new(ManualHostClock::new(5_000_000_000)),
        own_pid: None,
        disk_check_every: Duration::ZERO,
        selftest_duration: Duration::from_millis(100),
        // No FFmpeg: no automatic export, the tests make their own.
        ffmpeg_probe: Some(Arc::new(|| false)),
        ..EngineOptions::default()
    };
    let confirmer = Arc::new(FixedConfirmer::new(o.decision));
    let mut b = b
        .jobs(queue)
        .publish_accounts(store.clone())
        .cloud_backend(CloudBackend {
            tokens: Arc::new(Tokens(tokens.clone())),
            http: yt.clone(),
        })
        .youtube_config(YouTubeConfig {
            chunk_size: 256 * KIB,
            max_retries: o.max_retries,
            backoff_base: Duration::from_millis(10),
            ..YouTubeConfig::default()
        })
        .recording_sources(FakeSources::new())
        .recording_options(opts)
        .confirm_timeout(Duration::from_millis(150));
    b = if o.never_answers {
        b.confirmer(NeverAnswers)
    } else {
        b.confirmer(confirmer.clone())
    };
    let daemon = b.build();
    let root = lib.path().to_path_buf();
    daemon
        .register_job_handler(JOB_EXPORT, Arc::new(StubExport { root: root.clone() }))
        .unwrap();
    daemon.register_publish_handler().unwrap();
    daemon.start_jobs().unwrap();
    Rig {
        daemon,
        yt,
        tokens,
        store,
        confirmer,
        root,
        _dirs: (lib, cfg),
    }
}

fn agent() -> Actor {
    Actor::agent("claude").unwrap()
}

async fn call(d: &Daemon, who: Actor, method: &str, p: Value) -> Result<Value, DaemonError> {
    d.execute_method(who, 0, method, &p).await
}

async fn user(d: &Daemon, method: &str, p: Value) -> Value {
    call(d, Actor::User, method, p).await.unwrap()
}

fn code(e: &DaemonError) -> ErrorCode {
    e.code().unwrap()
}

/// A recording with one export (`youtube-1080p`).
async fn recording_with_export(r: &Rig) -> String {
    let id = record(&r.daemon).await;
    write_export(&r.root, &id, "youtube-1080p");
    id
}

async fn record(d: &Daemon) -> String {
    let s = user(
        d,
        "start_recording",
        json!({ "target": { "display_id": 1 }, "countdown_s": 0, "title": "Take" }),
    )
    .await;
    let id = s["capture_id"].as_str().unwrap().to_owned();
    let stopped = user(d, "stop_recording", json!({})).await;
    assert_eq!(stopped["capture_id"], id.as_str());
    id
}

fn request(id: &str) -> Value {
    json!({
        "capture_id": id, "platform": "youtube", "title": "Probes in 5 minutes",
        "description": "00:00 Intro", "tags": ["k8s", "tutorial"], "made_for_kids": false,
    })
}

async fn wait_job(d: &Daemon, id: &str, pred: impl Fn(&Value) -> bool) -> Value {
    let mut last = Value::Null;
    for _ in 0..3000 {
        let j = user(d, "get_job", json!({ "job_id": id })).await;
        if pred(&j) {
            return j;
        }
        last = j;
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the job did not reach the wanted state in 30 s; its last state: {last}");
}

async fn wait_done(d: &Daemon, id: &str) -> Value {
    let j = wait_job(d, id, |j| {
        matches!(j["state"].as_str(), Some("done" | "failed" | "cancelled"))
    })
    .await;
    assert_eq!(j["state"], "done", "{j}");
    j
}

fn drain(rx: &mut Receiver<Event>) -> Vec<Event> {
    let mut v = Vec::new();
    while let Ok(e) = rx.try_recv() {
        v.push(e);
    }
    v
}

async fn publishes_in_library(d: &Daemon, id: &str) -> Vec<Value> {
    let v = user(d, "list_library", json!({})).await;
    v["captures"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == id)
        .unwrap()["publishes"]
        .as_array()
        .unwrap()
        .clone()
}

// ---- list_publish_accounts --------------------------------------------------------------------

#[tokio::test]
async fn pub1_agents_see_the_label_and_defaults_but_never_the_key() {
    let r = rig();
    let v = call(&r.daemon, agent(), "list_publish_accounts", json!({}))
        .await
        .unwrap();
    let a = &v["accounts"][0];
    assert_eq!(a["account_id"], "yt1");
    assert_eq!(a["platform"], "youtube");
    assert_eq!(a["name"], "Teacher Channel");
    assert_eq!(a["status"], "connected");
    assert_eq!(a["default_privacy"], "unlisted");
    assert_eq!(a["id"], "yt1");
    assert_eq!(a["quota"]["used"], 0);
    assert_eq!(a["quota"]["limit"], 100);
    assert_eq!(a["uploads_today"], 0);
    assert_eq!(a["uploads_limit"], YOUTUBE_UPLOADS_PER_DAY);
    assert!(a["resets_at"].as_str().unwrap().ends_with('Z'));
    assert_eq!(v["platforms"]["youtube"]["title_max_chars"], 100);
    let text = v.to_string();
    assert!(!text.contains("tok-1"));
    // Free for agents: no dialog, and a read-only client may call it.
    r.daemon
        .policies()
        .set("viewer", cm_daemon::ClientPolicy::ReadOnly);
    call(
        &r.daemon,
        Actor::agent("viewer").unwrap(),
        "list_publish_accounts",
        json!({}),
    )
    .await
    .unwrap();
    assert!(r.confirmer.shown().is_empty());
    // Params are refused.
    let e = call(&r.daemon, agent(), "list_publish_accounts", json!({"x": 1}))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
}

#[tokio::test]
async fn pub1_without_a_store_there_are_no_accounts_and_publish_is_not_implemented() {
    let (b, lib) = builder(desktop());
    let d = b
        .confirmer(FixedConfirmer::new(Decision::Approve))
        .jobs(cm_daemon::library_job_queue(lib.path()).unwrap())
        .build();
    let v = user(&d, "list_publish_accounts", json!({})).await;
    assert_eq!(v["accounts"], json!([]));
    let e = call(&d, agent(), "publish_video", request("rec_1"))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::NotImplemented);
}

#[tokio::test]
async fn a_stored_public_default_is_listed_and_used_as_unlisted() {
    let r = rig_with(Opts {
        default_privacy: Privacy::Public,
        ..Opts::default()
    });
    let v = user(&r.daemon, "list_publish_accounts", json!({})).await;
    assert_eq!(v["accounts"][0]["default_privacy"], "unlisted");
    let id = recording_with_export(&r).await;
    let res = call(&r.daemon, agent(), "publish_video", request(&id))
        .await
        .unwrap();
    assert_eq!(res["privacy"], "unlisted");
    wait_done(&r.daemon, res["job_id"].as_str().unwrap()).await;
    assert_eq!(r.yt.videos()[0].requested_privacy, "unlisted");
}

// ---- the dialog, approval, the job --------------------------------------------------------------

#[tokio::test]
async fn pub2_the_dialog_shows_what_runs_and_an_approved_publish_completes() {
    let r = rig();
    let mut rx = r.daemon.subscribe();
    let id = recording_with_export(&r).await;
    let mut p = request(&id);
    p["privacy"] = json!("private");
    p["request_id"] = json!("pub-1");
    let res = call(&r.daemon, agent(), "publish_video", p).await.unwrap();

    // The dialog: platform, account label, title, privacy, made for kids, file and size, that it
    // goes over the internet, and the day's count.
    let shown = r.confirmer.shown();
    assert_eq!(shown.len(), 1);
    let req = &shown[0];
    assert_eq!(req.tool, "publish_video");
    // P3: the plan as typed parameters; the English summary and rows are unchanged.
    assert_eq!(req.kind, "publish_video");
    assert_eq!(
        req.params,
        json!({
            "title": "Probes in 5 minutes", "platform": "youtube", "platform_name": "YouTube",
            "account": "Teacher Channel", "privacy": "private", "privacy_defaulted": false,
            "preset": null, "size_bytes": 716_800, "captions": false,
            "uploads_used": 0, "uploads_limit": 100, "kept_private_note": false,
        })
    );
    assert_eq!(req.client, "claude");
    assert!(!req.allow_session);
    assert_eq!(
        req.summary,
        "Publish \"Probes in 5 minutes\" to YouTube as Private"
    );
    let d = &req.details;
    assert_eq!(d["platform"], "youtube");
    assert_eq!(d["account"], "Teacher Channel");
    assert_eq!(d["title"], "Probes in 5 minutes");
    assert_eq!(d["privacy"], "private");
    assert_eq!(d["made_for_kids"], false);
    assert_eq!(d["file"], "Take - youtube-1080p.mp4");
    assert_eq!(
        d["size"],
        "716.8 kB".replace("716.8 kB", d["size"].as_str().unwrap())
    );
    assert!(d["size"].as_str().unwrap().contains('B'), "{d}");
    assert!(d["sends"].as_str().unwrap().contains("over the internet"));
    assert_eq!(d["uploads_today"], "0 of 100 used today");
    assert_eq!(d["tags"], "k8s, tutorial");
    assert!(d.get("playlist").is_none(), "no playlist was given: {d}");
    assert!(d.get("privacy_source").is_none(), "the privacy was given");

    // Nothing was uploaded before the job ran; the job is the one the dialog described.
    assert_eq!(res["platform"], "youtube");
    assert_eq!(res["account"], "Teacher Channel");
    assert_eq!(res["privacy"], "private");
    assert_eq!(res["file"], "exports/Take - youtube-1080p.mp4");
    let job = wait_done(&r.daemon, res["job_id"].as_str().unwrap()).await;
    assert_eq!(job["type"], "publish");
    assert_eq!(job["inputs"]["account_id"], "yt1");

    // YouTube has the video with what the sheet said.
    let vids = r.yt.videos();
    assert_eq!(vids.len(), 1);
    assert_eq!(vids[0].title, "Probes in 5 minutes");
    assert_eq!(vids[0].description, "00:00 Intro");
    assert_eq!(vids[0].tags, vec!["k8s", "tutorial"]);
    assert_eq!(vids[0].requested_privacy, "private");
    assert_eq!(vids[0].made_for_kids, Some(false));
    assert_eq!(vids[0].data.len(), EXPORT_BYTES);
    assert!(r.yt.violations().is_empty());
    assert!(r.yt.unexpected().is_empty(), "{:?}", r.yt.unexpected());

    // The result.
    let out = &job["result"];
    assert_eq!(out["video_id"], vids[0].id.as_str());
    assert!(out["url"].as_str().unwrap().contains(&vids[0].id));
    assert_eq!(out["status"], "processing");
    assert_eq!(out["privacy"], "private");
    assert_eq!(out["uploads_today"], 1);
    assert!(out["record_warning"].is_null(), "{out}");

    // The manifest record (no undo step) is visible in list_library.
    let list = publishes_in_library(&r.daemon, &id).await;
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["platform"], "youtube");
    assert_eq!(list[0]["account"], "Teacher Channel");
    assert_eq!(list[0]["remote_id"], vids[0].id.as_str());
    assert_eq!(list[0]["url"], out["url"]);
    assert_eq!(list[0]["privacy"], "private");
    assert_eq!(list[0]["status"], "processing");
    let m = cm_library::read_manifest(&bundle_dir(&r.root, &id)).unwrap();
    assert!(
        !m.revisions.iter().any(|v| v.action.contains("publish")),
        "bookkeeping is not an undo step"
    );

    // The counter.
    let acc = user(&r.daemon, "list_publish_accounts", json!({})).await;
    assert_eq!(acc["accounts"][0]["uploads_today"], 1);

    // Events.
    let ev = drain(&mut rx);
    let queued = ev.iter().find(|e| e.name == "publish.queued").unwrap();
    assert_eq!(queued.data["job_id"], res["job_id"]);
    assert_eq!(queued.data["capture_id"], id.as_str());
    assert_eq!(queued.data["privacy"], "private");
    assert_eq!(queued.data["account"], "Teacher Channel");
    let done = ev.iter().find(|e| e.name == "publish.done").unwrap();
    assert_eq!(done.data["video_id"], vids[0].id.as_str());
    assert_eq!(done.data["url"], out["url"]);
    assert_eq!(done.data["status"], "processing");
    assert!(done.data["warnings"].as_array().unwrap().is_empty());
    assert!(ev.iter().any(|e| e.name == "job.done"));
}

#[tokio::test]
async fn pub2_a_missing_privacy_is_the_accounts_default_and_the_dialog_says_so() {
    let r = rig();
    let id = recording_with_export(&r).await;
    let res = call(&r.daemon, agent(), "publish_video", request(&id))
        .await
        .unwrap();
    let d = &r.confirmer.shown()[0].details;
    assert_eq!(d["privacy"], "unlisted");
    assert!(d["privacy_source"].as_str().unwrap().contains("default"));
    assert!(d["note"].as_str().unwrap().contains("unverified"));
    wait_done(&r.daemon, res["job_id"].as_str().unwrap()).await;
    assert_eq!(r.yt.videos()[0].requested_privacy, "unlisted");

    // Never silently public: only a request that says `public` gets it.
    let mut p = request(&id);
    p["privacy"] = json!("public");
    let res = call(&r.daemon, agent(), "publish_video", p).await.unwrap();
    assert_eq!(res["privacy"], "public");
    assert_eq!(r.confirmer.shown()[1].details["privacy"], "public");
    assert!(r.confirmer.shown()[1].summary.ends_with("as Public"));
    wait_done(&r.daemon, res["job_id"].as_str().unwrap()).await;
}

#[tokio::test]
async fn pub2_a_declined_or_timed_out_confirmation_never_uploads() {
    // Declined.
    let r = rig_with(Opts {
        decision: Decision::Decline,
        ..Opts::default()
    });
    let id = recording_with_export(&r).await;
    let e = call(&r.daemon, agent(), "publish_video", request(&id))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::UserDeclined);
    assert_eq!(r.confirmer.shown().len(), 1, "the dialog was shown");
    assert!(
        user(&r.daemon, "list_jobs", json!({"type": "publish"})).await["jobs"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(r.yt.videos().is_empty());
    assert_eq!(r.yt.count(Call::Init), 0, "not a byte went to YouTube");
    assert!(publishes_in_library(&r.daemon, &id).await.is_empty());

    // Nobody answers.
    let r = rig_with(Opts {
        never_answers: true,
        ..Opts::default()
    });
    let id = recording_with_export(&r).await;
    let e = call(&r.daemon, agent(), "publish_video", request(&id))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::ConfirmationTimeout);
    assert!(
        user(&r.daemon, "list_jobs", json!({"type": "publish"})).await["jobs"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(r.yt.count(Call::Init), 0);

    // A headless daemon (no dialog can be shown) never publishes for an agent either.
    let (b, lib) = builder(desktop());
    let store = PublishAccountStore::new(tempfile::tempdir().unwrap().keep());
    store.save(&[account(Privacy::Unlisted)]).unwrap();
    let d = b
        .jobs(cm_daemon::library_job_queue(lib.path()).unwrap())
        .publish_accounts(store)
        .build();
    d.register_publish_handler().unwrap();
    let e = call(&d, agent(), "publish_video", request("rec_1"))
        .await
        .unwrap_err();
    // A request that cannot be planned fails first; one that can is refused at the gate.
    assert!(
        matches!(
            code(&e),
            ErrorCode::NotFound | ErrorCode::UserDeclined | ErrorCode::InvalidInput
        ),
        "{e:?}"
    );
}

#[tokio::test]
async fn pub2_bad_requests_fail_with_the_field_name_before_any_dialog() {
    let r = rig();
    let id = recording_with_export(&r).await;
    let shot = user(
        &r.daemon,
        "capture_screenshot",
        json!({"target": {"display_id": 1}, "title": "shot"}),
    )
    .await["capture_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let with = |k: &str, v: Value| {
        let mut p = request(&id);
        p[k] = v;
        p
    };
    let without = |k: &str| {
        let mut p = request(&id);
        p.as_object_mut().unwrap().remove(k);
        p
    };
    let cases: Vec<(Value, ErrorCode, &str)> = vec![
        (without("title"), ErrorCode::InvalidInput, "title"),
        (with("title", json!("  ")), ErrorCode::InvalidInput, "title"),
        (
            with("title", json!("x".repeat(101))),
            ErrorCode::InvalidInput,
            "title",
        ),
        (
            with("title", json!("a <b> c")),
            ErrorCode::InvalidInput,
            "title",
        ),
        (
            with("description", json!("d".repeat(5001))),
            ErrorCode::InvalidInput,
            "description",
        ),
        (
            with("tags", json!(["a", "b".repeat(600)])),
            ErrorCode::InvalidInput,
            "tags",
        ),
        (with("tags", json!("a,b")), ErrorCode::InvalidInput, "tags"),
        (
            with("privacy", json!("everyone")),
            ErrorCode::InvalidInput,
            "privacy",
        ),
        (
            without("made_for_kids"),
            ErrorCode::InvalidInput,
            "made_for_kids",
        ),
        (
            with("thumbnail", json!("relative.png")),
            ErrorCode::InvalidInput,
            "thumbnail",
        ),
        (
            with("thumbnail", json!("/definitely/not/here.png")),
            ErrorCode::InvalidInput,
            "thumbnail",
        ),
        (
            with("captions", json!(true)),
            ErrorCode::NotImplemented,
            "captions",
        ),
        (
            with("export_preset", json!("no-such-preset")),
            ErrorCode::InvalidInput,
            "preset",
        ),
        (
            with("account_id", json!("nope")),
            ErrorCode::NotFound,
            "nope",
        ),
        (
            with("platform", json!("myspace")),
            ErrorCode::InvalidInput,
            "platform",
        ),
        (
            with("platform", json!("tiktok")),
            ErrorCode::NotImplemented,
            "TikTok",
        ),
        (
            with("capture_id", json!("rec_missing")),
            ErrorCode::NotFound,
            "rec_missing",
        ),
        (
            with("capture_id", json!(shot)),
            ErrorCode::InvalidInput,
            "screenshot",
        ),
        // Nothing may name an existing remote video or change its privacy.
        (
            with("video_id", json!("abc")),
            ErrorCode::InvalidInput,
            "video_id",
        ),
        (
            with("remote_id", json!("abc")),
            ErrorCode::InvalidInput,
            "remote_id",
        ),
        (
            with("update", json!(true)),
            ErrorCode::InvalidInput,
            "update",
        ),
        (
            with("metadata", json!({"video_id": "abc"})),
            ErrorCode::InvalidInput,
            "metadata.video_id",
        ),
    ];
    for (p, want, needle) in cases {
        let e = call(&r.daemon, agent(), "publish_video", p.clone())
            .await
            .unwrap_err();
        assert_eq!(code(&e), want, "{p}: {e:?}");
        assert!(
            e.to_string().contains(needle),
            "{needle} not in {e} for {p}"
        );
    }
    assert!(
        r.confirmer.shown().is_empty(),
        "no dialog for a request that cannot run"
    );
    assert!(r.yt.videos().is_empty());
}

#[tokio::test]
async fn pub2_a_recording_without_an_export_asks_for_one_before_any_dialog() {
    let r = rig();
    let id = record(&r.daemon).await;
    let e = call(&r.daemon, agent(), "publish_video", request(&id))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::NotFound);
    assert!(e.to_string().contains("export"), "{e}");
    assert!(r.confirmer.shown().is_empty());
}

#[tokio::test]
async fn pub2_the_metadata_object_is_the_same_as_the_flat_fields() {
    let r = rig();
    let id = recording_with_export(&r).await;
    let res = call(
        &r.daemon,
        agent(),
        "publish_video",
        json!({
            "capture_id": id, "platform": "youtube", "title": "Top level wins",
            "metadata": { "title": "Nested", "description": "From metadata",
                          "made_for_kids": true, "privacy": "private" },
        }),
    )
    .await
    .unwrap();
    wait_done(&r.daemon, res["job_id"].as_str().unwrap()).await;
    let v = &r.yt.videos()[0];
    assert_eq!(v.title, "Top level wins");
    assert_eq!(v.description, "From metadata");
    assert_eq!(v.made_for_kids, Some(true));
    assert_eq!(v.requested_privacy, "private");
}

#[tokio::test]
async fn pub2_ac_export_preset_exports_first_and_the_publish_waits_for_it() {
    let r = rig();
    let id = record(&r.daemon).await; // no export yet
    let mut p = request(&id);
    p["export_preset"] = json!("quick");
    let res = call(&r.daemon, agent(), "publish_video", p).await.unwrap();
    // The dialog says the export comes first (the file is not known yet).
    let d = &r.confirmer.shown()[0].details;
    assert!(d["file"].as_str().unwrap().contains("`quick`"), "{d}");
    assert!(d.get("size").is_none());
    assert_eq!(res["export_preset"], "quick");
    let export_job = res["export_job_id"].as_str().unwrap().to_owned();
    let job = user(&r.daemon, "get_job", json!({"job_id": res["job_id"]})).await;
    assert_eq!(job["depends_on"], json!([export_job]));
    wait_done(&r.daemon, res["job_id"].as_str().unwrap()).await;
    assert_eq!(r.yt.videos()[0].data.len(), EXPORT_BYTES);
}

#[tokio::test]
async fn pub2_a_repeated_request_id_returns_the_first_result_and_asks_once() {
    let r = rig();
    let id = recording_with_export(&r).await;
    let mut p = request(&id);
    p["request_id"] = json!("same");
    let a = call(&r.daemon, agent(), "publish_video", p.clone())
        .await
        .unwrap();
    let b = call(&r.daemon, agent(), "publish_video", p).await.unwrap();
    assert_eq!(a, b);
    assert_eq!(r.confirmer.shown().len(), 1, "no second dialog");
    wait_done(&r.daemon, a["job_id"].as_str().unwrap()).await;
    assert_eq!(r.yt.videos().len(), 1, "one video, not two");
    let jobs = user(&r.daemon, "list_jobs", json!({"type": "publish"})).await;
    assert_eq!(jobs["jobs"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn pub2_the_daily_limit_is_checked_before_the_dialog() {
    let r = rig();
    let id = recording_with_export(&r).await;
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    for _ in 0..YOUTUBE_UPLOADS_PER_DAY {
        r.store.record_upload("yt1", now).unwrap();
    }
    let v = user(&r.daemon, "list_publish_accounts", json!({})).await;
    assert_eq!(v["accounts"][0]["uploads_today"], YOUTUBE_UPLOADS_PER_DAY);
    let e = call(&r.daemon, agent(), "publish_video", request(&id))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::PlatformRejected);
    assert!(
        e.to_string().contains("Upload limit reached for today"),
        "{e}"
    );
    assert!(r.confirmer.shown().is_empty());
    // The next Pacific day it works again (the counter is per day).
    assert_eq!(
        r.store.uploads_today("yt1", next_pacific_midnight(now) + 1),
        0
    );
}

// ---- the daily count is per Google Cloud project ------------------------------------------------

fn request_yt1(id: &str) -> Value {
    let mut v = request(id);
    v["account_id"] = json!("yt1");
    v
}

fn extra_account(id: &str, name: &str) -> PublishAccount {
    PublishAccount {
        id: id.into(),
        platform: "youtube".into(),
        name: name.into(),
        account_id: format!("UC-{id}"),
        default_privacy: Privacy::Unlisted,
    }
}

fn used_by(v: &Value, id: &str) -> u64 {
    v["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["account_id"] == id)
        .unwrap()["uploads_today"]
        .as_u64()
        .unwrap()
}

#[tokio::test]
async fn pub2_two_accounts_with_one_client_share_the_daily_count() {
    let r = rig();
    let mut all = r.store.list().unwrap();
    all.push(extra_account("yt2", "Second channel"));
    all.push(extra_account("yt3", "Other project"));
    r.store.save(&all).unwrap();

    let id = recording_with_export(&r).await;
    let res = call(&r.daemon, agent(), "publish_video", request_yt1(&id))
        .await
        .unwrap();
    let job = wait_done(&r.daemon, res["job_id"].as_str().unwrap()).await;
    assert_eq!(job["result"]["uploads_today"], 1);

    let v = user(&r.daemon, "list_publish_accounts", json!({})).await;
    assert_eq!(used_by(&v, "yt1"), 1);
    assert_eq!(used_by(&v, "yt2"), 1, "the same project, the same count");
    assert_eq!(used_by(&v, "yt3"), 0, "another project has its own");
    // The counter file keys it by project, not by channel.
    let text = std::fs::read_to_string(r._dirs.1.path().join("publish_quota.json")).unwrap();
    assert!(text.contains("google-project:123456789012"), "{text}");
    assert!(
        !text.contains("123456789012-aaaa"),
        "no client id is stored: {text}"
    );
}

#[tokio::test]
async fn pub2_the_limit_of_a_shared_project_stops_every_account_of_it() {
    let r = rig();
    let mut all = r.store.list().unwrap();
    all.push(extra_account("yt2", "Second channel"));
    all.push(extra_account("yt3", "Other project"));
    r.store.save(&all).unwrap();
    let id = recording_with_export(&r).await;
    // The second channel used up the project's 100 uploads today.
    for _ in 0..YOUTUBE_UPLOADS_PER_DAY {
        r.store
            .record_upload_in("yt2", Some("google-project:123456789012"), now())
            .unwrap();
    }
    let e = call(&r.daemon, agent(), "publish_video", request_yt1(&id))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::PlatformRejected);
    assert!(
        e.to_string().contains("Upload limit reached for today"),
        "{e}"
    );
    assert!(r.confirmer.shown().is_empty());
    let v = user(&r.daemon, "list_publish_accounts", json!({})).await;
    assert_eq!(used_by(&v, "yt1"), YOUTUBE_UPLOADS_PER_DAY as u64);
    assert_eq!(used_by(&v, "yt3"), 0);
}

#[tokio::test]
async fn pub2_a_count_kept_per_account_by_an_older_version_still_counts() {
    let r = rig();
    let id = recording_with_export(&r).await;
    // An older version counted 99 uploads under the account's own id.
    for _ in 0..(YOUTUBE_UPLOADS_PER_DAY - 1) {
        r.store.record_upload("yt1", now()).unwrap();
    }
    let v = user(&r.daemon, "list_publish_accounts", json!({})).await;
    assert_eq!(used_by(&v, "yt1"), YOUTUBE_UPLOADS_PER_DAY as u64 - 1);
    // One more upload is allowed; the job moves the old count into the project and counts it.
    let res = call(&r.daemon, agent(), "publish_video", request(&id))
        .await
        .unwrap();
    let job = wait_done(&r.daemon, res["job_id"].as_str().unwrap()).await;
    assert_eq!(job["result"]["uploads_today"], YOUTUBE_UPLOADS_PER_DAY);
    let v = user(&r.daemon, "list_publish_accounts", json!({})).await;
    assert_eq!(used_by(&v, "yt1"), YOUTUBE_UPLOADS_PER_DAY as u64);
    // And now the limit is reached.
    let e = call(&r.daemon, agent(), "publish_video", request(&id))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::PlatformRejected);
}

// ---- the job: resume, auth, privacy lock, cancel --------------------------------------------------

#[tokio::test]
async fn pub2_ac3_a_dropped_connection_resumes_from_the_last_confirmed_chunk() {
    // The provider gives up at once (`max_retries: 0`); the queue retries the job with the
    // checkpoint the first attempt saved after its chunks.
    let r = rig();
    let id = recording_with_export(&r).await;
    r.yt.inject(Call::Chunk, 2 * 256 * KIB, Fault::DropBefore, 1);
    let res = call(&r.daemon, agent(), "publish_video", request(&id))
        .await
        .unwrap();
    let job = wait_done(&r.daemon, res["job_id"].as_str().unwrap()).await;
    assert_eq!(job["attempts"], 2, "{job}");
    assert_eq!(
        r.yt.sessions_started(),
        1,
        "the same upload session continued"
    );
    assert_eq!(r.yt.videos().len(), 1);
    assert_eq!(r.yt.videos()[0].data.len(), EXPORT_BYTES);
    // No chunk was sent twice: the retry started where YouTube had stopped.
    let chunks = r.yt.chunks();
    let mut starts: Vec<u64> = chunks.iter().map(|c| c.0).collect();
    let n = starts.len();
    starts.dedup();
    assert_eq!(starts.len(), n, "{chunks:?}");
    assert!(r.yt.violations().is_empty());
    assert_eq!(publishes_in_library(&r.daemon, &id).await.len(), 1);
    // Counted once, however many attempts it took.
    let acc = user(&r.daemon, "list_publish_accounts", json!({})).await;
    assert_eq!(acc["accounts"][0]["uploads_today"], 1);
}

#[tokio::test]
async fn sto3_auth_expired_pauses_the_job_and_a_reconnect_resumes_it() {
    let r = rig();
    let id = recording_with_export(&r).await;
    // The server stops accepting the token and the grant cannot be renewed.
    r.yt.set_valid_token("fresh-token");
    let old = Arc::new(FakeTokens::new("tok-1"));
    old.revoke_on_reject();
    *r.tokens.lock().unwrap() = old;
    let res = call(&r.daemon, agent(), "publish_video", request(&id))
        .await
        .unwrap();
    let jid = res["job_id"].as_str().unwrap().to_owned();
    let j = wait_job(&r.daemon, &jid, |j| j["state"] == "paused").await;
    assert_eq!(j["pause_reason"], "auth_expired", "{j}");
    let text = j["error"].to_string();
    assert!(
        text.contains("Reconnect needed for YouTube account"),
        "{text}"
    );
    assert!(text.contains("Teacher Channel"), "{text}");
    assert!(r.yt.videos().is_empty());
    // The account list says which account to reconnect.
    let acc = user(&r.daemon, "list_publish_accounts", json!({})).await;
    assert_eq!(acc["accounts"][0]["status"], "needs_reconnect");

    // The user signs in again: the same job continues.
    *r.tokens.lock().unwrap() = Arc::new(FakeTokens::new("fresh-token"));
    assert_eq!(r.daemon.resume_auth_expired_jobs(), 1);
    let j = wait_done(&r.daemon, &jid).await;
    assert_eq!(j["result"]["video_id"], r.yt.videos()[0].id.as_str());
    let acc = user(&r.daemon, "list_publish_accounts", json!({})).await;
    assert_eq!(acc["accounts"][0]["status"], "connected");
}

#[tokio::test]
async fn pub2_a_video_youtube_kept_private_is_a_result_not_a_failure() {
    let r = rig();
    r.yt.lock_to_private();
    let id = recording_with_export(&r).await;
    let mut rx = r.daemon.subscribe();
    let res = call(&r.daemon, agent(), "publish_video", request(&id))
        .await
        .unwrap();
    let job = wait_done(&r.daemon, res["job_id"].as_str().unwrap()).await;
    let out = &job["result"];
    assert_eq!(out["status"], "privacy_locked");
    assert_eq!(out["privacy"], "private");
    assert_eq!(out["privacy_requested"], "unlisted");
    assert!(!out["warnings"].as_array().unwrap().is_empty(), "{out}");
    let rec = &publishes_in_library(&r.daemon, &id).await[0];
    assert_eq!(rec["status"], "privacy_locked");
    assert_eq!(rec["privacy"], "private");
    assert_eq!(rec["privacy_requested"], "unlisted");
    let done = drain(&mut rx)
        .into_iter()
        .find(|e| e.name == "publish.done")
        .unwrap();
    assert_eq!(done.data["status"], "privacy_locked");
    // Nothing was changed afterwards: the daemon never calls `videos.update`.
    assert!(r.yt.unexpected().is_empty(), "{:?}", r.yt.unexpected());
}

#[tokio::test]
async fn pub2_a_refused_thumbnail_is_a_warning_in_the_result() {
    let r = rig();
    r.yt.thumbnails_need_verified_account();
    let id = recording_with_export(&r).await;
    let thumb = r._dirs.1.path().join("thumb.png");
    std::fs::write(&thumb, vec![1u8; 2000]).unwrap();
    let mut p = request(&id);
    p["thumbnail"] = json!(thumb.to_str().unwrap());
    let res = call(&r.daemon, agent(), "publish_video", p).await.unwrap();
    assert_eq!(r.confirmer.shown()[0].details["thumbnail"], "thumb.png");
    let job = wait_done(&r.daemon, res["job_id"].as_str().unwrap()).await;
    let w = job["result"]["warnings"].to_string();
    assert!(w.contains("verified"), "{w}");
}

/// Holds the first chunk until the test lets it go, so a job can be cancelled mid-upload.
struct Gate {
    inner: Arc<FakeYouTube>,
    at_chunk: Arc<AtomicBool>,
    release: Arc<tokio::sync::Notify>,
    armed: AtomicBool,
}

#[async_trait]
impl cm_storage::StorageHttp for Gate {
    async fn send(
        &self,
        req: cm_storage::http::HttpRequest,
    ) -> Result<cm_storage::http::HttpReply, cm_storage::http::TransportError> {
        if req.method == "PUT" && self.armed.swap(false, Ordering::SeqCst) {
            self.at_chunk.store(true, Ordering::SeqCst);
            self.release.notified().await;
        }
        self.inner.send(req).await
    }
}

#[tokio::test]
async fn pub2_a_running_publish_can_be_cancelled_and_leaves_no_record() {
    let r = rig();
    let id = recording_with_export(&r).await;
    // Rebuild the daemon's HTTP seam with a gate in front of the fake (new daemon, same library).
    let at_chunk = Arc::new(AtomicBool::new(false));
    let release = Arc::new(tokio::sync::Notify::new());
    let gate = Arc::new(Gate {
        inner: r.yt.clone(),
        at_chunk: at_chunk.clone(),
        release: release.clone(),
        armed: AtomicBool::new(true),
    });
    let (b, _lib2) = builder(desktop());
    let queue = cm_daemon::library_job_queue(_lib2.path()).unwrap();
    // The second daemon needs the capture: copy the first library's bundle folder over.
    let src = bundle_dir(&r.root, &id);
    let dst = _lib2.path().join(src.file_name().unwrap());
    copy_dir(&src, &dst);
    let d2 = b
        .jobs(queue)
        .publish_accounts(r.store.clone())
        .cloud_backend(CloudBackend {
            tokens: Arc::new(Tokens(r.tokens.clone())),
            http: gate,
        })
        .youtube_config(YouTubeConfig {
            chunk_size: 256 * KIB,
            max_retries: 0,
            ..YouTubeConfig::default()
        })
        .confirmer(FixedConfirmer::new(Decision::Approve))
        .build();
    user(&d2, "rescan_library", json!({})).await;
    d2.register_publish_handler().unwrap();
    d2.start_jobs().unwrap();
    let res = call(&d2, agent(), "publish_video", request(&id))
        .await
        .unwrap();
    let jid = res["job_id"].as_str().unwrap().to_owned();
    for _ in 0..3000 {
        if at_chunk.load(Ordering::SeqCst) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(
        at_chunk.load(Ordering::SeqCst),
        "the upload never reached its first chunk; job: {}",
        user(&d2, "get_job", json!({ "job_id": jid })).await
    );
    user(&d2, "cancel_job", json!({ "job_id": jid })).await;
    release.notify_one();
    let j = wait_job(&d2, &jid, |j| j["state"] == "cancelled").await;
    assert_eq!(j["state"], "cancelled");
    assert!(
        r.yt.videos().is_empty(),
        "the cancelled upload finished nothing"
    );
    assert!(publishes_in_library(&d2, &id).await.is_empty());
    assert_eq!(r.store.uploads_today("yt1", now()), 0);
}

fn now() -> i64 {
    time::OffsetDateTime::now_utc().unix_timestamp()
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let (src, dst) = (e.path(), to.join(e.file_name()));
        if src.is_dir() {
            copy_dir(&src, &dst);
        } else {
            std::fs::copy(&src, &dst).unwrap();
        }
    }
}

// ---- holds, and the Never tier --------------------------------------------------------------------

#[tokio::test]
async fn sto3_a_publish_waits_while_a_recording_runs() {
    let r = rig();
    let id = recording_with_export(&r).await;
    // A recording starts: the publish type is held with uploads.
    user(
        &r.daemon,
        "start_recording",
        json!({ "target": { "display_id": 1 }, "countdown_s": 0 }),
    )
    .await;
    assert!(r.daemon.is_job_type_held("publish"));
    let res = call(&r.daemon, agent(), "publish_video", request(&id))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert!(r.yt.videos().is_empty(), "held while recording");
    let j = user(&r.daemon, "get_job", json!({"job_id": res["job_id"]})).await;
    assert_eq!(j["held"], true, "{j}");
    user(&r.daemon, "stop_recording", json!({})).await;
    wait_done(&r.daemon, res["job_id"].as_str().unwrap()).await;
    assert_eq!(r.yt.videos().len(), 1);
}

/// The Never tier: no action and no parameter changes the privacy of a video that is already on
/// the platform, and the provider never calls an update route.
#[tokio::test]
async fn agt2_nothing_can_change_the_privacy_of_an_existing_remote_video() {
    for s in cm_daemon::ACTIONS {
        let n = s.name;
        assert!(
            !n.contains("privacy") && !n.contains("visibility") && !n.contains("update_video"),
            "`{n}` looks like a way to change a remote privacy"
        );
    }
    // The only publishing actions: the Free read and the Confirm publish of a new video.
    let publishing: Vec<_> = cm_daemon::ACTIONS
        .iter()
        .filter(|s| s.name.contains("publish"))
        .map(|s| (s.name, s.tier))
        .collect();
    assert_eq!(
        publishing,
        vec![
            ("publish_video", cm_daemon::Tier::Confirm),
            ("list_publish_accounts", cm_daemon::Tier::Free)
        ]
    );
    let r = rig();
    // A full publish never touches an update route.
    let id = recording_with_export(&r).await;
    let mut p = request(&id);
    p["privacy"] = json!("public");
    let res = call(&r.daemon, agent(), "publish_video", p).await.unwrap();
    wait_done(&r.daemon, res["job_id"].as_str().unwrap()).await;
    assert!(r.yt.unexpected().is_empty(), "{:?}", r.yt.unexpected());
    assert!(!r
        .yt
        .log()
        .iter()
        .any(|l| l.starts_with("PUT") && l.contains("/videos?") && !l.contains("upload")));
}

#[tokio::test]
async fn pub2_export_path_names_the_export_the_sheet_shows() {
    let r = rig();
    let id = recording_with_export(&r).await;
    let dir = bundle_dir(&r.root, &id);
    let given = dir.join("exports/Take - youtube-1080p.mp4");
    let mut p = request(&id);
    p["export_path"] = json!(given.to_str().unwrap());
    let res = call(&r.daemon, agent(), "publish_video", p).await.unwrap();
    assert_eq!(res["file"], "exports/Take - youtube-1080p.mp4");
    wait_done(&r.daemon, res["job_id"].as_str().unwrap()).await;
    // Outside the capture, relative, or together with a preset: refused before a dialog.
    let shown = r.confirmer.shown().len();
    for path in ["/etc/hosts", "exports/x.mp4"] {
        let mut p = request(&id);
        p["export_path"] = json!(path);
        let e = call(&r.daemon, agent(), "publish_video", p)
            .await
            .unwrap_err();
        assert_eq!(code(&e), ErrorCode::InvalidInput, "{e:?}");
    }
    let mut p = request(&id);
    p["export_path"] = json!(given.to_str().unwrap());
    p["export_preset"] = json!("quick");
    assert!(call(&r.daemon, agent(), "publish_video", p).await.is_err());
    assert_eq!(r.confirmer.shown().len(), shown);
}

/// The dialog lists the playlist right after the tags, as the id only (no network before approval).
#[tokio::test]
async fn pub2_the_dialog_shows_the_playlist_id_after_the_tags() {
    let r = rig();
    let id = recording_with_export(&r).await;
    let mut p = request(&id);
    p["playlist_id"] = json!("PLabc123");
    p["thumbnail"] = json!(r._dirs.1.path().join("t.png").to_str().unwrap());
    std::fs::write(r._dirs.1.path().join("t.png"), vec![1u8; 2000]).unwrap();
    call(&r.daemon, agent(), "publish_video", p).await.unwrap();
    let d = &r.confirmer.shown()[0].details;
    assert_eq!(d["playlist"], "PLabc123");
    let keys: Vec<&str> = d.as_object().unwrap().keys().map(|k| k.as_str()).collect();
    let pos = |k: &str| keys.iter().position(|x| *x == k).unwrap();
    assert_eq!(pos("playlist"), pos("tags") + 1, "{keys:?}");
    assert!(pos("playlist") < pos("thumbnail"), "{keys:?}");
    assert!(pos("playlist") < pos("sends"), "{keys:?}");
}
