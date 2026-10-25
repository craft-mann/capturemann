//! The upload records of a capture (STO-4.AC3).
//!
//! When an `upload` job of a capture finishes, one record per part that was sent is added to
//! `uploads[]` in the capture's `manifest.json` and the index row is refreshed. Records are
//! bookkeeping about copies, not edits: no revision is added, so there is nothing to undo.
//! `list_library` items and `get_capture` return them as `uploads`.
//!
//! A record is `{ destination_id, destination, what, remote_id, path, folder_id?, bytes, files,
//! share_link, at }`: `what` is `export` (the exported video, `remote_id` is the provider's id of
//! that file, `path` its folder and name below the destination root) or `original` (the bundle,
//! `remote_id` and `path` are its folder below the destination root, and on Google Drive
//! `folder_id` is the id of that folder, which is what a later delete needs). Records from before
//! `path` and `folder_id` existed lack them; the remote delete says so instead of guessing.

use std::path::Path;

use cm_storage::Destination;
use serde_json::{json, Value};

use crate::daemon::lock_library;

/// Which part of an upload job a file belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Part {
    /// A loose file (`upload { file }`): not a capture, never recorded.
    File,
    /// The newest export of the capture.
    Export,
    /// A file of the original bundle; `root` is the bundle folder at the destination.
    Bundle { root: String },
}

/// What the provider reported for one sent file.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Sent {
    pub remote_id: String,
    /// Folder and name below the destination root, as the user would read them.
    pub path: Option<String>,
    pub share_link: Option<String>,
    pub bytes: u64,
    /// Size and modification time of the source file just before it was sent (STO-5): what the
    /// end of the job compares with to find a bundle file that changed while it uploaded.
    pub fp: Option<String>,
}

impl Sent {
    pub(crate) fn to_json(&self) -> Value {
        json!({
            "remote_id": self.remote_id, "path": self.path,
            "share_link": self.share_link, "bytes": self.bytes, "fp": self.fp,
        })
    }

    pub(crate) fn from_json(v: &Value) -> Option<Sent> {
        Some(Sent {
            remote_id: v.get("remote_id")?.as_str()?.to_owned(),
            path: v.get("path").and_then(Value::as_str).map(str::to_owned),
            share_link: v
                .get("share_link")
                .and_then(Value::as_str)
                .map(str::to_owned),
            bytes: v.get("bytes").and_then(Value::as_u64).unwrap_or(0),
            fp: v.get("fp").and_then(Value::as_str).map(str::to_owned),
        })
    }
}

/// The records for a finished job: one for the export, one for the bundle (if sent).
/// `bundle_folder_id` is the provider's id of the folder the bundle copy claimed, when it has ids.
pub(crate) fn build(
    dest: &Destination,
    parts: &[(Part, Sent)],
    at: &str,
    bundle_folder_id: Option<&str>,
) -> Vec<Value> {
    let mut out = Vec::new();
    let base = |what: &str,
                remote_id: &str,
                path: Option<&str>,
                bytes: u64,
                files: usize,
                link: &Option<String>| {
        json!({
            "destination_id": dest.id,
            "destination": dest.name,
            "what": what,
            "remote_id": remote_id,
            "path": path,
            "bytes": bytes,
            "files": files,
            "share_link": link,
            "at": at,
        })
    };
    if let Some((_, s)) = parts.iter().find(|(p, _)| *p == Part::Export) {
        out.push(base(
            "export",
            &s.remote_id,
            s.path.as_deref(),
            s.bytes,
            1,
            &s.share_link,
        ));
    }
    let bundle: Vec<&(Part, Sent)> = parts
        .iter()
        .filter(|(p, _)| matches!(p, Part::Bundle { .. }))
        .collect();
    if let Some((Part::Bundle { root }, first)) = bundle.first().map(|b| (&b.0, &b.1)) {
        let bytes = bundle.iter().map(|(_, s)| s.bytes).sum();
        let mut rec = base(
            "original",
            root,
            Some(root),
            bytes,
            bundle.len(),
            // A share link of one file of the bundle would be misleading for the whole copy.
            &if bundle.len() == 1 {
                first.share_link.clone()
            } else {
                None
            },
        );
        if let Some(id) = bundle_folder_id {
            rec["folder_id"] = json!(id);
        }
        out.push(rec);
    }
    out
}

/// Writes the records into the capture's manifest and refreshes the index row, under the
/// capture's lock: the same lock `annotate`, `undo` and the other writers of the manifest take,
/// so this rewrite of `manifest.json` (a screenshot's is rewritten by the annotation editor as a
/// whole) can never interleave with theirs and drop a record or an edit. The bundle may have
/// been deleted meanwhile; that is an error for the caller to report, not to retry.
pub(crate) async fn save(
    library: &std::sync::Arc<std::sync::Mutex<cm_library::Library>>,
    locks: &crate::daemon::LockMap,
    capture_id: &str,
    records: Vec<Value>,
) -> Result<(), cm_core::CmError> {
    if records.is_empty() {
        return Ok(());
    }
    let _serial = locks.lock(capture_id).await;
    let (library, id) = (library.clone(), capture_id.to_owned());
    tokio::task::spawn_blocking(move || save_blocking(&library, &id, records))
        .await
        .map_err(|e| cm_core::CmError::internal(format!("worker failed: {e}")))?
}

fn save_blocking(
    library: &std::sync::Mutex<cm_library::Library>,
    capture_id: &str,
    records: Vec<Value>,
) -> Result<(), cm_core::CmError> {
    let dir: std::path::PathBuf = {
        let lib = lock_library(library);
        let r = lib.get(capture_id)?;
        lib.bundle_path(&r)
    };
    write(&dir, records)?;
    lock_library(library).index_dir(&dir)?;
    Ok(())
}

fn write(dir: &Path, records: Vec<Value>) -> Result<(), cm_core::CmError> {
    for r in records {
        cm_library::record_upload(dir, r)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cm_storage::{AutoUpload, Kind, What};

    fn dest() -> Destination {
        Destination {
            id: "d1".into(),
            name: "NAS".into(),
            kind: Kind::Local,
            path: "/x".into(),
            account: None,
            folder_pattern: "a".into(),
            what: What::Both,
            auto_upload: AutoUpload::Off,
            default: false,
        }
    }

    fn sent(id: &str, bytes: u64, link: Option<&str>) -> Sent {
        Sent {
            remote_id: id.into(),
            path: Some(format!("A/{id}")),
            share_link: link.map(str::to_owned),
            bytes,
            fp: None,
        }
    }

    #[test]
    fn export_and_bundle_get_one_record_each() {
        let root = "CaptureMann/2026/10/Lesson/Lesson.cmrec".to_owned();
        let parts = vec![
            (
                Part::Export,
                sent("/nas/Lesson - quick.mp4", 10, Some("https://l")),
            ),
            (Part::Bundle { root: root.clone() }, sent("/nas/a", 3, None)),
            (Part::Bundle { root: root.clone() }, sent("/nas/b", 4, None)),
        ];
        let r = build(&dest(), &parts, "2026-10-23T10:00:00Z", Some("fld1"));
        assert_eq!(r.len(), 2);
        assert_eq!(r[0]["what"], "export");
        assert_eq!(r[0]["share_link"], "https://l");
        assert_eq!(r[0]["destination"], "NAS");
        assert_eq!(r[1]["what"], "original");
        assert_eq!(r[1]["remote_id"], root);
        assert_eq!(r[1]["bytes"], 7);
        assert_eq!(r[1]["files"], 2);
        assert!(r[1]["share_link"].is_null());
        assert_eq!(r[0]["path"], "A//nas/Lesson - quick.mp4");
        assert_eq!(r[1]["path"], root);
        assert_eq!(r[1]["folder_id"], "fld1");
        assert!(r[0].get("folder_id").is_none());
    }

    #[test]
    fn a_loose_file_has_no_record() {
        let parts = vec![(Part::File, sent("/nas/x.png", 1, None))];
        assert!(build(&dest(), &parts, "t", None).is_empty());
    }

    #[test]
    fn sent_round_trips_through_json() {
        let s = sent("r", 5, Some("l"));
        assert_eq!(Sent::from_json(&s.to_json()), Some(s));
    }
}
