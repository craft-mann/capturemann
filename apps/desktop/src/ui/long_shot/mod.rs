//! Long Shot (CAP-6, design boards S1, S2, S2b): the flow around `cm-overlay long` and the
//! daemon's `capture_screenshot { mode: "long" }`.
//!
//! 1. The overlay (Long Shot mode) picks the area and the options (Auto or Hand scroll,
//!    direction). It takes no screenshot.
//! 2. The HUD (and the side preview, when there is room) opens at its S2b place, excluded from
//!    screen capture and never taking the focus from the window being scrolled.
//! 3. One `capture_screenshot` call runs the whole Long Shot. The HUD polls `long_shot_state`
//!    (counts and a small preview picture) and calls it to Pause; Stop is
//!    `Daemon::cancel_delayed_capture` with the call's `request_id`: what was captured is kept.
//! 4. The call returns the stored capture; the toast says how it ended.
//!
//! Everything that changes state goes through daemon actions.

pub mod geometry;
mod hud;
pub mod model;

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use cm_i18n::tr;
use gpui::{App, AppContext as _, Entity, Global, WindowHandle};
use serde_json::{json, Value};

use crate::backend::Backend;
use crate::overlay::{LongOutcome, LongPick, OverlayError};
use crate::recording::geometry::Rect;
use crate::recording::monitors::parse_engine_displays;
use crate::toast::{ToastKind, ToastSpec};
use crate::ui::app::{self, capture_flow, AppModel};
use crate::ui::recording::{display_for, visible_part};

use hud::{LongHud, LongPreview};
use model::LongState;

/// The running Long Shot, if any (one at a time: the capture slot is held for its whole run).
#[derive(Default)]
struct Holder(Option<Running>);
impl Global for Holder {}

struct Running {
    hud: WindowHandle<LongHud>,
    preview: Option<WindowHandle<LongPreview>>,
    model: Entity<LongModel>,
}

/// What the HUD and the preview share.
pub struct LongModel {
    pub state: LongState,
    pub request_id: String,
    pub app: Entity<AppModel>,
    /// The HUD shows a Preview button (no room beside the area).
    pub preview_button: bool,
    /// Where the Preview button's floating panel goes, and the display it is on.
    pub floating: (gpui::DisplayId, Rect),
}

static NEXT: AtomicU64 = AtomicU64::new(1);

fn new_request_id() -> String {
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    format!("long-{t}-{n}")
}

fn toast(app: &Entity<AppModel>, spec: ToastSpec, cx: &mut App) {
    app::push_toast(app, spec, cx);
}

fn free(app: &Entity<AppModel>, cx: &mut App) {
    app.update(cx, |m, _| m.capture_busy = false);
}

/// Whether a Long Shot is open (the pick or the run).
pub fn is_running(cx: &App) -> bool {
    cx.try_global::<Holder>().is_some_and(|h| h.0.is_some())
}

/// "Long Shot" (the launcher command, the hotkey, the menu): opens the overlay in Long Shot mode.
pub fn start(app: &Entity<AppModel>, cx: &mut App) {
    if is_running(cx) {
        return;
    }
    let (backend, busy) = {
        let m = app.read(cx);
        (m.backend.clone(), m.capture_busy)
    };
    if busy {
        return; // CAP-2: one capture at a time.
    }
    if backend.daemon.is_none() {
        toast(
            app,
            ToastSpec::new(ToastKind::Error, tr!("Capture unavailable")).body(tr!(
                "The library couldn\u{2019}t be opened, so there is nowhere to save captures."
            )),
            cx,
        );
        return;
    }
    app.update(cx, |m, _| m.capture_busy = true);
    let auto = model::overlay_flag(cm_capture::long_shot::auto_scroll_support());
    let app2 = app.clone();
    cx.spawn(async move |cx| {
        let b = backend.clone();
        let picked = backend
            .rt
            .spawn(async move { b.overlay.pick_long(auto).await })
            .await;
        cx.update(|cx| after_pick(&app2, picked, cx));
    })
    .detach();
}

fn after_pick(
    app: &Entity<AppModel>,
    picked: Result<Result<LongOutcome, OverlayError>, tokio::task::JoinError>,
    cx: &mut App,
) {
    match picked {
        Ok(Ok(LongOutcome::Picked(p))) => begin(app, p, cx),
        Ok(Ok(LongOutcome::Cancelled)) | Err(_) => free(app, cx),
        Ok(Ok(LongOutcome::Switch(name))) => {
            free(app, cx);
            match name.as_str() {
                "region" => {
                    capture_flow::start_capture(app, crate::overlay::CaptureMode::Region, cx)
                }
                "window" => {
                    capture_flow::start_capture(app, crate::overlay::CaptureMode::Window, cx)
                }
                "display" => {
                    capture_flow::start_capture(app, crate::overlay::CaptureMode::Display, cx);
                }
                "guide" => crate::ui::guide_view::hud::start(app, cx),
                // "Grant access...": macOS shows its own prompt; the user then starts Long Shot
                // again (the permission is read when the overlay opens).
                "accessibility" => {
                    cm_capture::long_shot::request_accessibility();
                    toast(
                        app,
                        ToastSpec::new(ToastKind::Info, tr!("Allow CaptureMann in Accessibility"))
                            .body(tr!(
                                "Then start Long Shot again. Until then, Hand scroll still works."
                            )),
                        cx,
                    );
                }
                _ => {}
            }
        }
        Ok(Err(e)) => {
            free(app, cx);
            toast(app, crate::flows::overlay_error_toast(&e), cx);
        }
    }
}

/// The overlay picked: open the HUD, then run the capture.
fn begin(app: &Entity<AppModel>, pick: LongPick, cx: &mut App) {
    let backend = app.read(cx).backend.clone();
    let Some(daemon) = backend.daemon.clone() else {
        free(app, cx);
        return;
    };
    let copy = capture_flow::take_copy(app, cx);
    let request_id = new_request_id();
    let app2 = app.clone();
    cx.spawn(async move |cx| {
        // The engine's own display list places the picked area in the desktop.
        let d2 = daemon.clone();
        let listed = backend
            .rt
            .spawn(async move {
                Backend::call(&d2, "list_targets", json!({ "windows": false }))
                    .await
                    .map(|v| parse_engine_displays(&v))
                    .unwrap_or_default()
            })
            .await
            .unwrap_or_default();
        let (p2, rid, a3) = (pick.clone(), request_id.clone(), app2.clone());
        cx.update(|cx| open_windows(&a3, &p2, &listed, &rid, cx));
        // The overlay's window needs a moment to leave the screen before the first frame.
        cx.background_executor()
            .timer(Duration::from_millis(
                crate::launcher::commands::CAPTURE_DELAY_MS,
            ))
            .await;
        let params = json!({
            "mode": "long",
            "target": pick.target_json(),
            "direction": pick.direction,
            "scroll": pick.scroll,
            "request_id": request_id,
        });
        let b = backend.clone();
        let result = backend
            .rt
            .spawn(async move {
                match Backend::call_coded(&daemon, "capture_screenshot", params).await {
                    Ok(v) => {
                        let id = v["capture_id"].as_str().unwrap_or_default().to_owned();
                        let done = b.after_capture(&id, copy).await;
                        Ok((v, done))
                    }
                    Err(e) => Err(e),
                }
            })
            .await;
        cx.update(|cx| finish(&app2, result, cx));
    })
    .detach();
}

fn open_windows(
    app: &Entity<AppModel>,
    pick: &LongPick,
    listed: &[crate::recording::monitors::EngineDisplay],
    request_id: &str,
    cx: &mut App,
) {
    let area_px = pick.area.or(pick.rect).unwrap_or([0, 0, 1, 1]);
    let area = geometry::area_global_px(listed, pick.display_id, area_px)
        .map(|g| geometry::area_in_points(g, pick.scale));
    let Some(area) = area else {
        log::warn!("long shot: the picked display is not in the engine's list; no HUD");
        return;
    };
    let Some(display) = display_for(cx, &area) else {
        return;
    };
    let usable = visible_part(&display.1);
    let layout = geometry::layout(&usable, &area);
    let floating = geometry::floating_preview(&usable, &area);
    let model = cx.new(|_| LongModel {
        state: LongState::new(&pick.scroll, &pick.direction),
        request_id: request_id.to_owned(),
        app: app.clone(),
        preview_button: layout.preview_button,
        floating: (display.0, floating),
    });
    let hud = hud::open_hud(&model, display, layout.hud, cx);
    let Some(hud) = hud else {
        return;
    };
    let preview = layout
        .preview
        .and_then(|r| hud::open_preview(&model, display, r, cx));
    cx.set_global(Holder(Some(Running {
        hud,
        preview,
        model: model.clone(),
    })));
    poll(&model, cx);
}

/// Asks the daemon for the counts and the preview picture four times a second until it ends.
fn poll(model: &Entity<LongModel>, cx: &mut App) {
    let model = model.downgrade();
    cx.spawn(async move |cx| loop {
        cx.background_executor()
            .timer(Duration::from_millis(250))
            .await;
        let Ok(Some((backend, request_id, since))) = model.update(cx, |m, cx| {
            let b = m.app.read(cx).backend.clone();
            if m.state.finished {
                return None;
            }
            Some((b, m.request_id.clone(), m.state.seq))
        }) else {
            break;
        };
        let Some(daemon) = backend.daemon.clone() else {
            break;
        };
        let answer = backend
            .rt
            .spawn(async move {
                Backend::call(
                    &daemon,
                    "long_shot_state",
                    json!({ "request_id": request_id, "since_seq": since }),
                )
                .await
                .ok()
            })
            .await
            .ok()
            .flatten();
        let Some(answer) = answer else { continue };
        let png = model
            .update(cx, |m, _| m.state.apply(&answer))
            .ok()
            .flatten();
        if let Some(png) = png {
            let decoded = cx
                .background_executor()
                .spawn(async move { crate::poster::decode_poster(&png, 360).ok() })
                .await;
            if let Some(img) = decoded {
                let size = (img.width(), img.height());
                let image = std::sync::Arc::new(gpui::RenderImage::new([image::Frame::new(img)]));
                let _ = model.update(cx, |m, cx| {
                    m.state.preview = Some(image);
                    m.state.preview_px = size;
                    cx.notify();
                });
            }
        } else {
            let _ = model.update(cx, |_, cx| cx.notify());
        }
    })
    .detach();
}

/// Closes the HUD and the preview.
fn close_windows(cx: &mut App) {
    let running = cx
        .try_global::<Holder>()
        .and_then(|h| h.0.as_ref().map(|r| (r.hud, r.preview, r.model.clone())));
    if let Some((hud, preview, model)) = running {
        model.update(cx, |m, _| m.state.finished = true);
        crate::surfaces::close(cx, hud);
        if let Some(p) = preview {
            crate::surfaces::close(cx, p);
        }
        if let Some(p) = hud::floating_handle(cx) {
            crate::surfaces::close(cx, p);
        }
    }
    cx.set_global(Holder(None));
}

/// What the capture call came back with: the daemon's answer and the CAP-4 finish (clipboard, folder).
type Finished =
    Result<(Value, Result<crate::backend::AfterCapture, String>), crate::backend::CallError>;

fn finish(app: &Entity<AppModel>, result: Result<Finished, tokio::task::JoinError>, cx: &mut App) {
    close_windows(cx);
    free(app, cx);
    match result {
        Ok(Ok((v, done))) => {
            let Some(spec) = model::done_toast(&v) else {
                toast(
                    app,
                    crate::flows::error_toast(
                        &tr!("Long Shot failed"),
                        tr!("The capture has no id"),
                    ),
                    cx,
                );
                return;
            };
            if let Ok(done) = &done {
                app.update(cx, |m, cx| {
                    m.recents.retain(|(id, _)| *id != done.capture_id);
                    m.recents
                        .insert(0, (done.capture_id.clone(), done.title.clone()));
                    m.recents.truncate(5);
                    m.library_epoch += 1;
                    cx.notify();
                });
                capture_flow::refresh_tray_recents(app, cx);
            }
            toast(app, spec, cx);
            if let Ok(done) = &done {
                if let Some(w) = crate::flows::folder_warning_toast(&done.folder) {
                    toast(app, w, cx);
                }
            }
        }
        Ok(Err(e)) => {
            let spec = if e.permission.is_some() {
                ToastSpec::new(ToastKind::Warning, tr!("Long Shot needs a permission"))
                    .body(crate::daemon_text::call_error_text(&e))
            } else {
                crate::flows::error_toast(
                    &tr!("Long Shot failed"),
                    crate::daemon_text::call_error_text(&e),
                )
            };
            toast(app, spec, cx);
        }
        Err(_) => {}
    }
}
