//! The continuous preview decoder against real FFmpeg fixtures (prints SKIPPED without FFmpeg).
//! The timing probes are `#[ignore]` (`cargo test -p cm-media --test decode -- --ignored
//! --nocapture`): they print decode throughput and scrub latency for this machine.

#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use cm_core::{EditContext, EditList, MediaTime, PauseGap, Span};
use cm_media::decode::{AudioBank, AudioFile, PcmTrack, PixelFormat, PreviewDecoder, PreviewTrack};
use cm_media::edit_plan::AudioKind;
use cm_media::tools;

fn ffmpeg() -> Option<PathBuf> {
    match tools::locate() {
        Ok(t) => Some(t.ffmpeg),
        Err(_) => {
            eprintln!("SKIPPED: FFmpeg is not installed");
            None
        }
    }
}

/// A `secs` long test pattern with a running frame counter at `size`, 30 fps.
fn make_video(ffmpeg: &Path, dir: &Path, name: &str, size: (u32, u32), secs: u32) -> PathBuf {
    let file = dir.join(name);
    let ok = Command::new(ffmpeg)
        .args(["-v", "error", "-f", "lavfi", "-i"])
        .arg(format!("testsrc2=size={}x{}:rate=30", size.0, size.1))
        .args([
            "-t",
            &secs.to_string(),
            "-pix_fmt",
            "yuv420p",
            "-g",
            "30",
            "-y",
        ])
        .arg(&file)
        .status()
        .unwrap()
        .success();
    assert!(ok, "could not make the fixture");
    file
}

fn track(file: PathBuf, size: (u32, u32)) -> PreviewTrack {
    PreviewTrack {
        file,
        start: MediaTime::ZERO,
        size,
        fps: 30.0,
    }
}

/// Playing forward: every 1/30 s gives the next picture, the frame positions are on the grid and
/// the pictures differ (the test pattern moves).
#[test]
fn plays_forward_through_one_process() {
    let Some(ff) = ffmpeg() else { return };
    let dir = tempfile::tempdir().unwrap();
    let file = make_video(&ff, dir.path(), "a.mp4", (640, 360), 3);
    let mut d = PreviewDecoder::open(
        &ff,
        &track(file, (640, 360)),
        None,
        Vec::new(),
        320,
        PixelFormat::Rgba,
    );
    let mut last: Option<Vec<u8>> = None;
    for i in 0..60 {
        let t = MediaTime::from_ticks(i * 120_000 / 30);
        let f = d.frames_at(t).unwrap();
        assert_eq!((f.screen.width, f.screen.height), (320, 180));
        assert_eq!(f.screen.data.len(), 320 * 180 * 4);
        assert!((f.screen.pos - t).ticks().abs() <= 1, "frame {i}");
        if let Some(prev) = &last {
            assert_ne!(prev, &f.screen.data, "frame {i} repeats");
        }
        last = Some(f.screen.data.clone());
    }
    assert_eq!(d.screen_seeks(), 1, "one process for the whole run");
}

/// A scrub restarts the process at the new time and the picture is that time's; the end of the
/// file is an error, not a black frame.
#[test]
fn scrubbing_seeks_and_the_end_is_an_error() {
    let Some(ff) = ffmpeg() else { return };
    let dir = tempfile::tempdir().unwrap();
    let file = make_video(&ff, dir.path(), "b.mp4", (320, 240), 4);
    let mut d = PreviewDecoder::open(
        &ff,
        &track(file, (320, 240)),
        None,
        Vec::new(),
        320,
        PixelFormat::Nv12,
    );
    let at_3 = d.frames_at(MediaTime::from_secs(3)).unwrap();
    assert_eq!(at_3.screen.data.len(), 320 * 240 * 3 / 2);
    let at_1 = d.frames_at(MediaTime::from_secs(1)).unwrap();
    assert_ne!(at_3.screen.data, at_1.screen.data);
    assert_eq!(d.screen_seeks(), 2);
    assert!(d.frames_at(MediaTime::from_secs(30)).is_err());
}

/// A pause gap is skipped: the picture after it comes from the earlier file position.
#[test]
fn a_pause_gap_shifts_the_file_position() {
    let Some(ff) = ffmpeg() else { return };
    let dir = tempfile::tempdir().unwrap();
    let file = make_video(&ff, dir.path(), "c.mp4", (320, 240), 4);
    let gap = PauseGap {
        from: MediaTime::from_secs(1),
        to: MediaTime::from_secs(2),
    };
    let mut gapped = PreviewDecoder::open(
        &ff,
        &track(file.clone(), (320, 240)),
        None,
        vec![gap],
        320,
        PixelFormat::Rgba,
    );
    let mut plain = PreviewDecoder::open(
        &ff,
        &track(file, (320, 240)),
        None,
        Vec::new(),
        320,
        PixelFormat::Rgba,
    );
    let a = gapped.frames_at(MediaTime::from_secs(3)).unwrap();
    let b = plain.frames_at(MediaTime::from_secs(2)).unwrap();
    assert_eq!(a.screen.data, b.screen.data, "session 3 s is file 2 s");
}

/// Decoded audio is mixed on the output timeline with the list's cuts and gains: a cut shortens
/// the mix, a mute silences the track, +6 dB doubles the level.
#[test]
fn audio_bank_renders_cuts_and_gains() {
    let rate = 48_000usize;
    let secs = 4usize;
    let tone: Vec<f32> = (0..rate * secs)
        .map(|i| 0.1 * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / rate as f32).sin())
        .collect();
    let bank = AudioBank::from_tracks(vec![PcmTrack {
        kind: AudioKind::Mic,
        start: MediaTime::ZERO,
        channels: 1,
        samples: tone,
    }]);
    let mut list = full(4);
    let plain = bank.render(&list, &[]).unwrap();
    assert_eq!(plain.channels, 2);
    assert_eq!(plain.samples.len() / 2, rate * secs);
    let peak = |m: &cm_audio::mix::MixedAudio| m.samples.iter().fold(0f32, |a, v| a.max(v.abs()));
    assert!((peak(&plain) - 0.1).abs() < 0.01);

    list = list
        .cut(&[Span::new(MediaTime::from_secs(1), MediaTime::from_secs(2))])
        .unwrap();
    let cut = bank.render(&list, &[]).unwrap();
    assert_eq!(cut.samples.len() / 2, rate * 3);

    list.audio.mic_gain_db = 6.0;
    let loud = bank.render(&list, &[]).unwrap();
    assert!((peak(&loud) / peak(&cut) - 1.995).abs() < 0.05);
    list.audio.mic_mute = true;
    let muted = bank.render(&list, &[]).unwrap();
    assert_eq!(peak(&muted), 0.0);
    // A second file with the real decoder: a WAV on disk reads back at the same length.
    let Some(ff) = ffmpeg() else { return };
    let dir = tempfile::tempdir().unwrap();
    let wav = dir.path().join("t.wav");
    let ok = Command::new(&ff)
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=2",
        ])
        .args(["-ar", "48000", "-ac", "1", "-y"])
        .arg(&wav)
        .status()
        .unwrap()
        .success();
    assert!(ok);
    let bank = AudioBank::load(
        &ff,
        &[AudioFile {
            kind: AudioKind::System,
            file: wav,
            start: MediaTime::ZERO,
            channels: 1,
        }],
    )
    .unwrap();
    let mixed = bank.render(&full(2), &[]).unwrap();
    assert_eq!(mixed.samples.len() / 2, 96_000);
}

fn rate(label: &str, ff: &Path, size: (u32, u32), max_width: u32, format: PixelFormat) {
    let dir = tempfile::tempdir().unwrap();
    let file = make_video(ff, dir.path(), "p.mp4", size, 6);
    let mut d = PreviewDecoder::open(ff, &track(file, size), None, Vec::new(), max_width, format);
    // Warm: the first picture includes the process start.
    let t0 = Instant::now();
    d.frames_at(MediaTime::ZERO).unwrap();
    let first = t0.elapsed();
    let n = 150;
    let t1 = Instant::now();
    for i in 1..=n {
        d.frames_at(MediaTime::from_ticks(i * 120_000 / 30))
            .unwrap();
    }
    let fps = n as f64 / t1.elapsed().as_secs_f64();
    // Scrub latency: jump around, time each picture.
    let mut worst = 0f64;
    let mut total = 0f64;
    let jumps = [4_500i64, 500, 3_000, 1_000, 5_500, 2_000];
    for ms in jumps {
        let t = Instant::now();
        d.frames_at(MediaTime::from_millis(ms)).unwrap();
        let e = t.elapsed().as_secs_f64() * 1000.0;
        worst = worst.max(e);
        total += e;
    }
    println!(
        "{label}: first picture {:.0} ms, sequential {fps:.0} fps, scrub mean {:.0} ms worst {worst:.0} ms",
        first.as_secs_f64() * 1000.0,
        total / jumps.len() as f64
    );
}

/// Decode throughput on this machine (not a stored report): 1080p and 4K sources decoded to a
/// 1280-wide preview, RGBA and NV12.
#[test]
#[ignore = "timing probe: prints numbers"]
fn throughput_probe() {
    let Some(ff) = ffmpeg() else { return };
    rate(
        "1080p -> 1280 RGBA",
        &ff,
        (1920, 1080),
        1280,
        PixelFormat::Rgba,
    );
    rate(
        "1080p -> 1280 NV12",
        &ff,
        (1920, 1080),
        1280,
        PixelFormat::Nv12,
    );
    rate(
        "4K -> 1280 RGBA",
        &ff,
        (3840, 2160),
        1280,
        PixelFormat::Rgba,
    );
    rate(
        "4K -> 1280 NV12",
        &ff,
        (3840, 2160),
        1280,
        PixelFormat::Nv12,
    );
}

fn full(secs: i64) -> EditList {
    EditList::full(&EditContext {
        end: MediaTime::from_secs(secs),
        live_at: None,
        gaps: &[],
    })
}
