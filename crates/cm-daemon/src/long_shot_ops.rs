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

use crate::action::{CaptureLongParams, ScrollMode};
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
    fn auto_scroll(&self) -> AutoScrollSupport;
}

/// The real screen.
pub struct RealLongShot;

struct CapturerFrames {
    capturer: Arc<Capturer>,
    target: cm_capture::CaptureTarget,
}

impl FrameSource for CapturerFrames {
    fn grab(&mut self) -> Result<Frame, CmError> {
        self.capturer
            .capture(CaptureRequest::new(self.target), None)?
            .map(|c| c.frame)
            .ok_or_else(|| CmError::invalid_input("capture was cancelled"))
    }
}

impl LongShotSystem for RealLongShot {
    fn frames(
        &self,
        capturer: Arc<Capturer>,
        target: cm_capture::CaptureTarget,
    ) -> Box<dyn FrameSource + Send> {
        Box::new(CapturerFrames { capturer, target })
    }

    fn scroller(&self, anchor: Option<(i32, i32)>) -> Result<Box<dyn Scroller + Send>, CmError> {
        long_shot::platform_scroller_at(anchor)
    }

    fn auto_scroll(&self) -> AutoScrollSupport {
        long_shot::auto_scroll_support()
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

/// The centre of the area in global physical px: where the pointer goes so that the wheel
/// reaches the area. `None` when the target cannot be found (the scroll then goes wherever the
/// pointer is).
fn area_centre(capturer: &Capturer, target: cm_capture::CaptureTarget) -> Option<(i32, i32)> {
    use cm_capture::{CaptureTarget, RegionRect};
    let targets = capturer.list_targets().ok()?;
    let centre = |r: PhysicalRect, ox: i32, oy: i32| {
        (
            ox + r.origin.x + (r.size.width / 2) as i32,
            oy + r.origin.y + (r.size.height / 2) as i32,
        )
    };
    match target {
        CaptureTarget::Display { display_id } => {
            let d = targets.displays.iter().find(|d| d.id == display_id)?;
            Some(centre(d.bounds, 0, 0))
        }
        CaptureTarget::Window { window_id } => {
            let w = targets.windows.iter().find(|w| w.id == window_id)?;
            Some(centre(w.bounds, 0, 0))
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
            Some(centre(r, d.bounds.origin.x, d.bounds.origin.y))
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
    note: Option<&'static str>,
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
        let area = {
            let c = self.capturer.clone();
            tokio::task::spawn_blocking(move || area_in_display(&c, target))
                .await
                .ok()
                .flatten()
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
                    std::thread::sleep(std::time::Duration::from_millis(400));
                }
                let mut frames = system.frames(capturer, target);
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
                        let anchor = area_centre(&capturer_for_anchor, target);
                        let mut scroller = system.scroller(anchor)?;
                        long_shot::run_auto_ex(
                            frames.as_mut(),
                            scroller.as_mut(),
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
                *meta_in.lock().unwrap_or_else(PoisonError::into_inner) = Some(LongMeta {
                    frames: result.image.frames,
                    end: result.end,
                    note: result.end.note(),
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
            if let Some(note) = m.note {
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
