//! `suggest_blueprint` and the 0.3.4 ops through `annotate` (ANN-8, ANN-9.AC4, ANN-11.AC4), driven
//! by a scripted recognizer (no real OCR engine).

#![allow(clippy::unwrap_used)]

mod common;

use base64::Engine as _;
use cm_core::CmError;
use cm_render::ocr::{TextLine, TextRecognizer};
use common::{builder, desktop, Client};
use image::RgbaImage;
use serde_json::{json, Value};

#[derive(Clone)]
struct Fixed(Vec<TextLine>, bool);

impl TextRecognizer for Fixed {
    fn name(&self) -> &'static str {
        "fixed"
    }
    fn is_available(&self) -> bool {
        self.1
    }
    fn recognize(&self, _image: &RgbaImage) -> Result<Vec<TextLine>, CmError> {
        Ok(self.0.clone())
    }
}

fn ok(r: &Value) -> &Value {
    assert!(r.get("error").is_none(), "unexpected error: {r}");
    &r["result"]
}

fn decode(v: &Value) -> RgbaImage {
    let png = base64::engine::general_purpose::STANDARD
        .decode(v["image"]["data"].as_str().unwrap())
        .unwrap();
    image::load_from_memory(&png).unwrap().to_rgba8()
}

fn lines() -> Vec<TextLine> {
    vec![
        TextLine::laid_out("second line", 120.0, 140.0, 8.0, 16.0),
        TextLine::laid_out("Hello from the screenshot", 120.0, 120.0, 8.0, 16.0),
    ]
}

async fn shot(c: &mut Client) -> String {
    let r = c
        .call("capture_screenshot", json!({"target": {"display_id": 1}}))
        .await;
    ok(&r)["capture_id"].as_str().unwrap().to_owned()
}

/// ANN-11.AC4: `suggest_blueprint` returns the boxes (reading order, with measured fill and
/// colour) and nothing is stored; an `annotate` with the `blueprint` op renders like a direct
/// render of the same op.
#[tokio::test]
async fn ann_11_ac4_suggest_then_annotate() {
    let (b, _dir) = builder(desktop());
    let d = b.text_recognizer(Fixed(lines(), true)).build();
    let mut c = Client::connect(&d, "agent");
    let id = shot(&mut c).await;
    let before = c.call("get_capture", json!({"capture_id": id})).await;

    let r = c.call("suggest_blueprint", json!({"capture_id": id})).await;
    let r = ok(&r);
    assert_eq!(r["recognizer"], "fixed");
    let boxes = r["boxes"].as_array().unwrap();
    assert_eq!(boxes.len(), 2);
    assert_eq!(boxes[0]["text_preview"], "Hello from the screenshot");
    assert!(boxes[0]["fill"]["colors"].is_array());
    assert!(boxes[0]["color"].as_str().unwrap().starts_with('#'));
    let after = c.call("get_capture", json!({"capture_id": id})).await;
    assert_eq!(
        ok(&before)["revision"],
        ok(&after)["revision"],
        "nothing is stored"
    );

    let op = json!({
        "op": "blueprint",
        "boxes": boxes.iter().map(|x| x["rect"].clone()).collect::<Vec<_>>(),
        "fills": boxes.iter().map(|x| x["fill"].clone()).collect::<Vec<_>>(),
        "colors": boxes.iter().map(|x| x["color"].clone()).collect::<Vec<_>>(),
    });
    let a = c
        .call("annotate", json!({"capture_id": id, "ops": [op.clone()]}))
        .await;
    let via_daemon = decode(ok(&a));
    // The same op rendered directly over the original.
    let cap = c.call("get_capture", json!({"capture_id": id})).await;
    let orig_path = std::path::Path::new(ok(&cap)["path"].as_str().unwrap()).join("original.png");
    let original = image::open(orig_path).unwrap().to_rgba8();
    let ops = cm_render::parse_ops(&json!([op])).unwrap();
    let direct = cm_render::render_annotations(&original, &ops).unwrap();
    assert_eq!(via_daemon, direct);
    assert_ne!(via_daemon, original, "the lines were replaced by bars");
}

#[tokio::test]
async fn suggest_blueprint_without_a_recognizer_is_not_implemented() {
    let (b, _dir) = builder(desktop());
    let d = b.text_recognizer(Fixed(vec![], false)).build();
    let mut c = Client::connect(&d, "agent");
    let id = shot(&mut c).await;
    let r = c.call("suggest_blueprint", json!({"capture_id": id})).await;
    assert_eq!(r["error"]["data"]["code"], "not_implemented");
}

/// ANN-9.AC4 and ANN-8.AC5 through the daemon: a base edit after another op, and a fifth lens,
/// are `invalid_input` naming `ops[i]`; nothing is stored.
#[tokio::test]
async fn base_edit_order_and_lens_limit_through_annotate() {
    let (b, _dir) = builder(desktop());
    let d = b.text_recognizer(Fixed(vec![], true)).build();
    let mut c = Client::connect(&d, "agent");
    let id = shot(&mut c).await;
    let retype = json!({"op":"retype","rect":[10,10,80,24],"fill":{"kind":"solid","colors":["#FFFFFF"]},"text":"x"});
    let arrow = json!({"op":"arrow","from":[1,1],"to":[50,50]});
    let r = c
        .call(
            "annotate",
            json!({"capture_id": id, "ops": [arrow, retype.clone()]}),
        )
        .await;
    assert_eq!(r["error"]["data"]["code"], "invalid_input", "{r}");
    assert!(
        r["error"]["message"].as_str().unwrap().contains("ops[1]"),
        "{r}"
    );
    // Appending a base edit to an image that already has an op is refused too (use replace).
    ok(&c
        .call("annotate", json!({"capture_id": id, "ops": [arrow]}))
        .await);
    let r = c
        .call(
            "annotate",
            json!({"capture_id": id, "ops": [retype.clone()]}),
        )
        .await;
    assert_eq!(r["error"]["data"]["code"], "invalid_input", "{r}");
    // The whole list with `replace` works, base edit first.
    let r = c
        .call(
            "annotate",
            json!({"capture_id": id, "ops": [retype, arrow], "replace": true}),
        )
        .await;
    ok(&r);
    let lens = json!({"op":"magnify","source":[10,10,40,40],"at":[200,200]});
    let five = vec![lens; 5];
    let r = c
        .call(
            "annotate",
            json!({"capture_id": id, "ops": five, "replace": true}),
        )
        .await;
    assert_eq!(r["error"]["data"]["code"], "invalid_input", "{r}");
    assert!(
        r["error"]["message"].as_str().unwrap().contains("ops[4]"),
        "{r}"
    );
}
