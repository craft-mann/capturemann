//! System audio through ScreenCaptureKit (macOS 13+): an audio-only use of its own `SCStream`.
//!
//! Why a separate stream instead of attaching to the screen track's `SCStream` in cm-capture:
//! system audio is toggled independently of the screen (and may outlive a screen restart), the
//! recorder keeps the plain `TrackRecorder` interface, and cm-capture needs no audio code or
//! lifetime coupling. The price is a second, tiny stream (16x16 px at 1 fps, frames discarded)
//! because ScreenCaptureKit cannot run without a content filter; its cost is negligible next to
//! the screen stream. If measurements ever show a problem, the same delegate can be attached to
//! the screen stream by adding an `Audio` output there.
//!
//! Audio arrives as Float32, 48 kHz, 2 channels (non-interleaved planes), stamped with the host
//! clock (the same `mach_absolute_time` base as the session clock).

use std::ptr::NonNull;
use std::sync::mpsc;
use std::sync::Mutex;
use std::time::Duration;

use block2::RcBlock;
use cm_core::{CmError, Permission};
use dispatch2::{DispatchQueue, DispatchRetained};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{define_class, msg_send, AnyThread, DefinedClass};
use objc2_core_audio_types::AudioBufferList;
use objc2_core_foundation::CFRetained;
use objc2_core_media::{CMBlockBuffer, CMSampleBuffer, CMTime, CMTimeFlags};
use objc2_foundation::{NSArray, NSError, NSObject, NSObjectProtocol, NSProcessInfo};
use objc2_screen_capture_kit::{
    SCContentFilter, SCDisplay, SCRunningApplication, SCShareableContent, SCStream,
    SCStreamConfiguration, SCStreamDelegate, SCStreamOutput, SCStreamOutputType,
};

use crate::source::{AudioSource, SinkNotifier, SourceFormat, SourceSink};

/// Sample rate and channel count requested from ScreenCaptureKit (AUD-2: 48 kHz stereo).
pub const SCK_RATE: u32 = 48_000;
pub const SCK_CHANNELS: u16 = 2;
const CALL_TIMEOUT: Duration = Duration::from_secs(15);

/// Major version of macOS (13 = Ventura).
pub fn macos_major_version() -> i64 {
    NSProcessInfo::processInfo()
        .operatingSystemVersion()
        .majorVersion as i64
}

struct SendBox<T>(T);
// SAFETY: used only to move retained ScreenCaptureKit objects out of a completion handler into the
// thread that is blocked waiting for them; the objects are not touched concurrently.
unsafe impl<T> Send for SendBox<T> {}

fn cm_ns(t: CMTime) -> Option<u64> {
    if t.timescale <= 0 || !t.flags.contains(CMTimeFlags::Valid) {
        return None;
    }
    let ns = i128::from(t.value) * 1_000_000_000 / i128::from(t.timescale);
    u64::try_from(ns).ok()
}

struct OutputIvars {
    sink: Mutex<Option<SourceSink>>,
    notifier: SinkNotifier,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements; the only ivar is a plain Rust struct.
    #[unsafe(super(NSObject))]
    #[name = "CMAudioSckOutput"]
    #[ivars = OutputIvars]
    struct AudioOutput;

    // SAFETY: `AudioOutput` is a direct NSObject subclass, so it satisfies NSObjectProtocol.
    unsafe impl NSObjectProtocol for AudioOutput {}

    // SAFETY: the method below has exactly the types of the `stream:didOutputSampleBuffer:ofType:`
    // selector; SCK calls it with a valid stream and sample buffer.
    unsafe impl SCStreamOutput for AudioOutput {
        #[unsafe(method(stream:didOutputSampleBuffer:ofType:))]
        unsafe fn stream_did_output(
            &self,
            _stream: &SCStream,
            sb: &CMSampleBuffer,
            ty: SCStreamOutputType,
        ) {
            // A panic must not unwind into ScreenCaptureKit (abort/UB): catch it, drop the chunk.
            cm_core::guard::guard_unit("cm-audio stream_did_output", || {
                // Video frames are requested only because SCK needs a screen output; discard them.
                if ty != SCStreamOutputType::Audio {
                    return;
                }
                // SAFETY: `sb` is a valid sample buffer for the duration of this callback.
                let Some((host_ns, data)) = (unsafe { extract_audio(sb) }) else {
                    return;
                };
                if let Ok(mut guard) = self.ivars().sink.lock() {
                    if let Some(sink) = guard.as_mut() {
                        sink.push(host_ns, &data);
                    }
                }
            });
        }
    }

    // SAFETY: the method below matches the types of the `stream:didStopWithError:` selector; it
    // only reads the NSError SCK passes (valid for the call) and uses a thread-safe notifier.
    unsafe impl SCStreamDelegate for AudioOutput {
        #[unsafe(method(stream:didStopWithError:))]
        unsafe fn stream_did_stop(&self, _stream: &SCStream, error: &NSError) {
            cm_core::guard::guard_unit("cm-audio stream_did_stop", || {
                self.ivars()
                    .notifier
                    .disconnected(error.localizedDescription().to_string());
            });
        }
    }
);

impl AudioOutput {
    fn new(sink: SourceSink) -> Retained<Self> {
        let notifier = sink.notifier();
        let this = Self::alloc().set_ivars(OutputIvars {
            sink: Mutex::new(Some(sink)),
            notifier,
        });
        // SAFETY: plain `init` on a freshly allocated NSObject subclass instance.
        unsafe { msg_send![super(this), init] }
    }
}

/// Copies one audio sample buffer into interleaved f32 and returns its host-clock capture time.
///
/// # Safety
/// `sb` must be a valid audio `CMSampleBuffer`.
unsafe fn extract_audio(sb: &CMSampleBuffer) -> Option<(u64, Vec<f32>)> {
    // SAFETY: `sb` is valid per this function's contract.
    let host_ns = cm_ns(unsafe { sb.presentation_time_stamp() })?;
    let mut needed: usize = 0;
    // SAFETY: size query only: every out-pointer is a local that outlives the call or null, and the
    // list capacity is 0, so CoreMedia writes only `needed`.
    let st = unsafe {
        sb.audio_buffer_list_with_retained_block_buffer(
            &mut needed,
            std::ptr::null_mut(),
            0,
            None,
            None,
            0,
            std::ptr::null_mut(),
        )
    };
    if st != 0 || needed < std::mem::size_of::<AudioBufferList>() {
        return None;
    }
    let mut storage = vec![0u64; needed.div_ceil(8)];
    let list = storage.as_mut_ptr().cast::<AudioBufferList>();
    let mut block: *mut CMBlockBuffer = std::ptr::null_mut();
    // SAFETY: `list` points to `needed` writable bytes (`storage`), 8-byte aligned via Vec<u64> as
    // AudioBufferList requires; `block` is a local out-pointer.
    let st = unsafe {
        sb.audio_buffer_list_with_retained_block_buffer(
            std::ptr::null_mut(),
            list,
            needed,
            None,
            None,
            0,
            &mut block,
        )
    };
    // Own the retained block buffer so it is released on every path.
    // SAFETY: on success CoreMedia returns `block` at +1 retain count, which we now own.
    let _block = NonNull::new(block).map(|p| unsafe { CFRetained::from_raw(p) });
    if st != 0 {
        return None;
    }
    // SAFETY: `st == 0`, so `list` (inside `storage`, alive to the end) was filled by CoreMedia.
    let n_buffers = unsafe { (*list).mNumberBuffers } as usize;
    // SAFETY: as above; this only computes the address of the trailing buffer array.
    let first = unsafe { (*list).mBuffers.as_ptr() };
    let mut planes: Vec<(&[f32], usize)> = Vec::new(); // (samples, channels in this buffer)
    for i in 0..n_buffers {
        // SAFETY: `i < mNumberBuffers`, and the list holds that many AudioBuffers inside `storage`.
        let b = unsafe { &*first.add(i) };
        if b.mData.is_null() {
            return None;
        }
        let len = b.mDataByteSize as usize / 4;
        // SAFETY: `mData` is non-null and holds `mDataByteSize` bytes of f32 audio (SCK delivers
        // float PCM), kept alive by `_block`; the slice is copied before `_block` drops.
        let samples = unsafe { std::slice::from_raw_parts(b.mData.cast::<f32>(), len) };
        planes.push((samples, (b.mNumberChannels as usize).max(1)));
    }
    let (out, actual) = interleave(&planes)?;
    // The stream was asked for SCK_CHANNELS, but the buffers say what they really hold: fit them
    // to what the sink was told, so a mono or multichannel device cannot shift every frame (R3-23).
    let out = fit_channels(out, actual);
    Some((host_ns, out))
}

/// Fits interleaved samples of `actual` channels to [`SCK_CHANNELS`] (mono is duplicated, extra
/// channels are dropped); unchanged when the count matches.
fn fit_channels(data: Vec<f32>, actual: usize) -> Vec<f32> {
    if actual == usize::from(SCK_CHANNELS) {
        return data;
    }
    crate::pipeline::map_channels(&data, u16::try_from(actual).unwrap_or(1), SCK_CHANNELS)
}

/// Combines ScreenCaptureKit buffers into interleaved samples and their channel count: either one
/// interleaved buffer or one buffer per channel.
fn interleave(planes: &[(&[f32], usize)]) -> Option<(Vec<f32>, usize)> {
    match planes {
        [] => None,
        [(s, c)] => Some((s.to_vec(), *c)),
        many => {
            let ch = many.len();
            let frames = many.iter().map(|(s, c)| s.len() / c).min()?;
            if many.iter().any(|(_, c)| *c != 1) {
                return None;
            }
            let mut out = Vec::with_capacity(frames * ch);
            for f in 0..frames {
                for (s, _) in many {
                    out.push(s[f]);
                }
            }
            Some((out, ch))
        }
    }
}

/// ScreenCaptureKit audio as an [`AudioSource`].
pub struct SckAudioSource {
    filter: Retained<SCContentFilter>,
    stream: Option<Retained<SCStream>>,
    output: Option<Retained<AudioOutput>>,
    queue: DispatchRetained<DispatchQueue>,
}

fn get_content() -> Result<Retained<SCShareableContent>, CmError> {
    let (tx, rx) = mpsc::channel::<Result<SendBox<Retained<SCShareableContent>>, String>>();
    let block = RcBlock::new(move |c: *mut SCShareableContent, e: *mut NSError| {
        // SAFETY: ScreenCaptureKit passes either a valid content object or null, and a valid
        // error or null; retain keeps the content alive past the handler.
        let r = match unsafe { Retained::retain(c) } {
            Some(c) => Ok(SendBox(c)),
            // SAFETY: with no content, SCK passes a valid NSError or null; `as_ref` handles null.
            None => Err(unsafe { e.as_ref() }
                .map(|e| e.localizedDescription().to_string())
                .unwrap_or_else(|| "unknown error".into())),
        };
        let _ = tx.send(r);
    });
    // SAFETY: the block is retained by the framework until it completes.
    unsafe { SCShareableContent::getShareableContentWithCompletionHandler(&block) };
    match rx.recv_timeout(CALL_TIMEOUT) {
        Ok(Ok(c)) => Ok(c.0),
        Ok(Err(e)) => Err(CmError::permission_missing(
            Permission::Screen,
            format!("Screen Recording access is needed for system audio ({e}). Allow it in System Settings > Privacy & Security > Screen & System Audio Recording."),
        )),
        Err(_) => Err(CmError::internal("ScreenCaptureKit did not answer in time")),
    }
}

fn main_display(content: &SCShareableContent) -> Option<Retained<SCDisplay>> {
    // SAFETY: plain property reads on retained objects.
    unsafe {
        let main_id = objc2_core_graphics::CGMainDisplayID();
        let displays = content.displays();
        displays
            .iter()
            .find(|d| d.displayID() == main_id)
            .or_else(|| displays.iter().next())
    }
}

impl SckAudioSource {
    /// Prepares the capture filter: the main display, minus the apps in `exclude_bundle_ids`
    /// (their audio is dropped too). CaptureMann's own process audio is excluded separately by
    /// the stream configuration.
    pub fn open(exclude_bundle_ids: &[String]) -> Result<Self, CmError> {
        if macos_major_version() < 13 {
            return Err(CmError::invalid_input(
                "System audio needs macOS 13 or later.",
            ));
        }
        if !objc2_core_graphics::CGPreflightScreenCaptureAccess() {
            return Err(CmError::permission_missing(
                Permission::Screen,
                "Screen Recording access is needed for system audio. Allow it in System Settings > Privacy & Security > Screen & System Audio Recording.",
            ));
        }
        let content = get_content()?;
        let display = main_display(&content).ok_or_else(|| {
            // SAFETY: plain property reads on a retained object.
            let (displays, apps) =
                unsafe { (content.displays().len(), content.applications().len()) };
            CmError::not_found(format!(
                "No display found for system audio capture (ScreenCaptureKit listed {displays} displays and {apps} apps)."
            ))
        })?;
        // SAFETY: filter construction with valid retained inputs.
        let filter = unsafe {
            let excluded: Vec<Retained<SCRunningApplication>> = content
                .applications()
                .iter()
                .filter(|a| {
                    let id = a.bundleIdentifier().to_string();
                    exclude_bundle_ids.contains(&id)
                })
                .collect();
            if excluded.is_empty() {
                SCContentFilter::initWithDisplay_excludingWindows(
                    SCContentFilter::alloc(),
                    &display,
                    &NSArray::new(),
                )
            } else {
                SCContentFilter::initWithDisplay_excludingApplications_exceptingWindows(
                    SCContentFilter::alloc(),
                    &display,
                    &NSArray::from_retained_slice(&excluded),
                    &NSArray::new(),
                )
            }
        };
        Ok(SckAudioSource {
            filter,
            stream: None,
            output: None,
            queue: DispatchQueue::new("dev.capturemann.audio.sck", None),
        })
    }
}

impl AudioSource for SckAudioSource {
    fn format(&self) -> SourceFormat {
        SourceFormat {
            sample_rate: SCK_RATE,
            channels: SCK_CHANNELS,
            device_name: "System audio".into(),
            device_id: "screencapturekit".into(),
        }
    }

    fn start(&mut self, sink: SourceSink) -> Result<(), CmError> {
        let output = AudioOutput::new(sink);
        // SAFETY: configuration setters on a freshly created configuration object.
        let cfg = unsafe {
            let cfg = SCStreamConfiguration::new();
            cfg.setCapturesAudio(true);
            cfg.setExcludesCurrentProcessAudio(true);
            cfg.setSampleRate(SCK_RATE as isize);
            cfg.setChannelCount(SCK_CHANNELS as isize);
            // Video is unused; keep it as small and slow as ScreenCaptureKit allows.
            cfg.setWidth(16);
            cfg.setHeight(16);
            cfg.setMinimumFrameInterval(CMTime {
                value: 1,
                timescale: 1,
                flags: CMTimeFlags::Valid,
                epoch: 0,
            });
            cfg.setShowsCursor(false);
            cfg.setQueueDepth(3);
            cfg
        };
        let delegate: &ProtocolObject<dyn SCStreamDelegate> = ProtocolObject::from_ref(&*output);
        // SAFETY: valid filter/configuration/delegate; the stream retains the delegate weakly, so
        // `self.output` keeps it alive for the stream's lifetime.
        let stream = unsafe {
            SCStream::initWithFilter_configuration_delegate(
                SCStream::alloc(),
                &self.filter,
                &cfg,
                Some(delegate),
            )
        };
        let out_obj: &ProtocolObject<dyn SCStreamOutput> = ProtocolObject::from_ref(&*output);
        // SAFETY: the output object and queue outlive the stream (stored in `self`).
        unsafe {
            stream
                .addStreamOutput_type_sampleHandlerQueue_error(
                    out_obj,
                    SCStreamOutputType::Audio,
                    Some(&self.queue),
                )
                .map_err(|e| {
                    CmError::internal(format!(
                        "addStreamOutput(audio): {}",
                        e.localizedDescription()
                    ))
                })?;
            stream
                .addStreamOutput_type_sampleHandlerQueue_error(
                    out_obj,
                    SCStreamOutputType::Screen,
                    Some(&self.queue),
                )
                .map_err(|e| {
                    CmError::internal(format!(
                        "addStreamOutput(screen): {}",
                        e.localizedDescription()
                    ))
                })?;
        }
        let (tx, rx) = mpsc::channel::<Option<String>>();
        let block = RcBlock::new(move |e: *mut NSError| {
            // SAFETY: null or a valid NSError for the duration of the handler.
            let msg = unsafe { e.as_ref() }.map(|e| e.localizedDescription().to_string());
            let _ = tx.send(msg);
        });
        // SAFETY: the block is retained until the framework calls it.
        unsafe { stream.startCaptureWithCompletionHandler(Some(&block)) };
        match rx.recv_timeout(CALL_TIMEOUT) {
            Ok(None) => {}
            Ok(Some(e)) => {
                return Err(CmError::permission_missing(
                    Permission::Screen,
                    format!("Could not start system audio capture: {e}"),
                ))
            }
            Err(_) => {
                return Err(CmError::internal(
                    "system audio capture did not start in time",
                ))
            }
        }
        self.output = Some(output);
        self.stream = Some(stream);
        Ok(())
    }

    fn stop(&mut self) {
        if let Some(stream) = self.stream.take() {
            let (tx, rx) = mpsc::channel::<()>();
            let block = RcBlock::new(move |_e: *mut NSError| {
                let _ = tx.send(());
            });
            // SAFETY: the block is retained until the framework calls it.
            unsafe { stream.stopCaptureWithCompletionHandler(Some(&block)) };
            let _ = rx.recv_timeout(Duration::from_secs(10));
        }
        // Drop the sink so no more chunks can be pushed.
        if let Some(out) = self.output.take() {
            if let Ok(mut g) = out.ivars().sink.lock() {
                *g = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn planar_buffers_interleave_in_channel_order() {
        let l = [1.0f32, 2.0, 3.0];
        let r = [-1.0f32, -2.0, -3.0];
        let (out, ch) = interleave(&[(&l, 1), (&r, 1)]).unwrap();
        assert_eq!(out, vec![1.0, -1.0, 2.0, -2.0, 3.0, -3.0]);
        assert_eq!(ch, 2);
    }

    #[test]
    fn single_interleaved_buffer_passes_through() {
        let s = [0.1f32, 0.2, 0.3, 0.4];
        assert_eq!(interleave(&[(&s, 2)]).unwrap(), (s.to_vec(), 2));
        assert!(interleave(&[]).is_none());
    }

    /// R3-23: a buffer with another channel count than the 2 the sink was told about is fitted,
    /// not passed through (that would shift every later frame).
    #[test]
    fn buffers_are_fitted_to_the_declared_channel_count() {
        let mono = [0.5f32, -0.5, 0.25];
        assert_eq!(
            fit_channels(mono.to_vec(), 1),
            vec![0.5, 0.5, -0.5, -0.5, 0.25, 0.25]
        );
        let six = [
            1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0,
        ];
        assert_eq!(fit_channels(six.to_vec(), 6), vec![1.0, 2.0, 7.0, 8.0]);
        let st = vec![1.0f32, 2.0];
        assert_eq!(fit_channels(st.clone(), 2), st);
    }

    #[test]
    fn cm_time_converts_to_ns() {
        let t = CMTime {
            value: 3_000_000_000,
            timescale: 1_000_000_000,
            flags: CMTimeFlags::Valid,
            epoch: 0,
        };
        assert_eq!(cm_ns(t), Some(3_000_000_000));
        let bad = CMTime {
            value: 1,
            timescale: 0,
            flags: CMTimeFlags::Valid,
            epoch: 0,
        };
        assert_eq!(cm_ns(bad), None);
    }

    #[test]
    fn os_version_is_plausible() {
        assert!(macos_major_version() >= 11);
    }
}
