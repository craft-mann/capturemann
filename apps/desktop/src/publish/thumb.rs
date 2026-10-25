//! The thumbnail picker's frames: one picture from the export at a position, scaled for
//! YouTube (1280 px wide, JPEG, far under the 2 MB limit). The decoder is FFmpeg, found the way
//! the exporter finds it; there is no other decoder in the app.

use std::path::{Path, PathBuf};
use std::process::Command;

use cm_i18n::tr;

use crate::recording::files::{poster_path, run_with_timeout, RunError, POSTER_TIMEOUT};

/// Why a frame could not be picked. The words are made when drawn ([`ExtractError::text`]), so a
/// note on screen follows a language switch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtractError {
    /// No FFmpeg next to the app or on the PATH.
    NoFfmpeg,
    /// FFmpeg ran but wrote no frame.
    NoFrame,
    /// FFmpeg could not be run (the OS text).
    Ffmpeg(RunError),
    /// The folder for the frame could not be made (the OS text).
    Io(String),
}

impl ExtractError {
    pub fn text(&self) -> String {
        match self {
            ExtractError::NoFfmpeg => tr!(
                "FFmpeg was not found, so a frame cannot be picked. Choose an image file instead."
            ),
            ExtractError::NoFrame => tr!("FFmpeg could not read a frame at that position."),
            ExtractError::Ffmpeg(e) => tr!("FFmpeg: {error}", error = e.to_string()),
            ExtractError::Io(e) => e.clone(),
        }
    }
}

/// The slider's arrow keys move this share of the video; with Shift, [`BIG_STEP`].
pub const KEY_STEP: f32 = 0.01;
pub const BIG_STEP: f32 = 0.10;
/// The last moment a frame may be taken at, before the end of the video.
const END_MARGIN_SECS: f64 = 0.25;
/// YouTube recommends 1280 x 720.
pub const WIDTH: u32 = 1280;

/// Where the picked frame is written: next to the posters, one file per recording.
pub fn frame_path(root: &Path, capture_id: &str) -> PathBuf {
    let poster = poster_path(root, capture_id);
    poster.with_file_name(format!(
        "{}-publish-thumb.jpg",
        poster
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("recording")
    ))
}

/// The time of the slider position `fraction` (0 to 1) in a video of `duration_s`. A frame is
/// never asked for at the very end, where a decoder may have none to give.
pub fn position_secs(duration_s: f64, fraction: f32) -> f64 {
    let d = duration_s.max(0.0);
    (d * f64::from(fraction.clamp(0.0, 1.0)))
        .min((d - END_MARGIN_SECS).max(0.0))
        .max(0.0)
}

/// The FFmpeg arguments that write one frame (before the output path).
pub fn ffmpeg_args(video: &Path, at_secs: f64) -> Vec<String> {
    vec![
        "-nostdin".into(),
        "-y".into(),
        "-v".into(),
        "error".into(),
        "-ss".into(),
        format!("{at_secs:.3}"),
        "-i".into(),
        video.display().to_string(),
        "-frames:v".into(),
        "1".into(),
        "-vf".into(),
        format!("scale='min({WIDTH},iw)':-2"),
        "-q:v".into(),
        "3".into(),
    ]
}

/// Writes the frame of `video` at `at_secs` to `out`.
pub fn extract(video: &Path, at_secs: f64, out: &Path) -> Result<(), ExtractError> {
    let tools = cm_media::tools::locate().map_err(|_| ExtractError::NoFfmpeg)?;
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir).map_err(|e| ExtractError::Io(e.to_string()))?;
    }
    let _ = std::fs::remove_file(out);
    let mut cmd = Command::new(tools.ffmpeg);
    cmd.args(ffmpeg_args(video, at_secs)).arg(out);
    match run_with_timeout(cmd, POSTER_TIMEOUT) {
        Ok(true) if out.is_file() => Ok(()),
        Ok(_) => Err(ExtractError::NoFrame),
        Err(e) => Err(ExtractError::Ffmpeg(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_slider_maps_to_a_time_inside_the_video() {
        assert_eq!(position_secs(100.0, 0.0), 0.0);
        assert_eq!(position_secs(100.0, 0.5), 50.0);
        assert!(
            (position_secs(100.0, 1.0) - 99.75).abs() < 1e-9,
            "never the very end"
        );
        assert_eq!(position_secs(100.0, 7.0), 99.75, "clamped");
        assert_eq!(position_secs(100.0, -3.0), 0.0);
        assert_eq!(position_secs(-1.0, 0.5), 0.0);
        assert_eq!(
            position_secs(0.1, 1.0),
            0.0,
            "a clip shorter than the margin"
        );
    }

    #[test]
    fn the_command_seeks_before_the_input_and_scales_to_1280() {
        let a = ffmpeg_args(Path::new("/lib/a.mp4"), 12.5);
        let at = a.iter().position(|x| x == "-ss").unwrap();
        let input = a.iter().position(|x| x == "-i").unwrap();
        assert!(at < input, "a fast seek");
        assert_eq!(a[at + 1], "12.500");
        assert!(a.iter().any(|x| x.contains("1280")));
        assert!(a.contains(&"-frames:v".to_owned()));
    }

    #[test]
    fn the_frame_lives_next_to_the_posters_and_is_a_jpeg() {
        let p = frame_path(Path::new("/lib"), "rec_01");
        assert!(p.starts_with("/lib/.capturemann/posters"));
        assert_eq!(p.extension().and_then(|e| e.to_str()), Some("jpg"));
        assert!(p.to_string_lossy().contains("rec_01"));
    }
}
