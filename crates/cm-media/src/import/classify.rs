//! The status of a probed file (IMP-1): Ready, Prepared for editing, or Can't import.

use super::probe::{Family, ProbedFile};
use super::{Problem, Reason};

/// Video codecs the bundled FFmpeg decodes (`DECODERS` in `packaging/ffmpeg/build-macos.sh`).
pub const VIDEO_CODECS: [&str; 2] = ["h264", "hevc"];
/// Audio codecs the bundled FFmpeg decodes: AAC, MP3 (`mp3float`, since 0.3.1) and PCM (8-bit unsigned, 16, 24 and 32-bit
/// integer, 32-bit float, little endian).
pub const AUDIO_CODECS: [&str; 7] = [
    "aac",
    "mp3",
    "pcm_u8",
    "pcm_s16le",
    "pcm_s24le",
    "pcm_s32le",
    "pcm_f32le",
];
/// Pixel formats used as they are (8-bit 4:2:0).
const PLAIN_PIX_FMTS: [&str; 3] = ["yuv420p", "yuvj420p", "nv12"];

/// Video or audio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind {
    Video,
    Audio,
}

impl MediaKind {
    pub fn as_str(self) -> &'static str {
        match self {
            MediaKind::Video => "video",
            MediaKind::Audio => "audio",
        }
    }
}

/// Why a video gets an editing copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrepareReason {
    VariableFrameRate,
    ContainerMkv,
    Rotated,
    PixelFormat,
}

impl PrepareReason {
    pub fn as_str(self) -> &'static str {
        match self {
            PrepareReason::VariableFrameRate => "variable_frame_rate",
            PrepareReason::ContainerMkv => "container_mkv",
            PrepareReason::Rotated => "rotated",
            PrepareReason::PixelFormat => "pixel_format",
        }
    }

    /// For the info note of the sheet.
    pub fn describe(self) -> &'static str {
        match self {
            PrepareReason::VariableFrameRate => "variable frame rate",
            PrepareReason::ContainerMkv => "MKV container",
            PrepareReason::Rotated => "rotated picture",
            PrepareReason::PixelFormat => "10-bit or non-standard colour format",
        }
    }
}

/// The status of a file.
#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    /// Used as it is.
    Ready,
    /// Kept as the master, with a constant-frame-rate editing copy made next to it.
    Prepared(Vec<PrepareReason>),
    CantImport(Problem),
}

impl Status {
    pub fn as_str(&self) -> &'static str {
        match self {
            Status::Ready => "ready",
            Status::Prepared(_) => "prepared",
            Status::CantImport(_) => "cant_import",
        }
    }

    pub fn is_importable(&self) -> bool {
        !matches!(self, Status::CantImport(_))
    }
}

/// The result of [`classify`].
#[derive(Debug, Clone, PartialEq)]
pub struct Classification {
    pub kind: MediaKind,
    pub status: Status,
}

fn cant(kind: MediaKind, p: Problem) -> Classification {
    Classification {
        kind,
        status: Status::CantImport(p),
    }
}

/// Decides what to do with a probed file. Pure.
pub fn classify(f: &ProbedFile) -> Classification {
    let kind = if f.video.is_some() {
        MediaKind::Video
    } else {
        MediaKind::Audio
    };
    if f.protected {
        return cant(
            kind,
            Problem::new(
                Reason::ProtectedMedia,
                "This file is protected (DRM) and can't be imported.",
            ),
        );
    }
    if f.video.is_none() && f.audio.is_none() {
        return cant(
            kind,
            Problem::new(
                Reason::NoStreams,
                "The file has no video or audio that CaptureMann can read.",
            ),
        );
    }
    if f.duration_s <= 0.0 {
        return cant(
            kind,
            Problem::new(Reason::UnreadableFile, "The file has no content to import."),
        );
    }
    let family = f.family();
    // Containers: video in MP4/MOV/M4V or Matroska; audio in MP4/M4A, WAV or MP3.
    let container_ok = matches!(
        (kind, family),
        (MediaKind::Video, Family::Mov | Family::Matroska)
            | (MediaKind::Audio, Family::Mov | Family::Wav | Family::Mp3)
    );
    if let Some(v) = &f.video {
        if !VIDEO_CODECS.contains(&v.codec.as_str()) {
            return cant(kind, Problem::unsupported_codec("video", &v.codec));
        }
    }
    if let Some(a) = &f.audio {
        if !AUDIO_CODECS.contains(&a.codec.as_str()) {
            return cant(kind, Problem::unsupported_codec("audio", &a.codec));
        }
    }
    if !container_ok {
        let name = f.container.split(',').next().unwrap_or("this").to_owned();
        return cant(kind, Problem::unsupported_format(&name));
    }
    let Some(v) = &f.video else {
        return Classification {
            kind,
            status: Status::Ready,
        };
    };
    let mut why = Vec::new();
    if v.variable_frame_rate {
        why.push(PrepareReason::VariableFrameRate);
    }
    if family == Family::Matroska {
        why.push(PrepareReason::ContainerMkv);
    }
    if v.rotation != 0 {
        why.push(PrepareReason::Rotated);
    }
    if v.pix_fmt
        .as_deref()
        .is_some_and(|p| !PLAIN_PIX_FMTS.contains(&p))
    {
        why.push(PrepareReason::PixelFormat);
    }
    Classification {
        kind,
        status: if why.is_empty() {
            Status::Ready
        } else {
            Status::Prepared(why)
        },
    }
}

#[cfg(test)]
mod tests {
    use super::super::probe::{AudioStream, VideoStream};
    use super::*;
    use std::path::PathBuf;

    fn video(codec: &str) -> VideoStream {
        VideoStream {
            index: 0,
            codec: codec.into(),
            width: 1920,
            height: 1080,
            fps: Some(30.0),
            rotation: 0,
            pix_fmt: Some("yuv420p".into()),
            hdr: false,
            color_space: None,
            variable_frame_rate: false,
        }
    }

    fn audio(codec: &str) -> AudioStream {
        AudioStream {
            index: 1,
            audio_index: 0,
            codec: codec.into(),
            sample_rate: 48000,
            channels: 2,
            duration_s: None,
        }
    }

    fn file(container: &str, v: Option<VideoStream>, a: Option<AudioStream>) -> ProbedFile {
        ProbedFile {
            path: PathBuf::from("/x/f"),
            size_bytes: 1000,
            container: container.into(),
            duration_s: 10.0,
            video: v,
            audio: a,
            other_audio_streams: 0,
            protected: false,
            title: None,
        }
    }

    const MOV: &str = "mov,mp4,m4a,3gp,3g2,mj2";

    fn reason(c: &Classification) -> &'static str {
        match &c.status {
            Status::CantImport(p) => p.reason.as_str(),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn supported_files_are_ready() {
        for (cont, v, a, kind) in [
            (MOV, Some("h264"), Some("aac"), MediaKind::Video),
            (MOV, Some("hevc"), Some("pcm_s16le"), MediaKind::Video),
            (MOV, Some("h264"), None, MediaKind::Video),
            (MOV, None, Some("aac"), MediaKind::Audio),
            ("wav", None, Some("pcm_s24le"), MediaKind::Audio),
            ("wav", None, Some("pcm_f32le"), MediaKind::Audio),
        ] {
            let c = classify(&file(cont, v.map(video), a.map(audio)));
            assert_eq!(c.status, Status::Ready, "{cont} {v:?} {a:?}");
            assert_eq!(c.kind, kind);
        }
    }

    #[test]
    fn prepared_reasons_are_listed() {
        let mut v = video("h264");
        v.variable_frame_rate = true;
        v.rotation = 90;
        v.pix_fmt = Some("yuv420p10le".into());
        let c = classify(&file("matroska,webm", Some(v), Some(audio("aac"))));
        assert_eq!(
            c.status,
            Status::Prepared(vec![
                PrepareReason::VariableFrameRate,
                PrepareReason::ContainerMkv,
                PrepareReason::Rotated,
                PrepareReason::PixelFormat
            ])
        );
        // One reason is enough.
        let mut v = video("hevc");
        v.rotation = 180;
        let c = classify(&file(MOV, Some(v), None));
        assert_eq!(c.status, Status::Prepared(vec![PrepareReason::Rotated]));
        // HDR alone does not prepare the file.
        let mut v = video("hevc");
        v.hdr = true;
        assert_eq!(classify(&file(MOV, Some(v), None)).status, Status::Ready);
    }

    #[test]
    fn unsupported_codecs_and_containers_are_named() {
        let c = classify(&file(
            "matroska,webm",
            Some(video("vp9")),
            Some(audio("opus")),
        ));
        assert_eq!(reason(&c), "unsupported_format");
        let Status::CantImport(p) = &c.status else {
            panic!()
        };
        assert_eq!(p.codec.as_deref(), Some("vp9"));
        // An AC-3 track never gets dropped silently.
        let c = classify(&file(MOV, Some(video("h264")), Some(audio("ac3"))));
        let Status::CantImport(p) = &c.status else {
            panic!()
        };
        assert_eq!(
            (p.reason, p.codec.as_deref()),
            (Reason::UnsupportedFormat, Some("ac3"))
        );
        // A WAV of 64-bit floats has no decoder in the shipped build.
        assert_eq!(
            reason(&classify(&file("wav", None, Some(audio("pcm_f64le"))))),
            "unsupported_format"
        );
        // Audio in Matroska, video in WAV.
        let c = classify(&file("matroska,webm", None, Some(audio("aac"))));
        let Status::CantImport(p) = &c.status else {
            panic!()
        };
        assert_eq!(p.format.as_deref(), Some("matroska"));
        assert_eq!(
            reason(&classify(&file("avi", Some(video("h264")), None))),
            "unsupported_format"
        );
    }

    #[test]
    fn protected_empty_and_streamless_files_cannot_be_imported() {
        let mut f = file(MOV, Some(video("h264")), Some(audio("aac")));
        f.protected = true;
        assert_eq!(reason(&classify(&f)), "protected_media");
        assert_eq!(reason(&classify(&file(MOV, None, None))), "no_streams");
        let mut f = file("wav", None, Some(audio("pcm_s16le")));
        f.duration_s = 0.0;
        assert_eq!(reason(&classify(&f)), "unreadable_file");
        assert!(!classify(&f).status.is_importable());
    }

    #[test]
    fn mp3_is_ready_as_audio_and_only_as_audio() {
        let c = classify(&file("mp3", None, Some(audio("mp3"))));
        assert_eq!((c.kind, c.status), (MediaKind::Audio, Status::Ready));
        // Bare AAC stays refused (no demuxer; ffprobe cannot open it, the probe names it).
        assert_eq!(
            reason(&classify(&file("aac", None, Some(audio("aac"))))),
            "unsupported_format"
        );
        // An MP3 stream is audio only; a video in an MP3 container is not a thing we read.
        assert_eq!(
            reason(&classify(&file(
                "mp3",
                Some(video("h264")),
                Some(audio("mp3"))
            ))),
            "unsupported_format"
        );
        // Another codec in the MP3 container is named by its codec.
        assert_eq!(
            reason(&classify(&file("mp3", None, Some(audio("ac3"))))),
            "unsupported_format"
        );
    }
}
