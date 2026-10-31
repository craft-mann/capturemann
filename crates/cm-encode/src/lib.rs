//! Hardware recording encoders and the fragmented MP4 writer (M3 slice 1: the screen track).
//!
//! * [`settings`]: quality, size, fps and codec selection and bitrate targets (REC-2, REC-3), pure.
//! * [`fmp4`]: a box parser and truncation helper for the files this crate writes (REC-8), plus
//!   builders for tests.
//! * [`writer`]: [`FragmentedMp4Writer`], the platform-neutral file half: init segment first, one
//!   `moof` per ~2 s fragment, written and synced on its own thread (NFR-6).
//! * [`encoder`]: the [`VideoEncoder`] trait recorders program against.
//! * [`synthetic`]: a pure-Rust encoder for tests without a display.
//!
//! There is no software H.264 fallback in v0.1: when the platform encoder rejects a plan, the
//! recording fails at the start with [`hardware_encoder_unavailable`] (REC-2).
//! * `mac` (macOS): `AVAssetWriter` + VideoToolbox fed IOSurface frames, 2 s CMAF segments through
//!   the writer delegate (the approach proven in spike S6). The only module with `unsafe`.
//! * `windows` (Windows): a stub returning the shared `not_implemented` error.

#![cfg_attr(test, allow(clippy::unwrap_used))]

pub mod cpu;
pub mod encoder;
pub mod ffmpeg_vp9;
pub mod fmp4;
pub mod settings;
pub mod synthetic;
pub mod writer;

#[cfg(target_os = "macos")]
pub mod mac;
#[cfg(windows)]
pub mod windows;

pub use cpu::{CpuFrame, PixelFormat};
pub use encoder::{
    hardware_encoder_unavailable, is_hardware_encoder_unavailable, is_no_frames, no_frames_error,
    AppendOutcome, EncoderConfig, VideoEncoder, HARDWARE_ENCODER_UNAVAILABLE,
    HARDWARE_ENCODER_UNAVAILABLE_MESSAGE, NO_FRAMES_MESSAGE,
};
pub use ffmpeg_vp9::FfmpegVp9Encoder;
pub use fmp4::{scan_bytes, scan_file, truncate_to_last_fragment, Fragment, Mp4Scan};
pub use settings::{
    bitrate_bps, disk_mb_per_minute, output_dimensions, plan_video, resolve_codec, smaller_plans,
    Codec, CodecChoice, OutputSize, Quality, VideoPlan, VideoSettings, FRAGMENT_SECS,
    MAX_H264_WIDTH,
};
pub use writer::{
    FragmentRecord, FragmentedMp4Writer, WriterHalt, WriterOptions, WriterSummary, QUEUE_SEGMENTS,
    SLOW_DISK_LIMIT,
};

/// The platform's hardware encoder.
#[cfg(target_os = "macos")]
pub use mac::{HardwareEncoder, MacFrame};
#[cfg(windows)]
pub use windows::HardwareEncoder;
