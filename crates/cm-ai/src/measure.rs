//! Accuracy measures for the release 0.2.3 work: word error rate for transcripts and diarization
//! error rate for speaker turns. Pure functions on plain text and seconds, so they are testable
//! without models. They are used by `examples/measure.rs` and the ignored real-model tests, never
//! by the app: an accuracy figure needs a hand-made reference, and none ships.

use std::collections::BTreeSet;

use crate::pieces::fold;

/// Hesitation sounds that transcribers of conversations write down and a captioning transcript
/// leaves out; [`word_errors_without`] can score with them removed from both texts (the usual
/// way to score speech that includes disfluencies). Used only for scoring.
pub const DISFLUENCIES: &[&str] = &[
    "um", "uh", "uhm", "umm", "er", "erm", "mm", "mmm", "hmm", "hm", "mhm", "uhhuh",
];

/// Substitutions, insertions and deletions between a reference text and a transcript.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WordErrors {
    pub reference_words: usize,
    pub substitutions: usize,
    pub insertions: usize,
    pub deletions: usize,
}

impl WordErrors {
    pub fn errors(&self) -> usize {
        self.substitutions + self.insertions + self.deletions
    }

    /// Errors per reference word (0.0 for an empty reference with no errors, otherwise can pass 1.0).
    pub fn wer(&self) -> f64 {
        if self.reference_words == 0 {
            return if self.errors() == 0 { 0.0 } else { 1.0 };
        }
        self.errors() as f64 / self.reference_words as f64
    }
}

/// Words as they are compared: split at white space, folded like [`fold`] (case, diacritics, `đ`,
/// punctuation), tokens that fold to nothing dropped.
pub fn words_of(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(fold)
        .filter(|w| !w.is_empty())
        .collect()
}

/// Word-level edit distance of `hypothesis` against `reference`, split into the three kinds.
pub fn word_errors(reference: &str, hypothesis: &str) -> WordErrors {
    word_errors_without(reference, hypothesis, &[])
}

/// [`word_errors`] with the words in `skip` (already folded, like [`DISFLUENCIES`]) removed from
/// both texts first.
pub fn word_errors_without(reference: &str, hypothesis: &str, skip: &[&str]) -> WordErrors {
    let keep = |w: &String| !skip.contains(&w.as_str());
    let r: Vec<String> = words_of(reference).into_iter().filter(keep).collect();
    let h: Vec<String> = words_of(hypothesis).into_iter().filter(keep).collect();
    let (n, m) = (r.len(), h.len());
    // cost[i][j]: edits to turn r[..i] into h[..j].
    let mut cost = vec![vec![0usize; m + 1]; n + 1];
    for (i, row) in cost.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, c) in cost[0].iter_mut().enumerate() {
        *c = j;
    }
    for i in 1..=n {
        for j in 1..=m {
            let same = usize::from(r[i - 1] != h[j - 1]);
            cost[i][j] = (cost[i - 1][j - 1] + same)
                .min(cost[i - 1][j] + 1)
                .min(cost[i][j - 1] + 1);
        }
    }
    let mut out = WordErrors {
        reference_words: n,
        ..WordErrors::default()
    };
    let (mut i, mut j) = (n, m);
    while i > 0 || j > 0 {
        if i > 0 && j > 0 && cost[i][j] == cost[i - 1][j - 1] + usize::from(r[i - 1] != h[j - 1]) {
            if r[i - 1] != h[j - 1] {
                out.substitutions += 1;
            }
            i -= 1;
            j -= 1;
        } else if i > 0 && cost[i][j] == cost[i - 1][j] + 1 {
            out.deletions += 1;
            i -= 1;
        } else {
            out.insertions += 1;
            j -= 1;
        }
    }
    out
}

/// One stretch of one speaker, in seconds; the label is any text (reference and result use
/// different labels, [`diarization_errors`] maps them).
#[derive(Debug, Clone, PartialEq)]
pub struct Turn {
    pub start: f64,
    pub end: f64,
    pub speaker: String,
}

/// Parses a reference turn list: one turn per line, `start duration speaker` in seconds, or an RTTM
/// `SPEAKER file channel start duration <NA> <NA> speaker ...` line. Blank lines and lines
/// starting with `#` are skipped.
pub fn parse_turns(text: &str) -> Result<Vec<Turn>, String> {
    let mut turns = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let f: Vec<&str> = line.split_whitespace().collect();
        let (start, dur, speaker) = match f.as_slice() {
            [s, d, who] => (*s, *d, *who),
            ["SPEAKER", _, _, s, d, _, _, who, ..] => (*s, *d, *who),
            _ => return Err(format!("line {}: expected `start duration speaker`", n + 1)),
        };
        let num = |v: &str| {
            v.parse::<f64>()
                .ok()
                .filter(|x| x.is_finite() && *x >= 0.0)
                .ok_or_else(|| format!("line {}: `{v}` is not a time in seconds", n + 1))
        };
        let (start, dur) = (num(start)?, num(dur)?);
        turns.push(Turn {
            start,
            end: start + dur,
            speaker: speaker.to_string(),
        });
    }
    Ok(turns)
}

/// Diarization error rate and its parts, in seconds of scored speech.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DiarizationErrors {
    /// Reference speech seconds that were scored (outside the collars).
    pub scored: f64,
    pub missed: f64,
    pub false_alarm: f64,
    pub confusion: f64,
    pub reference_speakers: usize,
    pub found_speakers: usize,
}

impl DiarizationErrors {
    pub fn der(&self) -> f64 {
        if self.scored <= 0.0 {
            return 0.0;
        }
        (self.missed + self.false_alarm + self.confusion) / self.scored
    }
}

const FRAME: f64 = 0.01;

fn labels(turns: &[Turn]) -> Vec<&str> {
    let set: BTreeSet<&str> = turns.iter().map(|t| t.speaker.as_str()).collect();
    set.into_iter().collect()
}

/// Diarization error rate of `found` against `reference`, in 10 ms frames. Frames within
/// `collar` seconds of any reference turn boundary are not scored. Labels are mapped one to one
/// by the most shared speech (greedy by overlap), as the labels of a result mean nothing by
/// themselves. Overlapping reference speech counts once per speaker.
pub fn diarization_errors(reference: &[Turn], found: &[Turn], collar: f64) -> DiarizationErrors {
    let ref_labels = labels(reference);
    let hyp_labels = labels(found);
    let end = reference
        .iter()
        .chain(found)
        .map(|t| t.end)
        .fold(0.0, f64::max);
    let frames = (end / FRAME).ceil() as usize + 1;

    // Active labels per frame, as bit sets over the label lists (up to 64 speakers each).
    let fill = |turns: &[Turn], names: &[&str]| {
        let mut v = vec![0u64; frames];
        for t in turns {
            let Some(bit) = names
                .iter()
                .position(|n| *n == t.speaker)
                .filter(|b| *b < 64)
            else {
                continue;
            };
            let a = (t.start / FRAME).round() as usize;
            let b = ((t.end / FRAME).round() as usize).min(frames);
            for f in &mut v[a.min(frames)..b] {
                *f |= 1 << bit;
            }
        }
        v
    };
    let r = fill(reference, &ref_labels);
    let h = fill(found, &hyp_labels);

    let mut ignored = vec![false; frames];
    if collar > 0.0 {
        let reach = (collar / FRAME).round() as usize;
        for t in reference {
            for edge in [t.start, t.end] {
                let c = (edge / FRAME).round() as usize;
                let from = c.saturating_sub(reach);
                let to = (c + reach).min(frames);
                for flag in &mut ignored[from.min(frames)..to] {
                    *flag = true;
                }
            }
        }
    }

    // Overlap of every (reference, found) pair, then a greedy one-to-one mapping.
    let mut shared = vec![vec![0usize; hyp_labels.len().min(64)]; ref_labels.len().min(64)];
    for f in 0..frames {
        if ignored[f] {
            continue;
        }
        for (i, row) in shared.iter_mut().enumerate() {
            if r[f] >> i & 1 == 0 {
                continue;
            }
            for (j, cell) in row.iter_mut().enumerate() {
                if h[f] >> j & 1 == 1 {
                    *cell += 1;
                }
            }
        }
    }
    let mut pairs: Vec<(usize, usize, usize)> = shared
        .iter()
        .enumerate()
        .flat_map(|(i, row)| row.iter().enumerate().map(move |(j, c)| (*c, i, j)))
        .filter(|(c, ..)| *c > 0)
        .collect();
    pairs.sort_by(|a, b| b.cmp(a));
    let (mut used_r, mut used_h) = (0u64, 0u64);
    let mut map = vec![None; ref_labels.len().min(64)];
    for (_, i, j) in pairs {
        if used_r >> i & 1 == 0 && used_h >> j & 1 == 0 {
            used_r |= 1 << i;
            used_h |= 1 << j;
            map[i] = Some(j);
        }
    }

    let (mut scored, mut missed, mut false_alarm, mut confusion) = (0usize, 0usize, 0usize, 0usize);
    for f in 0..frames {
        if ignored[f] {
            continue;
        }
        let nref = r[f].count_ones() as usize;
        let nhyp = h[f].count_ones() as usize;
        let correct = (0..map.len())
            .filter(|i| r[f] >> i & 1 == 1 && map[*i].is_some_and(|j| h[f] >> j & 1 == 1))
            .count();
        scored += nref;
        missed += nref.saturating_sub(nhyp);
        false_alarm += nhyp.saturating_sub(nref);
        confusion += nref.min(nhyp) - correct;
    }
    let secs = |n: usize| n as f64 * FRAME;
    DiarizationErrors {
        scored: secs(scored),
        missed: secs(missed),
        false_alarm: secs(false_alarm),
        confusion: secs(confusion),
        reference_speakers: ref_labels.len(),
        found_speakers: hyp_labels.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turn(start: f64, end: f64, who: &str) -> Turn {
        Turn {
            start,
            end,
            speaker: who.into(),
        }
    }

    #[test]
    fn identical_text_has_no_errors_and_folding_ignores_case_marks_and_punctuation() {
        let e = word_errors("Xin chào, các bạn Đây", "xin chao cac ban day");
        assert_eq!(e.errors(), 0);
        assert_eq!(e.reference_words, 5);
    }

    #[test]
    fn counts_each_kind_of_word_error() {
        let e = word_errors("a b c d", "a x c d e");
        assert_eq!((e.substitutions, e.insertions, e.deletions), (1, 1, 0));
        assert!((e.wer() - 0.5).abs() < 1e-9);
        let e = word_errors("a b c d", "a d");
        assert_eq!((e.substitutions, e.insertions, e.deletions), (0, 0, 2));
        assert_eq!(word_errors("", "").wer(), 0.0);
        assert_eq!(word_errors("", "a").wer(), 1.0);
    }

    #[test]
    fn skipped_words_count_in_neither_text() {
        let e = word_errors_without("so um we uh go", "so we go yeah", DISFLUENCIES);
        assert_eq!((e.reference_words, e.errors()), (3, 1));
        assert_eq!(word_errors("so um we", "so we").deletions, 1);
    }

    #[test]
    fn parses_simple_and_rttm_lines() {
        let t = parse_turns("# c\n0.0 2.5 A\nSPEAKER f 1 3.0 1.0 <NA> <NA> B <NA> <NA>\n").unwrap();
        assert_eq!(t, vec![turn(0.0, 2.5, "A"), turn(3.0, 4.0, "B")]);
        assert!(parse_turns("1 two A").is_err());
        assert!(parse_turns("oops").is_err());
    }

    #[test]
    fn perfect_result_with_other_labels_scores_zero() {
        let r = [turn(0.0, 5.0, "A"), turn(5.0, 10.0, "B")];
        let f = [turn(0.0, 5.0, "1"), turn(5.0, 10.0, "0")];
        let d = diarization_errors(&r, &f, 0.0);
        assert_eq!(d.der(), 0.0);
        assert_eq!((d.reference_speakers, d.found_speakers), (2, 2));
    }

    #[test]
    fn splits_missed_false_alarm_and_confusion() {
        let r = [turn(0.0, 10.0, "A"), turn(10.0, 20.0, "B")];
        // 0 to 8 right, 8 to 10 missed; 10 to 15 confused with A; 15 to 20 right; 20 to 22 extra.
        let f = [
            turn(0.0, 8.0, "x"),
            turn(10.0, 15.0, "x"),
            turn(15.0, 22.0, "y"),
        ];
        let d = diarization_errors(&r, &f, 0.0);
        assert!((d.scored - 20.0).abs() < 0.05);
        assert!((d.missed - 2.0).abs() < 0.05, "{d:?}");
        assert!((d.confusion - 5.0).abs() < 0.05, "{d:?}");
        assert!((d.false_alarm - 2.0).abs() < 0.05, "{d:?}");
        assert!((d.der() - 0.45).abs() < 0.01);
    }

    #[test]
    fn collar_skips_frames_around_reference_boundaries() {
        let r = [turn(0.0, 5.0, "A"), turn(5.0, 10.0, "B")];
        let f = [turn(0.0, 5.2, "a"), turn(5.2, 10.0, "b")];
        assert!(diarization_errors(&r, &f, 0.0).der() > 0.0);
        assert_eq!(diarization_errors(&r, &f, 0.25).der(), 0.0);
    }
}
