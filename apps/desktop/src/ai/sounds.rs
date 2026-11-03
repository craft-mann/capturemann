//! Sound entries in the editors (M2i): the `[unclear]` and `[noise]` stretches of a transcript
//! (`Transcript.sounds`), shown dimmed and bracketed between the words, with the speaker and the
//! time like a normal line. They are not words: nothing here is editable as text, the
//! highlighting of the playing word and the find bar never see them, and the TXT, SRT and VTT
//! exports already write them (`cm_ai::sounds::add_sound_lines`, in the daemon).
//!
//! The rule for which sounds show is the exports' rule: a sound that a caption line already
//! covers (at least half of its span, for example after the user typed over it) is not shown
//! again, and one the user removed in this session is hidden. Pure, so unit tested.

use std::collections::HashSet;

use cm_ai::caption::CaptionLine;
use cm_ai::speakers::Segment;
use cm_ai::transcribe::{Sound, SoundKind};
use cm_core::MediaTime;
use cm_i18n::tr_ctx;

/// A sound that is on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SoundRow {
    /// Its index in `Transcript.sounds` (what Remove names).
    pub index: usize,
    pub kind: SoundKind,
    pub start: MediaTime,
    pub end: MediaTime,
    /// The speaker heard over it, when speakers are known.
    pub speaker: Option<u32>,
}

/// The bracketed text of a sound in the UI language.
pub fn label(kind: SoundKind) -> String {
    match kind {
        SoundKind::Unclear => tr_ctx!("sound entry", "[unclear]"),
        SoundKind::Noise => tr_ctx!("sound entry", "[noise]"),
    }
}

/// The sounds to show: not covered by a line, not removed. `segments` are the speaker segments
/// (empty without speaker labels).
pub fn visible(
    sounds: &[Sound],
    lines: &[CaptionLine],
    hidden: &HashSet<usize>,
    segments: &[Segment],
) -> Vec<SoundRow> {
    sounds
        .iter()
        .enumerate()
        .filter(|(i, _)| !hidden.contains(i))
        .filter(|(_, s)| {
            let span = (s.end - s.start).ticks().max(1);
            let covered: i64 = lines
                .iter()
                .map(|l| (l.end.min(s.end) - l.start.max(s.start)).ticks().max(0))
                .sum();
            covered * 2 < span
        })
        .map(|(i, s)| {
            let probe = CaptionLine {
                start: s.start,
                end: s.end.max(s.start),
                text: String::new(),
            };
            SoundRow {
                index: i,
                kind: s.kind,
                start: s.start,
                end: s.end,
                speaker: crate::recedit::voice::line_speaker_by_segments(&probe, segments).0,
            }
        })
        .collect()
}

/// For each sound, the index of the first caption line that starts after it (the sound is drawn
/// before that line; `lines.len()` puts it at the end). Both lists are sorted by time.
pub fn before_lines(rows: &[SoundRow], lines: &[CaptionLine]) -> Vec<usize> {
    rows.iter()
        .map(|r| lines.partition_point(|l| l.start <= r.start))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: f64) -> MediaTime {
        MediaTime::from_secs_f64(s).unwrap()
    }

    fn line(a: f64, b: f64) -> CaptionLine {
        CaptionLine {
            start: t(a),
            end: t(b),
            text: "x".into(),
        }
    }

    fn sound(kind: SoundKind, a: f64, b: f64) -> Sound {
        Sound {
            kind,
            start: t(a),
            end: t(b),
        }
    }

    #[test]
    fn labels_are_bracketed() {
        assert_eq!(label(SoundKind::Unclear), "[unclear]");
        assert_eq!(label(SoundKind::Noise), "[noise]");
    }

    #[test]
    fn covered_and_removed_sounds_do_not_show() {
        let sounds = vec![
            sound(SoundKind::Noise, 1.0, 2.0),
            sound(SoundKind::Unclear, 5.0, 6.0),
            sound(SoundKind::Noise, 9.0, 10.0),
        ];
        // A line over the second one (the user wrote words there).
        let lines = vec![line(0.0, 0.5), line(4.5, 6.5)];
        let mut hidden = HashSet::new();
        let v = visible(&sounds, &lines, &hidden, &[]);
        assert_eq!(v.iter().map(|r| r.index).collect::<Vec<_>>(), vec![0, 2]);
        hidden.insert(0);
        let v = visible(&sounds, &lines, &hidden, &[]);
        assert_eq!(v.iter().map(|r| r.index).collect::<Vec<_>>(), vec![2]);
        assert_eq!(v[0].speaker, None);
    }

    #[test]
    fn a_sound_takes_the_speaker_heard_over_it() {
        let seg = vec![
            Segment {
                start: t(0.0),
                end: t(4.0),
                speaker: 0,
                energy: 0.0,
            },
            Segment {
                start: t(4.0),
                end: t(8.0),
                speaker: 1,
                energy: 0.0,
            },
        ];
        let v = visible(
            &[sound(SoundKind::Unclear, 5.0, 6.0)],
            &[],
            &HashSet::new(),
            &seg,
        );
        assert_eq!(v[0].speaker, Some(1));
    }

    #[test]
    fn a_sound_goes_between_the_lines_by_time() {
        let lines = vec![line(0.0, 2.0), line(4.0, 6.0)];
        let rows = visible(
            &[
                sound(SoundKind::Noise, 2.5, 3.5),
                sound(SoundKind::Noise, 7.0, 8.0),
                sound(SoundKind::Noise, 0.0, 0.0),
            ],
            &lines,
            &HashSet::new(),
            &[],
        );
        // The one at 0.0 starts with the first line: it follows it (a tie goes after the line).
        assert_eq!(before_lines(&rows, &lines), vec![1, 2, 1]);
    }
}
