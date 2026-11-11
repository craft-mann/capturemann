//! Copy text from a screenshot in the editor, the library menu and the capture toast (ANN-5).
//! The pure parts only: when the entries are enabled, the request to the daemon, what the
//! result means and the words of the toast. No GPUI types here.
//!
//! The recognition itself is the daemon action `recognize_text`, which reads the rendered image
//! (so blurred text is never copied) and puts the text on the clipboard. Nothing is stored and
//! there is no undo step.

use cm_core::{Pt, Rc};
use cm_i18n::{tr, trn};
use serde_json::{json, Value};

/// A dragged selection smaller than this (image px, either side) is a click, not a selection.
pub const MIN_SELECTION: f64 = 6.0;

/// Whether this machine has a text recognizer (the same answer as Auto-redact's).
pub fn available() -> bool {
    super::redact::available()
}

/// The tooltip of a dimmed "Copy text" entry: what to install, and what each system reads.
/// `None` when the entry works or nothing can be installed (then the entry is hidden).
pub fn unavailable_hint() -> Option<&'static str> {
    static HINT: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    HINT.get_or_init(|| {
        if available() || cm_render::ocr::install_hint().is_none() {
            return None;
        }
        if cfg!(windows) {
            return Some(tr!(
                "Copy text needs a Windows text recognition language. Add one in Settings > Time & language > Language & region (it installs the text recognition part), then restart CaptureMann. It reads only the languages you install there."
            ));
        }
        Some(tr!(
            "Copy text needs the tesseract program. Install it (Debian and Ubuntu: sudo apt install tesseract-ocr), then restart CaptureMann. It reads English; other languages need their tesseract language data."
        ))
    })
    .as_deref()
}

/// The rectangle of a drag from `a` to `b`, clamped to the image, or `None` when it is too small
/// to be a selection.
pub fn selection_rect(a: Pt, b: Pt, image: (u32, u32)) -> Option<Rc> {
    let (w, h) = (f64::from(image.0), f64::from(image.1));
    let x0 = a.x.min(b.x).clamp(0.0, w);
    let x1 = a.x.max(b.x).clamp(0.0, w);
    let y0 = a.y.min(b.y).clamp(0.0, h);
    let y1 = a.y.max(b.y).clamp(0.0, h);
    (x1 - x0 >= MIN_SELECTION && y1 - y0 >= MIN_SELECTION)
        .then(|| Rc::new(x0, y0, x1 - x0, y1 - y0))
}

/// The `recognize_text` request: the whole image when `region` is `None`; always copies.
pub fn request(capture_id: &str, region: Option<Rc>) -> Value {
    let mut p = json!({ "capture_id": capture_id, "copy_to_clipboard": true });
    if let Some(r) = region {
        p["region"] = json!([r.x.floor(), r.y.floor(), r.w.ceil(), r.h.ceil()]);
    }
    p
}

/// Asks the daemon for the text of `capture_id` (or of `region`) and copies it. The recognizer
/// runs in the daemon call's blocking task, never on the UI thread; call this from the runtime.
pub async fn run(
    daemon: &cm_daemon::Daemon,
    capture_id: &str,
    region: Option<Rc>,
) -> Result<Outcome, String> {
    let v = crate::backend::Backend::call(daemon, "recognize_text", request(capture_id, region))
        .await?;
    parse_outcome(&v)
}

/// What a `recognize_text` call came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// This many words were found and are on the clipboard.
    Copied(usize),
    /// Words were found but the clipboard refused them (the text is not lost; try again).
    NotCopied(usize),
    /// Nothing readable in the image or region.
    NoText,
}

pub fn parse_outcome(v: &Value) -> Result<Outcome, String> {
    let text = v
        .get("text")
        .and_then(Value::as_str)
        .ok_or_else(|| "recognize_text returned no text".to_owned())?;
    let words = cm_render::ocr::word_count(text);
    if words == 0 {
        return Ok(Outcome::NoText);
    }
    let copied = v
        .get("copied_to_clipboard")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    Ok(if copied {
        Outcome::Copied(words)
    } else {
        Outcome::NotCopied(words)
    })
}

impl Outcome {
    /// The toast line.
    pub fn message(&self) -> String {
        match self {
            Outcome::Copied(n) => trn!("Copied {n} word", "Copied {n} words", *n),
            Outcome::NotCopied(_) => tr!("Found text, but could not copy it to the clipboard"),
            Outcome::NoText => tr!("No text found"),
        }
    }

    pub fn is_success(&self) -> bool {
        matches!(self, Outcome::Copied(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drag_is_clamped_to_the_image_and_tiny_drags_are_clicks() {
        let r = selection_rect(Pt::new(150.0, 90.0), Pt::new(-20.0, 10.0), (100, 80)).unwrap();
        assert_eq!((r.x, r.y, r.w, r.h), (0.0, 10.0, 100.0, 70.0));
        assert!(selection_rect(Pt::new(5.0, 5.0), Pt::new(8.0, 40.0), (100, 80)).is_none());
        assert!(selection_rect(Pt::new(200.0, 200.0), Pt::new(300.0, 300.0), (100, 80)).is_none());
    }

    #[test]
    fn the_request_always_copies_and_rounds_the_region_outwards() {
        let whole = request("shot_1", None);
        assert_eq!(whole["copy_to_clipboard"], true);
        assert!(whole.get("region").is_none());
        let part = request("shot_1", Some(Rc::new(10.4, 20.6, 30.2, 40.0)));
        assert_eq!(part["region"], json!([10.0, 20.0, 31.0, 40.0]));
    }

    #[test]
    fn outcomes_from_results() {
        let ok = json!({"text": "one two\nthree", "copied_to_clipboard": true});
        assert_eq!(parse_outcome(&ok).unwrap(), Outcome::Copied(3));
        let no_clip = json!({"text": "one", "copied_to_clipboard": false});
        assert_eq!(parse_outcome(&no_clip).unwrap(), Outcome::NotCopied(1));
        let none = json!({"text": "  ", "copied_to_clipboard": false});
        assert_eq!(parse_outcome(&none).unwrap(), Outcome::NoText);
        assert!(parse_outcome(&json!({})).is_err());
    }

    #[test]
    fn toast_words() {
        cm_i18n::with_language(cm_i18n::Lang::En, || {
            assert_eq!(Outcome::Copied(1).message(), "Copied 1 word");
            assert_eq!(Outcome::Copied(12).message(), "Copied 12 words");
            assert_eq!(Outcome::NoText.message(), "No text found");
            assert!(Outcome::Copied(2).is_success() && !Outcome::NoText.is_success());
        });
    }
}
