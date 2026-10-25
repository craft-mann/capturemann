//! Microphone mute (AUD-6): the spans of host time in which the mic track is silenced.
//!
//! Muting never stops or restarts the track. The recorder keeps running, so the session clock and
//! the sync with the other tracks are untouched; the pipeline writes digital silence (exact
//! zeros) for the frames that fall inside a muted span.
//!
//! # The boundary rule
//!
//! A frame is silenced if and only if its capture time (the OS stamp of the chunk plus the frame's
//! offset in it, plus the calibration offset) is `>= from` and `< to` of a span (`to` is open
//! while the mic is muted). The rule is on the stamp axis, like the pause gates, so it does not
//! depend on how the OS cut the audio into chunks, and it covers chunks that were captured before
//! the toggle but are still queued.
//!
//! The toggle takes its time under the same lock the worker reads the spans with: a chunk that
//! the worker processed before the toggle only holds frames captured before the toggle time (a
//! frame cannot be captured after the moment the worker sees it), and every chunk it processes
//! after the toggle sees the new span. So no real sample of a muted span can reach the files.

use std::sync::{Mutex, MutexGuard};

use cm_core::MediaTime;

/// One muted span in host nanoseconds. `to` is `None` while the microphone is still muted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MuteSpan {
    pub from_ns: i128,
    pub to_ns: Option<i128>,
}

/// What a toggle did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MuteChange {
    /// `false` when the state was already what was asked for.
    pub changed: bool,
    /// Session time of the call.
    pub at: MediaTime,
}

/// The spans shared between the recorder (which toggles) and the worker's pipeline (which
/// silences).
#[derive(Debug, Default)]
pub struct MuteControl {
    spans: Mutex<Vec<MuteSpan>>,
}

impl MuteControl {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> MutexGuard<'_, Vec<MuteSpan>> {
        self.spans.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Mutes or unmutes. `now` reads the clock while the lock is held (see the module docs) and
    /// returns `(session time, host ns)`.
    pub fn set(&self, muted: bool, now: impl FnOnce() -> (MediaTime, i128)) -> MuteChange {
        let mut spans = self.lock();
        let open = spans.last().is_some_and(|s| s.to_ns.is_none());
        let (at, host_ns) = now();
        if open == muted {
            return MuteChange { changed: false, at };
        }
        if muted {
            spans.push(MuteSpan {
                from_ns: host_ns,
                to_ns: None,
            });
        } else if let Some(last) = spans.last_mut() {
            last.to_ns = Some(host_ns.max(last.from_ns));
        }
        MuteChange { changed: true, at }
    }

    pub fn is_muted(&self) -> bool {
        self.lock().last().is_some_and(|s| s.to_ns.is_none())
    }

    pub fn snapshot(&self) -> Vec<MuteSpan> {
        self.lock().clone()
    }
}

/// `ceil(ns * rate / 1e9)` clamped to `0..=max`: the first frame index whose time is `>= ns`
/// after the chunk start.
pub(crate) fn first_frame_at_or_after(ns: i128, rate: u32, max: usize) -> usize {
    if ns <= 0 {
        return 0;
    }
    let num = ns.saturating_mul(i128::from(rate));
    let f = (num + 999_999_999).div_euclid(1_000_000_000);
    usize::try_from(f.min(max as i128)).unwrap_or(max)
}

/// Splits `[a, b)` by the muted ranges (sorted, disjoint) into `(start, end, muted)` pieces.
pub(crate) fn split_by_mute(
    a: usize,
    b: usize,
    muted: &[(usize, usize)],
) -> Vec<(usize, usize, bool)> {
    let mut out = Vec::new();
    let mut cur = a;
    for &(ms, me) in muted {
        let s = ms.max(a);
        let e = me.min(b);
        if e <= s || e <= cur {
            continue;
        }
        let s = s.max(cur);
        if s > cur {
            out.push((cur, s, false));
        }
        out.push((s, e, true));
        cur = e;
    }
    if cur < b {
        out.push((cur, b, false));
    }
    out
}

/// Zeroes the parts of `data` (interleaved, `ch` channels, first frame at output frame `pos`)
/// that lie inside the muted output ranges.
pub(crate) fn zero_ranges(data: &mut [f32], ch: usize, pos: u64, ranges: &[(u64, u64)]) {
    let frames = (data.len() / ch.max(1)) as u64;
    let end = pos + frames;
    for &(s, e) in ranges {
        let s = s.max(pos);
        let e = e.min(end);
        if e > s {
            let (i, j) = ((s - pos) as usize * ch, (e - pos) as usize * ch);
            data[i..j].fill(0.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_index_rounds_up_to_the_first_frame_at_or_after_the_time() {
        // 48 kHz: one frame is 20_833.33 ns.
        assert_eq!(first_frame_at_or_after(0, 48_000, 480), 0);
        assert_eq!(first_frame_at_or_after(-5, 48_000, 480), 0);
        assert_eq!(first_frame_at_or_after(1, 48_000, 480), 1);
        assert_eq!(first_frame_at_or_after(20_833, 48_000, 480), 1);
        assert_eq!(first_frame_at_or_after(20_834, 48_000, 480), 2);
        // Exactly on a frame: that frame is included.
        assert_eq!(first_frame_at_or_after(1_000_000, 1000, 480), 1);
        assert_eq!(first_frame_at_or_after(9_000_000_000, 48_000, 480), 480);
    }

    #[test]
    fn split_covers_the_range_exactly() {
        assert_eq!(split_by_mute(0, 10, &[]), vec![(0, 10, false)]);
        assert_eq!(
            split_by_mute(0, 10, &[(3, 5)]),
            vec![(0, 3, false), (3, 5, true), (5, 10, false)]
        );
        assert_eq!(split_by_mute(0, 10, &[(0, 10)]), vec![(0, 10, true)]);
        assert_eq!(
            split_by_mute(2, 8, &[(0, 3), (6, 20)]),
            vec![(2, 3, true), (3, 6, false), (6, 8, true)]
        );
        assert_eq!(split_by_mute(4, 6, &[(0, 3)]), vec![(4, 6, false)]);
    }

    #[test]
    fn zeroing_respects_the_stream_position() {
        let mut d = vec![1.0f32; 8]; // 4 stereo frames at positions 10..14
        zero_ranges(&mut d, 2, 10, &[(11, 13)]);
        assert_eq!(d, vec![1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0]);
    }

    #[test]
    fn toggles_are_idempotent() {
        let c = MuteControl::new();
        let t = |n: i64| move || (MediaTime::from_ticks(n), i128::from(n) * 1000);
        assert!(!c.is_muted());
        assert!(!c.set(false, t(1)).changed);
        assert!(c.set(true, t(2)).changed);
        assert!(c.is_muted());
        assert!(!c.set(true, t(3)).changed);
        assert!(c.set(false, t(4)).changed);
        assert_eq!(
            c.snapshot(),
            vec![MuteSpan {
                from_ns: 2000,
                to_ns: Some(4000)
            }]
        );
    }
}
