//! App-level AI operations: read and change the speech models, the notice, transcription runs and
//! the daemon's AI events. Free functions on `&Entity<AppModel>` like the rest of the app, so the
//! views (the AI page, the details card, the recording editor) stay thin.
//!
//! Nothing here starts a download on its own: [`row_action`] sends `download_model` only for the
//! confirm and resume buttons of a model row, which the user presses.

use cm_i18n::tr;
use gpui::{App, Entity, PathPromptOptions};
use serde_json::Value;

use crate::ai::client::{AiClient, AiError, CaptionFormat, CaptionOptionsInfo, DaemonTransport};
use crate::ai::models::{Effect, RowAction};
use crate::ai::summary::TranscriptState;
use crate::daemon_text::{AppNote, ErrorNote};
use crate::flows;
use crate::toast::{ToastKind, ToastSpec};
use crate::ui::app::{self, AppModel};

/// `ai.auto_transcribe`: transcribe after each recording (read by the daemon).
pub const KEY_AUTO_TRANSCRIBE: &str = "ai.auto_transcribe";
/// `ai.glossary`: the user's names and terms (app-only: agents can read it, not write it).
pub const KEY_GLOSSARY: &str = "ai.glossary";
/// A per-viewer choice, kept in this app's own settings file (agents cannot read or write it).
pub const KEY_NOTICE_DISMISSED: &str = "ai.notice_dismissed";

/// The spec's default for auto-transcribe: on when the machine has 16 GB of RAM or more.
pub fn default_auto_transcribe() -> bool {
    #[cfg(target_os = "macos")]
    {
        let bytes = std::process::Command::new("sysctl")
            .args(["-n", "hw.memsize"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|s| s.trim().parse::<u64>().ok());
        bytes.is_some_and(|b| b >= 16 * 1024 * 1024 * 1024)
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

/// The client of the in-process daemon, `None` while there is no library.
pub fn client_of(app: &Entity<AppModel>, cx: &App) -> Option<AiClient> {
    let backend = app.read(cx).backend.clone();
    let daemon = backend.daemon.clone()?;
    Some(AiClient::new(std::sync::Arc::new(DaemonTransport {
        daemon,
        rt: backend.rt.clone(),
    })))
}

/// What the page says when the AI actions do not exist (an older daemon), else the error.
/// Kept as a note and worded when drawn, so it follows a language switch.
fn unavailable_note(e: &AiError) -> ErrorNote {
    if e.code.is_none() && e.message.to_lowercase().contains("method") {
        ErrorNote::app(AppNote::AiUnavailable)
    } else {
        ErrorNote::from_ai(e)
    }
}

/// Reads the model list (the app does this at start and when the AI page opens).
pub fn refresh(app: &Entity<AppModel>, cx: &mut App) {
    let Some(client) = client_of(app, cx) else {
        return;
    };
    let app2 = app.clone();
    cx.spawn(async move |cx| {
        let res = client.list_models().await;
        cx.update(|cx| {
            app2.update(cx, |m, cx| {
                match res {
                    Ok(l) => m.ai.set_list(l),
                    Err(e) => m.ai.set_load_error(unavailable_note(&e)),
                }
                cx.notify();
            })
        });
    })
    .detach();
}

/// Reads the list once, the first time something needs it.
pub fn ensure_loaded(app: &Entity<AppModel>, cx: &mut App) {
    let needed = {
        let m = app.read(cx);
        m.ai.list.is_none() && m.ai.load_error.is_none()
    };
    if needed {
        refresh(app, cx);
    }
}

fn set_busy(app: &Entity<AppModel>, busy: bool, error: Option<ErrorNote>, cx: &mut App) {
    app.update(cx, |m, cx| {
        m.ai.busy = busy;
        if !busy || error.is_some() {
            m.ai.action_error = error;
        }
        cx.notify();
    });
}

/// A button of a model row was pressed.
pub fn row_action(app: &Entity<AppModel>, model: &str, action: RowAction, cx: &mut App) {
    let effect = app.update(cx, |m, cx| {
        let e = m.ai.apply(model, &action);
        cx.notify();
        e
    });
    match effect {
        None => {}
        // The only path that sends `download_model`: the second press, by the user.
        Some(Effect::StartDownload) => start_download(app, model, cx),
        Some(Effect::CancelJob(job)) => cancel_download(app, job, cx),
        Some(Effect::Import) => import_model(app, model, cx),
        Some(Effect::Select) => select_model(app, model, cx),
        Some(Effect::Remove) => remove_model(app, model, cx),
        // Only the built-in text models have it (their own row handler).
        Some(Effect::StopUsing) => {}
    }
}

fn start_download(app: &Entity<AppModel>, model: &str, cx: &mut App) {
    let Some(client) = client_of(app, cx) else {
        return;
    };
    let (app2, id) = (app.clone(), model.to_owned());
    cx.spawn(async move |cx| {
        let res = client.download_model(&id).await;
        cx.update(|cx| {
            app2.update(cx, |m, cx| {
                match res {
                    Ok(job) => m.ai.download_started(&id, job),
                    Err(e) => m.ai.failed(&id, ErrorNote::from_ai(&e)),
                }
                cx.notify();
            })
        });
    })
    .detach();
}

fn cancel_download(app: &Entity<AppModel>, job: String, cx: &mut App) {
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
        cx.update(|cx| refresh(&app2, cx));
    })
    .detach();
}

fn import_model(app: &Entity<AppModel>, model: &str, cx: &mut App) {
    let Some(client) = client_of(app, cx) else {
        return;
    };
    let rx = cx.prompt_for_paths(PathPromptOptions {
        files: true,
        directories: false,
        multiple: false,
        prompt: Some(tr!("Choose a model file").into()),
    });
    let (app2, id) = (app.clone(), model.to_owned());
    cx.spawn(async move |cx| {
        let Ok(Ok(Some(paths))) = rx.await else {
            return;
        };
        let Some(path) = paths.first().map(|p| p.display().to_string()) else {
            return;
        };
        cx.update(|cx| set_busy(&app2, true, None, cx));
        let res = client.import_model(&id, &path).await;
        cx.update(|cx| {
            set_busy(&app2, false, res.as_ref().err().map(ErrorNote::from_ai), cx);
            refresh(&app2, cx);
        });
    })
    .detach();
}

fn select_model(app: &Entity<AppModel>, model: &str, cx: &mut App) {
    let Some(client) = client_of(app, cx) else {
        return;
    };
    set_busy(app, true, None, cx);
    let (app2, id) = (app.clone(), model.to_owned());
    cx.spawn(async move |cx| {
        let res = client.select_model(&id).await;
        cx.update(|cx| {
            set_busy(&app2, false, res.as_ref().err().map(ErrorNote::from_ai), cx);
            refresh(&app2, cx);
        });
    })
    .detach();
}

fn remove_model(app: &Entity<AppModel>, model: &str, cx: &mut App) {
    let Some(client) = client_of(app, cx) else {
        return;
    };
    set_busy(app, true, None, cx);
    let (app2, id) = (app.clone(), model.to_owned());
    cx.spawn(async move |cx| {
        let res = client.remove_model(&id).await;
        cx.update(|cx| {
            set_busy(&app2, false, res.as_ref().err().map(ErrorNote::from_ai), cx);
            refresh(&app2, cx);
        });
    })
    .detach();
}

/// Reads the transcript summary of a recording when the details page needs it (never asked, or
/// something may have changed it since). The page keeps showing the previous answer meanwhile.
pub fn ensure_transcript_info(app: &Entity<AppModel>, capture_id: &str, cx: &mut App) {
    let epoch = app.read(cx).transcribe.info_epoch;
    let go = app.update(cx, |m, _| {
        let go = m.transcribe.transcripts.needs_load(capture_id, epoch);
        if go {
            m.transcribe.transcripts.begin(capture_id, epoch);
        }
        go
    });
    if !go {
        return;
    }
    let id = capture_id.to_owned();
    let Some(client) = client_of(app, cx) else {
        app.update(cx, |m, cx| {
            m.transcribe
                .transcripts
                .set(&id, epoch, TranscriptState::Missing);
            cx.notify();
        });
        return;
    };
    let app2 = app.clone();
    cx.spawn(async move |cx| {
        let res = client.transcript_info(&id).await;
        cx.update(|cx| {
            app2.update(cx, |m, cx| {
                let state = match res {
                    Ok(Some(info)) => TranscriptState::Present(info),
                    Ok(None) => TranscriptState::Missing,
                    Err(e) => TranscriptState::Failed(unavailable_note(&e)),
                };
                m.transcribe.transcripts.set(&id, epoch, state);
                cx.notify();
            });
        });
    })
    .detach();
}

/// Re-generate pressed: opens the panel (model and language for this run) under the transcript.
pub fn open_regen(app: &Entity<AppModel>, capture_id: &str, cx: &mut App) {
    app.update(cx, |m, cx| {
        m.transcribe.open_regen(capture_id);
        cx.notify();
    });
}

pub fn close_regen(app: &Entity<AppModel>, capture_id: &str, cx: &mut App) {
    app.update(cx, |m, cx| {
        m.transcribe.close_regen(capture_id);
        cx.notify();
    });
}

/// A choice in the Re-generate panel: `None` model = the active one.
pub fn set_regen_model(
    app: &Entity<AppModel>,
    capture_id: &str,
    model: Option<String>,
    cx: &mut App,
) {
    app.update(cx, |m, cx| {
        if let Some(d) = m.transcribe.regen_mut(capture_id) {
            d.model = model;
        }
        cx.notify();
    });
}

pub fn set_regen_language(app: &Entity<AppModel>, capture_id: &str, code: &str, cx: &mut App) {
    app.update(cx, |m, cx| {
        if let Some(d) = m.transcribe.regen_mut(capture_id) {
            d.language = code.to_owned();
        }
        cx.notify();
    });
}

/// The Start button of the Re-generate panel (the user has read what it replaces).
pub fn start_regen(app: &Entity<AppModel>, capture_id: &str, cx: &mut App) {
    let Some(draft) = app.read(cx).transcribe.regen_of(capture_id).cloned() else {
        return;
    };
    transcribe_with(
        app,
        capture_id,
        draft.language_arg(),
        draft.model.as_deref(),
        cx,
    );
}

/// Download the transcript (TXT, SRT or VTT): the daemon writes the file into the recording's
/// `exports/` folder and the toast says where.
pub fn export_captions(
    app: &Entity<AppModel>,
    capture_id: &str,
    format: CaptionFormat,
    cx: &mut App,
) {
    let timestamps = app.read(cx).transcribe.txt_timestamps;
    let Some(client) = client_of(app, cx) else {
        return;
    };
    let (app2, id) = (app.clone(), capture_id.to_owned());
    cx.spawn(async move |cx| {
        let res = client.export_captions(&id, format, timestamps).await;
        cx.update(|cx| {
            let spec = match res {
                Ok(path) => ToastSpec::new(
                    ToastKind::Success,
                    tr!("{format} saved", format = format.label()),
                )
                .body(path.display().to_string()),
                Err(e) => flows::error_toast_live(
                    || tr!("Couldn\u{2019}t save the transcript file"),
                    e.message.clone(),
                ),
            };
            app::push_toast(&app2, spec, cx);
        });
    })
    .detach();
}

/// The "Burn in captions" tick beside the Export button.
pub fn set_export_burn(app: &Entity<AppModel>, capture_id: &str, on: bool, cx: &mut App) {
    app.update(cx, |m, cx| {
        m.transcribe.set_export_burn(capture_id, on);
        cx.notify();
    });
}

/// A caption switch on the details page (`enabled`: show in the editor's preview, `burn_in`:
/// burn into exported videos by default). The switch moves at once; the daemon records one
/// undoable revision, and the page goes back with a message if it refuses.
pub fn set_caption_options(
    app: &Entity<AppModel>,
    capture_id: &str,
    enabled: Option<bool>,
    burn_in: Option<bool>,
    cx: &mut App,
) {
    let Some(client) = client_of(app, cx) else {
        return;
    };
    let id = capture_id.to_owned();
    let before = app.update(cx, |m, cx| {
        let now = m.transcribe.transcripts.info(&id).map(|i| i.options)?;
        let want = CaptionOptionsInfo {
            enabled: enabled.unwrap_or(now.enabled),
            burn_in: burn_in.unwrap_or(now.burn_in),
        };
        let before = m.transcribe.transcripts.set_options(&id, want);
        // Changing the recording's own setting makes it the export default again.
        if burn_in.is_some() {
            m.transcribe.clear_export_burn(&id);
        }
        cx.notify();
        before
    });
    let Some(before) = before else {
        return;
    };
    let app2 = app.clone();
    cx.spawn(async move |cx| {
        let res = client.set_caption_options(&id, enabled, burn_in).await;
        cx.update(|cx| {
            match res {
                Ok(now) => app2.update(cx, |m, cx| {
                    m.transcribe.transcripts.set_options(&id, now);
                    cx.notify();
                }),
                Err(e) => {
                    app2.update(cx, |m, cx| {
                        m.transcribe.transcripts.set_options(&id, before);
                        cx.notify();
                    });
                    app::push_toast(
                        &app2,
                        flows::error_toast_live(
                            || tr!("Couldn\u{2019}t change the captions"),
                            e.message.clone(),
                        ),
                        cx,
                    );
                }
            };
        });
    })
    .detach();
}

/// "Open captions": the recording editor, scrolled to its Captions card.
pub fn open_captions(app: &Entity<AppModel>, capture_id: &str, cx: &mut App) {
    app.update(cx, |m, _| {
        m.transcribe.captions_focus = Some(capture_id.to_owned());
    });
    app::set_route(app, app::Route::RecordingEditor(capture_id.to_owned()), cx);
}

/// "Set up report" (the toast after a transcript): the recording editor on the Report tab's
/// setup.
pub fn open_report_setup(app: &Entity<AppModel>, capture_id: &str, cx: &mut App) {
    app.update(cx, |m, _| {
        m.transcribe.report_focus = Some(capture_id.to_owned());
    });
    app::set_route(app, app::Route::RecordingEditor(capture_id.to_owned()), cx);
}

/// "Export audio only" (the details page, the Output tab): the recording editor with the Export
/// audio sheet open (VOC-2).
pub fn open_export_audio(app: &Entity<AppModel>, capture_id: &str, cx: &mut App) {
    app.update(cx, |m, _| {
        m.transcribe.export_audio_focus = Some(capture_id.to_owned());
    });
    app::set_route(app, app::Route::RecordingEditor(capture_id.to_owned()), cx);
}

/// The Timestamps switch of the TXT download.
pub fn set_txt_timestamps(app: &Entity<AppModel>, on: bool, cx: &mut App) {
    app.update(cx, |m, cx| {
        m.transcribe.txt_timestamps = on;
        cx.notify();
    });
}

/// The `ai.auto_transcribe` switch: shown at once, saved through `set_settings`.
pub fn set_auto_transcribe(app: &Entity<AppModel>, on: bool, cx: &mut App) {
    app.update(cx, |m, cx| {
        m.ai.auto_transcribe = on;
        cx.notify();
    });
    app::set_setting(app, KEY_AUTO_TRANSCRIBE, Value::Bool(on), cx);
}

/// Saves the glossary: the page shows the new list at once and the daemon's settings follow (the
/// next transcription uses it). The list is cleaned the way the daemon reads it.
pub fn set_glossary(
    app: &Entity<AppModel>,
    entries: Vec<cm_ai::glossary::GlossaryEntry>,
    cx: &mut App,
) {
    app.update(cx, |m, cx| {
        m.ai.glossary = entries.clone();
        cx.notify();
    });
    let value = serde_json::to_value(&entries).unwrap_or(Value::Array(Vec::new()));
    app::set_setting(app, KEY_GLOSSARY, value, cx);
}

/// "Dismiss" on a notice: hidden here and in the library, remembered in this app's settings. The
/// hint beside a disabled Transcribe button stays (it explains the button).
pub fn dismiss_notice(app: &Entity<AppModel>, cx: &mut App) {
    let backend = app.read(cx).backend.clone();
    if let Err(e) = backend
        .settings
        .set_internal(KEY_NOTICE_DISMISSED, Value::Bool(true))
    {
        log::warn!("could not remember the dismissed AI notice: {e}");
    }
    app.update(cx, |m, cx| {
        m.ai.notice_dismissed = true;
        cx.notify();
    });
}

/// Transcribe pressed (the details card, the recording editor).
pub fn transcribe(app: &Entity<AppModel>, capture_id: &str, cx: &mut App) {
    transcribe_with(app, capture_id, None, None, cx);
}

/// Transcribe with a language (`None` = auto-detect) and a model (`None` = the active one). Also
/// the re-generate of a recording that already has a transcript: the daemon replaces it.
pub fn transcribe_with(
    app: &Entity<AppModel>,
    capture_id: &str,
    language: Option<&str>,
    model: Option<&str>,
    cx: &mut App,
) {
    let (language, model) = (language.map(str::to_owned), model.map(str::to_owned));
    let go = app.update(cx, |m, cx| {
        let go = m.ai.can_transcribe() && m.transcribe.start(capture_id);
        cx.notify();
        go
    });
    let (true, Some(client)) = (go, client_of(app, cx)) else {
        return;
    };
    let (app2, id) = (app.clone(), capture_id.to_owned());
    cx.spawn(async move |cx| {
        let res = client
            .transcribe(&id, language.as_deref(), model.as_deref())
            .await;
        cx.update(|cx| match res {
            Ok(job) => app2.update(cx, |m, cx| {
                m.transcribe.started(&id, job);
                cx.notify();
            }),
            Err(e) if e.is_no_model() => {
                // The list was stale: read it again; the hint beside the button takes over.
                app2.update(cx, |m, cx| {
                    m.transcribe.ready(&id);
                    cx.notify();
                });
                refresh(&app2, cx);
            }
            Err(e) => {
                app2.update(cx, |m, cx| {
                    m.transcribe.failed(&id, ErrorNote::from_ai(&e));
                    cx.notify();
                });
                app::push_toast(
                    &app2,
                    flows::error_toast_live(
                        || tr!("Couldn\u{2019}t start the transcription"),
                        e.message.clone(),
                    ),
                    cx,
                );
            }
        });
    })
    .detach();
}

/// The daemon's AI events: model download progress and list changes, transcripts, and the job
/// events of downloads and transcriptions this window started.
pub fn on_event(app: &Entity<AppModel>, ev: &cm_daemon::events::Event, cx: &mut App) {
    let data = &ev.data;
    let str_of = |k: &str| data.get(k).and_then(Value::as_str).map(str::to_owned);
    match ev.name.as_str() {
        "model.download.progress" => {
            let (Some(model), Some(done), Some(total)) = (
                str_of("model"),
                data.get("bytes_done").and_then(Value::as_u64),
                data.get("bytes_total").and_then(Value::as_u64),
            ) else {
                return;
            };
            let text = str_of("kind").as_deref() == Some("text");
            app.update(cx, |m, cx| {
                // A built-in text model (AI-7) has its own list; the ids never mix.
                if text {
                    m.builtin.models.progress(&model, done, total);
                } else {
                    m.ai.progress(&model, done, total);
                }
                cx.notify();
            });
        }
        "model.changed" => {
            if str_of("kind").as_deref() == Some("text") {
                crate::ui::ai_voice::refresh_text_models(app, cx);
                crate::ui::ai_voice::refresh_text_model(app, cx);
            } else {
                refresh(app, cx);
            }
        }
        // Edited captions, an undo, a re-generate that cleared edits: the details page's summary
        // ("edited" or not) is read again.
        "capture.revised" => {
            app.update(cx, |m, cx| {
                m.transcribe.bump_info();
                cx.notify();
            });
        }
        // The automatic transcription after a recording: the Transcript and Speakers tabs show it
        // running like one started by a click (a long recording takes minutes).
        "transcript.started" => {
            if let (Some(id), Some(job)) = (str_of("capture_id"), str_of("job_id")) {
                app.update(cx, |m, cx| {
                    m.transcribe.start(&id);
                    m.transcribe.started(&id, job);
                    cx.notify();
                });
            }
        }
        "transcript.ready" => {
            if let Some(id) = str_of("capture_id") {
                app.update(cx, |m, cx| {
                    m.transcribe.ready(&id);
                    cx.notify();
                });
                // A re-generate says what became of the caption edits (AI-6a).
                let count = |k: &str| data.get(k).and_then(Value::as_u64).unwrap_or(0) as usize;
                let kept =
                    crate::ai::summary::kept_edits_note(count("reapplied"), count("dropped"));
                app::push_toast(
                    app,
                    ToastSpec::new(ToastKind::Success, tr!("Transcript ready")).body(
                        kept.unwrap_or_else(|| tr!("Open the recording to edit its captions.")),
                    ),
                    cx,
                );
                crate::ui::report_offer::offer(app, &id, cx);
            }
        }
        // AI-5: a translation was written (the audio editor's language switcher reads it again).
        "translation.ready" => {
            let language = str_of("language").unwrap_or_default();
            app.update(cx, |m, cx| {
                m.transcribe.voice_epoch += 1;
                cx.notify();
            });
            app::push_toast(
                app,
                ToastSpec::new(
                    ToastKind::Success,
                    tr!(
                        "Translation ready: {language}",
                        language = crate::recedit::voice::language_name(&language)
                    ),
                ),
                cx,
            );
        }
        // AI-8, the automatic run after a transcript: the Speakers tab shows it like a click.
        "speakers.started" => {
            if let (Some(id), Some(job)) = (str_of("capture_id"), str_of("job_id")) {
                app.update(cx, |m, cx| {
                    use crate::ai::jobs::VoiceJobKind::Speakers;
                    m.transcribe.voice_job_begin(&id, Speakers);
                    m.transcribe.voice_job_adopt(&id, Speakers, &job);
                    cx.notify();
                });
            }
        }
        // AI-8 and AI-9 run by an agent or the CLI: the open editor reads the bundle again.
        "speakers.ready" | "report.ready" => {
            let kind = if ev.name == "speakers.ready" {
                crate::ai::jobs::VoiceJobKind::Speakers
            } else {
                crate::ai::jobs::VoiceJobKind::Report
            };
            let id = str_of("capture_id");
            app.update(cx, |m, cx| {
                match &id {
                    Some(id) => m.transcribe.voice_ready(id, kind),
                    None => m.transcribe.voice_epoch += 1,
                }
                cx.notify();
            });
        }
        "job.progress" => {
            if let (Some(job), Some(p)) = (
                str_of("job_id"),
                data.get("progress").and_then(Value::as_f64),
            ) {
                app.update(cx, |m, cx| {
                    if m.transcribe.voice_progress_by_job(&job, p)
                        || m.transcribe.progress_by_job(&job, p)
                    {
                        cx.notify();
                    }
                });
            }
        }
        "job.done" => {
            if let Some(job) = str_of("job_id") {
                app.update(cx, |m, cx| {
                    if !m.transcribe.voice_ended_by_job(&job, Ok(())) {
                        m.transcribe.done_by_job(&job);
                    }
                    cx.notify();
                });
                // A finished download turns into an installed model.
                refresh(app, cx);
                crate::ui::ai_voice::refresh_text_models(app, cx);
                crate::ui::ai_voice::refresh_text_model(app, cx);
            }
        }
        "job.failed" => {
            if let Some(job) = str_of("job_id") {
                let message = data
                    .get("error")
                    // The daemon sends `{ code, reason }`; `message` is kept for other senders.
                    .and_then(|e| e.get("reason").or_else(|| e.get("message")))
                    .and_then(Value::as_str)
                    .map_or_else(|| tr!("The job failed"), str::to_owned);
                let reason = data
                    .get("error")
                    .and_then(|e| e.get("reason"))
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .unwrap_or_else(|| message.clone());
                app.update(cx, |m, cx| {
                    if m.transcribe.voice_ended_by_job(&job, Err(reason)) {
                        cx.notify();
                        return;
                    }
                    m.transcribe.failed_by_job(&job, &message);
                    let model = m
                        .ai
                        .downloads
                        .iter()
                        .find(|(_, d)| {
                            matches!(d, crate::ai::models::DownloadState::Running { job_id: Some(j), .. } if *j == job)
                        })
                        .map(|(id, _)| id.clone());
                    if let Some(id) = model {
                        m.ai.failed(&id, message.clone());
                    }
                    let text_model = m
                        .builtin
                        .models
                        .downloads
                        .iter()
                        .find(|(_, d)| {
                            matches!(d, crate::ai::models::DownloadState::Running { job_id: Some(j), .. } if *j == job)
                        })
                        .map(|(id, _)| id.clone());
                    if let Some(id) = text_model {
                        m.builtin.models.failed(&id, message.clone());
                    }
                    cx.notify();
                });
            }
        }
        _ => {}
    }
}
