//! On-device text recognition for Auto-redact (ANN-2): the [`TextRecognizer`] trait, the
//! platform recognizers and a scripted fake for tests.
//!
//! * macOS: Apple Vision (`VNRecognizeTextRequest`, accurate level) in `apple`, the only
//!   place with `unsafe` code.
//! * Windows: `Windows.Media.Ocr` is a later milestone; [`system_recognizer`] returns a stub
//!   that reports itself unavailable and answers `not_implemented`.
//! * Anything else: the same stub.
//!
//! Recognition never leaves the machine and never needs a network or an AI key. When no
//! recognizer is available the editor hides its Auto-redact button and `suggest_redactions`
//! answers `not_implemented`.

use cm_core::{CmError, Rc};
use image::RgbaImage;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[cfg(target_os = "macos")]
mod apple;
pub mod detect;

pub use detect::{
    mask_preview, suggest_redactions, RedactionKind, RedactionSuggestion, SuggestOptions,
};

/// One recognized word: whitespace-delimited text and its box in **original-image pixels**
/// (top-left origin).
#[derive(Debug, Clone, PartialEq)]
pub struct TextWord {
    pub text: String,
    pub rect: Rc,
}

/// One recognized line of text with its box (original-image pixels, top-left origin) and
/// the recognizer's confidence in 0.0..=1.0. `words` may be empty when the recognizer gives
/// no word boxes; the line box is then used for the whole line.
#[derive(Debug, Clone, PartialEq)]
pub struct TextLine {
    pub text: String,
    pub rect: Rc,
    pub confidence: f32,
    pub words: Vec<TextWord>,
}

/// Recognizes text in an image. Implementations are blocking and may take a second or more on
/// a large screenshot: callers run them off the UI thread.
pub trait TextRecognizer: Send + Sync {
    /// Short stable name for results and logs, for example `"apple_vision"`.
    fn name(&self) -> &'static str;

    /// Whether [`TextRecognizer::recognize`] can work on this machine right now.
    fn is_available(&self) -> bool;

    /// Lines of text in reading order with boxes in the image's own pixels.
    ///
    /// # Errors
    /// `not_implemented` when unavailable, `internal` when the platform request fails.
    fn recognize(&self, image: &RgbaImage) -> Result<Vec<TextLine>, CmError>;
}

/// The recognizer of this OS: Apple Vision on macOS, an unavailable stub elsewhere.
pub fn system_recognizer() -> Arc<dyn TextRecognizer> {
    #[cfg(target_os = "macos")]
    {
        Arc::new(apple::VisionRecognizer)
    }
    #[cfg(not(target_os = "macos"))]
    {
        Arc::new(Unavailable)
    }
}

/// True when the OS recognizer can run (the editor shows Auto-redact only then).
pub fn system_available() -> bool {
    system_recognizer().is_available()
}

/// The stub for platforms without a recognizer yet (Windows until Windows.Media.Ocr lands).
#[derive(Debug, Clone, Copy, Default)]
pub struct Unavailable;

impl TextRecognizer for Unavailable {
    fn name(&self) -> &'static str {
        "none"
    }

    fn is_available(&self) -> bool {
        false
    }

    fn recognize(&self, _image: &RgbaImage) -> Result<Vec<TextLine>, CmError> {
        Err(CmError::not_implemented(
            "text recognition is not available on this platform yet, so redactions cannot be suggested",
        ))
    }
}

/// A scripted recognizer for tests: returns the lines it was given (or an error), and counts
/// its calls.
#[derive(Debug, Clone, Default)]
pub struct ScriptedRecognizer {
    lines: Vec<TextLine>,
    available: bool,
    fail: Option<String>,
    calls: Arc<AtomicUsize>,
}

impl ScriptedRecognizer {
    /// An available recognizer that answers `lines` for every image.
    pub fn new(lines: Vec<TextLine>) -> Self {
        ScriptedRecognizer {
            lines,
            available: true,
            fail: None,
            calls: Arc::default(),
        }
    }

    /// A recognizer that reports itself unavailable.
    pub fn unavailable() -> Self {
        ScriptedRecognizer::default()
    }

    /// An available recognizer whose requests fail with this message.
    pub fn failing(message: &str) -> Self {
        ScriptedRecognizer {
            available: true,
            fail: Some(message.to_owned()),
            ..Self::default()
        }
    }

    /// How many times `recognize` ran.
    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl TextRecognizer for ScriptedRecognizer {
    fn name(&self) -> &'static str {
        "scripted"
    }

    fn is_available(&self) -> bool {
        self.available
    }

    fn recognize(&self, _image: &RgbaImage) -> Result<Vec<TextLine>, CmError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if !self.available {
            return Err(CmError::not_implemented(
                "text recognition is not available",
            ));
        }
        if let Some(m) = &self.fail {
            return Err(CmError::internal(m.clone()));
        }
        Ok(self.lines.clone())
    }
}

impl TextLine {
    /// A line of single-space-separated words laid out left to right at `(x, y)` with `char_w`
    /// px per character and `h` px height (a helper for fixtures).
    pub fn laid_out(text: &str, x: f64, y: f64, char_w: f64, h: f64) -> TextLine {
        let mut words = Vec::new();
        let mut cx = x;
        for (i, w) in text.split(' ').enumerate() {
            if i > 0 {
                cx += char_w;
            }
            let wpx = w.chars().count() as f64 * char_w;
            if !w.is_empty() {
                words.push(TextWord {
                    text: w.to_owned(),
                    rect: Rc::new(cx, y, wpx, h),
                });
            }
            cx += wpx;
        }
        TextLine {
            text: text.to_owned(),
            rect: Rc::new(x, y, cx - x, h),
            confidence: 1.0,
            words,
        }
    }
}
