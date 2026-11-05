//! The decisions of the Windows Media Foundation encoder that need no Windows API: which plan the
//! H.264 transform is asked for, how Media Foundation packs a pair of numbers into one attribute,
//! and when to ask for a key frame. Pure Rust, tested on every OS (the encoder itself is
//! `windows.rs`).

use cm_core::{CmError, TICKS_PER_SECOND};

use crate::encoder::hardware_encoder_unavailable;
use crate::settings::{bitrate_bps, Codec, VideoPlan, FRAGMENT_SECS, MAX_H264_WIDTH};

/// The plan the H.264 transform runs for a requested plan. The Windows recorder writes H.264
/// only: a plan that asked for HEVC (the automatic choice above 1920 px wide) is recorded as H.264
/// with the bit rate the table gives H.264 for that size, and the recorder reports it through
/// `VideoEncoder::used_plan`. A frame wider than H.264 allows is `hardware_encoder_unavailable`,
/// so the recorder tries the smaller sizes (REC-2, issue 16).
pub fn h264_plan_for(plan: &VideoPlan) -> Result<VideoPlan, CmError> {
    if plan.width == 0
        || plan.height == 0
        || !plan.width.is_multiple_of(2)
        || !plan.height.is_multiple_of(2)
    {
        return Err(CmError::invalid_input(format!(
            "video size {}x{} must be even",
            plan.width, plan.height
        )));
    }
    if plan.width > MAX_H264_WIDTH {
        return Err(hardware_encoder_unavailable());
    }
    if plan.codec == Codec::H264 {
        return Ok(*plan);
    }
    Ok(VideoPlan {
        codec: Codec::H264,
        bitrate_bps: bitrate_bps(Codec::H264, plan.fps, plan.quality, plan.width, plan.height),
        ..*plan
    })
}

/// The plan as the recorder on this computer will really run it: on Windows the H.264 plan
/// [`h264_plan_for`] gives (the automatic HEVC above 1920 px wide becomes H.264, so `record start`
/// and the manifest name the codec of the file); a plan H.264 cannot take (wider than 4096 px) is
/// kept as asked, and the recorder then tries the smaller sizes. Elsewhere the plan is unchanged.
pub fn platform_plan(plan: VideoPlan) -> VideoPlan {
    if cfg!(windows) {
        h264_plan_for(&plan).unwrap_or(plan)
    } else {
        plan
    }
}

/// Media Foundation stores a size (`MF_MT_FRAME_SIZE`) or a ratio (`MF_MT_FRAME_RATE`) as one
/// 64-bit attribute: the first number in the high half, the second in the low half.
pub fn pack_pair(high: u32, low: u32) -> u64 {
    (u64::from(high) << 32) | u64::from(low)
}

/// When to ask the encoder for a key frame: on the first frame, then whenever a fragment's worth
/// of time has passed since the last one, so a fragment (which must start at a sync sample) is
/// ready every [`FRAGMENT_SECS`] whatever GOP length the transform chooses on its own.
#[derive(Debug, Clone)]
pub struct KeyframeClock {
    interval: i64,
    last: Option<i64>,
}

impl Default for KeyframeClock {
    fn default() -> Self {
        // A little short of the fragment length, so timestamp jitter cannot push the request one
        // frame past the 2 s mark.
        KeyframeClock {
            interval: TICKS_PER_SECOND * i64::from(FRAGMENT_SECS) - TICKS_PER_SECOND / 120,
            last: None,
        }
    }
}

impl KeyframeClock {
    /// Whether the frame at `pts` (ticks) should be a key frame; remembers the answer.
    pub fn should_force(&mut self, pts: i64) -> bool {
        let due = self.last.is_none_or(|l| pts - l >= self.interval);
        if due {
            self.last = Some(pts);
        }
        due
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Quality;

    fn plan(w: u32, h: u32, codec: Codec) -> VideoPlan {
        VideoPlan {
            width: w,
            height: h,
            fps: 30,
            codec,
            bitrate_bps: bitrate_bps(codec, 30, Quality::High, w, h),
            quality: Quality::High,
        }
    }

    #[test]
    fn an_h264_plan_is_kept_as_it_is() {
        let p = plan(1920, 1080, Codec::H264);
        assert_eq!(h264_plan_for(&p).unwrap(), p);
    }

    #[test]
    fn a_hevc_plan_becomes_h264_with_the_h264_bit_rate() {
        let p = plan(2560, 1440, Codec::Hevc);
        let q = h264_plan_for(&p).unwrap();
        assert_eq!(q.codec, Codec::H264);
        assert_eq!((q.width, q.height, q.fps), (2560, 1440, 30));
        assert_eq!(
            q.bitrate_bps,
            bitrate_bps(Codec::H264, 30, Quality::High, 2560, 1440)
        );
    }

    #[test]
    fn the_platform_plan_names_the_codec_the_file_gets() {
        let hevc = plan(3840, 2160, Codec::Hevc);
        let p = platform_plan(hevc);
        if cfg!(windows) {
            assert_eq!(p.codec, Codec::H264);
            assert_eq!((p.width, p.height), (3840, 2160));
            // Wider than H.264 allows: kept, the recorder falls back to smaller sizes.
            let wide = plan(5120, 2880, Codec::Hevc);
            assert_eq!(platform_plan(wide), wide);
        } else {
            assert_eq!(p, hevc);
        }
        let h = plan(1920, 1080, Codec::H264);
        assert_eq!(platform_plan(h), h);
    }

    #[test]
    fn wider_than_h264_allows_is_the_unavailable_error_so_smaller_sizes_are_tried() {
        let e = h264_plan_for(&plan(5120, 2880, Codec::Hevc)).unwrap_err();
        assert!(crate::encoder::is_hardware_encoder_unavailable(&e));
    }

    #[test]
    fn odd_or_empty_sizes_are_invalid_input() {
        assert!(h264_plan_for(&plan(1921, 1080, Codec::H264)).is_err());
        assert!(h264_plan_for(&plan(0, 1080, Codec::H264)).is_err());
    }

    #[test]
    fn pairs_are_packed_high_then_low() {
        assert_eq!(pack_pair(1920, 1080), 0x0000_0780_0000_0438);
        assert_eq!(pack_pair(30, 1), (30u64 << 32) | 1);
    }

    #[test]
    fn a_key_frame_is_asked_for_on_the_first_frame_and_every_two_seconds() {
        let mut k = KeyframeClock::default();
        let step = 4_000; // 30 fps
        let forced: Vec<i64> = (0..200)
            .map(|i| i * step)
            .filter(|&pts| k.should_force(pts))
            .collect();
        // 0, then the frame at (or just before) each 2 s mark: frames 60, 120, 180.
        assert_eq!(forced, [0, 240_000, 480_000, 720_000]);
    }

    #[test]
    fn a_pause_gap_does_not_leave_the_clock_waiting() {
        let mut k = KeyframeClock::default();
        assert!(k.should_force(0));
        assert!(!k.should_force(4_000));
        // The next frame is 10 s later (file time cannot jump, but dropped frames can): forced.
        assert!(k.should_force(1_200_000));
    }
}
