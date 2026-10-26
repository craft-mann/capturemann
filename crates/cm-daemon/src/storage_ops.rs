//! `list_destinations` and `upload` (STO-1, STO-3, STO-5), and the `upload` job.
//!
//! Destinations come from `destinations.json` ([`cm_storage::DestinationStore`]); the app writes
//! it, agents only read it. `upload { capture_id | file, destination_id?, folder?, what? }`
//! validates, then enqueues a job of type `upload` and returns its id. Free tier: it copies to a
//! place the user set up, nothing is deleted or published.
//!
//! The job copies one file at a time through the destination's provider (the local one today),
//! reports progress by bytes, and saves a checkpoint after every file, so a restart skips what
//! was copied. When the destination is offline it waits ("Destination offline", checked every
//! [`OFFLINE_POLL`]) instead of failing (STO-1.AC2).
//!
//! # Bundle copies claim a fresh folder
//!
//! An original-bundle copy goes into a folder of its own, `Title.cmrec`, else `Title (2).cmrec`,
//! and so on, so two copies never mix their files. The provider claims it
//! ([`StorageProvider::claim_bundle_folder`]): the local one creates the directory, Google Drive
//! checks the siblings, creates the folder and checks again that it is the oldest of that name.
//! The claim (name, and on Drive the folder id) is saved in the job checkpoint before the first
//! file is sent, so a restart goes on in the same folder instead of claiming another one; the
//! upload record keeps the Drive folder id for a later delete.
//!
//! # A bundle that changes while it uploads
//!
//! The checkpoint keeps, for every file sent, the size and modification time the source had just
//! before it was sent. After the last file, an original-bundle job looks at the bundle again:
//! a file that changed, and a file that appeared (never `recording.lock`, temp files, or anything
//! new below `exports/`), is sent into the same claimed folder with
//! [`StorageProvider::replace`] (local: atomic rename inside the bundle folder; Google Drive:
//! the content of the file with the recorded id, or the file of that name). At most three
//! rounds; then the job result's `notes` say the bundle kept changing. A file that was removed
//! is noted and left at the destination. `resent` in the job result lists what was sent again.
//! Files sent before a restart are matched by their place at the destination, so a file that
//! appeared meanwhile does not shift them. A destination without `replace` (OneDrive, Dropbox)
//! gets a note instead of a failed job. A loose file or an export is never re-sent.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use cm_core::{Actor, CmError};
use cm_jobs::{JobContext, JobHandler, JOB_UPLOAD};
use cm_library::manifest::CaptureKind;
use cm_storage::pattern::{expand_pattern, DateParts};
use cm_storage::{
    Destination, DestinationStore, FolderClaim, LocalProvider, Progress, StorageProvider, Target,
    What,
};
use serde::Serialize;
use serde_json::{json, Value};
use time::OffsetDateTime;

use crate::action::{Action, Fields};
use crate::daemon::{blocking, lock_library, Daemon, Inner};
use crate::error::DaemonError;
use crate::upload_records::{self, Part, Sent};
use cm_jobs::NewJob;

/// The action names of this module.
pub const STORAGE_METHODS: &[&str] = &["list_destinations", "upload"];

/// How often a job checks whether an offline destination is back (STO-1.AC2).
pub const OFFLINE_POLL: Duration = Duration::from_secs(30);

pub fn is_storage_method(name: &str) -> bool {
    STORAGE_METHODS.contains(&name)
}

/// `upload { capture_id | file, destination_id?, folder?, what?, request_id? }`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UploadParams {
    pub capture_id: Option<String>,
    pub file: Option<String>,
    pub destination_id: Option<String>,
    /// Replaces the destination's folder pattern for this upload.
    pub folder: Option<String>,
    /// What to send; the destination's own choice when absent.
    pub what: Option<What>,
    pub request_id: Option<String>,
}

pub(crate) fn parse_upload(f: &Fields<'_>) -> Result<Action, CmError> {
    f.only(&[
        "capture_id",
        "file",
        "destination_id",
        "folder",
        "what",
        "request_id",
    ])?;
    let capture_id = f.str_opt("capture_id")?;
    let file = f.str_opt("file")?;
    match (&capture_id, &file) {
        (Some(_), Some(_)) => {
            return Err(CmError::invalid_field(
                "capture_id",
                "give either `capture_id` or `file`, not both",
            ))
        }
        (None, None) => {
            return Err(CmError::invalid_field(
                "capture_id",
                "give `capture_id` or `file`",
            ))
        }
        _ => {}
    }
    let what = match f.str_opt("what")? {
        None => None,
        Some(s) => Some(What::parse(&s).ok_or_else(|| {
            CmError::invalid_field("what", "must be `export`, `original` or `both`")
        })?),
    };
    if what.is_some() && file.is_some() {
        return Err(CmError::invalid_field(
            "what",
            "applies to a capture, not to a file",
        ));
    }
    let folder = f.str_opt("folder")?;
    if folder.as_deref().is_some_and(|s| s.trim().is_empty()) {
        return Err(CmError::invalid_field("folder", "must not be empty"));
    }
    Ok(Action::Upload(UploadParams {
        capture_id,
        file,
        destination_id: f.str_opt("destination_id")?,
        folder,
        what,
        request_id: f.request_id()?,
    }))
}

pub(crate) fn summary(p: &UploadParams) -> String {
    let what = p.capture_id.as_deref().or(p.file.as_deref()).unwrap_or("?");
    match &p.destination_id {
        Some(d) => format!("Upload {what} to `{d}`"),
        None => format!("Upload {what} to the default destination"),
    }
}

fn date_parts(t: OffsetDateTime) -> DateParts {
    DateParts {
        year: t.year(),
        month: u8::from(t.month()),
        day: t.day(),
    }
}

fn destination_json(d: &Destination) -> Value {
    let mut v = json!({
        "id": d.id,
        "name": d.name,
        "kind": d.kind,
        "path": d.path,
        "folder_pattern": d.folder_pattern,
        "what": d.what,
        "auto_upload": d.auto_upload.as_str(),
        "default": d.default,
        // A cloud destination is "online" as far as we know; a lost connection shows up as an
        // upload job that fails with `auth_expired` or retries as `network`.
        "online": d.kind.is_cloud() || LocalProvider::new(&d.path).is_online(),
    });
    // The account label and folder, never a token or a key name.
    if let Some(a) = &d.account {
        v["account"] = json!({
            "label": a.label,
            "folder": a.folder_name,
            "link_mode": a.link_mode,
        });
        if a.app_folder {
            v["account"]["app_folder"] = json!(true);
        }
    }
    v
}

impl Inner {
    fn destination_store(&self) -> Option<&DestinationStore> {
        self.destinations.as_ref()
    }

    /// `list_destinations`.
    pub(crate) async fn list_destinations(&self) -> Result<Value, DaemonError> {
        let Some(store) = self.destination_store().cloned() else {
            return Ok(json!({ "destinations": [] }));
        };
        let list = blocking(move || {
            Ok(store
                .list()?
                .iter()
                .map(destination_json)
                .collect::<Vec<_>>())
        })
        .await?;
        Ok(json!({ "destinations": list }))
    }

    /// `upload`: validates, then enqueues the job.
    pub(crate) async fn upload(
        &self,
        actor: &Actor,
        p: UploadParams,
    ) -> Result<Value, DaemonError> {
        let store = self.destination_store().cloned().ok_or_else(|| {
            DaemonError::from(CmError::not_found(
                "no destination is set up; add one in Settings → Destinations",
            ))
        })?;
        let queue = self.job_queue()?.queue.clone();
        if !queue.has_handler(JOB_UPLOAD) {
            return Err(DaemonError::NotImplemented(
                "upload is not available: this daemon has no uploader registered".to_owned(),
            ));
        }
        let library = self.library.clone();
        let by = actor.to_string();
        let job = blocking(move || {
            let dest = store.resolve(p.destination_id.as_deref())?;
            let mut inputs = json!({
                "destination_id": dest.id,
                "folder": p.folder,
                "by": by,
            });
            let obj = inputs.as_object_mut().expect("object");
            if let Some(file) = &p.file {
                let path = PathBuf::from(file);
                if !path.is_file() {
                    return Err(CmError::invalid_field(
                        "file",
                        format!("`{file}` is not a file"),
                    ));
                }
                obj.insert("file".into(), json!(file));
            } else if let Some(id) = &p.capture_id {
                let (record, dir) = {
                    let lib = lock_library(&library);
                    let r = lib.get(id)?;
                    let d = lib.bundle_path(&r);
                    (r, d)
                };
                if !dir.is_dir() {
                    return Err(CmError::not_found(format!(
                        "the folder of `{id}` was moved or deleted"
                    )));
                }
                let what = p.what.unwrap_or(dest.what);
                if record.kind == CaptureKind::Screenshot && what != What::Original {
                    return Err(CmError::invalid_field(
                        "what",
                        "a screenshot has no export; use `original`",
                    ));
                }
                if what.wants_export() && latest_export(&dir)?.is_none() {
                    return Err(CmError::not_found(format!(
                        "`{id}` has no export yet; run `export_video` first"
                    )));
                }
                obj.insert("capture_id".into(), json!(id));
                obj.insert("what".into(), json!(what));
            }
            queue.enqueue(NewJob::new(JOB_UPLOAD, inputs))
        })
        .await?;
        Ok(json!({
            "job_id": job.id,
            "destination_id": job.inputs.get("destination_id"),
            "what": job.inputs.get("what"),
            "state": job.state.as_str(),
        }))
    }
}

/// The newest export file of a recording that still exists: `(absolute path, relative name)`.
pub(crate) fn latest_export(dir: &Path) -> Result<Option<PathBuf>, CmError> {
    let manifest = cm_library::read_manifest(dir)?;
    let exports = cm_library::exports_of(&manifest);
    // The newest still-existing file; an audio export (VOC-2) only when there is no other.
    let newest = |audio: bool| {
        exports
            .iter()
            .rev()
            .filter(|e| (e.get("kind").and_then(Value::as_str) == Some("audio")) == audio)
            .filter_map(|e| e.get("file").and_then(Value::as_str))
            // An entry must stay inside the bundle.
            .filter(|rel| !rel.starts_with('/') && !rel.split('/').any(|s| s == ".."))
            .map(|rel| dir.join(rel))
            .find(|p| p.is_file())
    };
    Ok(newest(false).or_else(|| newest(true)))
}

/// The relative names of [`walk`], for the remote delete.
pub(crate) fn walk_names(root: &Path) -> Result<Vec<String>, CmError> {
    Ok(walk(root)?.into_iter().map(|(_, rel)| rel).collect())
}

/// Every regular file of a folder, relative paths with `/`, sorted (so a restart sees the same
/// order). Symbolic links are skipped.
fn walk(root: &Path) -> Result<Vec<(PathBuf, String)>, CmError> {
    fn rec(dir: &Path, rel: &str, out: &mut Vec<(PathBuf, String)>) -> std::io::Result<()> {
        let mut entries: Vec<_> = std::fs::read_dir(dir)?.collect::<Result<_, _>>()?;
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let name = e.file_name().to_string_lossy().into_owned();
            let rel = if rel.is_empty() {
                name.clone()
            } else {
                format!("{rel}/{name}")
            };
            let ty = e.file_type()?;
            // Temp files of the library's atomic writes (`manifest.json.tmp`,
            // `render.png.tmp-123`) are not part of the capture and may vanish any moment.
            if name.ends_with(".tmp") || name.contains(".tmp-") {
                continue;
            }
            if ty.is_dir() {
                rec(&e.path(), &rel, out)?;
            } else if ty.is_file() {
                out.push((e.path(), rel));
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    rec(root, "", &mut out)
        .map_err(|e| CmError::internal(format!("cannot read {}: {e}", root.display())))?;
    Ok(out)
}

impl Daemon {
    /// Registers the executor of the job type `upload`. Call it next to
    /// [`Daemon::register_export_handler`], before [`Daemon::start_jobs`], in every process that
    /// owns a daemon. Errors with `not_implemented` without a job queue.
    pub fn register_upload_handler(&self) -> Result<(), DaemonError> {
        self.register_job_handler(JOB_UPLOAD, self.upload_handler(OFFLINE_POLL))
    }

    /// The handler, with a custom offline check interval (tests).
    pub fn upload_handler(&self, offline_poll: Duration) -> Arc<dyn JobHandler> {
        Arc::new(UploadHandler {
            locks: self.inner.capture_locks.clone(),
            library: self.inner.library.clone(),
            store: self.inner.destinations.clone(),
            cloud: self.inner.cloud.clone(),
            offline_poll,
        })
    }
}

/// The job checkpoint: how many files are done, the claimed bundle folder, what the provider
/// said about the files sent, and (while a file is in flight) its provider resume state.
fn checkpoint(
    done: usize,
    claim: Option<(String, Option<String>)>,
    sent: &[Sent],
    resume: Option<(&str, &Value)>,
) -> Value {
    let mut v = json!({
        "done": done,
        "bundle_root": claim.as_ref().map(|c| c.0.clone()),
        "bundle_folder_id": claim.and_then(|c| c.1),
        "sent": sent.iter().map(Sent::to_json).collect::<Vec<_>>(),
    });
    if let Some((path, state)) = resume {
        // The file in flight is named by its place at the destination, not by its index: a file
        // that appeared in the bundle meanwhile moves the indexes of the plan (STO-5).
        v["resume_path"] = json!(path);
        v["resume"] = state.clone();
    }
    v
}

fn claim_of(c: &FolderClaim) -> Option<(String, Option<String>)> {
    Some((c.name.clone(), c.id.clone()))
}

struct UploadHandler {
    locks: Arc<crate::daemon::LockMap>,
    library: Arc<std::sync::Mutex<cm_library::Library>>,
    store: Option<DestinationStore>,
    cloud: crate::storage_cloud::CloudBackend,
    offline_poll: Duration,
}

/// The provider of one destination: a local folder (can be offline, claims bundle folders) or a
/// cloud account.
enum Backend {
    Local(LocalProvider),
    Cloud(Arc<dyn StorageProvider>),
}

impl Backend {
    fn local(&self) -> Option<&LocalProvider> {
        match self {
            Backend::Local(p) => Some(p),
            Backend::Cloud(_) => None,
        }
    }

    fn provider(&self) -> &dyn StorageProvider {
        match self {
            Backend::Local(p) => p,
            Backend::Cloud(p) => p.as_ref(),
        }
    }
}

/// `auth_expired` says which destination to reconnect; the job fails with that code (Retry
/// works after reconnecting) instead of looping.
fn explain_auth(dest: &Destination, e: CmError) -> CmError {
    match e {
        CmError::AuthExpired { message } => CmError::AuthExpired {
            message: format!(
                "Reconnect needed for `{}` (auth_expired): {message}",
                dest.name
            ),
        },
        other => other,
    }
}

/// One file to copy.
#[derive(Clone)]
struct Item {
    src: PathBuf,
    target: Target,
    bytes: u64,
    /// Which part of the job this file belongs to (the upload record, STO-4.AC3).
    part: crate::upload_records::Part,
}

/// The files of the job, in order, and the folder name a bundle copy claims (when it is one).
type Library = Arc<std::sync::Mutex<cm_library::Library>>;

/// Where a capture's files go below the destination root: the folder pattern (or the `folder`
/// of the request) expanded for the capture.
fn base_folder(
    inputs: &Value,
    dest: &Destination,
    record: &cm_library::index::CaptureRecord,
) -> String {
    match inputs.get("folder").and_then(Value::as_str) {
        Some(f) => f.to_owned(),
        None => expand_pattern(
            &dest.folder_pattern,
            date_parts(record.created),
            &record.title,
        ),
    }
}

/// `(base folder, bundle folder name)` of an original-bundle copy that still has to claim its
/// folder; `None` for a loose file or an export-only upload.
fn bundle_to_claim(
    library: &Library,
    job: &cm_jobs::Job,
    dest: &Destination,
) -> Result<Option<(String, String)>, CmError> {
    let inputs = &job.inputs;
    if inputs.get("file").is_some() {
        return Ok(None);
    }
    let Some(id) = inputs.get("capture_id").and_then(Value::as_str) else {
        return Ok(None);
    };
    let what = inputs
        .get("what")
        .and_then(|w| w.as_str())
        .and_then(What::parse)
        .unwrap_or(dest.what);
    if !what.wants_original() {
        return Ok(None);
    }
    let (record, dir) = {
        let lib = lock_library(library);
        let r = lib.get(id)?;
        let d = lib.bundle_path(&r);
        (r, d)
    };
    let name = dir
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "bundle".into());
    Ok(Some((base_folder(inputs, dest, &record), name)))
}

/// The files of a bundle as upload items below `root` (the claimed bundle folder at the
/// destination), in the order of [`walk`].
fn bundle_items(dir: &Path, root: &str) -> Result<Vec<Item>, CmError> {
    let mut items = Vec::new();
    for (src, rel) in walk(dir)? {
        let (folder, file_name) = match rel.rsplit_once('/') {
            Some((d, f)) => (format!("{root}/{d}"), f.to_owned()),
            None => (root.to_owned(), rel.clone()),
        };
        let bytes = std::fs::metadata(&src).map(|m| m.len()).unwrap_or(0);
        items.push(Item {
            src,
            target: Target { folder, file_name },
            bytes,
            part: crate::upload_records::Part::Bundle {
                root: root.to_owned(),
            },
        });
    }
    Ok(items)
}

/// Where a file goes at the destination: folder and name, as the user would read them.
fn place(item: &Item) -> String {
    format!("{}/{}", item.target.folder, item.target.file_name)
}

/// Size and modification time of a file, `None` when it is gone. STO-5 compares it with the
/// value taken just before the file was sent.
fn fingerprint(path: &Path) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    let ns = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    Some(format!("{}:{ns}", meta.len()))
}

/// How many times the end of an original-bundle job looks for changes and sends them again.
const MAX_RESEND_ROUNDS: usize = 3;

fn plan(
    library: &Library,
    job: &cm_jobs::Job,
    dest: &Destination,
    claimed: Option<String>,
) -> Result<(Vec<Item>, Option<String>), CmError> {
    let inputs = &job.inputs;
    let folder_override = inputs.get("folder").and_then(Value::as_str);
    let size = |p: &Path| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
    if let Some(file) = inputs.get("file").and_then(Value::as_str) {
        let src = PathBuf::from(file);
        let name = src
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .ok_or_else(|| CmError::invalid_input("the file has no name"))?;
        let stem = src
            .file_stem()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let folder = match folder_override {
            Some(f) => f.to_owned(),
            None => expand_pattern(
                &dest.folder_pattern,
                date_parts(OffsetDateTime::now_utc()),
                &stem,
            ),
        };
        let bytes = size(&src);
        return Ok((
            vec![Item {
                src,
                target: Target {
                    folder,
                    file_name: name,
                },
                bytes,
                part: crate::upload_records::Part::File,
            }],
            None,
        ));
    }
    let id = inputs
        .get("capture_id")
        .and_then(Value::as_str)
        .ok_or_else(|| CmError::internal("the upload job has neither a file nor a capture"))?;
    let what = inputs
        .get("what")
        .and_then(|w| w.as_str())
        .and_then(What::parse)
        .unwrap_or(dest.what);
    let (record, dir) = {
        let lib = lock_library(library);
        let r = lib.get(id)?;
        let d = lib.bundle_path(&r);
        (r, d)
    };
    if !dir.is_dir() {
        return Err(CmError::not_found(format!(
            "the folder of `{id}` was moved or deleted"
        )));
    }
    let base = base_folder(inputs, dest, &record);
    let mut items = Vec::new();
    if what.wants_export() {
        if let Some(src) = latest_export(&dir)? {
            let name = src
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let bytes = size(&src);
            items.push(Item {
                src,
                target: Target {
                    folder: base.clone(),
                    file_name: name,
                },
                bytes,
                part: crate::upload_records::Part::Export,
            });
        } else if !what.wants_original() {
            return Err(CmError::not_found(format!(
                "`{id}` has no export to upload"
            )));
        }
    }
    let mut root = None;
    if what.wants_original() {
        let bundle_name = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "bundle".into());
        // The handler claimed the folder before planning; a plan made without a claim (never in
        // the job) falls back to the bundle's own name.
        let taken = claimed.unwrap_or_else(|| bundle_name.clone());
        items.extend(bundle_items(&dir, &format!("{base}/{taken}"))?);
        root = Some(taken);
    }
    Ok((items, root))
}

#[async_trait]
impl JobHandler for UploadHandler {
    async fn run(&self, ctx: JobContext) -> Result<Value, CmError> {
        let store = self
            .store
            .clone()
            .ok_or_else(|| CmError::not_found("no destination is set up"))?;
        let dest_id = ctx
            .job
            .inputs
            .get("destination_id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let dest = {
            let s = store.clone();
            let id = dest_id.clone();
            tokio::task::spawn_blocking(move || s.get(&id))
                .await
                .map_err(|e| CmError::internal(format!("worker failed: {e}")))??
        };
        let backend = if dest.kind.is_cloud() {
            Backend::Cloud(self.cloud.provider(&dest)?)
        } else {
            Backend::Local(LocalProvider::new(&dest.path))
        };
        let mut cancel = ctx.cancel.clone();

        // Resume: the folder a bundle copy claimed and how many files were done.
        let done_before = ctx
            .job
            .checkpoint
            .as_ref()
            .and_then(|c| c.get("done"))
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize;
        let claimed = ctx
            .job
            .checkpoint
            .as_ref()
            .and_then(|c| c.get("bundle_root"))
            .and_then(Value::as_str)
            .map(|name| FolderClaim {
                name: name.to_owned(),
                id: ctx
                    .job
                    .checkpoint
                    .as_ref()
                    .and_then(|c| c.get("bundle_folder_id"))
                    .and_then(Value::as_str)
                    .map(str::to_owned),
            });

        // What the provider said about the files already sent (for the upload record).
        let mut sent_items: Vec<Sent> = ctx
            .job
            .checkpoint
            .as_ref()
            .and_then(|c| c.get("sent"))
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Sent::from_json).collect())
            .unwrap_or_default();
        sent_items.truncate(done_before);

        // Claiming a folder touches the destination: wait for it to be there first.
        self.wait_online(backend.local(), &ctx, &mut cancel, 0.0)
            .await?;
        let (job, dest2, library) = (ctx.job.clone(), dest.clone(), self.library.clone());
        let target = tokio::task::spawn_blocking(move || bundle_to_claim(&library, &job, &dest2))
            .await
            .map_err(|e| CmError::internal(format!("worker failed: {e}")))??;
        let claim: Option<FolderClaim> = match (target, claimed) {
            (None, _) => None,
            // A restart: go on in the folder the first attempt claimed (same id on Drive).
            (Some((base, _)), Some(c)) => Some(
                backend
                    .provider()
                    .reuse_bundle_folder(&base, &c)
                    .await
                    .map_err(|e| explain_auth(&dest, e))?,
            ),
            (Some((base, name)), None) => {
                let c = backend
                    .provider()
                    .claim_bundle_folder(&base, &name)
                    .await
                    .map_err(|e| explain_auth(&dest, e))?;
                // Remember it before the first file goes, so a restart does not claim another.
                ctx.reporter
                    .save_checkpoint(&checkpoint(0, claim_of(&c), &[], None))?;
                Some(c)
            }
        };
        let (job, dest2, library) = (ctx.job.clone(), dest.clone(), self.library.clone());
        let taken = claim.as_ref().map(|c| c.name.clone());
        let (mut items, _root) =
            tokio::task::spawn_blocking(move || plan(&library, &job, &dest2, taken))
                .await
                .map_err(|e| CmError::internal(format!("worker failed: {e}")))??;
        let claim_ref = claim.as_ref().map(|c| (c.name.clone(), c.id.clone()));

        // The files the checkpoint says were sent, matched by their place at the destination: a
        // file that appeared in the bundle meanwhile moves the indexes of the plan (STO-5).
        let mut sent_for: Vec<Option<Sent>> = vec![None; items.len()];
        for (k, s) in sent_items.iter().enumerate() {
            let at = match &s.path {
                Some(p) => items.iter().position(|i| place(i) == *p),
                None => (k < items.len()).then_some(k),
            };
            if let Some(i) = at {
                sent_for[i] = Some(s.clone());
            }
        }
        let total: u64 = items.iter().map(|i| i.bytes).sum::<u64>().max(1);
        let mut sent: u64 = items
            .iter()
            .zip(&sent_for)
            .filter(|(_, s)| s.is_some())
            .map(|(i, _)| i.bytes)
            .sum();
        let mut files = Vec::new();
        let mut share_link: Option<String> = None;
        let mut notes: Vec<String> = Vec::new();

        for n in 0..items.len() {
            if sent_for[n].is_some() {
                continue;
            }
            let item = &items[n];
            self.wait_online(
                backend.local(),
                &ctx,
                &mut cancel,
                sent as f64 / total as f64,
            )
            .await?;
            let flag = Arc::new(AtomicBool::new(false));
            let last = Arc::new(AtomicU64::new(0));
            let (reporter, base) = (ctx.reporter.clone(), sent);
            let (flag2, last2) = (flag.clone(), last.clone());
            let progress = Progress::new(
                move |done, _| {
                    // At most one database write per percent.
                    let permille = (base + done) * 100 / total;
                    if last2.swap(permille, Ordering::Relaxed) != permille {
                        reporter.report((base + done) as f64 / total as f64, None);
                    }
                },
                flag2,
            );
            // A cloud upload that was interrupted continues from its last confirmed chunk.
            let here = place(item);
            let resume = ctx
                .job
                .checkpoint
                .as_ref()
                .filter(|c| c.get("resume_path").and_then(Value::as_str) == Some(here.as_str()))
                .and_then(|c| c.get("resume").cloned())
                .map(cm_storage::ResumeState);
            let saver = ctx.reporter.clone();
            let (claim_now, sent_now, here2) =
                (claim_ref.clone(), cp_sent(&sent_for), here.clone());
            let progress = progress.with_resume_saver(move |state| {
                // The files sent so far stay in the checkpoint: the upload record is built
                // from them when the job ends.
                let _ = saver.save_checkpoint(&checkpoint(
                    sent_now.len(),
                    claim_now.clone(),
                    &sent_now,
                    Some((&here2, &state.0)),
                ));
            });
            // Taken before the first byte is read: a change after this is seen at the end.
            let fp = fingerprint(&item.src);
            let uploaded = cancellable(
                backend
                    .provider()
                    .upload(&item.src, &item.target, resume, progress),
                &mut cancel,
                &flag,
            )
            .await
            .map_err(|e| explain_auth(&dest, e))?;
            sent += item.bytes;
            share_link = uploaded.share_link.clone().or(share_link);
            if let Some(note) = &uploaded.note {
                notes.push(note.clone());
            }
            files.push(json!({ "remote_id": uploaded.remote_id, "bytes": uploaded.bytes }));
            sent_for[n] = Some(Sent {
                remote_id: uploaded.remote_id.clone(),
                path: Some(here),
                share_link: uploaded.share_link.clone(),
                bytes: uploaded.bytes,
                fp,
            });
            let now = cp_sent(&sent_for);
            ctx.reporter
                .save_checkpoint(&checkpoint(now.len(), claim_ref.clone(), &now, None))?;
            ctx.reporter.report(sent as f64 / total as f64, None);
        }

        // STO-5: an original-bundle copy whose files changed while it was sent is brought up to
        // date inside the same folder (at least `manifest.json`).
        let mut resent = Vec::new();
        if items
            .iter()
            .any(|i| matches!(i.part, crate::upload_records::Part::Bundle { .. }))
        {
            let mut fresh = Reconciled::default();
            let outcome = self
                .reconcile(
                    &ctx,
                    &dest,
                    &backend,
                    &mut cancel,
                    claim_ref.clone(),
                    &mut items,
                    &mut sent_for,
                    &mut fresh,
                )
                .await;
            resent = fresh.resent;
            notes.extend(fresh.notes);
            outcome?;
        }

        let sent_items: Vec<Sent> = sent_for.into_iter().flatten().collect();
        let folder_id = claim.as_ref().and_then(|c| c.id.clone());
        let record_note = self
            .record_upload(&ctx, &dest, &items, &sent_items, folder_id.as_deref())
            .await;
        Ok(json!({
            "destination_id": dest.id,
            "destination": dest.name,
            "capture_id": ctx.job.inputs.get("capture_id"),
            "files": files,
            "resent": resent,
            "bytes": sent,
            "share_link": share_link,
            "notes": notes,
            "record_warning": record_note,
        }))
    }
}

/// The sent files of the checkpoint, in plan order.
fn cp_sent(sent_for: &[Option<Sent>]) -> Vec<Sent> {
    sent_for.iter().flatten().cloned().collect()
}

/// Runs one provider call and sets `flag` (which the provider watches) when the job is
/// interrupted, so the call stops at its next chunk.
async fn cancellable<T>(
    fut: impl std::future::Future<Output = T>,
    cancel: &mut cm_jobs::CancelToken,
    flag: &AtomicBool,
) -> T {
    tokio::pin!(fut);
    let mut interrupted = false;
    loop {
        tokio::select! {
            r = &mut fut => return r,
            _ = cancel.interrupted(), if !interrupted => {
                interrupted = true;
                flag.store(true, Ordering::Relaxed);
            }
        }
    }
}

/// What the end-of-job check did, kept outside the `Result` so a failure part-way still reports
/// the files already re-sent.
#[derive(Default)]
struct Reconciled {
    resent: Vec<Value>,
    notes: Vec<String>,
}

/// One difference between the bundle on disk and what was sent.
enum Change {
    /// A file that was sent (index in the plan) and has changed since.
    Changed(usize),
    /// A file that appeared in the bundle after the plan was made.
    Added(Item),
    /// A file that was sent and is gone now (the place at the destination).
    Removed(String),
}

fn bundle_dir(library: &Library, job: &cm_jobs::Job) -> Result<PathBuf, CmError> {
    let id = job
        .inputs
        .get("capture_id")
        .and_then(Value::as_str)
        .ok_or_else(|| CmError::internal("the upload job has no capture"))?;
    let lib = lock_library(library);
    let r = lib.get(id)?;
    Ok(lib.bundle_path(&r))
}

/// Compares the bundle on disk with what was sent. Never reported: `recording.lock`, temp files
/// (skipped by the walk) and anything new below `exports/` (an export made while the job ran is
/// not part of this copy).
fn diff_bundle(
    dir: &Path,
    root: &str,
    items: &[Item],
    sent_for: &[Option<Sent>],
) -> Result<Vec<Change>, CmError> {
    let mut out = Vec::new();
    for (n, item) in items.iter().enumerate() {
        if !matches!(item.part, crate::upload_records::Part::Bundle { .. }) {
            continue;
        }
        let Some(sent) = &sent_for[n] else { continue };
        match fingerprint(&item.src) {
            None => out.push(Change::Removed(place(item))),
            Some(now) => {
                let unknown = sent.fp.is_none() && item.target.file_name == "manifest.json";
                if sent.fp.as_deref().is_some_and(|was| was != now) || unknown {
                    out.push(Change::Changed(n));
                }
            }
        }
    }
    let known: std::collections::HashSet<String> = items.iter().map(place).collect();
    let exports = format!("{root}/exports/");
    for item in bundle_items(dir, root)? {
        let at = place(&item);
        if known.contains(&at) || at == format!("{root}/recording.lock") || at.starts_with(&exports)
        {
            continue;
        }
        out.push(Change::Added(item));
    }
    Ok(out)
}

impl UploadHandler {
    /// STO-5: looks at the bundle again after the last file went, and sends what changed or
    /// appeared into the same folder, replacing the remote copy. At most
    /// [`MAX_RESEND_ROUNDS`] rounds, so a busy editor cannot keep the job alive for ever; then a
    /// note says the bundle kept changing. A destination that cannot replace files
    /// (`not_implemented`) leaves a note instead of failing a finished upload.
    #[allow(clippy::too_many_arguments)]
    async fn reconcile(
        &self,
        ctx: &JobContext,
        dest: &Destination,
        backend: &Backend,
        cancel: &mut cm_jobs::CancelToken,
        claim: Option<(String, Option<String>)>,
        items: &mut Vec<Item>,
        sent_for: &mut Vec<Option<Sent>>,
        out: &mut Reconciled,
    ) -> Result<(), CmError> {
        let root = items
            .iter()
            .find_map(|i| match &i.part {
                crate::upload_records::Part::Bundle { root } => Some(root.clone()),
                _ => None,
            })
            .ok_or_else(|| CmError::internal("no bundle in the plan"))?;
        let (library, job) = (self.library.clone(), ctx.job.clone());
        let dir = tokio::task::spawn_blocking(move || bundle_dir(&library, &job))
            .await
            .map_err(|e| CmError::internal(format!("worker failed: {e}")))??;
        let mut noted_gone = std::collections::HashSet::new();
        for round in 1..=MAX_RESEND_ROUNDS + 1 {
            let (d, r, it, sf) = (dir.clone(), root.clone(), items.clone(), sent_for.clone());
            let changes = tokio::task::spawn_blocking(move || diff_bundle(&d, &r, &it, &sf))
                .await
                .map_err(|e| CmError::internal(format!("worker failed: {e}")))??;
            let mut work = Vec::new();
            for c in changes {
                match c {
                    Change::Removed(at) => {
                        if noted_gone.insert(at.clone()) {
                            out.notes.push(format!(
                                "{at} was removed from the bundle while it uploaded; the copy still has it"
                            ));
                        }
                    }
                    other => work.push(other),
                }
            }
            if work.is_empty() {
                return Ok(());
            }
            if round > MAX_RESEND_ROUNDS {
                out.notes.push(
                    "the bundle kept changing while it was sent; the copy may be a little behind, upload again to bring it up to date"
                        .to_owned(),
                );
                return Ok(());
            }
            for change in work {
                let (n, existing) = match change {
                    Change::Changed(n) => {
                        let id = sent_for[n].as_ref().map(|s| s.remote_id.clone());
                        (n, id)
                    }
                    Change::Added(item) => {
                        items.push(item);
                        sent_for.push(None);
                        (items.len() - 1, None)
                    }
                    Change::Removed(_) => continue,
                };
                self.wait_online(backend.local(), ctx, cancel, 1.0).await?;
                let item = &items[n];
                let Some(fp) = fingerprint(&item.src) else {
                    continue; // gone again: the next round notes it
                };
                ctx.reporter.report(
                    1.0,
                    Some("Updating the copy with your latest changes".to_owned()),
                );
                let flag = Arc::new(AtomicBool::new(false));
                let progress = Progress::new(|_, _| {}, flag.clone());
                let replaced = cancellable(
                    backend.provider().replace(
                        &item.src,
                        &item.target,
                        existing.as_deref(),
                        progress,
                    ),
                    cancel,
                    &flag,
                )
                .await;
                ctx.reporter.report(1.0, Some(String::new()));
                let up = match replaced {
                    Ok(u) => u,
                    Err(CmError::NotImplemented { .. }) => {
                        out.notes.push(format!(
                            "{} changed while it uploaded and {} cannot replace files yet: that file of the copy is out of date",
                            place(item),
                            dest.name
                        ));
                        return Ok(());
                    }
                    Err(e) => return Err(explain_auth(dest, e)),
                };
                let old = sent_for[n].take();
                let here = place(&items[n]);
                items[n].bytes = up.bytes;
                sent_for[n] = Some(Sent {
                    remote_id: up.remote_id.clone(),
                    path: Some(here.clone()),
                    // A replaced file keeps the link it had.
                    share_link: up.share_link.clone().or(old.and_then(|o| o.share_link)),
                    bytes: up.bytes,
                    fp: Some(fp),
                });
                out.resent
                    .push(json!({ "path": here, "remote_id": up.remote_id, "bytes": up.bytes }));
                let now = cp_sent(sent_for);
                ctx.reporter
                    .save_checkpoint(&checkpoint(now.len(), claim.clone(), &now, None))?;
            }
        }
        Ok(())
    }

    /// STO-4.AC3: records the finished upload of a capture in its manifest. A failure here must
    /// not redo the upload, so it comes back as a warning in the job result.
    async fn record_upload(
        &self,
        ctx: &JobContext,
        dest: &Destination,
        items: &[Item],
        sent: &[Sent],
        folder_id: Option<&str>,
    ) -> Option<String> {
        let id = ctx.job.inputs.get("capture_id").and_then(Value::as_str)?;
        let parts: Vec<(Part, Sent)> = items
            .iter()
            .zip(sent)
            .map(|(i, s)| (i.part.clone(), s.clone()))
            .collect();
        let at = OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default();
        let records = upload_records::build(dest, &parts, &at, folder_id);
        match upload_records::save(&self.library, &self.locks, id, records).await {
            Ok(()) => None,
            Err(e) => Some(format!("The upload record was not saved: {e}")),
        }
    }

    /// Waits while the destination is unreachable (STO-1.AC2), showing "Destination offline".
    async fn wait_online(
        &self,
        provider: Option<&LocalProvider>,
        ctx: &JobContext,
        cancel: &mut cm_jobs::CancelToken,
        fraction: f64,
    ) -> Result<(), CmError> {
        // A cloud destination has no mount to wait for; its errors retry as `network`.
        let Some(provider) = provider else {
            return Ok(());
        };
        let mut shown = false;
        while !provider.is_online() {
            if !shown {
                ctx.reporter
                    .report(fraction, Some("Destination offline".to_owned()));
                shown = true;
            }
            tokio::select! {
                _ = tokio::time::sleep(self.offline_poll) => {}
                _ = cancel.interrupted() => {
                    return Err(CmError::internal("The upload was interrupted"));
                }
            }
        }
        if shown {
            ctx.reporter.report(fraction, Some(String::new()));
        }
        Ok(())
    }
}
