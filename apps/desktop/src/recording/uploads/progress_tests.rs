//! The running-upload toast (R17): which event shows which progress toast. "Tested with fakes":
//! events are plain JSON with an explicit clock, there is no daemon and no window.

#![allow(clippy::unwrap_used)]

use std::time::{Duration, Instant};

use serde_json::json;

use super::*;
use crate::recording::upload_progress::Eta;

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

fn prog(job: &str, p: f64) -> Value {
    json!({ "job_id": job, "type": "upload", "progress": p, "message": null })
}

fn done(job: &str) -> Value {
    json!({ "job_id": job, "type": "upload", "state": "done",
            "result": { "destination_id": "drive", "capture_id": "rec_1", "files": [],
                        "share_link": null, "notes": [] } })
}

fn failed(job: &str) -> Value {
    json!({ "job_id": job, "type": "upload", "state": "failed",
            "error": { "code": "job_failed", "message": "x",
                       "data": { "job_id": job, "reason": "boom" } } })
}

fn ms(t0: Instant, n: u64) -> Instant {
    t0 + Duration::from_millis(n)
}

/// Title, destination, a bar, the percentage and Cancel; the first event has no ETA yet.
#[test]
fn a_running_upload_has_a_progress_toast_with_cancel() {
    let t0 = Instant::now();
    let mut t = UploadTracker::default();
    t.on_event("upload.queued", &queued("j1", "drive"), &dests);
    let e = t.on_event_at("job.progress", &prog("j1", 0.38), &dests, t0);
    let [Effect::Progress { .. }] = e.as_slice() else {
        panic!("{e:?}")
    };
    let (key, toast) = toast_for(&e[0]).unwrap();
    assert_eq!(
        key, "upload:j1",
        "the key Done and Failed use, so they replace it"
    );
    assert_eq!(toast.kind, ToastKind::Info);
    assert_eq!(toast.title, "Uploading Lesson to Google Drive");
    assert_eq!(toast.body.as_deref(), Some("38%"));
    assert_eq!(toast.progress, Some(38));
    assert_eq!(toast.buttons.len(), 1);
    assert_eq!(toast.buttons[0].label, "Cancel");
    assert_eq!(toast.buttons[0].action, ToastAction::CancelJob("j1".into()));
    let mut q = crate::toast::ToastQueue::default();
    q.push(toast, t0);
    assert_eq!(q.next_expiry(t0), None, "no auto-dismiss while running");
}

/// An ETA appears after the warm-up and is worded.
#[test]
fn the_eta_shows_after_the_warm_up() {
    let t0 = Instant::now();
    let mut t = UploadTracker::default();
    t.on_event("upload.queued", &queued("j1", "drive"), &dests);
    let mut last = None;
    for s in 0..=6u64 {
        let e = t.on_event_at(
            "job.progress",
            &prog("j1", s as f64 * 0.1),
            &dests,
            ms(t0, s * 1000),
        );
        if let Some(Effect::Progress { view, .. }) = e.first() {
            last = Some(*view);
        }
    }
    // 60 % at 6 s, 10 %/s: 4 s left, "less than a minute".
    let view = last.unwrap();
    assert_eq!(view.percent, 60);
    assert_eq!(view.eta, Eta::LessThanMinute);
    let (_, toast) = toast_for(&Effect::Progress {
        job_id: "j1".into(),
        title: None,
        destination: "Google Drive".into(),
        view,
    })
    .unwrap();
    assert_eq!(toast.title, "Uploading to Google Drive");
    assert_eq!(
        toast.body.as_deref(),
        Some("60% \u{00B7} less than a minute left")
    );
}

/// At most one repaint per 500 ms, and a closed toast does not come back.
#[test]
fn progress_does_not_flood_and_a_closed_toast_stays_closed() {
    let t0 = Instant::now();
    let mut t = UploadTracker::default();
    t.on_event("upload.queued", &queued("j1", "drive"), &dests);
    let mut painted = 0;
    for i in 0..100u64 {
        let e = t.on_event_at(
            "job.progress",
            &prog("j1", i as f64 / 200.0),
            &dests,
            ms(t0, i * 10),
        );
        painted += e.len();
    }
    assert_eq!(
        painted, 2,
        "100 events in 1 s: the first and the one at 500 ms"
    );
    t.progress_toast_closed("j1");
    assert!(t
        .on_event_at("job.progress", &prog("j1", 0.9), &dests, ms(t0, 5000))
        .is_empty());
}

/// Two uploads at once: each has its own toast key and its own clock.
#[test]
fn two_uploads_get_two_toasts() {
    let t0 = Instant::now();
    let mut t = UploadTracker::default();
    t.on_event("upload.queued", &queued("a", "drive"), &dests);
    t.on_event("upload.queued", &queued("b", "nas"), &dests);
    let a = t.on_event_at("job.progress", &prog("a", 0.1), &dests, t0);
    let b = t.on_event_at("job.progress", &prog("b", 0.2), &dests, ms(t0, 10));
    assert_eq!(toast_for(&a[0]).unwrap().0, "upload:a");
    assert_eq!(toast_for(&b[0]).unwrap().0, "upload:b");
    assert_eq!(toast_for(&b[0]).unwrap().1.title, "Uploading Lesson to NAS");
}

/// Ten quiet seconds: "Waiting..." once; the job ending replaces the toast under the same key.
#[test]
fn a_stalled_upload_says_waiting_and_the_end_replaces_the_toast() {
    let t0 = Instant::now();
    let mut t = UploadTracker::default();
    t.on_event("upload.queued", &queued("j1", "drive"), &dests);
    t.on_event_at("job.progress", &prog("j1", 0.4), &dests, t0);
    assert!(t.watching_progress());
    assert!(t.stalled(ms(t0, 9_000), &dests).is_empty());
    let e = t.stalled(ms(t0, 10_000), &dests);
    let (key, toast) = toast_for(&e[0]).unwrap();
    assert_eq!(key, "upload:j1");
    assert_eq!(toast.body.as_deref(), Some("40% \u{00B7} Waiting\u{2026}"));
    assert!(t.stalled(ms(t0, 20_000), &dests).is_empty(), "only once");
    let finished = t.on_event_at("job.done", &done("j1"), &dests, ms(t0, 21_000));
    assert_eq!(toast_for(&finished[0]).unwrap().0, "upload:j1");
    assert!(!t.watching_progress());
    // A late progress event after Done paints nothing.
    assert!(t
        .on_event_at("job.progress", &prog("j1", 0.99), &dests, ms(t0, 22_000))
        .is_empty());
}

/// Failure replaces the toast; Retry starts a new estimate with a new running toast.
#[test]
fn retry_after_failure_starts_a_fresh_progress_toast() {
    let t0 = Instant::now();
    let mut t = UploadTracker::default();
    t.on_event("upload.queued", &queued("j1", "drive"), &dests);
    for s in 0..4u64 {
        t.on_event_at(
            "job.progress",
            &prog("j1", s as f64 * 0.1),
            &dests,
            ms(t0, s * 1000),
        );
    }
    let lost = t.on_event_at("job.failed", &failed("j1"), &dests, ms(t0, 4000));
    assert_eq!(toast_for(&lost[0]).unwrap().1.kind, ToastKind::Error);
    let e = t.on_event_at("job.progress", &prog("j1", 0.05), &dests, ms(t0, 9000));
    let Some(Effect::Progress { view, .. }) = e.first() else {
        panic!("{e:?}")
    };
    assert_eq!(view.percent, 5);
    assert_eq!(view.eta, Eta::Unknown);
}

/// A cancelled upload (no daemon event) is closed locally and stops being watched.
#[test]
fn a_cancelled_upload_is_not_running_any_more() {
    let t0 = Instant::now();
    let mut t = UploadTracker::default();
    t.on_event("upload.queued", &queued("j1", "drive"), &dests);
    t.on_event_at("job.progress", &prog("j1", 0.4), &dests, t0);
    assert_eq!(t.running_of("rec_1").len(), 1);
    t.cancelled("j1");
    assert!(t.running_of("rec_1").is_empty());
    assert!(!t.watching_progress());
    assert!(t.stalled(ms(t0, 60_000), &dests).is_empty());
}

/// A long recording name is cut so the destination stays in the title.
#[test]
fn a_long_title_is_ellipsized() {
    let view = crate::recording::upload_progress::ProgressView {
        percent: 1,
        eta: Eta::Unknown,
    };
    let toast = running_toast(
        "j",
        Some("Recording 2026-10-23 08.40 about the whole quarterly planning"),
        "Google Drive",
        view,
    );
    assert_eq!(
        toast.title,
        "Uploading Recording 2026-10-23 08.40\u{2026} to Google Drive"
    );
    assert_eq!(ellipsize("short", 28), "short");
}
