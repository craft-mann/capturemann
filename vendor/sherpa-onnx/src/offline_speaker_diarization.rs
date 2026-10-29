//! Offline speaker diarization.
//!
//! This combines segmentation, speaker embedding extraction, and clustering.
//! See `rust-api-examples/examples/offline_speaker_diarization.rs`.

use crate::{speaker_embedding::SpeakerEmbeddingExtractorConfig, utils::to_c_ptr};
use sherpa_onnx_sys as sys;
use std::ffi::CString;
use std::slice;

#[derive(Clone, Debug)]
/// Pyannote segmentation model path.
pub struct OfflineSpeakerSegmentationPyannoteModelConfig {
    pub model: Option<String>,
    /// Sliding-window shift as a fraction of the model window size.
    pub window_shift_ratio: f32,
}

impl Default for OfflineSpeakerSegmentationPyannoteModelConfig {
    fn default() -> Self {
        Self {
            model: None,
            window_shift_ratio: 0.1,
        }
    }
}

impl OfflineSpeakerSegmentationPyannoteModelConfig {
    fn to_sys(
        &self,
        cstrings: &mut Vec<CString>,
    ) -> sys::OfflineSpeakerSegmentationPyannoteModelConfig {
        sys::OfflineSpeakerSegmentationPyannoteModelConfig {
            model: to_c_ptr(&self.model, cstrings),
            window_shift_ratio: self.window_shift_ratio,
        }
    }
}

#[derive(Clone, Debug)]
/// Segmentation model configuration for diarization.
pub struct OfflineSpeakerSegmentationModelConfig {
    pub pyannote: OfflineSpeakerSegmentationPyannoteModelConfig,
    pub num_threads: i32,
    pub debug: bool,
    pub provider: Option<String>,
}

impl Default for OfflineSpeakerSegmentationModelConfig {
    fn default() -> Self {
        Self {
            pyannote: Default::default(),
            num_threads: 1,
            debug: false,
            provider: Some("cpu".to_string()),
        }
    }
}

impl OfflineSpeakerSegmentationModelConfig {
    fn to_sys(&self, cstrings: &mut Vec<CString>) -> sys::OfflineSpeakerSegmentationModelConfig {
        sys::OfflineSpeakerSegmentationModelConfig {
            pyannote: self.pyannote.to_sys(cstrings),
            num_threads: self.num_threads,
            debug: self.debug as i32,
            provider: to_c_ptr(&self.provider, cstrings),
        }
    }
}

#[derive(Clone, Debug)]
/// Fast clustering options used after segmentation and embedding extraction.
pub struct FastClusteringConfig {
    pub num_clusters: i32,
    pub threshold: f32,
    pub compute_confidence: bool,
}

impl Default for FastClusteringConfig {
    fn default() -> Self {
        Self {
            num_clusters: -1,
            threshold: 0.5,
            compute_confidence: false,
        }
    }
}

impl FastClusteringConfig {
    fn to_sys(&self) -> sys::FastClusteringConfig {
        sys::FastClusteringConfig {
            num_clusters: self.num_clusters,
            threshold: self.threshold,
            compute_confidence: self.compute_confidence as i32,
        }
    }
}

#[derive(Clone, Debug)]
/// Top-level configuration for [`OfflineSpeakerDiarization`].
pub struct OfflineSpeakerDiarizationConfig {
    pub segmentation: OfflineSpeakerSegmentationModelConfig,
    pub embedding: SpeakerEmbeddingExtractorConfig,
    pub clustering: FastClusteringConfig,
    pub min_duration_on: f32,
    pub min_duration_off: f32,
}

impl Default for OfflineSpeakerDiarizationConfig {
    fn default() -> Self {
        Self {
            segmentation: Default::default(),
            embedding: Default::default(),
            clustering: Default::default(),
            min_duration_on: 0.3,
            min_duration_off: 0.5,
        }
    }
}

impl OfflineSpeakerDiarizationConfig {
    fn to_sys(&self, cstrings: &mut Vec<CString>) -> sys::OfflineSpeakerDiarizationConfig {
        sys::OfflineSpeakerDiarizationConfig {
            segmentation: self.segmentation.to_sys(cstrings),
            embedding: self.embedding.to_sys(cstrings),
            clustering: self.clustering.to_sys(),
            min_duration_on: self.min_duration_on,
            min_duration_off: self.min_duration_off,
        }
    }
}

#[derive(Clone, Debug)]
/// One diarization segment labeled with a speaker index.
pub struct OfflineSpeakerDiarizationSegment {
    pub start: f32,
    pub end: f32,
    pub speaker: i32,
    pub confidence: f32,
}

/// Offline speaker diarizer.
pub struct OfflineSpeakerDiarization {
    ptr: *const sys::OfflineSpeakerDiarization,
}

// SAFETY: The sherpa-onnx C library is thread-safe for single-object usage.
unsafe impl Send for OfflineSpeakerDiarization {}
unsafe impl Sync for OfflineSpeakerDiarization {}

impl OfflineSpeakerDiarization {
    /// Create a diarizer from `config`.
    pub fn create(config: &OfflineSpeakerDiarizationConfig) -> Option<Self> {
        let mut cstrings = Vec::new();
        let sys_config = config.to_sys(&mut cstrings);
        let ptr = unsafe { sys::SherpaOnnxCreateOfflineSpeakerDiarization(&sys_config) };
        if ptr.is_null() {
            None
        } else {
            Some(Self { ptr })
        }
    }

    /// Return the sample rate expected by the segmentation model.
    pub fn sample_rate(&self) -> i32 {
        unsafe { sys::SherpaOnnxOfflineSpeakerDiarizationGetSampleRate(self.ptr) }
    }

    /// Replace the current configuration.
    pub fn set_config(&self, config: &OfflineSpeakerDiarizationConfig) {
        let mut cstrings = Vec::new();
        let sys_config = config.to_sys(&mut cstrings);
        unsafe { sys::SherpaOnnxOfflineSpeakerDiarizationSetConfig(self.ptr, &sys_config) }
    }

    /// Process a complete waveform and return a diarization result.
    pub fn process(&self, samples: &[f32]) -> Option<OfflineSpeakerDiarizationResult> {
        let ptr = unsafe {
            sys::SherpaOnnxOfflineSpeakerDiarizationProcess(
                self.ptr,
                samples.as_ptr(),
                samples.len() as i32,
            )
        };
        if ptr.is_null() {
            None
        } else {
            Some(OfflineSpeakerDiarizationResult { ptr })
        }
    }
}

/// What the progress callback of [`OfflineSpeakerDiarization::process_with_progress`] holds.
struct ProgressState<'a> {
    callback: &'a mut dyn FnMut(i32, i32) -> bool,
}

unsafe extern "C" fn progress_trampoline(
    processed: i32,
    total: i32,
    arg: *mut std::os::raw::c_void,
) -> i32 {
    // SAFETY: `arg` is the `ProgressState` that `process_with_progress` put there and keeps alive
    // for the whole call; the library calls this on the thread that called `process`.
    let state = unsafe { &mut *arg.cast::<ProgressState<'_>>() };
    // The C API stops when the callback returns a non-zero value.
    i32::from(!(state.callback)(processed, total))
}

impl OfflineSpeakerDiarization {
    /// [`OfflineSpeakerDiarization::process`] that reports progress: `callback` gets the chunks
    /// done and the chunks in total (the segmentation and the voice-embedding steps; the final
    /// clustering reports nothing). Returning `false` asks the library to stop (the C API passes a
    /// non-zero value back), but **sherpa-onnx 2.0.1 ignores it**: measured on a 5-minute file, the
    /// run went on to the end and kept reporting. Added by CaptureMann: the crates.io crate has no
    /// binding for it.
    pub fn process_with_progress(
        &self,
        samples: &[f32],
        callback: &mut dyn FnMut(i32, i32) -> bool,
    ) -> Option<OfflineSpeakerDiarizationResult> {
        let mut state = ProgressState { callback };
        let ptr = unsafe {
            sys::SherpaOnnxOfflineSpeakerDiarizationProcessWithCallback(
                self.ptr,
                samples.as_ptr(),
                samples.len() as i32,
                Some(progress_trampoline),
                (&mut state as *mut ProgressState<'_>).cast(),
            )
        };
        if ptr.is_null() {
            None
        } else {
            Some(OfflineSpeakerDiarizationResult { ptr })
        }
    }
}

impl Drop for OfflineSpeakerDiarization {
    fn drop(&mut self) {
        unsafe {
            if !self.ptr.is_null() {
                sys::SherpaOnnxDestroyOfflineSpeakerDiarization(self.ptr);
            }
        }
    }
}

/// Result object returned by [`OfflineSpeakerDiarization::process`].
pub struct OfflineSpeakerDiarizationResult {
    ptr: *const sys::OfflineSpeakerDiarizationResult,
}

// SAFETY: The sherpa-onnx C library is thread-safe for single-object usage.
unsafe impl Send for OfflineSpeakerDiarizationResult {}
unsafe impl Sync for OfflineSpeakerDiarizationResult {}

impl OfflineSpeakerDiarizationResult {
    /// Return the number of speakers estimated for the recording.
    pub fn num_speakers(&self) -> i32 {
        unsafe { sys::SherpaOnnxOfflineSpeakerDiarizationResultGetNumSpeakers(self.ptr) }
    }

    /// Return the number of diarization segments.
    pub fn num_segments(&self) -> i32 {
        unsafe { sys::SherpaOnnxOfflineSpeakerDiarizationResultGetNumSegments(self.ptr) }
    }

    /// Return all segments sorted by start time.
    pub fn sort_by_start_time(&self) -> Vec<OfflineSpeakerDiarizationSegment> {
        let n = self.num_segments();
        if n <= 0 {
            return Vec::new();
        }

        unsafe {
            let p = sys::SherpaOnnxOfflineSpeakerDiarizationResultSortByStartTime(self.ptr);
            if p.is_null() {
                return Vec::new();
            }

            let segments = slice::from_raw_parts(p, n as usize)
                .iter()
                .map(|s| OfflineSpeakerDiarizationSegment {
                    start: s.start,
                    end: s.end,
                    speaker: s.speaker,
                    confidence: s.confidence,
                })
                .collect::<Vec<_>>();
            sys::SherpaOnnxOfflineSpeakerDiarizationDestroySegment(p);
            segments
        }
    }
}

impl Drop for OfflineSpeakerDiarizationResult {
    fn drop(&mut self) {
        unsafe {
            if !self.ptr.is_null() {
                sys::SherpaOnnxOfflineSpeakerDiarizationDestroyResult(self.ptr);
            }
        }
    }
}
