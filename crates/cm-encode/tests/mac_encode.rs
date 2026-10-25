//! Drives the real VideoToolbox encoder with synthetic frames (no display or permission needed),
//! then checks the fragmented MP4 with the in-repo box parser and, when installed, ffprobe/ffmpeg.
//! Covers REC-8 (crash-safe fragments) and the encoder half of REC-2/REC-3 on real hardware.
//! Run: `cargo test -p cm-encode --test mac_encode -- --ignored --nocapture`.
#![cfg(target_os = "macos")]
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use cm_core::MediaTime;
use cm_encode::{
    plan_video, scan_file, truncate_to_last_fragment, AppendOutcome, CodecChoice, EncoderConfig,
    HardwareEncoder, MacFrame, OutputSize, Quality, VideoEncoder, VideoSettings,
};

fn record(dir: &Path, w: u32, h: u32, fps: u32, secs: u32, codec: CodecChoice) -> PathBuf {
    let plan = plan_video(
        w,
        h,
        &VideoSettings {
            size: OutputSize::Source,
            fps,
            codec,
            quality: Quality::High,
        },
    )
    .unwrap();
    let path = dir.join("screen.mp4");
    let mut enc = HardwareEncoder::create(&EncoderConfig {
        path: path.clone(),
        plan,
    })
    .unwrap();
    let step = 120_000 / i64::from(fps);
    for i in 0..(fps * secs) {
        let frame = MacFrame::test_pattern(w as usize, h as usize, i).unwrap();
        loop {
            match enc
                .append(
                    &frame,
                    MediaTime::from_ticks(i64::from(i) * step),
                    MediaTime::from_ticks(step),
                )
                .unwrap()
            {
                AppendOutcome::Written => break,
                AppendOutcome::NotReady => std::thread::sleep(Duration::from_millis(2)),
            }
        }
    }
    let summary = enc.finish().unwrap();
    println!(
        "{} fragments, {} bytes, arrivals {:?}",
        summary.fragments.len(),
        summary.bytes,
        summary
            .fragments
            .iter()
            .map(|f| f.arrival_ms)
            .collect::<Vec<_>>()
    );
    path
}

fn ffprobe_frames(path: &Path) -> Option<(String, u64)> {
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-count_frames",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=codec_name,nb_read_frames",
            "-of",
            "csv=p=0",
        ])
        .arg(path)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let mut it = text.split(',');
    Some((it.next()?.to_string(), it.next()?.parse().ok()?))
}

fn check(codec: CodecChoice, fourcc: &str, ffname: &str, w: u32, h: u32) {
    let dir = tempfile::tempdir().unwrap();
    let (fps, secs) = (30u32, 7u32);
    let path = record(dir.path(), w, h, fps, secs, codec);
    let scan = scan_file(&path).unwrap();
    let init = scan.init.as_ref().unwrap();
    assert!(init.moov_first, "moov must come first");
    assert_eq!(init.codec, fourcc);
    assert_eq!((init.width, init.height), (w, h));
    assert_eq!(init.timescale, 120_000);
    assert!(scan.is_clean());
    assert_eq!(scan.frame_count(), u64::from(fps * secs));
    // 7 s of 2 s fragments is 4 fragments (2, 2, 2, 1), each starting on a sync sample, with no
    // gaps between them.
    let durs = scan.fragment_durations_secs();
    println!("fragment durations {durs:?}");
    assert_eq!(durs.len(), 4);
    assert!(scan.fragments.iter().all(|f| f.starts_with_sync));
    for pair in scan.fragments.windows(2) {
        assert_eq!(pair[0].base_time + pair[0].duration, pair[1].base_time);
    }
    for d in &durs[..3] {
        assert!((d - 2.0).abs() < 0.1, "fragment {d}");
    }
    assert!((scan.duration().unwrap().as_secs_f64() - f64::from(secs)).abs() < 0.05);

    // ffprobe reads it and counts every frame.
    if let Some((name, frames)) = ffprobe_frames(&path) {
        assert_eq!(name, ffname);
        assert_eq!(frames, u64::from(fps * secs));
    } else {
        eprintln!("ffprobe not available or failed: skipping the decode check");
    }

    // REC-8: cut the file in the middle of the last fragment; recovery truncates to the end of
    // fragment 3 and the file decodes up to there.
    let cut = scan.fragments[3].offset + scan.fragments[3].len / 2;
    let bytes = std::fs::read(&path).unwrap();
    let crashed = dir.path().join("crashed.mp4");
    std::fs::write(&crashed, &bytes[..cut as usize]).unwrap();
    assert!(!scan_file(&crashed).unwrap().is_clean());
    let (fixed, removed) = truncate_to_last_fragment(&crashed).unwrap();
    assert!(removed > 0);
    assert_eq!(fixed.fragments.len(), 3);
    assert_eq!(fixed.frame_count(), u64::from(fps * 6));
    if let Some((_, frames)) = ffprobe_frames(&crashed) {
        assert_eq!(frames, u64::from(fps * 6));
    }
}

/// REC-8.AC1 (encoder half), NFR-6: HEVC 1080p30, fragments of 2 s, truncation recovery.
#[test]
#[ignore = "drives the hardware encoder (VideoToolbox)"]
fn hevc_fragments_are_2s_and_survive_truncation() {
    check(CodecChoice::Hevc, "hvc1", "hevc", 1920, 1080);
}

/// Same for H.264.
#[test]
#[ignore = "drives the hardware encoder (VideoToolbox)"]
fn h264_fragments_are_2s_and_survive_truncation() {
    check(CodecChoice::H264, "avc1", "h264", 1280, 720);
}
