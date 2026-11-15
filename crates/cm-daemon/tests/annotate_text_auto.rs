//! ANN-1.AC8: `annotate` with a text op and no `color` stores Auto's colour (and outline) and
//! returns it; an explicit `color` is used as given; the same picture and position give the same
//! result.

#![allow(clippy::unwrap_used)]

mod common;

use common::{builder, desktop, Client};
use serde_json::{json, Value};

fn ok(r: &Value) -> &Value {
    assert!(r.get("error").is_none(), "unexpected error: {r}");
    &r["result"]
}

#[tokio::test]
async fn ann_1_ac8_annotate_text_without_color_is_auto() {
    let (b, _dir) = builder(desktop());
    let d = b.build();
    let mut c = Client::connect(&d, "agent");
    let r = c
        .call("capture_screenshot", json!({"target": {"display_id": 1}}))
        .await;
    let id = ok(&r)["capture_id"].as_str().unwrap().to_owned();

    // What the rule gives for this picture, computed directly.
    let cap = c.call("get_capture", json!({"capture_id": id})).await;
    let cap = ok(&cap);
    let dir = cap["path"].as_str().unwrap();
    let file = cap["original"]["file"].as_str().unwrap();
    let img = image::open(std::path::Path::new(dir).join(file))
        .unwrap()
        .to_rgba8();
    let want = cm_render::auto_text_style(&img, (20.0, 30.0), 24.0);

    let a = c
        .call(
            "annotate",
            json!({"capture_id": id, "return_image": false, "ops": [
                { "op": "text", "at": [20, 30], "text": "Auto" },
                { "op": "text", "at": [20, 30], "text": "Mine", "color": "#FF3B3B" },
                { "op": "text", "at": [20, 30], "text": "Keep", "outline": null },
            ]}),
        )
        .await;
    let a = ok(&a);
    let auto = a["auto_text"].as_array().unwrap();
    assert_eq!(auto.len(), 2, "the explicit colour is not reported: {a}");
    assert_eq!(auto[0]["index"], 0);
    assert_eq!(auto[0]["color"], want.color.to_hex());
    assert_eq!(auto[0]["outline"], json!(want.outline.map(|c| c.to_hex())));
    // An op with its own `outline: null` keeps it.
    assert_eq!(auto[1]["index"], 2);
    assert_eq!(auto[1]["outline"], Value::Null);

    let cap = c.call("get_capture", json!({"capture_id": id})).await;
    let ops = ok(&cap)["ops"].as_array().unwrap().clone();
    assert_eq!(ops[0]["color"], want.color.to_hex());
    assert_eq!(ops[1]["color"], "#FF3B3B");
    assert_eq!(
        ops[1]["outline"],
        json!("#000000"),
        "an explicit colour adds no outline of Auto's: the old default stays"
    );
    assert_eq!(ops[2]["color"], want.color.to_hex());

    // Deterministic: the same op again gives the same stored colour.
    let again = c
        .call(
            "annotate",
            json!({"capture_id": id, "return_image": false, "ops": [
                { "op": "text", "at": [20, 30], "text": "Auto" } ]}),
        )
        .await;
    assert_eq!(ok(&again)["auto_text"][0]["color"], auto[0]["color"]);
    assert_eq!(ok(&again)["auto_text"][0]["outline"], auto[0]["outline"]);
}
