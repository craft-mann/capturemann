//! Running Quick Capture (QC-1, QC-3, QC-4): what happens once the panel is closed or a preset's
//! shortcut is pressed, and what happens after a screenshot. Everything goes through the
//! existing actions and capture flows; the app changes no state itself (CLAUDE.md: one action
//! bus). What a call cannot carry (Auto-redact, the editor and its tool, the folder, the format
//! and scale, transcribing) is done here afterwards through `suggest_redactions`, `annotate`,
//! `transcribe` and the library's own copy paths.

use std::time::Duration;

use cm_core::annotation::{AnnotationOp, OpKind};
use cm_core::preset::{Preset, PresetKind, PresetTarget};
use cm_i18n::{tr, tr_ctx};
use gpui::{App, Entity};
use serde_json::{json, Map, Value};

use crate::backend::{AfterCapture, Backend, CopyResult};
use crate::overlay::CaptureMode;
use crate::quick::run::{AudioRun, Run, ShotRun, VideoRun};
use crate::quick::{Area, QuickCtx};
use crate::recording::start::{quick_defaults, SheetDefaults, Target};
use crate::toast::{ToastAction, ToastKind, ToastSpec};
use crate::ui::app::{self, AppModel, Command, Route};

// ---- notes from the capture flows -----------------------------------------------------------

/// The overlay told where it captured (a region or a display).
pub fn note_area(app: &Entity<AppModel>, display_id: u32, rect: Option<[i64; 4]>, cx: &mut App) {
    app.update(cx, |m, _| {
        if let Some(c) = &mut m.quick.ctx {
            c.area = Some(match rect {
                Some(rect) => Area::Region { display_id, rect },
                None => Area::Display {
                    display_id: Some(display_id),
                },
            });
        }
    });
}

/// A window chosen in the window list is being captured.
pub fn note_window(app: &Entity<AppModel>, window_id: u32, cx: &mut App) {
    app.update(cx, |m, _| {
        if let Some(c) = &mut m.quick.ctx {
            c.area = Some(Area::Window { window_id });
        }
    });
}

// ---- starting a run -------------------------------------------------------------------------

/// Starts `run`. The panel is already closed and its delay has passed.
pub fn execute(app: &Entity<AppModel>, run: Run, preset_name: Option<String>, cx: &mut App) {
    match run {
        Run::Shot(r) => shot(app, r, preset_name, cx),
        Run::Video(v) => video(app, v, cx),
        Run::Audio(a) => audio(app, a, cx),
    }
}

fn shot(app: &Entity<AppModel>, r: ShotRun, preset_name: Option<String>, cx: &mut App) {
    if app.read(cx).capture_busy {
        return; // CAP-2: one capture at a time.
    }
    if r.target == PresetTarget::LongShot {
        // The overlay picks the area and the scroll options; the run's choices wait for the end.
        app.update(cx, |m, _| {
            m.quick.ctx = None;
            m.copy_override = Some(r.copy && !r.redact);
            m.quick.long_run = Some(r);
        });
        crate::ui::long_shot::start(app, cx);
        return;
    }
    let mode = match r.target {
        PresetTarget::Window => CaptureMode::Window,
        PresetTarget::Display => CaptureMode::Display,
        _ => CaptureMode::Region,
    };
    let area = (mode == CaptureMode::Display).then_some(Area::Display { display_id: None });
    app.update(cx, |m, _| {
        m.copy_override = None;
        m.quick.ctx = Some(QuickCtx {
            run: r,
            area,
            preset_name,
        });
    });
    app::start_capture_inner(app, mode, cx);
}

fn video(app: &Entity<AppModel>, v: VideoRun, cx: &mut App) {
    let backend = app.read(cx).backend.clone();
    if crate::ui::recording::refuse_recording(app, &backend, cx)
        || crate::ui::recording::is_active(app, cx)
    {
        return;
    }
    let app2 = app.clone();
    match v.target {
        PresetTarget::Display | PresetTarget::LongShot => {
            let Some(daemon) = backend.daemon.clone() else {
                return;
            };
            cx.spawn(async move |cx| {
                let listed = backend
                    .rt
                    .spawn(async move {
                        Backend::call_coded(&daemon, "list_targets", json!({ "windows": false }))
                            .await
                    })
                    .await;
                cx.update(|cx| {
                    let displays = match listed {
                        Ok(Ok(v)) => crate::recording::sheet::parse_displays(&v),
                        _ => Vec::new(),
                    };
                    match displays.iter().find(|d| d.primary).or(displays.first()) {
                        Some(d) => start_video(&app2, &v, Target::Display { display_id: d.id }, cx),
                        None => app::push_toast(
                            &app2,
                            ToastSpec::new(ToastKind::Error, tr!("Recording didn\u{2019}t start"))
                                .body(tr!("No display was found.")),
                            cx,
                        ),
                    }
                });
            })
            .detach();
        }
        PresetTarget::Region | PresetTarget::Window => {
            let mode = if v.target == PresetTarget::Window {
                CaptureMode::Window
            } else {
                CaptureMode::Region
            };
            cx.spawn(async move |cx| {
                let b = backend.clone();
                let picked = backend
                    .rt
                    .spawn(async move { b.overlay.pick(mode).await })
                    .await;
                cx.update(|cx| match picked {
                    Ok(Ok(Some(p))) => start_video(&app2, &v, p.target(), cx),
                    Ok(Ok(None)) => {}
                    Ok(Err(e)) => app::push_toast(&app2, crate::flows::overlay_error_toast(&e), cx),
                    Err(e) => app::push_toast(
                        &app2,
                        crate::flows::error_toast_live(
                            || tr!("Couldn\u{2019}t choose what to record"),
                            e.to_string(),
                        ),
                        cx,
                    ),
                });
            })
            .detach();
        }
    }
}

/// The recording of a Video run once the target is known (the Record sheet is skipped: the
/// panel's Record button is the confirmation, QC-1).
pub fn start_video(app: &Entity<AppModel>, v: &VideoRun, target: Target, cx: &mut App) {
    let backend = app.read(cx).backend.clone();
    app::refresh_permissions(app, cx);
    let (defaults, dropped) = {
        let m = app.read(cx);
        let mut d = SheetDefaults::from_settings(&backend.settings.snapshot());
        d.mic = v.mic;
        d.system_audio = v.system_audio;
        d.camera = v.webcam;
        quick_defaults(
            &d,
            m.perms.state(crate::permissions::Id::Microphone),
            m.perms.state(crate::permissions::Id::Camera),
        )
    };
    let mut extra = Map::new();
    extra.insert("countdown_s".into(), json!(v.countdown_s));
    if let Some(f) = v.fps {
        extra.insert("fps".into(), json!(f));
    }
    if let Some(c) = v.cursor {
        extra.insert("cursor".into(), json!(c));
    }
    crate::ui::recording::start_quick(app, defaults.options(target), extra, cx);
    if let Some(spec) = crate::ui::recording::dropped_sources_toast(&dropped) {
        app::push_toast(app, spec, cx);
    }
}

fn audio(app: &Entity<AppModel>, a: AudioRun, cx: &mut App) {
    use crate::recording::audio_sheet::{start_params, AudioDefaults, AudioSheetModel};
    let backend = app.read(cx).backend.clone();
    if crate::ui::recording::refuse_recording(app, &backend, cx)
        || crate::ui::recording::is_active(app, cx)
    {
        return;
    }
    let defaults = AudioDefaults::from_settings(&backend.settings.snapshot());
    let mut params = start_params(
        defaults.preset,
        a.mic,
        defaults.mic_device.as_deref(),
        a.system_audio,
        None,
    );
    if !a.mic && !a.system_audio {
        // Nothing was left on: record the microphone, the one source every preset has.
        params["mic"] = json!(true);
    }
    // The panel's own tiles only: Audio has no Countdown tile, so Settings' countdown is not used.
    let params = crate::quick::run::audio_panel_params(params);
    app.update(cx, |m, _| m.quick.transcribe_audio = a.transcribe);
    crate::ui::recording::start_audio(app, params, Some(Map::new()), cx);
    if defaults.consent_notice {
        app::push_toast(
            app,
            ToastSpec::new(ToastKind::Info, tr!("Recording audio"))
                .body(AudioSheetModel::consent_text()),
            cx,
        );
    }
    if a.markers {
        let keys = {
            let m = app.read(cx);
            m.shortcuts
                .get(crate::shortcuts::ShortcutAction::AddMarker)
                .keycaps(m.platform)
        };
        app::push_toast(
            app,
            ToastSpec::new(ToastKind::Info, tr!("Markers are on"))
                .hint(tr_ctx!("keyboard shortcut hint", "Add a marker"), keys),
            cx,
        );
    }
}

/// The audio recording that started from the panel with Transcribe on has ended: starts the
/// transcription of its capture (the `transcribe` action).
pub fn audio_ended(app: &Entity<AppModel>, capture_id: &str, cx: &mut App) {
    let on = app.update(cx, |m, _| std::mem::take(&mut m.quick.transcribe_audio));
    if !on {
        return;
    }
    let backend = app.read(cx).backend.clone();
    let Some(daemon) = backend.daemon.clone() else {
        return;
    };
    let (app2, id) = (app.clone(), capture_id.to_owned());
    cx.spawn(async move |cx| {
        let res = backend
            .rt
            .spawn(async move {
                Backend::call_coded(&daemon, "transcribe", json!({ "capture_id": id })).await
            })
            .await;
        cx.update(|cx| match res {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => app::push_toast(
                &app2,
                crate::flows::error_toast_live(
                    || tr!("Couldn\u{2019}t start the transcript"),
                    crate::daemon_text::call_error_text(&e),
                ),
                cx,
            ),
            Err(_) => {}
        });
    })
    .detach();
}

/// A Long Shot ended (or did not): the steps of a run that came from the panel or a preset are
/// done on its capture, and a run that ended without one is forgotten.
pub fn long_finished(app: &Entity<AppModel>, done: Option<&AfterCapture>, cx: &mut App) {
    let run = app.update(cx, |m, _| m.quick.long_run.take());
    let (Some(run), Some(done)) = (run, done) else {
        return;
    };
    if !run.has_steps() {
        return;
    }
    let backend = app.read(cx).backend.clone();
    let (app2, id) = (app.clone(), done.capture_id.clone());
    cx.spawn(async move |cx| {
        let (b, r, i) = (backend.clone(), run.clone(), id.clone());
        let notes = backend
            .rt
            .spawn(async move { run_steps(&b, &r, &i).await })
            .await
            .unwrap_or_default();
        cx.update(|cx| {
            app2.update(cx, |m, cx| {
                m.library_epoch += 1;
                cx.notify();
            });
            for n in &notes {
                if let Some(spec) = note_toast(n) {
                    app::push_toast(&app2, spec, cx);
                }
            }
            if run.editor {
                if let Some(tool) = &run.tool {
                    app2.update(cx, |m, _| {
                        m.quick.editor_tool = Some((id.clone(), tool.clone()))
                    });
                }
                app::navigate(&app2, app::open_route(&id), cx);
            }
        });
    })
    .detach();
}

/// QC-4.AC5: runs the preset `id_or_name` (its shortcut, the Launcher or the tray). The call is
/// read the way an agent's `preset` argument is, by the daemon (QC-5), then run here.
pub fn run_preset(app: &Entity<AppModel>, id_or_name: &str, cx: &mut App) {
    let backend = app.read(cx).backend.clone();
    let Some(daemon) = backend.daemon.clone() else {
        return;
    };
    let found = daemon
        .preset_store()
        .map(|s| s.find(id_or_name))
        .unwrap_or_else(|| Err(cm_core::preset::unknown_preset(id_or_name)));
    let preset = match found {
        Ok(p) => p,
        Err(e) => {
            app::push_toast(
                app,
                ToastSpec::new(ToastKind::Warning, tr!("That preset is gone")).body(e.to_string()),
                cx,
            );
            return;
        }
    };
    let method = if preset.kind == PresetKind::Screenshot {
        "capture_screenshot"
    } else {
        "start_recording"
    };
    match daemon.resolve_preset_params(method, &json!({ "preset": preset.id })) {
        Ok(resolved) => {
            let run = crate::quick::run::run_from_resolved(&preset, &resolved);
            // The preset's picked state is the panel's from now on.
            app.update(cx, |m, _| m.quick.panel.apply_preset(&preset));
            let (name, app2) = (preset.name, app.clone());
            // The tray menu or the Launcher is still closing: the same wait as the panel.
            cx.spawn(async move |cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(
                        crate::launcher::commands::CAPTURE_DELAY_MS,
                    ))
                    .await;
                cx.update(|cx| execute(&app2, run, Some(name), cx));
            })
            .detach();
        }
        Err(e) => app::push_toast(
            app,
            ToastSpec::new(ToastKind::Warning, tr!("That preset can\u{2019}t run"))
                .body(e.to_string()),
            cx,
        ),
    }
}

// ---- after a screenshot (QC-3, QC-4) ---------------------------------------------------------

/// What a step after the capture did, for the toast.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepNote {
    Blurred(usize),
    RedactFailed(String),
    SavedTo(String),
    SaveFailed { folder: String, reason: String },
}

/// The steps a call cannot carry, on the stored capture: Auto-redact (then the copy, which waited
/// for it), the preset's folder. Never fails the run: each outcome is a note.
pub async fn run_steps(b: &Backend, run: &ShotRun, capture_id: &str) -> Vec<StepNote> {
    let mut notes = Vec::new();
    let Some(daemon) = b.daemon.clone() else {
        return notes;
    };
    if run.redact {
        match redact(&daemon, capture_id).await {
            Ok(n) => notes.push(StepNote::Blurred(n)),
            Err(e) => notes.push(StepNote::RedactFailed(e)),
        }
        // The copy waited for the blur: what is on the clipboard is the redacted picture.
        if run.copy {
            if let Err(e) = b.copy_capture(capture_id).await {
                log::warn!("quick capture: the copy after Auto-redact failed: {e}");
            }
        }
        // The Settings folder copy, if it is on, is made again from the redacted picture.
        let _ = b.save_capture_to_folder(capture_id).await;
    }
    if let Some(template) = &run.save_to {
        let date = crate::save_folder::local_now().date();
        let dir = crate::quick::files::resolve_dir(template, date, dirs::home_dir().as_deref());
        notes.push(match save_to(&daemon, &dir, run, capture_id).await {
            Ok(p) => StepNote::SavedTo(p),
            Err(reason) => StepNote::SaveFailed {
                folder: dir.display().to_string(),
                reason,
            },
        });
    }
    notes
}

/// Auto-redact on a stored capture: the daemon proposes the areas and they are blurred in one
/// `annotate` (one undo step). Returns how many areas were blurred.
pub async fn redact(daemon: &cm_daemon::Daemon, capture_id: &str) -> Result<usize, String> {
    let v = Backend::call(
        daemon,
        "suggest_redactions",
        crate::editor::redact::request(capture_id, &crate::editor::redact::KindChoice::default()),
    )
    .await?;
    let proposals = crate::editor::redact::parse_proposals(&v)?;
    if proposals.is_empty() {
        return Ok(0);
    }
    let ops: Vec<Value> = proposals
        .iter()
        .map(|p| {
            serde_json::to_value(AnnotationOp::new(
                "",
                OpKind::Blur {
                    rect: p.rect,
                    strength: crate::editor::redact::MIN_STRENGTH,
                },
            ))
            .unwrap_or(Value::Null)
        })
        .collect();
    Backend::call(
        daemon,
        "annotate",
        json!({ "capture_id": capture_id, "ops": ops, "replace": false, "return_image": false }),
    )
    .await?;
    Ok(proposals.len())
}

/// QC-4.AC7: writes the rendered picture of the capture into `dir` (made if it is not there), in
/// the preset's format and scale. The capture itself stays in the library.
pub async fn save_to(
    daemon: &cm_daemon::Daemon,
    dir: &std::path::Path,
    run: &ShotRun,
    capture_id: &str,
) -> Result<String, String> {
    let (_, png) = Backend::capture_png(daemon, capture_id).await?;
    let (bytes, ext) = crate::quick::files::render_copy(&png, run.format, run.scale)?;
    let dir = dir.to_owned();
    tokio::task::spawn_blocking(move || {
        std::fs::create_dir_all(&dir).map_err(|e| crate::save_folder::SaveError::from_io(&e))?;
        let pattern = crate::save_folder::SaveFolderConfig::from_settings(&Map::new()).pattern;
        let stem = crate::save_folder::expand_pattern(&pattern, crate::save_folder::local_now());
        let stem = stem.strip_suffix(".png").unwrap_or(&stem).to_owned();
        crate::save_folder::write_unique(&dir, &format!("{stem}.{ext}"), &bytes)
    })
    .await
    .map_err(|e| e.to_string())?
    .map(|p| p.display().to_string())
    .map_err(|e| e.reason())
}

/// The screenshot ended: runs the steps, then shows the bar or the Edit / Undo toast (QC-3).
pub fn finish_shot(app: &Entity<AppModel>, ctx: QuickCtx, done: &AfterCapture, cx: &mut App) {
    let backend = app.read(cx).backend.clone();
    if !ctx.run.has_steps() {
        present(app, &ctx, done, &[], cx);
        return;
    }
    let (app2, done2, ctx2) = (app.clone(), done.clone(), ctx.clone());
    cx.spawn(async move |cx| {
        let (b, run, id) = (backend.clone(), ctx.run.clone(), done2.capture_id.clone());
        let notes = backend
            .rt
            .spawn(async move { run_steps(&b, &run, &id).await })
            .await
            .unwrap_or_default();
        cx.update(|cx| {
            app2.update(cx, |m, cx| {
                m.library_epoch += 1;
                cx.notify();
            });
            present(&app2, &ctx2, &done2, &notes, cx)
        });
    })
    .detach();
}

fn present(
    app: &Entity<AppModel>,
    ctx: &QuickCtx,
    done: &AfterCapture,
    notes: &[StepNote],
    cx: &mut App,
) {
    use crate::quick::run::AfterMode;
    for n in notes {
        if let Some(spec) = note_toast(n) {
            app::push_toast(app, spec, cx);
        }
    }
    match ctx.run.after_mode() {
        AfterMode::Toast => {
            if ctx.run.editor {
                if let Some(tool) = &ctx.run.tool {
                    let id = done.capture_id.clone();
                    app.update(cx, |m, _| m.quick.editor_tool = Some((id, tool.clone())));
                }
                app::navigate(app, app::open_route(&done.capture_id), cx);
            }
            app::push_toast(app, edit_undo_toast(done, ctx.preset_name.clone()), cx);
        }
        AfterMode::Bar => {
            crate::ui::quick_after::open(app, ctx.area, done.capture_id.clone(), cx);
        }
    }
}

fn note_toast(n: &StepNote) -> Option<ToastSpec> {
    match n {
        StepNote::Blurred(0) => Some(ToastSpec::new(
            ToastKind::Info,
            tr!("Auto-redact found nothing to blur"),
        )),
        StepNote::Blurred(k) => Some(ToastSpec::new(
            ToastKind::Success,
            cm_i18n::trn!("Blurred {n} area", "Blurred {n} areas", *k),
        )),
        StepNote::RedactFailed(e) => Some(
            ToastSpec::new(ToastKind::Warning, tr!("Auto-redact didn\u{2019}t run"))
                .body(e.clone()),
        ),
        StepNote::SavedTo(_) => None,
        StepNote::SaveFailed { folder, reason } => Some(
            ToastSpec::new(
                ToastKind::Warning,
                tr!("Couldn\u{2019}t save to the preset\u{2019}s folder"),
            )
            .body(tr!(
                "{folder}: {reason}. The capture is in your library.",
                folder = folder,
                reason = reason
            )),
        ),
    }
}

/// QC-3.AC2: where the capture went, with Edit and Undo for [`TOAST_SECS`] seconds.
pub fn edit_undo_toast(done: &AfterCapture, preset: Option<String>) -> ToastSpec {
    let done = done.clone();
    ToastSpec::live(move || {
        let (title, kind) = match &done.copy {
            CopyResult::Copied => (tr!("Copied to clipboard"), ToastKind::Success),
            CopyResult::Failed(_) => (tr!("Copied to library only"), ToastKind::Warning),
            CopyResult::Skipped => (tr!("Saved to your library"), ToastKind::Success),
        };
        let mut spec = ToastSpec::new(kind, title);
        if let Some(p) = &preset {
            spec = spec.body(p.clone());
        }
        spec.button(
            tr_ctx!("button", "Edit"),
            ToastAction::OpenCapture(done.capture_id.clone()),
        )
        .button(
            tr_ctx!("button", "Undo"),
            ToastAction::UndoQuick(done.capture_id.clone()),
        )
        .dismiss_after(Duration::from_secs(crate::quick::run::TOAST_SECS))
    })
}

/// QC-3.AC5: `delete_capture` as the user (the toast's Undo asks nothing: the capture was just made).
pub async fn remove_capture(
    daemon: &cm_daemon::Daemon,
    capture_id: &str,
) -> Result<Value, crate::backend::CallError> {
    Backend::call_coded(
        daemon,
        "delete_capture",
        json!({ "capture_id": capture_id }),
    )
    .await
}

/// QC-3.AC5: removes the capture from the library (the toast's Undo).
pub fn undo(app: &Entity<AppModel>, capture_id: &str, cx: &mut App) {
    let backend = app.read(cx).backend.clone();
    let Some(daemon) = backend.daemon.clone() else {
        return;
    };
    let (app2, id) = (app.clone(), capture_id.to_owned());
    cx.spawn(async move |cx| {
        let id2 = id.clone();
        let res = backend
            .rt
            .spawn(async move { remove_capture(&daemon, &id2).await })
            .await;
        cx.update(|cx| match res {
            Ok(Ok(_)) => {
                app2.update(cx, |m, cx| {
                    m.recents.retain(|(i, _)| *i != id);
                    m.library_epoch += 1;
                    cx.notify();
                });
                app::refresh_tray_recents(&app2, cx);
                app::push_toast(
                    &app2,
                    ToastSpec::new(ToastKind::Success, tr!("Capture removed")),
                    cx,
                );
            }
            Ok(Err(e)) => app::push_toast(
                &app2,
                crate::flows::error_toast_live(
                    || tr!("Couldn\u{2019}t remove the capture"),
                    crate::daemon_text::call_error_text(&e),
                ),
                cx,
            ),
            Err(_) => {}
        });
    })
    .detach();
}

// ---- the bar's actions (QC-3) ---------------------------------------------------------------

/// Edit (E).
pub fn bar_edit(app: &Entity<AppModel>, id: &str, cx: &mut App) {
    app::navigate(app, app::open_route(id), cx);
}

/// Copy (C).
pub fn bar_copy(app: &Entity<AppModel>, id: &str, cx: &mut App) {
    app::handle_command(
        app,
        Command::ToastAction(ToastAction::CopyCapture(id.to_owned())),
        cx,
    );
}

/// Text (T): copies the recognised text (ANN-5).
pub fn bar_text(app: &Entity<AppModel>, id: &str, cx: &mut App) {
    app::handle_command(
        app,
        Command::ToastAction(ToastAction::CopyText(id.to_owned())),
        cx,
    );
}

/// Save (S): into the folder of Settings > General > Save a copy to a folder when one is set,
/// else the Save as dialog.
pub fn bar_save(app: &Entity<AppModel>, id: &str, cx: &mut App) {
    let backend = app.read(cx).backend.clone();
    let cfg = crate::save_folder::SaveFolderConfig::from_settings(&backend.settings.snapshot());
    if !(cfg.enabled && cfg.folder.is_some()) {
        app::handle_command(
            app,
            Command::ToastAction(ToastAction::SaveCaptureAs(id.to_owned())),
            cx,
        );
        return;
    }
    let (app2, id) = (app.clone(), id.to_owned());
    cx.spawn(async move |cx| {
        let b = backend.clone();
        let res = backend
            .rt
            .spawn(async move { b.save_capture_to_folder(&id).await })
            .await;
        cx.update(|cx| {
            let spec = match res {
                Ok(Ok(crate::save_folder::FolderResult::Saved(p))) => ToastSpec::new(
                    ToastKind::Success,
                    tr!("Saved to {folder}", folder = p.display()),
                ),
                Ok(Ok(crate::save_folder::FolderResult::Failed { folder, reason })) => {
                    ToastSpec::new(
                        ToastKind::Warning,
                        tr!("Couldn\u{2019}t save to {folder}", folder = folder),
                    )
                    .body(reason)
                }
                Ok(Ok(crate::save_folder::FolderResult::Skipped)) => {
                    ToastSpec::new(ToastKind::Info, tr!("Saved to your library"))
                }
                Ok(Err(e)) => crate::flows::error_toast_live(|| tr!("Couldn\u{2019}t save"), e),
                Err(e) => {
                    crate::flows::error_toast_live(|| tr!("Couldn\u{2019}t save"), e.to_string())
                }
            };
            app::push_toast(&app2, spec, cx);
        });
    })
    .detach();
}

/// Long Shot (L): extends the captured area by scrolling (CAP-6), the area as the start.
pub fn bar_long_shot(app: &Entity<AppModel>, area: Option<Area>, cx: &mut App) {
    let area = area.unwrap_or(Area::Display { display_id: None });
    let (app2, backend) = (app.clone(), app.read(cx).backend.clone());
    let Some(daemon) = backend.daemon.clone() else {
        return;
    };
    cx.spawn(async move |cx| {
        let listed = backend
            .rt
            .spawn(async move {
                Backend::call(&daemon, "list_targets", json!({ "windows": true })).await
            })
            .await;
        cx.update(|cx| {
            let placed = listed
                .ok()
                .and_then(|r| r.ok())
                .and_then(|v| crate::quick::area::parse_targets(&v))
                .and_then(|t| crate::quick::area::place(&t, &area));
            match placed {
                Some(p) => crate::ui::long_shot::begin_from_area(&app2, &p, cx),
                None => crate::ui::long_shot::start(&app2, cx),
            }
        });
    })
    .detach();
}

/// Record (R): records exactly the captured area, the countdown first, then the normal HUD.
pub fn bar_record(app: &Entity<AppModel>, area: Option<Area>, cx: &mut App) {
    let v = {
        let m = app.read(cx);
        let t = m.quick.panel.mem.video;
        VideoRun {
            target: PresetTarget::Display,
            mic: t.mic,
            system_audio: t.system_audio,
            webcam: t.webcam,
            countdown_s: t.countdown_s,
            fps: None,
            cursor: None,
        }
    };
    let backend = app.read(cx).backend.clone();
    if crate::ui::recording::refuse_recording(app, &backend, cx)
        || crate::ui::recording::is_active(app, cx)
    {
        return;
    }
    let area = area.unwrap_or(Area::Display { display_id: None });
    // A region or a window is a target as it is; an unknown display is the primary one.
    let direct = match area {
        Area::Region { display_id, rect } => Some(Target::Region { display_id, rect }),
        Area::Window { window_id } => Some(Target::Window { window_id }),
        Area::Display {
            display_id: Some(display_id),
        } => Some(Target::Display { display_id }),
        Area::Display { display_id: None } => None,
    };
    if let Some(t) = direct {
        start_video(app, &v, t, cx);
        return;
    }
    video(app, v, cx);
}

// ---- presets: the file, the actions, the hotkeys ---------------------------------------------

/// Reads `presets.json` into the app (the notice about skipped presets is kept for the page).
pub fn refresh_presets(app: &Entity<AppModel>, cx: &mut App) {
    let backend = app.read(cx).backend.clone();
    let Some(store) = backend.daemon.as_ref().and_then(|d| d.preset_store()) else {
        return;
    };
    let loaded = store.load();
    app.update(cx, |m, cx| {
        let first = m.quick.notice.is_none() && loaded.notice.is_some();
        m.quick.presets = loaded.presets;
        m.quick.notice = loaded.notice.clone();
        // A picked preset that no longer exists is let go; one that changed is read again.
        let picked = m.quick.panel.preset.as_ref().map(|p| p.id.clone());
        if let Some(id) = picked {
            match m.quick.presets.iter().find(|p| p.id == id).cloned() {
                Some(p) if m.quick.panel.preset.as_ref() != Some(&p) => {
                    m.quick.panel.preset = Some(p)
                }
                Some(_) => {}
                None => m.quick.panel.clear_preset(),
            }
        }
        if first {
            log::warn!("presets: {}", loaded.notice.unwrap_or_default());
        }
        cx.notify();
    });
    sync_tray_presets(app, cx);
}

/// The tray's "Presets" submenu follows the file.
pub fn sync_tray_presets(app: &Entity<AppModel>, cx: &mut App) {
    let list: Vec<(String, String)> = app
        .read(cx)
        .quick
        .presets
        .iter()
        .map(|p| (p.id.clone(), p.name.clone()))
        .collect();
    if let Some(t) = &app.read(cx).tray {
        t.set_presets(&list);
    }
}

/// QC-4.AC4: the one notice that names the skipped presets, shown once at start.
pub fn announce_notice(app: &Entity<AppModel>, cx: &mut App) {
    let notice = app.read(cx).quick.notice.clone();
    if let Some(n) = notice {
        app::push_toast(
            app,
            ToastSpec::new(ToastKind::Warning, tr!("Some presets were skipped")).body(n),
            cx,
        );
    }
}

/// One preset action (`save_preset`, `delete_preset`, `reorder_presets`) as the user; the
/// presets and their hotkeys are read again afterwards, then `then` runs with the answer.
pub fn preset_call(
    app: &Entity<AppModel>,
    method: &'static str,
    params: Value,
    then: impl FnOnce(&Entity<AppModel>, Result<Value, String>, &mut App) + 'static,
    cx: &mut App,
) {
    let backend = app.read(cx).backend.clone();
    let Some(daemon) = backend.daemon.clone() else {
        then(app, Err(tr!("The library is not available")), cx);
        return;
    };
    let app2 = app.clone();
    cx.spawn(async move |cx| {
        let res = backend
            .rt
            .spawn(async move { Backend::call(&daemon, method, params).await })
            .await
            .unwrap_or_else(|e| Err(e.to_string()));
        cx.update(|cx| {
            refresh_presets(&app2, cx);
            sync_hotkeys(&app2, cx);
            then(&app2, res, cx);
        });
    })
    .detach();
}

/// Registers the global hotkeys of the presets that have a shortcut (QC-4.AC5). Where the
/// desktop gives apps no global keys (a Wayland session) none are registered.
pub fn sync_hotkeys(app: &Entity<AppModel>, cx: &mut App) {
    unregister_hotkeys(app, cx);
    if crate::platform_hotkeys::backend() != crate::platform_hotkeys::Backend::Native {
        return;
    }
    let (map, presets) = {
        let m = app.read(cx);
        (m.shortcuts.clone(), m.quick.presets.clone())
    };
    let Some(holder) = cx.try_global::<crate::platform::HotkeyHolder>() else {
        return;
    };
    let mut ids = std::collections::HashMap::new();
    let mut chords = Vec::new();
    for (id, chord) in crate::quick::presets::hotkey_plan(&map, &presets) {
        let Ok(hk) = crate::platform::chord_to_hotkey(&chord) else {
            continue;
        };
        match holder.0.register(hk) {
            Ok(()) => {
                ids.insert(hk.id(), id);
                chords.push(chord);
            }
            Err(e) => log::warn!("preset {id}: could not register its shortcut: {e}"),
        }
    }
    app.update(cx, |m, _| {
        m.quick.hotkeys = ids;
        m.quick.registered = chords;
    });
}

/// Releases the presets' global hotkeys (before they are registered again, and while the
/// Shortcuts page waits for a new combination).
pub fn unregister_hotkeys(app: &Entity<AppModel>, cx: &mut App) {
    let chords = app.update(cx, |m, _| {
        m.quick.hotkeys.clear();
        std::mem::take(&mut m.quick.registered)
    });
    if let Some(h) = cx.try_global::<crate::platform::HotkeyHolder>() {
        for c in chords {
            if let Ok(hk) = crate::platform::chord_to_hotkey(&c) {
                let _ = h.0.unregister(hk);
            }
        }
    }
}

/// The preset a global hotkey id belongs to.
pub fn hotkey_preset(app: &Entity<AppModel>, id: u32, cx: &App) -> Option<String> {
    app.read(cx).quick.hotkeys.get(&id).cloned()
}

/// The panel's remembered choices are written to `quick_capture.json`.
pub fn remember_panel(app: &Entity<AppModel>, cx: &App) {
    crate::quick::state::save(
        &cm_daemon::config::config_dir(),
        app.read(cx).quick.panel.persisted(),
    );
}

/// Reads the remembered choices and the presets at start.
pub fn init(app: &Entity<AppModel>, cx: &mut App) {
    let mem = crate::quick::state::load(&cm_daemon::config::config_dir());
    app.update(cx, |m, _| m.quick.panel.mem = mem);
    refresh_presets(app, cx);
    sync_hotkeys(app, cx);
    announce_notice(app, cx);
}

/// A new [`Preset`] for the page: a screenshot of the Region target with nothing else.
pub fn blank_preset() -> Preset {
    Preset {
        id: String::new(),
        name: String::new(),
        kind: PresetKind::Screenshot,
        target: Some(PresetTarget::Region),
        options: Default::default(),
        save_to: None,
        after: Default::default(),
        shortcut: None,
        extra: Map::new(),
    }
}

/// The Studio route of the presets page.
pub fn open_presets_page(app: &Entity<AppModel>, cx: &mut App) {
    app.update(cx, |m, _| {
        m.settings_page_request = Some(crate::ui::settings::model::SettingsSection::Capture)
    });
    app::navigate(app, Route::Settings, cx);
}
