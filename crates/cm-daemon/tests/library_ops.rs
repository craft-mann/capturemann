//! `rename_capture`, `delete_capture` (local part: the Trash), `rescan_library`, and the
//! `annotate` options that keep the editor cheap (`return_image`).

#![allow(clippy::unwrap_used)]

mod common;

use cm_core::{Actor, ErrorCode};
use cm_daemon::testing::{FakeTrasher, FixedConfirmer};
use cm_daemon::{ClientPolicy, Daemon, DaemonError, Decision};
use cm_jobs::{NewJob, JOB_EXPORT, JOB_PUBLISH, JOB_UPLOAD};
use common::{builder, desktop, Client};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;

struct Rig {
    daemon: Daemon,
    confirmer: Arc<FixedConfirmer>,
    trasher: Arc<FakeTrasher>,
    dir: tempfile::TempDir,
    _trash: tempfile::TempDir,
}

fn rig(decision: Decision) -> Rig {
    let (b, dir) = builder(desktop());
    let trash = tempfile::tempdir().unwrap();
    let confirmer = Arc::new(FixedConfirmer::new(decision));
    let trasher = Arc::new(FakeTrasher::into_dir(trash.path()));
    let daemon = b
        .confirmer(confirmer.clone())
        .trasher(trasher.clone())
        .jobs(cm_daemon::library_job_queue(dir.path()).unwrap())
        .build();
    Rig {
        daemon,
        confirmer,
        trasher,
        dir,
        _trash: trash,
    }
}

fn agent(n: &str) -> Actor {
    Actor::agent(n).unwrap()
}

fn code(e: &DaemonError) -> ErrorCode {
    e.code().unwrap()
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

const RECT: &str = r##"[{"op":"rect","rect":[5,5,20,20],"color":"#2E8BFF","width":2}]"##;

// ---- rename_capture ---------------------------------------------------------------------------

#[tokio::test]
async fn rename_renames_folder_and_title_and_later_edits_work() {
    let r = rig(Decision::Decline);
    let d = &r.daemon;
    let s = shot(d, "Before").await;
    let id = s["capture_id"].as_str().unwrap().to_owned();
    let old_dir = PathBuf::from(s["path"].as_str().unwrap())
        .parent()
        .unwrap()
        .to_owned();
    let mut sub = d.subscribe();
    let out = d
        .execute_method(
            Actor::User,
            0,
            "rename_capture",
            &json!({"capture_id": id, "title": "After rename"}),
        )
        .await
        .unwrap();
    assert_eq!(out["title"], "After rename");
    let new_dir = PathBuf::from(out["path"].as_str().unwrap());
    assert!(new_dir.ends_with("After rename.cmshot"), "{new_dir:?}");
    assert!(new_dir.is_dir() && !old_dir.exists());
    let ev = sub.recv().await.unwrap();
    assert_eq!(ev.name, "capture.revised");
    assert_eq!(ev.data["renamed"], true);
    assert_eq!(ev.data["capture_id"], json!(id));
    // The handlers open bundles transiently: annotate, get and undo use the new folder.
    let ann = d
        .execute_method(
            Actor::User,
            0,
            "annotate",
            &json!({"capture_id": id, "ops": serde_json::from_str::<Value>(RECT).unwrap()}),
        )
        .await
        .unwrap();
    assert_eq!(ann["revision"], 1);
    let got = d
        .execute_method(
            Actor::User,
            0,
            "get_capture",
            &json!({"capture_id": id, "include_image": false}),
        )
        .await
        .unwrap();
    assert_eq!(got["title"], "After rename");
    assert!(got["path"]
        .as_str()
        .unwrap()
        .ends_with("After rename.cmshot"));
    let list = d
        .execute_method(Actor::User, 0, "list_library", &json!({}))
        .await
        .unwrap();
    assert_eq!(list["captures"][0]["title"], "After rename");
}

#[tokio::test]
async fn rename_is_free_tier_idempotent_and_policy_checked() {
    let r = rig(Decision::Decline);
    let d = &r.daemon;
    let id = shot(d, "One").await["capture_id"]
        .as_str()
        .unwrap()
        .to_owned();
    // No dialog for an agent.
    let p = json!({"capture_id": id, "title": "Two", "request_id": "rn-1"});
    let a = d
        .execute_method(agent("claude"), 0, "rename_capture", &p)
        .await
        .unwrap();
    assert!(r.confirmer.shown().is_empty());
    // The same request id returns the first result; a different title is not applied.
    let again = d
        .execute_method(
            agent("claude"),
            0,
            "rename_capture",
            &json!({"capture_id": id, "title": "Three", "request_id": "rn-1"}),
        )
        .await
        .unwrap();
    assert_eq!(a, again);
    // Read-only clients cannot rename.
    d.policies().set("viewer", ClientPolicy::ReadOnly);
    let e = d
        .execute_method(
            agent("viewer"),
            0,
            "rename_capture",
            &json!({"capture_id": id, "title": "X"}),
        )
        .await
        .unwrap_err();
    assert!(matches!(
        e,
        DaemonError::Declined {
            reason: "client_read_only",
            ..
        }
    ));
    // Unknown id and bad input.
    let e = d
        .execute_method(
            Actor::User,
            0,
            "rename_capture",
            &json!({"capture_id": "shot_nope", "title": "X"}),
        )
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::NotFound);
    let e = d
        .execute_method(
            Actor::User,
            0,
            "rename_capture",
            &json!({"capture_id": id, "title": "  "}),
        )
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::InvalidInput);
}

#[tokio::test]
async fn rename_and_delete_refuse_a_capture_being_recorded_or_exported() {
    let r = rig(Decision::Approve);
    let d = &r.daemon;
    let s = shot(d, "Busy one").await;
    let id = s["capture_id"].as_str().unwrap().to_owned();
    let dir = PathBuf::from(s["path"].as_str().unwrap())
        .parent()
        .unwrap()
        .to_owned();

    // An unfinished recording has a `recording.lock` in its folder.
    std::fs::write(dir.join("recording.lock"), "{}").unwrap();
    for (m, p) in [
        ("rename_capture", json!({"capture_id": id, "title": "Z"})),
        ("delete_capture", json!({"capture_id": id})),
    ] {
        for who in [Actor::User, agent("claude")] {
            let e = d.execute_method(who, 0, m, &p).await.unwrap_err();
            assert_eq!(code(&e), ErrorCode::Busy, "{m}: {e:?}");
        }
    }
    std::fs::remove_file(dir.join("recording.lock")).unwrap();
    assert!(r.trasher.trashed().is_empty());

    // A queued export job for the capture blocks them as well.
    let job = d
        .job_queue()
        .unwrap()
        .enqueue(NewJob::new(
            JOB_EXPORT,
            json!({"capture_id": id, "preset": "quick"}),
        ))
        .unwrap();
    for (m, p) in [
        ("rename_capture", json!({"capture_id": id, "title": "Z"})),
        ("delete_capture", json!({"capture_id": id})),
    ] {
        let e = d.execute_method(Actor::User, 0, m, &p).await.unwrap_err();
        assert_eq!(code(&e), ErrorCode::Busy, "{m}");
    }
    // Cancelled, it no longer does.
    d.job_queue().unwrap().cancel(&job.id).unwrap();
    d.execute_method(
        Actor::User,
        0,
        "rename_capture",
        &json!({"capture_id": id, "title": "Z"}),
    )
    .await
    .unwrap();
}

/// An upload or a publish job that is queued, running or paused blocks rename and delete and the
/// message names the job; a failed, cancelled or finished one does not.
#[tokio::test]
async fn rename_and_delete_refuse_a_capture_while_an_upload_or_publish_job_is_active() {
    let r = rig(Decision::Approve);
    let d = &r.daemon;
    let s = shot(d, "Being uploaded").await;
    let id = s["capture_id"].as_str().unwrap().to_owned();
    let queue = d.job_queue().unwrap();
    let rename = json!({"capture_id": id, "title": "Z"});
    let delete = json!({"capture_id": id});
    let delete_remote = json!({"capture_id": id, "also_delete_remote": true});

    for job_type in [JOB_UPLOAD, JOB_PUBLISH] {
        let noun = if job_type == JOB_UPLOAD {
            "uploaded"
        } else {
            "published"
        };
        // Another capture's job does not matter.
        let other = queue
            .enqueue(NewJob::new(job_type, json!({"capture_id": "someone-else"})))
            .unwrap();
        let job = queue
            .enqueue(NewJob::new(job_type, json!({"capture_id": id})))
            .unwrap();
        let blocked = |what: &str, e: &DaemonError| {
            assert_eq!(code(e), ErrorCode::Busy, "{what}: {e:?}");
            let m = e.to_string();
            assert!(
                m.contains(noun) && m.contains(&job.id.to_string()),
                "{what}: {m}"
            );
        };

        // Queued.
        for (m, p) in [
            ("rename_capture", &rename),
            ("delete_capture", &delete),
            ("delete_capture", &delete_remote),
        ] {
            for who in [Actor::User, agent("claude")] {
                let e = d.execute_method(who, 0, m, p).await.unwrap_err();
                blocked(m, &e);
            }
        }
        // No dialog was shown for the refused deletes.
        assert!(r.confirmer.shown().is_empty());

        // Paused still blocks.
        queue.pause(&job.id).unwrap();
        let e = d
            .execute_method(Actor::User, 0, "rename_capture", &rename)
            .await
            .unwrap_err();
        blocked("paused rename", &e);

        // Cancelled: free again.
        queue.cancel(&job.id).unwrap();
        queue.cancel(&other.id).unwrap();
        d.execute_method(Actor::User, 0, "rename_capture", &rename)
            .await
            .unwrap();
        d.execute_method(
            Actor::User,
            0,
            "rename_capture",
            &json!({"capture_id": id, "title": "Being uploaded"}),
        )
        .await
        .unwrap();
    }
    assert!(r.trasher.trashed().is_empty());
}

/// A job that ended (failed or done) does not block: the handler fails the first job and
/// finishes the second.
#[tokio::test]
async fn failed_and_finished_upload_jobs_do_not_block() {
    use async_trait::async_trait;
    use cm_jobs::{JobContext, JobHandler};
    struct Outcome(std::sync::atomic::AtomicBool);
    #[async_trait]
    impl JobHandler for Outcome {
        async fn run(&self, _ctx: JobContext) -> Result<Value, cm_core::CmError> {
            if self.0.swap(true, std::sync::atomic::Ordering::SeqCst) {
                Ok(json!({}))
            } else {
                Err(cm_core::CmError::invalid_input(
                    "the destination is unusable",
                ))
            }
        }
    }
    let r = rig(Decision::Approve);
    let d = &r.daemon;
    let s = shot(d, "Done and failed").await;
    let id = s["capture_id"].as_str().unwrap().to_owned();
    d.register_job_handler(JOB_UPLOAD, Arc::new(Outcome(Default::default())))
        .unwrap();
    d.start_jobs().unwrap();
    let queue = d.job_queue().unwrap();
    for _ in 0..2 {
        let job = queue
            .enqueue(NewJob::new(JOB_UPLOAD, json!({"capture_id": id})))
            .unwrap();
        for _ in 0..200 {
            let state = queue.get(&job.id).unwrap().state;
            if matches!(state.as_str(), "done" | "failed") {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        let state = queue.get(&job.id).unwrap().state;
        assert!(matches!(state.as_str(), "done" | "failed"), "{state:?}");
        d.execute_method(
            Actor::User,
            0,
            "rename_capture",
            &json!({"capture_id": id, "title": "Still free"}),
        )
        .await
        .unwrap();
    }
}

// ---- delete_capture ---------------------------------------------------------------------------

/// Total size in bytes of every file under `dir`.
fn dir_bytes(dir: &std::path::Path) -> u64 {
    std::fs::read_dir(dir)
        .unwrap()
        .map(|e| {
            let p = e.unwrap().path();
            if p.is_dir() {
                dir_bytes(&p)
            } else {
                std::fs::metadata(&p).unwrap().len()
            }
        })
        .sum()
}

#[tokio::test]
async fn delete_asks_with_title_and_size_then_trashes_and_unindexes() {
    let r = rig(Decision::Approve);
    let d = &r.daemon;
    let s = shot(d, "Quarterly numbers").await;
    let id = s["capture_id"].as_str().unwrap().to_owned();
    let dir = PathBuf::from(s["path"].as_str().unwrap())
        .parent()
        .unwrap()
        .to_owned();
    let mut sub = d.subscribe();
    // The size the dialog shows is the bundle's real size (it varies by a byte between runs).
    let expected_bytes = dir_bytes(&dir);
    let out = d
        .execute_method(
            agent("claude"),
            0,
            "delete_capture",
            &json!({"capture_id": id, "request_id": "del-1"}),
        )
        .await
        .unwrap();
    assert_eq!(out["moved_to_trash"], true);
    assert_eq!(out["deleted_remote"], false);
    // The dialog named the capture, its size and where it goes.
    let shown = r.confirmer.shown();
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].tool, "delete_capture");
    // P3: the typed plan next to the English dialog text.
    assert_eq!(shown[0].kind, "delete_capture");
    assert_eq!(shown[0].params["title"], "Quarterly numbers");
    assert_eq!(shown[0].params["id"], json!(id));
    assert_eq!(shown[0].params["remote_copies"], 0);
    assert_eq!(shown[0].params["size_bytes"], expected_bytes);
    assert!(
        shown[0].summary.contains("Quarterly numbers"),
        "{}",
        shown[0].summary
    );
    assert_eq!(shown[0].details["capture"], "Quarterly numbers");
    assert_eq!(shown[0].details["id"], json!(id));
    assert_eq!(shown[0].details["result"], "Moved to the Trash");
    assert!(
        shown[0].details["size"].as_str().unwrap().ends_with("KB"),
        "{}",
        shown[0].details
    );
    // Folder moved (not deleted by the daemon), row gone, event published.
    assert!(!dir.exists());
    assert_eq!(r.trasher.trashed(), vec![dir]);
    let list = d
        .execute_method(Actor::User, 0, "list_library", &json!({}))
        .await
        .unwrap();
    assert_eq!(list["captures"].as_array().unwrap().len(), 0);
    let mut names = Vec::new();
    while let Ok(ev) = sub.try_recv() {
        names.push((ev.name.clone(), ev.data.clone()));
    }
    let del = names
        .iter()
        .find(|(n, _)| n == "capture.deleted")
        .expect("capture.deleted");
    assert_eq!(del.1["capture_id"], json!(id));
    assert_eq!(del.1["by"], "agent:claude");
    // Idempotent by request id: no second dialog, same answer.
    let again = d
        .execute_method(
            agent("claude"),
            0,
            "delete_capture",
            &json!({"capture_id": id, "request_id": "del-1"}),
        )
        .await
        .unwrap();
    assert_eq!(again, out);
    assert_eq!(r.confirmer.shown().len(), 1);
    // Gone for real now.
    let e = d
        .execute_method(Actor::User, 0, "delete_capture", &json!({"capture_id": id}))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::NotFound);
}

#[tokio::test]
async fn delete_declined_by_the_user_keeps_everything() {
    let r = rig(Decision::Decline);
    let d = &r.daemon;
    let s = shot(d, "Keep me").await;
    let id = s["capture_id"].as_str().unwrap().to_owned();
    let e = d
        .execute_method(
            agent("claude"),
            0,
            "delete_capture",
            &json!({"capture_id": id}),
        )
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::UserDeclined);
    assert!(r.trasher.trashed().is_empty());
    assert!(Path::new(s["path"].as_str().unwrap()).exists());
    assert_eq!(r.confirmer.shown().len(), 1);
}

#[tokio::test]
async fn delete_by_the_in_process_user_skips_the_dialog() {
    let r = rig(Decision::Decline);
    let d = &r.daemon;
    let id = shot(d, "Mine").await["capture_id"]
        .as_str()
        .unwrap()
        .to_owned();
    d.execute_method(Actor::User, 0, "delete_capture", &json!({"capture_id": id}))
        .await
        .unwrap();
    assert!(r.confirmer.shown().is_empty());
    assert_eq!(r.trasher.trashed().len(), 1);
}

/// `also_delete_remote` on a capture that was never uploaded or published: the dialog says that
/// nothing is deleted remotely, and the capture goes to the Trash as usual. (The remote engine
/// itself is tested in `remote_delete.rs`.)
#[tokio::test]
async fn delete_with_remote_flag_and_no_remote_copy_just_trashes() {
    let r = rig(Decision::Approve);
    let d = &r.daemon;
    let id = shot(d, "Remote").await["capture_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let out = d
        .execute_method(
            agent("claude"),
            0,
            "delete_capture",
            &json!({"capture_id": id, "also_delete_remote": true}),
        )
        .await
        .unwrap();
    assert_eq!(out["moved_to_trash"], true);
    assert_eq!(out["deleted_remote"], false);
    let shown = r.confirmer.shown();
    assert_eq!(shown.len(), 1);
    assert!(
        shown[0].details["remote"]
            .as_str()
            .unwrap()
            .contains("No remote copy"),
        "{}",
        shown[0].details
    );
    assert_eq!(r.trasher.trashed().len(), 1);
    // `false` is the default and fine, and asks no remote question.
    let id = shot(d, "Local only").await["capture_id"]
        .as_str()
        .unwrap()
        .to_owned();
    d.execute_method(
        agent("claude"),
        0,
        "delete_capture",
        &json!({"capture_id": id, "also_delete_remote": false}),
    )
    .await
    .unwrap();
    assert!(r.confirmer.shown()[1].details.get("remote").is_none());
}

#[tokio::test]
async fn a_failing_trash_leaves_the_capture_indexed() {
    let r = rig(Decision::Approve);
    let d = &r.daemon;
    let id = shot(d, "Stuck").await["capture_id"]
        .as_str()
        .unwrap()
        .to_owned();
    r.trasher.fail_from_now_on();
    let e = d
        .execute_method(Actor::User, 0, "delete_capture", &json!({"capture_id": id}))
        .await
        .unwrap_err();
    assert_eq!(code(&e), ErrorCode::Internal);
    let list = d
        .execute_method(Actor::User, 0, "list_library", &json!({}))
        .await
        .unwrap();
    assert_eq!(list["captures"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn delete_over_the_socket_needs_the_gate() {
    let r = rig(Decision::Approve);
    let id = shot(&r.daemon, "Socket").await["capture_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut c = Client::connect(&r.daemon, "x");
    c.hello("claude-desktop").await;
    let resp = c.call("delete_capture", json!({"capture_id": id})).await;
    assert!(resp.get("error").is_none(), "{resp}");
    assert_eq!(r.confirmer.shown().len(), 1);
    // A headless daemon (no confirmer) never deletes for an agent.
    let (b, _dir) = builder(desktop());
    let d = b.trasher(FakeTrasher::new()).build();
    let id = shot(&d, "Headless").await["capture_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let mut c = Client::connect(&d, "y");
    c.hello("claude-desktop").await;
    let resp = c.call("delete_capture", json!({"capture_id": id})).await;
    assert_eq!(resp["error"]["data"]["data"]["reason"], "no_confirmer");
    assert_eq!(
        d.execute_method(Actor::User, 0, "list_library", &json!({}))
            .await
            .unwrap()["captures"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

// ---- rescan_library ---------------------------------------------------------------------------

#[tokio::test]
async fn rescan_reports_duplicates_and_removed_rows() {
    let r = rig(Decision::Decline);
    let d = &r.daemon;
    let a = shot(d, "Keep").await;
    let b = shot(d, "Vanish").await;
    let a_dir = PathBuf::from(a["path"].as_str().unwrap())
        .parent()
        .unwrap()
        .to_owned();
    let b_dir = PathBuf::from(b["path"].as_str().unwrap())
        .parent()
        .unwrap()
        .to_owned();
    // A hand-copied bundle has the same capture id; a deleted folder leaves a ghost row.
    copy_dir(&a_dir, &r.dir.path().join("Keep copy.cmshot"));
    std::fs::remove_dir_all(&b_dir).unwrap();
    let rep = d
        .execute_method(Actor::User, 0, "rescan_library", &json!({}))
        .await
        .unwrap();
    assert_eq!(rep["indexed"], 1);
    assert_eq!(rep["removed"], 1);
    assert_eq!(rep["duplicates"].as_array().unwrap().len(), 1);
    assert_eq!(rep["duplicates"][0]["id"], a["capture_id"]);
    assert_eq!(rep["duplicate_ids"].as_array().unwrap().len(), 1);
    // It is internal and mutating: read-only clients are refused, params are checked.
    d.policies().set("viewer", ClientPolicy::ReadOnly);
    assert!(d
        .execute_method(agent("viewer"), 0, "rescan_library", &json!({}))
        .await
        .is_err());
    assert!(d
        .execute_method(Actor::User, 0, "rescan_library", &json!({"x": 1}))
        .await
        .is_err());
}

// ---- annotate return_image ---------------------------------------------------------------------

#[tokio::test]
async fn annotate_can_skip_the_image() {
    let r = rig(Decision::Decline);
    let d = &r.daemon;
    let id = shot(d, "Img").await["capture_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let ops: Value = serde_json::from_str(RECT).unwrap();
    let with = d
        .execute_method(
            Actor::User,
            0,
            "annotate",
            &json!({"capture_id": id, "ops": ops}),
        )
        .await
        .unwrap();
    assert!(with["image"]["data"].as_str().unwrap().len() > 100);
    let without = d
        .execute_method(
            Actor::User,
            0,
            "annotate",
            &json!({"capture_id": id, "ops": ops, "replace": true, "return_image": false}),
        )
        .await
        .unwrap();
    assert!(without["image"].is_null(), "{without}");
    assert_eq!(without["revision"], 2);
    // The render on disk is still current: get_capture serves it.
    let got = d
        .execute_method(Actor::User, 0, "get_capture", &json!({"capture_id": id}))
        .await
        .unwrap();
    assert!(got["image"]["data"].as_str().is_some());
}

// ---- poster outside the library lock ----------------------------------------------------------

#[tokio::test]
async fn posters_are_written_and_counted_in_the_index_size() {
    let r = rig(Decision::Decline);
    let d = &r.daemon;
    let s = shot(d, "Poster").await;
    let id = s["capture_id"].as_str().unwrap().to_owned();
    let dir = PathBuf::from(s["path"].as_str().unwrap())
        .parent()
        .unwrap()
        .to_owned();
    let poster = dir.join("thumbs").join("poster.jpg");
    assert!(poster.is_file());
    let size = |d: &Daemon| {
        let d = d.clone();
        let id = id.clone();
        async move {
            let l = d
                .execute_method(Actor::User, 0, "list_library", &json!({}))
                .await
                .unwrap();
            l["captures"]
                .as_array()
                .unwrap()
                .iter()
                .find(|c| c["id"] == json!(id))
                .unwrap()["size_bytes"]
                .as_u64()
                .unwrap()
        }
    };
    let on_disk = |dir: &Path| -> u64 {
        fn walk(p: &Path) -> u64 {
            std::fs::read_dir(p)
                .unwrap()
                .flatten()
                .map(|e| {
                    let m = e.metadata().unwrap();
                    if m.is_dir() {
                        walk(&e.path())
                    } else {
                        m.len()
                    }
                })
                .sum()
        }
        walk(dir)
    };
    assert_eq!(size(d).await, on_disk(&dir));
    d.execute_method(
        Actor::User,
        0,
        "annotate",
        &json!({"capture_id": id, "ops": serde_json::from_str::<Value>(RECT).unwrap(), "return_image": false}),
    )
    .await
    .unwrap();
    assert_eq!(
        size(d).await,
        on_disk(&dir),
        "the row counts the new render and poster"
    );
}
