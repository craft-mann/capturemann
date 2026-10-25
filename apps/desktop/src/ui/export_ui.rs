//! Window glue of the quick export (REC-1, EXP-1): turns the daemon's `export.*` and `job.*` events
//! into toasts ("Recording saved - exporting...", "Exported <title>" with Play and Reveal, "FFmpeg
//! not found"), feeds the library details card, and runs the manual Export, Play and Reveal
//! actions. The decisions live in `recording::exports` (unit tested without a display).

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex, MutexGuard, PoisonError};
use std::time::Instant;

use cm_daemon::events::Event;
use cm_i18n::tr;
use gpui::{App, Entity};
use serde_json::json;

use crate::backend::Backend;
use crate::recording::exports::{
    export_failed_toast, export_skipped_toast, exported_toast, exporting_toast, Effect,
    ExportStatus, ExportTracker,
};
use crate::recording::files;
use crate::toast::{ToastKind, ToastSpec};
use crate::ui::app::{self, AppModel};

/// "Recording saved" is the title of the toast the export turns into a progress toast. The
/// toast is built with the same function (`recording::toasts::saved_title`), so the lookup
/// finds it in every language.
pub(crate) fn saved_prefix() -> String {
    crate::recording::toasts::saved_title()
}

/// The FFmpeg hint was shown in this run.
static HINTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

static TRACKER: LazyLock<Mutex<ExportTracker>> = LazyLock::new(Mutex::default);
/// The toast showing each capture's export, so progress and the result replace it in place.
static TOASTS: LazyLock<Mutex<HashMap<String, u64>>> = LazyLock::new(Mutex::default);

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The export job state of a recording for the details card.
pub fn status_of(capture_id: &str) -> Option<ExportStatus> {
    lock(&TRACKER).status_of(capture_id).cloned()
}

/// Applies one `export.*` or `job.*` event.
pub fn on_event(app: &Entity<AppModel>, ev: &Event, cx: &mut App) {
    let effects = {
        let mut t = lock(&TRACKER);
        let e = t.on_event(&ev.name, &ev.data);
        if ev.name == "export.queued" {
            t.prune(20);
        }
        e
    };
    for effect in effects {
        match effect {
            Effect::Progress {
                capture_id,
                title,
                pct,
                auto,
                first,
            } => upsert(
                app,
                &capture_id,
                exporting_toast(&capture_id, title.as_deref(), pct, auto),
                first,
                cx,
            ),
            Effect::Done {
                capture_id,
                title,
                captions,
            } => {
                upsert(
                    app,
                    &capture_id,
                    exported_toast(&capture_id, &title, captions.as_deref()),
                    false,
                    cx,
                );
                lock(&TOASTS).remove(&capture_id);
            }
            Effect::Skipped { capture_id, ffmpeg } => {
                // Once per run: a hint that tells how to get the combined MP4, not an error.
                if ffmpeg && !HINTED.swap(true, std::sync::atomic::Ordering::Relaxed) {
                    app::push_toast(app, export_skipped_toast(&capture_id), cx);
                }
            }
            Effect::Failed {
                capture_id,
                reason,
                ffmpeg_missing,
                ..
            } => {
                upsert(
                    app,
                    &capture_id,
                    export_failed_toast(&capture_id, &reason, ffmpeg_missing),
                    false,
                    cx,
                );
                lock(&TOASTS).remove(&capture_id);
            }
        }
    }
    // The details card observes the model: wake it for progress and results.
    app.update(cx, |_, cx| cx.notify());
}

/// Shows `spec` in the capture's export toast: replaces the one it already has, or (for the
/// first update) turns the take's "Recording saved" toast into it, or pushes a new toast.
fn upsert(app: &Entity<AppModel>, capture_id: &str, spec: ToastSpec, first: bool, cx: &mut App) {
    let known = lock(&TOASTS).get(capture_id).copied();
    let id = app.update(cx, |m, cx| {
        let now = Instant::now();
        let mut id = known.filter(|id| m.toasts.replace(*id, spec.clone(), now));
        if id.is_none() && first {
            id = m
                .toasts
                .find_for_capture(capture_id, &saved_prefix())
                .filter(|id| m.toasts.replace(*id, spec.clone(), now));
        }
        let id = id.unwrap_or_else(|| m.toasts.push(spec, now));
        cx.notify();
        id
    });
    lock(&TOASTS).insert(capture_id.to_owned(), id);
    app::sync_toasts(app, cx);
}

fn error_toast(app: &Entity<AppModel>, title: &str, why: &str, cx: &mut App) {
    app::push_toast(
        app,
        ToastSpec::new(ToastKind::Error, title.to_owned()).body(why.to_owned()),
        cx,
    );
}

/// The Export button: `export_video` as the user. `captions` is the choice of the checkbox beside
/// it (`Some(true)` burns the captions in, `Some(false)` leaves them out; `None` follows the
/// recording's own setting). The result arrives as events.
pub fn request(app: &Entity<AppModel>, capture_id: &str, captions: Option<bool>, cx: &mut App) {
    let backend = app.read(cx).backend.clone();
    let Some(daemon) = backend.daemon.clone() else {
        return;
    };
    let (app2, id) = (app.clone(), capture_id.to_owned());
    let params = export_params(&id, captions);
    cx.spawn(async move |cx| {
        let res = backend
            .rt
            .spawn(async move { Backend::call_coded(&daemon, "export_video", params).await })
            .await;
        cx.update(|cx| {
            let why = match res {
                Ok(Ok(_)) => return,
                Ok(Err(e)) => crate::daemon_text::call_error_text(&e),
                Err(e) => e.to_string(),
            };
            error_toast(
                &app2,
                &tr!("The video couldn\u{2019}t be exported"),
                &why,
                cx,
            );
        });
    })
    .detach();
}

/// Runs a file action (`files::play_in_system_player`, ...) on the recording `capture_id`.
fn file_action(
    app: &Entity<AppModel>,
    capture_id: &str,
    action: fn(&std::path::Path) -> Result<(), files::FileError>,
    cx: &mut App,
) {
    let root = app.read(cx).backend.library_root.clone();
    let (app2, id) = (app.clone(), capture_id.to_owned());
    cx.spawn(async move |cx| {
        let res = cx
            .background_executor()
            .spawn(async move {
                let root = root.ok_or(files::FileError::NoLibrary)?;
                let dir = files::find_bundle(&root, &id).ok_or(files::FileError::NoFolder)?;
                action(&dir)
            })
            .await;
        if let Err(e) = res {
            cx.update(|cx| {
                error_toast(
                    &app2,
                    &tr!("Couldn\u{2019}t open the video"),
                    &e.to_string(),
                    cx,
                )
            });
        }
    })
    .detach();
}

/// Toast "Play": the combined MP4 in the system player.
pub fn play(app: &Entity<AppModel>, capture_id: &str, cx: &mut App) {
    file_action(app, capture_id, files::play_in_system_player, cx);
}

/// Toast "Reveal": the combined MP4 in Finder.
pub fn reveal(app: &Entity<AppModel>, capture_id: &str, cx: &mut App) {
    file_action(app, capture_id, files::reveal_export, cx);
}

/// The `export_video` request: the capture and, when the user chose, whether to burn captions in.
fn export_params(capture_id: &str, captions: Option<bool>) -> serde_json::Value {
    let mut p = json!({ "capture_id": capture_id });
    if let Some(burn) = captions {
        p["captions"] = json!(if burn { "burn" } else { "none" });
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    /// AI-1: the export checkbox maps to `captions: "burn" | "none"`; no choice sends nothing, so
    /// the recording's own setting decides.
    #[test]
    fn the_captions_choice_is_sent_only_when_made() {
        assert_eq!(export_params("rec_1", None), json!({"capture_id": "rec_1"}));
        assert_eq!(
            export_params("rec_1", Some(true)),
            json!({"capture_id": "rec_1", "captions": "burn"})
        );
        assert_eq!(
            export_params("rec_1", Some(false)),
            json!({"capture_id": "rec_1", "captions": "none"})
        );
    }
}
