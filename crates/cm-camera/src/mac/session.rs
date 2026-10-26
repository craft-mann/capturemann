//! The shared `AVCaptureSession`: exactly one per camera per process, with a record output and a
//! preview output added on demand ("one session, two outputs").

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::{Duration, Instant};

use block2::RcBlock;
use cm_core::CmError;
use dispatch2::{DispatchQueue, DispatchRetained};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{define_class, msg_send, AnyThread, DefinedClass};
use objc2_av_foundation::{
    AVCaptureConnection, AVCaptureDevice, AVCaptureDeviceInput,
    AVCaptureDeviceWasDisconnectedNotification, AVCaptureOutput, AVCaptureSession,
    AVCaptureSessionPreset1280x720, AVCaptureSessionPreset1920x1080, AVCaptureSessionPresetHigh,
    AVCaptureSessionRuntimeErrorNotification, AVCaptureVideoDataOutput,
    AVCaptureVideoDataOutputSampleBufferDelegate,
};
use objc2_core_media::{CMSampleBuffer, CMTime, CMTimeFlags};
use objc2_core_video::{CVPixelBufferGetHeight, CVPixelBufferGetWidth};
use objc2_foundation::{
    NSDictionary, NSNotification, NSNotificationCenter, NSNumber, NSObject, NSObjectProtocol,
    NSString,
};

use super::sample::NativeSample;
use super::{cm_ns, devices, host_ns, SendBox};
use crate::permission::CameraPermission;
use crate::types::{
    CameraEvent, CameraFrame, CameraResolution, CameraSource, FrameSink, OpenRequest, SourceInfo,
    SourceRole,
};

const FOURCC_BGRA: i64 = 0x4247_5241;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    Record,
    Preview,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

struct PreviewTarget {
    sink: Arc<dyn FrameSink>,
    min_gap_ns: u64,
    last_capture_ns: u64,
}

/// Fans frames out to the sinks. Lives as long as the session; shared with the delegates.
#[derive(Default)]
struct Dispatcher {
    record: Mutex<Option<Arc<dyn FrameSink>>>,
    preview: Mutex<Option<PreviewTarget>>,
    record_frames: AtomicU64,
    preview_frames: AtomicU64,
}

impl Dispatcher {
    fn on_sample(&self, role: Role, sb: &CMSampleBuffer) {
        let delivered_host_ns = host_ns();
        // SAFETY: `sb` is valid for the duration of the delegate call.
        let (img, pts) = unsafe { (sb.image_buffer(), sb.presentation_time_stamp()) };
        let Some(img) = img else { return };
        // The capture PTS is on the host clock (CMClockGetHostTimeClock), the same timebase as
        // CLOCK_UPTIME_RAW, so it converts to session time directly.
        let capture_host_ns = u64::try_from(cm_ns(pts)).unwrap_or(0);
        let make = || CameraFrame {
            capture_host_ns,
            delivered_host_ns,
            width: u32::try_from(CVPixelBufferGetWidth(&img)).unwrap_or(0),
            height: u32::try_from(CVPixelBufferGetHeight(&img)).unwrap_or(0),
            native: Some(NativeSample::retain(sb)),
        };
        match role {
            Role::Record => {
                self.record_frames.fetch_add(1, Ordering::Relaxed);
                let sink = lock(&self.record).clone();
                if let Some(s) = sink {
                    s.on_frame(&make());
                }
            }
            Role::Preview => {
                self.preview_frames.fetch_add(1, Ordering::Relaxed);
                let sink = {
                    let mut g = lock(&self.preview);
                    match g.as_mut() {
                        Some(t)
                            if t.last_capture_ns == 0
                                || capture_host_ns.saturating_sub(t.last_capture_ns)
                                    + 2_000_000
                                    >= t.min_gap_ns =>
                        {
                            t.last_capture_ns = capture_host_ns;
                            Some(t.sink.clone())
                        }
                        _ => None,
                    }
                };
                if let Some(s) = sink {
                    s.on_frame(&make());
                }
            }
        }
    }

    fn on_dropped(&self, role: Role) {
        if role == Role::Record {
            let sink = lock(&self.record).clone();
            if let Some(s) = sink {
                s.on_event(CameraEvent::FramesDropped(1));
            }
        }
    }

    fn broadcast(&self, e: &CameraEvent) {
        let rec = lock(&self.record).clone();
        let pre = lock(&self.preview).as_ref().map(|t| t.sink.clone());
        for s in [rec, pre].into_iter().flatten() {
            s.on_event(e.clone());
        }
    }
}

struct DelegateIvars {
    disp: Arc<Dispatcher>,
    role: Role,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements; the class has no Drop and its only ivar
    // is a plain Rust struct.
    #[unsafe(super(NSObject))]
    #[name = "CMCameraFrameDelegate"]
    #[ivars = DelegateIvars]
    struct FrameDelegate;

    // SAFETY: `FrameDelegate` is a direct NSObject subclass, so it satisfies NSObjectProtocol.
    unsafe impl NSObjectProtocol for FrameDelegate {}

    // SAFETY: both methods below have exactly the types of the protocol's selectors; AVFoundation
    // passes live objects that are only used within each call.
    unsafe impl AVCaptureVideoDataOutputSampleBufferDelegate for FrameDelegate {
        #[unsafe(method(captureOutput:didOutputSampleBuffer:fromConnection:))]
        unsafe fn capture_output(
            &self,
            _output: &AVCaptureOutput,
            sb: &CMSampleBuffer,
            _conn: &AVCaptureConnection,
        ) {
            // A panic must not unwind into AVFoundation (abort/UB): catch it, drop the frame.
            cm_core::guard::guard_unit("cm-camera capture_output", || {
                let iv = self.ivars();
                iv.disp.on_sample(iv.role, sb);
            });
        }

        #[unsafe(method(captureOutput:didDropSampleBuffer:fromConnection:))]
        unsafe fn capture_dropped(
            &self,
            _output: &AVCaptureOutput,
            _sb: &CMSampleBuffer,
            _conn: &AVCaptureConnection,
        ) {
            cm_core::guard::guard_unit("cm-camera capture_dropped", || {
                let iv = self.ivars();
                iv.disp.on_dropped(iv.role);
            });
        }
    }
);

impl FrameDelegate {
    fn new(disp: Arc<Dispatcher>, role: Role) -> Retained<Self> {
        let this = Self::alloc().set_ivars(DelegateIvars { disp, role });
        // SAFETY: standard NSObject init on a freshly allocated instance with ivars set.
        unsafe { msg_send![super(this), init] }
    }
}

struct Output {
    out: Retained<AVCaptureVideoDataOutput>,
    // The output holds its delegate weakly; keep it alive here.
    _delegate: Retained<FrameDelegate>,
}

// SAFETY: AVCaptureVideoDataOutput is thread safe per Apple's AVFoundation threading rules, the
// delegate only holds an Arc<Dispatcher>, and `Output` is only touched under `SharedSession::cfg`.
unsafe impl Send for Output {}

#[derive(Default)]
struct Config {
    preset: Option<CameraResolution>,
    record: Option<Output>,
    preview: Option<Output>,
}

/// Which session preset to use for a request, given what the device supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PresetChoice {
    P1080,
    P720,
    High,
}

/// 1080p when asked and supported, else 720p, else the device's best (`High`), so a camera that
/// only has 640x480 or 1280x960 is recorded at its native mode (CAM-2 edge case).
pub(super) fn choose_preset(res: CameraResolution, has_1080: bool, has_720: bool) -> PresetChoice {
    match res {
        CameraResolution::P1080 if has_1080 => PresetChoice::P1080,
        _ if has_720 => PresetChoice::P720,
        _ => PresetChoice::High,
    }
}

pub(super) struct SharedSession {
    session: SendBox<Retained<AVCaptureSession>>,
    device: SendBox<Retained<AVCaptureDevice>>,
    _input: SendBox<Retained<AVCaptureDeviceInput>>,
    queue: SendBox<DispatchRetained<DispatchQueue>>,
    disp: Arc<Dispatcher>,
    cfg: Mutex<Config>,
    observers: Mutex<Vec<Observer>>,
    pub(super) id: String,
    pub(super) name: String,
}

type Observer = SendBox<Retained<ProtocolObject<dyn NSObjectProtocol>>>;
type Registry = Mutex<Vec<(String, Weak<SharedSession>)>>;

fn registry() -> &'static Registry {
    static REG: Registry = Mutex::new(Vec::new());
    &REG
}

/// `AVErrorApplicationIsNotAuthorizedToUseDevice`.
const AV_NOT_AUTHORIZED: isize = -11852;

/// The `NSError` code of an `AVCaptureSessionRuntimeErrorNotification`, if it carries one.
fn runtime_error_code(n: &NSNotification) -> Option<isize> {
    use objc2_foundation::{NSError, NSString};
    // userInfo is an immutable dictionary owned by the notification; the value is downcast with
    // a class check before use.
    let info = n.userInfo()?;
    let key = NSString::from_str("AVCaptureSessionErrorKey");
    let v = info.objectForKey(&key)?;
    v.downcast_ref::<NSError>().map(|e| e.code())
}

fn map_av_error(code: isize, domain: &str, msg: &str) -> CmError {
    // AVErrorDeviceAlreadyUsedByAnotherSession (-11804), AVErrorDeviceInUseByAnotherApplication
    // (-11815): the camera is held exclusively by someone else.
    // AVErrorApplicationIsNotAuthorizedToUseDevice (-11852): permission.
    match code {
        -11804 | -11815 => {
            CmError::device_busy(format!("The camera is in use by another app ({msg})"))
        }
        -11852 => CameraPermission::Denied
            .require()
            .err()
            .unwrap_or_else(|| CmError::internal(msg)),
        _ => CmError::internal(format!("camera error {domain} {code}: {msg}")),
    }
}

impl SharedSession {
    /// The one session for `device_id` (or the default camera) in this process.
    pub(super) fn acquire(device_id: Option<&str>) -> Result<Arc<SharedSession>, CmError> {
        devices::permission_status().require()?;
        let device = Self::find_device(device_id)?;
        // SAFETY: property read on a valid device.
        let id = unsafe { device.uniqueID() }.to_string();
        let mut reg = lock(registry());
        reg.retain(|(_, w)| w.strong_count() > 0);
        if let Some(s) = reg
            .iter()
            .find(|(k, _)| *k == id)
            .and_then(|(_, w)| w.upgrade())
        {
            return Ok(s);
        }
        let s = Arc::new(Self::create(device, id.clone())?);
        reg.push((id, Arc::downgrade(&s)));
        Ok(s)
    }

    fn find_device(device_id: Option<&str>) -> Result<Retained<AVCaptureDevice>, CmError> {
        // SAFETY: class methods returning optional retained devices.
        unsafe {
            match device_id {
                Some(id) => AVCaptureDevice::deviceWithUniqueID(&NSString::from_str(id))
                    .ok_or_else(|| CmError::not_found(format!("Camera {id} not found"))),
                None => devices::video_media_type()
                    .and_then(|mt| AVCaptureDevice::defaultDeviceWithMediaType(mt))
                    .ok_or_else(|| CmError::not_found("No camera found")),
            }
        }
    }

    fn create(device: Retained<AVCaptureDevice>, id: String) -> Result<Self, CmError> {
        // SAFETY: Objective-C calls on valid objects; the session is configured before it runs.
        unsafe {
            let name = device.localizedName().to_string();
            let input =
                AVCaptureDeviceInput::deviceInputWithDevice_error(&device).map_err(|e| {
                    map_av_error(
                        e.code(),
                        &e.domain().to_string(),
                        &e.localizedDescription().to_string(),
                    )
                })?;
            let session = AVCaptureSession::new();
            session.beginConfiguration();
            if !session.canAddInput(&input) {
                session.commitConfiguration();
                return Err(if device.isInUseByAnotherApplication() {
                    CmError::device_busy("The camera is in use by another app")
                } else {
                    CmError::internal("camera input was rejected by the capture session")
                });
            }
            session.addInput(&input);
            session.commitConfiguration();
            let disp = Arc::new(Dispatcher::default());
            let this = SharedSession {
                session: SendBox(session),
                device: SendBox(device),
                _input: SendBox(input),
                queue: SendBox(DispatchQueue::new("dev.capturemann.camera", None)),
                disp,
                cfg: Mutex::new(Config::default()),
                observers: Mutex::new(Vec::new()),
                id,
                name,
            };
            this.observe();
            Ok(this)
        }
    }

    /// Device removal and runtime errors reach every sink as events.
    fn observe(&self) {
        let center = NSNotificationCenter::defaultCenter();
        let disp = Arc::downgrade(&self.disp);
        let my_id = self.id.clone();
        let session_ptr = Retained::as_ptr(&self.session.0) as usize;
        let mut obs = lock(&self.observers);
        let d1 = disp.clone();
        let on_disconnect = RcBlock::new(move |n: std::ptr::NonNull<NSNotification>| {
            // A panic must not unwind into Foundation (abort/UB): catch it, skip the event.
            cm_core::guard::guard_unit("cm-camera on_disconnect", || {
                // SAFETY: the notification is valid for the block call.
                let n = unsafe { n.as_ref() };
                let dev = n
                    .object()
                    .and_then(|o| o.downcast::<AVCaptureDevice>().ok());
                // SAFETY: property read on a valid device.
                let same = dev.is_some_and(|d| unsafe { d.uniqueID() }.to_string() == my_id);
                if same {
                    if let Some(d) = d1.upgrade() {
                        d.broadcast(&CameraEvent::Disconnected);
                    }
                }
            });
        });
        let on_error = RcBlock::new(move |n: std::ptr::NonNull<NSNotification>| {
            cm_core::guard::guard_unit("cm-camera on_error", || {
                // SAFETY: the notification is valid for the block call.
                let n = unsafe { n.as_ref() };
                let obj_ptr = n.object().map(|o| Retained::as_ptr(&o) as usize);
                if obj_ptr == Some(session_ptr) {
                    if let Some(d) = disp.upgrade() {
                        // The NSError sits in userInfo["AVCaptureSessionErrorKey"]; -11852 means the
                        // user withdrew camera access (PRM-1).
                        let code = runtime_error_code(n);
                        let ev = if code == Some(AV_NOT_AUTHORIZED) {
                            CameraEvent::PermissionRevoked
                        } else {
                            CameraEvent::Error(format!(
                                "capture session runtime error{}",
                                code.map(|c| format!(" ({c})")).unwrap_or_default()
                            ))
                        };
                        d.broadcast(&ev);
                    }
                }
            });
        });
        // SAFETY: the notification names are framework constants; the blocks are copied by the
        // center and only touch Send data (a Weak<Dispatcher>, a String, an integer).
        unsafe {
            for (name, block) in [
                (AVCaptureDeviceWasDisconnectedNotification, &on_disconnect),
                (AVCaptureSessionRuntimeErrorNotification, &on_error),
            ] {
                let o = center.addObserverForName_object_queue_usingBlock(
                    Some(name),
                    None,
                    None,
                    block,
                );
                obs.push(SendBox(o));
            }
        }
    }

    fn set_preset(&self, cfg: &mut Config, res: CameraResolution, force: bool) {
        if cfg.preset == Some(res) && !force {
            return;
        }
        // SAFETY: session and device are valid; configuration is bracketed.
        unsafe {
            let s = &self.session.0;
            let can_1080 = s.canSetSessionPreset(AVCaptureSessionPreset1920x1080);
            let can_720 = s.canSetSessionPreset(AVCaptureSessionPreset1280x720);
            let preset = match choose_preset(res, can_1080, can_720) {
                PresetChoice::P1080 => AVCaptureSessionPreset1920x1080,
                PresetChoice::P720 => AVCaptureSessionPreset1280x720,
                PresetChoice::High => AVCaptureSessionPresetHigh,
            };
            s.beginConfiguration();
            s.setSessionPreset(preset);
            s.commitConfiguration();
        }
        cfg.preset = Some(res);
    }

    /// Asks the device for `fps` if its active format supports it. Failure is not an error: the
    /// recorder retimes whatever the camera delivers.
    fn apply_fps(&self, fps: u32) {
        let Ok(fps) = i32::try_from(fps) else { return };
        // SAFETY: the device configuration lock is held around the property writes.
        unsafe {
            let d = &self.device.0;
            let supported = d
                .activeFormat()
                .videoSupportedFrameRateRanges()
                .iter()
                .any(|r| {
                    r.minFrameRate() <= f64::from(fps) + 0.01
                        && f64::from(fps) <= r.maxFrameRate() + 0.01
                });
            if !supported || d.lockForConfiguration().is_err() {
                return;
            }
            let dur = CMTime {
                value: 1,
                timescale: fps,
                flags: CMTimeFlags::Valid,
                epoch: 0,
            };
            d.setActiveVideoMinFrameDuration(dur);
            d.setActiveVideoMaxFrameDuration(dur);
            d.unlockForConfiguration();
        }
    }

    fn add_output(&self, role: Role, bgra: bool) -> Option<Output> {
        // SAFETY: the output is configured before it is added; the delegate is retained by the
        // returned `Output` for as long as the output exists.
        unsafe {
            let out = AVCaptureVideoDataOutput::new();
            out.setAlwaysDiscardsLateVideoFrames(true);
            if bgra {
                let key = NSString::from_str("PixelFormatType"); // kCVPixelBufferPixelFormatTypeKey
                let val = NSNumber::new_i64(FOURCC_BGRA);
                let settings: Retained<NSDictionary<NSString, AnyObject>> =
                    NSDictionary::from_slices(&[&*key], &[&**val]);
                out.setVideoSettings(Some(&settings));
            }
            let delegate = FrameDelegate::new(self.disp.clone(), role);
            out.setSampleBufferDelegate_queue(
                Some(ProtocolObject::from_ref(&*delegate)),
                Some(&self.queue.0),
            );
            let s = &self.session.0;
            s.beginConfiguration();
            let ok = s.canAddOutput(&out);
            if ok {
                s.addOutput(&out);
            }
            s.commitConfiguration();
            ok.then_some(Output {
                out,
                _delegate: delegate,
            })
        }
    }

    fn remove_output(&self, o: Output) {
        // SAFETY: the output was added by `add_output` to this session.
        unsafe {
            o.out.setSampleBufferDelegate_queue(None, None);
            let s = &self.session.0;
            s.beginConfiguration();
            s.removeOutput(&o.out);
            s.commitConfiguration();
        }
    }

    fn frames(&self, role: Role) -> u64 {
        match role {
            Role::Record => self.disp.record_frames.load(Ordering::Relaxed),
            Role::Preview => self.disp.preview_frames.load(Ordering::Relaxed),
        }
    }

    /// Attaches a sink and waits for its first frame.
    pub(super) fn attach(
        &self,
        req: &OpenRequest,
        sink: Arc<dyn FrameSink>,
    ) -> Result<(), CmError> {
        let (role, preview_fps) = match req.role {
            SourceRole::Record => (Role::Record, 0),
            SourceRole::Preview { fps } => (Role::Preview, fps.max(1)),
        };
        let before;
        {
            let mut cfg = lock(&self.cfg);
            // The recorder owns the resolution; a preview only sets it when it is the first user.
            let first_user = cfg.preset.is_none();
            if role == Role::Record || first_user {
                self.set_preset(&mut cfg, req.resolution, false);
            }
            let slot_taken = match role {
                Role::Record => cfg.record.is_some(),
                Role::Preview => cfg.preview.is_some(),
            };
            if slot_taken {
                return Err(CmError::busy(match role {
                    Role::Record => "the camera is already being recorded",
                    Role::Preview => "a preview is already running on this camera",
                }));
            }
            before = self.frames(role);
            match role {
                Role::Record => *lock(&self.disp.record) = Some(sink),
                Role::Preview => {
                    *lock(&self.disp.preview) = Some(PreviewTarget {
                        sink,
                        min_gap_ns: 1_000_000_000 / u64::from(preview_fps),
                        last_capture_ns: 0,
                    })
                }
            }
            let Some(output) = self.add_output(role, role == Role::Preview) else {
                self.clear_sink(role);
                return Err(CmError::internal(
                    "camera output was rejected by the capture session",
                ));
            };
            match role {
                Role::Record => cfg.record = Some(output),
                Role::Preview => cfg.preview = Some(output),
            }
            // SAFETY: state query / start on a configured session (blocks until running).
            unsafe {
                if !self.session.0.isRunning() {
                    self.session.0.startRunning();
                }
            }
            if role == Role::Record {
                self.apply_fps(req.fps);
            }
        }
        let deadline = Instant::now() + req.first_frame_timeout;
        while self.frames(role) == before {
            if Instant::now() >= deadline {
                self.detach_role(role);
                // SAFETY: property read.
                let busy = unsafe { self.device.0.isInUseByAnotherApplication() };
                return Err(if busy {
                    CmError::device_busy(
                        "The camera is in use by another app and delivered no frames",
                    )
                } else {
                    CmError::internal(format!(
                        "camera {} delivered no frames within {} ms",
                        self.name,
                        req.first_frame_timeout.as_millis()
                    ))
                });
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        Ok(())
    }

    fn clear_sink(&self, role: Role) {
        match role {
            Role::Record => *lock(&self.disp.record) = None,
            Role::Preview => *lock(&self.disp.preview) = None,
        }
    }

    pub(super) fn detach(&self, role_is_record: bool) {
        self.detach_role(if role_is_record {
            Role::Record
        } else {
            Role::Preview
        });
    }

    fn detach_role(&self, role: Role) {
        let mut cfg = lock(&self.cfg);
        self.clear_sink(role);
        let removed = match role {
            Role::Record => cfg.record.take(),
            Role::Preview => cfg.preview.take(),
        };
        if let Some(o) = removed {
            self.remove_output(o);
        }
        if cfg.record.is_none() && cfg.preview.is_none() {
            // SAFETY: stopping a running session is always allowed.
            unsafe { self.session.0.stopRunning() };
            cfg.preset = None;
        }
    }
}

impl Drop for SharedSession {
    fn drop(&mut self) {
        let center = NSNotificationCenter::defaultCenter();
        for o in lock(&self.observers).drain(..) {
            // SAFETY: the observer was returned by `addObserverForName...` on the same center.
            unsafe { center.removeObserver(o.0.as_ref()) };
        }
        // SAFETY: stopping an already stopped session is a no-op.
        unsafe { self.session.0.stopRunning() };
    }
}

/// `CameraSource` over the shared session.
#[derive(Default)]
pub(super) struct HubSource {
    held: Option<(Arc<SharedSession>, Role)>,
}

impl CameraSource for HubSource {
    fn open(&mut self, req: &OpenRequest, sink: Arc<dyn FrameSink>) -> Result<SourceInfo, CmError> {
        self.close();
        let s = SharedSession::acquire(req.device_id.as_deref())?;
        s.attach(req, sink)?;
        let info = SourceInfo {
            device_id: s.id.clone(),
            device_name: s.name.clone(),
        };
        let role = match req.role {
            SourceRole::Record => Role::Record,
            SourceRole::Preview { .. } => Role::Preview,
        };
        self.held = Some((s, role));
        Ok(info)
    }

    fn close(&mut self) {
        if let Some((s, role)) = self.held.take() {
            s.detach(role == Role::Record);
        }
    }
}

impl Drop for HubSource {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_choice_prefers_requested_then_falls_back() {
        use CameraResolution::*;
        assert_eq!(choose_preset(P1080, true, true), PresetChoice::P1080);
        assert_eq!(choose_preset(P1080, false, true), PresetChoice::P720);
        assert_eq!(choose_preset(P720, true, true), PresetChoice::P720);
        assert_eq!(choose_preset(P720, false, false), PresetChoice::High);
    }

    #[test]
    fn av_error_codes_map_to_shared_error_codes() {
        use cm_core::ErrorCode;
        assert_eq!(
            map_av_error(-11804, "AVFoundationErrorDomain", "x").code(),
            ErrorCode::DeviceBusy
        );
        assert_eq!(
            map_av_error(-11815, "AVFoundationErrorDomain", "x").code(),
            ErrorCode::DeviceBusy
        );
        assert_eq!(
            map_av_error(-11852, "AVFoundationErrorDomain", "x").code(),
            ErrorCode::PermissionMissing
        );
        assert_eq!(map_av_error(-1, "d", "x").code(), ErrorCode::Internal);
    }
}
