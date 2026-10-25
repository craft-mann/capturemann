//! CAM-1: `camera.connected` / `camera.disconnected` from the daemon's device watcher, over a
//! scripted watcher (no camera, no polling thread).

#![allow(clippy::unwrap_used)]

mod common;

use std::time::Duration;

use cm_camera::{CameraInfo, CameraKind, FakeCameraWatcher};
use cm_core::TrackKind;
use cm_daemon::testing::FakeSources;
use cm_daemon::Event;
use common::{builder, desktop};
use serde_json::json;
use tokio::sync::broadcast::Receiver;

fn cam(id: &str, name: &str) -> CameraInfo {
    CameraInfo {
        id: id.into(),
        name: name.into(),
        kind: CameraKind::External,
        is_default: false,
        max_size: [1920, 1080],
        max_fps: 30.0,
        modes: Vec::new(),
        in_use_elsewhere: false,
    }
}

async fn next(rx: &mut Receiver<Event>, name: &str) -> Event {
    let wait = async {
        loop {
            let e = rx.recv().await.unwrap();
            if e.name == name {
                return e;
            }
        }
    };
    tokio::time::timeout(Duration::from_secs(10), wait)
        .await
        .unwrap_or_else(|_| panic!("no `{name}` event"))
}

#[tokio::test]
async fn cam1_plug_and_unplug_publish_events_while_idle() {
    let (b, _dir) = builder(desktop());
    let (watcher, hw) = FakeCameraWatcher::new();
    let daemon = b.camera_watcher(watcher).build();
    assert!(hw.is_running(), "the watcher starts with the daemon");
    let mut rx = daemon.subscribe();

    hw.connect(cam("usb-1", "Logitech C920"));
    let ev = next(&mut rx, "camera.connected").await;
    assert_eq!(ev.data["device_id"], "usb-1");
    assert_eq!(ev.data["name"], "Logitech C920");
    assert_eq!(ev.data["kind"], "external");
    assert_eq!(ev.data["is_default"], false);

    hw.disconnect("usb-1");
    let ev = next(&mut rx, "camera.disconnected").await;
    assert_eq!(ev.data["device_id"], "usb-1");

    daemon.shutdown_recording().await;
    assert!(!hw.is_running(), "the watcher stops with the daemon");
    hw.connect(cam("usb-2", "After shutdown"));
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(rx.try_recv().is_err(), "no events after the stop");
}

#[tokio::test]
async fn cam1_unplug_during_a_recording_fires_the_event_and_the_recording_goes_on() {
    let (b, _dir) = builder(desktop());
    let (watcher, hw) = FakeCameraWatcher::new();
    hw.connect(cam("usb-1", "Logitech C920"));
    let sources = FakeSources::new();
    let daemon = b
        .camera_watcher(watcher)
        .recording_sources(sources.clone())
        .build();
    let mut rx = daemon.subscribe();
    daemon
        .execute_method(
            cm_core::Actor::User,
            0,
            "start_recording",
            &json!({ "target": { "display_id": 1 }, "countdown_s": 0, "camera": {} }),
        )
        .await
        .unwrap();
    // The camera track notices first (its own AVFoundation observer), the watcher second.
    sources
        .handle(TrackKind::Camera)
        .unwrap()
        .warn("camera_lost", "The camera was disconnected");
    hw.disconnect("usb-1");
    let ev = next(&mut rx, "camera.disconnected").await;
    assert_eq!(ev.data["device_id"], "usb-1");
    let w = next(&mut rx, "recording.warning").await;
    assert_eq!(w.data["code"], "camera_lost");
    assert!(daemon.recording_active(), "the session continues");
    daemon
        .execute_method(cm_core::Actor::User, 0, "stop_recording", &json!({}))
        .await
        .unwrap();
}
