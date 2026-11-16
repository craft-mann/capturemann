//! S24: the Preview switches (`previews.long_shot`, `previews.click_guide`). A preview switched
//! off does not start (`feature_disabled`, before any dialog); reading and exporting what was made
//! with it, stopping a run that already started and every other tool keep working. With a scripted
//! click source and the fake screen: no real input, no real screen.

#![allow(clippy::unwrap_used)]

mod common;

use std::sync::Arc;

use cm_core::{Actor, ErrorCode};
use cm_daemon::guide::ScriptedClicks;
use cm_daemon::testing::FixedConfirmer;
use cm_daemon::{Daemon, DaemonError, Decision};
use common::{builder, desktop, Client};
use serde_json::{json, Value};

fn rig(
    decision: Decision,
) -> (
    Daemon,
    Arc<FixedConfirmer>,
    ScriptedClicks,
    tempfile::TempDir,
) {
    let confirmer = Arc::new(FixedConfirmer::new(decision));
    let clicks = ScriptedClicks::new();
    let (b, dir) = builder(desktop());
    let d = b
        .confirmer(confirmer.clone())
        .guide_clicks(clicks.clone())
        .build();
    (d, confirmer, clicks, dir)
}

fn agent() -> Actor {
    Actor::agent("claude").unwrap()
}

async fn user_sets(d: &Daemon, key: &str, value: Value) {
    d.execute_method(
        Actor::User,
        0,
        "set_settings",
        &json!({"values": {key: value}}),
    )
    .await
    .unwrap();
}

fn disabled(r: Result<Value, DaemonError>, feature: &str) {
    let e = r.expect_err("must be refused");
    assert_eq!(e.code(), Some(ErrorCode::FeatureDisabled), "{e:?}");
    let v = e.to_rpc_error();
    assert_eq!(v["data"]["code"], "feature_disabled", "{v}");
    assert_eq!(v["data"]["data"]["feature"], feature, "{v}");
    assert_eq!(v["code"], -32014, "{v}");
}

const LONG: fn() -> Value = || json!({"mode": "long", "target": {"window_id": 1}});
const GUIDE: fn() -> Value = || json!({"target": {"display_id": 1}, "countdown_s": 0});

#[tokio::test]
async fn both_previews_are_on_by_default() {
    let (d, confirmer, _clicks, _dir) = rig(Decision::Approve);
    // An agent starts a guide with the default settings (nothing stored).
    let r = d
        .execute_method(agent(), 0, "start_guide", &GUIDE())
        .await
        .unwrap();
    assert_eq!(r["state"], "recording");
    assert_eq!(confirmer.shown().len(), 1, "the Confirm dialog ran");
}

#[tokio::test]
async fn long_shot_off_refuses_every_actor_before_any_dialog() {
    let (d, confirmer, _clicks, _dir) = rig(Decision::Approve);
    user_sets(&d, "previews.long_shot", json!(false)).await;
    for who in [Actor::User, agent()] {
        disabled(
            d.execute_method(who, 0, "capture_screenshot", &LONG())
                .await,
            "long_shot",
        );
    }
    assert!(
        confirmer.shown().is_empty(),
        "refused before a dialog opened"
    );
    // An ordinary screenshot is not a preview.
    let shot = d
        .execute_method(
            agent(),
            0,
            "capture_screenshot",
            &json!({"target": {"display_id": 1}}),
        )
        .await
        .unwrap();
    assert!(shot["capture_id"].is_string(), "{shot}");
}

#[tokio::test]
async fn click_guide_off_refuses_start_but_keeps_reading_and_exporting() {
    let (d, confirmer, clicks, dir) = rig(Decision::Approve);
    let mut c = Client::connect(&d, "agent");
    let r = c.call("start_guide", GUIDE()).await;
    let id = r["result"]["guide_id"].as_str().unwrap().to_owned();
    clicks.click(24.0, 28.0);
    // Switched off while it records: stopping it (saving what was captured) still works.
    user_sets(&d, "previews.click_guide", json!(false)).await;
    let stopped = c.call("stop_guide", json!({"guide_id": id})).await;
    assert_eq!(stopped["result"]["state"], "stopped", "{stopped}");
    let before = confirmer.shown().len();
    // New work is refused for the app and for agents, with no dialog.
    for who in [Actor::User, agent()] {
        disabled(
            d.execute_method(who, 0, "start_guide", &GUIDE()).await,
            "click_guide",
        );
    }
    assert_eq!(confirmer.shown().len(), before);
    // What exists stays readable and exportable.
    let got = c.call("get_guide", json!({"guide_id": id})).await;
    assert_eq!(got["result"]["step_count"], 1, "{got}");
    let out = dir.path().join("guide.html");
    let ex = c
        .call(
            "export_guide",
            json!({"guide_id": id, "format": "html", "dest": out}),
        )
        .await;
    assert!(ex.get("error").is_none(), "{ex}");
    assert!(out.exists());
    // Switching it on again starts it again.
    user_sets(&d, "previews.click_guide", json!(true)).await;
    let again = c.call("start_guide", GUIDE()).await;
    assert_eq!(again["result"]["state"], "recording", "{again}");
}

#[tokio::test]
async fn one_preview_off_does_not_touch_the_other() {
    let (d, _confirmer, _clicks, _dir) = rig(Decision::Approve);
    user_sets(&d, "previews.long_shot", json!(false)).await;
    let r = d
        .execute_method(Actor::User, 0, "start_guide", &GUIDE())
        .await
        .unwrap();
    assert_eq!(r["state"], "recording");
}

#[tokio::test]
async fn an_agent_cannot_switch_a_preview_back_on() {
    let (d, _confirmer, _clicks, _dir) = rig(Decision::Approve);
    user_sets(&d, "previews.click_guide", json!(false)).await;
    let mut c = Client::connect(&d, "agent");
    c.hello("rogue").await;
    let r = c
        .call(
            "set_settings",
            json!({"values": {"previews.click_guide": true}}),
        )
        .await;
    assert_eq!(
        r["error"]["data"]["data"]["reason"], "app_only_setting",
        "{r}"
    );
    // The switch is readable (get_settings), and still off.
    let g = c.call("get_settings", json!({})).await;
    assert_eq!(g["result"]["settings"]["previews.click_guide"], false);
    disabled(
        d.execute_method(agent(), 0, "start_guide", &GUIDE()).await,
        "click_guide",
    );
}
