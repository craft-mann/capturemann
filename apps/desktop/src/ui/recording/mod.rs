//! The recording UI: HUD, countdown, area indicator, webcam bubble and start sheet, driven by the
//! daemon's `recording.*` events (REC-1, REC-4, REC-5, REC-6, CAM-1, CAM-4).
//!
//! # Ownership
//!
//! The desktop app records through the embedded daemon as `Actor::User`: the user's own Start,
//! Stop, Pause, Discard and Restart run directly (no Confirm dialog; the HUD asks "Discard this
//! take?" itself), while an agent over the socket still hits the Confirm gate. Windows follow the
//! events, not the caller: a recording an agent started shows the same countdown, HUD and brackets,
//! plus the agent toast (NFR-11: no way to record without the indicator).
//!
//! * [`RecState`] (an entity) holds the display-free [`HudState`], the bubble preferences and the
//!   window handles; the views observe it and nothing else, so a 10 Hz timer never re-renders the
//!   Studio window.
//! * [`on_event`] is fed every daemon event; [`start`], [`stop`], [`toggle_pause`], [`ask_discard`]
//!   and friends are what buttons, hotkeys, the tray and the launcher call.
//! * Every window is excluded from capture twice: the engine excludes this process's windows by pid
//!   and each window also gets `sharingType = none` ([`crate::platform_window`]).

use std::sync::Arc;
use std::time::{Duration, Instant};

use cm_daemon::events::Event;
use cm_i18n::tr;
use gpui::{
    point, px, size, App, AppContext as _, Bounds, DisplayId, Entity, Task,
    WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind, WindowOptions,
};
use serde_json::{json, Map, Value};

use crate::backend::{Backend, RecordingBlocker};
use crate::confirm::{discard_dialog, ConfirmUiMsg};
use crate::platform_window::{self, Tweaks};
use crate::recording::countdown;
use crate::recording::geometry::{
    self, bubble_dims, bubble_layout, hud_default, pick_display, rect_from_center_fraction,
    snap_hud, AreaKind, BubblePosition, BubbleShape, BubbleSize, Rect,
};
use crate::recording::model::{Change, Ended, EscAction, HudState, Phase};
use crate::recording::preview::{FrameSlot, PreviewRunner, PreviewSource};
use crate::recording::start::{self, SheetDefaults, StartOptions, Target};
use crate::recording::toasts;
use crate::recording::tray::{self, PauseAction, ToggleAction, TrayPlan};
use crate::toast::{ToastKind, ToastSpec};
use crate::ui::app::{self, AppModel};

pub mod banner;
pub mod bubble;
pub mod countdown_view;
pub mod details;
pub mod hud;
pub mod indicator;
pub mod sheet;
pub mod transcript_card;
pub mod window_list;

use bubble::BubbleView;
use countdown_view::CountdownView;
use hud::HudView;
use indicator::IndicatorView;
use sheet::StartSheet;

/// HUD window size without an agent chip: the bar (506 x 58, Stop carries its label) under a 36 px
/// strip for chips and hints. With an agent chip the bar is wider: `hud_layout::window_size`.
pub const HUD_BAR: (f64, f64) = (506.0, crate::recording::hud_layout::BAR_H);
pub const HUD_STRIP: f64 = crate::recording::hud_layout::STRIP_H;
pub const HUD_WINDOW: (f64, f64) = (HUD_BAR.0, HUD_BAR.1 + HUD_STRIP);

/// The bubble's current look (CAM-1), remembered in settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BubblePrefs {
    pub shape: BubbleShape,
    pub size: BubbleSize,
    pub mirror: bool,
    pub position: BubblePosition,
}

impl Default for BubblePrefs {
    fn default() -> Self {
        BubblePrefs {
            shape: BubbleShape::default(),
            size: BubbleSize::default(),
            mirror: true,
            position: BubblePosition::default(),
        }
    }
}

/// Everything the recording windows share.
pub struct RecState {
    pub hud: HudState,
    pub prefs: BubblePrefs,
    /// The recorded area (or the chosen display during setup), in global points.
    pub area: Option<Rect>,
    /// The display holding the area, in global points.
    pub display: Option<(DisplayId, Rect)>,
    pub frames: Arc<FrameSlot>,
    pub preview: Option<PreviewRunner>,
    pub preview_source: PreviewSource,
    /// Camera device of the running or about-to-start recording (`None`: system default).
    pub camera_device: Option<String>,
    /// The bubble shows before recording, from the start sheet (CAM-4).
    pub setup_preview: bool,
    /// A Discard or Restart dialog is open.
    pub dialog_open: bool,
    pub last_plan: Option<TrayPlan>,
    pub hud_win: Option<WindowHandle<HudView>>,
    pub countdown_win: Option<WindowHandle<CountdownView>>,
    pub indicator_win: Option<WindowHandle<IndicatorView>>,
    pub bubble_win: Option<WindowHandle<BubbleView>>,
    pub sheet_win: Option<WindowHandle<StartSheet>>,
    /// The OS asks for reduced motion: no ring animation, no blinking dot (R4-13). Read when the
    /// countdown and the HUD open.
    pub reduced_motion: bool,
    /// Bumped by every `camera.connected` / `camera.disconnected`; an open start sheet reloads
    /// its device list when it sees a new value (CAM-1).
    pub camera_list_epoch: u64,
    poll: Option<Task<()>>,
}

impl RecState {
    pub fn new(source: PreviewSource) -> RecState {
        RecState {
            hud: HudState::default(),
            prefs: BubblePrefs::default(),
            area: None,
            display: None,
            frames: FrameSlot::new(true),
            preview: None,
            preview_source: source,
            camera_device: None,
            setup_preview: false,
            dialog_open: false,
            last_plan: None,
            hud_win: None,
            countdown_win: None,
            indicator_win: None,
            bubble_win: None,
            sheet_win: None,
            reduced_motion: false,
            camera_list_epoch: 0,
            poll: None,
        }
    }

    /// Turns the camera off: the preview stops (the camera light goes out), the frame slot is
    /// emptied and the setup bubble flag clears. Returns whether a setup bubble was showing.
    pub fn drop_camera(&mut self) -> bool {
        self.preview = None;
        self.frames.clear();
        std::mem::take(&mut self.setup_preview)
    }

    /// `start_recording` or `restart_recording` failed (R4-1, R4-4): resets the HUD model and
    /// releases the camera, which the start sheet may have left running for the bubble. Returns
    /// whether the model was reset, in which case the caller closes every window.
    ///
    /// `foreign_run`: the engine said `busy`, so a run this app did not start (an agent's) may own
    /// the screen; only the pending start is forgotten then.
    pub fn fail_start(&mut self, foreign_run: bool) -> bool {
        let reset = if foreign_run {
            let pending = self.hud.phase == Phase::Starting;
            self.hud.abort_start();
            pending && self.hud.phase == Phase::Idle
        } else {
            self.hud.fail_start()
        };
        if reset {
            self.drop_camera();
            self.dialog_open = false;
        }
        reset
    }
}

// ---- displays and windows --------------------------------------------------------------------

fn rect_of(b: Bounds<gpui::Pixels>) -> Rect {
    Rect::new(
        f64::from(f32::from(b.origin.x)),
        f64::from(f32::from(b.origin.y)),
        f64::from(f32::from(b.size.width)),
        f64::from(f32::from(b.size.height)),
    )
}

/// The displays as `(id, global point bounds)`.
///
/// GPUI gives every display's bounds an origin of (0, 0), so a secondary display would read as
/// sitting on top of the primary; the real origin is taken from the platform when it can say.
pub fn displays(cx: &App) -> Vec<(DisplayId, Rect)> {
    cx.displays()
        .iter()
        .map(|d| {
            let r = rect_of(d.bounds());
            let r = u32::try_from(u64::from(d.id()))
                .ok()
                .and_then(crate::platform_window::display_frame)
                .map(|(x, y, w, h)| Rect::new(x, y, w, h))
                .unwrap_or(r);
            (d.id(), r)
        })
        .collect()
}

/// The display holding `area`.
pub fn display_for(cx: &App, area: &Rect) -> Option<(DisplayId, Rect)> {
    let all = displays(cx);
    let rects: Vec<Rect> = all.iter().map(|(_, r)| *r).collect();
    pick_display(&rects, area).map(|i| all[i])
}

/// The display holding the pointer, else the primary one.
pub fn pointer_display(cx: &App) -> Option<(DisplayId, Rect)> {
    let all = displays(cx);
    // Debug only: `CM_DEMO_DISPLAY=<n>` opens it on the n-th display (0 = primary), to look at
    // a secondary display without moving the real pointer.
    #[cfg(debug_assertions)]
    if let Some(d) = std::env::var("CM_DEMO_DISPLAY")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .and_then(|n| all.get(n).copied())
    {
        return Some(d);
    }
    crate::ui::launcher_window::pointer(cx)
        .and_then(|p| {
            let (x, y) = (f64::from(f32::from(p.x)), f64::from(f32::from(p.y)));
            all.iter().find(|(_, r)| r.contains(x, y)).copied()
        })
        .or_else(|| {
            cx.primary_display()
                .and_then(|d| all.iter().find(|(id, _)| *id == d.id()).copied())
        })
        .or_else(|| all.first().copied())
}

/// The part of `display` a window may use: without the menu bar and the Dock where the platform
/// says (macOS), else the whole display.
pub fn visible_part(display: &Rect) -> Rect {
    let (cx_, cy_) = display.center();
    let v = crate::platform_window::visible_frame_at(cx_, cy_)
        .map(|(x, y, w, h)| Rect::new(x, y, w, h))
        .unwrap_or(*display);
    demo_short(v)
}

/// Debug only: `CM_DEMO_VISIBLE_H=<points>` pretends the display is that tall, to look at the
/// sheet on a short screen (the body scrolls, the footer stays).
#[cfg(debug_assertions)]
pub fn demo_short(v: Rect) -> Rect {
    match std::env::var("CM_DEMO_VISIBLE_H")
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
    {
        Some(h) => Rect::new(v.x, v.y, v.w, h.min(v.h)),
        None => v,
    }
}

#[cfg(not(debug_assertions))]
pub fn demo_short(v: Rect) -> Rect {
    v
}

/// Window options for a borderless always-on-top panel at `global` (points), on `display`.
/// GPUI positions a window relative to its display, hence the subtraction.
pub fn panel_options(
    display: (DisplayId, Rect),
    global: Rect,
    focus: bool,
    movable: bool,
) -> WindowOptions {
    let local = global.relative_to(&display.1);
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin: point(px(local.x as f32), px(local.y as f32)),
            size: size(px(global.w as f32), px(global.h as f32)),
        })),
        titlebar: None,
        focus,
        show: true,
        kind: WindowKind::PopUp,
        is_movable: movable,
        is_resizable: false,
        is_minimizable: false,
        display_id: Some(display.0),
        window_background: WindowBackgroundAppearance::Transparent,
        ..crate::platform_window::popup_base()
    }
}

/// Hides a freshly opened window from screen capture (and optionally from the mouse). Only the
/// debug demo (`CM_DEMO_CAPTURABLE`) leaves windows capturable, so they can be screenshotted.
pub fn harden(window: &gpui::Window, click_through: bool) {
    if let Err(e) = platform_window::apply(
        window,
        Tweaks {
            exclude_from_capture: !crate::recording::demo::capturable(),
            click_through,
            no_shadow: true,
            // The click-through indicator covers the menu bar too: screen-saver level, like the picker overlay.
            level: click_through.then_some(1000),
        },
    ) {
        log::warn!("could not hide a recording window from capture: {e}");
    }
}

fn rec_of(app: &Entity<AppModel>, cx: &App) -> Entity<RecState> {
    app.read(cx).rec.clone()
}

fn close<V: 'static + gpui::Render>(cx: &mut App, h: Option<WindowHandle<V>>) {
    if let Some(h) = h {
        let _ = h.update(cx, |_, window, _| window.remove_window());
    }
}

// ---- events ----------------------------------------------------------------------------------

/// Applies one daemon event. Only `recording.*` events matter.
pub fn on_event(app: &Entity<AppModel>, ev: &Event, cx: &mut App) {
    if crate::recording::sheet::camera_event_changes_devices(&ev.name) {
        // The Record sheet's camera list is stale: it reloads on seeing the new epoch.
        rec_of(app, cx).update(cx, |r, cx| {
            r.camera_list_epoch += 1;
            cx.notify();
        });
        return;
    }
    if !ev.name.starts_with("recording.") {
        return;
    }
    let rec = rec_of(app, cx);
    let now = Instant::now();
    let (change, prompt) = rec.update(cx, |r, cx| {
        let before = r.hud.marker_prompt;
        let c = r.hud.apply_event(&ev.name, &ev.data, now);
        cx.notify();
        (c, r.hud.marker_prompt.filter(|p| Some(*p) != before))
    });
    handle_change(app, change, cx);
    if let Some(p) = prompt {
        expire_marker_prompt(&rec, p.at_ms, cx);
    }
}

/// The label prompt after a marker closes by itself after [`MARKER_PROMPT_TTL`](crate::recording::model::MARKER_PROMPT_TTL)
/// unless it was answered.
fn expire_marker_prompt(rec: &Entity<RecState>, at_ms: i64, cx: &mut App) {
    let rec = rec.clone();
    cx.spawn(async move |cx| {
        cx.background_executor()
            .timer(crate::recording::model::MARKER_PROMPT_TTL)
            .await;
        cx.update(|cx| {
            rec.update(cx, |r, cx| {
                if r.hud.close_marker_prompt(Some(at_ms)) {
                    cx.notify();
                }
            })
        });
    })
    .detach();
}

/// Opens, updates and closes the windows a model change calls for, then re-syncs the tray. The
/// events, the status poll and the resync after a lagged stream all end up here, so the windows
/// only ever follow the model.
fn handle_change(app: &Entity<AppModel>, change: Change, cx: &mut App) {
    match change {
        Change::None | Change::Updated => {}
        Change::Countdown => {
            show_countdown(app, cx);
            // A start that dies in the countdown sends no event on some paths: poll from the
            // first digit so the engine's idle answer closes the countdown (R4-3).
            start_poll(app, cx);
        }
        Change::SelfTest => {
            open_recording_windows(app, cx);
            start_poll(app, cx);
        }
        Change::Started { agent } => {
            close_countdown(app, cx);
            open_recording_windows(app, cx);
            start_poll(app, cx);
            if let Some((client, tool)) = agent {
                let id = rec_of(app, cx).read(cx).hud.recording_id.clone();
                app::push_toast(
                    app,
                    toasts::agent_started_toast(&client, &tool, id.as_deref()),
                    cx,
                );
            }
        }
        Change::Restarting => {
            // The take is gone; the HUD, brackets and bubble stay for the next one.
            close_countdown(app, cx);
        }
        Change::Ended(e) => end_all(app, &e, cx),
    }
    reconcile_countdown(app, cx);
    // Whatever is on screen is watched against the engine (R4-3), also a run that only announced
    // itself with `recording.starting`.
    if rec_of(app, cx).read(cx).hud.phase.is_active() {
        start_poll(app, cx);
    }
    sync_tray(app, cx);
}

/// The countdown window exists exactly while the model wants one (R4-2): a resume countdown that
/// ends by `recording.resumed`, a cancelled resume or a lost event leaves no disc on the screen.
fn reconcile_countdown(app: &Entity<AppModel>, cx: &mut App) {
    let rec = rec_of(app, cx);
    let (wanted, have) = {
        let r = rec.read(cx);
        (r.hud.countdown_wanted(), r.countdown_win.is_some())
    };
    if have && !wanted {
        close_countdown(app, cx);
    }
}

/// The event stream lagged and dropped events: ask the engine what is going on and catch up. A run
/// the model missed is adopted (HUD, indicator and tray appear: NFR-11), one that ended is closed
/// by the idle polls.
pub fn resync(app: &Entity<AppModel>, cx: &mut App) {
    let backend = app.read(cx).backend.clone();
    let Some(daemon) = backend.daemon.clone() else {
        return;
    };
    app::capture_changed(app, None, cx);
    let app2 = app.clone();
    cx.spawn(async move |cx| {
        let answer = backend
            .rt
            .spawn(async move { Backend::call(&daemon, "get_recording_status", json!({})).await })
            .await;
        cx.update(|cx| {
            if let Ok(Ok(v)) = answer {
                let rec = rec_of(&app2, cx);
                let change = rec.update(cx, |r, cx| {
                    let now = Instant::now();
                    let c = if r.hud.phase == Phase::Idle {
                        r.hud.adopt_status(&v, now)
                    } else {
                        r.hud.apply_status(&v, now)
                    };
                    cx.notify();
                    c
                });
                handle_change(&app2, change, cx);
            }
        });
    })
    .detach();
}

fn end_all(app: &Entity<AppModel>, e: &Ended, cx: &mut App) {
    // The agent toast's Stop has nothing left to stop.
    app::dismiss_stop_toasts(app, cx);
    let rec = rec_of(app, cx);
    let (hud, ind, cd, bub) = rec.update(cx, |r, _| {
        r.poll = None;
        r.drop_camera();
        r.dialog_open = false;
        (
            r.hud_win.take(),
            r.indicator_win.take(),
            r.countdown_win.take(),
            r.bubble_win.take(),
        )
    });
    close(cx, hud);
    close(cx, ind);
    close(cx, cd);
    close(cx, bub);
    // A Discard or Restart question for the take that just ended must not outlive it (R4-7).
    app::handle_confirm_msg(
        app,
        ConfirmUiMsg::Close {
            id: crate::confirm::LOCAL_DISCARD_ID.to_owned(),
        },
        cx,
    );
    if let Some(spec) = toasts::ended_toast(e) {
        if let Ended::Stopped {
            capture_id: Some(id),
            ..
        } = e
        {
            app::capture_changed(app, Some(id), cx);
        }
        app::push_toast(app, spec, cx);
    }
}

/// Updates the tray icon, tooltip and menu from the model (REC-5.AC7).
pub fn sync_tray(app: &Entity<AppModel>, cx: &mut App) {
    let rec = rec_of(app, cx);
    let attention = crate::ui::upload_ui::attention();
    let plan = tray::plan_with(&rec.read(cx).hud, Instant::now(), attention);
    if rec.read(cx).last_plan.as_ref() == Some(&plan) {
        return;
    }
    if let Some(t) = &app.read(cx).tray {
        t.apply_plan(&plan);
    }
    rec.update(cx, |r, _| r.last_plan = Some(plan));
}

// ---- the 10 Hz status poll -------------------------------------------------------------------

/// What makes the HUD repaint between engine answers: the whole second of the timer, the half
/// second of the blinking dot, and any change the model reports (R4-17).
fn repaint_key(h: &HudState, now: Instant) -> (i64, i64) {
    (h.recorded_ms(now) / 500, h.phase as i64)
}

fn start_poll(app: &Entity<AppModel>, cx: &mut App) {
    let rec = rec_of(app, cx);
    if rec.read(cx).poll.is_some() {
        return;
    }
    let app2 = app.clone();
    let rec2 = rec.clone();
    let task = cx.spawn(async move |cx| loop {
        let (every, active) = cx.update(|cx| {
            let ph = rec2.read(cx).hud.phase;
            (
                if matches!(ph, Phase::Recording | Phase::Paused | Phase::Resuming) {
                    Duration::from_millis(100)
                } else {
                    Duration::from_millis(250)
                },
                ph.is_active(),
            )
        });
        if !active {
            break;
        }
        cx.background_executor().timer(every).await;
        let backend = cx.update(|cx| app2.read(cx).backend.clone());
        let Some(daemon) = backend.daemon.clone() else {
            break;
        };
        let answer = backend
            .rt
            .spawn(async move { Backend::call(&daemon, "get_recording_status", json!({})).await })
            .await;
        cx.update(|cx| {
            let change = rec2.update(cx, |r, cx| {
                let now = Instant::now();
                let before = repaint_key(&r.hud, now);
                // Without an answer the timer keeps ticking from the clock; the next answer
                // rebases it.
                let c = match &answer {
                    Ok(Ok(v)) => r.hud.apply_status(v, now),
                    _ => Change::None,
                };
                if c != Change::None || before != repaint_key(&r.hud, now) {
                    cx.notify();
                }
                c
            });
            handle_change(&app2, change, cx);
        });
    });
    rec.update(cx, |r, _| r.poll = Some(task));
}

// ---- countdown -------------------------------------------------------------------------------

fn indicator_area(rec: &RecState, cx: &App) -> Option<(Rect, (DisplayId, Rect))> {
    let ind = rec.hud.indicator.as_ref()?;
    if ind.audio {
        // VOC-1: an audio recording has no area. The HUD sits on the display with the pointer.
        let d = pointer_display(cx)?;
        return Some((d.1, d));
    }
    let displays = displays(cx);
    let area = match ind.area.bounds {
        Some(b) => geometry::area_points(b, ind.area.scale),
        None => displays.first()?.1,
    };
    let rects: Vec<Rect> = displays.iter().map(|(_, r)| *r).collect();
    let i = pick_display(&rects, &area)?;
    Some((area, displays[i]))
}

fn show_countdown(app: &Entity<AppModel>, cx: &mut App) {
    let rec = rec_of(app, cx);
    if !rec.read(cx).hud.countdown_wanted() {
        return;
    }
    // Linux: the countdown is part of the banner in the Studio window (a disc over the screen
    // would be in the video, and Wayland cannot place it).
    if banner_instead_of_windows(app, cx) {
        return;
    }
    // The window already shows the digit from the model; open it once.
    if rec.read(cx).countdown_win.is_some() {
        rec.update(cx, |_, cx| cx.notify());
        return;
    }
    // Before `recording.started` there are no indicator facts, so the countdown sits on the
    // display of the start target, else the primary one (restart keeps the area).
    let resolved = {
        let r = rec.read(cx);
        match indicator_area(r, cx) {
            Some((a, d)) => Some((a, d)),
            None => r
                .display
                .or_else(|| displays(cx).first().copied())
                .map(|d| (r.area.unwrap_or(d.1), d)),
        }
    };
    let Some((area, display)) = resolved else {
        return;
    };
    let global = countdown::window_rect(&area, &display.1);
    let (app2, rec2) = (app.clone(), rec.clone());
    rec.update(cx, |r, _| {
        r.reduced_motion = platform_window::reduce_motion()
    });
    // R4-11: the countdown never activates CaptureMann or takes focus from the app being
    // recorded (a keystroke meant for it would land here). The global shortcuts cancel it; a
    // click on the disc makes the window key, and then Space and Esc work.
    let opened = cx.open_window(
        panel_options(display, global, false, false),
        move |window, cx| {
            harden(window, false);
            cx.new(|cx| CountdownView::new(app2, rec2, window, cx))
        },
    );
    if let Ok(h) = opened {
        rec.update(cx, |r, _| r.countdown_win = Some(h));
    }
}

fn close_countdown(app: &Entity<AppModel>, cx: &mut App) {
    let h = rec_of(app, cx).update(cx, |r, _| r.countdown_win.take());
    close(cx, h);
}

// ---- the windows of a live recording ---------------------------------------------------------

fn hud_positions_key(display: &Rect) -> String {
    format!("{:.0},{:.0}", display.x, display.y)
}

/// REC-5.AC4: where the HUD opens on `display`: the remembered position, else bottom center.
pub fn hud_rect(
    app: &Entity<AppModel>,
    display: &Rect,
    agent: Option<&str>,
    audio: bool,
    cx: &App,
) -> Rect {
    // The window is exactly the bar under the strip; an agent chip makes it wider, and the audio
    // bar is its own (narrower) one.
    let window = crate::recording::hud_layout::window_size_for(audio, agent);
    let bar = (window.0, HUD_BAR.1);
    let stored = app
        .read(cx)
        .backend
        .settings
        .snapshot()
        .get(start::KEY_HUD_POSITIONS)
        .and_then(|v| v.get(hud_positions_key(display)))
        .and_then(|v| Some((v.get(0)?.as_f64()?, v.get(1)?.as_f64()?)));
    match stored {
        Some(f) => snap_hud(&rect_from_center_fraction(f, window, display), display),
        None => {
            // Bottom centre stays the default (OD-20, Q13).
            let bar = hud_default(display, bar);
            Rect::new(bar.x, bar.y - HUD_STRIP, window.0, window.1)
        }
    }
}

/// Remembers the HUD's position for its display (center fractions).
pub fn remember_hud(app: &Entity<AppModel>, display: &Rect, hud: &Rect, cx: &mut App) {
    let f = geometry::center_fraction(hud, display);
    let mut all = app
        .read(cx)
        .backend
        .settings
        .snapshot()
        .get(start::KEY_HUD_POSITIONS)
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    all.insert(hud_positions_key(display), json!([f.0, f.1]));
    app::set_setting(app, start::KEY_HUD_POSITIONS, Value::Object(all), cx);
}

/// Whether the HUD, the countdown disc and the area brackets are left out because the controls
/// are the banner in the Studio window (`recording::surface`). Makes sure that window exists, so
/// a recording started from the tray, a hotkey, the CLI or an agent still has its indicator.
fn banner_instead_of_windows(app: &Entity<AppModel>, cx: &mut App) -> bool {
    if crate::recording::surface::surface() != crate::recording::surface::Surface::InWindow {
        return false;
    }
    if !app.read(cx).studio_open {
        app::open_studio(app, cx);
    }
    true
}

fn open_recording_windows(app: &Entity<AppModel>, cx: &mut App) {
    let rec = rec_of(app, cx);
    let area = indicator_area(rec.read(cx), cx);
    if banner_instead_of_windows(app, cx) {
        rec.update(cx, |r, _| {
            r.reduced_motion = platform_window::reduce_motion();
            if let Some((area, display)) = area {
                r.area = Some(area);
                r.display = Some(display);
            }
        });
        sync_tray(app, cx);
        return;
    }
    let Some((area, display)) = area else {
        return;
    };
    // Facts first: bubble preferences from the recording, then the windows.
    rec.update(cx, |r, _| {
        r.reduced_motion = platform_window::reduce_motion();
        r.area = Some(area);
        r.display = Some(display);
        if let Some(c) = r.hud.indicator.as_ref().and_then(|i| i.camera.clone()) {
            r.prefs = BubblePrefs {
                shape: c.shape,
                size: c.size,
                mirror: c.mirror,
                position: c.position,
            };
            r.frames.set_mirror(c.mirror);
        }
    });
    // Replace anything a previous take left (restart keeps the HUD and the bubble).
    let have_hud = rec.read(cx).hud_win.is_some();
    if !have_hud {
        let agent = rec
            .read(cx)
            .hud
            .started_by_agent()
            .map(|(client, _)| client.to_owned());
        let audio = rec.read(cx).hud.is_audio();
        let rect = hud_rect(app, &display.1, agent.as_deref(), audio, cx);
        let (app2, rec2) = (app.clone(), rec.clone());
        let opened = cx.open_window(
            panel_options(display, rect, false, true),
            move |window, cx| {
                harden(window, false);
                cx.new(|cx| HudView::new(app2, rec2, window, cx))
            },
        );
        if let Ok(h) = opened {
            rec.update(cx, |r, _| r.hud_win = Some(h));
        }
    }
    let old = rec.update(cx, |r, _| r.indicator_win.take());
    close(cx, old);
    // An audio recording has no area to outline (VOC-1): the HUD and the tray are its indicator.
    if !rec.read(cx).hud.is_audio()
        && crate::recording::surface::shows_area_indicator(crate::permissions::Os::current())
    {
        let (app2, rec2) = (app.clone(), rec.clone());
        let opened = cx.open_window(
            panel_options(display, display.1, false, false),
            move |window, cx| {
                harden(window, true);
                cx.new(|cx| IndicatorView::new(app2, rec2, display.1, window, cx))
            },
        );
        if let Ok(h) = opened {
            // AppKit clamps a new window below the menu bar; the indicator covers the whole
            // display so the brackets line up with the recorded pixels.
            let d = display.1;
            let fix = move |cx: &mut App| {
                let _ = h.update(cx, |_, window, _| {
                    platform_window::set_frame(window, d.x, d.y, d.w, d.h);
                });
            };
            fix(cx);
            // AppKit may clamp again when the window is first ordered front.
            cx.spawn(async move |cx| {
                for ms in [60u64, 250] {
                    cx.background_executor()
                        .timer(Duration::from_millis(ms))
                        .await;
                    cx.update(|cx| fix(cx));
                }
            })
            .detach();
            rec.update(cx, |r, _| r.indicator_win = Some(h));
        }
    }
    if rec.read(cx).hud.camera_on() {
        let hidden = rec.read(cx).hud.bubble_hidden;
        if !hidden {
            open_bubble(app, cx);
        }
        if rec.read(cx).preview.is_none() {
            let device = rec.read(cx).camera_device.clone();
            start_preview(app, device, cx);
        }
    }
    sync_tray(app, cx);
}

// ---- the bubble ------------------------------------------------------------------------------

/// The bubble's outer rectangle inside the current area (the window adds the toolbar band).
pub fn bubble_rect(rec: &RecState) -> Option<Rect> {
    let area = rec.area?;
    let dims = bubble_dims(rec.prefs.size, rec.prefs.shape);
    Some(rec.prefs.position.rect(&area, dims))
}

/// Opens (or reopens) the bubble window from the preferences.
pub fn open_bubble(app: &Entity<AppModel>, cx: &mut App) {
    let rec = rec_of(app, cx);
    let old = rec.update(cx, |r, _| r.bubble_win.take());
    close(cx, old);
    let (Some(rect), Some(display)) = ({
        let r = rec.read(cx);
        (bubble_rect(r), r.display)
    }) else {
        return;
    };
    // The window is the bubble plus the toolbar band (above, or below near the top edge).
    let win = bubble_layout(&rect, &display.1).window;
    let (app2, rec2) = (app.clone(), rec.clone());
    let opened = cx.open_window(
        panel_options(display, win, false, true),
        move |window, cx| {
            harden(window, false);
            cx.new(|cx| BubbleView::new(app2, rec2, window, cx))
        },
    );
    if let Ok(h) = opened {
        rec.update(cx, |r, _| r.bubble_win = Some(h));
    }
}

pub fn close_bubble(app: &Entity<AppModel>, cx: &mut App) {
    let h = rec_of(app, cx).update(cx, |r, _| r.bubble_win.take());
    close(cx, h);
}

/// Starts the camera preview into the shared frame slot, off the UI thread.
pub fn start_preview(app: &Entity<AppModel>, device: Option<String>, cx: &mut App) {
    let rec = rec_of(app, cx);
    let (slot, source) = {
        let r = rec.read(cx);
        (r.frames.clone(), r.preview_source)
    };
    rec.update(cx, |r, _| r.preview = None);
    slot.clear();
    let (app2, rec2) = (app.clone(), rec);
    cx.spawn(async move |cx| {
        let s = slot.clone();
        let result = cx
            .background_executor()
            .spawn(async move { crate::recording::preview::start(source, device, s) })
            .await;
        cx.update(|cx| match result {
            Ok(run) => rec2.update(cx, |r, cx| {
                r.preview = Some(run);
                cx.notify();
            }),
            Err(e) => {
                slot.set_error(Some(e.to_string()));
                app::push_toast(
                    &app2,
                    ToastSpec::new(
                        ToastKind::Warning,
                        tr!("The camera preview didn\u{2019}t start"),
                    )
                    .body(e.to_string()),
                    cx,
                );
            }
        });
    })
    .detach();
}

/// Applies a change of the bubble's look: updates the preferences, remembers them (CAM-1) and
/// redraws the window.
pub fn set_bubble(app: &Entity<AppModel>, f: impl FnOnce(&mut BubblePrefs), cx: &mut App) {
    let rec = rec_of(app, cx);
    let prefs = rec.update(cx, |r, cx| {
        f(&mut r.prefs);
        r.frames.set_mirror(r.prefs.mirror);
        cx.notify();
        r.prefs
    });
    for (key, v) in [
        (start::KEY_BUBBLE_SHAPE, json!(prefs.shape.key())),
        (start::KEY_BUBBLE_SIZE, json!(prefs.size.key())),
        (start::KEY_BUBBLE_MIRROR, json!(prefs.mirror)),
        (start::KEY_BUBBLE_POSITION, prefs.position.to_json()),
    ] {
        app::set_setting(app, key, v, cx);
    }
    if rec.read(cx).bubble_win.is_some() {
        open_bubble(app, cx);
    }
}

/// HUD Webcam button: shows or hides the bubble. The camera keeps recording (CAM-1: hiding does not
/// stop the camera track).
pub fn toggle_bubble(app: &Entity<AppModel>, cx: &mut App) {
    let rec = rec_of(app, cx);
    let hidden = rec.update(cx, |r, cx| {
        r.hud.bubble_hidden = !r.hud.bubble_hidden;
        cx.notify();
        r.hud.bubble_hidden
    });
    if hidden {
        close_bubble(app, cx);
    } else {
        open_bubble(app, cx);
    }
}

// ---- actions ---------------------------------------------------------------------------------

/// One recording action as the user (no confirmation: the user is at the machine).
fn call(
    app: &Entity<AppModel>,
    method: &'static str,
    params: Value,
    on_error: impl FnOnce(&Entity<AppModel>, crate::backend::CallError, &mut App) + 'static,
    cx: &mut App,
) {
    call_with(app, method, params, |_, _, _| {}, on_error, cx);
}

/// [`call`] that also reports the answer.
fn call_with(
    app: &Entity<AppModel>,
    method: &'static str,
    params: Value,
    on_ok: impl FnOnce(&Entity<AppModel>, Value, &mut App) + 'static,
    on_error: impl FnOnce(&Entity<AppModel>, crate::backend::CallError, &mut App) + 'static,
    cx: &mut App,
) {
    call_after(app, None, method, params, on_ok, on_error, cx);
}

/// [`call_with`] that first writes `settings` (a `set_settings` `values` object) and waits for
/// it, so the engine's snapshot at the start of a recording sees them (the layout at stop
/// compares `camera.*` against that snapshot).
fn call_after(
    app: &Entity<AppModel>,
    settings: Option<Value>,
    method: &'static str,
    params: Value,
    on_ok: impl FnOnce(&Entity<AppModel>, Value, &mut App) + 'static,
    on_error: impl FnOnce(&Entity<AppModel>, crate::backend::CallError, &mut App) + 'static,
    cx: &mut App,
) {
    let backend = app.read(cx).backend.clone();
    let Some(daemon) = backend.daemon.clone() else {
        return;
    };
    let app2 = app.clone();
    cx.spawn(async move |cx| {
        let res = backend
            .rt
            .spawn(async move {
                if let Some(values) = settings {
                    let _ =
                        Backend::call(&daemon, "set_settings", json!({ "values": values })).await;
                }
                Backend::call_coded(&daemon, method, params).await
            })
            .await;
        cx.update(|cx| match res {
            Ok(Ok(v)) => on_ok(&app2, v, cx),
            Ok(Err(e)) => on_error(&app2, e, cx),
            Err(e) => on_error(&app2, crate::backend::CallError::message(e.to_string()), cx),
        });
    })
    .detach();
}

fn error_toast(app: &Entity<AppModel>, e: &crate::backend::CallError, cx: &mut App) {
    app::push_toast(
        app,
        toasts::start_error_toast(e.code, e.permission, &e.message),
        cx,
    );
}

/// A `start_recording` or `restart_recording` call failed. Until `recording.started` arrives the
/// take does not exist, so the model goes back to idle and every window closes: the countdown
/// disc, the tray's red icon, the camera preview the sheet left running (R4-1, R4-4). The engine's
/// own `recording.discarded { start_failed }` may have done this already, and then the toast has
/// been shown too.
fn start_failed(app: &Entity<AppModel>, e: crate::backend::CallError, cx: &mut App) {
    let rec = rec_of(app, cx);
    let cancelled = e.code == Some(cm_core::ErrorCode::UserDeclined);
    let busy = e.code == Some(cm_core::ErrorCode::Busy);
    let reset = rec.update(cx, |r, cx| {
        let reset = r.fail_start(busy);
        cx.notify();
        reset
    });
    if reset {
        end_all(app, &Ended::Cancelled, cx);
    }
    // Idle without a reset means the engine's event already ended the run and told the user.
    if !cancelled && (reset || rec.read(cx).hud.phase != Phase::Idle) {
        error_toast(app, &e, cx);
    }
    if busy {
        resync(app, cx);
    }
    sync_tray(app, cx);
}

/// The title and body telling why recording is off (the toast, the launcher row).
pub fn blocker_text(blocker: RecordingBlocker) -> (String, String) {
    match blocker {
        RecordingBlocker::NotAvailableYet => (
            tr!("Recording is not available yet"),
            tr!("Screen and audio recording is not built for this system yet. Screenshots, the library and the editor work."),
        ),
        RecordingBlocker::NoLibrary => (
            tr!("Recording is unavailable"),
            tr!("The library couldn\u{2019}t be opened, so there is nowhere to save recordings."),
        ),
    }
}

/// Shows why and returns true when recording cannot start now. Every Record entry point (the
/// sheet, the hotkeys, the tray, the launcher) asks this one function.
fn refuse_recording(app: &Entity<AppModel>, backend: &Backend, cx: &mut App) -> bool {
    let Some(blocker) = backend.recording_blocker() else {
        return false;
    };
    let (title, body) = blocker_text(blocker);
    let kind = if blocker == RecordingBlocker::NotAvailableYet {
        ToastKind::Info
    } else {
        ToastKind::Error
    };
    app::push_toast(app, ToastSpec::new(kind, title).body(body), cx);
    true
}

/// Starts a recording with the sheet's (or the remembered) choices.
pub fn start(app: &Entity<AppModel>, opts: StartOptions, cx: &mut App) {
    start_with(app, opts, Map::new(), cx);
}

/// [`start`] plus `extra` `start_recording` parameters (a launcher action's one-off `fps` or
/// `countdown_s`, LNC-1 Actions).
pub fn start_with(
    app: &Entity<AppModel>,
    opts: StartOptions,
    extra: Map<String, Value>,
    cx: &mut App,
) {
    let rec = rec_of(app, cx);
    if rec.read(cx).hud.phase.is_active() {
        return;
    }
    let sheet = rec.update(cx, |r, cx| {
        r.hud.begin_start();
        r.camera_device = opts.camera.as_ref().and_then(|c| c.device.clone());
        if let Some(c) = &opts.camera {
            r.prefs = BubblePrefs {
                shape: c.shape,
                size: c.size,
                mirror: c.mirror,
                position: c.position,
            };
            r.frames.set_mirror(c.mirror);
        }
        cx.notify();
        r.sheet_win.take()
    });
    close(cx, sheet);
    // The setup bubble (CAM-4) goes; the recording opens its own at `recording.started`.
    close_bubble(app, cx);
    // Remember what the user chose for next time. The write is awaited before the start so the
    // engine's settings snapshot holds the position this recording starts at: a later drag back
    // onto an older stored position is then still seen as a change.
    let mut params = opts.to_params();
    if let Some(p) = params.as_object_mut() {
        p.extend(extra);
    }
    let values = Value::Object(opts.remembered());
    call_after(
        app,
        Some(values),
        "start_recording",
        params,
        |_, _, _| {},
        start_failed,
        cx,
    );
    sync_tray(app, cx);
}

pub fn stop(app: &Entity<AppModel>, cx: &mut App) {
    call(
        app,
        "stop_recording",
        json!({}),
        |app, e, cx| {
            app::push_toast(
                app,
                crate::flows::error_toast_live(
                    || tr!("Couldn\u{2019}t stop the recording"),
                    e.message.clone(),
                ),
                cx,
            );
        },
        cx,
    );
}

/// The agent toast's "Stop": the HUD's Stop path, only while that recording still runs. The toast
/// goes away either way (the caller dismisses it).
pub fn stop_recording_if_running(app: &Entity<AppModel>, id: &str, cx: &mut App) {
    let (phase, current) = {
        let rec = rec_of(app, cx);
        let r = rec.read(cx);
        (r.hud.phase, r.hud.recording_id.clone())
    };
    if toasts::stop_applies(phase, current.as_deref(), id) {
        stop(app, cx);
    }
}

pub fn toggle_pause(app: &Entity<AppModel>, cx: &mut App) {
    let paused = rec_of(app, cx).read(cx).hud.is_paused();
    let method = if paused {
        "resume_recording"
    } else {
        "pause_recording"
    };
    call(
        app,
        method,
        json!({}),
        |app, e, cx| {
            app::push_toast(
                app,
                crate::flows::error_toast_live(
                    || tr!("Couldn\u{2019}t change the recording"),
                    e.message.clone(),
                ),
                cx,
            );
        },
        cx,
    );
}

/// The HUD's Mic button (AUD-6): mutes or unmutes the microphone track of the running take.
/// The model follows the `recording.mic_muted` / `mic_unmuted` events (the same ones an agent's
/// or the CLI's call publishes), so nothing is guessed here.
pub fn toggle_mic_mute(app: &Entity<AppModel>, cx: &mut App) {
    let muted = rec_of(app, cx).read(cx).hud.mic_is_muted();
    call(
        app,
        "set_mic_muted",
        json!({ "muted": !muted }),
        |app, e, cx| {
            app::push_toast(
                app,
                crate::flows::error_toast_live(
                    || tr!("Couldn\u{2019}t change the microphone"),
                    e.message.clone(),
                ),
                cx,
            );
        },
        cx,
    );
}

/// The HUD's Marker button, the M key and Option+Shift+M (VOC-1): marks the running recording at
/// the HUD timer. Pauses are not in that time, so a marker never shifts. The label prompt follows
/// from the `recording.marker` event.
pub fn add_marker(app: &Entity<AppModel>, cx: &mut App) {
    let phase = rec_of(app, cx).read(cx).hud.phase;
    if !matches!(phase, Phase::Recording | Phase::Paused | Phase::Resuming) {
        app::push_toast(
            app,
            ToastSpec::new(ToastKind::Info, tr!("No recording to mark")).body(tr!(
                "Start a recording first, then add markers while it runs."
            )),
            cx,
        );
        return;
    }
    call(
        app,
        "add_marker",
        json!({}),
        |app, e, cx| {
            app::push_toast(
                app,
                crate::flows::error_toast_live(
                    || tr!("Couldn\u{2019}t add the marker"),
                    e.message.clone(),
                ),
                cx,
            );
        },
        cx,
    );
}

/// Names the marker set at `at_ms` (the label typed in the HUD's prompt, VOC-1): the same
/// `add_marker` action, which gives the marker at that recorded time its label.
pub fn label_marker(app: &Entity<AppModel>, at_ms: i64, label: &str, cx: &mut App) {
    let label = label.trim();
    if label.is_empty() {
        return;
    }
    call(
        app,
        "add_marker",
        json!({ "at": at_ms as f64 / 1000.0, "label": label }),
        |app, e, cx| {
            app::push_toast(
                app,
                crate::flows::error_toast_live(
                    || tr!("Couldn\u{2019}t label the marker"),
                    e.message.clone(),
                ),
                cx,
            );
        },
        cx,
    );
}

/// Closes the HUD's label prompt (Enter after a label, Esc, a click elsewhere).
pub fn close_marker_prompt(app: &Entity<AppModel>, cx: &mut App) {
    rec_of(app, cx).update(cx, |r, cx| {
        if r.hud.close_marker_prompt(None) {
            cx.notify();
        }
    });
}

/// Space during the countdown (REC-1.AC5).
pub fn skip_countdown(app: &Entity<AppModel>, cx: &mut App) {
    call(app, "skip_countdown", json!({}), |_, _, _| {}, cx);
}

/// Esc, or the shortcuts, on the countdown (REC-1.AC5). What it means depends on the phase
/// (R4-2): the start countdown is cancelled with nothing recorded; the resume countdown is
/// cancelled and the take stays paused; anything else is not a countdown and is left alone, so a
/// stray Esc can never delete a take.
pub fn cancel_countdown(app: &Entity<AppModel>, cx: &mut App) {
    let (action, id) = {
        let r = rec_of(app, cx).read(cx);
        (r.hud.esc_action(), r.hud.recording_id.clone())
    };
    match action {
        EscAction::CancelStart => {
            let params = match id {
                Some(id) => json!({ "recording_id": id }),
                None => json!({}),
            };
            call(app, "discard_recording", params, |_, _, _| {}, cx);
        }
        EscAction::CancelResume => {
            // The engine goes back to paused without an event of its own; settle the model now.
            call_with(
                app,
                "pause_recording",
                json!({}),
                |app, _, cx| {
                    let rec = rec_of(app, cx);
                    rec.update(cx, |r, cx| {
                        r.hud.resume_cancelled();
                        cx.notify();
                    });
                    reconcile_countdown(app, cx);
                    sync_tray(app, cx);
                },
                |_, _, _| {},
                cx,
            );
        }
        EscAction::Ignore => {}
    }
}

/// REC-5.AC3 / AC12: Discard and Restart ask first; cancelling keeps the recording.
pub fn ask_discard(app: &Entity<AppModel>, restart: bool, cx: &mut App) {
    let rec = rec_of(app, cx);
    {
        let r = rec.read(cx);
        if r.dialog_open || !matches!(r.hud.phase, Phase::Recording | Phase::Paused) {
            return;
        }
    }
    let (recorded, title, asked_about) = {
        let r = rec.read(cx);
        (
            r.hud.timer_text(Instant::now()),
            r.hud.title.clone(),
            r.hud.recording_id.clone(),
        )
    };
    let model = discard_dialog(restart, &recorded, title.as_deref());
    let (tx, rx) = tokio::sync::oneshot::channel();
    rec.update(cx, |r, _| r.dialog_open = true);
    app::handle_confirm_msg(app, ConfirmUiMsg::Show { model, reply: tx }, cx);
    let (app2, rec2) = (app.clone(), rec);
    cx.spawn(async move |cx| {
        let decision = rx.await.unwrap_or(cm_daemon::Decision::Decline);
        cx.update(|cx| {
            // The window is gone; forget its entry so the list does not grow (R4-23).
            app::handle_confirm_msg(
                &app2,
                ConfirmUiMsg::Close {
                    id: crate::confirm::LOCAL_DISCARD_ID.to_owned(),
                },
                cx,
            );
            rec2.update(cx, |r, _| r.dialog_open = false);
            if decision == cm_daemon::Decision::Decline {
                return;
            }
            // R4-7: an approval belongs to the take it was asked about. If that take ended and
            // another began meanwhile, the answer is dropped.
            if !rec2
                .read(cx)
                .hud
                .dialog_still_applies(asked_about.as_deref())
            {
                return;
            }
            let (method, failed) = if restart {
                ("restart_recording", tr!("Couldn\u{2019}t restart the take"))
            } else {
                ("discard_recording", tr!("Couldn\u{2019}t discard the take"))
            };
            let params = match &asked_about {
                Some(id) => json!({ "recording_id": id }),
                None => json!({}),
            };
            call(
                &app2,
                method,
                params,
                move |app, e, cx| {
                    if restart && rec_of(app, cx).update(cx, |r, _| r.fail_start(false)) {
                        // The old take is gone and the new one never began (R4-1).
                        end_all(app, &Ended::Cancelled, cx);
                        error_toast(app, &e, cx);
                        sync_tray(app, cx);
                        return;
                    }
                    app::push_toast(
                        app,
                        crate::flows::error_toast(&failed, crate::daemon_text::call_error_text(&e)),
                        cx,
                    );
                },
                cx,
            );
        });
    })
    .detach();
}

// ---- hotkeys, tray and launcher entry points -------------------------------------------------

/// ⌥⇧R: toggles start and stop (CAP-2).
pub fn toggle_hotkey(app: &Entity<AppModel>, cx: &mut App) {
    let phase = rec_of(app, cx).read(cx).hud.phase;
    match tray::toggle_action(phase) {
        ToggleAction::Stop => {
            if phase == Phase::Countdown {
                cancel_countdown(app, cx);
            } else {
                stop(app, cx);
            }
        }
        ToggleAction::Start => quick_start(app, cx),
        ToggleAction::Ignore => {}
    }
}

/// ⌥⇧V: starts an audio recording with the last used preset and sources, or stops the running
/// one (VOC-1.AC3). While a screen recording runs it does nothing but say so.
pub fn toggle_audio_hotkey(app: &Entity<AppModel>, cx: &mut App) {
    let (phase, audio) = {
        let r = rec_of(app, cx).read(cx);
        (r.hud.phase, r.hud.is_audio())
    };
    match tray::toggle_action(phase) {
        ToggleAction::Stop if audio => toggle_hotkey(app, cx),
        ToggleAction::Stop => app::push_toast(
            app,
            ToastSpec::new(ToastKind::Info, tr!("A recording is already running")),
            cx,
        ),
        ToggleAction::Start => quick_start_audio(app, cx),
        ToggleAction::Ignore => {}
    }
}

/// ⌥⇧M: adds a marker to the running recording (VOC-1.AC4).
pub fn add_marker_hotkey(app: &Entity<AppModel>, cx: &mut App) {
    add_marker(app, cx);
}

/// Starts an audio recording with the remembered preset and sources, without the sheet (the
/// shortcut and the launcher). The consent line, which the sheet would have shown, is shown as a
/// toast while `recording.consent_notice` is on.
pub fn quick_start_audio(app: &Entity<AppModel>, cx: &mut App) {
    let backend = app.read(cx).backend.clone();
    if refuse_recording(app, &backend, cx) {
        return;
    }
    let defaults =
        crate::recording::audio_sheet::AudioDefaults::from_settings(&backend.settings.snapshot());
    let consent = defaults.consent_notice;
    start_audio(app, defaults.quick_params(), None, cx);
    if consent {
        app::push_toast(
            app,
            ToastSpec::new(ToastKind::Info, tr!("Recording audio"))
                .body(crate::recording::audio_sheet::AudioSheetModel::consent_text()),
            cx,
        );
    }
}

/// Starts an audio recording (`start_recording { mode: "audio" }`): the user's own start, so no
/// Confirm dialog. `remember` is written first (the sheet's choices).
pub fn start_audio(
    app: &Entity<AppModel>,
    params: Value,
    remember: Option<Map<String, Value>>,
    cx: &mut App,
) {
    let rec = rec_of(app, cx);
    if rec.read(cx).hud.phase.is_active() {
        return;
    }
    let sheet = rec.update(cx, |r, cx| {
        r.hud.begin_start();
        r.camera_device = None;
        cx.notify();
        r.sheet_win.take()
    });
    close(cx, sheet);
    let mut remember = remember.unwrap_or_default();
    remember.insert(
        crate::recording::audio_sheet::KEY_SEEN.to_owned(),
        json!(true),
    );
    call_after(
        app,
        Some(Value::Object(remember)),
        "start_recording",
        params,
        |_, _, _| {},
        start_failed,
        cx,
    );
    sync_tray(app, cx);
}

/// ⌥⇧P: pause or resume; during the countdown it cancels it (REC-4.AC4).
pub fn pause_hotkey(app: &Entity<AppModel>, cx: &mut App) {
    let phase = rec_of(app, cx).read(cx).hud.phase;
    match tray::pause_action(phase) {
        PauseAction::Pause | PauseAction::Resume => toggle_pause(app, cx),
        PauseAction::CancelCountdown => cancel_countdown(app, cx),
        PauseAction::Ignore => {}
    }
}

/// Starts straight away with the remembered options on the primary display (the hotkey path).
pub fn quick_start(app: &Entity<AppModel>, cx: &mut App) {
    let backend = app.read(cx).backend.clone();
    if refuse_recording(app, &backend, cx) {
        return;
    }
    let (daemon, rt) = (backend.daemon.clone(), backend.rt.clone());
    // R4-5: read what the OS allows (never asks) and start without a microphone or camera it has
    // not allowed yet, instead of failing the whole start.
    app::refresh_permissions(app, cx);
    let (defaults, dropped) = {
        let m = app.read(cx);
        crate::recording::start::quick_defaults(
            &SheetDefaults::from_settings(&backend.settings.snapshot()),
            m.perms.state(crate::permissions::Id::Microphone),
            m.perms.state(crate::permissions::Id::Camera),
        )
    };
    let app2 = app.clone();
    cx.spawn(async move |cx| {
        let Some(daemon) = daemon else { return };
        let primary = rt
            .spawn(async move {
                Backend::call_coded(&daemon, "list_targets", json!({ "windows": false })).await
            })
            .await;
        cx.update(|cx| {
            // A failed listing is not "no display": say why (a missing Screen Recording grant
            // points at Permissions).
            let listed = match primary {
                Ok(Ok(v)) => Ok(crate::recording::sheet::parse_displays(&v)),
                Ok(Err(e)) => Err((e.code, e.message)),
                Err(e) => Err((None, e.to_string())),
            };
            let displays = match listed {
                Ok(d) => d,
                Err((code, message)) => {
                    app::push_toast(
                        &app2,
                        crate::recording::toasts::start_error_toast(
                            code,
                            Some(cm_core::Permission::Screen),
                            &message,
                        ),
                        cx,
                    );
                    return;
                }
            };
            let Some(d) = displays.iter().find(|d| d.primary).or(displays.first()) else {
                app::push_toast(
                    &app2,
                    ToastSpec::new(ToastKind::Error, tr!("Recording didn\u{2019}t start"))
                        .body(tr!("No display was found.")),
                    cx,
                );
                return;
            };
            let opts = defaults.options(Target::Display { display_id: d.id });
            start(&app2, opts, cx);
            if !dropped.is_empty() {
                // Whole sentences for each case (microphone, camera, both), never joined pieces.
                let (title, body) = match (
                    dropped.contains(&"microphone"),
                    dropped.contains(&"camera"),
                ) {
                    (true, true) => (
                        tr!("Recording without the microphone and camera"),
                        tr!("Access to the microphone and camera isn\u{2019}t allowed yet. Turn it on in Settings \u{2192} Permissions."),
                    ),
                    (true, false) => (
                        tr!("Recording without the microphone"),
                        tr!("Access to the microphone isn\u{2019}t allowed yet. Turn it on in Settings \u{2192} Permissions."),
                    ),
                    _ => (
                        tr!("Recording without the camera"),
                        tr!("Access to the camera isn\u{2019}t allowed yet. Turn it on in Settings \u{2192} Permissions."),
                    ),
                };
                app::push_toast(&app2, ToastSpec::new(ToastKind::Info, title).body(body), cx);
            }
        });
    })
    .detach();
}

/// Opens the start sheet (the launcher command and the tray's "Record…"). `target` pre-fills a
/// window or region picked first.
pub fn open_sheet(
    app: &Entity<AppModel>,
    target: Option<Target>,
    camera: Option<bool>,
    cx: &mut App,
) {
    open_sheet_with(app, target, camera, start::SheetPreset::default(), cx);
}

/// [`open_sheet`] with a launcher action's one-off choices pre-filled (LNC-1 Actions).
pub fn open_sheet_with(
    app: &Entity<AppModel>,
    target: Option<Target>,
    camera: Option<bool>,
    preset: start::SheetPreset,
    cx: &mut App,
) {
    open_sheet_mode(app, target, camera, preset, sheet::SheetMode::Screen, cx);
}

/// Opens the start sheet in Audio mode (the tray's "Record audio", the launcher, the mic button;
/// VOC-1).
pub fn open_audio_sheet(app: &Entity<AppModel>, cx: &mut App) {
    open_sheet_mode(
        app,
        None,
        None,
        start::SheetPreset::default(),
        sheet::SheetMode::Audio,
        cx,
    );
}

/// [`open_sheet_with`] in a given mode. A sheet that is already open is brought to the front
/// and switched to `mode`.
pub fn open_sheet_mode(
    app: &Entity<AppModel>,
    target: Option<Target>,
    camera: Option<bool>,
    preset: start::SheetPreset,
    mode: sheet::SheetMode,
    cx: &mut App,
) {
    // PRM-1: read the permission states when the sheet opens (never asks).
    app::refresh_permissions(app, cx);
    let rec = rec_of(app, cx);
    if rec.read(cx).hud.phase.is_active() {
        app::push_toast(
            app,
            ToastSpec::new(ToastKind::Info, tr!("A recording is already running")),
            cx,
        );
        return;
    }
    if let Some(h) = rec.read(cx).sheet_win {
        if h.update(cx, |this, window, cx| {
            this.set_mode(mode, window, cx);
            window.activate_window()
        })
        .is_ok()
        {
            return;
        }
    }
    let backend = app.read(cx).backend.clone();
    if refuse_recording(app, &backend, cx) {
        return;
    }
    let defaults = SheetDefaults::from_settings(&backend.settings.snapshot());
    // The display with the pointer (where the user just pressed the hotkey or chose the
    // launcher command), else the primary one.
    let Some(display) = pointer_display(cx) else {
        return;
    };
    let visible = visible_part(&display.1);
    if std::env::var_os("CM_DEMO_SHEET_TRACE").is_some() {
        log::info!(
            "sheet: displays {:?} chosen {:?} visible {visible:?}",
            displays(cx),
            display
        );
    }
    // Height for the rows the first frame shows; the sheet re-fits when the devices arrive.
    let mut first = crate::recording::sheet::SheetModel::new(&defaults);
    if let Some(t) = target {
        first.pick(t);
    }
    let (width, heights) = match mode {
        sheet::SheetMode::Screen => (sheet::SHEET_WIDTH, first.heights()),
        sheet::SheetMode::Audio => (
            crate::recording::audio_sheet::AUDIO_SHEET_WIDTH,
            crate::recording::audio_sheet::AudioSheetModel::new(
                &crate::recording::audio_sheet::AudioDefaults::from_settings(
                    &backend.settings.snapshot(),
                ),
            )
            .heights(),
        ),
    };
    let h = crate::recording::sheet::fit_height(&heights, visible.h);
    let global = crate::recording::sheet::initial_frame(&visible, width, h);
    let (app2, rec2) = (app.clone(), rec.clone());
    let opened = cx.open_window(
        panel_options(display, global, true, false),
        move |window, cx| {
            platform_window::exclude_from_capture(window);
            cx.new(|cx| {
                let open = sheet::SheetOpen {
                    target,
                    camera,
                    preset,
                    height: global.h,
                    display: display.1,
                    mode,
                };
                StartSheet::new(app2, rec2, defaults, open, window, cx)
            })
        },
    );
    if let Ok(handle) = opened {
        rec.update(cx, |r, cx| {
            r.sheet_win = Some(handle);
            cx.notify(); // the main window's scrim follows the sheet
        });
        cx.activate(true);
    }
}

/// Region and window picks go through the overlay in `--pick` mode (no screenshot is taken).
pub fn pick_then_sheet(app: &Entity<AppModel>, mode: crate::overlay::CaptureMode, cx: &mut App) {
    pick_then_sheet_with(app, mode, None, start::SheetPreset::default(), cx);
}

/// [`pick_then_sheet`] with a launcher action's camera choice and one-off preset (LNC-1 Actions).
pub fn pick_then_sheet_with(
    app: &Entity<AppModel>,
    mode: crate::overlay::CaptureMode,
    camera: Option<bool>,
    preset: start::SheetPreset,
    cx: &mut App,
) {
    let backend = app.read(cx).backend.clone();
    let app2 = app.clone();
    cx.spawn(async move |cx| {
        let b = backend.clone();
        let res = backend
            .rt
            .spawn(async move { b.overlay.pick(mode).await })
            .await;
        cx.update(|cx| match res {
            Ok(Ok(Some(p))) => open_sheet_with(&app2, Some(p.target()), camera, preset, cx),
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

/// Whether a recording (or its countdown) is on screen.
pub fn is_active(app: &Entity<AppModel>, cx: &App) -> bool {
    rec_of(app, cx).read(cx).hud.phase.is_active()
}

/// Closes the start sheet and its setup bubble without recording.
pub fn cancel_sheet(app: &Entity<AppModel>, cx: &mut App) {
    let rec = rec_of(app, cx);
    let (sheet, was_setup) = rec.update(cx, |r, cx| {
        let s = r.sheet_win.take();
        cx.notify(); // the main window's scrim follows the sheet
        let setup = std::mem::take(&mut r.setup_preview);
        if setup {
            r.preview = None;
            r.frames.clear();
        }
        (s, setup)
    });
    close(cx, sheet);
    if was_setup {
        close_bubble(app, cx);
    }
}

/// The sheet's camera toggle: shows or hides the live preview bubble before recording (CAM-4).
pub fn set_setup_camera(
    app: &Entity<AppModel>,
    on: bool,
    device: Option<String>,
    area: Option<Rect>,
    cx: &mut App,
) {
    let rec = rec_of(app, cx);
    if !on {
        let was = rec.update(cx, |r, _| {
            r.preview = None;
            r.frames.clear();
            std::mem::take(&mut r.setup_preview)
        });
        if was {
            close_bubble(app, cx);
        }
        return;
    }
    let defaults = SheetDefaults::from_settings(&app.read(cx).backend.settings.snapshot());
    let display = area
        .and_then(|a| display_for(cx, &a))
        .or_else(|| displays(cx).first().copied());
    let Some(display) = display else { return };
    rec.update(cx, |r, cx| {
        r.display = Some(display);
        r.area = Some(area.unwrap_or(display.1));
        r.prefs = BubblePrefs {
            shape: defaults.bubble.shape,
            size: defaults.bubble.size,
            mirror: defaults.bubble.mirror,
            position: defaults.bubble.position,
        };
        r.frames.set_mirror(defaults.bubble.mirror);
        r.setup_preview = true;
        r.camera_device = device.clone();
        cx.notify();
    });
    open_bubble(app, cx);
    start_preview(app, device, cx);
}

/// Where the Settings pages and details read their pieces from.
pub fn area_kind(rec: &RecState) -> AreaKind {
    rec.hud
        .indicator
        .as_ref()
        .map_or(AreaKind::Display, |i| i.area.kind)
}

/// `--demo-*`: starts the plan on the fake engine (see `recording::demo`).
pub fn run_demo(app: &Entity<AppModel>, plan: crate::recording::demo::DemoPlan, cx: &mut App) {
    let backend = app.read(cx).backend.clone();
    let app2 = app.clone();
    cx.spawn(async move |cx| {
        cx.background_executor()
            .timer(Duration::from_millis(900))
            .await;
        if plan.sheet {
            cx.update(|cx| open_sheet(&app2, None, Some(plan.camera), cx));
            return;
        }
        let Some(daemon) = backend.daemon.clone() else {
            return;
        };
        let d2 = daemon.clone();
        let listed =
            backend
                .rt
                .spawn(async move {
                    Backend::call(&d2, "list_targets", json!({ "windows": false })).await
                })
                .await;
        let displays = listed
            .ok()
            .and_then(Result::ok)
            .map(|v| crate::recording::sheet::parse_displays(&v))
            .unwrap_or_default();
        let Some(d) = displays.iter().find(|d| d.primary).or(displays.first()) else {
            return;
        };
        let params = crate::recording::demo::params(&plan, d.id, d.size);
        if plan.agent {
            // An agent over the socket would be asked first; the demo approves it so the agent
            // toast and chip can be seen.
            let actor = cm_core::Actor::Agent("claude-desktop".to_owned());
            let _ = backend
                .rt
                .spawn(async move {
                    daemon
                        .execute_method(actor, 1, "start_recording", &params)
                        .await
                })
                .await;
        } else {
            let d3 = daemon.clone();
            let n = plan.countdown_s;
            let _ = backend
                .rt
                .spawn(async move {
                    Backend::call(
                        &d3,
                        "set_settings",
                        json!({ "values": { "recording.countdown_s": n } }),
                    )
                    .await
                })
                .await;
            let opts = demo_options(&plan, d.id, d.size);
            cx.update(|cx| start(&app2, opts, cx));
        }
        if let Some(after) = plan.pause_after {
            cx.background_executor()
                .timer(
                    Duration::from_secs(u64::from(plan.countdown_s))
                        + after
                        + Duration::from_secs(1),
                )
                .await;
            cx.update(|cx| toggle_pause(&app2, cx));
        }
    })
    .detach();
}

fn demo_options(
    plan: &crate::recording::demo::DemoPlan,
    display: u32,
    size: [u32; 2],
) -> StartOptions {
    let target = if plan.region {
        let (w, h) = (i64::from(size[0]) * 6 / 10, i64::from(size[1]) * 6 / 10);
        Target::Region {
            display_id: display,
            rect: [
                (i64::from(size[0]) - w) / 2,
                ((i64::from(size[1]) - h) / 2 - 40).max(0),
                w,
                h,
            ],
        }
    } else {
        Target::Display {
            display_id: display,
        }
    };
    StartOptions {
        target,
        camera: plan.camera.then(start::CameraChoice::default),
        mic: plan.mic,
        mic_device: None,
        system_audio: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::Duration;

    fn counting_down(r: &mut RecState) {
        r.hud.begin_start();
        r.hud.apply_event(
            "recording.countdown",
            &json!({"recording_id": "rec_1", "remaining": 3, "total": 3, "context": "start"}),
            Instant::now(),
        );
    }

    fn with_preview(r: &mut RecState) {
        r.preview = Some(
            crate::recording::preview::start(PreviewSource::Fake, None, r.frames.clone())
                .expect("fake preview"),
        );
        r.setup_preview = true;
        for _ in 0..100 {
            if r.frames.has_frame() {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("the fake camera delivered no frame");
    }

    /// R4-4 (privacy): a start that fails after the sheet showed the camera must not leave the
    /// preview running (the camera light) with no bubble to show for it.
    #[test]
    fn r4_4_a_failed_start_releases_the_camera_preview() {
        let mut r = RecState::new(PreviewSource::Fake);
        with_preview(&mut r);
        counting_down(&mut r);
        r.dialog_open = true;
        assert!(r.fail_start(false));
        assert!(r.preview.is_none(), "the preview runner is dropped");
        assert!(!r.frames.has_frame() && !r.setup_preview);
        assert_eq!(r.hud.phase, Phase::Idle);
        assert!(!r.dialog_open);
    }

    /// R4-4: a refused start (no event at all) releases it too; a busy engine only forgets the
    /// pending start of this app.
    #[test]
    fn r4_4_refused_and_busy_starts() {
        let mut r = RecState::new(PreviewSource::Fake);
        with_preview(&mut r);
        r.hud.begin_start();
        assert!(r.fail_start(false));
        assert!(r.preview.is_none());

        // Busy while the engine counts down for someone else: the countdown stays.
        let mut r = RecState::new(PreviewSource::Fake);
        counting_down(&mut r);
        assert!(!r.fail_start(true));
        assert_eq!(r.hud.phase, Phase::Countdown);
        // Busy before any event: our pending start goes away.
        let mut r = RecState::new(PreviewSource::Fake);
        r.hud.begin_start();
        assert!(r.fail_start(true));
        assert_eq!(r.hud.phase, Phase::Idle);
    }

    /// R4-4: a live take keeps its preview when an unrelated error comes in.
    #[test]
    fn r4_4_a_live_take_keeps_the_preview() {
        let mut r = RecState::new(PreviewSource::Fake);
        with_preview(&mut r);
        r.hud.apply_event(
            "recording.started",
            &json!({"recording_id": "rec_1", "capture_id": "c", "indicator": {"area": {"kind": "display"}}}),
            Instant::now(),
        );
        assert!(!r.fail_start(false));
        assert!(r.preview.is_some());
    }

    /// R4-1: after a failed start the Record button is enabled again and the tray is idle.
    #[test]
    fn r4_1_tray_recovers_after_a_failed_start() {
        let mut r = RecState::new(PreviewSource::Fake);
        counting_down(&mut r);
        let during = tray::plan(&r.hud, Instant::now());
        assert!(!during.record_enabled && during.stop_enabled);
        assert!(r.fail_start(false));
        let after = tray::plan(&r.hud, Instant::now());
        assert!(after.record_enabled && !after.stop_enabled);
        assert_eq!(after.state, crate::platform::TrayState::Idle);
    }

    /// R4-7: the answer to a Discard dialog applies only to the take it was asked about.
    #[test]
    fn r4_7_a_stale_dialog_cannot_hit_the_next_take() {
        let mut r = RecState::new(PreviewSource::Fake);
        let now = Instant::now();
        let started = |id: &str| json!({"recording_id": id, "capture_id": "c", "indicator": {"area": {"kind": "display"}}});
        r.hud
            .apply_event("recording.started", &started("rec_1"), now);
        assert!(r.hud.dialog_still_applies(Some("rec_1")));
        r.hud.apply_event(
            "recording.stopped",
            &json!({"recording_id": "rec_1", "capture_id": "c"}),
            now,
        );
        assert!(!r.hud.dialog_still_applies(Some("rec_1")), "ended");
        r.hud
            .apply_event("recording.started", &started("rec_2"), now);
        assert!(!r.hud.dialog_still_applies(Some("rec_1")), "another take");
        assert!(r.hud.dialog_still_applies(Some("rec_2")));
        // Counting down is not a take the dialog can discard.
        let mut c = RecState::new(PreviewSource::Fake);
        counting_down(&mut c);
        assert!(!c.hud.dialog_still_applies(Some("rec_1")));
    }
}
