//! ScreenCaptureKit source (macOS): `SCStream` delivers IOSurface-backed `CMSampleBuffer`s on a
//! dispatch queue; each goes to the encoder as a GPU surface reference without mapping the pixels
//! (specs/01-technical-design.md "Recording pipeline"; spike S6). Plain objc2 bindings.
//!
//! Decisions from S6 that live here:
//!
//! * `minimumFrameInterval` is 1 / (1.1 x fps): asking SCK for exactly the display rate skips
//!   vsyncs (docs/project/spikes.md S6); the pipeline coalesces the few extra frames back to constant rate.
//! * NV12 (`420v`) capture, queue depth 8.
//! * Own recorder windows are hidden with `SCContentFilter` exclusion (by pid and by window id);
//!   their owners also set `sharingType = none` (use both, S6). The library window is the
//!   exception: its id is an `exceptingWindows` entry, so it is captured although the process is
//!   excluded (REC-6.AC8); the watcher also rebuilds the filter when that window opens or closes.
//! * Excluded apps (REC-6) are resolved against the apps running when the stream starts, and a
//!   watcher thread re-checks every second ([`WATCH_INTERVAL`]): an excluded app launched later
//!   (a password manager opened mid-recording) makes it rebuild the filter and call
//!   `updateContentFilter` (R3-3). The logic is in [`super::exclude`] and tested without SCK. A
//!   *window* target has no exclusions: it captures that one window only.
//!
//! All `unsafe` in cm-capture's recording path is in this file. Each block says why it is sound;
//! the common reasons: objc2 marks Objective-C calls `unsafe` because it cannot check selectors
//! and argument types, and every call here uses the documented signature on live objects.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use block2::RcBlock;
use cm_core::{CmError, Permission};
use cm_encode::MacFrame;
use dispatch2::{DispatchQueue, DispatchRetained};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{define_class, msg_send, sel, AnyThread, DefinedClass};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_core_graphics::CGPreflightScreenCaptureAccess;
use objc2_core_media::{CMSampleBuffer, CMTime, CMTimeFlags};
use objc2_foundation::{NSArray, NSError, NSObject, NSObjectProtocol};
use objc2_screen_capture_kit::{
    SCContentFilter, SCDisplay, SCRunningApplication, SCShareableContent, SCStream,
    SCStreamConfiguration, SCStreamDelegate, SCStreamOutput, SCStreamOutputType, SCWindow,
};

use super::exclude::{
    filter_spec, matching_pids, AppRef, ExclusionWatch, FilterSpec, WindowRef, WATCH_INTERVAL,
};
use super::plan::CapturePlan;
use super::source::{EventSink, ScreenSource, SourceEvent, StopReason};
use crate::CaptureTarget;

const FOURCC_420V: u32 = 0x3432_3076; // kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange
const STEP_TIMEOUT: Duration = Duration::from_secs(20);

/// Wrapper to move Objective-C handles into a `Send` struct.
struct SendBox<T>(T);
// SAFETY: the wrapped SCStream / delegate / queue objects are only touched from the thread that
// owns the `SckSource` (start/stop are `&mut self`); ScreenCaptureKit itself is thread-safe.
unsafe impl<T> Send for SendBox<T> {}

impl<T> SendBox<T> {
    fn get(&self) -> &T {
        &self.0
    }
}

fn host_ns_of(t: CMTime) -> u64 {
    if t.timescale <= 0 {
        return 0;
    }
    let ns = i128::from(t.value) * 1_000_000_000 / i128::from(t.timescale);
    u64::try_from(ns).unwrap_or(0)
}

fn cm(value: i64, timescale: i32) -> CMTime {
    CMTime {
        value,
        timescale,
        flags: CMTimeFlags::Valid,
        epoch: 0,
    }
}

struct OutputIvars {
    sink: Mutex<Option<EventSink<MacFrame>>>,
    closed: AtomicBool,
}

define_class!(
    /// Receives frames and the stop notification from `SCStream`.
    // SAFETY: NSObject has no subclassing requirements; the only ivar is a plain Rust struct.
    #[unsafe(super(NSObject))]
    #[name = "CmCaptureScreenOutput"]
    #[ivars = OutputIvars]
    struct ScreenOutput;

    // SAFETY: `ScreenOutput` is a direct NSObject subclass, so it satisfies NSObjectProtocol.
    unsafe impl NSObjectProtocol for ScreenOutput {}

    // SAFETY: the method below has exactly the types of the `stream:didOutputSampleBuffer:ofType:`
    // selector; SCK calls it with a valid stream and sample buffer.
    unsafe impl SCStreamOutput for ScreenOutput {
        #[unsafe(method(stream:didOutputSampleBuffer:ofType:))]
        unsafe fn stream_did_output(
            &self,
            _stream: &SCStream,
            sb: &CMSampleBuffer,
            ty: SCStreamOutputType,
        ) {
            // A panic must not unwind into ScreenCaptureKit (abort/UB): catch it, drop the frame.
            cm_core::guard::guard_unit("cm-capture stream_did_output", || {
                if ty != SCStreamOutputType::Screen || self.ivars().closed.load(Ordering::Relaxed) {
                    return;
                }
                // SAFETY: reading the stamp and image buffer of a sample buffer that is alive for
                // the duration of this callback.
                let (host_ns, has_image) = unsafe {
                    (
                        host_ns_of(sb.presentation_time_stamp()),
                        sb.image_buffer().is_some(),
                    )
                };
                let ev = if has_image {
                    SourceEvent::Frame {
                        host_ns,
                        frame: MacFrame::retain(sb),
                    }
                } else {
                    // Idle / blank / suspended status samples carry no pixels.
                    SourceEvent::Idle { host_ns }
                };
                if let Ok(mut s) = self.ivars().sink.lock() {
                    if let Some(sink) = s.as_mut() {
                        sink(ev);
                    }
                }
            });
        }
    }

    // SAFETY: the method below matches the types of the `stream:didStopWithError:` selector; it
    // only reads the NSError SCK passes (valid for the call).
    unsafe impl SCStreamDelegate for ScreenOutput {
        #[unsafe(method(stream:didStopWithError:))]
        unsafe fn stream_did_stop(&self, _stream: &SCStream, error: &NSError) {
            cm_core::guard::guard_unit("cm-capture stream_did_stop", || {
                if self.ivars().closed.load(Ordering::Relaxed) {
                    return;
                }
                let msg = error.localizedDescription().to_string();
                // SCStreamError: -3813 no window list, -3814 no display list, -3815 no capture
                // source, -3816 removing stream. These mean the window or display went away.
                let reason = match error.code() {
                    -3816..=-3813 => StopReason::TargetClosed(msg),
                    // SCStreamErrorUserDeclined: "user declined TCCs", the grant was removed in
                    // System Settings while the stream ran.
                    -3801 => StopReason::PermissionRevoked(msg),
                    _ => StopReason::Error(msg),
                };
                if let Ok(mut s) = self.ivars().sink.lock() {
                    if let Some(sink) = s.as_mut() {
                        sink(SourceEvent::Stopped(reason));
                    }
                }
            });
        }
    }
);

impl ScreenOutput {
    fn new(sink: EventSink<MacFrame>) -> Retained<Self> {
        let this = Self::alloc().set_ivars(OutputIvars {
            sink: Mutex::new(Some(sink)),
            closed: AtomicBool::new(false),
        });
        // SAFETY: `init` on a freshly allocated NSObject subclass with its ivars set.
        unsafe { msg_send![super(this), init] }
    }

    fn close(&self) {
        self.ivars().closed.store(true, Ordering::SeqCst);
        if let Ok(mut s) = self.ivars().sink.lock() {
            *s = None;
        }
    }
}

struct Live {
    /// Tells the exclusion watcher to end (set before the stream stops).
    watch_stop: Arc<AtomicBool>,
    stream: Retained<SCStream>,
    output: Retained<ScreenOutput>,
    _queue: DispatchRetained<DispatchQueue>,
}

/// The ScreenCaptureKit screen source.
#[derive(Default)]
pub struct SckSource {
    live: Option<SendBox<Live>>,
}

impl SckSource {
    pub fn new() -> Self {
        Self::default()
    }
}

fn shareable_content() -> Result<Retained<SCShareableContent>, CmError> {
    let (tx, rx) = mpsc::channel::<Result<SendBox<Retained<SCShareableContent>>, String>>();
    let block = RcBlock::new(move |c: *mut SCShareableContent, e: *mut NSError| {
        // SAFETY: SCK passes either a valid content object or a valid error; `retain` takes our
        // own reference, `as_ref` reads the error for the message.
        let r = match unsafe { Retained::retain(c) } {
            Some(c) => Ok(SendBox(c)),
            // SAFETY: with no content, SCK passes a valid NSError or null; `as_ref` handles null.
            None => Err(unsafe { e.as_ref() }.map_or_else(
                || "unknown error".to_owned(),
                |e| e.localizedDescription().to_string(),
            )),
        };
        let _ = tx.send(r);
    });
    // SAFETY: the completion handler block is copied by SCK and outlives the call.
    unsafe { SCShareableContent::getShareableContentWithCompletionHandler(&block) };
    rx.recv_timeout(STEP_TIMEOUT)
        .map_err(|_| CmError::internal("ScreenCaptureKit did not answer in time"))?
        .map(|b| b.0)
        .map_err(|m| CmError::internal(format!("ScreenCaptureKit: {m}")))
}

fn app_refs(content: &SCShareableContent) -> Vec<AppRef> {
    // SAFETY: plain getters on live objects.
    unsafe {
        content
            .applications()
            .iter()
            .map(|a| AppRef {
                pid: u32::try_from(a.processID()).unwrap_or(0),
                bundle_id: a.bundleIdentifier().to_string(),
            })
            .collect()
    }
}

fn window_refs(content: &SCShareableContent) -> Vec<WindowRef> {
    // SAFETY: plain getters on live objects.
    unsafe {
        content
            .windows()
            .iter()
            .map(|w| WindowRef {
                id: w.windowID(),
                owner_pid: w
                    .owningApplication()
                    .and_then(|o| u32::try_from(o.processID()).ok()),
            })
            .collect()
    }
}

/// A filter with the excluded pids and the kept window ids it covers.
type BuiltFilter = (Retained<SCContentFilter>, Vec<u32>, Vec<u32>);

/// The filter for `plan` from what is running now, and the excluded pids it covers (what the
/// watcher compares against).
fn build_filter(plan: &CapturePlan, content: &SCShareableContent) -> Result<BuiltFilter, CmError> {
    // SAFETY: plain getters on live ScreenCaptureKit objects, and initializers called with the
    // documented argument types.
    unsafe {
        match plan.target {
            CaptureTarget::Window { window_id } => {
                let win = content
                    .windows()
                    .iter()
                    .find(|w| w.windowID() == window_id)
                    .ok_or_else(|| CmError::not_found(format!("Window {window_id} not found")))?;
                Ok((
                    SCContentFilter::initWithDesktopIndependentWindow(
                        SCContentFilter::alloc(),
                        &win,
                    ),
                    Vec::new(),
                    Vec::new(),
                ))
            }
            CaptureTarget::Display { display_id } | CaptureTarget::Region { display_id, .. } => {
                let display = display_by_id(content, display_id)?;
                let ex = &plan.exclusions;
                let apps = app_refs(content);
                let covered = matching_pids(&apps, ex);
                match filter_spec(ex, &apps, &window_refs(content)) {
                    FilterSpec::Windows(ids) => {
                        let windows: Vec<Retained<SCWindow>> = content
                            .windows()
                            .iter()
                            .filter(|w| ids.contains(&w.windowID()))
                            .collect();
                        let arr = NSArray::from_retained_slice(&windows);
                        Ok((
                            SCContentFilter::initWithDisplay_excludingWindows(
                                SCContentFilter::alloc(),
                                &display,
                                &arr,
                            ),
                            covered,
                            Vec::new(),
                        ))
                    }
                    FilterSpec::Applications { pids, keep_windows } => {
                        // Application exclusion covers windows opened later (REC-6.AC2).
                        let running: Vec<Retained<SCRunningApplication>> = content
                            .applications()
                            .iter()
                            .filter(|a| {
                                u32::try_from(a.processID()).is_ok_and(|p| pids.contains(&p))
                            })
                            .collect();
                        let arr = NSArray::from_retained_slice(&running);
                        // Apple: "Any windows that are an exception to the filter will be shown
                        // if their owning application is in the provided list" (SCStream.h):
                        // these are the windows to capture although their app is excluded.
                        let kept: Vec<Retained<SCWindow>> = content
                            .windows()
                            .iter()
                            .filter(|w| keep_windows.contains(&w.windowID()))
                            .collect();
                        Ok((
                            SCContentFilter::initWithDisplay_excludingApplications_exceptingWindows(
                                SCContentFilter::alloc(),
                                &display,
                                &arr,
                                &NSArray::from_retained_slice(&kept),
                            ),
                            covered,
                            keep_windows,
                        ))
                    }
                }
            }
        }
    }
}

/// Watches for excluded apps that start after the stream did (R3-3) and updates the filter.
fn spawn_exclusion_watch(
    stream: Retained<SCStream>,
    plan: CapturePlan,
    applied: Vec<u32>,
    applied_keep: Vec<u32>,
    stop: Arc<AtomicBool>,
) {
    let stream = SendBox(stream);
    let _ = std::thread::Builder::new()
        .name("cm-sck-exclusions".into())
        .spawn(move || {
            let mut watch = ExclusionWatch::new(applied).with_keep(applied_keep);
            loop {
                let mut waited = Duration::ZERO;
                while waited < WATCH_INTERVAL {
                    if stop.load(Ordering::SeqCst) {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                    waited += Duration::from_millis(100);
                }
                let Ok(content) = shareable_content() else {
                    continue;
                };
                if stop.load(Ordering::SeqCst) {
                    return;
                }
                if watch
                    .needs_filter_update(
                        &app_refs(&content),
                        &window_refs(&content),
                        &plan.exclusions,
                    )
                    .is_none()
                {
                    continue;
                }
                let Ok((filter, covered, kept)) = build_filter(&plan, &content) else {
                    continue;
                };
                let (tx, rx) = mpsc::channel::<Option<String>>();
                let block = RcBlock::new(move |e: *mut NSError| {
                    // SAFETY: SCK passes null or a valid error for the duration of the callback.
                    let msg = unsafe { e.as_ref() }.map(|e| e.localizedDescription().to_string());
                    let _ = tx.send(msg);
                });
                // SAFETY: `stream` is a live SCStream (thread-safe); the block is copied by SCK.
                unsafe {
                    stream
                        .get()
                        .updateContentFilter_completionHandler(&filter, Some(&block));
                }
                if matches!(rx.recv_timeout(STEP_TIMEOUT), Ok(None)) {
                    watch.applied_with_keep(covered, kept);
                }
            }
        });
}

fn display_by_id(content: &SCShareableContent, id: u32) -> Result<Retained<SCDisplay>, CmError> {
    // SAFETY: plain getters on live objects.
    unsafe {
        content
            .displays()
            .iter()
            .find(|d| d.displayID() == id)
            .ok_or_else(|| CmError::not_found(format!("Display {id} not found")))
    }
}

fn configure(plan: &CapturePlan) -> Retained<SCStreamConfiguration> {
    // SAFETY: setters on a configuration object this function owns.
    unsafe {
        let cfg = SCStreamConfiguration::new();
        cfg.setWidth(plan.video.width as usize);
        cfg.setHeight(plan.video.height as usize);
        // 1.1 x fps: exactly the display rate skips vsyncs (S6).
        let rate = (f64::from(plan.video.fps) * 1.1).round() as i32;
        cfg.setMinimumFrameInterval(cm(1, rate));
        cfg.setPixelFormat(FOURCC_420V);
        cfg.setShowsCursor(plan.show_cursor);
        cfg.setQueueDepth(8);
        match plan.target {
            CaptureTarget::Region { .. } => {
                if let Some(c) = plan.crop {
                    // sourceRect is in points relative to the display's top-left corner; the
                    // plan's crop is physical px (screenshot convention), so divide by the scale.
                    let s = plan.scale.get();
                    cfg.setSourceRect(CGRect {
                        origin: CGPoint {
                            x: f64::from(c.origin.x) / s,
                            y: f64::from(c.origin.y) / s,
                        },
                        size: CGSize {
                            width: f64::from(c.size.width) / s,
                            height: f64::from(c.size.height) / s,
                        },
                    });
                }
            }
            CaptureTarget::Window { .. } => {
                // REC-1: a resized window is scaled to fit (letterboxed), never cropped.
                cfg.setScalesToFit(true);
                if cfg.respondsToSelector(sel!(setPreservesAspectRatio:)) {
                    cfg.setPreservesAspectRatio(true);
                }
            }
            CaptureTarget::Display { .. } => {}
        }
        cfg
    }
}

fn wait<T>(rx: &mpsc::Receiver<T>, what: &str) -> Result<T, CmError> {
    rx.recv_timeout(STEP_TIMEOUT)
        .map_err(|_| CmError::internal(format!("{what} timed out")))
}

impl ScreenSource for SckSource {
    type Frame = MacFrame;

    fn check_permission(&self) -> Result<(), CmError> {
        if CGPreflightScreenCaptureAccess() {
            Ok(())
        } else {
            Err(CmError::permission_missing(
                Permission::Screen,
                "Screen Recording permission is not granted. Enable it in System Settings > \
                 Privacy & Security > Screen & System Audio Recording, then restart CaptureMann.",
            ))
        }
    }

    fn start(&mut self, plan: &CapturePlan, sink: EventSink<MacFrame>) -> Result<(), CmError> {
        if self.live.is_some() {
            return Err(CmError::busy("A recording is already running"));
        }
        let content = shareable_content()?;
        let (filter, covered, kept) = build_filter(plan, &content)?;
        let cfg = configure(plan);
        let output = ScreenOutput::new(sink);
        let queue = DispatchQueue::new("dev.capturemann.screen", None);
        // SAFETY: the stream is created with a live filter and configuration; the output object
        // doubles as the delegate and is kept alive in `Live` until after `stopCapture`.
        let stream = unsafe {
            let delegate: &ProtocolObject<dyn SCStreamDelegate> =
                ProtocolObject::from_ref(&*output);
            let stream = SCStream::initWithFilter_configuration_delegate(
                SCStream::alloc(),
                &filter,
                &cfg,
                Some(delegate),
            );
            stream
                .addStreamOutput_type_sampleHandlerQueue_error(
                    ProtocolObject::from_ref(&*output),
                    SCStreamOutputType::Screen,
                    Some(&queue),
                )
                .map_err(|e| {
                    CmError::internal(format!("addStreamOutput: {}", e.localizedDescription()))
                })?;
            stream
        };
        let (tx, rx) = mpsc::channel::<Option<String>>();
        let block = RcBlock::new(move |e: *mut NSError| {
            // SAFETY: SCK passes null or a valid error for the duration of the callback.
            let msg = unsafe { e.as_ref() }.map(|e| e.localizedDescription().to_string());
            let _ = tx.send(msg);
        });
        // SAFETY: handler block is copied by SCK.
        unsafe { stream.startCaptureWithCompletionHandler(Some(&block)) };
        match wait(&rx, "startCapture")? {
            None => {
                let watch_stop = Arc::new(AtomicBool::new(false));
                let dynamic = !matches!(plan.target, CaptureTarget::Window { .. })
                    && (!plan.exclusions.bundle_ids.is_empty()
                        || plan.exclusions.keep_provider.is_some()
                        || !plan.exclusions.keep_window_ids.is_empty());
                if dynamic {
                    spawn_exclusion_watch(
                        stream.clone(),
                        plan.clone(),
                        covered,
                        kept,
                        Arc::clone(&watch_stop),
                    );
                }
                self.live = Some(SendBox(Live {
                    watch_stop,
                    stream,
                    output,
                    _queue: queue,
                }));
                Ok(())
            }
            Some(e) => {
                output.close();
                // The usual cause on a fresh install is the missing permission, which
                // preflight reported as granted only for a stale grant; keep the message.
                Err(CmError::internal(format!("Could not start capture: {e}")))
            }
        }
    }

    fn stop(&mut self) {
        let Some(SendBox(live)) = self.live.take() else {
            return;
        };
        live.watch_stop.store(true, Ordering::SeqCst);
        let (tx, rx) = mpsc::channel::<()>();
        let block = RcBlock::new(move |_e: *mut NSError| {
            let _ = tx.send(());
        });
        // SAFETY: handler block is copied by SCK; `stream` is live.
        unsafe { live.stream.stopCaptureWithCompletionHandler(Some(&block)) };
        let _ = rx.recv_timeout(Duration::from_secs(10));
        // After this no more events reach the sink, even if a callback was still queued.
        live.output.close();
    }
}
