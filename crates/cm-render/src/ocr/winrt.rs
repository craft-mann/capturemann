//! Text recognition with `Windows.Media.Ocr` (Windows 10 and 11, part of the OS: no extra program,
//! no model download, no network). Compiled on Windows only; the pure parts (picture preparation,
//! word boxes to lines, language sentences) are in `winocr` and tested everywhere.
//!
//! The engine is made from the user's language list (`TryCreateFromUserProfileLanguages`), so it
//! reads the languages the person has added in Windows that also have the OCR capability
//! installed. `AvailableRecognizerLanguages` lists what is installed; both go to diagnostics.
//! With no match the recognizer reports itself unavailable and `system_recognizer` falls back to
//! `tesseract` when that is installed.
//!
//! Status: written against the WinRT API and compiled for Windows; the real-engine run is the
//! manual step in `tests/manual/ANN-2.md` on a Windows machine (see the report in `verification/`
//! once it exists).

use super::winocr::{self, RawLine, RawWord};
use super::{TextLine, TextRecognizer};
use cm_core::{CmError, Rc};
use image::RgbaImage;
use windows::core::Result as WinResult;
use windows::Globalization::Language;
use windows::Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap};
use windows::Media::Ocr::OcrEngine;
use windows::Storage::Streams::DataWriter;

/// The Windows recognizer.
#[derive(Debug, Clone, Copy, Default)]
pub struct WindowsOcrRecognizer;

fn tag(l: &Language) -> String {
    l.LanguageTag().map(|t| t.to_string()).unwrap_or_default()
}

/// The OCR languages installed on this machine (BCP-47 tags), empty when the query fails.
pub fn installed_languages() -> Vec<String> {
    OcrEngine::AvailableRecognizerLanguages()
        .map(|v| v.into_iter().map(|l| tag(&l)).collect())
        .unwrap_or_default()
}

/// The engine for the user's language list, `None` when no language of it has OCR installed.
fn engine() -> Option<OcrEngine> {
    OcrEngine::TryCreateFromUserProfileLanguages().ok()
}

/// The tag of the language the engine would read, for diagnostics.
fn engine_language() -> Option<String> {
    engine()
        .and_then(|e| e.RecognizerLanguage().ok())
        .map(|l| tag(&l))
}

fn run(image: &RgbaImage) -> WinResult<(Vec<RawLine>, f64)> {
    let engine = OcrEngine::TryCreateFromUserProfileLanguages()?;
    let max_dim = OcrEngine::MaxImageDimension().unwrap_or(10_000);
    let p = winocr::prepare(image, max_dim);
    let writer = DataWriter::new()?;
    writer.WriteBytes(&p.bgra)?;
    let buffer = writer.DetachBuffer()?;
    let bitmap = SoftwareBitmap::CreateCopyFromBuffer(
        &buffer,
        BitmapPixelFormat::Bgra8,
        p.width as i32,
        p.height as i32,
    )?;
    let result = engine.RecognizeAsync(&bitmap)?.get()?;
    let mut lines = Vec::new();
    for line in result.Lines()? {
        let mut words = Vec::new();
        for w in line.Words()? {
            let r = w.BoundingRect()?;
            words.push(RawWord {
                text: w.Text()?.to_string(),
                rect: Rc::new(
                    f64::from(r.X),
                    f64::from(r.Y),
                    f64::from(r.Width),
                    f64::from(r.Height),
                ),
            });
        }
        lines.push(RawLine { words });
    }
    Ok((lines, p.scale))
}

impl TextRecognizer for WindowsOcrRecognizer {
    fn name(&self) -> &'static str {
        "windows_ocr"
    }

    fn is_available(&self) -> bool {
        engine().is_some()
    }

    fn install_hint(&self) -> Option<String> {
        engine()
            .is_none()
            .then(|| winocr::install_hint(&installed_languages()))
    }

    fn detail(&self) -> Option<String> {
        Some(winocr::describe_languages(
            engine_language().as_deref(),
            &installed_languages(),
        ))
    }

    fn recognize(&self, image: &RgbaImage) -> Result<Vec<TextLine>, CmError> {
        if image.width() == 0 || image.height() == 0 {
            return Err(CmError::invalid_input("the picture is empty"));
        }
        if engine().is_none() {
            return Err(CmError::not_implemented(winocr::install_hint(
                &installed_languages(),
            )));
        }
        // A fresh thread: the WinRT calls need an apartment, a new thread gets the implicit
        // multi-threaded one, and blocking on the async result is safe there (the caller may be
        // a single-threaded UI thread).
        let done = std::thread::scope(|s| {
            s.spawn(|| run(image))
                .join()
                .map_err(|_| CmError::internal("the Windows text recognizer panicked"))
        })?;
        let (raw, scale) =
            done.map_err(|e| CmError::internal(format!("Windows text recognition failed: {e}")))?;
        Ok(winocr::lines_from_raw(&raw, scale))
    }
}
