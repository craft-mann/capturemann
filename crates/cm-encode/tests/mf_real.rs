//! The Media Foundation hardware H.264 encoder on a real Windows machine: synthetic moving frames
//! (no screen, camera or microphone) go in, a fragmented MP4 comes out and is scanned.
//!
//! Hardware test: `#[ignore]`, run on purpose and one at a time, for example
//! `cargo test -p cm-encode --test mf_real -- --ignored --nocapture --test-threads=1`.
//! It prints `SKIPPED` when the machine has no hardware H.264 encoder.
#![cfg(windows)]
#![allow(clippy::unwrap_used)]

use cm_core::MediaTime;
use cm_encode::{
    bitrate_bps, fmp4, is_hardware_encoder_unavailable, Codec, CpuFrame, EncoderConfig,
    HardwareEncoder, PixelFormat, Quality, VideoEncoder, VideoPlan,
};

fn plan(w: u32, h: u32, fps: u32) -> VideoPlan {
    VideoPlan {
        width: w,
        height: h,
        fps,
        codec: Codec::H264,
        bitrate_bps: bitrate_bps(Codec::H264, fps, Quality::High, w, h),
        quality: Quality::High,
    }
}

/// A frame with a moving gradient and a bar, so the encoder has something to do.
fn frame(w: u32, h: u32, n: u32) -> CpuFrame {
    let mut data = Vec::with_capacity((w * h * 4) as usize);
    let bar = (n * 7) % w;
    for y in 0..h {
        for x in 0..w {
            let on_bar = x >= bar && x < bar + 24;
            data.extend_from_slice(&[
                (x.wrapping_add(n * 3) & 0xFF) as u8,
                (y & 0xFF) as u8,
                if on_bar { 255 } else { 40 },
                255,
            ]);
        }
    }
    CpuFrame::new(w, h, (w * 4) as usize, PixelFormat::Bgra, data).unwrap()
}

#[test]
#[ignore = "uses the hardware video encoder"]
fn five_seconds_of_1080p30_make_a_valid_fragmented_mp4() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("screen.mp4");
    let cfg = EncoderConfig {
        path: path.clone(),
        plan: plan(1920, 1080, 30),
    };
    let mut enc = match HardwareEncoder::create(&cfg) {
        Ok(e) => e,
        Err(e) if is_hardware_encoder_unavailable(&e) => {
            println!("SKIPPED: no hardware H.264 encoder ({e})");
            return;
        }
        Err(e) => panic!("{e}"),
    };
    let step = 4_000; // 30 fps in 120 kHz ticks
    let (mut written, mut not_ready) = (0, 0);
    let f = frame(1920, 1080, 0);
    for i in 0..150u32 {
        // A fresh frame every slot; the first is repeated to test the cached conversion too.
        let fr = if i % 10 == 0 {
            f.clone()
        } else {
            frame(1920, 1080, i)
        };
        let pts = MediaTime::from_ticks(i64::from(i) * step);
        loop {
            match enc.append(&fr, pts, MediaTime::from_ticks(step)).unwrap() {
                cm_encode::AppendOutcome::Written => {
                    written += 1;
                    break;
                }
                // Real time is 33 ms per frame; here the test is not paced, so wait for room.
                cm_encode::AppendOutcome::NotReady => {
                    not_ready += 1;
                    std::thread::sleep(std::time::Duration::from_millis(2));
                }
            }
        }
    }
    let summary = enc.finish().unwrap();
    println!(
        "written {written}, waited {not_ready} times, {} fragments, {} bytes",
        summary.fragments.len(),
        summary.bytes
    );
    let scan = fmp4::scan_file(&path).unwrap();
    assert!(scan.is_clean(), "the file ends on a fragment boundary");
    let init = scan.init.as_ref().unwrap();
    assert_eq!(
        (init.width, init.height, init.codec.as_str()),
        (1920, 1080, "avc1")
    );
    assert!(
        scan.fragments.len() >= 2,
        "{} fragments",
        scan.fragments.len()
    );
    assert!(scan.fragments.iter().all(|f| f.starts_with_sync));
    // 150 frames in, 150 frames out (no frame is dropped inside the encoder), 5 s.
    assert_eq!(scan.frame_count(), 150);
    assert_eq!(scan.duration().unwrap().ticks(), 150 * step);
    // Fragments are about 2 s long (the last one is shorter).
    for d in scan.fragment_durations_secs().iter().take(2) {
        assert!((1.5..=2.5).contains(d), "fragment of {d} s");
    }
}

#[test]
#[ignore = "uses the hardware video encoder"]
fn a_size_no_encoder_accepts_is_the_unavailable_error() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = EncoderConfig {
        path: dir.path().join("screen.mp4"),
        // Wider than H.264 allows (4096): rejected before any hardware is asked.
        plan: plan(8192, 4320, 30),
    };
    let e = HardwareEncoder::create(&cfg).err().unwrap();
    assert!(is_hardware_encoder_unavailable(&e), "{e}");
    assert!(!cfg.path.exists(), "a rejected plan leaves no file");
}
