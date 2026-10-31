//! A pure-Rust [`VideoEncoder`] that writes structurally valid (not decodable) fragmented MP4
//! files. CI tests of the recording pipeline use it so they need no display and no GPU; the
//! recovery tests use the files it writes.

use std::sync::{Arc, Mutex};

use cm_core::{CmError, MediaTime};

use crate::encoder::{AppendOutcome, EncoderConfig, VideoEncoder};
use crate::fmp4::synth;
use crate::settings::FRAGMENT_SECS;
use crate::writer::{FragmentedMp4Writer, WriterSummary};

/// A fake frame: just an id the tests can follow through the pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyntheticFrame(pub u64);

/// One frame as the encoder received it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Appended {
    pub frame: SyntheticFrame,
    pub pts: MediaTime,
    pub duration: MediaTime,
}

pub type AppendLog = Arc<Mutex<Vec<Appended>>>;

pub struct SyntheticEncoder {
    writer: FragmentedMp4Writer,
    spec: synth::Spec,
    log: AppendLog,
    /// Samples of the open fragment: (pts, duration).
    open: Vec<(i64, i64)>,
    sequence: u32,
    last_pts: Option<i64>,
    appends: u64,
    /// Every n-th append reports `NotReady` instead (0 = never).
    pub not_ready_every: u64,
    /// `finish` before any frame was written fails with the "no frames" error, like the hardware
    /// encoder does (R3-5). Off by default.
    pub fail_empty: bool,
    finished: Option<WriterSummary>,
}

impl SyntheticEncoder {
    pub fn create(cfg: &EncoderConfig) -> Result<Self, CmError> {
        let writer = FragmentedMp4Writer::create(&cfg.path)?;
        let spec = synth::Spec {
            width: cfg.plan.width,
            height: cfg.plan.height,
            codec: match cfg.plan.codec {
                crate::settings::Codec::Hevc => *b"hvc1",
                crate::settings::Codec::H264 => *b"avc1",
            },
            ..Default::default()
        };
        writer.push_init(synth::init_segment(&spec));
        Ok(SyntheticEncoder {
            writer,
            spec,
            log: Arc::default(),
            open: Vec::new(),
            sequence: 0,
            last_pts: None,
            appends: 0,
            not_ready_every: 0,
            fail_empty: false,
            finished: None,
        })
    }

    /// Everything appended so far, for assertions.
    pub fn log(&self) -> AppendLog {
        Arc::clone(&self.log)
    }

    fn flush(&mut self) {
        let Some(&(first, _)) = self.open.first() else {
            return;
        };
        self.sequence += 1;
        // The synthetic fragment has one constant sample duration (the first sample's); the
        // recorder produces constant frame rate, so that is exact.
        let dur = u32::try_from(self.open[0].1).unwrap_or(0);
        let samples = u32::try_from(self.open.len()).unwrap_or(u32::MAX);
        let seg = synth::fragment(
            &self.spec,
            self.sequence,
            u64::try_from(first).unwrap_or(0),
            samples,
            dur,
            32,
        );
        self.writer.push_segment(seg);
        self.open.clear();
    }
}

impl VideoEncoder for SyntheticEncoder {
    type Frame = SyntheticFrame;

    fn append(
        &mut self,
        frame: &SyntheticFrame,
        pts: MediaTime,
        duration: MediaTime,
    ) -> Result<AppendOutcome, CmError> {
        self.writer.check()?;
        self.appends += 1;
        if self.not_ready_every > 0 && self.appends.is_multiple_of(self.not_ready_every) {
            return Ok(AppendOutcome::NotReady);
        }
        let p = pts.ticks();
        if self.last_pts.is_some_and(|l| p <= l) {
            return Err(CmError::internal(format!(
                "non-increasing pts {p} after {:?}",
                self.last_pts
            )));
        }
        self.last_pts = Some(p);
        if let Some(&(first, _)) = self.open.first() {
            if p - first >= i64::from(FRAGMENT_SECS) * 120_000 {
                self.flush();
            }
        }
        self.open.push((p, duration.ticks()));
        if let Ok(mut l) = self.log.lock() {
            l.push(Appended {
                frame: *frame,
                pts,
                duration,
            });
        }
        Ok(AppendOutcome::Written)
    }

    fn check(&self) -> Result<(), CmError> {
        self.writer.check()
    }

    fn fragments_written(&self) -> usize {
        self.writer.fragments_written()
    }

    fn finish(&mut self) -> Result<WriterSummary, CmError> {
        if let Some(s) = &self.finished {
            return Ok(s.clone());
        }
        if self.fail_empty && self.log.lock().map_or(true, |l| l.is_empty()) {
            let _ = self.writer.finish();
            return Err(crate::encoder::no_frames_error());
        }
        self.flush();
        let s = self.writer.finish()?;
        self.finished = Some(s.clone());
        Ok(s)
    }
}

/// [`SyntheticEncoder`] for sources that deliver [`crate::CpuFrame`]s (the Linux PipeWire source):
/// the pixels are ignored, the timing and the file structure are the same. It reports `vp9` as
/// its codec like the real Linux encoder, so tests see the manifest string of that path.
pub struct CpuSyntheticEncoder {
    inner: SyntheticEncoder,
    /// The sizes of the first frames appended (what the real encoder sizes its input by).
    pub sizes: Vec<(u32, u32)>,
}

impl CpuSyntheticEncoder {
    pub fn create(cfg: &EncoderConfig) -> Result<Self, CmError> {
        Ok(CpuSyntheticEncoder {
            inner: SyntheticEncoder::create(cfg)?,
            sizes: Vec::new(),
        })
    }
}

impl VideoEncoder for CpuSyntheticEncoder {
    type Frame = crate::CpuFrame;

    fn append(
        &mut self,
        frame: &crate::CpuFrame,
        pts: MediaTime,
        duration: MediaTime,
    ) -> Result<AppendOutcome, CmError> {
        if self.sizes.len() < 4 {
            self.sizes.push((frame.width, frame.height));
        }
        self.inner
            .append(&SyntheticFrame(u64::from(frame.width)), pts, duration)
    }

    fn codec_name(&self) -> Option<&'static str> {
        Some("vp9")
    }

    fn check(&self) -> Result<(), CmError> {
        self.inner.check()
    }

    fn fragments_written(&self) -> usize {
        self.inner.fragments_written()
    }

    fn finish(&mut self) -> Result<WriterSummary, CmError> {
        self.inner.finish()
    }
}
