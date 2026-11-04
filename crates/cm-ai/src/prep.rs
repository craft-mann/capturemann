//! Audio preparation candidates for transcription (0.2.7 quality plan, section 3, ranks 2, 5 and 6).
//! Pure and display-free; nothing here is wired into [`crate::pieces`] or [`crate::pipeline`] yet
//! (the report `verification/benchmarks/audio-prep-2026-11-04.md` names the hooks).
//!
//! * [`lift_per_region`]: the quiet-input lift per stretch of speech instead of per call: a quiet
//!   speaker is brought up, the room between speakers is not.
//! * [`trim_silences`]: stretches of silence longer than `min_silence_secs` are cut to a short
//!   keep on both sides of the cut; an [`OffsetMap`] gives every decoded time back in source time,
//!   so word times stay right. Nothing is ever cut from a stored file: the trimmed audio is a copy
//!   for decoding.
//! * [`plan_after_speech`]: piece cuts just after the end of speech instead of at the quietest
//!   100 ms, so a piece does not end in a long silence (the window of zero padding that follows
//!   it is the condition in which Whisper writes text nobody said).
//!
//! All of them are general signal rules; none depends on a language or on words.

use std::ops::Range;

use crate::audio::SAMPLE_RATE;

/// One analysis frame: 20 ms.
const FRAME: usize = (SAMPLE_RATE as usize) / 50;

fn samples_of(secs: f64) -> usize {
    (secs * f64::from(SAMPLE_RATE)).round() as usize
}

fn frame_db(samples: &[f32]) -> Vec<f32> {
    samples
        .as_chunks::<FRAME>()
        .0
        .iter()
        .map(|c| 10.0 * (c.iter().map(|s| s * s).sum::<f32>() / FRAME as f32 + 1e-12).log10())
        .collect()
}

/// Where the recording is clearly not silent: runs of 20 ms frames above `floor + frac *
/// (speech - floor)` (floor: 10th percentile of the frame levels, speech: 97th, so a recording that is mostly silent still has a speech level), with dips of up
/// to `bridge_secs` bridged and runs shorter than `min_secs` dropped (a click is not speech).
/// Sample ranges. A low `frac` (0.2 to 0.25) is the careful choice for cutting, because a missed
/// quiet word is lost, while an extra region only keeps some silence. Empty when floor and speech
/// are less than 6 dB apart: nothing separates them, so nothing may be cut.
pub fn loud_regions(
    samples: &[f32],
    frac: f32,
    bridge_secs: f64,
    min_secs: f64,
) -> Vec<Range<usize>> {
    let db = frame_db(samples);
    if db.len() < 10 {
        return Vec::new();
    }
    let mut sorted = db.clone();
    sorted.sort_by(f32::total_cmp);
    let floor = sorted[sorted.len() / 10];
    let speech = sorted[sorted.len() * 97 / 100];
    if speech - floor < 6.0 {
        return Vec::new();
    }
    let thr = floor + (speech - floor) * frac;
    let bridge = (bridge_secs * 50.0) as usize;
    let mut runs: Vec<(usize, usize)> = Vec::new();
    for (i, l) in db.iter().enumerate() {
        if *l <= thr {
            continue;
        }
        match runs.last_mut() {
            Some(last) if i <= last.1 + bridge => last.1 = i + 1,
            _ => runs.push((i, i + 1)),
        }
    }
    runs.into_iter()
        .filter(|(a, b)| (b - a) as f64 / 50.0 >= min_secs)
        .map(|(a, b)| a * FRAME..(b * FRAME).min(samples.len()))
        .collect()
}

// ---- per-region lift -------------------------------------------------------------------------

/// How [`lift_per_region`] lifts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RegionLift {
    /// A region at or above this RMS level (dBFS) is left alone.
    pub below_db: f64,
    /// The level aimed at (dBFS RMS).
    pub target_db: f64,
    /// The most gain applied (dB).
    pub max_gain_db: f64,
    /// The gain moves to and from a region's value over this long, outside the region (seconds).
    pub ramp_secs: f64,
}

impl Default for RegionLift {
    fn default() -> Self {
        RegionLift {
            below_db: -26.0,
            target_db: -20.0,
            max_gain_db: 15.0,
            ramp_secs: 0.2,
        }
    }
}

/// `samples` with every region that is quieter than `opts.below_db` lifted towards `opts.target_db`
/// (at most `opts.max_gain_db`), with unity gain between regions and linear ramps around each one.
/// Same length as the input, so no time changes. `regions` are sample ranges, sorted, for example
/// from [`loud_regions`] or [`crate::vad::speech_regions`].
pub fn lift_per_region(samples: &[f32], regions: &[Range<usize>], opts: RegionLift) -> Vec<f32> {
    let mut gain = vec![1.0f32; samples.len()];
    let ramp = samples_of(opts.ramp_secs).max(1);
    for r in regions {
        let r = r.start.min(samples.len())..r.end.min(samples.len());
        if r.is_empty() {
            continue;
        }
        let rms = (samples[r.clone()]
            .iter()
            .map(|s| f64::from(*s).powi(2))
            .sum::<f64>()
            / r.len() as f64)
            .sqrt();
        let level = 20.0 * rms.max(1e-12).log10();
        if level >= opts.below_db || level < -90.0 {
            continue;
        }
        let g = 10f64.powf((opts.target_db - level).min(opts.max_gain_db) / 20.0) as f32;
        for x in &mut gain[r.clone()] {
            *x = x.max(g);
        }
        let from = r.start.saturating_sub(ramp);
        for (k, x) in gain[from..r.start].iter_mut().enumerate() {
            let t = (k + 1) as f32 / (r.start - from + 1) as f32;
            *x = x.max(1.0 + (g - 1.0) * t);
        }
        let to = (r.end + ramp).min(samples.len());
        for (k, x) in gain[r.end..to].iter_mut().enumerate() {
            let t = 1.0 - (k + 1) as f32 / (to - r.end + 1) as f32;
            *x = x.max(1.0 + (g - 1.0) * t);
        }
    }
    samples.iter().zip(gain).map(|(s, g)| s * g).collect()
}

// ---- silence trimming with an offset map ----------------------------------------------------

/// Where the samples of a trimmed copy came from: pieces of equal length, in order.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct OffsetMap {
    /// `(decoded range, source range)`, contiguous in the decoded audio.
    pub parts: Vec<(Range<usize>, Range<usize>)>,
}

impl OffsetMap {
    /// The identity map for `len` samples.
    pub fn identity(len: usize) -> Self {
        OffsetMap {
            parts: vec![(0..len, 0..len)],
        }
    }

    /// Source sample of a decoded sample (clamped to the last part).
    pub fn to_source(&self, decoded: usize) -> usize {
        let i = self.parts.partition_point(|(d, _)| d.end <= decoded);
        match self.parts.get(i) {
            Some((d, s)) => s.start + (decoded.max(d.start) - d.start),
            None => self.parts.last().map_or(decoded, |(_, s)| s.end),
        }
    }

    /// The same for seconds.
    pub fn to_source_secs(&self, decoded_secs: f64) -> f64 {
        self.to_source(samples_of(decoded_secs.max(0.0))) as f64 / f64::from(SAMPLE_RATE)
    }

    /// Seconds of source audio that were cut away.
    pub fn removed_secs(&self, source_len: usize) -> f64 {
        let kept: usize = self.parts.iter().map(|(d, _)| d.len()).sum();
        source_len.saturating_sub(kept) as f64 / f64::from(SAMPLE_RATE)
    }
}

/// Cuts every silence longer than `min_silence_secs` down to `2 * keep_secs`: `keep_secs` after the
/// speech before it and `keep_secs` before the speech after it (a leading or trailing silence keeps
/// `keep_secs` next to the speech only). `speech` are sample ranges from [`loud_regions`]. With no
/// regions nothing is cut. Returns the trimmed copy and the map back to the source.
pub fn trim_silences(
    samples: &[f32],
    speech: &[Range<usize>],
    min_silence_secs: f64,
    keep_secs: f64,
) -> (Vec<f32>, OffsetMap) {
    let (min, keep) = (samples_of(min_silence_secs), samples_of(keep_secs));
    if speech.is_empty() {
        return (samples.to_vec(), OffsetMap::identity(samples.len()));
    }
    // Source ranges to keep, merged.
    let mut keep_ranges: Vec<Range<usize>> = Vec::new();
    let mut at = 0usize; // end of the previous speech (0: start of the audio)
    let push = |a: usize, b: usize, out: &mut Vec<Range<usize>>| {
        if b > a {
            match out.last_mut() {
                Some(l) if a <= l.end => l.end = l.end.max(b),
                _ => out.push(a..b),
            }
        }
    };
    for (i, r) in speech.iter().enumerate() {
        let gap = r.start.saturating_sub(at);
        if gap > min {
            // after the previous speech (not for the leading silence), then before this one
            if i > 0 {
                push(at, (at + keep).min(r.start), &mut keep_ranges);
            }
            push(r.start - keep, r.end, &mut keep_ranges);
        } else {
            push(at, r.end, &mut keep_ranges);
        }
        at = r.end;
    }
    let tail = samples.len().saturating_sub(at);
    if tail > min {
        push(at, at + keep, &mut keep_ranges);
    } else {
        push(at, samples.len(), &mut keep_ranges);
    }
    let mut out = Vec::new();
    let mut parts = Vec::new();
    for k in keep_ranges {
        let d = out.len()..out.len() + k.len();
        out.extend_from_slice(&samples[k.clone()]);
        parts.push((d, k));
    }
    (out, OffsetMap { parts })
}

/// Stretches of `min_secs` or more that hold no speech by a conservative test: every 1 s window
/// (hop 0.5 s) whose frame levels have a spread (95th minus 10th percentile of 20 ms levels) under
/// 9 dB and a top (95th percentile) under 10 dB above the recording's 10th-percentile level is quiet.
/// Speech, even faint, has syllables and pauses, so its spread is wider; room tone, hum and a fan
/// are flat. Adjacent quiet windows are merged. Sample ranges; empty for audio shorter than 2 s.
pub fn quiet_regions(samples: &[f32], min_secs: f64) -> Vec<Range<usize>> {
    let db = frame_db(samples);
    if db.len() < 100 {
        return Vec::new();
    }
    let mut sorted = db.clone();
    sorted.sort_by(f32::total_cmp);
    let floor = sorted[sorted.len() / 10];
    let (win, hop) = (50usize, 25usize);
    let pct = |v: &mut Vec<f32>, q: usize| {
        v.sort_by(f32::total_cmp);
        v[(v.len() - 1) * q / 100]
    };
    let mut runs: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i + win <= db.len() {
        let mut w = db[i..i + win].to_vec();
        let (lo, hi) = (pct(&mut w, 10), pct(&mut w, 95));
        if hi - lo < 9.0 && hi < floor + 10.0 {
            match runs.last_mut() {
                Some(l) if i <= l.1 => l.1 = i + win,
                _ => runs.push((i, i + win)),
            }
        }
        i += hop;
    }
    runs.into_iter()
        .filter(|(a, b)| (b - a) as f64 / 50.0 >= min_secs)
        .map(|(a, b)| a * FRAME..(b * FRAME).min(samples.len()))
        .collect()
}

/// [`trim_silences`] driven by [`quiet_regions`]: every quiet stretch longer than `min_silence_secs`
/// is cut to `keep_secs` at each end (a leading or trailing one keeps `keep_secs` next to the
/// speech only). Returns the copy and the map back to the source.
pub fn trim_quiet(samples: &[f32], min_silence_secs: f64, keep_secs: f64) -> (Vec<f32>, OffsetMap) {
    let keep = samples_of(keep_secs);
    let quiet = quiet_regions(samples, min_silence_secs);
    if quiet.is_empty() {
        return (samples.to_vec(), OffsetMap::identity(samples.len()));
    }
    let mut kept: Vec<Range<usize>> = Vec::new();
    let mut at = 0usize;
    for q in &quiet {
        let lead = q.start == 0;
        let trail = q.end >= samples.len();
        let a = if lead { 0 } else { (q.start + keep).min(q.end) };
        let b = if trail {
            samples.len()
        } else {
            q.end.saturating_sub(keep).max(a)
        };
        // keep the speech before the stretch and `keep` of the stretch, drop the middle
        if q.start > at {
            kept.push(at..a.min(q.end).max(at));
        }
        at = b.max(at);
        if lead {
            at = b.max(q.start);
        }
    }
    if at < samples.len() {
        kept.push(at..samples.len());
    }
    // merge touching ranges
    let mut merged: Vec<Range<usize>> = Vec::new();
    for r in kept.into_iter().filter(|r| !r.is_empty()) {
        match merged.last_mut() {
            Some(l) if r.start <= l.end => l.end = l.end.max(r.end),
            _ => merged.push(r),
        }
    }
    let mut out = Vec::new();
    let mut parts = Vec::new();
    for k in merged {
        let d = out.len()..out.len() + k.len();
        out.extend_from_slice(&samples[k.clone()]);
        parts.push((d, k));
    }
    (out, OffsetMap { parts })
}

// ---- pieces that end after speech -------------------------------------------------------------

/// The target length of a piece, as in [`crate::pieces`].
const PIECE_SECS: f64 = 300.0;
const LONG_AUDIO_SECS: f64 = 600.0;
const SEARCH_SECS: f64 = 20.0;
/// How long after the last speech a cut falls (seconds).
const AFTER_SPEECH_SECS: f64 = 0.4;

/// The cuts of [`crate::pieces::plan`], but each at the end of a stretch of speech (plus 0.4 s)
/// nearest to the target instead of the quietest 100 ms: the piece then ends a moment after
/// someone stopped, not in the middle of a long silence. Falls back to the middle of the quietest
/// 100 ms in the search window when the window holds no speech end. Audio up to 600 s is one piece.
pub fn plan_after_speech(samples: &[f32]) -> Vec<Range<usize>> {
    let total = samples.len();
    if total <= samples_of(LONG_AUDIO_SECS) {
        #[allow(clippy::single_range_in_vec_init)]
        return vec![0..total];
    }
    let regions = loud_regions(samples, 0.25, 0.3, 0.16);
    let (piece, search) = (samples_of(PIECE_SECS), samples_of(SEARCH_SECS));
    let mut out = Vec::new();
    let mut from = 0usize;
    while total - from > piece + piece / 2 {
        let target = from + piece;
        let (lo, hi) = (target - search, target + search);
        // speech ends in the window, with at least 0.8 s of quiet after them
        let best = regions
            .iter()
            .enumerate()
            .filter(|(_, r)| r.end >= lo && r.end <= hi)
            .filter(|(i, r)| {
                regions
                    .get(i + 1)
                    .is_none_or(|n| n.start >= r.end + samples_of(0.8))
            })
            .min_by_key(|(_, r)| r.end.abs_diff(target))
            .map(|(_, r)| (r.end + samples_of(AFTER_SPEECH_SECS)).min(total));
        let cut = best.unwrap_or_else(|| quietest(samples, lo, hi));
        out.push(from..cut);
        from = cut;
    }
    out.push(from..total);
    out
}

fn quietest(samples: &[f32], lo: usize, hi: usize) -> usize {
    let hi = hi.min(samples.len());
    let lo = lo.min(hi);
    let frame = SAMPLE_RATE as usize / 10;
    let frames: Vec<f64> = samples[lo..hi]
        .chunks_exact(frame)
        .map(|c| c.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>() / frame as f64)
        .collect();
    let best = frames
        .iter()
        .enumerate()
        .min_by(|a, b| a.1.total_cmp(b.1))
        .map_or(0, |(i, _)| i);
    lo + best * frame + frame / 2
}

/// Seconds of quiet at the end of `range` (frames below the loud-region threshold of the whole
/// recording), for the "how many pieces end in a long silence" count.
pub fn tail_silence_secs(samples: &[f32], speech: &[Range<usize>], range: &Range<usize>) -> f64 {
    let last_end = speech
        .iter()
        .filter(|r| r.start < range.end)
        .map(|r| r.end.min(range.end))
        .max()
        .unwrap_or(range.start)
        .max(range.start);
    let _ = samples;
    (range.end - last_end) as f64 / f64::from(SAMPLE_RATE)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A deterministic "voice": a 200 Hz tone with a slow amplitude wobble.
    fn voice(secs: f64, amp: f32) -> Vec<f32> {
        (0..samples_of(secs))
            .map(|i| {
                let t = i as f32 / SAMPLE_RATE as f32;
                amp * (2.0 * std::f32::consts::PI * 200.0 * t).sin() * (0.6 + 0.4 * (t * 7.0).sin())
            })
            .collect()
    }

    fn hiss(secs: f64, amp: f32) -> Vec<f32> {
        let mut x: u32 = 12345;
        (0..samples_of(secs))
            .map(|_| {
                x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                amp * ((x >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0)
            })
            .collect()
    }

    fn scene(parts: &[(f64, bool, f32)]) -> Vec<f32> {
        let mut v = Vec::new();
        for &(secs, speech, amp) in parts {
            v.extend(if speech {
                voice(secs, amp)
            } else {
                hiss(secs, amp)
            });
        }
        v
    }

    #[test]
    fn loud_regions_find_speech_and_ignore_a_click() {
        let mut v = scene(&[(3.0, false, 0.001), (2.0, true, 0.2), (3.0, false, 0.001)]);
        v[10_000..10_030].fill(0.5); // a click
        let r = loud_regions(&v, 0.25, 0.3, 0.16);
        assert_eq!(r.len(), 1, "{r:?}");
        let (a, b) = (r[0].start as f64 / 16000.0, r[0].end as f64 / 16000.0);
        assert!((a - 3.0).abs() < 0.1 && (b - 5.0).abs() < 0.1, "{a} {b}");
    }

    #[test]
    fn nothing_separates_flat_audio() {
        assert!(loud_regions(&hiss(5.0, 0.01), 0.25, 0.3, 0.16).is_empty());
    }

    #[test]
    fn lift_is_per_region_and_keeps_the_length() {
        let v = scene(&[
            (2.0, false, 0.002),
            (2.0, true, 0.02),
            (2.0, false, 0.002),
            (2.0, true, 0.3),
        ]);
        let regions = loud_regions(&v, 0.25, 0.3, 0.16);
        assert_eq!(regions.len(), 2);
        let out = lift_per_region(&v, &regions, RegionLift::default());
        assert_eq!(out.len(), v.len());
        let rms = |x: &[f32]| (x.iter().map(|s| s * s).sum::<f32>() / x.len() as f32).sqrt();
        let (a, b) = (regions[0].clone(), regions[1].clone());
        // the quiet region comes up by about the cap, the loud one is untouched, the room between stays
        let lift = rms(&out[a.clone()]) / rms(&v[a.clone()]);
        assert!(lift > 4.0 && lift < 6.0, "lift {lift}");
        assert_eq!(&out[b.clone()], &v[b.clone()]);
        let mid = a.end + samples_of(0.6)..b.start - samples_of(0.6);
        assert_eq!(&out[mid.clone()], &v[mid]);
    }

    #[test]
    fn trimming_cuts_only_long_silences_and_maps_times_back() {
        // 1 s speech, 12 s silence, 1 s speech, 3 s silence, 1 s speech
        let v = scene(&[
            (1.0, true, 0.2),
            (12.0, false, 0.001),
            (1.0, true, 0.2),
            (3.0, false, 0.001),
            (1.0, true, 0.2),
        ]);
        let regions = loud_regions(&v, 0.25, 0.3, 0.16);
        assert_eq!(regions.len(), 3);
        let (t, map) = trim_silences(&v, &regions, 8.0, 1.0);
        // 12 s -> 2 s; the 3 s gap stays
        let cut = map.removed_secs(v.len());
        assert!((cut - 10.0).abs() < 0.3, "removed {cut}");
        assert_eq!(t.len() + (cut * 16000.0).round() as usize, v.len());
        // the second speech starts at 13 s in the source, at about 3 s in the copy
        let second_decoded = 1.0 + 1.0 + 1.0; // speech, kept 1 s, kept 1 s before the next
        let src = map.to_source_secs(second_decoded);
        assert!((src - 13.0).abs() < 0.15, "{src}");
        // the last speech: 17 s in the source
        let last_decoded = (t.len() as f64 / 16000.0) - 1.0;
        assert!((map.to_source_secs(last_decoded) - 17.0).abs() < 0.2);
        // every kept sample is the source sample the map says
        for (d, s) in &map.parts {
            assert_eq!(&t[d.clone()], &v[s.clone()]);
        }
    }

    #[test]
    fn quiet_regions_find_flat_room_and_leave_speech() {
        let mut v = scene(&[
            (12.0, false, 0.003),
            (3.0, true, 0.05),
            (12.0, false, 0.003),
        ]);
        // a faint "speaker": a voice 30 dB under the loud test voice, in a room at 0.003
        v.extend(scene(&[(4.0, true, 0.01)]));
        let q = quiet_regions(&v, 8.0);
        assert_eq!(q.len(), 2, "{q:?}");
        let (a, b) = (q[0].start as f64 / 16000.0, q[0].end as f64 / 16000.0);
        assert!(a < 0.6 && (b - 12.0).abs() < 1.2, "{a} {b}");
        // the faint voice at the end is not quiet
        assert!(q[1].end as f64 / 16000.0 <= 27.6, "{q:?}");
    }

    #[test]
    fn trim_quiet_keeps_the_map_honest() {
        let v = scene(&[
            (2.0, true, 0.2),
            (20.0, false, 0.003),
            (2.0, true, 0.2),
            (4.0, false, 0.003),
            (2.0, true, 0.2),
        ]);
        let (t, map) = trim_quiet(&v, 8.0, 1.0);
        let removed = map.removed_secs(v.len());
        assert!((removed - 18.0).abs() < 1.5, "removed {removed}");
        for (d, s) in &map.parts {
            assert_eq!(&t[d.clone()], &v[s.clone()]);
        }
        // the third burst of speech starts at 28 s in the source
        let last = t.len() as f64 / 16000.0 - 2.0;
        assert!((map.to_source_secs(last) - 28.0).abs() < 1.5);
    }

    #[test]
    fn trimming_without_regions_cuts_nothing() {
        let v = hiss(30.0, 0.001);
        let (t, map) = trim_silences(&v, &[], 8.0, 1.0);
        assert_eq!(t, v);
        assert_eq!(map.to_source(12345), 12345);
    }

    #[test]
    fn a_leading_and_trailing_silence_keep_one_second_next_to_speech() {
        let v = scene(&[(20.0, false, 0.001), (2.0, true, 0.2), (20.0, false, 0.001)]);
        let regions = loud_regions(&v, 0.25, 0.3, 0.16);
        let (t, map) = trim_silences(&v, &regions, 8.0, 1.0);
        assert!((t.len() as f64 / 16000.0 - 4.0).abs() < 0.3, "{}", t.len());
        assert!((map.to_source_secs(1.0) - 20.0).abs() < 0.15);
    }

    #[test]
    fn pieces_end_just_after_speech_not_in_silence() {
        // 700 s: speech in 20 s blocks every 30 s, but a 25 s silence right at the 300 s target
        let mut parts = Vec::new();
        let mut t = 0.0;
        while t < 700.0 {
            let silent = if (285.0..312.0).contains(&t) {
                27.0
            } else {
                10.0
            };
            parts.push((20.0, true, 0.2));
            parts.push((silent, false, 0.001));
            t += 20.0 + silent;
        }
        let v = scene(&parts);
        let cuts = plan_after_speech(&v);
        assert!(cuts.len() >= 2);
        assert_eq!(cuts[0].start, 0);
        assert_eq!(cuts.last().unwrap().end, v.len());
        for w in cuts.windows(2) {
            assert_eq!(w[0].end, w[1].start);
        }
        let regions = loud_regions(&v, 0.25, 0.3, 0.16);
        let after = tail_silence_secs(&v, &regions, &cuts[0]);
        assert!(after < 1.0, "the piece ends {after} s after speech");
        let plain = crate::pieces::plan(&v);
        let before = tail_silence_secs(&v, &regions, &plain[0]);
        assert!(before >= after, "{before} {after}");
    }

    #[test]
    fn short_audio_is_one_piece() {
        let v = hiss(100.0, 0.01);
        assert_eq!(plan_after_speech(&v), vec![0..v.len()]);
    }
}
