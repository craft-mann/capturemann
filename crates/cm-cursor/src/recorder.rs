//! [`CursorRecorder`]: the cursor `TrackRecorder`.

use std::fs::{self, File};
use std::io::BufWriter;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use cm_core::{
    CmError, MediaTime, PauseGap, TrackContext, TrackInfo, TrackKind, TrackRecorder, TrackStats,
};
use serde_json::json;

use crate::image::ImageStore;
use crate::pipeline::{Counters, Msg, Pipeline};
use crate::schema::{CursorHeader, CursorStatus, CURSORS_DIR, CURSOR_FILE};
use crate::source::{CursorSource, RawSender};
use crate::target::CursorTarget;

/// How to record.
#[derive(Debug, Clone)]
pub struct CursorConfig {
    /// The recorded area; see [`CursorTarget`] for the coordinate convention.
    pub target: CursorTarget,
    /// macOS: show the system Input Monitoring prompt when access is missing. The app normally
    /// does this during onboarding, so the default is `false` (recording never pops a dialog).
    pub request_permission: bool,
}

impl CursorConfig {
    pub fn new(target: CursorTarget) -> Self {
        CursorConfig {
            target,
            request_permission: false,
        }
    }
}

struct WriterResult {
    io_error: Option<String>,
    images: usize,
    gaps: Vec<PauseGap>,
}

struct Running {
    tx: RawSender,
    join: JoinHandle<WriterResult>,
    started: MediaTime,
    status: CursorStatus,
}

enum State {
    Idle,
    Running(Box<Running>),
    Stopped(Box<TrackInfo>),
}

/// Records `cursor.jsonl` and `cursors/` into the bundle directory.
pub struct CursorRecorder {
    config: CursorConfig,
    source: Box<dyn CursorSource>,
    counters: Arc<Counters>,
    state: State,
}

impl std::fmt::Debug for CursorRecorder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CursorRecorder")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl CursorRecorder {
    /// A recorder on the platform's real source (event tap / hook).
    pub fn new(config: CursorConfig) -> Self {
        let source = crate::platform::default_source(&config);
        Self::with_source(config, source)
    }

    /// A recorder on an injected source (tests, [`crate::FakeCursorSource`]).
    pub fn with_source(config: CursorConfig, source: Box<dyn CursorSource>) -> Self {
        CursorRecorder {
            config,
            source,
            counters: Arc::new(Counters::default()),
            state: State::Idle,
        }
    }

    /// What this session can record; `Some` after a successful `start`. A degraded status
    /// (`permission_missing(input_monitoring)`) is a HUD warning, not a recording failure.
    pub fn status(&self) -> Option<CursorStatus> {
        match &self.state {
            State::Running(r) => Some(r.status.clone()),
            _ => None,
        }
    }

    fn running(&self) -> Result<&Running, CmError> {
        match &self.state {
            State::Running(r) => Ok(r),
            _ => Err(CmError::invalid_input("cursor recorder is not running")),
        }
    }
}

pub(crate) fn io_err(e: &std::io::Error, path: &std::path::Path) -> CmError {
    if e.kind() == std::io::ErrorKind::StorageFull {
        CmError::disk_full(format!("{}: {e}", path.display()))
    } else {
        CmError::internal(format!("{}: {e}", path.display()))
    }
}

/// The writer thread: owns the file, flushes at least once a second.
fn writer_loop(
    rx: std::sync::mpsc::Receiver<Msg>,
    mut p: Pipeline<BufWriter<File>>,
) -> WriterResult {
    let mut last_flush = Instant::now();
    let mut last_sync = Instant::now();
    loop {
        let timeout = if p.has_pending() {
            Duration::from_millis(8)
        } else {
            Duration::from_millis(200)
        };
        match rx.recv_timeout(timeout) {
            Ok(Msg::Stop(at)) => {
                p.handle(Msg::Stop(at));
                break;
            }
            Ok(m) => p.handle(m),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => p.on_idle(),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                p.on_idle();
                break;
            }
        }
        if last_flush.elapsed() >= Duration::from_millis(800) {
            p.flush();
            last_flush = Instant::now();
            if last_sync.elapsed() >= Duration::from_secs(2) {
                p.sync();
                last_sync = Instant::now();
            }
        }
    }
    p.flush();
    p.sync();
    WriterResult {
        io_error: p.io_error.clone(),
        images: p.image_count(),
        gaps: p.gaps().to_vec(),
    }
}

impl TrackRecorder for CursorRecorder {
    fn kind(&self) -> TrackKind {
        TrackKind::Cursor
    }

    fn start(&mut self, ctx: TrackContext) -> Result<(), CmError> {
        if !matches!(self.state, State::Idle) {
            return Err(CmError::invalid_input("cursor recorder already started"));
        }
        let dir: PathBuf = ctx.dir;
        let cursors = dir.join(CURSORS_DIR);
        fs::create_dir_all(&cursors).map_err(|e| io_err(&e, &cursors))?;
        let path = dir.join(CURSOR_FILE);
        let file = File::create(&path).map_err(|e| io_err(&e, &path))?;

        let (tx, rx) = RawSender::channel();
        let status = match self.source.start(tx.clone()) {
            Ok(s) => s,
            Err(e) => {
                // Leave nothing behind: an empty cursor.jsonl would look like a recorded track.
                drop(file);
                let _ = fs::remove_file(&path);
                let _ = fs::remove_dir_all(&cursors);
                return Err(e);
            }
        };
        let header = CursorHeader::new(
            self.config.target,
            &status,
            crate::platform::CLOCK_NAME,
            crate::platform::PLATFORM,
        );
        let pipeline = Pipeline::new(
            ctx.clock.clone(),
            &header,
            BufWriter::new(file),
            ImageStore::new(cursors),
            self.counters.clone(),
        );
        let join = thread::Builder::new()
            .name("cm-cursor-writer".into())
            .spawn(move || writer_loop(rx, pipeline))
            .map_err(|e| CmError::internal(format!("cursor writer thread: {e}")))?;
        self.state = State::Running(Box::new(Running {
            tx,
            join,
            started: ctx.clock.now().max(MediaTime::ZERO),
            status,
        }));
        Ok(())
    }

    fn pause(&mut self, at: MediaTime) -> Result<(), CmError> {
        self.running()?.tx.control(Msg::Pause(at));
        Ok(())
    }

    fn resume(&mut self, at: MediaTime) -> Result<(), CmError> {
        self.running()?.tx.control(Msg::Resume(at));
        Ok(())
    }

    fn stop(&mut self, at: MediaTime) -> Result<TrackInfo, CmError> {
        match std::mem::replace(&mut self.state, State::Idle) {
            State::Idle => Err(CmError::invalid_input("cursor recorder was not started")),
            State::Stopped(info) => {
                let out = (*info).clone();
                self.state = State::Stopped(info);
                Ok(out)
            }
            State::Running(r) => {
                // After the source is joined every event is already queued ahead of Stop.
                self.source.stop();
                r.tx.control(Msg::Stop(at));
                let res = r.join.join().unwrap_or(WriterResult {
                    io_error: Some("cursor writer thread panicked".into()),
                    images: 0,
                    gaps: Vec::new(),
                });
                let c = &self.counters;
                let ld = |a: &std::sync::atomic::AtomicU64| a.load(Ordering::Relaxed);
                let info = TrackInfo {
                    kind: TrackKind::Cursor,
                    file: CURSOR_FILE.into(),
                    start: r.started,
                    end: at.max(r.started),
                    details: json!({
                        "schema": crate::schema::SCHEMA_VERSION,
                        "tick_rate": cm_core::TICKS_PER_SECOND,
                        "images_dir": CURSORS_DIR,
                        "images": res.images,
                        "clicks": r.status.clicks,
                        "scroll": r.status.scroll,
                        "move_source": r.status.move_source,
                        "permission_missing": r.status.missing_permission(),
                        "warnings": r.status.warnings,
                        "target": self.config.target,
                        "pause_gaps": res.gaps,
                        "coalesced_moves": ld(&c.coalesced),
                        "paused_discarded": ld(&c.paused_discarded),
                        "before_origin": ld(&c.before_origin),
                        "write_error": res.io_error,
                    }),
                    stats: TrackStats {
                        delivered: ld(&c.delivered),
                        written: ld(&c.written),
                        dropped: ld(&c.dropped),
                        late: 0,
                    },
                };
                self.state = State::Stopped(Box::new(info.clone()));
                Ok(info)
            }
        }
    }

    fn stats(&self) -> TrackStats {
        let c = &self.counters;
        let ld = |a: &std::sync::atomic::AtomicU64| a.load(Ordering::Relaxed);
        TrackStats {
            delivered: ld(&c.delivered),
            written: ld(&c.written),
            dropped: ld(&c.dropped),
            late: 0,
        }
    }
}
