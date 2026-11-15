//! `capture_screenshot { mode: "long", ... }` (CAP-6 Long Shot): the action, its parameters and
//! the run on top of `cm_capture::long_shot`.
//!
//! Tiers (AC7): Auto scroll from an agent is Confirm (it moves the user's screen), shown with
//! the generic ask-first dialog; Hand scroll is refused for agents (`invalid_input`,
//! `data.reason: "needs_user"`). Stopping a running Long Shot (Esc, the HUD's Stop) is
//! [`crate::Daemon::cancel_delayed_capture`] with the call's `request_id`: what was captured is
//! kept. Progress goes on the bus as [`PROGRESS_EVENT`].

use std::sync::{Arc, Mutex, PoisonError};

use cm_capture::long_shot::{
    self, AutoScrollSupport, Direction, EndReason, FrameSource, Limits, LongShotConfig, Scroller,
    ThreadSleeper, MAX_LONG_SIDE,
};
use cm_capture::{Capture, CaptureKind, CaptureRequest, Capturer, Frame};
use cm_core::{Actor, CmError, PhysicalPoint, PhysicalRect, PhysicalSize};
use serde::Serialize;
use serde_json::{json, Value};

use crate::action::{Action, CaptureLongParams, ScrollMode};
use crate::daemon::Inner;
use crate::error::DaemonError;

/// Bus event with the Long Shot's progress (`frames`, `len_px`, `max_len`, `header_px`,
/// `footer_px`, `join_ok`, `request_id`): the HUD counter and the side preview follow it.
pub const PROGRESS_EVENT: &str = "capture.long_shot.progress";
/// Bus events around one Long Shot run (`request_id`, which may be null for a CLI or MCP call
/// without one): the app grabs the Esc key while one runs, whoever started it. The started event
/// also says who (`client`: the agent's name, null for a person or the CLI), how (`scroll`,
/// `direction`) and where (`display_id`, `area` `[x, y, w, h]` physical px in that display's own
/// frame and its `scale`, null when the target is not found), so the app can show a small
/// passive HUD for a run it did not start itself (design S18).
pub const STARTED_EVENT: &str = "capture.long_shot.started";
pub const ENDED_EVENT: &str = "capture.long_shot.ended";

/// What a running Long Shot shows the HUD and the side preview, keyed by the call's
/// `request_id` (`long_shot_state`). Counts only on the bus; the pixels of the preview leave the
/// daemon only through that pull and are never logged.
#[derive(Default)]
pub struct LongStates(Mutex<std::collections::HashMap<String, Arc<LongState>>>);

#[derive(Default)]
pub struct LongState {
    paused: std::sync::atomic::AtomicBool,
    inner: Mutex<LongSnapshot>,
}

#[derive(Default, Clone)]
struct LongSnapshot {
    seq: u64,
    frames: u32,
    len_px: u32,
    max_len: u32,
    header_px: u32,
    footer_px: u32,
    /// The newest settled frame did not line up with the last one taken and was left out (Hand
    /// scroll); cleared when a frame joins again.
    join_failed: bool,
    preview: Option<(u32, u32, Vec<u8>)>,
}

impl LongStates {
    fn start(&self, id: &str) -> Arc<LongState> {
        let st = Arc::new(LongState::default());
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(id.to_owned(), st.clone());
        st
    }

    fn get(&self, id: &str) -> Option<Arc<LongState>> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(id)
            .cloned()
    }

    fn finish(&self, id: &str) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(id);
    }
}

impl LongState {
    fn is_paused(&self) -> bool {
        self.paused.load(std::sync::atomic::Ordering::SeqCst)
    }
}

/// Largest preview the daemon renders; the HUD's panel scales it.
const PREVIEW_MAX_W: u32 = 360;
const PREVIEW_MAX_H: u32 = 1200;

/// What touches the screen: frames of the target and the scroller. The real one uses the
/// capturer and `cm_capture::long_shot::platform_scroller`; tests give scripted ones.
pub trait LongShotSystem: Send + Sync {
    fn frames(
        &self,
        capturer: Arc<Capturer>,
        target: cm_capture::CaptureTarget,
    ) -> Box<dyn FrameSource + Send>;
    /// `anchor`: a point of the area in global physical px, where the pointer is put before each
    /// scroll (a wheel event goes to the window under the pointer).
    fn scroller(&self, anchor: Option<(i32, i32)>) -> Result<Box<dyn Scroller + Send>, CmError>;
    /// [`LongShotSystem::scroller`] for a window target: a console window gets the scroller's
    /// per-target fallbacks (`window_id`); the default knows none.
    fn scroller_for(
        &self,
        anchor: Option<(i32, i32)>,
        _window_id: Option<u32>,
    ) -> Result<Box<dyn Scroller + Send>, CmError> {
        self.scroller(anchor)
    }
    fn auto_scroll(&self) -> AutoScrollSupport;
    /// Brings the window to the front before Auto scroll's first step, so that the wheel reaches
    /// it and not a window that covers it. `true` when it is in front afterwards; the default is
    /// "no helper on this system" (`false`).
    fn raise_window(&self, _window_id: u32) -> bool {
        false
    }
}

/// The real screen.
pub struct RealLongShot;

struct CapturerFrames {
    capturer: Arc<Capturer>,
    target: cm_capture::CaptureTarget,
    /// Where the window was at the first frame (a window target only).
    first_rect: Option<PhysicalRect>,
}

impl FrameSource for CapturerFrames {
    fn grab(&mut self) -> Result<Frame, CmError> {
        let frame = self
            .capturer
            .capture(CaptureRequest::new(self.target), None)?
            .map(|c| c.frame)
            .ok_or_else(|| CmError::invalid_input("capture was cancelled"))?;
        if self.first_rect.is_none()
            && matches!(self.target, cm_capture::CaptureTarget::Window { .. })
        {
            self.first_rect = area_rect(&self.capturer, self.target);
        }
        Ok(frame)
    }

    /// A window target whose rectangle is not the one it had at the first frame (moved or
    /// resized by the person). Display and region targets do not move.
    fn target_changed(&mut self) -> bool {
        let (Some(first), cm_capture::CaptureTarget::Window { .. }) =
            (self.first_rect, self.target)
        else {
            return false;
        };
        area_rect(&self.capturer, self.target).is_some_and(|now| now != first)
    }
}

impl LongShotSystem for RealLongShot {
    fn frames(
        &self,
        capturer: Arc<Capturer>,
        target: cm_capture::CaptureTarget,
    ) -> Box<dyn FrameSource + Send> {
        Box::new(CapturerFrames {
            capturer,
            target,
            first_rect: None,
        })
    }

    fn scroller(&self, anchor: Option<(i32, i32)>) -> Result<Box<dyn Scroller + Send>, CmError> {
        long_shot::platform_scroller_at(anchor)
    }

    fn scroller_for(
        &self,
        anchor: Option<(i32, i32)>,
        window_id: Option<u32>,
    ) -> Result<Box<dyn Scroller + Send>, CmError> {
        long_shot::platform_scroller_for(anchor, window_id)
    }

    fn auto_scroll(&self) -> AutoScrollSupport {
        long_shot::auto_scroll_support()
    }

    fn raise_window(&self, window_id: u32) -> bool {
        long_shot::raise_window(window_id)
    }
}

/// The title of the window target, when it is listed now.
fn window_title(capturer: &Capturer, target: cm_capture::CaptureTarget) -> Option<String> {
    let cm_capture::CaptureTarget::Window { window_id } = target else {
        return None;
    };
    let targets = capturer.list_targets().ok()?;
    targets
        .windows
        .iter()
        .find(|w| w.id == window_id)
        .map(|w| w.title.trim().to_owned())
        .filter(|t| !t.is_empty())
}

/// The error of a window that cannot be found, with its title when it was known at the start:
/// `The window "Title" is no longer available` instead of `Window 6882546 not found` (a number
/// means nothing to a person; Windows QA pass 6).
pub fn name_missing_window(e: CmError, window_id: u32, title: Option<&str>) -> CmError {
    match title {
        Some(t)
            if e.code() == cm_core::ErrorCode::NotFound
                && e.to_string() == format!("Window {window_id} not found") =>
        {
            CmError::not_found(format!("The window \"{t}\" is no longer available"))
        }
        _ => e,
    }
}

/// Frames of a window target whose "not found" names the window by its title.
struct NamedWindow {
    inner: Box<dyn FrameSource + Send>,
    window_id: Option<u32>,
    title: Option<String>,
}

impl FrameSource for NamedWindow {
    fn grab(&mut self) -> Result<Frame, CmError> {
        self.inner.grab().map_err(|e| match self.window_id {
            Some(id) => name_missing_window(e, id, self.title.as_deref()),
            None => e,
        })
    }

    fn target_changed(&mut self) -> bool {
        self.inner.target_changed()
    }
}

/// Parses the long-mode keys of `capture_screenshot`. `f` has `mode: "long"`.
pub(crate) fn parse(
    f: &crate::action::Fields<'_>,
    target: crate::action::ScreenshotTarget,
) -> Result<CaptureLongParams, CmError> {
    // Seconds to wait before the first frame (CAP-5), so the user can get a menu or a window
    // ready; checked when it runs.
    let delay_s = match f.obj.get("delay_s") {
        None | Some(Value::Null) => None,
        Some(v) => Some(
            v.as_f64()
                .ok_or_else(|| CmError::invalid_field("delay_s", "must be a number"))?,
        ),
    };
    let direction = match f.str_opt("direction")?.as_deref() {
        None | Some("vertical") => Direction::Vertical,
        Some("horizontal") => Direction::Horizontal,
        Some(_) => {
            return Err(CmError::invalid_field(
                "direction",
                "must be `vertical` or `horizontal`",
            ))
        }
    };
    let max_px = match f.obj.get("max_px") {
        None | Some(Value::Null) => None,
        Some(v) => Some(
            v.as_u64()
                .filter(|n| (1..=u64::from(MAX_LONG_SIDE)).contains(n))
                .map(|n| n as u32)
                .ok_or_else(|| {
                    CmError::invalid_field(
                        "max_px",
                        format!("must be an integer from 1 to {MAX_LONG_SIDE}"),
                    )
                })?,
        ),
    };
    let scroll = match f.str_opt("scroll")?.as_deref() {
        None | Some("auto") => ScrollMode::Auto,
        Some("hand") => ScrollMode::Hand,
        Some(_) => return Err(CmError::invalid_field("scroll", "must be `auto` or `hand`")),
    };
    Ok(CaptureLongParams {
        target,
        direction,
        max_px,
        scroll,
        delay_s,
        copy_to_clipboard: f.bool_or("copy_to_clipboard", false)?,
        title: f.str_opt("title")?,
        request_id: f.request_id()?,
    })
}

/// Parameters that are only valid with `mode: "long"`.
pub(crate) const LONG_ONLY_KEYS: &[&str] = &["direction", "max_px", "scroll"];

impl CaptureLongParams {
    /// An agent's Auto scroll moves the user's screen: Confirm tier (AC7).
    pub fn moves_the_screen(&self) -> bool {
        self.scroll == ScrollMode::Auto
    }
}

/// The area in global physical px. `None` when the target cannot be found (the scroll then goes
/// wherever the pointer is).
fn area_rect(capturer: &Capturer, target: cm_capture::CaptureTarget) -> Option<PhysicalRect> {
    use cm_capture::{CaptureTarget, RegionRect};
    let targets = capturer.list_targets().ok()?;
    let moved = |r: PhysicalRect, ox: i32, oy: i32| PhysicalRect {
        origin: PhysicalPoint {
            x: ox + r.origin.x,
            y: oy + r.origin.y,
        },
        size: r.size,
    };
    match target {
        CaptureTarget::Display { display_id } => {
            let d = targets.displays.iter().find(|d| d.id == display_id)?;
            Some(moved(d.bounds, 0, 0))
        }
        CaptureTarget::Window { window_id } => {
            let w = targets.windows.iter().find(|w| w.id == window_id)?;
            Some(moved(w.bounds, 0, 0))
        }
        CaptureTarget::Region { display_id, rect } => {
            let d = targets.displays.iter().find(|d| d.id == display_id)?;
            let r = match rect {
                RegionRect::Physical(r) => r,
                RegionRect::Logical(l) => {
                    let s = d.scale.get();
                    PhysicalRect {
                        origin: PhysicalPoint {
                            x: (l.origin.x * s).round() as i32,
                            y: (l.origin.y * s).round() as i32,
                        },
                        size: PhysicalSize {
                            width: (l.size.width * s).round() as u32,
                            height: (l.size.height * s).round() as u32,
                        },
                    }
                }
            };
            Some(moved(r, d.bounds.origin.x, d.bounds.origin.y))
        }
    }
}

/// Points of the area where the pointer is put for the wheel, in the order they are tried: the
/// centre, then the four quarter points. A wheel event goes to the window under the pointer, so
/// the points that one of this app's own windows covers (`own`, global physical px) come last: a
/// CLI or agent run starts with the app's window often on top of the target's centre
/// (QA macOS pass 3, Nov 15, 2026: `nothing_scrolled`). All of them stay in the list, so an area
/// that is covered everywhere is still tried.
pub(crate) fn scroll_points(area: PhysicalRect, own: &[PhysicalRect]) -> Vec<(i32, i32)> {
    let (w, h) = (area.size.width as i32, area.size.height as i32);
    let at = |fx: i32, fy: i32| (area.origin.x + w * fx / 4, area.origin.y + h * fy / 4);
    let all = [at(2, 2), at(1, 1), at(3, 1), at(1, 3), at(3, 3)];
    let covered = |&(x, y): &(i32, i32)| {
        own.iter().any(|r| {
            x >= r.origin.x
                && x < r.origin.x + r.size.width as i32
                && y >= r.origin.y
                && y < r.origin.y + r.size.height as i32
        })
    };
    let (mut free, taken): (Vec<_>, Vec<_>) = all.into_iter().partition(|p| !covered(p));
    free.extend(taken);
    free
}

/// A scroller that keeps a list of points of the area: it starts at the first and moves to the
/// next one when the run asks for another point (the first step moved nothing).
struct MultiPointScroller {
    system: Arc<dyn LongShotSystem>,
    /// The window target (its class decides which fallbacks the scroller has).
    window_id: Option<u32>,
    points: Vec<(i32, i32)>,
    at: usize,
    current: Box<dyn Scroller + Send>,
}

impl MultiPointScroller {
    fn new(
        system: Arc<dyn LongShotSystem>,
        window_id: Option<u32>,
        points: Vec<(i32, i32)>,
    ) -> Result<Self, CmError> {
        let current = system.scroller_for(points.first().copied(), window_id)?;
        Ok(MultiPointScroller {
            system,
            window_id,
            points,
            at: 0,
            current,
        })
    }
}

impl Scroller for MultiPointScroller {
    fn scroll(&mut self, direction: Direction, amount_px: u32) -> Result<(), CmError> {
        self.current.scroll(direction, amount_px)
    }

    fn scroll_back(&mut self, direction: Direction, amount_px: u32) -> Result<bool, CmError> {
        self.current.scroll_back(direction, amount_px)
    }

    fn last_resort(&mut self, direction: Direction, amount_px: u32) -> Result<bool, CmError> {
        self.current.last_resort(direction, amount_px)
    }

    fn next_point(&mut self) -> bool {
        if self.at + 1 >= self.points.len() {
            return false;
        }
        match self
            .system
            .scroller_for(Some(self.points[self.at + 1]), self.window_id)
        {
            Ok(s) => {
                self.at += 1;
                self.current = s;
                true
            }
            Err(_) => false,
        }
    }
}

/// The area of a Long Shot for the passive HUD: its display, the rectangle in that display's own
/// physical px and the display's scale. `None` when the target is not listed.
pub(crate) fn area_in_display(
    capturer: &Capturer,
    target: cm_capture::CaptureTarget,
) -> Option<(u32, [i64; 4], f64)> {
    use cm_capture::{CaptureTarget, RegionRect};
    let targets = capturer.list_targets().ok()?;
    let rect4 = |r: &PhysicalRect, ox: i32, oy: i32| {
        [
            i64::from(r.origin.x - ox),
            i64::from(r.origin.y - oy),
            i64::from(r.size.width),
            i64::from(r.size.height),
        ]
    };
    match target {
        CaptureTarget::Display { display_id } => {
            let d = targets.displays.iter().find(|d| d.id == display_id)?;
            Some((
                display_id,
                [
                    0,
                    0,
                    i64::from(d.bounds.size.width),
                    i64::from(d.bounds.size.height),
                ],
                d.scale.get(),
            ))
        }
        CaptureTarget::Window { window_id } => {
            let w = targets.windows.iter().find(|w| w.id == window_id)?;
            let d = targets
                .displays
                .iter()
                .find(|d| Some(d.id) == w.display_id)?;
            Some((
                d.id,
                rect4(&w.bounds, d.bounds.origin.x, d.bounds.origin.y),
                d.scale.get(),
            ))
        }
        CaptureTarget::Region { display_id, rect } => {
            let d = targets.displays.iter().find(|d| d.id == display_id)?;
            let s = d.scale.get();
            let r = match rect {
                RegionRect::Physical(r) => rect4(&r, 0, 0),
                RegionRect::Logical(l) => [
                    (l.origin.x * s).round() as i64,
                    (l.origin.y * s).round() as i64,
                    (l.size.width * s).round() as i64,
                    (l.size.height * s).round() as i64,
                ],
            };
            Some((display_id, r, s))
        }
    }
}

impl crate::daemon::Inner {
    /// `busy` for a request that would start a Long Shot or a Click Guide while one of them is
    /// running. Asked before the confirmation gate, so no dialog opens over what is captured.
    pub(crate) fn refuse_when_capture_is_busy(&self, action: &Action) -> Result<(), DaemonError> {
        let starts_long = matches!(action, Action::CaptureLong(_));
        let starts_guide = matches!(
            action,
            Action::Guide(g) if matches!(g.op, crate::guide::GuideOp::Start { .. })
        );
        if !starts_long && !starts_guide {
            return Ok(());
        }
        if self.delayed.long_running() {
            return Err(CmError::busy(BUSY_LONG_SHOT).into());
        }
        if self.guide.is_recording() {
            return Err(CmError::busy(BUSY_GUIDE).into());
        }
        Ok(())
    }
}

/// What `busy` says when a Long Shot or a Click Guide is already running.
pub(crate) const BUSY_LONG_SHOT: &str = "A Long Shot is already running. Stop it first.";
pub(crate) const BUSY_GUIDE: &str = "A Click Guide is already recording. Stop it first.";

/// AC7: Hand scroll is refused for agents.
pub(crate) fn refuse_hand_for_agent(p: &CaptureLongParams) -> Result<(), DaemonError> {
    if p.scroll == ScrollMode::Hand {
        return Err(DaemonError::WithData {
            error: CmError::invalid_input(
                "Hand scroll needs a person to scroll. Use `scroll: \"auto\"` (the user confirms it) or ask the user to start a Long Shot.",
            ),
            data: json!({ "reason": "needs_user" }),
        });
    }
    Ok(())
}

impl Inner {
    /// `long_shot_state`: pauses or resumes a running Long Shot and reports it (see
    /// [`crate::action::LongShotStateParams`]). `{ "running": false }` once it has ended.
    pub(crate) fn long_shot_state(
        &self,
        p: crate::action::LongShotStateParams,
    ) -> Result<Value, DaemonError> {
        let Some(st) = self.long_states.get(&p.request_id) else {
            return Ok(json!({ "running": false }));
        };
        if p.stop == Some(true) {
            // Stop (Esc, the HUD): the run ends at its next step and keeps what it has.
            let stopped = self
                .delayed
                .cancel_matching(|r| r.as_deref() == Some(p.request_id.as_str()));
            return Ok(json!({ "running": stopped == 0, "stopped": stopped > 0 }));
        }
        if let Some(paused) = p.paused {
            st.paused.store(paused, std::sync::atomic::Ordering::SeqCst);
        }
        let s = st
            .inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let mut out = json!({
            "running": true,
            "paused": st.is_paused(),
            "seq": s.seq,
            "frames": s.frames,
            "len_px": s.len_px,
            "max_len": s.max_len,
            "header_px": s.header_px,
            "footer_px": s.footer_px,
            "join_ok": !s.join_failed,
        });
        if p.since_seq.is_none_or(|n| n < s.seq) {
            if let Some((w, h, png)) = &s.preview {
                // The preview was made for the size the first caller asked; a later size is
                // served by the same picture (the panel scales it).
                let _ = (p.preview_w, p.preview_h);
                out["preview"] = json!({
                    "width": w,
                    "height": h,
                    "png_base64": base64::Engine::encode(
                        &base64::engine::general_purpose::STANDARD,
                        png,
                    ),
                });
            }
        }
        Ok(out)
    }
}

#[derive(Debug, Clone, Serialize)]
struct LongMeta {
    frames: u32,
    end: EndReason,
    note: Option<String>,
    /// The longest side this run was allowed (`max_px`, at most 16,384), so that the toast can
    /// name the limit it met.
    limit_px: u32,
    header_px: u32,
    footer_px: u32,
    direction: Direction,
}

impl Inner {
    pub(crate) async fn capture_long(
        &self,
        actor: &Actor,
        p: CaptureLongParams,
    ) -> Result<Value, DaemonError> {
        let system = self.long_shot.clone();
        if p.scroll == ScrollMode::Auto {
            match system.auto_scroll() {
                AutoScrollSupport::Available => {}
                AutoScrollSupport::NeedsAccessibility => {
                    return Err(CmError::permission_missing(
                        cm_core::Permission::Accessibility,
                        "Allow Accessibility to scroll for you (System Settings > Privacy & Security > Accessibility), or use Hand scroll",
                    )
                    .into())
                }
                AutoScrollSupport::NoInjection => {
                    return Err(CmError::not_implemented(
                        "Auto scroll on Wayland: the system cannot scroll for you. Use Hand scroll",
                    )
                    .into())
                }
                AutoScrollSupport::NotBuilt => {
                    return Err(CmError::not_implemented(
                        "Auto scroll on this system yet. Use Hand scroll",
                    )
                    .into())
                }
            }
        }
        let guard = self.delayed.register_kind(p.request_id.clone(), true);
        let token = guard.token();
        let capturer = self.capturer.clone();
        let capturer_for_anchor = self.capturer.clone();
        let target = p.target.to_capture_target();
        let delay = cm_capture::delay_from_secs(p.delay_s.unwrap_or(0.0))?;
        let (direction, max_px, scroll) = (p.direction, p.max_px, p.scroll);
        let events = self.events.clone();
        let request_id = p.request_id.clone();
        let request_id_after = p.request_id.clone();
        let meta: Arc<Mutex<Option<LongMeta>>> = Arc::default();
        let meta_in = meta.clone();
        let state = request_id.as_deref().map(|id| self.long_states.start(id));
        let state_in = state.clone();
        // The app grabs Esc for as long as a Long Shot runs, even one it did not start.
        let (area, window_title) = {
            let c = self.capturer.clone();
            tokio::task::spawn_blocking(move || {
                (area_in_display(&c, target), window_title(&c, target))
            })
            .await
            .ok()
            .unwrap_or((None, None))
        };
        self.events.publish(
            STARTED_EVENT,
            json!({
                "request_id": request_id_after,
                "client": match actor {
                    Actor::Agent(name) => json!(name),
                    Actor::User => Value::Null,
                },
                "scroll": match scroll {
                    ScrollMode::Auto => "auto",
                    ScrollMode::Hand => "hand",
                },
                "direction": direction,
                "display_id": area.map(|a| a.0),
                "area": area.map(|a| a.1),
                "scale": area.map(|a| a.2),
            }),
        );
        let out = self
            .store_capture_ex(actor, p.copy_to_clipboard, p.title, None, move || {
                // The delay, in steps so Esc cancels it.
                let end = std::time::Instant::now() + delay;
                while std::time::Instant::now() < end {
                    if token.is_cancelled() {
                        return Err(CmError::invalid_input("capture was cancelled"));
                    }
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
                // The confirmation panel (agents) or the picker has just closed: give the system
                // a moment to take it off the screen so it is not in the first frame.
                if matches!(scroll, ScrollMode::Auto) {
                    // A window another one covers is brought to the front first: the wheel goes
                    // to the window under the pointer (QA Windows pass 3: `nothing_scrolled`).
                    // Only where the system has a helper (Windows); elsewhere nothing happens.
                    if let cm_capture::CaptureTarget::Window { window_id } = target {
                        system.raise_window(window_id);
                    }
                    std::thread::sleep(std::time::Duration::from_millis(400));
                }
                let mut frames: Box<dyn FrameSource + Send> = Box::new(NamedWindow {
                    inner: system.frames(capturer, target),
                    window_id: match target {
                        cm_capture::CaptureTarget::Window { window_id } => Some(window_id),
                        _ => None,
                    },
                    title: window_title,
                });
                let cfg = LongShotConfig {
                    direction,
                    limits: Limits::with_max_px(max_px),
                    ..Default::default()
                };
                // Pause holds the loop at its next step (what is on screen is not captured until
                // Resume); Stop while paused still ends it.
                let pause_state = state_in.clone();
                let mut stop = || {
                    while pause_state.as_ref().is_some_and(|s| s.is_paused())
                        && !token.is_cancelled()
                    {
                        std::thread::sleep(std::time::Duration::from_millis(30));
                    }
                    token.is_cancelled()
                };
                let progress_state = state_in.clone();
                let mut on_progress = |pr: long_shot::LongShotProgress| {
                    if let Some(s) = &progress_state {
                        let mut g = s.inner.lock().unwrap_or_else(PoisonError::into_inner);
                        g.frames = pr.frames;
                        g.len_px = pr.len_px;
                        g.max_len = pr.max_len;
                        g.header_px = pr.header_px;
                        g.footer_px = pr.footer_px;
                        g.join_failed = !pr.last_join_ok;
                    }
                    events.publish(
                        PROGRESS_EVENT,
                        json!({
                            "request_id": request_id,
                            "frames": pr.frames,
                            "len_px": pr.len_px,
                            "max_len": pr.max_len,
                            "header_px": pr.header_px,
                            "footer_px": pr.footer_px,
                            "join_ok": pr.last_join_ok,
                        }),
                    );
                };
                let preview_state = state_in.clone();
                let mut on_stitch = |st: &long_shot::Stitcher| {
                    let Some(s) = &preview_state else { return };
                    let preview = st
                        .thumbnail(PREVIEW_MAX_W, PREVIEW_MAX_H)
                        .and_then(|f| f.to_png().ok().map(|png| (f.width(), f.height(), png)));
                    let mut g = s.inner.lock().unwrap_or_else(PoisonError::into_inner);
                    g.preview = preview;
                    g.seq += 1;
                };
                let result = match scroll {
                    ScrollMode::Auto => {
                        let points = match area_rect(&capturer_for_anchor, target) {
                            Some(area) => {
                                scroll_points(area, &capturer_for_anchor.own_window_rects())
                            }
                            None => Vec::new(),
                        };
                        let window_id = match target {
                            cm_capture::CaptureTarget::Window { window_id } => Some(window_id),
                            _ => None,
                        };
                        let mut scroller =
                            MultiPointScroller::new(system.clone(), window_id, points)?;
                        long_shot::run_auto_ex(
                            frames.as_mut(),
                            &mut scroller,
                            &mut ThreadSleeper,
                            cfg,
                            &mut stop,
                            &mut on_progress,
                            &mut on_stitch,
                        )?
                    }
                    ScrollMode::Hand => long_shot::run_hand_ex(
                        frames.as_mut(),
                        &mut ThreadSleeper,
                        cfg,
                        &mut stop,
                        &mut on_progress,
                        &mut on_stitch,
                    )?,
                };
                let along = match direction {
                    Direction::Vertical => result.image.frame.height(),
                    Direction::Horizontal => result.image.frame.width(),
                };
                *meta_in.lock().unwrap_or_else(PoisonError::into_inner) = Some(LongMeta {
                    frames: result.image.frames,
                    end: result.end,
                    note: result.end.note_for(cfg.limits, along),
                    limit_px: cfg.limits.max_len,
                    header_px: result.image.header_len,
                    footer_px: result.image.footer_len,
                    direction,
                });
                let frame = result.image.frame;
                let size = frame.size();
                let mut extra = serde_json::Map::new();
                extra.insert("source".into(), json!("long_shot"));
                extra.insert("frames".into(), json!(result.image.frames));
                extra.insert("long_shot_end".into(), json!(result.end));
                Ok((
                    Capture {
                        frame,
                        kind: CaptureKind::Region,
                        display_id: None,
                        window_id: None,
                        scale: cm_core::ScaleFactor::ONE,
                        source: PhysicalRect {
                            origin: PhysicalPoint { x: 0, y: 0 },
                            size: PhysicalSize {
                                width: size.width,
                                height: size.height,
                            },
                        },
                        waited: std::time::Duration::ZERO,
                    },
                    extra,
                ))
            })
            .await;
        if let Some(id) = request_id_after.as_deref() {
            self.long_states.finish(id);
        }
        self.events
            .publish(ENDED_EVENT, json!({ "request_id": request_id_after }));
        let mut out = out?;
        drop(guard);
        let m = meta.lock().unwrap_or_else(PoisonError::into_inner).take();
        if let Some(m) = m {
            if let Some(note) = &m.note {
                if let Some(w) = out["warnings"].as_array_mut() {
                    w.push(json!(note));
                }
            }
            out["source"] = json!("long_shot");
            out["long_shot"] = serde_json::to_value(&m).unwrap_or(Value::Null);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod point_tests {
    use super::*;

    fn rect(x: i32, y: i32, w: u32, h: u32) -> PhysicalRect {
        PhysicalRect {
            origin: PhysicalPoint { x, y },
            size: PhysicalSize {
                width: w,
                height: h,
            },
        }
    }

    /// No window of ours on the area: the centre first, then the quarter points.
    #[test]
    fn ls_flaky_the_centre_is_first_when_nothing_covers_it() {
        let p = scroll_points(rect(100, 200, 800, 400), &[]);
        assert_eq!(p[0], (500, 400));
        assert_eq!(p.len(), 5);
        assert!(p.contains(&(300, 300)) && p.contains(&(700, 500)));
    }

    /// LS-flaky (b): the app's own window covers the centre; the wheel must not go there.
    #[test]
    fn ls_flaky_a_point_under_our_own_window_is_not_first() {
        let area = rect(0, 0, 1000, 800);
        let own = [rect(300, 200, 400, 400)];
        let p = scroll_points(area, &own);
        assert_eq!(p.len(), 5);
        assert_ne!(p[0], (500, 400));
        let covered = |&(x, y): &(i32, i32)| (300..700).contains(&x) && (200..600).contains(&y);
        assert!(!covered(&p[0]));
        // The covered centre is kept, as a last resort.
        assert!(p.contains(&(500, 400)));
        assert!(covered(p.last().unwrap()));
    }

    /// Covered everywhere: all points stay, the centre first.
    #[test]
    fn ls_flaky_an_area_covered_everywhere_keeps_every_point() {
        let area = rect(0, 0, 1000, 800);
        let p = scroll_points(area, &[rect(-10, -10, 2000, 2000)]);
        assert_eq!(p.len(), 5);
        assert_eq!(p[0], (500, 400));
    }

    type Log = Arc<Mutex<Vec<Option<(i32, i32)>>>>;

    struct Anchored {
        at: Option<(i32, i32)>,
        log: Log,
    }

    impl Scroller for Anchored {
        fn scroll(&mut self, _: Direction, _: u32) -> Result<(), CmError> {
            self.log.lock().unwrap().push(self.at);
            Ok(())
        }
    }

    struct Sys(Log);

    impl LongShotSystem for Sys {
        fn frames(
            &self,
            _: Arc<Capturer>,
            _: cm_capture::CaptureTarget,
        ) -> Box<dyn FrameSource + Send> {
            unreachable!("frames are not taken in this test")
        }
        fn scroller(
            &self,
            anchor: Option<(i32, i32)>,
        ) -> Result<Box<dyn Scroller + Send>, CmError> {
            Ok(Box::new(Anchored {
                at: anchor,
                log: self.0.clone(),
            }))
        }
        fn auto_scroll(&self) -> AutoScrollSupport {
            AutoScrollSupport::Available
        }
    }

    /// The scroller starts at the first point and moves on when asked, until the list is used up.
    #[test]
    fn ls_flaky_the_scroller_moves_to_the_next_point_when_asked() {
        let log = Log::default();
        let points = vec![(1, 1), (2, 2), (3, 3)];
        let mut s = MultiPointScroller::new(Arc::new(Sys(log.clone())), None, points).unwrap();
        s.scroll(Direction::Vertical, 10).unwrap();
        assert!(s.next_point());
        s.scroll(Direction::Vertical, 10).unwrap();
        assert!(s.next_point());
        assert!(!s.next_point());
        s.scroll(Direction::Vertical, 10).unwrap();
        assert_eq!(
            *log.lock().unwrap(),
            vec![Some((1, 1)), Some((2, 2)), Some((3, 3))]
        );
    }

    /// No target found: no point, the scroller is made without an anchor as before.
    #[test]
    fn ls_flaky_no_points_means_no_anchor_and_no_next_point() {
        let log = Log::default();
        let mut s = MultiPointScroller::new(Arc::new(Sys(log.clone())), None, Vec::new()).unwrap();
        s.scroll(Direction::Vertical, 10).unwrap();
        assert!(!s.next_point());
        assert_eq!(*log.lock().unwrap(), vec![None]);
    }

    /// QA Windows pass 3 (unresolved): a horizontal Long Shot of a region ended after one frame in
    /// one run. A region whose origin is not the display's origin, on a display that is not at
    /// (0, 0) and is scaled: the offset is applied once (the crop inside the display, the scroll
    /// points inside the global area) and the scale once (a logical rect only; a physical rect, as
    /// the action passes it, never).
    #[test]
    fn ls_region_offset_and_scale_are_applied_once() {
        use cm_capture::fake::{pattern_pixel, FakeBackend};
        use cm_capture::{CaptureTarget, RegionRect};
        let cap = Capturer::new(FakeBackend::new().with_display(7, 1920, -200, 1600, 1000, 2.0));
        let phys = rect(300, 100, 800, 600);
        let target = CaptureTarget::Region {
            display_id: 7,
            rect: RegionRect::Physical(phys),
        };
        // Global area: display origin plus the region's display-relative origin, once.
        assert_eq!(area_rect(&cap, target), Some(rect(2220, -100, 800, 600)));
        // What the overlay is told: display-relative, physical, with the display's scale.
        assert_eq!(
            area_in_display(&cap, target),
            Some((7, [300, 100, 800, 600], 2.0))
        );
        // The wheel goes to the centre of the global area.
        assert_eq!(
            scroll_points(area_rect(&cap, target).unwrap(), &[])[0],
            (2620, 200)
        );
        // The frame is the crop at the display-relative origin, once.
        let frame = cap
            .capture(CaptureRequest::new(target), None)
            .unwrap()
            .unwrap()
            .frame;
        assert_eq!((frame.width(), frame.height()), (800, 600));
        assert_eq!(frame.pixel(0, 0), Some(pattern_pixel(300, 100)));
        assert_eq!(frame.pixel(799, 599), Some(pattern_pixel(1099, 699)));
        // A logical rect is scaled once: the same area.
        let logical = CaptureTarget::Region {
            display_id: 7,
            rect: RegionRect::Logical(cm_core::Rect::new(150.0, 50.0, 400.0, 300.0)),
        };
        assert_eq!(area_rect(&cap, logical), area_rect(&cap, target));
        assert_eq!(
            area_in_display(&cap, logical),
            area_in_display(&cap, target)
        );
    }
}
