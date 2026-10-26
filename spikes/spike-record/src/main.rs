//! S3 spike: native capture -> FFmpeg (H.264) + mic, with drop and sync stats.
//!
//! Capture backends: Windows.Graphics.Capture (Windows), ScreenCaptureKit (macOS).
//! Frames go into a "latest frame" slot. A writer thread ticks at a constant
//! frame rate and pipes the latest frame to an FFmpeg sidecar as raw BGRA, so
//! the video timeline is wall-clock exact (constant frame rate, like Loom).
//! Mic audio goes to a WAV via cpal; both start times are taken from the same
//! monotonic clock and the offset is applied when muxing.
//!
//! Usage:
//!   spike-record [--seconds 60] [--fps 30] [--display 0] [--height 1080]
//!                [--encoder libx264|hw|<ffmpeg encoder name>] [--no-mic] [--out DIR]
//! Stop early with Ctrl+C.

mod backend;
mod sink;

use std::{
    io::Write,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use anyhow::{bail, Context, Result};
use sink::FrameSink;

pub struct Opts {
    pub seconds: f64,
    pub fps: u32,
    pub display: usize,
    pub height: u32,
    pub encoder: String,
    pub mic: bool,
    pub out_dir: PathBuf,
}

fn parse_args() -> Result<Opts> {
    let mut o = Opts {
        seconds: 60.0,
        fps: 30,
        display: 0,
        height: 1080,
        encoder: "libx264".into(),
        mic: true,
        out_dir: PathBuf::from("spike-out/record"),
    };
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut val = || args.next().with_context(|| format!("{a} needs a value"));
        match a.as_str() {
            "--seconds" => o.seconds = val()?.parse()?,
            "--fps" => o.fps = val()?.parse()?,
            "--display" => o.display = val()?.parse()?,
            "--height" => o.height = val()?.parse()?,
            "--encoder" => o.encoder = val()?,
            "--no-mic" => o.mic = false,
            "--out" => o.out_dir = PathBuf::from(val()?),
            "-h" | "--help" => {
                println!("see the header of spike-record/src/main.rs or docs/project/spikes.md");
                std::process::exit(0);
            }
            other => bail!("unknown argument {other}"),
        }
    }
    if o.encoder == "hw" {
        o.encoder = if cfg!(target_os = "macos") {
            "h264_videotoolbox".into()
        } else {
            "h264_mf".into() // or h264_nvenc / h264_qsv / h264_amf if your GPU has it
        };
    }
    Ok(o)
}

fn main() -> Result<()> {
    let opts = parse_args()?;
    std::fs::create_dir_all(&opts.out_dir)?;
    check_ffmpeg()?;
    check_encoder(&opts.encoder)?;

    let stop = Arc::new(AtomicBool::new(false));
    {
        let stop = stop.clone();
        ctrlc::set_handler(move || stop.store(true, Ordering::SeqCst)).context("ctrl-c handler")?;
    }

    println!(
        "CaptureMann S3 spike-record: {}s @ {} fps, display {}, height {}, encoder {}, mic {}",
        opts.seconds, opts.fps, opts.display, opts.height, opts.encoder, opts.mic
    );
    println!("Tip: play a fast-moving video full screen while recording so every frame changes.\n");

    let sink = Arc::new(FrameSink::default());

    // Mic first, so it is already warm when video starts.
    let wav_path = opts.out_dir.join("mic.wav");
    let mic = if opts.mic {
        Some(mic::start(&wav_path).context("starting mic capture (microphone permission?)")?)
    } else {
        None
    };

    let capture =
        backend::start(&opts, sink.clone(), stop.clone()).context("starting screen capture")?;

    // Wait for the first frame so we know the size.
    let wait_start = Instant::now();
    let (w, h) = loop {
        if let Some(size) = sink.size() {
            break size;
        }
        if wait_start.elapsed() > Duration::from_secs(5) {
            bail!("no frame within 5 s: check screen-recording permission");
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    println!(
        "first frame after {:.0} ms, {w}x{h} px",
        wait_start.elapsed().as_secs_f64() * 1e3
    );

    let video_path = opts.out_dir.join("video.mp4");
    let mut ffmpeg = spawn_ffmpeg(&opts, w, h, &video_path)?;
    let stats = write_loop(&opts, &sink, &stop, &mut ffmpeg, w, h)?;

    backend::stop(capture);
    drop(ffmpeg.stdin.take());
    let status = ffmpeg.wait()?;
    if !status.success() {
        bail!("ffmpeg exited with {status}");
    }

    let mic_stats = mic.map(mic::stop).transpose()?;
    report(
        &opts,
        &sink,
        &stats,
        mic_stats.as_ref(),
        &video_path,
        &wav_path,
    )
}

// ---------------------------------------------------------------------------
// Constant-frame-rate writer

struct WriteStats {
    t0: Instant,
    ticks: u64,
    late_ticks: u64,
    max_lag_ms: f64,
    final_lag_ms: f64,
    unique_frames: u64,
    elapsed: f64,
}

fn write_loop(
    opts: &Opts,
    sink: &FrameSink,
    stop: &AtomicBool,
    ffmpeg: &mut Child,
    w: u32,
    h: u32,
) -> Result<WriteStats> {
    let stdin = ffmpeg.stdin.as_mut().context("ffmpeg stdin")?;
    let interval = Duration::from_secs_f64(1.0 / opts.fps as f64);
    let total_ticks = (opts.seconds * opts.fps as f64).round() as u64;
    let mut frame = vec![0u8; (w * h * 4) as usize];
    let mut last_seq = 0u64;
    let mut s = WriteStats {
        t0: Instant::now(),
        ticks: 0,
        late_ticks: 0,
        max_lag_ms: 0.0,
        final_lag_ms: 0.0,
        unique_frames: 0,
        elapsed: 0.0,
    };
    let mut last_print = Instant::now();

    while s.ticks < total_ticks && !stop.load(Ordering::SeqCst) {
        let due = s.t0 + interval * s.ticks as u32;
        let now = Instant::now();
        if now < due {
            std::thread::sleep(due - now);
        }
        let lag = Instant::now().saturating_duration_since(due);
        let lag_ms = lag.as_secs_f64() * 1e3;
        if lag > interval {
            s.late_ticks += 1;
        }
        s.max_lag_ms = s.max_lag_ms.max(lag_ms);
        s.final_lag_ms = lag_ms;

        match sink.copy_latest(&mut frame, w, h) {
            Some(seq) if seq != last_seq => {
                last_seq = seq;
                s.unique_frames += 1;
            }
            _ => {} // same frame again (static screen) or size changed: repeat previous
        }
        stdin
            .write_all(&frame)
            .context("writing to ffmpeg (did it crash? see its error above)")?;
        s.ticks += 1;

        if last_print.elapsed() > Duration::from_secs(1) {
            last_print = Instant::now();
            print!(
                "\r  t={:>5.1}s  frames={:>5}  new={:>5}  late={:>4}  lag={:>6.1} ms   ",
                s.t0.elapsed().as_secs_f64(),
                s.ticks,
                s.unique_frames,
                s.late_ticks,
                lag_ms
            );
            let _ = std::io::stdout().flush();
        }
    }
    s.elapsed = s.t0.elapsed().as_secs_f64();
    println!();
    Ok(s)
}

// ---------------------------------------------------------------------------
// FFmpeg sidecar

fn check_ffmpeg() -> Result<()> {
    let out = Command::new("ffmpeg").arg("-version").output().context(
        "ffmpeg not found on PATH (macOS: brew install ffmpeg; Windows: winget install Gyan.FFmpeg)",
    )?;
    let first = String::from_utf8_lossy(&out.stdout)
        .lines()
        .next()
        .unwrap_or("")
        .to_string();
    println!("{first}");
    Ok(())
}

/// Fail fast, before capture starts, if FFmpeg doesn't have the encoder.
fn check_encoder(name: &str) -> Result<()> {
    let out = Command::new("ffmpeg")
        .args(["-hide_banner", "-encoders"])
        .output()
        .context("running ffmpeg -encoders")?;
    let list = String::from_utf8_lossy(&out.stdout);
    // Lines look like: " V....D h264_videotoolbox    VideoToolbox H.264 Encoder"
    let names: Vec<&str> = list
        .lines()
        .filter_map(|l| {
            let mut parts = l.split_whitespace();
            let flags = parts.next()?;
            (flags.len() == 6 && flags.starts_with('V'))
                .then(|| parts.next())
                .flatten()
        })
        .collect();
    if names.contains(&name) {
        println!("encoder: {name}");
        return Ok(());
    }
    let h264: Vec<&str> = names
        .iter()
        .copied()
        .filter(|n| n.contains("264") || n.contains("hevc") || n.contains("265"))
        .collect();
    bail!(
        "FFmpeg has no video encoder named '{name}'.\n\
         Use --encoder hw (VideoToolbox on macOS, Media Foundation on Windows) or one of: {}",
        if h264.is_empty() {
            "(none found)".to_string()
        } else {
            h264.join(", ")
        }
    )
}

fn spawn_ffmpeg(opts: &Opts, w: u32, h: u32, out: &Path) -> Result<Child> {
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-hide_banner", "-loglevel", "warning", "-y"])
        .args(["-f", "rawvideo", "-pix_fmt", "bgra"])
        .args(["-s", &format!("{w}x{h}")])
        .args(["-framerate", &opts.fps.to_string()])
        .args(["-i", "-"]);
    if h > opts.height {
        cmd.args(["-vf", &format!("scale=-2:{}:flags=bicubic", opts.height)]);
    }
    cmd.args(["-c:v", &opts.encoder]);
    match opts.encoder.as_str() {
        "libx264" => {
            cmd.args(["-preset", "veryfast", "-crf", "23"]);
        }
        "h264_videotoolbox" => {
            cmd.args(["-b:v", "8M", "-realtime", "1"]);
        }
        "h264_nvenc" => {
            cmd.args(["-preset", "p4", "-b:v", "8M"]);
        }
        _ => {
            cmd.args(["-b:v", "8M"]);
        }
    }
    cmd.args(["-pix_fmt", "yuv420p", "-movflags", "+faststart"])
        .arg(out);
    let shown: Vec<String> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    println!("ffmpeg {}", shown.join(" "));
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
    cmd.spawn().context("spawning ffmpeg")
}

fn mux(video: &Path, wav: &Path, audio_offset_s: f64, channels: u16, out: &Path) -> Result<()> {
    // Positive offset: audio started after video -> delay the audio input.
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-hide_banner", "-loglevel", "warning", "-y"]);
    if audio_offset_s >= 0.0 {
        cmd.arg("-i").arg(video);
        cmd.args(["-itsoffset", &format!("{audio_offset_s:.4}")])
            .arg("-i")
            .arg(wav);
        cmd.args(["-map", "0:v", "-map", "1:a"]);
    } else {
        cmd.args(["-itsoffset", &format!("{:.4}", -audio_offset_s)])
            .arg("-i")
            .arg(video);
        cmd.arg("-i").arg(wav);
        cmd.args(["-map", "0:v", "-map", "1:a"]);
    }
    // The WAV from hound is WAVE_FORMAT_EXTENSIBLE; FFmpeg reads a mono file as
    // layout "FL", which the AAC encoder rejects. Map it to plain mono/stereo.
    if channels == 1 {
        cmd.args(["-af", "pan=mono|c0=c0"]);
    } else {
        cmd.args(["-ac", "2"]);
    }
    cmd.args(["-c:v", "copy", "-c:a", "aac", "-b:a", "160k", "-shortest"])
        .arg(out);
    let status = cmd.status().context("running ffmpeg mux")?;
    anyhow::ensure!(status.success(), "ffmpeg mux failed: {status}");
    Ok(())
}

// ---------------------------------------------------------------------------
// Report

fn report(
    opts: &Opts,
    sink: &FrameSink,
    s: &WriteStats,
    mic: Option<&mic::MicStats>,
    video: &Path,
    wav: &Path,
) -> Result<()> {
    let expected = s.ticks as f64;
    let late_pct = 100.0 * s.late_ticks as f64 / expected.max(1.0);
    let received = sink.received();
    let capture_fps = received as f64 / s.elapsed;
    let video_dur = s.ticks as f64 / opts.fps as f64;

    println!("\n=== S3 results ===");
    println!(
        "wall time          {:.2} s, video duration {:.2} s ({} frames)",
        s.elapsed, video_dur, s.ticks
    );
    println!("capture delivered  {received} frames ({capture_fps:.1} fps; a static screen delivers fewer, that is fine)");
    println!(
        "unique frames      {} of {} written",
        s.unique_frames, s.ticks
    );
    println!(
        "late frames        {} ({late_pct:.2}%)  max lag {:.1} ms, final lag {:.1} ms",
        s.late_ticks, s.max_lag_ms, s.final_lag_ms
    );
    let drift_ms = (video_dur - s.elapsed) * 1e3;
    println!("video vs wall      {drift_ms:+.1} ms");

    let mut pass = late_pct <= 1.0 && s.final_lag_ms < 2.0 * 1000.0 / opts.fps as f64;
    if let Some(m) = mic {
        // When was the first mic sample captured, relative to video frame 0?
        let first_buf_s = m.first_buffer_frames as f64 / m.sample_rate as f64;
        let offset = signed_secs(m.first_callback, s.t0) - first_buf_s;
        let audio_dur = m.samples_per_channel as f64 / m.sample_rate as f64;
        let audio_wall = m
            .last_callback
            .duration_since(m.first_callback)
            .as_secs_f64()
            + first_buf_s;
        let audio_drift_ms = (audio_dur - audio_wall) * 1e3;
        println!(
            "mic                {} Hz x {} ch, {:.2} s of audio, start offset vs video {:+.1} ms, drift {:+.1} ms",
            m.sample_rate,
            m.channels,
            audio_dur,
            offset * 1e3,
            audio_drift_ms
        );
        if audio_drift_ms.abs() > 50.0 {
            println!("                   WARNING: audio clock drift > 50 ms; needs resampling to the video clock");
            pass = false;
        }
        let final_path = opts.out_dir.join("final.mp4");
        mux(video, wav, offset, m.channels, &final_path)?;
        println!("muxed              {}", final_path.display());
    } else {
        println!("video              {}", video.display());
    }

    println!(
        "\nS3 automatic checks: {}",
        if pass {
            "PASS"
        } else {
            "FAIL (see late frames / lag / drift above)"
        }
    );
    println!("Manual checks: play final.mp4, step to the A/V test flash and confirm the beep is within ~50 ms;");
    println!("watch CPU in Task Manager / Activity Monitor during the run (target < 30%).");
    Ok(())
}

/// a - b in seconds, signed.
fn signed_secs(a: Instant, b: Instant) -> f64 {
    a.saturating_duration_since(b).as_secs_f64() - b.saturating_duration_since(a).as_secs_f64()
}

// ---------------------------------------------------------------------------
// Microphone via cpal -> 32-bit float WAV

mod mic {
    use std::{
        fs::File,
        io::BufWriter,
        path::Path,
        sync::{Arc, Mutex},
        time::Instant,
    };

    use anyhow::{bail, Context, Result};
    use cpal::{
        traits::{DeviceTrait, HostTrait, StreamTrait},
        FromSample, Sample, SampleFormat,
    };

    type Writer = hound::WavWriter<BufWriter<File>>;

    struct State {
        writer: Option<Writer>,
        first_callback: Option<Instant>,
        last_callback: Option<Instant>,
        first_buffer_len: u64,
        samples: u64,
    }

    pub struct Mic {
        stream: cpal::Stream,
        state: Arc<Mutex<State>>,
        sample_rate: u32,
        channels: u16,
    }

    pub struct MicStats {
        pub first_callback: Instant,
        pub last_callback: Instant,
        pub first_buffer_frames: u64,
        pub samples_per_channel: u64,
        pub sample_rate: u32,
        pub channels: u16,
    }

    pub fn start(path: &Path) -> Result<Mic> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .context("no default input device")?;
        let config = device
            .default_input_config()
            .context("default input config")?;
        println!("mic: {:?} {config:?}", device.id().ok());
        let sample_rate = config.sample_rate();
        let channels = config.channels();
        let spec = hound::WavSpec {
            channels,
            sample_rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let state = Arc::new(Mutex::new(State {
            writer: Some(hound::WavWriter::create(path, spec)?),
            first_callback: None,
            last_callback: None,
            first_buffer_len: 0,
            samples: 0,
        }));
        let err_fn = |e| eprintln!("mic stream error: {e}");
        let st = state.clone();
        let stream = match config.sample_format() {
            SampleFormat::F32 => device.build_input_stream(
                config.into(),
                move |d: &[f32], _: &_| write(d, &st),
                err_fn,
                None,
            )?,
            SampleFormat::I16 => device.build_input_stream(
                config.into(),
                move |d: &[i16], _: &_| write(d, &st),
                err_fn,
                None,
            )?,
            SampleFormat::I32 => device.build_input_stream(
                config.into(),
                move |d: &[i32], _: &_| write(d, &st),
                err_fn,
                None,
            )?,
            SampleFormat::U8 => device.build_input_stream(
                config.into(),
                move |d: &[u8], _: &_| write(d, &st),
                err_fn,
                None,
            )?,
            other => bail!("unsupported mic sample format {other:?}"),
        };
        stream.play()?;
        Ok(Mic {
            stream,
            state,
            sample_rate,
            channels,
        })
    }

    fn write<T: Sample>(data: &[T], state: &Mutex<State>)
    where
        f32: FromSample<T>,
    {
        let now = Instant::now();
        // Never block the audio thread for long; a contended lock means we are stopping.
        let Ok(mut s) = state.lock() else { return };
        // The first callback's timestamp is when the first buffer was *delivered*;
        // its length is kept so the report can backdate to the first captured sample.
        if s.first_callback.is_none() {
            s.first_callback = Some(now);
            s.first_buffer_len = data.len() as u64;
        }
        s.last_callback = Some(now);
        s.samples += data.len() as u64;
        if let Some(w) = s.writer.as_mut() {
            for &x in data {
                let _ = w.write_sample(f32::from_sample(x));
            }
        }
    }

    pub fn stop(m: Mic) -> Result<MicStats> {
        drop(m.stream);
        let mut s = m
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("mic state poisoned"))?;
        if let Some(w) = s.writer.take() {
            w.finalize()?;
        }
        let first = s
            .first_callback
            .context("mic produced no audio (permission?)")?;
        Ok(MicStats {
            first_callback: first,
            last_callback: s.last_callback.unwrap_or(first),
            first_buffer_frames: s.first_buffer_len / m.channels.max(1) as u64,
            samples_per_channel: s.samples / m.channels.max(1) as u64,
            sample_rate: m.sample_rate,
            channels: m.channels,
        })
    }
}
