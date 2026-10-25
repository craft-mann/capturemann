//! macOS implementation of S6. Pure objc2 bindings (no Swift bridge, so no libswift* link
//! problems on a Command Line Tools-only machine).
//!
//! Pipeline: SCStream (IOSurface-backed CVPixelBuffers) -> `AVAssetWriterInput.append` with
//! VideoToolbox output settings -> `AVAssetWriter` (MPEG-4, `movieFragmentInterval` = 2 s).
//! The sample buffers go straight from ScreenCaptureKit to the writer: this process never
//! maps the pixels (the one exception is `verify-bubble`, which locks single frames on
//! purpose to look at them). Camera: AVCaptureSession -> AVCaptureVideoDataOutput -> a second
//! AVAssetWriter. Both writers start their session at the same host-clock origin `t0`.

use std::{
    cell::RefCell,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    process::Command,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
    time::{Duration, Instant},
};

use block2::RcBlock;
use dispatch2::{DispatchQueue, DispatchRetained};
use objc2::{
    define_class, msg_send,
    rc::Retained,
    runtime::{AnyObject, ProtocolObject},
    AnyThread, DefinedClass, MainThreadMarker,
};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSColor, NSScreen,
    NSWindow, NSWindowCollectionBehavior, NSWindowSharingType, NSWindowStyleMask,
};
use objc2_av_foundation::{
    AVAssetSegmentReport, AVAssetSegmentType, AVAssetWriter, AVAssetWriterDelegate, AVAssetWriterInput,
    AVFileTypeProfileMPEG4CMAFCompliant, AVAuthorizationStatus, AVCaptureConnection,
    AVCaptureDevice, AVCaptureDeviceInput, AVCaptureOutput, AVCaptureSession,
    AVCaptureVideoDataOutput, AVCaptureVideoDataOutputSampleBufferDelegate,
    AVCaptureVideoPreviewLayer, AVFileTypeMPEG4, AVFileTypeQuickTimeMovie, AVMediaTypeVideo,
    AVCaptureSessionPreset1280x720, AVCaptureSessionPreset1920x1080,
};
use objc2_core_graphics::CGPreflightScreenCaptureAccess;
use objc2_core_foundation::CFRetained;
use objc2_core_media::{CMSampleBuffer, CMSampleTimingInfo, CMTime, CMTimeFlags};
use objc2_core_video::{
    CVPixelBufferGetBaseAddress, CVPixelBufferGetBytesPerRow, CVPixelBufferGetHeight,
    CVPixelBufferGetWidth, CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags,
    CVPixelBufferUnlockBaseAddress,
};
use objc2_foundation::{
    NSArray, NSData, NSDictionary, NSError, NSNumber, NSObject, NSObjectProtocol, NSPoint, NSRect,
    NSSize, NSString, NSURL,
};
use objc2_uniform_type_identifiers::UTTypeMPEG4Movie;
use objc2_quartz_core::{CABasicAnimation, CALayer, CAMediaTiming};
use objc2_screen_capture_kit::{
    SCContentFilter, SCDisplay, SCShareableContent, SCStream, SCStreamConfiguration,
    SCStreamDelegate, SCStreamOutput, SCStreamOutputType, SCWindow,
};

/// MediaTime: 120,000 ticks per second (technical design, "One clock").
const TICKS_PER_SEC: i64 = 120_000;
const FOURCC_420V: u32 = 0x3432_3076; // kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange
const FOURCC_BGRA: u32 = 0x4247_5241; // kCVPixelFormatType_32BGRA

// ---------------------------------------------------------------------------------------
// Small helpers

/// Host clock in ns: the same timebase as `CMClockGetHostTimeClock` and `mach_absolute_time`
/// (both ScreenCaptureKit and AVCapture stamp frames on it).
fn host_ns() -> u64 {
    extern "C" {
        fn clock_gettime_nsec_np(clock_id: libc::c_int) -> u64;
    }
    const CLOCK_UPTIME_RAW: libc::c_int = 8;
    unsafe { clock_gettime_nsec_np(CLOCK_UPTIME_RAW) }
}

fn cm_ns(t: CMTime) -> i64 {
    let (v, s) = (t.value, t.timescale);
    if s == 0 {
        0
    } else {
        (v as i128 * 1_000_000_000 / s as i128) as i64
    }
}

fn cm(value: i64, timescale: i32) -> CMTime {
    CMTime { value, timescale, flags: CMTimeFlags::Valid, epoch: 0 }
}

fn ns_to_ticks(ns: i64) -> i64 {
    (ns as i128 * TICKS_PER_SEC as i128 / 1_000_000_000) as i64
}

struct SendBox<T>(T);
unsafe impl<T> Send for SendBox<T> {}
unsafe impl<T> Sync for SendBox<T> {}

fn cpu_secs() -> f64 {
    unsafe {
        let mut ru: libc::rusage = std::mem::zeroed();
        libc::getrusage(libc::RUSAGE_SELF, &mut ru);
        let tv = |t: libc::timeval| t.tv_sec as f64 + t.tv_usec as f64 * 1e-6;
        tv(ru.ru_utime) + tv(ru.ru_stime)
    }
}

fn max_rss_mb() -> f64 {
    unsafe {
        let mut ru: libc::rusage = std::mem::zeroed();
        libc::getrusage(libc::RUSAGE_SELF, &mut ru);
        ru.ru_maxrss as f64 / 1_048_576.0 // bytes on macOS
    }
}

fn rss_mb_now() -> Option<f64> {
    let out = Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .ok()?;
    String::from_utf8_lossy(&out.stdout).trim().parse::<f64>().ok().map(|k| k / 1024.0)
}

fn ncpu() -> f64 {
    std::thread::available_parallelism().map(|n| n.get() as f64).unwrap_or(1.0)
}

/// Run a closure on the main thread and wait for its result.
fn on_main<R: Send + 'static>(f: impl FnOnce(MainThreadMarker) -> R + Send + 'static) -> R {
    let (tx, rx) = mpsc::channel();
    DispatchQueue::main().exec_async(move || {
        let mtm = MainThreadMarker::new().expect("main thread");
        let _ = tx.send(f(mtm));
    });
    rx.recv().expect("main thread dropped the task")
}

// ---------------------------------------------------------------------------------------
// Options

#[derive(Clone, Debug)]
struct Opts {
    cmd: String,
    codec: String,
    secs: f64,
    fps: u32,
    pixfmt: u32,
    width: Option<usize>,
    height: Option<usize>,
    mbps: Option<f64>,
    camera: Option<bool>,
    camera_720: bool,
    bubble: String,
    exclude: String,
    motion: bool,
    request_camera: bool,
    /// SCK minimumFrameInterval denominator (default = 1.1 x fps; see docs/SPIKES.md)
    sck_rate: Option<i32>,
    container: String,
    out: PathBuf,
    file: Option<String>,
}

fn parse_opts() -> Opts {
    let mut a = std::env::args().skip(1);
    let cmd = a.next().unwrap_or_else(|| "help".into());
    let mut o = Opts {
        cmd,
        codec: "hevc".into(),
        secs: 20.0,
        fps: 60,
        pixfmt: FOURCC_420V,
        width: None,
        height: None,
        mbps: None,
        camera: None,
        camera_720: false,
        bubble: "marker".into(),
        exclude: "both".into(),
        motion: true,
        request_camera: false,
        sck_rate: None,
        container: "cmaf".into(),
        out: PathBuf::from("spike-out/encode"),
        file: None,
    };
    let rest: Vec<String> = a.collect();
    let mut i = 0;
    while i < rest.len() {
        let k = rest[i].as_str();
        let mut val = || {
            i += 1;
            rest.get(i).cloned().unwrap_or_default()
        };
        match k {
            "--codec" => o.codec = val(),
            "--secs" => o.secs = val().parse().unwrap_or(20.0),
            "--fps" => o.fps = val().parse().unwrap_or(60),
            "--pixfmt" => o.pixfmt = if val() == "bgra" { FOURCC_BGRA } else { FOURCC_420V },
            "--width" => o.width = val().parse().ok(),
            "--height" => o.height = val().parse().ok(),
            "--mbps" => o.mbps = val().parse().ok(),
            "--camera" => o.camera = Some(val() != "off"),
            "--camera-res" => o.camera_720 = val() == "720",
            "--bubble" => o.bubble = val(),
            "--exclude" => o.exclude = val(),
            "--motion" => o.motion = val() != "off",
            "--request-camera" => o.request_camera = true,
            "--sck-rate" => o.sck_rate = val().parse().ok(),
            "--container" => o.container = val(),
            "--out" => o.out = PathBuf::from(val()),
            other if !other.starts_with("--") => o.file = Some(other.to_string()),
            other => eprintln!("unknown option {other}"),
        }
        i += 1;
    }
    o
}

// ---------------------------------------------------------------------------------------
// Entry

pub fn main() {
    let opts = parse_opts();
    match opts.cmd.as_str() {
        "preflight" => std::process::exit(preflight(true)),
        "inspect" => {
            let f = opts.file.clone().expect("usage: spike-encode inspect <file.mp4>");
            inspect(Path::new(&f));
        }
        "run" | "verify-bubble" => {
            let mtm = MainThreadMarker::new().expect("main thread");
            let app = NSApplication::sharedApplication(mtm);
            app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
            std::thread::spawn(move || {
                let code = if opts.cmd == "run" { run(&opts) } else { verify_bubble(&opts) };
                std::process::exit(code);
            });
            app.run();
        }
        _ => {
            eprintln!("{}", include_str!("main.rs").lines().take(24).map(|l| l.trim_start_matches("//!")).collect::<Vec<_>>().join("\n"));
        }
    }
}

// ---------------------------------------------------------------------------------------
// Permissions

const SCREEN_STEPS: &str = "\
Screen Recording permission is NOT granted to this process.
  1. System Settings > Privacy & Security > Screen & System Audio Recording.
  2. Turn on the app that launched this command (Terminal, iTerm, VS Code, Claude Code host...).
     Add it with the + button if it is not listed.
  3. Quit and reopen that app (macOS only applies the grant to new processes), then re-run.
  Shortcut: tccutil reset ScreenCapture   (clears old decisions so the prompt appears again)";

const CAMERA_STEPS: &str = "\
Camera permission is NOT granted to this process.
  1. System Settings > Privacy & Security > Camera, turn on the app that launched this command.
  2. If it is not listed, run once with --request-camera from a Terminal window to trigger the
     prompt (an IDE or agent host without NSCameraUsageDescription may be killed by macOS instead).
  3. Quit and reopen that app, then re-run. Or pass --camera off to skip the camera track.";

fn camera_status() -> AVAuthorizationStatus {
    unsafe { AVCaptureDevice::authorizationStatusForMediaType(AVMediaTypeVideo.unwrap()) }
}

fn camera_status_name(s: AVAuthorizationStatus) -> &'static str {
    match s {
        AVAuthorizationStatus::Authorized => "authorized",
        AVAuthorizationStatus::Denied => "denied",
        AVAuthorizationStatus::Restricted => "restricted",
        _ => "not determined",
    }
}

fn preflight(print: bool) -> i32 {
    let screen = CGPreflightScreenCaptureAccess();
    let cam = camera_status();
    if print {
        println!("terminal           : {}", std::env::var("TERM_PROGRAM").unwrap_or("?".into()));
        println!("screen recording   : {}", if screen { "granted" } else { "MISSING" });
        println!("camera             : {}", camera_status_name(cam));
        let dev = unsafe { AVCaptureDevice::defaultDeviceWithMediaType(AVMediaTypeVideo.unwrap()) };
        match dev {
            Some(d) => println!("default camera     : {}", unsafe { d.localizedName() }),
            None => println!("default camera     : none found"),
        }
        if let Ok(mtm) = MainThreadMarker::new().ok_or(()) {
            for s in NSScreen::screens(mtm).iter() {
                let f = s.frame();
                println!(
                    "screen             : {}x{} pt, backingScaleFactor {}",
                    f.size.width, f.size.height, s.backingScaleFactor()
                );
            }
        }
        if !screen {
            println!("\n{SCREEN_STEPS}");
        }
        if cam != AVAuthorizationStatus::Authorized {
            println!("\n{CAMERA_STEPS}");
        }
    }
    if screen && cam == AVAuthorizationStatus::Authorized {
        0
    } else {
        2
    }
}

fn request_camera_access() -> bool {
    let (tx, rx) = mpsc::channel();
    let block = RcBlock::new(move |granted: objc2::runtime::Bool| {
        let _ = tx.send(granted.as_bool());
    });
    unsafe {
        AVCaptureDevice::requestAccessForMediaType_completionHandler(
            AVMediaTypeVideo.unwrap(),
            &block,
        );
    }
    rx.recv_timeout(Duration::from_secs(60)).unwrap_or(false)
}

// ---------------------------------------------------------------------------------------
// Tracks: stats + the append hot path

#[derive(Default)]
struct Stats {
    delivered: u64,
    appended: u64,
    not_ready: u64,
    append_failed: u64,
    before_t0: u64,
    no_image: u64,
    missed_slots: u64,
    last_pts_ns: Option<i64>,
    first_pts_ns: Option<i64>,
    last_appended_pts_ns: i64,
    /// (host-now - pts) in ms for every appended frame, in order.
    lags_ms: Vec<f32>,
    dims: (usize, usize),
    /// inter-frame PTS gaps in units of 0.1 ms
    gaps: Vec<u32>,
    first_error: String,
}

struct Track {
    name: &'static str,
    interval_ns: i64,
    t0_ns: i64,
    input: SendBox<Retained<AVAssetWriterInput>>,
    writer: SendBox<Retained<AVAssetWriter>>,
    stats: Mutex<Stats>,
    stop: AtomicBool,
}

impl Track {
    fn rebase(&self, sb: &CMSampleBuffer, pts_ns: i64) -> Option<CFRetained<CMSampleBuffer>> {
        use std::mem::MaybeUninit;
        use std::ptr::NonNull;
        unsafe {
            let mut info = MaybeUninit::<CMSampleTimingInfo>::uninit();
            if sb.sample_timing_info(0, NonNull::new(info.as_mut_ptr())?) != 0 {
                return None;
            }
            let mut info = info.assume_init();
            info.presentationTimeStamp = cm(ns_to_ticks(pts_ns - self.t0_ns), TICKS_PER_SEC as i32);
            // ScreenCaptureKit leaves the duration invalid; the fMP4 segmenter needs one.
            if !info.duration.flags.contains(CMTimeFlags::Valid) {
                info.duration = cm(ns_to_ticks(self.interval_ns), TICKS_PER_SEC as i32);
            }
            let dts = info.decodeTimeStamp;
            if dts.flags.contains(CMTimeFlags::Valid) {
                info.decodeTimeStamp = cm(ns_to_ticks(cm_ns(dts) - self.t0_ns), TICKS_PER_SEC as i32);
            }
            let mut out: *mut CMSampleBuffer = std::ptr::null_mut();
            if CMSampleBuffer::create_copy_with_new_timing(None, sb, 1, &info, NonNull::new(&mut out)?) != 0 {
                return None;
            }
            NonNull::new(out).map(|p| CFRetained::from_raw(p))
        }
    }
    fn on_sample(&self, sb: &CMSampleBuffer, has_image: bool, dims: impl FnOnce() -> (usize, usize)) {
        let now = host_ns() as i64;
        let mut s = self.stats.lock().unwrap();
        s.delivered += 1;
        if self.stop.load(Ordering::Relaxed) {
            return;
        }
        if !has_image {
            s.no_image += 1; // SCK idle/blank status frames carry no pixel buffer
            return;
        }
        let pts = unsafe { sb.presentation_time_stamp() };
        let pts_ns = cm_ns(pts);
        if pts_ns < self.t0_ns {
            s.before_t0 += 1;
            return;
        }
        if s.first_pts_ns.is_none() {
            s.first_pts_ns = Some(pts_ns);
            s.dims = dims();
        }
        if let Some(prev) = s.last_pts_ns {
            let gap = pts_ns - prev;
            s.gaps.push((gap / 100_000) as u32);
            if gap > self.interval_ns * 3 / 2 {
                s.missed_slots += ((gap as f64 / self.interval_ns as f64).round() as u64).saturating_sub(1);
            }
        }
        s.last_pts_ns = Some(pts_ns);
        let ready = unsafe { self.input.0.isReadyForMoreMediaData() };
        if !ready {
            s.not_ready += 1;
            return;
        }
        // Rebase to the session origin: PTS (host ns) -> MediaTime ticks since t0. A new
        // CMSampleBuffer is built around the SAME pixel buffer (no pixel copy).
        let rebased = self.rebase(sb, pts_ns);
        let to_append: &CMSampleBuffer = rebased.as_deref().unwrap_or(sb);
        if unsafe { self.input.0.appendSampleBuffer(to_append) } {
            s.appended += 1;
            s.last_appended_pts_ns = pts_ns;
            s.lags_ms.push(((now - pts_ns) as f64 / 1e6) as f32);
        } else {
            s.append_failed += 1;
            if s.append_failed == 1 {
                let (st, e) = unsafe { (self.writer.0.status().0, self.writer.0.error().map(|e| {
                    let u = e.userInfo().objectForKey(objc2_foundation::NSUnderlyingErrorKey).and_then(|o| o.downcast::<NSError>().ok());
                    format!("{} ({}) underlying: {:?}", e.localizedDescription(), e.code(), u.map(|u| format!("{} ({}) {} {}", u.domain(), u.code(), u.localizedDescription(), u.localizedFailureReason().map(|r| r.to_string()).unwrap_or_default())))
                })) };
                s.first_error = format!("after {} frames: writer status {st}, error {e:?}", s.appended);
            }
        }
    }
}

// ---------------------------------------------------------------------------------------
// ObjC delegates

struct ScreenIvars {
    track: Arc<Track>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "SpikeScreenOutput"]
    #[ivars = ScreenIvars]
    struct ScreenOutput;

    unsafe impl NSObjectProtocol for ScreenOutput {}

    unsafe impl SCStreamOutput for ScreenOutput {
        #[unsafe(method(stream:didOutputSampleBuffer:ofType:))]
        unsafe fn stream_did_output(
            &self,
            _stream: &SCStream,
            sb: &CMSampleBuffer,
            ty: SCStreamOutputType,
        ) {
            if ty != SCStreamOutputType::Screen {
                return;
            }
            let img = unsafe { sb.image_buffer() };
            let has = img.is_some();
            self.ivars().track.on_sample(sb, has, || match &img {
                Some(pb) => (CVPixelBufferGetWidth(pb), CVPixelBufferGetHeight(pb)),
                None => (0, 0),
            });
        }
    }

    unsafe impl SCStreamDelegate for ScreenOutput {
        #[unsafe(method(stream:didStopWithError:))]
        unsafe fn stream_did_stop(&self, _stream: &SCStream, error: &NSError) {
            eprintln!("SCStream stopped with error: {}", error.localizedDescription());
        }
    }
);

impl ScreenOutput {
    fn new(track: Arc<Track>) -> Retained<Self> {
        let this = Self::alloc().set_ivars(ScreenIvars { track });
        unsafe { msg_send![super(this), init] }
    }
}

struct CameraIvars {
    track: Arc<Track>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "SpikeCameraOutput"]
    #[ivars = CameraIvars]
    struct CameraOutput;

    unsafe impl NSObjectProtocol for CameraOutput {}

    unsafe impl AVCaptureVideoDataOutputSampleBufferDelegate for CameraOutput {
        #[unsafe(method(captureOutput:didOutputSampleBuffer:fromConnection:))]
        unsafe fn capture_output(
            &self,
            _output: &AVCaptureOutput,
            sb: &CMSampleBuffer,
            _conn: &AVCaptureConnection,
        ) {
            let img = unsafe { sb.image_buffer() };
            let has = img.is_some();
            self.ivars().track.on_sample(sb, has, || match &img {
                Some(pb) => (CVPixelBufferGetWidth(pb), CVPixelBufferGetHeight(pb)),
                None => (0, 0),
            });
        }
    }
);

impl CameraOutput {
    fn new(track: Arc<Track>) -> Retained<Self> {
        let this = Self::alloc().set_ivars(CameraIvars { track });
        unsafe { msg_send![super(this), init] }
    }
}

/// Receives the fragmented-MP4 segments from AVAssetWriter (init segment, then one
/// "separable" segment per ~2 s, each starting at a sync sample) and appends them to the file.
struct SegState {
    file: std::fs::File,
    init_bytes: usize,
    /// (earliest PTS s, duration s, bytes)
    segs: Vec<(f64, f64, usize)>,
}

struct SegIvars {
    st: Mutex<SegState>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "SpikeSegmentSink"]
    #[ivars = SegIvars]
    struct SegmentSink;

    unsafe impl NSObjectProtocol for SegmentSink {}

    unsafe impl AVAssetWriterDelegate for SegmentSink {
        #[unsafe(method(assetWriter:didOutputSegmentData:segmentType:segmentReport:))]
        unsafe fn did_output(
            &self,
            _writer: &AVAssetWriter,
            data: &NSData,
            ty: AVAssetSegmentType,
            report: Option<&AVAssetSegmentReport>,
        ) {
            use std::io::Write;
            let bytes = data.to_vec();
            let mut st = self.ivars().st.lock().unwrap();
            let _ = st.file.write_all(&bytes);
            if ty == AVAssetSegmentType::Initialization {
                st.init_bytes = bytes.len();
            } else if let Some(r) = report {
                if let Some(t) = r.trackReports().iter().next() {
                    let (p, d) = (cm_ns(t.earliestPresentationTimeStamp()), cm_ns(t.duration()));
                    st.segs.push((p as f64 / 1e9, d as f64 / 1e9, bytes.len()));
                }
            }
        }
    }
);

impl SegmentSink {
    fn new(file: std::fs::File) -> Retained<Self> {
        let this = Self::alloc().set_ivars(SegIvars {
            st: Mutex::new(SegState { file, init_bytes: 0, segs: Vec::new() }),
        });
        unsafe { msg_send![super(this), init] }
    }
}

/// verify-bubble: copy one full BGRA frame to the CPU (deliberately, to look at it).
struct GrabIvars {
    seen: AtomicU64,
    want: u64,
    frame: Mutex<Option<(usize, usize, Vec<u8>)>>, // w, h, tightly packed BGRA
}

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "SpikeGrabOutput"]
    #[ivars = GrabIvars]
    struct GrabOutput;

    unsafe impl NSObjectProtocol for GrabOutput {}

    unsafe impl SCStreamOutput for GrabOutput {
        #[unsafe(method(stream:didOutputSampleBuffer:ofType:))]
        unsafe fn stream_did_output(
            &self,
            _stream: &SCStream,
            sb: &CMSampleBuffer,
            ty: SCStreamOutputType,
        ) {
            if ty != SCStreamOutputType::Screen {
                return;
            }
            let Some(pb) = (unsafe { sb.image_buffer() }) else { return };
            let n = self.ivars().seen.fetch_add(1, Ordering::SeqCst) + 1;
            if n != self.ivars().want {
                return;
            }
            unsafe {
                CVPixelBufferLockBaseAddress(&pb, CVPixelBufferLockFlags::ReadOnly);
                let (w, h) = (CVPixelBufferGetWidth(&pb), CVPixelBufferGetHeight(&pb));
                let stride = CVPixelBufferGetBytesPerRow(&pb);
                let base = CVPixelBufferGetBaseAddress(&pb) as *const u8;
                let mut v = Vec::with_capacity(w * h * 4);
                for y in 0..h {
                    v.extend_from_slice(std::slice::from_raw_parts(base.add(y * stride), w * 4));
                }
                CVPixelBufferUnlockBaseAddress(&pb, CVPixelBufferLockFlags::ReadOnly);
                *self.ivars().frame.lock().unwrap() = Some((w, h, v));
            }
        }
    }
);

impl GrabOutput {
    fn new(want: u64) -> Retained<Self> {
        let this = Self::alloc().set_ivars(GrabIvars {
            seen: AtomicU64::new(0),
            want,
            frame: Mutex::new(None),
        });
        unsafe { msg_send![super(this), init] }
    }
}

// ---------------------------------------------------------------------------------------
// Writers

fn s(x: &str) -> Retained<NSString> {
    NSString::from_str(x)
}

fn num_i(v: i64) -> Retained<NSNumber> {
    NSNumber::new_i64(v)
}

fn num_f(v: f64) -> Retained<NSNumber> {
    NSNumber::new_f64(v)
}

struct VideoSpec {
    codec: &'static str, // "hvc1" or "avc1"
    width: usize,
    height: usize,
    bitrate: i64,
    fps: u32,
    container: String,
}

struct WriterBundle {
    writer: Retained<AVAssetWriter>,
    input: Retained<AVAssetWriterInput>,
    /// Present in the segment (fMP4) mode; the writer's delegate is a weak reference.
    sink: Option<Retained<SegmentSink>>,
}

fn make_writer(path: &Path, spec: &VideoSpec, _t0_ns: i64) -> Result<WriterBundle, String> {
    let _ = std::fs::remove_file(path);
    let mut sink = None;
    let writer = if spec.container == "cmaf" {
        // Fragmented MP4 the documented way: segment delegate + CMAF profile, 2 s segments.
        let file = std::fs::File::create(path).map_err(|e| format!("create {}: {e}", path.display()))?;
        let w = unsafe { AVAssetWriter::initWithContentType(AVAssetWriter::alloc(), UTTypeMPEG4Movie) };
        let sk = SegmentSink::new(file);
        unsafe {
            w.setOutputFileTypeProfile(AVFileTypeProfileMPEG4CMAFCompliant);
            w.setPreferredOutputSegmentInterval(cm(2, 1));
            w.setInitialSegmentStartTime(cm(0, TICKS_PER_SEC as i32));
            w.setDelegate(Some(ProtocolObject::from_ref(&*sk)));
        }
        sink = Some(sk);
        w
    } else {
        // Plain file writer; `movieFragmentInterval` is set but, measured, has no effect.
        let url = NSURL::fileURLWithPath(&s(path.to_str().unwrap()));
        let w = unsafe {
            AVAssetWriter::assetWriterWithURL_fileType_error(
                &url,
                if spec.container == "mov" { AVFileTypeQuickTimeMovie.unwrap() } else { AVFileTypeMPEG4.unwrap() },
            )
        }
        .map_err(|e| format!("AVAssetWriter: {}", e.localizedDescription()))?;
        unsafe { w.setMovieFragmentInterval(cm(2, 1)) };
        w
    };
    let profile = if spec.codec == "hvc1" { "HEVC_Main_AutoLevel" } else { "H264_High_AutoLevel" };
    let (k_br, k_fps, k_kf, k_pl, k_ro) = (s("AverageBitRate"), s("ExpectedFrameRate"), s("MaxKeyFrameIntervalDuration"), s("ProfileLevel"), s("AllowFrameReordering"));
    let (v_br, v_fps, v_kf, v_pl) = (num_i(spec.bitrate), num_i(spec.fps as i64), num_f(2.0), s(profile));
    let v_ro = NSNumber::new_bool(std::env::var("SPIKE_BFRAMES").is_ok());
    let props: Retained<NSDictionary<NSString, AnyObject>> = NSDictionary::from_slices(
        &[&*k_br, &*k_fps, &*k_kf, &*k_pl, &*k_ro],
        &[&**v_br, &**v_fps, &**v_kf, &**v_pl, &**v_ro],
    );
    let (k_c, k_w, k_h, k_p) = (s("AVVideoCodecKey"), s("AVVideoWidthKey"), s("AVVideoHeightKey"), s("AVVideoCompressionPropertiesKey"));
    let (v_c, v_w, v_h) = (s(spec.codec), num_i(spec.width as i64), num_i(spec.height as i64));
    let settings: Retained<NSDictionary<NSString, AnyObject>> = NSDictionary::from_slices(
        &[&*k_c, &*k_w, &*k_h, &*k_p],
        &[&**v_c, &**v_w, &**v_h, &**props],
    );
    let input = unsafe {
        AVAssetWriterInput::assetWriterInputWithMediaType_outputSettings(
            AVMediaTypeVideo.unwrap(),
            Some(&settings),
        )
    };
    unsafe {
        input.setExpectsMediaDataInRealTime(true);
        input.setMediaTimeScale(TICKS_PER_SEC as i32); // track time base = MediaTime ticks
        if !writer.canAddInput(&input) {
            return Err("writer rejected the input (codec/size not supported)".into());
        }
        writer.addInput(&input);
        if !writer.startWriting() {
            let e = writer.error().map(|e| e.localizedDescription().to_string());
            return Err(format!("startWriting failed: {e:?}"));
        }
        writer.startSessionAtSourceTime(cm(0, TICKS_PER_SEC as i32));
    }
    Ok(WriterBundle { writer, input, sink })
}

fn finish_writer(w: &AVAssetWriter, input: &AVAssetWriterInput) -> Result<(), String> {
    unsafe { input.markAsFinished() };
    let (tx, rx) = mpsc::channel();
    let block = RcBlock::new(move || {
        let _ = tx.send(());
    });
    unsafe { w.finishWritingWithCompletionHandler(&block) };
    rx.recv_timeout(Duration::from_secs(60)).map_err(|_| "finishWriting timed out".to_string())?;
    let status = unsafe { w.status() };
    if status.0 == 2 {
        Ok(())
    } else {
        let e = unsafe { w.error() }.map(|e| e.localizedDescription().to_string());
        Err(format!("writer status {} error {:?}", status.0, e))
    }
}

// ---------------------------------------------------------------------------------------
// Windows (main thread)

thread_local! {
    static KEEP: RefCell<Vec<Retained<NSWindow>>> = const { RefCell::new(Vec::new()) };
}

/// Where the bubble sits, in points, AppKit bottom-left coordinates.
const BUBBLE: NSRect = NSRect { origin: NSPoint { x: 100.0, y: 100.0 }, size: NSSize { width: 300.0, height: 300.0 } };
const MOTION: NSRect = NSRect { origin: NSPoint { x: 600.0, y: 300.0 }, size: NSSize { width: 900.0, height: 600.0 } };

fn borderless(mtm: MainThreadMarker, rect: NSRect) -> Retained<NSWindow> {
    let w = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            mtm.alloc(),
            rect,
            NSWindowStyleMask::Borderless,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    unsafe { w.setReleasedWhenClosed(false) };
    w.setOpaque(false);
    w.setBackgroundColor(Some(&NSColor::clearColor()));
    w.setHasShadow(false);
    w.setIgnoresMouseEvents(true);
    w.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::Stationary
            | NSWindowCollectionBehavior::FullScreenAuxiliary,
    );
    if let Some(v) = w.contentView() {
        v.setWantsLayer(true);
    }
    w
}

/// Create the bubble: always on top (screen-saver level, as S2 found necessary), circular.
fn make_bubble(
    mtm: MainThreadMarker,
    camera: Option<&Retained<AVCaptureSession>>,
    sharing_none: bool,
) -> Retained<NSWindow> {
    let w = borderless(mtm, BUBBLE);
    w.setLevel(1000); // kCGScreenSaverWindowLevel
    if sharing_none {
        w.setSharingType(NSWindowSharingType::None);
    }
    let view = w.contentView().unwrap();
    let layer = view.layer().unwrap();
    layer.setCornerRadius(BUBBLE.size.width / 2.0);
    layer.setMasksToBounds(true);
    match camera {
        None => {
            layer.setBackgroundColor(Some(&NSColor::magentaColor().CGColor()));
        }
        Some(session) => {
            let pl = unsafe { AVCaptureVideoPreviewLayer::layerWithSession(session) };
            pl.setFrame(objc2_foundation::NSRect::new(NSPoint::new(0.0, 0.0), BUBBLE.size));
            layer.addSublayer(&pl);
        }
    }
    w.orderFrontRegardless();
    KEEP.with(|k| k.borrow_mut().push(w.clone()));
    w
}

/// A window with rotating coloured bars, animated by the Core Animation render server
/// (driven by the display's vsync, not by this process), so the display produces a new frame
/// at its full rate and ScreenCaptureKit has something to deliver at 60 fps. (A first version
/// used an NSTimer; its jitter made the stimulus itself skip vsyncs.)
fn make_motion(mtm: MainThreadMarker) {
    let w = borderless(mtm, MOTION);
    w.setLevel(8);
    w.setOpaque(true);
    let layer = w.contentView().unwrap().layer().unwrap();
    layer.setMasksToBounds(true);
    layer.setBackgroundColor(Some(&NSColor::colorWithSRGBRed_green_blue_alpha(0.1, 0.2, 0.5, 1.0).CGColor()));
    let (cx, cy) = (MOTION.size.width / 2.0, MOTION.size.height / 2.0);
    for i in 0..6 {
        let bar = CALayer::new();
        let (bw, bh) = (40.0 + 30.0 * i as f64, 1400.0);
        bar.setBounds(NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(bw, bh)));
        bar.setPosition(NSPoint::new(cx, cy));
        let hue = i as f64 / 6.0;
        bar.setBackgroundColor(Some(&NSColor::colorWithHue_saturation_brightness_alpha(hue, 0.85, 0.95, 1.0).CGColor()));
        let anim = CABasicAnimation::animationWithKeyPath(Some(&s("transform.rotation.z")));
        let (from, to) = (NSNumber::new_f64(i as f64 * 0.5), NSNumber::new_f64(i as f64 * 0.5 + std::f64::consts::TAU));
        unsafe {
            anim.setFromValue(Some(&**from));
            anim.setToValue(Some(&**to));
            anim.setDuration(2.0 + i as f64);
            anim.setRepeatCount(f32::INFINITY);
            bar.addAnimation_forKey(&anim, Some(&s("spin")));
        }
        layer.addSublayer(&bar);
    }
    w.orderFrontRegardless();
    KEEP.with(|k| k.borrow_mut().push(w));
}

// ---------------------------------------------------------------------------------------
// ScreenCaptureKit plumbing (worker thread)

fn get_content() -> Result<Retained<SCShareableContent>, String> {
    let (tx, rx) = mpsc::channel::<Result<SendBox<Retained<SCShareableContent>>, String>>();
    let block = RcBlock::new(move |c: *mut SCShareableContent, e: *mut NSError| {
        let r = match unsafe { Retained::retain(c) } {
            Some(c) => Ok(SendBox(c)),
            None => Err(unsafe { e.as_ref() }
                .map(|e| e.localizedDescription().to_string())
                .unwrap_or_else(|| "unknown error".into())),
        };
        let _ = tx.send(r);
    });
    unsafe { SCShareableContent::getShareableContentWithCompletionHandler(&block) };
    rx.recv_timeout(Duration::from_secs(20))
        .map_err(|_| "SCShareableContent timed out".to_string())?
        .map(|b| b.0)
}

fn main_display(content: &SCShareableContent) -> Option<Retained<SCDisplay>> {
    let main_id = objc2_core_graphics::CGMainDisplayID();
    let displays = unsafe { content.displays() };
    displays.iter().find(|d| unsafe { d.displayID() } == main_id).or_else(|| displays.iter().next())
}

fn windows_by_number(content: &SCShareableContent, numbers: &[u32]) -> Vec<Retained<SCWindow>> {
    unsafe { content.windows() }
        .iter()
        .filter(|w| numbers.contains(&unsafe { w.windowID() }))
        .collect()
}

fn native_pixels(display: &SCDisplay) -> (usize, usize, f32) {
    let filter = unsafe {
        SCContentFilter::initWithDisplay_excludingWindows(
            SCContentFilter::alloc(),
            display,
            &NSArray::new(),
        )
    };
    let scale = unsafe { SCShareableContent::infoForFilter(&filter).pointPixelScale() };
    let (w, h) = unsafe { (display.width() as f32, display.height() as f32) };
    ((w * scale).round() as usize, (h * scale).round() as usize, scale)
}

fn start_stream(
    display: &SCDisplay,
    exclude: &[Retained<SCWindow>],
    w: usize,
    h: usize,
    fps: u32,
    pixfmt: u32,
    output: &ProtocolObject<dyn SCStreamOutput>,
    delegate: Option<&ProtocolObject<dyn SCStreamDelegate>>,
    queue: &DispatchRetained<DispatchQueue>,
) -> Result<Retained<SCStream>, String> {
    let wins = NSArray::from_retained_slice(exclude);
    let filter = unsafe {
        SCContentFilter::initWithDisplay_excludingWindows(SCContentFilter::alloc(), display, &wins)
    };
    let cfg = unsafe { SCStreamConfiguration::new() };
    unsafe {
        cfg.setWidth(w);
        cfg.setHeight(h);
        cfg.setMinimumFrameInterval(cm(1, fps as i32));
        cfg.setPixelFormat(pixfmt);
        cfg.setShowsCursor(true);
        cfg.setQueueDepth(8);
    }
    let stream = unsafe {
        SCStream::initWithFilter_configuration_delegate(SCStream::alloc(), &filter, &cfg, delegate)
    };
    unsafe {
        stream
            .addStreamOutput_type_sampleHandlerQueue_error(output, SCStreamOutputType::Screen, Some(queue))
            .map_err(|e| format!("addStreamOutput: {}", e.localizedDescription()))?;
    }
    let (tx, rx) = mpsc::channel::<Option<String>>();
    let block = RcBlock::new(move |e: *mut NSError| {
        let _ = tx.send(unsafe { e.as_ref() }.map(|e| e.localizedDescription().to_string()));
    });
    unsafe { stream.startCaptureWithCompletionHandler(Some(&block)) };
    match rx.recv_timeout(Duration::from_secs(20)) {
        Ok(None) => Ok(stream),
        Ok(Some(e)) => Err(format!("startCapture: {e}")),
        Err(_) => Err("startCapture timed out".into()),
    }
}

fn stop_stream(stream: &SCStream) {
    let (tx, rx) = mpsc::channel::<()>();
    let block = RcBlock::new(move |_e: *mut NSError| {
        let _ = tx.send(());
    });
    unsafe { stream.stopCaptureWithCompletionHandler(Some(&block)) };
    let _ = rx.recv_timeout(Duration::from_secs(10));
}

// ---------------------------------------------------------------------------------------
// verify-bubble

fn magenta_fraction(frame: &(usize, usize, Vec<u8>), x0: f64, y_top0: f64, x1: f64, y_top1: f64) -> f64 {
    let (w, h, px) = (frame.0, frame.1, &frame.2);
    let (xa, xb) = (x0.max(0.0) as usize, (x1 as usize).min(w));
    let (ya, yb) = (y_top0.max(0.0) as usize, (y_top1 as usize).min(h));
    let (mut hit, mut n) = (0usize, 0usize);
    for y in ya..yb {
        for x in xa..xb {
            let i = (y * w + x) * 4;
            let (b, g, r) = (px[i], px[i + 1], px[i + 2]);
            if r > 180 && b > 180 && g < 110 {
                hit += 1;
            }
            n += 1;
        }
    }
    hit as f64 / n.max(1) as f64
}

fn verify_bubble(_o: &Opts) -> i32 {
    if !CGPreflightScreenCaptureAccess() {
        println!("{SCREEN_STEPS}");
        println!("\nverify-bubble: NOT RUN (permission missing)");
        return 2;
    }
    let (win_no, scr_h) = on_main(|mtm| {
        make_motion(mtm);
        let w = make_bubble(mtm, None, false);
        let scr_h = NSScreen::mainScreen(mtm).map(|s| s.frame().size.height).unwrap_or(0.0);
        (w.windowNumber() as u32, scr_h)
    });
    std::thread::sleep(Duration::from_millis(800));
    println!("bubble window number {win_no}, solid magenta, 300x300 pt at (100,100) bottom-left");
    // (label, set NSWindow.sharingType = none, exclude via SCContentFilter)
    let modes = [
        ("control: no exclusion", false, false),
        ("sharingType = none only", true, false),
        ("SCContentFilter exclusion only", false, true),
        ("both", true, true),
    ];
    let mut results = Vec::new();
    for (label, sharing_none, filter) in modes {
        on_main(move |_m| {
            KEEP.with(|k| {
                for w in k.borrow().iter() {
                    if w.windowNumber() as u32 == win_no {
                        w.setSharingType(if sharing_none {
                            NSWindowSharingType::None
                        } else {
                            NSWindowSharingType::ReadOnly
                        });
                    }
                }
            })
        });
        std::thread::sleep(Duration::from_millis(500));
        let content = match get_content() {
            Ok(c) => c,
            Err(e) => {
                println!("SCShareableContent failed: {e}");
                return 1;
            }
        };
        let display = main_display(&content).unwrap();
        let (pw, ph, scale) = native_pixels(&display);
        let excl = if filter { windows_by_number(&content, &[win_no]) } else { vec![] };
        if filter && excl.is_empty() {
            println!("  (bubble window not found by SCShareableContent)");
        }
        let grab = GrabOutput::new(10);
        let queue = DispatchQueue::new("spike.grab", None);
        let stream = match start_stream(
            &display,
            &excl,
            pw,
            ph,
            60,
            FOURCC_BGRA,
            ProtocolObject::from_ref(&*grab),
            None,
            &queue,
        ) {
            Ok(s) => s,
            Err(e) => {
                println!("{e}");
                return 1;
            }
        };
        let t = Instant::now();
        while grab.ivars().frame.lock().unwrap().is_none() && t.elapsed() < Duration::from_secs(8) {
            std::thread::sleep(Duration::from_millis(50));
        }
        stop_stream(&stream);
        let frame = grab.ivars().frame.lock().unwrap().take();
        let Some(frame) = frame else {
            println!("{label}: no frame received");
            return 1;
        };
        let sc = scale as f64;
        let _ = std::fs::create_dir_all(&_o.out);
        let slug: String = label.chars().filter(|c| c.is_ascii_alphanumeric()).take(12).collect();
        let raw = _o.out.join(format!("verify-{slug}.bgra"));
        if std::fs::write(&raw, &frame.2).is_ok() {
            // Downscaled PNG so a human can eyeball it.
            let _ = Command::new("ffmpeg")
                .args(["-v", "error", "-y", "-f", "rawvideo", "-pix_fmt", "bgra", "-s", &format!("{}x{}", frame.0, frame.1), "-i"])
                .arg(&raw)
                .args(["-vf", "scale=1512:-2", "-frames:v", "1"])
                .arg(raw.with_extension("png"))
                .status();
            let _ = std::fs::remove_file(&raw);
        }
        let (x, y, w, h) = (BUBBLE.origin.x, BUBBLE.origin.y, BUBBLE.size.width, BUBBLE.size.height);
        let ytop = scr_h - y - h;
        // The inscribed square of the circle, scaled to pixels.
        let inset = w * 0.2;
        let frac = magenta_fraction(
            &frame,
            (x + inset) * sc,
            (ytop + inset) * sc,
            (x + w - inset) * sc,
            (ytop + h - inset) * sc,
        );
        println!("  {label:<34} frame {}x{}  magenta in bubble area: {:5.1}%", frame.0, frame.1, frac * 100.0);
        results.push((label, frac));
    }
    let control_ok = results[0].1 > 0.9;
    let sharing_hidden = results[1].1 < 0.01;
    let filter_hidden = results[2].1 < 0.01;
    let both_hidden = results[3].1 < 0.01;
    println!("\ncontrol shows bubble (test is valid) : {}", yn(control_ok));
    println!("sharingType=none alone hides it      : {}", yn(sharing_hidden));
    println!("SCContentFilter exclusion hides it   : {}", yn(filter_hidden));
    println!("both hide it                         : {}", yn(both_hidden));
    let pass = control_ok && filter_hidden && both_hidden;
    println!("verify-bubble: {}", if pass { "PASS" } else { "FAIL" });
    if pass { 0 } else { 1 }
}

fn yn(b: bool) -> &'static str {
    if b { "yes" } else { "NO" }
}

// ---------------------------------------------------------------------------------------
// run

struct TrackReport {
    name: &'static str,
    file: PathBuf,
    expected: f64,
    delivered: u64,
    appended: u64,
    not_ready: u64,
    append_failed: u64,
    before_t0: u64,
    missed: u64,
    late: u64,
    late_pct: f64,
    fps: f64,
    lag_median: f64,
    lag_max_rel: f64,
    lag_final_rel: f64,
    first_pts_ticks: i64,
    last_pts_ticks: i64,
    dims: (usize, usize),
    bytes: u64,
    mbps: f64,
    first_error: String,
    gap_p: [f64; 4],
    gap_hist: [u64; 4],
}

fn report_track(t: &Track, file: &Path, dur_s: f64) -> TrackReport {
    let s = t.stats.lock().unwrap();
    let mut sorted = s.lags_ms.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = sorted.get(sorted.len() / 2).copied().unwrap_or(0.0) as f64;
    let interval_ms = t.interval_ns as f64 / 1e6;
    let late = s.lags_ms.iter().filter(|l| (**l as f64 - median) > interval_ms).count() as u64;
    let max_rel = s.lags_ms.iter().map(|l| *l as f64 - median).fold(0.0, f64::max);
    let n_tail = ((1e9 / t.interval_ns as f64) as usize).clamp(1, s.lags_ms.len().max(1));
    let tail: f64 = s.lags_ms.iter().rev().take(n_tail).map(|l| *l as f64).sum::<f64>() / n_tail as f64;
    let first = s.first_pts_ns.unwrap_or(0);
    let span_s = (s.last_appended_pts_ns - first).max(0) as f64 / 1e9;
    let expected = (span_s * 1e9 / t.interval_ns as f64).round() + 1.0;
    // A frame is late if it arrived > 1 interval behind steady state, or never made it onto its
    // 60 Hz slot (gap-detected misses, not-ready drops, append failures, or a plain shortfall).
    let shortfall = (expected as u64).saturating_sub(s.appended);
    let bad = (late + shortfall.max(s.missed_slots + s.not_ready + s.append_failed)) as f64;
    let mut g = s.gaps.clone();
    g.sort();
    let pick = |q: f64| g.get(((g.len() as f64 - 1.0) * q) as usize).copied().unwrap_or(0) as f64 / 10.0;
    let iv = t.interval_ns as f64 / 1e5;
    let mut gap_hist = [0u64; 4];
    for x in &g {
        let r = *x as f64 / iv;
        gap_hist[if r < 1.25 { 0 } else if r < 1.5 { 1 } else if r < 2.5 { 2 } else { 3 }] += 1;
    }
    let bytes = std::fs::metadata(file).map(|m| m.len()).unwrap_or(0);
    TrackReport {
        name: t.name,
        file: file.to_path_buf(),
        expected,
        delivered: s.delivered,
        appended: s.appended,
        not_ready: s.not_ready,
        append_failed: s.append_failed,
        before_t0: s.before_t0,
        missed: s.missed_slots,
        late,
        late_pct: 100.0 * bad / expected.max(1.0),
        fps: s.appended as f64 / span_s.max(1e-9),
        lag_median: median,
        lag_max_rel: max_rel,
        lag_final_rel: tail - median,
        first_pts_ticks: ns_to_ticks(first - t.t0_ns),
        last_pts_ticks: ns_to_ticks(s.last_appended_pts_ns - t.t0_ns),
        dims: s.dims,
        bytes,
        mbps: bytes as f64 * 8.0 / dur_s.max(1e-9) / 1e6,
        first_error: s.first_error.clone(),
        gap_p: [pick(0.5), pick(0.95), pick(0.99), pick(1.0)],
        gap_hist,
    }
}

fn print_track(r: &TrackReport) {
    println!("--- {} track: {}", r.name, r.file.display());
    println!("  frame size            {}x{}", r.dims.0, r.dims.1);
    println!("  delivered / appended  {} / {}  (before t0 {}, not-ready drops {}, append failures {})", r.delivered, r.appended, r.before_t0, r.not_ready, r.append_failed);
    if !r.first_error.is_empty() {
        println!("  FIRST APPEND FAILURE  {}", r.first_error);
    }
    println!("  achieved fps          {:.2}", r.fps);
    println!("  expected frames       {:.0}  (missed slots from PTS gaps: {})", r.expected, r.missed);
    println!("  inter-frame gap ms    p50 {:.2}  p95 {:.2}  p99 {:.2}  max {:.2}   (histogram, in intervals: <1.25: {}, 1.25-1.5: {}, 1.5-2.5: {}, >2.5: {})", r.gap_p[0], r.gap_p[1], r.gap_p[2], r.gap_p[3], r.gap_hist[0], r.gap_hist[1], r.gap_hist[2], r.gap_hist[3]);
    println!("  late frames           {} lag-late + max(shortfall {} of expected, {} gap-missed + {} not-ready + {} failed) = {:.2}%", r.late, (r.expected as u64).saturating_sub(r.appended), r.missed, r.not_ready, r.append_failed, r.late_pct);
    println!("  delivery lag (host now - PTS): median {:.1} ms; max above median {:.1} ms; final-second mean above median {:+.1} ms", r.lag_median, r.lag_max_rel, r.lag_final_rel);
    println!("  first / last PTS      {} / {} ticks (120 kHz, relative to t0)", r.first_pts_ticks, r.last_pts_ticks);
    println!("  file                  {:.1} MB, {:.1} Mbps", r.bytes as f64 / 1e6, r.mbps);
}

fn json_track(r: &TrackReport) -> String {
    format!(
        "{{\"name\":\"{}\",\"w\":{},\"h\":{},\"delivered\":{},\"appended\":{},\"not_ready\":{},\"before_t0\":{},\"missed\":{},\"late\":{},\"late_pct\":{:.3},\"fps\":{:.3},\"lag_median_ms\":{:.2},\"lag_max_above_median_ms\":{:.2},\"lag_final_above_median_ms\":{:.2},\"first_pts_ticks\":{},\"mbps\":{:.2}}}",
        r.name, r.dims.0, r.dims.1, r.delivered, r.appended, r.not_ready, r.before_t0, r.missed, r.late, r.late_pct, r.fps, r.lag_median, r.lag_max_rel, r.lag_final_rel, r.first_pts_ticks, r.mbps
    )
}

fn run(o: &Opts) -> i32 {
    println!("== spike-encode run: {:?}", o);
    // ---- permissions
    let screen_ok = CGPreflightScreenCaptureAccess();
    if !screen_ok {
        println!("{SCREEN_STEPS}");
        println!("\nrun: NOT RUN (Screen Recording permission missing)");
        return 2;
    }
    let mut cam_status = camera_status();
    if cam_status == AVAuthorizationStatus::NotDetermined && o.request_camera {
        println!("requesting camera access (a system prompt should appear)...");
        request_camera_access();
        cam_status = camera_status();
    }
    let want_cam = o.camera.unwrap_or(cam_status == AVAuthorizationStatus::Authorized);
    let cam_ok = cam_status == AVAuthorizationStatus::Authorized;
    if want_cam && !cam_ok {
        println!("camera status: {}\n{CAMERA_STEPS}\n(continuing screen-only)", camera_status_name(cam_status));
    }
    let use_cam = want_cam && cam_ok;
    if let Err(e) = std::fs::create_dir_all(&o.out) {
        println!("cannot create {}: {e}", o.out.display());
        return 1;
    }

    // ---- display and sizes
    let content = match get_content() {
        Ok(c) => c,
        Err(e) => {
            println!("SCShareableContent: {e}\n{SCREEN_STEPS}");
            return 2;
        }
    };
    let display = main_display(&content).expect("no display");
    let (nw, nh, scale) = native_pixels(&display);
    let (w, h) = (o.width.unwrap_or(nw) & !1, o.height.unwrap_or(nh) & !1);
    println!("display native {nw}x{nh} px (scale {scale}); capturing {w}x{h} @ {} fps, pixfmt {}", o.fps, if o.pixfmt == FOURCC_BGRA { "BGRA" } else { "420v" });

    // ---- camera session (main-thread preview layer needs the session first)
    let cam_session: Option<SendBox<Retained<AVCaptureSession>>> = if use_cam {
        match unsafe { build_camera(o.camera_720) } {
            Ok(s) => Some(SendBox(s)),
            Err(e) => {
                println!("camera setup failed: {e} (continuing screen-only)");
                None
            }
        }
    } else {
        None
    };

    // ---- windows
    let bubble_mode = o.bubble.clone();
    let sharing_none = matches!(o.exclude.as_str(), "sharing" | "both");
    let filter_excl = matches!(o.exclude.as_str(), "filter" | "both");
    let motion = o.motion;
    let sess_for_main = cam_session.as_ref().map(|s| SendBox(s.0.clone()));
    let win_no = on_main(move |mtm| {
        if motion {
            make_motion(mtm);
        }
        if bubble_mode == "off" {
            return 0;
        }
        let cam = if bubble_mode == "camera" { sess_for_main.as_ref().map(|s| &s.0) } else { None };
        make_bubble(mtm, cam, sharing_none).windowNumber() as u32
    });
    std::thread::sleep(Duration::from_millis(800));
    let scr_h = on_main(|mtm| NSScreen::mainScreen(mtm).map(|s| s.frame().size.height).unwrap_or(0.0));

    let content = get_content().unwrap_or(content);
    let display = main_display(&content).unwrap_or(display);
    let excl = if filter_excl && win_no != 0 { windows_by_number(&content, &[win_no]) } else { vec![] };
    if filter_excl && win_no != 0 && excl.is_empty() {
        println!("WARNING: bubble window {win_no} not found in SCShareableContent; filter exclusion NOT applied");
    }

    // ---- writers (one clock: both sessions start at t0)
    let screen_codec = if o.codec == "h264" { "avc1" } else { "hvc1" };
    let mbps = o.mbps.unwrap_or(match (o.codec.as_str(), o.fps) {
        ("h264", f) if f > 30 => 90.0,
        ("h264", _) => 60.0,
        (_, f) if f > 30 => 60.0,
        _ => 40.0,
    });
    let t0 = host_ns() as i64 + 200_000_000; // 200 ms ahead: both writers' session origin
    let screen_path = o.out.join(format!("screen-{}-{}p{}.{}", o.codec, h, o.fps, if o.container == "cmaf" { "mp4" } else { o.container.as_str() }));
    let spec = VideoSpec { codec: screen_codec, width: w, height: h, bitrate: (mbps * 1e6) as i64, fps: o.fps, container: o.container.clone() };
    let sbundle = match make_writer(&screen_path, &spec, t0) {
        Ok(x) => x,
        Err(e) => {
            println!("screen writer: {e}");
            return 1;
        }
    };
    let (sw, sinput) = (sbundle.writer.clone(), sbundle.input.clone());
    let screen_track = Arc::new(Track {
        name: "screen",
        interval_ns: 1_000_000_000 / o.fps as i64,
        t0_ns: t0,
        input: SendBox(sinput.clone()),
        writer: SendBox(sw.clone()),
        stats: Mutex::new(Stats::default()),
        stop: AtomicBool::new(false),
    });

    let cam_dims = if o.camera_720 { (1280, 720) } else { (1920, 1080) };
    let cam_path = o.out.join(format!("camera-h264-{}p.{}", cam_dims.1, if o.container == "cmaf" { "mp4" } else { o.container.as_str() }));
    let mut cam_parts = None;
    let mut cam_bundle = None;
    if let Some(sess) = &cam_session {
        let cspec = VideoSpec { codec: "avc1", width: cam_dims.0, height: cam_dims.1, bitrate: 8_000_000, fps: 30, container: o.container.clone() };
        match make_writer(&cam_path, &cspec, t0) {
            Ok(b) => {
                let (cw, cinput) = (b.writer.clone(), b.input.clone());
                cam_bundle = Some(b);
                let track = Arc::new(Track {
                    name: "camera",
                    interval_ns: 1_000_000_000 / 30,
                    t0_ns: t0,
                    input: SendBox(cinput.clone()),
                    writer: SendBox(cw.clone()),
                    stats: Mutex::new(Stats::default()),
                    stop: AtomicBool::new(false),
                });
                cam_parts = Some((cw, cinput, track, sess.0.clone()));
            }
            Err(e) => println!("camera writer: {e} (continuing screen-only)"),
        }
    }

    // ---- start: camera first (it is already live for the bubble in real use), then screen
    let cam_out = cam_parts.as_ref().map(|(_, _, track, sess)| {
        let out = CameraOutput::new(track.clone());
        let q = DispatchQueue::new("spike.camera", None);
        unsafe { attach_camera_output(sess, &out, &q) };
        (out, q)
    });
    let screen_out = ScreenOutput::new(screen_track.clone());
    let squeue = DispatchQueue::new("spike.screen", None);
    // Wait for t0 so every sample is on or after the common origin.
    while (host_ns() as i64) < t0 {
        std::thread::sleep(Duration::from_millis(1));
    }
    let cpu0 = cpu_secs();
    let wall0 = Instant::now();
    let stream = match start_stream(
        &display,
        &excl,
        w,
        h,
        o.sck_rate.unwrap_or((o.fps as f64 * 1.1).round() as i32) as u32,
        o.pixfmt,
        ProtocolObject::from_ref(&*screen_out),
        Some(ProtocolObject::from_ref(&*screen_out)),
        &squeue,
    ) {
        Ok(s) => s,
        Err(e) => {
            println!("{e}\n{SCREEN_STEPS}");
            return 2;
        }
    };

    // ---- sample RSS each second; a mid-run `top` for helper processes (replayd, WindowServer...)
    let rss = Arc::new(Mutex::new(Vec::<f64>::new()));
    let done = Arc::new(AtomicBool::new(false));
    {
        let (rss, done) = (rss.clone(), done.clone());
        std::thread::spawn(move || {
            while !done.load(Ordering::Relaxed) {
                if let Some(m) = rss_mb_now() {
                    rss.lock().unwrap().push(m);
                }
                std::thread::sleep(Duration::from_secs(1));
            }
        });
    }
    let top = Arc::new(Mutex::new(String::new()));
    {
        let top = top.clone();
        let delay = Duration::from_secs_f64((o.secs / 4.0).max(1.0));
        std::thread::spawn(move || {
            std::thread::sleep(delay);
            if let Ok(out) = Command::new("top")
                .args(["-l", "2", "-s", "3", "-n", "10", "-o", "cpu", "-stats", "pid,command,cpu,mem"])
                .output()
            {
                let txt = String::from_utf8_lossy(&out.stdout).to_string();
                if let Some(idx) = txt.rfind("PID") {
                    *top.lock().unwrap() = txt[idx..].to_string();
                }
            }
        });
    }

    std::thread::sleep(Duration::from_secs_f64(o.secs));

    // ---- stop
    let wall = wall0.elapsed().as_secs_f64();
    let cpu = cpu_secs() - cpu0;
    done.store(true, Ordering::Relaxed);
    stop_stream(&stream);
    screen_track.stop.store(true, Ordering::SeqCst);
    if let Some((_, _, track, sess)) = &cam_parts {
        track.stop.store(true, Ordering::SeqCst);
        unsafe { sess.stopRunning() };
    }
    let mut ok = true;
    if let Err(e) = finish_writer(&sw, &sinput) {
        println!("screen writer finish: {e}");
        ok = false;
    }
    if let Some((cw, cinput, _, _)) = &cam_parts {
        if let Err(e) = finish_writer(cw, cinput) {
            println!("camera writer finish: {e}");
            ok = false;
        }
    }
    drop(cam_out);

    // ---- report
    let rs = report_track(&screen_track, &screen_path, wall);
    println!("\n================ S6 results ================");
    println!("codec {} {}x{} @ {} fps, {} Mbps target, 2 s fragments, bubble={} exclude={}", o.codec, w, h, o.fps, mbps, o.bubble, o.exclude);
    print_track(&rs);
    let rc = cam_parts.as_ref().map(|(_, _, t, _)| report_track(t, &cam_path, wall));
    if let Some(r) = &rc {
        print_track(r);
    }
    let rssv = rss.lock().unwrap().clone();
    println!("--- process");
    println!(
        "  CPU (this process, incl. in-process encoder glue + camera): {:.1}% of one core = {:.1}% of the machine ({} cores) over {:.1} s",
        100.0 * cpu / wall,
        100.0 * cpu / wall / ncpu(),
        ncpu() as u32,
        wall
    );
    println!(
        "  memory: RSS max {:.0} MB (ru_maxrss {:.0} MB), final {:.0} MB",
        rssv.iter().cloned().fold(0.0, f64::max),
        max_rss_mb(),
        rssv.last().copied().unwrap_or(0.0)
    );
    let toptxt = top.lock().unwrap().clone();
    if !toptxt.is_empty() {
        println!("  system top (mid-run, %CPU is per core; VideoToolbox/replayd/WindowServer cost shows here):");
        for l in toptxt.lines().take(11) {
            println!("    {l}");
        }
    }
    let mut offset_line = String::new();
    if let Some(r) = &rc {
        let d_ticks = r.first_pts_ticks - rs.first_pts_ticks;
        offset_line = format!(
            "camera first PTS minus screen first PTS: {} ticks = {:.1} ms (start order only; both stamped on the host clock)",
            d_ticks,
            d_ticks as f64 * 1000.0 / TICKS_PER_SEC as f64
        );
        println!("--- sync\n  {offset_line}");
        println!(
            "  camera delivery latency (median host-now minus PTS): {:.1} ms; screen: {:.1} ms; difference {:.1} ms",
            r.lag_median,
            rs.lag_median,
            r.lag_median - rs.lag_median
        );
    }

    // ---- inspect output files
    println!("--- segments (AVAssetWriter delegate, preferred interval 2 s)");
    print_segments("screen", &sbundle);
    if let Some(b) = &cam_bundle {
        print_segments("camera", b);
    }
    println!("--- file inspection");
    inspect(&screen_path);
    if rc.is_some() {
        inspect(&cam_path);
    }

    // ---- bubble check on the recorded file
    let mut bubble_line = String::from("not checked");
    if o.bubble == "marker" && win_no != 0 {
        let sc = rs.dims.0 as f64 / nw.max(1) as f64 * scale as f64; // px per point in the recorded frame
        let (x, y, bw, bh) = (BUBBLE.origin.x, BUBBLE.origin.y, BUBBLE.size.width, BUBBLE.size.height);
        let inset = bw * 0.2;
        let (cx, cy) = ((x + inset) * sc, (scr_h - y - bh + inset) * sc);
        let (cw_, ch_) = ((bw - 2.0 * inset) * sc, (bh - 2.0 * inset) * sc);
        let at = o.secs / 2.0;
        if let Some(frac) = marker_fraction_from_file(&screen_path, at, cx, cy, cw_, ch_) {
            let hidden = frac < 0.01;
            bubble_line = format!(
                "recorded frame at {at:.0} s, bubble area magenta {:.1}% => bubble {} in the recording (exclude={})",
                frac * 100.0,
                if hidden { "ABSENT" } else { "PRESENT" },
                o.exclude
            );
        } else {
            bubble_line = "ffmpeg frame extraction failed".into();
        }
    }
    println!("--- bubble: {bubble_line}");

    // ---- verdict
    let late_ok = rs.late_pct <= 1.0;
    let lag_ok = rs.lag_final_rel.abs() < 2.0 * 1000.0 / o.fps as f64;
    let fps_ok = rs.fps >= o.fps as f64 * 0.98;
    println!("--- verdict");
    println!("  late frames <= 1%      : {} ({:.2}%)", yn(late_ok), rs.late_pct);
    println!("  no growing lag         : {} (final {:+.1} ms)", yn(lag_ok), rs.lag_final_rel);
    println!("  achieved >= 98% of fps : {} ({:.1})", yn(fps_ok), rs.fps);
    if let Some(r) = &rc {
        println!("  camera late <= 1%      : {} ({:.2}%)", yn(r.late_pct <= 1.0), r.late_pct);
    }
    let pass = ok && late_ok && lag_ok && fps_ok && rc.as_ref().map(|r| r.late_pct <= 1.0).unwrap_or(true);
    println!("  S6 run: {}", if pass { "PASS" } else { "FAIL" });

    let json = format!(
        "{{\"codec\":\"{}\",\"w\":{},\"h\":{},\"fps\":{},\"mbps_target\":{},\"cpu_core_pct\":{:.2},\"rss_max_mb\":{:.1},\"screen\":{},\"camera\":{},\"offset\":\"{}\",\"bubble\":\"{}\",\"pass\":{}}}",
        o.codec, w, h, o.fps, mbps, 100.0 * cpu / wall, rssv.iter().cloned().fold(0.0, f64::max),
        json_track(&rs),
        rc.as_ref().map(json_track).unwrap_or("null".into()),
        offset_line, bubble_line, pass
    );
    let jp = o.out.join(format!("report-{}-{}p{}.json", o.codec, h, o.fps));
    let _ = std::fs::write(&jp, json);
    println!("report written to {}", jp.display());
    if pass { 0 } else { 1 }
}

fn print_segments(name: &str, b: &WriterBundle) {
    let Some(sink) = &b.sink else {
        println!("  {name}: not in segment mode");
        return;
    };
    let st = sink.ivars().st.lock().unwrap();
    if st.segs.is_empty() {
        println!("  {name}: no media segments received");
        return;
    }
    let mut d: Vec<f64> = st.segs.iter().map(|x| x.1).collect();
    d.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let total: usize = st.segs.iter().map(|x| x.2).sum();
    println!(
        "  {name}: init segment {} B, {} media segments, duration median {:.2} s min {:.2} s max {:.2} s, first PTS {:.3} s, {:.1} MB",
        st.init_bytes, st.segs.len(), d[d.len() / 2], d[0], d[d.len() - 1], st.segs[0].0, total as f64 / 1e6
    );
}

unsafe fn build_camera(use_720: bool) -> Result<Retained<AVCaptureSession>, String> {
    let dev = AVCaptureDevice::defaultDeviceWithMediaType(AVMediaTypeVideo.unwrap())
        .ok_or("no camera found")?;
    println!("camera: {}", dev.localizedName());
    let input = AVCaptureDeviceInput::deviceInputWithDevice_error(&dev)
        .map_err(|e| format!("camera input: {}", e.localizedDescription()))?;
    let session = AVCaptureSession::new();
    session.beginConfiguration();
    let preset = if use_720 { AVCaptureSessionPreset1280x720 } else { AVCaptureSessionPreset1920x1080 };
    {
        let p = preset;
        if session.canSetSessionPreset(p) {
            session.setSessionPreset(p);
        } else {
            println!("camera does not support the {} preset", if use_720 { "720p" } else { "1080p" });
        }
    }
    if !session.canAddInput(&input) {
        return Err("cannot add camera input".into());
    }
    session.addInput(&input);
    session.commitConfiguration();
    Ok(session)
}

unsafe fn attach_camera_output(
    session: &AVCaptureSession,
    delegate: &CameraOutput,
    queue: &DispatchRetained<DispatchQueue>,
) {
    let out = AVCaptureVideoDataOutput::new();
    out.setAlwaysDiscardsLateVideoFrames(true);
    out.setSampleBufferDelegate_queue(Some(ProtocolObject::from_ref(delegate)), Some(queue));
    session.beginConfiguration();
    if session.canAddOutput(&out) {
        session.addOutput(&out);
    }
    session.commitConfiguration();
    session.startRunning();
}

// ---------------------------------------------------------------------------------------
// Output inspection: MP4 box layout, ffprobe, decode check

fn read_boxes(path: &Path) -> Option<Vec<(String, u64, u64)>> {
    let mut f = std::fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    let mut pos = 0u64;
    let mut v = Vec::new();
    while pos + 8 <= len {
        f.seek(SeekFrom::Start(pos)).ok()?;
        let mut h = [0u8; 8];
        f.read_exact(&mut h).ok()?;
        let mut size = u32::from_be_bytes([h[0], h[1], h[2], h[3]]) as u64;
        let ty = String::from_utf8_lossy(&h[4..8]).to_string();
        if size == 1 {
            let mut b = [0u8; 8];
            f.read_exact(&mut b).ok()?;
            size = u64::from_be_bytes(b);
        } else if size == 0 {
            size = len - pos;
        }
        if size < 8 {
            break;
        }
        v.push((ty, pos, size));
        pos += size;
    }
    Some(v)
}

fn find_sub(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

fn inspect(path: &Path) {
    println!("file {}", path.display());
    let Some(boxes) = read_boxes(path) else {
        println!("  cannot read file");
        return;
    };
    let moov_idx = boxes.iter().position(|b| b.0 == "moov");
    let first_moof = boxes.iter().position(|b| b.0 == "moof");
    let moofs: Vec<_> = boxes.iter().filter(|b| b.0 == "moof").collect();
    println!(
        "  top-level boxes: {} (moov {} first moof; {} moof fragments)",
        boxes.len(),
        match (moov_idx, first_moof) {
            (Some(m), Some(f)) if m < f => "before",
            (Some(_), Some(_)) => "AFTER",
            (Some(_), None) => "present, no",
            _ => "missing,",
        },
        moofs.len()
    );
    // tfdt of each moof (decode time of its first sample, in the track timescale) -> intervals.
    let mut f = std::fs::File::open(path).unwrap();
    let mut tfdts: Vec<(u64, u64)> = Vec::new(); // (track-order, time) - keep first traf only
    for (_, pos, size) in &moofs {
        let mut buf = vec![0u8; (*size as usize).min(1 << 20)];
        f.seek(SeekFrom::Start(*pos)).unwrap();
        f.read_exact(&mut buf).unwrap();
        if let Some(i) = find_sub(&buf, b"tfdt") {
            let ver = buf[i + 4];
            let t = if ver == 1 {
                u64::from_be_bytes(buf[i + 8..i + 16].try_into().unwrap())
            } else {
                u32::from_be_bytes(buf[i + 8..i + 12].try_into().unwrap()) as u64
            };
            tfdts.push((*pos, t));
        }
    }
    // mdhd timescale from moov
    let mut timescale = TICKS_PER_SEC as u64;
    if let Some(mi) = moov_idx {
        let (_, pos, size) = &boxes[mi];
        let mut buf = vec![0u8; (*size as usize).min(1 << 22)];
        f.seek(SeekFrom::Start(*pos)).unwrap();
        f.read_exact(&mut buf).unwrap();
        if let Some(i) = find_sub(&buf, b"mdhd") {
            if buf[i + 4] == 0 {
                timescale = u32::from_be_bytes(buf[i + 16..i + 20].try_into().unwrap()) as u64;
            }
        }
    }
    if tfdts.len() >= 2 {
        let mut d: Vec<f64> = tfdts.windows(2).map(|w| (w[1].1 as f64 - w[0].1 as f64) / timescale as f64).collect();
        d.sort_by(|a, b| a.partial_cmp(b).unwrap());
        println!(
            "  fragment spacing (tfdt, track timescale {}): median {:.2} s, min {:.2} s, max {:.2} s over {} fragments",
            timescale, d[d.len() / 2], d[0], d[d.len() - 1], d.len() + 1
        );
    }
    // ffprobe
    if let Ok(out) = Command::new("ffprobe")
        .args([
            "-v", "error", "-select_streams", "v:0", "-show_entries",
            "stream=codec_name,codec_tag_string,profile,width,height,pix_fmt,avg_frame_rate,start_time,duration,bit_rate,time_base",
            "-of", "default=nw=1",
        ])
        .arg(path)
        .output()
    {
        let t = String::from_utf8_lossy(&out.stdout);
        let t: Vec<&str> = t.lines().collect();
        println!("  ffprobe: {}", t.join(", "));
    } else {
        println!("  ffprobe not found; skipped");
    }
    // full decode check
    if let Ok(out) = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(path)
        .args(["-f", "null", "-"])
        .output()
    {
        let err = String::from_utf8_lossy(&out.stderr);
        if err.trim().is_empty() {
            println!("  full decode with ffmpeg: no errors");
        } else {
            println!("  full decode with ffmpeg: ERRORS: {}", err.lines().take(3).collect::<Vec<_>>().join(" | "));
        }
    }
}

fn marker_fraction_from_file(path: &Path, at_s: f64, x: f64, y: f64, w: f64, h: f64) -> Option<f64> {
    let (x, y, w, h) = (x as i64 & !1, y as i64 & !1, (w as i64 & !1).max(2), (h as i64 & !1).max(2));
    let out = Command::new("ffmpeg")
        .args(["-v", "error", "-ss", &format!("{at_s}"), "-i"])
        .arg(path)
        .args(["-vf", &format!("crop={w}:{h}:{x}:{y}"), "-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
        .output()
        .ok()?;
    let px = out.stdout;
    if px.len() < 3 {
        return None;
    }
    let n = px.len() / 3;
    // Compressed + converted from 4:2:0, so use wide tolerances.
    let hit = px.chunks_exact(3).filter(|p| p[0] > 190 && p[1] < 80 && p[2] > 190).count();
    Some(hit as f64 / n as f64)
}
