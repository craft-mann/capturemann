//! `suggest_blueprint` (ANN-11): the text lines of a screenshot as boxes for a `blueprint` op.
//!
//! Free and read-only: nothing is stored, no revision is made, nothing leaves the computer. It
//! recognizes the text once; the `blueprint` op then stores the boxes (and the fill and bar colour
//! measured for each), so a render never runs text recognition again. It reads the original
//! picture with the capture's own base edits applied (a Retype or an earlier Blueprint), never
//! the annotations drawn over it, so the boxes and measured colours match what a Blueprint
//! base edit will paint on.

use cm_core::annotation::MAX_BLUEPRINT_BOXES;
use cm_core::CmError;
use serde_json::{json, Value};

use crate::action::{Action, Fields};
use crate::daemon::{blocking, lock_library, Inner};
use crate::error::DaemonError;

pub(crate) const METHOD: &str = "suggest_blueprint";

/// Longest `text_preview`, in characters.
const PREVIEW_CHARS: usize = 60;

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SuggestBlueprintParams {
    pub capture_id: String,
}

pub(crate) fn parse(f: &Fields<'_>) -> Result<Action, CmError> {
    f.only(&["capture_id"])?;
    Ok(Action::SuggestBlueprint(SuggestBlueprintParams {
        capture_id: f.str_req("capture_id")?,
    }))
}

fn preview(text: &str) -> String {
    let t = text.trim();
    match t.char_indices().nth(PREVIEW_CHARS) {
        Some((cut, _)) => format!("{}...", &t[..cut]),
        None => t.to_owned(),
    }
}

impl Inner {
    pub(crate) async fn suggest_blueprint(
        &self,
        p: SuggestBlueprintParams,
    ) -> Result<Value, DaemonError> {
        let recognizer = self.recognizer.clone();
        if !recognizer.is_available() {
            let how = recognizer.install_hint().unwrap_or_default();
            return Err(CmError::not_implemented(format!(
                "text recognition is not available on this machine, so a blueprint cannot be \
                 suggested{}",
                if how.is_empty() {
                    String::new()
                } else {
                    format!(" ({how})")
                }
            ))
            .into());
        }
        let library = self.library.clone();
        let out = blocking(move || {
            let bundle = lock_library(&library).open_screenshot(&p.capture_id)?;
            let original = image::open(bundle.original_path())
                .map_err(|e| CmError::internal(format!("cannot read original.png: {e}")))?
                .to_rgba8();
            // Only the base edits: the pixels a Blueprint will paint on.
            let base_ops: Vec<_> = bundle
                .ops()
                .iter()
                .filter(|o| o.kind.is_base_edit())
                .cloned()
                .collect();
            let image = if base_ops.is_empty() {
                original
            } else {
                cm_render::render_annotations(&original, &base_ops)?
            };
            let mut lines: Vec<_> = recognizer
                .recognize(&image)?
                .into_iter()
                .filter(|l| !l.text.trim().is_empty() && l.rect.w >= 1.0 && l.rect.h >= 1.0)
                .collect();
            // Reading order: top to bottom, then left to right.
            lines.sort_by(|a, b| {
                a.rect
                    .y
                    .total_cmp(&b.rect.y)
                    .then(a.rect.x.total_cmp(&b.rect.x))
            });
            lines.truncate(MAX_BLUEPRINT_BOXES);
            let (iw, ih) = (f64::from(image.width()), f64::from(image.height()));
            // Whole pixels, outward, inside the image.
            let rects: Vec<cm_core::Rc> = lines
                .iter()
                .map(|l| {
                    let (x0, y0) = (l.rect.x.floor().max(0.0), l.rect.y.floor().max(0.0));
                    let (x1, y1) = (
                        (l.rect.x + l.rect.w).ceil().min(iw),
                        (l.rect.y + l.rect.h).ceil().min(ih),
                    );
                    cm_core::Rc::new(x0, y0, (x1 - x0).max(1.0), (y1 - y0).max(1.0))
                })
                .collect();
            let styles = cm_render::measure_blueprint(&image, &rects);
            let boxes: Vec<Value> = lines
                .iter()
                .zip(&rects)
                .zip(&styles)
                .map(|((l, r), s)| {
                    json!({
                        "rect": [r.x, r.y, r.w, r.h],
                        "text_preview": preview(&l.text),
                        "confidence": (f64::from(l.confidence) * 100.0).round() / 100.0,
                        "fill": s.fill,
                        "color": s.color,
                        "busy": s.busy,
                    })
                })
                .collect();
            Ok(json!({
                "capture_id": p.capture_id,
                "recognizer": recognizer.name(),
                "image_size": [image.width(), image.height()],
                "boxes": boxes,
            }))
        })
        .await?;
        Ok(out)
    }
}
