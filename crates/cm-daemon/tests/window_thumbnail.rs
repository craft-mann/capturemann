//! `window_thumbnail` (internal, Free, mutating for read-only policy): the window picker's
//! downscaled PNG of one window. Tested with the fake backend.

#![allow(clippy::unwrap_used)]

mod common;

use base64::Engine as _;
use cm_capture::fake::FakeBackend;
use cm_daemon::ClientPolicy;
use common::{builder, Client};
use serde_json::{json, Value};

fn ok(r: &Value) -> &Value {
    assert!(r.get("error").is_none(), "unexpected error: {r}");
    &r["result"]
}

fn err_code(r: &Value) -> &str {
    r["error"]["data"]["code"].as_str().unwrap()
}

fn desktop() -> FakeBackend {
    FakeBackend::new()
        .with_display(1, 0, 0, 1200, 800, 2.0)
        .with_builtin(1)
        .with_window(10, "Editor", "Code", 100, 1, (0, 0, 1000, 500), false)
        .with_window(11, "Tiny", "Notes", 101, 1, (0, 0, 40, 30), false)
}

fn decode(result: &Value) -> (u32, u32) {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(result["png_base64"].as_str().unwrap())
        .unwrap();
    let img = image::load_from_memory(&bytes).unwrap();
    (img.width(), img.height())
}

#[tokio::test]
async fn returns_a_png_within_max_px_keeping_the_aspect() {
    let (b, _dir) = builder(desktop());
    let d = b.build();
    let mut c = Client::connect(&d, "overlay");

    let r = c
        .call("window_thumbnail", json!({"window_id": 10, "max_px": 200}))
        .await;
    let r = ok(&r);
    assert_eq!(r["window_id"], 10);
    assert_eq!(
        (r["width"].as_u64(), r["height"].as_u64()),
        (Some(200), Some(100))
    );
    assert_eq!(decode(r), (200, 100));

    // Default 360: the 1000x500 window becomes 360x180.
    let r = c.call("window_thumbnail", json!({"window_id": 10})).await;
    assert_eq!(decode(ok(&r)), (360, 180));

    // Below the clamp minimum: 10 behaves as 64.
    let r = c
        .call("window_thumbnail", json!({"window_id": 10, "max_px": 10}))
        .await;
    assert_eq!(decode(ok(&r)), (64, 32));

    // Above the clamp maximum: 5000 behaves as 1024, but a smaller window is not enlarged.
    let r = c
        .call("window_thumbnail", json!({"window_id": 10, "max_px": 5000}))
        .await;
    assert_eq!(decode(ok(&r)), (1000, 500));
    let r = c.call("window_thumbnail", json!({"window_id": 11})).await;
    let w = ok(&r)["width"].as_u64().unwrap();
    let h = ok(&r)["height"].as_u64().unwrap();
    assert!(w.max(h) <= 360);
    assert_eq!(decode(ok(&r)), (w as u32, h as u32));
}

#[tokio::test]
async fn errors_use_the_shared_codes() {
    let (b, _dir) = builder(desktop());
    let d = b.build();
    let mut c = Client::connect(&d, "overlay");
    let r = c.call("window_thumbnail", json!({"window_id": 999})).await;
    assert_eq!(err_code(&r), "not_found");
    let r = c.call("window_thumbnail", json!({})).await;
    assert_eq!(err_code(&r), "invalid_input");
    let r = c
        .call("window_thumbnail", json!({"window_id": 10, "max_px": "x"}))
        .await;
    assert_eq!(err_code(&r), "invalid_input");

    let (b, _dir2) = builder(desktop().without_permission());
    let d2 = b.build();
    let mut c2 = Client::connect(&d2, "overlay");
    let r = c2.call("window_thumbnail", json!({"window_id": 10})).await;
    assert_eq!(err_code(&r), "permission_missing");
}

#[tokio::test]
async fn read_only_clients_are_refused_and_nothing_is_stored() {
    let (b, _dir) = builder(desktop());
    let d = b.build();
    d.policies().set("viewer", ClientPolicy::ReadOnly);
    let mut v = Client::connect(&d, "viewer");
    v.hello("viewer").await;
    let r = v.call("window_thumbnail", json!({"window_id": 10})).await;
    assert!(r.get("error").is_some(), "{r}");

    let mut c = Client::connect(&d, "overlay");
    let r = c.call("window_thumbnail", json!({"window_id": 10})).await;
    ok(&r);
    let r = c.call("list_library", json!({})).await;
    assert_eq!(ok(&r)["captures"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn list_targets_reports_is_builtin() {
    let (b, _dir) = builder(desktop().with_display(2, 1200, 0, 800, 600, 1.0));
    let d = b.build();
    let mut c = Client::connect(&d, "overlay");
    let r = c.call("list_targets", json!({})).await;
    let displays = ok(&r)["displays"].as_array().unwrap().clone();
    let flag = |id: u64| {
        displays
            .iter()
            .find(|d| d["id"] == id)
            .map(|d| d["is_builtin"].clone())
    };
    assert_eq!(flag(1), Some(json!(true)));
    assert_eq!(flag(2), Some(json!(false)));
}
