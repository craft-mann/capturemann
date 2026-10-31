//! The video codec of a recorded track, as the manifest names it (`tracks.screen.details.codec`).
//!
//! The manifest keeps track facts in a free-form `details` object ([`crate::TrackInfo`]), so adding a
//! codec needs no schema bump: the value is a lower-case string, and nothing in a library or an older
//! app rejects a name it does not know (readers use [`VideoCodec::from_name`], which answers `None`
//! for an unknown string). Linux recordings written by the software VP9 encoder use `"vp9"` in an
//! MP4 file (specs/plans/linux-recording-plan.md, D-R2).

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// The codec of a video file inside a bundle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VideoCodec {
    Hevc,
    H264,
    /// Software VP9 in MP4 (the Linux fallback when no hardware encoder is usable).
    Vp9,
}

impl VideoCodec {
    /// The name written in the manifest.
    pub fn as_str(self) -> &'static str {
        match self {
            VideoCodec::Hevc => "hevc",
            VideoCodec::H264 => "h264",
            VideoCodec::Vp9 => "vp9",
        }
    }

    /// Parses a manifest name (also the usual aliases). `None` for a name this build does not know.
    pub fn from_name(name: &str) -> Option<VideoCodec> {
        match name.trim().to_ascii_lowercase().as_str() {
            "hevc" | "h265" | "h.265" => Some(VideoCodec::Hevc),
            "h264" | "h.264" | "avc" | "avc1" => Some(VideoCodec::H264),
            "vp9" | "vp09" => Some(VideoCodec::Vp9),
            _ => None,
        }
    }

    /// The codec of a track's `details` object; `None` when absent or unknown.
    pub fn of_details(details: &Value) -> Option<VideoCodec> {
        details
            .get("codec")?
            .as_str()
            .and_then(VideoCodec::from_name)
    }
}

/// Where the mouse pointer of a screen track is, when it is drawn at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorSource {
    /// Not recorded.
    None,
    /// Baked into the video frames; there is no cursor log, so the editor cannot add click or
    /// follow-cursor effects from it.
    Pixels,
    /// A separate cursor track with positions and clicks.
    Track,
}

impl CursorSource {
    pub fn as_str(self) -> &'static str {
        match self {
            CursorSource::None => "none",
            CursorSource::Pixels => "pixels",
            CursorSource::Track => "track",
        }
    }
}

/// Says in `details` which codec and encoder the file really holds, and where the cursor is.
///
/// Adds `codec`, `encoder`, `software` (true for a CPU encoder) and `cursor_source`; `cursor` keeps
/// its meaning of "a pointer was asked for" and is set to `false` when the file has none. Existing
/// keys are overwritten, others are kept. Does nothing when `details` is not an object.
pub fn describe_video_track(
    details: &mut Value,
    codec: VideoCodec,
    encoder: &str,
    software: bool,
    cursor: CursorSource,
) {
    let Some(map) = details.as_object_mut() else {
        return;
    };
    map.insert("codec".into(), json!(codec.as_str()));
    map.insert("encoder".into(), json!(encoder));
    map.insert("software".into(), json!(software));
    map.insert("cursor_source".into(), json!(cursor.as_str()));
    if cursor == CursorSource::None {
        map.insert("cursor".into(), json!(false));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MediaTime, TrackInfo, TrackKind, TrackStats};

    #[test]
    fn names_round_trip() {
        for c in [VideoCodec::Hevc, VideoCodec::H264, VideoCodec::Vp9] {
            assert_eq!(VideoCodec::from_name(c.as_str()), Some(c));
            let s = serde_json::to_string(&c).unwrap();
            assert_eq!(s, format!("\"{}\"", c.as_str()));
            assert_eq!(serde_json::from_str::<VideoCodec>(&s).unwrap(), c);
        }
        assert_eq!(VideoCodec::from_name("AVC1"), Some(VideoCodec::H264));
        assert_eq!(VideoCodec::from_name("av1"), None);
    }

    #[test]
    fn vp9_track_info_round_trips_and_old_ones_still_parse() {
        let mut details = json!({"size": [1280, 720], "fps": 30, "codec": "h264", "cursor": true});
        describe_video_track(
            &mut details,
            VideoCodec::Vp9,
            "libvpx-vp9",
            true,
            CursorSource::None,
        );
        let info = TrackInfo {
            kind: TrackKind::Screen,
            file: "screen.mp4".into(),
            start: MediaTime::from_ticks(0),
            end: MediaTime::from_ticks(1_200_000),
            details,
            stats: TrackStats {
                delivered: 1,
                written: 1,
                dropped: 0,
                late: 0,
            },
        };
        let text = serde_json::to_string(&info).unwrap();
        let back: TrackInfo = serde_json::from_str(&text).unwrap();
        assert_eq!(back, info);
        assert_eq!(VideoCodec::of_details(&back.details), Some(VideoCodec::Vp9));
        assert_eq!(back.details["software"], true);
        assert_eq!(back.details["cursor"], false);
        assert_eq!(back.details["cursor_source"], "none");

        // A manifest entry from before this change: no encoder, no cursor_source.
        let old = json!({"kind": "screen", "file": "screen.mp4", "start": 0, "end": 120000,
            "details": {"size": [1920, 1080], "fps": 60, "codec": "hevc", "cursor": true},
            "stats": {"delivered": 1, "written": 1, "dropped": 0, "late": 0}});
        let old: TrackInfo = serde_json::from_value(old).unwrap();
        assert_eq!(VideoCodec::of_details(&old.details), Some(VideoCodec::Hevc));
        assert!(old.details.get("cursor_source").is_none());
    }

    #[test]
    fn unknown_codec_in_details_is_none_not_an_error() {
        assert_eq!(VideoCodec::of_details(&json!({"codec": "av1"})), None);
        assert_eq!(VideoCodec::of_details(&json!({})), None);
    }
}
