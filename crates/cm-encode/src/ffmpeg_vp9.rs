//! The Linux encoder for [`CpuFrame`]s: raw frames go to an FFmpeg child process that writes a
//! fragmented MP4 (2 s fragments, so a crash loses at most one fragment and
//! `truncate_to_last_fragment` recovers the file). The codec is hardware H.264 through VA-API
//! (`h264_vaapi`, the user's own GPU driver) where the computer has it and a probe encode works,
//! and software VP9 (`libvpx-vp9`, real-time mode) otherwise: [`VideoBackend`], decision O10 of
//! release 0.3.0.
//!
//! Why this exists: Linux has no hardware encoder in a headless VM, and software H.264 does not
//! ship (OpenH264 was removed on Oct 24, 2026). libvpx is BSD-3 and already part of the LGPL FFmpeg
//! the product bundles. See `specs/plans/linux-recording-plan.md` (section 3) for the decisions
//! this path still needs. The caller supplies a constant-frame-rate stream (the recorder's pipeline
//! retimes to CFR); gaps in `pts` are filled by repeating the previous frame, and frames that map
//! to an already written slot are dropped.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
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
    cm_platform::process::command(ffmpeg)
        .args(["-hide_banner", "-encoders"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("libvpx-vp9"))
        .unwrap_or(false)
}

/// How the video is encoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VideoBackend {
    /// Software VP9 (libvpx), any computer.
    Vp9,
    /// Hardware H.264 through VA-API on this render node (for example `/dev/dri/renderD128`).
    /// `cqp`: the driver offers constant quality only (Intel UHD 620 with the iHD driver); the
    /// quality setting then picks the quantiser instead of a bit rate.
    VaapiH264 { device: PathBuf, cqp: bool },
}

impl VideoBackend {
    /// The codec name written to the recording's details.
    pub fn codec_name(&self) -> &'static str {
        match self {
            VideoBackend::Vp9 => "vp9",
            VideoBackend::VaapiH264 { .. } => "h264",
        }
    }

    /// One plain sentence for the person: what encodes the video and why.
    pub fn describe(&self) -> String {
        match self {
            VideoBackend::Vp9 => "software VP9 (no usable hardware H.264 encoder was found; \
                expect more CPU use)"
                .to_owned(),
            VideoBackend::VaapiH264 { device, .. } => {
                format!("hardware H.264 through VA-API ({})", device.display())
            }
        }
    }
}

/// The render nodes of the GPUs, in order (`/dev/dri/renderD128`, ...).
pub fn render_nodes() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir("/dev/dri")
        .map(|d| {
            d.filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with("renderD"))
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

/// Does this FFmpeg list the `h264_vaapi` encoder?
pub fn has_h264_vaapi(ffmpeg: &Path) -> bool {
    cm_platform::process::command(ffmpeg)
        .args(["-hide_banner", "-encoders"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("h264_vaapi"))
        .unwrap_or(false)
}

/// One probe encode on `device` with or without constant quality.
fn probe_once(ffmpeg: &Path, device: &Path, cqp: bool) -> bool {
    use std::io::Write as _;
    let mut cmd = cm_platform::process::command(ffmpeg);
    cmd.args(["-hide_banner", "-loglevel", "error", "-vaapi_device"])
        .arg(device)
        .args([
            "-f", "rawvideo", "-pix_fmt", "nv12", "-s", "256x256", "-i", "pipe:0",
        ])
        .args([
            "-vf",
            "format=nv12,hwupload",
            "-c:v",
            "h264_vaapi",
            "-profile:v",
            "high",
        ])
        .args(["-bf", "0", "-g", "60", "-frames:v", "1"]);
    if cqp {
        cmd.args(["-rc_mode", "CQP", "-qp", "26"]);
    } else {
        cmd.args(["-b:v", "2000000"]);
    }
    let Ok(mut child) = cmd
        .args(["-f", "null", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return false;
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(&vec![128_u8; 256 * 256 * 3 / 2]);
    }
    child.wait().map(|s| s.success()).unwrap_or(false)
}

/// Encodes one 256x256 frame on `device` to find out whether the driver really accepts H.264
/// (listing the encoder proves nothing: a computer without the right driver lists it too).
/// `Some(false)`: works with a bit rate; `Some(true)`: only with constant quality; `None`: no.
/// Takes well under a second on a working GPU.
pub fn probe_vaapi(ffmpeg: &Path, device: &Path) -> Option<bool> {
    if probe_once(ffmpeg, device, false) {
        Some(false)
    } else if probe_once(ffmpeg, device, true) {
        Some(true)
    } else {
        None
    }
}

/// The encoder this computer will use: VA-API H.264 when an FFmpeg with `h264_vaapi` and a
/// render node that passes [`probe_vaapi`] exist, else software VP9. `CM_VIDEO_ENCODER=vp9`
/// forces software (a way out for a driver that probes well and then misbehaves). The answer is
/// cached per FFmpeg for the life of the process.
pub fn choose_backend(ffmpeg: &Path) -> VideoBackend {
    use std::sync::OnceLock;
    static CHOSEN: OnceLock<(PathBuf, VideoBackend)> = OnceLock::new();
    if let Some((f, b)) = CHOSEN.get() {
        if f == ffmpeg {
            return b.clone();
        }
    }
    let forced_software =
        std::env::var("CM_VIDEO_ENCODER").is_ok_and(|v| v.eq_ignore_ascii_case("vp9"));
    let backend = if forced_software || !has_h264_vaapi(ffmpeg) {
        VideoBackend::Vp9
    } else {
        render_nodes()
            .into_iter()
            .find_map(|d| probe_vaapi(ffmpeg, &d).map(|cqp| (d, cqp)))
            .map_or(VideoBackend::Vp9, |(device, cqp)| VideoBackend::VaapiH264 {
                device,
                cqp,
            })
    };
    let _ = CHOSEN.set((ffmpeg.to_path_buf(), backend.clone()));
    backend
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
    ffmpeg_args_for(&VideoBackend::Vp9, cfg, src_w, src_h, pix_fmt, threads)
}

/// [`ffmpeg_args`] for a chosen [`VideoBackend`].
pub fn ffmpeg_args_for(
    backend: &VideoBackend,
    cfg: &EncoderConfig,
    src_w: u32,
    src_h: u32,
    pix_fmt: &str,
    threads: u32,
) -> Vec<String> {
    let p = &cfg.plan;
    let fps = p.fps.max(1);
    let s = |v: &[&str]| v.iter().map(|x| (*x).to_string()).collect::<Vec<_>>();
    let mut a = s(&["-hide_banner", "-loglevel", "error", "-y"]);
    if let VideoBackend::VaapiH264 { device, .. } = backend {
        a.extend([
            "-vaapi_device".into(),
            device.to_string_lossy().into_owned(),
        ]);
    }
    a.extend(s(&["-f", "rawvideo", "-pix_fmt", pix_fmt]));
    a.extend(["-s".into(), format!("{src_w}x{src_h}")]);
    a.extend(["-r".into(), fps.to_string(), "-i".into(), "pipe:0".into()]);
    let mut vf = Vec::new();
    // Even sizes are required for 4:2:0. The picture is converted with the BT.709 matrix and
    // the stream is tagged with it, so players show the colours the screen had.
    let (ow, oh) = (p.width & !1, p.height & !1);
    let resize = if (ow, oh) != (src_w, src_h) {
        format!("{ow}:{oh}:flags=bilinear:")
    } else {
        String::new()
    };
    vf.push(format!("scale={resize}out_color_matrix=bt709:out_range=tv"));
    match backend {
        VideoBackend::Vp9 => {
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
        }
        VideoBackend::VaapiH264 { cqp, .. } => {
            // The frame is converted on the CPU (BT.709) and uploaded as NV12; the GPU encodes.
            vf.push("format=nv12".into());
            vf.push("hwupload".into());
            a.extend(["-vf".into(), vf.join(",")]);
            // No B-frames: the pipeline stamps every frame, and a decoder needs no reordering.
            a.extend(s(&["-c:v", "h264_vaapi", "-profile:v", "high", "-bf", "0"]));
            if *cqp {
                // The driver has no bit-rate control (Intel UHD 620): constant quality.
                let qp = match p.quality {
                    crate::settings::Quality::High => 22,
                    crate::settings::Quality::Balanced => 26,
                    crate::settings::Quality::Small => 30,
                };
                a.extend([
                    "-rc_mode".into(),
                    "CQP".into(),
                    "-qp".into(),
                    qp.to_string(),
                ]);
            }
        }
    }
    a.extend(s(&[
        "-colorspace",
        "bt709",
        "-color_primaries",
        "bt709",
        "-color_trc",
        "bt709",
        "-color_range",
        "tv",
    ]));
    if !matches!(backend, VideoBackend::VaapiH264 { cqp: true, .. }) {
        a.extend(["-b:v".into(), p.bitrate_bps.max(100_000).to_string()]);
    }
    a.extend(["-g".into(), (fps * 2).to_string()]);
    a.extend(s(&[
        "-f",
        "mp4",
        "-movflags",
        "frag_keyframe+empty_moov+default_base_moof",
        "-frag_duration",
        "2000000",
        // Every fragment reaches the file when it is complete, so a crash loses at most one
        // (without it the muxer's output buffer holds back a quiet screen's fragments).
        "-flush_packets",
        "1",
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
    backend: VideoBackend,
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
        let backend = choose_backend(&ffmpeg);
        Self::with_backend(cfg, ffmpeg, backend)
    }

    /// Software VP9 with this FFmpeg (tests and the fallback).
    pub fn with_ffmpeg(cfg: &EncoderConfig, ffmpeg: PathBuf) -> Result<Self, CmError> {
        Self::with_backend(cfg, ffmpeg, VideoBackend::Vp9)
    }

    /// The encoder `backend` names. `not_found` when the FFmpeg lacks the codec it needs.
    pub fn with_backend(
        cfg: &EncoderConfig,
        ffmpeg: PathBuf,
        backend: VideoBackend,
    ) -> Result<Self, CmError> {
        match &backend {
            VideoBackend::Vp9 if !has_vp9(&ffmpeg) => {
                return Err(CmError::not_found(format!(
                    "`{}` has no libvpx-vp9 encoder (and no hardware H.264 encoder was found).",
                    ffmpeg.display()
                )));
            }
            VideoBackend::VaapiH264 { .. } if !has_h264_vaapi(&ffmpeg) => {
                return Err(CmError::not_found(format!(
                    "`{}` has no h264_vaapi encoder.",
                    ffmpeg.display()
                )));
            }
            _ => {}
        }
        Ok(FfmpegVp9Encoder {
            cfg: cfg.clone(),
            ffmpeg,
            backend,
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
        let args = ffmpeg_args_for(
            &self.backend,
            &self.cfg,
            frame.width,
            frame.height,
            frame.format.ffmpeg_name(),
            threads,
        );
        let mut child = cm_platform::process::command(&self.ffmpeg)
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
        self.backend.codec_name()
    }

    /// What encodes the video, for the recording's details and the diagnostics.
    pub fn backend(&self) -> &VideoBackend {
        &self.backend
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
        Some(self.backend.codec_name())
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
    use std::process::Command;

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

    /// O10 / REC-2: hardware H.264 through VA-API keeps 2 s key frames, has no B-frames and is
    /// tagged BT.709; the picture is converted on the CPU and uploaded as NV12.
    #[test]
    fn vaapi_args_encode_h264_with_two_second_keyframes_and_tagged_colours() {
        let d = Path::new("/x");
        let backend = VideoBackend::VaapiH264 {
            device: PathBuf::from("/dev/dri/renderD128"),
            cqp: false,
        };
        let a = ffmpeg_args_for(&backend, &cfg(d, 1920, 1080, 30), 1920, 1080, "bgr0", 4).join(" ");
        assert!(
            a.starts_with(
                "-hide_banner -loglevel error -y -vaapi_device /dev/dri/renderD128 -f rawvideo"
            ),
            "{a}"
        );
        assert!(a.contains("format=nv12,hwupload"), "{a}");
        assert!(a.contains("-c:v h264_vaapi") && a.contains("-bf 0"), "{a}");
        assert!(a.contains("-g 60"), "{a}");
        assert!(a.contains("-colorspace bt709"), "{a}");
        assert!(a.contains("frag_keyframe+empty_moov"), "{a}");
        assert!(!a.contains("libvpx"), "{a}");
        // A driver with constant quality only gets a quantiser, no bit rate.
        let q = VideoBackend::VaapiH264 {
            device: PathBuf::from("/dev/dri/renderD128"),
            cqp: true,
        };
        let c = ffmpeg_args_for(&q, &cfg(d, 1920, 1080, 30), 1920, 1080, "bgr0", 4).join(" ");
        assert!(
            c.contains("-rc_mode CQP -qp 26") && !c.contains("-b:v"),
            "{c}"
        );
        // The software path is tagged the same way.
        let v = ffmpeg_args(&cfg(d, 1920, 1080, 30), 1920, 1080, "bgr0", 4).join(" ");
        assert!(
            v.contains("out_color_matrix=bt709") && v.contains("-colorspace bt709"),
            "{v}"
        );
    }

    #[test]
    fn the_backend_names_its_codec() {
        assert_eq!(VideoBackend::Vp9.codec_name(), "vp9");
        let v = VideoBackend::VaapiH264 {
            device: PathBuf::from("/dev/dri/renderD128"),
            cqp: true,
        };
        assert_eq!(v.codec_name(), "h264");
        assert!(v.describe().contains("VA-API"));
        assert!(VideoBackend::Vp9.describe().contains("software VP9"));
    }

    #[test]
    fn args_scale_only_when_the_sizes_differ_and_force_even_output() {
        let d = Path::new("/x");
        let a = ffmpeg_args(&cfg(d, 1280, 720, 30), 1280, 720, "bgr0", 4).join(" ");
        // Only the colour conversion, no resize.
        assert!(a.contains("scale=out_color_matrix=bt709"), "{a}");
        assert!(!a.contains("scale=1280"), "{a}");
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
        // Another test thread may fork while the script is still open for writing, which makes the
        // first exec fail with "text file busy"; the probe then reports a missing encoder. Retry.
        let mut e = (0..20)
            .find_map(|_| {
                FfmpegVp9Encoder::with_ffmpeg(&cfg(dir.path(), 640, 480, 30), fake.clone())
                    .ok()
                    .or_else(|| {
                        std::thread::sleep(std::time::Duration::from_millis(50));
                        None
                    })
            })
            .expect("the fake ffmpeg starts");
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
