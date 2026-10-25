//! The library "Delete" flow with the remote question (LIB-1, LIB-1.AC4), as display-free logic.
//!
//! A capture with copies online (Drive uploads in `uploads[]`, YouTube videos in `publishes[]`)
//! gets a confirmation that lists them and offers "Also delete the copies on ... (permanent)",
//! default off: the copies stay unless the user ticks it. A capture without any keeps the single
//! plain confirmation. The rows are the daemon's own (`Daemon::describe_delete`, the same ones
//! the Confirm dialog of an agent's call shows), so the words never drift from what runs.
//!
//! Three steps, all decided here and drawn elsewhere (`ui::app::delete_capture`):
//!
//! 1. [`prepare`] asks the daemon what a delete with the remote copies would do and builds the
//!    [`DeletePrompt`], whose [`DeletePrompt::dialog`] is the confirmation.
//! 2. [`also_remote`] turns the user's answer into the `also_delete_remote` argument.
//! 3. [`finish`] calls `delete_capture` and turns the result, or the daemon's error with its
//!    `data.remote`, into a [`Notice`] in plain words. A partial failure keeps the local capture
//!    (the daemon guarantees it) and says what was and was not deleted.

use async_trait::async_trait;
use cm_core::ErrorCode;
use cm_daemon::{Daemon, Decision};
use cm_i18n::{tr, tr_ctx, trn};
use serde_json::{json, Value};

use crate::backend::{Backend, CallError};
use crate::confirm::{delete_dialog, DialogModel};
use crate::remote_text as rt;

/// What the flow needs of the daemon; a fake in the tests.
#[async_trait]
pub trait DeleteApi: Send + Sync {
    /// What `delete_capture { also_delete_remote: true }` would do: `{ details, remote_items }`,
    /// the dialog's English rows and the remote copies as structured data (the app words them).
    async fn describe(&self, capture_id: &str) -> Result<Value, String>;
    /// `delete_capture`, as the user (the app asked already).
    async fn delete(&self, capture_id: &str, also_remote: bool) -> Result<Value, CallError>;
}

/// The real thing.
pub struct DaemonApi(pub Daemon);

#[async_trait]
impl DeleteApi for DaemonApi {
    async fn describe(&self, capture_id: &str) -> Result<Value, String> {
        self.0
            .describe_delete(capture_id, true)
            .await
            .map(|d| json!({ "details": d.details, "remote_items": d.remote_items }))
            .map_err(|e| e.to_string())
    }

    async fn delete(&self, capture_id: &str, also_remote: bool) -> Result<Value, CallError> {
        let mut params = json!({ "capture_id": capture_id });
        if also_remote {
            params["also_delete_remote"] = json!(true);
        }
        Backend::call_coded(&self.0, "delete_capture", params).await
    }
}

/// The remote copies of one capture, as the dialog lists them. The copies are the daemon's
/// structured items (`remote_items`); the words are made when the dialog is drawn.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeletePrompt {
    /// Copies the tick would delete.
    pub removable: Vec<Value>,
    /// Copies the tick cannot delete (OneDrive, a removed destination ...), with the reason.
    pub stays: Vec<Value>,
}

/// A copy in words, or a plain "a copy" when this version cannot word it.
fn item_words(item: &Value) -> String {
    rt::item_text(item).unwrap_or_else(|| tr!("a copy"))
}

impl DeletePrompt {
    /// Reads the description of `describe_delete(.., true)`.
    pub fn from_details(description: &Value) -> DeletePrompt {
        let items = description.get("remote_items").unwrap_or(&Value::Null);
        DeletePrompt {
            removable: rt::items_of(items, "delete").into_iter().cloned().collect(),
            stays: rt::items_of(items, "left").into_iter().cloned().collect(),
        }
    }

    /// Whether there is anything the tick would delete. Without it the dialog is today's.
    pub fn offers_remote(&self) -> bool {
        !self.removable.is_empty()
    }

    /// The services behind the removable copies, in first-seen order: `Google Drive`, `YouTube`,
    /// `a destination folder`.
    pub fn services(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for item in &self.removable {
            let name = match item.get("form").and_then(Value::as_str) {
                Some("drive_file" | "drive_folder") => "Google Drive".to_owned(),
                Some("youtube") => "YouTube".to_owned(),
                Some("local_file" | "local_bundle") => tr!("a destination folder"),
                _ => tr!("the destination"),
            };
            if !out.contains(&name) {
                out.push(name);
            }
        }
        out
    }

    /// `Also delete the copies on Google Drive / YouTube (permanent)`.
    pub fn option_label(&self) -> Option<String> {
        self.offers_remote().then(|| {
            tr!(
                "Also delete the copies on {services} (permanent)",
                services = self.services().join(" / ")
            )
        })
    }

    /// The confirmation: today's, plus one row per online copy and the unticked-by-default
    /// option when there is something to delete online.
    pub fn dialog(&self, title: &str) -> DialogModel {
        let mut m = delete_dialog(title);
        if self.offers_remote() {
            m.summary = tr!(
                "It is removed from your library and moved to the Trash. Its copies online stay there unless you tick the box: deleting them cannot be undone ({services}).",
                services = self.services().join(", ")
            );
            for item in &self.removable {
                m.details
                    .push((tr_ctx!("dialog row", "Online copy"), item_words(item)));
            }
            m.option = self.option_label();
        }
        for item in &self.stays {
            let text = rt::left_text(item).unwrap_or_else(|| item_words(item));
            m.details
                .push((tr_ctx!("dialog row", "Stays online"), text));
        }
        m
    }
}

/// Asks the daemon what the remote copies are. When it cannot say (the capture is busy, the
/// daemon failed) the prompt is empty: the plain confirmation shows and `delete_capture` itself
/// reports the problem.
pub async fn prepare(api: &dyn DeleteApi, capture_id: &str) -> DeletePrompt {
    match api.describe(capture_id).await {
        Ok(details) => DeletePrompt::from_details(&details),
        Err(_) => DeletePrompt::default(),
    }
}

/// The user's answer as the `also_delete_remote` argument: `None` when they declined. The local
/// dialog reuses the checkbox's `ApproveForSession` answer for "approved with the box ticked".
pub fn also_remote(decision: Decision, prompt: &DeletePrompt) -> Option<bool> {
    match decision {
        Decision::Decline => None,
        Decision::Approve => Some(false),
        Decision::ApproveForSession => Some(prompt.offers_remote()),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Success,
    Error,
}

/// A toast in plain words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub level: Level,
    pub title: String,
    pub body: Option<String>,
    /// The capture left the library (refresh it).
    pub local_deleted: bool,
}

/// The English the daemon sent for a bucket (`deleted`, `left_in_place` ...): plain strings, or
/// `{ item, message }` objects. Used only when the result has no structured `items`.
fn strings(v: &Value, key: &str) -> Vec<String> {
    v.get(key)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|x| match x.as_str() {
                    Some(s) => s.to_owned(),
                    None => {
                        let item = x["item"]
                            .as_str()
                            .map_or_else(|| tr!("a copy"), str::to_owned);
                        match x["message"]
                            .as_str()
                            .or_else(|| x["reason"].as_str())
                            .filter(|m| !m.is_empty())
                        {
                            Some(m) => tr!("{item} ({reason})", item = item, reason = m),
                            None => item,
                        }
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The copies of one outcome (`deleted`, `already_gone`, `left`, `failed`, `not_tried`) in
/// words: from the structured `items` of the result when there are some (so the sentence
/// follows the language), else the daemon's English.
fn outcome_texts(remote: &Value, outcome: &str) -> Vec<String> {
    let Some(items) = remote.get("items").and_then(Value::as_array) else {
        let key = if outcome == "left" {
            "left_in_place"
        } else {
            outcome
        };
        return strings(remote, key);
    };
    items
        .iter()
        .filter(|i| i.get("outcome").and_then(Value::as_str) == Some(outcome))
        .map(|i| match outcome {
            "left" => rt::left_text(i).unwrap_or_else(|| item_words(i)),
            "failed" => match i["message"].as_str().filter(|m| !m.is_empty()) {
                // The provider's own words stay as they came.
                Some(m) => tr!("{item} ({reason})", item = item_words(i), reason = m),
                None => item_words(i),
            },
            _ => item_words(i),
        })
        .collect()
}

/// Calls `delete_capture` and words the result.
pub async fn finish(api: &dyn DeleteApi, capture_id: &str, also_remote: bool) -> Notice {
    notice(api.delete(capture_id, also_remote).await, also_remote)
}

/// The toast for a `delete_capture` result.
pub fn notice(result: Result<Value, CallError>, asked_remote: bool) -> Notice {
    match result {
        Ok(v) => {
            let remote = v.get("remote");
            let deleted = remote.map_or(0, |r| outcome_texts(r, "deleted").len());
            let gone = remote.map_or(0, |r| outcome_texts(r, "already_gone").len());
            let mut body = Vec::new();
            if deleted > 0 {
                body.push(trn!(
                    "Also deleted {n} online copy for good.",
                    "Also deleted {n} online copies for good.",
                    deleted + gone
                ));
            } else if asked_remote && gone > 0 {
                body.push(tr!("The online copies were already gone."));
            }
            if let Some(left) = remote
                .map(|r| outcome_texts(r, "left"))
                .filter(|l| !l.is_empty())
            {
                body.push(tr!("Still online: {items}.", items = left.join("; ")));
            }
            let warning = remote.unwrap_or(&v);
            if warning["warning_kind"] == "outcome_not_saved" {
                body.push(tr!(
                    "The outcome was not saved in the manifest: {detail}",
                    detail = warning["warning_detail"].as_str().unwrap_or_default()
                ));
            } else if let Some(w) = warning
                .get("warning")
                .or_else(|| v.get("warning"))
                .and_then(Value::as_str)
            {
                body.push(w.to_owned());
            }
            Notice {
                level: Level::Success,
                title: tr!("Capture deleted"),
                body: (!body.is_empty()).then(|| body.join(" ")),
                local_deleted: true,
            }
        }
        Err(e) => failure(&e),
    }
}

fn failure(e: &CallError) -> Notice {
    let remote = e.data.as_ref().and_then(|d| d.get("remote"));
    let Some(remote) = remote else {
        // Not a remote problem (busy, not found, ...): the daemon's own words.
        return Notice {
            level: Level::Error,
            title: tr!("Couldn\u{2019}t delete the capture"),
            body: Some(crate::daemon_text::call_error_text(e)),
            local_deleted: false,
        };
    };
    let mut done = outcome_texts(remote, "deleted");
    done.extend(outcome_texts(remote, "already_gone"));
    let mut not_done = outcome_texts(remote, "failed");
    not_done.extend(outcome_texts(remote, "not_tried"));
    let mut body = vec![tr!("The capture is still in your library.")];
    if !done.is_empty() {
        body.push(tr!("Deleted online: {items}.", items = done.join("; ")));
    }
    if !not_done.is_empty() {
        body.push(tr!(
            "Not deleted online: {items}.",
            items = not_done.join("; ")
        ));
    }
    body.push(match e.code {
        Some(ErrorCode::AuthExpired) => {
            tr!("The account needs to sign in again: reconnect it in Settings, then delete again.")
        }
        Some(ErrorCode::Network) => tr!("Check your connection, then delete again."),
        _ => tr!("Fix the problem and delete again; only the copies that are left are tried."),
    });
    Notice {
        level: Level::Error,
        title: tr!("Some online copies weren\u{2019}t deleted"),
        body: Some(body.join(" ")),
        local_deleted: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cm_i18n::{with_language, Lang};
    use std::sync::Mutex;

    /// What the daemon says for a capture with a Drive file, a Drive folder, a YouTube video and
    /// a OneDrive copy it cannot delete.
    fn details() -> Value {
        let drive = json!({ "type": "service", "service": "Google Drive", "account": "me", "name": "Drive" });
        json!({
            "details": {
                "capture": "Demo", "id": "rec_1", "kind": "recording", "size": "9.0 MB",
                "permanent": "Deleting remote copies is permanent", "result": "x",
            },
            // Out of order on purpose: the number decides.
            "remote_items": [
                { "state": "delete", "n": 3, "form": "drive_folder", "path": "Demo.cmrec", "place": drive },
                { "state": "left", "n": 1, "form": "copy", "what": "export", "path": "Demo.mp4",
                  "place": { "type": "service", "service": "OneDrive", "name": "OD" },
                  "reason": { "id": "provider_unavailable", "service": "OneDrive" } },
                { "state": "delete", "n": 1, "form": "drive_file", "path": "Demo.mp4", "place": drive },
                { "state": "delete", "n": 2, "form": "youtube", "title": "Demo",
                  "url": "https://youtu.be/x", "channel": "Me" },
            ],
        })
    }

    struct Fake {
        describe: Result<Value, String>,
        delete: Mutex<Option<Result<Value, CallError>>>,
        calls: Mutex<Vec<(String, bool)>>,
    }

    impl Fake {
        fn new(describe: Result<Value, String>, delete: Result<Value, CallError>) -> Fake {
            Fake {
                describe,
                delete: Mutex::new(Some(delete)),
                calls: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl DeleteApi for Fake {
        async fn describe(&self, _: &str) -> Result<Value, String> {
            self.describe.clone()
        }
        async fn delete(&self, id: &str, also: bool) -> Result<Value, CallError> {
            self.calls.lock().unwrap().push((id.to_owned(), also));
            self.delete.lock().unwrap().take().unwrap()
        }
    }

    fn err(code: ErrorCode, data: Option<Value>) -> CallError {
        CallError {
            code: Some(code),
            permission: None,
            message: "daemon words".to_owned(),
            data,
        }
    }

    /// LIB-1.AC4: the dialog lists each online copy and the box is off by default; copies the
    /// tick cannot delete are listed as staying.
    #[test]
    fn the_dialog_lists_the_online_copies_and_offers_the_box() {
        let p = DeletePrompt::from_details(&details());
        assert_eq!(p.removable.len(), 3);
        assert!(
            item_words(&p.removable[2]).contains("folder"),
            "numeric order, not text order"
        );
        assert_eq!(
            item_words(&p.removable[0]),
            "Google Drive: the file Demo.mp4 (Google Drive (me), destination \"Drive\")"
        );
        assert_eq!(
            item_words(&p.removable[1]),
            "YouTube: the video \"Demo\" https://youtu.be/x (channel \"Me\")"
        );
        assert_eq!(p.stays.len(), 1);
        assert_eq!(
            rt::left_text(&p.stays[0]).unwrap(),
            "export Demo.mp4 in OneDrive, destination \"OD\" (deleting on OneDrive is not available yet)"
        );
        assert_eq!(p.services(), ["Google Drive", "YouTube"]);
        assert_eq!(
            p.option_label().unwrap(),
            "Also delete the copies on Google Drive / YouTube (permanent)"
        );
        let m = p.dialog("Demo");
        assert_eq!(m.option, p.option_label());
        assert!(!m.allow_session);
        assert!(m.summary.contains("stay"), "{}", m.summary);
        let online: Vec<_> = m
            .details
            .iter()
            .filter(|(k, _)| k == "Online copy")
            .collect();
        assert_eq!(online.len(), 3);
        assert!(m.details.iter().any(|(k, _)| k == "Stays online"));
        assert_eq!(m.primary_label, "Delete capture");
    }

    /// A capture with no online copy keeps today's single confirmation.
    #[test]
    fn no_online_copy_means_the_plain_dialog() {
        let none = json!({ "details": { "capture": "Demo" }, "remote_items": [] });
        let p = DeletePrompt::from_details(&none);
        assert!(!p.offers_remote());
        assert_eq!(p.dialog("Demo"), delete_dialog("Demo"));
        // Only copies that cannot be deleted: no box, but the user is told they stay.
        let left = json!({ "remote_items": [
            { "state": "left", "n": 1, "form": "copy", "what": "export", "path": "a",
              "destination_name": "X", "reason": { "id": "destination_removed" } } ] });
        let p = DeletePrompt::from_details(&left);
        let m = p.dialog("Demo");
        assert_eq!(m.option, None);
        assert_eq!(
            m.details.last().unwrap(),
            &(
                "Stays online".to_owned(),
                "export a at \"X\" (that destination was removed from Settings)".to_owned()
            )
        );
    }

    #[test]
    fn the_answer_maps_to_the_argument() {
        let with = DeletePrompt::from_details(&details());
        let without = DeletePrompt::default();
        assert_eq!(also_remote(Decision::Decline, &with), None);
        assert_eq!(also_remote(Decision::Approve, &with), Some(false));
        assert_eq!(also_remote(Decision::ApproveForSession, &with), Some(true));
        // A box that was never shown cannot be ticked.
        assert_eq!(
            also_remote(Decision::ApproveForSession, &without),
            Some(false)
        );
    }

    #[tokio::test]
    async fn prepare_falls_back_to_the_plain_prompt_when_the_daemon_cannot_say() {
        let api = Fake::new(Err("busy".into()), Ok(json!({})));
        assert_eq!(prepare(&api, "rec_1").await, DeletePrompt::default());
        let api = Fake::new(Ok(details()), Ok(json!({})));
        assert!(prepare(&api, "rec_1").await.offers_remote());
    }

    /// Unticked: the daemon is asked without `also_delete_remote`, the copies stay.
    #[tokio::test]
    async fn unticked_deletes_only_the_local_capture() {
        let api = Fake::new(Ok(details()), Ok(json!({ "moved_to_trash": true })));
        let prompt = prepare(&api, "rec_1").await;
        let also = also_remote(Decision::Approve, &prompt).unwrap();
        let n = finish(&api, "rec_1", also).await;
        assert_eq!(
            api.calls.lock().unwrap().as_slice(),
            [("rec_1".to_owned(), false)]
        );
        assert_eq!(n.title, "Capture deleted");
        assert_eq!(n.body, None);
        assert!(n.local_deleted);
    }

    #[tokio::test]
    async fn ticked_deletes_the_copies_and_says_so() {
        let ok = json!({
            "moved_to_trash": true,
            "remote": { "deleted": ["a", "b"], "already_gone": ["c"], "left_in_place": [
                { "item": "OneDrive file", "reason": "not available yet" } ],
                "failed": [], "not_tried": [] },
        });
        let api = Fake::new(Ok(details()), Ok(ok));
        let prompt = prepare(&api, "rec_1").await;
        let also = also_remote(Decision::ApproveForSession, &prompt).unwrap();
        let n = finish(&api, "rec_1", also).await;
        assert_eq!(
            api.calls.lock().unwrap().as_slice(),
            [("rec_1".to_owned(), true)]
        );
        assert_eq!(n.level, Level::Success);
        let body = n.body.unwrap();
        assert!(
            body.contains("Also deleted 3 online copies for good"),
            "{body}"
        );
        assert!(body.contains("Still online: OneDrive file"), "{body}");
    }

    /// A revoked token: the capture stays, the toast says what was and was not deleted and what
    /// to do.
    #[tokio::test]
    async fn a_partial_failure_keeps_the_capture_and_says_what_is_left() {
        let data = json!({
            "capture_id": "rec_1", "kept_local": true,
            "remote": {
                "deleted": ["YouTube: the video \"Demo\""], "already_gone": [],
                "left_in_place": [],
                "failed": [{ "item": "Google Drive: the file Demo.mp4", "error": "auth_expired",
                             "message": "Google sign-in expired" }],
                "not_tried": ["Google Drive: the folder Demo.cmrec"],
            },
        });
        let api = Fake::new(Ok(details()), Err(err(ErrorCode::AuthExpired, Some(data))));
        let n = finish(&api, "rec_1", true).await;
        assert_eq!(n.level, Level::Error);
        assert!(!n.local_deleted);
        assert_eq!(n.title, "Some online copies weren\u{2019}t deleted");
        let b = n.body.unwrap();
        assert!(
            b.starts_with("The capture is still in your library."),
            "{b}"
        );
        assert!(b.contains("Deleted online: YouTube: the video"), "{b}");
        assert!(
            b.contains("Google Drive: the file Demo.mp4 (Google sign-in expired)"),
            "{b}"
        );
        assert!(b.contains("the folder Demo.cmrec"), "{b}");
        assert!(b.contains("reconnect it in Settings"), "{b}");
    }

    /// The result of a ticked delete carries the copies as structured `items`: the toast words
    /// them in the chosen language (English as the daemon's text, Vietnamese with the glossary).
    #[test]
    fn the_result_toast_is_worded_from_the_items() {
        let drive = json!({ "type": "service", "service": "Google Drive", "name": "Drive" });
        let items = json!([
            { "form": "drive_file", "path": "a.mp4", "place": drive, "outcome": "deleted" },
            { "form": "copy", "what": "export", "path": "b.mp4", "destination_name": "Old disk",
              "outcome": "left", "reason": { "id": "destination_removed" } },
        ]);
        let ok = json!({ "remote": { "deleted": ["English a"], "already_gone": [],
            "left_in_place": [ { "item": "English b", "reason": "English why" } ],
            "failed": [], "not_tried": [], "items": items } });
        let n = notice(Ok(ok.clone()), true);
        let body = n.body.unwrap();
        assert!(
            body.contains(
                "Still online: export b.mp4 at \"Old disk\" (that destination was removed from Settings)."
            ),
            "{body}"
        );
        with_language(Lang::Vi, || {
            let body = notice(Ok(ok.clone()), true).body.unwrap();
            assert!(
                body.contains("bản xuất b.mp4 tại “Old disk” (điểm lưu đó đã bị gỡ khỏi Cài đặt)"),
                "{body}"
            );
            assert!(!body.contains("English"), "{body}");
        });
        // A partial failure: the failed copy keeps the provider's own words after the item.
        let data = json!({ "capture_id": "rec_1", "kept_local": true, "remote": {
            "deleted": [], "already_gone": [], "left_in_place": [], "failed": [], "not_tried": [],
            "items": [
                { "form": "drive_file", "path": "a.mp4", "place": drive, "outcome": "failed",
                  "error": "auth_expired", "message": "Google sign-in expired" },
                { "form": "local_file", "path": "/nas/a.mp4", "outcome": "not_tried" } ] } });
        let n = notice(Err(err(ErrorCode::AuthExpired, Some(data.clone()))), true);
        let b = n.body.unwrap();
        assert!(
            b.contains("Google Drive: the file a.mp4 (Google Drive, destination \"Drive\") (Google sign-in expired)"),
            "{b}"
        );
        assert!(b.contains("Folder: the file /nas/a.mp4"), "{b}");
        with_language(Lang::Vi, || {
            let b = notice(Err(err(ErrorCode::AuthExpired, Some(data.clone()))), true)
                .body
                .unwrap();
            assert!(b.contains("Google Drive: tệp a.mp4"), "{b}");
            assert!(b.contains("(Google sign-in expired)"), "{b}");
            assert!(b.contains("Thư mục: tệp /nas/a.mp4"), "{b}");
        });
        // The manifest warning is a stable kind with its detail.
        let warn = json!({ "remote": { "deleted": [], "already_gone": [], "items": [],
            "warning": "English", "warning_kind": "outcome_not_saved", "warning_detail": "disk full" } });
        let b = notice(Ok(warn.clone()), true).body.unwrap();
        assert_eq!(b, "The outcome was not saved in the manifest: disk full");
        with_language(Lang::Vi, || {
            let b = notice(Ok(warn.clone()), true).body.unwrap();
            assert_eq!(b, "Chưa lưu được kết quả vào manifest: disk full");
        });
    }

    /// The dialog rows follow the language: Vietnamese rows for the copies and the ones that stay.
    #[test]
    fn the_dialog_rows_follow_the_language() {
        let p = DeletePrompt::from_details(&details());
        with_language(Lang::Vi, || {
            let m = p.dialog("Demo");
            let online: Vec<&String> = m
                .details
                .iter()
                .filter(|(k, _)| k == "Bản sao trực tuyến")
                .map(|(_, v)| v)
                .collect();
            assert_eq!(online.len(), 3, "{:?}", m.details);
            assert_eq!(
                online[0],
                "Google Drive: tệp Demo.mp4 (Google Drive (me), điểm lưu “Drive”)"
            );
            assert!(
                online[1].starts_with("YouTube: video “Demo”"),
                "{}",
                online[1]
            );
            let stays = m.details.last().unwrap();
            assert_eq!(
                stays.1,
                "bản xuất Demo.mp4 trong OneDrive, điểm lưu “OD” (chưa hỗ trợ xoá trên OneDrive)"
            );
        });
    }

    #[test]
    fn other_failures_use_the_daemons_words() {
        let n = notice(Err(err(ErrorCode::Busy, None)), false);
        assert_eq!(n.title, "Couldn\u{2019}t delete the capture");
        assert_eq!(n.body.as_deref(), Some("daemon words"));
        assert!(!n.local_deleted);
        let net = notice(
            Err(err(
                ErrorCode::Network,
                Some(json!({ "remote": { "failed": [{ "item": "x", "message": "" }] } })),
            )),
            true,
        );
        assert!(net.body.unwrap().contains("Check your connection"));
    }

    /// I18N P2: the online-copy count is one plural message (no `if n == 1`), English as before.
    #[test]
    fn the_copy_count_is_a_plural_message() {
        let one = notice(
            Ok(json!({ "remote": { "deleted": ["a"], "already_gone": [] } })),
            true,
        );
        assert_eq!(
            one.body.as_deref(),
            Some("Also deleted 1 online copy for good.")
        );
        let two = notice(
            Ok(json!({ "remote": { "deleted": ["a"], "already_gone": ["b"] } })),
            true,
        );
        assert_eq!(
            two.body.as_deref(),
            Some("Also deleted 2 online copies for good.")
        );
    }
}
