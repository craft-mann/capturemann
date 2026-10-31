//! `capture_screenshot { target: { pick: true } }` and the `capabilities` of `list_targets`
//! (CAP-1 on a Wayland session: the desktop's own picker chooses a window or a region). Tested
//! with a scripted picker; the real portal is `cm-capture`'s ignored `real_portal` test.

#![allow(clippy::unwrap_used)]

mod common;

use cm_capture::fake::FakeBackend;
use cm_capture::Frame;
use cm_core::CmError;
use common::{builder, desktop, Client};
use serde_json::{json, Value};

fn ok(r: &Value) -> &Value {
    assert!(r.get("error").is_none(), "{r}");
    &r["result"]
}

fn wayland_like(answer: Result<Frame, CmError>) -> FakeBackend {
    FakeBackend::new()
        .with_display(1, 0, 0, 400, 300, 1.0)
        .with_picker(answer)
}

fn chosen() -> Frame {
    Frame::new(3, 2, vec![200; 3 * 2 * 4]).unwrap()
}

#[tokio::test]
async fn list_targets_says_what_this_desktop_can_do() {
    let (b, _d) = builder(desktop());
    let d = b.build();
    let mut c = Client::connect(&d, "agent");
    let r = c.call("list_targets", json!({})).await;
    let caps = &ok(&r)["capabilities"];
    assert_eq!(caps["list_windows"], true, "{r}");
    assert_eq!(caps["capture_window"], true);
    assert_eq!(caps["pick"], false);

    let (b, _d) = builder(wayland_like(Ok(chosen())));
    let d = b.build();
    let mut c = Client::connect(&d, "agent");
    let r = c.call("list_targets", json!({"windows": false})).await;
    let caps = &ok(&r)["capabilities"];
    assert_eq!(caps["list_windows"], false, "{r}");
    assert_eq!(caps["capture_window"], false);
    assert_eq!(caps["capture_region"], true);
    assert_eq!(caps["pick"], true);
}

#[tokio::test]
async fn pick_stores_what_the_person_chose() {
    let (b, _d) = builder(wayland_like(Ok(chosen())));
    let d = b.build();
    let mut c = Client::connect(&d, "agent");
    let r = c
        .call("capture_screenshot", json!({"target": {"pick": true}}))
        .await;
    let out = ok(&r);
    assert_eq!(
        (out["width"].as_u64(), out["height"].as_u64()),
        (Some(3), Some(2)),
        "{r}"
    );
    assert_eq!(out["kind"], "region");
    assert!(out["capture_id"].as_str().is_some());
    // Stored in the library like any screenshot.
    let r = c.call("list_library", json!({})).await;
    assert_eq!(ok(&r)["captures"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn closing_the_picker_is_user_declined_and_stores_nothing() {
    let (b, _d) = builder(wayland_like(Err(CmError::user_declined("closed"))));
    let d = b.build();
    let mut c = Client::connect(&d, "agent");
    let r = c
        .call("capture_screenshot", json!({"target": {"pick": true}}))
        .await;
    assert_eq!(r["error"]["data"]["code"], "user_declined", "{r}");
    let r = c.call("list_library", json!({})).await;
    assert_eq!(ok(&r)["captures"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn a_desktop_without_a_picker_says_not_implemented() {
    let (b, _d) = builder(desktop());
    let d = b.build();
    let mut c = Client::connect(&d, "agent");
    let r = c
        .call("capture_screenshot", json!({"target": {"pick": true}}))
        .await;
    assert_eq!(r["error"]["data"]["code"], "not_implemented", "{r}");
}

#[tokio::test]
async fn pick_takes_no_other_target_key_and_no_delay() {
    let (b, _d) = builder(wayland_like(Ok(chosen())));
    let d = b.build();
    let mut c = Client::connect(&d, "agent");
    for params in [
        json!({"target": {"pick": true, "display_id": 1}}),
        json!({"target": {"pick": false}}),
        json!({"target": {"pick": true}, "delay_s": 2}),
    ] {
        let r = c.call("capture_screenshot", params.clone()).await;
        assert_eq!(r["error"]["data"]["code"], "invalid_input", "{params} {r}");
    }
}
