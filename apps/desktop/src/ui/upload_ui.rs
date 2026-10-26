//! Window glue of the upload notifications (STO-3): turns the daemon's `upload.queued` and
//! `job.*` events into toasts ("Uploaded to <destination>" with Copy link and Open, "Upload to
//! <destination> failed" with Retry, "Reconnect needed for <destination>"), and runs the toast
//! buttons. The decisions live in `recording::uploads` (unit tested without a display).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use cm_daemon::events::Event;
use cm_i18n::{tr, tr_ctx};
use cm_storage::{DestinationStore, Kind};
use gpui::{App, Entity};
use serde_json::{json, Value};

use crate::backend::Backend;
use crate::clipboard::{SystemClipboard, TextClipboard};
use crate::recording::attention::AttentionTracker;
use crate::recording::files;
use crate::recording::uploads::{
    auto_copy, copy_link_enabled, running_text, toast_for, AutoCopy, DestInfo, Effect, Phase,
    UploadStatus, UploadTracker,
};
use crate::toast::{ToastKind, ToastSpec};
use crate::ui::app::{self, AppModel, Route};

static TRACKER: LazyLock<Mutex<UploadTracker>> = LazyLock::new(Mutex::default);
/// Failed publishes and an offline library, the tray's other Attention causes.
static ATTENTION: LazyLock<Mutex<AttentionTracker>> = LazyLock::new(Mutex::default);
/// The toast showing each key (`upload:<job>`, `reconnect:<destination>`), so a newer toast for the
/// same thing replaces the older one in place.
static TOASTS: LazyLock<Mutex<HashMap<String, u64>>> = LazyLock::new(Mutex::default);

/// A timer is running that turns quiet uploads into "Waiting...".
static TICKING: AtomicBool = AtomicBool::new(false);

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

fn store() -> DestinationStore {
    DestinationStore::new(crate::settings::config_dir())
}

/// A destination by id, from `destinations.json`.
fn dest_info(id: &str) -> Option<DestInfo> {
    store().get(id).ok().map(|d| DestInfo {
        name: d.name,
        local: d.kind == Kind::Local,
    })
}

/// The unfinished uploads of a recording with the destination's name, for the details page.
pub fn running_of(capture_id: &str) -> Vec<(UploadStatus, String)> {
    let jobs: Vec<UploadStatus> = lock(&TRACKER)
        .running_of(capture_id)
        .into_iter()
        .cloned()
        .collect();
    jobs.into_iter()
        .map(|j| {
            let name = j
                .destination_id
                .as_deref()
                .and_then(dest_info)
                .map(|d| d.name)
                .or_else(|| j.destination.clone())
                .unwrap_or_else(|| tr_ctx!("fallback name", "the destination"));
            (j, name)
        })
        .collect()
}

/// The text of one running upload line.
pub fn status_line(s: &UploadStatus, destination: &str) -> String {
    running_text(s, destination)
}

/// Whether the line is waiting for a reconnect (the details page marks it).
pub fn is_waiting(s: &UploadStatus) -> bool {
    s.phase == Phase::Paused
}

/// Applies one `upload.queued` or `job.*` event.
pub fn on_event(app: &Entity<AppModel>, ev: &Event, cx: &mut App) {
    if ev.name != "upload.queued" && !ev.name.starts_with("job.") {
        return;
    }
    apply(app, &ev.name, &ev.data, cx);
}

fn apply(app: &Entity<AppModel>, name: &str, data: &Value, cx: &mut App) {
    let effects = {
        let mut t = lock(&TRACKER);
        let e = t.on_event(name, data, &dest_info);
        if name == "upload.queued" {
            t.prune(20);
        }
        e
    };
    // STO-4.AC1: a finished upload with a share link puts it on the clipboard (setting
    // `storage.copy_link_when_done`, default on); the toast then says so.
    let copy_on = copy_link_enabled(&app.read(cx).backend.settings.snapshot());
    for mut effect in effects {
        if let AutoCopy::Failed(e) = auto_copy(&mut effect, copy_on, &SystemClipboard::default()) {
            log::warn!("could not copy the share link: {e}");
        }
        if let Effect::Lookup { job_id } = &effect {
            look_up(app, job_id, name, data, cx);
            continue;
        }
        if let Some((key, spec)) = toast_for(&effect) {
            upsert(app, &key, spec, cx);
        }
    }
    start_ticker(app, cx);
    // The details card shows running uploads: wake it for progress and results.
    if name.starts_with("job.") || name == "upload.queued" {
        app.update(cx, |_, cx| cx.notify());
    }
    // The tray's Attention state follows the tracker.
    crate::ui::recording::sync_tray(app, cx);
}

/// The tray's Attention state: an upload waits for the user to reconnect an account, a publish
/// failed or waits for its account, or the library folder is offline.
pub fn attention() -> bool {
    lock(&TRACKER).needs_attention() || lock(&ATTENTION).needs_attention()
}

/// Applies a `job.*`, `publish.done` or `library.*` event to the publish and library Attention
/// causes and refreshes the tray when the answer changed.
pub fn on_attention_event(app: &Entity<AppModel>, ev: &Event, cx: &mut App) {
    let changed = lock(&ATTENTION).on_event(&ev.name, &ev.data);
    if changed {
        crate::ui::recording::sync_tray(app, cx);
    }
}

/// The user opened the Published page or a publish sheet: failed publishes were seen.
pub fn publishes_seen(app: &Entity<AppModel>, cx: &mut App) {
    let changed = lock(&ATTENTION).acknowledge_publishes();
    if changed {
        crate::ui::recording::sync_tray(app, cx);
    }
}

/// A toast was dismissed (its x, or after its button ran): when it was a "Reconnect needed" toast
/// the user has seen it, so the tray stops asking for attention.
pub fn toast_dismissed(app: &Entity<AppModel>, toast_id: u64, cx: &mut App) {
    let dest = {
        let toasts = lock(&TOASTS);
        toasts
            .iter()
            .find(|(_, id)| **id == toast_id)
            .and_then(|(key, _)| key.strip_prefix("reconnect:").map(str::to_owned))
    };
    if let Some(dest) = dest {
        lock(&TRACKER).acknowledged(&dest);
        crate::ui::recording::sync_tray(app, cx);
    }
    // A closed running-upload toast does not come back while the job runs.
    let job = {
        let toasts = lock(&TOASTS);
        toasts
            .iter()
            .find(|(_, id)| **id == toast_id)
            .and_then(|(key, _)| key.strip_prefix("upload:").map(str::to_owned))
    };
    if let Some(job) = job {
        lock(&TRACKER).progress_toast_closed(&job);
    }
}

/// While an upload toast is on screen, once a second: an upload with no progress for 10 s
/// becomes "Waiting...". Stops by itself when nothing is left to watch.
fn start_ticker(app: &Entity<AppModel>, cx: &mut App) {
    if !lock(&TRACKER).watching_progress() || TICKING.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    cx.spawn(async move |cx| loop {
        cx.background_executor().timer(Duration::from_secs(1)).await;
        let keep = cx.update(|cx| {
            let effects = lock(&TRACKER).stalled(Instant::now(), &dest_info);
            for effect in effects {
                if let Some((key, spec)) = toast_for(&effect) {
                    upsert(&app, &key, spec, cx);
                }
            }
            let keep = lock(&TRACKER).watching_progress();
            if !keep {
                TICKING.store(false, Ordering::SeqCst);
            }
            keep
        });
        if !keep {
            break;
        }
    })
    .detach();
}

/// `upload` as the user, to the default destination: a capture (`capture_id`: what its destination
/// asks for) or a file made from it (`file`: the exported audio). The progress and the result
/// arrive as events; a refusal (no destination, nothing to send) is a toast.
pub fn upload(app: &Entity<AppModel>, capture_id: Option<&str>, file: Option<&str>, cx: &mut App) {
    let backend = app.read(cx).backend.clone();
    let Some(daemon) = backend.daemon.clone() else {
        return;
    };
    let params = upload_params(capture_id, file);
    let app2 = app.clone();
    cx.spawn(async move |cx| {
        let res = backend
            .rt
            .spawn(async move { Backend::call_coded(&daemon, "upload", params).await })
            .await;
        cx.update(|cx| {
            let why = match res {
                Ok(Ok(_)) => return,
                Ok(Err(e)) => crate::daemon_text::call_error_text(&e),
                Err(e) => e.to_string(),
            };
            error_toast(&app2, &tr!("The upload couldn\u{2019}t start"), &why, cx);
        });
    })
    .detach();
}

/// The `upload` request: a file wins over a capture (the daemon takes only one of them).
fn upload_params(capture_id: Option<&str>, file: Option<&str>) -> Value {
    match (file, capture_id) {
        (Some(f), _) => json!({ "file": f }),
        (None, Some(c)) => json!({ "capture_id": c }),
        (None, None) => json!({}),
    }
}

/// Toast "Cancel" on a running upload: `cancel_job` as the user. The daemon sends no event for a
/// cancelled job, so the answer closes it here and a short "Upload cancelled" toast says so.
pub fn cancel(app: &Entity<AppModel>, job_id: &str, cx: &mut App) {
    let backend = app.read(cx).backend.clone();
    let Some(daemon) = backend.daemon.clone() else {
        return;
    };
    lock(&TRACKER).progress_toast_closed(job_id);
    let (app2, id) = (app.clone(), job_id.to_owned());
    cx.spawn(async move |cx| {
        let id2 = id.clone();
        let res = backend
            .rt
            .spawn(async move {
                Backend::call_coded(&daemon, "cancel_job", json!({ "job_id": id2 })).await
            })
            .await;
        cx.update(|cx| match res {
            Ok(Ok(_)) => {
                lock(&TRACKER).cancelled(&id);
                app::push_toast(
                    &app2,
                    ToastSpec::new(ToastKind::Info, tr!("Upload cancelled")),
                    cx,
                );
                app2.update(cx, |_, cx| cx.notify());
            }
            other => {
                let why = match other {
                    Ok(Err(e)) => crate::daemon_text::call_error_text(&e),
                    Err(e) => e.to_string(),
                    Ok(Ok(_)) => unreachable!(),
                };
                error_toast(&app2, &tr!("Couldn\u{2019}t cancel the upload"), &why, cx);
            }
        });
    })
    .detach();
}

/// An upload the app did not see being queued (started by the CLI or an agent): ask the daemon
/// what it is, then apply the event again.
fn look_up(app: &Entity<AppModel>, job_id: &str, name: &str, data: &Value, cx: &mut App) {
    let backend = app.read(cx).backend.clone();
    let Some(daemon) = backend.daemon.clone() else {
        return;
    };
    let (app2, name, data, id) = (
        app.clone(),
        name.to_owned(),
        data.clone(),
        job_id.to_owned(),
    );
    cx.spawn(async move |cx| {
        let res = backend
            .rt
            .spawn(async move { Backend::call(&daemon, "get_job", json!({ "job_id": id })).await })
            .await;
        if let Ok(Ok(job)) = res {
            cx.update(|cx| {
                lock(&TRACKER).learn(&job);
                apply(&app2, &name, &data, cx);
            });
        }
    })
    .detach();
}

/// Shows `spec` under `key`: replaces the toast the key already has, or pushes a new one.
fn upsert(app: &Entity<AppModel>, key: &str, spec: ToastSpec, cx: &mut App) {
    let known = lock(&TOASTS).get(key).copied();
    let id = app.update(cx, |m, cx| {
        let now = Instant::now();
        let id = known
            .filter(|id| m.toasts.replace(*id, spec.clone(), now))
            .unwrap_or_else(|| m.toasts.push(spec, now));
        cx.notify();
        id
    });
    lock(&TOASTS).insert(key.to_owned(), id);
    app::sync_toasts(app, cx);
}

/// The account was signed in again (Settings → Destinations): the "Reconnect needed" toast has done
/// its job, and a later expiry is news again.
pub fn reconnected(app: &Entity<AppModel>, destination_id: &str, cx: &mut App) {
    lock(&TRACKER).reconnected(destination_id);
    let toast = lock(&TOASTS).remove(&format!("reconnect:{destination_id}"));
    if let Some(id) = toast {
        app::dismiss_toast(app, id, cx);
    }
    crate::ui::recording::sync_tray(app, cx);
}

fn error_toast(app: &Entity<AppModel>, title: &str, why: &str, cx: &mut App) {
    app::push_toast(
        app,
        ToastSpec::new(ToastKind::Error, title.to_owned()).body(why.to_owned()),
        cx,
    );
}

/// Toast "Copy link".
pub fn copy_link(app: &Entity<AppModel>, link: &str, cx: &mut App) {
    match SystemClipboard::default().set_text(link) {
        Ok(()) => app::push_toast(
            app,
            ToastSpec::new(ToastKind::Success, tr!("Link copied")),
            cx,
        ),
        Err(e) => error_toast(app, &tr!("Couldn\u{2019}t copy the link"), &e, cx),
    }
}

/// Toast "Open" on a cloud upload: the link in the browser.
pub fn open_link(app: &Entity<AppModel>, link: &str, cx: &mut App) {
    if let Err(e) = files::open_link(link) {
        error_toast(
            app,
            &tr!("Couldn\u{2019}t open the link"),
            &e.to_string(),
            cx,
        );
    }
}

/// Toast "Open" on a copy to a local folder: the file in Finder / Explorer.
pub fn reveal_path(app: &Entity<AppModel>, path: &str, cx: &mut App) {
    if let Err(e) = files::reveal_path(std::path::Path::new(path)) {
        error_toast(
            app,
            &tr!("Couldn\u{2019}t show the file"),
            &e.to_string(),
            cx,
        );
    }
}

/// Toast "Reconnect...": Settings → Destinations, where the row says "Reconnect".
pub fn open_destinations(app: &Entity<AppModel>, cx: &mut App) {
    app::navigate(app, Route::Destinations, cx);
}

/// Toast "Retry": `retry_job` as the user. The failure toast turns into a short "Retrying" one;
/// if the job fails again the failure toast replaces it.
pub fn retry(app: &Entity<AppModel>, job_id: &str, cx: &mut App) {
    let backend = app.read(cx).backend.clone();
    let Some(daemon) = backend.daemon.clone() else {
        return;
    };
    upsert(
        app,
        &format!("upload:{job_id}"),
        ToastSpec::new(ToastKind::Info, tr!("Retrying the upload\u{2026}")),
        cx,
    );
    let (app2, id) = (app.clone(), job_id.to_owned());
    cx.spawn(async move |cx| {
        let res = backend
            .rt
            .spawn(async move {
                Backend::call_coded(&daemon, "retry_job", json!({ "job_id": id })).await
            })
            .await;
        cx.update(|cx| {
            let why = match res {
                Ok(Ok(_)) => return,
                Ok(Err(e)) => crate::daemon_text::call_error_text(&e),
                Err(e) => e.to_string(),
            };
            error_toast(&app2, &tr!("Couldn\u{2019}t retry the upload"), &why, cx);
        });
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// VOC-2: the toast's Upload sends the exported file, the header's Upload the capture; the
    /// daemon takes only one of them.
    #[test]
    fn the_upload_request_names_a_file_or_a_capture_never_both() {
        assert_eq!(
            upload_params(Some("rec_1"), Some("/x/a.m4a")),
            json!({"file": "/x/a.m4a"})
        );
        assert_eq!(
            upload_params(Some("rec_1"), None),
            json!({"capture_id": "rec_1"})
        );
        assert_eq!(upload_params(None, None), json!({}));
    }
}
