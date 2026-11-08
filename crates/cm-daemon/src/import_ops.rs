//! `import_media` (IMP-3, IMP-4): media files that CaptureMann did not record become captures.
//!
//! `import_media { paths, titles?, remove_originals?, dry_run?, after?, request_id? }` probes and
//! classifies every file (`cm_media::import`), then either answers with the rows (`dry_run`) or
//! queues one persisted `import` job and returns `{ job_id, captures }` at once. The job copies
//! each file into a staging folder, makes the editing copy when the file needs one, decodes the
//! sound to `mic.wav`, verifies the result, moves the bundle into the library (`capture.created`)
//! and, with `remove_originals`, moves the original to the Trash. Nothing of that touches the
//! library until a file is complete, and an original is only trashed after its copy was verified
//! and installed.
//!
//! # The tier
//!
//! The action is Free in the registry. A call with `remove_originals: true` (and not `dry_run`)
//! is **Confirm**: [`Action::gating_spec`](crate::action::Action::gating_spec) raises the tier
//! after the parameters are parsed, the plan ([`ImportPlan`]) is made first, and the dialog (kind
//! [`DIALOG_KIND`]) lists every file that will go to the Trash. The job then runs exactly that
//! plan: a file whose size or modified time changed after the dialog is not imported and not
//! trashed (`changed_since_approval`).

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Weak};
use std::time::{Duration, UNIX_EPOCH};

use async_trait::async_trait;
use cm_core::{Actor, CmError, Permission};
use cm_jobs::{Job, JobContext, JobHandler, JobQueue, NewJob, JOB_IMPORT};
use cm_library::import::{
    install, mint_id, new_staging_dir, sweep_staging, write_manifest, ImportBundle, ImportSource,
    SoundTrack, VideoTrack, EDITING_COPY_FILE, SOUND_FILE,
};
use cm_media::import::classify::{classify, MediaKind, Status};
use cm_media::import::prepare::{
    decode_sound, make_editing_copy, sound_channels, verify, RunFailure, VerifyInput,
};
use cm_media::import::probe::{probe, read_head, ProbeError, ProbedFile};
use cm_media::import::Problem;
use cm_media::run::map_io_error;
use cm_media::tools::{locate, Tools};
use serde::Serialize;
use serde_json::{json, Map, Value};
use time::OffsetDateTime;

use crate::action::{Action, Fields};
use crate::daemon::{blocking, lock_library, Inner};
use crate::error::DaemonError;
use crate::events;
use crate::library_ops::{human_size, Trasher};

/// `kind` of the confirmation of `remove_originals`.
pub const DIALOG_KIND: &str = "import_remove_originals";
/// Most paths in one call.
pub const MAX_PATHS: usize = 200;
/// Free space kept after an import (NFR-7).
pub const FREE_FLOOR_BYTES: u64 = 1_000_000_000;

/// `after.transcribe` given as an object.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct TranscribeAfter {
    pub model: Option<String>,
    pub language: Option<String>,
}

/// What to run on every imported capture: the existing `transcribe`, `diarize` and
/// `generate_report` actions, in that order.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ImportAfter {
    pub transcribe: Option<TranscribeAfter>,
    pub diarize: bool,
    pub report_template: Option<String>,
}

impl ImportAfter {
    pub fn is_empty(&self) -> bool {
        self.transcribe.is_none() && !self.diarize && self.report_template.is_none()
    }
}

/// `import_media` input.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ImportMediaParams {
    pub paths: Vec<String>,
    /// Same length as `paths`, or empty; an empty string keeps the default title.
    pub titles: Vec<String>,
    pub remove_originals: bool,
    pub dry_run: bool,
    pub after: ImportAfter,
    pub request_id: Option<String>,
}

/// Parses the input of `import_media`.
pub(crate) fn parse(f: &Fields<'_>) -> Result<Action, CmError> {
    f.only(&[
        "paths",
        "titles",
        "remove_originals",
        "dry_run",
        "after",
        "request_id",
    ])?;
    let strings = |key: &str| -> Result<Option<Vec<String>>, CmError> {
        match f.obj.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::Array(a)) => a
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    v.as_str().map(str::to_owned).ok_or_else(|| {
                        CmError::invalid_field(&format!("{key}[{i}]"), "must be a string")
                    })
                })
                .collect::<Result<Vec<_>, _>>()
                .map(Some),
            Some(_) => Err(CmError::invalid_field(key, "must be an array of strings")),
        }
    };
    let paths = strings("paths")?.ok_or_else(|| CmError::invalid_field("paths", "is required"))?;
    if paths.is_empty() || paths.len() > MAX_PATHS {
        return Err(CmError::invalid_field(
            "paths",
            format!("must hold 1 to {MAX_PATHS} file paths"),
        ));
    }
    for (i, p) in paths.iter().enumerate() {
        if !Path::new(p).is_absolute() {
            return Err(CmError::invalid_field(
                &format!("paths[{i}]"),
                "must be an absolute path",
            ));
        }
    }
    let titles = strings("titles")?.unwrap_or_default();
    if !titles.is_empty() && titles.len() != paths.len() {
        return Err(CmError::invalid_field(
            "titles",
            "must have one entry per path (an empty string keeps the file name)",
        ));
    }
    let after = match f.obj.get("after") {
        None | Some(Value::Null) => ImportAfter::default(),
        Some(Value::Object(o)) => {
            let inner = Fields { obj: o };
            inner
                .only(&["transcribe", "diarize", "report_template"])
                .map_err(|e| CmError::invalid_field("after", e))?;
            let transcribe = match o.get("transcribe") {
                None | Some(Value::Null | Value::Bool(false)) => None,
                Some(Value::Bool(true)) => Some(TranscribeAfter::default()),
                Some(Value::Object(t)) => {
                    let tf = Fields { obj: t };
                    tf.only(&["model", "language"])
                        .map_err(|e| CmError::invalid_field("after.transcribe", e))?;
                    Some(TranscribeAfter {
                        model: tf.str_opt("model")?,
                        language: tf.str_opt("language")?,
                    })
                }
                Some(_) => {
                    return Err(CmError::invalid_field(
                        "after.transcribe",
                        "must be true or an object with `model` and `language`",
                    ))
                }
            };
            let diarize = inner
                .bool_or("diarize", false)
                .map_err(|e| CmError::invalid_field("after.diarize", e))?;
            let report_template = inner.str_opt("report_template")?;
            if (diarize || report_template.is_some()) && transcribe.is_none() {
                return Err(CmError::invalid_field(
                    "after",
                    "`diarize` and `report_template` need `transcribe`",
                ));
            }
            ImportAfter {
                transcribe,
                diarize,
                report_template,
            }
        }
        Some(_) => return Err(CmError::invalid_field("after", "must be an object")),
    };
    Ok(Action::ImportMedia(ImportMediaParams {
        paths,
        titles,
        remove_originals: f.bool_or("remove_originals", false)?,
        dry_run: f.bool_or("dry_run", false)?,
        after,
        request_id: f.request_id()?,
    }))
}

pub(crate) fn summary(p: &ImportMediaParams) -> String {
    let n = p.paths.len();
    let files = if n == 1 {
        "1 file".to_owned()
    } else {
        format!("{n} files")
    };
    match (p.dry_run, p.remove_originals) {
        (true, _) => format!("Check {files} for import"),
        (false, true) => format!("Import {files} and move the originals to the Trash"),
        (false, false) => format!("Import {files}"),
    }
}

/// The tier-raising rule: Confirm for `remove_originals` unless it is a dry run.
pub(crate) fn needs_confirmation(p: &ImportMediaParams) -> bool {
    p.remove_originals && !p.dry_run
}

// ---- the plan -----------------------------------------------------------------------------

/// One file of a call, probed and classified.
#[derive(Debug, Clone)]
pub struct PlannedFile {
    pub path: PathBuf,
    pub name: String,
    pub size_bytes: u64,
    pub mtime_ms: i64,
    pub title: String,
    pub kind: MediaKind,
    pub status: &'static str,
    /// Why the file gets an editing copy.
    pub prepared: Vec<&'static str>,
    /// Set for a file that cannot be imported: `{ reason, message, format?, codec? }`.
    pub error: Option<Value>,
    /// The sheet's row (see the spec, IMP-3).
    pub row: Value,
    /// Estimated bytes the import writes.
    pub need_bytes: u64,
}

impl PlannedFile {
    pub fn importable(&self) -> bool {
        self.error.is_none()
    }
}

/// What a call will do, made before the dialog and before the job.
#[derive(Debug, Clone)]
pub struct ImportPlan {
    pub files: Vec<PlannedFile>,
    pub needed_bytes: u64,
    pub summary: String,
    pub details: Value,
    pub params: Value,
}

fn mtime_ms(meta: &std::fs::Metadata) -> i64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

fn error_value(p: &Problem) -> Value {
    let mut v = p.data();
    v["message"] = json!(p.message);
    v
}

fn default_title(path: &Path) -> String {
    path.file_stem().map_or_else(
        || "Imported".to_owned(),
        |s| s.to_string_lossy().into_owned(),
    )
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map_or_else(String::new, |s| s.to_string_lossy().into_owned())
}

#[allow(clippy::too_many_arguments)]
fn row_of(
    path: &Path,
    name: &str,
    size_bytes: u64,
    title: &str,
    probed: Option<&ProbedFile>,
    kind: MediaKind,
    status: &str,
    reasons: &[&str],
    error: Option<&Value>,
) -> Value {
    let mut row = json!({
        "path": path, "name": name, "size_bytes": size_bytes, "title": title,
        "kind": kind.as_str(), "status": status, "reasons": reasons,
    });
    if let Some(f) = probed {
        row["container"] = json!(f.container.split(',').next().unwrap_or(""));
        row["duration_s"] = json!(f.duration_s);
        if let Some(v) = &f.video {
            row["size"] = json!([v.width, v.height]);
            row["fps"] = json!(v.fps);
            row["video_codec"] = json!(v.codec);
            row["hdr"] = json!(v.hdr);
            row["rotation"] = json!(v.rotation);
        }
        if let Some(a) = &f.audio {
            row["audio_codec"] = json!(a.codec);
            row["audio_channels"] = json!(a.channels);
        }
        row["other_audio_streams"] = json!(f.other_audio_streams);
    }
    if let Some(e) = error {
        row["error"] = e.clone();
    }
    row
}

fn with_data(error: CmError, data: Value) -> DaemonError {
    DaemonError::WithData { error, data }
}

/// Estimated bytes an import writes: the copy, the editing copy (about the size of the source)
/// and the decoded sound.
fn need_bytes(f: &ProbedFile, prepared: bool) -> u64 {
    let sound = f.audio.as_ref().map_or(0, |a| {
        let ch = u64::from(sound_channels(a.channels, f.duration_s));
        (f.duration_s * 48_000.0 * 4.0) as u64 * ch
    });
    f.size_bytes * if prepared { 2 } else { 1 } + sound
}

/// Probes and classifies every path; the first problem that concerns the whole call (a folder, a
/// missing file, a refused permission, a file inside the library, no free space) is returned as
/// the error and nothing else happens. Runs on a blocking thread.
pub(crate) fn make_plan(
    p: &ImportMediaParams,
    library_root: &Path,
    free: Option<u64>,
) -> Result<ImportPlan, DaemonError> {
    let root = std::fs::canonicalize(library_root).unwrap_or_else(|_| library_root.to_path_buf());
    // Pass 1: the whole call fails on a folder, a missing file, a refused permission or a file
    // that already lives in the library.
    let mut seen = std::collections::BTreeSet::new();
    let mut metas = Vec::new();
    for (i, raw) in p.paths.iter().enumerate() {
        let path = PathBuf::from(raw);
        if !seen.insert(path.clone()) {
            continue;
        }
        match std::fs::metadata(&path) {
            Ok(m) if m.is_dir() => {
                return Err(with_data(
                    CmError::invalid_input(format!(
                        "`{raw}` is a folder. Folders can't be imported yet: open the folder and select the files inside."
                    )),
                    json!({ "reason": "folder_not_supported", "path": raw }),
                ))
            }
            Ok(m) => {
                if std::fs::canonicalize(&path).is_ok_and(|c| c.starts_with(&root)) {
                    return Err(with_data(
                        CmError::invalid_input(format!(
                            "`{raw}` is already inside the library folder"
                        )),
                        json!({ "reason": "inside_library", "path": raw }),
                    ));
                }
                match read_head(&path, 16) {
                    Ok(_) => {}
                    Err(ProbeError::Permission { path, message }) => {
                        return Err(with_data(
                            CmError::permission_missing(Permission::Files, message),
                            json!({ "permission": "files", "path": path }),
                        ))
                    }
                    Err(ProbeError::Other(e)) => return Err(e.into()),
                    Err(ProbeError::Problem(_)) => {}
                }
                metas.push((i, path, m));
            }
            Err(e) => match cm_media::import::probe::map_open_error(&path, &e) {
                ProbeError::Permission { path, message } => {
                    return Err(with_data(
                        CmError::permission_missing(Permission::Files, message),
                        json!({ "permission": "files", "path": path }),
                    ))
                }
                ProbeError::Other(e) => return Err(e.into()),
                ProbeError::Problem(pr) => return Err(pr.to_error().into()),
            },
        }
    }
    // Pass 2: probe.
    let tools: Tools = locate()?;
    let mut files = Vec::new();
    for (i, path, meta) in metas {
        let name = file_name(&path);
        let title = p
            .titles
            .get(i)
            .map(|t| t.trim())
            .filter(|t| !t.is_empty())
            .map_or_else(|| default_title(&path), str::to_owned);
        let size_bytes = meta.len();
        let mtime = mtime_ms(&meta);
        let (probed, error): (Option<ProbedFile>, Option<Problem>) = match probe(&tools, &path) {
            Ok(f) => (Some(f), None),
            Err(ProbeError::Problem(pr)) => (None, Some(pr)),
            Err(ProbeError::Permission { path, message }) => {
                return Err(with_data(
                    CmError::permission_missing(Permission::Files, message),
                    json!({ "permission": "files", "path": path }),
                ))
            }
            Err(ProbeError::Other(e)) => return Err(e.into()),
        };
        let (kind, status, prepared, problem) = match (&probed, error) {
            (_, Some(pr)) => (MediaKind::Video, "cant_import", Vec::new(), Some(pr)),
            (Some(f), None) => {
                let c = classify(f);
                match c.status {
                    Status::Ready => (c.kind, "ready", Vec::new(), None),
                    Status::Prepared(why) => (
                        c.kind,
                        "prepared",
                        why.iter().map(|r| r.as_str()).collect(),
                        None,
                    ),
                    Status::CantImport(pr) => (c.kind, "cant_import", Vec::new(), Some(pr)),
                }
            }
            (None, None) => unreachable!("a probe either succeeds or fails"),
        };
        let err_value = problem.as_ref().map(error_value);
        let need = match (&probed, &problem) {
            (Some(f), None) => need_bytes(f, status == "prepared"),
            _ => 0,
        };
        files.push(PlannedFile {
            row: row_of(
                &path,
                &name,
                size_bytes,
                &title,
                probed.as_ref(),
                kind,
                status,
                &prepared,
                err_value.as_ref(),
            ),
            path,
            name,
            size_bytes,
            mtime_ms: mtime,
            title,
            kind,
            status,
            prepared,
            error: err_value,
            need_bytes: need,
        });
    }
    let importable: Vec<&PlannedFile> = files.iter().filter(|f| f.importable()).collect();
    if importable.is_empty() && !p.dry_run {
        let reasons: std::collections::BTreeSet<&str> = files
            .iter()
            .filter_map(|f| f.error.as_ref()?.get("reason")?.as_str())
            .collect();
        let reason = if reasons.len() == 1 {
            reasons
                .iter()
                .next()
                .copied()
                .unwrap_or("nothing_to_import")
        } else {
            "nothing_to_import"
        };
        let first = files
            .iter()
            .find_map(|f| {
                f.error
                    .as_ref()?
                    .get("message")?
                    .as_str()
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| "None of the files can be imported.".to_owned());
        return Err(with_data(
            CmError::invalid_input(first),
            json!({ "reason": reason, "files": files.iter().map(|f| f.row.clone()).collect::<Vec<_>>() }),
        ));
    }
    let needed_bytes: u64 = importable.iter().map(|f| f.need_bytes).sum();
    if !p.dry_run {
        if let Some(free) = free {
            if free < needed_bytes.saturating_add(FREE_FLOOR_BYTES) {
                return Err(with_data(
                    CmError::disk_full(format!(
                        "The import needs about {} and the library's disk has {} free (CaptureMann keeps 1 GB free).",
                        human_size(needed_bytes),
                        human_size(free)
                    )),
                    json!({ "needed_bytes": needed_bytes, "free_bytes": free }),
                ));
            }
        }
    }
    let (summary, details, params) = dialog(p, &files);
    Ok(ImportPlan {
        files,
        needed_bytes,
        summary,
        details,
        params,
    })
}

/// The confirmation of `remove_originals`: every file that goes to the Trash, and the ones that
/// stay where they are.
fn dialog(p: &ImportMediaParams, files: &[PlannedFile]) -> (String, Value, Value) {
    let go: Vec<&PlannedFile> = files.iter().filter(|f| f.importable()).collect();
    let stay: Vec<&PlannedFile> = files.iter().filter(|f| !f.importable()).collect();
    let total: u64 = go.iter().map(|f| f.size_bytes).sum();
    let n = go.len();
    let summary = format!(
        "Import {} and move {} to the Trash ({} freed)",
        if n == 1 {
            "1 file".to_owned()
        } else {
            format!("{n} files")
        },
        if n == 1 {
            "the original"
        } else {
            "the originals"
        },
        human_size(total)
    );
    let mut rows = Map::new();
    for (i, f) in go.iter().enumerate() {
        rows.insert(
            format!("to_trash_{}", i + 1),
            json!(format!(
                "{} ({}, {}) in {}",
                f.name,
                human_size(f.size_bytes),
                f.kind.as_str(),
                f.path
                    .parent()
                    .map_or_else(String::new, |d| d.display().to_string())
            )),
        );
    }
    for (i, f) in stay.iter().enumerate() {
        let why = f
            .error
            .as_ref()
            .and_then(|e| e["message"].as_str())
            .unwrap_or("can't be imported");
        rows.insert(
            format!("stays_{}", i + 1),
            json!(format!(
                "{}: not imported, stays where it is ({why})",
                f.name
            )),
        );
    }
    rows.insert(
        "result".into(),
        json!("Each original goes to the Trash only after its copy in the library has been checked. You can restore it from the Trash."),
    );
    let params = json!({
        "files": go.iter().map(|f| json!({
            "path": f.path, "name": f.name, "size_bytes": f.size_bytes,
            "kind": f.kind.as_str(), "title": f.title,
        })).collect::<Vec<_>>(),
        "not_imported": stay.iter().map(|f| json!({
            "path": f.path, "reason": f.error.as_ref().map(|e| e["reason"].clone()),
        })).collect::<Vec<_>>(),
        "total_bytes": total,
        "trash_count": n,
        "remove_originals": p.remove_originals,
    });
    (summary, Value::Object(rows), params)
}

fn capture_rows(plan: &ImportPlan, ids: &BTreeMap<PathBuf, String>) -> Vec<Value> {
    plan.files
        .iter()
        .map(|f| match (&f.error, ids.get(&f.path)) {
            (None, Some(id)) => json!({
                "capture_id": id, "path": f.path, "title": f.title,
                "kind": f.kind.as_str(), "status": "queued",
            }),
            _ => json!({
                "capture_id": Value::Null, "path": f.path, "title": f.title,
                "kind": f.kind.as_str(), "status": "cant_import", "error": f.error,
            }),
        })
        .collect()
}

impl Inner {
    /// The plan of a call (the dialog of `remove_originals` is built from it).
    pub(crate) async fn plan_import(
        &self,
        p: &ImportMediaParams,
    ) -> Result<ImportPlan, DaemonError> {
        let (root, free_space) = {
            let lib = lock_library(&self.library);
            (lib.root().to_path_buf(), self.free_space.clone())
        };
        let free = free_space.free_bytes(&root);
        let p = p.clone();
        match tokio::task::spawn_blocking(move || make_plan(&p, &root, free)).await {
            Ok(r) => r,
            Err(e) => Err(DaemonError::Internal(format!("worker failed: {e}"))),
        }
    }

    /// `import_media`, after the confirmation (when the call needed one). `plan` is the approved
    /// plan; without it the plan is made now.
    pub(crate) async fn import_media(
        &self,
        actor: &Actor,
        p: ImportMediaParams,
        plan: Option<ImportPlan>,
    ) -> Result<Value, DaemonError> {
        let plan = match plan {
            Some(pl) => pl,
            None => self.plan_import(&p).await?,
        };
        if p.dry_run {
            return Ok(
                json!({ "files": plan.files.iter().map(|f| f.row.clone()).collect::<Vec<_>>() }),
            );
        }
        let queue = self.job_queue()?.queue.clone();
        if !queue.has_handler(JOB_IMPORT) {
            return Err(DaemonError::NotImplemented(
                "import is not available: this daemon has no import handler registered".to_owned(),
            ));
        }
        let now = self.clock.now();
        let ids: BTreeMap<PathBuf, String> = plan
            .files
            .iter()
            .filter(|f| f.importable())
            .map(|f| (f.path.clone(), mint_id(now)))
            .collect();
        let files: Vec<Value> = plan
            .files
            .iter()
            .filter(|f| f.importable())
            .map(|f| {
                json!({
                    "path": f.path, "name": f.name, "size_bytes": f.size_bytes,
                    "mtime_ms": f.mtime_ms, "title": f.title, "kind": f.kind.as_str(),
                    "capture_id": ids[&f.path], "status": f.status, "prepared": f.prepared,
                })
            })
            .collect();
        let inputs = json!({
            "files": files,
            "remove_originals": p.remove_originals,
            "after": p.after,
            "by": actor.to_string(),
            "total_bytes": plan.files.iter().filter(|f| f.importable()).map(|f| f.size_bytes).sum::<u64>(),
        });
        let job = blocking(move || queue.enqueue(NewJob::new(JOB_IMPORT, inputs))).await?;
        let captures = capture_rows(&plan, &ids);
        Ok(json!({
            "job_id": job.id,
            "state": job.state.as_str(),
            "captures": captures,
            "needed_bytes": plan.needed_bytes,
        }))
    }
}

// ---- the job ------------------------------------------------------------------------------

/// Runs `import` jobs. Registered with [`crate::Daemon::register_import_handler`].
pub struct ImportHandler {
    pub(crate) library: Arc<std::sync::Mutex<cm_library::Library>>,
    pub(crate) events: crate::events::EventBus,
    pub(crate) trasher: Arc<dyn Trasher>,
    pub(crate) free_space: Arc<dyn cm_capture::disk::FreeSpace>,
    pub(crate) inner: Weak<Inner>,
    /// How the FFmpeg tools are found (a test points this at its own).
    pub(crate) tools: Arc<dyn Fn() -> Result<Tools, CmError> + Send + Sync>,
}

/// What happened to one file.
#[derive(Debug, Clone)]
struct FileOutcome {
    path: String,
    capture_id: Option<String>,
    status: &'static str,
    error: Option<Value>,
    trash: Option<Value>,
    /// How the editing copy was made (IMP-2; O10): the encoder, its codec, and a sentence when it
    /// is not hardware H.264.
    editing_copy: Option<Value>,
    code: Option<CmError>,
}

impl FileOutcome {
    fn to_json(&self) -> Value {
        let mut v = json!({ "path": self.path, "status": self.status });
        if let Some(id) = &self.capture_id {
            v["capture_id"] = json!(id);
        }
        if let Some(e) = &self.error {
            v["error"] = e.clone();
        }
        if let Some(t) = &self.trash {
            v["trash"] = t.clone();
        }
        if let Some(c) = &self.editing_copy {
            v["editing_copy"] = c.clone();
        }
        v
    }

    fn failed(path: &str, e: CmError, reason: Option<&str>, detail: Option<&str>) -> FileOutcome {
        let mut err = json!({ "code": e.code().as_str(), "message": e.to_string() });
        if let Some(r) = reason {
            err["reason"] = json!(r);
        }
        if let Some(d) = detail {
            err["detail"] = json!(d);
        }
        FileOutcome {
            path: path.to_owned(),
            capture_id: None,
            status: "failed",
            error: Some(err),
            trash: None,
            editing_copy: None,
            code: Some(e),
        }
    }

    fn problem(path: &str, p: &Problem) -> FileOutcome {
        let mut o = FileOutcome::failed(path, p.to_error(), Some(p.reason.as_str()), None);
        if let Some(e) = o.error.as_mut() {
            if let Some(c) = &p.codec {
                e["codec"] = json!(c);
            }
            if let Some(fm) = &p.format {
                e["format"] = json!(fm);
            }
        }
        o
    }
}

/// The message of a file that cannot be prepared for editing. On Linux the build has a software
/// fallback (VP9, O10), so reaching this means even that is missing.
fn no_encoder_message() -> &'static str {
    if std::env::consts::OS == "linux" {
        "This computer can't prepare this file for editing (no hardware H.264 encoder, and this FFmpeg has no VP9 encoder to fall back on)."
    } else {
        "This computer can't prepare this file for editing (no hardware video encoder)."
    }
}

/// How a step of one file ended.
#[allow(clippy::large_enum_variant)]
enum Stop {
    /// The job was interrupted (cancel, pause or shutdown); the file is not done.
    Interrupted,
    Failed(FileOutcome),
}

fn io_stop(path: &str, e: &std::io::Error) -> Stop {
    let err = if map_io_error("", e).code() == cm_core::ErrorCode::DiskFull {
        CmError::disk_full("The disk is full, so the file could not be copied.")
    } else {
        CmError::internal(format!("cannot copy `{path}`: {e}"))
    };
    Stop::Failed(FileOutcome::failed(path, err, None, None))
}

/// Copies `from` to `to` in chunks, calling `progress(bytes_done)` and checking `interrupted`.
fn copy_file(
    from: &Path,
    to: &Path,
    interrupted: &dyn Fn() -> bool,
    progress: &mut dyn FnMut(u64),
) -> std::io::Result<bool> {
    let mut src = std::fs::File::open(from)?;
    let mut dst = std::fs::File::create(to)?;
    let mut buf = vec![0u8; 4 << 20];
    let mut done = 0u64;
    loop {
        if interrupted() {
            return Ok(false);
        }
        let n = src.read(&mut buf)?;
        if n == 0 {
            break;
        }
        dst.write_all(&buf[..n])?;
        done += n as u64;
        progress(done);
    }
    dst.sync_all()?;
    Ok(true)
}

fn master_name(kind: MediaKind, prepared: bool, source: &Path) -> String {
    let ext = source
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .filter(|e| !e.is_empty() && e.len() <= 8)
        .unwrap_or_else(|| "bin".to_owned());
    if kind == MediaKind::Video && !prepared {
        format!("screen.{ext}")
    } else {
        format!("original.{ext}")
    }
}

/// Throttled progress: the job's fraction and a message.
struct Progress<'a> {
    reporter: &'a cm_jobs::ProgressReporter,
    total: f64,
    before: f64,
    size: f64,
    last: f64,
}

impl Progress<'_> {
    /// `within` is the fraction of the current file (0..=1).
    fn set(&mut self, within: f64, message: &str) {
        let f = if self.total > 0.0 {
            ((self.before + self.size * within.clamp(0.0, 1.0)) / self.total).clamp(0.0, 0.99)
        } else {
            0.0
        };
        if f - self.last >= 0.005 || within <= 0.0 || within >= 1.0 {
            self.last = f.max(self.last);
            self.reporter.report(f, Some(message.to_owned()));
        }
    }
}

struct ImportRun {
    library: Arc<std::sync::Mutex<cm_library::Library>>,
    events: crate::events::EventBus,
    trasher: Arc<dyn Trasher>,
    tools: Tools,
    by: Actor,
    job_id: String,
    remove_originals: bool,
    root: PathBuf,
}

impl ImportRun {
    /// Imports one file. Blocking.
    fn one(
        &self,
        index: usize,
        f: &Value,
        cancel: &dyn Fn() -> bool,
        prog: &mut Progress<'_>,
    ) -> Result<FileOutcome, Stop> {
        let path_s = f["path"].as_str().unwrap_or("").to_owned();
        let path = PathBuf::from(&path_s);
        let capture_id = f["capture_id"].as_str().unwrap_or("").to_owned();
        let title = f["title"].as_str().unwrap_or("Imported").to_owned();
        let fail = |e: CmError, reason: Option<&str>, detail: Option<&str>| {
            Stop::Failed(FileOutcome::failed(&path_s, e, reason, detail))
        };
        // The file as approved.
        let meta = match std::fs::metadata(&path) {
            Ok(m) => m,
            Err(e) => {
                return Err(match cm_media::import::probe::map_open_error(&path, &e) {
                    ProbeError::Permission { message, .. } => Stop::Failed(FileOutcome::failed(
                        &path_s,
                        CmError::permission_missing(Permission::Files, message),
                        None,
                        None,
                    )),
                    ProbeError::Other(e) => fail(e, None, None),
                    ProbeError::Problem(p) => Stop::Failed(FileOutcome::problem(&path_s, &p)),
                })
            }
        };
        if meta.len() != f["size_bytes"].as_u64().unwrap_or(0)
            || mtime_ms(&meta) != f["mtime_ms"].as_i64().unwrap_or(0)
        {
            let (reason, detail) = if self.remove_originals {
                ("changed_since_approval", None)
            } else {
                ("unreadable_file", Some("changed"))
            };
            return Err(fail(
                CmError::invalid_input(format!(
                    "`{}` changed after it was checked, so it was not imported.",
                    file_name(&path)
                )),
                Some(reason),
                detail,
            ));
        }
        let source = match probe(&self.tools, &path) {
            Ok(s) => s,
            Err(ProbeError::Problem(p)) => {
                return Err(Stop::Failed(FileOutcome::problem(&path_s, &p)))
            }
            Err(ProbeError::Permission { message, .. }) => {
                return Err(fail(
                    CmError::permission_missing(Permission::Files, message),
                    None,
                    None,
                ))
            }
            Err(ProbeError::Other(e)) => return Err(fail(e, None, None)),
        };
        let c = classify(&source);
        let reasons = match c.status {
            Status::CantImport(p) => return Err(Stop::Failed(FileOutcome::problem(&path_s, &p))),
            Status::Ready => Vec::new(),
            Status::Prepared(w) => w.iter().map(|r| r.as_str().to_owned()).collect(),
        };
        let prepared = !reasons.is_empty();
        let kind = c.kind;

        let stage =
            new_staging_dir(&self.root, &self.job_id, index).map_err(|e| fail(e, None, None))?;
        let cleanup = |_: ()| {
            let _ = std::fs::remove_dir_all(&stage);
        };
        let finish_err = |s: Stop| {
            cleanup(());
            s
        };
        // 1. Copy.
        let master = master_name(kind, prepared, &path);
        let copy_share = if prepared { 0.3 } else { 0.7 };
        prog.set(0.0, "Copying into the library");
        let mut last = 0u64;
        let total = meta.len().max(1) as f64;
        let copied = copy_file(&path, &stage.join(&master), cancel, &mut |done| {
            last = done;
            prog.set(
                copy_share * (done as f64 / total),
                "Copying into the library",
            );
        });
        match copied {
            Ok(true) => {}
            Ok(false) => return Err(finish_err(Stop::Interrupted)),
            Err(e) => return Err(finish_err(io_stop(&path_s, &e))),
        }
        let _ = last;
        // 2. The editing copy.
        let mut screen_file = master.clone();
        let mut editing_copy: Option<Value> = None;
        if prepared {
            let out = stage.join(EDITING_COPY_FILE);
            prog.set(copy_share, "Preparing for editing");
            let mut on = |p: f64| prog.set(copy_share + 0.4 * p, "Preparing for editing");
            match make_editing_copy(&self.tools, &source, &out, &mut on, cancel) {
                Ok(copy) => {
                    screen_file = EDITING_COPY_FILE.to_owned();
                    editing_copy = Some(json!({
                        "encoder": copy.encoder, "codec": copy.codec,
                        "software": copy.software, "note": copy.note,
                    }));
                }
                Err(RunFailure::Cancelled) => return Err(finish_err(Stop::Interrupted)),
                Err(RunFailure::NoHardwareEncoder) => {
                    cleanup(());
                    let mut o = FileOutcome::failed(
                        &path_s,
                        CmError::invalid_input(no_encoder_message()),
                        Some("unsupported_format"),
                        Some("no_hardware_encoder"),
                    );
                    if let (Some(e), Some(v)) = (o.error.as_mut(), source.video.as_ref()) {
                        e["codec"] = json!(v.codec);
                    }
                    return Err(Stop::Failed(o));
                }
                Err(RunFailure::Failed(e)) => return Err(finish_err(fail(e, None, None))),
            }
        }
        // 3. The sound.
        let mut sound = None;
        if source.audio.is_some() {
            let out = stage.join(SOUND_FILE);
            prog.set(0.85, "Reading the sound");
            let mut on = |p: f64| prog.set(0.85 + 0.1 * p, "Reading the sound");
            match decode_sound(&self.tools, &source, &out, &mut on, cancel) {
                Ok(channels) => {
                    let secs =
                        cm_media::import::prepare::wav_seconds(&out).unwrap_or(source.duration_s);
                    let want = source.audio.as_ref().map_or(1, |a| a.channels.clamp(1, 2));
                    sound = Some(SoundTrack {
                        channels,
                        duration_s: secs,
                        downmixed: channels < want,
                    });
                }
                Err(RunFailure::Cancelled) => return Err(finish_err(Stop::Interrupted)),
                Err(RunFailure::NoHardwareEncoder) => {
                    return Err(finish_err(fail(
                        CmError::internal("unexpected encoder error"),
                        None,
                        None,
                    )))
                }
                Err(RunFailure::Failed(e)) => return Err(finish_err(fail(e, None, None))),
            }
        }
        // 4. Verify.
        prog.set(0.95, "Checking the copy");
        let master_path = stage.join(&master);
        let sound_path = stage.join(SOUND_FILE);
        let edit_path = stage.join(EDITING_COPY_FILE);
        let verdict = verify(&VerifyInput {
            tools: &self.tools,
            source: &source,
            kind,
            master: &master_path,
            sound: sound.as_ref().map(|_| sound_path.as_path()),
            editing_copy: prepared.then_some(edit_path.as_path()),
        });
        if let Err(p) = verdict {
            cleanup(());
            return Err(Stop::Failed(FileOutcome::problem(&path_s, &p)));
        }
        if cancel() {
            return Err(finish_err(Stop::Interrupted));
        }
        // 5. The manifest and the move into the library.
        let now = OffsetDateTime::now_utc();
        let video = source
            .video
            .as_ref()
            .filter(|_| kind == MediaKind::Video)
            .map(|v| VideoTrack {
                file: screen_file.clone(),
                size: (v.width, v.height),
                fps: v.fps.unwrap_or(30.0),
                codec: v.codec.clone(),
                master: prepared.then(|| master.clone()),
                prepared: reasons.clone(),
                hdr: v.hdr,
            });
        let bundle = ImportBundle {
            id: capture_id.clone(),
            title: title.clone(),
            duration_s: source.duration_s,
            video,
            sound,
            source: ImportSource {
                path: path_s.clone(),
                imported_at: now,
                original_name: file_name(&path),
                original_file: master.clone(),
            },
            created: now,
            by: self.by.clone(),
        };
        if let Err(e) = write_manifest(&stage, &bundle) {
            cleanup(());
            return Err(fail(e, None, None));
        }
        let record = {
            let mut lib = lock_library(&self.library);
            install(&mut lib, &stage, &title)
        };
        let record = match record {
            Ok(r) => r,
            Err(e) => {
                cleanup(());
                return Err(fail(e, None, None));
            }
        };
        self.events.publish(
            events::CAPTURE_CREATED,
            json!({ "capture_id": capture_id, "title": record.title, "by": self.by,
                    "job_id": self.job_id, "import": true, "kind": record.kind.as_str() }),
        );
        // 6. The original, only now that its copy is in the library.
        let trash = self.remove_originals.then(|| self.trash(&path));
        prog.set(1.0, "Done");
        Ok(FileOutcome {
            path: path_s,
            capture_id: Some(capture_id),
            status: "imported",
            error: None,
            trash,
            editing_copy,
            code: None,
        })
    }

    fn trash(&self, path: &Path) -> Value {
        match self.trasher.trash(path) {
            Ok(()) => json!({ "state": "trashed" }),
            Err(e) => json!({ "state": "failed", "message": e.to_string() }),
        }
    }
}

impl ImportHandler {
    fn finished_before(&self, id: &str) -> bool {
        lock_library(&self.library).get(id).is_ok()
    }
}

#[async_trait]
impl JobHandler for ImportHandler {
    async fn run(&self, ctx: JobContext) -> Result<Value, CmError> {
        let inputs = ctx.job.inputs.clone();
        let files: Vec<Value> = inputs["files"].as_array().cloned().unwrap_or_default();
        let remove_originals = inputs["remove_originals"].as_bool().unwrap_or(false);
        let by = inputs["by"]
            .as_str()
            .and_then(|s| s.parse::<Actor>().ok())
            .unwrap_or(Actor::User);
        let total: f64 = files
            .iter()
            .map(|f| f["size_bytes"].as_f64().unwrap_or(0.0))
            .sum();
        let tools = (self.tools)()?;
        let root = lock_library(&self.library).root().to_path_buf();
        // A disk that filled up since the call.
        if let Some(free) = self.free_space.free_bytes(&root) {
            if free < crate::import_ops::FREE_FLOOR_BYTES {
                return Err(CmError::disk_full(
                    "The disk is nearly full, so nothing was imported.",
                ));
            }
        }
        // Finished files from an earlier attempt (the checkpoint) and captures that exist already.
        let mut outcomes: Vec<Value> = ctx
            .job
            .checkpoint
            .as_ref()
            .and_then(|c| c["outcomes"].as_array().cloned())
            .unwrap_or_default();
        let mut failures: Vec<CmError> = Vec::new();
        let mut before: f64 = outcomes
            .iter()
            .map(|o| {
                files
                    .iter()
                    .find(|f| f["path"] == o["path"])
                    .and_then(|f| f["size_bytes"].as_f64())
                    .unwrap_or(0.0)
            })
            .sum();
        let mut interrupted = false;
        for (i, f) in files.iter().enumerate() {
            if outcomes.len() > i {
                continue;
            }
            if ctx.cancel.is_interrupted() {
                interrupted = true;
                break;
            }
            let id = f["capture_id"].as_str().unwrap_or("").to_owned();
            let size = f["size_bytes"].as_f64().unwrap_or(0.0);
            // A crash between the move into the library and the checkpoint: the capture is there.
            if self.finished_before(&id) {
                let mut o = FileOutcome {
                    path: f["path"].as_str().unwrap_or("").to_owned(),
                    capture_id: Some(id),
                    status: "imported",
                    error: None,
                    trash: None,
                    editing_copy: None,
                    code: None,
                };
                if remove_originals && Path::new(&o.path).is_file() {
                    let p = PathBuf::from(&o.path);
                    let same = std::fs::metadata(&p)
                        .is_ok_and(|m| m.len() == f["size_bytes"].as_u64().unwrap_or(0));
                    if same {
                        o.trash = Some(match self.trasher.trash(&p) {
                            Ok(()) => json!({ "state": "trashed" }),
                            Err(e) => json!({ "state": "failed", "message": e.to_string() }),
                        });
                    }
                }
                outcomes.push(o.to_json());
                before += size;
                continue;
            }
            let run = ImportRun {
                library: self.library.clone(),
                events: self.events.clone(),
                trasher: self.trasher.clone(),
                tools: tools.clone(),
                by: by.clone(),
                job_id: ctx.job.id.clone(),
                remove_originals,
                root: root.clone(),
            };
            let (reporter, cancel) = (ctx.reporter.clone(), ctx.cancel.clone());
            let (fc, idx) = (f.clone(), i);
            let b = before;
            let res = tokio::task::spawn_blocking(move || {
                let mut prog = Progress {
                    reporter: &reporter,
                    total,
                    before: b,
                    size,
                    last: 0.0,
                };
                run.one(idx, &fc, &|| cancel.is_interrupted(), &mut prog)
            })
            .await
            .map_err(|e| CmError::internal(format!("import worker failed: {e}")))?;
            match res {
                Ok(o) => {
                    let mut j = o.to_json();
                    if o.status == "imported" {
                        if let Some(a) = self.queue_after(&inputs["after"], &id).await {
                            j["after_jobs"] = a;
                        }
                    }
                    outcomes.push(j);
                }
                Err(Stop::Interrupted) => {
                    interrupted = true;
                    break;
                }
                Err(Stop::Failed(o)) => {
                    if let Some(e) = &o.code {
                        failures.push(e.clone());
                    }
                    outcomes.push(o.to_json());
                }
            }
            before += size;
            ctx.reporter
                .save_checkpoint(&json!({ "outcomes": outcomes }))?;
        }
        if interrupted {
            // Cancel, pause or shutdown: the finished files stay; the unfinished one starts again.
            sweep_staging(&root, Some(&ctx.job.id));
            return Err(CmError::internal("the import was interrupted"));
        }
        let imported = outcomes
            .iter()
            .filter(|o| o["status"] == "imported")
            .count();
        let failed = outcomes.iter().filter(|o| o["status"] == "failed").count();
        if imported == 0 && failed > 0 {
            let first = failures
                .into_iter()
                .next()
                .unwrap_or_else(|| CmError::internal("none of the files could be imported"));
            return Err(first);
        }
        let trashed: Vec<Value> = outcomes
            .iter()
            .filter(|o| o["trash"]["state"] == "trashed")
            .map(|o| o["path"].clone())
            .collect();
        // The independent jobs queued for each capture (`after`), by capture id.
        let after: Map<String, Value> = outcomes
            .iter()
            .filter_map(|o| {
                Some((
                    o["capture_id"].as_str()?.to_owned(),
                    o.get("after_jobs")?.clone(),
                ))
            })
            .collect();
        let after = if after.is_empty() {
            Value::Null
        } else {
            Value::Object(after)
        };
        ctx.reporter.report(1.0, Some("Done".to_owned()));
        Ok(
            json!({ "files": outcomes, "imported": imported, "failed": failed,
                   "trashed": trashed, "after": after }),
        )
    }

    async fn on_cancel(&self, job: &Job) {
        let root = lock_library(&self.library).root().to_path_buf();
        sweep_staging(&root, Some(&job.id));
    }
}

impl ImportHandler {
    /// `after`: queues the existing `transcribe` job of the capture that was just installed, and
    /// (when asked) `diarize` and a report once that transcript exists. Nothing here waits: the
    /// import ends when its files are in the library, and these are ordinary jobs of their own
    /// (`list_jobs`, `get_job`, the capture's own progress). A step that cannot run (no model) is
    /// `skipped` with its reason; the import is never affected. `None` without `after.transcribe`.
    async fn queue_after(&self, after: &Value, id: &str) -> Option<Value> {
        if !after["transcribe"].is_object() {
            return None;
        }
        let inner = self.inner.upgrade()?;
        let actor = Actor::User;
        let t = &after["transcribe"];
        let tr = crate::ai::AiParams {
            op: crate::ai::AiOp::Transcribe {
                capture_id: id.to_owned(),
                language: t["language"].as_str().map(str::to_owned),
                model: t["model"].as_str().map(str::to_owned),
                original_audio: None,
            },
            request_id: None,
        };
        let mut steps = Map::new();
        let started = inner.ai_op(&actor, tr).await;
        let job_id = match &started {
            Ok(v) => v["job_id"].as_str().map(str::to_owned),
            Err(_) => None,
        };
        match (&started, &job_id) {
            (Ok(_), Some(j)) => {
                steps.insert(
                    "transcribe".into(),
                    json!({ "state": "queued", "job_id": j }),
                );
            }
            (Ok(_), None) => {
                steps.insert("transcribe".into(), json!({ "state": "done" }));
            }
            (Err(e), _) => {
                let why = match e {
                    DaemonError::WithData { data, .. } => {
                        data["reason"].as_str().map(str::to_owned)
                    }
                    _ => None,
                }
                .unwrap_or_else(|| e.to_string());
                steps.insert(
                    "transcribe".into(),
                    json!({ "state": "skipped", "reason": why }),
                );
                return Some(Value::Object(steps));
            }
        }
        // The editor and the library card follow this transcription like an automatic one.
        if let Some(j) = &job_id {
            inner.events.publish(
                events::TRANSCRIPT_STARTED,
                json!({ "capture_id": id, "job_id": j, "automatic": false, "import": true }),
            );
        }
        let want_diarize = after["diarize"].as_bool() == Some(true);
        let template = after["report_template"].as_str().map(str::to_owned);
        if want_diarize {
            steps.insert("diarize".into(), json!({ "state": "after_transcript" }));
        }
        if template.is_some() {
            steps.insert("report".into(), json!({ "state": "after_transcript" }));
        }
        if (want_diarize || template.is_some()) && job_id.is_some() {
            // Detached: speakers and the report need the transcript, so they are queued when
            // its job is done. The import job does not wait for this.
            let (cid, tj) = (id.to_owned(), job_id.unwrap_or_default());
            tokio::spawn(async move {
                let Ok(rt) = inner.job_queue() else { return };
                let queue: JobQueue = rt.queue.clone();
                loop {
                    match queue.get(&tj) {
                        Ok(j) if j.state == cm_jobs::JobState::Done => break,
                        Ok(j) if j.state.is_finished() => return,
                        Ok(_) => tokio::time::sleep(Duration::from_millis(500)).await,
                        Err(_) => return,
                    }
                }
                if want_diarize {
                    let p = crate::ai::AiParams {
                        op: crate::ai::AiOp::Diarize {
                            capture_id: cid.clone(),
                            hints: Default::default(),
                            headset: false,
                        },
                        request_id: None,
                    };
                    let _ = inner.ai_op(&Actor::User, p).await;
                }
                if let Some(tpl) = template {
                    let p = crate::ai::AiParams {
                        op: crate::ai::AiOp::GenerateReport {
                            capture_id: cid,
                            template: tpl,
                            language: None,
                            context: Default::default(),
                            report_id: None,
                        },
                        request_id: None,
                    };
                    let _ = inner.ai_op(&Actor::User, p).await;
                }
            });
        }
        Some(Value::Object(steps))
    }
}

impl crate::Daemon {
    /// Registers the executor of the job type `import` (next to the other handlers, before
    /// [`crate::Daemon::start_jobs`]). Staging folders of an import that was cut short are
    /// removed first; none can be running yet.
    pub fn register_import_handler(&self) -> Result<(), DaemonError> {
        let root = lock_library(&self.inner.library).root().to_path_buf();
        sweep_staging(&root, None);
        self.register_job_handler(JOB_IMPORT, self.import_handler())
    }

    /// The handler itself (tests give it their own FFmpeg lookup).
    pub fn import_handler(&self) -> Arc<dyn JobHandler> {
        Arc::new(ImportHandler {
            library: self.inner.library.clone(),
            events: self.inner.events.clone(),
            trasher: self.inner.trasher.clone(),
            free_space: self.inner.free_space.clone(),
            inner: Arc::downgrade(&self.inner),
            tools: Arc::new(locate),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An absolute path on this platform (`/a.mov` has no drive letter on Windows).
    fn abs(name: &str) -> String {
        std::env::temp_dir()
            .join(name)
            .to_string_lossy()
            .into_owned()
    }

    fn params(v: Value) -> Result<ImportMediaParams, CmError> {
        let empty = Map::new();
        let f = Fields::new(&v, &empty)?;
        match parse(&f)? {
            Action::ImportMedia(p) => Ok(p),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn the_input_is_checked_by_name() {
        let p = params(json!({"paths": [abs("b.mov")]})).unwrap();
        assert!(!p.remove_originals && !p.dry_run && p.after.is_empty());
        let msg = |v: Value| params(v).unwrap_err().to_string();
        assert!(msg(json!({})).contains("paths: is required"));
        assert!(msg(json!({"paths": []})).contains("1 to 200"));
        assert!(msg(json!({"paths": ["rel/a.mov"]})).contains("paths[0]: must be an absolute path"));
        assert!(msg(json!({"paths": [abs("a.mov")], "titles": ["x", "y"]})).contains("titles"));
        assert!(msg(json!({"paths": [abs("a.mov")], "extra": 1})).contains("unknown parameter"));
        assert!(
            msg(json!({"paths": [abs("a.mov")], "after": {"diarize": true}}))
                .contains("need `transcribe`")
        );
        assert!(
            msg(json!({"paths": [abs("a.mov")], "remove_originals": "yes"})).contains("boolean")
        );
        let many: Vec<String> = (0..201).map(|i| format!("/a/{i}.mov")).collect();
        assert!(msg(json!({"paths": many})).contains("1 to 200"));
        let p = params(json!({"paths": [abs("a.mov"), abs("b.wav")], "titles": ["One", ""],
            "after": {"transcribe": {"language": "en"}, "diarize": true, "report_template": "minutes"},
            "remove_originals": true, "request_id": "r1"})).unwrap();
        assert_eq!(p.after.transcribe.unwrap().language.as_deref(), Some("en"));
        assert!(
            p.after.diarize
                && needs_confirmation(&ImportMediaParams {
                    remove_originals: true,
                    dry_run: false,
                    ..params(json!({"paths": [abs("a.mov")]})).unwrap()
                })
        );
    }

    #[test]
    fn only_a_real_remove_originals_call_needs_the_dialog() {
        let mut p = params(json!({"paths": [abs("a.mov")]})).unwrap();
        assert!(!needs_confirmation(&p));
        p.remove_originals = true;
        assert!(needs_confirmation(&p));
        p.dry_run = true;
        assert!(!needs_confirmation(&p));
    }

    #[test]
    fn the_master_is_named_by_kind() {
        let p = Path::new("/x/Clip.MOV");
        assert_eq!(master_name(MediaKind::Video, false, p), "screen.mov");
        assert_eq!(master_name(MediaKind::Video, true, p), "original.mov");
        assert_eq!(
            master_name(MediaKind::Audio, false, Path::new("/x/a.wav")),
            "original.wav"
        );
        assert_eq!(
            master_name(MediaKind::Audio, false, Path::new("/x/noext")),
            "original.bin"
        );
    }

    #[test]
    fn a_copy_checks_for_a_stop_between_chunks_and_reports_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let (from, to) = (dir.path().join("a.bin"), dir.path().join("b.bin"));
        std::fs::write(&from, vec![7u8; 9 << 20]).unwrap();
        let mut seen = Vec::new();
        assert!(copy_file(&from, &to, &|| false, &mut |d| seen.push(d)).unwrap());
        assert_eq!(std::fs::read(&to).unwrap(), std::fs::read(&from).unwrap());
        assert_eq!(*seen.last().unwrap(), 9 << 20);
        assert!(seen.windows(2).all(|w| w[0] < w[1]));
        // Stopped after the first chunk: false, and the caller removes the partial file.
        let n = std::cell::Cell::new(0);
        let stop = || {
            n.set(n.get() + 1);
            n.get() > 1
        };
        assert!(!copy_file(&from, &to, &stop, &mut |_| {}).unwrap());
    }

    #[test]
    fn the_summary_says_what_will_happen() {
        let mut p = params(json!({"paths": [abs("a.mov"), abs("b.mov")]})).unwrap();
        assert_eq!(summary(&p), "Import 2 files");
        p.remove_originals = true;
        assert_eq!(
            summary(&p),
            "Import 2 files and move the originals to the Trash"
        );
        p.dry_run = true;
        assert_eq!(summary(&p), "Check 2 files for import");
    }
}
