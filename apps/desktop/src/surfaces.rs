//! The floating surfaces of the app (record sheet, countdown, HUD, indicator, webcam bubble (the setup bubble is the same window), toast
//! stack, launcher, pickers) and their lifetime (release 0.2.7, M1; issue 21).
//!
//! Every surface has one owner (the field that holds its handle) and one close path: [`close`] from
//! outside the window, [`close_self`] from inside it. Both write a log line with the module name
//! `cm_desktop::surfaces`, forget the surface, and when closing from outside retry a close that
//! failed and report one that never succeeds ([`CloseStep::GaveUp`]); a surface that could not be
//! closed stays in the "live app windows" list that Copy diagnostics prints, marked `STUCK`.
//!
//! The registry and the retry rule know nothing about GPUI: they work on window ids and a
//! [`SurfaceHost`], so a scripted host can fail a close in tests.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use gpui::{AnyWindowHandle, App, Window};

/// What a window is for (the label in the log and in the diagnostics).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    Main,
    Studio,
    RecordSheet,
    Countdown,
    Hud,
    Indicator,
    WebcamBubble,
    Toast,
    Launcher,
    Picker,
    DelayPill,
    Confirm,
    Dialog,
}

impl Surface {
    pub fn label(self) -> &'static str {
        match self {
            Surface::Main => "main",
            Surface::Studio => "studio",
            Surface::RecordSheet => "record_sheet",
            Surface::Countdown => "countdown",
            Surface::Hud => "hud",
            Surface::Indicator => "indicator",
            Surface::WebcamBubble => "webcam_bubble",
            Surface::Toast => "toast",
            Surface::Launcher => "launcher",
            Surface::Picker => "picker",
            Surface::DelayPill => "delay_pill",
            Surface::Confirm => "confirm",
            Surface::Dialog => "dialog",
        }
    }
}

/// How many times a close is tried before it is reported as stuck.
pub const MAX_CLOSE_ATTEMPTS: u32 = 5;
/// The pause before the next try (grows with the attempt).
pub const RETRY_STEP: Duration = Duration::from_millis(150);

#[derive(Clone, Debug, PartialEq, Eq)]
enum State {
    Open,
    /// `attempts` closes failed so far.
    Closing {
        attempts: u32,
    },
    Stuck {
        attempts: u32,
        error: String,
    },
}

#[derive(Clone, Debug)]
struct Entry {
    id: u64,
    kind: Surface,
    opened: Instant,
    state: State,
}

/// What a host did with a close request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CloseResult {
    Closed,
    /// The window was already gone: nothing left to close.
    Gone,
    Failed(String),
}

/// The thing that can close a window (GPUI in the app, a script in tests).
pub trait SurfaceHost {
    fn close(&mut self, id: u64) -> CloseResult;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CloseStep {
    Done,
    RetryAfter(Duration),
    GaveUp,
}

/// The registry of surfaces that are open (or failed to close).
#[derive(Default)]
pub struct Surfaces {
    entries: Vec<Entry>,
}

impl Surfaces {
    pub const fn new() -> Self {
        Surfaces {
            entries: Vec::new(),
        }
    }

    pub fn opened(&mut self, kind: Surface, id: u64) {
        self.entries.retain(|e| e.id != id);
        self.entries.push(Entry {
            id,
            kind,
            opened: Instant::now(),
            state: State::Open,
        });
        log::info!(
            "{} window {id} opened ({} live)",
            kind.label(),
            self.entries.len()
        );
    }

    /// The window is gone (closed from inside, or found missing).
    pub fn forget(&mut self, id: u64, how: &str) {
        if let Some(i) = self.entries.iter().position(|e| e.id == id) {
            let e = self.entries.remove(i);
            log::info!(
                "{} window {id} {how} after {:.1}s ({} live)",
                e.kind.label(),
                e.opened.elapsed().as_secs_f32(),
                self.entries.len()
            );
        } else {
            log::info!("window {id} {how} (it was not a tracked surface)");
        }
    }

    /// One close attempt. A failure schedules another try; the last failure is reported.
    pub fn close_step(&mut self, host: &mut dyn SurfaceHost, id: u64) -> CloseStep {
        let prior = match self.entries.iter().find(|e| e.id == id) {
            Some(Entry {
                state: State::Closing { attempts },
                ..
            }) => *attempts,
            Some(Entry {
                state: State::Stuck { attempts, .. },
                ..
            }) => *attempts,
            _ => 0,
        };
        match host.close(id) {
            CloseResult::Closed => {
                self.forget(id, "closed");
                CloseStep::Done
            }
            CloseResult::Gone => {
                self.forget(id, "was already gone when it was closed");
                CloseStep::Done
            }
            CloseResult::Failed(error) => {
                let attempts = prior + 1;
                let label = self
                    .entries
                    .iter()
                    .find(|e| e.id == id)
                    .map_or("untracked", |e| e.kind.label());
                if attempts >= MAX_CLOSE_ATTEMPTS {
                    log::error!(
                        "{label} window {id} could not be closed after {attempts} tries: {error}"
                    );
                    if let Some(e) = self.entries.iter_mut().find(|e| e.id == id) {
                        e.state = State::Stuck { attempts, error };
                    }
                    CloseStep::GaveUp
                } else {
                    log::warn!(
                        "{label} window {id} close failed (try {attempts} of {MAX_CLOSE_ATTEMPTS}): {error}"
                    );
                    if let Some(e) = self.entries.iter_mut().find(|e| e.id == id) {
                        e.state = State::Closing { attempts };
                    }
                    CloseStep::RetryAfter(RETRY_STEP * attempts)
                }
            }
        }
    }

    /// The "live app windows" text for Copy diagnostics. `live` are the ids the toolkit still has;
    /// a tracked surface that is not among them was closed some other way and is dropped.
    pub fn report(&mut self, live: &[u64]) -> String {
        let gone: Vec<u64> = self
            .entries
            .iter()
            .filter(|e| !live.contains(&e.id))
            .map(|e| e.id)
            .collect();
        for id in gone {
            self.forget(id, "no longer exists");
        }
        let mut out = format!("Live app windows: {}\n", live.len()); // i18n-ignore: diagnostics text
        for id in live {
            match self.entries.iter().find(|e| e.id == *id) {
                Some(e) => {
                    out.push_str(&format!(
                        "  {} id={} open {:.0}s{}\n",
                        e.kind.label(),
                        e.id,
                        e.opened.elapsed().as_secs_f32(),
                        match &e.state {
                            State::Open => String::new(),
                            State::Closing { attempts } => format!(" CLOSING (failed {attempts}x)"),
                            State::Stuck { attempts, error } =>
                                format!(" STUCK (close failed {attempts}x: {error})"),
                        }
                    ));
                }
                None => out.push_str(&format!("  untracked id={id}\n")),
            }
        }
        out
    }
}

static REGISTRY: Mutex<Surfaces> = Mutex::new(Surfaces::new());

fn registry() -> std::sync::MutexGuard<'static, Surfaces> {
    REGISTRY.lock().unwrap_or_else(|p| p.into_inner())
}

/// Records a window that was just opened.
pub fn opened(kind: Surface, handle: impl Into<AnyWindowHandle>) {
    registry().opened(kind, handle.into().window_id().as_u64());
}

/// The text Copy diagnostics adds: every window the toolkit has, with what it is for.
pub fn live_report(cx: &App) -> String {
    let live: Vec<u64> = cx
        .windows()
        .iter()
        .map(|h| h.window_id().as_u64())
        .collect();
    registry().report(&live)
}

/// Closes a window from inside it (its own close button, Esc). The one close path for that case.
pub fn close_self(window: &mut Window) {
    let id = window.window_handle().window_id().as_u64();
    registry().forget(id, "closed itself");
    window.remove_window();
}

// ---- activation ------------------------------------------------------------------------------
//
// AppKit makes a window key before `activate` returns and then calls GPUI's window callbacks. When
// the caller is inside an `App` or window update, those callbacks find the app borrowed and fail
// with `RefCell already borrowed` (then `window not found` once the window is gone). Every
// activation therefore goes through [`activate_app`] / [`activate_window`] / [`bring_to_front`],
// which only queue the request; one spawned task performs the queue when no update is running.

/// A queued request.
#[derive(Clone, Debug, PartialEq)]
pub enum Activation {
    App,
    /// Make the window key and frontmost; with `visible`, center it there first.
    Window {
        id: u64,
        visible: Option<(f64, f64, f64, f64)>,
    },
}

/// The queue. It never calls a host itself: [`PendingActivations::request`] records,
/// [`flush`] performs later.
#[derive(Default)]
pub struct PendingActivations {
    queue: Vec<(Activation, String)>,
    scheduled: bool,
}

impl PendingActivations {
    pub const fn new() -> Self {
        PendingActivations {
            queue: Vec::new(),
            scheduled: false,
        }
    }

    /// Queues a request; the same one asked twice before the flush runs once (the later `visible`
    /// wins). Returns `true` when the caller must schedule the flush.
    pub fn request(&mut self, a: Activation, why: &str) -> bool {
        log::info!("activation requested ({why}): {a:?}");
        match self.queue.iter_mut().find(|(q, _)| match (q, &a) {
            (Activation::App, Activation::App) => true,
            (Activation::Window { id: x, .. }, Activation::Window { id: y, .. }) => x == y,
            _ => false,
        }) {
            Some(slot) => *slot = (a, why.to_owned()),
            None => self.queue.push((a, why.to_owned())),
        }
        !std::mem::replace(&mut self.scheduled, true)
    }

    fn take(&mut self) -> Vec<(Activation, String)> {
        self.scheduled = false;
        std::mem::take(&mut self.queue)
    }
}

/// What performs an activation (GPUI/AppKit in the app, a script in tests).
pub trait ActivationHost {
    fn activate_app(&mut self);
    fn bring_to_front(&mut self, id: u64, visible: Option<(f64, f64, f64, f64)>);
}

/// Performs the queued requests in order. Called only when no update is running.
pub fn flush(pending: &mut PendingActivations, host: &mut dyn ActivationHost) {
    for (a, why) in pending.take() {
        log::info!("activating ({why}): {a:?}");
        match a {
            Activation::App => host.activate_app(),
            Activation::Window { id, visible } => host.bring_to_front(id, visible),
        }
    }
}

thread_local! {
    // The UI thread only (windows are not `Send`).
    static PENDING: std::cell::RefCell<PendingActivations> =
        const { std::cell::RefCell::new(PendingActivations::new()) };
    static NATIVES: std::cell::RefCell<std::collections::HashMap<u64, crate::platform_window::NativeWindow>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

struct RealActivation<'a> {
    cx: &'a mut gpui::AsyncApp,
}

impl ActivationHost for RealActivation<'_> {
    fn activate_app(&mut self) {
        // Natively, with no `App` borrow held; elsewhere through GPUI.
        if !crate::platform_window::activate_app() {
            self.cx.update(|cx| cx.activate(true));
        }
    }

    fn bring_to_front(&mut self, id: u64, visible: Option<(f64, f64, f64, f64)>) {
        let native = NATIVES.with(|n| n.borrow_mut().remove(&id));
        if let Some(native) = native {
            native.bring_to_front(visible);
            return;
        }
        self.cx.update(|cx| {
            if let Some(h) = cx
                .windows()
                .into_iter()
                .find(|h| h.window_id().as_u64() == id)
            {
                let _ = h.update(cx, |_, window, _| window.activate_window());
            }
        });
    }
}

fn request(cx: &mut App, a: Activation, why: &str) {
    if PENDING.with(|p| p.borrow_mut().request(a, why)) {
        cx.spawn(async move |cx| {
            let mut jobs = PENDING.with(|p| std::mem::take(&mut *p.borrow_mut()));
            flush(&mut jobs, &mut RealActivation { cx });
        })
        .detach();
    }
}

/// Activates the app once the current update is over (see above).
pub fn activate_app(cx: &mut App, why: &str) {
    request(cx, Activation::App, why);
}

/// Makes `window` key and frontmost once the current update is over, centered on `visible` when
/// given. `window` is the one being updated; `cx` is any `App` handle (`&mut **cx` of a context).
pub fn bring_to_front(
    cx: &mut App,
    window: &Window,
    visible: Option<(f64, f64, f64, f64)>,
    why: &str,
) {
    let id = window.window_handle().window_id().as_u64();
    if let Some(n) = crate::platform_window::NativeWindow::of(window) {
        NATIVES.with(|m| m.borrow_mut().insert(id, n));
    }
    request(cx, Activation::Window { id, visible }, why);
}

struct GpuiHost<'a> {
    cx: &'a mut App,
    handle: AnyWindowHandle,
}

impl SurfaceHost for GpuiHost<'_> {
    fn close(&mut self, id: u64) -> CloseResult {
        let handle = self.handle;
        if !self
            .cx
            .windows()
            .iter()
            .any(|h| h.window_id().as_u64() == id)
        {
            return CloseResult::Gone;
        }
        match handle.update(self.cx, |_, window, _| window.remove_window()) {
            Ok(()) => CloseResult::Closed,
            Err(e) => CloseResult::Failed(format!("{e:#}")),
        }
    }
}

/// Closes a window from outside it. The one close path for that case: a failed close is tried again
/// a few times (the window may be in the middle of an update) and then reported.
pub fn close(cx: &mut App, handle: impl Into<AnyWindowHandle>) {
    let handle: AnyWindowHandle = handle.into();
    let id = handle.window_id().as_u64();
    let step = registry().close_step(&mut GpuiHost { cx, handle }, id);
    if let CloseStep::RetryAfter(wait) = step {
        schedule_retry(cx, handle, wait);
    }
}

fn schedule_retry(cx: &mut App, handle: AnyWindowHandle, wait: Duration) {
    cx.spawn(async move |cx| {
        let mut wait = wait;
        loop {
            cx.background_executor().timer(wait).await;
            let id = handle.window_id().as_u64();
            let step = cx.update(|cx| registry().close_step(&mut GpuiHost { cx, handle }, id));
            match step {
                CloseStep::RetryAfter(next) => wait = next,
                CloseStep::Done | CloseStep::GaveUp => break,
            }
        }
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A host whose close fails a set number of times.
    struct Script {
        results: Vec<CloseResult>,
        calls: u32,
    }

    impl SurfaceHost for Script {
        fn close(&mut self, _id: u64) -> CloseResult {
            self.calls += 1;
            if self.results.is_empty() {
                CloseResult::Closed
            } else {
                self.results.remove(0)
            }
        }
    }

    fn failing(n: usize) -> Script {
        Script {
            results: vec![CloseResult::Failed("RefCell already borrowed".into()); n],
            calls: 0,
        }
    }

    /// Runs the retry loop the way the app does, without waiting.
    fn run(reg: &mut Surfaces, host: &mut Script, id: u64) -> (CloseStep, u32) {
        let mut tries = 0;
        loop {
            tries += 1;
            match reg.close_step(host, id) {
                CloseStep::RetryAfter(_) => {}
                other => return (other, tries),
            }
        }
    }

    #[test]
    fn a_close_that_fails_twice_is_retried_and_succeeds() {
        let mut reg = Surfaces::new();
        reg.opened(Surface::RecordSheet, 7);
        let mut host = failing(2);
        let (step, tries) = run(&mut reg, &mut host, 7);
        assert_eq!((step, tries), (CloseStep::Done, 3));
        assert!(reg.report(&[]).contains("Live app windows: 0"));
    }

    #[test]
    fn a_close_that_never_works_is_reported_and_stays_in_the_diagnostics() {
        let mut reg = Surfaces::new();
        reg.opened(Surface::WebcamBubble, 9);
        let mut host = failing(100);
        let (step, tries) = run(&mut reg, &mut host, 9);
        assert_eq!(step, CloseStep::GaveUp);
        assert_eq!(tries, MAX_CLOSE_ATTEMPTS);
        let text = reg.report(&[9]);
        assert!(text.contains("webcam_bubble id=9"), "{text}");
        assert!(text.contains("STUCK"), "{text}");
        assert!(text.contains("RefCell already borrowed"), "{text}");
    }

    #[test]
    fn the_pause_between_tries_grows() {
        let mut reg = Surfaces::new();
        reg.opened(Surface::Toast, 1);
        let mut host = failing(3);
        assert_eq!(
            reg.close_step(&mut host, 1),
            CloseStep::RetryAfter(RETRY_STEP)
        );
        assert_eq!(
            reg.close_step(&mut host, 1),
            CloseStep::RetryAfter(RETRY_STEP * 2)
        );
    }

    #[test]
    fn a_window_that_is_already_gone_counts_as_closed() {
        let mut reg = Surfaces::new();
        reg.opened(Surface::Hud, 3);
        let mut host = Script {
            results: vec![CloseResult::Gone],
            calls: 0,
        };
        assert_eq!(reg.close_step(&mut host, 3), CloseStep::Done);
        assert_eq!(host.calls, 1);
    }

    #[test]
    fn the_report_lists_live_windows_and_drops_closed_ones() {
        let mut reg = Surfaces::new();
        reg.opened(Surface::Main, 1);
        reg.opened(Surface::Toast, 2);
        let text = reg.report(&[1, 5]);
        assert!(text.starts_with("Live app windows: 2"), "{text}");
        assert!(text.contains("main id=1"), "{text}");
        assert!(text.contains("untracked id=5"), "{text}");
        assert!(!text.contains("toast"), "{text}");
    }

    /// Records what a host was asked to do.
    #[derive(Default)]
    struct Calls(Vec<String>);

    impl ActivationHost for Calls {
        fn activate_app(&mut self) {
            self.0.push("app".into());
        }
        fn bring_to_front(&mut self, id: u64, visible: Option<(f64, f64, f64, f64)>) {
            self.0.push(format!("window {id} {}", visible.is_some()));
        }
    }

    #[test]
    fn an_activation_asked_inside_an_update_runs_only_after_it() {
        let mut pending = PendingActivations::new();
        let mut host = Calls::default();
        // The "update": it asks for activations and must see nothing happen.
        let must_schedule = pending.request(Activation::App, "sheet opened");
        pending.request(
            Activation::Window {
                id: 4,
                visible: None,
            },
            "dialog opened",
        );
        assert!(must_schedule, "the first request asks for a flush");
        assert!(host.0.is_empty(), "nothing runs inside the update");
        // The update is over: the flush performs them, in order.
        flush(&mut pending, &mut host);
        assert_eq!(host.0, ["app", "window 4 false"]);
    }

    #[test]
    fn the_same_activation_asked_twice_runs_once_and_the_flush_is_scheduled_once() {
        let mut pending = PendingActivations::new();
        let mut host = Calls::default();
        assert!(pending.request(Activation::App, "a"));
        assert!(!pending.request(Activation::App, "b"));
        let w = |visible| Activation::Window { id: 2, visible };
        pending.request(w(None), "c");
        pending.request(w(Some((0., 0., 10., 10.))), "d");
        flush(&mut pending, &mut host);
        assert_eq!(host.0, ["app", "window 2 true"]);
        // After a flush the next request schedules again.
        assert!(pending.request(Activation::App, "e"));
    }
}
