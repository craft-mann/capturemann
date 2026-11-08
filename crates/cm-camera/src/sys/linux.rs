//! The Linux camera backend: Video4Linux2 device nodes. Listing reads `/dev/video*` (and sysfs and
//! the udev symlinks for names, kinds and stable ids), the source streams memory-mapped buffers
//! from one node on its own thread, and Motion JPEG is decoded here so that the rest of the
//! pipeline sees `RawFrame`s like on Windows. There is no permission prompt on Linux: a desktop
//! login lets the user open the video nodes (see `v4l_logic::permission_summary`).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use cm_core::{CmError, HostClock, SystemHostClock};

use super::v4l2::{Device, Negotiated, Next, Sizes};
use crate::convert::{pick_mode, preview_due, PixelFormat, RawFrame, RawMode, Subtype};
use crate::permission::CameraPermission;
use crate::types::{
    CameraEvent, CameraFrame, CameraInfo, CameraSource, FrameSink, OpenRequest, SourceInfo,
    SourceRole,
};
use crate::v4l_logic::{
    capture_stamp_ns, classify_linux, decode_mjpeg, is_capture_node, node_index, node_mode,
    permission_summary, stable_id, stepwise_sizes, subtype_of, NodeMode,
};
use crate::NativeSample;

/// Time `open` allows for finding the camera and starting it, on top of the wait for the first
/// frame.
const OPEN_GRACE: Duration = Duration::from_secs(5);
const BUFFERS: u32 = 4;

/// One camera node as found.
#[derive(Debug, Clone)]
struct Node {
    info: CameraInfo,
    path: PathBuf,
    modes: Vec<NodeMode>,
}

/// What a scan of the video nodes found.
#[derive(Debug, Clone, Default)]
struct Scan {
    nodes: Vec<Node>,
    /// Video device nodes that exist, and how many could not be opened for lack of permission.
    existing: usize,
    opened: usize,
    denied: usize,
}

fn video_nodes() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir("/dev")
        .map(|d| {
            d.filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| node_index(p).is_some())
                .collect()
        })
        .unwrap_or_default();
    v.sort_by_key(|p| node_index(p));
    v
}

type Signature = Vec<(PathBuf, i64, i64)>;

/// Changes when a camera is plugged in or removed (node names and their creation times).
fn signature(nodes: &[PathBuf]) -> Signature {
    use std::os::unix::fs::MetadataExt;
    nodes
        .iter()
        .map(|p| {
            let (s, n) = std::fs::metadata(p).map_or((0, 0), |m| (m.ctime(), m.ctime_nsec()));
            (p.clone(), s, n)
        })
        .collect()
}

fn read_trimmed(p: &Path) -> Option<String> {
    std::fs::read_to_string(p).ok().map(|s| s.trim().to_owned())
}

/// The udev symlinks (`/dev/v4l/by-id/...`, `by-path`) that point at `node`.
fn udev_links(node: &Path) -> Vec<String> {
    let mut out = Vec::new();
    for dir in ["/dev/v4l/by-id", "/dev/v4l/by-path"] {
        let Ok(rd) = std::fs::read_dir(dir) else {
            continue;
        };
        for e in rd.filter_map(Result::ok) {
            if std::fs::canonicalize(e.path()).is_ok_and(|c| c == node) {
                out.push(e.path().to_string_lossy().into_owned());
            }
        }
    }
    out.sort();
    out
}

/// `(sysfs path of the node's device, its USB `removable` attribute)`.
fn sysfs_facts(node: &Path) -> (String, Option<String>) {
    let name = node.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let dev = std::fs::canonicalize(format!("/sys/class/video4linux/{name}/device"))
        .or_else(|_| std::fs::canonicalize(format!("/sys/class/video4linux/{name}")))
        .unwrap_or_default();
    let removable = dev
        .ancestors()
        .take(3)
        .find_map(|d| read_trimmed(&d.join("removable")));
    (dev.to_string_lossy().into_owned(), removable)
}

fn modes_of(dev: &Device) -> Vec<NodeMode> {
    let mut out = Vec::new();
    for fourcc in dev.formats() {
        if subtype_of(fourcc).is_none() {
            continue;
        }
        let sizes = match dev.sizes(fourcc) {
            Sizes::Discrete(v) => v,
            Sizes::Range { min, max, step } => stepwise_sizes(min, max, step),
        };
        for (w, h) in sizes {
            if let Some(m) = node_mode(fourcc, w, h, &dev.fps_list(fourcc, w, h)) {
                out.push(m);
            }
        }
    }
    out
}

fn scan_now() -> Scan {
    let mut scan = Scan::default();
    for path in video_nodes() {
        scan.existing += 1;
        let dev = match Device::open(&path) {
            Ok(d) => d,
            Err(e) => {
                if e.kind() == std::io::ErrorKind::PermissionDenied {
                    scan.denied += 1;
                }
                continue;
            }
        };
        scan.opened += 1;
        let Ok(caps) = dev.caps() else { continue };
        if !is_capture_node(caps.capabilities, caps.device_caps) {
            continue;
        }
        let modes = modes_of(&dev);
        if modes.is_empty() {
            continue;
        }
        let raw: Vec<RawMode> = modes.iter().map(|m| m.mode).collect();
        let folded = crate::convert::fold_modes(&raw);
        let top = folded.first().copied();
        let (sys_path, removable) = sysfs_facts(&path);
        let id = stable_id(&path, &udev_links(&path));
        scan.nodes.push(Node {
            info: CameraInfo {
                id,
                name: caps.card,
                kind: classify_linux(&caps.bus_info, &sys_path, removable.as_deref()),
                is_default: scan.nodes.is_empty(),
                max_size: top.map_or([0, 0], |m| [m.width, m.height]),
                max_fps: folded.iter().map(|m| m.max_fps).fold(0.0, f64::max),
                modes: folded,
                in_use_elsewhere: false,
            },
            path,
            modes,
        });
    }
    scan
}

/// The scan, redone only when the set of video nodes changed (listing opens every node, and the
/// device watcher asks every two seconds).
fn scan() -> Scan {
    static CACHE: Mutex<Option<(Signature, Scan)>> = Mutex::new(None);
    let sig = signature(&video_nodes());
    let mut g = CACHE.lock().unwrap_or_else(|p| p.into_inner());
    if let Some((s, scan)) = g.as_ref() {
        // A scan that could not open a node is retried (permissions may have changed).
        if *s == sig && scan.denied == 0 {
            return scan.clone();
        }
    }
    let fresh = scan_now();
    *g = Some((sig, fresh.clone()));
    fresh
}

pub fn list_cameras() -> Result<Vec<CameraInfo>, CmError> {
    let s = scan();
    if s.nodes.is_empty() && s.existing > 0 && s.opened == 0 && s.denied > 0 {
        return Err(permission_error());
    }
    Ok(s.nodes.into_iter().map(|n| n.info).collect())
}

fn permission_error() -> CmError {
    CameraPermission::Denied
        .require()
        .err()
        .unwrap_or_else(|| CmError::internal("camera permission"))
}

/// Whether the user may open the camera nodes. Never prompts.
pub fn permission_status() -> CameraPermission {
    let s = scan();
    permission_summary(s.existing, s.opened, s.denied)
}

/// Linux shows no prompt: the answer is the current state.
pub fn request_access(_timeout: Duration) -> CameraPermission {
    permission_status()
}

// ---- the source ----------------------------------------------------------------------------
//
// A Video4Linux2 node streams to one program at a time (a second `STREAMON` answers `EBUSY`), but
// the Record sheet's bubble and the recorder both want the camera. So the device is opened once per
// process by a worker thread, and every source attached to that node (the recorder, a preview)
// receives the frames: the recorder's settings win, and a preview that finds the camera running
// just joins it.

/// One receiver attached to a running camera.
struct Entry {
    id: u64,
    role: SourceRole,
    sink: Arc<dyn FrameSink>,
    last_preview: Option<u64>,
    /// Answered once, with the first delivered frame or the reason there is none.
    ready: Option<mpsc::Sender<Result<SourceInfo, CmError>>>,
}

/// A worker streaming one node.
struct Hub {
    entries: Arc<Mutex<Vec<Entry>>>,
    stop: Arc<AtomicBool>,
    alive: Arc<AtomicBool>,
    /// Streaming at a recorder's settings (else a preview's).
    record: bool,
    info: Arc<Mutex<Option<SourceInfo>>>,
    join: Option<JoinHandle<()>>,
}

impl Hub {
    fn stop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

static HUBS: Mutex<Vec<(PathBuf, Hub)>> = Mutex::new(Vec::new());
static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

/// What `open` waits on.
type Ready = mpsc::Receiver<Result<SourceInfo, CmError>>;

#[derive(Default)]
pub struct V4lSource {
    /// The node and the entry id this source attached.
    attached: Option<(PathBuf, u64)>,
}

impl V4lSource {
    /// Attaches `sink` to the node's worker, starting or restarting it as the request needs.
    fn attach(&mut self, req: &OpenRequest, sink: Arc<dyn FrameSink>) -> Result<Ready, CmError> {
        let path = resolve(req)?;
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = mpsc::channel();
        let record = req.role == SourceRole::Record;
        let mut entry = Entry {
            id,
            role: req.role.clone(),
            sink,
            last_preview: None,
            ready: Some(tx),
        };
        let mut hubs = lock(&HUBS);
        let mut carried: Vec<Entry> = Vec::new();
        if let Some(i) = hubs.iter().position(|(p, _)| *p == path) {
            let live = hubs[i].1.alive.load(Ordering::SeqCst);
            if live && (hubs[i].1.record || !record) {
                // Join the running camera.
                if let Some(info) = lock(&hubs[i].1.info).clone() {
                    if let Some(t) = entry.ready.take() {
                        let _ = t.send(Ok(info));
                    }
                }
                lock(&hubs[i].1.entries).push(entry);
                self.attached = Some((path, id));
                return Ok(rx);
            }
            // A recorder needs the camera at its settings, or the worker is dead: start a new one
            // and keep the receivers that were attached.
            let (_, mut old) = hubs.remove(i);
            old.stop();
            carried = std::mem::take(&mut *lock(&old.entries));
        }
        let mut all = carried;
        all.push(entry);
        let entries = Arc::new(Mutex::new(all));
        let stop = Arc::new(AtomicBool::new(false));
        let alive = Arc::new(AtomicBool::new(true));
        let info = Arc::new(Mutex::new(None));
        let join = {
            let (req, entries, stop, alive, info) = (
                req.clone(),
                entries.clone(),
                stop.clone(),
                alive.clone(),
                info.clone(),
            );
            std::thread::Builder::new()
                .name("cm-camera-v4l".into())
                .spawn(move || {
                    run(&req, &entries, &stop, &info);
                    alive.store(false, Ordering::SeqCst);
                })
                .map_err(|e| CmError::internal(format!("camera thread: {e}")))?
        };
        hubs.push((
            path.clone(),
            Hub {
                entries,
                stop,
                alive,
                record,
                info,
                join: Some(join),
            },
        ));
        self.attached = Some((path, id));
        Ok(rx)
    }
}

/// The device node a request names (or the first camera).
fn resolve(req: &OpenRequest) -> Result<PathBuf, CmError> {
    let s = scan();
    match &req.device_id {
        Some(id) => s
            .nodes
            .iter()
            .find(|n| &n.info.id == id || n.path.to_str() == Some(id))
            .map(|n| n.path.clone())
            .ok_or_else(|| CmError::not_found(format!("Camera {id} not found."))),
        None => match s.nodes.first() {
            Some(n) => Ok(n.path.clone()),
            None if s.existing > 0 && s.opened == 0 && s.denied > 0 => Err(permission_error()),
            None => Err(CmError::not_found("No camera found.")),
        },
    }
}

impl CameraSource for V4lSource {
    fn open(&mut self, req: &OpenRequest, sink: Arc<dyn FrameSink>) -> Result<SourceInfo, CmError> {
        self.close();
        let rx = self.attach(req, sink)?;
        match rx.recv_timeout(req.first_frame_timeout + OPEN_GRACE) {
            Ok(Ok(info)) => Ok(info),
            Ok(Err(e)) => {
                self.close();
                Err(e)
            }
            Err(_) => {
                self.close();
                Err(CmError::device_busy(
                    "The camera did not deliver a picture. It may be in use by another application.",
                ))
            }
        }
    }

    fn close(&mut self) {
        let Some((path, id)) = self.attached.take() else {
            return;
        };
        let last = {
            let mut hubs = lock(&HUBS);
            let Some(i) = hubs.iter().position(|(p, _)| *p == path) else {
                return;
            };
            let empty = {
                let mut e = lock(&hubs[i].1.entries);
                e.retain(|x| x.id != id);
                e.is_empty()
            };
            empty.then(|| hubs.remove(i).1)
        };
        if let Some(mut hub) = last {
            hub.stop();
        }
    }
}

impl Drop for V4lSource {
    fn drop(&mut self) {
        self.close();
    }
}

fn io_error(e: &std::io::Error, what: &str) -> CmError {
    match e.raw_os_error() {
        Some(libc::EBUSY) => CmError::device_busy(format!(
            "{what} is in use by another application. Close it and try again."
        )),
        Some(libc::EACCES | libc::EPERM) => permission_error(),
        Some(libc::ENODEV | libc::ENOENT | libc::ENXIO) => {
            CmError::not_found(format!("{what} is not connected."))
        }
        _ => CmError::internal(format!("{what}: {e}")),
    }
}

/// An opened camera, ready to stream.
struct Started {
    stream: super::v4l2::Stream,
    info: SourceInfo,
    got: Negotiated,
}

fn start(req: &OpenRequest) -> Result<Started, CmError> {
    let s = scan();
    let node = match &req.device_id {
        Some(id) => s
            .nodes
            .iter()
            .find(|n| &n.info.id == id || n.path.to_str() == Some(id))
            .ok_or_else(|| CmError::not_found(format!("Camera {id} not found.")))?,
        None => match s.nodes.first() {
            Some(n) => n,
            None if s.existing > 0 && s.opened == 0 && s.denied > 0 => {
                return Err(permission_error())
            }
            None => return Err(CmError::not_found("No camera found.")),
        },
    };
    let what = node.info.name.clone();
    let raw: Vec<RawMode> = node.modes.iter().map(|m| m.mode).collect();
    let idx = pick_mode(&raw, req.resolution.height(), req.fps)
        .ok_or_else(|| CmError::invalid_input(format!("{what} offers no video mode.")))?;
    let mode = node.modes[idx];
    let dev = Device::open(&node.path).map_err(|e| io_error(&e, &what))?;
    let got = dev
        .set_format(mode.fourcc, mode.mode.width, mode.mode.height)
        .map_err(|e| io_error(&e, &what))?;
    if subtype_of(got.fourcc).is_none() {
        return Err(CmError::invalid_input(format!(
            "{what} gives no picture layout CaptureMann can use."
        )));
    }
    // Some drivers keep their own rate; the answer is only informational.
    let _ = dev.set_fps(req.fps);
    let stream = dev.start(BUFFERS).map_err(|e| io_error(&e, &what))?;
    Ok(Started {
        stream,
        info: SourceInfo {
            device_id: node.info.id.clone(),
            device_name: node.info.name.clone(),
        },
        got,
    })
}

/// One dequeued buffer as a frame, or `None` for a short or damaged picture.
fn frame_of(data: &[u8], got: &Negotiated) -> Option<RawFrame> {
    let (w, h) = (got.width, got.height);
    match subtype_of(got.fourcc)? {
        Subtype::Mjpg => decode_mjpeg(data),
        sub => {
            let (format, min_stride) = match sub {
                Subtype::Nv12 => (PixelFormat::Nv12, w as usize),
                _ => (PixelFormat::Yuy2, w.div_ceil(2) as usize * 4),
            };
            let stride = (got.bytes_per_line as usize).max(min_stride);
            if data.len() < RawFrame::needed_len(format, stride, h) {
                return None;
            }
            Some(RawFrame {
                format,
                width: w,
                height: h,
                stride,
                data: Arc::new(data.to_vec()),
            })
        }
    }
}

fn run(
    req: &OpenRequest,
    entries: &Mutex<Vec<Entry>>,
    stop: &AtomicBool,
    info_out: &Mutex<Option<SourceInfo>>,
) {
    let fail = |e: CmError| {
        let mut g = lock(entries);
        for en in g.iter_mut() {
            if let Some(t) = en.ready.take() {
                let _ = t.send(Err(e.clone()));
            }
        }
    };
    let Started {
        mut stream,
        info,
        got,
    } = match start(req) {
        Ok(x) => x,
        Err(e) => {
            fail(e);
            return;
        }
    };
    let broadcast = |event: CameraEvent| {
        let sinks: Vec<_> = lock(entries).iter().map(|e| e.sink.clone()).collect();
        for s in sinks {
            s.on_event(event.clone());
        }
    };
    let clock = SystemHostClock;
    let mut announced = false;
    let began = Instant::now();
    while !stop.load(Ordering::SeqCst) {
        let d = match stream.next(200) {
            Ok(Next::Frame(d)) => d,
            Ok(Next::Idle) => {
                if !announced && began.elapsed() > req.first_frame_timeout + OPEN_GRACE {
                    fail(CmError::device_busy(
                        "The camera did not deliver a picture. It may be in use by another application.",
                    ));
                    return;
                }
                continue;
            }
            Ok(Next::Gone) => {
                if announced {
                    broadcast(CameraEvent::Disconnected);
                } else {
                    fail(CmError::not_found(format!(
                        "{} stopped before its first picture.",
                        info.device_name
                    )));
                }
                return;
            }
            Err(e) => {
                if announced {
                    broadcast(CameraEvent::Error(e.to_string()));
                } else {
                    fail(io_error(&e, &info.device_name));
                }
                return;
            }
        };
        let delivered = clock.now_ns();
        // Who gets this picture: every recorder, and a preview when its rate says so.
        let wanted: Vec<Arc<dyn FrameSink>> = {
            let mut g = lock(entries);
            g.iter_mut()
                .filter_map(|e| {
                    let want = match e.role {
                        SourceRole::Record => true,
                        SourceRole::Preview { fps } => {
                            let due = preview_due(e.last_preview, delivered, fps);
                            if due {
                                e.last_preview = Some(delivered);
                            }
                            due
                        }
                    };
                    want.then(|| e.sink.clone())
                })
                .collect()
        };
        let raw = if !wanted.is_empty() && !d.error {
            frame_of(stream.data(&d), &got)
        } else {
            None
        };
        if stream.requeue(&d).is_err() {
            broadcast(CameraEvent::Disconnected);
            return;
        }
        let Some(raw) = raw else { continue };
        let frame = CameraFrame {
            capture_host_ns: capture_stamp_ns(
                d.timestamp_sec,
                d.timestamp_usec,
                d.flags,
                delivered,
            ),
            delivered_host_ns: delivered,
            width: raw.width,
            height: raw.height,
            native: Some(NativeSample(raw)),
        };
        for s in &wanted {
            s.on_frame(&frame);
        }
        if !announced {
            announced = true;
            *lock(info_out) = Some(info.clone());
        }
        // Receivers waiting for their first picture, including ones that joined meanwhile.
        for en in lock(entries).iter_mut() {
            if let Some(t) = en.ready.take() {
                let _ = t.send(Ok(info.clone()));
            }
        }
    }
}

pub fn system_source() -> Box<dyn CameraSource> {
    // A recording survives the camera being unplugged and plugged in again: the wrapper reopens
    // the same device with back-off (crate::reconnect, tested with fakes).
    Box::new(crate::reconnect::ReconnectingSource::new(
        Box::new(V4lSource::default()),
        crate::reconnect::Backoff::default(),
    ))
}

pub fn system_writer_factory() -> Arc<dyn crate::writer::WriterFactory> {
    Arc::new(super::linux_writer::LinuxWriterFactory)
}
