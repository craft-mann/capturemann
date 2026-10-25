//! The shared contract between the recording session and each track source.
//!
//! The daemon's recording session owns one [`SessionClock`] and a set of [`TrackRecorder`]s (screen,
//! camera, mic, system audio, cursor). Each recorder writes its own file(s) into the bundle
//! directory and stamps everything with the session clock (specs/01-technical-design.md,
//! "Recording pipeline"). Pause stops writing all tracks and records a gap; resume continues in the
//! same files, so there is no join step.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::clock::SessionClock;
use crate::error::CmError;
use crate::media_time::MediaTime;

/// Which source a track comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrackKind {
    Screen,
    Camera,
    Mic,
    System,
    Cursor,
}

impl TrackKind {
    pub fn as_str(self) -> &'static str {
        match self {
            TrackKind::Screen => "screen",
            TrackKind::Camera => "camera",
            TrackKind::Mic => "mic",
            TrackKind::System => "system",
            TrackKind::Cursor => "cursor",
        }
    }
}

/// What a recorder gets when it starts.
#[derive(Debug, Clone)]
pub struct TrackContext {
    /// The bundle directory (`<title>.cmrec/`); recorders create their files directly inside it.
    pub dir: PathBuf,
    /// The session clock shared by every track.
    pub clock: SessionClock,
}

/// Counters a recorder reports for NFR-2 / NFR-3 checks and the HUD.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TrackStats {
    /// Frames (video) or buffers (audio) or events (cursor) delivered by the OS.
    pub delivered: u64,
    /// Written to the track file.
    pub written: u64,
    /// Dropped by the recorder (back-pressure, encoder not ready).
    pub dropped: u64,
    /// Written later than their deadline (video: more than one frame interval behind).
    pub late: u64,
}

/// The result of a finished track, recorded in the bundle manifest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackInfo {
    pub kind: TrackKind,
    /// File name relative to the bundle directory (e.g. `screen.mp4`).
    pub file: String,
    /// Session time of the first sample.
    pub start: MediaTime,
    /// Session time of the end of the last sample.
    pub end: MediaTime,
    /// Source-specific facts (size, fps, codec, sample rate, channels, ...), stored in the manifest.
    pub details: serde_json::Value,
    pub stats: TrackStats,
}

/// A pause gap on the session timeline: samples are not written between `from` and `to`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PauseGap {
    pub from: MediaTime,
    pub to: MediaTime,
}

/// One track source of a recording session. Implementations are headless; the engine behind each
/// is platform specific. All times are session times from the shared clock.
pub trait TrackRecorder: Send {
    fn kind(&self) -> TrackKind;

    /// Starts capturing into `ctx.dir`. The first stamped sample is at or after the session origin.
    fn start(&mut self, ctx: TrackContext) -> Result<(), CmError>;

    /// Stops writing samples from `at` on (device keeps running where restarting is slow).
    fn pause(&mut self, at: MediaTime) -> Result<(), CmError>;

    /// Resumes writing samples from `at` on, in the same file(s).
    fn resume(&mut self, at: MediaTime) -> Result<(), CmError>;

    /// Finishes the file(s) and returns what to record in the manifest. Also valid after an error.
    fn stop(&mut self, at: MediaTime) -> Result<TrackInfo, CmError>;

    /// Live counters.
    fn stats(&self) -> TrackStats;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_info_round_trips_as_json() {
        let info = TrackInfo {
            kind: TrackKind::Screen,
            file: "screen.mp4".into(),
            start: MediaTime::from_ticks(7060),
            end: MediaTime::from_ticks(1_207_060),
            details: serde_json::json!({"size": [3024, 1964], "fps": 60, "codec": "hevc"}),
            stats: TrackStats {
                delivered: 1200,
                written: 1198,
                dropped: 2,
                late: 3,
            },
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"kind\":\"screen\""));
        assert_eq!(serde_json::from_str::<TrackInfo>(&json).unwrap(), info);
    }

    #[test]
    fn kind_names_match_the_bundle_layout() {
        assert_eq!(TrackKind::Mic.as_str(), "mic");
        assert_eq!(TrackKind::System.as_str(), "system");
    }
}
