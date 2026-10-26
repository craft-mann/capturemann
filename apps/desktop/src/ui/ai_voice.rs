//! The voice part of the AI page (V8, AI-7, AI-8): what the buttons do.
//!
//! Every change goes through `set_settings` as the user, the way `ai.glossary` does: `ai.*` is
//! app-only, so an agent cannot write these keys. The API key goes to the keychain only and is
//! never put in a setting, a toast, the log or the page state. The rules and the state are in
//! `crate::ai::voice`.

use gpui::{App, Entity};
use serde_json::Value;

use cm_i18n::tr;

use crate::ai::voice::{self, ProviderChoice, TestGate, TestState};
use crate::backend::Backend;
use crate::flows;
use crate::ui::ai::client_of;
use crate::ui::app::{self, AppModel};

/// The first reads of the voice section, once per run: this Mac's memory, the text-model
/// settings and (when the provider is On this Mac) the servers on loopback.
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
    refresh_text_model(app, true, cx);
}

/// Reads `get_text_model`; `detect` also looks for Ollama and LM Studio afterwards when the
/// provider is On this Mac.
pub fn refresh_text_model(app: &Entity<AppModel>, detect: bool, cx: &mut App) {
    let Some(client) = client_of(app, cx) else {
        return;
    };
    let app2 = app.clone();
    cx.spawn(async move |cx| {
        let res = client.get_text_model().await;
        cx.update(|cx| {
            let local = app2.update(cx, |m, cx| {
                if let Ok(v) = &res {
                    m.ai.voice.text.set_from_daemon(v);
                }
                cx.notify();
                m.ai.voice.text.provider == ProviderChoice::OnThisMac
            });
            if detect && local {
                detect_servers(&app2, cx);
            }
        });
    })
    .detach();
}

/// Looks for Ollama (11434) and LM Studio (1234) on loopback only (2 s each) and lists their
/// models. Runs only when the page asks for it; never touches another host.
pub fn detect_servers(app: &Entity<AppModel>, cx: &mut App) {
    let backend = app.read(cx).backend.clone();
    app.update(cx, |m, cx| {
        m.ai.voice.text.detecting = true;
        cx.notify();
    });
    let app2 = app.clone();
    cx.spawn(async move |cx| {
        let found = backend
            .rt
            .spawn_blocking(cm_ai::text_model::detect_presets)
            .await
            .map(|v| v.iter().map(voice::PresetVm::from).collect::<Vec<_>>());
        cx.update(|cx| {
            app2.update(cx, |m, cx| {
                match found {
                    Ok(p) => m.ai.voice.text.set_presets(p),
                    Err(_) => m.ai.voice.text.detecting = false,
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
            refresh_text_model(&app2, false, cx);
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

/// A press on None / On this Mac / Remote / Anthropic.
pub fn text_choose_provider(app: &Entity<AppModel>, c: ProviderChoice, cx: &mut App) {
    change(app, cx, |v| v.text.choose_provider(c));
    if c == ProviderChoice::OnThisMac {
        detect_servers(app, cx);
    }
}

pub fn text_choose_preset(app: &Entity<AppModel>, preset: &str, cx: &mut App) {
    change(app, cx, |v| v.text.choose_preset(preset));
}

pub fn text_choose_model(app: &Entity<AppModel>, id: &str, cx: &mut App) {
    change(app, cx, |v| v.text.choose_model(id));
}

pub fn text_toggle_picker(app: &Entity<AppModel>, cx: &mut App) {
    app.update(cx, |m, cx| {
        let t = &mut m.ai.voice.text;
        t.picker_open = !t.picker_open;
        cx.notify();
    });
}

/// The model typed in the Remote or Anthropic box.
pub fn text_set_model(app: &Entity<AppModel>, text: &str, cx: &mut App) {
    change(app, cx, |v| v.text.set_typed_model(text));
}

/// The address typed in the Remote box; a refused address is shown under the box.
pub fn text_set_url(app: &Entity<AppModel>, text: &str, cx: &mut App) {
    change(app, cx, |v| match v.text.set_typed_url(text) {
        Ok(w) => {
            v.text.error = None;
            w
        }
        Err(e) => {
            v.text.error = Some(e);
            Vec::new()
        }
    });
}

/// The Test button: the first-use notice comes first for a new remote host.
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
