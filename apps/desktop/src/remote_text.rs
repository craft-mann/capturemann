//! The words for the remote copies of a capture (the delete dialog, the Confirm dialog of an
//! agent's `delete_capture`, and the result toast), made from the structured `remote_items` the
//! daemon sends next to its English rows (`cm_daemon` `remote_delete.rs`, `RemotePlan::items_json`).
//!
//! Each copy is `{ state, n, form, path, title, url, channel, place, reason, .. }`; this module
//! words it with whole sentences and named placeholders. In English the sentences are
//! byte-identical to the daemon's own rows (tests pin that); service names are brand names and
//! stay as they are. A form or reason id this version does not know gives `None`, and the caller
//! falls back to the daemon's English.

use cm_i18n::tr;
use serde_json::Value;

fn text<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str).filter(|s| !s.is_empty())
}

/// `folder "/Users/me/Drive"`, `Google Drive (me), destination "Drive"` or
/// `Google Drive, destination "Drive"`.
pub fn place_text(place: &Value) -> Option<String> {
    Some(match text(place, "type")? {
        "folder" => tr!("folder \"{path}\"", path = text(place, "path")?),
        "service" => match text(place, "account") {
            Some(account) => tr!(
                "{service} ({account}), destination \"{name}\"",
                service = text(place, "service")?,
                account = account,
                name = text(place, "name")?
            ),
            None => tr!(
                "{service}, destination \"{name}\"",
                service = text(place, "service")?,
                name = text(place, "name")?
            ),
        },
        _ => return None,
    })
}

/// Why a copy stays online.
pub fn reason_text(reason: &Value) -> Option<String> {
    Some(match text(reason, "id")? {
        "destination_removed" => tr!("that destination was removed from Settings"),
        "provider_unavailable" => tr!(
            "deleting on {service} is not available yet",
            service = text(reason, "service")?
        ),
        "drive_no_folder_id" => tr!(
            "this copy was made before folder ids were recorded; delete the folder in Google Drive yourself"
        ),
        "drive_no_file_id" => tr!("the record has no Google Drive file id"),
        "bundle_unreadable" => tr!("the bundle cannot be read"),
        "no_folder_in_destination" => tr!("the record has no folder inside the destination"),
        "path_outside_destination" => {
            tr!("the recorded path is not inside the destination folder")
        }
        "platform_unsupported" => tr!("deleting on that platform is not available yet"),
        "no_video_id" => tr!("the record has no video id"),
        "account_not_connected" => tr!(
            "that YouTube account is not connected any more; connect it again to delete the video"
        ),
        _ => return None,
    })
}

/// One copy as the dialog describes it: `Google Drive: the file Demo.mp4 (...)`.
pub fn item_text(item: &Value) -> Option<String> {
    let path = text(item, "path");
    let place = item.get("place").filter(|p| p.is_object());
    let place_text = place.and_then(place_text);
    Some(match text(item, "form")? {
        "drive_file" => tr!(
            "Google Drive: the file {path} ({place})",
            path = path?,
            place = place_text?
        ),
        "drive_folder" => tr!(
            "Google Drive: the folder {path} and everything in it ({place})",
            path = path?,
            place = place_text?
        ),
        "local_bundle" => tr!(
            "Folder: the {n} copied files of {path} in {place} (files you added there stay)",
            n = item.get("files").and_then(Value::as_u64)?,
            path = path?,
            place = place_text?
        ),
        "local_file" => tr!("Folder: the file {path}", path = path?),
        "youtube" => {
            let url = text(item, "url").unwrap_or_default();
            let channel = text(item, "channel")?;
            let title = text(item, "title")
                .map(str::to_owned)
                .unwrap_or_else(|| tr!("(untitled)"));
            tr!(
                "YouTube: the video \"{title}\" {url} (channel \"{channel}\")",
                title = title,
                url = url,
                channel = channel
            )
        }
        "copy" => {
            let original = text(item, "what") == Some("original");
            match (&place_text, text(item, "destination_name")) {
                (Some(place), _) if original => tr!(
                    "original bundle {path} in {place}",
                    path = path?,
                    place = place
                ),
                (Some(place), _) => tr!("export {path} in {place}", path = path?, place = place),
                (None, Some(name)) if original => tr!(
                    "original bundle {path} at \"{name}\"",
                    path = path?,
                    name = name
                ),
                (None, Some(name)) => {
                    tr!("export {path} at \"{name}\"", path = path?, name = name)
                }
                _ => return None,
            }
        }
        _ => return None,
    })
}

/// A copy that stays online: `<copy> (<why>)`.
pub fn left_text(item: &Value) -> Option<String> {
    Some(tr!(
        "{item} ({reason})",
        item = item_text(item)?,
        reason = reason_text(item.get("reason")?)?
    ))
}

/// A copy that is gone already: the row of the dialog.
pub fn gone_text(item: &Value) -> Option<String> {
    Some(tr!(
        "{item} (already gone, nothing to delete)",
        item = item_text(item)?
    ))
}

/// The row `Left in place: <copy> (<why>)` of the Confirm dialog.
pub fn left_row(item: &Value) -> Option<String> {
    Some(tr!("Left in place: {item}", item = left_text(item)?))
}

/// The copies of one state in number order.
pub fn items_of<'a>(items: &'a Value, state: &str) -> Vec<&'a Value> {
    let mut v: Vec<&Value> = items
        .as_array()
        .map(|a| {
            a.iter()
                .filter(|i| i.get("state").and_then(Value::as_str) == Some(state))
                .collect()
        })
        .unwrap_or_default();
    v.sort_by_key(|i| i.get("n").and_then(Value::as_u64).unwrap_or(0));
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use cm_i18n::{with_language, Lang};
    use serde_json::json;

    fn drive() -> Value {
        json!({ "type": "service", "service": "Google Drive", "account": "me", "name": "Drive" })
    }

    /// Every form of a copy, as the daemon's `view` builds it, with the English the daemon sends.
    fn samples() -> Vec<(Value, &'static str)> {
        vec![
            (
                json!({ "form": "drive_file", "path": "a.mp4", "place": drive() }),
                "Google Drive: the file a.mp4 (Google Drive (me), destination \"Drive\")",
            ),
            (
                json!({ "form": "drive_folder", "path": "a.cmrec", "place": drive() }),
                "Google Drive: the folder a.cmrec and everything in it (Google Drive (me), destination \"Drive\")",
            ),
            (
                json!({ "form": "local_bundle", "files": 7, "path": "a.cmrec",
                        "place": { "type": "folder", "path": "/nas" } }),
                "Folder: the 7 copied files of a.cmrec in folder \"/nas\" (files you added there stay)",
            ),
            (
                json!({ "form": "local_file", "path": "/nas/a.mp4" }),
                "Folder: the file /nas/a.mp4",
            ),
            (
                json!({ "form": "youtube", "title": "Demo", "url": "https://youtu.be/x", "channel": "Me" }),
                "YouTube: the video \"Demo\" https://youtu.be/x (channel \"Me\")",
            ),
            (
                json!({ "form": "youtube", "title": null, "url": "u", "channel": "Me" }),
                "YouTube: the video \"(untitled)\" u (channel \"Me\")",
            ),
            (
                json!({ "form": "copy", "what": "export", "path": "a.mp4",
                        "place": { "type": "service", "service": "OneDrive", "name": "OD" } }),
                "export a.mp4 in OneDrive, destination \"OD\"",
            ),
            (
                json!({ "form": "copy", "what": "original", "path": "a.cmrec",
                        "place": { "type": "folder", "path": "/nas" } }),
                "original bundle a.cmrec in folder \"/nas\"",
            ),
            (
                json!({ "form": "copy", "what": "export", "path": "a.mp4", "destination_name": "Old disk" }),
                "export a.mp4 at \"Old disk\"",
            ),
            (
                json!({ "form": "copy", "what": "original", "path": "a.cmrec", "destination_name": "Old disk" }),
                "original bundle a.cmrec at \"Old disk\"",
            ),
        ]
    }

    fn reasons() -> Vec<(Value, &'static str)> {
        vec![
            (
                json!({ "id": "destination_removed" }),
                "that destination was removed from Settings",
            ),
            (
                json!({ "id": "provider_unavailable", "service": "Dropbox" }),
                "deleting on Dropbox is not available yet",
            ),
            (
                json!({ "id": "drive_no_folder_id" }),
                "this copy was made before folder ids were recorded; delete the folder in Google Drive yourself",
            ),
            (
                json!({ "id": "drive_no_file_id" }),
                "the record has no Google Drive file id",
            ),
            (
                json!({ "id": "bundle_unreadable" }),
                "the bundle cannot be read",
            ),
            (
                json!({ "id": "no_folder_in_destination" }),
                "the record has no folder inside the destination",
            ),
            (
                json!({ "id": "path_outside_destination" }),
                "the recorded path is not inside the destination folder",
            ),
            (
                json!({ "id": "platform_unsupported" }),
                "deleting on that platform is not available yet",
            ),
            (json!({ "id": "no_video_id" }), "the record has no video id"),
            (
                json!({ "id": "account_not_connected" }),
                "that YouTube account is not connected any more; connect it again to delete the video",
            ),
        ]
    }

    /// In English every form and every reason reads exactly like the daemon's own text.
    #[test]
    fn english_matches_the_daemons_sentences() {
        for (item, english) in samples() {
            assert_eq!(item_text(&item).as_deref(), Some(english), "{item}");
        }
        for (reason, english) in reasons() {
            assert_eq!(reason_text(&reason).as_deref(), Some(english), "{reason}");
        }
        let left = json!({ "form": "copy", "what": "export", "path": "a", "destination_name": "X",
                           "reason": { "id": "destination_removed" } });
        assert_eq!(
            left_row(&left).unwrap(),
            "Left in place: export a at \"X\" (that destination was removed from Settings)"
        );
        assert_eq!(
            gone_text(&json!({ "form": "local_file", "path": "/a" })).unwrap(),
            "Folder: the file /a (already gone, nothing to delete)"
        );
    }

    /// The Vietnamese rendering: every form and every reason has its own sentence, with the
    /// parameters in it and no English left over.
    #[test]
    fn vietnamese_words_every_form_and_reason() {
        with_language(Lang::Vi, || {
            for (item, english) in samples() {
                let t = item_text(&item).unwrap();
                assert_ne!(t, english, "{item}");
                assert!(!t.contains("the file") && !t.contains("destination"), "{t}");
            }
            for (reason, english) in reasons() {
                let t = reason_text(&reason).unwrap();
                assert_ne!(t, english, "{reason}");
            }
            let t = item_text(&samples()[0].0).unwrap();
            assert_eq!(
                t,
                "Google Drive: tệp a.mp4 (Google Drive (me), điểm lưu “Drive”)"
            );
            let t = item_text(&samples()[4].0).unwrap();
            assert_eq!(t, "YouTube: video “Demo” https://youtu.be/x (kênh “Me”)");
            let t = item_text(&samples()[2].0).unwrap();
            assert!(t.contains('7') && t.contains("/nas"), "{t}");
            let left = json!({ "form": "copy", "what": "export", "path": "a", "destination_name": "X",
                               "reason": { "id": "destination_removed" } });
            assert_eq!(
                left_row(&left).unwrap(),
                "Giữ nguyên: bản xuất a tại “X” (điểm lưu đó đã bị gỡ khỏi Cài đặt)"
            );
        });
    }

    #[test]
    fn unknown_forms_and_reasons_give_none() {
        assert_eq!(item_text(&json!({ "form": "teleport", "path": "a" })), None);
        assert_eq!(reason_text(&json!({ "id": "from_the_future" })), None);
        assert_eq!(item_text(&json!({})), None);
        assert_eq!(
            left_text(&json!({ "form": "local_file", "path": "a" })),
            None
        );
    }

    #[test]
    fn items_are_taken_by_state_in_number_order() {
        let items = json!([
            { "state": "delete", "n": 2, "path": "b" },
            { "state": "left", "n": 1, "path": "x" },
            { "state": "delete", "n": 1, "path": "a" },
        ]);
        let d = items_of(&items, "delete");
        assert_eq!(
            (d[0]["path"].as_str(), d[1]["path"].as_str()),
            (Some("a"), Some("b"))
        );
        assert_eq!(items_of(&items, "gone").len(), 0);
        assert!(items_of(&Value::Null, "delete").is_empty());
    }
}
