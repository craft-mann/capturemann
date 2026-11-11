//! LIB-1.AC7: `capture.created` is announced as soon as the screenshot is in the library (bundle and
//! poster), before the clipboard copy and the inline image, so the sidebar count does not wait for them.

#![allow(clippy::unwrap_used)]

mod common;

use cm_core::{Actor, CmError};
use cm_daemon::events::Event;
use cm_daemon::Clipboard;
use common::{builder, desktop};
use serde_json::json;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

type Slot = Arc<Mutex<Option<broadcast::Receiver<Event>>>>;

/// A clipboard that records whether `capture.created` was already published when it was called.
struct Spy {
    rx: Slot,
    seen_before_copy: Arc<Mutex<Option<bool>>>,
}

impl Clipboard for Spy {
    fn copy_png(&self, _png: &[u8]) -> Result<(), CmError> {
        let mut found = false;
        if let Some(rx) = self.rx.lock().unwrap().as_mut() {
            while let Ok(ev) = rx.try_recv() {
                found |= ev.name == "capture.created";
            }
        }
        *self.seen_before_copy.lock().unwrap() = Some(found);
        Ok(())
    }
}

#[tokio::test]
async fn lib1_ac7_capture_created_comes_before_the_clipboard_copy() {
    let (b, _dir) = builder(desktop());
    let slot: Slot = Arc::new(Mutex::new(None));
    let seen = Arc::new(Mutex::new(None));
    let d = b
        .clipboard(Spy {
            rx: slot.clone(),
            seen_before_copy: seen.clone(),
        })
        .build();
    *slot.lock().unwrap() = Some(d.subscribe());
    let r = d
        .execute_method(
            Actor::User,
            0,
            "capture_screenshot",
            &json!({"target": {"display_id": 1}, "copy_to_clipboard": true}),
        )
        .await
        .unwrap();
    assert_eq!(r["copied_to_clipboard"], true);
    assert_eq!(
        *seen.lock().unwrap(),
        Some(true),
        "capture.created must already be published when the clipboard copy runs"
    );
}
