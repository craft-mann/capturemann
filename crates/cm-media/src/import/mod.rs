//! Importing media files that CaptureMann did not record (IMP-1, IMP-2; `specs/features/18-import.md`).
//!
//! This module is the FFmpeg side of the import: it never touches the library.
//!
//! * [`probe`]: reads a file with the bundled `ffprobe` into a [`ProbedFile`] (streams, duration,
//!   display size, rotation, variable frame rate, HDR, protection), and names the formats the
//!   bundled FFmpeg cannot open from their first bytes ([`probe::sniff_container`]).
//! * [`classify`]: the three statuses of the sheet, [`Status::Ready`], [`Status::Prepared`] (with
//!   the reasons) and [`Status::CantImport`] (with one [`Problem`]).
//! * [`prepare`]: the constant-frame-rate editing copy (hardware encoder only), the sound decoded
//!   once to `mic.wav`, and the verify step.
//!
//! # What the bundled FFmpeg reads (decided Nov 3, 2026)
//!
//! Video H.264 and HEVC in MP4, MOV, M4V and Matroska; audio AAC and PCM (8, 16, 24 and 32-bit
//! integer and 32-bit float little endian) in MP4/MOV/M4A and WAV. Everything else is recognised
//! and refused with [`Reason::UnsupportedFormat`], naming the format or codec; no decoder is
//! needed to name a codec. The lists are in [`classify`] and mirror `packaging/ffmpeg/build-macos.sh`.

pub mod classify;
pub mod prepare;
pub mod probe;

pub use classify::{classify, Classification, MediaKind, PrepareReason, Status};
pub use probe::{probe, AudioStream, ProbedFile, VideoStream};

use serde_json::{json, Value};

/// Why a file cannot be imported. These are the `data.reason` values of the shared errors
/// (`specs/README.md`, "Reasons").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Reason {
    /// The container or a codec is recognised and not supported yet.
    UnsupportedFormat,
    /// DRM or encryption.
    ProtectedMedia,
    /// Truncated, corrupt, empty, or it changed while being read.
    UnreadableFile,
    /// A readable file with no usable video or audio stream.
    NoStreams,
}

impl Reason {
    pub fn as_str(self) -> &'static str {
        match self {
            Reason::UnsupportedFormat => "unsupported_format",
            Reason::ProtectedMedia => "protected_media",
            Reason::UnreadableFile => "unreadable_file",
            Reason::NoStreams => "no_streams",
        }
    }
}

/// A file that cannot be imported, with what to tell the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub reason: Reason,
    /// English sentence for the row and the error message.
    pub message: String,
    /// The container (`avi`, `mp3`, `matroska`) for `UnsupportedFormat`.
    pub format: Option<String>,
    /// The codec (`vp9`, `opus`, `ac3`) for `UnsupportedFormat`.
    pub codec: Option<String>,
}

impl Problem {
    pub fn new(reason: Reason, message: impl Into<String>) -> Problem {
        Problem {
            reason,
            message: message.into(),
            format: None,
            codec: None,
        }
    }

    pub fn unsupported_format(format: &str) -> Problem {
        Problem {
            reason: Reason::UnsupportedFormat,
            message: format!("{} files are not supported yet.", display_name(format)),
            format: Some(format.to_owned()),
            codec: None,
        }
    }

    pub fn unsupported_codec(kind: &str, codec: &str) -> Problem {
        Problem {
            reason: Reason::UnsupportedFormat,
            message: format!(
                "The {kind} codec {} is not supported yet.",
                display_name(codec)
            ),
            format: None,
            codec: Some(codec.to_owned()),
        }
    }

    /// `data` of the shared error: `{ reason, format?, codec? }`.
    pub fn data(&self) -> Value {
        let mut v = json!({ "reason": self.reason.as_str() });
        if let Some(f) = &self.format {
            v["format"] = json!(f);
        }
        if let Some(c) = &self.codec {
            v["codec"] = json!(c);
        }
        v
    }

    /// The shared error: always `invalid_input`; the reason travels in [`Problem::data`].
    pub fn to_error(&self) -> cm_core::CmError {
        cm_core::CmError::invalid_input(self.message.clone())
    }
}

/// `mp3` -> `MP3`, `matroska` -> `Matroska`: how a format is named in a sentence.
fn display_name(s: &str) -> String {
    match s {
        "mp3" | "avi" | "aac" | "flac" | "aiff" | "ogg" | "webm" | "vp8" | "vp9" | "ac3"
        | "eac3" | "dts" | "wmv" | "asf" | "flv" | "mpeg" | "opus" | "av1" => s.to_uppercase(),
        other => {
            let mut c = other.chars();
            c.next()
                .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
                .unwrap_or_default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn problems_name_the_format_and_carry_data() {
        let p = Problem::unsupported_format("avi");
        assert_eq!(p.message, "AVI files are not supported yet.");
        assert_eq!(
            p.data(),
            json!({"reason": "unsupported_format", "format": "avi"})
        );
        let c = Problem::unsupported_codec("video", "vp9");
        assert_eq!(c.data()["codec"], "vp9");
        assert_eq!(c.to_error().code(), cm_core::ErrorCode::InvalidInput);
        assert_eq!(Reason::ProtectedMedia.as_str(), "protected_media");
    }
}
