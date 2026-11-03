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

mod checks;
mod live;
pub use checks::{
    drop_soft_repeats, fold, repeat_regions, soft_loop_words, Badness, SOFT_LOOP_WORDS,
};
pub use live::{LiveCheckpoint, LiveTranscription};

use cm_core::CmError;
use serde::{Deserialize, Serialize};

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
    let mut out = Vec::new();
    let mut from = 0usize;
    while let Some(cut) = plan_cut(samples, from) {
        out.push(from..cut);
        from = cut;
    }
    out.push(from..total);
    out
}

/// Where the piece that starts at `from` ends, when more than one and a half pieces remain in
/// `samples` (the rule of [`plan`]): the quietest spot within [`SEARCH_SECS`] of the target.
/// `None`: the rest is the last piece. Only the first one and a half pieces' worth of audio after
/// `from` plus the search distance matter, which is why the live path can use it on a prefix.
fn plan_cut(samples: &[f32], from: usize) -> Option<usize> {
    let (piece, search) = (samples_of(PIECE_SECS), samples_of(SEARCH_SECS));
    if samples.len().saturating_sub(from) > piece + piece / 2 {
        let target = from + piece;
        Some(quietest(samples, target - search, target + search))
    } else {
        None
    }
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

/// Where a run over the pieces stood after finishing a piece, so a job that was held (a recording
/// started) or interrupted can go on from there instead of from the first piece (seen on Oct 28,
/// 2026: a transcription of an 88 minute recording at 7x% started over from 0% when the user began
/// a new recording). The words are in file time, as the run returns them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Length of the audio, in samples, and how many pieces it is cut into: a checkpoint of other
    /// audio (the track changed) is not used.
    pub total_samples: usize,
    pub pieces: usize,
    /// Pieces finished, in order.
    pub done: usize,
    /// The words of the finished pieces.
    pub words: Vec<Word>,
    /// The language fixed so far (asked for, or heard in the first piece).
    pub language: Option<String>,
    /// Language, model and note of the first piece.
    pub first: Option<(String, String, Option<String>)>,
    pub split: usize,
    pub repeats_dropped: usize,
    #[serde(default)]
    pub other_audio: usize,
    #[serde(default)]
    pub other_audio_kept: usize,
    #[serde(default)]
    pub broken_reread: usize,
    #[serde(default)]
    pub broken_dropped: usize,
    #[serde(default)]
    pub gaps_reread: usize,
    #[serde(default)]
    pub gaps_filled: usize,
    /// Sound entries of the finished pieces.
    #[serde(default)]
    pub sounds: Vec<crate::transcribe::Sound>,
}

/// What a run over all the pieces did, beyond the words.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PiecesReport {
    /// Pieces the audio was cut into at the start.
    pub pieces: usize,
    /// Pieces that looped and were cut in half and transcribed again.
    pub split: usize,
    /// Words dropped from loops that survived the splitting.
    pub repeats_dropped: usize,
    /// Pieces that were bad (a loop or a soft loop) and read again from the other
    /// audio.
    pub other_audio: usize,
    /// Of those, the ones where the other reading was better and was kept.
    pub other_audio_kept: usize,
    /// Stretches with broken or hopeless words (U+FFFD, a foreign script on low confidence, a run
    /// of near-zero confidence) that were read again on their own, and the ones where the new
    /// reading replaced the old.
    pub broken_reread: usize,
    pub broken_reread_kept: usize,
    /// Words dropped or cleaned because they are not text (see [`crate::failure_kinds`]).
    pub broken_dropped: usize,
    /// Stretches of speech-like level with no words, read again on their own, and the ones that
    /// got words.
    pub gaps_reread: usize,
    pub gaps_filled: usize,
}

/// The same audio from the other source (the original microphone when the noise-reduced copy is
/// read, and the reverse), made only when a piece needs it.
pub type OtherAudio<'p> = Option<&'p mut dyn FnMut() -> Option<Vec<f32>>>;

struct Job<'a, 'p> {
    transcriber: &'a dyn Transcriber,
    samples: &'a [f32],
    other: OtherAudio<'p>,
    /// `None`: not made yet; `Some(None)`: there is none (or it does not fit).
    other_samples: Option<Option<Vec<f32>>>,
    base: cm_core::MediaTime,
    prompt: Option<&'a str>,
    /// The language of every piece after the first: the user's, else what the first piece heard.
    language: Option<String>,
    first: Option<(String, String, Option<String>)>,
    progress: &'p mut dyn FnMut(f32) -> bool,
    high: f32,
    report: PiecesReport,
    sounds: Vec<crate::transcribe::Sound>,
}

/// No language fixed yet: none, or a list to choose from (`vi,en`, see `whisper::LanguageRequest`).
fn is_unsettled(language: Option<&str>) -> bool {
    language.is_none_or(|l| l.contains(','))
}

impl Job<'_, '_> {
    /// Progress over the whole audio, never going back (a piece run again starts from its start).
    fn tick(&mut self, at_sample: usize, within: f32, len: usize) -> bool {
        let total = self.samples.len().max(1) as f32;
        let p = ((at_sample as f32 + within.clamp(0.0, 1.0) * len as f32) / total).min(0.999);
        self.high = self.high.max(p);
        (self.progress)(self.high)
    }

    /// The other audio, made the first time it is asked for. It must be as long as the main audio
    /// (within a second) to be the same timeline; otherwise there is none.
    fn other_audio(&mut self) -> bool {
        if self.other_samples.is_none() {
            let made = self.other.as_mut().and_then(|f| f());
            let fits = made.filter(|o| o.len().abs_diff(self.samples.len()) <= samples_of(1.0));
            self.other_samples = Some(fits);
        }
        matches!(self.other_samples, Some(Some(_)))
    }

    /// One reading of `range`, from the main audio or (`other`) from the other source.
    fn read(&mut self, range: Range<usize>, other: bool) -> Result<Transcript, CmError> {
        let (from, len) = (range.start, range.len());
        let main = self.samples;
        let other_samples = self.other_samples.take();
        let source: &[f32] = match (&other_samples, other) {
            (Some(Some(o)), true) => &o[range.start.min(o.len())..range.end.min(o.len())],
            _ => &main[range],
        };
        let audio = Pcm16k {
            samples: source,
            start: self.base + media_secs(from as f64 / f64::from(SAMPLE_RATE)),
        };
        let language = self.language.clone();
        let (transcriber, prompt) = (self.transcriber, self.prompt);
        let result =
            transcriber.transcribe_prompted(audio, language.as_deref(), prompt, &mut |p| {
                self.tick(from, p, len)
            });
        self.other_samples = other_samples;
        // A cancel arrives as the engine's own `cancelled` error and is passed on as it is.
        let t = result?;
        if self.first.is_none() {
            self.first = Some((t.language.clone(), t.model.clone(), t.note.clone()));
        }
        // Fixed once a piece has words: the first piece's language, which also settles a list of
        // languages to choose from (`vi,en`).
        if is_unsettled(self.language.as_deref()) && !t.words.is_empty() && !t.language.is_empty() {
            self.language = Some(t.language.clone());
        }
        Ok(t)
    }

    /// The words of one piece. A piece that is bad ([`Badness::is_bad`]: an exact loop or a soft
    /// loop) is read again from the other audio and the better reading is kept; one that is still
    /// bad is cut in half and each half is done the same way (up to [`MAX_SPLIT_DEPTH`] levels);
    /// what survives all that has its repeats taken out.
    fn piece(&mut self, range: Range<usize>, depth: usize) -> Result<Vec<Word>, CmError> {
        let words = self.piece_inner(range.clone(), depth)?;
        if depth == 0 {
            let words = self.repair(words, range.clone())?;
            return self.fill_gaps(words, range);
        }
        Ok(words)
    }

    fn piece_inner(&mut self, range: Range<usize>, depth: usize) -> Result<Vec<Word>, CmError> {
        let (from, len) = (range.start, range.len());
        let mut words = self.read(range.clone(), false)?.words;
        let mut bad = Badness::of(&words);
        if !bad.is_bad() {
            return Ok(words);
        }
        if self.other_audio() {
            self.report.other_audio += 1;
            let second = self.read(range, true)?.words;
            let second_bad = Badness::of(&second);
            if second_bad.score() < bad.score() {
                self.report.other_audio_kept += 1;
                words = second;
                bad = second_bad;
            }
        }
        if !bad.is_bad() {
            return Ok(words);
        }
        if depth < MAX_SPLIT_DEPTH && len as f64 > samples_of(MIN_SPLIT_SECS * 2.0) as f64 {
            self.report.split += 1;
            let mid = quietest(self.samples, from + len * 2 / 5, from + len * 3 / 5);
            let mut halves = self.piece_inner(from..mid, depth + 1)?;
            halves.extend(self.piece_inner(mid..from + len, depth + 1)?);
            return Ok(halves);
        }
        let (words, exact) = collapse_repeats(words);
        let (words, soft) = drop_soft_repeats(words);
        self.report.repeats_dropped += exact + soft;
        Ok(words)
    }
}

/// Seconds of audio kept around a suspect stretch when it is read again.
const REPAIR_PAD_SECS: f64 = 1.0;
/// A sentence repeated this many times in a row is a loop suspect (and this many times within
/// [`DENSE_WINDOW_SECS`] with other lines between).
const LOOP_MIN_REPEATS: usize = 3;
const DENSE_MIN_REPEATS: usize = 5;
const DENSE_WINDOW_SECS: f64 = 60.0;
/// A repeat that two readings agree on is taken as said only up to this many times and this many
/// words a second (speech does not go faster; a decoder stuck on a sound does).
const SAID_MAX_REPEATS: usize = 6;
const MAX_WORDS_PER_SEC: f64 = 7.0;
/// Words under this confidence, three or more in a row, make a stretch hopeless.
const HOPELESS_CONFIDENCE: f32 = 0.1;
const HOPELESS_RUN: usize = 3;

impl Job<'_, '_> {
    /// Whether the repeated sentence of `old` was really said: the window read again repeats the
    /// same sentence (folded text) about as often, `old` has at most [`SAID_MAX_REPEATS`] of it,
    /// at no more than [`MAX_WORDS_PER_SEC`].
    fn said_repeatedly(
        &self,
        old: &[Word],
        new_runs: &[crate::failure_kinds::SentenceRun],
    ) -> bool {
        let old_runs = crate::failure_kinds::sentence_runs(old, LOOP_MIN_REPEATS);
        let Some(longest) = old_runs.iter().max_by_key(|r| r.count) else {
            return false;
        };
        let span = old[longest.end - 1].end - old[longest.first].start;
        let secs = (span.ticks() as f64 / cm_core::TICKS_PER_SECOND as f64).max(0.1);
        let rate = (longest.end - longest.first) as f64 / secs;
        longest.count <= SAID_MAX_REPEATS
            && rate <= MAX_WORDS_PER_SEC
            && new_runs
                .iter()
                .any(|n| n.text == longest.text && n.count.abs_diff(longest.count) <= 1)
    }

    fn secs_of(&self, w: &Word) -> (f64, f64) {
        let t = |m: cm_core::MediaTime| {
            (m - self.base).ticks() as f64 / cm_core::TICKS_PER_SECOND as f64
        };
        (t(w.start), t(w.end))
    }

    /// Words that are not text, or a stretch of words nobody is sure of, are read again on their
    /// own (a window around them, no text of the neighbours as prompt); the new reading replaces
    /// the old one when it has nothing broken and is not less certain. What is still broken
    /// afterwards is cleaned: U+FFFD and control characters are removed from a word, and a word
    /// that is nothing but those, or a foreign script on low confidence, is dropped. General
    /// rules on characters and confidence, no word lists.
    fn repair(&mut self, words: Vec<Word>, range: Range<usize>) -> Result<Vec<Word>, CmError> {
        let language = self.language.clone().unwrap_or_default();
        let broken = |w: &Word| crate::failure_kinds::is_broken(w, &language);
        // Suspect words: broken ones, and runs of near-zero confidence.
        let mut suspect = vec![false; words.len()];
        for (i, w) in words.iter().enumerate() {
            suspect[i] |= broken(w);
        }
        let mut i = 0;
        while i < words.len() {
            if words[i].confidence < HOPELESS_CONFIDENCE {
                let mut j = i;
                while j < words.len() && words[j].confidence < HOPELESS_CONFIDENCE {
                    j += 1;
                }
                if j - i >= HOPELESS_RUN {
                    suspect[i..j].iter_mut().for_each(|s| *s = true);
                }
                i = j;
            } else {
                i += 1;
            }
        }
        // Short words that come back again and again, each alone after a long silence: rarely
        // speech, but never dropped unheard: they are read again like the other suspects, and
        // what the second reading says stands (a word really spoken is heard again).
        let mut spread = vec![false; words.len()];
        for i in crate::failure_kinds::spread_repeat_indices(&words, 4, 10.0, 5) {
            spread[i] = true;
            suspect[i] = true;
        }
        // A short sentence said again and again: three or more in a row, or five or more within a
        // minute with other lines in between. Read again on its own (the structure of the text
        // only: no phrase is known to the rule).
        let mut looped = vec![false; words.len()];
        for r in crate::failure_kinds::sentence_runs(&words, LOOP_MIN_REPEATS) {
            looped[r.first..r.end].iter_mut().for_each(|l| *l = true);
        }
        for i in
            crate::failure_kinds::dense_unit_indices(&words, DENSE_MIN_REPEATS, DENSE_WINDOW_SECS)
        {
            looped[i] = true;
        }
        for (s, l) in suspect.iter_mut().zip(&looped) {
            *s |= *l;
        }
        if !suspect.iter().any(|s| *s) {
            return Ok(words);
        }
        // Windows (seconds, in the audio) around runs of suspect words, merged when they touch.
        let piece_secs = (
            range.start as f64 / f64::from(SAMPLE_RATE),
            range.end as f64 / f64::from(SAMPLE_RATE),
        );
        let mut windows: Vec<(f64, f64)> = Vec::new();
        for (w, _) in words.iter().zip(&suspect).filter(|(_, s)| **s) {
            let (a, b) = self.secs_of(w);
            let (a, b) = (
                (a - REPAIR_PAD_SECS).max(piece_secs.0),
                (b + REPAIR_PAD_SECS).min(piece_secs.1),
            );
            match windows.last_mut() {
                Some(last) if a <= last.1 => last.1 = last.1.max(b),
                _ => windows.push((a, b)),
            }
        }
        let mut out: Vec<Word> = Vec::new();
        let mut taken = 0;
        for (a, b) in windows {
            // Words before the window stay.
            while taken < words.len() && self.secs_of(&words[taken]).0 < a {
                out.push(words[taken].clone());
                taken += 1;
            }
            let mut old = Vec::new();
            while taken < words.len() && self.secs_of(&words[taken]).0 < b {
                old.push(words[taken].clone());
                taken += 1;
            }
            let (sa, sb) = (samples_of(a), samples_of(b).min(self.samples.len()));
            if sb <= sa || old.is_empty() {
                out.extend(old);
                continue;
            }
            self.report.broken_reread += 1;
            let saved = self.prompt.take();
            let again = self.read(sa..sb, false);
            self.prompt = saved;
            let again = again?.words;
            let mean = |ws: &[Word]| {
                if ws.is_empty() {
                    0.0
                } else {
                    ws.iter().map(|w| w.confidence).sum::<f32>() / ws.len() as f32
                }
            };
            let clean = again.iter().all(|w| !broken(w));
            let is_looped = |w: &Word| {
                words
                    .iter()
                    .position(|x| x.start == w.start && x.text == w.text)
                    .is_some_and(|i| looped[i])
            };
            let flagged = old.iter().filter(|w| is_looped(w)).count();
            if flagged * 2 >= old.len() {
                // A repeated sentence. If the window read on its own repeats nothing, that reading
                // replaces it (silence included). If it repeats the same sentence at a human pace
                // and about as often, it was said; otherwise there are no reliable words there.
                let new_runs = crate::failure_kinds::sentence_runs(&again, LOOP_MIN_REPEATS);
                if clean && new_runs.is_empty() {
                    self.report.broken_reread_kept += 1;
                    out.extend(again);
                } else if self.said_repeatedly(&old, &new_runs) {
                    out.extend(old);
                } else {
                    let (mut from_s, mut to_s) = (f64::MAX, 0.0f64);
                    for w in old.iter().filter(|w| is_looped(w)) {
                        let (x, y) = self.secs_of(w);
                        from_s = from_s.min(x);
                        to_s = to_s.max(y);
                    }
                    self.report.broken_dropped += flagged;
                    self.note_sound(from_s, to_s.max(from_s));
                    out.extend(old.iter().filter(|w| !is_looped(w)).cloned());
                }
                continue;
            }
            // Words that were only suspected for coming back after silences: whatever a clean
            // second reading of their window says replaces them, silence included.
            let old_spread = old.iter().all(|w| {
                words
                    .iter()
                    .position(|x| x.start == w.start && x.text == w.text)
                    .is_some_and(|i| spread[i])
            });
            if clean && old_spread {
                self.report.broken_reread_kept += 1;
                out.extend(again);
                continue;
            }
            let old_hopeless = old
                .iter()
                .all(|w| broken(w) || w.confidence < HOPELESS_CONFIDENCE);
            if clean
                && ((again.is_empty() && old_hopeless)
                    || (!again.is_empty() && mean(&again) >= mean(&old)))
            {
                self.report.broken_reread_kept += 1;
                out.extend(again);
            } else {
                out.extend(old);
            }
        }
        out.extend(words[taken..].iter().cloned());
        // What is still broken is cleaned or dropped.
        let before = out.len();
        let mut cleaned = 0;
        let out: Vec<Word> = out
            .into_iter()
            .filter_map(|mut w| {
                if crate::failure_kinds::has_stray_chars(&w.text) {
                    w.text = crate::failure_kinds::strip_stray_chars(&w.text);
                    cleaned += 1;
                }
                (!w.text.trim().is_empty() && !broken(&w)).then_some(w)
            })
            .collect();
        self.report.broken_dropped += before - out.len() + cleaned;
        Ok(out)
    }
}

/// A stretch of speech-like level with no word for this long is read again (seconds).
const GAP_MIN_SECS: f64 = 3.0;
/// Words this close to a stretch count as covering it (seconds).
const GAP_PAD_SECS: f64 = 0.5;
/// A speech region whose words fill less than this share of it is read again whole.
const SPARSE_SHARE: f64 = 0.25;
/// At most this many stretches are read again per piece.
const MAX_GAP_READS: usize = 20;
/// A new reading of a stretch is kept when its words average at least this confidence.
const GAP_MIN_CONFIDENCE: f32 = 0.4;

/// The stretches of the region `ra..rb` that no word is within [`GAP_PAD_SECS`] of and that last
/// [`GAP_MIN_SECS`] or more.
fn uncovered(inside: &[&(f64, f64)], ra: f64, rb: f64) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    let mut at = ra;
    for &&(a, b) in inside {
        if a - GAP_PAD_SECS - at >= GAP_MIN_SECS {
            out.push((at, a - GAP_PAD_SECS));
        }
        at = at.max(b + GAP_PAD_SECS);
    }
    if rb - at >= GAP_MIN_SECS {
        out.push((at, rb));
    }
    out
}

impl Job<'_, '_> {
    /// Notes a stretch that still has no reliable words after the second reading: energy and no
    /// words, written as a sound (`[noise]`) rather than as words nobody can vouch for. The
    /// caller upgrades it to `[unclear]` where the speaker engine heard speech
    /// ([`crate::sounds::classify`]).
    fn note_sound(&mut self, a: f64, b: f64) {
        self.sounds.push(crate::transcribe::Sound {
            kind: crate::transcribe::SoundKind::Noise,
            start: self.base + media_secs(a),
            end: self.base + media_secs(b),
        });
    }

    /// Speech that got no words: where the level says someone speaks (see
    /// [`crate::vad::speech_regions`]) for [`GAP_MIN_SECS`] or more with no word near, the stretch
    /// is read again as a piece of its own that starts there (no earlier text as prompt, which is
    /// what lets Whisper skip speech in the middle of a long read). The new words are kept when
    /// they are clean, do not loop and are not hopeless; otherwise the stretch stays empty.
    fn fill_gaps(&mut self, words: Vec<Word>, range: Range<usize>) -> Result<Vec<Word>, CmError> {
        let regions = crate::vad::speech_regions(&self.samples[range.clone()], GAP_MIN_SECS);
        if regions.is_empty() {
            return Ok(words);
        }
        let offset = range.start as f64 / f64::from(SAMPLE_RATE);
        let spans: Vec<(f64, f64)> = words.iter().map(|w| self.secs_of(w)).collect();
        // Stretches to read again, in audio seconds: the gaps between words inside a speech region,
        // and a whole region that holds almost no words for its length (a short word standing for
        // seconds of speech, which a gap rule alone would leave as it is).
        let mut gaps: Vec<(f64, f64)> = Vec::new();
        let mut sparse: Vec<(f64, f64)> = Vec::new();
        for (ra, rb) in regions {
            let (ra, rb) = (
                offset + ra as f64 / f64::from(SAMPLE_RATE),
                offset + rb as f64 / f64::from(SAMPLE_RATE),
            );
            let inside: Vec<&(f64, f64)> =
                spans.iter().filter(|(a, b)| *b > ra && *a < rb).collect();
            let spoken: f64 = inside.iter().map(|(a, b)| b.min(rb) - a.max(ra)).sum();
            if rb - ra >= GAP_MIN_SECS && spoken < SPARSE_SHARE * (rb - ra) {
                sparse.push((ra, rb));
                continue;
            }
            gaps.extend(uncovered(&inside, ra, rb));
        }
        let mut added: Vec<Word> = Vec::new();
        for (a, b) in gaps.into_iter().take(MAX_GAP_READS) {
            let (sa, sb) = (samples_of(a), samples_of(b).min(range.end));
            if sb <= sa {
                continue;
            }
            self.report.gaps_reread += 1;
            let saved = self.prompt.take();
            let again = self.read(sa..sb, false);
            self.prompt = saved;
            let again = again?.words;
            let language = self.language.clone().unwrap_or_default();
            let ok = !again.is_empty()
                && again
                    .iter()
                    .all(|w| !crate::failure_kinds::is_broken(w, &language))
                && !Badness::of(&again).is_bad()
                && again.iter().map(|w| w.confidence).sum::<f32>() / again.len() as f32
                    >= GAP_MIN_CONFIDENCE;
            if ok {
                self.report.gaps_filled += 1;
                added.extend(again);
            } else {
                self.note_sound(a, b);
            }
        }
        // A sparse region is read again whole; its new words replace the old ones inside it when
        // they are clean and say more (more spoken seconds) than what was there.
        let mut words = words;
        for (a, b) in sparse.into_iter().take(MAX_GAP_READS) {
            let (sa, sb) = (
                samples_of((a - GAP_PAD_SECS).max(offset)),
                samples_of(b + GAP_PAD_SECS).min(range.end),
            );
            if sb <= sa {
                continue;
            }
            self.report.gaps_reread += 1;
            let saved = self.prompt.take();
            let again = self.read(sa..sb, false);
            self.prompt = saved;
            let again = again?.words;
            let language = self.language.clone().unwrap_or_default();
            let spoken = |ws: &[Word]| {
                ws.iter()
                    .map(|w| {
                        let (x, y) = self.secs_of(w);
                        y - x
                    })
                    .sum::<f64>()
            };
            let old: Vec<usize> = (0..words.len())
                .filter(|&i| {
                    let (x, _) = self.secs_of(&words[i]);
                    x >= a - GAP_PAD_SECS && x < b
                })
                .collect();
            let old_words: Vec<Word> = old.iter().map(|&i| words[i].clone()).collect();
            let ok = !again.is_empty()
                && again
                    .iter()
                    .all(|w| !crate::failure_kinds::is_broken(w, &language))
                && !Badness::of(&again).is_bad()
                && again.iter().map(|w| w.confidence).sum::<f32>() / again.len() as f32
                    >= GAP_MIN_CONFIDENCE
                && spoken(&again) > spoken(&old_words);
            if ok {
                self.report.gaps_filled += 1;
                let first = old.first().copied();
                let mut keep = vec![true; words.len()];
                old.iter().for_each(|&i| keep[i] = false);
                let mut next: Vec<Word> = words
                    .iter()
                    .zip(&keep)
                    .filter(|(_, k)| **k)
                    .map(|(w, _)| w.clone())
                    .collect();
                let _ = first;
                next.extend(again);
                next.sort_by_key(|w| w.start);
                words = next;
            } else {
                // Not read any better: the words that are there stay, and the rest of the region,
                // where no word is, is written as a sound.
                let inside: Vec<(f64, f64)> = old_words.iter().map(|w| self.secs_of(w)).collect();
                let refs: Vec<&(f64, f64)> = inside.iter().collect();
                for (x, y) in uncovered(&refs, a, b) {
                    self.note_sound(x, y);
                }
            }
        }
        if added.is_empty() {
            return Ok(words);
        }
        let mut all = words;
        all.extend(added);
        all.sort_by_key(|w| w.start);
        Ok(all)
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
    transcribe_pieces_resumable(
        transcriber,
        audio,
        language,
        prompt,
        None,
        &mut |_| {},
        None,
        progress,
    )
}

/// [`transcribe_pieces`] that can go on from a [`Checkpoint`] and hands out a new one after every
/// finished piece (not after the last: the transcript is the result then). A checkpoint that does
/// not fit this audio (another length or another cut) is ignored and the run starts over.
#[allow(clippy::too_many_arguments)]
pub fn transcribe_pieces_resumable<'p>(
    transcriber: &dyn Transcriber,
    audio: Pcm16k<'_>,
    language: Option<&str>,
    prompt: Option<&str>,
    resume: Option<Checkpoint>,
    on_checkpoint: &mut dyn FnMut(&Checkpoint),
    other: OtherAudio<'p>,
    progress: &'p mut dyn FnMut(f32) -> bool,
) -> Result<(Transcript, PiecesReport), CmError> {
    cm_core::perf::begin("transcribe");
    let pieces = {
        let _t = cm_core::perf::span("transcribe", "plan");
        plan(audio.samples)
    };
    let asked = language
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.eq_ignore_ascii_case("auto"))
        .map(str::to_owned);
    let mut job = Job {
        transcriber,
        samples: audio.samples,
        other,
        other_samples: None,
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
        sounds: Vec::new(),
    };
    let mut words = Vec::new();
    let mut start = 0;
    if let Some(c) = resume.filter(|c| {
        c.total_samples == audio.samples.len() && c.pieces == pieces.len() && c.done <= c.pieces
    }) {
        start = c.done;
        words = c.words;
        job.sounds = c.sounds;
        job.first = c.first;
        if is_unsettled(job.language.as_deref()) {
            job.language = c.language;
        }
        job.report.split = c.split;
        job.report.repeats_dropped = c.repeats_dropped;
        job.report.other_audio = c.other_audio;
        job.report.other_audio_kept = c.other_audio_kept;
        job.report.broken_reread = c.broken_reread;
        job.report.broken_dropped = c.broken_dropped;
        job.report.gaps_reread = c.gaps_reread;
        job.report.gaps_filled = c.gaps_filled;
        // The bar starts where the finished pieces end.
        if let Some(next) = pieces.get(start) {
            job.high = next.start as f32 / audio.samples.len().max(1) as f32;
        }
    }
    let total_pieces = pieces.len();
    for (i, range) in pieces.into_iter().enumerate().skip(start) {
        words.extend(job.piece(range, 0)?);
        if i + 1 < total_pieces {
            on_checkpoint(&Checkpoint {
                total_samples: audio.samples.len(),
                pieces: total_pieces,
                done: i + 1,
                words: words.clone(),
                language: job.language.clone(),
                first: job.first.clone(),
                split: job.report.split,
                repeats_dropped: job.report.repeats_dropped,
                other_audio: job.report.other_audio,
                other_audio_kept: job.report.other_audio_kept,
                broken_reread: job.report.broken_reread,
                broken_dropped: job.report.broken_dropped,
                gaps_reread: job.report.gaps_reread,
                gaps_filled: job.report.gaps_filled,
                sounds: job.sounds.clone(),
            });
        }
    }
    // Every piece is done: the bar was held under 1.0 until now.
    let _ = (job.progress)(1.0);
    cm_core::perf::finish("transcribe");
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
            sounds: job.sounds,
            segments: Vec::new(),
        },
        report,
    ))
}

#[cfg(test)]
mod tests;
