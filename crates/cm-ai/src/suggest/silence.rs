//! AI-4 silence suggestions from the microphone level.
//!
//! The mic track is read as mono PCM ([`decode_mono_f32`] with the FFmpeg sidecar, or any
//! samples the caller already has). Each 20 ms frame gets an RMS level in dBFS. A frame is
//! silent when it is below the threshold. By default the threshold follows the recording
//! ([`adaptive_threshold_db`]): [`SPEECH_SHARE`] of the way from the noise floor (10th percentile
//! of the frame levels) to the speech level (90th percentile), never closer than
//! [`NOISE_MARGIN_DB`] to the floor, so a quiet recording and a loud one are judged by their own
//! speech, not by a fixed -40 dBFS. When the levels do not separate (less than
//! [`MIN_SPEECH_RANGE_DB`] between floor and speech) or `adaptive` is off, the fixed rule applies:
//! the configured level ([`DEFAULT_SILENCE_DB`], -40 dBFS) or [`NOISE_MARGIN_DB`] above the floor,
//! whichever is higher. A run of silent frames at least `min_silence_s`
//! long ([`DEFAULT_MIN_SILENCE_S`], 1.2 s, configurable 0.5 to 5 s) becomes a suggestion that
//! cuts the pause down to `keep_s` ([`DEFAULT_KEEP_S`], 0.4 s) of breathing room: half of it is
//! kept on each side, so the cut never starts or ends on a word.
//!
//! A silence that touches the start or the end of the track is not suggested on a clip shorter than
//! [`SHORT_CLIP_S`], nor on a longer one unless it lasts [`EDGE_SILENCE_MIN_S`] or more.
//!
//! Pauses that straddle a recording pause (the media files are compacted at a pause) are split at
//! the join and judged on each side, and every time is mapped to session time through the
//! track's start and the pause gaps.

use std::io::Read;
use std::path::Path;
use std::process::Stdio;

use cm_core::edit::{session_to_file, Span};
use cm_core::{CmError, MediaTime, PauseGap};

use super::{ticks_of, CutSuggestion};

/// Default silence level (dBFS).
pub const DEFAULT_SILENCE_DB: f32 = -40.0;
/// Default shortest pause that is suggested (seconds).
pub const DEFAULT_MIN_SILENCE_S: f32 = 1.2;
/// Default breathing room left in a cut pause (seconds).
pub const DEFAULT_KEEP_S: f32 = 0.4;
/// Allowed `min_silence_s`.
pub const MIN_SILENCE_RANGE: (f32, f32) = (0.5, 5.0);
/// Allowed `silence_db`.
pub const SILENCE_DB_RANGE: (f32, f32) = (-80.0, -10.0);
/// In a noisy room the threshold rises to this far above the noise floor, so steady room noise
/// is not all counted as speech.
pub const NOISE_MARGIN_DB: f32 = 6.0;
/// How far from the noise floor to the speech level the adaptive threshold sits.
pub const SPEECH_SHARE: f32 = 0.4;
/// Below this many dB between the noise floor and the speech level the levels do not separate
/// speech from room, and the fixed rule applies.
pub const MIN_SPEECH_RANGE_DB: f32 = 8.0;
/// Padding around spoken words and speaker turns that no suggestion may touch ([`veto_speech`]).
pub const VETO_PAD_S: f64 = 0.15;
/// A recording shorter than this never gets its lead-in or tail suggested as a pause (seconds).
pub const SHORT_CLIP_S: f64 = 60.0;
/// On a longer recording an edge silence of at least this long may be suggested (seconds).
pub const EDGE_SILENCE_MIN_S: f64 = 3.0;
/// Frame length (seconds).
pub const FRAME_S: f64 = 0.020;
/// A screen that changed more than this share of its pixels is "active" (AI-4 edge case).
pub const SCREEN_ACTIVITY_THRESHOLD: f64 = 0.05;
/// The sample rate [`decode_mono_f32`] callers normally ask for.
pub const ANALYSIS_RATE: u32 = 16_000;

/// What the caller can tune (`silence_db`, `min_silence_s` of the agent interface).
#[derive(Debug, Clone)]
pub struct SilenceOptions {
    pub silence_db: f32,
    pub min_silence_s: f32,
    /// Breathing room left in each cut pause.
    pub keep_s: f32,
    /// Also suggest pauses while the screen changes ("Include silent screen activity").
    pub include_screen_activity: bool,
    /// Set the threshold from the recording's own levels (see the module docs). Off: the fixed
    /// `silence_db` rule of 0.2.6.
    pub adaptive: bool,
}

impl Default for SilenceOptions {
    fn default() -> Self {
        SilenceOptions {
            silence_db: DEFAULT_SILENCE_DB,
            min_silence_s: DEFAULT_MIN_SILENCE_S,
            keep_s: DEFAULT_KEEP_S,
            include_screen_activity: false,
            adaptive: true,
        }
    }
}

impl SilenceOptions {
    /// A level the user chose: the fixed rule applies (no adaptive threshold), as in 0.2.6.
    pub fn with_level(silence_db: f32) -> Self {
        SilenceOptions {
            silence_db,
            adaptive: false,
            ..SilenceOptions::default()
        }
    }

    pub fn validate(&self) -> Result<(), CmError> {
        let (lo, hi) = SILENCE_DB_RANGE;
        if !(self.silence_db.is_finite() && (lo..=hi).contains(&self.silence_db)) {
            return Err(CmError::invalid_field(
                "silence_db",
                format!("must be between {lo} and {hi} dBFS"),
            ));
        }
        let (lo, hi) = MIN_SILENCE_RANGE;
        if !(self.min_silence_s.is_finite() && (lo..=hi).contains(&self.min_silence_s)) {
            return Err(CmError::invalid_field(
                "min_silence_s",
                format!("must be between {lo} and {hi} seconds"),
            ));
        }
        if !(self.keep_s.is_finite() && self.keep_s >= 0.0 && self.keep_s < self.min_silence_s) {
            return Err(CmError::invalid_field(
                "keep_s",
                "must be at least 0 and shorter than min_silence_s",
            ));
        }
        Ok(())
    }
}

/// Reads `file` as mono f32 PCM at `rate` Hz with the FFmpeg sidecar at `ffmpeg` (found by the
/// caller, for example with `cm_media::tools`). A missing file is `not_found`; a decode failure
/// is `internal` with FFmpeg's message.
pub fn decode_mono_f32(ffmpeg: &Path, file: &Path, rate: u32) -> Result<Vec<f32>, CmError> {
    if !file.exists() {
        return Err(CmError::not_found(format!("{}", file.display())));
    }
    let mut child = cm_platform::process::command(ffmpeg)
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(file)
        .args(["-vn", "-ac", "1", "-ar"])
        .arg(rate.to_string())
        .args(["-f", "f32le", "-"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| CmError::internal(format!("cannot run {}: {e}", ffmpeg.display())))?;
    let mut bytes = Vec::new();
    if let Some(mut out) = child.stdout.take() {
        out.read_to_end(&mut bytes)
            .map_err(|e| CmError::internal(format!("reading FFmpeg output: {e}")))?;
    }
    let output = child
        .wait_with_output()
        .map_err(|e| CmError::internal(e.to_string()))?;
    if !output.status.success() {
        return Err(CmError::internal(format!(
            "FFmpeg could not decode {}: {}",
            file.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| f32::from_le_bytes(*b))
        .collect())
}

/// Level of each [`FRAME_S`] frame in dBFS.
fn frame_levels(samples: &[f32], rate: u32) -> Vec<f32> {
    let n = ((FRAME_S * f64::from(rate)).round() as usize).max(1);
    samples
        .chunks(n)
        .filter(|c| c.len() == n)
        .map(|c| {
            let ms = c.iter().map(|s| f64::from(*s) * f64::from(*s)).sum::<f64>() / n as f64;
            (10.0 * (ms + 1e-12).log10()) as f32
        })
        .collect()
}

/// The fixed-rule silence threshold for these levels (dBFS): `silence_db`, or the noise floor plus
/// [`NOISE_MARGIN_DB`] when that is higher.
pub fn threshold_db(levels: &[f32], silence_db: f32) -> f32 {
    if levels.is_empty() {
        return silence_db;
    }
    let mut sorted = levels.to_vec();
    sorted.sort_by(f32::total_cmp);
    let floor = sorted[sorted.len() / 10];
    (floor + NOISE_MARGIN_DB)
        .max(silence_db)
        .min(silence_db.max(-20.0))
}

/// The threshold that follows the recording: [`SPEECH_SHARE`] of the way from the noise floor to
/// the speech level, at least [`NOISE_MARGIN_DB`] above the floor and at most -20 dBFS. The speech
/// level is the 90th percentile of the frame levels. Falls back to [`threshold_db`] when the two
/// are less than [`MIN_SPEECH_RANGE_DB`] apart.
pub fn adaptive_threshold_db(levels: &[f32], silence_db: f32) -> f32 {
    if levels.is_empty() {
        return silence_db;
    }
    let mut sorted = levels.to_vec();
    sorted.sort_by(f32::total_cmp);
    let floor = sorted[sorted.len() / 10];
    let speech = sorted[(sorted.len() * 9 / 10).min(sorted.len() - 1)];
    let range = speech - floor;
    if range < MIN_SPEECH_RANGE_DB {
        return threshold_db(levels, silence_db);
    }
    (floor + (range * SPEECH_SHARE).max(NOISE_MARGIN_DB)).min(-20.0)
}

/// Session time of file position `file` (ticks of compacted file time, from the media's time
/// zero): every gap whose compacted start is before it (or at it, for a start) lies in between.
fn file_to_session(gaps: &[PauseGap], file: i64, is_start: bool) -> i64 {
    let mut t = file;
    let mut removed = 0;
    for g in gaps {
        let at = g.from.ticks() - removed;
        let len = (g.to - g.from).ticks();
        if file > at || (is_start && file == at) {
            t += len;
        }
        removed += len;
    }
    t
}

/// Suggests pauses to cut from the mic samples. `track_start` is the session time of the first
/// sample (`TrackInfo.start`) and `gaps` the recording's pause gaps; the cuts come out in session
/// time. Nothing is applied. Cuts that overlap existing cuts are removed by
/// [`merge_cuts`](super::merge_cuts), not here.
pub fn silence_cuts(
    samples: &[f32],
    rate: u32,
    track_start: MediaTime,
    gaps: &[PauseGap],
    opts: &SilenceOptions,
) -> Result<Vec<CutSuggestion>, CmError> {
    opts.validate()?;
    if rate == 0 {
        return Err(CmError::invalid_field("rate", "must be positive"));
    }
    let levels = frame_levels(samples, rate);
    let thr = if opts.adaptive {
        adaptive_threshold_db(&levels, opts.silence_db)
    } else {
        threshold_db(&levels, opts.silence_db)
    };

    // Runs of silent frames, as (first frame, one past the last).
    let mut runs: Vec<(usize, usize)> = Vec::new();
    let mut start: Option<usize> = None;
    for (i, l) in levels.iter().enumerate() {
        match (*l < thr, start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                runs.push((s, i));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        runs.push((s, levels.len()));
    }

    // Positions in the file (ticks from the track's first sample) where a recording pause joins.
    let mut gaps = gaps.to_vec();
    gaps.sort_by_key(|g| g.from);
    let base = session_to_file(&gaps, track_start).ticks();
    let joins: Vec<i64> = gaps
        .iter()
        .map(|g| session_to_file(&gaps, g.from).ticks() - base)
        .filter(|p| *p > 0)
        .collect();

    let frame_ticks = ticks_of(FRAME_S);
    let min_ticks = ticks_of(f64::from(opts.min_silence_s));
    let keep = ticks_of(f64::from(opts.keep_s));
    let mut out = Vec::new();
    for (a, b) in runs {
        // A silence that starts with the track or runs to its end is the lead-in or tail of the
        // clip, not a pause between speech. On a clip shorter than [`SHORT_CLIP_S`] it would be
        // most of what is suggested, so it is left; on a longer recording one of
        // [`EDGE_SILENCE_MIN_S`] or more is a dead start or end and may be suggested.
        let edge = a == 0 || b == levels.len();
        let run_s = (b - a) as f64 * FRAME_S;
        if edge && (levels.len() as f64 * FRAME_S < SHORT_CLIP_S || run_s < EDGE_SILENCE_MIN_S) {
            continue;
        }
        let (fa, fb) = (a as i64 * frame_ticks, b as i64 * frame_ticks);
        // Split at the joins strictly inside the run.
        let mut cuts_at: Vec<i64> = vec![fa];
        cuts_at.extend(joins.iter().copied().filter(|j| *j > fa && *j < fb));
        cuts_at.push(fb);
        for w in cuts_at.windows(2) {
            let (pa, pb) = (w[0], w[1]);
            let sa = file_to_session(&gaps, pa + base, true);
            let sb = file_to_session(&gaps, pb + base, false);
            let dur = sb - sa;
            if dur < min_ticks {
                continue;
            }
            out.push(CutSuggestion {
                start: MediaTime::from_ticks(sa + keep / 2),
                end: MediaTime::from_ticks(sb - keep / 2),
                kind: "silence".into(),
                detail: format!(
                    "{:.1} s pause",
                    dur as f64 / cm_core::TICKS_PER_SECOND as f64
                ),
            });
        }
    }
    out.sort_by_key(|c| c.start);
    Ok(out)
}

/// Stretches (session time) where the level is `above_db` dB over the track's own floor (10th
/// percentile) for at least `min_s` seconds, with dips of up to 0.3 s bridged. On a noise-reduced
/// copy this is where something speech-like survived the reduction: steady room noise is gone
/// there, so a pause suggested from the raw level that lies over such a stretch is likely speech
/// (a quiet speaker the raw level threshold missed). Used as evidence for [`veto_speech`].
pub fn activity_spans(
    samples: &[f32],
    rate: u32,
    track_start: MediaTime,
    gaps: &[PauseGap],
    above_db: f32,
    min_s: f64,
) -> Vec<Span> {
    let levels = frame_levels(samples, rate);
    if levels.is_empty() {
        return Vec::new();
    }
    let mut sorted = levels.clone();
    sorted.sort_by(f32::total_cmp);
    let thr = sorted[sorted.len() / 10] + above_db;
    let bridge = (0.3 / FRAME_S) as usize;
    let mut runs: Vec<(usize, usize)> = Vec::new();
    for (i, l) in levels.iter().enumerate() {
        if *l <= thr {
            continue;
        }
        match runs.last_mut() {
            Some(last) if i <= last.1 + bridge => last.1 = i + 1,
            _ => runs.push((i, i + 1)),
        }
    }
    let mut gaps = gaps.to_vec();
    gaps.sort_by_key(|g| g.from);
    let base = session_to_file(&gaps, track_start).ticks();
    let frame_ticks = ticks_of(FRAME_S);
    runs.into_iter()
        .filter(|(a, b)| (b - a) as f64 * FRAME_S >= min_s)
        .map(|(a, b)| {
            Span::new(
                MediaTime::from_ticks(file_to_session(&gaps, a as i64 * frame_ticks + base, true)),
                MediaTime::from_ticks(file_to_session(&gaps, b as i64 * frame_ticks + base, false)),
            )
        })
        .collect()
}

/// Removes from the pause suggestions every stretch that other evidence says is speech: `speech` are
/// spans in session time (spoken words from the transcript, speaker turns from the speaker
/// engine). A suggestion that overlaps a span, widened by [`VETO_PAD_S`] on both sides, is cut
/// down to what is left on either side; a left-over is kept when, with the breathing room that
/// stays, it is still a pause of `opts.min_silence_s`. Filler suggestions are untouched.
pub fn veto_speech(
    cuts: Vec<CutSuggestion>,
    speech: &[Span],
    opts: &SilenceOptions,
) -> Vec<CutSuggestion> {
    let pad = ticks_of(VETO_PAD_S);
    let min_ticks = ticks_of(f64::from(opts.min_silence_s));
    let keep = ticks_of(f64::from(opts.keep_s));
    let mut spans: Vec<(i64, i64)> = speech
        .iter()
        .map(|s| (s.from.ticks() - pad, s.to.ticks() + pad))
        .collect();
    spans.sort_unstable();
    let mut out = Vec::new();
    for c in cuts {
        if c.kind != "silence" {
            out.push(c);
            continue;
        }
        let (mut from, to) = (c.start.ticks(), c.end.ticks());
        let mut pieces: Vec<(i64, i64)> = Vec::new();
        for &(a, b) in &spans {
            if b <= from || a >= to {
                continue;
            }
            if a > from {
                pieces.push((from, a));
            }
            from = from.max(b);
            if from >= to {
                break;
            }
        }
        if from < to {
            pieces.push((from, to));
        }
        let whole = pieces.len() == 1 && pieces[0] == (c.start.ticks(), c.end.ticks());
        for (a, b) in pieces {
            if whole {
                out.push(c.clone());
                break;
            }
            if b - a + keep >= min_ticks {
                out.push(CutSuggestion {
                    start: MediaTime::from_ticks(a),
                    end: MediaTime::from_ticks(b),
                    kind: c.kind.clone(),
                    detail: format!(
                        "{:.1} s pause",
                        (b - a + keep) as f64 / cm_core::TICKS_PER_SECOND as f64
                    ),
                });
            }
        }
    }
    out.sort_by_key(|c| c.start);
    out
}

/// How much of the screen changed during a span (0.0 to 1.0). The daemon implements it with
/// decoded screen frames; tests use a scripted one.
pub trait ScreenActivity {
    fn changed_fraction(&self, span: Span) -> f64;
}

/// Drops the pause suggestions during which the screen changed more than
/// [`SCREEN_ACTIVITY_THRESHOLD`] (the user is typing code silently), unless the caller asked to
/// `include_screen_activity`. Filler cuts are never dropped.
pub fn drop_active(
    cuts: Vec<CutSuggestion>,
    activity: &dyn ScreenActivity,
    include_screen_activity: bool,
) -> Vec<CutSuggestion> {
    if include_screen_activity {
        return cuts;
    }
    cuts.into_iter()
        .filter(|c| {
            c.kind != "silence" || activity.changed_fraction(c.span()) <= SCREEN_ACTIVITY_THRESHOLD
        })
        .collect()
}

/// Share of pixels that differ between two RGBA frames of the same size: a pixel differs when any
/// of its R, G or B values differs by more than `tolerance` (compression noise is below it).
/// Frames of different sizes, or empty, count as fully changed or unchanged respectively.
pub fn frame_diff_fraction(a: &[u8], b: &[u8], tolerance: u8) -> f64 {
    if a.len() != b.len() {
        return 1.0;
    }
    let pixels = a.len() / 4;
    if pixels == 0 {
        return 0.0;
    }
    let changed = a
        .as_chunks::<4>()
        .0
        .iter()
        .zip(b.as_chunks::<4>().0.iter())
        .filter(|(p, q)| (0..3).any(|i| p[i].abs_diff(q[i]) > tolerance))
        .count();
    changed as f64 / pixels as f64
}
