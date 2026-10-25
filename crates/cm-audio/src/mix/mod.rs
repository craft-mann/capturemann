//! Audio post-processing for editing and export (AUD-4, EDT-2): per-track gain and mute, the edit
//! timeline applied to audio (cuts skipped, speed changes time-stretched with the pitch kept,
//! muted above 2x), the mix on the session clock, EBU R128 loudness normalisation and a -1 dBTP
//! limiter. Pure and headless: samples in, samples out.
//!
//! The export pipeline calls [`mix_tracks`] with the tracks, `manifest.gaps` and the
//! [`Timeline`] of the edit list; the result is on the *output* timeline, ready to encode.
//! Pauses are not part of the output (REC-4): build the timeline from
//! `EditList::without_pauses(gaps)`, as the export does. A timeline that still contains a pause
//! gets silence there, so audio and video would drift against a picture that skips it.
//!
//! # Time
//!
//! A track's file starts at its first sample (`TrackInfo.start`, session time) and is compacted
//! at pauses (REC-4). Sample positions therefore come from [`cm_core::edit::session_to_file`]
//! of the session time minus that of the track start, never from "file time 0 is session 0".
//! A piece that starts before the track's first sample gets silence there.
//!
//! Whole tracks are held in memory (about 11 MB per minute of 48 kHz stereo f32).

mod limiter;
mod loudness;
mod stretch;

use cm_core::edit::{session_to_file, Timeline};
use cm_core::{CmError, MediaTime, PauseGap, TICKS_PER_SECOND};

pub use limiter::limit_true_peak;
pub use loudness::{integrated_lufs, true_peak_db, SILENCE_DB};
pub use stretch::stretch;

/// Gain range of the editor (AUD-4: -inf to +12 dB; -inf is `mute`).
pub const GAIN_DB_RANGE: (f64, f64) = cm_core::edit::AUDIO_GAIN_DB_RANGE;
/// Platform presets normalise to this (AUD-4).
pub const PLATFORM_LUFS: f64 = -14.0;
/// True-peak ceiling of the limiter (AUD-4).
pub const TRUE_PEAK_CEILING_DBTP: f64 = -1.0;

/// One audio file as read from the bundle: interleaved f32 samples.
#[derive(Debug, Clone, Copy)]
pub struct TrackAudio<'a> {
    pub samples: &'a [f32],
    pub channels: u16,
    pub sample_rate: u32,
    /// `TrackInfo.start`: session time of the file's first sample.
    pub start: MediaTime,
    pub gain_db: f64,
    pub mute: bool,
}

/// What the mix is rendered to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MixParams {
    pub sample_rate: u32,
    /// 1 or 2. Mono sources are duplicated to stereo, stereo sources averaged to mono.
    pub out_channels: u16,
    /// Integrated loudness target (`-14` for platform presets); `None` leaves the level alone
    /// (the "Source" preset). The limiter always runs.
    pub loudness_lufs: Option<f64>,
    pub true_peak_dbtp: f64,
}

impl Default for MixParams {
    fn default() -> Self {
        MixParams {
            sample_rate: crate::TRACK_RATE,
            out_channels: 2,
            loudness_lufs: None,
            true_peak_dbtp: TRUE_PEAK_CEILING_DBTP,
        }
    }
}

/// The finished mix on the output timeline.
#[derive(Debug, Clone, PartialEq)]
pub struct MixedAudio {
    /// Interleaved samples, `out_channels` per frame.
    pub samples: Vec<f32>,
    pub channels: u16,
    pub sample_rate: u32,
    /// Gain added to reach the loudness target, in dB (0 when not normalising).
    pub normalise_gain_db: f64,
    /// Measured after normalising and limiting; `None` for silence.
    pub integrated_lufs: Option<f64>,
    pub true_peak_dbtp: f64,
}

/// Linear gain of a level in dB; at or below the range floor is silence.
pub fn db_to_gain(db: f64) -> f32 {
    if db <= GAIN_DB_RANGE.0 {
        0.0
    } else {
        10f64.powf(db / 20.0) as f32
    }
}

fn frames_at(t_ticks: i64, rate: u32) -> i64 {
    (t_ticks as f64 * f64::from(rate) / TICKS_PER_SECOND as f64).round() as i64
}

fn convert_channels(x: &[f32], from: usize, to: usize) -> Vec<f32> {
    match (from, to) {
        (a, b) if a == b => x.to_vec(),
        (1, 2) => x.iter().flat_map(|v| [*v, *v]).collect(),
        (2, 1) => x.chunks(2).map(|f| (f[0] + f[1]) * 0.5).collect(),
        _ => Vec::new(),
    }
}

/// `frames` frames of `x` from signed frame position `at`, zeros outside the file.
fn extract(x: &[f32], channels: usize, at: i64, frames: usize) -> Vec<f32> {
    let total = (x.len() / channels) as i64;
    let mut out = vec![0f32; frames * channels];
    let from = at.max(0);
    let to = (at + frames as i64).min(total);
    if to > from {
        let dst = ((from - at) as usize) * channels;
        let src = &x[from as usize * channels..to as usize * channels];
        out[dst..dst + src.len()].copy_from_slice(src);
    }
    out
}

fn fade(buf: &mut [f32], channels: usize, rate: u32, fade_in: bool, fade_out: bool) {
    let frames = buf.len() / channels;
    let n = ((f64::from(rate) * 0.002) as usize).min(frames / 2);
    for i in 0..n {
        let g = i as f32 / n as f32;
        for c in 0..channels {
            if fade_in {
                buf[i * channels + c] *= g;
            }
            if fade_out {
                buf[(frames - 1 - i) * channels + c] *= g;
            }
        }
    }
}

fn check_track(t: &TrackAudio<'_>, p: &MixParams) -> Result<(), CmError> {
    if !(1..=2).contains(&p.out_channels) {
        return Err(CmError::invalid_input("out_channels must be 1 or 2"));
    }
    if !(1..=2).contains(&t.channels) {
        return Err(CmError::invalid_input(format!(
            "an audio track has {} channels; only mono and stereo are supported",
            t.channels
        )));
    }
    if t.sample_rate != p.sample_rate {
        return Err(CmError::invalid_input(format!(
            "an audio track is {} Hz but the mix is {} Hz",
            t.sample_rate, p.sample_rate
        )));
    }
    if !t.gain_db.is_finite() || t.gain_db > GAIN_DB_RANGE.1 {
        return Err(CmError::invalid_input(format!(
            "gain must be at most {} dB",
            GAIN_DB_RANGE.1
        )));
    }
    Ok(())
}

/// `from..to` (session time) of a track on the output: read through the pause gaps, stretched
/// to `out_frames`.
fn render_range(
    t: &TrackAudio<'_>,
    gaps: &[PauseGap],
    (from, to): (MediaTime, MediaTime),
    out_frames: usize,
    out_channels: usize,
) -> Vec<f32> {
    let channels = usize::from(t.channels);
    let origin = session_to_file(gaps, t.start).ticks();
    let a = frames_at(session_to_file(gaps, from).ticks() - origin, t.sample_rate);
    let b = frames_at(session_to_file(gaps, to).ticks() - origin, t.sample_rate);
    let in_frames = (b - a).max(0) as usize;
    if in_frames == 0 {
        return vec![0.0; out_frames * out_channels];
    }
    let src = convert_channels(
        &extract(t.samples, channels, a, in_frames),
        channels,
        out_channels,
    );
    stretch(
        &src,
        out_channels,
        in_frames as f64 / out_frames.max(1) as f64,
        out_frames,
    )
}

/// The parts of `from..to` outside the pause gaps. A pause has no samples in any media file, so
/// it stays silent on the output while the timeline keeps its length.
fn outside_gaps(gaps: &[PauseGap], from: MediaTime, to: MediaTime) -> Vec<(MediaTime, MediaTime)> {
    let mut parts = vec![(from, to)];
    for g in gaps {
        parts = parts
            .into_iter()
            .flat_map(|(a, b)| {
                if g.to <= a || g.from >= b {
                    vec![(a, b)]
                } else {
                    let mut v = Vec::new();
                    if g.from > a {
                        v.push((a, g.from));
                    }
                    if g.to < b {
                        v.push((g.to, b));
                    }
                    v
                }
            })
            .collect();
    }
    parts
}

/// Renders one track onto the output timeline: pieces in order, cuts skipped, each at its speed
/// (muted where the timeline says so or `mute` is set), `gain_db` applied, `out_channels` wide.
/// The result is as long as the timeline and silent where the track has no samples.
pub fn render_track(
    track: &TrackAudio<'_>,
    gaps: &[PauseGap],
    timeline: &Timeline,
    params: &MixParams,
) -> Result<Vec<f32>, CmError> {
    check_track(track, params)?;
    let out_channels = usize::from(params.out_channels);
    let rate = params.sample_rate;
    let total = frames_at(timeline.duration().ticks(), rate).max(0) as usize;
    let mut out = vec![0f32; total * out_channels];
    let gain = db_to_gain(track.gain_db);
    if track.mute || gain == 0.0 {
        return Ok(out);
    }
    let pieces = timeline.pieces();
    for (i, piece) in pieces.iter().enumerate() {
        if !piece.audio {
            continue;
        }
        // A jump in the source (a cut, a speed change) gets a short fade so it does not click.
        let joins_prev = i > 0
            && pieces[i - 1].audio
            && pieces[i - 1].source.to == piece.source.from
            && (pieces[i - 1].factor - piece.factor).abs() < 1e-9;
        let joins_next = pieces.get(i + 1).is_some_and(|n| {
            n.audio && n.source.from == piece.source.to && (n.factor - piece.factor).abs() < 1e-9
        });
        let parts = outside_gaps(gaps, piece.source.from, piece.source.to);
        for (a, b) in &parts {
            let at = |t: MediaTime| {
                piece.out_start
                    + MediaTime::from_ticks(
                        ((t - piece.source.from).ticks() as f64 / piece.factor).round() as i64,
                    )
            };
            let n0 = frames_at(at(*a).ticks(), rate).max(0) as usize;
            let n1 = (frames_at(at(*b).ticks(), rate).max(0) as usize).min(total);
            if n1 <= n0 {
                continue;
            }
            let mut buf = render_range(track, gaps, (*a, *b), n1 - n0, out_channels);
            let whole = parts.len() == 1;
            fade(
                &mut buf,
                out_channels,
                rate,
                !(whole && joins_prev),
                !(whole && joins_next),
            );
            for (o, v) in out[n0 * out_channels..n1 * out_channels]
                .iter_mut()
                .zip(&buf)
            {
                *o = v * gain;
            }
        }
    }
    Ok(out)
}

/// Mixes `tracks` on the output timeline, then normalises to `params.loudness_lufs` (if set) and
/// limits to `params.true_peak_dbtp`. Tracks shorter than the video are padded with silence.
pub fn mix_tracks(
    tracks: &[TrackAudio<'_>],
    gaps: &[PauseGap],
    timeline: &Timeline,
    params: &MixParams,
) -> Result<MixedAudio, CmError> {
    let channels = usize::from(params.out_channels);
    let total = frames_at(timeline.duration().ticks(), params.sample_rate).max(0) as usize;
    let mut mix = vec![0f32; total * channels];
    for t in tracks {
        let rendered = render_track(t, gaps, timeline, params)?;
        for (m, v) in mix.iter_mut().zip(&rendered) {
            *m += v;
        }
    }
    let rate = params.sample_rate;
    let mut applied = 0.0;
    if let (Some(target), Some(now)) = (params.loudness_lufs, integrated_lufs(&mix, channels, rate))
    {
        // Limiting lowers the level a little, so close the gap in up to three rounds.
        let mut current = now;
        for _ in 0..3 {
            let step = target - current;
            if step.abs() < 0.1 {
                break;
            }
            let g = 10f64.powf(step / 20.0) as f32;
            mix.iter_mut().for_each(|v| *v *= g);
            applied += step;
            limit_true_peak(&mut mix, channels, rate, params.true_peak_dbtp);
            match integrated_lufs(&mix, channels, rate) {
                Some(l) => current = l,
                None => break,
            }
        }
    }
    limit_true_peak(&mut mix, channels, rate, params.true_peak_dbtp);
    Ok(MixedAudio {
        integrated_lufs: integrated_lufs(&mix, channels, rate),
        true_peak_dbtp: true_peak_db(&mix, channels),
        samples: mix,
        channels: params.out_channels,
        sample_rate: rate,
        normalise_gain_db: applied,
    })
}

#[cfg(test)]
mod tests;
