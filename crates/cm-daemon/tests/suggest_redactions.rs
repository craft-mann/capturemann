//! `suggest_redactions` (ANN-2.AC4): read-only, Free, driven by a scripted text recognizer.

#![allow(clippy::unwrap_used)]

mod common;

use cm_render::ocr::{ScriptedRecognizer, TextLine};
use common::{builder, desktop, Client};
use serde_json::{json, Value};

fn ok(r: &Value) -> &Value {
    assert!(r.get("error").is_none(), "unexpected error: {r}");
    &r["result"]
}

fn code(r: &Value) -> &str {
    r["error"]["data"]["code"].as_str().unwrap()
}

fn two_emails_and_a_key() -> Vec<TextLine> {
    vec![
        TextLine::laid_out("Owner: jane.doe@example.com", 50.0, 60.0, 10.0, 20.0),
        TextLine::laid_out("Billing: ap@vendor.example.org", 50.0, 140.0, 10.0, 20.0),
        TextLine::laid_out(
            "key sk-proj-A1b2C3d4E5f6G7h8I9j0K1l2",
            50.0,
            220.0,
            10.0,
            20.0,
        ),
    ]
}

async fn shot(c: &mut Client) -> String {
    let r = c
        .call("capture_screenshot", json!({"target": {"display_id": 1}}))
        .await;
    ok(&r)["capture_id"].as_str().unwrap().to_owned()
}

/// ANN-2.AC4: two emails give two blur rects and nothing is applied.
#[tokio::test]
async fn ann_2_ac4_two_emails_are_proposed_and_nothing_is_applied() {
    let (b, _dir) = builder(desktop());
    let rec = ScriptedRecognizer::new(vec![
        TextLine::laid_out("Owner: jane.doe@example.com", 50.0, 60.0, 10.0, 20.0),
        TextLine::laid_out("Billing: ap@vendor.example.org", 50.0, 140.0, 10.0, 20.0),
    ]);
    let d = b.text_recognizer(rec.clone()).build();
    let mut c = Client::connect(&d, "agent");
    let id = shot(&mut c).await;
    let before = c.call("get_capture", json!({"capture_id": id})).await;

    let r = c
        .call("suggest_redactions", json!({"capture_id": id}))
        .await;
    let r = ok(&r);
    let rects = r["rects"].as_array().unwrap();
    assert_eq!(rects.len(), 2, "{r}");
    assert!(rects.iter().all(|x| x["kind"] == "email"));
    assert_eq!(r["applied"], false);
    assert_eq!(rec.calls(), 1);
    // Rectangles are [x, y, w, h] in original px and lie inside the image.
    let [iw, ih] = [
        r["image_size"][0].as_f64().unwrap(),
        r["image_size"][1].as_f64().unwrap(),
    ];
    for x in rects {
        let a = x["rect"].as_array().unwrap();
        let f = |i: usize| a[i].as_f64().unwrap();
        assert!(f(0) >= 0.0 && f(1) >= 0.0 && f(0) + f(2) <= iw && f(1) + f(3) <= ih);
    }

    // Nothing applied: same ops, same revision as before.
    let after = c.call("get_capture", json!({"capture_id": id})).await;
    assert_eq!(ok(&after)["ops"], ok(&before)["ops"]);
    assert_eq!(ok(&after)["revision"], ok(&before)["revision"]);
    assert_eq!(ok(&after)["ops"], json!([]));
}

#[tokio::test]
async fn previews_are_masked_and_kinds_filter_works() {
    let (b, _dir) = builder(desktop());
    let d = b
        .text_recognizer(ScriptedRecognizer::new(two_emails_and_a_key()))
        .build();
    let mut c = Client::connect(&d, "agent");
    let id = shot(&mut c).await;
    let all = c
        .call("suggest_redactions", json!({"capture_id": id}))
        .await;
    let text = serde_json::to_string(ok(&all)).unwrap();
    assert_eq!(ok(&all)["rects"].as_array().unwrap().len(), 3);
    for secret in ["jane.doe", "example.com", "vendor", "proj-A1b2", "G7h8I9j0"] {
        assert!(!text.contains(secret), "{secret} leaked: {text}");
    }
    assert!(text.contains("ja...om"), "{text}");

    let only = c
        .call(
            "suggest_redactions",
            json!({"capture_id": id, "kinds": ["token"]}),
        )
        .await;
    let rects = ok(&only)["rects"].as_array().unwrap();
    assert_eq!(rects.len(), 1);
    assert_eq!(rects[0]["kind"], "token");
}

#[tokio::test]
async fn accepting_is_an_ordinary_annotate_with_blur_ops_in_one_undo_step() {
    let (b, _dir) = builder(desktop());
    let d = b
        .text_recognizer(ScriptedRecognizer::new(two_emails_and_a_key()))
        .build();
    let mut c = Client::connect(&d, "agent");
    let id = shot(&mut c).await;
    let s = c
        .call("suggest_redactions", json!({"capture_id": id}))
        .await;
    let ops: Vec<Value> = ok(&s)["rects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| json!({"op": "blur", "rect": r["rect"], "strength": 16}))
        .collect();
    let a = c
        .call(
            "annotate",
            json!({"capture_id": id, "ops": ops, "return_image": false}),
        )
        .await;
    ok(&a);
    let g = c.call("get_capture", json!({"capture_id": id})).await;
    assert_eq!(ok(&g)["ops"].as_array().unwrap().len(), 3);
    let u = c.call("undo", json!({"capture_id": id})).await;
    ok(&u);
    let g = c.call("get_capture", json!({"capture_id": id})).await;
    assert_eq!(ok(&g)["ops"], json!([]), "one undo removes all three");
}

#[tokio::test]
async fn no_recognizer_is_not_implemented_with_a_clear_message() {
    let (b, _dir) = builder(desktop());
    let d = b.text_recognizer(ScriptedRecognizer::unavailable()).build();
    let mut c = Client::connect(&d, "agent");
    let id = shot(&mut c).await;
    let r = c
        .call("suggest_redactions", json!({"capture_id": id}))
        .await;
    assert_eq!(code(&r), "not_implemented", "{r}");
    assert!(r["error"]["data"]["message"]
        .as_str()
        .unwrap()
        .contains("text recognition"));
}

/// A recognizer that can be installed (Linux: tesseract) says how in the error.
#[tokio::test]
async fn a_missing_helper_program_names_what_to_install() {
    use cm_render::ocr::tesseract::{SearchPaths, TesseractRecognizer};
    let (b, _dir) = builder(desktop());
    let d = b
        .text_recognizer(TesseractRecognizer::with_paths(SearchPaths::default()))
        .build();
    let mut c = Client::connect(&d, "agent");
    let id = shot(&mut c).await;
    let r = c
        .call("suggest_redactions", json!({"capture_id": id}))
        .await;
    assert_eq!(code(&r), "not_implemented", "{r}");
    let m = r["error"]["data"]["message"].as_str().unwrap();
    assert!(m.contains("apt install tesseract-ocr"), "{m}");
    assert!(m.contains("annotate"), "{m}");
}

#[tokio::test]
async fn failures_and_bad_input_are_reported() {
    let (b, _dir) = builder(desktop());
    let d = b
        .text_recognizer(ScriptedRecognizer::failing("engine exploded"))
        .build();
    let mut c = Client::connect(&d, "agent");
    let id = shot(&mut c).await;
    let r = c
        .call("suggest_redactions", json!({"capture_id": id}))
        .await;
    assert_eq!(code(&r), "internal", "{r}");
    let r = c
        .call("suggest_redactions", json!({"capture_id": "shot_nope"}))
        .await;
    assert_eq!(code(&r), "not_found", "{r}");
    let r = c
        .call(
            "suggest_redactions",
            json!({"capture_id": id, "kinds": ["face"]}),
        )
        .await;
    assert_eq!(code(&r), "invalid_input", "{r}");
}
