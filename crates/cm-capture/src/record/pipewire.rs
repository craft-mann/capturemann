//! The OS-independent half of the Linux screen source (portal ScreenCast + PipeWire).
//!
//! * [`PtsMapper`]: PipeWire presentation timestamps (`SPA_META_Header.pts`, `CLOCK_MONOTONIC`
//!   nanoseconds, the same clock as `cm_platform::clock::monotonic_ns` on Linux) to the host
//!   nanoseconds the recorder converts to session time.
//! * [`frame_from_buffer`]: one mapped PipeWire buffer to a [`CpuFrame`] with a host timestamp.
//! * [`TokenStore`]: where the portal's restore token lives. The caller decides (a file under its
//!   config directory, memory in tests); nothing here knows a path.
//! * [`PwBackend`]: what talks to the portal and PipeWire (real: `sys::pipewire_linux`, tests:
//!   [`ScriptedPw`]); [`PipeWireScreenSource`] is the [`ScreenSource`] on top of it.
//!
//! Everything here compiles and is tested on every OS; see `specs/plans/linux-recording-plan.md`.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use cm_core::{CmError, HostClock, Permission, SystemHostClock};
use cm_encode::{CpuFrame, PixelFormat};

use super::plan::CapturePlan;
use super::source::{EventSink, ScreenSource, SourceEvent};

/// A PTS further than this from the arrival time is not trusted.
pub const MAX_PTS_SKEW_NS: u64 = 500_000_000;
/// While the screen is still (no buffers) the source reports "unchanged" this often.
pub const IDLE_INTERVAL: Duration = Duration::from_millis(250);

/// Converts PipeWire timestamps to host nanoseconds.
#[derive(Debug, Default)]
pub struct PtsMapper {
    last: u64,
    /// Frames stamped with the arrival time because the PTS was missing or implausible.
    pub fallbacks: u64,
    /// Frames whose stamp had to be moved forward to keep the order.
    pub clamped: u64,
}

impl PtsMapper {
    /// `pts_ns`: the header meta (`None` or 0 when the compositor sent none), `arrival_ns`: host
    /// time when the buffer reached us. The result never goes backwards.
    pub fn map(&mut self, pts_ns: Option<i64>, arrival_ns: u64) -> u64 {
        let pts = pts_ns
            .and_then(|p| u64::try_from(p).ok())
            .filter(|p| *p > 0);
        let t = match pts {
            Some(p) if p.abs_diff(arrival_ns) <= MAX_PTS_SKEW_NS => p,
            _ => {
                self.fallbacks += 1;
                arrival_ns
            }
        };
        if t <= self.last && self.last != 0 {
            self.clamped += 1;
            self.last += 1;
        } else {
            self.last = t;
        }
        self.last
    }
}

/// One mapped buffer as the PipeWire callback sees it.
#[derive(Debug, Clone, Copy)]
pub struct RawBuffer<'a> {
    pub width: u32,
    pub height: u32,
    pub stride: usize,
    pub format: PixelFormat,
    pub data: &'a [u8],
    pub pts_ns: Option<i64>,
    pub arrival_ns: u64,
}

/// Copies a buffer into a frame (one memcpy; the PipeWire buffer goes back to the stream at once)
/// and stamps it. `Ok(None)` for an empty buffer (the compositor sends one for a cursor-only
/// update).
pub fn frame_from_buffer(
    b: &RawBuffer<'_>,
    mapper: &mut PtsMapper,
) -> Result<Option<(u64, CpuFrame)>, CmError> {
    if b.data.is_empty() {
        return Ok(None);
    }
    let rb = match b.format.bytes_per_pixel() {
        Some(bpp) => b.width as usize * bpp,
        None => b.width as usize,
    };
    // A stride of 0 means "tight".
    let stride = if b.stride == 0 { rb } else { b.stride };
    let rows = match b.format {
        PixelFormat::Nv12 => b.height as usize + (b.height as usize).div_ceil(2),
        _ => b.height as usize,
    };
    let need = (rows.saturating_sub(1)) * stride + rb;
    if b.width == 0 || b.height == 0 || b.data.len() < need {
        return Err(CmError::invalid_input(format!(
            "screen buffer of {} bytes does not hold {}x{} {:?} (stride {stride})",
            b.data.len(),
            b.width,
            b.height,
            b.format
        )));
    }
    let frame = CpuFrame::new(
        b.width,
        b.height,
        stride,
        b.format,
        b.data[..need.min(b.data.len())].to_vec(),
    )?;
    Ok(Some((mapper.map(b.pts_ns, b.arrival_ns), frame)))
}

/// Keeps the portal's restore token between runs.
pub trait TokenStore: Send + 'static {
    fn load(&self) -> Option<String>;
    fn save(&self, token: &str);
}

/// A token kept in memory (tests, or a caller that stores it elsewhere through a clone).
#[derive(Debug, Clone, Default)]
pub struct MemoryTokenStore(pub Arc<Mutex<Option<String>>>);

impl TokenStore for MemoryTokenStore {
    fn load(&self) -> Option<String> {
        self.0.lock().ok().and_then(|g| g.clone())
    }
    fn save(&self, token: &str) {
        if let Ok(mut g) = self.0.lock() {
            *g = Some(token.to_owned());
        }
    }
}

/// A token in a file whose path the caller chose.
#[derive(Debug, Clone)]
pub struct FileTokenStore(pub PathBuf);

impl TokenStore for FileTokenStore {
    fn load(&self) -> Option<String> {
        std::fs::read_to_string(&self.0)
            .ok()
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
    }
    fn save(&self, token: &str) {
        if let Some(dir) = self.0.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(e) = std::fs::write(&self.0, token) {
            log::warn!("cannot store the screen-cast token: {e}");
        }
    }
}

/// What a backend reports once the stream is negotiated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamInfo {
    pub width: u32,
    pub height: u32,
    /// A new restore token, when the portal issued one.
    pub restore_token: Option<String>,
}

/// The portal + PipeWire side.
pub trait PwBackend: Send + 'static {
    /// Opens the session (the portal may show its dialog; `restore_token` makes it silent) and
    /// starts delivering `SourceEvent::Frame` and `Stopped` to `sink`. Returns when streaming.
    fn open(
        &mut self,
        restore_token: Option<&str>,
        show_cursor: bool,
        sink: EventSink<CpuFrame>,
    ) -> Result<StreamInfo, CmError>;

    /// Ends the stream and the portal session; no events reach the sink afterwards.
    fn close(&mut self);
}

/// Shared by the backend's callback and the idle ticker so only one at a time calls the sink.
struct Shared {
    sink: Mutex<Option<EventSink<CpuFrame>>>,
    last_event_ns: AtomicU64,
    stop: AtomicBool,
}

/// A [`ScreenSource`] over a [`PwBackend`] and a [`TokenStore`].
pub struct PipeWireScreenSource<B: PwBackend, T: TokenStore> {
    backend: B,
    tokens: T,
    clock: Arc<dyn HostClock>,
    shared: Option<Arc<Shared>>,
    ticker: Option<JoinHandle<()>>,
    info: Option<StreamInfo>,
}

impl<B: PwBackend, T: TokenStore> PipeWireScreenSource<B, T> {
    pub fn new(backend: B, tokens: T) -> Self {
        Self::with_clock(backend, tokens, Arc::new(SystemHostClock))
    }

    pub fn with_clock(backend: B, tokens: T, clock: Arc<dyn HostClock>) -> Self {
        PipeWireScreenSource {
            backend,
            tokens,
            clock,
            shared: None,
            ticker: None,
            info: None,
        }
    }

    /// The negotiated size after `start` (the plan's size is only a hint on Wayland: the
    /// compositor's dialog picks the monitor).
    pub fn stream_info(&self) -> Option<&StreamInfo> {
        self.info.as_ref()
    }
}

impl<B: PwBackend, T: TokenStore> ScreenSource for PipeWireScreenSource<B, T> {
    type Frame = CpuFrame;

    fn check_permission(&self) -> Result<(), CmError> {
        // There is no query: the portal's dialog is the permission flow and a refusal comes back
        // from `start`.
        Ok(())
    }

    fn start(&mut self, plan: &CapturePlan, sink: EventSink<CpuFrame>) -> Result<(), CmError> {
        if self.shared.is_some() {
            return Err(CmError::internal("the screen source is already running"));
        }
        let shared = Arc::new(Shared {
            sink: Mutex::new(Some(sink)),
            last_event_ns: AtomicU64::new(self.clock.now_ns()),
            stop: AtomicBool::new(false),
        });
        // The backend gets a sink that records "an event happened" for the idle ticker.
        let s2 = Arc::clone(&shared);
        let clock = Arc::clone(&self.clock);
        let wrapped: EventSink<CpuFrame> = Box::new(move |ev| {
            if matches!(ev, SourceEvent::Frame { .. }) {
                s2.last_event_ns.store(clock.now_ns(), Ordering::Relaxed);
            }
            // The source ended: no more "unchanged" samples after the stop notice.
            let ended = matches!(ev, SourceEvent::Stopped(_));
            if let Ok(mut g) = s2.sink.lock() {
                if ended {
                    s2.stop.store(true, Ordering::Relaxed);
                }
                if let Some(sink) = g.as_mut() {
                    sink(ev);
                }
            }
        });
        let token = self.tokens.load();
        match self
            .backend
            .open(token.as_deref(), plan.show_cursor, wrapped)
        {
            Ok(info) => {
                if let Some(t) = &info.restore_token {
                    if token.as_deref() != Some(t.as_str()) {
                        self.tokens.save(t);
                    }
                }
                self.info = Some(info);
            }
            Err(e) => {
                if let Ok(mut g) = shared.sink.lock() {
                    *g = None;
                }
                return Err(e);
            }
        }
        let (s3, clock) = (Arc::clone(&shared), Arc::clone(&self.clock));
        self.ticker = std::thread::Builder::new()
            .name("cm-pw-idle".into())
            .spawn(move || {
                while !s3.stop.load(Ordering::Relaxed) {
                    std::thread::sleep(IDLE_INTERVAL / 5);
                    let now = clock.now_ns();
                    let last = s3.last_event_ns.load(Ordering::Relaxed);
                    if now.saturating_sub(last) >= IDLE_INTERVAL.as_nanos() as u64 {
                        s3.last_event_ns.store(now, Ordering::Relaxed);
                        if let Ok(mut g) = s3.sink.lock() {
                            if s3.stop.load(Ordering::Relaxed) {
                                break;
                            }
                            if let Some(sink) = g.as_mut() {
                                sink(SourceEvent::Idle { host_ns: now });
                            }
                        }
                    }
                }
            })
            .ok();
        self.shared = Some(shared);
        Ok(())
    }

    fn stop(&mut self) {
        if let Some(s) = self.shared.take() {
            s.stop.store(true, Ordering::Relaxed);
            self.backend.close();
            if let Some(t) = self.ticker.take() {
                let _ = t.join();
            }
            // After this no event reaches the sink.
            if let Ok(mut g) = s.sink.lock() {
                *g = None;
            }
        }
    }
}

impl<B: PwBackend, T: TokenStore> Drop for PipeWireScreenSource<B, T> {
    fn drop(&mut self) {
        self.stop();
    }
}

/// The portal declined (the user pressed Cancel in the dialog).
pub fn portal_declined(message: impl Into<String>) -> CmError {
    CmError::permission_missing(Permission::Screen, message)
}

/// A backend for tests: replays prepared events from its own thread.
pub struct ScriptedPw {
    pub info: StreamInfo,
    /// Events delivered right after `open`, with the delay before each.
    pub script: Vec<(Duration, SourceEvent<CpuFrame>)>,
    pub fail_with: Option<CmError>,
    /// The token `open` was called with (shared so a test can read it after the move).
    pub seen_token: Arc<Mutex<Option<Option<String>>>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl ScriptedPw {
    pub fn new(info: StreamInfo, script: Vec<(Duration, SourceEvent<CpuFrame>)>) -> Self {
        ScriptedPw {
            info,
            script,
            fail_with: None,
            seen_token: Arc::default(),
            stop: Arc::default(),
            thread: None,
        }
    }
}

impl PwBackend for ScriptedPw {
    fn open(
        &mut self,
        restore_token: Option<&str>,
        _show_cursor: bool,
        mut sink: EventSink<CpuFrame>,
    ) -> Result<StreamInfo, CmError> {
        if let Ok(mut g) = self.seen_token.lock() {
            *g = Some(restore_token.map(str::to_owned));
        }
        if let Some(e) = self.fail_with.take() {
            return Err(e);
        }
        let script = std::mem::take(&mut self.script);
        let stop = Arc::clone(&self.stop);
        self.thread = Some(std::thread::spawn(move || {
            for (delay, ev) in script {
                std::thread::sleep(delay);
                if stop.load(Ordering::Relaxed) {
                    return;
                }
                sink(ev);
            }
        }));
        Ok(self.info.clone())
    }

    fn close(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::plan::ScreenSettings;
    use crate::record::source::StopReason;
    use cm_core::ErrorCode;

    #[test]
    fn pts_is_used_when_plausible_else_arrival_and_never_goes_back() {
        let mut m = PtsMapper::default();
        assert_eq!(m.map(Some(1_000_000_000), 1_010_000_000), 1_000_000_000);
        // Missing, zero and far-off stamps fall back to the arrival time.
        assert_eq!(m.map(None, 1_020_000_000), 1_020_000_000);
        assert_eq!(m.map(Some(0), 1_030_000_000), 1_030_000_000);
        assert_eq!(m.map(Some(9_000_000_000), 1_040_000_000), 1_040_000_000);
        assert_eq!(m.fallbacks, 3);
        // An older stamp is moved forward by 1 ns, keeping order.
        assert_eq!(m.map(Some(1_039_000_000), 1_041_000_000), 1_040_000_001);
        assert_eq!(m.clamped, 1);
    }

    fn buf(data: &[u8], stride: usize) -> RawBuffer<'_> {
        RawBuffer {
            width: 2,
            height: 2,
            stride,
            format: PixelFormat::Bgrx,
            data,
            pts_ns: Some(5_000_000),
            arrival_ns: 5_100_000,
        }
    }

    #[test]
    fn a_buffer_becomes_a_stamped_frame() {
        let mut m = PtsMapper::default();
        let data = [9u8; 16];
        let (t, f) = frame_from_buffer(&buf(&data, 8), &mut m).unwrap().unwrap();
        assert_eq!(t, 5_000_000);
        assert_eq!((f.width, f.height, f.stride), (2, 2, 8));
        // Stride 0 means tight; a padded stride keeps its padding in the frame.
        assert!(frame_from_buffer(&buf(&data, 0), &mut m).unwrap().is_some());
        let padded = [1u8; 24];
        let (_, f) = frame_from_buffer(&buf(&padded, 16), &mut m)
            .unwrap()
            .unwrap();
        assert_eq!(f.packed().len(), 16);
    }

    #[test]
    fn empty_buffers_are_skipped_and_short_ones_rejected() {
        let mut m = PtsMapper::default();
        assert!(frame_from_buffer(&buf(&[], 8), &mut m).unwrap().is_none());
        let e = frame_from_buffer(&buf(&[0; 15], 8), &mut m).unwrap_err();
        assert_eq!(e.code(), ErrorCode::InvalidInput);
    }

    #[test]
    fn token_stores_round_trip() {
        let m = MemoryTokenStore::default();
        assert_eq!(m.load(), None);
        m.save("abc");
        assert_eq!(m.clone().load().as_deref(), Some("abc"));
        let dir = tempfile::tempdir().unwrap();
        let f = FileTokenStore(dir.path().join("a/b/token"));
        assert_eq!(f.load(), None);
        f.save("xyz");
        assert_eq!(f.load().as_deref(), Some("xyz"));
    }

    fn plan() -> CapturePlan {
        use crate::record::plan::plan_capture;
        use crate::{CaptureTarget, DisplayInfo, Targets};
        use cm_core::{PhysicalPoint, PhysicalRect, PhysicalSize, ScaleFactor};
        let targets = Targets {
            displays: vec![DisplayInfo {
                id: 1,
                name: "d".into(),
                bounds: PhysicalRect {
                    origin: PhysicalPoint { x: 0, y: 0 },
                    size: PhysicalSize {
                        width: 1280,
                        height: 720,
                    },
                },
                scale: ScaleFactor::new(1.0).unwrap(),
                is_primary: true,
                is_builtin: false,
            }],
            windows: vec![],
        };
        plan_capture(
            &ScreenSettings::new(CaptureTarget::Display { display_id: 1 }),
            &targets,
        )
        .unwrap()
    }

    fn frame(id: u8) -> CpuFrame {
        CpuFrame::new(2, 2, 8, PixelFormat::Bgrx, vec![id; 16]).unwrap()
    }

    #[test]
    fn start_uses_and_saves_the_token_delivers_frames_idles_and_stops() {
        let tokens = MemoryTokenStore::default();
        tokens.save("old");
        let script = vec![
            (
                Duration::from_millis(10),
                SourceEvent::Frame {
                    host_ns: 1,
                    frame: frame(1),
                },
            ),
            // A still screen: nothing for a while, then the stream ends.
            (
                Duration::from_millis(900),
                SourceEvent::Stopped(StopReason::TargetClosed("monitor".into())),
            ),
        ];
        let backend = ScriptedPw::new(
            StreamInfo {
                width: 1280,
                height: 720,
                restore_token: Some("new".into()),
            },
            script,
        );
        let seen = Arc::clone(&backend.seen_token);
        let mut src = PipeWireScreenSource::new(backend, tokens.clone());
        let log: Arc<Mutex<Vec<String>>> = Arc::default();
        let l2 = Arc::clone(&log);
        src.start(
            &plan(),
            Box::new(move |ev| {
                let s = match ev {
                    SourceEvent::Frame { host_ns, .. } => format!("frame {host_ns}"),
                    SourceEvent::Idle { .. } => "idle".to_string(),
                    SourceEvent::Stopped(_) => "stopped".to_string(),
                };
                l2.lock().unwrap().push(s);
            }),
        )
        .unwrap();
        assert_eq!(seen.lock().unwrap().clone(), Some(Some("old".to_string())));
        assert_eq!(tokens.load().as_deref(), Some("new"));
        assert_eq!(src.stream_info().unwrap().width, 1280);
        std::thread::sleep(Duration::from_millis(1100));
        src.stop();
        let n = log.lock().unwrap().len();
        let got = log.lock().unwrap().clone();
        assert_eq!(got.first().map(String::as_str), Some("frame 1"));
        assert!(got.iter().any(|s| s == "idle"), "{got:?}");
        assert_eq!(got.last().map(String::as_str), Some("stopped"));
        std::thread::sleep(Duration::from_millis(400));
        assert_eq!(log.lock().unwrap().len(), n, "no events after stop");
    }

    #[test]
    fn a_declined_portal_fails_the_start_and_keeps_the_old_token() {
        let tokens = MemoryTokenStore::default();
        tokens.save("keep");
        let mut backend = ScriptedPw::new(
            StreamInfo {
                width: 1,
                height: 1,
                restore_token: None,
            },
            vec![],
        );
        backend.fail_with = Some(portal_declined("The screen share was cancelled."));
        let mut src = PipeWireScreenSource::new(backend, tokens.clone());
        let e = src.start(&plan(), Box::new(|_| {})).unwrap_err();
        assert_eq!(e.code(), ErrorCode::PermissionMissing);
        assert_eq!(tokens.load().as_deref(), Some("keep"));
        // And the source can be started again later.
        src.stop();
    }
}
