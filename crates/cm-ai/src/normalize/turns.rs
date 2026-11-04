//! Turns: the unit the model is asked about. A turn is a run of words of one speaker; a run
//! that is too long (or, without speaker labels, longer than 30 s) is cut at its longest pause,
//! recursively, so no part is larger than the limits.

use crate::transcribe::Word;

/// A range of words of the transcript.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub first: usize,
    pub len: usize,
    pub speaker: Option<u32>,
}

impl Span {
    pub fn end(&self) -> usize {
        self.first + self.len
    }
}

/// Most words in one piece sent to the model.
pub const MAX_TURN_WORDS: usize = 140;
/// Longest stretch of audio of one piece when the transcript has no speaker labels.
pub const MAX_UNLABELLED_SECS: f64 = 30.0;

/// Cuts `[first, first+len)` at the longest gap until every piece has at most `max_words` words
/// and (when `max_secs` is set) spans at most that many seconds. A cut never leaves a piece of
/// fewer than a quarter of the words.
fn cut(
    words: &[Word],
    first: usize,
    len: usize,
    speaker: Option<u32>,
    max_words: usize,
    max_secs: Option<f64>,
    out: &mut Vec<Span>,
) {
    let secs = |a: usize, b: usize| words[b - 1].end.as_secs_f64() - words[a].start.as_secs_f64();
    let too_long =
        len > max_words || max_secs.is_some_and(|m| len > 1 && secs(first, first + len) > m);
    if !too_long || len < 2 {
        out.push(Span {
            first,
            len,
            speaker,
        });
        return;
    }
    let lo = (len / 4).max(1);
    let hi = (len - len / 4).min(len - 1).max(lo);
    let mut best = (f64::NEG_INFINITY, len / 2);
    for k in lo..=hi {
        let gap = words[first + k].start.as_secs_f64() - words[first + k - 1].end.as_secs_f64();
        if gap > best.0 {
            best = (gap, k);
        }
    }
    let k = best.1;
    cut(words, first, k, speaker, max_words, max_secs, out);
    cut(words, first + k, len - k, speaker, max_words, max_secs, out);
}

/// The turns of a transcript. `speakers` is one label per word (`None`: unknown, continues the
/// current speaker); without it the whole transcript is one run cut every 30 s at a pause.
pub fn build(words: &[Word], speakers: Option<&[Option<u32>]>) -> Vec<Span> {
    let mut out = Vec::new();
    if words.is_empty() {
        return out;
    }
    let Some(labels) = speakers.filter(|l| l.len() == words.len()) else {
        cut(
            words,
            0,
            words.len(),
            None,
            MAX_TURN_WORDS,
            Some(MAX_UNLABELLED_SECS),
            &mut out,
        );
        return out;
    };
    let mut start = 0usize;
    let mut current = labels.iter().flatten().next().copied();
    for (i, label) in labels.iter().enumerate() {
        if let Some(l) = *label {
            if Some(l) != current {
                if i > start {
                    cut(
                        words,
                        start,
                        i - start,
                        current,
                        MAX_TURN_WORDS,
                        None,
                        &mut out,
                    );
                    start = i;
                }
                current = Some(l);
            }
        }
    }
    cut(
        words,
        start,
        words.len() - start,
        current,
        MAX_TURN_WORDS,
        None,
        &mut out,
    );
    out
}

/// Splits one span in two at its longest inner pause (for a reply that did not fit): `None` for
/// a span under `min_words` words.
pub fn halve(words: &[Word], s: Span, min_words: usize) -> Option<(Span, Span)> {
    if s.len < min_words.max(2) {
        return None;
    }
    let mut out = Vec::new();
    cut(words, s.first, s.len, s.speaker, s.len - 1, None, &mut out);
    // `cut` with a limit one below the length cuts exactly once at the best inner gap.
    match out.as_slice() {
        [a, b] => Some((*a, *b)),
        _ => {
            let k = s.len / 2;
            Some((
                Span {
                    first: s.first,
                    len: k,
                    speaker: s.speaker,
                },
                Span {
                    first: s.first + k,
                    len: s.len - k,
                    speaker: s.speaker,
                },
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cm_core::MediaTime;

    fn words(n: usize, gaps: &[(usize, f64)]) -> Vec<Word> {
        let mut t = 0.0;
        (0..n)
            .map(|i| {
                if let Some((_, g)) = gaps.iter().find(|(at, _)| *at == i) {
                    t += g;
                }
                let w = Word {
                    text: format!("w{i}"),
                    start: MediaTime::from_secs_f64(t).unwrap(),
                    end: MediaTime::from_secs_f64(t + 0.3).unwrap(),
                    confidence: 0.9,
                };
                t += 0.4;
                w
            })
            .collect()
    }

    fn covers(spans: &[Span], n: usize) {
        let mut at = 0;
        for s in spans {
            assert_eq!(s.first, at);
            assert!(s.len > 0);
            at = s.end();
        }
        assert_eq!(at, n);
    }

    #[test]
    fn a_speaker_change_starts_a_turn() {
        let w = words(10, &[]);
        let labels: Vec<Option<u32>> = [1, 1, 1, 2, 2, 2, 2, 1, 1, 1]
            .iter()
            .map(|x| Some(*x))
            .collect();
        let t = build(&w, Some(&labels));
        assert_eq!(t.len(), 3);
        assert_eq!((t[1].first, t[1].len, t[1].speaker), (3, 4, Some(2)));
        covers(&t, 10);
    }

    #[test]
    fn an_unknown_label_continues_the_speaker() {
        let w = words(6, &[]);
        let labels = vec![Some(1), None, Some(1), None, Some(2), None];
        let t = build(&w, Some(&labels));
        assert_eq!(t.len(), 2);
        assert_eq!(t[0].len, 4);
    }

    #[test]
    fn a_long_turn_is_cut_at_its_longest_pause() {
        let w = words(300, &[(120, 2.0)]);
        let labels = vec![Some(1); 300];
        let t = build(&w, Some(&labels));
        covers(&t, 300);
        assert!(t.iter().all(|s| s.len <= MAX_TURN_WORDS));
        assert_eq!(t[0].end(), 120, "{t:?}");
    }

    #[test]
    fn without_speakers_pieces_are_at_most_thirty_seconds() {
        let w = words(400, &[]);
        let t = build(&w, None);
        covers(&t, 400);
        for s in &t {
            let secs = w[s.end() - 1].end.as_secs_f64() - w[s.first].start.as_secs_f64();
            assert!(secs <= MAX_UNLABELLED_SECS + 0.001, "{secs}");
            assert!(s.len <= MAX_TURN_WORDS);
        }
    }

    #[test]
    fn short_input_and_empty_input() {
        assert!(build(&[], None).is_empty());
        let w = words(1, &[]);
        assert_eq!(build(&w, None).len(), 1);
    }

    #[test]
    fn halving_cuts_once_and_keeps_every_word() {
        let w = words(40, &[(15, 1.5)]);
        let s = Span {
            first: 0,
            len: 40,
            speaker: None,
        };
        let (a, b) = halve(&w, s, 6).unwrap();
        assert_eq!((a.len, b.first, a.len + b.len), (15, 15, 40));
        assert!(halve(
            &w,
            Span {
                first: 0,
                len: 5,
                speaker: None
            },
            6
        )
        .is_none());
    }
}
