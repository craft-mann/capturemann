//! LIB-2.AC5 to AC12: `move_library` and the `library_move` job (tested with fakes and temp
//! folders; no real drive).

#![allow(clippy::unwrap_used)]

mod common;

use cm_capture::disk::FakeFreeSpace;
use cm_core::{Actor, ErrorCode};
use cm_daemon::library_move::{
    library_usage, list_bundles, verify_copy, FileCopier, StdCopier, KEY_LIBRARY_PATH,
};
use cm_daemon::testing::{FakeTrasher, FixedConfirmer};
use cm_daemon::{
    ClientPolicy, Daemon, DaemonError, Decision, MemorySettings, MoveMode, MoveProblem,
};
use cm_jobs::{NewJob, JOB_EXPORT};
use common::{builder, desktop, Client};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Copies like [`StdCopier`] but can be told to fail, or to be slow (so a test can cancel).
#[derive(Default)]
struct Script {
    fail: AtomicBool,
    slow: AtomicBool,
    copied: AtomicUsize,
}

struct Shared(Arc<Script>);

impl FileCopier for Shared {
    fn copy_file(
        &self,
        from: &Path,
        to: &Path,
        interrupted: &dyn Fn() -> bool,
        progress: &mut dyn FnMut(u64),
    ) -> std::io::Result<()> {
        let me = &self.0;
        if me.slow.load(Ordering::SeqCst) {
            for _ in 0..200 {
                if interrupted() {
                    return Err(std::io::ErrorKind::Interrupted.into());
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        if me.fail.load(Ordering::SeqCst) && me.copied.load(Ordering::SeqCst) >= 3 {
            return Err(std::io::Error::other(
                "Input/output error (drive unplugged)",
            ));
        }
        StdCopier.copy_file(from, to, interrupted, progress)?;
        me.copied.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

struct Rig {
    daemon: Daemon,
    confirmer: Arc<FixedConfirmer>,
    trasher: Arc<FakeTrasher>,
    script: Arc<Script>,
    free: Arc<FakeFreeSpace>,
    lib: tempfile::TempDir,
    dest: tempfile::TempDir,
    _trash: tempfile::TempDir,
}

fn rig(decision: Decision) -> Rig {
    let (b, lib) = builder(desktop());
    let trash = tempfile::tempdir().unwrap();
    let confirmer = Arc::new(FixedConfirmer::new(decision));
    let trasher = Arc::new(FakeTrasher::into_dir(trash.path()));
    let script = Arc::new(Script::default());
    let free = Arc::new(FakeFreeSpace::new(u64::MAX));
    struct Free(Arc<FakeFreeSpace>);
    impl cm_capture::disk::FreeSpace for Free {
        fn free_bytes(&self, p: &Path) -> Option<u64> {
            cm_capture::disk::FreeSpace::free_bytes(&*self.0, p)
        }
    }
    let mut defaults = Map::new();
    // A hand-edited policy that tries to make the move run without asking.
    defaults.insert(
        "agents.tool_policy".into(),
        json!({ "move_library": "allow" }),
    );
    let daemon = b
        .confirmer(confirmer.clone())
        .trasher(trasher.clone())
        .settings(MemorySettings::with_defaults(defaults))
        .free_space(Free(free.clone()))
        .move_copier(Shared(script.clone()))
        .jobs(cm_daemon::library_job_queue(lib.path()).unwrap())
        .build();
    daemon.register_library_move_handler().unwrap();
    Rig {
        daemon,
        confirmer,
        trasher,
        script,
        free,
        lib,
        dest: tempfile::tempdir().unwrap(),
        _trash: trash,
    }
}

async fn shot(d: &Daemon, title: &str) -> Value {
    d.execute_method(
        Actor::User,
        0,
        "capture_screenshot",
        &json!({"target": {"display_id": 1}, "title": title}),
    )
    .await
    .unwrap()
}

fn agent() -> Actor {
    Actor::agent("claude-desktop").unwrap()
}

async fn mv(r: &Rig, params: Value) -> Result<Value, DaemonError> {
    r.daemon
        .execute_method(agent(), 1, "move_library", &params)
        .await
}

fn target(r: &Rig, sub: &str) -> PathBuf {
    r.dest.path().join(sub)
}

async fn wait_state(d: &Daemon, id: &str, want: &[&str]) -> Value {
    for _ in 0..600 {
        let j = d
            .execute_method(Actor::User, 0, "get_job", &json!({"job_id": id}))
            .await
            .unwrap();
        if want.contains(&j["state"].as_str().unwrap()) {
            return j;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("job {id} never reached {want:?}");
}

async fn setting(d: &Daemon, key: &str) -> Value {
    match d
        .execute_method(Actor::User, 0, "get_settings", &json!({"keys": [key]}))
        .await
    {
        Ok(r) => r["settings"][key].clone(),
        Err(e) if e.code() == Some(ErrorCode::NotFound) => Value::Null,
        Err(e) => panic!("{e:?}"),
    }
}

fn files_of(root: &Path) -> Vec<(String, u64)> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            // The job queue and the index live in `.capturemann` and change while jobs run.
            if e.file_name() == ".capturemann" {
                continue;
            }
            if e.path().is_dir() {
                stack.push(e.path());
            } else {
                out.push((
                    e.path().strip_prefix(root).unwrap().display().to_string(),
                    e.metadata().unwrap().len(),
                ));
            }
        }
    }
    out.sort();
    out
}

async fn three_shots(r: &Rig) {
    for t in ["One", "Two", "Three"] {
        shot(&r.daemon, t).await;
    }
}

#[tokio::test]
async fn lib2_ac5_move_copies_verifies_then_switches_and_never_deletes() {
    let r = rig(Decision::Approve);
    three_shots(&r).await;
    r.daemon.start_jobs().unwrap();
    let to = target(&r, "New Library");
    let before = files_of(r.lib.path());
    let mut events = r.daemon.subscribe();
    let started = mv(&r, json!({"path": to, "request_id": "m1"}))
        .await
        .unwrap();
    assert_eq!(started["restart_required"], true);
    assert_eq!(started["mode"], "move");
    assert_eq!(started["bundles"], 3);
    let id = started["job_id"].as_str().unwrap().to_owned();
    // The dialog said exactly what runs: old path, new path, size, a move.
    let shown = r.confirmer.shown();
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].kind, "move_library");
    assert!(
        shown[0].summary.starts_with("Move the library from "),
        "{}",
        shown[0].summary
    );
    assert!(shown[0].summary.contains("3 captures"));
    assert_eq!(shown[0].params["mode"], "move");
    assert_eq!(shown[0].params["bundles"], 3);
    assert_eq!(shown[0].details["to"], json!(to.display().to_string()));
    assert!(!shown[0].allow_session);
    // The setting does not change before the job is done.
    let job = wait_state(&r.daemon, &id, &["done", "failed"]).await;
    assert_eq!(job["state"], "done", "{job}");
    assert_eq!(job["type"], "library_move");
    assert_eq!(job["result"]["bundles"], 3);
    assert_eq!(job["result"]["restart_required"], true);
    assert_eq!(
        setting(&r.daemon, KEY_LIBRARY_PATH).await,
        json!(to.to_string_lossy())
    );
    // Old library untouched, every bundle of the new one verified against it.
    assert_eq!(files_of(r.lib.path()), before);
    let names = list_bundles(&to).unwrap();
    assert_eq!(names.len(), 3);
    for n in &names {
        verify_copy(&r.lib.path().join(n), &to.join(n)).unwrap();
    }
    assert!(!names.iter().any(|n| n.ends_with(".cm-copying")));
    // Events: started, progress, moved.
    let mut seen = Vec::new();
    while let Ok(e) = events.try_recv() {
        seen.push(e);
    }
    let started_ev = seen
        .iter()
        .find(|e| e.name == "library.move_started")
        .unwrap();
    assert_eq!(started_ev.data["job_id"], json!(id));
    assert_eq!(started_ev.data["bundles"], 3);
    assert!(seen
        .iter()
        .any(|e| e.name == "job.progress" && e.data["type"] == "library_move"));
    let moved = seen.iter().find(|e| e.name == "library.moved").unwrap();
    assert_eq!(moved.data["restart_required"], true);
    assert_eq!(moved.data["bundles"], 3);
    assert_eq!(moved.data["from"], json!(r.lib.path()));
    assert_eq!(moved.data["to"], json!(to));
    assert!(moved.data["bytes"].as_u64().unwrap() > 0);
}

#[tokio::test]
async fn lib2_ac6_move_library_is_confirm_tier_and_cannot_be_loosened() {
    // Declined: nothing is copied, no job exists, the setting is untouched. The settings hold a
    // hand-edited `agents.tool_policy` that says "allow"; no setting changes a tier.
    let r = rig(Decision::Decline);
    three_shots(&r).await;
    let to = target(&r, "x");
    let e = mv(&r, json!({"path": to})).await.unwrap_err();
    assert_eq!(e.code(), Some(ErrorCode::UserDeclined));
    assert_eq!(r.confirmer.shown().len(), 1);
    assert!(!to.exists());
    assert_eq!(setting(&r.daemon, KEY_LIBRARY_PATH).await, Value::Null);
    let spec = cm_daemon::spec_for("move_library").unwrap();
    assert_eq!(spec.tier, cm_daemon::Tier::Confirm);
    assert!(spec.mutating);
    assert!(!cm_daemon::is_internal("move_library"));
    // Read-only and blocked clients are refused before any dialog.
    for (p, reason) in [
        (ClientPolicy::ReadOnly, "client_read_only"),
        (ClientPolicy::Blocked, "client_blocked"),
    ] {
        r.daemon.policies().set("claude-desktop", p);
        let e = mv(&r, json!({"path": to})).await.unwrap_err();
        assert!(
            matches!(e, DaemonError::Declined { reason: x, .. } if x == reason),
            "{e:?}"
        );
    }
    assert_eq!(r.confirmer.shown().len(), 1);
    // The user (in process) is not gated: the app asks itself.
    r.daemon
        .policies()
        .set("claude-desktop", ClientPolicy::Allowed);
    // A headless daemon never moves for an agent.
    let (b, dir) = builder(desktop());
    let d = b
        .jobs(cm_daemon::library_job_queue(dir.path()).unwrap())
        .build();
    let mut c = Client::connect(&d, "y");
    c.hello("claude-desktop").await;
    let resp = c.call("move_library", json!({"path": to})).await;
    assert_eq!(resp["error"]["data"]["data"]["reason"], "no_confirmer");
}

#[tokio::test]
async fn lib2_ac7_set_settings_still_refuses_library_path_for_agents() {
    let r = rig(Decision::Approve);
    let e = r
        .daemon
        .execute_method(
            agent(),
            1,
            "set_settings",
            &json!({"values": {"library.path": "/tmp/x"}}),
        )
        .await
        .unwrap_err();
    assert!(matches!(
        e,
        DaemonError::Declined {
            reason: "app_only_setting",
            ..
        }
    ));
}

#[tokio::test]
async fn lib2_ac8_every_precondition_fails_before_any_dialog_or_copy() {
    let r = rig(Decision::Approve);
    three_shots(&r).await;
    let reason = |e: &DaemonError| match e {
        DaemonError::WithData { data, .. } => data["reason"].as_str().unwrap().to_owned(),
        other => panic!("{other:?}"),
    };
    // Inside the library, the same folder, a file, not absolute.
    let e = mv(&r, json!({"path": r.lib.path().join("sub")}))
        .await
        .unwrap_err();
    assert_eq!(
        (reason(&e), e.code()),
        (
            "target_inside_library".into(),
            Some(ErrorCode::InvalidInput)
        )
    );
    let e = mv(&r, json!({"path": r.lib.path()})).await.unwrap_err();
    assert_eq!(reason(&e), "same_folder");
    let e = mv(&r, json!({"path": r.lib.path().parent().unwrap()}))
        .await
        .unwrap_err();
    assert_eq!(reason(&e), "target_contains_library");
    let file = target(&r, "f.txt");
    std::fs::write(&file, b"x").unwrap();
    let e = mv(&r, json!({"path": file})).await.unwrap_err();
    assert_eq!(reason(&e), "target_is_file");
    let e = mv(&r, json!({"path": "relative/folder"}))
        .await
        .unwrap_err();
    assert_eq!(reason(&e), "target_not_absolute");
    // Not enough space: 1.1 x the library size.
    r.free.set(10);
    let e = mv(&r, json!({"path": target(&r, "full")}))
        .await
        .unwrap_err();
    assert_eq!(
        (reason(&e), e.code()),
        ("not_enough_space".into(), Some(ErrorCode::DiskFull))
    );
    r.free.set(u64::MAX);
    // A job is running: busy, with the job named; nothing is cancelled.
    let queue = r.daemon.job_queue().unwrap();
    let job = queue
        .enqueue(NewJob::new(JOB_EXPORT, json!({"capture_id": "x"})))
        .unwrap();
    let e = mv(&r, json!({"path": target(&r, "busy")}))
        .await
        .unwrap_err();
    assert_eq!(
        (reason(&e), e.code()),
        ("job_running".into(), Some(ErrorCode::Busy))
    );
    match &e {
        DaemonError::WithData { data, .. } => assert_eq!(data["job_id"], json!(job.id)),
        _ => unreachable!(),
    }
    assert_eq!(queue.get(&job.id).unwrap().state.as_str(), "queued");
    // Unknown mode and unknown field.
    assert_eq!(
        mv(&r, json!({"path": "/x", "mode": "copy"}))
            .await
            .unwrap_err()
            .code(),
        Some(ErrorCode::InvalidInput)
    );
    assert_eq!(
        mv(&r, json!({"path": "/x", "delete_old": true}))
            .await
            .unwrap_err()
            .code(),
        Some(ErrorCode::InvalidInput)
    );
    // None of it opened a dialog or wrote anything.
    assert!(r.confirmer.shown().is_empty());
    assert_eq!(setting(&r.daemon, KEY_LIBRARY_PATH).await, Value::Null);
}

#[tokio::test]
async fn lib2_ac9_an_existing_library_offers_use_it_instead_of_a_move() {
    let r = rig(Decision::Approve);
    shot(&r.daemon, "Old").await;
    // The new folder already holds a library.
    let there = target(&r, "Elsewhere");
    std::fs::create_dir_all(there.join("Old.cmshot")).unwrap();
    std::fs::write(there.join("Old.cmshot/manifest.json"), b"{}").unwrap();
    let p = r
        .daemon
        .check_library_move(&there, MoveMode::Move)
        .await
        .unwrap_err();
    assert_eq!(
        p,
        MoveProblem::TargetHasLibrary {
            path: there.display().to_string(),
            bundles: 1
        }
    );
    let plan = r
        .daemon
        .check_library_move(&there, MoveMode::UseExisting)
        .await
        .unwrap();
    assert!(plan.to_has_library);
    assert_eq!(plan.bundles, 1);
    // The agent surface says the same.
    let e = mv(&r, json!({"path": there})).await.unwrap_err();
    assert_eq!(e.code(), Some(ErrorCode::InvalidInput));
    assert!(e.to_string().contains("use_existing"));
    let mut events = r.daemon.subscribe();
    let out = mv(&r, json!({"path": there, "mode": "use_existing"}))
        .await
        .unwrap();
    assert_eq!(out["job_id"], Value::Null);
    assert_eq!(out["state"], "done");
    assert_eq!(out["restart_required"], true);
    assert_eq!(
        setting(&r.daemon, KEY_LIBRARY_PATH).await,
        json!(there.to_string_lossy())
    );
    let mut moved = None;
    while let Ok(e) = events.try_recv() {
        if e.name == "library.moved" {
            moved = Some(e);
        }
    }
    assert_eq!(moved.unwrap().data["mode"], "use_existing");
    // Nothing was copied to or from the old library.
    assert_eq!(list_bundles(&there).unwrap().len(), 1);
    // `use_existing` on an empty folder is an error.
    let empty = target(&r, "empty");
    std::fs::create_dir_all(&empty).unwrap();
    let e = mv(&r, json!({"path": empty, "mode": "use_existing"}))
        .await
        .unwrap_err();
    assert_eq!(e.code(), Some(ErrorCode::InvalidInput));
}

#[tokio::test]
async fn lib2_ac10_fresh_only_writes_the_setting_and_leaves_old_files() {
    let r = rig(Decision::Approve);
    three_shots(&r).await;
    let before = files_of(r.lib.path());
    let to = target(&r, "Fresh");
    // No free space is needed for a fresh start.
    r.free.set(0);
    let out = mv(&r, json!({"path": to, "mode": "fresh"})).await.unwrap();
    assert_eq!(out["job_id"], Value::Null);
    assert_eq!(out["bundles"], 0);
    assert_eq!(
        setting(&r.daemon, KEY_LIBRARY_PATH).await,
        json!(to.to_string_lossy())
    );
    assert_eq!(files_of(r.lib.path()), before);
    let shown = &r.confirmer.shown()[0];
    assert!(shown.summary.starts_with("Start a new empty library at "));
    assert!(shown.summary.contains("stay where they are"));
}

#[tokio::test]
async fn lib2_ac11_a_failed_copy_keeps_the_old_library_and_a_retry_resumes() {
    let r = rig(Decision::Approve);
    three_shots(&r).await;
    r.daemon.start_jobs().unwrap();
    let to = target(&r, "Drive");
    let before = files_of(r.lib.path());
    r.script.fail.store(true, Ordering::SeqCst);
    let out = mv(&r, json!({"path": to})).await.unwrap();
    let id = out["job_id"].as_str().unwrap().to_owned();
    let job = wait_state(&r.daemon, &id, &["failed", "done"]).await;
    assert_eq!(job["state"], "failed");
    let reason = job["error"]["reason"].as_str().unwrap();
    assert!(
        reason.contains("Nothing was deleted from the old folder"),
        "{reason}"
    );
    // The setting did not move; the old library is whole; the new folder holds only whole bundles.
    assert_eq!(setting(&r.daemon, KEY_LIBRARY_PATH).await, Value::Null);
    assert_eq!(files_of(r.lib.path()), before);
    let partial = list_bundles(&to).unwrap();
    assert_eq!(partial.len(), 1);
    assert!(!std::fs::read_dir(&to)
        .unwrap()
        .flatten()
        .any(|e| e.file_name().to_string_lossy().ends_with(".cm-copying")));
    // The drive is back: retry resumes; the finished bundle is not copied again.
    r.script.fail.store(false, Ordering::SeqCst);
    let copied_before = r.script.copied.load(Ordering::SeqCst);
    r.daemon
        .execute_method(Actor::User, 0, "retry_job", &json!({"job_id": id}))
        .await
        .unwrap();
    let job = wait_state(&r.daemon, &id, &["done", "failed"]).await;
    assert_eq!(job["state"], "done", "{job}");
    assert_eq!(list_bundles(&to).unwrap().len(), 3);
    assert_eq!(
        r.script.copied.load(Ordering::SeqCst) - copied_before,
        files_of_count(&r, 2),
        "only the two missing bundles were copied"
    );
    assert_eq!(
        setting(&r.daemon, KEY_LIBRARY_PATH).await,
        json!(to.to_string_lossy())
    );
    assert_eq!(files_of(r.lib.path()), before);
}

/// Files in `n` bundles of the old library (a screenshot bundle has the same files each time).
fn files_of_count(r: &Rig, n: usize) -> usize {
    let names = list_bundles(r.lib.path()).unwrap();
    let per: usize = {
        let f = files_of(&r.lib.path().join(&names[0]));
        f.iter().filter(|(p, _)| !p.is_empty()).count()
    };
    per * n
}

#[tokio::test]
async fn lib2_ac12_cancel_keeps_the_old_library_active_and_the_copied_bundles() {
    let r = rig(Decision::Approve);
    three_shots(&r).await;
    r.daemon.start_jobs().unwrap();
    let to = target(&r, "Cancelled");
    let before = files_of(r.lib.path());
    r.script.slow.store(true, Ordering::SeqCst);
    let out = mv(&r, json!({"path": to})).await.unwrap();
    let id = out["job_id"].as_str().unwrap().to_owned();
    wait_state(&r.daemon, &id, &["running"]).await;
    let c = r
        .daemon
        .execute_method(Actor::User, 0, "cancel_job", &json!({"job_id": id}))
        .await
        .unwrap();
    assert!(c["cancelling"].as_bool().unwrap_or(false) || c["state"] == "cancelled");
    let job = wait_state(&r.daemon, &id, &["cancelled"]).await;
    assert_eq!(job["state"], "cancelled");
    assert_eq!(setting(&r.daemon, KEY_LIBRARY_PATH).await, Value::Null);
    assert_eq!(files_of(r.lib.path()), before);
    if to.exists() {
        assert!(!std::fs::read_dir(&to)
            .unwrap()
            .flatten()
            .any(|e| e.file_name().to_string_lossy().ends_with(".cm-copying")));
    }
}

#[tokio::test]
async fn lib2_a_second_move_is_refused_while_one_is_running() {
    let r = rig(Decision::Approve);
    three_shots(&r).await;
    r.daemon.start_jobs().unwrap();
    r.script.slow.store(true, Ordering::SeqCst);
    let out = mv(&r, json!({"path": target(&r, "a")})).await.unwrap();
    let id = out["job_id"].as_str().unwrap().to_owned();
    let e = mv(&r, json!({"path": target(&r, "b")})).await.unwrap_err();
    assert_eq!(e.code(), Some(ErrorCode::Busy));
    assert!(e.to_string().contains("already being moved"));
    r.daemon
        .execute_method(Actor::User, 0, "cancel_job", &json!({"job_id": id}))
        .await
        .unwrap();
    wait_state(&r.daemon, &id, &["cancelled"]).await;
}

#[tokio::test]
async fn lib2_ac13_the_trash_step_only_takes_verified_copies_and_never_deletes() {
    let r = rig(Decision::Approve);
    three_shots(&r).await;
    r.daemon.start_jobs().unwrap();
    let to = target(&r, "Moved");
    let out = mv(&r, json!({"path": to})).await.unwrap();
    let id = out["job_id"].as_str().unwrap().to_owned();
    let job = wait_state(&r.daemon, &id, &["done"]).await;
    let names: Vec<String> = job["result"]["copied"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(names.len(), 3);
    // One copy is damaged afterwards; one name was never copied; one is not a bundle name.
    let damaged = to.join(&names[0]);
    let file = std::fs::read_dir(&damaged)
        .unwrap()
        .flatten()
        .find(|e| e.file_name() != "manifest.json" && e.path().is_file())
        .unwrap();
    std::fs::write(file.path(), b"damaged").unwrap();
    let mut ask = names.clone();
    ask.push("Ghost.cmrec".into());
    ask.push("../escape.cmrec".into());
    ask.push("notes.txt".into());
    let report = r
        .daemon
        .trash_old_copies(r.lib.path(), &to, &ask)
        .await
        .unwrap();
    assert_eq!(report.trashed, names[1..].to_vec());
    let why: std::collections::BTreeMap<_, _> = report.kept.iter().cloned().collect();
    assert_eq!(why[&names[0]], "changed");
    assert_eq!(why["Ghost.cmrec"], "not_found");
    assert_eq!(why["../escape.cmrec"], "not_a_bundle");
    assert_eq!(why["notes.txt"], "not_a_bundle");
    // Trashed (moved to the Trash folder), not deleted; the damaged one stays; the new copies stay.
    assert_eq!(r.trasher.trashed().len(), 2);
    assert!(r.lib.path().join(&names[0]).is_dir());
    assert!(!r.lib.path().join(&names[1]).exists());
    assert_eq!(library_usage(&to).unwrap().bundles, 3);
    // The same folder is refused.
    assert!(r.daemon.trash_old_copies(&to, &to, &names).await.is_err());
}
