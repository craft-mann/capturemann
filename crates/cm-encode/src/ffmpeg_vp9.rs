//! A software VP9 encoder for [`CpuFrame`]s: raw frames go to an FFmpeg child process
//! (`libvpx-vp9`, real-time mode) that writes a fragmented MP4 (`vp09`, 2 s fragments, so a crash
//! loses at most one fragment and `truncate_to_last_fragment` recovers the file).
//!
//! Why this exists: Linux has no hardware encoder in a headless VM, and software H.264 does not
//! ship (OpenH264 was removed on Oct 24, 2026). libvpx is BSD-3 and already part of the LGPL FFmpeg
//! the product bundles. See `specs/plans/linux-recording-plan.md` (section 3) for the decisions
//! this path still needs. The caller supplies a constant-frame-rate stream (the recorder's pipeline
//! retimes to CFR); gaps in `pts` are filled by repeating the previous frame, and frames that map
//! to an already written slot are dropped.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{sync_channel, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use cm_core::{CmError, MediaTime};

use crate::cpu::CpuFrame;
use crate::encoder::{no_frames_error, AppendOutcome, EncoderConfig, VideoEncoder};
use crate::fmp4;
use crate::writer::{FragmentRecord, WriterSummary};

/// Frames queued for the FFmpeg pipe before `append` answers `NotReady` (about 130 ms at 60 fps).
pub const QUEUE_FRAMES: usize = 8;
/// Most repeats one `append` may add for a gap (10 s at 60 fps); a longer gap is a source stall.
const MAX_REPEAT: i64 = 600;
const TICKS: i64 = 120_000;

/// A packed frame and how many times it is written.
struct Job(Arc<Vec<u8>>, u32);

struct Shared {
    error: Mutex<Option<CmError>>,
}

/// Finds FFmpeg: `CM_FFMPEG` (file or folder), next to the executable, `PATH`.
pub fn find_ffmpeg() -> Option<PathBuf> {
    if let Some(v) = std::env::var_os("CM_FFMPEG") {
        let p = PathBuf::from(v);
        let p = if p.is_dir() { p.join("ffmpeg") } else { p };
        return p.is_file().then_some(p);
    }
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
    {
        let p = dir.join("ffmpeg");
        if p.is_file() {
            return Some(p);
        }
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join("ffmpeg"))
        .find(|p| p.is_file())
}

/// Does this FFmpeg have the libvpx-vp9 encoder?
pub fn has_vp9(ffmpeg: &Path) -> bool {
    Command::new(ffmpeg)
        .args(["-hide_banner", "-encoders"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("libvpx-vp9"))
        .unwrap_or(false)
}

/// The FFmpeg arguments for a stream of `src_w`x`src_h` frames of `pix_fmt`. Pure, so tests can
/// check them without running anything.
pub fn ffmpeg_args(
    cfg: &EncoderConfig,
    src_w: u32,
    src_h: u32,
    pix_fmt: &str,
    threads: u32,
) -> Vec<String> {
    let p = &cfg.plan;
    let fps = p.fps.max(1);
    let s = |v: &[&str]| v.iter().map(|x| (*x).to_string()).collect::<Vec<_>>();
    let mut a = s(&[
        "-hide_banner",
        "-loglevel",
        "error",
        "-y",
        "-f",
        "rawvideo",
        "-pix_fmt",
        pix_fmt,
    ]);
    a.extend(["-s".into(), format!("{src_w}x{src_h}")]);
    a.extend(["-r".into(), fps.to_string(), "-i".into(), "pipe:0".into()]);
    let mut vf = Vec::new();
    // Even sizes are required for 4:2:0.
    let (ow, oh) = (p.width & !1, p.height & !1);
    if (ow, oh) != (src_w, src_h) {
        vf.push(format!("scale={ow}:{oh}:flags=bilinear"));
    }
    vf.push("format=yuv420p".into());
    a.extend(["-vf".into(), vf.join(",")]);
    a.extend(s(&[
        "-c:v",
        "libvpx-vp9",
        "-deadline",
        "realtime",
        "-cpu-used",
        "8",
        "-row-mt",
        "1",
        "-lag-in-frames",
        "0",
        "-tile-columns",
        "2",
    ]));
    a.extend(["-threads".into(), threads.to_string()]);
    a.extend([
        "-b:v".into(),
        p.bitrate_bps.max(100_000).to_string(),
        "-g".into(),
        (fps * 2).to_string(),
    ]);
    a.extend(s(&[
        "-f",
        "mp4",
        "-movflags",
        "frag_keyframe+empty_moov+default_base_moof",
        "-frag_duration",
        "2000000",
    ]));
    a.push(cfg.path.to_string_lossy().into_owned());
    a
}

struct Running {
    tx: SyncSender<Job>,
    child: Child,
    writer: JoinHandle<()>,
}

pub struct FfmpegVp9Encoder {
    cfg: EncoderConfig,
    ffmpeg: PathBuf,
    shared: Arc<Shared>,
    running: Option<Running>,
    /// Slot index (frame number at the plan's fps) the next written frame takes.
    next_slot: Option<i64>,
    last_packed: Option<Arc<Vec<u8>>>,
    written_slots: u64,
    finished: Option<WriterSummary>,
}

impl FfmpegVp9Encoder {
    /// `not_found` when FFmpeg or its libvpx-vp9 encoder is missing. The process starts with the
    /// first frame (its size and layout are not known before).
    pub fn create(cfg: &EncoderConfig) -> Result<Self, CmError> {
        let ffmpeg = find_ffmpeg().ok_or_else(|| {
            CmError::not_found("FFmpeg was not found (set CM_FFMPEG); needed for VP9 recording.")
        })?;
        Self::with_ffmpeg(cfg, ffmpeg)
    }

    pub fn with_ffmpeg(cfg: &EncoderConfig, ffmpeg: PathBuf) -> Result<Self, CmError> {
        if !has_vp9(&ffmpeg) {
            return Err(CmError::not_found(format!(
                "`{}` has no libvpx-vp9 encoder.",
                ffmpeg.display()
            )));
        }
        Ok(FfmpegVp9Encoder {
            cfg: cfg.clone(),
            ffmpeg,
            shared: Arc::new(Shared {
                error: Mutex::new(None),
            }),
            running: None,
            next_slot: None,
            last_packed: None,
            written_slots: 0,
            finished: None,
        })
    }

    fn start(&mut self, frame: &CpuFrame) -> Result<(), CmError> {
        let threads = std::thread::available_parallelism()
            .map(|n| n.get() as u32)
            .unwrap_or(2)
            .clamp(1, 8);
        let args = ffmpeg_args(
            &self.cfg,
            frame.width,
            frame.height,
            frame.format.ffmpeg_name(),
            threads,
        );
        let mut child = Command::new(&self.ffmpeg)
            .args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| CmError::internal(format!("cannot start FFmpeg: {e}")))?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| CmError::internal("FFmpeg has no stdin"))?;
        let (tx, rx) = sync_channel::<Job>(QUEUE_FRAMES);
        let shared = Arc::clone(&self.shared);
        let writer = std::thread::Builder::new()
            .name("cm-vp9-pipe".into())
            .spawn(move || {
                while let Ok(Job(buf, repeat)) = rx.recv() {
                    for _ in 0..repeat {
                        if let Err(e) = stdin.write_all(&buf) {
                            if let Ok(mut g) = shared.error.lock() {
                                g.get_or_insert(CmError::internal(format!(
                                    "the FFmpeg pipe closed: {e}"
                                )));
                            }
                            return;
                        }
                    }
                }
                // Dropping stdin ends the stream; FFmpeg finishes the file.
            })
            .map_err(|e| CmError::internal(format!("cannot start the pipe thread: {e}")))?;
        self.running = Some(Running { tx, child, writer });
        Ok(())
    }

    /// The codec in the file (the manifest's `Codec` enum has no VP9 yet).
    pub fn codec_name(&self) -> &'static str {
        "vp9"
    }

    /// Slots handed to the pipe so far (frames in the output, repeats included).
    pub fn slots_written(&self) -> u64 {
        self.written_slots
    }
}

impl VideoEncoder for FfmpegVp9Encoder {
    type Frame = CpuFrame;

    fn append(
        &mut self,
        frame: &CpuFrame,
        pts: MediaTime,
        _duration: MediaTime,
    ) -> Result<AppendOutcome, CmError> {
        self.check()?;
        if self.finished.is_some() {
            return Err(CmError::internal("the encoder is finished"));
        }
        frame.validate()?;
        let fps = i64::from(self.cfg.plan.fps.max(1));
        let slot = (pts.ticks() * fps + TICKS / 2) / TICKS;
        let next = *self.next_slot.get_or_insert(slot);
        if slot < next {
            // Two frames rounded to one slot: the later one is dropped.
            return Ok(AppendOutcome::Written);
        }
        if self.running.is_none() {
            self.start(frame)?;
        }
        let gap = (slot - next).min(MAX_REPEAT);
        let packed = Arc::new(frame.packed().into_owned());
        let Some(run) = self.running.as_ref() else {
            return Err(CmError::internal("the encoder is not running"));
        };
        // Fill a gap by repeating the last frame; the new frame itself takes `slot`. Only the
        // first job can be refused; after it is queued the second waits for room.
        let mut jobs = Vec::with_capacity(2);
        if gap > 0 {
            if let Some(prev) = &self.last_packed {
                jobs.push(Job(Arc::clone(prev), gap as u32));
            }
        }
        jobs.push(Job(Arc::clone(&packed), 1));
        let mut it = jobs.into_iter();
        if let Some(first) = it.next() {
            match run.tx.try_send(first) {
                Ok(()) => {}
                Err(TrySendError::Full(_)) => return Ok(AppendOutcome::NotReady),
                Err(TrySendError::Disconnected(_)) => {
                    self.check()?;
                    return Err(CmError::internal("the FFmpeg pipe is closed"));
                }
            }
        }
        for j in it {
            run.tx
                .send(j)
                .map_err(|_| CmError::internal("the FFmpeg pipe is closed"))?;
        }
        self.written_slots += gap as u64 + 1;
        self.next_slot = Some(slot + 1);
        self.last_packed = Some(packed);
        Ok(AppendOutcome::Written)
    }

    fn codec_name(&self) -> Option<&'static str> {
        Some("vp9")
    }

    fn check(&self) -> Result<(), CmError> {
        match self.shared.error.lock() {
            Ok(g) => g.clone().map_or(Ok(()), Err),
            Err(_) => Ok(()),
        }
    }

    fn fragments_written(&self) -> usize {
        fmp4::scan_file(&self.cfg.path).map_or(0, |s| s.fragments.len())
    }

    fn finish(&mut self) -> Result<WriterSummary, CmError> {
        if let Some(s) = &self.finished {
            return Ok(s.clone());
        }
        let Some(run) = self.running.take() else {
            return Err(no_frames_error());
        };
        let Running {
            tx,
            mut child,
            writer,
        } = run;
        drop(tx);
        let _ = writer.join();
        let status = child
            .wait()
            .map_err(|e| CmError::internal(format!("waiting for FFmpeg: {e}")))?;
        let mut stderr = String::new();
        if let Some(mut e) = child.stderr.take() {
            let _ = std::io::Read::read_to_string(&mut e, &mut stderr);
        }
        if !status.success() {
            let first = stderr.lines().next().unwrap_or("no message");
            return Err(CmError::internal(format!(
                "FFmpeg exited with {status}: {first}"
            )));
        }
        self.check()?;
        let scan = fmp4::scan_file(&self.cfg.path)?;
        let ts = scan.init.as_ref().map_or(0, |i| u64::from(i.timescale));
        let ticks = |v: u64| {
            if ts == 0 {
                MediaTime::from_ticks(0)
            } else {
                MediaTime::from_ticks(
                    i64::try_from(u128::from(v) * 120_000 / u128::from(ts)).unwrap_or(0),
                )
            }
        };
        let fragments = scan
            .fragments
            .iter()
            .enumerate()
            .map(|(i, f)| FragmentRecord {
                index: i as u32,
                start: ticks(f.base_time),
                duration: ticks(f.duration),
                samples: f.samples,
                bytes: f.len,
                arrival_ms: 0,
            })
            .collect();
        let s = WriterSummary {
            path: self.cfg.path.clone(),
            bytes: scan.total_len,
            init_bytes: scan.fragments.first().map_or(0, |f| f.offset),
            fragments,
            halt: None,
        };
        self.finished = Some(s.clone());
        Ok(s)
    }
}

impl Drop for FfmpegVp9Encoder {
    fn drop(&mut self) {
        if let Some(mut run) = self.running.take() {
            drop(run.tx);
            let _ = run.writer.join();
            let _ = run.child.wait();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cpu::PixelFormat;
    use crate::settings::{Codec, Quality, VideoPlan};

    fn cfg(dir: &Path, w: u32, h: u32, fps: u32) -> EncoderConfig {
        EncoderConfig {
            path: dir.join("screen.mp4"),
            plan: VideoPlan {
                width: w,
                height: h,
                fps,
                codec: Codec::H264,
                bitrate_bps: 2_000_000,
                quality: Quality::Balanced,
            },
        }
    }

    fn frame(w: u32, h: u32, shade: u8) -> CpuFrame {
        CpuFrame::new(
            w,
            h,
            w as usize * 4,
            PixelFormat::Bgrx,
            (0..w * h * 4)
                .map(|i| (i as u8).wrapping_add(shade))
                .collect(),
        )
        .unwrap()
    }

    fn at(slot: i64, fps: i64) -> MediaTime {
        MediaTime::from_ticks(slot * TICKS / fps)
    }

    #[test]
    fn args_scale_only_when_the_sizes_differ_and_force_even_output() {
        let d = Path::new("/x");
        let a = ffmpeg_args(&cfg(d, 1280, 720, 30), 1280, 720, "bgr0", 4).join(" ");
        assert!(!a.contains("scale="), "{a}");
        assert!(
            a.contains("-pix_fmt bgr0 -s 1280x720 -r 30 -i pipe:0"),
            "{a}"
        );
        assert!(a.contains("libvpx-vp9") && a.contains("frag_keyframe+empty_moov"));
        let b = ffmpeg_args(&cfg(d, 1281, 721, 30), 1920, 1080, "bgra", 2).join(" ");
        assert!(b.contains("scale=1280:720"), "{b}");
        assert!(
            b.ends_with(&d.join("screen.mp4").display().to_string()),
            "{b}"
        );
    }

    fn ffmpeg_with_vp9() -> Option<PathBuf> {
        let f = find_ffmpeg().filter(|f| has_vp9(f));
        if f.is_none() {
            eprintln!("SKIPPED: no FFmpeg with libvpx-vp9");
        }
        f
    }

    #[test]
    fn missing_ffmpeg_is_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let e = FfmpegVp9Encoder::with_ffmpeg(
            &cfg(dir.path(), 64, 64, 30),
            "/nonexistent/ffmpeg".into(),
        )
        .err()
        .unwrap();
        assert_eq!(e.code(), cm_core::ErrorCode::NotFound);
    }

    #[test]
    fn a_recording_with_gaps_becomes_a_playable_fragmented_vp9_mp4() {
        let Some(ff) = ffmpeg_with_vp9() else { return };
        let dir = tempfile::tempdir().unwrap();
        let c = cfg(dir.path(), 128, 96, 30);
        let mut e = FfmpegVp9Encoder::with_ffmpeg(&c, ff.clone()).unwrap();
        // 3 s: slots 0..30 present, a 10-slot hole (repeated), then slots 40..90.
        let mut slot = 0;
        while slot < 90 {
            if (30..40).contains(&slot) {
                slot = 40;
            }
            // Faster than real time: retry while the pipe is full.
            while e
                .append(&frame(128, 96, slot as u8), at(slot, 30), at(1, 30))
                .unwrap()
                == AppendOutcome::NotReady
            {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            slot += 1;
        }
        // A frame mapping to a slot already written is dropped.
        e.append(&frame(128, 96, 0), at(10, 30), at(1, 30)).unwrap();
        let s = e.finish().unwrap();
        assert_eq!(e.slots_written(), 90);
        assert!(s.bytes > 0 && !s.fragments.is_empty());
        let out = Command::new(&ff)
            .args(["-hide_banner", "-i"])
            .arg(&c.path)
            .args(["-map", "0:v", "-f", "null", "-"])
            .output()
            .unwrap();
        let log = String::from_utf8_lossy(&out.stderr).into_owned();
        assert!(log.contains("vp9"), "{log}");
        assert!(
            log.contains("frame=   90") || log.contains("frame=90"),
            "{log}"
        );
        assert_eq!(e.finish().unwrap().bytes, s.bytes);
    }

    #[test]
    fn finish_without_frames_is_the_no_frames_error() {
        let Some(ff) = ffmpeg_with_vp9() else { return };
        let dir = tempfile::tempdir().unwrap();
        let mut e = FfmpegVp9Encoder::with_ffmpeg(&cfg(dir.path(), 64, 64, 30), ff).unwrap();
        assert!(crate::is_no_frames(&e.finish().unwrap_err()));
    }

    #[cfg(unix)]
    #[test]
    fn a_full_queue_answers_not_ready() {
        use std::os::unix::fs::PermissionsExt;
        // A fake ffmpeg that reads nothing from stdin.
        let dir = tempfile::tempdir().unwrap();
        let fake = dir.path().join("ffmpeg");
        std::fs::write(
            &fake,
            "#!/bin/sh\nif [ \"$2\" = \"-encoders\" ]; then echo libvpx-vp9; exit 0; fi\nsleep 3\n",
        )
        .unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut e = FfmpegVp9Encoder::with_ffmpeg(&cfg(dir.path(), 640, 480, 30), fake).unwrap();
        let f = frame(640, 480, 1);
        let mut not_ready = 0;
        for slot in 0..200 {
            match e.append(&f, at(slot, 30), at(1, 30)) {
                Ok(AppendOutcome::NotReady) => not_ready += 1,
                Ok(_) => {}
                Err(_) => break,
            }
        }
        assert!(not_ready > 0);
    }
}
