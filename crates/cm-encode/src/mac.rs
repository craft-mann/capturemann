//! macOS hardware encoder: `AVAssetWriter` (VideoToolbox) fed with IOSurface-backed
//! `CMSampleBuffer`s straight from ScreenCaptureKit, producing a fragmented MP4 through the
//! writer's segment delegate. Plain `objc2-*` bindings, no Swift bridge.
//!
//! Design decisions proven by spike S6 (docs/SPIKES.md):
//!
//! * `movieFragmentInterval` does nothing for MP4/MOV; the working path is the CMAF profile with a
//!   segment delegate (`preferredOutputSegmentInterval` 2 s), which hands over the init segment
//!   (`ftyp` + `moov`) first and then one segment per ~2 s starting at a sync sample.
//! * B-frames off (`AllowFrameReordering` false) and a 2 s maximum key-frame interval, so every
//!   segment is independently decodable and PTS equals DTS.
//! * Samples are retimed (a new `CMSampleBuffer` around the same pixel buffer) to the caller's
//!   file time in 120,000 ticks/s; the track media timescale is 120,000 as well.
//! * This process never maps the pixels: the frame goes from the capture queue to the encoder as a
//!   GPU surface reference.
//!
//! This is the only module in the crate with `unsafe`. Each block states why it is sound; the
//! recurring reasons are: objc2 bindings are `unsafe` because they cannot prove the receiver and
//! argument types of a message send, but every call here passes correctly typed live objects, and
//! `CMSampleBuffer` / `AVAssetWriter*` objects are thread-safe Core Foundation / AVFoundation
//! objects used from one thread at a time (the encoder owner).

use std::mem::MaybeUninit;
use std::ptr::NonNull;
use std::sync::mpsc;
use std::time::Duration;

use block2::RcBlock;
use cm_core::{CmError, MediaTime};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{define_class, msg_send, AnyThread, DefinedClass};
use objc2_av_foundation::{
    AVAssetSegmentReport, AVAssetSegmentType, AVAssetWriter, AVAssetWriterDelegate,
    AVAssetWriterInput, AVAssetWriterStatus, AVFileTypeProfileMPEG4CMAFCompliant, AVMediaTypeVideo,
};
use objc2_core_foundation::CFRetained;
use objc2_core_media::{
    CMSampleBuffer, CMSampleTimingInfo, CMTime, CMTimeFlags, CMVideoFormatDescription,
    CMVideoFormatDescriptionCreateForImageBuffer,
};
use objc2_core_video::{
    CVPixelBuffer, CVPixelBufferCreate, CVPixelBufferGetBaseAddress, CVPixelBufferGetBytesPerRow,
    CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags, CVPixelBufferUnlockBaseAddress,
};
use objc2_foundation::{NSData, NSDictionary, NSNumber, NSObject, NSObjectProtocol, NSString};
use objc2_uniform_type_identifiers::UTTypeMPEG4Movie;

use crate::encoder::{AppendOutcome, EncoderConfig, VideoEncoder};
use crate::settings::{Codec, FRAGMENT_SECS};
use crate::writer::{FragmentedMp4Writer, WriterSummary};

const TIMESCALE: i32 = 120_000;

fn cm(value: i64, timescale: i32) -> CMTime {
    CMTime {
        value,
        timescale,
        flags: CMTimeFlags::Valid,
        epoch: 0,
    }
}

fn nsstr(x: &str) -> Retained<NSString> {
    NSString::from_str(x)
}

/// A retained screen frame (`CMSampleBuffer` around an IOSurface). Cloning bumps a reference
/// count; no pixels are copied.
#[derive(Clone)]
pub struct MacFrame(CFRetained<CMSampleBuffer>);

// SAFETY: a CMSampleBuffer is an immutable-after-creation, reference-counted Core Foundation
// object that Apple documents as thread-safe to retain, release and read. We only ever read it
// (timing info, handing it to AVAssetWriterInput) and release it.
unsafe impl Send for MacFrame {}
// SAFETY: see above; shared references only read.
unsafe impl Sync for MacFrame {}

impl MacFrame {
    /// Retains a sample buffer delivered by ScreenCaptureKit.
    pub fn retain(sample: &CMSampleBuffer) -> Self {
        // SAFETY: `sample` is a valid reference, so the pointer is non-null and alive; `retain`
        // takes its own +1 reference.
        MacFrame(unsafe { CFRetained::retain(NonNull::from(sample)) })
    }

    fn retimed(&self, pts: i64, dur: i64) -> Option<CFRetained<CMSampleBuffer>> {
        let mut info = MaybeUninit::<CMSampleTimingInfo>::uninit();
        // SAFETY: `info` is a valid out-pointer for one CMSampleTimingInfo; index 0 exists for a
        // single-sample buffer (SCK delivers one frame per buffer). We only read `info` when the
        // call returned 0 (initialized).
        let mut info = unsafe {
            if self
                .0
                .sample_timing_info(0, NonNull::new(info.as_mut_ptr())?)
                != 0
            {
                return None;
            }
            info.assume_init()
        };
        info.presentationTimeStamp = cm(pts, TIMESCALE);
        // ScreenCaptureKit leaves the duration invalid and the segmenter needs one.
        info.duration = cm(dur, TIMESCALE);
        if info.decodeTimeStamp.flags.contains(CMTimeFlags::Valid) {
            info.decodeTimeStamp = cm(pts, TIMESCALE);
        }
        let mut out: *mut CMSampleBuffer = std::ptr::null_mut();
        // SAFETY: all pointers are valid for the call; on success (0) `out` holds a +1 retained
        // sample buffer that `CFRetained::from_raw` takes ownership of.
        unsafe {
            if CMSampleBuffer::create_copy_with_new_timing(
                None,
                &self.0,
                1,
                &info,
                NonNull::new(&mut out)?,
            ) != 0
            {
                return None;
            }
            NonNull::new(out).map(|p| CFRetained::from_raw(p))
        }
    }
}

impl MacFrame {
    /// A synthetic BGRA frame (moving gradient, different for every `n`) as a CPU-backed pixel
    /// buffer. Lets tests and self-checks drive the real hardware encoder without a display or
    /// Screen Recording permission. The timing on the buffer is a placeholder; the encoder
    /// retimes every frame.
    pub fn test_pattern(width: usize, height: usize, n: u32) -> Result<MacFrame, CmError> {
        const BGRA: u32 = 0x4247_5241;
        let fail = |what: &str, rc: i32| CmError::internal(format!("{what} failed ({rc})"));
        let mut pb: *mut CVPixelBuffer = std::ptr::null_mut();
        // SAFETY: `pb` is a valid out-pointer; on success it holds a +1 pixel buffer that
        // `CFRetained::from_raw` owns from here on.
        let pixels = unsafe {
            let rc = CVPixelBufferCreate(None, width, height, BGRA, None, NonNull::from(&mut pb));
            let ptr = NonNull::new(pb).ok_or_else(|| fail("CVPixelBufferCreate", rc))?;
            if rc != 0 {
                return Err(fail("CVPixelBufferCreate", rc));
            }
            CFRetained::from_raw(ptr)
        };
        // SAFETY: the buffer is locked for writing while we fill rows; `stride * y + width * 4`
        // stays inside the allocation (CVPixelBuffer rows are at least `width * 4` bytes and
        // there are `height` of them); unlocked before use by the encoder.
        unsafe {
            let flags = CVPixelBufferLockFlags(0);
            if CVPixelBufferLockBaseAddress(&pixels, flags) != 0 {
                return Err(fail("CVPixelBufferLockBaseAddress", -1));
            }
            let base = CVPixelBufferGetBaseAddress(&pixels).cast::<u8>();
            let stride = CVPixelBufferGetBytesPerRow(&pixels);
            if base.is_null() {
                CVPixelBufferUnlockBaseAddress(&pixels, flags);
                return Err(fail("CVPixelBufferGetBaseAddress", -1));
            }
            for y in 0..height {
                let row = std::slice::from_raw_parts_mut(base.add(y * stride), width * 4);
                for (x, px) in row.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                    px[0] = (x as u32).wrapping_add(n.wrapping_mul(8)) as u8;
                    px[1] = (y as u32 / 2) as u8;
                    px[2] = n.wrapping_mul(3) as u8;
                    px[3] = 255;
                }
            }
            CVPixelBufferUnlockBaseAddress(&pixels, flags);
        }
        let mut fd: *const CMVideoFormatDescription = std::ptr::null();
        // SAFETY: valid out-pointer; success gives a +1 format description we own.
        let fmt = unsafe {
            let rc =
                CMVideoFormatDescriptionCreateForImageBuffer(None, &pixels, NonNull::from(&mut fd));
            let ptr = NonNull::new(fd.cast_mut()).ok_or_else(|| fail("format description", rc))?;
            if rc != 0 {
                return Err(fail("format description", rc));
            }
            CFRetained::from_raw(ptr)
        };
        let timing = CMSampleTimingInfo {
            duration: cm(4000, TIMESCALE),
            presentationTimeStamp: cm(i64::from(n) * 4000, TIMESCALE),
            decodeTimeStamp: CMTime {
                value: 0,
                timescale: 0,
                flags: CMTimeFlags::empty(),
                epoch: 0,
            },
        };
        let mut sb: *mut CMSampleBuffer = std::ptr::null_mut();
        // SAFETY: all references are live for the call; success gives a +1 sample buffer we own.
        let sample = unsafe {
            let rc = CMSampleBuffer::create_ready_with_image_buffer(
                None,
                &pixels,
                &fmt,
                NonNull::from(&timing),
                NonNull::from(&mut sb),
            );
            let ptr = NonNull::new(sb).ok_or_else(|| fail("CMSampleBufferCreate", rc))?;
            if rc != 0 {
                return Err(fail("CMSampleBufferCreate", rc));
            }
            CFRetained::from_raw(ptr)
        };
        Ok(MacFrame(sample))
    }
}

struct SinkIvars {
    writer: FragmentedMp4Writer,
}

define_class!(
    /// Receives the init segment and then one segment per ~2 s from `AVAssetWriter`.
    #[unsafe(super(NSObject))]
    #[name = "CmEncodeSegmentSink"]
    #[ivars = SinkIvars]
    struct SegmentSink;

    unsafe impl NSObjectProtocol for SegmentSink {}

    unsafe impl AVAssetWriterDelegate for SegmentSink {
        #[unsafe(method(assetWriter:didOutputSegmentData:segmentType:segmentReport:))]
        unsafe fn did_output(
            &self,
            _writer: &AVAssetWriter,
            data: &NSData,
            ty: AVAssetSegmentType,
            _report: Option<&AVAssetSegmentReport>,
        ) {
            let bytes = data.to_vec();
            let w = &self.ivars().writer;
            if ty == AVAssetSegmentType::Initialization {
                w.push_init(bytes);
            } else {
                w.push_segment(bytes);
            }
        }
    }
);

impl SegmentSink {
    fn new(writer: FragmentedMp4Writer) -> Retained<Self> {
        let this = Self::alloc().set_ivars(SinkIvars { writer });
        // SAFETY: `init` on a freshly allocated NSObject subclass with its ivars set.
        unsafe { msg_send![super(this), init] }
    }
}

/// The macOS hardware encoder (HEVC or H.264 through VideoToolbox).
pub struct HardwareEncoder {
    writer: Retained<AVAssetWriter>,
    input: Retained<AVAssetWriterInput>,
    /// The writer's delegate is a weak reference; this keeps the sink alive.
    _sink: Retained<SegmentSink>,
    file: FragmentedMp4Writer,
    appended: u64,
    finished: Option<WriterSummary>,
}

// SAFETY: the Objective-C objects are only used through `&mut self` methods, i.e. from one thread
// at a time (the recorder serializes access with a mutex). AVAssetWriter and its inputs may be
// used from any thread as long as calls are not concurrent.
unsafe impl Send for HardwareEncoder {}

fn writer_error(w: &AVAssetWriter) -> String {
    // SAFETY: `error` / `localizedDescription` are plain getters on live objects.
    unsafe {
        w.error().map_or_else(
            || "unknown error".into(),
            |e| e.localizedDescription().to_string(),
        )
    }
}

impl HardwareEncoder {
    /// Creates the file and the writer; frames can be appended right away.
    pub fn create(cfg: &EncoderConfig) -> Result<Self, CmError> {
        let plan = &cfg.plan;
        let file = FragmentedMp4Writer::create(&cfg.path)?;
        // SAFETY: standard AVAssetWriter set-up with correctly typed arguments, on objects this
        // function owns. The delegate is held by `_sink` for the writer's whole life.
        let (writer, sink, input) = unsafe {
            let writer =
                AVAssetWriter::initWithContentType(AVAssetWriter::alloc(), UTTypeMPEG4Movie);
            let sink = SegmentSink::new(file.clone());
            writer.setOutputFileTypeProfile(AVFileTypeProfileMPEG4CMAFCompliant);
            writer.setPreferredOutputSegmentInterval(cm(i64::from(FRAGMENT_SECS), 1));
            writer.setInitialSegmentStartTime(cm(0, TIMESCALE));
            writer.setDelegate(Some(ProtocolObject::from_ref(&*sink)));

            let (codec, profile) = match plan.codec {
                Codec::Hevc => ("hvc1", "HEVC_Main_AutoLevel"),
                Codec::H264 => ("avc1", "H264_High_AutoLevel"),
            };
            let keys = [
                nsstr("AverageBitRate"),
                nsstr("ExpectedFrameRate"),
                nsstr("MaxKeyFrameIntervalDuration"),
                nsstr("ProfileLevel"),
                nsstr("AllowFrameReordering"),
            ];
            let bitrate = NSNumber::new_i64(i64::try_from(plan.bitrate_bps).unwrap_or(i64::MAX));
            let fps = NSNumber::new_i64(i64::from(plan.fps));
            let key_interval = NSNumber::new_f64(f64::from(FRAGMENT_SECS));
            let profile = nsstr(profile);
            let reorder = NSNumber::new_bool(false);
            let props: Retained<NSDictionary<NSString, AnyObject>> = NSDictionary::from_slices(
                &[&*keys[0], &*keys[1], &*keys[2], &*keys[3], &*keys[4]],
                &[&**bitrate, &**fps, &**key_interval, &**profile, &**reorder],
            );
            let (k_c, k_w, k_h, k_p) = (
                nsstr("AVVideoCodecKey"),
                nsstr("AVVideoWidthKey"),
                nsstr("AVVideoHeightKey"),
                nsstr("AVVideoCompressionPropertiesKey"),
            );
            let (v_c, v_w, v_h) = (
                nsstr(codec),
                NSNumber::new_i64(i64::from(plan.width)),
                NSNumber::new_i64(i64::from(plan.height)),
            );
            let settings: Retained<NSDictionary<NSString, AnyObject>> = NSDictionary::from_slices(
                &[&*k_c, &*k_w, &*k_h, &*k_p],
                &[&**v_c, &**v_w, &**v_h, &**props],
            );
            let Some(video) = AVMediaTypeVideo else {
                return Err(CmError::internal("AVMediaTypeVideo is unavailable"));
            };
            let input = AVAssetWriterInput::assetWriterInputWithMediaType_outputSettings(
                video,
                Some(&settings),
            );
            input.setExpectsMediaDataInRealTime(true);
            input.setMediaTimeScale(TIMESCALE);
            (writer, sink, input)
        };
        // SAFETY: plain AVAssetWriter state transitions on the objects created above.
        unsafe {
            if !writer.canAddInput(&input) {
                let _ = file.finish();
                // No software fallback in v0.1 (REC-2): the recording fails before it starts.
                return Err(crate::encoder::hardware_encoder_unavailable());
            }
            writer.addInput(&input);
            if !writer.startWriting() {
                let _ = writer_error(&writer);
                let _ = file.finish();
                return Err(crate::encoder::hardware_encoder_unavailable());
            }
            writer.startSessionAtSourceTime(cm(0, TIMESCALE));
        }
        Ok(HardwareEncoder {
            writer,
            input,
            _sink: sink,
            file,
            appended: 0,
            finished: None,
        })
    }
}

impl VideoEncoder for HardwareEncoder {
    type Frame = MacFrame;

    fn append(
        &mut self,
        frame: &MacFrame,
        pts: MediaTime,
        duration: MediaTime,
    ) -> Result<AppendOutcome, CmError> {
        self.file.check()?;
        // SAFETY: getter on a live input.
        if !unsafe { self.input.isReadyForMoreMediaData() } {
            return Ok(AppendOutcome::NotReady);
        }
        let retimed = frame
            .retimed(pts.ticks(), duration.ticks())
            .ok_or_else(|| CmError::internal("could not retime a screen frame"))?;
        // SAFETY: appending a valid sample buffer to an input that reported ready.
        if unsafe { self.input.appendSampleBuffer(&retimed) } {
            self.appended += 1;
            return Ok(AppendOutcome::Written);
        }
        // The disk error, if any, is the root cause; report it first.
        self.file.check()?;
        Err(CmError::internal(format!(
            "The encoder rejected a frame after {} frames: {}",
            self.appended,
            writer_error(&self.writer)
        )))
    }

    fn check(&self) -> Result<(), CmError> {
        self.file.check()
    }

    fn fragments_written(&self) -> usize {
        self.file.fragments_written()
    }

    fn finish(&mut self) -> Result<WriterSummary, CmError> {
        if let Some(s) = &self.finished {
            return Ok(s.clone());
        }
        if self.appended == 0 {
            // SAFETY: cancelling a writer that never received a sample.
            unsafe { self.writer.cancelWriting() };
            let _ = self.file.finish();
            return Err(crate::encoder::no_frames_error());
        }
        let (tx, rx) = mpsc::channel();
        let block = RcBlock::new(move || {
            let _ = tx.send(());
        });
        // SAFETY: standard finish sequence; the completion block is retained by the writer.
        unsafe {
            self.input.markAsFinished();
            self.writer.finishWritingWithCompletionHandler(&block);
        }
        let done = rx.recv_timeout(Duration::from_secs(60)).is_ok();
        // SAFETY: getter on a live writer.
        let status = unsafe { self.writer.status() };
        let summary = self.file.finish();
        // R3-5: whatever reached the disk is a valid file up to its last fragment. A late or
        // failed finish with at least one fragment is a take with a note (`halt`), not a lost one.
        let keep = |mut s: WriterSummary, reason: &'static str, message: String| {
            s.halt
                .get_or_insert(crate::writer::WriterHalt { reason, message });
            s
        };
        if !done {
            return match summary {
                Ok(s) if !s.fragments.is_empty() => {
                    let s = keep(
                        s,
                        "encoder_timeout",
                        "The encoder did not finish in time".to_owned(),
                    );
                    self.finished = Some(s.clone());
                    Ok(s)
                }
                _ => Err(CmError::internal("The encoder did not finish in time")),
            };
        }
        if status != AVAssetWriterStatus::Completed {
            let message = format!(
                "The encoder finished with an error: {}",
                writer_error(&self.writer)
            );
            return match summary {
                Ok(s) if !s.fragments.is_empty() => {
                    let s = keep(s, "encoder_error", message);
                    self.finished = Some(s.clone());
                    Ok(s)
                }
                Ok(_) => Err(CmError::internal(message)),
                Err(e) => Err(e),
            };
        }
        let summary = summary?;
        self.finished = Some(summary.clone());
        Ok(summary)
    }
}
