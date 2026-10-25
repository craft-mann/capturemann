//! Which event shows which toast (STO-3 notifications), with the texts and the dedupe. "Tested
//! with fakes": events are plain JSON, there is no daemon and no window.

#![allow(clippy::unwrap_used)]

use serde_json::json;

use super::*;

fn dests(id: &str) -> Option<DestInfo> {
    match id {
        "drive" => Some(DestInfo {
            name: "Google Drive".into(),
            local: false,
        }),
        "nas" => Some(DestInfo {
            name: "NAS".into(),
            local: true,
        }),
        _ => None,
    }
}

fn queued(job: &str, dest: &str) -> Value {
    json!({ "job_id": job, "capture_id": "rec_1", "title": "Lesson",
            "destination_id": dest, "destination": "ignored name", "what": "export",
            "trigger": "stop", "export_job_id": null })
}

fn done(job: &str, dest: &str, link: Value) -> Value {
    json!({ "job_id": job, "type": "upload", "state": "done",
            "result": { "destination_id": dest, "destination": "name in result", "capture_id": "rec_1",
                        "files": [{ "remote_id": "/Volumes/nas/CaptureMann/Lesson.mp4", "bytes": 10 }],
                        "bytes": 10, "share_link": link, "notes": [] } })
}

fn failed(job: &str, reason: &str) -> Value {
    json!({ "job_id": job, "type": "upload", "state": "failed",
            "error": { "code": "job_failed", "message": format!("Job {job} failed"),
                       "data": { "job_id": job, "reason": reason } } })
}

fn paused(job: &str) -> Value {
    json!({ "job_id": job, "type": "upload", "state": "paused", "pause_reason": "auth_expired" })
}

/// Completion of a cloud upload: "Copy link" and "Open" carry the link.
#[test]
fn a_finished_cloud_upload_offers_copy_link_and_open() {
    let mut t = UploadTracker::default();
    assert!(t
        .on_event("upload.queued", &queued("j1", "drive"), &dests)
        .is_empty());
    let e = t.on_event(
        "job.done",
        &done("j1", "drive", json!("https://drive.example/s/1")),
        &dests,
    );
    let [Effect::Done { share_link, .. }] = e.as_slice() else {
        panic!("{e:?}")
    };
    assert_eq!(share_link.as_deref(), Some("https://drive.example/s/1"));
    let (key, toast) = toast_for(&e[0]).unwrap();
    assert_eq!(key, "upload:j1");
    assert_eq!(toast.kind, ToastKind::Success);
    assert_eq!(toast.title, "Uploaded to Google Drive");
    assert_eq!(toast.body.as_deref(), Some("Lesson"));
    let labels: Vec<_> = toast.buttons.iter().map(|b| b.label.as_str()).collect();
    assert_eq!(labels, ["Copy link", "Open"]);
    assert_eq!(
        toast.buttons[0].action,
        ToastAction::CopyLink("https://drive.example/s/1".into())
    );
    assert_eq!(
        toast.buttons[1].action,
        ToastAction::OpenLink("https://drive.example/s/1".into())
    );
    let mut q = crate::toast::ToastQueue::default();
    q.push(toast, std::time::Instant::now());
    assert!(
        q.toasts()[0].sticky(),
        "a toast with buttons waits for the user"
    );
}

/// A destination whose link mode is "none" has a result but nothing to copy or open.
#[test]
fn an_upload_without_a_link_has_no_link_buttons() {
    let mut t = UploadTracker::default();
    t.on_event("upload.queued", &queued("j1", "drive"), &dests);
    let e = t.on_event("job.done", &done("j1", "drive", Value::Null), &dests);
    let (_, toast) = toast_for(&e[0]).unwrap();
    assert!(toast.buttons.is_empty());
    assert_eq!(toast.title, "Uploaded to Google Drive");
}

/// Local destinations: "Open" shows the file, there is no link to copy.
#[test]
fn a_finished_local_copy_opens_the_file() {
    let mut t = UploadTracker::default();
    t.on_event("upload.queued", &queued("j2", "nas"), &dests);
    let e = t.on_event("job.done", &done("j2", "nas", Value::Null), &dests);
    let (_, toast) = toast_for(&e[0]).unwrap();
    assert_eq!(toast.title, "Copied to NAS");
    assert_eq!(toast.buttons.len(), 1);
    assert_eq!(toast.buttons[0].label, "Open");
    assert_eq!(
        toast.buttons[0].action,
        ToastAction::RevealPath("/Volumes/nas/CaptureMann/Lesson.mp4".into())
    );
}

/// An upload started from the CLI or by an agent (never queued through `upload.queued`) still
/// ends with a toast, named from the result.
#[test]
fn a_finished_upload_the_app_never_saw_queued_still_gets_its_toast() {
    let mut t = UploadTracker::default();
    let none = |_: &str| None;
    let e = t.on_event(
        "job.done",
        &done("j9", "gone", json!("https://x.example/l")),
        &none,
    );
    let (_, toast) = toast_for(&e[0]).unwrap();
    assert_eq!(toast.title, "Uploaded to name in result");
    assert_eq!(toast.buttons.len(), 2);
    // The same event twice gives the same key, so the toast is replaced in place.
    let again = t.on_event(
        "job.done",
        &done("j9", "gone", json!("https://x.example/l")),
        &none,
    );
    assert_eq!(toast_for(&again[0]).unwrap().0, "upload:j9");
}

#[test]
fn a_note_from_the_provider_is_part_of_the_toast() {
    let mut t = UploadTracker::default();
    t.on_event("upload.queued", &queued("j1", "drive"), &dests);
    let mut d = done("j1", "drive", json!("https://l"));
    d["result"]["notes"] =
        json!(["Your organization does not allow public links; this link is for people in it."]);
    let e = t.on_event("job.done", &d, &dests);
    let (_, toast) = toast_for(&e[0]).unwrap();
    let body = toast.body.unwrap();
    assert!(body.starts_with("Lesson"), "{body}");
    assert!(body.contains("does not allow public links"), "{body}");
}

/// Failure: the readable reason and "Retry" with the job id.
#[test]
fn a_failed_upload_shows_the_reason_and_retry() {
    let mut t = UploadTracker::default();
    t.on_event("upload.queued", &queued("j1", "drive"), &dests);
    let e = t.on_event(
        "job.failed",
        &failed("j1", "Your Google Drive is full."),
        &dests,
    );
    let (key, toast) = toast_for(&e[0]).unwrap();
    assert_eq!(key, "upload:j1");
    assert_eq!(toast.kind, ToastKind::Error);
    assert_eq!(toast.title, "Upload to Google Drive failed");
    assert_eq!(
        toast.body.as_deref(),
        Some("Lesson \u{b7} Your Google Drive is full.")
    );
    assert_eq!(toast.buttons.len(), 1);
    assert_eq!(toast.buttons[0].label, "Retry");
    assert_eq!(toast.buttons[0].action, ToastAction::RetryJob("j1".into()));
    assert_eq!(
        t.running_of("rec_1").len(),
        0,
        "a failed job is not running"
    );
}

/// After "Retry" the job runs again: its next failure is a new toast, in the same place.
#[test]
fn a_retried_job_that_fails_again_replaces_its_toast() {
    let mut t = UploadTracker::default();
    t.on_event("upload.queued", &queued("j1", "drive"), &dests);
    let first = t.on_event("job.failed", &failed("j1", "boom"), &dests);
    t.on_event(
        "job.progress",
        &json!({ "job_id": "j1", "type": "upload", "progress": 0.3 }),
        &dests,
    );
    assert_eq!(t.running_of("rec_1").len(), 1);
    // The tracker saw it run again; a second failure has to be allowed through.
    let again = t.on_event("job.failed", &failed("j1", "boom 2"), &dests);
    assert_eq!(
        toast_for(&first[0]).unwrap().0,
        toast_for(&again[0]).unwrap().0
    );
}

/// `auth_expired`: one reconnect toast per destination, however many uploads wait.
#[test]
fn a_paused_upload_asks_to_reconnect_once_per_destination() {
    let mut t = UploadTracker::default();
    t.on_event("upload.queued", &queued("j1", "drive"), &dests);
    t.on_event("upload.queued", &queued("j2", "drive"), &dests);
    let e = t.on_event("job.paused", &paused("j1"), &dests);
    let (key, toast) = toast_for(&e[0]).unwrap();
    assert_eq!(key, "reconnect:drive");
    assert_eq!(toast.kind, ToastKind::Warning);
    assert_eq!(toast.title, "Reconnect needed for Google Drive");
    assert_eq!(toast.buttons[0].action, ToastAction::OpenDestinations);
    assert!(
        t.on_event("job.paused", &paused("j2"), &dests).is_empty(),
        "the second waiting upload adds no second toast"
    );
    assert_eq!(t.running_of("rec_1").len(), 2, "both are waiting");
}

/// Once the account works again and an upload runs, the next expiry is news again.
#[test]
fn the_reconnect_toast_comes_back_after_the_account_worked_again() {
    let mut t = UploadTracker::default();
    t.on_event("upload.queued", &queued("j1", "drive"), &dests);
    assert_eq!(t.on_event("job.paused", &paused("j1"), &dests).len(), 1);
    // Reconnected: the job runs, reports progress, finishes.
    t.on_event(
        "job.progress",
        &json!({ "job_id": "j1", "type": "upload", "progress": 0.5 }),
        &dests,
    );
    t.on_event("job.done", &done("j1", "drive", Value::Null), &dests);
    t.on_event("upload.queued", &queued("j3", "drive"), &dests);
    assert_eq!(t.on_event("job.paused", &paused("j3"), &dests).len(), 1);

    // The sign-in flow can also say so directly.
    assert!(t.on_event("job.paused", &paused("j3"), &dests).is_empty());
    t.reconnected("drive");
    assert_eq!(t.on_event("job.paused", &paused("j3"), &dests).len(), 1);
}

/// A failure whose code is `auth_expired` is the same as a pause: reconnect, not retry.
#[test]
fn an_expired_sign_in_that_surfaces_as_a_failure_asks_to_reconnect() {
    let mut t = UploadTracker::default();
    t.on_event("upload.queued", &queued("j1", "drive"), &dests);
    let e = t.on_event(
        "job.failed",
        &json!({ "job_id": "j1", "type": "upload", "state": "failed",
                 "error": { "code": "auth_expired", "message": "Reconnect needed" } }),
        &dests,
    );
    assert!(matches!(&e[..], [Effect::Reconnect { .. }]), "{e:?}");
}

/// Pauses that need nothing from the user, other job types, and unknown names are ignored.
#[test]
fn other_events_show_nothing() {
    let mut t = UploadTracker::default();
    t.on_event("upload.queued", &queued("j1", "drive"), &dests);
    assert!(t
        .on_event(
            "job.paused",
            &json!({ "job_id": "j1", "type": "upload", "pause_reason": "user" }),
            &dests
        )
        .is_empty());
    assert!(t
        .on_event(
            "job.done",
            &json!({ "job_id": "x", "type": "export", "result": {} }),
            &dests
        )
        .is_empty());
    assert!(t
        .on_event(
            "job.failed",
            &json!({ "job_id": "x", "type": "transcribe", "error": {} }),
            &dests
        )
        .is_empty());
    assert!(t.on_event("capture.created", &json!({}), &dests).is_empty());
}

/// A job started elsewhere is looked up once; the answer lets the same event be applied.
#[test]
fn an_unknown_job_is_looked_up_once_then_the_event_is_applied() {
    let mut t = UploadTracker::default();
    let e = t.on_event("job.failed", &failed("j7", "no space"), &dests);
    assert_eq!(
        e,
        vec![Effect::Lookup {
            job_id: "j7".into()
        }]
    );
    assert!(toast_for(&e[0]).is_none());
    // A failed lookup must not loop.
    assert!(t
        .on_event("job.failed", &failed("j7", "no space"), &dests)
        .is_empty());

    let mut t = UploadTracker::default();
    let e = t.on_event("job.paused", &paused("j8"), &dests);
    assert_eq!(
        e,
        vec![Effect::Lookup {
            job_id: "j8".into()
        }]
    );
    t.learn(
        &json!({ "job_id": "j8", "type": "upload", "state": "paused",
                     "pause_reason": "auth_expired", "progress": 0.2,
                     "inputs": { "destination_id": "drive", "capture_id": "rec_5" } }),
    );
    let e = t.on_event("job.paused", &paused("j8"), &dests);
    assert!(
        matches!(&e[..], [Effect::Reconnect { destination, .. }] if destination == "Google Drive"),
        "{e:?}"
    );
    // Jobs of other types are not learned.
    t.learn(&json!({ "job_id": "e1", "type": "export", "state": "running" }));
    assert!(t.running_of("rec_5").len() == 1);
}

#[test]
fn running_uploads_show_progress_for_the_details_page() {
    let mut t = UploadTracker::default();
    t.on_event("upload.queued", &queued("j1", "drive"), &dests);
    t.on_event(
        "job.progress",
        &json!({ "job_id": "j1", "type": "upload", "progress": 0.426 }),
        &dests,
    );
    let r = t.running_of("rec_1");
    assert_eq!(r.len(), 1);
    assert_eq!(
        running_text(r[0], "Google Drive"),
        "Uploading to Google Drive: 42%"
    );
    assert!(t.running_of("rec_2").is_empty());
    t.on_event("job.paused", &paused("j1"), &dests);
    let r = t.running_of("rec_1");
    assert_eq!(
        running_text(r[0], "Google Drive"),
        "Waiting for you to reconnect Google Drive"
    );
    t.on_event("job.done", &done("j1", "drive", Value::Null), &dests);
    assert!(t.running_of("rec_1").is_empty());
}

#[test]
fn finished_jobs_are_pruned_but_running_ones_stay() {
    let mut t = UploadTracker::default();
    for n in 0..5 {
        let id = format!("d{n}");
        t.on_event("upload.queued", &queued(&id, "drive"), &dests);
        t.on_event("job.done", &done(&id, "drive", Value::Null), &dests);
    }
    t.on_event("upload.queued", &queued("run", "drive"), &dests);
    t.prune(2);
    assert_eq!(t.jobs.len(), 3);
    assert!(t.jobs.contains_key("run"));
}

/// The decision logic never puts a link in the log or the title: only the buttons carry it.
#[test]
fn toast_texts_do_not_repeat_the_link() {
    let toast = uploaded_toast(
        Some("Lesson"),
        "Dropbox",
        None,
        Some("https://dbx.example/s/abc"),
        None,
    );
    assert!(!toast.title.contains("https"));
    assert!(!toast.body.unwrap_or_default().contains("https"));
}

/// The tray's Attention rule: on while an upload waits for a reconnect, off once the user
/// dismisses the toast, the upload runs again (retry), it finishes, or the account signs in.
#[test]
fn the_tray_attention_rule() {
    let progress = |job: &str| json!({ "job_id": job, "type": "upload", "progress": 0.3 });
    let mut t = UploadTracker::default();
    assert!(!t.needs_attention());
    t.on_event("upload.queued", &queued("j1", "drive"), &dests);
    // A plain failure is not an attention state: it has its own toast with Retry.
    t.on_event("job.failed", &failed("j1", "disk full"), &dests);
    assert!(!t.needs_attention());

    // Paused as auth_expired: on.
    t.on_event("upload.queued", &queued("j2", "drive"), &dests);
    t.on_event("job.paused", &paused("j2"), &dests);
    assert!(t.needs_attention());
    // A second job of the same destination keeps it on without a second toast.
    t.on_event("upload.queued", &queued("j3", "drive"), &dests);
    t.on_event("job.paused", &paused("j3"), &dests);
    assert!(t.needs_attention());

    // The user dismissed the toast: off.
    t.acknowledged("drive");
    assert!(!t.needs_attention());

    // Retried: it runs again, so progress turns it off and a later expiry turns it on again.
    t.on_event("job.paused", &paused("j2"), &dests);
    assert!(t.needs_attention());
    t.on_event("job.progress", &progress("j2"), &dests);
    assert!(!t.needs_attention());

    // Failed as auth_expired: on; signing in again: off.
    t.on_event(
        "job.failed",
        &json!({ "job_id": "j3", "type": "upload", "state": "failed",
                 "error": { "code": "auth_expired", "message": "Reconnect needed" } }),
        &dests,
    );
    assert!(t.needs_attention());
    t.reconnected("drive");
    assert!(!t.needs_attention());

    // A finished upload to the destination clears it too.
    t.on_event("job.paused", &paused("j2"), &dests);
    assert!(t.needs_attention());
    t.on_event("job.done", &done("j2", "drive", json!(null)), &dests);
    assert!(!t.needs_attention());
}

/// I18N P2: the toasts are built when asked for and keep their values in every language.
#[test]
fn toasts_keep_their_values_in_every_language() {
    use cm_i18n::{with_language, Lang};
    for lang in [Lang::Vi, Lang::EnXa] {
        with_language(lang, || {
            let t = uploaded_toast(Some("Take"), "Drive", None, Some("https://x/y"), None);
            assert!(
                t.title.contains("Drive") && !t.title.contains('{'),
                "{lang:?}"
            );
            let f = upload_failed_toast("j1", Some("Take"), "Drive", "no");
            assert!(
                f.title.contains("Drive") && !f.title.contains('{'),
                "{lang:?}"
            );
            let r = reconnect_toast("Drive");
            assert!(
                r.title.contains("Drive") && !r.title.contains('{'),
                "{lang:?}"
            );
        });
    }
    assert_eq!(
        uploaded_toast(None, "Drive", None, None, None).title,
        "Uploaded to Drive"
    );
    assert_ne!(
        with_language(Lang::EnXa, || reconnect_toast("Drive").title),
        reconnect_toast("Drive").title
    );
}
