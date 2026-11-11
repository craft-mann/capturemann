//! `recognize_text` (ANN-5): Free, read-only, reads the RENDERED image, driven by a scripted
//! recognizer (no real OCR engine, no real clipboard).

#![allow(clippy::unwrap_used)]

mod common;

use base64::Engine as _;
use cm_core::CmError;
use cm_daemon::testing::RecordingClipboard;
use cm_render::ocr::{TextLine, TextRecognizer};
use common::{builder, desktop, Client};
use image::RgbaImage;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

/// Answers `lines` for every image and keeps the images it was given.
#[derive(Clone, Default)]
struct Spy {
    lines: Vec<TextLine>,
    hint: Option<String>,
    unavailable: bool,
    seen: Arc<Mutex<Vec<RgbaImage>>>,
}

impl Spy {
    fn saying(lines: Vec<TextLine>) -> Spy {
        Spy {
            lines,
            ..Spy::default()
        }
    }
    fn last(&self) -> RgbaImage {
        self.seen.lock().unwrap().last().cloned().unwrap()
    }
    fn calls(&self) -> usize {
        self.seen.lock().unwrap().len()
    }
}

impl TextRecognizer for Spy {
    fn name(&self) -> &'static str {
        "spy"
    }
    fn is_available(&self) -> bool {
        !self.unavailable
    }
    fn install_hint(&self) -> Option<String> {
        self.hint.clone()
    }
    fn recognize(&self, image: &RgbaImage) -> Result<Vec<TextLine>, CmError> {
        self.seen.lock().unwrap().push(image.clone());
        Ok(self.lines.clone())
    }
}

fn ok(r: &Value) -> &Value {
    assert!(r.get("error").is_none(), "unexpected error: {r}");
    &r["result"]
}

fn code(r: &Value) -> &str {
    r["error"]["data"]["code"].as_str().unwrap()
}

fn two_lines() -> Vec<TextLine> {
    vec![
        TextLine::laid_out("Hello from the screenshot", 120.0, 120.0, 8.0, 16.0),
        TextLine::laid_out("second line", 120.0, 140.0, 8.0, 16.0),
    ]
}

async fn shot(c: &mut Client) -> String {
    let r = c
        .call("capture_screenshot", json!({"target": {"display_id": 1}}))
        .await;
    ok(&r)["capture_id"].as_str().unwrap().to_owned()
}

fn decode(v: &Value) -> RgbaImage {
    let png = base64::engine::general_purpose::STANDARD
        .decode(v["image"]["data"].as_str().unwrap())
        .unwrap();
    image::load_from_memory(&png).unwrap().to_rgba8()
}

const PIXELATE: &str = r##"[{"op":"pixelate","rect":[100,100,300,200],"block":32}]"##;

#[tokio::test]
async fn ann_5_text_comes_back_in_reading_order_and_is_copied_on_request() {
    let spy = Spy::saying(two_lines());
    let clip = Arc::new(RecordingClipboard::default());
    let (b, _dir) = builder(desktop());
    let d = b
        .text_recognizer(spy.clone())
        .clipboard(clip.clone())
        .build();
    let mut c = Client::connect(&d, "agent");
    let id = shot(&mut c).await;
    let before = c.call("get_capture", json!({"capture_id": id})).await;

    let r = c.call("recognize_text", json!({"capture_id": id})).await;
    let r = ok(&r);
    assert_eq!(r["text"], "Hello from the screenshot\nsecond line");
    assert_eq!(r["word_count"], 6);
    assert_eq!(r["recognizer"], "spy");
    assert_eq!(r["truncated"], false);
    assert_eq!(r["copied_to_clipboard"], false);
    assert!(r.get("lines").is_none(), "format text has no lines");
    assert!(clip.last_text().is_none(), "nothing is copied unless asked");

    let r = c
        .call(
            "recognize_text",
            json!({"capture_id": id, "copy_to_clipboard": true, "format": "lines"}),
        )
        .await;
    let r = ok(&r);
    assert_eq!(r["copied_to_clipboard"], true);
    assert_eq!(
        clip.last_text().unwrap(),
        "Hello from the screenshot\nsecond line"
    );
    let lines = r["lines"].as_array().unwrap();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0]["text"], "Hello from the screenshot");
    assert_eq!(lines[0]["rect"][0], 120.0);
    assert_eq!(lines[0]["confidence"], 1.0);
    assert!(
        clip.last_png().is_none(),
        "a text copy never touches the image"
    );

    // Read-only: same ops and revision, no new revision.
    let after = c.call("get_capture", json!({"capture_id": id})).await;
    assert_eq!(ok(&after)["revision"], ok(&before)["revision"]);
    assert_eq!(ok(&after)["ops"], ok(&before)["ops"]);
}

/// RED-2.AC1 for copied text: the recognizer is fed the rendered image, so a pixelated region
/// is not the original's pixels.
#[tokio::test]
async fn ann_5_reads_the_rendered_image_so_a_pixelated_region_is_not_the_original() {
    let spy = Spy::saying(two_lines());
    let (b, _dir) = builder(desktop());
    let d = b.text_recognizer(spy.clone()).build();
    let mut c = Client::connect(&d, "agent");
    let id = shot(&mut c).await;

    ok(&c.call("recognize_text", json!({"capture_id": id})).await);
    let original = spy.last();

    let ops: Value = serde_json::from_str(PIXELATE).unwrap();
    let a = c
        .call(
            "annotate",
            json!({"capture_id": id, "ops": ops, "return_image": true}),
        )
        .await;
    let rendered = decode(ok(&a));
    ok(&c.call("recognize_text", json!({"capture_id": id})).await);
    let seen = spy.last();

    assert_eq!(seen.dimensions(), original.dimensions());
    assert_eq!(
        seen, rendered,
        "the recognizer got exactly the rendered image"
    );
    assert_ne!(seen, original);
    let inside = (110..290u32)
        .flat_map(|x| (110..290u32).map(move |y| (x, y)))
        .filter(|&(x, y)| x < seen.width() && y < seen.height())
        .any(|(x, y)| seen.get_pixel(x, y) != original.get_pixel(x, y));
    assert!(inside, "the pixelated region differs from the original");
    assert_eq!(
        seen.get_pixel(5, 5),
        original.get_pixel(5, 5),
        "outside the region is untouched"
    );
}

#[tokio::test]
async fn ann_5_a_stale_render_is_not_used_after_undo() {
    let spy = Spy::saying(two_lines());
    let (b, _dir) = builder(desktop());
    let d = b.text_recognizer(spy.clone()).build();
    let mut c = Client::connect(&d, "agent");
    let id = shot(&mut c).await;
    ok(&c.call("recognize_text", json!({"capture_id": id})).await);
    let original = spy.last();
    let ops: Value = serde_json::from_str(PIXELATE).unwrap();
    ok(&c
        .call("annotate", json!({"capture_id": id, "ops": ops}))
        .await);
    ok(&c.call("undo", json!({"capture_id": id})).await);
    ok(&c.call("recognize_text", json!({"capture_id": id})).await);
    assert_eq!(spy.last(), original);
}

#[tokio::test]
async fn ann_5_region_is_cropped_and_a_crop_op_shifts_the_numbers() {
    let spy = Spy::saying(vec![TextLine::laid_out("inside", 20.0, 20.0, 8.0, 16.0)]);
    let (b, _dir) = builder(desktop());
    let d = b.text_recognizer(spy.clone()).build();
    let mut c = Client::connect(&d, "agent");
    let id = shot(&mut c).await;

    let r = c
        .call(
            "recognize_text",
            json!({"capture_id": id, "region": [100, 50, 200, 100], "format": "lines"}),
        )
        .await;
    let r = ok(&r);
    assert_eq!(r["region"], json!([100.0, 50.0, 200.0, 100.0]));
    // 200x100 plus an 8 px margin each side; the box is mapped back to original pixels.
    assert_eq!(spy.last().dimensions(), (216, 116));
    assert_eq!(r["lines"][0]["rect"][0], 112.0);
    assert_eq!(r["lines"][0]["rect"][1], 62.0);
    assert_eq!(r["text"], "inside");

    // After a crop op the image is smaller but the caller's numbers stay original pixels.
    let ops = json!([{"op": "crop", "rect": [100, 50, 200, 150]}]);
    ok(&c
        .call("annotate", json!({"capture_id": id, "ops": ops}))
        .await);
    let r = c
        .call(
            "recognize_text",
            json!({"capture_id": id, "region": [110, 60, 50, 50], "format": "lines"}),
        )
        .await;
    let r = ok(&r);
    assert_eq!(r["region"], json!([110.0, 60.0, 50.0, 50.0]));
    // Crop-local region (10, 10, 50, 50) plus margin: a 66x66 image at crop-local (2, 2).
    assert_eq!(spy.last().dimensions(), (66, 66));
    assert_eq!(r["lines"][0]["rect"][0], 122.0);
    assert_eq!(r["lines"][0]["rect"][1], 72.0);
}

#[tokio::test]
async fn ann_5_bad_regions_and_params_are_invalid_input() {
    let spy = Spy::saying(two_lines());
    let (b, _dir) = builder(desktop());
    let d = b.text_recognizer(spy.clone()).build();
    let mut c = Client::connect(&d, "agent");
    let id = shot(&mut c).await;
    for p in [
        json!({"capture_id": id, "region": [0, 0, 0, 10]}),
        json!({"capture_id": id, "region": [5000, 5000, 10, 10]}),
        json!({"capture_id": id, "region": [1, 2, 3]}),
        json!({"capture_id": id, "region": "all"}),
        json!({"capture_id": id, "format": "html"}),
        json!({"capture_id": id, "copy_to_clipboard": "yes"}),
        json!({"capture_id": id, "extra": 1}),
        json!({}),
    ] {
        let r = c.call("recognize_text", p.clone()).await;
        assert_eq!(code(&r), "invalid_input", "{p}: {r}");
    }
    assert_eq!(spy.calls(), 0, "refused before any recognition");
    let r = c
        .call("recognize_text", json!({"capture_id": "shot_nope"}))
        .await;
    assert_eq!(code(&r), "not_found");
}

#[tokio::test]
async fn ann_5_without_a_recognizer_it_says_not_implemented_with_the_install_hint() {
    let spy = Spy {
        unavailable: true,
        hint: Some("install tesseract".to_owned()),
        ..Spy::default()
    };
    let (b, _dir) = builder(desktop());
    let d = b.text_recognizer(spy.clone()).build();
    let mut c = Client::connect(&d, "agent");
    let id = shot(&mut c).await;
    let r = c.call("recognize_text", json!({"capture_id": id})).await;
    assert_eq!(code(&r), "not_implemented");
    let msg = r["error"]["message"].as_str().unwrap();
    assert!(msg.contains("install tesseract"), "{msg}");
    assert_eq!(spy.calls(), 0);
}

#[tokio::test]
async fn ann_5_no_text_found_is_an_empty_result_not_an_error() {
    let spy = Spy::saying(vec![]);
    let clip = Arc::new(RecordingClipboard::default());
    let (b, _dir) = builder(desktop());
    let d = b.text_recognizer(spy).clipboard(clip.clone()).build();
    let mut c = Client::connect(&d, "agent");
    let id = shot(&mut c).await;
    let r = c
        .call(
            "recognize_text",
            json!({"capture_id": id, "copy_to_clipboard": true}),
        )
        .await;
    let r = ok(&r);
    assert_eq!(r["text"], "");
    assert_eq!(r["word_count"], 0);
    assert_eq!(r["copied_to_clipboard"], false);
    assert!(
        clip.last_text().is_none(),
        "an empty result does not clear the clipboard"
    );
}
