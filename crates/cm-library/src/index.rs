//! The SQLite index (`captures`, `transcripts_fts`) and the [`Library`] that owns it.
//!
//! The index is a disposable cache: bundles on disk are the source of truth, and
//! [`Library::rescan`] rebuilds every row from them.

use crate::bundle::{free_bundle_path, ShotBundle, MANIFEST_FILE, THUMBS_DIR};
use crate::manifest::{check_schema, CaptureKind, ManifestProblem, RecManifest, ShotManifest};
use crate::rec::{
    mark_recovery_failed, owner_alive, recover_bundle, recovery_failure, RecBundle,
    RecoveryOutcome, RecoveryReport, SessionInfo, LOCK_FILE,
};
use crate::thumbs::{write_poster, DEFAULT_POSTER_EDGE, POSTER_FILE};
use crate::util::{dir_size, io_err, json_err, sanitize_title, sql_err, write_atomic};
use cm_core::{CmError, MediaTime, TrackInfo};
use rusqlite::{params, params_from_iter, types::Value as SqlValue, Connection, OptionalExtension};
use serde_json::Value;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

/// `PRAGMA user_version` of the index this build writes. A different value means "rebuild".
pub const INDEX_SCHEMA_VERSION: i32 = 3;
/// Index file location relative to the library root.
pub const INDEX_RELATIVE_PATH: &str = ".capturemann/index.sqlite";

/// Attempts to open an existing index before giving up on a busy database.
const BUSY_RETRIES: u32 = 2;
/// How long SQLite itself waits on a lock per attempt.
const BUSY_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(1000);

/// Why the index could not be opened.
#[derive(Debug)]
enum InitError {
    /// Corrupt, not a database, or another schema version: rebuild it.
    Unusable,
    /// Busy, locked or an I/O error: the file may be fine, so it must be kept.
    Transient(CmError),
}

impl InitError {
    fn from_sql(e: rusqlite::Error) -> Self {
        use rusqlite::ErrorCode;
        match e.sqlite_error_code() {
            Some(ErrorCode::DatabaseCorrupt | ErrorCode::NotADatabase) => InitError::Unusable,
            _ => InitError::Transient(sql_err(&e)),
        }
    }

    fn into_cm(self) -> CmError {
        match self {
            InitError::Unusable => CmError::invalid_input("index is corrupted"),
            InitError::Transient(e) => e,
        }
    }
}

/// One row of the `captures` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureRecord {
    pub id: String,
    /// Bundle folder name, relative to the library root (so moving the library keeps the index
    /// valid, LIB-2).
    pub path: String,
    pub title: String,
    pub kind: CaptureKind,
    /// Recordings only.
    pub duration: Option<MediaTime>,
    pub created: OffsetDateTime,
    /// Total size of the bundle's files.
    pub size_bytes: u64,
    /// Physical px of the original (screenshots).
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// The bundle could not be read; `id` is `broken:<folder name>` and the title the folder name.
    pub broken: bool,
    /// Upload records (`uploads[]`) in the manifest; derived when the bundle is indexed, so the
    /// "Uploaded" filter pages correctly. `0` for a broken bundle.
    pub upload_count: u32,
    /// Publish records (`publishes[]`) in the manifest; derived like `upload_count`.
    pub publish_count: u32,
}

/// The remote-copy filter of the library (LIB-1 chips "Uploaded" and "Published";
/// `list_library { status }`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusFilter {
    /// At least one upload record.
    Uploaded,
    /// No upload record.
    NotUploaded,
    /// At least one publish record.
    Published,
    /// No publish record.
    NotPublished,
}

impl StatusFilter {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "uploaded" => Self::Uploaded,
            "not_uploaded" => Self::NotUploaded,
            "published" => Self::Published,
            "not_published" => Self::NotPublished,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Uploaded => "uploaded",
            Self::NotUploaded => "not_uploaded",
            Self::Published => "published",
            Self::NotPublished => "not_published",
        }
    }

    fn sql(self) -> &'static str {
        match self {
            Self::Uploaded => "upload_count > 0",
            Self::NotUploaded => "upload_count = 0",
            Self::Published => "publish_count > 0",
            Self::NotPublished => "publish_count = 0",
        }
    }

    fn matches(self, r: &CaptureRecord) -> bool {
        match self {
            Self::Uploaded => r.upload_count > 0,
            Self::NotUploaded => r.upload_count == 0,
            Self::Published => r.publish_count > 0,
            Self::NotPublished => r.publish_count == 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortKey {
    #[default]
    Date,
    Title,
    Duration,
    Size,
}

/// Filter, sort and paging for [`Library::list`] and [`Library::search`].
#[derive(Debug, Clone)]
pub struct ListQuery {
    pub kind: Option<CaptureKind>,
    /// Only captures with (or without) upload or publish records.
    pub status: Option<StatusFilter>,
    /// Inclusive lower bound on `created`.
    pub created_from: Option<OffsetDateTime>,
    /// Exclusive upper bound on `created`.
    pub created_to: Option<OffsetDateTime>,
    pub sort: SortKey,
    /// Defaults to `true` (newest/largest first).
    pub descending: bool,
    pub limit: Option<usize>,
    pub offset: usize,
}

impl Default for ListQuery {
    fn default() -> Self {
        ListQuery {
            kind: None,
            status: None,
            created_from: None,
            created_to: None,
            sort: SortKey::Date,
            descending: true,
            limit: None,
            offset: 0,
        }
    }
}

/// A transcript line to index: when it starts and its text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptSegment {
    pub start: MediaTime,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptMatch {
    pub start: MediaTime,
    /// Text around the match with hits wrapped in `«` and `»` (raw text, not escaped).
    pub snippet: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    pub record: CaptureRecord,
    pub title_match: bool,
    /// Best transcript match, if any.
    pub transcript: Option<TranscriptMatch>,
}

/// Two bundle folders with the same capture id (a bundle copied in with the file manager).
///
/// The kept folder is the one already indexed for the id, else the oldest by manifest mtime,
/// else the shortest name (so `X.cmshot` wins over `X (2).cmshot`). A "Duplicate" feature must
/// mint a new id: use [`Library::duplicate`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateBundle {
    pub id: String,
    /// Folder name that stays indexed.
    pub kept: String,
    /// Folder name that was skipped.
    pub ignored: String,
}

/// Outcome of [`Library::rescan`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RescanReport {
    /// Bundles indexed (including broken ones).
    pub indexed: usize,
    /// Index rows dropped because their bundle is gone.
    pub removed: usize,
    /// Folder names whose manifest could not be read (indexed with `broken = true`).
    pub broken: Vec<String>,
    /// Folder names skipped because another bundle already has the same capture id (the
    /// ignored side of each entry in `duplicates`).
    pub duplicate_ids: Vec<String>,
    /// Every set of bundles sharing a capture id, with the one that was indexed (R1-5).
    pub duplicates: Vec<DuplicateBundle>,
    /// Recording folders skipped because they still have a `recording.lock` (a recording in
    /// progress, or one waiting for [`Library::recover_incomplete`]).
    pub incomplete: Vec<String>,
}

/// Outcome of [`Library::recover_incomplete`].
#[derive(Debug, Default)]
pub struct RecoverySummary {
    /// Bundles repaired and indexed (REC-8: "Recovered recording from `<time>`").
    pub recovered: Vec<RecoveryReport>,
    /// Folder names whose lock was stale after a finished manifest; the lock was removed.
    pub already_complete: Vec<String>,
    /// Folder names owned by a running process (this one included): left alone.
    pub in_progress: Vec<String>,
    /// Folders that could not be recovered (nothing usable on disk), with the reason. Their lock
    /// stays, marked `recovery_failed`, so the next launch skips them (see `quarantined`).
    pub failed: Vec<(String, CmError)>,
    /// Folders a previous launch already gave up on (`recovery_failed` in their lock): skipped,
    /// listed here so a UI can offer to open or delete them (R3-5).
    pub quarantined: Vec<String>,
}

/// A library folder plus its SQLite index.
#[derive(Debug)]
pub struct Library {
    root: PathBuf,
    conn: Connection,
    startup_rescan: Option<RescanReport>,
}

fn kind_of_dir(p: &Path) -> Option<CaptureKind> {
    CaptureKind::from_extension(p.extension()?.to_str()?)
}

impl Library {
    /// Opens the library at `root`, which must exist (LIB-2: an unplugged drive is reported as
    /// `not_found` "Library unavailable", never silently recreated).
    pub fn open(root: &Path) -> Result<Self, CmError> {
        if !root.is_dir() {
            return Err(CmError::not_found(format!(
                "Library unavailable: `{}` is not reachable",
                root.display()
            )));
        }
        Self::open_inner(root)
    }

    /// Like [`Library::open`] but creates `root` (and parents) first. Use for the first run.
    pub fn open_or_create(root: &Path) -> Result<Self, CmError> {
        std::fs::create_dir_all(root).map_err(|e| io_err("cannot create", root, &e))?;
        Self::open_inner(root)
    }

    fn open_inner(root: &Path) -> Result<Self, CmError> {
        let index = root.join(INDEX_RELATIVE_PATH);
        if let Some(parent) = index.parent() {
            std::fs::create_dir_all(parent).map_err(|e| io_err("cannot create", parent, &e))?;
        }
        let existed = index.is_file();
        // Only a corrupt file or another schema version is thrown away (LIB-1.AC3). A busy or
        // locked database (another process holds it) or an I/O problem is retried and then
        // reported: deleting a healthy index would only cost a rescan, but deleting it while
        // another process uses it can corrupt that process's view (R1-20).
        let mut conn = None;
        if existed {
            for attempt in 0..BUSY_RETRIES {
                match Self::init(&index) {
                    Ok(c) => {
                        conn = Some(c);
                        break;
                    }
                    Err(InitError::Unusable) => break,
                    Err(InitError::Transient(e)) if attempt + 1 == BUSY_RETRIES => return Err(e),
                    Err(InitError::Transient(_)) => {
                        std::thread::sleep(std::time::Duration::from_millis(50));
                    }
                }
            }
        }
        let (conn, rebuilt) = match conn {
            Some(c) => (c, false),
            None => {
                // Missing, corrupted or from another version: start over.
                for suffix in ["", "-wal", "-shm", "-journal"] {
                    let mut p = index.clone().into_os_string();
                    p.push(suffix);
                    let _ = std::fs::remove_file(PathBuf::from(p));
                }
                (Self::init(&index).map_err(InitError::into_cm)?, true)
            }
        };
        let mut lib = Library {
            root: root.to_owned(),
            conn,
            startup_rescan: None,
        };
        if rebuilt {
            lib.startup_rescan = Some(lib.rescan()?);
        }
        Ok(lib)
    }

    fn init(index: &Path) -> Result<Connection, InitError> {
        let conn = Connection::open(index).map_err(InitError::from_sql)?;
        conn.busy_timeout(BUSY_TIMEOUT)
            .map_err(InitError::from_sql)?;
        let version: i32 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .map_err(InitError::from_sql)?;
        if version == 0 {
            conn.execute_batch(SCHEMA).map_err(InitError::from_sql)?;
            conn.pragma_update(None, "user_version", INDEX_SCHEMA_VERSION)
                .map_err(InitError::from_sql)?;
        } else if version == 2 {
            // Version 3 lets `kind` be `audio` (VOC-1). SQLite cannot change a CHECK constraint,
            // so the table is rebuilt in place; a full rebuild of the index would lose the
            // transcript search rows, which cannot be recovered from the folders.
            conn.execute_batch(MIGRATE_2_TO_3)
                .map_err(InitError::from_sql)?;
        } else if version != INDEX_SCHEMA_VERSION {
            return Err(InitError::Unusable);
        }
        let check: String = conn
            .query_row("PRAGMA quick_check", [], |r| r.get(0))
            .map_err(InitError::from_sql)?;
        if check != "ok" {
            return Err(InitError::Unusable);
        }
        Ok(conn)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Set when the index was missing or unusable at open and was rebuilt from the bundles.
    pub fn startup_rescan(&self) -> Option<&RescanReport> {
        self.startup_rescan.as_ref()
    }

    pub fn bundle_path(&self, record: &CaptureRecord) -> PathBuf {
        self.root.join(&record.path)
    }

    /// `thumbs/poster.jpg` of a capture if it exists.
    pub fn poster_path(&self, record: &CaptureRecord) -> Option<PathBuf> {
        let p = self.bundle_path(record).join(THUMBS_DIR).join(POSTER_FILE);
        p.is_file().then_some(p)
    }

    // ---- screenshots -----------------------------------------------------------------------

    /// Creates a screenshot bundle in the library, writes its poster and indexes it.
    ///
    /// `title` defaults to `Screenshot yyyy-MM-dd HH.mm` in `now`'s offset (LIB-1). A poster
    /// failure does not fail the capture; it is reported in the bundle's warnings.
    pub fn create_screenshot(
        &mut self,
        png: &[u8],
        title: Option<&str>,
        now: OffsetDateTime,
    ) -> Result<ShotBundle, CmError> {
        let title = match title {
            Some(t) => t.to_owned(),
            None => default_title("Screenshot", now)?,
        };
        let mut bundle = ShotBundle::create(&self.root, &title, png, now)?;
        if let Err(e) = write_poster(&bundle, DEFAULT_POSTER_EDGE) {
            bundle.push_warning(format!("Thumbnail not created: {e}"));
        }
        self.index_shot(&bundle)?;
        Ok(bundle)
    }

    /// [`Library::create_screenshot`] minus the poster: creates and indexes the bundle only.
    /// Decoding and scaling the poster is slow, so a caller that shares the library behind a
    /// lock writes it afterwards with [`crate::thumbs::write_poster`], outside the lock, and
    /// then calls [`Library::index_shot`] again so the row's `size_bytes` includes it.
    pub fn create_screenshot_without_poster(
        &mut self,
        png: &[u8],
        title: Option<&str>,
        now: OffsetDateTime,
    ) -> Result<ShotBundle, CmError> {
        let title = match title {
            Some(t) => t.to_owned(),
            None => default_title("Screenshot", now)?,
        };
        let bundle = ShotBundle::create(&self.root, &title, png, now)?;
        self.index_shot(&bundle)?;
        Ok(bundle)
    }

    /// Opens a library screenshot by capture id.
    pub fn open_screenshot(&self, id: &str) -> Result<ShotBundle, CmError> {
        let rec = self.get(id)?;
        if rec.kind != CaptureKind::Screenshot {
            return Err(CmError::invalid_field(
                "capture_id",
                "not a screenshot (a recording: `list_library` describes it, the edit and export actions take it)",
            ));
        }
        ShotBundle::open(&self.bundle_path(&rec))
    }

    /// Saves the manifest, refreshes the poster and updates the index row (CAP-4: after each
    /// annotation save).
    pub fn save_shot(&mut self, bundle: &mut ShotBundle) -> Result<CaptureRecord, CmError> {
        bundle.save()?;
        if let Err(e) = write_poster(bundle, DEFAULT_POSTER_EDGE) {
            bundle.push_warning(format!("Thumbnail not updated: {e}"));
        }
        self.index_shot(bundle)
    }

    /// [`Library::save_shot`] minus the poster: saves the manifest and updates the index row.
    /// Write the poster afterwards with [`crate::thumbs::write_poster`], outside any lock, then
    /// call [`Library::index_shot`] again so `size_bytes` includes it.
    pub fn save_shot_without_poster(
        &mut self,
        bundle: &mut ShotBundle,
    ) -> Result<CaptureRecord, CmError> {
        bundle.save()?;
        self.index_shot(bundle)
    }

    /// Upserts the index row for an open bundle inside the library root.
    pub fn index_shot(&mut self, bundle: &ShotBundle) -> Result<CaptureRecord, CmError> {
        self.index_dir(bundle.dir())
    }

    /// Reads a bundle folder's header without touching the index (the file watcher decides
    /// what to do with it). `Err` when the manifest is missing, unreadable or does not parse.
    pub fn peek_dir(&self, dir: &Path) -> Result<CaptureRecord, CmError> {
        self.read_record(dir)
    }

    /// [`Library::peek_dir`], but an unreadable bundle yields the `broken` row that
    /// [`Library::rescan`] would index for it (id `broken:<folder>`).
    pub fn peek_dir_or_broken(&self, dir: &Path) -> CaptureRecord {
        self.read_record(dir)
            .unwrap_or_else(|_| broken_record(dir, &file_name(dir)))
    }

    /// The row whose bundle folder is `rel` (the folder name under the root), if any.
    pub fn get_by_path(&self, rel: &str) -> Result<Option<CaptureRecord>, CmError> {
        self.conn
            .query_row(
                &format!("SELECT {COLS} FROM captures WHERE path = ?1"),
                [rel],
                row_to_record,
            )
            .optional()
            .map_err(|e| sql_err(&e))
    }

    /// Reads a bundle folder's header and upserts its row.
    pub fn index_dir(&mut self, dir: &Path) -> Result<CaptureRecord, CmError> {
        let rec = self.read_record(dir)?;
        self.upsert(&rec)?;
        Ok(rec)
    }

    // ---- recordings ------------------------------------------------------------------------

    /// Starts a recording bundle in the library (`Recording yyyy-MM-dd HH.mm` by default). It is
    /// not indexed until [`Library::finish_recording`]; until then it carries `recording.lock`.
    pub fn create_recording(
        &self,
        title: Option<&str>,
        now: OffsetDateTime,
        settings: Value,
        session: &SessionInfo,
    ) -> Result<RecBundle, CmError> {
        let title = match title {
            Some(t) => t.to_owned(),
            None => default_title("Recording", now)?,
        };
        RecBundle::create(&self.root, &title, now, settings, session)
    }

    /// Finalizes a recording (manifest, lock removed) and indexes it.
    pub fn finish_recording(
        &mut self,
        bundle: RecBundle,
        tracks: &[TrackInfo],
        now: OffsetDateTime,
    ) -> Result<CaptureRecord, CmError> {
        let dir = bundle.dir().to_owned();
        bundle.finalize(tracks, now)?;
        self.index_dir(&dir)
    }

    /// Recovers recordings interrupted by a crash or power loss (REC-8): every `.cmrec` folder
    /// with a `recording.lock` whose owner process is gone is truncated to its last complete
    /// fragment, given a manifest built from the fragments' durations, and indexed. Call once at
    /// launch, before the first rescan result is shown.
    pub fn recover_incomplete(&mut self) -> Result<RecoverySummary, CmError> {
        self.recover_incomplete_at(OffsetDateTime::now_utc())
    }

    /// [`Library::recover_incomplete`] with an explicit clock (tests).
    pub fn recover_incomplete_at(
        &mut self,
        now: OffsetDateTime,
    ) -> Result<RecoverySummary, CmError> {
        let rd =
            std::fs::read_dir(&self.root).map_err(|e| io_err("cannot read", &self.root, &e))?;
        let mut dirs: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir() && kind_of_dir(p) == Some(CaptureKind::Recording))
            .filter(|p| p.join(LOCK_FILE).exists())
            .collect();
        dirs.sort();
        let mut out = RecoverySummary::default();
        for dir in dirs {
            let name = file_name(&dir);
            if owner_alive(&dir) {
                out.in_progress.push(name);
                continue;
            }
            if recovery_failure(&dir).is_some() {
                out.quarantined.push(name);
                continue;
            }
            // One folder that cannot be indexed or recovered must not stop the others (R3-18).
            match recover_bundle(&dir, now) {
                Ok(RecoveryOutcome::AlreadyComplete) => match self.index_dir(&dir) {
                    Ok(_) => out.already_complete.push(name),
                    Err(e) => out.failed.push((name, e)),
                },
                Ok(RecoveryOutcome::Recovered(report)) => match self.index_dir(&dir) {
                    Ok(_) => out.recovered.push(*report),
                    Err(e) => out.failed.push((name, e)),
                },
                Err(e) => {
                    // Nothing usable on disk: say so once and stop retrying every launch.
                    if matches!(e, CmError::InvalidInput { .. }) {
                        mark_recovery_failed(&dir, &e.to_string());
                    }
                    out.failed.push((name, e));
                }
            }
        }
        Ok(out)
    }

    // ---- rows ------------------------------------------------------------------------------

    pub fn upsert(&mut self, r: &CaptureRecord) -> Result<(), CmError> {
        let tx = self.conn.transaction().map_err(|e| sql_err(&e))?;
        tx.execute(
            "DELETE FROM captures WHERE path = ?1 AND id <> ?2",
            params![r.path, r.id],
        )
        .map_err(|e| sql_err(&e))?;
        tx.execute(
            "INSERT INTO captures (id, path, title, kind, duration, created, created_unix, size_bytes, width, height, broken, upload_count, publish_count)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(id) DO UPDATE SET path=excluded.path, title=excluded.title, kind=excluded.kind,
               duration=excluded.duration, created=excluded.created, created_unix=excluded.created_unix,
               size_bytes=excluded.size_bytes, width=excluded.width, height=excluded.height, broken=excluded.broken,
               upload_count=excluded.upload_count, publish_count=excluded.publish_count",
            params![
                r.id,
                r.path,
                r.title,
                r.kind.as_str(),
                r.duration.map(MediaTime::ticks),
                r.created.format(&Rfc3339).map_err(|e| CmError::internal(e.to_string()))?,
                r.created.unix_timestamp(),
                i64::try_from(r.size_bytes).unwrap_or(i64::MAX),
                r.width,
                r.height,
                r.broken,
                r.upload_count,
                r.publish_count,
            ],
        )
        .map_err(|e| sql_err(&e))?;
        tx.commit().map_err(|e| sql_err(&e))
    }

    /// Drops a capture from the index (and its transcript rows). Files are untouched.
    pub fn remove(&mut self, id: &str) -> Result<bool, CmError> {
        let tx = self.conn.transaction().map_err(|e| sql_err(&e))?;
        tx.execute("DELETE FROM transcripts_fts WHERE capture_id = ?1", [id])
            .map_err(|e| sql_err(&e))?;
        let n = tx
            .execute("DELETE FROM captures WHERE id = ?1", [id])
            .map_err(|e| sql_err(&e))?;
        tx.commit().map_err(|e| sql_err(&e))?;
        Ok(n > 0)
    }

    pub fn get(&self, id: &str) -> Result<CaptureRecord, CmError> {
        self.conn
            .query_row(
                &format!("SELECT {COLS} FROM captures WHERE id = ?1"),
                [id],
                row_to_record,
            )
            .optional()
            .map_err(|e| sql_err(&e))?
            .ok_or_else(|| CmError::not_found(format!("no capture `{id}`")))
    }

    pub fn count(&self) -> Result<usize, CmError> {
        self.conn
            .query_row("SELECT COUNT(*) FROM captures", [], |r| r.get::<_, i64>(0))
            .map(|n| usize::try_from(n).unwrap_or(0))
            .map_err(|e| sql_err(&e))
    }

    pub fn list(&self, q: &ListQuery) -> Result<Vec<CaptureRecord>, CmError> {
        let (where_sql, mut args) = where_clause(q, None);
        let col = match q.sort {
            SortKey::Date => "created_unix",
            SortKey::Title => "title COLLATE NOCASE",
            SortKey::Duration => "duration",
            SortKey::Size => "size_bytes",
        };
        let dir = if q.descending { "DESC" } else { "ASC" };
        let mut sql = format!(
            "SELECT {COLS} FROM captures {where_sql} ORDER BY {col} {dir}, created_unix DESC, id"
        );
        if q.limit.is_some() || q.offset > 0 {
            sql.push_str(" LIMIT ? OFFSET ?");
            // `usize::MAX` must not wrap to -1 (SQLite: "no limit" / "offset 0").
            let to_i64 = |v: usize| i64::try_from(v).unwrap_or(i64::MAX);
            args.push(SqlValue::Integer(q.limit.map_or(-1, to_i64)));
            args.push(SqlValue::Integer(to_i64(q.offset)));
        }
        let mut st = self.conn.prepare(&sql).map_err(|e| sql_err(&e))?;
        let rows = st
            .query_map(params_from_iter(args), row_to_record)
            .map_err(|e| sql_err(&e))?;
        rows.collect::<Result<_, _>>().map_err(|e| sql_err(&e))
    }

    /// Case-insensitive (ASCII) title match plus transcript full-text search (FTS5, prefix match
    /// on every word, all words required). Title hits come first (sorted per `q`), then
    /// transcript-only hits by relevance. `q.limit`/`q.offset` apply to the merged result.
    pub fn search(&self, text: &str, q: &ListQuery) -> Result<Vec<SearchHit>, CmError> {
        let words: Vec<&str> = text.split_whitespace().collect();
        if words.is_empty() {
            return Ok(Vec::new());
        }
        let like: Vec<String> = words
            .iter()
            .map(|w| {
                format!(
                    "%{}%",
                    w.replace('\\', "\\\\")
                        .replace('%', "\\%")
                        .replace('_', "\\_")
                )
            })
            .collect();
        let mut title_q = q.clone();
        title_q.limit = None;
        title_q.offset = 0;
        let (where_sql, args) = where_clause(&title_q, Some(like.len()));
        let col = match q.sort {
            SortKey::Date => "created_unix",
            SortKey::Title => "title COLLATE NOCASE",
            SortKey::Duration => "duration",
            SortKey::Size => "size_bytes",
        };
        let dir = if q.descending { "DESC" } else { "ASC" };
        let sql = format!("SELECT {COLS} FROM captures {where_sql} ORDER BY {col} {dir}, id");
        let mut all_args: Vec<SqlValue> = like.into_iter().map(SqlValue::Text).collect();
        all_args.extend(args);
        let mut st = self.conn.prepare(&sql).map_err(|e| sql_err(&e))?;
        let rows = st
            .query_map(params_from_iter(all_args), row_to_record)
            .map_err(|e| sql_err(&e))?;
        let title_recs: Vec<CaptureRecord> =
            rows.collect::<Result<_, _>>().map_err(|e| sql_err(&e))?;

        let fts = words
            .iter()
            .map(|w| format!("\"{}\"*", w.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" ");
        let mut st = self
            .conn
            .prepare(
                "SELECT capture_id, start_ticks, snippet(transcripts_fts, 0, '«', '»', '…', 16)
                 FROM transcripts_fts WHERE transcripts_fts MATCH ?1 ORDER BY rank",
            )
            .map_err(|e| sql_err(&e))?;
        let tx_rows: Vec<(String, i64, String)> = st
            .query_map([fts], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .map_err(|e| sql_err(&e))?
            .collect::<Result<_, _>>()
            .map_err(|e| sql_err(&e))?;
        let mut best: Vec<(String, TranscriptMatch)> = Vec::new();
        for (id, ticks, snippet) in tx_rows {
            if !best.iter().any(|(i, _)| *i == id) {
                best.push((
                    id,
                    TranscriptMatch {
                        start: MediaTime::from_ticks(ticks),
                        snippet,
                    },
                ));
            }
        }

        let mut hits: Vec<SearchHit> = title_recs
            .into_iter()
            .map(|record| {
                let transcript = best
                    .iter()
                    .find(|(i, _)| *i == record.id)
                    .map(|(_, m)| m.clone());
                SearchHit {
                    record,
                    title_match: true,
                    transcript,
                }
            })
            .collect();
        for (id, m) in best {
            if hits.iter().any(|h| h.record.id == id) {
                continue;
            }
            let Ok(record) = self.get(&id) else { continue };
            if !passes(q, &record) {
                continue;
            }
            hits.push(SearchHit {
                record,
                title_match: false,
                transcript: Some(m),
            });
        }
        let end = q
            .limit
            .map_or(hits.len(), |l| q.offset.saturating_add(l).min(hits.len()));
        let start = q.offset.min(end);
        Ok(hits.drain(start..end).collect())
    }

    /// Replaces the indexed transcript of a capture.
    ///
    /// Search hits carry the segment's `start`, so pass short segments (a sentence or a few
    /// words) to get word-accurate seek times.
    pub fn index_transcript(
        &mut self,
        capture_id: &str,
        segments: &[TranscriptSegment],
    ) -> Result<(), CmError> {
        self.get(capture_id)?;
        let tx = self.conn.transaction().map_err(|e| sql_err(&e))?;
        tx.execute(
            "DELETE FROM transcripts_fts WHERE capture_id = ?1",
            [capture_id],
        )
        .map_err(|e| sql_err(&e))?;
        {
            let mut st = tx
                .prepare("INSERT INTO transcripts_fts (text, capture_id, start_ticks) VALUES (?1, ?2, ?3)")
                .map_err(|e| sql_err(&e))?;
            for s in segments {
                st.execute(params![s.text, capture_id, s.start.ticks()])
                    .map_err(|e| sql_err(&e))?;
            }
        }
        tx.commit().map_err(|e| sql_err(&e))
    }

    // ---- rename ----------------------------------------------------------------------------

    /// Renames a capture: updates `title` in `manifest.json` (all other fields untouched) and
    /// renames the folder to `<title>.<ext>`, with ` (2)`… if taken (LIB-1.AC5). Reopen any
    /// `ShotBundle` of this capture afterwards; its path is stale.
    pub fn rename(&mut self, id: &str, new_title: &str) -> Result<CaptureRecord, CmError> {
        let rec = self.get(id)?;
        if rec.broken {
            return Err(CmError::invalid_input(
                "cannot rename a broken capture; repair it first",
            ));
        }
        let base = sanitize_title(new_title)?;
        let old = self.bundle_path(&rec);
        let mpath = old.join(MANIFEST_FILE);
        let text =
            std::fs::read_to_string(&mpath).map_err(|e| io_err("cannot read", &mpath, &e))?;
        let mut v: Value =
            serde_json::from_str(&text).map_err(|e| json_err("manifest.json", &e))?;
        check_schema(&v, "manifest.json").map_err(ManifestProblem::into_error)?;
        if let Some(m) = v.as_object_mut() {
            m.insert(
                "title".to_owned(),
                Value::String(new_title.trim().to_owned()),
            );
        }
        let out = serde_json::to_string_pretty(&v).map_err(|e| json_err("manifest.json", &e))?;
        write_atomic(&mpath, out.as_bytes())?;
        let new = free_bundle_path(&self.root, &base, rec.kind, &old);
        if new != old {
            std::fs::rename(&old, &new).map_err(|e| io_err("cannot rename", &old, &e))?;
        }
        self.index_dir(&new)
    }

    // ---- duplicate -------------------------------------------------------------------------

    /// Copies a capture to a new bundle with a fresh id and the title `"<title> copy"`, and
    /// indexes it. Copying folders by hand yields two bundles with one id (see
    /// [`DuplicateBundle`]); this is the supported way. A recording copies all its media, so it
    /// takes as long as the copy.
    pub fn duplicate(&mut self, id: &str) -> Result<CaptureRecord, CmError> {
        let rec = self.get(id)?;
        if rec.broken {
            return Err(CmError::invalid_input("cannot duplicate a broken capture"));
        }
        let src = self.bundle_path(&rec);
        if src.join(LOCK_FILE).exists() {
            return Err(CmError::busy("the recording is still in progress"));
        }
        let title = format!("{} copy", rec.title.trim());
        let base = sanitize_title(&title)?;
        let dst = free_bundle_path(&self.root, &base, rec.kind, Path::new(""));
        let res = (|| {
            copy_dir(&src, &dst)?;
            let mpath = dst.join(MANIFEST_FILE);
            let text =
                std::fs::read_to_string(&mpath).map_err(|e| io_err("cannot read", &mpath, &e))?;
            let mut v: Value =
                serde_json::from_str(&text).map_err(|e| json_err("manifest.json", &e))?;
            let prefix = if rec.kind == CaptureKind::Screenshot {
                "shot"
            } else {
                "rec"
            };
            if let Some(m) = v.as_object_mut() {
                m.insert(
                    "id".to_owned(),
                    Value::String(crate::util::new_id(prefix, OffsetDateTime::now_utc())),
                );
                m.insert("title".to_owned(), Value::String(title.clone()));
            }
            let out =
                serde_json::to_string_pretty(&v).map_err(|e| json_err("manifest.json", &e))?;
            write_atomic(&mpath, out.as_bytes())?;
            self.index_dir(&dst)
        })();
        if res.is_err() {
            let _ = std::fs::remove_dir_all(&dst);
        }
        res
    }

    // ---- rescan ----------------------------------------------------------------------------

    /// Rebuilds the index from the bundle folders directly under the root (LIB-1.AC3).
    ///
    /// New bundles are added, changed ones refreshed, vanished ones removed. Unreadable bundles
    /// are indexed as `broken`. Transcript rows of surviving captures are kept; they are not
    /// recoverable from folders yet (see the crate docs).
    pub fn rescan(&mut self) -> Result<RescanReport, CmError> {
        let mut report = RescanReport::default();
        let rd =
            std::fs::read_dir(&self.root).map_err(|e| io_err("cannot read", &self.root, &e))?;
        let mut dirs: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir() && kind_of_dir(p).is_some())
            .collect();
        dirs.sort();
        // Read every bundle first so duplicate ids can be resolved before indexing (R1-5).
        let mut entries: Vec<(String, CaptureRecord, std::time::SystemTime)> = Vec::new();
        for dir in dirs {
            let name = file_name(&dir);
            if dir.join(LOCK_FILE).exists() {
                report.incomplete.push(name);
                continue;
            }
            let rec = match self.read_record(&dir) {
                Ok(r) => r,
                Err(_) => {
                    report.broken.push(name.clone());
                    broken_record(&dir, &name)
                }
            };
            let age = std::fs::metadata(dir.join(MANIFEST_FILE))
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            entries.push((name, rec, age));
        }
        // Per id, the folder to keep.
        let mut keep: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for (i, (_, rec, age)) in entries.iter().enumerate() {
            let indexed = self.get(&rec.id).ok().map(|r| r.path);
            let better = |j: usize| -> bool {
                // Is entry `i` preferred over the current pick `j`?
                let (_, jr, jage) = &entries[j];
                let i_indexed = indexed.as_deref() == Some(rec.path.as_str());
                let j_indexed = indexed.as_deref() == Some(jr.path.as_str());
                if i_indexed != j_indexed {
                    return i_indexed;
                }
                if age != jage {
                    return age < jage;
                }
                let (il, jl) = (rec.path.chars().count(), jr.path.chars().count());
                if il != jl {
                    return il < jl;
                }
                rec.path < jr.path
            };
            match keep.get(&rec.id) {
                Some(&j) if !better(j) => {}
                _ => {
                    keep.insert(rec.id.clone(), i);
                }
            }
        }
        let mut seen_paths = HashSet::new();
        for (i, (name, rec, _)) in entries.iter().enumerate() {
            let k = keep[&rec.id];
            if k != i {
                report.duplicate_ids.push(name.clone());
                report.duplicates.push(DuplicateBundle {
                    id: rec.id.clone(),
                    kept: entries[k].0.clone(),
                    ignored: name.clone(),
                });
                continue;
            }
            seen_paths.insert(rec.path.clone());
            self.upsert(rec)?;
            report.indexed += 1;
        }
        let stale: Vec<String> = {
            let mut st = self
                .conn
                .prepare("SELECT id, path FROM captures")
                .map_err(|e| sql_err(&e))?;
            let rows = st
                .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
                .map_err(|e| sql_err(&e))?;
            let mut v = Vec::new();
            for row in rows {
                let (id, path) = row.map_err(|e| sql_err(&e))?;
                if !seen_paths.contains(&path) {
                    v.push(id);
                }
            }
            v
        };
        for id in stale {
            if self.remove(&id)? {
                report.removed += 1;
            }
        }
        Ok(report)
    }

    /// Builds a record from a bundle folder under the root.
    fn read_record(&self, dir: &Path) -> Result<CaptureRecord, CmError> {
        let kind = kind_of_dir(dir)
            .ok_or_else(|| CmError::invalid_input("not a .cmshot or .cmrec folder"))?;
        let rel = dir
            .strip_prefix(&self.root)
            .map_err(|_| CmError::invalid_input("bundle is outside the library root"))?
            .to_string_lossy()
            .into_owned();
        let mpath = dir.join(MANIFEST_FILE);
        let text =
            std::fs::read_to_string(&mpath).map_err(|e| io_err("cannot read", &mpath, &e))?;
        let size_bytes = dir_size(dir);
        let counts = remote_counts(&text);
        match kind {
            CaptureKind::Screenshot => {
                let m = ShotManifest::from_json(&text).map_err(ManifestProblem::into_error)?;
                Ok(CaptureRecord {
                    id: m.id,
                    path: rel,
                    title: m.title,
                    kind,
                    duration: None,
                    created: m.created,
                    size_bytes,
                    width: Some(m.original.size.width),
                    height: Some(m.original.size.height),
                    broken: false,
                    upload_count: counts.0,
                    publish_count: counts.1,
                })
            }
            CaptureKind::Recording | CaptureKind::Audio => {
                let m = RecManifest::from_json(&text).map_err(ManifestProblem::into_error)?;
                // An audio recording has no screen track (VOC-1): no size, kind `audio`.
                let kind = m.capture_kind();
                let size = m
                    .tracks
                    .get("screen")
                    .and_then(|t| t.get("size"))
                    .and_then(Value::as_array)
                    .and_then(|a| Some((a.first()?.as_u64()?, a.get(1)?.as_u64()?)));
                Ok(CaptureRecord {
                    id: m.id,
                    path: rel,
                    title: m.title,
                    kind,
                    duration: m.duration,
                    created: m.created,
                    size_bytes,
                    width: size.and_then(|s| u32::try_from(s.0).ok()),
                    height: size.and_then(|s| u32::try_from(s.1).ok()),
                    broken: false,
                    upload_count: counts.0,
                    publish_count: counts.1,
                })
            }
        }
    }
}

/// How many `uploads[]` and `publishes[]` records a manifest text holds (`(0, 0)` when it does
/// not parse as JSON; the typed parse reports that problem).
fn remote_counts(manifest_text: &str) -> (u32, u32) {
    let Ok(v) = serde_json::from_str::<Value>(manifest_text) else {
        return (0, 0);
    };
    let n = |key: &str| {
        v.get(key)
            .and_then(Value::as_array)
            .map_or(0, |a| u32::try_from(a.len()).unwrap_or(u32::MAX))
    };
    (n("uploads"), n("publishes"))
}

fn copy_dir(src: &Path, dst: &Path) -> Result<(), CmError> {
    std::fs::create_dir_all(dst).map_err(|e| io_err("cannot create", dst, &e))?;
    for entry in std::fs::read_dir(src)
        .map_err(|e| io_err("cannot read", src, &e))?
        .flatten()
    {
        let (from, to) = (entry.path(), dst.join(entry.file_name()));
        if from.is_dir() {
            copy_dir(&from, &to)?;
        } else {
            std::fs::copy(&from, &to).map_err(|e| io_err("cannot copy", &from, &e))?;
        }
    }
    Ok(())
}

fn file_name(p: &Path) -> String {
    p.file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
}

fn broken_record(dir: &Path, name: &str) -> CaptureRecord {
    let created = std::fs::metadata(dir)
        .and_then(|m| m.modified())
        .map(OffsetDateTime::from)
        .unwrap_or(OffsetDateTime::UNIX_EPOCH);
    CaptureRecord {
        id: format!("broken:{name}"),
        path: name.to_owned(),
        title: dir
            .file_stem()
            .map_or_else(|| name.to_owned(), |s| s.to_string_lossy().into_owned()),
        kind: kind_of_dir(dir).unwrap_or(CaptureKind::Screenshot),
        duration: None,
        created,
        size_bytes: dir_size(dir),
        width: None,
        height: None,
        broken: true,
        upload_count: 0,
        publish_count: 0,
    }
}

/// `"<prefix> yyyy-MM-dd HH.mm"` (LIB-1 default title).
pub fn default_title(prefix: &str, now: OffsetDateTime) -> Result<String, CmError> {
    let fmt = time::macros::format_description!("[year]-[month]-[day] [hour].[minute]");
    let stamp = now
        .format(&fmt)
        .map_err(|e| CmError::internal(e.to_string()))?;
    Ok(format!("{prefix} {stamp}"))
}

const MIGRATE_2_TO_3: &str = "
BEGIN;
CREATE TABLE captures_v3 (
  id           TEXT PRIMARY KEY,
  path         TEXT NOT NULL UNIQUE,
  title        TEXT NOT NULL,
  kind         TEXT NOT NULL CHECK (kind IN ('screenshot', 'recording', 'audio')),
  duration     INTEGER,
  created      TEXT NOT NULL,
  created_unix INTEGER NOT NULL,
  size_bytes   INTEGER NOT NULL DEFAULT 0,
  width        INTEGER,
  height       INTEGER,
  broken       INTEGER NOT NULL DEFAULT 0,
  upload_count  INTEGER NOT NULL DEFAULT 0,
  publish_count INTEGER NOT NULL DEFAULT 0
);
INSERT INTO captures_v3 (id, path, title, kind, duration, created, created_unix, size_bytes, width, height, broken, upload_count, publish_count)
  SELECT id, path, title, kind, duration, created, created_unix, size_bytes, width, height, broken, upload_count, publish_count FROM captures;
DROP TABLE captures;
ALTER TABLE captures_v3 RENAME TO captures;
CREATE INDEX captures_created ON captures (created_unix DESC);
CREATE INDEX captures_kind ON captures (kind);
PRAGMA user_version = 3;
COMMIT;
";

const SCHEMA: &str = "
CREATE TABLE captures (
  id           TEXT PRIMARY KEY,
  path         TEXT NOT NULL UNIQUE,
  title        TEXT NOT NULL,
  kind         TEXT NOT NULL CHECK (kind IN ('screenshot', 'recording', 'audio')),
  duration     INTEGER,
  created      TEXT NOT NULL,
  created_unix INTEGER NOT NULL,
  size_bytes   INTEGER NOT NULL DEFAULT 0,
  width        INTEGER,
  height       INTEGER,
  broken       INTEGER NOT NULL DEFAULT 0,
  upload_count  INTEGER NOT NULL DEFAULT 0,
  publish_count INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX captures_created ON captures (created_unix DESC);
CREATE INDEX captures_kind ON captures (kind);
CREATE VIRTUAL TABLE transcripts_fts USING fts5(
  text, capture_id UNINDEXED, start_ticks UNINDEXED,
  tokenize = 'unicode61 remove_diacritics 2'
);
";

const COLS: &str = "id, path, title, kind, duration, created, size_bytes, width, height, broken, upload_count, publish_count";

fn row_to_record(r: &rusqlite::Row<'_>) -> rusqlite::Result<CaptureRecord> {
    let bad = |i: usize, msg: &str| {
        rusqlite::Error::FromSqlConversionFailure(
            i,
            rusqlite::types::Type::Text,
            msg.to_owned().into(),
        )
    };
    let kind: String = r.get(3)?;
    let created: String = r.get(5)?;
    let size: i64 = r.get(6)?;
    Ok(CaptureRecord {
        id: r.get(0)?,
        path: r.get(1)?,
        title: r.get(2)?,
        kind: CaptureKind::parse(&kind).ok_or_else(|| bad(3, "unknown kind"))?,
        duration: r.get::<_, Option<i64>>(4)?.map(MediaTime::from_ticks),
        created: OffsetDateTime::parse(&created, &Rfc3339).map_err(|e| bad(5, &e.to_string()))?,
        size_bytes: u64::try_from(size).unwrap_or(0),
        width: r.get(7)?,
        height: r.get(8)?,
        broken: r.get(9)?,
        upload_count: r.get::<_, i64>(10).map(|n| u32::try_from(n).unwrap_or(0))?,
        publish_count: r.get::<_, i64>(11).map(|n| u32::try_from(n).unwrap_or(0))?,
    })
}

/// `WHERE` clause for kind/date filters; when `title_words` is set, that many `title LIKE ?`
/// terms (bound first by the caller) are prepended.
fn where_clause(q: &ListQuery, title_words: Option<usize>) -> (String, Vec<SqlValue>) {
    let mut conds: Vec<String> = Vec::new();
    let mut args = Vec::new();
    for _ in 0..title_words.unwrap_or(0) {
        conds.push("title LIKE ? ESCAPE '\\'".to_owned());
    }
    if let Some(k) = q.kind {
        conds.push("kind = ?".to_owned());
        args.push(SqlValue::Text(k.as_str().to_owned()));
    }
    if let Some(st) = q.status {
        conds.push(st.sql().to_owned());
    }
    if let Some(from) = q.created_from {
        conds.push("created_unix >= ?".to_owned());
        args.push(SqlValue::Integer(from.unix_timestamp()));
    }
    if let Some(to) = q.created_to {
        conds.push("created_unix < ?".to_owned());
        args.push(SqlValue::Integer(to.unix_timestamp()));
    }
    let sql = if conds.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conds.join(" AND "))
    };
    (sql, args)
}

fn passes(q: &ListQuery, r: &CaptureRecord) -> bool {
    q.kind.is_none_or(|k| k == r.kind)
        && q.status.is_none_or(|s| s.matches(r))
        && q.created_from.is_none_or(|f| r.created >= f)
        && q.created_to.is_none_or(|t| r.created < t)
}
