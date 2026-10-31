//! `.cmrec` recording bundles: creation, finalization and crash recovery (REC-8, NFR-6).
//!
//! ```text
//! <title>.cmrec/
//! ├─ recording.lock   exists while the recording runs; holds what recovery needs to rebuild
//! │                   the manifest (id, title, settings, pid, pause gaps)
//! ├─ session.json     clock origin, displays, devices, scale factors, app version
//! ├─ screen.mp4 ...   tracks, written by their recorders directly into the folder
//! └─ manifest.json    written once, at stop (finalize) or at recovery; its presence plus the
//!                     absence of the lock means "complete"
//! ```
//!
//! Life cycle: [`RecBundle::create`] writes `session.json` and the lock; recorders add their
//! files; [`RecBundle::finalize`] writes the manifest and removes the lock. If the app dies in
//! between, the next launch calls `Library::recover_incomplete`, which runs [`recover_bundle`]:
//! every fragmented MP4 is truncated to its last complete fragment (`cm_encode::fmp4`), JSONL
//! telemetry to its last whole line, durations are rebuilt from the fragments, the manifest is
//! written and the lock removed. A track that cannot be recovered is kept out of the media list
//! and marked `"missing": true`, the others are kept (REC-8 edge case).
//!
//! The recovery logic is pure file work on one folder (no index, no clock besides the `now` it
//! is given), so it is tested with truncated fixtures from `cm_encode::fmp4::synth`.

use std::fs;
use std::path::{Path, PathBuf};

use cm_core::{
    Actor, CmError, MediaTime, PauseGap, PhysicalRect, Revision, ScaleFactor, TrackInfo,
};
use cm_encode::fmp4;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use time::OffsetDateTime;

use crate::bundle::{create_unique_dir, MANIFEST_FILE};
use crate::manifest::{CaptureKind, RecManifest, SCHEMA_VERSION};
use crate::util::{io_err, json_err, new_id, sanitize_title, write_atomic};

pub const LOCK_FILE: &str = "recording.lock";
/// Reserved `RecBundle::set_extra` key: its value becomes `edits[0].layout` in the manifest.
pub const LAYOUT_KEY: &str = "layout";
/// Manifest field listing rendered exports (`exports/` folder), see [`record_export`].
pub const EXPORTS_KEY: &str = "exports";
pub const SESSION_FILE: &str = "session.json";
pub const TICK_RATE: u32 = 120_000;
/// Fragment length; recovery loses at most this much per track (NFR-6, REC-8).
pub const FRAGMENT_MS: i64 = 2000;

/// One display as it was during the recording (physical px, scale factor).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionDisplay {
    pub id: u32,
    pub name: String,
    pub bounds: PhysicalRect,
    pub scale: ScaleFactor,
    pub is_primary: bool,
}

/// `session.json`: facts about the machine and clock during the recording.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionInfo {
    /// Session clock origin in host nanoseconds (macOS: `CLOCK_UPTIME_RAW`).
    pub clock_origin_ns: u64,
    /// Which host timebase `clock_origin_ns` is on, e.g. `"uptime_raw_ns"`.
    pub timebase: String,
    pub displays: Vec<SessionDisplay>,
    /// Cameras, microphones and the like (names and ids), free-form per track crate.
    #[serde(default)]
    pub devices: Value,
    pub app_version: String,
    /// Anything else a track wants kept (for example `cursor.clicks: "unavailable"`, REC-7.AC4).
    #[serde(default, flatten)]
    pub extra: Map<String, Value>,
}

impl SessionInfo {
    fn to_json(&self) -> Result<String, CmError> {
        let v = json!({
            "schema": SCHEMA_VERSION,
            "clock": {
                "origin_ns": self.clock_origin_ns,
                "timebase": self.timebase,
                "tick_rate": TICK_RATE,
            },
            "displays": self.displays,
            "devices": self.devices,
            "app_version": self.app_version,
        });
        let mut v = v;
        if let Some(o) = v.as_object_mut() {
            for (k, val) in &self.extra {
                o.entry(k.clone()).or_insert_with(|| val.clone());
            }
        }
        serde_json::to_string_pretty(&v).map_err(|e| json_err(SESSION_FILE, &e))
    }
}

/// Contents of `recording.lock`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct LockFile {
    schema: u32,
    id: String,
    title: String,
    #[serde(with = "time::serde::rfc3339")]
    created: OffsetDateTime,
    tick_rate: u32,
    /// Process that owns the recording; recovery leaves a live owner's bundle alone.
    pid: u32,
    app_version: String,
    #[serde(default)]
    settings: Value,
    /// Closed pause gaps so far, in session time.
    #[serde(default)]
    gaps: Vec<PauseGap>,
    /// Warnings raised while recording (a track failed to start, a device was lost, ...); copied
    /// to `manifest.warnings` at finalize and recovery.
    #[serde(default)]
    warnings: Vec<Value>,
    /// Extra top-level manifest fields (for example `live_at`), copied at finalize and recovery.
    #[serde(default)]
    extra: Map<String, Value>,
    /// Per track facts only the running recorder knows (audio: first-sample session time, file
    /// names, channels, rate), so recovery can place a track the way the live path would have
    /// (REC-8.AC2, R3-2).
    #[serde(default)]
    tracks: Map<String, Value>,
    /// Set when recovery found nothing usable: later launches skip the folder instead of
    /// retrying it forever (R3-5). The files stay for the user.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    recovery_failed: Option<String>,
}

fn read_lock(dir: &Path) -> Result<LockFile, CmError> {
    let p = dir.join(LOCK_FILE);
    let text = fs::read_to_string(&p).map_err(|e| io_err("cannot read", &p, &e))?;
    serde_json::from_str(&text).map_err(|e| CmError::invalid_input(format!("{LOCK_FILE}: {e}")))
}

/// File whose advisory lock the recording process holds for as long as the bundle is open. The
/// kernel drops the lock when the process dies, so "is the owner alive" never depends on a pid
/// that may have been reused (R3-17).
pub const OWNER_FILE: &str = "recording.owner";

/// Holds the exclusive lock on [`OWNER_FILE`] (`flock` on Unix, `LockFileEx` on Windows, both
/// through `File::try_lock`).
#[derive(Debug)]
struct OwnerGuard {
    _file: fs::File,
}

fn take_owner(dir: &Path) -> Option<OwnerGuard> {
    let f = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join(OWNER_FILE))
        .ok()?;
    f.try_lock().ok()?;
    Some(OwnerGuard { _file: f })
}

/// Is the process that owns the recording in `dir` still running? Uses the owner file's lock
/// when the bundle has one (immune to pid reuse), else the pid in the lock file.
pub(crate) fn owner_alive(dir: &Path) -> bool {
    if let Ok(f) = fs::File::open(dir.join(OWNER_FILE)) {
        return match f.try_lock() {
            // We got it, so nobody holds it: the owner is gone. Released again at once.
            Ok(()) => {
                let _ = f.unlock();
                false
            }
            Err(fs::TryLockError::WouldBlock) => true,
            Err(fs::TryLockError::Error(_)) => false,
        };
    }
    lock_pid(dir).is_some_and(pid_alive)
}

/// An open recording bundle: the folder recorders write into, until it is finalized.
#[derive(Debug)]
pub struct RecBundle {
    dir: PathBuf,
    lock: LockFile,
    _owner: Option<OwnerGuard>,
}

impl RecBundle {
    /// Creates `<parent>/<title>.cmrec/` (with ` (2)`... if taken) holding `session.json` and the
    /// lock. Nothing is indexed until [`RecBundle::finalize`].
    pub fn create(
        parent: &Path,
        title: &str,
        now: OffsetDateTime,
        settings: Value,
        session: &SessionInfo,
    ) -> Result<Self, CmError> {
        let base = sanitize_title(title)?;
        fs::create_dir_all(parent).map_err(|e| io_err("cannot create", parent, &e))?;
        let dir = create_unique_dir(parent, &base, CaptureKind::Recording)?;
        let lock = LockFile {
            schema: SCHEMA_VERSION,
            id: new_id("rec", now),
            title: title.trim().to_owned(),
            created: now,
            tick_rate: TICK_RATE,
            pid: std::process::id(),
            app_version: session.app_version.clone(),
            settings,
            gaps: Vec::new(),
            warnings: Vec::new(),
            extra: Map::new(),
            tracks: Map::new(),
            recovery_failed: None,
        };
        // The owner lock comes first: a scan that sees the lock file finds the owner held.
        let owner = take_owner(&dir);
        let res = (|| {
            write_atomic(&dir.join(SESSION_FILE), session.to_json()?.as_bytes())?;
            write_lock(&dir, &lock)
        })();
        match res {
            Ok(()) => Ok(RecBundle {
                dir,
                lock,
                _owner: owner,
            }),
            Err(e) => {
                let _ = fs::remove_dir_all(&dir);
                Err(e)
            }
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn id(&self) -> &str {
        &self.lock.id
    }

    pub fn title(&self) -> &str {
        &self.lock.title
    }

    /// Records a closed pause gap in the lock, so recovery can map file time back to session
    /// time. Call on every resume (and with the final gap when stopping while paused).
    pub fn add_gap(&mut self, gap: PauseGap) -> Result<(), CmError> {
        self.lock.gaps.push(gap);
        write_lock(&self.dir, &self.lock)
    }

    /// Records a warning (kept in the lock, so it survives a crash) that ends up in
    /// `manifest.warnings`.
    pub fn add_warning(&mut self, warning: Value) -> Result<(), CmError> {
        self.lock.warnings.push(warning);
        write_lock(&self.dir, &self.lock)
    }

    /// Sets an extra top-level manifest field (kept in the lock, so recovery keeps it too).
    pub fn set_extra(&mut self, key: &str, value: Value) -> Result<(), CmError> {
        self.lock.extra.insert(key.to_owned(), value);
        write_lock(&self.dir, &self.lock)
    }

    /// Keeps a track's recovery facts in the lock (see `LockFile::tracks`): the first-sample
    /// session time of an audio track, its file names, channels and rate. Idempotent.
    pub fn set_track_hint(&mut self, kind: &str, hint: Value) -> Result<(), CmError> {
        self.lock.tracks.insert(kind.to_owned(), hint);
        write_lock(&self.dir, &self.lock)
    }

    /// Rewrites `session.json`: the session fills in what it only knows after the recorders
    /// started (devices, the moment all tracks were live).
    pub fn write_session(&self, session: &SessionInfo) -> Result<(), CmError> {
        write_atomic(&self.dir.join(SESSION_FILE), session.to_json()?.as_bytes())
    }

    /// Writes `manifest.json` from the finished tracks and removes the lock (completed marker).
    pub fn finalize(
        self,
        tracks: &[TrackInfo],
        now: OffsetDateTime,
    ) -> Result<RecManifest, CmError> {
        let mut entries = Map::new();
        for t in tracks {
            entries.insert(t.kind.as_str().to_owned(), track_entry(t));
        }
        let end = tracks
            .iter()
            .map(|t| t.end)
            .max()
            .unwrap_or(MediaTime::ZERO);
        let manifest = build_manifest(
            &self.lock,
            entries,
            session_to_file(&self.lock.gaps, end),
            now,
        );
        write_manifest(&self.dir, &manifest)?;
        remove_lock(&self.dir)?;
        Ok(manifest)
    }
}

fn write_lock(dir: &Path, lock: &LockFile) -> Result<(), CmError> {
    let text = serde_json::to_string_pretty(lock).map_err(|e| json_err(LOCK_FILE, &e))?;
    write_atomic(&dir.join(LOCK_FILE), text.as_bytes())
}

fn remove_lock(dir: &Path) -> Result<(), CmError> {
    let _ = fs::remove_file(dir.join(OWNER_FILE));
    let p = dir.join(LOCK_FILE);
    match fs::remove_file(&p) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(io_err("cannot remove", &p, &e)),
    }
}

pub(crate) fn write_manifest(dir: &Path, m: &RecManifest) -> Result<(), CmError> {
    let text = serde_json::to_string_pretty(m).map_err(|e| json_err(MANIFEST_FILE, &e))?;
    write_atomic(&dir.join(MANIFEST_FILE), text.as_bytes())
}

/// The manifest `tracks.<kind>` entry for a finished track: `file`, `start`, `end` (session
/// ticks), `stats`, plus everything in `details` (size, fps, codec, ...).
pub fn track_entry(t: &TrackInfo) -> Value {
    let mut o = Map::new();
    o.insert("file".into(), json!(t.file));
    if let Some(d) = t.details.as_object() {
        for (k, v) in d {
            o.insert(k.clone(), v.clone());
        }
    }
    o.insert("start".into(), json!(t.start.ticks()));
    o.insert("end".into(), json!(t.end.ticks()));
    o.insert("stats".into(), json!(t.stats));
    Value::Object(o)
}

/// Total recorded time (session end minus closed pause gaps that precede it).
fn session_to_file(gaps: &[PauseGap], end: MediaTime) -> MediaTime {
    let removed: i64 = gaps
        .iter()
        .filter(|g| g.to <= end)
        .map(|g| g.to.ticks() - g.from.ticks())
        .sum();
    MediaTime::from_ticks((end.ticks() - removed).max(0))
}

/// Session time for a file time: the inverse of dropping the gaps.
fn file_to_session(gaps: &[PauseGap], file: MediaTime) -> MediaTime {
    let mut t = file.ticks();
    for g in gaps {
        if g.from.ticks() <= t {
            t += g.to.ticks() - g.from.ticks();
        }
    }
    MediaTime::from_ticks(t)
}

/// Session time for the end of a file's last sample. Like [`file_to_session`], but an end that
/// sits exactly at a gap's start stays before the gap (nothing was recorded after it).
fn file_end_to_session(gaps: &[PauseGap], file: MediaTime) -> MediaTime {
    let mut t = file.ticks();
    for g in gaps {
        if g.from.ticks() < t {
            t += g.to.ticks() - g.from.ticks();
        }
    }
    MediaTime::from_ticks(t)
}

fn build_manifest(
    lock: &LockFile,
    tracks: Map<String, Value>,
    duration: MediaTime,
    now: OffsetDateTime,
) -> RecManifest {
    let mut extra = lock.extra.clone();
    if !lock.warnings.is_empty() {
        extra.insert("warnings".into(), Value::Array(lock.warnings.clone()));
    }
    // The reserved extra key `layout` (set with `RecBundle::set_extra("layout", ...)`) is the
    // layout at stop, e.g. `{ "camera": { visible, shape, rect } }`; it belongs to the edit list.
    let layout = extra.remove(LAYOUT_KEY);
    // VOC-1: `kind` and the live markers are typed manifest fields; the session keeps them in
    // the lock's extra map while recording so a crash does not lose them.
    let kind = extra
        .remove("kind")
        .and_then(|v| v.as_str().map(str::to_owned));
    let markers: Vec<crate::manifest::Marker> = extra
        .remove("markers")
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();
    let mut edit = json!({"in": 0, "out": duration.ticks(), "speed": 1.0});
    if let (Some(l), Some(o)) = (layout, edit.as_object_mut()) {
        o.insert("layout".into(), l);
    }
    RecManifest {
        schema: SCHEMA_VERSION,
        id: lock.id.clone(),
        title: lock.title.clone(),
        created: lock.created,
        tick_rate: Some(lock.tick_rate),
        duration: Some(duration),
        tracks,
        // One full-length segment at normal speed; trims, cuts and layouts are edits (EDT-*).
        edits: vec![edit],
        revisions: vec![Revision {
            rev: 1,
            action: "record".into(),
            by: Actor::User,
            at: now,
        }],
        settings: lock.settings.clone(),
        gaps: lock.gaps.clone(),
        captions: json!({"enabled": false, "burn_in": false}),
        kind,
        markers,
        extra,
    }
}

// ---- exports --------------------------------------------------------------------------------

/// Reads and parses `manifest.json` of a `.cmrec` folder.
pub fn read_manifest(dir: &Path) -> Result<RecManifest, CmError> {
    let p = dir.join(MANIFEST_FILE);
    let text = fs::read_to_string(&p).map_err(|e| io_err("cannot read", &p, &e))?;
    RecManifest::from_json(&text).map_err(crate::manifest::ManifestProblem::into_error)
}

/// The `exports` entries of a manifest (`[{ file, preset, created, duration_ms, size_bytes,
/// encoder, settings }]`), oldest first.
pub fn exports_of(m: &RecManifest) -> Vec<Value> {
    m.extra
        .get(EXPORTS_KEY)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

/// Records a finished export in the manifest, atomically: replaces the entry with the same
/// `preset` (an export overwrites only the same preset's previous file, EXP-1) or appends one, and
/// adds an `export` revision by `by`. Returns the written manifest.
pub fn record_export(
    dir: &Path,
    entry: Value,
    by: Actor,
    now: OffsetDateTime,
) -> Result<RecManifest, CmError> {
    let mut m = read_manifest(dir)?;
    let mut list = exports_of(&m);
    let preset = entry
        .get("preset")
        .and_then(Value::as_str)
        .map(str::to_owned);
    match preset {
        Some(p) => {
            if let Some(slot) = list
                .iter_mut()
                .find(|e| e.get("preset").and_then(Value::as_str) == Some(p.as_str()))
            {
                *slot = entry;
            } else {
                list.push(entry);
            }
        }
        None => list.push(entry),
    }
    m.extra.insert(EXPORTS_KEY.to_owned(), Value::Array(list));
    let rev = m.revisions.last().map_or(0, |r| r.rev) + 1;
    m.revisions.push(Revision {
        rev,
        action: "export".into(),
        by,
        at: now,
    });
    write_manifest(dir, &m)?;
    Ok(m)
}

// ---- upload records (STO-4.AC3) ---------------------------------------------------------

/// Manifest key of the upload records of a capture (either kind of bundle).
pub const UPLOADS_KEY: &str = "uploads";

fn read_manifest_value(dir: &Path) -> Result<Value, CmError> {
    let p = dir.join(MANIFEST_FILE);
    let text = fs::read_to_string(&p).map_err(|e| io_err("cannot read", &p, &e))?;
    serde_json::from_str(&text).map_err(|e| json_err(MANIFEST_FILE, &e))
}

/// The `uploads` records of a bundle (`.cmrec` or `.cmshot`), oldest first: `{ destination_id,
/// destination, what, remote_id, bytes, share_link, at }`. A bundle without any, or one whose
/// manifest cannot be read, has none.
pub fn uploads_of(dir: &Path) -> Vec<Value> {
    read_manifest_value(dir)
        .ok()
        .and_then(|v| v.get(UPLOADS_KEY).and_then(Value::as_array).cloned())
        .unwrap_or_default()
}

/// Records a finished upload in the manifest, atomically. It is bookkeeping about the copy, not
/// an edit: no revision is added, so there is nothing to undo. An earlier record with the same
/// destination, `what` and `remote_id` (the same remote file sent again) is replaced.
pub fn record_upload(dir: &Path, entry: Value) -> Result<(), CmError> {
    let mut m = read_manifest_value(dir)?;
    let Some(obj) = m.as_object_mut() else {
        return Err(CmError::invalid_input("manifest.json is not an object"));
    };
    let mut list = obj
        .get(UPLOADS_KEY)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let same = |a: &Value, b: &Value| {
        ["destination_id", "what", "remote_id"]
            .iter()
            .all(|k| a.get(*k) == b.get(*k))
    };
    match list.iter_mut().find(|e| same(e, &entry)) {
        Some(slot) => *slot = entry,
        None => list.push(entry),
    }
    obj.insert(UPLOADS_KEY.to_owned(), Value::Array(list));
    let text = serde_json::to_string_pretty(&m).map_err(|e| json_err(MANIFEST_FILE, &e))?;
    write_atomic(&dir.join(MANIFEST_FILE), text.as_bytes())
}

// ---- publish records (PUB-2) ------------------------------------------------------------

/// Manifest key of the publish records of a recording.
pub const PUBLISHES_KEY: &str = "publishes";

/// The `publishes` records of a bundle, oldest first: `{ platform, account_id, account,
/// remote_id, url, title, privacy, privacy_requested, status, warnings, at }`. A bundle without
/// any, or one whose manifest cannot be read, has none.
pub fn publishes_of(dir: &Path) -> Vec<Value> {
    read_manifest_value(dir)
        .ok()
        .and_then(|v| v.get(PUBLISHES_KEY).and_then(Value::as_array).cloned())
        .unwrap_or_default()
}

/// Records a finished publish in the manifest, atomically. Like [`record_upload`] it is
/// bookkeeping, not an edit: no revision, nothing to undo. A record with the same platform,
/// account and remote id replaces the earlier one (a retried job that finds the video again).
pub fn record_publish(dir: &Path, entry: Value) -> Result<(), CmError> {
    let mut m = read_manifest_value(dir)?;
    let Some(obj) = m.as_object_mut() else {
        return Err(CmError::invalid_input("manifest.json is not an object"));
    };
    let mut list = obj
        .get(PUBLISHES_KEY)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let same = |a: &Value, b: &Value| {
        ["platform", "account_id", "remote_id"]
            .iter()
            .all(|k| a.get(*k) == b.get(*k))
    };
    match list.iter_mut().find(|e| same(e, &entry)) {
        Some(slot) => *slot = entry,
        None => list.push(entry),
    }
    obj.insert(PUBLISHES_KEY.to_owned(), Value::Array(list));
    let text = serde_json::to_string_pretty(&m).map_err(|e| json_err(MANIFEST_FILE, &e))?;
    write_atomic(&dir.join(MANIFEST_FILE), text.as_bytes())
}

// ---- remote deletions (delete_capture with also_delete_remote) ---------------------------

/// Manifest key of the outcome of remote deletions.
pub const REMOTE_DELETIONS_KEY: &str = "remote_deletions";

/// At most this many outcomes are kept; the oldest go first.
const REMOTE_DELETIONS_KEEP: usize = 50;

/// The `remote_deletions` entries of a bundle, oldest first. Each is `{ kind: "upload" |
/// "publish", outcome: "deleted" | "already_gone" | "failed", error?, at, ... }` plus the keys of
/// the record it is about (`destination_id`, `what`, `remote_id` for an upload; `platform`,
/// `account_id`, `remote_id` for a publish).
pub fn remote_deletions_of(dir: &Path) -> Vec<Value> {
    read_manifest_value(dir)
        .ok()
        .and_then(|v| {
            v.get(REMOTE_DELETIONS_KEY)
                .and_then(Value::as_array)
                .cloned()
        })
        .unwrap_or_default()
}

/// Writes the outcome of remote deletions, atomically and in one rewrite. Entries with outcome
/// `deleted` or `already_gone` also remove the upload or publish record they name (matching
/// `destination_id`, `what` and `remote_id`, or `platform`, `account_id` and `remote_id`), so a
/// second attempt only meets what is left; a `failed` entry leaves its record in place. Like the
/// other records this is bookkeeping: no revision, nothing to undo.
pub fn record_remote_deletions(dir: &Path, entries: Vec<Value>) -> Result<(), CmError> {
    let mut m = read_manifest_value(dir)?;
    let Some(obj) = m.as_object_mut() else {
        return Err(CmError::invalid_input("manifest.json is not an object"));
    };
    let gone = |e: &Value| matches!(e["outcome"].as_str(), Some("deleted" | "already_gone"));
    let same = |a: &Value, b: &Value, keys: &[&str]| keys.iter().all(|k| a.get(*k) == b.get(*k));
    for (list_key, kind, keys) in [
        (
            UPLOADS_KEY,
            "upload",
            &["destination_id", "what", "remote_id"][..],
        ),
        (
            PUBLISHES_KEY,
            "publish",
            &["platform", "account_id", "remote_id"][..],
        ),
    ] {
        if let Some(list) = obj.get_mut(list_key).and_then(Value::as_array_mut) {
            list.retain(|rec| {
                !entries
                    .iter()
                    .any(|e| gone(e) && e["kind"] == kind && same(rec, e, keys))
            });
        }
    }
    let mut log = obj
        .get(REMOTE_DELETIONS_KEY)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    log.extend(entries);
    if log.len() > REMOTE_DELETIONS_KEEP {
        log.drain(..log.len() - REMOTE_DELETIONS_KEEP);
    }
    obj.insert(REMOTE_DELETIONS_KEY.to_owned(), Value::Array(log));
    let text = serde_json::to_string_pretty(&m).map_err(|e| json_err(MANIFEST_FILE, &e))?;
    write_atomic(&dir.join(MANIFEST_FILE), text.as_bytes())
}

// ---- recovery -----------------------------------------------------------------------------

/// What happened to one track during recovery.
#[derive(Debug, Clone, PartialEq)]
pub enum TrackRecovery {
    /// Already valid: nothing to cut.
    Intact { frames: u64, duration: MediaTime },
    /// Cut back to the last complete fragment or line.
    Truncated {
        removed_bytes: u64,
        frames: u64,
        duration: MediaTime,
    },
    /// Not recoverable; marked missing in the manifest.
    Missing { reason: String },
}

/// Result of recovering one bundle.
#[derive(Debug, Clone, PartialEq)]
pub struct RecoveryReport {
    pub dir: PathBuf,
    pub id: String,
    pub title: String,
    pub created: OffsetDateTime,
    /// Recovered recording length.
    pub duration: MediaTime,
    /// Upper bound of what was lost per track: one fragment (REC-8: "lost <= 2 s").
    pub lost_ms_max: i64,
    pub tracks: Vec<(String, TrackRecovery)>,
    pub manifest: RecManifest,
}

/// Outcome of looking at one bundle with a lock file.
#[derive(Debug, Clone, PartialEq)]
pub enum RecoveryOutcome {
    /// The lock was left behind after the manifest was written: it was just removed.
    AlreadyComplete,
    Recovered(Box<RecoveryReport>),
}

/// Media files a recording bundle may hold, by track name and kind of repair.
const FILES: [(&str, &str, FileKind); 5] = [
    ("screen", "screen.mp4", FileKind::Mp4),
    ("camera", "camera.mp4", FileKind::Mp4),
    ("mic", "mic.wav", FileKind::Wav),
    ("system", "system.wav", FileKind::Wav),
    ("cursor", "cursor.jsonl", FileKind::Jsonl),
];

#[derive(Clone, Copy)]
enum FileKind {
    Mp4,
    Wav,
    Jsonl,
}

/// Recovers one bundle that has a `recording.lock` (REC-8): see the module docs. Errors when
/// there is no usable media at all; the folder is then left as it was.
pub fn recover_bundle(dir: &Path, now: OffsetDateTime) -> Result<RecoveryOutcome, CmError> {
    let manifest_path = dir.join(MANIFEST_FILE);
    if manifest_path.is_file() {
        let text = fs::read_to_string(&manifest_path)
            .map_err(|e| io_err("cannot read", &manifest_path, &e))?;
        if RecManifest::from_json(&text).is_ok() {
            remove_lock(dir)?;
            return Ok(RecoveryOutcome::AlreadyComplete);
        }
    }
    let lock = read_lock(dir).unwrap_or_else(|_| fallback_lock(dir, now));
    // An audio recording (VOC-1) has no video: its WAV files are the media.
    let audio_only = lock.extra.get("kind").and_then(Value::as_str) == Some("audio");
    let mut tracks = Map::new();
    let mut report_tracks = Vec::new();
    let mut duration = MediaTime::ZERO;
    let mut any_media = false;
    for (name, file, kind) in FILES {
        let path = dir.join(file);
        if !path.is_file() {
            continue;
        }
        match recover_file(&path, kind, &lock.gaps, lock.tracks.get(name)) {
            Ok((mut entry, outcome, file_end)) => {
                // R3-6: every track lists the session's pauses under `pauses`, like a live stop.
                if let Some(o) = entry.as_object_mut() {
                    o.entry("pauses").or_insert_with(|| {
                        Value::Array(
                            lock.gaps
                                .iter()
                                .map(|g| json!({"from": g.from.ticks(), "to": g.to.ticks()}))
                                .collect(),
                        )
                    });
                }
                if matches!(kind, FileKind::Mp4) || (audio_only && matches!(kind, FileKind::Wav)) {
                    any_media = true;
                    duration = duration.max(file_end);
                }
                tracks.insert(name.to_owned(), entry);
                report_tracks.push((name.to_owned(), outcome));
            }
            Err(e) => {
                tracks.insert(
                    name.to_owned(),
                    json!({"file": file, "missing": true, "reason": e.to_string()}),
                );
                report_tracks.push((
                    name.to_owned(),
                    TrackRecovery::Missing {
                        reason: e.to_string(),
                    },
                ));
            }
        }
    }
    if !any_media {
        return Err(CmError::invalid_input(format!(
            "`{}` has no recoverable {} track",
            dir.display(),
            if audio_only { "audio" } else { "video" }
        )));
    }
    let mut manifest = build_manifest(&lock, tracks, duration, now);
    manifest.revisions[0].action = "recover".into();
    manifest.extra.insert(
        "recovered".into(),
        json!({
            "at": now.format(&time::format_description::well_known::Rfc3339).unwrap_or_default(),
            "lost_ms_max": FRAGMENT_MS,
        }),
    );
    write_manifest(dir, &manifest)?;
    remove_lock(dir)?;
    Ok(RecoveryOutcome::Recovered(Box::new(RecoveryReport {
        dir: dir.to_owned(),
        id: manifest.id.clone(),
        title: manifest.title.clone(),
        created: manifest.created,
        duration,
        lost_ms_max: FRAGMENT_MS,
        tracks: report_tracks,
        manifest,
    })))
}

/// A lock that cannot be read still marks an unfinished recording: rebuild what we can from the
/// folder name and the file times.
fn fallback_lock(dir: &Path, now: OffsetDateTime) -> LockFile {
    let created = fs::metadata(dir)
        .and_then(|m| m.modified())
        .map_or(now, OffsetDateTime::from);
    LockFile {
        schema: SCHEMA_VERSION,
        id: new_id("rec", created),
        title: dir
            .file_stem()
            .map_or_else(|| "Recording".into(), |s| s.to_string_lossy().into_owned()),
        created,
        tick_rate: TICK_RATE,
        pid: 0,
        app_version: String::new(),
        settings: Value::Null,
        gaps: Vec::new(),
        warnings: Vec::new(),
        extra: Map::new(),
        tracks: Map::new(),
        recovery_failed: None,
    }
}

/// Repairs one file. Returns the manifest entry, what was done and the file-time end.
fn recover_file(
    path: &Path,
    kind: FileKind,
    gaps: &[PauseGap],
    hint: Option<&Value>,
) -> Result<(Value, TrackRecovery, MediaTime), CmError> {
    let file = path
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    match kind {
        FileKind::Mp4 => {
            let (scan, removed) = fmp4::truncate_to_last_fragment(path)?;
            let init = scan
                .init
                .as_ref()
                .ok_or_else(|| CmError::invalid_input("no init segment"))?;
            let (start, end) = (
                scan.start().unwrap_or(MediaTime::ZERO),
                scan.end().unwrap_or(MediaTime::ZERO),
            );
            let duration = end - start;
            let frames = scan.frame_count();
            let fps = if duration.ticks() > 0 {
                (frames as f64 / duration.as_secs_f64()).round() as u32
            } else {
                0
            };
            let codec = match init.codec.as_str() {
                "hvc1" | "hev1" => "hevc",
                "avc1" | "avc3" => "h264",
                other => other,
            };
            let entry = json!({
                "file": file,
                "size": [init.width, init.height],
                "fps": fps,
                "codec": codec,
                "start": file_to_session(gaps, start).ticks(),
                "end": file_end_to_session(gaps, end).ticks(),
                "frames": frames,
                "fragments": scan.fragments.len(),
                "recovered": true,
            });
            let outcome = if removed > 0 {
                TrackRecovery::Truncated {
                    removed_bytes: removed,
                    frames,
                    duration,
                }
            } else {
                TrackRecovery::Intact { frames, duration }
            };
            Ok((entry, outcome, end))
        }
        FileKind::Jsonl => {
            let bytes = fs::read(path).map_err(|e| io_err("cannot read", path, &e))?;
            let keep = bytes.iter().rposition(|b| *b == b'\n').map_or(0, |i| i + 1);
            let removed = (bytes.len() - keep) as u64;
            if removed > 0 {
                let f = fs::OpenOptions::new()
                    .write(true)
                    .open(path)
                    .map_err(|e| io_err("cannot open", path, &e))?;
                f.set_len(keep as u64)
                    .map_err(|e| io_err("cannot truncate", path, &e))?;
            }
            let events = bytes[..keep].iter().filter(|b| **b == b'\n').count();
            let entry = json!({"file": file, "events": events, "recovered": true});
            let outcome = if removed > 0 {
                TrackRecovery::Truncated {
                    removed_bytes: removed,
                    frames: events as u64,
                    duration: MediaTime::ZERO,
                }
            } else {
                TrackRecovery::Intact {
                    frames: events as u64,
                    duration: MediaTime::ZERO,
                }
            };
            Ok((entry, outcome, MediaTime::ZERO))
        }
        FileKind::Wav => recover_wav(path, &file, gaps, hint),
    }
}

/// Recovers an audio track (R3-2): repairs the WAV size fields from the file length (the header
/// can be a second stale, or claim bytes a power loss never wrote), and rebuilds the manifest
/// entry the live path would have written (`start`, `end`, `channels`, the denoised file, the
/// pauses) from the facts the session kept in the lock. Without them the track keeps its old
/// shape (`start_unknown`) rather than being placed wrongly.
fn recover_wav(
    path: &Path,
    file: &str,
    gaps: &[PauseGap],
    hint: Option<&Value>,
) -> Result<(Value, TrackRecovery, MediaTime), CmError> {
    let before = cm_core::wav::read_info(path).ok();
    let info = cm_core::wav::repair_wav(path)
        .map_err(|e| CmError::invalid_input(format!("{file}: not a usable WAV: {e}")))?;
    let removed = before.map_or(0, |b| b.data_bytes.saturating_sub(info.data_bytes));
    // The denoised copy is the track's file when it is there (mic.denoised.wav), so its length
    // is the track's length.
    let mut main = info;
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    // End of the track in file time (pauses removed): the length of an audio recording.
    let mut file_end = MediaTime::ZERO;
    let mut entry = Map::new();
    entry.insert("file".into(), json!(file));
    entry.insert("channels".into(), json!(info.channels));
    entry.insert("rate".into(), json!(info.sample_rate));
    entry.insert("sample_format".into(), json!("f32"));
    entry.insert("recovered".into(), json!(true));
    if let Some(h) = hint {
        // The denoised copy is the track's file when it is there (mic.denoised.wav).
        if let Some(den) = h["denoised_file"].as_str() {
            let dp = dir.join(den);
            if let Some(di) = dp
                .is_file()
                .then(|| cm_core::wav::repair_wav(&dp).ok())
                .flatten()
            {
                main = di;
                entry.insert("file".into(), json!(den));
                entry.insert("raw_file".into(), json!(file));
                entry.insert("denoised_file".into(), json!(den));
                entry.insert("denoise".into(), json!(true));
            }
        }
        let frames = main.frames();
        let dur = MediaTime::from_ticks(
            i64::try_from(frames * u64::from(TICK_RATE) / u64::from(main.sample_rate.max(1)))
                .unwrap_or(i64::MAX),
        );
        if let Some(start) = h["start"].as_i64() {
            let before_start: i64 = gaps
                .iter()
                .filter(|g| g.to.ticks() <= start)
                .map(|g| g.to.ticks() - g.from.ticks())
                .sum();
            let end_file = MediaTime::from_ticks(start - before_start + dur.ticks());
            file_end = end_file;
            let end = file_end_to_session(gaps, end_file).ticks().max(start);
            let pauses: Vec<Value> = gaps
                .iter()
                .filter(|g| g.from.ticks() >= start && g.from.ticks() < end)
                .map(|g| json!({"from": g.from.ticks(), "to": g.to.ticks()}))
                .collect();
            entry.insert("start".into(), json!(start));
            entry.insert("end".into(), json!(end));
            entry.insert("first_sample_ticks".into(), json!(start));
            entry.insert("pauses".into(), Value::Array(pauses));
        } else {
            entry.insert("start_unknown".into(), json!(true));
        }
    } else {
        entry.insert("start_unknown".into(), json!(true));
    }
    let frames = main.frames();
    let dur = MediaTime::from_ticks(
        i64::try_from(frames * u64::from(TICK_RATE) / u64::from(main.sample_rate.max(1)))
            .unwrap_or(i64::MAX),
    );
    let outcome = if removed > 0 {
        TrackRecovery::Truncated {
            removed_bytes: removed,
            frames,
            duration: dur,
        }
    } else {
        TrackRecovery::Intact {
            frames,
            duration: dur,
        }
    };
    entry.insert("frames".into(), json!(frames));
    if file_end == MediaTime::ZERO {
        file_end = dur;
    }
    Ok((Value::Object(entry), outcome, file_end))
}

/// Is a process with this pid running? Used so a second instance does not "recover" a bundle
/// another instance is still recording into.
pub(crate) fn pid_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    #[cfg(unix)]
    {
        let Ok(p) = libc::pid_t::try_from(pid) else {
            return false;
        };
        // SAFETY: signal 0 only checks that the process exists and can be signalled; it sends
        // nothing and has no memory effects.
        let rc = unsafe { libc::kill(p, 0) };
        rc == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }
    #[cfg(not(unix))]
    {
        // Windows: no cheap check without another dependency; treat only our own pid as alive.
        pid == std::process::id()
    }
}

/// Marks a bundle whose recovery found nothing usable, so later launches skip it instead of
/// retrying forever (R3-5). The folder and its files stay.
pub(crate) fn mark_recovery_failed(dir: &Path, reason: &str) {
    if let Ok(mut lock) = read_lock(dir) {
        lock.recovery_failed = Some(reason.to_owned());
        let _ = write_lock(dir, &lock);
    }
}

/// The reason a previous launch gave up on this bundle, if it did.
pub(crate) fn recovery_failure(dir: &Path) -> Option<String> {
    read_lock(dir).ok().and_then(|l| l.recovery_failed)
}

/// The pid recorded in a bundle's lock, if readable.
pub(crate) fn lock_pid(dir: &Path) -> Option<u32> {
    read_lock(dir).ok().map(|l| l.pid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cm_core::TrackKind;
    use cm_encode::fmp4::synth::{fragment, init_segment, Spec};
    use time::macros::datetime;

    const NOW: OffsetDateTime = datetime!(2026-10-21 10:00:00 UTC);

    fn session() -> SessionInfo {
        SessionInfo {
            clock_origin_ns: 123,
            timebase: "uptime_raw_ns".into(),
            displays: vec![],
            devices: json!({}),
            app_version: "0.0.1".into(),
            extra: Map::new(),
        }
    }

    /// A screen.mp4 of `n` 2 s fragments (30 fps) plus `tail` bytes of a cut fragment.
    fn screen_fixture(n: u32, tail: usize) -> Vec<u8> {
        let spec = Spec {
            width: 3024,
            height: 1964,
            codec: *b"hvc1",
            ..Default::default()
        };
        let mut v = init_segment(&spec);
        for i in 0..n {
            v.extend(fragment(&spec, i + 1, u64::from(i) * 240_000, 60, 4000, 64));
        }
        if tail > 0 {
            let cut = fragment(&spec, n + 1, u64::from(n) * 240_000, 60, 4000, 64);
            v.extend(&cut[..tail.min(cut.len() - 1)]);
        }
        v
    }

    fn crashed_bundle(tmp: &Path, n: u32, tail: usize) -> RecBundle {
        let b = RecBundle::create(tmp, "Lesson", NOW, json!({"fps": 30}), &session()).unwrap();
        fs::write(b.dir().join("screen.mp4"), screen_fixture(n, tail)).unwrap();
        b
    }

    #[test]
    fn warnings_and_extras_reach_the_manifest_on_finalize_and_on_recovery() {
        let tmp = tempfile::tempdir().unwrap();
        let mut b = crashed_bundle(tmp.path(), 2, 0);
        b.add_warning(json!({"track": "camera", "code": "camera_failed"}))
            .unwrap();
        b.set_extra("live_at", json!(240)).unwrap();
        let mut s = session();
        s.extra.insert("session".into(), json!({"live_at": 240}));
        b.write_session(&s).unwrap();
        let sj: Value =
            serde_json::from_str(&fs::read_to_string(b.dir().join(SESSION_FILE)).unwrap()).unwrap();
        assert_eq!(sj["session"]["live_at"], 240);
        // Crash: recovery keeps both.
        let dir = b.dir().to_owned();
        let RecoveryOutcome::Recovered(r) = recover_bundle(&dir, NOW).unwrap() else {
            panic!("expected a recovery");
        };
        assert_eq!(r.manifest.extra["live_at"], 240);
        assert_eq!(r.manifest.extra["warnings"][0]["code"], "camera_failed");
        // Clean stop: finalize keeps both too.
        let mut b2 = RecBundle::create(tmp.path(), "Other", NOW, json!({}), &session()).unwrap();
        b2.add_warning(json!({"code": "disk_low"})).unwrap();
        b2.set_extra("live_at", json!(7)).unwrap();
        let m = b2.finalize(&[], NOW).unwrap();
        assert_eq!(m.extra["live_at"], 7);
        assert_eq!(m.extra["warnings"].as_array().unwrap().len(), 1);
    }

    /// The layout at stop (`layout` extra) lands in `edits[0].layout` and survives recovery.
    #[test]
    fn layout_at_stop_goes_into_the_first_edit() {
        let tmp = tempfile::tempdir().unwrap();
        let layout =
            json!({"camera": {"visible": true, "shape": "circle", "rect": [0.7, 0.6, 0.2, 0.3]}});
        let mut b = RecBundle::create(tmp.path(), "A", NOW, json!({}), &session()).unwrap();
        b.set_extra(LAYOUT_KEY, layout.clone()).unwrap();
        let m = b.finalize(&[], NOW).unwrap();
        assert_eq!(m.edits[0]["layout"], layout);
        assert!(!m.extra.contains_key(LAYOUT_KEY), "not a top-level field");
        assert_eq!(m.edits[0]["speed"], 1.0);
        // A crash after the layout was set: recovery keeps it.
        let mut c = crashed_bundle(tmp.path(), 2, 0);
        c.set_extra(LAYOUT_KEY, layout.clone()).unwrap();
        let RecoveryOutcome::Recovered(r) = recover_bundle(c.dir(), NOW).unwrap() else {
            panic!("expected a recovery");
        };
        assert_eq!(r.manifest.edits[0]["layout"], layout);
    }

    /// EXP-1: exports are listed in the manifest; an export overwrites only its own preset's entry.
    #[test]
    fn exports_are_recorded_atomically_one_per_preset() {
        let tmp = tempfile::tempdir().unwrap();
        let b = RecBundle::create(tmp.path(), "A", NOW, json!({}), &session()).unwrap();
        let dir = b.dir().to_owned();
        b.finalize(&[], NOW).unwrap();
        assert!(exports_of(&read_manifest(&dir).unwrap()).is_empty());
        let entry = |size: u64, preset: &str| {
            json!({"file": format!("exports/A - {preset}.mp4"), "preset": preset,
                   "created": "2026-10-21T10:00:00Z", "duration_ms": 1000, "size_bytes": size,
                   "encoder": "libx264", "settings": {}})
        };
        let m = record_export(&dir, entry(10, "quick"), Actor::User, NOW).unwrap();
        assert_eq!(exports_of(&m).len(), 1);
        assert_eq!(m.revisions.last().unwrap().action, "export");
        assert_eq!(m.revisions.last().unwrap().rev, 2);
        let m = record_export(
            &dir,
            entry(20, "quick"),
            Actor::agent("claude").unwrap(),
            NOW,
        )
        .unwrap();
        let list = exports_of(&m);
        assert_eq!(list.len(), 1, "same preset replaces");
        assert_eq!(list[0]["size_bytes"], 20);
        let m = record_export(&dir, entry(30, "youtube-1080p"), Actor::User, NOW).unwrap();
        assert_eq!(exports_of(&m).len(), 2);
        // Round-trips through disk, no temp file left behind.
        let again = read_manifest(&dir).unwrap();
        assert_eq!(exports_of(&again), exports_of(&m));
        assert!(!dir.join("manifest.json.tmp").exists());
    }

    /// STO-4.AC3: uploads are listed in the manifest, add no revision and survive a reload.
    #[test]
    fn uploads_are_recorded_without_a_revision() {
        let tmp = tempfile::tempdir().unwrap();
        let b = RecBundle::create(tmp.path(), "A", NOW, json!({}), &session()).unwrap();
        let dir = b.dir().to_owned();
        b.finalize(&[], NOW).unwrap();
        assert!(uploads_of(&dir).is_empty());
        let before = read_manifest(&dir).unwrap().revisions.len();
        let entry = |remote: &str, bytes: u64| {
            json!({"destination_id": "d1", "what": "export", "remote_id": remote,
                   "bytes": bytes, "share_link": null})
        };
        record_upload(&dir, entry("a.mp4", 1)).unwrap();
        record_upload(&dir, entry("b.mp4", 2)).unwrap();
        record_upload(&dir, entry("a.mp4", 3)).unwrap();
        let list = uploads_of(&dir);
        assert_eq!(list.len(), 2, "the same remote file replaces its record");
        assert_eq!(list[0]["bytes"], 3);
        let m = read_manifest(&dir).unwrap();
        assert_eq!(m.revisions.len(), before, "no undo step");
        assert!(!dir.join("manifest.json.tmp").exists());
    }

    /// Remote deletions: finished ones drop their record, failed ones keep it, the outcome is
    /// kept, and the rewrite adds no revision.
    #[test]
    fn remote_deletions_remove_their_records_and_keep_the_outcome() {
        let tmp = tempfile::tempdir().unwrap();
        let b = RecBundle::create(tmp.path(), "A", NOW, json!({}), &session()).unwrap();
        let dir = b.dir().to_owned();
        b.finalize(&[], NOW).unwrap();
        let before = read_manifest(&dir).unwrap().revisions.len();
        record_upload(
            &dir,
            json!({"destination_id": "d1", "what": "export", "remote_id": "f1"}),
        )
        .unwrap();
        record_upload(
            &dir,
            json!({"destination_id": "d1", "what": "original", "remote_id": "x/y"}),
        )
        .unwrap();
        record_publish(
            &dir,
            json!({"platform": "youtube", "account_id": "yt1", "remote_id": "v1"}),
        )
        .unwrap();
        record_remote_deletions(
            &dir,
            vec![
                json!({"kind": "upload", "destination_id": "d1", "what": "export",
                       "remote_id": "f1", "outcome": "deleted", "at": "t"}),
                json!({"kind": "upload", "destination_id": "d1", "what": "original",
                       "remote_id": "x/y", "outcome": "failed", "error": "network", "at": "t"}),
                json!({"kind": "publish", "platform": "youtube", "account_id": "yt1",
                       "remote_id": "v1", "outcome": "already_gone", "at": "t"}),
            ],
        )
        .unwrap();
        let up = uploads_of(&dir);
        assert_eq!(up.len(), 1, "only the failed one is left");
        assert_eq!(up[0]["remote_id"], "x/y");
        assert!(publishes_of(&dir).is_empty());
        let log = remote_deletions_of(&dir);
        assert_eq!(log.len(), 3);
        assert_eq!(log[1]["outcome"], "failed");
        assert_eq!(read_manifest(&dir).unwrap().revisions.len(), before);
        // The log is bounded.
        for _ in 0..60 {
            record_remote_deletions(&dir, vec![json!({"kind": "upload", "outcome": "failed"})])
                .unwrap();
        }
        assert_eq!(remote_deletions_of(&dir).len(), REMOTE_DELETIONS_KEEP);
        assert!(!dir.join("manifest.json.tmp").exists());
    }

    /// PUB-2: publishes are listed in the manifest, add no revision and survive a reload.
    #[test]
    fn publishes_are_recorded_without_a_revision() {
        let tmp = tempfile::tempdir().unwrap();
        let b = RecBundle::create(tmp.path(), "A", NOW, json!({}), &session()).unwrap();
        let dir = b.dir().to_owned();
        b.finalize(&[], NOW).unwrap();
        assert!(publishes_of(&dir).is_empty());
        let before = read_manifest(&dir).unwrap().revisions.len();
        let entry = |remote: &str, status: &str| {
            json!({"platform": "youtube", "account_id": "yt1", "remote_id": remote,
                   "url": format!("https://youtu.be/{remote}"), "status": status})
        };
        record_publish(&dir, entry("v1", "processing")).unwrap();
        record_publish(&dir, entry("v2", "processing")).unwrap();
        record_publish(&dir, entry("v1", "processed")).unwrap();
        let list = publishes_of(&dir);
        assert_eq!(list.len(), 2, "the same video replaces its record");
        assert_eq!(list[0]["status"], "processed");
        assert_eq!(read_manifest(&dir).unwrap().revisions.len(), before);
        assert!(uploads_of(&dir).is_empty(), "uploads are a separate list");
    }

    #[test]
    fn create_writes_session_and_lock_but_no_manifest() {
        let tmp = tempfile::tempdir().unwrap();
        let b = RecBundle::create(tmp.path(), "Lesson", NOW, json!({}), &session()).unwrap();
        assert!(b.dir().ends_with("Lesson.cmrec"));
        assert!(b.dir().join(LOCK_FILE).is_file());
        assert!(!b.dir().join(MANIFEST_FILE).exists());
        let s: Value =
            serde_json::from_str(&fs::read_to_string(b.dir().join(SESSION_FILE)).unwrap()).unwrap();
        assert_eq!(s["clock"]["origin_ns"], 123);
        assert_eq!(s["clock"]["tick_rate"], 120_000);
        assert_eq!(s["app_version"], "0.0.1");
        // A second recording with the same title gets a suffix.
        let b2 = RecBundle::create(tmp.path(), "Lesson", NOW, json!({}), &session()).unwrap();
        assert!(b2.dir().ends_with("Lesson (2).cmrec"));
    }

    #[test]
    fn finalize_writes_the_manifest_and_removes_the_lock() {
        let tmp = tempfile::tempdir().unwrap();
        let mut b = crashed_bundle(tmp.path(), 5, 0);
        b.add_gap(PauseGap {
            from: MediaTime::from_secs(4),
            to: MediaTime::from_secs(9),
        })
        .unwrap();
        let dir = b.dir().to_owned();
        let info = TrackInfo {
            kind: TrackKind::Screen,
            file: "screen.mp4".into(),
            start: MediaTime::from_millis(300),
            end: MediaTime::from_secs(15),
            details: json!({"size": [3024, 1964], "fps": 30, "codec": "hevc"}),
            stats: Default::default(),
        };
        let m = b.finalize(&[info], NOW).unwrap();
        assert!(!dir.join(LOCK_FILE).exists());
        let back = RecManifest::from_json(&fs::read_to_string(dir.join(MANIFEST_FILE)).unwrap())
            .unwrap_or_else(|_| panic!("manifest must parse"));
        assert_eq!(back, m);
        assert_eq!(m.schema, 1);
        assert_eq!(m.tick_rate, Some(120_000));
        // 15 s of session minus the 5 s gap.
        assert_eq!(m.duration, Some(MediaTime::from_secs(10)));
        assert_eq!(m.tracks["screen"]["size"], json!([3024, 1964]));
        assert_eq!(m.tracks["screen"]["file"], "screen.mp4");
        assert_eq!(
            m.edits,
            vec![json!({"in": 0, "out": 1_200_000, "speed": 1.0})]
        );
        assert_eq!(m.revisions.len(), 1);
        assert_eq!(m.gaps.len(), 1);
        assert_eq!(m.settings["fps"], 30);
    }

    /// REC-8.AC1 on a fixture: a crash 11.3 s into the recording leaves the 5 complete
    /// fragments (10 s), a partial sixth, and the user gets at least elapsed - 2 s.
    #[test]
    fn rec8_ac1_truncated_screen_file_is_recovered_up_to_the_last_fragment() {
        let tmp = tempfile::tempdir().unwrap();
        let b = crashed_bundle(tmp.path(), 5, 700);
        let dir = b.dir().to_owned();
        let before = fs::metadata(dir.join("screen.mp4")).unwrap().len();
        fs::write(
            dir.join("cursor.jsonl"),
            b"{\"t\":1}\n{\"t\":2}\n{\"t\":3,\"ty",
        )
        .unwrap();
        drop(b); // the process "dies": lock stays

        let RecoveryOutcome::Recovered(r) = recover_bundle(&dir, NOW).unwrap() else {
            panic!("expected recovery");
        };
        assert_eq!(r.duration, MediaTime::from_secs(10));
        assert!(r.duration >= MediaTime::from_millis(11_300 - 2000));
        assert_eq!(r.lost_ms_max, 2000);
        assert_eq!(
            r.tracks[0],
            (
                "screen".into(),
                TrackRecovery::Truncated {
                    removed_bytes: 700,
                    frames: 300,
                    duration: MediaTime::from_secs(10)
                }
            )
        );
        assert!(matches!(
            r.tracks[1],
            (ref n, TrackRecovery::Truncated { removed_bytes: 10, frames: 2, .. }) if n == "cursor"
        ));
        assert!(!dir.join(LOCK_FILE).exists());
        assert_eq!(
            fs::metadata(dir.join("screen.mp4")).unwrap().len(),
            before - 700
        );
        assert_eq!(
            fs::read(dir.join("cursor.jsonl")).unwrap(),
            b"{\"t\":1}\n{\"t\":2}\n"
        );
        let m = &r.manifest;
        assert_eq!(m.tracks["screen"]["size"], json!([3024, 1964]));
        assert_eq!(m.tracks["screen"]["codec"], "hevc");
        assert_eq!(m.tracks["screen"]["fps"], 30);
        assert_eq!(m.tracks["screen"]["frames"], 300);
        assert_eq!(m.duration, Some(MediaTime::from_secs(10)));
        assert_eq!(m.revisions[0].action, "recover");
        assert!(m.extra.contains_key("recovered"));
        // The written manifest parses and the file decodes as a clean fragmented MP4.
        let parsed = RecManifest::from_json(&fs::read_to_string(dir.join(MANIFEST_FILE)).unwrap());
        assert!(parsed.is_ok());
        assert!(fmp4::scan_file(&dir.join("screen.mp4")).unwrap().is_clean());
    }

    #[test]
    fn recovery_maps_file_time_back_to_session_time_with_gaps() {
        let tmp = tempfile::tempdir().unwrap();
        let mut b = crashed_bundle(tmp.path(), 5, 0); // 10 s of file
        b.add_gap(PauseGap {
            from: MediaTime::from_secs(4),
            to: MediaTime::from_secs(20),
        })
        .unwrap();
        let dir = b.dir().to_owned();
        drop(b);
        let RecoveryOutcome::Recovered(r) = recover_bundle(&dir, NOW).unwrap() else {
            panic!()
        };
        // File ends at 10 s; the 16 s gap came after 4 s of recording: session end = 26 s.
        assert_eq!(r.manifest.tracks["screen"]["end"], 26 * 120_000);
        assert_eq!(r.manifest.tracks["screen"]["start"], 0);
        assert_eq!(r.duration, MediaTime::from_secs(10));
        assert_eq!(r.manifest.gaps.len(), 1);
    }

    /// REC-4 for the camera: its file time has the gaps removed like the screen's, so recovery
    /// maps its start and end back to session time through the saved gaps the same way.
    #[test]
    fn recovery_maps_camera_file_time_back_through_gaps_like_the_screen() {
        let tmp = tempfile::tempdir().unwrap();
        let mut b = crashed_bundle(tmp.path(), 5, 0); // screen: 0..10 s of file
                                                      // Camera: 3 fragments, file time 0.3..6.3 s (it started 0.3 s after the origin).
        let spec = Spec {
            width: 1920,
            height: 1080,
            codec: *b"avc1",
            ..Default::default()
        };
        let mut cam = init_segment(&spec);
        for i in 0..3u32 {
            cam.extend(fragment(
                &spec,
                i + 1,
                36_000 + u64::from(i) * 240_000,
                60,
                4000,
                64,
            ));
        }
        fs::write(b.dir().join("camera.mp4"), cam).unwrap();
        // Two pauses: 4 s..20 s (16 s) and 24 s..26 s (2 s).
        for (from, to) in [(4, 20), (24, 26)] {
            b.add_gap(PauseGap {
                from: MediaTime::from_secs(from),
                to: MediaTime::from_secs(to),
            })
            .unwrap();
        }
        let dir = b.dir().to_owned();
        drop(b);
        let RecoveryOutcome::Recovered(r) = recover_bundle(&dir, NOW).unwrap() else {
            panic!()
        };
        let t = &r.manifest.tracks;
        assert_eq!(t["camera"]["codec"], "h264");
        // Start (0.3 s) precedes the first gap; the end (file 6.3 s) is 2.3 s into the second
        // stretch: 6.3 + 16 = 22.3 s, before the second gap at 24 s.
        assert_eq!(t["camera"]["start"], 36_000);
        assert_eq!(t["camera"]["end"], 22 * 120_000 + 36_000);
        // The screen (10 s of file) maps through both gaps: 10 + 16 + 2.
        assert_eq!(t["screen"]["end"], 28 * 120_000);
        // The longest file decides the duration, in file time.
        assert_eq!(r.duration, MediaTime::from_secs(10));
        assert_eq!(r.manifest.gaps.len(), 2);
    }

    #[test]
    fn a_file_end_at_a_pause_point_stays_before_the_gap() {
        let gaps = [PauseGap {
            from: MediaTime::from_secs(4),
            to: MediaTime::from_secs(9),
        }];
        assert_eq!(
            file_end_to_session(&gaps, MediaTime::from_secs(4)),
            MediaTime::from_secs(4)
        );
        assert_eq!(
            file_end_to_session(&gaps, MediaTime::from_secs(5)),
            MediaTime::from_secs(10)
        );
        // A start at the pause point is the first sample after the resume.
        assert_eq!(
            file_to_session(&gaps, MediaTime::from_secs(4)),
            MediaTime::from_secs(9)
        );
    }

    #[test]
    fn rec8_edge_unrecoverable_track_is_marked_missing_and_others_kept() {
        let tmp = tempfile::tempdir().unwrap();
        let b = crashed_bundle(tmp.path(), 3, 0);
        let dir = b.dir().to_owned();
        fs::write(dir.join("camera.mp4"), b"not an mp4 at all").unwrap();
        drop(b);
        let RecoveryOutcome::Recovered(r) = recover_bundle(&dir, NOW).unwrap() else {
            panic!()
        };
        assert_eq!(r.manifest.tracks["camera"]["missing"], true);
        assert_eq!(r.manifest.tracks["screen"]["frames"], 180);
        assert!(matches!(r.tracks[1], (_, TrackRecovery::Missing { .. })));
    }

    #[test]
    fn nothing_recoverable_is_an_error_and_leaves_the_lock() {
        let tmp = tempfile::tempdir().unwrap();
        let b = RecBundle::create(tmp.path(), "Empty", NOW, json!({}), &session()).unwrap();
        let dir = b.dir().to_owned();
        // Only the init segment made it to disk: no complete fragment.
        fs::write(dir.join("screen.mp4"), init_segment(&Spec::default())).unwrap();
        assert!(recover_bundle(&dir, NOW).is_err());
        assert!(dir.join(LOCK_FILE).exists());
        assert!(!dir.join(MANIFEST_FILE).exists());
    }

    #[test]
    fn a_lock_left_next_to_a_finished_manifest_is_just_removed() {
        let tmp = tempfile::tempdir().unwrap();
        let b = crashed_bundle(tmp.path(), 2, 0);
        let dir = b.dir().to_owned();
        let lock = fs::read(dir.join(LOCK_FILE)).unwrap();
        let info = TrackInfo {
            kind: TrackKind::Screen,
            file: "screen.mp4".into(),
            start: MediaTime::ZERO,
            end: MediaTime::from_secs(4),
            details: json!({}),
            stats: Default::default(),
        };
        b.finalize(&[info], NOW).unwrap();
        fs::write(dir.join(LOCK_FILE), lock).unwrap(); // crash between manifest and lock removal
        assert_eq!(
            recover_bundle(&dir, NOW).unwrap(),
            RecoveryOutcome::AlreadyComplete
        );
        assert!(!dir.join(LOCK_FILE).exists());
    }

    #[test]
    fn unreadable_lock_falls_back_to_the_folder_name() {
        let tmp = tempfile::tempdir().unwrap();
        let b = crashed_bundle(tmp.path(), 2, 0);
        let dir = b.dir().to_owned();
        fs::write(dir.join(LOCK_FILE), b"{ torn").unwrap();
        let RecoveryOutcome::Recovered(r) = recover_bundle(&dir, NOW).unwrap() else {
            panic!()
        };
        assert_eq!(r.title, "Lesson");
        assert!(r.id.starts_with("rec_"));
    }

    #[test]
    fn pid_zero_and_dead_pids_are_not_alive() {
        assert!(!pid_alive(0));
        assert!(pid_alive(std::process::id()));
    }
}
