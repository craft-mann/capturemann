//! Sound entries (M2i): stretches the transcript does not write as words because it cannot vouch
//! for any (`[unclear]`: speech evidence but no reliable words after a second reading; `[noise]`:
//! energy and no speech evidence). They live beside the words in `Transcript::sounds`, so
//! everything built from `words` (captions, reports, translation, glossary suggestions, library
//! text search) never sees them; the exports that want the bracketed text use
//! [`captions_with_sounds`].

use cm_core::edit::Span;

use crate::caption::{build_captions, CaptionLine};
use crate::transcribe::{Sound, SoundKind, Word};

/// `sounds` with every one that overlaps speech evidence (`evidence`: spans in session time where
/// the speaker engine, or another source other than raw level, says someone speaks) made
/// [`SoundKind::Unclear`]; a span with speech evidence is never left as [`SoundKind::Noise`].
/// The others keep their kind.
pub fn classify(sounds: &[Sound], evidence: &[Span]) -> Vec<Sound> {
    sounds
        .iter()
        .map(|s| {
            let heard = evidence.iter().any(|e| e.from < s.end && e.to > s.start);
            Sound {
                kind: if heard { SoundKind::Unclear } else { s.kind },
                ..s.clone()
            }
        })
        .collect()
}

/// Caption lines of `words` plus one line per sound with its bracketed label, in time order. For
/// the TXT, SRT and VTT exports; [`build_captions`] alone leaves the sounds out.
pub fn captions_with_sounds(words: &[Word], sounds: &[Sound]) -> Vec<CaptionLine> {
    let mut lines = build_captions(words);
    lines.extend(sounds.iter().map(|s| CaptionLine {
        start: s.start,
        end: s.end.max(s.start),
        text: s.kind.label().to_owned(),
    }));
    lines.sort_by_key(|l| l.start);
    lines
}

/// `lines` with a bracketed line added for each sound that no existing line already covers (at
/// least half of its span): for the export of lines that were edited, which have no sound entries
/// of their own. Sorted by start.
pub fn add_sound_lines(lines: &[CaptionLine], sounds: &[Sound]) -> Vec<CaptionLine> {
    let mut out = lines.to_vec();
    for s in sounds {
        let span = (s.end - s.start).ticks().max(1);
        let covered: i64 = lines
            .iter()
            .map(|l| (l.end.min(s.end) - l.start.max(s.start)).ticks().max(0))
            .sum();
        if covered * 2 < span {
            out.push(CaptionLine {
                start: s.start,
                end: s.end.max(s.start),
                text: s.kind.label().to_owned(),
            });
        }
    }
    out.sort_by_key(|l| l.start);
    out
}

/// The text of `words` for anything that is not an export (search, reports, translation): the
/// words only.
pub fn words_text(words: &[Word]) -> String {
    words
        .iter()
        .map(|w| w.text.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcribe::{media_secs, Transcript};

    fn sound(kind: SoundKind, a: f64, b: f64) -> Sound {
        Sound {
            kind,
            start: media_secs(a),
            end: media_secs(b),
        }
    }

    fn word(text: &str, at: f64) -> Word {
        Word {
            text: text.into(),
            start: media_secs(at),
            end: media_secs(at + 0.4),
            confidence: 0.9,
        }
    }

    #[test]
    fn a_span_with_speech_evidence_never_stays_noise() {
        let sounds = [
            sound(SoundKind::Noise, 10.0, 20.0),
            sound(SoundKind::Noise, 30.0, 40.0),
            sound(SoundKind::Unclear, 50.0, 60.0),
        ];
        let evidence = [Span::new(media_secs(12.0), media_secs(14.0))];
        let out = classify(&sounds, &evidence);
        assert_eq!(out[0].kind, SoundKind::Unclear);
        assert_eq!(out[1].kind, SoundKind::Noise);
        assert_eq!(out[2].kind, SoundKind::Unclear);
        // Nothing with evidence over it is noise, whatever the input kinds.
        assert!(
            classify(&sounds, &[Span::new(media_secs(0.0), media_secs(100.0))])
                .iter()
                .all(|s| s.kind == SoundKind::Unclear)
        );
    }

    #[test]
    fn words_only_consumers_never_see_sounds_and_exports_write_the_brackets() {
        let words = [word("hello", 1.0), word("again", 21.0)];
        let sounds = [
            sound(SoundKind::Unclear, 5.0, 12.0),
            sound(SoundKind::Noise, 14.0, 18.0),
        ];
        assert_eq!(words_text(&words), "hello again");
        assert!(build_captions(&words).iter().all(|l| !l.text.contains('[')));
        let lines = captions_with_sounds(&words, &sounds);
        let texts: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(texts, ["hello", "[unclear]", "[noise]", "again"]);
    }

    #[test]
    fn edited_lines_get_sound_lines_only_where_they_say_nothing() {
        let lines = [CaptionLine {
            start: media_secs(0.0),
            end: media_secs(6.0),
            text: "hello there".into(),
        }];
        let sounds = [
            sound(SoundKind::Noise, 1.0, 5.0),
            sound(SoundKind::Unclear, 10.0, 14.0),
        ];
        let out = add_sound_lines(&lines, &sounds);
        let texts: Vec<&str> = out.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(texts, ["hello there", "[unclear]"]);
    }

    #[test]
    fn an_old_transcript_without_sounds_loads_and_empty_sounds_are_not_written() {
        let old = r#"{"version":1,"language":"en","model":"m","words":[],"note":null}"#;
        let t: Transcript = serde_json::from_str(old).unwrap();
        assert!(t.sounds.is_empty());
        assert!(!serde_json::to_string(&t).unwrap().contains("sounds"));
        let mut t = t;
        t.sounds.push(sound(SoundKind::Noise, 1.0, 2.0));
        let json = serde_json::to_string(&t).unwrap();
        assert!(json.contains(r#""kind":"noise""#), "{json}");
        let back: Transcript = serde_json::from_str(&json).unwrap();
        assert_eq!(back.sounds, t.sounds);
    }
}
