//! The real click source: the REC-7 listener of `cm-cursor` (mouse-down events only) and the
//! mapping from the OS's global space to the followed target.

use std::sync::{Mutex, PoisonError};

use cm_capture::{CaptureTarget, RegionRect, Targets};
use cm_core::{CmError, Rect};
use cm_cursor::{Button, ClickListener, CursorTarget, TargetKind};

use super::{ClickSink, GuideClicks, LocalClick};

/// macOS reports the pointer in logical points, Windows and X11 in physical px.
fn div_for(scale: f64) -> f64 {
    if cfg!(target_os = "macos") {
        scale
    } else {
        1.0
    }
}

/// The followed target in the cursor crate's coordinate convention (the same rules as the cursor
/// track of a recording).
pub(crate) fn cursor_target(
    target: &CaptureTarget,
    targets: &Targets,
) -> Result<CursorTarget, CmError> {
    let display = |id: u32| {
        targets
            .displays
            .iter()
            .find(|d| d.id == id)
            .ok_or_else(|| CmError::not_found(format!("no display {id}")))
    };
    let rect =
        |x: f64, y: f64, w: f64, h: f64, div: f64| Rect::new(x / div, y / div, w / div, h / div);
    match target {
        CaptureTarget::Display { display_id } => {
            let d = display(*display_id)?;
            let div = div_for(d.scale.get());
            Ok(CursorTarget::display(
                d.id,
                rect(
                    f64::from(d.bounds.origin.x),
                    f64::from(d.bounds.origin.y),
                    f64::from(d.bounds.size.width),
                    f64::from(d.bounds.size.height),
                    div,
                ),
                div,
            ))
        }
        CaptureTarget::Region {
            display_id,
            rect: r,
        } => {
            let d = display(*display_id)?;
            let div = div_for(d.scale.get());
            let (rx, ry, rw, rh) = match r {
                RegionRect::Physical(p) => (
                    f64::from(p.origin.x),
                    f64::from(p.origin.y),
                    f64::from(p.size.width),
                    f64::from(p.size.height),
                ),
                RegionRect::Logical(l) => {
                    let s = d.scale.get();
                    (
                        l.origin.x * s,
                        l.origin.y * s,
                        l.size.width * s,
                        l.size.height * s,
                    )
                }
            };
            Ok(CursorTarget::from_global_rect(
                TargetKind::Region,
                Some(d.id),
                rect(
                    f64::from(d.bounds.origin.x) + rx,
                    f64::from(d.bounds.origin.y) + ry,
                    rw,
                    rh,
                    div,
                ),
                div,
            ))
        }
        CaptureTarget::Window { window_id } => {
            let w = targets
                .windows
                .iter()
                .find(|w| w.id == *window_id)
                .ok_or_else(|| CmError::not_found(format!("no window {window_id}")))?;
            let div = div_for(w.scale.get());
            Ok(CursorTarget::from_global_rect(
                TargetKind::Window,
                w.display_id,
                rect(
                    f64::from(w.bounds.origin.x),
                    f64::from(w.bounds.origin.y),
                    f64::from(w.bounds.size.width),
                    f64::from(w.bounds.size.height),
                    div,
                ),
                div,
            ))
        }
    }
}

/// The listener of this system, behind [`GuideClicks`].
#[derive(Default)]
pub struct SystemClicks {
    listener: Mutex<Option<ClickListener>>,
}

impl GuideClicks for SystemClicks {
    fn start(
        &self,
        target: &CaptureTarget,
        targets: &Targets,
        mut sink: ClickSink,
    ) -> Result<(), CmError> {
        let map = cursor_target(target, targets)?;
        let listener = ClickListener::start(move |c| {
            let (x, y, outside) = map.map(c.gx, c.gy);
            sink(LocalClick {
                x: f64::from(x),
                y: f64::from(y),
                gx: c.gx,
                gy: c.gy,
                outside,
                button: match c.button {
                    Button::Right => "right",
                    Button::Middle => "middle",
                    _ => "left",
                }
                .to_owned(),
                host_ns: c.host_ns,
            });
        })?;
        *self.listener.lock().unwrap_or_else(PoisonError::into_inner) = Some(listener);
        Ok(())
    }

    fn stop(&self) {
        let l = self
            .listener
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        drop(l);
    }
}
