//! Changing the library folder (LIB-2.AC5 to AC12): `move_library` and the `library_move` job.
//!
//! * The action `move_library { path, mode?, request_id? }` is **Confirm tier, always** (a
//!   Confirm row of [`crate::ACTIONS`] cannot be loosened by any setting). `mode` is `move`
//!   (default: copy every bundle to the new folder, then switch), `fresh` (switch to an empty
//!   library there; nothing is copied) or `use_existing` (the folder already holds a CaptureMann
//!   library: switch to it). The result is `{ job_id, state, mode, from, to, bytes, bundles,
//!   restart_required: true }`; `job_id` is `null` for `fresh` and `use_existing`, which finish at
//!   once.
//! * The **switch** is `library.path` written by the daemon itself (an app-only key an agent cannot
//!   write) once the copy is complete and verified. The running process keeps using the old
//!   library: the index, the job queue and the watcher live inside the library folder, so the new
//!   folder is used after a restart (`restart_required: true`).
//! * The **copy** never deletes anything from the old folder. Each bundle is copied to
//!   `<name>.cm-copying` in the new folder, checked against the original (the same files with the
//!   same byte sizes, `manifest.json` byte-identical and parsing), and only then renamed to its
//!   real name. So a name in the new folder always holds a complete bundle; a crash, an unplugged
//!   drive or a cancel leaves at most one `*.cm-copying` folder, removed by the next attempt. A
//!   resumed job compares what is already there with the originals and skips what matches.
//! * Bundles being recorded (`recording.lock`) are skipped with a warning and stay in the old
//!   folder. Symbolic links are never followed (and not copied).
//! * Moving the old copies to the Trash is a separate step ([`crate::Daemon::trash_old_copies`])
//!   that re-checks every bundle against its copy first.
//!
//! Events: `library.move_started { job_id, from, to, bundles, bytes }` and `library.moved { from,
//! to, mode, bundles, bytes, skipped, restart_required }`; progress is the ordinary `job.progress`.

use async_trait::async_trait;
use cm_capture::disk::FreeSpace;
use cm_core::{CmError, ErrorCode};
use cm_jobs::{Job, JobContext, JobFilter, JobHandler, JobState, NewJob, JOB_LIBRARY_MOVE};
use cm_library::rec::{LOCK_FILE, OWNER_FILE};
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use crate::action::MoveLibraryParams;
use crate::daemon::{blocking, lock_library, Daemon, Inner};
use crate::engines::SettingsStore;
use crate::error::DaemonError;
use crate::events::{EventBus, LIBRARY_MOVED, LIBRARY_MOVE_STARTED};
use crate::library_ops::human_size;

/// The setting the switch writes (app-only for agents).
pub use crate::config::KEY_LIBRARY_PATH;

/// Suffix of the folder a bundle is copied into before it is verified and renamed.
pub const COPYING_SUFFIX: &str = ".cm-copying";
/// The new folder must have this much free space compared with the library's size.
pub const SPACE_FACTOR: f64 = 1.1;
const CHUNK: usize = 4 * 1024 * 1024;

/// What `move_library` does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MoveMode {
    /// Copy the bundles to the new folder, verify them, then switch.
    Move,
    /// Switch to an empty library in the new folder; the old files stay where they are.
    Fresh,
    /// The new folder already holds a library: switch to it.
    UseExisting,
}

impl MoveMode {
    pub fn as_str(self) -> &'static str {
        match self {
            MoveMode::Move => "move",
            MoveMode::Fresh => "fresh",
            MoveMode::UseExisting => "use_existing",
        }
    }

    pub fn parse(s: &str) -> Option<MoveMode> {
        Some(match s {
            "move" => MoveMode::Move,
            "fresh" => MoveMode::Fresh,
            "use_existing" => MoveMode::UseExisting,
            _ => return None,
        })
    }
}

// ---- problems ------------------------------------------------------------------------------

/// Why a change of the library folder cannot start. Each one has a stable `reason` id, a shared
/// error code and an English message; the app words them itself from the variant.
#[derive(Debug, Clone, PartialEq)]
pub enum MoveProblem {
    RecordingRunning,
    SelftestRunning,
    /// A job is queued or running (`export`, `upload`, `publish`, `transcribe`, ...).
    JobRunning {
        job_type: String,
        job_id: String,
    },
    /// Another `library_move` job is queued, running or paused.
    MoveRunning {
        job_id: String,
    },
    TargetNotAbsolute {
        path: String,
    },
    TargetIsFile {
        path: String,
    },
    TargetNotWritable {
        path: String,
        error: String,
    },
    TargetInsideLibrary {
        path: String,
        library: String,
    },
    TargetContainsLibrary {
        path: String,
        library: String,
    },
    SameFolder {
        path: String,
    },
    NotEnoughSpace {
        needed: u64,
        free: u64,
        size: u64,
    },
    SourceUnreadable {
        path: String,
        error: String,
    },
    /// `mode: move` into a folder that already holds a library.
    TargetHasLibrary {
        path: String,
        bundles: usize,
    },
    /// `mode: use_existing` on a folder with no bundles.
    TargetHasNoLibrary {
        path: String,
    },
}

impl MoveProblem {
    pub fn reason(&self) -> &'static str {
        match self {
            MoveProblem::RecordingRunning => "recording_running",
            MoveProblem::SelftestRunning => "selftest_running",
            MoveProblem::JobRunning { .. } => "job_running",
            MoveProblem::MoveRunning { .. } => "move_running",
            MoveProblem::TargetNotAbsolute { .. } => "target_not_absolute",
            MoveProblem::TargetIsFile { .. } => "target_is_file",
            MoveProblem::TargetNotWritable { .. } => "target_not_writable",
            MoveProblem::TargetInsideLibrary { .. } => "target_inside_library",
            MoveProblem::TargetContainsLibrary { .. } => "target_contains_library",
            MoveProblem::SameFolder { .. } => "same_folder",
            MoveProblem::NotEnoughSpace { .. } => "not_enough_space",
            MoveProblem::SourceUnreadable { .. } => "source_unreadable",
            MoveProblem::TargetHasLibrary { .. } => "target_has_library",
            MoveProblem::TargetHasNoLibrary { .. } => "target_has_no_library",
        }
    }

    pub fn code(&self) -> ErrorCode {
        match self {
            MoveProblem::RecordingRunning
            | MoveProblem::SelftestRunning
            | MoveProblem::JobRunning { .. }
            | MoveProblem::MoveRunning { .. } => ErrorCode::Busy,
            MoveProblem::NotEnoughSpace { .. } => ErrorCode::DiskFull,
            MoveProblem::SourceUnreadable { .. } => ErrorCode::NotFound,
            _ => ErrorCode::InvalidInput,
        }
    }

    /// Whether waiting and asking again can help (the app offers "Check again").
    pub fn is_temporary(&self) -> bool {
        self.code() == ErrorCode::Busy
    }

    pub fn message(&self) -> String {
        match self {
            MoveProblem::RecordingRunning => "A recording is running; stop it before you change the library folder".into(),
            MoveProblem::SelftestRunning => "The 60 fps self-test is running; wait for it to finish before you change the library folder".into(),
            MoveProblem::JobRunning { job_type, job_id } => format!(
                "A {job_type} job is still running (job `{job_id}`); wait for it to finish before you change the library folder"
            ),
            MoveProblem::MoveRunning { job_id } => format!(
                "The library is already being moved (job `{job_id}`)"
            ),
            MoveProblem::TargetNotAbsolute { path } => format!(
                "`{path}` is not an absolute path; give the full path of the new folder"
            ),
            MoveProblem::TargetIsFile { path } => format!("`{path}` is a file, not a folder"),
            MoveProblem::TargetNotWritable { path, error } => format!(
                "CaptureMann cannot write to `{path}` ({error}); choose another folder"
            ),
            MoveProblem::TargetInsideLibrary { path, library } => format!(
                "`{path}` is inside the current library (`{library}`); choose a folder outside it"
            ),
            MoveProblem::TargetContainsLibrary { path, library } => format!(
                "`{path}` contains the current library (`{library}`); choose a folder that does not"
            ),
            MoveProblem::SameFolder { path } => format!("`{path}` is already the library folder"),
            MoveProblem::NotEnoughSpace { needed, free, size } => format!(
                "The new folder needs {} free ({} for the library plus a margin) but has {}",
                human_size(*needed),
                human_size(*size),
                human_size(*free)
            ),
            MoveProblem::SourceUnreadable { path, error } => format!(
                "The current library folder `{path}` cannot be read ({error}); nothing was copied"
            ),
            MoveProblem::TargetHasLibrary { path, bundles } => format!(
                "`{path}` already holds a CaptureMann library ({bundles} captures); use `mode: use_existing` to switch to it, or choose an empty folder"
            ),
            MoveProblem::TargetHasNoLibrary { path } => format!(
                "`{path}` holds no CaptureMann captures; use `mode: move` or `mode: fresh`"
            ),
        }
    }

    /// The typed parameters (`error.data`): `reason` plus what the message names.
    pub fn data(&self) -> Value {
        let mut m = Map::new();
        m.insert("reason".into(), json!(self.reason()));
        match self {
            MoveProblem::JobRunning { job_type, job_id } => {
                m.insert("job_type".into(), json!(job_type));
                m.insert("job_id".into(), json!(job_id));
            }
            MoveProblem::MoveRunning { job_id } => {
                m.insert("job_id".into(), json!(job_id));
            }
            MoveProblem::TargetNotAbsolute { path }
            | MoveProblem::TargetIsFile { path }
            | MoveProblem::SameFolder { path }
            | MoveProblem::TargetHasNoLibrary { path } => {
                m.insert("path".into(), json!(path));
            }
            MoveProblem::TargetNotWritable { path, error }
            | MoveProblem::SourceUnreadable { path, error } => {
                m.insert("path".into(), json!(path));
                m.insert("error".into(), json!(error));
            }
            MoveProblem::TargetInsideLibrary { path, library }
            | MoveProblem::TargetContainsLibrary { path, library } => {
                m.insert("path".into(), json!(path));
                m.insert("library".into(), json!(library));
            }
            MoveProblem::NotEnoughSpace { needed, free, size } => {
                m.insert("needed_bytes".into(), json!(needed));
                m.insert("free_bytes".into(), json!(free));
                m.insert("size_bytes".into(), json!(size));
            }
            MoveProblem::TargetHasLibrary { path, bundles } => {
                m.insert("path".into(), json!(path));
                m.insert("bundles".into(), json!(bundles));
            }
            MoveProblem::RecordingRunning | MoveProblem::SelftestRunning => {}
        }
        Value::Object(m)
    }

    fn cm_error(&self) -> CmError {
        let m = self.message();
        match self.code() {
            ErrorCode::Busy => CmError::busy(m),
            ErrorCode::DiskFull => CmError::disk_full(m),
            ErrorCode::NotFound => CmError::not_found(m),
            _ => CmError::invalid_input(m),
        }
    }
}

impl std::fmt::Display for MoveProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message())
    }
}

impl From<MoveProblem> for DaemonError {
    fn from(p: MoveProblem) -> DaemonError {
        DaemonError::WithData {
            data: p.data(),
            error: p.cm_error(),
        }
    }
}

// ---- sizes and free space ------------------------------------------------------------------

/// Size of a folder tree.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TreeStats {
    pub bytes: u64,
    pub files: u64,
}

/// Whether `name` is a bundle folder name (`.cmrec` or `.cmshot`).
pub fn is_bundle_name(name: &str) -> bool {
    name.ends_with(".cmrec") || name.ends_with(".cmshot")
}

/// Bytes and files of a tree. Symbolic links are neither followed nor counted, so a link that
/// points back up the tree cannot loop. An unreadable entry makes the whole call an error.
pub fn tree_stats(path: &Path) -> std::io::Result<TreeStats> {
    let mut st = TreeStats::default();
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let ty = entry.file_type()?;
            if ty.is_symlink() {
                continue;
            }
            if ty.is_dir() {
                stack.push(entry.path());
            } else if ty.is_file() {
                st.bytes += entry.metadata()?.len();
                st.files += 1;
            }
        }
    }
    Ok(st)
}

/// The bundle folders directly inside a library folder, sorted by name.
pub fn list_bundles(root: &Path) -> std::io::Result<Vec<String>> {
    let mut names = Vec::new();
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if entry.file_type()?.is_dir() && is_bundle_name(&name) {
            names.push(name);
        }
    }
    names.sort();
    Ok(names)
}

/// What a library folder holds: the bundles and their total size (the internal
/// `.capturemann` folder is not counted: the index is rebuilt, the queue stays behind).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryUsage {
    pub bundles: usize,
    pub bytes: u64,
}

pub fn library_usage(root: &Path) -> std::io::Result<LibraryUsage> {
    let names = list_bundles(root)?;
    let mut bytes = 0;
    for n in &names {
        bytes += tree_stats(&root.join(n))?.bytes;
    }
    Ok(LibraryUsage {
        bundles: names.len(),
        bytes,
    })
}

/// The "~/Movies/CaptureMann" form of a path: the home folder shortened to `~`.
pub fn display_path(path: &Path, home: Option<&Path>) -> String {
    if let Some(h) = home.filter(|h| !h.as_os_str().is_empty()) {
        if let Ok(rest) = path.strip_prefix(h) {
            return if rest.as_os_str().is_empty() {
                "~".to_owned()
            } else {
                format!("~/{}", rest.display())
            };
        }
    }
    path.display().to_string()
}

// ---- paths ---------------------------------------------------------------------------------

/// `p` made absolute and free of `.` and `..`, with the part that exists resolved through
/// symbolic links (the rest, which does not exist yet, is appended as written).
pub fn resolve(p: &Path) -> PathBuf {
    let mut lexical = PathBuf::new();
    for c in p.components() {
        match c {
            Component::ParentDir => {
                lexical.pop();
            }
            Component::CurDir => {}
            other => lexical.push(other.as_os_str()),
        }
    }
    let mut existing = lexical.clone();
    let mut tail: Vec<std::ffi::OsString> = Vec::new();
    loop {
        if let Ok(real) = std::fs::canonicalize(&existing) {
            let mut out = real;
            for t in tail.iter().rev() {
                out.push(t);
            }
            return out;
        }
        match (
            existing.file_name().map(ToOwned::to_owned),
            existing.parent(),
        ) {
            (Some(name), Some(parent)) => {
                tail.push(name);
                existing = parent.to_path_buf();
            }
            _ => return lexical,
        }
    }
}

/// The deepest existing ancestor of `p` (or `p` itself).
fn nearest_existing(p: &Path) -> PathBuf {
    let mut cur = p.to_path_buf();
    while !cur.exists() {
        match cur.parent() {
            Some(parent) => cur = parent.to_path_buf(),
            None => break,
        }
    }
    cur
}

/// Proves the folder (or the nearest existing ancestor, when it will be created) accepts files.
fn probe_writable(target: &Path) -> Result<(), String> {
    let dir = nearest_existing(target);
    if !dir.is_dir() {
        return Err("not a folder".to_owned());
    }
    let probe = dir.join(format!(".capturemann-write-test-{}", std::process::id()));
    let r = std::fs::File::create(&probe).and_then(|mut f| f.write_all(b"x"));
    let _ = std::fs::remove_file(&probe);
    r.map_err(|e| e.to_string())
}

// ---- the check -----------------------------------------------------------------------------

/// The facts [`check`] needs from the running daemon.
pub struct CheckInput<'a> {
    pub from: &'a Path,
    pub to: &'a Path,
    pub mode: MoveMode,
    pub recording_active: bool,
    pub selftest_running: bool,
    /// `(job type, job id)` of every queued or running job.
    pub active_jobs: &'a [(String, String)],
    pub free: &'a dyn FreeSpace,
}

/// What a change would do.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MovePlan {
    pub from: PathBuf,
    pub to: PathBuf,
    pub mode: MoveMode,
    /// Bytes that are copied (`move`), or that the new library already holds (`use_existing`).
    pub bytes: u64,
    /// Bundles that are copied or that the new library already holds.
    pub bundles: usize,
    pub free_bytes: Option<u64>,
    /// The folder already holds bundles.
    pub to_has_library: bool,
    pub to_bundles: usize,
}

/// Every precondition, in the order the user meets them; nothing is copied or created (the
/// write test leaves no file behind).
pub fn check(i: &CheckInput<'_>) -> Result<MovePlan, MoveProblem> {
    if i.recording_active {
        return Err(MoveProblem::RecordingRunning);
    }
    if i.selftest_running {
        return Err(MoveProblem::SelftestRunning);
    }
    if let Some((_, id)) = i.active_jobs.iter().find(|(t, _)| t == JOB_LIBRARY_MOVE) {
        return Err(MoveProblem::MoveRunning { job_id: id.clone() });
    }
    if let Some((t, id)) = i.active_jobs.first() {
        return Err(MoveProblem::JobRunning {
            job_type: t.clone(),
            job_id: id.clone(),
        });
    }
    let shown = i.to.display().to_string();
    if !i.to.is_absolute() {
        return Err(MoveProblem::TargetNotAbsolute { path: shown });
    }
    let (from, to) = (resolve(i.from), resolve(i.to));
    if to == from {
        return Err(MoveProblem::SameFolder { path: shown });
    }
    if to.starts_with(&from) {
        return Err(MoveProblem::TargetInsideLibrary {
            path: shown,
            library: from.display().to_string(),
        });
    }
    if from.starts_with(&to) {
        return Err(MoveProblem::TargetContainsLibrary {
            path: shown,
            library: from.display().to_string(),
        });
    }
    if to.exists() && !to.is_dir() {
        return Err(MoveProblem::TargetIsFile { path: shown });
    }
    // A file in the way of a folder that must be created.
    if let Some(anc) = to.ancestors().find(|a| a.exists()) {
        if !anc.is_dir() {
            return Err(MoveProblem::TargetIsFile {
                path: anc.display().to_string(),
            });
        }
    }
    if let Err(error) = probe_writable(&to) {
        return Err(MoveProblem::TargetNotWritable { path: shown, error });
    }
    let (to_bundles, to_has_library) = match list_bundles(&to) {
        Ok(b) => (b.len(), !b.is_empty()),
        Err(_) => (0, false),
    };
    match i.mode {
        MoveMode::Move if to_has_library => {
            return Err(MoveProblem::TargetHasLibrary {
                path: shown,
                bundles: to_bundles,
            })
        }
        MoveMode::UseExisting if !to_has_library => {
            return Err(MoveProblem::TargetHasNoLibrary { path: shown })
        }
        _ => {}
    }
    let free_bytes = i.free.free_bytes(&nearest_existing(&to));
    let (bundles, bytes) = match i.mode {
        MoveMode::Move => {
            let u = library_usage(i.from).map_err(|e| MoveProblem::SourceUnreadable {
                path: i.from.display().to_string(),
                error: e.to_string(),
            })?;
            let needed = (u.bytes as f64 * SPACE_FACTOR).ceil() as u64;
            if let Some(free) = free_bytes {
                if free < needed {
                    return Err(MoveProblem::NotEnoughSpace {
                        needed,
                        free,
                        size: u.bytes,
                    });
                }
            }
            (u.bundles, u.bytes)
        }
        MoveMode::Fresh => (0, 0),
        MoveMode::UseExisting => {
            let u = library_usage(&to).unwrap_or(LibraryUsage {
                bundles: 0,
                bytes: 0,
            });
            (u.bundles, u.bytes)
        }
    };
    Ok(MovePlan {
        from: i.from.to_path_buf(),
        to: i.to.to_path_buf(),
        mode: i.mode,
        bytes,
        bundles,
        free_bytes,
        to_has_library,
        to_bundles,
    })
}

// ---- copying and verifying -----------------------------------------------------------------

/// Copies one file. A trait so tests can break a copy (an unplugged drive, a bad write).
pub trait FileCopier: Send + Sync {
    /// Copies `from` to `to` (created or replaced), calling `progress` with the bytes written
    /// so far in each step and stopping with `ErrorKind::Interrupted` when `interrupted()`.
    fn copy_file(
        &self,
        from: &Path,
        to: &Path,
        interrupted: &dyn Fn() -> bool,
        progress: &mut dyn FnMut(u64),
    ) -> std::io::Result<()>;
}

/// Copies in 4 MiB chunks, flushes the file to disk and keeps the modification time.
#[derive(Debug, Clone, Copy, Default)]
pub struct StdCopier;

impl FileCopier for StdCopier {
    fn copy_file(
        &self,
        from: &Path,
        to: &Path,
        interrupted: &dyn Fn() -> bool,
        progress: &mut dyn FnMut(u64),
    ) -> std::io::Result<()> {
        let mut src = std::fs::File::open(from)?;
        let modified = src.metadata()?.modified().ok();
        let mut dst = std::fs::File::create(to)?;
        let mut buf = vec![0u8; CHUNK];
        loop {
            if interrupted() {
                return Err(std::io::ErrorKind::Interrupted.into());
            }
            let n = src.read(&mut buf)?;
            if n == 0 {
                break;
            }
            dst.write_all(&buf[..n])?;
            progress(n as u64);
        }
        dst.sync_all()?;
        if let Some(m) = modified {
            let _ = dst.set_modified(m);
        }
        Ok(())
    }
}

/// Relative path to byte size of every regular file under `root` (symbolic links skipped).
fn listing(root: &Path) -> std::io::Result<BTreeMap<PathBuf, u64>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let ty = entry.file_type()?;
            if ty.is_symlink() {
                continue;
            }
            let path = entry.path();
            if ty.is_dir() {
                stack.push(path);
            } else if ty.is_file() {
                let rel = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
                out.insert(rel, entry.metadata()?.len());
            }
        }
    }
    Ok(out)
}

/// Why a copy of a bundle does not match its original.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mismatch {
    Unreadable(String),
    FileCount {
        source: usize,
        copy: usize,
    },
    Missing(String),
    Extra(String),
    Size {
        file: String,
        source: u64,
        copy: u64,
    },
    Manifest(String),
}

impl std::fmt::Display for Mismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Mismatch::Unreadable(e) => write!(f, "cannot be read: {e}"),
            Mismatch::FileCount { source, copy } => {
                write!(f, "{copy} files in the copy, {source} in the original")
            }
            Mismatch::Missing(p) => write!(f, "`{p}` is missing from the copy"),
            Mismatch::Extra(p) => write!(f, "`{p}` is only in the copy"),
            Mismatch::Size { file, source, copy } => {
                write!(
                    f,
                    "`{file}` is {copy} bytes in the copy, {source} in the original"
                )
            }
            Mismatch::Manifest(e) => write!(f, "manifest.json: {e}"),
        }
    }
}

/// Checks a copied bundle against its original: the same files with the same sizes, and
/// `manifest.json` byte for byte (it must parse in the copy when it parses in the original).
pub fn verify_copy(src: &Path, dst: &Path) -> Result<TreeStats, Mismatch> {
    let a = listing(src).map_err(|e| Mismatch::Unreadable(format!("{}: {e}", src.display())))?;
    let b = listing(dst).map_err(|e| Mismatch::Unreadable(format!("{}: {e}", dst.display())))?;
    if a.len() != b.len() {
        return Err(Mismatch::FileCount {
            source: a.len(),
            copy: b.len(),
        });
    }
    for (rel, size) in &a {
        match b.get(rel) {
            None => return Err(Mismatch::Missing(rel.display().to_string())),
            Some(s) if s != size => {
                return Err(Mismatch::Size {
                    file: rel.display().to_string(),
                    source: *size,
                    copy: *s,
                })
            }
            Some(_) => {}
        }
    }
    if let Some(rel) = b.keys().find(|k| !a.contains_key(*k)) {
        return Err(Mismatch::Extra(rel.display().to_string()));
    }
    let manifest = Path::new("manifest.json");
    if a.contains_key(manifest) {
        let original = std::fs::read(src.join(manifest))
            .map_err(|e| Mismatch::Manifest(format!("original: {e}")))?;
        let copy = std::fs::read(dst.join(manifest))
            .map_err(|e| Mismatch::Manifest(format!("copy: {e}")))?;
        if original != copy {
            return Err(Mismatch::Manifest("differs from the original".into()));
        }
        if serde_json::from_slice::<Value>(&original).is_ok()
            && serde_json::from_slice::<Value>(&copy).is_err()
        {
            return Err(Mismatch::Manifest("does not parse in the copy".into()));
        }
    }
    Ok(TreeStats {
        bytes: a.values().sum(),
        files: a.len() as u64,
    })
}

/// How copying one bundle ended.
#[derive(Debug)]
pub enum CopyError {
    /// The job was paused, cancelled or shut down.
    Interrupted,
    Io(String),
    Mismatch(Mismatch),
}

fn io_message(e: &std::io::Error) -> String {
    e.to_string()
}

/// Copies `src` to `<dst>.cm-copying`, verifies it and renames it to `dst`. A `dst` that is
/// already there (a copy that no longer matches) is replaced only after the new copy verified.
/// `skipped_links` counts symbolic links that were not copied.
pub fn copy_bundle(
    src: &Path,
    dst: &Path,
    copier: &dyn FileCopier,
    interrupted: &dyn Fn() -> bool,
    progress: &mut dyn FnMut(u64),
    skipped_links: &mut usize,
) -> Result<TreeStats, CopyError> {
    let name = dst.file_name().map(|n| n.to_string_lossy().into_owned());
    let tmp = dst.with_file_name(format!("{}{COPYING_SUFFIX}", name.unwrap_or_default()));
    if tmp.exists() {
        std::fs::remove_dir_all(&tmp).map_err(|e| CopyError::Io(io_message(&e)))?;
    }
    let result = copy_tree(src, &tmp, copier, interrupted, progress, skipped_links);
    if let Err(e) = result {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err(e);
    }
    let stats = match verify_copy(src, &tmp) {
        Ok(s) => s,
        Err(m) => {
            let _ = std::fs::remove_dir_all(&tmp);
            return Err(CopyError::Mismatch(m));
        }
    };
    if dst.exists() {
        std::fs::remove_dir_all(dst).map_err(|e| CopyError::Io(io_message(&e)))?;
    }
    std::fs::rename(&tmp, dst).map_err(|e| CopyError::Io(io_message(&e)))?;
    Ok(stats)
}

fn copy_tree(
    src: &Path,
    dst: &Path,
    copier: &dyn FileCopier,
    interrupted: &dyn Fn() -> bool,
    progress: &mut dyn FnMut(u64),
    skipped_links: &mut usize,
) -> Result<(), CopyError> {
    let io = |e: std::io::Error| {
        if e.kind() == std::io::ErrorKind::Interrupted {
            CopyError::Interrupted
        } else {
            CopyError::Io(io_message(&e))
        }
    };
    std::fs::create_dir_all(dst).map_err(io)?;
    for entry in std::fs::read_dir(src).map_err(io)? {
        let entry = entry.map_err(io)?;
        let ty = entry.file_type().map_err(io)?;
        let (from, to) = (entry.path(), dst.join(entry.file_name()));
        if ty.is_symlink() {
            *skipped_links += 1;
        } else if ty.is_dir() {
            copy_tree(&from, &to, copier, interrupted, progress, skipped_links)?;
        } else if ty.is_file() {
            if interrupted() {
                return Err(CopyError::Interrupted);
            }
            copier
                .copy_file(&from, &to, interrupted, progress)
                .map_err(io)?;
        }
    }
    Ok(())
}

fn is_recording(bundle: &Path) -> bool {
    bundle.join(LOCK_FILE).exists() || bundle.join(OWNER_FILE).exists()
}

/// Removes the `*.cm-copying` leftovers of an interrupted attempt in the new folder.
pub fn clean_copying(to: &Path) {
    let Ok(rd) = std::fs::read_dir(to) else {
        return;
    };
    for e in rd.flatten() {
        if e.file_name().to_string_lossy().ends_with(COPYING_SUFFIX) {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
}

// ---- the job -------------------------------------------------------------------------------

/// What the copy produced.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MoveOutcome {
    /// Bundles in the new folder that are verified against the old one (the only ones the
    /// Trash step may touch).
    pub copied: Vec<String>,
    /// `(name, reason)`: left in the old folder (`recording`).
    pub skipped: Vec<(String, String)>,
    pub bytes: u64,
    pub skipped_links: usize,
}

/// Copies every bundle of `from` into `to` (see the module docs). `progress(done, total, name,
/// index, count)` is called as bytes are written. Errors are job failures with a readable
/// message; an interruption returns `Err(None)`.
pub fn run_copy(
    from: &Path,
    to: &Path,
    copier: &dyn FileCopier,
    interrupted: &dyn Fn() -> bool,
    progress: &mut dyn FnMut(u64, u64, &str, usize, usize),
    checkpoint: &mut dyn FnMut(&[String]),
) -> Result<MoveOutcome, Option<CmError>> {
    let fail = |m: String| Some(CmError::internal(m));
    let left_alone = "Nothing was deleted from the old folder.";
    let names = list_bundles(from).map_err(|e| {
        Some(CmError::not_found(format!(
            "The library folder `{}` cannot be read ({e}). {left_alone} Reconnect the drive and retry.",
            from.display()
        )))
    })?;
    std::fs::create_dir_all(to).map_err(|e| {
        if e.kind() == std::io::ErrorKind::StorageFull {
            Some(CmError::disk_full(format!(
                "The new folder `{}` is full ({e}). {left_alone}",
                to.display()
            )))
        } else {
            fail(format!(
                "Cannot create `{}` ({e}). {left_alone}",
                to.display()
            ))
        }
    })?;
    clean_copying(to);
    let mut total = 0u64;
    for n in &names {
        total += tree_stats(&from.join(n)).map(|s| s.bytes).unwrap_or(0);
    }
    let mut out = MoveOutcome::default();
    let mut done = 0u64;
    let mut handled: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut pass = names;
    for _round in 0..4 {
        let count = out.copied.len() + pass.len();
        for name in &pass {
            if interrupted() {
                clean_copying(to);
                return Err(None);
            }
            handled.insert(name.clone());
            let (src, dst) = (from.join(name), to.join(name));
            if is_recording(&src) {
                out.skipped.push((name.clone(), "recording".to_owned()));
                continue;
            }
            let index = out.copied.len() + 1;
            // A copy that is already there and matches the original is not copied again.
            if dst.is_dir() {
                if let Ok(s) = verify_copy(&src, &dst) {
                    done += s.bytes;
                    out.bytes += s.bytes;
                    out.copied.push(name.clone());
                    progress(done, total, name, index, count);
                    checkpoint(&out.copied);
                    continue;
                }
            }
            let mut attempt = 0;
            let stats = loop {
                attempt += 1;
                let before = done;
                let mut local = 0u64;
                let r = copy_bundle(
                    &src,
                    &dst,
                    copier,
                    interrupted,
                    &mut |n| {
                        local += n;
                        progress(before + local, total, name, index, count);
                    },
                    &mut out.skipped_links,
                );
                match r {
                    Ok(s) => break s,
                    Err(CopyError::Interrupted) => {
                        clean_copying(to);
                        return Err(None);
                    }
                    // Once more: the original may have been saved while it was copied.
                    Err(CopyError::Mismatch(m)) if attempt < 2 => {
                        let _ = m;
                        continue;
                    }
                    Err(CopyError::Mismatch(m)) => {
                        clean_copying(to);
                        return Err(fail(format!(
                            "The copy of `{name}` does not match the original ({m}). {left_alone} Retry the move."
                        )));
                    }
                    Err(CopyError::Io(e)) => {
                        clean_copying(to);
                        let full = e.to_ascii_lowercase().contains("no space");
                        let msg = format!(
                            "Copying `{name}` failed ({e}). {left_alone} Reconnect the drive or free some space, then retry; the captures already copied are kept."
                        );
                        return Err(Some(if full {
                            CmError::disk_full(msg)
                        } else {
                            CmError::internal(msg)
                        }));
                    }
                }
            };
            done += stats.bytes;
            out.bytes += stats.bytes;
            out.copied.push(name.clone());
            progress(done, total, name, index, count);
            checkpoint(&out.copied);
        }
        // Captures that appeared in the old folder while it was copied.
        let now = list_bundles(from).unwrap_or_default();
        pass = now.into_iter().filter(|n| !handled.contains(n)).collect();
        if pass.is_empty() {
            break;
        }
    }
    Ok(out)
}

/// The executor of the job type `library_move`.
pub struct LibraryMoveHandler {
    pub settings: Arc<dyn SettingsStore>,
    pub events: EventBus,
    pub copier: Arc<dyn FileCopier>,
}

fn inputs_of(job: &Job) -> Result<(PathBuf, PathBuf), CmError> {
    let get = |k: &str| {
        job.inputs
            .get(k)
            .and_then(Value::as_str)
            .map(PathBuf::from)
            .ok_or_else(|| CmError::internal(format!("the job has no `{k}`")))
    };
    Ok((get("from")?, get("to")?))
}

#[async_trait]
impl JobHandler for LibraryMoveHandler {
    async fn run(&self, ctx: JobContext) -> Result<Value, CmError> {
        let (from, to) = inputs_of(&ctx.job)?;
        let (copier, reporter, cancel) = (self.copier.clone(), ctx.reporter.clone(), ctx.cancel);
        let (f2, t2) = (from.clone(), to.clone());
        let c2 = cancel.clone();
        let reporter2 = reporter.clone();
        let outcome = tokio::task::spawn_blocking(move || {
            let mut last = std::time::Instant::now() - std::time::Duration::from_secs(1);
            run_copy(
                &f2,
                &t2,
                copier.as_ref(),
                &|| c2.is_interrupted(),
                &mut |done, total, name, index, count| {
                    if last.elapsed() < std::time::Duration::from_millis(150) && done < total {
                        return;
                    }
                    last = std::time::Instant::now();
                    let frac = if total == 0 {
                        1.0
                    } else {
                        done as f64 / total as f64
                    };
                    reporter.report(
                        frac.min(0.99),
                        Some(format!("Copying {index} of {count}: {name}")),
                    );
                },
                &mut |copied| {
                    let _ = reporter2.save_checkpoint(&json!({ "copied": copied }));
                },
            )
        })
        .await
        .map_err(|e| CmError::internal(format!("the copy stopped unexpectedly: {e}")))?;
        let out = match outcome {
            Ok(o) => o,
            // Interrupted: the queue decides the final state from the interrupt kind.
            Err(None) => return Err(CmError::internal("the move was interrupted")),
            Err(Some(e)) => return Err(e),
        };
        // Everything verified: only now does the library setting change.
        let mut values = Map::new();
        values.insert(
            KEY_LIBRARY_PATH.to_owned(),
            json!(to.to_string_lossy().into_owned()),
        );
        self.settings.set(&values).map_err(|e| {
            CmError::internal(format!(
                "The captures were copied, but the library setting could not be saved ({e}). Nothing was deleted; retry the move."
            ))
        })?;
        let skipped: Vec<Value> = out
            .skipped
            .iter()
            .map(|(n, r)| json!({ "name": n, "reason": r }))
            .collect();
        let result = json!({
            "from": from, "to": to, "mode": "move",
            "bundles": out.copied.len(), "bytes": out.bytes,
            "copied": out.copied, "skipped": skipped,
            "skipped_links": out.skipped_links,
            "restart_required": true,
            "warnings": env_warning(),
        });
        self.events.publish(
            LIBRARY_MOVED,
            json!({ "from": from, "to": to, "mode": "move", "bundles": out.copied.len(),
                    "bytes": out.bytes, "skipped": skipped, "restart_required": true }),
        );
        Ok(result)
    }

    async fn on_cancel(&self, job: &Job) {
        if let Ok((_, to)) = inputs_of(job) {
            clean_copying(&to);
        }
    }
}

fn env_warning() -> Vec<String> {
    if std::env::var_os("CAPTUREMANN_LIBRARY").is_some_and(|v| !v.is_empty()) {
        vec!["CAPTUREMANN_LIBRARY is set in this process and overrides the library setting; unset it to use the new folder".to_owned()]
    } else {
        Vec::new()
    }
}

// ---- the daemon side -----------------------------------------------------------------------

/// What the Confirm dialog shows for a `move_library`.
pub(crate) struct MovePrepared {
    pub plan: MovePlan,
    pub summary: String,
    pub details: Value,
    pub params: Value,
}

fn plan_text(plan: &MovePlan) -> (String, Value, Value) {
    let (from, to) = (
        plan.from.display().to_string(),
        plan.to.display().to_string(),
    );
    let size = human_size(plan.bytes);
    let summary = match plan.mode {
        MoveMode::Move => format!(
            "Move the library from {from} to {to} ({size}, {} captures)",
            plan.bundles
        ),
        MoveMode::Fresh => format!(
            "Start a new empty library at {to}; the captures in {from} stay where they are"
        ),
        MoveMode::UseExisting => format!(
            "Switch to the existing library at {to} ({} captures); the captures in {from} stay where they are",
            plan.bundles
        ),
    };
    let mut rows = Map::new();
    rows.insert("from".into(), json!(from));
    rows.insert("to".into(), json!(to));
    if plan.mode != MoveMode::Fresh {
        rows.insert("size".into(), json!(size));
        rows.insert("captures".into(), json!(plan.bundles));
    }
    rows.insert(
        "result".into(),
        json!(match plan.mode {
            MoveMode::Move =>
                "The captures are copied and checked; nothing is deleted from the old folder",
            MoveMode::Fresh => "Nothing is copied; the old files stay where they are",
            MoveMode::UseExisting => "Nothing is copied; the old files stay where they are",
        }),
    );
    rows.insert(
        "restart".into(),
        json!("CaptureMann starts using the new folder after it restarts"),
    );
    let params = json!({
        "mode": plan.mode.as_str(), "from": from, "to": to,
        "size_bytes": plan.bytes, "bundles": plan.bundles,
    });
    (summary, Value::Object(rows), params)
}

impl Inner {
    fn active_job_list(&self) -> Result<Vec<(String, String)>, CmError> {
        let Some(rt) = &self.jobs else {
            return Ok(Vec::new());
        };
        let mut jobs = rt.queue.list(&JobFilter {
            job_type: None,
            states: Some(vec![JobState::Queued, JobState::Running]),
            limit: Some(200),
        })?;
        // A paused move still owns the folder.
        jobs.extend(rt.queue.list(&JobFilter {
            job_type: Some(JOB_LIBRARY_MOVE.to_owned()),
            states: Some(vec![JobState::Paused]),
            limit: Some(10),
        })?);
        Ok(jobs.into_iter().map(|j| (j.job_type, j.id)).collect())
    }

    pub(crate) async fn check_library_move(
        &self,
        to: &Path,
        mode: MoveMode,
    ) -> Result<MovePlan, DaemonError> {
        let from = lock_library(&self.library).root().to_path_buf();
        let to = to.to_path_buf();
        let jobs = self.active_job_list()?;
        let recording = self
            .recording
            .as_ref()
            .is_some_and(crate::recording::RecordingEngine::is_active);
        let selftest = self
            .recording
            .as_ref()
            .is_some_and(crate::recording::RecordingEngine::selftest_running);
        let free = self.free_space.clone();
        let r = blocking(move || {
            Ok(check(&CheckInput {
                from: &from,
                to: &to,
                mode,
                recording_active: recording,
                selftest_running: selftest,
                active_jobs: &jobs,
                free: free.as_ref(),
            }))
        })
        .await?;
        r.map_err(DaemonError::from)
    }

    pub(crate) async fn plan_move_library(
        &self,
        p: &MoveLibraryParams,
    ) -> Result<MovePrepared, DaemonError> {
        if p.mode == MoveMode::Move && self.jobs.is_none() {
            return Err(DaemonError::NotImplemented(
                crate::jobs::NO_QUEUE.to_owned(),
            ));
        }
        let plan = self.check_library_move(Path::new(&p.path), p.mode).await?;
        let (summary, details, params) = plan_text(&plan);
        Ok(MovePrepared {
            plan,
            summary,
            details,
            params,
        })
    }

    pub(crate) async fn run_move_library(
        &self,
        prepared: MovePrepared,
    ) -> Result<Value, DaemonError> {
        let MovePrepared { plan, .. } = prepared;
        let base = json!({
            "mode": plan.mode.as_str(), "from": plan.from, "to": plan.to,
            "bytes": plan.bytes, "bundles": plan.bundles, "restart_required": true,
            "warnings": env_warning(),
        });
        let merge = |extra: Value| {
            let mut v = base.clone();
            if let (Some(o), Some(e)) = (v.as_object_mut(), extra.as_object()) {
                o.extend(e.clone());
            }
            v
        };
        if plan.mode == MoveMode::Move {
            let rt = self.job_queue()?;
            let queue = rt.queue.clone();
            let inputs = json!({ "from": plan.from, "to": plan.to });
            let job =
                blocking(move || queue.enqueue(NewJob::new(JOB_LIBRARY_MOVE, inputs))).await?;
            self.events.publish(
                LIBRARY_MOVE_STARTED,
                json!({ "job_id": job.id, "from": plan.from, "to": plan.to,
                        "bundles": plan.bundles, "bytes": plan.bytes }),
            );
            return Ok(merge(json!({ "job_id": job.id, "state": "queued" })));
        }
        // `fresh` and `use_existing`: only the setting changes.
        let settings = self.settings.clone();
        let to = plan.to.clone();
        blocking(move || {
            let mut values = Map::new();
            values.insert(KEY_LIBRARY_PATH.to_owned(), json!(to.to_string_lossy()));
            settings.set(&values).map(|_| ())
        })
        .await?;
        self.events.publish(
            LIBRARY_MOVED,
            json!({ "from": plan.from, "to": plan.to, "mode": plan.mode.as_str(),
                    "bundles": plan.bundles, "bytes": plan.bytes, "skipped": [],
                    "restart_required": true }),
        );
        Ok(merge(json!({ "job_id": Value::Null, "state": "done" })))
    }
}

/// What the Trash step did.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct TrashReport {
    pub trashed: Vec<String>,
    /// `(name, reason)`: `not_a_bundle`, `no_copy`, `changed`, `recording`, `trash_failed`.
    pub kept: Vec<(String, String)>,
}

impl Daemon {
    /// Registers the executor of the job type `library_move` (next to the other handlers, before
    /// [`Daemon::start_jobs`]).
    pub fn register_library_move_handler(&self) -> Result<(), DaemonError> {
        self.register_job_handler(JOB_LIBRARY_MOVE, self.library_move_handler())
    }

    pub fn library_move_handler(&self) -> Arc<dyn JobHandler> {
        Arc::new(LibraryMoveHandler {
            settings: self.inner.settings.clone(),
            events: self.inner.events.clone(),
            copier: self.inner.move_copier.clone(),
        })
    }

    /// What changing the library folder to `to` would do, or why it cannot be done now: the same
    /// checks `move_library` makes before its dialog. For the app, which asks the user itself.
    pub async fn check_library_move(
        &self,
        to: &Path,
        mode: MoveMode,
    ) -> Result<MovePlan, MoveProblem> {
        match self.inner.check_library_move(to, mode).await {
            Ok(p) => Ok(p),
            Err(DaemonError::WithData { data, .. }) => Err(problem_from_data(&data)),
            Err(e) => Err(MoveProblem::SourceUnreadable {
                path: to.display().to_string(),
                error: e.to_string(),
            }),
        }
    }

    /// Moves the old copies of a finished move to the Trash (never deletes). Each name must be a
    /// bundle of `from` whose copy in `to` still matches it; anything else is kept and listed.
    pub async fn trash_old_copies(
        &self,
        from: &Path,
        to: &Path,
        names: &[String],
    ) -> Result<TrashReport, DaemonError> {
        let (from, to, names) = (from.to_path_buf(), to.to_path_buf(), names.to_vec());
        let trasher = self.inner.trasher.clone();
        blocking(move || {
            let mut report = TrashReport::default();
            if resolve(&from) == resolve(&to) {
                return Err(CmError::invalid_input(
                    "the old and the new folder are the same; nothing was moved to the Trash",
                ));
            }
            for name in names {
                let keep =
                    |r: &mut TrashReport, why: &str| r.kept.push((name.clone(), why.to_owned()));
                if !is_bundle_name(&name) || name.contains(['/', '\\']) || name.starts_with('.') {
                    keep(&mut report, "not_a_bundle");
                    continue;
                }
                let (src, dst) = (from.join(&name), to.join(&name));
                if !src.is_dir() {
                    keep(&mut report, "not_found");
                    continue;
                }
                if is_recording(&src) {
                    keep(&mut report, "recording");
                    continue;
                }
                if !dst.is_dir() {
                    keep(&mut report, "no_copy");
                    continue;
                }
                if verify_copy(&src, &dst).is_err() {
                    keep(&mut report, "changed");
                    continue;
                }
                match trasher.trash(&src) {
                    Ok(()) => report.trashed.push(name),
                    Err(_) => keep(&mut report, "trash_failed"),
                }
            }
            Ok(report)
        })
        .await
    }
}

/// Rebuilds a problem from its `data` (what [`MoveProblem::data`] wrote).
fn problem_from_data(d: &Value) -> MoveProblem {
    let s = |k: &str| {
        d.get(k)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    let n = |k: &str| d.get(k).and_then(Value::as_u64).unwrap_or_default();
    match d.get("reason").and_then(Value::as_str).unwrap_or_default() {
        "recording_running" => MoveProblem::RecordingRunning,
        "selftest_running" => MoveProblem::SelftestRunning,
        "job_running" => MoveProblem::JobRunning {
            job_type: s("job_type"),
            job_id: s("job_id"),
        },
        "move_running" => MoveProblem::MoveRunning {
            job_id: s("job_id"),
        },
        "target_not_absolute" => MoveProblem::TargetNotAbsolute { path: s("path") },
        "target_is_file" => MoveProblem::TargetIsFile { path: s("path") },
        "target_not_writable" => MoveProblem::TargetNotWritable {
            path: s("path"),
            error: s("error"),
        },
        "target_inside_library" => MoveProblem::TargetInsideLibrary {
            path: s("path"),
            library: s("library"),
        },
        "target_contains_library" => MoveProblem::TargetContainsLibrary {
            path: s("path"),
            library: s("library"),
        },
        "same_folder" => MoveProblem::SameFolder { path: s("path") },
        "not_enough_space" => MoveProblem::NotEnoughSpace {
            needed: n("needed_bytes"),
            free: n("free_bytes"),
            size: n("size_bytes"),
        },
        "target_has_library" => MoveProblem::TargetHasLibrary {
            path: s("path"),
            bundles: n("bundles") as usize,
        },
        "target_has_no_library" => MoveProblem::TargetHasNoLibrary { path: s("path") },
        _ => MoveProblem::SourceUnreadable {
            path: s("path"),
            error: s("error"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cm_capture::disk::FakeFreeSpace;

    fn bundle(root: &Path, name: &str, files: &[(&str, &[u8])]) {
        let d = root.join(name);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("manifest.json"), br#"{"schema":1}"#).unwrap();
        for (f, body) in files {
            let p = d.join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, body).unwrap();
        }
    }

    fn input<'a>(
        from: &'a Path,
        to: &'a Path,
        mode: MoveMode,
        free: &'a FakeFreeSpace,
    ) -> CheckInput<'a> {
        CheckInput {
            from,
            to,
            mode,
            recording_active: false,
            selftest_running: false,
            active_jobs: &[],
            free,
        }
    }

    #[test]
    fn size_walk_counts_bundles_and_ignores_symlink_loops() {
        let d = tempfile::tempdir().unwrap();
        bundle(d.path(), "a.cmrec", &[("track.mp4", &[0u8; 1000])]);
        bundle(d.path(), "b.cmshot", &[("original.png", &[0u8; 500])]);
        std::fs::create_dir_all(d.path().join(".capturemann")).unwrap();
        std::fs::write(d.path().join(".capturemann/index.sqlite"), [0u8; 9999]).unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(d.path(), d.path().join("a.cmrec/loop")).unwrap();
        }
        let u = library_usage(d.path()).unwrap();
        assert_eq!(u.bundles, 2);
        assert_eq!(u.bytes, 2 * 12 + 1500);
        assert!(tree_stats(Path::new("/definitely/not/here")).is_err());
    }

    #[test]
    fn home_is_shortened() {
        let h = Path::new("/Users/a");
        assert_eq!(
            display_path(Path::new("/Users/a/Movies/CaptureMann"), Some(h)),
            "~/Movies/CaptureMann"
        );
        assert_eq!(display_path(Path::new("/Users/a"), Some(h)), "~");
        assert_eq!(display_path(Path::new("/Volumes/X"), Some(h)), "/Volumes/X");
        assert_eq!(
            display_path(Path::new("/Users/ab/x"), Some(h)),
            "/Users/ab/x"
        );
    }

    #[test]
    fn every_precondition_has_its_own_typed_problem() {
        let lib = tempfile::tempdir().unwrap();
        bundle(lib.path(), "a.cmrec", &[("t", &[1u8; 1000])]);
        let other = tempfile::tempdir().unwrap();
        let free = FakeFreeSpace::new(u64::MAX);
        let reason = |to: &Path, mode| match check(&input(lib.path(), to, mode, &free)) {
            Ok(_) => "ok",
            Err(e) => e.reason(),
        };
        assert_eq!(reason(other.path(), MoveMode::Move), "ok");
        assert_eq!(
            reason(Path::new("relative/x"), MoveMode::Move),
            "target_not_absolute"
        );
        assert_eq!(reason(lib.path(), MoveMode::Move), "same_folder");
        let dotted = lib.path().join("sub/../");
        assert_eq!(reason(&dotted, MoveMode::Move), "same_folder");
        assert_eq!(
            reason(&lib.path().join("inner"), MoveMode::Move),
            "target_inside_library"
        );
        assert_eq!(
            reason(lib.path().parent().unwrap(), MoveMode::Move),
            "target_contains_library"
        );
        let file = other.path().join("f.txt");
        std::fs::write(&file, b"x").unwrap();
        assert_eq!(reason(&file, MoveMode::Move), "target_is_file");
        assert_eq!(reason(&file.join("sub"), MoveMode::Move), "target_is_file");
        // A folder that does not exist yet is fine.
        assert_eq!(
            reason(&other.path().join("new/deeper"), MoveMode::Move),
            "ok"
        );
        // Existing library there.
        bundle(other.path(), "z.cmshot", &[]);
        assert_eq!(reason(other.path(), MoveMode::Move), "target_has_library");
        assert_eq!(reason(other.path(), MoveMode::UseExisting), "ok");
        assert_eq!(reason(other.path(), MoveMode::Fresh), "ok");
        let empty = tempfile::tempdir().unwrap();
        assert_eq!(
            reason(empty.path(), MoveMode::UseExisting),
            "target_has_no_library"
        );
        // Space: 1.1 x the library size.
        let tight = FakeFreeSpace::new(1_000);
        let e = check(&input(lib.path(), empty.path(), MoveMode::Move, &tight)).unwrap_err();
        assert_eq!(e.reason(), "not_enough_space");
        assert_eq!(e.code(), ErrorCode::DiskFull);
        let enough = FakeFreeSpace::new((1012.0 * 1.1) as u64 + 1);
        assert!(check(&input(lib.path(), empty.path(), MoveMode::Move, &enough)).is_ok());
        // `fresh` needs no space and no readable old library.
        assert!(check(&input(
            Path::new("/nonexistent/lib"),
            empty.path(),
            MoveMode::Fresh,
            &tight
        ))
        .is_ok());
        let e = check(&input(
            Path::new("/nonexistent/lib"),
            empty.path(),
            MoveMode::Move,
            &free,
        ))
        .unwrap_err();
        assert_eq!(e.reason(), "source_unreadable");
        assert_eq!(e.code(), ErrorCode::NotFound);
    }

    #[test]
    fn busy_problems_come_first_and_are_temporary() {
        let lib = tempfile::tempdir().unwrap();
        let to = tempfile::tempdir().unwrap();
        let free = FakeFreeSpace::new(u64::MAX);
        let mut i = input(lib.path(), to.path(), MoveMode::Move, &free);
        i.recording_active = true;
        assert_eq!(check(&i).unwrap_err(), MoveProblem::RecordingRunning);
        i.recording_active = false;
        i.selftest_running = true;
        assert_eq!(check(&i).unwrap_err(), MoveProblem::SelftestRunning);
        i.selftest_running = false;
        let jobs = vec![("export".to_owned(), "j1".to_owned())];
        i.active_jobs = &jobs;
        let e = check(&i).unwrap_err();
        assert!(matches!(&e, MoveProblem::JobRunning { job_type, .. } if job_type == "export"));
        assert!(e.is_temporary());
        assert_eq!(e.code(), ErrorCode::Busy);
        let jobs = vec![
            ("upload".to_owned(), "u".to_owned()),
            (JOB_LIBRARY_MOVE.to_owned(), "m".to_owned()),
        ];
        i.active_jobs = &jobs;
        assert_eq!(check(&i).unwrap_err().reason(), "move_running");
        // Round trip through the error data.
        for p in [
            MoveProblem::NotEnoughSpace {
                needed: 3,
                free: 2,
                size: 1,
            },
            MoveProblem::TargetInsideLibrary {
                path: "a".into(),
                library: "b".into(),
            },
            MoveProblem::JobRunning {
                job_type: "upload".into(),
                job_id: "j".into(),
            },
        ] {
            assert_eq!(problem_from_data(&p.data()), p);
        }
    }

    #[test]
    fn copy_verifies_resumes_and_never_touches_the_source() {
        let from = tempfile::tempdir().unwrap();
        let to = tempfile::tempdir().unwrap();
        bundle(
            from.path(),
            "a.cmrec",
            &[("tracks/screen.mp4", &[7u8; 5000])],
        );
        bundle(from.path(), "b.cmshot", &[("original.png", &[9u8; 300])]);
        let mut seen = Vec::new();
        let out = run_copy(
            from.path(),
            to.path(),
            &StdCopier,
            &|| false,
            &mut |d, t, n, i, c| seen.push((d, t, n.to_owned(), i, c)),
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(out.copied, ["a.cmrec", "b.cmshot"]);
        assert!(seen.last().unwrap().0 == seen.last().unwrap().1);
        assert!(verify_copy(&from.path().join("a.cmrec"), &to.path().join("a.cmrec")).is_ok());
        assert!(!to.path().join("a.cmrec.cm-copying").exists());
        // Resume: a corrupted finished bundle is redone, a half-copied one is cleaned.
        std::fs::write(to.path().join("a.cmrec/tracks/screen.mp4"), [0u8; 10]).unwrap();
        std::fs::create_dir_all(to.path().join("c.cmrec.cm-copying")).unwrap();
        let out = run_copy(
            from.path(),
            to.path(),
            &StdCopier,
            &|| false,
            &mut |_, _, _, _, _| {},
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(out.copied.len(), 2);
        assert!(!to.path().join("c.cmrec.cm-copying").exists());
        assert!(verify_copy(&from.path().join("a.cmrec"), &to.path().join("a.cmrec")).is_ok());
        assert_eq!(
            std::fs::read(from.path().join("a.cmrec/tracks/screen.mp4"))
                .unwrap()
                .len(),
            5000
        );
    }

    /// Breaks the copy of one file name: truncates it silently, or fails like an unplugged drive.
    struct Bad {
        file: &'static str,
        fail: bool,
    }
    impl FileCopier for Bad {
        fn copy_file(
            &self,
            from: &Path,
            to: &Path,
            interrupted: &dyn Fn() -> bool,
            progress: &mut dyn FnMut(u64),
        ) -> std::io::Result<()> {
            StdCopier.copy_file(from, to, interrupted, progress)?;
            if from.file_name().is_some_and(|n| n == self.file) {
                if self.fail {
                    return Err(std::io::Error::other("Input/output error"));
                }
                let f = std::fs::OpenOptions::new().write(true).open(to)?;
                f.set_len(1)?;
            }
            Ok(())
        }
    }

    #[test]
    fn a_corrupted_copy_is_detected_and_a_failing_drive_is_a_clean_error() {
        let from = tempfile::tempdir().unwrap();
        let to = tempfile::tempdir().unwrap();
        bundle(from.path(), "a.cmrec", &[("screen.mp4", &[7u8; 5000])]);
        let e = run_copy(
            from.path(),
            to.path(),
            &Bad {
                file: "screen.mp4",
                fail: false,
            },
            &|| false,
            &mut |_, _, _, _, _| {},
            &mut |_| {},
        )
        .unwrap_err()
        .unwrap();
        assert!(e.to_string().contains("does not match the original"), "{e}");
        assert!(
            !to.path().join("a.cmrec").exists(),
            "a bad copy never gets its real name"
        );
        assert!(!to.path().join("a.cmrec.cm-copying").exists());
        let e = run_copy(
            from.path(),
            to.path(),
            &Bad {
                file: "screen.mp4",
                fail: true,
            },
            &|| false,
            &mut |_, _, _, _, _| {},
            &mut |_| {},
        )
        .unwrap_err()
        .unwrap();
        assert!(
            e.to_string()
                .contains("Nothing was deleted from the old folder"),
            "{e}"
        );
        assert!(from.path().join("a.cmrec/screen.mp4").exists());
    }

    #[test]
    fn cancel_keeps_finished_bundles_and_leaves_the_old_library_alone() {
        let from = tempfile::tempdir().unwrap();
        let to = tempfile::tempdir().unwrap();
        for n in ["a.cmrec", "b.cmrec", "c.cmrec"] {
            bundle(from.path(), n, &[("t", &[1u8; 100])]);
        }
        let done = std::cell::Cell::new(0usize);
        let r = run_copy(
            from.path(),
            to.path(),
            &StdCopier,
            &|| done.get() >= 2,
            &mut |_, _, _, _, _| {},
            &mut |c| done.set(c.len()),
        );
        assert!(matches!(r, Err(None)));
        assert_eq!(list_bundles(to.path()).unwrap(), ["a.cmrec", "b.cmrec"]);
        assert_eq!(list_bundles(from.path()).unwrap().len(), 3);
    }

    #[test]
    fn a_bundle_being_recorded_is_skipped_and_links_are_not_followed() {
        let from = tempfile::tempdir().unwrap();
        let to = tempfile::tempdir().unwrap();
        bundle(from.path(), "live.cmrec", &[("recording.lock", b"1")]);
        bundle(from.path(), "ok.cmrec", &[("t", &[1u8; 10])]);
        #[cfg(unix)]
        std::os::unix::fs::symlink(from.path(), from.path().join("ok.cmrec/up")).unwrap();
        let out = run_copy(
            from.path(),
            to.path(),
            &StdCopier,
            &|| false,
            &mut |_, _, _, _, _| {},
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(out.copied, ["ok.cmrec"]);
        assert_eq!(
            out.skipped,
            [("live.cmrec".to_owned(), "recording".to_owned())]
        );
        #[cfg(unix)]
        {
            assert_eq!(out.skipped_links, 1);
            assert!(!to.path().join("ok.cmrec/up").exists());
        }
        assert!(!to.path().join("live.cmrec").exists());
    }

    #[test]
    fn verify_names_what_differs() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        bundle(a.path(), "x", &[("f", b"abc")]);
        bundle(b.path(), "x", &[("f", b"abc")]);
        assert!(verify_copy(&a.path().join("x"), &b.path().join("x")).is_ok());
        std::fs::write(b.path().join("x/extra"), b"1").unwrap();
        assert!(matches!(
            verify_copy(&a.path().join("x"), &b.path().join("x")),
            Err(Mismatch::FileCount { .. })
        ));
        std::fs::remove_file(b.path().join("x/extra")).unwrap();
        std::fs::write(b.path().join("x/f"), b"abcd").unwrap();
        assert!(matches!(
            verify_copy(&a.path().join("x"), &b.path().join("x")),
            Err(Mismatch::Size { .. })
        ));
        std::fs::write(b.path().join("x/f"), b"abc").unwrap();
        std::fs::write(b.path().join("x/manifest.json"), b"{\"schema\":2}").unwrap();
        assert!(matches!(
            verify_copy(&a.path().join("x"), &b.path().join("x")),
            Err(Mismatch::Manifest(_))
        ));
    }
}
