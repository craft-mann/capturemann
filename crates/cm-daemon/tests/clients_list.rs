//! AGT-2.AC15: every client that says `session.hello` is listed, with its last tool (Free tier
//! included), without entering the activity log. Tested with fakes.

#![allow(clippy::unwrap_used)]

mod common;

use cm_daemon::ClientPolicy;
use common::{daemon, Client};
use serde_json::json;

#[tokio::test]
async fn agt_2_ac15_hello_registers_the_client_and_free_calls_update_it() {
    let (d, _dir) = daemon();
    assert!(d.known_clients().is_empty());
    let mut c = Client::connect(&d, "x");
    c.hello("claude-test").await;
    let list = d.known_clients();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "claude-test");
    assert_eq!(list[0].last_tool, None);
    assert_eq!(list[0].calls, 0);
    let r = c.call("get_settings", json!({})).await;
    assert!(r.get("error").is_none(), "{r}");
    let r = c.call("list_targets", json!({})).await;
    assert!(r.get("error").is_none(), "{r}");
    let k = &d.known_clients()[0];
    assert_eq!(k.last_tool.as_deref(), Some("list_targets"));
    assert_eq!(k.calls, 2);
    assert!(k.last_seen >= k.first_seen);
    // Free-tier calls stay out of the 30-day activity log (AGT-2.AC6).
    assert!(d.activity().is_empty());
}

#[tokio::test]
async fn agt_2_ac15_two_sessions_with_one_name_are_one_client() {
    let (d, _dir) = daemon();
    let mut a = Client::connect(&d, "a");
    let mut b = Client::connect(&d, "b");
    a.hello("same").await;
    b.hello("same").await;
    a.call("get_settings", json!({})).await;
    b.call("get_settings", json!({})).await;
    let list = d.known_clients();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].calls, 2);
}

#[tokio::test]
async fn agt_2_ac15_a_blocked_client_is_listed_and_still_refused() {
    let (d, _dir) = daemon();
    d.policies().set("evil", ClientPolicy::Blocked);
    let mut c = Client::connect(&d, "x");
    let r = c.call("session.hello", json!({"client": "evil"})).await;
    assert_eq!(r["error"]["data"]["code"], "user_declined");
    assert_eq!(d.known_clients()[0].name, "evil");
}

#[tokio::test]
async fn agt_2_ac15_the_name_stays_fixed_and_the_list_is_bounded() {
    let (d, _dir) = daemon();
    let mut c = Client::connect(&d, "x");
    c.hello("first").await;
    let r = c.call("session.hello", json!({"client": "second"})).await;
    assert_eq!(r["error"]["data"]["code"], "invalid_input");
    let names: Vec<_> = d.known_clients().into_iter().map(|k| k.name).collect();
    assert_eq!(names, ["first"]);
    for i in 0..(cm_daemon::MAX_KNOWN_CLIENTS + 10) {
        let mut n = Client::connect(&d, "x");
        n.hello(&format!("bot-{i}")).await;
    }
    assert_eq!(d.known_clients().len(), cm_daemon::MAX_KNOWN_CLIENTS);
}
