//! The voice part of the AI page (V8, AI-7, AI-8): what the buttons do.
//!
//! Every change goes through `set_settings` as the user, the way `ai.glossary` does: `ai.*` is
//! app-only, so an agent cannot write these keys. The rules and the state are in
//! `crate::ai::voice`.

use gpui::{App, Entity};
use serde_json::Value;

use cm_i18n::tr;

use crate::ai::models::{Effect, RowAction};
use crate::ai::voice::{self, TestGate, TestState};
use crate::backend::Backend;
use crate::daemon_text::ErrorNote;
use crate::flows;
use crate::ui::ai::client_of;
use crate::ui::app::{self, AppModel};

/// The first reads of the voice section, once per run: this Mac's memory and the text-model
/// settings.
pub fn ensure_voice_loaded(app: &Entity<AppModel>, cx: &mut App) {
    let first = app.update(cx, |m, _| {
        !std::mem::replace(&mut m.ai.voice.requested, true)
    });
    if !first {
        return;
    }
    let bytes = voice::system_memory_bytes();
    app.update(cx, |m, cx| {
        m.ai.voice.memory_bytes = bytes;
        cx.notify();
    });
    refresh_text_model(app, cx);
    refresh_text_models(app, cx);
}

// ---- the built-in text models (AI-7, provider `embedded`) -------------------------------------

/// Reads `list_text_models` (the AI page opening, a finished download, a change by the CLI).
pub fn refresh_text_models(app: &Entity<AppModel>, cx: &mut App) {
    let Some(client) = client_of(app, cx) else {
        return;
    };
    let app2 = app.clone();
    cx.spawn(async move |cx| {
        let res = client.list_text_models().await;
        cx.update(|cx| {
            let ready = app2.update(cx, |m, cx| {
                // An older daemon without the actions: the list stays empty and the page says
                // plainly that no built-in model is available.
                if let Ok(l) = res {
                    m.builtin.set_list(l);
                }
                cx.notify();
                m.builtin.take_ready_to_use()
            });
            // The download that was waiting for "nothing in use" is there: use it.
            if let Some(id) = ready {
                builtin_call(&app2, &id, BuiltInCall::Select, cx);
            }
        });
    })
    .detach();
}

/// A button of a built-in model row. Only the confirm and resume presses send
/// `download_text_model`; nothing is downloaded before.
pub fn builtin_row_action(app: &Entity<AppModel>, model: &str, action: RowAction, cx: &mut App) {
    let effect = app.update(cx, |m, cx| {
        let e = m.builtin.models.apply(model, &action);
        cx.notify();
        e
    });
    match effect {
        None | Some(Effect::Import) => {}
        Some(Effect::StartDownload) => {
            // Nothing in use: the model that arrives is used at once.
            app.update(cx, |m, _| {
                if m.ai.voice.text.in_use().is_none() {
                    m.builtin.use_when_ready = Some(model.to_owned());
                }
            });
            builtin_start_download(app, model, cx)
        }
        Some(Effect::CancelJob(job)) => builtin_cancel_download(app, job, cx),
        Some(Effect::Select) => builtin_call(app, model, BuiltInCall::Select, cx),
        Some(Effect::StopUsing) => builtin_stop_using(app, cx),
        Some(Effect::Remove) => builtin_call(app, model, BuiltInCall::Remove, cx),
    }
}

/// The report card's "Download the recommended model (size)": the button names the size, so the
/// press is the user's confirmation. The model is selected (and the engine turned on) when the
/// file is there, so the report works without another press.
pub fn builtin_download_recommended(app: &Entity<AppModel>, cx: &mut App) {
    let id = app.read(cx).builtin.offer().map(|(id, _, _)| id);
    let Some(id) = id else {
        return;
    };
    builtin_row_action(app, &id, RowAction::Download, cx);
    builtin_row_action(app, &id, RowAction::ConfirmDownload, cx);
}

/// "Don't use a model": the built-in engine is turned off (`ai.text.provider` = none).
pub fn builtin_stop_using(app: &Entity<AppModel>, cx: &mut App) {
    change(app, cx, |v| v.text.stop_using());
}

enum BuiltInCall {
    Select,
    Remove,
}

fn builtin_start_download(app: &Entity<AppModel>, model: &str, cx: &mut App) {
    let Some(client) = client_of(app, cx) else {
        return;
    };
    let (app2, id) = (app.clone(), model.to_owned());
    cx.spawn(async move |cx| {
        let res = client.download_text_model(&id).await;
        cx.update(|cx| {
            app2.update(cx, |m, cx| {
                match res {
                    Ok(job) => m.builtin.models.download_started(&id, job),
                    Err(e) => m.builtin.models.failed(&id, ErrorNote::from_ai(&e)),
                }
                cx.notify();
            })
        });
    })
    .detach();
}

fn builtin_cancel_download(app: &Entity<AppModel>, job: String, cx: &mut App) {
    let Some(client) = client_of(app, cx) else {
        return;
    };
    let app2 = app.clone();
    cx.spawn(async move |cx| {
        if let Err(e) = client.cancel_job(&job).await {
            cx.update(|cx| {
                app::push_toast(
                    &app2,
                    flows::error_toast_live(
                        || tr!("Couldn\u{2019}t stop the download"),
                        e.message.clone(),
                    ),
                    cx,
                );
            });
        }
        cx.update(|cx| refresh_text_models(&app2, cx));
    })
    .detach();
}

fn builtin_call(app: &Entity<AppModel>, model: &str, what: BuiltInCall, cx: &mut App) {
    let Some(client) = client_of(app, cx) else {
        return;
    };
    let (app2, id) = (app.clone(), model.to_owned());
    cx.spawn(async move |cx| {
        let res = match what {
            BuiltInCall::Select => client.select_text_model(&id).await,
            BuiltInCall::Remove => client.remove_text_model(&id).await,
        };
        cx.update(|cx| {
            if let Err(e) = res {
                app::push_toast(
                    &app2,
                    flows::error_toast_live(
                        || tr!("Couldn\u{2019}t change the text model"),
                        e.message.clone(),
                    ),
                    cx,
                );
            }
            refresh_text_models(&app2, cx);
            // The model the daemon names (and the engine's state) changed with it.
            refresh_text_model(&app2, cx);
        });
    })
    .detach();
}

/// Reads `get_text_model`.
pub fn refresh_text_model(app: &Entity<AppModel>, cx: &mut App) {
    let Some(client) = client_of(app, cx) else {
        return;
    };
    let app2 = app.clone();
    cx.spawn(async move |cx| {
        let res = client.get_text_model().await;
        cx.update(|cx| {
            app2.update(cx, |m, cx| {
                if let Ok(v) = &res {
                    m.ai.voice.text.set_from_daemon(v);
                }
                cx.notify();
            })
        });
    })
    .detach();
}

/// Writes settings (already applied to the page state), then reads the text model back so the
/// page shows what the daemon holds. A refusal comes back as a toast.
fn write_settings(app: &Entity<AppModel>, writes: Vec<(&'static str, Value)>, cx: &mut App) {
    if writes.is_empty() {
        return;
    }
    let backend = app.read(cx).backend.clone();
    let app2 = app.clone();
    cx.spawn(async move |cx| {
        let Some(daemon) = backend.daemon.clone() else {
            return;
        };
        let values: serde_json::Map<String, Value> =
            writes.into_iter().map(|(k, v)| (k.to_owned(), v)).collect();
        let res = backend
            .rt
            .spawn(async move {
                Backend::call(
                    &daemon,
                    "set_settings",
                    serde_json::json!({ "values": values }),
                )
                .await
            })
            .await;
        let failure = match res {
            Ok(Ok(_)) => None,
            Ok(Err(e)) => Some(e),
            Err(e) => Some(e.to_string()),
        };
        cx.update(|cx| {
            if let Some(e) = failure {
                app::push_toast(
                    &app2,
                    flows::error_toast_live(|| tr!("Couldn\u{2019}t save the setting"), e),
                    cx,
                );
            }
            refresh_text_model(&app2, cx);
        });
    })
    .detach();
}

/// Applies `f` to the page state, then writes the settings it returns.
fn change(
    app: &Entity<AppModel>,
    cx: &mut App,
    f: impl FnOnce(&mut voice::VoiceVm) -> Vec<(&'static str, Value)>,
) {
    let writes = app.update(cx, |m, cx| {
        let w = f(&mut m.ai.voice);
        cx.notify();
        w
    });
    write_settings(app, writes, cx);
}

/// The Test button.
pub fn text_test(app: &Entity<AppModel>, cx: &mut App) {
    let gate = app.update(cx, |m, cx| {
        let g = m.ai.voice.text.begin_test();
        if let TestGate::Blocked(msg) = &g {
            m.ai.voice.text.test = TestState::Failed(msg.clone());
        }
        cx.notify();
        g
    });
    if gate == TestGate::Run {
        run_text_test(app, cx);
    }
}

fn run_text_test(app: &Entity<AppModel>, cx: &mut App) {
    let Some(client) = client_of(app, cx) else {
        return;
    };
    let app2 = app.clone();
    cx.spawn(async move |cx| {
        let res = client.test_text_model().await.map_err(|e| e.message);
        cx.update(|cx| {
            app2.update(cx, |m, cx| {
                m.ai.voice.text.test_answer(res);
                cx.notify();
            })
        });
    })
    .detach();
}

/// The "Find speakers after each transcript" switch (needs an installed speaker model).
pub fn set_auto_speakers(app: &Entity<AppModel>, on: bool, cx: &mut App) {
    let writes = app.update(cx, |m, cx| {
        let w = m.ai.voice.speakers.set_auto(on);
        cx.notify();
        w
    });
    for (k, v) in writes {
        app::set_setting(app, k, v, cx);
    }
}

/// The "Your name in transcripts" box (`profile.display_name`; an empty box means "Me").
pub fn set_display_name(app: &Entity<AppModel>, text: &str, cx: &mut App) {
    let writes = app.update(cx, |m, cx| {
        let w = m.ai.voice.speakers.set_display_name(text);
        cx.notify();
        w
    });
    for (k, v) in writes {
        app::set_setting(app, k, v, cx);
    }
}
