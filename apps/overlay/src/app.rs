//! The windowing layer: one borderless always-on-top window per display (winit + softbuffer).
//!
//! All decisions live in [`Picker`]; this module creates the windows, turns OS events into
//! picker calls, paints with [`crate::paint`] and acts on the returned [`Effect`]s. It does
//! not talk to the daemon: the session task (see [`crate::session::stream`]) pushes
//! [`SessionEvent`]s into an [`Inbox`] and [`run`] returns the user's choice.
//!
//! Start-up is event driven so the slow parts overlap (NFR-1, 150 ms to the first frame):
//!
//! 1. the windows are created hidden as soon as the display list is known, while the frames
//!    are still being frozen and decoded;
//! 2. each window is painted and only then shown when *its* frame arrives, so no window ever
//!    shows un-dimmed or blank content, and nothing is shown at all if the freeze fails;
//! 3. the window list (hover highlight, click to pick a window) comes last; until it does,
//!    pointer input is held by a [`Gate`] and replayed in order (CAP-1.AC5).

use std::collections::VecDeque;
use std::num::NonZeroU32;
use std::rc::Rc;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use cm_core::PhysicalSize;
use softbuffer::{Context, Surface};
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize as WinSize};
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{Key as WinKey, ModifiersState, NamedKey};
use winit::monitor::MonitorHandle;
use winit::window::{CursorIcon, Window, WindowId, WindowLevel};

use crate::canvas::Canvas;
use crate::error::OverlayError;
use crate::gate::{Gate, Held, Pointer};
use crate::paint::{paint, tokens, Scene};
use crate::picker::{CursorHint, Effect, Key, Mode, Picker};
use crate::platform::prepare_overlay_window;
use crate::selection::Handle;
use crate::session::{Choice, Frozen, SessionEvent};
use crate::targets::{
    match_monitors, pickable_windows, DisplayInfo, MonitorGeometry, Targets, WindowInfo,
};
use crate::text::TextPainter;
use crate::trace::Tracer;

/// Benchmark only: how long to wait for the window server to report the windows visible.
const VISIBLE_WAIT: Duration = Duration::from_secs(1);

/// How often to check that every display still exists.
const MONITOR_POLL: Duration = Duration::from_millis(500);

/// Options for [`run`].
#[derive(Debug, Clone, Copy)]
pub struct Options {
    pub mode: Mode,
    pub magnifier: bool,
    pub hint: bool,
    pub confirm_on_release: bool,
    /// Window mode: Tab ends the overlay with [`Finish::SwitchToList`] (R19 "Pick on screen").
    pub tab_to_list: bool,
    /// Benchmark: finish as soon as every display has painted its first frame.
    pub exit_after_first_frame: bool,
}

/// How the overlay ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Finish {
    Chosen(Choice),
    /// Esc, or every display went away (CAP-1.AC6): nothing is saved.
    Cancelled,
    /// Tab in window mode with `tab_to_list`: nothing is captured, the desktop app shows its
    /// window list instead.
    SwitchToList,
    /// `exit_after_first_frame`: every display showed its first frame.
    FirstFrame,
}

/// What [`run`] hands back.
pub struct Finished {
    pub finish: Finish,
    /// Every display whose frame had arrived by the time the overlay ended.
    pub frozen: Vec<Frozen>,
}

/// Mailbox between the session task (any thread) and the event loop (main thread).
pub struct Inbox {
    queue: Mutex<VecDeque<SessionEvent>>,
    proxy: OnceLock<EventLoopProxy<()>>,
}

impl Inbox {
    pub fn new() -> Arc<Inbox> {
        Arc::new(Inbox {
            queue: Mutex::new(VecDeque::new()),
            proxy: OnceLock::new(),
        })
    }

    /// Queues `ev` and wakes the event loop once it exists (events pushed earlier are picked
    /// up by [`Inbox::attach`]).
    pub fn push(&self, ev: SessionEvent) {
        self.queue
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push_back(ev);
        if let Some(p) = self.proxy.get() {
            let _ = p.send_event(());
        }
    }

    fn attach(&self, proxy: EventLoopProxy<()>) {
        let _ = self.proxy.set(proxy);
        if let Some(p) = self.proxy.get() {
            let _ = p.send_event(());
        }
    }

    fn take(&self) -> Vec<SessionEvent> {
        self.queue
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .drain(..)
            .collect()
    }
}

struct View {
    display: DisplayInfo,
    scale: f64,
    picker: Picker,
    window: Option<Rc<Window>>,
    surface: Option<Surface<Rc<Window>, Rc<Window>>>,
    monitor: MonitorGeometry,
    /// Set once the frame has arrived; the window is hidden until it is painted.
    frozen: Option<Frozen>,
    shown: bool,
    /// The window server reported the window on screen (`Occluded(false)`).
    visible: bool,
    /// Size of the buffer last painted, in px.
    painted: Option<(u32, u32)>,
}

struct App {
    inbox: Arc<Inbox>,
    opts: Options,
    tracer: Tracer,
    text: TextPainter,
    monitors: Option<Vec<MonitorHandle>>,
    displays: Option<Vec<DisplayInfo>>,
    /// Frames that arrived (kept for the final capture even if no window shows them).
    frozen: Vec<Frozen>,
    windows: Option<Vec<WindowInfo>>,
    gate: Gate,
    views: Vec<View>,
    views_created: bool,
    /// When every view had shown its first frame (see `check_first_frames`).
    first_frames_at: Option<Instant>,
    all_visible_marked: bool,
    /// The view that last saw the pointer: keys go there, whichever window the OS focused.
    active: usize,
    shift: bool,
    finish: Option<Finish>,
    error: Option<OverlayError>,
    clipboard: Option<arboard::Clipboard>,
    next_monitor_check: Instant,
}

/// Shows the overlay on every display as the session's events arrive and blocks until the
/// user decides.
pub fn run(inbox: &Arc<Inbox>, opts: Options, tracer: &Tracer) -> Result<Finished, OverlayError> {
    let text = TextPainter::new()
        .ok_or_else(|| OverlayError::internal("the bundled font could not be loaded"))?;
    tracer.mark("font_ready", serde_json::Value::Null);
    let event_loop = make_event_loop()?;
    event_loop.set_control_flow(ControlFlow::Wait);
    inbox.attach(event_loop.create_proxy());
    tracer.mark("event_loop_built", serde_json::Value::Null);
    let mut app = App {
        inbox: inbox.clone(),
        opts,
        tracer: tracer.clone(),
        text,
        monitors: None,
        displays: None,
        frozen: Vec::new(),
        windows: None,
        gate: Gate::new(opts.mode != Mode::Display, Instant::now()),
        views: Vec::new(),
        views_created: false,
        first_frames_at: None,
        all_visible_marked: false,
        active: 0,
        shift: false,
        finish: None,
        error: None,
        clipboard: None,
        next_monitor_check: Instant::now() + MONITOR_POLL,
    };
    event_loop
        .run_app(&mut app)
        .map_err(|e| OverlayError::internal(format!("event loop failed: {e}")))?;
    if let Some(e) = app.error {
        return Err(e);
    }
    // Frames that arrived while the event loop was shutting down still need releasing.
    for ev in app.inbox.take() {
        if let SessionEvent::Frozen(f) = ev {
            app.frozen.push(f);
        }
    }
    Ok(Finished {
        finish: app.finish.unwrap_or(Finish::Cancelled),
        frozen: app.frozen,
    })
}

#[cfg(target_os = "macos")]
fn make_event_loop() -> Result<EventLoop<()>, OverlayError> {
    use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
    // An accessory app: no Dock icon or menu bar of its own while the picker is up.
    // No default menu to build, and no activation at launch: the first window to be shown takes
    // focus itself (`Window::focus_window` activates the app), so the launch does not wait for
    // the window server to switch apps before the windows can be created.
    EventLoop::builder()
        .with_activation_policy(ActivationPolicy::Accessory)
        .with_default_menu(false)
        .with_activate_ignoring_other_apps(false)
        .build()
        .map_err(|e| OverlayError::internal(format!("cannot start the event loop: {e}")))
}

#[cfg(not(target_os = "macos"))]
fn make_event_loop() -> Result<EventLoop<()>, OverlayError> {
    EventLoop::new()
        .map_err(|e| OverlayError::internal(format!("cannot start the event loop: {e}")))
}

fn cursor_icon(h: CursorHint) -> CursorIcon {
    match h {
        CursorHint::Crosshair => CursorIcon::Crosshair,
        CursorHint::Pointer => CursorIcon::Pointer,
        CursorHint::Move => CursorIcon::Move,
        CursorHint::Resize(Handle::North | Handle::South) => CursorIcon::NsResize,
        CursorHint::Resize(Handle::East | Handle::West) => CursorIcon::EwResize,
        CursorHint::Resize(Handle::NorthEast | Handle::SouthWest) => CursorIcon::NeswResize,
        CursorHint::Resize(Handle::NorthWest | Handle::SouthEast) => CursorIcon::NwseResize,
    }
}

/// Tab, once (not a key repeat), when the desktop app asked for the hand-over.
fn is_tab_to_list(enabled: bool, event: &winit::event::KeyEvent) -> bool {
    enabled && !event.repeat && matches!(&event.logical_key, WinKey::Named(NamedKey::Tab))
}

fn map_key(event: &winit::event::KeyEvent, shift: bool) -> Option<Key> {
    let arrow = |dx, dy| Some(Key::Arrow { dx, dy, shift });
    match &event.logical_key {
        WinKey::Named(NamedKey::Escape) => Some(Key::Escape),
        WinKey::Named(NamedKey::Enter) => Some(Key::Enter),
        WinKey::Named(NamedKey::ArrowLeft) => arrow(-1, 0),
        WinKey::Named(NamedKey::ArrowRight) => arrow(1, 0),
        WinKey::Named(NamedKey::ArrowUp) => arrow(0, -1),
        WinKey::Named(NamedKey::ArrowDown) => arrow(0, 1),
        WinKey::Character(c) if c.eq_ignore_ascii_case("c") && !event.repeat => {
            Some(Key::CopyColor)
        }
        _ => None,
    }
}

fn geometry_of(monitors: &[MonitorHandle]) -> Vec<MonitorGeometry> {
    monitors
        .iter()
        .map(|m| MonitorGeometry {
            position: cm_core::PhysicalPoint {
                x: m.position().x,
                y: m.position().y,
            },
            size: PhysicalSize {
                width: m.size().width,
                height: m.size().height,
            },
        })
        .collect()
}

impl App {
    /// Takes what the session task has reported since the last call.
    fn drain_inbox(&mut self, el: &ActiveEventLoop) {
        for ev in self.inbox.take() {
            match ev {
                SessionEvent::Displays(d) => self.displays = Some(d),
                SessionEvent::Frozen(f) => {
                    self.frozen.push(f.clone());
                    if let Some(i) = self.views.iter().position(|v| v.display.id == f.display.id) {
                        self.views[i].frozen = Some(f);
                        self.redraw(i);
                    }
                }
                SessionEvent::Windows(w) => self.windows = Some(w),
                SessionEvent::Failed(e) => {
                    self.error = Some(e);
                    el.exit();
                    return;
                }
            }
        }
        self.create_views(el);
        self.sync_windows(el);
    }

    /// Creates one hidden window per display as soon as both the display list and the OS
    /// monitors are known. Frames that already arrived are painted right away.
    fn create_views(&mut self, el: &ActiveEventLoop) {
        if self.views_created || self.error.is_some() {
            return;
        }
        let (Some(monitors), Some(displays)) = (&self.monitors, &self.displays) else {
            return;
        };
        self.views_created = true;
        let geometry = geometry_of(monitors);
        let matched = match_monitors(displays, &geometry);
        let mut views = Vec::new();
        for (d, mi) in displays.iter().zip(matched) {
            let Some(mi) = mi else {
                self.tracer
                    .note(&serde_json::json!({"trace": "no_monitor", "display_id": d.id}));
                continue;
            };
            match self.create_view(el, d, &monitors[mi], geometry[mi]) {
                Ok(v) => views.push(v),
                Err(e) => {
                    self.error = Some(e);
                    el.exit();
                    return;
                }
            }
        }
        if views.is_empty() {
            self.error = Some(OverlayError::internal(
                "no display could be matched to an operating-system monitor",
            ));
            el.exit();
            return;
        }
        for v in &mut views {
            v.frozen = self
                .frozen
                .iter()
                .find(|f| f.display.id == v.display.id)
                .cloned();
        }
        self.views = views;
        self.tracer.mark("windows_created", serde_json::Value::Null);
        for i in 0..self.views.len() {
            self.redraw(i);
        }
    }

    fn create_view(
        &self,
        el: &ActiveEventLoop,
        display: &DisplayInfo,
        monitor: &MonitorHandle,
        geometry: MonitorGeometry,
    ) -> Result<View, OverlayError> {
        let attrs = Window::default_attributes()
            .with_title("CaptureMann overlay")
            .with_decorations(false)
            .with_resizable(false)
            // Shown by the first paint of the frozen frame, never blank or un-dimmed.
            .with_visible(false)
            .with_window_level(WindowLevel::AlwaysOnTop)
            .with_position(monitor.position())
            .with_inner_size(monitor.size());
        // Windows: borderless fullscreen covers the taskbar (macOS is covered by
        // `prepare_overlay_window`, not native fullscreen).
        #[cfg(not(target_os = "macos"))]
        let attrs = attrs.with_fullscreen(Some(winit::window::Fullscreen::Borderless(Some(
            monitor.clone(),
        ))));
        let window = Rc::new(el.create_window(attrs).map_err(|e| {
            OverlayError::internal(format!("cannot create the overlay window: {e}"))
        })?);
        self.tracer.mark(
            "window_created",
            serde_json::json!({"display_id": display.id}),
        );
        let prepared = prepare_overlay_window(&window);
        self.tracer.mark(
            "window_prepared",
            serde_json::json!({"display_id": display.id}),
        );
        window.set_cursor(CursorIcon::Crosshair);
        let surface = Context::new(window.clone())
            .and_then(|ctx| Surface::new(&ctx, window.clone()))
            .map_err(|e| OverlayError::internal(format!("cannot create the surface: {e}")))?;
        self.tracer.mark(
            "surface_ready",
            serde_json::json!({"display_id": display.id}),
        );
        let inner = window.inner_size();
        let pos = window.inner_position().ok();
        let offset = pos.map(|p| (p.x - monitor.position().x, p.y - monitor.position().y));
        let size = display.bounds.size;
        let covers =
            offset == Some((0, 0)) && (inner.width, inner.height) == (size.width, size.height);
        self.tracer.note(&serde_json::json!({
            "trace": "mapping",
            "display_id": display.id,
            "window_px": [inner.width, inner.height],
            "capture_px": [size.width, size.height],
            "offset_px": offset,
            "covers_display_1_to_1": covers,
            "prepared": prepared.as_ref().map_err(Clone::clone).map(|p| format!("{p:?}")),
        }));
        let scale = display.scale.get();
        // The window list comes later (`sync_windows`); the picker starts with none.
        let picker = Picker::new(
            self.opts.mode,
            size,
            scale,
            Vec::new(),
            self.opts.confirm_on_release,
        );
        Ok(View {
            display: display.clone(),
            scale,
            picker,
            window: Some(window),
            surface: Some(surface),
            monitor: geometry,
            frozen: None,
            shown: false,
            visible: false,
            painted: None,
        })
    }

    /// Gives every picker its windows once the list and the views both exist, then lets the
    /// held pointer input through (CAP-1.AC5).
    fn sync_windows(&mut self, el: &ActiveEventLoop) {
        if self.views.is_empty() || self.gate.is_open() && self.windows.is_none() {
            return;
        }
        let expired = self.gate.expired(Instant::now());
        let Some(list) = self.windows.take() else {
            if expired {
                // The list is late or failed: stop holding the user's clicks.
                self.tracer
                    .mark("windows_wait_expired", serde_json::Value::Null);
                self.replay_held(el);
            }
            return;
        };
        let targets = Targets {
            displays: self.views.iter().map(|v| v.display.clone()).collect(),
            windows: list,
        };
        let pid = std::process::id();
        for v in &mut self.views {
            v.picker
                .set_windows(pickable_windows(&targets, &v.display, pid));
        }
        self.tracer.mark("windows_applied", serde_json::Value::Null);
        self.replay_held(el);
        // Only a pointer on a display can have a window under it to highlight; repainting the
        // others would just delay the first frame's commit by two 4K paints.
        for v in &self.views {
            if let (Some(w), Some(_)) = (&v.window, v.picker.pointer()) {
                w.request_redraw();
            }
        }
    }

    fn replay_held(&mut self, el: &ActiveEventLoop) {
        for held in self.gate.open() {
            self.apply_pointer(held, el);
        }
    }

    fn apply_pointer(&mut self, (i, ev): Held, el: &ActiveEventLoop) {
        let Some(view) = self.views.get_mut(i) else {
            return;
        };
        let e = match ev {
            Pointer::Moved { x, y } => view.picker.pointer_moved(x, y),
            Pointer::Pressed => view.picker.press(),
            Pointer::Released => view.picker.release(),
        };
        self.apply(i, e, el);
    }

    fn submit_pointer(&mut self, i: usize, ev: Pointer, el: &ActiveEventLoop) {
        if let Some(held) = self.gate.submit(i, ev) {
            self.apply_pointer(held, el);
        }
    }

    /// Benchmark bookkeeping: marks `all_first_frames` (every view painted and shown) and
    /// `all_visible` (the window server reported every window on screen, which is later: the
    /// frame is composited after the paint), and with `exit_after_first_frame` finishes on the
    /// latter, or [`VISIBLE_WAIT`] after the former if the window server never says.
    fn check_first_frames(&mut self, el: &ActiveEventLoop) {
        let live = || self.views.iter().filter(|v| v.window.is_some());
        if !self.views_created
            || self.views.is_empty()
            || !live().all(|v| v.shown)
            || self.finish.is_some()
        {
            return;
        }
        let first_frames_at = *self.first_frames_at.get_or_insert_with(|| {
            self.tracer
                .mark("all_first_frames", serde_json::Value::Null);
            Instant::now()
        });
        let all_visible = live().all(|v| v.visible);
        if all_visible && !self.all_visible_marked {
            self.all_visible_marked = true;
            self.tracer.mark("all_visible", serde_json::Value::Null);
        }
        if self.opts.exit_after_first_frame
            && (all_visible || first_frames_at.elapsed() >= VISIBLE_WAIT)
        {
            self.finish = Some(Finish::FirstFrame);
            el.exit();
        }
    }

    fn view_for(&self, id: WindowId) -> Option<usize> {
        self.views
            .iter()
            .position(|v| v.window.as_ref().is_some_and(|w| w.id() == id))
    }

    fn redraw(&mut self, i: usize) {
        let show_hint = self.opts.hint
            && (self.active == i && self.views[i].picker.pointer().is_some()
                || self.views.iter().all(|v| v.picker.pointer().is_none()) && i == 0);
        let Some(frozen) = self.views[i].frozen.clone() else {
            // No frame yet: the window stays hidden.
            return;
        };
        let frame = &*frozen.frame;
        let tracer = self.tracer.clone();
        let text = &self.text;
        let magnifier = self.opts.magnifier;
        let view = &mut self.views[i];
        let (Some(window), Some(surface)) = (view.window.as_ref(), view.surface.as_mut()) else {
            return;
        };
        let size = window.inner_size();
        let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) else {
            return;
        };
        let t0 = Instant::now();
        let mut newly_shown = false;
        if let Err(e) = surface.resize(w, h) {
            log::warn!(
                "surface resize to {}x{} failed: {e}",
                size.width,
                size.height
            );
            return;
        }
        let mut buffer = match surface.buffer_mut() {
            Ok(b) => b,
            Err(e) => {
                log::warn!("buffer_mut: {e}");
                return;
            }
        };
        let t_buffer = t0.elapsed();
        match Canvas::new(&mut buffer, size.width, size.height) {
            Some(mut canvas) => {
                let scene = Scene {
                    picker: &view.picker,
                    scale: view.scale,
                    text,
                    show_loupe: magnifier,
                    show_hint,
                    tab_to_list: self.opts.tab_to_list,
                };
                if !paint(&mut canvas, frame, &scene) {
                    // Window and frame disagree (see the `mapping` trace): show black, not junk.
                    canvas.buf.fill(tokens::INK);
                }
            }
            None => buffer.fill(tokens::INK),
        }
        view.painted = Some((size.width, size.height));
        let t_paint = t0.elapsed();
        if let Err(e) = buffer.present() {
            log::warn!("present: {e}");
        }
        if !view.shown {
            view.shown = true;
            // The first present is on screen's layer; now the window may appear.
            window.set_visible(true);
            tracer.mark(
                "first_frame",
                serde_json::json!({
                    "display_id": view.display.id,
                    "buffer_ms": t_buffer.as_secs_f64() * 1e3,
                    "paint_ms": (t_paint - t_buffer).as_secs_f64() * 1e3,
                    "present_ms": (t0.elapsed() - t_paint).as_secs_f64() * 1e3,
                }),
            );
            if i == 0 {
                window.focus_window();
            }
            newly_shown = true;
        }
        if newly_shown
            && self.views_created
            && self.views.iter().all(|v| v.shown || v.window.is_none())
        {
            self.tracer.mark("all_painted", serde_json::Value::Null);
        }
    }

    fn apply(&mut self, i: usize, effect: Effect, el: &ActiveEventLoop) {
        match effect {
            Effect::Nothing => {}
            Effect::Redraw => {
                if let Some(w) = &self.views[i].window {
                    w.set_cursor(cursor_icon(self.views[i].picker.cursor()));
                    w.request_redraw();
                }
            }
            Effect::Cancel => {
                self.finish = Some(Finish::Cancelled);
                el.exit();
            }
            Effect::Done(outcome) => {
                self.finish = Some(Finish::Chosen(Choice {
                    display_id: self.views[i].display.id,
                    outcome,
                }));
                el.exit();
            }
            Effect::WarpPointer(p) => {
                if let Some(w) = &self.views[i].window {
                    let _ = w.set_cursor_position(PhysicalPosition::new(
                        f64::from(p.x) + 0.5,
                        f64::from(p.y) + 0.5,
                    ));
                    w.request_redraw();
                }
            }
            Effect::CopyColor => self.copy_color(i),
        }
    }

    /// CAP-3.AC4: `C` copies `#RRGGBB` of the pixel under the pointer and stays in the picker.
    fn copy_color(&mut self, i: usize) {
        let Some(frozen) = &self.views[i].frozen else {
            return;
        };
        let Some(p) = self.views[i].picker.pixel() else {
            return;
        };
        let Some(rgb) = frozen.frame.pixel(p.x, p.y) else {
            return;
        };
        let hex = crate::magnifier::hex_color(rgb);
        if self.clipboard.is_none() {
            self.clipboard = arboard::Clipboard::new().ok();
        }
        if let Some(cb) = self.clipboard.as_mut() {
            if let Err(e) = cb.set_text(hex) {
                log::warn!("could not copy the color: {e}");
            }
        }
    }

    /// CAP-1: when a display disappears, close its overlay and keep the others.
    fn drop_lost_displays(&mut self, el: &ActiveEventLoop) {
        let monitors: Vec<MonitorHandle> = el.available_monitors().collect();
        let geometry = geometry_of(&monitors);
        for v in &mut self.views {
            if v.window.is_some() && !geometry.contains(&v.monitor) {
                v.window = None;
                v.surface = None;
            }
        }
        if !self.views.is_empty() && self.views.iter().all(|v| v.window.is_none()) {
            self.finish = Some(Finish::Cancelled);
            el.exit();
        }
    }
}

impl ApplicationHandler<()> for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.monitors.is_none() {
            self.monitors = Some(el.available_monitors().collect());
            self.tracer.mark("resumed", serde_json::Value::Null);
            self.drain_inbox(el);
        }
    }

    fn user_event(&mut self, el: &ActiveEventLoop, _: ()) {
        if self.monitors.is_some() {
            self.drain_inbox(el);
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(i) = self.view_for(id) else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => {
                self.finish = Some(Finish::Cancelled);
                el.exit();
            }
            WindowEvent::RedrawRequested => self.redraw(i),
            WindowEvent::Resized(WinSize { .. }) | WindowEvent::ScaleFactorChanged { .. } => {
                // Showing or activating a window reports its (unchanged) size: the buffer is
                // already right, so only a real size change repaints.
                if let Some(w) = &self.views[i].window {
                    if self.views[i].painted != Some((w.inner_size().width, w.inner_size().height))
                    {
                        w.request_redraw();
                    }
                }
            }
            WindowEvent::Occluded(false) => {
                self.tracer.mark(
                    "visible",
                    serde_json::json!({ "display_id": self.views[i].display.id }),
                );
                self.views[i].visible = true;
            }
            WindowEvent::ModifiersChanged(m) => {
                self.shift = m.state().contains(ModifiersState::SHIFT)
            }
            WindowEvent::CursorEntered { .. } => {
                if self.active != i {
                    let prev = self.active;
                    self.active = i;
                    if let Some(w) = &self.views[i].window {
                        w.focus_window();
                    }
                    // The hint follows the pointer between displays.
                    if prev < self.views.len() {
                        if let Some(w) = &self.views[prev].window {
                            w.request_redraw();
                        }
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.active = i;
                self.submit_pointer(
                    i,
                    Pointer::Moved {
                        x: position.x,
                        y: position.y,
                    },
                    el,
                );
            }
            WindowEvent::CursorLeft { .. } => {
                let e = self.views[i].picker.pointer_left();
                self.apply(i, e, el);
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                self.active = i;
                let ev = match state {
                    ElementState::Pressed => Pointer::Pressed,
                    ElementState::Released => Pointer::Released,
                };
                self.submit_pointer(i, ev, el);
            }
            WindowEvent::Focused(focused) => {
                // Wayland compositors may refuse keyboard focus to a window nobody activated;
                // the trace shows whether this one got it (the pointer works either way).
                self.tracer.note(
                    &serde_json::json!({"trace": "focus", "display_id": i, "focused": focused}),
                );
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                self.tracer.note(
                    &serde_json::json!({"trace": "key", "key": format!("{:?}", event.logical_key)}),
                );
                if is_tab_to_list(self.opts.tab_to_list, &event) {
                    self.finish = Some(Finish::SwitchToList);
                    el.exit();
                } else if let Some(key) = map_key(&event, self.shift) {
                    // Keys go to the display the pointer is on, not the window the OS made key.
                    let target = if self.active < self.views.len() {
                        self.active
                    } else {
                        i
                    };
                    let e = self.views[target].picker.key(key);
                    self.apply(target, e, el);
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        let now = Instant::now();
        if now >= self.next_monitor_check {
            self.next_monitor_check = now + MONITOR_POLL;
            self.drop_lost_displays(el);
        }
        if self.gate.expired(now) {
            self.sync_windows(el);
        }
        self.check_first_frames(el);
        let mut wake = self
            .gate
            .deadline()
            .map_or(self.next_monitor_check, |d| d.min(self.next_monitor_check));
        if let (true, Some(t)) = (self.opts.exit_after_first_frame, self.first_frames_at) {
            wake = wake.min(t + VISIBLE_WAIT);
        }
        el.set_control_flow(ControlFlow::WaitUntil(wake));
    }
}
