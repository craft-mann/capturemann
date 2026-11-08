//! The camera track's file writer on Linux: the software VP9 encoder of `cm-encode` (an FFmpeg
//! child process, fragmented MP4 every 2 s), the same path the screen track uses there (decision
//! O10: no software H.264 ships; a hardware H.264 encoder through VA-API is not wired for the
//! camera yet). The track's details say `vp9`, so the codec is visible to everyone who reads the
//! recording.

use std::path::Path;
use std::sync::Arc;

use cm_core::{CmError, MediaTime};
use cm_encode::{
    AppendOutcome as EncodeOutcome, Codec, CpuFrame, EncoderConfig, FfmpegVp9Encoder,
    PixelFormat as CpuFormat, Quality, VideoEncoder, VideoPlan, WriterSummary,
};

use crate::convert::{PixelFormat, RawFrame};
use crate::types::CameraFrame;
use crate::writer::{
    AppendOutcome, FragmentInfo, FragmentWriter, WriterFactory, WriterReport, WriterSpec,
};

pub struct LinuxWriterFactory;

impl WriterFactory for LinuxWriterFactory {
    fn codec(&self) -> &'static str {
        "vp9"
    }

    fn create(&self, path: &Path, spec: &WriterSpec) -> Result<Box<dyn FragmentWriter>, CmError> {
        let cfg = EncoderConfig {
            path: path.to_path_buf(),
            plan: VideoPlan {
                width: spec.width,
                height: spec.height,
                fps: spec.fps,
                // The plan's codec is not used by the VP9 encoder; the file says what it holds.
                codec: Codec::H264,
                bitrate_bps: u64::from(spec.bitrate_bps),
                quality: Quality::High,
            },
        };
        Ok(Box::new(Writer {
            encoder: FfmpegVp9Encoder::create(&cfg)?,
            width: spec.width,
            height: spec.height,
            failed: None,
        }))
    }
}

struct Writer {
    encoder: FfmpegVp9Encoder,
    width: u32,
    height: u32,
    /// The first failure; once set every `append` answers it.
    failed: Option<String>,
}

impl Writer {
    fn fail(&mut self, why: String) -> AppendOutcome {
        self.failed = Some(why.clone());
        AppendOutcome::Failed(why)
    }
}

/// `raw` as the frame the encoder takes: NV12 of exactly the output size for the YUV layouts
/// (YUYV is converted and, when the size differs, scaled by area average), the pixels as they are
/// for packed RGB (FFmpeg scales those).
fn encoder_frame(raw: &RawFrame, width: u32, height: u32) -> Option<CpuFrame> {
    match raw.format {
        PixelFormat::Bgra => Some(CpuFrame {
            width: raw.width,
            height: raw.height,
            stride: raw.stride,
            format: CpuFormat::Bgra,
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

impl FragmentWriter for Writer {
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
        match self.encoder.check() {
            Ok(()) => AppendOutcome::Written,
            Err(e) => self.fail(e.to_string()),
        }
    }

    /// FFmpeg cuts a fragment about every 2 s on its own: a device that vanishes costs at most the
    /// fragment in progress.
    fn flush(&mut self) {}

    fn finish(&mut self) -> Result<WriterReport, CmError> {
        let summary = self.encoder.finish()?;
        Ok(report_of(&summary))
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_yuyv_frame_becomes_nv12_of_the_output_size() {
        // 4x2 YUYV, flat grey.
        let raw = RawFrame {
            format: PixelFormat::Yuy2,
            width: 4,
            height: 2,
            stride: 8,
            data: Arc::new([128u8, 128, 128, 128].repeat(4)),
        };
        let f = encoder_frame(&raw, 4, 2).unwrap();
        assert_eq!(f.format, CpuFormat::Nv12);
        assert_eq!(f.data.len(), 4 * 2 + 4 * 2 / 2);
        assert!(f.validate().is_ok());
    }
}
