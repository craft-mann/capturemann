//! Transcribing long audio piece by piece (AI-1), and noticing when a piece is a repetition loop.
//!
//! Whisper reads audio in 30 second windows and conditions each window on the text of the one
//! before it. On a long recording that lets one bad window poison everything after it: on Oct 28,
//! 2026 an 88 minute Vietnamese recording on Large v3 came back with one phrase repeated 2,480
//! times from 5:35 to the end (96% of the transcript), and the safeguards inside whisper.cpp never
//! fired because the repeated text was decoded with high confidence. The glossary has the same
//! problem the other way round: it is only in the first windows' prompt and is pushed out of the
//! rolling context after a few of them.
//!
//! So a long recording is cut at the quietest point near every [`PIECE_SECS`], and each piece is a
//! run of its own: no loop can pass a piece boundary, and the glossary starts every piece. Every
//! piece is then checked for loops ([`is_degenerate`]). A piece that loops is cut in half at a
//! quiet point and each half is transcribed again (up to [`MAX_SPLIT_DEPTH`] levels); if a piece
//! still loops, its repeats are collapsed to one occurrence ([`collapse_repeats`]) rather than
//! stored by the thousand. Pure of any engine: it only needs a [`Transcriber`].

use std::ops::Range;

use cm_core::CmError;

use crate::audio::SAMPLE_RATE;
use crate::transcribe::{media_secs, Pcm16k, Transcriber, Transcript, Word, TRANSCRIPT_VERSION};

/// The target length of a piece, in seconds.
pub const PIECE_SECS: f64 = 300.0;
/// Audio up to this long is one piece (cutting short audio only costs context).
pub const LONG_AUDIO_SECS: f64 = 600.0;
/// How far from the target a cut may move to find a quiet spot, in seconds.
const SEARCH_SECS: f64 = 20.0;
/// How many times a looping piece may be cut in half again.
pub const MAX_SPLIT_DEPTH: usize = 3;
/// A piece shorter than this is not cut in half again, in seconds.
const MIN_SPLIT_SECS: f64 = 20.0;
/// The energy frame used to find quiet spots (100 ms).
const FRAME: usize = (SAMPLE_RATE as usize) / 10;

fn samples_of(secs: f64) -> usize {
    (secs * f64::from(SAMPLE_RATE)).round() as usize
}

/// The start of the quietest 100 ms of `samples[lo..hi]` (smoothed over three frames), its centre.
fn quietest(samples: &[f32], lo: usize, hi: usize) -> usize {
    let hi = hi.min(samples.len());
    let lo = lo.min(hi);
    let frames: Vec<f64> = samples[lo..hi]
        .chunks(FRAME)
        .filter(|c| c.len() == FRAME)
        .map(|c| c.iter().map(|s| f64::from(*s) * f64::from(*s)).sum::<f64>() / FRAME as f64)
        .collect();
    if frames.len() < 3 {
        return lo + (hi - lo) / 2;
    }
    let mut best = (f64::MAX, 1usize);
    for i in 1..frames.len() - 1 {
        let e = (frames[i - 1] + frames[i] + frames[i + 1]) / 3.0;
        if e < best.0 {
            best = (e, i);
        }
    }
    lo + best.1 * FRAME + FRAME / 2
}

/// Where a recording is cut: the pieces, in order, covering the whole audio without a gap or an
/// overlap. Audio up to [`LONG_AUDIO_SECS`] is a single piece; longer audio is cut at the quietest
/// spot within [`SEARCH_SECS`] of every [`PIECE_SECS`], so every piece is between half and one and
/// a half piece long.
pub fn plan(samples: &[f32]) -> Vec<Range<usize>> {
    let total = samples.len();
    if total <= samples_of(LONG_AUDIO_SECS) {
        #[allow(clippy::single_range_in_vec_init)]
        return vec![0..total];
    }
    let (piece, search) = (samples_of(PIECE_SECS), samples_of(SEARCH_SECS));
    let mut out = Vec::new();
    let mut from = 0usize;
    while total - from > piece + piece / 2 {
        let target = from + piece;
        let cut = quietest(samples, target - search, target + search);
        out.push(from..cut);
        from = cut;
    }
    out.push(from..total);
    out
}

/// A word as it is compared for repetition: lower case, letters and digits only.
fn token(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// A stretch of words that is one group of words said again and again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Run {
    /// Index of the first word.
    pub start: usize,
    /// Words in the repeated group.
    pub unit: usize,
    /// How many times it is said in a row.
    pub count: usize,
}

impl Run {
    /// Words the run covers.
    pub fn len(&self) -> usize {
        self.unit * self.count
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

const MAX_UNIT: usize = 8;

/// How many repeats in a row make a run: a single word has to be said 8 times (people do say
/// "yeah yeah yeah"), a group of words 4 times, and a run has to cover at least 8 words.
fn is_run(unit: usize, count: usize) -> bool {
    count >= if unit == 1 { 8 } else { 4 } && unit * count >= 8
}

/// The runs of repeated word groups in `words` (a group of 1 to 8 words said four or more times
/// in a row; a single word eight or more times).
pub fn find_runs(words: &[Word]) -> Vec<Run> {
    let tokens: Vec<String> = words.iter().map(|w| token(&w.text)).collect();
    let n = tokens.len();
    let mut out = Vec::new();
    let mut i = 0;
    while i < n {
        let mut best: Option<Run> = None;
        for unit in 1..=MAX_UNIT {
            if i + unit * 2 > n {
                break;
            }
            let mut count = 1;
            while i + (count + 1) * unit <= n
                && tokens[i + count * unit..i + (count + 1) * unit] == tokens[i..i + unit]
            {
                count += 1;
            }
            if is_run(unit, count) && best.is_none_or(|b| unit * count > b.len()) {
                best = Some(Run {
                    start: i,
                    unit,
                    count,
                });
            }
        }
        match best {
            Some(r) => {
                i += r.len();
                out.push(r);
            }
            None => i += 1,
        }
    }
    out
}

/// Whether a piece's words are mostly a repetition loop: a quarter or more of its words (at least
/// 20 words in all) sit in runs, or one run alone is 40 words or more. A few repeated phrases in
/// real speech never come near this.
pub fn is_degenerate(words: &[Word]) -> bool {
    let runs = find_runs(words);
    let in_runs: usize = runs.iter().map(Run::len).sum();
    let longest = runs.iter().map(Run::len).max().unwrap_or(0);
    (words.len() >= 20 && in_runs * 4 >= words.len()) || longest >= 40
}

/// `words` with every run kept once: the first occurrence stays, the rest are dropped. Returns the
/// words and how many were dropped.
pub fn collapse_repeats(words: Vec<Word>) -> (Vec<Word>, usize) {
    let runs = find_runs(&words);
    if runs.is_empty() {
        return (words, 0);
    }
    let mut drop = vec![false; words.len()];
    let mut dropped = 0;
    for r in &runs {
        for flag in &mut drop[r.start + r.unit..r.start + r.len()] {
            *flag = true;
            dropped += 1;
        }
    }
    let kept = words
        .into_iter()
        .zip(drop)
        .filter_map(|(w, d)| (!d).then_some(w))
        .collect();
    (kept, dropped)
}

/// What a run over all the pieces did, beyond the words.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PiecesReport {
    /// Pieces the audio was cut into at the start.
    pub pieces: usize,
    /// Pieces that looped and were cut in half and transcribed again.
    pub split: usize,
    /// Words dropped from loops that survived the splitting.
    pub repeats_dropped: usize,
}

struct Job<'a, 'p> {
    transcriber: &'a dyn Transcriber,
    samples: &'a [f32],
    base: cm_core::MediaTime,
    prompt: Option<&'a str>,
    /// The language of every piece after the first: the user's, else what the first piece heard.
    language: Option<String>,
    first: Option<(String, String, Option<String>)>,
    progress: &'p mut dyn FnMut(f32) -> bool,
    high: f32,
    report: PiecesReport,
}

impl Job<'_, '_> {
    /// Progress over the whole audio, never going back (a piece run again starts from its start).
    fn tick(&mut self, at_sample: usize, within: f32, len: usize) -> bool {
        let total = self.samples.len().max(1) as f32;
        let p = ((at_sample as f32 + within.clamp(0.0, 1.0) * len as f32) / total).min(0.999);
        self.high = self.high.max(p);
        (self.progress)(self.high)
    }

    /// The words of one piece, split in half and done again while it loops.
    fn piece(&mut self, range: Range<usize>, depth: usize) -> Result<Vec<Word>, CmError> {
        let (from, len) = (range.start, range.len());
        let samples = self.samples;
        let audio = Pcm16k {
            samples: &samples[range],
            start: self.base + media_secs(from as f64 / f64::from(SAMPLE_RATE)),
        };
        let language = self.language.clone();
        let (transcriber, prompt) = (self.transcriber, self.prompt);
        let result =
            transcriber.transcribe_prompted(audio, language.as_deref(), prompt, &mut |p| {
                self.tick(from, p, len)
            });
        // A cancel arrives as the engine's own `cancelled` error and is passed on as it is.
        let t = result?;
        if self.first.is_none() {
            self.first = Some((t.language.clone(), t.model.clone(), t.note.clone()));
        }
        if self.language.is_none() && !t.words.is_empty() && !t.language.is_empty() {
            self.language = Some(t.language.clone());
        }
        if !is_degenerate(&t.words) {
            return Ok(t.words);
        }
        if depth < MAX_SPLIT_DEPTH && len as f64 > samples_of(MIN_SPLIT_SECS * 2.0) as f64 {
            self.report.split += 1;
            let mid = quietest(self.samples, from + len * 2 / 5, from + len * 3 / 5);
            let mut words = self.piece(from..mid, depth + 1)?;
            words.extend(self.piece(mid..from + len, depth + 1)?);
            return Ok(words);
        }
        let (words, dropped) = collapse_repeats(t.words);
        self.report.repeats_dropped += dropped;
        Ok(words)
    }
}

/// Transcribes `audio` piece by piece ([`plan`]), checking every piece for a repetition loop. The
/// words come back in file time like a single run's would (`audio.start` plus the position), the
/// language is the first piece's unless one was asked for, and `progress` runs over the whole audio
/// and never goes back. Audio up to [`LONG_AUDIO_SECS`] is one piece, so a short recording is
/// transcribed as before (and is still checked for a loop).
pub fn transcribe_pieces(
    transcriber: &dyn Transcriber,
    audio: Pcm16k<'_>,
    language: Option<&str>,
    prompt: Option<&str>,
    progress: &mut dyn FnMut(f32) -> bool,
) -> Result<(Transcript, PiecesReport), CmError> {
    let pieces = plan(audio.samples);
    let asked = language
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.eq_ignore_ascii_case("auto"))
        .map(str::to_owned);
    let mut job = Job {
        transcriber,
        samples: audio.samples,
        base: audio.start,
        prompt,
        language: asked,
        first: None,
        progress,
        high: 0.0,
        report: PiecesReport {
            pieces: pieces.len(),
            ..PiecesReport::default()
        },
    };
    let mut words = Vec::new();
    for range in pieces {
        words.extend(job.piece(range, 0)?);
    }
    // Every piece is done: the bar was held under 1.0 until now.
    let _ = (job.progress)(1.0);
    let mut report = job.report;
    // A loop can also run across piece boundaries (every piece repeating the same words): if the
    // whole is still mostly a loop, keep each run once.
    if is_degenerate(&words) {
        let (kept, dropped) = collapse_repeats(words);
        words = kept;
        report.repeats_dropped += dropped;
    }
    let (language, model, note) = job.first.unwrap_or_default();
    let note = if words.is_empty() { note } else { None };
    Ok((
        Transcript {
            version: TRANSCRIPT_VERSION,
            language: job.language.unwrap_or(language),
            model,
            words,
            note,
        },
        report,
    ))
}

#[cfg(test)]
mod tests;
