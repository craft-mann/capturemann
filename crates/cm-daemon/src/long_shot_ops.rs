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
/// `footer_px`, `request_id`): the HUD counter and the side preview follow it.
pub const PROGRESS_EVENT: &str = "capture.long_shot.progress";

/// What touches the screen: frames of the target and the scroller. The real one uses the
/// capturer and `cm_capture::long_shot::platform_scroller`; tests give scripted ones.
pub trait LongShotSystem: Send + Sync {
    fn frames(
        &self,
        capturer: Arc<Capturer>,
        target: cm_capture::CaptureTarget,
    ) -> Box<dyn FrameSource + Send>;
    fn scroller(&self) -> Result<Box<dyn Scroller + Send>, CmError>;
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

    fn scroller(&self) -> Result<Box<dyn Scroller + Send>, CmError> {
        long_shot::platform_scroller()
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
        let guard = self.delayed.register(p.request_id.clone());
        let token = guard.token();
        let capturer = self.capturer.clone();
        let target = p.target.to_capture_target();
        let delay = cm_capture::delay_from_secs(p.delay_s.unwrap_or(0.0))?;
        let (direction, max_px, scroll) = (p.direction, p.max_px, p.scroll);
        let events = self.events.clone();
        let request_id = p.request_id.clone();
        let meta: Arc<Mutex<Option<LongMeta>>> = Arc::default();
        let meta_in = meta.clone();
        let mut out = self
            .store_capture(actor, p.copy_to_clipboard, p.title, None, move || {
                // The delay, in steps so Esc cancels it.
                let end = std::time::Instant::now() + delay;
                while std::time::Instant::now() < end {
                    if token.is_cancelled() {
                        return Err(CmError::invalid_input("capture was cancelled"));
                    }
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
                let mut frames = system.frames(capturer, target);
                let cfg = LongShotConfig {
                    direction,
                    limits: Limits::with_max_px(max_px),
                    ..Default::default()
                };
                let mut stop = || token.is_cancelled();
                let mut on_progress = |pr: long_shot::LongShotProgress| {
                    events.publish(
                        PROGRESS_EVENT,
                        json!({
                            "request_id": request_id,
                            "frames": pr.frames,
                            "len_px": pr.len_px,
                            "max_len": pr.max_len,
                            "header_px": pr.header_px,
                            "footer_px": pr.footer_px,
                        }),
                    );
                };
                let result = match scroll {
                    ScrollMode::Auto => {
                        let mut scroller = system.scroller()?;
                        long_shot::run_auto(
                            frames.as_mut(),
                            scroller.as_mut(),
                            &mut ThreadSleeper,
                            cfg,
                            &mut stop,
                            &mut on_progress,
                        )?
                    }
                    ScrollMode::Hand => long_shot::run_hand(
                        frames.as_mut(),
                        &mut ThreadSleeper,
                        cfg,
                        &mut stop,
                        &mut on_progress,
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
                Ok(Capture {
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
                })
            })
            .await?;
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
