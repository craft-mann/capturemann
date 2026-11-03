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
/// start. Today the region gets no words. (Passes: it documents the failure.)
#[test]
fn a_speech_region_in_the_middle_gets_no_words_today() {
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
    assert!((c.ratio() - 0.5).abs() < 0.1, "{c:?}");
}

#[test]
#[ignore = "planned fix M2d-1: re-read speech regions that have no words, as a piece starting at the region"]
fn the_fix_reads_uncovered_speech_again() {
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
    for (i, t) in ["hello", "everyone", "today", "we", "start", "with", "the", "plan"]
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
    assert!(find_runs(&words).is_empty(), "find_runs needs words in a row");
    let found = spread_repeats(&words, 4, 10.0, 5);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].text, "ok");
    assert_eq!(found[0].count, 7);
    assert!(found[0].mean_confidence > 0.9);
}

#[test]
fn real_sentences_do_not_count_as_spread_repeats() {
    let words: Vec<Word> = ["yes", "no", "yes", "fine", "yes", "ok", "yes", "right", "yes"]
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
#[ignore = "planned fix M2d-3: a token that decodes to U+FFFD, or a foreign script on low confidence, is re-decoded or marked doubtful, never shown as text"]
fn the_fix_never_shows_broken_tokens_as_text() {
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
