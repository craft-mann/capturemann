//! The camera track's file writer on Windows: the shared Media Foundation hardware H.264 encoder
//! and fragmented MP4 muxer of `cm-encode` (the one the screen track uses), behind the camera
//! crate's [`FragmentWriter`] contract.
//!
//! Frames reach this writer as NV12, YUY2 or packed RGB: the source reader decodes Motion JPEG to
//! one of those before delivery (`sys/source.rs`), so there is no JPEG step here. NV12 and YUY2 are
//! converted to NV12 of the output size by [`RawFrame::to_nv12`]; RGB goes to the encoder as it is
//! (it converts and scales those itself). There is no software H.264 encoder: a computer without a
//! hardware one fails at `create` with `hardware_encoder_unavailable`.

use std::path::Path;
use std::sync::Arc;

use cm_core::{CmError, MediaTime};
use cm_encode::{
    AppendOutcome as EncodeOutcome, Codec, CpuFrame, EncoderConfig, HardwareEncoder,
    PixelFormat as CpuFormat, Quality, VideoEncoder, VideoPlan, WriterSummary,
};

use crate::convert::{PixelFormat, RawFrame};
use crate::types::CameraFrame;
use crate::writer::{
    AppendOutcome, FragmentInfo, FragmentWriter, WriterFactory, WriterReport, WriterSpec,
};

pub struct MfWriterFactory;

impl WriterFactory for MfWriterFactory {
    fn create(&self, path: &Path, spec: &WriterSpec) -> Result<Box<dyn FragmentWriter>, CmError> {
        let cfg = EncoderConfig {
            path: path.to_path_buf(),
            plan: plan_for(spec),
        };
        let encoder = HardwareEncoder::create(&cfg)?;
        Ok(Box::new(MfWriter {
            encoder,
            width: spec.width,
            height: spec.height,
            failed: None,
        }))
    }
}

/// The encode plan of a camera track: H.264 (no HEVC on Windows), the spec's size, rate and rate
/// target.
fn plan_for(spec: &WriterSpec) -> VideoPlan {
    VideoPlan {
        width: spec.width,
        height: spec.height,
        fps: spec.fps,
        codec: Codec::H264,
        bitrate_bps: u64::from(spec.bitrate_bps),
        quality: Quality::High,
    }
}

struct MfWriter {
    encoder: HardwareEncoder,
    width: u32,
    height: u32,
    /// The first failure; once set every `append` answers it.
    failed: Option<String>,
}

impl MfWriter {
    fn fail(&mut self, why: String) -> AppendOutcome {
        self.failed = Some(why.clone());
        AppendOutcome::Failed(why)
    }
}

/// `raw` as the frame the encoder takes at the output size: NV12 of exactly that size for the
/// YUV layouts, the pixels as they are for RGB.
fn encoder_frame(raw: &RawFrame, width: u32, height: u32) -> Option<CpuFrame> {
    match raw.format {
        PixelFormat::Bgra => Some(CpuFrame {
            width: raw.width,
            height: raw.height,
            stride: raw.stride,
            // Media Foundation's RGB32 leaves the fourth byte unused.
            format: CpuFormat::Bgrx,
            data: Arc::clone(&raw.data),
        }),
        PixelFormat::Nv12 | PixelFormat::Yuy2 => {
            let data = raw.to_nv12(width, height)?;
            Some(CpuFrame {
                width,
                height,
                stride: width as usize,
                format: CpuFormat::Nv12,
                data: Arc::new(data),
            })
        }
    }
}

impl FragmentWriter for MfWriter {
    fn append(
        &mut self,
        frame: &CameraFrame,
        pts: MediaTime,
        duration: MediaTime,
    ) -> AppendOutcome {
        if let Some(why) = &self.failed {
            return AppendOutcome::Failed(why.clone());
        }
        let Some(native) = &frame.native else {
            return self.fail("the camera frame has no pixels".into());
        };
        let Some(cpu) = encoder_frame(&native.0, self.width, self.height) else {
            return self.fail(format!(
                "cannot convert a {}x{} {:?} camera frame to {}x{}",
                native.0.width, native.0.height, native.0.format, self.width, self.height
            ));
        };
        match self.encoder.append(&cpu, pts, duration) {
            Ok(EncodeOutcome::Written) => {}
            Ok(EncodeOutcome::NotReady) => return AppendOutcome::NotReady,
            Err(e) => return self.fail(e.to_string()),
        }
        // A disk error found by the file writer stops the track at once, not at the next finish.
        match self.encoder.check() {
            Ok(()) => AppendOutcome::Written,
            Err(e) => self.fail(e.to_string()),
        }
    }

    /// The muxer cuts a fragment about every 2 s on its own and has no way to force one out, so
    /// this does nothing: a device that vanishes costs at most the fragment in progress.
    fn flush(&mut self) {}

    fn finish(&mut self) -> Result<WriterReport, CmError> {
        let summary = self.encoder.finish()?;
        Ok(report_of(&summary))
    }
}

/// What the camera crate records about a finished file.
fn report_of(summary: &WriterSummary) -> WriterReport {
    WriterReport {
        fragments: summary
            .fragments
            .iter()
            .map(|f| FragmentInfo {
                start: f.start,
                duration: f.duration,
                bytes: f.bytes,
            })
            .collect(),
        bytes: summary.bytes,
    }
}
