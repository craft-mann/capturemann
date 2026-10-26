//! H.264 camera track via `AVAssetWriter` with the CMAF segment delegate (S6: the plain
//! `movieFragmentInterval` does not fragment). B-frames off (frame reordering breaks the segment
//! writer, S6), 2 s segments, media timescale 120,000, file time (session time minus pause gaps) starts at the session origin.

use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::ptr::NonNull;
use std::sync::mpsc;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use block2::RcBlock;
use cm_core::{CmError, MediaTime, TICKS_PER_SECOND};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{define_class, msg_send, AnyThread, DefinedClass};
use objc2_av_foundation::{
    AVAssetSegmentReport, AVAssetSegmentType, AVAssetWriter, AVAssetWriterDelegate,
    AVAssetWriterInput, AVFileTypeProfileMPEG4CMAFCompliant, AVMediaTypeVideo,
};
use objc2_core_foundation::CFRetained;
use objc2_core_media::{CMSampleBuffer, CMSampleTimingInfo, CMTime, CMTimeFlags};
use objc2_foundation::{NSData, NSDictionary, NSNumber, NSObject, NSObjectProtocol, NSString};
use objc2_uniform_type_identifiers::UTTypeMPEG4Movie;

use super::{cm_ns, SendBox};
use crate::types::CameraFrame;
use crate::writer::{
    AppendOutcome, FragmentInfo, FragmentWriter, WriterFactory, WriterReport, WriterSpec,
    FRAGMENT_SECONDS,
};

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

fn cm(value: i64, timescale: i32) -> CMTime {
    CMTime {
        value,
        timescale,
        flags: CMTimeFlags::Valid,
        epoch: 0,
    }
}

fn ticks_of(t: CMTime) -> i64 {
    (i128::from(cm_ns(t)) * i128::from(TICKS_PER_SECOND) / 1_000_000_000) as i64
}

#[derive(Default)]
struct SegState {
    file: Option<File>,
    write_error: Option<String>,
    fragments: Vec<FragmentInfo>,
    bytes: u64,
}

struct SegIvars {
    st: Arc<Mutex<SegState>>,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements; the only ivar is a plain Rust struct.
    #[unsafe(super(NSObject))]
    #[name = "CMCameraSegmentSink"]
    #[ivars = SegIvars]
    struct SegmentSink;

    // SAFETY: `SegmentSink` is a direct NSObject subclass, so it satisfies NSObjectProtocol.
    unsafe impl NSObjectProtocol for SegmentSink {}

    // SAFETY: the method below has exactly the types of the `assetWriter:didOutputSegmentData:
    // segmentType:segmentReport:` selector; it only copies the NSData valid for the call.
    unsafe impl AVAssetWriterDelegate for SegmentSink {
        #[unsafe(method(assetWriter:didOutputSegmentData:segmentType:segmentReport:))]
        unsafe fn did_output(
            &self,
            _writer: &AVAssetWriter,
            data: &NSData,
            ty: AVAssetSegmentType,
            report: Option<&AVAssetSegmentReport>,
        ) {
            // A panic must not unwind into AVFoundation (abort/UB): catch it, drop the segment.
            cm_core::guard::guard_unit("cm-camera did_output", || {
                let bytes = data.to_vec();
                let mut st = lock(&self.ivars().st);
                if let Some(f) = st.file.as_mut() {
                    // `sync_data` per segment (~2 s): a power loss loses at most the segment in
                    // flight, like the screen writer (R3-12).
                    if let Err(e) = f.write_all(&bytes).and_then(|()| f.sync_data()) {
                        st.write_error.get_or_insert(e.to_string());
                    }
                }
                st.bytes += bytes.len() as u64;
                if ty != AVAssetSegmentType::Initialization {
                    // SAFETY: the report and its track reports are valid for the delegate call.
                    let times = unsafe {
                        report
                            .and_then(|r| r.trackReports().iter().next())
                            .map(|t| (t.earliestPresentationTimeStamp(), t.duration()))
                    };
                    if let Some((start, dur)) = times {
                        st.fragments.push(FragmentInfo {
                            start: MediaTime::from_ticks(ticks_of(start)),
                            duration: MediaTime::from_ticks(ticks_of(dur)),
                            bytes: bytes.len() as u64,
                        });
                    }
                }
            });
        }
    }
);

impl SegmentSink {
    fn new(st: Arc<Mutex<SegState>>) -> Retained<Self> {
        let this = Self::alloc().set_ivars(SegIvars { st });
        // SAFETY: standard NSObject init on a freshly allocated instance with ivars set.
        unsafe { msg_send![super(this), init] }
    }
}

pub(super) struct AvWriterFactory;

impl WriterFactory for AvWriterFactory {
    fn create(&self, path: &Path, spec: &WriterSpec) -> Result<Box<dyn FragmentWriter>, CmError> {
        Ok(Box::new(AvWriter::new(path, spec)?))
    }
}

struct AvWriter {
    writer: SendBox<Retained<AVAssetWriter>>,
    input: SendBox<Retained<AVAssetWriterInput>>,
    // The writer's delegate is weak: keep the sink alive.
    _sink: SendBox<Retained<SegmentSink>>,
    state: Arc<Mutex<SegState>>,
    failed: Option<String>,
    finished: bool,
}

fn s(x: &str) -> Retained<NSString> {
    NSString::from_str(x)
}

impl AvWriter {
    fn new(path: &Path, spec: &WriterSpec) -> Result<Self, CmError> {
        let file = File::create(path)
            .map_err(|e| CmError::internal(format!("cannot create {}: {e}", path.display())))?;
        let state = Arc::new(Mutex::new(SegState {
            file: Some(file),
            ..SegState::default()
        }));
        let sink = SegmentSink::new(state.clone());
        let scale = TICKS_PER_SECOND as i32;
        // SAFETY: Objective-C calls on freshly created objects; the writer is configured fully
        // before `startWriting`.
        unsafe {
            let w = AVAssetWriter::initWithContentType(AVAssetWriter::alloc(), UTTypeMPEG4Movie);
            w.setOutputFileTypeProfile(AVFileTypeProfileMPEG4CMAFCompliant);
            w.setPreferredOutputSegmentInterval(cm(i64::from(FRAGMENT_SECONDS), 1));
            w.setInitialSegmentStartTime(cm(0, scale));
            w.setDelegate(Some(ProtocolObject::from_ref(&*sink)));

            let (k_br, k_fps, k_kf, k_pl, k_ro) = (
                s("AverageBitRate"),
                s("ExpectedFrameRate"),
                s("MaxKeyFrameIntervalDuration"),
                s("ProfileLevel"),
                s("AllowFrameReordering"),
            );
            let (v_br, v_fps, v_kf, v_pl, v_ro) = (
                NSNumber::new_i64(i64::from(spec.bitrate_bps)),
                NSNumber::new_i64(i64::from(spec.fps)),
                NSNumber::new_f64(f64::from(FRAGMENT_SECONDS)),
                s("H264_High_AutoLevel"),
                NSNumber::new_bool(false), // B-frames off: reordering breaks the CMAF writer (S6)
            );
            let props: Retained<NSDictionary<NSString, AnyObject>> = NSDictionary::from_slices(
                &[&*k_br, &*k_fps, &*k_kf, &*k_pl, &*k_ro],
                &[&**v_br, &**v_fps, &**v_kf, &**v_pl, &**v_ro],
            );
            let (k_c, k_w, k_h, k_p, k_s) = (
                s("AVVideoCodecKey"),
                s("AVVideoWidthKey"),
                s("AVVideoHeightKey"),
                s("AVVideoCompressionPropertiesKey"),
                s("AVVideoScalingModeKey"),
            );
            let (v_c, v_w, v_h, v_s) = (
                s("avc1"),
                NSNumber::new_i64(i64::from(spec.width)),
                NSNumber::new_i64(i64::from(spec.height)),
                s("AVVideoScalingModeResizeAspect"),
            );
            let settings: Retained<NSDictionary<NSString, AnyObject>> = NSDictionary::from_slices(
                &[&*k_c, &*k_w, &*k_h, &*k_p, &*k_s],
                &[&**v_c, &**v_w, &**v_h, &**props, &**v_s],
            );
            let media = AVMediaTypeVideo
                .ok_or_else(|| CmError::internal("AVMediaTypeVideo is unavailable"))?;
            let input = AVAssetWriterInput::assetWriterInputWithMediaType_outputSettings(
                media,
                Some(&settings),
            );
            input.setExpectsMediaDataInRealTime(true);
            input.setMediaTimeScale(scale);
            if !w.canAddInput(&input) {
                return Err(CmError::platform_rejected(
                    format!("the H.264 encoder rejected {}x{}", spec.width, spec.height),
                    None,
                ));
            }
            w.addInput(&input);
            if !w.startWriting() {
                let e = w.error().map(|e| e.localizedDescription().to_string());
                return Err(CmError::internal(format!("startWriting failed: {e:?}")));
            }
            w.startSessionAtSourceTime(cm(0, scale));
            Ok(AvWriter {
                writer: SendBox(w),
                input: SendBox(input),
                _sink: SendBox(sink),
                state,
                failed: None,
                finished: false,
            })
        }
    }

    /// A new sample buffer around the same pixel buffer with session-time PTS and a duration.
    /// The CMAF writer does not rebase to the session start (S6), so times are absolute.
    fn retimed(
        sb: &CMSampleBuffer,
        pts: MediaTime,
        duration: MediaTime,
    ) -> Option<CFRetained<CMSampleBuffer>> {
        let scale = TICKS_PER_SECOND as i32;
        let mut info = CMSampleTimingInfo {
            duration: cm(duration.ticks(), scale),
            presentationTimeStamp: cm(pts.ticks(), scale),
            decodeTimeStamp: CMTime {
                value: 0,
                timescale: 0,
                flags: CMTimeFlags::empty(),
                epoch: 0,
            }, // invalid: no reordering, DTS = PTS
        };
        let mut out: *mut CMSampleBuffer = std::ptr::null_mut();
        // SAFETY: `sb` is valid; `info` and `out` outlive the call; on success `out` holds a +1
        // CMSampleBuffer which `from_raw` takes ownership of.
        unsafe {
            let st = CMSampleBuffer::create_copy_with_new_timing(
                None,
                sb,
                1,
                &raw mut info,
                NonNull::new(&raw mut out)?,
            );
            if st != 0 {
                return None;
            }
            NonNull::new(out).map(|p| CFRetained::from_raw(p))
        }
    }

    fn writer_error(&self) -> String {
        // SAFETY: property reads on a valid writer.
        unsafe {
            let st = self.writer.0.status().0;
            let err = self
                .writer
                .0
                .error()
                .map(|e| format!("{} ({})", e.localizedDescription(), e.code()));
            format!("writer status {st}: {}", err.unwrap_or_default())
        }
    }
}

impl FragmentWriter for AvWriter {
    fn append(
        &mut self,
        frame: &CameraFrame,
        pts: MediaTime,
        duration: MediaTime,
    ) -> AppendOutcome {
        if let Some(m) = &self.failed {
            return AppendOutcome::Failed(m.clone());
        }
        let Some(native) = frame.native.as_ref() else {
            return AppendOutcome::Failed("frame has no platform sample".into());
        };
        // SAFETY: the input is valid until `finish`.
        if !unsafe { self.input.0.isReadyForMoreMediaData() } {
            return AppendOutcome::NotReady;
        }
        let Some(rebased) = Self::retimed(native.buffer(), pts, duration) else {
            return AppendOutcome::Failed("could not retime the sample buffer".into());
        };
        // SAFETY: `rebased` is a valid sample buffer for the call.
        if unsafe { self.input.0.appendSampleBuffer(&rebased) } {
            if let Some(e) = lock(&self.state).write_error.clone() {
                let m = format!("writing camera.mp4 failed: {e}");
                self.failed = Some(m.clone());
                return AppendOutcome::Failed(m);
            }
            AppendOutcome::Written
        } else {
            let m = self.writer_error();
            self.failed = Some(m.clone());
            AppendOutcome::Failed(m)
        }
    }

    fn flush(&mut self) {
        // `flushSegment` only applies with an indefinite segment interval and must precede a sync
        // sample, so it cannot be used with the 2 s interval. The tail since the last segment is
        // written at `finish`.
    }

    fn finish(&mut self) -> Result<WriterReport, CmError> {
        if !self.finished {
            self.finished = true;
            let (tx, rx) = mpsc::channel();
            let block = RcBlock::new(move || {
                let _ = tx.send(());
            });
            // SAFETY: the input and writer are valid; `markAsFinished` precedes finishing, and
            // the completion block only sends on a channel.
            unsafe {
                if self.failed.is_none() {
                    self.input.0.markAsFinished();
                }
                self.writer.0.finishWritingWithCompletionHandler(&block);
            }
            rx.recv_timeout(Duration::from_secs(30))
                .map_err(|_| CmError::internal("finishing camera.mp4 timed out"))?;
            // SAFETY: property read.
            if unsafe { self.writer.0.status().0 } != 2 && self.failed.is_none() {
                self.failed = Some(self.writer_error());
            }
        }
        let mut st = lock(&self.state);
        st.file = None; // closes the file
        let report = WriterReport {
            fragments: st.fragments.clone(),
            bytes: st.bytes,
        };
        match (&self.failed, report.fragments.is_empty()) {
            // A failed writer that already produced fragments leaves a valid fMP4 prefix.
            (Some(m), true) => Err(CmError::internal(format!("camera.mp4 is empty: {m}"))),
            _ => Ok(report),
        }
    }
}
