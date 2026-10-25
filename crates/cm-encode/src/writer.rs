//! The platform-neutral half of the fragmented MP4 writer.
//!
//! A platform encoder (AVAssetWriter on macOS, a Media Foundation sink later) hands this writer
//! the init segment (`ftyp` + `moov`) once and then one segment (`moof` + `mdat`) per ~2 s. Bytes
//! go to the file in arrival order on a dedicated thread, which also `fsync`s after every segment,
//! so the encoder's callback queue is never blocked by the disk and a crash loses at most the
//! fragment in flight (NFR-6, REC-8). The writer parses each segment as it lands and keeps a log
//! (start, duration, frames, arrival time) that the recorder reports and the tests check.

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use cm_core::{CmError, MediaTime};

use crate::fmp4;

/// One fragment as written.
#[derive(Debug, Clone, PartialEq)]
pub struct FragmentRecord {
    pub index: u32,
    /// Media time of the first sample (120 kHz ticks).
    pub start: MediaTime,
    pub duration: MediaTime,
    pub samples: u32,
    pub bytes: u64,
    /// Milliseconds from writer creation to the moment the segment reached the writer.
    pub arrival_ms: u64,
}

/// What the writer reports when it is closed.
#[derive(Debug, Clone, PartialEq)]
pub struct WriterSummary {
    pub path: PathBuf,
    pub bytes: u64,
    pub init_bytes: u64,
    pub fragments: Vec<FragmentRecord>,
    /// Set when the disk failed (full, I/O error, too slow) after at least one fragment was
    /// written: what is in the file is kept and valid, and the take can still be finalized
    /// (R3-5). `None` for a clean close.
    pub halt: Option<WriterHalt>,
}

/// Why a writer stopped writing before the end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriterHalt {
    /// `disk_full` (ENOSPC), `disk_slow` (the disk could not keep up) or `io_error`.
    pub reason: &'static str,
    pub message: String,
}

/// Segments the writer queues before it pushes back on the encoder (R3-11). One segment is
/// about 2 s, so this is about 20 s of video.
pub const QUEUE_SEGMENTS: usize = 10;
/// How long the queue may stay full before the writer gives up with `disk_slow` (R3-11).
pub const SLOW_DISK_LIMIT: Duration = Duration::from_secs(5);

/// Knobs for tests (a slow or failing disk); the product values are the defaults.
#[derive(Debug, Clone)]
pub struct WriterOptions {
    pub queue_segments: usize,
    pub slow_disk_limit: Duration,
    /// Test hook: every disk write sleeps this long (a slow disk).
    pub write_delay: Duration,
    /// Test hook: from this many bytes on, writes fail with ENOSPC.
    pub fail_after_bytes: Option<u64>,
}

impl Default for WriterOptions {
    fn default() -> Self {
        WriterOptions {
            queue_segments: QUEUE_SEGMENTS,
            slow_disk_limit: SLOW_DISK_LIMIT,
            write_delay: Duration::ZERO,
            fail_after_bytes: None,
        }
    }
}

enum Msg {
    Init(Vec<u8>),
    Segment(Vec<u8>, u64),
    Close,
}

struct Inner {
    path: PathBuf,
    created: Instant,
    slow_limit: Duration,
    /// Segments that found the queue full (the encoder was held back).
    queue_full: std::sync::atomic::AtomicU64,
    tx: Mutex<Option<SyncSender<Msg>>>,
    handle: Mutex<Option<JoinHandle<()>>>,
    failed: AtomicBool,
    error: Mutex<Option<CmError>>,
    log: Mutex<WriterSummary>,
}

/// Writes init + segments to one file. Cheap to clone; all clones feed the same file.
#[derive(Clone)]
pub struct FragmentedMp4Writer(Arc<Inner>);

impl std::fmt::Debug for FragmentedMp4Writer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "FragmentedMp4Writer({})", self.0.path.display())
    }
}

fn io_error(path: &Path, e: &std::io::Error) -> CmError {
    // ENOSPC is 28 on both macOS and Linux; `StorageFull` covers Windows.
    let full = e.kind() == std::io::ErrorKind::StorageFull || e.raw_os_error() == Some(28);
    let msg = format!("cannot write `{}`: {e}", path.display());
    if full {
        CmError::disk_full(msg)
    } else {
        CmError::internal(msg)
    }
}

impl FragmentedMp4Writer {
    /// Creates (truncates) the file and starts the writer thread.
    pub fn create(path: &Path) -> Result<Self, CmError> {
        Self::create_with(path, WriterOptions::default())
    }

    /// [`FragmentedMp4Writer::create`] with explicit queue and fault knobs (tests).
    pub fn create_with(path: &Path, opts: WriterOptions) -> Result<Self, CmError> {
        let mut file = File::create(path).map_err(|e| io_error(path, &e))?;
        let (tx, rx) = sync_channel::<Msg>(opts.queue_segments.max(1));
        let inner = Arc::new(Inner {
            path: path.to_owned(),
            created: Instant::now(),
            slow_limit: opts.slow_disk_limit,
            queue_full: std::sync::atomic::AtomicU64::new(0),
            tx: Mutex::new(Some(tx)),
            handle: Mutex::new(None),
            failed: AtomicBool::new(false),
            error: Mutex::new(None),
            log: Mutex::new(WriterSummary {
                path: path.to_owned(),
                bytes: 0,
                init_bytes: 0,
                fragments: Vec::new(),
                halt: None,
            }),
        });
        let worker = Arc::clone(&inner);
        let handle = std::thread::Builder::new()
            .name("cm-fmp4-writer".into())
            .spawn(move || {
                for msg in rx {
                    let (bytes, arrival, is_init) = match msg {
                        Msg::Init(b) => (b, 0, true),
                        Msg::Segment(b, a) => (b, a, false),
                        Msg::Close => break,
                    };
                    if worker.failed.load(Ordering::SeqCst) {
                        continue; // drain after a failure
                    }
                    if !opts.write_delay.is_zero() {
                        std::thread::sleep(opts.write_delay);
                    }
                    let injected = opts.fail_after_bytes.is_some_and(|limit| {
                        worker.log.lock().map_or(0, |l| l.bytes) + bytes.len() as u64 > limit
                    });
                    let res = if injected {
                        Err(std::io::Error::from_raw_os_error(28))
                    } else {
                        file.write_all(&bytes).and_then(|()| file.sync_data())
                    };
                    if let Err(e) = res {
                        worker.failed.store(true, Ordering::SeqCst);
                        if let Ok(mut slot) = worker.error.lock() {
                            *slot = Some(io_error(&worker.path, &e));
                        }
                        continue;
                    }
                    let Ok(mut log) = worker.log.lock() else {
                        continue;
                    };
                    log.bytes += bytes.len() as u64;
                    if is_init {
                        log.init_bytes = bytes.len() as u64;
                    } else if let Ok(scan) = describe_segment(&bytes) {
                        let index = u32::try_from(log.fragments.len()).unwrap_or(u32::MAX);
                        log.fragments.push(FragmentRecord {
                            index,
                            start: scan.0,
                            duration: scan.1,
                            samples: scan.2,
                            bytes: bytes.len() as u64,
                            arrival_ms: arrival,
                        });
                    }
                }
                let _ = file.sync_all();
            })
            .map_err(|e| CmError::internal(format!("cannot start writer thread: {e}")))?;
        if let Ok(mut h) = inner.handle.lock() {
            *h = Some(handle);
        }
        Ok(FragmentedMp4Writer(inner))
    }

    pub fn path(&self) -> &Path {
        &self.0.path
    }

    /// Queues the init segment. Must be called before any segment.
    pub fn push_init(&self, bytes: Vec<u8>) {
        self.send(Msg::Init(bytes));
    }

    /// Queues one media segment (`moof` + `mdat`, optionally with `styp`).
    pub fn push_segment(&self, bytes: Vec<u8>) {
        let ms = u64::try_from(self.0.created.elapsed().as_millis()).unwrap_or(u64::MAX);
        self.send(Msg::Segment(bytes, ms));
    }

    /// Queues a message. A full queue (the disk is slower than the encoder) holds the caller
    /// back, which backs the encoder up so it reports `NotReady` and frames are counted as late;
    /// memory stays bounded. If the queue stays full for [`WriterOptions::slow_disk_limit`], the
    /// writer fails with `disk_slow` and the recorder stops cleanly (R3-11).
    fn send(&self, msg: Msg) {
        let tx = self.0.tx.lock().ok().and_then(|t| t.clone());
        let Some(tx) = tx else {
            return;
        };
        let mut msg = msg;
        let mut waited_since: Option<Instant> = None;
        loop {
            match tx.try_send(msg) {
                Ok(()) | Err(TrySendError::Disconnected(_)) => return,
                Err(TrySendError::Full(m)) => {
                    if waited_since.is_none() {
                        self.0.queue_full.fetch_add(1, Ordering::Relaxed);
                    }
                    let since = *waited_since.get_or_insert_with(Instant::now);
                    if self.0.failed.load(Ordering::SeqCst) {
                        return; // the writer is draining after a failure: drop
                    }
                    if since.elapsed() >= self.0.slow_limit {
                        self.0.failed.store(true, Ordering::SeqCst);
                        if let Ok(mut slot) = self.0.error.lock() {
                            slot.get_or_insert(CmError::disk_full(format!(
                                "disk_slow: the disk cannot keep up with the recording (`{}`)",
                                self.0.path.display()
                            )));
                        }
                        return;
                    }
                    msg = m;
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
        }
    }

    /// How many segments had to wait because the queue was full.
    pub fn queue_full_count(&self) -> u64 {
        self.0.queue_full.load(Ordering::Relaxed)
    }

    /// The first disk error seen by the writer thread, if any (`disk_full` for ENOSPC).
    pub fn check(&self) -> Result<(), CmError> {
        if self.0.failed.load(Ordering::SeqCst) {
            if let Ok(e) = self.0.error.lock() {
                if let Some(e) = e.as_ref() {
                    return Err(e.clone());
                }
            }
            return Err(CmError::internal("fragment writer failed"));
        }
        Ok(())
    }

    /// Fragments logged so far.
    pub fn fragments_written(&self) -> usize {
        self.0.log.lock().map_or(0, |l| l.fragments.len())
    }

    /// Drains the queue, syncs and closes the file. Idempotent. If the disk failed after at least
    /// one fragment was written, this still succeeds and the summary says why it stopped early
    /// (`halt`): the file is valid up to its last fragment and the take is kept (R3-5). Only a
    /// failure before any fragment is an error.
    pub fn finish(&self) -> Result<WriterSummary, CmError> {
        let tx = self.0.tx.lock().ok().and_then(|mut t| t.take());
        if let Some(tx) = tx {
            // The queue may be full while the disk is slow; the writer thread drains it.
            let mut msg = Msg::Close;
            loop {
                match tx.try_send(msg) {
                    Ok(()) | Err(TrySendError::Disconnected(_)) => break,
                    Err(TrySendError::Full(m)) => {
                        msg = m;
                        std::thread::sleep(Duration::from_millis(5));
                    }
                }
            }
        }
        let handle = self.0.handle.lock().ok().and_then(|mut h| h.take());
        if let Some(h) = handle {
            let _ = h.join();
        }
        let failure = self.check().err();
        let mut summary = self
            .0
            .log
            .lock()
            .map(|l| l.clone())
            .map_err(|_| CmError::internal("fragment log poisoned"))?;
        match failure {
            None => Ok(summary),
            Some(e) if summary.fragments.is_empty() => Err(e),
            Some(e) => {
                let message = e.to_string();
                summary.halt = Some(WriterHalt {
                    reason: if message.contains("disk_slow") {
                        "disk_slow"
                    } else if matches!(e, CmError::DiskFull { .. }) {
                        "disk_full"
                    } else {
                        "io_error"
                    },
                    message,
                });
                Ok(summary)
            }
        }
    }
}

/// `(start, duration, samples)` of the `moof` in a segment, in 120 kHz ticks. All recording files
/// use a 120,000 media timescale (`setMediaTimeScale` on macOS), so track time is session ticks.
fn describe_segment(bytes: &[u8]) -> Result<(MediaTime, MediaTime, u32), CmError> {
    let f = fmp4::parse_segment(bytes)
        .ok_or_else(|| CmError::internal("segment has no complete fragment"))?;
    Ok((
        MediaTime::from_ticks(i64::try_from(f.base_time).unwrap_or(i64::MAX)),
        MediaTime::from_ticks(i64::try_from(f.duration).unwrap_or(i64::MAX)),
        f.samples,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fmp4::synth::*;

    #[test]
    fn writes_init_then_segments_in_order_and_logs_them() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("screen.mp4");
        let w = FragmentedMp4Writer::create(&path).unwrap();
        let s = Spec::default();
        w.push_init(init_segment(&s));
        for i in 0..3u32 {
            w.push_segment(fragment(&s, i + 1, u64::from(i) * 240_000, 60, 4000, 40));
        }
        let sum = w.finish().unwrap();
        assert_eq!(sum.fragments.len(), 3);
        assert_eq!(sum.fragments[1].start.ticks(), 240_000);
        assert_eq!(sum.fragments[1].duration.ticks(), 240_000);
        assert_eq!(sum.fragments[2].samples, 60);
        let scan = fmp4::scan_file(&path).unwrap();
        assert!(scan.is_clean());
        assert!(scan.init.unwrap().moov_first);
        assert_eq!(scan.fragments.len(), 3);
        assert_eq!(sum.bytes, std::fs::metadata(&path).unwrap().len());
        // finish is idempotent
        assert_eq!(w.finish().unwrap().fragments.len(), 3);
    }

    /// R3-5: the disk fills after some fragments: `finish` still succeeds, says why the file
    /// ended early, and what was written is a valid file (the take is kept).
    #[test]
    fn r3_5_a_full_disk_after_some_fragments_finishes_with_a_halt() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("screen.mp4");
        let s = Spec::default();
        let init = init_segment(&s);
        let frag = fragment(&s, 1, 0, 60, 4000, 40);
        // Room for the init and two fragments; the third write hits ENOSPC.
        let limit = (init.len() + 2 * frag.len() + 10) as u64;
        let w = FragmentedMp4Writer::create_with(
            &path,
            WriterOptions {
                fail_after_bytes: Some(limit),
                ..WriterOptions::default()
            },
        )
        .unwrap();
        w.push_init(init);
        for i in 0..5u32 {
            w.push_segment(fragment(&s, i + 1, u64::from(i) * 240_000, 60, 4000, 40));
        }
        let sum = w.finish().expect("kept, not an error");
        assert_eq!(sum.fragments.len(), 2);
        let halt = sum.halt.expect("says why");
        assert_eq!(halt.reason, "disk_full");
        assert!(w.check().is_err(), "the failure is still visible");
        let scan = fmp4::scan_file(&path).unwrap();
        assert!(scan.is_clean(), "the file ends at a fragment boundary");
        assert_eq!(scan.fragments.len(), 2);
    }

    /// A disk that fails before any fragment is an error (there is nothing to keep).
    #[test]
    fn r3_5_a_failure_before_the_first_fragment_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let w = FragmentedMp4Writer::create_with(
            &dir.path().join("screen.mp4"),
            WriterOptions {
                fail_after_bytes: Some(10),
                ..WriterOptions::default()
            },
        )
        .unwrap();
        let s = Spec::default();
        w.push_init(init_segment(&s));
        w.push_segment(fragment(&s, 1, 0, 60, 4000, 40));
        let e = w.finish().unwrap_err();
        assert!(matches!(e, CmError::DiskFull { .. }), "{e:?}");
    }

    /// R3-11: a slow disk holds the encoder back (bounded queue) instead of growing memory
    /// without limit, and gives up with `disk_slow` when the queue stays full too long.
    #[test]
    fn r3_11_a_slow_disk_is_bounded_and_ends_with_disk_slow() {
        let dir = tempfile::tempdir().unwrap();
        let w = FragmentedMp4Writer::create_with(
            &dir.path().join("screen.mp4"),
            WriterOptions {
                queue_segments: 2,
                slow_disk_limit: Duration::from_millis(300),
                write_delay: Duration::from_millis(600),
                fail_after_bytes: None,
            },
        )
        .unwrap();
        let s = Spec::default();
        let t0 = Instant::now();
        w.push_init(init_segment(&s));
        for i in 0..6u32 {
            w.push_segment(fragment(&s, i + 1, u64::from(i) * 240_000, 60, 4000, 40));
        }
        // The pushes were held back (not queued without bound) and gave up after the limit.
        assert!(
            t0.elapsed() >= Duration::from_millis(250),
            "{:?}",
            t0.elapsed()
        );
        assert!(w.queue_full_count() > 0);
        let e = w.check().unwrap_err();
        assert!(e.to_string().contains("disk_slow"), "{e}");
        let _ = w.finish();
    }

    #[test]
    fn creating_in_a_missing_directory_is_a_clean_error() {
        let e = FragmentedMp4Writer::create(Path::new("/nonexistent-dir-cm/x.mp4")).unwrap_err();
        assert!(!e.to_string().is_empty());
    }
}
