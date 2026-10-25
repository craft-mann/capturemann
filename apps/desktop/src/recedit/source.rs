//! Where the preview's pictures and sound come from. A [`FrameSource`] gives the screen (and
//! camera) picture at a session time: [`DecodedFrames`] streams them from the recording's master
//! files with `cm_media::decode` (one long-lived FFmpeg process per track, so playback keeps up
//! and a scrub is one restart), [`SyntheticFrames`] is a frame-counter pattern for when there is
//! no FFmpeg. [`audio_files`] lists the audio tracks the editor mixes and plays.
//!
//! Times are **session time**; the decoder maps them into the files with `cm_core::track_time`,
//! the same mapping the exporter uses (pauses removed, each track's own start).

use std::path::Path;

use cm_core::{MediaTime, TICKS_PER_SECOND};
use cm_i18n::tr;
use cm_library::manifest::RecManifest;
use cm_media::decode::{AudioFile, PixelFormat, PreviewDecoder, PreviewTrack, VideoFrame};
use cm_media::edit_plan::AudioKind;
use image::RgbaImage;

use crate::preview::frame::{nv12_to_bgra, SyntheticSource};
use crate::recedit::render::PREVIEW_WIDTH;

/// The pictures for one preview frame.
pub struct SourceFrames {
    pub screen: RgbaImage,
    pub camera: Option<RgbaImage>,
}

/// Something that can show the recording at a session time.
pub trait FrameSource: Send {
    /// Size of the screen picture, for the preview's aspect ratio.
    fn size(&self) -> (u32, u32);
    /// The pictures at `session`. An error leaves the previous picture on screen.
    fn frame_at(&mut self, session: MediaTime) -> Result<SourceFrames, String>;
    /// What the user is looking at, for the status strip.
    fn label(&self) -> String;
}

/// A frame-counter pattern (the 1080p60 test's source, at 1280x720): moves with the time, so
/// scrubbing, cuts and speed are visible without any media.
pub struct SyntheticFrames {
    src: SyntheticSource,
}

impl Default for SyntheticFrames {
    fn default() -> Self {
        SyntheticFrames {
            src: SyntheticSource::new(1280, 720),
        }
    }
}

impl FrameSource for SyntheticFrames {
    fn size(&self) -> (u32, u32) {
        (1280, 720)
    }

    fn frame_at(&mut self, session: MediaTime) -> Result<SourceFrames, String> {
        let n = (session.ticks().max(0) * 30 / TICKS_PER_SECOND) as u32;
        let nv12 = self.src.frame(n);
        let mut bgra = nv12_to_bgra(&nv12);
        for px in bgra.as_chunks_mut::<4>().0 {
            px.swap(0, 2);
        }
        let screen = RgbaImage::from_raw(nv12.width as u32, nv12.height as u32, bgra)
            .ok_or("bad synthetic frame")?;
        Ok(SourceFrames {
            screen,
            camera: None,
        })
    }

    fn label(&self) -> String {
        tr!("Test pattern (FFmpeg not found)")
    }
}

/// Pictures streamed from the master files by the continuous decoder.
pub struct DecodedFrames {
    decoder: PreviewDecoder,
    size: (u32, u32),
}

impl DecodedFrames {
    pub fn new(decoder: PreviewDecoder, size: (u32, u32)) -> DecodedFrames {
        DecodedFrames { decoder, size }
    }

    /// How many times the screen stream was restarted (a jump, not a frame step).
    pub fn seeks(&self) -> u32 {
        self.decoder.screen_seeks()
    }
}

fn image_of(f: &VideoFrame) -> Result<RgbaImage, String> {
    RgbaImage::from_raw(f.width, f.height, f.data.clone())
        .ok_or_else(|| "bad frame size".to_owned())
}

impl FrameSource for DecodedFrames {
    fn size(&self) -> (u32, u32) {
        self.size
    }

    fn frame_at(&mut self, session: MediaTime) -> Result<SourceFrames, String> {
        let f = self.decoder.frames_at(session).map_err(|e| e.to_string())?;
        Ok(SourceFrames {
            screen: image_of(&f.screen)?,
            // A camera that has not started or has ended shows no bubble.
            camera: f.camera.as_deref().and_then(|c| image_of(c).ok()),
        })
    }

    fn label(&self) -> String {
        tr!("Decoded from the recording with FFmpeg")
    }
}

fn track_json<'a>(m: &'a RecManifest, name: &str) -> Option<&'a serde_json::Value> {
    let t = m.tracks.get(name)?;
    (t.get("missing").and_then(serde_json::Value::as_bool) != Some(true)).then_some(t)
}

fn start_of(t: &serde_json::Value) -> MediaTime {
    MediaTime::from_ticks(
        t.get("start")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0),
    )
}

/// A video track that is on disk, with the size and frame rate the decoder needs.
fn video_track(dir: &Path, m: &RecManifest, name: &str) -> Option<PreviewTrack> {
    let t = track_json(m, name)?;
    let file = dir.join(t.get("file")?.as_str()?);
    let dim = |i: usize| {
        t.get("size")
            .and_then(serde_json::Value::as_array)
            .and_then(|a| a.get(i))
            .and_then(serde_json::Value::as_u64)
    };
    let size = match (dim(0), dim(1)) {
        (Some(w), Some(h)) if w > 0 && h > 0 => (w as u32, h as u32),
        _ if name == "screen" => (1920, 1080),
        _ => (1280, 720),
    };
    let fps = t
        .get("fps")
        .and_then(serde_json::Value::as_f64)
        .filter(|f| *f >= 1.0)
        .unwrap_or(30.0);
    file.is_file().then(|| PreviewTrack {
        file,
        start: start_of(t),
        size,
        fps,
    })
}

/// The audio tracks on disk: the microphone (mono) and the system audio (stereo). The mic entry
/// already names the denoised file when denoising was on.
pub fn audio_files(dir: &Path, m: &RecManifest) -> Vec<AudioFile> {
    [
        ("mic", AudioKind::Mic, 1u16),
        ("system", AudioKind::System, 2u16),
    ]
    .into_iter()
    .filter_map(|(name, kind, channels)| {
        let t = track_json(m, name)?;
        let file = dir.join(t.get("file")?.as_str()?);
        file.is_file().then(|| AudioFile {
            kind,
            file,
            start: start_of(t),
            channels,
        })
    })
    .collect()
}

/// Screen size from the manifest (`tracks.screen.size`), 1920x1080 when absent.
pub fn screen_size(m: &RecManifest) -> (u32, u32) {
    let a = m
        .tracks
        .get("screen")
        .and_then(|s| s.get("size"))
        .and_then(serde_json::Value::as_array);
    let dim = |i: usize| a.and_then(|a| a.get(i)).and_then(serde_json::Value::as_u64);
    match (dim(0), dim(1)) {
        (Some(w), Some(h)) if w > 0 && h > 0 => (w as u32, h as u32),
        _ => (1920, 1080),
    }
}

/// The best source for the recording in `dir`: the streaming decoder over the screen (and
/// camera) masters, else the test pattern. The string says why when it is the pattern.
pub fn open(dir: &Path, m: &RecManifest) -> (Box<dyn FrameSource>, Option<String>) {
    let Some(screen) = video_track(dir, m, "screen") else {
        return (
            Box::new(SyntheticFrames::default()),
            Some(tr!("The recording has no screen track to show.")),
        );
    };
    match cm_media::tools::locate() {
        Ok(tools) => {
            let camera = video_track(dir, m, "camera");
            let decoder = PreviewDecoder::open(
                &tools.ffmpeg,
                &screen,
                camera.as_ref(),
                m.gaps.clone(),
                PREVIEW_WIDTH,
                PixelFormat::Rgba,
            );
            (Box::new(DecodedFrames::new(decoder, screen_size(m))), None)
        }
        Err(_) => (
            Box::new(SyntheticFrames::default()),
            Some(tr!(
                "FFmpeg is not installed, so the preview shows a test pattern."
            )),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn manifest(json: &str) -> RecManifest {
        RecManifest::from_json(json).unwrap()
    }

    /// The pattern moves with the time (a cut or a speed change is visible) and is repeatable.
    #[test]
    fn synthetic_frames_follow_the_time() {
        let mut s = SyntheticFrames::default();
        let a = s.frame_at(MediaTime::ZERO).unwrap();
        let b = s.frame_at(MediaTime::from_secs(1)).unwrap();
        let a2 = s.frame_at(MediaTime::ZERO).unwrap();
        assert_ne!(a.screen.as_raw(), b.screen.as_raw());
        assert_eq!(a.screen.as_raw(), a2.screen.as_raw());
        assert_eq!(a.screen.dimensions(), (1280, 720));
        assert!(a.camera.is_none());
    }

    #[test]
    fn size_comes_from_the_manifest_or_defaults() {
        let m = manifest(
            r#"{"schema":1,"id":"rec_1","title":"t","created":"2026-10-21T10:00:00Z",
                "tracks":{"screen":{"file":"screen.mp4","size":[3024,1964]}}}"#,
        );
        assert_eq!(screen_size(&m), (3024, 1964));
        let none =
            manifest(r#"{"schema":1,"id":"rec_1","title":"t","created":"2026-10-21T10:00:00Z"}"#);
        assert_eq!(screen_size(&none), (1920, 1080));
    }

    /// No screen track: the pattern, with the reason.
    #[test]
    fn open_without_a_screen_track_explains() {
        let dir = std::env::temp_dir();
        let m =
            manifest(r#"{"schema":1,"id":"rec_1","title":"t","created":"2026-10-21T10:00:00Z"}"#);
        let (src, why) = open(&dir, &m);
        assert!(why.unwrap().contains("no screen track"));
        assert_eq!(src.size(), (1280, 720));
    }

    /// Only audio files that exist are listed, mic mono and system stereo, with their starts.
    #[test]
    fn audio_files_lists_the_tracks_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("mic.wav"), b"x").unwrap();
        let m = manifest(
            r#"{"schema":1,"id":"rec_1","title":"t","created":"2026-10-21T10:00:00Z",
                "tracks":{"mic":{"file":"mic.wav","start":1200},
                          "system":{"file":"system.wav","start":0}}}"#,
        );
        let files = audio_files(dir.path(), &m);
        assert_eq!(files.len(), 1, "system.wav is not on disk");
        assert_eq!(files[0].kind, AudioKind::Mic);
        assert_eq!(files[0].channels, 1);
        assert_eq!(files[0].start, MediaTime::from_ticks(1200));
    }

    /// Playing an edited recording at 30 ticks a second with the editor's pacer, model and the real
    /// decoder (EDT-1.AC2): the cut is never shown, the picture follows the output time, and the
    /// stream restarts once for the jump over the cut, not once per picture. Skipped without FFmpeg.
    #[test]
    fn edt1_ac2_playing_across_a_cut_restarts_the_stream_once() {
        use crate::recedit::model::{Basis, EditorModel};
        use crate::recedit::pacer::{Pacer, Tick};
        use crate::recedit::snap::FrameGrid;
        use cm_core::{EditContext, EditList, Span};
        use std::time::{Duration, Instant};

        let Ok(tools) = cm_media::tools::locate() else {
            eprintln!("SKIPPED: FFmpeg is not installed");
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        // 4 s of 30 fps video with a running timestamp burned in as picture changes.
        let made = Command::new(&tools.ffmpeg)
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=320x240:rate=30",
            ])
            .args(["-t", "4", "-pix_fmt", "yuv420p", "-g", "30", "-y"])
            .arg(dir.path().join("screen.mp4"))
            .status()
            .unwrap();
        assert!(made.success());
        let m = manifest(
            r#"{"schema":1,"id":"rec_1","title":"t","created":"2026-10-21T10:00:00Z",
                "tracks":{"screen":{"file":"screen.mp4","size":[320,240],"fps":30,"start":0}}}"#,
        );
        let screen = video_track(dir.path(), &m, "screen").unwrap();
        let decoder = PreviewDecoder::open(
            &tools.ffmpeg,
            &screen,
            None,
            Vec::new(),
            PREVIEW_WIDTH,
            PixelFormat::Rgba,
        );
        let mut src = DecodedFrames::new(decoder, (320, 240));

        let basis = Basis {
            end: MediaTime::from_secs(4),
            live_at: None,
            gaps: Vec::new(),
        };
        let list = EditList::full(&EditContext {
            end: basis.end,
            live_at: None,
            gaps: &[],
        })
        .cut(&[Span::new(MediaTime::from_secs(1), MediaTime::from_secs(2))])
        .unwrap();
        let mut model = EditorModel::new(
            "rec_1",
            basis,
            list,
            FrameGrid::new(MediaTime::ZERO, Some(30.0)),
            (false, false),
        );
        let duration = model.output_duration().as_secs_f64();
        assert!((duration - 3.0).abs() < 1e-6);

        let t0 = Instant::now();
        let mut pacer = Pacer::default();
        pacer.start(0.0, t0);
        let mut shown = Vec::new();
        for i in 1..200u64 {
            let now = t0 + Duration::from_micros(i * 33_333);
            match pacer.tick(now, None, duration, false) {
                Tick::Play { out, .. } => {
                    assert!(model.set_output_time(out));
                    let f = src.frame_at(model.playhead).unwrap();
                    shown.push((model.playhead, f.screen.as_raw()[..64].to_vec()));
                }
                Tick::Ended => break,
                Tick::Looped => panic!("not looping"),
            }
        }
        assert!(shown.len() >= 85, "played {} pictures", shown.len());
        assert!(
            shown
                .iter()
                .all(|(t, _)| *t < MediaTime::from_secs(1) || *t >= MediaTime::from_secs(2)),
            "a picture from inside the cut was shown"
        );
        // Entering the file once and jumping the cut once: two starts, not ~90.
        assert!(src.seeks() <= 2, "{} restarts", src.seeks());
    }

    /// The streaming source gives a real picture, scaled to the preview width, and no picture
    /// past the end (skipped without FFmpeg).
    #[test]
    fn the_decoder_source_gives_pictures_from_a_file() {
        let Ok(tools) = cm_media::tools::locate() else {
            eprintln!("SKIPPED: FFmpeg is not installed");
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let made = Command::new(&tools.ffmpeg)
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=320x240:rate=30",
            ])
            .args(["-t", "2", "-pix_fmt", "yuv420p", "-y"])
            .arg(dir.path().join("screen.mp4"))
            .status()
            .unwrap();
        assert!(made.success());
        let m = manifest(
            r#"{"schema":1,"id":"rec_1","title":"t","created":"2026-10-21T10:00:00Z",
                "tracks":{"screen":{"file":"screen.mp4","size":[320,240],"fps":30,"start":0}}}"#,
        );
        let (mut src, why) = open(dir.path(), &m);
        assert!(why.is_none());
        let f = src.frame_at(MediaTime::from_secs(1)).unwrap();
        assert_eq!(f.screen.dimensions(), (320, 240));
        assert!(f.camera.is_none());
        assert!(src.frame_at(MediaTime::from_secs(30)).is_err());
    }
}
