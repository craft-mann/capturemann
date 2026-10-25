//! Apple Vision text recognition (`VNRecognizeTextRequest`, accurate level) through plain
//! `objc2` messages. This is the only module of `cm-render` with `unsafe` code; every block
//! has a SAFETY comment.
//!
//! Vision is loaded with `dlopen` at run time instead of being linked, so a Mac without the
//! framework (or a sandbox that hides it) simply reports the recognizer as unavailable. The
//! calls follow the documented API: a request object, a `VNImageRequestHandler` made from PNG
//! data, `performRequests:error:`, then `VNRecognizedTextObservation`s whose top candidate
//! gives the string, a confidence and a box for any character range.
//!
//! Status: written against the Vision headers and exercised by an ignored test on a
//! synthetic PNG (`tests/ocr_real.rs`); not run in CI.

use super::{TextLine, TextRecognizer, TextWord};
use cm_core::{CmError, Rc};
use image::RgbaImage;
use objc2::msg_send;
use objc2::rc::{autoreleasepool, Allocated, Retained};
use objc2::runtime::{AnyClass, AnyObject};
use objc2_foundation::{NSArray, NSData, NSDictionary, NSError, NSRange, NSRect, NSString};
use std::ffi::{c_char, c_int, c_void, CStr};
use std::sync::OnceLock;

extern "C" {
    fn dlopen(path: *const c_char, mode: c_int) -> *mut c_void;
}

const RTLD_LAZY: c_int = 0x1;
const VISION_PATH: &CStr = c"/System/Library/Frameworks/Vision.framework/Vision";
/// `VNRequestTextRecognitionLevelAccurate`.
const LEVEL_ACCURATE: isize = 0;

/// Loads Vision once; true when the framework and the request class are there.
fn vision_loaded() -> bool {
    static LOADED: OnceLock<bool> = OnceLock::new();
    *LOADED.get_or_init(|| {
        // SAFETY: `dlopen` is called with a valid NUL-terminated path and a valid mode. The
        // handle is deliberately never closed (the framework stays loaded for the process).
        let handle = unsafe { dlopen(VISION_PATH.as_ptr(), RTLD_LAZY) };
        !handle.is_null()
            && AnyClass::get(c"VNRecognizeTextRequest").is_some()
            && AnyClass::get(c"VNImageRequestHandler").is_some()
    })
}

/// The macOS recognizer.
#[derive(Debug, Clone, Copy, Default)]
pub struct VisionRecognizer;

impl TextRecognizer for VisionRecognizer {
    fn name(&self) -> &'static str {
        "apple_vision"
    }

    fn is_available(&self) -> bool {
        vision_loaded()
    }

    fn recognize(&self, image: &RgbaImage) -> Result<Vec<TextLine>, CmError> {
        if !vision_loaded() {
            return Err(CmError::not_implemented(
                "Apple Vision is not available on this Mac, so text cannot be recognized",
            ));
        }
        let png = crate::encode_png(image)?;
        let (w, h) = (f64::from(image.width()), f64::from(image.height()));
        autoreleasepool(|_| run_request(&png, w, h))
    }
}

fn nserror_text(e: &NSError) -> String {
    e.localizedDescription().to_string()
}

fn run_request(png: &[u8], width: f64, height: f64) -> Result<Vec<TextLine>, CmError> {
    let request_class = AnyClass::get(c"VNRecognizeTextRequest")
        .ok_or_else(|| CmError::not_implemented("VNRecognizeTextRequest is missing"))?;
    let handler_class = AnyClass::get(c"VNImageRequestHandler")
        .ok_or_else(|| CmError::not_implemented("VNImageRequestHandler is missing"))?;
    let data = NSData::with_bytes(png);
    let options: Retained<NSDictionary<NSString, AnyObject>> = NSDictionary::new();

    // SAFETY: `new` on a Vision request class returns a retained, initialized object. The
    // setters take an NSInteger and a BOOL, which is what is passed. Language correction is
    // switched off on purpose: it would "fix" emails and keys into dictionary words.
    let request: Retained<AnyObject> = unsafe {
        let r: Retained<AnyObject> = msg_send![request_class, new];
        let _: () = msg_send![&*r, setRecognitionLevel: LEVEL_ACCURATE];
        let _: () = msg_send![&*r, setUsesLanguageCorrection: false];
        r
    };

    // SAFETY: `alloc` + `initWithData:options:` is the documented initializer; `data` and
    // `options` are valid Foundation objects that outlive the call.
    let handler: Retained<AnyObject> = unsafe {
        let a: Allocated<AnyObject> = msg_send![handler_class, alloc];
        let h: Option<Retained<AnyObject>> = msg_send![a, initWithData: &*data, options: &*options];
        h.ok_or_else(|| CmError::internal("Vision could not read the image data"))?
    };

    let requests = NSArray::from_slice(&[&*request]);
    // SAFETY: `performRequests:error:` takes an NSArray of requests and an error out-pointer,
    // which the `error: _` form of `msg_send!` supplies and turns into a `Result`.
    let performed: Result<(), Retained<NSError>> =
        unsafe { msg_send![&*handler, performRequests: &*requests, error: _] };
    performed.map_err(|e| {
        CmError::internal(format!(
            "Vision text recognition failed: {}",
            nserror_text(&e)
        ))
    })?;

    // SAFETY: `results` is an NSArray of VNRecognizedTextObservation (or nil if none).
    let results: Option<Retained<NSArray<AnyObject>>> = unsafe { msg_send![&*request, results] };
    let Some(results) = results else {
        return Ok(Vec::new());
    };
    let count = results.len();
    let mut lines = Vec::with_capacity(count);
    for i in 0..count {
        let obs = results.objectAtIndex(i);
        if let Some(line) = read_observation(&obs, width, height) {
            lines.push(line);
        }
    }
    Ok(lines)
}

/// Vision's normalized box (origin bottom-left) to image pixels (origin top-left).
fn to_px(b: NSRect, width: f64, height: f64) -> Rc {
    Rc::new(
        b.origin.x * width,
        (1.0 - b.origin.y - b.size.height) * height,
        b.size.width * width,
        b.size.height * height,
    )
}

fn read_observation(obs: &AnyObject, width: f64, height: f64) -> Option<TextLine> {
    // SAFETY: `topCandidates:` takes an NSUInteger and returns an NSArray of VNRecognizedText.
    let cands: Retained<NSArray<AnyObject>> = unsafe { msg_send![obs, topCandidates: 1usize] };
    if cands.is_empty() {
        return None;
    }
    let cand = cands.objectAtIndex(0);
    // SAFETY: `string` returns an NSString, `confidence` a float (VNConfidence), and
    // `boundingBox` a CGRect, on VNRecognizedText / VNRectangleObservation respectively.
    let (text, confidence, bbox): (Retained<NSString>, f32, NSRect) = unsafe {
        (
            msg_send![&*cand, string],
            msg_send![&*cand, confidence],
            msg_send![obs, boundingBox],
        )
    };
    let text = text.to_string();
    let words = word_boxes(&cand, &text, width, height);
    Some(TextLine {
        text,
        rect: to_px(bbox, width, height),
        confidence,
        words,
    })
}

/// One box per whitespace-separated word, asked from Vision by UTF-16 range.
fn word_boxes(cand: &AnyObject, text: &str, width: f64, height: f64) -> Vec<TextWord> {
    let mut words = Vec::new();
    let mut utf16 = 0usize;
    let mut cur: Option<(usize, usize, String)> = None; // start, len, text
    let flush = |cur: &mut Option<(usize, usize, String)>, words: &mut Vec<TextWord>| {
        if let Some((start, len, t)) = cur.take() {
            if let Some(rect) = range_box(cand, start, len, width, height) {
                words.push(TextWord { text: t, rect });
            }
        }
    };
    for ch in text.chars() {
        let n = ch.len_utf16();
        if ch.is_whitespace() {
            flush(&mut cur, &mut words);
        } else {
            match &mut cur {
                Some((_, len, t)) => {
                    *len += n;
                    t.push(ch);
                }
                None => cur = Some((utf16, n, ch.to_string())),
            }
        }
        utf16 += n;
    }
    flush(&mut cur, &mut words);
    words
}

fn range_box(cand: &AnyObject, start: usize, len: usize, width: f64, height: f64) -> Option<Rc> {
    let range = NSRange::new(start, len);
    // SAFETY: `boundingBoxForRange:error:` takes an NSRange inside the string (the range is
    // built from the string's own UTF-16 offsets) and returns a VNRectangleObservation, or
    // nil with an error, which `error: _` turns into `Err`.
    let obs: Result<Retained<AnyObject>, Retained<NSError>> =
        unsafe { msg_send![cand, boundingBoxForRange: range, error: _] };
    let obs = obs.ok()?;
    // SAFETY: a VNRectangleObservation has `boundingBox` returning a CGRect.
    let b: NSRect = unsafe { msg_send![&*obs, boundingBox] };
    Some(to_px(b, width, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vision_box_maps_to_top_left_pixels() {
        // A box in the top-left corner of Vision's space (y up) lands at y = 0.
        let b = NSRect::new(
            objc2_foundation::NSPoint::new(0.1, 0.9),
            objc2_foundation::NSSize::new(0.2, 0.1),
        );
        let r = to_px(b, 1000.0, 500.0);
        assert!((r.x - 100.0).abs() < 1e-9 && r.y.abs() < 1e-9);
        assert!((r.w - 200.0).abs() < 1e-9 && (r.h - 50.0).abs() < 1e-9);
    }
}
