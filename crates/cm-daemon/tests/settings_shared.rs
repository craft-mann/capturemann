//! Windows lab round 3, F7: the CLI (an agent client over JSON-RPC) and the desktop app (the
//! user, in-process) are two clients of one daemon and one settings store. A write by either is
//! visible to the other at once, and `settings.changed` tells the app to refresh its cache.
//! Fakes only.

#![allow(clippy::unwrap_used)]

mod common;

use std::sync::Arc;

use cm_core::{Actor, CmError};
use cm_daemon::{MemorySettings, SettingsStore};
use common::{builder, desktop, Client};
use serde_json::{json, Map, Value};

#[derive(Clone, Default)]
struct Shared(Arc<MemorySettings>);

impl SettingsStore for Shared {
    fn all(&self) -> Result<Map<String, Value>, CmError> {
        self.0.all()
    }
    fn set(&self, v: &Map<String, Value>) -> Result<Map<String, Value>, CmError> {
        self.0.set(v)
    }
}

#[tokio::test]
async fn cli_and_app_share_capture_delay_through_one_daemon() {
    let store = Shared::default();
    let (b, _dir) = builder(desktop());
    let daemon = b.settings(store.clone()).build();
    let mut app_events = daemon.subscribe();

    let mut cli = Client::connect(&daemon, "cli");
    cli.hello("capturemann-cli").await;

    // CLI -> app: the store the app reads holds the value, and the app is told.
    let r = cli
        .call("set_settings", json!({"values": {"capture.delay_s": 5}}))
        .await;
    assert!(r.get("error").is_none(), "{r}");
    assert_eq!(store.all().unwrap()["capture.delay_s"], 5);
    let ev = app_events.recv().await.unwrap();
    assert_eq!(ev.name, "settings.changed");
    assert_eq!(ev.data["keys"], json!(["capture.delay_s"]));

    // App -> CLI: a write by the in-process user is what `settings get` returns at once.
    daemon
        .execute_method(
            Actor::User,
            0,
            "set_settings",
            &json!({"values": {"capture.delay_s": 10}}),
        )
        .await
        .unwrap();
    let ev = app_events.recv().await.unwrap();
    assert_eq!(ev.name, "settings.changed");
    let g = cli
        .call("get_settings", json!({"keys": ["capture.delay_s"]}))
        .await;
    assert_eq!(g["result"]["settings"]["capture.delay_s"], 10, "{g}");
}
