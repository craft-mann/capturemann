//! The target of a capture in a confirmation dialog, in words (`Display 1`, `Window: Editor
//! (Code)`, `Region on display 1`), not as the raw `{ "display_id": 1 }` of the call (the generic
//! dialog of a tightened tool and the Long Shot dialog showed `{..}`: QA macOS pass 7).
//!
//! The wording is built from the live list of windows, so the dialog says which window a call
//! wants to record, with titles made safe for display. The typed form ([`typed`]) has the shape
//! of the `target` of `start_recording`'s plan (`type`, `id`, `title`, `app`, `width`, `height`),
//! so the app words it in the language of the interface.

use cm_capture::Targets;
use serde_json::{json, Value};

use crate::action::{Action, ScreenshotTarget};
use crate::daemon::{blocking, Inner};
use crate::guide::GuideOp;
use crate::recording::engine::describe::dialog_text;

/// The target a call names, for the actions that capture or follow something on the screen.
pub(crate) fn target_of(action: &Action) -> Option<ScreenshotTarget> {
    match action {
        Action::CaptureLong(p) => Some(p.target),
        Action::Guide(g) => match &g.op {
            GuideOp::Start { target, .. } => Some(*target),
            _ => None,
        },
        _ => None,
    }
}

/// `Display 1`, `Window: Editor (Code)` or `Region on display 1`, in English (the daemon's text
/// for clients that do not word it themselves). A window that is not listed (gone, or the
/// listing failed) is `Window 10`.
pub(crate) fn text(t: &ScreenshotTarget, targets: Option<&Targets>) -> String {
    match t {
        ScreenshotTarget::Display { display_id } => format!("Display {display_id}"),
        ScreenshotTarget::Region { display_id, .. } => format!("Region on display {display_id}"),
        ScreenshotTarget::Window { window_id } => {
            match targets.and_then(|t| t.windows.iter().find(|w| w.id == *window_id)) {
                Some(w) => {
                    let (title, app) = (dialog_text(&w.title), dialog_text(&w.app));
                    match (title.trim().is_empty(), app.trim().is_empty()) {
                        (false, false) => format!("Window: {title} ({app})"),
                        (false, true) => format!("Window: {title}"),
                        (true, false) => format!("Window of {app}"),
                        (true, true) => format!("Window {window_id}"),
                    }
                }
                None => format!("Window {window_id}"),
            }
        }
    }
}

/// The typed `target` of the confirmation parameters (see the module docs).
pub(crate) fn typed(t: &ScreenshotTarget, targets: Option<&Targets>) -> Value {
    match t {
        ScreenshotTarget::Display { display_id } => {
            let d = targets.and_then(|t| t.displays.iter().find(|d| d.id == *display_id));
            json!({ "type": "display", "id": display_id,
                    "name": d.map(|d| dialog_text(&d.name)),
                    "width": d.map(|d| d.bounds.size.width),
                    "height": d.map(|d| d.bounds.size.height) })
        }
        ScreenshotTarget::Window { window_id } => {
            let w = targets.and_then(|t| t.windows.iter().find(|w| w.id == *window_id));
            json!({ "type": "window", "id": window_id,
                    "title": w.map(|w| dialog_text(&w.title)),
                    "app": w.map(|w| dialog_text(&w.app)),
                    "width": w.map(|w| w.bounds.size.width),
                    "height": w.map(|w| w.bounds.size.height) })
        }
        ScreenshotTarget::Region { display_id, rect } => {
            json!({ "type": "region", "display_id": display_id,
                    "x": rect[0], "y": rect[1], "width": rect[2], "height": rect[3] })
        }
    }
}

impl Inner {
    /// Words the target of `action` in the rows (`details`) and the typed parameters (`params`)
    /// of its confirmation. Anything that is not such an action, or whose rows are not objects,
    /// is left alone.
    pub(crate) async fn word_target(
        &self,
        action: &Action,
        details: &mut Value,
        params: &mut Value,
    ) {
        let Some(t) = target_of(action) else { return };
        let capturer = self.capturer.clone();
        // Only a window needs the live list (its title); a failed listing falls back to ids.
        let targets = if matches!(t, ScreenshotTarget::Window { .. }) {
            blocking(move || capturer.list_targets_with(true))
                .await
                .ok()
        } else {
            let capturer = self.capturer.clone();
            blocking(move || capturer.list_displays()).await.ok()
        };
        if let Some(o) = details.as_object_mut() {
            o.insert("target".into(), json!(text(&t, targets.as_ref())));
        }
        if let Some(o) = params.as_object_mut() {
            o.insert("target".into(), typed(&t, targets.as_ref()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_target_is_said_in_words() {
        assert_eq!(
            text(&ScreenshotTarget::Display { display_id: 1 }, None),
            "Display 1"
        );
        assert_eq!(
            text(
                &ScreenshotTarget::Region {
                    display_id: 2,
                    rect: [0, 0, 10, 10]
                },
                None
            ),
            "Region on display 2"
        );
        assert_eq!(
            text(&ScreenshotTarget::Window { window_id: 10 }, None),
            "Window 10"
        );
    }
}
