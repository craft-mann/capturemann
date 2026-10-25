//! Fakes for CI and for other crates' tests: a scripted camera and a writer that records what it
//! was given and cuts fragments like the real one.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use cm_core::{CmError, MediaTime};

use crate::types::{
    CameraEvent, CameraFrame, CameraSource, FrameSink, OpenRequest, SourceInfo, SourceRole,
};
use crate::writer::{
    AppendOutcome, FragmentInfo, FragmentWriter, WriterFactory, WriterReport, WriterSpec,
    FRAGMENT_SECONDS,
};

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

#[derive(Default)]
struct FakeState {
    sink: Option<Arc<dyn FrameSink>>,
    open_error: Option<CmError>,
    last_request: Option<OpenRequest>,
    closed: bool,
}

/// Drives a [`FakeCameraSource`] from a test.
#[derive(Clone)]
pub struct FakeCameraHandle {
    state: Arc<Mutex<FakeState>>,
    size: (u32, u32),
}

impl FakeCameraHandle {
    /// Delivers one frame captured at `capture_host_ns` that reached the callback `latency_ns`
    /// later. Returns false when nothing is listening.
    pub fn deliver(&self, capture_host_ns: u64, latency_ns: u64) -> bool {
        let sink = lock(&self.state).sink.clone();
        match sink {
            Some(s) => {
                s.on_frame(&CameraFrame {
                    capture_host_ns,
                    delivered_host_ns: capture_host_ns + latency_ns,
                    width: self.size.0,
                    height: self.size.1,
                    native: None,
                });
                true
            }
            None => false,
        }
    }

    pub fn event(&self, e: CameraEvent) {
        let sink = lock(&self.state).sink.clone();
        if let Some(s) = sink {
            s.on_event(e);
        }
    }

    /// The next `open` fails with this error (device_busy, permission_missing, ...).
    pub fn fail_open_with(&self, e: CmError) {
        lock(&self.state).open_error = Some(e);
    }

    pub fn last_request(&self) -> Option<OpenRequest> {
        lock(&self.state).last_request.clone()
    }

    pub fn is_open(&self) -> bool {
        let s = lock(&self.state);
        s.sink.is_some() && !s.closed
    }
}

/// A scripted camera.
pub struct FakeCameraSource {
    state: Arc<Mutex<FakeState>>,
}

impl FakeCameraSource {
    /// A fake delivering frames of `size` (native size reported to the writer).
    pub fn new(size: (u32, u32)) -> (Self, FakeCameraHandle) {
        let state = Arc::new(Mutex::new(FakeState::default()));
        (
            FakeCameraSource {
                state: state.clone(),
            },
            FakeCameraHandle { state, size },
        )
    }
}

impl CameraSource for FakeCameraSource {
    fn open(&mut self, req: &OpenRequest, sink: Arc<dyn FrameSink>) -> Result<SourceInfo, CmError> {
        let mut s = lock(&self.state);
        s.last_request = Some(req.clone());
        if let Some(e) = s.open_error.take() {
            return Err(e);
        }
        s.sink = Some(sink);
        s.closed = false;
        Ok(SourceInfo {
            device_id: req.device_id.clone().unwrap_or_else(|| "fake-cam".into()),
            device_name: if matches!(req.role, SourceRole::Record) {
                "Fake Camera".into()
            } else {
                "Fake Camera (preview)".into()
            },
        })
    }

    fn close(&mut self) {
        let mut s = lock(&self.state);
        s.closed = true;
        s.sink = None;
    }
}

/// What a [`FakeWriterFactory`] saw.
#[derive(Debug, Clone, Default)]
pub struct WriterLog {
    pub spec: Option<WriterSpec>,
    pub path: Option<PathBuf>,
    /// (pts, duration) of every frame written, in order.
    pub appended: Vec<(MediaTime, MediaTime)>,
    pub flushes: u32,
    pub finished: bool,
}

/// A writer factory whose writers log appends and cut a fragment at the first frame at or after
/// each [`FRAGMENT_SECONDS`] boundary (like the keyframe-driven CMAF segmenter).
#[derive(Clone, Default)]
pub struct FakeWriterFactory {
    log: Arc<Mutex<WriterLog>>,
    fail_create: Arc<Mutex<Option<CmError>>>,
    fail_after: Arc<Mutex<Option<usize>>>,
    not_ready_every: Arc<Mutex<Option<usize>>>,
}

impl FakeWriterFactory {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn log(&self) -> WriterLog {
        lock(&self.log).clone()
    }
    pub fn fail_create_with(&self, e: CmError) {
        *lock(&self.fail_create) = Some(e);
    }
    /// After `n` frames, appends report `Failed`.
    pub fn fail_after(&self, n: usize) {
        *lock(&self.fail_after) = Some(n);
    }
    /// Every `n`th append reports `NotReady`.
    pub fn not_ready_every(&self, n: usize) {
        *lock(&self.not_ready_every) = Some(n);
    }
}

impl WriterFactory for FakeWriterFactory {
    fn create(&self, path: &Path, spec: &WriterSpec) -> Result<Box<dyn FragmentWriter>, CmError> {
        if let Some(e) = lock(&self.fail_create).take() {
            return Err(e);
        }
        std::fs::write(path, b"fake camera.mp4")
            .map_err(|e| CmError::internal(format!("fake writer: {e}")))?;
        {
            let mut l = lock(&self.log);
            l.spec = Some(*spec);
            l.path = Some(path.to_path_buf());
        }
        Ok(Box::new(FakeWriter {
            log: self.log.clone(),
            fail_after: *lock(&self.fail_after),
            not_ready_every: *lock(&self.not_ready_every),
            calls: 0,
            frag_start: None,
            frag_end: MediaTime::ZERO,
            frag_bytes: 0,
            fragments: Vec::new(),
        }))
    }
}

struct FakeWriter {
    log: Arc<Mutex<WriterLog>>,
    fail_after: Option<usize>,
    not_ready_every: Option<usize>,
    calls: usize,
    frag_start: Option<MediaTime>,
    frag_end: MediaTime,
    frag_bytes: u64,
    fragments: Vec<FragmentInfo>,
}

impl FakeWriter {
    fn cut(&mut self) {
        if let Some(start) = self.frag_start.take() {
            self.fragments.push(FragmentInfo {
                start,
                duration: self.frag_end - start,
                bytes: self.frag_bytes,
            });
            self.frag_bytes = 0;
        }
    }
}

impl FragmentWriter for FakeWriter {
    fn append(&mut self, _f: &CameraFrame, pts: MediaTime, duration: MediaTime) -> AppendOutcome {
        self.calls += 1;
        if self.fail_after.is_some_and(|n| self.calls > n) {
            return AppendOutcome::Failed("fake writer failure".into());
        }
        if self
            .not_ready_every
            .is_some_and(|n| self.calls.is_multiple_of(n))
        {
            return AppendOutcome::NotReady;
        }
        if let Some(start) = self.frag_start {
            if pts - start >= MediaTime::from_secs(i64::from(FRAGMENT_SECONDS)) {
                self.cut();
            }
        }
        self.frag_start.get_or_insert(pts);
        self.frag_end = pts + duration;
        self.frag_bytes += 1000;
        lock(&self.log).appended.push((pts, duration));
        AppendOutcome::Written
    }

    fn flush(&mut self) {
        self.cut();
        lock(&self.log).flushes += 1;
    }

    fn finish(&mut self) -> Result<WriterReport, CmError> {
        self.cut();
        lock(&self.log).finished = true;
        Ok(WriterReport {
            bytes: self.fragments.iter().map(|f| f.bytes).sum(),
            fragments: self.fragments.clone(),
        })
    }
}
