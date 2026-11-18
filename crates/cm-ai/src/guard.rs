//! A temporary guard against text nobody said (AI-1.AC12 to AC14).
//!
//! Whisper was trained on video with sign-offs and promotions, and over quiet stretches it
//! sometimes writes one ("subscribe to the channel ...", "thanks for watching ..."): the words
//! come back with confidence near 1.0 because the decoder is sure of text it learnt, not of sound
//! it heard. There is no model fix yet, so two guards drop such lines after the transcription,
//! and each dropped line is counted and kept with its time range ([`DroppedLine`], stored in
//! `transcript.json`) so nothing disappears silently.
//!
//! * [`drop_unsupported`] is a SHAPE check, with no text and no phrases in it. A line (a run of
//!   words that ends at a sentence mark or at a pause of [`LINE_GAP_SECS`]) is unsupported when
//!   the audio under it is no louder than the room (the 80th percentile of its 20 ms levels is
//!   within a margin of the quiet end of the whole recording) and either the line is long enough
//!   to be a sentence while the room is the only sound, or the decoder is certain of every word
//!   ([`SURE`]) although there is nothing but room to hear. Real speech, even quiet, rises above
//!   the room, and a line of several words rarely has no word under 0.97. Needs the audio and a
//!   recording where speech and room can be told apart ([`Reference::new`]); otherwise it keeps
//!   everything.
//! * [`drop_doubtful`] (0.3.6, AI-1.AC12b) writes a line as an `[unclear]` sound instead when most of
//!   its words are low in confidence ([`LOW_CONFIDENCE`]) over audio that is not speech-like (under a
//!   quarter of its frames 10 dB over the room), and works on recordings under 30 s too.
//! * [`drop_ignored`] drops the lines that contain a phrase of the user's own list
//!   (`ai.ignore_phrases`, empty by default), compared with [`fold`] (case, accents, punctuation).
//!
//! Measured on the maintainer's library on Nov 10, 2026: `verification/benchmarks/outro-guard-2026-11-10.md`.

use std::collections::HashMap;
use std::ops::Range;

use cm_core::MediaTime;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::audio::SAMPLE_RATE;
use crate::pieces::fold;
use crate::transcribe::Word;

/// Setting: a JSON array of phrases (app-only, like the glossary).
pub const SETTING_IGNORE_PHRASES: &str = "ai.ignore_phrases";
/// Most phrases, and the longest one (characters). The settings store refuses values over 16 KiB.
pub const MAX_PHRASES: usize = 50;
pub const MAX_PHRASE_CHARS: usize = 100;
/// A phrase needs at least this many letters and digits after folding, so that a stray letter
/// cannot empty a transcript.
pub const MIN_PHRASE_CHARS: usize = 3;

/// A pause this long (seconds), or a sentence mark, ends a line.
pub const LINE_GAP_SECS: f64 = 1.0;
/// A recording needs this many seconds of frames that are not digital silence to have a reference
/// level (AI-1.AC12): the room is the 10th percentile and the speech the 90th, so each end needs
/// a few hundred frames. Measured in `verification/benchmarks/short-audio-guard-2026-11-18.md`.
const MIN_AUDIO_SECS: f64 = 4.0;
/// Audio shorter than this (seconds) must show a wider range between speech and room, because
/// its percentiles rest on fewer frames.
const SHORT_AUDIO_SECS: f64 = 30.0;
/// Speech and room must be at least this far apart (dB, 90th against 10th percentile).
const MIN_RANGE_DB: f32 = 10.0;
/// The same for audio under [`SHORT_AUDIO_SECS`].
const MIN_RANGE_SHORT_DB: f32 = 15.0;
/// A line shorter than this (seconds) is never judged.
const MIN_LINE_SECS: f64 = 1.0;
/// Level margin (dB over the room) under which five words or more are unsupported.
const ROOM_MARGIN_DB: f32 = 2.0;
/// Words in a line judged by the room margin alone.
const ROOM_MIN_WORDS: usize = 5;
/// Level margin (dB over the room) under which a line the decoder is sure of is unsupported.
const SURE_MARGIN_DB: f32 = 9.0;
/// Every word at least this confident.
pub const SURE: f32 = 0.97;
const SURE_MIN_WORDS: usize = 3;
/// 20 ms frames.
const FRAME: usize = (SAMPLE_RATE as usize) / 50;

/// Languages the shape check runs on for now (ISO 639-1 codes of the transcript). The invented
/// sign-offs it was built for were found in Vietnamese transcripts; on the English files of the
/// measurement it made one drop, a real sentence spoken softly (maintainer's listening, Nov 10,
/// 2026), so until the shape check has a second opinion (a speech detector) it stays off there.
pub const LANGUAGES: [&str; 1] = ["vi"];

/// Whether the shape check runs on a transcript in `language`.
pub fn applies_to(language: &str) -> bool {
    LANGUAGES.iter().any(|l| language.eq_ignore_ascii_case(l))
}

/// Why a line was dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DropReason {
    /// The shape check: text over audio that holds no speech.
    Unsupported,
    /// The line contains a phrase of the user's ignore list.
    Ignored,
    /// Low-confidence words over audio that is not speech-like: written as an `[unclear]` sound
    /// instead ([`drop_doubtful`]).
    Doubtful,
}

/// A line left out of a transcript, kept so the drop can be seen (and the line re-created by a
/// re-generate with the guard off).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DroppedLine {
    pub start: MediaTime,
    pub end: MediaTime,
    pub words: u32,
    pub reason: DropReason,
}

/// The lines of `words`: a run ends after a sentence mark or before a pause of [`LINE_GAP_SECS`].
pub fn lines(words: &[Word]) -> Vec<Range<usize>> {
    let gap = (LINE_GAP_SECS * cm_core::TICKS_PER_SECOND as f64) as i64;
    let mut out = Vec::new();
    let mut from = 0;
    for i in 0..words.len() {
        let mark = words[i]
            .text
            .trim_end()
            .chars()
            .last()
            .is_some_and(|c| matches!(c, '.' | '?' | '!' | '…'));
        let pause = words
            .get(i + 1)
            .is_some_and(|n| (n.start - words[i].end).ticks() >= gap);
        if mark || pause || i + 1 == words.len() {
            out.push(from..i + 1);
            from = i + 1;
        }
    }
    out
}

/// The levels of a recording (20 ms frames, dB) and the two ends that matter: the room and
/// the speech.
pub struct Reference {
    db: Vec<f32>,
    floor: f32,
}

impl Reference {
    /// `None` when the audio has too few frames that are not silent, or speech and room are less than [`MIN_RANGE_DB`] apart.
    pub fn new(samples: &[f32]) -> Option<Reference> {
        let db: Vec<f32> = samples
            .as_chunks::<FRAME>()
            .0
            .iter()
            .map(|c| 10.0 * (c.iter().map(|s| s * s).sum::<f32>() / FRAME as f32 + 1e-12).log10())
            .collect();
        // The room is estimated from frames that are not digital silence: muted time is exact
        // zeros (-120 dB) and would pull the 10th percentile far under any real room, so every
        // line would look 100 dB above it (BUG-10, AI-1.AC15). With too few other frames there is
        // no reference, as for short audio.
        let mut sorted: Vec<f32> = samples
            .as_chunks::<FRAME>()
            .0
            .iter()
            .zip(&db)
            .filter(|(c, _)| !crate::audio::is_digital_silence(&c[..]))
            .map(|(_, d)| *d)
            .collect();
        let secs = sorted.len() as f64 * 0.02;
        if secs < MIN_AUDIO_SECS {
            return None;
        }
        sorted.sort_by(f32::total_cmp);
        let floor = sorted[sorted.len() / 10];
        let speech = sorted[sorted.len() * 9 / 10];
        let wanted = if (samples.len() as f64) < SHORT_AUDIO_SECS * f64::from(SAMPLE_RATE) {
            MIN_RANGE_SHORT_DB
        } else {
            MIN_RANGE_DB
        };
        (speech - floor >= wanted).then_some(Reference { db, floor })
    }

    /// How far (dB) the 80th percentile level between `a` and `b` seconds is over the room.
    pub fn margin(&self, a: f64, b: f64) -> Option<f32> {
        let fa = (a.max(0.0) * 50.0) as usize;
        let fb = ((b * 50.0) as usize).max(fa + 1).min(self.db.len());
        if fa >= fb {
            return None;
        }
        let mut seg = self.db[fa..fb].to_vec();
        seg.sort_by(f32::total_cmp);
        Some(seg[seg.len() * 8 / 10] - self.floor)
    }
}

/// A word is low in confidence below this.
pub const LOW_CONFIDENCE: f32 = 0.5;
/// A line is doubtful when at least this share of its words is low in confidence.
const LOW_SHARE: f32 = 0.5;
/// A frame is speech-like when it is this many dB over the room.
const SPEECH_OVER_DB: f32 = 10.0;
/// A line is not speech-like when fewer than this share of its frames are speech-like.
const SPEECH_SHARE_UNDER: f32 = 0.25;
/// A line is judged only when its words cover at least this many seconds of audio (capped per
/// word, see [`MAX_WORD_SECS`]): under that the share of 20 ms frames rests on a handful of them.
const MIN_VOICED_SECS: f64 = 0.25;
/// One word counts for at most this many seconds when the line's frames are read, so a last
/// word stretched over a pause (its piece's end) does not make real speech look like room.
const MAX_WORD_SECS: f64 = 1.0;

impl Reference {
    /// The share of the frames of `a` to `b` seconds that are [`SPEECH_OVER_DB`] over the room.
    pub fn speech_share(&self, a: f64, b: f64) -> Option<f32> {
        let fa = (a.max(0.0) * 50.0) as usize;
        let fb = ((b * 50.0) as usize).max(fa + 1).min(self.db.len());
        if fa >= fb {
            return None;
        }
        let n = self.db[fa..fb]
            .iter()
            .filter(|d| **d - self.floor >= SPEECH_OVER_DB)
            .count();
        Some(n as f32 / (fb - fa) as f32)
    }
}

/// Whether the line `words` is mostly low in confidence over audio that is not speech-like
/// (AI-1.AC12). `words` start at `base` in the audio.
pub fn is_doubtful(words: &[Word], base: MediaTime, reference: &Reference) -> bool {
    if words.is_empty() {
        return false;
    }
    let low = words
        .iter()
        .filter(|w| w.confidence < LOW_CONFIDENCE)
        .count();
    if (low as f32) < LOW_SHARE * words.len() as f32 {
        return false;
    }
    // The frames under the words, each word read for at most MAX_WORD_SECS.
    let (mut speechy, mut total) = (0.0f64, 0.0f64);
    for w in words {
        let a = (w.start - base).as_secs_f64();
        let b = (w.end - base).as_secs_f64().min(a + MAX_WORD_SECS);
        if let Some(share) = reference.speech_share(a, b.max(a + 0.02)) {
            let len = (b - a).max(0.02);
            speechy += f64::from(share) * len;
            total += len;
        }
    }
    total >= MIN_VOICED_SECS && (speechy / total) < f64::from(SPEECH_SHARE_UNDER)
}

/// The doubtful lines of `words` as `[unclear]` sounds (AI-1.AC12): their words are taken out of
/// the transcript, and each line is kept in the dropped list with [`DropReason::Doubtful`]. Needs a
/// [`Reference`]; without one everything stays. `sounds` are in the same time as `words`.
pub fn drop_doubtful(
    words: Vec<Word>,
    samples: &[f32],
    base: MediaTime,
) -> (Vec<Word>, Vec<crate::transcribe::Sound>, Vec<DroppedLine>) {
    let Some(reference) = Reference::new(samples) else {
        return (words, Vec::new(), Vec::new());
    };
    let mut drop = vec![false; words.len()];
    let (mut sounds, mut out) = (Vec::new(), Vec::new());
    for r in lines(&words) {
        if is_doubtful(&words[r.clone()], base, &reference) {
            let line = dropped(&words, r.clone(), DropReason::Doubtful);
            sounds.push(crate::transcribe::Sound {
                kind: crate::transcribe::SoundKind::Unclear,
                start: line.start,
                end: line.end,
            });
            out.push(line);
            drop[r].iter_mut().for_each(|d| *d = true);
        }
    }
    if out.is_empty() {
        return (words, sounds, out);
    }
    (without(words, &drop), sounds, out)
}

/// Whether the line `words` (audio starts at `base`) is text over a stretch that holds no speech.
pub fn is_unsupported(words: &[Word], base: MediaTime, reference: &Reference) -> bool {
    let (Some(first), Some(last)) = (words.first(), words.last()) else {
        return false;
    };
    let (a, b) = (
        (first.start - base).as_secs_f64(),
        (last.end - base).as_secs_f64(),
    );
    if b - a < MIN_LINE_SECS {
        return false;
    }
    let Some(margin) = reference.margin(a, b) else {
        return false;
    };
    let lowest = words.iter().map(|w| w.confidence).fold(1.0f32, f32::min);
    (words.len() >= ROOM_MIN_WORDS && margin < ROOM_MARGIN_DB)
        || (words.len() >= SURE_MIN_WORDS && lowest >= SURE && margin < SURE_MARGIN_DB)
}

fn dropped(words: &[Word], r: Range<usize>, reason: DropReason) -> DroppedLine {
    DroppedLine {
        start: words[r.start].start,
        end: words[r.end - 1].end,
        words: r.len() as u32,
        reason,
    }
}

fn without(words: Vec<Word>, drop: &[bool]) -> Vec<Word> {
    words
        .into_iter()
        .zip(drop)
        .filter_map(|(w, d)| (!d).then_some(w))
        .collect()
}

/// `words` without the unsupported lines (see the module), and the lines dropped. `samples` is
/// the audio the words were read from and `base` its start in the time of the words.
pub fn drop_unsupported(
    words: Vec<Word>,
    samples: &[f32],
    base: MediaTime,
) -> (Vec<Word>, Vec<DroppedLine>) {
    let Some(reference) = Reference::new(samples) else {
        return (words, Vec::new());
    };
    let mut out = Vec::new();
    let mut drop = vec![false; words.len()];
    for r in lines(&words) {
        if is_unsupported(&words[r.clone()], base, &reference) {
            out.push(dropped(&words, r.clone(), DropReason::Unsupported));
            drop[r].iter_mut().for_each(|d| *d = true);
        }
    }
    if out.is_empty() {
        return (words, out);
    }
    (without(words, &drop), out)
}

/// The phrases of the setting: strings, trimmed and clipped, no duplicates (compared folded), none
/// with fewer than [`MIN_PHRASE_CHARS`] letters or digits; a damaged setting gives none.
pub fn phrases_from_value(v: Option<&Value>) -> Vec<String> {
    let Some(items) = v.and_then(Value::as_array) else {
        return Vec::new();
    };
    clean_phrases(items.iter().filter_map(Value::as_str))
}

/// The cleaning rule of [`phrases_from_value`], for the editor too.
pub fn clean_phrases<'a>(items: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut seen: Vec<String> = Vec::new();
    let mut out = Vec::new();
    for s in items {
        let s: String = s
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(MAX_PHRASE_CHARS)
            .collect();
        let k = fold(&s);
        if k.chars().count() < MIN_PHRASE_CHARS || seen.contains(&k) {
            continue;
        }
        seen.push(k);
        out.push(s);
        if out.len() >= MAX_PHRASES {
            break;
        }
    }
    out
}

/// `words` without the lines that contain one of `phrases`, and the lines dropped. A phrase is
/// found in the folded text of the words, from the start of one word to the end of another (it
/// may run over a pause, in which case the lines it touches all go).
pub fn drop_ignored(words: Vec<Word>, phrases: &[String]) -> (Vec<Word>, Vec<DroppedLine>) {
    let needles: Vec<String> = phrases
        .iter()
        .map(|p| fold(p))
        .filter(|k| k.chars().count() >= MIN_PHRASE_CHARS)
        .collect();
    if needles.is_empty() || words.is_empty() {
        return (words, Vec::new());
    }
    let mut text = String::new();
    let (mut starts, mut ends): (HashMap<usize, usize>, HashMap<usize, usize>) =
        (HashMap::new(), HashMap::new());
    for (i, w) in words.iter().enumerate() {
        let f = fold(&w.text);
        if f.is_empty() {
            continue;
        }
        starts.insert(text.len(), i);
        text.push_str(&f);
        ends.insert(text.len(), i);
    }
    let all = lines(&words);
    let line_of = |w: usize| all.iter().position(|r| r.contains(&w)).unwrap_or(0);
    let mut hit = vec![false; all.len()];
    for n in &needles {
        let mut from = 0;
        while let Some(p) = text[from..].find(n.as_str()) {
            let at = from + p;
            if let (Some(&i), Some(&j)) = (starts.get(&at), ends.get(&(at + n.len()))) {
                (line_of(i)..=line_of(j)).for_each(|l| hit[l] = true);
            }
            from = at + text[at..].chars().next().map_or(1, char::len_utf8);
        }
    }
    let mut out = Vec::new();
    let mut drop = vec![false; words.len()];
    for (l, r) in all.iter().enumerate() {
        if hit[l] {
            out.push(dropped(&words, r.clone(), DropReason::Ignored));
            drop[r.clone()].iter_mut().for_each(|d| *d = true);
        }
    }
    if out.is_empty() {
        return (words, out);
    }
    (without(words, &drop), out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcribe::media_secs;

    /// AI-1.AC12: the shape check runs on Vietnamese only for now.
    #[test]
    fn the_shape_check_runs_on_vietnamese_only() {
        assert!(applies_to("vi"));
        assert!(applies_to("VI"));
        assert!(!applies_to("en"));
        assert!(!applies_to(""));
    }

    fn w(text: &str, at: f64, conf: f32) -> Word {
        Word {
            text: text.into(),
            start: media_secs(at),
            end: media_secs(at + 0.3),
            confidence: conf,
        }
    }

    /// Words one every 0.4 s from `at`, with the given confidence.
    fn say(text: &str, at: f64, conf: f32) -> Vec<Word> {
        text.split(' ')
            .enumerate()
            .map(|(i, t)| w(t, at + i as f64 * 0.4, conf))
            .collect()
    }

    /// 60 s: a noisy-room floor of 0.002 with speech-level noise (0.2) in 0..20 s and 40..60 s;
    /// the stretch 20..40 s is the room only.
    fn audio() -> Vec<f32> {
        (0..60 * 16_000)
            .map(|i| {
                let s = i / 16_000;
                let a = if (20..40).contains(&s) { 0.002 } else { 0.2 };
                a * ((i as f32) * 0.37).sin()
            })
            .collect()
    }

    #[test]
    fn a_sign_off_over_the_room_goes_and_real_speech_stays() {
        let mut words = say("so the numbers look fine this quarter.", 5.0, 0.8);
        // The invented line: confident, over the quiet stretch (different channel names must not matter).
        words.extend(say(
            "Hãy subscribe cho kênh La La School để không bỏ lỡ",
            25.0,
            0.99,
        ));
        words.extend(say("and then we move to the next item.", 45.0, 0.9));
        let (kept, dropped) = drop_unsupported(words.clone(), &audio(), MediaTime::ZERO);
        assert_eq!(dropped.len(), 1);
        assert_eq!(dropped[0].reason, DropReason::Unsupported);
        assert_eq!(dropped[0].words, 11);
        assert!((dropped[0].start.as_secs_f64() - 25.0).abs() < 1e-6);
        assert_eq!(kept.len(), words.len() - 11);
        assert!(kept.iter().all(|k| k.text != "subscribe"));
    }

    #[test]
    fn a_muted_recording_keeps_a_room_level_and_the_guard_still_works() {
        // 120 s: 0..60 s muted (exact zeros), speech-level noise 60..80 s and 100..120 s, the room
        // only 80..100 s (BUG-10: the zeros used to set the room level to -120 dB).
        let x: Vec<f32> = (0..120 * 16_000)
            .map(|i| {
                let s = i / 16_000;
                let a = match s {
                    0..=59 => 0.0,
                    80..=99 => 0.002,
                    _ => 0.2,
                };
                a * ((i as f32) * 0.37).sin()
            })
            .collect();
        let mut words = say("so the numbers look fine this quarter.", 65.0, 0.8);
        words.extend(say(
            "Hãy subscribe cho kênh La La School để không bỏ lỡ",
            85.0,
            0.99,
        ));
        let (kept, dropped) = drop_unsupported(words.clone(), &x, MediaTime::ZERO);
        assert_eq!(dropped.len(), 1);
        assert!((dropped[0].start.as_secs_f64() - 85.0).abs() < 1e-6);
        assert_eq!(kept.len(), words.len() - dropped[0].words as usize);
        // Too few frames that are not silent (under 4 s): no reference, everything stays.
        let mostly_muted: Vec<f32> = x
            .iter()
            .enumerate()
            .map(|(i, v)| if i < 118 * 16_000 { 0.0 } else { *v })
            .collect();
        assert!(Reference::new(&mostly_muted).is_none());
        let (kept, dropped) = drop_unsupported(words.clone(), &mostly_muted, MediaTime::ZERO);
        assert!(dropped.is_empty());
        assert_eq!(kept.len(), words.len());
    }

    #[test]
    fn a_long_line_over_the_room_goes_whatever_the_confidence() {
        let words = say("one two three four five six", 25.0, 0.5);
        let (kept, dropped) = drop_unsupported(words, &audio(), MediaTime::ZERO);
        assert!(kept.is_empty());
        assert_eq!(dropped.len(), 1);
    }

    #[test]
    fn unsure_short_text_over_the_room_stays() {
        // Four words, one of them unsure: neither rule fires.
        let mut words = say("yes okay maybe later", 25.0, 0.99);
        words[2].confidence = 0.6;
        let (kept, dropped) = drop_unsupported(words, &audio(), MediaTime::ZERO);
        assert_eq!(kept.len(), 4);
        assert!(dropped.is_empty());
    }

    #[test]
    fn speech_that_is_confident_but_audible_is_kept() {
        let words = say("thank you all for watching today.", 5.0, 0.99);
        let (kept, dropped) = drop_unsupported(words.clone(), &audio(), MediaTime::ZERO);
        assert_eq!(kept, words);
        assert!(dropped.is_empty());
    }

    /// 9 s: speech-level noise in 0..3 s and 6..9 s, the room only in between.
    fn short_audio() -> Vec<f32> {
        (0..9 * 16_000)
            .map(|i| {
                let a = if (3..6).contains(&(i / 16_000)) {
                    0.002
                } else {
                    0.2
                };
                a * ((i as f32) * 0.37).sin()
            })
            .collect()
    }

    /// AI-1.AC12 (0.3.6): audio under 30 s is judged when speech and room are 15 dB apart and
    /// 4 s of it is not digital silence.
    #[test]
    fn short_audio_has_a_reference_when_speech_and_room_are_clearly_apart() {
        assert!(Reference::new(&short_audio()).is_some());
        // 3 s of content: too few frames. Flat audio: no range.
        assert!(Reference::new(&short_audio()[..3 * 16_000]).is_none());
        let flat: Vec<f32> = (0..9 * 16_000)
            .map(|i| 0.1 * ((i as f32) * 0.3).sin())
            .collect();
        assert!(Reference::new(&flat).is_none());
        // 12 dB apart is enough for a long recording, not for a short one.
        let narrow = |secs: usize| -> Vec<f32> {
            (0..secs * 16_000)
                .map(|i| {
                    let a = if (i / 16_000) % 2 == 0 { 0.05 } else { 0.0125 };
                    a * ((i as f32) * 0.37).sin()
                })
                .collect()
        };
        assert!(Reference::new(&narrow(9)).is_none());
        assert!(Reference::new(&narrow(40)).is_some());
    }

    /// AI-1.AC12 (0.3.6): mostly low-confidence words over the room become one `[unclear]` sound
    /// entry; the same words over speech-level audio, or sure ones over the room, stay.
    #[test]
    fn doubtful_words_over_the_room_become_an_unclear_sound() {
        let mut words = say("so the numbers", 0.5, 0.9);
        words.extend(say("one two three four", 3.5, 0.2));
        words.extend(say("then we move", 6.5, 0.9));
        let (kept, sounds, dropped) = drop_doubtful(words.clone(), &short_audio(), MediaTime::ZERO);
        assert_eq!(kept.len(), 6);
        assert_eq!(dropped.len(), 1);
        assert_eq!(dropped[0].reason, DropReason::Doubtful);
        assert_eq!(dropped[0].words, 4);
        assert_eq!(sounds.len(), 1);
        assert_eq!(sounds[0].kind, crate::transcribe::SoundKind::Unclear);
        assert_eq!(
            (sounds[0].start, sounds[0].end),
            (dropped[0].start, dropped[0].end)
        );
        // The same low confidence over speech level stays.
        let mut loud = say("so the numbers", 0.5, 0.2);
        loud.extend(say("then we move", 6.5, 0.1));
        assert_eq!(
            drop_doubtful(loud.clone(), &short_audio(), MediaTime::ZERO).0,
            loud
        );
        // Sure words over the room are not doubtful.
        let sure = say("one two three four", 3.5, 0.9);
        assert!(drop_doubtful(sure, &short_audio(), MediaTime::ZERO)
            .2
            .is_empty());
        // One low word among sure ones is not "mostly low".
        let mut mixed = say("one two three four", 3.5, 0.9);
        mixed[1].confidence = 0.1;
        assert!(drop_doubtful(mixed, &short_audio(), MediaTime::ZERO)
            .2
            .is_empty());
    }

    /// AI-1.AC12 (0.3.6): a line with under a quarter second of audio under its words is not
    /// judged, and a last word stretched over a long pause does not turn speech into room.
    #[test]
    fn a_tiny_line_and_a_stretched_last_word_are_handled() {
        let tiny = vec![Word {
            text: "x".into(),
            start: media_secs(4.0),
            end: media_secs(4.1),
            confidence: 0.1,
        }];
        assert!(drop_doubtful(tiny, &short_audio(), MediaTime::ZERO)
            .2
            .is_empty());
        let mut real = say("so the numbers look", 0.5, 0.2);
        real.last_mut().unwrap().end = media_secs(9.0);
        assert!(drop_doubtful(real, &short_audio(), MediaTime::ZERO)
            .2
            .is_empty());
    }

    #[test]
    fn without_a_reference_level_everything_stays() {
        // Short audio, and audio where speech and room are the same level.
        let words = say("one two three four five six", 2.0, 0.99);
        let short = vec![0.0f32; 16_000 * 10];
        assert_eq!(
            drop_unsupported(words.clone(), &short, MediaTime::ZERO).0,
            words
        );
        let flat: Vec<f32> = (0..60 * 16_000)
            .map(|i| 0.1 * ((i as f32) * 0.3).sin())
            .collect();
        assert_eq!(
            drop_unsupported(words.clone(), &flat, MediaTime::ZERO).0,
            words
        );
    }

    #[test]
    fn the_audio_start_is_the_zero_of_the_levels() {
        // The same words, but the audio starts 20 s into the word times.
        let words = say("one two three four five six", 25.0, 0.5);
        let (kept, _) = drop_unsupported(words.clone(), &audio(), media_secs(20.0));
        assert_eq!(kept, words, "5 s into the audio is speech level");
    }

    #[test]
    fn lines_end_at_a_mark_or_a_pause() {
        let mut words = say("a b c.", 0.0, 0.9);
        words.extend(say("d e", 3.0, 0.9));
        words.extend(say("f g", 10.0, 0.9));
        assert_eq!(lines(&words), [0..3, 3..5, 5..7]);
    }

    #[test]
    fn ignore_phrases_match_folded_text_inside_a_line() {
        let words = say(
            "ok so Hãy Subscribe cho Kênh Ghiền Mì Gõ nhé. then we go",
            1.0,
            0.9,
        );
        let phrases = vec!["subscribe CHO kenh".to_string()];
        let (kept, dropped) = drop_ignored(words.clone(), &phrases);
        assert_eq!(dropped.len(), 1);
        assert_eq!(dropped[0].reason, DropReason::Ignored);
        assert_eq!(
            kept.iter().map(|k| k.text.as_str()).collect::<Vec<_>>(),
            ["then", "we", "go"]
        );
        // Another channel name: the fragment still matches.
        let other = say("Hãy subscribe cho kênh La La School", 1.0, 0.9);
        assert!(drop_ignored(other, &phrases).0.is_empty());
    }

    #[test]
    fn ignore_phrases_need_whole_words_and_do_nothing_when_empty() {
        let words = say("we subscribed to the newsletter", 1.0, 0.9);
        let phrases = vec!["subscribe".to_string()];
        assert_eq!(
            drop_ignored(words.clone(), &phrases).0,
            words,
            "mid-word is no match"
        );
        assert_eq!(drop_ignored(words.clone(), &[]).0, words);
        assert_eq!(
            drop_ignored(words.clone(), &["ab".to_string()]).0,
            words,
            "too short"
        );
    }

    #[test]
    fn a_phrase_over_a_pause_drops_every_line_it_touches() {
        let mut words = say("Hãy subscribe cho", 1.0, 0.9);
        words.extend(say("kênh La La School", 6.0, 0.9));
        words.extend(say("and that is all", 20.0, 0.9));
        let (kept, dropped) = drop_ignored(words, &["subscribe cho kênh".to_string()]);
        assert_eq!(dropped.len(), 2);
        assert_eq!(kept.len(), 4);
    }

    #[test]
    fn the_setting_is_cleaned() {
        let v = serde_json::json!(["  Subscribe   cho kênh ", "subscribe cho kenh", "ab", 7, ""]);
        assert_eq!(phrases_from_value(Some(&v)), ["Subscribe cho kênh"]);
        assert!(phrases_from_value(None).is_empty());
        assert!(phrases_from_value(Some(&serde_json::json!("x"))).is_empty());
    }
}
