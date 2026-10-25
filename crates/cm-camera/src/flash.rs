//! Flash detection for the optical sync test (tests/manual/CAM-2.md): finds brightness steps in a
//! recorded video (via `ffmpeg`/`ffprobe`, post-recording only) and matches them against a log of
//! when a flashing window changed colour, or against the same steps in another track.

use std::path::Path;
use std::process::Command;

use cm_core::CmError;
use serde::{Deserialize, Serialize};

use crate::latency::{summarize, LatencySummary};

/// A brightness step in a video.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Edge {
    /// Seconds on the file's timeline (file time; equal to session time when nothing was paused, as in the sync probe), sub-frame interpolated.
    pub t_s: f64,
    /// Dark to bright.
    pub rising: bool,
}

/// What the flash window logged: when it changed colour, in host nanoseconds (`CLOCK_UPTIME_RAW`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FlashEvent {
    pub host_ns: u64,
    pub rising: bool,
}

/// Mean-luma time series of a video.
#[derive(Debug, Clone, PartialEq)]
pub struct LumaSeries {
    pub times_s: Vec<f64>,
    pub mean: Vec<f64>,
}

/// Finds rising and falling crossings of the midpoint between the 10th and 90th percentile
/// luma. `None` if the clip has no flash (contrast below `min_contrast` of 255).
pub fn detect_edges(s: &LumaSeries, min_contrast: f64) -> Option<Vec<Edge>> {
    if s.mean.len() < 3 || s.mean.len() != s.times_s.len() {
        return None;
    }
    let mut sorted = s.mean.clone();
    sorted.sort_by(f64::total_cmp);
    let at = |q: f64| sorted[((sorted.len() - 1) as f64 * q).round() as usize];
    let (lo, hi) = (at(0.1), at(0.9));
    if hi - lo < min_contrast {
        return None;
    }
    let mid = (lo + hi) / 2.0;
    let mut edges = Vec::new();
    for i in 1..s.mean.len() {
        let (a, b) = (s.mean[i - 1], s.mean[i]);
        if (a < mid) != (b < mid) {
            let f = (mid - a) / (b - a);
            let t = s.times_s[i - 1] + f * (s.times_s[i] - s.times_s[i - 1]);
            edges.push(Edge {
                t_s: t,
                rising: b > a,
            });
        }
    }
    Some(edges)
}

/// Signed offsets (detected - expected, seconds) for each expected edge that has a detected edge
/// of the same direction within `[-before_s, +after_s]`.
pub fn match_edges(expected: &[Edge], detected: &[Edge], before_s: f64, after_s: f64) -> Vec<f64> {
    expected
        .iter()
        .filter_map(|e| {
            detected
                .iter()
                .filter(|d| d.rising == e.rising)
                .map(|d| d.t_s - e.t_s)
                .filter(|o| *o >= -before_s && *o <= after_s)
                .min_by(|a, b| a.abs().total_cmp(&b.abs()))
        })
        .collect()
}

/// Offsets in ms summarized (median/p95/min/max over the matched edges).
pub fn summarize_offsets_ms(offsets_s: &[f64]) -> Option<LatencySummary> {
    let ms: Vec<f32> = offsets_s.iter().map(|o| (o * 1000.0) as f32).collect();
    summarize(&ms)
}

/// The flash log converted to the session timeline.
pub fn expected_edges(log: &[FlashEvent], origin_host_ns: u64) -> Vec<Edge> {
    log.iter()
        .map(|f| Edge {
            t_s: (f.host_ns as f64 - origin_host_ns as f64) / 1e9,
            rising: f.rising,
        })
        .collect()
}

/// Decodes `path` to a 16x16 gray series (ffmpeg) with per-frame times (ffprobe).
pub fn luma_series(path: &Path) -> Result<LumaSeries, CmError> {
    let probe = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "frame=pts_time",
            "-of",
            "csv=p=0",
        ])
        .arg(path)
        .output()
        .map_err(|e| CmError::internal(format!("ffprobe not available: {e}")))?;
    if !probe.status.success() {
        return Err(CmError::internal(format!(
            "ffprobe failed: {}",
            String::from_utf8_lossy(&probe.stderr)
        )));
    }
    let times_s: Vec<f64> = String::from_utf8_lossy(&probe.stdout)
        .lines()
        .filter_map(|l| l.trim().trim_end_matches(',').parse().ok())
        .collect();
    let dec = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(path)
        .args([
            "-map",
            "0:v:0",
            "-vf",
            "scale=16:16:flags=area,format=gray",
            "-fps_mode",
            "passthrough",
            "-f",
            "rawvideo",
            "-",
        ])
        .output()
        .map_err(|e| CmError::internal(format!("ffmpeg not available: {e}")))?;
    if !dec.status.success() {
        return Err(CmError::internal(format!(
            "ffmpeg failed: {}",
            String::from_utf8_lossy(&dec.stderr)
        )));
    }
    let mean: Vec<f64> = dec
        .stdout
        .as_chunks::<256>()
        .0
        .iter()
        .map(|f| f.iter().map(|&p| f64::from(p)).sum::<f64>() / 256.0)
        .collect();
    let n = mean.len().min(times_s.len());
    Ok(LumaSeries {
        times_s: times_s[..n].to_vec(),
        mean: mean[..n].to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 30 fps clip, dark/bright every `period` frames starting dark, brightness step at the frame.
    fn square(n: usize, period: usize, t0: f64) -> LumaSeries {
        LumaSeries {
            times_s: (0..n).map(|i| t0 + i as f64 / 30.0).collect(),
            mean: (0..n)
                .map(|i| if (i / period) % 2 == 1 { 220.0 } else { 20.0 })
                .collect(),
        }
    }

    #[test]
    fn detects_rising_and_falling_edges_with_subframe_interpolation() {
        let e = detect_edges(&square(120, 15, 0.0), 30.0).unwrap();
        assert_eq!(e.len(), 7);
        assert!(e[0].rising && !e[1].rising);
        // The step happens between frame 14 and 15: interpolated at the midpoint.
        assert!((e[0].t_s - (14.5 / 30.0)).abs() < 1e-9);
    }

    #[test]
    fn flat_video_has_no_flash() {
        let s = LumaSeries {
            times_s: (0..60).map(|i| f64::from(i) / 30.0).collect(),
            mean: vec![100.0; 60],
        };
        assert!(detect_edges(&s, 30.0).is_none());
    }

    #[test]
    fn matches_against_a_flash_log_and_reports_the_offset() {
        // Flash window changed colour at session t = 0.5 s, 1.0 s, ...; the camera file sees each
        // change 60 ms later.
        let origin = 1_000_000_000_000u64;
        let log: Vec<FlashEvent> = (1..=6)
            .map(|i| FlashEvent {
                host_ns: origin + i * 500_000_000,
                rising: i % 2 == 1,
            })
            .collect();
        let expected = expected_edges(&log, origin);
        let detected: Vec<Edge> = expected
            .iter()
            .map(|e| Edge {
                t_s: e.t_s + 0.060,
                rising: e.rising,
            })
            .collect();
        let offs = match_edges(&expected, &detected, 0.1, 0.5);
        assert_eq!(offs.len(), 6);
        let s = summarize_offsets_ms(&offs).unwrap();
        assert!((s.median_ms - 60.0).abs() < 0.01);
    }

    #[test]
    fn direction_must_agree_and_far_edges_are_ignored() {
        let exp = [Edge {
            t_s: 1.0,
            rising: true,
        }];
        let det = [
            Edge {
                t_s: 1.02,
                rising: false,
            },
            Edge {
                t_s: 3.0,
                rising: true,
            },
        ];
        assert!(match_edges(&exp, &det, 0.1, 0.5).is_empty());
    }
}
