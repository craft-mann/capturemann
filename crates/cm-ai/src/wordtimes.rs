//! Repair of words that the decoder stacked on one instant.
//!
//! Whisper's word timing sometimes gives a run of words the same start and end (a few milliseconds
//! at most), most often at the end of a phrase. The words are right, the times are not: a caption
//! built from them has a zero-length line, a cut made on them falls inside speech. [`repair`]
//! spreads such a run evenly, in proportion to the length of each word, over the room between the
//! word before it and the word after it. It works on structure only (counts, durations, gaps), no
//! text is read except its length.

use cm_core::{MediaTime, TICKS_PER_SECOND};

use crate::transcribe::Word;

/// A word shorter than this is collapsed (10 ms).
pub const COLLAPSED_SECS: f64 = 0.010;
/// Fewer collapsed words than this in a row are left alone (a single short word is real).
pub const MIN_STACK: usize = 4;
/// The most time a spread gives one character (seconds): a long silence after a stack is not
/// filled with its words.
pub const MAX_SECS_PER_CHAR: f64 = 0.12;

fn ticks(secs: f64) -> i64 {
    (secs * TICKS_PER_SECOND as f64).round() as i64
}

/// Spreads each stack of [`MIN_STACK`] or more collapsed words over the gap between its
/// neighbours. `audio_end` is the end of the decoded audio: nothing is moved past it. Returns the
/// number of words whose time changed. Words keep their order and a word whose duration is
/// plausible is never touched.
pub fn repair(words: &mut [Word], audio_end: Option<MediaTime>) -> usize {
    let collapsed = |w: &Word| (w.end - w.start).ticks() < ticks(COLLAPSED_SECS);
    let mut changed = 0;
    let mut i = 0;
    while i < words.len() {
        if !collapsed(&words[i]) {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        while j < words.len() && collapsed(&words[j]) {
            j += 1;
        }
        if j - i >= MIN_STACK {
            changed += spread(words, i, j, audio_end);
        }
        i = j;
    }
    changed
}

fn spread(words: &mut [Word], i: usize, j: usize, audio_end: Option<MediaTime>) -> usize {
    let t0 = words[i].start.ticks();
    // The room starts at the end of the word before, or at the stack's own time when it opens
    // the audio.
    let left = if i > 0 {
        words[i - 1].end.ticks().min(t0)
    } else {
        t0
    };
    let mut right = if j < words.len() {
        words[j].start.ticks()
    } else {
        audio_end.map_or(t0, MediaTime::ticks)
    };
    if let Some(end) = audio_end {
        right = right.min(end.ticks());
    }
    let room = right - left;
    if room <= 0 {
        return 0;
    }
    let lens: Vec<i64> = words[i..j]
        .iter()
        .map(|w| w.text.chars().count().max(1) as i64)
        .collect();
    let total: i64 = lens.iter().sum();
    let span = room.min(ticks(MAX_SECS_PER_CHAR) * total);
    // Where the block sits: at the decoder's own time when that fits, else as near as it can.
    let start = t0.clamp(left, right - span);
    let mut at = start;
    let mut done = 0i64;
    let mut moved = 0;
    for (w, len) in words[i..j].iter_mut().zip(lens) {
        done += len;
        let end = start + span * done / total;
        let (s, e) = (MediaTime::from_ticks(at), MediaTime::from_ticks(end));
        if w.start != s || w.end != e {
            moved += 1;
        }
        w.start = s;
        w.end = e;
        at = end;
    }
    moved
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(text: &str, a: f64, b: f64) -> Word {
        Word {
            text: text.into(),
            start: MediaTime::from_ticks(ticks(a)),
            end: MediaTime::from_ticks(ticks(b)),
            confidence: 0.9,
        }
    }

    fn secs(m: MediaTime) -> f64 {
        m.ticks() as f64 / TICKS_PER_SECOND as f64
    }

    #[test]
    fn a_stack_is_spread_in_proportion_to_length_between_its_neighbours() {
        let mut v = vec![
            w("zan", 0.0, 1.0),
            w("a", 1.2, 1.2),
            w("blorp", 1.2, 1.2),
            w("qu", 1.2, 1.205),
            w("nim", 1.2, 1.2),
            w("kral", 2.0, 3.0),
        ];
        let n = repair(&mut v, None);
        assert_eq!(n, 4);
        // 11 characters may take 1.32 s, the gap is 1.0 s: the whole gap is used.
        assert!((secs(v[1].start) - 1.0).abs() < 1e-6);
        assert!((secs(v[4].end) - 2.0).abs() < 1e-6);
        for p in v.windows(2) {
            assert!(p[0].end <= p[1].start, "{p:?}");
        }
        // Proportional: "blorp" (5) is five times "a" (1).
        let d = |k: usize| (v[k].end - v[k].start).ticks() as f64;
        assert!((d(2) / d(1) - 5.0).abs() < 0.01);
        // The neighbours did not move.
        assert_eq!(v[0], w("zan", 0.0, 1.0));
        assert_eq!(v[5], w("kral", 2.0, 3.0));
    }

    #[test]
    fn a_long_silence_after_a_stack_is_not_filled() {
        let mut v = vec![
            w("a", 5.0, 5.0),
            w("bb", 5.0, 5.0),
            w("c", 5.0, 5.0),
            w("dd", 5.0, 5.0),
            w("ee", 60.0, 61.0),
        ];
        assert_eq!(repair(&mut v, None), 4);
        let total: f64 = secs(v[3].end) - secs(v[0].start);
        assert!((total - 6.0 * MAX_SECS_PER_CHAR).abs() < 1e-3, "{total}");
        assert!((secs(v[0].start) - 5.0).abs() < 1e-6);
    }

    #[test]
    fn short_stacks_and_plausible_words_are_left_alone() {
        let mut v = vec![
            w("a", 1.0, 1.0),
            w("b", 1.0, 1.0),
            w("c", 1.0, 1.0),
            w("d", 1.5, 1.7),
            w("e", 1.7, 1.7),
        ];
        let before = v.clone();
        assert_eq!(repair(&mut v, None), 0);
        assert_eq!(v, before);
    }

    #[test]
    fn nothing_moves_when_there_is_no_room_or_past_the_audio() {
        // The next word starts at the same instant: no room.
        let mut v = vec![
            w("a", 1.0, 1.0),
            w("b", 1.0, 1.0),
            w("c", 1.0, 1.0),
            w("d", 1.0, 1.0),
            w("e", 1.0, 2.0),
        ];
        let before = v.clone();
        assert_eq!(repair(&mut v, None), 0);
        assert_eq!(v, before);
        // A stack at the end is held by the end of the audio.
        let mut v = vec![
            w("aa", 9.9, 9.9),
            w("bb", 9.9, 9.9),
            w("cc", 9.9, 9.9),
            w("dd", 9.9, 9.9),
        ];
        let end = MediaTime::from_ticks(ticks(10.0));
        repair(&mut v, Some(end));
        assert!(v.iter().all(|x| x.end <= end), "{v:?}");
        assert!(v[0].start >= MediaTime::from_ticks(0));
        for p in v.windows(2) {
            assert!(p[0].end <= p[1].start);
        }
    }

    #[test]
    fn chinese_and_vietnamese_looking_text_is_measured_in_characters() {
        let mut v = vec![
            w("好", 2.0, 2.0),
            w("的的的", 2.0, 2.0),
            w("ổổ", 2.0, 2.0),
            w("x", 2.0, 2.0),
            w("zed", 3.0, 3.5),
        ];
        repair(&mut v, None);
        let d = |k: usize| (v[k].end - v[k].start).ticks();
        assert_eq!(d(1), d(0) * 3);
        assert_eq!(d(2), d(0) * 2);
    }
}
