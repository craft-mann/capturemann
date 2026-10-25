//! STO-4.AC1 and AC2: copy the link when the upload finishes. "Tested with fakes": a fake
//! clipboard, plain JSON events, no window.

#![allow(clippy::unwrap_used)]

use std::cell::RefCell;

use serde_json::json;

use super::*;

#[derive(Default)]
struct FakeClipboard {
    text: RefCell<Option<String>>,
    fail: bool,
}

impl TextClipboard for FakeClipboard {
    fn set_text(&self, text: &str) -> Result<(), String> {
        if self.fail {
            return Err("no clipboard".into());
        }
        *self.text.borrow_mut() = Some(text.to_owned());
        Ok(())
    }
}

fn dests(id: &str) -> Option<DestInfo> {
    (id == "drive").then(|| DestInfo {
        name: "Google Drive".into(),
        local: false,
    })
}

fn done_effect(link: Value) -> Effect {
    let mut t = UploadTracker::default();
    t.on_event(
        "upload.queued",
        &json!({ "job_id": "j1", "capture_id": "rec_1", "title": "Lesson",
                 "destination_id": "drive", "destination": "Google Drive" }),
        &dests,
    );
    t.on_event(
        "job.done",
        &json!({ "job_id": "j1", "type": "upload", "state": "done",
                 "result": { "destination_id": "drive", "capture_id": "rec_1",
                             "files": [{ "remote_id": "f", "bytes": 10 }],
                             "share_link": link, "notes": [] } }),
        &dests,
    )
    .remove(0)
}

#[test]
fn a_finished_upload_with_a_link_puts_it_on_the_clipboard_by_default() {
    let mut e = done_effect(json!("https://drive.example/s/1"));
    let clip = FakeClipboard::default();
    assert!(copy_link_enabled(&serde_json::Map::new()), "default on");
    assert_eq!(auto_copy(&mut e, true, &clip), AutoCopy::Copied);
    assert_eq!(
        clip.text.borrow().as_deref(),
        Some("https://drive.example/s/1")
    );
    let (_, spec) = toast_for(&e).unwrap();
    assert!(spec.body.unwrap().contains("Link copied to the clipboard"));
}

#[test]
fn the_setting_off_leaves_the_clipboard_alone() {
    let mut e = done_effect(json!("https://drive.example/s/1"));
    let clip = FakeClipboard::default();
    let off: serde_json::Map<String, Value> =
        [("storage.copy_link_when_done".to_owned(), json!(false))]
            .into_iter()
            .collect();
    assert!(!copy_link_enabled(&off));
    assert_eq!(auto_copy(&mut e, false, &clip), AutoCopy::Skipped);
    assert!(clip.text.borrow().is_none());
    // The toast still offers Copy link by hand.
    let (_, spec) = toast_for(&e).unwrap();
    assert!(spec.buttons.iter().any(|b| b.label == "Copy link"));
}

#[test]
fn no_link_means_nothing_is_copied() {
    // Link mode none, or a local folder: the upload has no share link.
    let mut e = done_effect(Value::Null);
    let clip = FakeClipboard::default();
    assert_eq!(auto_copy(&mut e, true, &clip), AutoCopy::Skipped);
    assert!(clip.text.borrow().is_none());
    // A failure is not a done upload.
    let mut f = Effect::Failed {
        job_id: "j".into(),
        title: None,
        destination: "Drive".into(),
        reason: "x".into(),
    };
    assert_eq!(auto_copy(&mut f, true, &clip), AutoCopy::Skipped);
}

#[test]
fn a_clipboard_that_refuses_is_reported_and_the_toast_is_unchanged() {
    let mut e = done_effect(json!("https://drive.example/s/1"));
    let before = e.clone();
    let clip = FakeClipboard {
        fail: true,
        ..Default::default()
    };
    assert_eq!(
        auto_copy(&mut e, true, &clip),
        AutoCopy::Failed("no clipboard".into())
    );
    assert_eq!(e, before, "no false \"copied\" note");
}

#[test]
fn an_organization_note_and_the_copied_note_are_both_shown() {
    let mut e = done_effect(json!("https://drive.example/s/1"));
    if let Effect::Done { note, .. } = &mut e {
        *note = Some("Public links are forbidden: this is an organization link".into());
    }
    auto_copy(&mut e, true, &FakeClipboard::default());
    let Effect::Done { note, .. } = e else {
        panic!()
    };
    let n = note.unwrap();
    assert!(n.contains("organization link") && n.ends_with("Link copied to the clipboard"));
}
