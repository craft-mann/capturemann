//! Reproductions of the three failure kinds of M2d with synthetic audio and word lists (no
//! recording of anyone's). The tests that pass pin the measures; the `#[ignore]` ones state what
//! the planned fix must achieve through the public pipeline and fail today.

use super::*;
use crate::audio::SAMPLE_RATE;
use crate::pieces::{find_runs, transcribe_pieces};
use crate::transcribe::{
    cancelled, media_secs, Pcm16k, ScriptedTranscriber, Transcriber, Transcript, Word,
    TRANSCRIPT_VERSION,
};
use cm_core::{CmError, MediaTime};

const RATE: usize = SAMPLE_RATE as usize;

fn w(text: &str, at: f64, conf: f32) -> Word {
    Word {
        text: text.to_owned(),
        start: media_secs(at),
        end: media_secs(at + 0.3),
        confidence: conf,
    }
}

/// Tone at `amp` for each `(from, to)` second span, silence elsewhere, `secs` long.
fn signal(secs: f64, spans: &[(f64, f64, f32)]) -> Vec<f32> {
    let mut x = vec![0.0f32; (secs * RATE as f64) as usize];
    for &(a, b, amp) in spans {
        for (i, v) in x
            .iter_mut()
            .enumerate()
            .take((b * RATE as f64) as usize)
            .skip((a * RATE as f64) as usize)
        {
            *v = amp * (i as f32 * 0.31).sin();
        }
    }
    x
}

// ---- (a) speech that got no words ------------------------------------------------------------

#[test]
fn coverage_is_seconds_of_speech_with_words_over_seconds_of_speech() {
    // 30 s of speech in two stretches; words only in the first one and at the start of the second.
    let speech = [(0.0, 10.0), (40.0, 60.0)];
    let words: Vec<(f64, f64)> = (0..20)
        .map(|i| (f64::from(i) * 0.5, f64::from(i) * 0.5 + 0.4))
        .chain([(40.0, 41.0)])
        .collect();
    let c = coverage(&speech, &words, 1.0, 5.0);
    assert!((c.speech_secs - 30.0).abs() < 0.2, "{c:?}");
    // First stretch fully covered (10 s), second: 40 to 42 covered (2 s).
    assert!((c.covered_secs - 12.0).abs() < 0.3, "{c:?}");
    assert!((c.ratio() - 0.4).abs() < 0.02, "{c:?}");
    assert_eq!(c.uncovered.len(), 1, "{c:?}");
    assert!((c.uncovered[0].0 - 42.0).abs() < 0.2 && (c.uncovered[0].1 - 60.0).abs() < 0.2);
    assert!((c.uncovered_secs() - 18.0).abs() < 0.4);
}

#[test]
fn no_speech_is_full_coverage_and_short_gaps_are_not_listed() {
    assert_eq!(coverage(&[], &[], 1.0, 5.0).ratio(), 1.0);
    let c = coverage(&[(0.0, 20.0)], &[(0.0, 8.0), (12.0, 20.0)], 0.5, 5.0);
    assert!(c.uncovered.is_empty(), "{c:?}");
    assert!(c.ratio() > 0.85);
}

/// A transcriber that hears speech only when it starts within a second of the audio it is given
/// (a model that skips a region in the middle of a long read but gets it when the read starts
/// there). Words: one per whole second of the burst it hears.
struct HearsAtStart;

impl Transcriber for HearsAtStart {
    fn transcribe(
        &self,
        audio: Pcm16k<'_>,
        _language: Option<&str>,
        progress: &mut dyn FnMut(f32) -> bool,
    ) -> Result<Transcript, CmError> {
        if !progress(1.0) {
            return Err(cancelled());
        }
        let frame = RATE / 10;
        let loud: Vec<bool> = audio
            .samples
            .chunks(frame)
            .map(|c| c.iter().map(|s| s * s).sum::<f32>() / c.len() as f32 > 1e-3)
            .collect();
        let mut words = Vec::new();
        let first = loud.iter().position(|l| *l);
        if let Some(first) = first.filter(|f| *f < 10) {
            let mut i = first;
            while i < loud.len() && loud[i] {
                words.push(Word {
                    text: format!("w{}", words.len()),
                    start: audio.start + media_secs(i as f64 / 10.0),
                    end: audio.start + media_secs(i as f64 / 10.0 + 0.1),
                    confidence: 0.9,
                });
                i += 10;
            }
        }
        Ok(Transcript {
            version: TRANSCRIPT_VERSION,
            language: "en".into(),
            model: "hears-at-start".into(),
            words,
            note: None,
        })
    }
}

/// The reproduction: speech at 40 s to 50 s, nothing before it, and a model that only hears the
/// start of what it is given. Before the fix the region got no words (coverage 50 %).
#[test]
fn uncovered_speech_is_read_again_as_a_piece_that_starts_there() {
    let x = signal(60.0, &[(0.0, 10.0, 0.2), (40.0, 50.0, 0.2)]);
    let (t, _) = transcribe_pieces(
        &HearsAtStart,
        Pcm16k {
            samples: &x,
            start: MediaTime::ZERO,
        },
        Some("en"),
        None,
        &mut |_| true,
    )
    .unwrap();
    let c = word_coverage(&[(0.0, 10.0), (40.0, 50.0)], &t.words, 1.0, 5.0);
    assert!(c.ratio() > 0.9, "{c:?}");
}

// ---- (b) one short word recurring after long gaps --------------------------------------------

/// Seven "ok" 14 to 39 s apart on weak signal, between real sentences.
fn spread_words() -> (Vec<Word>, Vec<(f64, f64, f32)>) {
    let mut words = Vec::new();
    let mut spans = vec![(0.0, 12.0, 0.2f32)];
    for (i, t) in [
        "hello", "everyone", "today", "we", "start", "with", "the", "plan",
    ]
    .iter()
    .enumerate()
    {
        words.push(w(t, i as f64 * 1.4, 0.9));
    }
    let mut at = 26.0;
    for gap in [14.0, 22.0, 31.0, 39.0, 17.0, 25.0, 33.0] {
        spans.push((at, at + 0.4, 0.002)); // weak signal under the word
        words.push(w("ok", at, 0.95)); // and high self-confidence
        at += gap + 0.4;
    }
    (words, spans)
}

#[test]
fn eight_in_a_row_does_not_see_seven_spread_out_repeats_but_the_measure_does() {
    let (words, _) = spread_words();
    assert!(
        find_runs(&words).is_empty(),
        "find_runs needs words in a row"
    );
    let found = spread_repeats(&words, 4, 10.0, 5);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].text, "ok");
    assert_eq!(found[0].count, 7);
    assert!(found[0].mean_confidence > 0.9);
}

#[test]
fn real_sentences_do_not_count_as_spread_repeats() {
    let words: Vec<Word> = [
        "yes", "no", "yes", "fine", "yes", "ok", "yes", "right", "yes",
    ]
    .iter()
    .enumerate()
    .map(|(i, t)| w(t, i as f64 * 15.0, 0.9))
    .collect();
    // "yes" 5 times after long gaps is a count to look at, but a loop rule that also needs weak
    // signal (the planned fix) would keep it; the measure alone reports it, and a threshold of 6
    // does not.
    assert!(spread_repeats(&words, 4, 10.0, 6).is_empty());
}

#[test]
#[ignore = "planned fix M2d-2: drop or re-decode a short word that recurs after long gaps on weak signal"]
fn the_fix_drops_spread_out_repeats_on_weak_signal() {
    let (words, spans) = spread_words();
    let x = signal(300.0, &spans);
    let mut script = ScriptedTranscriber::new(words);
    script.language = "en".into();
    let (t, _) = transcribe_pieces(
        &script,
        Pcm16k {
            samples: &x,
            start: MediaTime::ZERO,
        },
        Some("en"),
        None,
        &mut |_| true,
    )
    .unwrap();
    let oks = t.words.iter().filter(|w| w.text == "ok").count();
    assert!(oks <= 1, "{oks} ok left");
}

// ---- (c) tokens that are not text -------------------------------------------------------------

#[test]
fn scripts_are_told_apart_by_letter_not_by_word_list() {
    assert_eq!(script_of('a'), Some(Script::Latin));
    assert_eq!(script_of('ế'), Some(Script::Latin)); // Vietnamese
    assert_eq!(script_of('한'), Some(Script::Hangul));
    assert_eq!(script_of('я'), Some(Script::Cyrillic));
    assert_eq!(script_of('7'), None);
    assert!(expected_scripts("fr").is_some());
    assert!(expected_scripts("xx").is_none());
}

fn broken_words() -> Vec<Word> {
    vec![
        w("c\u{FFFD}n", 0.5, 0.03),
        w("fine", 1.0, 0.9),
        w("한", 1.5, 0.02),
        w("ok", 2.0, 0.9),
        w("naïve", 2.5, 0.8),
    ]
}

#[test]
fn health_counts_replacement_characters_and_foreign_scripts() {
    let h = health(&broken_words(), "en");
    assert_eq!(
        h,
        Health {
            words: 5,
            replacement_char: 1,
            foreign_script: 1,
            low_confidence: 2,
            broken_and_doubtful: 2,
        }
    );
    // Korean text in a Korean transcript is fine; a language with no entry makes no claim.
    assert_eq!(health(&broken_words(), "ko").foreign_script, 0);
    assert_eq!(health(&broken_words(), "xx").foreign_script, 0);
}

#[test]
fn broken_tokens_are_never_shown_as_text() {
    let x = signal(10.0, &[(0.0, 4.0, 0.2)]);
    let (t, _) = transcribe_pieces(
        &ScriptedTranscriber::new(broken_words()),
        Pcm16k {
            samples: &x,
            start: MediaTime::ZERO,
        },
        Some("en"),
        None,
        &mut |_| true,
    )
    .unwrap();
    let h = health(&t.words, "en");
    assert_eq!(h.replacement_char, 0, "{:?}", t.words);
    assert_eq!(h.broken_and_doubtful, 0, "{:?}", t.words);
}

/// A transcriber whose long reads come out broken at the start and whose short reads (a window
/// read on its own) are clean: the shape of weak-signal garbage at the start of a file.
struct GarbageThenClean;

impl Transcriber for GarbageThenClean {
    fn transcribe(
        &self,
        audio: Pcm16k<'_>,
        _language: Option<&str>,
        _progress: &mut dyn FnMut(f32) -> bool,
    ) -> Result<Transcript, CmError> {
        let long = audio.samples.len() > 20 * RATE;
        let words = if long {
            vec![
                Word {
                    text: "\u{FFFD}\u{FFFD}".into(),
                    start: audio.start + media_secs(2.0),
                    end: audio.start + media_secs(2.3),
                    confidence: 0.02,
                },
                Word {
                    text: "한".into(),
                    start: audio.start + media_secs(2.5),
                    end: audio.start + media_secs(2.8),
                    confidence: 0.03,
                },
                Word {
                    text: "ok".into(),
                    start: audio.start + media_secs(10.0),
                    end: audio.start + media_secs(10.3),
                    confidence: 0.9,
                },
            ]
        } else {
            vec![Word {
                text: "hello".into(),
                start: audio.start + media_secs(1.5),
                end: audio.start + media_secs(2.0),
                confidence: 0.8,
            }]
        };
        Ok(Transcript {
            version: TRANSCRIPT_VERSION,
            language: "en".into(),
            model: "garbage-then-clean".into(),
            words,
            note: None,
        })
    }
}

/// M2d-3: a stretch of broken words is read again on its own; a clean reading replaces it.
#[test]
fn a_broken_stretch_is_read_again_and_a_clean_reading_replaces_it() {
    let x = signal(30.0, &[(0.0, 30.0, 0.2)]);
    let (t, report) = transcribe_pieces(
        &GarbageThenClean,
        Pcm16k {
            samples: &x,
            start: MediaTime::ZERO,
        },
        Some("en"),
        None,
        &mut |_| true,
    )
    .unwrap();
    let texts: Vec<&str> = t.words.iter().map(|w| w.text.as_str()).collect();
    assert_eq!(texts, ["hello", "ok"], "{:?}", t.words);
    assert_eq!((report.broken_reread, report.broken_reread_kept), (1, 1));
}

/// An old transcript with broken characters still shows none of them in its captions.
#[test]
fn captions_never_show_stray_characters() {
    let words = vec![
        w("\u{FFFD}\u{FFFD}", 0.0, 0.5),
        w("c\u{FFFD}n", 0.6, 0.5),
        w("tho\u{200B}ught", 1.0, 0.9),
    ];
    let lines = crate::caption::build_captions(&words);
    let text: String = lines
        .iter()
        .map(|l| l.text.clone())
        .collect::<Vec<_>>()
        .join(" ");
    assert_eq!(text, "cn thought");
}

/// A long read that writes one short word for twelve seconds of speech; a read of the region on its
/// own writes a word a second (the maintainer's "Cảmơn" over a full quiet sentence, in shape).
struct OneWordForALongRegion;

impl Transcriber for OneWordForALongRegion {
    fn transcribe(
        &self,
        audio: Pcm16k<'_>,
        _language: Option<&str>,
        _progress: &mut dyn FnMut(f32) -> bool,
    ) -> Result<Transcript, CmError> {
        let secs = audio.samples.len() as f64 / RATE as f64;
        let w = |text: String, at: f64, end: f64| Word {
            text,
            start: audio.start + media_secs(at),
            end: audio.start + media_secs(end),
            confidence: 0.94,
        };
        let words = if secs > 30.0 {
            // The first region is read properly, the second (40 to 52 s) gets one 0.6 s word.
            let mut v: Vec<Word> = (0..9)
                .map(|i| w(format!("a{i}"), f64::from(i), f64::from(i) + 0.5))
                .collect();
            v.push(w("thanks".into(), 46.0, 46.6));
            v
        } else {
            (0..secs as i32 - 1)
                .map(|i| w(format!("b{i}"), f64::from(i), f64::from(i) + 0.5))
                .collect()
        };
        Ok(Transcript {
            version: TRANSCRIPT_VERSION,
            language: "en".into(),
            model: "one-word".into(),
            words,
            note: None,
        })
    }
}

/// M2d-1: a short word standing for seconds of speech is not accepted as the reading of the
/// region: the region is read again on its own and its words replace it.
#[test]
fn a_short_word_over_a_long_speech_region_is_read_again() {
    let x = signal(60.0, &[(0.0, 10.0, 0.2), (40.0, 52.0, 0.2)]);
    let (t, report) = transcribe_pieces(
        &OneWordForALongRegion,
        Pcm16k {
            samples: &x,
            start: MediaTime::ZERO,
        },
        Some("en"),
        None,
        &mut |_| true,
    )
    .unwrap();
    assert!(!t.words.iter().any(|w| w.text == "thanks"), "{:?}", t.words);
    let in_region = t
        .words
        .iter()
        .filter(|w| w.start >= media_secs(39.0))
        .count();
    assert!(in_region >= 8, "{in_region} words in the 12 s region");
    assert_eq!((report.gaps_reread, report.gaps_filled), (1, 1));
}
