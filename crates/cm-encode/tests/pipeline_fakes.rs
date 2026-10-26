//! Everything in `cm-encode` that needs no VideoToolbox: plan resolution, the shared error
//! codes, the synthetic encoder (the scripted fake behind `VideoEncoder`), the fragmented MP4
//! writer, and the scanner/truncation used by crash recovery. Tests cite REC-2 (size and codec),
//! REC-3 (fps), REC-8 and NFR-6 (2 s fragments, a crash loses at most one) and R3-5 (empty and
//! halted takes). Nothing here touches the display, camera, microphone or an encoder.
#![allow(clippy::unwrap_used)]

use std::path::{Path, PathBuf};

use cm_core::{CmError, ErrorCode, MediaTime};
use cm_encode::fmp4::synth::{self, Spec};
use cm_encode::synthetic::{SyntheticEncoder, SyntheticFrame};
use cm_encode::{
    hardware_encoder_unavailable, is_hardware_encoder_unavailable, is_no_frames, no_frames_error,
    output_dimensions, plan_video, resolve_codec, scan_bytes, scan_file, truncate_to_last_fragment,
    AppendOutcome, Codec, CodecChoice, EncoderConfig, FragmentedMp4Writer, OutputSize, Quality,
    VideoEncoder, VideoPlan, VideoSettings, WriterOptions, FRAGMENT_SECS, MAX_H264_WIDTH,
};

const FRAME: i64 = 4000; // one 30 fps frame in 120 kHz ticks
const FRAG: i64 = 240_000; // 2 s

fn t(ticks: i64) -> MediaTime {
    MediaTime::from_ticks(ticks)
}

fn cfg(dir: &Path) -> EncoderConfig {
    EncoderConfig {
        path: dir.join("screen.mp4"),
        plan: plan_video(1920, 1080, &VideoSettings::default()).unwrap(),
    }
}

/// Appends `n` 30 fps frames starting at `start` ticks.
fn feed(enc: &mut SyntheticEncoder, start: i64, n: i64) {
    for i in 0..n {
        let out = enc
            .append(&SyntheticFrame(i as u64), t(start + i * FRAME), t(FRAME))
            .unwrap();
        assert_eq!(out, AppendOutcome::Written);
    }
}

// ----- settings ------------------------------------------------------------------------------

/// REC-2.AC1..AC3 as a table: (source w, h, size) -> output (w, h). Always even, never upscaled.
#[test]
fn rec2_output_dimensions_table() {
    use OutputSize::*;
    type Row = ((u32, u32, OutputSize), (u32, u32));
    let table: &[Row] = &[
        ((3024, 1964, Source), (3024, 1964)),
        ((3024, 1964, P1080), (1662, 1080)),
        ((3840, 2160, P720), (1280, 720)),
        ((1920, 1080, P1080), (1920, 1080)), // same height: unchanged
        ((1920, 1080, P2160), (1920, 1080)), // never upscale
        ((1281, 721, Source), (1280, 720)),  // odd cut to even
        ((1281, 721, P2160), (1280, 720)),
        ((100, 4000, P720), (18, 720)), // portrait region
        ((10, 4000, P720), (2, 720)),   // width never reaches 0
        ((2, 4000, P720), (2, 720)),
        ((4000, 2, P720), (4000, 2)), // thin strip: target not below source
        ((2, 2, Source), (2, 2)),
    ];
    for &((w, h, size), want) in table {
        assert_eq!(
            output_dimensions(w, h, size).unwrap(),
            want,
            "{w}x{h} {size:?}"
        );
    }
}

/// Too-small sources are `invalid_input` (exit code 2), whatever the size preset.
#[test]
fn rec2_degenerate_sources_are_invalid_input() {
    for (w, h) in [(0, 0), (1, 1), (0, 1080), (1920, 0), (1, 1080), (1920, 1)] {
        for size in [OutputSize::Source, OutputSize::P720] {
            let e = output_dimensions(w, h, size).unwrap_err();
            assert_eq!(e.code(), ErrorCode::InvalidInput, "{w}x{h}");
            assert_eq!(e.code().exit_code(), 2);
            assert!(plan_video(w, h, &VideoSettings::default()).is_err());
        }
    }
}

/// Huge sources do not overflow or panic (u32::MAX squared fits u64) and keep a sane plan.
#[test]
fn rec2_extreme_sources_do_not_panic() {
    let e = plan_video(u32::MAX, u32::MAX, &VideoSettings::default());
    // H.264 is refused above 4096 but Auto picks HEVC, so this resolves.
    let p = e.unwrap();
    assert_eq!(p.codec, Codec::Hevc);
    assert_eq!(p.width % 2, 0);
    assert!(p.bitrate_bps > 0);
    let p = plan_video(
        u32::MAX,
        u32::MAX,
        &VideoSettings {
            size: OutputSize::P720,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(p.height, 720);
}

/// REC-2.AC2 as a table: the H.264 width limit is on the OUTPUT width, so a forced H.264 on a
/// 5K display is fine once the preset brings it under 4096 and refused (invalid_input) before.
#[test]
fn rec2_ac2_forced_h264_checks_the_output_width() {
    let h264 = |size| VideoSettings {
        size,
        codec: CodecChoice::H264,
        ..Default::default()
    };
    assert_eq!(
        plan_video(5120, 2880, &h264(OutputSize::P1080))
            .unwrap()
            .codec,
        Codec::H264
    );
    let e = plan_video(5120, 2880, &h264(OutputSize::Source)).unwrap_err();
    assert_eq!(e.code(), ErrorCode::InvalidInput);
    assert!(e.to_string().contains("4096"));
    for (w, ok) in [
        (MAX_H264_WIDTH, true),
        (MAX_H264_WIDTH + 1, false),
        (0, true),
    ] {
        assert_eq!(resolve_codec(CodecChoice::H264, w).is_ok(), ok, "{w}");
    }
    // HEVC has no width limit.
    assert_eq!(
        resolve_codec(CodecChoice::Hevc, u32::MAX).unwrap(),
        Codec::Hevc
    );
}

/// REC-3: only 30 and 60 fps, as `invalid_input` naming the field; the plan carries the fps.
#[test]
fn rec3_fps_table() {
    for (fps, ok) in [
        (30, true),
        (60, true),
        (0, false),
        (1, false),
        (29, false),
        (31, false),
        (59, false),
        (61, false),
        (120, false),
        (u32::MAX, false),
    ] {
        let r = plan_video(
            1920,
            1080,
            &VideoSettings {
                fps,
                ..Default::default()
            },
        );
        assert_eq!(r.is_ok(), ok, "{fps}");
        match r {
            Ok(p) => assert_eq!(p.fps, fps),
            Err(e) => {
                assert_eq!(e.code(), ErrorCode::InvalidInput);
                assert!(e.to_string().contains("fps"), "{e}");
            }
        }
    }
}

/// The bitrate floor, the ordering HEVC < H.264 and 30 < 60 fps, and quality ordering hold over a
/// grid of sizes (NFR-6 sizing: no plan is free or absurdly small).
#[test]
fn bitrate_grid_invariants() {
    for (w, h) in [
        (2, 2),
        (320, 180),
        (1280, 720),
        (1920, 1080),
        (2560, 1440),
        (3840, 2160),
        (5120, 2880),
        (7680, 4320),
    ] {
        let b = |c, f, q| cm_encode::bitrate_bps(c, f, q, w, h);
        for q in [Quality::High, Quality::Balanced, Quality::Small] {
            assert!(b(Codec::Hevc, 30, q) <= b(Codec::H264, 30, q), "{w}x{h}");
            assert!(b(Codec::Hevc, 30, q) <= b(Codec::Hevc, 60, q), "{w}x{h}");
            assert!(b(Codec::H264, 30, q) <= b(Codec::H264, 60, q), "{w}x{h}");
            assert!(b(Codec::Hevc, 30, q) >= 250_000, "floor {w}x{h}");
        }
        assert!(b(Codec::H264, 30, Quality::High) >= b(Codec::H264, 30, Quality::Balanced));
        assert!(b(Codec::H264, 30, Quality::Balanced) >= b(Codec::H264, 30, Quality::Small));
    }
    assert_eq!(cm_encode::disk_mb_per_minute(0), 0.0);
    assert_eq!(cm_encode::disk_mb_per_minute(8_000_000), 60.0);
}

#[test]
fn settings_wire_names_round_trip_and_unknown_values_are_rejected() {
    for (size, name) in [
        (OutputSize::Source, "source"),
        (OutputSize::P2160, "2160p"),
        (OutputSize::P1440, "1440p"),
        (OutputSize::P1080, "1080p"),
        (OutputSize::P720, "720p"),
    ] {
        let j = serde_json::to_string(&size).unwrap();
        assert_eq!(j, format!("\"{name}\""));
        assert_eq!(serde_json::from_str::<OutputSize>(&j).unwrap(), size);
    }
    for bad in ["\"4k\"", "\"480p\"", "1080", "\"\""] {
        assert!(serde_json::from_str::<OutputSize>(bad).is_err(), "{bad}");
    }
    assert!(serde_json::from_str::<CodecChoice>("\"av1\"").is_err());
    assert!(serde_json::from_str::<Quality>("\"ultra\"").is_err());
    assert_eq!(Codec::Hevc.as_str(), "hevc");
    assert_eq!(Codec::H264.as_str(), "h264");
    // Missing fields take the defaults (1080p-free: Source, 30 fps, Auto, High).
    let d: VideoSettings = serde_json::from_str("{}").unwrap();
    assert_eq!(d, VideoSettings::default());
    let p = plan_video(1920, 1080, &d).unwrap();
    let back: VideoPlan = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
    assert_eq!(back, p);
    assert_eq!(FRAGMENT_SECS, 2);
}

// ----- shared errors -------------------------------------------------------------------------

/// REC-2: the two encoder errors map to the shared codes and CLI exit codes of specs/README.md.
#[test]
fn encoder_errors_map_to_shared_codes() {
    let e = hardware_encoder_unavailable();
    assert_eq!(e.code(), ErrorCode::PlatformRejected);
    assert_eq!(e.code().as_str(), "platform_rejected");
    assert_eq!(e.code().exit_code(), 5);
    assert!(is_hardware_encoder_unavailable(&e));
    // Other errors, even platform_rejected with another reason or none, are not that error.
    for other in [
        CmError::platform_rejected("x", Some("other".into())),
        CmError::platform_rejected(cm_encode::HARDWARE_ENCODER_UNAVAILABLE_MESSAGE, None),
        CmError::internal("hardware_encoder_unavailable"),
        no_frames_error(),
    ] {
        assert!(!is_hardware_encoder_unavailable(&other), "{other:?}");
    }

    let n = no_frames_error();
    assert_eq!(n.code(), ErrorCode::InvalidInput);
    assert!(is_no_frames(&n));
    assert!(!is_no_frames(&CmError::invalid_input("No frames")));
    assert!(!is_no_frames(&CmError::internal(
        cm_encode::NO_FRAMES_MESSAGE
    )));
    assert!(!is_no_frames(&hardware_encoder_unavailable()));
}

// ----- the synthetic (scripted) encoder ------------------------------------------------------

/// NFR-6 / REC-8: fragments cut every 2 s of media time, on the first frame at or past 2 s.
#[test]
fn nfr6_fragments_are_cut_every_two_seconds() {
    let dir = tempfile::tempdir().unwrap();
    let mut enc = SyntheticEncoder::create(&cfg(dir.path())).unwrap();
    feed(&mut enc, 0, 150); // 5 s at 30 fps
    let sum = enc.finish().unwrap();
    let samples: Vec<u32> = sum.fragments.iter().map(|f| f.samples).collect();
    assert_eq!(samples, vec![60, 60, 30]);
    let starts: Vec<i64> = sum.fragments.iter().map(|f| f.start.ticks()).collect();
    assert_eq!(starts, vec![0, FRAG, 2 * FRAG]);
    // Fragment indexes count up from 0 and arrival times never go backwards.
    for (i, f) in sum.fragments.iter().enumerate() {
        assert_eq!(f.index as usize, i);
    }
    assert!(sum
        .fragments
        .windows(2)
        .all(|w| w[0].arrival_ms <= w[1].arrival_ms));
    // Sizes add up: init + fragments == file length == summary.bytes.
    let file_len = std::fs::metadata(&sum.path).unwrap().len();
    assert_eq!(sum.bytes, file_len);
    assert_eq!(
        sum.init_bytes + sum.fragments.iter().map(|f| f.bytes).sum::<u64>(),
        file_len
    );
    assert!(sum.halt.is_none());
    let scan = scan_file(&sum.path).unwrap();
    assert!(scan.is_clean());
    assert!(scan.init.as_ref().unwrap().moov_first);
    assert_eq!(scan.fragments.len(), 3);
    assert_eq!(scan.frame_count(), 150);
    assert!(scan.fragments.iter().all(|f| f.starts_with_sync));
}

/// The boundary is exact: 239,999 ticks after the first sample stays, 240,000 starts a new one.
#[test]
fn fragment_boundary_is_exactly_two_seconds_after_the_first_sample() {
    for (second_pts, fragments) in [(FRAG - 1, 1), (FRAG, 2), (FRAG + 1, 2)] {
        let dir = tempfile::tempdir().unwrap();
        let mut enc = SyntheticEncoder::create(&cfg(dir.path())).unwrap();
        for pts in [0, second_pts] {
            enc.append(&SyntheticFrame(0), t(pts), t(FRAME)).unwrap();
        }
        let sum = enc.finish().unwrap();
        assert_eq!(
            sum.fragments.len(),
            fragments,
            "second frame at {second_pts}"
        );
    }
}

/// Zero frames: the file is just the init segment (valid, nothing to recover) and the default
/// `finish` succeeds; with `fail_empty` it is the "no frames" error like the hardware encoder
/// (R3-5), which the owner turns into "delete the empty bundle".
#[test]
fn r3_5_zero_frames() {
    let dir = tempfile::tempdir().unwrap();
    let c = cfg(dir.path());
    let mut enc = SyntheticEncoder::create(&c).unwrap();
    let sum = enc.finish().unwrap();
    assert!(sum.fragments.is_empty());
    assert_eq!(sum.bytes, sum.init_bytes);
    let scan = scan_file(&c.path).unwrap();
    assert!(scan.init.is_some() && scan.fragments.is_empty());
    assert!(scan.is_clean());
    assert!(scan.start().is_none() && scan.end().is_none() && scan.duration().is_none());
    assert_eq!(scan.frame_count(), 0);
    assert!(scan.fragment_durations_secs().is_empty());
    let e = truncate_to_last_fragment(&c.path).unwrap_err();
    assert_eq!(e.code(), ErrorCode::InvalidInput);

    let dir2 = tempfile::tempdir().unwrap();
    let mut enc = SyntheticEncoder::create(&cfg(dir2.path())).unwrap();
    enc.fail_empty = true;
    let e = enc.finish().unwrap_err();
    assert!(is_no_frames(&e), "{e:?}");
    // Asking again stays an error (nothing was recorded).
    assert!(is_no_frames(&enc.finish().unwrap_err()));
}

/// One frame is one fragment of one sample; `finish` is idempotent and later appends neither
/// panic nor change the closed file.
#[test]
fn one_frame_and_finish_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let c = cfg(dir.path());
    let mut enc = SyntheticEncoder::create(&c).unwrap();
    assert_eq!(enc.fragments_written(), 0);
    feed(&mut enc, 0, 1);
    let a = enc.finish().unwrap();
    let b = enc.finish().unwrap();
    assert_eq!(a, b);
    assert_eq!(a.fragments.len(), 1);
    assert_eq!(a.fragments[0].samples, 1);
    assert_eq!(enc.fragments_written(), 1);
    let before = std::fs::read(&c.path).unwrap();
    let _ = enc.append(&SyntheticFrame(9), t(FRAME), t(FRAME));
    assert_eq!(enc.finish().unwrap(), a);
    assert_eq!(std::fs::read(&c.path).unwrap(), before);
    let scan = scan_bytes(&before).unwrap();
    assert_eq!(scan.duration().unwrap().ticks(), FRAME);
}

/// `append` needs strictly increasing pts: equal and earlier values are an `internal` error, the
/// frame is not logged, and the encoder keeps working with later frames.
#[test]
fn out_of_order_pts_is_rejected_and_not_recorded() {
    let dir = tempfile::tempdir().unwrap();
    let mut enc = SyntheticEncoder::create(&cfg(dir.path())).unwrap();
    let log = enc.log();
    enc.append(&SyntheticFrame(1), t(10_000), t(FRAME)).unwrap();
    for bad in [10_000, 9_999, 0, -1] {
        let e = enc
            .append(&SyntheticFrame(2), t(bad), t(FRAME))
            .unwrap_err();
        assert_eq!(e.code(), ErrorCode::Internal, "{bad}");
        assert!(e.to_string().contains("non-increasing"), "{e}");
    }
    enc.append(&SyntheticFrame(3), t(10_001), t(FRAME)).unwrap();
    let ids: Vec<u64> = log.lock().unwrap().iter().map(|a| a.frame.0).collect();
    assert_eq!(ids, vec![1, 3]);
    assert_eq!(enc.finish().unwrap().fragments[0].samples, 2);
}

/// A frame that reports `NotReady` is not written and does not move the clock: the same pts can
/// be offered again.
#[test]
fn not_ready_frames_are_skipped_without_advancing_time() {
    let dir = tempfile::tempdir().unwrap();
    let mut enc = SyntheticEncoder::create(&cfg(dir.path())).unwrap();
    enc.not_ready_every = 3;
    let mut written = 0;
    let mut pts = 0;
    for _ in 0..30 {
        match enc.append(&SyntheticFrame(0), t(pts), t(FRAME)).unwrap() {
            AppendOutcome::Written => {
                written += 1;
                pts += FRAME;
            }
            AppendOutcome::NotReady => {} // retry the same pts
        }
    }
    let sum = enc.finish().unwrap();
    assert_eq!(
        sum.fragments.iter().map(|f| f.samples).sum::<u32>(),
        written
    );
    assert_eq!(enc.log().lock().unwrap().len() as u32, written);
}

/// Huge timestamps (a very long or late-starting take) survive the whole chain: tick values far
/// beyond 32 bits are written to `tfdt` and read back exactly.
#[test]
fn huge_timestamps_round_trip() {
    let start: i64 = 1 << 50; // about 297,000 years of ticks; far beyond any recording
    let dir = tempfile::tempdir().unwrap();
    let c = cfg(dir.path());
    let mut enc = SyntheticEncoder::create(&c).unwrap();
    feed(&mut enc, start, 61); // crosses one 2 s boundary
    let sum = enc.finish().unwrap();
    assert_eq!(sum.fragments.len(), 2);
    assert_eq!(sum.fragments[0].start.ticks(), start);
    assert_eq!(sum.fragments[1].start.ticks(), start + FRAG);
    let scan = scan_file(&c.path).unwrap();
    assert_eq!(scan.start().unwrap().ticks(), start);
    assert_eq!(scan.end().unwrap().ticks(), start + 61 * FRAME);

    // The extreme: a first frame at 0 and a second at i64::MAX makes two fragments, no overflow.
    let dir = tempfile::tempdir().unwrap();
    let mut enc = SyntheticEncoder::create(&cfg(dir.path())).unwrap();
    enc.append(&SyntheticFrame(0), t(0), t(FRAME)).unwrap();
    enc.append(&SyntheticFrame(1), t(i64::MAX), t(FRAME))
        .unwrap();
    let sum = enc.finish().unwrap();
    assert_eq!(sum.fragments.len(), 2);
    assert_eq!(sum.fragments[1].start.ticks(), i64::MAX);
}

/// A gap in media time (what a pause leaves if the caller does not close it up) is kept as is:
/// the new fragment starts at the later time and the file's duration spans the gap, while the
/// frame count does not. (The recorder removes pause gaps before it calls the encoder, REC-5.)
#[test]
fn a_gap_in_pts_starts_a_new_fragment_and_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    let c = cfg(dir.path());
    let mut enc = SyntheticEncoder::create(&c).unwrap();
    feed(&mut enc, 0, 10);
    feed(&mut enc, 10 * FRAME + 10 * FRAG, 10);
    let sum = enc.finish().unwrap();
    assert_eq!(sum.fragments.len(), 2);
    let scan = scan_file(&c.path).unwrap();
    assert_eq!(scan.frame_count(), 20);
    assert_eq!(scan.start().unwrap().ticks(), 0);
    assert_eq!(scan.end().unwrap().ticks(), 20 * FRAME + 10 * FRAG);
}

/// The init segment records the plan: size and codec for both codecs.
#[test]
fn init_segment_carries_the_plan() {
    for (codec, fourcc) in [(CodecChoice::Hevc, "hvc1"), (CodecChoice::H264, "avc1")] {
        let dir = tempfile::tempdir().unwrap();
        let c = EncoderConfig {
            path: dir.path().join("s.mp4"),
            plan: plan_video(
                1662,
                1080,
                &VideoSettings {
                    codec,
                    ..Default::default()
                },
            )
            .unwrap(),
        };
        let mut enc = SyntheticEncoder::create(&c).unwrap();
        feed(&mut enc, 0, 2);
        enc.finish().unwrap();
        let init = scan_file(&c.path).unwrap().init.unwrap();
        assert_eq!(init.codec, fourcc);
        assert_eq!((init.width, init.height), (1662, 1080));
        assert_eq!(init.timescale, 120_000);
    }
}

/// The encoder cannot create its file in a missing folder: a clean error, not a panic.
#[test]
fn create_in_a_missing_folder_fails_cleanly() {
    let c = EncoderConfig {
        path: PathBuf::from("/nonexistent-cm-encode-dir/screen.mp4"),
        plan: plan_video(1920, 1080, &VideoSettings::default()).unwrap(),
    };
    assert!(SyntheticEncoder::create(&c).is_err());
}

// ----- writer ---------------------------------------------------------------------------------

fn seg(s: &Spec, seq: u32, base: u64, samples: u32) -> Vec<u8> {
    synth::fragment(s, seq, base, samples, FRAME as u32, 100)
}

/// NFR-6: a crash mid-fragment. The last segment arrives cut short: the file keeps its bytes but
/// the scan ignores the partial tail, the log does not count it, and recovery truncates exactly
/// that tail.
#[test]
fn nfr6_a_truncated_last_fragment_is_ignored_and_recovered() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("screen.mp4");
    let w = FragmentedMp4Writer::create(&path).unwrap();
    let s = Spec::default();
    w.push_init(synth::init_segment(&s));
    for i in 0..3u32 {
        w.push_segment(seg(&s, i + 1, u64::from(i) * FRAG as u64, 60));
    }
    let full4 = seg(&s, 4, 3 * FRAG as u64, 60);
    let cut = full4.len() / 2;
    w.push_segment(full4[..cut].to_vec());
    let sum = w.finish().unwrap();
    assert_eq!(sum.fragments.len(), 3, "the partial segment is not logged");
    let scan = scan_file(&path).unwrap();
    assert_eq!(scan.fragments.len(), 3);
    assert!(!scan.is_clean());
    assert_eq!(scan.total_len - scan.valid_len, cut as u64);
    let (fixed, removed) = truncate_to_last_fragment(&path).unwrap();
    assert_eq!(removed, cut as u64);
    assert!(fixed.is_clean());
    assert_eq!(fixed.frame_count(), 180);
    // Recovery is idempotent.
    assert_eq!(truncate_to_last_fragment(&path).unwrap().1, 0);
}

/// Damage in the middle: the scan stops there, so later fragments are not trusted (a recovered
/// file never has a hole).
#[test]
fn damage_in_the_middle_ends_the_valid_prefix() {
    let s = Spec::default();
    let mut v = synth::init_segment(&s);
    v.extend(seg(&s, 1, 0, 60));
    let good = v.len() as u64;
    let mut bad = seg(&s, 2, FRAG as u64, 60);
    // Corrupt the moof's size so it claims more than the file has.
    let styp = u32::from_be_bytes([bad[0], bad[1], bad[2], bad[3]]) as usize;
    bad[styp..styp + 4].copy_from_slice(&u32::MAX.to_be_bytes());
    v.extend(bad);
    v.extend(seg(&s, 3, 2 * FRAG as u64, 60));
    let scan = scan_bytes(&v).unwrap();
    assert_eq!(scan.fragments.len(), 1);
    assert_eq!(scan.valid_len, good);
}

/// Misuse that must be visible: a segment before the init gives a file with no usable init.
#[test]
fn a_segment_before_the_init_is_not_a_recoverable_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("screen.mp4");
    let w = FragmentedMp4Writer::create(&path).unwrap();
    let s = Spec::default();
    w.push_segment(seg(&s, 1, 0, 10));
    w.push_init(synth::init_segment(&s));
    w.finish().unwrap();
    assert!(scan_file(&path).unwrap().fragments.is_empty());
    assert!(truncate_to_last_fragment(&path).is_err());
}

/// Pushing after `finish` is dropped quietly; the closed file does not change.
#[test]
fn pushes_after_finish_are_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("screen.mp4");
    let w = FragmentedMp4Writer::create(&path).unwrap();
    let s = Spec::default();
    w.push_init(synth::init_segment(&s));
    w.push_segment(seg(&s, 1, 0, 60));
    let a = w.finish().unwrap();
    let before = std::fs::read(&path).unwrap();
    w.push_segment(seg(&s, 2, FRAG as u64, 60));
    assert_eq!(w.finish().unwrap(), a);
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(w.fragments_written(), 1);
    assert!(w.check().is_ok());
}

/// Clones feed one file: segments pushed from several threads all arrive, each thread's own
/// order is kept, and the file is a clean sequence of whole fragments (no interleaved bytes).
#[test]
fn clones_on_several_threads_write_whole_fragments() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("screen.mp4");
    let w = FragmentedMp4Writer::create(&path).unwrap();
    let s = Spec::default();
    w.push_init(synth::init_segment(&s));
    let mut hs = vec![];
    for th in 0..4u64 {
        let w = w.clone();
        let s = s.clone();
        hs.push(std::thread::spawn(move || {
            for i in 0..5u64 {
                let base = (th * 1000 + i) * 1000;
                w.push_segment(seg(&s, 1, base, 3));
            }
        }));
    }
    for h in hs {
        h.join().unwrap();
    }
    let sum = w.finish().unwrap();
    assert_eq!(sum.fragments.len(), 20);
    let scan = scan_file(&path).unwrap();
    assert!(scan.is_clean());
    assert_eq!(scan.fragments.len(), 20);
    for th in 0..4u64 {
        let mine: Vec<u64> = scan
            .fragments
            .iter()
            .map(|f| f.base_time)
            .filter(|b| b / 1_000_000 == th)
            .collect();
        assert_eq!(mine.len(), 5);
        assert!(mine.windows(2).all(|p| p[0] < p[1]), "thread {th} order");
    }
}

/// `create` truncates an existing file, and `queue_segments: 0` is treated as 1 (not a deadlock).
#[test]
fn create_truncates_and_a_zero_queue_still_works() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("screen.mp4");
    std::fs::write(&path, vec![7u8; 10_000]).unwrap();
    let w = FragmentedMp4Writer::create_with(
        &path,
        WriterOptions {
            queue_segments: 0,
            ..WriterOptions::default()
        },
    )
    .unwrap();
    let s = Spec::default();
    w.push_init(synth::init_segment(&s));
    for i in 0..4u32 {
        w.push_segment(seg(&s, i + 1, u64::from(i) * FRAG as u64, 60));
    }
    let sum = w.finish().unwrap();
    assert_eq!(sum.fragments.len(), 4);
    let scan = scan_file(&path).unwrap();
    assert!(scan.is_clean());
    assert_eq!(std::fs::metadata(&path).unwrap().len(), sum.bytes);
}

/// A path that is a directory cannot be a recording file: a clean error.
#[test]
fn creating_over_a_directory_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    assert!(FragmentedMp4Writer::create(dir.path()).is_err());
}

/// R3-5: after a halt, `finish` stays idempotent and keeps reporting the same halt.
#[test]
fn r3_5_halt_is_stable_across_repeated_finish() {
    let dir = tempfile::tempdir().unwrap();
    let s = Spec::default();
    let init = synth::init_segment(&s);
    let frag = seg(&s, 1, 0, 60);
    let w = FragmentedMp4Writer::create_with(
        &dir.path().join("screen.mp4"),
        WriterOptions {
            fail_after_bytes: Some((init.len() + frag.len() + 5) as u64),
            ..WriterOptions::default()
        },
    )
    .unwrap();
    w.push_init(init);
    for i in 0..3u32 {
        w.push_segment(seg(&s, i + 1, u64::from(i) * FRAG as u64, 60));
    }
    let a = w.finish().unwrap();
    let b = w.finish().unwrap();
    assert_eq!(a, b);
    assert_eq!(a.fragments.len(), 1);
    assert_eq!(a.halt.as_ref().unwrap().reason, "disk_full");
    let e = w.check().unwrap_err();
    assert_eq!(e.code(), ErrorCode::DiskFull);
    assert_eq!(e.code().exit_code(), 5);
}

/// A disk that fails on the very first write (the init) is `disk_full` with nothing to keep.
#[test]
fn a_failing_first_write_is_disk_full() {
    let dir = tempfile::tempdir().unwrap();
    let w = FragmentedMp4Writer::create_with(
        &dir.path().join("screen.mp4"),
        WriterOptions {
            fail_after_bytes: Some(0),
            ..WriterOptions::default()
        },
    )
    .unwrap();
    w.push_init(synth::init_segment(&Spec::default()));
    let e = w.finish().unwrap_err();
    assert_eq!(e.code(), ErrorCode::DiskFull);
}

// ----- fmp4 scanner edge cases -----------------------------------------------------------------

/// Other timescales convert to 120 kHz ticks; a zero timescale yields no times instead of a
/// division by zero.
#[test]
fn scan_converts_timescales() {
    for (ts, base, dur, want_end) in [
        (90_000u32, 90_000u64, 90_000u32, 240_000i64),
        (1000, 1000, 2000, 360_000),
        (600, 600, 600, 240_000),
    ] {
        let s = Spec {
            timescale: ts,
            ..Spec::default()
        };
        let mut v = synth::init_segment(&s);
        v.extend(synth::fragment(&s, 1, base, 1, dur, 10));
        let scan = scan_bytes(&v).unwrap();
        assert_eq!(scan.start().unwrap().ticks(), 120_000, "ts {ts}");
        assert_eq!(scan.end().unwrap().ticks(), want_end, "ts {ts}");
    }
    let s = Spec {
        timescale: 0,
        ..Spec::default()
    };
    let mut v = synth::init_segment(&s);
    v.extend(synth::fragment(&s, 1, 0, 1, 10, 10));
    let scan = scan_bytes(&v).unwrap();
    assert!(scan.start().is_none() && scan.duration().is_none());
    assert!(scan.fragment_durations_secs().is_empty());
}

/// `parse_segment` on incomplete input is `None` (never a panic): empty, header-only, a `moof`
/// without its `mdat`, and every prefix of a real segment.
#[test]
fn parse_segment_rejects_incomplete_input() {
    let s = Spec::default();
    let full = seg(&s, 1, 0, 5);
    assert!(cm_encode::fmp4::parse_segment(&full).is_some());
    assert!(cm_encode::fmp4::parse_segment(&[]).is_none());
    assert!(cm_encode::fmp4::parse_segment(&[0, 0, 0, 8]).is_none());
    for cut in 0..full.len() {
        assert!(
            cm_encode::fmp4::parse_segment(&full[..cut]).is_none(),
            "prefix of {cut} bytes parsed"
        );
    }
}

/// A 64-bit box size (`size == 1`) and a final box with size 0 ("to the end of file") are
/// understood by the scanner.
#[test]
fn scan_handles_large_size_and_to_end_boxes() {
    let s = Spec::default();
    let mut v = synth::init_segment(&s);
    let frag = seg(&s, 1, 0, 4);
    // Rewrite the mdat (the last box) with a 64-bit size header.
    let mdat_at = frag.len() - (4 * 100 + 8);
    let mut big = frag[..mdat_at].to_vec();
    big.extend_from_slice(&1u32.to_be_bytes());
    big.extend_from_slice(b"mdat");
    big.extend_from_slice(&((4 * 100 + 16) as u64).to_be_bytes());
    big.extend_from_slice(&vec![0u8; 400]);
    v.extend(&big);
    let scan = scan_bytes(&v).unwrap();
    assert_eq!(scan.fragments.len(), 1);
    assert!(scan.is_clean());

    // size 0 on the mdat: runs to the end of the file.
    let mut zero = frag[..mdat_at].to_vec();
    zero.extend_from_slice(&0u32.to_be_bytes());
    zero.extend_from_slice(b"mdat");
    zero.extend_from_slice(&vec![0u8; 400]);
    let mut v = synth::init_segment(&s);
    v.extend(&zero);
    let scan = scan_bytes(&v).unwrap();
    assert_eq!(scan.fragments.len(), 1);
    assert!(scan.is_clean());
}

/// A missing file is `not_found` for both the scan and the recovery helper.
#[test]
fn missing_files_are_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("nope.mp4");
    assert_eq!(scan_file(&p).unwrap_err().code(), ErrorCode::NotFound);
    assert_eq!(
        truncate_to_last_fragment(&p).unwrap_err().code(),
        ErrorCode::NotFound
    );
}

/// A second `moov` after the first (a malformed or concatenated file) ends the valid prefix
/// instead of replacing the init.
#[test]
fn a_second_moov_ends_the_valid_prefix() {
    let s = Spec::default();
    let mut v = synth::init_segment(&s);
    v.extend(seg(&s, 1, 0, 60));
    let good = v.len() as u64;
    v.extend(synth::init_segment(&s));
    v.extend(seg(&s, 2, FRAG as u64, 60));
    let scan = scan_bytes(&v).unwrap();
    assert_eq!(scan.fragments.len(), 1);
    assert_eq!(scan.valid_len, good);
}

/// Random-looking input never panics the scanner (cheap property check: a fixed xorshift stream,
/// including mutated copies of a valid file).
#[test]
fn scanner_never_panics_on_mutated_input() {
    let s = Spec::default();
    let mut base = synth::init_segment(&s);
    for i in 0..3u32 {
        base.extend(seg(&s, i + 1, u64::from(i) * FRAG as u64, 5));
    }
    let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = move || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    };
    for _ in 0..400 {
        let mut v = base.clone();
        for _ in 0..(1 + next() % 4) {
            let at = (next() as usize) % v.len();
            v[at] = next() as u8;
        }
        if next() % 3 == 0 {
            v.truncate((next() as usize) % v.len());
        }
        let scan = scan_bytes(&v).unwrap();
        assert!(scan.valid_len <= scan.total_len);
    }
    for _ in 0..200 {
        let n = (next() % 300) as usize;
        let v: Vec<u8> = (0..n).map(|_| next() as u8).collect();
        let _ = scan_bytes(&v).unwrap();
    }
}
