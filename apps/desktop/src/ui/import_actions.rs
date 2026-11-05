//! Import media as the app drives it (IMP-5): the file picker, the check (`import_media` with
//! `dry_run`), the start, the job's events and the cancel. The words and the state are in
//! `crate::import_flow`; the windows are `ui::import_sheet`.

use std::path::{Path, PathBuf};

use cm_i18n::tr;
use gpui::{App, Entity};
use serde_json::{json, Value};

use crate::backend::Backend;
use crate::daemon_text::call_error_text;
use crate::import_flow::{self as f, ImportRun, Sheet};
use crate::toast::{ToastAction, ToastKind, ToastSpec};
use crate::ui::app::{self, AppModel};

/// Opens the sheet: with `paths`, checks them; with none, asks for files first.
pub fn open(app: &Entity<AppModel>, paths: Vec<PathBuf>, cx: &mut App) {
    app::open_studio(app, cx);
    if paths.is_empty() {
        // After the window has been raised: when this runs from inside the Studio the raise is
        // queued until the current update ends, and a file panel opened before it ended up
        // behind the raised window (the Import button seemed to do nothing, Nov 3, 2026).
        // The raise itself runs in a task of its own, so wait a moment for it before the panel opens.
        let app = app.clone();
        cx.spawn(async move |cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(200))
                .await;
            cx.update(|cx| pick(&app, cx));
        })
        .detach();
        return;
    }
    add_paths(app, paths, cx);
}

/// The system file picker: files only, several at once (folders are not importable).
pub fn pick(app: &Entity<AppModel>, cx: &mut App) {
    let rx = cx.prompt_for_paths(gpui::PathPromptOptions {
        files: true,
        directories: false,
        multiple: true,
        prompt: Some(cm_i18n::tr_ctx!("file dialog", "Choose files").into()),
    });
    let app = app.clone();
    cx.spawn(async move |cx| {
        let Ok(Ok(Some(paths))) = rx.await else {
            return;
        };
        cx.update(|cx| add_paths(&app, paths, cx));
    })
    .detach();
}

/// Adds files to the sheet (opening it if needed) after the daemon has looked at them.
pub fn add_paths(app: &Entity<AppModel>, paths: Vec<PathBuf>, cx: &mut App) {
    let is_dir = |p: &Path| p.is_dir();
    let folders = paths.iter().filter(|p| is_dir(p)).count();
    let files = f::drop_files(&paths, is_dir);
    app.update(cx, |m, cx| {
        let sheet = m.import.sheet.get_or_insert_with(Sheet::default);
        sheet.busy = true;
        sheet.error = None;
        sheet.folders_skipped += folders;
        cx.notify();
    });
    if files.is_empty() {
        // Only folders were dropped: nothing to show (an empty sheet is not left open).
        app.update(cx, |m, cx| {
            if m.import.sheet.as_ref().is_some_and(|s| s.rows.is_empty()) {
                m.import.sheet = None;
            } else if let Some(s) = m.import.sheet.as_mut() {
                s.busy = false;
            }
            cx.notify();
        });
        return;
    }
    let backend = app.read(cx).backend.clone();
    let Some(daemon) = backend.daemon.clone() else {
        finish_check(
            app,
            Err(tr!(
                "Importing needs the CaptureMann engine, which is not running."
            )),
            None,
            false,
            cx,
        );
        return;
    };
    let root = backend.library_root.clone();
    let app2 = app.clone();
    cx.spawn(async move |cx| {
        let res = backend
            .rt
            .spawn(async move {
                let params = json!({ "paths": files.iter().map(|p| p.to_string_lossy()).collect::<Vec<_>>(), "dry_run": true });
                let rows = Backend::call_coded(&daemon, "import_media", params).await;
                // Free space and the speaker model, for the footer and the After import switches.
                let free = root.as_deref().and_then(|r| {
                    use cm_capture::disk::FreeSpace;
                    cm_capture::disk::SystemFreeSpace.free_bytes(r)
                });
                let speakers = Backend::call_coded(&daemon, "list_models", json!({}))
                    .await
                    .ok()
                    .map(|m| {
                        m["models"].as_array().is_some_and(|a| {
                            a.iter().any(|x| x["kind"] == "speaker" && x["installed"] == true)
                        })
                    })
                    .unwrap_or(false);
                (rows, free, speakers)
            })
            .await;
        cx.update(|cx| match res {
            Ok((Ok(v), free, speakers)) => finish_check(&app2, Ok(v), free, speakers, cx),
            Ok((Err(e), ..)) => finish_check(&app2, Err(call_error_text(&e)), None, false, cx),
            Err(e) => finish_check(&app2, Err(e.to_string()), None, false, cx),
        });
    })
    .detach();
}

fn finish_check(
    app: &Entity<AppModel>,
    res: Result<Value, String>,
    free: Option<u64>,
    speakers: bool,
    cx: &mut App,
) {
    app.update(cx, |m, cx| {
        if let Some(s) = m.import.sheet.as_mut() {
            s.busy = false;
            match res {
                Ok(v) => {
                    let rows = v["files"].as_array().cloned().unwrap_or_default();
                    s.add_rows(&rows);
                    s.free_bytes = free.or(s.free_bytes);
                    s.speaker_model = speakers;
                }
                Err(msg) => s.error = Some(msg),
            }
            m.import.epoch += 1;
        }
        cx.notify();
    });
}

pub fn close_sheet(app: &Entity<AppModel>, cx: &mut App) {
    app.update(cx, |m, cx| {
        m.import.sheet = None;
        cx.notify();
    });
}

/// The sheet's Import button: `import_media` as the user (the sheet is the consent, also for
/// removing the originals).
pub fn start(app: &Entity<AppModel>, cx: &mut App) {
    let (params, sizes) = {
        let m = app.read(cx);
        let Some(s) = m.import.sheet.as_ref().filter(|s| s.can_import()) else {
            return;
        };
        let sizes: Vec<(PathBuf, u64)> = s
            .rows
            .iter()
            .map(|r| (r.path.clone(), r.size_bytes))
            .collect();
        (s.request(), sizes)
    };
    let backend = app.read(cx).backend.clone();
    let Some(daemon) = backend.daemon.clone() else {
        return;
    };
    app.update(cx, |m, cx| {
        if let Some(s) = m.import.sheet.as_mut() {
            s.busy = true;
            s.error = None;
        }
        cx.notify();
    });
    let app2 = app.clone();
    cx.spawn(async move |cx| {
        let res = backend
            .rt
            .spawn(async move { Backend::call_coded(&daemon, "import_media", params).await })
            .await;
        cx.update(|cx| match res {
            Ok(Ok(v)) => started(&app2, &v, &sizes, cx),
            Ok(Err(e)) => fail_start(&app2, call_error_text(&e), cx),
            Err(e) => fail_start(&app2, e.to_string(), cx),
        });
    })
    .detach();
}

fn fail_start(app: &Entity<AppModel>, msg: String, cx: &mut App) {
    app.update(cx, |m, cx| {
        if let Some(s) = m.import.sheet.as_mut() {
            s.busy = false;
            s.error = Some(msg);
        }
        cx.notify();
    });
}

fn started(app: &Entity<AppModel>, answer: &Value, sizes: &[(PathBuf, u64)], cx: &mut App) {
    let Some(run) = ImportRun::from_started(answer, sizes) else {
        fail_start(app, tr!("The import did not start."), cx);
        return;
    };
    app.update(cx, |m, cx| {
        m.import.sheet = None;
        m.import.runs.push(run);
        m.library_epoch += 1;
        cx.notify();
    });
    update_toast(app, cx);
}

/// `cancel_job` on the running import: the daemon sends no event for it, so the answer closes
/// the cards here.
pub fn cancel(app: &Entity<AppModel>, job_id: &str, cx: &mut App) {
    let backend = app.read(cx).backend.clone();
    let Some(daemon) = backend.daemon.clone() else {
        return;
    };
    let (app2, id) = (app.clone(), job_id.to_owned());
    cx.spawn(async move |cx| {
        let id2 = id.clone();
        let res = backend
            .rt
            .spawn(async move {
                Backend::call_coded(&daemon, "cancel_job", json!({ "job_id": id2 })).await
            })
            .await;
        cx.update(|cx| {
            if matches!(res, Ok(Ok(_))) {
                end_run(&app2, &id, |r| r.fail(&tr!("Cancelled"), true), cx);
                app::push_toast(
                    &app2,
                    ToastSpec::new(ToastKind::Info, tr!("Import cancelled"))
                        .body(tr!("The files already imported are in your library.")),
                    cx,
                );
                app2.update(cx, |m, cx| {
                    m.library_epoch += 1;
                    cx.notify();
                });
            }
        });
    })
    .detach();
}

fn end_run(app: &Entity<AppModel>, job: &str, f: impl FnOnce(&mut ImportRun), cx: &mut App) {
    app.update(cx, |m, cx| {
        if let Some(r) = m.import.runs.iter_mut().find(|r| r.job_id == job) {
            f(r);
        }
        cx.notify();
    });
    update_toast(app, cx);
}

/// Shows, updates or closes the job toast. It stays while the import runs; the toast that ends
/// it is a normal 5 s one (Success) or stays (a failure).
fn update_toast(app: &Entity<AppModel>, cx: &mut App) {
    let (spec, old) = {
        let m = app.read(cx);
        let Some(run) = m.import.running() else {
            return;
        };
        let pct = (run.fraction * 100.0).round().clamp(0.0, 100.0) as u8;
        let mut spec = ToastSpec::new(ToastKind::Info, run.toast_title()).body(run.toast_body());
        spec.progress = Some(pct);
        (
            spec.button(tr!("Cancel"), ToastAction::CancelImport(run.job_id.clone())),
            m.import.toast_id,
        )
    };
    let id = app.update(cx, |m, cx| {
        let now = std::time::Instant::now();
        let id = match old.filter(|id| m.toasts.replace(*id, spec.clone(), now)) {
            Some(id) => id,
            None => m.toasts.push(spec, now),
        };
        m.import.toast_id = Some(id);
        cx.notify();
        id
    });
    let _ = id;
    app::sync_toasts(app, cx);
}

/// Job events of an `import` job (`job.progress`, `job.done`, `job.failed`).
pub fn on_event(app: &Entity<AppModel>, ev: &cm_daemon::Event, cx: &mut App) {
    if ev.data["type"] != "import" {
        return;
    }
    let Some(job) = ev.data["job_id"].as_str().map(str::to_owned) else {
        return;
    };
    match ev.name.as_str() {
        "job.progress" => {
            let p = ev.data["progress"].as_f64().unwrap_or(0.0);
            let msg = f::step_label(ev.data["message"].as_str().unwrap_or(""));
            end_run(app, &job, |r| r.progress(p, &msg), cx);
        }
        "job.done" => {
            let result = ev.data["result"].clone();
            end_run(app, &job, |r| r.finish(&result), cx);
            close_toast(app, cx);
            let (ok, bad) = {
                let m = app.read(cx);
                m.import
                    .runs
                    .iter()
                    .find(|r| r.job_id == job)
                    .map(|r| {
                        (
                            r.cards
                                .iter()
                                .filter(|c| c.state == f::CardState::Done)
                                .count(),
                            r.cards
                                .iter()
                                .filter(|c| matches!(c.state, f::CardState::Failed(_)))
                                .count(),
                        )
                    })
                    .unwrap_or((0, 0))
            };
            let title = if ok == 1 {
                tr!("Imported 1 file")
            } else {
                tr!("Imported {n} files", n = ok)
            };
            let mut spec = ToastSpec::new(
                if bad > 0 {
                    ToastKind::Warning
                } else {
                    ToastKind::Success
                },
                title,
            )
            .button(tr!("Open library"), ToastAction::OpenLibrary);
            if bad > 0 {
                spec = spec.body(tr!(
                    "{n} could not be imported. Their cards say why.",
                    n = bad
                ));
            }
            app::push_toast(app, spec, cx);
            app.update(cx, |m, cx| {
                m.library_epoch += 1;
                cx.notify();
            });
        }
        "job.failed" => {
            let reason = ev.data["error"]["reason"].as_str().unwrap_or("").to_owned();
            let code = ev.data["error"]["code"].as_str().unwrap_or("");
            let msg = if code == "disk_full" {
                tr!("There isn't enough free space on the library's disk.")
            } else if code == "permission_missing" {
                if crate::permissions::Os::current() == crate::permissions::Os::Mac {
                    tr!("CaptureMann isn't allowed to read these files. Allow it in System Settings, Privacy & Security, Files and Folders.")
                } else {
                    tr!("CaptureMann isn't allowed to read these files. Check the permissions of the folder they are in.")
                }
            } else if reason.is_empty() {
                tr!("The import failed.")
            } else {
                reason
            };
            let m2 = msg.clone();
            end_run(app, &job, |r| r.fail(&m2, false), cx);
            close_toast(app, cx);
            // A failure stays until dismissed.
            app::push_toast(
                app,
                ToastSpec::new(ToastKind::Error, tr!("Import failed")).body(msg),
                cx,
            );
        }
        _ => {}
    }
}

fn close_toast(app: &Entity<AppModel>, cx: &mut App) {
    let id = app.update(cx, |m, _| m.import.toast_id.take());
    if let Some(id) = id {
        app::dismiss_toast(app, id, cx);
    }
}

/// Dismisses a failed card.
pub fn dismiss_card(app: &Entity<AppModel>, run: usize, card: usize, cx: &mut App) {
    app.update(cx, |m, cx| {
        if let Some(c) = m
            .import
            .runs
            .get_mut(run)
            .and_then(|r| r.cards.get_mut(card))
        {
            c.dismissed = true;
        }
        cx.notify();
    });
}

/// "Show file": the original in Finder.
pub fn show_file(path: &Path) {
    let _ = crate::recording::files::reveal_path(path);
}
