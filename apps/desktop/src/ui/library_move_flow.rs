//! "Change library folder" (LIB-2) as the app drives it: the folder picker, the checks and the
//! question, the `move_library` call (as the user: the app asked already), the copy job's
//! progress, the restart and the optional Trash step. The words and the state are in
//! `crate::library_move`; this file only moves them between the daemon and the windows.

use gpui::{App, Entity};
use std::path::PathBuf;
use std::time::Duration;

use cm_daemon::{Decision, MoveMode, MoveProblem};
use cm_i18n::{tr, tr_ctx};
use serde_json::json;

use crate::backend::Backend;
use crate::flows;
use crate::library_move::{self as lm, MovedInfo, Phase};
use crate::toast::{ToastKind, ToastSpec};
use crate::ui::app::{self, AppModel};

fn set_phase(app: &Entity<AppModel>, phase: Phase, cx: &mut App) {
    app.update(cx, |m, cx| {
        m.library_move = phase;
        cx.notify();
    });
}

fn phase(app: &Entity<AppModel>, cx: &App) -> Phase {
    app.read(cx).library_move.clone()
}

/// "Change...": the system folder picker, then the checks.
pub fn change_folder(app: &Entity<AppModel>, cx: &mut App) {
    if matches!(phase(app, cx), Phase::Checking | Phase::Moving { .. }) {
        return;
    }
    let rx = cx.prompt_for_paths(gpui::PathPromptOptions {
        files: false,
        directories: true,
        multiple: false,
        prompt: Some(tr_ctx!("folder dialog", "Choose folder").into()),
    });
    let app = app.clone();
    cx.spawn(async move |cx| {
        let Ok(Ok(Some(paths))) = rx.await else {
            return;
        };
        let Some(path) = paths.into_iter().next() else {
            return;
        };
        cx.update(|cx| check_and_ask(&app, path, cx));
    })
    .detach();
}

/// What the checks found for a chosen folder.
enum Found {
    Move(cm_daemon::MovePlan),
    FreshOnly {
        plan: cm_daemon::MovePlan,
        needed: u64,
        free: u64,
    },
    UseExisting(cm_daemon::MovePlan),
    Problem(MoveProblem),
}

fn check_and_ask(app: &Entity<AppModel>, path: PathBuf, cx: &mut App) {
    let backend = app.read(cx).backend.clone();
    let Some(daemon) = backend.daemon.clone() else {
        return;
    };
    set_phase(app, Phase::Checking, cx);
    let app = app.clone();
    cx.spawn(async move |cx| {
        let p = path.clone();
        let found = backend
            .rt
            .spawn(async move {
                match daemon.check_library_move(&p, MoveMode::Move).await {
                    Ok(plan) => Found::Move(plan),
                    // A library is there: offer to use it.
                    Err(MoveProblem::TargetHasLibrary { .. }) => {
                        match daemon.check_library_move(&p, MoveMode::UseExisting).await {
                            Ok(plan) => Found::UseExisting(plan),
                            Err(e) => Found::Problem(e),
                        }
                    }
                    // It does not fit: a fresh start still can.
                    Err(MoveProblem::NotEnoughSpace { needed, free, .. }) => {
                        match daemon.check_library_move(&p, MoveMode::Fresh).await {
                            Ok(plan) => Found::FreshOnly { plan, needed, free },
                            Err(e) => Found::Problem(e),
                        }
                    }
                    Err(e) => Found::Problem(e),
                }
            })
            .await;
        cx.update(|cx| {
            set_phase(&app, Phase::Idle, cx);
            let Ok(found) = found else { return };
            match found {
                Found::Move(plan) => {
                    let model = lm::move_dialog(&plan);
                    let to = plan.to.clone();
                    app::ask_confirm(
                        &app,
                        model,
                        move |app, decision, cx| match decision {
                            Decision::Decline => {}
                            Decision::Approve => start(app, to, MoveMode::Move, cx),
                            Decision::ApproveForSession => start(app, to, MoveMode::Fresh, cx),
                        },
                        cx,
                    );
                }
                Found::FreshOnly { plan, needed, free } => {
                    let model = lm::fresh_only_dialog(&plan, needed, free);
                    let to = plan.to.clone();
                    app::ask_confirm(
                        &app,
                        model,
                        move |app, d, cx| {
                            if d != Decision::Decline {
                                start(app, to, MoveMode::Fresh, cx);
                            }
                        },
                        cx,
                    );
                }
                Found::UseExisting(plan) => {
                    let model = lm::use_existing_dialog(&plan);
                    let to = plan.to.clone();
                    app::ask_confirm(
                        &app,
                        model,
                        move |app, d, cx| {
                            if d != Decision::Decline {
                                start(app, to, MoveMode::UseExisting, cx);
                            }
                        },
                        cx,
                    );
                }
                Found::Problem(p) => {
                    let again = p.is_temporary();
                    app::ask_confirm(
                        &app,
                        lm::problem_dialog(&p),
                        move |app, d, cx| {
                            if d == Decision::Decline {
                                return;
                            }
                            // "Check again" asks about the same folder; the others pick another.
                            if again {
                                check_and_ask(app, path.clone(), cx);
                            } else {
                                change_folder(app, cx);
                            }
                        },
                        cx,
                    );
                }
            }
        });
    })
    .detach();
}

/// `move_library` as the user, then the progress (a move) or the answer (fresh, use existing).
fn start(app: &Entity<AppModel>, to: PathBuf, mode: MoveMode, cx: &mut App) {
    let backend = app.read(cx).backend.clone();
    let Some(daemon) = backend.daemon.clone() else {
        return;
    };
    set_phase(app, Phase::Checking, cx);
    let app = app.clone();
    cx.spawn(async move |cx| {
        let params = json!({ "path": to.to_string_lossy(), "mode": mode.as_str() });
        let res = backend
            .rt
            .spawn(async move { Backend::call_coded(&daemon, "move_library", params).await })
            .await;
        cx.update(|cx| match res {
            Ok(Ok(v)) => {
                let from = PathBuf::from(v["from"].as_str().unwrap_or_default());
                match v["job_id"].as_str() {
                    Some(job) => {
                        set_phase(
                            &app,
                            Phase::Moving {
                                job_id: job.to_owned(),
                                to: to.clone(),
                                fraction: 0.0,
                            },
                            cx,
                        );
                        watch(&app, job.to_owned(), from, to, cx);
                    }
                    None => {
                        let to2 = to.clone();
                        set_phase(&app, Phase::Switched { from, to, mode }, cx);
                        ask_restart(&app, lm::switched_dialog(&to2), cx);
                    }
                }
            }
            Ok(Err(e)) => {
                set_phase(&app, Phase::Idle, cx);
                app::push_toast(
                    &app,
                    flows::error_toast_live(
                        || tr!("Couldn\u{2019}t change the library folder"),
                        e.message,
                    ),
                    cx,
                );
            }
            Err(e) => {
                set_phase(&app, Phase::Idle, cx);
                app::push_toast(
                    &app,
                    flows::error_toast_live(
                        || tr!("Couldn\u{2019}t change the library folder"),
                        e.to_string(),
                    ),
                    cx,
                );
            }
        });
    })
    .detach();
}

/// Looks at the job every half second until it ends (the progress bar, then the answer).
pub fn watch(app: &Entity<AppModel>, job_id: String, from: PathBuf, to: PathBuf, cx: &mut App) {
    let backend = app.read(cx).backend.clone();
    let Some(daemon) = backend.daemon.clone() else {
        return;
    };
    let app = app.clone();
    cx.spawn(async move |cx| loop {
        cx.background_executor()
            .timer(Duration::from_millis(500))
            .await;
        let (d, id) = (daemon.clone(), job_id.clone());
        let job = backend
            .rt
            .spawn(async move { Backend::call(&d, "get_job", json!({ "job_id": id })).await })
            .await;
        let Ok(Ok(job)) = job else {
            continue;
        };
        let state = job["state"].as_str().unwrap_or_default().to_owned();
        let done = cx.update(|cx| {
            // The user dismissed it or another move took over: stop looking.
            match phase(&app, cx) {
                Phase::Moving { job_id: j, .. } if j == job_id => {}
                _ => return true,
            }
            match state.as_str() {
                "done" => {
                    let r = &job["result"];
                    let info = MovedInfo {
                        from: from.clone(),
                        to: to.clone(),
                        bundles: r["bundles"].as_u64().unwrap_or(0) as usize,
                        bytes: r["bytes"].as_u64().unwrap_or(0),
                        copied: r["copied"]
                            .as_array()
                            .map(|a| {
                                a.iter()
                                    .filter_map(|v| v.as_str().map(str::to_owned))
                                    .collect()
                            })
                            .unwrap_or_default(),
                        trashed: false,
                    };
                    set_phase(&app, Phase::Moved(info.clone()), cx);
                    ask_restart(&app, lm::moved_dialog(&info), cx);
                    true
                }
                "failed" => {
                    let reason = job["error"]["reason"]
                        .as_str()
                        .unwrap_or_default()
                        .to_owned();
                    set_phase(
                        &app,
                        Phase::Failed {
                            job_id: job_id.clone(),
                            to: to.clone(),
                            reason: reason.clone(),
                        },
                        cx,
                    );
                    app::push_toast(
                        &app,
                        flows::error_toast_live(|| tr!("The library move stopped"), reason),
                        cx,
                    );
                    true
                }
                "cancelled" => {
                    set_phase(&app, Phase::Cancelled { to: to.clone() }, cx);
                    true
                }
                _ => {
                    let fraction = job["progress"].as_f64().unwrap_or(0.0);
                    set_phase(
                        &app,
                        Phase::Moving {
                            job_id: job_id.clone(),
                            to: to.clone(),
                            fraction,
                        },
                        cx,
                    );
                    false
                }
            }
        });
        if done {
            break;
        }
    })
    .detach();
}

/// `library.move_started` and `library.moved` from the bus: a move or switch that an agent
/// started shows in the Library row like one the user started.
pub fn on_event(app: &Entity<AppModel>, ev: &cm_daemon::Event, cx: &mut App) {
    if !matches!(phase(app, cx), Phase::Idle) {
        return;
    }
    let path = |k: &str| PathBuf::from(ev.data[k].as_str().unwrap_or_default());
    match ev.name.as_str() {
        "library.move_started" => {
            let (job, from, to) = (
                ev.data["job_id"].as_str().unwrap_or_default().to_owned(),
                path("from"),
                path("to"),
            );
            set_phase(
                app,
                Phase::Moving {
                    job_id: job.clone(),
                    to: to.clone(),
                    fraction: 0.0,
                },
                cx,
            );
            watch(app, job, from, to, cx);
        }
        "library.moved" => {
            let mode = match ev.data["mode"].as_str() {
                Some("fresh") => MoveMode::Fresh,
                Some("use_existing") => MoveMode::UseExisting,
                _ => return,
            };
            set_phase(
                app,
                Phase::Switched {
                    from: path("from"),
                    to: path("to"),
                    mode,
                },
                cx,
            );
        }
        _ => {}
    }
}

/// At startup of the Settings page: a copy that was running when the app quit resumes by itself
/// (the job queue recovers it); pick it up again so the row shows it.
pub fn adopt_running(app: &Entity<AppModel>, cx: &mut App) {
    if !matches!(phase(app, cx), Phase::Idle) {
        return;
    }
    let backend = app.read(cx).backend.clone();
    let Some(daemon) = backend.daemon.clone() else {
        return;
    };
    let app = app.clone();
    cx.spawn(async move |cx| {
        let listed = backend
            .rt
            .spawn(async move {
                Backend::call(
                    &daemon,
                    "list_jobs",
                    json!({ "type": "library_move", "limit": 5 }),
                )
                .await
            })
            .await;
        let Ok(Ok(v)) = listed else { return };
        let running = v["jobs"].as_array().and_then(|jobs| {
            jobs.iter()
                .find(|j| matches!(j["state"].as_str(), Some("queued" | "running" | "paused")))
        });
        let Some(job) = running else { return };
        let (id, from, to) = (
            job["job_id"].as_str().unwrap_or_default().to_owned(),
            PathBuf::from(job["inputs"]["from"].as_str().unwrap_or_default()),
            PathBuf::from(job["inputs"]["to"].as_str().unwrap_or_default()),
        );
        cx.update(|cx| {
            if matches!(phase(&app, cx), Phase::Idle) {
                set_phase(
                    &app,
                    Phase::Moving {
                        job_id: id.clone(),
                        to: to.clone(),
                        fraction: job["progress"].as_f64().unwrap_or(0.0),
                    },
                    cx,
                );
                watch(&app, id, from, to, cx);
            }
        });
    })
    .detach();
}

/// Cancels the copy: the captures copied so far stay in the new folder, the old library stays in
/// use.
pub fn cancel(app: &Entity<AppModel>, cx: &mut App) {
    job_action(app, "cancel_job", cx);
}

/// Resumes a failed copy.
pub fn retry(app: &Entity<AppModel>, cx: &mut App) {
    let Phase::Failed { job_id, to, .. } = phase(app, cx) else {
        return;
    };
    let from = app
        .read(cx)
        .backend
        .library_root
        .clone()
        .unwrap_or_default();
    job_action(app, "retry_job", cx);
    set_phase(
        app,
        Phase::Moving {
            job_id: job_id.clone(),
            to: to.clone(),
            fraction: 0.0,
        },
        cx,
    );
    watch(app, job_id, from, to, cx);
}

fn job_action(app: &Entity<AppModel>, method: &'static str, cx: &mut App) {
    let id = match phase(app, cx) {
        Phase::Moving { job_id, .. } | Phase::Failed { job_id, .. } => job_id,
        _ => return,
    };
    let backend = app.read(cx).backend.clone();
    let Some(daemon) = backend.daemon.clone() else {
        return;
    };
    cx.spawn(async move |_| {
        let _ = backend
            .rt
            .spawn(async move { Backend::call(&daemon, method, json!({ "job_id": id })).await })
            .await;
    })
    .detach();
}

/// Clears the note of a failed, cancelled or finished move.
pub fn dismiss(app: &Entity<AppModel>, cx: &mut App) {
    if !matches!(phase(app, cx), Phase::Moving { .. } | Phase::Checking) {
        set_phase(app, Phase::Idle, cx);
    }
}

/// "The library moved to ...": Restart CaptureMann, or later (Cancel; the row keeps the note).
pub fn ask_restart(app: &Entity<AppModel>, model: crate::confirm::DialogModel, cx: &mut App) {
    app::ask_confirm(
        app,
        model,
        |app, d, cx| {
            if d != Decision::Decline {
                app::restart(app, cx);
            }
        },
        cx,
    );
}

/// The optional Trash step, with its own confirmation. Only the bundles the job verified are
/// offered, and the daemon checks each against its copy again.
pub fn ask_trash(app: &Entity<AppModel>, cx: &mut App) {
    let Phase::Moved(info) = phase(app, cx) else {
        return;
    };
    if info.trashed || info.copied.is_empty() {
        return;
    }
    app::ask_confirm(
        app,
        lm::trash_dialog(&info),
        move |app, d, cx| {
            if d == Decision::Decline {
                return;
            }
            let backend = app.read(cx).backend.clone();
            let Some(daemon) = backend.daemon.clone() else {
                return;
            };
            let (app2, info2) = (app.clone(), info.clone());
            cx.spawn(async move |cx| {
                let (from, to, names) =
                    (info2.from.clone(), info2.to.clone(), info2.copied.clone());
                let res = backend
                    .rt
                    .spawn(async move { daemon.trash_old_copies(&from, &to, &names).await })
                    .await;
                cx.update(|cx| match res {
                    Ok(Ok(report)) => {
                        let (title, body) =
                            lm::trash_result_text(report.trashed.len(), report.kept.len());
                        let mut spec = ToastSpec::new(ToastKind::Success, title);
                        if let Some(b) = body {
                            spec = spec.body(b);
                        }
                        app::push_toast(&app2, spec, cx);
                        let mut done = info2;
                        done.trashed = true;
                        set_phase(&app2, Phase::Moved(done), cx);
                    }
                    Ok(Err(e)) => app::push_toast(
                        &app2,
                        flows::error_toast_live(
                            || tr!("Couldn\u{2019}t move the old copies to the Trash"),
                            e.to_string(),
                        ),
                        cx,
                    ),
                    Err(e) => app::push_toast(
                        &app2,
                        flows::error_toast_live(
                            || tr!("Couldn\u{2019}t move the old copies to the Trash"),
                            e.to_string(),
                        ),
                        cx,
                    ),
                });
            })
            .detach();
        },
        cx,
    );
}
