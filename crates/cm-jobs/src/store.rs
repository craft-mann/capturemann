//! SQLite persistence for jobs (`jobs`, `job_deps` tables; `CREATE IF NOT EXISTS`, so the
//! tables can live in the library index database or a dedicated file).
//!
//! The API is synchronous and every call is short; async code calls it directly.

use std::collections::hash_map::RandomState;
use std::collections::HashSet;
use std::hash::BuildHasher;
use std::path::Path;
use std::sync::{Mutex, MutexGuard, PoisonError};

use cm_core::{CmError, ErrorCode};
use rusqlite::types::ToSql;
use rusqlite::{Connection, OptionalExtension, Row};
use serde_json::Value;

use crate::model::{Job, JobError, JobFilter, JobId, JobState, NewJob};
use crate::scrub::scrub_secrets;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS jobs (
    id              TEXT PRIMARY KEY NOT NULL,
    job_type        TEXT NOT NULL,
    inputs          TEXT NOT NULL,
    state           TEXT NOT NULL,
    progress        REAL NOT NULL DEFAULT 0,
    message         TEXT,
    attempts        INTEGER NOT NULL DEFAULT 0,
    checkpoint      BLOB,
    result          TEXT,
    error_code      TEXT,
    error_reason    TEXT,
    pause_reason    TEXT,
    not_before_ms   INTEGER NOT NULL DEFAULT 0,
    created_at_ms   INTEGER NOT NULL,
    updated_at_ms   INTEGER NOT NULL,
    started_at_ms   INTEGER,
    finished_at_ms  INTEGER
);
CREATE INDEX IF NOT EXISTS jobs_state ON jobs(state);
CREATE TABLE IF NOT EXISTS job_deps (
    job_id      TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    depends_on  TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    PRIMARY KEY (job_id, depends_on)
);
CREATE INDEX IF NOT EXISTS job_deps_on ON job_deps(depends_on);
";

const COLUMNS: &str = "id, job_type, inputs, state, progress, message, attempts, checkpoint, \
    result, error_code, error_reason, pause_reason, not_before_ms, created_at_ms, \
    updated_at_ms, started_at_ms, finished_at_ms";

/// Creates `path` empty with mode 0600 if it does not exist, and tightens an existing one.
fn private_file(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .open(path)?;
    }
    #[cfg(not(unix))]
    {
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
    }
    restrict(path)
}

/// Owner-only permissions (no-op when the file does not exist or off Unix).
fn restrict(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if path.exists() {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        }
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

fn db_err(e: rusqlite::Error) -> CmError {
    CmError::internal(format!("job store: {e}"))
}

fn conv_err(
    idx: usize,
    ty: rusqlite::types::Type,
    e: impl std::error::Error + Send + Sync + 'static,
) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(idx, ty, Box::new(e))
}

fn states_sql(states: &[JobState]) -> String {
    states
        .iter()
        .map(|s| format!("'{}'", s.as_str()))
        .collect::<Vec<_>>()
        .join(",")
}

fn row_to_job(row: &Row<'_>) -> rusqlite::Result<Job> {
    use rusqlite::types::Type;
    let inputs: String = row.get(2)?;
    let state: String = row.get(3)?;
    let checkpoint: Option<Vec<u8>> = row.get(7)?;
    let result: Option<String> = row.get(8)?;
    let error_code: Option<String> = row.get(9)?;
    let error_reason: Option<String> = row.get(10)?;
    let error = match (error_code, error_reason) {
        (Some(c), Some(reason)) => {
            let code =
                serde_json::from_value(Value::String(c)).map_err(|e| conv_err(9, Type::Text, e))?;
            Some(JobError { code, reason })
        }
        _ => None,
    };
    Ok(Job {
        id: row.get(0)?,
        job_type: row.get(1)?,
        inputs: serde_json::from_str(&inputs).map_err(|e| conv_err(2, Type::Text, e))?,
        state: JobState::parse(&state).ok_or_else(|| {
            conv_err(
                3,
                Type::Text,
                std::io::Error::new(std::io::ErrorKind::InvalidData, format!("state {state}")),
            )
        })?,
        progress: row.get(4)?,
        message: row.get(5)?,
        attempts: row.get(6)?,
        depends_on: Vec::new(),
        checkpoint: checkpoint
            .map(|b| serde_json::from_slice(&b).map_err(|e| conv_err(7, Type::Blob, e)))
            .transpose()?,
        result: result
            .map(|r| serde_json::from_str(&r).map_err(|e| conv_err(8, Type::Text, e)))
            .transpose()?,
        error,
        pause_reason: row.get(11)?,
        not_before_ms: row.get(12)?,
        created_at_ms: row.get(13)?,
        updated_at_ms: row.get(14)?,
        started_at_ms: row.get(15)?,
        finished_at_ms: row.get(16)?,
    })
}

/// Persistent job table.
#[derive(Debug)]
pub struct JobStore {
    conn: Mutex<Connection>,
}

impl JobStore {
    /// Opens (creating it) the database. The file and its `-wal` / `-shm` side files are
    /// owner-only (0600) on Unix: `checkpoint` can hold upload session URLs (R2-12).
    pub fn open(path: impl AsRef<Path>) -> Result<Self, CmError> {
        let path = path.as_ref();
        private_file(path).map_err(|e| {
            CmError::internal(format!(
                "job store: cannot create `{}`: {e}",
                path.display()
            ))
        })?;
        let conn = Connection::open(path).map_err(db_err)?;
        // WAL is a persistent no-op on shared databases that already use it.
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(db_err)?;
        let store = Self::init(conn)?;
        for suffix in ["-wal", "-shm"] {
            let mut side = path.as_os_str().to_owned();
            side.push(suffix);
            let _ = restrict(Path::new(&side));
        }
        Ok(store)
    }

    pub fn open_in_memory() -> Result<Self, CmError> {
        Self::init(Connection::open_in_memory().map_err(db_err)?)
    }

    fn init(conn: Connection) -> Result<Self, CmError> {
        conn.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(db_err)?;
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(db_err)?;
        conn.execute_batch(SCHEMA).map_err(db_err)?;
        Ok(JobStore {
            conn: Mutex::new(conn),
        })
    }

    fn conn(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn new_id() -> JobId {
        let s = RandomState::new();
        format!("job_{:016x}", s.hash_one(0u8))
    }

    fn load(conn: &Connection, id: &str) -> Result<Option<Job>, CmError> {
        let q = format!("SELECT {COLUMNS} FROM jobs WHERE id = ?1");
        let mut job = match conn.query_row(&q, [id], row_to_job).optional() {
            Ok(j) => j,
            Err(e @ rusqlite::Error::FromSqlConversionFailure(..)) => {
                // A poison row (R2-12): repair it so it can be read (and shown as failed)
                // instead of failing every query that touches it.
                Self::repair(conn, id, &e.to_string());
                conn.query_row(&q, [id], row_to_job)
                    .optional()
                    .map_err(db_err)?
            }
            Err(e) => return Err(db_err(e)),
        };
        if let Some(j) = &mut job {
            j.depends_on = Self::deps_of(conn, &j.id)?;
        }
        Ok(job)
    }

    fn deps_of(conn: &Connection, id: &str) -> Result<Vec<JobId>, CmError> {
        let mut stmt = conn
            .prepare("SELECT depends_on FROM job_deps WHERE job_id = ?1 ORDER BY rowid")
            .map_err(db_err)?;
        let rows = stmt
            .query_map([id], |r| r.get::<_, String>(0))
            .map_err(db_err)?;
        rows.collect::<Result<_, _>>().map_err(db_err)
    }

    /// Makes an unreadable row readable: unparsable `inputs` / `result` / `checkpoint` /
    /// `error_code` / `state` are replaced, and a job that is not finished becomes `failed`
    /// with a reason (it cannot run with unreadable inputs). Never panics, never fails the
    /// caller: it logs one line.
    fn repair(conn: &Connection, id: &str, why: &str) {
        log::warn!("repairing unreadable job row `{id}`: {why}");
        let reason = scrub_secrets(&format!("The job record was unreadable ({why})"));
        let steps = [
            "UPDATE jobs SET inputs = '{}' WHERE id = ?1 AND NOT json_valid(inputs)",
            "UPDATE jobs SET result = NULL WHERE id = ?1 AND result IS NOT NULL AND NOT json_valid(result)",
            "UPDATE jobs SET checkpoint = NULL WHERE id = ?1 AND checkpoint IS NOT NULL AND NOT json_valid(checkpoint)",
        ];
        for sql in steps {
            let _ = conn.execute(sql, [id]);
        }
        let list = ErrorCode::ALL
            .iter()
            .map(|c| format!("'{}'", c.as_str()))
            .collect::<Vec<_>>()
            .join(",");
        let _ = conn.execute(
            "UPDATE jobs SET state = 'failed', error_code = 'internal', error_reason = ?2,
                finished_at_ms = COALESCE(finished_at_ms, CAST(strftime('%s','now') AS INTEGER) * 1000)
             WHERE id = ?1 AND state NOT IN ('done','failed','cancelled')",
            rusqlite::params![id, reason],
        );
        let _ = conn.execute(
            &format!(
                "UPDATE jobs SET error_code = 'internal', error_reason = COALESCE(error_reason, ?2)
                 WHERE id = ?1 AND error_code IS NOT NULL AND error_code NOT IN ({list})"
            ),
            rusqlite::params![id, reason],
        );
    }

    fn load_many(conn: &Connection, sql: &str, params: &[&dyn ToSql]) -> Result<Vec<Job>, CmError> {
        let mut stmt = conn.prepare(sql).map_err(db_err)?;
        let mut jobs: Vec<Job> = Vec::new();
        let mut bad: Vec<(String, String)> = Vec::new();
        {
            let rows = stmt
                .query_map(params, |r| {
                    let id: String = r.get(0)?;
                    Ok(row_to_job(r).map_err(|e| (id, e.to_string())))
                })
                .map_err(db_err)?;
            for row in rows {
                match row.map_err(db_err)? {
                    Ok(j) => jobs.push(j),
                    Err(b) => bad.push(b),
                }
            }
        }
        // Poison rows are repaired (marked failed with a reason) and left out of this result;
        // the next query shows them as failed jobs.
        for (id, why) in bad {
            Self::repair(conn, &id, &why);
        }
        jobs.into_iter()
            .map(|mut j| {
                j.depends_on = Self::deps_of(conn, &j.id)?;
                Ok(j)
            })
            .collect()
    }

    /// Inserts a `queued` job. Unknown dependencies are `not_found`; a self-dependency or a
    /// reused id is `invalid_input`.
    pub fn insert(&self, new: &NewJob, now_ms: i64) -> Result<Job, CmError> {
        if new.job_type.trim().is_empty() {
            return Err(CmError::invalid_field("job_type", "must not be empty"));
        }
        let id = new.id.clone().unwrap_or_else(Self::new_id);
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(db_err)?;
        if Self::load(&tx, &id)?.is_some() {
            return Err(CmError::invalid_field(
                "id",
                format!("job {id} already exists"),
            ));
        }
        let mut seen = HashSet::new();
        for dep in &new.depends_on {
            if dep == &id {
                return Err(CmError::invalid_field(
                    "depends_on",
                    "a job cannot depend on itself",
                ));
            }
            if !seen.insert(dep.clone()) {
                continue;
            }
            if Self::load(&tx, dep)?.is_none() {
                return Err(CmError::not_found(format!(
                    "dependency job {dep} not found"
                )));
            }
        }
        tx.execute(
            "INSERT INTO jobs (id, job_type, inputs, state, created_at_ms, updated_at_ms)
             VALUES (?1, ?2, ?3, 'queued', ?4, ?4)",
            rusqlite::params![id, new.job_type, new.inputs.to_string(), now_ms],
        )
        .map_err(db_err)?;
        for dep in &seen {
            tx.execute(
                "INSERT INTO job_deps (job_id, depends_on) VALUES (?1, ?2)",
                [&id, dep],
            )
            .map_err(db_err)?;
        }
        let job = Self::load(&tx, &id)?.ok_or_else(|| CmError::internal("job vanished"))?;
        tx.commit().map_err(db_err)?;
        Ok(job)
    }

    pub fn get(&self, id: &str) -> Result<Option<Job>, CmError> {
        Self::load(&self.conn(), id)
    }

    /// Newest first.
    pub fn list(&self, filter: &JobFilter) -> Result<Vec<Job>, CmError> {
        let mut sql = format!("SELECT {COLUMNS} FROM jobs WHERE 1=1");
        let mut params: Vec<&dyn ToSql> = Vec::new();
        if let Some(t) = &filter.job_type {
            sql.push_str(" AND job_type = ?1");
            params.push(t);
        }
        if let Some(states) = &filter.states {
            sql.push_str(&format!(" AND state IN ({})", states_sql(states)));
        }
        sql.push_str(" ORDER BY rowid DESC");
        if let Some(limit) = filter.limit {
            sql.push_str(&format!(" LIMIT {limit}"));
        }
        Self::load_many(&self.conn(), &sql, &params)
    }

    /// Queued jobs that may start now: backoff elapsed and every dependency `done`.
    /// Oldest first.
    pub fn eligible(&self, now_ms: i64) -> Result<Vec<Job>, CmError> {
        Self::load_many(
            &self.conn(),
            &format!(
                "SELECT {COLUMNS} FROM jobs j
                 WHERE state = 'queued' AND not_before_ms <= ?1
                   AND NOT EXISTS (
                     SELECT 1 FROM job_deps d JOIN jobs p ON p.id = d.depends_on
                     WHERE d.job_id = j.id AND p.state != 'done')
                 ORDER BY rowid"
            ),
            &[&now_ms],
        )
    }

    /// Jobs that list `id` as a dependency.
    pub fn dependents(&self, id: &str) -> Result<Vec<JobId>, CmError> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare("SELECT job_id FROM job_deps WHERE depends_on = ?1 ORDER BY rowid")
            .map_err(db_err)?;
        let rows = stmt
            .query_map([id], |r| r.get::<_, String>(0))
            .map_err(db_err)?;
        rows.collect::<Result<_, _>>().map_err(db_err)
    }

    /// Adds a dependency to a `queued`/`paused` job; cycles are `invalid_input`.
    pub fn add_dependency(&self, id: &str, depends_on: &str) -> Result<Job, CmError> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(db_err)?;
        let job = Self::load(&tx, id)?
            .ok_or_else(|| CmError::not_found(format!("job {id} not found")))?;
        if Self::load(&tx, depends_on)?.is_none() {
            return Err(CmError::not_found(format!(
                "dependency job {depends_on} not found"
            )));
        }
        if !matches!(job.state, JobState::Queued | JobState::Paused) {
            return Err(CmError::invalid_field(
                "job",
                format!("cannot add a dependency to a {} job", job.state.as_str()),
            ));
        }
        if id == depends_on {
            return Err(CmError::invalid_field(
                "depends_on",
                "a job cannot depend on itself",
            ));
        }
        // Cycle iff `id` is reachable from `depends_on` through existing dependencies.
        let mut stack = vec![depends_on.to_owned()];
        let mut seen = HashSet::new();
        while let Some(n) = stack.pop() {
            if n == id {
                return Err(CmError::invalid_field(
                    "depends_on",
                    format!("dependency cycle: {depends_on} already depends on {id}"),
                ));
            }
            if seen.insert(n.clone()) {
                stack.extend(Self::deps_of(&tx, &n)?);
            }
        }
        tx.execute(
            "INSERT OR IGNORE INTO job_deps (job_id, depends_on) VALUES (?1, ?2)",
            [id, depends_on],
        )
        .map_err(db_err)?;
        let job = Self::load(&tx, id)?.ok_or_else(|| CmError::internal("job vanished"))?;
        tx.commit().map_err(db_err)?;
        Ok(job)
    }

    /// `UPDATE jobs SET <set>, updated_at_ms = now WHERE id AND state IN (from)`.
    /// Returns the updated job, or `None` when the job was not in a `from` state.
    fn set(
        &self,
        id: &str,
        from: &[JobState],
        set: &str,
        now_ms: i64,
        extra: &[(&str, &dyn ToSql)],
    ) -> Result<Option<Job>, CmError> {
        let conn = self.conn();
        let sql = format!(
            "UPDATE jobs SET {set}, updated_at_ms = :now WHERE id = :id AND state IN ({})",
            states_sql(from)
        );
        let mut params: Vec<(&str, &dyn ToSql)> = vec![(":now", &now_ms), (":id", &id)];
        params.extend_from_slice(extra);
        let n = conn.execute(&sql, params.as_slice()).map_err(db_err)?;
        if n == 0 {
            return Ok(None);
        }
        Self::load(&conn, id)
    }

    /// `queued -> running`, counting an attempt.
    pub fn try_start(&self, id: &str, now_ms: i64) -> Result<Option<Job>, CmError> {
        self.set(
            id,
            &[JobState::Queued],
            "state = 'running', attempts = attempts + 1, started_at_ms = :now, \
             finished_at_ms = NULL, pause_reason = NULL",
            now_ms,
            &[],
        )
    }

    /// Updates progress of a running job (clamped to 0..=1).
    pub fn set_progress(
        &self,
        id: &str,
        progress: f64,
        message: Option<&str>,
        now_ms: i64,
    ) -> Result<Option<Job>, CmError> {
        let message = message.map(scrub_secrets);
        let message = message.as_deref();
        let p = if progress.is_finite() {
            progress.clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.set(
            id,
            &[JobState::Running],
            "progress = :p, message = COALESCE(:m, message)",
            now_ms,
            &[(":p", &p), (":m", &message)],
        )
    }

    /// Saves the resumable-state blob of a running job (STO-3, NFR-8).
    pub fn save_checkpoint(
        &self,
        id: &str,
        checkpoint: &Value,
        now_ms: i64,
    ) -> Result<bool, CmError> {
        let bytes =
            serde_json::to_vec(checkpoint).map_err(|e| CmError::invalid_field("checkpoint", e))?;
        Ok(self
            .set(
                id,
                &[JobState::Running],
                "checkpoint = :c",
                now_ms,
                &[(":c", &bytes)],
            )?
            .is_some())
    }

    pub fn finish_done(
        &self,
        id: &str,
        result: &Value,
        now_ms: i64,
    ) -> Result<Option<Job>, CmError> {
        self.set(
            id,
            &[JobState::Running],
            "state = 'done', progress = 1.0, result = :r, error_code = NULL, \
             error_reason = NULL, finished_at_ms = :now",
            now_ms,
            &[(":r", &result.to_string())],
        )
    }

    pub fn fail(
        &self,
        id: &str,
        from: &[JobState],
        code: ErrorCode,
        reason: &str,
        now_ms: i64,
    ) -> Result<Option<Job>, CmError> {
        self.set(
            id,
            from,
            "state = 'failed', error_code = :c, error_reason = :r, finished_at_ms = :now",
            now_ms,
            &[(":c", &code.as_str()), (":r", &scrub_secrets(reason))],
        )
    }

    /// Back to `queued` (retry backoff, shutdown, hold released). `decrement_attempt` undoes
    /// the attempt count for interruptions that are not failures.
    pub fn requeue(
        &self,
        id: &str,
        not_before_ms: i64,
        error: Option<(ErrorCode, &str)>,
        decrement_attempt: bool,
        now_ms: i64,
    ) -> Result<Option<Job>, CmError> {
        let (code, reason) = match error {
            Some((c, r)) => (Some(c.as_str()), Some(scrub_secrets(r))),
            None => (None, None),
        };
        let set = if decrement_attempt {
            "state = 'queued', not_before_ms = :nb, attempts = MAX(attempts - 1, 0), \
             pause_reason = NULL, error_code = COALESCE(:c, error_code), \
             error_reason = COALESCE(:r, error_reason)"
        } else {
            "state = 'queued', not_before_ms = :nb, pause_reason = NULL, \
             error_code = COALESCE(:c, error_code), error_reason = COALESCE(:r, error_reason)"
        };
        self.set(
            id,
            &[JobState::Running, JobState::Paused],
            set,
            now_ms,
            &[(":nb", &not_before_ms), (":c", &code), (":r", &reason)],
        )
    }

    pub fn set_paused(
        &self,
        id: &str,
        reason: &str,
        decrement_attempt: bool,
        now_ms: i64,
    ) -> Result<Option<Job>, CmError> {
        let set = if decrement_attempt {
            "state = 'paused', pause_reason = :pr, attempts = MAX(attempts - 1, 0)"
        } else {
            "state = 'paused', pause_reason = :pr"
        };
        self.set(
            id,
            &[JobState::Running, JobState::Queued],
            set,
            now_ms,
            &[(":pr", &reason)],
        )
    }

    /// A running job waits for the user to sign in again: `paused` with `pause_reason`
    /// [`crate::PAUSE_AUTH_EXPIRED`], the reason kept in `error`, the attempt not counted.
    pub fn set_paused_auth_expired(
        &self,
        id: &str,
        reason: &str,
        now_ms: i64,
    ) -> Result<Option<Job>, CmError> {
        self.set(
            id,
            &[JobState::Running],
            "state = 'paused', pause_reason = :pr, attempts = MAX(attempts - 1, 0), \
             error_code = :c, error_reason = :r",
            now_ms,
            &[
                (":pr", &crate::model::PAUSE_AUTH_EXPIRED),
                (":c", &ErrorCode::AuthExpired.as_str()),
                (":r", &scrub_secrets(reason)),
            ],
        )
    }

    pub fn set_cancelled(&self, id: &str, now_ms: i64) -> Result<Option<Job>, CmError> {
        self.set(
            id,
            &[JobState::Running, JobState::Queued, JobState::Paused],
            "state = 'cancelled', pause_reason = NULL, finished_at_ms = :now",
            now_ms,
            &[],
        )
    }

    /// `paused -> queued`.
    pub fn resume(&self, id: &str, now_ms: i64) -> Result<Option<Job>, CmError> {
        self.set(
            id,
            &[JobState::Paused],
            "state = 'queued', pause_reason = NULL",
            now_ms,
            &[],
        )
    }

    /// `failed -> queued` with attempts reset; the checkpoint is kept so work resumes.
    pub fn reset_for_retry(&self, id: &str, now_ms: i64) -> Result<Option<Job>, CmError> {
        self.set(
            id,
            &[JobState::Failed],
            "state = 'queued', attempts = 0, not_before_ms = 0, error_code = NULL, \
             error_reason = NULL, finished_at_ms = NULL",
            now_ms,
            &[],
        )
    }

    /// `cancelled -> queued` with attempts and checkpoint reset: a cancel cleaned up the partial
    /// output, so the work starts over.
    pub fn reset_cancelled(&self, id: &str, now_ms: i64) -> Result<Option<Job>, CmError> {
        self.set(
            id,
            &[JobState::Cancelled],
            "state = 'queued', attempts = 0, not_before_ms = 0, checkpoint = NULL, \
             error_code = NULL, error_reason = NULL, finished_at_ms = NULL, pause_reason = NULL",
            now_ms,
            &[],
        )
    }

    /// Resumes every paused job whose pause reason is `reason`.
    pub fn resume_by_reason(&self, reason: &str, now_ms: i64) -> Result<Vec<Job>, CmError> {
        let ids: Vec<String> = {
            let conn = self.conn();
            let mut stmt = conn
                .prepare("SELECT id FROM jobs WHERE state = 'paused' AND pause_reason = ?1 ORDER BY rowid")
                .map_err(db_err)?;
            let rows = stmt.query_map([reason], |r| r.get(0)).map_err(db_err)?;
            rows.collect::<Result<_, _>>().map_err(db_err)?
        };
        let mut out = Vec::new();
        for id in ids {
            if let Some(j) = self.resume(&id, now_ms)? {
                out.push(j);
            }
        }
        Ok(out)
    }

    /// Crash recovery: `running` and automatically held (`hold:*`) jobs return to `queued`
    /// with their checkpoint; the interrupted attempt is not counted.
    pub fn recover(&self, now_ms: i64) -> Result<Vec<Job>, CmError> {
        let ids: Vec<(String, JobState)> = {
            let conn = self.conn();
            let mut stmt = conn
                .prepare(
                    "SELECT id, state FROM jobs WHERE state = 'running'
                     OR (state = 'paused' AND pause_reason LIKE 'hold:%') ORDER BY rowid",
                )
                .map_err(db_err)?;
            let rows = stmt
                .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
                .map_err(db_err)?;
            let raw: Vec<(String, String)> = rows.collect::<Result<_, _>>().map_err(db_err)?;
            raw.into_iter()
                .filter_map(|(i, s)| JobState::parse(&s).map(|s| (i, s)))
                .collect()
        };
        let mut out = Vec::new();
        for (id, state) in ids {
            let dec = state == JobState::Running;
            if let Some(j) = self.requeue(&id, 0, None, dec, now_ms)? {
                out.push(j);
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn store() -> JobStore {
        JobStore::open_in_memory().unwrap()
    }

    #[test]
    fn cycle_self_and_unknown_dependencies_are_rejected() {
        let s = store();
        let a = s.insert(&NewJob::new("export", json!({})), 1).unwrap();
        let b = s
            .insert(
                &NewJob::new("upload", json!({})).depends_on(a.id.clone()),
                2,
            )
            .unwrap();
        let c = s
            .insert(
                &NewJob::new("publish", json!({})).depends_on(b.id.clone()),
                3,
            )
            .unwrap();
        // a -> c would close a -> c -> b -> a.
        let e = s.add_dependency(&a.id, &c.id).unwrap_err();
        assert_eq!(e.code(), ErrorCode::InvalidInput);
        assert!(e.to_string().contains("cycle"));
        assert_eq!(
            s.add_dependency(&a.id, &a.id).unwrap_err().code(),
            ErrorCode::InvalidInput
        );
        assert_eq!(
            s.insert(&NewJob::new("x", json!({})).depends_on("nope"), 4)
                .unwrap_err()
                .code(),
            ErrorCode::NotFound
        );
        assert_eq!(
            s.insert(&NewJob::new("x", json!({})).with_id("s").depends_on("s"), 4)
                .unwrap_err()
                .code(),
            ErrorCode::InvalidInput
        );
        assert!(s.get(&a.id).unwrap().unwrap().depends_on.is_empty());
    }

    #[test]
    fn eligible_waits_for_dependencies_and_backoff() {
        let s = store();
        let a = s.insert(&NewJob::new("export", json!({})), 1).unwrap();
        let b = s
            .insert(
                &NewJob::new("upload", json!({})).depends_on(a.id.clone()),
                2,
            )
            .unwrap();
        let ids = |v: Vec<Job>| v.into_iter().map(|j| j.id).collect::<Vec<_>>();
        assert_eq!(ids(s.eligible(10).unwrap()), vec![a.id.clone()]);
        s.try_start(&a.id, 10).unwrap().unwrap();
        s.finish_done(&a.id, &json!({"path": "x.mp4"}), 11).unwrap();
        assert_eq!(ids(s.eligible(12).unwrap()), vec![b.id.clone()]);
        s.try_start(&b.id, 12).unwrap().unwrap();
        s.requeue(
            &b.id,
            5_000,
            Some((ErrorCode::Network, "timeout")),
            false,
            13,
        )
        .unwrap();
        assert!(s.eligible(4_999).unwrap().is_empty());
        assert_eq!(ids(s.eligible(5_000).unwrap()), vec![b.id]);
    }

    #[test]
    fn checkpoint_survives_reopen_but_is_not_serialized() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("jobs.db");
        let id = {
            let s = JobStore::open(&path).unwrap();
            let j = s
                .insert(&NewJob::new("upload", json!({"a": 1})), 1)
                .unwrap();
            s.try_start(&j.id, 2).unwrap().unwrap();
            assert!(s
                .save_checkpoint(&j.id, &json!({"session": "https://u", "chunk": 3}), 3)
                .unwrap());
            j.id
        };
        let s = JobStore::open(&path).unwrap();
        let j = s.get(&id).unwrap().unwrap();
        assert_eq!(j.inputs, json!({"a": 1}));
        assert_eq!(
            j.checkpoint,
            Some(json!({"session": "https://u", "chunk": 3}))
        );
        assert!(!serde_json::to_string(&j).unwrap().contains("session"));
    }
}
