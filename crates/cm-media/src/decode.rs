//! Continuous preview decoding for the recording editor (EDT-1 to EDT-3 preview).
//!
//! The editor used to start one FFmpeg process per picture. Here each track has one long-lived
//! FFmpeg process that streams raw frames; a small cache and a forward-skip window turn
//! "give me the picture at time t" into cheap reads while playing and a restart (seek) only when
//! the request jumps. Times are **session time**; [`PreviewDecoder`] maps them into each track
//! file with [`cm_core::track_time`], the mapping the exporter uses (pauses removed, each track's
//! own start).
//!
//! * [`FrameStream`] is the process boundary (a fake in tests); [`FfmpegStream`] is the real one.
//! * [`TrackDecoder`] owns the cache and the seek-or-read-forward decision ([`plan`]).
//! * [`PreviewDecoder`] is a screen track plus an optional camera track.
//! * [`AudioBank`] decodes the audio tracks to PCM once and renders the mixed audio of an edit
//!   list on the output timeline with `cm_audio::mix` (the exporter's mix, so what is heard is
//!   what is exported).

use std::collections::VecDeque;
use std::fmt;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{sync_channel, Receiver};
use std::sync::Arc;
use std::thread::JoinHandle;

use cm_audio::mix::{mix_tracks, MixParams, MixedAudio, TrackAudio};
use cm_core::{track_time, CmError, EditList, MediaTime, PauseGap, TICKS_PER_SECOND};

use crate::dsp_audio::decode_pcm;
use crate::edit_plan::AudioKind;
use crate::graph::AUDIO_RATE;

/// How far ahead of the stream a request may be and still be reached by reading forward instead
/// of restarting the process. Decoding 0.75 s of frames is cheaper than a seek at preview sizes.
pub const SKIP_WINDOW: MediaTime = MediaTime::from_millis(750);

/// Frames the stream thread decodes ahead of the reader.
pub const PREFETCH_FRAMES: usize = 4;

/// Recent frames kept so scrubbing back a frame or two does not restart the stream.
pub const CACHE_FRAMES: usize = 6;

/// Pixel layout of a decoded frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    /// 4 bytes per pixel, straight alpha (always opaque for video).
    Rgba,
    /// Planar luma then interleaved CbCr at half resolution (what `surface()` shows).
    Nv12,
}

impl PixelFormat {
    /// Bytes in one `width` x `height` frame (both even for NV12).
    pub fn frame_len(self, width: u32, height: u32) -> usize {
        let px = width as usize * height as usize;
        match self {
            PixelFormat::Rgba => px * 4,
            PixelFormat::Nv12 => px + px / 2,
        }
    }

    fn ffmpeg_name(self) -> &'static str {
        match self {
            PixelFormat::Rgba => "rgba",
            PixelFormat::Nv12 => "nv12",
        }
    }
}

/// One decoded picture.
#[derive(Debug, Clone)]
pub struct VideoFrame {
    /// Position in the track file (file time), the start of the interval the picture shows.
    pub pos: MediaTime,
    pub width: u32,
    pub height: u32,
    pub format: PixelFormat,
    pub data: Vec<u8>,
}

/// What a stream decodes: the source picture size is scaled down to at most `max_width`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StreamSpec {
    pub src_size: (u32, u32),
    pub max_width: u32,
    /// Frames per second the stream is normalised to (the track's recorded rate).
    pub fps: f64,
    pub format: PixelFormat,
}

impl StreamSpec {
    /// Output size: the source aspect, at most `max_width` wide, both sides even.
    pub fn out_size(&self) -> (u32, u32) {
        let (w, h) = (self.src_size.0.max(2), self.src_size.1.max(2));
        let ow = w.min(self.max_width.max(2)) & !1;
        let oh = ((u64::from(ow) * u64::from(h) + u64::from(w) / 2) / u64::from(w)) as u32 & !1;
        (ow.max(2), oh.max(2))
    }

    /// Length of one frame in ticks.
    pub fn frame_ticks(&self) -> i64 {
        frame_ticks(self.fps)
    }
}

fn frame_ticks(fps: f64) -> i64 {
    ((TICKS_PER_SECOND as f64 / fps.clamp(1.0, 240.0)).round() as i64).max(1)
}

/// Why a picture could not be given.
#[derive(Debug, Clone, PartialEq)]
pub enum DecodeError {
    /// The track has not started at this time (a camera that joined late).
    NotStarted,
    /// The time is past the last frame.
    EndOfStream,
    /// FFmpeg could not be run or the file could not be read.
    Stream(String),
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecodeError::NotStarted => f.write_str("the track has not started at this time"),
            DecodeError::EndOfStream => f.write_str("the track has ended before this time"),
            DecodeError::Stream(e) => f.write_str(e),
        }
    }
}

impl std::error::Error for DecodeError {}

/// The process boundary: frames in order from a start position.
pub trait FrameStream: Send {
    /// (Re)starts decoding at file position `pos`; the next frame has `pos` as its position.
    fn start(&mut self, pos: MediaTime) -> Result<(), String>;
    /// The next frame, or `None` at the end of the file (or after a failure).
    fn next_frame(&mut self) -> Option<VideoFrame>;
}

/// What to do for a requested position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Plan {
    /// The frame at this cache index already shows the position.
    Cached(usize),
    /// The stream is just behind the position: read forward.
    Forward,
    /// Restart the stream at the position (a jump, or a step back out of the cache).
    Seek,
}

/// The seek math, pure. `cache` holds the positions of the cached frames, `dur` is one frame,
/// `next` is where the running stream delivers its next frame (`None` when it is not running).
pub fn plan(
    cache: &[MediaTime],
    dur: MediaTime,
    next: Option<MediaTime>,
    want: MediaTime,
    window: MediaTime,
) -> Plan {
    if let Some(i) = cache.iter().rposition(|p| *p <= want && want < *p + dur) {
        return Plan::Cached(i);
    }
    match next {
        Some(n) if want >= n && want - n <= window => Plan::Forward,
        _ => Plan::Seek,
    }
}

/// One track: a stream plus the cache and the seek decision.
pub struct TrackDecoder<S: FrameStream> {
    stream: S,
    dur: MediaTime,
    /// File position of the frame the stream delivers next; `None` while it is stopped.
    next: Option<MediaTime>,
    ended: bool,
    cache: VecDeque<Arc<VideoFrame>>,
    /// Restarts since creation, for tests and the status line.
    pub seeks: u32,
}

impl<S: FrameStream> TrackDecoder<S> {
    pub fn new(stream: S, fps: f64) -> TrackDecoder<S> {
        TrackDecoder {
            stream,
            dur: MediaTime::from_ticks(frame_ticks(fps)),
            next: None,
            ended: false,
            cache: VecDeque::with_capacity(CACHE_FRAMES),
            seeks: 0,
        }
    }

    fn push(&mut self, f: VideoFrame) -> Arc<VideoFrame> {
        self.next = Some(f.pos + self.dur);
        let f = Arc::new(f);
        if self.cache.len() == CACHE_FRAMES {
            self.cache.pop_front();
        }
        self.cache.push_back(f.clone());
        f
    }

    /// The picture that shows file position `want`.
    pub fn frame_at(&mut self, want: MediaTime) -> Result<Arc<VideoFrame>, DecodeError> {
        let want = want.max(MediaTime::ZERO);
        let positions: Vec<MediaTime> = self.cache.iter().map(|f| f.pos).collect();
        let next = if self.ended { None } else { self.next };
        match plan(&positions, self.dur, next, want, SKIP_WINDOW) {
            Plan::Cached(i) => return Ok(self.cache[i].clone()),
            Plan::Forward => {}
            Plan::Seek => {
                self.stream.start(want).map_err(DecodeError::Stream)?;
                self.seeks += 1;
                self.ended = false;
                self.next = Some(want);
                self.cache.clear();
            }
        }
        loop {
            let Some(f) = self.stream.next_frame() else {
                self.ended = true;
                // Within one frame of the last picture the last picture still shows.
                return match self.cache.back() {
                    Some(last) if want < last.pos + self.dur + self.dur => Ok(last.clone()),
                    _ => Err(DecodeError::EndOfStream),
                };
            };
            let covers = f.pos <= want && want < f.pos + self.dur;
            let f = self.push(f);
            if covers || f.pos > want {
                return Ok(f);
            }
        }
    }
}

// ---- the FFmpeg stream ------------------------------------------------------------------

struct Running {
    child: Child,
    rx: Receiver<VideoFrame>,
    reader: Option<JoinHandle<()>>,
}

impl Running {
    fn stop(mut self) {
        let _ = self.child.kill();
        // Dropping the receiver unblocks a reader parked in `send`.
        drop(self.rx);
        let _ = self.child.wait();
        if let Some(r) = self.reader.take() {
            let _ = r.join();
        }
    }
}

/// One FFmpeg process per `start`, decoding to raw frames on a thread with a small prefetch.
pub struct FfmpegStream {
    ffmpeg: PathBuf,
    file: PathBuf,
    spec: StreamSpec,
    running: Option<Running>,
}

impl FfmpegStream {
    pub fn new(ffmpeg: PathBuf, file: PathBuf, spec: StreamSpec) -> FfmpegStream {
        FfmpegStream {
            ffmpeg,
            file,
            spec,
            running: None,
        }
    }

    /// The FFmpeg arguments for a start at `pos` (pure, so tests can check them).
    pub fn args(&self, pos: MediaTime) -> Vec<String> {
        let (w, h) = self.spec.out_size();
        let pix = self.spec.format.ffmpeg_name();
        vec![
            "-hide_banner".into(),
            "-nostdin".into(),
            "-v".into(),
            "error".into(),
            "-ss".into(),
            format!("{:.3}", pos.as_secs_f64()),
            "-i".into(),
            self.file.to_string_lossy().into_owned(),
            "-an".into(),
            "-sn".into(),
            "-vf".into(),
            format!(
                "fps={:.5},scale={w}:{h}:flags=bilinear,format={pix}",
                self.spec.fps
            ),
            "-f".into(),
            "rawvideo".into(),
            "-pix_fmt".into(),
            pix.into(),
            "pipe:1".into(),
        ]
    }
}

impl FrameStream for FfmpegStream {
    fn start(&mut self, pos: MediaTime) -> Result<(), String> {
        if let Some(r) = self.running.take() {
            r.stop();
        }
        let mut child = Command::new(&self.ffmpeg)
            .args(self.args(pos))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("cannot run FFmpeg: {e}"))?;
        let Some(mut out) = child.stdout.take() else {
            let _ = child.kill();
            return Err("FFmpeg has no output pipe".to_owned());
        };
        let (w, h) = self.spec.out_size();
        let format = self.spec.format;
        let len = format.frame_len(w, h);
        let (tx, rx) = sync_channel::<VideoFrame>(PREFETCH_FRAMES);
        let fps = self.spec.fps;
        let reader = std::thread::Builder::new()
            .name("cm-preview-decode".into())
            .spawn(move || {
                let mut index: i64 = 0;
                loop {
                    let mut data = vec![0u8; len];
                    if out.read_exact(&mut data).is_err() {
                        return;
                    }
                    let at = pos.ticks()
                        + (index as f64 * TICKS_PER_SECOND as f64 / fps.clamp(1.0, 240.0)).round()
                            as i64;
                    index += 1;
                    let frame = VideoFrame {
                        pos: MediaTime::from_ticks(at),
                        width: w,
                        height: h,
                        format,
                        data,
                    };
                    if tx.send(frame).is_err() {
                        return;
                    }
                }
            })
            .map_err(|e| format!("cannot start the decode thread: {e}"))?;
        self.running = Some(Running {
            child,
            rx,
            reader: Some(reader),
        });
        Ok(())
    }

    fn next_frame(&mut self) -> Option<VideoFrame> {
        self.running.as_ref()?.rx.recv().ok()
    }
}

impl Drop for FfmpegStream {
    fn drop(&mut self) {
        if let Some(r) = self.running.take() {
            r.stop();
        }
    }
}

// ---- screen + camera --------------------------------------------------------------------

/// A recorded video track as the manifest describes it.
#[derive(Debug, Clone)]
pub struct PreviewTrack {
    pub file: PathBuf,
    /// `TrackInfo.start`: session time of the file's first sample.
    pub start: MediaTime,
    pub size: (u32, u32),
    pub fps: f64,
}

/// The pictures for one session time.
#[derive(Debug, Clone)]
pub struct PreviewFrames {
    pub screen: Arc<VideoFrame>,
    /// `None` before the camera started or after it ended: no bubble.
    pub camera: Option<Arc<VideoFrame>>,
}

struct Placed<S: FrameStream> {
    start: MediaTime,
    dec: TrackDecoder<S>,
}

/// Screen and camera decoders on the session clock.
pub struct PreviewDecoder<S: FrameStream = FfmpegStream> {
    gaps: Vec<PauseGap>,
    screen: Placed<S>,
    camera: Option<Placed<S>>,
}

impl PreviewDecoder<FfmpegStream> {
    /// Real decoding with `ffmpeg`. The screen is at most `max_width` wide, the camera at most
    /// half of it (it is a bubble).
    pub fn open(
        ffmpeg: &Path,
        screen: &PreviewTrack,
        camera: Option<&PreviewTrack>,
        gaps: Vec<PauseGap>,
        max_width: u32,
        format: PixelFormat,
    ) -> PreviewDecoder<FfmpegStream> {
        let make = |t: &PreviewTrack, w: u32| {
            let spec = StreamSpec {
                src_size: t.size,
                max_width: w,
                fps: t.fps,
                format,
            };
            Placed {
                start: t.start,
                dec: TrackDecoder::new(
                    FfmpegStream::new(ffmpeg.to_path_buf(), t.file.clone(), spec),
                    t.fps,
                ),
            }
        };
        PreviewDecoder {
            gaps,
            screen: make(screen, max_width),
            camera: camera.map(|c| make(c, (max_width / 2).max(2))),
        }
    }
}

impl<S: FrameStream> PreviewDecoder<S> {
    pub fn from_parts(
        gaps: Vec<PauseGap>,
        screen: (MediaTime, TrackDecoder<S>),
        camera: Option<(MediaTime, TrackDecoder<S>)>,
    ) -> PreviewDecoder<S> {
        PreviewDecoder {
            gaps,
            screen: Placed {
                start: screen.0,
                dec: screen.1,
            },
            camera: camera.map(|(start, dec)| Placed { start, dec }),
        }
    }

    /// Restarts of the screen stream so far.
    pub fn screen_seeks(&self) -> u32 {
        self.screen.dec.seeks
    }

    /// The pictures at `session`. A screen error is an error; a camera that has not started or
    /// has ended just gives no bubble.
    pub fn frames_at(&mut self, session: MediaTime) -> Result<PreviewFrames, DecodeError> {
        let pos =
            track_time(&self.gaps, self.screen.start, session).ok_or(DecodeError::NotStarted)?;
        let screen = self.screen.dec.frame_at(pos)?;
        let camera = self.camera.as_mut().and_then(|c| {
            let pos = track_time(&self.gaps, c.start, session)?;
            c.dec.frame_at(pos).ok()
        });
        Ok(PreviewFrames { screen, camera })
    }
}

// ---- audio ------------------------------------------------------------------------------

/// A decoded audio file: interleaved f32 at [`AUDIO_RATE`].
#[derive(Debug, Clone)]
pub struct PcmTrack {
    pub kind: AudioKind,
    pub start: MediaTime,
    pub channels: u16,
    pub samples: Vec<f32>,
}

/// Where one audio file is and how to read it.
#[derive(Debug, Clone)]
pub struct AudioFile {
    pub kind: AudioKind,
    pub file: PathBuf,
    pub start: MediaTime,
    pub channels: u16,
}

/// The decoded audio tracks of a recording, kept so each edit re-mixes without decoding again.
#[derive(Debug, Clone, Default)]
pub struct AudioBank {
    tracks: Vec<PcmTrack>,
}

impl AudioBank {
    /// Decodes every file (a file that cannot be read fails the whole load).
    pub fn load(ffmpeg: &Path, files: &[AudioFile]) -> Result<AudioBank, CmError> {
        let mut tracks = Vec::with_capacity(files.len());
        for f in files {
            tracks.push(PcmTrack {
                kind: f.kind,
                start: f.start,
                channels: f.channels,
                samples: decode_pcm(ffmpeg, &f.file, f.channels)?,
            });
        }
        Ok(AudioBank { tracks })
    }

    pub fn from_tracks(tracks: Vec<PcmTrack>) -> AudioBank {
        AudioBank { tracks }
    }

    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
    }

    /// The stereo mix of `list` on the output timeline (pauses removed, cuts skipped, speed with
    /// the pitch kept, muted above 2x unless `keep_audio`, the list's gains and mutes), the same
    /// call the exporter makes. No loudness target: the preview plays the source level.
    pub fn render(&self, list: &EditList, gaps: &[PauseGap]) -> Result<MixedAudio, CmError> {
        let timeline = list.without_pauses(gaps).timeline();
        let tracks: Vec<TrackAudio<'_>> = self
            .tracks
            .iter()
            .map(|t| {
                let (gain_db, mute) = match t.kind {
                    AudioKind::Mic => (list.audio.mic_gain_db, list.audio.mic_mute),
                    AudioKind::System => (list.audio.system_gain_db, list.audio.system_mute),
                };
                TrackAudio {
                    samples: &t.samples,
                    channels: t.channels,
                    sample_rate: AUDIO_RATE,
                    start: t.start,
                    gain_db,
                    mute,
                }
            })
            .collect();
        mix_tracks(
            &tracks,
            gaps,
            &timeline,
            &MixParams {
                sample_rate: AUDIO_RATE,
                out_channels: 2,
                loudness_lufs: None,
                ..MixParams::default()
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc as StdArc;

    fn t(ms: i64) -> MediaTime {
        MediaTime::from_millis(ms)
    }

    /// A scripted stream: `total` frames of 100 ms each, positions from the start position.
    struct Fake {
        total: i64,
        at: i64,
        origin: MediaTime,
        starts: StdArc<AtomicU32>,
    }

    impl Fake {
        fn new(total: i64) -> (Fake, StdArc<AtomicU32>) {
            let starts = StdArc::new(AtomicU32::new(0));
            (
                Fake {
                    total,
                    at: i64::MAX,
                    origin: MediaTime::ZERO,
                    starts: starts.clone(),
                },
                starts,
            )
        }
    }

    impl FrameStream for Fake {
        fn start(&mut self, pos: MediaTime) -> Result<(), String> {
            self.starts.fetch_add(1, Ordering::Relaxed);
            self.origin = pos;
            self.at = 0;
            Ok(())
        }

        fn next_frame(&mut self) -> Option<VideoFrame> {
            if self.at == i64::MAX {
                return None;
            }
            let pos = self.origin + t(self.at * 100);
            // The file ends at `total` frames from zero.
            if pos >= t(self.total * 100) {
                return None;
            }
            self.at += 1;
            Some(VideoFrame {
                pos,
                width: 2,
                height: 2,
                format: PixelFormat::Rgba,
                data: vec![(pos.ticks() / 12_000) as u8; 16],
            })
        }
    }

    fn decoder(total: i64) -> (TrackDecoder<Fake>, StdArc<AtomicU32>) {
        let (f, starts) = Fake::new(total);
        (TrackDecoder::new(f, 10.0), starts)
    }

    /// Seek math: a cached frame wins, a short jump ahead reads forward, anything else seeks.
    #[test]
    fn plan_chooses_cache_forward_or_seek() {
        let dur = t(100);
        let cache = [t(0), t(100), t(200)];
        assert_eq!(
            plan(&cache, dur, Some(t(300)), t(150), SKIP_WINDOW),
            Plan::Cached(1)
        );
        assert_eq!(
            plan(&cache, dur, Some(t(300)), t(300), SKIP_WINDOW),
            Plan::Forward
        );
        assert_eq!(
            plan(&cache, dur, Some(t(300)), t(1000), SKIP_WINDOW),
            Plan::Forward
        );
        assert_eq!(
            plan(&cache, dur, Some(t(300)), t(1100), SKIP_WINDOW),
            Plan::Seek
        );
        // Behind everything cached, or no running stream: seek.
        assert_eq!(
            plan(&[t(500)], dur, Some(t(600)), t(100), SKIP_WINDOW),
            Plan::Seek
        );
        assert_eq!(plan(&[], dur, None, t(0), SKIP_WINDOW), Plan::Seek);
    }

    /// Playing forward frame by frame starts the stream once.
    #[test]
    fn playing_forward_does_not_restart_the_stream() {
        let (mut d, starts) = decoder(100);
        for i in 0..40 {
            let f = d.frame_at(t(i * 100)).unwrap();
            assert_eq!(f.pos, t(i * 100));
        }
        assert_eq!(starts.load(Ordering::Relaxed), 1);
        assert_eq!(d.seeks, 1);
    }

    /// Faster than real time (4x at 30 fps shows every fourth frame) reads forward too.
    #[test]
    fn skipping_frames_while_playing_fast_reads_forward() {
        let (mut d, starts) = decoder(100);
        for i in 0..20 {
            d.frame_at(t(i * 300)).unwrap();
        }
        assert_eq!(starts.load(Ordering::Relaxed), 1);
    }

    /// Asking for the same time again, and stepping back a frame or two, hits the cache.
    #[test]
    fn repeats_and_small_steps_back_hit_the_cache() {
        let (mut d, starts) = decoder(100);
        d.frame_at(t(0)).unwrap();
        d.frame_at(t(500)).unwrap();
        let a = d.frame_at(t(520)).unwrap();
        let b = d.frame_at(t(540)).unwrap();
        assert!(Arc::ptr_eq(&a, &b));
        assert_eq!(d.frame_at(t(400)).unwrap().pos, t(400));
        assert_eq!(starts.load(Ordering::Relaxed), 1);
    }

    /// A jump (scrub) restarts at the new time; the picture is the one at that time.
    #[test]
    fn a_jump_seeks() {
        let (mut d, starts) = decoder(100);
        d.frame_at(t(0)).unwrap();
        let f = d.frame_at(t(5_000)).unwrap();
        assert_eq!(f.pos, t(5_000));
        let back = d.frame_at(t(1_000)).unwrap();
        assert_eq!(back.pos, t(1_000));
        assert_eq!(starts.load(Ordering::Relaxed), 3);
    }

    /// Past the last frame there is no picture; within a frame of the end the last one stays.
    #[test]
    fn the_end_of_the_file_is_an_error_past_the_last_frame() {
        let (mut d, _) = decoder(10); // frames at 0..900 ms, file ends at 1,000 ms
        assert_eq!(d.frame_at(t(900)).unwrap().pos, t(900));
        assert_eq!(d.frame_at(t(1_050)).unwrap().pos, t(900));
        assert_eq!(d.frame_at(t(5_000)).err(), Some(DecodeError::EndOfStream));
    }

    fn preview(
        screen_start: i64,
        cam_start: Option<i64>,
        gaps: Vec<PauseGap>,
    ) -> PreviewDecoder<Fake> {
        PreviewDecoder::from_parts(
            gaps,
            (t(screen_start), decoder(100).0),
            cam_start.map(|c| (t(c), decoder(100).0)),
        )
    }

    fn gap(from: i64, to: i64) -> PauseGap {
        PauseGap {
            from: t(from),
            to: t(to),
        }
    }

    /// Session time maps through each track's own start: before the first sample there is no
    /// picture (screen) or no bubble (camera).
    #[test]
    fn session_time_maps_through_track_starts() {
        let mut p = preview(300, Some(1_000), Vec::new());
        assert_eq!(p.frames_at(t(100)).err(), Some(DecodeError::NotStarted));
        let f = p.frames_at(t(500)).unwrap();
        assert_eq!(f.screen.pos, t(200));
        assert!(f.camera.is_none(), "the camera has not started yet");
        let f = p.frames_at(t(1_500)).unwrap();
        assert_eq!(f.screen.pos, t(1_200));
        assert_eq!(f.camera.unwrap().pos, t(500));
    }

    /// Pauses are not in the files: a time after a gap is earlier in the file, and playing
    /// across the gap keeps reading the same stream.
    #[test]
    fn pause_gaps_are_removed_from_the_file_position() {
        let mut p = preview(0, None, vec![gap(1_000, 2_000)]);
        assert_eq!(p.frames_at(t(2_500)).unwrap().screen.pos, t(1_500));
        let mut p2 = preview(0, None, vec![gap(1_000, 2_000)]);
        for ms in (0..900).step_by(100).chain((2_000..3_000).step_by(100)) {
            p2.frames_at(t(ms)).unwrap();
        }
        assert_eq!(p2.screen_seeks(), 1);
    }

    #[test]
    fn out_size_keeps_the_aspect_and_is_even() {
        let s = StreamSpec {
            src_size: (3024, 1964),
            max_width: 1280,
            fps: 30.0,
            format: PixelFormat::Rgba,
        };
        let (w, h) = s.out_size();
        assert_eq!(w, 1280);
        assert_eq!(h % 2, 0);
        assert!((f64::from(h) - 1280.0 * 1964.0 / 3024.0).abs() <= 2.0);
        // Smaller than the cap: unchanged (made even).
        let small = StreamSpec {
            src_size: (641, 361),
            ..s
        };
        assert_eq!(small.out_size().0, 640);
        assert_eq!(PixelFormat::Nv12.frame_len(4, 2), 12);
        assert_eq!(PixelFormat::Rgba.frame_len(4, 2), 32);
    }
}
