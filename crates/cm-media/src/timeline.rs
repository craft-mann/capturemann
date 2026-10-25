//! Track alignment on the session clock (REC-1 clock origin, REC-4 pauses, AUD-4).
//!
//! Every recorded file has *continuous* timestamps: pausing removes the gap from each file, so
//! `file time = session time - the pause gaps before it`. A track's manifest `start` is the
//! session time of its first sample (the screen's first frame ~0.2 s after the origin, the camera
//! 150-250 ms later, the microphone ~290 ms *before* the screen). To line the tracks up we map each
//! start into file time and take the difference against the screen's first frame; because every
//! track drops the same gaps, that difference is also the offset for every later sample.
//!
//! The exported video starts at the first screen frame (t = 0). Material from a track that starts
//! earlier is trimmed (`atrim`/`trim`); a track that starts later is delayed (`adelay`, or the first
//! camera frame held with `tpad` so the bubble is there from the first frame).

use serde_json::Value;

/// Ticks per second of `MediaTime` (120,000).
pub const TICKS_PER_SECOND: i64 = 120_000;

/// A closed pause gap in session ticks, `(from, to)`.
pub type Gap = (i64, i64);

/// Reads `manifest.gaps` (`[{from, to}]`), dropping empty and malformed entries, sorted.
pub fn gaps_from_json(v: Option<&Value>) -> Vec<Gap> {
    let mut gaps: Vec<Gap> = v
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|g| Some((g.get("from")?.as_i64()?, g.get("to")?.as_i64()?)))
                .filter(|(f, t)| t > f)
                .collect()
        })
        .unwrap_or_default();
    gaps.sort_unstable();
    gaps
}

/// File time of a session time: the session time minus the pause time before it. A time inside a
/// gap maps to the gap's start.
pub fn session_to_file(t: i64, gaps: &[Gap]) -> i64 {
    let removed: i64 = gaps
        .iter()
        .map(|&(from, to)| (t.min(to) - from).max(0))
        .sum();
    t - removed
}

/// Seconds from the screen's first sample to this track's first sample, on the file-time axis
/// shared by all tracks. Positive: the track starts after the screen.
pub fn offset_seconds(track_start: i64, screen_start: i64, gaps: &[Gap]) -> f64 {
    let d = session_to_file(track_start, gaps) - session_to_file(screen_start, gaps);
    d as f64 / TICKS_PER_SECOND as f64
}

/// Length of the screen file in seconds: its session span minus the pauses inside it.
pub fn file_duration_seconds(start: i64, end: i64, gaps: &[Gap]) -> f64 {
    let d = session_to_file(end, gaps) - session_to_file(start, gaps);
    d.max(0) as f64 / TICKS_PER_SECOND as f64
}

/// How a track is placed relative to the video start.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Placement {
    /// Starts with the video (within 1 ms).
    Aligned,
    /// Starts this many seconds after the video start.
    Delay(f64),
    /// Starts this many seconds before the video start: that much is cut off the front.
    Trim(f64),
}

/// Classifies an offset (1 ms is below anything audible or visible and below `adelay`'s
/// resolution).
pub fn placement(offset_s: f64) -> Placement {
    if offset_s.abs() < 0.0005 {
        Placement::Aligned
    } else if offset_s > 0.0 {
        Placement::Delay(offset_s)
    } else {
        Placement::Trim(-offset_s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const MS: i64 = 120; // ticks per millisecond

    #[test]
    fn gaps_shift_file_time() {
        let gaps = [(10_000 * MS, 12_000 * MS), (20_000 * MS, 21_000 * MS)];
        assert_eq!(session_to_file(5_000 * MS, &gaps), 5_000 * MS);
        assert_eq!(session_to_file(12_000 * MS, &gaps), 10_000 * MS);
        assert_eq!(session_to_file(15_000 * MS, &gaps), 13_000 * MS);
        assert_eq!(session_to_file(25_000 * MS, &gaps), 22_000 * MS);
        // Inside a gap: the gap's start.
        assert_eq!(session_to_file(11_000 * MS, &gaps), 10_000 * MS);
    }

    /// REC-1/AUD-4 shapes: screen at 390 ms, camera 190 ms later, mic 290 ms before the screen.
    #[test]
    fn offsets_follow_the_start_times() {
        let screen = 390 * MS;
        let camera = 580 * MS;
        let mic = 100 * MS; // the mic starts before the screen's first frame
        let sys = 390 * MS;
        assert!((offset_seconds(camera, screen, &[]) - 0.190).abs() < 1e-9);
        assert!((offset_seconds(mic, screen, &[]) + 0.290).abs() < 1e-9);
        assert_eq!(offset_seconds(sys, screen, &[]), 0.0);
        assert_eq!(placement(0.190), Placement::Delay(0.190));
        assert_eq!(placement(-0.290), Placement::Trim(0.290));
        assert_eq!(placement(0.0004), Placement::Aligned);
    }

    /// A track that starts after the first pause (rare) is placed on the file axis.
    #[test]
    fn a_start_after_a_pause_uses_file_time() {
        let gaps = [(1_000 * MS, 3_000 * MS)];
        // Screen at 200 ms; camera joins at session 3,500 ms = file 1,500 ms.
        assert!((offset_seconds(3_500 * MS, 200 * MS, &gaps) - 1.3).abs() < 1e-9);
    }

    #[test]
    fn duration_drops_the_pauses_inside_the_file() {
        let gaps = [(5_000 * MS, 6_000 * MS)];
        let d = file_duration_seconds(200 * MS, 10_200 * MS, &gaps);
        assert!((d - 9.0).abs() < 1e-9);
        assert_eq!(file_duration_seconds(10, 5, &[]), 0.0);
    }

    #[test]
    fn manifest_gaps_are_read_sorted_and_cleaned() {
        let v = json!([{"from": 500, "to": 900}, {"from": 100, "to": 200}, {"from": 5, "to": 5}, {"x": 1}]);
        assert_eq!(gaps_from_json(Some(&v)), vec![(100, 200), (500, 900)]);
        assert!(gaps_from_json(None).is_empty());
    }
}
