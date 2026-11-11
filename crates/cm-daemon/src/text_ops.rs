//! `recognize_text` (ANN-5): copy the text out of a screenshot.
//!
//! Free and read-only: nothing is stored, no revision is made, nothing leaves the computer. It
//! reads the **rendered** image (annotations and redactions applied), never `original.png`, so
//! text under a blur or pixelate is not read back (RED-2.AC1); this is the opposite of
//! `suggest_redactions` on purpose. The recognizer runs on a worker thread without the library
//! lock.

use cm_core::{CmError, Rc};
use cm_render::ocr::{
    layout_text, recognize_region, word_count, LayoutOptions, PixelRegion, TextLine,
};
use image::RgbaImage;
use serde_json::{json, Value};

use crate::action::{Action, Fields};
use crate::daemon::{blocking, lock_library, Inner};
use crate::error::DaemonError;

pub(crate) const METHOD: &str = "recognize_text";

/// Largest region (width x height, px) one call reads. A 5K Retina screenshot is 14.7 MP.
pub const MAX_REGION_PIXELS: u64 = 64_000_000;
/// Longest text one call returns or copies, in characters; the rest is cut and `truncated` is true.
pub const MAX_TEXT_CHARS: usize = 100_000;

/// What the reply carries besides the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TextFormat {
    /// `text` only.
    #[default]
    Text,
    /// `text` and `lines` with a box and confidence each.
    Lines,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct RecognizeTextParams {
    pub capture_id: String,
    /// `[x, y, w, h]` in pixels of the original image; `None` is the whole (cropped) image.
    pub region: Option<[f64; 4]>,
    pub copy_to_clipboard: bool,
    pub format: TextFormat,
}

pub(crate) fn parse(f: &Fields<'_>) -> Result<Action, CmError> {
    f.only(&["capture_id", "region", "copy_to_clipboard", "format"])?;
    let region = match f.obj.get("region") {
        None | Some(Value::Null) => None,
        Some(v) => {
            let bad = || CmError::invalid_field("region", "must be [x, y, w, h], four numbers");
            let a = v.as_array().filter(|a| a.len() == 4).ok_or_else(bad)?;
            let mut out = [0.0; 4];
            for (slot, n) in out.iter_mut().zip(a) {
                *slot = n.as_f64().filter(|n| n.is_finite()).ok_or_else(bad)?;
            }
            Some(out)
        }
    };
    let format = match f.str_opt("format")?.as_deref() {
        None | Some("text") => TextFormat::Text,
        Some("lines") => TextFormat::Lines,
        Some(_) => {
            return Err(CmError::invalid_field(
                "format",
                "must be \"text\" or \"lines\"",
            ))
        }
    };
    Ok(Action::RecognizeText(RecognizeTextParams {
        capture_id: f.str_req("capture_id")?,
        region,
        copy_to_clipboard: f.bool_or("copy_to_clipboard", false)?,
        format,
    }))
}

/// The rendered image of a screenshot and the origin of its pixels in the original image (a crop
/// op moves it).
fn rendered_image(bundle: &cm_library::ShotBundle) -> Result<(RgbaImage, (u32, u32)), CmError> {
    let png = if bundle.ops().is_empty() {
        std::fs::read(bundle.original_path())
    } else if bundle.manifest().render.is_some() && !bundle.render_is_stale() {
        std::fs::read(bundle.render_path())
    } else {
        Ok(crate::handlers::render_png(bundle)?)
    }
    .map_err(|e| CmError::internal(format!("cannot read the image: {e}")))?;
    let image = image::load_from_memory(&png)
        .map_err(|e| CmError::internal(format!("cannot decode the image: {e}")))?
        .to_rgba8();
    let origin = bundle.visible_rect().map_or((0, 0), |r| {
        let r = r.normalized();
        (r.x.round().max(0.0) as u32, r.y.round().max(0.0) as u32)
    });
    Ok((image, origin))
}

/// The recognized text of `lines` cut to [`MAX_TEXT_CHARS`]; the flag says it was cut.
pub(crate) fn limit_text(mut text: String) -> (String, bool) {
    match text.char_indices().nth(MAX_TEXT_CHARS) {
        Some((cut, _)) => {
            text.truncate(cut);
            (text.trim_end().to_owned(), true)
        }
        None => (text, false),
    }
}

fn rect_json(r: Rc, ox: f64, oy: f64) -> Value {
    json!([
        (r.x + ox).round(),
        (r.y + oy).round(),
        r.w.round(),
        r.h.round()
    ])
}

impl Inner {
    pub(crate) async fn recognize_text(
        &self,
        p: RecognizeTextParams,
    ) -> Result<Value, DaemonError> {
        let recognizer = self.recognizer.clone();
        if !recognizer.is_available() {
            let how = recognizer.install_hint().unwrap_or_default();
            return Err(CmError::not_implemented(format!(
                "text recognition is not available on this machine, so text cannot be copied \
                 from a screenshot{}",
                if how.is_empty() {
                    String::new()
                } else {
                    format!(" ({how})")
                }
            ))
            .into());
        }
        // Read the image under the capture's lock (an `annotate` in flight finishes first), then
        // let go of it for the slow part.
        let serial = self.capture_locks.lock(&p.capture_id).await;
        let library = self.library.clone();
        let id = p.capture_id.clone();
        let (image, origin) = blocking(move || {
            let bundle = lock_library(&library).open_screenshot(&id)?;
            rendered_image(&bundle)
        })
        .await?;
        drop(serial);

        let clipboard = self.clipboard.clone();
        blocking(move || {
            let (ox, oy) = (f64::from(origin.0), f64::from(origin.1));
            let size = image.dimensions();
            let region = match p.region {
                // The caller's numbers are original-image pixels; the image we hold may be a crop.
                Some([x, y, w, h]) => PixelRegion::clamp(Rc::new(x - ox, y - oy, w, h), size)?,
                None => PixelRegion::whole(size),
            };
            if u64::from(region.w) * u64::from(region.h) > MAX_REGION_PIXELS {
                return Err(CmError::invalid_field(
                    "region",
                    format!("is larger than {MAX_REGION_PIXELS} pixels; read it in parts"),
                ));
            }
            let lines: Vec<TextLine> = recognize_region(recognizer.as_ref(), &image, region)?;
            let (text, truncated) = limit_text(layout_text(&lines, &LayoutOptions::default()));
            let mut warnings: Vec<String> = Vec::new();
            let copied = p.copy_to_clipboard
                && !text.is_empty()
                && match clipboard.copy_text(&text) {
                    Ok(()) => true,
                    Err(e) => {
                        warnings.push(format!("Not copied to the clipboard: {e}"));
                        false
                    }
                };
            let mut out = json!({
                "capture_id": p.capture_id,
                "recognizer": recognizer.name(),
                "region": rect_json(region.rc(), ox, oy),
                "text": text,
                "word_count": word_count(&text),
                "truncated": truncated,
                "copied_to_clipboard": copied,
                "warnings": warnings,
            });
            if p.format == TextFormat::Lines {
                out["lines"] = Value::Array(
                    lines
                        .iter()
                        .filter(|l| !l.text.trim().is_empty())
                        .map(|l| {
                            json!({
                                "text": l.text.trim(),
                                "rect": rect_json(l.rect, ox, oy),
                                "confidence": (f64::from(l.confidence) * 100.0).round() / 100.0,
                            })
                        })
                        .collect(),
                );
            }
            Ok(out)
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_text_is_cut_and_flagged() {
        let (t, cut) = limit_text("a".repeat(MAX_TEXT_CHARS + 5));
        assert!(cut);
        assert_eq!(t.chars().count(), MAX_TEXT_CHARS);
        let (t, cut) = limit_text("short".to_owned());
        assert!(!cut);
        assert_eq!(t, "short");
        // Multi-byte characters are cut on a character boundary.
        let (t, cut) = limit_text("ế".repeat(MAX_TEXT_CHARS + 1));
        assert!(cut);
        assert_eq!(t.chars().count(), MAX_TEXT_CHARS);
    }
}
